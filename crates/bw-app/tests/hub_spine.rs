//! **Hub exit gate.** Headless, no UI:
//!
//!   CreateSkill / CreateAgent → land in the global hub state (not project-
//!   scoped) → CreateProject → creation flow → CompleteCreation →
//!   StartSession → RunWorkflow(Dynamic, same shape as the real stage
//!   template) → PromoteWorkflow (mints a Static hub row) →
//!   RunHubWorkflow (looks the row up, runs it, bumps `uses`)
//!
//! asserting at each step that the hub library really is global (visible with
//! no active project involved in its own CRUD) and that `uses` only ever
//! moves because a hub workflow was actually run — never fabricated.

use bw_app::{App, Command, View};
use bw_core::model::{stage_workflow, Cadence, HubSource, LibSource, ProjectCycle, StageKind};
use bw_core::{MetricId, ProjectId, SessionId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{MetricRole, SessionKind, SqliteStore, Store};
use std::sync::Arc;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_hub_spine_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn hub_library_is_global_and_uses_only_moves_on_a_real_run() {
    let path = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );

    // ── skill/agent CRUD is global — no project needs to exist yet ──
    let skill_id = bw_core::SkillId::new();
    app.dispatch(Command::CreateSkill {
        id: skill_id,
        name: "web-scan".into(),
        desc: "扫描公开网页并结构化提取".into(),
        category: "检索".into(),
        source: LibSource::SelfBuilt,
        content: String::new(),
    })
    .await
    .unwrap();
    assert_eq!(app.snapshot().skills.len(), 1);
    assert_eq!(app.snapshot().skills[0].name, "web-scan");

    let agent_id = bw_core::AgentId::new();
    app.dispatch(Command::CreateAgent {
        id: agent_id,
        name: "竞品分析 Agent".into(),
        role: "强检索、低臆测".into(),
        skills: vec!["web-scan".into()],
        model: "claude-opus".into(),
        instructions: String::new(),
    })
    .await
    .unwrap();
    assert_eq!(app.snapshot().agents.len(), 1);
    assert_eq!(app.snapshot().agents[0].name, "竞品分析 Agent");

    // Empty name is rejected, same rule as every other create command.
    assert!(app
        .dispatch(Command::CreateSkill {
            id: bw_core::SkillId::new(),
            name: "  ".into(),
            desc: String::new(),
            category: String::new(),
            source: LibSource::SelfBuilt,
            content: String::new(),
        })
        .await
        .is_err());

    // ── minimal creation flow to get one project into Running ──
    let project = ProjectId::new();
    let session = SessionId::new();
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
    app.dispatch(Command::UpsertManualMetric {
        id: MetricId::new(),
        name: "周复用次数".into(),
        def: String::new(),
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

    // ── run the stage's standard (Dynamic) workflow, exactly as the real
    //    "▶ 运行" button does ──
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
        spec: stage_workflow(StageKind::Prototype),
    })
    .await
    .unwrap();

    // ── promote it: no workflow_spec row exists yet (Dynamic specs are never
    //    persisted) — this mints the first one ──
    assert!(app.snapshot().workflow_specs.is_empty());
    let promoted_id = bw_core::WorkflowId::new();
    app.dispatch(Command::PromoteWorkflow {
        new_id: promoted_id,
        session,
        source: HubSource::SelfBuilt,
    })
    .await
    .unwrap();

    let specs = &app.snapshot().workflow_specs;
    assert_eq!(specs.len(), 1);
    let promoted = &specs[0];
    assert_eq!(promoted.id, promoted_id);
    assert_eq!(promoted.name, stage_workflow(StageKind::Prototype).name);
    match &promoted.kind {
        bw_core::model::WorkflowKind::Static {
            maturity,
            version,
            uses,
            source,
            trigger,
            ..
        } => {
            assert_eq!(*maturity, bw_core::model::Maturity::Fresh);
            assert_eq!(*version, 1);
            assert_eq!(*uses, 0, "promotion never fabricates a use");
            assert_eq!(*source, HubSource::SelfBuilt);
            assert_eq!(*trigger, None);
        }
        bw_core::model::WorkflowKind::Dynamic { .. } => panic!("expected Static"),
    }

    // ── run it via the hub path: uses must move from 0 → 1, and only because
    //    a real run happened, not as a side effect of promotion or listing ──
    app.dispatch(Command::RunHubWorkflow {
        session,
        workflow_id: promoted_id,
    })
    .await
    .unwrap();

    let specs = &app.snapshot().workflow_specs;
    let ran = specs.iter().find(|s| s.id == promoted_id).unwrap();
    match &ran.kind {
        bw_core::model::WorkflowKind::Static { uses, .. } => {
            assert_eq!(*uses, 1, "RunHubWorkflow must bump uses exactly once")
        }
        bw_core::model::WorkflowKind::Dynamic { .. } => panic!("expected Static"),
    }

    // The run itself produced real session messages, same as a plain RunWorkflow.
    let msgs = store.session_messages(session).await.unwrap();
    assert!(!msgs.is_empty());

    let _ = std::fs::remove_file(&path);
}
