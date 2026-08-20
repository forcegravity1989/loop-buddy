//! 右侧 360px 详情抽屉。卡面上不放按钮,动作全在这里。

use crate::bridge::{Bridge, Panel, PanelNav, Req};
use crate::vm::ProjectVm;
use bw_v4::command::Command;
use bw_v4::model::{IssueId, IssueStatus};
use dioxus::prelude::*;

/// **按 id 从最新的 ViewModel 里现查**,不拿点卡片那一刻的快照 —— 拿快照的
/// 话,▶开工 之后活已经到「评审中」了,面板还显示「▶ 开工」,再点一次只会
/// 收到一句「这张活现在不是能开工的状态」。
#[component]
pub fn DetailPanel(p: ProjectVm, mut selected: Signal<Option<IssueId>>, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    let picked = selected.read().and_then(|id| {
        p.board
            .columns
            .iter()
            .flat_map(|col| col.cards.iter())
            .find(|c| c.id == id)
            .cloned()
    });
    let Some(c) = picked else {
        return rsx! {};
    };
    let b_run = bridge.clone();
    let b_review = bridge.clone();
    let b_done = bridge.clone();
    let b_block = bridge.clone();
    let b_sess = bridge.clone();
    let nav = use_context::<PanelNav>();
    let id = c.id;
    rsx! {
        div { class: "plan-detail",
            div { class: "plan-detail-head",
                h3 { "详情 · #{c.number}" }
                button { class: "drawer-close", onclick: move |_| selected.set(None), "✕" }
            }
            div { class: "plan-detail-body",
                div { class: "detail-title", "{c.title}" }
                {row("状态", c.status.label())}
                {row("类别", &c.category)}
                {row("开工工具", if c.tool.is_empty() { "—" } else { &c.tool })}
                {row("workflow", if c.workflow.is_empty() { "—" } else { &c.workflow })}
                {row("种类", &c.kind)}
                {row("来源", &c.origin)}
                {row("排在", if c.week_of.is_empty() { "待办池" } else { &c.week_of })}
                {row("版本", if c.version.is_empty() { "—" } else { &c.version })}
                {row("推的指标", if c.metric_key.is_empty() { "—" } else { &c.metric_key })}
                {row("结清", if c.settled { "已结清(只结这一次)" } else { "未结清" })}

                div { class: "detail-row",
                    div { class: "k", "操作" }
                    div { class: "kcard-actions", style: "margin-top:3px;",
                        // 同一个位置按状态互斥切换,不堆一排常驻按钮。
                        match c.status {
                            IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress => rsx! {
                                button {
                                    class: "btn btn-sm btn-primary",
                                    onclick: move |_| b_run.cmd(Command::RunIssue { id }),
                                    "▶ 开工"
                                }
                            },
                            IssueStatus::InReview => rsx! {
                                button {
                                    class: "btn btn-sm btn-primary",
                                    onclick: move |_| b_done.cmd(Command::TransitionIssue {
                                        id,
                                        to: IssueStatus::Done,
                                    }),
                                    "✓ 点完成"
                                }
                            },
                            IssueStatus::Done => rsx! {
                                span { style: "color:var(--ink-3);", "已完成并结清" }
                            },
                            IssueStatus::Blocked => rsx! {
                                button {
                                    class: "btn btn-sm",
                                    onclick: move |_| b_review.cmd(Command::TransitionIssue {
                                        id,
                                        to: IssueStatus::Todo,
                                    }),
                                    "解除阻塞 → 待办"
                                }
                            },
                            IssueStatus::Cancelled => rsx! {
                                span { style: "color:var(--ink-3);", "已取消" }
                            },
                        }
                        if !matches!(
                            c.status,
                            IssueStatus::Done | IssueStatus::Blocked | IssueStatus::Cancelled
                        ) {
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| b_block.cmd(Command::BlockIssue {
                                    id,
                                    reason: "在计划屏手动标为阻塞".into(),
                                }),
                                "⛔ 阻塞"
                            }
                        }
                    }
                }
                if c.status == IssueStatus::InReview {
                    div { style: "font-size:11px;color:var(--ink-4);line-height:1.8;",
                        "「完成」永远是人点的。跑完的活最远只到这一步。"
                    }
                }
                div { class: "detail-actions",
                    button {
                        class: "btn btn-primary btn-sm",
                        onclick: move |_| {
                            b_sess.send(Req::SelectSession(Some(id)));
                            nav.go(Panel::Session);
                        },
                        "去会话 →"
                    }
                    // 高保真上这里还有一颗「蒸馏」:把做完的活变成一篇技能。
                    // V4 还没有这条命令(见 docs/LEFTOVERS.md),做成灰态,
                    // 不放一颗点了没反应的按钮。
                    button { class: "btn btn-sm", disabled: true, title: "V4 还没接蒸馏", "蒸馏" }
                }
            }
        }
    }
}

fn row(k: &str, v: &str) -> Element {
    rsx! {
        div { class: "detail-row",
            div { class: "k", "{k}" }
            "{v}"
        }
    }
}
