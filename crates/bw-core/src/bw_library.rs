//! plan/17 · The bw-standard library's **vendored package documents** — the
//! one place BW's own eight standard skills exist as raw `SKILL.md` text.
//!
//! 「一套系统」的关键一步:BW 自带技能不再是散在 Rust 字符串常量里的
//! "第二形态"——正本就是 `docs/skills/<slug>/SKILL.md` 真实文件(与
//! mattpocock/superpowers 等外库完全同构的**包**),这里只是把它们
//! `include_str!` 进二进制,供:
//!
//! - `bw-app` 的 canon 构建器用**与导入路径同一个解析器**解析后播种/对账
//!   (装载系统因此只有一条:SKILL.md 文档 → 解析 → 行);
//! - [`crate::playbook`] 的 prompt 注入取 body(`skill_body::strip_frontmatter`
//!   剥掉 frontmatter,`demote_headings` 下沉标题)。
//!
//! `include_str!` 是编译期读取——本模块保持 bw-core 的零 IO/wasm32 约束。
//! 这里**不做解析**:frontmatter 的 name/description 由 bw-app 侧的唯一
//! 解析器负责(serde_yaml 不进内核);`slug`/`stage` 是目录名与既有拍板的
//! 复述,canon 构建器会用「解析出的 name 必须等于 slug」守卫两者一致。

use crate::model::StageKind;

/// Where a bw-standard skill doc belongs in the five-stage methodology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BwSkillDocKind {
    /// One of the five stage working-method skills (playbook 注入对象)。
    Stage(StageKind),
    /// 标配 Issue 三件套(plan/13 D8:创建流的起手活,钉原型段,分类「标配」)。
    StandardIssue,
}

/// One vendored skill package document: the raw `SKILL.md` file text,
/// frontmatter and all.
pub struct BwSkillDoc {
    /// The package directory name under `docs/skills/` — must equal the
    /// frontmatter `name` (canon 构建器守卫)。
    pub slug: &'static str,
    pub kind: BwSkillDocKind,
    pub raw: &'static str,
}

pub const EVIDENCE_FIRST_MD: &str = include_str!("../../../docs/skills/evidence-first/SKILL.md");
pub const SPEC_TO_TESTS_MD: &str = include_str!("../../../docs/skills/spec-to-tests/SKILL.md");
pub const BASELINE_BEFORE_TOUCH_MD: &str =
    include_str!("../../../docs/skills/baseline-before-touch/SKILL.md");
pub const FRESH_EYES_FUNNEL_MD: &str =
    include_str!("../../../docs/skills/fresh-eyes-funnel/SKILL.md");
pub const BREAKING_DRILL_MD: &str = include_str!("../../../docs/skills/breaking-drill/SKILL.md");
pub const COMPETITIVE_ANALYSIS_MD: &str =
    include_str!("../../../docs/skills/competitive-analysis/SKILL.md");
pub const NORTH_STAR_DISCOVERY_MD: &str =
    include_str!("../../../docs/skills/north-star-discovery/SKILL.md");
pub const METRICS_BINDING_MD: &str = include_str!("../../../docs/skills/metrics-binding/SKILL.md");

/// The whole bw-standard skill library, in seed order (five stage skills in
/// stage order, then the standard-issue trio in its 竞品分析→找指标→绑数据
/// chain order).
pub fn bw_standard_skill_docs() -> [BwSkillDoc; 8] {
    [
        BwSkillDoc {
            slug: "evidence-first",
            kind: BwSkillDocKind::Stage(StageKind::Prototype),
            raw: EVIDENCE_FIRST_MD,
        },
        BwSkillDoc {
            slug: "spec-to-tests",
            kind: BwSkillDocKind::Stage(StageKind::Build),
            raw: SPEC_TO_TESTS_MD,
        },
        BwSkillDoc {
            slug: "baseline-before-touch",
            kind: BwSkillDocKind::Stage(StageKind::Optimize),
            raw: BASELINE_BEFORE_TOUCH_MD,
        },
        BwSkillDoc {
            slug: "fresh-eyes-funnel",
            kind: BwSkillDocKind::Stage(StageKind::Growth),
            raw: FRESH_EYES_FUNNEL_MD,
        },
        BwSkillDoc {
            slug: "breaking-drill",
            kind: BwSkillDocKind::Stage(StageKind::Ops),
            raw: BREAKING_DRILL_MD,
        },
        BwSkillDoc {
            slug: "competitive-analysis",
            kind: BwSkillDocKind::StandardIssue,
            raw: COMPETITIVE_ANALYSIS_MD,
        },
        BwSkillDoc {
            slug: "north-star-discovery",
            kind: BwSkillDocKind::StandardIssue,
            raw: NORTH_STAR_DISCOVERY_MD,
        },
        BwSkillDoc {
            slug: "metrics-binding",
            kind: BwSkillDocKind::StandardIssue,
            raw: METRICS_BINDING_MD,
        },
    ]
}
