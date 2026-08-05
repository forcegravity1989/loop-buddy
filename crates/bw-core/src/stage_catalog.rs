//! 技能的五角色归属:静态归类表 + 归类出处枚举。
//!
//! 用户 2026-08-05 拍板「通用的 skill 应该被划分到对应的五角色中」。归类值
//! 有三条来源(优先级递增):**本表**(随包发行/vendored 的 65 件技能,进 git
//! 可 diff 可 review)、蒸馏派生(有 `distilled_from_issue` 的技能按出处 Issue
//! 的 stage)、UI 人工覆盖(`StageOrigin::Manual` 之后 Boot 不再回填)。
//!
//! **本模块守 bw-core 的零 IO / wasm32 约束**:只有 `const` 数据和纯函数,
//! 不读盘、不查库。Boot 侧的对账逻辑在 bw-app,不在这里。
//!
//! 设计依据见 `docs/superpowers/specs/2026-08-05-skill-five-role-classification-design.md`。

use crate::model::StageKind::{self, Build, Growth, Ops, Optimize, Prototype};
use serde::{Deserialize, Serialize};

/// 一条技能的**归类动作从哪来**——与 `skill_stage` 关联表的行数共同派生四态:
///
/// | 状态 | 判据 |
/// |---|---|
/// | 挂 N 个阶段 | 关联表 1..=4 行 |
/// | 全阶段通用 | 关联表 5 行 |
/// | 已判定「不属任何阶段」 | 0 行 且 `origin != Unclassified` |
/// | 未归类(Unknown) | 0 行 且 `origin == Unclassified` |
///
/// 第三、四档的区分是这个枚举存在的全部理由:`obsidian-vault`(笔记工具)、
/// `scaffold-exercises`(课程脚手架)这类技能**不是没人管**,是判过了、跟项目
/// 五阶段无关——把它们和真没人管的混成一格,就是仓里「无数据=Unknown,绝不
/// 假装」那条纪律的反面。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOrigin {
    /// 还没人归过类。DB 里是 `''`。
    #[default]
    Unclassified,
    /// 由 [`SKILL_STAGE_CATALOG`] 静态表回填。
    Table,
    /// 由蒸馏出处 Issue 的 stage 派生。
    Distilled,
    /// 人工在 SkillHub 里改过——Boot 的静态表回填从此整条跳过这件技能。
    Manual,
}

/// 「全阶段通用」的展开值。挂满五阶段 = 每个阶段的注入候选集都含它。
pub const ALL_FIVE: &[StageKind] = &[Prototype, Build, Optimize, Growth, Ops];

/// 空集 = **已判定**「不属任何阶段」(区别于「还没人判」——后者根本不在本表里)。
const NO_STAGE: &[StageKind] = &[];

/// 65 件随包发行 / vendored 技能的阶段归属正本。
///
/// 口径(指标类技能,spec §6.0):定=原型 / 用来打磨=优化 / 用来增长=运营 /
/// 用来守稳=运维(末项仅当该技能真涉及可观测性接入)。
///
/// 本地自建/蒸馏技能**不进本表**——它们是本机产物,归类走蒸馏派生或人工。
pub const SKILL_STAGE_CATALOG: &[(&str, &[StageKind])] = &[
    // ── bw-standard(8):五条方法论招牌技能单挂本阶段(它们是
    //    `playbook::stage_skills(kind)` 的正本,外扩会让「阶段=角色=方法论」
    //    的一一对应失效);指标/对标类按 §6.0 口径扩挂。
    ("evidence-first", &[Prototype]),
    ("competitive-analysis", &[Prototype, Growth]),
    ("north-star-discovery", &[Prototype, Optimize, Growth]),
    ("metrics-binding", &[Prototype, Optimize, Growth, Ops]),
    ("spec-to-tests", &[Build]),
    ("baseline-before-touch", &[Optimize]),
    ("fresh-eyes-funnel", &[Growth]),
    ("breaking-drill", &[Ops]),
    // ── mohit/pm-claude-skills(2):PR #74 升的基础技能,按 §6.0 同口径。
    ("metrics-framework", &[Prototype, Optimize, Growth]),
    ("metric-tree-builder", &[Prototype, Optimize, Growth]),
    // ── mattpocock-skills(41)
    ("ask-matt", ALL_FIVE),
    ("batch-grill-me", &[Prototype]),
    ("claude-handoff", ALL_FIVE),
    ("code-review", &[Build, Optimize]),
    ("codebase-design", &[Prototype, Optimize]),
    ("design-an-interface", &[Prototype]),
    ("diagnosing-bugs", &[Optimize, Ops]),
    ("domain-modeling", &[Prototype, Build]),
    ("edit-article", &[Growth]),
    ("git-guardrails-claude-code", &[Ops]),
    ("grill-me", &[Prototype]),
    ("grill-with-docs", &[Prototype]),
    ("grilling", &[Prototype]),
    ("handoff", ALL_FIVE),
    ("implement", &[Build]),
    ("improve-codebase-architecture", &[Optimize]),
    ("loop-me", &[Prototype]),
    ("migrate-to-shoehorn", &[Optimize]),
    ("obsidian-vault", NO_STAGE),
    ("prototype", &[Prototype]),
    ("qa", &[Optimize, Ops]),
    ("request-refactor-plan", &[Optimize]),
    ("research", &[Prototype, Growth]),
    ("resolving-merge-conflicts", &[Build]),
    ("scaffold-exercises", NO_STAGE),
    ("setup-matt-pocock-skills", NO_STAGE),
    ("setup-pre-commit", &[Build, Ops]),
    ("setup-ts-deep-modules", &[Optimize]),
    ("tdd", &[Build]),
    ("teach", NO_STAGE),
    ("to-questionnaire", &[Prototype]),
    ("to-spec", &[Prototype, Build]),
    ("to-tickets", &[Prototype, Build]),
    ("triage", &[Build, Ops]),
    ("ubiquitous-language", &[Prototype, Build]),
    ("wayfinder", &[Prototype, Build]),
    ("wizard", &[Ops]),
    ("writing-beats", &[Growth]),
    ("writing-fragments", &[Growth]),
    ("writing-great-skills", NO_STAGE),
    ("writing-shape", &[Growth]),
    // ── superpowers(14)
    ("brainstorming", &[Prototype]),
    ("dispatching-parallel-agents", ALL_FIVE),
    ("executing-plans", &[Build]),
    ("finishing-a-development-branch", &[Build]),
    ("receiving-code-review", &[Build, Optimize]),
    ("requesting-code-review", &[Build, Optimize]),
    ("subagent-driven-development", &[Build]),
    ("systematic-debugging", &[Build, Optimize, Ops]),
    ("test-driven-development", &[Build]),
    ("using-git-worktrees", &[Build]),
    ("using-superpowers", ALL_FIVE),
    ("verification-before-completion", ALL_FIVE),
    ("writing-plans", &[Prototype, Build]),
    ("writing-skills", NO_STAGE),
];

/// 查表。`None` = 这件技能不在静态表里(本地自建/外部新库)——诚实的「本表
/// 管不着」,**不是**「不属任何阶段」(后者在表里,值为空集)。
///
/// 线性扫描:65 条 × 每次 Boot 的技能数,量级微不足道,不值得为它引入 HashMap
/// (那会让本模块从 `const` 数据变成需要 lazy 初始化的东西)。
pub fn stages_for(name: &str) -> Option<&'static [StageKind]> {
    SKILL_STAGE_CATALOG
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, stages)| *stages)
}
