//! 一个外部能力一个适配模块。
//!
//! 每个目录里必须有一份 `README.md`,固定三段:**借自哪个项目/文件**、
//! **借了什么**、**没借什么**。这样以后持续借鉴也不会变成散弹式修改。
//!
//! 已经开出来的:
//!
//! - [`claude_cli`] —— Claude CLI 开工工具(声明 + 探活)
//! - [`chat_group`] —— 项目群工厂(只有 trait 与「没配群」的实现)
//!
//! 还没开的(各自随对应的刀落地):嵌入终端(xterm + PTY)、Cursor、
//! Open Design 内嵌、代码图。

pub mod chat_group;
pub mod claude_cli;
