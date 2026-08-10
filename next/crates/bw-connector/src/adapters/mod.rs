//! 各家适配器的 feature 门。骨架阶段(next 切片二A)只立模块声明与 feature
//! 边界,三个占位模块各自只有一句「未建」的说明——真实收编(v1 `github.rs` /
//! `codehub.rs` / `connectors_file.rs` 整体搬过来)是下一任务(切片二B)。
//! `contract.rs` / `caps.rs` / `registry.rs` 里不出现任何一家的名字:注册表
//! 存的是 `Arc<dyn Connector>`,不是枚举 arm——删掉某一家不需要动注册表一个
//! 字符。

#[cfg(feature = "codehub")]
pub mod codehub;
#[cfg(feature = "gh")]
pub mod gh;
#[cfg(feature = "script")]
pub mod script;
