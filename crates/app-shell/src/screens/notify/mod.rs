//! 项目内 · 通知。只有两类真的需要人动手的事:等人合入的、卡住的。
//!
//! 不做收件箱、不做已读未读的复杂账本——「看到哪个时间点」只是一个 key/value,
//! 不参与任何计数。

use crate::bridge::Bridge;
use crate::theme;
use crate::vm::{CardItemVm, ProjectVm};
use bw_v4::command::Command;
use bw_v4::model::IssueStatus;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    let b = bridge.clone();
    let pid = p.id;
    let n = &p.notify;
    rsx! {
        div {
            style: "max-width:860px;margin:0 auto;",
            div {
                style: "display:flex;align-items:baseline;gap:12px;margin-bottom:18px;",
                div { style: "font-family:{theme::SERIF};font-size:20px;", "通知" }
                div { style: "flex:1;" }
                button {
                    style: "{theme::btn_ghost()}",
                    onclick: move |_| b.cmd(Command::MarkNotifySeen {
                        project_id: pid,
                        at: time::OffsetDateTime::now_utc().unix_timestamp(),
                    }),
                    "标记已读到现在"
                }
            }

            {section(
                "等人评审合入",
                "跑完的活最远只到这一步。「合入并完成」先真的把 MR 合了,再把活推到完成 —— \
                 合入没成就整条不算数,活留在原地可以重试。没挂远端的项目只走「完成」那一步。",
                &n.in_review,
                true,
                bridge,
            )}
            div { style: "height:16px;" }
            {section(
                "卡住了",
                "如实停在原地,可以重试。卡在哪写在活的说明里。",
                &n.blocked,
                false,
                bridge,
            )}
        }
    }
}

fn section(
    title: &str,
    hint: &str,
    items: &[CardItemVm],
    mergeable: bool,
    bridge: &Bridge,
) -> Element {
    rsx! {
        div {
            style: "{theme::card()}padding:18px 20px;",
            div {
                style: "display:flex;align-items:baseline;gap:8px;margin-bottom:4px;",
                div { style: "font-family:{theme::SERIF};font-size:16px;", "{title}" }
                div { style: "font-size:12px;color:{theme::INK_4};", "{items.len()}" }
            }
            div { style: "font-size:12px;color:{theme::INK_3};margin-bottom:12px;", "{hint}" }
            if items.is_empty() {
                div { style: "font-size:13px;color:{theme::INK_4};padding:6px 0;", "没有。" }
            }
            for c in items.iter() {
                {row(c, mergeable, bridge)}
            }
        }
    }
}

fn row(c: &CardItemVm, mergeable: bool, bridge: &Bridge) -> Element {
    let (b, b2) = (bridge.clone(), bridge.clone());
    let id = c.id;
    rsx! {
        div {
            key: "{c.id:?}",
            style: "display:flex;align-items:center;gap:10px;padding:10px 0;\
                    border-top:1px solid {theme::BORDER};",
            span { style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_4};", "#{c.number}" }
            span { style: "font-size:13px;flex:1;", "{c.title}" }
            span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{c.category}" }
            if mergeable {
                button {
                    style: "{theme::btn_ghost()}padding:6px 12px;font-size:12px;",
                    onclick: move |_| b.cmd(Command::TransitionIssue {
                        id,
                        to: IssueStatus::Done,
                    }),
                    "只标完成"
                }
                button {
                    style: "{theme::btn_primary()}padding:6px 14px;font-size:12px;",
                    onclick: move |_| b2.cmd(Command::MergeAndSettle { id }),
                    "合入并完成"
                }
            }
        }
    }
}
