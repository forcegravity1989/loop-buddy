//! 运行管理器的公开类型:入参 [`StartRun`]、配置 [`RunManagerConfig`]、
//! 错误 [`RunError`]、两个读操作的返回形状 [`ReapReport`]/[`RunSnapshot`]
//! (design-s4-runmanager.md §3.1)。

use std::path::PathBuf;
use std::time::Duration;

use bw_connector::InjectBlock;
use bw_core::{IssueId, IssueStatus, ProjectId, RunId};
use bw_store::StoreError;

/// 开工要给的最小信息。**没有 `kind` 字段**——`RunManager::start` 永远只
/// 创建交付运行;「咨询」只从既有交付运行降级而来(`demote_to_consultation`,
/// design §3.5),不是一条独立的创建路径。**没有 `project` 字段**——项目
/// 由 `issue` 反查(`IssueRow.project_id`),不接受调用方另指定一个可能对
/// 不上的值。
pub struct StartRun {
    pub issue: IssueId,
    /// 哪条执行连接器。`None` 的解析规则照 registry 里 `issue_ops` 的先
    /// 例:恰好一条 → 用它;零条 → 如实报未连接;多条 → 如实报无法判
    /// 定,**绝不「取第一条」蒙混**(design §3.1)。
    pub connector_name: Option<String>,
    pub workspace: PathBuf,
    pub branch: String,
    pub inject: Vec<InjectBlock>,
    pub budget_usd: Option<f64>,
}

/// 运行管理器的节奏配置。目前只有一项——轮询节奏该不该由连接器自己声明
/// 是 design §11 开放问题 1,本片钉一个全局默认,不预先给这个结构体加还
/// 没有消费者的字段。
#[derive(Clone, Copy, Debug)]
pub struct RunManagerConfig {
    /// 每个运行的轮询任务两次 `Execute::poll` 之间睡多久。默认 250ms
    /// (design §3.3)。
    pub poll_interval: Duration,
}

impl Default for RunManagerConfig {
    fn default() -> Self {
        Self {
            poll_interval: Duration::from_millis(250),
        }
    }
}

/// `RunManager::reap_on_restart` 的返回——收拾了哪些运行编号(design
/// §3.1「返回收拾了几条」;给的是 `Vec`,调用方要计数就是 `.reaped.len()`,
/// 要逐条核对读回就是这份清单本身,信息量只多不少)。
#[derive(Debug, Clone)]
pub struct ReapReport {
    pub reaped: Vec<RunId>,
}

/// `RunManager::snapshot` 的返回——聚合计数,不是一百行明细(design
/// §3.1)。**刻意保持最小**:本片只保证这条读接口存在且如实,没有把它做
/// 成一个功能完备的查询面——`run_races` 指挥器的逐运行断言全部走独立的
/// SQL 读回(§5.2),不依赖这个类型的字段够不够用。
///
/// **next 切片五C 加字段**(design-s5-hexpanel.md §1.2 第④段「当前 Loop」
/// 点名的四个数,每个都有真实消费者——`bw-app::view::hex` 的 Loop 段渲染
/// 这四个数,`hex_readback` 指挥器逐个读回核对):
/// - `running` 仍然来自循环独占的内存活跃表(原 `active` 字段改名,更贴
///   合它现在的定位——「在跑」是四个数里唯一一个不查库的,其余三个天生
///   要问 `bw-store`,查库这件事对这个方法不再是新鲜事);
/// - `in_review`/`recent_failed`/`unsettled` 三个走
///   [`bw_store::IssueStore::list_issues_by_status`]/
///   [`bw_store::RunStore::count_failed_runs`]/
///   [`bw_store::RunStore::list_unsettled_runs`]——真实查库,不是猜的。
#[derive(Debug, Clone)]
pub struct RunSnapshot {
    pub project: Option<ProjectId>,
    /// 在跑:循环内存活跃表里,当前这个筛选范围内还「活着」(starting/
    /// running,含交付 + 咨询)的运行数。
    pub running: usize,
    /// 评审中:这个筛选范围内状态为 `InReview` 的活数——「完成」的候诊
    /// 室有多满(design §1.2④)。
    pub in_review: usize,
    /// 最近失败:`state = Failed` 的运行数(原始计数,**不叠加**「没有更
    /// 晚运行接手」那条收窄——那是待人处理③「停在原地的运行」专属的更
    /// 严格判据,两个数字故意不同)。
    pub recent_failed: usize,
    /// 遗留未结账:关了门但没结账(`ended_at IS NOT NULL AND settled_at
    /// IS NULL`)的运行数——待人处理①同一条判据的聚合版。
    pub unsettled: usize,
}

/// 运行管理器公开方法的统一错误面。**不包含任何写 Done 的分支**——运行
/// 管理器压根没有改活状态到 Done 的代码路径(design §3.6/§7),这条错误
/// 枚举同样如实反映这一点。
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Store(#[from] StoreError),

    #[error("这件活已经有一个交付运行在跑(运行 {existing:?})")]
    AlreadyRunning { existing: RunId },

    #[error("活不存在:{0:?}")]
    IssueNotFound(IssueId),

    #[error("活当前状态不允许开工:{status:?}(只有待办池/待办/进行中能开工)")]
    NotStartable { status: IssueStatus },

    #[error("工作区 {workspace} 已经有别的运行在用(运行 {existing:?})")]
    WorkspaceBusy { workspace: PathBuf, existing: RunId },

    #[error("项目没有登记支持执行能力的连接器")]
    NoExecutor,

    #[error("项目登记了 {names:?} 共 {n} 条执行连接器,未指名时无法判定用哪条")]
    AmbiguousExecutor { names: Vec<String>, n: usize },

    #[error("指定的执行连接器 {name:?} 未登记,或不支持执行能力")]
    UnknownExecutor { name: String },

    #[error("运行不存在:{0:?}")]
    RunNotFound(RunId),

    #[error("运行管理器已关闭(命令队列已断)")]
    Closed,
}
