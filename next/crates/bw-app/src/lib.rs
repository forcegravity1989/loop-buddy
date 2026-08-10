//! `bw-app` — next 切片四:编排层(design-s4-runmanager.md §1)。
//!
//! 运行管理器(开工/取消/结束回写/重启清理)住在这里——它要写库、要判
//! 状态机,这是编排层的活;放进 `bw-engine` 会把「引擎只管对外能力的实
//! 现,不管编排判断」这条边界揉回旧工程结构病里去。
//!
//! **切片四A** 只立了骨架:[`App`] 把存储层装起来。**切片四B** 在此之上
//! 把运行管理器本体接起来(见 [`run`] 模块——`RunManager` 单口 API:开
//! 工/取消/重启清理/快照,§3)。「降级为咨询」(design §3.5)是切片四C
//! 的事,`RunManager` 的公开 API 会在那时候补上
//! `demote_to_consultation`,本片不预置。
//!
//! 正式依赖里刻意没有 `bw-engine`:PTY/agentcli 那堆原生依赖不该渗进编排
//! 层,`scripts/guard-app-layering.sh` 在 CI 里守这条线(只查
//! `[dependencies]` 节,查的是 `cargo tree -e normal` 真实依赖图——
//! `bw-engine` 将来作为 dev-dependency 出现在「真实并行三件」那一档指挥
//! 器里,不受影响)。

mod error;
pub mod run;
pub use error::AppError;

use bw_store::SqliteStore;

/// 编排层的把手。目前只是「存储层的一个薄壳」——运行管理器
/// (`run::RunManager`)不经过 `App`,它自己独立开库(design §3.1
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
}
