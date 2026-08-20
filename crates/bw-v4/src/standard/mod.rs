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
pub mod detect;
pub mod skills;

/// 一个规范件模板:属于哪个大类、渲染到项目仓的哪个路径、模板正文。
pub struct Template {
    /// `.bw/standard.toml` 的 `enabled` 里那个键。
    pub category: &'static str,
    /// 项目仓内的相对路径。
    pub target: &'static str,
    pub body: &'static str,
    /// 哪一站铺它。见 [`LayAt`]。
    pub lay_at: LayAt,
    /// 一句话:这份件是干什么的。**开工时进 agent 的规范索引** —— agent 拿到
    /// 的是「这句话 + 路径」,正文自己按需去读,不整篇塞进系统提示词。
    pub note: &'static str,
}

const VERSION_RAW: &str = include_str!("../../../../standard/VERSION");

/// buddy 二进制自带的规范版本,如 `5.0`。
pub fn version() -> &'static str {
    VERSION_RAW.trim()
}

/// 章程落在哪。**在 `.bw/` 里,不在仓根** —— 试点第一天定的:buddy 铺进用户
/// 仓的东西全部收进 `.bw/`,仓根只留一份一行的 `CLAUDE.md`(Claude Code 的
/// 自动发现入口,唯一躲不掉的)。这样人一眼就知道「哪些是 bw 的资产」,也不会
/// 和项目自己的 README / CLAUDE.md / docs 打架。
pub const CHARTER_REL_PATH: &str = ".bw/PROJECT.md";

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

/// 一份规范件**在哪一站落地**。
///
/// 试点第一天定的:接入那一下只铺人马上要用的,其余的等你第一次真的走到那一站
/// 再出现。原来在接入时一口气铺 23 件、第一个 MR 1789 行,人根本看不完;而且
/// 大半的件(指标、周计划目录、发版记录)那一刻一个字都还用不上。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayAt {
    /// 第 0 站接入就铺:名片、章程、agent 约定、规范版本。没有它们总览什么都
    /// 读不到、agent 也不知道这个项目的规矩。
    Adopt,
    /// 到那一站第一次用到才铺。人也可以在配置屏点一次「规范铺底」提前补齐。
    OnFirstUse,
}

/// 铺底要落的核心件,按落盘顺序。`.bw/managed.toml` 不在表里——它记的是
/// 「这些件的指纹」,所以必须**最后**写,保证记的是刚落盘那一刻的真实内容。
///
/// **落点分两类,别混**:
///
/// - **`.bw/` 下的是 buddy 的资产** —— 名片、指标、周计划、发版记录、规范清单。
///   收在一个目录里,「bw 到底往我仓里放了什么」一眼可数,也不会因为项目把
///   `docs/` 或 `.claude/` 写进 .gitignore 就有件悄悄进不了版本控制。
/// - **仓根的 `AGENTS.md` 与 `CLAUDE.md` 是项目自己的** —— 它们是整个生态约定
///   俗成的位置(Claude Code / Cursor / Codex 都在仓根找),塞进 `.bw/` 谁都
///   读不到。buddy 只在它们**不存在**时搭一份初稿,已经有就一个字不动。
pub const CORE_TEMPLATES: &[Template] = &[
    Template {
        category: "charter",
        target: CHARTER_REL_PATH,
        body: CHARTER_TMPL,
        lay_at: LayAt::Adopt,
        note: "项目名片:这个项目是什么、对标谁、北极星是什么",
    },
    Template {
        category: "agents",
        // **仓根,不在 `.bw/` 里。** 这一份是**项目自己的**开发与维护手册,不是
        // buddy 的资产 —— Claude Code / Cursor / Codex 都在仓根找它,塞进 `.bw/`
        // 反而谁都读不到。buddy 只搭初稿、只维护里面那一小段带 `bw:managed`
        // 标记的话;仓里已经有这份文件就一个字不写,交给规范铺底第 2 步去谈。
        target: "AGENTS.md",
        body: AGENTS_TMPL,
        lay_at: LayAt::Adopt,
        note: "这个项目自己的开发与维护手册:怎么建、怎么跑、怎么测、目录、规矩",
    },
    Template {
        category: "agents",
        target: "CLAUDE.md",
        body: CLAUDE_TMPL,
        lay_at: LayAt::Adopt,
        note: "Claude Code 的自动发现入口,一行,指向仓根 AGENTS.md",
    },
    Template {
        category: "meta",
        target: ".bw/standard.toml",
        body: STANDARD_TMPL,
        lay_at: LayAt::Adopt,
        note: "这个项目铺的是哪一版规范、开了哪几类件",
    },
    Template {
        category: "metrics",
        target: ".bw/metrics.toml",
        body: METRICS_TMPL,
        // 第 2 站「更新指标 + 制定本周计划」才第一次用到。
        lay_at: LayAt::OnFirstUse,
        note: "指标正本:北极星/滞后/引领三层,每条附采集方案。读数只能来自真实采集",
    },
    Template {
        category: "issue-policy",
        target: ".bw/issue-policy.toml",
        body: ISSUE_POLICY_TMPL,
        // 第 2 站排期、第 4 站按类别选开工工具才用到。
        lay_at: LayAt::OnFirstUse,
        note: "活的分类 → 用什么工具开工、挂哪份剧本",
    },
    Template {
        category: "docs-core",
        target: ".bw/plan/README.md",
        body: PLAN_README,
        lay_at: LayAt::OnFirstUse,
        note: "周计划怎么写、写到哪个文件",
    },
    Template {
        category: "docs-core",
        target: ".bw/releases.md",
        body: RELEASES_TMPL,
        // 第 5 站第一次发版才用到。
        lay_at: LayAt::OnFirstUse,
        note: "版本与发布记录",
    },
    Template {
        category: "docs-core",
        target: ".bw/design/README.md",
        body: DESIGN_README,
        lay_at: LayAt::OnFirstUse,
        note: "设计文档写到哪、怎么分篇",
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
