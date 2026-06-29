//! `bw-store` — local-first persistence behind a [`Store`] trait.
//!
//! Two encoded invariants (plan `§2.5` / `§5`):
//! 1. **Values are born only as observations.** [`Store::append_observation`] is
//!    append-only; there is no value setter elsewhere.
//! 2. **Signals are written only by derive.** [`Store::recompute_signals`] is the
//!    *sole* writer of every `signal` / `hit` column — the trait exposes no
//!    `set_signal`. It reads observations + targets, calls `bw_core::derive`, and
//!    writes the resulting cache.
//!
//! The trait is the seam for Tier E: swap [`SqliteStore`] for an IndexedDB /
//! remote adapter with no schema migration (`updated_at + rev` on every table).

#![forbid(unsafe_code)]

use async_trait::async_trait;
use bw_core::derive::AmberBand;
use bw_core::model::{Cadence, ProjectPhase, Role, Signal, SourceKind, StageKind, StagePhase};
use bw_core::{MetricId, ProjectId, SessionId};
use time::OffsetDateTime;

mod sqlite;
pub use sqlite::SqliteStore;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// Leading (controllable) vs lagging (outcome) metric.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MetricRole {
    Leading,
    Lagging,
}

/// Create (build) vs optimize (iterate) task session.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SessionKind {
    Create,
    Optimize,
}

// ───────────────────────────── write DTOs ─────────────────────────────

pub struct NewProject {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
}

pub struct NewMetric {
    pub id: MetricId,
    pub project_id: ProjectId,
    pub role: MetricRole,
    pub stage_kind: Option<StageKind>,
    pub name: String,
    pub def: String,
    pub target_raw: String,
    pub amber: AmberBand,
    pub last_target: String,
    pub driver: String,
    pub pos: i64,
}

pub struct NewStage {
    pub project_id: ProjectId,
    pub kind: StageKind,
    pub phase: StagePhase,
    pub progress: u8,
    pub schedule: Cadence,
    pub owns: String,
    pub accept: String,
    pub control: String,
}

pub struct NewSession {
    pub id: SessionId,
    pub project_id: ProjectId,
    pub stage_kind: Option<StageKind>,
    pub kind: SessionKind,
    pub title: String,
    pub snippet: String,
}

// ───────────────────────────── read DTOs ─────────────────────────────

#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
    pub phase: ProjectPhase,
    pub cold_step: Option<u8>,
    pub north_star: String,
    pub ns_def: String,
    /// Cached derived signal (read-only; recompute is authoritative).
    pub signal: Option<Signal>,
    pub weekly_signal: Option<Signal>,
}

#[derive(Clone, Debug)]
pub struct MetricSignal {
    pub name: String,
    pub value_raw: String,
    pub target_raw: String,
    pub stage_kind: Option<StageKind>,
    pub signal: Option<Signal>,
    pub hit: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct StageSignal {
    pub kind: StageKind,
    pub routine: Option<Signal>,
}

/// A stage's persisted operating definition — the `owns` / `accept` / `control`
/// triplet plus phase/progress — read straight from `op_stage`. Signals are
/// **not** carried here; the UI joins these against [`PersistedSignals::stages`]
/// (the derive cache) by [`StageKind`]. Returned in [`StageKind::ALL`] order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageDetail {
    pub kind: StageKind,
    pub phase: StagePhase,
    pub progress: u8,
    pub owns: String,
    pub accept: String,
    pub control: String,
}

/// A metric's recent **real** observation series for a sparkline — the numeric
/// prefix of each `observation.raw`, oldest→newest, capped at the most recent
/// [`MetricTrend::CAP`] points. The series is *only* what was actually persisted:
/// one observation ⇒ one point (a flat, honest sparkline). `stage_kind` lets the
/// UI bucket the trend under its control point.
#[derive(Clone, Debug, PartialEq)]
pub struct MetricTrend {
    pub name: String,
    pub stage_kind: Option<StageKind>,
    /// Parsed numeric prefixes of the recent observations, oldest→newest.
    pub trend: Vec<f32>,
}

impl MetricTrend {
    /// Most-recent observations to surface in a sparkline (plan `§3.3` ~6 weeks;
    /// we cap a little higher so denser cadences still read).
    pub const CAP: usize = 12;
}

/// The persisted derived caches for a project — what the UI reads cheaply and
/// what the integration test checks against an independent `bw_core` recompute.
#[derive(Clone, Debug)]
pub struct PersistedSignals {
    pub project: Option<Signal>,
    pub weekly: Option<Signal>,
    pub stages: Vec<StageSignal>,
    pub metrics: Vec<MetricSignal>,
}

#[derive(Clone, Debug)]
pub struct SessionRow {
    pub id: SessionId,
    pub title: String,
    pub kind: SessionKind,
}

#[derive(Clone, Debug)]
pub struct MessageRow {
    pub role: Role,
    pub text: String,
}

// ───────────────────────────── the trait ─────────────────────────────

#[async_trait]
pub trait Store: Send + Sync {
    async fn create_project(&self, p: NewProject) -> Result<()>;
    async fn set_project_phase(
        &self,
        id: ProjectId,
        phase: ProjectPhase,
        cold_step: Option<u8>,
    ) -> Result<()>;
    async fn set_north_star(&self, id: ProjectId, north_star: &str, ns_def: &str) -> Result<()>;

    async fn upsert_metric(&self, m: NewMetric) -> Result<()>;
    /// Append-only — the sole birthplace of a value.
    async fn append_observation(
        &self,
        metric_id: MetricId,
        source: SourceKind,
        raw: &str,
        ts: OffsetDateTime,
    ) -> Result<()>;

    async fn materialize_stages(&self, stages: Vec<NewStage>) -> Result<()>;

    async fn ensure_session(&self, s: NewSession) -> Result<()>;
    async fn append_message(&self, session_id: SessionId, role: Role, text: &str) -> Result<()>;

    /// **The sole signal writer** — reads observations + targets, derives via
    /// `bw_core`, writes every `signal` / `hit` cache for the project.
    async fn recompute_signals(&self, project_id: ProjectId, now: OffsetDateTime) -> Result<()>;

    /// Record a weekly-review snapshot. `derived` is the machine truth; an
    /// optional `human_override` is stored *alongside* it (never overwriting) so
    /// the divergence stays auditable (plan `§2.5`).
    async fn annotate_weekly_review(
        &self,
        project_id: ProjectId,
        week_of: OffsetDateTime,
        derived: Signal,
        human_override: Option<Signal>,
        reason: &str,
    ) -> Result<()>;

    // reads
    async fn get_project(&self, id: ProjectId) -> Result<Option<ProjectRow>>;
    async fn list_projects(&self) -> Result<Vec<ProjectRow>>;
    async fn persisted_signals(&self, id: ProjectId) -> Result<PersistedSignals>;
    /// Per-stage operating definition (`owns`/`accept`/`control` + phase/progress)
    /// from `op_stage`, in [`StageKind::ALL`] order. Read-only; no derive here.
    async fn stage_details(&self, project: ProjectId) -> Result<Vec<StageDetail>>;
    /// Each metric's recent real observation series for sparklines (numeric prefix
    /// of `observation.raw`, oldest→newest, capped at [`MetricTrend::CAP`]). Metric
    /// order mirrors `metric.pos`. Never fabricated — a metric with one observation
    /// yields a one-point trend.
    async fn metric_trends(&self, project: ProjectId) -> Result<Vec<MetricTrend>>;
    async fn list_sessions(&self, project_id: ProjectId) -> Result<Vec<SessionRow>>;
    async fn session_messages(&self, session_id: SessionId) -> Result<Vec<MessageRow>>;
}

// ───────────────────────── text codecs (shared) ─────────────────────────

pub(crate) fn sig_text(s: Signal) -> &'static str {
    match s {
        Signal::Green => "green",
        Signal::Amber => "amber",
        Signal::Red => "red",
        Signal::Unknown => "unknown",
    }
}

pub(crate) fn parse_sig(s: &str) -> Option<Signal> {
    match s {
        "green" => Some(Signal::Green),
        "amber" => Some(Signal::Amber),
        "red" => Some(Signal::Red),
        "unknown" => Some(Signal::Unknown),
        _ => None,
    }
}

/// Parse the leading numeric value of a display `raw` for trend plotting, e.g.
/// `"60%" → 60.0`, `"842ms" → 842.0`, `"¥2.30" → 2.30`, `"5/7" → 5.0`,
/// `"-3" → -3.0`. Scans for the first numeric run (optional sign, digits, one
/// dot) anywhere in the string, so a currency/symbol prefix doesn't block it.
/// Returns `None` for non-numeric values (`"清零"`), which the caller drops from
/// the series — the sparkline only ever plots real numbers.
pub(crate) fn parse_leading_f32(raw: &str) -> Option<f32> {
    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        // A number starts at a digit, a dot, or a sign immediately followed by
        // a digit/dot.
        let starts_here = c.is_ascii_digit()
            || c == b'.'
            || ((c == b'-' || c == b'+')
                && bytes
                    .get(i + 1)
                    .is_some_and(|n| n.is_ascii_digit() || *n == b'.'));
        if !starts_here {
            i += 1;
            continue;
        }
        let start = i;
        if c == b'-' || c == b'+' {
            i += 1;
        }
        let mut seen_dot = false;
        while i < bytes.len() {
            match bytes[i] {
                b'0'..=b'9' => i += 1,
                b'.' if !seen_dot => {
                    seen_dot = true;
                    i += 1;
                }
                _ => break,
            }
        }
        return raw[start..i].parse::<f32>().ok();
    }
    None
}

pub(crate) fn stage_kind_text(k: StageKind) -> &'static str {
    match k {
        StageKind::CompetitorInsight => "competitor_insight",
        StageKind::RequirementIntake => "requirement_intake",
        StageKind::NorthStar => "north_star",
        StageKind::Leading => "leading",
        StageKind::Lagging => "lagging",
        StageKind::PrototypeCreate => "prototype_create",
        StageKind::ProgressMgmt => "progress_mgmt",
    }
}

pub(crate) fn parse_stage_kind(s: &str) -> Option<StageKind> {
    StageKind::ALL
        .into_iter()
        .find(|k| stage_kind_text(*k) == s)
}

pub(crate) fn cadence_text(c: &Cadence) -> String {
    match c {
        Cadence::RealTime => "realtime".into(),
        Cadence::Daily => "daily".into(),
        Cadence::Weekly => "weekly".into(),
        Cadence::Cron(e) => format!("cron:{e}"),
    }
}

pub(crate) fn parse_cadence(s: &str) -> Cadence {
    match s {
        "realtime" => Cadence::RealTime,
        "daily" => Cadence::Daily,
        "weekly" => Cadence::Weekly,
        other => other
            .strip_prefix("cron:")
            .map(|e| Cadence::Cron(e.to_string()))
            .unwrap_or(Cadence::Weekly),
    }
}
