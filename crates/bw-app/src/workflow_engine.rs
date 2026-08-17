//! 旧的聊天式工作流执行引擎(`RunWorkflow`/`RunHubWorkflow` → phase 循环 → session/message)。
//! 退役计划见 docs/BACKLOG.md 第 1 条。从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    /// Shared by `Command::RunWorkflow`, `Command::RunHubWorkflow`,
    /// `Command::RunStagePlaybook` and `tick_scheduler`'s real auto-fire —
    /// they differ only in how `spec` was obtained (a hub lookup + a `uses`
    /// bump) and look identical once they have one.
    /// `project` is explicit (not read off `self.state.active_project`) so a
    /// background scheduler fire can run a workflow against its *bound*
    /// project without touching — let alone hijacking — whatever project the
    /// user currently has open.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_workflow_inner(
        &mut self,
        project: ProjectId,
        session: SessionId,
        spec: WorkflowSpec,
        trigger: RunTrigger,
        cron_task_id: Option<CronTaskId>,
        issue_id: Option<IssueId>,
        // plan/17 S2: isolated per-issue worktree for the executor + evidence
        // + PR tail. None = main workspace (pre-S2 behavior; every caller
        // except run_issue_body passes None). run_issue_body passes its
        // provisioned IssueWorktreeGuard's path so the agent works + commits
        // in isolation, decoupling InReview-window collisions.
        issue_worktree: Option<&Path>,
    ) -> Result<RunOutcome, AppError> {
        // plan/17 S3: this is now a thin inline composition of three stages —
        // `prepare_run` (起手, `&self` only) → `run_round_loop` (the long
        // adversarial loop, self-contained, `tokio::spawn`-able) →
        // `finalize_run` (收尾, `&mut self`). Inline callers (cron / stage
        // playbook / creation drafting) await all three in sequence here,
        // byte-for-byte the pre-S3 behavior. The issue path instead spawns
        // `run_round_loop` and defers `finalize_run` to `run_issue_settle`
        // (see `run_issue_now`'s `settle_tx` branch) — same three stages, a
        // different execution topology. `mut spec` moved into `prepare_run`,
        // which owns the skills-prompt mutation.
        let prep = self
            .prepare_run(
                project,
                session,
                spec,
                trigger,
                cron_task_id,
                issue_id,
                issue_worktree,
            )
            .await?;
        let (end, last_run_log, final_run_ok) = Self::run_round_loop(
            self.store.clone(),
            self.events.clone(),
            &prep.engine,
            &prep.spec,
            &prep.ctx,
            prep.session,
            prep.p,
            prep.issue_id,
            prep.trigger,
            prep.cron_task_id,
            &prep.params_json,
            prep.eval_idx,
            prep.num_phases,
            prep.max_iter,
            prep.range_end,
        )
        .await?;
        self.finalize_run(
            &prep.spec,
            &prep.heads_workspace,
            &prep.head_before,
            &prep.proj,
            prep.p,
            prep.issue_id,
            end,
            last_run_log,
            final_run_ok,
        )
        .await
    }

    /// plan/17 S3: the 起手 stage of [`run_workflow_inner`] — everything up to
    /// (but not including) the round loop. `&self` only (every touch here is a
    /// shared borrow: `self.store` / `self.skills_prompt_block` /
    /// `self.resolve_agent_route` / `self.emit` / `self.state.claude_config`),
    /// which is precisely why the long round loop could be peeled off without
    /// needing `&mut self` (the comment that has lived at the old engine-
    /// selection site since iter 3). Returns an owned [`PreparedRun`] so the
    /// issue path can move `engine` + `spec` into a `tokio::spawn`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn prepare_run(
        &self,
        project: ProjectId,
        session: SessionId,
        mut spec: WorkflowSpec,
        trigger: RunTrigger,
        cron_task_id: Option<CronTaskId>,
        issue_id: Option<IssueId>,
        issue_worktree: Option<&Path>,
    ) -> Result<PreparedRun, AppError> {
        let p = project;
        let proj = self.store.get_project(p).await?.ok_or(AppError::NotFound)?;

        // Skill refs become *operative* here: for a non-playbook spec (a
        // playbook already bakes its skill bodies into every phase prompt in
        // bw-core), resolve each ref against the Skill Hub and append the
        // real bodies to the shared prompt. Name-only refs with no stored
        // content contribute nothing — never a fabricated placeholder.
        if spec.phase_prompts.is_empty() && !spec.skills.is_empty() {
            let block = self.skills_prompt_block(p, &spec.skills).await?;
            if !block.is_empty() {
                spec.prompt = format!("{}{block}", spec.prompt);
            }
        }

        let ctx = RunCtx {
            project: p,
            workflow: spec.id,
        };

        // Record the run's start *before* the engine runs — so even a crash
        // mid-run leaves an honest "started, never settled" row instead of a
        // fabricated success (iter 1 telemetry foundation). `params_json`
        // snapshots what the spec *was* at run time (phases/loop/agents/
        // skills) — so after a later "优化" changes the spec, history still
        // shows what each past run actually executed (iter 3 param capture).
        // P4: capture the workspace HEAD before the engine touches anything —
        // the "before" half of this run's recorded change window. Mock runs
        // (no workspace) record nothing: no files were ever at stake.
        let heads_workspace = match issue_worktree {
            Some(p) => p.to_string_lossy().to_string(),
            None => proj.workspace_path.trim().to_string(),
        };
        let head_before = if heads_workspace.is_empty() {
            None
        } else {
            evidence::head_commit(&heads_workspace).await.ok().flatten()
        };

        // T6 (plan/12 §3): resolve the executing Agent's CLI + tools BEFORE
        // anything runs — routing is real, not a display label, and the
        // decision must apply identically whether this project runs on the
        // real `ClaudeCliExecutor` or the shared Mock engine (an unsupported
        // CLI is never silently allowed through on a mock project).
        let (agent_cli, agent_tools) = self.resolve_agent_route(issue_id).await?;
        // The literal `--allowedTools` value `ClaudeCliExecutor` would pass —
        // computed here, before any subprocess spawn, so it's recorded in
        // `params_json` independent of whether the real `claude -p` call
        // ever succeeds (gateway 抖动 is never a verification gate).
        let allowed_tools = allowed_tools_arg(&agent_tools, proj.allow_commands);

        let params_json = run_params_snapshot(
            &spec,
            trigger,
            &agent_cli,
            &agent_tools,
            allowed_tools.as_deref(),
        );

        // The review gate: the FIRST Evaluator phase (T8's real `role`, not a
        // name guess). A workflow with none is a straight pipeline — one round,
        // all phases, byte-for-byte the pre-T9 behavior. (A single review gate
        // per workflow is all the built-in playbooks model today; a second
        // Evaluator, if authored, runs as a plain tail phase.)
        let eval_idx = spec
            .phases
            .iter()
            .position(|ph| ph.role == PhaseRole::Evaluator);
        let num_phases = spec.phases.len();
        let max_iter = spec.loop_config.max_iter.max(1) as u32;
        let range_end = match eval_idx {
            Some(e) => e + 1, // through the gate, inclusive
            None => num_phases,
        };

        // Announce once, before the first round — real name/agents/skills off
        // `spec`, so a live subscriber can render "this run uses X/Y".
        self.emit(Event::RunStarted {
            workflow_name: spec.name.clone(),
            agents: spec.agents.clone(),
            skills: spec.skills.clone(),
        });

        // `workspace_path` is per-project runtime data, not baked into a
        // long-lived Engine: unconfigured projects run on the shared Mock engine
        // (byte-for-byte today's behavior); a configured one gets a fresh
        // one-shot real executor for THIS call (shared across the call's rounds).
        // Held immutably across the loop — every in-loop `self` touch is a shared
        // borrow (`self.store` / `self.emit`), never `&mut self`, so this holds.
        //
        // plan/17 S3: `Engine` is now `Clone`; `prepare_run` returns an OWNED
        // engine so the issue path can move it into `tokio::spawn`. The mock
        // path clones the shared `mock_engine`'s `Arc` (cheap); the real path
        // builds the same fresh one-shot executor, just owned rather than
        // borrowed. Inline path passes it by `&` into `run_round_loop`.
        //
        // T6 (plan/12 §3): the `agent_cli` match happens FIRST, before the
        // mock/real branch below — an unsupported
        // CLI ("codex"/"cursor"/…) routes to the honest `UnsupportedCliExecutor`
        // regardless of whether this project even has a real workspace
        // configured. Only `"claude-code"` (the default for an unassigned
        // issue or any other caller) reaches the existing mock-vs-real split,
        // unchanged.
        let engine: Engine = match agent_cli.as_str() {
            "claude-code" => {
                if proj.workspace_path.trim().is_empty() {
                    self.mock_engine.clone()
                } else {
                    let executor = ClaudeCliExecutor::new(
                        self.state.claude_config.clone(),
                        issue_worktree
                            .map(PathBuf::from)
                            .unwrap_or_else(|| PathBuf::from(proj.workspace_path.trim())),
                        proj.allow_commands,
                        agent_tools.clone(),
                    );
                    Engine::new(Arc::new(executor))
                }
            }
            other => {
                // 诚实报错,绝不静默回落到 claude-code:本机没有为 codex/cursor
                // 等值接好真实执行器。Reuses the `Executor` trait seam — this
                // executor's first (and only) call errors, and the existing
                // "executor failed → settle Failed" path records it honestly.
                let executor = UnsupportedCliExecutor::new(other.to_string());
                Engine::new(Arc::new(executor))
            }
        };

        Ok(PreparedRun {
            engine,
            spec,
            ctx,
            params_json,
            eval_idx,
            num_phases,
            max_iter,
            range_end,
            heads_workspace,
            head_before,
            proj,
            p,
            session,
            issue_id,
            trigger,
            cron_task_id,
        })
    }

    /// plan/17 S3: the long adversarial round loop, extracted verbatim from
    /// `run_workflow_inner` so it can run on a `tokio::spawn` with NO `&mut
    /// App` — it only ever touched `self.store` / `self.emit` / `engine` /
    /// `live`, all shared borrows or owned/move-able. `self.emit(X)` became
    /// `live.send(X)` (emit IS `self.events.send`, and `live` is exactly
    /// `self.events.clone()`). `self.store` became the `store: Arc<dyn
    /// Store>` param. Returns the `LoopEnd` + the last round's
    /// `workflow_run` id + `final_run_ok`; `Err` is a `?`-early-bail (a
    /// store error before any settle — the round's row stays "started,
    /// never settled", honest, never a fabricated success).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_round_loop(
        store: Arc<dyn Store>,
        live: broadcast::Sender<Event>,
        engine: &Engine,
        spec: &WorkflowSpec,
        ctx: &RunCtx,
        session: SessionId,
        p: ProjectId,
        issue_id: Option<IssueId>,
        trigger: RunTrigger,
        cron_task_id: Option<CronTaskId>,
        params_json: &str,
        eval_idx: Option<usize>,
        num_phases: usize,
        max_iter: u32,
        range_end: usize,
    ) -> Result<(LoopEnd, WorkflowRunId, bool), AppError> {
        // ── Adversarial review loop (plan/12 §4, T9) ────────────────────────
        // Each round is its OWN settled `workflow_run` row: "多轮 run 记录" reads
        // back as multiple rows, and settle-once holds because each row is
        // settled exactly once. Round 1 runs from phase 0; each Evaluator打回
        // restarts from the reject target and increments the round.
        let mut start = 0usize;
        let mut round: u32 = 1;
        let mut baton: Option<String> = None;
        // Set at the top of every round (before any `break`), so it is
        // definitely-assigned for the after-loop accounting — the last round's
        // row is the one that produced the final state.
        let mut last_run_log: WorkflowRunId;
        let mut final_run_ok = false;

        let end: LoopEnd = loop {
            // Record this round's row start *before* the engine runs — a crash
            // mid-round leaves an honest "started, never settled" row, never a
            // fabricated success.
            let started_at = OffsetDateTime::now_utc().unix_timestamp();
            let t0 = Instant::now();
            let run_log_id = store
                .record_workflow_run_start(bw_store::NewWorkflowRun {
                    workflow_id: spec.id,
                    workflow_name: &spec.name,
                    project_id: Some(p),
                    session_id: Some(session),
                    trigger,
                    started_at,
                    cron_task_id,
                    params_json,
                })
                .await?;
            // A3: bind this round's run to the Issue it executes (RunIssue passes
            // Some; every other caller None). Every round of an issue-run is
            // bound, so `list_runs_for_issue` reads the whole loop back.
            if let Some(iid) = issue_id {
                store.set_run_issue(run_log_id, iid).await?;
            }
            last_run_log = run_log_id;

            // Execute this round's phase range: through the gate for a gated
            // workflow, or all phases for an ungated one. Outputs come back on
            // the return value; live events stream via the callback.
            let range_res = engine
                .run_phase_range(spec, ctx, start..range_end, baton.clone(), |e| {
                    forward_progress(&live, e)
                })
                .await;

            let finished_at = OffsetDateTime::now_utc().unix_timestamp();
            let duration_ms = t0.elapsed().as_millis() as i64;

            let outputs = match range_res {
                Ok(o) => o,
                Err(e) => {
                    // Honest executor failure — settle Failed, stop the loop.
                    store
                        .settle_workflow_run(
                            run_log_id,
                            RunStatus::Failed,
                            finished_at,
                            duration_ms,
                            0,
                            &e.to_string(),
                        )
                        .await?;
                    break LoopEnd::Failed(AppError::Engine(e.to_string()));
                }
            };

            // Persist this round's phase outputs as session messages (每阶段留痕).
            let phases_completed = outputs.len() as u32;
            for output in &outputs {
                store
                    .append_message(session, Author::Agent, &output.text)
                    .await?;
                let _ = live.send(Event::SessionMessageAdded {
                    session,
                    role: Author::Agent,
                    text: output.text.clone(),
                });
            }

            // Ungated pipeline: this single round is the whole run.
            let Some(e_idx) = eval_idx else {
                store
                    .settle_workflow_run(
                        run_log_id,
                        RunStatus::Ok,
                        finished_at,
                        duration_ms,
                        phases_completed,
                        "",
                    )
                    .await?;
                let _ = live.send(Event::WorkflowDone);
                final_run_ok = true;
                break LoopEnd::Passed;
            };

            // Parse the gate's real verdict from its output (the range's last
            // phase). No parseable verdict = honest review failure, NEVER a
            // default pass (plan/12 §4).
            let eval_text = outputs.last().map(|o| o.text.clone()).unwrap_or_default();
            let Some(outcome) = parse_phase_outcome(&eval_text) else {
                let msg = format!(
                    "评审输出缺结构化裁决(阶段「{}」· 轮次 {round}/{max_iter}):{}",
                    spec.phases[e_idx].name,
                    review_tail(&eval_text)
                );
                store
                    .settle_workflow_run(
                        run_log_id,
                        RunStatus::Failed,
                        finished_at,
                        duration_ms,
                        phases_completed,
                        &msg,
                    )
                    .await?;
                break LoopEnd::Failed(AppError::Engine(msg));
            };

            match outcome.verdict {
                Verdict::Pass => {
                    // Gate passed. Run any phases AFTER the gate (built-ins have
                    // none) in order — a genuine pass proceeds — then settle Ok.
                    let mut total = phases_completed;
                    if e_idx + 1 < num_phases {
                        let tail_res = engine
                            .run_phase_range(
                                spec,
                                ctx,
                                (e_idx + 1)..num_phases,
                                Some(review_tail(&eval_text)),
                                |e| forward_progress(&live, e),
                            )
                            .await;
                        match tail_res {
                            Ok(tail) => {
                                for output in &tail {
                                    store
                                        .append_message(session, Author::Agent, &output.text)
                                        .await?;
                                    let _ = live.send(Event::SessionMessageAdded {
                                        session,
                                        role: Author::Agent,
                                        text: output.text.clone(),
                                    });
                                }
                                total += tail.len() as u32;
                            }
                            Err(e) => {
                                store
                                    .settle_workflow_run(
                                        run_log_id,
                                        RunStatus::Failed,
                                        OffsetDateTime::now_utc().unix_timestamp(),
                                        t0.elapsed().as_millis() as i64,
                                        phases_completed,
                                        &e.to_string(),
                                    )
                                    .await?;
                                break LoopEnd::Failed(AppError::Engine(e.to_string()));
                            }
                        }
                    }
                    store
                        .settle_workflow_run(
                            run_log_id,
                            RunStatus::Ok,
                            OffsetDateTime::now_utc().unix_timestamp(),
                            t0.elapsed().as_millis() as i64,
                            total,
                            "",
                        )
                        .await?;
                    let _ = live.send(Event::WorkflowDone);
                    final_run_ok = true;
                    break LoopEnd::Passed;
                }
                Verdict::RejectToPhase(proposed) => {
                    // Effective reject target: a declared `reject_to_phase`
                    // (Static track) wins and the agent's proposal is IGNORED; an
                    // undeclared one (Dynamic track) honours the agent's proposal.
                    let target = match spec.phases[e_idx].reject_to_phase {
                        Some(t) => t as usize,
                        None => proposed as usize,
                    };
                    let reason = if outcome.reason.trim().is_empty() {
                        "评审未通过".to_string()
                    } else {
                        outcome.reason.clone()
                    };
                    // A reject target must be a real phase strictly before the
                    // gate (loop BACK, not forward/self). Anything else is an
                    // un-actionable verdict → honest failure (never guess).
                    if target >= num_phases || target > e_idx {
                        let msg = format!(
                            "评审打回目标越界(阶段索引 {target} / 共 {num_phases} 阶段 · 轮次 {round}/{max_iter}):{reason}"
                        );
                        store
                            .settle_workflow_run(
                                run_log_id,
                                RunStatus::Failed,
                                finished_at,
                                duration_ms,
                                phases_completed,
                                &msg,
                            )
                            .await?;
                        break LoopEnd::Failed(AppError::Engine(msg));
                    }
                    if round >= max_iter {
                        // Cap hit: never auto-Failed, never auto-Done. Settle this
                        // round Failed with the cap reason; hand a Blocked outcome
                        // up (a bound Issue is parked Blocked by the caller).
                        let cap_reason = format!("对抗循环 {round}/{max_iter} 仍未通过:{reason}");
                        store
                            .settle_workflow_run(
                                run_log_id,
                                RunStatus::Failed,
                                finished_at,
                                duration_ms,
                                phases_completed,
                                &cap_reason,
                            )
                            .await?;
                        break LoopEnd::Blocked(cap_reason);
                    }
                    // Loop back: settle this round Failed (deliverable rejected),
                    // carry the reject feedback forward as the next round's baton
                    // (the regenerating phase sees WHY), restart from the target.
                    let row_msg = format!(
                        "评审打回阶段「{}」(轮次 {round}/{max_iter}):{reason}",
                        spec.phases[target].name
                    );
                    store
                        .settle_workflow_run(
                            run_log_id,
                            RunStatus::Failed,
                            finished_at,
                            duration_ms,
                            phases_completed,
                            &row_msg,
                        )
                        .await?;
                    baton = Some(review_tail(&eval_text));
                    start = target;
                    round += 1;
                }
            }
        };

        Ok((end, last_run_log, final_run_ok))
    }

    /// plan/17 S3: the 收尾 stage of [`run_workflow_inner`] — change window +
    /// usage accounting, ONCE per run, attributed to the LAST round's row.
    /// Extracted verbatim from the old inline tail; `&mut self` so it can run
    /// on the main thread (it touches `refresh_agents` / `refresh_skills` /
    /// `scan_and_register_artifacts` — the only `&mut self` parts of the whole
    /// run). Takes individual refs (not a struct) so the inline path can pass
    /// them straight out of `PreparedRun` and the backgrounded path out of
    /// `FinalizeCtx` — no clone needed in either. Inline callers run it right
    /// after `run_round_loop`; the issue path runs it inside `run_issue_settle`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finalize_run(
        &mut self,
        spec: &WorkflowSpec,
        heads_workspace: &str,
        head_before: &Option<String>,
        proj: &ProjectRow,
        p: ProjectId,
        issue_id: Option<IssueId>,
        end: LoopEnd,
        last_run_log: WorkflowRunId,
        final_run_ok: bool,
    ) -> Result<RunOutcome, AppError> {
        // Attributed to the LAST round's row (the one that produced the final
        // state). Runs on every terminal outcome (pass / block / honest failure)
        // — a failed run's partial real output is still real output. Doing this
        // once per issue-run (not per round) keeps agent win_rate / skill `uses`
        // honest: one real work item = one agent run, one skill use.
        let run_ok = final_run_ok;
        let run_log_id = last_run_log;
        if !heads_workspace.is_empty() {
            let head_after = evidence::head_commit(heads_workspace).await.ok().flatten();
            self.store
                .set_run_heads(run_log_id, head_before.clone(), head_after)
                .await?;
        }
        // plan/20 R3: 记账行 == 注入行——用与注入完全同一条就近规则
        // (`scope::scoped_pick`,项目行遮蔽全局行、他项目行永不命中)解析
        // 出这次 run 实际采用的那一行,按 id 打点;解析落空(未登记的
        // ad-hoc ref)如实跳过。此前的按名全表 UPDATE 会把跨作用域同名行
        // (W1 起每个项目都有五角色副本)齐 bump,战绩互相污账。
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
        // versions against the final round's run. Scan errors are a 0-fresh
        // no-op — they never turn a settled run into an error.
        if !proj.workspace_path.trim().is_empty() {
            let stage_kind = spec
                .stage_ref
                .and_then(|n| StageKind::ALL.into_iter().find(|s| s.index() == n));
            if let Ok(fresh) = self
                .scan_and_register_artifacts(
                    p,
                    &proj.workspace_path,
                    Some(run_log_id),
                    stage_kind,
                    // A2: run-time issue归属 — the活's产物 bind to both run
                    // and issue so the Done edge's idempotent re-scan matches.
                    issue_id,
                )
                .await
            {
                if fresh > 0 {
                    self.emit(Event::ArtifactsRegistered { fresh });
                }
            }
        }

        // The after-loop accounting above runs on EVERY terminal outcome
        // (pass / block / honest failure) — a failed run's partial real
        // output is still real output. Only now does the outcome surface:
        // `Failed` propagates as `Err` so the caller's tail (issue stays
        // InProgress) mirrors the inline `?`-early-bail path.
        match end {
            LoopEnd::Passed => Ok(RunOutcome::Completed),
            LoopEnd::Blocked(reason) => Ok(RunOutcome::BlockedAtCap { reason }),
            LoopEnd::Failed(err) => Err(err),
        }
    }

    /// T6 (plan/12 §3): resolve which Agent CLI executes an issue-run and
    /// what `tools` (AllowedTools) it declares. Only `RunIssue` has a
    /// concrete assignee to route by — an issue with no assignee, an
    /// assignee row that's since been deleted, or a blank `agent_cli`
    /// (the five built-in stage-role rows) all read back as the honest
    /// default: `"claude-code"` with no tools restriction, byte-for-byte
    /// every other caller's (`RunHubWorkflow`, cron, stage playbook without
    /// an issue) pre-T6 behavior.
    pub(crate) async fn resolve_agent_route(
        &self,
        issue_id: Option<IssueId>,
    ) -> Result<(String, Vec<String>), AppError> {
        const DEFAULT_CLI: &str = "claude-code";
        let Some(iid) = issue_id else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let Some(issue) = self.store.get_issue(iid).await? else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let Some(agent_id) = issue.assignee else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let Some(agent) = self.store.get_agent(agent_id).await? else {
            return Ok((DEFAULT_CLI.to_string(), Vec::new()));
        };
        let cli = if agent.agent_cli.trim().is_empty() {
            DEFAULT_CLI.to_string()
        } else {
            agent.agent_cli.clone()
        };
        Ok((cli, agent.tools.clone()))
    }

    /// T17 (plan/12 §10 v1.1#4): `Command::ParseWorkflowContent`'s real work —
    /// see that variant's doc comment for the full contract. One-shot (no
    /// session, no `workflow_run` row — this isn't a workflow *run*, it's a
    /// single document-understanding call), routed through the same
    /// mock/real `Engine` split `run_workflow_inner` uses, keyed off the
    /// workflow's OWN `project_id` binding rather than `self.active()`:
    /// this is a Hub-reachable action (`BW_SEL` can deep-link straight into
    /// a `ComponentDetail` with no project open at all), so it must not
    /// require one. A hub-library workflow (`project_id: None`) — the
    /// common case, every built-in template and every hand-authored hub
    /// entry today — always runs on the shared Mock engine, honestly
    /// self-labelled; a project-owned workflow with a configured
    /// `workspace_path` gets a real one-shot `ClaudeCliExecutor`, exactly
    /// like a real workflow run would.
    pub(crate) async fn parse_workflow_content(
        &mut self,
        workflow_id: WorkflowId,
    ) -> Result<(), AppError> {
        let spec = self
            .store
            .get_workflow_spec(workflow_id)
            .await?
            .ok_or(AppError::NotFound)?;
        if spec.content.trim().is_empty() {
            return Err(AppError::Invalid(
                "没有原始文档,无可解析(content 为空)".into(),
            ));
        }

        // No issue is involved in a Hub-level parse call, so this always
        // resolves to the honest default ("claude-code", no tools) unless a
        // future caller threads one through — kept as a real call (not a
        // hardcoded literal) so a later change to the default lands here
        // for free, same discipline `run_workflow_inner` follows.
        let (agent_cli, agent_tools) = self.resolve_agent_route(None).await?;
        let fresh_engine;
        let engine: &Engine = match agent_cli.as_str() {
            "claude-code" => {
                let real_workspace = match spec.project_id {
                    Some(pid) => self
                        .store
                        .get_project(pid)
                        .await?
                        .map(|p| (p.workspace_path, p.allow_commands)),
                    None => None,
                };
                match real_workspace {
                    Some((path, allow_commands)) if !path.trim().is_empty() => {
                        let executor = ClaudeCliExecutor::new(
                            self.state.claude_config.clone(),
                            PathBuf::from(path.trim()),
                            allow_commands,
                            agent_tools.clone(),
                        );
                        fresh_engine = Engine::new(Arc::new(executor));
                        &fresh_engine
                    }
                    _ => &self.mock_engine,
                }
            }
            other => {
                // 诚实报错,同 `run_workflow_inner`:不给未接好的 CLI 静默
                // 回落到 claude-code。
                let executor = UnsupportedCliExecutor::new(other.to_string());
                fresh_engine = Engine::new(Arc::new(executor));
                &fresh_engine
            }
        };

        let mut prompt = spec.content.clone();
        prompt.push_str(workflow_parse_contract_suffix());
        let node = PhaseNode {
            name: "解析工作流文档".to_string(),
            role: PhaseRole::Neutral,
            prompt,
            agents: spec.agents.clone(),
            skills: spec.skills.clone(),
            max_iter: 1,
            retries: 0,
            prior_summary: None,
        };
        let ctx = RunCtx {
            project: spec.project_id.unwrap_or(ProjectId::nil()),
            workflow: spec.id,
        };
        let output = engine
            .run_adhoc(node, &ctx)
            .await
            .map_err(|e| AppError::Engine(e.to_string()))?;

        // Honest parse: no keyword guessing, no partial adoption. On ANY
        // problem `phases` stays exactly what it was before this call — the
        // caller (UI) shows the real reason and lets the user retry.
        let phases = parse_workflow_phases(&output.text).map_err(AppError::Invalid)?;

        self.store
            .update_workflow_spec(
                workflow_id,
                WorkflowEdit {
                    prompt: spec.prompt.clone(),
                    goal: spec.goal.clone(),
                    phases,
                    // A freshly-parsed phase carries its own real per-phase
                    // binding (name/role/reject_to_phase/agent/skills) but
                    // no per-phase INSTRUCTION text — same "empty = shared
                    // `prompt`" fallback every other phase-editing path
                    // already honors (`UpdateWorkflowSpec`'s hand-edit form
                    // included). A per-phase-instruction authoring UI is
                    // later work, not this ticket.
                    phase_prompts: Vec::new(),
                    agents: spec.agents.clone(),
                    skills: spec.skills.clone(),
                    note: "T17 · 解析为流程图".to_string(),
                },
            )
            .await?;
        self.refresh_workflow_specs().await?;
        self.emit(Event::WorkflowSpecsChanged);
        Ok(())
    }

    /// **The self-driving optimization loop (iter 18).** Runs the full
    /// measure→propose→gate cycle over every hub workflow, once. This is the
    /// engine the goal asked for: "通过不断的执行 schedule 的 workflow 来优化
    /// workflow 本身" — a cron task can fire this on a cadence (iter 22 wires
    /// that) so the hub keeps optimizing *itself* without a click.
    ///
    /// What it does, per workflow:
    ///   1. **Measure** — fetch real analytics + usage rank + the run log +
    ///      cron effectiveness (every number read from the store, none
    ///      invented).
    ///   2. **Propose** — `analysis::propose_optimizations` turns the evidence
    ///      into ranked, grounded suggestions.
    ///   3. **Gate** — `analysis::review_proposal` decides AutoApply /
    ///      DeferToHuman / Reject under the default policy (the autonomy dial).
    ///      Only the *positive* kind auto-applies; everything content-changing
    ///      or destructive defers to a human.
    ///   4. **Report** — returns what was considered, what was auto-applied,
    ///      what needs a human. Emits `OptimizationCycleReported`.
    ///
    /// It deliberately does **not** rewrite specs or retire workflows on its
    /// own — that's the safety design from iter 13. The loop's autonomy is
    /// bounded: it measures relentlessly, proposes honestly, and acts only on
    /// the safe-positive.
    pub async fn run_optimization_cycle(&mut self) -> Result<OptimizationReport, AppError> {
        use bw_core::analysis::{propose_optimizations, review_proposal, ApplyPolicy};

        let policy = ApplyPolicy::default();
        let specs = self.store.list_workflow_specs().await?;
        let ranking = self.store.hub_usage_ranking().await?;
        let cron_tasks = self.store.list_cron_tasks().await?;
        let mut scanned = 0u32;
        let mut proposals = 0u32;
        let mut auto_applied = Vec::new();
        let mut defer_to_human = Vec::new();
        let mut rejected = 0u32;

        for spec in &specs {
            scanned += 1;
            let mut analytics = self.store.workflow_analytics(spec.id).await?;
            // A cold workflow has no runs, so analytics.workflow_name reads
            // back empty — fill it from the spec so proposals name it honestly.
            if analytics.workflow_name.is_empty() {
                analytics.workflow_name = spec.name.clone();
            }
            let usage = ranking
                .iter()
                .find(|r| r.workflow_id == spec.id)
                .cloned()
                .unwrap_or_else(|| bw_core::model::UsageRank {
                    workflow_id: spec.id,
                    workflow_name: spec.name.clone(),
                    stage_ref: spec.stage_ref,
                    total_runs: 0,
                    ok_runs: 0,
                    failed_runs: 0,
                    success_rate: None,
                    last_run_at: None,
                    cold: true,
                });
            let runs = self.store.list_workflow_runs(spec.id).await?;
            let failures = bw_core::analysis::failure_modes(&runs);
            // Cron effectiveness: a task targeting this workflow contributes
            // its real scheduled-fire track record to the proposal inputs.
            let cron_eff = match cron_tasks.iter().find(|c| c.target == spec.name) {
                Some(c) => Some(self.store.cron_effectiveness(c.id).await?),
                None => None,
            };
            let ps = propose_optimizations(&analytics, &usage, &failures, cron_eff.as_ref());
            for p in ps {
                proposals += 1;
                let settled = analytics.ok_runs + analytics.failed_runs;
                match review_proposal(&p, settled, &policy) {
                    bw_core::analysis::ApplyDecision::AutoApply => {
                        auto_applied.push(p.title);
                    }
                    bw_core::analysis::ApplyDecision::DeferToHuman(_) => {
                        defer_to_human.push(p.title);
                    }
                    bw_core::analysis::ApplyDecision::Reject(_) => {
                        rejected += 1;
                    }
                }
            }
        }
        let report = OptimizationReport {
            scanned,
            proposals,
            auto_applied,
            defer_to_human,
            rejected,
        };
        self.emit(Event::OptimizationCycleReported {
            report: report.clone(),
        });
        Ok(report)
    }
}
