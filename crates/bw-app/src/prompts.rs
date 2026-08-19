//! 注入队友的提示词片段:蒸馏技能块、阶段目录块、标准技能块。
//! 从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    /// A3: render up to 3 distilled (compounded) skills for project `p` as a
    /// prompt block, same-stage preferred then proven-first (`uses` desc as the
    /// distill-time proxy — `SkillCard` carries no timestamp). Only skills with
    /// real `content` distilled from a Done issue in THIS project qualify.
    /// Returns `(prompt_block, skill_refs)`. The block carries the real content
    /// (injected into the prompt); the name-led refs are returned separately so
    /// the caller can put them on `spec.skills` and let `run_workflow_inner`'s
    /// usage accounting bump each one's `uses` — closing the compounding loop
    /// (a distilled skill used by a run → uses+1). Honest `(empty, [])` when the
    /// project has no compounded skill yet.
    ///
    /// `exclude_name` skips a skill already carried by another block on the
    /// same run (in practice: the Issue's `standard_skill`). P3 opened the
    /// skill-choice dropdown to every content-bearing row, including distilled
    /// ones (`crates/app-desktop/src/screens/op.rs` `skill_choices`), so a
    /// project's own distilled skill can now be picked as `standard_skill`
    /// AND still be the top-scored candidate here (same project, same stage).
    /// Without this exclusion the caller would push the same name onto
    /// `spec.skills` twice — `run_workflow_inner`'s by-name, no-dedup
    /// accounting (`record_skill_use_by_name` in the `for s in &spec.skills`
    /// loop) would then bump that one skill's `uses` by 2 for one run,
    /// breaking settle-once, and the prompt would carry its body twice. Fixing
    /// it here (source) rather than de-duping `spec.skills` after the fact
    /// keeps the prompt honest too — the body is injected once, not twice.
    pub(crate) async fn distilled_skills_block(
        &self,
        project: ProjectId,
        stage: StageKind,
        exclude_name: &str,
    ) -> Result<(String, Vec<SkillRef>), AppError> {
        const MAX: usize = 3;
        let catalog = self.store.list_skills().await?;
        // (uses, same_stage, skill) — resolve each distilled skill back to its
        // origin issue's project+stage to scope the compounding to this project.
        let mut scored: Vec<(u32, bool, SkillCard)> = Vec::new();
        for s in catalog {
            if !exclude_name.trim().is_empty() && s.name == exclude_name {
                continue; // already carried by standard_skill_block — avoid a double count/body
            }
            let Some(iid) = s.distilled_from_issue else {
                continue;
            };
            let Some(issue) = self.store.get_issue(iid).await? else {
                continue;
            };
            if issue.project_id != project || s.content.trim().is_empty() {
                continue; // wrong project, or a content-less catalog reference
            }
            scored.push((s.uses, issue.stage == stage, s));
        }
        // Same-stage first, then proven-first; stable within ties.
        scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        let picked: Vec<&SkillCard> = scored.iter().take(MAX).map(|(_, _, s)| s).collect();
        if picked.is_empty() {
            return Ok((String::new(), Vec::new()));
        }
        let bodies: Vec<String> = picked
            .iter()
            .map(|s| format!("- {}：\n{}", s.name, s.content.trim()))
            .collect();
        let block = format!(
            "\n\n## 复利技能(本项目蒸馏,同阶段优先)\n{}\n",
            bodies.join("\n\n")
        );
        let refs: Vec<SkillRef> = picked
            .iter()
            .map(|s| SkillRef {
                name: s.name.clone(),
                def: s.desc.clone(),
                from: s.category.clone(),
            })
            .collect();
        Ok((block, refs))
    }

    /// 本阶段(含全阶段通用)技能的**目录**块 + 需要物化的候选行。
    ///
    /// 只出目录不出正文:正文由 `skill_materialize` 落到工作区
    /// `.claude/skills/`,让 CLI 按需加载。desc 里本来就有触发段(「适用:…」/
    /// "Use when …"),那正是 agent 判断该不该加载一件技能的唯一依据。
    ///
    /// 候选 = 挂了本阶段的 ∪ 挂满五阶段的。「已判定不属任何阶段」与「未归类」
    /// 都**不进**候选 —— 前者判过了不属于,后者没人判过,都不该被当成本阶段的
    /// 推荐技能。
    pub(crate) async fn stage_catalog_block(
        &self,
        stage: StageKind,
    ) -> Result<(String, Vec<SkillCard>), AppError> {
        /// 目录块字符上限。按候选最多的原型段(29 条 × 约 110 字符 ≈ 3200)取,
        /// 留余量。超限按 uses 降序截断并如实写明未列出的条数 —— 静默截断会让
        /// prompt 读起来像「本阶段就这些技能」,那是假的。
        const MAX_BLOCK_CHARS: usize = 4000;
        const DESC_CAP: usize = 80;

        let mut candidates: Vec<SkillCard> = self
            .store
            .list_skills()
            .await?
            .into_iter()
            .filter(|s| !s.content.trim().is_empty() && s.stages.contains(&stage))
            .collect();
        if candidates.is_empty() {
            return Ok((String::new(), Vec::new()));
        }
        candidates.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.name.cmp(&b.name)));

        let mut lines: Vec<String> = Vec::new();
        let mut total = 0usize;
        let mut listed = 0usize;
        for s in &candidates {
            let line = format!(
                "- {} — {}",
                s.name,
                first_sentence_capped(&s.desc, DESC_CAP)
            );
            let n = line.chars().count() + 1;
            if total + n > MAX_BLOCK_CHARS {
                break;
            }
            total += n;
            lines.push(line);
            listed += 1;
        }
        let omitted = candidates.len() - listed;
        let tail = if omitted > 0 {
            format!("\n（另有 {omitted} 件同样已物化，未在此列出——目录到此为止，不是技能到此为止）")
        } else {
            String::new()
        };
        let block = format!(
            "\n\n## 本阶段可用技能（已物化到 .claude/skills/，按需自行加载）\n{}{}\n",
            lines.join("\n"),
            tail
        );
        Ok((block, candidates))
    }

    /// C8 · 标配 Issue 三件套的 Skill 注入(plan/13 D8): resolve an Issue's
    /// `standard_skill` slug (set once by `seed_standard_issue_trio`, C9's
    /// by-name convention) against the Skill Hub catalog and render its real
    /// content as a prompt block, same shape as `distilled_skills_block`. An
    /// empty slug, or a slug that doesn't resolve to a content-bearing row
    /// (none today — all three standard cards are seeded by C9+C10), is an
    /// honest `(empty, [])` — never an error, never a fabricated body.
    /// This always returns at most one ref; `distilled_skills_block` is
    /// called with this slug as its `exclude_name` so it can't pick the same
    /// skill again (P3 let a distilled skill be chosen as `standard_skill`
    /// too — see that function's doc comment). With the exclusion in place,
    /// `record_skill_use_by_name` accounting sees each skill exactly once
    /// per run.
    pub(crate) async fn standard_skill_block(
        &self,
        project: ProjectId,
        slug: &str,
    ) -> Result<(String, Vec<SkillRef>), AppError> {
        if slug.trim().is_empty() {
            return Ok((String::new(), Vec::new()));
        }
        let catalog = self.store.list_skills().await?;
        // plan/20 R2: 同 `skills_prompt_block` 的就近遮蔽——项目里收录改过
        // 的标配副本优先于全局正本,他项目的行永不命中。
        let Some(skill) = bw_core::scope::scoped_pick(
            catalog.iter(),
            Some(project),
            |s| s.project_id,
            |s| s.name == slug,
        )
        .filter(|s| !s.content.trim().is_empty()) else {
            return Ok((String::new(), Vec::new()));
        };
        let block = format!(
            "\n\n## 标配技能(创建流关联,来自 {})\n{}\n",
            skill.name,
            // plan/16 S7: same nesting rule as `skills_prompt_block` — the
            // stored body is spec-shaped (`#` title), the injector demotes.
            bw_core::skill_body::demote_headings(skill.content.trim(), 2)
        );
        let refs = vec![SkillRef {
            name: skill.name.clone(),
            def: skill.desc.clone(),
            from: skill.category.clone(),
        }];
        Ok((block, refs))
    }

    /// V1 Issue2 Phase1: fetch the RAW skill body (full content, headings
    /// NOT demoted) for interactive injection — the skill body goes into
    /// the interactive session as the first user message (positional
    /// `prompt` argument), so it should keep its original `#`-shaped
    /// structure. Same lookup logic as `standard_skill_block` (including
    /// plan/20 R2 就近遮蔽), but returns the raw content string instead of a
    /// formatted block. An empty/content-less slug returns an empty string
    /// (honest no-op, never fails the run).
    ///
    /// plan/20 合入后 `scoped_pick` 是必须的,不是对齐洁癖:W1 起每个项目
    /// 都有一份自己的五角色副本、跨作用域同名合法(R4),裸 `find(by name)`
    /// 会撞上别的项目那一行,把他项目改过的正文灌进本项目的交互式会话。
    pub(crate) async fn fetch_skill_body(
        &self,
        project: ProjectId,
        slug: &str,
    ) -> Result<String, AppError> {
        if slug.trim().is_empty() {
            return Ok(String::new());
        }
        let catalog = self.store.list_skills().await?;
        let Some(skill) = bw_core::scope::scoped_pick(
            catalog.iter(),
            Some(project),
            |s| s.project_id,
            |s| s.name == slug,
        )
        .filter(|s| !s.content.trim().is_empty()) else {
            return Ok(String::new());
        };
        Ok(skill.content.trim().to_string())
    }
}
