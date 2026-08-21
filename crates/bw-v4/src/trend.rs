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

/// 一周一个点。**采不到的项一律 `None`,绝不填 0** —— 0 是一个真实的数值,
/// 「没采到」不是。三条线都守这一条:git 读不动(不是仓、没装 git)时
/// `commits` 是 `None`,远端查不成时另外两条是 `None`,画的时候断开,不画一条
/// 「这几周都是 0」的假实线(第一版把 git 失败 `unwrap_or_default` 成 0,评审抓的)。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeekPoint {
    /// ISO 周,如 `2026-W34`。
    pub week: String,
    /// 本机提交数。复算命令见 `git::week_counts_many` 的文档。
    pub commits: Option<u32>,
    /// 远端**合入**的 PR 数。`None` = 没挂远端 / 远端不是 GitHub / 这次没查成。
    pub merged_prs: Option<u32>,
    /// 这一周**周末那一刻**还没关闭的 issue 数。
    ///
    /// 和上面两条不是一类东西:那两条是**流量**(这一周发生了多少),这条是
    /// **存量**(到这一刻还欠着多少)。放在同一排看的理由是它俩正好互补 ——
    /// 提交和合入告诉你推了多少,未处理 issue 告诉你欠的有没有被推下去。
    pub open_issues: Option<u32>,
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

    // git 那条:**一趟遍历出齐所有周**,不按周循环(逐周查会把全史遍历次数
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

    // 远端那两条:**provider 判断走底座那一份**([`v4_engine::remote::Remote::for_project`]),
    // 不在这儿手写字符串比较 —— 手写会和它分叉:底座把空 provider 当 github,
    // 存量项目就是空的,于是同一个项目「合入」走 gh、走势图却说「不是 GitHub」。
    if remote.is_none() && has_remote {
        trend.remote_note =
            "这个项目的远端不是 GitHub,合入的 PR 与未处理 issue 还没接 —— 只画提交那一条".into();
    }

    // 一趟拉全、本地按周分桶。**不按周去查** —— 见 `v4_engine::github` 里那两个
    // 读数函数上面的说明。
    let (merged, open_issues) = match &remote {
        None => (None, None),
        Some(owner_repo) => {
            let (a, b) = tokio::join!(
                merged_prs_by_week(owner_repo, &list),
                open_issues_by_week(owner_repo, &list),
            );
            let merged = note_or(&mut trend.remote_note, a);
            let open = note_or(&mut trend.remote_note, b);
            (merged, open)
        }
    };

    for (i, week) in list.iter().enumerate() {
        let c = counts.as_ref().and_then(|m| m.get(week));
        trend.points.push(WeekPoint {
            week: week.clone(),
            commits: c.map(|c| c.commits),
            merged_prs: merged.as_ref().and_then(|v| v.get(i).copied()),
            open_issues: open_issues.as_ref().and_then(|v| v.get(i).copied()),
        });
    }
    trend
}

/// 成功就把值拿出来,失败就把原话记进 `note`(**第一条错误就够了**,两条线
/// 一起失败不必把同一句话刷两遍)并返回 `None`。**绝不吞错误、绝不用 0 顶上**。
fn note_or<T>(note: &mut String, r: Result<T, String>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            if note.is_empty() {
                *note = e;
            }
            None
        }
    }
}

/// 每周合入的 PR 数,和 `weeks` 一一对应。
async fn merged_prs_by_week(owner_repo: &str, weeks: &[String]) -> Result<Vec<u32>, String> {
    let rows = v4_engine::github::merged_pr_times(owner_repo)
        .await
        .map_err(|e| format!("远端合入的 PR 没查成:{e}"))?;
    // 拉回来的条数正好顶到上限 = 可能被截断了。**这时候宁可整条线不画**,
    // 也不端一个少算的数出去(设计正本 §10.9:采不到就是采不到,不写 0)。
    if rows.len() as u32 >= v4_engine::github::LIST_CAP {
        return Err(format!(
            "这个仓的已合并 PR 超过 {} 条,一趟拉不全 —— 这条线不画,免得给你一个少算的数",
            v4_engine::github::LIST_CAP
        ));
    }
    let stamps: Vec<i64> = rows.iter().filter_map(|s| epoch_of(s)).collect();
    let mut out = Vec::with_capacity(weeks.len());
    for w in weeks {
        let (s, u) = window_of(w)?;
        out.push(stamps.iter().filter(|t| (s..u).contains(t)).count() as u32);
    }
    Ok(out)
}

/// 每周**周末那一刻**还开着的 issue 数,和 `weeks` 一一对应。
///
/// 算法就是定义本身:到那一刻为止建过的,减去到那一刻为止关掉的。所以它天然
/// 可回溯 —— 拿同一份流水换个时刻,过去任意一周的值都算得出来,不需要谁提前
/// 把它存下来。复算:见 `v4_engine::github::issue_times` 的文档。
async fn open_issues_by_week(owner_repo: &str, weeks: &[String]) -> Result<Vec<u32>, String> {
    let rows = v4_engine::github::issue_times(owner_repo)
        .await
        .map_err(|e| format!("远端未处理 issue 数没查成:{e}"))?;
    if rows.len() as u32 >= v4_engine::github::LIST_CAP {
        return Err(format!(
            "这个仓的 issue 超过 {} 条,一趟拉不全 —— 这条线不画,免得给你一个少算的数",
            v4_engine::github::LIST_CAP
        ));
    }
    let born: Vec<i64> = rows.iter().filter_map(|(c, _)| epoch_of(c)).collect();
    let closed: Vec<i64> = rows
        .iter()
        .filter_map(|(_, k)| epoch_of(k.as_deref()?))
        .collect();
    let mut out = Vec::with_capacity(weeks.len());
    for w in weeks {
        // 周末那一刻 = 下周一 00:00 的前一瞬,用左闭右开的右边界即可。
        let (_, end) = window_of(w)?;
        let opened = born.iter().filter(|t| **t < end).count();
        let shut = closed.iter().filter(|t| **t < end).count();
        out.push(opened.saturating_sub(shut) as u32);
    }
    Ok(out)
}

/// 那一周的 `[周一 00:00, 下周一 00:00)`,unix 秒,**按本机时区切**。
///
/// 和 git 那条线用的是同一个函数,所以两条线切在同一刀上 —— 曾经远端那条按
/// UTC 切、git 那条按本机切,UTC+8 下周一早上的动静掉进上一周(评审抓的)。
fn window_of(week: &str) -> Result<(i64, i64), String> {
    crate::git::week_window(week).map_err(|e| e.to_string())
}

/// RFC3339 → unix 秒。**解析不出来的点直接跳过,不猜、不补** —— 少一个点画出来
/// 是断开,而猜一个点画出来是假数据。
fn epoch_of(s: &str) -> Option<i64> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|t| t.unix_timestamp())
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
