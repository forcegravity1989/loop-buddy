//! v1 `crates/bw-engine/src/workspace.rs` 的**部分**收编——不是整体搬迁。
//! 只收了 `github.rs`/`codehub.rs` 真实引用到的四项:`git_in`(内部 git
//! shell-out 辅助)、`commit_initial`(开仓首提交)、`stage_commit_push`
//! (活分支 stage+commit+push,`open_pr`/`create_mr` 共用)、`ProvisionError`
//! (三家共用的错误类型)。这四项**函数体零改写**,一字未动。
//!
//! v1 原文件另外七项(`provision_git_workspace`/`commit_file`/`write_file`/
//! `IssueWorktreeGuard`/`provision_issue_worktree`/`is_owned_workspace`/
//! `FileChange`+`diff_numstat`)未被 `github.rs`/`codehub.rs` 引用,本次
//! 没有搬(主控裁决 #1:workspace 辅助单副本落这里;这份文件不是那份文件的
//! 完整拷贝,是它的一个真实被引用子集)。删减清单与核对方式见 next 切片二B
//! 的 commit 正文与 `task-s2b-report.md`。
//!
//! 本文件顶部这段说明是新写的(不是"移植"——整体搬迁的 `github.rs`/
//! `codehub.rs` 保留了 v1 原有的文件头;这份文件因为是选摘,原文件头会
//! 误导,所以换成准确描述这四项来历的说明)。下面四项各自的文档注释(含
//! 对已删除项 `provision_git_workspace`/`commit_file` 的引用)是 v1 原文,
//! 一字未动。

use std::path::Path;
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
    let output = tokio::process::Command::new("git")
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

/// Stage **all** of a run's edits on its work branch, commit them (idempotent
/// — a clean tree left by an executor that committed its own work is *not* a
/// failure), and push the branch to `origin` so a PR/MR can be opened on it.
/// Shared by [`crate::github::open_pr`] and [`crate::codehub::create_mr`] so
/// the F5 nothing-to-commit footgun (2026-07-24: 干净树被误判成「提交活分支
/// 改动失败」,PR 环整段被卡死) lives in `stage_commit_push` (the older
/// [`commit_file`] path still uses a pre-F5 stderr-only check — a known
/// pre-existing gap, left untouched here). The commit is authored as the
/// workbench; `issue_number` + `title` form its message (`issue #<n>:
/// <title>`). Never merges — opening the PR/MR is the caller's next step.
pub(crate) async fn stage_commit_push(
    workspace: &Path,
    branch: &str,
    issue_number: u32,
    title: &str,
) -> Result<(), ProvisionError> {
    git_in(workspace, &["add", "-A"]).await?;
    let commit = tokio::process::Command::new("git")
        .current_dir(workspace)
        .args([
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            &format!("issue #{issue_number}: {title}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| ProvisionError::Git(e.to_string()))?;
    if !commit.status.success() {
        // git prints "nothing to commit, working tree clean" on STDOUT, not
        // stderr — an executor that committed its own work leaves a clean
        // tree, and that idempotent case must not read as a failure (F5).
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
