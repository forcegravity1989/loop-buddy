//! 运行管理器(design-s4-runmanager.md §3/§4):[`RunManager`] 单口 API
//! (开工/取消/降级为咨询/重启清理/快照),单条命令队列 + 一个循环任务独
//! 占内存活跃表——竞态串行化天然成立,压根没有锁。
//!
//! 十件并行 + 五个竞态(同一件活开不出第二个交付运行 / 取消与完成同时
//! 到达只结算一次 / 单条失败不牵连 / 重启后遗留运行如实标注不假活 / 晚
//! 到消息不错账)的确定性复现指挥器是切片四D 的事(`bw-app/examples/
//! run_races.rs`),不写单元测试(本仓核心纪律)。

mod manager;
mod types;

pub use manager::RunManager;
pub use types::{ReapReport, RunError, RunManagerConfig, RunSnapshot, RunStateChanged, StartRun};

/// next 切片 5.5(design-s5-hexpanel.md §5.3):`RunManager::terminal_input`
/// 的入参类型——重导出自 `bw-connector`(契约层的正本),不是 `bw-app` 自
/// 己发明的类型。壳(`app-desktop`)拼键盘/尺寸事件时从这里取,不需要单
/// 独再依赖一次 `bw_connector::caps`。
pub use bw_connector::caps::TermInput;
