//! 会话:内嵌终端的 PTY 生命周期。
//!
//! 分工看清楚了就不会乱:
//!
//! - **`claude_conversation` 表**存的是会话的**身份**——buddy 自己的
//!   `ConversationId`、claude CLI 回传的 `--resume` id、固定的 worktree 路径、
//!   分支名。进程死了它还在,重启后拿它把会话接回来。
//! - **`TerminalManager`(纯内存)**存的是**进程**——子进程句柄、字节流、当前
//!   终端尺寸。进程死了就没了,重启后不存在,也不该存在。
//!
//! PTY 只在桌面壳里开([`App::with_pty`]);指挥器和别的 headless 场景一律走
//! 阻塞的 `run_skill`——没有界面去渲染字节流,起个 PTY 只会留下一个没人读的
//! 子进程。

use super::{App, AppError, Result};
use crate::command::Event;
use crate::model::{ConversationId, IssueId, IssueStatus};
use bw_core::WorkflowId;
use bw_engine::interactive_cli::PtyInput;
use bw_engine::{build_resume_plan, build_startup_plan, ConversationMeta, RunCtx, CLAUDE};

impl App {
    /// 桌面壳开 PTY。开了之后 ▶跑 不再阻塞等执行器返回,而是把子进程挂进
    /// [`bw_engine::TerminalManager`],字节流交给会话屏渲染。
    pub fn with_pty(mut self) -> Self {
        self.pty_enabled = true;
        self
    }

    /// 内嵌终端这一跳:把各会话缓冲里的字节取出来,交给界面按会话 id 路由。
    pub fn drain_pty_events(&mut self) -> Vec<(ConversationId, Vec<u8>)> {
        self.terminal.drain_events()
    }

    /// 这个会话的 PTY 还活着没有。左列「运行中 / 空闲」两态里的那一半靠它。
    pub fn pty_live(&self, id: ConversationId) -> bool {
        self.terminal.is_live(id)
    }

    pub(super) async fn terminal_input(
        &mut self,
        conversation_id: ConversationId,
        bytes: Vec<u8>,
    ) -> Result<Vec<Event>> {
        self.terminal.input(conversation_id, PtyInput::Bytes(bytes));
        Ok(vec![])
    }

    pub(super) async fn terminal_resize(
        &mut self,
        conversation_id: ConversationId,
        cols: u16,
        rows: u16,
    ) -> Result<Vec<Event>> {
        self.terminal.note_fit_size(cols, rows);
        self.terminal.resize(conversation_id, cols, rows);
        Ok(vec![])
    }

    /// ■停止:关掉这件活的 PTY。
    ///
    /// **状态原地不动**。停下来不是失败也不是完成,这张活还在「进行中」,人
    /// 随时能再点▶跑接回去(有 `--resume` id 就精确接回那次对话)。
    pub(super) async fn cancel_run(&mut self, id: IssueId) -> Result<Vec<Event>> {
        let Some(conv) = self.store.conversation_for_issue(id).await? else {
            return Ok(vec![Event::RunCancelled {
                id,
                was_live: false,
            }]);
        };
        let was_live = self.terminal.is_live(conv.id);
        self.terminal.close(conv.id);
        Ok(vec![Event::RunCancelled { id, was_live }])
    }

    /// ▶跑 的 PTY 分支。返回 `Ok(true)` 表示已经挂起来了,调用方到此为止。
    ///
    /// 和阻塞那条路最大的区别:**这里不等执行器返回**。子进程在后台跑,状态
    /// 停在「进行中」,推「评审中」是会话收尾那一下的事——所以这条路径下
    /// ▶跑 更不可能把活推到「完成」。
    pub(super) async fn run_issue_pty(&mut self, id: IssueId) -> Result<bool> {
        if !self.pty_enabled {
            return Ok(false);
        }
        let issue = self.issue_or_err(id).await?;
        let ws = self.workspace_of(issue.project_id).await?;
        // 工作区目录压根不在,就别起终端 —— 在一个不存在的目录里 spawn
        // claude,人看到的是一屏报错。退回阻塞那条路,那边会用自我标注的替身
        // 跑一遍,产出带【mock】字样,谁都不会误以为它真干了活。
        if !ws.is_dir() {
            return Ok(false);
        }

        // 这张活的终端还开着就别重开。`attach` 第一件事是 close 掉同 id 的旧会
        // 话(整个进程组被 kill),而 `--resume` 那条路今天走不通(见下),所以
        // 重开一定是**从头一次新对话** —— 人跟 agent 谈到一半的上下文就这么没
        // 了,还没有任何提示。顶栏「▶开工」和「■停止」是挨着放的,误点很容易。
        if let Some(existing) = self.store.conversation_for_issue(id).await? {
            if self.terminal.is_live(existing.id) {
                return Err(AppError::Refused(
                    "这张活的终端还开着。要重开先点「■ 停止」——重开是从头一次新对话,                     不是接着刚才那段聊。"
                        .into(),
                ));
            }
        }

        // 分支名记的是**这个工作区当前真的在哪个分支上**,不是拼一个
        // `bw/issue-N` 出来。V4 还没有给每张活开 worktree 与分支(见
        // `docs/LEFTOVERS.md`),拼一个不存在的名字摆在界面上,人拿
        // `git rev-parse --abbrev-ref HEAD` 一对就对不上。
        let branch = crate::git::current_branch(&ws)
            .await
            .unwrap_or_else(|_| "—(读不出当前分支)".into());

        let conv = self
            .store
            .upsert_conversation(&crate::model::Conversation {
                id: ConversationId::new(),
                project_id: issue.project_id,
                issue_id: id,
                claude_session_id: String::new(),
                workspace_path: ws.display().to_string(),
                branch_name: branch.clone(),
                created_at: 0,
                last_opened_at: 0,
            })
            .await?;

        let prompt = format!("#{} {}\n\n{}", issue.number, issue.title, issue.body);
        let workflow_body = super::bootstrap::workflow_body(&ws, &issue.workflow);
        let system_prompt = super::bootstrap::agent_system_prompt(&issue, workflow_body.as_deref());
        // 有 `--resume` id 就精确接回那一次对话,不是模糊地接「最近一次」。
        let plan = if conv.claude_session_id.is_empty() {
            build_startup_plan(&CLAUDE, &prompt, &system_prompt, &ws)
        } else {
            build_resume_plan(&CLAUDE, Some(&conv.claude_session_id), &ws)
        }
        .map_err(|e| AppError::Exec(e.to_string()))?;

        let meta = ConversationMeta {
            conversation_id: conv.id,
            issue_id: id,
            claude_session_id: conv.claude_session_id.clone(),
            workspace_path: ws.clone(),
            branch_name: branch,
        };
        let (bytes_tx, input_rx) = self.terminal.attach(conv.id, meta, None);

        let executor = self.executor.clone();
        let ctx = RunCtx {
            project: issue.project_id,
            workflow: WorkflowId::nil(),
        };
        tokio::spawn(async move {
            // 跑完的结果不在这里落库:PTY 会话的收尾(提交、开 PR、推「评审
            // 中」)是人在终端里看着发生的,由下一跳的轮询认领。这里只保证
            // 子进程不会因为没人 await 就被丢掉。
            let _ = executor
                .run_skill_pty(&plan, &ctx, bytes_tx, input_rx)
                .await;
        });

        if issue.status != IssueStatus::InProgress {
            self.store
                .set_issue_status(id, IssueStatus::InProgress)
                .await?;
        }
        Ok(true)
    }
}
