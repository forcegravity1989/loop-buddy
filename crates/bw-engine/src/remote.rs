//! Provider-neutral remote-repo dispatch — a thin enum hiding whether a
//! project's remote is GitHub (`gh` CLI) or CodeHub (`codehub-cli`). Call
//! sites ask [`Remote::for_project`] once, then call methods on the returned
//! [`Remote`]; the provider branch lives here (one `match` per method),
//! not scattered across N call sites in `bw-app`.
//!
//! Both arms are wired: GitHub delegates to [`crate::github`] (`gh`), CodeHub
//! delegates to [`crate::codehub`] (`codehub-cli`). The type + dispatch shape
//! landed in P2 with only the GitHub arm filled; P3 filled the CodeHub arm
//! with real `codehub-cli` shell-outs — no `CodehubUnwired` variant remains
//! (call sites never match on `Remote`; they call methods, so adding a
//! provider means filling the arm here, not revisiting N call sites in
//! `bw-app`).

use crate::codehub::{self, CodehubError};
use crate::github::{self, GithubError};
use std::path::Path;
use time::Date;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error(transparent)]
    Github(#[from] GithubError),
    #[error(transparent)]
    Codehub(#[from] CodehubError),
    /// `for_project` 收到一个既非 `github` 也非 `codehub` 的 provider 串——配置
    /// 错(不是 github/codehub 侧的运行时错),独立 variant,不冒充 GithubError。
    #[error("未知 provider(既非 github 也非 codehub):{0}")]
    UnknownProvider(String),
}

/// A project's remote repo, provider-dispatched. Build via
/// [`Remote::for_project`] (from a project's `provider`/`remote_host`/
/// `remote_path`) or directly when a connector's `config` already carries a
/// github `owner/repo`. Never matched at call sites — call methods.
pub enum Remote {
    /// `"owner/repo"`. `github.com` is implied — `gh` is github.com-global and
    /// never takes a host param, so the stored `remote_host="github.com"` is
    /// carried for symmetry/Enterprise-future but unused by every github call.
    Github(String),
    /// `host` (green/yellow/inner-source domain) + `path` (`"org/repo"`).
    /// Stateless today: every method re-passes `host`/`path` to the matching
    /// `codehub::xxx` shell-out (P3 wired all three arms). A future stateful
    /// `CodehubClient { host, path }` holding a resolved `project_id` would
    /// save the per-call `project view`, but the arms work as-is without it.
    Codehub { host: String, path: String },
}

impl Remote {
    /// Factory from a project's stored identity triple. `provider="github"`
    /// (or the legacy `""` default — pre-C16 存量行) → `Remote::Github(path)`;
    /// `provider="codehub"` → `Remote::Codehub { host, path }`. An unknown
    /// `provider` ⇒ `Err`. Callers gate on an empty `remote_path` *before*
    /// this (the honest no-remote case); reaching here with an empty path is
    /// a caller bug, not a quiet `Ok`.
    pub fn for_project(provider: &str, host: &str, path: &str) -> Result<Self, RemoteError> {
        let path = path.trim();
        match provider.trim() {
            "github" | "" => Ok(Remote::Github(path.to_string())),
            "codehub" => Ok(Remote::Codehub {
                host: host.trim().to_string(),
                path: path.to_string(),
            }),
            other => Err(RemoteError::UnknownProvider(other.to_string())),
        }
    }

    /// `gh repo view` (github) / `codehub-cli project view` (codehub, P3).
    /// Read-only. Returns a one-line human detail (name · visibility · pushed).
    pub async fn probe(&self) -> Result<String, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::probe_repo(r).await?),
            Remote::Codehub { host, path } => Ok(codehub::probe(host, path).await?),
        }
    }

    /// `gh issue create` (github) / `codehub-cli issue create` (codehub, P3).
    /// Returns the issue number the remote minted (`gh`'s / codehub's `iid`).
    pub async fn create_issue(&self, title: &str, body: &str) -> Result<u32, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::create_issue(r, title, body).await?),
            Remote::Codehub { host, path } => {
                Ok(codehub::create_issue(host, path, title, body).await?)
            }
        }
    }

    /// `gh api search/issues` total_count (github) / `codehub-cli issue|mr
    /// list --jq length` count (codehub). Read-only. codehub 查询口径见
    /// [`codehub::collect_count`](crate::codehub::collect_count)(P3 最小词汇
    /// `issues:<state>`/`mrs:<state>`,复杂窗口留 P5)。
    pub async fn collect_count(&self, query: &str, today: Date) -> Result<u64, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::collect_github_count(r, query, today).await?),
            Remote::Codehub { host, path } => {
                Ok(codehub::collect_count(host, path, query, today).await?)
            }
        }
    }

    /// Open a merge/pull request on the remote for the run's `bw/issue-<n>`
    /// branch — the **return-path** counterpart of [`Self::create_issue`]:
    /// stage + commit + push the run's edits, then open a PR (github) or MR
    /// (codehub) a human later merges to reach `InReview`→`Done`. Github
    /// delegates to [`github::open_pr`] (`Created`/`Adopted` preserved);
    /// codehub shells `codehub-cli mr create` (`Created` only — no `Adopted`,
    /// fails honestly if an MR already exists). **Never merges.**
    ///
    /// Bug③ (2026-07-30): before this, `run_issue_now` called
    /// `github::open_pr` directly → `gh pr create` crashed on codehub remotes
    /// (gh doesn't know codehub) → issue stuck `InProgress`, no `InReview`,
    /// and the post-merge `SyncMetricsFile` never ran → trio metrics never
    /// loaded. Routing through the factory makes codehub open a real MR.
    pub async fn create_mr(
        &self,
        workspace: &Path,
        issue_number: u32,
        title: &str,
    ) -> Result<github::PrOpened, RemoteError> {
        match self {
            Remote::Github(_) => Ok(github::open_pr(workspace, issue_number, title).await?),
            Remote::Codehub { host, path } => {
                Ok(codehub::create_mr(host, path, workspace, issue_number, title).await?)
            }
        }
    }

    /// Merge the open PR/MR — the **human验收** action (one-click merge → the
    /// caller settles `Done`). Github: `gh pr merge --squash`; codehub:
    /// `codehub-cli mr merge <iid> --squash -y`. On `Err` the Issue stays
    /// `InReview` retryable — never fabricated, never reverse-settled. Only
    /// ever called from `MergeIssuePr` (a human click), never from any
    /// run/executor path. Bug③ (2026-07-30): before this, `MergeIssuePr`
    /// crashed `gh pr merge` on codehub remotes.
    pub async fn merge_mr(&self, pr_number: u32) -> Result<(), RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::merge_pr(r, pr_number).await?),
            Remote::Codehub { host, path } => Ok(codehub::merge_mr(host, path, pr_number).await?),
        }
    }

    /// V1 Issue2 Phase2a: read back whether an open PR/MR exists for the
    /// issue's `bw/issue-<n>` branch — the InReview detection poller's
    /// query (读回为证: buddy checks the remote itself, not agent self-report).
    /// Github: `gh pr list --head <branch> --state open` (delegates to
    /// [`github::open_pr_for_branch`]); codehub: `codehub-cli mr list
    /// --source-branch <branch> --state opened` (delegates to
    /// [`codehub::open_mr_for_branch`]). `Ok(None)` = no open PR/MR — the
    /// honest "nothing to review yet" answer. Read-only, zero side effects.
    pub async fn open_mr_for_branch(&self, branch: &str) -> Result<Option<u32>, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::open_pr_for_branch(r, branch).await?),
            Remote::Codehub { host, path } => {
                Ok(codehub::open_mr_for_branch(host, path, branch).await?)
            }
        }
    }
}
