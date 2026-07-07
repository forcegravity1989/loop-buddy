//! `bw-app` — the UI-agnostic orchestration brain (plan `§3`).
//!
//! Command in, event out, single subscribable state. The UI never touches the
//! store or engine directly: it [`dispatch`](App::dispatch)es a [`Command`],
//! reads [`snapshot`](App::snapshot), and reacts to the [`Event`] stream from
//! [`subscribe`](App::subscribe). `App` is generic over the [`Executor`], so the
//! colleague team's real backend hot-swaps for [`MockExecutor`] with zero changes
//! here (Tier C).

#![forbid(unsafe_code)]

use bw_core::derive::AmberBand;
use bw_core::model::{
    Cadence, ProjectPhase, Role, Signal, SourceKind, StageKind, StagePhase, WorkflowSpec,
};
use bw_core::{MetricId, ProjectId, SessionId};
use bw_engine::{Engine, Executor, MockExecutor, RunCtx, RunEvent};
use bw_store::{
    MetricRole, NewMetric, NewProject, NewSession, NewStage, ProjectRow, SessionKind, Store,
};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::broadcast;

/// Top-level workspace view (only meaningful for `hub == workspace`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum View {
    #[default]
    Projects,
    Wizard,
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

/// Stage-axis selection: all stages or one control point (1..=7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    All,
    Stage(u8),
}

/// UI → kernel intents.
pub enum Command {
    /// App start: load the project wall and re-derive every running project's
    /// signals against the *current* clock (staleness must show on the wall).
    Boot,
    CreateProject {
        id: ProjectId,
        name: String,
        kind: String,
        desc: String,
    },
    SetWizardStep {
        step: u8,
    },
    UpdateNorthStar {
        value: String,
        def: String,
    },
    /// 对标竞品 + 机会缺口 (wizard steps 1/2 — the real inputs behind the
    /// prototype's demo matrix).
    UpdateBrief {
        benchmark: String,
        opportunity: String,
    },
    /// Record a metric + its current value as an append-only Manual observation
    /// (wizard steps 4/5). Signal is derived, never set here.
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
    /// Week-plan edit (step 7 / progress panel): new target + this week's
    /// driver. No value is touched; recompute re-derives against the new target.
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
    /// 进度管理 lever: hand-set plan progress for one stage (plan data, not a
    /// signal — the derive chain is untouched).
    SetStageProgress {
        stage_kind: StageKind,
        progress: u8,
    },
    CompleteWizard,
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
    SendSessionMessage {
        session: SessionId,
        text: String,
    },
    AnnotateWeeklyReview {
        human_override: Option<Signal>,
        reason: String,
    },
    OpenProject(ProjectId),
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
    WizardStepChanged(u8),
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
    pub wizard_step: u8,
    pub active_project: Option<ProjectId>,
    pub active_session: Option<SessionId>,
    pub projects: Vec<ProjectRow>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            view: View::Projects,
            panel: Panel::Progress,
            scope: Scope::All,
            wizard_step: 0,
            active_project: None,
            active_session: None,
            projects: Vec::new(),
        }
    }
}

/// The orchestration brain.
pub struct App<E: Executor = MockExecutor> {
    store: Arc<dyn Store>,
    engine: Engine<E>,
    state: AppState,
    events: broadcast::Sender<Event>,
}

impl<E: Executor> App<E> {
    pub fn new(store: Arc<dyn Store>, engine: Engine<E>) -> Self {
        let (tx, _rx) = broadcast::channel(256);
        Self {
            store,
            engine,
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
                self.state.view = View::Wizard;
                self.state.wizard_step = 0;
                self.refresh_projects().await?;
                self.emit(Event::ProjectsChanged);
                self.emit(Event::ViewChanged(View::Wizard));
            }

            Command::SetWizardStep { step } => {
                let p = self.active()?;
                self.state.wizard_step = step;
                self.store
                    .set_project_phase(p, ProjectPhase::ColdStart, Some(step))
                    .await?;
                self.emit(Event::WizardStepChanged(step));
            }

            Command::UpdateNorthStar { value, def } => {
                let p = self.active()?;
                self.store.set_north_star(p, &value, &def).await?;
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
                // Idempotency guard: re-confirming a wizard step must not mint a
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

            Command::CompleteWizard => {
                let p = self.active()?;
                self.store
                    .set_project_phase(p, ProjectPhase::Running, None)
                    .await?;
                self.store.materialize_stages(seven_stages(p)).await?;
                self.store.recompute_signals(p, now()).await?;
                self.state.view = View::App;
                self.state.wizard_step = 7;
                self.refresh_projects().await?;
                self.emit(Event::ProjectUpdated(p));
                self.emit(Event::ViewChanged(View::App));
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
                let ctx = RunCtx {
                    project: p,
                    workflow: spec.id,
                };
                // Progress events are emitted LIVE from inside the engine
                // callback (broadcast::send is sync), so a subscriber watches
                // phases advance while the run is still going. Only persistence
                // (async) is deferred to after the run.
                let live = self.events.clone();
                let mut completed: Vec<bw_engine::PhaseOutput> = Vec::new();
                let run = self
                    .engine
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
                    ProjectPhase::ColdStart => View::Wizard,
                    ProjectPhase::Running => {
                        // Freshness is clock-relative — re-derive on open so a
                        // value that went stale since last time shows as such.
                        self.store.recompute_signals(id, now()).await?;
                        self.refresh_projects().await?;
                        View::App
                    }
                };
                self.state.wizard_step = proj.cold_step.unwrap_or(0);
                self.emit(Event::ViewChanged(self.state.view));
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

/// Materialize the seven control points for a freshly completed project.
fn seven_stages(project: ProjectId) -> Vec<NewStage> {
    StageKind::ALL
        .into_iter()
        .map(|kind| {
            let phase = match kind {
                StageKind::CompetitorInsight
                | StageKind::RequirementIntake
                | StageKind::NorthStar => StagePhase::Finalized,
                StageKind::Leading | StageKind::Lagging => StagePhase::Monitoring,
                StageKind::PrototypeCreate => StagePhase::Iterating,
                StageKind::ProgressMgmt => StagePhase::Running,
            };
            NewStage {
                project_id: project,
                kind,
                phase,
                progress: 0,
                schedule: Cadence::Weekly,
                owns: String::new(),
                accept: String::new(),
                control: String::new(),
            }
        })
        .collect()
}
