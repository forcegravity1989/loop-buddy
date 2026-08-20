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
    include_str!("../../../../standard/06-defaults/ops/week-planning/SKILL.md");
pub const METRICS_REFRESH_SKILL: &str = include_str!(
    "../../../../standard/06-defaults/ops/week-planning/skills/metrics-refresh/SKILL.md"
);
pub const ASSET_AUDIT_SKILL: &str =
    include_str!("../../../../standard/06-defaults/ops/asset-audit/SKILL.md");
pub const MERGE_ADJUST_SKILL: &str = include_str!(
    "../../../../standard/06-defaults/ops/standard-bootstrap-agent/merge-adjust/SKILL.md"
);

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

/// buddy 自带的全部技能:九份方法论技能(正本 `docs/skills/`,经 `bw-core`
/// 取用)+ 四份运作剧本(正本 `standard/06-defaults/`)。
pub fn all() -> Vec<SkillPack> {
    let mut v: Vec<SkillPack> = bw_core::bw_library::bw_standard_skill_docs()
        .into_iter()
        .map(|d| SkillPack {
            slug: d.slug,
            rel: format!("{}/SKILL.md", d.slug),
            desc: d.desc,
            raw: d.raw,
        })
        .collect();
    // 四份运作剧本。`metrics-refresh` 是 `week-planning` 第二步调用的子技能,
    // 所以挂在它目录下面,不是平级的第二个技能。
    for (slug, rel, raw) in [
        (
            "week-planning",
            "week-planning/SKILL.md",
            WEEK_PLANNING_SKILL,
        ),
        (
            "metrics-refresh",
            "week-planning/skills/metrics-refresh/SKILL.md",
            METRICS_REFRESH_SKILL,
        ),
        ("asset-audit", "asset-audit/SKILL.md", ASSET_AUDIT_SKILL),
        ("merge-adjust", "merge-adjust/SKILL.md", MERGE_ADJUST_SKILL),
    ] {
        v.push(SkillPack {
            slug,
            rel: rel.to_string(),
            desc: desc_of(raw),
            raw,
        });
    }
    v
}
