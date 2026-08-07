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
    stage_workflow, FeedLevel, HubKind, HubSource, IssuePriority, IssueStatus, MaturityPeriod,
    Signal, StageKind,
};
use bw_core::{ConversationId, IssueId, ProjectId, SessionId, SkillId, WorkflowId};
use bw_store::SessionKind;
use dioxus::document;
use dioxus::prelude::*;
use std::time::Duration;
use ui::vm::{MetricVm, SessionCardVm, VersionLogVm};
use ui::{sparkline_path, SparkPath, WowDir};

/// Provider-aware web URL for a remote issue. codehub →
/// `https://{domain}/{path}/issues/{iid}`; github → the canonical `github.com`
/// path. Empty path = no remote attached → empty string (caller renders plain
/// text). Bug③+UI: was a hardcoded `github.com` URL even for codehub projects.
fn remote_issue_url(provider: &str, host: &str, path: &str, n: u32) -> String {
    if path.trim().is_empty() {
        return String::new();
    }
    match provider.trim() {
        "codehub" => format!(
            "https://{}/{path}/issues/{n}",
            bw_core::codehub_alias_to_domain(host)
        ),
        _ => format!("https://github.com/{path}/issues/{n}"),
    }
}

/// Provider-aware web URL for a PR/MR. codehub → GitLab-style
/// `/-/merge_requests/{iid}`; github → `/pull/{n}`. Empty path = no remote.
fn remote_mr_url(provider: &str, host: &str, path: &str, n: u32) -> String {
    if path.trim().is_empty() {
        return String::new();
    }
    match provider.trim() {
        "codehub" => {
            format!(
                "https://{}/{path}/-/merge_requests/{n}",
                bw_core::codehub_alias_to_domain(host)
            )
        }
        _ => format!("https://github.com/{path}/pull/{n}"),
    }
}

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
    let mut confirming_delete = use_signal(|| false);
    let k_del = k.clone();
    rsx! {
        div {
            style: "width:100%;text-align:left;background:{card_alt};border:1.4px solid {bd};border-radius:8px;padding:9px 10px;margin-bottom:7px;",
            div {
                style: "display:flex;align-items:flex-start;gap:6px;",
                button {
                    style: "flex:1;text-align:left;background:transparent;border:none;cursor:pointer;padding:0;font:inherit;color:inherit;",
                    onclick: move |e| {
                        e.stop_propagation();
                        k.send(Command::SetPanel(Panel::Workflow));
                        k.send(Command::SelectSession(Some(sid)));
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
    // P3: 关联技能选择器只列 content 非空的行 —— 空壳技能选了也注入不了
    // (`standard_skill_block` 的诚实降级口径),不该出现在选项里。
    // plan/20 R1: 池 = 本项目行 + 全局基础库行,他项目的行绝不出现;
    // 种A(工作区登记行)照旧排除。
    let skill_choices: Vec<_> = op
        .hub
        .skills
        .iter()
        .filter(|s| {
            bw_core::scope::in_scope(s.project_id, Some(op.id))
                && !s.content.trim().is_empty()
                && !s.is_project_assets
        })
        .cloned()
        .collect();
    let mut new_skill = use_signal(String::new);
    // plan/20 R1(plan/08 S1 完成标准原文):「指派下拉只出现自己的五个
    // 角色」——严格只列本项目自有队友(W1 出生/补种保证每个项目都有)。
    let agents: Vec<_> = op
        .hub
        .agents
        .iter()
        .filter(|a| a.project_id == Some(op.id) && !a.is_project_assets)
        .cloned()
        .collect();
    // Board-wide: at most one card is "entering a block reason" at a time.
    // Fully qualified: `Signal` bare would resolve to `bw_core::model::Signal`
    // (the derived-health enum), already imported unqualified above.
    let mut blocking: dioxus::prelude::Signal<Option<IssueId>> = use_signal(|| None);
    let mut block_reason = use_signal(String::new);
    // V1-TermRefactor4: 点卡立刻亮「恢复中…」(dispatch 返回前 Vm 还不更新);
    // 与 op.pty_restoring 合并;焦点已活且 App 恢复标记已清 → 不再显示。
    let mut restoring_issue: dioxus::prelude::Signal<Option<IssueId>> = use_signal(|| None);

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
                select {
                    style: "border:1px solid {border};border-radius:7px;padding:7px 9px;font-size:12px;background:#FFF;color:{ink2};max-width:200px;",
                    title: "关联技能(可空)——选中后该 Issue 开工时会带着这条方法",
                    value: "{new_skill}",
                    onchange: move |e| new_skill.set(e.value()),
                    option { value: "", "不关联技能" }
                    for s in skill_choices.iter() {
                        option {
                            key: "{s.id:?}",
                            value: "{s.name}",
                            title: "{s.desc}",
                            "{s.name}"
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
                                standard_skill: new_skill(),
                            });
                            new_title.set(String::new());
                            new_skill.set(String::new());
                        }
                    },
                    "＋ 创建 Issue"
                }
            }
            // P4: the evidence overlay — floats above the board while open.
            if let Some(d) = op.issue_detail.clone() {
                IssueDetailOverlay {
                    can_consult: op.consultable_issues.contains(&d.id),
                    sessions: op.sessions.clone(),
                    active_run: op.active_run,
                    project_id: op.id,
                    d: d,
                }
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
                                let k_merge = k.clone();
                                let k_detail = k.clone();
                                let k_cancel = k.clone();
                                let agents = agents.clone();
                                let i_id = i.id;
                                // plan/17 S3: is THIS card's run in flight?
                                // (`active_run` carries (project, issue).) And is
                                // a same-project sibling in flight (serial lock
                                // → 「▶ 跑」 greyed)? A run on another project
                                // doesn't block this card.
                                let is_running = op.active_run == Some((op.id, i_id));
                                let is_focused = op.focused_issue == Some(i_id);
                                let can_consult = op.consultable_issues.contains(&i_id);
                                let can_resume = op.resumable_issues.contains(&i_id);
                                let resume_ready = op.focused_issue == Some(i_id)
                                    && op.pty_active
                                    && op.pty_restoring.is_none();
                                let is_restoring = (restoring_issue() == Some(i_id)
                                    && !resume_ready)
                                    || (op.pty_restoring.is_some()
                                        && op.focused_issue == Some(i_id));
                                let same_project_busy =
                                    op.active_run.map(|(p, _)| p) == Some(op.id);
                                // plan/17 S3: the 「▶ 跑」 button's label /
                                // cursor / color when a same-project run is in
                                // flight (serial lock — RunIssue is rejected
                                // server-side; the UI just tells the truth).
                                let (run_label, run_cursor, run_color) = if same_project_busy {
                                    ("▶ 跑(排队中)".to_string(), "not-allowed", ink3)
                                } else {
                                    ("▶ 跑".to_string(), "pointer", clay)
                                };
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
                                let op_sessions = op.sessions.clone();
                                let advance = next_issue_status(i.status);
                                let advance_label = advance.map(|s| s.label()).unwrap_or("");
                                let is_blocked = i.status == IssueStatus::Blocked;
                                let entering_reason = blocking() == Some(i_id);
                                let card_left = if is_focused { clay } else { i.status_color };
                                let card_extra = if is_focused {
                                    format!("box-shadow:inset 0 0 0 1px {clay};")
                                } else {
                                    String::new()
                                };
                                rsx! {
                                    div {
                                        key: "{i.number}",
                                        style: "{card} padding:10px 12px;margin-bottom:9px;border-left:3px solid {card_left};{card_extra}",
                                        div { style: "font-size:11px;color:{ink3};font-family:{mono};display:flex;align-items:center;gap:6px;",
                                            span { "#{i.number} · {i.stage.label()}" }
                                            if is_focused {
                                                span { style: "color:{clay};border:1px solid {clay};border-radius:4px;padding:0 5px;font-size:10px;", "当前会话" }
                                            }
                                            if is_restoring {
                                                span { style: "color:{ink3};border:1px solid {border};border-radius:4px;padding:0 5px;font-size:10px;", "恢复中…" }
                                            }
                                        }
                                        // C4 · issue 身份映射: 号非 0 才渲染。
                                        // Bug③+UI: provider-aware link to the
                                        // remote issue (codehub `{host}/{path}/issues`
                                        // / github `github.com/.../issues`), shown
                                        // as a short label not a raw URL. Empty path
                                        // = no remote → plain text.
                                        if i.github_number != 0 {
                                            div {
                                                style: "font-size:10.5px;color:{ink3};font-family:{mono};margin-top:1px;",
                                                if op.remote_path.trim().is_empty() {
                                                    "远端 #{i.github_number}"
                                                } else {
                                                    a {
                                                        href: "{remote_issue_url(&op.provider, &op.remote_host, &op.remote_path, i.github_number)}",
                                                        target: "_blank",
                                                        style: "color:{ink3};text-decoration:none;",
                                                        "远端 #{i.github_number} ↗"
                                                    }
                                                }
                                            }
                                        }
                                        // C5 · PR 验收环: 有 PR 号才渲染,如实展示
                                        // 「PR #N」——验收=人 merge,号非 0 即有开放 PR。
                                        // Bug③+UI: link to the MR/PR web URL.
                                        if i.pr_number != 0 {
                                            div {
                                                style: "font-size:10.5px;color:{clay};font-family:{mono};margin-top:1px;",
                                                if op.remote_path.trim().is_empty() {
                                                    "PR #{i.pr_number}"
                                                } else {
                                                    a {
                                                        href: "{remote_mr_url(&op.provider, &op.remote_host, &op.remote_path, i.pr_number)}",
                                                        target: "_blank",
                                                        style: "color:{clay};text-decoration:none;",
                                                        "PR #{i.pr_number} ↗"
                                                    }
                                                }
                                            }
                                        }
                                        // P4: the title opens the evidence
                                        // overlay (runs / diffs / artifacts).
                                        div {
                                            style: "font-size:13px;margin:3px 0 4px;color:{ink};cursor:pointer;",
                                            onclick: move |_| {
                                                // 重启后点卡:立刻亮「恢复中…」,内核走 OpenIssueDetail 唤醒。
                                                if can_resume {
                                                    restoring_issue.set(Some(i_id));
                                                }
                                                k_detail.send(Command::OpenIssueDetail(i_id));
                                            },
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
                                                // the same StartSession +
                                                // RunIssue path, real or mock
                                                // per project config. Mock
                                                // projects run self-labeled.
                                                // plan/17 S3 (① 中止): when this
                                                // card's run is in flight, show
                                                // 「⬇ 终止」 instead of 「▶ 跑」
                                                // (aborts the backgrounded run,
                                                // issue stays InProgress, never
                                                // auto-Done — 铁律). When a
                                                // same-project sibling is
                                                // running, grey 「▶ 跑」 (serial
                                                // lock — RunIssue would be
                                                // rejected anyway; honest UI).
                                                if is_running {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{alert};font-size:11.5px;padding:0;font-weight:700;",
                                                        onclick: move |_| {
                                                            k_cancel.send(Command::CancelRun { id: i_id });
                                                        },
                                                        "⬇ 终止"
                                                    }
                                                } else if runnable {
                                                    button {
                                                        style: "cursor:{run_cursor};background:transparent;border:none;color:{run_color};font-size:11.5px;padding:0;font-weight:700;",
                                                        disabled: same_project_busy,
                                                        onclick: move |_| {
                                                            if can_resume {
                                                                restoring_issue.set(Some(i_id));
                                                            }
                                                            let sid = existing_issue_session(
                                                                &op_sessions,
                                                                run_stage,
                                                                &run_sess_title,
                                                            )
                                                            .unwrap_or_else(SessionId::new);
                                                            k_run.send(Command::StartSession {
                                                                id: sid,
                                                                stage_kind: Some(run_stage),
                                                                kind: SessionKind::Create,
                                                                title: run_sess_title.clone(),
                                                            });
                                                            k_run.send(Command::RunIssue { session: sid, id: i_id });
                                                            // Jump the user straight to the session/terminal that just
                                                            // started (or resumed) — otherwise the board gives no visible
                                                            // feedback at all that anything happened, which is exactly
                                                            // what made repeat clicks feel like they were silently
                                                            // spawning duplicates instead of reusing the existing run.
                                                            k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                                            k_run.send(Command::SetPanel(Panel::Workflow));
                                                            k_run.send(Command::SelectSession(Some(sid)));
                                                        },
                                                        "{run_label}"
                                                    }
                                                } else if can_consult {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;font-weight:700;",
                                                        onclick: move |_| {
                                                            restoring_issue.set(Some(i_id));
                                                            let sid = existing_issue_session(
                                                                &op_sessions,
                                                                run_stage,
                                                                &run_sess_title,
                                                            )
                                                            .unwrap_or_else(SessionId::new);
                                                            k_run.send(Command::StartSession {
                                                                id: sid,
                                                                stage_kind: Some(run_stage),
                                                                kind: SessionKind::Create,
                                                                title: run_sess_title.clone(),
                                                            });
                                                            k_run.send(Command::RunIssue { session: sid, id: i_id });
                                                            k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                                            k_run.send(Command::SetPanel(Panel::Workflow));
                                                            k_run.send(Command::SelectSession(Some(sid)));
                                                        },
                                                        "续聊"
                                                    }
                                                }
                                                // C5 · PR 验收环: InReview + 有 PR 时,
                                                // merge 是首选验收路径(人 merge → 关单)。
                                                // 不硬拦下面的 →已完成(只留痕不拦人)。
                                                if i.status == IssueStatus::InReview && i.pr_number != 0 {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;font-weight:700;",
                                                        onclick: move |_| k_merge.send(Command::MergeIssuePr { id: i_id }),
                                                        "⛙ merge PR #{i.pr_number}"
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
fn IssueDetailOverlay(
    d: ui::vm::IssueDetailVm,
    sessions: Vec<SessionCardVm>,
    active_run: Option<(ProjectId, IssueId)>,
    project_id: ProjectId,
    can_consult: bool,
) -> Element {
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
    let k_merge = k.clone();
    let k_distill = k.clone();
    let k_cancel = k.clone();
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
    // P2 (2026-08-06 cowelink 验证): 对仗看板卡片(op.rs 列表行 :769-780,930-
    // 941)的同一段 active_run/串行锁判断——弹窗此前完全不看 active_run,
    // 「▶ 跑」永远可点、永远不知道这件活(或同项目另一件)是不是已经在跑。
    let is_running = active_run == Some((project_id, d.id));
    let same_project_busy = active_run.map(|(p, _)| p) == Some(project_id);
    let (run_label, run_cursor, run_color) = if same_project_busy {
        ("▶ 跑(排队中)".to_string(), "not-allowed", ink3)
    } else {
        ("▶ 跑".to_string(), "pointer", clay)
    };

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
                // C5 · PR 验收环: 有 PR 号如实展示,验收=人 merge。
                if d.pr_number != 0 {
                    div { style: "font-size:11.5px;color:{clay};font-family:{mono};margin-bottom:6px;", "PR #{d.pr_number} · 等待人工 merge 验收" }
                }
                if let Some(reason) = d.blocked_reason.clone() {
                    div { style: "margin:6px 0;padding:6px 9px;background:#F2E4DD;border-radius:6px;font-size:12px;color:{alert};", "⛔ {reason}" }
                }
                if !d.desc.trim().is_empty() {
                    div { style: "font-size:12.5px;color:{ink2};white-space:pre-wrap;margin:6px 0 10px;line-height:1.7;", "{d.desc}" }
                }

                // ── runs + real changes ──
                div { style: "font-size:12px;color:{ink3};letter-spacing:.05em;margin:12px 0 6px;", "运行史({d.runs.len()})" }
                if d.runs.is_empty() {
                    if d.is_interactive {
                        // P2: 交互式活(找指标/绑数据)不写 workflow_run——
                        // 「还没有运行」对已经跑过的交互式活是假话,过程在
                        // 嵌入终端/claude 会话里,不在这张运行史列表里。
                        div { style: "font-size:12px;color:{ink3};", "交互式活不写运行史——过程在下方嵌入终端 / claude 会话里,不是没跑过。" }
                    } else {
                        div { style: "font-size:12px;color:{ink3};", "还没有运行——「▶ 跑」会真实开工并留痕。" }
                    }
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
                    if is_running {
                        // P2: 对仗看板卡片 :930-937 — 这件活正在跑,给「⬇ 终止」
                        // 而不是一个假装可点的「▶ 跑」。
                        button {
                            style: "cursor:pointer;border:none;border-radius:7px;background:transparent;border:1px solid {alert};color:{alert};padding:6px 15px;font-size:12.5px;font-weight:700;",
                            onclick: move |_| k_cancel.send(Command::CancelRun { id }),
                            "⬇ 终止"
                        }
                    } else if runnable {
                        button {
                            style: "cursor:{run_cursor};border:none;border-radius:7px;background:{run_color};color:#FFF;padding:7px 16px;font-size:12.5px;",
                            disabled: same_project_busy,
                            onclick: move |_| {
                                if same_project_busy {
                                    return;
                                }
                                let sid = existing_issue_session(&sessions, run_stage, &run_sess_title)
                                    .unwrap_or_else(SessionId::new);
                                k_run.send(Command::StartSession {
                                    id: sid,
                                    stage_kind: Some(run_stage),
                                    kind: SessionKind::Create,
                                    title: run_sess_title.clone(),
                                });
                                k_run.send(Command::RunIssue { session: sid, id });
                                // Same as the board's 「▶ 跑」 — jump straight to the
                                // session/terminal instead of leaving the user staring at
                                // the (now stale) detail overlay with no visible sign
                                // anything started.
                                k_run.send(Command::CloseIssueDetail);
                                k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                k_run.send(Command::SetPanel(Panel::Workflow));
                                k_run.send(Command::SelectSession(Some(sid)));
                            },
                            "{run_label}"
                        }
                    } else if can_consult {
                        button {
                            style: "cursor:pointer;border:none;border-radius:7px;background:transparent;border:1px solid {clay};color:{clay};padding:7px 16px;font-size:12.5px;",
                            onclick: move |_| {
                                let sid = existing_issue_session(&sessions, run_stage, &run_sess_title)
                                    .unwrap_or_else(SessionId::new);
                                k_run.send(Command::StartSession {
                                    id: sid,
                                    stage_kind: Some(run_stage),
                                    kind: SessionKind::Create,
                                    title: run_sess_title.clone(),
                                });
                                k_run.send(Command::RunIssue { session: sid, id });
                                k_run.send(Command::CloseIssueDetail);
                                k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                k_run.send(Command::SetPanel(Panel::Workflow));
                                k_run.send(Command::SelectSession(Some(sid)));
                            },
                            "续聊"
                        }
                    }
                    if in_review {
                        // C5 · PR 验收环 (D3): 有 PR → 首选「merge PR」(人 merge →
                        // 关单 → 走现有 InReview→Done 记账)。无 PR(存量/无仓活)→
                        // 保留裸「确认完成」路径(全活 PR 化是纪律不是硬闸)。
                        if d.pr_number != 0 {
                            button {
                                style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:7px 16px;font-size:12.5px;",
                                onclick: move |_| {
                                    k_merge.send(Command::MergeIssuePr { id });
                                    k_merge.send(Command::OpenIssueDetail(id));
                                },
                                "⛙ merge PR #{d.pr_number}(验收)"
                            }
                        } else {
                            button {
                                style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:7px 16px;font-size:12.5px;",
                                onclick: move |_| {
                                    k_done.send(Command::TransitionIssue { id, status: IssueStatus::Done });
                                    k_done.send(Command::OpenIssueDetail(id));
                                },
                                "✓ 确认完成(人裁)"
                            }
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

    // Weekly sparkline geometry from weekly_spark (8-week, carry-forward).
    let wk_spark = sparkline_path(&m.weekly_spark, 92.0, 24.0);
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
            // Weekly sparkline (8-week trend · from observation ISO-week buckets)
            div {
                style: "display:flex;align-items:center;gap:8px;flex-wrap:wrap;",
                Spark { spark: wk_spark, color: spark_color, w: 92.0, h: 24.0 }
                span { style: "font-family:{mono};font-size:9.5px;color:{ink3};", "8 周走势" }
            }
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
                    "选中阶段轴上的任一阶段可查看其方法循环与历史记录;这里是三个可复用库的入口——沉淀过的工作流、可插拔技能、配置好的智能体。"
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
    let spec_preview = stage_workflow(s.kind);
    let phases = spec_preview
        .phases
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(" → ");
    let goal = spec_preview.goal.clone();
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
    // V1-Issue2-PTY-cleanup: the old 「▶ 运行」 button (`Command::RunStagePlaybook`)
    // spawned a fresh mock-chat session on every click, titled "{stage}·第N轮" —
    // an ever-growing pile of dead 「找指标」/「绑数据」 session records with no
    // real executor behind them (`SendSessionMessage`'s reply is always a
    // hardcoded 【mock】 echo). Real work now starts exclusively from an
    // Issue card's 「▶ 跑」 (`Command::RunIssue`) on the Issues panel — this
    // stage view is read-only: the method-loop preview, and whatever a real
    // run (chat-based non-interactive, or the PTY terminal below) produced.
    let chat_area = match op.chat.clone() {
        // A live PTY session (`op.pty_active`) is the one place a real reply
        // exists — showing the 发送 box next to it invites the user to type
        // into a box whose "reply" is a canned echo that goes nowhere, right
        // above the terminal where their input actually reaches claude.
        Some(chat) if !op.pty_active => rsx! { Chat { chat } },
        Some(_) => rsx! {},
        None => rsx! {
            div { style: "color:{ink3};font-size:12.5px;", "到「Issue」面板点「▶ 跑」开工——记录会出现在这里。" }
        },
    };
    rsx! {
        RunBanner { run: run.clone() }
        div {
            style: "{card} padding:18px 20px;margin-bottom:14px;",
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;",
                span { style: "font-family:{serif};font-size:15px;font-weight:600;", "{spec_preview.name}" }
            }
            div { style: "font-size:12.5px;color:{ink2};margin-bottom:4px;", "方法循环:{phases}" }
            div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "验收:{goal} · loop ≤3 迭代" }
            if op.chat.is_some() && !op.pty_active {
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
        // 重启恢复:点卡到首包之间显示「恢复中…」(首包后 pty_restoring 清空)。
        if op.pty_restoring.is_some() {
            div {
                style: "{card} padding:10px 14px;margin-bottom:10px;font-size:12.5px;color:{ink3};",
                "恢复中…"
            }
        }
        // 多会话常驻:所有活 PTY 挂 xterm;仅焦点可见(隐藏的仍收字节)。
        if op.pty_active {
            for cid in op.pty_live_ids.clone() {
                {
                    let is_focused = op.pty_conversation_id == Some(cid);
                    rsx! {
                        TerminalWidget {
                            key: "{cid.uuid()}",
                            conversation_id: cid,
                            focused: is_focused,
                        }
                    }
                }
            }
        }
        if let Some(msg) = promoted_msg() {
            Toast { msg, onclose: move |_| promoted_msg.set(None) }
        }
    }
}

/// "结果呈现": pairs the run's real phase names (from `RunVm`, so it reflects
/// whatever actually ran — the stage's own template, an imported hub
/// workflow, or an ad-hoc dynamic one — not just the stage's default
/// preview) with the real `Author::Agent` session messages, in order. A
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

// ─── V1 终端会话重构·底座: per-conversation xterm (xterm.js) ───────────

/// Bundled xterm assets (no CDN — a failed fetch used to leave no terminal).
const XTERM_JS: &str = include_str!("../../public/xterm.min.js");
const XTERM_CSS: &str = include_str!("../../public/xterm.css");
const FIT_ADDON_JS: &str = include_str!("../../public/xterm-addon-fit.min.js");

/// Pre-handler write/drain keyed by conversation id. Must run BEFORE any
/// bytes arrive so the pre-handler buffer (orca §2.4) catches early output.
/// Map shape: `window.__bw_term_sessions[id] = { term, fit, ready, buffer,
/// input[], resize }` — 修掉旧全局 `window.__bw_term` 单例(设计 md §7.3)。
const TERM_PRE_HANDLER_JS: &str = r#"
window.__bw_term_sessions = window.__bw_term_sessions || {};
window.__bw_term_ensure = function(id) {
    var s = window.__bw_term_sessions;
    if (!s[id]) s[id] = { term: null, fit: null, ready: false, buffer: '', input: [], resize: null };
    return s[id];
};
window.__bw_term_write = function(id, text) {
    var sess = window.__bw_term_ensure(id);
    if (!sess.ready || !sess.term) {
        sess.buffer += text;
        return;
    }
    try { sess.term.write(text); } catch(e) {}
};
// One call drains both queues for one conversation: Rust polls on a timer,
// and every `document::eval` is a full IPC round trip.
window.__bw_term_drain = function(id) {
    var sess = window.__bw_term_sessions && window.__bw_term_sessions[id];
    if (!sess) return null;
    var input = null;
    if (sess.input && sess.input.length > 0) {
        input = sess.input.join('');
        sess.input = [];
    }
    var resize = sess.resize || null;
    sess.resize = null;
    if (input === null && resize === null) return null;
    return { input: input, resize: resize, ready: !!sess.ready };
};
"#;

/// Build the per-conversation init IIFE. `id` is the conversation uuid
/// string. Re-attach path: panel switch drops the Dioxus div but JS Map +
/// PTY stay; remount re-homes `term.element` and re-fits (尺寸同步链).
fn term_init_js(conversation_id: &str) -> String {
    let id_json = serde_json::to_string(conversation_id).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"
return (async function(id) {{
    var div = document.getElementById('__bw_terminal_' + id);
    if (!div) return {{ ok: false, reason: 'div not found' }};
    var sess = window.__bw_term_ensure(id);

    var push = function(data) {{
        sess.input = sess.input || [];
        sess.input.push(data);
    }};
    var CTRL_A = 'a'.charCodeAt(0);
    var KEYS = {{
        Enter: '\r', Backspace: '\x7f', Escape: '\x1b', Tab: '\t',
        Delete: '\x1b[3~', Insert: '\x1b[2~',
        ArrowUp: '\x1b[A', ArrowDown: '\x1b[B',
        ArrowRight: '\x1b[C', ArrowLeft: '\x1b[D',
        Home: '\x1b[H', End: '\x1b[F',
        PageUp: '\x1b[5~', PageDown: '\x1b[6~',
    }};
    var keyBytes = function(e) {{
        if (e.ctrlKey && e.key.length === 1) {{
            var c = e.key.toLowerCase().charCodeAt(0);
            if (c >= CTRL_A && c < CTRL_A + 26) return String.fromCharCode(c - CTRL_A + 1);
            return null;
        }}
        if (KEYS[e.key]) return KEYS[e.key];
        if (e.key.length !== 1) return null;
        return e.altKey ? '\x1b' + e.key : e.key;
    }};
    var wireDiv = function(div, term) {{
        var textarea = div.querySelector('.xterm-helper-textarea');
        div.tabIndex = 0;
        var focusTerm = function() {{
            term.focus();
            if (!textarea || document.activeElement !== textarea) div.focus();
        }};
        div.addEventListener('click', focusTerm);
        focusTerm();
        div.addEventListener('keydown', function(e) {{
            if (textarea && e.target === textarea) return;
            var data = keyBytes(e);
            if (data === null) return;
            push(data);
            e.preventDefault();
        }});
    }};

    // Re-attach: existing term for this conversation survives Dioxus remount.
    if (sess.term) {{
        if (sess.term.element && !div.contains(sess.term.element)) {{
            div.appendChild(sess.term.element);
            if (sess.fit) sess.fit.fit();
            wireDiv(div, sess.term);
        }}
        return {{ ok: true, reason: 'already-initialized' }};
    }}

    if (!window.Terminal || !window.FitAddon) {{
        return {{ ok: false, reason: 'xterm bundles not loaded' }};
    }}

    var term = new Terminal({{
        fontFamily: 'JetBrains Mono, Consolas, monospace',
        fontSize: 13,
        cols: 80,
        rows: 24,
        cursorBlink: true,
        theme: {{ background: '#1e1e2e', foreground: '#cdd6f4' }},
    }});
    var fitAddon = new FitAddon.FitAddon();
    term.loadAddon(fitAddon);
    term.open(div);
    fitAddon.fit();

    term.onData(push);
    wireDiv(div, term);
    term.onResize(function(size) {{
        sess.resize = {{ cols: size.cols, rows: size.rows }};
    }});

    sess.term = term;
    sess.fit = fitAddon;
    sess.ready = true;
    if (sess.buffer) {{
        term.write(sess.buffer);
        sess.buffer = '';
    }}
    // Push fit size immediately so Rust can resize PTY off 80×24.
    sess.resize = {{ cols: term.cols, rows: term.rows }};

    return {{ ok: true }};
}})({id_json})
"#
    )
}

/// Take the longest valid UTF-8 prefix out of `buf`, leaving whatever
/// trailing bytes form an incomplete character behind for the next batch.
///
/// PTY output is a byte stream cut into ~100ms batches at arbitrary offsets;
/// a 3-byte CJK character routinely straddles two of them. Decoding a batch
/// in isolation (`from_utf8_lossy`) replaces both halves with U+FFFD.
fn take_utf8_prefix(buf: &mut Vec<u8>) -> String {
    match std::str::from_utf8(buf) {
        Ok(s) => {
            let out = s.to_string();
            buf.clear();
            out
        }
        Err(e) => {
            let valid = e.valid_up_to();
            match e.error_len() {
                None => {
                    let out = String::from_utf8_lossy(&buf[..valid]).into_owned();
                    buf.drain(..valid);
                    out
                }
                Some(_) => {
                    let out = String::from_utf8_lossy(buf).into_owned();
                    buf.clear();
                    out
                }
            }
        }
    }
}

/// 按 conversation_id 挂的嵌入终端;focused=false 时隐藏但仍收字节。
#[component]
fn TerminalWidget(conversation_id: ConversationId, focused: bool) -> Element {
    let k = use_context::<Kernel>();
    let cid_str = conversation_id.uuid().to_string();
    let div_id = format!("__bw_terminal_{cid_str}");

    use_future(move || {
        let k = k.clone();
        let cid = conversation_id;
        let cid_str = cid.uuid().to_string();
        async move {
            let debug = std::env::var("BW_PTY_DEBUG").is_ok_and(|v| v != "0");
            let cid_json = serde_json::to_string(&cid_str).unwrap_or_else(|_| "\"\"".into());

            let _ = document::eval(TERM_PRE_HANDLER_JS).await;
            let _ = document::eval(XTERM_JS).await;
            let _ = document::eval(FIT_ADDON_JS).await;
            let _ = document::eval(&format!(
                "if(!document.getElementById('__bw_xterm_css')){{var __s=document.createElement('style');__s.id='__bw_xterm_css';__s.textContent={};document.head.appendChild(__s)}}",
                serde_json::to_string(XTERM_CSS).unwrap_or_else(|_| String::new())
            ))
            .await;
            let init = document::eval(&term_init_js(&cid_str)).await;
            if debug {
                eprintln!("[pty] terminal init {cid_str}: {init:?}");
            }

            let mut pty_rx = k.pty_bytes();
            let _ = pty_rx.borrow_and_update();
            let mut carry: Vec<u8> = Vec::new();
            loop {
                tokio::select! {
                    result = pty_rx.changed() => {
                        if result.is_err() {
                            break;
                        }
                        let batches = pty_rx.borrow().clone();
                        for (batch_cid, bytes) in batches {
                            if batch_cid != cid || bytes.is_empty() {
                                continue;
                            }
                            carry.extend_from_slice(&bytes);
                            let text = take_utf8_prefix(&mut carry);
                            if text.is_empty() {
                                continue;
                            }
                            let escaped = serde_json::to_string(&text)
                                .unwrap_or_else(|_| "\"\"".into());
                            let script = format!(
                                "window.__bw_term_write({cid_json}, {escaped})"
                            );
                            let _ = document::eval(&script).await;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(30)) => {
                        let drain_script = format!(
                            "return window.__bw_term_drain ? window.__bw_term_drain({cid_json}) : null"
                        );
                        let Ok(v) = document::eval(&drain_script).await else { continue };
                        let Some(obj) = v.as_object() else { continue };
                        if let Some(input) = obj.get("input").and_then(|i| i.as_str()) {
                            if !input.is_empty() {
                                if debug {
                                    eprintln!("[pty] stdin {} bytes: {input:?}", input.len());
                                }
                                k.send(Command::TerminalInput {
                                    conversation_id: cid,
                                    bytes: input.as_bytes().to_vec(),
                                });
                            }
                        }
                        if let Some(r) = obj.get("resize").and_then(|r| r.as_object()) {
                            let cols = r.get("cols").and_then(|c| c.as_u64()).unwrap_or(80) as u16;
                            let rows = r.get("rows").and_then(|r| r.as_u64()).unwrap_or(24) as u16;
                            k.send(Command::TerminalResize {
                                conversation_id: cid,
                                cols,
                                rows,
                            });
                        }
                    }
                }
            }
        }
    });

    let border = theme::BORDER;
    let wrap = if focused {
        format!("margin-top:14px;border:1px solid {border};border-radius:8px;overflow:hidden;")
    } else {
        "display:none;".into()
    };
    rsx! {
        div {
            style: "{wrap}",
            div {
                style: "background:#1e1e2e;color:#cdd6f4;font-family:JetBrains Mono,Consolas,monospace;font-size:11px;padding:4px 10px;display:flex;align-items:center;gap:6px;",
                span { style: "opacity:0.7;", "● in-app terminal" }
                span { style: "opacity:0.4;margin-left:auto;", "claude interactive session" }
            }
            div {
                id: "{div_id}",
                style: "min-height:320px;background:#1e1e2e;",
            }
        }
    }
}
