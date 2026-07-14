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
        phase_prompts: vec![],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
    }
}

/// A phase-recording executor: captures each PhaseNode's prompt and
/// prior_summary so the per-phase selection + relay-baton behavior can be
/// asserted from outside the engine.
struct Recording(std::sync::Mutex<Vec<(String, Option<String>)>>);

#[async_trait::async_trait]
impl bw_engine::Executor for Recording {
    async fn run_phase(
        &self,
        phase: &bw_engine::PhaseNode,
        _ctx: &RunCtx,
    ) -> Result<bw_engine::PhaseOutput, bw_engine::ExecError> {
        self.0
            .lock()
            .unwrap()
            .push((phase.prompt.clone(), phase.prior_summary.clone()));
        Ok(bw_engine::PhaseOutput {
            text: format!("完成:{}", phase.name),
            done: true,
            gaps: vec![],
        })
    }
}

#[tokio::test]
async fn per_phase_prompts_are_selected_and_baton_relayed() {
    let rec = Arc::new(Recording(std::sync::Mutex::new(Vec::new())));
    let engine = Engine::new(rec.clone());
    let ctx = RunCtx {
        project: bw_core::ProjectId::nil(),
        workflow: WorkflowId::nil(),
    };

    let mut s = spec(&["证据", "洞察", "假设"]);
    // Middle phase deliberately blank ⇒ falls back to the shared prompt.
    s.phase_prompts = vec!["做证据".into(), "  ".into(), "做假设".into()];

    engine.run_workflow(&s, &ctx, |_| {}).await.unwrap();

    let calls = rec.0.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert_eq!(calls[0].0, "做证据");
    assert_eq!(
        calls[1].0, "界定→采集→结构化→分析",
        "空白条目回退共享 prompt"
    );
    assert_eq!(calls[2].0, "做假设");
    // Relay baton: phase 1 has none; later phases carry the previous output.
    assert!(calls[0].1.is_none());
    assert_eq!(calls[1].1.as_deref(), Some("完成:证据"));
    assert_eq!(calls[2].1.as_deref(), Some("完成:洞察"));
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
