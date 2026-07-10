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

use bw_app::{App, Command, Event, Panel, Scope, View};
use bw_core::model::{
    AgentRef, HubCard, ProjectCycle, ProjectPhase, Role, SessionStatus, Signal, SkillRef, StageKind,
};
use bw_core::{MetricId, SessionId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{MetricRole, SqliteStore, Store};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::{broadcast, mpsc, watch};
use ui::vm::{
    activity_row, agent_card, attention_from_rows, cadence_label, connector_card, cron_row,
    hub_overview, knowledge_row, metric_vm, notify_feed, observation_feed, project_card,
    session_status_label, settings_vm, skill_card, stage_detail, stage_nav, version_log_vm,
    week_plan_rows, workflow_hub_row, ActivityRowVm, ActivitySource, AgentCardVm, ConnectorCardVm,
    CronRowVm, FeedItemVm, FeedSource, KnowledgeRowVm, MetricVm, NotifyItemVm, ProjectCardVm,
    SessionCardVm, SettingsVm, SkillCardVm, StageDetailVm, StageNavItemVm, WeekPlanRowVm,
    WorkflowHubRowVm,
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
    pub cycle: ProjectCycle,
    pub benchmark: String,
    /// 三个月后怎样算成 (stored in the `opportunity` column).
    pub win: String,
    pub north_star: String,
    pub ns_def: String,
    pub leading: Vec<MetricVm>,
    pub lagging: Vec<MetricVm>,
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
    pub name: String,
    pub kind: String,
    pub project_signal: Signal,
    pub cycle: ProjectCycle,
    pub active_stage: StageKind,
    pub north_star: String,
    pub ns_def: String,
    /// Real-executor target directory. Empty = unconfigured — this project
    /// only ever runs `RunWorkflow` on `MockExecutor`.
    pub workspace_path: String,
    pub allow_commands: bool,
    pub panel: Panel,
    pub scope: Scope,
    pub nav: Vec<StageNavItemVm>,
    pub attention: Attention,
    pub archived: usize,
    pub stages: Vec<StageVm>,
    pub metrics: Vec<MetricVm>,
    pub week_plan: Vec<WeekPlanRowVm>,
    pub stats: ui::vm::StatCardsVm,
    pub overall: u8,
    pub sessions: Vec<SessionCardVm>,
    pub chat: Option<ChatVm>,
    /// Threaded down for the "从 Hub 导入" overview strip in the Workflow
    /// panel — same data as the top-level `Vm.hub`, just also reachable from
    /// deep inside `Op`'s component tree without re-prop-drilling `Vm` itself.
    pub hub: HubVm,
    /// Real `git log` for this project's `workspace_path` (Version panel).
    /// `NotLoaded` until `Command::LoadVersionLog` is dispatched at least
    /// once for this specific project.
    pub version_log: ui::vm::VersionLogVm,
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
            UiNote::Handoff { .. } | UiNote::Error(_) => {}
        }
    }
}

// ───────────────────────────── the bridge ─────────────────────────────

#[derive(Clone)]
pub struct Kernel {
    tx: mpsc::UnboundedSender<Command>,
    vm_rx: watch::Receiver<Vm>,
    notes: broadcast::Sender<UiNote>,
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
    let (vm_tx, vm_rx) = watch::channel(Vm::default());
    let (note_tx, _keep) = broadcast::channel::<UiNote>(256);
    let notes = note_tx.clone();

    std::thread::Builder::new()
        .name("bw-kernel".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("kernel runtime");
            rt.block_on(async move {
                let store: Arc<dyn Store> = match SqliteStore::open(&db_path()).await {
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
                );

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
                            _ => continue,
                        };
                        let _ = fwd.send(note);
                    }
                });

                if let Err(e) = app.dispatch(Command::Boot).await {
                    let _ = note_tx.send(UiNote::Error(e.to_string()));
                }
                let _ = vm_tx.send(build_vm(&app, &store).await);

                while let Some(cmd) = cmd_rx.recv().await {
                    if let Err(e) = app.dispatch(cmd).await {
                        let _ = note_tx.send(UiNote::Error(e.to_string()));
                    }
                    let _ = vm_tx.send(build_vm(&app, &store).await);
                }
            });
        })
        .expect("spawn kernel thread");

    Kernel {
        tx: cmd_tx,
        vm_rx,
        notes,
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
        let stage_progresses: Vec<u8> = if p.phase == ProjectPhase::Running {
            match store.list_stages(p.id).await {
                Ok(stages) => stages.iter().map(|s| s.progress).collect(),
                Err(_) => Vec::new(),
            }
        } else {
            Vec::new()
        };
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
        ));
    }

    // Hub library — global, built unconditionally (no active project
    // involved), so it's ready even before the `active_project` early-return
    // below and reachable from the standalone Hub screens (rail-routed, not
    // tied to `active_project` at all).
    let workflows: Vec<WorkflowHubRowVm> = state
        .workflow_specs
        .iter()
        .filter_map(workflow_hub_row)
        .collect();
    let workflow_details: Vec<ui::vm::WorkflowDetailVm> = state
        .workflow_specs
        .iter()
        .filter_map(ui::vm::workflow_detail)
        .collect();
    let skills: Vec<SkillCardVm> = state.skills.iter().map(skill_card).collect();
    let agents: Vec<AgentCardVm> = state.agents.iter().map(agent_card).collect();
    let project_names: Vec<(bw_core::ProjectId, String)> = state
        .projects
        .iter()
        .map(|p| (p.id, p.name.clone()))
        .collect();
    let cron_tasks: Vec<CronRowVm> = state
        .cron_tasks
        .iter()
        .map(|c| cron_row(c, &project_names))
        .collect();
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

    let mut vm = Vm {
        ready: true,
        fatal: None,
        view: state.view,
        projects: cards,
        create: None,
        op: None,
        hub: hub.clone(),
        settings,
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
    for o in &observations {
        series.entry(o.metric_id).or_default().push(o.raw.clone());
        latest_ts.insert(o.metric_id, o.ts);
    }

    let metrics: Vec<MetricVm> = sigs
        .metrics
        .iter()
        .map(|m| {
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
                series.get(&m.id).map(Vec::as_slice).unwrap_or(&[]),
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
            leading: metrics.iter().filter(|m| m.leading).cloned().collect(),
            lagging: metrics.iter().filter(|m| !m.leading).cloned().collect(),
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
                    agent: m.role == Role::Agent,
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

    let overall = overall_progress(&stages.iter().map(|s| s.progress).collect::<Vec<_>>());
    let stats = ui::vm::stat_cards(
        stages.len(),
        &sessions
            .iter()
            .map(|s| {
                (
                    s.kind == bw_store::SessionKind::Create,
                    s.status == SessionStatus::Active,
                )
            })
            .collect::<Vec<_>>(),
    );

    vm.op = Some(OpVm {
        name: row.name.clone(),
        kind: row.kind.clone(),
        project_signal: ui::vm::resolved(sigs.project),
        cycle: row.cycle,
        active_stage: row.active_stage,
        north_star: row.north_star.clone(),
        ns_def: row.ns_def.clone(),
        workspace_path: row.workspace_path.clone(),
        allow_commands: row.allow_commands,
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
        stats,
        overall,
        sessions: session_cards,
        chat,
        hub,
        version_log,
    });
    vm
}
