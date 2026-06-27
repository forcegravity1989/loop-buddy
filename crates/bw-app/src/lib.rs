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
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum View {
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
    CreateProject {
        id: ProjectId,
        name: String,
        kind: String,
    },
    SetWizardStep {
        step: u8,
    },
    UpdateNorthStar {
        value: String,
        def: String,
    },
    /// Record a metric + its current value as an append-only Manual observation
    /// (wizard steps 4/5/7). Signal is derived, never set here.
    UpsertManualMetric {
        id: MetricId,
        name: String,
        role: MetricRole,
        stage_kind: Option<StageKind>,
        target: String,
        amber: AmberBand,
        value: String,
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
            Command::CreateProject { id, name, kind } => {
                self.store
                    .create_project(NewProject {
                        id,
                        name,
                        kind,
                        desc: String::new(),
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

            Command::UpsertManualMetric {
                id,
                name,
                role,
                stage_kind,
                target,
                amber,
                value,
            } => {
                let p = self.active()?;
                self.store
                    .upsert_metric(NewMetric {
                        id,
                        project_id: p,
                        role,
                        stage_kind,
                        name,
                        def: String::new(),
                        target_raw: target,
                        amber,
                        last_target: String::new(),
                        driver: String::new(),
                        pos: 0,
                    })
                    .await?;
                // The value is born as an explicit Manual observation; the signal
                // it implies is computed later by recompute, never set here.
                self.store
                    .append_observation(id, SourceKind::Manual, &value, now())
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
                let mut run_events = Vec::new();
                self.engine
                    .run_workflow(&spec, &ctx, |e| run_events.push(e))
                    .await
                    .map_err(|e| AppError::Engine(e.to_string()))?;

                // Drain run events into persisted messages + app events (no async
                // inside the engine callback).
                for e in run_events {
                    match e {
                        RunEvent::PhaseStarted { idx, name } => {
                            self.emit(Event::WorkflowProgress {
                                phase_idx: idx,
                                status: format!("started:{name}"),
                            });
                        }
                        RunEvent::PhaseCompleted { idx, output } => {
                            self.store
                                .append_message(session, Role::Agent, &output.text)
                                .await?;
                            self.emit(Event::SessionMessageAdded {
                                session,
                                role: Role::Agent,
                                text: output.text,
                            });
                            self.emit(Event::WorkflowProgress {
                                phase_idx: idx,
                                status: "completed".into(),
                            });
                        }
                        RunEvent::WorkflowDone { .. } => self.emit(Event::WorkflowDone),
                        RunEvent::WorkflowFailed { error } => return Err(AppError::Engine(error)),
                    }
                }
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
                self.state.view = match proj.phase {
                    ProjectPhase::ColdStart => View::Wizard,
                    ProjectPhase::Running => View::App,
                };
                self.state.wizard_step = proj.cold_step.unwrap_or(0);
                self.emit(Event::ViewChanged(self.state.view));
            }

            Command::BackToProjects => {
                self.state.view = View::Projects;
                self.emit(Event::ViewChanged(View::Projects));
            }

            Command::SetPanel(p) => self.state.panel = p,
            Command::SetScope(s) => self.state.scope = s,
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
