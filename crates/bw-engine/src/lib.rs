//! `bw-engine` — workflow execution engine.
//!
//! A workflow is a sequence of phases driven by a swappable [`Executor`]. We
//! ship a deterministic [`MockExecutor`]; the real `AnthropicExecutor` is a
//! *colleague team*'s job, plugged in through the same trait (Tier C). The trait
//! together with [`PhaseNode`] / [`PhaseOutput`] / [`RunEvent`] is the **frozen
//! cross-team contract** (plan `00 §9`): a real impl that passes
//! [`contract::check`] is hot-swappable for the mock with zero changes upstream.

#![forbid(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use bw_core::model::{AgentRef, SkillRef, WorkflowSpec};
use bw_core::{ProjectId, WorkflowId};

pub mod claude_cli;
pub mod contract;
pub mod evidence;
pub mod git_log;
mod mock;
pub mod workspace;

pub use claude_cli::{ClaudeCliConfig, ClaudeCliExecutor, PermissionMode};
pub use evidence::{EvidenceError, WorkspaceEvidence, WorkspaceFile};
pub use git_log::{read_commits, GitCommit, GitLogError};
pub use mock::MockExecutor;
pub use workspace::{provision_git_workspace, ProvisionError};

/// One executable phase, built from a [`WorkflowSpec`] phase.
#[derive(Clone, Debug)]
pub struct PhaseNode {
    pub name: String,
    pub prompt: String,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub max_iter: u8,
    pub retries: u8,
    /// Tail of the *previous* phase's real output — the relay baton, so a
    /// stateless per-phase executor still sees what the phase before it
    /// actually produced. `None` on the first phase.
    pub prior_summary: Option<String>,
}

/// The product of running one phase. `done` ends the phase loop; `gaps` feed the
/// next iteration when not done.
#[derive(Clone, Debug)]
pub struct PhaseOutput {
    pub text: String,
    pub done: bool,
    pub gaps: Vec<String>,
}

/// Context handed to an executor for a run.
#[derive(Clone, Copy, Debug)]
pub struct RunCtx {
    pub project: ProjectId,
    pub workflow: WorkflowId,
}

/// The swappable execution backend. **This is the frozen contract.** `Send +
/// Sync` so a run can be driven from any task; async because the real impl does
/// IO (Anthropic API / a Claude Code subprocess).
#[async_trait]
pub trait Executor: Send + Sync {
    async fn run_phase(&self, phase: &PhaseNode, ctx: &RunCtx) -> Result<PhaseOutput, ExecError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    #[error("executor failed: {0}")]
    Failed(String),
}

/// Append-only run events, surfaced to the app as a phase progresses.
#[derive(Clone, Debug)]
pub enum RunEvent {
    PhaseStarted { idx: usize, name: String },
    PhaseCompleted { idx: usize, output: PhaseOutput },
    WorkflowDone { summary: RunSummary },
    WorkflowFailed { error: String },
}

#[derive(Clone, Debug, Default)]
pub struct RunSummary {
    pub phases_run: usize,
    pub final_output: String,
}

/// Drives a [`WorkflowSpec`] through its phases using one [`Executor`].
///
/// Holds a type-erased `Arc<dyn Executor>` (not generic) so the caller can
/// pick a backend per call — e.g. `bw-app` builds a fresh [`Engine`] around a
/// [`ClaudeCliExecutor`] for a configured project, and reuses one long-lived
/// [`Engine`] around [`MockExecutor`] otherwise. `#[async_trait]`'s expansion
/// already boxes the futures, so `Executor` is dyn-safe with no wrapper enum.
pub struct Engine {
    executor: Arc<dyn Executor>,
}

impl Engine {
    pub fn new(executor: Arc<dyn Executor>) -> Self {
        Self { executor }
    }

    /// Run every phase in order. Each phase loops until the executor reports
    /// `done` or `max_iter` is hit (so a stuck phase can't spin forever). Emits a
    /// [`RunEvent`] at each boundary via `on_event`.
    pub async fn run_workflow(
        &self,
        spec: &WorkflowSpec,
        ctx: &RunCtx,
        mut on_event: impl FnMut(RunEvent),
    ) -> Result<RunSummary, ExecError> {
        let mut summary = RunSummary::default();
        let mut baton: Option<String> = None;

        for (idx, phase_name) in spec.phases.iter().enumerate() {
            // A phase runs its own instruction when the spec carries one
            // (playbook path); a missing/blank entry falls back to the shared
            // `prompt` — byte-for-byte the pre-playbook behavior.
            let phase_prompt = spec
                .phase_prompts
                .get(idx)
                .filter(|p| !p.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| spec.prompt.clone());
            let node = PhaseNode {
                name: phase_name.name.clone(),
                prompt: phase_prompt,
                agents: spec.agents.clone(),
                skills: spec.skills.clone(),
                max_iter: spec.loop_config.max_iter,
                retries: spec.loop_config.retries,
                prior_summary: baton.clone(),
            };
            on_event(RunEvent::PhaseStarted {
                idx,
                name: node.name.clone(),
            });

            let cap = node.max_iter.max(1);
            let mut output = None;
            for _ in 0..cap {
                match self.executor.run_phase(&node, ctx).await {
                    Ok(o) => {
                        let done = o.done;
                        output = Some(o);
                        if done {
                            break;
                        }
                    }
                    Err(e) => {
                        on_event(RunEvent::WorkflowFailed {
                            error: e.to_string(),
                        });
                        return Err(e);
                    }
                }
            }

            // `cap >= 1` guarantees at least one iteration ran.
            let output = output.expect("phase loop runs at least once");
            baton = Some(relay_tail(&output.text));
            summary.phases_run += 1;
            summary.final_output = output.text.clone();
            on_event(RunEvent::PhaseCompleted { idx, output });
        }

        on_event(RunEvent::WorkflowDone {
            summary: summary.clone(),
        });
        Ok(summary)
    }
}

/// The relay baton passed between phases: the **tail** of the previous
/// phase's output (playbook instructions end each phase with a real summary
/// of what was done, so the tail is the highest-signal slice), capped so a
/// long transcript can't blow up the next phase's prompt.
const RELAY_TAIL_MAX_CHARS: usize = 1500;

fn relay_tail(text: &str) -> String {
    let trimmed = text.trim();
    let total = trimmed.chars().count();
    if total <= RELAY_TAIL_MAX_CHARS {
        return trimmed.to_string();
    }
    let skip = total - RELAY_TAIL_MAX_CHARS;
    let tail: String = trimmed.chars().skip(skip).collect();
    format!("…（前文省略 {skip} 字符）{tail}")
}
