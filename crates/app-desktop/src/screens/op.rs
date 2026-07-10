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
use bw_core::model::{stage_workflow, FeedLevel, HubKind, HubSource, Signal, StageKind};
use bw_core::{SessionId, WorkflowId};
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

const PANELS: [(Panel, &str); 5] = [
    (Panel::Progress, "进度"),
    (Panel::Workflow, "工作流"),
    (Panel::Routine, "定时任务"),
    (Panel::Artifact, "产物"),
    (Panel::Version, "版本"),
];

#[component]
fn Toolbar(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let border = theme::BORDER;
    let ink = theme::INK;
    let ink2 = theme::INK_2;
    rsx! {
        div {
            style: "display:flex;gap:6px;padding:10px 22px;border-bottom:1px solid {border};flex:none;",
            for (panel, label) in PANELS {
                {
                    let k = k.clone();
                    let active = op.panel == panel;
                    let (bg, fg) = if active { (ink, "#FFF") } else { ("transparent", ink2) };
                    rsx! {
                        button {
                            key: "{label}",
                            style: "cursor:pointer;border:none;border-radius:8px;background:{bg};color:{fg};padding:7px 14px;font-size:12.5px;",
                            onclick: move |_| k.send(Command::SetPanel(panel)),
                            "{label}"
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
                HealthOverview { op }
            } else {
                StageSessions { op }
            }
        }
    }
}

#[component]
fn HealthOverview(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let ink3 = theme::INK_3;
    let card_alt = theme::CARD_ALT;
    let needs_you: Vec<SessionCardVm> = op.sessions.iter().filter(|s| s.active).cloned().collect();
    let quiet = needs_you.is_empty() && op.attention.watch.is_empty();
    rsx! {
        div { style: "font-size:11px;color:{ink3};letter-spacing:.06em;margin-bottom:8px;", "健康概览" }
        if quiet {
            div { style: "font-size:12px;color:{ink3};line-height:1.7;", "一切安静。绿色隐身,只有红黄出声。" }
        }
        if !needs_you.is_empty() {
            div { style: "font-size:11px;color:{ink3};margin:6px 0;", "进行中 · 待你介入" }
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
        if !op.attention.watch.is_empty() {
            div { style: "font-size:11px;color:{ink3};margin:8px 0 6px;", "阶段信号 · 需关注" }
            for (kind, sig) in op.attention.watch.clone() {
                {
                    let k = k.clone();
                    let color = ui::signal_color(sig);
                    let dot = theme::dot(color, 8);
                    let label = kind.label();
                    let sig_label = ui::vm::signal_label(sig);
                    rsx! {
                        button {
                            key: "{kind.index()}",
                            style: "width:100%;text-align:left;background:transparent;border:1px solid #ECE6DA;border-radius:8px;padding:8px 10px;margin-bottom:6px;cursor:pointer;display:flex;align-items:center;gap:8px;",
                            onclick: move |_| k.send(Command::SetScope(Scope::Stage(kind))),
                            span { style: "{dot}" }
                            span { style: "font-size:12.5px;", "{label}" }
                            span { style: "margin-left:auto;font-size:11px;color:{ink3};", "{sig_label}" }
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
        (Panel::Artifact, _) => rsx! { ArtifactPanel {} },
        (Panel::Version, _) => rsx! { VersionPanel { op } },
    }
}

/// Deliberately still a stub — not "not built yet," but assessed and left
/// this way. The prototype's "产物画廊/产物画布" is rich structured content
/// (a clickable web app, a data matrix, a document) fabricated from a
/// commit-id hash; this app has no matching real concept (`session`s are
/// plain text transcripts, nothing structured is ever produced or stored).
/// Unlike Version — where "real git commits" was an unambiguous, already-
/// available honest data source — there's no equally clean real source
/// here yet. Repurposing something adjacent (e.g. "recently changed files
/// under `workspace_path`") would be a materially weaker echo of what this
/// panel promises, likely to read as broken rather than honest. Revisit
/// once sessions can produce a real structured output.
#[component]
fn ArtifactPanel() -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    rsx! {
        div {
            style: "{card} padding:26px 30px;max-width:560px;",
            div { style: "font-weight:600;margin-bottom:8px;", "产物画廊 / 产物画布" }
            p { style: "color:{ink2};font-size:13px;line-height:1.7;margin:0;",
                "还没有真实数据源:会话目前只产出纯文本,没有结构化产物这个存量。等会话能真正产出可展示的结构化输出后再做,不提前拼一个半假的版本。"
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
    rsx! {
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
            // A fresh spec per run — the template is methodology, the run is real.
            k.send(Command::RunWorkflow {
                session: sid,
                spec: stage_workflow(stage_kind),
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
