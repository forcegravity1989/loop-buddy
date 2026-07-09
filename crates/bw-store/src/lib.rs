//! `bw-store` — local-first persistence behind a [`Store`] trait.
//!
//! Three encoded invariants (plan `§2.5` / `§5` + 体系重构 v2 `§07`):
//! 1. **Values are born only as observations.** [`Store::append_observation`] is
//!    append-only; there is no value setter elsewhere.
//! 2. **Signals are written only by derive.** [`Store::recompute_signals`] is the
//!    *sole* writer of every `signal` / `hit` column — the trait exposes no
//!    `set_signal`. It reads observations + targets, calls `bw_core::derive`, and
//!    writes the resulting cache.
//! 3. **Stage transitions are born only as handoffs.** [`Store::handoff_stage`]
//!    is append-only (audit log); `project.active_stage` is derived from the
//!    latest entry, never set independently.
//!
//! The trait is the seam for Tier E: swap [`SqliteStore`] for an IndexedDB /
//! remote adapter with no schema migration (`updated_at + rev` on every table).

#![forbid(unsafe_code)]

use async_trait::async_trait;
use bw_core::derive::AmberBand;
use bw_core::model::{
    AgentCard, AgentRef, Cadence, Connector, ConnectorStatus, CronStatus, CronTask, HubSource,
    KnowledgeSource, LibSource, LoopConfig, Maturity, ProjectCycle, ProjectPhase, Role,
    SessionStatus, Signal, SkillCard, SkillRef, SourceKind, StageKind, WorkflowKind, WorkflowSpec,
};
use bw_core::{
    AgentId, ConnectorId, CronTaskId, KnowledgeSourceId, MetricId, ProjectId, SessionId, SkillId,
    WorkflowId,
};
use time::OffsetDateTime;

mod sqlite;
pub use sqlite::SqliteStore;

pub mod seed;
pub use seed::seed_hub_if_empty;

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
    pub schedule: Cadence,
}

pub struct NewSession {
    pub id: SessionId,
    pub project_id: ProjectId,
    pub stage_kind: Option<StageKind>,
    pub kind: SessionKind,
    pub title: String,
    pub snippet: String,
}

/// Hub library (global — no `project_id`). `uses`/`runs` are omitted here:
/// they're usage-derived counters that start at 0, filled by a separate
/// write path (`record_workflow_use`), not part of creation.
pub struct NewWorkflowSpec {
    pub id: WorkflowId,
    pub name: String,
    pub kind: WorkflowKind,
    pub prompt: String,
    pub goal: String,
    pub stage_ref: Option<u8>,
    pub phases: Vec<String>,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub loop_config: LoopConfig,
}

pub struct NewSkill {
    pub id: SkillId,
    pub name: String,
    pub maturity: Maturity,
    pub desc: String,
    pub category: String,
    pub source: LibSource,
}

pub struct NewAgent {
    pub id: AgentId,
    pub name: String,
    pub role: String,
    pub maturity: Maturity,
    pub skills: Vec<String>,
    pub model: String,
}

pub struct NewCronTask {
    pub id: CronTaskId,
    pub name: String,
    pub target: String,
    pub schedule: Cadence,
    pub project_id: Option<ProjectId>,
}

pub struct NewConnector {
    pub id: ConnectorId,
    pub name: String,
    pub kind: String,
    pub scope: String,
}

pub struct NewKnowledgeSource {
    pub id: KnowledgeSourceId,
    pub name: String,
    pub kind: String,
    pub used_by: String,
}

// ───────────────────────────── read DTOs ─────────────────────────────

#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
    pub phase: ProjectPhase,
    pub cycle: ProjectCycle,
    pub active_stage: StageKind,
    pub north_star: String,
    pub ns_def: String,
    /// 对标竞品 / 机会缺口 — real creation-flow inputs.
    pub benchmark: String,
    pub opportunity: String,
    /// Real-executor target directory. Empty = unconfigured — the project
    /// only ever runs on `MockExecutor`, regardless of `allow_commands`.
    pub workspace_path: String,
    /// Whether the real executor may also run shell commands (Bash), not
    /// just edit files. Meaningless while `workspace_path` is empty.
    pub allow_commands: bool,
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

/// One materialized stage, as the operating view reads it.
#[derive(Clone, Debug)]
pub struct StageRow {
    pub kind: StageKind,
    pub progress: u8,
    /// History of hand-set progress values (progress is *plan* data, not a
    /// signal — setting it by hand is legitimate; the history stays real).
    pub trend: Vec<f32>,
    /// Handoff/DoD checklist state, indexed like `StageKind::dod_items()`.
    pub dod: Vec<bool>,
    pub schedule: Cadence,
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

/// One audited stage transition, oldest-first-consumable.
#[derive(Clone, Debug)]
pub struct HandoffRow {
    pub from_stage: StageKind,
    pub to_stage: StageKind,
    pub risky: bool,
    pub note: String,
    pub at: OffsetDateTime,
}

/// A handoff joined with its project's name — the real, cross-project audit
/// feed behind Activity Hub. Not a new table: `handoff` is already the
/// append-only birthplace of every stage transition (§ module doc invariant
/// 3); this just reads it globally instead of per-project.
#[derive(Clone, Debug)]
pub struct GlobalHandoffRow {
    pub project_id: ProjectId,
    pub project_name: String,
    pub from_stage: StageKind,
    pub to_stage: StageKind,
    pub risky: bool,
    pub note: String,
    pub at: OffsetDateTime,
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
    /// Delete a project and everything scoped to it (metrics + their
    /// observations, stages, sessions + their messages, weekly reviews,
    /// handoffs) — the CRUD-completeness counterpart to `create_project`, for
    /// after-the-fact editing/correction. Irreversible; the caller is
    /// responsible for any user-facing confirmation.
    async fn delete_project(&self, id: ProjectId) -> Result<()>;
    async fn set_project_phase(&self, id: ProjectId, phase: ProjectPhase) -> Result<()>;
    async fn set_project_cycle(&self, id: ProjectId, cycle: ProjectCycle) -> Result<()>;
    async fn set_north_star(&self, id: ProjectId, north_star: &str, ns_def: &str) -> Result<()>;
    /// 对标竞品 + 机会缺口/三月成功标准 (creation-flow real inputs).
    async fn set_brief(&self, id: ProjectId, benchmark: &str, opportunity: &str) -> Result<()>;
    /// Configure the real-executor target directory + whether it may also run
    /// shell commands. Empty `path` clears configuration (reverts to
    /// Mock-only). Does not touch any signal or observation.
    async fn set_workspace(&self, id: ProjectId, path: &str, allow_commands: bool) -> Result<()>;

    async fn upsert_metric(&self, m: NewMetric) -> Result<()>;
    /// Week-plan edit: update a metric's target + this week's driver, keeping
    /// the previous target as `last_target`. Touches no value and no signal —
    /// recompute re-derives against the new target.
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

    /// Materializes all five stages at creation, `dod` all-unchecked.
    async fn materialize_stages(&self, stages: Vec<NewStage>) -> Result<()>;
    /// Hand-set plan progress for one stage (plan data, not a signal — the
    /// derive chain is untouched). Appends the value to the stage's real
    /// progress-trend history.
    async fn set_stage_progress(
        &self,
        project_id: ProjectId,
        kind: StageKind,
        progress: u8,
    ) -> Result<()>;
    /// Flip one handoff/DoD checklist box for a stage.
    async fn toggle_dod(&self, project_id: ProjectId, kind: StageKind, index: usize) -> Result<()>;
    /// Append-only stage transition — the sole birthplace of `active_stage`.
    /// `to == from.next()` normally; the caller decides `risky` (DoD not fully
    /// checked) and supplies an audit `note`.
    async fn handoff_stage(
        &self,
        project_id: ProjectId,
        from: StageKind,
        to: StageKind,
        risky: bool,
        note: &str,
        at: OffsetDateTime,
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
    /// The five materialized stages (empty while cold-starting).
    async fn list_stages(&self, project_id: ProjectId) -> Result<Vec<StageRow>>;
    /// All observations of a project's metrics, oldest first — the real series
    /// behind sparklines and the routine feed.
    async fn list_observations(&self, project_id: ProjectId) -> Result<Vec<ObservationRow>>;
    /// Stage-transition audit log, newest first.
    async fn list_handoffs(&self, project_id: ProjectId) -> Result<Vec<HandoffRow>>;
    /// Cross-project stage-transition audit log, newest first, capped at
    /// `limit` — the real feed behind Activity Hub.
    async fn list_recent_handoffs(&self, limit: u32) -> Result<Vec<GlobalHandoffRow>>;
    async fn list_sessions(&self, project_id: ProjectId) -> Result<Vec<SessionRow>>;
    async fn session_messages(&self, session_id: SessionId) -> Result<Vec<MessageRow>>;

    // ── hub library (global — no active-project gate) ──
    async fn create_workflow_spec(&self, w: NewWorkflowSpec) -> Result<()>;
    async fn list_workflow_specs(&self) -> Result<Vec<WorkflowSpec>>;
    async fn get_workflow_spec(&self, id: WorkflowId) -> Result<Option<WorkflowSpec>>;
    /// Promote a `Dynamic` spec to a new `Static` hub entry: mints a fresh row
    /// (`maturity: Fresh, version: 1, uses: 0`), copying prompt/goal/phases/
    /// agents/skills/stage_ref/loop_config from `from`. The session that
    /// inspired it is untouched — this is purely additive, never a mutation
    /// of run history.
    async fn promote_workflow(
        &self,
        new_id: WorkflowId,
        from: &WorkflowSpec,
        source: HubSource,
    ) -> Result<()>;
    /// Bump a hub spec's `uses` counter by 1 — called when it's run via
    /// `RunHubWorkflow`.
    async fn record_workflow_use(&self, id: WorkflowId) -> Result<()>;

    async fn create_skill(&self, s: NewSkill) -> Result<()>;
    async fn list_skills(&self) -> Result<Vec<SkillCard>>;
    async fn get_skill(&self, id: SkillId) -> Result<Option<SkillCard>>;

    async fn create_agent(&self, a: NewAgent) -> Result<()>;
    async fn list_agents(&self) -> Result<Vec<AgentCard>>;
    async fn get_agent(&self, id: AgentId) -> Result<Option<AgentCard>>;

    async fn create_cron_task(&self, c: NewCronTask) -> Result<()>;
    async fn list_cron_tasks(&self) -> Result<Vec<CronTask>>;

    async fn create_connector(&self, c: NewConnector) -> Result<()>;
    async fn list_connectors(&self) -> Result<Vec<Connector>>;

    async fn create_knowledge_source(&self, k: NewKnowledgeSource) -> Result<()>;
    async fn list_knowledge_sources(&self) -> Result<Vec<KnowledgeSource>>;
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
        StageKind::Prototype => "prototype",
        StageKind::Build => "build",
        StageKind::Optimize => "optimize",
        StageKind::Growth => "growth",
        StageKind::Ops => "ops",
    }
}

pub(crate) fn parse_stage_kind(s: &str) -> Option<StageKind> {
    StageKind::ALL
        .into_iter()
        .find(|k| stage_kind_text(*k) == s)
}

pub(crate) fn cycle_text(c: ProjectCycle) -> &'static str {
    match c {
        ProjectCycle::Explore => "explore",
        ProjectCycle::Expand => "expand",
        ProjectCycle::Mature => "mature",
    }
}

pub(crate) fn parse_cycle(s: &str) -> ProjectCycle {
    match s {
        "expand" => ProjectCycle::Expand,
        "mature" => ProjectCycle::Mature,
        _ => ProjectCycle::Explore,
    }
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

pub(crate) fn maturity_text(m: Maturity) -> &'static str {
    match m {
        Maturity::Mature => "mature",
        Maturity::Polishing => "polishing",
        Maturity::Fresh => "fresh",
    }
}

pub(crate) fn parse_maturity(s: &str) -> Maturity {
    match s {
        "mature" => Maturity::Mature,
        "polishing" => Maturity::Polishing,
        _ => Maturity::Fresh,
    }
}

pub(crate) fn lib_source_text(s: LibSource) -> &'static str {
    match s {
        LibSource::Official => "official",
        LibSource::SelfBuilt => "self_built",
    }
}

pub(crate) fn parse_lib_source(s: &str) -> LibSource {
    match s {
        "official" => LibSource::Official,
        _ => LibSource::SelfBuilt,
    }
}

pub(crate) fn parse_cron_status(s: &str) -> CronStatus {
    match s {
        "running" => CronStatus::Running,
        "failed" => CronStatus::Failed,
        "paused" => CronStatus::Paused,
        _ => CronStatus::Normal,
    }
}

pub(crate) fn parse_connector_status(s: &str) -> ConnectorStatus {
    match s {
        "connected" => ConnectorStatus::Connected,
        "syncing" => ConnectorStatus::Syncing,
        "error" => ConnectorStatus::Error,
        _ => ConnectorStatus::Disconnected,
    }
}
