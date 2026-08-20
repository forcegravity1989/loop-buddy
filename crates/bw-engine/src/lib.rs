//! `bw-engine` — everything that touches the outside world for the kernel:
//! the interactive `claude` executor (embedded-terminal PTY on every platform
//! via [`pty_backend`], plus a self-labeled [`MockInteractiveExecutor`] for
//! projects without a real workspace), workspace/worktree provisioning,
//! git evidence collection, the GitHub / CodeHub remotes, and the `.bw/*.toml`
//! file readers. No UI, no SQL — `bw-app` orchestrates, this crate executes.
//!
//! The 2026-07 phase-loop engine (`Engine` / `Executor` / `MockExecutor` /
//! `ClaudeCliExecutor` shelling out to `claude -p`) was deleted on
//! 2026-08-18: every Issue ▶跑 goes through [`InteractiveExecutor`] now, and
//! the phase-loop had no remaining entry point.

#![forbid(unsafe_code)]

use bw_core::{ProjectId, WorkflowId};

pub mod claude_bin;
pub mod claude_cli;
pub mod codehub;
pub mod connectors_file;
pub mod evidence;
pub mod git_log;
pub mod github;
pub mod interactive_cli;
pub mod metrics_file;
pub mod project_file;
pub mod pty_backend;
pub mod remote;
mod terminal_manager;
pub mod win_cmd;
pub mod workspace;

pub use win_cmd::{is_windows_script, std_cmd, tokio_cmd};

pub use claude_bin::{claude_binary_candidates, resolve_claude_binary, which_on_path};
pub use claude_cli::ClaudeCliConfig;
pub use codehub::{CodehubError, CodehubRepoRef, CodehubRepoSummary};
pub use connectors_file::{ConnectorDef, ConnectorKind, ConnectorsFile, ConnectorsFileError};
pub use evidence::{EvidenceError, WorkspaceEvidence, WorkspaceFile};
pub use git_log::{read_commits, GitCommit, GitLogError};
pub use github::{GithubError, GithubRepoRef, GithubRepoSummary};
pub use interactive_cli::{
    build_consultation_resume_plan, build_project_context_block, build_resume_plan,
    build_startup_plan, InteractiveCliExecutor, InteractiveExecutor, LaunchPlan,
    MockInteractiveExecutor, PromptInjectionMode, PtyInput, SkillOutput, TuiAgentConfig, CLAUDE,
    CONSULTATION_APPEND_PROMPT, CURSOR,
};
pub use metrics_file::{
    CollectKind, CollectPlan, MetricDef, MetricsFile, MetricsFileError, NorthStarDef,
};
pub use project_file::{ProjectFile, ProjectFileError};
pub use terminal_manager::{
    ConversationMeta, TerminalManager, TerminalSession, OUTPUT_BATCH_CAP, OUTPUT_BATCH_MAX_BYTES,
};
pub use workspace::{
    provision_git_workspace, provision_issue_worktree, IssueWorktreeGuard, ProvisionError,
};

/// Context handed to an executor for a run.
#[derive(Clone, Copy, Debug)]
pub struct RunCtx {
    pub project: ProjectId,
    pub workflow: WorkflowId,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("executor failed: {0}")]
    Failed(String),
}
