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

/// 一周的 git 读数。**每个字段旁边的注释就是真跑的那条命令** —— 界面上的
/// 每个数字都要能在终端一字不差地复算出来,这是纪律不是修辞。
///
/// 特别地:这里跑的是当前分支,**不带 `--all`**。带上 `--all` 会把
/// remote-tracking 分支和别的 worktree 的提交都算进「本周有没有真实提交」,
/// 一次 `git fetch` 就能把健康灯点亮 —— 那就成了可以造假的数字。
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

/// 提交**这次 buddy 自己写过的那几个文件**(铺底第 1 步、发版本这类动作)。
/// 没有改动就返回 `Ok(false)`,不造一个空提交。
///
/// 只 add 点名的路径,不用 `add -A`:用户点「规范铺底」的时候工作区多半是
/// 脏的,`add -A` 会把他手上没写完的改动一起打包提交,commit message 还写
/// 着「规范铺底」—— 那是在替他做他没同意的事。
pub async fn commit_paths(
    workspace: &Path,
    paths: &[String],
    message: &str,
) -> Result<CommitOutcome, GitError> {
    let mut out = CommitOutcome::default();
    if paths.is_empty() {
        return Ok(out);
    }
    for p in paths {
        // **单个路径 add 失败不能拖垮整次提交。** 最常见的原因是项目把这个路径
        // 写进了 `.gitignore`(buddy 自己的仓就忽略 `.claude/`)—— 那是项目的
        // 决定,不该用 `-f` 顶回去,如实记一笔就好。
        if git(workspace, &["add", "--", p]).await.is_err() {
            out.refused.push(p.clone());
        }
    }
    // 只看**暂存区**有没有东西:工作区别的地方脏不脏与这次提交无关。
    let staged = git(workspace, &["diff", "--cached", "--name-only"]).await?;
    if staged.trim().is_empty() {
        return Ok(out);
    }
    git(workspace, &["commit", "-m", message]).await?;
    out.committed = true;
    Ok(out)
}

/// 一次提交的结果。`refused` 是仓自己(多半经 `.gitignore`)拒收的路径 ——
/// 文件写下去了,但没进版本控制,这件事必须说出来,不能让人以为进仓了。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitOutcome {
    pub committed: bool,
    pub refused: Vec<String>,
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

/// 一个改动文件:`git status --porcelain` 的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    /// 两个字母的状态码,如 `" M"`、`"??"`、`"A "`。
    pub code: String,
    pub path: String,
}

impl ChangedFile {
    /// 给人看的一句话。**认不出的状态码原样回显**,不猜成「已修改」。
    pub fn label(&self) -> String {
        match self.code.trim() {
            "M" => "改过".into(),
            "A" => "新加".into(),
            "D" => "删了".into(),
            "R" => "改名".into(),
            "??" => "没跟踪".into(),
            other => other.to_string(),
        }
    }
}

/// 工作区里相对上次提交的改动(提没提交都算)。
///
/// 这不是 `diff_numstat` 能回答的问题——那个比较的是两个已经落库的提交,而这
/// 里要的正是「还没提交的东西长什么样」。
pub async fn changed_files(workspace: &Path) -> Result<Vec<ChangedFile>, GitError> {
    let out = git(workspace, &["status", "--porcelain"]).await?;
    Ok(out
        .lines()
        .filter(|l| l.len() > 3)
        .map(|l| ChangedFile {
            code: l[..2].to_string(),
            // 改名行是 `R  old -> new`,取箭头右边那个,人关心的是新名字。
            path: l[3..]
                .rsplit(" -> ")
                .next()
                .unwrap_or(&l[3..])
                .trim()
                .to_string(),
        })
        .collect())
}

/// 单个文件的 diff 正文。没跟踪的文件 `git diff` 是空的——这时退回整份文件
/// 内容并在头部标一行,而不是给人看一片空白。
pub async fn file_diff(workspace: &Path, rel: &str) -> Result<String, GitError> {
    let tracked = git(workspace, &["ls-files", "--error-unmatch", rel])
        .await
        .is_ok();
    if !tracked {
        let body = std::fs::read_to_string(workspace.join(rel)).unwrap_or_default();
        return Ok(format!("(这个文件还没进版本控制,下面是它的全文)\n\n{body}"));
    }
    let staged = git(workspace, &["diff", "--cached", "--", rel])
        .await
        .unwrap_or_default();
    let unstaged = git(workspace, &["diff", "--", rel])
        .await
        .unwrap_or_default();
    let both = format!("{staged}{unstaged}");
    if both.trim().is_empty() {
        return Ok("(相对上次提交没有改动)".into());
    }
    Ok(both)
}

/// 分支与主干的领先/落后。拿不到就是拿不到(没有上游、没有主干),返回
/// `None`,界面显示「—」,不显示 0。
pub async fn ahead_behind(workspace: &Path, base: &str) -> Option<(u32, u32)> {
    let out = git(
        workspace,
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("{base}...HEAD"),
        ],
    )
    .await
    .ok()?;
    let mut it = out.split_whitespace();
    let behind: u32 = it.next()?.parse().ok()?;
    let ahead: u32 = it.next()?.parse().ok()?;
    Some((ahead, behind))
}

/// 一层目录的内容。懒加载:点开哪个目录才读哪一层,不一次扫整棵树。
///
/// 三个目录永远不进结果:`.git`(不是项目内容)、`target` 与 `node_modules`
/// (构建产物,量大且没人在文件树里点它们)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// 相对仓根的路径。
    pub rel: String,
    pub name: String,
    pub is_dir: bool,
}

pub fn list_dir(workspace: &Path, rel: &str) -> Vec<TreeEntry> {
    const SKIP: &[&str] = &[".git", "target", "node_modules"];
    let dir = if rel.is_empty() {
        workspace.to_path_buf()
    } else {
        workspace.join(rel)
    };
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<TreeEntry> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if SKIP.contains(&name.as_str()) {
                return None;
            }
            let is_dir = e.file_type().ok()?.is_dir();
            let rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            Some(TreeEntry { rel, name, is_dir })
        })
        .collect();
    // 目录在前,同类按名字。文件树里最烦的是每次展开顺序都不一样。
    out.sort_by(|a, b| (!a.is_dir, &a.name).cmp(&(!b.is_dir, &b.name)));
    out
}
