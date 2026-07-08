//! **Exit gate.** One headless integration test drives the whole spine with no
//! UI:
//!
//!   CreateProject → creation flow (cycle, brief, north star, a Manual metric)
//!   → CompleteCreation → RunWorkflow(mock) → SendSessionMessage
//!   → everything in SQLite
//!
//! then **kills the store, reopens the same file**, and asserts the project,
//! phase, north_star, active_stage and session messages all survive, and every
//! persisted signal equals an independent `bw_core` derive (recompute wrote it,
//! not a hand).

use bw_app::{App, Command, Event, View};
use bw_core::derive::{evaluate_metric, measure, parse_target, reduce_worst_of};
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
        .join(format!("bw_spine_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

fn workflow() -> WorkflowSpec {
    WorkflowSpec {
        id: WorkflowId::new(),
        name: "「原型」标准工作流".into(),
        kind: WorkflowKind::Dynamic {
            origin: "阶段标准模板".into(),
            stage: "原型".into(),
        },
        prompt: "证据→洞察→假设→原型→验证".into(),
        goal: "产出验证过的原型 + 北极星草案".into(),
        stage_ref: Some(1),
        phases: vec!["证据".into(), "洞察".into(), "假设".into()],
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
        let mut app = App::new(
            store.clone(),
            Engine::new(Arc::new(MockExecutor::new())),
            ClaudeCliConfig::default(),
        );
        let mut rx = app.subscribe();

        app.dispatch(Command::CreateProject {
            id: project,
            name: "增长看板".into(),
            kind: "看板 / 网页应用".into(),
            desc: "把 agent 会话里长出的工作流沉淀成可复用资产".into(),
        })
        .await
        .unwrap();
        assert_eq!(app.snapshot().view, View::Create);

        app.dispatch(Command::SetCycle {
            cycle: ProjectCycle::Explore,
        })
        .await
        .unwrap();
        app.dispatch(Command::UpdateBrief {
            benchmark: "n8n\nDify".into(),
            opportunity: "被持续复用 · 效率可量化提升".into(),
        })
        .await
        .unwrap();
        app.dispatch(Command::UpdateNorthStar {
            value: "每周被复用的工作流运行数".into(),
            def: "非作者导入并成功跑通的次数 / 周".into(),
        })
        .await
        .unwrap();

        // Draft review: one leading metric, current value recorded as a Manual observation.
        app.dispatch(Command::UpsertManualMetric {
            id: leading,
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
        assert_eq!(app.snapshot().view, View::App);

        // Run a (mock) workflow → phase outputs become session messages.
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
            spec: workflow(),
        })
        .await
        .unwrap();

        app.dispatch(Command::SendSessionMessage {
            session,
            text: "把假设卡补上".into(),
        })
        .await
        .unwrap();

        // The first handoff: Prototype → Build, cleanly (no risky flag).
        app.dispatch(Command::HandoffStage {
            risky: false,
            note: "原型经真实使用验证 · 北极星草案已定 · Spec 骨架已固化".into(),
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
    assert!(events.iter().any(|e| matches!(
        e,
        Event::StageHandoff {
            from: StageKind::Prototype,
            to: StageKind::Build,
            risky: false
        }
    )));
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
    assert_eq!(proj.north_star, "每周被复用的工作流运行数");
    assert_eq!(proj.cycle, ProjectCycle::Explore);
    assert_eq!(proj.active_stage, StageKind::Build); // survived the handoff

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
        .find(|x| x.name == "周复用次数")
        .unwrap();
    assert_eq!(m.value_raw, "8");
    assert_eq!(m.signal, Some(expect_metric), "persisted signal == derived");
    assert_eq!(m.hit, Some(true));

    let proto_stage = sigs
        .stages
        .iter()
        .find(|s| s.kind == StageKind::Prototype)
        .unwrap();
    assert_eq!(proto_stage.routine, Some(Signal::Green));

    let stage_signals: Vec<Signal> = sigs.stages.iter().map(|s| s.routine.unwrap()).collect();
    assert_eq!(sigs.stages.len(), 5);
    assert_eq!(
        sigs.project,
        Some(reduce_worst_of(stage_signals).into_inner()),
        "project signal == worst-of stages (no hand-set)"
    );
    assert_eq!(sigs.project, Some(Signal::Green));

    // The handoff audit trail survived, with the honest note attached.
    let handoffs = store.list_handoffs(project).await.unwrap();
    assert_eq!(handoffs.len(), 1);
    assert!(!handoffs[0].risky);
    assert_eq!(handoffs[0].from_stage, StageKind::Prototype);
    assert_eq!(handoffs[0].to_stage, StageKind::Build);

    // Session + messages survived.
    let sessions = store.list_sessions(project).await.unwrap();
    assert_eq!(sessions.len(), 1);
    let msgs = store.session_messages(session).await.unwrap();
    assert_eq!(msgs.len(), 5);
    assert!(msgs.iter().any(|m| m.text.contains("把假设卡补上")));
    assert!(
        msgs.iter()
            .filter(|m| matches!(m.role, bw_core::model::Role::Agent))
            .count()
            == 4
    );

    let _ = std::fs::remove_file(&path);
}
