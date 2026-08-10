//! 三个 git shell-out 辅助 + 它们共用的错误类型——**从 `bw-connector`
//! `upstream/workspace.rs` 移过来的单副本**(next 切片五A,
//! design-s5-hexpanel.md §6.2/§9)。
//!
//! **搬迁理由**(不是重新发明):`bw-connector` 的 `github.rs`/`codehub.rs`
//! 真实引用 `git_in`(内部 git shell-out)/`commit_initial`(开仓首提交)/
//! `stage_commit_push`(活分支提交并推送)/`ProvisionError`(三家共用的错误
//! 类型)这四项。它们本身不是「对外连接」——本地 git 读写是内建工作区函数
//! (design §6.2「为什么它不是连接器」),不该住在连接器 crate 里当「内建函
//! 数」(那是切片二裁决 #1 留下的命名将就,切片三开放问题 4 已经登记过)。
//! 现在 `bw-workspace` 有了一个正经的家,`bw-connector` 反过来依赖这个
//! crate 复用这四项——单副本,不复制。
//!
//! **移植纪律**:函数体一个字不改;唯一动的是可见性(`pub(crate)` → `pub`
//! ——从「connector crate 内可见」放宽到「对 bw-connector 等外部 crate 可
//! 见」)与模块路径。这与 next 切片一A「只改引用路径」是同一类改动,
//! `bw-connector` 侧的移植纪律见其 commit 正文。
//!
//! 下面四项各自的文档注释是 v1 原文(`crates/bw-engine/src/workspace.rs`),
//! 经 `bw-connector` `upstream/workspace.rs` 转手一次,一字未动地转手到这
//! 里——包括其中对已删除项 `provision_git_workspace`/`commit_file` 的引用
//! (那两项 v1 有、`bw-connector` 未选摘、`bw-workspace` 也没有;引用原样
//! 保留是「不改文档注释」这条移植纪律的自然结果,不是新的说明性文字)。

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

pub async fn git_in(dir: &Path, args: &[&str]) -> Result<(), ProvisionError> {
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
pub async fn commit_initial(
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
pub async fn stage_commit_push(
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
