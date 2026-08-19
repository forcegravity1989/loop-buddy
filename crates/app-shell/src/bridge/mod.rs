//! 内核桥:一条独立 tokio 线程拿着 `bw_v4::App`,壳这边只发命令、收 ViewModel。
//!
//! 壳里没有一行业务判断。界面点一下 → 发一条 `Command` 过桥 → 内核执行 → 重建
//! 一份 [`Vm`] → 经 watch 通道推回来 → 界面重画。**壳自己绝不伪造成功事件**:
//! 命令失败就把失败的原话放进 `Vm::note`,不假装做成了。

mod vm_build;
mod vm_kb;
mod vm_panels;

use crate::vm::Vm;
use bw_v4::app::App;
use bw_v4::command::Command;
use bw_v4::model::ProjectId;
use bw_v4::V4Store;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, watch};

/// 深链要跳到哪。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Panel {
    #[default]
    Overview,
    Plan,
    Session,
    Notify,
    Config,
    Kb,
}

impl Panel {
    pub fn parse(s: &str) -> Option<Panel> {
        Some(match s {
            "overview" => Panel::Overview,
            "plan" => Panel::Plan,
            "session" => Panel::Session,
            "notify" => Panel::Notify,
            "config" => Panel::Config,
            "kb" => Panel::Kb,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Panel::Overview => "总览",
            Panel::Plan => "计划",
            Panel::Session => "会话",
            Panel::Notify => "通知",
            Panel::Config => "配置",
            Panel::Kb => "知识库",
        }
    }

    pub const ALL: [Panel; 6] = [
        Panel::Overview,
        Panel::Plan,
        Panel::Session,
        Panel::Notify,
        Panel::Config,
        Panel::Kb,
    ];
}

/// 顶层三屏里不依赖「打开某个项目」的那两个。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopView {
    Onboard,
    Settings,
}

impl TopView {
    pub fn parse(s: &str) -> Option<TopView> {
        Some(match s {
            "onboard" => TopView::Onboard,
            "settings" => TopView::Settings,
            _ => return None,
        })
    }
}

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

    let pty_tx_handle = pty_tx.clone();

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
                let root = workspaces_root();
                let _ = std::fs::create_dir_all(&root);
                // 桌面壳里 ▶跑 起的是真的 claude,挂在内嵌终端里,人全程看得
                // 见、随时能停。指挥器那条路仍然走自我标注的替身 —— 它没有界
                // 面渲染字节流。
                let mut app = App::new(
                    store.clone(),
                    root.clone(),
                    Arc::new(bw_engine::InteractiveCliExecutor::new()),
                )
                .with_pty();

                let mut ui = vm_build::UiState {
                    open: None,
                    session_open: None,
                    session_tab: crate::vm::SessionTab::default(),
                    expanded_dirs: Vec::new(),
                    open_file: String::new(),
                    kb_tab: crate::vm::KbTab::default(),
                    kb_codegraph: None,
                    kb_assets: None,
                    viewing_week: bw_v4::isoweek::current_week(),
                    open_doc: None,
                    note: None,
                    pending_drafts: None,
                    db_path: db.clone(),
                    workspaces_root: root.display().to_string(),
                };

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
                            let cg = vm_kb::build_codegraph(&ws);
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
                                Ok(events) => ui.note = vm_build::note_of(&events),
                                Err(e) => ui.note = Some(format!("定时没跑成:{e}")),
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
                                    ui.note = vm_build::note_of(&events);
                                    // 「开始本周」产出的草稿活标先接住,等人在计划屏
                                    // 点确认才真的建活。
                                    if let Some((week, titles)) = drafts_of(&events) {
                                        ui.pending_drafts = Some((week, titles));
                                    }
                                    if confirming {
                                        ui.pending_drafts = None;
                                    }
                                }
                                // 失败就把失败的原话摆出来。壳不替内核圆场。
                                Err(e) => ui.note = Some(format!("没做成:{e}")),
                            }
                        }
                        Req::Open(id) => {
                            ui.open = id;
                            ui.open_doc = None;
                            ui.viewing_week = bw_v4::isoweek::current_week();
                        }
                        Req::ViewWeek(w) => ui.viewing_week = w,
                        Req::OpenDoc(p) => ui.open_doc = p,
                        Req::DropDrafts => ui.pending_drafts = None,
                        Req::SelectSession(id) => {
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
                            if let Some(pid) = ui.open {
                                let ws = app.workspace_of(pid).await.ok();
                                match (t, ws) {
                                    (crate::vm::KbTab::CodeGraph, Some(ws)) => {
                                        ui.kb_codegraph = Some(vm_kb::build_codegraph(&ws));
                                    }
                                    (crate::vm::KbTab::Assets, Some(ws)) => {
                                        ui.kb_assets =
                                            Some(vm_kb::build_assets(&store, pid, &ws).await);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Req::Refresh => {}
                    }
                    vm = vm_build::build(&app, &ui).await;
                    let _ = vm_tx.send(vm.clone());
                }
            });
        })
        .expect("内核线程起不来");

    Bridge {
        tx,
        vm: vm_rx,
        pty: pty_tx_handle,
    }
}
