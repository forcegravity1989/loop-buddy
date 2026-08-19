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
pub fn route(panel: Panel, project: &ProjectVm, bridge: &Bridge) -> Element {
    match panel {
        Panel::Overview => overview::view(project, bridge),
        Panel::Plan => plan::view(project, bridge),
        Panel::Session => session::view(project),
        Panel::Notify => notify::view(project, bridge),
        Panel::Config => config::view(project, bridge),
        Panel::Kb => kb::view(project, bridge),
    }
}
