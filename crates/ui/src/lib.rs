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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_is_not_green() {
        assert_ne!(signal_color(Signal::Unknown), signal_color(Signal::Green));
        assert_eq!(signal_color(Signal::Green), "#5F7355");
    }

    #[test]
    fn progress_and_overall() {
        assert_eq!(progress_color(100), "#5F7355");
        assert_eq!(progress_color(99), "#C5654A");
        assert_eq!(overall_progress(&[60, 80, 100, 40, 0, 20, 80]), 54);
        assert_eq!(overall_progress(&[]), 0);
    }

    #[test]
    fn wow() {
        assert_eq!(wow_delta(&[1.0, 2.0, 3.0]), WowDir::Up);
        assert_eq!(wow_delta(&[3.0, 1.0]), WowDir::Down);
        assert_eq!(wow_delta(&[2.0, 2.0]), WowDir::Flat);
        assert_eq!(wow_delta(&[5.0]), WowDir::Flat);
        assert_eq!(wow_delta(&[]), WowDir::Flat);
    }

    #[test]
    fn sparkline_geometry() {
        let sp = sparkline_path(&[0.0, 5.0, 10.0], 100.0, 20.0);
        // 3 points across the width; min at bottom (y=h), max at top (y=0).
        assert_eq!(sp.polyline, "0.0,20.0 50.0,10.0 100.0,0.0");
        assert!(sp.area.starts_with("M 0.0,20.0"));
        assert!(sp.area.ends_with("Z"));
        assert_eq!((sp.last_x, sp.last_y), (100.0, 0.0));

        // Flat series → mid-height, no NaN.
        let flat = sparkline_path(&[7.0, 7.0], 10.0, 20.0);
        assert_eq!(flat.last_y, 10.0);

        // Empty → empty geometry, no panic.
        assert_eq!(sparkline_path(&[], 10.0, 10.0).polyline, "");
    }

    #[test]
    fn green_hides_only_trouble_speaks() {
        let stages = [
            StageAttention {
                kind: StageKind::Prototype,
                signal: Signal::Green,
                active_sessions: 0,
            },
            StageAttention {
                kind: StageKind::Build,
                signal: Signal::Amber,
                active_sessions: 0,
            },
            StageAttention {
                kind: StageKind::Optimize,
                signal: Signal::Green,
                active_sessions: 2,
            },
            StageAttention {
                kind: StageKind::Ops,
                signal: Signal::Unknown,
                active_sessions: 0,
            },
        ];
        let a = overview_attention(&stages);
        // green + no session is hidden in the steady count
        assert_eq!(a.steady, 1);
        // amber + unknown surface as "needs watching"
        assert_eq!(a.watch.len(), 2);
        // the stage with an active session asks for you (even though it's green)
        assert_eq!(a.needs_you, vec![StageKind::Optimize]);
    }

    #[test]
    fn stage_tint_covers_all_five() {
        for k in StageKind::ALL {
            let (bg, fg, bd) = stage_tint(k);
            assert!(!bg.is_empty() && !fg.is_empty() && !bd.is_empty());
        }
    }
}
