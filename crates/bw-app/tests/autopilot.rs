//! A1 — autopilot cron (`create_issue` mode): a due task mints a stage-scoped
//! Todo Issue (optionally assigned by name), no-hijack (it never runs a
//! workflow). Idempotent (not-due = no re-create); an unknown assignee is an
//! honest unassigned Issue, not a failure; `run_workflow` tasks never mint.

use bw_app::{App, Command};
use bw_core::model::{Cadence, CronStatus, IssueStatus, ProjectCycle, StageKind};
use bw_core::{AgentId, CronTaskId, ProjectId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

const AGENT_NAME: &str = "构建师 · auto";

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_autopilot_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

async fn running_project(app: &mut App, project: ProjectId, agent: AgentId) {
    app.dispatch(Command::CreateProject {
        id: project,
        name: "A1 Autopilot 项目".into(),
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
    app.dispatch(Command::CreateAgent {
        id: agent,
        name: AGENT_NAME.into(),
        role: "构建".into(),
        skills: vec![],
        model: "sonnet".into(),
        instructions: "构建师:接 Issue 后写真实代码与测试。".into(),
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn autopilot_due_task_mints_todo_issue_assigned_then_idempotent() {
    let path = tmp_db();
    let project = ProjectId::new();
    let agent = AgentId::new();
    let task = CronTaskId::new();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    running_project(&mut app, project, agent).await;

    app.dispatch(Command::CreateAutopilotTask {
        id: task,
        name: "晨间建单".into(),
        schedule: Cadence::Daily,
        project_id: Some(project),
        stage: StageKind::Build,
        assignee: Some(AGENT_NAME.into()),
    })
    .await
    .unwrap();

    // First tick: a never-run Daily task is due → mints the Issue (Todo).
    let fired = app.tick_scheduler().await.unwrap();
    assert_eq!(fired.len(), 1, "the autopilot task fired");
    let issues = store
        .list_issues(project, Some(StageKind::Build), None)
        .await
        .unwrap();
    assert_eq!(issues.len(), 1);
    let i = &issues[0];
    assert_eq!(
        i.status,
        IssueStatus::Todo,
        "autopilot建单 = committed work (Todo), not the Backlog parking lot"
    );
    assert_eq!(i.stage, StageKind::Build);
    assert!(
        i.title.contains("[auto]"),
        "title is prefixed [auto]: {}",
        i.title
    );
    assert_eq!(i.assignee, Some(agent), "assigned to the named agent");

    let t = store
        .list_cron_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.id == task)
        .unwrap();
    assert_eq!(t.status, CronStatus::Normal);
    assert!(t.last_run_at.is_some(), "cron last_run recorded");

    // Idempotent: an immediate second tick (Daily, just ran) is NOT due.
    let fired2 = app.tick_scheduler().await.unwrap();
    assert!(fired2.is_empty(), "not due again — no duplicate fire");
    let issues2 = store
        .list_issues(project, Some(StageKind::Build), None)
        .await
        .unwrap();
    assert_eq!(issues2.len(), 1, "still exactly one issue");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn autopilot_unknown_assignee_creates_unassigned_not_failure() {
    let path = tmp_db();
    let project = ProjectId::new();
    let agent = AgentId::new();
    let task = CronTaskId::new();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    running_project(&mut app, project, agent).await;

    app.dispatch(Command::CreateAutopilotTask {
        id: task,
        name: "夜巡".into(),
        schedule: Cadence::Daily,
        project_id: Some(project),
        stage: StageKind::Prototype,
        assignee: Some("不存在的幽灵 agent".into()),
    })
    .await
    .unwrap();

    let fired = app.tick_scheduler().await.unwrap();
    assert_eq!(
        fired.len(),
        1,
        "a 0-match assignee is NOT a failure — the task still fired"
    );
    let issues = store
        .list_issues(project, Some(StageKind::Prototype), None)
        .await
        .unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].assignee, None,
        "unassigned — the named agent doesn't exist"
    );
    let t = store
        .list_cron_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.id == task)
        .unwrap();
    assert_eq!(
        t.status,
        CronStatus::Normal,
        "not Failed — 0-match is honest"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn run_workflow_mode_never_mints_issues() {
    // A default (run_workflow) task doesn't create Issues even when its target
    // names no spec (it's simply skipped) — the mode dispatch is airtight.
    let path = tmp_db();
    let project = ProjectId::new();
    let agent = AgentId::new();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    running_project(&mut app, project, agent).await;

    app.dispatch(Command::CreateCronTask {
        id: CronTaskId::new(),
        name: "不存在的流程".into(),
        target: "no-such-workflow".into(),
        schedule: Cadence::Daily,
        project_id: Some(project),
    })
    .await
    .unwrap();

    let fired = app.tick_scheduler().await.unwrap();
    assert!(fired.is_empty(), "no spec match → skipped");
    let issues = store.list_issues(project, None, None).await.unwrap();
    assert!(
        issues.is_empty(),
        "run_workflow mode never mints Issues (no-hijack holds)"
    );

    let _ = std::fs::remove_file(&path);
}
