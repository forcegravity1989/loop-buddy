//! `ui` — shared ViewModel + pure-function selectors (plan `§4`), the Rust port
//! of the prototype `buildApp()`. Every function here is pure over `bw-core`
//! types and unit-tested, so UI correctness is provable without a window. Stays
//! wasm32-clean for the Web keepalive.

#![forbid(unsafe_code)]

pub mod vm;

use bw_core::model::{Signal, StageKind};
use serde::Serialize;

/// `sigColor(s)` → hex. The fourth state, `Unknown`, gets the warm-paper grey
/// that reads as "no data" — never green (plan `§6`).
pub fn signal_color(s: Signal) -> &'static str {
    match s {
        Signal::Green => "#5F7355",
        Signal::Amber => "#B5862F",
        Signal::Red => "#B0503A",
        Signal::Unknown => "#A19B8D",
    }
}

/// (background, foreground, border) tint for a stage's role chip, derived
/// from its brand color (体系重构 v2 `capTint`/`capDark`/`capTintBd`). Replaces
/// the old maturity-phase badge — a stage's badge is now its role, not a
/// hand-tracked lifecycle state.
pub fn stage_tint(kind: StageKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        StageKind::Prototype => ("#F7EDE7", "#7A3D2D", "#E6D2C8"),
        StageKind::Build => ("#F5ECD6", "#8A6720", "#E8D9B5"),
        StageKind::Optimize => ("#F1F4EC", "#4A5E42", "#DCE5D2"),
        StageKind::Growth => ("#E9F0F1", "#3E6167", "#D3E0E2"),
        StageKind::Ops => ("#F5F1E8", "#6B655C", "#E6E0D3"),
    }
}

/// Progress bar color: clay until complete, green at 100% (plan `§5`).
pub fn progress_color(progress: u8) -> &'static str {
    if progress >= 100 {
        "#5F7355"
    } else {
        "#C5654A"
    }
}

/// `opOverall` — overall project progress = mean of the stage progresses.
pub fn overall_progress(stage_progresses: &[u8]) -> u8 {
    if stage_progresses.is_empty() {
        return 0;
    }
    let sum: u32 = stage_progresses.iter().map(|&p| u32::from(p)).sum();
    (sum / stage_progresses.len() as u32) as u8
}

/// Week-over-week direction from the last two trend points.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum WowDir {
    Up,
    Down,
    Flat,
}

/// `wow_delta` — compares the final two points of a trend.
pub fn wow_delta(trend: &[f32]) -> WowDir {
    match trend {
        [.., a, b] if b > a => WowDir::Up,
        [.., a, b] if b < a => WowDir::Down,
        _ => WowDir::Flat,
    }
}

/// A sparkline rendered to SVG geometry (plan `§4` / `§5` `wsMetrics`).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SparkPath {
    /// `points` string for `<polyline points="…">`.
    pub polyline: String,
    /// `d` for a filled area `<path d="…">` under the curve.
    pub area: String,
    /// Current-value endpoint (for the trailing dot).
    pub last_x: f32,
    pub last_y: f32,
}

/// Normalize a trend into an SVG polyline + filled area over a `w × h` box.
/// Higher values sit higher (smaller y). A flat series draws at mid-height.
pub fn sparkline_path(trend: &[f32], w: f32, h: f32) -> SparkPath {
    if trend.is_empty() {
        return SparkPath {
            polyline: String::new(),
            area: String::new(),
            last_x: 0.0,
            last_y: 0.0,
        };
    }

    let min = trend.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = trend.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let span = max - min;
    let n = trend.len();

    let x_at = |i: usize| -> f32 {
        if n == 1 {
            0.0
        } else {
            (i as f32 / (n - 1) as f32) * w
        }
    };
    let y_at = |v: f32| -> f32 {
        if span <= f32::EPSILON {
            h * 0.5 // flat series → mid-height
        } else {
            h - ((v - min) / span) * h
        }
    };

    let mut polyline = String::new();
    for (i, &v) in trend.iter().enumerate() {
        if i > 0 {
            polyline.push(' ');
        }
        polyline.push_str(&format!("{:.1},{:.1}", x_at(i), y_at(v)));
    }

    // Area: baseline → curve → baseline, closed.
    let mut area = format!("M {:.1},{:.1}", x_at(0), h);
    for (i, &v) in trend.iter().enumerate() {
        area.push_str(&format!(" L {:.1},{:.1}", x_at(i), y_at(v)));
    }
    area.push_str(&format!(" L {:.1},{:.1} Z", x_at(n - 1), h));

    SparkPath {
        polyline,
        area,
        last_x: x_at(n - 1),
        last_y: y_at(trend[n - 1]),
    }
}

/// One stage's input to the health-overview filter.
#[derive(Clone, Copy, Debug)]
pub struct StageAttention {
    pub kind: StageKind,
    pub signal: Signal,
    pub active_sessions: u32,
}

/// Result of the "green hides, only red/amber/unknown speak" filter (plan `§5`).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Attention {
    /// Stages with an in-progress session → "进行中·待你介入".
    pub needs_you: Vec<StageKind>,
    /// Stages whose signal ≠ green → "环节信号·需关注".
    pub watch: Vec<(StageKind, Signal)>,
    /// Count of quiet, healthy stages folded into the footnote.
    pub steady: usize,
}

/// `overviewHealth` — surface only what needs a human: in-progress sessions and
/// non-green stages. Healthy & quiet stages sink to a footnote count. This is the
/// `buildApp()` business rule, not a template detail.
pub fn overview_attention(stages: &[StageAttention]) -> Attention {
    let mut a = Attention::default();
    for s in stages {
        if s.active_sessions > 0 {
            a.needs_you.push(s.kind);
        }
        if s.signal != Signal::Green {
            a.watch.push((s.kind, s.signal));
        }
        if s.signal == Signal::Green && s.active_sessions == 0 {
            a.steady += 1;
        }
    }
    a
}

// ───────────────────── plan/14 C15 · 失败说人话 ─────────────────────

/// Which real failure shape a raw executor/action error text matched, driving
/// which headline `explain_failure` picked. `Unknown` is not a failure of the
/// classifier — it's the honest "we don't have a canned translation for this
/// one yet" case (规范条 3: "其余类别不硬翻").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum FailureCategory {
    /// `crates/bw-engine/src/claude_cli.rs`'s `CliResult::error_text` when
    /// `result` is empty and `errors`/`subtype` carry
    /// `subtype=error_max_budget_usd` — the real text a user hit
    /// (plan/14 缘起台账 #3): `"Reached maximum budget ($0.5)
    /// (subtype=error_max_budget_usd)"`.
    BudgetExhausted,
    /// `is_transient_gateway_error` in the same file: `"API Error: 529/503/
    /// 502/504"`, or a message containing "overloaded"/"访问量过大". By the
    /// time this reaches the UI the executor's own bounded retry
    /// (`TRANSIENT_BACKOFF_SECS`) has already been exhausted.
    GatewayTransient,
    /// The `ATTEMPT_TIMEOUT_SECS` guard's own text: `"claude CLI attempt
    /// exceeded {N}s (hung child killed)"`.
    Timeout,
    /// Any other real failure text (spawn failure, JSON parse failure, empty
    /// workspace, `gh` CLI errors bubbled through `ActionState::Fail`, …) —
    /// not guessed at, shown via a generic sentence + the untouched original.
    Unknown,
}

/// A raw failure string translated into what a human should read first, with
/// the original **never discarded** — `raw` always carries the exact text
/// that came out of the executor/action, verbatim, so nothing is hidden, only
/// deprioritized behind a fold (规范条 3: "如实,不隐藏").
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FailureExplanation {
    pub category: FailureCategory,
    /// One user-language sentence. For `Unknown` this is a generic "起草没
    /// 走完" line, never a fabricated specific cause.
    pub headline: String,
    /// The untouched input to `explain_failure` — the technical-details fold
    /// renders this, byte for byte.
    pub raw: String,
}

/// Pure text classifier — no IO, no async, matches only against the real
/// literal shapes `crates/bw-engine/src/claude_cli.rs` is confirmed to
/// produce (see `FailureCategory`'s doc comments for each pattern's exact
/// source). Never guesses at meaning for text it doesn't recognize: those
/// fall through to `Unknown` with a generic headline, `raw` intact.
///
/// Shared by the drafting-run failure card and the `ActionsBanner` fail
/// state (plan/14 C15, 规范条 3) — both hold a raw `String` from a real
/// `Err`/`ActionState::Fail`, never a value this function invents.
pub fn explain_failure(raw: &str) -> FailureExplanation {
    let lower = raw.to_ascii_lowercase();

    if raw.contains("subtype=error_max_budget_usd") || lower.contains("reached maximum budget") {
        return FailureExplanation {
            category: FailureCategory::BudgetExhausted,
            headline: "预算到顶,起草没做完——重试会重新计费".to_string(),
            raw: raw.to_string(),
        };
    }

    let gateway_markers = [
        "API Error: 529",
        "API Error: 503",
        "API Error: 502",
        "API Error: 504",
    ];
    if gateway_markers.iter().any(|m| raw.contains(m))
        || lower.contains("overloaded")
        || raw.contains("访问量过大")
    {
        return FailureExplanation {
            category: FailureCategory::GatewayTransient,
            headline: "AI 网关暂时不可用,稍等重试通常就好".to_string(),
            raw: raw.to_string(),
        };
    }

    if raw.contains("hung child killed") {
        return FailureExplanation {
            category: FailureCategory::Timeout,
            headline: "执行超时被终止,可重试".to_string(),
            raw: raw.to_string(),
        };
    }

    FailureExplanation {
        category: FailureCategory::Unknown,
        headline: "起草没走完,原因未归类——技术详情里是原始报错,可直接重试".to_string(),
        raw: raw.to_string(),
    }
}
