//! A4 — the per-stage "完成 Issue 数" machine metric: idempotent seeding with
//! an empty target (honest Unknown, never a fake green), the Done-edge feed
//! (change-guarded), and the HandoffStage open-issues guard.

use bw_app::{App, Command};
use bw_core::model::{Cadence, IssuePriority, IssueStatus, ProjectCycle, Signal, StageKind};
use bw_core::{IssueId, ProjectId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_stage_metrics_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

async fn running_project(app: &mut App, project: ProjectId) {
    app.dispatch(Command::CreateProject {
        id: project,
        name: "A4 测试项目".into(),
        kind: "自举".into(),
        desc: String::new(),
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCycle {
        cycle: ProjectCycle::Explore,
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();
}

async fn done_metric_value(store: &Arc<dyn Store>, project: ProjectId, stage: StageKind) -> String {
    store
        .persisted_signals(project)
        .await
        .unwrap()
        .metrics
        .into_iter()
        .find(|m| m.name == "阶段完成 Issue 数" && m.stage_kind == Some(stage))
        .map(|m| m.value_raw)
        .unwrap_or_default()
}

#[tokio::test]
async fn stage_done_metric_seeded_per_stage_targetless_and_idempotent() {
    let path = tmp_db();
    let project = ProjectId::new();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    running_project(&mut app, project).await;

    let metrics = store.persisted_signals(project).await.unwrap().metrics;
    let done_metrics: Vec<_> = metrics
        .iter()
        .filter(|m| m.name == "阶段完成 Issue 数")
        .collect();
    assert_eq!(
        done_metrics.len(),
        StageKind::ALL.len(),
        "one seeded metric per stage"
    );
    for m in &done_metrics {
        assert!(
            m.target_raw.trim().is_empty(),
            "empty target ⇒ honest Unknown"
        );
        assert_ne!(
            m.signal,
            Some(Signal::Green),
            "a count with no goal is never green"
        );
    }

    // Idempotent: Boot re-seeds every project — no duplicates.
    app.dispatch(Command::Boot).await.unwrap();
    let count2 = store
        .persisted_signals(project)
        .await
        .unwrap()
        .metrics
        .iter()
        .filter(|m| m.name == "阶段完成 Issue 数")
        .count();
    assert_eq!(count2, StageKind::ALL.len(), "re-seed is a no-op");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn done_edge_feeds_stage_done_count_only_on_change() {
    let path = tmp_db();
    let project = ProjectId::new();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    running_project(&mut app, project).await;

    let i1 = IssueId::new();
    let i2 = IssueId::new();
    app.dispatch(Command::CreateIssue {
        id: i1,
        stage: StageKind::Build,
        title: "t1".into(),
        desc: String::new(),
        priority: IssuePriority::Medium,
    })
    .await
    .unwrap();
    app.dispatch(Command::CreateIssue {
        id: i2,
        stage: StageKind::Build,
        title: "t2".into(),
        desc: String::new(),
        priority: IssuePriority::Medium,
    })
    .await
    .unwrap();

    // Done one → the Build metric reads "1"; Done another → "2".
    app.dispatch(Command::TransitionIssue {
        id: i1,
        status: IssueStatus::Done,
    })
    .await
    .unwrap();
    assert_eq!(
        done_metric_value(&store, project, StageKind::Build).await,
        "1"
    );
    app.dispatch(Command::TransitionIssue {
        id: i2,
        status: IssueStatus::Done,
    })
    .await
    .unwrap();
    assert_eq!(
        done_metric_value(&store, project, StageKind::Build).await,
        "2"
    );

    // A stage with no Done issues is unaffected (empty, not "2").
    let proto = done_metric_value(&store, project, StageKind::Prototype).await;
    assert!(proto.is_empty(), "other stages unaffected: {proto:?}");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn handoff_with_open_issues_is_forced_risky_and_tagged() {
    let path = tmp_db();
    let project = ProjectId::new();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    running_project(&mut app, project).await;

    // An open (Backlog = non-terminal) issue in the active stage (Prototype).
    app.dispatch(Command::CreateIssue {
        id: IssueId::new(),
        stage: StageKind::Prototype,
        title: "未完的活".into(),
        desc: String::new(),
        priority: IssuePriority::Medium,
    })
    .await
    .unwrap();

    // Caller claims NOT risky — but the open work forces it honest.
    app.dispatch(Command::HandoffStage {
        risky: false,
        note: String::new(),
    })
    .await
    .unwrap();

    let h = store
        .list_handoffs(project)
        .await
        .unwrap()
        .first()
        .expect("handoff recorded")
        .clone();
    assert!(h.risky, "open issues force a risky handoff");
    assert!(
        h.note.contains("留") && h.note.contains("未完"),
        "note tags the open count: {}",
        h.note
    );

    let _ = std::fs::remove_file(&path);
}
