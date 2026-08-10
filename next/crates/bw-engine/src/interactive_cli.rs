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
//! The [`InteractiveExecutor`] trait is the seam Phase 2 widened (PTY/xterm/
//! hook); Phase 1's concrete impl is a plain OS terminal spawn, still used as
//! the non-Windows fallback.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use bw_core::playbook::PlaybookCtx;
use tokio::sync::mpsc;

use crate::pty_backend::PtyBackend;
use crate::{ExecError, RunCtx};

// ─── PtyInput (V1 Issue2 Phase2b) ───────────────────────────────────────

/// Input the App sends to the PTY executor. The App holds the sender side;
/// the executor holds the receiver. Sent via mpsc so the executor can
/// `select!` between PTY output and user input.
#[derive(Clone, Debug)]
pub enum PtyInput {
    /// User-typed bytes (from the UI's xterm.js `onData` →
    /// `Command::TerminalInput`).
    Bytes(Vec<u8>),
    /// Terminal resize (from the UI's xterm.js `onResize` →
    /// `Command::TerminalResize`).
    Resize { cols: u16, rows: u16 },
}

// ─── PromptInjectionMode ───────────────────────────────────────────────

/// How the skill body is injected into the interactive session. One variant
/// per real injection strategy — adding a CLI that needs a different one
/// means adding a variant and the matching arm in [`build_startup_plan`],
/// not a new impl (orca §2.4's declarative table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptInjectionMode {
    /// The skill body rides as the positional `prompt` argument, becoming the
    /// session's first user message. This is what claude actually supports:
    /// the design's `--prefill` (draft the text in the input box for review)
    /// does not exist in `claude --help` (verified 2026-08-04), so the flag
    /// and its config field were dropped rather than kept as dead forward-
    /// compat (§2.6 #5).
    PositionalArgv,
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
    pub yolo_flag: &'static str,
    /// V1 Issue2 Phase2a: the flag to resume the most recent session in the
    /// current directory. For claude: `--continue` (`-c`). Resume re-enters
    /// the existing session (no new prompt, no `--append-system-prompt`) —
    /// the skill body + bridge prompt were injected on the first run and
    /// persist in the session.
    pub resume_flag: &'static str,
    /// V1 Issue2 Phase2b: the flag to resume a SPECIFIC session by id. For
    /// claude: `--resume <session_id>`. More precise than `--continue` (which
    /// resumes "most recent in cwd") — the session_id is captured from the
    /// SessionStart hook event and stored on the issue. When the caller has a
    /// session_id, `build_resume_plan` uses this; when not, it falls back to
    /// `resume_flag` (`--continue`).
    pub resume_id_flag: &'static str,
    pub supported: bool,
}

/// The claude CLI. `supported = true` — we can really spawn it.
pub static CLAUDE: TuiAgentConfig = TuiAgentConfig {
    slug: "claude",
    detect_cmd: "claude --version",
    launch_cmd: "claude",
    // The skill body goes in as the positional prompt (claude has no
    // `--prefill`; see `PromptInjectionMode::PositionalArgv`).
    prompt_injection_mode: PromptInjectionMode::PositionalArgv,
    yolo_flag: "--dangerously-skip-permissions",
    // Verified 2026-08-05 via `claude --help`: `-c, --continue` resumes the
    // most recent conversation in the current directory. Each interactive
    // issue has its own worktree (`bw/issue-N`), so `--continue` from that
    // cwd hits the session started there. No session_id needed (unlike
    // `--resume <id>` which 2a doesn't have a hook to capture).
    // Phase2b: `--resume <session_id>` is the precise resume (captured from
    // the SessionStart hook event, stored on the issue). `--continue` remains
    // the fallback when no session_id is available (F1: empty session_id
    // actually falls back to build_startup_plan, not --continue).
    resume_flag: "--continue",
    // Verified 2026-08-05: `claude --resume <session_id>` resumes a specific
    // session by id (interactive, no -p, no new prompt). The session_id is
    // captured from the SessionStart hook payload and stored on the issue.
    resume_id_flag: "--resume",
    supported: true,
};

/// Cursor placeholder. Not supported in Phase 1 — calling
/// [`build_startup_plan`] with this config returns an honest error.
pub static CURSOR: TuiAgentConfig = TuiAgentConfig {
    slug: "cursor",
    detect_cmd: "cursor --version",
    launch_cmd: "cursor",
    prompt_injection_mode: PromptInjectionMode::PositionalArgv,
    yolo_flag: "",
    resume_flag: "--continue",
    resume_id_flag: "--resume",
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
    /// V1 Issue2 W2 (§9.5): whether `run_skill_pty` should submit the
    /// positional prompt after the TUI loads. claude interactive's
    /// positional argv doesn't auto-submit in buddy's GLM-gateway environment
    /// (the TUI starts and waits for Enter). `true` on first run (the startup
    /// plan has a positional skill body) → `run_skill_pty` sends `\r` after a
    /// brief ready-wait. `false` on resume (no positional — the session
    /// continues from where it left off).
    pub submit_prompt: bool,
}

/// Build the startup plan for an interactive agent session.
///
/// For claude (`supported = true`):
/// `claude --append-system-prompt <system_prompt> <position_prompt>
///  --dangerously-skip-permissions --disallowedTools "Bash(gh pr merge)"`
///
/// - `system_prompt` → `--append-system-prompt`(caller 传 bridge prompt +
///   技能正文 + 蒸馏/目录块,见 `run_issue_interactive`)。
/// - `position_prompt` → 位置 prompt(auto-submit 首句用户消息;caller 传
///   issue 标题+描述)。
///
/// No `-p`/`--print` (interactive), no `--max-budget-usd` (interactive
/// sessions are user-paced, no per-call cap). The env is inherited from
/// the process but the nested-execution vars are stripped (same as
/// [`crate::ClaudeCliExecutor`]) so the child uses its own CLI config.
pub fn build_startup_plan(
    agent: &TuiAgentConfig,
    position_prompt: &str,
    system_prompt: &str,
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
    // System prompt — appended to claude's default system prompt. Caller
    // assembles bridge (project context + 铁律 + 技能契约) + 技能正文 +
    // 蒸馏/目录块 into this one string.
    args.push("--append-system-prompt".to_string());
    args.push(system_prompt.to_string());
    // Positional `prompt` — the first user message, auto-submitted
    // (`submit_prompt: true` sends Enter). Caller passes issue 标题+描述
    // (the requirement); the skill methodology lives in the system prompt
    // above, so claude runs the method ON the requirement.
    // DEVIATION: design once called for `--prefill <skill_body>` (draft in
    // the input box); the flag doesn't exist in the current CLI. Positional
    // prompt achieves the same effect, just auto-sent instead of a draft.
    args.push(position_prompt.to_string());
    // Skip permissions — interactive sessions need to read/write files
    // and run commands without per-action prompts (the user is watching
    // the terminal and can intervene at any time).
    args.push(agent.yolo_flag.to_string());
    // Deny `gh pr merge` (验收 = 人 merge, 铁律). This is a partial fence,
    // not a wall: it doesn't cover `codehub-cli mr merge` (the CodeHub
    // equivalent) and a deny list under `--dangerously-skip-permissions` is
    // only as strong as the CLI's own enforcement. 「合并永远是人」的真正
    // 约束在衔接层 system prompt 里说清楚(`build_bridge_system_prompt`),
    // 且 buddy 侧 Done 入边由状态机守死 —— 别把这两行当成拦得住的保证。
    args.push("--disallowedTools".to_string());
    args.push("Bash(gh pr merge)".to_string());

    Ok(LaunchPlan {
        binary: agent.launch_cmd.to_string(),
        args,
        env,
        cwd: workspace_cwd.to_path_buf(),
        // First run: the positional skill body needs an Enter to submit (§9.5).
        submit_prompt: true,
    })
}

/// Build the resume plan for an interactive agent session (V1 Issue2
/// Phase2b). Resume re-enters an existing session — no new prompt, no
/// `--append-system-prompt` (the skill body + bridge prompt were injected
/// on the first run and persist in the session stored under
/// `~/.claude/projects/<encoded-cwd>/`).
///
/// When `session_id` is `Some(id)`:
/// `claude --resume <id> --dangerously-skip-permissions --disallowedTools "Bash(gh pr merge)"`
/// — precise resume of the exact session (captured from the SessionStart hook).
///
/// When `session_id` is `None`:
/// `claude --continue --dangerously-skip-permissions --disallowedTools "Bash(gh pr merge)"`
/// — fallback: resume the most recent session in this cwd. Used when no
/// session_id was captured (hook not yet wired, or SessionStart didn't fire).
/// In practice, the App layer's F1 fix routes empty-session_id issues to
/// `build_startup_plan` (re-inject skill) instead of this fallback, so
/// `None` is only reached when the caller explicitly wants the imprecise
/// `--continue` path.
///
/// The `cwd` MUST match the first run's cwd (the issue's `bw/issue-N`
/// worktree path) so `--continue` finds the right session. The env strip
/// is identical to [`build_startup_plan`] (nested-execution vars removed).
pub fn build_resume_plan(
    agent: &TuiAgentConfig,
    session_id: Option<&str>,
    workspace_cwd: &Path,
) -> Result<LaunchPlan, ExecError> {
    if !agent.supported {
        return Err(ExecError::Failed(format!(
            "TUI agent '{}' 暂不支持(Phase 1 仅 claude)",
            agent.slug
        )));
    }

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

    // --resume <session_id> (precise) or --continue (fallback). No positional
    // prompt (the session continues from where it left off). Same permission
    // posture as the first run + deny `gh pr merge` (验收 = 人 merge, 铁律).
    let mut args = Vec::with_capacity(5);
    if let Some(id) = session_id.filter(|s| !s.is_empty()) {
        args.push(agent.resume_id_flag.to_string());
        args.push(id.to_string());
    } else {
        args.push(agent.resume_flag.to_string());
    }
    args.push(agent.yolo_flag.to_string());
    args.push("--disallowedTools".to_string());
    args.push("Bash(gh pr merge)".to_string());

    Ok(LaunchPlan {
        binary: agent.launch_cmd.to_string(),
        args,
        env,
        cwd: workspace_cwd.to_path_buf(),
        // Resume: no positional prompt to submit — the session continues.
        submit_prompt: false,
    })
}

/// V1-TermRefactor5 · 咨询态守门规则(设计 md §6.2)。
///
/// 行为约定,不是技术只读——claude 仍有完整 CLI 能力;buddy 不宣称硬隔离。
/// 仅 [`build_consultation_resume_plan`](Done/InReview 续聊)注入;
/// 交付 resume([`build_resume_plan`])不带这条。
pub const CONSULTATION_APPEND_PROMPT: &str = "\
这件活已经完成并由人验收。你可以继续回答历史决策、代码解释、后续讨论。\
如果用户提出新的文件修改、代码开发或其他会产生交付的工作,\
请建议用户在 buddy 中新建一件活来处理,不要把新交付继续记在这件已完成的活上。";

/// Build the consultation resume plan (V1-TermRefactor5 · 咨询态).
///
/// Same as [`build_resume_plan`] (`--resume` / `--continue` + yolo + deny
/// `gh pr merge`), plus `--append-system-prompt` with
/// [`CONSULTATION_APPEND_PROMPT`]. Only the `open_conversation` path
/// (Done/InReview 续聊) should call this — delivery resume stays on
/// [`build_resume_plan`] so InProgress 交付不注入咨询规则。
///
/// Honest posture: this is a behavioural convention, not a hard sandbox.
pub fn build_consultation_resume_plan(
    agent: &TuiAgentConfig,
    session_id: Option<&str>,
    workspace_cwd: &Path,
) -> Result<LaunchPlan, ExecError> {
    let mut plan = build_resume_plan(agent, session_id, workspace_cwd)?;
    // Inject consultation rules ahead of the permission flags so the
    // append is adjacent to the resume identity (mirrors startup plan's
    // `--append-system-prompt` placement near the front).
    let insert_at = if plan.args.first().is_some_and(|a| a == agent.resume_id_flag) {
        2 // after `--resume <id>`
    } else {
        1 // after `--continue`
    };
    plan.args
        .insert(insert_at, "--append-system-prompt".to_string());
    plan.args
        .insert(insert_at + 1, CONSULTATION_APPEND_PROMPT.to_string());
    Ok(plan)
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
    // V1 收口:空技能(无技能 issue)显「未关联技能」;非空(含未知 typo)显
    // 「你正在执行技能: {slug}」让用户看到 slug 能自查。下方技能契约 match 对
    // 未知 slug 走 `_` 臂只给通用铁律,空 slug 同样。
    if skill_slug.trim().is_empty() {
        s.push_str("\n未关联技能,由你驱动或按用户要求干活;项目上下文与铁律已就位。\n");
    } else {
        s.push_str(&format!("\n你正在执行技能: `{skill_slug}`\n"));
    }

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
                "- 每条指标(含北极星)必须附 `collect`。采集 kind 优先 `script`\
                 (自动:机械解析数据源产出 JSON,`query`=字段在 JSON 里的点分路径;\
                 buddy 自带 instance 包 codehub/github CLI,或项目侧 `derive_*.py`)\
                 或 `manual`(人手填,戴「手填」徽)。`github`/`codehub`/`bw`/`connector`\
                 是 legacy inline arm(格式档 `docs/metrics-toml-format.md` 仍列五值兼容),\
                 正退休进 `script`——新写优先 `script`/`manual`,不写 legacy kind。\n",
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
                "- 读 `docs/metrics-toml-format.md`(采集 kind 以 `script`|`manual` 两 kind\
                 为方向;`github`/`codehub`/`bw`/`connector` 是 legacy 仍列五值兼容、正退休进 `script`;\
                 占位符语法、upsert 语义)。\n",
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
                "- 绑数据=搭采集装置:写采集脚本到 `.bw/scripts/`(buddy 自带 instance 包 \
                 codehub/github CLI,或项目侧 `derive_*.py` 留原位)+ 写连接器清单 \
                 `.bw/connectors.toml`(格式见 `docs/connectors-toml-format.md`,数组表键名 \
                 是单数 `[[connector]]`,写成复数 `[[connectors]]` 会解析失败)+ \
                 给 metric 配 `collect_kind='script'`+`collect_query=字段路径`。\
                 **agent 不调 buddy API**——靠文件正本 + PR 合入后 buddy 感知 sync(\
                 `.bw/connectors.toml` → `connector` 行 upsert,`.bw/metrics.toml` → \
                 `metric` 行 upsert;cron 到点自动跑 script connector → observation → \
                 signal)。`script` 的 `query` 只写字段在脚本输出 JSON 里的点分路径\
                 (如 `leading.L1`),不含脚本路径。\n",
            );
            s.push_str(
                "- **`.bw/connectors.toml` 的 `output` 字段必须真实写文件**:buddy 采集时\
                 只读 `output` 指向的那个文件,完全不看脚本的 stdout——哪怕脚本 `print()` \
                 出了正确的 JSON,不写进 `output` 文件也等于没接。脚本必须真的落盘一份 \
                 JSON,再把相对路径填进 `output`,收尾前务必确认这个文件真的存在。\n",
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
    s.push_str("- 改动落在活分支,正常提交 + 提 PR/MR,**合并永远是人手动作**。\n");
    s.push_str(
        "- **你只负责提上去,绝不自己合入**:`gh pr merge`、\
         `codehub-cli mr merge` 以及任何等价的合并/直推主干动作一律不许执行。\
         提完 PR/MR 把地址打屏给用户,合入由人在 buddy 里点。\n",
    );

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
/// interactive session, not phase-by-phase). Phase 2 widened this
/// seam (PTY bytes → the App's `watch` channel), but Phase 1's concrete
/// impls are a plain OS terminal spawn ([`InteractiveCliExecutor`]) and
/// a self-labeled mock ([`MockInteractiveExecutor`]).
#[async_trait]
pub trait InteractiveExecutor: Send + Sync {
    /// Run one interactive skill session (first run). Returns when the
    /// session ends (user exits the terminal / process exits / wall-clock
    /// timeout).
    async fn run_skill(&self, plan: &LaunchPlan, ctx: &RunCtx) -> Result<SkillOutput, ExecError>;

    /// Resume an existing interactive skill session (V1 Issue2 Phase2b).
    /// The plan is built by [`build_resume_plan`] (`--resume <session_id>`
    /// when a session_id is available, `--continue` as fallback). The session
    /// persists under `~/.claude/projects/<encoded-cwd>/` from the first run;
    /// resume re-enters it. Same lifecycle as `run_skill` (exits when the
    /// process exits / wall-clock timeout).
    async fn run_skill_resume(
        &self,
        plan: &LaunchPlan,
        ctx: &RunCtx,
    ) -> Result<SkillOutput, ExecError>;

    /// V1 Issue2 Phase2b: spawn the agent in a PTY and stream bytes. The
    /// caller provides two channels:
    ///  - `bytes_tx`: PTY → App. The executor reads master bytes and sends
    ///    them here (the App forwards them on its `pty_bytes` watch channel;
    ///    there is no `Event` variant for terminal bytes — that design was
    ///    dropped to avoid double-consuming the stream).
    ///  - `input_rx`: App → PTY. The App sends [`PtyInput::Bytes`] (user
    ///    typed) and [`PtyInput::Resize`] (terminal resized). The executor
    ///    writes to the PTY master / resizes.
    ///
    /// Returns when the PTY child exits (like `run_skill`). The caller
    /// settles the run the same way. If the executor doesn't support PTY,
    /// the default returns `Err` (the caller falls back to `run_skill`).
    ///
    /// **Three races** (orca §2.4, solved in the UI layer, not here):
    ///  - ACK backpressure (`ackData`): the UI throttles to prevent flooding.
    ///  - rendererDispatcherReady handshake: the UI buffers until xterm.js
    ///    signals ready (prevents reload losing bytes).
    ///  - resize re-assertion (`getAppliedSize`): the UI re-sends resize if
    ///    the PTY's applied size doesn't match.
    async fn run_skill_pty(
        &self,
        plan: &LaunchPlan,
        ctx: &RunCtx,
        bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
        input_rx: mpsc::UnboundedReceiver<PtyInput>,
    ) -> Result<SkillOutput, ExecError> {
        // Default: PTY not supported. The caller falls back to run_skill.
        let _ = (plan, ctx, bytes_tx, input_rx);
        Err(ExecError::Failed(
            "PTY not supported by this executor (use run_skill instead)".into(),
        ))
    }
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
            // Can't wait on the claude process — osascript returns immediately
            // after telling Terminal to open. Sleeping to timeout then claiming
            // `completed = true` would be a false-success (违反「读回为证」).
            // Stay honest: report not-completed so the issue stays InProgress
            // (never auto-Done) and the human verifies in Terminal instead.
            return Ok(SkillOutput {
                completed: false,
                summary: "未验证：osascript 启动 Terminal 后拿不到 claude 句柄，无法等待其退出。请在 Terminal 里确认会话结束后手动推进，buddy 不替你判定完成。".to_string(),
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

    async fn run_skill_resume(
        &self,
        plan: &LaunchPlan,
        ctx: &RunCtx,
    ) -> Result<SkillOutput, ExecError> {
        // Resume uses the same spawn logic as the first run — the LaunchPlan
        // carries the resume args (`--resume <session_id>` or `--continue`
        // fallback, instead of the startup plan's `--append-system-prompt
        // <bridge> <skill_body>`). The process spawn, terminal window, and
        // wall-clock timeout are identical.
        self.run_skill(plan, ctx).await
    }

    /// V1 Issue2 W2 (§9) + next 切片三B(PTY 平台接缝提取):spawn `claude`
    /// in a PTY and stream bytes. 平台分叉不再在这个函数体里——由
    /// [`crate::pty_backend::active`] 选一份平台实现(Windows:
    /// conpty-oxide,函数体整段搬自本方法提取前的版本,零逻辑改写;Unix:
    /// 本片新写的 portable-pty 实现)。这里只做两件与平台无关的事:解析
    /// claude 可执行路径(`self.claude_binary` 覆盖 / `plan.binary` 兜底)、
    /// 把解析好的值连同 `plan`/两个 channel 转交给选中的后端。
    ///
    /// 之前的版本里这个方法整个挂 `#[cfg(windows)]`,非 Windows 落到 trait
    /// 默认实现「PTY not supported」——现在两个平台都有真实后端,不再需要
    /// 这个 cfg 门,详见 `pty_backend` 模块文档。
    async fn run_skill_pty(
        &self,
        plan: &LaunchPlan,
        _ctx: &RunCtx,
        bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
        input_rx: mpsc::UnboundedReceiver<PtyInput>,
    ) -> Result<SkillOutput, ExecError> {
        let binary = self.claude_binary.as_deref().unwrap_or(&plan.binary);
        crate::pty_backend::active()
            .run(binary, plan, bytes_tx, input_rx)
            .await
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
                // is the real evidence. On Windows/Linux the spawned child
                // (terminal+claude) is killed here via `kill_on_drop` when
                // `child` drops on return; on macOS `osascript` already
                // returned immediately and the Terminal app is independent
                // (not a child), so it stays open — only the wall-clock
                // deadline fires.
                Ok(SkillOutput {
                    completed: true,
                    summary: "(wall-clock timeout)".to_string(),
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

    async fn run_skill_resume(
        &self,
        _plan: &LaunchPlan,
        _ctx: &RunCtx,
    ) -> Result<SkillOutput, ExecError> {
        // Resume: no placeholder write (the first run already wrote it).
        // Self-labeled 【mock】 — never pretend to be real execution.
        Ok(SkillOutput {
            completed: true,
            summary: "【mock】交互式技能会话 resume 完成(流程演示,未真 spawn claude)".to_string(),
        })
    }

    /// V1 Issue2 Phase2b: mock PTY mode. Sends a few 【mock】 bytes (so the
    /// UI has something to render in tests) and drains `input_rx`. Never
    /// spawns a real PTY — self-labeled, never pretends to be real execution.
    async fn run_skill_pty(
        &self,
        plan: &LaunchPlan,
        _ctx: &RunCtx,
        bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
        mut input_rx: mpsc::UnboundedReceiver<PtyInput>,
    ) -> Result<SkillOutput, ExecError> {
        let _ = write_mock_metrics_toml(&plan.cwd);

        // Send mock output so the UI has something to render in tests.
        let _ = bytes_tx.send(
            "【mock】pty output (no real claude spawned)\r\n"
                .as_bytes()
                .to_vec(),
        );

        // Drain input (the UI may send resize/input — ignore, just consume).
        while input_rx.try_recv().is_ok() {}

        Ok(SkillOutput {
            completed: true,
            summary: "【mock】pty 交互式技能会话完成(流程演示,未真 spawn claude)".to_string(),
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
        // P5 · 2026-08-06: output-file-not-stdout must be in the prompt an
        // agent actually receives, not just in docs it may not read.
        assert!(prompt.contains("完全不看脚本的 stdout"));
    }

    #[test]
    fn build_bridge_system_prompt_unknown_skill_still_has_generic_rules() {
        let ctx = mock_playbook_ctx();
        let prompt = build_bridge_system_prompt(&ctx, "some-unknown-skill");
        assert!(prompt.contains("Done 永不自动"));
        assert!(prompt.contains("通用铁律"));
        // 非空(含未知 typo)slug 仍显「你正在执行技能」,让用户自查。
        assert!(prompt.contains("你正在执行技能: `some-unknown-skill`"));
    }

    #[test]
    fn build_bridge_system_prompt_empty_skill_shows_unspecified_not_skill_label() {
        // V1 收口:无技能 issue 也走终端 —— 空 slug 不报错,bridge 显
        // 「未关联技能」(不假装在执行某技能),通用铁律仍在。
        let ctx = mock_playbook_ctx();
        let prompt = build_bridge_system_prompt(&ctx, "");
        assert!(prompt.contains("Done 永不自动"));
        assert!(prompt.contains("未关联技能"));
        assert!(!prompt.contains("你正在执行技能"));
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

    #[test]
    fn build_resume_plan_claude_with_session_id() {
        let tmp = tempfile_dir();
        let plan = build_resume_plan(&CLAUDE, Some("abc-123-session-id"), &tmp)
            .expect("claude is supported");
        assert_eq!(plan.binary, "claude");
        // --resume <session_id> --dangerously-skip-permissions --disallowedTools "Bash(gh pr merge)"
        assert!(plan.args.contains(&"--resume".to_string()));
        assert!(plan.args.contains(&"abc-123-session-id".to_string()));
        assert!(plan
            .args
            .contains(&"--dangerously-skip-permissions".to_string()));
        assert!(plan.args.contains(&"--disallowedTools".to_string()));
        assert!(plan.args.contains(&"Bash(gh pr merge)".to_string()));
        // No startup-plan artifacts: no --append-system-prompt, no positional
        // skill body (resume re-enters the existing session).
        assert!(!plan.args.iter().any(|a| a == "--append-system-prompt"));
        assert!(!plan.args.contains(&"--continue".to_string())); // precise resume, not fallback
        assert_eq!(plan.args.len(), 5); // resume_flag + id + yolo + disallowedTools + value
                                        // Env vars stripped
        assert!(!plan.env.contains_key("ANTHROPIC_AUTH_TOKEN"));
        assert!(!plan.env.contains_key("CLAUDECODE"));
        // Cwd preserved
        assert_eq!(plan.cwd, tmp);
    }

    #[test]
    fn build_resume_plan_claude_fallback_continue() {
        let tmp = tempfile_dir();
        // No session_id → fallback to --continue (the 2a behavior).
        let plan = build_resume_plan(&CLAUDE, None, &tmp).expect("claude is supported");
        assert_eq!(plan.binary, "claude");
        assert!(plan.args.contains(&"--continue".to_string()));
        assert!(!plan.args.contains(&"--resume".to_string()));
        assert!(plan
            .args
            .contains(&"--dangerously-skip-permissions".to_string()));
        assert!(plan.args.contains(&"--disallowedTools".to_string()));
        assert_eq!(plan.args.len(), 4); // continue + yolo + disallowedTools + value
    }

    #[test]
    fn build_resume_plan_claude_empty_session_id_falls_back() {
        let tmp = tempfile_dir();
        // Empty session_id → treated as None → --continue fallback.
        let plan = build_resume_plan(&CLAUDE, Some(""), &tmp).expect("claude is supported");
        assert!(plan.args.contains(&"--continue".to_string()));
        assert!(!plan.args.contains(&"--resume".to_string()));
    }

    #[test]
    fn build_resume_plan_cursor_unsupported() {
        let tmp = tempfile_dir();
        let result = build_resume_plan(&CURSOR, Some("id"), &tmp);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cursor"));
        assert!(err.to_string().contains("暂不支持"));
    }

    #[test]
    fn build_consultation_resume_plan_appends_rules() {
        let tmp = tempfile_dir();
        let plan = build_consultation_resume_plan(&CLAUDE, Some("abc-123-session-id"), &tmp)
            .expect("claude is supported");
        assert_eq!(plan.binary, "claude");
        assert!(plan.args.contains(&"--resume".to_string()));
        assert!(plan.args.contains(&"abc-123-session-id".to_string()));
        // Consultation-only: --append-system-prompt with §6.2 rules.
        let append_idx = plan
            .args
            .iter()
            .position(|a| a == "--append-system-prompt")
            .expect("consultation plan must append system prompt");
        assert_eq!(
            plan.args.get(append_idx + 1).map(String::as_str),
            Some(CONSULTATION_APPEND_PROMPT)
        );
        assert!(CONSULTATION_APPEND_PROMPT.contains("新建一件活"));
        assert!(!CONSULTATION_APPEND_PROMPT.contains("只读"));
        // Same permission posture as delivery resume.
        assert!(plan
            .args
            .contains(&"--dangerously-skip-permissions".to_string()));
        assert!(plan.args.contains(&"--disallowedTools".to_string()));
        assert!(plan.args.contains(&"Bash(gh pr merge)".to_string()));
        // No positional skill body / no --continue (precise resume).
        assert!(!plan.args.contains(&"--continue".to_string()));
        assert!(!plan.submit_prompt);
        assert_eq!(plan.cwd, tmp);
    }

    #[test]
    fn build_resume_plan_stays_without_consultation_append() {
        // Delivery resume must NOT inject consultation rules — only
        // open_conversation uses build_consultation_resume_plan.
        let tmp = tempfile_dir();
        let plan = build_resume_plan(&CLAUDE, Some("delivery-session"), &tmp).unwrap();
        assert!(!plan.args.iter().any(|a| a == "--append-system-prompt"));
        assert!(!plan
            .args
            .iter()
            .any(|a| a.contains("新建一件活") || a == CONSULTATION_APPEND_PROMPT));
    }

    #[test]
    fn build_consultation_resume_plan_cursor_unsupported() {
        let tmp = tempfile_dir();
        let result = build_consultation_resume_plan(&CURSOR, Some("id"), &tmp);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn mock_executor_resume_returns_completed_no_placeholder_write() {
        let tmp = tempfile_dir();
        // Simulate a first run that wrote the placeholder.
        let skill_body = "# 找指标\n\nskill body";
        let bridge = "bridge prompt";
        let startup_plan = build_startup_plan(&CLAUDE, skill_body, bridge, &tmp).unwrap();
        let ctx = RunCtx {
            project: bw_core::ProjectId::nil(),
            workflow: bw_core::WorkflowId::nil(),
        };
        let mock = MockInteractiveExecutor::new();
        let _ = mock.run_skill(&startup_plan, &ctx).await.unwrap();
        let metrics_path = tmp.join(".bw").join("metrics.toml");
        let mtime_before = std::fs::metadata(&metrics_path)
            .expect("placeholder exists from first run")
            .modified()
            .unwrap();

        // Resume: should NOT rewrite the placeholder (it already exists).
        let resume_plan = build_resume_plan(&CLAUDE, Some("session-id-123"), &tmp).unwrap();
        let output = mock.run_skill_resume(&resume_plan, &ctx).await.unwrap();
        assert!(output.completed);
        assert!(output.summary.contains("【mock】"));
        assert!(output.summary.contains("resume"));
        // The placeholder file was NOT rewritten (mtime unchanged).
        let mtime_after = std::fs::metadata(&metrics_path)
            .expect("placeholder still exists")
            .modified()
            .unwrap();
        assert_eq!(mtime_before, mtime_after);
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
