//! 九屏 —— 顶层三屏(项目墙 / 接入 / 设置)+ 项目内六入口。
//!
//! 这是**唯一知道「有哪些屏」的地方**。每个屏目录只对外暴露一个 `view` 函数,
//! 屏与屏之间不许互相引用(`scripts/guard-no-cross-screen-import.sh` 守着);
//! 要共享数据就经 `crate::vm` 的 ViewModel,或者经命令/事件绕一圈。

pub mod config;
pub mod kb;
pub mod notify;
pub mod onboard;
pub mod overview;
pub mod plan;
pub mod session;
pub mod settings;
pub mod wall;

use crate::bridge::{Bridge, Panel};
use crate::vm::ProjectVm;
use dioxus::prelude::*;

/// 六入口的分发。顶层三屏不在这里 —— 它们不依赖「打开了某个项目」。
///
/// **每个屏必须是组件(`#[component]`),不能是普通函数**:普通函数的
/// `use_signal` 会落在**调用方**的 hook 表上,按序号取。切一次屏,序号还在、
/// 类型换了,Dioxus 取 hook 时 downcast 失败直接 panic(接入 → 计划 → 配置
/// 这条路必炸)。做成组件,每屏有自己的作用域,序号各算各的。
pub fn route(panel: Panel, project: &ProjectVm, bridge: &Bridge) -> Element {
    let p = project.clone();
    let b = bridge.clone();
    match panel {
        Panel::Overview => rsx! { overview::View { p, bridge: b } },
        Panel::Plan => rsx! { plan::View { p, bridge: b } },
        Panel::Session => rsx! { session::View { p, bridge: b } },
        Panel::Notify => rsx! { notify::View { p, bridge: b } },
        Panel::Config => rsx! { config::View { p, bridge: b } },
        Panel::Kb => rsx! { kb::View { p, bridge: b } },
    }
}
