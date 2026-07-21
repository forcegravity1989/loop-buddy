//! `view=app` — the operating view: the real monitoring/run loop, now over
//! the five stage=role=methodology stages (体系重构 v2).
//!
//! Everything rendered here traces back to persisted rows: signals from the
//! derive cache, trends from observation history, feeds from real records,
//! chat transcripts from the message table, methodology text from
//! `StageKind`'s own static metadata. The two live loops:
//!
//! * **监控**: 记录观测值 → RecordObservation → recompute → 信号翻转可见;
//! * **运行**: 运行标准工作流 → MockExecutor 流式推进 → 阶段横幅实时更新 →
//!   产出落为会话消息(同事团队的真执行器经同一 trait 热插拔)。
//!
//! Plus the handoff loop: 勾 DoD → 交棒(可带险,永不静默拦截)→ 下一段自动换装,
//! `运维 → 原型` 回流闭环。

use crate::kernel::{ChatVm, Kernel, MsgVm, OpVm, RunVm, StageVm};
use crate::screens::chrome::Toast;
use crate::theme;
use bw_app::{Command, Panel, Scope};
use bw_core::model::{
    stage_workflow, FeedLevel, HubKind, HubSource, IssuePriority, IssueStatus, Signal, StageKind,
};
use bw_core::{IssueId, SessionId, SkillId, WorkflowId};
use bw_store::SessionKind;
use dioxus::prelude::*;
use ui::vm::{MetricVm, SessionCardVm, VersionLogVm};
use ui::{sparkline_path, SparkPath, WowDir};

#[component]
pub fn Op(op: OpVm, run: RunVm, on_pick_hub: EventHandler<HubKind>) -> Element {
    let paper = theme::PAPER;
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
                    style: "flex:1;min-width:0;overflow-y:auto;padding:18px 22px 40px;",
                    Center { op, run, on_pick_hub }
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
    rsx! {
        div { style: "font-size:11px;color:{ink3};letter-spacing:.06em;margin-bottom:8px;", "进行中 · 待你介入" }
        if needs_you.is_empty() {
            div { style: "font-size:12px;color:{ink3};line-height:1.7;", "没有进行中的会话——到「工作流」面板运行一轮标准工作流开始。" }
        }
        for s in needs_you {
            {
                let k = k.clone();
                let sid = s.id;
                let stage = s.stage_kind;
                let stage_label = stage.map(|x| x.label()).unwrap_or("项目");
                rsx! {
                    button {
                        key: "{s.title}",
                        style: "width:100%;text-align:left;background:{card_alt};border:1px solid #DBD4C5;border-radius:8px;padding:9px 10px;margin-bottom:7px;cursor:pointer;",
                        onclick: move |_| {
                            if let Some(kind) = stage {
                                k.send(Command::SetScope(Scope::Stage(kind)));
                            }
                            k.send(Command::SetPanel(Panel::Workflow));
                            k.send(Command::SelectSession(Some(sid)));
                        },
                        div { style: "font-size:12.5px;margin-bottom:3px;", "{s.title}" }
                        div { style: "font-size:11px;color:{ink3};", "{stage_label} · {s.status_label}" }
                    }
                }
            }
        }
    }
}

/// L2(plan/11): the health-signal half of the old `HealthOverview`, now a
/// card at the top of the 看板/进度 panel instead of a left-rail widget that
/// used to render on every panel regardless of relevance. Same data
/// (`op.attention`), same click-through (switch scope to the flagged stage).
#[component]
fn HealthOverviewCard(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let quiet = op.attention.watch.is_empty();
    rsx! {
        div {
            style: "{card} padding:16px 20px;margin-bottom:16px;",
            div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:10px;", "健康概览" }
            if quiet {
                div { style: "font-size:12.5px;color:{ink3};line-height:1.7;", "一切安静。绿色隐身,只有红黄出声。" }
            } else {
                div {
                    style: "display:flex;flex-wrap:wrap;gap:8px;",
                    for (kind , sig) in op.attention.watch.clone() {
                        {
                            let k = k.clone();
                            let color = ui::signal_color(sig);
                            let dot = theme::dot(color, 8);
                            let label = kind.label();
                            let sig_label = ui::vm::signal_label(sig);
                            rsx! {
                                button {
                                    key: "{kind.index()}",
                                    style: "text-align:left;background:transparent;border:1px solid #ECE6DA;border-radius:8px;padding:8px 12px;cursor:pointer;display:flex;align-items:center;gap:8px;",
                                    onclick: move |_| k.send(Command::SetScope(Scope::Stage(kind))),
                                    span { style: "{dot}" }
                                    span { style: "font-size:12.5px;", "{label}" }
                                    span { style: "font-size:11px;color:{ink3};", "{sig_label}" }
                                }
                            }
                        }
                    }
                }
            }
            div {
                style: "font-size:11px;color:{ink3};margin-top:12px;border-top:1px dashed #E2DCCF;padding-top:10px;",
                "{op.attention.steady} 个阶段平稳 · {op.archived} 条已归档"
            }
        }
    }
}

#[component]
fn StageSessions(op: OpVm) -> Element {
    let ink3 = theme::INK_3;
    let agent = theme::AGENT;
    let Scope::Stage(kind) = op.scope else {
        return rsx! { span {} };
    };
    let active_id = op.chat.as_ref().map(|c| c.id);
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
            div { style: "font-size:12px;color:{ink3};line-height:1.7;", "该阶段暂无记录。到「工作流」面板运行一轮标准工作流,记录会出现在这里。" }
        }
        if !creates.is_empty() {
            div { style: "font-size:11px;color:{ink3};margin:6px 0;", "创建" }
            for s in creates {
                SessionCard { s: s.clone(), selected: active_id == Some(s.id) }
            }
        }
        if !opts.is_empty() {
            div { style: "font-size:11px;color:{agent};margin:8px 0 6px;", "优化" }
            for s in opts {
                SessionCard { s: s.clone(), selected: active_id == Some(s.id) }
            }
        }
    }
}

#[component]
fn SessionCard(s: SessionCardVm, selected: bool) -> Element {
    let k = use_context::<Kernel>();
    let ink3 = theme::INK_3;
    let bd = if selected { theme::CLAY } else { "#DBD4C5" };
    let card_alt = theme::CARD_ALT;
    let sid = s.id;
    rsx! {
        button {
            style: "width:100%;text-align:left;background:{card_alt};border:1.4px solid {bd};border-radius:8px;padding:9px 10px;margin-bottom:7px;cursor:pointer;",
            onclick: move |_| {
                k.send(Command::SetPanel(Panel::Workflow));
                k.send(Command::SelectSession(Some(sid)));
            },
            div { style: "font-size:12.5px;margin-bottom:3px;", "{s.title}" }
            div { style: "font-size:11px;color:{ink3};", "{s.status_label}" }
        }
    }
}

// ───────────────────────── center ─────────────────────────

#[component]
fn Center(op: OpVm, run: RunVm, on_pick_hub: EventHandler<HubKind>) -> Element {
    let stage = match op.scope {
        Scope::Stage(kind) => op.stages.iter().find(|s| s.kind == kind).cloned(),
        Scope::All => None,
    };
    match (op.panel, stage) {
        (Panel::Progress, None) => rsx! { ProgressAll { op } },
        (Panel::Progress, Some(s)) => rsx! { ProgressStage { op, s } },
        (Panel::Workflow, s) => rsx! { WorkflowPanel { op, stage: s, run, on_pick_hub } },
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
fn next_issue_status(s: IssueStatus) -> Option<IssueStatus> {
    match s {
        IssueStatus::Backlog => Some(IssueStatus::Todo),
        IssueStatus::Todo => Some(IssueStatus::InProgress),
        IssueStatus::InProgress => Some(IssueStatus::InReview),
        IssueStatus::InReview => Some(IssueStatus::Done),
        IssueStatus::Done | IssueStatus::Blocked | IssueStatus::Cancelled => None,
    }
}

/// `true` for the three states `can_transition_to(Blocked)` actually allows
/// (bw-core's table) — only these get the "⛔ 阻塞" action.
fn can_block(s: IssueStatus) -> bool {
    matches!(
        s,
        IssueStatus::Todo | IssueStatus::InProgress | IssueStatus::InReview
    )
}

/// The Issue board (R1): real assignable work units grouped by status into
/// columns, each card carrying its stage + agent teammate + a one-click
/// advance to the next status. The create strip scopes a new issue to a
/// chosen stage. Every card is a real `issue` row — nothing invented.
///
/// A5-H adds: a real assign dropdown (was static text), a Blocked column
/// (previously invisible on the board — a stuck issue used to vanish from
/// view), and the only path to/from Blocked (reason required going in,
/// two explicit outs coming back). Cancelled stays off-board by design
/// (dropped work, not a state to manage from here).
#[component]
fn IssuesPanel(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let border = theme::BORDER;
    let ink = theme::INK;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let alert = theme::ALERT_DEEP;
    let mono = theme::MONO;
    let initial_stage = op.active_stage;
    let mut new_title = use_signal(String::new);
    let mut new_stage = use_signal(move || initial_stage);
    let agents = op.hub.agents.clone();
    // Board-wide: at most one card is "entering a block reason" at a time.
    // Fully qualified: `Signal` bare would resolve to `bw_core::model::Signal`
    // (the derived-health enum), already imported unqualified above.
    let mut blocking: dioxus::prelude::Signal<Option<IssueId>> = use_signal(|| None);
    let mut block_reason = use_signal(String::new);

    let cols: [(IssueStatus, &str); 6] = [
        (IssueStatus::Backlog, "待办池"),
        (IssueStatus::Todo, "待办"),
        (IssueStatus::InProgress, "进行中"),
        (IssueStatus::InReview, "评审中"),
        (IssueStatus::Done, "已完成"),
        (IssueStatus::Blocked, "阻塞"),
    ];
    // Precompute the columns outside rsx so the board stays borrow-clean.
    let grouped: Vec<_> = cols
        .iter()
        .map(|(st, label)| {
            (
                *label,
                op.issues
                    .iter()
                    .filter(|i| i.status == *st)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    rsx! {
        div { style: "max-width:1120px;",
            div {
                style: "{card} padding:12px 16px;margin-bottom:16px;display:flex;gap:10px;align-items:center;flex-wrap:wrap;",
                input {
                    value: "{new_title}",
                    placeholder: "新 Issue 标题(作用域到选中阶段)…",
                    style: "flex:1;min-width:220px;border:1px solid {border};border-radius:7px;padding:8px 11px;font-size:13px;background:#FFF;",
                    oninput: move |e| new_title.set(e.value()),
                }
                for s in StageKind::ALL {
                    {
                        let sel = new_stage() == s;
                        let (bg, fg) = if sel { (clay, "#FFF") } else { ("transparent", ink2) };
                        rsx! {
                            button {
                                key: "{s:?}",
                                style: "cursor:pointer;border:1px solid {border};border-radius:20px;background:{bg};color:{fg};padding:5px 12px;font-size:12px;",
                                onclick: move |_| new_stage.set(s),
                                "{s.label()}"
                            }
                        }
                    }
                }
                button {
                    style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:8px 16px;font-size:13px;flex:none;",
                    onclick: move |_| {
                        let t = new_title().trim().to_string();
                        if !t.is_empty() {
                            k.send(Command::CreateIssue {
                                id: IssueId::new(),
                                stage: new_stage(),
                                title: t,
                                desc: String::new(),
                                priority: IssuePriority::Medium,
                            });
                            new_title.set(String::new());
                        }
                    },
                    "＋ 创建 Issue"
                }
            }
            // P4: the evidence overlay — floats above the board while open.
            if let Some(d) = op.issue_detail.clone() {
                IssueDetailOverlay { d }
            }
            div { style: "display:flex;gap:12px;align-items:flex-start;",
                for (label, list) in grouped {
                    div { key: "{label}", style: "flex:1;min-width:190px;",
                        div { style: "font-size:11.5px;color:{ink3};margin-bottom:9px;letter-spacing:.04em;", "{label} · {list.len()}" }
                        for i in list {
                            {
                                // One clone per closure below — each `move`
                                // closure needs to independently own a
                                // `Kernel`, since only one of a card's several
                                // buttons ever fires but Rust still has to
                                // typecheck every branch.
                                let k_select = k.clone();
                                let k_a = k.clone();
                                let k_b = k.clone();
                                let k_run = k.clone();
                                let k_detail = k.clone();
                                let agents = agents.clone();
                                let i_id = i.id;
                                // P3: only work not yet under review / settled
                                // can be started from the board — same states
                                // `RunIssue` itself accepts (guard lives in
                                // bw-app; this just hides a doomed button).
                                let runnable = matches!(
                                    i.status,
                                    IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
                                );
                                let run_stage = i.stage;
                                let run_sess_title = format!("#{} {}", i.number, i.title);
                                let advance = next_issue_status(i.status);
                                let advance_label = advance.map(|s| s.label()).unwrap_or("");
                                let is_blocked = i.status == IssueStatus::Blocked;
                                let entering_reason = blocking() == Some(i_id);
                                rsx! {
                                    div {
                                        key: "{i.number}",
                                        style: "{card} padding:10px 12px;margin-bottom:9px;border-left:3px solid {i.status_color};",
                                        div { style: "font-size:11px;color:{ink3};font-family:{mono};", "#{i.number} · {i.stage.label()}" }
                                        // P4: the title opens the evidence
                                        // overlay (runs / diffs / artifacts).
                                        div {
                                            style: "font-size:13px;margin:3px 0 4px;color:{ink};cursor:pointer;",
                                            onclick: move |_| k_detail.send(Command::OpenIssueDetail(i_id)),
                                            "{i.title}"
                                        }
                                        div { style: "font-size:11px;color:{ink2};margin-bottom:5px;", "{i.priority_label}" }
                                        select {
                                            style: "font-size:11.5px;border:1px solid {border};border-radius:5px;padding:3px 5px;background:#FFF;max-width:100%;",
                                            onchange: move |e| {
                                                let v = e.value();
                                                let assignee = v
                                                    .parse::<usize>()
                                                    .ok()
                                                    .and_then(|idx| agents.get(idx))
                                                    .map(|a| a.id);
                                                k_select.send(Command::AssignIssue { id: i_id, assignee });
                                            },
                                            option { value: "", selected: i.assignee_name.is_none(), "未分配" }
                                            for (idx , a) in agents.iter().enumerate() {
                                                option {
                                                    key: "{idx}",
                                                    value: "{idx}",
                                                    selected: i.assignee_name.as_deref() == Some(a.name.as_str()),
                                                    "{a.name}({a.role})"
                                                }
                                            }
                                        }
                                        if is_blocked {
                                            div {
                                                style: "margin-top:7px;padding:6px 8px;background:#F2E4DD;border-radius:6px;font-size:11.5px;color:{alert};",
                                                "⛔ {i.blocked_reason.clone().unwrap_or_default()}"
                                            }
                                            div { style: "margin-top:6px;display:flex;gap:10px;",
                                                button {
                                                    style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;",
                                                    onclick: move |_| k_a.send(Command::TransitionIssue { id: i_id, status: IssueStatus::Todo }),
                                                    "解除→待办"
                                                }
                                                button {
                                                    style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;",
                                                    onclick: move |_| k_b.send(Command::TransitionIssue { id: i_id, status: IssueStatus::InProgress }),
                                                    "解除→进行中"
                                                }
                                            }
                                        } else if entering_reason {
                                            div { style: "margin-top:7px;",
                                                input {
                                                    value: "{block_reason}",
                                                    placeholder: "阻塞原因(必填)…",
                                                    style: "width:100%;font-size:11.5px;border:1px solid {border};border-radius:5px;padding:4px 7px;background:#FFF;",
                                                    oninput: move |e| block_reason.set(e.value()),
                                                }
                                                div { style: "margin-top:5px;display:flex;gap:10px;",
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{alert};font-size:11.5px;padding:0;",
                                                        onclick: move |_| {
                                                            let reason = block_reason().trim().to_string();
                                                            if !reason.is_empty() {
                                                                k_a.send(Command::BlockIssue { id: i_id, reason });
                                                                blocking.set(None);
                                                            }
                                                        },
                                                        "确认阻塞"
                                                    }
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:11.5px;padding:0;",
                                                        onclick: move |_| blocking.set(None),
                                                        "取消"
                                                    }
                                                }
                                            }
                                        } else {
                                            div { style: "margin-top:6px;display:flex;gap:12px;",
                                                // P3: really start the work —
                                                // same session+run path the
                                                // stage "▶ 运行" uses. Mock
                                                // projects run self-labeled.
                                                if runnable {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;font-weight:700;",
                                                        onclick: move |_| {
                                                            let sid = SessionId::new();
                                                            k_run.send(Command::StartSession {
                                                                id: sid,
                                                                stage_kind: Some(run_stage),
                                                                kind: SessionKind::Create,
                                                                title: run_sess_title.clone(),
                                                            });
                                                            k_run.send(Command::RunIssue { session: sid, id: i_id });
                                                        },
                                                        "▶ 跑"
                                                    }
                                                }
                                                if let Some(ns) = advance {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;",
                                                        onclick: move |_| k_a.send(Command::TransitionIssue { id: i_id, status: ns }),
                                                        "→ {advance_label}"
                                                    }
                                                }
                                                if can_block(i.status) {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:11.5px;padding:0;",
                                                        onclick: move |_| {
                                                            block_reason.set(String::new());
                                                            blocking.set(Some(i_id));
                                                        },
                                                        "⛔ 阻塞"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// P4: the Issue-detail overlay — the review gate's evidence surface. Every
/// number shown is a stored fact: real runs (status/duration/phases), the
/// files each run really changed (diff between its recorded HEAD pair), and
/// registered artifact versions. Nothing is synthesized; a missing record
/// says so instead of pretending "no changes". Actions dispatch the same
/// guarded commands the board uses — 「确认完成」 is the human's call, here
/// as everywhere.
#[component]
fn IssueDetailOverlay(d: ui::vm::IssueDetailVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let border = theme::BORDER;
    let ink = theme::INK;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let alert = theme::ALERT_DEEP;
    let mono = theme::MONO;
    let id = d.id;
    let k_close = k.clone();
    let k_done = k.clone();
    let k_back = k.clone();
    let k_run = k.clone();
    let k_distill = k.clone();
    let mut distilling = use_signal(|| false);
    let mut skill_name = use_signal(|| format!("{} · 做法", d.title));
    let mut skill_desc = use_signal(|| format!("来自 Issue #{} 的实战沉淀", d.number));
    let mut skill_content = use_signal(String::new);
    let runnable = matches!(
        d.status,
        IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
    );
    let in_review = d.status == IssueStatus::InReview;
    let done = d.status == IssueStatus::Done;
    let run_stage = d.stage;
    let run_sess_title = format!("#{} {}", d.number, d.title);
    let assignee = d.assignee_name.clone().unwrap_or_else(|| "未分配".into());

    rsx! {
        div {
            style: "position:fixed;inset:0;background:rgba(35,33,28,.38);z-index:60;display:flex;align-items:flex-start;justify-content:center;padding:48px 16px;",
            div {
                style: "{card} width:720px;max-width:96vw;max-height:82vh;overflow-y:auto;padding:18px 22px;",
                // ── header ──
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    div { style: "font-size:11.5px;color:{ink3};font-family:{mono};", "#{d.number} · {d.stage_label} · {d.status_label}" }
                    div { style: "flex:1;" }
                    button {
                        style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:14px;",
                        onclick: move |_| k_close.send(Command::CloseIssueDetail),
                        "✕"
                    }
                }
                div { style: "font-size:16px;color:{ink};margin:4px 0 2px;", "{d.title}" }
                div { style: "font-size:12px;color:{ink2};margin-bottom:6px;", "指派:{assignee} · {d.priority_label}" }
                if let Some(reason) = d.blocked_reason.clone() {
                    div { style: "margin:6px 0;padding:6px 9px;background:#F2E4DD;border-radius:6px;font-size:12px;color:{alert};", "⛔ {reason}" }
                }
                if !d.desc.trim().is_empty() {
                    div { style: "font-size:12.5px;color:{ink2};white-space:pre-wrap;margin:6px 0 10px;line-height:1.7;", "{d.desc}" }
                }

                // ── runs + real changes ──
                div { style: "font-size:12px;color:{ink3};letter-spacing:.05em;margin:12px 0 6px;", "运行史({d.runs.len()})" }
                if d.runs.is_empty() {
                    div { style: "font-size:12px;color:{ink3};", "还没有运行——「▶ 跑」会真实开工并留痕。" }
                }
                for (ri , r) in d.runs.iter().enumerate() {
                    div {
                        key: "{ri}",
                        style: "border:1px solid {border};border-radius:8px;padding:8px 11px;margin-bottom:8px;",
                        div { style: "font-size:12px;color:{ink};font-family:{mono};",
                            if r.ok {
                                span { style: "color:#5F7355;", "● {r.status_label}" }
                            } else {
                                span { style: "color:{alert};", "● {r.status_label}" }
                            }
                            span { style: "color:{ink3};", " · {r.trigger_label} · {r.duration_label} · {r.phases_label}" }
                        }
                        if !r.error.is_empty() {
                            div { style: "font-size:11.5px;color:{alert};margin-top:4px;white-space:pre-wrap;", "{r.error}" }
                        }
                        if let Some(why) = r.changes_unavailable.clone() {
                            div { style: "font-size:11.5px;color:{ink3};margin-top:5px;", "变更:{why}" }
                        } else if r.changes.is_empty() {
                            div { style: "font-size:11.5px;color:{ink3};margin-top:5px;", "变更:本次运行没有提交任何文件改动(如实)。" }
                        } else {
                            div { style: "margin-top:5px;",
                                for (ci , (path , add , del)) in r.changes.iter().enumerate() {
                                    div {
                                        key: "{ci}",
                                        style: "font-size:11.5px;font-family:{mono};color:{ink2};display:flex;gap:8px;",
                                        span { style: "flex:1;overflow:hidden;text-overflow:ellipsis;", "{path}" }
                                        span { style: "color:#5F7355;", "+{add}" }
                                        span { style: "color:{alert};", "-{del}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // ── artifacts ──
                div { style: "font-size:12px;color:{ink3};letter-spacing:.05em;margin:12px 0 6px;", "产物登记({d.artifacts.len()})" }
                if d.artifacts.is_empty() {
                    div { style: "font-size:12px;color:{ink3};", "尚无登记——确认完成时会扫描工作区并登记(带险不登)。" }
                }
                for (ai , (path , commit , bytes)) in d.artifacts.iter().enumerate() {
                    div {
                        key: "{ai}",
                        style: "font-size:11.5px;font-family:{mono};color:{ink2};display:flex;gap:10px;",
                        span { style: "flex:1;overflow:hidden;text-overflow:ellipsis;", "{path}" }
                        span { style: "color:{ink3};", "{commit} · {bytes}B" }
                    }
                }

                // ── actions(status-gated;same guarded commands as the board)──
                div { style: "display:flex;gap:14px;margin-top:16px;align-items:center;flex-wrap:wrap;",
                    if runnable {
                        button {
                            style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:7px 16px;font-size:12.5px;",
                            onclick: move |_| {
                                let sid = SessionId::new();
                                k_run.send(Command::StartSession {
                                    id: sid,
                                    stage_kind: Some(run_stage),
                                    kind: SessionKind::Create,
                                    title: run_sess_title.clone(),
                                });
                                k_run.send(Command::RunIssue { session: sid, id });
                                k_run.send(Command::OpenIssueDetail(id));
                            },
                            "▶ 跑"
                        }
                    }
                    if in_review {
                        button {
                            style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:7px 16px;font-size:12.5px;",
                            onclick: move |_| {
                                k_done.send(Command::TransitionIssue { id, status: IssueStatus::Done });
                                k_done.send(Command::OpenIssueDetail(id));
                            },
                            "✓ 确认完成(人裁)"
                        }
                        button {
                            style: "cursor:pointer;border:1px solid {border};border-radius:7px;background:transparent;color:{ink2};padding:7px 14px;font-size:12.5px;",
                            onclick: move |_| {
                                k_back.send(Command::TransitionIssue { id, status: IssueStatus::InProgress });
                                k_back.send(Command::OpenIssueDetail(id));
                            },
                            "↩ 打回"
                        }
                    }
                    if done && !distilling() {
                        button {
                            style: "cursor:pointer;border:1px solid {border};border-radius:7px;background:transparent;color:{clay};padding:7px 14px;font-size:12.5px;",
                            onclick: move |_| distilling.set(true),
                            "⚗ 蒸馏为技能"
                        }
                    }
                    if d.settled {
                        span { style: "font-size:11px;color:{ink3};", "已记账(同一件活绝不记两次)" }
                    }
                }

                // ── distill form(content is the human's judgment — required)──
                if distilling() {
                    div { style: "margin-top:12px;border-top:1px dashed {border};padding-top:12px;",
                        input {
                            value: "{skill_name}",
                            style: "width:100%;font-size:12.5px;border:1px solid {border};border-radius:6px;padding:6px 9px;background:#FFF;margin-bottom:6px;",
                            oninput: move |e| skill_name.set(e.value()),
                        }
                        input {
                            value: "{skill_desc}",
                            style: "width:100%;font-size:12.5px;border:1px solid {border};border-radius:6px;padding:6px 9px;background:#FFF;margin-bottom:6px;",
                            oninput: move |e| skill_desc.set(e.value()),
                        }
                        textarea {
                            value: "{skill_content}",
                            placeholder: "正文(必填,人写):这件活的可复用做法——下次同类活会被真实注入…",
                            style: "width:100%;min-height:110px;font-size:12.5px;border:1px solid {border};border-radius:6px;padding:8px 10px;background:#FFF;font-family:inherit;line-height:1.7;",
                            oninput: move |e| skill_content.set(e.value()),
                        }
                        div { style: "display:flex;gap:12px;margin-top:8px;",
                            button {
                                style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:6px 14px;font-size:12px;",
                                onclick: move |_| {
                                    let content = skill_content().trim().to_string();
                                    let name = skill_name().trim().to_string();
                                    if !content.is_empty() && !name.is_empty() {
                                        k_distill.send(Command::DistillSkillFromIssue {
                                            skill_id: SkillId::new(),
                                            issue_id: id,
                                            name,
                                            desc: skill_desc().trim().to_string(),
                                            category: "孵化沉淀".into(),
                                            content,
                                        });
                                        distilling.set(false);
                                    }
                                },
                                "确认蒸馏"
                            }
                            button {
                                style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:12px;",
                                onclick: move |_| distilling.set(false),
                                "取消"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ── progress · all ──

/// Real-executor workspace config — a persistent strip at the top of
/// 「进度 · 全部」. Unconfigured (empty `workspace_path`) shows a plain
/// "未配置" state (every run stays on `MockExecutor`); configured shows the
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
                    span { style: "font-size:11px;color:{ink3};flex:none;", "{permission_label}" }
                } else {
                    span { style: "font-size:12.5px;color:{ink3};flex:1;", "未配置 —— 「▶ 运行」目前始终为模拟执行" }
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
                div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "配置后「▶ 运行」将真正读写这个目录下的文件 —— 路径必须已存在" }
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

#[component]
fn ProgressAll(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let bar_color = ui::progress_color(op.overall);
    let overall = op.overall;
    let mix = op.cycle.mix();
    let stats = [
        ("工作流累计", op.stats.workflows_total),
        ("定时任务运行中", op.stats.routines_active),
        ("优化中待验收", op.stats.optimizing),
    ];
    let goal_color = if op.week_review.goal_negative {
        "#C5654A"
    } else {
        ink2
    };
    rsx! {
        // L2(plan/11): health belongs on the board, at the very top — the
        // number-one thing "整体进展" answers before anything else.
        HealthOverviewCard { op: op.clone() }
        // P5: weekly-review card — pure read of recorded facts, top of panel.
        div {
            style: "{card} padding:16px 20px;margin-bottom:16px;",
            div {
                style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:12px;",
                span { style: "font-family:{serif};font-size:16px;font-weight:600;", "本周复盘" }
                span { style: "font-size:12px;color:{ink3};", "{op.week_review.week_label}" }
            }
            div {
                style: "display:grid;grid-template-columns:repeat(4,1fr);gap:12px;",
                div {
                    div { style: "font-size:11px;color:{ink3};margin-bottom:4px;", "本周完成" }
                    div { style: "font-family:{mono};font-size:20px;font-weight:600;", "{op.week_review.done_this_week} 件" }
                }
                div {
                    div { style: "font-size:11px;color:{ink3};margin-bottom:4px;", "仍开着" }
                    div { style: "font-family:{mono};font-size:20px;font-weight:600;", "{op.week_review.open_count} 件" }
                }
                div {
                    div { style: "font-size:11px;color:{ink3};margin-bottom:4px;", "本周未记指标" }
                    div { style: "font-family:{mono};font-size:20px;font-weight:600;", "{op.week_review.metrics_stale} 个" }
                }
                div {
                    div { style: "font-size:11px;color:{ink3};margin-bottom:4px;", "90 天目标" }
                    div { style: "font-family:{mono};font-size:13px;font-weight:600;color:{goal_color};", "{op.week_review.goal_label}" }
                }
            }
            div {
                style: "font-size:11px;color:{ink3};margin-top:10px;",
                "全从已记录的数据算:本周结算的 Issue、未结 Issue、本周无观测的指标、距创建日 90 天。"
            }
        }
        WorkspaceConfig { op: op.clone() }
        div {
            style: "{card} padding:20px 22px;margin-bottom:16px;",
            div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:10px;",
                span { style: "font-family:{serif};font-size:16px;font-weight:600;", "总进度" }
                span { style: "font-family:{mono};font-size:14px;", "{overall}%" }
            }
            div {
                style: "height:8px;border-radius:4px;background:#E6E0D2;overflow:hidden;margin-bottom:10px;",
                div { style: "height:100%;width:{overall}%;background:{bar_color};" }
            }
            div {
                style: "display:flex;align-items:center;gap:8px;",
                span { style: "font-size:11px;color:{ink3};", "{op.cycle.label()} 配比" }
                div {
                    style: "flex:1;display:flex;height:6px;border-radius:3px;overflow:hidden;max-width:220px;",
                    for (i, sk) in StageKind::ALL.iter().enumerate() {
                        span { key: "{i}", style: "width:{mix[i]}%;background:{sk.color()};", "" }
                    }
                }
                span { style: "font-size:11px;color:{ink3};", "{op.cycle.main_loop_label()}" }
            }
            div { style: "font-size:11.5px;color:{ink3};margin-top:8px;", "各阶段进度的平均值;阶段进度在「进度 × 阶段」里手动维护 —— 它是计划数据,不是信号。" }
        }
        div {
            style: "display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-bottom:16px;",
            for (label, value) in stats {
                div {
                    key: "{label}",
                    style: "{card} padding:16px 18px;",
                    div { style: "font-size:12px;color:{ink3};margin-bottom:6px;", "{label}" }
                    div { style: "font-family:{mono};font-size:22px;font-weight:600;", "{value}" }
                }
            }
        }
        if !op.week_plan.is_empty() {
            div {
                style: "{card} padding:20px 22px;margin-bottom:16px;",
                div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:12px;", "本周计划" }
                div {
                    style: "display:grid;grid-template-columns:1.4fr .8fr .8fr .9fr 1.6fr .5fr;gap:8px;font-size:12px;color:{ink3};margin-bottom:6px;",
                    span { "引领指标" } span { "上周目标" } span { "当前值" } span { "本周目标" } span { "依据 / 抓手" } span { "达成" }
                }
                for row in op.week_plan.clone() {
                    {
                        let hit_txt = match row.hit {
                            Some(true) => "●",
                            Some(false) => "○",
                            None => "—",
                        };
                        let hit_color = match row.hit {
                            Some(true) => ui::signal_color(Signal::Green),
                            Some(false) => ui::signal_color(Signal::Red),
                            None => ink3,
                        };
                        rsx! {
                            div {
                                key: "{row.name}",
                                style: "display:grid;grid-template-columns:1.4fr .8fr .8fr .9fr 1.6fr .5fr;gap:8px;font-size:12.5px;align-items:center;margin-bottom:7px;",
                                span { "{row.name}" }
                                span { style: "font-family:{mono};color:{ink2};", "{row.last_target}" }
                                span { style: "font-family:{mono};", "{row.current}" }
                                span { style: "font-family:{mono};", "{row.target}" }
                                span { style: "color:{ink2};", "{row.driver}" }
                                span { style: "color:{hit_color};", "{hit_txt}" }
                            }
                        }
                    }
                }
            }
        }
        div {
            style: "{card} padding:20px 22px;",
            div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:12px;", "阶段" }
            for s in op.stages.clone() {
                {
                    let k = k.clone();
                    let color = ui::signal_color(s.health);
                    let dot = theme::dot(color, 8);
                    let (chip_bg, chip_fg, _) = ui::stage_tint(s.kind);
                    let chip = theme::chip(chip_bg, chip_fg);
                    let bar = ui::progress_color(s.progress);
                    let kind = s.kind;
                    let progress = s.progress;
                    let n = s.n;
                    let role = s.detail.role;
                    rsx! {
                        button {
                            key: "{n}",
                            style: "width:100%;display:grid;grid-template-columns:24px 1.4fr 130px 1fr 60px;gap:10px;align-items:center;background:transparent;border:none;border-bottom:1px dashed #ECE6DA;padding:10px 2px;cursor:pointer;text-align:left;",
                            onclick: move |_| {
                                k.send(Command::SetScope(Scope::Stage(kind)));
                                k.send(Command::SetPanel(Panel::Workflow));
                            },
                            span { style: "{dot}" }
                            span { style: "font-size:13px;", "{n} {kind.label()}" }
                            span { style: "{chip}", "{role}" }
                            div {
                                style: "height:5px;border-radius:3px;background:#E6E0D2;overflow:hidden;",
                                div { style: "height:100%;width:{progress}%;background:{bar};" }
                            }
                            span { style: "font-family:{mono};font-size:12px;color:{ink2};text-align:right;", "{progress}%" }
                        }
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

#[component]
fn MetricCard(m: MetricVm) -> Element {
    let card = theme::card();
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let color = ui::signal_color(m.signal).to_string();
    let dot = theme::dot(&color, 9);
    let spark = m.spark.clone();
    rsx! {
        div {
            style: "{card} padding:16px 18px;",
            div {
                style: "display:flex;align-items:center;gap:8px;margin-bottom:8px;",
                span { style: "{dot}" }
                span { style: "font-size:13px;font-weight:500;", "{m.name}" }
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
            RecordInline { metric: m.clone() }
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
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let input = theme::input();
    let clay = theme::CLAY;
    let empty = s.metrics.is_empty();
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
                for m in s.metrics.clone() {
                    MetricCard { key: "{m.name}", m }
                }
            }
        }
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
fn WorkflowPanel(
    op: OpVm,
    stage: Option<StageVm>,
    run: RunVm,
    on_pick_hub: EventHandler<HubKind>,
) -> Element {
    match stage {
        None => rsx! {
            div {
                div { style: "font-weight:600;margin-bottom:4px;", "从 Hub 导入" }
                p { style: "color:{theme::INK_2};font-size:12.5px;line-height:1.7;margin:0 0 14px;",
                    "选中阶段轴上的任一阶段可运行其内置标准工作流;这里是三个可复用库的入口——沉淀过的工作流、可插拔技能、配置好的智能体。"
                }
                HubOverviewStrip { hub: op.hub.clone(), on_pick_hub }
            }
        },
        Some(s) => rsx! { WorkflowStage { op, s, run } },
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

/// Live run visualization: a real step-track (not a flat pill row) fed
/// purely by `RunVm` — every node/line color is derived from real
/// `PhaseStarted`/`PhaseCompleted`/`RunFailed` facts, plus the real
/// `AgentRef`/`SkillRef` crew `RunStarted` announced for this run (empty is
/// an honest "this run declared none", not a placeholder).
#[component]
fn RunBanner(run: RunVm) -> Element {
    let card_alt = theme::CARD_ALT;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    if run.phases.is_empty() {
        return rsx! { span {} };
    }
    let status = if let Some(e) = &run.failed {
        format!("运行失败:{e}")
    } else if run.running {
        "运行中…".to_string()
    } else {
        "本轮完成 · 产出已写入会话".to_string()
    };
    let workflow_name = run.workflow_name.clone();
    rsx! {
        div {
            style: "background:{card_alt};border:1px solid #DBD4C5;border-radius:10px;padding:14px 16px;margin-bottom:14px;",
            div {
                style: "display:flex;align-items:baseline;gap:8px;margin-bottom:10px;",
                if !workflow_name.is_empty() {
                    span { style: "font-size:12.5px;font-weight:600;", "{workflow_name}" }
                }
                span { style: "font-size:12px;color:{ink3};", "{status}" }
            }
            PhaseTrack { run: run.clone() }
            if !run.agents.is_empty() || !run.skills.is_empty() {
                div {
                    style: "display:flex;flex-wrap:wrap;gap:5px;margin-top:12px;padding-top:10px;border-top:1px dashed {theme::BORDER};",
                    for (i , a) in run.agents.iter().enumerate() {
                        span {
                            key: "ag{i}",
                            title: "{a.def}",
                            style: "{theme::chip(\"#EDE8F5\", theme::AGENT)}",
                            "◆ {a.name}"
                        }
                    }
                    for (i , s) in run.skills.iter().enumerate() {
                        span {
                            key: "sk{i}",
                            title: "{s.def}",
                            style: "{theme::chip(\"#EFE9DA\", ink2)}",
                            "🧩 {s.name}"
                        }
                    }
                }
            }
        }
    }
}

/// A numbered step-track: circular phase badges connected by a progress
/// line, colored by real status — done (✓ green) / running (● clay,
/// pulsing) / failed (✕ red, only the phase that was in flight when it
/// failed) / pending (○ gray, hasn't started).
#[component]
fn PhaseTrack(run: RunVm) -> Element {
    let ink2 = theme::INK_2;
    let green = ui::signal_color(Signal::Green);
    let red = ui::signal_color(Signal::Red);
    let clay = theme::CLAY;
    let gray = "#D8D2C4";
    let current_idx = run.phases.iter().position(|(_, done)| !done);
    let n = run.phases.len();

    rsx! {
        div {
            style: "display:flex;align-items:flex-start;width:100%;",
            for (i, (name, done)) in run.phases.iter().enumerate() {
                {
                    let is_current = current_idx == Some(i);
                    let failed_here = is_current && run.failed.is_some();
                    let (badge_bg, badge_border, badge_fg, mark): (&str, &str, &str, String) = if *done {
                        ("#FFFDF8", green, green, "✓".into())
                    } else if failed_here {
                        ("#FFFDF8", red, red, "✕".into())
                    } else if is_current {
                        (clay, clay, "#FFF", (i + 1).to_string())
                    } else {
                        ("#FFFDF8", gray, "#B4AD9C", (i + 1).to_string())
                    };
                    let prev_done = i > 0 && run.phases[i - 1].1;
                    let left_color = if prev_done { green } else { gray };
                    let right_color = if *done { green } else { gray };
                    rsx! {
                        div {
                            key: "{i}",
                            style: "display:flex;flex-direction:column;align-items:center;flex:1;min-width:0;",
                            div {
                                style: "display:flex;align-items:center;width:100%;",
                                div {
                                    style: if i == 0 { "flex:1;height:2px;background:transparent;".to_string() } else { format!("flex:1;height:2px;background:{left_color};") },
                                }
                                div {
                                    style: "width:24px;height:24px;border-radius:50%;display:flex;align-items:center;justify-content:center;font-size:11px;font-weight:700;flex:none;background:{badge_bg};color:{badge_fg};border:2px solid {badge_border};",
                                    "{mark}"
                                }
                                div {
                                    style: if i + 1 == n { "flex:1;height:2px;background:transparent;".to_string() } else { format!("flex:1;height:2px;background:{right_color};") },
                                }
                            }
                            span {
                                style: "font-size:10px;color:{ink2};margin-top:5px;text-align:center;padding:0 2px;line-height:1.3;",
                                "{name}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn WorkflowStage(op: OpVm, s: StageVm, run: RunVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let primary = theme::btn_primary();
    let spec_preview = stage_workflow(s.kind);
    let phases = spec_preview.phases.join(" → ");
    let goal = spec_preview.goal.clone();
    let stage_kind = s.kind;
    let round = op
        .sessions
        .iter()
        .filter(|x| x.stage_kind == Some(stage_kind))
        .count()
        + 1;
    let running = run.running;
    let launch = {
        let k = k.clone();
        move |_| {
            if running {
                return;
            }
            let sid = SessionId::new();
            k.send(Command::StartSession {
                id: sid,
                stage_kind: Some(stage_kind),
                kind: SessionKind::Create,
                title: format!("{} · 第{}轮", stage_kind.label(), round),
            });
            k.send(Command::SelectSession(Some(sid)));
            // The kernel assembles this stage's playbook (role instructions +
            // real project context) — the UI only names the stage.
            k.send(Command::RunStagePlaybook {
                session: sid,
                stage_kind,
            });
        }
    };
    let mut promoted_msg = use_signal(|| None::<String>);
    let promote = {
        let k = k.clone();
        let session = op.chat.as_ref().map(|c| c.id);
        move |_| {
            let Some(session) = session else {
                return;
            };
            k.send(Command::PromoteWorkflow {
                new_id: WorkflowId::new(),
                session,
                source: HubSource::SelfBuilt,
            });
            promoted_msg.set(Some("已沉淀为静态工作流 → WorkflowHub".into()));
        }
    };
    let chat_area = match op.chat.clone() {
        Some(chat) => rsx! { Chat { chat } },
        None => rsx! {
            div { style: "color:{ink3};font-size:12.5px;", "运行一轮,或从左栏选择一条会话查看记录。" }
        },
    };
    rsx! {
        RunBanner { run: run.clone() }
        div {
            style: "{card} padding:18px 20px;margin-bottom:14px;",
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;",
                span { style: "font-family:{serif};font-size:15px;font-weight:600;", "{spec_preview.name}" }
                {
                    let opacity = if running { ".5" } else { "1" };
                    let run_label = if running {
                        "运行中…"
                    } else if run.failed.is_some() {
                        "↻ 重新运行"
                    } else {
                        "▶ 运行"
                    };
                    rsx! {
                        button {
                            style: "{primary} padding:8px 18px;font-size:13px;opacity:{opacity};",
                            disabled: running,
                            onclick: launch,
                            "{run_label}"
                        }
                    }
                }
            }
            if op.workspace_path.trim().is_empty() {
                div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "当前未配置工作目录 → 本轮仍为模拟执行" }
            }
            div { style: "font-size:12.5px;color:{ink2};margin-bottom:4px;", "方法循环:{phases}" }
            div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "验收:{goal} · loop ≤3 迭代" }
            if op.chat.is_some() {
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:5px 12px;font-size:11.5px;",
                    onclick: promote,
                    "↑ 沉淀为静态"
                }
            }
        }
        RunOutputs {
            phases: run.phases.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>(),
            msgs: op.chat.as_ref().map(|c| c.msgs.clone()).unwrap_or_default(),
        }
        {chat_area}
        if let Some(msg) = promoted_msg() {
            Toast { msg, onclose: move |_| promoted_msg.set(None) }
        }
    }
}

/// "结果呈现": pairs the run's real phase names (from `RunVm`, so it reflects
/// whatever actually ran — the stage's own template, an imported hub
/// workflow, or an ad-hoc dynamic one — not just the stage's default
/// preview) with the real `Role::Agent` session messages, in order. A
/// best-effort zip (agent messages are appended in completion order, one per
/// phase, by `run_workflow_inner`) — honestly labeled as such, not a hard
/// per-phase binding the store actually tracks.
#[component]
fn RunOutputs(phases: Vec<String>, msgs: Vec<MsgVm>) -> Element {
    let agent_msgs: Vec<&MsgVm> = msgs.iter().filter(|m| m.agent).collect();
    if agent_msgs.is_empty() {
        return rsx! {};
    }
    let card = theme::card();
    let ink3 = theme::INK_3;
    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:14px;",
            div { style: "font-size:12.5px;font-weight:600;margin-bottom:2px;", "产出" }
            div { style: "font-size:10.5px;color:{ink3};margin-bottom:10px;", "按完成顺序把每条 agent 产出与对应阶段配对(最佳努力对齐)" }
            for (i , m) in agent_msgs.iter().enumerate() {
                {
                    let phase_label = phases.get(i).cloned().unwrap_or_else(|| format!("第{}步", i + 1));
                    let text = m.text.clone();
                    rsx! {
                        div {
                            key: "{i}",
                            style: "margin-bottom:10px;padding-bottom:10px;border-bottom:1px dashed {theme::BORDER};",
                            div { style: "font-size:11px;color:{theme::CLAY};font-weight:600;margin-bottom:4px;", "{i + 1}. {phase_label}" }
                            div { style: "font-size:12.5px;color:{theme::INK};line-height:1.6;white-space:pre-wrap;", "{text}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Chat(chat: ChatVm) -> Element {
    let k = use_context::<Kernel>();
    let mut text = use_signal(String::new);
    let card = theme::card();
    let ink = theme::INK;
    let ink3 = theme::INK_3;
    let agent = theme::AGENT;
    let input = theme::input();
    let clay = theme::CLAY;
    let sid = chat.id;
    let send = move |_| {
        let t = text().trim().to_string();
        if !t.is_empty() {
            k.send(Command::SendSessionMessage {
                session: sid,
                text: t,
            });
            text.set(String::new());
        }
    };
    rsx! {
        div {
            style: "{card} padding:16px 18px;",
            div {
                style: "display:flex;align-items:center;gap:8px;margin-bottom:12px;",
                span { style: "font-size:13.5px;font-weight:600;", "{chat.title}" }
                span { style: "font-size:11px;color:{ink3};border:1px solid #E2DCCF;border-radius:6px;padding:1px 7px;", "{chat.status_label}" }
            }
            div {
                style: "display:flex;flex-direction:column;gap:8px;max-height:420px;overflow-y:auto;margin-bottom:12px;",
                if chat.msgs.is_empty() {
                    span { style: "font-size:12px;color:{ink3};", "还没有消息。" }
                }
                for (i, m) in chat.msgs.iter().enumerate() {
                    {
                        let (align, bg, fg, who) = if m.agent {
                            ("flex-start", "#FFFDF8", ink, "Agent")
                        } else {
                            ("flex-end", ink, "#F6F3EC", "Builder")
                        };
                        let text = m.text.clone();
                        rsx! {
                            div {
                                key: "{i}",
                                style: "display:flex;flex-direction:column;align-items:{align};",
                                span { style: "font-size:10px;color:{agent};margin-bottom:2px;", "{who}" }
                                span {
                                    style: "max-width:72%;background:{bg};color:{fg};border:1px solid #E2DCCF;border-radius:10px;padding:8px 12px;font-size:13px;line-height:1.65;white-space:pre-wrap;",
                                    "{text}"
                                }
                            }
                        }
                    }
                }
            }
            div {
                style: "display:flex;gap:8px;",
                textarea {
                    style: "{input} min-height:44px;font-size:13px;",
                    placeholder: "把要求说给这条会话…(真实回复由同事团队的执行器经 Executor trait 接入)",
                    value: "{text}",
                    oninput: move |e| text.set(e.value()),
                }
                button {
                    style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:8px;padding:0 18px;font-size:13px;flex:none;",
                    onclick: send,
                    "发送"
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
