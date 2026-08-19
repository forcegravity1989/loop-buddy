//! 现算:从 git 里当场取数,一个中间值都不存。
//!
//! 「这周有没有真实提交」「上周合入了几次」「有哪些标签」——V3 是把这些写进
//! `observation`/`workflow_run` 表再读回来,V4 直接问 git。代价是打开项目要
//! 现算(buddy 自己的仓几百个提交是几十毫秒级),换来的是**造不了假**:界面上
//! 每个数字都能用同一条 git 命令在终端里复算出来。

use crate::isoweek;
use std::path::Path;
use std::process::Stdio;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("工作区未配置")]
    NotConfigured,
    #[error("无法运行 git:{0}")]
    Spawn(String),
    #[error("git 失败:{0}")]
    Failed(String),
    #[error("认不出的 ISO 周:{0}")]
    BadWeek(String),
}

/// 一周的 git 读数。每个字段旁边的注释就是复算它的命令。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeekStats {
    pub week: String,
    /// `git log --since=<周一> --until=<下周一> --oneline | wc -l`
    pub commits: u32,
    /// `git log --merges --since=… --until=… --oneline | wc -l`
    pub merges: u32,
    /// `git log --numstat` 按目录聚合后的前三名。
    pub top_dirs: Vec<String>,
}

async fn git(workspace: &Path, args: &[&str]) -> Result<String, GitError> {
    if workspace.as_os_str().is_empty() {
        return Err(GitError::NotConfigured);
    }
    let out = bw_engine::tokio_cmd("git")
        .arg("-C")
        .arg(workspace)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| GitError::Spawn(e.to_string()))?;
    if !out.status.success() {
        return Err(GitError::Failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn week_range(week: &str) -> Result<(String, String), GitError> {
    let (start, end) = isoweek::week_bounds(week).ok_or_else(|| GitError::BadWeek(week.into()))?;
    Ok((start.to_string(), end.to_string()))
}

/// 这个目录是不是一个 git 仓。不是就返回 false,不报错——很多判据在没有仓的
/// 项目上本来就该是「没数据」。
pub async fn is_repo(workspace: &Path) -> bool {
    git(workspace, &["rev-parse", "--git-dir"]).await.is_ok()
}

/// 某一周有没有真实提交。健康判据 (a) 的后半句。
pub async fn has_commits_in_week(workspace: &Path, week: &str) -> Result<bool, GitError> {
    Ok(week_stats(workspace, week).await?.commits > 0)
}

/// 某一周有没有合入。健康判据 (c) 的前半句。
pub async fn has_merges_in_week(workspace: &Path, week: &str) -> Result<bool, GitError> {
    Ok(week_stats(workspace, week).await?.merges > 0)
}

pub async fn week_stats(workspace: &Path, week: &str) -> Result<WeekStats, GitError> {
    let (since, until) = week_range(week)?;
    let commits = git(
        workspace,
        &[
            "log",
            "--all",
            &format!("--since={since}"),
            &format!("--until={until}"),
            "--pretty=format:%H",
        ],
    )
    .await?;
    let merges = git(
        workspace,
        &[
            "log",
            "--all",
            "--merges",
            &format!("--since={since}"),
            &format!("--until={until}"),
            "--pretty=format:%H",
        ],
    )
    .await?;
    let numstat = git(
        workspace,
        &[
            "log",
            "--all",
            &format!("--since={since}"),
            &format!("--until={until}"),
            "--numstat",
            "--pretty=format:",
        ],
    )
    .await
    .unwrap_or_default();
    Ok(WeekStats {
        week: week.to_string(),
        commits: nonempty_lines(&commits),
        merges: nonempty_lines(&merges),
        top_dirs: top_dirs(&numstat, 3),
    })
}

/// 仓里一共多少条提交。铺底探测「这是不是个有历史的仓」用它。
pub async fn commit_count(workspace: &Path) -> Result<u32, GitError> {
    let out = git(workspace, &["rev-list", "--all", "--count"]).await?;
    Ok(out.trim().parse().unwrap_or(0))
}

/// 仓里的标签(按建立时间)。没有就是空,不编。
pub async fn tags(workspace: &Path) -> Result<Vec<String>, GitError> {
    let out = git(workspace, &["tag", "-l", "--sort=creatordate"]).await?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// 当前分支名。
pub async fn current_branch(workspace: &Path) -> Result<String, GitError> {
    Ok(git(workspace, &["rev-parse", "--abbrev-ref", "HEAD"])
        .await?
        .trim()
        .to_string())
}

/// 有没有未提交的改动。
pub async fn is_dirty(workspace: &Path) -> Result<bool, GitError> {
    Ok(!git(workspace, &["status", "--porcelain"])
        .await?
        .trim()
        .is_empty())
}

/// 把仓里的改动提交上去(铺底第 1 步、发版本这类 buddy 自己写仓的动作)。
/// 没有改动就返回 `Ok(false)`,不造一个空提交。
pub async fn commit_all(workspace: &Path, message: &str) -> Result<bool, GitError> {
    git(workspace, &["add", "-A"]).await?;
    if !is_dirty(workspace).await? {
        return Ok(false);
    }
    git(workspace, &["commit", "-m", message]).await?;
    Ok(true)
}

/// 根提交的作者 —— 判「这个仓是不是 buddy 自己建的空仓」。
pub async fn root_commit_author(workspace: &Path) -> Result<String, GitError> {
    let roots = git(workspace, &["rev-list", "--max-parents=0", "HEAD"]).await?;
    let Some(first) = roots.lines().last().map(str::trim) else {
        return Ok(String::new());
    };
    Ok(git(workspace, &["log", "-1", "--pretty=format:%an", first])
        .await?
        .trim()
        .to_string())
}

fn nonempty_lines(s: &str) -> u32 {
    s.lines().filter(|l| !l.trim().is_empty()).count() as u32
}

fn top_dirs(numstat: &str, n: usize) -> Vec<String> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for line in numstat.lines() {
        let Some(path) = line.split('\t').nth(2) else {
            continue;
        };
        let dir = match path.rsplit_once('/') {
            Some((d, _)) => d.to_string(),
            None => ".".to_string(),
        };
        *counts.entry(dir).or_default() += 1;
    }
    let mut v: Vec<(String, u32)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|(d, _)| d).collect()
}
