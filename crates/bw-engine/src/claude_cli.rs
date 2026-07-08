//! `ClaudeCliExecutor` — shells out to the local `claude` CLI (non-interactive
//! `-p` mode) so a workflow phase can actually read/write files and, opt-in,
//! run commands — not just produce text. This is the first executor in this
//! codebase with real side effects.

use std::path::PathBuf;
use std::process::Stdio;

use async_trait::async_trait;
use serde::Deserialize;

use crate::{ExecError, Executor, PhaseNode, PhaseOutput, RunCtx};

/// Permission mode passed to `claude --permission-mode`. Only the two modes
/// this executor may use are represented here — never
/// `bypassPermissions`/`--dangerously-skip-permissions` as a *default*: the
/// CLI's own `--help` text restricts that mode to "sandboxes with no internet
/// access", which a desktop app's subprocess is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionMode {
    AcceptEdits,
    BypassPermissions,
}

impl PermissionMode {
    pub fn as_cli_flag(&self) -> &'static str {
        match self {
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::BypassPermissions => "bypassPermissions",
        }
    }
}

/// Process-wide configuration. Per-project data (`workspace`/`allow_commands`)
/// is supplied separately at [`ClaudeCliExecutor::new`] time, since it's
/// runtime data read from a `ProjectRow`, not something fixed at startup.
#[derive(Clone, Debug)]
pub struct ClaudeCliConfig {
    /// Override the `claude` binary path/name. `None` → resolved from `PATH`.
    pub binary: Option<String>,
    /// `--max-budget-usd` cap applied to a single phase call.
    pub max_budget_usd: f64,
    /// Mode used when the project has NOT opted into command execution.
    pub default_mode: PermissionMode,
    /// Mode used when the project HAS opted into command execution
    /// (`allow_commands = true`). Still `AcceptEdits` by default (never
    /// bypass permissions by default) — this field exists so it can be
    /// reconfigured to `BypassPermissions` if `--allowedTools Bash` alone
    /// turns out not to unlock command execution under `acceptEdits` (this
    /// combination is unverified in a sandboxed dev environment; see the
    /// implementation plan's "已知留白" #1).
    pub commands_mode: PermissionMode,
}

impl Default for ClaudeCliConfig {
    fn default() -> Self {
        Self {
            binary: None,
            max_budget_usd: 0.50,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        }
    }
}

/// Real, side-effecting executor: shells out to `claude -p` once per phase,
/// inside `workspace`.
pub struct ClaudeCliExecutor {
    config: ClaudeCliConfig,
    workspace: PathBuf,
    allow_commands: bool,
}

impl ClaudeCliExecutor {
    pub fn new(config: ClaudeCliConfig, workspace: PathBuf, allow_commands: bool) -> Self {
        Self {
            config,
            workspace,
            allow_commands,
        }
    }
}

/// Interpolates a [`PhaseNode`]'s name/prompt/agents/skills into a real
/// prompt. Every phase in a `WorkflowSpec` currently shares the same
/// `prompt` text (an existing property, not introduced here) — agents/skills
/// are surfaced as advisory hints, not enforced.
pub fn build_prompt(phase: &PhaseNode) -> String {
    let mut out = format!("# 阶段：{}\n\n{}", phase.name, phase.prompt);
    if !phase.agents.is_empty() {
        let names: Vec<&str> = phase.agents.iter().map(|a| a.name.as_str()).collect();
        out.push_str(&format!("\n\n建议协作角色：{}", names.join("、")));
    }
    if !phase.skills.is_empty() {
        let names: Vec<&str> = phase.skills.iter().map(|s| s.name.as_str()).collect();
        out.push_str(&format!("\n建议参考技能：{}", names.join("、")));
    }
    out
}

/// Shape of `claude -p --output-format json`'s stdout (empirically verified
/// against a real, auth-failed call — fields present regardless of success).
#[derive(Deserialize)]
struct CliResult {
    #[serde(default)]
    result: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    permission_denials: Vec<serde_json::Value>,
}

#[async_trait]
impl Executor for ClaudeCliExecutor {
    async fn run_phase(&self, phase: &PhaseNode, _ctx: &RunCtx) -> Result<PhaseOutput, ExecError> {
        if self.workspace.as_os_str().is_empty() {
            return Err(ExecError::Failed(
                "ClaudeCliExecutor constructed with an empty workspace path".into(),
            ));
        }

        let prompt = build_prompt(phase);
        let mode = if self.allow_commands {
            self.config.commands_mode
        } else {
            self.config.default_mode
        };

        let mut cmd =
            tokio::process::Command::new(self.config.binary.as_deref().unwrap_or("claude"));
        cmd.current_dir(&self.workspace)
            .arg("-p")
            .arg(&prompt)
            .arg("--output-format")
            .arg("json")
            .arg("--no-session-persistence")
            .arg("--max-budget-usd")
            .arg(self.config.max_budget_usd.to_string())
            .arg("--permission-mode")
            .arg(mode.as_cli_flag())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if self.allow_commands {
            cmd.arg("--allowedTools").arg("Bash");
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| ExecError::Failed(format!("failed to spawn claude CLI: {e}")))?;

        if !output.status.success() && output.stdout.is_empty() {
            return Err(ExecError::Failed(format!(
                "claude CLI exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        let parsed: CliResult = serde_json::from_slice(&output.stdout).map_err(|e| {
            ExecError::Failed(format!(
                "failed to parse claude CLI JSON output: {e} (raw: {})",
                String::from_utf8_lossy(&output.stdout)
            ))
        })?;

        if parsed.is_error {
            return Err(ExecError::Failed(parsed.result));
        }

        let mut text = parsed.result;
        // v1 has no multi-turn loop, so `done` is unconditionally `true` here —
        // folding denials into `gaps` instead would violate contract.rs's
        // "done && !gaps.is_empty() is illegal" invariant and could re-run an
        // inherently-stuck phase up to `max_iter` times, multiplying real spend.
        if !parsed.permission_denials.is_empty() {
            text.push_str(&format!(
                "\n\n[权限提示] {} 项操作被当前权限模式拒绝",
                parsed.permission_denials.len()
            ));
        }

        Ok(PhaseOutput {
            text,
            done: true,
            gaps: vec![],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bw_core::model::{AgentRef, SkillRef};

    fn node(agents: Vec<AgentRef>, skills: Vec<SkillRef>) -> PhaseNode {
        PhaseNode {
            name: "证据".into(),
            prompt: "界定→采集→结构化→分析".into(),
            agents,
            skills,
            max_iter: 3,
            retries: 1,
        }
    }

    #[test]
    fn build_prompt_interpolates_name_and_prompt() {
        let out = build_prompt(&node(vec![], vec![]));
        assert!(out.contains("证据"));
        assert!(out.contains("界定→采集→结构化→分析"));
        assert!(!out.contains("建议协作角色"), "no agents ⇒ no hint line");
        assert!(!out.contains("建议参考技能"), "no skills ⇒ no hint line");
    }

    #[test]
    fn build_prompt_surfaces_agents_and_skills_as_advisory_hints() {
        let agents = vec![AgentRef {
            name: "产品策略师".into(),
            def: String::new(),
            from: String::new(),
        }];
        let skills = vec![SkillRef {
            name: "竞品分析".into(),
            def: String::new(),
            from: String::new(),
        }];
        let out = build_prompt(&node(agents, skills));
        assert!(out.contains("产品策略师"));
        assert!(out.contains("竞品分析"));
    }

    #[test]
    fn permission_mode_maps_to_the_real_cli_flag_values() {
        assert_eq!(PermissionMode::AcceptEdits.as_cli_flag(), "acceptEdits");
        assert_eq!(
            PermissionMode::BypassPermissions.as_cli_flag(),
            "bypassPermissions"
        );
    }

    #[test]
    fn default_config_never_defaults_to_bypass_permissions() {
        let config = ClaudeCliConfig::default();
        assert_eq!(config.default_mode, PermissionMode::AcceptEdits);
        assert_eq!(config.commands_mode, PermissionMode::AcceptEdits);
    }

    #[tokio::test]
    async fn empty_workspace_fails_fast_without_spawning() {
        let executor = ClaudeCliExecutor::new(ClaudeCliConfig::default(), PathBuf::new(), false);
        let ctx = RunCtx {
            project: bw_core::ProjectId::nil(),
            workflow: bw_core::WorkflowId::nil(),
        };
        let err = executor.run_phase(&node(vec![], vec![]), &ctx).await;
        assert!(err.is_err());
    }
}
