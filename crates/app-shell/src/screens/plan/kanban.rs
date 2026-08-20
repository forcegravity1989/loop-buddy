//! 六列看板本体。列头带「这一列是什么意思」的定义,卡片左边框按状态染色。

use super::Filters;
use crate::bridge::Bridge;
use crate::chrome::light_dot;
use crate::vm::{CardItemVm, ColumnVm, ProjectVm};
use bw_v4::command::Command;
use bw_v4::model::{IssueId, IssueStatus, Signal as HealthSignal};
use dioxus::prelude::*;

/// 松手之后等人确认的那一下。
#[derive(Clone, PartialEq)]
pub struct PendingMove {
    pub id: IssueId,
    pub title: String,
    pub from: IssueStatus,
    pub to: IssueStatus,
}

/// 每一列一句「什么样的活会在这一列」。照高保真的 KANBAN_COL_DEF。
fn col_def(s: IssueStatus) -> &'static str {
    match s {
        IssueStatus::Backlog => "未排进任何一周",
        IssueStatus::Todo => "已排进这一周,等开工",
        IssueStatus::InProgress => "agent 在干",
        IssueStatus::InReview => "MR 开着,等人审",
        IssueStatus::Done => "人点过完成",
        IssueStatus::Blocked => "填了原因",
        IssueStatus::Cancelled => "已取消",
    }
}

/// 卡片左边框与状态灯点的颜色。用的是健康那三色 + 灰,不另起一套。
fn status_signal(s: IssueStatus) -> Option<HealthSignal> {
    match s {
        IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::Cancelled => None,
        IssueStatus::InProgress | IssueStatus::InReview => Some(HealthSignal::Amber),
        IssueStatus::Done => Some(HealthSignal::Green),
        IssueStatus::Blocked => Some(HealthSignal::Red),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn board(
    p: &ProjectVm,
    filters: Option<Filters>,
    bridge: &Bridge,
    dragging: Signal<Option<CardItemVm>>,
    pending: Signal<Option<PendingMove>>,
    selected: Signal<Option<IssueId>>,
    bounced: Signal<String>,
) -> Element {
    // 筛选器只在「全部活」视图上生效;看某一周时不过滤。
    let f = if p.view_all { filters } else { None };
    // 拖进「待办」= 排进哪一周。看某一周就排进那一周;「全部活」视图下左栏
    // 并没有选中任何一周,`viewing_week` 只是上次点过的值(可能是历史周),
    // 拿它排期会把活悄悄塞进过去的一周 —— 这种时候一律排进当前周。
    let drop_week = if p.view_all {
        p.current_week.as_str()
    } else {
        p.viewing_week.as_str()
    };
    rsx! {
        div { class: "kanban",
            for col in p.board.columns.iter() {
                {column(col, f.clone(), drop_week, bridge, dragging, pending, selected, bounced)}
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn column(
    col: &ColumnVm,
    filters: Option<Filters>,
    drop_week: &str,
    bridge: &Bridge,
    mut dragging: Signal<Option<CardItemVm>>,
    mut pending: Signal<Option<PendingMove>>,
    selected: Signal<Option<IssueId>>,
    mut bounced: Signal<String>,
) -> Element {
    let target = col.status;
    let week = drop_week.to_string();
    let b_drop = bridge.clone();
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
            bounced.set(String::new());
            b_drop.cmd(Command::ScheduleIssue {
                id: card.id,
                // 拖回待办池 = 清空排期;拖进待办 = 排进左栏正在看的那一周。
                week_of: if target == IssueStatus::Backlog {
                    None
                } else {
                    Some(week.clone())
                },
            });
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
        }));
    };

    // 拖着一张卡经过时,这一列能不能放。能放描 clay,不能放描灰。
    let cls = match dragging.read().as_ref() {
        None => "kanban-col".to_string(),
        Some(c) if c.status == target => "kanban-col".to_string(),
        Some(c) => {
            let schedule = matches!(target, IssueStatus::Backlog | IssueStatus::Todo)
                && matches!(c.status, IssueStatus::Backlog | IssueStatus::Todo);
            if schedule || c.status.can_transition_to(target) {
                "kanban-col drop-ok".to_string()
            } else {
                "kanban-col drop-no".to_string()
            }
        }
    };

    let cards: Vec<&CardItemVm> = col
        .cards
        .iter()
        .filter(|c| filters.as_ref().is_none_or(|f| f.keep(c)))
        .collect();

    rsx! {
        div {
            key: "{col.status:?}",
            class: "{cls}",
            ondragover: move |e| e.prevent_default(),
            ondrop: drop,
            div { class: "kanban-col-head",
                "{col.title} · {cards.len()}"
                div { class: "kanban-col-def", {col_def(col.status)} }
            }
            for c in cards {
                {card_view(c, dragging, selected)}
            }
        }
    }
}

fn card_view(
    c: &CardItemVm,
    mut dragging: Signal<Option<CardItemVm>>,
    mut selected: Signal<Option<IssueId>>,
) -> Element {
    let c1 = c.clone();
    let cid = c.id;
    let sig = status_signal(c.status);
    let edge = crate::theme::signal_color(sig);
    rsx! {
        div {
            key: "{c.id:?}",
            class: "kcard",
            draggable: true,
            style: "border-left-color:{edge};",
            ondragstart: move |_| dragging.set(Some(c1.clone())),
            ondragend: move |_| dragging.set(None),
            div { class: "kcard-top",
                span { class: "chip chip-outline", "{c.category}" }
                if c.kind != "业务活" {
                    span { class: "chip chip-gray", "{c.kind}" }
                }
                if c.origin == "自动建" {
                    span { class: "chip chip-gray", "自动" }
                }
                // 没排周的活在每一周的视图里都露面(运作活按设计就不排周)。
                // 不挂这个徽记的话,人会以为它属于正在看的这一周。
                if c.week_of.is_empty() {
                    span {
                        class: "chip chip-gray",
                        title: "这张活没排进任何一周,在哪一周看都能看到它",
                        "未排周"
                    }
                }
                span { style: "margin-left:auto;", title: "{c.status.label()}", {light_dot(sig, false)} }
            }
            div {
                class: "kcard-title",
                onclick: move |_| selected.set(Some(cid)),
                "#{c.number} {c.title}"
            }
        }
    }
}
