//! 界面⇄编排层的桥(design-s5-hexpanel.md §4.1)。一条独立的操作系统线
//! 程持有整个编排层(`App` + `RunManager`,各自独立开一条存储连接,同
//! `bw-app` 既有的「两个各自独立的存储连接持有者」架构决定——见
//! `bw_app::App` 文档);界面与它之间只有三条通道:
//!
//! - `mpsc`(命令进)——界面事件处理器发完就走,不等结果;
//! - `watch`(数据出)——每次处理完命令重建一份 [`Vm`],界面订阅;
//! - `broadcast`(提示出)——转瞬即逝的 [`UiNote`](错误提示等),不进
//!   `Vm` 持久状态。
//!
//! **`ShellCommand` 不是 `bw_app::Command` 的替代品或超集**——分两类:
//! ①纯本地导航([`ShellCommand::SelectProject`]/[`ShellCommand::SetPanel`]/
//! [`ShellCommand::Refresh`]),②转发给编排层自己的命令总线
//! ([`ShellCommand::Dispatch`],走 [`bw_app::App::dispatch`])。「哪个项
//! 目打开着、哪一屏在显示」根本不是编排层要记的事实——`App::hex_view`/
//! `App::attention_view` 的签名已经证明这一点:它们直接接受 `project_id`/
//! `project: Option<ProjectId>` 作为参数,不是从编排层某个持久状态里
//! 读。五-2 的 `bw-app/src/lib.rs` 顶部文档也早已明确「`Boot`/
//! `OpenProject`/`SetPanel`(壳的导航状态,壳本片不建)」不在 `bw_app::
//! Command` 里——这些是壳自己的界面导航状态,本片是它们的第一个真实归
//! 属。
//!
//! **壳不持有调度时钟**(§4.1「砍掉的部分」/§10.1 第 4 条,
//! `scripts/guard-shell-no-clock.sh` 机器守这条线):[`spawn`] 起的主循
//! 环里没有 `interval`/`sleep`,新鲜数据只能靠人点(`ShellCommand::
//! Refresh`)或人写命令之后的自动重建——**没有任何东西会自己刷新**,这
//! 是设计稿明确要的行为,不是遗漏。
//!
//! **主循环只有一条 `select!`/`recv` 臂,不是设计稿工程对照表原文写的
//! 「两条」**——如实标注这处偏差:对照表那句话描述的是"收命令"与"收编
//! 排层发回来的事件"两条通道,但"编排层发回来的事件"在**这一片**的真
//! 实实现里就是 `App::dispatch` 调用的同步返回值(`Result<Event,
//! AppError>`),不是一条独立的、需要单独 `select!` 臂去订阅的异步事件
//! 流——`bw-app`/`RunManager` 今天**没有**提供这样一条可订阅的运行进度
//! /终端字节广播通道(那条通道属于嵌入终端,plan/23 §7 主控裁决 7 钉死
//! 落在切片 5.5,不在本片)。为了凑「两条」而开一个不消费任何真实数据
//! 源的 `select!` 分支,就是本仓库最忌的那种"形式对了、内容是空的"东
//! 西;因此这里如实一条 `.recv().await`,把"收命令"与"处理命令返回的事
//! 件"揉进同一条分支——`tokio::select!` 宏本身也没有用到,因为只有一
//! 个要等待的源。等嵌入终端接进来(切片 5.5)、真的有第二条事件源时,
//! 再加第二条臂。

use std::sync::mpsc as std_mpsc;

use bw_app::run::{RunManager, RunManagerConfig};
use bw_app::view::hex::LoopInputs;
use bw_app::view::{AttentionView, HexView};
use bw_app::{App, Command};
use bw_connector::ConnectorRegistry;
use bw_core::ProjectId;
use bw_store::{IssueStore, ProjectRow, RunStore};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, watch};

/// 两屏,不多不少(design §1.1「屏的形状:两屏,不是七个面板」)。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Panel {
    #[default]
    Hex,
    Attention,
}

/// 界面发的命令——见本文件顶部模块文档「`ShellCommand` 不是…」一节。
pub enum ShellCommand {
    /// 顶栏项目下拉切换(§1.1「顶栏一个项目下拉,不做项目墙」)。
    SelectProject(ProjectId),
    SetPanel(Panel),
    /// 手动刷新——壳不持有时钟(§4.1),新鲜数据没有别的来源。
    Refresh,
    /// 转发进编排层自己的命令总线。
    Dispatch(Command),
}

/// 壳启动时"这次深链解析到底通没通"的结果——[`spawn`] 用一条
/// `std::sync::mpsc`(不是 tokio 的,main() 在调用 `dioxus::launch` 之前
/// 还是纯同步代码)把它带回 `main()`,`main()` 据此决定是退出码非零
/// (§4.4「一处新增:project NOT FOUND 时退出码非零」)还是继续打开窗
/// 口——这个决定必须在**任何窗口被创建之前**做出,而编排层线程是
/// fire-and-forget 的,没法把"要不要继续启动 GUI"这个判断直接带回
/// `main()` 的返回值/退出码里,所以在这里另开一条一次性的通知通道。
pub enum DeepLinkOutcome {
    /// `BW_OPEN` 没对上任何项目——`main()` 必须在调用 `dioxus::launch`
    /// 之前退出,非零码,不打开任何窗口。
    NotFound,
    /// 首次启动流程跑完(不管有没有 `BW_OPEN`,或者 `BW_OPEN` 已经解析
    /// 成功)——`main()` 可以继续。
    Ready,
}

/// 一次调用的产物,用完即弃——同 `bw_app::view::HexView`/`AttentionView`
/// 自己的文档「不是任何持久缓存」。**这里也没有任何推导**:所有字段要
/// 么是编排层已经算好的视图(`hex`/`attention`),要么是从这些视图里筛
/// 出来的既成事实子集(`open_runs`),要么是纯本地导航状态
/// (`panel`/`selected`)——壳里禁推导这条边界,见模块文档。
// `PartialEq` 是 dioxus `#[component]` props 变更检测要的——`HexScreen`/
// `AttentionScreen` 把整份 `Vm` 当 prop 传下去(§4.3「壳里禁推导」:屏只
// 读现成字段,不重新查/算,`Vm` 本身就是它们唯一的输入)。
#[derive(Clone, Default, PartialEq)]
pub struct Vm {
    pub ready: bool,
    /// 编排层某一步真实失败了(存储打不开、组装用例报错)——如实说,不
    /// 是伪造一份空数据继续渲染。
    pub fatal: Option<String>,
    pub panel: Panel,
    /// 顶栏项目下拉的真实数据源(`bw_store::IssueStore::list_projects`,
    /// 切片五E 新加的读法——壳是它的第一个消费者)。
    pub projects: Vec<ProjectRow>,
    pub selected: Option<ProjectId>,
    pub hex: Option<HexView>,
    pub attention: Option<AttentionView>,
    /// 「进行中」的运行(`ended_at IS NULL`)——逃生舱卡片的数据源
    /// (design §5.2)。从 `hex.evidence.runs` 筛出来,不是另开一条查
    /// 询:§4.3「查库在调用它的用例里做」,壳这一层只做展示层筛选,不
    /// 新增查询。
    pub open_runs: Vec<bw_store::RunRow>,
}

/// 转瞬即逝的提示——运行/命令进度、错误提示这类不进 [`Vm`] 持久状态的
/// 东西(§4.1「提示出」通道)。本片壳没有 `RunIssue`/嵌入终端,没有真
/// 实的运行进度流可报,因此这个枚举目前只有两种,如实反映现状,不预
/// 先造几个没有生产者的变体。
#[derive(Clone, Debug)]
pub enum UiNote {
    Error(String),
    /// 一条 `Command` 真的成功了——携带它的 `Event`(`Debug` 格式化文
    /// 本),给人一个"刚才那次操作确实发生了"的确认,不是必需但廉价诚
    /// 实。
    Ok(String),
}

#[derive(Clone)]
pub struct Kernel {
    tx: mpsc::UnboundedSender<ShellCommand>,
    vm_rx: watch::Receiver<Vm>,
    notes: broadcast::Sender<UiNote>,
}

impl Kernel {
    pub fn send(&self, c: ShellCommand) {
        let _ = self.tx.send(c);
    }
    pub fn vm(&self) -> watch::Receiver<Vm> {
        self.vm_rx.clone()
    }
    pub fn notes(&self) -> broadcast::Receiver<UiNote> {
        self.notes.subscribe()
    }
}

/// `BW_DB` 覆盖数据库路径;不给就落一个与仓根旧壳(`BuildersWorkbench`
/// 目录)不同名的默认目录——两套壳各自的默认库不会因为同名目录互相踩
/// (`BuildersWorkbenchNext`,§4.4「`BW_DB`…不变」讲的是变量名和语义,
/// 不是要求默认路径也撞旧壳)。
fn db_path() -> String {
    if let Ok(p) = std::env::var("BW_DB") {
        return p;
    }
    let base = if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| format!("{h}/Library/Application Support/BuildersWorkbenchNext"))
            .ok()
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(|a| format!("{a}\\BuildersWorkbenchNext"))
            .ok()
    } else {
        std::env::var("HOME")
            .map(|h| format!("{h}/.local/share/builders-workbench-next"))
            .ok()
    };
    match base {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            format!("{dir}/workbench.db")
        }
        None => "workbench-next.db".into(),
    }
}

/// 壳当前的导航状态(纯本地,见模块文档「`ShellCommand` 不是…」)。
struct Nav {
    project: Option<ProjectId>,
    panel: Panel,
}

/// 深链解析(design §4.4)——`BW_OPEN`/`BW_PANEL`。**落地方式尽量照搬旧
/// 壳的做法**:用真实查询(`list_projects` + 按名字找)落位,不伪造一份
/// 渲染数据;`BW_PANEL` 只剩两个值(§4.4「值集缩小」)。
///
/// 一处如实的偏差:旧壳的解析发生在编排层线程内部、且从不因为解析失败
/// 而阻止 GUI 启动(fire-and-forget)。本片新增了「NOT FOUND 时退出码
/// 非零、且不能已经打开了窗口」这条要求(§4.4)——这条要求没法在"编排层
/// 线程独占状态、`main()` 只管调 `dioxus::launch`"这套架构里被优雅满足
/// (那条线程是 detached 的,没有把"继续还是退出"带回 `main()` 返回值
/// 的路)。因此解析本身仍然发生在编排层线程里(用的是真实的
/// `list_projects` 查询,不是另开一条平行逻辑),但**是否放行 GUI 启
/// 动**这个判断经 [`DeepLinkOutcome`] 这条一次性通道明确带回 `main()`,
/// `main()` 在调用 `dioxus::launch` 之前同步等它——这样"解析用真实数
/// 据"与"NOT FOUND 时不开窗口"两条约束都满足,代价写在这里,不是悄悄
/// 绕过。
fn resolve_deep_link(projects: &[ProjectRow]) -> (Nav, bool) {
    let mut nav = Nav {
        project: None,
        panel: Panel::Hex,
    };
    if let Ok(pl) = std::env::var("BW_PANEL") {
        nav.panel = match pl.as_str() {
            "attention" => Panel::Attention,
            _ => Panel::Hex,
        };
    }
    if let Ok(name) = std::env::var("BW_OPEN") {
        match projects.iter().find(|p| p.name == name) {
            Some(p) => {
                nav.project = Some(p.id);
            }
            None => {
                eprintln!("[BW_OPEN] {name:?} NOT FOUND");
                return (nav, true);
            }
        }
    } else if let Some(first) = projects.first() {
        // 没有深链——真实数据里挑第一个(按 `created_at` 升序,确定性
        // 选择,不是伪造),不显示空白项目墙(§8「不做项目墙」)。
        nav.project = Some(first.id);
    }
    (nav, false)
}

/// 从一次真实 `RunManager::snapshot` 调用 + 两条真实 store 查询拼出
/// `LoopInputs`——与 `hex_readback.rs` `current_loop_inputs` 同一手法
/// (那份文档「评审 Important-2 修复」一节已经把理由讲清楚:四个数直接
/// 来自 `RunSnapshot`,`LoopInputs` 是它的真实消费者,不是另起一份平行
/// 查询)。
async fn current_loop_inputs(
    app: &App,
    manager: &RunManager,
    project_id: ProjectId,
) -> Result<LoopInputs, String> {
    let project = Some(project_id);
    let snap = manager
        .snapshot(project)
        .await
        .map_err(|e| format!("RunManager::snapshot 失败:{e}"))?;
    let stalled = app
        .store
        .list_stalled_runs(project)
        .await
        .map_err(|e| e.to_string())?;
    let has_any_run = !app
        .store
        .list_runs(project)
        .await
        .map_err(|e| e.to_string())?
        .is_empty();
    Ok(LoopInputs {
        running: snap.running,
        in_review: snap.in_review,
        recent_failed: snap.recent_failed,
        unsettled: snap.unsettled,
        exceptions: stalled,
        has_any_run,
    })
}

/// 组装一份 [`Vm`]——查库(`App::hex_view`/`App::attention_view`/
/// `RunManager::snapshot`,这三个都是各自的真正实现,壳这里不重复发
/// 明),拼装,不做任何推导(§4.3「查库在调用它的用例里做」)。
async fn build_vm(app: &App, manager: &RunManager, nav: &Nav) -> Vm {
    let now = OffsetDateTime::now_utc();
    let projects = app.store.list_projects().await.unwrap_or_default();
    let mut vm = Vm {
        ready: true,
        fatal: None,
        panel: nav.panel,
        selected: nav.project,
        projects,
        hex: None,
        attention: None,
        open_runs: Vec::new(),
    };

    let Some(pid) = nav.project else {
        return vm;
    };

    let loop_inputs = match current_loop_inputs(app, manager, pid).await {
        Ok(li) => li,
        Err(e) => {
            vm.fatal = Some(format!("运行聚合快照读取失败:{e}"));
            return vm;
        }
    };
    // 交付证据第③栏「现采不落库」(§2.6):`root_path` 空 = 工作区未配
    // 置,`workspace_evidence` 如实是 `None`,不去猜一个假路径来采。
    let workspace_evidence = match app.store.get_project(pid).await {
        Ok(Some(p)) if !p.root_path.trim().is_empty() => {
            bw_workspace::evidence::collect(&p.root_path).await.ok()
        }
        _ => None,
    };
    let hex = match app
        .hex_view(pid, loop_inputs, workspace_evidence, now)
        .await
    {
        Ok(h) => h,
        Err(e) => {
            vm.fatal = Some(format!("六段视图组装失败:{e}"));
            return vm;
        }
    };
    let attention = match app.attention_view(Some(pid), now).await {
        Ok(a) => a,
        Err(e) => {
            vm.fatal = Some(format!("待人处理投影组装失败:{e}"));
            return vm;
        }
    };
    vm.open_runs = hex
        .evidence
        .runs
        .iter()
        .filter(|r| r.ended_at.is_none())
        .cloned()
        .collect();
    vm.hex = Some(hex);
    vm.attention = Some(attention);
    vm
}

/// 深链渲染证明(design §4.4)——`[BW_OPEN] "项目名" -> panel=Hex 指标=N
/// 观测=N 运行=N 待人处理=N`,数字从真实 `Vm` 读,不硬编。只在真的解析
/// 了 `BW_OPEN` 时打(没有深链的正常交互式启动不刷屏)。
fn print_bw_open_line(project_name: &str, vm: &Vm) {
    let (metrics, observations, runs) = match &vm.hex {
        Some(h) => (
            h.metrics.cards.len(),
            h.evidence.observations.len(),
            h.evidence.runs.len(),
        ),
        None => (0, 0, 0),
    };
    let attention = vm.attention.as_ref().map(|a| a.total_count()).unwrap_or(0);
    eprintln!(
        "[BW_OPEN] {project_name:?} -> panel={:?} 指标={metrics} 观测={observations} 运行={runs} 待人处理={attention}",
        vm.panel,
    );
    // 逃生舱渲染证明(裁决 7「本片必须真能用,评审要实测」)——点击这条
    // 路在这套桌面框架上打不通(仓库踩过两轮的结论),深链 stderr 才是
    // 可靠证明;这里把每张「进行中」运行卡真实算出来的续接命令原样打
    // 出来,供人复核语法(§7.2「截图是加分项不是必需项」同一条纪律)。
    for run in &vm.open_runs {
        let eh = app_desktop::escape_hatch::build(run);
        match eh.resume_command {
            Some(cmd) => eprintln!(
                "[BW_ESCAPE_HATCH] 运行={} 上游会话={:?} 续接命令: {cmd}",
                eh.run_label, eh.upstream_session
            ),
            None => eprintln!(
                "[BW_ESCAPE_HATCH] 运行={} 这家不支持指派会话号,续接方式未知",
                eh.run_label
            ),
        }
    }
}

/// 起壳线程。**返回时不保证第一份 `Vm` 已经就绪**——`outcome_tx` 那条
/// 一次性通知才是"首次启动流程真的跑完了"的信号,`main()` 用它决定何
/// 时(以及要不要)调用 `dioxus::launch`,见 [`DeepLinkOutcome`] 文档。
pub fn spawn(outcome_tx: std_mpsc::Sender<DeepLinkOutcome>) -> Kernel {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<ShellCommand>();
    let (vm_tx, vm_rx) = watch::channel(Vm::default());
    let (note_tx, _keep) = broadcast::channel::<UiNote>(64);
    // 留一份给要返回的 `Kernel`——线程闭包移进去的是克隆(广播 sender 本
    // 来就是给多个持有者共用的),不是唯一那份。
    let note_tx_thread = note_tx.clone();

    std::thread::Builder::new()
        .name("bw-next-kernel".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("kernel runtime");
            rt.block_on(async move {
                let real_db_path = db_path();
                let app = match App::open(&real_db_path).await {
                    Ok(a) => a,
                    Err(e) => {
                        let _ = vm_tx.send(Vm {
                            ready: true,
                            fatal: Some(format!("本地数据库打开失败:{e}")),
                            ..Vm::default()
                        });
                        let _ = outcome_tx.send(DeepLinkOutcome::Ready);
                        return;
                    }
                };
                // 空注册表:本片壳不发起 RunIssue/CollectMetrics(见
                // Cargo.toml 同一条注释),`RunManager::snapshot` 不碰
                // 注册表,空的不影响它。
                let registry = Arc::new(ConnectorRegistry::default());
                let manager = match RunManager::open(
                    std::path::Path::new(&real_db_path),
                    registry,
                    RunManagerConfig::default(),
                )
                .await
                {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = vm_tx.send(Vm {
                            ready: true,
                            fatal: Some(format!("运行管理器打开失败:{e}")),
                            ..Vm::default()
                        });
                        let _ = outcome_tx.send(DeepLinkOutcome::Ready);
                        return;
                    }
                };
                // 重启后收拾遗留(design-s4-runmanager.md §3.1)——桌面壳
                // 是 `RunManager` 第一个"真的会被人重复启动"的调用方
                // (`run_races`/`hex_readback` 两个指挥器都是单次跑完就
                // 扔的进程,从不需要处理"上一次没关干净"这种情况);清
                // 不清由调用方显式决定,壳启动就是那个显式决定的时刻。
                match manager.reap_on_restart().await {
                    Ok(report) if !report.reaped.is_empty() => {
                        eprintln!("[BW_REAP] 重启收拾遗留运行 {} 条", report.reaped.len());
                    }
                    Ok(_) => {}
                    Err(e) => eprintln!("[BW_REAP] 失败:{e}"),
                }

                let projects = app.store.list_projects().await.unwrap_or_default();
                let (mut nav, not_found) = resolve_deep_link(&projects);
                if not_found {
                    let _ = outcome_tx.send(DeepLinkOutcome::NotFound);
                    return;
                }

                let vm = build_vm(&app, &manager, &nav).await;
                if std::env::var("BW_OPEN").is_ok() {
                    if let Some(pid) = nav.project {
                        let name = vm
                            .projects
                            .iter()
                            .find(|p| p.id == pid)
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        print_bw_open_line(&name, &vm);
                    }
                }
                let _ = vm_tx.send(vm);
                let _ = outcome_tx.send(DeepLinkOutcome::Ready);

                // 主循环——只有一条要等的源(见模块文档「主循环只有一
                // 条…」),没有 `tokio::select!`,没有任何定时器构造。
                while let Some(cmd) = cmd_rx.recv().await {
                    match cmd {
                        ShellCommand::SelectProject(pid) => nav.project = Some(pid),
                        ShellCommand::SetPanel(p) => nav.panel = p,
                        ShellCommand::Refresh => {}
                        ShellCommand::Dispatch(c) => match app.dispatch(c).await {
                            Ok(ev) => {
                                let _ = note_tx_thread.send(UiNote::Ok(format!("{ev:?}")));
                            }
                            Err(e) => {
                                let _ = note_tx_thread.send(UiNote::Error(e.to_string()));
                            }
                        },
                    }
                    let next = build_vm(&app, &manager, &nav).await;
                    let _ = vm_tx.send(next);
                }
            });
        })
        .expect("spawn kernel thread");

    Kernel {
        tx: cmd_tx,
        vm_rx,
        notes: note_tx,
    }
}
