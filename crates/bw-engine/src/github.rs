//! GitHub shell-out — mints or adopts a GitHub repo via the `gh` CLI, same
//! subprocess pattern `workspace.rs` uses for local git. Relies entirely on
//! the user's own `gh auth login` on this machine; no token handling here.

use crate::workspace::{commit_initial, git_in};
use std::path::Path;
use std::process::Stdio;

#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    #[error("gh 未安装或不在 PATH")]
    NotInstalled,
    #[error("gh 命令失败:{0}")]
    Command(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubRepoRef {
    pub owner: String,
    pub repo: String,
    pub html_url: String,
    pub private: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubRepoSummary {
    pub owner: String,
    pub repo: String,
    pub private: bool,
    pub updated_at: String,
}

fn spawn_err(e: std::io::Error) -> GithubError {
    if e.kind() == std::io::ErrorKind::NotFound {
        GithubError::NotInstalled
    } else {
        GithubError::Command(e.to_string())
    }
}

fn stderr_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

async fn current_login() -> Result<String, GithubError> {
    let output = tokio::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Mint a brand-new GitHub repo under the authenticated user's account and
/// clone it into `dest_root/<slug>`, then make the same first commit
/// `provision_git_workspace` makes locally (so `is_owned_workspace` correctly
/// reports this repo as workbench-owned) and push it.
pub async fn create_repo(
    slug: &str,
    private: bool,
    dest_root: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<GithubRepoRef, GithubError> {
    let owner = current_login().await?;
    let vis_flag = if private { "--private" } else { "--public" };
    let output = tokio::process::Command::new("gh")
        .current_dir(dest_root)
        .args(["repo", "create", slug, vis_flag, "--clone"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let dir = dest_root.join(slug);
    commit_initial(&dir, readme_title, readme_body)
        .await
        .map_err(|e| GithubError::Command(format!("初始提交失败:{e}")))?;
    git_in(&dir, &["push", "-u", "origin", "HEAD"])
        .await
        .map_err(|e| GithubError::Command(format!("推送失败:{e}")))?;
    Ok(GithubRepoRef {
        owner: owner.clone(),
        repo: slug.to_string(),
        html_url: format!("https://github.com/{owner}/{slug}"),
        private,
    })
}

/// Clone an already-existing GitHub repo the user picked into `dest`.
pub async fn clone_repo(
    owner: &str,
    repo: &str,
    dest: &Path,
) -> Result<GithubRepoRef, GithubError> {
    let owner_repo = format!("{owner}/{repo}");
    let output = tokio::process::Command::new("gh")
        .args(["repo", "clone", &owner_repo, &dest.to_string_lossy()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let view = tokio::process::Command::new("gh")
        .args(["repo", "view", &owner_repo, "--json", "isPrivate"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    let private = if view.status.success() {
        serde_json::from_slice::<serde_json::Value>(&view.stdout)
            .ok()
            .and_then(|v| v.get("isPrivate").and_then(|b| b.as_bool()))
            .unwrap_or(false)
    } else {
        false
    };
    Ok(GithubRepoRef {
        owner: owner.to_string(),
        repo: repo.to_string(),
        html_url: format!("https://github.com/{owner_repo}"),
        private,
    })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoJson {
    name_with_owner: String,
    is_private: bool,
    updated_at: String,
}

/// List repos owned by the authenticated user — the "接入已有仓" picker's
/// data source. Read-only, no local filesystem side effects.
pub async fn list_repos(limit: u32) -> Result<Vec<GithubRepoSummary>, GithubError> {
    let output = tokio::process::Command::new("gh")
        .args([
            "repo",
            "list",
            "--json",
            "nameWithOwner,isPrivate,updatedAt",
            "--limit",
            &limit.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let rows: Vec<RepoJson> = serde_json::from_slice(&output.stdout)
        .map_err(|e| GithubError::Command(format!("解析 gh repo list 输出失败:{e}")))?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let (owner, repo) = r.name_with_owner.split_once('/')?;
            Some(GithubRepoSummary {
                owner: owner.to_string(),
                repo: repo.to_string(),
                private: r.is_private,
                updated_at: r.updated_at,
            })
        })
        .collect())
}
