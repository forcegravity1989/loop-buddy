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
    /// T6 (plan/12 §3): the assigned Agent's declared `tools`
    /// (`AgentCard.tools`, == AllowedTools) — translated to `--allowedTools`
    /// by this adapter, never by the caller. Empty = no restriction declared,
    /// byte-for-byte the pre-T6 behavior when `allow_commands` is also false.
    tools: Vec<String>,
}

impl ClaudeCliExecutor {
    pub fn new(
        config: ClaudeCliConfig,
        workspace: PathBuf,
        allow_commands: bool,
        tools: Vec<String>,
    ) -> Self {
        Self {
            config,
            workspace,
            allow_commands,
            tools,
        }
    }
}

/// T6 (plan/12 §3): the exact `--allowedTools` value this executor would
/// pass the `claude` CLI for a given `tools`/`allow_commands` pair — pure and
/// deterministic (no IO), so a caller (`bw-app`'s run-params snapshot) can
/// record the real invocation argument BEFORE the subprocess is even
/// spawned. That's what makes the value verifiable without depending on the
/// real `claude -p` call succeeding (gateway 抖动 is never a verification
/// gate here).
///
/// `allow_commands` (a per-project opt-in, unrelated to any one Agent's
/// declared `tools`) always implies `Bash` — merged in, never duplicated.
/// `None` = no `--allowedTools` flag at all: the honest pre-T6 behavior when
/// `tools` is empty and the project hasn't opted into command execution.
pub fn allowed_tools_arg(tools: &[String], allow_commands: bool) -> Option<String> {
    let mut merged: Vec<String> = tools.to_vec();
    if allow_commands && !merged.iter().any(|t| t == "Bash") {
        merged.push("Bash".to_string());
    }
    if merged.is_empty() {
        None
    } else {
        Some(merged.join(","))
    }
}

/// Interpolates a [`PhaseNode`]'s name/prompt/agents/skills into a real
/// prompt. Every phase in a `WorkflowSpec` currently shares the same
/// `prompt` text (an existing property, not introduced here) — agents/skills
/// are surfaced as advisory hints, not enforced.
pub fn build_prompt(phase: &PhaseNode) -> String {
    let mut out = format!("# 阶段：{}\n\n{}", phase.name, phase.prompt);
    if let Some(prior) = phase
        .prior_summary
        .as_deref()
        .filter(|p| !p.trim().is_empty())
    {
        out.push_str(&format!(
            "\n\n## 上一阶段真实产出（接力棒，供衔接，不要重做）\n{prior}"
        ));
    }
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
///
/// 2026-07-21 真实探测确认:`result` 字段在 `subtype:"error_max_budget_usd"`
/// (预算耗尽)时**完全不出现**——只有 `errors`(字符串数组)和 `subtype` 带
/// 真实原因。旧版只读 `result`,预算耗尽时 `err_text` 落到空字符串,
/// `ExecError::Failed("")` 让使用者看不出真实失败原因(见
/// `[[bw-real-executor-pending-verification]]` 的诚实排查记录)。
#[derive(Deserialize)]
struct CliResult {
    #[serde(default)]
    result: String,
    #[serde(default)]
    is_error: bool,
    #[serde(default)]
    permission_denials: Vec<serde_json::Value>,
    #[serde(default)]
    subtype: String,
    #[serde(default)]
    errors: Vec<String>,
}

impl CliResult {
    /// The real failure text: prefer `result` (carries normal API-rejection
    /// messages like 429/529), fall back to `errors`/`subtype` for the shapes
    /// that never populate `result` at all (budget exhaustion confirmed;
    /// likely other terminal-reason variants too).
    fn error_text(&self) -> String {
        if !self.result.trim().is_empty() {
            return self.result.clone();
        }
        if !self.errors.is_empty() {
            return format!("{}(subtype={})", self.errors.join("; "), self.subtype);
        }
        format!(
            "claude CLI 返回 is_error=true 但无诊断文本(subtype={})",
            self.subtype
        )
    }
}

/// Gateway-side transient failures (overload / brief unavailability). Only
/// these are retried — auth errors, budget stops, and parse failures are
/// final on the first occurrence. Patterns cover both Anthropic first-party
/// ("overloaded_error") and third-party inference gateways（如 bigmodel 的
/// 「访问量过大」529）.
fn is_transient_gateway_error(msg: &str) -> bool {
    [
        "API Error: 529",
        "API Error: 503",
        "API Error: 502",
        "API Error: 504",
    ]
    .iter()
    .any(|p| msg.contains(p))
        || msg.to_ascii_lowercase().contains("overloaded")
        || msg.contains("访问量过大")
}

/// Bounded retry schedule for transient gateway errors. Failed-at-gateway
/// attempts cost $0 (the error precedes generation), so the per-phase budget
/// cap is not multiplied in practice.
const TRANSIENT_BACKOFF_SECS: &[u64] = &[30, 90, 180];
/// A single attempt may legitimately run long (real coding work), but a hung
/// child must not silently eat the whole stage window.
const ATTEMPT_TIMEOUT_SECS: u64 = 30 * 60;

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

        let mut attempt = 0usize;
        loop {
            let mut cmd =
                tokio::process::Command::new(self.config.binary.as_deref().unwrap_or("claude"));
            // 宿主若本身运行在 Claude Code 会话内（嵌套执行），环境里会注入会话级
            // 令牌/网关地址/模型别名——子 CLI 用它们会 401。剥离后子进程回落到
            // 用户自己的 CLI 配置，这是唯一对子进程有效的凭据。
            for var in [
                "ANTHROPIC_AUTH_TOKEN",
                "ANTHROPIC_BASE_URL",
                "ANTHROPIC_MODEL",
                "CLAUDECODE",
                "CLAUDE_CODE_SESSION_ID",
                "CLAUDE_CODE_CHILD_SESSION",
                "CLAUDE_CODE_ENTRYPOINT",
            ] {
                cmd.env_remove(var);
            }
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
                .stderr(Stdio::piped())
                // 阶段级超时会 drop 这个 future——没有 kill_on_drop 的话
                // 子 claude 进程会泄漏并继续烧预算。
                .kill_on_drop(true);

            if let Some(arg) = allowed_tools_arg(&self.tools, self.allow_commands) {
                cmd.arg("--allowedTools").arg(arg);
            }

            let output = tokio::time::timeout(
                std::time::Duration::from_secs(ATTEMPT_TIMEOUT_SECS),
                cmd.output(),
            )
            .await
            .map_err(|_| {
                ExecError::Failed(format!(
                    "claude CLI attempt exceeded {ATTEMPT_TIMEOUT_SECS}s (hung child killed)"
                ))
            })?
            .map_err(|e| ExecError::Failed(format!("failed to spawn claude CLI: {e}")))?;

            // 两条失败通道都可能是瞬时网关错误：非零退出+空 stdout（错误只
            // 打到 stderr），或 JSON 里的 is_error（网关错误经 CLI 转述）。
            let err_text = if !output.status.success() && output.stdout.is_empty() {
                format!(
                    "claude CLI exited with {}: {}",
                    output.status,
                    String::from_utf8_lossy(&output.stderr)
                )
            } else {
                let parsed: CliResult = serde_json::from_slice(&output.stdout).map_err(|e| {
                    ExecError::Failed(format!(
                        "failed to parse claude CLI JSON output: {e} (raw: {})",
                        String::from_utf8_lossy(&output.stdout)
                    ))
                })?;
                if !parsed.is_error {
                    let mut text = parsed.result;
                    // v1 has no multi-turn loop, so `done` is unconditionally
                    // `true` here — folding denials into `gaps` instead would
                    // violate contract.rs's "done && !gaps.is_empty() is
                    // illegal" invariant and could re-run an inherently-stuck
                    // phase up to `max_iter` times, multiplying real spend.
                    if !parsed.permission_denials.is_empty() {
                        text.push_str(&format!(
                            "\n\n[权限提示] {} 项操作被当前权限模式拒绝",
                            parsed.permission_denials.len()
                        ));
                    }
                    return Ok(PhaseOutput {
                        text,
                        done: true,
                        gaps: vec![],
                    });
                }
                parsed.error_text()
            };

            if attempt < TRANSIENT_BACKOFF_SECS.len() && is_transient_gateway_error(&err_text) {
                let delay = TRANSIENT_BACKOFF_SECS[attempt];
                attempt += 1;
                eprintln!(
                    "  [executor] 瞬时网关错误（第 {attempt} 次，{delay}s 后重试）: {}",
                    err_text.chars().take(120).collect::<String>()
                );
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                continue;
            }
            return Err(ExecError::Failed(err_text));
        }
    }
}
