//! 外壳骨架:图标轨 / 项目轨 / 顶栏 / 指南抽屉 / toast。
//!
//! 结构照 `docs/v4-prototype/hifi/index.html` 的 `iconRailHTML` /
//! `projectRailHTML` / `topbarHTML` / `guideMarkupHTML` 搬,类名同名同义,
//! **不写行内样式**。
//!
//! 与原型的两处不同,都是「原型是演示、真壳没有这东西」造成的,如实改掉而不
//! 是照抄一个假的:
//!
//! - 原型顶栏右边是「演示数据 ✱ / 重置 / 头像」。真壳的数据来自本机库和项目
//!   仓,没有可重置的演示数据,也没有账号 —— 改成「数据来自哪个库文件」+
//!   一颗「刷新」(真的重算一遍 ViewModel)。头像先不放,放了就是装饰。
//! - 原型的 toast 2.6 秒自动消失。真壳这条位置放的是命令回执(建活成功、
//!   铺底失败的原话),自动消失等于把失败信息藏起来 —— 改成点 ✕ 才关。

use crate::bridge::{Bridge, Panel, Req, TopView};
use crate::theme;
use bw_v4::model::Signal;
use dioxus::prelude::*;

/// 原型那套线性图标。内容是写死的常量,没有任何外部输入拼进去。
fn icon_path(name: &str) -> &'static str {
    match name {
        "wall" => {
            r#"<rect x="3" y="3" width="7" height="7" rx="1.4"/><rect x="14" y="3" width="7" height="7" rx="1.4"/><rect x="3" y="14" width="7" height="7" rx="1.4"/><rect x="14" y="14" width="7" height="7" rx="1.4"/>"#
        }
        "settings" => {
            r#"<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 01-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 010-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 112.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 014 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 010 4h-.09a1.65 1.65 0 00-1.51 1z"/>"#
        }
        "overview" => r#"<circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="2.6"/>"#,
        "plan" => r#"<path d="M5 6h14M5 12h14M5 18h9"/>"#,
        "session" => {
            r#"<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M7 9l3 3-3 3M13 15h4"/>"#
        }
        "config" => {
            r#"<path d="M4 7h9M17 7h3M4 12h3M9 12h11M4 17h13M19 17h1"/><circle cx="15" cy="7" r="2"/><circle cx="6" cy="12" r="2"/><circle cx="17" cy="17" r="2"/>"#
        }
        "space" => {
            r#"<path d="M4 6a2 2 0 012-2h6l2 2h6a2 2 0 012 2v9a2 2 0 01-2 2H6a2 2 0 01-2-2V6z"/><path d="M4 9h16"/>"#
        }
        "back" => r#"<path d="M15 5l-7 7 7 7"/>"#,
        "notify" => {
            r#"<path d="M6 8a6 6 0 0112 0c0 4 1.6 5.4 1.6 5.4H4.4S6 12 6 8z"/><path d="M9.5 17a2.5 2.5 0 005 0"/>"#
        }
        _ => "",
    }
}

pub fn icon(name: &str) -> Element {
    rsx! {
        svg {
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.7",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            dangerous_inner_html: "{icon_path(name)}",
        }
    }
}

pub fn light_dot(signal: Option<Signal>, big: bool) -> Element {
    let cls = if big { "dot dot-lg" } else { "dot" };
    let color = theme::signal_color(signal);
    rsx! { span { class: "{cls}", style: "background:{color}" } }
}

/// 六个入口各自的图标名。原型里知识库那一格用的是 `space`(内部沿用旧命名)。
fn panel_icon(p: Panel) -> &'static str {
    match p {
        Panel::Overview => "overview",
        Panel::Plan => "plan",
        Panel::Session => "session",
        Panel::Notify => "notify",
        Panel::Config => "config",
        Panel::Kb => "space",
    }
}

#[component]
pub fn IconRail(
    in_project: bool,
    top_view: Option<TopView>,
    on_wall: EventHandler<()>,
    on_settings: EventHandler<()>,
) -> Element {
    let settings_active = top_view == Some(TopView::Settings);
    let wall_active = !in_project && top_view.is_none();
    rsx! {
        div { class: "iconrail",
            div { class: "rail-logo", "B" }
            if in_project {
                button {
                    class: "rail-btn",
                    title: "项目墙",
                    onclick: move |_| on_wall.call(()),
                    {icon("back")}
                }
            } else {
                button {
                    class: if wall_active { "rail-btn active" } else { "rail-btn" },
                    title: "项目墙",
                    onclick: move |_| on_wall.call(()),
                    {icon("wall")}
                }
            }
            div { class: "spacer" }
            button {
                class: if settings_active { "rail-btn active" } else { "rail-btn" },
                title: "设置",
                onclick: move |_| on_settings.call(()),
                {icon("settings")}
            }
        }
    }
}

#[component]
pub fn ProjectRail(
    name: String,
    version: String,
    signal: Option<Signal>,
    unread: u32,
    cur: Panel,
    on_wall: EventHandler<()>,
    on_nav: EventHandler<Panel>,
) -> Element {
    rsx! {
        div { class: "projectrail",
            div { class: "pr-back", onclick: move |_| on_wall.call(()),
                {icon("back")}
                span { "项目墙" }
            }
            div { class: "pr-head",
                div { class: "pr-head-name", title: "{name}", "{name}" }
                div { class: "pr-head-meta",
                    span { "{version}" }
                    {light_dot(signal, false)}
                }
            }
            nav { class: "pr-nav",
                for p in Panel::ALL {
                    div {
                        key: "{p:?}",
                        class: if p == cur { "pr-link active" } else { "pr-link" },
                        onclick: move |_| on_nav.call(p),
                        span { style: "width:15px;height:15px;display:flex;", {icon(panel_icon(p))} }
                        span { "{p.label()}" }
                        if p == Panel::Notify && unread > 0 {
                            span { class: "pr-badge", "{unread}" }
                        }
                    }
                }
            }
        }
    }
}

/// 顶栏。`source` 是「这一屏的数字从哪来」的实话 —— 原型那格写的是
/// 「演示数据 ✱」,真壳写库文件名。
#[component]
pub fn TopBar(title: String, project: Option<String>, source: String, bridge: Bridge) -> Element {
    let b = bridge.clone();
    rsx! {
        div { class: "topbar",
            div { class: "topbar-title",
                "{title}"
                if let Some(p) = project.as_ref() {
                    " · {p}"
                }
            }
            div { class: "spacer" }
            span { class: "demo-tag", title: "本机库文件;界面上的数字都能用 sqlite3 直接查这个库核对", "{source}" }
            button {
                class: "btn btn-sm btn-ghost",
                onclick: move |_| b.send(Req::Refresh),
                "刷新"
            }
        }
    }
}

struct Chapter {
    id: &'static str,
    label: &'static str,
    body: &'static str,
}

/// 指南正文照搬原型,只把「演示 / 留位」的说法对齐真壳现状。
const CHAPTERS: [Chapter; 4] = [
    Chapter {
        id: "env",
        label: "环境准备",
        body: "claude CLI、Open Design、codehub-cli 三项本机环境按需安装,缺一项不影响开工,只影响对应类别的活能不能开工。「测一下」会真实探活,结果记进健康信号,不假装绿。welink-cli 的探活还没实现,先留位显示灰色无数据,不会因为点了「测一下」就变绿。",
    },
    Chapter {
        id: "onboard",
        label: "接入项目",
        body: "接入只问两件事:仓在哪、这个项目想做什么。规范铺底会在后台自动建分支、写核心件、提 MR,评审中合入即可,不用手填目录或分支。",
    },
    Chapter {
        id: "cycle",
        label: "一周一圈",
        body: "每周由「更新指标与制定本周计划」这张运作活开场:复盘上周 → 更新指标 → 引导出本周目标与活清单。活从待办池推到评审中都是 agent 干的,「完成」永远由你在看板上点一下。",
    },
    Chapter {
        id: "faq",
        label: "常见问题",
        body: "常见问题会随实际使用逐步收进这里。当前先看前三章;遇到没写清楚的地方,用下面的「问题上报」占位记一笔(还没接真实上报,点了不会发出去)。",
    },
];

#[component]
pub fn GuideDrawer() -> Element {
    // 开在哪一章由外壳级的共享信号说了算 —— 项目墙那条「怎么处理 →」要能把
    // 抽屉直接翻到环境那一章。
    let guide = use_context::<crate::bridge::GuideNav>();
    let Some(cur_id) = *guide.0.read() else {
        return rsx! {
            div { class: "guide-tab", onclick: move |_| guide.open("env"), "指南" }
        };
    };
    let cur = CHAPTERS
        .iter()
        .find(|c| c.id == cur_id)
        .unwrap_or(&CHAPTERS[0]);
    rsx! {
        div { class: "guide-panel",
            div { class: "guide-panel-head",
                h3 { style: "font-size:15px;margin:0;", "指南" }
                button { class: "drawer-close", onclick: move |_| guide.close(), "✕" }
            }
            div { class: "guide-chapters",
                for c in CHAPTERS.iter() {
                    div {
                        key: "{c.id}",
                        class: if c.id == cur_id { "guide-chapter active" } else { "guide-chapter" },
                        onclick: move |_| guide.open(c.id),
                        "{c.label}"
                    }
                }
            }
            div { class: "guide-body", "{cur.body}" }
            div { class: "guide-report", title: "还没接真实上报", "⚑ 问题上报" }
        }
    }
}

/// 命令回执。**不自动消失** —— 这里放的是「铺底失败,原话是……」这种话。
#[component]
pub fn Toast(note: Option<String>) -> Element {
    let mut dismissed = use_signal(String::new);
    let Some(text) = note else {
        return rsx! {};
    };
    if *dismissed.read() == text {
        return rsx! {};
    }
    let t = text.clone();
    rsx! {
        div { class: "toast show",
            "{text}"
            span {
                style: "margin-left:10px;cursor:pointer;opacity:.6;",
                onclick: move |_| dismissed.set(t.clone()),
                "✕"
            }
        }
    }
}
