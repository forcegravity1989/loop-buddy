//! `view = App` — the operating view (the 5-panel × 8-scope matrix).
//!
//! P2-C scope: the operating chrome (← 全部项目 top bar, the 7-stage axis, the
//! 进度/工作流/定时任务/产物/版本 toolbar) plus **`showProgStage`** — the
//! Progress panel for a single `Scope::Stage(n)` (prototype rows 1008–1075):
//! per-metric KPI sparklines, the owns / accept / control cards, a big 进度趋势
//! sparkline with a WoW direction, and the 手填·未接入度量源 honesty badge.
//!
//! Every signal dot here is **read** from the derive cache (via [`OpsVM`], which
//! the bridge fills from `persisted_signals`) and rendered with
//! [`ui::signal_color`] — the UI never sets or derives a signal. Every trend is
//! the real observation series from `metric_trends`; a metric with one
//! observation draws a flat (honest) sparkline. Other panel/scope combinations
//! render a brief P3 placeholder.

use bw_app::{Command, Panel, Scope};
use bw_core::model::StagePhase;
use dioxus::prelude::*;
use ui::{progress_color, signal_color, sparkline_path, wow_delta, WowDir};

use crate::bridge::{CommandBus, OpsMetricVM, OpsStageVM, ViewModel};
use crate::theme;

/// `phaseLabel` — the Chinese maturity label for a stage phase (badge text only;
/// **not** health). Mirrors the prototype's `stage.phase` strings.
fn phase_label(p: StagePhase) -> &'static str {
    match p {
        StagePhase::Finalized => "已定稿",
        StagePhase::Iterating => "迭代中",
        StagePhase::Monitoring => "监测中",
        StagePhase::Running => "持续运行",
    }
}

/// `signalLabel` — the stage-health word the prototype shows next to the dot.
fn signal_label(s: bw_core::Signal) -> &'static str {
    match s {
        bw_core::Signal::Green => "正常",
        bw_core::Signal::Amber => "关注",
        bw_core::Signal::Red => "阻塞",
        bw_core::Signal::Unknown => "暂无数据",
    }
}

#[component]
pub fn OpsScreen() -> Element {
    let vm = use_context::<Signal<ViewModel>>();
    let snap = vm();

    let proj = snap
        .projects
        .iter()
        .find(|p| Some(p.id) == snap.active_project)
        .cloned();
    let proj_name = proj.as_ref().map(|p| p.name.clone()).unwrap_or_default();
    // Project-level phase badge (运营中 / 冷启动中) — the operating view is only
    // reachable for Running projects, but read it honestly all the same.
    let proj_phase_label = match proj.as_ref().map(|p| p.phase) {
        Some(bw_core::model::ProjectPhase::Running) => "运营中",
        _ => "冷启动中",
    };

    let scope = snap.scope;
    let panel = snap.panel;
    let ops = snap.ops.clone();

    rsx! {
        div {
            style: "height:100vh;display:flex;flex-direction:column;background:{theme::PAPER};",
            TopBar { name: proj_name, phase_label: proj_phase_label }
            StageAxis { stages: ops.stages.clone(), scope }
            Toolbar { panel, scope, stages: ops.stages.clone() }

            // ── shell body ───────────────────────────────────────────────────
            div {
                style: "flex:1;overflow-y:auto;min-height:0;",
                match (panel, scope) {
                    // The one path P2-C fully builds.
                    (Panel::Progress, Scope::Stage(_)) => match ops.active.clone() {
                        Some(stage) => rsx! { ShowProgStage { stage } },
                        None => rsx! { Placeholder { note: "该环节不存在".to_string() } },
                    },
                    // Everything else lands in P3.
                    _ => rsx! { Placeholder { note: panel_scope_note(panel, scope) } },
                }
            }
        }
    }
}

/// Human label for the P3 placeholder so the chrome is still navigable.
fn panel_scope_note(panel: Panel, scope: Scope) -> String {
    let p = match panel {
        Panel::Progress => "进度",
        Panel::Workflow => "工作流",
        Panel::Routine => "定时任务",
        Panel::Artifact => "产物",
        Panel::Version => "版本",
    };
    let s = match scope {
        Scope::All => "全部环节",
        Scope::Stage(_) => "单环节",
    };
    format!("{p} · {s} 视图将在 P3 落地")
}

// ───────────────────────────── operating chrome ─────────────────────────────

#[component]
fn TopBar(name: String, phase_label: &'static str) -> Element {
    let bus = use_context::<CommandBus>();
    rsx! {
        div {
            style: "flex:none;background:rgba(239,235,226,0.92);border-bottom:1px solid {theme::BORDER};\
                    padding:11px 22px;display:flex;align-items:center;gap:15px;",
            // ← 全部项目 (brand mark + label), dispatch BackToProjects.
            div {
                onclick: move |_| bus.send(Command::BackToProjects),
                style: "cursor:pointer;display:flex;align-items:center;gap:9px;",
                div {
                    style: "width:26px;height:26px;border-radius:6px;background:{theme::CLAY};\
                            display:flex;align-items:center;justify-content:center;color:#fff;\
                            font:700 13px/1 {theme::FONT_MONO};",
                    "B"
                }
                span { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};", "← 全部项目" }
            }
            div { style: "width:1px;height:16px;background:{theme::SCROLL_THUMB};" }
            div { style: "font:600 15px/1.2 {theme::FONT_SANS};color:{theme::INK};", "{name}" }
            span {
                style: "font:500 11px/1 {theme::FONT_SANS};background:{theme::BADGE_RUNNING_BG};\
                        color:{theme::BADGE_RUNNING_FG};border-radius:{theme::RADIUS_SM};padding:4px 9px;",
                "{phase_label}"
            }
        }
    }
}

#[component]
fn StageAxis(stages: Vec<OpsStageVM>, scope: Scope) -> Element {
    let bus = use_context::<CommandBus>();
    let all_active = matches!(scope, Scope::All);
    let (all_bg, all_fg) = if all_active {
        ("#F1ECE3", theme::INK)
    } else {
        ("transparent", theme::INK_2)
    };

    rsx! {
        div {
            style: "flex:none;background:{theme::PAPER};border-bottom:1px solid {theme::BORDER};\
                    padding:8px 22px;display:flex;align-items:center;gap:8px;overflow-x:auto;",
            // ◎ 全部环节 · 总览 → Scope::All
            div {
                onclick: move |_| bus.send(Command::SetScope(Scope::All)),
                style: "cursor:pointer;flex:none;padding:7px 12px;border-radius:7px;background:{all_bg};\
                        display:flex;align-items:center;gap:7px;",
                div { style: "font:700 12px/1 {theme::FONT_MONO};color:{theme::CLAY};", "◎" }
                div {
                    style: "font:600 12.5px/1 {theme::FONT_SANS};color:{all_fg};white-space:nowrap;",
                    "全部环节 · 总览"
                }
            }
            div { style: "width:1px;height:20px;background:{theme::SCROLL_THUMB};flex:none;" }

            for st in stages.iter().cloned() {
                StageAxisButton { key: "{st.index}", stage: st, scope }
            }
        }
    }
}

#[component]
fn StageAxisButton(stage: OpsStageVM, scope: Scope) -> Element {
    let bus = use_context::<CommandBus>();
    let n = stage.index;
    let active = matches!(scope, Scope::Stage(s) if s == n);

    // Derived signal dot — read from the cache, never set.
    let dot = signal_color(stage.signal);
    let (bg, border, name_color, weight) = if active {
        ("#F1ECE3", theme::BORDER_2, theme::INK, "700")
    } else {
        ("transparent", "transparent", theme::INK_2, "500")
    };
    let num = format!("{n:02}");

    rsx! {
        div {
            onclick: move |_| bus.send(Command::SetScope(Scope::Stage(n))),
            style: "cursor:pointer;flex:none;padding:7px 11px;border-radius:7px;display:flex;\
                    align-items:center;gap:7px;border:1px solid {border};background:{bg};",
            div { style: "font:700 10px/1 {theme::FONT_MONO};color:#C2BBAB;", "{num}" }
            div { style: "width:6px;height:6px;border-radius:50%;background:{dot};" }
            div {
                style: "font:{weight} 12.5px/1 {theme::FONT_SANS};color:{name_color};white-space:nowrap;",
                "{stage.label}"
            }
        }
    }
}

#[component]
fn Toolbar(panel: Panel, scope: Scope, stages: Vec<OpsStageVM>) -> Element {
    // Scope label: 全部环节·总览, or 0n · <stage name>.
    let scope_label = match scope {
        Scope::All => "全部环节 · 总览".to_string(),
        Scope::Stage(n) => {
            let name = stages
                .iter()
                .find(|s| s.index == n)
                .map(|s| s.label.clone())
                .unwrap_or_default();
            format!("{n:02} · {name}")
        }
    };

    rsx! {
        div {
            style: "flex:none;background:{theme::CARD_BG_2};border-bottom:1px solid {theme::BORDER};\
                    padding:0 22px;display:flex;align-items:center;gap:16px;height:46px;",
            div {
                style: "font:600 10px/1 {theme::FONT_MONO};letter-spacing:.12em;text-transform:uppercase;\
                        color:{theme::PLACEHOLDER};flex:none;",
                "查看范围"
            }
            div { style: "font:600 13px/1 {theme::FONT_SANS};color:{theme::INK};flex:none;", "{scope_label}" }
            div { style: "width:1px;height:18px;background:{theme::SCROLL_THUMB};flex:none;" }
            div {
                style: "display:flex;background:{theme::PROGRESS_TRACK};border-radius:9px;padding:3px;gap:2px;",
                TabButton { label: "进度", target: Panel::Progress, current: panel }
                TabButton { label: "工作流", target: Panel::Workflow, current: panel }
                TabButton { label: "定时任务", target: Panel::Routine, current: panel }
                TabButton { label: "产物", target: Panel::Artifact, current: panel }
                TabButton { label: "版本", target: Panel::Version, current: panel }
            }
            div {
                style: "margin-left:auto;font:400 11px/1.5 {theme::FONT_SANS};color:{theme::PLACEHOLDER};flex:none;",
                "同一面板 · 不同环节看到不同内容"
            }
        }
    }
}

#[component]
fn TabButton(label: &'static str, target: Panel, current: Panel) -> Element {
    let bus = use_context::<CommandBus>();
    let on = target == current;
    // Selected tab bg #23211C / fg paper, per the prototype.
    let (bg, fg) = if on {
        ("#23211C", "#F3EEE6")
    } else {
        ("transparent", theme::INK_3)
    };
    rsx! {
        div {
            onclick: move |_| bus.send(Command::SetPanel(target)),
            style: "cursor:pointer;border-radius:7px;padding:7px 16px;font:600 12.5px/1 {theme::FONT_SANS};\
                    background:{bg};color:{fg};",
            "{label}"
        }
    }
}

#[component]
fn Placeholder(note: String) -> Element {
    rsx! {
        div {
            style: "max-width:560px;margin:0 auto;padding:80px 40px;text-align:center;",
            div {
                style: "font:600 12px/1 {theme::FONT_MONO};letter-spacing:.14em;text-transform:uppercase;\
                        color:{theme::CLAY};margin-bottom:14px;",
                "P3"
            }
            p {
                style: "font:400 15px/1.8 {theme::FONT_SANS};color:{theme::INK_2};margin:0;",
                "{note}"
            }
        }
    }
}

// ───────────────────────────── showProgStage ─────────────────────────────

#[component]
fn ShowProgStage(stage: OpsStageVM) -> Element {
    let (phase_bg, phase_fg) = ui::phase_style(stage.phase);
    let sig = signal_color(stage.signal);
    let num = format!("{:02}", stage.index);
    let prog = stage.progress;
    // Clay until complete, green at 100% (shared rule) — both the bar fill and
    // the percentage read.
    let prog_color = progress_color(prog);

    // Big trend: the stage's representative REAL series. WoW from its last two
    // points; < 2 points ⇒ 持平 (honest, never fabricated).
    let trend = stage.trend.clone();
    let has_trend = trend.len() >= 2;

    rsx! {
        div {
            style: "padding:34px 40px;",

            // ── header: num · name · phase badge ─────────────────────────────
            div {
                style: "display:flex;align-items:center;gap:12px;margin-bottom:18px;flex-wrap:wrap;",
                div { style: "font:700 14px/1 {theme::FONT_MONO};color:#C2BBAB;", "{num}" }
                h1 { style: "font:600 26px/1.2 {theme::FONT_SERIF};margin:0;", "{stage.label}" }
                span {
                    style: "font:500 11px/1 {theme::FONT_SANS};background:{phase_bg};color:{phase_fg};\
                            border-radius:{theme::RADIUS_SM};padding:5px 10px;",
                    "{phase_label(stage.phase)}"
                }
            }

            // ── health + progress strip ──────────────────────────────────────
            div {
                style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};\
                        border-radius:{theme::RADIUS_MD};padding:13px 20px;margin-bottom:18px;\
                        display:flex;align-items:center;gap:18px;flex-wrap:wrap;",
                div {
                    style: "display:flex;align-items:center;gap:9px;flex:none;",
                    span {
                        style: "width:9px;height:9px;border-radius:50%;background:{sig};\
                                box-shadow:0 0 0 3px {sig}22;",
                    }
                    span {
                        style: "font:600 13.5px/1 {theme::FONT_SANS};color:{theme::INK};",
                        "健康 · {signal_label(stage.signal)}"
                    }
                }
                div { style: "width:1px;height:20px;background:{theme::BORDER};flex:none;" }
                div {
                    style: "display:flex;align-items:center;gap:11px;flex:1;min-width:240px;",
                    span {
                        style: "font:500 12px/1 {theme::FONT_SANS};color:{theme::INK_3};\
                                white-space:nowrap;flex:none;",
                        "环节进度"
                    }
                    div {
                        style: "flex:1;min-width:50px;height:6px;background:{theme::PROGRESS_TRACK};\
                                border-radius:3px;overflow:hidden;",
                        div { style: "width:{prog}%;height:100%;background:{prog_color};" }
                    }
                    span {
                        style: "font:700 16px/1 {theme::FONT_MONO};color:{prog_color};\
                                white-space:nowrap;flex:none;",
                        "{prog}"
                        span { style: "font-size:11px;color:{theme::PLACEHOLDER};", "%" }
                    }
                }
            }

            // ── KPI sparklines ───────────────────────────────────────────────
            if stage.metrics.is_empty() {
                div {
                    style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};\
                            border-radius:{theme::RADIUS_MD};padding:22px 26px;margin-bottom:18px;\
                            font:400 13px/1.7 {theme::FONT_SANS};color:{theme::INK_3};",
                    "该环节暂无监测指标 · 信号将在指标接入后由真实观测推导。"
                }
            } else {
                div {
                    style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};\
                            border-radius:{theme::RADIUS_MD};padding:22px 26px;margin-bottom:18px;",
                    div {
                        style: "font:600 10px/1 {theme::FONT_MONO};letter-spacing:.12em;\
                                text-transform:uppercase;color:{theme::CLAY};margin-bottom:16px;",
                        "监测指标 · 实时趋势"
                    }
                    div {
                        style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:12px;",
                        for m in stage.metrics.iter().cloned() {
                            MetricCard { key: "{m.name}", metric: m }
                        }
                    }
                }
            }

            // ── owns / accept / control cards ────────────────────────────────
            div {
                style: "display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-bottom:18px;",
                TripCard {
                    eyebrow: "本环节产出", eyebrow_color: theme::PLACEHOLDER,
                    bg: "#fff", text_color: theme::INK, text_weight: "600",
                    body: stage.owns.clone(),
                }
                TripCard {
                    eyebrow: "验收信号", eyebrow_color: "#5F7355",
                    bg: "#fff", text_color: "#3A3833", text_weight: "500",
                    body: stage.accept.clone(),
                }
                TripCard {
                    eyebrow: "控制点", eyebrow_color: "#B0503A",
                    bg: "#F2E4DD", text_color: "#7A3D2D", text_weight: "500",
                    body: stage.control.clone(),
                }
            }

            // ── big 进度趋势 sparkline + WoW ─────────────────────────────────
            div {
                style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};\
                        border-radius:{theme::RADIUS_MD};padding:22px 26px;",
                div {
                    style: "display:flex;align-items:center;justify-content:space-between;gap:12px;\
                            flex-wrap:wrap;margin-bottom:16px;",
                    div {
                        style: "font:600 10px/1 {theme::FONT_MONO};letter-spacing:.12em;\
                                text-transform:uppercase;color:{theme::CLAY};",
                        "进度趋势"
                    }
                    WowBadge { trend: trend.clone() }
                }
                if has_trend {
                    BigTrend { trend: trend.clone() }
                } else {
                    div {
                        style: "font:400 12.5px/1.7 {theme::FONT_SANS};color:{theme::INK_3};",
                        "暂无足够观测绘制趋势 · 当前进度 {prog}%。趋势仅来自真实录入,"
                        "单点观测显示为持平。"
                    }
                }
            }
        }
    }
}

/// One KPI card: signal dot, value + target, manual badge, and a sparkline of
/// the metric's REAL observation trend (flat when < 2 points).
#[component]
fn MetricCard(metric: OpsMetricVM) -> Element {
    let col = signal_color(metric.signal);
    let value = if metric.value_raw.is_empty() {
        "—".to_string()
    } else {
        metric.value_raw.clone()
    };
    // Direction glyph from first→last of the real series (→ when < 2 points).
    let dir = match wow_delta(&metric.trend) {
        WowDir::Up => "↑",
        WowDir::Down => "↓",
        WowDir::Flat => "→",
    };

    // Sparkline geometry over a 120×34 box (matches the prototype card).
    let sp = sparkline_path(&metric.trend, 120.0, 34.0);

    rsx! {
        div {
            style: "background:#fff;border:1px solid #E6E0D3;border-radius:9px;padding:15px 16px;",
            div {
                style: "display:flex;align-items:center;gap:7px;margin-bottom:12px;",
                span { style: "width:7px;height:7px;border-radius:50%;background:{col};flex:none;" }
                span {
                    style: "font:600 12.5px/1.3 {theme::FONT_SANS};color:{theme::INK};flex:1;min-width:0;",
                    "{metric.name}"
                }
                if metric.manual {
                    // The honesty marker: this value is hand-filled, no live source.
                    span {
                        style: "font:600 9px/1 {theme::FONT_MONO};color:#8A6720;background:#F5ECD6;\
                                border-radius:5px;padding:3px 6px;flex:none;white-space:nowrap;",
                        "手填 · 未接入度量源"
                    }
                }
            }
            div {
                style: "display:flex;align-items:flex-end;justify-content:space-between;gap:12px;",
                div {
                    div {
                        style: "display:flex;align-items:baseline;gap:4px;",
                        span { style: "font:700 23px/1 {theme::FONT_MONO};color:{col};", "{value}" }
                        span { style: "font:600 13px/1 {theme::FONT_MONO};color:{col};", "{dir}" }
                    }
                    div {
                        style: "font:500 10.5px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};margin-top:7px;",
                        "目标 {metric.target_raw}"
                    }
                }
                svg {
                    view_box: "0 0 120 34",
                    style: "width:120px;height:34px;display:block;flex:none;overflow:visible;",
                    path { d: "{sp.area}", style: "fill:{col};fill-opacity:0.1;" }
                    polyline {
                        points: "{sp.polyline}",
                        style: "fill:none;stroke:{col};stroke-width:2;stroke-linejoin:round;stroke-linecap:round;",
                    }
                    circle {
                        cx: "{sp.last_x}", cy: "{sp.last_y}", r: "3",
                        style: "fill:{col};stroke:#fff;stroke-width:1.5;",
                    }
                }
            }
        }
    }
}

/// The big 进度趋势 sparkline over the stage's representative real series.
#[component]
fn BigTrend(trend: Vec<f32>) -> Element {
    let w = 600.0_f32;
    let h = 88.0_f32;
    let sp = sparkline_path(&trend, w, h);
    rsx! {
        svg {
            view_box: "0 0 {w} {h}",
            style: "width:100%;height:{h}px;display:block;overflow:visible;",
            path { d: "{sp.area}", style: "fill:{theme::CLAY};fill-opacity:0.1;" }
            polyline {
                points: "{sp.polyline}",
                style: "fill:none;stroke:{theme::CLAY};stroke-width:2.5;stroke-linejoin:round;stroke-linecap:round;",
            }
            circle {
                cx: "{sp.last_x}", cy: "{sp.last_y}", r: "4",
                style: "fill:{theme::CLAY};stroke:#fff;stroke-width:2;",
            }
        }
    }
}

/// The WoW direction pill for the big trend. ≥2 points → ↑/↓/持平 from the last
/// two; otherwise 持平/— (never a fabricated delta).
#[component]
fn WowBadge(trend: Vec<f32>) -> Element {
    let (sym, label, color, bg) = if trend.len() >= 2 {
        match wow_delta(&trend) {
            WowDir::Up => ("↑", "较上次上升", "#4A5E42", "#E7EDE2"),
            WowDir::Down => ("↓", "较上次下降", "#B0503A", "#F2E4DD"),
            WowDir::Flat => ("→", "持平", "#6B6557", "#EDE8DE"),
        }
    } else {
        ("—", "暂无对比", theme::INK_3, "#EDE8DE")
    };
    rsx! {
        span {
            style: "display:inline-flex;align-items:center;gap:5px;background:{bg};border-radius:20px;\
                    padding:4px 9px;flex:none;",
            span { style: "font:700 11px/1 {theme::FONT_MONO};color:{color};", "{sym}" }
            span { style: "font:600 10px/1 {theme::FONT_SANS};color:{color};", "{label}" }
        }
    }
}

/// One of the three owns/accept/control cards.
#[component]
fn TripCard(
    eyebrow: &'static str,
    eyebrow_color: &'static str,
    bg: &'static str,
    text_color: &'static str,
    text_weight: &'static str,
    body: String,
) -> Element {
    // A blank field shows an em-dash so the card keeps its shape.
    let text = if body.trim().is_empty() {
        "—".to_string()
    } else {
        body
    };
    rsx! {
        div {
            style: "background:{bg};border:1px solid #E6E0D3;border-radius:9px;padding:16px 18px;",
            div {
                style: "font:600 10px/1 {theme::FONT_MONO};letter-spacing:.1em;text-transform:uppercase;\
                        color:{eyebrow_color};margin-bottom:9px;",
                "{eyebrow}"
            }
            div {
                style: "font:{text_weight} 13.5px/1.6 {theme::FONT_SANS};color:{text_color};",
                "{text}"
            }
        }
    }
}
