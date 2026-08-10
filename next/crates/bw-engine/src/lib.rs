//! `bw-engine` — 移植骨架(next 切片一B)。四个模块(evidence / metrics_file /
//! terminal_manager / interactive_cli)的正文逐字节来自 `origin/v1`,零改写;
//! 本文件是新写的根定义,只搬运这四个模块唯一依赖的两个根类型
//! (`RunCtx` / `ExecError`,逐字复制自 `origin/v1:crates/bw-engine/src/lib.rs`
//! 第 92-97 行、107-111 行)与模块声明,不含其余 v1 根类型(`Executor`/
//! `PhaseOutput`/`MockExecutor` 等留给后续切片接线时再搬)。

use bw_core::{ProjectId, WorkflowId};

/// Context handed to an executor for a run.
#[derive(Clone, Copy, Debug)]
pub struct RunCtx {
    pub project: ProjectId,
    pub workflow: WorkflowId,
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("executor failed: {0}")]
    Failed(String),
}

pub mod evidence;
// v1 源码跨平台既有告警:`InteractiveCliExecutor::timeout` / `::await_child`
// 只在 Windows/Linux 的 cfg 分支被引用,macOS 下不被引用而触发 dead_code。
// 零改写约束下无法逐项 #[allow],豁免收在本模块;切片三把该模块接线进
// run_skill 后复查收窄。
#[allow(dead_code)]
pub mod interactive_cli;
pub mod metrics_file;
// PTY 平台接缝(next 切片三B,从 `interactive_cli.rs` 的 `run_skill_pty`
// 提取)。见模块文档:Windows 实现整段搬运,Unix 实现是本片新写。
pub mod pty_backend;
pub mod terminal_manager;
