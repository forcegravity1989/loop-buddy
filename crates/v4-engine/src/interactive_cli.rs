//! Interactive CLI engine (V1 Issue 2 Phase 1, option (c)).
//!
//! Launches a TUI agent (claude CLI) in an interactive terminal session,
//! pre-loaded with a skill body (as the first user message) and a buddy
//! system prompt (static `system-prompt.md` + dynamic project context).
//! One skill = one interactive session (§2.3) — no phase splitting, no
//! adversarial loop.
//!
//! 不解析 `session.jsonl`(§2.5 砍了对话摘要 collector)—— 终端回滚区加
//! `session.jsonl` 本身就是记录。干没干成看仓里的真状态(git、MR),不看这里。
//!
//! **V4 只有内嵌终端这一条路**:[`InteractiveCliExecutor`] 干活全走
//! `run_skill_pty` → [`crate::pty_backend`],所有平台一样。它的 `run_skill`
//! (从 V3 拷来的那条「起一个系统终端窗口」的老路)已经如实退场,只剩一句错误
//! —— 拆掉它的原委见 [`InteractiveCliExecutor`] 的说明。真正还在用 `run_skill`
//! 的只有自我标注的 [`MockInteractiveExecutor`](headless 指挥器走它)。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
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

/// 宿主会话注入的变量,**按前缀剥,不按名字列**。
///
/// 从 buddy 自己被一个 Claude Code 会话启动时(开发期与试点期就是这样),宿主
/// 会往环境里塞一大批 `CLAUDE…` 打头的变量。这里原本列了固定的四个名字,而实测
/// 宿主注入了十几个 —— 漏掉的 `CLAUDE_CODE_MESSAGING_SOCKET` /
/// `CLAUDE_CODE_MESSAGING_TOKEN` / `CLAUDE_CODE_HOST_SESSION_ID` 足以让子
/// `claude` **接回宿主会话、读到宿主的对话内容**(2026-08-21 实测:只剥四个时,
/// 子进程复述了父进程正在跑的脚本原文;补齐这三个之后干净)。
///
/// 按名字列的做法保证会随宿主加变量而再次腐烂,所以改成前缀:凡是 `CLAUDE`
/// 打头的一律不进子进程 —— 它们都是**父会话的身份**,对子进程只有害处。
/// 真实用户从 Finder 双击启动 buddy 时,这一族一个都不存在,剥了也不损失什么。
const HOST_ENV_PREFIX: &str = "CLAUDE";

/// 人自己配的厂商端点。默认剥(宿主的凭据同样不该漏进子进程),但试点后门
/// [`keep_anthropic_env`] 开着时豁免。
const VENDOR_ENV: &[&str] = &[
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
];

fn is_spawnable_env_key(key: &str) -> bool {
    !key.is_empty() && !key.contains('=') && !key.contains('\0')
}

/// **试点期的临时后门,用完删** —— 登记在 `docs/LEFTOVERS.md` 的「试点-9」。
///
/// 设了 `BW_KEEP_ANTHROPIC_ENV`(任意非空值)就**不剥** [`VENDOR_ENV`] 那三个,
/// 把人自己配的厂商端点原样透传给子 `claude`;`CLAUDE…` 那一族照剥不误
/// (它们是宿主会话的身份,漏进子进程一定串台,见 [`HOST_ENV_PREFIX`])。
///
/// **它存在的唯一理由**:本机 `claude` 的登录过期、人一时不方便重登,想先拿
/// 另一个厂商的端点把旅程跑起来。buddy 的正常姿态是**不管**机器上的 `claude`
/// 怎么鉴权 —— 那是 CLI 自己该保证的事,buddy 只负责起进程。所以这不是一个
/// 产品功能,不进设置屏、不写进仓、不做界面提示。
fn keep_anthropic_env() -> bool {
    std::env::var_os("BW_KEEP_ANTHROPIC_ENV").is_some_and(|v| !v.is_empty())
}

/// 这个名字这一次要不要剥。宿主那一族永远剥;厂商那三个默认剥、后门开着时豁免。
fn is_stripped(key: &str) -> bool {
    if key.to_ascii_uppercase().starts_with(HOST_ENV_PREFIX) {
        return true;
    }
    !keep_anthropic_env() && VENDOR_ENV.iter().any(|v| key.eq_ignore_ascii_case(v))
}

fn child_env_from_process() -> HashMap<String, String> {
    std::env::vars()
        .filter(|(k, _)| is_spawnable_env_key(k))
        .filter(|(k, _)| !is_stripped(k))
        .collect()
}

/// 剥的是**本进程环境里真实存在的**那些名字 —— 前缀规则没法凭空枚举,只能照着
/// 父进程的环境逐个过。子命令的环境是从本进程继承来的,所以这个来源是对的。
pub(crate) fn apply_child_env(cmd: &mut impl EnvSink) {
    for key in std::env::vars().map(|(k, _)| k) {
        if is_spawnable_env_key(&key) && is_stripped(&key) {
            cmd.remove_env(&key);
        }
    }
}

/// Small sink so PTY and tokio Command share the same strip list without
/// replaying the full process map (that rebuild trips windows-spawn). The
/// PTY command types implement it in `pty_backend` (conpty-oxide on Windows,
/// portable-pty on unix) — same list, same rule on every platform.
pub(crate) trait EnvSink {
    fn remove_env(&mut self, key: &str);
}

impl EnvSink for tokio::process::Command {
    fn remove_env(&mut self, key: &str) {
        self.env_remove(key);
    }
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
/// the process but the nested-execution vars are stripped so the child uses
/// its own CLI config.
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

    // Inherit process env, minus nested-execution vars and Windows hidden
    // names (`=C:`). Do not replay this map through ConPTY `env()` — see
    // `apply_child_env`.
    let env = child_env_from_process();

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
    // 约束在 buddy 系统提示词(`docs/buddy/system-prompt.md`)里说清楚,
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

    let env = child_env_from_process();

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

// ─── Bridge system prompt ──────────────────────────────────────────────

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
        // 默认:这个执行器不支持内嵌终端(自我标注的替身就是这种)。如实
        // 报错,**不静默退化**成别的跑法 —— 走不走这条路由调用方的
        // `pty_enabled` 决定,不由这里偷偷改道。
        let _ = (plan, ctx, bytes_tx, input_rx);
        Err(ExecError::Failed("这个执行器没有内嵌终端(PTY)实现".into()))
    }
}

// ─── InteractiveCliExecutor(真跑 claude 的那个执行器) ─────────────────

/// 真执行器:把 claude 起在**内嵌终端**里(伪终端,见 [`crate::pty_backend`]),
/// 字节流交给会话屏渲染,人全程看得见、随时能停。
///
/// **它只实现 `run_skill_pty` 一条路。** 从 V3 拷过来的时候还带着另一条:
/// `run_skill` —— 起一个系统终端窗口、等它退出。那条路在 V4 里从第一天起就没
/// 有调用方(新壳建 App 时一律 `with_pty()`,`run_issue` 于是先走 PTY 分支;
/// 工作区不在时退回的那条又用的是自我标注的替身),而且它的 Windows 分支自相
/// 矛盾:注释说「从 GUI 程序起控制台进程会新开一个控制台窗口,用户能看见
/// agent」,代码用的却是 [`crate::win_cmd::tokio_cmd`] —— 那个辅助函数专门就是
/// 来按掉窗口的(`CREATE_NO_WINDOW`)。真跑起来会是:窗口不出现,stdio 继承自
/// 没有控制台的 GUI 父进程,交互式 claude 拿不到任何终端,然后当场报错或者静默
/// 挂到一小时的墙钟超时。
///
/// 所以 2026-08-21 把那条路整段删了,而不是二选一去修它 —— 两个修法(让控制台
/// 真出来 / 承认没有窗口并如实报错)都要先在 Windows 真机上把行为摸清楚,而这
/// 条路根本没人走。`run_skill` 现在只返回一句如实的错误。
pub struct InteractiveCliExecutor {
    /// Override the claude binary path (e.g. from `BW_CLAUDE_BIN`).
    /// `None` → use `LaunchPlan.binary` (which is `TuiAgentConfig.launch_cmd`).
    claude_binary: Option<String>,
}

impl InteractiveCliExecutor {
    pub fn new() -> Self {
        Self {
            claude_binary: None,
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
    /// 已退场。真执行器只走内嵌终端(`run_skill_pty`)—— 详见
    /// [`InteractiveCliExecutor`] 的说明。不静默退化成别的行为,如实报错。
    async fn run_skill(&self, _plan: &LaunchPlan, _ctx: &RunCtx) -> Result<SkillOutput, ExecError> {
        Err(ExecError::Failed(
            "真执行器只在内嵌终端里起 claude(run_skill_pty);起系统终端窗口那条路已经删了".into(),
        ))
    }

    /// 同 [`Self::run_skill`],已退场。接回上一场对话走的是
    /// `build_resume_plan` + `run_skill_pty`,不经这里。
    async fn run_skill_resume(
        &self,
        plan: &LaunchPlan,
        ctx: &RunCtx,
    ) -> Result<SkillOutput, ExecError> {
        self.run_skill(plan, ctx).await
    }

    /// Spawn the agent in a PTY and stream bytes. The platform split
    /// (conpty-oxide on Windows, portable-pty on macOS/Linux) lives in
    /// [`crate::pty_backend`]; this method only resolves the binary override
    /// and delegates.
    ///
    /// Flow: read loop drains PTY output → `bytes_tx`; `input_rx` bytes are
    /// written to the PTY, resizes applied; on first run (`plan.submit_prompt`)
    /// a `\r` is sent after a brief ready-wait to submit the positional skill
    /// body (claude interactive positional argv doesn't auto-submit in buddy's
    /// GLM-gateway env); child exit → kill (idempotent) → return completed.
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

// ─── MockInteractiveExecutor (for tests / no-claude environments) ──────

/// Mock interactive executor — never spawns a real terminal. Returns a
/// self-labeled `【mock】` `SkillOutput` and, if the plan's cwd is a real
/// writable directory, writes a placeholder `.bw/metrics.toml` so the
/// interactive path's downstream (SyncMetricsFile, artifact scan) has
/// something to chew on in tests / no-claude environments.
///
/// The self-labeled mock for the interactive path:
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
    async fn run_skill(&self, _plan: &LaunchPlan, _ctx: &RunCtx) -> Result<SkillOutput, ExecError> {
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
        _plan: &LaunchPlan,
        _ctx: &RunCtx,
        bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
        mut input_rx: mpsc::UnboundedReceiver<PtyInput>,
    ) -> Result<SkillOutput, ExecError> {
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

// **替身不写任何仓文件。** 它以前会往 `.bw/metrics.toml` 写一份占位的指标正本
// (还是 `schema_version = 1` 的旧格式),于是替身跑一次就在人的仓里留下一份
// 内容是假的、格式是过期的正本,而界面会把它**当正本读**。替身存在的唯一目的
// 是廉价验证管线本身,**绝不冒充真实执行、更不该替人写正本** —— 要留痕就写进
// 活的正文里。2026-08-21 整段删除。

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    use super::*;

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
        assert!(plan.env.keys().all(|k| is_spawnable_env_key(k)));
        // Cwd preserved
        assert_eq!(plan.cwd, tmp);
    }

    #[test]
    fn spawnable_env_key_rejects_windows_hidden_names() {
        assert!(!is_spawnable_env_key(""));
        assert!(!is_spawnable_env_key("=C:"));
        assert!(!is_spawnable_env_key("=ExitCode"));
        assert!(is_spawnable_env_key("PATH"));
        assert!(is_spawnable_env_key("BW_CLAUDE_BIN"));
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

    #[tokio::test]
    async fn mock_executor_returns_completed_and_writes_placeholder() {
        let tmp = tempfile_dir();
        let skill_body = "# 找指标\n\nskill body";
        let bridge = "bridge prompt";
        let plan = build_startup_plan(&CLAUDE, skill_body, bridge, &tmp).unwrap();
        let ctx = RunCtx {
            project: uuid::Uuid::nil(),
            workflow: uuid::Uuid::nil(),
        };
        let mock = MockInteractiveExecutor::new();
        let output = mock.run_skill(&plan, &ctx).await.unwrap();
        assert!(output.completed);
        assert!(output.summary.contains("【mock】"));
        // **替身跑完之后,人的仓里不该多出任何东西。** 它以前会写一份占位的
        // `.bw/metrics.toml`,现在这条断言反过来守着「一个字都不写」。
        assert!(
            !tmp.join(".bw").exists(),
            "替身不该往仓里写任何文件,更不该写指标正本"
        );
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

    #[tokio::test]
    async fn mock_executor_never_writes_into_the_repo() {
        // 续接那一路同样一个字都不写。**替身的唯一职责是返回一段自我标注的
        // 输出**,不是替人在仓里留东西。
        let tmp = tempfile_dir();
        let ctx = RunCtx {
            project: uuid::Uuid::nil(),
            workflow: uuid::Uuid::nil(),
        };
        let mock = MockInteractiveExecutor::new();
        let resume_plan = build_resume_plan(&CLAUDE, Some("session-id-123"), &tmp).unwrap();
        let output = mock.run_skill_resume(&resume_plan, &ctx).await.unwrap();
        assert!(output.completed);
        assert!(output.summary.contains("【mock】"));
        assert!(!tmp.join(".bw").exists(), "续接那一路也不许往仓里写");
    }

    /// Create a temp directory for test isolation.
    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bw-interactive-test-{}-{}",
            std::process::id(),
            // 进程内自增,不用时间戳:并行跑的两个测试可能在同一纳秒取到同一个
            // 名字,于是共用一个临时目录、互相看见对方写的文件(这就是
            // LEFTOVERS 里「减负-21」那条偶发失败的根因)。
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
