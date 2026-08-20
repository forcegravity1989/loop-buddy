//! 现算:从 git 里当场取数,一个中间值都不存。
//!
//! 「这周有没有真实提交」「上周合入了几次」「有哪些标签」——V3 是把这些写进
//! `observation`/`workflow_run` 表再读回来,V4 直接问 git。代价是打开项目要
//! 现算(buddy 自己的仓几百个提交是几十毫秒级),换来的是**造不了假**:界面上
//! 每个数字都能用同一条 git 命令在终端里复算出来。

use crate::isoweek;
use std::path::{Path, PathBuf};
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

/// 这个仓有没有第一条提交。**刚 `git init` 或者刚建出来的空仓没有 HEAD**,
/// 从它身上开分支会被 git 顶回来(`invalid reference`),所以要先问这一句。
pub async fn has_head(workspace: &Path) -> bool {
    git(workspace, &["rev-parse", "--verify", "HEAD"])
        .await
        .is_ok()
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
    // 一条提交都没有的仓,`rev-parse HEAD` 会失败 —— 但分支名是有的
    // (`git init` 就定好了,main 还是 master),`symbolic-ref` 读得出来。
    // 读不出名字就报「没有分支」,不要在这种时候编一个。
    match git(workspace, &["rev-parse", "--abbrev-ref", "HEAD"]).await {
        Ok(s) => Ok(s.trim().to_string()),
        Err(e) => match git(workspace, &["symbolic-ref", "--short", "HEAD"]).await {
            Ok(s) => Ok(s.trim().to_string()),
            Err(_) => Err(e),
        },
    }
}

/// 把分支推到 `origin` 并建立跟踪(`git push -u origin <branch>`)。
///
/// 推不上去就如实报错。**不吞**:推失败而后面照样去开 MR,开出来的会是一个
/// 指向不存在分支的 MR,或者干脆报一句和真正原因无关的错。
pub async fn push_branch(workspace: &Path, branch: &str) -> Result<(), GitError> {
    git(workspace, &["push", "-u", "origin", branch]).await?;
    Ok(())
}

/// 收掉一棵 worktree(`git worktree remove` + `prune`)。
///
/// **不带 `--force`**:带上它会连未提交的改动一起删。调用方
/// ([`crate::app`] 的结清收尾)已经先确认过这棵树是干净的,真在这一步失败就
/// 说明情况和刚才读到的不一样了 —— 那就该失败,不该硬删。
pub async fn worktree_remove(main_workspace: &Path, tree: &str) -> Result<(), GitError> {
    git(main_workspace, &["worktree", "remove", tree]).await?;
    let _ = git(main_workspace, &["worktree", "prune"]).await;
    Ok(())
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

/// 把这棵树里的**全部**改动提交掉,返回有没有真的产生一个提交。
///
/// **只准对「一张活自己的 worktree」用**。那棵树整棵都属于这一张活,里面每一个
/// 改动都是这次干活的产物,所以 `git add -A` 是对的。规范铺底那边不能用它 ——
/// 那边写的是**人的主检出**,`add -A` 会把人手上没写完的改动一起打包进去。
pub async fn commit_all(workspace: &Path, message: &str) -> Result<bool, GitError> {
    git(workspace, &["add", "-A"]).await?;
    let staged = git(workspace, &["diff", "--cached", "--name-only"]).await?;
    if staged.trim().is_empty() {
        return Ok(false);
    }
    // `--cleanup=whitespace` 不能省:提交信息第一行是 `#<活号> <标题>`,而人要是
    // 把 `commit.cleanup` 设成了 `strip`/`scissors`,`#` 开头的行会被当注释整行
    // 删掉 —— 信息被删空,commit 直接失败。显式定死就跟本机配置无关了。
    git(
        workspace,
        &["commit", "--cleanup=whitespace", "-m", message],
    )
    .await?;
    Ok(true)
}

/// 当前 HEAD 的完整 sha。
pub async fn head_sha(workspace: &Path) -> Result<String, GitError> {
    Ok(git(workspace, &["rev-parse", "HEAD"])
        .await?
        .trim()
        .to_string())
}

/// 这棵树上有几个提交是 `base` 上没有的 —— 「这张活到底干出东西没有」的判据。
///
/// `base` 传 sha 而不是分支名:分支名要在**这棵树里**解析,而活的 worktree 上
/// `main` 指向哪里取决于它是什么时候开出来的;调用方手里已经有主检出当下的
/// sha,直接传过来,这个数就没有歧义。
pub async fn commits_ahead_of(workspace: &Path, base: &str) -> Result<u32, GitError> {
    let out = git(
        workspace,
        &["rev-list", "--count", &format!("{base}..HEAD")],
    )
    .await?;
    Ok(out.trim().parse().unwrap_or(0))
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
    // **必须用 `-z`**。不带它的时候,git 会把带空格或非 ASCII 的路径用引号包起
    // 来、还把中文转成八进制转义(`"\344\270\255…"`)。那串东西直接拿去当
    // pathspec 查 diff 是查不到的 —— 界面会对一个明明改过的文件说「它还没进版
    // 本控制」,然后给人看一片空白。`-z` 用 NUL 分隔,一个字节都不转义。
    let out = git(workspace, &["status", "--porcelain", "-z"]).await?;
    let mut fields = out.split('\0').filter(|f| !f.is_empty());
    let mut rows = Vec::new();
    while let Some(entry) = fields.next() {
        if entry.len() < 4 {
            continue;
        }
        let code = entry[..2].to_string();
        let path = entry[3..].to_string();
        // 改名/复制的记录后面**紧跟一个额外字段**装原路径。不把它读掉,下一轮
        // 就会把原路径当成一条独立的改动行。人关心的是新名字,原路径丢掉。
        if code.starts_with('R') || code.starts_with('C') {
            let _ = fields.next();
        }
        rows.push(ChangedFile { code, path });
    }
    Ok(rows)
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

/// 仓里最早那条提交的日期(`YYYY-MM-DD`)。空仓返回 `None`。
///
/// 回填的起点靠它 —— 从这一天所在的那一周开始往今天扫。
pub async fn first_commit_date(workspace: &Path) -> Option<String> {
    let out = git(
        workspace,
        &["log", "--reverse", "--date=short", "--pretty=format:%ad"],
    )
    .await
    .ok()?;
    out.lines().next().map(|s| s.trim().to_string())
}

/// 标签 + 它指向那条提交的日期。**取的是提交日期,不是打标签的日期**:标签
/// 可以事后补打,提交日期才是那个版本真正发生的时刻。
pub async fn tags_with_dates(workspace: &Path) -> Vec<(String, String)> {
    let Ok(out) = git(
        workspace,
        &[
            "for-each-ref",
            "--sort=creatordate",
            "--format=%(refname:short)\t%(committerdate:short)",
            "refs/tags",
        ],
    )
    .await
    else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|l| {
            let (tag, date) = l.split_once('\t')?;
            Some((tag.trim().to_string(), date.trim().to_string()))
        })
        .collect()
}

/// 一条产物登记。**没有登记表**——`git log --name-only` 就是登记表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Artifact {
    pub path: String,
    /// 最近一次碰它的提交(短 hash)。
    pub commit: String,
    pub subject: String,
    /// 提交消息里解析得到的活号。解析不到就是 `None` —— **不强凑**。
    pub issue_number: Option<u32>,
}

/// 现扫产物登记:按提交从新到旧走 `--name-only`,每个文件只记最近碰它的那一次。
///
/// `max_commits` 是往回看多少个提交;老仓全量扫一遍既慢又没意义(几年前动过
/// 的文件不是「本项目的产物」这个问题的答案)。
pub async fn artifacts(workspace: &Path, max_commits: usize) -> Result<Vec<Artifact>, GitError> {
    // 记录分隔用 \x1e(记录分隔符),文件名里不可能出现它;比按空行切稳。
    let out = git(
        workspace,
        &[
            "log",
            &format!("-n{max_commits}"),
            "--name-only",
            "--no-merges",
            "--pretty=format:\x1e%h\x1f%s",
        ],
    )
    .await?;

    let mut seen: Vec<Artifact> = Vec::new();
    for chunk in out.split('\x1e') {
        let chunk = chunk.trim_start_matches('\n');
        if chunk.is_empty() {
            continue;
        }
        let mut lines = chunk.lines();
        let Some(head) = lines.next() else { continue };
        let (commit, subject) = head.split_once('\x1f').unwrap_or((head, ""));
        let issue_number = parse_issue_number(subject);
        for path in lines.filter(|l| !l.trim().is_empty()) {
            if seen.iter().any(|a| a.path == path) {
                continue;
            }
            seen.push(Artifact {
                path: path.to_string(),
                commit: commit.to_string(),
                subject: subject.to_string(),
                issue_number,
            });
        }
    }
    Ok(seen)
}

/// 从提交消息里找 `#<号>`。找不到就 `None`,**不猜**。
fn parse_issue_number(subject: &str) -> Option<u32> {
    let rest = subject.split_once('#')?.1;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// 这个仓在浏览器里的根地址,比如 `https://github.com/owner/repo`。
///
/// **从 `origin` 的地址推**,不从库里的 `remote_host` 拼 —— codehub 的 host 存的
/// 是区的别名(`green`/`open`/`yellow`),不是域名,拼不出能点的地址;而 `origin`
/// 里躺着的是真域名。推不出来就返回 `None`,调用方**不给链接**,不编一个。
///
/// **不起子进程,直接读 `.git/config`** —— 这个值每重拼一次界面数据就要一次,
/// 起一次 `git remote get-url` 是白花的几十毫秒。worktree 里的 `.git` 是个文件,
/// 指向主仓那份 config,跟过去读。
pub fn browse_base(workspace: &Path) -> Option<String> {
    let dot_git = workspace.join(".git");
    let config = if dot_git.is_dir() {
        dot_git.join("config")
    } else {
        // `gitdir: /主仓/.git/worktrees/<名>` —— config 在 `/主仓/.git/config`。
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let gitdir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
        gitdir.parent()?.parent()?.join("config")
    };
    let text = std::fs::read_to_string(config).ok()?;
    let mut in_origin = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_origin = l.replace(' ', "") == "[remote\"origin\"]";
            continue;
        }
        if in_origin {
            if let Some(url) = l.strip_prefix("url") {
                if let Some(url) = url.trim_start().strip_prefix('=') {
                    return normalize_browse(url.trim());
                }
            }
        }
    }
    None
}

/// `git@host:路径.git` / `https://host/路径[.git]` / `ssh://git@host/路径` →
/// `https://host/路径`。认不出的写法返回 `None`。
fn normalize_browse(url: &str) -> Option<String> {
    if url.is_empty() {
        return None;
    }
    let rest = if let Some(r) = url.strip_prefix("ssh://git@") {
        r.to_string()
    } else if let Some(scp) = url.strip_prefix("git@") {
        // `git@github.com:owner/repo.git` —— 冒号是 host 与路径的分界。
        scp.replacen(':', "/", 1)
    } else {
        let r = url
            .strip_prefix("https://")
            .or(url.strip_prefix("http://"))?;
        // `https://user@host/路径` 里那段凭据不该出现在给人点的链接里。
        r.split_once('@').map(|(_, r)| r).unwrap_or(r).to_string()
    };
    let rest = rest.trim_end_matches('/').trim_end_matches(".git");
    if !rest.contains('/') {
        return None;
    }
    Some(format!("https://{rest}"))
}

/// 这个路径被 git 跟踪着没有。
///
/// 用 `ls-files`(跟踪着就打印路径,没跟踪就打印空)而不是 `--error-unmatch`:
/// 后者「没跟踪」和「git 根本没跑起来」都是非零退出,分不开。**分不开的时候
/// 返回 `Some(true)`** —— 这个判断的下游是删文件,拿不准就别删。
pub async fn is_tracked(workspace: &Path, rel: &str) -> bool {
    match git(workspace, &["ls-files", "--", rel]).await {
        Ok(out) => !out.trim().is_empty(),
        Err(_) => true,
    }
}
