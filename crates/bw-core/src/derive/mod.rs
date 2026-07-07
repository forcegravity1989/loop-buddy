//! The metric → signal → health derivation chain (plan `§2.5`), and the sealed
//! [`Derived`] that makes "health is always derived" a compile-time fact.
//!
//! Six layers, bottom-up:
//!
//! | L | what | here |
//! |---|------|------|
//! | **L0** Observation | raw event, append-only | stored (bw-store) |
//! | **L1** MeasuredValue | normalize to scalar + freshness | [`measure`] |
//! | **L2** value-vs-target | the missing layer; (value,target)→signal | [`evaluate_metric`] |
//! | **L3** StageMetric.signal | one KPI | [`evaluate_metric`] |
//! | **L4** Routine.signal | worst-of its KPIs | [`reduce_worst_of`] |
//! | **L5** OpStage health | projection of L4 (selector) | `ui::` |
//! | **L6** Project.signal | worst-of its 7 stages | [`reduce_worst_of`] |
//!
//! L0/L1 are the *only* birthplace of a value; L2/L4/L6 the *only* birthplace of
//! a [`Signal`](crate::model::Signal). Everything above L4 is pure projection.

mod eval;
mod measure;
mod sealed;
mod target;

pub use eval::{evaluate_metric, reduce_worst_of, MetricEvaluation};
pub use measure::{measure, MeasuredValue, Measurement, MetricShape};
pub use sealed::Derived;
pub use target::{
    parse_target, parse_target_with, AmberBand, Comparator, Target, TargetParseError,
};
