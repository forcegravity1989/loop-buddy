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
use bw_core::model::{stage_template_workflow, HubSource, Maturity, StageKind};
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
            })
            .await?;
    }

    Ok(())
}

/// Seed the five stage-role agents and the stage working-method skills —
/// the *executable* entities behind 五角色真实执行, projected straight from
/// `bw_core::playbook` (instructions = the real preamble template, skill
/// content = the real injected body). By-name idempotent, and deliberately
/// separate from [`seed_hub_if_empty`]'s all-or-nothing gate: an existing,
/// already-seeded database still gains these on first boot after the 完整形态
/// upgrade — they're this app's own methodology, not external catalog data.
///
/// T7 (2026-07-23, plan/12 §0/§2/§3): also backfills `stage_ref` on a named
/// row that already exists but still reads `None` — a database seeded by a
/// pre-T7 binary has these five agents/skills with no `stage_ref` column
/// value at all (honest NULL from `add_column_if_missing`); this call
/// classifies them by the same by-name match already used to decide
/// "already seeded", instead of leaving them permanently unclassified next
/// to freshly-seeded rows that do carry a real value. Never touches a row
/// whose `stage_ref` is already `Some` — there is no stage-editing UI yet, so
/// today that can only be a value this same backfill wrote on an earlier boot.
pub async fn seed_stage_entities_if_missing(store: &dyn Store) -> Result<()> {
    let existing_skills = store.list_skills().await?;
    let existing_agents = store.list_agents().await?;

    for kind in StageKind::ALL {
        for sk in bw_core::playbook::stage_skills(kind) {
            if let Some(existing) = existing_skills.iter().find(|s| s.name == sk.name) {
                if existing.stage_ref.is_none() {
                    store.set_skill_stage_ref(existing.id, Some(kind)).await?;
                }
                continue;
            }
            store
                .create_skill(NewSkill {
                    id: SkillId::new(),
                    name: sk.name.to_string(),
                    // The methodology the app itself ships — Mature, but
                    // T2 (plan/12 §6): under the unified HubSource this is
                    // `SelfBuilt`, not `Official` — `Official` now means "a
                    // curated *external* library" (carries `official_library`);
                    // this app's own built-in methodology isn't one. Same
                    // precedent `stage_template_workflow` already set on the
                    // Workflow side (`HubSource::SelfBuilt` for the identical
                    // class of content, unchanged by T1).
                    maturity: Maturity::Mature,
                    desc: sk.def.to_string(),
                    category: kind.label().to_string(),
                    // T7: the built-in stage-methodology skill really is
                    // this stage's role — a declared fact, not a guess.
                    stage_ref: Some(kind),
                    source: HubSource::SelfBuilt,
                    content: sk.content.to_string(),
                    // 五阶段方法论技能是全局共享的(见本函数文档:「这个 app
                    // 自己的方法论」),不是某个项目专属——project_id 留空。
                    project_id: None,
                })
                .await?;
        }
    }

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

// ───────────────── C9+C10 · 标配 Issue 三件套的 Skill 内容 (plan/13 D8/D9) ─────────────────
//
// 「竞品分析」（competitive-analysis，C10 补全）+「找指标」
// （north-star-discovery，C9）+「绑数据」（metrics-binding，C9）——创建流
// 自动建的标配 Issue 三件套（竞品分析 → 找指标 → 绑数据）挂 Skill 的全部
// 三件。正本是仓里的真实文件 `docs/skills/<slug>/SKILL.md`（给 plan/12 流
// 合入后的 `ImportSkillPackage` 留吸收形态：文件树本身就是"包"）；这里用
// `include_str!` 把文件内容原样编译进二进制作为 `SkillCard.content`，不是
// 另抄一份会漂移的 Rust 字符串——文件才是唯一正本，Rust 端只是把它变成可
// 查询的一行。
//
// 归属选择（对照仓内既有惯例，两条先例二选一）：
// - `Command::CreateSkill`（UI"新建"路径）新建即 `Maturity::Polishing`——
//   适合"刚做的、还没验证过"的用户自建技能。
// - `seed_stage_entities_if_missing`（本文件上方）把 app 自带的方法论技能
//   按名幂等地种成 `Maturity::Mature` + 官方来源——适合"app 自己出品的标准
//   打法"。（合流注:T1 把 LibSource 统一成 HubSource,标配卡挂
//   `Official { official_library: "bw-standard" }`——BW 自带标准库的诚实标签,
//   与 ecc/mattpocock-skills/superpowers 并列。）
// 竞品分析/找指标/绑数据是 plan/13 D8 拍板的标配流程的一部分，不是某次会
// 话里用户现造的内容，性质更接近后者：跟随 `seed_stage_entities_if_missing`
// 的先例，Mature + Official + 独立种子函数、Boot 时调用。`category` 用新值
// "标配"——既不是 OMC/ECC 目录分类，也不挂在某个 `StageKind` 下（三件套发
// 生在项目刚建好、进 Prototype 段之前的创建流末尾，不是某阶段的常规工作方
// 法），"标配"如实反映它在 plan/13 里的身份。

const NORTH_STAR_DISCOVERY_SKILL_MD: &str =
    include_str!("../../../docs/skills/north-star-discovery/SKILL.md");
const METRICS_BINDING_SKILL_MD: &str =
    include_str!("../../../docs/skills/metrics-binding/SKILL.md");
// C10 · plan/13 D8/D9: 标配三件套的第一件(竞品分析→找指标→绑数据),
// C9 落地时这张卡还不存在(见 lib.rs `seed_standard_issue_trio` 的注
// 释:「竞品分析卡是 C10 票,这里先建关联,`run_issue_now` 注入时按名查
// 不到就如实跳过、零报错」)——本票把文件与种子行都补上,C8 留的口自动
// 接上,不用回填任何既有 Issue。
const COMPETITIVE_ANALYSIS_SKILL_MD: &str =
    include_str!("../../../docs/skills/competitive-analysis/SKILL.md");

struct StandardIssueSkill {
    name: &'static str,
    desc: &'static str,
    content: &'static str,
}

const STANDARD_ISSUE_SKILLS: &[StandardIssueSkill] = &[
    StandardIssueSkill {
        name: "competitive-analysis",
        desc: "起草对标名单、各家北极星猜测、差异定位、可借鉴打法,产出报告 PR 进仓——检索不可用时如实降级为「人喂材料+agent 整理」,绝不由幻觉填充对标事实",
        content: COMPETITIVE_ANALYSIS_SKILL_MD,
    },
    StandardIssueSkill {
        name: "north-star-discovery",
        desc: "结合项目意图与竞品分析报告推导北极星+滞后+引领三层指标,每条必附采集方案——先对后亮,北极星绝不为「采得到」退化成工程虚荣指标",
        content: NORTH_STAR_DISCOVERY_SKILL_MD,
    },
    StandardIssueSkill {
        name: "metrics-binding",
        desc: "为 .bw/metrics.toml 里绑不上的指标找到点亮的最便宜路径——绝不伪造数据、绝不为了点亮而改指标定义",
        content: METRICS_BINDING_SKILL_MD,
    },
];

/// 按名幂等地种下标配 Issue 三件套的三个 Skill(竞品分析/找指标/绑数
/// 据)——`name` 就是它们稳定可查的 slug,C8 票的标配 Issue 会按这个名字
/// 关联注入。已存在(同名)就跳过,不覆盖——内容更新走 `UpdateSkill`,不
/// 是重新 seed。
pub async fn seed_standard_issue_skills_if_missing(store: &dyn Store) -> Result<()> {
    let have: std::collections::HashSet<String> = store
        .list_skills()
        .await?
        .into_iter()
        .map(|s| s.name)
        .collect();

    for s in STANDARD_ISSUE_SKILLS {
        if have.contains(s.name) {
            continue;
        }
        store
            .create_skill(NewSkill {
                id: SkillId::new(),
                name: s.name.to_string(),
                maturity: Maturity::Mature,
                desc: s.desc.to_string(),
                category: "标配".to_string(),
                source: HubSource::Official {
                    official_library: "bw-standard".to_string(),
                },
                // plan/13 D8: 标配三件套是创建流落地后原型阶段的起手活,
                // stage_ref 钉原型阶段——这是拍板不是猜测(T7 的「不猜」
                // 约束针对无人分类的导入目录行,不适用于此)。
                stage_ref: Some(StageKind::Prototype),
                content: s.content.to_string(),
                // 标配 Skill 全局共享(任何项目的标配 Issue 都能挂),同
                // seed_stage_entities_if_missing 的方法论技能一致口径。
                project_id: None,
            })
            .await?;
    }
    Ok(())
}
