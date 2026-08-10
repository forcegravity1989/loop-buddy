//! `bw-app` — next 切片四:编排层(design-s4-runmanager.md §1)。
//!
//! 运行管理器(开工/取消/结束回写/重启清理)住在这里——它要写库、要判
//! 状态机,这是编排层的活;放进 `bw-engine` 会把「引擎只管对外能力的实
//! 现,不管编排判断」这条边界揉回旧工程结构病里去。
//!
//! **本片(切片四A)只立骨架**:[`App`] 负责把存储层装起来,[`AppError`]
//! 是编排层自己的错误类型(目前只包一层 [`bw_store::StoreError`])。运行
//! 管理器本体(命令队列 + 循环任务 + 五竞态)是下一任务的事——见
//! `design-s4-runmanager.md` §3/§4。
//!
//! 正式依赖里刻意没有 `bw-engine`:PTY/agentcli 那堆原生依赖不该渗进编排
//! 层,`scripts/guard-app-layering.sh` 在 CI 里守这条线(只查
//! `[dependencies]` 节,`bw-engine` 将来作为 dev-dependency 出现在「真实并
//! 行三件」那一档指挥器里是合法的)。

mod error;
pub use error::AppError;

use bw_store::SqliteStore;

/// 编排层的把手。目前只是「存储层的一个薄壳」——运行管理器落地后,这里
/// 会长出命令队列的发信端与收尾守卫(design §3.1 `RunManager`)。
pub struct App {
    pub store: SqliteStore,
}

impl App {
    /// 装配:开库,拿到存储层把手。**不建任何运行管理器循环任务**——那是
    /// 下一任务 `App::open` 演进后的事;本片的 `open` 只做存储装配这一半。
    pub async fn open(db_path: &str) -> Result<Self, AppError> {
        let store = SqliteStore::open(db_path).await?;
        Ok(Self { store })
    }
}
