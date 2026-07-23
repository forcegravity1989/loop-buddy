//! [`MockExecutor`] — deterministic fake execution. This is what the whole MVP
//! runs on (plan `04 §P1`): UI + engine structure never change between the mock
//! and the colleague team's real executor.

use crate::{ExecError, Executor, PhaseNode, PhaseOutput, RunCtx};
use async_trait::async_trait;
use bw_core::model::PhaseRole;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

/// Returns a canned, deterministic [`PhaseOutput`] keyed on the phase name —
/// always `done` on the first iteration (no spinning). By default a plain phase
/// echoes its name; an **Evaluator** phase additionally renders a `VERDICT: PASS`
/// block so a workflow with a review gate still completes on the mock (no
/// fabricated stall) — T9.
///
/// An optional per-phase delay simulates real execution latency so the desktop
/// shell can prove its live progress stream (phase-by-phase, not one burst).
///
/// For T9 E2E orchestration the mock can be **scripted**: [`MockExecutor::scripted`]
/// queues exact output texts per phase name, consumed in order across calls, so a
/// test can drive the adversarial loop deterministically (round-1 打回, round-2
/// 通过, or a malformed verdict block to exercise honest parse failure). A phase
/// with no (or a drained) script entry falls back to the default behavior above.
#[derive(Debug, Default)]
pub struct MockExecutor {
    delay: Duration,
    // Scripted per-phase outputs, consumed FIFO. Interior mutability because
    // `Executor::run_phase` takes `&self` (the engine holds an `Arc<dyn>`); the
    // guard is never held across an `.await`.
    script: Mutex<HashMap<String, VecDeque<String>>>,
}

impl MockExecutor {
    pub fn new() -> Self {
        MockExecutor::default()
    }

    /// A mock that takes `delay` per phase (visible streaming in the UI).
    pub fn with_delay(delay: Duration) -> Self {
        MockExecutor {
            delay,
            script: Mutex::new(HashMap::new()),
        }
    }

    /// A **scripted** mock (T9 E2E). Each `(phase_name, outputs)` entry queues the
    /// exact texts the mock returns for successive calls to that phase — used to
    /// drive the adversarial review loop deterministically. Texts should be
    /// self-labelled `【mock】` by the caller (E2E discipline). A phase not in the
    /// script, or one whose queue is drained, falls back to the default echo /
    /// default-PASS behavior.
    pub fn scripted(entries: Vec<(String, Vec<String>)>) -> Self {
        let map = entries
            .into_iter()
            .map(|(name, outs)| (name, outs.into_iter().collect()))
            .collect();
        MockExecutor {
            delay: Duration::ZERO,
            script: Mutex::new(map),
        }
    }
}

#[async_trait]
impl Executor for MockExecutor {
    async fn run_phase(&self, phase: &PhaseNode, _ctx: &RunCtx) -> Result<PhaseOutput, ExecError> {
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        // Scripted output takes precedence (owned String popped out; the lock is
        // released before we build the result — never held across an await).
        let scripted = self
            .script
            .lock()
            .expect("mock script mutex poisoned")
            .get_mut(&phase.name)
            .and_then(|q| q.pop_front());
        if let Some(text) = scripted {
            return Ok(PhaseOutput {
                text,
                done: true,
                gaps: Vec::new(),
            });
        }
        let text = if phase.role == PhaseRole::Evaluator {
            format!(
                "【mock】阶段「{}」评审完成\nVERDICT: PASS\nREASON: 【mock】默认通过(流程演示)",
                phase.name
            )
        } else {
            format!("【mock】阶段「{}」已完成", phase.name)
        };
        Ok(PhaseOutput {
            text,
            done: true,
            gaps: Vec::new(),
        })
    }
}
