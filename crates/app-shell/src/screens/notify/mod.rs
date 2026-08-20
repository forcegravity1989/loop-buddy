//! 项目内 · 通知。上半是「待处理」,下半是「事件」——最近发生了什么。
//!
//! **通知只有一类:有 MR 等你合入。** 试点第一天定的边界:通知就该是「有件事
//! 非你不可、而且现在就能做」。「活阻塞了」「agent 停下来等你回话」这些是**状态**
//! ——该在计划屏和会话屏上看见,不该也来占通知位;尤其「等你回话」那种,真要
//! 提醒也该是系统级的弹窗,不是一个你得先点进来才看得到的列表。那些等实践清楚了
//! 单独设计,现在**不摆冗余的位、不留冗余的代码**(阻塞那一段连同它的按钮已经
//! 整段删掉,不是注释掉)。
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
use bw_v4::model::IssueId;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let nav = use_context::<PanelNav>();
    let (p, bridge) = (&p, &bridge);
    let b_seen = bridge.clone();
    let b_cfg = bridge.clone();
    let pid = p.id;
    let n = &p.notify;
    let pending = n.to_merge.len();
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

            div { class: "notify-sect-title", "等你合入 · {pending}" }
            div { class: "notify-list",
                if pending == 0 {
                    div { class: "drawer-empty", "没有等你合入的 MR" }
                }
                for c in n.to_merge.iter() {
                    {review_item(c, bridge, nav)}
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
/// 就整条不算数,活留在原地可以重试。库里没记 MR 号的,内核会拿这张活的分支
/// 去远端现查一次(队友自己开的 MR 就这么找到);确实没有 MR 可合的(项目没
/// 挂远端),只走「完成」那一步,事件里如实说没合。
fn review_item(c: &CardItemVm, bridge: &Bridge, nav: PanelNav) -> Element {
    let (b_open, b_merge) = (bridge.clone(), bridge.clone());
    let id = c.id;
    rsx! {
        div { key: "{c.id:?}", class: "drawer-item",
            div { class: "desc",
                "#{c.number} {c.title}"
                span { class: "chip chip-clay", style: "margin-left:6px;", "MR #{c.pr_number}" }
                span { class: "chip chip-gray", style: "margin-left:4px;", "{c.category}" }
            }
            div { style: "font-size:11px;color:var(--ink-3);margin-bottom:6px;",
                "先看一眼产出它的那场会话再决定合不合。"
            }
            div { class: "acts",
                button {
                    class: "btn btn-sm btn-ghost",
                    // 落到这张活的会话屏,并**把上次那场对话接回来** —— 光切
                    // 视图不接回,人看到的是一片空白。
                    onclick: move |_| {
                        b_open.send(Req::SelectSession(Some(id)));
                        nav.go(Panel::Session);
                    },
                    "看会话"
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
