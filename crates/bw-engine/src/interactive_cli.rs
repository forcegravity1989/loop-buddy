//! Interactive CLI engine (V1 Issue 2 Phase 1, option (c)).
//!
//! Launches a TUI agent (claude CLI) in an interactive terminal session,
//! pre-loaded with a skill body (as the first user message) and a bridge
//! system prompt (project context + buddy 契约). One skill = one
//! interactive session (§2.3) — no phase splitting, no adversarial loop.
//!
//! Phase 1 scope: spawn a system terminal running the launch plan +
//! best-effort completion detection (wait on the process with a wall-clock
//! timeout). No PTY/xterm embedding, no session.jsonl parsing (§2.5 砍了
//! 对话摘要 collector) — the terminal scrollback + session.jsonl itself is
//! the record. buddy only keeps file-level evidence (HEAD diff, artifacts,
//! status) via the existing `issue_run_tail` / `finalize_run` path.
//!
//! The [`InteractiveExecutor`] trait is the seam Phase 2 will widen
//! (PTY/xterm/hook → `Event::TerminalBytes`), but Phase 1's concrete impl
//! is a plain OS terminal spawn.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use bw_core::playbook::PlaybookCtx;

use crate::{ExecError, RunCtx};

// ─── PromptInjectionMode ───────────────────────────────────────────────

/// How the skill body is injected into the interactive session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptInjectionMode {
    /// Inject the skill body via a flag (design intent: `--prefill`,
    /// which drafts the text in the input box). Falls back to the
    /// positional `prompt` argument when the flag is unavailable —
    /// see [`build_startup_plan`]'s deviation note.
    FlagPrefill,
}

// ─── TuiAgentConfig (orca §2.4 declarative CLI table) ──────────────────

/// Declarative config for a TUI agent CLI. The static table names the
/// agents buddy can launch interactively; `supported` gates whether
/// [`build_startup_plan`] will produce a plan or error honestly.
#[derive(Clone, Debug)]
pub struct TuiAgentConfig {
    pub slug: &'static str,
    pub detect_cmd: &'static str,
    pub launch_cmd: &'static str,
    pub prompt_injection_mode: PromptInjectionMode,
    pub draft_prompt_flag: &'static str,
    pub yolo_flag: &'static str,
    pub supported: bool,
}

/// The claude CLI. `supported = true` — we can really spawn it.
pub static CLAUDE: TuiAgentConfig = TuiAgentConfig {
    slug: "claude",
    detect_cmd: "claude --version",
    launch_cmd: "claude",
    prompt_injection_mode: PromptInjectionMode::FlagPrefill,
    // Design intent: `--prefill` (draft the skill body in the input box).
    // DEVIATION: `--prefill` does not exist in the current `claude --help`
    // (verified 2026-08-04). `build_startup_plan` uses the positional
    // `prompt` argument instead — the skill body becomes the first user
    // message (semantically equivalent: the agent starts working on the
    // skill immediately; difference: auto-sent, not a draft-for-review).
    // The flag name is kept for forward-compat: when the CLI adds it,
    // `build_startup_plan` can switch to `--prefill` with a one-line change.
    draft_prompt_flag: "--prefill",
    yolo_flag: "--dangerously-skip-permissions",
    supported: true,
};

/// Cursor placeholder. Not supported in Phase 1 — calling
/// [`build_startup_plan`] with this config returns an honest error.
pub static CURSOR: TuiAgentConfig = TuiAgentConfig {
    slug: "cursor",
    detect_cmd: "cursor --version",
    launch_cmd: "cursor",
    prompt_injection_mode: PromptInjectionMode::FlagPrefill,
    draft_prompt_flag: "--prefill",
    yolo_flag: "",
    supported: false,
};

// ─── LaunchPlan ────────────────────────────────────────────────────────

/// A fully-resolved startup plan for an interactive agent session. Built
/// by [`build_startup_plan`] from a [`TuiAgentConfig`] + skill body +
/// bridge system prompt. The caller spawns `binary args...` with `env`
/// and `cwd`.
#[derive(Clone, Debug)]
pub struct LaunchPlan {
    pub binary: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: PathBuf,
}

/// Build the startup plan for an interactive agent session.
///
/// For claude (`supported = true`):
/// `claude --append-system-prompt <bridge> <skill_body>
///  --dangerously-skip-permissions --disallowedTools "Bash(gh pr merge)"`
///
/// No `-p`/`--print` (interactive), no `--max-budget-usd` (interactive
/// sessions are user-paced, no per-call cap). The env is inherited from
/// the process but the nested-execution vars are stripped (same as
/// [`crate::ClaudeCliExecutor`]) so the child uses its own CLI config.
pub fn build_startup_plan(
    agent: &TuiAgentConfig,
    skill_body: &str,
    bridge_system_prompt: &str,
    workspace_cwd: &Path,
) -> Result<LaunchPlan, ExecError> {
    if !agent.supported {
        return Err(ExecError::Failed(format!(
            "TUI agent '{}' 暂不支持(Phase 1 仅 claude)",
            agent.slug
        )));
    }

    // Strip nested-execution env vars (same rationale as ClaudeCliExecutor:
    // the host may be running inside a Claude Code session whose injected
    // tokens/gateway/model alias cause 401 in the child).
    let mut env: HashMap<String, String> = std::env::vars().collect();
    for var in [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_MODEL",
        "CLAUDECODE",
        "CLAUDE_CODE_SESSION_ID",
        "CLAUDE_CODE_CHILD_SESSION",
        "CLAUDE_CODE_ENTRYPOINT",
    ] {
        env.remove(var);
    }

    let mut args = Vec::with_capacity(8);
    // Bridge system prompt — appended to the default system prompt.
    args.push("--append-system-prompt".to_string());
    args.push(bridge_system_prompt.to_string());
    // Skill body — positional `prompt` argument (first user message).
    // DEVIATION: design calls for `--prefill <skill_body>` (draft in the
    // input box); the flag doesn't exist in the current CLI. Positional
    // prompt achieves the same effect (skill body is the first thing the
    // agent sees and acts on), just auto-sent instead of a draft.
    args.push(skill_body.to_string());
    // Skip permissions — interactive sessions need to read/write files
    // and run commands without per-action prompts (the user is watching
    // the terminal and can intervene at any time).
    args.push(agent.yolo_flag.to_string());
    // Defense-in-depth: deny `gh pr merge` (验收 = 人 merge, 铁律). Same
    // rule as the one-shot ClaudeCliExecutor path.
    args.push("--disallowedTools".to_string());
    args.push("Bash(gh pr merge)".to_string());

    Ok(LaunchPlan {
        binary: agent.launch_cmd.to_string(),
        args,
        env,
        cwd: workspace_cwd.to_path_buf(),
    })
}

// ─── Bridge system prompt ──────────────────────────────────────────────

/// Build the bridge (衔接层) system prompt — the persistent context the
/// interactive agent operates under. This is NOT the skill body (which
/// goes via the positional prompt); it's the project context + buddy
/// 契约 that stays in the system prompt for the whole session.
///
/// Contents:
/// - Project context (`desc`/`benchmark`/`opportunity`/`north_star`/
///   `ns_def`/`handoff_note`/`workspace_hint` from [`PlaybookCtx`])
/// - Skill-specific产出契约 + 读上游 (north-star-discovery /
///   metrics-binding)
/// - 通用铁律 (Done 永不自动, settle-once, Signal derive-only,
///   绝不伪造观测/Unknown≠绿, 禁 gh pr merge)
pub fn build_bridge_system_prompt(playbook_ctx: &PlaybookCtx, skill_slug: &str) -> String {
    let mut s = String::new();
    s.push_str("# Buddy 衔接层 system prompt\n\n");
    s.push_str(
        "你是 Builders' Workbench (buddy) 派出的交互式 AI 队友,在一个真实项目工作区里干活。\n",
    );
    s.push_str("下面是 buddy 给你的项目上下文 + 契约,请严格遵守。\n\n");

    // ── Project context ──
    s.push_str("## 项目上下文\n");
    s.push_str(&format!("- 项目名: {}\n", playbook_ctx.project_name));
    if !playbook_ctx.project_kind.trim().is_empty() {
        s.push_str(&format!("- 项目类型: {}\n", playbook_ctx.project_kind));
    }
    if !playbook_ctx.project_desc.trim().is_empty() {
        s.push_str(&format!("- 项目说明: {}\n", playbook_ctx.project_desc));
    }
    if !playbook_ctx.benchmark.trim().is_empty() {
        s.push_str(&format!("- 对标对象: {}\n", playbook_ctx.benchmark));
    }
    if !playbook_ctx.opportunity.trim().is_empty() {
        s.push_str(&format!("- 差异化机会: {}\n", playbook_ctx.opportunity));
    }
    if !playbook_ctx.north_star.trim().is_empty() {
        s.push_str(&format!("- 北极星: {}\n", playbook_ctx.north_star));
        if !playbook_ctx.ns_def.trim().is_empty() {
            s.push_str(&format!("- 北极星定义: {}\n", playbook_ctx.ns_def));
        }
    }
    if !playbook_ctx.handoff_note.trim().is_empty() {
        s.push_str(&format!("- 上一阶段交棒: {}\n", playbook_ctx.handoff_note));
    }
    if !playbook_ctx.workspace_hint.trim().is_empty() {
        s.push_str(&format!("- 工作区: {}\n", playbook_ctx.workspace_hint));
    }
    s.push_str(&format!("\n你正在执行技能: `{skill_slug}`\n"));

    // ── Skill-specific 产出契约 + 读上游 ──
    s.push_str("\n## 技能契约\n\n");
    match skill_slug {
        "north-star-discovery" => {
            s.push_str("### 你的产出\n");
            s.push_str(
                "- 写 `<工作区>/.bw/metrics.toml`,严格按 `docs/metrics-toml-format.md` 的结构。\n",
            );
            s.push_str("- 三层结构:恰好 1 个 `[north_star]`,0..N 个 `[[lagging]]`,0..N 个 `[[leading]]`。\n");
            s.push_str(
                "- 每条指标(含北极星)必须附 `collect`,kind 只能是 \
                 `github`/`connector`/`bw`/`manual`/`script` 五选一。\n",
            );
            s.push_str(
                "- 产 `<工作区>/docs/metrics-rationale.md`(人读推导过程,四块:输入摘要/\
                 北极星为什么是这条/滞后引领因果链/采集方案诚实评估)。\n\n",
            );
            s.push_str("### 读上游\n");
            s.push_str(
                "- 读 `<工作区>/docs/competitive-analysis.md`(若存在)——对标名单是北极星推导的第一手依据。\n",
            );
            s.push_str(
                "- 读项目仓既有指标体系:扫 `governance/`、`derive_*.py`/`derive_*.sh`、\
                 `connectors/`、`data-sources/`。三层指标优先对齐既有体系,不另起炉灶。\n",
            );
            s.push_str("- 读项目自身意图(`desc`/`benchmark`/`opportunity`)。\n\n");
            s.push_str("### 硬约束\n");
            s.push_str("- 北极星绝不为「采得到」退化成 commit/PR/issue 数这类工程虚荣指标。\n");
            s.push_str("- 北极星必须落在「使用段」或「价值段」,不能落在「供给段」。\n");
            s.push_str(
                "- `script` kind 的 `query` 只写字段在脚本输出 JSON 里的点分路径\
                 (如 `north_star.adoption_rate`),不含脚本路径。\n",
            );
        }
        "metrics-binding" => {
            s.push_str("### 你的产出\n");
            s.push_str(
                "- 改 `<工作区>/.bw/metrics.toml` 的 `collect` 字段(只改采集方案,\
                 不动 `name`/`def`/`target`)。\n",
            );
            s.push_str("- 更新 `<工作区>/docs/metrics-rationale.md` 的绑定进度段落。\n\n");
            s.push_str("### 读上游\n");
            s.push_str("- 读 `.bw/metrics.toml`(已存在,是本技能的输入)。\n");
            s.push_str(
                "- 读 `docs/metrics-toml-format.md`(五值封闭枚举、占位符语法、upsert 语义)。\n",
            );
            s.push_str("- 读 `docs/metrics-rationale.md`(找指标技能留下的推导)。\n");
            s.push_str(
                "- 扫项目仓既有采集脚本:`governance/`、`derive_*.py`、`connectors/`、`data-sources/`。\n\n",
            );
            s.push_str("### 硬约束\n");
            s.push_str("- 绝不伪造数据。Unknown 就是 Unknown,不因为跑过一遍就假装变绿。\n");
            s.push_str("- 绝不为了点亮而改指标定义(`name`/`def`/`target` 一律不动)。\n");
            s.push_str("- 项目侧自采脚本(`derive_*.py`)是 `script` kind,不是 `manual`——别降级。\n");
            s.push_str(
                "- 绑数据=建 script connector 时,经正规 create_connector 路径,脚本落 `.bw/scripts/`。\n",
            );
        }
        _ => {
            // Unknown skill slug — still give the generic 契约 below.
        }
    }

    // ── 通用铁律 ──
    s.push_str("\n## 通用铁律(不可违反)\n\n");
    s.push_str(
        "- **Done 永不自动**:你干完只到「评审中」,完成永远由人显式点。\
         不要自行标记 Issue Done。\n",
    );
    s.push_str("- **settle-once**:每件活的记账只记一次,绝不重复。\n");
    s.push_str(
        "- **Signal derive-only**:健康灯只被真实数据点亮,无数据=Unknown≠绿,\
         绝不伪造观测。\n",
    );
    s.push_str("- 改动落在活分支,正常提交 + 提 PR,**合并永远是人手动作**。\n");
    s.push_str("- 禁用 `gh pr merge`(已由 `--disallowedTools` 拦截)。\n");

    s
}

// ─── SkillOutput ───────────────────────────────────────────────────────

/// What an interactive skill session produced. Phase 1 doesn't parse
/// session.jsonl (§2.5 砍了对话摘要 collector) — `summary` is
/// best-effort and may be empty (the terminal scrollback + session.jsonl
/// itself is the record). `completed` is the signal the caller uses to
/// decide `RunOutcome::Completed` vs `Err`.
#[derive(Clone, Debug, Default)]
pub struct SkillOutput {
    pub completed: bool,
    pub summary: String,
}

// ─── InteractiveExecutor trait ─────────────────────────────────────────

/// The swappable interactive execution backend (§2.3: one skill = one
/// interactive session, not phase-by-phase). Phase 2 will widen this
/// seam (PTY bytes → `Event::TerminalBytes`), but Phase 1's concrete
/// impls are a plain OS terminal spawn ([`InteractiveCliExecutor`]) and
/// a self-labeled mock ([`MockInteractiveExecutor`]).
#[async_trait]
pub trait InteractiveExecutor: Send + Sync {
    /// Run one interactive skill session. Returns when the session ends
    /// (user exits the terminal / process exits / wall-clock timeout).
    async fn run_skill(&self, plan: &LaunchPlan, ctx: &RunCtx) -> Result<SkillOutput, ExecError>;
}

// ─── InteractiveCliExecutor (Phase 1 (c) real spawn) ──────────────────

/// Phase 1 (c) interactive executor: spawns a system terminal running
/// the launch plan and waits for the process to exit (with a wall-clock
/// timeout backstop).
///
/// NOT PTY/xterm (Phase 2). The terminal is a real OS terminal window —
/// the user sees and interacts with the agent directly. stdin/stdout/
/// stderr are inherited from the OS (the terminal provides the TTY), not
/// piped — the terminal scrollback is the record (§2.5).
///
/// Completion detection (deviation from design's session.jsonl mtime
/// polling — see commit note): wait on the spawned process to exit. On
/// Windows, spawning a console process from a GUI app (Dioxus/wry) creates
/// a new console window automatically. On macOS, `osascript` opens Terminal
/// — the `osascript` process itself exits immediately, so we rely on the
/// wall-clock timeout exclusively. On Linux, spawn directly (may not open
/// a terminal window — best-effort, not a primary platform).
pub struct InteractiveCliExecutor {
    /// Override the claude binary path (e.g. from `BW_CLAUDE_BIN`).
    /// `None` → use `LaunchPlan.binary` (which is `TuiAgentConfig.launch_cmd`).
    claude_binary: Option<String>,
    /// Wall-clock timeout for the whole session. The user is interacting
    /// in real time, so this is generous. On timeout we declare
    /// `completed = true` (the user may still be working — we can't block
    /// the run forever, and the worktree's git state is the real evidence).
    timeout: Duration,
}

impl InteractiveCliExecutor {
    pub fn new() -> Self {
        Self {
            claude_binary: None,
            timeout: Duration::from_secs(60 * 60), // 1 hour
        }
    }

    /// Override the claude binary path (e.g. from `BW_CLAUDE_BIN`).
    pub fn with_claude_binary(mut self, binary: Option<String>) -> Self {
        self.claude_binary = binary;
        self
    }

    /// Override the wall-clock timeout (mainly for tests).
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Default for InteractiveCliExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl InteractiveExecutor for InteractiveCliExecutor {
    async fn run_skill(&self, plan: &LaunchPlan, _ctx: &RunCtx) -> Result<SkillOutput, ExecError> {
        let binary = self.claude_binary.as_deref().unwrap_or(&plan.binary);

        // Build the spawn command. On Windows, spawning a console process
        // from a GUI app creates a new console window (the user sees the
        // agent). On macOS, we use osascript to open Terminal (the osascript
        // process exits immediately, so we rely on the timeout). On Linux,
        // spawn directly (best-effort — may not open a terminal window).
        #[cfg(target_os = "windows")]
        {
            let mut cmd = tokio::process::Command::new(binary);
            cmd.args(&plan.args);
            for (k, v) in &plan.env {
                cmd.env(k, v);
            }
            cmd.current_dir(&plan.cwd);
            cmd.kill_on_drop(true);
            // Inherit stdin/stdout/stderr — the console window provides the
            // TTY for interactive use. Not piping means we can't capture
            // output, but §2.5 砍了对话摘要 collector: the scrollback is
            // the record, buddy doesn't parse it.
            let child = cmd.spawn().map_err(|e| {
                ExecError::Failed(format!("failed to spawn interactive terminal: {e}"))
            })?;
            return self.await_child(child).await;
        }
        #[cfg(target_os = "macos")]
        {
            // macOS: osascript opens Terminal and runs the command. The
            // osascript process exits immediately (it just tells Terminal
            // to open) — we can't wait on the claude process itself. Fall
            // back to the wall-clock timeout.
            let cwd = plan.cwd.display().to_string();
            let mut cmd_line = format!("cd '{cwd}' && {binary}");
            for a in &plan.args {
                cmd_line.push(' ');
                cmd_line.push_str(&shell_quote(a));
            }
            let script = format!("tell application \"Terminal\" to do script \"{cmd_line}\"");
            let mut cmd = tokio::process::Command::new("osascript");
            cmd.arg("-e").arg(&script);
            cmd.kill_on_drop(true);
            let _child = cmd.spawn().map_err(|e| {
                ExecError::Failed(format!("failed to spawn Terminal (osascript): {e}"))
            })?;
            // Can't wait on the claude process — use timeout.
            tokio::time::sleep(self.timeout).await;
            return Ok(SkillOutput {
                completed: true,
                summary: "(wall-clock timeout — Terminal session may still be running)".to_string(),
            });
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            // Linux/other: spawn directly. May not open a terminal window
            // (would need xterm/gnome-terminal wrapper) — best-effort.
            let mut cmd = tokio::process::Command::new(binary);
            cmd.args(&plan.args);
            for (k, v) in &plan.env {
                cmd.env(k, v);
            }
            cmd.current_dir(&plan.cwd);
            cmd.kill_on_drop(true);
            let child = cmd.spawn().map_err(|e| {
                ExecError::Failed(format!("failed to spawn interactive terminal: {e}"))
            })?;
            return self.await_child(child).await;
        }
    }
}

impl InteractiveCliExecutor {
    /// Wait for a spawned child process to exit, with a wall-clock timeout.
    /// On normal exit → `completed = true`. On timeout → `completed = true`
    /// (the user may still be working — the worktree's git state is the
    /// real evidence, not the process's exit code).
    async fn await_child(
        &self,
        mut child: tokio::process::Child,
    ) -> Result<SkillOutput, ExecError> {
        match tokio::time::timeout(self.timeout, child.wait()).await {
            Ok(Ok(_status)) => Ok(SkillOutput {
                completed: true,
                summary: String::new(),
            }),
            Ok(Err(e)) => Err(ExecError::Failed(format!(
                "interactive terminal error: {e}"
            ))),
            Err(_) => {
                // Timeout — declare completed. The worktree's git state
                // is the real evidence; the terminal may still be open.
                Ok(SkillOutput {
                    completed: true,
                    summary: "(wall-clock timeout — terminal may still be running)".to_string(),
                })
            }
        }
    }
}

/// Single-quote a shell argument for osascript (naive but sufficient for
/// file paths and flag values — not a security boundary, the args are
/// buddy-internal, not user-controlled).
#[cfg(target_os = "macos")]
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ─── MockInteractiveExecutor (for tests / no-claude environments) ──────

/// Mock interactive executor — never spawns a real terminal. Returns a
/// self-labeled `【mock】` `SkillOutput` and, if the plan's cwd is a real
/// writable directory, writes a placeholder `.bw/metrics.toml` so the
/// interactive path's downstream (SyncMetricsFile, artifact scan) has
/// something to chew on in tests / no-claude environments.
///
/// The counterpart of [`crate::MockExecutor`] for the interactive path:
/// its sole purpose is to cheaply verify the interactive plumbing works
/// end-to-end without a real `claude` CLI or gateway. Never pretend to be
/// real execution — the 【mock】 label is honest.
#[derive(Debug, Default)]
pub struct MockInteractiveExecutor;

impl MockInteractiveExecutor {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl InteractiveExecutor for MockInteractiveExecutor {
    async fn run_skill(&self, plan: &LaunchPlan, _ctx: &RunCtx) -> Result<SkillOutput, ExecError> {
        // Best-effort: write a placeholder metrics.toml so downstream
        // (SyncMetricsFile) has something to sync. Errors here are
        // non-fatal — the mock's primary job is to return a labeled output.
        let _ = write_mock_metrics_toml(&plan.cwd);

        Ok(SkillOutput {
            completed: true,
            summary: "【mock】交互式技能会话完成(流程演示,未真 spawn claude)".to_string(),
        })
    }
}

/// Write a placeholder `.bw/metrics.toml` to `cwd` (best-effort, non-fatal).
/// Uses blocking `std::fs` — the mock's file write is a test/demo convenience,
/// not a hot path; `tokio::fs` would need the `fs` feature (not enabled in
/// bw-engine's tokio dep, and Phase 1 adds no new features).
fn write_mock_metrics_toml(cwd: &Path) -> std::io::Result<()> {
    let bw_dir = cwd.join(".bw");
    std::fs::create_dir_all(&bw_dir)?;
    let metrics_path = bw_dir.join("metrics.toml");
    let placeholder = "# 【mock】placeholder metrics.toml — written by MockInteractiveExecutor\n\
                       # Replace with real metrics after a real interactive session.\n\
                       schema_version = 1\n\n\
                       [north_star]\n\
                       name = \"【mock】北极星(占位)\"\n\
                       def  = \"【mock】定义占位 — 真实交互式会话后替换\"\n\
                       collect = { kind = \"manual\", query = \"\" }\n";
    // Don't overwrite a real file — only write if it doesn't exist.
    if !metrics_path.exists() {
        std::fs::write(&metrics_path, placeholder)?;
    }
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bw_core::playbook::PlaybookCtx;

    fn mock_playbook_ctx() -> PlaybookCtx {
        PlaybookCtx {
            project_name: "测试项目".to_string(),
            project_kind: "content".to_string(),
            project_desc: "一个测试用项目".to_string(),
            benchmark: "竞品A".to_string(),
            opportunity: "差异化机会X".to_string(),
            north_star: "周活跃用户数".to_string(),
            ns_def: "过去7天至少活跃一次".to_string(),
            handoff_note: String::new(),
            workspace_hint: "工作区 /tmp/test (git 仓库)".to_string(),
        }
    }

    #[test]
    fn build_startup_plan_claude_supported() {
        let tmp = tempfile_dir();
        let skill_body = "# 找指标\n\n这是 skill 正文。";
        let bridge = "bridge system prompt";
        let plan =
            build_startup_plan(&CLAUDE, skill_body, bridge, &tmp).expect("claude is supported");
        assert_eq!(plan.binary, "claude");
        // --append-system-prompt <bridge> <skill_body> --dangerously-skip-permissions --disallowedTools "Bash(gh pr merge)"
        assert!(plan.args.contains(&"--append-system-prompt".to_string()));
        assert!(plan.args.contains(&bridge.to_string()));
        assert!(plan.args.contains(&skill_body.to_string()));
        assert!(plan
            .args
            .contains(&"--dangerously-skip-permissions".to_string()));
        assert!(plan.args.contains(&"--disallowedTools".to_string()));
        assert!(plan.args.contains(&"Bash(gh pr merge)".to_string()));
        // No -p / --print / --max-budget-usd
        assert!(!plan.args.iter().any(|a| a == "-p" || a == "--print"));
        assert!(!plan.args.iter().any(|a| a == "--max-budget-usd"));
        // Env vars stripped
        assert!(!plan.env.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert!(!plan.env.contains_key("CLAUDECODE"));
        // Cwd preserved
        assert_eq!(plan.cwd, tmp);
    }

    #[test]
    fn build_startup_plan_cursor_unsupported() {
        let tmp = tempfile_dir();
        let result = build_startup_plan(&CURSOR, "body", "bridge", &tmp);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cursor"));
        assert!(err.to_string().contains("暂不支持"));
    }

    #[test]
    fn build_bridge_system_prompt_north_star() {
        let ctx = mock_playbook_ctx();
        let prompt = build_bridge_system_prompt(&ctx, "north-star-discovery");
        // Project context
        assert!(prompt.contains("测试项目"));
        assert!(prompt.contains("竞品A"));
        assert!(prompt.contains("周活跃用户数"));
        // Skill-specific
        assert!(prompt.contains("metrics.toml"));
        assert!(prompt.contains("competitive-analysis.md"));
        assert!(prompt.contains("governance/"));
        assert!(prompt.contains("虚荣指标"));
        // 通用铁律
        assert!(prompt.contains("Done 永不自动"));
        assert!(prompt.contains("settle-once"));
        assert!(prompt.contains("Unknown"));
    }

    #[test]
    fn build_bridge_system_prompt_metrics_binding() {
        let ctx = mock_playbook_ctx();
        let prompt = build_bridge_system_prompt(&ctx, "metrics-binding");
        assert!(prompt.contains("metrics-binding"));
        assert!(prompt.contains("绝不伪造数据"));
        assert!(prompt.contains("不动 `name`/`def`/`target`"));
        assert!(prompt.contains("derive_*.py"));
    }

    #[test]
    fn build_bridge_system_prompt_unknown_skill_still_has_generic_rules() {
        let ctx = mock_playbook_ctx();
        let prompt = build_bridge_system_prompt(&ctx, "some-unknown-skill");
        assert!(prompt.contains("Done 永不自动"));
        assert!(prompt.contains("通用铁律"));
    }

    #[tokio::test]
    async fn mock_executor_returns_completed_and_writes_placeholder() {
        let tmp = tempfile_dir();
        let skill_body = "# 找指标\n\nskill body";
        let bridge = "bridge prompt";
        let plan = build_startup_plan(&CLAUDE, skill_body, bridge, &tmp).unwrap();
        let ctx = RunCtx {
            project: bw_core::ProjectId::nil(),
            workflow: bw_core::WorkflowId::nil(),
        };
        let mock = MockInteractiveExecutor::new();
        let output = mock.run_skill(&plan, &ctx).await.unwrap();
        assert!(output.completed);
        assert!(output.summary.contains("【mock】"));
        // Placeholder metrics.toml was written
        let metrics_path = tmp.join(".bw").join("metrics.toml");
        assert!(
            metrics_path.exists(),
            "placeholder metrics.toml should exist"
        );
        let content = std::fs::read_to_string(&metrics_path).unwrap();
        assert!(content.contains("【mock】"));
        assert!(content.contains("schema_version = 1"));
        assert!(content.contains("[north_star]"));
    }

    /// Create a temp directory for test isolation.
    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bw-interactive-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
