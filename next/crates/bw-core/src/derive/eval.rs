//! **L2 evaluate + L4/L6 reduce.** The only two functions allowed to mint a
//! [`Derived<Signal>`]:
//!
//! - [`evaluate_metric`] — one (value, target) → one signal (the L2/L3 leaf).
//! - [`reduce_worst_of`] — many signals → one (the L4 routine / L6 project roll-up).

use super::measure::Measurement;
use super::sealed::Derived;
use super::target::{AmberBand, Comparator, Target};
use crate::model::Signal;
use serde::Serialize;

/// Outcome of evaluating one metric against its target (L2).
#[derive(Clone, Copy, Debug, Serialize)]
pub struct MetricEvaluation {
    signal: Derived<Signal>,
    /// `true` iff the (post-staleness) signal is Green.
    pub hit: bool,
    /// Signed gap `value − target` for threshold targets; `None` otherwise.
    pub distance: Option<f64>,
}

impl MetricEvaluation {
    /// The derived signal value.
    pub fn signal(&self) -> Signal {
        self.signal.into_inner()
    }

    /// The sealed signal, to store directly into a write-through cache field.
    pub fn derived(&self) -> Derived<Signal> {
        self.signal
    }
}

/// **L2.** Evaluate a measurement against a target. `trend` is only consulted by
/// [`Target::DirectionUp`]; pass the recent series (e.g. weekly progress).
///
/// Honesty rules baked in:
/// - `Missing` ⇒ `Unknown` (never green on no data).
/// - `stale` source caps a Green at Amber ("a fresh green can't mask a dead source").
/// - `TrackOnly` ⇒ `Unknown` (observe, don't grade).
pub fn evaluate_metric(m: &Measurement, target: &Target, trend: &[f64]) -> MetricEvaluation {
    let (raw, distance) = match m {
        Measurement::Missing => (Signal::Unknown, None),
        Measurement::Value(v) => judge(v.magnitude, target, trend),
    };

    let stale = matches!(m, Measurement::Value(v) if v.stale);
    let signal = if stale && raw == Signal::Green {
        Signal::Amber
    } else {
        raw
    };

    MetricEvaluation {
        signal: Derived::seal(signal),
        hit: signal == Signal::Green,
        distance,
    }
}

/// **L4 / L6.** Worst-of reduction over child signals (plan `§2.5`):
/// any Red ⇒ Red; else any Amber ⇒ Amber; else (any Unknown *and* no Green) ⇒
/// Unknown; else Green. An empty input ⇒ Unknown (no data ≠ healthy).
pub fn reduce_worst_of(signals: impl IntoIterator<Item = Signal>) -> Derived<Signal> {
    let mut any_amber = false;
    let mut any_unknown = false;
    let mut any_green = false;

    for s in signals {
        match s {
            Signal::Red => return Derived::seal(Signal::Red),
            Signal::Amber => any_amber = true,
            Signal::Unknown => any_unknown = true,
            Signal::Green => any_green = true,
        }
    }

    let result = if any_amber {
        Signal::Amber
    } else if any_unknown && !any_green {
        Signal::Unknown
    } else if any_green {
        Signal::Green
    } else {
        // Empty input: nothing observed at all.
        Signal::Unknown
    };
    Derived::seal(result)
}

/// Core threshold/qualitative judgment (no staleness handling — that's applied
/// by the caller). Returns the raw signal + signed distance.
fn judge(value: f64, target: &Target, trend: &[f64]) -> (Signal, Option<f64>) {
    match target {
        Target::TrackOnly => (Signal::Unknown, None),

        // Direction alone never goes Red; needs ≥2 points or it's Unknown.
        Target::DirectionUp => {
            if let [.., prev, last] = trend {
                let s = if last > prev {
                    Signal::Green
                } else {
                    Signal::Amber
                };
                (s, Some(last - prev))
            } else {
                (Signal::Unknown, None)
            }
        }

        // 清零 / 全覆盖 are strict by default (no band) — flagged as an open
        // P0 design question (plan `§2.5` 开放设计问题); a per-metric band can
        // soften them later without changing the chain.
        Target::DriveToZero => {
            judge_threshold(value, Comparator::Le, 0.0, AmberBand::AbsPoints(0.0))
        }
        Target::FullCoverage => {
            judge_threshold(value, Comparator::Ge, 100.0, AmberBand::AbsPoints(0.0))
        }

        Target::Threshold {
            cmp,
            value: t,
            amber,
            ..
        } => judge_threshold(value, *cmp, *t, *amber),
    }
}

/// Threshold rule with an Amber band (plan `§2.5` 判定):
/// HigherIsBetter — `≥T`→Green, `[T−β, T)`→Amber, `<T−β`→Red. LowerIsBetter is
/// the mirror; `Eq` is green at the value, amber within β, red outside.
fn judge_threshold(
    value: f64,
    cmp: Comparator,
    target: f64,
    amber: AmberBand,
) -> (Signal, Option<f64>) {
    let band = match amber {
        AmberBand::RelPct(p) => target.abs() * p,
        AmberBand::AbsPoints(a) => a,
    };
    let distance = value - target;

    let signal = if matches!(cmp, Comparator::Eq) {
        let d = distance.abs();
        if d <= f64::EPSILON {
            Signal::Green
        } else if d <= band {
            Signal::Amber
        } else {
            Signal::Red
        }
    } else if cmp.higher_is_better() {
        if value >= target {
            Signal::Green
        } else if value >= target - band {
            Signal::Amber
        } else {
            Signal::Red
        }
    } else {
        // lower is better
        if value <= target {
            Signal::Green
        } else if value <= target + band {
            Signal::Amber
        } else {
            Signal::Red
        }
    };

    (signal, Some(distance))
}
