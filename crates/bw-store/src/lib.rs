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
    AgentCard, AgentRef, Author, Cadence, Connector, ConnectorStatus, CronEffectiveness, CronMode,
    CronStatus, CronTask, HubSource, Issue, IssuePriority, IssueStatus, KnowledgeSource,
    LoopConfig, Maturity, MaturityPeriod, PhaseMeta, Readiness, RunStatus, RunTrigger,
    SessionStatus, Signal, SkillCard, SkillRef, SourceKind, StageKind, UsageRank, WorkflowKind,
    WorkflowRun, WorkflowRunAnalytics, WorkflowSpec, WorkflowVersion,
};
use bw_core::{
    AgentId, ConnectorId, CronTaskId, IssueId, KnowledgeSourceId, MetricId, ProjectId, SessionId,
    SkillFileId, SkillId, WorkflowId, WorkflowRunId,
};
use time::OffsetDateTime;

mod sqlite;
pub use sqlite::SqliteStore;

pub mod seed;
pub use seed::{
    seed_hub_if_empty, seed_stage_entities_if_missing, seed_standard_issue_skills_if_missing,
};

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

/// Where a metric *definition* (not its value — that's [`bw_core::model::SourceKind`])
/// came from. `Manual` is the byte-for-byte pre-C6 default: a row created by
/// the UI's `UpsertManualMetric`. `File` marks a row whose definition (name/
/// def/target/collect plan) was synced from the project's `.bw/metrics.toml`
/// source of truth (plan/13 D5) — purely a provenance label, never read by
/// the derive chain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MetricOrigin {
    Manual,
    File,
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
    /// C16(plan/14 规范条 4): 仓平台选择器的选中值 —— 今天恒 `"github"`
    /// (唯一可选项)。落进 `project.provider`,不派生也不重算。
    pub provider: String,
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

/// C6 (plan/13 D5+D6): one metric's definition as read from
/// `.bw/metrics.toml` — no id (the file has no id concept; identity for the
/// upsert is `(project, role, name)`), no operational week-plan fields
/// (`last_target`/`driver`/`pos`/`amber` aren't in the file's vocabulary and
/// are left alone on an existing row, defaulted on a freshly inserted one).
pub struct MetricDefSync {
    pub name: String,
    pub def: String,
    pub target_raw: String,
    /// `CollectKind::as_str()` text — `bw-store` doesn't depend on
    /// `bw-engine`, so this arrives pre-stringified (already validated
    /// against the fixed vocabulary at parse time).
    pub collect_kind: String,
    pub collect_query: String,
}

/// C6: the whole `.bw/metrics.toml` file, shaped for one atomic sync call.
pub struct MetricsFileSync {
    pub project_id: ProjectId,
    pub north_star_name: String,
    pub north_star_def: String,
    pub north_star_collect_kind: String,
    pub north_star_collect_query: String,
    pub lagging: Vec<MetricDefSync>,
    pub leading: Vec<MetricDefSync>,
}

/// C6: the honest receipt of one sync — real counts, not "however many were
/// in the file" (a metric skipped for some future validation reason would
/// make those differ).
#[derive(Clone, Copy, Debug, Default)]
pub struct MetricsFileSyncSummary {
    pub lagging_synced: u32,
    pub leading_synced: u32,
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

/// Bundle of the fields known when a run *starts* (iter 1/3). Grouping them
/// into a struct keeps `record_workflow_run_start` under the argument-count
/// lint and mirrors the `New*` pattern used for every other write. The run's
/// id is *minted inside* the store (not passed in) so the caller can't lose
/// the handle that settles the row later.
pub struct NewWorkflowRun<'a> {
    pub workflow_id: WorkflowId,
    pub workflow_name: &'a str,
    pub project_id: Option<ProjectId>,
    pub session_id: Option<SessionId>,
    pub trigger: RunTrigger,
    pub started_at: i64,
    /// The cron task that fired this run, if any (iter 4). `None` for manual
    /// runs; `Some` only on the scheduler's auto-fire path, so a per-task
    /// effectiveness aggregate can attribute outcomes correctly.
    pub cron_task_id: Option<CronTaskId>,
    /// Snapshot of the spec's shape at run time (iter 3) — what this run is
    /// actually executing, frozen before the engine runs. `''` is valid
    /// (no snapshot) and stays backward-compatible with iter 1 rows.
    pub params_json: &'a str,
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
    pub phases: Vec<PhaseMeta>,
    /// Per-phase real instructions (playbook), index-aligned with `phases`.
    /// Empty = pre-playbook workflow (every phase shares `prompt`).
    pub phase_prompts: Vec<String>,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub loop_config: LoopConfig,
    /// 践行最小切片(2026-07-20):`None` = hub library(全局,现有行为不变);
    /// `Some` = 项目自有,只这一条项目自己看得见——查询收窄(P2 全量)不在本次范围,
    /// 这里只落列 + 落值,读回走 sqlite 直查。
    pub project_id: Option<ProjectId>,
    /// T16 (plan/12 §10 v1.1#3): the workflow's main MD document — see
    /// `bw_core::model::WorkflowSpec::content`'s doc comment. `''` for every
    /// caller today (no create-form field yet); carried as a real DTO field
    /// so a future content-authoring path has somewhere to write it without
    /// another schema change.
    pub content: String,
}

/// The editable content of an existing **Static** hub workflow — the "优化"
/// action on a spec that already exists, distinct from `NewWorkflowSpec`
/// (creation) and `promote_workflow` (mint a new row from a session). Omits
/// `name`/`stage_ref`/`loop_config`: this is a content revision (prompt,
/// goal, method, crew), not a re-classification.
pub struct WorkflowEdit {
    pub prompt: String,
    pub goal: String,
    pub phases: Vec<PhaseMeta>,
    /// Per-phase instructions, index-aligned with `phases` (may be empty —
    /// an edit that drops back to a single shared `prompt` is legal).
    pub phase_prompts: Vec<String>,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    /// Caller's reason for this "优化" — recorded on the version snapshot
    /// (iter 5) so the evolution history says *why* each change happened,
    /// not just that it did. `''` is valid (no reason given).
    pub note: String,
}

pub struct NewSkill {
    pub id: SkillId,
    pub name: String,
    pub maturity: Maturity,
    pub desc: String,
    pub category: String,
    /// T7 (plan/12 §0/§2): which stage role this skill belongs to; `None` =
    /// general/cross-stage. See `bw_core::model::SkillCard::stage_ref`'s doc
    /// comment for the `Option<StageKind>`-vs-`WorkflowSpec`'s-`Option<u8>`
    /// alignment call.
    pub stage_ref: Option<StageKind>,
    pub source: HubSource,
    /// Executable body (may be empty for a catalog reference entry). For a
    /// skill minted by `ImportSkillPackage`, this is SKILL.md's own body —
    /// every *other* file in the imported folder lands in `skill_file`
    /// instead (see [`NewSkillFile`]).
    pub content: String,
    /// 践行最小切片(2026-07-20):`None` = hub library(全局);`Some` = 项目自有。
    /// 见 [`NewWorkflowSpec::project_id`]。
    pub project_id: Option<ProjectId>,
}

/// One real support file belonging to an imported skill folder (T2, plan/12
/// §2) — copy-on-import: `content` is the file's real bytes at import time,
/// completely decoupled from the source path afterward. `rel_path` is the
/// real path relative to the skill folder's root (`"references/mocking.md"`,
/// `"agents/openai.yaml"`, …) — no predetermined category/subfolder scheme,
/// as-observed on disk.
pub struct NewSkillFile {
    pub rel_path: String,
    pub content: String,
}

/// Editable content fields for an existing skill — `maturity`/`source`/
/// `uses` are lifecycle data untouched by an edit, same rule
/// `WorkflowEdit`/`update_workflow_spec` already established. `source` is
/// "untouched" with one T11 exception below.
pub struct SkillEdit {
    pub name: String,
    pub desc: String,
    pub category: String,
    pub content: String,
    /// T11 (2026-07-23, plan/12 §7): the caller (`bw-app`'s `UpdateSkill`
    /// handler) has already compared this edit's substantive fields
    /// (`content`/`desc`/`category`) against the row's pre-edit values and
    /// found this row `Official` — `true` means "flip `source` to
    /// `self_built` in this same UPDATE". `official_library` is
    /// deliberately left untouched either way (留痕 — `parse_hub_source`
    /// already ignores that column whenever the tag isn't `official`, so the
    /// domain-level `HubSource` read-back is honestly `SelfBuilt` regardless;
    /// the raw column survives for `SkillCard::adapted_from` / re-import
    /// dedup to read).
    pub flip_to_self_built: bool,
}

pub struct NewAgent {
    pub id: AgentId,
    pub name: String,
    pub role: String,
    /// T7 (plan/12 §0/§3): same classification dimension as
    /// `NewSkill::stage_ref` — `None` = general/cross-stage.
    pub stage_ref: Option<StageKind>,
    pub maturity: Maturity,
    pub skills: Vec<String>,
    pub model: String,
    /// Standing instructions (may be empty for a catalog reference entry).
    pub instructions: String,
    /// T5 (2026-07-23, plan/12 §3): AllowedTools — real AGENT.md `tools`
    /// frontmatter for an imported row; `[]` for a hand-authored
    /// `CreateAgent` row or one of the five built-in stage-role agents (no
    /// restriction declared).
    pub tools: Vec<String>,
    /// T5: which Agent CLI executes this agent. First version always
    /// `"claude-code"` — the only one with a real executor.
    pub agent_cli: String,
    /// T5 (plan/12 §6/§8): provenance, the same [`HubSource`] Skill/Workflow
    /// already carry.
    pub source: HubSource,
    /// 践行最小切片(2026-07-20):`None` = hub library(全局);`Some` = 项目自有。
    /// 见 [`NewWorkflowSpec::project_id`]。
    pub project_id: Option<ProjectId>,
}

/// Editable content fields for an existing agent — `maturity`/`runs`/
/// `win_rate` are lifecycle data untouched by an edit.
pub struct AgentEdit {
    pub name: String,
    pub role: String,
    pub skills: Vec<String>,
    pub model: String,
    pub instructions: String,
    /// T11 (2026-07-23, plan/12 §7): same flip signal as
    /// `SkillEdit::flip_to_self_built` — see its doc comment. The caller has
    /// already compared `instructions`/`role`/`model` against the row's
    /// pre-edit values and found it `Official`.
    pub flip_to_self_built: bool,
}

pub struct NewCronTask {
    pub id: CronTaskId,
    pub name: String,
    /// T10: for `RunSkill`/`RunPrompt`, the caller must set this to `mode`'s
    /// own payload (the skill id as text / the raw prompt text respectively)
    /// — `create_cron_task` just stores whatever is here; it does not
    /// re-derive it from `mode`. See `bw_core::model::CronMode`'s doc.
    pub target: String,
    pub schedule: Cadence,
    pub project_id: Option<ProjectId>,
    /// A1: what this task does when due (default `RunWorkflow`).
    pub mode: CronMode,
    /// A1: stage for a `CreateIssue` task (`None` for `RunWorkflow`).
    pub issue_stage: Option<StageKind>,
    /// A1: agent NAME to assign the minted Issue to (`None` = unassigned).
    pub issue_assignee: Option<String>,
}

/// Write DTO for creating an [`Issue`]. `status` defaults to `Backlog`;
/// `number` is auto-assigned per project (1, 2, 3, …) inside `create_issue`.
pub struct NewIssue {
    pub id: IssueId,
    pub project_id: ProjectId,
    pub stage: StageKind,
    pub title: String,
    pub desc: String,
    pub priority: IssuePriority,
    /// C8 · plan/13 D8: stable Skill-Hub slug this Issue is wired to (`""` =
    /// none — every hand-created / autopilot Issue). Only the standard-Issue
    /// trio's seeder sets a real value.
    pub standard_skill: String,
}

pub struct NewConnector {
    pub id: ConnectorId,
    pub name: String,
    pub kind: String,
    pub scope: String,
    /// Project this connector feeds (a `git-repo` connector is always bound).
    pub project_id: Option<ProjectId>,
    /// Kind-specific real config (`git-repo`: workspace path; `claude-cli`:
    /// binary override, empty = PATH).
    pub config: String,
}

/// One real workspace-file version to register (完整形态 · 产物). Identity is
/// `(project_id, path, git_commit)` — the store ignores duplicates, so a
/// caller can re-scan freely and only genuinely new versions land.
pub struct NewArtifact {
    pub id: bw_core::ArtifactId,
    pub project_id: ProjectId,
    pub workflow_run_id: Option<WorkflowRunId>,
    /// A2: bind this version to an Issue (set on the Done-edge scan; `None`
    /// for run-settle and manual-collect registrations).
    pub issue_id: Option<IssueId>,
    pub stage_kind: Option<StageKind>,
    pub path: String,
    pub kind: bw_core::model::ArtifactKind,
    pub bytes: u64,
    pub git_commit: String,
    pub registered_at: i64,
}

pub struct NewKnowledgeSource {
    pub id: KnowledgeSourceId,
    pub name: String,
    pub kind: String,
    pub used_by: String,
}

// ───────────────────────────── read DTOs ─────────────────────────────

/// One persisted `skill_file` row, as read back — the real support-file
/// contents an imported skill folder carried alongside its SKILL.md (T2,
/// plan/12 §2).
#[derive(Clone, Debug)]
pub struct SkillFileRow {
    pub id: SkillFileId,
    pub skill_id: SkillId,
    pub rel_path: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
    pub phase: Readiness,
    pub cycle: MaturityPeriod,
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
    /// "owner/repo" — empty = not attached to GitHub (local-only workspace,
    /// or GitHub attach failed and soft-degraded). Set once, at creation.
    pub github_remote: String,
    /// C16(plan/14 规范条 4): 仓平台选择器的选中值(`"github"` 今天唯一可能
    /// 的取值)。老库开出来的存量行经 `add_column_if_missing` 默认 `'github'`
    /// ——和"这仓当时就是接 GitHub 建的"这个真实状态一致(pre-C16 没有别的
    /// 平台可选)。
    pub provider: String,
    /// C6: the north star's collection plan, synced from `.bw/metrics.toml`'s
    /// `north_star.collect` (empty = never synced from a source-of-truth
    /// file yet — the creation-flow-typed name/def has no collect plan).
    pub north_star_collect_kind: String,
    pub north_star_collect_query: String,
    /// Cached derived signal (read-only; recompute is authoritative).
    pub signal: Option<Signal>,
    pub weekly_signal: Option<Signal>,
    /// Unix seconds — the project's birth moment (P5: 90-day countdown).
    pub created_at: i64,
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
    /// C6: this definition's collection plan (empty = never synced from
    /// `.bw/metrics.toml`).
    pub collect_kind: String,
    pub collect_query: String,
    /// C6: `Manual` (界面手建, the byte-for-byte pre-C6 default) or `File`
    /// (synced from the project's metrics source-of-truth file).
    pub origin: MetricOrigin,
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
    pub role: Author,
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
    async fn set_project_phase(&self, id: ProjectId, phase: Readiness) -> Result<()>;
    async fn set_project_cycle(&self, id: ProjectId, cycle: MaturityPeriod) -> Result<()>;
    async fn set_north_star(&self, id: ProjectId, north_star: &str, ns_def: &str) -> Result<()>;
    /// 对标竞品 + 机会缺口/三月成功标准 (creation-flow real inputs).
    async fn set_brief(&self, id: ProjectId, benchmark: &str, opportunity: &str) -> Result<()>;
    /// Configure the real-executor target directory + whether it may also run
    /// shell commands. Empty `path` clears configuration (reverts to
    /// Mock-only). Does not touch any signal or observation.
    async fn set_workspace(&self, id: ProjectId, path: &str, allow_commands: bool) -> Result<()>;
    /// Record the GitHub remote a project's workspace was created from or
    /// adopted from ("owner/repo"). Called once, right after a successful
    /// `bw_engine::github::create_repo`/`clone_repo` — never touched again.
    async fn set_github_remote(&self, id: ProjectId, github_remote: &str) -> Result<()>;

    async fn upsert_metric(&self, m: NewMetric) -> Result<()>;
    /// C6 (plan/13 D5+D6): atomically sync `.bw/metrics.toml`'s definitions
    /// into the cache. North star name/def go through the exact same UPDATE
    /// `set_north_star` performs; its collect plan lands in the two
    /// dedicated `project` columns. Each lagging/leading metric is upserted
    /// by **(project, role, name)** — the file has no id concept, so name is
    /// the join key; re-syncing an unchanged file inserts zero new rows. A
    /// synced row (new or pre-existing) is stamped `origin = File`; if it
    /// collides with a UI-typed row (same project/role/name), the file wins
    /// the definition (def/target/collect) but keeps that row's identity, so
    /// its observation history under `metric_id` is untouched. Never touches
    /// `observation` and never calls `recompute_signals` — this is
    /// definitions-only (collection execution is a later ticket, C7). All
    /// writes run in one transaction: the caller is expected to have already
    /// parsed the whole file successfully (parse-all-or-write-nothing lives
    /// one layer up, in `bw-engine::metrics_file::read`), and this method
    /// keeps its own partial-failure window closed too.
    async fn sync_metrics_file(&self, sync: MetricsFileSync) -> Result<MetricsFileSyncSummary>;
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
    /// 该指标最近一次观测的 unix 秒(无观测 = None)。采集器 window-guard
    /// 用:值平台期也要按窗口落新点,否则「今天真实测过没变」会被过期降级
    /// 误判成「久无数据」(code-review Standards #5)。
    async fn latest_observation_ts(&self, metric_id: MetricId) -> Result<Option<i64>>;

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
    async fn append_message(&self, session_id: SessionId, role: Author, text: &str) -> Result<()>;

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
    /// T16.5 (GH#54): raw `phases`/`phase_prompts` column overwrite for one
    /// `workflow_spec` row — deliberately bypasses `update_workflow_spec`'s
    /// "optimize" path (no version bump, no `workflow_version` snapshot
    /// written, `kind_json`/`name`/`prompt`/`goal`/`agents_json`/
    /// `skills_json` untouched). This is the one-shot legacy-migration's own
    /// narrow tool: refresh a built-in stage template's phase metadata from
    /// the current playbook without disturbing its `uses`/`version`/other
    /// track-record columns. Business judgement (which row, which values)
    /// lives in `bw-app`'s `legacy_migration` module, never here — this is a
    /// dumb column write, same "store 无业务判断" rule every other `Store`
    /// method already follows.
    async fn refresh_workflow_template_phases(
        &self,
        id: WorkflowId,
        phases: Vec<PhaseMeta>,
        phase_prompts: Vec<String>,
    ) -> Result<()>;

    // ── workflow_run: append-only execution telemetry (iter 1) ──────────────
    /// Insert a fresh run row at `status = Running`, returning the minted id
    /// the caller passes to [`Store::settle_workflow_run`] when the engine
    /// returns. The run's start is the *only* thing recorded here — outcome
    /// is settled separately so a crash mid-run still leaves an honest
    /// "started, never settled" row rather than a fabricated success.
    async fn record_workflow_run_start(&self, run: NewWorkflowRun<'_>) -> Result<WorkflowRunId>;
    /// A3: bind a run to the Issue it executes (RunIssue). Separate from run
    /// creation so `NewWorkflowRun` stays stable; NULL until a RunIssue fire
    /// sets it. Feeds `list_runs_for_issue` ("which runs did this issue
    /// produce?").
    async fn set_run_issue(&self, run_id: WorkflowRunId, issue_id: IssueId) -> Result<()>;
    /// P4: record the workspace HEAD pair (run start / settle) captured by the
    /// app around a real-workspace run — the recorded fact behind an Issue
    /// detail's "这次运行改了什么". Mock runs never call this (both stay NULL).
    async fn set_run_heads(
        &self,
        run_id: WorkflowRunId,
        head_before: Option<String>,
        head_after: Option<String>,
    ) -> Result<()>;
    /// Settle a run's terminal state exactly once: `status`, real
    /// `finished_at`/`duration_ms`, `phases_completed`, and `error`. No-op-safe
    /// if the row already settled (idempotent re-runs of the dogfood).
    async fn settle_workflow_run(
        &self,
        id: WorkflowRunId,
        status: RunStatus,
        finished_at: i64,
        duration_ms: i64,
        phases_completed: u32,
        error: &str,
    ) -> Result<()>;
    /// Recorded runs for one workflow, newest first — the series optimization
    /// analytics (iter 2) aggregates over.
    async fn list_workflow_runs(&self, workflow_id: WorkflowId) -> Result<Vec<WorkflowRun>>;
    /// All recorded runs across every workflow, newest first — for a global
    /// "what actually ran" feed / cross-workflow analytics.
    async fn list_all_workflow_runs(&self, limit: u32) -> Result<Vec<WorkflowRun>>;
    /// Aggregate analytics for one workflow over its run history (iter 2).
    /// Returns a zeroed-name row with `total_runs = 0` if the workflow has
    /// never run — never an error, so a caller can show "未运行" honestly.
    async fn workflow_analytics(&self, workflow_id: WorkflowId) -> Result<WorkflowRunAnalytics>;
    /// Effectiveness of one cron schedule over its auto-fired runs (iter 4).
    /// Manual runs of the same workflow are excluded — this is purely the
    /// schedule's track record. `fires = 0` (never fired) is not an error.
    async fn cron_effectiveness(&self, cron_task_id: CronTaskId) -> Result<CronEffectiveness>;
    /// Revise an existing **Static** spec's authored content ("优化" a hub
    /// workflow) — bumps `version`; `uses`/`maturity`/`source`/`scope`/
    /// `trigger` are preserved untouched from the row being edited. Errors
    /// if `id` resolves to a `Dynamic` spec (nothing durable to edit) or to
    /// no row at all.
    async fn update_workflow_spec(&self, id: WorkflowId, edit: WorkflowEdit) -> Result<()>;
    /// The frozen content-history of a Static workflow (iter 5), newest
    /// version first — every prior prompt/goal/phases/agents/skills, each
    /// with the reason it was replaced. Empty for a spec never updated.
    async fn list_workflow_versions(&self, workflow_id: WorkflowId)
        -> Result<Vec<WorkflowVersion>>;

    /// Global usage ranking of every hub workflow by real run history
    /// (iter 6) — hottest (most-run) first, coldest (never-run) last.
    async fn hub_usage_ranking(&self) -> Result<Vec<UsageRank>>;

    async fn create_skill(&self, s: NewSkill) -> Result<()>;
    async fn list_skills(&self) -> Result<Vec<SkillCard>>;
    async fn get_skill(&self, id: SkillId) -> Result<Option<SkillCard>>;
    async fn update_skill(&self, id: SkillId, edit: SkillEdit) -> Result<()>;
    /// Credit one real run to every skill row named `name` (`uses += 1`).
    /// Returns how many rows matched — `0` (an unregistered ad-hoc ref) is
    /// honest data, not an error.
    async fn record_skill_use_by_name(&self, name: &str) -> Result<u32>;
    /// Distill a new skill from a completed, assigned Issue — the "every
    /// solution compounds into a reusable skill" link. The issue must exist,
    /// be `Done`, and have a real assignee; the new skill is `SelfBuilt` /
    /// `Polishing` / `uses = 0`, carrying `distilled_from_issue` +
    /// `origin_agent` from the source issue. Additive: each call mints a new
    /// skill row (distilling the same issue twice produces two skills, not an
    /// error).
    async fn distill_skill_from_issue(&self, skill: NewSkill, from_issue: IssueId) -> Result<()>;
    /// Copy-on-import a real skill folder (T2, plan/12 §2): inserts the
    /// `skill` row plus every `files` entry as a `skill_file` row, in one
    /// transaction — either the whole package lands or none of it does.
    /// `skill.content` must already be SKILL.md's own body; `files` is
    /// everything else found in the folder (recursively, real relative
    /// paths). Additive, like `distill_skill_from_issue`: re-importing the
    /// same folder mints another skill row rather than upserting (dedup is
    /// `ImportSkillLibrary`'s concern, T3).
    async fn import_skill_package(&self, skill: NewSkill, files: Vec<NewSkillFile>) -> Result<()>;
    /// Every real support file belonging to one skill, insertion order
    /// (oldest first) — the file-tree source for a Skill detail view (T4).
    async fn list_skill_files(&self, skill_id: SkillId) -> Result<Vec<SkillFileRow>>;
    /// T7 (plan/12 §0/§2): narrow backfill setter — classifies an *existing*
    /// row (not a content edit, so deliberately separate from `SkillEdit`,
    /// same reasoning `record_skill_use_by_name` already established for
    /// single-column, non-content updates). Used by
    /// `seed_stage_entities_if_missing` to backfill `stage_ref` on the five
    /// built-in stage skills when they were seeded by an older binary,
    /// before this column carried real values.
    async fn set_skill_stage_ref(&self, id: SkillId, stage_ref: Option<StageKind>) -> Result<()>;
    /// T14 (2026-07-24, plan/12 §10 v1.1): delete one skill row plus its
    /// `skill_file` children (a legacy shell has none, but a real imported
    /// package might in general — this stays correct either way), in one
    /// transaction. The *decision* of which rows are safe to delete is
    /// bw-app's business judgement (this repo's "store 无业务判断" rule);
    /// this is purely the mechanical delete once that decision is made.
    async fn delete_skill(&self, id: SkillId) -> Result<()>;

    async fn create_agent(&self, a: NewAgent) -> Result<()>;
    async fn list_agents(&self) -> Result<Vec<AgentCard>>;
    async fn get_agent(&self, id: AgentId) -> Result<Option<AgentCard>>;
    async fn update_agent(&self, id: AgentId, edit: AgentEdit) -> Result<()>;
    /// T7: same backfill role as `set_skill_stage_ref`, for the five
    /// built-in stage-role agents.
    async fn set_agent_stage_ref(&self, id: AgentId, stage_ref: Option<StageKind>) -> Result<()>;
    /// Credit one settled run to every agent row named `name`: `runs += 1`,
    /// `wins += ok as int`, `win_rate` recomputed from the real counters.
    /// Returns how many rows matched (0 = unregistered ref, honest no-op).
    async fn record_agent_run_by_name(&self, name: &str, ok: bool) -> Result<u32>;
    /// T14: delete one agent row. No table carries a real FK onto `agent(id)`
    /// (`issue.assignee` is a plain, unconstrained id string) so this is a
    /// single-table delete; same "mechanics only, decision lives in bw-app"
    /// split as `delete_skill`.
    async fn delete_agent(&self, id: AgentId) -> Result<()>;

    async fn create_cron_task(&self, c: NewCronTask) -> Result<()>;
    async fn list_cron_tasks(&self) -> Result<Vec<CronTask>>;
    /// Pure status flip — pause/resume, the "人工介入" action on a cron task.
    /// Never touches `last_run`: nothing actually ran.
    async fn set_cron_status(&self, id: CronTaskId, status: CronStatus) -> Result<()>;
    /// Record that a task's target really ran just now — either a manual
    /// "▶ 立即执行" or a real auto-fire from `App::tick_scheduler` — with the
    /// real outcome `status`, a real caller-formatted display timestamp
    /// (`last_run`), and (set here, server-side) the real unix-seconds clock
    /// (`last_run_at`) `cron_due` compares future ticks against.
    async fn record_cron_run(
        &self,
        id: CronTaskId,
        status: CronStatus,
        last_run: String,
    ) -> Result<()>;

    async fn create_connector(&self, c: NewConnector) -> Result<()>;
    async fn list_connectors(&self) -> Result<Vec<Connector>>;
    /// Record the outcome of a *real* sync probe: the new status + a real
    /// display timestamp. The only writer of `connector.status` after
    /// creation — a connector's health is probe-derived, never hand-flipped.
    async fn set_connector_sync(
        &self,
        id: ConnectorId,
        status: ConnectorStatus,
        last_sync: &str,
    ) -> Result<()>;

    async fn create_knowledge_source(&self, k: NewKnowledgeSource) -> Result<()>;
    async fn list_knowledge_sources(&self) -> Result<Vec<KnowledgeSource>>;

    // ── artifact: append-only real-file registry (完整形态 · 产物) ──
    /// Register a batch of scanned workspace files. Duplicate identities
    /// (`project × path × git_commit`) are ignored; returns how many rows
    /// were *genuinely new* — the honest "this run produced N new artifact
    /// versions" number.
    async fn register_artifacts(&self, items: Vec<NewArtifact>) -> Result<u32>;
    /// All registered artifact versions for a project, newest first.
    async fn list_artifacts(&self, project_id: ProjectId) -> Result<Vec<bw_core::model::Artifact>>;
    // ── issue: assignable, stage-scoped work units ──
    async fn create_issue(&self, i: NewIssue) -> Result<()>;
    async fn list_issues(
        &self,
        project_id: ProjectId,
        stage: Option<StageKind>,
        status: Option<IssueStatus>,
    ) -> Result<Vec<Issue>>;
    async fn get_issue(&self, id: IssueId) -> Result<Option<Issue>>;
    async fn transition_issue(&self, id: IssueId, status: IssueStatus) -> Result<()>;
    async fn assign_issue(&self, id: IssueId, assignee: Option<AgentId>) -> Result<()>;
    /// A2: the runs bound to an Issue (fired by RunIssue), newest first — the
    /// "which runs did this issue produce?" half of an Issue's detail page.
    async fn list_runs_for_issue(&self, issue_id: IssueId) -> Result<Vec<WorkflowRun>>;
    /// A2: the artifact versions whose Done-edge scan registered them against
    /// this Issue — the "what did this issue produce?" half. Empty until an
    /// issue is transitioned Done (or a RunIssue run registers artifacts).
    async fn list_artifacts_for_issue(
        &self,
        issue_id: IssueId,
    ) -> Result<Vec<bw_core::model::Artifact>>;
    /// Stamp the FIRST settle time (COALESCE — later calls keep the original).
    /// The app's Done-edge accounting fires iff this was previously NULL.
    async fn mark_issue_settled(&self, id: IssueId, at: i64) -> Result<()>;
    /// C4 · issue 身份映射: record the GitHub issue number `gh issue create`
    /// minted for this Issue. The App layer calls this only after a real
    /// success — a failed/skipped mapping simply never calls it, leaving the
    /// honest `0` default in place.
    async fn set_issue_github_number(&self, id: IssueId, github_number: u32) -> Result<()>;
    /// C5 · PR 验收环: record the pull-request number a run's `open_pr` opened
    /// for this Issue. The App layer calls this only after a real `gh pr
    /// create` success — a failed/skipped PR simply never calls it, leaving
    /// the honest `0` default (提 PR 失败不炸 run). Never a fabricated number.
    async fn set_issue_pr_number(&self, id: IssueId, pr_number: u32) -> Result<()>;
    /// A5-F: the only way an issue reaches `Blocked` — sets status and reason
    /// together in one write. Legality (which source states may block) and
    /// the non-empty-reason rule are the App layer's job; the store just
    /// persists what it's told.
    async fn block_issue(&self, id: IssueId, reason: &str) -> Result<()>;
    /// A5-H: count of non-terminal (`!is_terminal()`) issues in a project —
    /// the project wall's "open work" badge. Same predicate as the A4 handoff
    /// risky-guard, so the two numbers never disagree.
    async fn count_open_issues(&self, project_id: ProjectId) -> Result<i64>;

    // ── app_meta: tiny key/value table for one-shot app-level markers ──
    /// T14 (2026-07-24, plan/12 §10 v1.1): read a marker's value, `None` if
    /// never set (including "table exists but this key was never written" —
    /// the honest default for every pre-T14 DB and every fresh one).
    async fn get_app_meta(&self, key: &str) -> Result<Option<String>>;
    /// Upsert a marker. Used by the legacy-shell migration to record
    /// "already ran" so a second boot is a real zero-op, not a re-scan that
    /// happens to find nothing.
    async fn set_app_meta(&self, key: &str, value: &str) -> Result<()>;
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

pub(crate) fn cycle_text(c: MaturityPeriod) -> &'static str {
    match c {
        MaturityPeriod::Explore => "explore",
        MaturityPeriod::Expand => "expand",
        MaturityPeriod::Mature => "mature",
    }
}

pub(crate) fn parse_cycle(s: &str) -> MaturityPeriod {
    match s {
        "expand" => MaturityPeriod::Expand,
        "mature" => MaturityPeriod::Mature,
        _ => MaturityPeriod::Explore,
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

/// The `cron_task.mode` discriminant text. T10: `RunSkill`/`RunPrompt` carry
/// data now, but that data lives in `cron_task.target` (see `parse_cron_mode`
/// below), so this stays a plain by-reference discriminant lookup — no
/// column, no migration.
pub(crate) fn cron_mode_text(m: &CronMode) -> &'static str {
    match m {
        CronMode::RunWorkflow => "run_workflow",
        CronMode::RunSkill { .. } => "run_skill",
        CronMode::RunPrompt { .. } => "run_prompt",
        CronMode::CreateIssue => "create_issue",
        CronMode::CollectMetrics => "collect_metrics",
    }
}

/// Reconstruct a full [`CronMode`] from the two raw columns that carry it:
/// `mode` (the discriminant) and `target` (T10's payload column — a real
/// `SkillId` as text for `run_skill`, the raw prompt for `run_prompt`; unused
/// by the two pre-T10 variants, whose own `target` semantics are untouched).
/// An unparseable `run_skill` target (should never happen — the id is only
/// ever written by this same code) reads back as the nil id rather than
/// panicking — `App::tick_scheduler`'s `get_skill` lookup then honestly
/// reports "not found", same as an actually-deleted skill.
pub(crate) fn parse_cron_mode(mode: &str, target: &str) -> CronMode {
    match mode {
        "create_issue" => CronMode::CreateIssue,
        "collect_metrics" => CronMode::CollectMetrics,
        "run_skill" => CronMode::RunSkill {
            skill_id: uuid::Uuid::parse_str(target)
                .map(SkillId::from_uuid)
                .unwrap_or_else(|_| SkillId::from_uuid(uuid::Uuid::nil())),
        },
        "run_prompt" => CronMode::RunPrompt {
            prompt: target.to_string(),
        },
        _ => CronMode::RunWorkflow,
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

/// `source` (discriminant tag) + `official_library` (sub-tag, meaningful only
/// for `Official`) column values for a [`HubSource`] — T2's unification of
/// Skill's provenance onto the same enum Workflow already uses (plan/12 §6),
/// reused as-is by T5 for `agent` (same two-column shape, same enum — no
/// reason to duplicate the mapping per hub table). Mirrors `HubSource`'s own
/// on-disk shape 1:1, just spread across two plain `TEXT` columns instead of
/// one JSON blob (the `skill` table already had a dedicated `source TEXT`
/// column pre-T2 — no reason to switch to JSON just to gain a struct
/// variant). Despite the generic name, still named after its first caller;
/// renamed here to `hub_source_columns` now that a second table uses it.
pub(crate) fn hub_source_columns(s: &HubSource) -> (&'static str, String) {
    match s {
        HubSource::Official { official_library } => ("official", official_library.clone()),
        HubSource::Adopted => ("adopted", String::new()),
        HubSource::SelfBuilt => ("self_built", String::new()),
        HubSource::WithinSession => ("within_session", String::new()),
    }
}

/// Inverse of [`hub_source_columns`]. Handles one legacy shape: pre-T2 rows
/// written by the retired `LibSource::Official` (the 5 built-in
/// stage-methodology skills, seeded before this migration) have
/// `source='official'` but no `official_library` value — old DBs never had
/// that column. T2 reclassifies those as `SelfBuilt`: `Official` now means
/// "a curated *external* library" (must carry a real sub-tag), and this
/// app's own built-in methodology isn't one — the same call
/// `stage_template_workflow` already made on the Workflow side. Old
/// databases keep opening either way; nothing crashes, the label just
/// becomes honest under the new, stricter definition of `Official`. T5
/// reuses this unchanged for `agent.source`/`agent.official_library` — the
/// five built-in stage-role agents predate any `source` column exactly like
/// the five built-in stage-methodology skills did, so the same fallback
/// (`SelfBuilt`) is the honest read for them too.
pub(crate) fn parse_hub_source(tag: &str, official_library: &str) -> HubSource {
    match tag {
        "official" if !official_library.is_empty() => HubSource::Official {
            official_library: official_library.to_string(),
        },
        "official" => HubSource::SelfBuilt,
        "adopted" => HubSource::Adopted,
        "within_session" => HubSource::WithinSession,
        _ => HubSource::SelfBuilt,
    }
}

/// T11 (2026-07-23, plan/12 §7): `SkillCard::adapted_from` /
/// `AgentCard::adapted_from` — the "改编自 <库名>" 留痕 read-back. `source`/
/// `official_library` is a two-column scheme (see `hub_source_columns`); a
/// T11 flip (`update_skill`/`update_agent` with `flip_to_self_built`)
/// rewrites `source` to `'self_built'` but deliberately leaves the raw
/// `official_library` column value in place. `parse_hub_source` above only
/// ever reads that column when `tag == "official"`, so it never surfaces
/// there post-flip — this is the sibling read that recovers it: non-`None`
/// exactly when the tag has moved off `official` but the column still holds
/// a value, i.e. exactly the flipped-away case. `None` for a still-`Official`
/// row (its library already shows up in `source` itself, no duplication
/// needed) and for a row that was never official (empty column).
pub(crate) fn parse_adapted_from(tag: &str, official_library: &str) -> Option<String> {
    if tag != "official" && !official_library.is_empty() {
        Some(official_library.to_string())
    } else {
        None
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

pub(crate) fn cron_status_text(s: CronStatus) -> &'static str {
    match s {
        CronStatus::Running => "running",
        CronStatus::Normal => "normal",
        CronStatus::Failed => "failed",
        CronStatus::Paused => "paused",
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

pub(crate) fn connector_status_text(s: ConnectorStatus) -> &'static str {
    match s {
        ConnectorStatus::Connected => "connected",
        ConnectorStatus::Syncing => "syncing",
        ConnectorStatus::Error => "error",
        ConnectorStatus::Disconnected => "disconnected",
    }
}

pub(crate) fn issue_status_text(s: IssueStatus) -> &'static str {
    match s {
        IssueStatus::Backlog => "backlog",
        IssueStatus::Todo => "todo",
        IssueStatus::InProgress => "in_progress",
        IssueStatus::InReview => "in_review",
        IssueStatus::Done => "done",
        IssueStatus::Blocked => "blocked",
        IssueStatus::Cancelled => "cancelled",
    }
}

pub(crate) fn parse_issue_status(s: &str) -> IssueStatus {
    match s {
        "todo" => IssueStatus::Todo,
        "in_progress" => IssueStatus::InProgress,
        "in_review" => IssueStatus::InReview,
        "done" => IssueStatus::Done,
        "blocked" => IssueStatus::Blocked,
        "cancelled" => IssueStatus::Cancelled,
        _ => IssueStatus::Backlog,
    }
}

pub(crate) fn issue_priority_text(p: IssuePriority) -> &'static str {
    match p {
        IssuePriority::None => "none",
        IssuePriority::Low => "low",
        IssuePriority::Medium => "medium",
        IssuePriority::High => "high",
        IssuePriority::Urgent => "urgent",
    }
}

pub(crate) fn parse_issue_priority(s: &str) -> IssuePriority {
    match s {
        "low" => IssuePriority::Low,
        "medium" => IssuePriority::Medium,
        "high" => IssuePriority::High,
        "urgent" => IssuePriority::Urgent,
        _ => IssuePriority::None,
    }
}
