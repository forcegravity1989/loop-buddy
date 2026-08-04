# 预研报告:交互式 claude CLI 执行器(A 方案·嵌终端)

> 日期:2026-08-04 · worktree: `agent-af7dc61e6f61d94a1`
> 目标:验能否把后台 one-shot `claude -p` 换成交互式 claude CLI(用户在 app 内嵌终端里多轮对话),skill 注入引导,跑完 evidence 回流到 issue 卡。
> 纪律:守 CLAUDE.md 铁律(UI 无关内核 / 不碰派生链);不 push;预研 worktree 隔离,不动现有 one-shot 路径代码(只读,新加并列)。

---

## 0. 结论速览

| # | 问题 | Verdict | 一句话 |
|---|---|---|---|
| 1 | PTY spawn `claude`(不带 -p) | **可行** | `portable-pty` 0.9 在 Windows ConPTY 上已 spike 通过,读出 ANSI 字节流 |
| 2 | xterm.js 在 wry WebView 嵌 + 收发 | **可行(有条件)** | flow.rs 已证 `document::eval` 跑复杂 JS;xterm.js 纯 JS 可加载,但无持久双向通道,需轮询 |
| 3 | 事件总线桥(executor↔widget) | **可行(需新事件类型)** | 新增 `Event::TerminalBytes` + `Command::TerminalInput`,executor 持 PTY master 读字节推事件 |
| 4 | evidence 回流 | **可行** | 交互式 session.jsonl 一定写(`--no-session-persistence` 仅 `--print`);解析比 one-shot final text 丰富 |
| 5 | run_phase 契约适配 | **需要重新设计** | 交互式是用户驱动不定时,`run_phase` 阻塞到 claude 退出语义不合;建议不走 phase 拆分 |
| 6 | 权限/预算/安全 | **有条件可行** | `--permission-mode`/`--disallowedTools` 交互式仍认;`--max-budget-usd` **不认**(仅 `--print`) |

**总体 verdict:A 方案(嵌终端)技术上可行,主要风险在预算失控(Q6)和 run_phase 契约语义(Q5),不是 PTY/xterm 本身。建议下一步:做一个 `InteractiveExecutor` 原型,不走 `run_phase` 拆分,一个 skill 一个交互会话,跑完读 session.jsonl 收尾。**

---

## 1. PTY spawn `claude`(不带 -p)能不能跑起来

### Verdict:可行

### 证据

**最小 spike 已起,位置:`spike/pty-spike/`**

- `spike/pty-spike/Cargo.toml` — `portable-pty = "0.9"`,独立 workspace(不扰主 workspace)
- `spike/pty-spike/src/main.rs` — `native_pty_system()` 开 PTY,spawn `cmd /c echo hello-from-pty`,从 master 端读字节

**Spike 运行结果(Windows 11,ConPTY)**:

```
[pty-spike] read 4 bytes: "\u{1b}[6n"
```

`\u{1b}[6n` 是 ANSI DSR(Device Status Report)游标位置请求——ConPTY 初始化时发出的终端控制序列。这证明:

1. **`portable-pty` 0.9 在 Windows 上编译通过**(27 个依赖包全下载编译成功)
2. **ConPTY 被选中**(Windows 10+ 的伪终端 API;`\u{1b}[6n` 是 ConPTY 的标志性初始化序列)
3. **master 端可读出字节流**,且字节包含 ANSI 转义序列(正是 xterm.js 要渲染的东西)
4. **slave spawn 成功**,子进程在 PTY 里跑起来了

**crate 选型**:`portable-pty`(wezterm 作者出的,社区最成熟的跨平台 PTY crate)
- Windows:ConPTY(Windows 10 1809+)
- macOS:posix_openpt
- Linux:openpty
- API:`native_pty_system()` → `openpty(PtySize)` → `slave.spawn_command(CommandBuilder)` → `master.try_clone_reader()` → `reader.read(&mut buf)`

**关键 API 片段**(spike 实测可用):

```rust
let pty_system = portable_pty::native_pty_system();
let pair = pty_system.openpty(portable_pty::PtySize {
    rows: 24, cols: 80, pixel_width: 0, pixel_height: 0,
})?;
let mut cmd = portable_pty::CommandBuilder::new("claude");
cmd.arg("--permission-mode").arg("acceptEdits");
// ... 更多 flag
let child = pair.slave.spawn_command(cmd)?;
drop(pair.slave); // EOF 传播
let mut reader = pair.master.try_clone_reader()?;
let mut writer = pair.master.take_writer()?; // 写用户输入回 PTY
// reader 是 blocking std::Read — 需用 tokio::task::spawn_blocking 包
```

**Windows 上的坑(已踩)**:
- `CommandBuilder::new("cmd /c echo hello")` 不行——它把整个字符串当程序名。必须 `new("cmd").arg("/c").arg("echo").arg("hello")`。
- ConPTY 的 `cmd /c echo hello` 不会干净退出(reader 一直 block)——ConPTY 的 pseudo-console 在子进程退出后仍保持连接。**这恰恰说明交互式 claude(长驻 TUI)是 PTY 的正确用法**,而不是 `echo` 这种 one-shot。
- `BW_CLAUDE_BIN` 指向 claude.exe 全路径——`CommandBuilder::new( BW_CLAUDE_BIN 的值)` 直接可用,和现有 `claude_cli.rs:231` 同路径。

### 风险

- **blocking reader**:`portable-pty` 的 reader 是 `std::Read`(blocking),不是 async。在 tokio 体系里要用 `spawn_blocking` 包,或用 `tokio::io::AsyncRead` 适配(需自己实现或用 `tokio_util::io::ReaderStream`)。这是工程量,不是阻塞点。
- **ConPTY 最低版本**:Windows 10 1809(build 17763)。用户环境是 Windows 11 26100,没问题。但产品化时要检查版本。
- **PTY 尺寸**:初始 24x80;窗口 resize 时要调 `master.resize(PtySize)`,否则 claude TUI 渲染错位。

---

## 2. xterm.js 在 wry WebView 里能不能嵌 + 收发字节

### Verdict:可行(有条件)

### 证据

**buddy 已有 `document::eval` 驱动复杂 JS 的实证——`crates/app-desktop/src/flow.rs`:**

- `flow.rs:129` — `document::eval(&script).await` 执行 JS 脚本,返回 `serde_json::Value`
- `flow.rs:475-528`(`snap_script`)——在 webview 里跑一整套:遍历 styleSheets → clone DOM → serialize → SVG foreignObject → Image load → canvas drawImage → toDataURL。这比加载 xterm.js 复杂得多。
- `flow.rs:206-228`(`click_script`)——`document.querySelectorAll('*')` 遍历 + `el.click()` 派发真实 DOM 事件
- `flow.rs:233-251`(`fill_script`)——find input + set value + dispatchEvent

**结论:`document::eval` 在 Dioxus 0.7 / wry WebView 里可以执行任意 JS,包括加载外部库、操作 DOM、返回数据给 Rust。**

**xterm.js 加载方案**:

xterm.js 是纯 JS 库(无原生依赖),可以通过 `document::eval` 注入 `<script>` 标签从 CDN 或本地 assets 加载:

```js
// eval 这段 JS:
if (!window.__xterm_loaded) {
  const link = document.createElement('link');
  link.rel = 'stylesheet';
  link.href = 'https://cdn.jsdelivr.net/npm/xterm@5.5.0/css/xterm.css';
  document.head.appendChild(link);
  const script = document.createElement('script');
  script.src = 'https://cdn.jsdelivr.net/npm/xterm@5.5.0/lib/xterm.js';
  script.onload = () => { window.__xterm_loaded = true; };
  document.head.appendChild(script);
}
```

然后初始化终端:

```js
const term = new Terminal({ cols: 80, rows: 24, theme: { background: '#EFEBE2' } });
term.open(document.getElementById('bw-terminal'));
window.__bw_term = term; // 全局引用,Rust 侧 eval 驱动
```

**字节流推送(Rust→xterm)**:

```js
// Rust 侧:eval 这段,把 bytes base64 编码后传进来
const data = atob("...base64...");
window.__bw_term.write(data);
```

**用户输入(xterm→Rust)**:

```js
// 初始化时注册 onData 回调,把用户输入 stash 到全局变量
window.__bw_term.onData((data) => {
  // data 是用户敲键产生的字符串(含 ANSI 转义)
  window.__bw_input_queue = (window.__bw_input_queue || '') + data;
});
```

Rust 侧轮询 `window.__bw_input_queue`(同 flow.rs 的 `readback_script` 模式):

```js
const q = window.__bw_input_queue || '';
window.__bw_input_queue = '';
return q;
```

### 条件/限制

1. **无持久双向通道**:`document::eval` 是 one-shot(发一条 JS,等返回值),不是 WebSocket 式持久连接。Rust→JS 推字节、JS→Rust 收输入都要**轮询**(同 flow.rs 的 300ms `POLL_INTERVAL`)。对终端来说,300ms 刷新率肉眼看可以接受但有延迟感;可以缩到 50-100ms。
2. **eval 竞态**:flow.rs 文档了 `document::eval` 的 re-render race(变异操作触发 Dioxus 重渲染 → eval 的 `await` 可能 resolve 到 `EvalError::Finished`)。flow.rs 用 stash + readback 双 eval 解决。终端字节流推送也会遇到同款竞态,但**终端 write 是非变异的**(只往 xterm canvas 写字,不触发 Dioxus re-render),所以风险比 flow.rs 的 click/fill 低。
3. **CDN 依赖**:首次加载 xterm.js 需要网络。产品化时应打包进 assets(本地文件),不走 CDN。但 `WebFetch` 在 GLM 网关下不可用——这里 xterm.js 是 webview 自己加载(CDP/WebView 内部网络),不走 Claude 的 WebFetch,所以网关不影响。
4. **Dioxus 0.7 的 `document::eval` 是全局 eval**,不绑定具体组件生命周期。终端 widget 卸载时要 eval 一段 JS 清理 `window.__bw_term`。同 flow.rs spawn_driver 的 use_future 模式。

### 不需要改的

- 不需要改 Dioxus 版本(0.7.9 已有 `document::eval`)
- 不需要改 wry 配置(wry 的 WebView 天然支持 JS)
- 不需要新 crate 依赖(app-desktop 已有 `serde_json` 解析 eval 返回值;base64 已有)

---

## 3. 事件总线桥:executor→widget 不破铁律

### Verdict:可行(需新事件类型 + 新 Command)

### 铁律约束

`scripts/guard-kernel-ui-free.sh` 强制 `bw-core`/`bw-engine`/`bw-store`/`bw-app`/`ui` 五个 crate **禁依赖 dioxus/tauri/wry/leptos**。所以:

- **PTY 必须在 bw-engine 或 bw-app 里开**(它们是 native runtime crate,不是 UI crate)——但 `portable-pty` 不是 UI 依赖,可以通过。
- **终端字节流必须经事件总线**从 executor(bw-engine/bw-app)流到 app-desktop 的 widget,不能反向耦合。
- **xterm.js 只在 app-desktop 里**,通过 `document::eval` 驱动。

### 现有事件总线架构(`crates/app-desktop/src/kernel.rs`)

```
UI ──Command──> mpsc ──> [kernel thread: App] ──watch──> Vm ──> UI
                          │
                          └──broadcast──> UiNote ──> UI
```

- `Command`(mpsc):UI → kernel,fire-and-forget
- `Vm`(watch):kernel → UI,每次 dispatch 后重建
- `UiNote`(broadcast):kernel → UI,瞬时通知(run 进度/错误/toast)
- `Event`(bw-app 内部):App.subscribe() → kernel 转发为 UiNote

### 设计:新事件类型 + 新 Command

**executor 侧(bw-engine/bw-app)**:

新 `InteractiveExecutor` 实现 `Executor` trait(或新 trait),持有 PTY master + writer。在 `run_phase`(或新方法)里:
1. spawn `claude`(不带 -p)进 PTY
2. `spawn_blocking` 读 PTY master 字节
3. 每读到一批字节,emit `Event::TerminalBytes { issue_id, bytes }`
4. 阻塞等待(用户在终端里交互),直到 claude 退出(child.wait())
5. 返回 `PhaseOutput`(从 session.jsonl 摘要)

**新 Event 变体(bw-app/src/lib.rs Event enum)**:

```rust
/// Interactive terminal byte stream from a live claude session.
/// Pushed to the UI for xterm.js rendering — NOT part of Vm (transient,
/// high-volume, same channel shape as UiNote).
TerminalBytes {
    issue_id: IssueId,
    bytes: Vec<u8>,  // raw PTY output, includes ANSI escapes
},
```

**新 Command(bw-app Command enum)**:

```rust
/// User typed in the xterm widget — write back to the PTY master.
TerminalInput {
    issue_id: IssueId,
    bytes: Vec<u8>,  // raw keystrokes from xterm.onData
},
```

**kernel 桥接(kernel.rs)**:

在现有 `Event → UiNote` 转发里加一条:

```rust
Event::TerminalBytes { issue_id, bytes } => {
    UiNote::TerminalBytes { issue_id, bytes }
}
```

`UiNote::TerminalBytes` 加入 `kernel.rs` 的 `UiNote` enum。

**UI 侧(app-desktop)**:

新组件 `TerminalWidget`:
- mount 时 `document::eval` 加载 xterm.js + 初始化 Terminal
- `use_future` 里订阅 `kernel.notes()`,收到 `UiNote::TerminalBytes` 时 `document::eval` 把 bytes base64 push 进 xterm
- `use_future` 轮询 `window.__bw_input_queue`(50-100ms),有数据就 `kernel.send(Command::TerminalInput { ... })`
- unmount 时 eval 清理

### 铁律验证

- `portable-pty` 只进 bw-engine(不是 UI crate)——✓ 通过 guard
- `Event::TerminalBytes` 是 `Vec<u8>`,不引用任何 UI 类型——✓
- xterm.js 只在 app-desktop 的 `document::eval` JS 里——✓
- 字节流单向:executor → Event → UiNote → widget;用户输入:widget → Command → executor → PTY writer——✓ 不反向耦合

### 风险

- **字节量**:`UiNote` broadcast channel 容量 256(kernel.rs:493)。终端输出可能很高频(claude TUI 每秒刷新多次)。需要节流:executor 侧攒 50-100ms 的字节再 emit 一批,或用独立 channel 不走 broadcast。
- **kernel 单线程**:`App` 在 kernel 线程单线程独占(kernel.rs:663 select!)。`Command::TerminalInput` 会插队到 `select!` 的 cmd arm,和正常 dispatch 交错。但 TerminalInput 是轻量(写字节到 PTY writer),不阻塞。**需注意:如果交互式 executor 在 `run_phase` 里阻塞 kernel 线程,UI 会冻死**——这和 PRACTICE-buddy.md §4.3 bug① 同一个根因。**交互式 executor 必须走 `run_issue_backgrounded` 路径**(tokio::spawn 甩后台),不能 inline。

---

## 4. evidence 回流

### Verdict:可行,且比 one-shot 丰富

### 现状(one-shot 路径)

`claude_cli.rs:251-258` 注释:buddy 不传 `--no-session-persistence`,所以 claude 写 `session.jsonl` 到 `~/.claude/projects/<cwd-hash>/session.jsonl`。但 one-shot 路径**不读这个文件**——`run_phase` 返回的 `PhaseOutput.text` 只是 `CliResult.result`(final text),不解析 session.jsonl。

现有收尾(`lib.rs:4050 issue_run_tail`):
- `create_mr`(提 PR/MR)
- `transition_issue(InReview)`
- `scan_and_register_artifacts`(文件改动登记)

evidence 侧(`evidence.rs`):
- `WorkspaceEvidence::collect` — git commit count / tracked files / dirty paths / recent subjects
- `list_workspace_files` — tracked files + bytes
- `head_commit` — HEAD hash

### 交互式跑完的收尾设计

**session.jsonl 一定写**:`claude --help` 确认 `--no-session-persistence` 标注 "(only works with --print)"——交互式 session **总是持久化**。

**session.jsonl 格**(每行一个 JSON 对象):
- `type: "user"` — 用户消息(含初始 prompt + 用户多轮输入)
- `type: "assistant"` — claude 回复(content blocks)
- `tool_use` blocks — claude 调了哪些 tool(Bash/Read/Write/Edit/...)
- `tool_result` blocks — tool 返回什么
- `type: "system"` — 系统事件

**解析方案**(新 `bw-engine/src/session_log.rs`):

```rust
pub struct SessionSummary {
    pub user_messages: Vec<String>,      // 用户问了什么
    pub assistant_messages: Vec<String>, // claude 答了什么
    pub tool_calls: Vec<ToolCall>,       // 调了哪些 tool
    pub final_text: String,              // 最后一条 assistant 消息
}

pub struct ToolCall {
    pub tool: String,   // "Bash" / "Read" / "Write" / ...
    pub input_summary: String,
}

pub fn parse_session_jsonl(path: &Path) -> Result<SessionSummary, ...>;
```

**摘要喂给 WorkflowPanel**:把 `SessionSummary` 转成会话消息(`store.append_message(session, Author::Agent, &text)`),和现有 `run_round_loop` 里 `lib.rs:1887-1896` 做的一模一样——每个 phase output 存一条 agent message。

**比 one-shot 丰富在哪**:
- one-shot 只有 `CliResult.result`(一段 final text)
- 交互式有完整对话:用户每轮问了什么、claude 每轮答了什么、调了哪些 tool、tool 返回什么
- 可以在 issue 卡片上展示"agent 调了 5 次 Bash、改了 3 个文件、读了 2 个文件"——比一段文字硬核得多
- **读回为证**:session.jsonl 是 claude 自己写的,在 `~/.claude/projects/` 下,`sqlite3` + 文件独立查证

**文件改动 + artifact 登记**:走现有 `issue_run_tail` 同款——`scan_and_register_artifacts` + `create_mr` + `transition_issue(InReview)`。**交互式跑完的收尾和 one-shot 完全复用 `issue_run_tail`**,只是 `PhaseOutput.text` 的来源从 `CliResult.result` 换成 `SessionSummary.final_text`(或拼接的对话摘要)。

### 风险

- **session.jsonl 路径定位**:`~/.claude/projects/<cwd-hash>/session.jsonl`。cwd-hash 是 cwd 路径的 SHA(或类似)。需要确定 claude 的 hash 规则才能找到文件。可以在 spawn 前记录 cwd,spawn 后扫 `~/.claude/projects/` 找最新修改的 session.jsonl(粗暴但可靠)。
- **session.jsonl 实时性**:交互式跑完(claude 退出)后文件才完整。跑的过程中读可能不完整。收尾时读没问题。
- **JSONL 解析**:每行一个 JSON,但单行可能很大(tool_result 含完整文件内容)。用逐行 `serde_json::Deserializer::from_reader` 流式解析。

---

## 5. run_phase 契约适配

### Verdict:需要重新设计,不能直接套

### 现状契约

`bw-engine/src/lib.rs:89`:

```rust
#[async_trait]
pub trait Executor: Send + Sync {
    async fn run_phase(&self, phase: &PhaseNode, ctx: &RunCtx) -> Result<PhaseOutput, ExecError>;
}
```

`PhaseOutput { text, done, gaps }` — "跑到完成"的语义。

`Engine::run_phase_range`(lib.rs:178)在 for 循环里逐 phase 调 `run_phase`,每个 phase 内部循环到 `done` 或 `max_iter`。

### 交互式的语义冲突

- **one-shot**:`run_phase` 阻塞到 claude 退出(几十秒到几分钟),返回 `PhaseOutput`。语义 = "跑到完成"。
- **交互式**:用户在终端里和 claude 多轮对话,时长不定(可能几分钟,可能几小时)。`run_phase` 如果阻塞到 claude 退出,意味着整个 kernel 线程(或 backgrounded task)一直挂着,用户随时在终端里交互。这不是"跑到完成",是"用户主导会话"。

### 三种适配方案

**方案 1:交互式不走 phase 拆分(推荐)**

一个 skill = 一个交互会话。`InteractiveExecutor` 不实现 `Executor` trait,而是新 trait:

```rust
#[async_trait]
pub trait InteractiveExecutor: Send + Sync {
    async fn run_interactive(
        &self,
        prompt: &str,          // 注入了 skill 的初始 prompt
        ctx: &RunCtx,
        on_bytes: impl FnMut(IssueId, Vec<u8>),  // PTY 字节推 UI
        on_input: impl FnMut() -> Vec<u8>,       // UI 输入回 PTY(轮询)
    ) -> Result<SessionSummary, ExecError>;
}
```

调用方(bw-app)在 issue run 时选 InteractiveExecutor 而非 Engine::run_workflow,跑完一个交互会话就收尾。不走 phase 拆分、不走 adversarial loop。

优点:语义干净,不和现有 `run_phase` 契约冲突,one-shot 路径零扰动。
缺点:交互式 issue 不享受 phase 拆分 + adversarial review loop。

**方案 2:InteractiveExecutor 实现 Executor trait,run_phase 阻塞到 claude 退出**

`run_phase` 内部 spawn PTY claude,阻塞读字节推事件,直到 child.wait() 返回。`PhaseOutput.text` = session.jsonl 摘要。`done = true`(交互式不循环)。

优点:复用现有 `Engine::run_phase_range` + `run_round_loop` + `issue_run_tail`,改动最小。
缺点:`run_phase` 语义从"几十秒"变成"不定时(可能几小时)",`ATTEMPT_TIMEOUT_SECS = 30min`(claude_cli.rs:210)会 kill 掉长交互。需要为交互式路径去掉/放大超时。

**方案 3:交互式走 `run_adhoc`**(`lib.rs:266`)

`Engine::run_adhoc` 已经是"one-shot 调一次 executor,不进 phase 循环"的 seam。交互式 executor 可以在这里阻塞到 claude 退出。

优点:现有 seam,不新加 trait。
缺点:`run_adhoc` 语义也是"跑到完成返回",交互式不定时阻塞语义仍不合。

**建议:方案 1(新 trait)**。理由:交互式和 one-shot 是两种本质不同的执行模式,硬塞同一个 trait 会让语义模糊。新 trait + bw-app 在 issue run 时按配置选 executor(同现有的 MockExecutor/ClaudeCliExecutor 热插拔模式),one-shot 路径完全不动。

### 风险

- **交互式不走 adversarial review loop**:现有 T9(plan/12 §4)的评审→打回→重跑机制依赖 phase 拆分。交互式如果也要评审,得在交互会话结束后单独跑一个 one-shot evaluator phase(用现有 ClaudeCliExecutor)。
- **max_iter 语义**:交互式没有"迭代到 done"的概念,用户说停就停(claude 退出或用户关终端)。

---

## 6. 权限/预算/安全

### Verdict:有条件可行,`--max-budget-usd` 是硬伤

### 证据(`claude --help` 实测)

| flag | 交互式可用 | 证据 |
|---|---|---|
| `--permission-mode` | **是** | help 文本无"(only works with --print)"限制;choices: acceptEdits/auto/bypassPermissions/manual/dontAsk/plan |
| `--disallowedTools` | **是** | help 文本无限制;逗号分隔 tool 名 |
| `--allowedTools` | **是** | help 文本无限制 |
| `--max-budget-usd` | **否** | help 明确标注 "(only works with --print)" |
| `--output-format` | **否** | help 明确标注 "(only works with --print)" |
| `--resume` | **是** | 交互式可 resume 之前的 session |
| `--no-session-persistence` | **否** | help 明确标注 "(only works with --print)"——交互式一定写 session.jsonl |

### 预算失控(Q6 硬伤)

**`--max-budget-usd` 在交互式不认**。这意味着:

- one-shot 路径:`claude_cli.rs:259` 传 `--max-budget-usd 0.50`(或 env 配的 1000),花到上限 claude 自己停。
- 交互式路径:**没有 CLI flag 级的预算封顶**。用户在终端里和 claude 聊到天荒地老,预算不受 CLI 控制。

**退路**:
1. **settings.json 配置**:claude 的 `~/.claude.json` 或 `--settings` 可能支持 budget 配置(需进一步验证;help 里 `--settings` 描述是 "Path to a settings JSON file or a JSON string")。
2. **executor 侧超时**:在 `InteractiveExecutor` 里设一个 wall-clock 超时(如 30min),超时 kill child(`kill_on_drop(true)` 同 claude_cli.rs:268)。这不是预算封顶,但能防 runaway。
3. **监控 session.jsonl**:跑的过程中轮询 session.jsonl 的 token 数,超阈值 kill。但 session.jsonl 是追加写的,解析正在写的文件有竞态。
4. **诚实标注**:UI 上标"交互式无预算封顶,请自行留意花费"——诚实,不假装有封顶。

### `gh pr merge` 禁令

`claude_cli.rs:278` 传 `--disallowedTools "Bash(gh pr merge),Bash(gh pr merge:*)"`。

交互式下:
- `--disallowedTools` 仍认(help 无限制)→ claude 在交互式里调 `gh pr merge` 会被拒。
- **但用户在终端里直接敲 `gh pr merge` 呢?**——不会。交互式 claude 是 TUI,用户不是在 shell 里敲命令,是在 claude 的 prompt 里打字。用户输入的是消息给 claude,claude 决定调不调 tool。所以 `--disallowedTools` 仍有效。
- **风险**:如果用户在 claude 里让它 `Bash(git merge ...)` 或其他绕路方式合 PR,`--disallowedTools` 的 pattern 匹配是精确的(`Bash(gh pr merge)` 只拦这个具体命令)。和 one-shot 路径面临的风险完全一样——`claude_cli.rs:277` 注释已说"deny 规则在 bypass 时是否被 CLI 强制取决于 CLI 版本行为,所以两半都要在"(纵深防御)。交互式不增加新风险。

### `--permission-mode` 交互式仍认

交互式可以传 `--permission-mode acceptEdits`(同 one-shot),claude 在交互式里调 tool 时走 acceptEdits 权限。也可以传 `bypassPermissions`(用户全局默认)——但 buddy 执行器应硬传 `acceptEdits`(同 one-shot 铁律,见 practice-buddy-landing §3)。

### 环境变量剥离

`claude_cli.rs:235-244` 剥离了 7 个 `ANTHROPIC_*`/`CLAUDECODE_*` 环境变量(防嵌套执行的 401)。交互式 executor 要做同款剥离。

### 风险小结

| 风险 | 严重度 | 退路 |
|---|---|---|
| `--max-budget-usd` 交互式不认 | **高** | wall-clock 超时 + UI 诚实标注无预算封顶;长期看可验证 settings.json 是否支持 |
| 用户在终端里绕过 tool 限制 | 低 | 交互式是 claude TUI 不是 shell;`--disallowedTools` 仍拦 claude 的 tool 调用 |
| `--output-format json` 不可用 | 中 | 不依赖 JSON output;靠 session.jsonl 做 evidence 回流(见 Q4) |

---

## 7. 构建计划(若决定推进 A 方案)

### 新增文件

| 文件 | 职责 | 所在 crate |
|---|---|---|
| `crates/bw-engine/src/interactive.rs` | `InteractiveExecutor` + PTY spawn + 字节流读取 + session.jsonl 解析 | bw-engine |
| `crates/bw-engine/src/session_log.rs` | session.jsonl 解析器(`SessionSummary`) | bw-engine |
| `crates/app-desktop/src/screens/terminal.rs` | `TerminalWidget` 组件(xterm.js 加载 + 字节推送 + 输入轮询) | app-desktop |

### 修改文件(只加,不改现有路径)

| 文件 | 改动 | 铁律验证 |
|---|---|---|
| `crates/bw-engine/Cargo.toml` | 加 `portable-pty = "0.9"` | portable-pty 非 UI 依赖,通过 guard |
| `crates/bw-engine/src/lib.rs` | `pub mod interactive; pub mod session_log;` + re-export | 不动现有 Executor trait |
| `crates/bw-app/src/lib.rs` | Event enum 加 `TerminalBytes`;Command enum 加 `TerminalInput` + `StartInteractiveRun` | 新变体,不改现有 |
| `crates/app-desktop/src/kernel.rs` | UiNote 加 `TerminalBytes`;Event→UiNote 转发加一条 | 新变体 |
| `crates/app-desktop/src/screens/op.rs` | IssueDetailOverlay 里条件渲染 `TerminalWidget`(交互式 issue) | 新组件引用 |

### 新事件/Command 类型

```rust
// bw-app Event enum 新增:
TerminalBytes { issue_id: IssueId, bytes: Vec<u8> },

// bw-app Command enum 新增:
StartInteractiveRun { issue_id: IssueId },
TerminalInput { issue_id: IssueId, bytes: Vec<u8> },
TerminalClose { issue_id: IssueId },

// app-desktop UiNote enum 新增:
TerminalBytes { issue_id: IssueId, bytes: Vec<u8> },
```

### 执行拓扑(复用现有 backgrounded 路径)

```
用户点「交互式开工」
  → Command::StartInteractiveRun
  → app.prepare_issue_run(同现有,skill 注入 + worktree + transition InProgress)
  → app.run_interactive_backgrounded (新方法,同 run_issue_backgrounded 模式)
    → tokio::spawn InteractiveExecutor::run_interactive
      → PTY spawn claude (不带 -p,带 --permission-mode/--disallowedTools/--allowedTools)
      → spawn_blocking 读 PTY master 字节
      → emit Event::TerminalBytes { issue_id, bytes }  ← 推 UI
      ← poll Command::TerminalInput { issue_id, bytes } ← UI 回传用户输入
      → writer.write(bytes) 写回 PTY
      → child.wait() 阻塞到 claude 退出
      → parse session.jsonl → SessionSummary
      → settle_tx.send(SettleReq) 通知 kernel 收尾
  → app.run_issue_settle (同现有)
    → issue_run_tail(create_mr + transition InReview + scan_and_register_artifacts)
```

### 对 one-shot 路径零扰动的验证

1. `Executor` trait(`bw-engine/src/lib.rs:89`)不动——`InteractiveExecutor` 是新 trait 或新结构,不实现现有 trait。
2. `ClaudeCliExecutor`(`bw-engine/src/claude_cli.rs`)**只读不改**——交互式 executor 在 `interactive.rs`,并列。
3. `Engine::run_workflow` / `run_phase_range` / `run_adhoc`(**不动**——交互式不走这些方法。
4. `run_issue_now` / `run_issue_body` / `run_issue_backgrounded` / `issue_run_tail`——**只加新方法 `run_interactive_backgrounded`**,现有方法不动。
5. `Event` / `Command` enum——**只加变体**,现有变体不改。
6. `guard-kernel-ui-free.sh`——`portable-pty` 不是 dioxus/tauri/wry/leptos,通过。
7. `cargo check -p bw-core --target wasm32-unknown-unknown`——bw-core 不加新依赖,通过。
8. `cargo clippy --workspace --exclude app-desktop`——新代码在 bw-engine/bw-app,过 clippy。

### 验收(E2E 读回)

```bash
# 交互式跑完一个 issue 后:
sqlite3 <db> "SELECT status FROM issue WHERE id=...;"  # → InReview(人没点 Done)
sqlite3 <db> "SELECT * FROM session_message WHERE session=...;"  # → 交互对话留痕
ls ~/.claude/projects/<hash>/session.jsonl  # → claude 自己写的会话档
sqlite3 <db> "SELECT * FROM artifact WHERE issue_id=...;"  # → 文件改动登记
# issue_run_tail 同款收尾:PR 号、InReview 状态、artifact 版本
```

---

## 8. 风险清单

| # | 风险 | 严重度 | 退路 |
|---|---|---|---|
| R1 | `--max-budget-usd` 交互式不认,预算失控 | **高** | wall-clock 超时 kill + UI 诚实标注;长期验证 settings.json |
| R2 | kernel 单线程冻死(同 bug①) | **高** | 交互式必须走 backgrounded(tokio::spawn),不能 inline |
| R3 | `document::eval` 轮询延迟(50-100ms) | 中 | 可接受;长期看 wry IPC 是否有更优通道 |
| R4 | session.jsonl 路径定位(cwd-hash) | 中 | 扫 `~/.claude/projects/` 找最新修改的文件 |
| R5 | ConPTY 最低版本(Windows 10 1809) | 低 | 用户环境 Win11 26100,没问题;产品化检查 |
| R6 | xterm.js CDN 首次加载需网络 | 低 | 打包进 assets,不走 CDN |
| R7 | PTY reader blocking→需 spawn_blocking | 低 | 工程量,非阻塞点 |
| R8 | 交互式不走 adversarial review loop | 中 | 交互结束后单独跑 one-shot evaluator |
| R9 | 终端 resize 要同步 PTY 尺寸 | 低 | widget resize 时 eval `term.resize()` + `master.resize()` |

---

## 9. 退路:B 方案(外部终端)

如果 A 方案(嵌终端)在 R1(预算)或 R2(kernel 冻死)上卡住不可行,退路是 B 方案:

- **不嵌终端**,而是用 `std::process::Command` 打开系统终端(macOS Terminal.app / Windows Windows Terminal)跑 `claude`。
- executor 只负责 spawn + 等 claude 退出 + 读 session.jsonl 收尾。
- 用户在外部终端里交互,app 里只显示"交互式运行中…(请在外部终端操作)"。
- **优点**:零 PTY/xterm 工程,零 kernel 冻死风险,零预算封顶问题(外部终端用户自己控制)。
- **缺点**:不是"app 内嵌",体验割裂;但 evidence 回流(session.jsonl)和 issue_run_tail 收尾完全一样。

B 方案是 A 方案的真子集(A 的收尾部分),所以即使 A 不行,B 也能直接用 `interactive.rs` 里的 session.jsonl 解析 + `issue_run_tail` 复用。

---

## 附录:Spike 代码

### `spike/pty-spike/Cargo.toml`

```toml
[package]
name = "pty-spike"
version = "0.1.0"
edition = "2021"

[dependencies]
portable-pty = "0.9"

[workspace]
```

### `spike/pty-spike/src/main.rs`

见 `spike/pty-spike/src/main.rs`(本 worktree 内)。实测输出:

```
[pty-spike] read 4 bytes: "\u{1b}[6n"
```

`\u{1b}[6n` = ConPTY 初始化的 DSR 序列,证明 PTY master 端字节流在 Windows 上可用。
