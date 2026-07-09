//! bw-store level checks: append-only observation → recompute derives signals →
//! persistence survives a reopen, and the persisted cache matches an independent
//! `bw_core` derive (no fabrication). Plus the handoff/DoD audit trail
//! (体系重构 v2 `§07`③): append-only, never silently blocked.

use bw_core::derive::{evaluate_metric, measure, parse_target, reduce_worst_of, Measurement};
use bw_core::model::{
    stage_workflow, Cadence, HubSource, LibSource, LoopConfig, Maturity, ProjectPhase, SourceKind,
    StageKind, WorkflowKind,
};
use bw_core::{AgentId, MetricId, ProjectId, Signal, SkillId, WorkflowId};
use bw_store::{
    MetricRole, NewAgent, NewMetric, NewProject, NewSkill, NewStage, NewWorkflowSpec, SqliteStore,
    Store,
};
use time::OffsetDateTime;

fn tmp_db() -> String {
    let p = std::env::temp_dir().join(format!("bw_store_test_{}.db", uuid::Uuid::new_v4()));
    p.to_string_lossy().into_owned()
}

fn all_five(project: ProjectId) -> Vec<NewStage> {
    StageKind::ALL
        .into_iter()
        .map(|kind| NewStage {
            project_id: project,
            kind,
            schedule: Cadence::Weekly,
        })
        .collect()
}

#[tokio::test]
async fn recompute_derives_and_persists_across_reopen() {
    let path = tmp_db();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let project = ProjectId::new();
    let metric = MetricId::new();

    {
        let store = SqliteStore::open(&path).await.unwrap();
        store
            .create_project(NewProject {
                id: project,
                name: "增长看板".into(),
                kind: "看板 / 网页应用".into(),
                desc: String::new(),
            })
            .await
            .unwrap();
        store
            .upsert_metric(NewMetric {
                id: metric,
                project_id: project,
                role: MetricRole::Leading,
                stage_kind: Some(StageKind::Prototype),
                name: "每周有效对话数".into(),
                def: String::new(),
                target_raw: "≥5".into(),
                amber: Default::default(),
                last_target: String::new(),
                driver: String::new(),
                pos: 0,
            })
            .await
            .unwrap();
        // The value is born ONLY as an observation.
        store
            .append_observation(metric, SourceKind::Manual, "8", now)
            .await
            .unwrap();
        store.materialize_stages(all_five(project)).await.unwrap();
        store.recompute_signals(project, now).await.unwrap();
        // drop store → close db
    }

    // Reopen the same file: data + derived signals survive.
    let store = SqliteStore::open(&path).await.unwrap();
    let sigs = store.persisted_signals(project).await.unwrap();

    // independent derive of the same metric, to prove the cache wasn't fabricated
    let m = measure("8", now, SourceKind::Manual, &Cadence::Weekly, now);
    assert!(matches!(m, Measurement::Value(_)));
    let expect_metric = evaluate_metric(&m, &parse_target("≥5").unwrap(), &[]).signal();
    assert_eq!(expect_metric, Signal::Green);

    let leading = sigs
        .metrics
        .iter()
        .find(|x| x.name == "每周有效对话数")
        .unwrap();
    assert_eq!(leading.value_raw, "8");
    assert_eq!(leading.signal, Some(expect_metric)); // persisted == derived
    assert_eq!(leading.hit, Some(true));

    // L4: Prototype stage routine = worst-of [Green] = Green
    let proto_stage = sigs
        .stages
        .iter()
        .find(|s| s.kind == StageKind::Prototype)
        .unwrap();
    assert_eq!(proto_stage.routine, Some(Signal::Green));

    // L6: project = worst-of (Green + 4×Unknown) = Green (a green tolerates unknowns)
    let stage_signals: Vec<Signal> = sigs.stages.iter().map(|s| s.routine.unwrap()).collect();
    assert_eq!(
        sigs.project,
        Some(reduce_worst_of(stage_signals).into_inner())
    );
    assert_eq!(sigs.project, Some(Signal::Green));

    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.name, "增长看板");
    assert_eq!(proj.phase as u8, ProjectPhase::ColdStart as u8); // phase not advanced here
    assert_eq!(proj.active_stage, StageKind::Prototype); // default entry stage

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn missing_observation_is_unknown_not_green() {
    let path = tmp_db();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let project = ProjectId::new();
    let metric = MetricId::new();

    let store = SqliteStore::open(&path).await.unwrap();
    store
        .create_project(NewProject {
            id: project,
            name: "x".into(),
            kind: "y".into(),
            desc: String::new(),
        })
        .await
        .unwrap();
    store
        .upsert_metric(NewMetric {
            id: metric,
            project_id: project,
            role: MetricRole::Leading,
            stage_kind: Some(StageKind::Prototype),
            name: "无数据指标".into(),
            def: String::new(),
            target_raw: "≥5".into(),
            amber: Default::default(),
            last_target: String::new(),
            driver: String::new(),
            pos: 0,
        })
        .await
        .unwrap();
    // no observation appended
    store.materialize_stages(all_five(project)).await.unwrap();
    store.recompute_signals(project, now).await.unwrap();

    let sigs = store.persisted_signals(project).await.unwrap();
    let m = sigs
        .metrics
        .iter()
        .find(|x| x.name == "无数据指标")
        .unwrap();
    assert_eq!(m.signal, Some(Signal::Unknown)); // never green on no data
    assert_eq!(m.hit, Some(false));

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn dod_and_handoff_are_real_and_audited() {
    let path = tmp_db();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let project = ProjectId::new();

    let store = SqliteStore::open(&path).await.unwrap();
    store
        .create_project(NewProject {
            id: project,
            name: "增长看板".into(),
            kind: "看板 / 网页应用".into(),
            desc: String::new(),
        })
        .await
        .unwrap();
    store.materialize_stages(all_five(project)).await.unwrap();

    // Freshly materialized: every DoD box starts unchecked (no fabricated readiness).
    let stages = store.list_stages(project).await.unwrap();
    let proto = stages
        .iter()
        .find(|s| s.kind == StageKind::Prototype)
        .unwrap();
    assert_eq!(
        proto.dod,
        vec![false; StageKind::Prototype.dod_items().len()]
    );

    // Check exactly one box.
    store
        .toggle_dod(project, StageKind::Prototype, 0)
        .await
        .unwrap();
    let stages = store.list_stages(project).await.unwrap();
    let proto = stages
        .iter()
        .find(|s| s.kind == StageKind::Prototype)
        .unwrap();
    assert!(proto.dod[0]);
    assert!(!proto.dod[1..].iter().any(|&v| v));

    // Hand off with an incomplete checklist — allowed, but marked risky and
    // audited (never silently blocked; plan honesty rule extends to process).
    store
        .handoff_stage(
            project,
            StageKind::Prototype,
            StageKind::Build,
            true,
            "性能基线未测 · 带险交棒".into(),
            now,
        )
        .await
        .unwrap();

    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.active_stage, StageKind::Build); // derived from the log, not hand-set separately

    let log = store.list_handoffs(project).await.unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].from_stage, StageKind::Prototype);
    assert_eq!(log[0].to_stage, StageKind::Build);
    assert!(log[0].risky);
    assert!(log[0].note.contains("带险交棒"));

    // The reflux: Ops hands back to Prototype, closing the loop — same table,
    // no special-casing.
    store
        .handoff_stage(
            project,
            StageKind::Ops,
            StageKind::Prototype,
            false,
            "复盘洞察已回流".into(),
            now,
        )
        .await
        .unwrap();
    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.active_stage, StageKind::Prototype);
    assert_eq!(store.list_handoffs(project).await.unwrap().len(), 2);

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn workflow_spec_create_list_get_roundtrip() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let id = WorkflowId::new();

    store
        .create_workflow_spec(NewWorkflowSpec {
            id,
            name: "深度访谈 → 问题定义".into(),
            kind: WorkflowKind::Static {
                maturity: Maturity::Mature,
                version: 3,
                uses: 12,
                scope: "跨项目复用".into(),
                source: HubSource::SelfBuilt,
                trigger: Some("deep interview".into()),
            },
            prompt: "界定→采集→结构化→分析".into(),
            goal: "产出验证过的问题陈述".into(),
            stage_ref: Some(1),
            phases: vec!["访谈提纲".into(), "深挖场景".into()],
            agents: vec![],
            skills: vec![],
            loop_config: LoopConfig {
                retries: 2,
                max_iter: 3,
            },
        })
        .await
        .unwrap();

    let listed = store.list_workflow_specs().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);

    let got = store.get_workflow_spec(id).await.unwrap().unwrap();
    assert_eq!(got.name, "深度访谈 → 问题定义");
    assert_eq!(got.phases, vec!["访谈提纲", "深挖场景"]);
    assert_eq!(got.loop_config.retries, 2);
    assert_eq!(got.loop_config.max_iter, 3);
    match got.kind {
        WorkflowKind::Static {
            maturity,
            version,
            uses,
            source,
            trigger,
            ..
        } => {
            assert_eq!(maturity, Maturity::Mature);
            assert_eq!(version, 3);
            assert_eq!(uses, 12);
            assert_eq!(source, HubSource::SelfBuilt);
            assert_eq!(trigger.as_deref(), Some("deep interview"));
        }
        WorkflowKind::Dynamic { .. } => panic!("expected Static"),
    }
    assert!(store
        .get_workflow_spec(WorkflowId::new())
        .await
        .unwrap()
        .is_none());

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn promote_workflow_mints_fresh_static_row_from_a_dynamic_spec() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();

    // The exact shape bw-app's PromoteWorkflow reconstructs: a Dynamic spec
    // built the same way `RunWorkflow` builds one for a live session.
    let dynamic = stage_workflow(StageKind::Prototype);
    let new_id = WorkflowId::new();

    store
        .promote_workflow(new_id, &dynamic, HubSource::SelfBuilt)
        .await
        .unwrap();

    let promoted = store.get_workflow_spec(new_id).await.unwrap().unwrap();
    assert_eq!(promoted.name, dynamic.name);
    assert_eq!(promoted.prompt, dynamic.prompt);
    assert_eq!(promoted.goal, dynamic.goal);
    assert_eq!(promoted.phases, dynamic.phases);
    assert_eq!(promoted.stage_ref, dynamic.stage_ref);
    match promoted.kind {
        WorkflowKind::Static {
            maturity,
            version,
            uses,
            source,
            trigger,
            ..
        } => {
            assert_eq!(maturity, Maturity::Fresh);
            assert_eq!(version, 1);
            assert_eq!(uses, 0);
            assert_eq!(source, HubSource::SelfBuilt);
            assert_eq!(trigger, None);
        }
        WorkflowKind::Dynamic { .. } => panic!("promotion must produce a Static row"),
    }

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn record_workflow_use_increments_uses() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let id = WorkflowId::new();

    store
        .create_workflow_spec(NewWorkflowSpec {
            id,
            name: "北极星推导编排".into(),
            kind: WorkflowKind::Static {
                maturity: Maturity::Fresh,
                version: 1,
                uses: 0,
                scope: String::new(),
                source: HubSource::SelfBuilt,
                trigger: None,
            },
            prompt: "p".into(),
            goal: "g".into(),
            stage_ref: None,
            phases: vec![],
            agents: vec![],
            skills: vec![],
            loop_config: LoopConfig {
                retries: 1,
                max_iter: 1,
            },
        })
        .await
        .unwrap();

    store.record_workflow_use(id).await.unwrap();
    store.record_workflow_use(id).await.unwrap();

    let got = store.get_workflow_spec(id).await.unwrap().unwrap();
    match got.kind {
        WorkflowKind::Static { uses, .. } => assert_eq!(uses, 2),
        WorkflowKind::Dynamic { .. } => panic!("expected Static"),
    }

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn skill_create_list_get_roundtrip() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let id = SkillId::new();

    store
        .create_skill(NewSkill {
            id,
            name: "web-scan".into(),
            maturity: Maturity::Polishing,
            desc: "扫描公开网页并结构化提取".into(),
            category: "检索".into(),
            source: LibSource::SelfBuilt,
        })
        .await
        .unwrap();

    let listed = store.list_skills().await.unwrap();
    assert_eq!(listed.len(), 1);

    let got = store.get_skill(id).await.unwrap().unwrap();
    assert_eq!(got.name, "web-scan");
    assert_eq!(got.maturity, Maturity::Polishing);
    assert_eq!(got.category, "检索");
    assert_eq!(got.source, LibSource::SelfBuilt);
    assert_eq!(got.uses, 0, "a freshly created skill starts unused");
    assert!(store.get_skill(SkillId::new()).await.unwrap().is_none());

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn agent_create_list_get_roundtrip() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let id = AgentId::new();

    store
        .create_agent(NewAgent {
            id,
            name: "竞品分析 Agent".into(),
            role: "强检索、低臆测，所有结论附公开来源引用".into(),
            maturity: Maturity::Polishing,
            skills: vec!["web-scan".into(), "对比矩阵".into()],
            model: "claude-opus".into(),
        })
        .await
        .unwrap();

    let listed = store.list_agents().await.unwrap();
    assert_eq!(listed.len(), 1);

    let got = store.get_agent(id).await.unwrap().unwrap();
    assert_eq!(got.name, "竞品分析 Agent");
    assert_eq!(
        got.skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["web-scan", "对比矩阵"]
    );
    assert_eq!(got.model, "claude-opus");
    assert_eq!(got.runs, 0, "a freshly created agent starts unused");
    assert!(store.get_agent(AgentId::new()).await.unwrap().is_none());

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn delete_project_removes_everything_scoped_to_it() {
    let path = tmp_db();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let project = ProjectId::new();
    let metric = MetricId::new();

    let store = SqliteStore::open(&path).await.unwrap();
    store
        .create_project(NewProject {
            id: project,
            name: "待删除".into(),
            kind: "y".into(),
            desc: String::new(),
        })
        .await
        .unwrap();
    store
        .upsert_metric(NewMetric {
            id: metric,
            project_id: project,
            role: MetricRole::Leading,
            stage_kind: Some(StageKind::Prototype),
            name: "m".into(),
            def: String::new(),
            target_raw: "≥5".into(),
            amber: Default::default(),
            last_target: String::new(),
            driver: String::new(),
            pos: 0,
        })
        .await
        .unwrap();
    store
        .append_observation(metric, SourceKind::Manual, "8", now)
        .await
        .unwrap();
    store.materialize_stages(all_five(project)).await.unwrap();
    let session = bw_core::SessionId::new();
    store
        .ensure_session(bw_store::NewSession {
            id: session,
            project_id: project,
            stage_kind: Some(StageKind::Prototype),
            kind: bw_store::SessionKind::Create,
            title: "s".into(),
            snippet: String::new(),
        })
        .await
        .unwrap();
    store
        .append_message(session, bw_core::model::Role::Builder, "hi")
        .await
        .unwrap();
    store
        .handoff_stage(
            project,
            StageKind::Prototype,
            StageKind::Build,
            false,
            "n",
            now,
        )
        .await
        .unwrap();

    store.delete_project(project).await.unwrap();

    assert!(store.get_project(project).await.unwrap().is_none());
    assert!(store.list_observations(project).await.unwrap().is_empty());
    assert!(store.list_stages(project).await.unwrap().is_empty());
    assert!(store.list_sessions(project).await.unwrap().is_empty());
    assert!(store.session_messages(session).await.unwrap().is_empty());
    assert!(store.list_handoffs(project).await.unwrap().is_empty());
    assert!(!store
        .list_projects()
        .await
        .unwrap()
        .iter()
        .any(|p| p.id == project));

    let _ = std::fs::remove_file(&path);
}
