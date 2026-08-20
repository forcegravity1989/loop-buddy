//! 总览与通知那几块的现算派生 —— 周进度、运作活、名片改动、仓统计、事件流。
//!
//! 从 `vm_panels.rs` 拆出来的:那个文件已经过 600 行的软目标了
//! (`scripts/guard-file-lines.sh` 会提醒)。这里放的都是「库和仓里没有这张
//! 表,每次现算出来」的东西。

use crate::vm::*;
use bw_v4::model::IssueStatus;
use bw_v4::repo::week_plan_file;
use std::path::Path;

/// 某一周的五段计数。`week` 传 `None` = 不按周过滤(计划屏的「全部活」视图)。
///
/// **直接数活,不数看板列**:看板的范围跟着计划屏左栏走,总览那块「本周计划
/// 进度」要的是本周,两者不能共用一份数——共用的后果是人在计划屏点了历史周,
/// 总览的「本周」标题下摆着那一周的数。
///
/// 待办池不算进去,它按定义就是「没排进任何一周」;**阻塞单独一段**,不能像
/// 早先那样整列漏掉——红的那张活正是最该被看见的。
pub(super) fn build_week_counts(issues: &[bw_v4::Issue], week: Option<&str>) -> WeekCountsVm {
    let in_scope = |i: &&bw_v4::Issue| match week {
        None => true,
        Some(w) => i.week_of == w,
    };
    let n = |st: IssueStatus| {
        issues
            .iter()
            .filter(in_scope)
            .filter(|i| i.status == st)
            .count()
    };
    WeekCountsVm {
        todo: n(IssueStatus::Todo),
        doing: n(IssueStatus::InProgress),
        review: n(IssueStatus::InReview),
        done: n(IssueStatus::Done),
        blocked: n(IssueStatus::Blocked),
    }
}

/// 「本周运作」那张表。周计划文件里没有这一段就是空的 —— 不替它编三行。
pub(super) fn build_ops(plan: Option<&week_plan_file::WeekPlan>) -> Vec<OpsChipVm> {
    plan.map(|p| {
        p.ops
            .iter()
            .map(|r| OpsChipVm {
                title: r.title.clone(),
                status: r.status.clone(),
                note: r.note.clone(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// 名片改动那张在途的轻量活。名片是仓文件,改它走分支 + MR,所以总览要能
/// 看见「改了、还没合」。只认最新的一张。
pub(super) fn build_card_mr(issues: &[bw_v4::Issue]) -> Option<CardMrVm> {
    let i = issues
        .iter()
        .filter(|i| i.title.starts_with("编辑项目名片"))
        .filter(|i| {
            matches!(
                i.status,
                IssueStatus::InReview | IssueStatus::InProgress | IssueStatus::Todo
            )
        })
        .max_by_key(|i| i.number)?;
    Some(CardMrVm {
        issue_id: Some(i.id),
        number: i.number,
        status: i.status.label().to_string(),
        pr_number: i.pr_number,
        mergeable: i.status == IssueStatus::InReview,
    })
}

/// 代码仓级指标。**每一项都注明从哪采的**;采不到就整块给出原话,不填 0。
///
/// 高保真那张网格上还有几项是远端来的(合入的 PR、远端 issue、开放 PR),
/// 那要走 GitHub / codehub 的接口,今天还没接 —— 与其编几个数,不如只列采得
/// 到的,并在界面上说清楚少了哪几项(见 `docs/LEFTOVERS.md`)。
pub(super) async fn collect_repo_stats(ws: &Path) -> RepoStatsVm {
    let e = match bw_engine::evidence::collect(&ws.display().to_string()).await {
        Ok(e) => e,
        Err(err) => {
            return RepoStatsVm {
                items: Vec::new(),
                error: format!("读不到仓统计:{err}"),
            }
        }
    };
    let mut items = vec![
        (e.commit_count.to_string(), "累计提交".into(), "git".into()),
        (
            e.tracked_files.to_string(),
            "跟踪的文件".into(),
            "git".into(),
        ),
        (
            e.dirty_paths.to_string(),
            "没提交的改动".into(),
            "git".into(),
        ),
        (
            e.docs_files.to_string(),
            "docs/ 下的 .md".into(),
            "git".into(),
        ),
    ];
    if let Some(d) = bw_v4::git::first_commit_date(ws).await {
        items.push((d, "首次提交".into(), "git".into()));
    }
    if let Ok(tags) = bw_v4::git::tags(ws).await {
        items.push((tags.len().to_string(), "打过的标签".into(), "git".into()));
    }
    RepoStatsVm {
        items,
        error: String::new(),
    }
}

/// 事件流。**没有事件表** —— 这条流是从四张表里现算的:
///
/// - 活建出来了(`issue.created_at`)
/// - 活结清了(`issue.settled_at`,只结一次,所以这条不会重复)
/// - 会话开出来了(`claude_conversation.created_at`)
///
/// 存不下来的事(某一次运行失败、某一条群消息发没发出去)就**不在流里**。
/// 与其补一条编的,不如少一条。
pub(super) fn build_notify_events(
    issues: &[bw_v4::Issue],
    convs: &[bw_v4::model::Conversation],
) -> Vec<NotifyEventVm> {
    let mut out: Vec<(i64, NotifyEventVm)> = Vec::new();
    let done_of = |id| {
        issues
            .iter()
            .find(|i| i.id == id)
            .is_some_and(|i| i.status == IssueStatus::Done)
    };
    for i in issues {
        if i.created_at > 0 {
            out.push((
                i.created_at,
                NotifyEventVm {
                    time: stamp(i.created_at),
                    text: format!("建了活 #{} {}", i.number, i.title),
                    issue: Some(i.id),
                    done: done_of(i.id),
                },
            ));
        }
        if let Some(t) = i.settled_at {
            out.push((
                t,
                NotifyEventVm {
                    time: stamp(t),
                    text: format!("#{} {} 完成并结清", i.number, i.title),
                    issue: Some(i.id),
                    done: true,
                },
            ));
        }
    }
    for c in convs {
        let Some(i) = issues.iter().find(|i| i.id == c.issue_id) else {
            continue;
        };
        if c.created_at > 0 {
            out.push((
                c.created_at,
                NotifyEventVm {
                    time: stamp(c.created_at),
                    text: format!("#{} {} 开了会话({})", i.number, i.title, c.branch_name),
                    issue: Some(i.id),
                    done: done_of(i.id),
                },
            ));
        }
    }
    // 新的在前。同一秒的保持稳定顺序,不让它每次重拼都跳。
    out.sort_by_key(|(t, _)| std::cmp::Reverse(*t));
    out.into_iter().take(80).map(|(_, e)| e).collect()
}

/// 时间戳按**本机时区**格式化 —— 周也是按本机时区算的,两处得一致。
///
/// 偏移取的是 `isoweek` 在启动单线程阶段探好存下来的那一份。**这里绝不能自己
/// 再调一次 `current_local_offset()`**:那个系统调用在多线程进程里必然返回
/// Err,退回 UTC 之后事件流比真实时间早 8 小时,而同屏的周号仍是本机周。
fn stamp(unix: i64) -> String {
    let offset = bw_v4::isoweek::local_offset();
    match time::OffsetDateTime::from_unix_timestamp(unix) {
        Ok(t) => {
            let t = t.to_offset(offset);
            format!(
                "{:02}-{:02} {:02}:{:02}",
                t.month() as u8,
                t.day(),
                t.hour(),
                t.minute()
            )
        }
        Err(_) => String::new(),
    }
}
