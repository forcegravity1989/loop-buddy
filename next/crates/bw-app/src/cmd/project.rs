//! 项目聚合的命令用例(design-s5-hexpanel.md §4.2 `cmd/project.rs`):切
//! 阶段(首次开棒)、交棒(往下一棒推进)。合法性判断住在这里(用例
//! 层),`bw_store::HandoffStore` 那两个方法只管机械写——同 `RunStore`/
//! `IssueStore` 既有的分工。

use bw_core::{HandoffId, ProjectId, StageKind};
use bw_store::{HandoffStore, IssueStore, SqliteStore};

use crate::AppError;

fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// 首次开棒:`project.active_stage` 从 `NULL` 推到 `stage`。**只允许在
/// 从未开棒时调用**——已经开过棒的项目要往下一棒走,走
/// [`handoff_stage`],不是再调一次这个方法(`bw_store::HandoffStore::
/// set_active_stage` 本身在存储层是「无条件写」,这条「只许开一次」的
/// 合法性判断是这一层加的,不是存储层的事——同 `IssueStatus::
/// can_transition_to` 那条「合法性由调用方查、存储层只管写」的既有分
/// 工)。
pub async fn set_active_stage(
    store: &SqliteStore,
    project_id: ProjectId,
    stage: StageKind,
) -> Result<(), AppError> {
    let Some(project) = store.get_project(project_id).await? else {
        return Err(AppError::ProjectNotFound(project_id));
    };
    if let Some(current) = project.active_stage {
        return Err(AppError::StageAlreadySet {
            project: project_id,
            current,
        });
    }
    store
        .set_active_stage(project_id, stage, now_unix())
        .await?;
    Ok(())
}

/// 交棒:从当前一棒(`project.active_stage`)推到它的下一棒
/// (`StageKind::next()`——五阶段闭环,`Ops → Prototype` 是复盘回流,不是
/// 死路)。**从未开棒时不能交棒**——没有「从」这个非空起点,`handoff`
/// 表的 `from_stage` 是 `NOT NULL`,如实报错而不是替用户瞎猜一个起点。
pub async fn handoff_stage(
    store: &SqliteStore,
    project_id: ProjectId,
    risky: bool,
    note: &str,
) -> Result<HandoffId, AppError> {
    let Some(project) = store.get_project(project_id).await? else {
        return Err(AppError::ProjectNotFound(project_id));
    };
    let Some(from) = project.active_stage else {
        return Err(AppError::NoActiveStage(project_id));
    };
    let to = from.next();
    let id = store
        .handoff_stage(project_id, from, to, risky, note, now_unix())
        .await?;
    Ok(id)
}
