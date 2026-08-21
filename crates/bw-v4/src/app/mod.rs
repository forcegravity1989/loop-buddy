//! 编排:所有用例与守卫都在这一层。
//!
//! 界面发一条 [`Command`],这里执行它,回一串 [`Event`]。守卫也在这里——
//! 状态能不能转、活能不能结第二次、周计划文件在不在,都在动库/动仓之前问清楚。
//!
//! 一屏一屏地看,这层的分工是:
//!
//! - [`project`] —— 接入、改名片、配项目群
//! - [`plan`] —— 开始本周、排期、发版本、缓存对账
//! - [`issue`] —— 建活、▶跑、状态转移
//! - [`bootstrap`] —— 规范铺底与对账
//! - [`chat_notify`] —— 三件事同步进项目群(发出去就算完,不记账)
//! - [`backfill`] —— 老项目历史回填(资产盘点的首次模式)
//! - [`ops`] —— 三张运作活:周计划、资产盘点、规范铺底
//! - [`session`] —— 内嵌终端的 PTY 生命周期
//! - [`tools`] —— 开工工具映射与探活
//! - [`worktree`] —— 每张活一棵自己的 worktree 与分支
//! - [`health`] —— 三条判据的现算

mod backfill;
mod bootstrap;
mod chat_notify;
/// 指标采集:起脚本、传时间窗、读标准输出的 JSON。壳那边「立即采集」直接调它。
pub mod collect;
mod health;
mod issue;
mod ops;
mod plan;
mod progress;
mod project;
mod session;
mod tools;
mod worktree;

pub use bootstrap::{agent_system_prompt, skill_pointer, SkillPointer};
pub use health::collect_health_inputs;
pub use ops::{ops1_title, skill_slug, OPS1_WORKFLOW, OPS2_WORKFLOW, OPS3_WORKFLOW};
pub use progress::{ProgressLine, StepState};

use crate::command::{Command, Event};
use crate::model::ProjectId;
use crate::repo::RepoFileError;
use crate::store::{StoreError, V4Store};
use std::path::PathBuf;
use std::sync::Arc;
use v4_engine::{InteractiveExecutor, TerminalManager};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    RepoFile(#[from] RepoFileError),
    #[error("git:{0}")]
    Git(#[from] crate::git::GitError),
    #[error("执行器:{0}")]
    Exec(String),
    #[error("找不到项目 {0}")]
    NoSuchProject(String),
    #[error("找不到这张活")]
    NoSuchIssue,
    #[error("{0} 还没有配工作区,这一步需要一个真实的仓")]
    NoWorkspace(String),
    /// 状态机不许这么转。如实弹回,不悄悄改成一个合法的转移。
    #[error("不能从「{from}」转到「{to}」")]
    IllegalTransition { from: String, to: String },
    #[error("{0}")]
    Refused(String),
}

pub type Result<T> = std::result::Result<T, AppError>;

/// V4 的编排大脑。
pub struct App {
    pub(crate) store: V4Store,
    /// 工作区根目录。项目没单独配路径时,仓就在 `<root>/<slug>`。
    pub(crate) workspaces_root: PathBuf,
    /// 干活入口的后端。没配真实工作区的项目用自我标注的替身
    /// ([`v4_engine::MockInteractiveExecutor`]),产出带【mock】字样。
    pub(crate) executor: Arc<dyn InteractiveExecutor>,
    /// 活着的 PTY 会话。纯内存,进程死了就没了 —— 会话的**身份**在
    /// `claude_conversation` 表里,那才是重启后接得回来的东西。
    pub(crate) terminal: TerminalManager,
    /// 桌面壳开、headless 不开。见 [`App::with_pty`]。
    pub(crate) pty_enabled: bool,
    /// 长命令边做边报的口子。桌面壳接上,headless 不接。见
    /// [`progress`](crate::app::progress) —— 只为让人看得见,不承担账目。
    pub(crate) progress: Option<tokio::sync::broadcast::Sender<ProgressLine>>,
    /// buddy 自己的资产目录 —— 技能库摊在这儿(`<asset_root>/skills/`)。
    /// **不在任何一个用户项目的仓里**:用户的 `.gitignore` 怎么写不该由 buddy
    /// 决定,而 buddy 自带的剧本每个项目都一样,没有复制 N 份的道理。
    pub(crate) asset_root: PathBuf,
}

impl App {
    pub fn new(
        store: V4Store,
        workspaces_root: impl Into<PathBuf>,
        executor: Arc<dyn InteractiveExecutor>,
    ) -> Self {
        let workspaces_root = workspaces_root.into();
        Self {
            // 默认落在工作区根目录旁边的 `.bw-assets/`。桌面壳会用
            // [`App::with_asset_root`] 换成库文件旁边那份;headless 例子就用
            // 这个默认值,跑完随临时目录一起没。
            asset_root: workspaces_root.join(".bw-assets"),
            store,
            workspaces_root,
            executor,
            terminal: TerminalManager::new(),
            pty_enabled: false,
            progress: None,
        }
    }

    /// buddy 自己的资产放哪。**改工作区根目录不会跟着搬** —— 这里放的是
    /// buddy 自带的东西,和用户把仓放在哪没关系。
    pub fn with_asset_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.asset_root = root.into();
        self
    }

    /// buddy 技能库摊在哪。开工前 [`App::ensure_skill_assets`] 保证它是齐的。
    pub fn skills_dir(&self) -> PathBuf {
        self.asset_root.join("skills")
    }

    /// 把 buddy 自带的技能摊到 [`App::skills_dir`],返回那个目录。
    ///
    /// 幂等:内容一致就不写。**写不下去不算开工失败** —— 调用方拿到 `None`
    /// 就不给 agent 指技能这条路,如实少一段,不编一个路径出来。
    pub fn ensure_skill_assets(&self) -> Option<PathBuf> {
        let dir = self.skills_dir();
        for pack in crate::standard::skills::all() {
            let path = dir.join(&pack.rel);
            if std::fs::read_to_string(&path).is_ok_and(|on_disk| on_disk == pack.raw) {
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok()?;
            }
            std::fs::write(&path, pack.raw).ok()?;
        }
        Some(dir)
    }

    /// 把「一步一句」的回执接出去。不接的话长命令照跑,只是没人看得见中间过程。
    pub fn with_progress(mut self, tx: tokio::sync::broadcast::Sender<ProgressLine>) -> Self {
        self.progress = Some(tx);
        self
    }

    /// 报一步。**没人订就丢掉** —— 报不出去绝不影响命令本身。
    pub(crate) fn step(&self, line: ProgressLine) {
        if let Some(tx) = &self.progress {
            let _ = tx.send(line);
        }
    }

    pub fn store(&self) -> &V4Store {
        &self.store
    }

    pub fn workspaces_root(&self) -> &std::path::Path {
        &self.workspaces_root
    }

    /// 某个项目的仓在哪。项目行里配了就用那条路径,没配就是
    /// `<工作区根目录>/<slug>`。
    pub async fn workspace_of(&self, id: ProjectId) -> Result<PathBuf> {
        let p = self
            .store
            .project(id)
            .await?
            .ok_or_else(|| AppError::NoSuchProject(id.uuid().to_string()))?;
        Ok(self.workspace_at(&p.slug, &p.workspace_path))
    }

    /// 仓该落在哪:项目行里配了路径就用它,没配就是 `<工作区根目录>/<slug>`。
    /// **接入的时候项目行还不存在**(要先把仓取下来才建行),所以这条规矩单
    /// 独提出来一份,两处共用 —— 两边各写一遍就会走散。
    pub(crate) fn workspace_at(&self, slug: &str, workspace_path: &str) -> PathBuf {
        if workspace_path.trim().is_empty() {
            self.workspaces_root.join(slug)
        } else {
            PathBuf::from(workspace_path.trim())
        }
    }

    /// 唯一的入口。
    pub async fn dispatch(&mut self, cmd: Command) -> Result<Vec<Event>> {
        match cmd {
            Command::CreateProject {
                slug,
                intent,
                remote,
                workspace_path,
            } => {
                self.create_project(slug, intent, remote, workspace_path)
                    .await
            }
            Command::RemoveProject { project_id } => self.remove_project(project_id).await,
            Command::ReopenSession { issue_id } => self.reopen_session(issue_id).await,
            Command::EditProjectCard { project_id, intent } => {
                self.edit_project_card(project_id, intent).await
            }
            Command::SetProjectChat {
                project_id,
                provider,
                group_id,
                notify,
            } => {
                self.set_project_chat(project_id, provider, group_id, notify)
                    .await
            }
            Command::RunStandardBootstrap { project_id } => {
                self.run_standard_bootstrap(project_id, true).await
            }
            Command::ReconcileStandard { project_id } => self.reconcile_standard(project_id).await,
            Command::StartWeekPlanning { project_id, week } => {
                self.start_week_planning(project_id, week).await
            }
            Command::ConfirmWeekDraft {
                project_id,
                week,
                titles,
            } => self.confirm_week_draft(project_id, week, titles).await,
            Command::CreateIssue {
                project_id,
                title,
                body,
                category,
                kind,
                origin,
                week_of,
            } => {
                self.create_issue(project_id, title, body, category, kind, origin, week_of)
                    .await
            }
            Command::ScheduleIssue { id, week_of } => self.schedule_issue(id, week_of).await,
            Command::ReorderIssue { id, after } => self.reorder_issue(id, after).await,
            Command::SetIssueWorkflow { id, workflow } => {
                self.set_issue_workflow(id, workflow).await
            }
            Command::SetCurrentVersion {
                project_id,
                version,
            } => self.set_current_version(project_id, version).await,
            Command::CutRelease {
                project_id,
                version,
                note,
                included,
            } => self.cut_release(project_id, version, note, included).await,
            Command::RefreshIssueCacheFromPlan { project_id, week } => {
                self.refresh_issue_cache(project_id, week).await
            }
            Command::RunIssue { id } => self.run_issue(id).await,
            Command::SubmitIssueWork { id } => self.submit_issue_work(id).await,
            Command::SetWorkspacesRoot { path } => self.set_workspaces_root(path).await,
            Command::TransitionIssue { id, to } => self.transition_issue(id, to).await,
            Command::BlockIssue { id, reason } => self.block_issue(id, reason).await,
            Command::SaveToolMapping {
                project_id,
                category,
                tool,
                workflow,
            } => {
                self.save_tool_mapping(project_id, category, tool, workflow)
                    .await
            }
            Command::ProbeTool { name } => self.probe_tool(name).await,
            Command::MarkNotifySeen { project_id, at } => {
                self.mark_notify_seen(project_id, at).await
            }
            Command::PullWorkspace { project_id } => self.pull_workspace(project_id).await,
            Command::TickScheduler { project_id } => self.tick_scheduler(project_id).await,
            Command::MergeAndSettle { id } => self.merge_and_settle(id).await,
            Command::SyncNotifyToChat {
                issue_id,
                event_type,
            } => self.sync_notify_to_chat(issue_id, event_type).await,
            Command::BackfillHistory { project_id } => self.backfill_history(project_id).await,
            Command::CancelRun { id } => self.cancel_run(id).await,
            Command::TerminalInput {
                conversation_id,
                bytes,
            } => self.terminal_input(conversation_id, bytes).await,
            Command::TerminalResize {
                conversation_id,
                cols,
                rows,
            } => self.terminal_resize(conversation_id, cols, rows).await,
        }
    }
}
