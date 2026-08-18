//! `view=app` — the operating view: the real monitoring/run loop, now over
//! the five stage=role=methodology stages (体系重构 v2).
//!
//! Everything rendered here traces back to persisted rows: signals from the
//! derive cache, trends from observation history, feeds from real records,
//! methodology text from `StageKind`'s own static metadata. The two live
//! loops:
//!
//! * **监控**: 记录观测值 → RecordObservation → recompute → 信号翻转可见;
//! * **运行**: Issue 看板「▶ 跑」→ 内嵌终端里真跑(`RunIssue`)→ 最远到
//!   「评审中」,「完成」由人点。
//!
//! Plus the handoff loop: 勾 DoD → 交棒(可带险,永不静默拦截)→ 下一段自动换装,
//! `运维 → 原型` 回流闭环。

use crate::kernel::{Kernel, OpVm, StageVm};
use crate::theme;
use bw_app::{Command, Panel, Scope};
use bw_core::model::{
    stage_workflow, FeedLevel, HubKind, IssuePriority, IssueStatus, MaturityPeriod, Signal,
    StageKind,
};
use bw_core::{ConversationId, IssueId, ProjectId, SessionId, SkillId};
use bw_store::SessionKind;
use dioxus::document;
use dioxus::prelude::*;
use std::time::Duration;
use ui::vm::{IssueVm, MetricVm, SessionCardVm, VersionLogVm};
use ui::{sparkline_path, trend_chart, SparkPath, TrendChart, WowDir};

mod issues;
mod terminal_widget;
use issues::IssuesPanel;
use terminal_widget::TerminalWidget;

#[component]
pub fn Op(op: OpVm, on_pick_hub: EventHandler<HubKind>) -> Element {
    let paper = theme::PAPER;
    // Live PTY: center column fills height so the terminal can flex vertically.
    // Other panels keep scrollable content.
    let embed_od =
        op.panel == Panel::Progress && matches!(op.scope, Scope::Stage(StageKind::Prototype));
    let center = if op.panel == Panel::Workflow && op.pty_active {
        "flex:1;min-width:0;min-height:0;display:flex;flex-direction:column;overflow:hidden;padding:14px 22px 16px;"
    } else if embed_od {
        "flex:1;min-width:0;min-height:0;display:flex;flex-direction:column;overflow:hidden;padding:10px 14px 10px;"
    } else {
        "flex:1;min-width:0;overflow-y:auto;padding:18px 22px 40px;"
    };
    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};",
            TopBar { op: op.clone() }
            StageAxis { op: op.clone() }
            Toolbar { op: op.clone() }
            div {
                style: "flex:1;display:flex;min-height:0;",
                LeftRail { op: op.clone() }
                div {
                    style: "{center}",
                    Center { op, on_pick_hub }
                }
            }
        }
    }
}

// ───────────────────────── chrome rows ─────────────────────────

#[component]
fn TopBar(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let border = theme::BORDER;
    let sig = ui::signal_color(op.project_signal);
    let dot = theme::dot(sig, 10);
    let chip = theme::chip("#E7EDE2", "#4A5E42");
    let (role_bg, role_fg, _) = ui::stage_tint(op.active_stage);
    let role_chip = theme::chip(role_bg, role_fg);
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:14px;padding:14px 22px;border-bottom:1px solid {border};flex:none;",
            button {
                style: "background:transparent;border:none;color:{ink2};cursor:pointer;font-size:13px;padding:0;",
                onclick: move |_| k.send(Command::BackToProjects),
                "← 全部项目"
            }
            span { style: "{dot}" }
            span { style: "font-family:{serif};font-size:17px;font-weight:600;", "{op.name}" }
            span { style: "{chip}", "运营中" }
            span { style: "{role_chip}", "当前 {op.active_stage.role_short()}" }
            span { style: "color:{ink3};font-size:12px;", "{op.kind} · {op.cycle.label()}" }
            if !op.north_star.is_empty() {
                span {
                    style: "margin-left:auto;color:{ink3};font-size:12px;max-width:380px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;",
                    title: "{op.ns_def}",
                    "北极星 · {op.north_star}"
                }
            }
        }
    }
}

#[component]
fn StageAxis(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let border = theme::BORDER;
    let ink = theme::INK;
    let ink2 = theme::INK_2;
    let all_active = op.scope == Scope::All;
    let (all_bg, all_fg) = if all_active {
        (ink, "#FFF")
    } else {
        ("transparent", ink2)
    };
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:6px;padding:10px 22px;border-bottom:1px solid {border};flex:none;overflow-x:auto;",
            button {
                style: "cursor:pointer;border:1px solid {border};border-radius:8px;background:{all_bg};color:{all_fg};padding:6px 12px;font-size:12px;white-space:nowrap;",
                onclick: {
                    let k = k.clone();
                    move |_| {
                        k.send(Command::SetScope(Scope::All));
                        k.send(Command::SetPanel(Panel::Progress));
                    }
                },
                "◎ 全部阶段 · 总览"
            }
            for item in op.nav.clone() {
                {
                    let k = k.clone();
                    let active = op.scope == Scope::Stage(item.kind);
                    let is_hot = item.kind == op.active_stage;
                    let (tint_bg, tint_fg, tint_bd) = ui::stage_tint(item.kind);
                    let (bg, fg, bd) = if active {
                        (tint_bg, tint_fg, item.color)
                    } else {
                        ("transparent", ink2, tint_bd)
                    };
                    let color = ui::signal_color(item.signal);
                    let dot = theme::dot(color, 7);
                    let kind = item.kind;
                    rsx! {
                        button {
                            key: "{item.n}",
                            title: "{item.role_short}",
                            style: "cursor:pointer;border:1.4px solid {bd};border-radius:8px;background:{bg};color:{fg};padding:6px 11px;font-size:12px;display:flex;align-items:center;gap:7px;white-space:nowrap;",
                            onclick: move |_| k.send(Command::SetScope(Scope::Stage(kind))),
                            span { style: "{dot}" }
                            span { "{item.n} {item.label}" }
                            if is_hot {
                                span { style: "font-size:9px;color:{item.color};", "●当前" }
                            }
                            if item.active > 0 {
                                span {
                                    style: "background:#C5654A;color:#FFF;border-radius:8px;font-size:10px;padding:0 5px;line-height:15px;",
                                    "{item.active}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// L2(plan/11): two groups, not one flat row — 看板(对外可验证的整体进展 +
// 难造假的健康)vs 过程件(达成看板数字的内部机制)。`Panel` itself is
// untouched (bw-app); this split is pure UI grouping.
const BOARD_PANELS: [(Panel, &str); 3] = [
    (Panel::Progress, "进度"),
    (Panel::Issues, "Issue 看板"),
    (Panel::Version, "版本"),
];
const PROCESS_PANELS: [(Panel, &str); 3] = [
    (Panel::Workflow, "工作流"),
    (Panel::Routine, "定时任务"),
    (Panel::Artifact, "产物"),
];

#[component]
fn Toolbar(op: OpVm) -> Element {
    let border = theme::BORDER;
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:14px;padding:10px 22px;border-bottom:1px solid {border};flex:none;",
            PanelGroup { label: "看板", panels: &BOARD_PANELS, op: op.clone() }
            span { style: "width:1px;height:20px;background:{border};", "" }
            PanelGroup { label: "过程件", panels: &PROCESS_PANELS, op: op.clone() }
        }
    }
}

#[component]
fn PanelGroup(label: &'static str, panels: &'static [(Panel, &'static str)], op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let ink = theme::INK;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:6px;",
            span { style: "font-size:10.5px;color:{ink3};letter-spacing:.05em;margin-right:2px;", "{label}" }
            for (panel , plabel) in panels.iter().copied() {
                {
                    let k = k.clone();
                    let active = op.panel == panel;
                    let (bg, fg) = if active { (ink, "#FFF") } else { ("transparent", ink2) };
                    rsx! {
                        button {
                            key: "{plabel}",
                            style: "cursor:pointer;border:none;border-radius:8px;background:{bg};color:{fg};padding:7px 14px;font-size:12.5px;",
                            onclick: move |_| k.send(Command::SetPanel(panel)),
                            "{plabel}"
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────── left rail ─────────────────────────

#[component]
fn LeftRail(op: OpVm) -> Element {
    let border = theme::BORDER;
    rsx! {
        div {
            style: "width:232px;flex:none;border-right:1px solid {border};overflow-y:auto;padding:14px;",
            if op.scope == Scope::All {
                ActiveSessionsRail { op }
            } else {
                StageSessions { op }
            }
        }
    }
}

/// L2(plan/11): what's left of the old `HealthOverview` in the left rail —
/// just "进行中 · 待你介入", session-nav, not health. The signal/attention
/// half moved into `HealthOverviewCard` at the top of the 进度 panel (看板
/// 数字属于看板,不属于每个面板都挂一份的侧栏挂件).
#[component]
fn ActiveSessionsRail(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let ink3 = theme::INK_3;
    let card_alt = theme::CARD_ALT;
    let needs_you: Vec<SessionCardVm> = op.sessions.iter().filter(|s| s.active).cloned().collect();
    let sessions = op.sessions.clone();
    let issues = op.issues.clone();
    rsx! {
        div { style: "font-size:11px;color:{ink3};letter-spacing:.06em;margin-bottom:8px;", "进行中 · 待你介入" }
        if needs_you.is_empty() {
            div { style: "font-size:12px;color:{ink3};line-height:1.7;", "没有进行中的会话——到「Issue」面板点「▶ 跑」开工。" }
        }
        for s in needs_you {
            {
                let k = k.clone();
                let sid = s.id;
                let title = s.title.clone();
                let stage = s.stage_kind;
                let stage_label = stage.map(|x| x.label()).unwrap_or("项目");
                let sessions = sessions.clone();
                let issues = issues.clone();
                rsx! {
                    button {
                        key: "{sid.uuid()}",
                        style: "width:100%;text-align:left;background:{card_alt};border:1px solid #DBD4C5;border-radius:8px;padding:9px 10px;margin-bottom:7px;cursor:pointer;",
                        onclick: move |_| {
                            wake_session_like_board(
                                &k,
                                &sessions,
                                &issues,
                                sid,
                                &title,
                                stage,
                            );
                        },
                        div { style: "font-size:12.5px;margin-bottom:3px;", "{s.title}" }
                        div { style: "font-size:11px;color:{ink3};", "{stage_label} · {s.status_label}" }
                    }
                }
            }
        }
    }
}

/// V1-Issue3 · cross-stage health overview moved to `wall.rs` (project-list
/// entry page). The ProgressAll page no longer repeats it — the 阶段轴 already
/// shows per-stage signal dots, so a separate health-overview card here was
/// redundant. See `wall::HealthOverviewBar`.

#[component]
fn StageSessions(op: OpVm) -> Element {
    let ink3 = theme::INK_3;
    let agent = theme::AGENT;
    let Scope::Stage(kind) = op.scope else {
        return rsx! { span {} };
    };
    let active_id = op.active_session;
    let mine: Vec<SessionCardVm> = op
        .sessions
        .iter()
        .filter(|s| s.stage_kind == Some(kind))
        .cloned()
        .collect();
    let creates: Vec<SessionCardVm> = mine.iter().filter(|s| s.create).cloned().collect();
    let opts: Vec<SessionCardVm> = mine.iter().filter(|s| !s.create).cloned().collect();
    let empty = mine.is_empty();
    rsx! {
        div { style: "font-size:11px;color:{ink3};letter-spacing:.06em;margin-bottom:8px;", "阶段记录" }
        if empty {
            div { style: "font-size:12px;color:{ink3};line-height:1.7;", "该阶段暂无记录。到「Issue」面板点「▶ 跑」开工,记录会出现在这里。" }
        }
        if !creates.is_empty() {
            div { style: "font-size:11px;color:{ink3};margin:6px 0;", "创建" }
            for s in creates {
                SessionCard {
                    s: s.clone(),
                    selected: active_id == Some(s.id),
                    sessions: op.sessions.clone(),
                    issues: op.issues.clone(),
                }
            }
        }
        if !opts.is_empty() {
            div { style: "font-size:11px;color:{agent};margin:8px 0 6px;", "优化" }
            for s in opts {
                SessionCard {
                    s: s.clone(),
                    selected: active_id == Some(s.id),
                    sessions: op.sessions.clone(),
                    issues: op.issues.clone(),
                }
            }
        }
    }
}

#[component]
fn SessionCard(
    s: SessionCardVm,
    selected: bool,
    sessions: Vec<SessionCardVm>,
    issues: Vec<IssueVm>,
) -> Element {
    let k = use_context::<Kernel>();
    let ink3 = theme::INK_3;
    let bd = if selected { theme::CLAY } else { "#DBD4C5" };
    let card_alt = theme::CARD_ALT;
    let sid = s.id;
    let title = s.title.clone();
    let stage_kind = s.stage_kind;
    let mut confirming_delete = use_signal(|| false);
    let k_del = k.clone();
    rsx! {
        div {
            key: "{sid.uuid()}",
            style: "width:100%;text-align:left;background:{card_alt};border:1.4px solid {bd};border-radius:8px;padding:9px 10px;margin-bottom:7px;",
            div {
                style: "display:flex;align-items:flex-start;gap:6px;",
                button {
                    style: "flex:1;text-align:left;background:transparent;border:none;cursor:pointer;padding:0;font:inherit;color:inherit;",
                    onclick: move |e| {
                        e.stop_propagation();
                        wake_session_like_board(
                            &k,
                            &sessions,
                            &issues,
                            sid,
                            &title,
                            stage_kind,
                        );
                    },
                    div { style: "font-size:12.5px;margin-bottom:3px;", "{s.title}" }
                    div { style: "font-size:11px;color:{ink3};", "{s.status_label}" }
                }
                button {
                    title: "删除此会话记录",
                    style: "background:transparent;border:none;color:{ink3};cursor:pointer;font-size:14px;padding:0 0 0 4px;line-height:1;flex-shrink:0;",
                    onclick: move |e| {
                        e.stop_propagation();
                        confirming_delete.set(true);
                    },
                    "×"
                }
            }
            if confirming_delete() {
                div {
                    style: "margin-top:8px;padding-top:8px;border-top:1px dashed {ink3};display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                    span { style: "font-size:11.5px;color:{ink3};flex:1;", "删除此会话记录?不可恢复" }
                    button {
                        style: "cursor:pointer;background:{theme::ALERT_DEEP};color:#FFF;border:none;border-radius:6px;padding:4px 10px;font-size:11.5px;",
                        onclick: move |e| {
                            e.stop_propagation();
                            k_del.send(Command::DeleteSession(sid));
                        },
                        "确认"
                    }
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {ink3};border-radius:6px;padding:4px 10px;font-size:11.5px;",
                        onclick: move |e| {
                            e.stop_propagation();
                            confirming_delete.set(false);
                        },
                        "取消"
                    }
                }
            }
        }
    }
}

// ───────────────────────── center ─────────────────────────

#[component]
fn Center(op: OpVm, on_pick_hub: EventHandler<HubKind>) -> Element {
    let stage = match op.scope {
        Scope::Stage(kind) => op.stages.iter().find(|s| s.kind == kind).cloned(),
        Scope::All => None,
    };
    match (op.panel, stage) {
        (Panel::Progress, None) => rsx! { ProgressAll { op } },
        (Panel::Progress, Some(s)) => {
            let fill = if s.kind == StageKind::Prototype {
                "height:100%;min-height:0;display:flex;flex-direction:column;"
            } else {
                ""
            };
            rsx! {
                div {
                    style: "{fill}",
                    ProgressStage { op, s }
                }
            }
        }
        (Panel::Workflow, s) => {
            let fill = if op.pty_active {
                "height:100%;min-height:0;display:flex;flex-direction:column;"
            } else {
                ""
            };
            rsx! {
                div {
                    style: "{fill}",
                    WorkflowPanel { op, stage: s, on_pick_hub }
                }
            }
        }
        (Panel::Routine, None) => rsx! { RoutineAll { op } },
        (Panel::Routine, Some(s)) => rsx! { RoutineStage { s } },
        (Panel::Artifact, _) => rsx! { ArtifactPanel { op } },
        (Panel::Version, _) => rsx! { VersionPanel { op } },
        (Panel::Issues, _) => rsx! { IssuesPanel { op } },
    }
}

/// Kind chip color — muted per-type hues from the existing stage palette
/// family, keyed on the display label (the Vm carries labels, not enums).
fn artifact_kind_color(kind_label: &str) -> &'static str {
    match kind_label {
        "文档" => "#4F7E86",
        "代码" => "#C5654A",
        "测试" => "#6E8C5A",
        "脚本" => "#CC8B3C",
        "配置" => "#8A8275",
        _ => "#A19B8D",
    }
}

/// The real artifact registry — every row is a tracked file version really
/// scanned out of the project's workspace (`git ls-files` + `stat` + HEAD),
/// registered by post-run auto-scans or a manual "重新采集". The long-ago
/// stub's reason ("no real data source yet") retired when the evidence
/// collector + all-in-one-codebase workspace landed; this panel now renders
/// exactly that source, nothing invented.
#[component]
fn ArtifactPanel(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let k2 = k.clone();
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let clay = theme::CLAY;
    let configured = !op.workspace_path.trim().is_empty();

    rsx! {
        div {
            style: "max-width:820px;",
            div {
                style: "{card} padding:14px 20px;margin-bottom:16px;display:flex;align-items:center;gap:12px;",
                span { style: "font-size:12px;color:{ink3};flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                    if configured {
                        "真实产物登记 · 扫描自 {op.workspace_path}"
                    } else {
                        "未配置真实工作区 —— 没有可扫描的代码仓"
                    }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:6px 14px;font-size:12px;flex:none;",
                    onclick: move |_| k.send(Command::LoadArtifacts),
                    "读取登记"
                }
                if configured {
                    button {
                        style: "cursor:pointer;background:{clay};color:#fff;border:1px solid {clay};border-radius:7px;padding:6px 14px;font-size:12px;flex:none;",
                        onclick: move |_| {
                            k2.send(Command::CollectArtifacts);
                        },
                        "重新采集"
                    }
                }
            }
            match &op.artifacts {
                None => rsx! {
                    div { style: "{card} padding:26px 30px;color:{ink2};font-size:13px;line-height:1.7;",
                        "还没有加载过 —— 点「读取登记」查看已登记产物,或「重新采集」扫描工作区。"
                    }
                },
                Some(rows) if rows.is_empty() => rsx! {
                    div { style: "{card} padding:26px 30px;color:{ink2};font-size:13px;line-height:1.7;",
                        "登记表是空的 —— 这个项目的工作区还没有任何被追踪的文件(或尚未采集过)。"
                    }
                },
                Some(rows) => rsx! {
                    div {
                        for a in rows.clone() {
                            div {
                                key: "{a.path}",
                                style: "{card} padding:11px 18px;margin-bottom:6px;display:flex;align-items:center;gap:12px;",
                                span {
                                    style: "font-family:{mono};font-size:10.5px;color:#fff;background:{artifact_kind_color(a.kind_label)};border-radius:5px;padding:2px 8px;flex:none;",
                                    "{a.kind_label}"
                                }
                                span {
                                    style: "flex:1;min-width:0;font-size:13px;font-family:{mono};overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                                    "{a.path}"
                                }
                                if let Some(stage) = a.stage_label {
                                    span { style: "font-size:11px;color:{ink2};flex:none;", "{stage}段" }
                                }
                                if a.versions > 1 {
                                    span { style: "font-size:11px;color:{ink2};flex:none;", "{a.versions} 个版本" }
                                }
                                if a.from_run {
                                    span { style: "font-size:11px;color:{ink3};flex:none;", "run 产出" }
                                }
                                span { style: "font-size:11px;color:{ink3};flex:none;", "{a.bytes_label}" }
                                span { style: "font-family:{mono};font-size:11px;color:{ink3};flex:none;", "{a.commit_label}" }
                                span { style: "font-size:11px;color:{ink3};flex:none;", "{a.time_label}" }
                            }
                        }
                    }
                },
            }
        }
    }
}

/// Real `git log` on the project's `workspace_path` — no fabricated
/// commits/PRs/issues (unlike the prototype's hash-derived fake GitHub
/// view): a project with no configured workdir, or one that isn't a git
/// repo, says so plainly instead of inventing a history for it.
#[component]
fn VersionPanel(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let clay = theme::CLAY;
    let configured = !op.workspace_path.trim().is_empty();

    rsx! {
        div {
            style: "max-width:760px;",
            div {
                style: "{card} padding:14px 20px;margin-bottom:16px;display:flex;align-items:center;gap:12px;",
                span { style: "font-size:12px;color:{ink3};flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                    if configured {
                        "真实 git log · {op.workspace_path}"
                    } else {
                        "未配置真执行工作目录 —— 没有可读取的 git 仓库"
                    }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:6px 14px;font-size:12px;flex:none;",
                    onclick: move |_| k.send(Command::LoadVersionLog),
                    "刷新提交记录"
                }
            }
            match &op.version_log {
                VersionLogVm::NotLoaded => rsx! {
                    div { style: "{card} padding:26px 30px;color:{ink2};font-size:13px;line-height:1.7;",
                        "还没有加载过 —— 点上面的「刷新提交记录」读取真实 git log。"
                    }
                },
                VersionLogVm::Unavailable(msg) => rsx! {
                    div { style: "{card} padding:26px 30px;color:{ink2};font-size:13px;line-height:1.7;", "{msg}" }
                },
                VersionLogVm::Commits(commits) if commits.is_empty() => rsx! {
                    div { style: "{card} padding:26px 30px;color:{ink2};font-size:13px;line-height:1.7;",
                        "这个仓库还没有任何提交。"
                    }
                },
                VersionLogVm::Commits(commits) => rsx! {
                    div {
                        for c in commits.clone() {
                            div {
                                key: "{c.short_hash}",
                                style: "{card} padding:11px 18px;margin-bottom:6px;display:flex;align-items:center;gap:14px;",
                                span { style: "font-family:{mono};font-size:11.5px;color:{ink3};flex:none;", "{c.short_hash}" }
                                span {
                                    style: "flex:1;min-width:0;font-size:13px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                                    "{c.subject}"
                                }
                                span { style: "font-size:11.5px;color:{ink2};flex:none;", "{c.author}" }
                                span { style: "font-size:11px;color:{ink3};flex:none;font-family:{mono};", "{c.date_label}" }
                            }
                        }
                    }
                },
            }
        }
    }
}

// ── issue board (R1) ──

/// The kanban status that follows `s` in the lifecycle, if any (terminal +
/// `Blocked` don't advance — they're a stop or a side state). Deliberately
/// forward-only: reopen/rewind stay API-only (A5-H leaves no UI for them —
/// settle-once is the safety net for the ones that ARE public).
/// An Issue's 「▶ 跑」 used to mint a brand-new `SessionId` on every click,
/// even when the issue already had a resumable session — `run_issue_interactive`
/// resumes the same underlying claude session via `claude_session_id` on the
/// claude_conversation row regardless of which `SessionId` the UI passes in, so the extra
/// records were purely cosmetic: a growing pile of dead-looking "阶段记录"
/// cards, all titled after the same issue, that the user can't tell apart or
/// delete. There's no `issue_id` column on `session` (a schema change this
/// fix doesn't need) — the de-dup key is the same `(stage_kind, title)` pair
/// `run_sess_title` already makes unique per issue, so reuse the existing
/// session id when one exists instead of minting another.
fn existing_issue_session(
    sessions: &[SessionCardVm],
    stage: StageKind,
    title: &str,
) -> Option<SessionId> {
    sessions
        .iter()
        .find(|s| s.stage_kind == Some(stage) && s.title == title)
        .map(|s| s.id)
}

/// Sidebar → same wake chain as board ▶跑/续聊 (Bug2 再发,2026-08-10).
/// Pure stage-playbook cards (title not `#N …`) keep SelectSession-only.
fn wake_session_like_board(
    k: &Kernel,
    sessions: &[SessionCardVm],
    issues: &[IssueVm],
    sid: SessionId,
    title: &str,
    stage_kind: Option<StageKind>,
) {
    let issue = issues
        .iter()
        .find(|i| format!("#{} {}", i.number, i.title) == title);
    let Some(issue) = issue else {
        if let Some(kind) = stage_kind {
            k.send(Command::SetScope(Scope::Stage(kind)));
        }
        k.send(Command::SetPanel(Panel::Workflow));
        k.send(Command::SelectSession(Some(sid)));
        return;
    };
    let stage = issue.stage;
    let sess_title = format!("#{} {}", issue.number, issue.title);
    let sess_id = existing_issue_session(sessions, stage, &sess_title).unwrap_or(sid);
    k.send(Command::StartSession {
        id: sess_id,
        stage_kind: Some(stage),
        kind: SessionKind::Create,
        title: sess_title,
    });
    k.send(Command::RunIssue {
        session: sess_id,
        id: issue.id,
    });
    k.send(Command::SetScope(Scope::Stage(stage)));
    k.send(Command::SetPanel(Panel::Workflow));
    k.send(Command::SelectSession(Some(sess_id)));
}

// ── progress · all ──

/// P9(项目编辑缺口): CRUD 里的 U 这一档——建完项目后,`name`/`kind`/`descr`
/// 此前连 store 层 setter 都没有(改个项目名只能删了重建,而删除会带走这个
/// 项目的全部 Issue/run/产物/记账);`benchmark`/`opportunity`/`cycle` 有
/// Command 但只在创建流(`create.rs`)可达,建完就够不着。这张卡把三组都接
/// 到运营面板「进度 · 全部」的常驻位置。
///
/// 重名口径:查过 `CreateProject`——它对项目名**不做任何唯一性校验**
/// (schema 里 `project.name` 没有 `UNIQUE`,命令层和 UI 层都没有查重;UI 唯一
/// 的门是 create.rs 的 `can_send = !name().trim().is_empty()`,只挡空名,不挡
/// 重名)。改名要和建项目同一口径,所以这里也不做重名校验——只有空名会被
/// `Command::UpdateProjectIdentity` 在 bw-app 命令层如实拒绝(`AppError::Invalid`),
/// 同 UI 门槛,但真正生效(UI 按钮 disabled 不是防线)。
#[component]
fn EditProjectCard(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let input_style = theme::input();
    let label_style = theme::label();

    let mut editing = use_signal(|| false);
    let mut name = use_signal(|| op.name.clone());
    let mut kind = use_signal(|| op.kind.clone());
    let mut desc = use_signal(|| op.desc.clone());
    let mut benchmark = use_signal(|| op.benchmark.clone());
    let mut opportunity = use_signal(|| op.opportunity.clone());
    let mut cycle = use_signal(|| op.cycle);
    // 起草(创建流)之后北极星的两个字段还能不能编,取决于这个项目有没有挂仓
    // ——见下方分叉的完整理由。
    let mut ns_value = use_signal(|| op.north_star.clone());
    let mut ns_def = use_signal(|| op.ns_def.clone());

    let has_repo = !op.remote_path.trim().is_empty();
    let can_save = !name().trim().is_empty();
    let opacity = if can_save { "1" } else { ".45" };

    let name0 = op.name.clone();
    let kind0 = op.kind.clone();
    let desc0 = op.desc.clone();
    let benchmark0 = op.benchmark.clone();
    let opportunity0 = op.opportunity.clone();
    let cycle0 = op.cycle;
    let ns_value0 = op.north_star.clone();
    let ns_def0 = op.ns_def.clone();

    if !editing() {
        rsx! {
            div {
                style: "{card} padding:14px 18px;margin-bottom:16px;display:flex;align-items:center;gap:12px;",
                div {
                    style: "flex:1;min-width:0;",
                    div {
                        style: "font-size:12.5px;color:{ink2};display:flex;align-items:center;gap:8px;",
                        span { style: "font-weight:600;", "{op.name}" }
                        span { style: "color:{ink3};", "{op.kind} · {op.cycle.label()}" }
                    }
                    if !op.desc.trim().is_empty() {
                        div {
                            style: "font-size:11.5px;color:{ink3};margin-top:3px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                            "{op.desc}"
                        }
                    }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:5px 12px;font-size:12px;flex:none;",
                    onclick: move |_| {
                        name.set(name0.clone());
                        kind.set(kind0.clone());
                        desc.set(desc0.clone());
                        benchmark.set(benchmark0.clone());
                        opportunity.set(opportunity0.clone());
                        cycle.set(cycle0);
                        ns_value.set(ns_value0.clone());
                        ns_def.set(ns_def0.clone());
                        editing.set(true);
                    },
                    "编辑项目"
                }
            }
        }
    } else {
        rsx! {
            div {
                style: "{card} padding:16px 18px;margin-bottom:16px;",
                div { style: "font-family:{theme::SERIF};font-size:15px;font-weight:600;margin-bottom:12px;", "编辑项目" }

                label { style: "{label_style}", "项目名" }
                input {
                    style: "{input_style} margin-bottom:10px;",
                    value: "{name}",
                    oninput: move |e| name.set(e.value()),
                }
                label { style: "{label_style}", "类型" }
                input {
                    style: "{input_style} margin-bottom:10px;",
                    value: "{kind}",
                    oninput: move |e| kind.set(e.value()),
                }
                label { style: "{label_style}", "一句话说明" }
                textarea {
                    style: "{input_style} min-height:48px;margin-bottom:14px;",
                    value: "{desc}",
                    oninput: move |e| desc.set(e.value()),
                }

                div {
                    style: "font-size:11px;color:{ink3};letter-spacing:.05em;margin-bottom:8px;",
                    "定位与机会"
                }
                label { style: "{label_style}", "对标竞品" }
                textarea {
                    style: "{input_style} min-height:48px;margin-bottom:10px;",
                    value: "{benchmark}",
                    oninput: move |e| benchmark.set(e.value()),
                }
                label { style: "{label_style}", "机会缺口 / 三个月成功标准" }
                textarea {
                    style: "{input_style} min-height:48px;margin-bottom:14px;",
                    value: "{opportunity}",
                    oninput: move |e| opportunity.set(e.value()),
                }

                div {
                    style: "font-size:11px;color:{ink3};letter-spacing:.05em;margin-bottom:8px;",
                    "项目所处周期"
                }
                div {
                    style: "display:flex;gap:6px;margin-bottom:14px;",
                    for opt in [MaturityPeriod::Explore, MaturityPeriod::Expand, MaturityPeriod::Mature] {
                        {
                            let sel = cycle() == opt;
                            let (bd, bg, fg) = if sel {
                                ("1.5px solid #C5654A", "#C5654A", "#fff")
                            } else {
                                ("1px solid #DDD5C5", "transparent", "#57534A")
                            };
                            rsx! {
                                div {
                                    key: "{opt.label()}",
                                    onclick: move |_| cycle.set(opt),
                                    style: "cursor:pointer;border:{bd};background:{bg};color:{fg};border-radius:15px;padding:6px 13px;font-size:12px;font-weight:500;",
                                    "{opt.label()}"
                                }
                            }
                        }
                    }
                }

                // 北极星:必须按有无仓分叉,不是漏做。
                //
                // 有仓项目(remote_path 非空)—— D1「产品信息正本在仓、BW=
                // 操作台+信息转化层」+ D5「指标正本机读:仓里 .bw/metrics.toml
                // 承载北极星+滞后+引领指标定义,merge 后才同步进 SQLite 作
                // 缓存」。这里的 north_star/ns_def 就是那份缓存,如果在这给
                // 一个编辑口,用户改的是缓存,不是正本——SQLite 和
                // metrics.toml 从此各说各话,两份正本打架,直接撞 D1。所以
                // 有仓项目只读展示 + 一句诚实提示指去仓里改,不给输入框。
                //
                // 无仓项目(remote_path 为空)—— 没有 metrics.toml 这回事,
                // SQLite 里的 north_star/ns_def 本身就是唯一正本(存量本地
                // 项目、纯本地项目一直如此)。不给编辑口就是死路——同
                // benchmark/opportunity/cycle 一样,只在创建流可达,建完就
                // 锁死。所以走 `Command::UpdateNorthStar` 的编辑口。
                div {
                    style: "font-size:11px;color:{ink3};letter-spacing:.05em;margin-bottom:8px;",
                    "北极星"
                }
                if has_repo {
                    div {
                        style: "background:{theme::CARD_ALT};border:1px solid {theme::BORDER};border-radius:8px;padding:10px 12px;margin-bottom:14px;",
                        div { style: "font-size:12.5px;color:{ink2};margin-bottom:4px;",
                            if op.north_star.trim().is_empty() { "(尚未同步)" } else { "{op.north_star}" }
                        }
                        if !op.ns_def.trim().is_empty() {
                            div { style: "font-size:11.5px;color:{ink3};margin-bottom:6px;", "{op.ns_def}" }
                        }
                        div { style: "font-size:11px;color:{ink3};",
                            "指标正本在仓里的 .bw/metrics.toml —— 改指标走 PR,和改代码同一道验收门,这里只读。"
                        }
                    }
                } else {
                    textarea {
                        style: "{input_style} min-height:40px;margin-bottom:8px;",
                        placeholder: "北极星(三个月成功标准)",
                        value: "{ns_value}",
                        oninput: move |e| ns_value.set(e.value()),
                    }
                    textarea {
                        style: "{input_style} min-height:40px;margin-bottom:14px;",
                        placeholder: "定义:怎么算、数据从哪来",
                        value: "{ns_def}",
                        oninput: move |e| ns_def.set(e.value()),
                    }
                }

                div {
                    style: "display:flex;gap:8px;",
                    button {
                        style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:7px;padding:6px 14px;font-size:12px;opacity:{opacity};",
                        disabled: !can_save,
                        onclick: move |_| {
                            k.send(Command::UpdateProjectIdentity {
                                name: name().trim().to_string(),
                                kind: kind().trim().to_string(),
                                descr: desc().trim().to_string(),
                            });
                            k.send(Command::UpdateBrief {
                                benchmark: benchmark().trim().to_string(),
                                opportunity: opportunity().trim().to_string(),
                            });
                            k.send(Command::SetCycle { cycle: cycle() });
                            if !has_repo {
                                k.send(Command::UpdateNorthStar {
                                    value: ns_value().trim().to_string(),
                                    def: ns_def().trim().to_string(),
                                });
                            }
                            editing.set(false);
                        },
                        "保存"
                    }
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid #E2DCCF;border-radius:7px;padding:6px 14px;font-size:12px;",
                        onclick: move |_| editing.set(false),
                        "取消"
                    }
                }
            }
        }
    }
}

/// Real-executor workspace config — a persistent strip at the top of
/// 「进度 · 全部」. Unconfigured (empty `workspace_path`) shows a plain
/// "未配置" state (every Issue run stays on the self-labelled mock
/// interactive executor); configured shows the
/// path + permission tier with a "修改" button. Not part of the creation
/// flow — the target directory is a post-creation, advanced, optional
/// capability.
#[component]
fn WorkspaceConfig(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let clay = theme::CLAY;
    let input_style = theme::input();

    let mut editing = use_signal(|| false);
    let mut path = use_signal(|| op.workspace_path.clone());
    let mut allow = use_signal(|| op.allow_commands);
    let configured = !op.workspace_path.trim().is_empty();

    if !editing() {
        let path0 = op.workspace_path.clone();
        let allow0 = op.allow_commands;
        let btn_label = if configured { "修改" } else { "配置" };
        let permission_label = if op.allow_commands {
            "可运行命令"
        } else {
            "仅编辑文件"
        };
        rsx! {
            div {
                style: "{card} padding:14px 18px;margin-bottom:16px;display:flex;align-items:center;gap:12px;",
                span { style: "font-size:12px;color:{ink3};flex:none;", "真执行工作目录" }
                if configured {
                    span {
                        style: "font-family:{mono};font-size:12.5px;color:{ink2};flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                        "{op.workspace_path}"
                    }
                    if !op.remote_path.trim().is_empty() {
                        span {
                            style: "font-size:11px;color:{ink3};flex:none;",
                            "GitHub · {op.remote_path}"
                        }
                    }
                    span { style: "font-size:11px;color:{ink3};flex:none;", "{permission_label}" }
                } else {
                    span { style: "font-size:12.5px;color:{ink3};flex:1;", "未配置 —— 「▶ 跑」目前始终为模拟执行" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:5px 12px;font-size:12px;flex:none;",
                    onclick: move |_| {
                        path.set(path0.clone());
                        allow.set(allow0);
                        editing.set(true);
                    },
                    "{btn_label}"
                }
            }
        }
    } else {
        rsx! {
            div {
                style: "{card} padding:14px 18px;margin-bottom:16px;",
                div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "配置后「▶ 跑」将真正读写这个目录下的文件 —— 路径必须已存在" }
                input {
                    style: "{input_style} width:100%;padding:6px 9px;font-size:12px;margin-bottom:8px;",
                    placeholder: "例如 /Users/you/projects/my-app(留空 = 清空配置,只跑模拟)",
                    value: "{path}",
                    oninput: move |e| path.set(e.value()),
                }
                button {
                    style: "cursor:pointer;background:transparent;border:none;padding:0;margin-bottom:10px;font-size:12px;color:{ink2};display:flex;align-items:center;gap:6px;",
                    onclick: move |_| allow.set(!allow()),
                    span { if allow() { "☑" } else { "☐" } }
                    "允许运行命令(不只编辑文件)"
                }
                div {
                    style: "display:flex;gap:8px;",
                    button {
                        style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:7px;padding:6px 14px;font-size:12px;",
                        onclick: move |_| {
                            k.send(Command::SetWorkspace {
                                path: path(),
                                allow_commands: allow(),
                            });
                            editing.set(false);
                        },
                        "保存"
                    }
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid #E2DCCF;border-radius:7px;padding:6px 14px;font-size:12px;",
                        onclick: move |_| editing.set(false),
                        "取消"
                    }
                }
            }
        }
    }
}

/// P1(loop-buddy↔aihot 接线 spec):给「绑定本地目录」建的存量项目补一个
/// 接入 GitHub 仓的入口 —— `CreateProject` 只有「新建仓」「克隆已有仓」两条
/// 路径会写 `remote_path`,绑定本地目录那条从不写,产品里此前没有补救
/// 入口。只在 `remote_path` 为空时渲染;`AttachRepo` dispatch(bw-app)
/// 把「接本地 origin」排在任何写库动作之前(P1-fix),所以只有真正接成
/// 才会写 `remote_path`、卡片才会随之消失 —— 半途失败(如本地 origin
/// 已指向别的仓)时一个字节都还没进库,卡片原样留在原地,用户可以就地
/// 重试,不会被冲进死路。真实网络调用(`gh repo view`),Started→Ok/Fail
/// 的进度靠既有 `ActionProgress` toast 显示,失败走通用 `UiNote::Error`
/// (`AttachRepo` 探活失败/本地 origin 不符时 dispatch 直接返回 `Err`)。
#[component]
fn AttachRepoCard(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let input_style = theme::input();

    let mut owner_repo = use_signal(String::new);
    let mut push_local = use_signal(|| false);
    let (owner, repo) = owner_repo()
        .split_once('/')
        .map(|(o, r)| (o.trim().to_string(), r.trim().to_string()))
        .unwrap_or_default();
    let can_send = !owner.is_empty() && !repo.is_empty();
    let has_workspace = !op.workspace_path.trim().is_empty();
    let opacity = if can_send { "1" } else { ".45" };

    rsx! {
        div {
            style: "{card} padding:14px 18px;margin-bottom:16px;",
            div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "接入仓库 —— 这个项目还没挂 GitHub 仓" }
            div {
                style: "display:flex;align-items:center;gap:8px;",
                input {
                    style: "{input_style} flex:1;padding:6px 9px;font-size:12px;",
                    placeholder: "owner/repo(例如 forcegravity1989/aihot)",
                    value: "{owner_repo}",
                    oninput: move |e| owner_repo.set(e.value()),
                }
                button {
                    style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:7px;padding:6px 14px;font-size:12px;opacity:{opacity};flex:none;",
                    disabled: !can_send,
                    onclick: move |_| {
                        k.send(Command::AttachRepo {
                            owner: owner.clone(),
                            repo: repo.clone(),
                            push_local: push_local(),
                        });
                        owner_repo.set(String::new());
                    },
                    "接入"
                }
            }
            if has_workspace {
                button {
                    style: "cursor:pointer;background:transparent;border:none;padding:0;margin-top:8px;font-size:12px;color:{ink3};display:flex;align-items:center;gap:6px;",
                    onclick: move |_| push_local.set(!push_local()),
                    span { if push_local() { "☑" } else { "☐" } }
                    "同时推送本地提交"
                }
            }
            div { style: "font-size:11px;color:{ink3};margin-top:8px;", "先真探活(gh repo view)——仓不存在或无权限,不写任何东西。" }
        }
    }
}

#[component]
fn ProgressAll(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let ink4 = theme::INK_4;
    let mono = theme::MONO;
    let border = theme::BORDER;
    let clay = theme::CLAY;
    let k_sync = k.clone();
    // PF1-R3 · 项目指标改卡片后,「↻ 立即采集」按钮从 R2-2 的 strip 挪到项目
    // 指标区头。对仗 ProgressStage C7 取法,发 Command::CollectMetrics(走老
    // handler lib.rs:6798,采集不是结算,不推 Issue 状态)。
    let k_collect = k.clone();

    // V1-Issue3 · split metrics: intrinsic (项目指标 strip) vs business (业务指标
    // 值卡). stage_kind.is_none() = project-level (北极星/L1/L2/L3 + codehub
    // 公共指标). Intrinsic ones are buddy's own code-stats, not user business
    // metrics — whitelist-driven (is_intrinsic_metric, name-based).
    //
    // main 合入(metric-archive):两条分流都排除已停用的行 —— 停用的语义是
    // 「退出界面默认视图 + 退出健康灯上卷 + 退出自动采集」,不滤掉的话它还
    // 会照常在滞后/引领区点灯,停用就等于没停。已停用的行走下方的
    // `archived_business` 折叠区。
    let intrinsic: Vec<MetricVm> = op
        .metrics
        .iter()
        .filter(|m| m.is_intrinsic && !m.archived)
        .cloned()
        .collect();
    let business: Vec<MetricVm> = op
        .metrics
        .iter()
        .filter(|m| !m.is_intrinsic && m.stage_kind.is_none() && !m.archived)
        .cloned()
        .collect();
    let archived_business: Vec<MetricVm> = op
        .metrics
        .iter()
        .filter(|m| m.stage_kind.is_none() && m.archived)
        .cloned()
        .collect();

    // North star: if a business metric matches the project's north_star name,
    // use it (might still be grey if no observations). Else the north star has
    // no metric row (v1 留白: collect 落 project 列非 metric 行) — render honest
    // grey. Either way the rendering is data-driven, not hardcoded.
    let ns_name = op.north_star.clone();
    let ns_def = op.ns_def.clone();
    let ns_metric: Option<MetricVm> = business.iter().find(|m| m.name == ns_name).cloned();
    let lagging: Vec<MetricVm> = business
        .iter()
        .filter(|m| !m.leading && m.name != ns_name)
        .cloned()
        .collect();
    let leading: Vec<MetricVm> = business
        .iter()
        .filter(|m| m.leading && m.name != ns_name)
        .cloned()
        .collect();

    // buddy 情况 — derived from existing op data (attention/week_review/issues).
    let in_review = op
        .issues
        .iter()
        .filter(|i| i.status == IssueStatus::InReview)
        .count();
    let stage_label = op.active_stage.label();
    let done_week = op.week_review.done_this_week;
    let open_count = op.week_review.open_count;
    let metrics_stale = op.week_review.metrics_stale;

    // PF1-R4 · 项目指标区定型:round 3 把 intrinsic 全量(5 阶段完成 +
    // 2 仓指标)都渲染成卡,阶段完成不该是大卡。改:只渲染 2 张代码仓
    // 卡(开放 Issue 数 / 已合入 MR 数),下方加一行小字显 active stage
    // 的阶段完成数(内联 div,不复活 strip 组件)。R4-2 的 spark 1 点
    // 显点让 1 周数据也能在折线显个点。
    let repo_metrics: Vec<MetricVm> = intrinsic
        .iter()
        .filter(|m| m.name == "开放 Issue 数" || m.name == "已合入 MR 数")
        .cloned()
        .collect();
    // PF1-R5 · 阶段完成一行五阶段(不只 active):所有阶段并排一行小字,
    // 值空显「—」。对仗旧 strip 多项一行口径,但不复活 strip 组件。
    let mut stage_done_rows = intrinsic
        .iter()
        .filter(|m| m.name == "阶段完成 Issue 数")
        .filter_map(|m| m.stage_kind.as_ref().map(|sk| (sk, m)))
        .collect::<Vec<_>>();
    stage_done_rows.sort_by_key(|(sk, _)| sk.index());
    let stage_done_txt = if stage_done_rows.is_empty() {
        "阶段完成:—".to_string()
    } else {
        let parts: Vec<String> = stage_done_rows
            .iter()
            .map(|(sk, m)| {
                let v = if m.value_raw.trim().is_empty() {
                    "—"
                } else {
                    m.value_raw.trim()
                };
                format!("{} {}", sk.label(), v)
            })
            .collect();
        format!("阶段完成:{} · 机器记", parts.join(" / "))
    };

    // ▾配置 collapse toggle — default collapsed (config is secondary on the
    // overview; the v2 layout surfaces metrics first, config behind a click).
    let mut config_open = use_signal(|| false);
    let cfg_arrow = if config_open() { "▴" } else { "▾" };
    let cfg_label = if config_open() {
        "收起 ▾"
    } else {
        "展开 ▸"
    };

    rsx! {
        // ═══ 1. 项目指标 · 代码仓级 (intrinsic · 卡片 · 不点健康灯) ═══
        // PF1-R3 · 原 strip(compact 小条)看不清、无 delta/趋势、无数据
        // 「—」像分隔符且全局样式垮。改用 BizMetricCard(对仗业务指标卡),
        // 复用数据层已有的 weekly_delta/weekly_spark。intrinsic 指标不点灯
        // (决议 a:代码仓 Issue/MR 是工程数,signal 恒 Unknown 无信息量)。
        // PF1-R4 见上方 repo_metrics/stage_done_txt 计算(移出 rsx:Dioxus
        // rsx! 块不接受 let 绑定)。
        if !repo_metrics.is_empty() {
            div {
                style: "{card} padding:20px 22px;margin-bottom:16px;",
                div {
                    style: "display:flex;justify-content:space-between;align-items:center;margin-bottom:4px;",
                    span { style: "font-family:{serif};font-size:16px;font-weight:600;", "项目指标 · 代码仓级" }
                    button {
                        style: "font-size:12px;color:{clay};border:1px solid {clay};border-radius:7px;padding:5px 12px;cursor:pointer;background:transparent;",
                        onclick: move |_| k_collect.send(Command::CollectMetrics),
                        "↻ 立即采集"
                    }
                }
                div { style: "font-size:12px;color:{ink3};margin-bottom:16px;", "只当现状数 · 不点健康灯 · 不参与项目健康派生" }
                div {
                    style: "display:grid;grid-template-columns:repeat(2,1fr);gap:12px;",
                    for m in repo_metrics.iter().cloned() {
                        BizMetricCard { key: "{m.name}", m, is_north_star: false }
                    }
                }
                // 阶段完成一行(内联小字,不复活 strip):只显 active stage 那条。
                div {
                    style: "margin-top:12px;font-size:12px;color:{ink3};font-family:{mono};",
                    "{stage_done_txt}"
                }
            }
        }

        // ═══ 2. 业务指标 section (北極星 → 滯後 → 引領, 值卡并排) ═══
        div {
            style: "{card} padding:20px 22px;margin-bottom:16px;",
            div {
                style: "display:flex;justify-content:space-between;align-items:center;margin-bottom:4px;",
                span { style: "font-family:{serif};font-size:16px;font-weight:600;", "业务指标" }
                // plan18-⑦ · ↻同步按钮: W2 Phase3 owns its removal; v2 总览
                // 暂保留在不碍眼处(偏差:HTML 原型显退场,按窗口边界留 W2)。
                button {
                    style: "font-size:12px;color:{ink2};border:1px solid {ink3};border-radius:4px;padding:3px 10px;cursor:pointer;background:transparent;",
                    onclick: move |_| k_sync.send(Command::SyncMetricsFile),
                    "↻ 同步指标文件"
                }
            }
            div { style: "font-size:12px;color:{ink3};margin-bottom:16px;", "北极星 → 滞后 → 引领 · 带健康灯 · 上卷项目健康" }

            // 北極星 (首卡 · 高亮 · 全宽)
            if !ns_name.is_empty() && ns_metric.is_some() {
                BizMetricCard { m: ns_metric.clone().unwrap(), is_north_star: true }
            }
            if !ns_name.is_empty() && ns_metric.is_none() {
                // North star has no metric row (v1 留白 · 窗口二未绑采数) → honest grey.
                div {
                    style: "background:#F0EDE5;border:1px dashed #C8C2B4;border-left:4px solid {clay};border-radius:10px;padding:18px 20px;margin-bottom:16px;",
                    div {
                        style: "display:flex;align-items:center;gap:8px;margin-bottom:8px;",
                        span { style: "{theme::dot(ui::signal_color(Signal::Unknown), 10)}" }
                        span { style: "font-family:{serif};font-size:16px;font-weight:600;", "★ {ns_name}" }
                    }
                    div {
                        style: "display:flex;align-items:baseline;gap:14px;margin-bottom:8px;",
                        span { style: "font-family:{serif};font-weight:700;font-size:20px;color:{ink4};", "—" }
                        span { style: "font-size:12px;color:{ink3};", "目标未设" }
                    }
                    div {
                        style: "font-size:12px;color:{ink3};",
                        span { "vs 上周 " }
                        span { style: "font-family:{mono};color:{ink4};", "—" }
                        span { "  无观测" }
                    }
                    div { style: "font-size:11px;color:{ink4};margin-top:8px;", "无观测 · Unknown≠绿 · 窗口二未绑采数" }
                    if !ns_def.is_empty() {
                        div { style: "font-size:11.5px;color:{ink3};margin-top:6px;line-height:1.6;", "{ns_def}" }
                    }
                }
            }

            // 滞后性指标 (结果型 · 看趋势不追本周)
            if !lagging.is_empty() {
                div { style: "font-family:{mono};font-size:10.5px;font-weight:600;letter-spacing:.08em;color:{clay};margin:14px 0 10px;", "滞后性指标 · 结果型 · 看趋势不追本周" }
                div {
                    style: "display:grid;grid-template-columns:repeat(2,1fr);gap:12px;",
                    for m in lagging.iter().cloned() {
                        BizMetricCard { key: "{m.name}", m, is_north_star: false }
                    }
                }
            }

            // 引领性指标 (定本周驱动项 · 计划指指标·指标验计划 · 本周计划折进卡里)
            if !leading.is_empty() {
                div { style: "font-family:{mono};font-size:10.5px;font-weight:600;letter-spacing:.08em;color:{clay};margin:14px 0 10px;", "引领性指标 · 定本周驱动项 · 计划指指标·指标验计划" }
                div {
                    style: "display:grid;grid-template-columns:repeat(2,1fr);gap:12px;",
                    for m in leading.iter().cloned() {
                        BizMetricCard { key: "{m.name}", m, is_north_star: false }
                    }
                }
            }

            // main 合入(metric-archive):已停用的指标不混在上面三段里,收进
            // 这个默认收起的折叠区。放在业务指标区末尾、与「引领」段平级 ——
            // git 自动合并把它塞进了 `if !leading.is_empty()` 内部,那样一个
            // 没有引领指标的项目就永远看不到自己停用过什么。折叠区内部用
            // main 的 `MetricCard` 渲染(带「恢复」按钮),不改 v2 的 BizMetricCard。
            ArchivedMetrics { metrics: archived_business.clone() }
        }

        // ═══ 3. buddy 情况 · 一行 (non-card · derived from existing data) ═══
        div {
            style: "margin:0 0 16px;padding:11px 14px;border-left:3px solid {clay};background:{theme::CARD_ALT};border-radius:0 8px 8px 0;font-size:13.5px;color:{ink2};display:flex;align-items:center;gap:10px;flex-wrap:wrap;",
            span { style: "font-family:{mono};font-size:10px;font-weight:600;letter-spacing:.06em;color:{ink3};", "buddy 情况" }
            span { "●{stage_label}阶段" }
            if in_review > 0 {
                span { style: "color:{ink3};", "·" }
                span { style: "font-family:{mono};font-weight:600;color:{clay};", "{in_review}" }
                span { "条 Issue 评审中待你 merge" }
            }
            if metrics_stale > 0 {
                span { style: "color:{ink3};", "·" }
                span { "{metrics_stale} 个指标本周未记·建议复盘" }
            }
            span { style: "color:{ink3};", "·" }
            span { "本周完成 " }
            span { style: "font-family:{mono};font-weight:600;color:{clay};", "{done_week}" }
            span { " / 开放 " }
            span { style: "font-family:{mono};font-weight:600;color:{clay};", "{open_count}" }
        }

        // ═══ 4. ▾配置 (collapsed by default · 收进次级) ═══
        div {
            style: "{card} overflow:hidden;",
            button {
                style: "width:100%;display:flex;align-items:center;gap:11px;padding:13px 18px;cursor:pointer;background:transparent;border:none;text-align:left;",
                onclick: move |_| config_open.set(!config_open()),
                span { style: "font-family:{mono};color:{ink3};font-size:14px;", "{cfg_arrow}" }
                span { style: "font-family:{serif};font-weight:600;font-size:15px;color:{ink2};", "配置" }
                span { style: "font-size:12px;color:{ink3};", "收进次级 · 总览是看指标的不是改配置的" }
                span { style: "margin-left:auto;font-family:{mono};font-size:12px;color:{ink3};", "{cfg_label}" }
            }
            if config_open() {
                div {
                    style: "border-top:1px solid {border};padding:14px 18px;",
                    EditProjectCard { op: op.clone() }
                    WorkspaceConfig { op: op.clone() }
                    if op.remote_path.trim().is_empty() {
                        AttachRepoCard { op: op.clone() }
                    }
                }
            }
        }
    }
}

// ── progress · stage ──

#[component]
fn Spark(spark: SparkPath, color: String, w: f32, h: f32) -> Element {
    let ink4 = theme::INK_4;
    if spark.polyline.is_empty() {
        return rsx! { span { style: "font-size:11px;color:{ink4};", "尚无观测" } };
    }
    rsx! {
        svg {
            width: "{w}",
            height: "{h}",
            view_box: "0 0 {w} {h}",
            path { d: "{spark.area}", fill: "{color}", opacity: "0.13" }
            polyline {
                points: "{spark.polyline}",
                fill: "none",
                stroke: "{color}",
                stroke_width: "1.6",
            }
            circle { cx: "{spark.last_x}", cy: "{spark.last_y}", r: "2.4", fill: "{color}" }
        }
    }
}

/// Card-sized weekly trend: y ticks, x week labels, value markers on points.
/// Replaces the tiny sparkline on BizMetricCard (Issue 3 走势可读走势).
#[component]
fn WeeklyTrendChart(chart: TrendChart, color: String) -> Element {
    let ink3 = theme::INK_3;
    let ink4 = theme::INK_4;
    let mono = theme::MONO;
    if chart.points.is_empty() {
        return rsx! {
            div {
                style: "height:120px;display:flex;align-items:center;justify-content:center;font-size:12px;color:{ink4};border:1px dashed #E2DCCF;border-radius:8px;",
                "尚无观测 · 折线空"
            }
        };
    }
    let w = chart.width;
    let h = chart.height;
    let plot_bottom = chart.plot_top + chart.plot_h;
    let plot_right = chart.plot_left + chart.plot_w;
    let grid = "#E8E2D6";
    rsx! {
        div {
            style: "width:100%;",
            div {
                style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:4px;",
                span { style: "font-family:{mono};font-size:11px;color:{ink3};letter-spacing:0.02em;", "按周走势" }
                span { style: "font-family:{mono};font-size:10px;color:{ink4};", "横轴=日期 · 纵轴=值" }
            }
            svg {
                width: "100%",
                height: "{h}",
                view_box: "0 0 {w} {h}",
                preserve_aspect_ratio: "xMidYMid meet",
                // Plot frame
                rect {
                    x: "{chart.plot_left}",
                    y: "{chart.plot_top}",
                    width: "{chart.plot_w}",
                    height: "{chart.plot_h}",
                    fill: "#FBF9F4",
                    stroke: "{grid}",
                    stroke_width: "1",
                }
                // Horizontal grid + y labels
                for (yi, (y, label)) in chart.y_ticks.iter().cloned().enumerate() {
                    g {
                        key: "yg{yi}",
                        line {
                            x1: "{chart.plot_left}",
                            y1: "{y}",
                            x2: "{plot_right}",
                            y2: "{y}",
                            stroke: "{grid}",
                            stroke_width: "1",
                            stroke_dasharray: "3 3",
                        }
                        text {
                            x: "{chart.plot_left - 6.0}",
                            y: "{y + 3.5}",
                            text_anchor: "end",
                            style: "font-family:{mono};font-size:10px;fill:{ink3};",
                            "{label}"
                        }
                    }
                }
                // Area + line
                if !chart.area.is_empty() {
                    path { d: "{chart.area}", fill: "{color}", opacity: "0.12" }
                }
                polyline {
                    points: "{chart.polyline}",
                    fill: "none",
                    stroke: "{color}",
                    stroke_width: "2.2",
                    stroke_linejoin: "round",
                    stroke_linecap: "round",
                }
                // Points + value labels + x labels
                for (i, p) in chart.points.iter().cloned().enumerate() {
                    g {
                        key: "pt{i}",
                        circle {
                            cx: "{p.x}",
                            cy: "{p.y}",
                            r: "3.2",
                            fill: "#FFFDF8",
                            stroke: "{color}",
                            stroke_width: "1.8",
                        }
                        text {
                            x: "{p.x}",
                            y: "{p.y - 8.0}",
                            text_anchor: "middle",
                            style: "font-family:{mono};font-size:10px;font-weight:600;fill:{color};",
                            "{p.value_label}"
                        }
                        text {
                            x: "{p.x}",
                            y: "{plot_bottom + 14.0}",
                            text_anchor: "middle",
                            style: "font-family:{mono};font-size:10px;fill:{ink3};",
                            "{p.x_label}"
                        }
                    }
                }
            }
        }
    }
}

/// Inline "record this week's value" form — the monitoring heartbeat.
#[component]
fn RecordInline(metric: MetricVm) -> Element {
    let k = use_context::<Kernel>();
    let mut val = use_signal(String::new);
    let input = theme::input();
    let clay = theme::CLAY;
    let id = metric.id;
    let send = move |_| {
        let v = val().trim().to_string();
        if !v.is_empty() {
            k.send(Command::RecordObservation {
                metric: id,
                value: v,
            });
            val.set(String::new());
        }
    };
    rsx! {
        div {
            style: "display:flex;gap:6px;margin-top:10px;",
            input {
                style: "{input} padding:6px 9px;font-size:12px;",
                placeholder: "记录本周值,如 6 / 58% / 5/7",
                value: "{val}",
                oninput: move |e| val.set(e.value()),
            }
            button {
                style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:7px;padding:6px 13px;font-size:12px;flex:none;",
                onclick: send,
                "记录"
            }
        }
    }
}

/// 「已停用 (N) ▾」折叠区 —— 停用的指标的唯一去处。默认收起(停用的本意
/// 就是别再占视线),展开后是灰显的卡片 + 每张卡上的「恢复」按钮,不需要
/// 开命令行改库。一条都没有时整个区段不渲染(不摆一个空抽屉)。
///
/// 这里刻意**不**给「彻底删除」:observation 表是 append-only 的,硬删
/// metric 行要么级联抹掉真实测量历史、要么留下孤儿观测。停用把「不想再看见
/// 它」和「它当初真测过什么」拆成两件事,两个都保住。
#[component]
fn ArchivedMetrics(metrics: Vec<MetricVm>) -> Element {
    let mut open = use_signal(|| false);
    let ink3 = theme::INK_3;
    let n = metrics.len();
    if n == 0 {
        return rsx! {};
    }
    let caret = if open() { "▾" } else { "▸" };
    rsx! {
        div {
            style: "margin-top:14px;border-top:1px dashed #ECE6DA;padding-top:10px;",
            button {
                style: "background:transparent;border:none;cursor:pointer;padding:2px 0;font-size:12px;color:{ink3};",
                onclick: move |_| open.toggle(),
                "{caret} 已停用 ({n})"
            }
            if open() {
                div {
                    style: "font-size:11px;color:{ink3};margin:8px 0 10px;line-height:1.6;",
                    "已停用的指标不计入项目健康、不再自动采集、不进本周计划;它们的历史观测一条未删,随时可恢复。"
                }
                div {
                    style: "display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px;",
                    for m in metrics.clone() {
                        MetricCard { key: "{m.name}", m }
                    }
                }
            }
        }
    }
}

#[component]
fn MetricCard(m: MetricVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let color = ui::signal_color(m.signal).to_string();
    let dot = theme::dot(&color, 9);
    let spark = m.spark.clone();
    // 停用态整卡压暗:它不参与项目健康、不再采集,视觉上不能和在用的指标
    // 抢注意力。灯还画着,但下面那行会讲清它是冻结值。
    let archived_css = if m.archived { "opacity:0.55;" } else { "" };
    let id = m.id;
    let archived = m.archived;
    let toggle = {
        let k = k.clone();
        move |_| {
            k.send(Command::SetMetricArchived {
                metric: id,
                archived: !archived,
            })
        }
    };
    // C7 · 采集来源徽记: label this metric's collection source. github is wired
    // in v1 (real gh pull); bw/connector read「v1 未接」and stay dimmed —
    // honest about what does and doesn't feed a real number yet. manual keeps
    // its existing 手填 badge below (a different axis: the latest value's source).
    let (collect_badge, collect_dim) = match m.collect_kind.as_str() {
        "github" => ("采集 · GitHub".to_string(), false),
        "script" => ("采集 · 项目侧脚本".to_string(), false),
        "bw" => ("采集 · BW 记账 · v1 未接".to_string(), true),
        "connector" => ("采集 · Connector · v1 未接".to_string(), true),
        _ => (String::new(), false),
    };
    let collect_dim_css = if collect_dim { "opacity:0.6;" } else { "" };
    rsx! {
        div {
            style: "{card} padding:16px 18px;{archived_css}",
            div {
                style: "display:flex;align-items:center;gap:8px;margin-bottom:8px;",
                span { style: "{dot}" }
                span { style: "font-size:13px;font-weight:500;", "{m.name}" }
                if !collect_badge.is_empty() {
                    span { style: "margin-left:auto;font-size:10.5px;color:{ink3};border:1px solid #E2DCCF;border-radius:6px;padding:1px 6px;{collect_dim_css}", "{collect_badge}" }
                }
                if m.manual {
                    span { style: "margin-left:auto;font-size:10.5px;color:{ink3};border:1px solid #E2DCCF;border-radius:6px;padding:1px 6px;", "手填 · 未接入度量源" }
                }
            }
            div {
                style: "display:flex;align-items:baseline;gap:10px;margin-bottom:8px;",
                span { style: "font-family:{mono};font-size:22px;font-weight:600;", "{m.value_raw}" }
                span { style: "font-size:12px;color:{ink3};", "目标 {m.target_raw}" }
            }
            Spark { spark, color, w: 120.0, h: 34.0 }
            if !m.def.is_empty() {
                div { style: "font-size:11.5px;color:{ink3};margin-top:8px;line-height:1.6;", "{m.def}" }
            }
            // 停用态:不给记录框(停用就是别再拿这条量了),换成一句如实
            // 说明 + 恢复入口。「冻结」两个字必须说出来 —— 上面那盏灯是停用
            // 那一刻的旧值,不是此刻重算的结果,不讲清就是在骗人。
            if m.archived {
                div {
                    style: "display:flex;align-items:center;gap:8px;margin-top:10px;",
                    span { style: "font-size:11px;color:{ink3};line-height:1.6;flex:1;",
                        "已停用 · 不计入项目健康、不再自动采集;上方信号为停用时刻的冻结值,历史观测一条未删。"
                    }
                    button {
                        style: "flex:none;font-size:11.5px;color:{ink3};border:1px solid #E2DCCF;border-radius:6px;padding:3px 10px;cursor:pointer;background:transparent;",
                        onclick: toggle,
                        "恢复"
                    }
                }
            } else {
                RecordInline { metric: m.clone() }
                // 停用按钮只给界面手建的指标。正本(.bw/metrics.toml)同步来
                // 的指标去留由正本说了算:从文件里删掉 → 下次同步自动停用;
                // 写回文件 → 自动恢复。这里再给个按钮只会被下次同步推翻,
                // 等于在界面上摆一个假开关。
                if m.from_file {
                    div { style: "font-size:11px;color:{ink3};margin-top:8px;line-height:1.6;",
                        "来自正本 .bw/metrics.toml · 从该文件里删掉这条并「↻ 同步指标文件」即停用"
                    }
                } else {
                    div {
                        style: "display:flex;justify-content:flex-end;margin-top:8px;",
                        button {
                            style: "font-size:11.5px;color:{ink3};border:1px solid #E2DCCF;border-radius:6px;padding:3px 10px;cursor:pointer;background:transparent;",
                            onclick: toggle,
                            "停用"
                        }
                    }
                }
            }
        }
    }
}

/// V1-Issue3 · v2 业务指标值卡 — 当前值+目标+信号灯 → delta上周变化 →
/// 按周折线. 北極星卡高亮(is_north_star=true). 引领卡额外带「本周目标+达成」
/// (本周计划折进卡). 无观测=grey+折线空+delta「—」(data-driven, not hardcoded).
#[component]
fn BizMetricCard(m: MetricVm, is_north_star: bool) -> Element {
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let ink4 = theme::INK_4;
    let mono = theme::MONO;
    let clay = theme::CLAY;
    let sig_color = ui::signal_color(m.signal).to_string();
    // PF1-R3 · 决议 a:intrinsic 指标(代码仓 Issue/MR/阶段完成)是工程数
    // 不是健康,signal 恒 Unknown 无信息量 → 不渲染信号灯 dot。其余
    // (值/delta/折线/collect 徽)照常。
    let show_dot = !m.is_intrinsic;
    let dot = if show_dot {
        theme::dot(&sig_color, 9)
    } else {
        String::new()
    };
    let has_obs = m.collection_chain.has_observation;

    // Weekly trend chart (readable axes + value markers · from observation weeks).
    // X labels = ISO week-end dates (MM-DD), same bucket as weekly_spark.
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let wk_chart = trend_chart(&m.weekly_spark, 320.0, 128.0, now_unix);
    let spark_color = sig_color.clone();

    // Delta: vs 上周 (green↑/red↓/grey—). None when <2 weeks of history.
    let (delta_txt, delta_clr, delta_arrow) = match m.weekly_delta {
        Some(d) if d > 0.0 => (format!("+{d:.1}"), "#5F7355", "↑"),
        Some(d) if d < 0.0 => (format!("{d:.1}"), "#B0503A", "↓"),
        Some(_) => ("0.0".into(), ink3, "→"),
        None => ("—".into(), ink4, ""),
    };

    // North star gets a clay left border + larger name/value.
    let ns_css = if is_north_star {
        format!("border-left:4px solid {clay};padding:18px 20px;")
    } else {
        String::new()
    };
    let grey_css = if !has_obs {
        "background:#F0EDE5;border:1px dashed #C8C2B4;".to_string()
    } else {
        String::new()
    };
    let name_size = if is_north_star { "16px" } else { "14px" };
    let val_size = if is_north_star { "24px" } else { "22px" };
    // PF1-R3-fixup: 无观测时值显「—」不显空(review Low · 用户要求#3),
    // 对所有指标一致(业务+项目),dashed 边框 + delta「—」已就位,值补齐。
    let val_display = if m.value_raw.trim().is_empty() {
        "—".to_string()
    } else {
        m.value_raw.clone()
    };

    // 引领卡: 本周目标 + 达成 ●/○ (folded week_plan — the card IS the plan).
    let hit_txt = match m.hit {
        Some(true) => "●",
        Some(false) => "○",
        None => "—",
    };
    let hit_clr = match m.hit {
        Some(true) => ui::signal_color(Signal::Green),
        Some(false) => ui::signal_color(Signal::Red),
        None => ink4,
    };

    // Collection chain — pre-compute strings to avoid if-let in rsx!.
    let conn_name = m
        .collection_chain
        .connector_name
        .clone()
        .unwrap_or_default();
    let has_tick = m.collection_chain.cron_last_tick.is_some();
    let show_chain = is_north_star || !has_obs;
    // 采集链尾段读派生出来的灯,不从「有没有观测」反推:有观测也可能仍是
    // Unknown(没设目标、过期降级),那时写「非 Unknown」就是 UI 自己在替
    // 健康下结论。
    let chain_tail = match m.signal {
        Signal::Unknown => "有观测 · 灯仍 Unknown",
        Signal::Green => "有观测 · 灯绿",
        Signal::Amber => "有观测 · 灯黄",
        Signal::Red => "有观测 · 灯红",
    };
    let collect_label = m.collection_chain.collect_label.clone();

    rsx! {
        div {
            style: "{card} {ns_css} {grey_css} padding:14px 16px;display:flex;flex-direction:column;gap:10px;",
            // Head: signal dot (intrinsic 指标不点灯)+ name (+ collect badge)
            div {
                style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                if show_dot {
                    span { style: "{dot}" }
                }
                span { style: "font-family:{serif};font-weight:600;font-size:{name_size};", "{m.name}" }
                if !collect_label.is_empty() && collect_label != "machine" {
                    span { style: "margin-left:auto;font-family:{mono};font-size:10.5px;color:{ink3};border:1px solid #E2DCCF;border-radius:4px;padding:1px 6px;", "{collect_label}" }
                }
                if m.manual {
                    span { style: "font-family:{mono};font-size:10.5px;color:#8A6720;border:1px solid #E2DCCF;border-radius:4px;padding:1px 6px;", "手填" }
                }
            }
            // Top: current value + target (+ leading: 本周目标 + 达成)
            div {
                style: "display:flex;align-items:baseline;gap:14px;flex-wrap:wrap;",
                span { style: "font-family:{serif};font-weight:700;font-size:{val_size};", "{val_display}" }
                if !m.target_raw.is_empty() {
                    span { style: "font-size:12px;color:{ink3};", "目标 {m.target_raw}" }
                } else {
                    span { style: "font-size:12px;color:{ink4};", "目标未设" }
                }
                if m.leading {
                    span {
                        style: "display:inline-flex;align-items:baseline;gap:5px;font-size:12px;",
                        span { style: "color:{ink3};", "本周目标" }
                        span { style: "font-family:{mono};font-weight:600;", "{m.last_target}" }
                    }
                    span {
                        style: "display:inline-flex;align-items:center;gap:5px;font-size:11.5px;color:{ink3};",
                        span { style: "width:9px;height:9px;border-radius:50%;border:1px solid rgba(0,0,0,.08);background:{hit_clr};opacity:0.55;" }
                        "{hit_txt} 达成"
                    }
                }
            }
            // Delta: vs 上周
            div {
                style: "display:flex;align-items:center;gap:6px;font-size:12.5px;",
                span { style: "color:{ink3};", "vs 上周" }
                span { style: "font-family:{mono};font-weight:600;color:{delta_clr};", "{delta_arrow} {delta_txt}" }
                if !has_obs {
                    span { style: "color:{ink4};", "  无观测" }
                }
            }
            // Weekly trend (8-week · observation ISO-week buckets)
            WeeklyTrendChart { chart: wk_chart, color: spark_color }
            // Collection chain footer (north star card or no-observation cards).
            // Honest: 无数据=Unknown≠绿.
            if show_chain {
                div {
                    style: "font-size:11px;color:{ink3};display:flex;align-items:center;gap:6px;flex-wrap:wrap;line-height:1.5;",
                    if !has_obs {
                        span { style: "color:{ink4};", "无观测 · Unknown≠绿" }
                    } else {
                        span { style: "font-weight:500;color:{ink2};", "采集链:" }
                        if !conn_name.is_empty() {
                            span { style: "font-family:{mono};font-size:10.5px;color:{clay};", "{conn_name}" }
                            span { "→" }
                        }
                        if has_tick {
                            span { style: "font-family:{mono};font-size:10.5px;color:{ink3};", "cron 有 tick" }
                        } else {
                            span { style: "font-family:{mono};font-size:10.5px;color:{ink4};", "cron 未跑" }
                        }
                        span { "→" }
                        span { "{chain_tail}" }
                    }
                }
            }
            if !m.def.is_empty() {
                div { style: "font-size:11.5px;color:{ink3};line-height:1.6;", "{m.def}" }
            }
            // P10(2026-08-06 cowelink 验证):总览业务卡此前看得到 manual
            // 指标(灰卡+「手填」徽)但填不了——手填入口只挂在旧 `MetricCard`
            // (阶段面板),v2 总览的 `BizMetricCard` 没有任何录入框。复用既有
            // `RecordInline`(同一份组件,阶段面板那份原样不动)。gate 在
            // `collect_kind`(这条指标的采集*计划*)而非 `m.manual`(最近一次
            // 观测的来源)——刻意如此:一条 manual 指标在**还没有任何观测**
            // 时 `manual` 是 false(没有"最近来源"这回事),但依然需要这个
            // 输入框才能填出第一条观测,否则永远死锁。停用的指标不给填(和
            // `MetricCard` 一致——停用就是别再拿这条量了)。北极星如果绑定了
            // 一条真实 metric 行(`ns_metric.is_some()`),也走的是这同一个
            // `BizMetricCard`,manual 时同样会出现这个框——不需要特殊处理。
            if m.collect_kind == "manual" && !m.archived {
                RecordInline { metric: m.clone() }
            }
        }
    }
}

#[component]
fn StageDetailCard(op: OpVm, s: StageVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let d = s.detail.clone();
    let (tint_bg, tint_fg, tint_bd) = ui::stage_tint(s.kind);
    let is_active_stage = op.active_stage == s.kind;

    let checked_all = d.dod_all_checked;
    let unchecked_labels: Vec<&'static str> = d
        .dod
        .iter()
        .filter(|x| !x.checked)
        .map(|x| x.label)
        .collect();
    let kind = s.kind;
    let handoff = {
        let k = k.clone();
        move |_| {
            let risky = !checked_all;
            let note = if risky {
                format!("带险交棒 · 未勾:{}", unchecked_labels.join("、"))
            } else {
                "交棒清单已勾满".to_string()
            };
            k.send(Command::HandoffStage { risky, note });
        }
    };

    rsx! {
        div {
            style: "{card} padding:20px 22px;margin-top:16px;",
            div {
                style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;margin-bottom:12px;",
                span { style: "font-family:{serif};font-size:15px;font-weight:600;", "{d.role}" }
                span { style: "font-size:10.5px;background:{tint_bg};color:{tint_fg};border:1px solid {tint_bd};border-radius:5px;padding:3px 8px;", "方法论 · {d.methodology}" }
                span { style: "margin-left:auto;font-family:{serif};font-size:15px;color:{d.color};", "{d.seek}" }
                span { style: "font-size:10.5px;color:{ink3};", "{d.cycle_rhythm}" }
            }
            div { style: "font-size:10.5px;color:{ink3};margin-bottom:6px;", "核心问题" }
            div { style: "font-family:{serif};font-size:14.5px;margin-bottom:14px;", "{d.core_question}" }

            div { style: "font-size:10.5px;color:{ink3};margin-bottom:8px;", "方法循环" }
            div {
                style: "display:flex;align-items:center;gap:6px;flex-wrap:wrap;margin-bottom:16px;",
                for (i, step) in d.method_loop.iter().enumerate() {
                    {
                        let is_last = i == d.method_loop.len() - 1;
                        rsx! {
                            span {
                                key: "{i}",
                                style: "background:{tint_bg};color:{tint_fg};border:1px solid {tint_bd};border-radius:6px;padding:6px 10px;font-size:12px;",
                                "{step}"
                            }
                            if !is_last {
                                span { style: "color:#C2BBAB;font-size:11px;", "→" }
                            }
                        }
                    }
                }
                span { style: "color:{d.color};font-size:13px;", "↺" }
            }

            div {
                style: "display:grid;grid-template-columns:1fr 1fr;gap:16px;margin-bottom:16px;",
                div {
                    div { style: "font-size:10.5px;color:{ink3};margin-bottom:8px;", "默认视图 · 引领焦点" }
                    div { style: "font-size:12.5px;color:{ink2};margin-bottom:3px;", "{d.default_view}" }
                    div { style: "font-size:12.5px;color:{ink2};", "{d.lead_focus}" }
                }
                div {
                    div { style: "font-size:10.5px;color:{ink3};margin-bottom:8px;", "AI 编队" }
                    for (name, def) in d.ai_crew.iter() {
                        div { key: "{name}", style: "font-size:12px;color:{ink2};margin-bottom:3px;",
                            span { style: "color:{d.color};font-weight:600;", "{name}" } " · {def}"
                        }
                    }
                }
            }

            div {
                style: "background:#23211C;border-radius:8px;padding:11px 14px;margin-bottom:16px;",
                span { style: "font-size:9.5px;letter-spacing:.08em;color:#E0A78F;margin-right:8px;", "反模式" }
                span { style: "font-size:11.5px;color:#C9BEB0;", "{d.anti_patterns}" }
            }

            div {
                style: "border-left:3px solid {d.color};background:{tint_bg};border-radius:8px;padding:14px 16px;",
                div {
                    style: "display:flex;align-items:baseline;gap:10px;margin-bottom:10px;",
                    span { style: "font-size:11px;letter-spacing:.06em;color:{tint_fg};font-weight:600;", "交棒清单 DoD" }
                    span { style: "font-size:11px;color:{ink3};", "已交棒 {d.handoff_count} 次" }
                }
                for (i, item) in d.dod.iter().enumerate() {
                    {
                        let (box_bg, box_bd, mark) = if item.checked {
                            (d.color, d.color, "✓")
                        } else {
                            ("transparent", "#CFC7B6", "")
                        };
                        let k = k.clone();
                        rsx! {
                            div {
                                key: "{i}",
                                onclick: move |_| k.send(Command::ToggleDod { stage_kind: kind, index: i }),
                                style: "cursor:pointer;display:flex;align-items:center;gap:10px;padding:4px 0;",
                                span { style: "width:16px;height:16px;border-radius:4px;border:1.5px solid {box_bd};background:{box_bg};color:#fff;font-size:10px;line-height:14px;text-align:center;flex:none;", "{mark}" }
                                span { style: "font-size:13px;color:#3A3833;", "{item.label}" }
                            }
                        }
                    }
                }
                if is_active_stage {
                    div {
                        style: "margin-top:14px;display:flex;align-items:center;gap:10px;",
                        button {
                            style: "cursor:pointer;background:{d.color};color:#fff;border:none;border-radius:7px;padding:9px 16px;font-size:12.5px;font-weight:600;",
                            onclick: handoff,
                            "{d.handoff_label}"
                        }
                        if !checked_all {
                            span { style: "font-size:11px;color:#B0503A;", "未勾满也可交棒 · 将记「带险交棒」" }
                        }
                    }
                } else {
                    div { style: "margin-top:12px;font-size:11.5px;color:{ink3};", "当前主持:{op.active_stage.role_short()} —— 只能从当前阶段交棒" }
                }
            }
        }
    }
}

#[component]
fn ProgressStage(op: OpVm, s: StageVm) -> Element {
    if s.kind == StageKind::Prototype {
        return rsx! { PrototypeProgress { op, s } };
    }
    rsx! { ProgressStageGeneric { op, s } }
}

/// Run Open Design discovery off the UI task. The dioxus Signal is unsync,
/// so the blocking work stays on a std thread and the result comes back
/// over a oneshot onto this runtime.
fn spawn_open_design_probe(
    mut url: dioxus::prelude::Signal<Option<String>>,
    mut probing: dioxus::prelude::Signal<bool>,
) {
    spawn(async move {
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::open_design::discover_web_url());
        });
        url.set(rx.await.ok().flatten());
        probing.set(false);
    });
}

/// Prototype progress: live Open Design home, collapsible DoD/handoff.
/// Empty metrics / manual % were unused after the overview refactor.
#[component]
fn PrototypeProgress(op: OpVm, s: StageVm) -> Element {
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let card = theme::card();
    let (chip_bg, chip_fg, _) = ui::stage_tint(s.kind);
    let chip = theme::chip(chip_bg, chip_fg);
    let url = use_signal(|| None::<String>);
    let mut probing = use_signal(|| true);
    let mut dod_open = use_signal(|| false);
    let mut probe_started = use_signal(|| false);
    if !probe_started() {
        probe_started.set(true);
        spawn_open_design_probe(url, probing);
    }
    let retry = move |_| {
        if probing() {
            return;
        }
        probing.set(true);
        spawn_open_design_probe(url, probing);
    };
    rsx! {
        div {
            style: "height:100%;min-height:0;display:flex;flex-direction:column;gap:8px;",
            div {
                style: "display:flex;align-items:center;gap:10px;flex:none;",
                span { style: "font-family:{serif};font-size:18px;font-weight:600;", "{s.n} {s.kind.label()}" }
                span { style: "{chip}", "{s.kind.role_short()}" }
                span { style: "font-size:12px;color:{ink3};", "Open Design · 首页" }
                button {
                    style: "margin-left:auto;cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:5px 12px;font-size:12px;",
                    disabled: probing(),
                    onclick: retry,
                    if probing() { "正在发现…" } else { "重新发现" }
                }
            }
            div {
                style: "flex:1;min-height:0;border:1px solid {theme::BORDER};border-radius:10px;overflow:hidden;background:#FFF;",
                if let Some(src) = url() {
                    iframe {
                        src: "{src}/",
                        style: "width:100%;height:100%;border:0;display:block;",
                        allow: "clipboard-read; clipboard-write; fullscreen",
                    }
                } else {
                    div {
                        style: "{card} height:100%;box-sizing:border-box;padding:28px 24px;border:none;box-shadow:none;",
                        div { style: "font-weight:600;margin-bottom:8px;",
                            if probing() { "正在寻找 Open Design" } else { "还没有接到 Open Design" }
                        }
                        p {
                            style: "color:{theme::INK_2};font-size:13px;margin:0 0 14px;line-height:1.7;",
                            if probing() {
                                "正在问本机已打开的 Open Design 要首页地址，马上就好。"
                            } else {
                                "本屏会嵌本机已打开的 Open Design 首页。请先打开 Open Design，再点「重新发现」。不另弹窗口；没接到就不假装嵌进去了。"
                            }
                        }
                    }
                }
            }
            div { style: "flex:none;",
                button {
                    style: "cursor:pointer;background:transparent;border:1px solid {theme::BORDER};border-radius:8px;padding:7px 12px;font-size:12px;color:{theme::INK_2};width:100%;text-align:left;",
                    onclick: move |_| dod_open.set(!dod_open()),
                    if dod_open() { "▾ 完成清单与交棒" } else { "▸ 完成清单与交棒" }
                }
                if dod_open() {
                    div {
                        style: "max-height:40vh;overflow-y:auto;",
                        StageDetailCard { op: op.clone(), s: s.clone() }
                    }
                }
            }
        }
    }
}

/// 单阶段进度视图的通用渲染器(构建/优化/运营推广/运维四个阶段共用);
/// 原型阶段走 `PrototypeProgress`(V3 内嵌 Open Design)。曾叫
/// `ProgressStageLegacy`——它不是遗留代码,是四个阶段的现役渲染器,2026-08-17 改名。
#[component]
fn ProgressStageGeneric(op: OpVm, s: StageVm) -> Element {
    let k = use_context::<Kernel>();
    // C7 · 立即采集: a manual pull entrance alongside the standard daily cron.
    // Cloned up front because `set_progress` below moves `k`.
    let k_collect = k.clone();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let input = theme::input();
    let clay = theme::CLAY;
    // 「该阶段还没有指标」的判据看的是**在用**的指标:全部停用等同于没有,
    // 不能因为抽屉里躺着几条已退役的行就假装这个阶段还有在测的东西。
    let live_metrics: Vec<MetricVm> = s.metrics.iter().filter(|m| !m.archived).cloned().collect();
    let archived_metrics: Vec<MetricVm> =
        s.metrics.iter().filter(|m| m.archived).cloned().collect();
    let empty = live_metrics.is_empty();
    let mut prog = use_signal(|| s.progress.to_string());
    let stage_kind = s.kind;
    let trend_spark = sparkline_path(&s.trend, 520.0, 74.0);
    let trend_color = ui::signal_color(s.health).to_string();
    let wow = match ui::wow_delta(&s.trend) {
        WowDir::Up => "↑ 较上次抬升",
        WowDir::Down => "↓ 较上次回落",
        WowDir::Flat => "→ 持平",
    };
    let (chip_bg, chip_fg, _) = ui::stage_tint(s.kind);
    let chip = theme::chip(chip_bg, chip_fg);
    let set_progress = move |_| {
        if let Ok(v) = prog().trim().parse::<u8>() {
            k.send(Command::SetStageProgress {
                stage_kind,
                progress: v.min(100),
            });
        }
    };
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:10px;margin-bottom:14px;",
            span { style: "font-family:{serif};font-size:18px;font-weight:600;", "{s.n} {s.kind.label()}" }
            span { style: "{chip}", "{s.kind.role_short()}" }
            span { style: "font-size:12px;color:{ink3};", "体检节奏 · {s.schedule_label}" }
            button {
                style: "margin-left:auto;cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:5px 12px;font-size:12px;",
                onclick: move |_| k_collect.send(Command::CollectMetrics),
                "立即采集"
            }
        }
        if empty {
            div {
                style: "{card} padding:20px 22px;margin-bottom:16px;",
                div { style: "font-weight:600;margin-bottom:6px;", "该阶段还没有指标" }
                p { style: "color:{ink2};font-size:13px;margin:0;line-height:1.7;",
                    "在此阶段运行工作流或记录一条观测,即可开始追踪 —— 无数据的阶段读作「无数据」,绝不冒充绿色。"
                }
            }
        } else {
            div {
                style: "display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px;margin-bottom:16px;",
                for m in live_metrics.clone() {
                    MetricCard { key: "{m.name}", m }
                }
            }
        }
        ArchivedMetrics { metrics: archived_metrics.clone() }
        div {
            style: "{card} padding:20px 22px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;margin-bottom:10px;",
                span { style: "font-family:{serif};font-size:15px;font-weight:600;", "进度趋势(手动维护的计划数据)" }
                span { style: "font-size:12px;color:{ink3};", "{wow}" }
            }
            Spark { spark: trend_spark, color: trend_color, w: 520.0, h: 74.0 }
            div {
                style: "display:flex;gap:8px;align-items:center;margin-top:12px;",
                span { style: "font-family:{mono};font-size:13px;", "{s.progress}%" }
                input {
                    style: "{input} width:110px;padding:6px 9px;font-size:12px;",
                    value: "{prog}",
                    oninput: move |e| prog.set(e.value()),
                }
                button {
                    style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:7px;padding:6px 13px;font-size:12px;",
                    onclick: set_progress,
                    "更新进度"
                }
                span { style: "font-size:11.5px;color:{ink3};", "0–100;每次更新都会追加到趋势史" }
            }
        }
        StageDetailCard { op, s }
    }
}

// ── workflow panel ──

#[component]
fn WorkflowPanel(op: OpVm, stage: Option<StageVm>, on_pick_hub: EventHandler<HubKind>) -> Element {
    match stage {
        None => rsx! {
            div {
                div { style: "font-weight:600;margin-bottom:4px;", "从 Hub 导入" }
                p { style: "color:{theme::INK_2};font-size:12.5px;line-height:1.7;margin:0 0 14px;",
                    "选中阶段轴上的任一阶段可查看其方法循环与历史记录;这里是三个可复用库的入口——沉淀过的工作流、可插拔技能、配置好的智能体。"
                }
                HubOverviewStrip { hub: op.hub.clone(), on_pick_hub }
            }
        },
        Some(s) => {
            // Dioxus 0.7: a lone child's `key` is ignored (diff_vcomponent
            // never reads it). Keyed `for` → Fragment → diff_keyed_children,
            // so SetScope remounts the stage tree (Issues↔Workflow heal).
            let stage_items = vec![s];
            let fill = if op.pty_active {
                "height:100%;min-height:0;display:flex;flex-direction:column;"
            } else {
                ""
            };
            rsx! {
                div {
                    style: "{fill}",
                    for s in stage_items {
                        {
                            let stage_key = format!("{:?}", s.kind);
                            rsx! {
                                WorkflowStage { key: "{stage_key}", op: op.clone(), s }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HubOverviewStrip(hub: crate::kernel::HubVm, on_pick_hub: EventHandler<HubKind>) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    rsx! {
        div {
            style: "display:grid;grid-template-columns:repeat(3,1fr);gap:14px;",
            for hc in hub.overview.clone() {
                {
                    let dot = theme::dot(&hc.color, 8);
                    let kind = hc.id;
                    rsx! {
                        div {
                            key: "{hc.name}",
                            style: "{card} padding:16px 18px;display:flex;flex-direction:column;",
                            div { style: "display:flex;align-items:center;gap:8px;margin-bottom:4px;",
                                span { style: "{dot}" }
                                span { style: "font-size:13.5px;font-weight:600;", "{hc.name}" }
                                span { style: "margin-left:auto;font-family:{theme::MONO};font-size:12px;color:{ink3};", "{hc.count}" }
                            }
                            div { style: "font-size:11.5px;color:{ink3};margin-bottom:8px;", "{hc.kind_label}" }
                            p { style: "color:{ink2};font-size:12px;line-height:1.6;margin:0 0 10px;flex:1;", "{hc.desc}" }
                            if !hc.items.is_empty() {
                                div { style: "display:flex;flex-wrap:wrap;gap:5px;margin-bottom:10px;",
                                    for (i , item) in hc.items.iter().enumerate() {
                                        span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", ink2)}", "{item}" }
                                    }
                                }
                            }
                            button {
                                style: "cursor:pointer;background:transparent;border:none;padding:0;font-size:12px;color:{theme::CLAY};text-align:left;",
                                onclick: move |_| on_pick_hub.call(kind),
                                "浏览并导入 →"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn WorkflowStage(op: OpVm, s: StageVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let spec_preview = stage_workflow(s.kind);
    let phases = spec_preview
        .phases
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(" → ");
    let goal = spec_preview.goal.clone();
    // V1-TermRefactor5 · 咨询态:焦点是 Done/InReview 续聊时,终端上方给「转成新活」。
    let consult_promote = op.focused_issue.and_then(|fid| {
        if !op.consultable_issues.contains(&fid) {
            return None;
        }
        op.issues
            .iter()
            .find(|i| i.id == fid)
            .map(|i| (i.stage, i.number, i.title.clone()))
    });
    // Real work starts exclusively from an Issue card's 「▶ 跑」
    // (`Command::RunIssue`) on the Issues panel — this stage view is
    // read-only: the method-loop preview, and the embedded terminal(s) of
    // whatever is running. (The old chat-style engine's transcript/banner
    // views were removed with that engine, 2026-08-18.)
    // Key includes stage + focus: cross-SetScope remounts the host; focus
    // flips remount the newly-visible widget onto a real-size div (same heal
    // as Issues↔Workflow). Unfocused peers stay mounted off-screen so PTY
    // byte pumps keep running.
    let stage_key = format!("{:?}", s.kind);
    let live_terms: Vec<(ConversationId, bool, String)> = if op.pty_active {
        op.pty_live_ids
            .iter()
            .map(|cid| {
                let focused = op.pty_conversation_id == Some(*cid);
                let focus_tag = if focused { "f" } else { "h" };
                let term_key = format!("{}-{stage_key}-{focus_tag}", cid.uuid());
                (*cid, focused, term_key)
            })
            .collect()
    } else {
        Vec::new()
    };
    let stage_fill = if op.pty_active {
        "display:flex;flex-direction:column;height:100%;min-height:0;"
    } else {
        ""
    };
    rsx! {
        div {
            style: "{stage_fill}",
            // V1-TermClose2 · UI 门控:方法循环卡(来自 stage_workflow)只在无终端
            // 会话(!pty_active)时显——issue 终端会话无 phase loop,显这张卡会误导。
            if !op.pty_active {
                div {
                    style: "{card} padding:18px 20px;margin-bottom:14px;",
                    div {
                        style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;",
                        span { style: "font-family:{serif};font-size:15px;font-weight:600;", "{spec_preview.name}" }
                    }
                    div { style: "font-size:12.5px;color:{ink2};margin-bottom:4px;", "方法循环:{phases}" }
                    div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "验收:{goal} · loop ≤3 迭代" }
                }
                div { style: "color:{ink3};font-size:12.5px;", "到「Issue」面板点「▶ 跑」开工——终端会出现在这里。" }
            }
            // 重启恢复:点卡到首包之间显示「恢复中…」(首包后 pty_restoring 清空)。
            if op.pty_restoring.is_some() {
                div {
                    style: "{card} padding:10px 14px;margin-bottom:10px;font-size:12.5px;color:{ink3};",
                    "恢复中…"
                }
            }
            // V1-TermRefactor5 · 咨询态:终端区显式「转成新活」(不做自动意图分类;不宣称只读)。
            if let Some((promote_stage, promote_number, promote_title)) = consult_promote {
                {
                    let k_new = k.clone();
                    rsx! {
                        div {
                            style: "display:flex;align-items:center;gap:10px;margin:0 0 8px;flex:none;",
                            span { style: "font-size:11.5px;color:{ink3};", "咨询中 · 新交付请另开一件活" }
                            button {
                                style: "cursor:pointer;border:1px solid {theme::BORDER};border-radius:7px;background:transparent;color:{theme::INK_2};padding:5px 12px;font-size:11.5px;",
                                title: "在同项目新建一件活,承接咨询里冒出的新交付诉求",
                                onclick: move |_| {
                                    k_new.send(Command::CreateIssue {
                                        id: IssueId::new(),
                                        stage: promote_stage,
                                        title: format!("来自咨询：{promote_title}"),
                                        desc: format!(
                                            "从 #{} 「{}」的咨询会话转来。",
                                            promote_number, promote_title
                                        ),
                                        priority: IssuePriority::Medium,
                                        standard_skill: String::new(),
                                    });
                                    k_new.send(Command::SetPanel(Panel::Issues));
                                },
                                "转成新活"
                            }
                        }
                    }
                }
            }
            // 多会话常驻:所有活 PTY 挂 xterm;仅焦点可见(隐藏的仍收字节)。
            for term in live_terms.iter() {
                {
                    let cid = term.0;
                    let is_focused = term.1;
                    let term_key = term.2.clone();
                    rsx! {
                        TerminalWidget {
                            key: "{term_key}",
                            conversation_id: cid,
                            focused: is_focused,
                        }
                    }
                }
            }
        }
    }
}

// ── routine panel ──

#[component]
fn RoutineAll(op: OpVm) -> Element {
    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    rsx! {
        div {
            style: "{card} padding:20px 22px;",
            div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:12px;", "定时任务(按阶段)" }
            for s in op.stages.clone() {
                {
                    let color = ui::signal_color(s.health);
                    let dot = theme::dot(color, 8);
                    let watch_count = s.metrics.len();
                    rsx! {
                        div {
                            key: "{s.n}",
                            style: "display:flex;align-items:center;gap:10px;border-bottom:1px dashed #ECE6DA;padding:9px 2px;",
                            span { style: "{dot}" }
                            span { style: "font-size:13px;min-width:130px;", "{s.n} {s.kind.label()}" }
                            span { style: "font-size:12px;color:{ink3};", "{s.schedule_label} · 盯 {watch_count} 项" }
                        }
                    }
                }
            }
            div { style: "font-size:11.5px;color:{ink3};margin-top:10px;",
                "真实定时喂数(Connector/Cron)属 Tier D;当前观测值经「记录」手填入库,链路一致。"
            }
        }
    }
}

#[component]
fn RoutineStage(s: StageVm) -> Element {
    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let amber = ui::signal_color(Signal::Amber);
    let red = ui::signal_color(Signal::Red);
    let watches: Vec<String> = s.metrics.iter().map(|m| m.name.clone()).collect();
    let empty_feed = s.feed.is_empty();
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:10px;margin-bottom:14px;",
            span { style: "font-family:{serif};font-size:18px;font-weight:600;", "{s.n} {s.kind.label()} · 观测" }
            span { style: "font-size:12px;color:{ink3};", "节奏 · {s.schedule_label}" }
        }
        div {
            style: "{card} padding:18px 20px;margin-bottom:14px;",
            div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "监测项" }
            if watches.is_empty() {
                span { style: "font-size:12.5px;color:{ink3};", "该阶段没有指标可盯 —— 先运行一次工作流或记录一条观测。" }
            }
            div {
                style: "display:flex;gap:8px;flex-wrap:wrap;",
                for w in watches {
                    span {
                        key: "{w}",
                        style: "border:1px solid #E2DCCF;border-radius:7px;padding:3px 10px;font-size:12px;",
                        "{w}"
                    }
                }
            }
        }
        div {
            style: "{card} padding:18px 20px;",
            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "观测流(真实记录,最新在前)" }
            if empty_feed {
                span { style: "font-size:12.5px;color:{ink3};", "还没有观测记录。在「进度 × 本阶段」的指标卡里记录本周值,这里会出现每一笔。" }
            }
            for (i, f) in s.feed.iter().enumerate() {
                {
                    let color = match f.level {
                        FeedLevel::Err => red,
                        FeedLevel::Warn => amber,
                        FeedLevel::Info => ink3,
                    };
                    let time = f.time_label.clone();
                    let text = f.text.clone();
                    rsx! {
                        div {
                            key: "{i}",
                            style: "display:flex;gap:10px;border-bottom:1px dashed #ECE6DA;padding:8px 2px;font-size:12.5px;",
                            span { style: "color:{ink3};min-width:64px;", "{time}" }
                            span { style: "color:{color};", "{text}" }
                        }
                    }
                }
            }
        }
    }
}

// ─── V1 终端会话重构·底座: per-conversation xterm (xterm.js) ───────────
