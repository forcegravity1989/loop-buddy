//! V4 桌面壳 —— 六入口 + 顶层三屏。
//!
//! 一条规矩:**这个 crate 只渲染和转发意图,它自己什么都不算**。健康是什么
//! 颜色、活能不能转到那一列、指标有没有读数,全部由内核算好塞进 ViewModel。
//! 壳里不该出现第二套判断逻辑,更不该在命令失败时自己造一个成功的样子。
//!
//! 深链(启动即打读回证据,和旧壳同一套机制):
//!
//! ```bash
//! BW_DB=<db> BW_OPEN=<项目> BW_PANEL=overview|plan|session|notify|config|kb ./bw-v4-dev
//! BW_VIEW=onboard|settings ./bw-v4-dev
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adapters;
mod bridge;
mod chrome;
mod screens;
mod theme;
mod vm;

use adapters::terminal_xterm::TerminalWidget;
use bridge::{Bridge, DeepLink, Panel, Req, TopView};
use dioxus::prelude::*;
use vm::SessionTab;

fn main() {
    // 起任何线程之前先把本机时区定住 —— 周是按本机时区算的。
    bw_v4::isoweek::init_local_offset();
    let deep_link = bridge::read_deep_link();
    let builder = dioxus::desktop::WindowBuilder::new()
        .with_title("Builders' Workbench")
        .with_inner_size(dioxus::desktop::LogicalSize::new(1440.0, 920.0));

    let cfg = dioxus::desktop::Config::new().with_window(builder);
    // Windows:wry 默认的拖放处理器会让 WebView2 屏蔽页面内的拖放,计划屏那
    // 六列就拖不动了。必须关掉。
    #[cfg(windows)]
    let cfg = cfg.with_disable_drag_drop_handler(true);

    LAUNCH_DEEP_LINK.with(|c| *c.borrow_mut() = Some(deep_link));
    dioxus::LaunchBuilder::new().with_cfg(cfg).launch(Root);
}

thread_local! {
    static LAUNCH_DEEP_LINK: std::cell::RefCell<Option<DeepLink>> = const { std::cell::RefCell::new(None) };
}

#[component]
fn GlobalChrome() -> Element {
    rsx! {
        document::Title { "Builders' Workbench" }
        document::Style { {theme::GLOBAL_CSS} }
    }
}

/// 顶栏那格「数据来自哪」写的就是本机库文件名。
fn db_label(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

#[component]
fn Root() -> Element {
    let deep_link = use_hook(|| {
        LAUNCH_DEEP_LINK
            .with(|c| c.borrow_mut().take())
            .unwrap_or_default()
    });
    let initial_panel = deep_link.panel;
    let initial_view = deep_link.view;
    let bridge = use_context_provider(move || bridge::spawn(deep_link));

    let mut vm = use_signal(vm::Vm::default);
    let nav = use_context_provider(move || bridge::PanelNav(Signal::new(initial_panel)));
    use_context_provider(|| bridge::GuideNav(Signal::new(None)));
    let mut panel = nav.0;
    let mut top_view = use_signal(move || initial_view);

    // 订阅内核推过来的 ViewModel。
    use_future({
        let rx = bridge.vm.clone();
        move || {
            let mut rx = rx.clone();
            async move {
                loop {
                    if rx.changed().await.is_err() {
                        break;
                    }
                    let next = rx.borrow_and_update().clone();
                    vm.set(next);
                }
            }
        }
    });

    let v = vm.read().clone();

    if let Some(fatal) = &v.fatal {
        return rsx! {
            GlobalChrome {}
            div { class: "app-root", style: "align-items:center;justify-content:center;padding:40px;",
                div { class: "card", style: "padding:28px;max-width:560px;color:var(--alert-deep);font-size:14px;line-height:1.9;",
                    h3 { "起不来" }
                    "{fatal}"
                }
            }
        };
    }

    if !v.ready {
        return rsx! {
            GlobalChrome {}
            div { class: "app-root", style: "align-items:center;justify-content:center;color:var(--ink-3);",
                "正在读本机数据……"
            }
        };
    }

    let cur = *panel.read();
    let tv = *top_view.read();
    let project = v.open.clone();
    let in_project = project.is_some() && tv.is_none();
    let source = db_label(&v.settings.db_path);

    // 顶栏标题:项目内是六入口的名字,顶层是那一屏的名字。
    let title = match (tv, in_project) {
        (Some(TopView::Onboard), _) => "接入项目".to_string(),
        (Some(TopView::Settings), _) => "设置".to_string(),
        (None, true) => cur.label().to_string(),
        (None, false) => "项目墙".to_string(),
    };

    let b_rail = bridge.clone();
    let unread = project
        .as_ref()
        .map(|p| (p.notify.in_review.len() + p.notify.blocked.len()) as u32)
        .unwrap_or(0);

    rsx! {
        GlobalChrome {}
        div { class: "app-root",
            chrome::IconRail {
                in_project,
                top_view: tv,
                on_wall: move |_| {
                    top_view.set(None);
                    b_rail.send(Req::Open(None));
                },
                on_settings: move |_| top_view.set(Some(TopView::Settings)),
            }
            if in_project {
                if let Some(p) = project.as_ref() {
                    chrome::ProjectRail {
                        name: p.name.clone(),
                        version: if p.card.current_version.is_empty() { "—".to_string() } else { p.card.current_version.clone() },
                        signal: p.health.signal,
                        unread,
                        cur,
                        on_wall: {
                            let b = bridge.clone();
                            move |_| { top_view.set(None); b.send(Req::Open(None)); }
                        },
                        on_nav: move |p: bridge::Panel| panel.set(p),
                    }
                }
            }
            div { class: "main",
                chrome::TopBar {
                    title,
                    project: if in_project { project.as_ref().map(|p| p.name.clone()) } else { None },
                    source,
                    bridge: bridge.clone(),
                }
                // 仓文件读不动的实话。**不退回默认值假装文件不存在**。
                for w in v.warnings.iter() {
                    div { key: "{w}", class: "warnbar", "{w}" }
                }
                // 会话屏的三栏由 .content 自己摆 —— 终端挂在这一层,靠这套
                // 网格落进中栏下半格(理由见 screens/session 的头注)。
                div {
                    class: if in_project && cur == Panel::Session { "content session-mode" } else { "content" },
                    {body(tv, cur, &v, &project, &bridge, top_view)}
                }
            }
            // 指南只在项目墙露出来 —— 和原型一致:进了项目,屏本身就是说明书。
            if !in_project && tv.is_none() {
                chrome::GuideDrawer {}
            }
            chrome::Toast { note: v.note.clone() }
        }
    }
}

/// 内容区分发。终端**挂在屏的外面**,理由见下面那段注释。
fn body(
    tv: Option<TopView>,
    cur: Panel,
    v: &vm::Vm,
    project: &Option<vm::ProjectVm>,
    bridge: &Bridge,
    mut top_view: Signal<Option<TopView>>,
) -> Element {
    if let Some(t) = tv {
        let close = move |_| top_view.set(None);
        return match t {
            TopView::Onboard => rsx! {
                screens::onboard::View { vm: v.clone(), bridge: bridge.clone(), close }
            },
            TopView::Settings => rsx! {
                screens::settings::View { vm: v.clone(), close }
            },
        };
    }
    let Some(p) = project.clone() else {
        let b = bridge.clone();
        return rsx! {
            screens::wall::View {
                vm: v.clone(),
                bridge: b,
                go_top: move |t| top_view.set(Some(t)),
            }
        };
    };
    rsx! {
        {screens::route(cur, &p, bridge)}
        // 终端本体挂在**屏的外面**,不在会话屏里面。
        //
        // 挂在会话屏里的话,人一切到别的面板,整个屏连同终端一起被卸载
        // —— 收字节的那条循环也就没了,这期间 agent 说的话是**真丢的**,
        // 切回来只剩一个空终端。挂在这里,六个面板怎么切它都活着;不是
        // 当前焦点的那个只是被挪到屏外(不是 display:none —— 那会让
        // xterm 以 0×0 打开,再回来一片空白)。
        //
        // 只挂「进程还活着」或「正被选中」的会话:早就跑完的会话再挂一
        // 个每 30ms 轮询一次的循环纯属白烧。代价是切走再回来看一个已经
        // 结束的会话,回放是空的 —— 存不存回放还没做(见 LEFTOVERS)。
        for s in p.sessions.iter().filter(|s| s.live || p.session_open == Some(s.issue_id)) {
            TerminalWidget {
                key: "{s.conversation_id:?}",
                conversation_id: s.conversation_id,
                focused: cur == Panel::Session
                    && p.session_open == Some(s.issue_id)
                    && p.workbench.tab == SessionTab::Terminal,
                bridge: bridge.clone(),
            }
        }
    }
}
