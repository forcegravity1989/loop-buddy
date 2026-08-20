# 穿刺笔记：stablyai/orca 源码研究(2026-08-18)

> **30 秒导读**:这是对开源项目 `stablyai/orca`("多路并行 agent 的 IDE")的**源码级**穿刺笔记,看它的终端内嵌、会话生命周期、右侧栏是怎么做的。**为什么还留着**:V4 的会话屏和两个适配模块(`crates/app-shell/src/adapters/terminal_xterm/`、`claude_cli/`)的 README 直接引它当接法依据,删了源码注释就断链。**怎么读**:结论「借模式,不整体嵌」已经采纳并落进代码,这里是当时看到的事实;要看今天怎么实现的,读 [`../design/05-session-screen.md`](../design/05-session-screen.md) 与那两个适配模块的源码。

---

## 1. Orca 现在是什么

**技术栈**(`package.json`、`LICENSE` 实读):
- Electron `^43.1.0` + React `^19.2.7` + TypeScript,渲染层是普通网页(不是原生 UI)。
- 终端:`node-pty ^1.1.0` + `@xterm/xterm 6.1.0-beta.287` + 一整套官方插件(`addon-fit`/`addon-search`/`addon-unicode11`/`addon-web-links`/`addon-webgl`/`addon-serialize`/`addon-ligatures`)+ `@xterm/headless`(无头模拟器,用于历史回放,和 BW 旧笔记提到的 `HeadlessEmulator` 思路一致)。
- 许可证:`LICENSE` 全文是纯 MIT,版权方 "Lovecast Inc.",**没有额外商业条款**。
- 发布形态:桌面客户端 macOS/Windows/Linux(`electron-builder`,dmg/exe/AppImage,还有 Homebrew cask、AUR)+ 同一个 npm 包里内置的 `orca` CLI(`package.json` `bin.orca = "./out/cli/index.js"`)+ 无头服务器模式 `orca serve`(`docs/reference/headless-linux-server.md`)+ 独立的 iOS/Android 手机伴侣 App(`mobile/` 目录,单独的移动端仓库结构)。**不是一个可单独复用的库**,CLI 和桌面壳共享同一份构建产物。

**架构关键变化**(相对 BW 旧笔记里的 "orca-main" 快照):`src/main/daemon/` 这个独立 daemon 目录**在本快照里已不存在**。本地会话的 PTY 现在直接托管在 Electron 主进程内(`src/main/providers/local-pty-provider*.ts`);只有 SSH 远程场景才会把一个叫 `relay` 的二进制部署到远端主机(`src/relay/relay.ts`、`src/main/ssh/ssh-relay-deploy.ts`),本地和远程走同一套帧协议(`src/relay/protocol.ts`)。**BW 旧笔记里"daemon 持有 PTY 跨重启存活"的机制在这份新快照里已经不是这么实现的**,读旧笔记时要留意这个偏差。

**"怎么跑起任意 agent"——核心是 PTY 驱动的终端接管,不是 SDK/stream-json**:
- 适配层就是一张静态注册表:`src/shared/tui-agent-config.ts:53-337` 的 `TUI_AGENT_CONFIG`,给约 30 个 agent(claude/codex/cursor/gemini/grok/opencode/droid/copilot/devin/hermes/pi/kimi/……)各配一条 `TuiAgentConfig`:`detectCmd`(PATH 探测用的二进制名)、`launchCmd`(启动命令,个别平台有 `launchCmdByPlatform` 覆盖)、`expectedProcess`、以及最关键的 **`promptInjectionMode`**(`argv` / `flag-prompt` / `flag-prompt-interactive` / `flag-interactive` / `hermes-query` / `stdin-after-start` 六种)。
- 首条提示词怎么塞进去,按 `promptInjectionMode` 分流:能走位置参数的直接拼 argv;不能的就等终端"看起来准备好接收输入"了再模拟粘贴/回车(`draftPasteReadySignal`,如 `codex-composer-prompt`、`grok-composer-prompt` 这类**逐 agent 手调的终端渲染特征字符串**,`draftPasteReadyTimeoutMs` 兜底超时)。部分 agent 有 `--prefill`/环境变量(`draftPromptEnvVar`)可以"预填不提交",省掉这个竞态。
- 首启动的"信任这个目录吗"弹窗会吃掉粘贴的字符,所以 Orca 会在 `preflightTrust` 标记的 agent(cursor/copilot/codex)启动前**预写信任标记文件**,绕过弹窗。
- **恢复**:`src/shared/agent-session-resume.ts:242-280` 的 `getAgentResumeArgv()` 是一张纯粹的 per-agent resume 命令表,例如 `claude → ['claude','--resume',id]`、`codex → ['codex','resume',id]`、`gemini → ['gemini','--resume',id]`——这条和 BW 自己现在的 `claude --resume <session_id>` 设计是**同一思路**,可以互相印证。这只是"PTY 已死、需要重开"这条路径;"PTY 还活着直接重连"是另一条路径(本次没有重新逐行核对,BW 旧笔记 §5 的描述与本仓库结构仍能对上,不重复穿刺)。
- 工作目录 = 该 agent 会话所在的 git worktree(或"文件夹工作区",见 §4);CLI 参数按 `promptInjectionMode` 现场拼,不存在一张全局"flags 列表"。

---

## 2. 终端 UX 细节

**(a) 选中即复制/复制粘贴**:xterm 6 内置选区 API(不需要额外的 SelectionAddon),挂在 `src/renderer/src/components/terminal-pane/use-terminal-pane-lifecycle.ts:1200-1230` 的 `terminal.onSelectionChange()` 上。默认只是选中,是否"选中即写剪贴板"由设置项 `terminalClipboardOnSelect` 控制;Linux 下还额外支持 X11 "主选区"(中键粘贴),写入有防抖(拖拽选中时不会疯狂写)和长度上限(`terminalSelectionExceedsPrimaryLimit`,避免整屏回滚区都塞进剪贴板)。标准 Cmd/Ctrl+C 复制、全选走右键菜单和快捷键处理(`TerminalContextMenu.tsx`、`keyboard-handlers.ts`)。Web 客户端模式下 `navigator.clipboard` 只在安全上下文可用,`terminal-clipboard-event-paste.ts` 专门处理了这个降级路径——**这是一个已知会踩的坑类别,Orca 显式处理了,不是遗留 bug**。

**(b) 重启后持久化/续接**:依赖 `@xterm/headless` + `addon-serialize` 的快照/回放机制(依赖仍在,和 BW 旧笔记记录的 `HeadlessEmulator` 思路一致);本次没有逐文件重新穿刺这条链路的实现细节,不重复武断结论。

**(c) 每个面板几个终端/标签/分屏**:`orca` CLI 直接暴露了这套模型——`src/cli/specs/core.ts:181-284` 有 `terminal list/show/read/send/wait/stop/create/switch/close/rename/split`,证实一个 worktree 下可以开多个终端、可分屏、可脚本化操作(对应 README "Terminal Splits" 的说法)。渲染侧对应 `terminal-pane/`、`tab-bar/`、`tab-group/`、`floating-terminal/` 几个组件目录。

**(d) 怎么判断 agent 是 idle/等待输入/做完了**:**主要不是靠猜终端输出**。状态类型定义在 `src/shared/agent-status-types.ts:18`:
```
AGENT_STATUS_STATES = ['working', 'blocked', 'waiting', 'done']
```
紧挨着的注释原话:"status comes from hooks (Claude, Codex, etc.) — never inferred from terminal titles; a narrow interrupt fallback synthesizes a final `done`"。具体做法:对支持原生 hook/statusline 机制的 CLI(Claude Code 首当其冲,`src/main/claude/hook-service.ts`、`statusline-script.ts`),Orca 会把一个"托管 hook"写进该 CLI 自己的配置文件里,由 CLI 在生命周期节点主动 POST 到 Orca 起的一个**仅监听 127.0.0.1 的本地 HTTP 服务器**(`src/main/agent-hooks/server.ts:2520-2543`,`this.server!.listen(0, '127.0.0.1', ...)`,随机空闲端口 + token 鉴权)。这个 server 对约十种 agent 分别做了归一化解析(`src/main/agent-hooks/server-claude-normalization.ts`、`server-codex-normalization.ts`、`server-cursor-normalization.ts`、`server-gemini-normalization.ts`、`server-opencode-normalization.ts` 等,一 agent 一份适配)。终端渲染特征(`draftPasteReadySignal`)只用于"判断composer 准备好接收粘贴了没",不是"活干完了没"的信号源。

---

## 3. 右侧边栏 / 代码结构

组件全在 `src/renderer/src/components/right-sidebar/`(单目录 200+ 文件),挂载表在 `right-sidebar-panel-content.tsx:6-38`,一共这些 tab:`explorer`(文件树,`FileExplorer.tsx`)、`source-control`(Git 状态/暂存/提交/diff/发 PR,`SourceControl.tsx`)、`checks`(CI/GitHub Actions/GitLab 流水线状态,`ChecksPanel.tsx`)、`ports`(SSH 端口转发)、`vault`(跨账号/跨 CLI 的会话浏览器,`AiVaultPanel.tsx`)、`workspaces`(一个文件夹下的多个 worktree)、`pr-checks`、以及沙箱化的插件 tab。

**数据源**:文件树 = 文件系统 `readdir` + 自建 watch(`file-explorer-watch-path.ts`、`file-explorer-watcher-reconcile.ts`,`package.json` 里**没有** `chokidar` 依赖,是自研的);Git 状态/diff = 包一层 git 命令行(`src/relay/git-handler.ts` 及同目录十余个 `git-handler-*-ops.ts`,`package.json` 里**没有** `simple-git`/`isomorphic-git`,是自研 wrapper);全文搜索走 ripgrep(`src/relay/fs-handler-install-rg.ts` 会按需装 rg 二进制)。

**明确的结论**:Orca 的右侧边栏**没有符号大纲/AST/tree-sitter 面板**(`package.json` 无 tree-sitter 相关依赖,组件目录里也找不到对应面板)。用户说的"代码结构"在 Orca 里落地成两件事:**文件树 + Git 改动文件/diff**,不是语义级的符号地图。这对 BW 判断"侧边栏能看到代码结构"该做多大范围是个有用的参照——业界这一档产品把这四个字实现成文件树+diff 就算数了。

---

## 4. Worktree / 并行 agent

**隔离单位** = 一个 git worktree(非 git 目录则叫"文件夹工作区",`AGENTS.md` "Folder Workspace Use Case" 明确要求两种都要考虑)。创建与启动是**一条命令**:
```
orca worktree create --repo <selector> --agent <id> --prompt <text> \
  --issue <n> --linear-issue <id-or-url> --parent-worktree <selector> --json
```
(`src/cli/specs/core.ts:86-135`)。底层 `git worktree add` 包在 `src/relay/git-handler-worktree-ops.ts:31` 的 `addWorktreeOp()`。

**并行 fan-out 怎么做的**:README 说"一条 prompt 分给五个 agent,各自独立 worktree,比完手动挑赢家合并"——源码里**没有一个专门的"fan"原语**,机制就是对同一个 base 分支重复调用 N 次 `orca worktree create --agent X --prompt <同一段文字>`,各自独立 worktree/分支/PTY,合并靠人工挑选。

**Issue/任务映射**:一个 worktree 卡片上可以挂 `issue` / `linear-issue` / `jira-issue` 三种可选属性之一(`src/shared/worktree/card-properties.ts:7-11`,`TASK_WORKTREE_CARD_PROPERTIES`),对接代码在 `src/main/linear/`(`issue-context*.ts`、`mappers.ts`)、`src/main/github/`、`src/main/gitlab/`。渲染侧还有一块自建的看板 `WorkspaceKanbanDrawer.tsx` 及一批同前缀文件(`src/renderer/src/components/sidebar/WorkspaceKanban*.ts*`),按本地派生的 `WorkspaceStatus` 把 worktree 卡片分泳道、支持拖拽——**这是 Orca 自己的轻量看板,按 worktree 分组,不是把外部 tracker 的看板搬进来**。整体映射关系是:一个外部 issue 可以对应**多个**并行 worktree(即多个 agent 尝试),一个 worktree 至多关联**一个**外部 issue。

---

## 5. 给 BW 的三个接法评估

BW 是 Rust 内核 + Dioxus 0.7/wry WebView(WebView 本身能跑任意 JS,包括 xterm.js),不是 Electron;内核已有一版跑在 PTY 上的交互式执行器(Windows conpty + macOS portable-pty,待合入)。

**(A)整体内嵌 Orca 作为旁路进程,靠深链/CLI 打开会话**
- 现实性:`orca` CLI 是真实、完整、可脚本化的本地自动化面:`orca worktree create --agent <id> --prompt <text> --json` 一条命令建目录+发 agent+带首条提示词,`orca terminal create/send/read/wait` 可以纯脚本化驱动一个已开的终端,`orca open`/`orca status` 判活。`orca serve` 还有专门的无头模式(`docs/reference/headless-linux-server.md`)。
- 硬伤:Orca 依然是一个完整的 Electron 桌面应用(~300MB+ 体量),把它当"旁路"意味着 BW 用户机器上要装并维护第二个独立发行的 GUI 应用,和 BW"单一 Rust 原生二进制、非 Electron"的产品立场直接冲突;真正跨进程通讯的是 CLI 客户端连本地 Unix socket / Windows 具名管道(`src/cli/runtime/transport.ts:23-33`,`findTransport(metadata,'unix','named-pipe')`),这个 socket 协议虽然内部有版本化纪律(`docs/reference/remote-wire-compatibility.md` 讲得很细),但**只对外暴露 `orca` CLI 这一层,socket 本身不是面向第三方的公开契约**,直接接协议风险高。
- 结论:**不推荐作为主路径**。只有当 BW 想给"本机已经装了 Orca 的重度用户"提供一个"甩给 Orca 处理"的逃生舱时才值得做一次性能验证,工作量集中在打通 CLI 调用+解析 `--json` 输出,量级：小(几天);但要接受"这活儿离开了 BW 的 UI"的产品代价,不建议作为常态路径。

**(B)只借模式/组件,搬进 BW 自己的 WebView(推荐)**
和 BW 早年对旧版 orca-main 的做法(`docs/archive/v1-prototype/orca-terminal-session-reference.md`)一脉相承——只借架构思路,不借 TypeScript/Electron 代码。这次值得借的东西:

- **多 agent 适配器的分类法**(不是代码,是设计模式):`src/shared/tui-agent-config.ts` 那张 `promptInjectionMode` 六分类(argv/flag-prompt/flag-prompt-interactive/flag-interactive/hermes-query/stdin-after-start)+ `draftPasteReadySignal` 竞态处理,可以直接套进 BW 的 `docs/v3-prototype/cursor-agent-executor.md` 里"以后接第二个 CLI 怎么配"的设计——目前那篇文档还没细分到这个粒度。
- **恢复命令表**:`src/shared/agent-session-resume.ts:242-280` 印证了 BW 现有 `claude --resume <session_id>` 的设计方向没跑偏,以后加别的 CLI 时可以照抄这张表的形状(每个 agent 一行 resume argv)。
- **终端选中即复制的完整实现**:`use-terminal-pane-lifecycle.ts:1200-1230` 这段(防抖 + 长度上限 + 是否写主选区/剪贴板可配)是直接可抄的 UX 细节,BW 已经在用 xterm.js,这段可以照着重写一份 Rust/JS 胶水代码。
- **原生 hook 优先于终端猜测的状态判定思路**——这是本次穿刺里对 BW 最有价值的一条:Claude Code 自带的 hooks/statusLine 机制是**CLI 官方能力**,不是 Orca 专有协议;BW 的执行器目前只接 Claude,完全可以直接照抄"往 `~/.claude/settings.json` 写一条托管 hook,本地起一个 loopback HTTP 服务收状态"这个模式(不需要 Orca 那套多 agent 归一化层,BW 只服务一种 CLI),把 idle/等待/完成的判定从"猜终端输出"升级成"CLI 主动上报"。这比照抄 xterm 复制逻辑价值更大,建议优先立项。
- **侧边栏范围参照**:确认"代码结构"侧边栏做成"文件树 + Git 改动/diff"就是业界这一档的常见落地(§3),不用做符号级大纲也算交得出货,给 BW 定 MVP 范围时可以直接引用这条作为参照点。

**(C)复用 Orca 的 daemon,走它的 IPC 协议**
- **这个选项在当前快照里不成立**:BW 旧笔记里的独立 daemon(`src/main/daemon/*`)已经不存在,本地 PTY 现在直接跑在 Electron 主进程里(`src/main/providers/local-pty-provider*.ts`),没有可以脱离 Electron 主程序单独复用的"PTY host"进程。唯一可达的通讯面是 §5(A) 提到的那条 CLI↔本地 socket 协议,而它要求一个**活着的 Orca 桌面进程**在背后,并且协议本身不是公开契约——绕开 CLI 直连 socket 没有比 (A) 更划算,反而失去了 CLI 的稳定包装层。**结论:C 并入 A 讨论,不单列为可行路径。**

**推荐**:主推 **(B)**——继续"只借模式不借代码"的既定路线,这次新增两条高价值可借项(适配器分类法、原生 hook 状态上报),其中 **hook 状态上报**建议列为下一个可以直接立项的小任务(BW 只服务 Claude Code 一种 CLI,改动面比 Orca 小得多)。(A) 仅作为长期"逃生舱"备选,不investment。(C) 不再单独考虑。

---

## 6. 和 BW 产品命题的重叠检查

明确说:**没有重叠**。逐项核实:
- 全仓搜索 "weekly plan"/"health signal"/"healthScore" 关键字**零命中**——Orca 没有"周计划""健康信号"这类概念。
- Orca 有一个 `dashboard`(`src/renderer/src/components/dashboard/`):是运行中/排队中/已完成 agent 会话的**实时运维看板**(按 bucket 分组、显示血缘关系),性质是监控台,不是从数据派生的红黄绿信号,也没有"无数据=灰、不许手设"这类约束。
- Orca 有 `skills`(`skills/` 目录 + `src/renderer/src/components/skills/`):是**agent 技能包的打包/分享/安装/版本新鲜度提醒**系统(团队间分发可复用的 CLI 指令包),没有"从做完的 Issue 蒸馏出技能、记录来源、按胜率注入"这套闭环。
- 没有五阶段生命周期、交棒、「同一件活绝不记两次」的记账约束、「健康只能从数据推导」这类概念的任何对应物。

一句话:Orca 是"agent 舰队驾驶舱"(会话/终端/worktree/代码评审为中心),BW 是"项目管理方法论工作台"(阶段/交棒/健康/复利为中心),两者在产品命题层面不是同一物种,只在"怎么把一个 CLI agent 塞进 PTY 里用好"这个工程子问题上有交集(即 §1-§3、§5)。

---

_本篇为一次性穿刺记录,不随 orca 上游更新维护。orca 源码锚点行号对应克隆时刻的 main 分支快照,上游变动后可能漂移,复核时以当次拉取的源码为准。_
