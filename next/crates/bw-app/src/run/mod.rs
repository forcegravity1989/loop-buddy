//! 运行管理器(design-s4-runmanager.md §3):[`RunManager`] 单口 API(开
//! 工/取消/重启清理/快照),单条命令队列 + 一个循环任务独占内存活跃
//! 表——竞态串行化天然成立,压根没有锁。「降级为咨询」(design §3.5)
//! 是切片四C 的事,本片(切片四B)不预置。
//!
//! 十件并行 + 五个竞态(同一件活开不出第二个交付运行 / 取消与完成同时
//! 到达只结算一次 / 单条失败不牵连 / 重启后遗留运行如实标注不假活 / 晚
//! 到消息不错账)的确定性复现指挥器是切片四D 的事(`bw-app/examples/
//! run_races.rs`),不写单元测试(本仓核心纪律)。

mod manager;
mod types;

pub use manager::RunManager;
pub use types::{ReapReport, RunError, RunManagerConfig, RunSnapshot, StartRun};
