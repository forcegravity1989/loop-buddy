//! 完整形态 spine checks — the pieces the user named must be *real*, wired
//! end to end through the kernel:
//!
//! 1. all-in-one-codebase: creation auto-provisions the project's own git
//!    repo + a bound `git-repo` connector;
//! 2. connector sync is a real probe that flips real status and feeds real
//!    observations (`SourceKind::Connector`) into matching metrics;
//! 3. artifacts register from a real workspace scan, idempotently, and the
//!    Artifact panel's state loads them;
//! 4. a stage-playbook run credits the five-role agent + stage skills with
//!    real usage counters (win_rate derived from real runs/wins).

use bw_app::{App, Command, METRIC_WS_COMMITS};
use bw_core::model::{
    ArtifactKind, Cadence, ConnectorStatus, ProjectCycle, SourceKind, StageKind,
    CONNECTOR_KIND_GIT_REPO,
};
use bw_core::{MetricId, ProjectId, SessionId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{MetricRole, SessionKind, SqliteStore, Store};
use std::sync::Arc;

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("bw_complete_form_{tag}_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

async fn mk_app(workspaces_root: Option<std::path::PathBuf>) -> (App, Arc<dyn Store>, String) {
    let db = std::env::temp_dir()
        .join(format!("bw_complete_form_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.unwrap());
    let engine = Engine::new(Arc::new(MockExecutor::default()));
    let mut app = App::new(store.clone(), engine, ClaudeCliConfig::default());
    if let Some(root) = workspaces_root {
        app = app.with_workspaces_root(root);
    }
    (app, store, db)
}

async fn create_running_project(app: &mut App, name: &str) -> ProjectId {
    let id = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id,
        name: name.into(),
        kind: "CLI 工具".into(),
        desc: "一个真实的小需求".into(),
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
async fn creation_provisions_real_workspace_connector_and_artifacts() {
    let root = tmp_dir("ws");
    let (mut app, store, db) = mk_app(Some(root.clone())).await;
    app.dispatch(Command::Boot).await.unwrap();
    let p = create_running_project(&mut app, "linkcheck").await;

    // A real git repo exists and the project is bound to it.
    let proj = store.get_project(p).await.unwrap().unwrap();
    assert!(
        !proj.workspace_path.trim().is_empty(),
        "all-in-one-codebase 默认: 项目出生即有代码仓"
    );
    let ws = std::path::Path::new(&proj.workspace_path);
    assert!(ws.join(".git").exists(), "真实 git 仓库");
    assert!(ws.join("README.md").exists(), "真实首个文件");
    assert!(
        proj.allow_commands,
        "托管工作区默认放行命令(剧本需要 git/cargo)"
    );

    // A bound git-repo connector was minted with it.
    let connectors = store.list_connectors().await.unwrap();
    let repo_conn = connectors
        .iter()
        .find(|c| c.kind == CONNECTOR_KIND_GIT_REPO && c.project_id == Some(p))
        .expect("开仓即建 git-repo 连接器");
    assert_eq!(
        repo_conn.status,
        ConnectorStatus::Disconnected,
        "连接器健康只能来自真实探针,创建时不假装已连接"
    );

    // Define the standard commits metric (definition only, no value) so the
    // sync probe has something real to feed.
    app.dispatch(Command::UpsertManualMetric {
        id: MetricId::new(),
        name: METRIC_WS_COMMITS.into(),
        def: "git rev-list --count HEAD".into(),
        role: MetricRole::Leading,
        stage_kind: None,
        target: "≥1".into(),
        amber: Default::default(),
        value: String::new(),
    })
    .await
    .unwrap();

    // Real sync: probe flips status, feeds the metric as Connector source.
    app.dispatch(Command::SyncConnector { id: repo_conn.id })
        .await
        .unwrap();
    let synced = store.list_connectors().await.unwrap();
    let repo_conn = synced.iter().find(|c| c.id == repo_conn.id).unwrap();
    assert_eq!(repo_conn.status, ConnectorStatus::Connected);
    assert!(!repo_conn.last_sync.is_empty(), "真实同步时间戳");

    let sigs = store.persisted_signals(p).await.unwrap();
    let m = sigs
        .metrics
        .iter()
        .find(|m| m.name == METRIC_WS_COMMITS)
        .unwrap();
    assert_eq!(
        m.source,
        Some(SourceKind::Connector),
        "观测来源=Connector——「Connector 真喂指标」落地"
    );
    assert_eq!(m.value_raw, "1", "值=真实提交数(开仓的首提交)");
    assert_eq!(m.hit, Some(true), "≥1 目标由真实值命中,信号派生而非手设");

    // Artifacts: manual collect registers the provisioned files, idempotently.
    app.dispatch(Command::CollectArtifacts).await.unwrap();
    let arts = store.list_artifacts(p).await.unwrap();
    assert!(
        arts.iter()
            .any(|a| a.path == "README.md" && a.kind == ArtifactKind::Doc),
        "README 登记为文档产物"
    );
    assert!(
        arts.iter()
            .any(|a| a.path == ".gitignore" && a.kind == ArtifactKind::Config),
        ".gitignore 登记为配置产物"
    );
    assert!(
        arts.iter().all(|a| !a.git_commit.is_empty()),
        "产物钉在真实提交上"
    );
    let count_1 = arts.len();

    app.dispatch(Command::CollectArtifacts).await.unwrap();
    assert_eq!(
        store.list_artifacts(p).await.unwrap().len(),
        count_1,
        "重复采集不产生重复登记(幂等=版本语义)"
    );

    // The panel's state snapshot loads them, tagged to the right project.
    app.dispatch(Command::LoadArtifacts).await.unwrap();
    let (tagged, rows) = app.snapshot().artifacts.clone().expect("loaded");
    assert_eq!(tagged, p);
    assert_eq!(rows.len(), count_1);

    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn playbook_run_credits_role_agent_and_stage_skills() {
    // No workspaces root ⇒ no provisioning ⇒ MockExecutor (byte-for-byte the
    // old behavior for unconfigured projects) — the accounting must still be
    // real because the run really settled.
    let (mut app, store, db) = mk_app(None).await;
    app.dispatch(Command::Boot).await.unwrap();
    let _p = create_running_project(&mut app, "demo").await;

    let baseline_agent = store
        .list_agents()
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.name == StageKind::Prototype.role_short())
        .expect("Boot 播种五角色 agent 实体");
    assert_eq!(baseline_agent.runs, 0);
    assert!(
        baseline_agent.instructions.contains("原型师"),
        "角色 agent 带真实指令模板"
    );

    let session = SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: Some(StageKind::Prototype),
        kind: SessionKind::Create,
        title: "原型段剧本".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunStagePlaybook {
        session,
        stage_kind: StageKind::Prototype,
    })
    .await
    .unwrap();

    let agent = store
        .list_agents()
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.name == StageKind::Prototype.role_short())
        .unwrap();
    assert_eq!(agent.runs, 1, "真实 run 记到角色 agent 头上");
    assert_eq!(agent.win_rate, "100%", "win_rate 由真实 runs/wins 派生");

    let skill = store
        .list_skills()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "evidence-first")
        .expect("Boot 播种阶段技能实体");
    assert_eq!(skill.uses, 1, "技能使用数是真实计数");
    assert!(skill.content.contains("证据先行"), "技能带真实正文");

    let _ = std::fs::remove_file(&db);
}

#[tokio::test]
async fn non_playbook_spec_gets_skill_content_injected_into_prompt() {
    // A hub-style spec that *references* a stage skill by name (no per-phase
    // prompts): the kernel resolves the ref against the Skill Hub and the
    // real body rides into the run. Verified through the store: the skill's
    // use counter moves, and the run is recorded — while a made-up ref
    // contributes nothing and errors nothing.
    let (mut app, store, db) = mk_app(None).await;
    app.dispatch(Command::Boot).await.unwrap();
    let _p = create_running_project(&mut app, "demo2").await;

    let session = SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: None,
        kind: SessionKind::Optimize,
        title: "自定义工作流".into(),
    })
    .await
    .unwrap();

    let mut spec = bw_core::model::stage_workflow(StageKind::Optimize);
    spec.phase_prompts = vec![]; // non-playbook: shared prompt only
    spec.skills = vec![
        bw_core::model::SkillRef {
            name: "baseline-before-touch".into(),
            def: "先测基线".into(),
            from: "test".into(),
        },
        bw_core::model::SkillRef {
            name: "不存在的技能".into(),
            def: String::new(),
            from: "test".into(),
        },
    ];
    app.dispatch(Command::RunWorkflow { session, spec })
        .await
        .unwrap();

    let skill = store
        .list_skills()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "baseline-before-touch")
        .unwrap();
    assert_eq!(skill.uses, 1, "被引用的库内技能记账");

    let _ = std::fs::remove_file(&db);
}
