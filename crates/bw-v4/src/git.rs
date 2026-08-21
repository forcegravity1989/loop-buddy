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

/// 一周的 git 读数。**复算命令写在 [`week_counts_many`] 的文档里** —— 界面上
/// 的每个数字都要能在终端复算出来,这是纪律不是修辞。注意复算**不要**用
/// `git log --since/--until`:那正是 2026-08-21 修掉的错误窗口(多算一天、
/// 还会漏算),用它复算出来的数和界面对不上是命令的错,不是界面的错。
///
/// 特别地:这里跑的是当前分支,**不带 `--all`**。带上 `--all` 会把
/// remote-tracking 分支和别的 worktree 的提交都算进「本周有没有真实提交」,
/// 一次 `git fetch` 就能把健康灯点亮 —— 那就成了可以造假的数字。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WeekStats {
    pub week: String,
    /// 提交时间落在 `[周一 00:00, 下周一 00:00)`(本机时区)的提交数。
    pub commits: u32,
    /// 同窗口内父提交 ≥ 2 的提交数(合入)。
    pub merges: u32,
    /// `git log --numstat` 按目录聚合后的前三名。
    pub top_dirs: Vec<String>,
}

/// 一周的轻量计数:提交数、合入数。健康判据和走势图只要这两个数 ——
/// **不要为它们付 numstat 的钱**(那是 [`week_stats_many`] 的事,贵一个量级)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WeekCounts {
    pub commits: u32,
    pub merges: u32,
}

async fn git(workspace: &Path, args: &[&str]) -> Result<String, GitError> {
    if workspace.as_os_str().is_empty() {
        return Err(GitError::NotConfigured);
    }
    let out = v4_engine::tokio_cmd("git")
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

/// 一周的窗口,换算成本机时区的 unix 秒:`[周一 00:00, 下周一 00:00)`。
pub fn week_window(week: &str) -> Result<(i64, i64), GitError> {
    let (start, end) = isoweek::week_bounds(week).ok_or_else(|| GitError::BadWeek(week.into()))?;
    let at = |d: time::Date| {
        d.with_hms(0, 0, 0)
            .expect("00:00:00 一定合法")
            .assume_offset(isoweek::local_offset())
            .unix_timestamp()
    };
    Ok((at(start), at(end)))
}

/// 多个周的提交/合入计数,**一趟遍历全部出齐**。
///
/// **不用 `git log --since/--until` 截窗口**,而是把提交时间全取回来自己按
/// 时间戳过滤。两个理由都是 2026-08-21 实测撞出来的,不是洁癖:
///
/// 1. **`--until=<下周一>` 会把下周一那一整天算进来。** git 用 approxidate 解析
///    这类不带时刻的日期,实测 `--until=2026-08-03` 连当天 11:26 的提交都收 ——
///    于是每一周都多算了下一周第一天,健康判据「本周有没有提交」和走势图一起偏。
/// 2. **`--since` 会提前停止遍历。** 提交时间不严格单调(rebase、cherry-pick、
///    机器时钟)时 git 会漏掉一批,同一条命令换个起点、隔几分钟跑,结果能从
///    78 变成 83。走势图上的数**必须**是每次算都一样的。
///
/// 全量遍历一次的代价:这个仓 751 条提交,`--pretty=format:'%ct %P'` 实测约
/// 25 毫秒 —— **要几周都是这一趟**,健康判两周、走势看八周、回填补一百周,
/// 都不再把遍历次数乘上去(第一版就是这么乘出 2 秒卡顿和 75 秒冻屏的,评审
/// 抓的)。
///
/// 复算(`<since>`/`<until>` 用 [`week_window`] 的 unix 秒;合入 = 父提交 ≥ 2,
/// 即行内字段 ≥ 3 个):
///
/// ```bash
/// git -C <仓> log --pretty=format:'%ct %P' | awk -v s=<since> -v u=<until> '$1>=s && $1<u' | wc -l
/// git -C <仓> log --pretty=format:'%ct %P' | awk -v s=<since> -v u=<until> '$1>=s && $1<u && NF>=3' | wc -l
/// ```
pub async fn week_counts_many(
    workspace: &Path,
    weeks: &[String],
) -> Result<std::collections::HashMap<String, WeekCounts>, GitError> {
    let mut windows = Vec::with_capacity(weeks.len());
    for w in weeks {
        windows.push((w.clone(), week_window(w)?));
    }
    // 每行:`<unix 秒> <父提交…>`。父提交 ≥ 2 = 合入。
    let out = git(workspace, &["log", "--pretty=format:%ct %P"]).await?;
    let mut map: std::collections::HashMap<String, WeekCounts> = weeks
        .iter()
        .map(|w| (w.clone(), WeekCounts::default()))
        .collect();
    for line in out.lines() {
        let mut fields = line.split_whitespace();
        let Some(ts) = fields.next().and_then(|t| t.parse::<i64>().ok()) else {
            continue;
        };
        let parents = fields.count();
        for (w, (since, until)) in &windows {
            if ts >= *since && ts < *until {
                let c = map.get_mut(w).expect("map 由同一份 weeks 建出");
                c.commits += 1;
                if parents >= 2 {
                    c.merges += 1;
                }
                break;
            }
        }
    }
    Ok(map)
}

/// 多个周的完整读数(计数 + 目录榜),**一趟 numstat 遍历全部出齐**。
///
/// 和 [`week_counts_many`] 的分工:那边只要计数、便宜(轻一个量级);这边要
/// 目录榜,得付一次全量 `--numstat` 的钱(本仓实测约 0.7 秒、174KB 输出)——
/// 所以**只给真要目录榜的调用方用**(回填、周计划的「上周完成情况」),健康
/// 判据和走势图别走这条。
pub async fn week_stats_many(
    workspace: &Path,
    weeks: &[String],
) -> Result<std::collections::HashMap<String, WeekStats>, GitError> {
    let mut windows = Vec::with_capacity(weeks.len());
    for w in weeks {
        windows.push((w.clone(), week_window(w)?));
    }
    // `\x01` 是提交行的标记(git 自己在 pretty 里也这么用;numstat 对含控制
    // 字符的路径会 C-quote,所以路径伪造不出这个前缀)。每条提交行:
    // `\x01<unix 秒> <父提交…>`,后面跟着它的 numstat 行。
    let out = git(
        workspace,
        &["log", "--numstat", "--pretty=format:\x01%ct %P"],
    )
    .await?;

    let mut counts: std::collections::HashMap<String, WeekCounts> =
        std::collections::HashMap::new();
    let mut dirs: std::collections::HashMap<String, std::collections::HashMap<String, u32>> =
        std::collections::HashMap::new();
    // 当前提交落在哪个周的桶里;不在任何桶就丢。
    let mut bucket: Option<String> = None;
    for line in out.lines() {
        if let Some(head) = line.strip_prefix('\x01') {
            bucket = None;
            let mut fields = head.split_whitespace();
            let Some(ts) = fields.next().and_then(|t| t.parse::<i64>().ok()) else {
                continue;
            };
            let parents = fields.count();
            for (w, (since, until)) in &windows {
                if ts >= *since && ts < *until {
                    let c = counts.entry(w.clone()).or_default();
                    c.commits += 1;
                    if parents >= 2 {
                        c.merges += 1;
                    }
                    bucket = Some(w.clone());
                    break;
                }
            }
        } else if let Some(week) = &bucket {
            let Some(path) = line.split('\t').nth(2) else {
                continue;
            };
            let dir = match numstat_path(path).rsplit_once('/') {
                Some((d, _)) => d.to_string(),
                None => ".".to_string(),
            };
            *dirs
                .entry(week.clone())
                .or_default()
                .entry(dir)
                .or_default() += 1;
        }
    }

    let mut map = std::collections::HashMap::new();
    for w in weeks {
        let c = counts.get(w).copied().unwrap_or_default();
        let top = dirs
            .remove(w)
            .map(|m| {
                let mut v: Vec<(String, u32)> = m.into_iter().collect();
                v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                v.into_iter().take(3).map(|(d, _)| d).collect()
            })
            .unwrap_or_default();
        map.insert(
            w.clone(),
            WeekStats {
                week: w.clone(),
                commits: c.commits,
                merges: c.merges,
                top_dirs: top,
            },
        );
    }
    Ok(map)
}

/// 这一周的提交、合入、动得最多的目录。单周场景的便捷入口;要多周就用
/// [`week_stats_many`],别在循环里逐周调这个。
pub async fn week_stats(workspace: &Path, week: &str) -> Result<WeekStats, GitError> {
    let weeks = vec![week.to_string()];
    let mut map = week_stats_many(workspace, &weeks).await?;
    Ok(map.remove(week).unwrap_or_default())
}

/// numstat 的路径列对**改名**输出的是紧凑合并形式(`a/{old => new}/f.rs` 或
/// `old.rs => new.rs`),直接当路径用会切出根本不存在的目录(评审抓的:本仓
/// 那批目录搬迁提交就能复现)。这里取**新路径**那一侧 —— 改动落点在新位置。
fn numstat_path(raw: &str) -> String {
    if let (Some(l), Some(r)) = (raw.find('{'), raw.find('}')) {
        if l < r {
            if let Some((_, new)) = raw[l + 1..r].split_once(" => ") {
                let joined = format!("{}{}{}", &raw[..l], new, &raw[r + 1..]);
                // `{old => }` 两侧的斜杠会撞在一起,收掉。
                return joined.replace("//", "/");
            }
        }
    }
    if let Some((_, new)) = raw.split_once(" => ") {
        return new.to_string();
    }
    raw.to_string()
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

/// 把这个检出快进到远端最新(`git fetch origin <当前分支>` +
/// `git merge --ff-only FETCH_HEAD`),返回 HEAD 有没有真的往前挪。
///
/// **只快进,绝不制造 merge 提交**。这里改的是人自己的检出,岔开了就该停下来
/// 报错让人自己处理,不能替他合。工作区脏、没挂远端、没网、本机主干和远端岔
/// 开——每一种都如实报错,由调用方原话端到界面上,不吞。
pub async fn pull_ff(workspace: &Path) -> Result<bool, GitError> {
    let before = head_sha(workspace).await?;
    let branch = current_branch(workspace).await?;
    // `git fetch origin <branch>` 一定会写 `FETCH_HEAD`;`origin/<branch>` 这个
    // 远端跟踪引用要不要跟着更新,取决于这个仓的 refspec 配置。所以下一步认
    // `FETCH_HEAD`,不认 `origin/<branch>` —— 后者在个别配置下会是个旧值。
    git(workspace, &["fetch", "origin", &branch]).await?;
    git(workspace, &["merge", "--ff-only", "FETCH_HEAD"]).await?;
    let after = head_sha(workspace).await?;
    Ok(before != after)
}

/// 本机有没有这条分支。
pub async fn branch_exists(workspace: &Path, branch: &str) -> bool {
    git(
        workspace,
        &["rev-parse", "--verify", &format!("refs/heads/{branch}")],
    )
    .await
    .is_ok()
}

/// 收掉一条**已经合过**的本机分支。
///
/// 先试 `git branch -d`,让 git 自己确认它真的合过。远端按 squash 合的时候这
/// 一步会被拒:squash 把一串提交压成主干上的一条新提交,原来那几条并不是主干
/// 的祖先,git 看不出「合过」——而 buddy 这两个远端(GitHub / codehub)合 MR
/// 走的都是 squash。所以这里会退到 `-D`。
///
/// **只准在读回确认远端真的合了之后调用**:那时候改动已经在远端主干上、分支
/// 本身也推上去过,`-D` 删掉的是本机的一个指针,不是劳动成果。没合成的分支
/// 绝不能拿它删。
pub async fn delete_merged_branch(workspace: &Path, branch: &str) -> Result<(), GitError> {
    if git(workspace, &["branch", "-d", branch]).await.is_ok() {
        return Ok(());
    }
    git(workspace, &["branch", "-D", branch]).await?;
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
