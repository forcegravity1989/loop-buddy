//! The **executor consistency suite** — the cross-team contract as runnable code.
//!
//! Any [`Executor`] implementation (our [`MockExecutor`](crate::MockExecutor) or
//! the colleague team's real `AnthropicExecutor`) must pass [`check`]. The
//! colleague's test is one line:
//!
//! ```ignore
//! bw_engine::contract::check(&AnthropicExecutor::new()).await.unwrap();
//! ```
//!
//! The contract is deliberately backend-agnostic: it cannot require determinism
//! (a real model isn't deterministic), only the structural invariants the rest
//! of the system relies on.

use crate::{Executor, PhaseNode, RunCtx};
use bw_core::model::PhaseRole;
use bw_core::{ProjectId, WorkflowId};

/// Run a representative phase through `exec` and assert the invariants every
/// executor must hold. Returns `Err(reason)` on the first violation.
pub async fn check<E: Executor>(exec: &E) -> Result<(), String> {
    let ctx = RunCtx {
        project: ProjectId::nil(),
        workflow: WorkflowId::nil(),
    };
    let phase = PhaseNode {
        name: "契约自检".into(),
        role: PhaseRole::Neutral,
        prompt: "确认产出结构合约".into(),
        agents: Vec::new(),
        skills: Vec::new(),
        max_iter: 2,
        retries: 1,
        prior_summary: None,
    };

    let out = exec
        .run_phase(&phase, &ctx)
        .await
        .map_err(|e| format!("run_phase on a well-formed node must succeed, got: {e}"))?;

    if out.text.trim().is_empty() {
        return Err("PhaseOutput.text must be non-empty".into());
    }
    if out.done && !out.gaps.is_empty() {
        return Err("a `done` phase must report no outstanding gaps".into());
    }

    Ok(())
}
