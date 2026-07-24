//! `UnsupportedCliExecutor` — the honest "not installed / not supported yet"
//! stand-in for any `Agent.agent_cli` value other than `"claude-code"`
//! (plan/12 §3, T6). First version ships exactly one real backend
//! ([`crate::ClaudeCliExecutor`]); routing an Agent whose declared CLI
//! ("codex" / "cursor" / …) has none yet to THIS executor reuses the
//! existing [`crate::Executor`] trait seam instead of opening a new one —
//! `bw-app`'s run loop already settles a `workflow_run` row `Failed` with the
//! executor's error text on ANY `Executor::run_phase` error, so no new
//! plumbing is needed to make the failure land honestly in the run record.

use async_trait::async_trait;

use crate::{ExecError, Executor, PhaseNode, PhaseOutput, RunCtx};

/// Errors immediately on every call — never fabricates a success. `cli` is
/// the agent's declared (unsupported) `agent_cli` value, echoed verbatim in
/// the error text so the settled run row reads back exactly which CLI was
/// requested.
pub struct UnsupportedCliExecutor {
    cli: String,
}

impl UnsupportedCliExecutor {
    pub fn new(cli: impl Into<String>) -> Self {
        Self { cli: cli.into() }
    }
}

#[async_trait]
impl Executor for UnsupportedCliExecutor {
    async fn run_phase(&self, _phase: &PhaseNode, _ctx: &RunCtx) -> Result<PhaseOutput, ExecError> {
        Err(ExecError::Failed(format!(
            "本机未安装/暂不支持 {} CLI,当前仅支持 claude-code",
            self.cli
        )))
    }
}
