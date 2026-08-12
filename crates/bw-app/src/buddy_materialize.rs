//! buddy 规范运行副本物化:把 `docs/buddy/standards/*.md` 写进业务工作区
//! `.claude/buddy/standards/`,带 `.bw-managed` 标记,让 Claude 按需 Read。
//!
//! 与 `skill_materialize` 同构但独立(§4.3 #6):复用「有标记才覆盖、路径必须安全」
//! 的守护模式,但不把两类资产塞进同一个函数或共用版本格式。
//!
//! 关键规则(§4.3):
//! - 不提交进用户项目 Git —— 通过 `git rev-parse --git-path info/exclude` 找本地排除文件,
//!   把 `.claude/buddy/` 加入;worktree 兼容。
//! - 每次物化前核验排除规则 —— 先确认排除可写且规则已生效,再写运行副本。
//! - 无标记 / 符号链接 / 路径不安全 → 拒覆盖报错,不碰用户手写文件。
//! - 写入采用临时文件后替换 —— 不让 Claude 读到半份规范。
//! - 无仓项目不物化 —— 调用方在 `workspace_path` 为空时跳过本模块。

use std::path::{Path, PathBuf};
use std::process::Stdio;

/// `.bw-managed` 标记文件名。与 skill_materialize 同名但内容格式不同
/// (skill = `id + rev`,standards = `buddy-standards + 指纹`),两者互不混用。
const MANAGED_MARKER: &str = ".bw-managed";

/// 物化结果。字段都是**发生过的事**,不是计划 —— 调用方据此留痕。
#[derive(Debug, Default)]
pub struct StandardsMaterializeReport {
    /// 本次真写了盘的文件数。
    pub written: usize,
    /// 版本一致、整条跳过(未写盘)。
    pub unchanged: bool,
}

/// 物化失败原因。失败 = 不启动真实 Claude(§8)。
#[derive(Debug, thiserror::Error)]
pub enum StandardsMaterializeError {
    #[error("无法读取 Git 本地排除文件:{0}")]
    GitExcludeRead(String),
    #[error("无法写入 Git 本地排除文件:{0}")]
    GitExcludeWrite(String),
    #[error("`.claude/buddy/` 已存在但不是 buddy 管理(无 `{MANAGED_MARKER}` 标记)。请移动或更名该目录后重试")]
    ForeignDirectory,
    #[error("`.claude/buddy/` 是符号链接,拒绝覆盖")]
    SymlinkConflict,
    #[error("规范运行副本写入失败:{0}")]
    WriteFailure(String),
}

/// 把 buddy 规范物化到 `<workspace>/.claude/buddy/standards/`。
///
/// 调用方在 `workspace` 非空时调用。返回 `Ok(report)` = 可以启动真实 Claude;
/// `Err` = 不启动,把人话错误给用户(§4.3 #3、§8)。
pub async fn materialize_standards(
    workspace: &Path,
) -> Result<StandardsMaterializeReport, StandardsMaterializeError> {
    let buddy_root = workspace.join(".claude/buddy");
    let standards_root = buddy_root.join("standards");
    let marker_path = buddy_root.join(MANAGED_MARKER);

    let fingerprint = format!(
        "buddy-standards\n{}",
        bw_core::buddy_assets::standards_fingerprint()
    );

    // ── 1. 确认 Git 本地排除规则 ──
    // 先确认当前目录是真实 Git 工作区、排除文件可写且 `.claude/buddy/` 已经被排除,
    // 再写运行副本。任一条件不满足 → 不启动(§4.3 #3)。
    ensure_git_exclude(workspace, ".claude/buddy/")?;

    // ── 2. 冲突保护 ──
    // 目录已存在但不是 buddy 管理(无标记) → 拒覆盖。符号链接 → 拒覆盖。
    if buddy_root.exists() {
        if buddy_root.is_symlink() {
            return Err(StandardsMaterializeError::SymlinkConflict);
        }
        match tokio::fs::read_to_string(&marker_path).await {
            Ok(existing) if existing.trim() == fingerprint.trim() => {
                // 版本一致。但若规范正文被误删(标记在、文件缺),不能当 unchanged
                // (§4.3 #4:内容没变才不重写;文件丢失 ≠ 内容没变)—— 落到下面重写。
                // `standards_files` paths are relative to `.claude/buddy/`
                // (e.g. `standards/metrics.md`) — join from buddy_root, not
                // standards_root, or files land at standards/standards/*.
                let all_present = bw_core::buddy_assets::standards_files()
                    .iter()
                    .all(|(rel, _)| buddy_root.join(rel).exists());
                if all_present {
                    return Ok(StandardsMaterializeReport {
                        written: 0,
                        unchanged: true,
                    });
                }
            }
            Ok(_) => { /* buddy 托管但版本不同 —— 往下重写 */ }
            Err(_) => {
                // 目录在、标记不在 = 用户自己的东西。绝不动。
                return Err(StandardsMaterializeError::ForeignDirectory);
            }
        }
    }

    // ── 3. 写入采用临时文件后替换(§4.3 #7)──
    // 先创建目录,再逐文件写临时文件 → rename,最后写标记。
    // 若本次开始时还没有托管标记,中途失败必须清掉半写目录——否则下次会被
    // 判成 ForeignDirectory,真实开工永久锁死(§8)。
    let had_marker = marker_path.exists();
    let rollback_incomplete = |err: String| -> StandardsMaterializeError {
        if !had_marker {
            let _ = std::fs::remove_dir_all(&buddy_root);
        }
        StandardsMaterializeError::WriteFailure(err)
    };

    tokio::fs::create_dir_all(&standards_root)
        .await
        .map_err(|e| rollback_incomplete(e.to_string()))?;

    let mut written = 0;
    for (rel, content) in bw_core::buddy_assets::standards_files() {
        // rel is relative to `.claude/buddy/` (see buddy_assets::standards_files).
        let target = buddy_root.join(rel);
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| rollback_incomplete(e.to_string()))?;
        }
        write_atomic(&target, content)
            .await
            .map_err(|e| rollback_incomplete(e.to_string()))?;
        written += 1;
    }

    // 写标记文件(原子写)。
    write_atomic(&marker_path, &fingerprint)
        .await
        .map_err(|e| rollback_incomplete(e.to_string()))?;
    written += 1;

    Ok(StandardsMaterializeReport {
        written,
        unchanged: false,
    })
}

/// 确认 `.claude/buddy/` 已加入 Git 本地排除文件(`info/exclude`)。
///
/// 用 `git rev-parse --git-path info/exclude` 找当前工作区实际使用的本地排除
/// 文件(普通 clone 与 worktree 都能命中正确位置)。如果 `.claude/buddy/` 还
/// 不在排除列表里,追加一行。不修改项目的 `.gitignore`(§4.3 #2)。
fn ensure_git_exclude(workspace: &Path, pattern: &str) -> Result<(), StandardsMaterializeError> {
    let exclude_path = git_info_exclude_path(workspace)?;
    let content = std::fs::read_to_string(&exclude_path)
        .map_err(|e| StandardsMaterializeError::GitExcludeRead(format!("{exclude_path:?}: {e}")))?;
    // 逐行比对:pattern 在排除列表里就跳过(忽略行尾空白)。
    if content.lines().any(|line| line.trim() == pattern) {
        return Ok(());
    }
    // 追加 pattern。确保前有换行。
    let mut new_content = content;
    if !new_content.ends_with('\n') && !new_content.is_empty() {
        new_content.push('\n');
    }
    new_content.push_str(pattern);
    new_content.push('\n');
    std::fs::write(&exclude_path, &new_content).map_err(|e| {
        StandardsMaterializeError::GitExcludeWrite(format!("{exclude_path:?}: {e}"))
    })?;
    Ok(())
}

/// 用 `git rev-parse --git-path info/exclude` 找当前工作区的本地排除文件路径。
fn git_info_exclude_path(workspace: &Path) -> Result<PathBuf, StandardsMaterializeError> {
    let output = std::process::Command::new("git")
        .current_dir(workspace)
        .args(["rev-parse", "--git-path", "info/exclude"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| StandardsMaterializeError::GitExcludeRead(e.to_string()))?;
    if !output.status.success() {
        return Err(StandardsMaterializeError::GitExcludeRead(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let rel = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if rel.is_empty() {
        return Err(StandardsMaterializeError::GitExcludeRead(
            "git rev-parse returned empty path".into(),
        ));
    }
    // git rev-parse --git-path 可能返回相对或绝对路径。相对路径相对于 workspace。
    let p = PathBuf::from(&rel);
    Ok(if p.is_absolute() {
        p
    } else {
        workspace.join(p)
    })
}

/// 原子写:先写临时文件,再 rename。不让 Claude 读到半份规范(§4.3 #7)。
async fn write_atomic(target: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = target.with_extension("tmp");
    tokio::fs::write(&tmp, content).await?;
    if let Err(e) = tokio::fs::rename(&tmp, target).await {
        let _ = tokio::fs::remove_file(&tmp).await; // rename 失败清理 .tmp(§8)
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_git(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        assert!(Command::new("git")
            .args(["init"])
            .current_dir(dir)
            .status()
            .unwrap()
            .success());
    }

    #[tokio::test]
    async fn materialize_writes_marker_and_is_idempotent() {
        let root = std::env::temp_dir().join(format!(
            "bw-mat-ok-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        init_git(&root);
        let r1 = materialize_standards(&root).await.expect("first");
        assert!(!r1.unchanged && r1.written > 0);
        assert!(root.join(".claude/buddy/.bw-managed").exists());
        assert!(
            root.join(".claude/buddy/standards/metrics.md").exists(),
            "must be standards/metrics.md not standards/standards/"
        );
        assert!(root.join(".claude/buddy/standards/connectors.md").exists());
        assert!(!root
            .join(".claude/buddy/standards/standards/metrics.md")
            .exists());
        let r2 = materialize_standards(&root).await.expect("second");
        assert!(r2.unchanged && r2.written == 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn foreign_directory_without_marker_is_refused_not_deleted() {
        let root = std::env::temp_dir().join(format!(
            "bw-mat-foreign-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        ));
        init_git(&root);
        let buddy = root.join(".claude/buddy/standards");
        std::fs::create_dir_all(&buddy).unwrap();
        std::fs::write(buddy.join("user.md"), "mine").unwrap();
        let err = materialize_standards(&root)
            .await
            .expect_err("foreign must fail");
        assert!(matches!(err, StandardsMaterializeError::ForeignDirectory));
        assert!(buddy.join("user.md").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
