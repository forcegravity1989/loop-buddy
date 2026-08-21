//! 近 N 周的走势 —— **全部现算,一个数都不存**。
//!
//! 这是 [`design.md`](../../../docs/v4-prototype/design.md) 第 10 章「指标采集与读数」
//! 里「A 类 · 可回溯」那一类的第一个真实落点:能采到今天的数,就能采到过去任意
//! 一周的数,所以根本不需要谁提前把历史存下来。要看四周就算四个窗口,要看八周
//! 就算八个 —— **面板要多少就采多少**,不存在「存了 30 天却要画 8 周」那种错配。
//!
//! 每个数都能自己复算,命令写在各字段的注释里。

use crate::isoweek;
use crate::model::Project;
use std::path::Path;
use time::Duration;

/// 一周一个点。**采不到的项一律 `None`,绝不填 0** —— 0 是一个真实的数值,
/// 「没采到」不是。三条线都守这一条:git 读不动(不是仓、没装 git)时
/// `commits`/`merges` 也是 `None`,画的时候断开,不画一条「这几周都是 0」的
/// 假实线(第一版把 git 失败 `unwrap_or_default` 成 0,评审抓的)。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeekPoint {
    /// ISO 周,如 `2026-W34`。
    pub week: String,
    /// 本机提交数。复算命令见 `git::week_counts_many` 的文档。
    pub commits: Option<u32>,
    /// 本机合入数(父提交 ≥ 2)。
    pub merges: Option<u32>,
    /// 远端**合入**的 PR 数。`None` = 没挂远端 / 远端按周查还没接 / 这次没查成。
    pub merged_prs: Option<u32>,
}

/// 走势 + 两句「那条线为什么是空的」。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trend {
    /// 旧的在前,最后一个是本周。
    pub points: Vec<WeekPoint>,
    /// git 那两条没采到时的原话。空 = 采到了。**不吞错误**。
    pub git_note: String,
    /// 远端那条没采到时的原话。空 = 采到了,或者压根没挂远端。**不吞错误**。
    pub remote_note: String,
}

/// 算最近 `weeks` 周(含本周),旧的在前。
///
/// 三条线都遵守同一条:**采不到就是 `None`,失败原话带回,绝不变成 0**。
/// git 那两条一趟遍历出齐;远端那条只在挂了 GitHub 时才问,各周并发。
pub async fn recent_weeks(ws: &Path, project: &Project, weeks: u32) -> Trend {
    recent_weeks_inner(ws, github_repo(project), project.has_remote(), weeks).await
}

/// 只算 git 那两条,不问远端。项目行读不出来时用它 —— **不伪造一个空项目**
/// 去问远端,那等于拿一个假地址去查。
pub async fn recent_weeks_git_only(ws: &Path, weeks: u32) -> Trend {
    recent_weeks_inner(ws, None, false, weeks).await
}

async fn recent_weeks_inner(
    ws: &Path,
    remote: Option<String>,
    has_remote: bool,
    weeks: u32,
) -> Trend {
    let mut list = week_labels(weeks);
    list.reverse(); // week_labels 从本周往回数,这里翻成「旧的在前」

    // git 那两条:**一趟遍历出齐所有周**,不按周循环(逐周查会把全史遍历次数
    // 乘上周数 —— 第一版就是这么让回填冻屏一分钟的)。读不动就整体留空 +
    // 一句原话,绝不 0 充数。
    let mut trend = Trend::default();
    let counts = match crate::git::week_counts_many(ws, &list).await {
        Ok(m) => Some(m),
        Err(e) => {
            trend.git_note = format!("本机提交数没读成:{e}");
            None
        }
    };

    // 远端那条:`remote` 是 `github_repo()` 判出来的(**provider 判断走底座
    // 那一份**,不在这儿手写字符串比较 —— 手写会和 `Remote::for_project` 分叉:
    // 它把空 provider 当 github,存量项目就是空的,于是同一个项目「合入」走
    // gh、走势图却说「不是 GitHub」)。
    if remote.is_none() && has_remote {
        trend.remote_note =
            "这个项目的远端不是 GitHub,按周查合入数还没接 —— 只画 git 那两条".into();
    }

    // 四周就是四个网络往返,串行等于四倍等待。并发发出去,回来再按周对齐。
    let remote_counts: Vec<Result<u32, String>> = match &remote {
        None => Vec::new(),
        Some(owner_repo) => {
            let mut set = Vec::new();
            for week in &list {
                set.push(merged_prs_of(owner_repo.clone(), week.clone()));
            }
            futures_join_all(set).await
        }
    };

    for (i, week) in list.iter().enumerate() {
        let c = counts.as_ref().and_then(|m| m.get(week));
        let merged_prs = match remote_counts.get(i) {
            None => None,
            Some(Ok(n)) => Some(*n),
            Some(Err(e)) => {
                // 第一条错误就够了 —— 八周全失败不必刷屏同一句话。
                if trend.remote_note.is_empty() {
                    trend.remote_note = e.clone();
                }
                None
            }
        };
        trend.points.push(WeekPoint {
            week: week.clone(),
            commits: c.map(|c| c.commits),
            merges: c.map(|c| c.merges),
            merged_prs,
        });
    }
    trend
}

/// 并发跑完一批 future 并按原顺序收结果。**不引 futures crate** —— 这是唯一
/// 用到的组合子,tokio 的 `JoinSet` 不保序,自己 spawn 再按序 await 更直白。
async fn futures_join_all<T: Send + 'static>(
    tasks: Vec<impl std::future::Future<Output = T> + Send + 'static>,
) -> Vec<T> {
    let handles: Vec<_> = tasks.into_iter().map(tokio::spawn).collect();
    let mut out = Vec::with_capacity(handles.len());
    for h in handles {
        match h.await {
            Ok(v) => out.push(v),
            // 任务 panic 了:这一格没有结果,但不能把整批拖垮。
            Err(_) => return out,
        }
    }
    out
}

/// 从本周往回数 `weeks` 个 ISO 周(含本周),新的在前。
fn week_labels(weeks: u32) -> Vec<String> {
    let mut out = Vec::new();
    let mut w = isoweek::current_week();
    for _ in 0..weeks.max(1) {
        out.push(w.clone());
        let Some(prev) = isoweek::previous_week(&w) else {
            break;
        };
        w = prev;
    }
    out
}

/// `owner/repo`,只在远端确实是 GitHub 时给。
///
/// **provider 的判断走底座那一份**([`v4_engine::remote::Remote::for_project`]),
/// 不在这里手写 `provider == "github"` —— 底座把空 provider 当 github(存量项目
/// 就是空的),手写一份会分叉:同一个项目「合入并完成」走 gh,走势图却报
/// 「远端不是 GitHub」。
fn github_repo(project: &Project) -> Option<String> {
    if !project.has_remote() {
        return None;
    }
    match v4_engine::remote::Remote::for_project(
        &project.provider,
        &project.remote_host,
        &project.remote_path,
    ) {
        Ok(v4_engine::remote::Remote::Github(owner_repo)) => Some(owner_repo),
        _ => None,
    }
}

/// 某一周合入的 PR 数。
///
/// **周边界带本机时区偏移**:git 那两条按本机时区切周(`git::week_window`),
/// 而 GitHub 的 `merged:` 限定符不带时区时按 UTC 解释 —— UTC+8 下同一张图上
/// 两条线会错开 8 小时,周一早上合的 PR 掉进上一周(评审抓的)。所以这里给的
/// 是带偏移的完整时刻,让两边切在同一刀上。
async fn merged_prs_of(owner_repo: String, week: String) -> Result<u32, String> {
    let (monday, next_monday) =
        isoweek::week_bounds(&week).ok_or_else(|| format!("认不出周号 {week}"))?;
    let off = isoweek::local_offset();
    let at = |d: time::Date| -> String {
        let t = d
            .with_hms(0, 0, 0)
            .expect("00:00:00 一定合法")
            .assume_offset(off);
        // `2026-08-17T00:00:00+08:00` —— GitHub 搜索认这种带偏移的完整时刻。
        format!(
            "{:04}-{:02}-{:02}T00:00:00{}{:02}:{:02}",
            t.year(),
            u8::from(t.month()),
            t.day(),
            if off.whole_hours() < 0 { "-" } else { "+" },
            off.whole_hours().abs(),
            (off.whole_minutes() % 60).abs(),
        )
    };
    // GitHub 的 `merged:a..b` 含两端。给 `<下周一 00:00` 的开区间语义,就把
    // 上界写成下周一前一秒 —— 用 `..` 配一个含末端的时刻即可。
    let until = next_monday - Duration::days(1);
    v4_engine::github::merged_pr_count(&owner_repo, &at(monday), &{
        let s = at(until);
        s.replacen("T00:00:00", "T23:59:59", 1)
    })
    .await
    .map_err(|e| format!("远端合入数没查成:{e}"))
}
