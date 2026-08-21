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

    /// V2-②-I: list open issues on the remote — read-only. Never creates.
    /// Used to rebuild local issue rows for later-comers / out-of-band opens.
    pub async fn list_open_issues(&self) -> Result<Vec<github::RemoteOpenIssue>, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::list_open_issues(r).await?),
            Remote::Codehub { host, path } => Ok(codehub::list_open_issues(host, path).await?),
        }
    }

    /// 在**已经推上去的分支**上开 PR/MR —— 和 [`Self::create_mr`] 的区别是:
    /// 这里不替调用方 `git add -A` 提交,分支上有什么就提什么;正文也由调用方
    /// 给,**不自动挂 `Closes #<n>`**。
    ///
    /// V4 走的是这条:活是本机连续号,和远端 issue 号没有对应关系,拼一句
    /// `Closes #3` 会去关掉那个仓里毫不相干的第 3 号 issue;而且规范铺底只
    /// 提交自己写下去的那几个文件,`add -A` 会把人手上没写完的改动一起打包。
    /// **绝不合入** —— 合入永远是人点的那一下。
    pub async fn create_mr_on_branch(
        &self,
        workspace: &Path,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<github::PrOpened, RemoteError> {
        match self {
            Remote::Github(_) => {
                Ok(github::create_pr_on_branch(workspace, branch, title, body).await?)
            }
            Remote::Codehub { host, path } => Ok(codehub::create_mr_on_branch(
                host, path, workspace, branch, title, body, None,
            )
            .await?),
        }
    }

    /// Merge the open PR/MR. Github: `gh pr merge --squash`; codehub:
    /// `codehub-cli mr merge <iid> --squash -y`. Two call sites with different
    /// human/auto semantics (§7): `MergeIssuePr` — the **human验收** action
    /// (one-click merge → caller settles `Done`); on `Err` the Issue stays
    /// `InReview` retryable, never fabricated, never reverse-settled.
    /// `write_project_toml_pr` — the **auto-merge** of a `.bw/project.toml`
    /// config PR (project.toml is configuration, not an Issue, so auto-merging
    /// it doesn't break "Done 永不自动"; issue PRs are never auto-merged —
    /// that path is unchanged). Bug③ (2026-07-30): before this, `MergeIssuePr`
    /// crashed `gh pr merge` on codehub remotes.
    pub async fn merge_mr(&self, pr_number: u32) -> Result<(), RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::merge_pr(r, pr_number).await?),
            Remote::Codehub { host, path } => Ok(codehub::merge_mr(host, path, pr_number).await?),
        }
    }

    /// V2-② Phase A (§7): open a PR/MR for `.bw/project.toml` on the
    /// `bw/project-init` branch — the first Buddy to adopt an existing repo
    /// writes the project intent as a config PR (not an Issue PR). Buddy then
    /// auto-merges via [`merge_mr`]. Parallels [`create_mr`] but without an
    /// issue number (project.toml is a config file, not an Issue). Github
    /// delegates to [`github::open_project_init_pr`]; codehub to
    /// [`codehub::create_project_init_mr`]. **Never merges** — the caller
    /// (bw-app's creation flow) auto-merges on success, or surfaces a tip on
    /// failure. This is the one exception to "issue PR never auto-merges":
    /// project.toml is configuration, not an Issue (§7,不破「Done 永不自动」).
    pub async fn create_project_init_mr(
        &self,
        workspace: &Path,
        title: &str,
    ) -> Result<github::PrOpened, RemoteError> {
        match self {
            Remote::Github(_) => Ok(github::open_project_init_pr(workspace, title).await?),
            Remote::Codehub { host, path } => {
                Ok(codehub::create_project_init_mr(host, path, workspace, title).await?)
            }
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
