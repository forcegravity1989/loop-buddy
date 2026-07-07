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
use bw_core::model::{
    Cadence, ProjectPhase, Role, SessionStatus, Signal, SourceKind, StageKind, StagePhase,
};
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
    /// 对标竞品 / 机会缺口 — real wizard inputs (step 1 / 2).
    pub benchmark: String,
    pub opportunity: String,
    /// Cached derived signal (read-only; recompute is authoritative).
    pub signal: Option<Signal>,
    pub weekly_signal: Option<Signal>,
}

#[derive(Clone, Debug)]
pub struct MetricSignal {
    pub id: MetricId,
    pub name: String,
    pub role: MetricRole,
    pub def: String,
    pub value_raw: String,
    pub target_raw: String,
    pub last_target: String,
    pub driver: String,
    pub stage_kind: Option<StageKind>,
    /// Source of the latest observation (`None` = no observation yet).
    pub source: Option<SourceKind>,
    pub signal: Option<Signal>,
    pub hit: Option<bool>,
}

/// One materialized control point, as the operating view reads it.
#[derive(Clone, Debug)]
pub struct StageRow {
    pub kind: StageKind,
    pub phase: StagePhase,
    pub progress: u8,
    /// History of hand-set progress values (progress is *plan* data, not a
    /// signal — setting it by hand is legitimate; the history stays real).
    pub trend: Vec<f32>,
    pub schedule: Cadence,
    pub owns: String,
    pub accept: String,
    pub control: String,
    pub routine_signal: Option<Signal>,
}

/// One append-only observation, for trends (sparkline history) and the routine
/// feed. Real recorded values only — the UI never invents a series.
#[derive(Clone, Debug)]
pub struct ObservationRow {
    pub metric_id: MetricId,
    pub ts: OffsetDateTime,
    pub source: SourceKind,
    pub raw: String,
}

#[derive(Clone, Debug)]
pub struct StageSignal {
    pub kind: StageKind,
    pub routine: Option<Signal>,
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
    pub stage_kind: Option<StageKind>,
    pub status: SessionStatus,
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
    /// 对标竞品 + 机会缺口 (wizard step 1/2 real inputs).
    async fn set_brief(&self, id: ProjectId, benchmark: &str, opportunity: &str) -> Result<()>;

    async fn upsert_metric(&self, m: NewMetric) -> Result<()>;
    /// Week-plan edit (wizard step 7 / progress panel): update a metric's
    /// target + this week's driver, keeping the previous target as `last_target`.
    /// Touches no value and no signal — recompute re-derives against the new target.
    async fn update_week_plan(
        &self,
        metric: MetricId,
        new_target: &str,
        last_target: &str,
        driver: &str,
    ) -> Result<()>;
    /// Append-only — the sole birthplace of a value.
    async fn append_observation(
        &self,
        metric_id: MetricId,
        source: SourceKind,
        raw: &str,
        ts: OffsetDateTime,
    ) -> Result<()>;

    async fn materialize_stages(&self, stages: Vec<NewStage>) -> Result<()>;
    /// 进度管理 lever: hand-set a stage's progress (plan data, NOT a signal —
    /// signals stay derive-only). Appends the value to the stage's real
    /// progress-trend history.
    async fn set_stage_progress(
        &self,
        project_id: ProjectId,
        kind: StageKind,
        progress: u8,
    ) -> Result<()>;

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
    /// The seven materialized control points (empty while cold-starting).
    async fn list_stages(&self, project_id: ProjectId) -> Result<Vec<StageRow>>;
    /// All observations of a project's metrics, oldest first — the real series
    /// behind sparklines and the routine feed.
    async fn list_observations(&self, project_id: ProjectId) -> Result<Vec<ObservationRow>>;
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

pub(crate) fn session_status_text(s: SessionStatus) -> &'static str {
    match s {
        SessionStatus::Active => "active",
        SessionStatus::Archived => "archived",
        SessionStatus::Done => "done",
    }
}

pub(crate) fn parse_session_status(s: &str) -> SessionStatus {
    match s {
        "archived" => SessionStatus::Archived,
        "done" => SessionStatus::Done,
        _ => SessionStatus::Active,
    }
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
