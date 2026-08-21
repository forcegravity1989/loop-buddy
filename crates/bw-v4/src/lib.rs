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
//! **不依赖任何 V3 crate**(2026-08-21 起,`cargo tree` 读回为 0):领域类型
//! (身份、`Signal`、活的状态机、五类别)自持在 [`model`];干活的能力走
//! `v4-engine`(执行器、worktree、git、远端),那是从 `bw-engine` 拷过来接管
//! 的一份,不是复用。

#![forbid(unsafe_code)]

pub mod app;
pub mod chat;
pub mod command;
pub mod derive;
pub mod git;
pub mod isoweek;
pub mod model;
pub mod repo;
pub mod standard;
pub mod store;
pub mod trend;

pub use command::{Command, Event};
pub use model::{Issue, IssueKind, IssueOrigin, IssueStatus, Project, Signal};
pub use store::{V4Store, DEFAULT_DB_FILENAME};
