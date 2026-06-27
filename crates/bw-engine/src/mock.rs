//! [`MockExecutor`] — deterministic fake execution. This is what the whole MVP
//! runs on (plan `04 §P1`): UI + engine structure never change between the mock
//! and the colleague team's real executor.

use crate::{ExecError, Executor, PhaseNode, PhaseOutput, RunCtx};
use async_trait::async_trait;

/// Returns a canned, deterministic [`PhaseOutput`] keyed on the phase name —
/// always `done` on the first iteration (no spinning), `text` echoing the phase.
#[derive(Clone, Debug, Default)]
pub struct MockExecutor;

impl MockExecutor {
    pub fn new() -> Self {
        MockExecutor
    }
}

#[async_trait]
impl Executor for MockExecutor {
    async fn run_phase(&self, phase: &PhaseNode, _ctx: &RunCtx) -> Result<PhaseOutput, ExecError> {
        Ok(PhaseOutput {
            text: format!("【mock】阶段「{}」已完成", phase.name),
            done: true,
            gaps: Vec::new(),
        })
    }
}
