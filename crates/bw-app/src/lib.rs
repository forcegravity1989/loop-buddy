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
    stage_workflow, AgentCard, AgentRef, Cadence, HubSource, LibSource, LoopConfig, Maturity,
    ProjectCycle, ProjectPhase, Role, Signal, SkillCard, SkillRef, SourceKind, StageKind,
    WorkflowKind, WorkflowSpec,
};
use bw_core::{AgentId, MetricId, ProjectId, SessionId, SkillId, WorkflowId};
use bw_engine::{ClaudeCliConfig, ClaudeCliExecutor, Engine, RunCtx, RunEvent};
use bw_store::{
    MetricRole, NewAgent, NewMetric, NewProject, NewSession, NewSkill, NewStage, NewWorkflowSpec,
    ProjectRow, SessionKind, Store,
};
use std::path::PathBuf;
use std::sync::Arc;
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
    CreateSkill {
        id: SkillId,
        name: String,
        desc: String,
        category: String,
        source: LibSource,
    },
    CreateAgent {
        id: AgentId,
        name: String,
        role: String,
        skills: Vec<String>,
        model: String,
    },
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
        }
    }
}

/// The orchestration brain.
pub struct App {
    store: Arc<dyn Store>,
    mock_engine: Engine,
    claude_config: ClaudeCliConfig,
    state: AppState,
    events: broadcast::Sender<Event>,
}

impl App {
    pub fn new(store: Arc<dyn Store>, mock_engine: Engine, claude_config: ClaudeCliConfig) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self {
            store,
            mock_engine,
            claude_config,
            state: AppState::default(),
            events: tx,
        }
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

    /// Shared by `Command::RunWorkflow` and `Command::RunHubWorkflow` — the
    /// latter differs only in how `spec` was obtained (a hub lookup + a
    /// `uses` bump) and looks identical once it has one.
    async fn run_workflow_inner(
        &mut self,
        session: SessionId,
        spec: WorkflowSpec,
    ) -> Result<(), AppError> {
        let p = self.active()?;
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;
        let ctx = RunCtx {
            project: p,
            workflow: spec.id,
        };

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
                self.claude_config.clone(),
                PathBuf::from(proj.workspace_path.trim()),
                proj.allow_commands,
            );
            fresh_engine = Engine::new(Arc::new(executor));
            &fresh_engine
        };

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
        run.map_err(|e| AppError::Engine(e.to_string()))?;
        Ok(())
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
                self.refresh_workflow_specs().await?;
                self.refresh_skills().await?;
                self.refresh_agents().await?;
                self.emit(Event::ProjectsChanged);
            }

            Command::CreateProject {
                id,
                name,
                kind,
                desc,
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
                self.refresh_projects().await?;
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
                self.emit(Event::ProjectUpdated(p));
            }

            Command::UpdateNorthStar { value, def } => {
                let p = self.active()?;
                self.store.set_north_star(p, &value, &def).await?;
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
                self.store
                    .handoff_stage(p, from, to, risky, &note, now())
                    .await?;
                self.refresh_projects().await?;
                self.emit(Event::StageHandoff { from, to, risky });
                self.emit(Event::ProjectUpdated(p));
            }

            Command::CompleteCreation { cadence } => {
                let p = self.active()?;
                self.store
                    .set_project_phase(p, ProjectPhase::Running)
                    .await?;
                self.store
                    .materialize_stages(five_stages(p, cadence))
                    .await?;
                self.store.recompute_signals(p, now()).await?;
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
                self.run_workflow_inner(session, spec).await?;
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
                        agents,
                        skills,
                        loop_config,
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
                let spec = self
                    .store
                    .get_workflow_spec(workflow_id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                self.store.record_workflow_use(workflow_id).await?;
                self.refresh_workflow_specs().await?;
                self.run_workflow_inner(session, spec).await?;
            }

            Command::CreateSkill {
                id,
                name,
                desc,
                category,
                source,
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
                    })
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
                    })
                    .await?;
                self.refresh_agents().await?;
                self.emit(Event::AgentsChanged);
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
