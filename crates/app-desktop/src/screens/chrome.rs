//! Global chrome: the 64px icon rail (always visible), hub placeholders, boot
//! and fatal frames, the error toast.

use crate::theme;
use dioxus::prelude::*;

/// The ten rail destinations (prototype `hub` values).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hub {
    Workspace,
    Skill,
    Agent,
    Workflow,
    Cron,
    Connector,
    Knowledge,
    Activity,
    Notify,
    Settings,
}

impl Hub {
    pub fn label(self) -> &'static str {
        match self {
            Hub::Workspace => "工作台",
            Hub::Skill => "SkillHub",
            Hub::Agent => "AgentHub",
            Hub::Workflow => "Routines",
            Hub::Cron => "CronHub",
            Hub::Connector => "Connectors",
            Hub::Knowledge => "Knowledge",
            Hub::Activity => "Activity",
            Hub::Notify => "通知",
            Hub::Settings => "设置",
        }
    }

    /// Stroke-based inline SVG path(s), 24×24 box (inventory §6).
    fn icon_paths(self) -> &'static str {
        match self {
            Hub::Workspace => "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
            Hub::Skill => "M12 3l7 9-7 9-7-9z",
            Hub::Agent => {
                "M7 8h10v8a2 2 0 01-2 2H9a2 2 0 01-2-2zM9 8V6a3 3 0 016 0v2M10 13h.01M14 13h.01"
            }
            Hub::Workflow => "M6 6h.01M18 6h.01M12 18h.01M6.5 6.5L11.5 17M17.5 6.5L12.5 17M7 6h10",
            Hub::Cron => "M12 21a9 9 0 100-18 9 9 0 000 18zM12 7v5l3 3",
            Hub::Connector => "M7 9v6M17 9v6M7 12h10M4 10v4M20 10v4",
            Hub::Knowledge => "M5 5a2 2 0 012-2h12v18H7a2 2 0 01-2-2zM19 3v18M9 7h6M9 11h6",
            Hub::Activity => "M3 12h4l2-6 4 12 2-6h6",
            Hub::Notify => "M6 9a6 6 0 1112 0v5l2 3H4l2-3zM10 20a2 2 0 004 0",
            Hub::Settings => "M4 9h16M4 15h16",
        }
    }
}

pub const RAIL_HUBS: [Hub; 10] = [
    Hub::Workspace,
    Hub::Skill,
    Hub::Agent,
    Hub::Workflow,
    Hub::Cron,
    Hub::Connector,
    Hub::Knowledge,
    Hub::Activity,
    Hub::Notify,
    Hub::Settings,
];

#[component]
pub fn IconRail(hub: Hub, on_pick: EventHandler<Hub>) -> Element {
    let rail_bg = theme::RAIL_BG;
    let border = theme::BORDER;
    let clay = theme::CLAY;
    let serif = theme::SERIF;
    rsx! {
        div {
            style: "width:64px;flex:none;background:{rail_bg};border-right:1px solid {border};display:flex;flex-direction:column;align-items:center;padding:14px 0;gap:4px;",
            div {
                style: "width:34px;height:34px;border-radius:9px;background:{clay};color:#FFF;display:flex;align-items:center;justify-content:center;font-family:{serif};font-weight:700;font-size:17px;margin-bottom:10px;",
                "B"
            }
            for h in RAIL_HUBS {
                RailIcon { hub: h, current: hub, on_pick }
            }
        }
    }
}

#[component]
fn RailIcon(hub: Hub, current: Hub, on_pick: EventHandler<Hub>) -> Element {
    let active = hub == current;
    let (bg, stroke) = if active {
        ("#DED5C2", theme::INK)
    } else {
        ("transparent", theme::INK_3)
    };
    let paths = hub.icon_paths();
    let title = hub.label();
    rsx! {
        button {
            title: "{title}",
            onclick: move |_| on_pick.call(hub),
            style: "width:40px;height:40px;border:none;border-radius:9px;background:{bg};cursor:pointer;display:flex;align-items:center;justify-content:center;padding:0;",
            svg {
                view_box: "0 0 24 24",
                width: "19",
                height: "19",
                fill: "none",
                stroke: "{stroke}",
                stroke_width: "1.7",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "{paths}" }
            }
        }
    }
}

/// Honest placeholder for hubs that belong to the P3 breadth pass — no mock
/// lists pretending to be data.
#[component]
pub fn HubStub(hub: Hub) -> Element {
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let name = hub.label();
    rsx! {
        div {
            style: "display:flex;align-items:center;justify-content:center;height:100%;",
            div {
                style: "{card} padding:34px 44px;max-width:460px;text-align:center;",
                div { style: "font-family:{serif};font-size:22px;font-weight:600;margin-bottom:10px;", "{name}" }
                p { style: "color:{ink2};font-size:13px;line-height:1.7;margin:0;",
                    "该库属于 P3 · 铺屏阶段的交付范围。当前版本聚焦创建引导流与监控运行流,不展示任何模拟数据。"
                }
                p { style: "color:{ink3};font-size:12px;margin:12px 0 0;", "回到工作台继续 →" }
            }
        }
    }
}

#[component]
pub fn BootFrame() -> Element {
    let ink3 = theme::INK_3;
    rsx! {
        div {
            style: "display:flex;align-items:center;justify-content:center;height:100%;color:{ink3};font-size:13px;",
            "正在打开本地工作台…"
        }
    }
}

#[component]
pub fn FatalFrame(msg: String) -> Element {
    let card = theme::card();
    let red = ui::signal_color(bw_core::Signal::Red);
    rsx! {
        div {
            style: "display:flex;align-items:center;justify-content:center;height:100%;",
            div {
                style: "{card} padding:28px 36px;max-width:520px;",
                div { style: "color:{red};font-weight:600;margin-bottom:8px;", "无法启动" }
                div { style: "font-size:13px;line-height:1.7;", "{msg}" }
            }
        }
    }
}

/// Bottom-center transient error strip.
#[component]
pub fn Toast(msg: String, onclose: EventHandler<()>) -> Element {
    let deep = theme::ALERT_DEEP;
    rsx! {
        div {
            style: "position:fixed;left:50%;transform:translateX(-50%);bottom:22px;background:{deep};color:#FFF;border-radius:9px;padding:10px 14px;font-size:13px;display:flex;align-items:center;gap:12px;box-shadow:0 10px 28px rgba(35,33,28,.25);max-width:70vw;z-index:50;",
            span { "{msg}" }
            button {
                onclick: move |_| onclose.call(()),
                style: "background:transparent;border:none;color:#F3D9CF;cursor:pointer;font-size:13px;padding:0;",
                "关闭"
            }
        }
    }
}
