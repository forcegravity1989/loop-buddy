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
//! 正式依赖里刻意没有 `bw-engine`:PTY/agentcli 那堆原生依赖不该渗进编排
//! 层,`scripts/guard-app-layering.sh` 在 CI 里守这条线(只查
//! `[dependencies]` 节,查的是 `cargo tree -e normal` 真实依赖图——
//! `bw-engine` 作为 dev-dependency 出现在「真实并行三件」那一档指挥器
//! 里,不受影响)。

mod error;
pub mod run;
pub use error::AppError;

use bw_core::{IssueId, IssueStatus};
use bw_store::{IssueStore, SqliteStore};

/// 编排层的把手。存储层的一个薄壳 + 状态转移的合法性守卫。运行管理器
/// (`run::RunManager`)不经过 `App`——它自己独立开库(design §3.1
/// `RunManager::open` 签名直接收 `db: &Path`),`App` 与 `RunManager` 是
/// 两个各自独立的存储连接持有者,不共享同一个 `SqliteStore` 句柄(SQLite
/// 允许多个连接指向同一个文件,单写者串行化靠文件锁,不靠这里少开一个
/// 连接)。
pub struct App {
    pub store: SqliteStore,
}

impl App {
    /// 装配:开库,拿到存储层把手。**不建任何运行管理器循环任务**——运行
    /// 管理器是独立的 `run::RunManager::open`,不经过 `App::open`。
    pub async fn open(db_path: &str) -> Result<Self, AppError> {
        let store = SqliteStore::open(db_path).await?;
        Ok(Self { store })
    }

    /// 活状态的显式转移——「完成永远由人点;`InReview → Done` 是合法转移
    /// 表里 Done 的唯一入边」这条产品铁律在编排层的落点(design §3.6)。
    /// 流程:①读当前状态 ②查 `bw_core::IssueStatus::can_transition_to`
    /// ③合法才落到 `bw_store::IssueStore::transition_issue_status` 的比较
    /// 并置写。**`RunManager` 的代码路径里没有一处调用这个方法**——它是
    /// 「人点」的那个动作(以及 `run_races` 指挥器演示「完成永远人点」用
    /// 的入口,§5.2),不是运行管理器自动推进的一部分。
    pub async fn transition_issue(&self, id: IssueId, to: IssueStatus) -> Result<(), AppError> {
        let Some(row) = self.store.get_issue(id).await? else {
            return Err(AppError::IssueNotFound(id));
        };
        if !row.status.can_transition_to(to) {
            return Err(AppError::IllegalTransition {
                from: row.status,
                to,
            });
        }
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let wrote = self
            .store
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
}
