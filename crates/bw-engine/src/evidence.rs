//! Workspace evidence collector — turns a project workspace's *real* state
//! (git history, tracked files, dirty status) into numbers the metric derive
//! chain can eat. Same idiom as [`crate::git_log`]: read-only subprocesses,
//! real output, no fabrication. This is the first non-Manual `MetricSource`
//! producer in the codebase — the minimal down payment on Tier D ("Connector
//! 真喂指标"), scoped to what a local workspace can honestly answer.
//!
//! The collector never *interprets* — it reports counts and lists; deciding
//! which metric an item feeds (and recording the observation) is the
//! caller's job, so the append-only observation trail stays the single
//! source of truth.

use std::process::Stdio;

/// A real, point-in-time reading of a workspace. Every field comes from a
/// command that actually ran; nothing is estimated.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceEvidence {
    /// `git rev-list --count HEAD` — total commits. `0` on a repo with no
    /// commits yet (git errors there; treated as honest zero).
    pub commit_count: u32,
    /// `git ls-files` line count — tracked files.
    pub tracked_files: u32,
    /// `git status --porcelain` line count — uncommitted paths (an honesty
    /// signal: a "committed" stage claim with a dirty tree is suspect).
    pub dirty_paths: u32,
    /// Subjects of the newest commits (up to the collector's cap), newest
    /// first — real provenance quotes for reports.
    pub recent_subjects: Vec<String>,
    /// Tracked markdown docs under `docs/` — the playbook phases write their
    /// evidence there, so this counts real playbook artifacts.
    pub docs_files: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum EvidenceError {
    #[error("工作目录未配置")]
    NotConfigured,
    #[error("无法运行 git:{0}")]
    Spawn(String),
}

async fn git_stdout(workspace: &str, args: &[&str]) -> Result<Option<String>, EvidenceError> {
    let output = tokio::process::Command::new("git")
        .current_dir(workspace)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| EvidenceError::Spawn(e.to_string()))?;
    if !output.status.success() {
        // A failed subcommand (e.g. `rev-list` on a commitless repo) is a
        // real "nothing there yet", not a collector crash.
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
}

fn count_lines(s: &str) -> u32 {
    s.lines().filter(|l| !l.trim().is_empty()).count() as u32
}

/// Collect real evidence from `workspace`. Read-only; never mutates the repo.
pub async fn collect(workspace: &str) -> Result<WorkspaceEvidence, EvidenceError> {
    if workspace.trim().is_empty() {
        return Err(EvidenceError::NotConfigured);
    }

    let commit_count = git_stdout(workspace, &["rev-list", "--count", "HEAD"])
        .await?
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);

    let ls_files = git_stdout(workspace, &["ls-files"])
        .await?
        .unwrap_or_default();
    let tracked_files = count_lines(&ls_files);
    let docs_files = ls_files
        .lines()
        .filter(|l| {
            let l = l.trim();
            l.starts_with("docs/") && l.ends_with(".md")
        })
        .count() as u32;

    let dirty_paths = git_stdout(workspace, &["status", "--porcelain"])
        .await?
        .map(|s| count_lines(&s))
        .unwrap_or(0);

    let recent_subjects = git_stdout(workspace, &["log", "--max-count=10", "--pretty=format:%s"])
        .await?
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();

    Ok(WorkspaceEvidence {
        commit_count,
        tracked_files,
        dirty_paths,
        recent_subjects,
        docs_files,
    })
}

/// One tracked file as really found in the workspace — the raw material an
/// artifact registration is made of. `bytes` is a real `stat` at scan time
/// (`0` if the file vanished between `git ls-files` and the stat — rare but
/// honest).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceFile {
    /// Workspace-relative path, exactly as git reports it.
    pub path: String,
    pub bytes: u64,
}

/// The workspace's current short HEAD hash — the "which version of the
/// codebase was this seen at" stamp for artifact registration. `None` on a
/// repo with no commits yet (an honest "no version to pin to", not an error).
pub async fn head_commit(workspace: &str) -> Result<Option<String>, EvidenceError> {
    if workspace.trim().is_empty() {
        return Err(EvidenceError::NotConfigured);
    }
    Ok(git_stdout(workspace, &["rev-parse", "--short", "HEAD"])
        .await?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty()))
}

/// Every tracked file in the workspace with its real on-disk size. Read-only:
/// one `git ls-files` + one `stat` per file, no interpretation — classifying
/// and persisting is the caller's job.
pub async fn list_workspace_files(workspace: &str) -> Result<Vec<WorkspaceFile>, EvidenceError> {
    if workspace.trim().is_empty() {
        return Err(EvidenceError::NotConfigured);
    }
    let ls = git_stdout(workspace, &["ls-files"])
        .await?
        .unwrap_or_default();
    let root = std::path::Path::new(workspace);
    let mut files = Vec::new();
    for line in ls.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        // Plain sync stat: workspaces here are small (tens of files) and the
        // engine's tokio feature set has no `fs` — not worth adding one.
        let bytes = std::fs::metadata(root.join(path))
            .map(|m| m.len())
            .unwrap_or(0);
        files.push(WorkspaceFile {
            path: path.to_string(),
            bytes,
        });
    }
    Ok(files)
}
