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

/// Create (idempotently) a real git workspace at `dir`.
///
/// - Fresh directory: `git init` + a real `README.md` (from the caller's own
///   project data, never invented) + one first commit, authored explicitly as
///   the workbench so repo history says truthfully who made it.
/// - Existing repo (`dir/.git` present): a no-op — re-provisioning must never
///   touch a workspace that already has real history.
pub async fn provision_git_workspace(
    dir: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<(), ProvisionError> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| ProvisionError::CreateDir(e.to_string()))?;
    git_in(dir, &["init", "-q"]).await?;
    commit_initial(dir, readme_title, readme_body).await
}

/// Write the workbench's opening README/.gitignore and make the first
/// commit, authored as the workbench. Split out of `provision_git_workspace`
/// so `bw_engine::github::create_repo` can reuse the exact same first-commit
/// authorship on a directory `gh repo create --clone` already initialized
/// (the `.git`-exists early return above doesn't apply there — the repo is
/// real but has zero commits yet).
pub(crate) async fn commit_initial(
    dir: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<(), ProvisionError> {
    let readme = format!("# {readme_title}\n\n{readme_body}\n");
    std::fs::write(dir.join("README.md"), readme)
        .map_err(|e| ProvisionError::Write(e.to_string()))?;
    std::fs::write(dir.join(".gitignore"), "/target\n")
        .map_err(|e| ProvisionError::Write(e.to_string()))?;
    git_in(dir, &["add", "-A"]).await?;
    git_in(
        dir,
        &[
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            "chore: workspace 开仓(builders-workbench 托管起点)",
        ],
    )
    .await?;
    Ok(())
}

/// Write (or overwrite) one file in a workspace and commit it, authored as
/// the workbench. Idempotent: when the bytes are unchanged git reports a
/// clean tree and we return `Ok` — re-confirming a creation-flow step is not
/// a new fact. Only real git/write failures error.
pub async fn commit_file(
    dir: &Path,
    rel_path: &str,
    content: &str,
    message: &str,
) -> Result<(), ProvisionError> {
    let full = dir.join(rel_path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ProvisionError::Write(e.to_string()))?;
    }
    std::fs::write(&full, content).map_err(|e| ProvisionError::Write(e.to_string()))?;
    git_in(dir, &["add", "--", rel_path]).await?;
    // `git commit` exits non-zero when nothing is staged; that is the
    // idempotent re-confirm case, not a failure.
    let out = crate::win_cmd::tokio_cmd("git")
        .current_dir(dir)
        .args([
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            message,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ProvisionError::Git(e.to_string()))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    if stderr.contains("nothing to commit") || stderr.contains("no changes") {
        Ok(())
    } else {
        Err(ProvisionError::Git(format!(
            "commit {rel_path} → {}",
            stderr.trim()
        )))
    }
}

/// 纯写盘,**不** git add/commit —— 与 `commit_file` 的区别就在这里。
///
/// 用于 BW 托管的派生文件(技能物化):它们每次 run 都可能重写,提交进 git 会
/// 把工作区的历史刷成噪音,而且这些文件的正本在库里,不在仓里。父目录不存在
/// 就建。
pub async fn write_file(dir: &Path, rel_path: &str, content: &str) -> Result<(), ProvisionError> {
    let path = dir.join(rel_path);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ProvisionError::Write(e.to_string()))?;
    }
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| ProvisionError::Write(e.to_string()))?;
    Ok(())
}

/// General form of [`stage_commit_push`] — takes an arbitrary commit message
/// instead of formatting one from an issue number. Used by the project.toml
/// PR path ([`crate::github::open_project_init_pr`] /
/// [`crate::codehub::create_project_init_mr`]) where there is no issue number
/// (the commit is `chore: …`, not `issue #<n>: …`). Same idempotent
/// nothing-to-commit semantics, same authorship, same push.
pub(crate) async fn stage_commit_push_msg(
    workspace: &Path,
    branch: &str,
    commit_message: &str,
) -> Result<(), ProvisionError> {
    git_in(workspace, &["add", "-A"]).await?;
    let commit = crate::win_cmd::tokio_cmd("git")
        .current_dir(workspace)
        .args([
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            commit_message,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ProvisionError::Git(e.to_string()))?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        let stdout = String::from_utf8_lossy(&commit.stdout);
        let combined = format!("{stdout}\n{stderr}");
        if !(combined.contains("nothing to commit") || combined.contains("no changes")) {
            return Err(ProvisionError::Git(format!(
                "提交活分支改动失败:{}",
                combined.trim()
            )));
        }
    }
    git_in(workspace, &["push", "-u", "origin", branch]).await?;
    Ok(())
}

/// plan/17 S2: RAII guard for an issue worktree. `Drop` removes the worktree
/// (best-effort, sync) so **every** exit path of `run_issue_body` — early
/// `?`, `Ok`, `Err`, panic — cleans up. Never panics on a remove failure
/// (best-effort: an orphaned worktree is orphaned git metadata, not data
/// loss; the branch stays on `origin` for review/merge). Sync `git` in `Drop`
/// because `Drop` can't `await` — the call is sub-second and this is the
/// one place cleanup lives, so a brief block is the honest trade.
pub struct IssueWorktreeGuard {
    main_workspace: PathBuf,
    path: Option<PathBuf>,
}

impl IssueWorktreeGuard {
    /// Construct a guard. `path = None` makes a no-op guard (used when
    /// provisioning failed or the project isn't `pr_eligible`); cleanup then
    /// does nothing. `Some(path)` schedules `git worktree remove --force` +
    /// `prune` for `Drop`.
    pub fn new(main_workspace: PathBuf, path: Option<PathBuf>) -> Self {
        Self {
            main_workspace,
            path,
        }
    }

    /// The worktree path this guard owns, or `None` if none was provisioned.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

impl Drop for IssueWorktreeGuard {
    fn drop(&mut self) {
        let Some(p) = &self.path else {
            return;
        };
        let Some(s) = p.to_str() else {
            return;
        };
        for args in [
            ["worktree", "remove", "--force", s].as_slice(),
            ["worktree", "prune"].as_slice(),
        ] {
            let _ = crate::win_cmd::std_cmd("git")
                .current_dir(&self.main_workspace)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
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

/// Is this a workspace the workbench owns — i.e. it minted the repo and
/// authored a root commit? Bound, pre-existing repos are never owned, so the
/// workbench must not rewrite their files. False on any doubt (no `.git`, no
/// commits, or no root commit authored by the workbench).
pub async fn is_owned_workspace(dir: &Path) -> bool {
    if !dir.join(".git").exists() {
        return false;
    }
    let out = match crate::win_cmd::tokio_cmd("git")
        .current_dir(dir)
        .args(["log", "--max-parents=0", "--format=%an"])
        .output()
        .await
    {
        Ok(o) => o,
        Err(_) => return false,
    };
    if !out.status.success() {
        return false;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|a| a.trim() == "Builders' Workbench")
}

/// One file's real change stat between two commits (`git diff --numstat`).
/// Binary files (numstat prints `-`) record 0/0 — present, size unknown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub added: u32,
    pub deleted: u32,
}

/// What really changed between two recorded commits — `git diff --numstat
/// from..to`, parsed. Read-only; errors surface as strings so a detail view
/// can show "对比不可用:…" honestly instead of an empty list pretending
/// nothing changed.
pub async fn diff_numstat(
    workspace: &str,
    from: &str,
    to: &str,
) -> Result<Vec<FileChange>, String> {
    let output = crate::win_cmd::tokio_cmd("git")
        .current_dir(workspace)
        .args(["diff", "--numstat", &format!("{from}..{to}")])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text
        .lines()
        .filter_map(|line| {
            let mut it = line.splitn(3, '\t');
            let added = it.next()?.trim();
            let deleted = it.next()?.trim();
            let path = it.next()?.trim();
            if path.is_empty() {
                return None;
            }
            Some(FileChange {
                path: path.to_string(),
                added: added.parse().unwrap_or(0),
                deleted: deleted.parse().unwrap_or(0),
            })
        })
        .collect())
}
