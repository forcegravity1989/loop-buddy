//! 建活、▶跑、状态转移。
//!
//! 两条铁律的落点都在这个文件里:
//!
//! - **▶跑最远只到「评审中」**。执行器返回成功也只推到 InReview,不会自己
//!   推到「已完成」。
//! - **「完成」永远由人点**。`TransitionIssue { to: Done }` 是唯一入口,而且
//!   状态机只允许从「评审中」进 Done;结清那一下走的 SQL 自带
//!   `WHERE settled_at IS NULL`,同一件活绝不结两次。

use super::worktree;
use super::{App, AppError, Result};
use crate::command::Event;
use crate::model::{Category, Issue, IssueId, IssueKind, IssueOrigin, IssueStatus, ProjectId};
use crate::repo::issue_policy_file;
use bw_core::WorkflowId;
use bw_engine::{build_startup_plan, InteractiveExecutor, RunCtx, TuiAgentConfig, CLAUDE};

impl App {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn create_issue(
        &mut self,
        project_id: ProjectId,
        title: String,
        body: String,
        category: Option<Category>,
        kind: IssueKind,
        origin: IssueOrigin,
        week_of: String,
    ) -> Result<Vec<Event>> {
        // 幂等:同一个项目里标题一样的活不建第二张(指挥器重跑靠它)。
        if let Some(existing) = self.store.issue_by_title(project_id, &title).await? {
            return Ok(vec![Event::IssueCreated {
                id: existing.id,
                number: existing.number,
            }]);
        }

        // 类别决定默认工具与 workflow —— 查项目仓的 `.bw/issue-policy.toml`。
        // 文件没有、或没配这个类别,就如实留空,不替用户挑一个。
        let (tool, workflow) = match category {
            None => (String::new(), String::new()),
            Some(c) => {
                let ws = self.workspace_of(project_id).await?;
                issue_policy_file::read(&ws)
                    .ok()
                    .flatten()
                    .and_then(|f| f.mapping_for(c).cloned())
                    .map(|m| (m.tool, m.workflow))
                    .unwrap_or_default()
            }
        };

        let number = self.store.next_issue_number(project_id).await?;
        let issue = Issue {
            id: IssueId::new(),
            project_id,
            number,
            remote_number: 0,
            title,
            body,
            // 新建的活落在待办池,除非明确排了周。
            status: if week_of.is_empty() {
                IssueStatus::Backlog
            } else {
                IssueStatus::Todo
            },
            branch: String::new(),
            pr_number: 0,
            week_of,
            version: String::new(),
            tool,
            kind,
            origin,
            workflow,
            category,
            sort_order: number as f64,
            metric_key: String::new(),
            created_at: 0,
            updated_at: 0,
            settled_at: None,
        };
        let issue = self.store.insert_issue(&issue).await?;
        Ok(vec![Event::IssueCreated {
            id: issue.id,
            number: issue.number,
        }])
    }

    pub(super) async fn set_issue_workflow(
        &mut self,
        id: IssueId,
        workflow: String,
    ) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(id).await?;
        self.store
            .set_issue_dispatch(id, &issue.tool, &workflow, &issue.metric_key)
            .await?;
        Ok(vec![Event::IssueReordered { id }])
    }

    /// 唯一的干活入口。
    ///
    /// 干成了推「评审中」;干砸了如实停在原地(状态不动),可以重试,不假装
    /// 前进。**任何情况下都不会走到「已完成」**。
    pub(super) async fn run_issue(&mut self, id: IssueId) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(id).await?;
        if !matches!(
            issue.status,
            IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
        ) {
            return Err(AppError::Refused(format!(
                "这张活现在是「{}」,不是能开工的状态",
                issue.status.label()
            )));
        }

        // 桌面壳里开工 = 起一个内嵌终端,人看着 agent 干活。这条路不等执行器
        // 返回,所以下面阻塞那一段不再走。
        if self.run_issue_pty(id).await? {
            return Ok(vec![Event::IssueRan {
                id,
                ok: true,
                summary: "已在内嵌终端里开工,过程看会话屏".into(),
            }]);
        }

        let ws = self.workspace_of(issue.project_id).await?;
        // 有真实工作区就在**这张活自己的 worktree** 里干,和 PTY 那条路一样;
        // 没有工作区就没什么可隔离的,下面走自我标注的替身。
        let tree = if ws.is_dir() {
            let t = worktree::provision(&ws, issue.number).await?;
            // 变了才写,同 `run_issue_pty`:分支没变就别白顶一次 `updated_at`。
            if issue.branch != t.branch {
                self.store
                    .set_issue_remote(id, &t.branch, issue.pr_number, issue.remote_number)
                    .await?;
            }
            Some(t)
        } else {
            None
        };
        let run_dir = tree
            .as_ref()
            .map(|t| t.path.clone())
            .unwrap_or_else(|| ws.clone());
        // 先推进行中 —— 界面上立刻看得出它开工了。
        if issue.status != IssueStatus::InProgress {
            self.store
                .set_issue_status(id, IssueStatus::InProgress)
                .await?;
        }

        let agent: &TuiAgentConfig = &CLAUDE;
        let prompt = format!("#{} {}\n\n{}", issue.number, issue.title, issue.body);
        // 剧本先在干活那棵树里找,找不到再回主工作区找(有些仓把 `.claude/`
        // 写进了 .gitignore,技能包进不了分支,只在主检出里)。
        let workflow_body = super::bootstrap::workflow_body(&run_dir, &issue.workflow)
            .or_else(|| super::bootstrap::workflow_body(&ws, &issue.workflow));
        let system_prompt = super::bootstrap::agent_system_prompt(&issue, workflow_body.as_deref());
        let plan = build_startup_plan(agent, &prompt, &system_prompt, &run_dir)
            .map_err(|e| AppError::Exec(e.to_string()))?;
        let ctx = RunCtx {
            project: issue.project_id,
            workflow: WorkflowId::nil(),
        };

        // 没有真实工作区的项目走自我标注的替身:产出带【mock】字样,不冒充
        // 真的干过活。有工作区但没开 PTY(指挥器、headless)时也走这条。
        let out = if tree.is_some() {
            self.executor.run_skill(&plan, &ctx).await
        } else {
            bw_engine::MockInteractiveExecutor::new()
                .run_skill(&plan, &ctx)
                .await
        };
        match out {
            Ok(o) if o.completed => {
                // 最远到「评审中」。这一步不是人点的,所以到此为止。
                self.store
                    .set_issue_status(id, IssueStatus::InReview)
                    .await?;
                let mut events = vec![Event::IssueRan {
                    id,
                    ok: true,
                    summary: o.summary,
                }];
                if let Some(e) = self.chat_notify_issue(id, "review").await {
                    events.push(e);
                }
                Ok(events)
            }
            Ok(o) => Ok(vec![Event::IssueRan {
                id,
                ok: false,
                summary: o.summary,
            }]),
            Err(e) => Ok(vec![Event::IssueRan {
                id,
                ok: false,
                summary: format!("开工失败:{e}"),
            }]),
        }
    }

    /// 状态转移。合法性由 `bw_core` 的状态机守——不合法就如实报错弹回,不悄悄
    /// 换成一个合法的转移。
    pub(super) async fn transition_issue(
        &mut self,
        id: IssueId,
        to: IssueStatus,
    ) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(id).await?;
        if issue.status == to {
            return Ok(vec![Event::IssueTransitioned {
                id,
                to,
                settled: false,
            }]);
        }
        if !issue.status.can_transition_to(to) {
            return Err(AppError::IllegalTransition {
                from: issue.status.label().to_string(),
                to: to.label().to_string(),
            });
        }
        self.store.set_issue_status(id, to).await?;
        // 结清只发生在真正走进「已完成」的那一下,而且只结一次。
        let settled = if to == IssueStatus::Done {
            self.store.settle_issue(id).await?
        } else {
            false
        };
        // 结清了就把这张活那棵 worktree 收掉 —— 活完了,那棵树没用了。
        // **只在它干净的时候收**:里面还有没提交的改动就原样留着,「我点了
        // 完成」不构成删掉别人劳动成果的授权。
        if settled {
            self.cleanup_issue_worktree(&issue).await;
        }
        let mut events = vec![Event::IssueTransitioned { id, to, settled }];
        // 进评审 = 该人看一眼了,配了群就往群里说一声。
        if to == IssueStatus::InReview {
            if let Some(e) = self.chat_notify_issue(id, "review").await {
                events.push(e);
            }
        }
        Ok(events)
    }

    pub(super) async fn block_issue(&mut self, id: IssueId, reason: String) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(id).await?;
        if !issue.status.can_transition_to(IssueStatus::Blocked) {
            return Err(AppError::IllegalTransition {
                from: issue.status.label().to_string(),
                to: IssueStatus::Blocked.label().to_string(),
            });
        }
        self.store
            .set_issue_status(id, IssueStatus::Blocked)
            .await?;
        // 卡在哪写进这张活的说明里 —— 不写下来,下次接手的人只看见一个红点。
        let body = if issue.body.is_empty() {
            format!("阻塞原因:{reason}")
        } else {
            format!("{}\n\n阻塞原因:{reason}", issue.body)
        };
        self.store.set_issue_body(id, &body).await?;
        Ok(vec![Event::IssueBlocked { id }])
    }

    /// 收掉这张活的 worktree。收不掉(脏的、路径算不出来、项目没了)就什么
    /// 都不做 —— 收尾失败不该把「这张活已经完成」这件事顶回去。
    async fn cleanup_issue_worktree(&self, issue: &Issue) {
        let Ok(ws) = self.workspace_of(issue.project_id).await else {
            return;
        };
        let Some(tree) = bw_engine::workspace::issue_worktree_path(&ws, issue.number) else {
            return;
        };
        worktree::remove_if_clean(&ws, &tree).await;
    }

    pub(crate) async fn issue_or_err(&self, id: IssueId) -> Result<Issue> {
        self.store.issue(id).await?.ok_or(AppError::NoSuchIssue)
    }
}
