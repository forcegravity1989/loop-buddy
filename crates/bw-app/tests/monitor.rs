//! **P2 monitoring-loop test.** The operating view's heartbeat, headless:
//!
//!   RecordObservation → recompute → signal flips (derived, never set)
//!   UpdateWeekPlan (target moves) → recompute → same value, new meaning
//!   observation history accumulates append-only (the real sparkline series)
//!   RunWorkflow emits progress LIVE (progress events precede persistence)
//!
//! Every assertion checks the persisted cache against an independent `bw_core`
//! derive — the UI can only ever show what the chain computed.

use bw_app::{App, Command, Event};
use bw_core::derive::{evaluate_metric, measure, parse_target};
use bw_core::model::{Cadence, LoopConfig, SourceKind, StageKind, WorkflowKind, WorkflowSpec};
use bw_core::{MetricId, ProjectId, SessionId, Signal, WorkflowId};
use bw_engine::{Engine, MockExecutor};
use bw_store::{MetricRole, SessionKind, SqliteStore, Store};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::broadcast::error::TryRecvError;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_monitor_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

async fn wizard_to_running(app: &mut App, project: ProjectId, metric: MetricId) {
    app.dispatch(Command::CreateProject {
        id: project,
        name: "增长看板".into(),
        kind: "看板 / 网页应用".into(),
        desc: String::new(),
    })
    .await
    .unwrap();
    app.dispatch(Command::UpdateBrief {
        benchmark: "Linear\nHeight".into(),
        opportunity: "深耕单人 Builder 的运营闭环".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::UpsertManualMetric {
        id: metric,
        name: "每周有效对话数".into(),
        def: "7日窗口内 ≥2 轮的对话数".into(),
        role: MetricRole::Leading,
        stage_kind: Some(StageKind::Leading),
        target: "≥5".into(),
        amber: Default::default(),
        value: "8".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteWizard).await.unwrap();
}

/// Independent re-derive of a weekly Manual metric, same inputs as recompute.
fn derive_now(value: &str, target: &str) -> Signal {
    let t = OffsetDateTime::now_utc();
    evaluate_metric(
        &measure(value, t, SourceKind::Manual, &Cadence::Weekly, t),
        &parse_target(target).unwrap(),
        &[],
    )
    .signal()
}

#[tokio::test]
async fn record_observation_rederives_never_sets() {
    let path = tmp_db();
    let project = ProjectId::new();
    let metric = MetricId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(store.clone(), Engine::new(MockExecutor::new()));
    wizard_to_running(&mut app, project, metric).await;

    // Wizard value 8 against ≥5 ⇒ green.
    let sigs = store.persisted_signals(project).await.unwrap();
    assert_eq!(sigs.metrics[0].signal, Some(Signal::Green));
    assert_eq!(sigs.project, Some(Signal::Green));

    // The monitoring heartbeat: a worse value arrives as a new observation.
    app.dispatch(Command::RecordObservation {
        metric,
        value: "3".into(),
    })
    .await
    .unwrap();

    let sigs = store.persisted_signals(project).await.unwrap();
    let expect = derive_now("3", "≥5");
    assert_ne!(expect, Signal::Green, "3 misses ≥5");
    assert_eq!(sigs.metrics[0].value_raw, "3", "latest observation wins");
    assert_eq!(sigs.metrics[0].signal, Some(expect), "persisted == derived");
    assert_eq!(
        sigs.project,
        Some(expect),
        "the miss rolls all the way up to the project signal"
    );

    // Append-only history: both values, oldest first — the real sparkline series.
    let obs = store.list_observations(project).await.unwrap();
    assert_eq!(
        obs.iter().map(|o| o.raw.as_str()).collect::<Vec<_>>(),
        vec!["8", "3"]
    );
    assert!(obs.iter().all(|o| o.metric_id == metric));

    // Empty values are refused — no fabricated observations.
    assert!(app
        .dispatch(Command::RecordObservation {
            metric,
            value: "  ".into(),
        })
        .await
        .is_err());

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn week_plan_edit_moves_target_and_rederives() {
    let path = tmp_db();
    let project = ProjectId::new();
    let metric = MetricId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(store.clone(), Engine::new(MockExecutor::new()));
    wizard_to_running(&mut app, project, metric).await;

    // Step-7 style edit: raise this week's bar, keep last week's for the table.
    app.dispatch(Command::UpdateWeekPlan {
        metric,
        new_target: "≥12".into(),
        last_target: "≥5".into(),
        driver: "在首页新增引导入口".into(),
    })
    .await
    .unwrap();

    let sigs = store.persisted_signals(project).await.unwrap();
    let m = &sigs.metrics[0];
    assert_eq!(m.target_raw, "≥12");
    assert_eq!(m.last_target, "≥5");
    assert_eq!(m.driver, "在首页新增引导入口");
    // Same value (8), new target (≥12): meaning changed, and it was re-derived.
    let expect = derive_now("8", "≥12");
    assert_ne!(expect, Signal::Green);
    assert_eq!(m.signal, Some(expect), "persisted == derived after edit");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn run_progress_streams_before_persistence() {
    let path = tmp_db();
    let project = ProjectId::new();
    let metric = MetricId::new();
    let session = SessionId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(store.clone(), Engine::new(MockExecutor::new()));
    let mut rx = app.subscribe();
    wizard_to_running(&mut app, project, metric).await;

    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: Some(StageKind::CompetitorInsight),
        kind: SessionKind::Create,
        title: "竞品洞察 · 首轮".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunWorkflow {
        session,
        spec: WorkflowSpec {
            id: WorkflowId::new(),
            name: "竞品洞察工作流".into(),
            kind: WorkflowKind::Dynamic {
                origin: "环节".into(),
                stage: "竞品洞察".into(),
            },
            prompt: "界定→采集→分析".into(),
            goal: "产出竞品矩阵".into(),
            stage_ref: Some(1),
            phases: vec!["界定".into(), "采集".into(), "分析".into()],
            agents: vec![],
            skills: vec![],
            loop_config: LoopConfig {
                retries: 1,
                max_iter: 3,
            },
        },
    })
    .await
    .unwrap();

    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(e) => events.push(e),
            Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }

    // Live ordering: every phase-progress event lands before the first persisted
    // message event — the UI sees the run advance, then the transcript arrive.
    let last_progress = events
        .iter()
        .rposition(|e| matches!(e, Event::WorkflowProgress { .. }))
        .expect("progress events emitted");
    let first_msg = events
        .iter()
        .position(|e| matches!(e, Event::SessionMessageAdded { .. }))
        .expect("messages persisted");
    assert!(
        last_progress < first_msg,
        "progress must stream live, not replay after persistence"
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                Event::WorkflowProgress { status, .. } if status.starts_with("started:")
            ))
            .count(),
        3,
        "one started event per phase"
    );
    assert!(events.iter().any(|e| matches!(e, Event::WorkflowDone)));

    // And the transcript is really in the store.
    assert_eq!(store.session_messages(session).await.unwrap().len(), 3);

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn boot_lists_and_rederives_running_projects() {
    let path = tmp_db();
    let project = ProjectId::new();
    let metric = MetricId::new();

    {
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
        let mut app = App::new(store.clone(), Engine::new(MockExecutor::new()));
        wizard_to_running(&mut app, project, metric).await;
    }

    // Fresh process: Boot loads the wall and re-derives against the current clock.
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(store.clone(), Engine::new(MockExecutor::new()));
    app.dispatch(Command::Boot).await.unwrap();

    let projects = &app.snapshot().projects;
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "增长看板");
    assert_eq!(projects[0].benchmark, "Linear\nHeight");
    assert_eq!(projects[0].signal, Some(derive_now("8", "≥5")));

    let _ = std::fs::remove_file(&path);
}
