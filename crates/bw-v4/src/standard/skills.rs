//! buddy 自带的技能库 —— **放在 buddy 自己的目录里,不复制进用户的仓**。
//!
//! 以前是铺底时把十三份 `SKILL.md` 复制进项目仓 `.claude/skills/`,试点第一天
//! 当场证明这条路走不通:buddy 自己的仓把 `.claude/` 写进了 `.gitignore`,
//! 十三个文件一个都没进第一个 MR,而界面上还显示铺好了。用户的仓怎么写
//! `.gitignore` 不该由 buddy 决定,所以技能不再进用户的仓。
//!
//! 现在的做法:开工前把这些文件展开到 buddy 自己的资产目录
//! (`<asset_root>/skills/…`,见 `App::skills_dir`),系统提示词里只给 agent
//! **名字 + 一句话 + 完整路径**,正文让它自己按需去读。这就是渐进式加载 ——
//! 和 buddy 自己的系统提示词(`docs/buddy/system-prompt.md`)是同一套规矩:
//! 提示词里放索引,正文按需读。

/// 一份技能包:名字、一句话、正文,以及它在技能目录下的相对落点。
pub struct SkillPack {
    /// 技能名(= 目录名 = frontmatter 的 `name`)。agent 在提示词里看到的就是它。
    pub slug: &'static str,
    /// 相对 `<asset_root>/skills/` 的落点。子技能挂在入口包目录下面,
    /// 不是平级的第二个技能。
    pub rel: String,
    /// frontmatter 那句 `description`,原样取自文件。
    pub desc: &'static str,
    pub raw: &'static str,
}

pub const WEEK_PLANNING_SKILL: &str =
    include_str!("../../../../standard/skills/week-planning/SKILL.md");
pub const METRICS_REFRESH_SKILL: &str =
    include_str!("../../../../standard/skills/metrics-refresh/SKILL.md");
pub const ASSET_AUDIT_SKILL: &str =
    include_str!("../../../../standard/skills/asset-audit/SKILL.md");
pub const PROJECT_HANDBOOK_SKILL: &str =
    include_str!("../../../../standard/skills/project-handbook/SKILL.md");
pub const EVIDENCE_FIRST_SKILL: &str =
    include_str!("../../../../standard/skills/evidence-first/SKILL.md");
pub const SPEC_TO_TESTS_SKILL: &str =
    include_str!("../../../../standard/skills/spec-to-tests/SKILL.md");
pub const BASELINE_BEFORE_TOUCH_SKILL: &str =
    include_str!("../../../../standard/skills/baseline-before-touch/SKILL.md");
pub const FRESH_EYES_FUNNEL_SKILL: &str =
    include_str!("../../../../standard/skills/fresh-eyes-funnel/SKILL.md");
pub const BREAKING_DRILL_SKILL: &str =
    include_str!("../../../../standard/skills/breaking-drill/SKILL.md");
pub const COMPETITIVE_ANALYSIS_SKILL: &str =
    include_str!("../../../../standard/skills/competitive-analysis/SKILL.md");
pub const METRICS_RENDER_SKILL: &str =
    include_str!("../../../../standard/skills/metrics-render/SKILL.md");

/// frontmatter 里那句 `description`。取不到就返回空串 —— 不编一句假的。
///
/// 这四份运作剧本的一句话只能这么来:正本是 `standard/` 下的真实文件,手抄一份
/// 常量迟早和文件漂移。九份方法论技能不走这里 —— `bw-core` 那边已经有一份被
/// Boot 守卫过「与文件逐字相等」的 `desc`,用它。
fn desc_of(raw: &'static str) -> &'static str {
    let mut lines = raw.lines();
    if lines.next().map(str::trim) != Some("---") {
        return "";
    }
    for line in lines {
        if line.trim() == "---" {
            return "";
        }
        if let Some(rest) = line.strip_prefix("description:") {
            return rest.trim();
        }
    }
    ""
}

/// buddy 自带的全部技能 —— **一个正本目录,全部平级**:`standard/skills/<名字>/SKILL.md`,
/// 十一份(七份方法论 + 四份运作剧本)。
///
/// 2026-08-21 收敛前是两个正本目录(`docs/skills/` 九份经 `bw-core` 取用 +
/// `standard/06-defaults/` 四份),还用目录层级表达「谁调用谁」——但注入给
/// agent 时是摊平的,层级只在源码树里存在。收敛之后:谁调用谁写在正文里
/// (`week-planning` 第二步指名调 `metrics-refresh`),目录不再表达从属;
/// `north-star-discovery` / `metrics-binding` 两份的内容已并入
/// `metrics-refresh`,不进这份清单 —— `docs/skills/` 那两份原文件留给 V3,
/// V4 不再读它们。
pub fn all() -> Vec<SkillPack> {
    const PACKS: [(&str, &str); 11] = [
        ("week-planning", WEEK_PLANNING_SKILL),
        ("metrics-refresh", METRICS_REFRESH_SKILL),
        ("asset-audit", ASSET_AUDIT_SKILL),
        ("project-handbook", PROJECT_HANDBOOK_SKILL),
        ("evidence-first", EVIDENCE_FIRST_SKILL),
        ("spec-to-tests", SPEC_TO_TESTS_SKILL),
        ("baseline-before-touch", BASELINE_BEFORE_TOUCH_SKILL),
        ("fresh-eyes-funnel", FRESH_EYES_FUNNEL_SKILL),
        ("breaking-drill", BREAKING_DRILL_SKILL),
        ("competitive-analysis", COMPETITIVE_ANALYSIS_SKILL),
        ("metrics-render", METRICS_RENDER_SKILL),
    ];
    PACKS
        .iter()
        .map(|(slug, raw)| SkillPack {
            slug,
            rel: format!("{slug}/SKILL.md"),
            desc: desc_of(raw),
            raw,
        })
        .collect()
}
