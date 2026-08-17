//! Issue 运行生命周期:准备 → 交互式跑(内嵌终端)→ 结算(只推到评审中,Done 永远由人点)→ 取消。
//! 从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    /// V1 Issue2 Phase1: the interactive path's settle accounting — the
    /// counterpart of `finalize_run` for one-shot TUI agent sessions.
    /// Does the same agent/skill `uses` bumping + artifact reflux, but
    /// SKIPS the phase-based parts (no `workflow_run` row → no HEAD diff
    /// via `set_run_heads`, no `LoopEnd` conversion). The interactive
    /// session's terminal scrollback + session.jsonl is the record (§2.5
    /// 砍了对话摘要 collector); the worktree's git state + artifacts are
    /// the file-level evidence buddy keeps.
    ///
    /// DEVIATION: HEAD diff is not recorded for interactive runs (no
    /// `workflow_run` row to attach `set_run_heads` to). The worktree's
    /// own git log IS the diff evidence — it just isn't indexed in
    /// buddy's `workflow_run.head_before/after` columns. Recorded in the
    /// commit deviation note; Phase 2 can add a `workflow_run` row for
    /// interactive sessions if needed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finalize_run_interactive(
        &mut self,
        spec: &WorkflowSpec,
        proj: &ProjectRow,
        p: ProjectId,
        issue_id: Option<IssueId>,
        skill_output: SkillOutput,
    ) -> Result<RunOutcome, AppError> {
        let run_ok = skill_output.completed;

        // Agent/skill uses accounting — same as `finalize_run`: one real
        // work item = one agent run, one skill use. Doing this once per
        // interactive session (not per phase) keeps the compounding loop
        // honest. plan/20 R3(main 合入): by-id via `scope::scoped_pick`,
        // 同一条就近规则,他项目的同名五角色副本绝不被连带 bump。
        let agent_catalog = self.store.list_agents().await?;
        for a in &spec.agents {
            if let Some(row) = bw_core::scope::scoped_pick(
                agent_catalog.iter(),
                Some(p),
                |x| x.project_id,
                |x| x.name == a.name,
            ) {
                self.store.record_agent_run(row.id, run_ok).await?;
            }
        }
        let skill_catalog = self.store.list_skills().await?;
        for s in &spec.skills {
            if let Some(row) = bw_core::scope::scoped_pick(
                skill_catalog.iter(),
                Some(p),
                |x| x.project_id,
                |x| x.name == s.name,
            ) {
                self.store.record_skill_use(row.id).await?;
            }
        }
        if !spec.agents.is_empty() {
            self.refresh_agents().await?;
            self.emit(Event::AgentsChanged);
        }
        if !spec.skills.is_empty() {
            self.refresh_skills().await?;
            self.emit(Event::SkillsChanged);
        }

        // Artifact reflux: scan the real workspace and register new file
        // versions. No `workflow_run_id` (interactive sessions don't create
        // one) — artifacts bind to the issue via `issue_id`. Scan errors
        // are a 0-fresh no-op, same as the phase-loop path.
        if !proj.workspace_path.trim().is_empty() {
            let stage_kind = spec
                .stage_ref
                .and_then(|n| StageKind::ALL.into_iter().find(|s| s.index() == n));
            if let Ok(fresh) = self
                .scan_and_register_artifacts(
                    p,
                    &proj.workspace_path,
                    None, // no workflow_run row for interactive
                    stage_kind,
                    issue_id,
                )
                .await
            {
                if fresh > 0 {
                    self.emit(Event::ArtifactsRegistered { fresh });
                }
            }
        }

        // Convert SkillOutput → RunOutcome. `completed = true` → Completed
        // (the caller's tail advances to InReview iff a PR opens). `false`
        // → Err (the issue stays InProgress, never auto-Done).
        if skill_output.completed {
            Ok(RunOutcome::Completed)
        } else {
            Err(AppError::Engine(skill_output.summary))
        }
    }

    /// V1 Issue2 Phase2a: the resume path's settle accounting — a slimmed
    /// `finalize_run_interactive` that does artifact scan ONLY (no agent/skill
    /// `uses` bump — settle-once: the first run already counted the use; no
    /// MR creation — the agent creates it in-session; no status transition —
    /// InReview comes from the `poll_interactive_inreview` poller). Called
    /// from `run_issue_settle` when `ar.is_resume` is true.
    pub(crate) async fn finalize_run_interactive_resume(
        &mut self,
        proj: &ProjectRow,
        p: ProjectId,
        issue_id: Option<IssueId>,
        stage: StageKind,
    ) -> Result<(), AppError> {
        if !proj.workspace_path.trim().is_empty() {
            if let Ok(fresh) = self
                .scan_and_register_artifacts(
                    p,
                    &proj.workspace_path,
                    None, // no workflow_run row for interactive
                    Some(stage),
                    issue_id,
                )
                .await
            {
                if fresh > 0 {
                    self.emit(Event::ArtifactsRegistered { fresh });
                }
            }
        }
        Ok(())
    }

    /// plan/17 S1+S3: entry guard + lifecycle for the same-project serial
    /// run lock (`AppState::active_run`). Backgrounded path (`settle_tx`
    /// wired — the desktop kernel): `run_issue_backgrounded` sets
    /// `active_run` after the 起手 prefix succeeds and returns immediately;
    /// the kernel's settle arm later drives `run_issue_settle`, which clears
    /// it. Inline path (`settle_tx` = None — examples / headless drivers):
    /// `run_issue_body` runs the whole run inline; the `await` already
    /// serializes, so `active_run` stays `None` (dormant). Both callers
    /// (`Command::RunIssue` and C8 「立即开工」) route here, so the lock
    /// applies uniformly.
    pub(crate) async fn run_issue_now(
        &mut self,
        session: SessionId,
        id: IssueId,
    ) -> Result<(), AppError> {
        let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
        let p = issue.project_id;

        // Done/InReview 有会话行 → 咨询 resume(不占 active_run)。
        // V1 收口:不再限 is_interactive_skill —— 任何有 claude_conversation
        // 行的 Done/InReview issue 都能续聊(与 UI consultable_issues 对齐)。
        let conv = self.store.get_conversation_by_issue(id).await?;
        let is_resume = conv
            .as_ref()
            .map(|c| !c.claude_session_id.is_empty())
            .unwrap_or(false);
        if is_resume && matches!(issue.status, IssueStatus::Done | IssueStatus::InReview) {
            return self.open_conversation(id).await;
        }
        // 本件交付已在跑且 PTY 仍活 → 只切焦点。
        // PTY 已死但 active_run 仍挂(zombie 锁)时不要 early-return:
        // 旧逻辑 focus 死 id 后直接 Ok,侧栏「唤不醒」、看板因完整 ▶跑/
        // remount 仍能救回来。
        if let Some(ar) = &self.state.active_run {
            if ar.issue.id == id {
                if let Some(c) = &conv {
                    if self.state.terminal_manager.is_live(c.id) {
                        self.focus_conversation(c.id, id).await;
                        return Ok(());
                    }
                }
            }
        }
        // 咨询 PTY 已活 → 只切焦点。
        if let Some(c) = &conv {
            if self.state.terminal_manager.is_live(c.id) {
                self.focus_conversation(c.id, id).await;
                return Ok(());
            }
        }

        // Fail fast on a same-project in-flight run before touching anything.
        // 本件 zombie(锁在、PTY 死)放行:清掉名额后再走 interactive resume。
        if let Some(ar) = &self.state.active_run {
            if ar.issue.id == id {
                let _zombie = self.state.active_run.take();
                // handle/guard drop with ActiveRun — PTY already gone; settle
                // may still arrive and no-op via cleanup paths.
            } else if ar.project == p {
                return Err(AppError::Invalid(
                    "该项目有活正在跑，等它到评审中/干完再开下一张".into(),
                ));
            }
        }
        // V1 收口:所有 issue ▶跑 走交互式嵌入终端(buddy 脚本调度的阶段循环
        // 路径退场;多 agent 转由技能方法论 prompt 驱动,claude 在会话内用
        // SubAgent 调度)。非 issue 命令(RunStagePlaybook/hub workflow/cron)
        // 仍用阶段循环机器。
        self.run_issue_interactive(session, id).await
    }

    /// plan/17 S3: the shared 起手 prefix of an issue-run — get + validate
    /// the Issue, build the stage-playbook `WorkflowSpec` (+ standard /
    /// distilled skill injection), transition to InProgress (refresh + emit),
    /// and provision the isolated issue worktree behind an RAII guard.
    /// `&mut self` (transition + refresh), but returns FAST — the long round
    /// loop is NOT here. Both the inline path (`run_issue_body`) and the
    /// backgrounded path (`run_issue_backgrounded`) start here, so the
    /// spec / worktree / guard setup is never duplicated.
    ///
    /// V1 Issue2 Phase2a: `resume` controls the interactive resume path.
    /// When `true` (interactive issue with an existing claude_conversation row
    /// re-clicked ▶跑): skip the status check (Done/InReview can resume —
    /// the session persists, no status change happens) and skip the
    /// InProgress transition (the issue stays in its current state). The
    /// worktree is still provisioned (for the cwd `--continue` needs). One-
    /// shot callers always pass `false`.
    pub(crate) async fn prepare_issue_run(
        &mut self,
        id: IssueId,
        resume: bool,
    ) -> Result<IssueRunPrep, AppError> {
        let issue = self.store.get_issue(id).await?.ok_or(AppError::NotFound)?;
        // A5-F: only work not yet settled/parked/under-review/blocked
        // can be (re)started this way. InProgress is a legal starting
        // point too — it's the retry path after an honest failure
        // (the issue stays InProgress on error, never faked forward).
        // V1 Issue2 Phase2a: resume bypasses this check — a Done/InReview
        // issue can resume its interactive session (no status change).
        if !resume
            && !matches!(
                issue.status,
                IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
            )
        {
            return Err(AppError::Invalid(format!(
                "#{} 处于{},不能直接运行",
                issue.number,
                issue.status.label()
            )));
        }
        let p = issue.project_id;
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;

        // Same stage-playbook scaffolding as RunStagePlaybook (fills the
        // role preamble + real project context), then the issue is
        // stamped on top so the agent runs its stage methodology against
        // THIS concrete work item.
        let handoff_note = self
            .store
            .list_handoffs(p)
            .await?
            .first()
            .map(|h| h.note.clone())
            .unwrap_or_default();
        let workspace_hint = if proj.workspace_path.trim().is_empty() {
            "（未配置真实工作区 —— 本次运行在 MockExecutor 上,产出仅为流程演示）".to_string()
        } else {
            format!(
                "工作区 {}（git 仓库）。产出落于此;先查看现状再动手。",
                proj.workspace_path.trim()
            )
        };
        let ctx = bw_core::playbook::PlaybookCtx {
            project_name: proj.name.clone(),
            project_kind: proj.kind.clone(),
            project_desc: proj.desc.clone(),
            benchmark: proj.benchmark.clone(),
            opportunity: proj.opportunity.clone(),
            north_star: proj.north_star.clone(),
            ns_def: proj.ns_def.clone(),
            handoff_note,
            workspace_hint,
        };
        let mut spec = stage_workflow_with_playbook(issue.stage, &ctx);
        // C8 · plan/13 D8: 标配 Issue 携带的稳定 Skill 关联(按 C9+C10 种下
        // 的 slug)。理论上仍可能查不到(例如 seed 尚未跑过的极早期状态),
        // 那种情况如实跳过、零报错,不是本注入路径的正常状态。V1 收口:issue
        // 全转终端后 spec.prompt/phase_prompts 不再服务 issue(交互式用 bridge
        // 系统提示词 + IssueRunPrep.distilled_block/catalog_block);这里只取
        // `standard_refs` 给 `spec.skills` 记 uses,格式化块不再拼进 spec。
        let (_, standard_refs) = self.standard_skill_block(p, &issue.standard_skill).await?;
        // V2-① 第二轮: 蒸馏块排除的不是 issue.standard_skill(可能为空),
        // 而是 effective_skill(显式 > 阶段默认)。否则当 issue 无显式技能、
        // 用阶段默认做主 Skill 时,同名蒸馏技能会被同时注入为主 Skill 和
        // 蒸馏补充,违反 §6.5「不冒充第二份主 Skill」。
        let effective_exclude = if !issue.standard_skill.trim().is_empty() {
            issue.standard_skill.clone()
        } else {
            bw_core::playbook::stage_skills(issue.stage)
                .first()
                .map(|s| s.name.to_string())
                .unwrap_or_default()
        };
        // Distilled (compounded) skills from this project, same-stage
        // preferred, capped at 3. Exclude the effective main Skill by name
        // to avoid double-injecting the same content as both main Skill
        // and distilled supplement (§6.5).
        let (distilled_block, distilled_refs) = self
            .distilled_skills_block(p, issue.stage, &effective_exclude)
            .await?;
        // 五角色归类的落地(2026-08-05):本阶段技能的目录进 prompt、正文物化到
        // 工作区。与上面两块的关键区别 —— 目录里的技能**不进** `spec.skills`,
        // 因此不记 uses:目录列了二十几条、agent 实际可能只读两条,全都记一笔
        // 就是造假,会稀释 uses「真被用了」的语义。真解析(读 claude CLI 的
        // session jsonl 里的 tool_use=Skill 记录,只给真加载的记账)已验证可行,
        // 用户 2026-08-05 明确说本轮不做。
        let (catalog_block, catalog_skills) = self.stage_catalog_block(issue.stage).await?;
        let workspace_present = !proj.workspace_path.trim().is_empty();
        if !catalog_skills.is_empty() && workspace_present {
            let mut with_files = Vec::with_capacity(catalog_skills.len());
            for s in catalog_skills {
                let files = self.store.list_skill_files(s.id).await?;
                with_files.push((s, files));
            }
            let report =
                skill_materialize::materialize_stage_skills(&proj.workspace_path, &with_files)
                    .await;
            if !report.skipped_foreign.is_empty() {
                eprintln!(
                    "[BW_SKILL_MATERIALIZE] 跳过 {} 件同名但非 BW 托管的技能目录(用户自己的 skill 优先):{}",
                    report.skipped_foreign.len(),
                    report.skipped_foreign.join(", ")
                );
            }
            if !report.skipped_unsafe.is_empty() {
                // 评审找出的真坑(2026-08-06):路径穿越拦截,跟上面「用户的
                // 东西优先」是完全不同性质的跳过 —— 单独一行,避免混进
                // `skipped_foreign` 让人误读成"这只是普通冲突"。
                eprintln!(
                    "[BW_SKILL_MATERIALIZE_SECURITY] 拦截 {} 件路径穿越技能(name/rel_path 校验不过,未写盘):{}",
                    report.skipped_unsafe.len(),
                    report.skipped_unsafe.join(", ")
                );
            }
            eprintln!(
                "[BW_SKILL_MATERIALIZE] 写入 {} · 未变 {}",
                report.written.len(),
                report.unchanged
            );
        }
        // Important 修复(评审实测确认,plan 原文缺口,由控制者拍板修):
        // `catalog_block` 的文案原文承诺「已物化到 .claude/skills/」——但上面
        // 的物化分支只在 `workspace_present` 时跑。空工作区(Mock 项目)磁盘
        // 上根本不存在这些文件,若仍把这段“已物化”的文案拼进 prompt,就是对
        // 执行 agent 说假话,直接违反本仓「mock 必须自我标注、绝不假装」的
        // 核心纪律。所以拼接（不只是物化）也要进这道闸门:工作区为空时这一
        // 段干脆不出现在 prompt 里,而不是出现一段承诺落空的文案。
        let catalog_block = if workspace_present {
            catalog_block
        } else {
            String::new()
        };
        spec.name = format!("#{} {}", issue.number, issue.title);
        // Put the injected skills on spec.skills so finalize_run_interactive's
        // usage accounting bumps each one's `uses` — the compounding
        // loop closes here (a run that rides a distilled/standard skill →
        // uses+1, exactly once per ref present).
        spec.skills.extend(standard_refs);
        spec.skills.extend(distilled_refs);

        // Start: commit to the work (Backlog/Todo → InProgress). A
        // retry (issue already InProgress from a prior failed run)
        // skips this — X→X is not a legal table edge, and there's
        // nothing to change anyway.
        // V1 Issue2 Phase2a: resume skips this entirely — the issue
        // stays in its current state (InProgress/InReview/Done); only
        // the session is resumed, no status transition.
        if !resume && issue.status != IssueStatus::InProgress {
            self.store
                .transition_issue(id, IssueStatus::InProgress)
                .await?;
            self.refresh_issues().await?;
            self.emit(Event::IssuesChanged);
        }

        // C5 · PR 验收环 (plan/13 D3): a repo-attached, GitHub-mapped
        // Issue quarantines the run onto `bw/issue-<n>` BEFORE the
        // executor touches anything, so the executor never advances the
        // base branch — only a human merge does. No-repo / unmapped
        // projects (`pr_eligible` false) never touch git here → today's
        // behavior byte-for-byte. A branch-checkout failure degrades
        // honestly: the run proceeds on the current branch, no PR is
        // opened (提 PR 失败不炸 run).
        let pr_eligible = !proj.remote_path.trim().is_empty()
            && issue.github_number != 0
            && !proj.workspace_path.trim().is_empty();
        // plan/17 S2: isolate this run in its own git worktree off the main
        // workspace (master), so concurrent/back-to-back issue runs in one
        // project never collide on the shared working tree. Main workspace
        // stays on master; only this worktree carries `bw/issue-<n>`. A
        // provisioning failure degrades honestly (run on main workspace, no
        // PR) — same fallback shape as the old `checkout_issue_branch` path,
        // just isolated when it succeeds.
        let issue_ws: Option<PathBuf> = if pr_eligible {
            match bw_engine::workspace::provision_issue_worktree(
                std::path::Path::new(proj.workspace_path.trim()),
                issue.github_number,
            )
            .await
            {
                Ok(p) => Some(p),
                Err(e) => {
                    self.emit(Event::ConnectorSynced {
                        name: format!("#{} · 活分支", issue.number),
                        ok: false,
                        detail: format!("开 issue worktree 失败,本次运行在主工作区、不提 PR:{e}"),
                    });
                    None
                }
            }
        } else {
            None
        };
        // RAII: remove the worktree on EVERY exit path (early `?`/Ok/Err/
        // panic). The branch stays on `origin` (pushed by `stage_commit_push`)
        // for review/merge — only the local working dir is dropped. The
        // caller owns the guard so it lives as long as the run needs it.
        let guard = bw_engine::workspace::IssueWorktreeGuard::new(
            PathBuf::from(proj.workspace_path.trim()),
            issue_ws.clone(),
        );

        Ok(IssueRunPrep {
            issue,
            proj,
            spec,
            issue_ws,
            guard,
            distilled_block,
            catalog_block,
        })
    }

    /// V1 收口:the INTERACTIVE issue-run path (one-shot TUI agent session,
    /// no phase loop). Same 起手 prefix (`prepare_issue_run`) as the old
    /// phase-loop path, then builds a [`LaunchPlan`] (skill body from the
    /// Skill Hub + bridge system prompt from [`PlaybookCtx`]) and either:
    ///  - **Backgrounded** (`settle_tx` wired — desktop kernel): spawn a
    ///    task that calls [`InteractiveExecutor::run_skill_pty`] (PTY spawn +
    ///    byte streaming), report back via `settle_tx`. The kernel's settle
    ///    arm drives `finalize_run_interactive(_resume)` + PTY close + guard
    ///    drop (no `issue_run_tail` — MR 由 agent 自提、InReview 由 poll 检测;
    ///    无 MR 的活诚实停在 InProgress,铁律:Done 永不自动)。
    ///  - **Inline** (`settle_tx` = None — examples / headless drivers):
    ///    await `run_skill` inline, then `finalize_run_interactive`
    ///    synchronously.
    ///
    /// A project with no real `workspace_path` runs on
    /// [`MockInteractiveExecutor`] (self-labeled 【mock】 + placeholder
    /// `.bw/metrics.toml`); a configured one runs on
    /// [`InteractiveCliExecutor`] (spawns a PTY running the
    /// `claude` CLI). V1 收口:所有 issue ▶跑 都走本函数(issue 脚本调度
    /// 路径 `run_issue_body`/`run_issue_backgrounded`/`issue_run_tail` 已删,
    /// 不为向后兼容留旧路径)。
    pub(crate) async fn run_issue_interactive(
        &mut self,
        _session: SessionId,
        id: IssueId,
    ) -> Result<(), AppError> {
        // V1 终端会话重构(阶段1): first-run vs resume — decided by the
        // claude_conversation 行(按 issue_id 查,非空 claude_session_id =
        // resume)。会话身份在 claude_conversation 表(不在 issue 行):首次
        // spawn 前 ensure_conversation 建行(留空 claude_session_id + 填
        // workspace_path/branch);hook SessionStart 回传填 claude_session_id。
        // 空 claude_session_id = 首次 spawn 没捕获到(失败或 hook 未 fire)→
        // fallback build_startup_plan(重灌 skill,F1)。非空 → `--resume <id>`
        // (precise session)。
        let conv = self.store.get_conversation_by_issue(id).await?;
        let is_resume = conv
            .as_ref()
            .map(|c| !c.claude_session_id.is_empty())
            .unwrap_or(false);
        // 重启恢复:is_live==false 时走 resume 分支(不误撞锁——上方 run_issue_now
        // 已确认无同卡活 PTY / 无同卡 active_run)。点卡到首包显示「恢复中…」。
        if is_resume {
            if let Some(c) = &conv {
                self.state.pty_restoring = Some(c.id);
            }
        }
        let prep = match self.prepare_issue_run(id, is_resume).await {
            Ok(p) => p,
            Err(e) => {
                if is_resume {
                    self.state.pty_restoring = None;
                }
                return Err(e);
            }
        };
        let p = prep.issue.project_id;
        let issue = prep.issue.clone();
        let standard_skill = prep.issue.standard_skill.clone();
        let IssueRunPrep {
            issue: _,
            proj,
            spec,
            issue_ws,
            guard,
            distilled_block,
            catalog_block,
        } = prep;

        let workspace_cwd = issue_ws
            .as_deref()
            .unwrap_or_else(|| Path::new(proj.workspace_path.trim()));

        // V2-①: 物化 buddy 规范运行副本到工作区 `.claude/buddy/standards/`。
        // 首次启动和恢复都做(§7.1:恢复前再次校准规范运行副本)。
        // 无真实工作区的项目跳过(MockExecutor 路径,继续标【mock】)。
        // 物化失败 → 不启动真实 Claude,如实报错给用户(§4.3 #3、§8)。
        if !proj.workspace_path.trim().is_empty() {
            match buddy_materialize::materialize_standards(workspace_cwd).await {
                Ok(report) => {
                    eprintln!(
                        "[BW_BUDDY_MATERIALIZE] written={} unchanged={}",
                        report.written, report.unchanged
                    );
                }
                Err(e) => {
                    if is_resume {
                        self.state.pty_restoring = None;
                    }
                    return Err(AppError::Invalid(format!(
                        "buddy 规范运行副本物化失败,不启动真实 Claude:{e}"
                    )));
                }
            }
        }

        // Build the plan: startup (first run) or resume (--resume <session_id>).
        let plan = if is_resume {
            // Resume: no new prompt, no bridge system prompt. The session
            // persists under ~/.claude/projects/<encoded-cwd>/ from the
            // first run; `--resume <session_id>` re-enters the exact session.
            // session_id 来自 claude_conversation 行(hook 回传)。
            // Spec §7.1 / 验收 11.5: 如实提示沿用首次上下文,要新版方法须新开执行。
            self.emit(Event::ConnectorSynced {
                name: format!("issue #{} · 恢复会话", issue.number),
                ok: true,
                detail: "沿用首次开工的系统提示词与主 Skill;规范文件已更新。若要换新方法请新开执行(不要指望恢复暗换)"
                    .into(),
            });
            let session_id = conv
                .as_ref()
                .map(|c| c.claude_session_id.as_str())
                .unwrap_or("");
            match build_resume_plan(&CLAUDE, Some(session_id), workspace_cwd) {
                Ok(p) => p,
                Err(e) => {
                    self.state.pty_restoring = None;
                    return Err(AppError::Engine(e.to_string()));
                }
            }
        } else {
            // First run: fetch skill body + build bridge system prompt.
            let handoff_note = self
                .store
                .list_handoffs(p)
                .await?
                .first()
                .map(|h| h.note.clone())
                .unwrap_or_default();
            let workspace_hint = if proj.workspace_path.trim().is_empty() {
                "（未配置真实工作区 —— 交互式会话在 MockInteractiveExecutor 上,产出仅为流程演示）"
                    .to_string()
            } else {
                format!(
                    "工作区 {}（git 仓库）。产出落于此;先查看现状再动手。",
                    proj.workspace_path.trim()
                )
            };
            let playbook_ctx = bw_core::playbook::PlaybookCtx {
                project_name: proj.name.clone(),
                project_kind: proj.kind.clone(),
                project_desc: proj.desc.clone(),
                benchmark: proj.benchmark.clone(),
                opportunity: proj.opportunity.clone(),
                north_star: proj.north_star.clone(),
                ns_def: proj.ns_def.clone(),
                handoff_note,
                workspace_hint,
            };
            // V2-① 第二轮: issue 无显式 Skill 时按 issue.stage 装载阶段默认
            // SOP;有显式则用显式(替换,不叠加)。§6.1: 一张 Issue 只选一份
            // 主 Skill —— 显式 > 阶段默认 > 无。默认 SOP 按 issue 自身阶段
            // 选,不随项目交棒改变。
            let (effective_skill, is_default) = if !standard_skill.trim().is_empty() {
                (standard_skill.clone(), false)
            } else {
                let default_slug = bw_core::playbook::stage_skills(issue.stage)
                    .first()
                    .map(|s| s.name.to_string())
                    .unwrap_or_default();
                (default_slug, true)
            };
            let skill_body = match self.fetch_skill_body(p, &effective_skill).await {
                Ok(b) => b,
                Err(e) => return Err(e),
            };
            // §6.1 点3: 阶段默认 Skill 正文为空 = 资产损坏或打包遗漏。系统
            // 提示词仍可用,但开工前在界面和运行日志中明确告警,不假装已加载。
            if is_default && !effective_skill.is_empty() && skill_body.trim().is_empty() {
                eprintln!(
                    "[BW_SKILL_WARN] 阶段默认方法 `{effective_skill}` 正文为空(issue #{}, stage {:?}),本次仅依据 buddy 规范处理",
                    issue.number, issue.stage
                );
                self.emit(Event::ConnectorSynced {
                    name: format!("issue #{} · 阶段默认技能", issue.number),
                    ok: false,
                    detail: format!(
                        "阶段默认方法 `{effective_skill}` 正文为空或缺失,本次仅依据 buddy 规范处理(不假装已加载默认 SOP)"
                    ),
                });
            }
            // §8: 显式选择的 Skill 正文为空(被删/被清空)→ 不悄悄回落、不假装
            // 已加载,告诉用户所选不可用,让其修正选择(不启动真实 Claude)。
            if !is_default && !effective_skill.is_empty() && skill_body.trim().is_empty() {
                return Err(AppError::Invalid(format!(
                    "所选技能 `{effective_skill}` 不可用(正文为空或已删除),请在 issue 上改选其它技能后再跑"
                )));
            }
            // V2-①: 系统提示词 = 静态正本(system-prompt.md) + 动态项目上下文 +
            // 主 Skill 正文 + 蒸馏块 + 目录块,用 `---` 分隔(§13.5)。
            // §6.5: 蒸馏/目录是补充,不冒充第二份主 Skill,不偷偷替换用户
            // 显式选择的主 Skill。
            let project_context =
                build_project_context_block(&playbook_ctx, &effective_skill, is_default);
            let mut system_prompt = bw_core::buddy_assets::SYSTEM_PROMPT_MD.to_string();
            for block in [
                &project_context,
                &skill_body,
                &distilled_block,
                &catalog_block,
            ] {
                let block = block.trim();
                if !block.is_empty() {
                    system_prompt.push_str("\n\n---\n\n");
                    system_prompt.push_str(block);
                }
            }
            let issue_prompt = if issue.desc.trim().is_empty() {
                issue.title.clone()
            } else {
                format!("{}\n\n{}", issue.title, issue.desc)
            };
            build_startup_plan(&CLAUDE, &issue_prompt, &system_prompt, workspace_cwd)
                .map_err(|e| AppError::Engine(e.to_string()))?
        };

        // V1 终端会话重构: 首次 spawn 前建会话行;resume 时行已在。两端都要
        // ConversationId 交给 TerminalManager。
        let branch_name = if issue.github_number != 0 {
            format!("bw/issue-{}", issue.github_number)
        } else {
            String::new()
        };
        let conversation_id = if !is_resume {
            self.store
                .ensure_conversation(id, p, workspace_cwd.to_str().unwrap_or(""), &branch_name)
                .await?
        } else {
            conv.as_ref().map(|c| c.id).ok_or_else(|| {
                AppError::Invalid("resume 需要已有 claude_conversation 行,但库里没有".into())
            })?
        };

        // V1 Issue2 Phase2b: register the worktree cwd → IssueId mapping so
        // the hook listener can route SessionStart/Stop events (which carry
        // `cwd` in the payload) back to this issue. Registered before spawn
        // so events arrive as soon as the session starts.
        let cwd_key = workspace_cwd.to_string_lossy().to_string();
        if !cwd_key.is_empty() {
            self.state.interactive_sessions.insert(cwd_key, id);
        }

        // Construct the executor: real spawn if workspace configured, mock
        // if not (same honest split as the phase-loop path's engine
        // selection).
        let executor: Arc<dyn InteractiveExecutor> = if proj.workspace_path.trim().is_empty() {
            Arc::new(MockInteractiveExecutor::new())
        } else {
            Arc::new(
                InteractiveCliExecutor::new()
                    .with_claude_binary(self.state.claude_config.binary.clone()),
            )
        };

        let ctx = RunCtx {
            project: p,
            workflow: spec.id,
        };

        // Announce (same event as the phase-loop path — the UI shows
        // "this run uses X/Y").
        self.emit(Event::RunStarted {
            workflow_name: spec.name.clone(),
            agents: spec.agents.clone(),
            skills: spec.skills.clone(),
        });

        // V1 Issue2 Phase2b: when PTY mode is enabled (desktop kernel wires
        // it via `App::with_pty`), the backgrounded path creates byte-stream
        // channels and uses `run_skill_pty` (PTY spawn + byte streaming)
        // instead of `run_skill` (system terminal). The inline path
        // (examples/headless) never uses PTY — no UI to render bytes.

        if let Some(settle_tx) = self.settle_tx.clone() {
            // ── Backgrounded (desktop kernel) ──
            let handle = if self.state.pty_enabled {
                // V1 终端会话重构·底座: 经 TerminalManager.attach 拿带身份的
                // channels(有界缓冲在 Manager 侧);不再塞全局单槽。
                let meta = ConversationMeta {
                    conversation_id,
                    issue_id: id,
                    claude_session_id: conv
                        .as_ref()
                        .map(|c| c.claude_session_id.clone())
                        .unwrap_or_default(),
                    workspace_path: workspace_cwd.to_path_buf(),
                    branch_name: branch_name.clone(),
                };
                let (bytes_tx, input_rx) =
                    self.state
                        .terminal_manager
                        .attach(conversation_id, meta, None);
                self.focus_conversation(conversation_id, id).await;
                if is_resume {
                    let _ = self
                        .backfill_conversation_workspace_if_empty(
                            id,
                            workspace_cwd.to_str().unwrap_or(""),
                            &branch_name,
                        )
                        .await;
                }
                tokio::spawn(async move {
                    let result = executor
                        .run_skill_pty(&plan, &ctx, bytes_tx, input_rx)
                        .await
                        .map_err(|e| AppError::Engine(e.to_string()));
                    let _ = settle_tx.send(SettleReq {
                        project: p,
                        issue: id,
                        outcome: SettleOutcome::Interactive(result),
                    });
                })
            } else {
                // System-terminal mode (2a behavior — PTY not enabled or
                // examples/headless). The task calls run_skill / run_skill_resume.
                tokio::spawn(async move {
                    let result = if is_resume {
                        executor.run_skill_resume(&plan, &ctx).await
                    } else {
                        executor.run_skill(&plan, &ctx).await
                    }
                    .map_err(|e| AppError::Engine(e.to_string()));
                    let _ = settle_tx.send(SettleReq {
                        project: p,
                        issue: id,
                        outcome: SettleOutcome::Interactive(result),
                    });
                })
            };
            self.state.active_run = Some(ActiveRun {
                project: p,
                issue,
                handle,
                guard,
                finalize: FinalizeCtx {
                    spec,
                    proj: proj.clone(),
                    p,
                    issue_id: Some(id),
                },
                is_resume,
            });
            Ok(())
        } else {
            // ── Inline (examples / headless drivers) ──
            let skill_output = if is_resume {
                executor.run_skill_resume(&plan, &ctx).await
            } else {
                executor.run_skill(&plan, &ctx).await
            }
            .map_err(|e| AppError::Engine(e.to_string()))?;
            // V1 Issue2 Phase2a: interactive path skips issue_run_tail (no
            // MR creation — agent does it in-session; InReview comes from
            // polling). First run: uses + artifacts. Resume: artifacts only
            // (settle-once — uses were bumped on the first run).
            if is_resume {
                self.finalize_run_interactive_resume(&proj, p, Some(id), issue.stage)
                    .await?;
            } else {
                let _ = self
                    .finalize_run_interactive(&spec, &proj, p, Some(id), skill_output)
                    .await?;
            }
            // Guard drops on return (worktree cleanup). Issue stays in its
            // current state (InProgress on first run; unchanged on resume).
            Ok(())
        }
    }

    /// plan/17 S3: settle a backgrounded issue-run on the main thread. Called
    /// by the kernel's settle arm when the spawned round loop reports back.
    /// `take()` is the double-settle guard: a `CancelRun` that already
    /// cleared `active_run` makes a late-arriving completion a no-op (and
    /// vice-versa — the first of cancel/completion to take `active_run` wins,
    /// the other is an honest no-op). `finalize_run` reads `head_after` from
    /// the worktree (guard still alive in `ar`), then the tail's `create_mr`
    /// commits the worktree — guard drops only AFTER both, at the end of
    /// `issue_run_tail`.
    pub async fn run_issue_settle(&mut self, req: SettleReq) -> Result<(), AppError> {
        // 咨询 PTY 退出:不碰 active_run / 状态机 / settle-once。
        if let SettleOutcome::ConsultationEnded { conversation_id } = req.outcome {
            self.state.terminal_manager.close(conversation_id);
            self.clear_focus_if(conversation_id);
            self.clear_restoring_if(conversation_id);
            if let Some(cr) = self.state.consultation_runs.remove(&conversation_id) {
                self.forget_interactive_session(cr.issue_id);
                drop(cr.guard);
            }
            self.emit(Event::IssuesChanged);
            return Ok(());
        }

        let Some(ar) = self.state.active_run.take() else {
            // active_run 空:要么早已 settle(no-op),要么是降级交付的 PTY
            // 退出(spawn 闭包发的是 Interactive 不是 ConsultationEnded)。
            // 查 consultation_runs:在则按咨询退出收尾(降级时已记账,
            // skill_output 忽略);不在则真 no-op。
            self.cleanup_demoted_consultation(req.issue).await;
            return Ok(());
        };
        if ar.issue.id != req.issue || ar.project != req.project {
            // 别的 issue 的活(新交付起来了)—— restore,别误清。但若这个
            // straggler 是降级交付退出(req.issue 在 consultation_runs 里),
            // 顺带清掉它的咨询记录(PTY 已死,guard 该 drop)。
            self.state.active_run = Some(ar);
            self.cleanup_demoted_consultation(req.issue).await;
            return Ok(());
        }
        // V1 收口:issue ▶跑 全走嵌入终端后,settle 只剩 Interactive(交付)与
        // ConsultationEnded(咨询,上方已早返回)。First run: `finalize_run_interactive`
        // (uses + artifacts)。Resume: `finalize_run_interactive_resume` (artifacts
        // only, settle-once)。无 `issue_run_tail`:MR 由 agent 自提、InReview 由
        // `poll_interactive_inreview` 检测;无 MR 的活诚实停在 InProgress,不假装
        // 前进(铁律:Done 永不自动)。
        let outcome = match req.outcome {
            SettleOutcome::Interactive(Ok(skill_output)) => {
                if ar.is_resume {
                    self.finalize_run_interactive_resume(
                        &ar.finalize.proj,
                        ar.finalize.p,
                        ar.finalize.issue_id,
                        ar.issue.stage,
                    )
                    .await
                    .map(|_| RunOutcome::Completed)
                } else {
                    self.finalize_run_interactive(
                        &ar.finalize.spec,
                        &ar.finalize.proj,
                        ar.finalize.p,
                        ar.finalize.issue_id,
                        skill_output,
                    )
                    .await
                }
            }
            SettleOutcome::Interactive(Err(e)) => Err(e),
            SettleOutcome::ConsultationEnded { .. } => unreachable!("handled above"),
        };
        // No `issue_run_tail` — the agent creates the MR in-session and the
        // InReview poller detects it. Guard drops here (worktree cleanup); the
        // issue stays in its current state (InProgress on first run, unchanged
        // on resume). Never auto-Done. 只关本件交付 PTY,不伤后台咨询。
        if let Ok(Some(c)) = self.store.get_conversation_by_issue(ar.issue.id).await {
            self.state.terminal_manager.close(c.id);
            self.clear_focus_if(c.id);
            self.clear_restoring_if(c.id);
        }
        self.forget_interactive_session(ar.issue.id);
        drop(ar.guard);
        self.refresh_issues().await?;
        self.emit(Event::IssuesChanged);
        outcome.map(|_| ())
    }

    /// plan/17 S3 (① 中止): abort the in-flight backgrounded interactive run
    /// on an issue. `abort` the `JoinHandle` —— future 被丢弃时 `claude` 子进程
    /// 也随之收尾:非 PTY 的 `run_skill` 路径靠 `tokio::process` 的
    /// `kill_on_drop`;PTY 路径 Windows 靠 conpty-oxide 托管会话 kill-on-drop、
    /// Unix 靠 `pty_backend::unix::ChildGuard` 的 `Drop` 按进程组杀(2026-08-17
    /// 评审前 Unix 这条缺失,中止一次漏一个孤儿 `claude`;`pty_smoke -- --abort`
    /// 读回为证)—— then close the PTY + drop the worktree guard. The
    /// issue STAYS `InProgress` (retryable), `settled_at` stays empty (no
    /// auto-Done, 铁律). No `finalize_run` accounting: agent/skill `uses` are NOT
    /// bumped — a cancelled run did not complete, so not counting it as a use
    /// is the honest call (偏差 vs the normal `Failed` path which does
    /// finalize with `run_ok=false`; noted in the commit). A normal
    /// completion that already sent a settle req is a no-op here (`active_run`
    /// already taken / being taken by the settle arm).
    pub(crate) async fn cancel_run(&mut self, id: IssueId) -> Result<(), AppError> {
        let Some(ar) = self.state.active_run.take() else {
            return Ok(()); // no in-flight run — nothing to cancel
        };
        if ar.issue.id != id {
            // Wrong issue — restore and leave it in flight.
            self.state.active_run = Some(ar);
            return Ok(());
        }
        ar.handle.abort(); // drop the spawned run future → 子进程随之收尾(见函数文档)
                           // 只关本件交付 PTY,不伤其它会话。
        if let Ok(Some(c)) = self.store.get_conversation_by_issue(ar.issue.id).await {
            self.state.terminal_manager.close(c.id);
            self.clear_focus_if(c.id);
            self.clear_restoring_if(c.id);
        }
        self.forget_interactive_session(ar.issue.id);
        self.emit(Event::WorkflowFailed(format!(
            "Issue #{} 已中止",
            ar.issue.number
        )));
        // The abort already succeeded — `active_run` was cleared by `take()`
        // above and the guard will clean the worktree when `ar` drops at fn
        // exit. A `refresh_issues` failure here is NOT the user's cancel
        // failing (and `state.issues` already shows InProgress, which is the
        // honest post-cancel state), so swallow it rather than surface a
        // misleading "cancel failed" toast.
        let _ = self.refresh_issues().await;
        self.emit(Event::IssuesChanged);
        // `ar` drops here → guard removes the worktree. The pushed branch
        // stays on `origin` if `stage_commit_push` had already run, but a
        // mid-run abort usually hadn't — either way no fabrication.
        Ok(())
    }
}
