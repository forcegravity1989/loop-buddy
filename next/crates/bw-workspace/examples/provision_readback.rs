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
//! 5. **供给只造不删,升级成断言**(评审 task-s5a-review.md Important-1
//!    附带的 Minor-6):前四步全跑完、调用早已返回、局部变量早就出了作用
//!    域之后,重新读回三个路径——仍然存在。再扫一遍 `bw-workspace` 的
//!    源码,断言公开面上没有任何看起来像删除的函数、没有任何 `impl
//!    Drop`(v1 那个「作用域结束自动删」的守卫如果被加回来,一定会在这
//!    两处至少露一次面)。突变自证:临时在 `provision.rs` 里加一个
//!    `pub fn remove_worktree(...)` 或 `impl Drop for X`,这一节必须变红。
//! 6. **C1(next 紧急修,2026-08-11,终审 Critical-1)**:目标兄弟目录已
//!    经存在、但不是合法的 git 工作树(没有 `.git`)——真造一个这样的目
//!    录(写一个标记文件进去,模拟用户自己留在旁边的东西),调用应该如
//!    实报 `ProvisionError::Occupied`,标记文件必须原样还在。突变自证:
//!    临时把 `provision.rs` 那一分支改回 `let _ =
//!    std::fs::remove_dir_all(&sibling)`,这一节必须变红(标记文件消
//!    失、返回值也从 `Err` 变 `Ok`)。
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

use bw_workspace::git_support::{commit_initial, git_in, ProvisionError};
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

/// 「删除语义」的疑似关键字——用来扫 `bw-workspace/src` 的公开函数名。这份
/// 清单只是「看起来像删除」的字面猜测,不是穷举;它的价值不在覆盖所有可能
/// 的命名,而在于给 v1 那种 `remove_worktree`/`delete_*`/`cleanup_*` 式的
/// 命名一个会被当场拦下的靶子——真加回一个自动清理函数,几乎不可能绕开
/// 这几个词全部。
const DELETE_LIKE_KEYWORDS: &[&str] = &[
    "remove", "delete", "destroy", "cleanup", "clean_up", "purge", "teardown", "rm_",
];

/// 扫 `bw-workspace/src` 下所有 `.rs` 文件,断言:①公开面上(`pub fn`/
/// `pub async fn`)没有任何函数名沾上「删除语义关键字」;②没有任何
/// `impl Drop`(v1 那个「作用域结束自动删工作树」的守卫就是靠这个)。
/// 返回发现的问题列表——空列表 = 断言通过。
///
/// **为什么是源码扫描,不是运行期调用**:「crate 公开面上不存在删除函
/// 数」这件事本身没有运行期行为可触发——它是一个「这个符号存在不存在」
/// 的静态事实。设计稿 §7.1 第 6 节已经在用同一手法(全库搜索,断言编排层
/// 没有第二条写「已完成」的路径),这里是同一条老规矩的又一次应用。
/// **突变自证**:临时在 `src/provision.rs` 里加一行 `pub fn
/// remove_worktree(...)` 或 `impl Drop for Foo`,这个函数必须把它扫出来。
fn scan_no_delete_surface(src_dir: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    let mut stack = vec![src_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let Ok(content) = std::fs::read_to_string(&path) else {
                findings.push(format!("{}: 读取失败,无法扫描", path.display()));
                continue;
            };
            for (lineno, line) in content.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("impl Drop") {
                    findings.push(format!(
                        "{}:{}: 出现 `impl Drop`(v1 那个「作用域结束自动删工作树」的守卫就是\
                         靠这个实现的——供给只造不删,这个 crate 不该有任何 Drop 清理)",
                        path.display(),
                        lineno + 1
                    ));
                }
                let is_pub_fn =
                    trimmed.starts_with("pub fn ") || trimmed.starts_with("pub async fn ");
                if is_pub_fn {
                    if let Some((_, after_fn)) = trimmed.split_once("fn ") {
                        let name = after_fn.split('(').next().unwrap_or("").trim();
                        let lower = name.to_lowercase();
                        for kw in DELETE_LIKE_KEYWORDS {
                            if lower.contains(kw) {
                                findings.push(format!(
                                    "{}:{}: 公开函数 `{name}` 的名字含疑似删除语义关键字 `{kw}`",
                                    path.display(),
                                    lineno + 1
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    findings
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

    println!("== 4 · 供给只造不删,升级成断言(不再只是文档口径)==");
    // 调用早就返回了,局部变量也早就"出了作用域"意义上该被清理的时间点
    // (如果这里真有一个 v1 式的 Drop 守卫,它此刻应该已经删过了)——现在
    // 再读一次,路径应该原样还在。
    if main_workspace.exists() && worktree_path.exists() && second_worktree.exists() {
        println!("  ✓ 三个路径(主工作区 + 两棵 issue 工作树)全部原样还在,没有被任何自动清理动过");
    } else {
        eprintln!(
            "ASSERT FAILED: 供给只造不删——三个路径都应该还在,实得 main={} #42={} #7={}",
            main_workspace.exists(),
            worktree_path.exists(),
            second_worktree.exists()
        );
        all_ok = false;
    }
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let findings = scan_no_delete_surface(&src_dir);
    if findings.is_empty() {
        println!(
            "  ✓ 源码扫描 {}:公开面上没有疑似删除函数,也没有 impl Drop",
            src_dir.display()
        );
    } else {
        for f in &findings {
            eprintln!("ASSERT FAILED: {f}");
        }
        all_ok = false;
    }
    println!();

    println!("== 5 · C1(next 紧急修):目标目录已占且非法工作树,必须如实报错,不许代删 ==");
    // 真造一个「兄弟目录存在、但不是合法工作树」的场景——用一个前四步都
    // 没碰过的活编号,手工在它应该落点的路径上建一个目录 + 写一个标记文
    // 件(模拟用户自己留在旁边的东西:笔记/手工克隆/别的工具生成/`.git`
    // 被清掉的旧工作树),再调用 `provision_issue_worktree`,断言:①必须
    // 如实报错(`ProvisionError::Occupied`),不能返回 `Ok`;②标记文件必
    // 须原样还在——如果它消失了,说明目录被整删重造了,C1 的洞又开着。
    let occupied_issue = 99u32;
    let stem = main_workspace
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let occupied_sibling = main_workspace
        .parent()
        .expect("主工作区应该有 parent(临时目录下的一层)")
        .join(format!("{stem}-issue-{occupied_issue}"));
    if let Err(e) = std::fs::create_dir_all(&occupied_sibling) {
        eprintln!("ASSERT FAILED: 建「占位、非工作树」目录应该成功,实得错误: {e}");
        eprintln!("PROVISION_READBACK_FAILED");
        return ExitCode::FAILURE;
    }
    let marker = occupied_sibling.join("not-a-worktree-please-dont-delete-me.txt");
    if let Err(e) = std::fs::write(
        &marker,
        "这是用户自己留在这里的文件,不是 BW 造的工作树——provision_issue_worktree \
         如果把这个目录整删重来,这个文件会消失,C1 就复发了。\n",
    ) {
        eprintln!("ASSERT FAILED: 写占位标记文件应该成功,实得错误: {e}");
        eprintln!("PROVISION_READBACK_FAILED");
        return ExitCode::FAILURE;
    }
    match provision_issue_worktree(&main_workspace, occupied_issue).await {
        Ok(p) => {
            eprintln!(
                "ASSERT FAILED: 目标目录已存在且不是合法工作树时应该如实报错,实得 Ok({})\
                 ——说明代码把它当过期残留整删重造了,C1 又开了这个洞",
                p.display()
            );
            all_ok = false;
        }
        Err(ProvisionError::Occupied(msg)) => {
            println!("  ✓ 如实报错(ProvisionError::Occupied),没有代删:{msg}");
        }
        Err(other) => {
            eprintln!("ASSERT FAILED: 应该报 ProvisionError::Occupied,实得别的错误变体: {other}");
            all_ok = false;
        }
    }
    if marker.exists() {
        println!("  ✓ 占位标记文件原样还在,目录没有被删除/清空");
    } else {
        eprintln!("ASSERT FAILED: 占位标记文件应该原样还在,实得已消失——目录被删除重造了");
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
