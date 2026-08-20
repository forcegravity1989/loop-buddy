//! 每张活一棵自己的 git worktree。
//!
//! **为什么必须有**:两张活可能同时在跑。一张的 agent 正在改文件,另一张的
//! agent 也在改同一个目录,谁都不知道自己看到的是不是对方写到一半的东西;人
//! 自己手上那份没写完的改动还会被一起卷进某次提交。worktree 把每张活隔在自己
//! 的目录和自己的分支上,主检出永远不动。
//!
//! 供给这一步**复用 bw-engine 的
//! [`provision_issue_worktree`](bw_engine::workspace::provision_issue_worktree)**
//! ——和 V3 用的是同一份实现,一个字都没改:在主仓的**兄弟目录**
//! `<主仓名>-issue-<号>` 里开一棵,分支 `bw/issue-<号>`,已经存在就原样接着用
//! (重跑幂等)。
//!
//! **不给 RAII guard,不跑完就删**。V3 是跑完 `Drop` 里把 worktree 删掉;V4
//! 不能那样:会话屏的文件树和 diff 读的就是这棵 worktree,跑完立刻删,人点开
//! 会话只看到一片空白,agent 还没提交的改动也一起没了。V4 的 worktree 活到这
//! 张活**结清**那一刻,而且只在它干净(没有未提交改动)时才收,见
//! [`remove_if_clean`]。
//!
//! **两个根的分工**(别混):进得了版本控制的件写进 worktree,走分支和 MR;
//! 仓自己的 `.gitignore` 拒收的件(buddy 自己的仓就忽略 `.claude/`)属于**本机
//! 检出**,不属于任何分支 —— 它们要落在主工作区,否则技能包只存在于这一张活
//! 的 worktree 里,下一张活开新 worktree 就读不到剧本了。见 [`mirror_ignored`]。

use super::{AppError, Result};
use std::path::{Path, PathBuf};

/// 一张活干活的地方。
pub(super) struct IssueTree {
    /// agent 真正在里面干活的目录。
    pub path: PathBuf,
    /// 这棵树在哪个分支上。
    pub branch: String,
    /// 真的隔出来了没有。`false` = 这个工作区不是 git 仓,开不了 worktree,
    /// 只能就地干 —— 如实标出来,不假装隔离过。
    pub isolated: bool,
}

/// 给这张活开(或接着用)一棵 worktree。
///
/// 不是 git 仓就退回主工作区,`isolated=false`;是 git 仓但开不出来就**如实
/// 报错**,不悄悄退回主工作区 —— 那等于把「两张活撞车」的风险藏起来。
pub(super) async fn provision(main: &Path, number: u32) -> Result<IssueTree> {
    if !crate::git::is_repo(main).await {
        let branch = crate::git::current_branch(main)
            .await
            .unwrap_or_else(|_| "—(不是 git 仓)".into());
        return Ok(IssueTree {
            path: main.to_path_buf(),
            branch,
            isolated: false,
        });
    }
    let path = bw_engine::workspace::provision_issue_worktree(main, number)
        .await
        .map_err(|e| AppError::Exec(format!("给第 {number} 号活开 worktree 没成:{e}")))?;
    Ok(IssueTree {
        path,
        branch: bw_engine::github::issue_branch(number),
        isolated: true,
    })
}

/// 结清时收掉这棵树 —— **只在它干净的时候**。
///
/// 树里还有没提交的改动就原样留着(返回 `false`):那是真实存在的劳动成果,
/// 「这张活我点完成了」不构成删掉它的授权。返回 `true` = 真的收掉了。
pub(super) async fn remove_if_clean(main: &Path, tree: &Path) -> bool {
    if tree == main || !tree.is_dir() {
        return false;
    }
    match crate::git::is_dirty(tree).await {
        Ok(false) => {}
        // 脏的、或者连脏不脏都读不出来 —— 都不动它。
        _ => return false,
    }
    let Some(s) = tree.to_str() else { return false };
    if crate::git::worktree_remove(main, s).await.is_err() {
        return false;
    }
    true
}

/// 把仓拒收的那几个件同步一份到主工作区。
///
/// 只在主工作区**还没有**同名文件时才放 —— 绝不覆盖已经在那儿的东西。返回
/// (放过去了的, 因为已存在而没放的),两串都由调用方如实写进活的说明。
pub(super) fn mirror_ignored(
    main: &Path,
    tree: &Path,
    rels: &[String],
) -> (Vec<String>, Vec<String>) {
    let (mut copied, mut kept) = (Vec::new(), Vec::new());
    for rel in rels {
        let dst = main.join(rel);
        if dst.exists() {
            kept.push(rel.clone());
            continue;
        }
        let src = tree.join(rel);
        if !src.is_file() {
            continue;
        }
        if let Some(parent) = dst.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                continue;
            }
        }
        if std::fs::copy(&src, &dst).is_ok() {
            copied.push(rel.clone());
        }
    }
    (copied, kept)
}
