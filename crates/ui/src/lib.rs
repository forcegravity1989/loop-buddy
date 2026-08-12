//! `ui` — shared ViewModel + pure-function selectors (plan `§4`), the Rust port
//! of the prototype `buildApp()`. Every function here is pure over `bw-core`
//! types and unit-tested, so UI correctness is provable without a window. Stays
//! wasm32-clean for the Web keepalive.

#![forbid(unsafe_code)]

pub mod vm;

use bw_core::model::{Signal, StageKind};
use serde::Serialize;
use time::OffsetDateTime;
use vm::iso_week_start_unix;

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

/// One plotted sample for a labelled trend chart (axes + value markers).
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TrendPoint {
    pub x: f32,
    pub y: f32,
    pub value: f32,
    /// Short x-axis label: ISO week-end date `MM-DD` (Sunday).
    pub x_label: String,
    /// Value label drawn near the marker.
    pub value_label: String,
}

/// Geometry for a readable weekly trend chart (not a tiny sparkline): plot
/// box, polyline/area, per-point markers, and axis tick labels.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct TrendChart {
    pub width: f32,
    pub height: f32,
    pub plot_left: f32,
    pub plot_top: f32,
    pub plot_w: f32,
    pub plot_h: f32,
    pub polyline: String,
    pub area: String,
    pub points: Vec<TrendPoint>,
    /// `(y_px, label)` for the left axis — max / mid / min.
    pub y_ticks: Vec<(f32, String)>,
    pub y_min: f32,
    pub y_max: f32,
}

fn format_tick(v: f32) -> String {
    if (v - v.round()).abs() < 0.05 {
        format!("{}", v.round() as i32)
    } else {
        format!("{v:.1}")
    }
}

/// Week-bucket x labels for an oldest→newest series ending at the current
/// ISO week. Label = that week's Sunday (`MM-DD`), matching `weekly_spark`
/// buckets so a Monday leadership read shows calendar dates, not "本周/−N".
fn week_x_label(i: usize, n: usize, now_unix: i64) -> String {
    if n == 0 {
        return String::new();
    }
    let weeks_ago = (n - 1 - i) as i64;
    const DAY: i64 = 86_400;
    let week_start = iso_week_start_unix(now_unix);
    let sunday = week_start + 6 * DAY - weeks_ago * 7 * DAY;
    let dt = OffsetDateTime::from_unix_timestamp(sunday).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    format!("{:02}-{:02}", u8::from(dt.month()), dt.day())
}

/// Build a chart with left/bottom gutters for axis labels. `total_w`/`total_h`
/// are the full SVG size; the plot sits inside padded margins. `now_unix`
/// anchors week-end date labels to the same ISO week as `weekly_spark`.
pub fn trend_chart(trend: &[f32], total_w: f32, total_h: f32, now_unix: i64) -> TrendChart {
    let pad_l = 36.0;
    let pad_r = 10.0;
    let pad_t = 18.0; // room for value labels above markers
    let pad_b = 22.0;
    let plot_w = (total_w - pad_l - pad_r).max(1.0);
    let plot_h = (total_h - pad_t - pad_b).max(1.0);

    if trend.is_empty() {
        return TrendChart {
            width: total_w,
            height: total_h,
            plot_left: pad_l,
            plot_top: pad_t,
            plot_w,
            plot_h,
            polyline: String::new(),
            area: String::new(),
            points: Vec::new(),
            y_ticks: Vec::new(),
            y_min: 0.0,
            y_max: 0.0,
        };
    }

    let min = trend.iter().cloned().fold(f32::INFINITY, f32::min);
    let max = trend.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    // Flat series: give a 1-unit pad so the line isn't glued to the border.
    let (y_min, y_max) = if (max - min) <= f32::EPSILON {
        let base = min;
        (base - 1.0, base + 1.0)
    } else {
        let pad = (max - min) * 0.08;
        (min - pad, max + pad)
    };
    let span = (y_max - y_min).max(f32::EPSILON);
    let n = trend.len();

    let x_at = |i: usize| -> f32 {
        let local = if n == 1 {
            plot_w / 2.0
        } else {
            (i as f32 / (n - 1) as f32) * plot_w
        };
        pad_l + local
    };
    let y_at = |v: f32| -> f32 { pad_t + (plot_h - ((v - y_min) / span) * plot_h) };

    let mut polyline = String::new();
    let mut points = Vec::with_capacity(n);
    for (i, &v) in trend.iter().enumerate() {
        let x = x_at(i);
        let y = y_at(v);
        if i > 0 {
            polyline.push(' ');
        }
        polyline.push_str(&format!("{x:.1},{y:.1}"));
        points.push(TrendPoint {
            x,
            y,
            value: v,
            x_label: week_x_label(i, n, now_unix),
            value_label: format_tick(v),
        });
    }

    let area = if n == 1 {
        String::new()
    } else {
        let bottom = pad_t + plot_h;
        let mut a = format!("M {:.1},{:.1}", x_at(0), bottom);
        for (i, &v) in trend.iter().enumerate() {
            a.push_str(&format!(" L {:.1},{:.1}", x_at(i), y_at(v)));
        }
        a.push_str(&format!(" L {:.1},{:.1} Z", x_at(n - 1), bottom));
        a
    };

    let mid = (y_min + y_max) / 2.0;
    let y_ticks = vec![
        (y_at(y_max), format_tick(y_max)),
        (y_at(mid), format_tick(mid)),
        (y_at(y_min), format_tick(y_min)),
    ];

    TrendChart {
        width: total_w,
        height: total_h,
        plot_left: pad_l,
        plot_top: pad_t,
        plot_w,
        plot_h,
        polyline,
        area,
        points,
        y_ticks,
        y_min,
        y_max,
    }
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
            w / 2.0 // PF1-R4: n==1 居中,避免 circle 被左边缘裁掉
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

    // Area: baseline → curve → baseline, closed. n==1 时退化为零宽竖线,
    // 不画(area 留空)——polyline 仍 1 点,Spark 的 circle 在 (w/2, y) 显点。
    let area = if n == 1 {
        String::new()
    } else {
        let mut a = format!("M {:.1},{:.1}", x_at(0), h);
        for (i, &v) in trend.iter().enumerate() {
            a.push_str(&format!(" L {:.1},{:.1}", x_at(i), y_at(v)));
        }
        a.push_str(&format!(" L {:.1},{:.1} Z", x_at(n - 1), h));
        a
    };

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

#[cfg(test)]
mod trend_chart_tests {
    use super::trend_chart;
    use time::{Date, Month, PrimitiveDateTime, Time};

    fn unix_at(y: i32, month: Month, d: u8) -> i64 {
        PrimitiveDateTime::new(
            Date::from_calendar_date(y, month, d).expect("date"),
            Time::from_hms(12, 0, 0).expect("time"),
        )
        .assume_utc()
        .unix_timestamp()
    }

    #[test]
    fn labels_axes_and_points_with_week_end_dates() {
        // Wed 2026-08-12 → current ISO week ends Sun 08-16.
        let now = unix_at(2026, Month::August, 12);
        let c = trend_chart(&[1.0, 3.0, 6.0], 320.0, 128.0, now);
        assert_eq!(c.points.len(), 3);
        assert_eq!(c.points[0].x_label, "08-02");
        assert_eq!(c.points[1].x_label, "08-09");
        assert_eq!(c.points[2].x_label, "08-16");
        assert_eq!(c.points[2].value_label, "6");
        assert_eq!(c.y_ticks.len(), 3);
        assert!(!c.polyline.is_empty());
    }

    #[test]
    fn empty_trend_is_empty_chart() {
        let c = trend_chart(&[], 320.0, 128.0, 0);
        assert!(c.points.is_empty());
        assert!(c.polyline.is_empty());
    }
}
