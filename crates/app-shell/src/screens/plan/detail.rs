//! 右侧 360px 详情抽屉。卡面上不放按钮,动作全在这里。
//!
//! **这是一张活的落脚点**:通知屏说「有 MR 等你合」,点过来落在这儿。所以这里
//! 得能一次看全 —— 活要干什么(正文)、干到哪(状态、分支)、干出了什么
//! (MR 链接、远端 issue 链接)、以及怎么去看过程(去会话)。链接只在真推得出
//! 地址时才给,推不出就说明为什么,不摆一个点了报错的链接。

use super::kanban::PendingMove;
use crate::bridge::{Bridge, Panel, PanelNav, Req};
use crate::vm::ProjectVm;
use bw_v4::command::Command;
use bw_v4::model::{IssueId, IssueStatus};
use dioxus::prelude::*;

/// **按 id 从最新的 ViewModel 里现查**,不拿点卡片那一刻的快照 —— 拿快照的
/// 话,▶开工 之后活已经到「评审中」了,面板还显示「▶ 开工」,再点一次只会
/// 收到一句「这张活现在不是能开工的状态」。
#[component]
pub fn DetailPanel(
    p: ProjectVm,
    mut selected: Signal<Option<IssueId>>,
    /// 「✓ 点完成」不直接发命令,而是把这一下**塞进和拖卡片同一个确认框**。
    /// 两个入口发的是同一条命令,拦的也该是同一下 —— 一个有拦、一个没拦,
    /// 就成了「哪个按钮危险取决于你从哪儿点进来」。
    mut pending: Signal<Option<PendingMove>>,
    bridge: Bridge,
) -> Element {
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
    let b_block = bridge.clone();
    let b_sess = bridge.clone();
    let nav = use_context::<PanelNav>();
    let id = c.id;
    // 先快照出来:`c` 是借来的,闭包要活到点击那一刻。
    let title = c.title.clone();
    let pr = c.pr_number;
    // 链接只在**真推得出地址**时才有。`browse_base` 是从 `.git/config` 的 origin
    // 推的,没有 origin(本机仓)就是空串,那时候只显示号码、不给链接。
    let base = p.browse_base.trim().trim_end_matches('/');
    let is_gh = base.contains("github.com");
    let mr_url = (!base.is_empty() && c.pr_number > 0).then(|| {
        if is_gh {
            format!("{base}/pull/{}", c.pr_number)
        } else {
            // codehub 是 GitLab 那一系,MR 与 issue 都在 `/-/` 底下。
            format!("{base}/-/merge_requests/{}", c.pr_number)
        }
    });
    let issue_url = (!base.is_empty() && c.remote_number > 0).then(|| {
        if is_gh {
            format!("{base}/issues/{}", c.remote_number)
        } else {
            format!("{base}/-/issues/{}", c.remote_number)
        }
    });
    rsx! {
        div { class: "plan-detail",
            div { class: "plan-detail-head",
                h3 { "详情 · #{c.number}" }
                button { class: "drawer-close", onclick: move |_| selected.set(None), "✕" }
            }
            div { class: "plan-detail-body",
                div { class: "detail-title", "{c.title}" }
                if !c.body.trim().is_empty() {
                    div { class: "detail-body-text", "{c.body}" }
                }
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
                {row("分支", if c.branch.is_empty() { "—(还没开过工)" } else { &c.branch })}
                {link_row("MR", c.pr_number, mr_url.as_deref(), "还没开 MR")}
                {link_row("远端 issue", c.remote_number, issue_url.as_deref(), "只在本机,没建远端 issue")}

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
                            // 「✓ 点完成」和看板上把卡片拖进「已完成」是同一条
                            // 路:同一个确认框、同一条命令 —— 两个入口不能一个
                            // 合一个不合,也不能一个拦一下一个不拦。
                            IssueStatus::InReview => rsx! {
                                button {
                                    class: "btn btn-sm btn-primary",
                                    onclick: move |_| pending.set(Some(PendingMove {
                                        id,
                                        title: title.clone(),
                                        from: IssueStatus::InReview,
                                        to: IssueStatus::Done,
                                        pr_number: pr,
                                    })),
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

/// 一行「号码 + 能点的链接」。没号码就说没有;有号码但推不出地址(本机仓、
/// origin 认不出)就只显示号码,并说明为什么点不了 —— 不摆一个点了报错的链接。
fn link_row(k: &str, number: u32, url: Option<&str>, none_hint: &str) -> Element {
    let opened = url.map(str::to_string);
    rsx! {
        div { class: "detail-row",
            div { class: "k", "{k}" }
            if number == 0 {
                span { style: "color:var(--ink-3);", "—({none_hint})" }
            } else if let Some(u) = opened {
                span {
                    class: "detail-link",
                    title: "{u}",
                    onclick: move |_| crate::chrome::open_in_browser(&u),
                    "#{number} ↗"
                }
            } else {
                span {
                    title: "这个仓的 origin 推不出能点的地址(本机仓,或者地址写法认不出来)",
                    "#{number}(没有可点的地址)"
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
