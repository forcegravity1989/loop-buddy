//! bw-store level checks: append-only observation → recompute derives signals →
//! persistence survives a reopen, and the persisted cache matches an independent
//! `bw_core` derive (no fabrication).

use bw_core::derive::{evaluate_metric, measure, parse_target, reduce_worst_of, Measurement};
use bw_core::model::{Cadence, ProjectPhase, SourceKind, StageKind, StagePhase};
use bw_core::{MetricId, ProjectId, Signal};
use bw_store::{MetricRole, NewMetric, NewProject, NewStage, SqliteStore, Store};
use time::OffsetDateTime;

fn tmp_db() -> String {
    let p = std::env::temp_dir().join(format!("bw_store_test_{}.db", uuid::Uuid::new_v4()));
    p.to_string_lossy().into_owned()
}

fn all_seven(project: ProjectId) -> Vec<NewStage> {
    StageKind::ALL
        .into_iter()
        .map(|kind| NewStage {
            project_id: project,
            kind,
            phase: StagePhase::Running,
            progress: 0,
            schedule: Cadence::Weekly,
            owns: String::new(),
            accept: String::new(),
            control: String::new(),
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
                stage_kind: Some(StageKind::Leading),
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
        store.materialize_stages(all_seven(project)).await.unwrap();
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

    // L4: Leading stage routine = worst-of [Green] = Green
    let lead_stage = sigs
        .stages
        .iter()
        .find(|s| s.kind == StageKind::Leading)
        .unwrap();
    assert_eq!(lead_stage.routine, Some(Signal::Green));

    // L6: project = worst-of (Green + 6×Unknown) = Green (a green tolerates unknowns)
    let stage_signals: Vec<Signal> = sigs.stages.iter().map(|s| s.routine.unwrap()).collect();
    assert_eq!(
        sigs.project,
        Some(reduce_worst_of(stage_signals).into_inner())
    );
    assert_eq!(sigs.project, Some(Signal::Green));

    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.name, "增长看板");
    assert_eq!(proj.phase as u8, ProjectPhase::ColdStart as u8); // phase not advanced here

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
            stage_kind: Some(StageKind::Leading),
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
    store.materialize_stages(all_seven(project)).await.unwrap();
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
