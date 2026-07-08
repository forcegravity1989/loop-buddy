//! `Engine::run_workflow` drives a spec's phases through the MockExecutor and
//! emits the expected event sequence.

use std::sync::Arc;

use bw_core::model::{LoopConfig, WorkflowKind, WorkflowSpec};
use bw_core::WorkflowId;
use bw_engine::{Engine, MockExecutor, RunCtx, RunEvent};

fn spec(phases: &[&str]) -> WorkflowSpec {
    WorkflowSpec {
        id: WorkflowId::nil(),
        name: "竞品洞察工作流".into(),
        kind: WorkflowKind::Dynamic {
            origin: "向导".into(),
            stage: "竞品洞察".into(),
        },
        prompt: "界定→采集→结构化→分析".into(),
        goal: "产出竞品矩阵".into(),
        stage_ref: Some(1),
        phases: phases.iter().map(|s| s.to_string()).collect(),
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
    }
}

#[tokio::test]
async fn runs_all_phases_in_order_with_events() {
    let engine = Engine::new(Arc::new(MockExecutor::new()));
    let ctx = RunCtx {
        project: bw_core::ProjectId::nil(),
        workflow: WorkflowId::nil(),
    };

    let mut events = Vec::new();
    let summary = engine
        .run_workflow(&spec(&["界定", "采集", "分析"]), &ctx, |e| {
            events.push(e)
        })
        .await
        .unwrap();

    assert_eq!(summary.phases_run, 3);
    assert!(summary.final_output.contains("分析"));

    // started/completed per phase (3+3), then a single done.
    let started = events
        .iter()
        .filter(|e| matches!(e, RunEvent::PhaseStarted { .. }))
        .count();
    let completed = events
        .iter()
        .filter(|e| matches!(e, RunEvent::PhaseCompleted { .. }))
        .count();
    let done = events
        .iter()
        .filter(|e| matches!(e, RunEvent::WorkflowDone { .. }))
        .count();
    assert_eq!((started, completed, done), (3, 3, 1));

    // last event is WorkflowDone.
    assert!(matches!(events.last(), Some(RunEvent::WorkflowDone { .. })));
}

#[tokio::test]
async fn empty_workflow_still_reports_done() {
    let engine = Engine::new(Arc::new(MockExecutor::new()));
    let ctx = RunCtx {
        project: bw_core::ProjectId::nil(),
        workflow: WorkflowId::nil(),
    };
    let mut events = Vec::new();
    let summary = engine
        .run_workflow(&spec(&[]), &ctx, |e| events.push(e))
        .await
        .unwrap();
    assert_eq!(summary.phases_run, 0);
    assert!(matches!(events.as_slice(), [RunEvent::WorkflowDone { .. }]));
}
