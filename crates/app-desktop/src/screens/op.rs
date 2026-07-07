//! `view=app` — the operating view: the real monitoring/run loop.
//!
//! Everything rendered here traces back to persisted rows: signals from the
//! derive cache, trends from observation history, feeds from real records,
//! chat transcripts from the message table. The two live loops:
//!
//! * **监控**: 记录观测值 → RecordObservation → recompute → 信号翻转可见;
//! * **运行**: 运行标准工作流 → MockExecutor 流式推进 → 阶段横幅实时更新 →
//!   产出落为会话消息(同事团队的真执行器经同一 trait 热插拔)。

use crate::kernel::{ChatVm, Kernel, OpVm, RunVm, StageVm};
use crate::{templates, theme};
use bw_app::{Command, Panel, Scope};
use bw_core::model::{FeedLevel, Signal};
use bw_core::SessionId;
use bw_store::SessionKind;
use dioxus::prelude::*;
use ui::vm::{MetricVm, SessionCardVm};
use ui::{sparkline_path, SparkPath, WowDir};

#[component]
pub fn Op(op: OpVm, run: RunVm) -> Element {
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
                    Center { op, run }
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
            span { style: "color:{ink3};font-size:12px;", "{op.kind}" }
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
                "◎ 全部环节 · 总览"
            }
            for item in op.nav.clone() {
                {
                    let k = k.clone();
                    let active = op.scope == Scope::Stage(item.n);
                    let (bg, fg) = if active { (ink, "#FFF") } else { ("transparent", ink2) };
                    let color = ui::signal_color(item.signal);
                    let dot = theme::dot(color, 7);
                    let n = item.n;
                    rsx! {
                        button {
                            key: "{n}",
                            style: "cursor:pointer;border:1px solid {border};border-radius:8px;background:{bg};color:{fg};padding:6px 11px;font-size:12px;display:flex;align-items:center;gap:7px;white-space:nowrap;",
                            onclick: move |_| k.send(Command::SetScope(Scope::Stage(n))),
                            span { style: "{dot}" }
                            span { "{item.n:02} {item.label}" }
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
                                    k.send(Command::SetScope(Scope::Stage(kind.index())));
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
            div { style: "font-size:11px;color:{ink3};margin:8px 0 6px;", "环节信号 · 需关注" }
            for (kind, sig) in op.attention.watch.clone() {
                {
                    let k = k.clone();
                    let color = ui::signal_color(sig);
                    let dot = theme::dot(color, 8);
                    let n = kind.index();
                    let label = kind.label();
                    let sig_label = ui::vm::signal_label(sig);
                    rsx! {
                        button {
                            key: "{n}",
                            style: "width:100%;text-align:left;background:transparent;border:1px solid #ECE6DA;border-radius:8px;padding:8px 10px;margin-bottom:6px;cursor:pointer;display:flex;align-items:center;gap:8px;",
                            onclick: move |_| k.send(Command::SetScope(Scope::Stage(n))),
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
            "{op.attention.steady} 个环节平稳 · {op.archived} 条已归档"
        }
    }
}

#[component]
fn StageSessions(op: OpVm) -> Element {
    let ink3 = theme::INK_3;
    let agent = theme::AGENT;
    let Scope::Stage(n) = op.scope else {
        return rsx! { span {} };
    };
    let active_id = op.chat.as_ref().map(|c| c.id);
    let mine: Vec<SessionCardVm> = op
        .sessions
        .iter()
        .filter(|s| s.stage_kind.map(|x| x.index()) == Some(n))
        .cloned()
        .collect();
    let creates: Vec<SessionCardVm> = mine.iter().filter(|s| s.create).cloned().collect();
    let opts: Vec<SessionCardVm> = mine.iter().filter(|s| !s.create).cloned().collect();
    let empty = mine.is_empty();
    rsx! {
        div { style: "font-size:11px;color:{ink3};letter-spacing:.06em;margin-bottom:8px;", "环节记录" }
        if empty {
            div { style: "font-size:12px;color:{ink3};line-height:1.7;", "该环节暂无记录。到「工作流」面板运行一轮标准工作流,记录会出现在这里。" }
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
fn Center(op: OpVm, run: RunVm) -> Element {
    let stage = match op.scope {
        Scope::Stage(n) => op.stages.iter().find(|s| s.n == n).cloned(),
        Scope::All => None,
    };
    match (op.panel, stage) {
        (Panel::Progress, None) => rsx! { ProgressAll { op } },
        (Panel::Progress, Some(s)) => rsx! { ProgressStage { s } },
        (Panel::Workflow, s) => rsx! { WorkflowPanel { op, stage: s, run } },
        (Panel::Routine, None) => rsx! { RoutineAll { op } },
        (Panel::Routine, Some(s)) => rsx! { RoutineStage { s } },
        (Panel::Artifact, _) => rsx! { P3Stub { what: "产物画廊 / 产物画布" } },
        (Panel::Version, _) => rsx! { P3Stub { what: "版本 · commit 时间线与 issues" } },
    }
}

#[component]
fn P3Stub(what: &'static str) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    rsx! {
        div {
            style: "{card} padding:26px 30px;max-width:560px;",
            div { style: "font-weight:600;margin-bottom:8px;", "{what}" }
            p { style: "color:{ink2};font-size:13px;line-height:1.7;margin:0;",
                "属于 P3 · 铺屏阶段。这里将展示真实产物与版本记录 —— 不放模拟数据。"
            }
        }
    }
}

// ── progress · all ──

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
    let stats = [
        ("工作流累计", op.stats.workflows_total),
        ("定时任务运行中", op.stats.routines_active),
        ("优化中待验收", op.stats.optimizing),
    ];
    rsx! {
        div {
            style: "{card} padding:20px 22px;margin-bottom:16px;",
            div { style: "display:flex;justify-content:space-between;align-items:baseline;margin-bottom:10px;",
                span { style: "font-family:{serif};font-size:16px;font-weight:600;", "总进度" }
                span { style: "font-family:{mono};font-size:14px;", "{overall}%" }
            }
            div {
                style: "height:8px;border-radius:4px;background:#E6E0D2;overflow:hidden;margin-bottom:6px;",
                div { style: "height:100%;width:{overall}%;background:{bar_color};" }
            }
            div { style: "font-size:11.5px;color:{ink3};", "各环节进度的平均值;环节进度在「进度 × 环节」里手动维护 —— 它是计划数据,不是信号。" }
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
            div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:12px;", "环节" }
            for s in op.stages.clone() {
                {
                    let k = k.clone();
                    let color = ui::signal_color(s.health);
                    let dot = theme::dot(color, 8);
                    let (chip_bg, chip_fg) = s.phase_chip;
                    let chip = theme::chip(chip_bg, chip_fg);
                    let bar = ui::progress_color(s.progress);
                    let n = s.n;
                    let progress = s.progress;
                    rsx! {
                        button {
                            key: "{n}",
                            style: "width:100%;display:grid;grid-template-columns:24px 1.4fr 110px 1fr 60px;gap:10px;align-items:center;background:transparent;border:none;border-bottom:1px dashed #ECE6DA;padding:10px 2px;cursor:pointer;text-align:left;",
                            onclick: move |_| {
                                k.send(Command::SetScope(Scope::Stage(n)));
                                k.send(Command::SetPanel(Panel::Workflow));
                            },
                            span { style: "{dot}" }
                            span { style: "font-size:13px;", "{s.n:02} {s.label}" }
                            span { style: "{chip}", "{s.phase_label}" }
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
fn ProgressStage(s: StageVm) -> Element {
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
    let (chip_bg, chip_fg) = s.phase_chip;
    let chip = theme::chip(chip_bg, chip_fg);
    let meta = [
        ("我负责什么", s.owns.clone()),
        ("验收信号", s.accept.clone()),
        ("控制点", s.control.clone()),
    ];
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
            span { style: "font-family:{serif};font-size:18px;font-weight:600;", "{s.n:02} {s.label}" }
            span { style: "{chip}", "{s.phase_label}" }
            span { style: "font-size:12px;color:{ink3};", "节奏 · {s.schedule_label}" }
        }
        if empty {
            div {
                style: "{card} padding:20px 22px;margin-bottom:16px;",
                div { style: "font-weight:600;margin-bottom:6px;", "该环节还没有指标" }
                p { style: "color:{ink2};font-size:13px;margin:0;line-height:1.7;",
                    "回到向导第 4/5 步为它补上指标,或在此环节以外先运营 —— 无数据的环节读作「无数据」,绝不冒充绿色。"
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
            style: "display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-bottom:16px;",
            for (t, body) in meta {
                {
                    let text = if body.is_empty() { "—".to_string() } else { body };
                    rsx! {
                        div {
                            key: "{t}",
                            style: "{card} padding:14px 16px;",
                            div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "{t}" }
                            div { style: "font-size:12.5px;color:{ink2};line-height:1.7;", "{text}" }
                        }
                    }
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
    }
}

// ── workflow panel ──

#[component]
fn WorkflowPanel(op: OpVm, stage: Option<StageVm>, run: RunVm) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    match stage {
        None => rsx! {
            div {
                style: "{card} padding:22px 24px;max-width:640px;",
                div { style: "font-weight:600;margin-bottom:8px;", "工作流库(跨环节)属 P3" }
                p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0;",
                    "当前版本为每个环节内置一条标准工作流:选中环节轴上的任一控制点,即可运行并实时观察阶段推进;产出会作为会话消息入库。沉淀/复用型工作流库在 P3 铺屏时交付。"
                }
            }
        },
        Some(s) => rsx! { WorkflowStage { op, s, run } },
    }
}

#[component]
fn RunBanner(run: RunVm) -> Element {
    let card_alt = theme::CARD_ALT;
    let clay = theme::CLAY;
    let ink3 = theme::INK_3;
    let green = ui::signal_color(Signal::Green);
    let red = ui::signal_color(Signal::Red);
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
    rsx! {
        div {
            style: "background:{card_alt};border:1px solid #DBD4C5;border-radius:10px;padding:12px 16px;margin-bottom:14px;",
            div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "{status}" }
            div {
                style: "display:flex;gap:8px;flex-wrap:wrap;",
                for (i, (name, done)) in run.phases.iter().enumerate() {
                    {
                        let color = if *done { green } else if run.failed.is_some() { red } else { clay };
                        let mark = if *done { "✓" } else { "…" };
                        rsx! {
                            span {
                                key: "{i}",
                                style: "border:1.4px solid {color};color:{color};border-radius:7px;padding:3px 10px;font-size:12px;",
                                "{name} {mark}"
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
    let spec_preview = templates::stage_workflow(s.kind);
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
                spec: templates::stage_workflow(stage_kind),
            });
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
                    let run_label = if running { "运行中…" } else { "▶ 运行" };
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
            div { style: "font-size:12.5px;color:{ink2};margin-bottom:4px;", "阶段:{phases}" }
            div { style: "font-size:12px;color:{ink3};", "验收:{goal} · loop ≤3 迭代" }
        }
        {chat_area}
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
            div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:12px;", "定时任务(按环节)" }
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
                            span { style: "font-size:13px;min-width:130px;", "{s.n:02} {s.label}" }
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
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let amber = ui::signal_color(Signal::Amber);
    let red = ui::signal_color(Signal::Red);
    let watches: Vec<String> = s.metrics.iter().map(|m| m.name.clone()).collect();
    let empty_feed = s.feed.is_empty();
    rsx! {
        div {
            style: "display:flex;align-items:center;gap:10px;margin-bottom:14px;",
            span { style: "font-family:{serif};font-size:18px;font-weight:600;", "{s.n:02} {s.label} · 观测" }
            span { style: "font-size:12px;color:{ink3};", "节奏 · {s.schedule_label}" }
        }
        div {
            style: "{card} padding:18px 20px;margin-bottom:14px;",
            div { style: "font-size:12px;color:{ink3};margin-bottom:8px;", "监测项" }
            if watches.is_empty() {
                span { style: "font-size:12.5px;color:{ink3};", "该环节没有指标可盯 —— 先在向导里补指标。" }
            }
            div {
                style: "display:flex;gap:8px;flex-wrap:wrap;",
                for w in watches {
                    span {
                        key: "{w}",
                        style: "border:1px solid #E2DCCF;border-radius:7px;padding:3px 10px;font-size:12px;color:{ink2};",
                        "{w}"
                    }
                }
            }
        }
        div {
            style: "{card} padding:18px 20px;",
            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "观测流(真实记录,最新在前)" }
            if empty_feed {
                span { style: "font-size:12.5px;color:{ink3};", "还没有观测记录。在「进度 × 本环节」的指标卡里记录本周值,这里会出现每一笔。" }
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
