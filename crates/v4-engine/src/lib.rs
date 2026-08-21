//! `v4-engine` —— V4 的能力底座:凡是要碰外部世界的都在这儿。
//!
//! 交互式 `claude` 执行器(各平台都走 [`pty_backend`] 的内嵌终端 PTY,没有真实
//! 工作区的项目退回自我标注的 [`MockInteractiveExecutor`])、工作区与 worktree
//! 供给、git 读数、GitHub / codehub 两个远端。**没有 UI,没有 SQL,也没有任何
//! 业务语义** —— 它只知道「怎么起一个进程、怎么开一棵树、怎么调一次 CLI」。
//!
//! ## 它和 `bw-engine` 是什么关系
//!
//! 这是 2026-08-21 从 `crates/bw-engine` **拷过来接管的**,不是复用。判据是
//! 用户拍板的一句话:V3 那一整个目录最终会被整体删掉,所以 V4 不能有任何一条
//! 依赖指向那边 —— 包括「只依赖 bw-engine」这种看起来很轻的耦合(它自己还依赖
//! `bw-core`,V4 会顺着传递过去)。V3 那份原样留着伺候 V3 直到它被删。
//!
//! 拷过来的同时做了三件减法,所以这份比原来少了一半:
//!
//! 1. **断掉 `bw-core`**。原来四处用到它的 id 类型与 `PlaybookCtx`;现在底座只
//!    认裸 [`uuid::Uuid`],语义类型(`IssueId` 这些)留给上层各自包 —— 底座本来
//!    就不该知道一个 id 是「活」还是「会话」。
//! 2. **删掉只有 V3 用的出口**:仓文件解析(V4 自己在 `bw-v4/src/repo/` 有一
//!    套)、`git log` 读数(V4 有 `bw-v4/src/git.rs`)、`claude` 配置结构,以及
//!    十几个只被 V3 编排调用的远端函数。
//! 3. **删掉两条零调用死链**:`collect_count` 系(5 项)与 `create_mr`/`open_pr`
//!    系(4 项)。前者留下的 `time` 依赖也跟着退场。

#![forbid(unsafe_code)]

pub mod claude_bin;
pub mod codehub;
pub mod evidence;
pub mod github;
pub mod interactive_cli;
pub mod pty_backend;
pub mod remote;
mod terminal_manager;
pub mod win_cmd;
pub mod workspace;

pub use win_cmd::{std_cmd, tokio_cmd};

pub use claude_bin::{resolve_claude_binary, which_on_path};
pub use codehub::{CodehubError, CodehubRepoRef, CodehubRepoSummary};
pub use evidence::{EvidenceError, WorkspaceEvidence, WorkspaceFile};
pub use github::{GithubError, GithubRepoRef, GithubRepoSummary};
pub use interactive_cli::{
    build_resume_plan, build_startup_plan, InteractiveCliExecutor, InteractiveExecutor, LaunchPlan,
    MockInteractiveExecutor, PtyInput, TuiAgentConfig, CLAUDE,
};
pub use terminal_manager::{ConversationMeta, TerminalManager};
pub use workspace::{provision_issue_worktree, ProvisionError};

/// 一次运行的上下文。
///
/// **这两个都是裸 UUID,不是语义类型** —— 底座不该知道一个 id 背后是项目还是
/// workflow,它只负责把这两个值原样带进日志和执行器。上层(`bw-v4`)自己包成
/// `ProjectId` 这类类型,进来之前 `.uuid()` 一下即可。
#[derive(Clone, Copy, Debug)]
pub struct RunCtx {
    pub project: uuid::Uuid,
    pub workflow: uuid::Uuid,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("executor failed: {0}")]
    Failed(String),
}
