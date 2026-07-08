//! Per-stage standard workflow templates. Phases mirror each stage's real
//! method loop (`StageKind::method_loop`) — methodology content straight from
//! the design doc, not simulated data: every run instantiates a fresh spec and
//! its outputs are real engine events persisted as session messages.

use bw_core::model::{LoopConfig, StageKind, WorkflowKind, WorkflowSpec};
use bw_core::WorkflowId;

/// The standard (dynamic, use-and-discard) workflow for one stage, driven
/// straight through its method loop.
pub fn stage_workflow(kind: StageKind) -> WorkflowSpec {
    let goal = format!(
        "{} → {}",
        kind.core_question(),
        kind.dod_items().first().copied().unwrap_or("交棒条件达成")
    );
    WorkflowSpec {
        id: WorkflowId::new(),
        name: format!("「{}」标准工作流", kind.label()),
        kind: WorkflowKind::Dynamic {
            origin: "阶段标准模板".into(),
            stage: kind.label().into(),
        },
        prompt: kind.method_loop().join(" → "),
        goal,
        stage_ref: Some(kind.index()),
        phases: kind.method_loop().iter().map(|s| s.to_string()).collect(),
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
    }
}

/// The drafting run for the creation flow: one workflow, phases matching the
/// "正在按方法论起草体系" loading copy. Runs through the same `Engine` as any
/// other workflow — `MockExecutor` produces a clearly-labeled mock transcript;
/// nothing here is injected into the editable draft fields as fact.
pub fn drafting_workflow() -> WorkflowSpec {
    WorkflowSpec {
        id: WorkflowId::new(),
        name: "创建 · 体系起草".into(),
        kind: WorkflowKind::Dynamic {
            origin: "创建流程".into(),
            stage: StageKind::Prototype.label().into(),
        },
        prompt: "周期判定 → 北极星起草 → 指标框架 → 阶段激活".into(),
        goal: "产出可编辑的北极星候选 + 指标框架草案".into(),
        stage_ref: Some(StageKind::Prototype.index()),
        phases: vec![
            "周期判定".into(),
            "北极星起草".into(),
            "指标框架".into(),
            "阶段激活".into(),
        ],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 1,
        },
    }
}
