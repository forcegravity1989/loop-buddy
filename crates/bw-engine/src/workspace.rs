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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn provisions_a_real_repo_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("bw-provision-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        provision_git_workspace(&dir, "demo", "一个真实项目")
            .await
            .unwrap();
        assert!(dir.join(".git").exists(), "real git repo");
        assert!(dir.join("README.md").exists(), "real first file");
        let head = crate::evidence::head_commit(&dir.to_string_lossy())
            .await
            .unwrap();
        assert!(head.is_some(), "real first commit");

        // Second call: must not touch the existing history.
        std::fs::write(dir.join("later.md"), "后来的真实工作").unwrap();
        provision_git_workspace(&dir, "demo", "一个真实项目")
            .await
            .unwrap();
        assert!(
            dir.join("later.md").exists(),
            "re-provision must not clobber real work"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
