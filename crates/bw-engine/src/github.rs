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

/// C4 · issue 身份映射: 经 `gh issue create` 真开一个 GitHub issue,返回
/// `gh` 铸造的 issue 号(这就是这张 Issue 的跨系统身份)。`gh issue create`
/// 成功时把新 issue 的 URL 打到 stdout(如
/// `https://github.com/owner/repo/issues/42`),号即 URL 末段。只做 create
/// ——close/PR 是另一票的事。
pub async fn create_issue(owner_repo: &str, title: &str, body: &str) -> Result<u32, GithubError> {
    let output = tokio::process::Command::new("gh")
        .args([
            "issue", "create", "--repo", owner_repo, "--title", title, "--body", body,
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
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    url.rsplit('/')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| GithubError::Command(format!("无法从 gh 输出解析 issue 号:{url:?}")))
}

// ─────────────────────── C5 · PR 验收环 (plan/13 D3) ───────────────────────
//
// 三件套 + 收尾:提 PR / 查 PR 状态 / merge PR,外加 merge 后的 issue 补关。
// 关键纪律:**执行器只提 PR、永不 merge**——`open_pr` 在执行器路径里被调用,
// `merge_pr` 只从 bw-app 的人手命令(MergeIssuePr)里调用,两者物理隔离。
// 验收=人 merge;issue 关闭是 merge 的后果(PR body 的 `Closes #<n>` 关键字让
// GitHub 自动关单,`merge_pr` 后再幂等核对补关)。BW 绝不反向改写 GitHub:检测
// 到的漂移(PR 已被网页 merge 等)只反映、不 reopen、不改写远端。

/// The work branch a run's changes live on for a given GitHub issue —
/// `bw/issue-<github_number>`. One deterministic branch per Issue so a retry
/// re-uses the same branch (and the same PR), never fans out.
pub fn issue_branch(github_number: u32) -> String {
    format!("bw/issue-{github_number}")
}

fn git_err(prefix: &str, e: crate::workspace::ProvisionError) -> GithubError {
    GithubError::Command(format!("{prefix}:{e}"))
}

/// Quarantine a run's work onto the Issue's branch **before** the executor
/// touches anything (plan/13 D3: the executor must never advance the base
/// branch — only a human merge does). Checks out `bw/issue-<n>`, creating it
/// at the current HEAD the first time and re-using it on a retry. All of the
/// run's edits then land on this branch by construction, whatever the executor
/// does (dirty tree or its own commits), leaving the base branch untouched.
pub async fn checkout_issue_branch(
    workspace: &Path,
    github_number: u32,
) -> Result<String, GithubError> {
    let branch = issue_branch(github_number);
    // First run: create the branch at HEAD. Retry: the branch already exists,
    // so `-b` fails and we plain-checkout it (keeping any prior branch work).
    if git_in(workspace, &["checkout", "-b", &branch])
        .await
        .is_err()
    {
        git_in(workspace, &["checkout", &branch])
            .await
            .map_err(|e| git_err("切到活分支失败", e))?;
    }
    Ok(branch)
}

/// 提 PR (plan/13 D3): commit whatever the run produced on the Issue branch,
/// push it, and open a pull request whose body carries `Closes #<github_number>`
/// so a later human merge auto-closes the Issue — one action验收. Returns the
/// PR number `gh` minted (parsed from the PR URL it prints, same idiom as
/// `create_issue`). Every step is fallible and the caller treats any failure as
/// "提 PR 失败不炸 run": the run's own accounting stands, `pr_number` stays 0,
/// the Issue is retryable. **Never merges** — this only opens the PR.
pub async fn open_pr(
    workspace: &Path,
    github_number: u32,
    title: &str,
) -> Result<u32, GithubError> {
    let branch = issue_branch(github_number);
    // Stage + commit the run's edits. The executor may have left a dirty tree
    // (the common `acceptEdits` case) or committed itself; either way this
    // makes the branch carry a real, mergeable diff. "nothing to commit" is the
    // idempotent already-committed case, not a failure.
    git_in(workspace, &["add", "-A"])
        .await
        .map_err(|e| git_err("暂存活分支改动失败", e))?;
    let commit = tokio::process::Command::new("git")
        .current_dir(workspace)
        .args([
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            &format!("issue #{github_number}: {title}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        if !(stderr.contains("nothing to commit") || stderr.contains("no changes")) {
            return Err(GithubError::Command(format!(
                "提交活分支改动失败:{}",
                stderr.trim()
            )));
        }
    }
    git_in(workspace, &["push", "-u", "origin", &branch])
        .await
        .map_err(|e| git_err("推送活分支失败", e))?;
    // gh infers the base repo + default base branch from the origin remote in
    // `workspace`; `Closes #<n>` in the body is what auto-closes the Issue on
    // merge (D3: issue 关闭是 merge 的后果).
    let body = format!(
        "BW 执行器为 Issue #{github_number} 提交的改动,等待人工 merge 验收。\n\nCloses #{github_number}"
    );
    let output = tokio::process::Command::new("gh")
        .current_dir(workspace)
        .args([
            "pr", "create", "--head", &branch, "--title", title, "--body", &body,
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
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    url.rsplit('/')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| GithubError::Command(format!("无法从 gh 输出解析 PR 号:{url:?}")))
}

/// 查 PR 状态 (plan/13 D3; C7 之前本票自用): `gh pr view --json state` → the
/// raw state string (`OPEN` / `MERGED` / `CLOSED`). Read-only, no side effects
/// — used to detect drift (a PR merged on the web) without ever rewriting it.
pub async fn pr_state(owner_repo: &str, pr_number: u32) -> Result<String, GithubError> {
    gh_json_field(&[
        "pr",
        "view",
        &pr_number.to_string(),
        "--repo",
        owner_repo,
        "--json",
        "state",
        "--jq",
        ".state",
    ])
    .await
}

/// merge PR (plan/13 D3): the **human** verification action — merges the PR,
/// which (via `Closes #<n>`) closes the Issue. Called only from bw-app's
/// `MergeIssuePr` command, never from any executor/run path. Squash-merge keeps
/// the base branch history one-commit-per-Issue.
pub async fn merge_pr(owner_repo: &str, pr_number: u32) -> Result<(), GithubError> {
    let output = tokio::process::Command::new("gh")
        .args([
            "pr",
            "merge",
            &pr_number.to_string(),
            "--repo",
            owner_repo,
            "--squash",
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
    Ok(())
}

/// `gh issue view --json state` → `OPEN` / `CLOSED`. Lets `MergeIssuePr` verify
/// the `Closes #<n>` keyword actually closed the Issue and补关 idempotently if
/// GitHub didn't (rare, but honest belt-and-suspenders).
pub async fn issue_state(owner_repo: &str, github_number: u32) -> Result<String, GithubError> {
    gh_json_field(&[
        "issue",
        "view",
        &github_number.to_string(),
        "--repo",
        owner_repo,
        "--json",
        "state",
        "--jq",
        ".state",
    ])
    .await
}

/// Idempotent补关: close the GitHub issue directly. Only called after a merge
/// when `issue_state` still reads `OPEN` (the `Closes` keyword should have done
/// it). `gh issue close` on an already-closed issue is a no-op success.
pub async fn close_issue(owner_repo: &str, github_number: u32) -> Result<(), GithubError> {
    let output = tokio::process::Command::new("gh")
        .args([
            "issue",
            "close",
            &github_number.to_string(),
            "--repo",
            owner_repo,
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
    Ok(())
}

/// Run a read-only `gh ... --json ... --jq ...` and return the trimmed stdout.
async fn gh_json_field(args: &[&str]) -> Result<String, GithubError> {
    let output = tokio::process::Command::new("gh")
        .args(args)
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
