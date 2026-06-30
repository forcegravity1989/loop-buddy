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

/// `stage_details` returns the 7 stages in `StageKind::ALL` order carrying the
/// owns/accept/control persisted at materialize time, and `metric_trends`
/// surfaces only the real persisted observations (two appended → two points,
/// oldest→newest — never a fabricated multi-week series).
#[tokio::test]
async fn stage_details_and_metric_trend_are_read_from_real_rows() {
    let path = tmp_db();
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let project = ProjectId::new();
    let metric = MetricId::new();

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

    // Two real observations, in time order. The display raws carry a unit suffix
    // so we also prove the numeric-prefix parse ("6/7" → 6.0, "8" → 8.0).
    store
        .append_observation(metric, SourceKind::Manual, "6/7", now)
        .await
        .unwrap();
    store
        .append_observation(
            metric,
            SourceKind::Manual,
            "8",
            now + time::Duration::days(7),
        )
        .await
        .unwrap();

    // Materialize 7 stages, giving the Leading stage a distinct owns/accept/control
    // so we can assert it round-trips (others left empty).
    let stages: Vec<NewStage> = StageKind::ALL
        .into_iter()
        .map(|kind| {
            let is_lead = kind == StageKind::Leading;
            NewStage {
                project_id: project,
                kind,
                phase: if is_lead {
                    StagePhase::Iterating
                } else {
                    StagePhase::Running
                },
                progress: if is_lead { 40 } else { 0 },
                schedule: Cadence::Weekly,
                owns: if is_lead {
                    "三条引领指标".into()
                } else {
                    String::new()
                },
                accept: if is_lead {
                    "可控 / 可统计 / 难造假".into()
                } else {
                    String::new()
                },
                control: if is_lead {
                    "三者同时满足".into()
                } else {
                    String::new()
                },
            }
        })
        .collect();
    store.materialize_stages(stages).await.unwrap();
    store.recompute_signals(project, now).await.unwrap();

    // ── stage_details: 7 rows, canonical order, owns/accept/control preserved ──
    let details = store.stage_details(project).await.unwrap();
    assert_eq!(details.len(), 7);
    let order: Vec<StageKind> = details.iter().map(|d| d.kind).collect();
    assert_eq!(
        order,
        StageKind::ALL.to_vec(),
        "returned in StageKind::ALL order"
    );

    let lead = details
        .iter()
        .find(|d| d.kind == StageKind::Leading)
        .unwrap();
    assert_eq!(lead.owns, "三条引领指标");
    assert_eq!(lead.accept, "可控 / 可统计 / 难造假");
    assert_eq!(lead.control, "三者同时满足");
    assert_eq!(lead.progress, 40);
    assert_eq!(lead.phase as u8, StagePhase::Iterating as u8);

    // ── metric_trends: exactly the 2 real points, oldest→newest, no fabrication ──
    let trends = store.metric_trends(project).await.unwrap();
    let mt = trends.iter().find(|t| t.name == "每周有效对话数").unwrap();
    assert_eq!(mt.stage_kind, Some(StageKind::Leading));
    assert_eq!(
        mt.trend,
        vec![6.0_f32, 8.0_f32],
        "two observations → two points, in order"
    );

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
