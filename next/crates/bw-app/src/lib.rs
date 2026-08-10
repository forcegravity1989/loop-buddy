//! `bw-app` — next 切片四:编排层(design-s4-runmanager.md §1)。
//!
//! 运行管理器(开工/取消/结束回写/重启清理)住在这里——它要写库、要判
//! 状态机,这是编排层的活;放进 `bw-engine` 会把「引擎只管对外能力的实
//! 现,不管编排判断」这条边界揉回旧工程结构病里去。
//!
//! **切片四A** 只立了骨架:[`App`] 把存储层装起来。**切片四B/C** 在此之
//! 上把运行管理器本体接起来(见 [`run`] 模块——`RunManager` 单口 API:开
//! 工/取消/降级为咨询/重启清理,§3/§4),并给 [`App`] 补上
//! [`App::transition_issue`]——「完成永远由人点」这条铁律在编排层的落
//! 点:写活状态之前必须先查 `bw_core::IssueStatus::can_transition_to`,
//! `RunManager` 自己完全不碰这个方法(它只用 `bw_store::IssueStore::
//! mark_issue_in_progress` 那条没有 `IssueStatus` 形参的窄方法,见该方法
//! 文档)。
//!
//! **next 切片五C**(design-s5-hexpanel.md §4.2/§4.3)在此之上做两件事:
//!
//! 1. **命令/事件总线按聚合拆**:[`Command`]/[`Event`] 两个枚举 + 一条
//!    [`App::dispatch`] 路由,真正的用例正文全部住进 [`cmd`] 的三个子模
//!    块(项目/指标/活各自一片)——`dispatch` 里只有一个转发用的
//!    `match`,不含任何业务判断。**这份 `Command` 是设计稿草案的一次收
//!    敛**:`Boot`/`OpenProject`/`SetPanel`(壳的导航状态,壳本片不建)、
//!    `RunIssue`/`CancelRun`(要驱动独立于 `App` 的 `RunManager`,合并会
//!    碰切片四钉下的「两个各自独立的存储连接持有者」边界)、
//!    `CollectMetrics`/`ProbeConnectors`(自动/后台触发的连接器调用,
//!    §3.3/§8 明令本片不做)都不在这个枚举里——设计稿原文自己写着「草
//!    案,实施时按真实需要收敛」,这就是那次收敛,详情见对应 commit 正
//!    文。
//! 2. **六段总控视图的组装用例**:[`App::hex_view`]——查库,拼装喂给
//!    [`view::hex::build`] 那个纯函数,纯推导逻辑与「去哪查库」两件事分
//!    开放,但装配本身仍然是 `App` 的方法,让 `hex_readback` 指挥器与将
//!    来的桌面壳调的是同一段代码,不是两份平行发明。待人处理投影的组
//!    装用例(`App::attention_view`)是下一个 commit(切片五D)的事,本
//!    片(切片五C)先把六段这一半立起来。
//!
//! 正式依赖里刻意没有 `bw-engine`:PTY/agentcli 那堆原生依赖不该渗进编排
//! 层,`scripts/guard-app-layering.sh` 在 CI 里守这条线(只查
//! `[dependencies]` 节,查的是 `cargo tree -e normal` 真实依赖图——
//! `bw-engine` 作为 dev-dependency 出现在「真实并行三件」那一档指挥器
//! 里,不受影响)。`bw-workspace` 是切片五C 新加的正式依赖(design §6.2
//! 分层图 `bw-workspace ← bw-app`)——指标同步用例要读项目仓正本,六段视
//! 图组装用例要现采工作区证据。

pub mod cmd;
mod error;
pub mod run;
pub mod view;
pub use error::AppError;

use std::collections::HashMap;

use bw_core::{HandoffId, IssueId, IssueStatus, MetricId, ObservationId, ProjectId, StageKind};
use bw_store::{
    HandoffStore, IssueStore, MetricStore, MetricSyncReport, ObservationStore, RunStore,
    SqliteStore,
};
use bw_workspace::evidence::WorkspaceEvidence;
use time::OffsetDateTime;

/// 编排层的把手。存储层的一个薄壳 + 状态转移的合法性守卫。运行管理器
/// (`run::RunManager`)不经过 `App`——它自己独立开库(design §3.1
/// `RunManager::open` 签名直接收 `db: &Path`),`App` 与 `RunManager` 是
/// 两个各自独立的存储连接持有者,不共享同一个 `SqliteStore` 句柄(SQLite
/// 允许多个连接指向同一个文件,单写者串行化靠文件锁,不靠这里少开一个
/// 连接)。
pub struct App {
    pub store: SqliteStore,
}

/// 编排层的命令面(design §4.2)。**六个变体,是设计稿草案的一次收敛**
/// ——见本文件顶部模块文档「为什么」。壳(切片五E,本片不建)会把界面动
/// 作翻译成这些命令发过来;`hex_readback` 指挥器同样经这条总线造数据,
/// 不绕开它直接调 `cmd::*` 私自造一条平行路径。
pub enum Command {
    SetActiveStage {
        project: ProjectId,
        stage: StageKind,
    },
    HandoffStage {
        project: ProjectId,
        risky: bool,
        note: String,
    },
    SyncMetricsFile {
        project: ProjectId,
        workspace: String,
    },
    RecordManualObservation {
        metric: MetricId,
        raw: String,
    },
    ArchiveMetric {
        metric: MetricId,
    },
    TransitionIssue {
        issue: IssueId,
        to: IssueStatus,
    },
}

/// 每条 [`Command`] 成功之后的回执——壳(未来)拿它决定要不要重建六段视
/// 图并重新渲染;`hex_readback` 拿它核对「命令真的做了它说要做的事」。
#[derive(Debug)]
pub enum Event {
    StageSet,
    HandoffRecorded(HandoffId),
    MetricsSynced(MetricSyncReport),
    ObservationRecorded(ObservationId),
    /// `true` = 这次真停用了;`false` = 已经停用过,幂等空转(同
    /// `bw_store::MetricStore::archive_metric` 的返回值语义)。
    MetricArchived {
        changed: bool,
    },
    IssueTransitioned,
}

impl App {
    /// 装配:开库,拿到存储层把手。**不建任何运行管理器循环任务**——运行
    /// 管理器是独立的 `run::RunManager::open`,不经过 `App::open`。
    pub async fn open(db_path: &str) -> Result<Self, AppError> {
        let store = SqliteStore::open(db_path).await?;
        Ok(Self { store })
    }

    /// 命令总线的唯一入口(design §4.2)。**只做路由,不含任何用例正
    /// 文**——每一支都是「转给对应聚合模块 → 把结果包成 `Event`」,业务
    /// 判断全部住在 [`cmd`] 的子模块里。这是「命令/事件总线按聚合拆,不
    /// 是一个巨型分发器」的落地:这个 `match` 本身增长的速度只跟着新聚
    /// 合的数量走,不会随着每条命令的实现细节一起膨胀。
    pub async fn dispatch(&self, cmd: Command) -> Result<Event, AppError> {
        match cmd {
            Command::SetActiveStage { project, stage } => {
                cmd::project::set_active_stage(&self.store, project, stage).await?;
                Ok(Event::StageSet)
            }
            Command::HandoffStage {
                project,
                risky,
                note,
            } => {
                let id = cmd::project::handoff_stage(&self.store, project, risky, &note).await?;
                Ok(Event::HandoffRecorded(id))
            }
            Command::SyncMetricsFile { project, workspace } => {
                let report =
                    cmd::metric::sync_metrics_file(&self.store, project, &workspace).await?;
                Ok(Event::MetricsSynced(report))
            }
            Command::RecordManualObservation { metric, raw } => {
                let id = cmd::metric::record_manual_observation(&self.store, metric, raw).await?;
                Ok(Event::ObservationRecorded(id))
            }
            Command::ArchiveMetric { metric } => {
                let changed = cmd::metric::archive_metric(&self.store, metric).await?;
                Ok(Event::MetricArchived { changed })
            }
            Command::TransitionIssue { issue, to } => {
                cmd::issue::transition(&self.store, issue, to).await?;
                Ok(Event::IssueTransitioned)
            }
        }
    }

    /// 活状态的显式转移——「完成永远由人点;`InReview → Done` 是合法转移
    /// 表里 Done 的唯一入边」这条产品铁律在编排层的落点(design §3.6)。
    /// **正文已经搬进 [`cmd::issue::transition`]**(切片五C「按聚合拆」的
    /// 第一次真实搬迁);这个方法保留作为直调入口——`run_races` 指挥器
    /// (切片四)与将来经 [`Command::TransitionIssue`] 走总线的调用方,两
    /// 条路径最终都落到同一个函数体,不是两份平行实现。
    pub async fn transition_issue(&self, id: IssueId, to: IssueStatus) -> Result<(), AppError> {
        cmd::issue::transition(&self.store, id, to).await
    }

    /// 六段总控视图组装用例(design §4.3「查库在调用它的用例里做」)——
    /// 查库、拼装,喂给 [`view::hex::build`] 那个纯函数。`loop_inputs`(运
    /// 行管理器聚合快照的四个数 + 例外明细)与 `workspace_evidence`(交
    /// 付证据段③,§2.6「现采不落库」)由**调用方**准备好传进来:前者要
    /// 问独立于 `App` 的 `run::RunManager`,后者要跑真实 git 子进程——两
    /// 者都不是 `App`/`SqliteStore` 能自己查出来的东西,这个方法自己也
    /// 不去猜一份假数据。
    pub async fn hex_view(
        &self,
        project_id: ProjectId,
        loop_inputs: view::hex::LoopInputs,
        workspace_evidence: Option<WorkspaceEvidence>,
        now: OffsetDateTime,
    ) -> Result<view::HexView, AppError> {
        let project = self
            .store
            .get_project(project_id)
            .await?
            .ok_or(AppError::ProjectNotFound(project_id))?;
        let metrics = self.store.list_metrics(project_id).await?;
        let mut observations_by_metric = HashMap::with_capacity(metrics.len());
        for m in &metrics {
            let obs = self.store.list_observations(m.id).await?;
            observations_by_metric.insert(m.id, obs);
        }
        let issue_stage_counts = self.store.count_issues_by_stage(project_id).await?;
        let handoffs = self.store.list_handoffs(project_id).await?;
        let all_runs = self.store.list_runs(Some(project_id)).await?;

        Ok(view::hex::build(
            project_id,
            &project.name,
            project.active_stage,
            &issue_stage_counts,
            &metrics,
            &observations_by_metric,
            loop_inputs,
            &handoffs,
            &all_runs,
            workspace_evidence,
            now,
        ))
    }
}
