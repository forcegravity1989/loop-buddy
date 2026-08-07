# orca-main 终端会话架构参考(可借鉴机制 + buddy 取舍)

> **30 秒导读**:这份是 orca-main(`D:\2026\code\orca-main`,Electron + React + node-pty + xterm.js)的终端多会话架构摘要,供 buddy 的终端会话重构(`issue2-terminal-conversation-refactor.md`)参考。**只记可借鉴机制 + 源码锚点 + buddy 为什么取舍不同**。orca 是高分开源项目,buddy 在「内嵌 claude cli 的多会话切换」上吸纳它的交互和技术模式;但 buddy 是 Rust/Dioxus 单人构建者工作台,不是 node/多 agent 工作台,代码不同栈,只借模式不借代码。现在作数,作为重构的参考对照。

---

## 1. PTY 会话怎么 spawn 和管理

**orca 结论**:用 `node-pty`;PTY 子进程由一个**独立 daemon 进程**持有(不归 Electron main),daemon 可独立于 Electron 存活;**一会话 = 一 PTY 子进程 = 一 shell,严格一对一**。

**buddy 取舍**:buddy 是单人单机工作台,不引入独立 daemon(那是 orca 多 agent / 跨重启持久化的需要)。buddy 用**进程内 `TerminalManager`**(`Map<conversation_id, TerminalSession>`)持有多个 PTY 子进程,buddy 进程退出即全清,靠 `claude_conversation` 表恢复身份。这是「会话身份落库 + PTY 连接纯内存」的折中:体感接近 orca(点卡接回),但架构轻(无 daemon)。

**orca 锚点**:
- `src/main/daemon/pty-subprocess.ts:2` `import * as pty from 'node-pty'`;`:603` `createPtySubprocess()` 内 `pty.spawn(...)`。
- `src/main/daemon/terminal-host.ts:27` `sessions = new Map<string, Session>()`;`terminal-host-session-create.ts:97-119` 每次 create 新建一个 `Session`,内部 new 一个 subprocess。

---

## 2. 字节流怎么路由到前端

**orca 结论**:**按 sessionId 多路复用**在一条 IPC 推送通道上;daemon 端按 `clientId+sessionId` 入 batcher,renderer 端有一个**单例 dispatcher**,`Map<ptyId, handler>` 路由,**不会串**。

**buddy 取舍**:直接吸纳。buddy 当前是单槽 watch(`kernel.rs:525`),无身份,全局只有一个流。重构改为**字节带 `conversation_id` 标签 + 消费端按 id 路由**(`TerminalManager::events()` 返回 `Vec<(ConversationId, TerminalEvent)>`)。这是修现象三(绑指标卡看到绑数据 CLI)的核心:字节不再是无身份全局流。

**orca 锚点**:
- daemon 侧:`src/main/daemon/daemon-server.ts:774-789` `streamClient.onData` 带 `routedSessionId` 入 batcher。
- 协议事件:`daemon-pty-provider.ts:126-129` payload 显式带 id。
- renderer 侧:`src/renderer/src/components/terminal-pane/pty-dispatcher.ts:50` 单例 dispatcher + `:150` `ptyDataHandlers.get(payload.id)` 按 id 投递。

---

## 3. 前端 xterm 实例:一个还是多个

**orca 结论**:**每个 pane/tab 一个独立 xterm `Terminal` 实例**,常驻 DOM(切 tab 是显隐/换容器,不是重建);切回来历史还在(实例没销毁);冷恢复时靠 daemon 侧 `HeadlessEmulator` 快照 + replay 回填。

**buddy 取舍**:直接吸纳。buddy 当前是全局单例 `window.__bw_term`(`op.rs:3158`),切卡重用同一实例 → 滚动位置丢、会话串显。重构改为**每会话一个独立 xterm 实例**(`Map<conversation_id, Terminal>`),切卡只换可见容器 + 键盘焦点,不销毁隐藏终端。

**orca 锚点**:
- `src/renderer/src/lib/pane-manager/pane-dom-creation.ts:47` `const terminal = new Terminal(terminalOpts)` 每 pane 一个 xterm(含独立 FitAddon/SearchAddon/SerializeAddon)。
- `ManagedPaneInternal`(`pane-dom-creation.ts:111-149`)字段 `terminal`/`container`/`xtermContainer` 常驻 pane 树。
- 重连/复用:`pty-connection.ts:2080` / `:5469` / `:5484` `replayIntoTerminal(pane, ...)` 切回时把快照 replay 进**已存在的** xterm。

---

## 4. 能否多个会话并发活

**orca 结论**:**能,多会话并发活**。后台会话的 PTY 子进程不暂停;字节要么继续推(被 renderer 的 eager-buffer / parked-terminal-byte-watcher 接住),要么在 `setPtyBackgrounded` 标记后**按事实丢弃瞬时输出但保留事实**,显示时用快照重绘——不丢语义。

**buddy 取舍**:吸纳到「A 交付 + B 咨询并发」(用户确认)。buddy 不做 orca 的「后台 droppable + snapshot」那套复杂机制(那是 orca 多 agent / 远程 / 大并发的需要);buddy 用**每会话有界 mpsc(64 批)**简单兜住后台输出,切回一次性读出。简单,够用。

**orca 锚点**:
- 每会话独立缓冲:`session.ts:123` `pendingOutputRecords` / `:127` `outputSequence` / `:128` `producerPaused`。
- 后台标记:`daemon-pty-adapter.ts:995` `setPtyBackgrounded`;`daemon-pty-router.ts:108` 路由到对应 adapter。
- 流控(背压):`session.ts:253` `pauseProducer()` 调 `subprocess.pause?.()` 停读 fd,5s 失效自动 resume。
- renderer 侧后台接字节:`parked-terminal-byte-watcher.ts`、`pty-eager-buffer-clamp.ts`。

---

## 5. resume / 重连语义

**orca 结论**:是 **PTY reattach(重连到仍活的 PTY 子进程)+ 快照 replay**,**不是 `claude --resume <session_id>`**。切到一张还在跑的卡 = 自动 attach + 快照回填,无需点按钮;PTY 子进程已退出则成 tombstone,需新 spawn。

**buddy 取舍**:**关键差异**——buddy 杀 PTY(kill claude 进程)后会话身份仍存(`claude_conversation` 表),点卡 = **重 spawn `claude --resume <session_id>`**(让 claude 自己恢复对话状态),不是重连活 PTY。因为 buddy 不持有跨进程存活的 daemon。代价:点卡到 PTY 就绪有 1-2 秒 spawn 延迟(spawn 进程 + 加载 jsonl);orca 是零延迟重连。用户接受这个差异(单人工作台,1-2 秒可接受)。

**buddy 的 resume 成立前提**(两轮评估确认):claude 把会话持久化到 `~/.claude/projects/<encoded-cwd>/<session_id>.jsonl`,encoded-cwd 是 cwd 编码。buddy 按原路径重建 worktree → cwd 不变 → encoded-cwd 一致 → `--resume` 能找到历史。**不同路径则找不到**。详见 `issue2-terminal-conversation-refactor.md §5`。

**orca 锚点**:
- 单入口 `createOrAttach`:`terminal-host-session-create.ts:26-66`。`:42-54` 已有 live session → `detachAllClients()` + `attachClient` + 返回 `snapshot`,**不 respawn**;`:62-66` `attachOnly:true` 且无 session → 抛 `SessionNotFoundError`。
- renderer 侧 attach-only:`src/main/ipc/pty.ts:719-732` `provider.spawn({...attachOnly: true, command: undefined})` 切回已有卡只重连不重 spawn。
- 冷恢复(daemon 崩了):`cold-restore-payload-cache.ts` / `history-reader.ts` / `terminal-history-seed-segments.ts` 从磁盘历史回填字节再 replay。
- tombstone:`terminal-host.ts:29` `killedTombstones`;exit 后 `reapSession`(`terminal-host.ts:118`)清掉,不能重连。

---

## 6. buddy 砍掉不借的(orca 特有)

- 远程 / SSH PTY + relay(多机协作,buddy 单机不要)。
- 移动 companion(buddy 无移动端)。
- 多 worktree 并行(buddy 一个项目一个主工作区,issue worktree 已够)。
- AI Vault 跨 16 CLI 聚合(buddy 只内嵌 claude cli)。
- 多 account / CDP 浏览器内嵌(buddy 是 Dioxus/wry WebView,不是浏览器内嵌)。
- **daemon 持久化 PTY 跨重启**(buddy 用「身份落库 + 点卡重 spawn」替代,不引入 daemon)。
- 团队协作 provider(Linear/Jira/GitLab……buddy 反命题:非团队协作)。

---

## 7. 三条 race(orca 解决,buddy 要照搬)

`issue2-metrics-interactive-loop.md §2.4` 已记 orca 的三条 race,buddy 重构要照搬:

1. **ACK backpressure(`ackData`)**:UI 节流防淹没。buddy 当前 `document::eval` 同步不需要(已实现),但多会话后要复核。
2. **rendererDispatcherReady 握手**:UI 在 xterm.js 就绪前缓冲字节,防 reload 丢字节。buddy 当前用 `__bw_term_buffer`(已实现),多会话后每会话独立 buffer。
3. **resize 重断言(`getAppliedSize`)**:UI 重发 resize 若 PTY 应用尺寸不匹配。buddy 当前未做——窄窗错行的修法之一就是这条:每会话独立尺寸 + 重断言。

---

## 8. 结论:buddy 复刻「切卡即 resume」的改动量

| 项 | orca | buddy 复刻 |
|---|---|---|
| PTY 持有 | 独立 daemon,跨重启存活 | 进程内 TerminalManager,重启清空靠表恢复 |
| 字节路由 | 单例 dispatcher 按 id | 每会话有界 mpsc + id 路由 |
| xterm | 每 pane 一个,常驻 DOM | 每会话一个,切卡显隐 |
| 多会话并发 | 后台 droppable + snapshot | 后台有界 mpsc 简单兜 |
| resume 语义 | 重连活 PTY,零延迟 | 重 spawn `--resume`,1-2 秒延迟 |
| daemon | 是 | **否**(buddy 不要 daemon) |

**改动量档位:中**。卡点不在单条机制(每条 buddy 都有对应简化版),而在**把全局单例(`pty_input_tx` / `window.__bw_term` / `pty_tx` watch)改成 per-conversation 多例 + id 路由**——这是 `issue2-terminal-conversation-refactor.md §7` 的核心改动。

---

_本篇为参考对照;设计与落地以 `issue2-terminal-conversation-refactor.md` 为准。orca 源码锚点供实施时按需核验,不重做全仓探索。_
