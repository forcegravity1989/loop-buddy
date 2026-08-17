//! 调度器:到点触发 Cron、Autopilot 建活(绝不自动完成)、评审中轮询候选。
//! 从 lib.rs 机械拆出(2026-08-17),逻辑未改。

use super::*;

impl App {
    /// The real scheduler tick — call this on an interval (see
    /// `app-desktop/src/kernel.rs`) to really auto-fire due cron tasks, no
    /// click required. Reads cron tasks + hub specs fresh from the store
    /// (never trusts a possibly-stale in-memory snapshot for a decision this
    /// consequential), fires each task whose `bw_core::model::cron_due` says
    /// yes, and returns which ones fired — `[]` on a quiet tick, which is
    /// the common case and not an error.
    ///
    /// Deliberately does **not** touch `self.state.active_project`/`view`/
    /// `panel`/`scope`/`active_session`: unlike the desktop UI's manual "▶
    /// 立即执行" (which *does* navigate the caller to go watch, because a
    /// human just asked for that), an unattended background fire must not
    /// yank whatever project/screen the user currently has open. Real
    /// "monitoring" here means `Event::CronAutoFired` + the cron row's own
    /// persisted status/`last_run`, not a hijacked view.
    ///
    /// One task failing (a real `run_workflow_inner` error) is recorded as
    /// `CronStatus::Failed` and does not stop the rest of this tick from
    /// evaluating the remaining tasks.
    pub async fn tick_scheduler(&mut self) -> Result<Vec<CronTaskId>, AppError> {
        let now_ts = now();
        let tasks = self.store.list_cron_tasks().await?;
        let specs = self.store.list_workflow_specs().await?;
        let mut fired = Vec::new();

        for c in tasks {
            if c.status != CronStatus::Normal {
                continue; // Paused/Running/Failed never auto-fire — pause is real human intervention, honored here.
            }
            let Some(pid) = c.project_id else {
                continue; // "全部项目" tasks can't resolve a single project to run in — same rule the manual trigger's `can_run` check uses.
            };
            if !cron_due(&c.schedule, c.last_run_at, now_ts) {
                continue;
            }

            // A1: autopilot — a create_issue task mints a stage-scoped Issue
            // (Todo, optionally assigned) instead of running a workflow. No-hijack
            // by construction: this branch never calls run_workflow_inner.
            if c.mode == CronMode::CreateIssue {
                let Some(stage) = c.issue_stage else {
                    continue; // misconfigured — no stage to scope the Issue to
                };
                self.store
                    .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                    .await?;
                let res = self
                    .autopilot_fire(pid, &c.name, stage, c.issue_assignee.as_deref(), now_ts)
                    .await;
                let (ok, status) = match &res {
                    Ok(_) => (true, CronStatus::Normal),
                    Err(_) => (false, CronStatus::Failed),
                };
                self.store
                    .record_cron_run(c.id, status, run_at_label(now()))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.refresh_issues().await?;
                self.emit(Event::CronTasksChanged);
                self.emit(Event::IssuesChanged);
                self.emit(Event::CronAutoFired {
                    id: c.id,
                    name: c.name.clone(),
                    ok,
                });
                fired.push(c.id);
                continue;
            }

            // C7: the standard collector — pull real data into the project's
            // metrics as append-only observations. No-hijack by construction:
            // this branch never calls run_workflow_inner, never settles
            // anything — collecting is observation, not work, so an unattended
            // auto-fire can't breach 「Done 永不自动」.
            if c.mode == CronMode::CollectMetrics {
                self.store
                    .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                    .await?;
                let res = self.collect_project_metrics(pid).await;
                let (ok, status) = match &res {
                    Ok(s) if s.is_success() => (true, CronStatus::Normal),
                    Ok(_) | Err(_) => (false, CronStatus::Failed),
                };
                self.store
                    .record_cron_run(c.id, status, run_at_label(now()))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
                self.emit(Event::CronAutoFired {
                    id: c.id,
                    name: c.name.clone(),
                    ok,
                });
                fired.push(c.id);
                continue;
            }

            // T10 (plan/12 §5): RunSkill / RunPrompt — both really execute,
            // through the identical run_workflow_inner engine/executor path
            // the RunWorkflow branch below uses, against a single ad-hoc
            // prompt (`cron_prompt_workflow`) instead of a real hub
            // workflow's phases. Resolved fresh on every fire, never cached:
            // a RunSkill task whose skill was deleted since creation fails
            // honestly right here — no crash, no silent no-op, no fabricated
            // success.
            let adhoc_spec = match &c.mode {
                CronMode::RunSkill { skill_id } => {
                    Some(match self.store.get_skill(*skill_id).await? {
                        Some(skill) => Ok(cron_prompt_workflow(
                            format!("⚙ 定时技能 · {}", skill.name),
                            skill.content.clone(),
                        )),
                        None => Err("引用的技能已删除".to_string()),
                    })
                }
                CronMode::RunPrompt { prompt } => Some(Ok(cron_prompt_workflow(
                    format!("💬 定时 Prompt · {}", c.name),
                    prompt.clone(),
                ))),
                _ => None,
            };

            if let Some(adhoc_spec) = adhoc_spec {
                self.store
                    .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);

                let ok = match adhoc_spec {
                    Err(reason) => {
                        // Nothing to execute (referenced skill gone) — an
                        // honest failed fire, recorded as a real workflow_run
                        // row so CronEffectiveness reflects it exactly like an
                        // engine failure would (settle-once, never a
                        // fabricated success or a silently skipped fire).
                        let started_at = OffsetDateTime::now_utc().unix_timestamp();
                        let run_id = self
                            .store
                            .record_workflow_run_start(bw_store::NewWorkflowRun {
                                workflow_id: WorkflowId::new(),
                                workflow_name: &c.name,
                                project_id: Some(pid),
                                session_id: None,
                                trigger: RunTrigger::Scheduled,
                                started_at,
                                cron_task_id: Some(c.id),
                                params_json: "",
                            })
                            .await?;
                        self.store
                            .settle_workflow_run(
                                run_id,
                                RunStatus::Failed,
                                OffsetDateTime::now_utc().unix_timestamp(),
                                0,
                                0,
                                &reason,
                            )
                            .await?;
                        false
                    }
                    Ok(spec) => {
                        let session = SessionId::new();
                        self.store
                            .ensure_session(NewSession {
                                id: session,
                                project_id: pid,
                                stage_kind: None,
                                kind: SessionKind::Optimize,
                                title: format!("⏰ 定时触发 · {}", c.name),
                                snippet: String::new(),
                            })
                            .await?;
                        let result = self
                            .run_workflow_inner(
                                pid,
                                session,
                                spec,
                                RunTrigger::Scheduled,
                                Some(c.id),
                                None,
                                None,
                            )
                            .await;
                        matches!(result, Ok(RunOutcome::Completed))
                    }
                };

                let outcome = if ok {
                    CronStatus::Normal
                } else {
                    CronStatus::Failed
                };
                self.store
                    .record_cron_run(c.id, outcome, run_at_label(now()))
                    .await?;
                self.refresh_cron_tasks().await?;
                self.emit(Event::CronTasksChanged);
                self.emit(Event::CronAutoFired {
                    id: c.id,
                    name: c.name.clone(),
                    ok,
                });
                fired.push(c.id);
                continue;
            }

            // plan/20 R2: cron 按名找 workflow 走同一条就近规则——本项目行
            // 优先、全局兜底、他项目行永不命中(plan/08 S1「cron 的按名找
            // workflow 同样本项目优先」)。
            let Some(spec) = bw_core::scope::scoped_pick(
                specs.iter(),
                Some(pid),
                |w| w.project_id,
                |w| w.name == c.target,
            )
            .cloned() else {
                continue; // target doesn't (yet) name a real hub workflow — same rule as the manual trigger.
            };

            self.store
                .record_cron_run(c.id, CronStatus::Running, run_at_label(now_ts))
                .await?;
            self.refresh_cron_tasks().await?;
            self.emit(Event::CronTasksChanged);

            let session = SessionId::new();
            self.store
                .ensure_session(NewSession {
                    id: session,
                    project_id: pid,
                    stage_kind: spec
                        .stage_ref
                        .and_then(|n| StageKind::ALL.into_iter().find(|s| s.index() == n)),
                    kind: SessionKind::Optimize,
                    title: format!("⏰ 定时触发 · {}", c.name),
                    snippet: String::new(),
                })
                .await?;
            self.store.record_workflow_use(spec.id).await?;
            self.refresh_workflow_specs().await?;

            let result = self
                .run_workflow_inner(
                    pid,
                    session,
                    spec,
                    RunTrigger::Scheduled,
                    Some(c.id),
                    None,
                    None,
                )
                .await;
            // A scheduled run "succeeds" only when the workflow actually passed.
            // A hit review cap (`BlockedAtCap`) has no bound Issue to park here
            // (cron RunWorkflow passes `issue_id = None`) — its honest per-round
            // rows already record "上限未通过", so we surface it as Failed rather
            // than a fake green (no fabricated Issue — plan/12 §4).
            let ok = matches!(result, Ok(RunOutcome::Completed));
            let outcome = if ok {
                CronStatus::Normal
            } else {
                CronStatus::Failed
            };
            self.store
                .record_cron_run(c.id, outcome, run_at_label(now()))
                .await?;
            self.refresh_cron_tasks().await?;
            self.emit(Event::CronTasksChanged);
            self.emit(Event::CronAutoFired {
                id: c.id,
                name: c.name.clone(),
                ok,
            });

            fired.push(c.id);
        }
        // V1 Issue2 Phase2b: drain hook events from the listener (localhost
        // HTTP server). SessionStart → store session_id (F1 fix); Stop → set
        // pending_stop_check for immediate InReview detection. Best-effort —
        // a failure is silently skipped (the 5-minute poller below is the
        // backstop). No listener (hook_event_rx = None) = no-op.
        let _ = self.poll_hook_events().await;

        // V1 Issue2 Phase2b: Stop-triggered InReview check. When a `Stop`
        // hook event was received (agent finished a turn), run
        // `poll_interactive_inreview` immediately — the Stop is the real-time
        // trigger (replaces 2a's idle-only cadence). The tick's cadence
        // is the natural throttle (multiple Stops in one tick = one check).
        if self.state.pending_stop_check {
            self.state.pending_stop_check = false;
            self.state.scheduler_ui_dirty = true;
            if let Err(e) = self.poll_interactive_inreview().await {
                self.emit(Event::ConnectorSynced {
                    name: "InReview (Stop 触发)".into(),
                    ok: false,
                    detail: format!("Stop 触发 InReview 检测失败,短周期轮询兜底:{e}"),
                });
            }
        }

        // V1 Issue2 Phase2a: InReview detection poller (adaptive backstop).
        // Checks codehub/github for open MRs on interactive issues (InProgress
        // + interactive (has conversation) + pr_number == 0). Not a cron task — a
        // separate periodic check that rides `tick_scheduler`'s cadence.
        // While candidates wait for an MR, poll every ~15s so detection is
        // seconds–teens of seconds (not multi-minute silence). Idle projects
        // keep the 5 min interval to avoid flooding the remote API.
        // Never auto-Done (铁律) — only backfills pr_number +
        // transitions to InReview. In Phase2b this is the BACKSTOP for the
        // Stop-triggered check above — catches MRs the Stop trigger missed
        // (hook not installed, agent session not via buddy, MR not yet
        // visible on the first Stop poll, etc.).
        let now_ts = now().unix_timestamp();
        let interval =
            inreview_poll_interval_secs(self.has_inreview_poll_candidates().await.unwrap_or(false));
        if now_ts - self.state.last_inreview_poll >= interval {
            self.state.last_inreview_poll = now_ts;
            self.state.scheduler_ui_dirty = true;
            // Best-effort: a poller failure is recorded as a toast but never
            // blocks the scheduler's cron-fired list from returning.
            if let Err(e) = self.poll_interactive_inreview().await {
                self.emit(Event::ConnectorSynced {
                    name: "InReview 轮询".into(),
                    ok: false,
                    detail: format!("本轮 InReview 检测失败,下轮重试:{e}"),
                });
            }
        }
        Ok(fired)
    }

    /// True when any project has at least one interactive InProgress issue
    /// still waiting for an open MR (`pr_number == 0`). Drives the active
    /// (short) InReview poll interval — idle projects stay on the long
    /// backstop so we don't hammer the remote API every tick.
    pub(crate) async fn has_inreview_poll_candidates(&self) -> Result<bool, AppError> {
        for proj in &self.state.projects {
            if proj.remote_path.trim().is_empty() {
                continue;
            }
            let conv_ids: std::collections::HashSet<IssueId> = self
                .store
                .list_conversation_issue_ids(proj.id)
                .await?
                .into_iter()
                .collect();
            if conv_ids.is_empty() {
                continue;
            }
            let issues = self
                .store
                .list_issues(proj.id, None, Some(IssueStatus::InProgress))
                .await?;
            if issues
                .iter()
                .any(|i| conv_ids.contains(&i.id) && i.pr_number == 0 && i.github_number != 0)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// A1: the create_issue cron path — mint a stage-scoped Issue (Todo,
    /// optionally assigned by name). No-hijack: never runs a workflow. A missing
    /// named agent is an honest unassigned Issue, not a failure.
    pub(crate) async fn autopilot_fire(
        &mut self,
        project: ProjectId,
        name: &str,
        stage: StageKind,
        assignee: Option<&str>,
        fired_at: OffsetDateTime,
    ) -> Result<IssueId, AppError> {
        let issue_id = IssueId::new();
        let title = format!("[auto] {name}");
        let desc = format!(
            "Autopilot 建单(定时任务「{name}」于 {} 触发,{} 阶段)。",
            run_at_label(fired_at),
            stage.label()
        );
        self.store
            .create_issue(NewIssue {
                id: issue_id,
                project_id: project,
                stage,
                title: title.clone(),
                desc: desc.clone(),
                priority: IssuePriority::Medium,
                standard_skill: String::new(),
            })
            .await?;
        // C4: Autopilot/cron 建单同样过身份映射 —— 建单入口不止手动创建一
        // 处,漏一条就是"手动建的有号、定时建的没号"的诚实性缺口。announce=
        // false(plan/14 C14 范围收敛):Autopilot 建单不在本票覆盖的创建流
        // 动作里,行为一个字节不变。
        self.sync_issue_to_github(project, issue_id, &title, &desc, false)
            .await?;
        // Todo (committed work), not Backlog (the parking lot) — autopilot建单
        // is a commitment, and Backlog is the suppress-firing pile in multica.
        self.store
            .transition_issue(issue_id, IssueStatus::Todo)
            .await?;
        // Assign by name if the named agent exists — honest 0-match otherwise.
        // plan/20 R2: 就近优先——本项目的五角色副本(W1)优先于全局同名行,
        // 他项目的行永不命中,自动建单的战绩才落在本项目的账上。
        if let Some(agent_name) = assignee {
            let agents = self.store.list_agents().await?;
            if let Some(agent) = bw_core::scope::scoped_pick(
                agents.iter(),
                Some(project),
                |a| a.project_id,
                |a| a.name == agent_name,
            ) {
                self.store.assign_issue(issue_id, Some(agent.id)).await?;
            }
        }
        Ok(issue_id)
    }
}
