# V1 遗留深度分析 · 交互式引擎 / PTY 架构组

> ⚠️ **归正注记(2026-08-07)**:本文对 W2-1(切走丢字 / 重启黑框 / 窄窗错行)的诊断「单槽 watch 丢字节」只盖了三现象之一,且把「切走丢字」和「窄窗错行」混为一谈。W2-1 的真根因经两轮独立架构评估已归正:四个生命周期(活 / 交付运行 / Claude 会话 / 终端连接)被错误绑死。归正后的设计事实源见 [`issue2-terminal-conversation-refactor.md`](issue2-terminal-conversation-refactor.md),以该篇为准——别照本文 W2-1 段落施工。本文其余四条(V1-P1 macOS、W1-2 clone 堵命令循环、W2-3 无预算封顶、W2-7 诊断 spike 清理)仍作数。

> **30 秒导读**：这份报告覆盖五条遗留（V1-P1 macOS 跑不了、W2-1 离开面板丢字节/重启黑框消失、W1-2 clone 堵命令循环、W2-3 交互式无预算封顶、W2-7 诊断 spike 清理核实）。给现状代码行号 → 根因 → 方案选项与取舍 → 推荐 → 工作量 → 是否动铁律。结论先行：**V1-P1 的 portable-pty Unix 后端是让 macOS 真正可用的唯一正路，且工作量不大（portable-pty 的 Unix 后端工作正常，issue2 §9 那条「不投递 stdout」的坑是 Windows ConPTY 专有的，不影响 Unix）**；W2-7 清理已做完；其余三条都不动铁律，按痛度排序 W1-2 > W2-1 > W2-3。报告只读分析为主，未碰主分支代码；portable-pty 可行性未在本机做 demo（本机是 Windows，验不了 Unix 后端），结论基于 API 形态与 issue2 §9 的事实陈述。
>
> **代号说明**：PTY = 伪终端（把 claude 放进一个可被嵌入 UI 的虚拟终端里，字节流双向收发）；ConPTY = Windows 的伪终端实现；铁律 = CLAUDE.md 列的产品不可违反约束；读回为证 = 任何「完成/数字是 X」的陈述必须能从 DB 或工作区独立复核。

---

## V1-P1 · macOS 上交互式跑不了（V1 实际 Windows-only）

### 现状代码（行号）

- `crates/bw-engine/src/interactive_cli.rs:517-529` —— `InteractiveExecutor` trait 的 `run_skill_pty` 默认实现：`Err("PTY not supported by this executor (use run_skill instead)")`。
- `crates/bw-engine/src/interactive_cli.rs:696` —— `InteractiveCliExecutor::run_skill_pty` 整个函数挂 `#[cfg(windows)]`，后端是 Windows 专有的 `conpty-oxide`（`Cargo.toml:35-36`）。非 Windows 上这个方法不存在，走 trait 默认 `Err`。
- `crates/bw-engine/src/interactive_cli.rs:615-640` —— `run_skill` 的 macOS 分支：`osascript` 调 Terminal.app，osascript 进程立即退出拿不到 claude 句柄，于是 `tokio::time::sleep(self.timeout)`（`self.timeout = 1 小时`，L565）睡满，然后 L636-638 返回 `SkillOutput { completed: true, summary: "(wall-clock timeout — Terminal session may still be running)" }`。**这是谎报完成**。
- `crates/bw-engine/src/interactive_cli.rs:808-840` —— `await_child` 超时分支同样返回 `completed: true`（设计如此，注释 L811-813 说「worktree git state is the real evidence」；Windows/Linux 上至少先等了真进程退出，macOS osascript 路径连等都没等）。
- `crates/app-desktop/src/kernel.rs:573` —— 桌面壳无条件 `.with_pty()`。
- `crates/bw-app/src/lib.rs:1493-1499` —— `with_pty` 设 `pty_enabled = true`。
- `crates/bw-app/src/lib.rs:5544-5563` —— `run_issue_interactive` 在 `pty_enabled` 为真时建 channel 并 `tokio::spawn` 调 `executor.run_skill_pty(...)`。macOS 上这个调用返回 trait 默认 `Err`，spawned task 把 `SettleOutcome::Interactive(Err(...))` 发回 settle channel。
- `crates/bw-engine/Cargo.toml:38-39` —— `portable-pty = "0.9"` 仅作非 Windows keepalive 依赖挂着，**没有接任何代码路径**。
- `crates/bw-engine/src/interactive_cli.rs:27-34` —— 顶部注释明说「非 Windows 走 trait 默认，caller 用 run_skill」，但只对 `pty_enabled == false` 的 headless/example 路径成立；桌面壳 `pty_enabled` 恒真，**不存在这条回落**。

### 根因

两层：

1. **缺 Unix PTY 后端**。`run_skill_pty` 只在 Windows 上有实现（conpty-oxide），非 Windows 走 trait 默认 `Err`。桌面壳又无条件开 PTY 模式，所以 macOS 点「▶跑(交互)」= spawn 一个立刻返回 `Err` 的 task → settle 收到失败 → 用户看到一句英文报错，跑不成。
2. **回落路径谎报完成**。`run_skill` 的 macOS osascript 分支睡满 1 小时后返回 `completed = true`，违反「读回为证」——没人验证过 claude 退没退、产出有没有落盘。注释自己承认「may still be running」。这条路径只在 `pty_enabled == false`（headless/example）时才被走到，桌面壳碰不到，但仍是诚实缺口。

关于 portable-pty 的关键事实判断：issue2 §9 弃用 portable-pty 0.9.0 是因为**它的 Windows ConPTY 后端不把子进程 stdout 投递给 reader**——这是 Windows 专有问题。portable-pty 的 Unix 后端（macOS/Linux 用 nix 的 `openpty`）工作正常，能 spawn 进程、能从 master 读到 stdout、能写 stdin、能 resize。所以把它接成非 Windows 的 `run_skill_pty` 后端是可行的。

### 方案选项（取舍表）

| 方案 | 做法 | 优点 | 缺点 | 工作量 | 动铁律？ |
|---|---|---|---|---|---|
| **A. 补 portable-pty Unix 后端（推荐）** | 给 `run_skill_pty` 加 `#[cfg(not(windows))]` 实现，用 `native_pty_system().openpty()` + `slave.spawn_command(CommandBuilder)` + `master.try_clone_reader()`/`take_writer()`/`resize()`，结构镜像现有 conpty-oxide 实现（L696-807） | macOS 真正可用，嵌入终端真跑；不动铁律；portable-pty 已在依赖里 | 需在 Unix 机上实测（本机 Windows 验不了）；portable-pty API 与 conpty-oxide 略有差异，spawn_blocking 读循环要重写 | 中（1-2 个会话） | 否（PTY 是 UI 管道，不碰 Signal/Done/settle） |
| B. 桌面壳非 Windows 拒绝并说人话 | `run_issue_interactive` 在 `cfg(not(windows))` 时不发 spawn，直接 `Err` 并发一条人话 toast「本机暂不支持嵌入式交互终端，V1 仅 Windows」 | 立刻消除英文报错；诚实（不假装能跑） | macOS 仍跑不了交互式，只是从「英文报错」变「人话拒绝」；治标 | 小（半天） | 否 |
| C. 保持现状 + 文档纠偏 | 只改 `CLAUDE.md`/指南：V1 交互式实际 Windows-only | 零代码风险 | macOS 用户照样撞墙；不解决产品可用性 | 极小 | 否 |

**推荐**：A（补 portable-pty Unix 后端）作为正路；在 A 落地前，先做 B 的「人话拒绝」作为过渡（半天活，立刻改善体验）。C 必须做（文档不能再宣称 macOS 交互式可用）。

### osascript 谎报完成的修法（必做，独立于上面）

`interactive_cli.rs:635-639` 改成返回 `completed: false`（或 `Err`），summary 写「macOS 系统终端无法判定 claude 退出，未验证完成 —— 请检查工作区 git 状态」。`await_child` 超时分支（L827-839）可保留 `completed: true`，因为 Windows/Linux 上它至少先 `child.wait()` 等了真进程，超时是兜底而非盲睡；但 macOS osascript 分支没等任何进程，不能借这个兜底。这条改动小（改一个返回值），独立于 PTY 后端是否补。

工作量：小（半小时）。动铁律？是——这条直接修「读回为证」的违反，但改法是收紧（不再谎报），不放松任何约束。

### portable-pty Unix 后端的工作量细节

portable-pty 0.9 的 API 形态（基于 crate 文档，未本机 demo）：

```rust
use portable_pty::{native_pty_system, CommandBuilder, Size};
let pair = native_pty_system().openpty(Size::new(80, 24))?;
let mut cmd = CommandBuilder::new(binary);
cmd.args(&plan.args);
cmd.cwd(&plan.cwd);
for (k, v) in &plan.env { cmd.env(k, v); }
let mut child = pair.slave.spawn_command(cmd)?;
let mut reader = pair.master.try_clone_reader()?;   // std::io::Read
let mut writer = pair.master.take_writer()?;         // std::io::Write
pair.master.resize(Size::new(cols, rows))?;
// spawn_blocking 读 reader → bytes_tx；主循环 select input_rx → writer.write_all；resize → master.resize
```

这与现有 conpty-oxide 实现（L713-807：`PtyCommand` → `spawn` → `into_parts` → 读循环 + input 循环 + resize）结构一一对应。差异点：portable-pty 的 `master.take_writer()` 返回 `Box<dyn Write + Send>`（不是 `OwnedWriteHalf`），`resize` 直接 `master.resize(...)`（不是 `controller.resize`）。读循环、submit_prompt（L760-791）的 `\r` 逻辑可原样复用。估 1 个会话能写出 + 编译过；第 2 个会话在 Unix 机上实测 claude 真跑。

### demo 结论

未做本机 demo（本机 Windows，验不了 portable-pty Unix 后端）。结论基于 portable-pty 的 API 形态与 issue2 §9 的事实陈述（弃用理由是 Windows ConPTY 专有，非 Unix）。若要 demo，需在 macOS/Linux 机上写一个独立 `cargo eval` 脚本 spawn `cat` 或 `claude --version` 验 reader 能拿到 stdout —— 这一步建议在真正动手补后端前做，作为可行性确认。

---

## W2-1 · 离开面板丢字节 + 重启后黑框消失无提示

> **归正注记(2026-08-07)**:本节诊断的「watch 单槽丢字节」只盖了三现象之一,且把「切走丢字」和「窄窗错行」混为一谈。截图确诊窄窗错行是 PTY 列数 ≠ xterm 实际宽度(横向堆叠,非丢字非乱码);更深根因是「活/交付运行/Claude 会话/终端连接」四个生命周期被错误绑死。终端多会话架构重构的设计事实源见 [`issue2-terminal-conversation-refactor.md`](issue2-terminal-conversation-refactor.md),本节保留作分析过程记录,读 W2-1 以归正 md 为准。

### 现状代码（行号）

- `crates/app-desktop/src/kernel.rs:526` —— `let (pty_tx, pty_rx) = watch::channel(Vec::<u8>::new());`，**单槽**（非队列）。
- `crates/app-desktop/src/kernel.rs:449` —— `pub fn pty_bytes(&self) -> watch::Receiver<Vec<u8>>`，每次调返回新 receiver。
- `crates/app-desktop/src/kernel.rs:744-749` —— `pty_ticker`（100ms）调 `app.drain_pty_bytes()` 把 mpsc 里所有待处理字节聚成一个 Vec，`pty_tx.send(bytes)`。watch 是单槽：无 receiver 时，每次 send 覆盖上一个未取值。
- `crates/bw-app/src/lib.rs:1812-1828` —— `drain_pty_bytes`：`try_recv` 循环把 mpsc 全部 pending 收成一个 Vec。executor 侧 `bytes_tx` 是 `mpsc::unbounded_channel`（L5549），**字节在 mpsc 层不丢**；丢的是 watch 层。
- `crates/app-desktop/src/screens/op.rs:3292-3402` —— `TerminalWidget` 的 `use_future`：`let mut pty_rx = k.pty_bytes();`（L3327）拿一个新 receiver，`pty_rx.changed().await`（L3339）取值。组件卸载 = `use_future` 被 drop = `pty_rx` 被 drop。
- `crates/app-desktop/src/screens/op.rs:3174-3181` —— re-attach guard：`if (window.__bw_term)` 则把 `term.element` 搬进当前 div + 重绑 div 级监听器，`onData` 不重绑。**这只解决了「xterm DOM 在不在」，不解决「PTY 字节有没有丢」**。
- `crates/app-desktop/src/kernel.rs:81-83` / `:939` / `:1304-1306` —— `pty_active = state.pty_input_tx.is_some()`，纯内存状态，进程重启后为 `None`。

### 根因

两个独立现象：

1. **离开面板期间字节丢**。TerminalWidget 卸载 → `pty_rx` 被 drop。期间内核 pty_ticker 仍在 100ms 跑：drain mpsc → `pty_tx.send(bytes)`，watch 无 receiver，每次 send 覆盖前值。用户切回来 → 新 `pty_rx` → `borrow_and_update()` 只能看到当前槽里那一份（卸载期间最后被 send 进去的批次），中间批次全被覆盖丢了。注意 mpsc 层不丢（unbounded 缓冲），但 `drain_pty_bytes` 每 tick 把 mpsc 清空成空 Vec，所以 mpsc 不会堆积——丢的是 watch 单槽的覆盖。
2. **重启后黑框消失无提示**。`pty_active` 是 `state.pty_input_tx.is_some()`，纯内存。buddy 重启 = 进程死 = state 全失 = `pty_active = false`。工作流面板 `if op.pty_active`（`op.rs:2844`）不渲染 `TerminalWidget`，于是连黑框都没有，用户看不到任何「会话已断开」的提示。这本身不是 bug（状态如实），但体验生硬。

### 方案选项（取舍表）

#### 现象 1（离开面板丢字节）

| 方案 | 做法 | 优点 | 缺点 | 工作量 | 动铁律？ |
|---|---|---|---|---|---|
| **A. watch 单槽 → 有界队列 mpsc（推荐）** | `pty_tx` 从 `watch::channel` 换成 `mpsc::channel::<Vec<u8>, 64>`（有界），`pty_bytes()` 返回 receiver clone；TerminalWidget 卸载期间队列累积，remount 后 `recv()` 把积压全部读出 | 彻底堵死丢字节窗口；改动集中在 kernel.rs 一处类型 + TerminalWidget 取值方式 | watch → mpsc 改动触及 `Kernel` 结构体字段类型 + `pty_bytes()` 签名；TerminalWidget 的 `changed().await` 改成 `recv().await`；有界队列满时需定策略（丢最老 / 背压） | 中（1 个会话） | 否 |
| B. 服务端 scrollback 缓冲 | executor 侧或 kernel 侧维护一个 ring buffer 存最近 N KB，TerminalWidget remount 时先 replay ring buffer | 用户能看见完整历史（含离开期间） | 工作量大（要维护 ring buffer + replay 逻辑 + 与 xterm.js 写入协调）；xterm.js 自身已有 scrollback，重复 | 大 | 否 |
| C. 卸载时不丢：让 `use_future` 不被 drop | 用 dioxus 的 `use_coroutine` 或全局 spawn 让 pty_rx 持久存活，TerminalWidget 只读不持有 | 改动小 | 改动 dioxus 生命周期管理方式，可能引新坑；不如 A 干净 | 中 | 否 |

**推荐**：A（watch → 有界 mpsc）。最干净、堵死根因、改动局部。有界容量 64 批（每批 ≤ 8KB，共 ≤ 512KB）足够覆盖任何合理离开时长。满时策略选「丢最老」（背压会让 claude 卡住，更糟）。

#### 现象 2（重启后黑框消失无提示）

| 方案 | 做法 | 优点 | 缺点 | 工作量 | 动铁律？ |
|---|---|---|---|---|---|
| **A. UI 提示「会话已断开，点▶跑用 --resume 接回」（推荐）** | 当 `interactive_started == true && claude_session_id 非空 && !pty_active` 时，工作流面板渲染一张占位卡：「上次会话已断开（buddy 重启），点▶跑用 `--resume` 接回」 | 诚实（不假装有终端）；引导用户恢复；与现有 resume 路径（`run_issue_interactive` L5415 `is_resume`）天然衔接 | 需在 `build_vm` 多带一个标志区分「从未跑过」vs「跑过但断了」 | 小（半天） | 否 |
| B. 重启后自动重连 PTY | 启动时扫 DB 找 `interactive_started && claude_session_id 非空` 的活，自动 `run_skill_pty` resume | 体验无缝 | 风险大：重启时序复杂、claude 进程可能已死、自动 spawn 不可控；违反「不假装绿」精神（假装会话还活着） | 大 | 待定（自动重连可能触碰 Done 语义） |

**推荐**：A（UI 提示）。诚实、轻、引导用户。B 的自动重连风险大、且与「人在看板可见可停」的精神不符。

### 工作量汇总

现象 1 的 A（mpsc 队列）约 1 个会话；现象 2 的 A（UI 提示）约半天。两者独立，可分开做。都不动铁律。

---

## W1-2 · codehub clone 同步堵命令循环 → UI 冻死

### 现状代码（行号）

- `crates/bw-app/src/lib.rs:6283` —— `match bw_engine::codehub::clone_repo(&host, &path, &dir).await { ... }`，在 `CompleteCreation` 的 dispatch 内同步 `.await`。github 分支同构（L6217 `bw_engine::github::clone_repo(...).await`）。
- `crates/app-desktop/src/kernel.rs:709-714` —— 桌面壳命令循环：`cmd = cmd_rx.recv() => { app.dispatch(cmd).await; vm_tx.send(...) }`。单线程、串行。`dispatch` 内的 `.await` 不返回，下一个 cmd 就卡在 `cmd_rx` 里出不来。
- PF1（`piercing-fixes-1.md` 点④）修的是 cron 抢跑时序（clone 完成前 cron 不跑），**没动 clone 同步堵的根因**。

### 根因

`CompleteCreation` 把 `git clone`（网络 IO，秒级到十几秒级）当成 dispatch 内的一步同步 `.await`。桌面壳命令循环单线程串行，clone 不返回，期间所有 `Command`（包括 Intent 提交）全排在 `cmd_rx` 里出不来 → UI 看似冻死（点了没反应，因为 vm_tx 也要等 dispatch 返回后才 send）。

### 方案选项（取舍表）

| 方案 | 做法 | 优点 | 缺点 | 工作量 | 动铁律？ |
|---|---|---|---|---|---|
| **A. clone 异步化 + loading 态（推荐）** | `CompleteCreation` 把 clone 那段 `tokio::spawn` 出去（同 `run_issue_interactive` 的 spawn 模式 L5553），dispatch 立刻返回；spawn 内完成后发一个 `SettleCreation` 类回执走 settle channel（复用现有 settle 机制 L722）；UI 在 `ActionProgress::Started`（L6276）到 `Ok/Fail` 之间显示 loading | 释放命令循环，UI 不冻；复用现有 spawn+settle 模式，不发明新机制 | 要给 `CompleteCreation` 加一个「创建进行中」态，store 要能持久化（否则重启丢失）；spawn 内的错误回执路径要接好 | 中（1 个会话） | 否 |
| B. clone 仍同步但移到独立线程 | 给命令循环加一个 worker 线程跑长任务，主循环不阻塞 | 不改 dispatch 结构 | 引入多线程共享 `&mut app` 问题（现有架构刻意单线程避免 Arc<Mutex>）；改动大、收益不如 A | 大 | 否 |
| C. 接受现状，UI 加「创建中…」遮罩 | dispatch 仍同步堵，但 UI 在提交后立刻盖一层「创建中，请稍候」遮罩挡住用户操作 | 改动极小（纯 UI） | 治标：命令循环仍堵，期间别的面板操作也冻；只是让用户「知道在等」 | 小 | 否 |

**推荐**：A（异步化）。根因解。模式与 `run_issue_interactive` 一致（spawn + settle 回执），代码库已有这套。需注意：`CompleteCreation` 当前在 dispatch 内同步写 DB（set_workspace/set_remote/create_connector），spawn 后这些要在 spawn 内做，且 dispatch 返回前要先把「创建中」态写进 store（或一个 in-memory 标志）让 UI 能显示 loading。

### 动铁律？

否。clone 是 IO，不碰 Signal/Done/settle。但要注意 settle-once：异步化后 `CompleteCreation` 的记账不能重复（创建只成一次），现有 `ActiveRun`/settle 机制已有 take() 双结算守卫（L5636），照搬即可。

---

## W2-3 · 交互式无 per-token 预算封顶

### 现状代码（行号）

- `crates/bw-engine/src/interactive_cli.rs:696-807` —— `run_skill_pty` 全程无 wall-clock deadline，无 token 估算。只在 child 退出 / App 丢 input 端 / 用户取消时收尾，返回 `Ok(SkillOutput { completed: true })`。
- `crates/bw-engine/src/interactive_cli.rs:168` 注释明说「No `--max-budget-usd` (interactive sessions are user-paced, no per-call cap)」。
- 后台 one-shot 轨（`ClaudeCliExecutor`）照旧 `--max-budget-usd 0.5` + `ATTEMPT_TIMEOUT_SECS`，一行没动（LEFTOVERS W2-3 处置段确认）。

### 根因

设计取舍：交互式会话是用户实时操作的，user-paced，不像后台 one-shot 那样会 runaway。所以没挂预算封顶。这是对 CLAUDE.md「单次花费封顶」的显式偏差，2026-08-06 review 已接受。

### 方案选项（取舍表）

| 方案 | 做法 | 优点 | 缺点 | 工作量 | 动铁律？ |
|---|---|---|---|---|---|
| A. 加 wall-clock deadline | `run_skill_pty` 主循环加一个 `tokio::time::sleep(INTERACTIVE_DEADLINE)` select arm，到点 kill child + 返回 `completed: false` | 防止用户离开后 claude 无限跑 | 交互式是 user-paced，硬截止会误杀（用户在思考，claude 不该被砍）；deadline 设多少都尴尬 | 小 | 待定（kill child 不碰 Done，但 `completed: false` 语义要接好 settle） |
| B. 加 token 估算封顶 | 解析 claude 输出里的 token 计数，超阈值 kill | 防花费失控 | claude interactive 输出无结构化 token 计数，要靠 heuristic；不准；复杂 | 大 | 同上 |
| **C. 接受不补（推荐）** | 维持现状，只在文档/指南如实标注「交互式无封顶，用户在场监控」 | 诚实；零风险；与 user-paced 设计一致 | 偏离「单次花费封顶」字面 | 零 | 否（已接受的偏差） |

**推荐**：C（接受不补）。理由：交互式本质是 user-paced，用户在场就是在场监控，runaway 风险与后台 one-shot 不同量级；后台 one-shot 轨的封顶没动，铁律在那条线仍守。若日后真要补，A（wall-clock）比 B（token 估算）现实，但 deadline 值需实测定。当前不必投入。

### 动铁律？

C 不动铁律（已接受的显式偏差）。若未来选 A/B，需审 `completed: false` 在 settle 路径的语义（不能让它绕过「Done 永不自动」——kill child 不该推 issue 状态，只该停 run）。

---

## W2-7 · §9.7 诊断 spike 清理核实

### 核实结论：清理已完成

逐项核对：

- `crates/bw-engine/examples/` 目录：**空**（只有 `.` / `..`，无任何 .rs 文件）。pty_spike / conpty_direct / conpty_test / conpty_oxide_test / conpty_oxide_claude 五个诊断 spike 源文件**全部已删**。
- `[dev-dependencies]`（`Cargo.toml:41-45`）：只有 `tokio` + `async-trait`。§9.7 说的 `conpty` / `conpty-oxide` / `winapi` 在 dev-deps **已清干净**（注：`conpty-oxide` 仍出现在 L36，但那是 `[target.'cfg(windows)'.dependencies]` 生产依赖，是 `run_skill_pty` Windows 后端在用，不是诊断 spike 残留——这是对的）。
- `interactive_cli.rs` 全文 grep `pty-diag` / `pty_diag`：**无命中**。诊断日志已清。
- 全仓 `find -name "pty-diag*"`：**无命中**。pty-diag.log 文件已清。

**结论**：W2-7 可从 LEFTOVERS 标记为已核实完成，无需进一步动作。

---

## 优先级建议（痛度 × 工作量 × 是否阻塞别的）

排序从高到低：

1. **V1-P1 · osascript 谎报完成修法（必做，独立小改）**。痛度高（违反读回为证铁律）、工作量极小（半小时，改一个返回值）、不阻塞别的。应最先做，无论是否补 Unix PTY 后端。
2. **W1-2 · clone 异步化**。痛度高（用户实地撞出的 UI 冻死）、工作量中（1 会话）、PF1 已缓解时序但根因未解、阻塞创建流体验。优先级仅次于铁律违反。
3. **V1-P1 · portable-pty Unix 后端（A 方案）**。痛度高（macOS 完全跑不了交互式）、工作量中（1-2 会话）、决定产品在 macOS 是否可用。与 1 的修法独立，但若做 A 则 osascript 分支不再被走到（仍建议改掉谎报）。
4. **W2-1 · watch→mpsc 队列 + 重启提示**。痛度中（离开面板丢字节要跨多个 100ms 批次才明显；重启黑框是体验生硬非功能性 bug）、工作量中（1 会话 + 半天）、不阻塞别的。
5. **W2-3 · 接受不补**。痛度低（已接受）、工作量零、不阻塞。维持现状即可。
6. **W2-7**。已完成，无需动作。

阻塞关系：无强阻塞。V1-P1 的 osascript 修法与 Unix 后端独立；W1-2 与 W2-1 都动 kernel.rs 但改的是不同段（clone 在 bw-app dispatch、pty watch 在 kernel select），可并行排期但别同会话改以免 merge 冲突。

---

## 附：未做事项与待定

- **portable-pty Unix 后端未本机 demo**（本机 Windows，验不了）。建议真正动手补后端前，在 macOS/Linux 机上写一个 `cargo eval` 独立脚本，spawn `cat` 或 `claude --version`，确认 `master.try_clone_reader()` 能拿到 stdout。这是可行性确认的廉价一步，不到 10 分钟。
- **W2-1 有界 mpsc 满时策略**选「丢最老」还是「背压」待实测定（建议丢最老，背压会让 claude 卡住更糟）。
- **W1-2 异步化后「创建中」态的持久化**待设计：若 buddy 在 clone 进行中重启，状态丢失，需用户重试——可接受（创建本就可重试），但 UI 要诚实反映「未完成」。
