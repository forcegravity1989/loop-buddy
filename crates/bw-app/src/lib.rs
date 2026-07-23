//! `bw-app` — the UI-agnostic orchestration brain (plan `§3`).
//!
//! Command in, event out, single subscribable state. The UI never touches the
//! store or engine directly: it [`dispatch`](App::dispatch)es a [`Command`],
//! reads [`snapshot`](App::snapshot), and reacts to the [`Event`] stream from
//! [`subscribe`](App::subscribe). `App` holds one long-lived Mock [`Engine`]
//! (every project without a configured `workspace_path` runs on it, byte-for-
//! byte today's behavior) plus a process-wide [`ClaudeCliConfig`].
//! [`Command::RunWorkflow`] builds a fresh, one-shot real [`Engine`] around a
//! [`ClaudeCliExecutor`] per call for any project that HAS configured a
//! workspace — `workspace_path`/`allow_commands` are per-project runtime data
//! read from the store, not something fixed at [`App::new`] time.

#![forbid(unsafe_code)]

mod agent_import;
mod skill_import;

use bw_core::derive::AmberBand;
use bw_core::model::{
    classify_artifact_path, cron_due, parse_phase_outcome, stage_workflow,
    stage_workflow_with_playbook, AgentCard, AgentRef, Artifact, Cadence, Connector,
    ConnectorStatus, CronMode, CronStatus, CronTask, HubSource, Issue, IssuePriority, IssueStatus,
    KnowledgeSource, LoopConfig, Maturity, PhaseMeta, PhaseRole, ProjectCycle, ProjectPhase, Role,
    RunStatus, RunTrigger, Signal, SkillCard, SkillRef, SourceKind, StageKind, Verdict,
    WorkflowKind, WorkflowSpec, CONNECTOR_KIND_CLAUDE_CLI, CONNECTOR_KIND_GIT_REPO,
};
use bw_core::{
    AgentId, ArtifactId, ConnectorId, CronTaskId, IssueId, KnowledgeSourceId, MetricId, ProjectId,
    SessionId, SkillId, WorkflowId, WorkflowRunId,
};
use bw_engine::{
    allowed_tools_arg, evidence, ClaudeCliConfig, ClaudeCliExecutor, Engine, GitCommit,
    PermissionMode, RunCtx, RunEvent, UnsupportedCliExecutor,
};
use bw_store::{
    AgentEdit, GlobalHandoffRow, MetricRole, NewAgent, NewArtifact, NewConnector, NewCronTask,
    NewIssue, NewKnowledgeSource, NewMetric, NewProject, NewSession, NewSkill, NewSkillFile,
    NewStage, NewWorkflowSpec, ProjectRow, SessionKind, SkillEdit, Store, WorkflowEdit,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use time::OffsetDateTime;
use tokio::sync::broadcast;

/// Top-level workspace view (only meaningful for `hub == workspace`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum View {
    #[default]
    Projects,
    /// The creation card-flow (意图 → 快答 → 起草 → 审阅确认).
    Create,
    App,
}

/// Operating-view toolbar tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Progress,
    Workflow,
    Routine,
    Artifact,
    Version,
    /// Issue 看板 (R1) — assignable work units scoped to a stage, the
    /// multica-style board the operating view now surfaces.
    Issues,
}

/// Stage-axis selection: all stages or one of the five.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    All,
    Stage(StageKind),
}

/// UI → kernel intents.
pub enum Command {
    /// App start: load the project wall and re-derive every running project's
    /// signals against the *current* clock (staleness must show on the wall).
    Boot,
    /// Creation flow step 1 (意图): mints the project row immediately so the
    /// rest of the flow (drafting run, resume-if-interrupted) has somewhere
    /// real to attach to. `desc` carries the free-text brief.
    CreateProject {
        id: ProjectId,
        name: String,
        kind: String,
        desc: String,
        /// P1: optional pre-existing repo to bind (must contain `.git`). When
        /// `None` and a workspaces root is configured, a fresh repo is minted
        /// at creation. Bound repos are never rewritten by the workbench.
        workspace: Option<String>,
    },
    /// Creation flow step 2 (快速问题 · 周期).
    SetCycle {
        cycle: ProjectCycle,
    },
    /// 对标竞品 + 三个月成功标准 (creation flow's free-text questions).
    UpdateBrief {
        benchmark: String,
        opportunity: String,
    },
    UpdateNorthStar {
        value: String,
        def: String,
    },
    /// Record a metric + its current value as an append-only Manual observation
    /// (creation-flow review, or later while operating a stage). Signal is
    /// derived, never set here.
    UpsertManualMetric {
        id: MetricId,
        name: String,
        def: String,
        role: MetricRole,
        stage_kind: Option<StageKind>,
        target: String,
        amber: AmberBand,
        value: String,
    },
    /// Week-plan edit: new target + this week's driver. No value is touched;
    /// recompute re-derives against the new target.
    UpdateWeekPlan {
        metric: MetricId,
        new_target: String,
        last_target: String,
        driver: String,
    },
    /// The monitoring loop's heartbeat: a new Manual value is born as an
    /// observation, then every signal is re-derived. Never sets a signal.
    RecordObservation {
        metric: MetricId,
        value: String,
    },
    /// A **machine-collected** observation — same append-only path as
    /// `RecordObservation`, but the source is the collector that really
    /// measured it (`Ci` / `GitPr` / …), never `Manual`. This is the evidence
    /// collector's write path (`bw_engine::evidence` → metric), the first
    /// non-Manual L0 producer (Tier D's minimal down payment).
    RecordCollectedObservation {
        metric: MetricId,
        value: String,
        source: SourceKind,
    },
    /// Hand-set plan progress for one stage (plan data, not a signal — the
    /// derive chain is untouched).
    SetStageProgress {
        stage_kind: StageKind,
        progress: u8,
    },
    /// Flip one handoff/DoD checklist box.
    ToggleDod {
        stage_kind: StageKind,
        index: usize,
    },
    /// Advance the project's active stage (or reflux `Ops → Prototype`).
    /// `risky` and `note` are the caller's honest account of the checklist
    /// state — a handoff is never silently blocked on an unchecked box.
    HandoffStage {
        risky: bool,
        note: String,
    },
    /// Confirms the creation-flow draft: materializes the five stages (each
    /// on the chosen review cadence) and switches the project into `Running`.
    CompleteCreation {
        cadence: Cadence,
    },
    /// Configure (or, with an empty `path`, clear) the real-executor target
    /// directory + whether it may also run shell commands. `path` must be a
    /// real, existing directory unless empty — a bad path fails fast here
    /// rather than surfacing only when a workflow is next run.
    SetWorkspace {
        path: String,
        allow_commands: bool,
    },
    /// Replace the process-wide `ClaudeCliConfig` outright (Settings hub).
    /// In-memory only — same persistence tier it already had (env-var-seeded
    /// once at boot); this just makes it editable for the rest of the
    /// process's lifetime instead of frozen.
    SetClaudeConfig {
        binary: Option<String>,
        max_budget_usd: f64,
        default_mode: PermissionMode,
        commands_mode: PermissionMode,
    },
    /// Real `git log` on the active project's `workspace_path` (Version
    /// panel). Explicit, user-triggered — never fetched eagerly on `Boot`,
    /// since it's per-project, potentially slow, and most projects have no
    /// `workspace_path` configured at all.
    LoadVersionLog,
    /// Load the active project's registered artifacts into state (Artifact
    /// panel). Same explicit-load pattern as `LoadVersionLog`.
    LoadArtifacts,
    /// L1(plan/11): load one cron task's real fire history
    /// (`Store::cron_effectiveness` — always existed, never had a caller).
    /// Same explicit-load pattern as `LoadArtifacts`.
    LoadCronEffectiveness(CronTaskId),
    /// P4: assemble one Issue's detail (its runs + each run's real file
    /// changes + its artifacts) into state for the board overlay. Read-only.
    OpenIssueDetail(IssueId),
    /// P4: close the overlay (clears the assembled detail).
    CloseIssueDetail,
    /// Re-scan the active project's workspace right now and register any new
    /// artifact versions (the manual counterpart to the automatic post-run
    /// scan). Requires a configured workspace.
    CollectArtifacts,
    /// Run a connector's *real* probe: `git-repo` collects live workspace
    /// evidence (and feeds it to the bound project's matching metrics as
    /// `SourceKind::Connector` observations — Tier D for real); `claude-cli`
    /// checks the executor binary. Any other kind errors honestly — there is
    /// no fake "synced" state.
    SyncConnector {
        id: ConnectorId,
    },
    StartSession {
        id: SessionId,
        stage_kind: Option<StageKind>,
        kind: SessionKind,
        title: String,
    },
    RunWorkflow {
        session: SessionId,
        spec: WorkflowSpec,
    },
    /// Run one stage's **playbook** workflow for the active project: the
    /// kernel assembles the real project context (brief/north star/last
    /// handoff note/workspace state) into `stage_workflow_with_playbook`'s
    /// per-phase instructions, then executes through the same
    /// `run_workflow_inner` path as any other run. This is the "五角色真实
    /// 执行" entry point — the UI/conductor names the stage; the kernel owns
    /// what the role actually gets told.
    RunStagePlaybook {
        session: SessionId,
        stage_kind: StageKind,
    },
    /// A3: run an Issue — assemble the issue's title/desc + its stage's role
    /// playbook + any distilled (compounded) skills from the same project into
    /// one real run through `run_workflow_inner`. The run records the issue_id,
    /// so the issue's detail answers "which runs/产物 did this produce?". The
    /// issue is pushed `InProgress` at start, `InReview` on success, and left
    /// `InProgress` on failure — **never auto-Done** (Done is a human
    /// `TransitionIssue` only; one work item, one human-confirmed credit).
    RunIssue {
        session: SessionId,
        id: IssueId,
    },
    /// Reload the hub library (`workflow_specs`/`skills`/`agents`) from the
    /// store. Called at `Boot`; also dispatchable standalone for a manual
    /// refresh. Deliberately separate from `Boot` — hub data has nothing to
    /// do with project-signal staleness, so this keeps `Boot`'s own contract
    /// unchanged.
    RefreshHubs,
    CreateWorkflowSpec {
        id: WorkflowId,
        name: String,
        prompt: String,
        goal: String,
        stage_ref: Option<u8>,
        phases: Vec<String>,
        /// Per-phase real instructions (playbook), index-aligned with
        /// `phases`; empty = every phase shares `prompt` (legacy behavior).
        phase_prompts: Vec<String>,
        agents: Vec<AgentRef>,
        skills: Vec<SkillRef>,
        loop_config: LoopConfig,
        maturity: Maturity,
        scope: String,
        source: HubSource,
        trigger: Option<String>,
    },
    /// Promote the workflow most recently run in `session` (reconstructed
    /// from the session's `stage_kind`, since a `Dynamic` spec is never
    /// itself persisted) into a new `Static` hub entry.
    PromoteWorkflow {
        new_id: WorkflowId,
        session: SessionId,
        source: HubSource,
    },
    /// Run a workflow already stored in the hub. Looks the spec up, bumps its
    /// `uses` counter, then executes identically to `RunWorkflow`.
    RunHubWorkflow {
        session: SessionId,
        workflow_id: WorkflowId,
    },
    /// "优化" an existing **Static** hub workflow — revise its authored
    /// content in place (bumps `version`; `uses`/`maturity`/`source` are
    /// untouched). Distinct from `PromoteWorkflow` (mints a brand-new row
    /// from a session run) and `CreateWorkflowSpec` (a fresh spec).
    UpdateWorkflowSpec {
        id: WorkflowId,
        prompt: String,
        goal: String,
        phases: Vec<String>,
        /// Per-phase instructions (may be empty — dropping back to a single
        /// shared `prompt` is a legal edit).
        phase_prompts: Vec<String>,
        agents: Vec<AgentRef>,
        skills: Vec<SkillRef>,
        /// Why this "优化" happened — frozen onto the version snapshot (iter 5).
        note: String,
    },
    CreateSkill {
        id: SkillId,
        name: String,
        desc: String,
        category: String,
        source: HubSource,
        /// Executable body (may be empty — a catalog reference entry).
        content: String,
    },
    /// Distill a new skill from a completed, assigned Issue — the "every
    /// solution compounds into a reusable skill" link. Provenance + Done/
    /// assignee validation lives in the store; this is a thin wrapper that
    /// delegates and refreshes, like `CreateSkill`. `content` is the distilled
    /// method body itself — a skill minted from real work must be executable
    /// content, not another empty catalog card.
    DistillSkillFromIssue {
        skill_id: SkillId,
        issue_id: IssueId,
        name: String,
        desc: String,
        category: String,
        content: String,
    },
    /// Copy-on-import a real, on-disk skill folder (T2, plan/12 §2):
    /// `source_path` must contain a `SKILL.md` whose frontmatter has
    /// `name`/`description`; every other file underneath lands in
    /// `skill_file` verbatim (real relative paths, no predetermined
    /// category). Once imported, the new skill has zero dependency on
    /// `source_path` — it can move, change, or vanish afterward.
    ///
    /// `official_library` is not part of plan/12 §2's headline
    /// `{ source_path, project_id }` shorthand, but this command still needs
    /// it: without an explicit sub-tag, a generic "import any SKILL.md
    /// folder" command has no honest way to know whether the folder came
    /// from a BW-curated library — inventing "mattpocock-skills" from a path
    /// convention would be the exact kind of guessing this ticket's own
    /// frontmatter-parsing rule forbids. `None` = ad-hoc personal import →
    /// `HubSource::SelfBuilt`; `Some(lib)` → `HubSource::Official {
    /// official_library: lib }`. T3's `ImportSkillLibrary` (batch) threads a
    /// real `Some(..)` through this same field for every package it finds
    /// under a library root.
    ImportSkillPackage {
        source_path: String,
        project_id: Option<ProjectId>,
        official_library: Option<String>,
    },
    /// Batch-import every real skill folder under a library root (T3,
    /// plan/12 §1/§2): finds every directory that directly contains a
    /// `SKILL.md` (`node_modules`/`.git`/`target` pruned without
    /// descending — real libraries don't nest skills inside these, it's
    /// pure efficiency/safety insurance), and each hit goes through the
    /// exact same disk-parsing path `ImportSkillPackage` uses — a batch
    /// import and a hand-run single-package import of the same folder
    /// produce byte-identical rows.
    ///
    /// Idempotent by `(name, official_library)`: a name already imported
    /// from the same `official_library` is skipped, never overwritten —
    /// re-running this (e.g. a library version bump) can't silently clobber
    /// a row a user has since hand-edited (T11 territory: editing flips a
    /// row to `SelfBuilt`, which this check's `official_library` filter
    /// naturally no longer matches, so an edited row is never skipped-away
    /// from re-import consideration by name collision with itself) or
    /// double-insert a duplicate. `official_library` is required (not
    /// `Option`, unlike `ImportSkillPackage`) — a library import is by
    /// definition an official-selection provenance, never an ad-hoc
    /// personal one. Emits `Event::SkillLibraryImported` with the real
    /// imported/skipped tally.
    ImportSkillLibrary {
        root_path: String,
        official_library: String,
        project_id: Option<ProjectId>,
    },
    /// SkillHub's detail-panel edit — content only (`maturity`/`uses` are
    /// lifecycle data, untouched).
    UpdateSkill {
        id: SkillId,
        name: String,
        desc: String,
        category: String,
        content: String,
    },
    CreateAgent {
        id: AgentId,
        name: String,
        role: String,
        skills: Vec<String>,
        model: String,
        /// Standing instructions (may be empty — a catalog reference entry).
        instructions: String,
    },
    /// AgentHub's detail-panel edit — content only (`maturity`/`runs`/
    /// `win_rate` are lifecycle data, untouched).
    UpdateAgent {
        id: AgentId,
        name: String,
        role: String,
        skills: Vec<String>,
        model: String,
        instructions: String,
    },
    /// Import a real, on-disk AGENT.md (T5, plan/12 §3): `source_path` must
    /// be a file whose frontmatter has `name`/`description`, and may have
    /// `tools` (→ AllowedTools)/`model`; the body becomes `instructions`.
    /// Every other frontmatter key is read and silently ignored (same rule
    /// `ImportSkillPackage` follows for SKILL.md). No file-tree concept here
    /// unlike Skill — one AGENT.md is the entire definition, so this maps
    /// straight onto `Store::create_agent`, no new store method needed.
    ///
    /// `official_library`: same shape as `ImportSkillPackage`'s field —
    /// `None` = ad-hoc personal import → `HubSource::SelfBuilt`;
    /// `Some(lib)` → `HubSource::Official { official_library: lib }`. The
    /// 67-file ECC batch import threads `Some("ecc")` through this same
    /// field for every file.
    ///
    /// T11 (2026-07-23, plan/12 §7): unlike `ImportSkillPackage` (which stays
    /// purely additive — dedup is `ImportSkillLibrary`'s job, a separate
    /// batch command), Skill has no standalone "import one AGENT.md" caller
    /// driving a real 67-file batch the way this command does (there is no
    /// `ImportAgentLibrary`; the vendored-ECC example dispatches this command
    /// once per file in a loop). So *this* singular command is where an
    /// `official_library: Some(lib)` re-import's own idempotency has to
    /// live: a name that already exists under `lib` (still `Official`, or
    /// hand-edited and flipped to `SelfBuilt` — see `AgentCard::adapted_from`)
    /// is silently skipped, never overwritten or duplicated. An ad-hoc
    /// `None` import stays purely additive (no batch-reimport concept for a
    /// personal one-off), matching `ImportSkillPackage`'s own rule.
    ImportAgentDefinition {
        source_path: String,
        official_library: Option<String>,
    },
    CreateCronTask {
        id: CronTaskId,
        name: String,
        target: String,
        schedule: Cadence,
        project_id: Option<ProjectId>,
    },
    /// A1: an autopilot cron task — when due, it mints a stage-scoped Issue
    /// (Todo, optionally assigned) instead of running a workflow. No-hijack: it
    /// never auto-runs anything. `assignee` is an agent NAME matched at fire
    /// time (no match ⇒ honest unassigned Issue, not a failure).
    CreateAutopilotTask {
        id: CronTaskId,
        name: String,
        schedule: Cadence,
        project_id: Option<ProjectId>,
        stage: StageKind,
        assignee: Option<String>,
    },
    /// T10 (plan/12 §5): a cron task that, when due, really runs a Skill's
    /// `content` as the prompt — a genuine `SkillId` reference, resolved at
    /// *fire* time (never at creation time), so an honest "技能已删除"
    /// failure is possible without ever crashing.
    CreateRunSkillCronTask {
        id: CronTaskId,
        name: String,
        schedule: Cadence,
        project_id: Option<ProjectId>,
        skill_id: SkillId,
    },
    /// T10: a cron task that, when due, really runs a bare prompt — no
    /// entity involved at all.
    CreateRunPromptCronTask {
        id: CronTaskId,
        name: String,
        schedule: Cadence,
        project_id: Option<ProjectId>,
        prompt: String,
    },
    /// Pause/resume a cron task — the "人工介入" lever. Pure status flip;
    /// never touches `last_run` since nothing actually ran.
    SetCronStatus {
        id: CronTaskId,
        status: CronStatus,
    },
    /// Record that a cron task's target really ran just now (this app has no
    /// background scheduler — manually triggered from Cron Hub's "▶ 立即执行").
    /// `status` is the real outcome (`Running` when the caller fires this
    /// before dispatching the actual run, `Normal`/`Failed` once it's known).
    MarkCronRun {
        id: CronTaskId,
        status: CronStatus,
    },
    CreateConnector {
        id: ConnectorId,
        name: String,
        kind: String,
        scope: String,
        /// Project this connector feeds (`git-repo` is always bound).
        project_id: Option<ProjectId>,
        /// Kind-specific real config (`git-repo`: workspace path;
        /// `claude-cli`: binary override, empty = PATH).
        config: String,
    },
    CreateKnowledgeSource {
        id: KnowledgeSourceId,
        name: String,
        kind: String,
        used_by: String,
    },
    /// Create a new issue in the active project (defaults to `Backlog`,
    /// auto-assigned per-project number). Scoped to the given stage.
    CreateIssue {
        id: IssueId,
        stage: StageKind,
        title: String,
        desc: String,
        priority: IssuePriority,
    },
    /// Move an issue to a new kanban status (the kanban lifecycle transition).
    TransitionIssue {
        id: IssueId,
        status: IssueStatus,
    },
    /// Assign (or, with `None`, unassign) an issue to an agent teammate.
    AssignIssue {
        id: IssueId,
        assignee: Option<AgentId>,
    },
    /// A5-F: the only path into `Blocked` — `reason` must be non-empty.
    /// `TransitionIssue { status: Blocked }` is rejected; this is how a stuck
    /// issue leaves a record of *why*, not just *that*.
    BlockIssue {
        id: IssueId,
        reason: String,
    },
    /// Reload the active project's issues from the store (mirrors
    /// `RefreshHubs` for the hub library, but project-scoped).
    RefreshIssues,
    SendSessionMessage {
        session: SessionId,
        text: String,
    },
    AnnotateWeeklyReview {
        human_override: Option<Signal>,
        reason: String,
    },
    OpenProject(ProjectId),
    /// Delete a project and everything scoped to it. The CRUD-completeness
    /// counterpart to `CreateProject` — irreversible; the UI is responsible
    /// for confirming with the user before dispatching this.
    DeleteProject(ProjectId),
    BackToProjects,
    SetPanel(Panel),
    SetScope(Scope),
    /// Select (or clear) the chat-focused session in the operating view.
    SelectSession(Option<SessionId>),
}

/// Kernel → UI facts (already happened).
#[derive(Clone, Debug)]
pub enum Event {
    ProjectsChanged,
    ProjectUpdated(ProjectId),
    ViewChanged(View),
    SessionMessageAdded {
        session: SessionId,
        role: Role,
        text: String,
    },
    /// A run is really about to begin — carries the spec's own name/agents/
    /// skills so the UI can show what's actually behind this run (real
    /// `AgentRef`/`SkillRef` data from the spec, never invented) before the
    /// first `WorkflowProgress` phase event arrives. Emitted once, first,
    /// ahead of any `WorkflowProgress` for the same run.
    RunStarted {
        workflow_name: String,
        agents: Vec<AgentRef>,
        skills: Vec<SkillRef>,
    },
    WorkflowProgress {
        phase_idx: usize,
        status: String,
    },
    WorkflowDone,
    WorkflowFailed(String),
    WeeklyReviewAnnotated,
    StageHandoff {
        from: StageKind,
        to: StageKind,
        risky: bool,
    },
    WorkflowSpecsChanged,
    SkillsChanged,
    /// A batch `Command::ImportSkillLibrary` just finished — the real tally,
    /// not an assumption: `imported` = new rows this run actually inserted,
    /// `skipped` = `(name, official_library)` matches that already existed
    /// and were left untouched (idempotent re-run safety).
    SkillLibraryImported {
        official_library: String,
        imported: u32,
        skipped: u32,
    },
    AgentsChanged,
    CronTasksChanged,
    /// A real, unattended auto-fire from `App::tick_scheduler` just finished
    /// (not a manual "▶ 立即执行") — the live "monitoring" signal for the
    /// scheduler: a subscriber can toast/notify without the run having
    /// stolen `active_project`/the user's current screen to get there.
    CronAutoFired {
        id: CronTaskId,
        name: String,
        ok: bool,
    },
    ConnectorsChanged,
    /// A connector's real probe just finished — `detail` is the probe's
    /// honest summary (e.g. "3 提交 · 12 文件" or the error text).
    ConnectorSynced {
        name: String,
        ok: bool,
        detail: String,
    },
    KnowledgeSourcesChanged,
    IssuesChanged,
    ActivityChanged,
    ClaudeConfigChanged,
    VersionLogChanged,
    /// New artifact versions were registered (post-run auto-scan or a manual
    /// `CollectArtifacts`). Carries the honest count of *genuinely new* rows.
    ArtifactsRegistered {
        fresh: u32,
    },
    /// The `AppState.artifacts` snapshot was (re)loaded.
    ArtifactsChanged,
    /// L1(plan/11): the `AppState.cron_effectiveness` snapshot was (re)loaded.
    CronEffectivenessChanged,
    /// The self-driving optimization cycle (iter 18) just ran. Carries the
    /// full report — scanned workflows, proposals generated, what was
    /// auto-applied (safe/positive), and what was deferred to a human. A
    /// subscriber can surface "N opportunities found" without the cycle
    /// having changed anything destructive.
    OptimizationCycleReported {
        report: OptimizationReport,
    },
}

/// The outcome of one self-driving optimization cycle (iter 18) — the
/// measure→propose→gate loop's receipt. Every count is real (derived from the
/// store), never asserted. `auto_applied`/`defer_to_human` carry the human-
/// readable titles so a UI can render them directly.
#[derive(Clone, Debug)]
pub struct OptimizationReport {
    /// Hub workflows scanned this cycle.
    pub scanned: u32,
    /// Total proposals generated across all workflows.
    pub proposals: u32,
    /// Safe/positive proposals the loop applied on its own (titles).
    pub auto_applied: Vec<String>,
    /// Proposals needing a human's judgement before acting (titles).
    pub defer_to_human: Vec<String>,
    /// Proposals rejected for insufficient evidence (count only).
    pub rejected: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] bw_store::StoreError),
    #[error("engine: {0}")]
    Engine(String),
    #[error("no active project")]
    NoActiveProject,
    #[error("project not found")]
    NotFound,
    #[error("invalid: {0}")]
    Invalid(String),
}

/// How a `run_workflow_inner` call resolved once its adversarial review loop
/// settled (T9, plan/12 §4). An honest *failure* (executor error, or a review
/// output with no parseable verdict) is NOT an outcome here — it surfaces as
/// `Err(AppError)`, leaving any associated Issue untouched (RunIssue keeps it
/// `InProgress` for a retry).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    /// The workflow ran to completion — a straight pipeline with no gate, or a
    /// gated one whose Evaluator finally rendered `PASS`. The caller advances a
    /// bound Issue to `InReview` (never `Done` — that stays a human decision).
    Completed,
    /// The Evaluator kept rejecting up to `loop_config.max_iter` rounds. Not a
    /// failure and never auto-`Failed`: the caller parks a bound Issue in
    /// `Blocked` (with this reason) for a human to decide (retry / rework the
    /// workflow / drop). A run with no bound Issue just leaves its honest
    /// per-round rows — no fabricated Issue (plan/12 §4).
    BlockedAtCap { reason: String },
}

/// How the adversarial review loop in `run_workflow_inner` terminated —
/// internal to that function (the outward-facing shape is [`RunOutcome`] /
/// `Err`). Carried out of the `loop` as its break value so the after-loop
/// accounting runs exactly once for every terminal path.
enum LoopEnd {
    /// The workflow passed (a gate rendered `PASS`, or there was no gate).
    Passed,
    /// The gate kept rejecting up to `max_iter` rounds — a Blocked outcome
    /// (never auto-`Failed`).
    Blocked(String),
    /// An honest failure: an executor error, or a review output with no
    /// parseable verdict. Surfaces as `Err`.
    Failed(AppError),
}

#[derive(Clone, Debug)]
pub struct AppState {
    pub view: View,
    pub panel: Panel,
    pub scope: Scope,
    pub active_project: Option<ProjectId>,
    pub active_session: Option<SessionId>,
    pub projects: Vec<ProjectRow>,
    /// Hub library — global, loaded independent of any active project.
    pub workflow_specs: Vec<WorkflowSpec>,
    pub skills: Vec<SkillCard>,
    pub agents: Vec<AgentCard>,
    pub cron_tasks: Vec<CronTask>,
    pub connectors: Vec<Connector>,
    pub knowledge_sources: Vec<KnowledgeSource>,
    /// Issues for the active project (empty when no project is open). Mirrors
    /// `cron_tasks` but project-scoped — loaded by `refresh_issues`.
    pub issues: Vec<Issue>,
    /// Activity feed — derived from `handoff` (+ `project` join), never
    /// written to directly. See `Store::list_recent_handoffs`.
    pub recent_activity: Vec<GlobalHandoffRow>,
    /// Process-wide `ClaudeCliExecutor` config (Settings hub). Seeded once
    /// from env vars at boot (`App::new`'s caller decides that), editable
    /// afterward via `Command::SetClaudeConfig` — in memory only, same
    /// persistence tier it already had.
    pub claude_config: ClaudeCliConfig,
    /// Last real `git log` fetch (Version panel), tagged with which project
    /// it's for so a stale result from a previously-open project is never
    /// shown against the wrong one. `None` until `Command::LoadVersionLog`
    /// runs at least once — never eagerly fetched (per-project, potentially
    /// slow, and most projects have no `workspace_path` at all).
    pub version_log: Option<(ProjectId, Result<Vec<GitCommit>, String>)>,
    /// Registered artifacts of the active project (Artifact panel) — same
    /// explicit-load, project-tagged pattern as `version_log`.
    pub artifacts: Option<(ProjectId, Vec<Artifact>)>,
    /// L1(plan/11): last-loaded cron task's real fire history — same single-
    /// slot, task-tagged explicit-load pattern as `artifacts`/`version_log`.
    pub cron_effectiveness: Option<(CronTaskId, bw_core::model::CronEffectiveness)>,
    /// P4: the explicitly-opened Issue detail (board overlay) — same
    /// explicit-load pattern as `artifacts`. `None` = no overlay open.
    pub issue_detail: Option<IssueDetailData>,
}

/// P4: everything the Issue-detail overlay shows, assembled read-only at
/// `OpenIssueDetail` time. `changes` pairs each run with the files it really
/// touched (`Err` = the honest reason a diff isn't available — mock run, or
/// a run recorded before change-tracking existed).
#[derive(Clone, Debug)]
pub struct IssueDetailData {
    pub issue: Issue,
    pub runs: Vec<bw_core::model::WorkflowRun>,
    pub changes: Vec<bw_core::model::RunChanges>,
    pub artifacts: Vec<Artifact>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            view: View::Projects,
            panel: Panel::Progress,
            scope: Scope::All,
            active_project: None,
            active_session: None,
            projects: Vec::new(),
            workflow_specs: Vec::new(),
            skills: Vec::new(),
            agents: Vec::new(),
            cron_tasks: Vec::new(),
            connectors: Vec::new(),
            knowledge_sources: Vec::new(),
            issues: Vec::new(),
            recent_activity: Vec::new(),
            claude_config: ClaudeCliConfig::default(),
            version_log: None,
            artifacts: None,
            cron_effectiveness: None,
            issue_detail: None,
        }
    }
}

/// The orchestration brain.
pub struct App {
    store: Arc<dyn Store>,
    mock_engine: Engine,
    state: AppState,
    events: broadcast::Sender<Event>,
    /// Root under which `CompleteCreation` auto-provisions each new project's
    /// own git workspace (all-in-one-codebase 默认: 每个项目=一个代码仓).
    /// `None` (the default, and every pre-完整形态 caller) keeps the old
    /// behavior: no provisioning, workspace stays an explicit opt-in.
    workspaces_root: Option<PathBuf>,
}

impl App {
    pub fn new(store: Arc<dyn Store>, mock_engine: Engine, claude_config: ClaudeCliConfig) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self {
            store,
            mock_engine,
            state: AppState {
                claude_config,
                ..AppState::default()
            },
            events: tx,
            workspaces_root: None,
        }
    }

    /// Enable all-in-one-codebase auto-provisioning: every project completed
    /// through the creation flow gets its own real git repo under `root`
    /// (created + `git init` + first commit + a bound `git-repo` connector),
    /// so the five roles have a real substrate from birth instead of Mock.
    pub fn with_workspaces_root(mut self, root: PathBuf) -> Self {
        self.workspaces_root = Some(root);
        self
    }

    /// Subscribe to the event stream. Each subscriber gets its own receiver.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    /// The current state (read-only).
    pub fn snapshot(&self) -> &AppState {
        &self.state
    }

    /// Borrow the store (for read queries the UI projects through selectors).
    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    fn emit(&self, e: Event) {
        // Ignore "no receivers" — events are fire-and-forget facts.
        let _ = self.events.send(e);
    }

    fn active(&self) -> Result<ProjectId, AppError> {
        self.state.active_project.ok_or(AppError::NoActiveProject)
    }

    async fn refresh_projects(&mut self) -> Result<(), AppError> {
        self.state.projects = self.store.list_projects().await?;
        Ok(())
    }

    async fn refresh_workflow_specs(&mut self) -> Result<(), AppError> {
        self.state.workflow_specs = self.store.list_workflow_specs().await?;
        Ok(())
    }

    async fn refresh_skills(&mut self) -> Result<(), AppError> {
        self.state.skills = self.store.list_skills().await?;
        Ok(())
    }

    async fn refresh_agents(&mut self) -> Result<(), AppError> {
        self.state.agents = self.store.list_agents().await?;
        Ok(())
    }

    async fn refresh_cron_tasks(&mut self) -> Result<(), AppError> {
        self.state.cron_tasks = self.store.list_cron_tasks().await?;
        Ok(())
    }

    async fn refresh_connectors(&mut self) -> Result<(), AppError> {
        self.state.connectors = self.store.list_connectors().await?;
        Ok(())
    }

    async fn refresh_knowledge_sources(&mut self) -> Result<(), AppError> {
        self.state.knowledge_sources = self.store.list_knowledge_sources().await?;
        Ok(())
    }

    /// Reload the active project's issues. When no project is active, the list
    /// is cleared to empty (not an error — the UI shows an empty board).
    async fn refresh_issues(&mut self) -> Result<(), AppError> {
        match self.state.active_project {
            Some(p) => {
                self.state.issues = self.store.list_issues(p, None, None).await?;
            }
            None => self.state.issues.clear(),
        }
        Ok(())
    }

    async fn refresh_activity(&mut self) -> Result<(), AppError> {
        self.state.recent_activity = self.store.list_recent_handoffs(50).await?;
        Ok(())
    }

    /// Shared by `Command::RunWorkflow`, `Command::RunHubWorkflow`, and
    /// `tick_scheduler`'s real auto-fire — the first two differ only in how
    /// `spec` was obtained (a hub lookup + a `uses` bump) and look identical
    /// once they have one. `project` is explicit (not read off
    /// `self.state.active_project`) so a background scheduler fire can run a
    /// workflow against its *bound* project without touching — let alone
    /// hijacking — whatever project the user currently has open.
    async fn run_workflow_inner(
        &mut self,
        project: ProjectId,
        session: SessionId,
        mut spec: WorkflowSpec,
        trigger: RunTrigger,
        cron_task_id: Option<CronTaskId>,
        issue_id: Option<IssueId>,
    ) -> Result<RunOutcome, AppError> {
        let p = project;
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;

        // Skill refs become *operative* here: for a non-playbook spec (a
        // playbook already bakes its skill bodies into every phase prompt in
        // bw-core), resolve each ref against the Skill Hub and append the
        // real bodies to the shared prompt. Name-only refs with no stored
        // content contribute nothing — never a fabricated placeholder.
        if spec.phase_prompts.is_empty() && !spec.skills.is_empty() {
            let block = self.skills_prompt_block(&spec.skills).await?;
            if !block.is_empty() {
                spec.prompt = format!("{}{block}", spec.prompt);
            }
        }

        let ctx = RunCtx {
            project: p,
            workflow: spec.id,
        };

        // Record the run's start *before* the engine runs — so even a crash
        // mid-run leaves an honest "started, never settled" row instead of a
        // fabricated success (iter 1 telemetry foundation). `params_json`
        // snapshots what the spec *was* at run time (phases/loop/agents/
        // skills) — so after a later "优化" changes the spec, history still
        // shows what each past run actually executed (iter 3 param capture).
        // P4: capture the workspace HEAD before the engine touches anything —
        // the "before" half of this run's recorded change window. Mock runs
        // (no workspace) record nothing: no files were ever at stake.
        let heads_workspace = proj.workspace_path.trim().to_string();
        let head_before = if heads_workspace.is_empty() {
            None
        } else {
            evidence::head_commit(&heads_workspace).await.ok().flatten()
        };

        // T6 (plan/12 §3): resolve the executing Agent's CLI + tools BEFORE
        // anything runs — routing is real, not a display label, and the
        // decision must apply identically whether this project runs on the
        // real `ClaudeCliExecutor` or the shared Mock engine (an unsupported
        // CLI is never silently allowed through on a mock project).
        let (agent_cli, agent_tools) = self.resolve_agent_route(issue_id).await?;
        // The literal `--allowedTools` value `ClaudeCliExecutor` would pass —
        // computed here, before any subprocess spawn, so it's recorded in
        // `params_json` independent of whether the real `claude -p` call
        // ever succeeds (gateway 抖动 is never a verification gate).
        let allowed_tools = allowed_tools_arg(&agent_tools, proj.allow_commands);

        let params_json = run_params_snapshot(
            &spec,
            trigger,
            &agent_cli,
            &agent_tools,
            allowed_tools.as_deref(),
        );

        // The review gate: the FIRST Evaluator phase (T8's real `role`, not a
        // name guess). A workflow with none is a straight pipeline — one round,
        // all phases, byte-for-byte the pre-T9 behavior. (A single review gate
        // per workflow is all the built-in playbooks model today; a second
        // Evaluator, if authored, runs as a plain tail phase.)
        let eval_idx = spec
            .phases
            .iter()
            .position(|ph| ph.role == PhaseRole::Evaluator);
        let num_phases = spec.phases.len();
        let max_iter = spec.loop_config.max_iter.max(1) as u32;

        // Announce once, before the first round — real name/agents/skills off
        // `spec`, so a live subscriber can render "this run uses X/Y".
        self.emit(Event::RunStarted {
            workflow_name: spec.name.clone(),
            agents: spec.agents.clone(),
            skills: spec.skills.clone(),
        });

        // `workspace_path` is per-project runtime data, not baked into a
        // long-lived Engine: unconfigured projects run on the shared Mock engine
        // (byte-for-byte today's behavior); a configured one gets a fresh
        // one-shot real executor for THIS call (shared across the call's rounds).
        // Held immutably across the loop — every in-loop `self` touch is a shared
        // borrow (`self.store` / `self.emit`), never `&mut self`, so this holds.
        //
        // T6 (plan/12 §3): the `agent_cli` match happens FIRST, before the
        // mock/real branch below — an unsupported CLI ("codex"/"cursor"/…)
        // routes to the honest `UnsupportedCliExecutor` regardless of whether
        // this project even has a real workspace configured. Only
        // `"claude-code"` (the default for an unassigned issue or any other
        // caller) reaches the existing mock-vs-real split, unchanged.
        let fresh_engine;
        let engine: &Engine = match agent_cli.as_str() {
            "claude-code" => {
                if proj.workspace_path.trim().is_empty() {
                    &self.mock_engine
                } else {
                    let executor = ClaudeCliExecutor::new(
                        self.state.claude_config.clone(),
                        PathBuf::from(proj.workspace_path.trim()),
                        proj.allow_commands,
                        agent_tools.clone(),
                    );
                    fresh_engine = Engine::new(Arc::new(executor));
                    &fresh_engine
                }
            }
            other => {
                // 诚实报错,绝不静默回落到 claude-code:本机没有为 codex/cursor
                // 等值接好真实执行器。Reuses the `Executor` trait seam — this
                // executor's first (and only) call errors, and the existing
                // "executor failed → settle Failed" path records it honestly.
                let executor = UnsupportedCliExecutor::new(other.to_string());
                fresh_engine = Engine::new(Arc::new(executor));
                &fresh_engine
            }
        };

        // Live progress streams out of the engine callback (broadcast::send is
        // sync); only persistence (async) is deferred to after each round.
        let live = self.events.clone();

        // ── Adversarial review loop (plan/12 §4, T9) ────────────────────────
        // Each round is its OWN settled `workflow_run` row: "多轮 run 记录" reads
        // back as multiple rows, and settle-once holds because each row is
        // settled exactly once. Round 1 runs from phase 0; each Evaluator打回
        // restarts from the reject target and increments the round.
        let range_end = match eval_idx {
            Some(e) => e + 1, // through the gate, inclusive
            None => num_phases,
        };
        let mut start = 0usize;
        let mut round: u32 = 1;
        let mut baton: Option<String> = None;
        // Set at the top of every round (before any `break`), so it is
        // definitely-assigned for the after-loop accounting — the last round's
        // row is the one that produced the final state.
        let mut last_run_log: WorkflowRunId;
        let mut final_run_ok = false;

        let end: LoopEnd = loop {
            // Record this round's row start *before* the engine runs — a crash
            // mid-round leaves an honest "started, never settled" row, never a
            // fabricated success.
            let started_at = OffsetDateTime::now_utc().unix_timestamp();
            let t0 = Instant::now();
            let run_log_id = self
                .store
                .record_workflow_run_start(bw_store::NewWorkflowRun {
                    workflow_id: spec.id,
                    workflow_name: &spec.name,
                    project_id: Some(p),
                    session_id: Some(session),
                    trigger,
                    started_at,
                    cron_task_id,
                    params_json: &params_json,
                })
                .await?;
            // A3: bind this round's run to the Issue it executes (RunIssue passes
            // Some; every other caller None). Every round of an issue-run is
            // bound, so `list_runs_for_issue` reads the whole loop back.
            if let Some(iid) = issue_id {
                self.store.set_run_issue(run_log_id, iid).await?;
            }
            last_run_log = run_log_id;

            // Execute this round's phase range: through the gate for a gated
            // workflow, or all phases for an ungated one. Outputs come back on
            // the return value; live events stream via the callback.
            let range_res = engine
                .run_phase_range(&spec, &ctx, start..range_end, baton.clone(), |e| {
                    forward_progress(&live, e)
                })
                .await;

            let finished_at = OffsetDateTime::now_utc().unix_timestamp();
            let duration_ms = t0.elapsed().as_millis() as i64;

            let outputs = match range_res {
                Ok(o) => o,
                Err(e) => {
                    // Honest executor failure — settle Failed, stop the loop.
                    self.store
                        .settle_workflow_run(
                            run_log_id,
                            RunStatus::Failed,
                            finished_at,
                            duration_ms,
                            0,
                            &e.to_string(),
                        )
                        .await?;
                    break LoopEnd::Failed(AppError::Engine(e.to_string()));
                }
            };

            // Persist this round's phase outputs as session messages (每阶段留痕).
            let phases_completed = outputs.len() as u32;
            for output in &outputs {
                self.store
                    .append_message(session, Role::Agent, &output.text)
                    .await?;
                self.emit(Event::SessionMessageAdded {
                    session,
                    role: Role::Agent,
                    text: output.text.clone(),
                });
            }

            // Ungated pipeline: this single round is the whole run.
            let Some(e_idx) = eval_idx else {
                self.store
                    .settle_workflow_run(
                        run_log_id,
                        RunStatus::Ok,
                        finished_at,
                        duration_ms,
                        phases_completed,
                        "",
                    )
                    .await?;
                let _ = live.send(Event::WorkflowDone);
                final_run_ok = true;
                break LoopEnd::Passed;
            };

            // Parse the gate's real verdict from its output (the range's last
            // phase). No parseable verdict = honest review failure, NEVER a
            // default pass (plan/12 §4).
            let eval_text = outputs.last().map(|o| o.text.clone()).unwrap_or_default();
            let Some(outcome) = parse_phase_outcome(&eval_text) else {
                let msg = format!(
                    "评审输出缺结构化裁决(阶段「{}」· 轮次 {round}/{max_iter}):{}",
                    spec.phases[e_idx].name,
                    review_tail(&eval_text)
                );
                self.store
                    .settle_workflow_run(
                        run_log_id,
                        RunStatus::Failed,
                        finished_at,
                        duration_ms,
                        phases_completed,
                        &msg,
                    )
                    .await?;
                break LoopEnd::Failed(AppError::Engine(msg));
            };

            match outcome.verdict {
                Verdict::Pass => {
                    // Gate passed. Run any phases AFTER the gate (built-ins have
                    // none) in order — a genuine pass proceeds — then settle Ok.
                    let mut total = phases_completed;
                    if e_idx + 1 < num_phases {
                        let tail_res = engine
                            .run_phase_range(
                                &spec,
                                &ctx,
                                (e_idx + 1)..num_phases,
                                Some(review_tail(&eval_text)),
                                |e| forward_progress(&live, e),
                            )
                            .await;
                        match tail_res {
                            Ok(tail) => {
                                for output in &tail {
                                    self.store
                                        .append_message(session, Role::Agent, &output.text)
                                        .await?;
                                    self.emit(Event::SessionMessageAdded {
                                        session,
                                        role: Role::Agent,
                                        text: output.text.clone(),
                                    });
                                }
                                total += tail.len() as u32;
                            }
                            Err(e) => {
                                self.store
                                    .settle_workflow_run(
                                        run_log_id,
                                        RunStatus::Failed,
                                        OffsetDateTime::now_utc().unix_timestamp(),
                                        t0.elapsed().as_millis() as i64,
                                        phases_completed,
                                        &e.to_string(),
                                    )
                                    .await?;
                                break LoopEnd::Failed(AppError::Engine(e.to_string()));
                            }
                        }
                    }
                    self.store
                        .settle_workflow_run(
                            run_log_id,
                            RunStatus::Ok,
                            OffsetDateTime::now_utc().unix_timestamp(),
                            t0.elapsed().as_millis() as i64,
                            total,
                            "",
                        )
                        .await?;
                    let _ = live.send(Event::WorkflowDone);
                    final_run_ok = true;
                    break LoopEnd::Passed;
                }
                Verdict::RejectToPhase(proposed) => {
                    // Effective reject target: a declared `reject_to_phase`
                    // (Static track) wins and the agent's proposal is IGNORED; an
                    // undeclared one (Dynamic track) honours the agent's proposal.
                    let target = match spec.phases[e_idx].reject_to_phase {
                        Some(t) => t as usize,
                        None => proposed as usize,
                    };
                    let reason = if outcome.reason.trim().is_empty() {
                        "评审未通过".to_string()
                    } else {
                        outcome.reason.clone()
                    };
                    // A reject target must be a real phase strictly before the
                    // gate (loop BACK, not forward/self). Anything else is an
                    // un-actionable verdict → honest failure (never guess).
                    if target >= num_phases || target > e_idx {
                        let msg = format!(
                            "评审打回目标越界(阶段索引 {target} / 共 {num_phases} 阶段 · 轮次 {round}/{max_iter}):{reason}"
                        );
                        self.store
                            .settle_workflow_run(
                                run_log_id,
                                RunStatus::Failed,
                                finished_at,
                                duration_ms,
                                phases_completed,
                                &msg,
                            )
                            .await?;
                        break LoopEnd::Failed(AppError::Engine(msg));
                    }
                    if round >= max_iter {
                        // Cap hit: never auto-Failed, never auto-Done. Settle this
                        // round Failed with the cap reason; hand a Blocked outcome
                        // up (a bound Issue is parked Blocked by the caller).
                        let cap_reason = format!("对抗循环 {round}/{max_iter} 仍未通过:{reason}");
                        self.store
                            .settle_workflow_run(
                                run_log_id,
                                RunStatus::Failed,
                                finished_at,
                                duration_ms,
                                phases_completed,
                                &cap_reason,
                            )
                            .await?;
                        break LoopEnd::Blocked(cap_reason);
                    }
                    // Loop back: settle this round Failed (deliverable rejected),
                    // carry the reject feedback forward as the next round's baton
                    // (the regenerating phase sees WHY), restart from the target.
                    let row_msg = format!(
                        "评审打回阶段「{}」(轮次 {round}/{max_iter}):{reason}",
                        spec.phases[target].name
                    );
                    self.store
                        .settle_workflow_run(
                            run_log_id,
                            RunStatus::Failed,
                            finished_at,
                            duration_ms,
                            phases_completed,
                            &row_msg,
                        )
                        .await?;
                    baton = Some(review_tail(&eval_text));
                    start = target;
                    round += 1;
                }
            }
        };

        // ── After the loop: change window + usage accounting, ONCE ──────────
        // Attributed to the LAST round's row (the one that produced the final
        // state). Runs on every terminal outcome (pass / block / honest failure)
        // — a failed run's partial real output is still real output. Doing this
        // once per issue-run (not per round) keeps agent win_rate / skill `uses`
        // honest: one real work item = one agent run, one skill use.
        let run_ok = final_run_ok;
        let run_log_id = last_run_log;
        if !heads_workspace.is_empty() {
            let head_after = evidence::head_commit(&heads_workspace).await.ok().flatten();
            self.store
                .set_run_heads(run_log_id, head_before, head_after)
                .await?;
        }
        for a in &spec.agents {
            self.store.record_agent_run_by_name(&a.name, run_ok).await?;
        }
        for s in &spec.skills {
            self.store.record_skill_use_by_name(&s.name).await?;
        }
        if !spec.agents.is_empty() {
            self.refresh_agents().await?;
            self.emit(Event::AgentsChanged);
        }
        if !spec.skills.is_empty() {
            self.refresh_skills().await?;
            self.emit(Event::SkillsChanged);
        }
        // Artifact reflux: scan the real workspace and register new file
        // versions against the final round's run. Scan errors are a 0-fresh
        // no-op — they never turn a settled run into an error.
        if !proj.workspace_path.trim().is_empty() {
            let stage_kind = spec
                .stage_ref
                .and_then(|n| StageKind::ALL.into_iter().find(|s| s.index() == n));
            if let Ok(fresh) = self
                .scan_and_register_artifacts(
                    p,
                    &proj.workspace_path,
                    Some(run_log_id),
                    stage_kind,
                    // A2: run-time issue归属 — the活's产物 bind to both run
                    // and issue so the Done edge's idempotent re-scan matches.
                    issue_id,
                )
                .await
            {
                if fresh > 0 {
                    self.emit(Event::ArtifactsRegistered { fresh });
                }
            }
        }

        match end {
            LoopEnd::Passed => Ok(RunOutcome::Completed),
            LoopEnd::Blocked(reason) => Ok(RunOutcome::BlockedAtCap { reason }),
            LoopEnd::Failed(err) => Err(err),
        }
    }

    /// T6 (plan/12 §3): resolve which Agent CLI executes an issue-run and
    /// what `tools` (AllowedTools) it declares. Only `RunIssue` has a
    /// concrete assignee to route by — an issue with no assignee, an
    /// assignee row that's since been deleted, or a blank `agent_cli`
    /// (the five built-in stage-role rows) all read back as the honest
    /// default: `"claude-code"` with no tools restriction, byte-for-byte
    /// every other caller's (`RunHubWorkflow`, cron, stage playbook without
    /// an issue) pre-T6 behavior.
    async fn resolve_agent_route(
        &self,
        issue_id: Option<IssueId>,
    ) -> Result<(String, Vec<String>), AppError> {
        const DEFAULT_CLI: &str = "claude-code";
        let Some(iid) = issue_id else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let Some(issue) = self.store.get_issue(iid).await? else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let Some(agent_id) = issue.assignee else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let Some(agent) = self.store.get_agent(agent_id).await? else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let cli = if agent.agent_cli.trim().is_empty() {
            DEFAULT_CLI.to_string()
        } else {
            agent.agent_cli.clone()
        };
        Ok((cli, agent.tools.clone()))
    }

    /// Resolve skill refs against the hub and render the non-empty bodies as
    /// a prompt block. Pure read; the honest empty string when nothing
    /// resolves. Capped so a pathological catalog can't drown the task.
    async fn skills_prompt_block(&self, refs: &[SkillRef]) -> Result<String, AppError> {
        const MAX_BLOCK_CHARS: usize = 6000;
        let catalog = self.store.list_skills().await?;
        let mut bodies = Vec::new();
        let mut total = 0usize;
        for r in refs {
            let Some(skill) = catalog
                .iter()
                .find(|s| s.name == r.name && !s.content.trim().is_empty())
            else {
                continue;
            };
            let chars = skill.content.chars().count();
            if total + chars > MAX_BLOCK_CHARS {
                break;
            }
            total += chars;
            bodies.push(skill.content.trim().to_string());
        }
        if bodies.is_empty() {
            return Ok(String::new());
        }
        Ok(format!(
            "\n\n## 技能(工作方法,来自技能库)\n{}\n",
            bodies.join("\n\n")
        ))
    }

    /// A3: render up to 3 distilled (compounded) skills for project `p` as a
    /// prompt block, same-stage preferred then proven-first (`uses` desc as the
    /// distill-time proxy — `SkillCard` carries no timestamp). Only skills with
    /// real `content` distilled from a Done issue in THIS project qualify.
    /// Returns `(prompt_block, skill_refs)`. The block carries the real content
    /// (injected into the prompt); the name-led refs are returned separately so
    /// the caller can put them on `spec.skills` and let `run_workflow_inner`'s
    /// usage accounting bump each one's `uses` — closing the compounding loop
    /// (a distilled skill used by a run → uses+1). Honest `(empty, [])` when the
    /// project has no compounded skill yet.
    async fn distilled_skills_block(
        &self,
        project: ProjectId,
        stage: StageKind,
    ) -> Result<(String, Vec<SkillRef>), AppError> {
        const MAX: usize = 3;
        let catalog = self.store.list_skills().await?;
        // (uses, same_stage, skill) — resolve each distilled skill back to its
        // origin issue's project+stage to scope the compounding to this project.
        let mut scored: Vec<(u32, bool, SkillCard)> = Vec::new();
        for s in catalog {
            let Some(iid) = s.distilled_from_issue else {
                continue;
            };
            let Some(issue) = self.store.get_issue(iid).await? else {
                continue;
            };
            if issue.project_id != project || s.content.trim().is_empty() {
                continue; // wrong project, or a content-less catalog reference
            }
            scored.push((s.uses, issue.stage == stage, s));
        }
        // Same-stage first, then proven-first; stable within ties.
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        let picked: Vec<&SkillCard> = scored.iter().take(MAX).map(|(_, _, s)| s).collect();
        if picked.is_empty() {
            return Ok((String::new(), Vec::new()));
        }
        let bodies: Vec<String> = picked
            .iter()
            .map(|s| format!("- {}：\n{}", s.name, s.content.trim()))
            .collect();
        let block = format!(
            "\n\n## 复利技能(本项目蒸馏,同阶段优先)\n{}\n",
            bodies.join("\n\n")
        );
        let refs: Vec<SkillRef> = picked
            .iter()
            .map(|s| SkillRef {
                name: s.name.clone(),
                def: s.desc.clone(),
                from: s.category.clone(),
            })
            .collect();
        Ok((block, refs))
    }

    /// A4: name of the machine-fed "完成 Issue 数" leading metric, seeded per
    /// project×stage with an EMPTY target so its signal stays Unknown (honest
    /// "no goal set") — never a fake green from a raw completion count.
    fn stage_done_metric_name() -> &'static str {
        "阶段完成 Issue 数"
    }

    /// A4: idempotently seed the per-stage "完成 Issue 数" leading metric (one
    /// per stage, empty target). By-name idempotent — a re-seed adds nothing,
    /// so Boot can backfill pre-A4 projects safely.
    async fn seed_stage_done_metrics(&self, project: ProjectId) -> Result<(), AppError> {
        let have: std::collections::HashSet<StageKind> = self
            .store
            .persisted_signals(project)
            .await?
            .metrics
            .into_iter()
            .filter(|m| m.name == Self::stage_done_metric_name())
            .filter_map(|m| m.stage_kind)
            .collect();
        for kind in StageKind::ALL {
            if have.contains(&kind) {
                continue;
            }
            self.store
                .upsert_metric(NewMetric {
                    id: MetricId::new(),
                    project_id: project,
                    role: MetricRole::Leading,
                    stage_kind: Some(kind),
                    name: Self::stage_done_metric_name().into(),
                    def: "本阶段已完成的 Issue 数(每次 Done 自动计数,机器源)".into(),
                    target_raw: String::new(),
                    amber: AmberBand::default(),
                    last_target: String::new(),
                    driver: String::new(),
                    pos: 100 + kind.index() as i64,
                })
                .await?;
        }
        Ok(())
    }

    /// A4: feed the stage's "完成 Issue 数" metric the current count of Done
    /// issues in that stage, but only when it changed (change-guard — same
    /// idempotency as a manual re-confirm). Machine source (Telemetry). The
    /// metric's empty target keeps its signal Unknown: a count is not a goal.
    async fn feed_stage_done_count(
        &self,
        project: ProjectId,
        stage: StageKind,
    ) -> Result<(), AppError> {
        self.seed_stage_done_metrics(project).await?;
        let done = self
            .store
            .list_issues(project, Some(stage), Some(IssueStatus::Done))
            .await?
            .len() as i64;
        let new_raw = done.to_string();
        let metric = self
            .store
            .persisted_signals(project)
            .await?
            .metrics
            .into_iter()
            .find(|m| m.name == Self::stage_done_metric_name() && m.stage_kind == Some(stage));
        let Some(m) = metric else {
            return Ok(()); // metric missing — honest no-op
        };
        if m.value_raw == new_raw {
            return Ok(()); // change-guard: no new fact
        }
        self.store
            .append_observation(m.id, SourceKind::Telemetry, &new_raw, now())
            .await?;
        self.store.recompute_signals(project, now()).await?;
        Ok(())
    }

    /// Run one connector's real probe. Returns `(healthy, honest detail)`;
    /// errors only on kinds that have no real probe (there is no fake
    /// "synced" for those) or store failures.
    async fn probe_connector(&mut self, c: &Connector) -> Result<(bool, String), AppError> {
        match c.kind.as_str() {
            CONNECTOR_KIND_GIT_REPO => {
                // The bound project's *current* workspace is the live truth;
                // `config` is the provisioning-time record / fallback.
                let workspace = match c.project_id {
                    Some(p) => self
                        .store
                        .get_project(p)
                        .await?
                        .map(|proj| proj.workspace_path)
                        .filter(|w| !w.trim().is_empty())
                        .unwrap_or_else(|| c.config.clone()),
                    None => c.config.clone(),
                };
                match evidence::collect(&workspace).await {
                    Ok(ev) => {
                        // Tier D for real: the probe's numbers flow into the
                        // bound project's matching metrics as machine-source
                        // observations (only when the metric exists and the
                        // value really changed — no observation spam).
                        if let Some(p) = c.project_id {
                            self.feed_workspace_metrics(p, &ev).await?;
                        }
                        Ok((
                            true,
                            format!(
                                "{} 提交 · {} 追踪文件 · {} 文档 · {} 未提交路径",
                                ev.commit_count, ev.tracked_files, ev.docs_files, ev.dirty_paths
                            ),
                        ))
                    }
                    Err(e) => Ok((false, e.to_string())),
                }
            }
            CONNECTOR_KIND_CLAUDE_CLI => {
                let binary = if c.config.trim().is_empty() {
                    self.state
                        .claude_config
                        .binary
                        .clone()
                        .unwrap_or_else(|| "claude".into())
                } else {
                    c.config.trim().to_string()
                };
                match claude_version_probe(&binary).await {
                    Ok(v) => Ok((true, v)),
                    Err(e) => Ok((false, e)),
                }
            }
            other => Err(AppError::Invalid(format!(
                "连接器类型「{other}」没有真实探针——不支持同步(诚实拒绝,不伪造状态)"
            ))),
        }
    }

    /// Feed a real workspace evidence reading into the project's matching
    /// metrics (`METRIC_WS_COMMITS` / `METRIC_WS_DOCS`) as
    /// `SourceKind::Connector` observations. Metrics the project hasn't
    /// defined are skipped — the kernel never invents a metric (targets are
    /// human intent, not machine output).
    async fn feed_workspace_metrics(
        &mut self,
        project: ProjectId,
        ev: &evidence::WorkspaceEvidence,
    ) -> Result<(), AppError> {
        let sigs = self.store.persisted_signals(project).await?;
        let mut touched = false;
        for (name, value) in [
            (METRIC_WS_COMMITS, ev.commit_count.to_string()),
            (METRIC_WS_DOCS, ev.docs_files.to_string()),
        ] {
            let Some(m) = sigs.metrics.iter().find(|m| m.name == name) else {
                continue;
            };
            if m.value_raw == value {
                continue; // unchanged — a re-probe is not a new fact
            }
            self.store
                .append_observation(m.id, SourceKind::Connector, &value, now())
                .await?;
            touched = true;
        }
        if touched {
            self.store.recompute_signals(project, now()).await?;
            self.emit(Event::ProjectUpdated(project));
        }
        Ok(())
    }

    /// Scan `workspace` (real `git ls-files` + `stat` + short HEAD) and
    /// register every tracked file as an artifact version. Idempotent at the
    /// store layer — returns only the genuinely-new count.
    async fn scan_and_register_artifacts(
        &self,
        project: ProjectId,
        workspace: &str,
        workflow_run_id: Option<WorkflowRunId>,
        stage_kind: Option<StageKind>,
        issue_id: Option<IssueId>,
    ) -> Result<u32, AppError> {
        let files = evidence::list_workspace_files(workspace)
            .await
            .map_err(|e| AppError::Invalid(e.to_string()))?;
        if files.is_empty() {
            return Ok(0);
        }
        let commit = evidence::head_commit(workspace)
            .await
            .map_err(|e| AppError::Invalid(e.to_string()))?
            .unwrap_or_default();
        let registered_at = now().unix_timestamp();
        let items = files
            .into_iter()
            .map(|f| NewArtifact {
                id: ArtifactId::new(),
                project_id: project,
                workflow_run_id,
                issue_id,
                stage_kind,
                kind: classify_artifact_path(&f.path),
                path: f.path,
                bytes: f.bytes,
                git_commit: commit.clone(),
                registered_at,
            })
            .collect();
        Ok(self.store.register_artifacts(items).await?)
    }

    /// The real scheduler tick — call this on an interval (see
    /// `app-desktop/src/kernel.rs`) to really auto-fire due cron tasks, no
    /// click required. Reads cron tasks + hub specs fresh from the store
    /// (never trusts a possibly-stale in-memory snapshot for a decision this
    /// consequential), fires each task whose `bw_core::model::cron_due` says
    /// yes, and returns which ones fired — `[]` on a quiet tick, which is
    /// the common case and not an error.
    ///
    /// Deliberately does **not** touch `self.state.active_project`/`view`/
    /// `panel`/`scope`/`active_session`: unlike the desktop UI's manual "▶
    /// 立即执行" (which *does* navigate the caller to go watch, because a
    /// human just asked for that), an unattended background fire must not
    /// yank whatever project/screen the user currently has open. Real
    /// "monitoring" here means `Event::CronAutoFired` + the cron row's own
    /// persisted status/`last_run`, not a hijacked view.
    ///
    /// One task failing (a real `run_workflow_inner` error) is recorded as
    /// `CronStatus::Failed` and does not stop the rest of this tick from
    /// evaluating the remaining tasks.
    pub async fn tick_scheduler(&mut self) -> Result<Vec<CronTaskId>, AppError> {
        let now_ts = now();
        let tasks = self.store.list_cron_tasks().await?;
        let specs = self.store.list_workflow_specs().await?;
        let mut fired = Vec::new();

        for c in tasks {
            if c.status != CronStatus::Normal {
                continue; // Paused/Running/Failed never auto-fire — pause is real human intervention, honored here.
            }
            let Some(pid) = c.project_id else {
                continue; // "全部项目" tasks can't resolve a single project to run in — same rule the manual trigger's `can_run` check uses.
            };
            if !cron_due(&c.schedule, c.last_run_at, now_ts) {
                continue;
            }

            // A1: autopilot — a create_issue task mints a stage-scoped Issue
            // (Todo, optionally assigned) instead of running a workflow. No-hijack
            // by construction: this branch never calls run_workflow_inner.
            if c.mode == CronMode::CreateIssue {
                let Some(stage) = c.issue_stage else {
                    continue; // misconfigured — no stage to scope the Issue to
                };
                self.store
                    .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                    .await?;
                let res = self
                    .autopilot_fire(pid, &c.name, stage, c.issue_assignee.as_deref(), now_ts)
                    .await;
                let (ok, status) = match &res {
                    Ok(_) => (true, CronStatus::Normal),
                    Err(_) => (false, CronStatus::Failed),
                };
                self.store
                    .record_cron_run(c.id, status, run_at_label(now()))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.refresh_issues().await?;
                self.emit(Event::CronTasksChanged);
                self.emit(Event::IssuesChanged);
                self.emit(Event::CronAutoFired {
                    id: c.id,
                    name: c.name.clone(),
                    ok,
                });
                fired.push(c.id);
                continue;
            }

            // T10 (plan/12 §5): RunSkill / RunPrompt — both really execute,
            // through the identical run_workflow_inner engine/executor path
            // the RunWorkflow branch below uses, against a single ad-hoc
            // prompt (`cron_prompt_workflow`) instead of a real hub
            // workflow's phases. Resolved fresh on every fire, never cached:
            // a RunSkill task whose skill was deleted since creation fails
            // honestly right here — no crash, no silent no-op, no fabricated
            // success.
            let adhoc_spec = match &c.mode {
                CronMode::RunSkill { skill_id } => {
                    Some(match self.store.get_skill(*skill_id).await? {
                        Some(skill) => Ok(cron_prompt_workflow(
                            format!("⚙ 定时技能 · {}", skill.name),
                            skill.content.clone(),
                        )),
                        None => Err("引用的技能已删除".to_string()),
                    })
                }
                CronMode::RunPrompt { prompt } => Some(Ok(cron_prompt_workflow(
                    format!("💬 定时 Prompt · {}", c.name),
                    prompt.clone(),
                ))),
                _ => None,
            };

            if let Some(adhoc_spec) = adhoc_spec {
                self.store
                    .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);

                let ok = match adhoc_spec {
                    Err(reason) => {
                        // Nothing to execute (referenced skill gone) — an
                        // honest failed fire, recorded as a real workflow_run
                        // row so CronEffectiveness reflects it exactly like an
                        // engine failure would (settle-once, never a
                        // fabricated success or a silently skipped fire).
                        let started_at = OffsetDateTime::now_utc().unix_timestamp();
                        let run_id = self
                            .store
                            .record_workflow_run_start(bw_store::NewWorkflowRun {
                                workflow_id: WorkflowId::new(),
                                workflow_name: &c.name,
                                project_id: Some(pid),
                                session_id: None,
                                trigger: RunTrigger::Scheduled,
                                started_at,
                                cron_task_id: Some(c.id),
                                params_json: "",
                            })
                            .await?;
                        self.store
                            .settle_workflow_run(
                                run_id,
                                RunStatus::Failed,
                                OffsetDateTime::now_utc().unix_timestamp(),
                                0,
                                0,
                                &reason,
                            )
                            .await?;
                        false
                    }
                    Ok(spec) => {
                        let session = SessionId::new();
                        self.store
                            .ensure_session(NewSession {
                                id: session,
                                project_id: pid,
                                stage_kind: None,
                                kind: SessionKind::Optimize,
                                title: format!("⏰ 定时触发 · {}", c.name),
                                snippet: String::new(),
                            })
                            .await?;
                        let result = self
                            .run_workflow_inner(
                                pid,
                                session,
                                spec,
                                RunTrigger::Scheduled,
                                Some(c.id),
                                None,
                            )
                            .await;
                        matches!(result, Ok(RunOutcome::Completed))
                    }
                };

                let outcome = if ok {
                    CronStatus::Normal
                } else {
                    CronStatus::Failed
                };
                self.store
                    .record_cron_run(c.id, outcome, run_at_label(now()))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
                self.emit(Event::CronAutoFired {
                    id: c.id,
                    name: c.name.clone(),
                    ok,
                });
                fired.push(c.id);
                continue;
            }

            let Some(spec) = specs.iter().find(|w| w.name == c.target).cloned() else {
                continue; // target doesn't (yet) name a real hub workflow — same rule as the manual trigger.
            };

            self.store
                .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                .await?;
            self.refresh_cron_tasks().await?;
            self.emit(Event::CronTasksChanged);

            let session = SessionId::new();
            self.store
                .ensure_session(NewSession {
                    id: session,
                    project_id: pid,
                    stage_kind: spec
                        .stage_ref
                        .and_then(|n| StageKind::ALL.into_iter().find(|s| s.index() == n)),
                    kind: SessionKind::Optimize,
                    title: format!("⏰ 定时触发 · {}", c.name),
                    snippet: String::new(),
                })
                .await?;
            self.store.record_workflow_use(spec.id).await?;
            self.refresh_workflow_specs().await?;

            let result = self
                .run_workflow_inner(pid, session, spec, RunTrigger::Scheduled, Some(c.id), None)
                .await;
            // A scheduled run "succeeds" only when the workflow actually passed.
            // A hit review cap (`BlockedAtCap`) has no bound Issue to park here
            // (cron RunWorkflow passes `issue_id = None`) — its honest per-round
            // rows already record "上限未通过", so we surface it as Failed rather
            // than a fake green (no fabricated Issue — plan/12 §4).
            let ok = matches!(result, Ok(RunOutcome::Completed));
            let outcome = if ok {
                CronStatus::Normal
            } else {
                CronStatus::Failed
            };
            self.store
                .record_cron_run(c.id, outcome, run_at_label(now()))
                .await?;
            self.refresh_cron_tasks().await?;
            self.emit(Event::CronTasksChanged);
            self.emit(Event::CronAutoFired {
                id: c.id,
                name: c.name.clone(),
                ok,
            });

            fired.push(c.id);
        }
        Ok(fired)
    }

    /// A1: the create_issue cron path — mint a stage-scoped Issue (Todo,
    /// optionally assigned by name). No-hijack: never runs a workflow. A missing
    /// named agent is an honest unassigned Issue, not a failure.
    async fn autopilot_fire(
        &mut self,
        project: ProjectId,
        name: &str,
        stage: StageKind,
        assignee: Option<&str>,
        fired_at: OffsetDateTime,
    ) -> Result<IssueId, AppError> {
        let issue_id = IssueId::new();
        self.store
            .create_issue(NewIssue {
                id: issue_id,
                project_id: project,
                stage,
                title: format!("[auto] {name}"),
                desc: format!(
                    "Autopilot 建单(定时任务「{name}」于 {} 触发,{} 阶段)。",
                    run_at_label(fired_at),
                    stage.label()
                ),
                priority: IssuePriority::Medium,
            })
            .await?;
        // Todo (committed work), not Backlog (the parking lot) — autopilot建单
        // is a commitment, and Backlog is the suppress-firing pile in multica.
        self.store
            .transition_issue(issue_id, IssueStatus::Todo)
            .await?;
        // Assign by name if the named agent exists — honest 0-match otherwise.
        if let Some(agent_name) = assignee {
            if let Some(agent) = self
                .store
                .list_agents()
                .await?
                .into_iter()
                .find(|a| a.name == agent_name)
            {
                self.store.assign_issue(issue_id, Some(agent.id)).await?;
            }
        }
        Ok(issue_id)
    }

    /// **The self-driving optimization loop (iter 18).** Runs the full
    /// measure→propose→gate cycle over every hub workflow, once. This is the
    /// engine the goal asked for: "通过不断的执行 schedule 的 workflow 来优化
    /// workflow 本身" — a cron task can fire this on a cadence (iter 22 wires
    /// that) so the hub keeps optimizing *itself* without a click.
    ///
    /// What it does, per workflow:
    ///   1. **Measure** — fetch real analytics + usage rank + the run log +
    ///      cron effectiveness (every number read from the store, none
    ///      invented).
    ///   2. **Propose** — `analysis::propose_optimizations` turns the evidence
    ///      into ranked, grounded suggestions.
    ///   3. **Gate** — `analysis::review_proposal` decides AutoApply /
    ///      DeferToHuman / Reject under the default policy (the autonomy dial).
    ///      Only the *positive* kind auto-applies; everything content-changing
    ///      or destructive defers to a human.
    ///   4. **Report** — returns what was considered, what was auto-applied,
    ///      what needs a human. Emits `OptimizationCycleReported`.
    ///
    /// It deliberately does **not** rewrite specs or retire workflows on its
    /// own — that's the safety design from iter 13. The loop's autonomy is
    /// bounded: it measures relentlessly, proposes honestly, and acts only on
    /// the safe-positive.
    pub async fn run_optimization_cycle(&mut self) -> Result<OptimizationReport, AppError> {
        use bw_core::analysis::{propose_optimizations, review_proposal, ApplyPolicy};

        let policy = ApplyPolicy::default();
        let specs = self.store.list_workflow_specs().await?;
        let ranking = self.store.hub_usage_ranking().await?;
        let cron_tasks = self.store.list_cron_tasks().await?;
        let mut scanned = 0u32;
        let mut proposals = 0u32;
        let mut auto_applied = Vec::new();
        let mut defer_to_human = Vec::new();
        let mut rejected = 0u32;

        for spec in &specs {
            scanned += 1;
            let mut analytics = self.store.workflow_analytics(spec.id).await?;
            // A cold workflow has no runs, so analytics.workflow_name reads
            // back empty — fill it from the spec so proposals name it honestly.
            if analytics.workflow_name.is_empty() {
                analytics.workflow_name = spec.name.clone();
            }
            let usage = ranking
                .iter()
                .find(|r| r.workflow_id == spec.id)
                .cloned()
                .unwrap_or_else(|| bw_core::model::UsageRank {
                    workflow_id: spec.id,
                    workflow_name: spec.name.clone(),
                    stage_ref: spec.stage_ref,
                    total_runs: 0,
                    ok_runs: 0,
                    failed_runs: 0,
                    success_rate: None,
                    last_run_at: None,
                    cold: true,
                });
            let runs = self.store.list_workflow_runs(spec.id).await?;
            let failures = bw_core::analysis::failure_modes(&runs);
            // Cron effectiveness: a task targeting this workflow contributes
            // its real scheduled-fire track record to the proposal inputs.
            let cron_eff = match cron_tasks.iter().find(|c| c.target == spec.name) {
                Some(c) => Some(self.store.cron_effectiveness(c.id).await?),
                None => None,
            };
            let ps = propose_optimizations(&analytics, &usage, &failures, cron_eff.as_ref());
            for p in ps {
                proposals += 1;
                let settled = analytics.ok_runs + analytics.failed_runs;
                match review_proposal(&p, settled, &policy) {
                    bw_core::analysis::ApplyDecision::AutoApply => {
                        auto_applied.push(p.title);
                    }
                    bw_core::analysis::ApplyDecision::DeferToHuman(_) => {
                        defer_to_human.push(p.title);
                    }
                    bw_core::analysis::ApplyDecision::Reject(_) => {
                        rejected += 1;
                    }
                }
            }
        }
        let report = OptimizationReport {
            scanned,
            proposals,
            auto_applied,
            defer_to_human,
            rejected,
        };
        self.emit(Event::OptimizationCycleReported {
            report: report.clone(),
        });
        Ok(report)
    }

    pub async fn dispatch(&mut self, cmd: Command) -> Result<(), AppError> {
        match cmd {
            Command::Boot => {
                // Staleness is clock-relative: what was green last week may be
                // amber-capped today. Re-derive every running project on boot so
                // the wall never shows a stale cache as fresh truth.
                let projects = self.store.list_projects().await?;
                for p in &projects {
                    if p.phase == ProjectPhase::Running {
                        self.store.recompute_signals(p.id, now()).await?;
                    }
                }
                self.refresh_projects().await?;
                // Real OMC/ECC catalog, not fabricated sample data — a no-op
                // once the hub tables are non-empty (checked inside).
                bw_store::seed_hub_if_empty(self.store.as_ref()).await?;
                // The five stage-role agents + stage working-method skills
                // (bw_core::playbook projections) — by-name idempotent, so an
                // already-seeded database gains them too.
                bw_store::seed_stage_entities_if_missing(self.store.as_ref()).await?;
                // A4: backfill the per-stage "完成 Issue 数" metric for every
                // project — pre-A4 projects gain it; already-seeded ones are
                // unchanged (by-name idempotent).
                for p in &projects {
                    self.seed_stage_done_metrics(p.id).await?;
                }
                self.refresh_workflow_specs().await?;
                self.refresh_skills().await?;
                self.refresh_agents().await?;
                self.refresh_cron_tasks().await?;
                self.refresh_connectors().await?;
                self.refresh_knowledge_sources().await?;
                self.refresh_activity().await?;
                self.refresh_issues().await?;
                self.emit(Event::ProjectsChanged);
            }

            Command::CreateProject {
                id,
                name,
                kind,
                desc,
                workspace,
            } => {
                self.store
                    .create_project(NewProject {
                        id,
                        name,
                        kind,
                        desc,
                    })
                    .await?;
                self.state.active_project = Some(id);
                self.state.view = View::Create;
                // P1: 建项目即建仓 —— 出生那一刻仓就存在(而非走完创建流才有)。
                // 绑定已有仓:只校验含 .git,绝不动原文件;新建仓在 workspaces_root
                // 下 mint,失败沿用既有降级(项目以 Mock 模式活着,创建本身不破)。
                let bound = workspace
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                match bound {
                    Some(path) => {
                        if !std::path::Path::new(path).join(".git").exists() {
                            return Err(AppError::Invalid(format!(
                                "绑定的工作目录不是 git 仓库(无 .git):{path}"
                            )));
                        }
                        self.store.set_workspace(id, path, true).await?;
                    }
                    None => {
                        if let Some(root) = self.workspaces_root.clone() {
                            match provision_workspace(&root, &proj).await {
                                Ok(path) => {
                                    self.store.set_workspace(id, &path, true).await?;
                                    self.store
                                        .create_connector(NewConnector {
                                            id: ConnectorId::new(),
                                            name: format!("{} · 代码仓", proj.name),
                                            kind: CONNECTOR_KIND_GIT_REPO.into(),
                                            scope: proj.name.clone(),
                                            project_id: Some(id),
                                            config: path.clone(),
                                        })
                                        .await?;
                                }
                                Err(e) => {
                                    self.emit(Event::ConnectorSynced {
                                        name: format!("{} · 代码仓", proj.name),
                                        ok: false,
                                        detail: format!("自动开仓失败,项目将以 Mock 模式运行:{e}"),
                                    });
                                }
                            }
                        }
                    }
                }
                // 章程开篇(仅 owned 仓写;bound 仓尊重「不动原文件」)。
                let _ = write_charter(self, id, "开篇").await;
                // 模板能力(用户 2026-07-20 拍板):四份组件标准文件写进仓里,
                // 供人与 agent 之后在这个项目里创建 agent/skill/workflow/cron 时
                // 对照(同一 owned-workspace 门槛,一次性,不随创建流逐步改写)。
                let _ = write_component_standards(self, id).await;
                self.refresh_projects().await?;
                self.refresh_connectors().await?;
                self.emit(Event::ProjectsChanged);
                self.emit(Event::ViewChanged(View::Create));
            }

            Command::SetCycle { cycle } => {
                let p = self.active()?;
                self.store.set_project_cycle(p, cycle).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpdateBrief {
                benchmark,
                opportunity,
            } => {
                let p = self.active()?;
                self.store.set_brief(p, &benchmark, &opportunity).await?;
                let _ = write_charter(self, p, "定位与机会").await;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpdateNorthStar { value, def } => {
                let p = self.active()?;
                self.store.set_north_star(p, &value, &def).await?;
                let _ = write_charter(self, p, "北极星").await;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpsertManualMetric {
                id,
                name,
                def,
                role,
                stage_kind,
                target,
                amber,
                value,
            } => {
                let p = self.active()?;
                // Idempotency guard: re-confirming a step must not mint a
                // duplicate observation — only a *changed* value is a new fact.
                let latest = self
                    .store
                    .persisted_signals(p)
                    .await?
                    .metrics
                    .into_iter()
                    .find(|m| m.id == id)
                    .map(|m| m.value_raw);
                self.store
                    .upsert_metric(NewMetric {
                        id,
                        project_id: p,
                        role,
                        stage_kind,
                        name,
                        def,
                        target_raw: target,
                        amber,
                        last_target: String::new(),
                        driver: String::new(),
                        pos: 0,
                    })
                    .await?;
                // The value is born as an explicit Manual observation; the signal
                // it implies is computed later by recompute, never set here.
                let value = value.trim();
                if !value.is_empty() && latest.as_deref() != Some(value) {
                    self.store
                        .append_observation(id, SourceKind::Manual, value, now())
                        .await?;
                }
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpdateWeekPlan {
                metric,
                new_target,
                last_target,
                driver,
            } => {
                let p = self.active()?;
                self.store
                    .update_week_plan(metric, &new_target, &last_target, &driver)
                    .await?;
                // The target moved ⇒ the same value may now mean a different
                // signal. Re-derive; never patch by hand.
                self.store.recompute_signals(p, now()).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::RecordObservation { metric, value } => {
                let p = self.active()?;
                let value = value.trim();
                if value.is_empty() {
                    return Err(AppError::Invalid("观测值不能为空".into()));
                }
                self.store
                    .append_observation(metric, SourceKind::Manual, value, now())
                    .await?;
                self.store.recompute_signals(p, now()).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::RecordCollectedObservation {
                metric,
                value,
                source,
            } => {
                let p = self.active()?;
                let value = value.trim();
                if value.is_empty() {
                    return Err(AppError::Invalid("观测值不能为空".into()));
                }
                if matches!(source, SourceKind::Manual) {
                    // A hand-typed value must go through `RecordObservation`
                    // and wear its `手填` badge — letting a caller stamp
                    // `Manual` here would blur the one line this command
                    // exists to draw (machine-measured vs hand-entered).
                    return Err(AppError::Invalid(
                        "机器采集观测不能标记为 Manual——请走 RecordObservation".into(),
                    ));
                }
                self.store
                    .append_observation(metric, source, value, now())
                    .await?;
                self.store.recompute_signals(p, now()).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::SetStageProgress {
                stage_kind,
                progress,
            } => {
                let p = self.active()?;
                self.store
                    .set_stage_progress(p, stage_kind, progress)
                    .await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::ToggleDod { stage_kind, index } => {
                let p = self.active()?;
                self.store.toggle_dod(p, stage_kind, index).await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::HandoffStage { risky, note } => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                let from = proj.active_stage;
                let to = from.next();
                // A4: leaving a stage with unfinished (non-terminal) issues is a
                // risky handoff by definition — force it honest + tag the note,
                // so open work can't slip silently into the next stage.
                let open_in_stage = self
                    .store
                    .list_issues(p, Some(from), None)
                    .await?
                    .iter()
                    .filter(|i| !i.status.is_terminal())
                    .count();
                let (risky, note) = if open_in_stage > 0 {
                    let tag = format!("留 {} 件未完 Issue;", open_in_stage);
                    let note = if note.trim().is_empty() {
                        tag
                    } else {
                        format!("{tag} {note}")
                    };
                    (true, note)
                } else {
                    (risky, note)
                };
                self.store
                    .handoff_stage(p, from, to, risky, &note, now())
                    .await?;
                self.refresh_projects().await?;
                self.refresh_activity().await?;
                self.emit(Event::StageHandoff { from, to, risky });
                self.emit(Event::ProjectUpdated(p));
                self.emit(Event::ActivityChanged);
            }

            Command::CompleteCreation { cadence } => {
                let p = self.active()?;
                self.store
                    .set_project_phase(p, ProjectPhase::Running)
                    .await?;
                self.store
                    .materialize_stages(five_stages(p, cadence))
                    .await?;
                // A4: seed the per-stage "完成 Issue 数" leading metric (empty
                // target ⇒ honest Unknown) so Done-edge feeds have a home. The
                // recompute at the end of CompleteCreation derives its signal.
                self.seed_stage_done_metrics(p).await?;
                // All-in-one-codebase default: a project completing creation
                // gets its own real git repo (when a workspaces root is
                // configured and no workspace was set by hand), plus a bound
                // `git-repo` connector. Provisioning failure degrades to the
                // old Mock-only behavior — creation itself never breaks.
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                if self.workspaces_root.is_some() && proj.workspace_path.trim().is_empty() {
                    let root = self.workspaces_root.clone().expect("checked above");
                    match provision_workspace(&root, &proj).await {
                        Ok(path) => {
                            self.store.set_workspace(p, &path, true).await?;
                            self.store
                                .create_connector(NewConnector {
                                    id: ConnectorId::new(),
                                    name: format!("{} · 代码仓", proj.name),
                                    kind: CONNECTOR_KIND_GIT_REPO.into(),
                                    scope: proj.name.clone(),
                                    project_id: Some(p),
                                    config: path.clone(),
                                })
                                .await?;
                            self.refresh_connectors().await?;
                            self.emit(Event::ConnectorsChanged);
                        }
                        Err(e) => {
                            // Loud, honest degradation — never a silent fake.
                            self.emit(Event::ConnectorSynced {
                                name: format!("{} · 代码仓", proj.name),
                                ok: false,
                                detail: format!("自动开仓失败,项目将以 Mock 模式运行:{e}"),
                            });
                        }
                    }
                }
                self.store.recompute_signals(p, now()).await?;
                let _ = write_charter(self, p, "完成创建").await;
                self.state.view = View::App;
                self.refresh_projects().await?;
                self.emit(Event::ProjectUpdated(p));
                self.emit(Event::ViewChanged(View::App));
            }

            Command::SetWorkspace {
                path,
                allow_commands,
            } => {
                let p = self.active()?;
                let trimmed = path.trim();
                if !trimmed.is_empty() && !std::path::Path::new(trimmed).is_dir() {
                    return Err(AppError::Invalid(format!("工作目录不存在:{trimmed}")));
                }
                self.store.set_workspace(p, trimmed, allow_commands).await?;
                self.refresh_projects().await?;
                self.emit(Event::ProjectUpdated(p));
            }

            Command::SetClaudeConfig {
                binary,
                max_budget_usd,
                default_mode,
                commands_mode,
            } => {
                if max_budget_usd <= 0.0 {
                    return Err(AppError::Invalid("预算上限必须大于 0".into()));
                }
                self.state.claude_config = ClaudeCliConfig {
                    binary,
                    max_budget_usd,
                    default_mode,
                    commands_mode,
                };
                self.emit(Event::ClaudeConfigChanged);
            }

            Command::LoadVersionLog => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                let result = bw_engine::read_commits(&proj.workspace_path, 30)
                    .await
                    .map_err(|e| e.to_string());
                self.state.version_log = Some((p, result));
                self.emit(Event::VersionLogChanged);
            }

            Command::LoadArtifacts => {
                let p = self.active()?;
                let rows = self.store.list_artifacts(p).await?;
                self.state.artifacts = Some((p, rows));
                self.emit(Event::ArtifactsChanged);
            }

            // L1(plan/11): a real backend function (`cron_effectiveness`)
            // that has existed since the cron-run-attribution work landed but
            // never had a caller — this is that caller.
            Command::LoadCronEffectiveness(id) => {
                let e = self.store.cron_effectiveness(id).await?;
                self.state.cron_effectiveness = Some((id, e));
                self.emit(Event::CronEffectivenessChanged);
            }

            // P4: assemble one Issue's evidence — its runs, what each run
            // really changed (diff between the recorded HEAD pair), and its
            // registered artifacts. Read-only; every number the overlay shows
            // comes from the store / git, nothing synthesized here.
            Command::OpenIssueDetail(id) => {
                let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                let runs = self.store.list_runs_for_issue(id).await?;
                let artifacts = self.store.list_artifacts_for_issue(id).await?;
                let workspace = self
                    .store
                    .get_project(issue.project_id)
                    .await?
                    .map(|p| p.workspace_path.trim().to_string())
                    .unwrap_or_default();
                let mut changes = Vec::with_capacity(runs.len());
                for r in &runs {
                    let entry = match (&r.head_before, &r.head_after) {
                        (Some(b), Some(a)) if !workspace.is_empty() => {
                            if b == a {
                                // A real run that committed nothing — an
                                // honest empty list, not an error.
                                Ok(Vec::new())
                            } else {
                                bw_engine::workspace::diff_numstat(&workspace, b, a)
                                    .await
                                    .map(|v| {
                                        v.into_iter()
                                            .map(|c| (c.path, c.added, c.deleted))
                                            .collect::<Vec<_>>()
                                    })
                                    .map_err(|e| format!("对比不可用:{e}"))
                            }
                        }
                        _ => Err("无变更记录(演示模式运行,或早于变更追踪)".to_string()),
                    };
                    changes.push((r.id, entry));
                }
                self.state.issue_detail = Some(IssueDetailData {
                    issue,
                    runs,
                    changes,
                    artifacts,
                });
            }

            Command::CloseIssueDetail => {
                self.state.issue_detail = None;
            }

            Command::CollectArtifacts => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                if proj.workspace_path.trim().is_empty() {
                    return Err(AppError::Invalid(
                        "未配置真实工作区——没有可扫描的代码仓".into(),
                    ));
                }
                let fresh = self
                    .scan_and_register_artifacts(p, &proj.workspace_path, None, None, None)
                    .await?;
                self.emit(Event::ArtifactsRegistered { fresh });
                // Refresh the panel snapshot in the same dispatch so the UI
                // sees the scan's result without a second command.
                let rows = self.store.list_artifacts(p).await?;
                self.state.artifacts = Some((p, rows));
                self.emit(Event::ArtifactsChanged);
            }

            Command::SyncConnector { id } => {
                let all = self.store.list_connectors().await?;
                let c = all
                    .into_iter()
                    .find(|c| c.id == id)
                    .ok_or(AppError::NotFound)?;
                let (ok, detail) = self.probe_connector(&c).await?;
                let status = if ok {
                    ConnectorStatus::Connected
                } else {
                    ConnectorStatus::Error
                };
                self.store
                    .set_connector_sync(id, status, &run_at_label(now()))
                    .await?;
                self.refresh_connectors().await?;
                self.emit(Event::ConnectorsChanged);
                self.emit(Event::ConnectorSynced {
                    name: c.name.clone(),
                    ok,
                    detail,
                });
            }

            Command::StartSession {
                id,
                stage_kind,
                kind,
                title,
            } => {
                let p = self.active()?;
                self.store
                    .ensure_session(NewSession {
                        id,
                        project_id: p,
                        stage_kind,
                        kind,
                        title,
                        snippet: String::new(),
                    })
                    .await?;
                self.state.active_session = Some(id);
            }

            Command::RunWorkflow { session, spec } => {
                let p = self.active()?;
                self.run_workflow_inner(p, session, spec, RunTrigger::Manual, None, None)
                    .await?;
            }

            Command::RunStagePlaybook {
                session,
                stage_kind,
            } => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                // The baton this stage received — the latest real handoff
                // note (empty on a project's very first stage).
                // `list_handoffs` is newest-first (ORDER BY created_at DESC),
                // so the latest note is `.first()`.
                let handoff_note = self
                    .store
                    .list_handoffs(p)
                    .await?
                    .first()
                    .map(|h| h.note.clone())
                    .unwrap_or_default();
                let workspace_hint = if proj.workspace_path.trim().is_empty() {
                    "（未配置真实工作区 —— 本次运行在 MockExecutor 上，产出仅为流程演示）"
                        .to_string()
                } else {
                    format!(
                        "工作区 {}（git 仓库）。请在其中完成一切产出；之前阶段的产出也在这里，先查看现状再动手。",
                        proj.workspace_path.trim()
                    )
                };
                let ctx = bw_core::playbook::PlaybookCtx {
                    project_name: proj.name.clone(),
                    project_kind: proj.kind.clone(),
                    project_desc: proj.desc.clone(),
                    benchmark: proj.benchmark.clone(),
                    opportunity: proj.opportunity.clone(),
                    north_star: proj.north_star.clone(),
                    ns_def: proj.ns_def.clone(),
                    handoff_note,
                    workspace_hint,
                };
                let spec = stage_workflow_with_playbook(stage_kind, &ctx);
                self.run_workflow_inner(p, session, spec, RunTrigger::Manual, None, None)
                    .await?;
            }

            Command::RefreshHubs => {
                self.refresh_workflow_specs().await?;
                self.refresh_skills().await?;
                self.refresh_agents().await?;
            }

            Command::CreateWorkflowSpec {
                id,
                name,
                prompt,
                goal,
                stage_ref,
                phases,
                phase_prompts,
                agents,
                skills,
                loop_config,
                maturity,
                scope,
                source,
                trigger,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                if !phase_prompts.is_empty() && phase_prompts.len() != phases.len() {
                    return Err(AppError::Invalid(
                        "phase_prompts 必须为空或与 phases 等长".into(),
                    ));
                }
                self.store
                    .create_workflow_spec(NewWorkflowSpec {
                        id,
                        name,
                        kind: WorkflowKind::Static {
                            maturity,
                            version: 1,
                            uses: 0,
                            scope,
                            source,
                            trigger,
                        },
                        prompt,
                        goal,
                        stage_ref,
                        // The hub create form is still name-only text editing
                        // (no role-declaration UI yet) — every phase it
                        // authors is honestly `Neutral`. Built-in stage
                        // playbooks are the only source of real roles today
                        // (`bw_core::playbook::phase_metas`).
                        phases: phases.into_iter().map(PhaseMeta::neutral).collect(),
                        phase_prompts,
                        agents,
                        skills,
                        loop_config,
                        // 践行最小切片(2026-07-20):Command 层暂不带 project_id
                        // 参数(那是 P2 全量的事,见 plan/08 §0)——Hub 创建口径
                        // 不变,一律全局。
                        project_id: None,
                    })
                    .await?;
                self.refresh_workflow_specs().await?;
                self.emit(Event::WorkflowSpecsChanged);
            }

            Command::PromoteWorkflow {
                new_id,
                session,
                source,
            } => {
                let p = self.active()?;
                let sess = self
                    .store
                    .list_sessions(p)
                    .await?
                    .into_iter()
                    .find(|s| s.id == session)
                    .ok_or(AppError::NotFound)?;
                let spec = match sess.stage_kind {
                    Some(kind) => stage_workflow(kind),
                    None => {
                        return Err(AppError::Invalid("会话未关联阶段,无法沉淀".into()));
                    }
                };
                self.store.promote_workflow(new_id, &spec, source).await?;
                self.refresh_workflow_specs().await?;
                self.emit(Event::WorkflowSpecsChanged);
            }

            Command::RunHubWorkflow {
                session,
                workflow_id,
            } => {
                let p = self.active()?;
                let spec = self
                    .store
                    .get_workflow_spec(workflow_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                self.store.record_workflow_use(workflow_id).await?;
                self.refresh_workflow_specs().await?;
                self.run_workflow_inner(p, session, spec, RunTrigger::Manual, None, None)
                    .await?;
            }

            Command::RunIssue { session, id } => {
                let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                // A5-F: only work not yet settled/parked/under-review/blocked
                // can be (re)started this way. InProgress is a legal starting
                // point too — it's the retry path after an honest failure
                // (the issue stays InProgress on error, never faked forward).
                if !matches!(
                    issue.status,
                    IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
                ) {
                    return Err(AppError::Invalid(format!(
                        "#{} 处于{},不能直接运行",
                        issue.number,
                        issue.status.label()
                    )));
                }
                let p = issue.project_id;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;

                // Same stage-playbook scaffolding as RunStagePlaybook (fills the
                // role preamble + real project context), then the issue is
                // stamped on top so the agent runs its stage methodology against
                // THIS concrete work item.
                let handoff_note = self
                    .store
                    .list_handoffs(p)
                    .await?
                    .first()
                    .map(|h| h.note.clone())
                    .unwrap_or_default();
                let workspace_hint = if proj.workspace_path.trim().is_empty() {
                    "（未配置真实工作区 —— 本次运行在 MockExecutor 上,产出仅为流程演示）"
                        .to_string()
                } else {
                    format!(
                        "工作区 {}（git 仓库）。产出落于此;先查看现状再动手。",
                        proj.workspace_path.trim()
                    )
                };
                let ctx = bw_core::playbook::PlaybookCtx {
                    project_name: proj.name.clone(),
                    project_kind: proj.kind.clone(),
                    project_desc: proj.desc.clone(),
                    benchmark: proj.benchmark.clone(),
                    opportunity: proj.opportunity.clone(),
                    north_star: proj.north_star.clone(),
                    ns_def: proj.ns_def.clone(),
                    handoff_note,
                    workspace_hint,
                };
                let mut spec = stage_workflow_with_playbook(issue.stage, &ctx);
                let issue_brief = format!(
                    "\n\n## 本件活(Issue #{})\n标题:{}\n描述:{}\n请用本阶段方法论完成它,产出落为工作区真实文件。\n",
                    issue.number, issue.title, issue.desc
                );
                // Distilled (compounded) skills from this project, same-stage
                // preferred, capped at 3. Appended to the prompt directly — a
                // playbook spec has non-empty phase_prompts, so the generic
                // skills injection in run_workflow_inner is skipped by design.
                let (distilled_block, distilled_refs) =
                    self.distilled_skills_block(p, issue.stage).await?;
                spec.name = format!("#{} {}", issue.number, issue.title);
                spec.prompt = format!("{}{}{}", spec.prompt, issue_brief, distilled_block);
                // Put the injected skills on spec.skills so run_workflow_inner's
                // usage accounting bumps each one's `uses` — the compounding
                // loop closes here (a run that rides a distilled skill → uses+1).
                // The content itself is already in the prompt via distilled_block;
                // generic injection is skipped (playbook spec has phase_prompts).
                spec.skills.extend(distilled_refs);

                // Start: commit to the work (Backlog/Todo → InProgress). A
                // retry (issue already InProgress from a prior failed run)
                // skips this — X→X is not a legal table edge, and there's
                // nothing to change anyway.
                if issue.status != IssueStatus::InProgress {
                    self.store
                        .transition_issue(id, IssueStatus::InProgress)
                        .await?;
                    self.refresh_issues().await?;
                    self.emit(Event::IssuesChanged);
                }

                // Run through the same path as any run, bound to this issue.
                let run = self
                    .run_workflow_inner(p, session, spec, RunTrigger::Manual, None, Some(id))
                    .await;
                match run {
                    Ok(RunOutcome::Completed) => {
                        // A completed run only reaches 评审中 — never 完成. Done
                        // stays an explicit human `TransitionIssue` (铁律).
                        self.store
                            .transition_issue(id, IssueStatus::InReview)
                            .await?;
                        self.refresh_issues().await?;
                        self.emit(Event::IssuesChanged);
                    }
                    Ok(RunOutcome::BlockedAtCap { reason }) => {
                        // The adversarial loop hit its cap without passing. Never
                        // auto-Done, never auto-Failed: park the work in Blocked
                        // via the SAME guarded path `BlockIssue` uses (Blocked's
                        // only entry) — `can_transition_to` is the single source
                        // of truth. Re-read the current status (the run left it
                        // InProgress); InProgress→Blocked is a legal edge.
                        let cur = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                        if cur.status.can_transition_to(IssueStatus::Blocked) {
                            self.store.block_issue(id, &reason).await?;
                        }
                        self.emit(Event::WorkflowFailed(format!(
                            "Issue #{} {}",
                            issue.number, reason
                        )));
                        self.refresh_issues().await?;
                        self.emit(Event::IssuesChanged);
                    }
                    Err(e) => {
                        // Honest failure: the issue stays InProgress (not faked
                        // to InReview/Done/Blocked). Done remains a human
                        // TransitionIssue; a retry re-runs from InProgress.
                        self.emit(Event::WorkflowFailed(format!(
                            "Issue #{} 运行失败:{}",
                            issue.number, e
                        )));
                        self.refresh_issues().await?;
                        self.emit(Event::IssuesChanged);
                        return Err(e);
                    }
                }
            }

            Command::UpdateWorkflowSpec {
                id,
                prompt,
                goal,
                phases,
                phase_prompts,
                agents,
                skills,
                note,
            } => {
                if !phase_prompts.is_empty() && phase_prompts.len() != phases.len() {
                    return Err(AppError::Invalid(
                        "phase_prompts 必须为空或与 phases 等长".into(),
                    ));
                }
                self.store
                    .update_workflow_spec(
                        id,
                        WorkflowEdit {
                            prompt,
                            goal,
                            // Same name-only-text-editing scope as
                            // `CreateWorkflowSpec` above — an "优化" through
                            // this form honestly resets every phase to
                            // `Neutral` (a per-phase role editor is later UI
                            // work, not this ticket).
                            phases: phases.into_iter().map(PhaseMeta::neutral).collect(),
                            phase_prompts,
                            agents,
                            skills,
                            note,
                        },
                    )
                    .await?;
                self.refresh_workflow_specs().await?;
                self.emit(Event::WorkflowSpecsChanged);
            }

            Command::CreateSkill {
                id,
                name,
                desc,
                category,
                source,
                content,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_skill(NewSkill {
                        id,
                        name,
                        // A freshly created skill is honestly "just made,
                        // not yet proven" — Polishing, never Fresh (the
                        // SkillHub/AgentHub UI has no chip for a 3rd tier).
                        maturity: Maturity::Polishing,
                        desc,
                        category,
                        // T7: no stage selector on the hand-authored create
                        // form yet (out of this ticket's scope) — honest
                        // 通用 until an editor exists to classify it.
                        stage_ref: None,
                        source,
                        content,
                        project_id: None, // Hub 创建口径不变,一律全局
                    })
                    .await?;
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::DistillSkillFromIssue {
                skill_id,
                issue_id,
                name,
                desc,
                category,
                content,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .distill_skill_from_issue(
                        NewSkill {
                            id: skill_id,
                            name,
                            maturity: Maturity::Polishing,
                            desc,
                            category,
                            // 忽略:store::distill_skill_from_issue 同样改从源
                            // Issue 的真实 stage 派生(T7,与 project_id 同一
                            // provenance-not-input 规则),不采用这里传入的值。
                            stage_ref: None,
                            source: HubSource::SelfBuilt,
                            content,
                            // 忽略:store::distill_skill_from_issue 改从源 Issue
                            // 的真实 project_id 派生归属(provenance),不采用
                            // 这里传入的值。
                            project_id: None,
                        },
                        issue_id,
                    )
                    .await?;
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::ImportSkillPackage {
                source_path,
                project_id,
                official_library,
            } => {
                let parsed = skill_import::import_skill_package_from_disk(&source_path)
                    .map_err(AppError::Invalid)?;
                if parsed.name.trim().is_empty() {
                    return Err(AppError::Invalid(
                        "SKILL.md frontmatter 的 name 不能为空".into(),
                    ));
                }
                let source = match official_library {
                    Some(lib) => HubSource::Official {
                        official_library: lib,
                    },
                    None => HubSource::SelfBuilt,
                };
                self.store
                    .import_skill_package(
                        NewSkill {
                            id: SkillId::new(),
                            name: parsed.name,
                            // standards.rs 铁律:maturity 系统派生,新建一律
                            // fresh——外部库再有名,在 BW 里的成熟度只能由
                            // BW 本地真实使用挣出来,不从外部声誉继承。
                            // (/code-review 硬违规修正:原 Mature 引 seed
                            // 先例,但 seed 是内置角色路径,标准未为导入开例外。)
                            maturity: Maturity::Fresh,
                            desc: parsed.desc,
                            // T2 scope: no category assignment on import (no
                            // predetermined classification per plan/12 §2);
                            // stays empty, editable later via `UpdateSkill`.
                            category: String::new(),
                            // T7 (plan/12 §0/§2): no stage guessing on import
                            // either — 通用 until a human classifies it.
                            stage_ref: None,
                            source,
                            content: parsed.content,
                            project_id,
                        },
                        parsed
                            .files
                            .into_iter()
                            .map(|(rel_path, content)| NewSkillFile { rel_path, content })
                            .collect(),
                    )
                    .await?;
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::ImportSkillLibrary {
                root_path,
                official_library,
                project_id,
            } => {
                let dirs =
                    skill_import::find_skill_package_dirs(&root_path).map_err(AppError::Invalid)?;

                // Idempotency key: (name, official_library). Snapshot what
                // already exists in this library once up front, then keep it
                // updated locally as this loop inserts — catches a same-name
                // collision *within* this run too, not just against rows
                // that predate it.
                //
                // T11 (plan/12 §7): a name counts as "already in this
                // library" whether the row is still `Official { lib }` *or*
                // has since been hand-edited and flipped to `SelfBuilt` —
                // `adapted_from` is exactly the surviving `official_library`
                // read-back for that second case (see its doc comment). Only
                // matching the still-`Official` branch (the pre-T11 shape of
                // this filter) would let a re-import mint a brand-new
                // `Official` duplicate of a name the user has since made
                // their own — both an overwrite risk if it raced a later
                // `UpdateSkill` and a same-name-ambiguity risk either way,
                // exactly the two failure modes T11 exists to prevent.
                self.refresh_skills().await?;
                let mut existing_names: std::collections::HashSet<String> = self
                    .state
                    .skills
                    .iter()
                    .filter(|s| {
                        matches!(&s.source, HubSource::Official { official_library: lib } if lib == &official_library)
                            || s.adapted_from.as_deref() == Some(official_library.as_str())
                    })
                    .map(|s| s.name.clone())
                    .collect();

                let mut imported = 0u32;
                let mut skipped = 0u32;
                for dir in dirs {
                    let source_path = dir.to_string_lossy().into_owned();
                    let parsed = skill_import::import_skill_package_from_disk(&source_path)
                        .map_err(AppError::Invalid)?;
                    if parsed.name.trim().is_empty() {
                        return Err(AppError::Invalid(format!(
                            "{source_path}: SKILL.md frontmatter 的 name 不能为空"
                        )));
                    }
                    if existing_names.contains(&parsed.name) {
                        skipped += 1;
                        continue;
                    }
                    self.store
                        .import_skill_package(
                            NewSkill {
                                id: SkillId::new(),
                                name: parsed.name.clone(),
                                // 同 ImportSkillPackage:标准规定新建一律
                                // fresh,成熟度由 BW 本地使用派生。
                                maturity: Maturity::Fresh,
                                desc: parsed.desc,
                                // T3 scope, same as T2: no predetermined
                                // category on import.
                                category: String::new(),
                                // T7: same 通用-until-classified rule as
                                // `ImportSkillPackage` — no guessing across
                                // 55 imported skills either.
                                stage_ref: None,
                                source: HubSource::Official {
                                    official_library: official_library.clone(),
                                },
                                content: parsed.content,
                                project_id,
                            },
                            parsed
                                .files
                                .into_iter()
                                .map(|(rel_path, content)| NewSkillFile { rel_path, content })
                                .collect(),
                        )
                        .await?;
                    existing_names.insert(parsed.name);
                    imported += 1;
                }

                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
                self.emit(Event::SkillLibraryImported {
                    official_library,
                    imported,
                    skipped,
                });
            }

            Command::UpdateSkill {
                id,
                name,
                desc,
                category,
                content,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                // T11 (plan/12 §7): "编辑即脱离源头" — an `Official` row whose
                // substantive fields (content/desc/category; `name` is
                // identity, not content) really changed flips to `SelfBuilt`
                // in this same update. Compared against the real pre-edit
                // row, not the caller's own state cache, so a stale UI still
                // decides correctly. A no-op edit (identical content
                // resubmitted) or a rename-only edit never flips.
                let existing = self.store.get_skill(id).await?;
                let flip_to_self_built = existing.as_ref().is_some_and(|s| {
                    matches!(s.source, HubSource::Official { .. })
                        && (s.content != content || s.desc != desc || s.category != category)
                });
                self.store
                    .update_skill(
                        id,
                        SkillEdit {
                            name,
                            desc,
                            category,
                            content,
                            flip_to_self_built,
                        },
                    )
                    .await?;
                self.refresh_skills().await?;
                self.emit(Event::SkillsChanged);
            }

            Command::CreateAgent {
                id,
                name,
                role,
                skills,
                model,
                instructions,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_agent(NewAgent {
                        id,
                        name,
                        role,
                        // T7: no stage selector on the hand-authored create
                        // form yet (out of this ticket's scope) — honest
                        // 通用 until an editor exists to classify it.
                        stage_ref: None,
                        maturity: Maturity::Polishing,
                        skills,
                        model,
                        instructions,
                        // T5 (plan/12 §3): a hand-authored Hub agent declares
                        // no AllowedTools restriction yet (editable later,
                        // same "empty = unset" honesty `ImportSkillPackage`'s
                        // category follows) and runs on the one real executor
                        // this app has; self-authored ⇒ `SelfBuilt`.
                        tools: Vec::new(),
                        agent_cli: "claude-code".to_string(),
                        source: HubSource::SelfBuilt,
                        project_id: None, // Hub 创建口径不变,一律全局
                    })
                    .await?;
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
            }

            Command::UpdateAgent {
                id,
                name,
                role,
                skills,
                model,
                instructions,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                // T11 (plan/12 §7): same flip rule as `UpdateSkill` above.
                // Substantive fields for an Agent are `instructions`/`role`/
                // `model` — the ticket's own list also names `tools`, but
                // `UpdateAgent`/`AgentEdit` carry no `tools` field to edit
                // (AllowedTools isn't wired into this form), so it can never
                // differ through this path and is correctly left out of the
                // comparison. `name`/`skills` (tag list) are identity/
                // structural, not content, same "rename alone doesn't flip"
                // call `UpdateSkill` makes for its own `name`.
                let existing = self.store.get_agent(id).await?;
                let flip_to_self_built = existing.as_ref().is_some_and(|a| {
                    matches!(a.source, HubSource::Official { .. })
                        && (a.instructions != instructions || a.role != role || a.model != model)
                });
                self.store
                    .update_agent(
                        id,
                        AgentEdit {
                            name,
                            role,
                            skills,
                            model,
                            instructions,
                            flip_to_self_built,
                        },
                    )
                    .await?;
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
            }

            Command::ImportAgentDefinition {
                source_path,
                official_library,
            } => {
                let parsed = agent_import::import_agent_definition_from_disk(&source_path)
                    .map_err(AppError::Invalid)?;
                if parsed.name.trim().is_empty() {
                    return Err(AppError::Invalid(
                        "AGENT.md frontmatter 的 name 不能为空".into(),
                    ));
                }
                let source = match &official_library {
                    Some(lib) => HubSource::Official {
                        official_library: lib.clone(),
                    },
                    None => HubSource::SelfBuilt,
                };
                // T11 (plan/12 §7): idempotent re-import, `official_library`
                // path only — see this Command variant's doc comment for why
                // the check lives here rather than in a separate batch
                // command. `adapted_from` catches a name that has since been
                // hand-edited and flipped away from `Official`, exactly the
                // same union `ImportSkillLibrary` now checks.
                let is_duplicate = if let Some(lib) = &official_library {
                    self.refresh_agents().await?;
                    self.state.agents.iter().any(|a| {
                        a.name == parsed.name
                            && (matches!(&a.source, HubSource::Official { official_library: l } if l == lib)
                                || a.adapted_from.as_deref() == Some(lib.as_str()))
                    })
                } else {
                    false
                };
                if !is_duplicate {
                    self.store
                        .create_agent(NewAgent {
                            id: AgentId::new(),
                            name: parsed.name,
                            role: parsed.description,
                            // T7: same 通用-until-classified rule as the Skill
                            // import path — no guessing across 67 ECC agents.
                            stage_ref: None,
                            // 同 ImportSkillPackage:标准规定新建一律 fresh,
                            // 成熟度由 BW 本地真实使用派生,不从外部继承。
                            maturity: Maturity::Fresh,
                            // ECC AGENT.md files don't declare skill tags of
                            // their own; no predetermined mapping (no guessing).
                            skills: Vec::new(),
                            model: parsed.model,
                            instructions: parsed.instructions,
                            tools: parsed.tools,
                            agent_cli: "claude-code".to_string(),
                            source,
                            project_id: None,
                        })
                        .await?;
                }
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
            }

            Command::CreateCronTask {
                id,
                name,
                target,
                schedule,
                project_id,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_cron_task(NewCronTask {
                        id,
                        name,
                        target,
                        schedule,
                        project_id,
                        mode: CronMode::RunWorkflow,
                        issue_stage: None,
                        issue_assignee: None,
                    })
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::CreateAutopilotTask {
                id,
                name,
                schedule,
                project_id,
                stage,
                assignee,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_cron_task(NewCronTask {
                        id,
                        name,
                        target: String::new(), // unused in create_issue mode
                        schedule,
                        project_id,
                        mode: CronMode::CreateIssue,
                        issue_stage: Some(stage),
                        issue_assignee: assignee,
                    })
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::CreateRunSkillCronTask {
                id,
                name,
                schedule,
                project_id,
                skill_id,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_cron_task(NewCronTask {
                        id,
                        name,
                        // T10: the real skill id is the payload — round-tripped
                        // through `target` (see `bw_store::parse_cron_mode`).
                        target: skill_id.uuid().to_string(),
                        schedule,
                        project_id,
                        mode: CronMode::RunSkill { skill_id },
                        issue_stage: None,
                        issue_assignee: None,
                    })
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::CreateRunPromptCronTask {
                id,
                name,
                schedule,
                project_id,
                prompt,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                if prompt.trim().is_empty() {
                    return Err(AppError::Invalid("Prompt 不能为空".into()));
                }
                self.store
                    .create_cron_task(NewCronTask {
                        id,
                        name,
                        // T10: the prompt text itself is the payload — same
                        // `target` column `RunSkill`/`RunWorkflow` reuse.
                        target: prompt.clone(),
                        schedule,
                        project_id,
                        mode: CronMode::RunPrompt { prompt },
                        issue_stage: None,
                        issue_assignee: None,
                    })
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::SetCronStatus { id, status } => {
                self.store.set_cron_status(id, status).await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::MarkCronRun { id, status } => {
                self.store
                    .record_cron_run(id, status, run_at_label(now()))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
            }

            Command::CreateConnector {
                id,
                name,
                kind,
                scope,
                project_id,
                config,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_connector(NewConnector {
                        id,
                        name,
                        kind,
                        scope,
                        project_id,
                        config,
                    })
                    .await?;
                self.refresh_connectors().await?;
                self.emit(Event::ConnectorsChanged);
            }

            Command::CreateKnowledgeSource {
                id,
                name,
                kind,
                used_by,
            } => {
                if name.trim().is_empty() {
                    return Err(AppError::Invalid("名称不能为空".into()));
                }
                self.store
                    .create_knowledge_source(NewKnowledgeSource {
                        id,
                        name,
                        kind,
                        used_by,
                    })
                    .await?;
                self.refresh_knowledge_sources().await?;
                self.emit(Event::KnowledgeSourcesChanged);
            }

            Command::CreateIssue {
                id,
                stage,
                title,
                desc,
                priority,
            } => {
                let p = self.active()?;
                if title.trim().is_empty() {
                    return Err(AppError::Invalid("标题不能为空".into()));
                }
                self.store
                    .create_issue(NewIssue {
                        id,
                        project_id: p,
                        stage,
                        title,
                        desc,
                        priority,
                    })
                    .await?;
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::TransitionIssue { id, status } => {
                // Read the prior state first: the accounting below must fire
                // exactly once per work item, on its FIRST …→Done edge.
                // `settled_at` is the persistent settle-once marker — without
                // it, a Done → reopen → Done bounce (reachable through this
                // public command even though the desktop only offers forward
                // moves) would credit the same work twice.
                let prev = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                // A5-F: `Blocked` has its own entry point (`BlockIssue`) that
                // forces a reason — bare `TransitionIssue` never reaches it,
                // even though the edge is graph-legal (`can_transition_to`
                // says so); this command-level rule sits on top of the table.
                if status == IssueStatus::Blocked {
                    return Err(AppError::Invalid(format!(
                        "#{} 转 Blocked 需要阻塞原因;请使用 BlockIssue 命令",
                        prev.number
                    )));
                }
                // A re-dispatch of the SAME status (e.g. a duplicated Done
                // command) is a harmless re-affirmation, not a transition —
                // `can_transition_to` has no self-loops by design, so it's
                // checked only for a genuine state change. The settle-once
                // guard below (keyed on `prev.status != Done`) already makes
                // this safe: re-affirming Done fires no accounting twice.
                if status != prev.status && !prev.status.can_transition_to(status) {
                    return Err(AppError::Invalid(format!(
                        "非法转移:#{} {}→{}",
                        prev.number,
                        prev.status.label(),
                        status.label()
                    )));
                }
                self.store.transition_issue(id, status).await?;
                let newly_done = status == IssueStatus::Done
                    && prev.status != IssueStatus::Done
                    && prev.settled_at.is_none();
                if newly_done {
                    let issue = prev;
                    self.store
                        .mark_issue_settled(id, now().unix_timestamp())
                        .await?;
                    // The Done edge is the issue-side settle: the same real
                    // accounting a workflow-run settle does, fed by the same
                    // store functions. An issue completed by an agent teammate
                    // is one real run + one real win for that agent —
                    // `win_rate` derives from these counters, never hand-set.
                    // (Cancelled records nothing: dropping an issue is not
                    // evidence about the agent's work, and inventing a loss
                    // would fabricate a metric. Reopen-and-redo also records
                    // nothing new: one work item, one credit — the first win
                    // stands in the append-only history.)
                    if let Some(agent_id) = issue.assignee {
                        if let Some(agent) = self.store.get_agent(agent_id).await? {
                            self.store
                                .record_agent_run_by_name(&agent.name, true)
                                .await?;
                            self.refresh_agents().await?;
                            self.emit(Event::AgentsChanged);
                        }
                    }
                    // Artifact reflux, issue-scoped: whatever real files exist
                    // in the workspace at completion time get registered
                    // against the issue's stage (idempotent — an unchanged
                    // workspace registers 0 fresh rows).
                    if let Ok(Some(proj)) = self.store.get_project(issue.project_id).await {
                        if !proj.workspace_path.trim().is_empty() {
                            if let Ok(fresh) = self
                                .scan_and_register_artifacts(
                                    issue.project_id,
                                    &proj.workspace_path,
                                    None,
                                    Some(issue.stage),
                                    Some(id),
                                )
                                .await
                            {
                                if fresh > 0 {
                                    self.emit(Event::ArtifactsRegistered { fresh });
                                }
                            }
                        }
                    }
                    // A4: feed the stage's machine "完成 Issue 数" metric —
                    // change-guarded; empty target ⇒ Unknown (no fake green).
                    self.feed_stage_done_count(issue.project_id, issue.stage)
                        .await?;
                }
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::AssignIssue { id, assignee } => {
                self.store.assign_issue(id, assignee).await?;
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::BlockIssue { id, reason } => {
                let reason = reason.trim().to_string();
                if reason.is_empty() {
                    return Err(AppError::Invalid("转 Blocked 必须给出阻塞原因".into()));
                }
                let prev = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                // Same table as TransitionIssue queries — Blocked is only
                // reachable from Todo/InProgress/InReview (`can_transition_to`
                // is the single source of truth for both entry points).
                if !prev.status.can_transition_to(IssueStatus::Blocked) {
                    return Err(AppError::Invalid(format!(
                        "非法转移:#{} {}→阻塞",
                        prev.number,
                        prev.status.label()
                    )));
                }
                self.store.block_issue(id, &reason).await?;
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::RefreshIssues => {
                self.refresh_issues().await?;
                self.emit(Event::IssuesChanged);
            }

            Command::SendSessionMessage { session, text } => {
                self.store
                    .append_message(session, Role::Builder, &text)
                    .await?;
                self.emit(Event::SessionMessageAdded {
                    session,
                    role: Role::Builder,
                    text: text.clone(),
                });
                // Deterministic mock reply (the real agent reply arrives via Tier C).
                let reply = format!("【mock】已收到:{text}");
                self.store
                    .append_message(session, Role::Agent, &reply)
                    .await?;
                self.emit(Event::SessionMessageAdded {
                    session,
                    role: Role::Agent,
                    text: reply,
                });
            }

            Command::AnnotateWeeklyReview {
                human_override,
                reason,
            } => {
                let p = self.active()?;
                let derived = self
                    .store
                    .persisted_signals(p)
                    .await?
                    .project
                    .unwrap_or(Signal::Unknown);
                // MVP rule (plan §2.5): an override may only be *more* pessimistic.
                if let Some(ov) = human_override {
                    if severity(ov) < severity(derived) {
                        return Err(AppError::Invalid(
                            "周复盘 override 只能更悲观,不能更乐观".into(),
                        ));
                    }
                }
                self.store
                    .annotate_weekly_review(p, now(), derived, human_override, &reason)
                    .await?;
                self.emit(Event::WeeklyReviewAnnotated);
            }

            Command::OpenProject(id) => {
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                self.state.active_project = Some(id);
                self.state.active_session = None;
                self.state.panel = Panel::Progress;
                self.state.scope = Scope::All;
                self.state.view = match proj.phase {
                    ProjectPhase::ColdStart => View::Create,
                    ProjectPhase::Running => {
                        // Freshness is clock-relative — re-derive on open so a
                        // value that went stale since last time shows as such.
                        self.store.recompute_signals(id, now()).await?;
                        self.refresh_projects().await?;
                        View::App
                    }
                };
                self.refresh_issues().await?;
                self.emit(Event::ViewChanged(self.state.view));
            }

            Command::DeleteProject(id) => {
                self.store.delete_project(id).await?;
                if self.state.active_project == Some(id) {
                    self.state.active_project = None;
                    self.state.active_session = None;
                    self.state.view = View::Projects;
                    self.emit(Event::ViewChanged(View::Projects));
                }
                self.refresh_projects().await?;
                self.emit(Event::ProjectsChanged);
            }

            Command::BackToProjects => {
                self.state.view = View::Projects;
                self.state.active_project = None;
                self.state.active_session = None;
                self.refresh_projects().await?;
                self.refresh_issues().await?;
                self.emit(Event::ViewChanged(View::Projects));
            }

            Command::SetPanel(p) => self.state.panel = p,
            Command::SetScope(s) => self.state.scope = s,
            Command::SelectSession(s) => self.state.active_session = s,
        }
        Ok(())
    }
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// Standard workspace-derived metric names — the join keys between the
/// `git-repo` connector's probe and a project's metric definitions. A project
/// that defines metrics with these names (the conductor does; the creation
/// flow may) gets them machine-fed on every sync.
pub const METRIC_WS_COMMITS: &str = "工作区真实提交数";
pub const METRIC_WS_DOCS: &str = "剧本产物文档数";

/// `claude --version` probe with a hard timeout — the `claude-cli`
/// connector's real health check. Returns the version line on success.
async fn claude_version_probe(binary: &str) -> Result<String, String> {
    let fut = tokio::process::Command::new(binary)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();
    let output = tokio::time::timeout(std::time::Duration::from_secs(10), fut)
        .await
        .map_err(|_| "探针超时(10s)".to_string())?
        .map_err(|e| format!("无法运行 {binary}:{e}"))?;
    if !output.status.success() {
        return Err(format!(
            "{binary} --version 退出码非零:{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Filesystem-safe slug for a project's workspace directory: ascii
/// alphanumerics kept, everything else (CJK included) dropped, always
/// suffixed with the id's first 8 hex chars so two "同名" projects can never
/// collide (and a fully-CJK name still yields a unique, valid dir).
fn workspace_slug(name: &str, id: ProjectId) -> String {
    let base: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let id8: String = id.uuid().simple().to_string().chars().take(8).collect();
    if base.is_empty() {
        format!("proj-{id8}")
    } else {
        format!("{base}-{id8}")
    }
}

/// Provision the project's own git workspace under `root` (all-in-one-
/// codebase default). Returns the real path. The README is written from the
/// project's own creation-flow data — real inputs, not invented content.
async fn provision_workspace(root: &std::path::Path, proj: &ProjectRow) -> Result<String, String> {
    let dir = root.join(workspace_slug(&proj.name, proj.id));
    let body = if proj.desc.trim().is_empty() {
        "(创建流程未填写 brief)".to_string()
    } else {
        proj.desc.trim().to_string()
    };
    bw_engine::provision_git_workspace(&dir, &proj.name, &body)
        .await
        .map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

/// P1: the project's charter (`PROJECT.md`) — every line is a real creation-
/// flow input, never invented. Empty fields show 「(待填)」 so an in-progress
/// charter reads honestly rather than faking completeness.
fn charter_md(proj: &ProjectRow) -> String {
    const PENDING: &str = "(待填)";
    let mut s = String::new();
    s.push_str(&format!("# {}\n\n", proj.name));
    let kind = proj.kind.trim();
    if !kind.is_empty() {
        s.push_str(&format!("**类型**:{kind}\n\n"));
    }
    let desc = proj.desc.trim();
    if !desc.is_empty() {
        s.push_str(&format!("{desc}\n\n"));
    }
    s.push_str("## 定位与机会\n\n");
    let bench = proj.benchmark.trim();
    let opp = proj.opportunity.trim();
    s.push_str(&format!(
        "- **对标**:{}\n",
        if bench.is_empty() { PENDING } else { bench }
    ));
    s.push_str(&format!(
        "- **机会**:{}\n\n",
        if opp.is_empty() { PENDING } else { opp }
    ));
    s.push_str("## 北极星(三个月成功标准)\n\n");
    let ns = proj.north_star.trim();
    if ns.is_empty() {
        s.push_str(&format!("{PENDING}\n\n"));
    } else {
        s.push_str(&format!("{ns}\n\n"));
        let def = proj.ns_def.trim();
        if !def.is_empty() {
            s.push_str(&format!("> 定义:{def}\n\n"));
        }
    }
    s.push_str("---\n\n> 本章程由 Builders' Workbench 在创建流程中逐步写就,每次更新留一次提交。\n");
    s
}

/// P1: write the project's `PROJECT.md` charter into its OWNED workspace and
/// commit it (`docs(bw): 项目章程 · <节>`)。Bound、pre-existing 仓永不写;
/// 无工作区则 no-op。Best-effort —— 章程写失败不阻断创建流。
async fn write_charter(app: &App, p: ProjectId, section: &str) -> Result<(), AppError> {
    let proj = app.store.get_project(p).await?.ok_or(AppError::NotFound)?;
    let ws = proj.workspace_path.trim();
    if ws.is_empty() {
        return Ok(());
    }
    let dir = std::path::Path::new(ws);
    if !bw_engine::workspace::is_owned_workspace(dir).await {
        return Ok(());
    }
    bw_engine::workspace::commit_file(
        dir,
        "PROJECT.md",
        &charter_md(&proj),
        &format!("docs(bw): 项目章程 · {section}"),
    )
    .await
    .map_err(|e| AppError::Engine(format!("写章程失败:{e}")))?;
    Ok(())
}

/// 模板能力:写四份组件标准文件(`.claude/standards/*.md`)进项目的 owned 工作区。
/// 内容是 [`bw_core::standards`] 里通用、versioned-in-code 的方法论文本(不含
/// per-project 数据),所以只在出生那一刻写一次——不像章程随创建流逐步补内容,
/// 这四份文件从第一天起就是完整的。Bound(绑定已有仓)项目不写,同 `write_charter`
/// 的「不动原文件」纪律;无工作区则 no-op;best-effort,失败不阻断创建流。
async fn write_component_standards(app: &App, p: ProjectId) -> Result<(), AppError> {
    let proj = app.store.get_project(p).await?.ok_or(AppError::NotFound)?;
    let ws = proj.workspace_path.trim();
    if ws.is_empty() {
        return Ok(());
    }
    let dir = std::path::Path::new(ws);
    if !bw_engine::workspace::is_owned_workspace(dir).await {
        return Ok(());
    }
    for (rel_path, content) in [
        (
            ".claude/standards/agent-standards.md",
            bw_core::standards::AGENT_STANDARDS_MD,
        ),
        (
            ".claude/standards/skill-standards.md",
            bw_core::standards::SKILL_STANDARDS_MD,
        ),
        (
            ".claude/standards/workflow-standards.md",
            bw_core::standards::WORKFLOW_STANDARDS_MD,
        ),
        (
            ".claude/standards/cron-standards.md",
            bw_core::standards::CRON_STANDARDS_MD,
        ),
    ] {
        bw_engine::workspace::commit_file(
            dir,
            rel_path,
            content,
            "docs(bw): 模板能力 · 组件标准文件",
        )
        .await
        .map_err(|e| AppError::Engine(format!("写标准文件失败({rel_path}):{e}")))?;
    }
    Ok(())
}

/// Snapshot of the spec's shape at run time, serialized into the run's
/// `params_json` (iter 3). Records what a run *actually executed* — so after
/// a later `UpdateWorkflowSpec` rewrites the phases, a past run's history
/// still truthfully shows the phases it ran. Pure function of the spec +
/// trigger; no IO, no secrets.
/// Forward one engine [`RunEvent`] to the live UI stream (T9 helper — shared by
/// every `run_phase_range` call inside the adversarial loop so a subscriber sees
/// phases advance and re-advance across rounds). `WorkflowDone` is emitted by
/// the loop itself once the whole run truly finishes, so it's a no-op here.
fn forward_progress(live: &broadcast::Sender<Event>, e: RunEvent) {
    match e {
        RunEvent::PhaseStarted { idx, name } => {
            let _ = live.send(Event::WorkflowProgress {
                phase_idx: idx,
                status: format!("started:{name}"),
            });
        }
        RunEvent::PhaseCompleted { idx, .. } => {
            let _ = live.send(Event::WorkflowProgress {
                phase_idx: idx,
                status: "completed".into(),
            });
        }
        RunEvent::WorkflowFailed { error } => {
            let _ = live.send(Event::WorkflowFailed(error));
        }
        RunEvent::WorkflowDone { .. } => {}
    }
}

/// The tail slice of a review output — enough context to seed the next round's
/// reject baton or an honest error message, without dragging a whole transcript.
fn review_tail(text: &str) -> String {
    const MAX: usize = 400;
    let t = text.trim();
    let n = t.chars().count();
    if n <= MAX {
        return t.to_string();
    }
    t.chars().skip(n - MAX).collect()
}

/// T10 (plan/12 §5): the ephemeral spec `tick_scheduler` runs a `RunSkill`/
/// `RunPrompt` cron task through — same `run_workflow_inner` engine/executor
/// path a `RunWorkflow` cron task uses (so evidence lands in `workflow_run`/
/// `CronEffectiveness` identically), just one neutral phase carrying the
/// skill's `content` or the bare prompt text instead of a real hub workflow's
/// phases. Never persisted — minted fresh on every fire, like `stage_workflow`'s
/// throwaway `Dynamic` specs.
fn cron_prompt_workflow(name: String, prompt: String) -> WorkflowSpec {
    WorkflowSpec {
        id: WorkflowId::new(),
        name,
        kind: WorkflowKind::Dynamic {
            origin: "定时任务".into(),
            stage: String::new(),
        },
        prompt,
        goal: "定时任务真实执行".into(),
        stage_ref: None,
        phases: vec![PhaseMeta::neutral("执行")],
        phase_prompts: vec![],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 1,
        },
        project_id: None,
    }
}

/// `agent_cli`/`tools`/`allowed_tools_arg` are T6 (plan/12 §3) additions: the
/// resolved Agent-CLI route and the exact `--allowedTools` value it implies,
/// snapshotted BEFORE the engine runs — so a run's real invocation
/// parameters read back from `params_json` regardless of whether the
/// executor call itself ever completes (an `UnsupportedCliExecutor` errors
/// on its very first call; a real `claude -p` call may hit a flaky gateway).
fn run_params_snapshot(
    spec: &WorkflowSpec,
    trigger: RunTrigger,
    agent_cli: &str,
    tools: &[String],
    allowed_tools_arg: Option<&str>,
) -> String {
    // serde_json::Value keeps this stable as the spec grows — adding a field
    // later is additive, not a schema break on historical run rows.
    let v = serde_json::json!({
        "phases": spec.phases,
        "phase_count": spec.phases.len(),
        // Whether this run executed per-phase playbook instructions (vs the
        // legacy shared prompt) — an A/B axis for later run analytics.
        "playbook": !spec.phase_prompts.is_empty(),
        "loop": { "retries": spec.loop_config.retries, "max_iter": spec.loop_config.max_iter },
        "agents": spec.agents.len(),
        "skills": spec.skills.len(),
        "stage_ref": spec.stage_ref,
        "trigger": trigger.text(),
        "kind": match &spec.kind {
            WorkflowKind::Static { version, .. } => format!("static:v{version}"),
            WorkflowKind::Dynamic { origin, .. } => format!("dynamic:{origin}"),
        },
        "agent_cli": agent_cli,
        "tools": tools,
        "allowed_tools_arg": allowed_tools_arg,
    });
    v.to_string()
}

/// Compact, real `"YYYY-MM-DD HH:MM"` label for `CronTask.last_run` — a
/// plain display string (same tier as `next_run`), not a typed timestamp
/// column, so this is formatted once here rather than at every read site.
fn run_at_label(at: OffsetDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        at.year(),
        u8::from(at.month()),
        at.day(),
        at.hour(),
        at.minute()
    )
}

/// Worse signals sort higher. `Unknown` sits between green and amber — more
/// pessimistic than green, less than a known amber.
fn severity(s: Signal) -> u8 {
    match s {
        Signal::Green => 0,
        Signal::Unknown => 1,
        Signal::Amber => 2,
        Signal::Red => 3,
    }
}

/// Materialize the five stages for a freshly completed project, all on the
/// chosen review cadence. `active_stage` is already `Prototype` from
/// creation — every project's first lap starts there.
fn five_stages(project: ProjectId, cadence: Cadence) -> Vec<NewStage> {
    StageKind::ALL
        .into_iter()
        .map(|kind| NewStage {
            project_id: project,
            kind,
            schedule: cadence.clone(),
        })
        .collect()
}
