//! Real `git log` reader for the Version panel — shells out to the system
//! `git`, same "real subprocess, real output, no fabrication" idiom as
//! [`crate::claude_cli`]. Read-only: never writes to the repo, never invents
//! commits/PRs/issues the way a mocked "GitHub view" would.

use std::process::Stdio;

/// One real commit, exactly as `git log` reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub short_hash: String,
    pub author: String,
    /// Raw `--date=iso-strict` string, e.g. `2026-07-09T03:15:42+00:00`.
    pub date: String,
    pub subject: String,
}

#[derive(Debug, thiserror::Error)]
pub enum GitLogError {
    #[error("工作目录未配置")]
    NotConfigured,
    #[error("无法运行 git:{0}")]
    Spawn(String),
    #[error("{0}")]
    GitFailed(String),
    #[error("无法解析 git 输出")]
    Parse,
}

// Unit/record separators — real commit subjects can contain almost any
// character but not these two control codes, so they're safe delimiters
// without needing to shell-escape or use a slower per-commit invocation.
const FIELD_SEP: char = '\u{1f}';
const RECORD_SEP: char = '\u{1e}';

/// Read up to `limit` real commits from `workspace_path` via `git log`.
/// Empty `workspace_path` short-circuits to `NotConfigured` without
/// spawning anything; a non-git directory or missing `git` binary surface
/// git's own real error text, never a fabricated status.
pub async fn read_commits(
    workspace_path: &str,
    limit: usize,
) -> Result<Vec<GitCommit>, GitLogError> {
    if workspace_path.trim().is_empty() {
        return Err(GitLogError::NotConfigured);
    }

    let format = format!("%H{FIELD_SEP}%h{FIELD_SEP}%an{FIELD_SEP}%ad{FIELD_SEP}%s{RECORD_SEP}");
    let output = tokio::process::Command::new("git")
        .current_dir(workspace_path)
        .arg("log")
        .arg(format!("--max-count={limit}"))
        .arg("--date=iso-strict")
        .arg(format!("--pretty=format:{format}"))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| GitLogError::Spawn(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(GitLogError::GitFailed(if stderr.is_empty() {
            format!("git log exited with {}", output.status)
        } else {
            stderr
        }));
    }

    parse_commits(&String::from_utf8_lossy(&output.stdout))
}

fn parse_commits(text: &str) -> Result<Vec<GitCommit>, GitLogError> {
    let mut commits = Vec::new();
    for record in text.split(RECORD_SEP) {
        let record = record.trim_matches('\n');
        if record.is_empty() {
            continue;
        }
        let mut parts = record.split(FIELD_SEP);
        let (Some(hash), Some(short_hash), Some(author), Some(date), Some(subject)) = (
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
            parts.next(),
        ) else {
            return Err(GitLogError::Parse);
        };
        commits.push(GitCommit {
            hash: hash.to_string(),
            short_hash: short_hash.to_string(),
            author: author.to_string(),
            date: date.to_string(),
            subject: subject.to_string(),
        });
    }
    Ok(commits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_workspace_short_circuits_without_spawning() {
        let err = read_commits("", 10).await.unwrap_err();
        assert!(matches!(err, GitLogError::NotConfigured));
    }

    #[test]
    fn parses_a_realistic_multi_record_log() {
        let text = format!(
            "abc123{FIELD_SEP}abc12{FIELD_SEP}Builder{FIELD_SEP}2026-07-09T03:15:42+00:00{FIELD_SEP}first commit{RECORD_SEP}\ndef456{FIELD_SEP}def45{FIELD_SEP}Agent{FIELD_SEP}2026-07-08T10:00:00+00:00{FIELD_SEP}second{RECORD_SEP}"
        );
        let commits = parse_commits(&text).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].short_hash, "abc12");
        assert_eq!(commits[0].subject, "first commit");
        assert_eq!(commits[1].author, "Agent");
    }

    #[test]
    fn empty_output_is_zero_commits_not_an_error() {
        let commits = parse_commits("").unwrap();
        assert!(commits.is_empty());
    }
}
