//! `bw-store-import` —— next 切片七-1 修复轮:导入专用写入面(design-
//! s7-real-project.md §2.4 第三处硬碰撞的结构解法,task-s7a-review.md
//! Critical-1)。
//!
//! **这个 crate 存在的唯一理由是让 `app-desktop`(壳)结构上够不着导入写
//! 入面**——七A 最初把它做成 `bw-store` 的一个 `import` cargo 特性,2026-
//! 08-11 复审用编译期探针 + 产物比对 + 运行时建表三重实证发现:cargo 的
//! 特性统一在仓库门禁自己跑的 `cargo check/clippy --workspace --all-targets`
//! 下,会把 `bw-app` `[dev-dependencies]` 里那份带 `import` 特性的
//! `bw-store` 与 `app-desktop` 依赖的那份统一编译成**同一份产物**——特性
//! 开关挡不住这件事(cargo 的特性统一规则对同一个 crate、同一个目标平
//! 台、同一次调用里的全部请求方一视同仁,不分 normal/dev 依赖边)。
//!
//! 结构解法:把导入面搬进这个独立 crate。`app-desktop` 的 `Cargo.toml`
//! 里永远不会出现 `bw-store-import` 这一行——没有依赖边就没有特性可统
//! 一,壳的编译图上连这个 crate 的名字都不存在,`use bw_store_import::
//! ImportStore` 在壳的源码里写不出来,不管用哪种 cargo 调用方式都是如
//! 此。`scripts/guard-import-feature-off.sh` 分两档核验这件事:一档查
//! `app-desktop` 自己声明的依赖图(隔离构建档),一档直接复现门禁真实构
//! 建形态、检查产物二进制里有没有这个 crate 的字节痕迹(workspace 统一
//! 构建产物档)。
//!
//! **两条硬约束**(照搬 design §2.4,结构解法不改变约束本身):
//! 1. 建表只在这个 crate 自己的 `ImportSqliteStore::open` 被真正调用时才
//!    发生——见 [`sqlite::ImportSqliteStore::open`] 文档。`bw-store::
//!    SqliteStore::open`(壳唯一会调用的开库路径)完全不知道
//!    `schema_import.sql` 的存在,不是「特性关了就不知道」,是这个 crate
//!    根本不在 `bw-store` 的依赖图里,`bw-store` 编译期看不见这份文件。
//! 2. 这条写入面里不许有任何更新或删除方法,只有插入——观测表的两条数
//!    据库触发器(`trg_observation_no_update`/`trg_observation_no_delete`)
//!    照旧生效,导入器也绕不过(它走的还是同一张 `observation` 表)。
//!
//! **为什么这不算给「完成永远由人点」开后门**:导入写的是"这件活在旧库里
//! 当时就是已完成"这个历史事实,不是一次状态转移——它不经过合法转移表
//! (`bw_core::IssueStatus::can_transition_to`),因为它压根不是转移。这个
//! 区别现在靠**crate 边界**立着(旧版靠特性开关立,已证明立不住):壳的
//! 产物依赖图上没有这个 crate,生产路径上就没有第二条能写"已完成"的路;
//! 这个 crate 只被 `bw-app` 的 `import_legacy` 指挥器(经
//! `[dev-dependencies]`)引用,那是一次性导入,人在命令行上亲自确认过
//! (`--confirm`)。

#![forbid(unsafe_code)]

mod sqlite;
pub use sqlite::ImportSqliteStore;

use async_trait::async_trait;
use bw_core::{
    ArtifactId, HandoffId, IssueId, IssueStatus, MetricId, ObservationId, ProjectId, StageKind,
};
use bw_store::{MetricTier, Result};

/// 旧库一行项目的完整历史形态(design §2.2①)。`active_stage` 是
/// `Option`——签名上允许 `None`,但调用方(导入器)读到的旧库这一列
/// **不可空**,所以真实调用永远是 `Some`;这里不收窄成必填,是因为
/// 「这个方法天生写不出哪个值」不是这个类型该守的边界(那是导入器自
/// 己读旧库时的事实,不是这条写入面的合法性判断——同 `IssueStore`/
/// `RunStore` 一贯的"合法性由调用方决定,这层只管写"分工)。
#[derive(Clone, Debug)]
pub struct ImportProjectRow {
    pub id: ProjectId,
    pub name: String,
    pub root_path: String,
    pub active_stage: Option<StageKind>,
    pub created_at: i64,
}

/// 旧库一行指标(或合成的北极星行)的完整历史形态(design §2.2②③)。
/// `origin` 是裸 `String`("file" | "manual")——同 `bw_store::MetricRow::
/// origin` 的既有形状,不是这里新造的判断。
#[derive(Clone, Debug)]
pub struct ImportMetricRow {
    pub id: MetricId,
    pub project_id: ProjectId,
    pub tier: MetricTier,
    pub name: String,
    pub def: String,
    pub target_raw: String,
    pub collect_kind: String,
    pub collect_query: String,
    pub origin: String,
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 旧库一行观测的完整历史形态(design §2.2④)。`id` 原样沿用旧库编
/// 号——这是幂等语义成立的关键一环(§2.6:重复导入天然安全,靠的就是
/// 编号原样沿用 + 数据库自己拒绝重复主键)。
#[derive(Clone, Debug)]
pub struct ImportObservationRow {
    pub id: ObservationId,
    pub metric_id: MetricId,
    pub project_id: ProjectId,
    pub ts: i64,
    pub raw_value: String,
    pub source: String,
    pub source_hint: String,
    pub created_at: i64,
}

/// 旧库一行活的完整历史形态(design §2.2⑤)。**导入的活按旧库状态原
/// 样搬,不做任何状态推进**——`status` 收 `IssueStatus` 本身(不是像
/// `bw_store::IssueStore::mark_issue_in_progress` 那样窄到写不出别的
/// 值),因为这里就是要把旧库里真实存在的"已完成"这个历史事实原样写进
/// 来,不经过合法转移表(见本文件顶部模块文档"为什么这不算开后门")。
#[derive(Clone, Debug)]
pub struct ImportIssueRow {
    pub id: IssueId,
    pub project_id: ProjectId,
    pub number: i64,
    pub title: String,
    pub status: IssueStatus,
    pub settled_at: Option<i64>,
    pub stage: Option<StageKind>,
    pub body: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 旧库一行交棒的完整历史形态(design §2.2⑦)。
#[derive(Clone, Debug)]
pub struct ImportHandoffRow {
    pub id: HandoffId,
    pub project_id: ProjectId,
    pub from_stage: StageKind,
    pub to_stage: StageKind,
    pub risky: bool,
    pub note: String,
    pub created_at: i64,
}

/// 旧库一行产物登记的完整历史形态(design §2.2⑥,主控裁决 1:导成历
/// 史存档表 `artifact_archive`,与新工程读时现采的交付证据栏分开)。
/// `imported_at` 不在这里——store 自己盖当前时刻,同 `created_at` 在
/// 其它写入面上一贯"由实现取当前时刻"的分工;这条历史行本身没有"导入
/// 时刻"这个概念上的历史值可言,它就是"现在"。
#[derive(Clone, Debug)]
pub struct ImportArtifactRow {
    pub id: ArtifactId,
    pub project_id: ProjectId,
    pub issue_id: Option<IssueId>,
    pub stage: Option<StageKind>,
    pub path: String,
    pub kind: String,
    pub bytes: i64,
    pub git_commit: String,
    pub registered_at: i64,
}

/// 一次真写(`--confirm`)的流水记账输入(design §2.6)。`created_at`
/// 不在这里——store 自己盖当前时刻(这次真写发生的那一刻本身就是历
/// 史,不需要调用方传入)。
#[derive(Clone, Debug)]
pub struct ImportLedgerEntry {
    pub legacy_db_path: String,
    pub legacy_db_fingerprint: String,
    pub project_count: i64,
    pub metric_count: i64,
    pub observation_count: i64,
    pub issue_count: i64,
    pub handoff_count: i64,
    pub artifact_count: i64,
}

/// 导入专用写入面。**只有插入,没有任何更新/删除方法**——这是这条
/// trait 存在的全部理由(§2.4 硬约束 2)。每个 `import_*` 方法返回
/// `bool`:`true` = 这次真插入了一行新的;`false` = 撞了主键,数据库
/// 自己跳过(`INSERT OR IGNORE`)——调用方(导入器)靠这个区分"新增"
/// 与"重复",不需要自己先查一遍存不存在。
#[async_trait]
pub trait ImportStore: Send + Sync {
    async fn import_project(&self, row: ImportProjectRow) -> Result<bool>;
    async fn import_metric(&self, row: ImportMetricRow) -> Result<bool>;
    async fn import_observation(&self, row: ImportObservationRow) -> Result<bool>;
    async fn import_issue(&self, row: ImportIssueRow) -> Result<bool>;
    async fn import_handoff(&self, row: ImportHandoffRow) -> Result<bool>;
    async fn import_artifact(&self, row: ImportArtifactRow) -> Result<bool>;

    /// 只追加的导入流水——每次真写记一行(design §2.6)。不是防重复
    /// 的机制本身(防重复靠上面六个方法各自的 `INSERT OR IGNORE`),
    /// 是让第二次运行能说人话。
    async fn record_import_ledger(&self, entry: ImportLedgerEntry) -> Result<()>;
}
