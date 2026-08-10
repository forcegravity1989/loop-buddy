//! **L2 target model + parser.** The prototype only ever stored hand-written
//! signals; "value-vs-target" was never built (plan `§2.5`). This is it: parse
//! every target syntax found in the source into a [`Target`], so the signal can
//! be *computed* from (value, target) instead of typed by a human.
//!
//! Recognized syntax (from the prototype): `≥5` `≤24h` `<800` `>3` `=10`
//! `100%` `7/7` (bare ⇒ implicit `≥`), and the qualitative tokens
//! `清零` `全覆盖` `↑` `跟踪`.

use super::measure::parse_scalar;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Comparison direction of a threshold target.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Comparator {
    /// `≥` / `>=`
    Ge,
    /// `>`
    Gt,
    /// `≤` / `<=`
    Le,
    /// `<`
    Lt,
    /// `=` / `==`
    Eq,
}

impl Comparator {
    /// `≥`/`>` mean higher is better; `≤`/`<` mean lower is better.
    pub fn higher_is_better(self) -> bool {
        matches!(self, Comparator::Ge | Comparator::Gt)
    }
}

/// The Amber tolerance band around a threshold — **per metric, stored, honest**
/// (plan `§2.5`). A flat relative 10% is wrong for tight targets: 10% of a
/// `99.9%` availability target would green-light `89.9%`. Use [`AmberBand::AbsPoints`]
/// there instead.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum AmberBand {
    /// Band as a fraction of the target value (default `0.10`).
    RelPct(f64),
    /// Band as absolute points in the metric's unit.
    AbsPoints(f64),
}

impl Default for AmberBand {
    fn default() -> Self {
        AmberBand::RelPct(0.10)
    }
}

/// A parsed target. Threshold targets carry their Amber band; qualitative ones
/// have fixed semantics (see [`evaluate_metric`](super::eval::evaluate_metric)).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Target {
    Threshold {
        cmp: Comparator,
        value: f64,
        unit: String,
        amber: AmberBand,
    },
    /// `清零` — drive a count to zero (lower-is-better at 0).
    DriveToZero,
    /// `全覆盖` — full coverage (≥ 100%).
    FullCoverage,
    /// `↑` — direction-only: green iff trending up, never red on direction alone.
    DirectionUp,
    /// `跟踪` — observe only; always derives to `Unknown` (no pass/fail).
    TrackOnly,
}

/// Why a target string could not be parsed. Surfaced to a target-editor lint —
/// never silently swallowed (an unparseable target derives to `Unknown`).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum TargetParseError {
    #[error("empty target string")]
    Empty,
    #[error("unrecognized target syntax: {0:?}")]
    Unrecognized(String),
}

/// Parse a target with the default Amber band (`RelPct(0.10)`).
pub fn parse_target(raw: &str) -> Result<Target, TargetParseError> {
    parse_target_with(raw, AmberBand::default())
}

/// Parse a target, supplying the per-metric Amber band for threshold targets.
pub fn parse_target_with(raw: &str, amber: AmberBand) -> Result<Target, TargetParseError> {
    let s = raw.trim();
    if s.is_empty() {
        return Err(TargetParseError::Empty);
    }

    // Qualitative tokens (whole-string match).
    match s {
        "清零" => return Ok(Target::DriveToZero),
        "全覆盖" => return Ok(Target::FullCoverage),
        "↑" | "上升" | "提升" => return Ok(Target::DirectionUp),
        "跟踪" | "观察" => return Ok(Target::TrackOnly),
        _ => {}
    }

    // Threshold: optional comparator prefix, then a scalar. Bare `100%` / `7/7`
    // mean an implicit lower bound (`≥`).
    let (cmp, rest) = strip_comparator(s);
    let scalar =
        parse_scalar(rest).ok_or_else(|| TargetParseError::Unrecognized(raw.to_string()))?;
    Ok(Target::Threshold {
        cmp: cmp.unwrap_or(Comparator::Ge),
        value: scalar.magnitude,
        unit: scalar.unit,
        amber,
    })
}

/// Split a leading comparator (Unicode or ASCII) off the front of `s`.
fn strip_comparator(s: &str) -> (Option<Comparator>, &str) {
    for (p, c) in [
        (">=", Comparator::Ge),
        ("<=", Comparator::Le),
        ("==", Comparator::Eq),
    ] {
        if let Some(r) = s.strip_prefix(p) {
            return (Some(c), r.trim_start());
        }
    }
    let mut chars = s.chars();
    let cmp = match chars.clone().next() {
        Some('≥') => Some(Comparator::Ge),
        Some('≤') => Some(Comparator::Le),
        Some('>') => Some(Comparator::Gt),
        Some('<') => Some(Comparator::Lt),
        Some('=') => Some(Comparator::Eq),
        _ => None,
    };
    if cmp.is_some() {
        chars.next();
        (cmp, chars.as_str().trim_start())
    } else {
        (None, s)
    }
}
