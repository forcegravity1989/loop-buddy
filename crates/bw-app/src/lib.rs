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
//!
//! File layout (2026-08-17 mechanical split; `impl App` is spread over the
//! child modules, each `use super::*` and adds nothing new):
//! - `lib.rs` — module docs, imports, [`AppState`]/[`App`] and their private
//!   run/settle types, `App::new` + builders + `refresh_*`, free helper fns.
//! - `command.rs` — [`Command`]/[`Event`] and the view enums (`pub use`d here).
//! - `dispatch.rs` — [`App::dispatch`], the single Command → use-case router.
//! - `issue_run.rs` — Issue run lifecycle (prepare → interactive run → settle → cancel).
//! - `terminal.rs` — embedded terminal / session focus / hook events / in-review poll.
//! - `scheduler.rs` — cron tick, autopilot (creates issues, never completes them).
//! - `metrics.rs` — metric seeding, `.bw/metrics.toml` sync, connector collection.
//! - `project_sync.rs` — workspace probe, GitHub/CodeHub issue sync, assets, artifacts.
//! - `prompts.rs` — prompt blocks injected into teammate sessions.
//! - `workflow_engine.rs` — the old chat-style workflow engine (retirement: docs/BACKLOG.md #1).

#![forbid(unsafe_code)]

mod agent_import;
mod buddy_materialize;
mod bw_canon;
mod hook_listener;
mod skill_import;
mod skill_materialize;

mod command;
pub use command::*;
mod dispatch;
mod issue_run;
mod metrics;
mod project_sync;
mod prompts;
mod scheduler;
mod terminal;
mod workflow_engine;

use bw_core::derive::AmberBand;
use bw_core::model::{
    classify_artifact_path, cron_due, parse_phase_outcome, parse_workflow_phases, stage_workflow,
    stage_workflow_with_playbook, workflow_parse_contract_suffix, AgentCard, AgentRef, Artifact,
    Author, Cadence, Connector, ConnectorStatus, CronMode, CronStatus, CronTask, HubSource, Issue,
    IssuePriority, IssueStatus, KnowledgeSource, LoopConfig, Maturity, MaturityPeriod, PhaseMeta,
    PhaseRole, Readiness, RunStatus, RunTrigger, SkillCard, SkillRef, SourceKind, StageKind,
    Verdict, WorkflowKind, WorkflowSpec, BW_PROJECT_ASSETS_LIBRARY, BW_STANDARD_LIBRARY,
    CONNECTOR_KIND_CLAUDE_CLI, CONNECTOR_KIND_CODEHUB_REPO, CONNECTOR_KIND_GITHUB_REPO,
    CONNECTOR_KIND_GIT_REPO, CONNECTOR_KIND_SCRIPT,
};
use bw_core::stage_catalog::StageOrigin;
use bw_core::{
    AgentId, ArtifactId, ConnectorId, ConversationId, CronTaskId, IssueId, KnowledgeSourceId,
    MetricId, ProjectId, SessionId, SkillId, WorkflowId, WorkflowRunId,
};
use bw_engine::{
    allowed_tools_arg, build_consultation_resume_plan, build_project_context_block,
    build_resume_plan, build_startup_plan, evidence, ClaudeCliConfig, ClaudeCliExecutor,
    CodehubRepoSummary, ConversationMeta, Engine, GitCommit, GithubRepoSummary,
    InteractiveCliExecutor, InteractiveExecutor, MockInteractiveExecutor, PermissionMode,
    PhaseNode, ProjectFile, RunCtx, RunEvent, SkillOutput, TerminalManager, UnsupportedCliExecutor,
    CLAUDE,
};
use bw_store::{
    AgentEdit, ConnectorDefSync, ConnectorsFileSync, GlobalHandoffRow, MetricDefSync, MetricRole,
    MetricsFileSync, NewAgent, NewArtifact, NewConnector, NewCronTask, NewIssue,
    NewKnowledgeSource, NewMetric, NewProject, NewSession, NewSkill, NewSkillFile, NewStage,
    NewWorkflowSpec, ProjectFileSync, ProjectRow, SessionKind, SkillEdit, Store, WorkflowEdit,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use time::{Date, Month, OffsetDateTime, Time};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

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

/// codehub 对接(P2):bw-app 调 `Remote::for_project` / `Remote::xxx` 时把
/// [`bw_engine::remote::RemoteError`] 归进既有 `Engine(String)` 口径——
/// github 臂的 `GithubError` 与 codehub 臂的未接线拒斥都走这一条。
impl From<bw_engine::remote::RemoteError> for AppError {
    fn from(e: bw_engine::remote::RemoteError) -> Self {
        AppError::Engine(e.to_string())
    }
}

/// plan/16 §2 防线 1 (S1): the one name-format guard all three skill-writing
/// commands (`CreateSkill`/`UpdateSkill`/`DistillSkillFromIssue`) share — one
/// rule, one error message, no hand-copied variants drifting apart.
/// `ImportSkillPackage`/`ImportSkillLibrary` deliberately do NOT call this:
/// external library text enters verbatim (分域规则), its violations surface
/// as Advisory findings instead.
fn guard_skill_name(name: &str) -> Result<(), AppError> {
    if !bw_core::skill_spec::is_valid_skill_name(name) {
        return Err(AppError::Invalid(
            "技能名须为 1-64 字符的小写 kebab-case(字母/数字/单连字符,如 evidence-first)——plan/16 S1"
                .into(),
        ));
    }
    Ok(())
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

/// plan/17 S3: everything `run_round_loop` needs to drive one issue-run's
/// adversarial round loop, and everything `finalize_run` needs to settle the
/// after-loop accounting. Built once on the main thread (`prepare_run`) so the
/// long round loop can run on a `tokio::spawn` with NO `&mut App` (the loop
/// only ever touched `self.store`/`self.emit` — shared borrows — per the
/// long-standing `lib.rs` comment; S3 just makes that structural). The owned
/// `Engine` is what unblocked the extraction: mock path clones the shared
/// `mock_engine`'s `Arc`, real path clones a fresh one-shot executor's `Arc`.
#[derive(Clone)]
struct PreparedRun {
    engine: Engine,
    spec: WorkflowSpec,
    ctx: RunCtx,
    params_json: String,
    eval_idx: Option<usize>,
    num_phases: usize,
    max_iter: u32,
    /// One past the last phase index the loop runs each round (through the
    /// gate, inclusive; or `num_phases` when there's no gate).
    range_end: usize,
    heads_workspace: String,
    head_before: Option<String>,
    proj: ProjectRow,
    p: ProjectId,
    session: SessionId,
    issue_id: Option<IssueId>,
    trigger: RunTrigger,
    cron_task_id: Option<CronTaskId>,
}

/// plan/17 S3: the slice of [`PreparedRun`] that `finalize_run` reads after
/// the loop returns — kept on the main thread (in [`ActiveRun`]) so the
/// after-loop accounting (change window, agent/skill usage, artifact scan)
/// runs under `&mut self`, exactly as it did inline. The `Engine` + loop
/// locals are NOT here — they lived inside the spawn and are gone by settle.
#[derive(Clone)]
struct FinalizeCtx {
    spec: WorkflowSpec,
    proj: ProjectRow,
    p: ProjectId,
    issue_id: Option<IssueId>,
}

/// plan/17 S3 + V1 Issue2 Phase1: outcome a backgrounded run reports back
/// to the main thread via the settle mpsc. The phase-loop variant carries
/// `LoopEnd` + the last round's `workflow_run` id + `final_run_ok`; the
/// interactive variant carries a `SkillOutput` (one-shot interactive
/// session, no phase loop). `Err` in either is a `?`-early-bail.
/// `pub(crate)` — the kernel only ever MOVES a `SettleReq` value, never
/// names/constructs `SettleOutcome`.
pub(crate) enum SettleOutcome {
    Interactive(Result<SkillOutput, AppError>),
    /// 咨询 PTY 退出:不占 active_run、不改状态、不 settle-once。
    ConsultationEnded {
        conversation_id: ConversationId,
    },
}

/// plan/17 S3: outcome a backgrounded round loop reports back to the main
/// thread via the settle mpsc. Sent from the spawned task; received by the
/// kernel's `select!` settle arm.
pub struct SettleReq {
    pub(crate) project: ProjectId,
    pub(crate) issue: IssueId,
    // Private-typed on purpose: `SettleOutcome` carries internal types
    // (`LoopEnd`). The kernel only ever MOVES a `SettleReq` value
    // (channel → `run_issue_settle`), never names/constructures it — so
    // the fields stay crate-internal while the type itself is pub (the
    // kernel names it for the mpsc type param).
    pub(crate) outcome: SettleOutcome,
}

/// V1 收口:issue ▶跑 全走嵌入终端(`run_issue_interactive`)后,issue 脚本
/// 调度路径退场。`ActiveRun` 只服务交互式交付(same-project 串行锁 +
/// `CancelRun` 的 `abort` + settle 的 `finalize_run_interactive(_resume)` +
/// worktree guard)。`proj`/`issue_ws`/`pr_eligible` 三字段曾服务 `issue_run_tail`
/// 的 create_mr/transition,随该函数一并删去。`finalize: FinalizeCtx` 承载 settle
/// 所需的 spec/proj/issue_id;`is_resume` 选 finalize 分支(settle-once)。
struct ActiveRun {
    project: ProjectId,
    /// The bound Issue (id + number + github_number + title) — used to match
    /// `CancelRun`'s id and settle's conversation lookup.
    issue: Issue,
    handle: JoinHandle<()>,
    guard: bw_engine::workspace::IssueWorktreeGuard,
    finalize: FinalizeCtx,
    /// V1 Issue2 Phase2a: whether this run is a resume (not the first run).
    /// Set from the conversation's claude_session_id at dispatch time. The settle arm
    /// uses it to pick `finalize_run_interactive_resume` (artifact scan only,
    /// no uses bump — settle-once) vs `finalize_run_interactive` (first run:
    /// uses + artifacts). Issue stays in its current state (never auto-Done).
    is_resume: bool,
}

/// 咨询态 PTY(Done/InReview 续聊)。不进 active_run。
struct ConsultationRun {
    issue_id: IssueId,
    #[allow(dead_code)] // 后续若加「关咨询」可 abort
    handle: JoinHandle<()>,
    guard: bw_engine::workspace::IssueWorktreeGuard,
}

/// V1 收口:the shared 起手 prefix of an issue-run, returned by
/// `prepare_issue_run` so the interactive path (`run_issue_interactive`,
/// first-run + resume branches) starts from the right setup — get+validate
/// the Issue, build the stage-playbook `WorkflowSpec` (+ standard/distilled
/// skill injection), transition to InProgress, and provision the isolated
/// issue worktree behind an RAII guard. The guard is owned here so the caller
/// can place it where the worktree must outlive (the interactive path's
/// `ActiveRun`, or the consultation path's `ConsultationRun`).
struct IssueRunPrep {
    issue: Issue,
    proj: ProjectRow,
    spec: WorkflowSpec,
    issue_ws: Option<PathBuf>,
    guard: bw_engine::workspace::IssueWorktreeGuard,
    /// 同阶段蒸馏技能块(最多 3 条,经验复利)。V1 收口:interactive 路径
    /// 并进系统提示词(原来只进 phase-loop 的 spec.prompt,issue 全转终端
    /// 后会静默丢失)。
    distilled_block: String,
    /// 本阶段技能目录块(工作区为空时为空,守「不假装物化」)。同上并入系统提示词。
    catalog_block: String,
}

/// plan/17 S3: `AppState` no longer `derive(Clone, Debug)` — `active_run`
/// now holds an `ActiveRun` with a `JoinHandle` (not `Clone`) and a
/// worktree guard (not `Debug`), so the blanket derive breaks. Nothing
/// clones `AppState` wholesale (`snapshot()` returns `&AppState`; every
/// reader clones only the field it needs), and nothing formats it — so the
/// derives were unused baggage. Drop them rather than fake-impl the trait.
pub struct AppState {
    pub view: View,
    pub panel: Panel,
    pub scope: Scope,
    pub active_project: Option<ProjectId>,
    pub active_session: Option<SessionId>,
    /// plan/17 S1: same-project serial run lock. `Some` while a run is
    /// in-flight on that project — blocks a second `RunIssue` (or C8
    /// 「立即开工」) on the SAME project until the in-flight run settles.
    /// Cross-project parallel stays allowed (`plan/05` §3.5 models
    /// project-level parallelism only; single-project multi-issue was never
    /// designed). plan/17 S3: now carries the spawned round-loop `JoinHandle`
    /// (for `CancelRun`'s `abort`) + everything `run_issue_settle` needs to
    /// finalize + tail on the main thread. The lock is real post-S3 — the
    /// backgrounded run stays in-flight across dispatch returns, so without
    /// this a same-project second `RunIssue` would race two worktrees. In-memory
    /// only — a crashed run leaves it set, but a restart re-seeds `None`.
    /// Crate-private type (carries a `JoinHandle` + guard that can't cross
    /// the crate boundary cleanly); the UI reads it via [`App::active_run`].
    pub(crate) active_run: Option<ActiveRun>,
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
    /// V2-② Intent UX: last remote `.bw/project.toml` probe for the create
    /// flow's「接入已有仓」path. Process-local; cleared on「新建仓」.
    pub remote_project_probe: RemoteProjectProbe,
    /// V1 Issue2 Phase2a: unix ts of the last InReview poll for interactive
    /// issues. The poller checks codehub/github for open MRs on interactive
    /// issues that are InProgress + have a claude_conversation row (interactive) + `pr_number == 0`.
    /// Adaptive throttle: ~15s while candidates wait for an open MR, 5 min
    /// when idle — so a just-opened MR is not stuck behind a multi-minute
    /// backstop. `0` = never polled (first tick runs it).
    pub last_inreview_poll: i64,
    /// Set when `tick_scheduler` mutates UI-visible state without firing a
    /// cron (notably InReview poll / Stop-triggered MR detection). The
    /// desktop kernel rebuilds Vm when this is true even if `fired` is empty
    /// — otherwise the board stays stale while toasts already say「评审中」.
    pub scheduler_ui_dirty: bool,
    /// V1 Issue2 Phase2b: cwd → IssueId map for hook event routing. When
    /// `run_issue_interactive` spawns a session, it registers the worktree
    /// cwd here. When the hook listener receives a SessionStart/Stop event
    /// (which carries `cwd`), `poll_hook_events` looks up the issue by cwd
    /// to store `session_id` or trigger InReview detection. Entries are
    /// dropped by `forget_interactive_session` when the run settles or is
    /// cancelled — a session that's over must not keep a cwd registered, or a
    /// late/forged hook event could still rewrite that issue's session id.
    pub interactive_sessions: HashMap<String, IssueId>,
    /// V1 Issue2 Phase2b: a `Stop` hook event was received since the last
    /// `tick_scheduler` fire. `true` → run `poll_interactive_inreview` on
    /// this tick (the tick's cadence is the natural throttle — no additional
    /// debounce needed within a single tick; the 5-minute poller remains as
    /// backstop for Stop events processed with delay).
    pub pending_stop_check: bool,
    pub terminal_manager: TerminalManager,
    /// UI 焦点会话(切卡只改这个,不杀其它 PTY)。
    pub focused_conversation: Option<ConversationId>,
    pub focused_issue: Option<IssueId>,
    /// 咨询态活连接;与 active_run 并存,不占交付名额。
    consultation_runs: HashMap<ConversationId, ConsultationRun>,
    /// V1-TermRefactor4 · 重启恢复:点卡 resume 进行中(重建 worktree →
    /// spawn PTY → 首包字节前)。UI 显示「恢复中…」;Boot 绝不批量唤醒。
    pub pty_restoring: Option<ConversationId>,
    /// V1 Issue2 Phase2b: whether PTY mode is enabled (the desktop kernel
    /// wires it via `App::with_pty`). When `true`, `run_issue_interactive`
    /// uses `run_skill_pty` (in-app terminal). When `false`, it uses the
    /// old `run_skill` (system terminal / mock).
    pub pty_enabled: bool,
    /// CodeHub 为主体的创建流: last `Command::ListCodehubRepos` result. Same
    /// process-internal cache pattern as `github_repos` — a direct read-through
    /// of `codehub-cli project list --mine`, not a derived Signal.
    pub codehub_repos: Vec<CodehubRepoSummary>,
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
    /// V1 终端会话重构(阶段1): 该活是否已有交互式会话(claude_conversation
    /// 行存在与否)。替代旧 `issue.interactive_started` —— P2 诚实文案用它
    /// 判断「过程在嵌入终端里」还是「还没有运行」。
    pub is_interactive: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            view: View::Projects,
            panel: Panel::Progress,
            scope: Scope::All,
            active_project: None,
            active_session: None,
            active_run: None,
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
            remote_project_probe: RemoteProjectProbe::Idle,
            last_inreview_poll: 0,
            scheduler_ui_dirty: false,
            interactive_sessions: HashMap::new(),
            pending_stop_check: false,
            terminal_manager: TerminalManager::new(),
            focused_conversation: None,
            focused_issue: None,
            consultation_runs: HashMap::new(),
            pty_restoring: None,
            pty_enabled: false,
            codehub_repos: Vec::new(),
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
    /// plan/17 S3: back-channel a backgrounded issue-run uses to hand its
    /// outcome back to the main thread. `None` (default — every example /
    /// headless driver) keeps `RunIssue` INLINE (the old blocking behavior
    /// those callers rely on: `dispatch(RunIssue)` settles before returning).
    /// `Some` (the desktop kernel wires it via [`App::with_settle_channel`])
    /// backgrounds the issue run: `run_issue_now` spawns the round loop and
    /// returns immediately, and the kernel's `select!` settle arm later
    /// drives `run_issue_settle` under `&mut self` — the UI never freezes.
    settle_tx: Option<mpsc::UnboundedSender<SettleReq>>,
    /// V1 Issue2 Phase2b: hook event receiver from the hook listener
    /// (localhost HTTP server). `None` when the listener failed to start
    /// (port in use, no home dir) — the app works without real-time hooks,
    /// falling back to 2a's 5-minute InReview poller. Drained in
    /// [`App::poll_hook_events`] (called from `tick_scheduler`).
    hook_event_rx: Option<mpsc::UnboundedReceiver<hook_listener::HookEvent>>,
    /// V1 Issue2 Phase2b: the port the hook listener bound to. Written into
    /// `~/.claude/settings.json`'s curl commands. `None` when the listener
    /// isn't running.
    hook_port: Option<u16>,
}

impl App {
    pub fn new(store: Arc<dyn Store>, mock_engine: Engine, claude_config: ClaudeCliConfig) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        // V1 Issue2 Phase2b: start the hook listener (localhost HTTP server
        // that receives claude SessionStart/Stop hook events). Best-effort —
        // if it fails (port in use, no home dir, no tokio runtime), the app
        // works without real-time hooks (falls back to 2a's 5-minute InReview
        // poller). Sync `bind()` gets the port immediately (no `block_on`
        // needed — safe to call inside a tokio runtime like the desktop
        // kernel). The accept loop is spawned if a runtime is available.
        let (hook_tx, hook_rx) = mpsc::unbounded_channel::<hook_listener::HookEvent>();
        let (hook_port, hook_event_rx) = match hook_listener::HookListener::bind() {
            Ok((port, listener)) => {
                // Write hooks config to ~/.claude/settings.json (idempotent
                // merge — preserves user hooks). If this fails, the listener
                // is useless (curl has nowhere to POST).
                if hook_listener::install_hooks_config(port).is_ok() {
                    // Spawn the accept loop (needs a tokio runtime). Inside
                    // the desktop kernel's runtime, this works; in
                    // examples/headless (no runtime), the listener is dropped.
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle.spawn(async move {
                            hook_listener::HookListener::spawn(listener, hook_tx);
                        });
                        (Some(port), Some(hook_rx))
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            }
            Err(_) => (None, None),
        };
        Self {
            store,
            mock_engine,
            state: AppState {
                claude_config,
                ..AppState::default()
            },
            events: tx,
            workspaces_root: None,
            settle_tx: None,
            hook_event_rx,
            hook_port,
        }
    }

    /// plan/17 S3: wire the back-channel that turns `RunIssue` from a
    /// blocking inline call into a backgrounded one. The desktop kernel
    /// owns the matching `mpsc::UnboundedReceiver` and polls it in its
    /// `select!` loop; each settle drives `run_issue_settle` + a `Vm`
    /// rebuild. Returns `self` for chaining with `with_workspaces_root`.
    /// Once set, the same-project serial lock (`active_run`) becomes the
    /// real guard (a backgrounded run stays in-flight across dispatch
    /// returns). Examples / headless drivers never call this → `RunIssue`
    /// stays inline, byte-for-byte the pre-S3 behavior they depend on.
    pub fn with_settle_channel(mut self, tx: mpsc::UnboundedSender<SettleReq>) -> Self {
        self.settle_tx = Some(tx);
        self
    }

    /// Enable all-in-one-codebase auto-provisioning: every project completed
    /// through the creation flow gets its own real git repo under `root`
    /// (created + `git init` + first commit + a bound `git-repo` connector),
    /// so the five roles have a real substrate from birth instead of Mock.
    pub fn with_workspaces_root(mut self, root: PathBuf) -> Self {
        self.workspaces_root = Some(root);
        self
    }

    /// V1 Issue2 Phase2b: enable PTY mode — `run_issue_interactive` spawns
    /// `claude` in a PTY (portable-pty) instead of a system terminal, and
    /// streams bytes via a dedicated `watch` channel / `Command::TerminalInput`.
    /// The desktop kernel calls this; examples / headless drivers don't
    /// (they use the old system-terminal / mock path).
    pub fn with_pty(mut self) -> Self {
        self.state.pty_enabled = true;
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

    /// plan/17 S3: the in-flight backgrounded run's (project, issue), or
    /// `None` when nothing's running. The UI uses this to show a 「⬇ 终止」
    /// button on exactly the issue whose run is in flight (and to grey-out
    /// 「▶ 跑」 for same-project siblings while the serial lock holds).
    /// Returns a `Copy` tuple — the `JoinHandle` / guard inside `ActiveRun`
    /// stay crate-internal.
    pub fn active_run(&self) -> Option<(ProjectId, IssueId)> {
        self.state
            .active_run
            .as_ref()
            .map(|ar| (ar.project, ar.issue.id))
    }

    /// Borrow the store (for read queries the UI projects through selectors).
    pub fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    /// V1 Issue2 Phase2b: the port the hook listener is bound to, or `None`
    /// when the listener isn't running (port in use, no home dir, no runtime).
    /// The UI can show this for debugging; `~/.claude/settings.json`'s curl
    /// commands embed it.
    pub fn hook_port(&self) -> Option<u16> {
        self.hook_port
    }

    /// Desktop kernel: after `tick_scheduler`, rebuild Vm if the tick mutated
    /// issues/connectors without returning a non-empty cron `fired` list.
    pub fn take_scheduler_ui_dirty(&mut self) -> bool {
        std::mem::take(&mut self.state.scheduler_ui_dirty)
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

    /// plan/16 §2 防线 1 (S2): the skill name is the join key
    /// (`SkillRef` / `agent.skills` / 蒸馏溯源 all match by name), so a
    /// duplicate is ambiguity, not a style problem. `exempt` = the row being
    /// renamed itself (an unchanged-name `UpdateSkill` must not self-collide).
    /// Read against the store, not `self.state` — same stale-UI reasoning as
    /// `UpdateSkill`'s T11 flip check.
    ///
    /// plan/20 R4: 唯一性按**作用域**强制——全局一池、每项目一池(`scope` =
    /// 这行将要落在的池)。跨作用域允许同名:那是「收录=复制归我」的天然
    /// 结果,按名引用处由就近规则(R2 `scope::scoped_pick`)确定性消歧。
    async fn guard_skill_name_unique(
        &self,
        name: &str,
        scope: Option<ProjectId>,
        exempt: Option<SkillId>,
    ) -> Result<(), AppError> {
        let taken = self
            .store
            .list_skills()
            .await?
            .iter()
            .any(|s| s.name == name && s.project_id == scope && Some(s.id) != exempt);
        if taken {
            return Err(AppError::Invalid(format!(
                "技能名「{name}」已存在——名字在同一作用域内是联合键,不容歧义(plan/16 S2 · plan/20 R4)"
            )));
        }
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
}

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// V1 Issue2 Phase2a: InReview poller throttle while interactive issues are
/// waiting for an open MR. Rides `tick_scheduler` (~5s); 15s keeps detection
/// in the "seconds–teens of seconds" band without flooding the remote every
/// tick. Must stay ≥ the desktop ticker interval.
const INREVIEW_POLL_ACTIVE_SECS: i64 = 15;
/// Idle backstop when no InProgress+conversation+pr_number==0 candidates
/// exist — avoid hammering codehub/github on quiet projects.
const INREVIEW_POLL_IDLE_SECS: i64 = 5 * 60;

/// Interval helper (testable): active while candidates wait, idle otherwise.
fn inreview_poll_interval_secs(has_candidates: bool) -> i64 {
    if has_candidates {
        INREVIEW_POLL_ACTIVE_SECS
    } else {
        INREVIEW_POLL_IDLE_SECS
    }
}

/// plan18-③ · `script` connector 的 config 解析结构。config 是 JSON 字符串,
/// 存项目仓里既有采集脚本的相对工作区路径 + 输出文件 + 跑脚本的命令。
#[derive(Debug, Clone, Default)]
struct ScriptConnectorConfig {
    script: String,
    output: String,
    /// 跑脚本的命令(`python` / `ts-node` / `node` …),空则默认 `python`。
    command: String,
}

impl ScriptConnectorConfig {
    /// 从 connector `config` JSON 字符串解析(script/output/command 三字段,
    /// 缺的当空串,不硬性要求都填——采集时会再校验 script 非空)。
    fn from_config(s: &str) -> serde_json::Result<Self> {
        let v: serde_json::Value = serde_json::from_str(s)?;
        let get = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        Ok(Self {
            script: get("script"),
            output: get("output"),
            command: get("command"),
        })
    }
}

/// plan18-③ · 按点分路径从一个 JSON 里取字段值(供 `script` 指标的
/// `collect_query` 取回脚本输出里的某条指标)。兼容 skill 写的
/// `field:leading.L1` / `data.json:leading.L1` / 裸 `leading.L1` 三种写法,
/// 一律取点分路径。
fn json_field_by_path<'a>(v: &'a serde_json::Value, raw: &str) -> Option<&'a serde_json::Value> {
    let path = raw.trim();
    let path = path.strip_prefix("field:").unwrap_or(path).trim();
    let path = path.strip_prefix("data.json:").unwrap_or(path).trim();
    if path.is_empty() {
        return None;
    }
    let mut cur = v;
    for seg in path.split('.') {
        cur = cur.get(seg)?;
    }
    Some(cur)
}

/// C7 · 采集器 receipt — an honest tally of one collection pass (manual or
/// cron). Real counts, so a caller can toast the truth and a failure is never
/// silently swallowed. `changed` vs `unchanged` prove the change-guard held;
/// `failed` with `first_error` proves a script failure wrote nothing;
/// `deferred` proves a legacy definition or missing script output stayed blank.
#[derive(Default)]
struct MetricCollectSummary {
    changed: u32,
    unchanged: u32,
    failed: u32,
    deferred: u32,
    first_error: Option<String>,
}

impl MetricCollectSummary {
    /// A green collection must have measured at least one real value and left
    /// no failures or deferred definitions behind. Manual metrics are outside
    /// this collector, so an all-manual project correctly reports no auto-collect.
    fn is_success(&self) -> bool {
        self.failed == 0 && self.deferred == 0 && self.changed + self.unchanged > 0
    }
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

/// W3-9: tell apart a buddy-provisioned workspace clone (under
/// `workspaces_root`, named `<slug>-<uuid8hex>` — safe to delete with the
/// project) from a user-bound pre-existing directory (the user's own path,
/// which buddy must never delete). The judgment is path-based: `project`
/// has no `is_bound` column, so this is the only reliable discriminator.
fn is_buddy_built_clone(
    workspace_path: &str,
    name: &str,
    id: ProjectId,
    workspaces_root: Option<&std::path::Path>,
) -> bool {
    let ws = workspace_path.trim();
    if ws.is_empty() {
        return false;
    }
    let Some(root) = workspaces_root else {
        return false;
    };
    let path = std::path::Path::new(ws);
    if !path.starts_with(root) {
        return false;
    }
    match path.file_name().and_then(|n| n.to_str()) {
        Some(name_on_disk) => name_on_disk == workspace_slug(name, id),
        None => false,
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

/// V1 Issue 1 phase2 · Buddy-owned repo-stats collector for a remote-backed
/// project. Writes `.bw/collect_stats.sh` (launcher) + `.bw/collect_stats.py`
/// (real collector: today scalars + `history` of the last 30 calendar days).
/// CreateProject and every `collect_project_metrics` call refresh these files
/// so already-onboarded workspaces pick up Phase B without re-creating.
fn write_buddy_collect_stats(proj: &ProjectRow) {
    let root = proj.workspace_path.trim();
    if root.is_empty() || proj.remote_path.trim().is_empty() {
        return;
    }
    let bw_dir = Path::new(root).join(".bw");
    let _ = std::fs::create_dir_all(&bw_dir);
    let _ = std::fs::write(
        bw_dir.join("collect_stats.py"),
        build_collect_stats_py(proj),
    );
    let _ = std::fs::write(bw_dir.join("collect_stats.sh"), build_collect_stats_sh());
}

/// Thin sh launcher — collect arm keeps `command: sh` + `.bw/collect_stats.sh`
/// for存量 connectors; delegates to the Python collector when available.
fn build_collect_stats_sh() -> String {
    r#"#!/bin/sh
# BW 自带采集脚本 — 委托 .bw/collect_stats.py(当日值 + 近 30 天 history)
# CreateProject / CollectMetrics 会覆盖本文件,勿手改。
ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd) || exit 1
cd "$ROOT" || exit 1
run_py() {
  if command -v py >/dev/null 2>&1; then py -3 "$@"
  elif command -v python3 >/dev/null 2>&1; then python3 "$@"
  elif command -v python >/dev/null 2>&1; then python "$@"
  else return 1
  fi
}
run_py .bw/collect_stats.py
"#
    .to_string()
}

/// Python body for Buddy 仓统计. Emits:
/// ```json
/// {"open_issues": N, "merged_mrs": M,
///  "history": {"open_issues":[{"ts":"YYYY-MM-DD","v":n},…],
///              "merged_mrs":[{"ts":"YYYY-MM-DD","v":m},…]}}
/// ```
/// `history` covers today and the previous 29 days (30 points). open_issues
/// as-of day D = created on/before D and not closed on/before D; merged_mrs
/// as-of D = merged_at date ≤ D.
fn build_collect_stats_py(proj: &ProjectRow) -> String {
    match proj.provider.as_str() {
        "codehub" => format!(
            r#"#!/usr/bin/env python3
# BW 自带采集 — codehub 仓统计(当日 + 近 30 天 history)。勿手改;Buddy 会覆盖。
from __future__ import annotations

import json
import subprocess
import sys
from datetime import date, datetime, timedelta
from pathlib import Path

HOST = "{host}"
PATH_NS = "{path}"
OUT = Path(".bw/collect_stats.json")
HISTORY_DAYS = 30


def run(argv: list[str]) -> str:
    try:
        p = subprocess.run(argv, capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return ""
    if p.returncode != 0:
        return ""
    return (p.stdout or "").strip()


def load_json(raw: str):
    if not raw:
        return []
    try:
        v = json.loads(raw)
    except json.JSONDecodeError:
        return []
    if isinstance(v, list):
        return v
    if isinstance(v, dict):
        for k in ("items", "list", "data", "values"):
            if isinstance(v.get(k), list):
                return v[k]
    return []


def parse_dt(s: str | None):
    if not s:
        return None
    s = s.strip()
    if s.endswith("Z"):
        s = s[:-1] + "+00:00"
    try:
        return datetime.fromisoformat(s)
    except ValueError:
        return None


def as_date(dt: datetime | None) -> date | None:
    if dt is None:
        return None
    if dt.tzinfo is not None:
        return dt.astimezone().date()
    return dt.date()


def open_count_as_of(issues, d: date) -> int:
    n = 0
    for iss in issues:
        created = as_date(parse_dt(iss.get("created_at")))
        if created is None or created > d:
            continue
        closed = as_date(parse_dt(iss.get("closed_at")))
        if closed is not None and closed <= d:
            continue
        n += 1
    return n


def merged_count_as_of(mrs, d: date) -> int:
    n = 0
    for mr in mrs:
        merged = as_date(parse_dt(mr.get("merged_at")))
        if merged is not None and merged <= d:
            n += 1
    return n


def main() -> int:
    issues_raw = run([
        "codehub-cli", "-H", HOST, "issue", "list", "-p", PATH_NS,
        "--state", "all", "-l", "0", "-f", "json",
        "--json", "id,state,created_at,closed_at",
    ])
    mrs_raw = run([
        "codehub-cli", "-H", HOST, "mr", "list", "-p", PATH_NS,
        "--state", "merged", "-l", "0", "-f", "json",
        "--json", "id,merged_at",
    ])
    issues = load_json(issues_raw)
    mrs = load_json(mrs_raw)
    today = date.today()
    hist_oi = []
    hist_mm = []
    for i in range(HISTORY_DAYS - 1, -1, -1):
        d = today - timedelta(days=i)
        hist_oi.append({{"ts": d.isoformat(), "v": open_count_as_of(issues, d)}})
        hist_mm.append({{"ts": d.isoformat(), "v": merged_count_as_of(mrs, d)}})
    out = {{
        "open_issues": hist_oi[-1]["v"] if hist_oi else 0,
        "merged_mrs": hist_mm[-1]["v"] if hist_mm else 0,
        "history": {{"open_issues": hist_oi, "merged_mrs": hist_mm}},
    }}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
"#,
            host = proj.remote_host,
            path = proj.remote_path,
        ),
        _ => format!(
            r#"#!/usr/bin/env python3
# BW 自带采集 — GitHub 仓统计(当日 + 近 30 天 history)。勿手改;Buddy 会覆盖。
from __future__ import annotations

import json
import subprocess
import sys
from datetime import date, datetime, timedelta
from pathlib import Path

REPO = "{path}"
OUT = Path(".bw/collect_stats.json")
HISTORY_DAYS = 30


def run(argv: list[str]) -> str:
    try:
        p = subprocess.run(argv, capture_output=True, text=True, check=False)
    except FileNotFoundError:
        return ""
    if p.returncode != 0:
        return ""
    return (p.stdout or "").strip()


def load_json(raw: str):
    if not raw:
        return []
    try:
        v = json.loads(raw)
    except json.JSONDecodeError:
        return []
    return v if isinstance(v, list) else []


def parse_dt(s: str | None):
    if not s:
        return None
    s = s.strip()
    if s.endswith("Z"):
        s = s[:-1] + "+00:00"
    try:
        return datetime.fromisoformat(s)
    except ValueError:
        return None


def as_date(dt: datetime | None) -> date | None:
    if dt is None:
        return None
    if dt.tzinfo is not None:
        return dt.astimezone().date()
    return dt.date()


def open_count_as_of(issues, d: date) -> int:
    n = 0
    for iss in issues:
        created = as_date(parse_dt(iss.get("createdAt")))
        if created is None or created > d:
            continue
        closed = as_date(parse_dt(iss.get("closedAt")))
        if closed is not None and closed <= d:
            continue
        n += 1
    return n


def merged_count_as_of(prs, d: date) -> int:
    n = 0
    for pr in prs:
        merged = as_date(parse_dt(pr.get("mergedAt")))
        if merged is not None and merged <= d:
            n += 1
    return n


def main() -> int:
    issues = load_json(run([
        "gh", "issue", "list", "--repo", REPO, "--state", "all",
        "--limit", "1000", "--json", "createdAt,closedAt",
    ]))
    prs = load_json(run([
        "gh", "pr", "list", "--repo", REPO, "--state", "merged",
        "--limit", "1000", "--json", "mergedAt",
    ]))
    today = date.today()
    hist_oi = []
    hist_mm = []
    for i in range(HISTORY_DAYS - 1, -1, -1):
        d = today - timedelta(days=i)
        hist_oi.append({{"ts": d.isoformat(), "v": open_count_as_of(issues, d)}})
        hist_mm.append({{"ts": d.isoformat(), "v": merged_count_as_of(prs, d)}})
    out = {{
        "open_issues": hist_oi[-1]["v"] if hist_oi else 0,
        "merged_mrs": hist_mm[-1]["v"] if hist_mm else 0,
        "history": {{"open_issues": hist_oi, "merged_mrs": hist_mm}},
    }}
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
"#,
            path = proj.remote_path,
        ),
    }
}

/// Parse `history.<field>` from a script output JSON into `(day, value)` pairs.
/// Missing / malformed history → `None` (caller falls back to today-only).
fn history_series_for_field(out: &serde_json::Value, field: &str) -> Option<Vec<(Date, String)>> {
    let field = field.trim();
    if field.is_empty() {
        return None;
    }
    // Accept `history.open_issues` or top-level history + collect_query `open_issues`.
    let path = field.strip_prefix("history.").unwrap_or(field);
    let arr = out.pointer(&format!("/history/{path}"))?.as_array()?;
    let mut series = Vec::with_capacity(arr.len());
    for item in arr {
        let ts = item.get("ts")?.as_str()?;
        let day = parse_ymd(ts)?;
        let v = item.get("v")?;
        let value = match v {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        series.push((day, value));
    }
    if series.is_empty() {
        None
    } else {
        Some(series)
    }
}

fn parse_ymd(s: &str) -> Option<Date> {
    let mut parts = s.trim().split('-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: u8 = parts.next()?.parse().ok()?;
    let d: u8 = parts.next()?.parse().ok()?;
    let month = Month::try_from(m).ok()?;
    Date::from_calendar_date(y, month, d).ok()
}

fn date_at_noon_utc(day: Date) -> OffsetDateTime {
    OffsetDateTime::new_utc(day, Time::from_hms(12, 0, 0).expect("valid noon"))
}

/// PF1-R5c · Windows: app 进程的 PATH 可能缺 Git 的 bin/usr-bin(sh.exe/bash.exe
/// 所在),`Command::new("sh")` 报 program not found。从 PATH 里的 git.exe 位置
/// 推导(系统 PATH 通常有 `<root>\cmd\git.exe`):`<root>\bin\bash.exe` 或
/// `<root>\usr\bin\sh.exe`。返回首个存在的全路径。
fn resolve_script_interpreter_via_git() -> Option<String> {
    if !cfg!(windows) {
        return None;
    }
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(';') {
        let dir_path = Path::new(dir);
        if !dir_path.join("git.exe").exists() {
            continue;
        }
        // git.exe 可能在 <root>\cmd\ 或 <root>\mingw64\bin\ 或 <root>\bin\。
        // 试 root = dir_path 与 dir_path.parent(),找 bin\bash.exe / usr\bin\sh.exe。
        let roots = [
            dir_path.to_path_buf(),
            dir_path.parent().unwrap_or(dir_path).to_path_buf(),
        ];
        for root in &roots {
            let bash = root.join("bin").join("bash.exe");
            if bash.exists() {
                return Some(bash.to_string_lossy().into_owned());
            }
            let sh = root.join("usr").join("bin").join("sh.exe");
            if sh.exists() {
                return Some(sh.to_string_lossy().into_owned());
            }
        }
    }
    None
}

/// PF1-R5c · collect arm 跑 .sh 脚本的解释器候选链(Windows 专用增强)。
/// 顺序:BW_SH_BIN env 强制 → config command + 裸名 sh/bash/sh.exe/bash.exe
/// (PATH 搜)→ 从 git.exe 推导的全路径 → 常见安装位兜底。collect arm 试到
/// 第一个 spawn 成功的为止(NotFound 才换下一个)。
fn script_interpreter_candidates(command: &str) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let push = |v: &mut Vec<String>, s: String| {
        if !v.contains(&s) {
            v.push(s);
        }
    };
    if cfg!(windows) {
        if let Ok(p) = std::env::var("BW_SH_BIN") {
            let p = p.trim();
            if !p.is_empty() {
                push(&mut v, p.to_string());
            }
        }
    }
    push(&mut v, command.to_string());
    if cfg!(windows) {
        // P5 · 2026-08-06 real-world incident: a fresh Windows machine may
        // have the `py` launcher (installed by python.org's installer by
        // default) on PATH but not a bare `python`/`python3` — the default
        // script command (`ScriptConnectorConfig::from_config` defaults
        // empty `command` to `"python"`) then fails NotFound with no
        // fallback. `py` (no version arg) launches the newest installed
        // Python, same semantics as bare `python` for a single-version box.
        if matches!(command, "python" | "python3") {
            push(&mut v, "py".to_string());
        }
        for c in ["bash", "sh.exe", "bash.exe"] {
            push(&mut v, c.to_string());
        }
        if let Some(bash) = resolve_script_interpreter_via_git() {
            push(&mut v, bash);
        }
        for p in [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files\Git\usr\bin\sh.exe",
            r"C:\Program Files (x86)\Git\bin\bash.exe",
        ] {
            push(&mut v, p.to_string());
        }
    }
    v
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

/// V2-② Phase A (§6.1): render the project's intent as `.bw/project.toml`
/// content — the five fields a creation flow collects. The first-comer
/// writes this into the repo as the canonical intent (later-comers read it
/// back via `sync_project_file_for`). North star is **not** here (it lives in
/// `.bw/metrics.toml`). Returns `None` if serialization fails (shouldn't
/// happen for string-only data, but honest rather than panicking).
fn project_toml_content(proj: &ProjectRow) -> Option<String> {
    let file = bw_engine::project_file::ProjectFile {
        name: proj.name.clone(),
        kind: proj.kind.clone(),
        brief: proj.desc.clone(),
        benchmark: proj.benchmark.clone(),
        opportunity: proj.opportunity.clone(),
    };
    bw_engine::project_file::render(&file).ok()
}

/// V2-② Phase A (§6): check whether the workspace already has
/// `.bw/project.toml` — the first-comer/later-comer判据. Empty workspace →
/// false (no file = first-comer, same as a workspace that simply has none
/// yet).
fn has_project_toml(workspace: &str) -> bool {
    !workspace.trim().is_empty()
        && std::path::Path::new(workspace)
            .join(bw_engine::project_file::PROJECT_FILE_REL_PATH)
            .exists()
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

/// desc 的第一句,按字符数截断。技能的 description 上限 1024 字符,整句进目录
/// 会把 prompt 撑得没法读;第一句恰好是触发段所在。
fn first_sentence_capped(desc: &str, cap: usize) -> String {
    let head = desc
        .split(['。', '\n'])
        .next()
        .unwrap_or(desc)
        .split(". ")
        .next()
        .unwrap_or(desc)
        .trim();
    if head.chars().count() <= cap {
        return head.to_string();
    }
    let cut: String = head.chars().take(cap).collect();
    format!("{cut}…")
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

/// V1 Issue2 Phase 3: `bw_engine::connectors_file::ConnectorsFile` (parsed
/// toml) → `ConnectorsFileSync` (the store's write shape). Pure reshaping —
/// no validation here, `read` already guaranteed every connector has a valid
/// `kind = "script"` and `name`/`script` by the time this runs (a file
/// missing those fails to parse, never reaches this function). The `config`
/// column is the JSON `{script, command, output}` that
/// `ScriptConnectorConfig::from_config` parses back — matches the existing
/// connector `config` format.
fn connectors_file_sync(
    project_id: ProjectId,
    file: &bw_engine::connectors_file::ConnectorsFile,
) -> ConnectorsFileSync {
    let connectors = file
        .connectors
        .iter()
        .map(|c| {
            let config = serde_json::json!({
                "script": c.script,
                "command": c.command,
                "output": c.output,
            })
            .to_string();
            ConnectorDefSync {
                name: c.name.clone(),
                config,
            }
        })
        .collect();
    ConnectorsFileSync {
        project_id,
        connectors,
    }
}

/// V2-② Phase A: `bw_engine::project_file::ProjectFile` (parsed toml) →
/// `ProjectFileSync` (the store's write shape). Pure reshaping — no
/// validation here, `read` already guaranteed every field is present (a file
/// missing `name`/`kind` fails to parse, never reaches this function).
fn project_file_sync(
    project_id: ProjectId,
    file: &bw_engine::project_file::ProjectFile,
) -> ProjectFileSync {
    ProjectFileSync {
        project_id,
        name: file.name.clone(),
        kind: file.kind.clone(),
        brief: file.brief.clone(),
        benchmark: file.benchmark.clone(),
        opportunity: file.opportunity.clone(),
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
