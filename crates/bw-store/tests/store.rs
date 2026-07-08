//! bw-store level checks: append-only observation → recompute derives signals →
//! persistence survives a reopen, and the persisted cache matches an independent
//! `bw_core` derive (no fabrication). Plus the handoff/DoD audit trail
//! (体系重构 v2 `§07`③): append-only, never silently blocked.

use bw_core::derive::{evaluate_metric, measure, parse_target, reduce_worst_of, Measurement};
use bw_core::model::{Cadence, ProjectPhase, SourceKind, StageKind};
use bw_core::{MetricId, ProjectId, Signal};
use bw_store::{MetricRole, NewMetric, NewProject, NewStage, SqliteStore, Store};
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
