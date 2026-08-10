//! 各家适配器的 feature 门。`gh`/`codehub` 两家在 next 切片二B 真实收编
//! (v1 `github.rs`/`codehub.rs` 整体搬过来,见 `crate::upstream`);`script`
//! 在 next 切片二C 真实收编(新写,无对应的 v1 冻结自由函数可整体搬,见
//! `script.rs` 模块文档)。`contract.rs`/`caps.rs`/`registry.rs` 里不出现
//! 任何一家的名字:注册表存的是 `Arc<dyn Connector>`,不是枚举 arm——删掉
//! 某一家不需要动注册表一个字符,这条边界因此落在本文件的 [`from_entry`]
//! 里,不落在那三个文件里。

#[cfg(feature = "codehub")]
pub mod codehub;
#[cfg(feature = "gh")]
pub mod gh;
#[cfg(feature = "script")]
pub mod script;
#[cfg(feature = "script")]
pub mod script_source;

/// 登记工厂(brief 要求:「两家的构造函数(from ConnectorEntry)接入注册表
/// 工厂」)。composition root(桌面壳 / headless 指挥器)拿到一份
/// `Vec<ConnectorEntry>`(项目行 provider/remote_host/remote_path 转来的
/// 登记)时,不用自己按 `kind` 手写 match 再分别 `use` 三家的类型——这一个
/// 函数按 `entry.kind` 分派,按 feature 收敛「这个 kind 该构造哪个适配器
/// 类型」这件事,composition root 只管调 `ConnectorRegistry::register`。
///
/// 某个 kind 对应的 feature 没开(或者 kind 本来就不该走这条工厂,比如
/// `Script`/`AgentCli`——它们各自有自己的构造路径,不经这里),返回
/// `None`;composition root 自己决定「找不到工厂」算不算错,这里不代它拍板
/// (同「Ambiguous 不取第一条蒙混」的谨慎口径)。
///
/// **git-repo 种类明确不出现**(裁决 #2):本地 git 读写是内建工作区/采证
/// 函数,不是连接器,`ConnectorKind` 里没有、也不该有一个泛化的
/// `GitRepo`——只有 `GithubRepo`/`CodehubRepo` 这两个具体厂商的仓连接器
/// 种类会走到这个工厂里。
///
/// **不校验 `entry.config` 与 `kind` 配套**(config 现阶段整体未被 gh/codehub
/// 消费,见 [`crate::contract::ConfigRef`] 文档);错配登记不会报错。
pub fn from_entry(
    entry: &crate::contract::ConnectorEntry,
) -> Option<std::sync::Arc<dyn crate::caps::Connector>> {
    match &entry.kind {
        #[cfg(feature = "gh")]
        crate::contract::ConnectorKind::GithubRepo => Some(gh::GhConnector::from_entry(entry)),
        #[cfg(feature = "codehub")]
        crate::contract::ConnectorKind::CodehubRepo => {
            Some(codehub::CodehubConnector::from_entry(entry))
        }
        _ => None,
    }
}
