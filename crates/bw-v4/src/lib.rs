//! `bw-v4` —— V4 内核。
//!
//! # 为什么另起一个 crate
//!
//! V4 把库从 20 张表砍到 4 张(仓是正本,库只放定位与显示缓存),
//! `bw-store` 的 20 表 schema 与 `bw-app` 建立在其上的用例整体接不上了。
//! 与其在旧 crate 里同时维护两套互斥的数据模型,不如新开一层:V4 自己的
//! 库、自己的仓文件解析器、自己的推导与命令总线。旧的五个 crate 一行不动,
//! 旧壳 `app-desktop` 继续编译、继续可跑。
//!
//! **复用的是**:`bw-core` 的身份类型、`Signal`、活的状态机
//! (`can_transition_to` —— 「完成」的唯一入边是「评审中」这条铁律就锁在
//! 那里)、五阶段元数据(V4 降级成活的类别标签);`bw-engine` 的交互式执行器、
//! 工作区/worktree 供给、git 读数、`.bw/metrics.toml` 解析。
//!
//! # 分层
//!
//! - [`model`] —— V4 的领域类型(零 IO)。
//! - [`store`] —— 四张表的哑存储,不做业务判断。
//! - [`derive`] —— 健康三判据的现算,带密封类型。
//!
//! 仓文件解析(`repo`)、命令总线(`command`)、编排(`app`)随后几步加进来。

#![forbid(unsafe_code)]

pub mod derive;
pub mod git;
pub mod isoweek;
pub mod model;
pub mod repo;
pub mod store;

pub use model::{Issue, IssueKind, IssueOrigin, IssueStatus, Project, Signal};
pub use store::{V4Store, DEFAULT_DB_FILENAME};
