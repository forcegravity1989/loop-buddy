//! 近 N 周的走势 —— **全部现算,一个数都不存**。
//!
//! 这是 [`design/14-metrics-collection.md`](../../../docs/v4-prototype/design/14-metrics-collection.md)
//! 里「A 类 · 可回溯」那一类的第一个真实落点:能采到今天的数,就能采到过去任意
//! 一周的数,所以根本不需要谁提前把历史存下来。要看四周就算四个窗口,要看八周
//! 就算八个 —— **面板要多少就采多少**,不存在「存了 30 天却要画 8 周」那种错配。
//!
//! 每个数都能自己复算,命令写在各字段的注释里。

use crate::isoweek;
use crate::model::Project;
use std::path::Path;
use time::Duration;

/// 一周一个点。**采不到的项如实留空,绝不填 0** —— 0 是一个真实的数值,
/// 「没采到」不是。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeekPoint {
    /// ISO 周,如 `2026-W34`。
    pub week: String,
    /// `git log --since=<周一> --until=<下周一> --pretty=format:%H | wc -l`
    pub commits: u32,
    /// `git log --merges --since=… --until=… | wc -l`
    pub merges: u32,
    /// 远端**合入**的 PR 数。`None` = 没挂远端 / 远端不是 GitHub / 这次没查成。
    pub merged_prs: Option<u32>,
}

/// 走势 + 一句「远端那条为什么是空的」。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trend {
    /// 旧的在前,最后一个是本周。
    pub points: Vec<WeekPoint>,
    /// 远端那条线没采到时的原话。空 = 采到了(或者压根没挂远端,那时
    /// [`Self::remote_attempted`] 是 false)。**不吞错误**。
    pub remote_note: String,
    /// 这次到底试没试过远端。false 时界面不该说「远端采集失败」——
    /// 它压根没试。
    pub remote_attempted: bool,
}

/// 算最近 `weeks` 周(含本周),旧的在前。
///
/// git 那两条一定会算(读不动就是 0 —— 那是 git 真的没给出提交,不是缺数据);
/// 远端那条只在项目挂了 GitHub 远端时才试,失败原话原样带回,**不静默变成 0**。
pub async fn recent_weeks(ws: &Path, project: &Project, weeks: u32) -> Trend {
    let mut list = week_labels(weeks);
    list.reverse(); // week_labels 从本周往回数,这里翻成「旧的在前」

    // 远端只在挂了 GitHub 时才问。codehub 的按周窗口查询今天还没有对应的
    // 命令,如实说没接,不猜一个 codehub-cli 的参数写上去。
    let remote = github_repo(project);
    let mut trend = Trend {
        remote_attempted: remote.is_some(),
        ..Trend::default()
    };
    if remote.is_none() && project.has_remote() {
        trend.remote_note = format!(
            "这个项目的远端是 {},按周查合入数还没接 —— 只画 git 那两条",
            if project.provider.is_empty() {
                "(没写提供方)"
            } else {
                &project.provider
            }
        );
    }

    for week in list {
        let stats = crate::git::week_stats(ws, &week).await.unwrap_or_default();
        let merged_prs = match &remote {
            None => None,
            Some(owner_repo) => match merged_prs_of(owner_repo, &week).await {
                Ok(n) => Some(n),
                Err(e) => {
                    // 第一条错误就够了 —— 八周全失败会刷屏同一句话。
                    if trend.remote_note.is_empty() {
                        trend.remote_note = e;
                    }
                    None
                }
            },
        };
        trend.points.push(WeekPoint {
            week,
            commits: stats.commits,
            merges: stats.merges,
            merged_prs,
        });
    }
    trend
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
fn github_repo(project: &Project) -> Option<String> {
    if !project.has_remote() || project.provider != "github" {
        return None;
    }
    Some(project.remote_path.clone())
}

/// 某一周合入的 PR 数。
///
/// **这里把「左闭右开」换成「闭区间」**:`week_bounds` 给的是(周一,下周一),
/// 而 GitHub 的 `merged:a..b` 含两端 —— 直接把下周一丢进去会把下一周第一天的
/// PR 算进这一周。
async fn merged_prs_of(owner_repo: &str, week: &str) -> Result<u32, String> {
    let (monday, next_monday) =
        isoweek::week_bounds(week).ok_or_else(|| format!("认不出周号 {week}"))?;
    let sunday = next_monday - Duration::days(1);
    v4_engine::github::merged_pr_count(owner_repo, &monday.to_string(), &sunday.to_string())
        .await
        .map_err(|e| format!("远端合入数没查成:{e}"))
}
