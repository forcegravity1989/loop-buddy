//! `view = Wizard` — the 0→1 cold-start wizard (faithful port of prototype
//! rows 88–627; spec `plan/01 §3.2`).
//!
//! ## Module layout
//! - This file owns **state + orchestration**: the [`WizardScreen`] entry
//!   component, the local-only input model ([`WizState`]), the sticky top step
//!   bar, the per-step footer, step routing, and the one-shot final-confirm
//!   dispatch chain.
//! - [`mod steps`](steps) owns the **eight step bodies** (the verbose `rsx!`),
//!   each a `#[component]` fed the relevant `Signal`s as props.
//!
//! ## Step routing
//! The current step is `vm().wizard_step` (0..=7) — the kernel's authoritative
//! `cold_step`. Navigation never mutates local field state; it only dispatches
//! [`Command::SetWizardStep`] so the step persists (resume-on-reopen). The body
//! is chosen by a `match` on that step.
//!
//! ## Why local state, single dispatch (avoids duplicate observations)
//! `Command::UpsertManualMetric` **appends an observation every call**, so all
//! editable input lives in [`WizState`] ([`use_signal`]) and is dispatched
//! **exactly once**, on the step-7 「完成」 confirm — never per keystroke and
//! never on each `SetWizardStep`. See [`finish`] for the exact ordered chain.
//!
//! Health stays derived: the wizard sets no signal except the explicit
//! human-override path on the final confirm (step-7 selector chose amber/red).
//!
//! ## P2 limitation (accepted)
//! Only `cold_step` is persisted on quit; the local field values in
//! [`WizState`] are lost on resume. The forward flow (new → 7 steps → complete)
//! is what P2 exits on.

mod steps;

use bw_app::Command;
use bw_core::derive::AmberBand;
use bw_core::model::StageKind;
// `bw_core::Signal` (derived health enum) aliased so bare `Signal<T>` is always
// the Dioxus reactive signal (which the bridge + props use).
use bw_core::MetricId;
use bw_core::Signal as HealthSignal;
use bw_store::MetricRole;
use dioxus::prelude::*;

use crate::bridge::{CommandBus, ViewModel};
use crate::theme;

/// One editable leading/lagging metric row (wizard steps 4/5/7). Mirrors the
/// prototype's `LeadingMetric`/`LaggingMetric` seed shape; only `target` and
/// `driver` are user-editable, the rest are presentational context.
#[derive(Clone, PartialEq)]
pub struct MetricRow {
    pub name: String,
    pub def: String,
    /// Current / 实际 value — becomes the Manual observation `value`.
    pub cur: String,
    /// 来源 (leading only; empty for lagging).
    pub source: String,
    /// 「可控 · 难造假」 chip (leading only).
    pub ok: String,
    /// Last week's target, shown read-only in the step-7 grid.
    pub last_target: String,
    /// Whether last week's target was hit (drives the 达成/未达成 chip).
    pub hit: bool,
    /// Editable 本周目标 / 目标.
    pub target: String,
    /// Editable 依据 · 本周交付 (step-7 only). Folded into nothing on dispatch —
    /// there is no driver field on `UpsertManualMetric` (P2: dropped, see docs).
    pub driver: String,
}

/// All wizard input, held locally so the kernel sees each metric exactly once
/// (on the final confirm). `Copy`-free; read via `state()` and written via
/// `state.with_mut(..)`.
#[derive(Clone, PartialEq)]
pub struct WizState {
    pub north_star: String,
    pub ns_def: String,
    pub leading: Vec<MetricRow>,
    pub lagging: Vec<MetricRow>,
    /// Step-7 weekly health selector (`green`/`amber`/`red`). Default green =
    /// no override (the derived health stands).
    pub weekly_signal: HealthSignal,
}

impl WizState {
    /// Seed the warm sample-project values from the prototype (rows 2224–2237)
    /// so the wizard demonstrates a real project end-to-end and every editable
    /// field starts populated.
    fn seed() -> Self {
        Self {
            north_star: "服务有效可用性 99.9%".into(),
            ns_def: "用户可成功调用的请求时间占比,从真实网关日志计算,难以人为修饰。".into(),
            leading: vec![
                MetricRow {
                    name: "告警覆盖率".into(),
                    def: "已配置自动告警的关键指标 / 全部关键指标".into(),
                    cur: "60%".into(),
                    source: "监控系统自动统计".into(),
                    ok: "可控 · 难造假".into(),
                    last_target: "70%".into(),
                    hit: false,
                    target: "85%".into(),
                    driver: "本周交付 11 个关键指标的默认告警模板".into(),
                },
                MetricRow {
                    name: "根因采纳率".into(),
                    def: "agent 给出的根因建议被值班采纳的比例".into(),
                    cur: "41%".into(),
                    source: "事故复盘自动记录".into(),
                    ok: "可控 · 难造假".into(),
                    last_target: "50%".into(),
                    hit: false,
                    target: "70%".into(),
                    driver: "根因建议补证据链 + 值班一键采纳".into(),
                },
                MetricRow {
                    name: "延迟回归运行率".into(),
                    def: "每周自动跑 P95 延迟回归的天数 / 7".into(),
                    cur: "5/7".into(),
                    source: "CI 流水线记录".into(),
                    ok: "可控 · 难造假".into(),
                    last_target: "6/7".into(),
                    hit: false,
                    target: "7/7".into(),
                    driver: "CI 加夜间定时触发,补齐周末两天".into(),
                },
            ],
            lagging: vec![
                MetricRow {
                    name: "月度有效可用性".into(),
                    def: "北极星的直接结果".into(),
                    cur: "99.4%".into(),
                    source: String::new(),
                    ok: String::new(),
                    last_target: String::new(),
                    hit: false,
                    target: "99.9%".into(),
                    driver: String::new(),
                },
                MetricRow {
                    name: "平均故障定位时间".into(),
                    def: "告警到定位根因的中位时间".into(),
                    cur: "38min".into(),
                    source: String::new(),
                    ok: String::new(),
                    last_target: String::new(),
                    hit: false,
                    target: "<15min".into(),
                    driver: String::new(),
                },
            ],
            weekly_signal: HealthSignal::Green,
        }
    }
}

/// The 8-step labels (step 0 引子 carries an empty label; steps 1–7 are the
/// seven control points, matching the prototype `labels[]` + the 引子 head).
const STEP_LABELS: [&str; 8] = [
    "引子",
    "竞品洞察",
    "需求导入",
    "北极星指标",
    "引领指标",
    "滞后指标",
    "原型创建",
    "进度管理",
];

/// Wizard root. Reads `wizard_step` from the VM, holds all editable input in
/// local state, and routes to the matching step body.
#[component]
pub fn WizardScreen() -> Element {
    let vm = use_context::<Signal<ViewModel>>();
    let step = vm().wizard_step;

    // Local-only input, seeded once on mount. Survives `SetWizardStep`
    // navigation (component stays mounted); reset only when the screen unmounts.
    let state = use_signal(WizState::seed);

    rsx! {
        StepBar { step }

        // ── step body ──────────────────────────────────────────────────────
        match step {
            0 => rsx! { steps::Step0Intro {} },
            1 => rsx! { steps::Step1Insight {} },
            2 => rsx! { steps::Step2Requirement {} },
            3 => rsx! { steps::Step3NorthStar { state } },
            4 => rsx! { steps::Step4Leading { state } },
            5 => rsx! { steps::Step5Lagging { state } },
            6 => rsx! { steps::Step6Prototype {} },
            _ => rsx! { steps::Step7Progress { state } },
        }
    }
}

/// Sticky top bar: brand + an 8-dot step bar (引子 + 1–7). Each dot is colored
/// done/current/todo; clicking navigates via `go(n)` → `SetWizardStep`.
#[component]
fn StepBar(step: u8) -> Element {
    // Navigation is dispatched by the individual `StepDot` children.
    rsx! {
        div {
            style: "position:sticky;top:0;z-index:50;background:rgba(239,235,226,0.88);\
                    backdrop-filter:blur(12px);border-bottom:1px solid {theme::BORDER};",
            div {
                style: "max-width:1180px;margin:0 auto;padding:14px 40px;display:flex;\
                        align-items:center;gap:24px;",

                // brand cluster
                div {
                    style: "display:flex;align-items:center;gap:11px;flex:none;",
                    div {
                        style: "width:26px;height:26px;border-radius:6px;background:{theme::CLAY};\
                                display:flex;align-items:center;justify-content:center;color:#fff;\
                                font:700 13px/1 {theme::FONT_MONO};",
                        "B"
                    }
                    div {
                        style: "font:600 14px/1.2 {theme::FONT_SANS};letter-spacing:.01em;",
                        "Builders 工作台"
                    }
                    div { style: "width:1px;height:16px;background:{theme::SCROLL_THUMB};" }
                    div {
                        style: "font:400 13px/1.2 {theme::FONT_SANS};color:{theme::INK_3};",
                        "OPC 项目管理向导"
                    }
                }

                // 8 dots, right-aligned, horizontally scrollable on narrow widths
                div {
                    style: "flex:1;display:flex;align-items:center;gap:0;overflow-x:auto;\
                            justify-content:flex-end;",
                    for (i , label) in STEP_LABELS.iter().enumerate() {
                        StepDot { idx: i as u8, current: step, label: *label }
                    }
                }
            }
        }
    }
}

/// One step-bar dot + label. `done` (< current, clay) / `current` (ink) /
/// `todo` (paper, dashed). Clicking dispatches `SetWizardStep { step: idx }`.
#[component]
fn StepDot(idx: u8, current: u8, label: &'static str) -> Element {
    let bus = use_context::<CommandBus>();

    // The prototype keys "done" off a `completed[]` set; here forward progress is
    // monotonic, so any step before the current one reads as done — same visual.
    let (dot_bg, dot_color, dot_border, label_color) = if idx < current {
        (theme::CLAY, "#FFFFFF", "1px solid transparent", theme::INK)
    } else if idx == current {
        (theme::INK, "#FFFFFF", "1px solid transparent", theme::INK)
    } else {
        (
            theme::CARD_BG,
            theme::INK_3,
            "1px solid #D8D1C2",
            theme::PLACEHOLDER,
        )
    };
    let num = format!("{idx:02}");

    rsx! {
        div {
            onclick: move |_| bus.send(Command::SetWizardStep { step: idx }),
            style: "display:flex;align-items:center;gap:8px;cursor:pointer;flex:none;padding:4px 10px;",
            div {
                style: "width:22px;height:22px;border-radius:50%;display:flex;align-items:center;\
                        justify-content:center;font:600 10px/1 {theme::FONT_MONO};\
                        background:{dot_bg};color:{dot_color};border:{dot_border};",
                "{num}"
            }
            div {
                style: "font:500 12px/1 {theme::FONT_SANS};color:{label_color};white-space:nowrap;",
                "{label}"
            }
        }
    }
}

/// Per-step footer: ← 上一步 (hidden on step 0) + the next/confirm button.
/// Steps 1–6 advance with `SetWizardStep { step + 1 }`; step 7's button runs the
/// one-shot [`finish`] dispatch chain from `state`.
///
/// `state` is read **only** on step 7 (the `finish` chain). Presentational steps
/// (1/2/6) pass a throwaway local signal — harmless, since their button only
/// dispatches a step bump and never touches `state`.
#[component]
pub fn StepFooter(step: u8, state: Signal<WizState>) -> Element {
    let bus = use_context::<CommandBus>();

    let next_label = match step {
        1 => "确认洞察,进入下一环节 →",
        2 => "确认需求,继续 →",
        3 => "确认北极星,继续 →",
        4 => "确认引领指标,继续 →",
        5 => "确认滞后指标,继续 →",
        6 => "确认原型,继续 →",
        _ => "完成,生成项目看板 →",
    };
    // Step 7's confirm is clay (terminal action); earlier steps are ink.
    let next_bg = if step >= 7 { theme::CLAY } else { theme::INK };

    rsx! {
        div {
            style: "margin-top:36px;display:flex;align-items:center;gap:14px;",
            button {
                onclick: move |_| bus.send(Command::SetWizardStep { step: step - 1 }),
                style: "background:transparent;color:{theme::INK_2};border:1px solid {theme::SCROLL_THUMB};\
                        border-radius:{theme::RADIUS_SM};padding:13px 22px;\
                        font:500 14px/1 {theme::FONT_SANS};cursor:pointer;",
                "← 上一步"
            }
            button {
                onclick: move |_| {
                    if step >= 7 {
                        finish(bus, state());
                    } else {
                        bus.send(Command::SetWizardStep { step: step + 1 });
                    }
                },
                style: "background:{next_bg};color:#fff;border:none;border-radius:{theme::RADIUS_SM};\
                        padding:13px 26px;font:600 14px/1 {theme::FONT_SANS};cursor:pointer;",
                "{next_label}"
            }
        }
    }
}

/// The one-shot final-confirm chain (step-7 「完成」). Dispatches each metric
/// **exactly once** (no duplicate Manual observations), in the order the kernel
/// expects. See module docs.
///
/// Order:
/// 1. `UpdateNorthStar` (step-3 state).
/// 2. one `UpsertManualMetric` per **leading** row (role Leading, stage Leading).
/// 3. one `UpsertManualMetric` per **lagging** row (role Lagging, stage Lagging).
/// 4. `CompleteWizard` — kernel sets phase=Running, materializes 7 stages,
///    recomputes signals, routes `view = App`.
/// 5. **only if** the step-7 selector chose amber/red (more pessimistic than the
///    derived health): `AnnotateWeeklyReview { human_override: Some(sig), .. }`.
///    Green is skipped — the kernel rejects a more-optimistic override.
fn finish(bus: CommandBus, st: WizState) {
    // 1 · north star.
    bus.send(Command::UpdateNorthStar {
        value: st.north_star.clone(),
        def: st.ns_def.clone(),
    });

    // 2 · leading metrics — one observation each.
    for m in &st.leading {
        bus.send(Command::UpsertManualMetric {
            id: MetricId::new(),
            name: m.name.clone(),
            role: MetricRole::Leading,
            stage_kind: Some(StageKind::Leading),
            target: m.target.clone(),
            amber: AmberBand::default(),
            value: m.cur.clone(),
        });
    }

    // 3 · lagging metrics — one observation each.
    for m in &st.lagging {
        bus.send(Command::UpsertManualMetric {
            id: MetricId::new(),
            name: m.name.clone(),
            role: MetricRole::Lagging,
            stage_kind: Some(StageKind::Lagging),
            target: m.target.clone(),
            amber: AmberBand::default(),
            value: m.cur.clone(),
        });
    }

    // 4 · materialize the project (phase=Running, 7 stages, recompute, view=App).
    bus.send(Command::CompleteWizard);

    // 5 · human override only when more pessimistic than green (kernel rejects a
    // more-optimistic override; green = let the derived health stand).
    if st.weekly_signal != HealthSignal::Green {
        bus.send(Command::AnnotateWeeklyReview {
            human_override: Some(st.weekly_signal),
            reason: "向导首周人工判定".into(),
        });
    }
}
