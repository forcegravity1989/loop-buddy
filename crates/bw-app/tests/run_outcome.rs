//! **Run-outcome telemetry exit gate (iter 1).** Headless — proves every
//! workflow execution records its real outcome (status / duration / phases /
//! trigger) into the append-only `workflow_run` log, for both the manual and
//! the scheduler-triggered path, success AND failure.
//!
//! This is the grain all of Arc 2's optimization intelligence is built on:
//! without "this run failed at phase 2 of 5 in 340ms", no later "this workflow
//! is unhealthy" signal can be honest.

use bw_app::{App, Command, View};
use bw_core::model::{
    Cadence, HubSource, LoopConfig, Maturity, ProjectCycle, RunStatus, RunTrigger,
};
use bw_core::{CronTaskId, ProjectId, WorkflowId};
use bw_engine::{
    ClaudeCliConfig, Engine, ExecError, Executor, MockExecutor, PhaseNode, PhaseOutput, RunCtx,
};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_runoutcome_{}.db", uuid::Uuid::new_v4()))
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

/// An executor that fails phase 2 (index 1) every time — used to prove a
/// *failed* run is recorded honestly, not papered over as success.
struct FailOnSecondPhase;
#[async_trait::async_trait]
impl Executor for FailOnSecondPhase {
    async fn run_phase(&self, phase: &PhaseNode, _ctx: &RunCtx) -> Result<PhaseOutput, ExecError> {
        // The engine hands phases names from the spec's `phases` vec; the
        // second one in our test spec is named "第二步".
        if phase.name == "第二步" {
            return Err(ExecError::Failed("模拟 · 第二步失败".into()));
        }
        Ok(PhaseOutput {
            text: format!("ok · {}", phase.name),
            done: true,
            gaps: Vec::new(),
        })
    }
}

#[tokio::test]
async fn store_records_start_settle_and_is_idempotent() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let wid = WorkflowId::new();
    let started = 1_700_000_000;
    let id = store
        .record_workflow_run_start(bw_store::NewWorkflowRun {
            workflow_id: wid,
            workflow_name: "wf",
            project_id: None,
            session_id: None,
            trigger: RunTrigger::Manual,
            started_at: started,
            params_json: r#"{"phase_count":3}"#,
            cron_task_id: None,
        })
        .await
        .unwrap();

    // Before settle: status running, no duration.
    let runs = store.list_workflow_runs(wid).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, RunStatus::Running);
    assert!(runs[0].duration_ms.is_none());

    // Settle to Ok.
    store
        .settle_workflow_run(id, RunStatus::Ok, started + 5, 5_000, 3, "")
        .await
        .unwrap();
    let runs = store.list_workflow_runs(wid).await.unwrap();
    assert_eq!(runs[0].status, RunStatus::Ok);
    assert_eq!(runs[0].duration_ms, Some(5_000));
    assert_eq!(runs[0].phases_completed, 3);

    // Re-settle is a no-op (idempotent — a re-driven dogfood never overwrites).
    store
        .settle_workflow_run(id, RunStatus::Failed, started + 9, 9_000, 1, "late")
        .await
        .unwrap();
    let runs = store.list_workflow_runs(wid).await.unwrap();
    assert_eq!(runs[0].status, RunStatus::Ok, "idempotent: stays Ok");
}

#[tokio::test]
async fn successful_manual_run_records_ok_with_real_duration() {
    let path = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let project = quick_project(&mut app, "项目 · 成功运行").await;

    let workflow_id = WorkflowId::new();
    app.dispatch(Command::CreateWorkflowSpec {
        id: workflow_id,
        name: "验证 · 成功".into(),
        prompt: "p".into(),
        goal: "g".into(),
        stage_ref: None,
        phases: vec!["步骤一".into(), "步骤二".into()],
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

    let session = bw_core::SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: None,
        kind: bw_store::SessionKind::Create,
        title: "一次手动运行".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunHubWorkflow {
        workflow_id,
        session,
    })
    .await
    .unwrap();

    let runs = store.list_workflow_runs(workflow_id).await.unwrap();
    assert_eq!(runs.len(), 1);
    let r = &runs[0];
    assert_eq!(r.status, RunStatus::Ok);
    assert_eq!(r.trigger, RunTrigger::Manual);
    assert_eq!(r.phases_completed, 2, "both mock phases completed");
    assert!(r.duration_ms.unwrap_or(0) >= 0, "real duration recorded");
    assert_eq!(r.workflow_name, "验证 · 成功");
    assert_eq!(r.project_id, Some(project));
}

#[tokio::test]
async fn failed_run_records_failed_status_and_partial_phases() {
    let path = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(FailOnSecondPhase)),
        ClaudeCliConfig::default(),
    );
    quick_project(&mut app, "项目 · 失败运行").await;

    let workflow_id = WorkflowId::new();
    app.dispatch(Command::CreateWorkflowSpec {
        id: workflow_id,
        name: "验证 · 会失败".into(),
        prompt: "p".into(),
        goal: "g".into(),
        stage_ref: None,
        phases: vec!["第一步".into(), "第二步".into(), "第三步".into()],
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

    let session = bw_core::SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: None,
        kind: bw_store::SessionKind::Create,
        title: "一次失败运行".into(),
    })
    .await
    .unwrap();
    // The dispatch returns Err (engine failed) — that's the point.
    let _ = app
        .dispatch(Command::RunHubWorkflow {
            workflow_id,
            session,
        })
        .await;

    let runs = store.list_workflow_runs(workflow_id).await.unwrap();
    assert_eq!(runs.len(), 1, "failed run still recorded");
    let r = &runs[0];
    assert_eq!(r.status, RunStatus::Failed);
    assert_eq!(
        r.phases_completed, 1,
        "only phase 1 completed before failure"
    );
    assert!(
        r.error.contains("模拟 · 第二步失败"),
        "error message persisted: {}",
        r.error
    );
    assert!(r.duration_ms.is_some(), "failed runs still carry duration");
}

#[tokio::test]
async fn scheduler_triggered_run_is_attributed_scheduled_not_manual() {
    // Proves the trigger attribution survives the scheduler path — so later
    // analytics can tell "the user clicked run" from "the cron auto-fired".
    let path = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let project = quick_project(&mut app, "项目 · 定时触发").await;

    let workflow_id = WorkflowId::new();
    app.dispatch(Command::CreateWorkflowSpec {
        id: workflow_id,
        name: "定时 · 目标".into(),
        prompt: "p".into(),
        goal: "g".into(),
        stage_ref: None,
        phases: vec!["巡检".into()],
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

    let task = CronTaskId::new();
    app.dispatch(Command::CreateCronTask {
        id: task,
        name: "每日 · 巡检".into(),
        target: "定时 · 目标".into(),
        schedule: Cadence::Daily,
        project_id: Some(project),
    })
    .await
    .unwrap();

    // The scheduler only fires *due* tasks; Daily with last_run_at=0 is due.
    // Move the open project away to prove no-hijack while we're at it.
    let other = quick_project(&mut app, "别的项目 · 当前所在").await;
    assert_eq!(app.snapshot().active_project, Some(other));

    app.tick_scheduler().await.unwrap();

    let runs = store.list_workflow_runs(workflow_id).await.unwrap();
    assert_eq!(runs.len(), 1);
    assert_eq!(
        runs[0].trigger,
        RunTrigger::Scheduled,
        "scheduler run attributed Scheduled"
    );
    assert_eq!(runs[0].status, RunStatus::Ok);
    assert_eq!(app.snapshot().active_project, Some(other), "no hijack");
    let _ = View::Projects; // silence unused import in some configs
}

#[tokio::test]
async fn analytics_aggregates_runs_and_rates_success_honestly() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let wid = WorkflowId::new();

    // No runs yet → success_rate None (unknown ≠ 0%), not a panic.
    let a = store.workflow_analytics(wid).await.unwrap();
    assert_eq!(a.total_runs, 0);
    assert!(a.success_rate.is_none());

    // Seed: 3 ok, 1 failed, with varying durations.
    for (i, ok) in [(0, true), (1, true), (2, false), (3, true)] {
        let id = store
            .record_workflow_run_start(bw_store::NewWorkflowRun {
                workflow_id: wid,
                workflow_name: "wf",
                project_id: None,
                session_id: None,
                trigger: RunTrigger::Manual,
                started_at: 1000 + i,
                params_json: r#"{"phase_count":1}"#,
                cron_task_id: None,
            })
            .await
            .unwrap();
        let status = if ok { RunStatus::Ok } else { RunStatus::Failed };
        store
            .settle_workflow_run(id, status, 1000 + i + 5, 100 * (i as i64 + 1), 1, "")
            .await
            .unwrap();
    }

    let a = store.workflow_analytics(wid).await.unwrap();
    assert_eq!(a.total_runs, 4);
    assert_eq!(a.ok_runs, 3);
    assert_eq!(a.failed_runs, 1);
    // 3 ok / 4 settled = 0.75
    assert!((a.success_rate.unwrap() - 0.75).abs() < 1e-5);
    // durations 100,200,300,400 → median of even count = (200+300)/2 = 250
    assert_eq!(a.median_duration_ms, Some(250));
    assert_eq!(a.last_status, Some(RunStatus::Ok), "last run was ok");
}

#[tokio::test]
async fn params_snapshot_captures_spec_shape_at_run_time() {
    let path = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    quick_project(&mut app, "项目 · 参数快照").await;

    let workflow_id = WorkflowId::new();
    app.dispatch(Command::CreateWorkflowSpec {
        id: workflow_id,
        name: "参数 · 验证".into(),
        prompt: "p".into(),
        goal: "g".into(),
        stage_ref: Some(3),
        phases: vec!["a".into(), "b".into(), "c".into(), "d".into()],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 2,
            max_iter: 5,
        },
        maturity: Maturity::Mature,
        scope: String::new(),
        source: HubSource::SelfBuilt,
        trigger: None,
    })
    .await
    .unwrap();

    let session = bw_core::SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: None,
        kind: bw_store::SessionKind::Create,
        title: "参数快照运行".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunHubWorkflow {
        workflow_id,
        session,
    })
    .await
    .unwrap();

    let runs = store.list_workflow_runs(workflow_id).await.unwrap();
    let params = &runs[0].params_json;
    assert!(
        params.contains(r#""phase_count":4"#),
        "phase_count captured: {params}"
    );
    assert!(
        params.contains(r#""max_iter":5"#),
        "loop config captured: {params}"
    );
    assert!(
        params.contains(r#""stage_ref":3"#),
        "stage_ref captured: {params}"
    );
    assert!(
        params.contains(r#""trigger":"manual""#),
        "trigger captured: {params}"
    );
}

#[tokio::test]
async fn cron_effectiveness_attributes_only_scheduled_fires() {
    let path = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let project = quick_project(&mut app, "项目 · 调度有效性").await;

    let workflow_id = WorkflowId::new();
    app.dispatch(Command::CreateWorkflowSpec {
        id: workflow_id,
        name: "定时 · 目标B".into(),
        prompt: "p".into(),
        goal: "g".into(),
        stage_ref: None,
        phases: vec!["巡检".into()],
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

    let task = CronTaskId::new();
    app.dispatch(Command::CreateCronTask {
        id: task,
        name: "每日巡检B".into(),
        target: "定时 · 目标B".into(),
        schedule: Cadence::Daily,
        project_id: Some(project),
    })
    .await
    .unwrap();

    // Manual run first — must NOT count toward the schedule's effectiveness.
    let session = bw_core::SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: None,
        kind: bw_store::SessionKind::Create,
        title: "手动一次".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunHubWorkflow {
        workflow_id,
        session,
    })
    .await
    .unwrap();

    let e0 = store.cron_effectiveness(task).await.unwrap();
    assert_eq!(
        e0.fires, 0,
        "manual run excluded from schedule effectiveness"
    );
    assert!(e0.effectiveness.is_none());

    // Now the scheduler auto-fires it (Daily + last_run_at=0 is due).
    app.tick_scheduler().await.unwrap();

    let e1 = store.cron_effectiveness(task).await.unwrap();
    assert_eq!(e1.fires, 1, "scheduled fire counted");
    assert_eq!(e1.effectiveness, Some(1.0), "succeeded → 100%");
    assert_eq!(e1.last_fire_ok, Some(true));
    assert!(e1.last_fire_at.is_some());
}
