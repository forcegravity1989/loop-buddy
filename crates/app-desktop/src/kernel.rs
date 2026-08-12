//! The UI⇄kernel bridge. A dedicated thread owns the [`App`] (and its tokio
//! runtime, so sqlx never depends on Dioxus's executor); the UI talks to it
//! through three runtime-agnostic channels:
//!
//! * `mpsc`   — commands in (fire-and-forget from event handlers),
//! * `watch`  — the latest [`Vm`] out (rebuilt after every dispatch),
//! * `broadcast` — transient [`UiNote`]s (live run progress, errors) that are
//!   not part of persistent state.
//!
//! The Vm is assembled from **store reads + `ui` pure builders** only. Nothing
//! in here invents data: trends are observation history, signals come from the
//! persisted derive cache (`None` ⇒ Unknown), feeds are real records, stage
//! methodology text is `StageKind`'s own static metadata.

use bw_app::{ActionState, App, Command, Event, Panel, RemoteProjectProbe, Scope, SettleReq, View};
use bw_core::model::{
    AgentRef, Author, CronMode, HubCard, IssueStatus, MaturityPeriod, Readiness, SessionStatus,
    Signal, SkillRef, StageKind, CONNECTOR_KIND_SCRIPT,
};
use bw_core::{ConversationId, MetricId, SessionId};
use bw_engine::{
    ClaudeCliConfig, CodehubRepoSummary, Engine, GithubRepoSummary, MockExecutor, PermissionMode,
};
use bw_store::{MetricRole, SqliteStore, Store};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, watch};
use ui::vm::{
    activity_row, agent_card, attention_from_rows, cadence_label, collect_label, connector_card,
    cron_row, hub_overview, is_intrinsic_metric, issue_card, knowledge_row, last_week_has_real_obs,
    metric_vm, notify_feed, observation_feed, project_card, session_status_label, settings_vm,
    skill_card, stage_detail, stage_nav, version_log_vm, week_plan_rows, weekly_delta,
    weekly_spark, workflow_hub_row, ActivityRowVm, ActivitySource, AgentCardVm, CollectionChainVm,
    ConnectorCardVm, CronRowVm, FeedItemVm, FeedSource, IssueVm, KnowledgeRowVm, MetricVm,
    NotifyItemVm, ProjectCardVm, SessionCardVm, SettingsVm, SkillCardVm, StageDetailVm,
    StageNavItemVm, WeekPlanRowVm, WorkflowHubRowVm,
};
use ui::{overall_progress, Attention};

// ───────────────────────────── view model ─────────────────────────────

#[derive(Clone, PartialEq, Default)]
pub struct Vm {
    /// False until the first kernel build lands (renders a quiet boot frame).
    pub ready: bool,
    /// Set when the store cannot open — the app is unusable, say so plainly.
    pub fatal: Option<String>,
    pub view: View,
    pub projects: Vec<ProjectCardVm>,
    pub create: Option<CreateVm>,
    pub op: Option<OpVm>,
    /// Hub library — global, built unconditionally (no active project
    /// required), unlike `create`/`op`.
    pub hub: HubVm,
    /// The real, editable `ClaudeCliConfig` (Settings hub) — also global.
    pub settings: SettingsVm,
    /// L1(plan/11): last-loaded cron task's real fire history — lives at the
    /// top level (not `OpVm`) because the component-detail overlay that
    /// shows it is rendered outside any one project's `Op` tree, same as
    /// `hub`. `None` until `Command::LoadCronEffectiveness` runs for a task.
    pub cron_effectiveness: Option<(bw_core::CronTaskId, ui::vm::CronEffectivenessVm)>,
    /// GitHub 为主体的创建流: last `Command::ListGithubRepos` result — lives
    /// at the top level (not `CreateVm`) because the Repo 卡片 renders before
    /// any project row exists. Empty until the Repo 卡片 first dispatches
    /// `ListGithubRepos` (switching to "接入已有仓").
    pub github_repos: Vec<GithubRepoSummary>,
    /// CodeHub 为主体的创建流: last `Command::ListCodehubRepos` result —
    /// same process-internal cache pattern as `github_repos`, populated by
    /// the Repo 卡片's「接入已有仓」refresh button dispatching
    /// `ListCodehubRepos{host}`.
    pub codehub_repos: Vec<CodehubRepoSummary>,
    /// V2-② Intent UX: remote `.bw/project.toml` probe for「接入已有仓」.
    pub remote_project_probe: RemoteProjectProbe,
    /// plan/17 S3: the in-flight backgrounded run's (project, issue), or
    /// `None` when nothing's running. The issue board uses this to show a
    /// 「⬇ 终止」 button on exactly the issue whose run is in flight, and to
    /// keep 「▶ 跑」 greyed for same-project siblings (serial lock).
    pub active_run: Option<(bw_core::ProjectId, bw_core::IssueId)>,
    /// 是否有活着的 PTY 连接。
    pub pty_active: bool,
    /// 当前焦点会话(驱动可见 xterm)。
    pub pty_conversation_id: Option<ConversationId>,
    /// 所有活着的会话(多 xterm 常驻用)。
    pub pty_live_ids: Vec<ConversationId>,
    /// 点卡 resume 进行中(「恢复中…」);首包字节后清空。
    pub pty_restoring: Option<ConversationId>,
    /// 焦点对应的 issue(看板「当前会话」)。
    pub focused_issue: Option<bw_core::IssueId>,
    /// Done/InReview 且有可 resume 会话的 issue(看板「续聊」)。
    pub consultable_issues: Vec<bw_core::IssueId>,
    /// 任意状态、有非空 claude_session_id 的 issue(重启后点卡可 resume)。
    pub resumable_issues: Vec<bw_core::IssueId>,
}

/// The Workflow/Skill/Agent hub library, plus the 3-card "从 Hub 导入"
/// overview. Rebuilt on every dispatch, same as `projects` — at this scale
/// (tens to low hundreds of rows, all already in memory via `AppState`) that
/// costs nothing extra; revisit if a later hub's row count changes that.
#[derive(Clone, PartialEq, Default)]
pub struct HubVm {
    pub workflows: Vec<WorkflowHubRowVm>,
    /// Full detail (prompt + real agent/skill provenance tuples) per
    /// `workflows` row — a separate parallel list rather than folded into
    /// `WorkflowHubRowVm` itself, since the row list is what every filter/
    /// group pass iterates and most consumers only need the summary.
    /// `workflow_detail` already existed (unit-tested) but was never wired
    /// to a screen until now.
    pub workflow_details: Vec<ui::vm::WorkflowDetailVm>,
    pub skills: Vec<SkillCardVm>,
    pub agents: Vec<AgentCardVm>,
    pub overview: Vec<HubCard>,
    pub cron_tasks: Vec<CronRowVm>,
    pub connectors: Vec<ConnectorCardVm>,
    pub knowledge_sources: Vec<KnowledgeRowVm>,
    /// Cross-project audit feed — real `handoff` rows, newest first.
    pub activity: Vec<ActivityRowVm>,
    /// Derived from flipped signals already visible above (no table of its
    /// own) — failed cron tasks, errored connectors, risky/clean handoffs.
    pub notifications: Vec<NotifyItemVm>,
}

/// The creation flow's real, persisted-so-far draft (screen-local navigation
/// state — which card is showing — lives in the screen, not here).
#[derive(Clone, PartialEq)]
pub struct CreateVm {
    pub name: String,
    pub kind: String,
    /// The free-text brief (stored as the project's `desc`).
    pub brief: String,
    pub cycle: MaturityPeriod,
    pub benchmark: String,
    /// 三个月后怎样算成 (stored in the `opportunity` column).
    pub win: String,
    pub north_star: String,
    pub ns_def: String,
    pub leading: Vec<MetricVm>,
    pub lagging: Vec<MetricVm>,
    /// "owner/repo" — empty = this project isn't attached to GitHub (Repo 卡
    /// 片选了本地/失败软降级). C8: the Review 卡's「立即让队友开工第一件?」
    /// checkbox only renders when this is non-empty — a remote_path-empty
    /// project gets zero standard Issues at `CompleteCreation`, so a visible
    /// checkbox there would be dead UI.
    pub remote_path: String,
    pub remote_host: String,
}

#[derive(Clone, PartialEq)]
pub struct StageVm {
    pub kind: StageKind,
    pub n: u8,
    pub progress: u8,
    pub trend: Vec<f32>,
    pub schedule_label: String,
    pub health: Signal,
    pub metrics: Vec<MetricVm>,
    pub feed: Vec<FeedItemVm>,
    pub detail: StageDetailVm,
}

#[derive(Clone, PartialEq)]
pub struct MsgVm {
    pub agent: bool,
    pub text: String,
}

#[derive(Clone, PartialEq)]
pub struct ChatVm {
    pub id: SessionId,
    pub title: String,
    pub status_label: &'static str,
    pub msgs: Vec<MsgVm>,
}

#[derive(Clone, PartialEq)]
pub struct OpVm {
    pub id: bw_core::ProjectId,
    pub name: String,
    pub kind: String,
    /// P9: one-line project description (`project.descr`) — was written once
    /// at `CreateProject` and never surfaced back to the operating view;
    /// needed so the「编辑项目」card can show + edit it.
    pub desc: String,
    pub project_signal: Signal,
    pub cycle: MaturityPeriod,
    pub active_stage: StageKind,
    pub north_star: String,
    pub ns_def: String,
    /// P9: 对标竞品 / 机会缺口 —— real creation-flow inputs, editable via
    /// `Command::UpdateBrief`. Surfaced here so 「编辑项目」 has something to
    /// show/edit outside the creation flow (previously only reachable from
    /// `create.rs`, unreachable post-creation).
    pub benchmark: String,
    pub opportunity: String,
    /// Real-executor target directory. Empty = unconfigured — this project
    /// only ever runs `RunWorkflow` on `MockExecutor`.
    pub workspace_path: String,
    pub allow_commands: bool,
    /// "owner/repo" — empty = this project isn't attached to GitHub (local-
    /// only workspace, or the GitHub attach attempt failed and soft-degraded).
    pub remote_path: String,
    pub remote_host: String,
    /// `"github"` / `"codehub"` — drives provider-aware remote links in the
    /// issue board (issue/MR web URLs) so a codehub project doesn't show a
    /// hardcoded `github.com` link. Empty = legacy default (treated as github).
    pub provider: String,
    pub panel: Panel,
    pub scope: Scope,
    pub nav: Vec<StageNavItemVm>,
    pub attention: Attention,
    pub archived: usize,
    pub stages: Vec<StageVm>,
    pub metrics: Vec<MetricVm>,
    pub week_plan: Vec<WeekPlanRowVm>,
    pub overall: u8,
    pub sessions: Vec<SessionCardVm>,
    /// The project's Issues (R1) — assignable work units scoped to a stage,
    /// the multica-style board the operating view now surfaces.
    pub issues: Vec<IssueVm>,
    pub chat: Option<ChatVm>,
    /// Threaded down for the "从 Hub 导入" overview strip in the Workflow
    /// panel — same data as the top-level `Vm.hub`, just also reachable from
    /// deep inside `Op`'s component tree without re-prop-drilling `Vm` itself.
    pub hub: HubVm,
    /// Real `git log` for this project's `workspace_path` (Version panel).
    /// `NotLoaded` until `Command::LoadVersionLog` is dispatched at least
    /// once for this specific project.
    pub version_log: ui::vm::VersionLogVm,
    /// Registered artifacts (Artifact panel) — `None` until
    /// `Command::LoadArtifacts` ran for this project; `Some(vec![])` is a
    /// really-empty registry, a different honest state.
    pub artifacts: Option<Vec<ui::vm::ArtifactRowVm>>,
    /// P4: the Issue-detail overlay, `None` = closed. Opened per-issue by
    /// `Command::OpenIssueDetail`, cleared by `Command::CloseIssueDetail`.
    pub issue_detail: Option<ui::vm::IssueDetailVm>,
    /// plan/17 S3: the in-flight backgrounded run's (project, issue), or
    /// `None`. Threaded from [`Vm::active_run`] (`App::active_run`) so the
    /// issue board can show a 「⬇ 终止」 button on the running card + keep
    /// 「▶ 跑」 greyed for same-project siblings while the serial lock holds.
    pub active_run: Option<(bw_core::ProjectId, bw_core::IssueId)>,
    /// P5: weekly-review card (top of the progress panel).
    pub week_review: ui::vm::WeekReviewVm,
    pub pty_active: bool,
    pub pty_conversation_id: Option<ConversationId>,
    pub pty_live_ids: Vec<ConversationId>,
    /// 点卡 resume 进行中 → UI「恢复中…」。
    pub pty_restoring: Option<ConversationId>,
    pub focused_issue: Option<bw_core::IssueId>,
    pub consultable_issues: Vec<bw_core::IssueId>,
    /// 有可 `--resume` 会话的 issue(含 InProgress;点卡「恢复中…」用)。
    pub resumable_issues: Vec<bw_core::IssueId>,
}

/// Transient, non-persistent notices (live run progress, dispatch errors).
#[derive(Clone, Debug, PartialEq)]
pub enum UiNote {
    /// A run is really about to begin — the canonical "new run, reset the
    /// banner" signal (not the `PhaseStarted{idx:0}` heuristic this replaced),
    /// carrying the spec's own real name/agents/skills.
    RunStarted {
        workflow_name: String,
        agents: Vec<AgentRef>,
        skills: Vec<SkillRef>,
    },
    PhaseStarted {
        idx: usize,
        name: String,
    },
    PhaseCompleted {
        idx: usize,
    },
    RunDone,
    RunFailed(String),
    Handoff {
        from: StageKind,
        to: StageKind,
        risky: bool,
    },
    Error(String),
    /// A real, unattended scheduler auto-fire just finished (see
    /// `App::tick_scheduler`) — surfaced as a toast, never a navigation:
    /// unlike a manual "▶ 立即执行", nothing about the user's current screen
    /// should change just because a background task ran.
    CronAutoFired {
        name: String,
        ok: bool,
    },
    /// New artifact versions were really registered (post-run auto-scan or a
    /// manual collect) — `fresh` is the genuinely-new count.
    ArtifactsRegistered {
        fresh: u32,
    },
    /// A connector's real probe finished — `detail` is its honest summary.
    ConnectorSynced {
        name: String,
        ok: bool,
        detail: String,
    },
    /// plan/14 C14 · mirrors `bw_app::Event::ActionProgress` verbatim — see
    /// its doc comment. Folded into [`ActionsVm`] by `ActionsVm::apply`.
    ActionProgress {
        name: String,
        state: ActionState,
    },
}

/// Folded run-progress state the UI renders as the live banner. Fed purely by
/// [`UiNote`]s — it reflects what the engine actually reported, nothing more.
#[derive(Clone, PartialEq, Default)]
pub struct RunVm {
    pub running: bool,
    /// The spec name currently (or most recently) running — empty until the
    /// first `RunStarted`.
    pub workflow_name: String,
    /// Real `AgentRef`/`SkillRef` from the spec that's running — empty is
    /// honest ("this run declared none"), not a loading state.
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    /// (phase name, completed) in start order.
    pub phases: Vec<(String, bool)>,
    pub failed: Option<String>,
}

impl RunVm {
    pub fn apply(&mut self, note: &UiNote) {
        match note {
            UiNote::RunStarted {
                workflow_name,
                agents,
                skills,
            } => {
                self.running = true;
                self.workflow_name = workflow_name.clone();
                self.agents = agents.clone();
                self.skills = skills.clone();
                self.phases.clear();
                self.failed = None;
            }
            UiNote::PhaseStarted { name, .. } => {
                self.running = true;
                self.phases.push((name.clone(), false));
            }
            UiNote::PhaseCompleted { idx } => {
                if let Some(p) = self.phases.get_mut(*idx) {
                    p.1 = true;
                }
            }
            UiNote::RunDone => self.running = false,
            UiNote::RunFailed(e) => {
                self.running = false;
                self.failed = Some(e.clone());
            }
            UiNote::Handoff { .. }
            | UiNote::Error(_)
            | UiNote::CronAutoFired { .. }
            | UiNote::ArtifactsRegistered { .. }
            | UiNote::ConnectorSynced { .. }
            | UiNote::ActionProgress { .. } => {}
        }
    }
}

// ─────────────────────── plan/14 C14 · action progress ───────────────────────

/// The "how long is too long to stay silent" line for a creation-flow
/// background action (体验规范条 2). An action that starts *and* resolves
/// before this elapses never renders a pending indicator at all — 秒级内
/// 完成的动作不闪烁噪音. Shared by every render-time visibility decision
/// over [`ActionItem`] (see `screens::create::visible_actions`).
pub const ACTION_PENDING_THRESHOLD: Duration = Duration::from_millis(800);
/// How long a resolved-`Ok` indicator lingers before the render layer stops
/// showing it (可淡出) — short, since success needs no reading time.
pub const ACTION_OK_LINGER: Duration = Duration::from_millis(2200);
/// How long a resolved-`Fail` indicator lingers — longer than `Ok`'s, so
/// there's real time to read the reason (原因入口); the paired
/// `UiNote::ConnectorSynced` toast (unchanged, still fires) is the
/// persistent, dismissible record once this fades.
pub const ACTION_FAIL_LINGER: Duration = Duration::from_secs(6);

/// One creation-flow background action's raw progress — the real facts a
/// `Started`/`Ok`/`Fail` triple reported, with real wall-clock timestamps.
/// Visibility (is it past the pending threshold? has its linger window
/// expired?) is a render-time decision computed from these against
/// `Instant::now()` — this struct never decides, only records.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionItem {
    pub name: String,
    pub started_at: std::time::Instant,
    /// `(ok, detail, resolved_at)` once the real call has finished; `None`
    /// while still running.
    pub resolved: Option<(bool, String, std::time::Instant)>,
}

/// Folded background-action state the creation flow's progress strip
/// renders (plan/14 C14). Fed purely by `UiNote::ActionProgress` — every
/// entry is a real Started/Ok/Fail triple from a real `gh`-backed call,
/// never synthesized. Bounded in practice to the handful of actions one
/// creation flow fires (repo create/clone, repo list, the 3-Issue trio,
/// landing push) — no pruning needed for a single session.
#[derive(Clone, PartialEq, Default)]
pub struct ActionsVm {
    pub items: Vec<ActionItem>,
}

impl ActionsVm {
    pub fn apply(&mut self, note: &UiNote) {
        let UiNote::ActionProgress { name, state } = note else {
            return;
        };
        match state {
            ActionState::Started => {
                // A retry reuses the same `name` — replace, don't duplicate,
                // so the strip never shows a stale failed attempt alongside
                // its retry.
                self.items.retain(|i| &i.name != name);
                self.items.push(ActionItem {
                    name: name.clone(),
                    started_at: std::time::Instant::now(),
                    resolved: None,
                });
            }
            ActionState::Ok(detail) => {
                if let Some(i) = self.items.iter_mut().find(|i| &i.name == name) {
                    i.resolved = Some((true, detail.clone(), std::time::Instant::now()));
                }
            }
            ActionState::Fail(detail) => {
                if let Some(i) = self.items.iter_mut().find(|i| &i.name == name) {
                    i.resolved = Some((false, detail.clone(), std::time::Instant::now()));
                }
            }
        }
    }
}

// ───────────────────────────── the bridge ─────────────────────────────

#[derive(Clone)]
pub struct Kernel {
    tx: mpsc::UnboundedSender<Command>,
    vm_rx: watch::Receiver<Vm>,
    notes: broadcast::Sender<UiNote>,
    /// V1 终端会话重构·底座: 带 conversation_id 的 PTY 字节批次(dedicated
    /// watch,NOT the Vm)。pty_ticker 发送;UI 按 id 写入对应 xterm。
    pty_rx: watch::Receiver<Vec<(ConversationId, Vec<u8>)>>,
}

impl Kernel {
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }
    pub fn vm(&self) -> watch::Receiver<Vm> {
        self.vm_rx.clone()
    }
    pub fn notes(&self) -> broadcast::Receiver<UiNote> {
        self.notes.subscribe()
    }
    /// V1 终端会话重构·底座: 带 conversation_id 的字节批次 watch。
    pub fn pty_bytes(&self) -> watch::Receiver<Vec<(ConversationId, Vec<u8>)>> {
        self.pty_rx.clone()
    }
}

fn db_path() -> String {
    if let Ok(p) = std::env::var("BW_DB") {
        return p;
    }
    let base = if cfg!(target_os = "macos") {
        std::env::var("HOME")
            .map(|h| format!("{h}/Library/Application Support/BuildersWorkbench"))
            .ok()
    } else if cfg!(target_os = "windows") {
        std::env::var("APPDATA")
            .map(|a| format!("{a}\\BuildersWorkbench"))
            .ok()
    } else {
        std::env::var("HOME")
            .map(|h| format!("{h}/.local/share/builders-workbench"))
            .ok()
    };
    match base {
        Some(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            format!("{dir}/workbench.db")
        }
        None => "workbench.db".into(),
    }
}

/// Where auto-provisioned project repos live: `BW_WORKSPACES` override, else
/// a `workspaces/` directory next to the database — same env-override-else-
/// derived pattern as [`db_path`].
fn workspaces_root() -> std::path::PathBuf {
    if let Ok(p) = std::env::var("BW_WORKSPACES") {
        return std::path::PathBuf::from(p);
    }
    let db = std::path::PathBuf::from(db_path());
    match db.parent() {
        Some(dir) => dir.join("workspaces"),
        None => std::path::PathBuf::from("workspaces"),
    }
}

/// Process-wide `ClaudeCliExecutor` config, env-override-else-default (same
/// pattern as [`db_path`]). Per-project data (`workspace_path`/
/// `allow_commands`) lives in the store instead — see `Command::SetWorkspace`.
fn claude_config_from_env() -> ClaudeCliConfig {
    let mut config = ClaudeCliConfig::default();
    if let Ok(bin) = std::env::var("BW_CLAUDE_BIN") {
        config.binary = Some(bin);
    }
    if let Ok(cap) = std::env::var("BW_CLAUDE_MAX_BUDGET_USD") {
        if let Ok(v) = cap.parse() {
            config.max_budget_usd = v;
        }
    }
    config
}

/// Spawn the kernel thread. Returns immediately; the first real [`Vm`] arrives
/// on the watch channel once `Boot` has run.
pub fn spawn() -> Kernel {
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<Command>();
    // plan/17 S3: the back-channel a backgrounded issue-run uses to hand its
    // outcome back to this main loop (so `RunIssue` can return before the
    // long claude spawn finishes — the UI never freezes). Polled by the
    // `select!` settle arm below; each settle drives `run_issue_settle`
    // under `&mut app` + a `Vm` rebuild, exactly like a dispatched command.
    let (settle_tx, mut settle_rx) = mpsc::unbounded_channel::<SettleReq>();
    let (vm_tx, vm_rx) = watch::channel(Vm::default());
    let (note_tx, _keep) = broadcast::channel::<UiNote>(256);
    let notes = note_tx.clone();
    // V1 终端会话重构·底座: 带 conversation_id 的字节通道(替代无身份单槽)。
    let (pty_tx, pty_rx) = watch::channel(Vec::<(ConversationId, Vec<u8>)>::new());

    std::thread::Builder::new()
        .name("bw-kernel".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("kernel runtime");
            rt.block_on(async move {
                let real_db_path = db_path();
                let store: Arc<dyn Store> = match SqliteStore::open(&real_db_path).await {
                    Ok(s) => Arc::new(s),
                    Err(e) => {
                        let _ = vm_tx.send(Vm {
                            ready: true,
                            fatal: Some(format!("本地数据库打开失败:{e}")),
                            ..Vm::default()
                        });
                        return;
                    }
                };
                // MockExecutor with visible latency: the run flow must *stream*
                // in the UI (per-phase), not land as one burst. This is the
                // shared, long-lived engine every project without a configured
                // workspace_path runs on; a configured project instead gets a
                // fresh ClaudeCliExecutor built per-call inside bw-app's
                // RunWorkflow dispatch.
                let mut app = App::new(
                    store.clone(),
                    Engine::new(Arc::new(MockExecutor::with_delay(Duration::from_millis(
                        450,
                    )))),
                    claude_config_from_env(),
                )
                // All-in-one-codebase default: projects born through the
                // creation flow get their own real git repo next to the DB.
                .with_workspaces_root(workspaces_root())
                // plan/17 S3: wire the back-channel that turns `RunIssue`
                // from a blocking inline call into a backgrounded one. The
                // matching `settle_rx` is polled in the `select!` loop
                // below; examples / headless drivers never wire this →
                // `RunIssue` stays inline there.
                .with_settle_channel(settle_tx)
                // V1 Issue2 Phase2b: enable PTY mode — interactive issue runs
                // spawn claude in a PTY (portable-pty) instead of a system
                // terminal, streaming bytes to the UI's xterm.js widget.
                .with_pty();

                // Live event → transient note forwarding. Runs concurrently with
                // dispatch (progress events are emitted mid-run).
                let mut ev = app.subscribe();
                let fwd = note_tx.clone();
                tokio::spawn(async move {
                    while let Ok(e) = ev.recv().await {
                        let note = match e {
                            Event::RunStarted {
                                workflow_name,
                                agents,
                                skills,
                            } => UiNote::RunStarted {
                                workflow_name,
                                agents,
                                skills,
                            },
                            Event::WorkflowProgress { phase_idx, status } => {
                                if let Some(name) = status.strip_prefix("started:") {
                                    UiNote::PhaseStarted {
                                        idx: phase_idx,
                                        name: name.to_string(),
                                    }
                                } else {
                                    UiNote::PhaseCompleted { idx: phase_idx }
                                }
                            }
                            Event::WorkflowDone => UiNote::RunDone,
                            Event::WorkflowFailed(err) => UiNote::RunFailed(err),
                            Event::StageHandoff { from, to, risky } => {
                                UiNote::Handoff { from, to, risky }
                            }
                            Event::CronAutoFired { name, ok, .. } => {
                                UiNote::CronAutoFired { name, ok }
                            }
                            Event::ArtifactsRegistered { fresh } if fresh > 0 => {
                                UiNote::ArtifactsRegistered { fresh }
                            }
                            Event::ConnectorSynced { name, ok, detail } => {
                                UiNote::ConnectorSynced { name, ok, detail }
                            }
                            Event::ActionProgress { name, state } => {
                                UiNote::ActionProgress { name, state }
                            }
                            _ => continue,
                        };
                        let _ = fwd.send(note);
                    }
                });

                if let Err(e) = app.dispatch(Command::Boot).await {
                    let _ = note_tx.send(UiNote::Error(e.to_string()));
                }

                // T14 (2026-07-24, plan/12 §10 v1.1): real-daily-DB one-shot
                // legacy-shell migration — dispatched right after `Boot` with
                // the real DB path this thread already opened the store
                // from (see `Command::MigrateLegacyShellsIfNeeded`'s doc
                // comment for why it's threaded in rather than read off the
                // store). A fresh DB, or one already migrated, is a true
                // no-op inside the handler; failure surfaces as a
                // `UiNote::Error` the same way any other dispatch failure
                // does — never a boot-blocking panic. `[BW_MIGRATE]` stderr
                // line is this ticket's own render/readback proof, same
                // discipline as `[BW_OPEN]`/`[BW_HUB]` below and in
                // `main.rs`.
                if let Err(e) = app
                    .dispatch(Command::MigrateLegacyShellsIfNeeded {
                        db_path: real_db_path.clone(),
                    })
                    .await
                {
                    let _ = note_tx.send(UiNote::Error(e.to_string()));
                }
                eprintln!(
                    "[BW_MIGRATE] skills={} agents={}",
                    app.snapshot().skills.len(),
                    app.snapshot().agents.len()
                );

                let _ = vm_tx.send(build_vm(&app, &store).await);

                // Hands-free deep-link (verify/demo): open a named project and
                // optionally a panel from env — skips the wall→open→tab clicks.
                // BW_OPEN=<project name>;
                // BW_PANEL=progress|workflow|routine|artifact|version|issues.
                if let Ok(name) = std::env::var("BW_OPEN") {
                    if let Some(p) = app.snapshot().projects.iter().find(|p| p.name == name) {
                        let pid = p.id;
                        let _ = app.dispatch(Command::OpenProject(pid)).await;
                        if let Ok(pl) = std::env::var("BW_PANEL") {
                            let panel = match pl.as_str() {
                                "workflow" => Panel::Workflow,
                                "routine" => Panel::Routine,
                                "artifact" => Panel::Artifact,
                                "version" => Panel::Version,
                                "issues" => Panel::Issues,
                                _ => Panel::Progress,
                            };
                            let _ = app.dispatch(Command::SetPanel(panel)).await;
                        }
                        let s = app.snapshot();
                        eprintln!(
                            "[BW_OPEN] {name:?} -> view={:?} panel={:?} projects={} issues={}",
                            s.view,
                            s.panel,
                            s.projects.len(),
                            s.issues.len()
                        );
                        let _ = vm_tx.send(build_vm(&app, &store).await);
                    } else {
                        eprintln!("[BW_OPEN] project {name:?} NOT FOUND");
                    }
                }

                // The real scheduler clock: `App` is owned single-threaded by
                // this loop (no `Arc<Mutex<_>>`), so an auto-fire tick has to
                // interleave with command dispatch via `select!` rather than
                // run on its own spawned task — same thread, same `&mut app`,
                // no synchronization needed. A quiet tick (nothing due) is
                // free: `Vm` is only rebuilt when `tick_scheduler` actually
                // fired something, so idle polling costs nothing extra.
                let mut ticker = tokio::time::interval(Duration::from_secs(5));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                // V1 Issue2 Phase2b: fast timer for PTY terminal bytes —
                // drains `pty_bytes_rx` and rebuilds the Vm with the bytes.
                // 100ms = 10fps, sufficient for terminal output (the human
                // eye reads text at ~10 chars/sec; 8KB chunks every 100ms
                // is plenty). Not a `tick_scheduler` call — this is a
                // dedicated drain that bypasses cron/scheduler logic.
                let mut pty_ticker = tokio::time::interval(Duration::from_millis(100));
                pty_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

                loop {
                    tokio::select! {
                        cmd = cmd_rx.recv() => {
                            let Some(cmd) = cmd else { break };
                            if let Err(e) = app.dispatch(cmd).await {
                                let _ = note_tx.send(UiNote::Error(e.to_string()));
                            }
                            let _ = vm_tx.send(build_vm(&app, &store).await);
                        }
                        // plan/17 S3: a backgrounded issue-run's round loop
                        // finished (or errored) — drive `finalize_run` + tail
                        // on the main thread, same as a dispatched command.
                        // The spawned task ran on THIS `current_thread`
                        // runtime (cooperatively interleaved at the other
                        // arms' await points), so no `Arc<Mutex>` was needed.
                        Some(req) = settle_rx.recv() => {
                            if let Err(e) = app.run_issue_settle(req).await {
                                let _ = note_tx.send(UiNote::Error(e.to_string()));
                            }
                            let _ = vm_tx.send(build_vm(&app, &store).await);
                        }
                        _ = ticker.tick() => {
                            match app.tick_scheduler().await {
                                Ok(fired) if !fired.is_empty() => {
                                    let _ = vm_tx.send(build_vm(&app, &store).await);
                                }
                                Ok(_) => {}
                                Err(e) => {
                                    let _ = note_tx.send(UiNote::Error(e.to_string()));
                                }
                            }
                        }
                        // V1 终端会话重构·底座: drain 带 id 的批次 → watch。
                        // 重启恢复:首包到达清 pty_restoring 时重建 Vm,让「恢复中…」消失。
                        _ = pty_ticker.tick() => {
                            let was_restoring = app.pty_restoring().is_some();
                            let batches = app.drain_pty_events();
                            let cleared_restoring =
                                was_restoring && app.pty_restoring().is_none();
                            if !batches.is_empty() {
                                let _ = pty_tx.send(batches);
                            }
                            if cleared_restoring {
                                let _ = vm_tx.send(build_vm(&app, &store).await);
                            }
                        }
                    }
                }
            });
        })
        .expect("spawn kernel thread");

    Kernel {
        tx: cmd_tx,
        vm_rx,
        notes,
        pty_rx,
    }
}

// ───────────────────────────── vm assembly ─────────────────────────────

/// Store rows → `ui` pure builders → one renderable snapshot.
async fn build_vm(app: &App, store: &Arc<dyn Store>) -> Vm {
    let state = app.snapshot();
    let now = OffsetDateTime::now_utc();

    // Project wall cards. Progress is real: 0 while cold-starting (nothing
    // materializes until confirm), mean of hand-set stage progress once running.
    let mut cards = Vec::with_capacity(state.projects.len() + 1);
    for p in &state.projects {
        let stage_progresses: Vec<u8> = if p.phase == Readiness::Running {
            match store.list_stages(p.id).await {
                Ok(stages) => stages.iter().map(|s| s.progress).collect(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
        // A5-H: the wall's "open work" badge — same non-terminal predicate as
        // the A4 handoff risky-guard. Recomputed on every `build_vm` call
        // (i.e. after every dispatched command), so it's never stale.
        let open_issues = store.count_open_issues(p.id).await.unwrap_or(0) as usize;
        cards.push(project_card(
            p.id,
            &p.name,
            &p.kind,
            &p.desc,
            p.phase,
            p.cycle,
            p.active_stage,
            p.signal,
            &stage_progresses,
            open_issues,
            &p.workspace_path,
        ));
    }

    // Hub library — global, built unconditionally (no active project
    // involved), so it's ready even before the `active_project` early-return
    // below and reachable from the standalone Hub screens (rail-routed, not
    // tied to `active_project` at all).
    // W1: fold each spec's real run record (aggregated off `workflow_run`)
    // into its hub row — a cold workflow keeps the honest "暂无运行".
    let usage_ranking = store.hub_usage_ranking().await.unwrap_or_default();
    let workflows: Vec<WorkflowHubRowVm> = state
        .workflow_specs
        .iter()
        .filter_map(|spec| {
            let mut row = workflow_hub_row(spec)?;
            if let Some(rank) = usage_ranking.iter().find(|r| r.workflow_id == spec.id) {
                ui::vm::attach_run_record(&mut row, rank);
            }
            Some(row)
        })
        .collect();
    let workflow_details: Vec<ui::vm::WorkflowDetailVm> = state
        .workflow_specs
        .iter()
        .filter_map(ui::vm::workflow_detail)
        .collect();
    // T4(plan/12 §2): fold each skill's real `skill_file` rows in — one
    // indexed-by-`skill_id` read per skill (`idx_skill_file_skill`), same
    // eager-per-row-in-`build_vm` convention `usage_ranking`/`connectors`/
    // etc. already use above; a skill with none gets an honest empty `files`
    // (`skill_card`'s own graceful-degradation signal), not a wasted query
    // guard.
    let mut skills: Vec<SkillCardVm> = Vec::with_capacity(state.skills.len());
    for s in &state.skills {
        let files = store
            .list_skill_files(s.id)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|f| ui::vm::SkillFileVm {
                rel_path: f.rel_path,
                content: f.content,
            })
            .collect();
        skills.push(skill_card(s, files));
    }
    let agents: Vec<AgentCardVm> = state.agents.iter().map(agent_card).collect();
    let project_names: Vec<(bw_core::ProjectId, String)> = state
        .projects
        .iter()
        .map(|p| (p.id, p.name.clone()))
        .collect();
    // PF1-3a: CollectMetrics 任务的副标题从该项目 collect_kind='script' 的
    // metric name 派生(对仗 mode_label/mode_icon 同源)。卡只读 VM 无 store
    // 访问,所以这里(kernel build_vm)填。best-effort:读失败空 Vec。
    let mut cron_tasks: Vec<CronRowVm> = Vec::with_capacity(state.cron_tasks.len());
    for c in state.cron_tasks.iter() {
        let mut row = cron_row(c, &project_names, &state.skills, now);
        if matches!(c.mode, CronMode::CollectMetrics) {
            if let Some(pid) = c.project_id {
                if let Ok(sigs) = store.persisted_signals(pid).await {
                    row.collect_targets = sigs
                        .metrics
                        .iter()
                        .filter(|m| m.collect_kind == "script")
                        .map(|m| m.name.clone())
                        .collect();
                }
            }
        }
        cron_tasks.push(row);
    }
    let connectors: Vec<ConnectorCardVm> = state.connectors.iter().map(connector_card).collect();
    let knowledge_sources: Vec<KnowledgeRowVm> =
        state.knowledge_sources.iter().map(knowledge_row).collect();
    let activity: Vec<ActivityRowVm> = state
        .recent_activity
        .iter()
        .map(|g| {
            activity_row(
                &ActivitySource {
                    project_id: g.project_id,
                    project_name: g.project_name.clone(),
                    from_stage: g.from_stage,
                    to_stage: g.to_stage,
                    risky: g.risky,
                    note: g.note.clone(),
                    at: g.at,
                },
                now,
            )
        })
        .collect();
    let notifications: Vec<NotifyItemVm> =
        notify_feed(&state.cron_tasks, &state.connectors, &activity);
    let hub = HubVm {
        overview: hub_overview(
            workflows.len(),
            &workflows.iter().map(|w| w.name.clone()).collect::<Vec<_>>(),
            skills.len(),
            &skills.iter().map(|s| s.name.clone()).collect::<Vec<_>>(),
            agents.len(),
            &agents.iter().map(|a| a.name.clone()).collect::<Vec<_>>(),
        ),
        workflows,
        workflow_details,
        skills,
        agents,
        cron_tasks,
        connectors,
        knowledge_sources,
        activity,
        notifications,
    };
    let settings = settings_vm(
        state.claude_config.binary.as_deref(),
        state.claude_config.max_budget_usd,
        state.claude_config.default_mode == PermissionMode::BypassPermissions,
        state.claude_config.commands_mode == PermissionMode::BypassPermissions,
    );

    // L1(plan/11): pre-format the last-loaded cron task's fire history, if
    // any — same explicit single-slot pattern as `version_log`/`artifacts`.
    let cron_effectiveness = state
        .cron_effectiveness
        .as_ref()
        .map(|(id, e)| (*id, ui::vm::cron_effectiveness_vm(e)));

    let mut vm = Vm {
        ready: true,
        fatal: None,
        view: state.view,
        projects: cards,
        create: None,
        op: None,
        hub: hub.clone(),
        settings,
        cron_effectiveness,
        github_repos: state.github_repos.clone(),
        codehub_repos: state.codehub_repos.clone(),
        remote_project_probe: state.remote_project_probe.clone(),
        active_run: app.active_run(),
        pty_active: app.pty_active(),
        pty_conversation_id: app.focused_pty_conversation(),
        pty_live_ids: app.live_pty_ids(),
        pty_restoring: app.pty_restoring(),
        focused_issue: app.focused_pty_issue(),
        consultable_issues: Vec::new(),
        resumable_issues: Vec::new(),
    };

    let Some(pid) = state.active_project else {
        return vm;
    };
    let Some(row) = state.projects.iter().find(|p| p.id == pid).cloned() else {
        return vm;
    };

    // Shared detail reads for the active project.
    let sigs = store
        .persisted_signals(pid)
        .await
        .unwrap_or_else(|_| bw_store::PersistedSignals {
            project: None,
            weekly: None,
            stages: Vec::new(),
            metrics: Vec::new(),
        });
    let observations = store.list_observations(pid).await.unwrap_or_default();

    // Observation series per metric (ASC) — the honest trend + feed source.
    let mut series: HashMap<MetricId, Vec<String>> = HashMap::new();
    let mut latest_ts: HashMap<MetricId, OffsetDateTime> = HashMap::new();
    // V1-Issue3 · per-metric (ts, raw) pairs for ISO-week sparkline bucketing.
    let mut series_ts: HashMap<MetricId, Vec<(i64, String)>> = HashMap::new();
    for o in &observations {
        series.entry(o.metric_id).or_default().push(o.raw.clone());
        latest_ts.insert(o.metric_id, o.ts);
        series_ts
            .entry(o.metric_id)
            .or_default()
            .push((o.ts.unix_timestamp(), o.raw.clone()));
    }

    // V1-Issue3 · the project's CollectMetrics cron last tick + script
    // connector name — shared across all this project's metrics'
    // collection_chain. Read from AppState's cached rows (already loaded by
    // `refresh_cron_tasks`/`refresh_connectors`), no new store call.
    let now_unix = now.unix_timestamp();
    let cron_last_tick: Option<i64> = state
        .cron_tasks
        .iter()
        .filter(|c| c.project_id == Some(pid) && matches!(c.mode, CronMode::CollectMetrics))
        .filter_map(|c| c.last_run_at)
        .map(|t| t.unix_timestamp())
        .max();
    let script_connector: Option<String> = state
        .connectors
        .iter()
        .find(|c| c.project_id == Some(pid) && c.kind == CONNECTOR_KIND_SCRIPT)
        .map(|c| c.name.clone());

    let metrics: Vec<MetricVm> = sigs
        .metrics
        .iter()
        .map(|m| {
            let mid = m.id;
            let obs_ts = series_ts.get(&mid).map(Vec::as_slice).unwrap_or(&[]);
            let wk_spark = weekly_spark(obs_ts, now_unix);
            // V1-quickfix(W3-8): if the latest week has no real observation
            // (only carry-forward), delta is meaningless — show "—" not "0.0".
            let wk_delta = if last_week_has_real_obs(obs_ts, now_unix) {
                weekly_delta(&wk_spark)
            } else {
                None
            };
            let is_intrinsic = is_intrinsic_metric(&m.name);
            let chain = CollectionChainVm {
                collect_label: collect_label(&m.collect_kind, is_intrinsic).to_string(),
                connector_name: if m.collect_kind == "script" {
                    script_connector.clone()
                } else {
                    None
                },
                cron_last_tick,
                has_observation: latest_ts.contains_key(&mid),
            };
            metric_vm(
                m.id,
                &m.name,
                &m.def,
                m.role == MetricRole::Leading,
                m.stage_kind,
                &m.value_raw,
                &m.target_raw,
                &m.last_target,
                &m.driver,
                m.signal,
                m.hit,
                m.source,
                &m.collect_kind,
                m.archived,
                m.origin == bw_store::MetricOrigin::File,
                series.get(&m.id).map(Vec::as_slice).unwrap_or(&[]),
                &wk_spark,
                wk_delta,
                chain,
            )
        })
        .collect();
    let week_plan = week_plan_rows(&metrics);

    if state.view == View::Create {
        vm.create = Some(CreateVm {
            name: row.name.clone(),
            kind: row.kind.clone(),
            brief: row.desc.clone(),
            cycle: row.cycle,
            benchmark: row.benchmark.clone(),
            win: row.opportunity.clone(),
            north_star: row.north_star.clone(),
            ns_def: row.ns_def.clone(),
            // 创建流的指标表单里不出现已停用的指标:这是一张「这个项目
            // 要用哪些指标」的编辑表,一条已经判定不再用的行留在里面,
            // 提交时会被 UpsertManualMetric 原样写回去、等于悄悄复活它。
            // (运营视图有「已停用」折叠区可看可恢复,那是它的去处。)
            leading: metrics
                .iter()
                .filter(|m| m.leading && !m.archived)
                .cloned()
                .collect(),
            lagging: metrics
                .iter()
                .filter(|m| !m.leading && !m.archived)
                .cloned()
                .collect(),
            remote_path: row.remote_path.clone(),
            remote_host: row.remote_host.clone(),
        });
        return vm;
    }

    if state.view != View::App {
        return vm;
    }

    // ── operating view ──
    let stages = store.list_stages(pid).await.unwrap_or_default();
    let sessions = store.list_sessions(pid).await.unwrap_or_default();
    let handoffs = store.list_handoffs(pid).await.unwrap_or_default();
    let mut handoff_count: HashMap<StageKind, u32> = HashMap::new();
    for h in &handoffs {
        *handoff_count.entry(h.from_stage).or_default() += 1;
    }

    let stage_sigs: Vec<(StageKind, Option<Signal>)> =
        sigs.stages.iter().map(|s| (s.kind, s.routine)).collect();
    let session_flags: Vec<(Option<StageKind>, bool)> = sessions
        .iter()
        .map(|s| (s.stage_kind, s.status == SessionStatus::Active))
        .collect();

    // Metric name/signal lookup for the feed.
    let metric_info: HashMap<MetricId, (String, Signal)> = metrics
        .iter()
        .map(|m| (m.id, (m.name.clone(), m.signal)))
        .collect();
    let feed_input = |filter: Option<StageKind>| -> Vec<FeedItemVm> {
        let rows: Vec<FeedSource> = observations
            .iter()
            .filter(|o| {
                filter.is_none()
                    || metrics
                        .iter()
                        .find(|m| m.id == o.metric_id)
                        .map(|m| m.stage_kind == filter)
                        .unwrap_or(false)
            })
            .filter_map(|o| {
                let (metric_name, current_signal) = metric_info.get(&o.metric_id)?.clone();
                Some(FeedSource {
                    metric_name,
                    raw: o.raw.clone(),
                    source: o.source,
                    ts: o.ts,
                    current_signal,
                    is_latest: latest_ts.get(&o.metric_id) == Some(&o.ts),
                })
            })
            .collect();
        observation_feed(&rows, now)
    };

    let stage_vms: Vec<StageVm> = stages
        .iter()
        .map(|s| StageVm {
            kind: s.kind,
            n: s.kind.index(),
            progress: s.progress,
            trend: s.trend.clone(),
            schedule_label: cadence_label(&s.schedule),
            health: ui::vm::resolved(
                sigs.stages
                    .iter()
                    .find(|x| x.kind == s.kind)
                    .and_then(|x| x.routine),
            ),
            metrics: metrics
                .iter()
                .filter(|m| m.stage_kind == Some(s.kind))
                .cloned()
                .collect(),
            feed: feed_input(Some(s.kind)),
            detail: stage_detail(
                s.kind,
                &s.dod,
                handoff_count.get(&s.kind).copied().unwrap_or(0),
            ),
        })
        .collect();

    let session_cards: Vec<SessionCardVm> = sessions
        .iter()
        .map(|s| SessionCardVm {
            id: s.id,
            title: s.title.clone(),
            create: s.kind == bw_store::SessionKind::Create,
            stage_kind: s.stage_kind,
            status_label: session_status_label(s.status),
            active: s.status == SessionStatus::Active,
        })
        .collect();

    let chat = match state.active_session {
        Some(sid) => {
            let title = session_cards
                .iter()
                .find(|s| s.id == sid)
                .map(|s| (s.title.clone(), s.status_label))
                .unwrap_or_else(|| ("会话".to_string(), "进行中"));
            let msgs = store
                .session_messages(sid)
                .await
                .unwrap_or_default()
                .into_iter()
                .map(|m| MsgVm {
                    agent: m.role == Author::Agent,
                    text: m.text,
                })
                .collect();
            Some(ChatVm {
                id: sid,
                title: title.0,
                status_label: title.1,
                msgs,
            })
        }
        None => None,
    };

    let version_log = version_log_vm(state.version_log.as_ref().and_then(|(vpid, result)| {
        (*vpid == pid).then(|| {
            result
                .as_ref()
                .map(|commits| {
                    commits
                        .iter()
                        .map(|c| ui::vm::CommitSource {
                            short_hash: c.short_hash.clone(),
                            author: c.author.clone(),
                            date: c.date.clone(),
                            subject: c.subject.clone(),
                        })
                        .collect()
                })
                .map_err(|e| e.clone())
        })
    }));

    // Artifact registry snapshot — same explicit-load, project-tagged rule
    // as `version_log`: `None` until `LoadArtifacts` ran for THIS project.
    let artifacts = state
        .artifacts
        .as_ref()
        .and_then(|(apid, rows)| (*apid == pid).then(|| ui::vm::artifact_rows(rows, now)));

    let overall = overall_progress(&stages.iter().map(|s| s.progress).collect::<Vec<_>>());

    // P5: weekly-review card — a pure read of already-recorded facts. Counts
    // come off `state.issues` + the per-metric latest-observation-ts map built
    // above; the date math (ISO week, 90-day line) lives in the VM.
    let week_start = ui::vm::iso_week_start_unix(now_unix);
    let week_review = ui::vm::week_review_vm(
        now_unix,
        row.created_at,
        state
            .issues
            .iter()
            .filter(|i| i.settled_at.map_or(false, |t| t >= week_start))
            .count() as u32,
        state
            .issues
            .iter()
            .filter(|i| !i.status.is_terminal())
            .count() as u32,
        sigs.metrics
            .iter()
            // 停用的指标不再计入「久没数据」——它本来就不该再有新数据了,
            // 把它数进去等于用一条已退役的指标制造一个假的待办。
            .filter(|m| {
                !m.archived
                    && latest_ts
                        .get(&m.id)
                        .map_or(true, |t| t.unix_timestamp() < week_start)
            })
            .count() as u32,
    );
    let resumable = store
        .list_resumable_conversation_issue_ids(pid)
        .await
        .unwrap_or_default();

    vm.op = Some(OpVm {
        id: pid,
        name: row.name.clone(),
        kind: row.kind.clone(),
        desc: row.desc.clone(),
        project_signal: ui::vm::resolved(sigs.project),
        cycle: row.cycle,
        active_stage: row.active_stage,
        north_star: row.north_star.clone(),
        ns_def: row.ns_def.clone(),
        benchmark: row.benchmark.clone(),
        opportunity: row.opportunity.clone(),
        workspace_path: row.workspace_path.clone(),
        allow_commands: row.allow_commands,
        remote_path: row.remote_path.clone(),
        remote_host: row.remote_host.clone(),
        provider: row.provider.clone(),
        panel: state.panel,
        scope: state.scope,
        nav: stage_nav(&stage_sigs, &session_flags),
        attention: attention_from_rows(&stage_sigs, &session_flags),
        archived: sessions
            .iter()
            .filter(|s| s.status != SessionStatus::Active)
            .count(),
        stages: stage_vms,
        metrics,
        week_plan,
        overall,
        sessions: session_cards,
        issues: state
            .issues
            .iter()
            .map(|i| issue_card(i, &state.agents))
            .collect(),
        chat,
        hub,
        version_log,
        artifacts,
        // P4: the explicitly-opened Issue detail — assembled by
        // `Command::OpenIssueDetail`, mapped 1:1 here, `None` = no overlay.
        issue_detail: state.issue_detail.as_ref().map(|d| {
            ui::vm::issue_detail_vm(
                &d.issue,
                &d.runs,
                &d.changes,
                &d.artifacts,
                &state.agents,
                d.is_interactive,
            )
        }),
        active_run: app.active_run(),
        week_review,
        pty_active: app.pty_active(),
        pty_conversation_id: app.focused_pty_conversation(),
        pty_live_ids: app.live_pty_ids(),
        pty_restoring: app.pty_restoring(),
        focused_issue: app.focused_pty_issue(),
        consultable_issues: {
            state
                .issues
                .iter()
                .filter(|i| {
                    matches!(i.status, IssueStatus::Done | IssueStatus::InReview)
                        && resumable.contains(&i.id)
                })
                .map(|i| i.id)
                .collect()
        },
        resumable_issues: resumable,
    });
    vm.pty_active = app.pty_active();
    vm.pty_conversation_id = app.focused_pty_conversation();
    vm.pty_live_ids = app.live_pty_ids();
    vm.pty_restoring = app.pty_restoring();
    vm.focused_issue = app.focused_pty_issue();
    vm
}
