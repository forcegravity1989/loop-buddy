//! `provision_readback` — `bw-workspace` 最小行为读回指挥器(next 切片
//! 五A,design-s5-hexpanel.md §10.2「A 自己要真跑一遍」)。
//!
//! 不开界面,直接调用 [`bw_workspace::provision_issue_worktree`],真造一棵
//! git 工作树并读回:
//!
//! 1. 真 `git init` + 真首提交,造一个「主工作区」。
//! 2. 造一棵活的 issue 工作树 → 读回:路径真的存在、`.git` 是一个指向主仓
//!    的 worktree 链接(不是普通仓)、分支真的是 `bw/issue-<n>`
//!    (`git rev-parse --abbrev-ref HEAD` 读回)、主工作区自己的分支没有被
//!    动过。
//! 3. 再造一次同一个 issue 的工作树(幂等复用)→ 必须返回同一个路径、不
//!    报错、不产生第二棵工作树(`git worktree list` 读回条数不变)。
//! 4. 造第二个 issue 的工作树 → 与第一个各自独立、互不干扰。
//!
//! **供给只造不删**(design §6.3,主控裁决 #12 附带确认):本指挥器造出来
//! 的主工作区与工作树跑完**不删**,路径打印出来给人手工复核——这不是遗
//! 漏,是与产品语义一致的真实纪律(自动删工作树会让还活着的上游会话再也
//! 接不回来)。为了不让反复手跑这份指挥器把临时目录堆满,开跑前会清掉
//! **更早**几次跑遗留的同前缀目录(同 `bw-store` `examples/store_guards.rs`
//! 的 `clear_stale_dbs` 先例),只留本次这一份。
//!
//! 跑法:`cd next && cargo run -p bw-workspace --example provision_readback`
//! 退出码 0 且末行 `PROVISION_READBACK_OK` = 全部断言通过。

use bw_workspace::git_support::{commit_initial, git_in};
use bw_workspace::provision::{issue_branch, provision_issue_worktree};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use uuid::Uuid;

const DIR_NAME_PREFIX: &str = "bw-workspace-provision-readback-";

/// 本次跑要用的主工作区目录——uuid v4 命名,同 `store_guards`/`run_races`
/// 的既有先例。
fn fresh_main_workspace() -> PathBuf {
    std::env::temp_dir().join(format!("{DIR_NAME_PREFIX}{}", Uuid::new_v4()))
}

/// 清掉临时目录里所有**更早**的 `bw-workspace-provision-readback-*` 目录
/// (含它们各自的 issue 工作树兄弟目录,按同前缀一并扫到)——跑完之后临时
/// 目录里只剩本次这一份,下一次跑不会意外读到任何一次旧跑的残留。
fn clear_stale_dirs(current: &Path) {
    let dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let current_name = current.file_name().map(|n| n.to_os_string());
    for entry in entries.flatten() {
        let name = entry.file_name();
        if current_name.as_deref() == Some(name.as_os_str()) {
            continue;
        }
        if name.to_string_lossy().starts_with(DIR_NAME_PREFIX) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

async fn git_stdout(dir: &Path, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new("git")
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[tokio::main]
async fn main() -> ExitCode {
    let main_workspace = fresh_main_workspace();
    clear_stale_dirs(&main_workspace);

    println!("== provision_readback · bw-workspace 最小行为读回指挥器(next 切片五A) ==");
    println!(
        "本次主工作区:{}   ← 跑完不删(供给只造不删,§6.3),给人手工复核",
        main_workspace.display()
    );
    println!();

    let mut all_ok = true;

    println!("== 0 · 造主工作区(真 git init + 真首提交)==");
    if let Err(e) = std::fs::create_dir_all(&main_workspace) {
        eprintln!("ASSERT FAILED: 建主工作区目录应该成功,实得错误: {e}");
        eprintln!("PROVISION_READBACK_FAILED");
        return ExitCode::FAILURE;
    }
    if let Err(e) = git_in(&main_workspace, &["init", "-q"]).await {
        eprintln!("ASSERT FAILED: git init 应该成功,实得错误: {e}");
        eprintln!("PROVISION_READBACK_FAILED");
        return ExitCode::FAILURE;
    }
    if let Err(e) = commit_initial(
        &main_workspace,
        "provision_readback 示例主工作区",
        "本目录由 next 切片五A 的 provision_readback 指挥器真实创建,供 \
         provision_issue_worktree 的行为读回使用。",
    )
    .await
    {
        eprintln!("ASSERT FAILED: 首提交应该成功,实得错误: {e}");
        eprintln!("PROVISION_READBACK_FAILED");
        return ExitCode::FAILURE;
    }
    let Some(main_branch_before) =
        git_stdout(&main_workspace, &["rev-parse", "--abbrev-ref", "HEAD"]).await
    else {
        eprintln!("ASSERT FAILED: 读主工作区当前分支失败");
        eprintln!("PROVISION_READBACK_FAILED");
        return ExitCode::FAILURE;
    };
    println!("主工作区已就绪,当前分支: {main_branch_before}");
    println!();

    println!("== 1 · 造一棵活的 issue 工作树(issue #42)==");
    let issue_number = 42u32;
    let expected_branch = issue_branch(issue_number);
    let worktree_path = match provision_issue_worktree(&main_workspace, issue_number).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: provision_issue_worktree 应该成功,实得错误: {e}");
            eprintln!("PROVISION_READBACK_FAILED");
            return ExitCode::FAILURE;
        }
    };
    println!("工作树路径: {}", worktree_path.display());

    if worktree_path.exists() {
        println!("  ✓ 路径真的存在");
    } else {
        eprintln!("ASSERT FAILED: 工作树路径应该存在,实得不存在: {worktree_path:?}");
        all_ok = false;
    }
    let dot_git = worktree_path.join(".git");
    if dot_git.is_file() {
        println!("  ✓ .git 是一个文件(worktree 链接,不是普通仓的 .git 目录)");
    } else {
        eprintln!("ASSERT FAILED: {dot_git:?} 应该是一个文件(git worktree 的链接标志),实得不是");
        all_ok = false;
    }
    let branch_in_worktree =
        git_stdout(&worktree_path, &["rev-parse", "--abbrev-ref", "HEAD"]).await;
    match &branch_in_worktree {
        Some(b) if b == &expected_branch => {
            println!("  ✓ 工作树分支 = {b}(与 issue_branch(42) 一致)")
        }
        other => {
            eprintln!("ASSERT FAILED: 工作树分支应为 {expected_branch:?},实得 {other:?}");
            all_ok = false;
        }
    }
    let main_branch_after =
        git_stdout(&main_workspace, &["rev-parse", "--abbrev-ref", "HEAD"]).await;
    if main_branch_after.as_deref() == Some(main_branch_before.as_str()) {
        println!("  ✓ 主工作区自己的分支没有被动过(仍是 {main_branch_before})");
    } else {
        eprintln!(
            "ASSERT FAILED: 主工作区分支应保持 {main_branch_before:?} 不变,实得 {main_branch_after:?}"
        );
        all_ok = false;
    }
    println!();

    println!("== 2 · 再造一次同一个 issue 的工作树(幂等复用)==");
    let worktree_count_before = git_stdout(&main_workspace, &["worktree", "list", "--porcelain"])
        .await
        .map(|s| s.lines().filter(|l| l.starts_with("worktree ")).count())
        .unwrap_or(0);
    let reprovisioned = match provision_issue_worktree(&main_workspace, issue_number).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: 幂等复用调用应该成功,实得错误: {e}");
            eprintln!("PROVISION_READBACK_FAILED");
            return ExitCode::FAILURE;
        }
    };
    if reprovisioned == worktree_path {
        println!("  ✓ 返回同一个路径: {}", reprovisioned.display());
    } else {
        eprintln!(
            "ASSERT FAILED: 幂等复用应返回同一个路径,首次 {worktree_path:?},第二次 {reprovisioned:?}"
        );
        all_ok = false;
    }
    let worktree_count_after = git_stdout(&main_workspace, &["worktree", "list", "--porcelain"])
        .await
        .map(|s| s.lines().filter(|l| l.starts_with("worktree ")).count())
        .unwrap_or(0);
    println!(
        "  git worktree list 条数: {worktree_count_before} → {worktree_count_after}(应不变,不产生第二棵工作树)"
    );
    if worktree_count_before == worktree_count_after {
        println!("  ✓ 条数不变");
    } else {
        eprintln!(
            "ASSERT FAILED: 幂等复用不应产生新的 worktree 条目,实得 {worktree_count_before} → {worktree_count_after}"
        );
        all_ok = false;
    }
    println!();

    println!("== 3 · 造第二个 issue(#7)的工作树,与第一个互不干扰 ==");
    let second_issue = 7u32;
    let second_worktree = match provision_issue_worktree(&main_workspace, second_issue).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: 第二个 issue 的工作树应该成功,实得错误: {e}");
            eprintln!("PROVISION_READBACK_FAILED");
            return ExitCode::FAILURE;
        }
    };
    if second_worktree != worktree_path && second_worktree.exists() {
        println!("  ✓ 第二棵工作树独立存在: {}", second_worktree.display());
    } else {
        eprintln!("ASSERT FAILED: 第二棵工作树应与第一棵不同且真实存在,实得 {second_worktree:?}");
        all_ok = false;
    }
    let second_branch = git_stdout(&second_worktree, &["rev-parse", "--abbrev-ref", "HEAD"]).await;
    let expected_second_branch = issue_branch(second_issue);
    if second_branch.as_deref() == Some(expected_second_branch.as_str()) {
        println!("  ✓ 第二棵工作树分支 = {expected_second_branch}");
    } else {
        eprintln!(
            "ASSERT FAILED: 第二棵工作树分支应为 {expected_second_branch:?},实得 {second_branch:?}"
        );
        all_ok = false;
    }
    println!();

    if all_ok {
        println!("主工作区留在:{}", main_workspace.display());
        println!("issue #42 工作树留在:{}", worktree_path.display());
        println!("issue #7  工作树留在:{}", second_worktree.display());
        println!("PROVISION_READBACK_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("主工作区留在:{}", main_workspace.display());
        eprintln!("PROVISION_READBACK_FAILED");
        ExitCode::FAILURE
    }
}
