//! **Monitoring-loop test.** The operating view's heartbeat, headless:
//!
//!   RecordObservation → recompute → signal flips (derived, never set)
//!   UpdateWeekPlan (target moves) → recompute → same value, new meaning
//!   observation history accumulates append-only (the real sparkline series)
//!   RunWorkflow emits progress LIVE (progress events precede persistence)
//!   ToggleDod + HandoffStage → risky handoff still happens, just audited
//!   Ops → Prototype reflux closes the loop
//!
//! Every assertion checks the persisted cache against an independent `bw_core`
//! derive — the UI can only ever show what the chain computed.

use bw_app::{App, Command, Event};
use bw_core::derive::{evaluate_metric, measure, parse_target};
use bw_core::model::{
    Cadence, LoopConfig, ProjectCycle, SourceKind, StageKind, WorkflowKind, WorkflowSpec,
};
use bw_core::{MetricId, ProjectId, SessionId, Signal, WorkflowId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
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

async fn creation_to_running(app: &mut App, project: ProjectId, metric: MetricId) {
    app.dispatch(Command::CreateProject {
        id: project,
        name: "增长看板".into(),
        kind: "看板 / 网页应用".into(),
        desc: "把 agent 会话里长出的工作流沉淀成可复用资产".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCycle {
        cycle: ProjectCycle::Explore,
    })
    .await
    .unwrap();
    app.dispatch(Command::UpdateBrief {
        benchmark: "n8n\nDify".into(),
        opportunity: "被持续复用".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::UpsertManualMetric {
        id: metric,
        name: "周复用次数".into(),
        def: "非作者触发的工作流运行数".into(),
        role: MetricRole::Leading,
        stage_kind: Some(StageKind::Prototype),
        target: "≥5".into(),
        amber: Default::default(),
        value: "8".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();
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
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    creation_to_running(&mut app, project, metric).await;

    // Draft value 8 against ≥5 ⇒ green.
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
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    creation_to_running(&mut app, project, metric).await;

    // Progress-panel style edit: raise this week's bar, keep last week's for the table.
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
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let mut rx = app.subscribe();
    creation_to_running(&mut app, project, metric).await;

    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: Some(StageKind::Prototype),
        kind: SessionKind::Create,
        title: "原型 · 首轮".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunWorkflow {
        session,
        spec: WorkflowSpec {
            id: WorkflowId::new(),
            name: "「原型」标准工作流".into(),
            kind: WorkflowKind::Dynamic {
                origin: "阶段标准模板".into(),
                stage: "原型".into(),
            },
            prompt: "证据→洞察→假设".into(),
            goal: "产出验证过的原型".into(),
            stage_ref: Some(1),
            phases: vec!["证据".into(), "洞察".into(), "假设".into()],
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
async fn dod_toggle_and_risky_handoff_then_reflux() {
    let path = tmp_db();
    let project = ProjectId::new();
    let metric = MetricId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let mut rx = app.subscribe();
    creation_to_running(&mut app, project, metric).await;

    app.dispatch(Command::ToggleDod {
        stage_kind: StageKind::Prototype,
        index: 0,
    })
    .await
    .unwrap();
    let stages = store.list_stages(project).await.unwrap();
    let proto = stages
        .iter()
        .find(|s| s.kind == StageKind::Prototype)
        .unwrap();
    assert!(proto.dod[0]);

    // Hand off without checking the rest — allowed, marked risky.
    app.dispatch(Command::HandoffStage {
        risky: true,
        note: "性能基线未测 · 带险交棒".into(),
    })
    .await
    .unwrap();
    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.active_stage, StageKind::Build);

    // Walk the rest of the loop back to Ops, then reflux to Prototype.
    for _ in 0..3 {
        app.dispatch(Command::HandoffStage {
            risky: false,
            note: "干净交棒".into(),
        })
        .await
        .unwrap();
    }
    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.active_stage, StageKind::Ops);

    app.dispatch(Command::HandoffStage {
        risky: false,
        note: "复盘洞察已回流原型段".into(),
    })
    .await
    .unwrap();
    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.active_stage, StageKind::Prototype, "the loop closes");

    let handoffs = store.list_handoffs(project).await.unwrap();
    assert_eq!(handoffs.len(), 5);
    assert!(handoffs.iter().any(|h| h.risky));

    let mut events = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(e) => events.push(e),
            Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::StageHandoff { .. }))
            .count(),
        5
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn boot_lists_and_rederives_running_projects() {
    let path = tmp_db();
    let project = ProjectId::new();
    let metric = MetricId::new();

    {
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
        let mut app = App::new(
            store.clone(),
            Engine::new(Arc::new(MockExecutor::new())),
            ClaudeCliConfig::default(),
        );
        creation_to_running(&mut app, project, metric).await;
    }

    // Fresh process: Boot loads the wall and re-derives against the current clock.
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    let projects = &app.snapshot().projects;
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].name, "增长看板");
    assert_eq!(projects[0].benchmark, "n8n\nDify");
    assert_eq!(projects[0].signal, Some(derive_now("8", "≥5")));

    let _ = std::fs::remove_file(&path);
}
