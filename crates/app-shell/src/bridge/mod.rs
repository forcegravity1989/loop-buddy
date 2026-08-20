//! 内核桥:一条独立 tokio 线程拿着 `bw_v4::App`,壳这边只发命令、收 ViewModel。
//!
//! 壳里没有一行业务判断。界面点一下 → 发一条 `Command` 过桥 → 内核执行 → 重建
//! 一份 [`Vm`] → 经 watch 通道推回来 → 界面重画。**壳自己绝不伪造成功事件**:
//! 命令失败就把失败的原话放进 `Vm::note`,不假装做成了。

mod nav;
mod vm_build;
mod vm_derive;
mod vm_kb;
mod vm_panels;

pub use nav::{GuideNav, Panel, PanelNav, TopView};

use crate::vm::Vm;
use bw_v4::app::App;
use bw_v4::command::Command;
use bw_v4::model::ProjectId;
use bw_v4::V4Store;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, watch};

/// 启动时从环境变量读到的深链意图。
#[derive(Clone, Debug, Default)]
pub struct DeepLink {
    pub open: Option<String>,
    pub panel: Panel,
    pub view: Option<TopView>,
}

pub fn read_deep_link() -> DeepLink {
    let mut dl = DeepLink {
        open: std::env::var("BW_OPEN").ok().filter(|s| !s.is_empty()),
        ..Default::default()
    };
    if let Ok(p) = std::env::var("BW_PANEL") {
        match Panel::parse(&p) {
            Some(panel) => dl.panel = panel,
            // 认不出的值如实打出来,不静默 fallback 到某个默认屏。
            None => eprintln!("[BW_OPEN] 未知 BW_PANEL={p:?},忽略"),
        }
    }
    if let Ok(v) = std::env::var("BW_VIEW") {
        match TopView::parse(&v) {
            Some(view) => dl.view = Some(view),
            None => eprintln!("[BW_OPEN] 未知 BW_VIEW={v:?},忽略"),
        }
    }
    dl
}

/// 壳发给内核的一条请求。
pub enum Req {
    /// 一条真命令。
    Cmd(Command),
    /// 打开某个项目(纯本机导航,不改任何数据)。
    Open(Option<ProjectId>),
    /// 计划屏切到看某一周。
    ViewWeek(String),
    /// 计划屏左栏点「全部」/ 点回某一周。
    ViewAll(bool),
    /// 知识库屏打开某份文档。
    OpenDoc(Option<String>),
    /// 「先不建」——把还没确认的草稿丢掉。草稿本来就没进库,丢掉不留痕。
    DropDrafts,
    /// 会话屏:选中哪个会话 / 切页签 / 展开某个目录 / 中栏开哪个文件。
    /// 全是纯导航,一律不进库。
    SelectSession(Option<bw_v4::model::IssueId>),
    SessionTab(crate::vm::SessionTab),
    ToggleDir(String),
    OpenFile {
        path: String,
        diff: bool,
    },
    /// 知识库屏切页签。代码图与资产两个页签**切过去那一刻现跑一次**
    /// (起 codegraph 子进程、走 `git log`、采仓统计),结果留到下次再点。
    KbTab(crate::vm::KbTab),
    /// 上面那一跑的结果回来了。**不是界面发的** —— 是内核自己派出去的那个任务
    /// 算完之后发回来的,见 `Req::KbTab` 的处理。
    KbComputed {
        tab: crate::vm::KbTab,
        codegraph: Option<crate::vm::CodeGraphVm>,
        assets: Option<crate::vm::AssetsVm>,
    },
    /// 总览那块「项目指标 · 代码仓级」:现采一次。
    CollectRepoStats,
    /// 上面那一采的结果。**不是界面发的**,是派出去的那个任务算完发回来的。
    /// 带着「这是给哪个项目采的」——采一次要几百毫秒,人完全来得及在这期间
    /// 切走,不认项目就会把上一个项目的提交数摆在新项目的总览上。
    RepoStatsComputed {
        project: ProjectId,
        stats: crate::vm::RepoStatsVm,
    },
    /// 接入屏:去平台列一遍「我账号下的仓」。真调 `gh repo list` /
    /// `codehub-cli`,列不出来就把原话摆出来,绝不造一份假列表。
    ListRepos {
        github: bool,
        host: String,
    },
    /// 上面那一问的结果。**不是界面发的**,是派出去的任务问完发回来的。
    ReposListed {
        rows: Vec<crate::vm::RepoRowVm>,
        error: Option<String>,
    },
    /// 接入屏:点了某一行仓 —— 去读那个仓远端的 `.bw/project.toml`,
    /// 读得到就说明这个项目已经被 buddy 接管过,名片直接回显。
    PickRepo {
        github: bool,
        host: String,
        path: String,
        /// 去哪个分支上找 `.bw/project.toml`。列表里那一行给的默认分支;人手打
        /// 地址时是空的,那就交给引擎兜底(它会用 `main`)。**这一格要是错了,
        /// 表现就是把一个接管过的仓报成「没接管过」** —— 默认分支不叫 main 的
        /// 仓很常见。
        git_ref: String,
    },
    /// 上面那一读的结果。`prefill = None` = 那个仓还没被接管过,四个字段要人填。
    RepoPicked {
        path: String,
        prefill: Option<crate::vm::RepoPrefillVm>,
        probe: crate::vm::RepoProbe,
    },
    /// 重新算一遍并推一份新的 ViewModel。
    Refresh,
}

/// 桥是整个进程共用的**一个**句柄,clone 出来的都指向同一个内核线程,
/// 因此永远相等 —— 这样它才能当 Dioxus 组件的 props(props 要求 `PartialEq`,
/// Dioxus 用它判断要不要重渲染;桥本身不携带会变的状态,变的是它推过来的
/// ViewModel)。
#[derive(Clone)]
pub struct Bridge {
    tx: mpsc::UnboundedSender<Req>,
    pub vm: watch::Receiver<Vm>,
    /// PTY 字节流,按会话 id 分批。**不走 ViewModel**——终端一秒钟能吐几百
    /// 批字节,每一批都重拼一次整个 ViewModel 会把界面拖垮;而且字节流是
    /// 一次性的,进了 ViewModel 就会在每次重渲染时被重复写进终端。
    ///
    /// **是 broadcast 不是 watch**:watch 只留最新一个值,一轮渲染慢过一跳就把
    /// 上一批**静默覆盖**掉 —— claude 大段刷屏(编译输出、长 diff)时终端会缺
    /// 段,而且断口两侧的字节被拼起来,跨批的汉字直接变乱码。broadcast 留一整
    /// 个队列,真丢的时候会明说丢了多少(`Lagged`),不装作没发生。
    pub pty: broadcast::Sender<Vec<(bw_v4::model::ConversationId, Vec<u8>)>>,
    /// 长命令的「一步一句」回执(接入项目就是靠它才不像死了)。**和 PTY 同理
    /// 走 broadcast、不走 ViewModel**:命令还没做完,ViewModel 根本没重拼的机会
    /// —— 内核那条队列此刻正卡在 `dispatch` 里,进度得从旁边这条道出来。
    pub progress: broadcast::Sender<bw_v4::app::ProgressLine>,
}

impl PartialEq for Bridge {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Bridge {
    pub fn send(&self, req: Req) {
        let _ = self.tx.send(req);
    }

    pub fn cmd(&self, c: Command) {
        self.send(Req::Cmd(c));
    }
}

/// 库文件默认落在**和旧壳同一个目录、不同名字**:旧壳开 `workbench.db`,
/// 新壳开 `workbench-v4.db`。两个库互不相扰,但放在一起,人找得到、备份
/// 一起带走。`BW_DB` 覆盖。
pub fn db_path() -> String {
    if let Ok(p) = std::env::var("BW_DB") {
        return p;
    }
    let base = if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| format!("{h}/Library/Application Support/BuildersWorkbench"))
            .ok()
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(|a| format!("{a}\\BuildersWorkbench"))
            .ok()
    } else {
        std::env::var("HOME")
            .map(|h| format!("{h}/.local/share/builders-workbench"))
            .ok()
    };
    match base {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            format!("{dir}/{}", bw_v4::DEFAULT_DB_FILENAME)
        }
        None => bw_v4::DEFAULT_DB_FILENAME.to_string(),
    }
}

pub fn workspaces_root() -> PathBuf {
    std::env::var("BW_WORKSPACES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_home().join(".builders-workbench").join("workspaces"))
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// 刷新的回执。**报刚读出来的数**,不报「读过了」—— 这样人一眼能看出刷没刷到
/// 东西:数变了就是仓里真有新东西,数没变就是确实没变化,而不是按钮坏了。
fn refresh_note(vm: &Vm) -> String {
    let tools = vm.env.iter().filter(|t| t.ok == Some(true)).count();
    match &vm.open {
        Some(p) => {
            let w = &p.week_counts;
            format!(
                "刷新完:{} · 本周 待办{} 进行中{} 评审中{} 完成{} 阻塞{} · 本机工具 {}/{} 可用",
                p.name,
                w.todo,
                w.doing,
                w.review,
                w.done,
                w.blocked,
                tools,
                vm.env.len()
            )
        }
        None => format!(
            "刷新完:{} 个项目 · 本机工具 {}/{} 可用",
            vm.projects.len(),
            tools,
            vm.env.len()
        ),
    }
}

/// 去平台列「我账号下的仓」。**两个平台各自的 CLI 都已经有这个能力**
/// (`github::list_repos` / `codehub::list_repos`,V3 就在用),这里只是把它接到
/// 界面上。列不出来就把 CLI 的原话端回去 —— 多半是没装、没登录、或者 codehub
/// 域名填错,这三件事人一看原话就知道该干嘛。
async fn list_repos(github: bool, host: &str) -> (Vec<crate::vm::RepoRowVm>, Option<String>) {
    const LIMIT: u32 = 100;
    if github {
        match bw_engine::github::list_repos(LIMIT).await {
            Ok(rows) => (
                rows.into_iter()
                    .map(|r| crate::vm::RepoRowVm {
                        path: format!("{}/{}", r.owner, r.repo),
                        private: r.private,
                        description: r.description,
                        default_branch: r.default_branch,
                        pushed_at: r.pushed_at,
                    })
                    .collect(),
                None,
            ),
            Err(e) => (Vec::new(), Some(format!("{e}"))),
        }
    } else if host.trim().is_empty() {
        (
            Vec::new(),
            Some("要先填 codehub 域名 —— buddy 不知道你们内部那台在哪".into()),
        )
    } else {
        match bw_engine::codehub::list_repos(host.trim(), LIMIT).await {
            Ok(rows) => (
                rows.into_iter()
                    .map(|r| crate::vm::RepoRowVm {
                        path: r.path,
                        // codehub 给的是 `visibility` 字符串,不是布尔;
                        // 只有明写 private 才算私有,拿不准就不画那个锁。
                        private: r.visibility.eq_ignore_ascii_case("private"),
                        description: r.description,
                        default_branch: r.default_branch,
                        pushed_at: r.pushed_at,
                    })
                    .collect(),
                None,
            ),
            Err(e) => (Vec::new(), Some(format!("{e}"))),
        }
    }
}

/// 读某个仓远端的 `.bw/project.toml`。读得到 = 这个项目已经被 buddy 接管过
/// (你自己接过、或者同事先接的),名片四个字段直接回显,人不用再填一遍。
/// 读不到就是 `None` —— **不猜、不拿仓描述冒充名片**。
async fn fetch_prefill(
    github: bool,
    host: &str,
    path: &str,
    git_ref: &str,
) -> (Option<crate::vm::RepoPrefillVm>, crate::vm::RepoProbe) {
    use crate::vm::RepoProbe;
    let got = if github {
        match path.split_once('/') {
            Some((owner, repo)) => bw_engine::github::fetch_project_toml(owner, repo, git_ref)
                .await
                .map_err(|e| e.to_string()),
            None => Err(format!("「{path}」不是 owner/repo 的样子,没法查")),
        }
    } else if host.trim().is_empty() {
        Err("codehub 域名还没填,查不了".into())
    } else {
        bw_engine::codehub::fetch_project_toml(host.trim(), path, git_ref)
            .await
            .map_err(|e| e.to_string())
    };
    match got {
        // 平台说「没有这份文件」。但**仓根本不存在也是同一个 404** —— 地址
        // 敲错一个字母就会得到「还没接管过,请填」,人填完才发现接的是个
        // 不存在的仓。所以这一支要再问一次「这个仓在不在」,只有仓在、文件
        // 不在,才是真的没接管过。走到这支的只有没接管过的仓,多一次调用
        // 不落在常路上。
        Ok(None) => {
            let exists = if github {
                bw_engine::github::probe_repo(path)
                    .await
                    .map_err(|e| e.to_string())
            } else {
                bw_engine::codehub::probe(host.trim(), path)
                    .await
                    .map_err(|e| e.to_string())
            };
            match exists {
                Ok(_) => (None, RepoProbe::Absent),
                Err(e) => (None, RepoProbe::NoRepo(e)),
            }
        }
        Ok(Some(file)) => (
            Some(crate::vm::RepoPrefillVm {
                name: file.name,
                brief: file.brief,
                benchmark: file.benchmark,
                north_star: file.opportunity,
            }),
            RepoProbe::Adopted,
        ),
        // 没登录、网断了、分支名不对、文件写坏了 —— 都是「没查成」,
        // **不能**说成「没被接管过」。
        Err(e) => (None, RepoProbe::Failed(e)),
    }
}

/// 从一批事件里挑出「开始本周」产出的草稿活标。
fn drafts_of(events: &[bw_v4::command::Event]) -> Option<(String, Vec<String>)> {
    events.iter().find_map(|e| match e {
        bw_v4::command::Event::WeekPlanStarted {
            week, draft_titles, ..
        } => Some((week.clone(), draft_titles.clone())),
        _ => None,
    })
}

/// 起内核线程。返回的桥可以被界面各处 clone。
pub fn spawn(deep_link: DeepLink) -> Bridge {
    let (tx, mut rx) = mpsc::unbounded_channel::<Req>();
    let (vm_tx, vm_rx) = watch::channel(Vm::default());
    // 1024 批 ≈ 一分钟的密集输出。真的堆到这个数还没人取,说明界面那头已经
    // 卡住了,丢批是次要问题。
    let (pty_tx, _) = broadcast::channel(1024);
    // 一条长命令十来行顶天了,64 够用;没人订的时候 send 直接丢,不阻塞。
    let (prog_tx, _) = broadcast::channel(64);

    let pty_tx_handle = pty_tx.clone();
    let prog_tx_handle = prog_tx.clone();
    // 内核往自己这条队列里回发用的口子(知识库那两个页签算完之后发回来)。
    let tx_back = tx.clone();

    std::thread::Builder::new()
        .name("bw-v4-kernel".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("内核 runtime 起不来");
            rt.block_on(async move {
                let db = db_path();
                let store = match V4Store::open(&db).await {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = vm_tx.send(Vm {
                            ready: true,
                            fatal: Some(format!("本机数据库打不开({db}):{e}")),
                            ..Vm::default()
                        });
                        return;
                    }
                };
                // 工作区根目录三级取值:**人在设置屏改过的最大**(存在库的
                // `app_meta` 里),其次是 `BW_WORKSPACES`(开发期覆盖),最后
                // 才是默认值。人改过就该一直算数,不然设置屏那个输入框是假的。
                let root = match store.meta(bw_v4::store::WORKSPACES_ROOT_KEY).await {
                    Ok(Some(saved)) if !saved.trim().is_empty() => PathBuf::from(saved.trim()),
                    _ => workspaces_root(),
                };
                let _ = std::fs::create_dir_all(&root);
                // 桌面壳里 ▶跑 起的是真的 claude,挂在内嵌终端里,人全程看得
                // 见、随时能停。指挥器那条路仍然走自我标注的替身 —— 它没有界
                // 面渲染字节流。
                let mut app = App::new(
                    store.clone(),
                    root.clone(),
                    Arc::new(bw_engine::InteractiveCliExecutor::new()),
                )
                .with_pty()
                .with_progress(prog_tx.clone());

                let mut ui = vm_build::UiState {
                    open: None,
                    session_open: None,
                    session_tab: crate::vm::SessionTab::default(),
                    expanded_dirs: Vec::new(),
                    open_file: String::new(),
                    kb_tab: crate::vm::KbTab::default(),
                    kb_codegraph: None,
                    kb_assets: None,
                    repo_stats: None,
                    viewing_week: bw_v4::isoweek::current_week(),
                    view_all: false,
                    open_doc: None,
                    note: None,
                    note_seq: 0,
                    pending_drafts: None,
                    db_path: db.clone(),
                    workspaces_root: root.display().to_string(),
                    repos: crate::vm::RepoPickerVm::default(),
                };

                // 「刷新」按下了没有。回执要等 ViewModel 拼完才写得出来。
                let mut refresh_receipt = false;

                // 深链:BW_OPEN=<slug 或项目名> 打开某个项目。
                if let Some(want) = &deep_link.open {
                    match vm_build::find_project(&store, want).await {
                        Some(p) => {
                            ui.open = Some(p.id);
                            eprintln!(
                                "[BW_OPEN] {want:?} -> project={} panel={:?}",
                                p.slug, deep_link.panel
                            );
                        }
                        None => eprintln!("[BW_OPEN] 找不到项目 {want:?}"),
                    }
                }

                let mut vm = vm_build::build(&app, &ui).await;
                eprintln!("[BW_BOOT] projects={} db={db}", vm.projects.len());
                // 环境条的探活结果也打出来 —— 它说「找不到 claude」的时候,人
                // 得能在终端里拿 `which claude` 当场对。2026-08-20 就栽在这上面:
                // 探活在 macOS 上恒返回「找不到」,而界面上没有任何办法核。
                for t in &vm.env {
                    let state = match t.ok {
                        Some(true) => "有",
                        Some(false) => "没有",
                        None => "没探",
                    };
                    eprintln!("[BW_ENV] {} = {} · {}", t.label, state, t.detail);
                }
                // 打开了项目就把这一份 ViewModel 的关键计数打出来 —— 这些数
                // 都能用 sqlite3 当场对(活数 / 会话数 / 事件数 / 周数),
                // 比截图硬。
                if let Some(o) = &vm.open {
                    eprintln!(
                        "[BW_VM] project={} 活={} 会话={} 事件={} 周={} 等你合入={} 技能={}",
                        o.slug,
                        o.board.columns.iter().map(|c| c.cards.len()).sum::<usize>(),
                        o.sessions.len(),
                        o.notify.events.len(),
                        o.weeks.len(),
                        o.notify.to_merge.len(),
                        o.config.skills.len(),
                    );
                    // 事件流最新那一条的时间戳也打出来 —— 它是**本机时区**
                    // 的实证:能直接和 `date -r <unix>` 对上,对不上就说明
                    // 时区那条路又退回 UTC 了(见 vm_derive::stamp)。
                    // 总览那条「本周计划进度」的五段。**只认当前周**,与计划屏
                    // 左栏看的是哪一周无关 —— 这一行就是那条纪律的读回。
                    let c = &o.week_counts;
                    eprintln!(
                        "[BW_VM] 本周{} 待办={} 进行中={} 评审中={} 完成={} 阻塞={}",
                        o.current_week, c.todo, c.doing, c.review, c.done, c.blocked,
                    );
                    if let Some(e) = o.notify.events.first() {
                        eprintln!("[BW_VM] 最新事件 {} · {}", e.time, e.text);
                    }
                    eprintln!(
                        "[BW_VM] 映射={} 连接器={} 定时={} 群={} 指标={} 未读={}",
                        o.config.mappings.len(),
                        o.config.connectors.len(),
                        o.config.crons.len(),
                        o.config.chat_provider,
                        o.metrics.lagging.len()
                            + o.metrics.leading.len()
                            + usize::from(o.metrics.north_star.is_some()),
                        o.notify.unread,
                    );
                }
                // BW_KB_DUMP=1:把知识库三个页签的数字打进 stderr,好让人拿
                // `git ls-files` / `codegraph files -j` / `cat docs/releases.md`
                // 当场对。截图对不了数,这个能。
                if std::env::var("BW_KB_DUMP").is_ok_and(|v| v != "0") {
                    if let Some(pid) = ui.open {
                        if let Ok(ws) = app.workspace_of(pid).await {
                            let kb = vm_kb::build_kb(&ws, crate::vm::KbTab::Docs, None);
                            for g in &kb.groups {
                                eprintln!("[BW_KB] 组「{}」{} 个文件", g.title, g.files.len());
                            }
                            let cg = vm_kb::build_codegraph(&ws).await;
                            eprintln!(
                                "[BW_KB] 代码图 state={} 榜上 {} 行 头一名={} 头一名体积={} err={:?}",
                                cg.state,
                                cg.rows.len(),
                                cg.rows.first().map(|r| r.path.as_str()).unwrap_or("—"),
                                cg.rows.first().map(|r| r.size).unwrap_or(0),
                                cg.error
                            );
                            let a = vm_kb::build_assets(&store, pid, &ws).await;
                            eprintln!(
                                "[BW_KB] 资产:技能 {} · 蒸馏 {} · 产物 {} · 发版 {} · 仓统计 {:?}",
                                a.skills.len(),
                                a.distilled.len(),
                                a.artifacts.len(),
                                a.releases.len(),
                                a.repo_stats
                            );
                        }
                    }
                }
                // 仓文件读不动的实话也打进 stderr:界面上有横幅,命令行验收
                // 也看得见,不用靠截图。
                for w in &vm.warnings {
                    eprintln!("[BW_WARN] {w}");
                }
                let _ = vm_tx.send(vm.clone());

                // 两个节拍器。终端字节 60ms 一次(约 16fps,人眼看着连续,而
                // 且不重拼 ViewModel);定时判据 60s 一次 —— 它查的是「本周有
                // 没有那张活」,快一点慢一点都不影响结论。
                let mut pty_tick = tokio::time::interval(Duration::from_millis(60));
                let mut sched_tick = tokio::time::interval(Duration::from_secs(60));
                loop {
                    let req = tokio::select! {
                        r = rx.recv() => match r {
                            Some(r) => r,
                            None => break,
                        },
                        _ = pty_tick.tick() => {
                            let batches = app.drain_pty_events();
                            if !batches.is_empty() {
                                let _ = pty_tx.send(batches);
                            }
                            continue;
                        }
                        _ = sched_tick.tick() => {
                            // 只对打开着的项目跑定时。没打开任何项目时不动 ——
                            // 后台替一堆项目自动建活,人一打开就看见一片自己没
                            // 点过的活,不是好体验。
                            let Some(pid) = ui.open else { continue };
                            match app.dispatch(Command::TickScheduler { project_id: pid }).await {
                                Ok(events) if events.is_empty() => continue,
                                Ok(events) => ui.set_note(vm_build::note_of(&events)),
                                Err(e) => ui.set_note(Some(format!("定时没跑成:{e}"))),
                            }
                            vm = vm_build::build(&app, &ui).await;
                            let _ = vm_tx.send(vm.clone());
                            continue;
                        }
                    };
                    // 终端的键盘与尺寸**不重拼 ViewModel**。重拼一次要跑十来个
                    // git 子进程(健康三判据 + 改动文件 + 领先落后)、扫一遍
                    // docs/plan/、解析四个 toml —— 而人打字时每 30ms 就来一条。
                    // 全串在内核这条单线程上,终端会明显卡顿,pty 那一跳也排不
                    // 上队。这两条命令本来也不改任何会显示的东西。
                    if let Req::Cmd(
                        c @ (Command::TerminalInput { .. } | Command::TerminalResize { .. }),
                    ) = req
                    {
                        let _ = app.dispatch(c).await;
                        continue;
                    }
                    match req {
                        Req::Cmd(c) => {
                            let confirming = matches!(c, Command::ConfirmWeekDraft { .. });
                            match app.dispatch(c).await {
                                Ok(events) => {
                                    ui.set_note(vm_build::note_of(&events));
                                    // 「开始本周」产出的草稿活标先接住,等人在计划屏
                                    // 点确认才真的建活。
                                    if let Some((week, titles)) = drafts_of(&events) {
                                        ui.pending_drafts = Some((week, titles));
                                    }
                                    if confirming {
                                        ui.pending_drafts = None;
                                    }
                                    // 接完一个项目就把它打开 —— 人点「完成接入」
                                    // 要的是进这个项目,不是停在接入屏上自己再点
                                    // 一遍项目墙。
                                    if let Some(bw_v4::command::Event::ProjectCreated {
                                        id,
                                        ..
                                    }) = events.first()
                                    {
                                        ui.open = Some(*id);
                                        ui.open_doc = None;
                                        ui.viewing_week = bw_v4::isoweek::current_week();
                                        ui.view_all = false;
                                        ui.kb_tab = crate::vm::KbTab::default();
                                        ui.kb_codegraph = None;
                                        ui.kb_assets = None;
                                        ui.repo_stats = None;
                                        ui.repos = crate::vm::RepoPickerVm::default();
                                    }
                                    // 项目被移走了:它要是正开着,得回项目墙 ——
                                    // 留在一个已经不存在的项目上,整屏都是空的。
                                    if events.iter().any(|e| {
                                        matches!(e, bw_v4::command::Event::ProjectRemoved { .. })
                                    }) {
                                        ui.open = None;
                                        ui.open_doc = None;
                                        ui.kb_codegraph = None;
                                        ui.kb_assets = None;
                                        ui.repo_stats = None;
                                    }
                                    // 改了工作区根目录:设置屏那一行要立刻显示
                                    // 新值,不能等下次重启。
                                    for e in &events {
                                        if let bw_v4::command::Event::WorkspacesRootChanged {
                                            path,
                                            ..
                                        } = e
                                        {
                                            ui.workspaces_root = path.clone();
                                        }
                                    }
                                }
                                // 失败就把失败的原话摆出来。壳不替内核圆场。
                                Err(e) => ui.set_note(Some(format!("没做成:{e}"))),
                            }
                        }
                        Req::Open(id) => {
                            ui.open = id;
                            ui.open_doc = None;
                            ui.viewing_week = bw_v4::isoweek::current_week();
                            ui.view_all = false;
                            // 知识库那两个页签的结果是**上一个项目**的:它的技能
                            // 清单、它的 git 产物、它的发版记录、它的仓统计。留着
                            // 的话换个项目点进去,一屏数字没有一个是这个项目的,
                            // 而且什么提示都没有。一律清空,让它显示「点一下就现扫」。
                            ui.kb_tab = crate::vm::KbTab::default();
                            ui.kb_codegraph = None;
                            ui.kb_assets = None;
                            // 仓统计同理:留着就是把上一个项目的提交数摆在这个
                            // 项目的总览上。
                            ui.repo_stats = None;
                        }
                        Req::ViewWeek(w) => {
                            ui.viewing_week = w;
                            ui.view_all = false;
                        }
                        Req::ViewAll(v) => ui.view_all = v,
                        Req::OpenDoc(p) => ui.open_doc = p,
                        Req::DropDrafts => ui.pending_drafts = None,
                        Req::SelectSession(id) => {
                            // **选中一张有会话的活 = 把上次那场对话接回来。**
                            // 光切视图的话人看到的是一片空白 —— V3 本来就是接
                            // 回来的,V4 这条掉了。只在「真有会话、而且没开着」
                            // 时才发命令:没有会话的活(buddy 自己写的、回填的)
                            // 发过去只会弹一句「还没有过会话」,那不是错误,是
                            // 常态。
                            if let Some(iid) = id {
                                if let Ok(Some(conv)) =
                                    app.store().conversation_for_issue(iid).await
                                {
                                    if !app.pty_live(conv.id) {
                                        if let Err(e) = app
                                            .dispatch(Command::ReopenSession { issue_id: iid })
                                            .await
                                        {
                                            ui.set_note(Some(format!("接不回上次那场会话:{e}")));
                                        }
                                    }
                                }
                            }
                            ui.session_open = id;
                            // 换会话 = 换工作区,上一个会话展开的目录、开着的
                            // 文件在新工作区里多半不存在,一律清掉重来。
                            ui.expanded_dirs.clear();
                            ui.open_file.clear();
                            ui.session_tab = crate::vm::SessionTab::default();
                        }
                        Req::SessionTab(t) => ui.session_tab = t,
                        Req::ToggleDir(d) => {
                            if let Some(i) = ui.expanded_dirs.iter().position(|x| *x == d) {
                                ui.expanded_dirs.remove(i);
                                // 收起一个目录,它下面所有已展开的子目录也一起
                                // 收起,不然再点开它会看见一堆孤儿层。
                                ui.expanded_dirs
                                    .retain(|x| !x.starts_with(&format!("{d}/")));
                            } else {
                                ui.expanded_dirs.push(d);
                            }
                        }
                        Req::OpenFile { path, diff } => {
                            ui.open_file = path;
                            ui.session_tab = if diff {
                                crate::vm::SessionTab::Diff
                            } else {
                                crate::vm::SessionTab::File
                            };
                        }
                        Req::KbTab(t) => {
                            ui.kb_tab = t;
                            // 每次点页签就是一次全新的现跑 —— 不缓存,和「对账
                            // 是纯读操作不需要缓存」同一个取舍。
                            ui.kb_codegraph = None;
                            ui.kb_assets = None;
                            // **派出去算,不在这条循环里等**。这两样各要起好几个
                            // 子进程(codegraph、git log、仓统计),在这里 await
                            // 就等于把内核这条单线程连同 60ms 的终端节拍一起按住
                            // —— 人正看着 agent 刷屏时点一下「代码图」,终端就卡住
                            // 了。算完经 KbComputed 发回来。
                            let want = matches!(
                                t,
                                crate::vm::KbTab::CodeGraph | crate::vm::KbTab::Assets
                            );
                            if let (true, Some(pid)) = (want, ui.open) {
                                if let Ok(ws) = app.workspace_of(pid).await {
                                    let back = tx_back.clone();
                                    let st = store.clone();
                                    tokio::spawn(async move {
                                        let (codegraph, assets) =
                                            if t == crate::vm::KbTab::CodeGraph {
                                                (Some(vm_kb::build_codegraph(&ws).await), None)
                                            } else {
                                                (
                                                    None,
                                                    Some(vm_kb::build_assets(&st, pid, &ws).await),
                                                )
                                            };
                                        let _ = back.send(Req::KbComputed {
                                            tab: t,
                                            codegraph,
                                            assets,
                                        });
                                    });
                                }
                            }
                        }
                        Req::KbComputed {
                            tab,
                            codegraph,
                            assets,
                        } => {
                            // 算的时候人可能已经切走了。结果就丢掉 —— 把上一个
                            // 页签的数字贴到当前页签上比不显示更糟。
                            if ui.kb_tab == tab {
                                ui.kb_codegraph = codegraph;
                                ui.kb_assets = assets;
                            }
                        }
                        Req::CollectRepoStats => {
                            // 和知识库那两个页签同一个做法:派出去算,不在这条
                            // 循环里 await —— 它要起好几个 git 子进程,在这里等
                            // 就把内核这条单线程连同终端的 60ms 节拍一起按住。
                            if let Some(pid) = ui.open {
                                if let Ok(ws) = app.workspace_of(pid).await {
                                    let back = tx_back.clone();
                                    tokio::spawn(async move {
                                        let _ = back.send(Req::RepoStatsComputed {
                                            project: pid,
                                            stats: vm_derive::collect_repo_stats(&ws).await,
                                        });
                                    });
                                }
                            }
                        }
                        // 采的时候是哪个项目,回来时还得是哪个项目 —— 不是就
                        // 整份丢掉,宁可让人再点一次,也不摆错项目的数。
                        Req::RepoStatsComputed { project, stats } => {
                            if ui.open == Some(project) {
                                ui.repo_stats = Some(stats);
                            }
                        }
                        Req::ListRepos { github, host } => {
                            ui.repos = crate::vm::RepoPickerVm {
                                loading: true,
                                asked: true,
                                ..Default::default()
                            };
                            // 起子进程要几百毫秒到几秒,不能占着内核这条单线程
                            // ——派出去,回来再发一条 Req。
                            let back = tx_back.clone();
                            tokio::spawn(async move {
                                let (rows, error) = list_repos(github, &host).await;
                                let _ = back.send(Req::ReposListed { rows, error });
                            });
                        }
                        Req::ReposListed { rows, error } => {
                            ui.repos.loading = false;
                            ui.repos.rows = rows;
                            ui.repos.error = error;
                        }
                        Req::PickRepo {
                            github,
                            host,
                            path,
                            git_ref,
                        } => {
                            ui.repos.picked = Some(path.clone());
                            ui.repos.prefill = None;
                            ui.repos.probe = crate::vm::RepoProbe::Loading;
                            let back = tx_back.clone();
                            tokio::spawn(async move {
                                let (prefill, probe) =
                                    fetch_prefill(github, &host, &path, &git_ref).await;
                                let _ = back.send(Req::RepoPicked {
                                    path,
                                    prefill,
                                    probe,
                                });
                            });
                        }
                        Req::RepoPicked {
                            path,
                            prefill,
                            probe,
                        } => {
                            // 人可能在读的过程中又点了别的仓 —— 认准是不是当前
                            // 这一个,不然回来的名片会盖到另一个仓上。
                            if ui.repos.picked.as_deref() == Some(path.as_str()) {
                                ui.repos.prefill = prefill;
                                ui.repos.probe = probe;
                            }
                        }
                        Req::Refresh => {
                            // 「刷新」本身不用干活 —— 探活、仓文件、git 都是拼
                            // ViewModel 那一刻现跑的。但回执必须**带上刚读出来的
                            // 数**:一句「重新读了一遍」等于什么都没说,人还是不
                            // 知道刷没刷、刷出了什么。数要等下面 build 完才有,
                            // 所以这里只立个旗。
                            refresh_receipt = true;
                        }
                    }
                    vm = vm_build::build(&app, &ui).await;
                    // 刷新的回执在这里补:此刻的 vm 才是「刚读出来的那一份」。
                    if refresh_receipt {
                        refresh_receipt = false;
                        ui.set_note(Some(refresh_note(&vm)));
                        vm.note = ui.note.clone();
                        vm.note_seq = ui.note_seq;
                    }
                    let _ = vm_tx.send(vm.clone());
                }
            });
        })
        .expect("内核线程起不来");

    Bridge {
        tx,
        vm: vm_rx,
        pty: pty_tx_handle,
        progress: prog_tx_handle,
    }
}
