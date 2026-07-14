//! **Real scheduler exit gate.** Headless, no UI, no clock mocking — proves
//! `App::tick_scheduler` (not the manual "▶ 立即执行" path) really auto-fires
//! due cron tasks: real session + real message + real `uses` bump + real
//! `last_run_at`, while leaving whatever project/view the caller currently
//! has open completely untouched (the whole point of the background-fire
//! design — see `tick_scheduler`'s own doc comment in `bw-app/src/lib.rs`).
//!
//! Also proves the guard rails that keep it honest: paused tasks never
//! fire, "全部项目" (unbound) tasks never fire, tasks whose target doesn't
//! name a real hub workflow never fire, and a task that just fired doesn't
//! immediately re-fire on the next tick.

use bw_app::{App, Command, View};
use bw_core::model::{Cadence, CronStatus, HubSource, LoopConfig, Maturity, ProjectCycle};
use bw_core::{CronTaskId, ProjectId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_scheduler_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

async fn quick_project(app: &mut App, name: &str) -> ProjectId {
    let id = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id,
        name: name.into(),
        kind: "验证".into(),
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
    id
}

#[tokio::test]
async fn tick_scheduler_auto_fires_due_tasks_without_hijacking_the_open_project() {
    let path = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );

    // Project A owns the cron tasks; Project B is what the user is *currently
    // looking at* when the tick happens — the scenario the no-hijack
    // guarantee exists for.
    let project_a = quick_project(&mut app, "项目 A · 定时目标").await;
    let project_b = quick_project(&mut app, "项目 B · 用户当前所在").await;
    assert_eq!(app.snapshot().active_project, Some(project_b));
    assert_eq!(app.snapshot().view, View::App);

    // A real, global Static hub workflow to be the cron target.
    let workflow_id = bw_core::WorkflowId::new();
    app.dispatch(Command::CreateWorkflowSpec {
        id: workflow_id,
        name: "验证 · 定时工作流".into(),
        prompt: "p".into(),
        goal: "g".into(),
        stage_ref: None,
        phases: vec!["步骤一".into()],
        phase_prompts: vec![],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 1,
        },
        maturity: Maturity::Mature,
        scope: String::new(),
        source: HubSource::SelfBuilt,
        trigger: None,
    })
    .await
    .unwrap();

    // ── four cron tasks, each testing one real guard rail ──
    let due = CronTaskId::new();
    app.dispatch(Command::CreateCronTask {
        id: due,
        name: "到期 · 应自动触发".into(),
        target: "验证 · 定时工作流".into(),
        schedule: Cadence::Daily,
        project_id: Some(project_a),
    })
    .await
    .unwrap();

    let paused = CronTaskId::new();
    app.dispatch(Command::CreateCronTask {
        id: paused,
        name: "已暂停 · 不应触发".into(),
        target: "验证 · 定时工作流".into(),
        schedule: Cadence::Daily,
        project_id: Some(project_a),
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCronStatus {
        id: paused,
        status: CronStatus::Paused,
    })
    .await
    .unwrap();

    let unbound = CronTaskId::new();
    app.dispatch(Command::CreateCronTask {
        id: unbound,
        name: "全部项目 · 不应触发".into(),
        target: "验证 · 定时工作流".into(),
        schedule: Cadence::Daily,
        project_id: None,
    })
    .await
    .unwrap();

    let unmatched = CronTaskId::new();
    app.dispatch(Command::CreateCronTask {
        id: unmatched,
        name: "目标不存在 · 不应触发".into(),
        target: "这个名字在 Hub 里不存在".into(),
        schedule: Cadence::Daily,
        project_id: Some(project_a),
    })
    .await
    .unwrap();

    let before_view = app.snapshot().view;
    let before_panel = app.snapshot().panel;
    let before_scope = app.snapshot().scope;
    let before_active_session = app.snapshot().active_session;

    // ── the real tick ──
    let fired = app.tick_scheduler().await.unwrap();
    assert_eq!(
        fired,
        vec![due],
        "exactly the one real due+bound+matched task must fire"
    );

    // No UI hijack: everything about "what the user is currently looking at"
    // must be byte-for-byte unchanged, even though the fired task belongs to
    // a *different* project than the one currently open.
    assert_eq!(app.snapshot().active_project, Some(project_b));
    assert_eq!(app.snapshot().view, before_view);
    assert_eq!(app.snapshot().panel, before_panel);
    assert_eq!(app.snapshot().scope, before_scope);
    assert_eq!(app.snapshot().active_session, before_active_session);

    // Real side effects, read back from the store (not trusted from memory):
    let specs = store.list_workflow_specs().await.unwrap();
    let ran = specs.iter().find(|w| w.id == workflow_id).unwrap();
    match ran.kind {
        bw_core::model::WorkflowKind::Static { uses, .. } => {
            assert_eq!(
                uses, 1,
                "a real auto-fire must bump uses exactly like a manual run"
            )
        }
        bw_core::model::WorkflowKind::Dynamic { .. } => panic!("expected Static"),
    }

    let sessions_a = store.list_sessions(project_a).await.unwrap();
    let fired_session = sessions_a
        .iter()
        .find(|s| s.title.contains("定时触发"))
        .expect("tick_scheduler must really create a session under project A, not B");
    let msgs = store.session_messages(fired_session.id).await.unwrap();
    assert!(
        !msgs.is_empty(),
        "the auto-fired run must really produce output, not a silent no-op"
    );

    let crons = store.list_cron_tasks().await.unwrap();
    let due_row = crons.iter().find(|c| c.id == due).unwrap();
    assert_eq!(due_row.status, CronStatus::Normal);
    let last_run_at = due_row
        .last_run_at
        .expect("a real fire must set a real clock");
    assert!(
        (time::OffsetDateTime::now_utc() - last_run_at)
            .whole_seconds()
            .abs()
            < 10,
        "last_run_at must be the real current time"
    );
    assert!(
        !due_row.last_run.is_empty(),
        "the display label must also be set, same as a manual trigger"
    );

    // The three guard rails: genuinely untouched, not just "didn't error".
    for id in [paused, unbound, unmatched] {
        let row = crons.iter().find(|c| c.id == id).unwrap();
        assert_eq!(row.last_run_at, None, "{id:?} must never have been touched");
        assert!(row.last_run.is_empty());
    }
    assert_eq!(
        crons.iter().find(|c| c.id == paused).unwrap().status,
        CronStatus::Paused,
        "pause must survive a tick untouched — it is real human intervention"
    );

    // ── immediately ticking again must not double-fire the task that just ran ──
    let fired_again = app.tick_scheduler().await.unwrap();
    assert_eq!(
        fired_again,
        Vec::<CronTaskId>::new(),
        "a Daily task that ran moments ago is not due again — no infinite/duplicate firing"
    );

    let _ = std::fs::remove_file(&path);
}
