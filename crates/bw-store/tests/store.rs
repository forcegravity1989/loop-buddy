//! bw-store level checks: append-only observation → recompute derives signals →
//! persistence survives a reopen, and the persisted cache matches an independent
//! `bw_core` derive (no fabrication). Plus the handoff/DoD audit trail
//! (体系重构 v2 `§07`③): append-only, never silently blocked.

use bw_core::derive::{evaluate_metric, measure, parse_target, reduce_worst_of, Measurement};
use bw_core::model::{
    stage_workflow, Cadence, CronStatus, HubSource, LibSource, LoopConfig, Maturity, ProjectPhase,
    SourceKind, StageKind, WorkflowKind,
};
use bw_core::{AgentId, CronTaskId, MetricId, ProjectId, Signal, SkillId, WorkflowId};
use bw_store::{
    AgentEdit, MetricRole, NewAgent, NewCronTask, NewMetric, NewProject, NewSkill, NewStage,
    NewWorkflowSpec, SkillEdit, SqliteStore, Store,
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
            "性能基线未测 · 带险交棒",
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
            "复盘洞察已回流",
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
            phase_prompts: vec![],
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
            phase_prompts: vec![],
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
            content: "### 扫描方法\n1. 只记录真实抓到的页面。".into(),
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

    // A real edit (AgentHub/SkillHub's detail-panel "编辑 →") changes content
    // but must never touch lifecycle fields (maturity/uses) — same rule
    // `update_workflow_spec` established for workflows.
    store
        .update_skill(
            id,
            SkillEdit {
                name: "web-scan-v2".into(),
                desc: "扫描公开网页并结构化提取,新增去重".into(),
                category: "检索/数据".into(),
                content: "### 扫描方法 v2\n1. 只记录真实抓到的页面。\n2. 去重。".into(),
            },
        )
        .await
        .unwrap();
    let edited = store.get_skill(id).await.unwrap().unwrap();
    assert_eq!(edited.name, "web-scan-v2");
    assert_eq!(edited.desc, "扫描公开网页并结构化提取,新增去重");
    assert_eq!(edited.category, "检索/数据");
    assert_eq!(
        edited.maturity,
        Maturity::Polishing,
        "editing content must not touch lifecycle fields"
    );
    assert_eq!(edited.uses, 0, "editing content must not fabricate usage");

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
            instructions: "你是竞品分析师;结论必须附来源。".into(),
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

    // Same rule as skills: a real edit changes content, never lifecycle data.
    store
        .update_agent(
            id,
            AgentEdit {
                name: "竞品分析 Agent v2".into(),
                role: "强检索、低臆测，新增中文来源优先".into(),
                skills: vec!["web-scan".into()],
                model: "claude-sonnet".into(),
                instructions: "你是竞品分析师;中文来源优先,结论必须附来源。".into(),
            },
        )
        .await
        .unwrap();
    let edited = store.get_agent(id).await.unwrap().unwrap();
    assert_eq!(edited.name, "竞品分析 Agent v2");
    assert_eq!(edited.role, "强检索、低臆测，新增中文来源优先");
    assert_eq!(
        edited
            .skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        vec!["web-scan"]
    );
    assert_eq!(edited.model, "claude-sonnet");
    assert_eq!(
        edited.maturity,
        Maturity::Polishing,
        "editing content must not touch lifecycle fields"
    );
    assert_eq!(edited.runs, 0, "editing content must not fabricate usage");

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

#[tokio::test]
async fn list_recent_handoffs_joins_project_name_newest_first_and_respects_limit() {
    let path = tmp_db();
    let t0 = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let a = ProjectId::new();
    let b = ProjectId::new();

    let store = SqliteStore::open(&path).await.unwrap();
    store
        .create_project(NewProject {
            id: a,
            name: "项目 A".into(),
            kind: "y".into(),
            desc: String::new(),
        })
        .await
        .unwrap();
    store
        .create_project(NewProject {
            id: b,
            name: "项目 B".into(),
            kind: "y".into(),
            desc: String::new(),
        })
        .await
        .unwrap();
    store.materialize_stages(all_five(a)).await.unwrap();
    store.materialize_stages(all_five(b)).await.unwrap();

    // A hands off first (t0), then B (t0+1, risky), then A again (t0+2 —
    // the newest of all three, and the second time A appears).
    store
        .handoff_stage(a, StageKind::Prototype, StageKind::Build, false, "A1", t0)
        .await
        .unwrap();
    store
        .handoff_stage(
            b,
            StageKind::Prototype,
            StageKind::Build,
            true,
            "B1 险",
            t0 + time::Duration::seconds(1),
        )
        .await
        .unwrap();
    store
        .handoff_stage(
            a,
            StageKind::Build,
            StageKind::Optimize,
            false,
            "A2",
            t0 + time::Duration::seconds(2),
        )
        .await
        .unwrap();

    let all = store.list_recent_handoffs(10).await.unwrap();
    assert_eq!(all.len(), 3);
    // Newest first, across both projects — the join resolves the real name,
    // not just the id.
    assert_eq!(all[0].project_name, "项目 A");
    assert_eq!(all[0].note, "A2");
    assert_eq!(all[0].from_stage, StageKind::Build);
    assert_eq!(all[0].to_stage, StageKind::Optimize);
    assert!(!all[0].risky);
    assert_eq!(all[1].project_name, "项目 B");
    assert_eq!(all[1].note, "B1 险");
    assert!(all[1].risky);
    assert_eq!(all[2].project_name, "项目 A");
    assert_eq!(all[2].note, "A1");

    let capped = store.list_recent_handoffs(2).await.unwrap();
    assert_eq!(capped.len(), 2, "limit is respected");
    assert_eq!(capped[0].note, "A2");
    assert_eq!(capped[1].note, "B1 险");

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn cron_task_last_run_at_roundtrips_real_clock_for_scheduler() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let id = CronTaskId::new();

    store
        .create_cron_task(NewCronTask {
            id,
            name: "验证 · 真实定时".into(),
            target: "wf".into(),
            schedule: Cadence::Daily,
            project_id: None,
        })
        .await
        .unwrap();
    let fresh = store
        .list_cron_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    assert_eq!(
        fresh.last_run_at, None,
        "never-run task must read back as real None, not a fabricated epoch"
    );

    store
        .record_cron_run(id, CronStatus::Normal, "2026-07-10 12:00".into())
        .await
        .unwrap();
    let ran = store
        .list_cron_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap();
    let last_run_at = ran
        .last_run_at
        .expect("record_cron_run must set a real clock");
    let now = OffsetDateTime::now_utc();
    assert!(
        (now - last_run_at).whole_seconds().abs() < 10,
        "last_run_at should be the real current time, not a stale/fabricated value"
    );

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn opening_a_db_from_before_last_run_at_existed_migrates_without_crashing() {
    // Reproduces, on purpose, the exact class of bug that already crashed
    // this app once (archive/workbench-pre-5stage-migration.db): a real
    // on-disk DB whose `cron_task` table predates a new column. Build one by
    // hand (mirrors the table shape `da6e437` originally shipped, minus
    // `last_run_at`), then confirm `SqliteStore::open` migrates it in place
    // instead of the new column simply not existing.
    let path = tmp_db();
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE cron_task (
                id TEXT PRIMARY KEY, name TEXT NOT NULL, target TEXT NOT NULL DEFAULT '',
                schedule TEXT NOT NULL DEFAULT 'weekly', project_id TEXT,
                status TEXT NOT NULL DEFAULT 'normal', last_run TEXT NOT NULL DEFAULT '',
                next_run TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL, rev INTEGER NOT NULL DEFAULT 0
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO cron_task (id, name, created_at, updated_at) VALUES (?, '老任务', 0, 0)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    // The real regression: before the migration guard, this second `open()`
    // against the pre-existing file either errors (column missing on
    // INSERT/SELECT) or silently can't see the new column. Neither is
    // acceptable for a real user's real local DB.
    let store = SqliteStore::open(&path)
        .await
        .expect("open must migrate the old table, not fail against it");
    let tasks = store.list_cron_tasks().await.unwrap();
    assert_eq!(
        tasks.len(),
        1,
        "pre-existing row must survive the migration"
    );
    assert_eq!(tasks[0].name, "老任务");
    assert_eq!(
        tasks[0].last_run_at, None,
        "migrated column defaults to 0 → None, not a fabricated timestamp"
    );

    // And the newly-migrated column is really writable, not just readable.
    store
        .record_cron_run(tasks[0].id, CronStatus::Normal, "刚刚".into())
        .await
        .unwrap();
    let after = store.list_cron_tasks().await.unwrap();
    assert!(after[0].last_run_at.is_some());

    let _ = std::fs::remove_file(&path);
}

// ═══════════════════ 完整形态: artifact / accounting / connector sync ═══════════════════

#[tokio::test]
async fn artifact_registration_is_idempotent_and_versions_by_commit() {
    use bw_core::model::{classify_artifact_path, ArtifactKind};
    use bw_core::ArtifactId;
    use bw_store::NewArtifact;

    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let project = ProjectId::new();
    store
        .create_project(bw_store::NewProject {
            id: project,
            name: "p".into(),
            kind: "k".into(),
            desc: String::new(),
        })
        .await
        .unwrap();

    let mk = |path: &str, commit: &str| NewArtifact {
        id: ArtifactId::new(),
        project_id: project,
        workflow_run_id: None,
        stage_kind: Some(StageKind::Prototype),
        path: path.into(),
        kind: classify_artifact_path(path),
        bytes: 120,
        git_commit: commit.into(),
        registered_at: 1_700_000_000,
    };

    // First scan at commit aaa111: two files, both fresh.
    let fresh = store
        .register_artifacts(vec![
            mk("docs/evidence.md", "aaa111"),
            mk("src/main.rs", "aaa111"),
        ])
        .await
        .unwrap();
    assert_eq!(fresh, 2);

    // Re-scan of the unchanged workspace: same identities ⇒ zero new rows.
    let again = store
        .register_artifacts(vec![
            mk("docs/evidence.md", "aaa111"),
            mk("src/main.rs", "aaa111"),
        ])
        .await
        .unwrap();
    assert_eq!(again, 0, "re-scan must not duplicate");

    // A new commit revising one file: exactly one new version row.
    let v2 = store
        .register_artifacts(vec![mk("docs/evidence.md", "bbb222")])
        .await
        .unwrap();
    assert_eq!(v2, 1);

    let all = store.list_artifacts(project).await.unwrap();
    assert_eq!(all.len(), 3);
    let evidence_versions: Vec<_> = all
        .iter()
        .filter(|a| a.path == "docs/evidence.md")
        .collect();
    assert_eq!(
        evidence_versions.len(),
        2,
        "rows sharing a path are that artifact's version history"
    );
    assert!(all.iter().any(|a| a.kind == ArtifactKind::Doc));
    assert!(all.iter().any(|a| a.kind == ArtifactKind::Code));
    assert_eq!(all[0].stage_kind, Some(StageKind::Prototype));

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn skill_and_agent_accounting_updates_real_counters() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();

    store
        .create_skill(NewSkill {
            id: SkillId::new(),
            name: "evidence-first".into(),
            maturity: Maturity::Mature,
            desc: "d".into(),
            category: "原型".into(),
            source: LibSource::Official,
            content: "### 证据先行\n1. 只写站得住的。".into(),
        })
        .await
        .unwrap();
    store
        .create_agent(NewAgent {
            id: AgentId::new(),
            name: "原型师".into(),
            role: "假设驱动探索".into(),
            maturity: Maturity::Mature,
            skills: vec!["evidence-first".into()],
            model: "claude CLI".into(),
            instructions: "你是原型师。".into(),
        })
        .await
        .unwrap();

    // Unregistered names are an honest no-op (0 rows), never an error.
    assert_eq!(store.record_skill_use_by_name("不存在").await.unwrap(), 0);
    assert_eq!(
        store
            .record_agent_run_by_name("不存在", true)
            .await
            .unwrap(),
        0
    );

    assert_eq!(
        store
            .record_skill_use_by_name("evidence-first")
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .record_skill_use_by_name("evidence-first")
            .await
            .unwrap(),
        1
    );
    let skill = &store.list_skills().await.unwrap()[0];
    assert_eq!(skill.uses, 2, "uses is a real counter now");
    assert!(skill.content.contains("证据先行"));

    // 2 ok + 1 failed ⇒ runs=3, win_rate=66% — derived from real counters.
    store
        .record_agent_run_by_name("原型师", true)
        .await
        .unwrap();
    store
        .record_agent_run_by_name("原型师", true)
        .await
        .unwrap();
    store
        .record_agent_run_by_name("原型师", false)
        .await
        .unwrap();
    let agent = &store.list_agents().await.unwrap()[0];
    assert_eq!(agent.runs, 3);
    assert_eq!(agent.win_rate, "66%");
    assert!(agent.instructions.contains("原型师"));

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn connector_sync_is_probe_written_and_survives_reopen() {
    use bw_core::model::{ConnectorStatus, CONNECTOR_KIND_GIT_REPO};
    use bw_core::ConnectorId;
    use bw_store::NewConnector;

    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let project = ProjectId::new();
    store
        .create_project(bw_store::NewProject {
            id: project,
            name: "p".into(),
            kind: "k".into(),
            desc: String::new(),
        })
        .await
        .unwrap();

    let id = ConnectorId::new();
    store
        .create_connector(NewConnector {
            id,
            name: "p 代码仓".into(),
            kind: CONNECTOR_KIND_GIT_REPO.into(),
            scope: "p".into(),
            project_id: Some(project),
            config: "/tmp/ws".into(),
        })
        .await
        .unwrap();

    let c = &store.list_connectors().await.unwrap()[0];
    assert_eq!(
        c.status,
        ConnectorStatus::Disconnected,
        "born disconnected — health comes only from a real probe"
    );
    assert_eq!(c.project_id, Some(project));
    assert_eq!(c.config, "/tmp/ws");

    store
        .set_connector_sync(id, ConnectorStatus::Connected, "2026-07-14 10:00")
        .await
        .unwrap();
    drop(store);
    let store = SqliteStore::open(&path).await.unwrap();
    let c = &store.list_connectors().await.unwrap()[0];
    assert_eq!(c.status, ConnectorStatus::Connected);
    assert_eq!(c.last_sync, "2026-07-14 10:00");

    let _ = std::fs::remove_file(&path);
}

/// The 完整形态 migration guard: a database created *before* the new columns
/// (simulated by dropping them is impossible in sqlite — instead verify the
/// stage-entity seeder is by-name idempotent and fills an already-seeded DB).
#[tokio::test]
async fn stage_entities_seed_into_existing_dbs_idempotently() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();

    bw_store::seed_stage_entities_if_missing(&store)
        .await
        .unwrap();
    let skills_1 = store.list_skills().await.unwrap();
    let agents_1 = store.list_agents().await.unwrap();
    assert_eq!(agents_1.len(), 5, "五角色 agent 实体");
    assert!(skills_1.len() >= 5, "每阶段至少一个技能实体");
    let proto = agents_1.iter().find(|a| a.name == "原型师").unwrap();
    assert!(
        proto.instructions.contains("原型师"),
        "role agent carries its real preamble template"
    );
    assert!(skills_1.iter().all(|s| {
        // Stage skills carry real bodies; (this DB has only stage skills.)
        !s.content.trim().is_empty()
    }));

    // Second call: by-name idempotent, nothing duplicated.
    bw_store::seed_stage_entities_if_missing(&store)
        .await
        .unwrap();
    assert_eq!(store.list_skills().await.unwrap().len(), skills_1.len());
    assert_eq!(store.list_agents().await.unwrap().len(), 5);

    let _ = std::fs::remove_file(&path);
}
