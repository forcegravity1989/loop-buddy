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
/// 第三、四档的区分是这个枚举存在的全部理由——「已判定不属任何阶段」这一档
/// 不是没人管,是判过了、跟项目五阶段无关,不该和真没人管的混成一格,这正是
/// 仓里「无数据=Unknown,绝不假装」那条纪律的反面。
///
/// **用户 2026-08-06 拍板**:静态表(`SKILL_STAGE_CATALOG`)里不再生产这一档——
/// 原来挂空集的 6 条(`obsidian-vault`/`scaffold-exercises` 等)已各自挂上至少
/// 一个阶段,`NO_STAGE` 常量随之删除。但这一档**状态本身没有废**:它仍可由
/// 人工在 SkillHub 里提交空集产生(`StageOrigin::Manual` + `skill_stage` 零
/// 关联行),四态判据(见下表)不变。
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
    /// 搬自已被删除的旧 `skill.stage_ref` 单值列。原始出处已不可考——它可能
    /// 是早年的 seed 回填,也可能来自本分支静态表覆盖不到的另一条产品线
    /// (真实案例:`metrics-render` 来自未合入本分支的 c14932d)。标成
    /// `Legacy` 而不是 `Table`,是因为本分支的静态表并没有为它背书;标成
    /// `Legacy` 而不是 `Manual`,是因为没人在 UI 里点过它。
    Legacy,
}

/// 「全阶段通用」的展开值。挂满五阶段 = 每个阶段的注入候选集都含它。
pub const ALL_FIVE: &[StageKind] = &[Prototype, Build, Optimize, Growth, Ops];

/// 65 件随包发行 / vendored 技能的阶段归属正本。
///
/// 口径(指标类技能,spec §6.0):定=原型 / 用来打磨=优化 / 用来增长=运营 /
/// 用来守稳=运维(末项仅当该技能真涉及可观测性接入)。
///
/// 本地自建/蒸馏技能**不进本表**——它们是本机产物,归类走蒸馏派生或人工。
pub const SKILL_STAGE_CATALOG: &[(&str, &[StageKind])] = &[
    // ── bw-standard(8):方法论招牌技能不是需要保护的特殊类,用户 2026-08-06
    //    拍板「它的区别只在于官方提供,还是用户上传的」——与其它技能同一把
    //    尺子(它实际在哪些阶段被用)判,按各自 desc 里写明的适用面扩挂;
    //    指标/对标类仍按 §6.0 口径。
    ("evidence-first", ALL_FIVE), // desc 自己写着「或任何需要引用事实与数字的产出」——本就是跨阶段品质
    ("competitive-analysis", &[Prototype, Growth]),
    ("north-star-discovery", &[Prototype, Optimize, Growth]),
    ("metrics-binding", &[Prototype, Optimize, Growth, Ops]),
    ("spec-to-tests", &[Build, Optimize]), // desc:「以及评审时逐条核对验收标准」——评审发生在优化
    ("baseline-before-touch", &[Optimize, Ops]), // desc:「优化段动手重构或调性能之前」+ 改线上东西前先量基线是 SRE 本职
    ("fresh-eyes-funnel", &[Optimize, Growth]),  // desc:「或对照验证上线改动」——后半句是优化
    ("breaking-drill", &[Build, Ops]),           // desc:「或发布前的稳健性检查」——发布前属构建
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
    ("obsidian-vault", &[Prototype, Growth]), // 组织笔记与知识:探索期积累素材(原型)+ 内容生产的素材库(运营)
    ("prototype", &[Prototype]),
    ("qa", &[Optimize, Ops]),
    ("request-refactor-plan", &[Optimize]),
    ("research", &[Prototype, Growth]),
    ("resolving-merge-conflicts", &[Build]),
    ("scaffold-exercises", &[Build, Growth]), // 按规格生成练习目录骨架(构建)+ 产出的是教学内容(运营)
    ("setup-matt-pocock-skills", &[Build]),   // 一次性把仓配置成可用形态,是搭项目基础形态
    ("setup-pre-commit", &[Build, Ops]),
    ("setup-ts-deep-modules", &[Optimize]),
    ("tdd", &[Build]),
    ("teach", &[Prototype]), // 学一个新概念是为了做决定——属于假设驱动探索的前置
    ("to-questionnaire", &[Prototype]),
    ("to-spec", &[Prototype, Build]),
    ("to-tickets", &[Prototype, Build]),
    ("triage", &[Build, Ops]),
    ("ubiquitous-language", &[Prototype, Build]),
    ("wayfinder", &[Prototype, Build]),
    ("wizard", &[Ops]),
    ("writing-beats", &[Growth]),
    ("writing-fragments", &[Growth]),
    ("writing-great-skills", &[Optimize]), // 写技能的参考:把做过的事提炼得更简,是求简
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
    ("writing-skills", &[Build, Optimize]), // 创建/编辑/验证技能并部署:造出来(构建)+ 提炼求简(优化)
];

/// 查表。`None` = 这件技能不在静态表里(本地自建/外部新库)——诚实的「本表
/// 管不着」,**不是**「不属任何阶段」(后者会在表里、值为空集;2026-08-06
/// 拍板后本表当前 65 条均已挂至少一个阶段,但空集仍是这个类型合法能表达
/// 的值,人工在 UI 里仍可产生等价语义——见 [`StageOrigin`] 上的说明)。
///
/// 线性扫描:65 条 × 每次 Boot 的技能数,量级微不足道,不值得为它引入 HashMap
/// (那会让本模块从 `const` 数据变成需要 lazy 初始化的东西)。
pub fn stages_for(name: &str) -> Option<&'static [StageKind]> {
    SKILL_STAGE_CATALOG
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, stages)| *stages)
}
