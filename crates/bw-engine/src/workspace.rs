//! Workspace provisioner — the all-in-one-codebase default's mechanical arm:
//! every project gets exactly one real git repo, and this module mints it
//! (directory + `git init` + one real first commit). The only *writing*
//! subprocess module in the engine; everything it creates is immediately
//! verifiable on disk (`.git/`, `README.md`, `git log`), nothing is staged
//! for later or simulated.

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

async fn git_in(dir: &Path, args: &[&str]) -> Result<(), ProvisionError> {
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
    let out = tokio::process::Command::new("git")
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

/// Is this a workspace the workbench owns — i.e. it minted the repo and
/// authored a root commit? Bound, pre-existing repos are never owned, so the
/// workbench must not rewrite their files. False on any doubt (no `.git`, no
/// commits, or no root commit authored by the workbench).
pub async fn is_owned_workspace(dir: &Path) -> bool {
    if !dir.join(".git").exists() {
        return false;
    }
    let out = match tokio::process::Command::new("git")
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
    let output = tokio::process::Command::new("git")
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
