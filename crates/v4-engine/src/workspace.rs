//! Workspace provisioner — the all-in-one-codebase default's mechanical arm:
//! every project gets exactly one real git repo, and this module mints it
//! (directory + `git init` + one real first commit). The only *writing*
//! subprocess module in the engine; everything it creates is immediately
//! verifiable on disk (`.git/`, `README.md`, `git log`), nothing is staged
//! for later or simulated.

use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, thiserror::Error)]
pub enum ProvisionError {
    #[error("创建目录失败:{0}")]
    CreateDir(String),
    #[error("git 命令失败:{0}")]
    Git(String),
    #[error("写初始文件失败:{0}")]
    Write(String),
}

pub(crate) async fn git_in(dir: &Path, args: &[&str]) -> Result<(), ProvisionError> {
    let output = crate::win_cmd::tokio_cmd("git")
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ProvisionError::Git(e.to_string()))?;
    if !output.status.success() {
        return Err(ProvisionError::Git(format!(
            "git {} → {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Where an issue's worktree lives: the sibling directory
/// `<main_workspace>-issue-<n>`. **The one place that rule is written down** —
/// [`provision_issue_worktree`] creates it here, and cleanup paths (V4 settles
/// the issue, then removes the tree) find it here. `None` when the main
/// workspace has no parent directory or a non-UTF-8 name.
pub fn issue_worktree_path(main_workspace: &Path, issue_number: u32) -> Option<PathBuf> {
    let parent = main_workspace.parent()?;
    let stem = main_workspace.file_name().and_then(|n| n.to_str())?;
    Some(parent.join(format!("{stem}-issue-{issue_number}")))
}

/// plan/17 S2: provision an isolated per-issue git worktree off the main
/// workspace's HEAD (master), so two concurrent/back-to-back issue runs in
/// one project never collide on the shared working tree. The worktree lives
/// in a sibling directory `<main_workspace>-issue-<n>` and carries branch
/// `bw/issue-<n>` (created here if missing, same retry-fallback semantics
/// as `github::checkout_issue_branch`). Main workspace stays on master —
/// only the issue worktree carries the issue branch. The caller wraps the
/// returned path in an [`IssueWorktreeGuard`] so cleanup is automatic.
pub async fn provision_issue_worktree(
    main_workspace: &Path,
    issue_number: u32,
) -> Result<PathBuf, ProvisionError> {
    let branch = crate::github::issue_branch(issue_number);
    let sibling = issue_worktree_path(main_workspace, issue_number).ok_or_else(|| {
        ProvisionError::CreateDir(format!(
            "算不出 worktree 路径(主工作区没有上级目录,或者目录名不是 UTF-8):{}",
            main_workspace.display()
        ))
    })?;
    let sibling_str = sibling
        .to_str()
        .ok_or_else(|| {
            ProvisionError::CreateDir(format!("worktree 路径非 UTF-8:{}", sibling.display()))
        })?
        .to_string();
    // A prior run that crashed before its guard's `Drop` ran leaves the
    // sibling dir behind. Prune stale worktree metadata; if the dir survives
    // prune with a `.git` worktree file, it's a live worktree for this branch
    // — reuse it (idempotent retry, mirroring `checkout_issue_branch`).
    if sibling.exists() {
        let _ = crate::win_cmd::tokio_cmd("git")
            .current_dir(main_workspace)
            .args(["worktree", "prune"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;
        if sibling.join(".git").exists() {
            return Ok(sibling);
        }
        // Stale leftover dir with no worktree metadata — clear it so the
        // `worktree add` below creates a fresh one.
        let _ = std::fs::remove_dir_all(&sibling);
    }
    // First run: `worktree add -b <branch> <path>` from main HEAD (master).
    // Retry (branch already exists from a prior run): fall back to checking
    // the existing branch out into the new worktree.
    if git_in(
        main_workspace,
        &["worktree", "add", "-b", &branch, &sibling_str],
    )
    .await
    .is_err()
    {
        git_in(main_workspace, &["worktree", "add", &sibling_str, &branch]).await?;
    }
    Ok(sibling)
}

/// One file's real change stat between two commits (`git diff --numstat`).
/// Binary files (numstat prints `-`) record 0/0 — present, size unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: u32,
    pub deleted: u32,
}
