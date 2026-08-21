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

use super::worktree;
use super::{App, AppError, Result};
use crate::command::Event;
use crate::model::{ConversationId, IssueId, IssueStatus};
use v4_engine::interactive_cli::PtyInput;
use v4_engine::{build_resume_plan, build_startup_plan, ConversationMeta, RunCtx, CLAUDE};

impl App {
    /// 桌面壳开 PTY。开了之后 ▶跑 不再阻塞等执行器返回,而是把子进程挂进
    /// [`v4_engine::TerminalManager`],字节流交给会话屏渲染。
    pub fn with_pty(mut self) -> Self {
        self.pty_enabled = true;
        self
    }

    /// 内嵌终端这一跳:把各会话缓冲里的字节取出来,交给界面按会话 id 路由。
    pub fn drain_pty_events(&mut self) -> Vec<(ConversationId, Vec<u8>)> {
        // 底座按裸 UUID 路由,出了这一层就包回语义类型 —— 转换只发生在这儿,
        // 上面的用例代码永远只见 `ConversationId`。
        self.terminal
            .drain_events()
            .into_iter()
            .map(|(id, bytes)| (ConversationId::from_uuid(id), bytes))
            .collect()
    }

    /// 这个会话的 PTY 还活着没有。左列「运行中 / 空闲」两态里的那一半靠它。
    pub fn pty_live(&self, id: ConversationId) -> bool {
        self.terminal.is_live(id.uuid())
    }

    pub(super) async fn terminal_input(
        &mut self,
        conversation_id: ConversationId,
        bytes: Vec<u8>,
    ) -> Result<Vec<Event>> {
        self.terminal
            .input(conversation_id.uuid(), PtyInput::Bytes(bytes));
        Ok(vec![])
    }

    pub(super) async fn terminal_resize(
        &mut self,
        conversation_id: ConversationId,
        cols: u16,
        rows: u16,
    ) -> Result<Vec<Event>> {
        self.terminal.note_fit_size(cols, rows);
        self.terminal.resize(conversation_id.uuid(), cols, rows);
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
        let was_live = self.terminal.is_live(conv.id.uuid());
        self.terminal.close(conv.id.uuid());
        Ok(vec![Event::RunCancelled { id, was_live }])
    }

    /// 把上次那场会话接回来看看。**不改活的状态,不发任何 prompt。**
    ///
    /// 为什么要单独一条命令、不复用 ▶开工:▶开工 会把活推到「进行中」。而人从
    /// 通知点进来看一张**已经在评审中**的活时,要的是「让我看看 agent 都干了
    /// 什么」,不是把这张活拽回上一格。`claude --resume <id>` 本身也确实什么都
    /// 不做 —— 它不带 prompt、不带 `--append-system-prompt`,只是重新进入那场
    /// 对话等人说话,既不花钱也不会动手。
    ///
    /// 这条是**从 V3 掉下来的行为**:V3 里「选中一张活」就等于接回它的会话
    /// (`bw-app/src/terminal.rs` 有整套测试在守),V4 的 `SelectSession` 退化
    /// 成了纯切视图,于是点进去是一片空白。
    pub(super) async fn reopen_session(&mut self, id: IssueId) -> Result<Vec<Event>> {
        if !self.pty_enabled {
            return Err(AppError::Refused("这个进程没开内嵌终端,接不回会话".into()));
        }
        let Some(conv) = self.store.conversation_for_issue(id).await? else {
            return Err(AppError::Refused(
                "这张活还没有过会话 —— 点「▶开工」起一场".into(),
            ));
        };
        // 已经开着就什么都不做。再 attach 一次会先把旧的整个进程组 kill 掉,
        // 人跟 agent 谈到一半的上下文就没了。
        if self.terminal.is_live(conv.id.uuid()) {
            return Ok(vec![Event::SessionReopened {
                issue_id: id,
                live: true,
            }]);
        }
        let issue = self.issue_or_err(id).await?;
        let ws = self.workspace_of(issue.project_id).await?;
        if !ws.is_dir() {
            return Err(AppError::NoWorkspace(issue.project_id.uuid().to_string()));
        }
        let tree = worktree::provision(&ws, issue.number).await?;
        let plan = build_resume_plan(
            &CLAUDE,
            Some(conv.claude_session_id.as_str()).filter(|s| !s.is_empty()),
            &tree.path,
        )
        .map_err(|e| AppError::Exec(e.to_string()))?;

        let meta = ConversationMeta {
            conversation_id: conv.id.uuid(),
            issue_id: id.uuid(),
            claude_session_id: conv.claude_session_id.clone(),
            workspace_path: tree.path.clone(),
            branch_name: tree.branch,
        };
        let (bytes_tx, input_rx) = self.terminal.attach(conv.id.uuid(), meta, None);
        let executor = self.executor.clone();
        let ctx = RunCtx {
            project: issue.project_id.uuid(),
            workflow: uuid::Uuid::nil(),
        };
        tokio::spawn(async move {
            let _ = executor
                .run_skill_pty(&plan, &ctx, bytes_tx, input_rx)
                .await;
        });
        Ok(vec![Event::SessionReopened {
            issue_id: id,
            live: false,
        }])
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

        // 这张活的终端还开着就别重开。`attach` 第一件事是 close 掉同 id 的旧
        // 会话(整个进程组被 kill)—— 人跟 agent 谈到一半的上下文就这么没了,
        // 还没有任何提示。顶栏「▶开工」和「■停止」是挨着放的,误点很容易。
        // (光接回上次那场对话不必走这条路,那是 `reopen_session`。)
        if let Some(existing) = self.store.conversation_for_issue(id).await? {
            if self.terminal.is_live(existing.id.uuid()) {
                return Err(AppError::Refused(
                    "这张活的终端还开着。要重开先点「■ 停止」——重开是从头一次新对话,不是接着刚才那段聊。"
                        .into(),
                ));
            }
        }

        // agent 不在人的主检出里干活,在**这张活自己的一棵 worktree** 里干,
        // 分支是 `bw/issue-<号>`。两张活同时开工也就互不干扰了。分支名不是拼
        // 出来的:worktree 真的在这个分支上,`git rev-parse --abbrev-ref HEAD`
        // 一对就对得上。
        let tree = worktree::provision(&ws, issue.number).await?;

        let conv = self
            .store
            .upsert_conversation(&crate::model::Conversation {
                id: ConversationId::new(),
                project_id: issue.project_id,
                issue_id: id,
                claude_session_id: String::new(),
                workspace_path: tree.path.display().to_string(),
                branch_name: tree.branch.clone(),
                created_at: 0,
                last_opened_at: 0,
            })
            .await?;
        // 界面上这张活的分支从这里来。**变了才写** —— 反复停止/重开时分支根本
        // 没变,白写一条 UPDATE 还会把 `updated_at` 顶新,让这张活在「最近动过」
        // 里排到前面,而它其实什么都没变。
        if issue.branch != tree.branch {
            self.store
                .set_issue_remote(id, &tree.branch, issue.pr_number, issue.remote_number)
                .await?;
        }

        let prompt = format!("#{} {}\n\n{}", issue.number, issue.title, issue.body);
        // 剧本在 buddy 自己的技能目录里,不在用户的仓里。系统提示词只拿到
        // 名字 + 一句话 + 路径,正文让 agent 自己按需读。
        let skills_dir = self.ensure_skill_assets();
        let skill = skills_dir
            .as_deref()
            .and_then(|d| super::bootstrap::skill_pointer(d, &issue.workflow));
        let system_prompt =
            super::bootstrap::agent_system_prompt(&issue, &tree.path, skill.as_ref());
        // 有 `--resume` id 就精确接回那一次对话,不是模糊地接「最近一次」。
        let mut plan = if conv.claude_session_id.is_empty() {
            build_startup_plan(&CLAUDE, &prompt, &system_prompt, &tree.path)
        } else {
            build_resume_plan(&CLAUDE, Some(&conv.claude_session_id), &tree.path)
        }
        .map_err(|e| AppError::Exec(e.to_string()))?;
        if let Some(d) = &skills_dir {
            super::bootstrap::allow_skills_dir(&mut plan, d);
        }

        let meta = ConversationMeta {
            conversation_id: conv.id.uuid(),
            issue_id: id.uuid(),
            claude_session_id: conv.claude_session_id.clone(),
            workspace_path: tree.path.clone(),
            branch_name: tree.branch,
        };
        let (bytes_tx, input_rx) = self.terminal.attach(conv.id.uuid(), meta, None);

        let executor = self.executor.clone();
        let ctx = RunCtx {
            project: issue.project_id.uuid(),
            workflow: uuid::Uuid::nil(),
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
