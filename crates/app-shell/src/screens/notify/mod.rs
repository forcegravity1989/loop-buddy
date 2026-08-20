//! 项目内 · 通知。**结构照 `hifi/index.html` 的 `renderNotify` 排**:上半是
//! 「待处理」——真的需要人动手的事;下半是「事件」——最近发生了什么。
//!
//! 两条如实:
//!
//! 1. **事件流没有事件表**。它是从四张表里现算的(活什么时候建的、什么时候
//!    结清的、会话什么时候开的)。存不下来的事就不在流里,不补一条假的。
//! 2. **不做已读未读的账本**。「看到哪个时间点」只是一个 key/value,不参与
//!    任何计数。

use crate::bridge::{Bridge, Panel, PanelNav, Req};
use crate::vm::{CardItemVm, ProjectVm};
use bw_v4::command::Command;
use bw_v4::model::{IssueId, IssueStatus};
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let nav = use_context::<PanelNav>();
    let (p, bridge) = (&p, &bridge);
    let b_seen = bridge.clone();
    let b_cfg = bridge.clone();
    let pid = p.id;
    let n = &p.notify;
    let pending = n.in_review.len() + n.blocked.len();
    let chat_unset = p.card.chat == "未配";
    rsx! {
        section { style: "max-width:820px;",
            div { style: "display:flex;align-items:baseline;gap:12px;margin-bottom:6px;",
                h1 { style: "font-size:20px;margin:0;", "通知" }
                div { class: "spacer" }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| b_seen.cmd(Command::MarkNotifySeen {
                        project_id: pid,
                        at: time::OffsetDateTime::now_utc().unix_timestamp(),
                    }),
                    "标记已读到现在"
                }
            }
            if chat_unset {
                div { style: "font-size:11.5px;color:var(--ink-3);margin-bottom:10px;",
                    "配了项目群,这些通知能直达群 → "
                    span {
                        style: "color:var(--clay);cursor:pointer;",
                        onclick: move |_| {
                            let _ = &b_cfg;
                            nav.go(Panel::Config);
                        },
                        "配置"
                    }
                }
            }

            div { class: "notify-sect-title", "待处理 · {pending}" }
            div { class: "notify-list",
                if pending == 0 {
                    div { class: "drawer-empty", "待处理已清空" }
                }
                for c in n.in_review.iter() {
                    {review_item(c, bridge, nav)}
                }
                for c in n.blocked.iter() {
                    {blocked_item(c, bridge, nav)}
                }
            }

            div { class: "notify-sect-title", "事件" }
            div { class: "notify-events",
                if n.events.is_empty() {
                    div { class: "drawer-empty", "暂无事件" }
                }
                for (i, e) in n.events.iter().enumerate() {
                    {event_row(i, e, bridge, nav)}
                }
            }
            div { style: "font-size:10.5px;color:var(--ink-4);margin-top:10px;line-height:1.8;",
                "事件流是从库里那四张表现算的:活什么时候建的、什么时候结清的、会话\
                 什么时候开的。存不下来的事(某一次运行失败、某条群消息发没发出去)\
                 不在这条流里 —— 少一条,不编一条。"
                br {}
                "只列最近 80 条。更早的事没有消失,是这条流没往下翻 —— 要查更早的\
                 直接查库(issue 的 created_at / settled_at、claude_conversation 的 created_at)。"
            }
        }
    }
}

/// 等人合入的活。「合入并完成」先真的把 MR 合了,再把活推到完成 —— 合入没成
/// 就整条不算数,活留在原地可以重试。
fn review_item(c: &CardItemVm, bridge: &Bridge, nav: PanelNav) -> Element {
    let (b_open, b_done, b_merge) = (bridge.clone(), bridge.clone(), bridge.clone());
    let id = c.id;
    rsx! {
        div { key: "{c.id:?}", class: "drawer-item",
            div { class: "desc",
                "#{c.number} {c.title}"
                span { class: "chip", style: "margin-left:6px;", "评审中" }
                span { class: "chip chip-gray", style: "margin-left:4px;", "{c.category}" }
            }
            div { class: "acts",
                button {
                    class: "btn btn-sm btn-ghost",
                    onclick: move |_| {
                        b_open.send(Req::SelectSession(Some(id)));
                        nav.go(Panel::Session);
                    },
                    "打开"
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| b_done.cmd(Command::TransitionIssue {
                        id,
                        to: IssueStatus::Done,
                    }),
                    "只标完成"
                }
                button {
                    class: "btn btn-sm btn-primary",
                    onclick: move |_| b_merge.cmd(Command::MergeAndSettle { id }),
                    "合入并完成"
                }
            }
        }
    }
}

fn blocked_item(c: &CardItemVm, bridge: &Bridge, nav: PanelNav) -> Element {
    let (b_open, b_back) = (bridge.clone(), bridge.clone());
    let id = c.id;
    rsx! {
        div { key: "{c.id:?}", class: "drawer-item",
            div { class: "desc",
                "#{c.number} {c.title}"
                span { class: "chip chip-red", style: "margin-left:6px;", "卡住了" }
            }
            div { style: "font-size:11px;color:var(--ink-3);margin-bottom:6px;",
                "如实停在原地,可以重试。卡在哪写在活的说明里。"
            }
            div { class: "acts",
                button {
                    class: "btn btn-sm btn-ghost",
                    onclick: move |_| {
                        b_open.send(Req::SelectSession(Some(id)));
                        nav.go(Panel::Session);
                    },
                    "打开"
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| b_back.cmd(Command::TransitionIssue {
                        id,
                        to: IssueStatus::Todo,
                    }),
                    "解除阻塞 → 待办"
                }
            }
        }
    }
}

fn event_row(i: usize, e: &crate::vm::NotifyEventVm, bridge: &Bridge, nav: PanelNav) -> Element {
    let b = bridge.clone();
    let id: Option<IssueId> = e.issue;
    rsx! {
        div {
            key: "{i}",
            class: "notify-event",
            style: if id.is_some() { "cursor:pointer;" } else { "" },
            onclick: move |_| {
                if let Some(id) = id {
                    b.send(Req::SelectSession(Some(id)));
                    nav.go(Panel::Session);
                }
            },
            span { class: "mono", style: "color:var(--ink-3);flex:none;", "{e.time}" }
            span { style: "flex:1;",
                "{e.text}"
                if e.done {
                    span { class: "chip chip-green", style: "margin-left:6px;", "已处理" }
                }
            }
        }
    }
}
