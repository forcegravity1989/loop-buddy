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

use bw_core::derive::AmberBand;
use bw_core::model::{
    classify_artifact_path, cron_due, stage_workflow, stage_workflow_with_playbook, AgentCard,
    AgentRef, Artifact, Cadence, Connector, ConnectorStatus, CronMode, CronStatus, CronTask,
    HubSource, Issue, IssuePriority, IssueStatus, KnowledgeSource, LibSource, LoopConfig, Maturity,
    ProjectCycle, ProjectPhase, Role, RunStatus, RunTrigger, Signal, SkillCard, SkillRef,
    SourceKind, StageKind, WorkflowKind, WorkflowSpec, CONNECTOR_KIND_CLAUDE_CLI,
    CONNECTOR_KIND_GITHUB_REPO, CONNECTOR_KIND_GIT_REPO,
};
use bw_core::{
    AgentId, ArtifactId, ConnectorId, CronTaskId, IssueId, KnowledgeSourceId, MetricId, ProjectId,
    SessionId, SkillId, WorkflowId, WorkflowRunId,
};
use bw_engine::{
    evidence, ClaudeCliConfig, ClaudeCliExecutor, Engine, GitCommit, GithubRepoSummary,
    PermissionMode, RunCtx, RunEvent,
};
use bw_store::{
    AgentEdit, GlobalHandoffRow, MetricDefSync, MetricRole, MetricsFileSync, NewAgent, NewArtifact,
    NewConnector, NewCronTask, NewIssue, NewKnowledgeSource, NewMetric, NewProject, NewSession,
    NewSkill, NewStage, NewWorkflowSpec, ProjectRow, SessionKind, SkillEdit, Store, WorkflowEdit,
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

/// Where a newly-created project's git remote comes from — the Repo 卡片的
/// 选择,carried into `Command::CreateProject`. `New` mints a fresh GitHub
/// repo (`gh repo create --clone`); `Existing` clones one the user already
/// owns. `None` on the command (every pre-2026-07-22 caller) keeps every
/// existing behavior — pure local mint or bound-local-path — untouched.
#[derive(Clone, Debug)]
pub enum GithubOrigin {
    New { slug: String, private: bool },
    Existing { owner: String, repo: String },
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
        /// P1: optional pre-existing *local* repo to bind (must contain
        /// `.git`). Mutually exclusive with `github` — the Repo 卡片 is the
        /// sole UI entry point and only ever sets one of the two.
        workspace: Option<String>,
        /// GitHub 为主体(2026-07-22): Repo 卡片的选择. `None` = neither
        /// bound (`workspace` also `None`) → today's local-mint-if-configured
        /// default, unchanged.
        github: Option<GithubOrigin>,
    },
    /// GitHub 为主体的创建流(2026-07-22): 读一次当前用户可接入的仓列表,
    /// 填充 `AppState.github_repos`(Repo 卡片"接入已有仓"下拉的数据源)。
    /// 显式加载,同 `LoadVersionLog`/`LoadArtifacts` 惯例——不在每次
    /// rebuild 里打 GitHub API。
    ListGithubRepos,
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
    /// C6 (plan/13 D5+D6): read the active project's `.bw/metrics.toml`
    /// (metrics source of truth) and sync it into the SQLite cache — north
    /// star name/def/collect plan updated in place, every lagging/leading
    /// metric upserted by name (idempotent: re-syncing an unchanged file
    /// inserts zero new rows). No configured workspace, or a workspace with
    /// no file yet, is a deliberate silent no-op — same "nothing to report"
    /// stance as a project that was never wired to GitHub. A file that fails
    /// to parse emits `Event::ConnectorSynced { ok: false, .. }` and writes
    /// nothing (parse succeeds in full or the cache stays untouched). Never
    /// appends an observation or calls `recompute_signals` — this syncs
    /// *definitions*, not values (collection execution is a later ticket,
    /// C7).
    SyncMetricsFile,
    /// C7 · 采集器 (plan/13 D7): pull real data into the active project's
    /// metrics *right now* — the manual「立即采集」counterpart to the standard
    /// daily collect cron. For every `collect.kind = "github"` metric it runs
    /// a real `gh` count query against the project's remote and appends an
    /// append-only observation *only when the value changed* (change-guard),
    /// then re-derives signals. `bw`/`connector` kinds are v1 未接 — left
    /// blank, their signal stays Unknown (无数据 ≠ 绿). A `gh` failure writes
    /// nothing and surfaces an honest `ConnectorSynced { ok: false, .. }`
    /// toast; the signal degrades on staleness instead of flashing a fake
    /// zero. Never settles or runs anything — collection is observation, not
    /// work.
    CollectMetrics,
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
        source: LibSource,
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
    /// C5 · PR 验收环 (plan/13 D3): the **human验收** entry point for an Issue
    /// whose run opened a PR — merge the PR, which (via `Closes #<n>`) closes
    /// the GitHub issue, then settle the Issue Done through the *existing*
    /// `TransitionIssue` InReview→Done accounting path (settle-once reused, no
    /// second accounting path). The executor never merges — this command is the
    /// only place `gh pr merge` is ever called. Idempotent: a re-dispatch after
    /// the Issue is already Done is a no-op that never re-merges or re-accounts.
    /// Issues with no PR (no-repo/存量) keep using bare `TransitionIssue` to
    /// Done — 全活 PR 化是纪律不是硬闸 (只留痕不拦人).
    MergeIssuePr {
        id: IssueId,
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
    /// GitHub 为主体的创建流: last `Command::ListGithubRepos` result. Process-
    /// internal cache of live GitHub data, not persisted — it's a direct
    /// read-through, not one of this app's own derived Signals.
    pub github_repos: Vec<GithubRepoSummary>,
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
            github_repos: Vec::new(),
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
    ) -> Result<(), AppError> {
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

        let started_at = OffsetDateTime::now_utc().unix_timestamp();
        let t0 = Instant::now();
        let params_json = run_params_snapshot(&spec, trigger);
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
        // A3: bind this run to the Issue it executes (RunIssue passes Some;
        // every other caller passes None). Kept as a separate UPDATE so the
        // run-creation DTO stays stable — the issue link is a RunIssue concern.
        if let Some(iid) = issue_id {
            self.store.set_run_issue(run_log_id, iid).await?;
        }

        // `workspace_path` is per-project runtime data, not something
        // baked into a long-lived Engine at App::new time: unconfigured
        // projects keep running on the shared Mock engine (byte-for-
        // byte today's behavior, zero regression); a configured one
        // gets a fresh, one-shot real executor built just for this call.
        let fresh_engine;
        let engine: &Engine = if proj.workspace_path.trim().is_empty() {
            &self.mock_engine
        } else {
            let executor = ClaudeCliExecutor::new(
                self.state.claude_config.clone(),
                PathBuf::from(proj.workspace_path.trim()),
                proj.allow_commands,
            );
            fresh_engine = Engine::new(Arc::new(executor));
            &fresh_engine
        };

        // Announce what's actually about to run — real name/agents/skills
        // straight off `spec`, before the first phase event — so a live
        // subscriber can render "this run uses X/Y" without guessing.
        self.emit(Event::RunStarted {
            workflow_name: spec.name.clone(),
            agents: spec.agents.clone(),
            skills: spec.skills.clone(),
        });

        // Progress events are emitted LIVE from inside the engine
        // callback (broadcast::send is sync), so a subscriber watches
        // phases advance while the run is still going. Only persistence
        // (async) is deferred to after the run.
        let live = self.events.clone();
        let mut completed: Vec<bw_engine::PhaseOutput> = Vec::new();
        let run = engine
            .run_workflow(&spec, &ctx, |e| match e {
                RunEvent::PhaseStarted { idx, name } => {
                    let _ = live.send(Event::WorkflowProgress {
                        phase_idx: idx,
                        status: format!("started:{name}"),
                    });
                }
                RunEvent::PhaseCompleted { idx, output } => {
                    let _ = live.send(Event::WorkflowProgress {
                        phase_idx: idx,
                        status: "completed".into(),
                    });
                    completed.push(output);
                }
                RunEvent::WorkflowDone { .. } => {
                    let _ = live.send(Event::WorkflowDone);
                }
                RunEvent::WorkflowFailed { error } => {
                    let _ = live.send(Event::WorkflowFailed(error));
                }
            })
            .await;

        // Persist whatever phases completed, even on failure — the run
        // history must not silently vanish.
        let phases_completed = completed.len() as u32;
        for output in completed {
            self.store
                .append_message(session, Role::Agent, &output.text)
                .await?;
            self.emit(Event::SessionMessageAdded {
                session,
                role: Role::Agent,
                text: output.text,
            });
        }

        // Settle the run record with the real outcome + real elapsed time.
        // `phases_completed` is the honest count of phases that finished — a
        // partial run that died at phase 2 of 5 records `2`, not silence.
        let finished_at = OffsetDateTime::now_utc().unix_timestamp();
        let duration_ms = t0.elapsed().as_millis() as i64;
        match &run {
            Ok(_) => {
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
            }
            Err(e) => {
                self.store
                    .settle_workflow_run(
                        run_log_id,
                        RunStatus::Failed,
                        finished_at,
                        duration_ms,
                        phases_completed,
                        &e.to_string(),
                    )
                    .await?;
            }
        }

        // P4: the "after" half of the change window — recorded on success AND
        // failure alike (a failed run that still committed something must not
        // hide it). Diffing happens lazily at detail-open time, never here.
        if !heads_workspace.is_empty() {
            let head_after = evidence::head_commit(&heads_workspace).await.ok().flatten();
            self.store
                .set_run_heads(run_log_id, head_before, head_after)
                .await?;
        }

        // Usage accounting: the run really happened, so the entities it rode
        // on get their real counters bumped — the agent that hosted it (ok
        // AND failed both count; win_rate needs the losses) and every skill
        // whose content/name it carried. Refs that don't resolve to a hub
        // row are an honest 0-row no-op.
        let run_ok = run.is_ok();
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

        // Artifact reflux: scan the real workspace the run just worked in and
        // register any new file versions against this run. A failed run's
        // partial output is still real output — scan regardless of outcome.
        // Scan errors (e.g. git missing) must not turn a settled run into an
        // error; they surface as a 0-fresh no-op with the run's own outcome
        // untouched.
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
                    // A2 原设计:「RunIssue 落 run 时写入」issue 归属——此前
                    // 误传 None,导致活的产物只绑 run 不绑 issue,Done 边沿
                    // 的幂等重扫(同 commit)又补不回来。P4 证据面测试暴露
                    // 了这个偏差,修复即对齐冻结设计。
                    issue_id,
                )
                .await
            {
                if fresh > 0 {
                    self.emit(Event::ArtifactsRegistered { fresh });
                }
            }
        }

        run.map_err(|e| AppError::Engine(e.to_string()))?;
        Ok(())
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

    /// C4 · issue 身份映射(plan/13 D2): a project with a `github_remote`
    /// gets every BW-minted Issue mirrored as a real GitHub issue — the issue
    /// number is the Issue's cross-system identity. Called AFTER the Issue
    /// already exists in `bw-store`, so a `gh` failure never blocks the
    /// BW-side create (创建不破): the Issue simply keeps `github_number = 0`
    /// and an honest `ConnectorSynced { ok: false, .. }` toast fires — same
    /// soft-degrade shape as the Repo 卡片's `create_repo`/`clone_repo`
    /// paths. `github_remote` empty (no repo, or a 存量项目) short-circuits
    /// before touching `gh` at all — today's behavior, byte-for-byte.
    async fn sync_issue_to_github(
        &mut self,
        project_id: ProjectId,
        issue_id: IssueId,
        title: &str,
        desc: &str,
    ) -> Result<(), AppError> {
        let proj = self
            .store
            .get_project(project_id)
            .await?
            .ok_or(AppError::NotFound)?;
        let remote = proj.github_remote.trim();
        if remote.is_empty() {
            return Ok(());
        }
        let body = if desc.trim().is_empty() {
            "(BW 建单同步,未填写详情)".to_string()
        } else {
            desc.trim().to_string()
        };
        match bw_engine::github::create_issue(remote, title, &body).await {
            Ok(gh_number) => {
                self.store
                    .set_issue_github_number(issue_id, gh_number)
                    .await?;
            }
            Err(e) => {
                self.emit(Event::ConnectorSynced {
                    name: format!("{title} · GitHub Issue"),
                    ok: false,
                    detail: format!("GitHub 开 issue 失败,BW 侧 Issue 已建立,号未映射:{e}"),
                });
            }
        }
        Ok(())
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

    /// C7 · 采集器 (plan/13 D7): pull real data into `project`'s metrics as
    /// append-only observations. Shared by the manual `Command::CollectMetrics`
    /// and the standard collect cron (`tick_scheduler`), so both entrances walk
    /// exactly one code path.
    ///
    /// v1 只真采一类:每条 `collect.kind = "github"` 指标跑一次真实 `gh` 计数
    /// 查询,change-guard 命中(值未变)则不重复记点;值变了才 append 一个观测
    /// 并 `recompute_signals`(唯一派生入口)。`bw`/`connector` 两类**如实留白**
    /// ——不采集、不写零值,signal 保持既有(无数据即 Unknown,绝不假绿)。
    /// `gh` 失败:零新观测、如实计入失败(供 ok:false toast),signal 靠既有
    /// 过期降级自然变灰。北极星的采集方案落在 `project` 列(非 metric 行),没有
    /// 可挂观测的 metric_id,v1 不采(留白见 docs/metrics-toml-format.md)。
    async fn collect_project_metrics(
        &mut self,
        project: ProjectId,
    ) -> Result<MetricCollectSummary, AppError> {
        let proj = self
            .store
            .get_project(project)
            .await?
            .ok_or(AppError::NotFound)?;
        let remote = proj.github_remote.trim().to_string();
        let sigs = self.store.persisted_signals(project).await?;
        let today = now().date();
        let mut summary = MetricCollectSummary::default();
        let mut touched = false;

        for m in &sigs.metrics {
            match m.collect_kind.as_str() {
                "github" => {
                    if remote.is_empty() {
                        // No repo to pull from — honest, not a silent success.
                        summary.no_remote_github = true;
                        continue;
                    }
                    match bw_engine::github::collect_github_count(&remote, &m.collect_query, today)
                        .await
                    {
                        Ok(count) => {
                            let value = count.to_string();
                            if m.value_raw == value {
                                summary.unchanged += 1; // change-guard: not a new fact
                            } else {
                                self.store
                                    .append_observation(m.id, SourceKind::Github, &value, now())
                                    .await?;
                                summary.changed += 1;
                                touched = true;
                            }
                        }
                        Err(e) => {
                            summary.failed += 1;
                            if summary.first_error.is_none() {
                                summary.first_error = Some(format!("{}:{e}", m.name));
                            }
                        }
                    }
                }
                // v1 未接:bw / connector 如实留白——不采集、不写零值。
                "bw" | "connector" => summary.deferred += 1,
                // manual(或空 collect_kind = 界面手建)不归采集器管。
                _ => {}
            }
        }

        if touched {
            self.store.recompute_signals(project, now()).await?;
            self.emit(Event::ProjectUpdated(project));
        }
        Ok(summary)
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

            // C7: the standard collector — pull real data into the project's
            // metrics as append-only observations. No-hijack by construction:
            // this branch never calls run_workflow_inner, never settles
            // anything — collecting is observation, not work, so an unattended
            // auto-fire can't breach 「Done 永不自动」.
            if c.mode == CronMode::CollectMetrics {
                self.store
                    .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                    .await?;
                let res = self.collect_project_metrics(pid).await;
                let (ok, status) = match &res {
                    Ok(s) => (s.failed == 0 && !s.no_remote_github, CronStatus::Normal),
                    Err(_) => (false, CronStatus::Failed),
                };
                self.store
                    .record_cron_run(c.id, status, run_at_label(now()))
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
            let ok = result.is_ok();
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
        let title = format!("[auto] {name}");
        let desc = format!(
            "Autopilot 建单(定时任务「{name}」于 {} 触发,{} 阶段)。",
            run_at_label(fired_at),
            stage.label()
        );
        self.store
            .create_issue(NewIssue {
                id: issue_id,
                project_id: project,
                stage,
                title: title.clone(),
                desc: desc.clone(),
                priority: IssuePriority::Medium,
            })
            .await?;
        // C4: Autopilot/cron 建单同样过身份映射 —— 建单入口不止手动创建一
        // 处,漏一条就是"手动建的有号、定时建的没号"的诚实性缺口。
        self.sync_issue_to_github(project, issue_id, &title, &desc)
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
                // C9 · plan/13 D8: the two standard-Issue-trio skills (找指标/
                // 绑数据) — by-name idempotent, so an already-seeded database
                // gains them too. Content is `include_str!`-ed straight from
                // docs/skills/<slug>/SKILL.md (the real file in the repo).
                bw_store::seed_standard_issue_skills_if_missing(self.store.as_ref()).await?;
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
                github,
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
                // 绑定已有本地仓:只校验含 .git,绝不动原文件。GitHub 为主体
                // (2026-07-22): github 非空时改走 gh CLI 开仓/接入,新建失败
                // 软降级回本地 mint,接入失败不兜底(不拿无关空仓冒充)。两条
                // 路径都绝不让 CreateProject 本身失败——只有本地 bind 校验例外。
                let bound = workspace
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                match (bound, github) {
                    (Some(path), _) => {
                        if !std::path::Path::new(path).join(".git").exists() {
                            return Err(AppError::Invalid(format!(
                                "绑定的工作目录不是 git 仓库(无 .git):{path}"
                            )));
                        }
                        self.store.set_workspace(id, path, true).await?;
                    }
                    (None, Some(GithubOrigin::New { slug, private })) => {
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let body = if proj.desc.trim().is_empty() {
                                    "(创建流程未填写 brief)".to_string()
                                } else {
                                    proj.desc.trim().to_string()
                                };
                                match bw_engine::github::create_repo(
                                    &slug, private, &root, &proj.name, &body,
                                )
                                .await
                                {
                                    Ok(r) => {
                                        let path = root.join(&slug).to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &path, true).await?;
                                        self.store
                                            .set_github_remote(
                                                id,
                                                &format!("{}/{}", r.owner, r.repo),
                                            )
                                            .await?;
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
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · GitHub", proj.name),
                                                kind: CONNECTOR_KIND_GITHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{}/{}", r.owner, r.repo),
                                            })
                                            .await?;
                                    }
                                    Err(e) => {
                                        let mut detail =
                                            format!("GitHub 建仓失败,已尝试改建本地仓:{e}");
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
                                            Err(local_e) => {
                                                detail = format!(
                                                    "GitHub 建仓失败:{e};本地兜底也失败:{local_e}"
                                                );
                                            }
                                        }
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · GitHub", proj.name),
                                            ok: false,
                                            detail,
                                        });
                                    }
                                }
                            }
                            None => {
                                self.emit(Event::ConnectorSynced {
                                    name: format!("{} · GitHub", proj.name),
                                    ok: false,
                                    detail: "未配置本地工作区根目录,无法建仓".into(),
                                });
                            }
                        }
                    }
                    (None, Some(GithubOrigin::Existing { owner, repo })) => {
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let dir = root.join(workspace_slug(&proj.name, id));
                                match bw_engine::github::clone_repo(&owner, &repo, &dir).await {
                                    Ok(r) => {
                                        let path = dir.to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &path, true).await?;
                                        self.store
                                            .set_github_remote(
                                                id,
                                                &format!("{}/{}", r.owner, r.repo),
                                            )
                                            .await?;
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
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · GitHub", proj.name),
                                                kind: CONNECTOR_KIND_GITHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{}/{}", r.owner, r.repo),
                                            })
                                            .await?;
                                    }
                                    Err(e) => {
                                        // 不兜底本地 mint —— 拿一个跟用户选的仓无关
                                        // 的空仓冒充"已接入",比"暂不挂仓库"更不诚实。
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · GitHub", proj.name),
                                            ok: false,
                                            detail: format!("接入 {owner}/{repo} 失败:{e}"),
                                        });
                                    }
                                }
                            }
                            None => {
                                self.emit(Event::ConnectorSynced {
                                    name: format!("{} · GitHub", proj.name),
                                    ok: false,
                                    detail: "未配置本地工作区根目录,无法接入".into(),
                                });
                            }
                        }
                    }
                    (None, None) => {
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
                // C7 · 标配采集 cron (plan/13 D7):挂了 GitHub 仓的项目出生即
                // 带一条每日采集器,由现成 tick_scheduler 到点真实触发,把
                // GitHub 数据拉成 append-only 观测。只有 github 项目挂(无 remote
                // 即无 github 源可采);软降级回本地/接入失败的项目 github_remote
                // 仍空,不挂——不给采不到的东西装一个空跑的 cron。no-hijack:
                // CollectMetrics 只观测,绝不自动跑活/结算。
                let github_backed = self
                    .store
                    .get_project(id)
                    .await?
                    .map(|pr| !pr.github_remote.trim().is_empty())
                    .unwrap_or(false);
                if github_backed {
                    self.store
                        .create_cron_task(NewCronTask {
                            id: CronTaskId::new(),
                            name: format!("{} · 指标采集", proj.name),
                            target: String::new(),
                            schedule: Cadence::Daily,
                            project_id: Some(id),
                            mode: CronMode::CollectMetrics,
                            issue_stage: None,
                            issue_assignee: None,
                        })
                        .await?;
                    self.refresh_cron_tasks().await?;
                    self.emit(Event::CronTasksChanged);
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

            Command::ListGithubRepos => {
                match bw_engine::github::list_repos(30).await {
                    Ok(repos) => self.state.github_repos = repos,
                    Err(e) => {
                        self.state.github_repos = Vec::new();
                        self.emit(Event::ConnectorSynced {
                            name: "GitHub 仓库列表".into(),
                            ok: false,
                            detail: e.to_string(),
                        });
                    }
                }
                self.emit(Event::ProjectsChanged);
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

            Command::SyncMetricsFile => {
                let p = self.active()?;
                let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
                match bw_engine::metrics_file::read(&proj.workspace_path) {
                    // No configured workspace, or a workspace with no file
                    // yet — 文件不存在:零动作零噪音,行为与今日完全一致
                    // (no store write, no event; `read` folds both cases
                    // into the same honest `Ok(None)`).
                    Ok(None) => {}
                    Ok(Some(file)) => {
                        let sync = metrics_file_sync(p, &file);
                        let summary = self.store.sync_metrics_file(sync).await?;
                        self.emit(Event::ProjectUpdated(p));
                        self.emit(Event::ConnectorSynced {
                            name: "metrics.toml".into(),
                            ok: true,
                            detail: format!(
                                "北极星 · {} 条滞后指标 · {} 条引领指标已同步",
                                summary.lagging_synced, summary.leading_synced
                            ),
                        });
                    }
                    Err(e) => {
                        // 坏 toml:如实报错,沿用 ConnectorSynced ok:false 惯例;
                        // `read` only returns `Err` on a parse/IO failure — the
                        // cache is untouched (nothing was written above).
                        self.emit(Event::ConnectorSynced {
                            name: "metrics.toml".into(),
                            ok: false,
                            detail: e.to_string(),
                        });
                    }
                }
            }

            Command::CollectMetrics => {
                let p = self.active()?;
                let s = self.collect_project_metrics(p).await?;
                // Honest toast: any gh failure — or a github metric with no
                // remote to pull from — is ok:false. Never claim a green
                // collection we didn't actually perform.
                let ok = s.failed == 0 && !s.no_remote_github;
                let mut detail = format!(
                    "采集 · {} 更新 · {} 未变 · {} 未接(bw/connector 留白)",
                    s.changed, s.unchanged, s.deferred
                );
                if s.no_remote_github {
                    detail.push_str(";项目未挂 GitHub 仓,github 指标无法采集");
                }
                if let Some(err) = &s.first_error {
                    detail.push_str(&format!(";首个失败:{err}"));
                }
                self.emit(Event::ConnectorSynced {
                    name: "指标采集".into(),
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
                        phases,
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

                // C5 · PR 验收环 (plan/13 D3): a repo-attached, GitHub-mapped
                // Issue quarantines the run onto `bw/issue-<n>` BEFORE the
                // executor touches anything, so the executor never advances the
                // base branch — only a human merge does. No-repo / unmapped
                // projects (`pr_eligible` false) never touch git here → today's
                // behavior byte-for-byte. A branch-checkout failure degrades
                // honestly: the run proceeds on the current branch, no PR is
                // opened (提 PR 失败不炸 run).
                let pr_eligible = !proj.github_remote.trim().is_empty()
                    && issue.github_number != 0
                    && !proj.workspace_path.trim().is_empty();
                let on_issue_branch = if pr_eligible {
                    match bw_engine::github::checkout_issue_branch(
                        std::path::Path::new(proj.workspace_path.trim()),
                        issue.github_number,
                    )
                    .await
                    {
                        Ok(_) => true,
                        Err(e) => {
                            self.emit(Event::ConnectorSynced {
                                name: format!("#{} · 活分支", issue.number),
                                ok: false,
                                detail: format!("开活分支失败,本次运行在当前分支、不提 PR:{e}"),
                            });
                            false
                        }
                    }
                } else {
                    false
                };

                // Run through the same path as any run, bound to this issue.
                let run = self
                    .run_workflow_inner(p, session, spec, RunTrigger::Manual, None, Some(id))
                    .await;
                match run {
                    Ok(()) => {
                        // C5: on the issue branch → try to open the PR (提 PR).
                        // Success stores the PR number and lets the Issue reach
                        // InReview (which now *derives from an open PR*, D3).
                        // Failure fires an honest toast and leaves the Issue at
                        // InProgress — retryable via RunIssue, never faked into
                        // review with no PR behind it.
                        let opened_pr = if on_issue_branch {
                            match bw_engine::github::open_pr(
                                std::path::Path::new(proj.workspace_path.trim()),
                                issue.github_number,
                                &issue.title,
                            )
                            .await
                            {
                                Ok(pr) => {
                                    self.store.set_issue_pr_number(id, pr).await?;
                                    self.emit(Event::ConnectorSynced {
                                        name: format!("#{} · PR", issue.number),
                                        ok: true,
                                        detail: format!(
                                            "已提 PR #{pr}(Closes #{}),等待人工 merge 验收",
                                            issue.github_number
                                        ),
                                    });
                                    true
                                }
                                Err(e) => {
                                    self.emit(Event::ConnectorSynced {
                                        name: format!("#{} · PR", issue.number),
                                        ok: false,
                                        detail: format!("提 PR 失败,活留在进行中可重试:{e}"),
                                    });
                                    false
                                }
                            }
                        } else {
                            false
                        };
                        // InReview iff there's really something to review: an
                        // open PR (pr issues), or — for no-repo/unmapped issues
                        // — the run succeeding (today's meaning, unchanged). A
                        // pr_eligible issue whose PR failed stays InProgress.
                        if !pr_eligible || opened_pr {
                            self.store
                                .transition_issue(id, IssueStatus::InReview)
                                .await?;
                        }
                        self.refresh_issues().await?;
                        self.emit(Event::IssuesChanged);
                    }
                    Err(e) => {
                        // Honest failure: the issue stays InProgress (not faked
                        // to InReview/Done). Done remains a human TransitionIssue.
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
                            phases,
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
                            source: LibSource::SelfBuilt,
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
                self.store
                    .update_skill(
                        id,
                        SkillEdit {
                            name,
                            desc,
                            category,
                            content,
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
                        maturity: Maturity::Polishing,
                        skills,
                        model,
                        instructions,
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
                self.store
                    .update_agent(
                        id,
                        AgentEdit {
                            name,
                            role,
                            skills,
                            model,
                            instructions,
                        },
                    )
                    .await?;
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
                        title: title.clone(),
                        desc: desc.clone(),
                        priority,
                    })
                    .await?;
                // C4: 项目挂了 GitHub 仓时,建单同时经 gh 真开一个 GitHub
                // issue;github_remote 为空的项目在这里直接短路返回,今天的
                // 行为一个字节不变。
                self.sync_issue_to_github(p, id, &title, &desc).await?;
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

            Command::MergeIssuePr { id } => {
                let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
                // Idempotent short-circuit BEFORE any gh call: an already-Done
                // / already-settled Issue is a no-op — a re-dispatch never
                // re-merges (so `gh pr merge` stays called exactly once) and
                // never re-accounts (settle-once already stands).
                if issue.status == IssueStatus::Done || issue.settled_at.is_some() {
                    return Ok(());
                }
                if issue.pr_number == 0 {
                    return Err(AppError::Invalid(format!(
                        "#{} 没有 PR,无法 merge;无 PR 的活用 TransitionIssue 显式完成",
                        issue.number
                    )));
                }
                // 评审中由开放 PR 派生 (D3): only an InReview PR issue is
                // merge-acceptable.
                if issue.status != IssueStatus::InReview {
                    return Err(AppError::Invalid(format!(
                        "#{} 处于{},不在评审中,不能 merge",
                        issue.number,
                        issue.status.label()
                    )));
                }
                let proj = self
                    .store
                    .get_project(issue.project_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                let remote = proj.github_remote.trim().to_string();
                if remote.is_empty() {
                    return Err(AppError::Invalid(format!(
                        "#{} 的项目未挂 GitHub 仓,无法 merge PR",
                        issue.number
                    )));
                }
                // merge PR — the human验收 action, the ONLY place `gh pr merge`
                // is ever called (never from any executor/run path; plan/13
                // D3+D11).
                if let Err(e) = bw_engine::github::merge_pr(&remote, issue.pr_number).await {
                    // 绝不反向改写:a merge failure (including the drift case of
                    // a PR already merged on the web) is only reflected — the
                    // Issue stays InReview and retryable, nothing is settled.
                    self.emit(Event::ConnectorSynced {
                        name: format!("#{} · merge", issue.number),
                        ok: false,
                        detail: format!("merge PR #{} 失败,活留在评审中:{e}", issue.pr_number),
                    });
                    return Ok(());
                }
                // Settle Done through the EXISTING TransitionIssue InReview→Done
                // path — settle-once accounting reused verbatim, no second
                // accounting path. (Box::pin: `dispatch` recurses into itself;
                // TransitionIssue never re-enters MergeIssuePr, so it's bounded.)
                Box::pin(self.dispatch(Command::TransitionIssue {
                    id,
                    status: IssueStatus::Done,
                }))
                .await?;
                // issue 关闭是 merge 的后果: `Closes #<n>` should have closed it;
                // verify and补关 idempotently if GitHub didn't. Never reopen,
                // never fight drift.
                match bw_engine::github::issue_state(&remote, issue.github_number).await {
                    Ok(state) if state.eq_ignore_ascii_case("OPEN") => {
                        if let Err(e) =
                            bw_engine::github::close_issue(&remote, issue.github_number).await
                        {
                            self.emit(Event::ConnectorSynced {
                                name: format!("#{} · 关单", issue.number),
                                ok: false,
                                detail: format!("PR 已 merge,但补关 GitHub issue 失败:{e}"),
                            });
                        }
                    }
                    _ => {}
                }
                self.emit(Event::ConnectorSynced {
                    name: format!("#{} · 验收", issue.number),
                    ok: true,
                    detail: format!(
                        "已 merge PR #{},#{} 验收完成",
                        issue.pr_number, issue.number
                    ),
                });
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

/// C7 · 采集器 receipt — an honest tally of one collection pass (manual or
/// cron). Real counts, so a caller can toast the truth and a failure is never
/// silently swallowed. `changed` vs `unchanged` prove the change-guard held;
/// `failed` with `first_error` prove a `gh` failure wrote nothing; `deferred`
/// proves the bw/connector 留白; `no_remote_github` proves github can't be faked
/// without a repo.
#[derive(Default)]
struct MetricCollectSummary {
    changed: u32,
    unchanged: u32,
    failed: u32,
    deferred: u32,
    first_error: Option<String>,
    no_remote_github: bool,
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
fn run_params_snapshot(spec: &WorkflowSpec, trigger: RunTrigger) -> String {
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

/// C6: `bw_engine::metrics_file::MetricsFile` (parsed toml) → `MetricsFileSync`
/// (the store's write shape). Pure reshaping — no validation here, `read`
/// already guaranteed every metric carries a `collect` plan by the time this
/// runs (a file missing one fails to parse, never reaches this function).
fn metrics_file_sync(
    project_id: ProjectId,
    file: &bw_engine::metrics_file::MetricsFile,
) -> MetricsFileSync {
    let to_def = |m: &bw_engine::metrics_file::MetricDef| MetricDefSync {
        name: m.name.clone(),
        def: m.def.clone(),
        target_raw: m.target.clone(),
        collect_kind: m.collect.kind.as_str().to_string(),
        collect_query: m.collect.query.clone(),
    };
    MetricsFileSync {
        project_id,
        north_star_name: file.north_star.name.clone(),
        north_star_def: file.north_star.def.clone(),
        north_star_collect_kind: file.north_star.collect.kind.as_str().to_string(),
        north_star_collect_query: file.north_star.collect.query.clone(),
        lagging: file.lagging.iter().map(to_def).collect(),
        leading: file.leading.iter().map(to_def).collect(),
    }
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
