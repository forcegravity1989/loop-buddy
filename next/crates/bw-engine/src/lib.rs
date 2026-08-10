//! `bw-engine` — 移植骨架(next 切片一B)。四个模块(evidence / metrics_file /
//! terminal_manager / interactive_cli)的正文逐字节来自 `origin/v1`,零改写;
//! 本文件是新写的根定义,只搬运这四个模块唯一依赖的两个根类型
//! (`RunCtx` / `ExecError`,逐字复制自 `origin/v1:crates/bw-engine/src/lib.rs`
//! 第 91-96 行、106-110 行)与模块声明,不含其余 v1 根类型(`Executor`/
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
pub mod interactive_cli;
pub mod metrics_file;
pub mod terminal_manager;
