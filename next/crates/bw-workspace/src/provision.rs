//! 造一棵活的、按活分支切好的 git 工作树——**从 v1 `crates/bw-engine/src/
//! workspace.rs` 搬过来**(next 切片五A,design-s5-hexpanel.md §6.2/§6.4/
//! §9)。这是运行管理器(切片四B)开工时要的那个「已经存在、分支已经切好
//! 的工作区」的第一个真实供给方——切片四B 落地时,`bw-app::run::StartRun`
//! 的 `workspace`/`branch` 两个入参就是必填的,但当时没有任何一条生产路径
//! 能把它们交给运行管理器(plan/23 §10 第 7 条)。本文件清偿这条缺口。
//!
//! **移植纪律**:函数体基本不改,**两处有意分歧**(都不是漏改,逐条登
//! 记):
//!
//! 1. `crate::github::issue_branch` 这一处调用——v1 原文里它调用同一个
//!    crate 内 `github.rs` 模块的一个纯文本函数(`format!("bw/issue-{n}")`,
//!    零逻辑);`bw-workspace` 不依赖 `bw-connector`(工作区 crate 谁也不
//!    依赖,design §10.1 第 2 条),没有 `github` 模块可调,因此把这一行
//!    为不变的纯文本计算原样内联成 [`issue_branch`](同名同实现),就地调
//!    用。这是「只改引用路径,不改行为」的又一个实例(与 next 切片一A 的
//!    既有判例同类),不算函数体改写。
//! 2. **C1(next 紧急修,2026-08-11,终审 Critical-1)**:兄弟目录已存在
//!    且没有 `.git`(不是合法工作树)时,v1 原实现在此处 `let _ =
//!    std::fs::remove_dir_all(&sibling)`——整删重来,删失败还被 `let _ =`
//!    吞掉,错误不报、删除不留痕。这条路径落在用户项目根的兄弟目录上
//!    (不是 BW 自己的数据区),名字是可预测的确定性命名
//!    (`<项目名>-issue-<活编号>`),用户自己在旁边建过同名目录(笔记/手
//!    工克隆/别的工具生成/`.git` 文件被清掉的旧工作树)完全可能——一次
//!    点击触发的无条件递归删除,直接违反「破坏性永不自动」这条产品铁
//!    律。BW 按这条铁律改为**如实失败**:不删任何东西,把路径与发现的
//!    情况说清楚,让人自己去处理。这是对 v1 逐字节移植件的一处有意分
//!    歧,分歧登记在 `plan/23-opc-stitching-rebuild.md` §10(可回报伙伴
//!    修 v1,我们不擅改 v1 分支)。
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
/// only the issue worktree carries the issue branch. **供给只造不删**
/// (主控裁决,design-s5 §6.3):清理不是这里发生的自动行为,而是一个显式
/// 动作(理由见本文件头的模块文档——上游按工作目录编码存会话,续接必须
/// 同路径,自动删了就再也接不回来)。**这一行是评审 task-s5a-review.md
/// Important-1 点名的一处有意的文档更正**:v1 原文这里写的是「调用方把
/// 返回路径包进 `IssueWorktreeGuard`,清理自动发生」,正好讲反了本文件
/// 明令不搬的那件事,不是移植走样。
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
    // 供给只造不删,所以任何一次更早的调用都会把这个兄弟目录原样留在磁
    // 盘上(不只是"崩溃"那一种情况——正常跑完同样不删)。Prune stale
    // worktree metadata; if the dir survives prune with a `.git` worktree
    // file, it's a live worktree for this branch — reuse it (idempotent
    // retry, mirroring `checkout_issue_branch`).
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
        // **只造不删(C1,见本文件头注释「移植纪律」第 2 条)**:目录存在
        // 但不是一棵合法的工作树——不猜它是不是「过期残留」,如实拒绝,
        // 绝不代人整删重来。
        return Err(ProvisionError::Occupied(format!(
            "{} 已存在,但不是一个合法的 git 工作树(没有 .git)。为避免误删\
             用户自己的文件,这里不会自动清空或删除——请确认这个目录的内容\
             后手动处理(移走或删除都可以),再重新开工",
            sibling.display()
        )));
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
