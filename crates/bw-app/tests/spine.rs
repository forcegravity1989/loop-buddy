//! **P1 exit gate (plan `04 §P1`).** One headless integration test drives the
//! whole spine with no UI:
//!
//!   CreateProject → 7-step wizard (record a Manual value) → CompleteWizard
//!   → RunWorkflow(mock) → SendSessionMessage → everything in SQLite
//!
//! then **kills the store, reopens the same file**, and asserts the project,
//! phase, north_star and session messages all survive, and every persisted
//! signal equals an independent `bw_core` derive (recompute wrote it, not a hand).

use bw_app::{App, Command, Event, View};
use bw_core::derive::{evaluate_metric, measure, parse_target, reduce_worst_of};
use bw_core::model::{Cadence, LoopConfig, SourceKind, StageKind, WorkflowKind, WorkflowSpec};
use bw_core::{MetricId, ProjectId, SessionId, Signal, WorkflowId};
use bw_engine::{Engine, MockExecutor};
use bw_store::{MetricRole, SessionKind, SqliteStore, Store};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::broadcast::error::TryRecvError;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_spine_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

fn workflow() -> WorkflowSpec {
    WorkflowSpec {
        id: WorkflowId::new(),
        name: "竞品洞察工作流".into(),
        kind: WorkflowKind::Dynamic {
            origin: "向导".into(),
            stage: "竞品洞察".into(),
        },
        prompt: "界定→采集→结构化→分析".into(),
        goal: "产出竞品矩阵".into(),
        stage_ref: Some(1),
        phases: vec!["界定".into(), "采集".into(), "分析".into()],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
    }
}

#[tokio::test]
async fn full_spine_survives_kill_and_reopen() {
    let path = tmp_db();
    let project = ProjectId::new();
    let leading = MetricId::new();
    let session = SessionId::new();

    let mut events: Vec<Event> = Vec::new();

    // ── phase 1: live app, full flow ───────────────────────────────────────
    {
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
        let mut app = App::new(store.clone(), Engine::new(MockExecutor::new()));
        let mut rx = app.subscribe();

        app.dispatch(Command::CreateProject {
            id: project,
            name: "增长看板".into(),
            kind: "看板 / 网页应用".into(),
            desc: "面向增长团队的实时看板".into(),
        })
        .await
        .unwrap();
        assert_eq!(app.snapshot().view, View::Wizard);

        for step in 1..=7u8 {
            app.dispatch(Command::SetWizardStep { step }).await.unwrap();
        }
        app.dispatch(Command::UpdateNorthStar {
            value: "每周留存对话用户数".into(),
            def: "7日内有≥2次有效对话的用户".into(),
        })
        .await
        .unwrap();

        // Step 4: leading metric, current value recorded as a Manual observation.
        app.dispatch(Command::UpsertManualMetric {
            id: leading,
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
        assert_eq!(app.snapshot().view, View::App);

        // Run a (mock) workflow → phase outputs become session messages.
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
            spec: workflow(),
        })
        .await
        .unwrap();

        app.dispatch(Command::SendSessionMessage {
            session,
            text: "把竞品矩阵补上 Notion".into(),
        })
        .await
        .unwrap();

        // drain the event stream
        loop {
            match rx.try_recv() {
                Ok(e) => events.push(e),
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(_)) => continue,
            }
        }
        // store + app dropped here → db closed (simulates process exit)
    }

    // event-stream sanity (before reopen)
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::ViewChanged(View::App))));
    assert!(events.iter().any(|e| matches!(e, Event::WorkflowDone)));
    let msg_events = events
        .iter()
        .filter(|e| matches!(e, Event::SessionMessageAdded { .. }))
        .count();
    assert_eq!(
        msg_events, 5,
        "3 mock phase outputs + 1 builder + 1 mock reply"
    );

    // ── phase 2: reopen the SAME file, assert durability + derive integrity ──
    let store = SqliteStore::open(&path).await.unwrap();

    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.name, "增长看板");
    assert_eq!(
        proj.phase as u8,
        bw_core::model::ProjectPhase::Running as u8
    );
    assert_eq!(proj.north_star, "每周留存对话用户数");

    let sigs = store.persisted_signals(project).await.unwrap();

    // Independent re-derive: persisted cache must equal what bw_core computes.
    let t = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let expect_metric = evaluate_metric(
        &measure("8", t, SourceKind::Manual, &Cadence::Weekly, t),
        &parse_target("≥5").unwrap(),
        &[],
    )
    .signal();
    assert_eq!(expect_metric, Signal::Green);

    let m = sigs
        .metrics
        .iter()
        .find(|x| x.name == "每周有效对话数")
        .unwrap();
    assert_eq!(m.value_raw, "8");
    assert_eq!(m.signal, Some(expect_metric), "persisted signal == derived");
    assert_eq!(m.hit, Some(true));

    let lead_stage = sigs
        .stages
        .iter()
        .find(|s| s.kind == StageKind::Leading)
        .unwrap();
    assert_eq!(lead_stage.routine, Some(Signal::Green));

    let stage_signals: Vec<Signal> = sigs.stages.iter().map(|s| s.routine.unwrap()).collect();
    assert_eq!(sigs.stages.len(), 7);
    assert_eq!(
        sigs.project,
        Some(reduce_worst_of(stage_signals).into_inner()),
        "project signal == worst-of stages (no hand-set)"
    );
    assert_eq!(sigs.project, Some(Signal::Green));

    // Session + messages survived.
    let sessions = store.list_sessions(project).await.unwrap();
    assert_eq!(sessions.len(), 1);
    let msgs = store.session_messages(session).await.unwrap();
    assert_eq!(msgs.len(), 5);
    assert!(msgs
        .iter()
        .any(|m| m.text.contains("把竞品矩阵补上 Notion")));
    assert!(
        msgs.iter()
            .filter(|m| matches!(m.role, bw_core::model::Role::Agent))
            .count()
            == 4
    );

    let _ = std::fs::remove_file(&path);
}
