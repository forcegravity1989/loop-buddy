//! Provider-neutral remote-repo dispatch — a thin enum hiding whether a
//! project's remote is GitHub (`gh` CLI) or CodeHub (`codehub-cli`). Call
//! sites ask [`Remote::for_project`] once, then call methods on the returned
//! [`Remote`]; the provider branch lives here (one `match` per method),
//! not scattered across N call sites in `bw-app`.
//!
//! Pure seam today: only the GitHub arm is wired (delegating to
//! [`crate::github`]). The CodeHub arms return [`RemoteError::CodehubUnwired`]
//! until P3 fills them with real `codehub-cli` shell-outs — the type + the
//! dispatch shape land now so P3 is "fill the arm", not "revisit every call
//! site".

use crate::github::{self, GithubError};
use time::Date;

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error(transparent)]
    Github(#[from] GithubError),
    /// CodeHub arm not wired yet (P3). Reaching this means a codehub project
    /// hit a Remote method before `codehub.rs` existed — honest refusal, not a
    /// silent fake.
    #[error("codehub 远端尚未接线(P3):{0}")]
    CodehubUnwired(&'static str),
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
    /// P3 swaps this for a stateful `CodehubClient { host, path }` holding a
    /// resolved `project_id`; the arms below stay `CodehubUnwired` until then.
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
            other => Err(RemoteError::Github(GithubError::Command(format!(
                "未知 provider: {other}"
            )))),
        }
    }

    /// `gh repo view` (github) / `codehub-cli project view` (codehub, P3).
    /// Read-only. Returns a one-line human detail (name · visibility · pushed).
    pub async fn probe(&self) -> Result<String, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::probe_repo(r).await?),
            Remote::Codehub { .. } => Err(RemoteError::CodehubUnwired("probe")),
        }
    }

    /// `gh issue create` (github) / `codehub-cli issue create` (codehub, P3).
    /// Returns the issue number the remote minted (`gh`'s / codehub's `iid`).
    pub async fn create_issue(&self, title: &str, body: &str) -> Result<u32, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::create_issue(r, title, body).await?),
            Remote::Codehub { .. } => Err(RemoteError::CodehubUnwired("create_issue")),
        }
    }

    /// `gh api search/issues` total_count (github) / `codehub-cli issue|mr
    /// list` paginated count (codehub, P3). Read-only.
    pub async fn collect_count(&self, query: &str, today: Date) -> Result<u64, RemoteError> {
        match self {
            Remote::Github(r) => Ok(github::collect_github_count(r, query, today).await?),
            Remote::Codehub { .. } => Err(RemoteError::CodehubUnwired("collect_count")),
        }
    }
}
