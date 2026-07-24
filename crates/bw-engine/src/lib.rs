//! `bw-engine` — workflow execution engine.
//!
//! A workflow is a sequence of phases driven by a swappable [`Executor`]. We
//! ship a deterministic [`MockExecutor`]; the real backend today is
//! [`ClaudeCliExecutor`] (shells out to the local `claude` CLI). The trait
//! together with [`PhaseNode`] / [`PhaseOutput`] / [`RunEvent`] is the **frozen
//! cross-team contract** (plan `00 §9`): a real impl that passes
//! [`contract::check`] is hot-swappable for the mock with zero changes upstream.
//!
//! T6 (plan/12 §3): an [`Executor`] is selected per-run by the assigned
//! Agent's declared `agent_cli` — `"claude-code"` routes to
//! [`ClaudeCliExecutor`] (with its `tools` translated to `--allowedTools`,
//! see [`claude_cli::allowed_tools_arg`]); anything else routes to
//! [`UnsupportedCliExecutor`], an honest "not installed yet" stand-in that
//! reuses this same trait seam instead of opening a new one.

#![forbid(unsafe_code)]

use std::sync::Arc;

use async_trait::async_trait;
use bw_core::model::{verdict_contract_suffix, AgentRef, PhaseRole, SkillRef, WorkflowSpec};
use bw_core::{ProjectId, WorkflowId};

pub mod claude_cli;
pub mod contract;
pub mod evidence;
pub mod git_log;
pub mod github;
pub mod metrics_file;
mod mock;
mod unsupported_cli;
pub mod workspace;

pub use claude_cli::{allowed_tools_arg, ClaudeCliConfig, ClaudeCliExecutor, PermissionMode};
pub use evidence::{EvidenceError, WorkspaceEvidence, WorkspaceFile};
pub use git_log::{read_commits, GitCommit, GitLogError};
pub use github::{GithubError, GithubRepoRef, GithubRepoSummary};
pub use metrics_file::{
    CollectKind, CollectPlan, MetricDef, MetricsFile, MetricsFileError, NorthStarDef,
};
pub use mock::MockExecutor;
pub use unsupported_cli::UnsupportedCliExecutor;
pub use workspace::{provision_git_workspace, ProvisionError};

/// One executable phase, built from a [`WorkflowSpec`] phase.
#[derive(Clone, Debug)]
pub struct PhaseNode {
    pub name: String,
    /// This phase's real role (T8). An [`Executor`] uses it to shape its
    /// behavior — e.g. the [`MockExecutor`] renders a default `PASS` verdict for
    /// an `Evaluator` phase so a gated workflow still completes on the mock.
    pub role: PhaseRole,
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

    /// Run every phase in order, once, then signal `WorkflowDone`. The
    /// straight-pipeline convenience wrapper over [`run_phase_range`] — used for
    /// workflows with no review gate. Each phase loops until the executor
    /// reports `done` or `max_iter` is hit (so a stuck phase can't spin forever).
    ///
    /// The **adversarial** review loop (an Evaluator phase打回 → 重跑 → 重审) is
    /// NOT driven here — that policy (parse the verdict, decide the reject
    /// target, cap the rounds, park the Issue in Blocked) lives in `bw-app`,
    /// which composes [`run_phase_range`] one round at a time so each round
    /// leaves its own settled `workflow_run` row (plan/12 §4, T9).
    pub async fn run_workflow(
        &self,
        spec: &WorkflowSpec,
        ctx: &RunCtx,
        mut on_event: impl FnMut(RunEvent),
    ) -> Result<RunSummary, ExecError> {
        let n = spec.phases.len();
        let outputs = self
            .run_phase_range(spec, ctx, 0..n, None, &mut on_event)
            .await?;
        let summary = RunSummary {
            phases_run: outputs.len(),
            final_output: outputs.last().map(|o| o.text.clone()).unwrap_or_default(),
        };
        on_event(RunEvent::WorkflowDone {
            summary: summary.clone(),
        });
        Ok(summary)
    }

    /// Run the phases in `range` (absolute 0-based indices into `spec.phases`),
    /// in order, returning each phase's final [`PhaseOutput`]. `baton_in` seeds
    /// the relay for the first phase of the range (an adversarial re-run passes
    /// the evaluator's reject feedback here so the regenerating phase sees *why*
    /// it was sent back). Emits `PhaseStarted`/`PhaseCompleted` with **absolute**
    /// indices; does NOT emit `WorkflowDone` (the caller decides when the
    /// workflow is truly finished). On an executor error it emits
    /// `WorkflowFailed` and returns `Err`, exactly as the old full-run loop did.
    ///
    /// An `Evaluator` phase gets the machine-parseable
    /// [`verdict_contract_suffix`] appended to its prompt so a real executor
    /// knows to end its output with a `VERDICT:` line the caller can parse back.
    pub async fn run_phase_range(
        &self,
        spec: &WorkflowSpec,
        ctx: &RunCtx,
        range: std::ops::Range<usize>,
        baton_in: Option<String>,
        mut on_event: impl FnMut(RunEvent),
    ) -> Result<Vec<PhaseOutput>, ExecError> {
        let mut outputs = Vec::new();
        let mut baton = baton_in;

        for idx in range {
            let Some(phase) = spec.phases.get(idx) else {
                break;
            };
            // A phase runs its own instruction when the spec carries one
            // (playbook path); a missing/blank entry falls back to the shared
            // `prompt` — byte-for-byte the pre-playbook behavior.
            let mut phase_prompt = spec
                .phase_prompts
                .get(idx)
                .filter(|p| !p.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| spec.prompt.clone());
            // A review gate carries the verdict output-contract so a real
            // executor emits a parseable decision.
            if phase.role == PhaseRole::Evaluator {
                phase_prompt.push_str(verdict_contract_suffix());
            }
            let node = PhaseNode {
                name: phase.name.clone(),
                role: phase.role,
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
            on_event(RunEvent::PhaseCompleted {
                idx,
                output: output.clone(),
            });
            outputs.push(output);
        }

        Ok(outputs)
    }

    /// T17 (plan/12 §10 v1.1#4): run ONE ad-hoc phase that is not backed by
    /// any `WorkflowSpec.phases` entry — the seam `bw-app`'s
    /// `Command::ParseWorkflowContent` uses to feed a workflow's raw
    /// `content` MD (plus `workflow_parse_contract_suffix`) through the SAME
    /// swappable [`Executor`] every real phase run already goes through
    /// (mock projects self-label, real projects really call `claude -p`),
    /// with none of [`run_phase_range`]'s per-spec-index bookkeeping — there
    /// is no phase index to advance and no `RunEvent` stream to drive since
    /// this isn't part of the workflow's own pipeline, just a one-shot
    /// "read this document" call. No retry loop, no relay baton: one call,
    /// one honest result (the caller decides success/failure from the text).
    pub async fn run_adhoc(&self, node: PhaseNode, ctx: &RunCtx) -> Result<PhaseOutput, ExecError> {
        self.executor.run_phase(&node, ctx).await
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
