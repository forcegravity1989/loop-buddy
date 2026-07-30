//! plan/17 · bw-standard 技能库的 canon 构建器 —— vendored 包文档
//! (`bw_core::bw_library`,五阶段方法论 + 标配三件套共 8 件)经**与导入路径
//! 同一个解析器**(`crate::skill_import::parse_skill_md`)解析成规范行。
//! 装载系统因此只有一条:SKILL.md 文档 → 解析 → 行;Boot 的播种
//! (`bw_store::seed_bw_standard_skills_if_missing`)与自愈对账(Pass 1/2)
//! 都吃这一份输出,Rust 端不再有第二份 desc/content 抄本。
//!
//! 三道守卫。vendored 文档是编译进二进制的开发者产物,坏了就是开发者错误,
//! 必须报出来而不是静默播错行——但**如实说清后果**:Boot 的 Err 被桌面壳
//! (`kernel.rs`)吞成一条 toast,应用照常开窗,所以调用方拿到 `Err` 后
//! 不能提前 `?` 掉后面的初始化(见 `App::dispatch` 的 `Command::Boot`:
//! 跳过 bw-standard 播种/对账,其余照跑,错误留到 Boot 末尾抛出)。
//!
//! 1. 解析出的 frontmatter `name` 必须等于包目录名 `slug`——`bw_library` 里
//!    的 slug 是目录名的复述,这条守卫钉死两者不漂移;
//! 2. `bw_library` 声明的 `desc` 必须与严格解析出的 `description` 逐字相
//!    等——声明是复述,文件是正本,漂了就报;
//! 3. 文件里的 `category`(若写了)必须与 `BwSkillDocKind` 派生出的归类一
//!    致——否则改文件那个键就是个不报错也不生效的空操作,正是本轮要消灭的
//!    「同一事实两处存放」。

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
            if doc.desc != parsed.desc {
                return Err(format!(
                    "bw-standard 包 {}:bw_library 声明的 desc 与文件 frontmatter \
                     description 不一致(声明是复述,文件是正本)",
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
            if let Some(file_category) = &parsed.category {
                if file_category != &category {
                    return Err(format!(
                        "bw-standard 包 {}:文件 frontmatter category「{file_category}」\
                         与 BwSkillDocKind 派生的「{category}」不一致",
                        doc.slug
                    ));
                }
            }
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
