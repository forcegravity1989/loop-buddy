//! 内嵌终端与会话焦点:PTY 字节抽干、hook 事件、会话↔Issue 对应、评审中轮询、咨询态。
//! 从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    /// V1 Issue2 Phase2b: drain pending hook events from the hook listener
    /// (localhost HTTP server). Called from `tick_scheduler` (and can be
    /// called from the desktop kernel's `select!` loop for more prompt
    /// processing). Processes:
    ///  - **SessionStart**: look up the issue by `cwd` (via the
    ///    `interactive_sessions` map) and store `session_id` via
    ///    `store.set_conversation_session_id` (writes the `claude_conversation`
    ///    row, not the retired `issue` columns). This is the F1 fix: only when
    ///    the hook fires (session really established) is the session_id
    ///    stored — an empty session_id means the first spawn failed, and the
    ///    next ▶ run falls back to `build_startup_plan`.
    ///  - **Stop**: set `pending_stop_check = true` so the next
    ///    `tick_scheduler` fire runs `poll_interactive_inreview` (the Stop
    ///    event is the real-time trigger; the tick's cadence is the natural
    ///    throttle — no additional debounce needed within a single tick;
    ///    the 5-minute poller remains as backstop).
    ///
    /// Best-effort: a failed store write (session_id) or a missing cwd →
    /// issue mapping is silently skipped (logged via toast). Never blocks
    /// the tick scheduler's cron-fired list from returning.
    pub(crate) async fn poll_hook_events(&mut self) -> Result<(), AppError> {
        // Drain all pending events first (releases the mutable borrow on
        // `hook_event_rx` before we call `self.emit` / `self.store` below).
        let events: Vec<hook_listener::HookEvent> = {
            let Some(rx) = self.hook_event_rx.as_mut() else {
                return Ok(()); // no listener running
            };
            let mut events = Vec::new();
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
            events
        };
        for event in events {
            match event {
                hook_listener::HookEvent::SessionStart { session_id, cwd } => {
                    // Look up the issue by cwd (the worktree path the
                    // session was spawned in).
                    if let Some(&issue_id) = self.state.interactive_sessions.get(&cwd) {
                        if let Err(e) = self
                            .store
                            .set_conversation_session_id(issue_id, &session_id)
                            .await
                        {
                            self.emit(Event::ConnectorSynced {
                                name: "Hook SessionStart".into(),
                                ok: false,
                                detail: format!(
                                    "存储 session_id 失败,该活的 resume 仍用 fallback:{e}"
                                ),
                            });
                        } else {
                            self.emit(Event::ConnectorSynced {
                                name: "Hook SessionStart".into(),
                                ok: true,
                                detail: format!(
                                    "捕获 claude session_id (前8位: {}…) — 下次 ▶跑 用 --resume",
                                    &session_id[..session_id.len().min(8)]
                                ),
                            });
                        }
                    }
                    // cwd not in map → the hook fired for a session buddy
                    // didn't spawn (user's own claude session). Silently
                    // ignore — we only care about buddy-spawned sessions.
                }
                hook_listener::HookEvent::Stop { .. } => {
                    // Mark for InReview check on this tick. The tick's
                    // cadence is the natural throttle (multiple Stop events
                    // in one tick = one check).
                    self.state.pending_stop_check = true;
                }
            }
        }
        Ok(())
    }

    /// V1 终端会话重构·底座: drain 各会话有界缓冲,返回带 conversation_id
    /// 的批次。kernel 的 100ms pty_ticker 调用后按 id 路由到对应 xterm。
    /// 重启恢复:若正在「恢复中」的会话已收到非空字节 → 清 pty_restoring
    /// (PTY 就绪,「恢复中…」消失)。
    pub fn drain_pty_events(&mut self) -> Vec<(ConversationId, Vec<u8>)> {
        let batches = self.state.terminal_manager.drain_events();
        if let Some(rid) = self.state.pty_restoring {
            if batches.iter().any(|(id, b)| *id == rid && !b.is_empty()) {
                self.state.pty_restoring = None;
            }
        }
        batches
    }

    /// 点卡 resume 是否仍在「恢复中」(重建 worktree / spawn / 等首包)。
    pub fn pty_restoring(&self) -> Option<ConversationId> {
        self.state.pty_restoring
    }

    /// 是否有活着的 PTY 连接(UI `pty_active`)。
    pub fn pty_active(&self) -> bool {
        self.state.terminal_manager.has_live()
    }

    /// 焦点会话;无焦点时回落任一活连接。
    pub fn focused_pty_conversation(&self) -> Option<ConversationId> {
        self.state
            .focused_conversation
            .filter(|id| self.state.terminal_manager.is_live(*id))
            .or_else(|| self.state.terminal_manager.live_ids().first().copied())
    }

    pub fn focused_pty_issue(&self) -> Option<IssueId> {
        let cid = self.focused_pty_conversation()?;
        self.state
            .terminal_manager
            .issue_id(cid)
            .or(self.state.focused_issue)
    }

    pub fn live_pty_ids(&self) -> Vec<ConversationId> {
        self.state.terminal_manager.live_ids()
    }

    pub(crate) async fn focus_conversation(
        &mut self,
        conversation_id: ConversationId,
        issue_id: IssueId,
    ) {
        self.state.focused_conversation = Some(conversation_id);
        self.state.focused_issue = Some(issue_id);
        // Bug2(V1-TermFocus): 终端→左——回写 active_session 到该 issue 对应的
        // 阶段记录(按 run_sess_title `#N 标题` 反查 SessionId)。左侧高亮跟着
        // 终端焦点走。找不到(纯阶段记录不挂 issue)则静默跳过。
        self.writeback_active_session(issue_id).await;
    }

    /// Bug2:按 issue 的 run_sess_title(`#N 标题`)在 store 的 session 表里
    /// 反查 SessionId,回写 `active_session`。这样左侧 session 卡高亮跟着
    /// 嵌终端焦点走(终端切焦点 → 左侧高亮同步)。用 `self.state.issues`
    /// 拿 number+title(内存,无需额外查库),再查 session(一次 list_sessions)。
    pub(crate) async fn writeback_active_session(&mut self, issue_id: IssueId) {
        let (p, title) = match self.state.issues.iter().find(|i| i.id == issue_id) {
            Some(issue) => (
                issue.project_id,
                format!("#{} {}", issue.number, issue.title),
            ),
            None => return,
        };
        if let Ok(sessions) = self.store.list_sessions(p).await {
            if let Some(s) = sessions.into_iter().find(|s| s.title == title) {
                self.state.active_session = Some(s.id);
            }
        }
    }

    /// Bug2(V1-TermFocus)+收口补洞:左→终端——解析 session→issue 后走与
    /// issue 看板「▶跑/续聊」相同的 `run_issue_now`(活 PTY 切焦点 /
    /// Done·InReview resume 进 `open_conversation` / 否则起交互式)。
    ///
    /// 旧实现只在「已有 conversation 且(活 PTY|非空 claude_session_id)」时
    /// 动作,缺行或 hook 未回填 session_id 时静默 no-op → 阶段记录高亮了、
    /// 工作流区仍空(看板因直接 `RunIssue` 不受影响)。匹配不到 `#N 标题`
    /// (纯 stage-playbook 阶段记录)→ 保持原行为(只设 active_session)。
    pub(crate) async fn sync_session_to_terminal(
        &mut self,
        sid: SessionId,
    ) -> Result<(), AppError> {
        let Some(issue_id) = self.resolve_issue_for_session(sid).await? else {
            return Ok(());
        };
        self.run_issue_now(sid, issue_id).await
    }

    /// 阶段记录 session.title = `#N 标题`(op.rs `run_sess_title`)。按
    /// number+title 在 `self.state.issues` 匹配;匹配不到 = 不挂 issue 的
    /// 纯阶段循环会话。
    pub(crate) async fn resolve_issue_for_session(
        &self,
        sid: SessionId,
    ) -> Result<Option<IssueId>, AppError> {
        let p = self.active()?;
        let title = self
            .store
            .list_sessions(p)
            .await?
            .into_iter()
            .find(|s| s.id == sid)
            .map(|s| s.title);
        let Some(title) = title else {
            return Ok(None);
        };
        Ok(self
            .state
            .issues
            .iter()
            .find(|i| format!("#{} {}", i.number, i.title) == title)
            .map(|i| i.id))
    }

    pub(crate) fn clear_focus_if(&mut self, conversation_id: ConversationId) {
        if self.state.focused_conversation != Some(conversation_id) {
            return;
        }
        let next = self
            .state
            .terminal_manager
            .live_ids()
            .into_iter()
            .find(|id| *id != conversation_id);
        self.state.focused_conversation = next;
        self.state.focused_issue = next.and_then(|cid| self.state.terminal_manager.issue_id(cid));
    }

    pub(crate) fn clear_restoring_if(&mut self, conversation_id: ConversationId) {
        if self.state.pty_restoring == Some(conversation_id) {
            self.state.pty_restoring = None;
        }
    }

    /// 迁移留空的 workspace_path/branch_name:resume 成功后回填(只填空列)。
    pub(crate) async fn backfill_conversation_workspace_if_empty(
        &self,
        issue_id: IssueId,
        workspace_path: &str,
        branch_name: &str,
    ) -> Result<(), AppError> {
        if workspace_path.is_empty() && branch_name.is_empty() {
            return Ok(());
        }
        self.store
            .update_conversation_workspace_if_empty(issue_id, workspace_path, branch_name)
            .await?;
        Ok(())
    }

    /// V1 Issue2 Phase2b: drop every `cwd → issue` mapping pointing at `id`.
    /// Called when an interactive run settles or is cancelled — the session is
    /// over, so a late (or forged) hook event carrying that cwd must not still
    /// rewrite this issue's `claude_session_id`. Keyed by value, not by cwd,
    /// because the worktree path isn't at hand on every teardown path.
    pub(crate) fn forget_interactive_session(&mut self, id: IssueId) {
        self.state.interactive_sessions.retain(|_, v| *v != id);
    }

    /// V1 Issue2 Phase2a: poll codehub/github for open MRs on interactive
    /// issues that are InProgress + have a claude_conversation row (interactive) + `pr_number == 0`.
    /// When an open MR is found (读回为证: buddy checks the remote itself,
    /// not agent self-report), it backfills `pr_number` and transitions to
    /// InReview (D3: 评审中由存在开放 PR 派生). Never reaches Done — that
    /// edge stays exclusively a human `MergeIssuePr`/`TransitionIssue`.
    ///
    /// Covers BOTH codehub and github (unlike `collect_github_issue_drift`
    /// which is github-only and manual-trigger). Called from
    /// `tick_scheduler`, throttled adaptively (`INREVIEW_POLL_ACTIVE_SECS`
    /// while candidates wait, `INREVIEW_POLL_IDLE_SECS` when idle) so a
    /// just-opened MR lands in InReview in seconds–teens of seconds, not
    /// multi-minute silence. A failed remote query degrades honestly (toast
    /// + skip this issue this round), never fabricating an InReview.
    pub(crate) async fn poll_interactive_inreview(&mut self) -> Result<(), AppError> {
        for proj in self.state.projects.clone() {
            let remote_path = proj.remote_path.trim();
            if remote_path.is_empty() {
                continue; // no remote = no MR to detect
            }
            let remote = match bw_engine::remote::Remote::for_project(
                &proj.provider,
                &proj.remote_host,
                remote_path,
            ) {
                Ok(r) => r,
                Err(e) => {
                    self.emit(Event::ConnectorSynced {
                        name: format!("{} · InReview 轮询", proj.name),
                        ok: false,
                        detail: format!("远端 provider 不可用,跳过:{e}"),
                    });
                    continue;
                }
            };
            // V1 终端会话重构(阶段1): interactive_started 改读 claude_conversation
            // 行存在(一次查全项目 issue_ids,避免 N+1)。
            let conv_ids: std::collections::HashSet<IssueId> = self
                .store
                .list_conversation_issue_ids(proj.id)
                .await?
                .into_iter()
                .collect();
            // Interactive issues in InProgress + 有会话行 + no PR.
            let candidates: Vec<_> = self
                .store
                .list_issues(proj.id, None, Some(IssueStatus::InProgress))
                .await?
                .into_iter()
                .filter(|i| conv_ids.contains(&i.id) && i.pr_number == 0 && i.github_number != 0)
                .collect();
            for issue in candidates {
                let branch = bw_engine::github::issue_branch(issue.github_number);
                let open_mr = match remote.open_mr_for_branch(&branch).await {
                    Ok(v) => v,
                    Err(e) => {
                        self.emit(Event::ConnectorSynced {
                            name: format!("#{} · InReview 轮询", issue.number),
                            ok: false,
                            detail: format!("查开放 MR 失败,该活保持原状可重试:{e}"),
                        });
                        continue;
                    }
                };
                let Some(pr) = open_mr else {
                    continue; // no open MR yet — honest "nothing to review"
                };
                // D3: 评审中由「存在开放 PR」派生. Transition FIRST, then
                // backfill `pr_number` — the candidate filter above keys on
                // `pr_number == 0`, so writing the PR number before a
                // transition that then fails (store error / status moved
                // under us) would drop the issue out of the候选集 forever:
                // stuck InProgress with a PR nobody re-checks. Doing the
                // transition first means a failure leaves `pr_number == 0`
                // and the next poll retries the whole thing.
                let cur = self.store.get_issue(issue.id).await?;
                let transitioned = match cur {
                    Some(cur) if cur.status == IssueStatus::InReview => true, // already there
                    Some(cur) if cur.status.can_transition_to(IssueStatus::InReview) => {
                        self.store
                            .transition_issue(issue.id, IssueStatus::InReview)
                            .await?;
                        true
                    }
                    _ => false,
                };
                if !transitioned {
                    // Status moved somewhere the state machine won't take to
                    // InReview. Say so instead of claiming a transition that
                    // didn't happen; `pr_number` stays 0 so a later poll (or a
                    // human) can still pick it up.
                    self.emit(Event::ConnectorSynced {
                        name: format!("#{} · InReview", issue.number),
                        ok: false,
                        detail: format!(
                            "查到开放 PR/MR #{pr},但该活当前状态不允许转入评审中,保持原状"
                        ),
                    });
                    continue;
                }
                self.store.set_issue_pr_number(issue.id, pr).await?;
                // Bug1(V1-TermDemote):issue 离开 InProgress → InReview。若交付
                // PTY 仍活(claude 提完 MR 往往不退出),降级为咨询 → 放
                // active_run 锁,同项目别的 issue 可跑。PTY + worktree 留着
                // (不杀、不清)。PTY 已死则 no-op,让待处理 settle 正常 finalize。
                let _ = self.demote_delivery_to_consultation(issue.id).await;
                self.emit(Event::ConnectorSynced {
                    name: format!("#{} · InReview", issue.number),
                    ok: true,
                    detail: format!(
                        "轮询检测到开放 PR/MR #{pr},已关联 Issue #{} 并转入评审中",
                        issue.github_number
                    ),
                });
            }
        }
        self.refresh_issues().await?;
        self.emit(Event::IssuesChanged);
        Ok(())
    }

    /// Done/InReview 咨询:spawn PTY、不占 active_run、退出不 settle。
    /// 重启后点卡:is_live==false → 重建 worktree + `--resume`(不批量唤醒)。
    pub(crate) async fn open_conversation(&mut self, id: IssueId) -> Result<(), AppError> {
        let conv = self
            .store
            .get_conversation_by_issue(id)
            .await?
            .ok_or_else(|| AppError::Invalid("咨询需要已有会话行".into()))?;
        if conv.claude_session_id.is_empty() {
            return Err(AppError::Invalid(
                "会话尚无 claude_session_id,无法 resume".into(),
            ));
        }
        if self.state.terminal_manager.is_live(conv.id) {
            // 活 PTY:只切焦点。consultation_runs 有记录但 PTY 已死时不要
            // 短路——否则侧栏/续聊 focus 死 id,看起来「唤不醒」(看板完整
            // ▶跑 序列常能 remount 救回来)。settle 排队间的竞态由
            // ConsultationEnded 收尾后再点一次覆盖。
            self.focus_conversation(conv.id, id).await;
            self.clear_restoring_if(conv.id);
            self.emit(Event::IssuesChanged);
            return Ok(());
        }
        // PTY 已死但仍占着 consultation_runs:清掉残骸再 spawn,避免旧
        // ConsultationEnded settle 误清新 handle(见原注释竞态)。
        if let Some(cr) = self.state.consultation_runs.remove(&conv.id) {
            drop(cr);
        }
        if !self.state.pty_enabled {
            return Err(AppError::Invalid("咨询需要嵌入终端(PTY);当前未启用".into()));
        }
        let Some(settle_tx) = self.settle_tx.clone() else {
            return Err(AppError::Invalid(
                "咨询只在桌面内核路径可用(需要 settle 通道)".into(),
            ));
        };

        // 点卡到 PTY 首包:「恢复中…」(不在 Boot 批量起历史会话)。
        self.state.pty_restoring = Some(conv.id);

        let prep = match self.prepare_issue_run(id, true).await {
            Ok(p) => p,
            Err(e) => {
                self.state.pty_restoring = None;
                return Err(e);
            }
        };
        let p = prep.issue.project_id;
        let github_number = prep.issue.github_number;
        let workflow_id = prep.spec.id;
        let IssueRunPrep {
            issue: _,
            proj,
            spec: _,
            issue_ws,
            guard,
            distilled_block: _,
            catalog_block: _,
        } = prep;
        let workspace_cwd = issue_ws
            .as_deref()
            .unwrap_or_else(|| Path::new(proj.workspace_path.trim()));
        // V2-①: 咨询 resume 也校准规范运行副本(§7.1)。
        if !proj.workspace_path.trim().is_empty() {
            if let Err(e) = buddy_materialize::materialize_standards(workspace_cwd).await {
                self.state.pty_restoring = None;
                return Err(AppError::Invalid(format!(
                    "buddy 规范运行副本物化失败,不启动真实 Claude:{e}"
                )));
            }
        }
        // 咨询 resume:注入 §6.2 规则;失败清 pty_restoring。
        let plan = match build_consultation_resume_plan(
            &CLAUDE,
            Some(&conv.claude_session_id),
            workspace_cwd,
        ) {
            Ok(p) => p,
            Err(e) => {
                self.state.pty_restoring = None;
                return Err(AppError::Engine(e.to_string()));
            }
        };
        let branch_name = if github_number != 0 {
            format!("bw/issue-{github_number}")
        } else {
            conv.branch_name.clone()
        };
        let conversation_id = conv.id;
        let cwd_key = workspace_cwd.to_string_lossy().to_string();
        if !cwd_key.is_empty() {
            self.state.interactive_sessions.insert(cwd_key, id);
        }

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
            workflow: workflow_id,
        };
        let meta = ConversationMeta {
            conversation_id,
            issue_id: id,
            claude_session_id: conv.claude_session_id.clone(),
            workspace_path: workspace_cwd.to_path_buf(),
            branch_name: branch_name.clone(),
        };
        let (bytes_tx, input_rx) = self
            .state
            .terminal_manager
            .attach(conversation_id, meta, None);
        self.focus_conversation(conversation_id, id).await;
        let _ = self
            .backfill_conversation_workspace_if_empty(
                id,
                workspace_cwd.to_str().unwrap_or(""),
                &branch_name,
            )
            .await;

        let handle = tokio::spawn(async move {
            let _ = executor
                .run_skill_pty(&plan, &ctx, bytes_tx, input_rx)
                .await;
            let _ = settle_tx.send(SettleReq {
                project: p,
                issue: id,
                outcome: SettleOutcome::ConsultationEnded { conversation_id },
            });
        });
        self.state.consultation_runs.insert(
            conversation_id,
            ConsultationRun {
                issue_id: id,
                handle,
                guard,
            },
        );
        self.emit(Event::IssuesChanged);
        Ok(())
    }

    /// Bug1 修复(V1-TermDemote):issue 离开 InProgress(→InReview 或→Done)
    /// 且仍持有交付锁 + PTY 还活着时,把这笔交付**降级为咨询**:清掉
    /// `active_run`(放锁,同项目别的 issue 可跑)、把 PTY handle + worktree
    /// guard 从 `active_run` 迁到 `consultation_runs`(keyed by
    /// `conversation_id`),PTY 和 worktree 都留着(跟续聊形态一样)。不杀
    /// 进程、不清工作区(用户明确要求 worktree 不清)。
    ///
    /// 降级时记账:补记一次技能被用过(`record_skill_use` for
    /// `ar.finalize.spec.skills`,首次 run;resume 不记 settle-once)。合入
    /// 边(Done)已记 agent 战绩(8845),不重复记 agent;InReview 边此刻
    /// 未记 agent,agent 留到 Done 边(8845)记——所以降级只记 skill,不记
    /// agent,agent 永远只在 Done 边记一次。
    ///
    /// 降级后 PTY 退出时发的 `Interactive` settle 因 `active_run` 已空走
    /// `cleanup_demoted_consultation`(下方),不重复 finalize;skill_output
    /// 忽略(降级时已记账)。
    ///
    /// PTY 已死时不降级(restore `active_run`),让待处理的 settle 走正常
    /// finalize 路径(记 agent+skill、drop guard)。这条 idempotent:active_run
    /// 不持本 issue / PTY 已死 / 无会话行 → no-op。
    pub(crate) async fn demote_delivery_to_consultation(
        &mut self,
        issue_id: IssueId,
    ) -> Result<(), AppError> {
        let Some(ar) = self.state.active_run.take() else {
            return Ok(()); // 无在跑交付 —— 无可降级
        };
        if ar.issue.id != issue_id {
            // 别的 issue 的活 —— 还原,别误伤
            self.state.active_run = Some(ar);
            return Ok(());
        }
        // 只在 PTY 仍活时降级(claude 提完 MR 往往不退出)。PTY 已死 →
        // restore,让待处理 settle 走正常 finalize(记 agent+skill、drop guard)。
        let conv = match self.store.get_conversation_by_issue(issue_id).await? {
            Some(c) => c,
            None => {
                self.state.active_run = Some(ar);
                return Ok(());
            }
        };
        if !self.state.terminal_manager.is_live(conv.id) {
            self.state.active_run = Some(ar);
            return Ok(());
        }
        // 降级:首次 run 补记 skill uses(resume 不记,settle-once);不记
        // agent(Done 边已记 / 会记)。与 `finalize_run_interactive` 共用
        // `credit_skill_uses`(by-id scoped_pick,他项目不连带 bump)。
        if let Some(fin) = &ar.finalize {
            let (p, skills) = (fin.p, fin.spec.skills.clone());
            self.credit_skill_uses(p, &skills).await?;
            // 交付到此结束(人已点 Done,PTY 只是留作咨询):把首跑的 run 行
            // 按 Ok 结清,不让它永远挂在 Running。这里结清的是「交付」这件
            // 事,不是 PTY 进程——进程还活着,但它之后的输入输出属于咨询,
            // 不再改这一行;PTY 真退出时走 `cleanup_demoted_consultation`,
            // 不会再碰 run 行(settle-once)。
            self.settle_run_row(fin, true, "").await?;
        }
        // handle + guard 迁到 consultation_runs;active_run 已 take(放锁)。
        self.state.consultation_runs.insert(
            conv.id,
            ConsultationRun {
                issue_id,
                handle: ar.handle,
                guard: ar.guard,
            },
        );
        self.emit(Event::IssuesChanged);
        Ok(())
    }

    /// Bug1:降级后的交付 PTY 退出时,spawn 闭包仍发
    /// `SettleOutcome::Interactive`(不是 `ConsultationEnded`)。`active_run`
    /// 已空 → `run_issue_settle` 的 None 分支 / straggler 分支调本方法:
    /// 查 `req.issue` 对应的 conversation,若在 `consultation_runs` 里(降级
    /// 交付退出)→ 按咨询退出收尾(close PTY,clear_focus,clear_restoring,
    /// forget_session,drop guard,refresh)。skill_output 忽略(降级时
    /// 已记账)。不在 consultation_runs 里 → 真 no-op(早已 settle)。
    pub(crate) async fn cleanup_demoted_consultation(&mut self, issue_id: IssueId) {
        if let Ok(Some(c)) = self.store.get_conversation_by_issue(issue_id).await {
            if self.state.consultation_runs.contains_key(&c.id) {
                self.state.terminal_manager.close(c.id);
                self.clear_focus_if(c.id);
                self.clear_restoring_if(c.id);
                if let Some(cr) = self.state.consultation_runs.remove(&c.id) {
                    self.forget_interactive_session(cr.issue_id);
                    drop(cr.guard);
                }
                self.emit(Event::IssuesChanged);
            }
        }
    }
}

/// V1-TermFocus 缺口读回:阶段记录 `SelectSession` 必须把焦点切到对应
/// conversation(与 issue 看板 `RunIssue` 路径等价)。无 UI,纯命令层。
#[cfg(test)]
mod select_session_focus_tests {
    use super::*;
    use bw_core::model::{IssuePriority, MaturityPeriod};
    use bw_store::SqliteStore;
    use std::path::PathBuf;

    async fn boot_project(db: &str) -> (App, Arc<dyn Store>, ProjectId) {
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open(db).await.expect("open db"));
        let mut app = App::new(store.clone(), ClaudeCliConfig::default()).with_pty();
        app.dispatch(Command::Boot).await.expect("boot");
        let pid = ProjectId::new();
        app.dispatch(Command::CreateProject {
            provider: "github".into(),
            id: pid,
            name: "focus-switch".into(),
            kind: "test".into(),
            desc: "SelectSession focus".into(),
            workspace: None,
            github: None,
            codehub: None,
        })
        .await
        .expect("create");
        app.dispatch(Command::SetCycle {
            cycle: MaturityPeriod::Explore,
        })
        .await
        .expect("cycle");
        app.dispatch(Command::CompleteCreation {
            cadence: Cadence::Weekly,
            run_first: false,
        })
        .await
        .expect("complete");
        app.dispatch(Command::OpenProject(pid)).await.expect("open");
        (app, store, pid)
    }

    async fn mk_issue(app: &mut App, title: &str) -> IssueId {
        let id = IssueId::new();
        app.dispatch(Command::CreateIssue {
            id,
            stage: StageKind::Prototype,
            title: title.into(),
            desc: String::new(),
            priority: IssuePriority::Medium,
            standard_skill: String::new(),
        })
        .await
        .expect("create issue");
        id
    }

    fn attach_live(app: &mut App, cid: ConversationId, issue: IssueId, session_id: &str) {
        let meta = ConversationMeta {
            conversation_id: cid,
            issue_id: issue,
            claude_session_id: session_id.into(),
            workspace_path: PathBuf::from("/tmp/focus-switch"),
            branch_name: String::new(),
        };
        let _ = app.state.terminal_manager.attach(cid, meta, Some((80, 24)));
    }

    #[tokio::test]
    async fn select_session_switches_focused_live_pty() {
        let db = std::env::temp_dir().join(format!(
            "bw_select_session_focus_{}.db",
            uuid::Uuid::new_v4()
        ));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let (mut app, store, pid) = boot_project(&db_s).await;

        let issue_a = mk_issue(&mut app, "找指标").await;
        let issue_b = mk_issue(&mut app, "绑数据").await;
        // refresh_issues happens inside CreateIssue; re-read numbers.
        app.dispatch(Command::OpenProject(pid))
            .await
            .expect("reopen");
        let (num_a, title_a) = {
            let i = app.state.issues.iter().find(|i| i.id == issue_a).unwrap();
            (i.number, i.title.clone())
        };
        let (num_b, title_b) = {
            let i = app.state.issues.iter().find(|i| i.id == issue_b).unwrap();
            (i.number, i.title.clone())
        };

        let sess_a = SessionId::new();
        let sess_b = SessionId::new();
        app.dispatch(Command::StartSession {
            id: sess_a,
            stage_kind: Some(StageKind::Prototype),
            kind: SessionKind::Create,
            title: format!("#{num_a} {title_a}"),
        })
        .await
        .expect("sess a");
        app.dispatch(Command::StartSession {
            id: sess_b,
            stage_kind: Some(StageKind::Prototype),
            kind: SessionKind::Create,
            title: format!("#{num_b} {title_b}"),
        })
        .await
        .expect("sess b");

        let cid_a = store
            .ensure_conversation(issue_a, pid, "/tmp/a", "bw/a")
            .await
            .unwrap();
        let cid_b = store
            .ensure_conversation(issue_b, pid, "/tmp/b", "bw/b")
            .await
            .unwrap();
        store
            .set_conversation_session_id(issue_a, "claude-a")
            .await
            .unwrap();
        store
            .set_conversation_session_id(issue_b, "claude-b")
            .await
            .unwrap();

        attach_live(&mut app, cid_a, issue_a, "claude-a");
        attach_live(&mut app, cid_b, issue_b, "claude-b");
        app.focus_conversation(cid_a, issue_a).await;
        assert_eq!(app.state.focused_conversation, Some(cid_a));

        // 阶段记录点击路径:只发 SelectSession(不发 RunIssue)。
        app.dispatch(Command::SelectSession(Some(sess_b)))
            .await
            .expect("select b");

        assert_eq!(
            app.state.focused_conversation,
            Some(cid_b),
            "阶段记录 SelectSession 必须把嵌终端焦点切到 B(TermFocus 左→终端)"
        );
        assert_eq!(app.state.active_session, Some(sess_b));

        let _ = std::fs::remove_file(&db);
    }

    #[tokio::test]
    async fn select_session_empty_claude_session_id_runs_like_board() {
        // 旧洞:有 conversation 行但 claude_session_id 空(hook 未回填)时,
        // sync 静默 no-op → 阶段记录高亮、工作流空;看板 ▶跑 走 RunIssue
        // 能起手。收口后 SelectSession 必须同样走 run_issue_now。
        let db = std::env::temp_dir().join(format!(
            "bw_select_session_empty_sid_{}.db",
            uuid::Uuid::new_v4()
        ));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let (mut app, store, pid) = boot_project(&db_s).await;
        let issue = mk_issue(&mut app, "找指标").await;
        app.dispatch(Command::OpenProject(pid)).await.unwrap();
        let (num, title) = {
            let i = app.state.issues.iter().find(|i| i.id == issue).unwrap();
            (i.number, i.title.clone())
        };
        let sess = SessionId::new();
        app.dispatch(Command::StartSession {
            id: sess,
            stage_kind: Some(StageKind::Prototype),
            kind: SessionKind::Create,
            title: format!("#{num} {title}"),
        })
        .await
        .unwrap();
        store
            .ensure_conversation(issue, pid, "/tmp/x", "")
            .await
            .unwrap();
        // 故意不 set_conversation_session_id —— 模拟 hook 未 fire。

        app.dispatch(Command::SelectSession(Some(sess)))
            .await
            .expect("SelectSession 应像看板一样起手,不再静默");

        app.dispatch(Command::OpenProject(pid)).await.unwrap();
        let status = app
            .state
            .issues
            .iter()
            .find(|i| i.id == issue)
            .map(|i| i.status)
            .expect("issue");
        assert_eq!(
            status,
            IssueStatus::InProgress,
            "空 claude_session_id 时阶段记录点击也必须真正开工(与 RunIssue 等价)"
        );
        let _ = std::fs::remove_file(&db);
    }

    #[tokio::test]
    async fn select_session_propagates_resume_errors() {
        // 旧洞:`let _ = sync` 吞掉 open_conversation 错误 → 用户只见空白。
        // Done + 有 session_id + 无 settle 通道(headless)→ 必须 Err 上浮。
        let db =
            std::env::temp_dir().join(format!("bw_select_session_err_{}.db", uuid::Uuid::new_v4()));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let (mut app, store, pid) = boot_project(&db_s).await;
        let issue = mk_issue(&mut app, "找指标").await;
        for st in [
            IssueStatus::Todo,
            IssueStatus::InProgress,
            IssueStatus::InReview,
            IssueStatus::Done,
        ] {
            app.dispatch(Command::TransitionIssue {
                id: issue,
                status: st,
            })
            .await
            .unwrap();
        }
        app.dispatch(Command::OpenProject(pid)).await.unwrap();
        let (num, title) = {
            let i = app.state.issues.iter().find(|i| i.id == issue).unwrap();
            (i.number, i.title.clone())
        };
        let sess = SessionId::new();
        app.dispatch(Command::StartSession {
            id: sess,
            stage_kind: Some(StageKind::Prototype),
            kind: SessionKind::Create,
            title: format!("#{num} {title}"),
        })
        .await
        .unwrap();
        store
            .ensure_conversation(issue, pid, "/tmp/x", "")
            .await
            .unwrap();
        store
            .set_conversation_session_id(issue, "claude-resume")
            .await
            .unwrap();

        let err = app
            .dispatch(Command::SelectSession(Some(sess)))
            .await
            .expect_err("无 settle 通道时咨询 resume 失败必须上浮,不能 Ok(())");
        let msg = err.to_string();
        assert!(
            msg.contains("settle")
                || msg.contains("咨询")
                || msg.contains("PTY")
                || msg.contains("终端"),
            "错误应说明咨询/终端路径不可用,实际:{msg}"
        );
        let _ = std::fs::remove_file(&db);
    }

    #[tokio::test]
    async fn select_session_still_switches_after_open_issue_detail() {
        // 用户实跑(2026-08-10):重启后侧栏 SelectSession 能切;一经由看板
        // OpenIssueDetail 切过焦点,侧栏再点就「没用」。命令层回归:看板点卡
        // 后 SelectSession 仍须把 focused_conversation 切回目标卡(与侧栏
        // 首切同一条 run_issue_now);并清掉 issue_detail,避免弹层 state
        // 残留。
        let db =
            std::env::temp_dir().join(format!("bw_select_after_board_{}.db", uuid::Uuid::new_v4()));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let (mut app, store, pid) = boot_project(&db_s).await;

        let issue_a = mk_issue(&mut app, "找指标").await;
        let issue_b = mk_issue(&mut app, "绑数据").await;
        app.dispatch(Command::OpenProject(pid))
            .await
            .expect("reopen");
        let (num_a, title_a) = {
            let i = app.state.issues.iter().find(|i| i.id == issue_a).unwrap();
            (i.number, i.title.clone())
        };
        let (num_b, title_b) = {
            let i = app.state.issues.iter().find(|i| i.id == issue_b).unwrap();
            (i.number, i.title.clone())
        };

        let sess_a = SessionId::new();
        let sess_b = SessionId::new();
        app.dispatch(Command::StartSession {
            id: sess_a,
            stage_kind: Some(StageKind::Prototype),
            kind: SessionKind::Create,
            title: format!("#{num_a} {title_a}"),
        })
        .await
        .expect("sess a");
        app.dispatch(Command::StartSession {
            id: sess_b,
            stage_kind: Some(StageKind::Prototype),
            kind: SessionKind::Create,
            title: format!("#{num_b} {title_b}"),
        })
        .await
        .expect("sess b");

        let cid_a = store
            .ensure_conversation(issue_a, pid, "/tmp/a", "bw/a")
            .await
            .unwrap();
        let cid_b = store
            .ensure_conversation(issue_b, pid, "/tmp/b", "bw/b")
            .await
            .unwrap();
        store
            .set_conversation_session_id(issue_a, "claude-a")
            .await
            .unwrap();
        store
            .set_conversation_session_id(issue_b, "claude-b")
            .await
            .unwrap();

        attach_live(&mut app, cid_a, issue_a, "claude-a");
        attach_live(&mut app, cid_b, issue_b, "claude-b");

        // 侧栏先切到 A(重启后首切路径)。
        app.dispatch(Command::SelectSession(Some(sess_a)))
            .await
            .expect("sidebar A");
        assert_eq!(app.state.focused_conversation, Some(cid_a));
        assert!(app.state.issue_detail.is_none());

        // 看板点 B 标题 → OpenIssueDetail(旧半套窄门曾只 focus;现走 run_issue_now)。
        app.dispatch(Command::OpenIssueDetail(issue_b))
            .await
            .expect("board open B");
        assert_eq!(
            app.state.focused_conversation,
            Some(cid_b),
            "看板点卡必须切到 B 的终端焦点"
        );
        assert!(app.state.issue_detail.is_some(), "看板点卡仍应打开证据弹层");

        // 再侧栏点回 A —— 用户报「就没用了」的那一步。
        app.dispatch(Command::SelectSession(Some(sess_a)))
            .await
            .expect("sidebar A again");
        assert_eq!(
            app.state.focused_conversation,
            Some(cid_a),
            "看板切过之后,侧栏 SelectSession 仍须能切回 A"
        );
        assert!(
            app.state.issue_detail.is_none(),
            "侧栏切会话应收起看板证据弹层,避免回 Issue 面板再挡侧栏"
        );

        let _ = std::fs::remove_file(&db);
    }

    #[tokio::test]
    async fn run_issue_now_respawns_when_active_run_zombie() {
        // 侧栏唤不醒的命令层洞:active_run 仍挂本件但 PTY 已死时,旧逻辑
        // early-return 只 focus 死 id。应收掉 zombie 锁并允许再次走
        // interactive(本测用 mock 无 PTY 路径:清锁后不应再被「同项目忙碌」挡住)。
        let db =
            std::env::temp_dir().join(format!("bw_zombie_active_run_{}.db", uuid::Uuid::new_v4()));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let (mut app, store, pid) = boot_project(&db_s).await;
        let issue = mk_issue(&mut app, "找指标").await;
        app.dispatch(Command::OpenProject(pid)).await.unwrap();
        let cid = store
            .ensure_conversation(issue, pid, "/tmp/z", "bw/z")
            .await
            .unwrap();
        store
            .set_conversation_session_id(issue, "claude-z")
            .await
            .unwrap();

        // 伪造「锁在、PTY 不在」:写入 active_run 但不 attach。
        let issue_row = app
            .state
            .issues
            .iter()
            .find(|i| i.id == issue)
            .cloned()
            .expect("issue");
        let handle = tokio::spawn(async {});
        app.state.active_run = Some(crate::ActiveRun {
            project: pid,
            issue: issue_row,
            handle,
            guard: bw_engine::workspace::IssueWorktreeGuard::new(
                std::path::PathBuf::from("/tmp"),
                None,
            ),
            finalize: None,
            proj: app.store.get_project(pid).await.unwrap().expect("proj"),
        });
        assert!(
            !app.state.terminal_manager.is_live(cid),
            "本测前提:无活 PTY"
        );

        // run_issue_now 必须能越过 zombie(headless 无 settle 通道时 Done
        // 咨询会 Err;先把状态保持可交付的 InProgress 路径)。
        app.dispatch(Command::TransitionIssue {
            id: issue,
            status: IssueStatus::Todo,
        })
        .await
        .ok();
        app.dispatch(Command::TransitionIssue {
            id: issue,
            status: IssueStatus::InProgress,
        })
        .await
        .ok();

        let result = app.run_issue_now(SessionId::new(), issue).await;
        assert!(
            app.state.active_run.is_none()
                || result.is_ok()
                || result
                    .as_ref()
                    .err()
                    .map(|e| !e.to_string().contains("有活正在跑"))
                    .unwrap_or(false),
            "zombie 锁不得再以「有活正在跑」挡住本件唤醒: {result:?}"
        );
        let _ = std::fs::remove_file(&db);
    }
}

/// Bug A (cowelink · 找指标): InReview detection must not sit behind a
/// multi-minute silence while candidates wait, and the desktop shell must
/// rebuild Vm when the poller mutates issues without a cron fire.
#[cfg(test)]
mod inreview_poll_refresh_tests {
    use super::{
        inreview_poll_interval_secs, App, AppState, ClaudeCliConfig, INREVIEW_POLL_ACTIVE_SECS,
        INREVIEW_POLL_IDLE_SECS,
    };
    use bw_store::{SqliteStore, Store};
    use std::sync::Arc;

    #[test]
    fn active_interval_is_teens_of_seconds_not_minutes() {
        assert_eq!(inreview_poll_interval_secs(true), INREVIEW_POLL_ACTIVE_SECS);
        assert!(INREVIEW_POLL_ACTIVE_SECS <= 30);
        assert!(INREVIEW_POLL_ACTIVE_SECS >= 5);
        assert_eq!(inreview_poll_interval_secs(false), INREVIEW_POLL_IDLE_SECS);
        assert!(INREVIEW_POLL_IDLE_SECS >= 60);
    }

    #[tokio::test]
    async fn take_scheduler_ui_dirty_clears_once() {
        let db =
            std::env::temp_dir().join(format!("bw_inreview_dirty_{}.db", uuid::Uuid::new_v4()));
        let db_s = db.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&db);
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_s).await.expect("open"));
        let mut app = App::new(store, ClaudeCliConfig::default());
        assert!(!app.take_scheduler_ui_dirty());
        app.state.scheduler_ui_dirty = true;
        assert!(app.take_scheduler_ui_dirty());
        assert!(!app.take_scheduler_ui_dirty());
        let _ = AppState::default().scheduler_ui_dirty;
        let _ = std::fs::remove_file(&db);
    }
}
