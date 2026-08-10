//! 造一棵活的、按活分支切好的 git 工作树——**从 v1 `crates/bw-engine/src/
//! workspace.rs` 搬过来**(next 切片五A,design-s5-hexpanel.md §6.2/§6.4/
//! §9)。这是运行管理器(切片四B)开工时要的那个「已经存在、分支已经切好
//! 的工作区」的第一个真实供给方——切片四B 落地时,`bw-app::run::StartRun`
//! 的 `workspace`/`branch` 两个入参就是必填的,但当时没有任何一条生产路径
//! 能把它们交给运行管理器(plan/23 §10 第 7 条)。本文件清偿这条缺口。
//!
//! **移植纪律**:函数体不改。唯一的真实改动是 `crate::github::issue_branch`
//! 这一处调用——v1 原文里它调用同一个 crate 内 `github.rs` 模块的一个纯
//! 文本函数(`format!("bw/issue-{n}")`,零逻辑);`bw-workspace` 不依赖
//! `bw-connector`(工作区 crate 谁也不依赖,design §10.1 第 2 条),没有
//! `github` 模块可调,因此把这一行为不变的纯文本计算原样内联成
//! [`issue_branch`](同名同实现),就地调用。这是「只改引用路径,不改行
//! 为」的又一个实例(与 next 切片一A 的既有判例同类),不算函数体改写。
//!
//! **主控裁决 §6.3 钉死的一处不搬**:v1 原文件里与 `provision_issue_worktree`
//! 配套的 `IssueWorktreeGuard`(作用域结束 `Drop` 里强制删工作树)**不搬**
//! ——上游按工作目录编码存会话,续接必须同路径;一次交付运行结束后,它
//! 的会话很可能还活着(「降级为咨询」就是为这种情况准备的语义:名额放
//! 开、会话不杀、工作区不清)。自动删工作树等于让那个还活着的会话再也接
//! 不回来。新形态:**供给只造不删**;清理是一个显式动作(未来一条清理用
//! 例,或人自己删)。工作树会越积越多,这是已知的、如实登记的代价(plan/23
//! §10 第 14 条),不是漏掉的。

use crate::git_support::{git_in, ProvisionError};
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// 活编号 → 分支名的映射:`bw/issue-<n>`。v1 原文住在 `github.rs` 里(见本
/// 文件头注释);这里是它在 `bw-workspace` 侧的落点,内容逐字相同
/// (`format!("bw/issue-{issue_number}")`),供 [`provision_issue_worktree`]
/// 就地调用,不跨 crate 反查 `bw-connector`。
pub fn issue_branch(issue_number: u32) -> String {
    format!("bw/issue-{issue_number}")
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
    let branch = issue_branch(issue_number);
    let parent = main_workspace.parent().ok_or_else(|| {
        ProvisionError::CreateDir(format!("主工作区无 parent:{}", main_workspace.display()))
    })?;
    let stem = main_workspace
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let sibling = parent.join(format!("{stem}-issue-{issue_number}"));
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
        let _ = tokio::process::Command::new("git")
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
