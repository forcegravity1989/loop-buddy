//! 项目内 · 计划。**结构照 `hifi/index.html` 的 `renderPlan` 排**:左栏 180px
//! 周列表 + 中栏工具条 / 周目标 / 进度条 / 六列看板,右侧 360px 详情抽屉。
//!
//! **所有列都能拖**,但两种拖不是一回事:
//!
//! - 拖进/拖出「待办池」是**排期**,直接生效(改的是这张活排在哪一周)。
//! - 拖进进行中 / 评审中 / 已完成 / 阻塞是**状态动作**,松手弹确认框,确认了
//!   才真的发命令;状态机不允许的转移松手即弹回,连确认框都不弹。
//!
//! 卡面上没有按钮 —— 按钮全在右侧详情抽屉。同一个位置按状态互斥切换文案,
//! 不堆一排常驻按钮。

mod detail;
mod kanban;

use crate::bridge::{Bridge, Req};
use crate::vm::{CardItemVm, ProjectVm};
use bw_v4::command::Command;
use bw_v4::model::{IssueId, IssueStatus};
use dioxus::prelude::*;

use detail::DetailPanel;
use kanban::{board, PendingMove};

/// 「全部活」视图上的四个筛选器。纯界面过滤,不发命令。
#[derive(Clone, Default, PartialEq)]
struct Filters {
    category: String,
    version: String,
    origin: String,
    keyword: String,
}

impl Filters {
    fn keep(&self, c: &CardItemVm) -> bool {
        (self.category.is_empty() || c.category == self.category)
            && (self.version.is_empty() || c.version == self.version)
            && (self.origin.is_empty() || c.origin == self.origin)
            && (self.keyword.is_empty()
                || c.title
                    .to_lowercase()
                    .contains(&self.keyword.to_lowercase()))
    }
}

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    let dragging = use_signal(|| None::<CardItemVm>);
    let pending = use_signal(|| None::<PendingMove>);
    let selected = use_signal(|| None::<IssueId>);
    let bounced = use_signal(String::new);
    let filters = use_signal(Filters::default);

    rsx! {
        section { class: "plan-grid", style: "height:calc(100vh - 112px);",
            div { class: "plan-col", {week_rail(p, bridge)} }
            div { class: "plan-col plan-mid",
                {toolbar(p, bridge)}
                if p.view_all {
                    {filter_bar(filters)}
                } else {
                    {week_head(p)}
                }
                {draft_confirm(p, bridge)}
                if !bounced.read().is_empty() {
                    div { class: "mr-banner", style: "margin:0 0 8px;color:var(--alert-deep);", "{bounced}" }
                }
                {board(p, Some(filters.read().clone()), bridge, dragging, pending, selected, bounced)}
            }
        }
        DetailPanel { p: p.clone(), selected, bridge: bridge.clone() }
        if let Some(pm) = pending.read().clone() {
            {confirm_dialog(pm, pending, bridge)}
        }
    }
}

// ── 左栏 ────────────────────────────────────────────

fn week_rail(p: &ProjectVm, bridge: &Bridge) -> Element {
    let b_all = bridge.clone();
    let (current, history): (Vec<_>, Vec<_>) = p.weeks.iter().partition(|w| !w.backfill);
    rsx! {
        div {
            class: if p.view_all { "plan-allrow active" } else { "plan-allrow" },
            onclick: move |_| b_all.send(Req::ViewAll(true)),
            "全部"
        }
        if p.weeks.is_empty() {
            div { class: "detail-empty",
                "docs/plan/ 里还没有周计划文件。周列表是扫这个目录得到的,没有索引表。"
            }
        }
        for w in current.iter() {
            {week_row(w, p, bridge)}
        }
        if !history.is_empty() {
            div { class: "plan-weekgroup", "历史周(回填)" }
        }
        for w in history.iter() {
            {week_row(w, p, bridge)}
        }
        div { class: "plan-versionsel",
            label { class: "label", "在研版本" }
            div { class: "mono", style: "font-size:13px;", "{p.card.current_version}" }
        }
    }
}

fn week_row(w: &crate::vm::WeekVm, p: &ProjectVm, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let week = w.week.clone();
    let active = !p.view_all && w.week == p.viewing_week;
    let is_current = w.week == p.current_week;
    rsx! {
        div {
            key: "{w.week}",
            class: if active { "plan-weekrow active" } else { "plan-weekrow" },
            onclick: move |_| b.send(Req::ViewWeek(week.clone())),
            span {
                "{w.week}"
                if is_current { " · 本周" }
            }
            if w.backfill {
                span { class: "chip-muted", "回填" }
            } else {
                span { class: "chip-muted", "{w.activity_count}" }
            }
        }
    }
}

// ── 中栏顶部 ────────────────────────────────────────

fn toolbar(p: &ProjectVm, bridge: &Bridge) -> Element {
    let b_new = bridge.clone();
    let b_rel = bridge.clone();
    let b_refresh = bridge.clone();
    let pid = p.id;
    let week = p.viewing_week.clone();
    let week2 = p.viewing_week.clone();
    let version = p.card.current_version_raw.clone();
    let can_release = !next_version(&version).is_empty();
    let done_ids: Vec<IssueId> = p
        .board
        .columns
        .iter()
        .find(|c| c.status == IssueStatus::Done)
        .map(|c| c.cards.iter().map(|x| x.id).collect())
        .unwrap_or_default();
    let title = if p.view_all {
        "全部活".to_string()
    } else if p.viewing_week == p.current_week {
        format!("{} · 本周", p.viewing_week)
    } else {
        p.viewing_week.clone()
    };
    rsx! {
        div { class: "plan-toolbar",
            div { class: "left", "{title}" }
            div { class: "right",
                if !p.view_all {
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| b_refresh.cmd(Command::RefreshIssueCacheFromPlan {
                            project_id: pid,
                            week: week2.clone(),
                        }),
                        "按文件刷新"
                    }
                    button {
                        class: "btn btn-sm",
                        // 在研版本填了、而且这一周真有完成的活,才发得了版。
                        disabled: done_ids.is_empty() || !can_release,
                        onclick: move |_| b_rel.cmd(Command::CutRelease {
                            project_id: pid,
                            version: next_version(&version),
                            note: "本周完成的活".into(),
                            included: done_ids.clone(),
                        }),
                        "发版本"
                    }
                    button {
                        class: "btn btn-sm btn-primary",
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
                    // 高保真上这里还有一颗「预览 · 未合入」开关:看活自己的
                    // worktree 里那份还没合入的 .bw/metrics.toml 与 docs/plan/。
                    // 还没接,做成明确的灰态,点不动 —— 不放一个点了没反应的开关。
                    label { class: "switch", title: "还没接:要去读活自己 worktree 里未合入的仓文件",
                        input { r#type: "checkbox", disabled: true }
                        " 预览 · 未合入"
                    }
                }
            }
        }
    }
}

fn week_head(p: &ProjectVm) -> Element {
    let w = p.weeks.iter().find(|w| w.week == p.viewing_week);
    let goal = w
        .and_then(|w| w.goal.clone())
        .unwrap_or_else(|| "(这一周还没写周目标)".into());
    // 这条进度条画在它正上方那个看板上,所以数的是看板的范围,不是本周。
    let c = &p.board_counts;
    rsx! {
        div { class: "goal-box", "{goal}" }
        div { class: "week-progress",
            div { style: "width:{c.pct(c.done)}%;background:var(--green);" }
            div { style: "width:{c.pct(c.review)}%;background:var(--amber);" }
            div { style: "width:{c.pct(c.doing)}%;background:var(--clay);" }
            div { style: "width:{c.pct(c.blocked)}%;background:var(--red);" }
            div { style: "width:{c.pct(c.todo)}%;background:#E4DDC8;" }
        }
        if !p.ops.is_empty() {
            div { class: "ops-chip-row",
                for o in p.ops.iter() {
                    span { key: "{o.title}", class: "chip chip-outline", title: "{o.note}",
                        "{o.title} · {o.status}"
                    }
                }
            }
        }
    }
}

fn filter_bar(mut filters: Signal<Filters>) -> Element {
    rsx! {
        div { class: "filter-bar",
            select {
                onchange: move |e| filters.write().category = e.value(),
                option { value: "", "类别 · 全部" }
                for c in bw_v4::model::StageKind::ALL {
                    option { key: "{c:?}", value: "{c.label()}", "{c.label()}" }
                }
            }
            input {
                placeholder: "版本…",
                oninput: move |e| filters.write().version = e.value(),
            }
            select {
                onchange: move |e| filters.write().origin = e.value(),
                option { value: "", "来源 · 全部" }
                option { value: "人建", "人建" }
                option { value: "自动建", "自动建" }
                option { value: "agent 拆", "agent 拆" }
            }
            input {
                placeholder: "关键字…",
                oninput: move |e| filters.write().keyword = e.value(),
            }
        }
    }
}

/// 「开始本周」产出的草稿活标,等人确认。**确认之前一张活都没建** —— 草稿
/// 只在界面里活着,库里查不到。人点「先不建」就丢掉,不留痕。
fn draft_confirm(p: &ProjectVm, bridge: &Bridge) -> Element {
    if p.pending_drafts.is_empty() {
        return rsx! {};
    }
    let b_ok = bridge.clone();
    let b_no = bridge.clone();
    let pid = p.id;
    let week = p.viewing_week.clone();
    let titles = p.pending_drafts.clone();
    rsx! {
        div { class: "card", style: "padding:14px 16px;margin-bottom:10px;flex:none;",
            div { style: "font-size:12.5px;color:var(--ink-2);margin-bottom:8px;line-height:1.8;",
                "{week} 的周计划文件写好了,下面是草稿活标。确认之前一张活都还没建。"
            }
            for t in titles.iter() {
                div { key: "{t}", style: "font-size:12.5px;padding:3px 0;", "· {t}" }
            }
            div { style: "display:flex;gap:8px;margin-top:10px;",
                button {
                    class: "btn btn-sm btn-primary",
                    onclick: move |_| b_ok.cmd(Command::ConfirmWeekDraft {
                        project_id: pid,
                        week: week.clone(),
                        titles: titles.clone(),
                    }),
                    "确认,按这些建活"
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| b_no.send(Req::DropDrafts),
                    "先不建"
                }
            }
        }
    }
}

/// `v0.3` → `v0.4`。**认不出格式就返回空**,由调用方把按钮置灰 —— 不猜一个
/// 版本号出来,更不能把「(待填)」这种给人看的占位文案当版本号发出去。
fn next_version(cur: &str) -> String {
    let t = cur.trim().trim_start_matches('v');
    let Some((head, tail)) = t.rsplit_once('.') else {
        return String::new();
    };
    match tail.parse::<u32>() {
        Ok(n) if !head.is_empty() => format!("v{head}.{}", n + 1),
        _ => String::new(),
    }
}

fn confirm_dialog(
    pm: PendingMove,
    mut pending: Signal<Option<PendingMove>>,
    bridge: &Bridge,
) -> Element {
    let b = bridge.clone();
    let pm2 = pm.clone();
    // 走到这里的一定是**状态动作** —— 排期在松手那一刻就直接发命令了,不经过
    // 这个框(见本文件顶部的说明)。
    let confirm = move |_| {
        b.cmd(Command::TransitionIssue {
            id: pm2.id,
            to: pm2.to,
        });
        pending.set(None);
    };
    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal", style: "max-width:420px;",
                h3 { "确认一下" }
                div { style: "font-size:13px;line-height:1.85;color:var(--ink-2);margin-bottom:8px;",
                    "把「{pm.title}」从「{pm.from.label()}」移到「{pm.to.label()}」。"
                    if pm.to == IssueStatus::Done {
                        br {}
                        "这一下就是「人点完成」——活会在这一刻结清,只结这一次。"
                    }
                    if pm.to == IssueStatus::InReview {
                        br {}
                        "拖过来**只改状态**,不会提交、不会推分支、不会开 MR。要让别人有东西可评审,                         去会话屏点「提交并开 MR」。"
                    }
                }
                div { class: "modal-actions",
                    button { class: "btn", onclick: move |_| pending.set(None), "算了" }
                    button { class: "btn btn-primary", onclick: confirm, "确认" }
                }
            }
        }
    }
}
