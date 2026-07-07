//! Per-stage standard workflow templates. These are *methodology content*
//! (phase names, goals, gates — straight from the prototype's 控制点 copy),
//! not simulated data: every run instantiates a fresh spec and its outputs are
//! real engine events persisted as session messages.

use bw_core::model::{LoopConfig, StageKind, WorkflowKind, WorkflowSpec};
use bw_core::WorkflowId;

/// The standard (dynamic, use-and-discard) workflow for one control point.
pub fn stage_workflow(kind: StageKind) -> WorkflowSpec {
    let (phases, goal, prompt): (&[&str], &str, &str) = match kind {
        StageKind::CompetitorInsight => (
            &["界定", "采集", "结构化", "分析"],
            "产出竞品矩阵与机会缺口;「发现→洞察」过 GATE 由人确认",
            "围绕项目的对标名单,界定比较维度→采集公开信号→结构化为矩阵→分析差距",
        ),
        StageKind::RequirementIntake => (
            &["收集", "去重归并", "对齐控制点"],
            "把零散诉求合并为可执行需求,并对齐到七个控制点",
            "汇集本周新增诉求,合并重复项,标注影响的控制点与指标",
        ),
        StageKind::NorthStar => (
            &["口径核对", "数据源检查"],
            "北极星口径清晰、可计算、难造假",
            "核对北极星计算口径与数据源,标记任何可被粉饰的环节",
        ),
        StageKind::Leading => (
            &["取数", "对照目标", "定位偏差"],
            "每条引领指标有最新值,偏差有解释",
            "为每条引领指标取最新值,对照本周目标,给出偏差定位",
        ),
        StageKind::Lagging => (
            &["取数", "趋势判读"],
            "滞后指标趋势清楚,无粉饰",
            "为每条滞后指标取最新值并判读趋势",
        ),
        StageKind::PrototypeCreate => (
            &["拆解反馈", "迭代原型", "回归检查"],
            "原型即规格:本轮反馈全部落进可点击原型",
            "把最新反馈拆解为原型改动,迭代后做回归检查",
        ),
        StageKind::ProgressMgmt => (
            &["汇总各环节", "生成周报"],
            "周计划对齐:目标、实际、抓手三对齐",
            "汇总七个环节的进度与信号,生成本周复盘素材",
        ),
    };
    WorkflowSpec {
        id: WorkflowId::new(),
        name: format!("「{}」标准工作流", kind.label()),
        kind: WorkflowKind::Dynamic {
            origin: "环节标准模板".into(),
            stage: kind.label().into(),
        },
        prompt: prompt.into(),
        goal: goal.into(),
        stage_ref: Some(kind.index()),
        phases: phases.iter().map(|s| s.to_string()).collect(),
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
    }
}
