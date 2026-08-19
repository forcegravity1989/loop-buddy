//! 内核桥:一条独立 tokio 线程拿着 `bw_v4::App`,壳这边只发命令、收 ViewModel。
//!
//! 壳里没有一行业务判断。界面点一下 → 发一条 `Command` 过桥 → 内核执行 → 重建
//! 一份 [`Vm`] → 经 watch 通道推回来 → 界面重画。**壳自己绝不伪造成功事件**:
//! 命令失败就把失败的原话放进 `Vm::note`,不假装做成了。

mod vm_build;
mod vm_panels;

use crate::vm::Vm;
use bw_v4::app::App;
use bw_v4::command::Command;
use bw_v4::model::ProjectId;
use bw_v4::V4Store;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, watch};

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
    /// 重新算一遍并推一份新的 ViewModel。
    Refresh,
}

#[derive(Clone)]
pub struct Bridge {
    tx: mpsc::UnboundedSender<Req>,
    pub vm: watch::Receiver<Vm>,
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

/// 起内核线程。返回的桥可以被界面各处 clone。
pub fn spawn(deep_link: DeepLink) -> Bridge {
    let (tx, mut rx) = mpsc::unbounded_channel::<Req>();
    let (vm_tx, vm_rx) = watch::channel(Vm::default());

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
                let mut app = App::new(
                    store.clone(),
                    root.clone(),
                    // A 刀所有 ▶跑 都走自我标注的替身:内嵌终端是下一刀的事,
                    // 现在接一个假的真执行器只会让人以为它真跑了。
                    Arc::new(bw_engine::MockInteractiveExecutor::new()),
                );

                let mut ui = vm_build::UiState {
                    open: None,
                    viewing_week: bw_v4::isoweek::current_week(),
                    open_doc: None,
                    note: None,
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
                let _ = vm_tx.send(vm.clone());

                while let Some(req) = rx.recv().await {
                    match req {
                        Req::Cmd(c) => match app.dispatch(c).await {
                            Ok(events) => ui.note = vm_build::note_of(&events),
                            // 失败就把失败的原话摆出来。壳不替内核圆场。
                            Err(e) => ui.note = Some(format!("没做成:{e}")),
                        },
                        Req::Open(id) => {
                            ui.open = id;
                            ui.open_doc = None;
                            ui.viewing_week = bw_v4::isoweek::current_week();
                        }
                        Req::ViewWeek(w) => ui.viewing_week = w,
                        Req::OpenDoc(p) => ui.open_doc = p,
                        Req::Refresh => {}
                    }
                    vm = vm_build::build(&app, &ui).await;
                    let _ = vm_tx.send(vm.clone());
                }
            });
        })
        .expect("内核线程起不来");

    Bridge { tx, vm: vm_rx }
}
