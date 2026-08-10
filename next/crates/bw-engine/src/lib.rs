//! `bw-engine` — 移植骨架(next 切片一B)。原本四个模块(evidence /
//! metrics_file / terminal_manager / interactive_cli)的正文逐字节来自
//! `origin/v1`,零改写;本文件是新写的根定义,只搬运这些模块依赖的两个根
//! 类型(`RunCtx` / `ExecError`,逐字复制自
//! `origin/v1:crates/bw-engine/src/lib.rs` 第 92-97 行、107-111 行)与模块
//! 声明,不含其余 v1 根类型(`Executor`/`PhaseOutput`/`MockExecutor` 等留
//! 给后续切片接线时再搬)。
//!
//! **next 切片五A**:`evidence`/`metrics_file` 两个模块**换落点**到
//! `bw-workspace`(design-s5-hexpanel.md §6.2/§9)——它们在这里从移植进来
//! 那天起就是零消费者(编排层要用,但编排层依赖不到这个 crate,分层门禁
//! 挡着);`bw-workspace` 是它们第一次有真实消费者的地方。本 crate 反过来
//! 依赖 `bw-workspace` 复用它们(`examples/port_readback.rs` 就是这条复用
//! 的调用点),不再自己拥有这两个模块。

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

// agentcli 层(next 切片三C/D,design-s3-agentcli.md §1)。落点定案:不新开
// crate,装在这里(PTY 依赖已经在 bw-engine)。见模块文档。
pub mod agentcli;
// 原有的 `#[allow(dead_code)]` 豁免(macOS 曾经走 osascript 分支,不引用
// `InteractiveCliExecutor::timeout`/`::await_child`,触发 dead_code)在
// next 切片三-1 修删掉 osascript 分支后已经不再需要——macOS 现在与
// Linux/Windows 一样调用 `await_child`,`cargo clippy --all-targets
// -D warnings` 复核过豁免摘掉后仍全绿,不留不必要的 allow。
pub mod interactive_cli;
// PTY 平台接缝(next 切片三B,从 `interactive_cli.rs` 的 `run_skill_pty`
// 提取)。见模块文档:Windows 实现整段搬运,Unix 实现是本片新写。
pub mod pty_backend;
pub mod terminal_manager;
