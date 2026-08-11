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
///
/// **主控裁决(死代码审计「需人判断」项,留+登记组,2026-08-11)**:今天
/// 只有一个变体,`match` 在唯一消费点(`build_startup_plan`)因此看起来
/// 像多余的一层包装——这是**第二家 CLI 的脚手架**,不是死代码:一旦真
/// 的接入一家注入方式不同的 CLI(比如不接受位置参数、只认某个
/// `--prefill` 式旗标的),新加一个变体 + `build_startup_plan` 里补一条
/// 匹配分支就够了,不用回头改调用方的类型签名。裁决是留着,不折叠成一
/// 个只有一种取值的普通类型——第二家 CLI 接入之前,这条登记本身就是它
/// 存在的理由说明。
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
    /// 跳过逐次授权确认的旗标(claude:`--dangerously-skip-permissions`)。
    /// next 切片三-2 修:改 `Option`(原来是空串 `""` 表示「没有」——一个
    /// 空字符串会被当成一个真实 argv 元素推进去,是隐患;`None` 才是「这
    /// 家没有这个旗标」的正确表达,与 `system_prompt_flag`/`session_id_flag`
    /// 同型)。`None` 时 [`build_startup_plan`]/[`build_resume_plan`] 都不
    /// 推这个 arg。
    pub yolo_flag: Option<&'static str>,
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

    // ── next 切片三C(design-s3-agentcli.md §5.2):原来写死在
    // `build_startup_plan`/`build_resume_plan` 函数体里的三处 claude 专属
    // 字面量,包一层搬进这一行——第二家 CLI 若旗标不同名,加一行就够了,不
    // 用再改函数体。──
    /// 系统提示词注入旗标(claude:`--append-system-prompt`,2026-08-10 核
    /// `claude --help` 属实)。`None` = 这家不支持系统提示词注入——复利块
    /// 只能走位置 prompt,如实降级,绝不假装注入了。
    pub system_prompt_flag: Option<&'static str>,
    /// 由 BW 指派会话号的旗标(claude:`--session-id`,同日核对属实;
    /// next 切片三-1 已用一次真实交互式会话验证过这个旗标在非 `--print`
    /// 模式下真被接受,见任务报告的实测小节)。`None` = 这家不能指派会话
    /// 号,只能靠上游目录反查(`agentcli::session::discover_sessions`)兜底。
    ///
    /// **next 切片三-2 修的坑,记在这里免得下一个人踩第二次**:切片三C/D
    /// 把这个字段加进来之后,`build_startup_plan` 一度**从没读过它**——
    /// `agentcli::connector::AgentCliConnector::start` 首启时 BW 自己编了
    /// 一个 uuid 塞进 `SessionRow.upstream_session`/`ExecTicket`,但从没把
    /// 这个 uuid 交给 claude(argv 里根本没有 `--session-id`),claude 会
    /// 用它自己生成的会话号落 jsonl——票据上的 `upstream_session` 是一句
    /// 谎话,读回必空。`build_startup_plan` 现在多了一个 `session_id` 形
    /// 参,只有这个字段是 `Some` 且调用方真给了值,才会把它塞进 argv;字
    /// 段/参数任一为空,`agentcli::connector` 那边就如实把
    /// `SessionRow.upstream_session` 留成空串,不再编一个猜的号。
    pub session_id_flag: Option<&'static str>,
    /// 起手就禁掉的工具列表。**如实口径:这是篱笆不是墙**(v1 原注释已
    /// 写明:禁得掉 `gh pr merge`,禁不掉等价的其它写法),真约束在系统
    /// 提示词与 `bw-core` 状态机。空切片 = 这家没有配对应的禁用规则。
    pub deny_tools: &'static [&'static str],

    /// 原字段名 `supported`,next 切片三C 改名 + 语义钉死:**只有在本机
    /// 真跑过 `--help` 逐项核对过参数的行才准置 `true`**。「登记了」≠
    /// 「能跑」,与连接器那条「装了≠连上了」同一口径——`build_startup_plan`/
    /// `build_resume_plan` 拒绝为 `verified = false` 的行装配计划。
    pub verified: bool,
}

/// The claude CLI. `verified = true` — we can really spawn it.
pub static CLAUDE: TuiAgentConfig = TuiAgentConfig {
    slug: "claude",
    detect_cmd: "claude --version",
    launch_cmd: "claude",
    // The skill body goes in as the positional prompt (claude has no
    // `--prefill`; see `PromptInjectionMode::PositionalArgv`).
    prompt_injection_mode: PromptInjectionMode::PositionalArgv,
    yolo_flag: Some("--dangerously-skip-permissions"),
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
    // next 切片三C:2026-08-10 核 `claude --help` 属实。
    system_prompt_flag: Some("--append-system-prompt"),
    // next 切片三-1 已用一次真实交互式会话验证过(见任务报告):BW 自己
    // 指派一个 uuid,claude 真的用它落 session jsonl,不是仅登记未核对。
    session_id_flag: Some("--session-id"),
    deny_tools: &["Bash(gh pr merge)"],
    verified: true,
};

/// Cursor placeholder. Not verified in Phase 1 — calling
/// [`build_startup_plan`] with this config returns an honest error(design
/// §5.3:本机没装 cursor,`which cursor` 未找到,参数没法核对,不许标
/// `verified: true` 假装能跑)。
pub static CURSOR: TuiAgentConfig = TuiAgentConfig {
    slug: "cursor",
    detect_cmd: "cursor --version",
    launch_cmd: "cursor",
    prompt_injection_mode: PromptInjectionMode::PositionalArgv,
    // next 切片三-2 修(M4):`None` 而不是空串——空串会被当成一个真实的
    // argv 元素推进去(隐患),`None` 才是「没有」。cursor 的 verified=false
    // 让这行今天永远走不到装配那一步,但类型上仍要如实。
    yolo_flag: None,
    resume_flag: "--continue",
    resume_id_flag: "--resume",
    system_prompt_flag: None,
    session_id_flag: None,
    deny_tools: &[],
    verified: false,
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
/// For claude (`verified = true`,给了 `session_id`):
/// `claude --append-system-prompt <system_prompt> --session-id <session_id>
///  <position_prompt> --dangerously-skip-permissions
///  --disallowedTools "Bash(gh pr merge)"`
///
/// - `system_prompt` → `--append-system-prompt`(caller 传 bridge prompt +
///   技能正文 + 蒸馏/目录块,见 `run_issue_interactive`)。
/// - `session_id` → `--session-id`(next 切片三-2 修新增形参;`agent.
///   session_id_flag` 为 `None`,或这里传 `None`/空串,都不推这个
///   arg——**如实降级,绝不把 BW 编的号硬塞给 claude**。caller
///   [`crate::agentcli::connector::AgentCliConnector::start`] 只在自己真
///   打算让 claude 采纳这个号时才传 `Some`,见该函数文档)。
/// - `position_prompt` → 位置 prompt(auto-submit 首句用户消息;caller 传
///   issue 标题+描述)。
///
/// No `-p`/`--print` (interactive), no `--max-budget-usd` (interactive
/// sessions are user-paced, no per-call cap). The env is inherited from
/// the process but the nested-execution vars are stripped (same as
/// [`crate::ClaudeCliExecutor`]) so the child uses its own CLI config.
///
/// **缺口清偿(2026-08-10,清偿 plan/23-opc-stitching-rebuild.md §10 第 3
/// 条),如实更新口径**:这条「剥离」曾经是句空话——`LaunchPlan.env` 这个
/// `HashMap` 本身删对了键,但真实子进程(`pty_backend::unix::UnixPtyBackend`)
/// 起步时用 `portable_pty::CommandBuilder::new` 已经整份继承了当前进程环
/// 境,之前只对剩下的键调 `cmd.env(k, v)`,从没清空过,删掉的键因此照样漏
/// 进子进程。现在 `pty_backend.rs` 的 unix/windows 两份后端起步都先
/// `env_clear()` 再整个套用 `plan.env`(见该文件 `unix`/`windows` 两个子模
/// 块的行内注释),这个 `HashMap` 真正成了子进程环境的唯一来源——`pty_smoke`
/// 指挥器的 env-strip 探针节是这句话的确定性证据(不依赖网关,每次门禁都
/// 跑)。**范围如实标注**:这条清偿只覆盖 `run_skill_pty`(真实交互式会话
/// 的通道)。`InteractiveCliExecutor::run_skill` 的 `tokio::process::Command`
/// 路径(非 PTY,v1 零改写移植件,当前不是 agentcli 层的真实交互通道)同一
/// 模式的隐患未动,按既有规矩留给读到这条登记的下一个会话裁决。
pub fn build_startup_plan(
    agent: &TuiAgentConfig,
    position_prompt: &str,
    system_prompt: &str,
    session_id: Option<&str>,
    workspace_cwd: &Path,
) -> Result<LaunchPlan, ExecError> {
    if !agent.verified {
        return Err(ExecError::Failed(format!(
            "TUI agent '{}' 暂不支持(Phase 1 仅 claude)",
            agent.slug
        )));
    }

    // Strip nested-execution env vars (same rationale as ClaudeCliExecutor:
    // the host may be running inside a Claude Code session whose injected
    // tokens/gateway/model alias cause 401 in the child).
    //
    // **如实更新口径(缺口清偿轮,2026-08-10)**:这一步删的是
    // `LaunchPlan.env` 这个 `HashMap` 本身——它现在真的是子进程环境的唯一
    // 来源了。原因不在这里:`pty_backend::unix::UnixPtyBackend::run`/
    // `windows::WindowsPtyBackend::run` 起步时都已经先 `env_clear()` 再整
    // 个套用这个 `HashMap`,不再是「先整份继承父进程环境、再对剩下的键逐
    // 条覆盖」——这里删掉的键因此真的从子进程环境里消失了。曾经的空话已
    // 消灭于 `pty_backend.rs` 的这次修复(commit 见
    // `plan/23-opc-stitching-rebuild.md` §10 第 3 条的清偿记录)。
    // `InteractiveCliExecutor::run_skill` 的 `tokio::process::Command` 路径
    // (非 PTY,当前不是 agentcli 层的真实交互通道)不在这次清偿范围内,同
    // 一模式的隐患是否要修留给下一个会话裁决。
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

    let mut args = Vec::with_capacity(9);
    // next 切片三C(design §5.2):系统提示词旗标从写死的字面量改读
    // `agent.system_prompt_flag`。`None`(这家不支持)或 `system_prompt`
    // 为空(调用方没给任何内容,§6.2「空就是空」)都不推这两个 arg——如实
    // 降级,绝不假装注入了。caller 组装的内容不变:bridge(项目上下文+
    // 铁律+技能契约)+ 技能正文 + 蒸馏/目录块。
    if let Some(flag) = agent.system_prompt_flag {
        if !system_prompt.is_empty() {
            args.push(flag.to_string());
            args.push(system_prompt.to_string());
        }
    }
    // next 切片三-2 修(C1):会话号真接线。之前这里完全没有这段——
    // `agentcli::connector::AgentCliConnector::start` 会在内存里编一个
    // uuid 塞进 `SessionRow.upstream_session`/票据,但从来没把它交给
    // claude,argv 里压根没有 `--session-id`,claude 会用自己生成的号落
    // jsonl,票据上的号是一句谎话。现在:旗标存在(`agent.
    // session_id_flag` 是 `Some`)且调用方真给了非空 `session_id`,才推
    // 这两个 arg——两个条件缺一个都不推,如实,不猜。位置紧跟在系统提示
    // 词之后、位置 prompt 之前,和「身份/上下文类旗标在前,用户消息在
    // 后」的顺序一致。
    if let Some(flag) = agent.session_id_flag {
        if let Some(id) = session_id.filter(|s| !s.is_empty()) {
            args.push(flag.to_string());
            args.push(id.to_string());
        }
    }
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
    // the terminal and can intervene at any time). next 切片三-2 修
    // (M4):`yolo_flag` 现在是 `Option`,`None` 就不推(空串曾经会被当
    // 成一个真实 argv 元素推进去,是隐患)。
    if let Some(flag) = agent.yolo_flag {
        args.push(flag.to_string());
    }
    // next 切片三C(design §5.2):起手禁掉的工具改读 `agent.deny_tools`——
    // 「篱笆不是墙」,真约束在衔接层 system prompt 与 `bw-core` 状态机(见
    // `TuiAgentConfig::deny_tools` 文档)。claude 行今天只有一条
    // `Bash(gh pr merge)`,循环产出与原来手写的两行字面量逐字节一致
    // (合并永远是人,铁律)。
    for tool in agent.deny_tools {
        args.push("--disallowedTools".to_string());
        args.push((*tool).to_string());
    }

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
    if !agent.verified {
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
    // posture as the first run + deny `gh pr merge` (验收 = 人 merge, 铁律)。
    let mut args = Vec::with_capacity(5);
    if let Some(id) = session_id.filter(|s| !s.is_empty()) {
        args.push(agent.resume_id_flag.to_string());
        args.push(id.to_string());
    } else {
        args.push(agent.resume_flag.to_string());
    }
    // next 切片三-2 修(M4):`yolo_flag` 现在是 `Option`,同
    // `build_startup_plan` 的口径,`None` 不推。
    if let Some(flag) = agent.yolo_flag {
        args.push(flag.to_string());
    }
    // next 切片三C(design §5.2):同 `build_startup_plan`,读
    // `agent.deny_tools` 而不是写死的字面量——两处装配出同一条 claude 行的
    // 禁用工具清单,不会因为改了一处漏改另一处而分叉。
    for tool in agent.deny_tools {
        args.push("--disallowedTools".to_string());
        args.push((*tool).to_string());
    }

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
///
/// **next 切片三C 未动这里的 `--append-system-prompt` 字面量**——design
/// §6.4 明文这条路径「保留移植原样、不接线」(要活的状态才谈得上,状态要
/// 存储,存储是切片四/五的事)。三处字面量的提取范围钉在
/// `build_startup_plan`/`build_resume_plan`(design §5.1 原话点名的
/// `build_startup_plan`),这里不是「漏改」,是范围裁剪:接线那天(要用
/// `agent.system_prompt_flag`)再一并处理,不在这个不接线的路径上先改一半。
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
    /// next 切片三-2 修(I5):子进程的真实退出码,能拿到就拿,拿不到就
    /// 如实 `None`——不是每条路径都能观测到:`InteractiveCliExecutor::
    /// run_skill`(非 PTY 路径)`child.wait()` 就在手边,直接读;
    /// `pty_backend::unix::UnixPtyBackend` 收尾处原来 `let _ =
    /// child.wait();` 把状态丢了,本轮改成在 `spawn_blocking` 里捕获并
    /// 经这个字段回传;`pty_backend::windows::WindowsPtyBackend` 收尾只
    /// `child.kill()`、从未 `wait()` 过(零改写约束下的 Windows 整段搬运
    /// 件,本轮不碰它的收尾逻辑),这里如实 `None`,不假装量出来一个。
    /// mock 执行器同样如实 `None`——退出码是「真进程」的概念,自我标注
    /// 的【mock】路径没有这个东西。
    pub exit_code: Option<i32>,
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
/// a new console window automatically. On macOS/Linux, spawn directly and
/// wait on that same process (best-effort — may not open a visible terminal
/// window depending on how buddy itself was launched).
///
/// **减法(next 切片三-1 修,design-s3-agentcli.md §7.4)**: macOS 曾经走
/// `osascript` 开系统 Terminal 的一条独立分支——但那条分支自己的旧注释就
/// 写明拿不到 claude 的进程句柄、等不到它退出,所以只能诚实报
/// `completed = false`,永远靠人手动推进。Unix PTY 后端
/// (`pty_backend::unix`,next 切片三B)补齐之后,交互式会话真正的通道是
/// `run_skill_pty`(能起、能双向倒腾字节、能判定退出),这条判定不了完成
/// 的旧回落分支失去存在理由——按 `CLAUDE.md`「发现过时的实现路径直接
/// 移除」删掉,macOS 现在落到与 Linux 相同的直接 spawn 分支,不再留一条
/// 体验不同、结果也判定不了的兼容路径。
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
        // agent). On macOS/Linux, spawn directly (best-effort — may not
        // open a visible terminal window; this OS-terminal path isn't the
        // real interactive channel anyway — that's `run_skill_pty`, §4
        // agentcli 层). macOS previously had its own `osascript` branch
        // here; removed (see this struct's doc comment for why — design
        // §7.4 减法, `CLAUDE.md`「不为向后兼容留旧路径」).
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
        #[cfg(not(target_os = "windows"))]
        {
            // macOS/Linux/other: spawn directly (best-effort — may not open
            // a visible terminal window without an xterm/gnome-terminal-style
            // wrapper on Linux, and on macOS the process simply isn't attached
            // to any Terminal.app window). Same completion detection as
            // Windows: wait on the process, wall-clock timeout backstop.
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
            // next 切片三-2 修(I5):这条路径的退出码本来就在手边
            // (`std::process::ExitStatus::code()`),之前直接用 `_status`
            // 扔掉——顺手接上,不是新观测手段,只是不再丢已经有的事实。
            Ok(Ok(status)) => Ok(SkillOutput {
                completed: true,
                summary: String::new(),
                exit_code: status.code(),
            }),
            Ok(Err(e)) => Err(ExecError::Failed(format!(
                "interactive terminal error: {e}"
            ))),
            Err(_) => {
                // Timeout — declare completed. The worktree's git state
                // is the real evidence. The spawned child (terminal+claude)
                // is killed here via `kill_on_drop` when `child` drops on
                // return — all platforms take this same path now (the macOS
                // `osascript` special case that spawned an independent,
                // unwaitable Terminal process was removed; see `run_skill`'s
                // struct doc comment).
                Ok(SkillOutput {
                    completed: true,
                    summary: "(wall-clock timeout)".to_string(),
                    // 超时兜底:压根没等到退出,没有退出码可言,如实 None。
                    exit_code: None,
                })
            }
        }
    }
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
            // next 切片三-2 修(I5):mock 从不真 spawn 进程,退出码是「真进
            // 程」的概念——如实 None,不假装量出来一个。
            exit_code: None,
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
            exit_code: None,
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
            exit_code: None,
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
