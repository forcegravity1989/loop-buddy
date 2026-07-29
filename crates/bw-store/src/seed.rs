//! Hub-catalog seeding. `seed_hub_if_empty` only inserts when all three hub
//! tables are empty, so re-opening an already-seeded database is a no-op,
//! never a duplicate.
//!
//! T1 (2026-07-23, plan/12 §1/§6/§8): the former OMC (oh-my-claudecode) /
//! ECC (Everything Claude Code) directory-only seeds (73 OMC skills, 37 OMC
//! agents, 271 ECC skills, 67 ECC agents, 92 ECC commands-as-workflows) were
//! deleted here — every one of those rows carried only a name + one-line
//! description with **no real body** (`content`/`instructions` empty), which
//! is the "展示是错的" shell this repo's honesty rules forbid. Real official
//! catalogs (mattpocock-skills, superpowers, ECC agents with real file
//! bodies) are a separate, later import path (`HubSource::Official` +
//! `official_library` sub-tag) — not rebuilt here.
//!
//! What `seed_hub_if_empty` still plants: the app's own 5 stage-template
//! workflows (`bw_core::model::stage_template_workflow`, one per `StageKind`)
//! — not external data, but this app's own already-designed methodology
//! (`StageKind`'s core question / method loop / DoD) made into a standing,
//! importable Hub entry instead of only the ephemeral spec a session builds
//! on the fly.

use crate::{NewAgent, NewSkill, NewWorkflowSpec, Result, Store};
use bw_core::model::{
    stage_template_workflow, HubSource, Maturity, StageKind, BW_STANDARD_LIBRARY,
};
use bw_core::{AgentId, SkillId};

/// Seed the hub library's own stage-template workflows if it's currently
/// empty. Called once at `Boot`; safe to call on every boot since it checks
/// first and is a no-op once seeded.
pub async fn seed_hub_if_empty(store: &dyn Store) -> Result<()> {
    if !store.list_skills().await?.is_empty()
        || !store.list_agents().await?.is_empty()
        || !store.list_workflow_specs().await?.is_empty()
    {
        return Ok(());
    }

    for kind in StageKind::ALL {
        let spec = stage_template_workflow(kind);
        store
            .create_workflow_spec(NewWorkflowSpec {
                id: spec.id,
                name: spec.name,
                kind: spec.kind,
                prompt: spec.prompt,
                goal: spec.goal,
                stage_ref: spec.stage_ref,
                phases: spec.phases,
                phase_prompts: spec.phase_prompts,
                agents: spec.agents,
                skills: spec.skills,
                loop_config: spec.loop_config,
                project_id: None,
                content: spec.content,
            })
            .await?;
    }

    Ok(())
}

/// Seed the five stage-role agents — the *executable* entities behind
/// 五角色真实执行, projected straight from `bw_core::playbook`
/// (instructions = the real preamble template). By-name idempotent, and
/// deliberately separate from [`seed_hub_if_empty`]'s all-or-nothing gate: an
/// existing, already-seeded database still gains these on first boot after
/// the 完整形态 upgrade — they're this app's own methodology, not external
/// catalog data.
///
/// plan/17: the stage working-method *skills* that used to ride along here
/// now enter through the unified package-document path instead —
/// [`seed_bw_standard_skills_if_missing`], fed by bw-app's single SKILL.md
/// parser over `bw_core::bw_library`'s vendored docs — so skill 装载只剩一条链.
///
/// T7 (2026-07-23, plan/12 §0/§2/§3): also backfills `stage_ref` on a named
/// row that already exists but still reads `None` — a database seeded by a
/// pre-T7 binary has these five agents with no `stage_ref` column
/// value at all (honest NULL from `add_column_if_missing`); this call
/// classifies them by the same by-name match already used to decide
/// "already seeded", instead of leaving them permanently unclassified next
/// to freshly-seeded rows that do carry a real value. Never touches a row
/// whose `stage_ref` is already `Some` — there is no stage-editing UI yet, so
/// today that can only be a value this same backfill wrote on an earlier boot.
pub async fn seed_stage_role_agents_if_missing(store: &dyn Store) -> Result<()> {
    let existing_agents = store.list_agents().await?;

    for (kind, ra) in bw_core::playbook::role_agents() {
        if let Some(existing) = existing_agents.iter().find(|a| a.name == ra.name) {
            if existing.stage_ref.is_none() {
                store.set_agent_stage_ref(existing.id, Some(kind)).await?;
            }
            continue;
        }
        store
            .create_agent(NewAgent {
                id: AgentId::new(),
                name: ra.name.to_string(),
                role: ra.role,
                // T7: same "declared fact, not a guess" reasoning as the
                // stage skill above — this is literally the agent `kind`'s
                // own role.
                stage_ref: Some(kind),
                maturity: Maturity::Mature,
                skills: ra.skills,
                model: ra.model.to_string(),
                instructions: ra.instructions,
                // T5 (plan/12 §3): the five built-in stage-role agents
                // declare no AllowedTools restriction (honest — the
                // playbook never specified one) and run on the one real
                // executor this app has (`claude-code`). Their provenance
                // is this app's own methodology, same call
                // `stage_template_workflow`/`seed_stage_entities_if_missing`
                // already made for the identical class of skill content
                // (`HubSource::SelfBuilt`, plan/12 §6 acceptance criterion).
                tools: Vec::new(),
                agent_cli: "claude-code".to_string(),
                source: HubSource::SelfBuilt,
                // 同上:五角色是全局单例,不因这次践行的项目自有切片改变。
                project_id: None,
            })
            .await?;
    }
    Ok(())
}

// ───────────────── plan/17 · bw-standard 技能库的统一播种 ─────────────────
//
// 正本是仓里的真实包文件 `docs/skills/<slug>/SKILL.md`(五阶段方法论技能 +
// 标配 Issue 三件套,经 `bw_core::bw_library` 以 `include_str!` 编译进二进
// 制)。这里**不再持有任何 desc/content 抄本、也不解析**:bw-app 的 canon
// 构建器(`bw_canon`)用与 `ImportSkillPackage` 同一个解析器把 vendored 文
// 档解析成 [`CanonicalSkill`] 行传进来——装载系统只有一条:SKILL.md 文档 →
// 解析 → 行。此前这里的 `STANDARD_ISSUE_SKILLS` 静态表(desc 手抄 + 整文件
// include_str)与 `seed_stage_entities_if_missing` 里的 playbook 投影是
// 「第二形态」的最后两处,随本轮删除。
//
// 归属沿革(plan/13 D8 + plan/16 §4 拍板,不变):八件都是 app 自己出品的
// 标准打法 —— `Maturity::Mature` + `Official { "bw-standard" }`;编辑即脱离
// 源头(T11 翻 `SelfBuilt` + 留痕「改编自」),Boot 对账只追 Official 行。

/// One bw-standard library row's canonical shape, ready to compare against or
/// write into the `skill` table — the parsed projection of one vendored
/// `SKILL.md` package document. Named fields, not a positional tuple — desc
/// and content are both `String`, and a swap must not compile silently.
pub struct CanonicalSkill {
    pub name: String,
    /// Frontmatter `description`, via bw-app's single strict parser.
    pub desc: String,
    /// plan/16 S7: the SKILL.md **body**, YAML frontmatter already stripped —
    /// the same shape `ImportSkillPackage` has always stored for external
    /// libraries(详情展示与 prompt 注入共用这一形态).
    pub content: String,
    /// 归类徽记:五阶段技能 = `kind.label()`(原型/构建/…),三件套 = 「标配」
    /// —— 由 `bw_library` 的 `BwSkillDocKind` 拍板派生,不是猜测。
    pub category: String,
    /// 每件 bw-standard 技能都有阶段锚(五阶段技能挂本阶段;标配三件套钉
    /// 原型段 —— plan/13 D8 拍板:创建流落地后原型阶段的起手活)。
    pub stage_ref: StageKind,
}

/// 按名幂等地种下 bw-standard 技能库(五阶段方法论 + 标配三件套)。已存在
/// (同名)只回填缺失的 `stage_ref`(T7 语义:pre-T7 库的诚实 NULL 按既有
/// 拍板补上,已有值绝不动),不覆盖 desc/content——内容对账是 bw-app Boot
/// 的 Pass 2(只追 `Official { "bw-standard" }` 行),不是重新 seed。
pub async fn seed_bw_standard_skills_if_missing(
    store: &dyn Store,
    canon: &[CanonicalSkill],
) -> Result<()> {
    let existing_skills = store.list_skills().await?;
    for c in canon {
        if let Some(existing) = existing_skills.iter().find(|s| s.name == c.name) {
            // stage_ref 回填只认**确实是我们这一行**的行(Official
            // {bw-standard})。纯按名匹配会把用户自建的同名技能(比如他自
            // 己那条 metrics-binding,stage_ref 为 NULL = 通用/跨阶段,是既
            // 有的诚实语义)误当成「已种下的标配」,悄悄改掉他的归类——用户
            // 什么都没做,自己的技能被系统动了。被本函数替换掉的三件套种
            // 子路径在命中同名时就是什么都不碰,这里跟随那条更保守的先例。
            // 老库里读回 SelfBuilt 的那批真·内置行由 Boot 的 Pass 1 升源,
            // 下一次 Boot 走到这里就满足条件、照常补上。
            if existing.stage_ref.is_none()
                && matches!(
                    &existing.source,
                    HubSource::Official { official_library }
                        if official_library == BW_STANDARD_LIBRARY
                )
            {
                store
                    .set_skill_stage_ref(existing.id, Some(c.stage_ref))
                    .await?;
            }
            continue;
        }
        store
            .create_skill(NewSkill {
                id: SkillId::new(),
                name: c.name.clone(),
                maturity: Maturity::Mature,
                desc: c.desc.clone(),
                category: c.category.clone(),
                stage_ref: Some(c.stage_ref),
                source: HubSource::Official {
                    official_library: BW_STANDARD_LIBRARY.to_string(),
                },
                content: c.content.clone(),
                // bw-standard 技能全局共享(方法论/标配都不是某项目专属),
                // project_id 留空。
                project_id: None,
            })
            .await?;
    }
    Ok(())
}
