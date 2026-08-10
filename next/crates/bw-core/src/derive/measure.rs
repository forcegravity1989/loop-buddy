//! **L0 Observation → L1 MeasuredValue.** Deterministic ingestion: turn a raw
//! display string (`"60%"`, `"5/7"`, `"842ms"`, `"18h"`, `"8"`) plus its
//! freshness context into a normalized scalar, or [`Measurement::Missing`].
//!
//! Pure & wasm-clean: `now` is passed in, never read from the clock here.

use crate::model::{Cadence, SourceKind};
use serde::Serialize;
use time::{Duration, OffsetDateTime};

/// Coarse shape of a metric value, inferred from its display string. Drives unit
/// handling now; reserved for per-metric aggregation (latest/mean/p95) in Tier D.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum MetricShape {
    Percent,
    Count,
    Ratio,
    DurationMs,
    RawNumber,
}

/// L1 output: a normalized scalar with provenance and freshness.
#[derive(Clone, Debug, Serialize)]
pub struct MeasuredValue {
    /// Normalized magnitude. Percent in points (`60.0`), ratio as a fraction
    /// (`5/7 → 0.714…`), durations in milliseconds, others as-is.
    pub magnitude: f64,
    /// Normalized unit: `"%"`, `"ms"`, `"ratio"`, or `""`.
    pub unit: String,
    pub shape: MetricShape,
    /// Original ratio operands, kept for display (`5/7` stays `5/7`).
    pub ratio: Option<(u32, u32)>,
    pub as_of: OffsetDateTime,
    pub source: SourceKind,
    /// Latest observation older than the cadence window ⇒ `true`. A stale source
    /// caps the derived signal at Amber (see `evaluate_metric`).
    pub stale: bool,
}

/// L1 result. `Missing` is a first-class outcome — it derives to `Unknown`,
/// never to green. No observation ⇒ no value (plan `§2.5`).
#[derive(Clone, Debug, Serialize)]
pub enum Measurement {
    Value(MeasuredValue),
    Missing,
}

/// Ingest a raw value into a [`Measurement`].
///
/// - `raw_value`  — display string from an Observation (Manual or Connector).
/// - `as_of`      — when it was observed.
/// - `cadence`    — expected refresh rhythm; defines the staleness window.
/// - `now`        — caller-supplied clock (kept pure for wasm + determinism).
pub fn measure(
    raw_value: &str,
    as_of: OffsetDateTime,
    source: SourceKind,
    cadence: &Cadence,
    now: OffsetDateTime,
) -> Measurement {
    match parse_scalar(raw_value) {
        None => Measurement::Missing,
        Some(s) => Measurement::Value(MeasuredValue {
            magnitude: s.magnitude,
            unit: s.unit,
            shape: s.shape,
            ratio: s.ratio,
            as_of,
            source,
            stale: now - as_of > cadence_window(cadence),
        }),
    }
}

/// Parse just the magnitude of a display value (`"60%"→60`, `"5/7"→0.714…`,
/// `"842ms"→842`, `"8"→8`). For plotting real observation history as a trend —
/// same normalization as [`measure`], no freshness context needed.
pub fn parse_magnitude(raw: &str) -> Option<f64> {
    parse_scalar(raw).map(|s| s.magnitude)
}

/// How long until a source of the given cadence is considered stale.
fn cadence_window(c: &Cadence) -> Duration {
    match c {
        Cadence::RealTime => Duration::minutes(10),
        Cadence::Daily => Duration::hours(24),
        Cadence::Weekly => Duration::days(7),
        // Conservative default until Tier D parses the cron expression.
        Cadence::Cron(_) => Duration::days(1),
    }
}

/// A parsed scalar: magnitude + normalized unit (+ ratio operands for display).
pub(crate) struct Scalar {
    pub magnitude: f64,
    pub unit: String,
    pub shape: MetricShape,
    pub ratio: Option<(u32, u32)>,
}

/// Parse a human display value. Returns `None` for empty / unrecognized input
/// (which the caller turns into `Missing` / `Unknown`). Shared by the target
/// parser so a target's number is normalized exactly like a value's.
pub(crate) fn parse_scalar(raw: &str) -> Option<Scalar> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }

    // Ratio: "5/7"
    if let Some((num, den)) = s.split_once('/') {
        let n: u32 = num.trim().parse().ok()?;
        let d: u32 = den.trim().parse().ok()?;
        if d == 0 {
            return None;
        }
        return Some(Scalar {
            magnitude: f64::from(n) / f64::from(d),
            unit: "ratio".into(),
            shape: MetricShape::Ratio,
            ratio: Some((n, d)),
        });
    }

    // Percent: "60%", "99.95%"
    if let Some(num) = s.strip_suffix('%') {
        let v: f64 = num.trim().replace(',', "").parse().ok()?;
        return Some(Scalar {
            magnitude: v,
            unit: "%".into(),
            shape: MetricShape::Percent,
            ratio: None,
        });
    }

    // Number with optional trailing unit: "842ms", "18h", "8", "1,200".
    let split = s
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(s.len());
    let (num_part, unit_part) = s.split_at(split);
    let magnitude: f64 = num_part.replace(',', "").parse().ok()?;
    let unit = unit_part.trim();

    if unit.is_empty() {
        return Some(Scalar {
            magnitude,
            unit: String::new(),
            shape: MetricShape::RawNumber,
            ratio: None,
        });
    }
    if let Some(ms) = duration_to_ms(magnitude, unit) {
        return Some(Scalar {
            magnitude: ms,
            unit: "ms".into(),
            shape: MetricShape::DurationMs,
            ratio: None,
        });
    }
    // Unknown unit (e.g. a count suffix) — keep number, treat as a count.
    Some(Scalar {
        magnitude,
        unit: unit.to_string(),
        shape: MetricShape::Count,
        ratio: None,
    })
}

/// Normalize a duration to milliseconds, or `None` if the unit isn't a duration.
fn duration_to_ms(v: f64, unit: &str) -> Option<f64> {
    let factor = match unit {
        "ms" => 1.0,
        "s" | "sec" | "秒" => 1_000.0,
        "min" | "分" | "分钟" => 60_000.0,
        "h" | "hr" | "小时" => 3_600_000.0,
        "d" | "天" => 86_400_000.0,
        _ => return None,
    };
    Some(v * factor)
}
