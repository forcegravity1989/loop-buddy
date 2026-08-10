//! 活聚合的命令用例(design-s5-hexpanel.md §4.2 `cmd/issue.rs`)。**只有
//! 一条**在本片范围内:活状态的显式转移——「完成永远由人点」这条铁律的
//! 编排层落点。`RunIssue`/`CancelRun`(设计稿草案里同属活聚合)不在这里
//! ——那两条命令要驱动 `run::RunManager`,而 `RunManager` 是独立于 `App`
//! 的另一个存储连接持有者(切片四的既有架构决定,`lib.rs` 顶部文档已经
//! 写清楚);把它们并进这个 `Command` 总线,意味着 `App` 要么反过来持有
//! `RunManager`、要么在两者之间转发,两条路都会碰这条既有边界,超出本
//! 片「命令/事件总线按聚合拆」的范围。设计稿原文自己也说这份 `Command`
//! 草案「实施时按真实需要收敛」——这就是一次收敛,写进 commit 正文,不
//! 是遗漏。

use bw_core::{IssueId, IssueStatus};
use bw_store::{IssueStore, SqliteStore};

use crate::AppError;

/// 「完成永远由人点」在编排层的落点(design-s4-runmanager.md §3.6):
/// ①读当前状态 ②查 `bw_core::IssueStatus::can_transition_to` ③合法才落
/// 到 `bw_store::IssueStore::transition_issue_status` 的比较并置写。
///
/// 从 `App::transition_issue`(切片四B)原样搬过来——这是「按聚合拆」的
/// 第一个真实搬迁:此前这条用例的正文直接写在 `App` 结构体的方法体里,
/// 现在挪进它所属的聚合模块,`App::transition_issue` 变成一行转发
/// (`lib.rs`「只做路由」)。
pub async fn transition(store: &SqliteStore, id: IssueId, to: IssueStatus) -> Result<(), AppError> {
    let Some(row) = store.get_issue(id).await? else {
        return Err(AppError::IssueNotFound(id));
    };
    if !row.status.can_transition_to(to) {
        return Err(AppError::IllegalTransition {
            from: row.status,
            to,
        });
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let wrote = store
        .transition_issue_status(id, row.status, to, now)
        .await?;
    if !wrote {
        return Err(AppError::TransitionRaced {
            from: row.status,
            to,
        });
    }
    Ok(())
}
