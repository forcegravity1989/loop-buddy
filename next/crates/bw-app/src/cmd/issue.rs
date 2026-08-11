//! 活聚合的命令用例(design-s5-hexpanel.md §4.2 `cmd/issue.rs`)。**两条**
//! 在本片范围内:①活状态的显式转移——「完成永远由人点」这条铁律的编排
//! 层落点;②活的所属阶段(切片五E 主控补充要求,见 [`set_stage`] 文
//! 档)。`RunIssue`/`CancelRun`(设计稿草案里同属活聚合)不在这里——那两
//! 条命令要驱动 `run::RunManager`,而 `RunManager` 是独立于 `App` 的另一
//! 个存储连接持有者(切片四的既有架构决定,`lib.rs` 顶部文档已经写清
//! 楚);把它们并进这个 `Command` 总线,意味着 `App` 要么反过来持有
//! `RunManager`、要么在两者之间转发,两条路都会碰这条既有边界,超出本
//! 片「命令/事件总线按聚合拆」的范围。设计稿原文自己也说这份 `Command`
//! 草案「实施时按真实需要收敛」——这就是一次收敛,写进 commit 正文,不
//! 是遗漏。

use bw_core::{IssueId, IssueStatus, StageKind};
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

/// `issue.stage` 的第一条生产写入方(切片五E 主控补充要求,plan/23 §10
/// 第 16 条清偿)——`Command::SetIssueStage` 的落地。切片五B 建了这一列
/// (design §2.1),切片五C/D 建了读它的两处消费者(五角色责任卡的活数
/// 分组、六段/待人处理的推导),但没有任何命令能写它;`hex_readback` 只
/// 能绕过 store 直接 `UPDATE issue SET stage = ?` 造数,每一行都标注非
/// 生产路径产出。这条命令补上正经的出生路径:①活必须存在②纯机械写,
/// 不判断「这件活该不该归到这个阶段」这类业务合法性——同
/// `cmd::project::set_active_stage`「合法性住用例层、存储层只管写」的既
/// 有分工,这里合法性检查只有「活存在」这一条,因为「一件活归哪个阶
/// 段」不像状态机转移那样有一张合法转移表,人显式指哪一阶段就是哪一阶
/// 段(包括显式退回 `None`=未归类)。
pub async fn set_stage(
    store: &SqliteStore,
    id: IssueId,
    stage: Option<StageKind>,
) -> Result<(), AppError> {
    if store.get_issue(id).await?.is_none() {
        return Err(AppError::IssueNotFound(id));
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    store.set_issue_stage(id, stage, now).await?;
    Ok(())
}
