//! 项目内 · 计划。左栏周列表 + 六列看板。
//!
//! **所有列都能拖**,但两种拖不是一回事:
//!
//! - 拖进/拖出「待办池」是**排期**,直接生效(改的是这张活排在哪一周)。
//! - 拖进进行中 / 评审中 / 已完成 / 阻塞是**状态动作**,松手弹确认框,确认了
//!   才真的发命令;状态机不允许的转移松手即弹回,连确认框都不弹。
//!
//! 卡面上没有按钮 —— 按钮全在右侧详情面板。同一个位置按状态互斥切换文案,
//! 不堆一排常驻按钮。

use crate::bridge::{Bridge, Req};
use crate::theme;
use crate::vm::{CardItemVm, ColumnVm, ProjectVm};
use bw_v4::command::Command;
use bw_v4::model::{IssueId, IssueStatus};
use dioxus::prelude::*;

/// 松手之后等人确认的那一下。
#[derive(Clone, PartialEq)]
struct PendingMove {
    id: IssueId,
    title: String,
    from: IssueStatus,
    to: IssueStatus,
    /// 排期方向要知道排进哪一周 —— 就是左栏正在看的那一周。
    week: String,
}

pub fn view(p: &ProjectVm, bridge: &Bridge) -> Element {
    let dragging = use_signal(|| None::<CardItemVm>);
    let pending = use_signal(|| None::<PendingMove>);
    let selected = use_signal(|| None::<CardItemVm>);
    let bounced = use_signal(String::new);

    rsx! {
        div {
            style: "display:flex;gap:16px;align-items:flex-start;",
            {week_rail(p, bridge)}
            div {
                style: "flex:1;min-width:0;",
                {board_head(p, bridge)}
                if !bounced.read().is_empty() {
                    div {
                        style: "margin-bottom:10px;padding:8px 12px;border-radius:8px;\
                                background:#F6E7E2;color:{theme::ALERT_DEEP};font-size:12px;",
                        "{bounced}"
                    }
                }
                div {
                    style: "display:flex;gap:12px;overflow-x:auto;padding-bottom:12px;",
                    for col in p.board.columns.iter() {
                        {column(col, &p.viewing_week, dragging, pending, selected, bounced)}
                    }
                }
            }
            {detail_panel(selected, bridge)}
        }
        if let Some(pm) = pending.read().clone() {
            {confirm_dialog(pm, pending, bridge)}
        }
    }
}

fn week_rail(p: &ProjectVm, bridge: &Bridge) -> Element {
    rsx! {
        div {
            style: "width:210px;flex:none;{theme::card()}padding:14px;max-height:calc(100vh - 160px);\
                    overflow:auto;",
            div { style: "font-size:12px;color:{theme::INK_3};margin-bottom:10px;", "周" }
            if p.weeks.is_empty() {
                div {
                    style: "font-size:12px;color:{theme::INK_4};line-height:1.8;",
                    "docs/plan/ 里还没有周计划文件。周列表是扫这个目录得到的,没有索引表。"
                }
            }
            for w in p.weeks.iter() {
                {week_row(w, &p.viewing_week, bridge)}
            }
        }
    }
}

fn week_row(w: &crate::vm::WeekVm, viewing: &str, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let week = w.week.clone();
    let active = w.week == viewing;
    let bg = if active {
        theme::CARD_ALT
    } else {
        "transparent"
    };
    rsx! {
        div {
            key: "{w.week}",
            style: "padding:8px 10px;border-radius:8px;cursor:pointer;background:{bg};margin-bottom:2px;",
            onclick: move |_| b.send(Req::ViewWeek(week.clone())),
            div {
                style: "display:flex;align-items:center;gap:6px;",
                div { style: "font-family:{theme::MONO};font-size:12px;", "{w.week}" }
                if w.backfill {
                    span { style: "{theme::chip(theme::CARD, theme::INK_4)}", "回填" }
                }
            }
            div {
                style: "font-size:11px;color:{theme::INK_4};margin-top:2px;",
                "{w.activity_count} 张业务活"
            }
        }
    }
}

fn board_head(p: &ProjectVm, bridge: &Bridge) -> Element {
    let b_new = bridge.clone();
    let b_rel = bridge.clone();
    let b_refresh = bridge.clone();
    let pid = p.id;
    let week = p.viewing_week.clone();
    let week2 = p.viewing_week.clone();
    let version = p.card.current_version.clone();
    let done_ids: Vec<IssueId> = p
        .board
        .columns
        .iter()
        .find(|c| c.status == IssueStatus::Done)
        .map(|c| c.cards.iter().map(|x| x.id).collect())
        .unwrap_or_default();
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:10px;margin-bottom:12px;flex-wrap:wrap;",
            div { style: "font-family:{theme::SERIF};font-size:19px;", "{p.viewing_week}" }
            div { style: "font-size:12px;color:{theme::INK_3};", "在研版本 {p.card.current_version}" }
            div { style: "flex:1;" }
            button {
                style: "{theme::btn_ghost()}",
                onclick: move |_| b_refresh.cmd(Command::RefreshIssueCacheFromPlan {
                    project_id: pid,
                    week: week2.clone(),
                }),
                "按文件刷新"
            }
            button {
                style: "{theme::btn_ghost()}",
                disabled: done_ids.is_empty(),
                onclick: move |_| b_rel.cmd(Command::CutRelease {
                    project_id: pid,
                    version: next_version(&version),
                    note: "本周完成的活".into(),
                    included: done_ids.clone(),
                }),
                "发版本"
            }
            button {
                style: "{theme::btn_primary()}",
                onclick: move |_| b_new.cmd(Command::CreateIssue {
                    project_id: pid,
                    title: format!("新活 · {week}"),
                    body: String::new(),
                    category: Some(bw_v4::model::StageKind::Build),
                    kind: bw_v4::model::IssueKind::Business,
                    origin: bw_v4::model::IssueOrigin::Human,
                    week_of: week.clone(),
                }),
                "新建活"
            }
        }
    }
}

/// `v0.3` → `v0.4`。认不出格式就原样返回 —— 不猜一个版本号出来。
fn next_version(cur: &str) -> String {
    let t = cur.trim().trim_start_matches('v');
    match t.rsplit_once('.') {
        Some((head, tail)) => match tail.parse::<u32>() {
            Ok(n) => format!("v{head}.{}", n + 1),
            Err(_) => cur.to_string(),
        },
        None => cur.to_string(),
    }
}

fn column(
    col: &ColumnVm,
    viewing_week: &str,
    mut dragging: Signal<Option<CardItemVm>>,
    mut pending: Signal<Option<PendingMove>>,
    selected: Signal<Option<CardItemVm>>,
    mut bounced: Signal<String>,
) -> Element {
    let target = col.status;
    let week = viewing_week.to_string();
    let drop = move |_| {
        let Some(card) = dragging.read().clone() else {
            return;
        };
        dragging.set(None);
        if card.status == target {
            return;
        }
        // 待办池 ⇄ 待办 是排期,直接生效,不弹框。
        if matches!(target, IssueStatus::Backlog | IssueStatus::Todo)
            && matches!(card.status, IssueStatus::Backlog | IssueStatus::Todo)
        {
            pending.set(Some(PendingMove {
                id: card.id,
                title: card.title.clone(),
                from: card.status,
                to: target,
                week: week.clone(),
            }));
            return;
        }
        if !card.status.can_transition_to(target) {
            bounced.set(format!(
                "「{}」不能直接从「{}」拖到「{}」——卡片弹回原处,状态没动。",
                card.title,
                card.status.label(),
                target.label()
            ));
            return;
        }
        bounced.set(String::new());
        pending.set(Some(PendingMove {
            id: card.id,
            title: card.title.clone(),
            from: card.status,
            to: target,
            week: week.clone(),
        }));
    };

    rsx! {
        div {
            key: "{col.status:?}",
            style: "width:250px;flex:none;background:{theme::CARD_ALT};border:1px solid {theme::BORDER};\
                    border-radius:10px;padding:10px;min-height:220px;",
            ondragover: move |e| e.prevent_default(),
            ondrop: drop,
            div {
                style: "font-size:12px;color:{theme::INK_2};margin-bottom:8px;display:flex;gap:6px;",
                span { "{col.title}" }
                span { style: "color:{theme::INK_4};", "{col.cards.len()}" }
            }
            for c in col.cards.iter() {
                {card_view(c, dragging, selected)}
            }
            if col.cards.is_empty() {
                div { style: "font-size:11px;color:{theme::INK_4};padding:10px 4px;", "空" }
            }
        }
    }
}

fn card_view(
    c: &CardItemVm,
    mut dragging: Signal<Option<CardItemVm>>,
    mut selected: Signal<Option<CardItemVm>>,
) -> Element {
    let c1 = c.clone();
    let c2 = c.clone();
    rsx! {
        div {
            key: "{c.id:?}",
            draggable: true,
            ondragstart: move |_| dragging.set(Some(c1.clone())),
            ondragend: move |_| dragging.set(None),
            onclick: move |_| selected.set(Some(c2.clone())),
            style: "background:{theme::CARD};border:1px solid {theme::BORDER};border-radius:8px;\
                    padding:10px 11px;margin-bottom:8px;",
            div {
                style: "display:flex;gap:6px;align-items:baseline;",
                span { style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_4};", "#{c.number}" }
                span { style: "font-size:13px;line-height:1.55;flex:1;", "{c.title}" }
            }
            div {
                style: "display:flex;gap:5px;flex-wrap:wrap;margin-top:8px;",
                span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{c.category}" }
                span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{c.tool}" }
                if c.kind != "业务活" {
                    span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{c.kind}" }
                }
                if !c.version.is_empty() {
                    span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{c.version}" }
                }
            }
        }
    }
}

fn detail_panel(mut selected: Signal<Option<CardItemVm>>, bridge: &Bridge) -> Element {
    let Some(c) = selected.read().clone() else {
        return rsx! {
            div {
                style: "width:260px;flex:none;{theme::card()}padding:16px;color:{theme::INK_4};\
                        font-size:12px;line-height:1.9;",
                "点一张卡片看详情。卡面上不放按钮 —— 动作都在这里。"
            }
        };
    };
    let b_run = bridge.clone();
    let b_review = bridge.clone();
    let b_done = bridge.clone();
    let b_block = bridge.clone();
    let id = c.id;
    rsx! {
        div {
            style: "width:260px;flex:none;{theme::card()}padding:16px;max-height:calc(100vh - 160px);overflow:auto;",
            div {
                style: "display:flex;align-items:baseline;gap:6px;margin-bottom:10px;",
                span { style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_4};", "#{c.number}" }
                div { style: "flex:1;" }
                button {
                    style: "cursor:pointer;background:none;border:none;color:{theme::INK_4};font-size:16px;",
                    onclick: move |_| selected.set(None),
                    "×"
                }
            }
            div { style: "font-family:{theme::SERIF};font-size:15px;line-height:1.6;margin-bottom:12px;", "{c.title}" }
            {meta_row("状态", c.status.label())}
            {meta_row("类别", &c.category)}
            {meta_row("开工工具", &c.tool)}
            {meta_row("workflow", &c.workflow)}
            {meta_row("种类", &c.kind)}
            {meta_row("来源", &c.origin)}
            {meta_row("排在", if c.week_of.is_empty() { "待办池" } else { &c.week_of })}
            {meta_row("版本", if c.version.is_empty() { "—" } else { &c.version })}
            {meta_row("推的指标", if c.metric_key.is_empty() { "—" } else { &c.metric_key })}
            {meta_row("结清", if c.settled { "已结清(只结这一次)" } else { "未结清" })}

            div { style: "height:14px;" }
            // 同一个位置按状态互斥切换,不堆一排常驻按钮。
            match c.status {
                IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress => rsx! {
                    button {
                        style: "{theme::btn_primary()}width:100%;",
                        onclick: move |_| b_run.cmd(Command::RunIssue { id }),
                        "▶ 开工"
                    }
                },
                IssueStatus::InReview => rsx! {
                    button {
                        style: "{theme::btn_primary()}width:100%;",
                        onclick: move |_| b_done.cmd(Command::TransitionIssue { id, to: IssueStatus::Done }),
                        "确认完成"
                    }
                    div {
                        style: "font-size:11px;color:{theme::INK_4};margin-top:8px;line-height:1.8;",
                        "「完成」永远是人点的。跑完的活最远只到这一步。"
                    }
                },
                IssueStatus::Done => rsx! {
                    div {
                        style: "font-size:12px;color:{theme::INK_3};line-height:1.9;",
                        "这张活已经完成并结清了。"
                    }
                },
                IssueStatus::Blocked => rsx! {
                    button {
                        style: "{theme::btn_ghost()}width:100%;",
                        onclick: move |_| b_review.cmd(Command::TransitionIssue { id, to: IssueStatus::Todo }),
                        "解除阻塞,回待办"
                    }
                },
                IssueStatus::Cancelled => rsx! { div { style: "font-size:12px;color:{theme::INK_3};", "已取消。" } },
            }
            if !matches!(c.status, IssueStatus::Done | IssueStatus::Blocked | IssueStatus::Cancelled) {
                button {
                    style: "{theme::btn_ghost()}width:100%;margin-top:8px;",
                    onclick: move |_| b_block.cmd(Command::BlockIssue {
                        id,
                        reason: "在计划屏手动标为阻塞".into(),
                    }),
                    "标为阻塞"
                }
            }
        }
    }
}

fn meta_row(k: &str, v: &str) -> Element {
    rsx! {
        div {
            style: "display:flex;gap:8px;font-size:12px;padding:3px 0;",
            div { style: "width:66px;flex:none;color:{theme::INK_4};", "{k}" }
            div { style: "color:{theme::INK_2};word-break:break-all;", "{v}" }
        }
    }
}

fn confirm_dialog(
    pm: PendingMove,
    mut pending: Signal<Option<PendingMove>>,
    bridge: &Bridge,
) -> Element {
    let b = bridge.clone();
    let is_schedule = matches!(pm.to, IssueStatus::Backlog | IssueStatus::Todo)
        && matches!(pm.from, IssueStatus::Backlog | IssueStatus::Todo);
    let pm2 = pm.clone();
    let confirm = move |_| {
        if is_schedule {
            b.cmd(Command::ScheduleIssue {
                id: pm2.id,
                // 拖回待办池 = 清空排期(None);拖进待办 = 排进左栏正在看的那一周。
                week_of: if pm2.to == IssueStatus::Backlog {
                    None
                } else {
                    Some(pm2.week.clone())
                },
            });
        } else {
            b.cmd(Command::TransitionIssue {
                id: pm2.id,
                to: pm2.to,
            });
        }
        pending.set(None);
    };
    rsx! {
        div {
            style: "position:fixed;inset:0;background:rgba(35,33,28,.34);display:flex;\
                    align-items:center;justify-content:center;z-index:50;",
            div {
                style: "{theme::card()}padding:22px;max-width:420px;",
                div { style: "font-family:{theme::SERIF};font-size:17px;margin-bottom:10px;", "确认一下" }
                div {
                    style: "font-size:13px;line-height:1.85;color:{theme::INK_2};margin-bottom:18px;",
                    "把「{pm.title}」从「{pm.from.label()}」移到「{pm.to.label()}」。"
                    if pm.to == IssueStatus::Done {
                        br {}
                        "这一下就是「人点完成」——活会在这一刻结清,只结这一次。"
                    }
                }
                div {
                    style: "display:flex;gap:10px;justify-content:flex-end;",
                    button {
                        style: "{theme::btn_ghost()}",
                        onclick: move |_| pending.set(None),
                        "算了"
                    }
                    button { style: "{theme::btn_primary()}", onclick: confirm, "确认" }
                }
            }
        }
    }
}
