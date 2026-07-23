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
pub async fn seed_stage_entities_if_missing(store: &dyn Store) -> Result<()> {
    let have_skills: std::collections::HashSet<String> = store
        .list_skills()
        .await?
        .into_iter()
        .map(|s| s.name)
        .collect();
    let have_agents: std::collections::HashSet<String> = store
        .list_agents()
        .await?
        .into_iter()
        .map(|a| a.name)
        .collect();

    for kind in StageKind::ALL {
        for sk in bw_core::playbook::stage_skills(kind) {
            if have_skills.contains(sk.name) {
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
                    source: HubSource::SelfBuilt,
                    content: sk.content.to_string(),
                    // 五阶段方法论技能是全局共享的(见本函数文档:「这个 app
                    // 自己的方法论」),不是某个项目专属——project_id 留空。
                    project_id: None,
                })
                .await?;
        }
    }

    for (_kind, ra) in bw_core::playbook::role_agents() {
        if have_agents.contains(ra.name) {
            continue;
        }
        store
            .create_agent(NewAgent {
                id: AgentId::new(),
                name: ra.name.to_string(),
                role: ra.role,
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
