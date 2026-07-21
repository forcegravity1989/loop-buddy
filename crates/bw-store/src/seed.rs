//! Real skill/agent/workflow catalog seeded from OMC (oh-my-claudecode) and
//! ECC (Everything Claude Code) role-capability maps — not fabricated sample
//! data. `seed_hub_if_empty` only inserts when all three hub tables are empty,
//! so re-opening an already-seeded database is a no-op, never a duplicate.
//!
//! 73 OMC skills, 37 OMC agents, 271 ECC
//! skills, 67 ECC agents, 92 ECC commands (modelled
//! as workflows — each is a real, named, invokable multi-step command in the
//! source repo, the closest existing category to this app's `WorkflowSpec`).
//! Categories map onto the 5-stage lifecycle by the source documents' own
//! role split (原型构建/Builder/优化清理者/增长者/维护者 ==
//! Prototype/Build/Optimize/Growth/Ops).
//!
//! On top of that external catalog, `seed_hub_if_empty` also plants the
//! app's own 5 stage-template workflows (`bw_core::model::stage_template_workflow`,
//! one per `StageKind`) — not external data, but this app's own already-designed
//! methodology (`StageKind`'s core question / method loop / DoD) made into a
//! standing, importable Hub entry instead of only the ephemeral spec a
//! session builds on the fly.

use crate::{NewAgent, NewSkill, NewWorkflowSpec, Result, Store};
use bw_core::model::{
    stage_template_workflow, HubSource, LibSource, LoopConfig, Maturity, StageKind, WorkflowKind,
};
use bw_core::{AgentId, SkillId, WorkflowId};

struct SeedSkill {
    name: &'static str,
    desc: &'static str,
    category: &'static str,
}

struct SeedAgent {
    name: &'static str,
    desc: &'static str,
    category: &'static str,
}

struct SeedWorkflow {
    name: &'static str,
    desc: &'static str,
    stage_ref: u8,
    source: HubSource,
}

const OMC_SKILLS: &[SeedSkill] = &[
    SeedSkill {
        name: r##"deep-research"##,
        desc: r##"竞品/参考多源调研并核验"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"deep-interview"##,
        desc: r##"深挖真实问题与场景(触发: deep interview)"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"web-access"##,
        desc: r##"联网检索参考资料"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"external-context"##,
        desc: r##"引入外部上下文/资料"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"plan"##,
        desc: r##"规划落盘(触发: /oh-my-claudecode:plan)"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"overall-planning"##,
        desc: r##"全局统筹"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"investigation-first"##,
        desc: r##"先调研再动手"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"critical-thinking-logical-reasoning"##,
        desc: r##"逻辑校验问题定义"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"contradiction-analysis"##,
        desc: r##"识别需求矛盾点"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"support-with-evidence"##,
        desc: r##"用证据支撑价值主张"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"spark-prairie-fire"##,
        desc: r##"创意火花/点子发散"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"mass-line"##,
        desc: r##"从用户中来到用户中去"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"draw-io-diagram-generator"##,
        desc: r##"原型/流程图"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"presentation-outline / pptx"##,
        desc: r##"原型汇报材料"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"deep-dive"##,
        desc: r##"专题深挖(触发: deep-analyze)"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"ultrawork"##,
        desc: r##"高密度执行模式(触发: ulw)"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"autopilot"##,
        desc: r##"自动驾驶式持续构建(触发: autopilot)"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"ralph"##,
        desc: r##"永不停滚的迭代(触发: ralph)"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"ralplan"##,
        desc: r##"规划+滚动执行(触发: ralplan)"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"team"##,
        desc: r##"多 agent 流水线协作(触发: /team)"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"omc-teams"##,
        desc: r##"团队编排"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"TDD mode"##,
        desc: r##"测试驱动开发(触发: tdd)"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"deepinit"##,
        desc: r##"项目/环境初始化"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"init"##,
        desc: r##"脚手架"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"run"##,
        desc: r##"启动应用验证可用"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"verify"##,
        desc: r##"完成度核验"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"claude-api"##,
        desc: r##"构建 AI 应用时的 API 参考"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"mcp-setup"##,
        desc: r##"接入 MCP 工具/数据源"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"ccg"##,
        desc: r##"代码生成(触发: ccg)"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"release"##,
        desc: r##"打包发布"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"ai-slop-cleaner"##,
        desc: r##"去 AI 味/冗余产出(触发: deslop)"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"simplify"##,
        desc: r##"复用/简化/提效清理"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"code-review"##,
        desc: r##"diff 评审,可 --fix(触发: /code-review)"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"review"##,
        desc: r##"通用评审"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"criticism-self-criticism"##,
        desc: r##"自我批判式打磨"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"ultraqa"##,
        desc: r##"高质量 QA 轮次(触发: ultraqa)"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"visual-verdict"##,
        desc: r##"视觉验收"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"trace"##,
        desc: r##"因果追踪(触发: /trace)"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"debug"##,
        desc: r##"调试(触发: /debug)"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"verify"##,
        desc: r##"行为一致性核验"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"self-improve"##,
        desc: r##"沉淀改进"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"deep-research"##,
        desc: r##"市场/PMF 多源调研"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"novel-analytics"##,
        desc: r##"分析框架(可迁移到产品指标)"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"web-access"##,
        desc: r##"舆情/渠道检索"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"external-context"##,
        desc: r##"引入外部数据"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"presentation-outline / pptx"##,
        desc: r##"增长复盘材料"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"mass-line"##,
        desc: r##"收集真实用户反馈"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"qiqing-liuyu"##,
        desc: r##"舆情/留语监测"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"practice-cognition"##,
        desc: r##"从实践中提炼认知"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"protracted-strategy"##,
        desc: r##"持久战式增长策略"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"overall-planning"##,
        desc: r##"增长全局统筹"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"support-with-evidence"##,
        desc: r##"数据证据支撑决策"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"wiki"##,
        desc: r##"知识/经验沉淀"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"writer-memory"##,
        desc: r##"长期记忆沉淀"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"learner"##,
        desc: r##"从反馈中学习"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"configure-notifications"##,
        desc: r##"关键指标通知"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"security-review"##,
        desc: r##"安全审查(触发: /security-review)"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"review"##,
        desc: r##"变更评审"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"code-review"##,
        desc: r##"diff 把关"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"trace / debug"##,
        desc: r##"排障"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"verify"##,
        desc: r##"完成度/契约核验"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"omc-doctor"##,
        desc: r##"ECC 自身健康检查"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"omc-setup / setup"##,
        desc: r##"环境与工具同步"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"mcp-setup"##,
        desc: r##"MCP 工具链维护"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"hud"##,
        desc: r##"运行面板/可观测"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"loop"##,
        desc: r##"定时巡检/看护(触发: /loop)"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"configure-notifications"##,
        desc: r##"告警通知"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"fewer-permission-prompts"##,
        desc: r##"收敛权限提示"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"update-config"##,
        desc: r##"settings/hooks 维护"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"keybindings-help"##,
        desc: r##"键位维护"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"release"##,
        desc: r##"稳定发版"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"self-improve"##,
        desc: r##"流程持续改进"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"claude-md-improver"##,
        desc: r##"规则文件维护"##,
        category: r##"维护者"##,
    },
];

const OMC_AGENTS: &[SeedAgent] = &[
    SeedAgent {
        name: r##"analyst"##,
        desc: r##"[OPUS] 需求/问题域分析,前置咨询"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"planner"##,
        desc: r##"[OPUS] 访谈式规划,产出实现路径"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"architect"##,
        desc: r##"[OPUS] 只读架构建议,原型结构定型"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"designer"##,
        desc: r##"[SONNET] UI/UX 原型与视觉稿"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"document-specialist"##,
        desc: r##"外部文档/SDK 查证,竞品资料检索"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"writer"##,
        desc: r##"[HAIKU] 价值主张/说明文档"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"explore"##,
        desc: r##"在已有代码库中定位可复用资产"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"executor"##,
        desc: r##"[SONNET] 核心实现执行(复杂任务用 opus)"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"architect"##,
        desc: r##"[OPUS] 生产级架构与调试顾问"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"test-engineer"##,
        desc: r##"测试策略/集成/e2e/TDD"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"debugger"##,
        desc: r##"构建/编译/回归定位"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"git-master"##,
        desc: r##"原子提交/历史/分支管理"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"explore"##,
        desc: r##"代码定位"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"document-specialist"##,
        desc: r##"SDK/框架用法查证"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"code-simplifier"##,
        desc: r##"简化、提清晰、保功能"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"code-reviewer"##,
        desc: r##"严重度分级评审"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"critic"##,
        desc: r##"[OPUS] 多视角方案/代码审视"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"designer"##,
        desc: r##"[SONNET] 交互打磨"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"tracer"##,
        desc: r##"因果追踪,定位性能瓶颈"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"debugger"##,
        desc: r##"回归与卡顿排查"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"qa-tester"##,
        desc: r##"tmux 驱动的交互实测"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"verifier"##,
        desc: r##"验证优化未破坏行为"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"scientist"##,
        desc: r##"数据分析/指标研究"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"analyst"##,
        desc: r##"[OPUS] PMF/留存/转化分析"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"document-specialist"##,
        desc: r##"市场/竞品情报"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"writer"##,
        desc: r##"[HAIKU] 运营文案/公告"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"designer"##,
        desc: r##"[SONNET] 推广物料"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"explore"##,
        desc: r##"从代码/日志中提取使用信号"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"security-reviewer"##,
        desc: r##"OWASP/密钥/不安全模式"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"code-reviewer"##,
        desc: r##"回归与质量把关"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"debugger"##,
        desc: r##"线上问题定位"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"tracer"##,
        desc: r##"跨服务因果追踪"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"test-engineer"##,
        desc: r##"稳定性/防 flaky"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"qa-tester"##,
        desc: r##"CLI/回归实测"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"verifier"##,
        desc: r##"发布前核验"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"git-master"##,
        desc: r##"热修/回滚/历史"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"architect"##,
        desc: r##"[OPUS] 扩展性/容量架构"##,
        category: r##"维护者"##,
    },
];

const ECC_SKILLS: &[SeedSkill] = &[
    SeedSkill {
        name: r##"agent-sort"##,
        desc: r##"Build an evidence-backed ECC install plan for a specific repo by sorting skills, commands,"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"angular-developer"##,
        desc: r##"Generates Angular code and provides architectural guidance. Trigger when creating projects"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"architecture-decision-records"##,
        desc: r##"Capture architectural decisions made during Claude Code sessions as structured ADRs. Auto-"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"blueprint"##,
        desc: r##">-"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"codebase-onboarding"##,
        desc: r##"Analyze an unfamiliar codebase and generate a structured onboarding guide with architectur"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"council"##,
        desc: r##"Convene a four-voice council for ambiguous decisions, tradeoffs, and go/no-go calls. Use w"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"deep-research"##,
        desc: r##"Multi-source deep research using firecrawl and exa MCPs. Searches the web, synthesizes fin"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"design-system"##,
        desc: r##"Use this skill to generate or audit design systems, check visual consistency, and review P"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"documentation-lookup"##,
        desc: r##"Use up-to-date library and framework docs via Context7 MCP instead of training data. Activ"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"ecc-guide"##,
        desc: r##"Guide users through ECC's current agents, skills, commands, hooks, rules, install profiles"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"exa-search"##,
        desc: r##"Neural search via Exa MCP for web, code, and company research. Use when the user needs web"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"frontend-design-direction"##,
        desc: r##"Set an ECC-specific frontend design direction for production UI work. Use when building or"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"intent-driven-development"##,
        desc: r##"Turn ambiguous or high-impact product and engineering changes into scoped, verifiable acce"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"iterative-retrieval"##,
        desc: r##"Pattern for progressively refining context retrieval to solve the subagent context problem"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"jira-integration"##,
        desc: r##"Use this skill when retrieving Jira tickets, analyzing requirements, updating ticket statu"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"knowledge-ops"##,
        desc: r##"Knowledge base management, ingestion, sync, and retrieval across multiple storage layers ("##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"laravel-plugin-discovery"##,
        desc: r##"Discover and evaluate Laravel packages via LaraPlugins.io MCP. Use when the user wants to"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"liquid-glass-design"##,
        desc: r##"iOS 26 Liquid Glass design system — dynamic glass material with blur, reflection, and inte"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"plan-orchestrate"##,
        desc: r##"Read a plan document, decompose it into steps, design a per-step agent chain from the ECC"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"product-capability"##,
        desc: r##"Translate PRD intent, roadmap asks, or product discussions into an implementation-ready ca"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"product-lens"##,
        desc: r##"Use this skill to validate the "why" before building, run product diagnostics, and pressur"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"repo-scan"##,
        desc: r##"Cross-stack source code asset audit — classifies every file, detects embedded third-party"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"research-ops"##,
        desc: r##"Evidence-first current-state research workflow for ECC. Use when the user wants fresh fact"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"scientific-db-pubmed-database"##,
        desc: r##"Direct PubMed and NCBI E-utilities search workflows for biomedical literature, MeSH querie"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"scientific-db-uspto-database"##,
        desc: r##"USPTO patent and trademark data workflow for official record lookup, PatentSearch queries,"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"search-first"##,
        desc: r##"Research-before-coding workflow. Search for existing tools, libraries, and patterns before"##,
        category: r##"原型构建"##,
    },
    SeedSkill {
        name: r##"agent-harness-construction"##,
        desc: r##"Design and optimize AI agent action spaces, tool definitions, and observation formatting f"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"agentic-os"##,
        desc: r##"Build persistent multi-agent operating systems on Claude Code. Covers kernel architecture,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"ai-first-engineering"##,
        desc: r##"Engineering operating model for teams where AI agents generate a large share of implementa"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"ai-regression-testing"##,
        desc: r##"Regression testing strategies for AI-assisted development. Sandbox-mode API testing withou"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"android-clean-architecture"##,
        desc: r##"Clean Architecture patterns for Android and Kotlin Multiplatform projects — module structu"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"api-connector-builder"##,
        desc: r##"Build a new API connector or provider by matching the target repo's existing integration p"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"api-design"##,
        desc: r##"REST API design patterns including resource naming, status codes, pagination, filtering, e"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"autonomous-agent-harness"##,
        desc: r##"Transform Claude Code into a fully autonomous agent system with persistent memory, schedul"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"autonomous-loops"##,
        desc: r##""Patterns and architectures for autonomous Claude Code loops — from simple sequential pipe"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"backend-patterns"##,
        desc: r##"Backend architecture patterns, API design, database optimization, and server-side best pra"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"browser-qa"##,
        desc: r##"Use this skill to automate visual testing and UI interaction verification using browser au"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"bun-runtime"##,
        desc: r##"Bun as runtime, package manager, bundler, and test runner. When to choose Bun vs Node, mig"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"claude-devfleet"##,
        desc: r##"Orchestrate multi-agent coding tasks via Claude DevFleet — plan projects, dispatch paralle"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"clickhouse-io"##,
        desc: r##"ClickHouse database patterns, query optimization, analytics, and data engineering best pra"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"coding-standards"##,
        desc: r##"Baseline cross-project coding conventions for naming, readability, immutability, and code-"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"compose-multiplatform-patterns"##,
        desc: r##"Compose Multiplatform and Jetpack Compose patterns for KMP projects — state management, na"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"content-hash-cache-pattern"##,
        desc: r##"Cache expensive file processing results using SHA-256 content hashes — path-independent, a"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"continuous-agent-loop"##,
        desc: r##"Patterns for continuous autonomous agent loops with quality gates, evals, and recovery con"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"cpp-coding-standards"##,
        desc: r##"C++ coding standards based on the C++ Core Guidelines (isocpp.github.io). Use when writing"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"cpp-testing"##,
        desc: r##"Use only when writing/updating/fixing C++ tests, configuring GoogleTest/CTest, diagnosing"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"csharp-testing"##,
        desc: r##"C# and .NET testing patterns with xUnit, FluentAssertions, mocking, integration tests, and"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"dashboard-builder"##,
        desc: r##"Build monitoring dashboards that answer real operator questions for Grafana, SigNoz, and s"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"data-scraper-agent"##,
        desc: r##"Build a fully automated AI-powered data collection agent for any public source — job board"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"data-throughput-accelerator"##,
        desc: r##"Use when large data ingestion, backfill, export, ETL, warehouse loading, manifest catch-up"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"database-migrations"##,
        desc: r##"Database migration best practices for schema changes, data migrations, rollbacks, and zero"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"deployment-patterns"##,
        desc: r##"Deployment workflows, CI/CD pipeline patterns, Docker containerization, health checks, rol"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"django-celery"##,
        desc: r##"Django + Celery async task patterns — configuration, task design, beat scheduling, retries"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"django-patterns"##,
        desc: r##"Django architecture patterns, REST API design with DRF, ORM best practices, caching, signa"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"django-tdd"##,
        desc: r##"Django testing strategies with pytest-django, TDD methodology, factory_boy, mocking, cover"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"dmux-workflows"##,
        desc: r##"Multi-agent orchestration using dmux (tmux pane manager for AI agents). Patterns for paral"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"dotnet-patterns"##,
        desc: r##"Idiomatic C# and .NET patterns, conventions, dependency injection, async/await, and best p"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"dynamic-workflow-mode"##,
        desc: r##""Design task-local harnesses, eval gates, and reusable skill extraction for Claude dynamic"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"e2e-testing"##,
        desc: r##"Playwright E2E testing patterns, Page Object Model, configuration, CI/CD integration, arti"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"error-handling"##,
        desc: r##"Patterns for robust error handling across TypeScript, Python, and Go. Covers typed errors,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"fastapi-patterns"##,
        desc: r##"FastAPI best practices covering project structure, Pydantic v2 schemas, dependency injecti"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"foundation-models-on-device"##,
        desc: r##"Apple FoundationModels framework for on-device LLM — text generation, guided generation wi"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"fsharp-testing"##,
        desc: r##"F# testing patterns with xUnit, FsUnit, Unquote, FsCheck property-based testing, integrati"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"gan-style-harness"##,
        desc: r##""GAN-inspired Generator-Evaluator agent harness for building high-quality applications aut"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"generating-python-installer"##,
        desc: r##""Commercial-grade Python installer expert for Windows: Nuitka extreme compilation, dist sl"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"git-workflow"##,
        desc: r##"Git workflow patterns including branching strategies, commit conventions, merge vs rebase,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"golang-patterns"##,
        desc: r##"Idiomatic Go patterns, best practices, and conventions for building robust, efficient, and"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"healthcare-cdss-patterns"##,
        desc: r##"Clinical Decision Support System (CDSS) development patterns. Drug interaction checking, d"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"hermes-imports"##,
        desc: r##"Convert local Hermes operator workflows into sanitized ECC skills and release-pack artifac"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"hexagonal-architecture"##,
        desc: r##"Design, implement, and refactor Ports & Adapters systems with clear domain boundaries, dep"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"java-coding-standards"##,
        desc: r##""Java coding standards for Spring Boot and Quarkus services: naming, immutability, Optiona"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"jpa-patterns"##,
        desc: r##"JPA/Hibernate patterns for entity design, relationships, query optimization, transactions,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"kotlin-coroutines-flows"##,
        desc: r##"Kotlin Coroutines and Flow patterns for Android and KMP — structured concurrency, Flow ope"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"kotlin-exposed-patterns"##,
        desc: r##"JetBrains Exposed ORM patterns including DSL queries, DAO pattern, transactions, HikariCP"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"kotlin-ktor-patterns"##,
        desc: r##"Ktor server patterns including routing DSL, plugins, authentication, Koin DI, kotlinx.seri"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"kotlin-patterns"##,
        desc: r##"Idiomatic Kotlin patterns, best practices, and conventions for building robust, efficient,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"kotlin-testing"##,
        desc: r##"Kotlin testing patterns with Kotest, MockK, coroutine testing, property-based testing, and"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"kubernetes-patterns"##,
        desc: r##"Kubernetes workload patterns, resource management, RBAC, probes, autoscaling, ConfigMap/Se"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"laravel-patterns"##,
        desc: r##"Laravel architecture patterns, routing/controllers, Eloquent ORM, service layers, queues,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"laravel-tdd"##,
        desc: r##"Laravel testing strategies with PHPUnit, Pest, model factories, HTTP tests, Sanctum authen"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"laravel-verification"##,
        desc: r##""Verification loop for Laravel projects: env checks, linting, static analysis, tests with"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"mcp-server-patterns"##,
        desc: r##"Build MCP servers with Node/TypeScript SDK — tools, resources, prompts, Zod validation, st"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"motion-advanced"##,
        desc: r##"Advanced motion patterns for React / Next.js — drag & drop, gestures, text animations, SVG"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"motion-foundations"##,
        desc: r##"Motion tokens, spring presets, performance rules, device adaptation, accessibility enforce"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"motion-patterns"##,
        desc: r##"Production-ready animation patterns for React / Next.js — button, modal, toast, stagger, p"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"motion-ui"##,
        desc: r##""Production-ready UI motion system for React/Next.js. Use when implementing animations, tr"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"mysql-patterns"##,
        desc: r##"MySQL and MariaDB schema, query, indexing, transaction, replication, and connection-pool p"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"nestjs-patterns"##,
        desc: r##"NestJS architecture patterns for modules, controllers, providers, DTO validation, guards,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"nextjs-turbopack"##,
        desc: r##"Next.js 16+ and Turbopack — incremental bundling, FS caching, dev speed, and when to use T"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"orch-add-feature"##,
        desc: r##"Orchestrate building a brand-new feature end to end — research, plan, TDD implementation,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"orch-build-mvp"##,
        desc: r##"Orchestrate bootstrapping a working MVP from a design or spec document — ingest the doc, p"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"orch-change-feature"##,
        desc: r##"Orchestrate altering an existing, working feature to new desired behavior — update its tes"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"orch-fix-defect"##,
        desc: r##"Orchestrate fixing a bug — reproduce it as a failing regression test, fix to green, review"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"orch-pipeline"##,
        desc: r##"Shared orchestration engine for the orch-* skill family. Defines the gated Research-Plan-T"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"orch-refine-code"##,
        desc: r##"Orchestrate a behavior-preserving refactor — confirm tests are green, restructure without"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"parallel-execution-optimizer"##,
        desc: r##"Use when the user wants a task done much faster through parallel work, concurrent agents,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"perl-patterns"##,
        desc: r##"Modern Perl 5.36+ idioms, best practices, and conventions for building robust, maintainabl"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"perl-testing"##,
        desc: r##"Perl testing patterns using Test2::V0, Test::More, prove runner, mocking, coverage with De"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"prisma-patterns"##,
        desc: r##"Prisma ORM patterns for TypeScript backends — schema design, query optimization, transacti"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"python-patterns"##,
        desc: r##"Pythonic idioms, PEP 8 standards, type hints, and best practices for building robust, effi"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"python-testing"##,
        desc: r##"Python testing strategies using pytest, TDD methodology, fixtures, mocking, parametrizatio"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"quarkus-patterns"##,
        desc: r##"Quarkus 3.x LTS architecture patterns with Camel for messaging, RESTful API design, CDI se"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"quarkus-tdd"##,
        desc: r##"Test-driven development for Quarkus 3.x LTS using JUnit 5, Mockito, REST Assured, Camel te"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"quarkus-verification"##,
        desc: r##""Verification loop for Quarkus projects: build, static analysis, tests with coverage, secu"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"react-patterns"##,
        desc: r##"React 18/19 patterns including hooks discipline, server/client component boundaries, Suspe"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"redis-patterns"##,
        desc: r##"Redis data structure patterns, caching strategies, distributed locks, rate limiting, pub/s"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"rust-patterns"##,
        desc: r##"Idiomatic Rust patterns, ownership, error handling, traits, concurrency, and best practice"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"rust-testing"##,
        desc: r##"Rust testing patterns including unit tests, integration tests, async testing, property-bas"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"scientific-pkg-gget"##,
        desc: r##"gget CLI and Python workflow for quick genomic database queries, sequence lookup, BLAST-st"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"springboot-patterns"##,
        desc: r##"Spring Boot architecture patterns, REST API design, layered services, data access, caching"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"springboot-tdd"##,
        desc: r##"Test-driven development for Spring Boot using JUnit 5, Mockito, MockMvc, Testcontainers, a"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"springboot-verification"##,
        desc: r##""Verification loop for Spring Boot projects: build, static analysis, tests with coverage,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"swift-actor-persistence"##,
        desc: r##"Thread-safe data persistence in Swift using actors — in-memory cache with file-backed stor"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"swift-concurrency-6-2"##,
        desc: r##"Swift 6.2 Approachable Concurrency — single-threaded by default, @concurrent for explicit"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"swiftui-patterns"##,
        desc: r##"SwiftUI architecture patterns, state management with @Observable, view composition, naviga"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"team-agent-orchestration"##,
        desc: r##""Run team-based orchestration for agent squads using work items, ownership, agent Kanban,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"team-builder"##,
        desc: r##"Interactive agent picker for composing and dispatching parallel teams"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"tinystruct-patterns"##,
        desc: r##"Expert guidance for developing with the tinystruct Java framework. Use when working on the"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"ui-to-vue"##,
        desc: r##"Use when the user has UI screenshots or design exports that need batch conversion into Vue"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"vite-patterns"##,
        desc: r##"Vite build tool patterns including config, plugins, HMR, env variables, proxy setup, SSR,"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"vue-patterns"##,
        desc: r##"Vue.js 3 Composition API patterns, component architecture, reactivity best practices, Pini"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"windows-desktop-e2e"##,
        desc: r##"E2E testing for Windows native desktop apps (WPF, WinForms, Win32/MFC, Qt) using pywinauto"##,
        category: r##"Builder"##,
    },
    SeedSkill {
        name: r##"accessibility"##,
        desc: r##"Design, implement, and audit inclusive digital products using WCAG 2.2 Level AA"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"agent-eval"##,
        desc: r##"Head-to-head comparison of coding agents (Claude Code, Aider, Codex, etc.) on custom tasks"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"agent-introspection-debugging"##,
        desc: r##"Structured self-debugging workflow for AI agent failures using capture, diagnosis, contain"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"agent-self-evaluation"##,
        desc: r##"Use after completing any non-trivial task. The agent self-rates its output on 5 axes — acc"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"benchmark"##,
        desc: r##"Use this skill to measure performance baselines, detect regressions before/after PRs, and"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"benchmark-methodology"##,
        desc: r##">-"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"benchmark-optimization-loop"##,
        desc: r##"Use when the user asks to make something faster, try many variants, run recursive optimiza"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"click-path-audit"##,
        desc: r##""Trace every user-facing button/touchpoint through its full state change sequence to find"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"code-tour"##,
        desc: r##"Create CodeTour `.tour` files — persona-targeted, step-by-step walkthroughs with real file"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"continuous-learning"##,
        desc: r##""[DEPRECATED - use continuous-learning-v2] Legacy v1 stop-hook skill extractor. v2 is a st"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"continuous-learning-v2"##,
        desc: r##"Instinct-based learning system that observes sessions via hooks, creates atomic instincts"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"eval-harness"##,
        desc: r##"Formal evaluation framework for Claude Code sessions implementing eval-driven development"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"flutter-dart-code-review"##,
        desc: r##"Library-agnostic Flutter/Dart code review checklist covering widget best practices, state"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"frontend-a11y"##,
        desc: r##">"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"frontend-patterns"##,
        desc: r##"Frontend development patterns for React, Next.js, state management, performance optimizati"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"golang-testing"##,
        desc: r##"Go testing patterns including table-driven tests, subtests, benchmarks, fuzzing, and test"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"inherit-legacy-style"##,
        desc: r##"Legacy-project style inheritance skill. Use when the user types /inherit-legacy-style, or"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"latency-critical-systems"##,
        desc: r##"Use for latency-sensitive systems such as realtime dashboards, market data, streaming agen"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"make-interfaces-feel-better"##,
        desc: r##"Apply concrete design-engineering details that make interfaces feel polished. Use when rev"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"mle-workflow"##,
        desc: r##"Production machine-learning engineering workflow for data contracts, reproducible training"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"plankton-code-quality"##,
        desc: r##""Write-time code quality enforcement using Plankton — auto-formatting, linting, and Claude"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"prompt-optimizer"##,
        desc: r##">-"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"pytorch-patterns"##,
        desc: r##"PyTorch deep learning patterns and best practices for building robust, efficient, and repr"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"react-performance"##,
        desc: r##"React and Next.js performance optimization patterns adapted from Vercel Engineering's Reac"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"recursive-decision-ledger"##,
        desc: r##"Use when the user asks for repeated rollouts, marked decision processes, high-dimensional"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"regex-vs-llm-structured-text"##,
        desc: r##"Decision framework for choosing between regex and LLM when parsing structured text — start"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"rules-distill"##,
        desc: r##""Scan skills to extract cross-cutting principles and distill them into rules — append, rev"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"santa-method"##,
        desc: r##""Multi-agent adversarial verification with convergence loop. Two independent review agents"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"scientific-thinking-literature-review"##,
        desc: r##"Systematic literature-review workflow for academic, biomedical, technical, and scientific"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"scientific-thinking-scholar-evaluation"##,
        desc: r##"Structured scholarly-work evaluation for papers, proposals, literature reviews, methods se"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"skill-comply"##,
        desc: r##"Visualize whether skills, rules, and agent definitions are actually followed — auto-genera"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"skill-scout"##,
        desc: r##"Search existing local, marketplace, GitHub, and web skill sources before creating a new sk"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"skill-stocktake"##,
        desc: r##""Use when auditing Claude skills and commands for quality. Supports Quick Scan (changed sk"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"tdd-workflow"##,
        desc: r##"Use this skill when writing new features, fixing bugs, or refactoring code. Enforces test-"##,
        category: r##"优化清理者"##,
    },
    SeedSkill {
        name: r##"article-writing"##,
        desc: r##"Write articles, guides, blog posts, tutorials, newsletter issues, and other long-form cont"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"blender-motion-state-inspection"##,
        desc: r##"Use this skill when inspecting Blender characters, rigs, poses, animation retargeting, gro"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"brand-discovery"##,
        desc: r##">-"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"brand-voice"##,
        desc: r##"Build a source-derived writing style profile from real posts, essays, launch notes, docs,"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"competitive-platform-analysis"##,
        desc: r##">-"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"competitive-report-structure"##,
        desc: r##">-"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"connections-optimizer"##,
        desc: r##"Reorganize the user's X and LinkedIn network with review-first pruning, add/follow recomme"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"content-engine"##,
        desc: r##"Create platform-native content systems for X, LinkedIn, TikTok, YouTube, newsletters, and"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"crosspost"##,
        desc: r##"Multi-platform content distribution across X, LinkedIn, Threads, and Bluesky. Adapts conte"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"email-ops"##,
        desc: r##"Evidence-first mailbox triage, drafting, send verification, and sent-mail-safe follow-up w"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"fal-ai-media"##,
        desc: r##"Unified media generation via fal.ai MCP — image, video, and audio. Covers text-to-image (N"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"frontend-slides"##,
        desc: r##"Create stunning, animation-rich HTML presentations from scratch or by converting PowerPoin"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"google-workspace-ops"##,
        desc: r##"Operate across Google Drive, Docs, Sheets, and Slides as one workflow surface for plans, t"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"investor-materials"##,
        desc: r##"Create and update pitch decks, one-pagers, investor memos, accelerator applications, finan"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"investor-outreach"##,
        desc: r##"Draft cold emails, warm intro blurbs, follow-ups, update emails, and investor communicatio"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"ios-icon-gen"##,
        desc: r##"Generate iOS app icons as PNG imagesets for Xcode asset catalogs from SF Symbols (5000+ Ap"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"ito-basket-compare"##,
        desc: r##"Compare Itô prediction-market baskets against a user's knowledge base, portfolio notes, fi"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"ito-data-atlas-agent"##,
        desc: r##"Design background Data Atlas style agents for Itô basket research, market discovery, param"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"ito-market-intelligence"##,
        desc: r##"Research prediction-market events, venues, underliers, liquidity, and news context for Itô"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"ito-trade-planner"##,
        desc: r##"Build a non-advisory prediction-market trade planning worksheet for Itô or venue workflows"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"lead-intelligence"##,
        desc: r##"AI-native lead intelligence and outreach pipeline. Replaces Apollo, Clay, and ZoomInfo wit"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"manim-video"##,
        desc: r##"Build reusable Manim explainers for technical concepts, graphs, system diagrams, and produ"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"market-research"##,
        desc: r##"Conduct market research, competitive analysis, investor due diligence, and industry intell"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"marketing-campaign"##,
        desc: r##"End-to-end marketing campaign planning and execution. Covers audience research, positionin"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"messages-ops"##,
        desc: r##"Evidence-first live messaging workflow for ECC. Use when the user wants to read texts or D"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"ml-adoption-playbook"##,
        desc: r##"End-to-end methodology for AI agents and software engineers to add machine learning algori"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"openclaw-persona-forge"##,
        desc: r##""为 OpenClaw AI Agent 锻造完整的龙虾灵魂方案。根据用户偏好或随机抽卡， 输出身份定位、灵魂描述(SOUL.md)、角色化底线规则、名字和头像生图提示词。 如当前"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"prediction-market-oracle-research"##,
        desc: r##"Research prediction markets as data sources or oracle signals for products, agents, dashbo"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"recsys-pipeline-architect"##,
        desc: r##"Design composable recommendation, ranking, and feed pipelines using the six-stage Source→H"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"remotion-video-creation"##,
        desc: r##"Best practices for Remotion - Video creation in React. 29 domain-specific rules covering 3"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"seo"##,
        desc: r##"Audit, plan, and implement SEO improvements across technical SEO, on-page optimization, st"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"social-graph-ranker"##,
        desc: r##"Weighted social-graph ranking for warm intro discovery, bridge scoring, and network gap an"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"social-publisher"##,
        desc: r##"Agent-driven scheduling and publishing of social media posts across 13 platforms via Socia"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"taste"##,
        desc: r##"A creative-direction (taste) layer for music videos and short-form edits in the angelcore"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"ui-demo"##,
        desc: r##"Record polished UI demo videos using Playwright. Use when the user asks to create a demo,"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"video-editing"##,
        desc: r##"AI-assisted video editing workflows for cutting, structuring, and augmenting real footage."##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"videodb"##,
        desc: r##"See, Understand, Act on video and audio. See- ingest from local files, URLs, RTSP/live fee"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"x-api"##,
        desc: r##"X/Twitter API integration for posting tweets, threads, reading timelines, search, and anal"##,
        category: r##"增长者"##,
    },
    SeedSkill {
        name: r##"agent-architecture-audit"##,
        desc: r##"Full-stack diagnostic for agent and LLM applications. Audits the 12-layer agent stack for"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"agent-payment-x402"##,
        desc: r##"Add x402 payment execution to AI agents with per-task budgets, spending controls, and non-"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"agentic-engineering"##,
        desc: r##"Operate as an agentic engineer using eval-first execution, decomposition, and cost-aware m"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"automation-audit-ops"##,
        desc: r##"Evidence-first automation inventory and overlap audit workflow for ECC. Use when the user"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"canary-watch"##,
        desc: r##"Use this skill to monitor and verify a deployed URL after releases — checks HTTP endpoints"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"carrier-relationship-management"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"cisco-ios-patterns"##,
        desc: r##"Cisco IOS and IOS-XE review patterns for show commands, config hierarchy, wildcard masks,"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"ck"##,
        desc: r##"Persistent per-project memory for Claude Code. Auto-loads project context on session start"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"codehealth-mcp"##,
        desc: r##"Real-time structural Code Health via CodeScene MCP — review before edits, verify score del"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"config-gc"##,
        desc: r##"Garbage collection for your Claude Code configuration. Periodically scans ~/.claude (skill"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"configure-ecc"##,
        desc: r##"Interactive installer for Everything Claude Code — guides users through selecting and inst"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"context-budget"##,
        desc: r##"Audits Claude Code context window consumption across agents, skills, MCP servers, and rule"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"cost-aware-llm-pipeline"##,
        desc: r##"Cost optimization patterns for LLM API usage — model routing by task complexity, budget tr"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"cost-tracking"##,
        desc: r##"Track and report Claude Code token usage, spending, and budgets from a local cost-tracking"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"customer-billing-ops"##,
        desc: r##"Operate customer billing workflows such as subscriptions, refunds, churn triage, billing-p"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"customs-trade-compliance"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"dart-flutter-patterns"##,
        desc: r##"Production-ready Dart and Flutter patterns covering null safety, immutable state, async co"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"defi-amm-security"##,
        desc: r##"Security checklist for Solidity AMM contracts, liquidity pools, and swap flows. Covers ree"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"django-security"##,
        desc: r##"Django security best practices, authentication, authorization, CSRF protection, SQL inject"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"django-verification"##,
        desc: r##""Verification loop for Django projects: migrations, linting, tests with coverage, security"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"docker-patterns"##,
        desc: r##"Docker and Docker Compose patterns for local development, container security, networking,"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"ecc-tools-cost-audit"##,
        desc: r##"Evidence-first ECC Tools burn and billing audit workflow. Use when investigating runaway P"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"energy-procurement"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"enterprise-agent-ops"##,
        desc: r##"Operate long-lived agent workloads with observability, security boundaries, and lifecycle"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"evm-token-decimals"##,
        desc: r##"Prevent silent decimal mismatch bugs across EVM chains. Covers runtime decimal lookup, cha"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"finance-billing-ops"##,
        desc: r##"Evidence-first revenue, pricing, refunds, team-billing, and billing-model truth workflow f"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"flox-environments"##,
        desc: r##""Create reproducible, cross-platform (macOS/Linux) development environments with Flox, a d"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"gateguard"##,
        desc: r##"Fact-forcing gate that blocks Edit/Write/Bash (including MultiEdit) and demands concrete i"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"github-ops"##,
        desc: r##"GitHub repository operations, automation, and management. Issue triage, PR management, CI/"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"healthcare-emr-patterns"##,
        desc: r##"EMR/EHR development patterns for healthcare applications. Clinical safety, encounter workf"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"healthcare-eval-harness"##,
        desc: r##"Patient safety evaluation harness for healthcare application deployments. Automated test s"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"healthcare-phi-compliance"##,
        desc: r##"Protected Health Information (PHI) and Personally Identifiable Information (PII) complianc"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"hipaa-compliance"##,
        desc: r##"HIPAA-specific entrypoint for healthcare privacy and security work. Use when a task is exp"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"homelab-network-readiness"##,
        desc: r##"Readiness checklist for homelab VLAN segmentation, local DNS filtering, and WireGuard-styl"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"homelab-network-setup"##,
        desc: r##"Practical home and homelab network planning for gateways, switches, access points, IP rang"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"homelab-pihole-dns"##,
        desc: r##"Pi-hole installation, blocklist management, DNS-over-HTTPS setup, DHCP integration, local"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"homelab-vlan-segmentation"##,
        desc: r##"Segmenting home networks into VLANs for IoT, guest, trusted, and server traffic using UniF"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"homelab-wireguard-vpn"##,
        desc: r##"WireGuard VPN server setup, peer configuration, key generation, split tunneling vs full tu"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"hookify-rules"##,
        desc: r##"This skill should be used when the user asks to create a hookify rule, write a hook rule,"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"inventory-demand-planning"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"laravel-security"##,
        desc: r##"Laravel security best practices — authentication, authorization, Eloquent safety, CSRF, XS"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"llm-trading-agent-security"##,
        desc: r##"Security patterns for autonomous trading agents with wallet or transaction authority. Cove"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"logistics-exception-management"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"nanoclaw-repl"##,
        desc: r##"Operate and extend NanoClaw v2, ECC's zero-dependency session-aware REPL built on claude -"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"netmiko-ssh-automation"##,
        desc: r##"Safe Python Netmiko patterns for read-only collection, bounded batch SSH, TextFSM parsing,"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"network-bgp-diagnostics"##,
        desc: r##"Diagnostics-only BGP troubleshooting patterns for neighbor state, route exchange, prefix p"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"network-config-validation"##,
        desc: r##"Pre-deployment checks for router and switch configuration, including dangerous commands, d"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"network-interface-health"##,
        desc: r##"Diagnose interface errors, drops, CRCs, duplex mismatches, flapping, speed negotiation iss"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"nodejs-keccak256"##,
        desc: r##"Prevent Ethereum hashing bugs in JavaScript and TypeScript. Node's sha3-256 is NIST SHA3,"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"nutrient-document-processing"##,
        desc: r##"Process, convert, OCR, extract, redact, sign, and fill documents using the Nutrient DWS AP"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"nuxt4-patterns"##,
        desc: r##"Nuxt 4 app patterns for hydration safety, performance, route rules, lazy loading, and SSR-"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"opensource-pipeline"##,
        desc: r##""Open-source pipeline: fork, sanitize, and package private projects for safe public releas"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"perl-security"##,
        desc: r##"Comprehensive Perl security covering taint mode, input validation, safe process execution,"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"postgres-patterns"##,
        desc: r##"PostgreSQL database patterns for query optimization, schema design, indexing, and security"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"prediction-market-risk-review"##,
        desc: r##"Review prediction-market, basket, oracle, and trading-agent workflows for compliance, safe"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"production-audit"##,
        desc: r##"Local-evidence production readiness audit for shipped apps, pre-launch reviews, post-merge"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"production-scheduling"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"project-flow-ops"##,
        desc: r##"Operate execution flow across GitHub and Linear by triaging issues and pull requests, link"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"quality-nonconformance"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"quarkus-security"##,
        desc: r##"Quarkus Security best practices for authentication, authorization, JWT/OIDC, RBAC, input v"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"ralphinho-rfc-pipeline"##,
        desc: r##"RFC-driven multi-agent DAG execution pattern with quality gates, merge queues, and work un"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"react-testing"##,
        desc: r##"React component testing with React Testing Library, Vitest/Jest, MSW for network mocking,"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"returns-reverse-logistics"##,
        desc: r##">"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"safety-guard"##,
        desc: r##"Use this skill to prevent destructive operations when working on production systems or run"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"security-bounty-hunter"##,
        desc: r##"Hunt for exploitable, bounty-worthy security issues in repositories. Focuses on remotely r"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"security-review"##,
        desc: r##"Use this skill when adding authentication, handling user input, working with secrets, crea"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"security-scan"##,
        desc: r##"Scan your Claude Code configuration (.claude/ directory) for security vulnerabilities, mis"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"springboot-security"##,
        desc: r##"Spring Security best practices for authn/authz, validation, CSRF, secrets, headers, rate l"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"strategic-compact"##,
        desc: r##"Suggests manual context compaction at logical intervals to preserve context through task p"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"swift-protocol-di-testing"##,
        desc: r##"Protocol-based dependency injection for testable Swift code — mock file system, network, a"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"terminal-ops"##,
        desc: r##"Evidence-first repo execution workflow for ECC. Use when the user wants a command run, a r"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"token-budget-advisor"##,
        desc: r##">-"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"uncloud"##,
        desc: r##"Use when managing an Uncloud cluster — deploying services, configuring Caddy ingress, addi"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"unified-notifications-ops"##,
        desc: r##"Operate notifications as one ECC-native workflow across GitHub, Linear, desktop alerts, ho"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"verification-loop"##,
        desc: r##""A comprehensive verification system for Claude Code sessions.""##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"visa-doc-translate"##,
        desc: r##"Translate visa application documents (images) to English and create a bilingual PDF with o"##,
        category: r##"维护者"##,
    },
    SeedSkill {
        name: r##"workspace-surface-audit"##,
        desc: r##"Audit the active repo, MCP servers, plugins, connectors, env surfaces, and harness setup,"##,
        category: r##"维护者"##,
    },
];

const ECC_AGENTS: &[SeedAgent] = &[
    SeedAgent {
        name: r##"architect"##,
        desc: r##"Software architecture specialist for system design, scalability, and technical decision-making. Use PROACTI"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"code-explorer"##,
        desc: r##"Deeply analyzes existing codebase features by tracing execution paths, mapping architecture layers, and"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"docs-lookup"##,
        desc: r##"When the user asks how to use a library, framework, or API or needs up-to-date code examples, use Context"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"gan-planner"##,
        desc: r##""GAN Harness — Planner agent. Expands a one-line prompt into a full product specification with features,"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"planner"##,
        desc: r##"Expert planning specialist for complex features and refactoring. Use PROACTIVELY when users request feature i"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"spec-miner"##,
        desc: r##"Extracts behavioral specs from existing codebases for OpenSpec. Produces flat Requirement and Invariant bl"##,
        category: r##"原型构建"##,
    },
    SeedAgent {
        name: r##"build-error-resolver"##,
        desc: r##"Build and TypeScript error resolution specialist. Use PROACTIVELY when build fails or type error"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"code-architect"##,
        desc: r##"Designs feature architectures by analyzing existing codebase patterns and conventions, then providing"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"cpp-build-resolver"##,
        desc: r##"C++ build, CMake, and compilation error resolution specialist. Fixes build errors, linker issues,"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"dart-build-resolver"##,
        desc: r##"Dart/Flutter build, analysis, and dependency error resolution specialist. Fixes `dart analyze` er"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"django-build-resolver"##,
        desc: r##"Django/Python build, migration, and dependency error resolution specialist. Fixes pip/Poetry er"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"doc-updater"##,
        desc: r##"Documentation and codemap specialist. Use PROACTIVELY for updating codemaps and documentation. Runs /upda"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"e2e-runner"##,
        desc: r##"End-to-end testing specialist using Vercel Agent Browser (preferred) with Playwright fallback. Use PROACTI"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"gan-generator"##,
        desc: r##""GAN Harness — Generator agent. Implements features according to the spec, reads evaluator feedback, an"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"go-build-resolver"##,
        desc: r##"Go build, vet, and compilation error resolution specialist. Fixes build errors, go vet issues, and"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"java-build-resolver"##,
        desc: r##"Java/Maven/Gradle build, compilation, and dependency error resolution specialist. Automatically d"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"kotlin-build-resolver"##,
        desc: r##"Kotlin/Gradle build, compilation, and dependency error resolution specialist. Fixes build error"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"loop-operator"##,
        desc: r##"Operate autonomous agent loops, monitor progress, and intervene safely when loops stall."##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"pytorch-build-resolver"##,
        desc: r##"PyTorch runtime, CUDA, and training error resolution specialist. Fixes tensor shape mismatches"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"react-build-resolver"##,
        desc: r##"Diagnose and fix React build failures across Vite, webpack, Next.js, CRA, Parcel, esbuild, and B"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"rust-build-resolver"##,
        desc: r##"Rust build, compilation, and dependency error resolution specialist. Fixes cargo build errors, bo"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"swift-build-resolver"##,
        desc: r##"Swift/Xcode build, compilation, and dependency error resolution specialist. Fixes swift build er"##,
        category: r##"Builder"##,
    },
    SeedAgent {
        name: r##"a11y-architect"##,
        desc: r##"Accessibility Architect specializing in WCAG 2.2 compliance for Web and Native platforms. Use PROACTIV"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"agent-evaluator"##,
        desc: r##"Evaluates agent output against 5-axis quality rubric (accuracy, completeness, clarity, actionability,"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"code-reviewer"##,
        desc: r##"Expert code review specialist. Proactively reviews code for quality, security, and maintainability. Use"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"code-simplifier"##,
        desc: r##"Simplifies and refines code for clarity, consistency, and maintainability while preserving behavior."##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"comment-analyzer"##,
        desc: r##"Analyze code comments for accuracy, completeness, maintainability, and comment rot risk."##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"conversation-analyzer"##,
        desc: r##"Use this agent when analyzing conversation transcripts to find behaviors worth preventing with"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"flutter-reviewer"##,
        desc: r##"Flutter and Dart code reviewer. Reviews Flutter code for widget best practices, state management pat"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"gan-evaluator"##,
        desc: r##""GAN Harness — Evaluator agent. Tests the live running application via Playwright, scores against rubri"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"go-reviewer"##,
        desc: r##"Expert Go code reviewer specializing in idiomatic Go, concurrency patterns, error handling, and performan"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"harmonyos-app-resolver"##,
        desc: r##"HarmonyOS application development expert specializing in ArkTS and ArkUI. Reviews code for V2"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"harness-optimizer"##,
        desc: r##"Analyze and improve the local agent harness configuration for reliability, cost, and throughput."##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"java-reviewer"##,
        desc: r##"Expert Java code reviewer for Spring Boot and Quarkus projects. Automatically detects the framework and"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"mle-reviewer"##,
        desc: r##"Production machine-learning engineering reviewer for data contracts, feature pipelines, training reprodu"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"performance-optimizer"##,
        desc: r##"Performance analysis and optimization specialist. Use PROACTIVELY for identifying bottlenecks,"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"pr-test-analyzer"##,
        desc: r##"Review pull request test coverage quality and completeness, with emphasis on behavioral coverage and"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"react-reviewer"##,
        desc: r##"Expert React/JSX code reviewer specializing in hook correctness, render performance, server/client com"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"refactor-cleaner"##,
        desc: r##"Dead code cleanup and consolidation specialist. Use PROACTIVELY for removing unused code, duplicates"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"rust-reviewer"##,
        desc: r##"Expert Rust code reviewer specializing in ownership, lifetimes, error handling, unsafe usage, and idiom"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"silent-failure-hunter"##,
        desc: r##"Review code for silent failures, swallowed errors, bad fallbacks, and missing error propagation"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"swift-reviewer"##,
        desc: r##"Expert Swift code reviewer specializing in protocol-oriented design, value semantics, ARC memory manag"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"tdd-guide"##,
        desc: r##"Test-Driven Development specialist enforcing write-tests-first methodology. Use PROACTIVELY when writing ne"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"type-design-analyzer"##,
        desc: r##"Analyze type design for encapsulation, invariant expression, usefulness, and enforcement."##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"vue-reviewer"##,
        desc: r##"Expert Vue.js code reviewer specializing in Composition API correctness, reactivity pitfalls, component"##,
        category: r##"优化清理者"##,
    },
    SeedAgent {
        name: r##"chief-of-staff"##,
        desc: r##"Personal communication chief of staff that triages email, Slack, LINE, and Messenger. Classifies messa"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"marketing-agent"##,
        desc: r##"Marketing strategist and copywriter for campaign planning, audience research, positioning, copy creat"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"seo-specialist"##,
        desc: r##"SEO specialist for technical SEO audits, on-page optimization, structured data, Core Web Vitals, and c"##,
        category: r##"增长者"##,
    },
    SeedAgent {
        name: r##"cpp-reviewer"##,
        desc: r##"Expert C++ code reviewer specializing in memory safety, modern C++ idioms, concurrency, and performance."##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"csharp-reviewer"##,
        desc: r##"Expert C# code reviewer specializing in .NET conventions, async patterns, security, nullable referenc"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"database-reviewer"##,
        desc: r##"PostgreSQL database specialist for query optimization, schema design, security, and performance. Us"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"django-reviewer"##,
        desc: r##"Expert Django code reviewer specializing in ORM correctness, DRF patterns, migration safety, security"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"fastapi-reviewer"##,
        desc: r##"Reviews FastAPI applications for async correctness, dependency injection, Pydantic schemas, security"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"fsharp-reviewer"##,
        desc: r##"Expert F# code reviewer specializing in functional idioms, type safety, pattern matching, computation"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"healthcare-reviewer"##,
        desc: r##"Reviews healthcare application code for clinical safety, CDSS accuracy, PHI compliance, and medic"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"homelab-architect"##,
        desc: r##"Designs home and small-lab network plans from hardware inventory, goals, and operator experience le"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"kotlin-reviewer"##,
        desc: r##"Kotlin and Android/KMP code reviewer. Reviews Kotlin code for idiomatic patterns, coroutine safety, C"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"network-architect"##,
        desc: r##"Designs enterprise or multi-site network architecture from requirements, using existing network ski"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"network-config-reviewer"##,
        desc: r##"Reviews router and switch configurations for security, correctness, stale references, risky c"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"network-troubleshooter"##,
        desc: r##"Diagnoses network connectivity, routing, DNS, interface, and policy symptoms with a read-only"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"opensource-forker"##,
        desc: r##"Fork any project for open-sourcing. Copies files, strips secrets and credentials (20+ patterns), re"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"opensource-packager"##,
        desc: r##"Generate complete open-source packaging for a sanitized project. Produces CLAUDE.md, setup.sh, RE"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"opensource-sanitizer"##,
        desc: r##"Verify an open-source fork is fully sanitized before release. Scans for leaked secrets, PII, int"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"php-reviewer"##,
        desc: r##"Expert PHP code reviewer specializing in PSR-12 compliance, PHP type system, Eloquent ORM patterns, secu"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"python-reviewer"##,
        desc: r##"Expert Python code reviewer specializing in PEP 8 compliance, Pythonic idioms, type hints, security,"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"security-reviewer"##,
        desc: r##"Security vulnerability detection and remediation specialist. Use PROACTIVELY after writing code tha"##,
        category: r##"维护者"##,
    },
    SeedAgent {
        name: r##"typescript-reviewer"##,
        desc: r##"Expert TypeScript/JavaScript code reviewer specializing in type safety, async correctness, Node/w"##,
        category: r##"维护者"##,
    },
];

const ECC_WORKFLOWS: &[SeedWorkflow] = &[
    SeedWorkflow {
        name: r##"ecc-guide"##,
        desc: r##"Navigate ECC's current agents, skills, commands, hooks, install profiles, and docs from the live repository"##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"epic-claim"##,
        desc: r##"Claim an epic issue, stamp coordination state, and sync local ownership."##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"epic-decompose"##,
        desc: r##"Break an epic into task children without creating task branches."##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"epic-publish"##,
        desc: r##"Publish a validated epic update back to the issue and local cache."##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"epic-sync"##,
        desc: r##"Sync epic issue bodies, labels, and local coordination snapshots from GitHub."##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"epic-unblock"##,
        desc: r##"Sweep blocked epic issues and reopen anything whose dependencies are closed."##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"epic-validate"##,
        desc: r##"Validate epic readiness, dependencies, and coordination policy."##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"jira"##,
        desc: r##"Retrieve a Jira ticket, analyze requirements, update status, or add comments. Uses the jira-integration skill an"##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"plan"##,
        desc: r##"Restate requirements, assess risks, and create step-by-step implementation plan. WAIT for user CONFIRM before to"##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"plan-prd"##,
        desc: r##""Generate a lean, problem-first PRD and hand off to /plan for implementation planning.""##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"project-init"##,
        desc: r##"Detect a project's stack and produce a dry-run ECC onboarding plan using the repository's install manife"##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"prp-prd"##,
        desc: r##""Interactive PRD generator - problem-first, hypothesis-driven product spec with back-and-forth questioning""##,
        stage_ref: 1,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"aside"##,
        desc: r##"Answer a quick side question without interrupting or losing context from the current task. Resume work automati"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"build-fix"##,
        desc: r##"Detect the project build system and incrementally fix build/type errors with minimal safe changes."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"checkpoint"##,
        desc: r##"Create, verify, or list workflow checkpoints after running verification checks."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"cpp-build"##,
        desc: r##"Fix C++ build errors, CMake issues, and linker problems incrementally. Invokes the cpp-build-resolver agent"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"cpp-test"##,
        desc: r##"Enforce TDD workflow for C++. Write GoogleTest tests first, then implement. Verify coverage with gcov/lcov."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"feature-dev"##,
        desc: r##"Guided feature development with codebase understanding and architecture focus"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"flutter-build"##,
        desc: r##"Fix Dart analyzer errors and Flutter build failures incrementally. Invokes the dart-build-resolver agen"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"flutter-test"##,
        desc: r##"Run Flutter/Dart tests, report failures, and incrementally fix test issues. Covers unit, widget, golden,"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"gan-build"##,
        desc: r##"Run a generator/evaluator build loop for implementation tasks with bounded iterations and scoring."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"go-build"##,
        desc: r##"Fix Go build errors, go vet warnings, and linter issues incrementally. Invokes the go-build-resolver agent f"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"go-test"##,
        desc: r##"Enforce TDD workflow for Go. Write table-driven tests first, then implement. Verify 80%+ coverage with go tes"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"gradle-build"##,
        desc: r##"Fix Gradle build errors for Android and KMP projects"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"kotlin-build"##,
        desc: r##"Fix Kotlin/Gradle build errors, compiler warnings, and dependency issues incrementally. Invokes the kotl"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"kotlin-test"##,
        desc: r##"Enforce TDD workflow for Kotlin. Write Kotest tests first, then implement. Verify 80%+ coverage with Kove"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"loop-status"##,
        desc: r##"Inspect active loop state, progress, failure signals, and recommended intervention."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"multi-backend"##,
        desc: r##"Run a backend-focused multi-model workflow for APIs, algorithms, data, and business logic."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"multi-execute"##,
        desc: r##"Execute a multi-model implementation plan while preserving Claude as the only filesystem writer."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"multi-frontend"##,
        desc: r##"Run a frontend-focused multi-model workflow for components, layouts, animation, and UI polish."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"multi-plan"##,
        desc: r##"Create a multi-model implementation plan without modifying production code."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"multi-workflow"##,
        desc: r##"Run a full multi-model development workflow with research, planning, execution, optimization, and revi"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"orch-add-feature"##,
        desc: r##"Orchestrate building a brand-new feature end to end — research, plan, TDD, review, gated commit. Wra"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"orch-build-mvp"##,
        desc: r##"Orchestrate bootstrapping a working MVP from a design/spec doc — ingest, slice, scaffold, TDD, review,"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"orch-change-feature"##,
        desc: r##"Orchestrate altering an existing, working feature to new desired behavior — update tests to the n"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"orch-fix-defect"##,
        desc: r##"Orchestrate fixing a bug — reproduce it as a failing regression test, fix to green, review, gated com"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"orch-refine-code"##,
        desc: r##"Orchestrate a behavior-preserving refactor — confirm tests green, restructure without changing behav"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"pr"##,
        desc: r##""Create a GitHub PR from current branch with unpushed commits — discovers templates, analyzes changes, pushes""##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"prp-commit"##,
        desc: r##""Quick commit with natural language file targeting — describe what to commit in plain English""##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"prp-implement"##,
        desc: r##"Execute an implementation plan with rigorous validation loops"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"prp-plan"##,
        desc: r##"Create comprehensive feature implementation plan with codebase analysis and pattern extraction"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"prp-pr"##,
        desc: r##""Create a GitHub PR from current branch with unpushed commits — discovers templates, analyzes changes, pushes""##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"react-build"##,
        desc: r##"Fix React build failures (Vite, webpack, Next.js, CRA, Parcel, esbuild, Bun) incrementally — JSX/TSX comp"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"rust-build"##,
        desc: r##"Fix Rust build errors, borrow checker issues, and dependency problems incrementally. Invokes the rust-buil"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"rust-test"##,
        desc: r##"Enforce TDD workflow for Rust. Write tests first, then implement. Verify 80%+ coverage with cargo-llvm-cov."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"setup-pm"##,
        desc: r##"Configure your preferred package manager (npm/pnpm/yarn/bun)"##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"update-codemaps"##,
        desc: r##"Scan project structure and generate token-lean architecture codemaps."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"update-docs"##,
        desc: r##"Sync documentation from source-of-truth files such as scripts, schemas, routes, and exports."##,
        stage_ref: 2,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"code-review"##,
        desc: r##"Code review — local uncommitted changes or GitHub PR (pass PR number/URL for PR mode)"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"epic-review"##,
        desc: r##"Mark epic review requested, approved, or changes requested."##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"evolve"##,
        desc: r##"Analyze instincts and suggest or generate evolved structures"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"fastapi-review"##,
        desc: r##"Review a FastAPI application for architecture, async correctness, dependency injection, Pydantic schem"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"flutter-review"##,
        desc: r##"Review Flutter/Dart code for idiomatic patterns, widget best practices, state management, performance,"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"gan-design"##,
        desc: r##"Run a generator/evaluator design loop for frontend or visual work with bounded iterations and scoring."##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"harness-audit"##,
        desc: r##"Run a deterministic repository harness audit and return a prioritized scorecard."##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"instinct-export"##,
        desc: r##"Export instincts from project/global scope to a file"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"instinct-import"##,
        desc: r##"Import instincts from file or URL into project/global scope"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"instinct-status"##,
        desc: r##"Show learned instincts (project + global) with confidence"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"learn"##,
        desc: r##"Extract reusable patterns from the current session and save them as candidate skills or guidance."##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"learn-eval"##,
        desc: r##""Extract reusable patterns from the session, self-evaluate quality before saving, and determine the right"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"prune"##,
        desc: r##"Delete pending instincts older than 30 days that were never promoted"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"quality-gate"##,
        desc: r##"Run the ECC formatter quality gate for a single file and report remediation steps."##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"react-review"##,
        desc: r##"Comprehensive React/JSX code review for hook correctness, render performance, server/client component bo"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"react-test"##,
        desc: r##"Enforce TDD workflow for React. Write React Testing Library tests first (behavior-focused, accessibility-f"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"refactor-clean"##,
        desc: r##"Safely identify and remove dead code with verification after each change."##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"review-pr"##,
        desc: r##"Comprehensive PR review using specialized agents"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"rust-review"##,
        desc: r##"Comprehensive Rust code review for ownership, lifetimes, error handling, unsafe usage, and idiomatic patt"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"santa-loop"##,
        desc: r##"Adversarial dual-review convergence loop — two independent model reviewers must both approve before code s"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"skill-create"##,
        desc: r##"Analyze local git history to extract coding patterns and generate SKILL.md files. Local version of the S"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"skill-health"##,
        desc: r##"Show skill portfolio health dashboard with charts and analytics"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"test-coverage"##,
        desc: r##"Analyze coverage, identify gaps, and generate missing tests toward the target threshold."##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"vue-review"##,
        desc: r##"Comprehensive Vue.js code review for Composition API correctness, reactivity, composable patterns, templat"##,
        stage_ref: 3,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"marketing-campaign"##,
        desc: r##"Plan and execute a full marketing campaign. Accepts a product brief and returns positioning, landi"##,
        stage_ref: 4,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"projects"##,
        desc: r##"List known projects and their instinct statistics"##,
        stage_ref: 4,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"promote"##,
        desc: r##"Promote project-scoped instincts to global scope"##,
        stage_ref: 4,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"auto-update"##,
        desc: r##"Pull the latest ECC repo changes and reinstall the current managed targets."##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"cost-report"##,
        desc: r##"Generate a local Claude Code cost report from a cost-tracker SQLite database."##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"cpp-review"##,
        desc: r##"Comprehensive C++ code review for memory safety, modern C++ idioms, concurrency, and security. Invokes the"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"go-review"##,
        desc: r##"Comprehensive Go code review for idiomatic patterns, concurrency safety, error handling, and security. Invo"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"hookify"##,
        desc: r##"Create hooks to prevent unwanted behaviors from conversation analysis or explicit instructions"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"hookify-configure"##,
        desc: r##"Enable or disable hookify rules interactively"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"hookify-help"##,
        desc: r##"Get help with the hookify system"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"hookify-list"##,
        desc: r##"List all configured hookify rules"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"kotlin-review"##,
        desc: r##"Comprehensive Kotlin code review for idiomatic patterns, null safety, coroutine safety, and security. I"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"loop-start"##,
        desc: r##"Start a managed autonomous loop pattern with safety defaults and explicit stop conditions."##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"model-route"##,
        desc: r##"Recommend the best model tier for the current task based on complexity, risk, and budget."##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"pm2"##,
        desc: r##"Analyze a project and generate PM2 service commands for detected frontend, backend, or database services."##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"python-review"##,
        desc: r##"Comprehensive Python code review for PEP 8 compliance, type hints, security, and Pythonic idioms. Invok"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"resume-session"##,
        desc: r##"Load the most recent session file from ~/.claude/session-data/ and resume work with full context from"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"save-session"##,
        desc: r##"Save current session state to a dated file in ~/.claude/session-data/ so work can be resumed in a future"##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"security-scan"##,
        desc: r##"Run AgentShield against agent, hook, MCP, permission, and secret surfaces."##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
    SeedWorkflow {
        name: r##"sessions"##,
        desc: r##"Manage Claude Code session history, aliases, and session metadata."##,
        stage_ref: 5,
        source: HubSource::Ecc,
    },
];

/// Seed the hub library from the real OMC/ECC catalogs if it's currently
/// empty. Called once at `Boot`; safe to call on every boot since it checks
/// first and is a no-op once seeded.
pub async fn seed_hub_if_empty(store: &dyn Store) -> Result<()> {
    if !store.list_skills().await?.is_empty()
        || !store.list_agents().await?.is_empty()
        || !store.list_workflow_specs().await?.is_empty()
    {
        return Ok(());
    }

    for s in OMC_SKILLS.iter() {
        store
            .create_skill(NewSkill {
                id: SkillId::new(),
                name: s.name.to_string(),
                maturity: Maturity::Mature,
                desc: s.desc.to_string(),
                category: s.category.to_string(),
                source: LibSource::Official,
                // Catalog *reference*: the full text lives in the source
                // repo — an empty body here is honest, not missing data.
                content: String::new(),
                project_id: None, // hub catalog seed — global, unchanged behavior
            })
            .await?;
    }
    for s in ECC_SKILLS.iter() {
        store
            .create_skill(NewSkill {
                id: SkillId::new(),
                name: s.name.to_string(),
                maturity: Maturity::Mature,
                desc: s.desc.to_string(),
                category: s.category.to_string(),
                source: LibSource::Official,
                content: String::new(),
                project_id: None,
            })
            .await?;
    }

    for a in OMC_AGENTS.iter() {
        store
            .create_agent(NewAgent {
                id: AgentId::new(),
                name: a.name.to_string(),
                role: format!("[{}] {}", a.category, a.desc),
                maturity: Maturity::Mature,
                skills: vec![],
                model: "claude-sonnet".to_string(),
                instructions: String::new(),
                project_id: None,
            })
            .await?;
    }
    for a in ECC_AGENTS.iter() {
        store
            .create_agent(NewAgent {
                id: AgentId::new(),
                name: a.name.to_string(),
                role: format!("[{}] {}", a.category, a.desc),
                maturity: Maturity::Mature,
                skills: vec![],
                model: "claude-sonnet".to_string(),
                instructions: String::new(),
                project_id: None,
            })
            .await?;
    }

    for w in ECC_WORKFLOWS.iter() {
        store
            .create_workflow_spec(NewWorkflowSpec {
                id: WorkflowId::new(),
                name: w.name.to_string(),
                kind: WorkflowKind::Static {
                    maturity: Maturity::Mature,
                    version: 1,
                    uses: 0,
                    scope: "跨项目复用".to_string(),
                    source: w.source,
                    trigger: Some(format!("/{}", w.name)),
                },
                prompt: w.desc.to_string(),
                goal: w.desc.to_string(),
                stage_ref: Some(w.stage_ref),
                phases: vec![w.name.to_string()],
                phase_prompts: vec![],
                agents: vec![],
                skills: vec![],
                loop_config: LoopConfig {
                    retries: 1,
                    max_iter: 3,
                },
                project_id: None,
            })
            .await?;
    }

    for kind in StageKind::ALL {
        let spec = stage_template_workflow(kind);
        store
            .create_workflow_spec(NewWorkflowSpec {
                id: spec.id,
                name: spec.name,
                kind: spec.kind,
                prompt: spec.prompt,
                goal: spec.goal,
                stage_ref: spec.stage_ref,
                phases: spec.phases,
                phase_prompts: spec.phase_prompts,
                agents: spec.agents,
                skills: spec.skills,
                loop_config: spec.loop_config,
                project_id: None,
            })
            .await?;
    }

    Ok(())
}

/// Seed the five stage-role agents and the stage working-method skills —
/// the *executable* entities behind 五角色真实执行, projected straight from
/// `bw_core::playbook` (instructions = the real preamble template, skill
/// content = the real injected body). By-name idempotent, and deliberately
/// separate from [`seed_hub_if_empty`]'s all-or-nothing gate: an existing,
/// already-seeded database still gains these on first boot after the 完整形态
/// upgrade — they're this app's own methodology, not external catalog data.
pub async fn seed_stage_entities_if_missing(store: &dyn Store) -> Result<()> {
    let have_skills: std::collections::HashSet<String> = store
        .list_skills()
        .await?
        .into_iter()
        .map(|s| s.name)
        .collect();
    let have_agents: std::collections::HashSet<String> = store
        .list_agents()
        .await?
        .into_iter()
        .map(|a| a.name)
        .collect();

    for kind in StageKind::ALL {
        for sk in bw_core::playbook::stage_skills(kind) {
            if have_skills.contains(sk.name) {
                continue;
            }
            store
                .create_skill(NewSkill {
                    id: SkillId::new(),
                    name: sk.name.to_string(),
                    // The methodology the app itself ships — Mature, 官方.
                    maturity: Maturity::Mature,
                    desc: sk.def.to_string(),
                    category: kind.label().to_string(),
                    source: LibSource::Official,
                    content: sk.content.to_string(),
                    // 五阶段方法论技能是全局共享的(见本函数文档:「这个 app
                    // 自己的方法论」),不是某个项目专属——project_id 留空。
                    project_id: None,
                })
                .await?;
        }
    }

    for (_kind, ra) in bw_core::playbook::role_agents() {
        if have_agents.contains(ra.name) {
            continue;
        }
        store
            .create_agent(NewAgent {
                id: AgentId::new(),
                name: ra.name.to_string(),
                role: ra.role,
                maturity: Maturity::Mature,
                skills: ra.skills,
                model: ra.model.to_string(),
                instructions: ra.instructions,
                // 同上:五角色是全局单例,不因这次践行的项目自有切片改变。
                project_id: None,
            })
            .await?;
    }
    Ok(())
}
