//! plan/17 · bw-standard 技能库的 canon 构建器 —— vendored 包文档
//! (`bw_core::bw_library`,五阶段方法论 + 标配三件套共 8 件)经**与导入路径
//! 同一个解析器**(`crate::skill_import::parse_skill_md`)解析成规范行。
//! 装载系统因此只有一条:SKILL.md 文档 → 解析 → 行;Boot 的播种
//! (`bw_store::seed_bw_standard_skills_if_missing`)与自愈对账(Pass 1/2)
//! 都吃这一份输出,Rust 端不再有第二份 desc/content 抄本。
//!
//! 两道守卫(vendored 文档是编译进二进制的开发者产物,坏了就该 Boot 当场
//! 报错拒绝启动,绝不静默播错行):
//!
//! 1. 解析出的 frontmatter `name` 必须等于包目录名 `slug`——`bw_library` 里
//!    的 slug 是目录名的复述,这条守卫钉死两者不漂移;
//! 2. 内核宽容提取器 `bw_core::skill_body::frontmatter_description`(喂
//!    `SkillRef.def` 的那只,YAML 解析器不进内核)的输出必须与严格解析出的
//!    `description` 一致——两个实现谁漂移谁被当场抓住,vendored 文档也因此
//!    被约束在宽容器读得懂的子集内(顶格单行普通标量)。

use bw_core::bw_library::{bw_standard_skill_docs, BwSkillDocKind};
use bw_core::model::StageKind;
use bw_store::CanonicalSkill;

/// Parse the eight vendored bw-standard package documents into canonical
/// rows, in seed order. `Err` = a vendored doc is malformed or violates a
/// guard — a developer error the caller (Boot) surfaces loudly.
pub(crate) fn bw_standard_skill_canon() -> Result<Vec<CanonicalSkill>, String> {
    bw_standard_skill_docs()
        .iter()
        .map(|doc| {
            let parsed = crate::skill_import::parse_skill_md(doc.raw)
                .map_err(|e| format!("bw-standard 包 {}:{e}", doc.slug))?;
            if parsed.name != doc.slug {
                return Err(format!(
                    "bw-standard 包 {}:frontmatter name「{}」与包目录名不一致",
                    doc.slug, parsed.name
                ));
            }
            let kernel_desc = bw_core::skill_body::frontmatter_description(doc.raw);
            if kernel_desc.as_deref() != Some(parsed.desc.as_str()) {
                return Err(format!(
                    "bw-standard 包 {}:内核宽容提取的 description 与严格解析不一致\
                     (SkillRef.def 会漂移;description 须为顶格单行普通标量)",
                    doc.slug
                ));
            }
            let (category, stage_ref) = match doc.kind {
                // 五阶段方法论技能挂本阶段,归类徽记即阶段名(T7:拍板,
                // 不是猜测)。
                BwSkillDocKind::Stage(kind) => (kind.label().to_string(), kind),
                // plan/13 D8: 标配三件套是创建流落地后原型阶段的起手活,
                // stage_ref 钉原型段。
                BwSkillDocKind::StandardIssue => ("标配".to_string(), StageKind::Prototype),
            };
            Ok(CanonicalSkill {
                name: parsed.name,
                desc: parsed.desc,
                content: parsed.body,
                category,
                stage_ref,
            })
        })
        .collect()
}
