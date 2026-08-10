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
//! 这里**不做解析**:frontmatter 由 bw-app 侧的唯一解析器负责(serde_yaml
//! 不进内核)。`slug`/`desc`/`kind` 是目录名、frontmatter 那句话、既有拍板
//! 的**复述**,不是第二个正本——canon 构建器逐条守卫它们与严格解析结果一致
//! (name==slug、desc 逐字相等、frontmatter category 与 kind 派生值相等),
//! 任一处漂移都在 Boot 当场报出来,不会静默生效。

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
    /// The frontmatter `description`, declared here so kernel-side
    /// projections (`crate::model::stage_workflow_with_playbook`'s
    /// `SkillRef.def`) get it **without parsing anything** — YAML 解析器不进
    /// 内核,而按需现扫一遍原文既慢又会在写法合法但扫不动时静默退化成空串。
    /// 同 `slug`:这是文件里那句话的复述,不是第二个正本 —— canon 构建器用
    /// 严格解析结果守卫两者逐字相等,谁漂移谁在 Boot 被当场抓住。
    pub desc: &'static str,
    pub raw: &'static str,
}

pub const EVIDENCE_FIRST_MD: &str = include_str!("../../../../docs/skills/evidence-first/SKILL.md");
pub const SPEC_TO_TESTS_MD: &str = include_str!("../../../../docs/skills/spec-to-tests/SKILL.md");
pub const BASELINE_BEFORE_TOUCH_MD: &str =
    include_str!("../../../../docs/skills/baseline-before-touch/SKILL.md");
pub const FRESH_EYES_FUNNEL_MD: &str =
    include_str!("../../../../docs/skills/fresh-eyes-funnel/SKILL.md");
pub const BREAKING_DRILL_MD: &str = include_str!("../../../../docs/skills/breaking-drill/SKILL.md");
pub const COMPETITIVE_ANALYSIS_MD: &str =
    include_str!("../../../../docs/skills/competitive-analysis/SKILL.md");
pub const NORTH_STAR_DISCOVERY_MD: &str =
    include_str!("../../../../docs/skills/north-star-discovery/SKILL.md");
pub const METRICS_BINDING_MD: &str =
    include_str!("../../../../docs/skills/metrics-binding/SKILL.md");
pub const METRICS_RENDER_MD: &str = include_str!("../../../../docs/skills/metrics-render/SKILL.md");

// 每条包文档 frontmatter `description` 的复述(见 `BwSkillDoc::desc`):必须
// 与文件逐字相等,Boot canon 构建器守卫。`const` 而非函数内联字面量,是因为
// [`crate::playbook::stage_skills`] 返回 `&'static [StageSkill]`,只能引用
// 常量——两处因此共用同一份,不会各抄一遍。
pub const EVIDENCE_FIRST_DESC: &str =
    "证据先行:只写站得住的内容,先验知识标注「未核实」。适用:原型段整理证据与调研文档,或任何需要引用事实与数字的产出";
pub const SPEC_TO_TESTS_DESC: &str =
    "规格即测试:每条验收标准落成一个可跑的用例。适用:构建段从 SPEC 落实现之前,以及评审时逐条核对验收标准";
pub const BASELINE_BEFORE_TOUCH_DESC: &str =
    "先测基线再动手:无基线不优化,删减优先。适用:优化段动手重构或调性能之前,以及收尾时报基线 delta";
pub const FRESH_EYES_FUNNEL_DESC: &str =
    "新用户漏斗走查:以新人视角亲手走一遍,只记录真实摩擦。适用:运营推广段诊断「发现→安装→首用→复用」漏斗,或对照验证上线改动";
pub const BREAKING_DRILL_DESC: &str =
    "破坏性演练:系统性喂坏输入,坏行为当场修复。适用:运维段事故演练、健康检查脚本落地,或发布前的稳健性检查";
pub const COMPETITIVE_ANALYSIS_DESC: &str =
    "起草对标名单、各家北极星猜测、差异定位、可借鉴打法,产出报告 PR 进仓——检索不可用时如实降级为「人喂材料+agent 整理」,绝不由幻觉填充对标事实。适用:项目创建后的标配起手活「竞品分析」,或任何需要重摸对标的时点";
pub const NORTH_STAR_DISCOVERY_DESC: &str =
    "结合项目意图与竞品分析报告推导北极星+滞后+引领三层指标,每条必附采集方案——先对后亮,北极星绝不为「采得到」退化成工程虚荣指标。适用:标配 Issue「找指标」,或北极星需要重推的时点";
pub const METRICS_BINDING_DESC: &str =
    "为 .bw/metrics.toml 里绑不上的指标找到点亮的最便宜路径——绝不伪造数据、绝不为了点亮而改指标定义。适用:标配 Issue「绑数据」,或健康灯长期 Unknown 需要接真数据源的时点";
pub const METRICS_RENDER_DESC: &str =
    "把项目指标的真实状态渲染成一页可复核的自包含 HTML——零观测渲染成「无数据」而不是 0,每个数字自带 sqlite3 复核命令,绿色隐身。适用:想看某个项目/全部项目此刻的指标看板,或要把指标状态发给别人看的时点";

/// The whole bw-standard skill library, in seed order (five stage skills in
/// stage order, then the standard-issue skills in their 竞品分析→找指标→绑数据
/// chain order, with 渲染指标 appended as the chain's read-out end).
///
/// 注意:`BwSkillDocKind::StandardIssue` 只决定这条 skill 的 category(「标配」)
/// 与 stage 归属,**不**决定创建流自动建哪几张 Issue —— 那是
/// [`crate::super`] 之外 `bw-app` 里 `seed_standard_issue_trio` 的
/// 硬编码三元组。所以这里多一条 skill 不会让每个新项目多出一张活。
pub fn bw_standard_skill_docs() -> [BwSkillDoc; 9] {
    [
        BwSkillDoc {
            slug: "evidence-first",
            kind: BwSkillDocKind::Stage(StageKind::Prototype),
            desc: EVIDENCE_FIRST_DESC,
            raw: EVIDENCE_FIRST_MD,
        },
        BwSkillDoc {
            slug: "spec-to-tests",
            kind: BwSkillDocKind::Stage(StageKind::Build),
            desc: SPEC_TO_TESTS_DESC,
            raw: SPEC_TO_TESTS_MD,
        },
        BwSkillDoc {
            slug: "baseline-before-touch",
            kind: BwSkillDocKind::Stage(StageKind::Optimize),
            desc: BASELINE_BEFORE_TOUCH_DESC,
            raw: BASELINE_BEFORE_TOUCH_MD,
        },
        BwSkillDoc {
            slug: "fresh-eyes-funnel",
            kind: BwSkillDocKind::Stage(StageKind::Growth),
            desc: FRESH_EYES_FUNNEL_DESC,
            raw: FRESH_EYES_FUNNEL_MD,
        },
        BwSkillDoc {
            slug: "breaking-drill",
            kind: BwSkillDocKind::Stage(StageKind::Ops),
            desc: BREAKING_DRILL_DESC,
            raw: BREAKING_DRILL_MD,
        },
        BwSkillDoc {
            slug: "competitive-analysis",
            kind: BwSkillDocKind::StandardIssue,
            desc: COMPETITIVE_ANALYSIS_DESC,
            raw: COMPETITIVE_ANALYSIS_MD,
        },
        BwSkillDoc {
            slug: "north-star-discovery",
            kind: BwSkillDocKind::StandardIssue,
            desc: NORTH_STAR_DISCOVERY_DESC,
            raw: NORTH_STAR_DISCOVERY_MD,
        },
        BwSkillDoc {
            slug: "metrics-binding",
            kind: BwSkillDocKind::StandardIssue,
            desc: METRICS_BINDING_DESC,
            raw: METRICS_BINDING_MD,
        },
        BwSkillDoc {
            slug: "metrics-render",
            kind: BwSkillDocKind::StandardIssue,
            desc: METRICS_RENDER_DESC,
            raw: METRICS_RENDER_MD,
        },
    ]
}
