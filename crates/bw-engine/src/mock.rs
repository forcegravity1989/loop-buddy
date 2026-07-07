//! [`MockExecutor`] — deterministic fake execution. This is what the whole MVP
//! runs on (plan `04 §P1`): UI + engine structure never change between the mock
//! and the colleague team's real executor.

use crate::{ExecError, Executor, PhaseNode, PhaseOutput, RunCtx};
use async_trait::async_trait;
use std::time::Duration;

/// Returns a canned, deterministic [`PhaseOutput`] keyed on the phase name —
/// always `done` on the first iteration (no spinning), `text` echoing the phase.
///
/// An optional per-phase delay simulates real execution latency so the desktop
/// shell can prove its live progress stream (phase-by-phase, not one burst).
/// Tests use the zero-delay default and stay instant.
#[derive(Clone, Debug, Default)]
pub struct MockExecutor {
    delay: Duration,
}

impl MockExecutor {
    pub fn new() -> Self {
        MockExecutor::default()
    }

    /// A mock that takes `delay` per phase (visible streaming in the UI).
    pub fn with_delay(delay: Duration) -> Self {
        MockExecutor { delay }
    }
}

#[async_trait]
impl Executor for MockExecutor {
    async fn run_phase(&self, phase: &PhaseNode, _ctx: &RunCtx) -> Result<PhaseOutput, ExecError> {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(PhaseOutput {
            text: format!("【mock】阶段「{}」已完成", phase.name),
            done: true,
            gaps: Vec::new(),
        })
    }
}
