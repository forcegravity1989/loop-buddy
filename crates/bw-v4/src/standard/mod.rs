//! `standard/` —— 规范件正本,编译期打进二进制。
//!
//! 仓根 `standard/` 与 `crates/`、`docs/` 平级,`include_str!` 在编译期读进
//! 常量(零 IO、和 `bw-core` 的 `buddy_assets`/`bw_library` 同一个已验证的做
//! 法)。**改 `standard/` 就是改产品行为**,不要搬。
//!
//! 两个版本号别混:
//! - `standard/VERSION`(这里的 [`version`]):**buddy 侧** —— 当前二进制自带
//!   哪一版规范,随发布走。
//! - 项目仓 `.bw/standard.toml` 的 `version`:**项目侧** —— 这个项目铺的是哪
//!   一版,随项目走。
//!
//! 两个值常常相等,含义不同,不合并成一个字段。

pub mod bootstrap;

/// 一个规范件模板:属于哪个大类、渲染到项目仓的哪个路径、模板正文。
pub struct Template {
    /// `.bw/standard.toml` 的 `enabled` 里那个键。
    pub category: &'static str,
    /// 项目仓内的相对路径。
    pub target: &'static str,
    pub body: &'static str,
}

const VERSION_RAW: &str = include_str!("../../../../standard/VERSION");

/// buddy 二进制自带的规范版本,如 `4.0`。
pub fn version() -> &'static str {
    VERSION_RAW.trim()
}

pub const CHARTER_TMPL: &str = include_str!("../../../../standard/01-charter/PROJECT.md.tmpl");
pub const AGENTS_TMPL: &str = include_str!("../../../../standard/02-agents/AGENTS.md.tmpl");
pub const CLAUDE_TMPL: &str = include_str!("../../../../standard/02-agents/CLAUDE.md.tmpl");
pub const PLAN_README: &str = include_str!("../../../../standard/03-docs/plan/README.md");
pub const WEEK_TMPL: &str = include_str!("../../../../standard/03-docs/plan/WEEK.md.tmpl");
pub const RELEASES_TMPL: &str = include_str!("../../../../standard/03-docs/releases.md.tmpl");
pub const DESIGN_README: &str = include_str!("../../../../standard/03-docs/design/README.md");
pub const METRICS_TMPL: &str = include_str!("../../../../standard/04-metrics/metrics.toml.tmpl");
pub const ISSUE_POLICY_TMPL: &str =
    include_str!("../../../../standard/05-issue-policy/issue-policy.toml.tmpl");
pub const STANDARD_TMPL: &str = include_str!("../../../../standard/08-meta/standard.toml.tmpl");

/// 铺底第 1 步要落的核心件,按落盘顺序。`.bw/managed.toml` 不在表里——它记的
/// 是「这些件的指纹」,所以必须**最后**写,保证记的是刚落盘那一刻的真实内容。
pub const CORE_TEMPLATES: &[Template] = &[
    Template {
        category: "charter",
        target: "PROJECT.md",
        body: CHARTER_TMPL,
    },
    Template {
        category: "agents",
        target: "AGENTS.md",
        body: AGENTS_TMPL,
    },
    Template {
        category: "agents",
        target: "CLAUDE.md",
        body: CLAUDE_TMPL,
    },
    Template {
        category: "docs-core",
        target: "docs/plan/README.md",
        body: PLAN_README,
    },
    Template {
        category: "docs-core",
        target: "docs/releases.md",
        body: RELEASES_TMPL,
    },
    Template {
        category: "docs-core",
        target: "docs/design/README.md",
        body: DESIGN_README,
    },
    Template {
        category: "metrics",
        target: ".bw/metrics.toml",
        body: METRICS_TMPL,
    },
    Template {
        category: "issue-policy",
        target: ".bw/issue-policy.toml",
        body: ISSUE_POLICY_TMPL,
    },
    Template {
        category: "meta",
        target: ".bw/standard.toml",
        body: STANDARD_TMPL,
    },
];

/// 把 `{{key}}` 换成值。没给到的占位符**原样留着**,而不是替换成空串——
/// 留着能在文件里一眼看出「这里本来该填什么但没填到」,替成空串就查不出来了。
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

/// 三张运作 workflow 的正本(规范第 6 类「默认件与鱼塘」)。
///
/// 和预置技能包一样必须**真的复制进项目仓**——agent 开工时按活的 `workflow`
/// 字段找剧本,找的是项目仓 `.claude/skills/` 下的文件,不是 buddy 的安装目录。
///
/// `standard/06-defaults/ops/README.md` 不在这张表里:它说的是 buddy 自己
/// `standard/` 目录怎么排版、版本怎么走,项目仓里没人读得到用处。
pub const WEEK_PLANNING_SKILL: &str =
    include_str!("../../../../standard/06-defaults/ops/week-planning/SKILL.md");
pub const METRICS_REFRESH_SKILL: &str = include_str!(
    "../../../../standard/06-defaults/ops/week-planning/skills/metrics-refresh/SKILL.md"
);
pub const ASSET_AUDIT_SKILL: &str =
    include_str!("../../../../standard/06-defaults/ops/asset-audit/SKILL.md");
pub const MERGE_ADJUST_SKILL: &str = include_str!(
    "../../../../standard/06-defaults/ops/standard-bootstrap-agent/merge-adjust/SKILL.md"
);

/// 项目仓内的落点 → 正文。子技能 `metrics-refresh` 挂在入口包目录下面,
/// 不是平级的第二个技能——它只由 `week-planning` 第二步调用。
pub fn ops_workflow_packages() -> Vec<(String, &'static str)> {
    vec![
        (
            ".claude/skills/week-planning/SKILL.md".into(),
            WEEK_PLANNING_SKILL,
        ),
        (
            ".claude/skills/week-planning/skills/metrics-refresh/SKILL.md".into(),
            METRICS_REFRESH_SKILL,
        ),
        (
            ".claude/skills/asset-audit/SKILL.md".into(),
            ASSET_AUDIT_SKILL,
        ),
        (
            ".claude/skills/merge-adjust/SKILL.md".into(),
            MERGE_ADJUST_SKILL,
        ),
    ]
}

/// 预置技能包:buddy 自带的九份方法论技能。
///
/// Claude CLI 只在项目仓里找技能,不会去读 buddy 的安装目录——所以铺底必须
/// 真的把这些文件**复制进项目仓** `.claude/skills/`,不是只记一个名字。正本
/// 是 `docs/skills/<slug>/SKILL.md` 真实文件,这里经 `bw-core` 的既有常量取
/// 用,不复制第二份文本。
pub fn preset_skill_packages() -> Vec<(String, &'static str)> {
    bw_core::bw_library::bw_standard_skill_docs()
        .into_iter()
        .map(|d| (format!(".claude/skills/{}/SKILL.md", d.slug), d.raw))
        .collect()
}
