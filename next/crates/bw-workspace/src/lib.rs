//! `bw-workspace` —— next 切片五A:本地工作区能力的正经的家
//! (design-s5-hexpanel.md §6)。清偿两处登记在案的命名将就:plan/23 §10
//! 第 7 条(工作区供给没落点)与切片三开放问题 4(git 辅助住在连接器 crate
//! 里当「内建函数」)。
//!
//! 收三类东西,**都是「对本地工作区做的事」,都不是对外连接**(§6.2):
//!
//! - [`provision`] —— 造一棵活的、按活分支切好的 git 工作树
//!   (`provision_issue_worktree`,从 v1 搬;**不搬**「作用域结束自动删」
//!   的 `Drop` 守卫——供给只造不删,理由见模块文档)。
//! - [`git_support`] —— 三个 git shell-out 辅助 + 共用错误类型
//!   (`git_in`/`commit_initial`/`stage_commit_push`/`ProvisionError`),从
//!   `bw-connector` 移过来的单副本;`bw-connector` 反过来依赖这个 crate。
//! - [`evidence`] / [`metrics_file`] —— 从 `bw-engine` **换落点**的两个移
//!   植件(采证 / 项目正本文件解析),函数体零改写。它们在 `bw-engine`
//!   里从移植进来那天起就是零消费者;换落点的理由是它们第一次有了真实
//!   消费者(编排层),而编排层依赖不到 `bw-engine`(分层门禁挡着)。
//!
//! **为什么它不是连接器**:本地 git 读写是内建工作区函数,不是「对外借
//! 力」——`ConnectorKind` 里没有、也不该有一个泛化的 `GitRepo`(design §3
//! 已经把这条写死)。
//!
//! **方向单一,谁都能依赖它,它谁也不依赖**:不依赖 `bw-connector` / 编排
//! 层 `bw-app`(design §10.1 第 2 条,`scripts/guard-app-layering.sh` 新增
//! 的两条边守这条线)。目前也不依赖 `bw-core`——这四类东西现在都不触碰任
//! 何内核类型(`provision_issue_worktree` 接的是裸 `u32`/`&Path`,`evidence`/
//! `metrics_file` 更是连 io 之外什么都不碰),加一条用不到的依赖不如不加。
//!
//! - [`metric_shape`] —— next 切片五B 新写(design §1.3/§2.2):把
//!   [`metrics_file::MetricsFile`] 的三种形状(`north_star` 单表 +
//!   `lagging`/`leading` 数组表)归一成同一张表的三种层级,供 `bw-store`
//!   的 `metric` 表消费。这不是移植件,是新写的归一逻辑——落在这里而不
//!   是 `bw-store`,是因为它只需要已解析好的 `MetricsFile`,不碰任何存
//!   储层类型,`bw-store` 因此不需要为了消费它反过来依赖这个 crate。
//! - [`metrics_lenient`] —— next 切片五-1 修复轮新写(评审 Important-4):
//!   `metrics_file::MetricsFile.north_star` 不是 `Option`,`.bw/metrics.toml`
//!   缺 `[north_star]` 一节时移植件会整份解析失败。这个文件在移植件外
//!   面单独包一层,`north_star` 缺节时如实降级为 `None`,让滞后/引领照
//!   常同步——移植件本体(`metrics_file.rs`)零改写。

pub mod evidence;
pub mod git_support;
pub mod metric_shape;
pub mod metrics_file;
pub mod metrics_lenient;
pub mod provision;

pub use git_support::{commit_initial, git_in, stage_commit_push, ProvisionError};
pub use metric_shape::{flatten, flatten_lenient, FlatMetricDef, MetricTier};
pub use metrics_lenient::{read_lenient, LenientMetricsFile};
pub use provision::{issue_branch, provision_issue_worktree};
