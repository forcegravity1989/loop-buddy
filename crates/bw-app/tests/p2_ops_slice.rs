//! **P2 exit gate (plan `04 §P2`).** A headless proof of the operating-view data
//! slice. The native window can't be screenshotted in CI, but the *data contract*
//! `showProgStage` renders from can be locked: drive the exact `Command` sequence
//! the desktop wizard dispatches (P2-B), then assert the three reads the bridge's
//! `build_ops` consumes — `stage_details` + `metric_trends` + `persisted_signals`
//! — reflect the entered Manual value and the **derived, never fabricated**
//! signal.
//!
//! This complements `spine.rs` (the P1 kernel gate): same flow, but it asserts the
//! *new* ops reads that the operating view renders, end to end from the wizard.

use bw_app::{App, Command};
use bw_core::model::StageKind;
use bw_core::{MetricId, ProjectId, Signal};
use bw_engine::{Engine, MockExecutor};
use bw_store::{MetricRole, SqliteStore, Store};
use std::sync::Arc;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_p2ops_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

/// New project → 7 wizard steps → record one Manual leading value → complete,
/// then assert the operating view's data path mirrors it.
#[tokio::test]
async fn wizard_flow_feeds_show_prog_stage() {
    let path = tmp_db();
    let project = ProjectId::new();
    let metric = MetricId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(store.clone(), Engine::new(MockExecutor::new()));

    // ── the desktop wizard's dispatch sequence (P2-B) ──────────────────────────
    app.dispatch(Command::CreateProject {
        id: project,
        name: "增长看板".into(),
        kind: "看板 / 网页应用".into(),
    })
    .await
    .unwrap();
    for step in 1..=7u8 {
        app.dispatch(Command::SetWizardStep { step }).await.unwrap();
    }
    app.dispatch(Command::UpdateNorthStar {
        value: "每周留存对话用户数".into(),
        def: "7日内有≥2次有效对话的用户".into(),
    })
    .await
    .unwrap();
    // Step 4: a leading metric whose current value is recorded as a Manual
    // observation. `8` against `≥5` must derive Green — from the value, not set.
    app.dispatch(Command::UpsertManualMetric {
        id: metric,
        name: "每周有效对话数".into(),
        role: MetricRole::Leading,
        stage_kind: Some(StageKind::Leading),
        target: "≥5".into(),
        amber: Default::default(),
        value: "8".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteWizard).await.unwrap();

    // ── what showProgStage renders off (bridge::build_ops reads exactly these) ──

    // 1. stage_details — the 7 control points in canonical order.
    let details = store.stage_details(project).await.unwrap();
    assert_eq!(details.len(), 7);
    assert_eq!(
        details.iter().map(|d| d.kind).collect::<Vec<_>>(),
        StageKind::ALL.to_vec()
    );
    // owns/accept/control are empty post-wizard: no path populates stage
    // definitions yet (the wizard doesn't capture them, `seven_stages` defaults
    // them blank). The UI honestly renders "—". This pins that known P2 gap so a
    // future stage-definition source is a deliberate change, not a silent one.
    let lead_detail = details
        .iter()
        .find(|d| d.kind == StageKind::Leading)
        .unwrap();
    assert!(
        lead_detail.owns.is_empty()
            && lead_detail.accept.is_empty()
            && lead_detail.control.is_empty(),
        "P2: owns/accept/control unpopulated until a stage-definition source lands"
    );

    // 2. metric_trends — the REAL observation series. One observation → one point.
    let trends = store.metric_trends(project).await.unwrap();
    let lead_trend = trends.iter().find(|t| t.name == "每周有效对话数").unwrap();
    assert_eq!(lead_trend.stage_kind, Some(StageKind::Leading));
    assert_eq!(
        lead_trend.trend,
        vec![8.0],
        "one observation yields one honest point — never a fabricated series"
    );

    // 3. persisted_signals — the derived, read-only cache the dots render from.
    let sigs = store.persisted_signals(project).await.unwrap();
    let m = sigs
        .metrics
        .iter()
        .find(|x| x.name == "每周有效对话数")
        .unwrap();
    assert_eq!(m.value_raw, "8");
    assert_eq!(
        m.signal,
        Some(Signal::Green),
        "8 ≥ 5 derives Green from the entered value, not a hand-set signal"
    );
    assert_eq!(m.hit, Some(true));
    let lead_stage = sigs
        .stages
        .iter()
        .find(|s| s.kind == StageKind::Leading)
        .unwrap();
    assert_eq!(lead_stage.routine, Some(Signal::Green));

    let _ = std::fs::remove_file(&path);
}
