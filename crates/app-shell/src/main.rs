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
mod screens;
mod theme;
mod vm;

use adapters::terminal_xterm::TerminalWidget;
use bridge::{DeepLink, Panel, Req, TopView};
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
    let mut panel = use_signal(move || initial_panel);
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
            div {
                style: "min-height:100vh;background:{theme::PAPER};font-family:{theme::SANS};\
                        display:flex;align-items:center;justify-content:center;padding:40px;",
                div {
                    style: "{theme::card()}padding:28px;max-width:560px;color:{theme::ALERT_DEEP};\
                            font-size:14px;line-height:1.9;",
                    div { style: "font-family:{theme::SERIF};font-size:18px;margin-bottom:10px;", "起不来" }
                    "{fatal}"
                }
            }
        };
    }

    if !v.ready {
        return rsx! {
            GlobalChrome {}
            div {
                style: "min-height:100vh;background:{theme::PAPER};font-family:{theme::SANS};\
                        display:flex;align-items:center;justify-content:center;color:{theme::INK_3};",
                "正在读本机数据……"
            }
        };
    }

    // 顶层两屏(接入 / 设置)盖在项目墙之上;打开了项目就走六入口。
    if let Some(tv) = *top_view.read() {
        let b = bridge.clone();
        let close = move |_| top_view.set(None);
        return rsx! {
            GlobalChrome {}
            div {
                style: "min-height:100vh;background:{theme::PAPER};font-family:{theme::SANS};color:{theme::INK};",
                match tv {
                    TopView::Onboard => rsx! {
                        screens::onboard::View { vm: v.clone(), bridge: b.clone(), close }
                    },
                    TopView::Settings => rsx! {
                        screens::settings::View { vm: v.clone(), close }
                    },
                }
            }
        };
    }

    let Some(project) = v.open.clone() else {
        let b = bridge.clone();
        return rsx! {
            GlobalChrome {}
            div {
                style: "min-height:100vh;background:{theme::PAPER};font-family:{theme::SANS};color:{theme::INK};",
                screens::wall::View {
                    vm: v.clone(),
                    bridge: b.clone(),
                    go_top: move |tv| top_view.set(Some(tv)),
                }
            }
        };
    };

    let b_nav = bridge.clone();
    let cur = *panel.read();
    rsx! {
        GlobalChrome {}
        div {
            style: "min-height:100vh;background:{theme::PAPER};font-family:{theme::SANS};color:{theme::INK};\
                    display:flex;flex-direction:column;",
            // ── 项目内顶栏:六入口 ─────────────────────────
            div {
                style: "display:flex;align-items:center;gap:6px;padding:10px 18px;\
                        background:{theme::RAIL_BG};border-bottom:1px solid {theme::BORDER};",
                button {
                    style: "{theme::btn_ghost()}margin-right:10px;",
                    onclick: move |_| b_nav.send(Req::Open(None)),
                    "← 项目墙"
                }
                div {
                    style: "font-family:{theme::SERIF};font-size:15px;margin-right:16px;",
                    "{project.name}"
                }
                for p in Panel::ALL {
                    button {
                        key: "{p:?}",
                        style: if p == cur {
                            format!("cursor:pointer;border:none;border-radius:8px;padding:7px 14px;font-size:13px;background:{};color:#FFF;", theme::CLAY)
                        } else {
                            format!("cursor:pointer;border:none;border-radius:8px;padding:7px 14px;font-size:13px;background:transparent;color:{};", theme::INK_2)
                        },
                        onclick: move |_| panel.set(p),
                        "{p.label()}"
                    }
                }
            }
            // ── 仓文件读不动的实话 ─────────────────────────
            for w in v.warnings.iter() {
                div {
                    key: "{w}",
                    style: "padding:8px 18px;background:#F6E7E2;border-bottom:1px solid {theme::BORDER};\
                            font-size:12px;color:{theme::ALERT_DEEP};",
                    "{w}"
                }
            }
            // ── 后台动作回执 ───────────────────────────────
            if let Some(note) = &v.note {
                div {
                    style: "padding:8px 18px;background:{theme::CARD_ALT};border-bottom:1px solid {theme::BORDER};\
                            font-size:12px;color:{theme::INK_2};",
                    "{note}"
                }
            }
            div {
                style: "flex:1;min-height:0;overflow:auto;padding:20px 24px 40px;\
                        display:flex;flex-direction:column;",
                {screens::route(cur, &project, &bridge)}
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
                for s in project.sessions.iter().filter(|s| s.live || project.session_open == Some(s.issue_id)) {
                    TerminalWidget {
                        key: "{s.conversation_id:?}",
                        conversation_id: s.conversation_id,
                        focused: cur == Panel::Session
                            && project.session_open == Some(s.issue_id)
                            && project.workbench.tab == SessionTab::Terminal,
                        bridge: bridge.clone(),
                    }
                }
            }
        }
    }
}
