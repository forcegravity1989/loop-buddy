# 05 · 会话屏

> **30 秒导读**:这篇管 V4 会话屏——一件活怎么在这个屏里被 agent 真干起来、人怎么在这里陪着看。给接着做 V4 的会话、以后要再接一个开工工具的同事看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 三栏主体(内嵌终端 / 文件树 / 改动 diff / git 状态 / MR 号)已经能用;Cursor 适配、内嵌 Open Design、蒸馏按钮、agent 四态回报这四样还没做,在 §3 末尾如实列着。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

## 0 · 这篇管什么、不管什么

**管**:会话屏这一整屏——左列会话怎么按目录/worktree 分组、中栏终端怎么嵌入(能续接、能复制)、文件只读视图和 diff 从哪来、右栏文件树/改动文件/git 状态/MR 卡怎么生成、顶部一行的四个按钮和 agent 状态怎么回报、Open Design 怎么嵌进来、Cursor 这条新执行器怎么接、蒸馏这个动作摆在哪。对应母文档第 4 站全部、§2.6 全景表里「业务活开工」与「蒸馏」两行、§5 信息架构表里的「会话」入口、待拍-09(开工工具三个)/待拍-11(借 Orca 的模式)/待拍-22(右侧栏首版范围)/待拍-24(工具映射三列)。

**不管**:开工工具与 workflow 怎么注册、`.bw/issue-policy.toml` 三列映射怎么解析、技能怎么按类别默认注入——[04-tools-and-workflows.md](04-tools-and-workflows.md)的地盘,本篇只消费其产出(某张活的 `tool` 字段、要注入的技能正文)。规范铺底、AGENTS.md 五层怎么写进仓——03 篇的地盘。周计划、看板拖拽——06 篇。项目群通知——07 篇。

**与 01 篇的边界**:01 篇定"新壳建法"三条规矩——一屏一模块、一个外部能力一个适配模块、文件行数守卫。本篇把"会话屏"这一个屏幕模块、和它挂的几个适配模块(终端、Claude CLI、Cursor、Open Design、codegraph、agent 状态回报)在这三条规矩下具体铺开,§2.6 和 §3 是落地处。

## 1 · 用户看到什么、做什么

人在计划屏(或总览的待人处理)点开一张活的「去会话」,或者直接在项目内左栏点「会话」入口,进的是同一个屏。

**左边**是一列会话,按目录分组:最上面永远有一行「仓根 · main」,是一个常驻的纯 shell 会话——不跑 agent、不注入提示词,就是项目主工作区里的普通终端,给人手动敲 `git log`、看状态用,不用自己另开 PowerShell。往下每一件正在进行或刚推到评审的活各占一组,组名是这件活的 worktree 目录名(比如 `loop-buddy-issue-102`)加分支名(`bw/issue-102`),行上写着活标题、状态点(运行中的绿点/等你输入的黄点/空闲的灰点)、最近一次动静的时间。点一行切到那件活的会话。

**中间**是标签页。默认打开「终端」——那件活正在跑(或跑过)的交互式对话,内容和真终端一样能选中复制。人在右边文件树点开任意文件,中栏新开一个「只读」标签;点右边改动文件列表的一行,新开一个 diff 标签。原型类活走 Open Design 开工时,中栏多一个「Open Design」标签,是嵌进来的设计器网页,人可以直接在里面和 agent 一起调原型,不用切出工作台。

**右边**是这个 worktree 的代码结构侧栏:一棵可展开收起的文件树(点文件即在中栏打开)、一份改动文件清单(点一行即开 diff)、一小块 git 状态(分支名、领先/落后主干几个提交)、开了 MR 就有一张 MR 卡(号码、状态、检查是否通过、「合入」按钮)。

**顶部**一行是活标题、当前状态和动作条:▶开工(未开工或已停下时可点)、■停止(中止不代表放弃,活留在原地能重跑)、**提交并开 MR**(agent 干完之后由人点这一下,把这棵树里的改动提交、推分支、开 MR,活进评审中)、只推到评审(纯改状态,不碰仓)、蒸馏(把过程蒸成一篇技能,以后同类活自动用;还没做)。**没有任何东西会自动开 MR**——什么时候算干完由人说了算。人点▶开工,buddy 按这张活配的开工工具(Claude CLI / Cursor / Open Design 三选一,规则见 04 篇)真的跑起来;终端类工具在中栏终端里看着 agent 敲命令、改文件、跑测试,网页内嵌类工具在 Open Design 标签里看着它画图。活推到评审中后,人点合入、点完成——那是第 5 站的事,本篇只到"活被推到评审中"为止。

## 2 · 设计

### 2.1 左列:会话按目录/worktree 分组

**每个活最多一个会话,不是新规则,是复用已有的库约束**:`claude_conversation` 表上 `issue_id` 是唯一键(`crates/bw-core/src/model.rs` 的 `ClaudeConversation`),一件交互式活最多绑一行会话身份——buddy 自己的 `ConversationId`、claude CLI 回传的 `claude_session_id`(用于 `--resume`)、固定的 `workspace_path`、分支名。**这张表已经是"会话与活的对应关系"的正本**,左列分组直接读它 join `issue`,不需另建会话表。真正在跑的 PTY 进程(子进程句柄、字节流 channel、当前终端尺寸)是纯内存的 `TerminalManager`,进程死了就消失,重启后从 `claude_conversation` 行恢复身份(同一 `workspace_path` 重建 worktree,`claude_session_id` 拿去 `--resume`)——这条路径 V1 已走通,V4 原样沿用。

分组规则:
- **仓根 · main**:每个项目固定一行,不对应任何 `IssueId`,点开就是主工作区目录下的一个纯 shell PTY(`bash`/`zsh` 或 Windows 下的 PowerShell,取用户本机默认 shell,不传任何系统提示词、不传首条消息)。这一行不进 `claude_conversation` 表——它根本不是一次 agent 会话。
- **每个活一行**:凡是这个项目里状态在"进行中"或"评审中"、且有 `claude_conversation` 行(说明真的开工过)的活,各占一组;组名 = worktree 目录名(约定 `<主工作区目录名>-issue-<n>`,真代码见 `crates/bw-engine/src/workspace.rs` 的 `provision_issue_worktree`)+ 分支名(`bw/issue-<n>`,`crates/bw-engine/src/github.rs::issue_branch`)——这两条命名约定已核实是真代码。
- **agent 状态点**(运行中/等你输入/空闲/已推评审)怎么来见 §2.4,不是猜终端文字猜出来的。
- **"+ 新会话"少用**:因为 `claude_conversation.issue_id` 唯一,一件活不能有第二个 agent 会话——点"+新会话"开的是同一个 worktree 目录下**再加一个纯 shell PTY**(和仓根那行同性质,不进 `claude_conversation` 表),用途是人想在 agent 跑着的同时手动敲一条命令(比如单独跑一次测试),不是给这件活开第二个 agent。这也是它"少用"的原因——能力边界本来就窄。

### 2.2 中栏:标签页

**终端**(默认标签,`pty_backend.rs` 提供的 PTY 后端,Windows conpty-oxide / macOS·Linux portable-pty 两家都已验证能起子进程、能读回、能收尾、中止不留孤儿——见 §5)。**能续接**:关掉会话屏再回来,只要 `claude_conversation` 行还在、`claude_session_id` 非空,`build_resume_plan` 就用 `--resume <session_id>` 精确接回(不是"最近一次"那种模糊的 `--continue`),已是真代码。**内容可复制**:今天 xterm 是 5.3.0,`orca.md` §2a 指出 xterm 6 内置原生选区 API,挂在 `onSelectionChange()` 上,不需额外插件;V4 升级到 6,加一个"选中即写剪贴板"开关(默认开)+ 一个「复制」按钮兜底(高保真原型 `terminalHTML` 已画了按钮,还没接真复制)。实现细节记进 `adapters/terminal_pty/README.md`:借自 Orca `use-terminal-pane-lifecycle.ts` 的选区防抖+长度上限那段,不借它的 `@xterm/headless` 回放机制(见 §4)。

**打开的文件(只读视图)**:从右栏文件树点开,纯文本 + 可选关键字高亮(高保真原型的 `codeViewHTML`/`highlightLine` 已证明"关键字正则染色"这个量级够用,不接整套语法高亮库,以后要换不影响这层接口)。

**改动文件的 diff**(单文件级,不是整个 PR 的合并视图):点右栏改动文件列表打开,数据源是子进程 `git diff`。待拍-22 已定这是首版范围——不做符号级大纲,不做多文件并排 diff。

**内嵌 Open Design**(仅原型类活出现):沿用 `crates/app-desktop/src/open_design.rs` 已有的 `discover_web_url()` 探活——先看 `BW_OPEN_DESIGN_URL` 环境变量,没有就按平台查具名管道/unix socket 的 STATUS 端点,拿到本机跑着的 Open Design web 侧车的 loopback HTTP 地址,嵌进中栏标签页的 WebView,不改探活逻辑本身。

### 2.3 右栏:文件树 / 改动文件 / git 状态 / MR 卡

待拍-22 已定首版范围:文件树 + 改动文件与 diff + git 状态 + MR 卡,**不做符号级大纲**(留口见 §3)。`docs/v4-prototype/research/orca.md` §3 核实过,业界这一档产品(Orca)的右侧栏就是文件树 + Git 改动/diff,没有 AST/符号面板,这个量级本来就够交货。

- **文件树**:这个 worktree 目录下的文件,懒加载展开,忽略 `.git`/`target`/`node_modules`;点文件即在中栏打开只读视图。数据源是 `evidence.rs` 已有的 `list_workspace_files`(今天是一次性扁平列表),V4 在它上面包一层懒加载目录结构(新函数,见 §3)。
- **改动文件列表**:`git status --porcelain` 的结果,列出相对上次提交的改动(不管提没提交)。今天没有现成函数——`workspace.rs` 只有 `diff_numstat`(比较两个已落库的 commit),不适用于还没提交的进行中状态,需要新函数,见 §3。
- **git 状态**:分支名、领先/落后主干几个提交、有没有未提交的改动——新函数,见 §3。
- **MR 卡**:号码、状态(开着/已合入)、检查是否通过、一个「合入」按钮。合入按钮直接调已有的 `Command::MergeIssuePr`,不新造命令;"检查是否通过"今天没有对应查询函数(`github.rs` 现有的只有 `open_pr`/`merge_pr`/`issue_state`),如实记进 §3 的缺口。

### 2.4 顶部一行:活标题 / 状态 / 动作条 / agent 状态怎么回报

动作条上是:▶开工 / ■停止 / **提交并开 MR** / 只推到评审 / 蒸馏。前四个对应已有命令
(`RunIssue` / `CancelRun` / `SubmitIssueWork` / `TransitionIssue`,见 §3),蒸馏还没有命令、
是灰的。本篇重点是"▶开工按工具怎么分发"、"干完了怎么交出去"和"agent 状态怎么真实回报"
这三件事。

**干完了怎么交出去(第 4 站到第 5 站那一下)**:agent 是在这张活自己的 worktree 里改的
文件,那棵树和它的分支在别人那儿是看不见的。**「提交并开 MR」就是把它变成可评审的
东西**:把树里的改动提交掉(agent 自己已经提交过的就跳过)→ 推分支 → 开 MR → 活进
「评审中」。三条守则:

- **由人点,而且最远只到「评审中」**。agent 什么时候算干完只有人知道;「完成」还是得
  评审完之后再点一次(那条边在状态机上只有「评审中 → 完成」这一个入口)。
- **没干出东西就如实弹回**。判据是这棵树比主检出多几个提交,不是"有没有文件动过";
  0 个就直接拒绝,活留在原地,不留分支、不留 MR 号。
- **每一条不做的理由都说出来**。没挂远端、推分支失败、开 MR 失败,都写进事件正文;
  界面上绝不出现一个来历不明的空 MR 号,失败也不假装进了评审,可以重试。

旁边那个「只推到评审」是**纯状态动作**,不碰仓——留给"改动不在这棵树里"或者"远端还
没挂上"的情况。计划屏把卡拖到「评审中」也是同一件事,那个确认框里写明了。

**▶开工的分发**:一张活的开工工具字段(`tool`:claude_cli / cursor / open_design,母文档 §6 已定义在 `issue` 表上)由 04 篇的工具映射决定默认值、人可在活上换。会话屏只认两类接法(母文档 §3 第 4 站已定,预研见 `deepseek-harness.md` §5 路线 C):
- **终端类**(Claude CLI、Cursor):在 PTY 里起一条命令,注入 buddy 系统提示词 + 这件活的技能/workflow 正文(正文内容由 04 篇的注入规则决定,本篇只管这段文本最终被塞进 PTY 启动命令的哪个位置)。Claude CLI 已经是真代码(`interactive_cli.rs` 的 `build_startup_plan`);Cursor 今天是设计稿未落地,落地做法见 §3。
- **本机网页内嵌类**(Open Design):探活拿 URL,嵌进中栏标签页,见 §2.2。

**agent 状态怎么回报,不靠猜终端文字**:待拍-11 明确借 Orca 这一条——"原生 hook 优先于终端猜测的状态判定思路"。这在 buddy 里**不是新引进的能力,是已真实存在、只是覆盖面不够**:`hook_listener.rs` 今天已是一个真实运行的本机回环 HTTP 服务器,固定监听 `127.0.0.1:51790`,往 `~/.claude/settings.json` 写一段 curl 命令当 hook,收 claude CLI 的 `SessionStart`(捕获 `session_id`,供下次精确 `--resume`)和 `Stop`(agent 完成一轮等输入,今天用来触发评审中轮询)两种事件。**左列四态在这套已有机制上再接两个事件即可**,不用另起一套:

| 状态 | 来源 |
|---|---|
| 运行中 | `SessionStart`(会话刚起)或新增的 `PreToolUse`(agent 正在调用一个工具,说明它没在等人)|
| 等你输入 | `Stop`(agent 说完一轮话,等下一句)或新增的 `Notification`(claude CLI 需要权限确认,或空等用户超过一段时间时会发这个事件)|
| 空闲 | 这个 worktree 没有活着的 PTY 会话(`TerminalManager::is_live` 为假),且这件活也不在"进行中" |
| 已推评审 | 不靠 hook,直接读 `issue.status == InReview`(远端有开着的 MR 这条既有推导链,不新开一条信号源)|

端口不新开:一个 buddy 进程一个监听端口,服务这台机器运行期间的所有会话,按 `cwd`(worktree 路径)路由到具体哪件活(`interactive_sessions: HashMap<String, IssueId>` 已是真字段),不是每个会话单开一个端口。

**Cursor 没有这套 hook 机制,状态如实退化**:Cursor Agent CLI 不提供等价的生命周期回调,唯一能拿到的信号是"这个 worktree 有没有活着的 PTY 进程"——所以 Cursor 会话左列只显示两态:**运行中**(进程活着)/ **未知**(不猜是干完了还是卡住了),绝不伪造"等你输入"这种细粒度状态——没有数据支撑的展示,和"没数据就是 Unknown、绝不假装绿"是同一条精神。

### 2.5 蒸馏

顶部「蒸馏」是活的附属动作,不是新命令——直接复用已有的 `Command::DistillSkillFromIssue`。点一下起一次真实的交互式会话(和▶开工走同一套 `InteractiveExecutor`),agent 把这件活的过程整理成一篇技能草稿,人确认名字/描述/正文,直接产出到项目仓 `.claude/skills/`——**不再有落库这一步**:V1/V2 时代有一张 `skill` 表登记技能与来源活的关系,02 篇盘点时判定"没人取的不存",这张表连同它的 `source_issue_id` 列一起被取消(仓内 `.claude/skills/**/SKILL.md` 扫目录即得,见 02 篇 §2.5/§2.6)。「记着来源活」这件事因此挪进产出的文件本身——具体用哪个 frontmatter 字段名装来源活号,留给 [04-tools-and-workflows.md](04-tools-and-workflows.md)(技能包格式的地盘)定,05 只提出这条诉求。这条蒸馏链路本身(交互式会话产出草稿、人确认、写仓)V1/V2 已用真实数据跑通,V4 只是把入口从旧界面搬到会话屏顶部、去掉库这一步,别的机制不变。

### 2.6 模块边界

`screens/session/`(桌面壳新目录,母文档 §7 待拍-17"新壳+旧内核")**只做布局与状态**——三栏 + 顶部条的 Dioxus 组件、从 `Command`/`Event` 读出来拼成的 ViewModel,不直接碰 PTY、不直接拼 `git` 命令行。真正干活的是一批适配模块,每个对应一个外部能力,各自一份 README 记"借自哪、借了什么、没借什么"(01 篇的建法原则):

| 适配模块 | 现状 | 对外暴露 |
|---|---|---|
| `adapters/terminal_pty/` | **已有**,`bw-engine::{pty_backend, terminal_manager}` 沿用,V4 只加前端复制 | `attach/input/resize/drain_events`(即 `TerminalManager` 已有四个方法) |
| `adapters/claude_cli/` | **已有**,`bw-engine::interactive_cli` 沿用 | `build_startup_plan`、`build_resume_plan`、`run_skill_pty` |
| `adapters/cursor_cli/` | **未落地,V4 新建**,依据 `docs/v3-prototype/cursor-agent-executor.md` | 对齐 claude_cli 三个函数形状(写 `AGENTS.md`(仓根) 代替 `--append-system-prompt`、`create-chat`+`--resume`、复用同一 PTY 后端) |
| `adapters/open_design/` | **已有**,`app-desktop::open_design::discover_web_url()` 沿用 | `discover_web_url() -> Option<String>` |
| `adapters/agent_hooks/` | **已有,V4 增量**,`bw-app::hook_listener` | `bind`、`spawn`、`install_hooks_config`(今天只装 `SessionStart`+`Stop`,增量到四个事件,见 §3) |
| `adapters/codegraph/` | **未建,留口**,待拍-22 | `symbols_for_file(path) -> Vec<SymbolRow>`,今天不实现,调用点先返回 `None` |

## 3 · 工程对照

> 下面写的是真代码,不是计划。落点在 V4 自己的两个 crate 里(`crates/bw-v4` + 
> `crates/app-shell`,见 01 篇 §2.1),旧壳 `app-desktop` 一行没用到。

**目录**:界面在 `crates/app-shell/src/screens/session/mod.rs`(三栏布局 + 顶部条,只管布局
与状态);内嵌终端在 `crates/app-shell/src/adapters/terminal_xterm/`(xterm.js 资产、初始化
脚本、PTY 字节桥,一份 `README.md` 记借了什么没借什么)。内核侧新增
`crates/bw-v4/src/app/session.rs`(PTY 生命周期)与 `crates/bw-v4/src/git.rs` 里的四个现算
查询。`bw_engine::{interactive_cli, pty_backend, terminal_manager}` 原样沿用,一行没改。

**库**:只用 `claude_conversation` 一张表,存的是**身份**(`ConversationId` / `--resume` id /
worktree 路径 / 分支名),`issue_id UNIQUE` 就是「一件活最多一个会话」这条规则本身。进程本
身在内存的 `TerminalManager` 里,死了就没了,也不该存。`workflow_run` 表在 02 篇已取
消,所以本屏**不展示成败与耗时**——一轮跑成没成看远端 MR 合没合入,不查任何 `outcome` 列。

**命令**(全部在 `crates/bw-v4/src/command.rs`,不是 `bw-app`):

| 命令 | 干什么 |
|---|---|
| `RunIssue { id }` | ▶开工。工作区目录在就起内嵌终端跑真 claude;不在就退回阻塞那条路用自我标注的替身,产出带【mock】字样 |
| `CancelRun { id }` | ■停止。**只关 PTY,状态原地不动**——停下来既不是失败也不是完成 |
| `SubmitIssueWork { id }` | 「提交并开 MR」。提交这棵树里的改动 → 推分支 → 开 MR → 推到评审中。这棵树比主检出没多出提交就如实弹回,状态不动;MR 没开成也照实说原因,不摆空号 |
| `TransitionIssue { id, to }` | 只推到评审(纯状态动作)。Done 那条边由 `bw_core` 的状态机守着,会话屏根本没有「完成」按钮 |
| `MergeAndSettle { id }` | 通知屏的「合入并完成」,不在本屏(见 07 篇) |
| `TerminalInput { conversation_id, bytes }` / `TerminalResize { conversation_id, cols, rows }` | 键盘与尺寸 |

`OpenFileTab`/`OpenDiffTab`/`ExpandTreeDir` 三样**没有做成 `Command`**:它们不改任何
数据,是纯导航。做成命令会让「命令 = 会改变什么」这条线变模糊。实际走的是桥自己的
`Req::{SelectSession, SessionTab, ToggleDir, OpenFile}`(`crates/app-shell/src/bridge/mod.rs`),
只改壳这边的 `UiState`,一律不进库。

**PTY 字节流不走 ViewModel、也不走 `Event`**:桥上单开一条 `watch` 通道
(`Bridge::pty`),内核线程 60ms 一跳 `App::drain_pty_events()` 往里推。理由有两条,都是真
的会出事:终端一秒能吐几百批字节,每批都重拼一次 ViewModel 会把界面拖垮;而且字节是一次性
的流,进了 ViewModel 每次重渲染都会被重新写进终端一遍。

**agent 状态:只有两态,而且都是真的。** 设计里的四态(运行中 / 等你输入 / 空闲 / 已推评审)要
靠 claude 的 hook 回传 `Notification`/`PreToolUse`,**这一步没做**——`hook_listener` 那套增量
留在 `docs/LEFTOVERS.md`。今天唯一真实的信号是 `TerminalManager::is_live`:进程在 = 运行中,
不在 = 空闲。「等你输入」不显示,不猜。这和「没数据就是 Unknown、绝不假装绿」是同一条精神。

**右栏四类数据全部现算**,函数在 `crates/bw-v4/src/git.rs`:

| 要什么 | 函数 | 说明 |
|---|---|---|
| 文件树 | `git::list_dir(ws, rel)` | 懒加载,点开哪层读哪层;跳过 `.git`/`target`/`node_modules`;目录在前、同类按名字排 |
| 改动文件 | `git::changed_files(ws)` | `git status --porcelain`,提没提交都算;改名行取箭头右边的新名字 |
| 单文件 diff | `git::file_diff(ws, rel)` | 暂存 + 未暂存两段拼起来;没跟踪的文件退回全文并标一行,不给人看空白 |
| 分支状态 | `git::ahead_behind(ws, "main")` | 问不出来返回 `None`,界面显示「—」,**不显示 0**(0 会被读成「和主干一样」) |

**MR 卡**只有号码,没有「检查是否通过」那一列 —— 对应 `gh pr checks` 的函数今天不存在,
是已知缺口(见 `docs/LEFTOVERS.md`)。

**没做的三件**,如实列在这里而不是留在正文里当描述:Cursor 适配(`CURSOR` 常量仍
`supported: false`)、内嵌 Open Design 页签、蒸馏按钮(`DistillSkillFromIssue` 在 V4 还没有对
应命令)。codegraph 留口同样没建。

## 4 · 边界与失败

**不做**:
- **多标签同活多会话**——`claude_conversation.issue_id` 唯一键决定一件活只有一个 agent 会话身份;Orca 那种"同一 prompt 分给五个 agent 各自 worktree 比赛"的 fan-out 模式,母文档旅程里没有这一步。
- **符号级大纲**——待拍-22 已定首版不做,留口见 §3,是刻意延后不是忘了。
- **终端录像/回放**——Orca 靠 `@xterm/headless` + `addon-serialize` 做这件事,预研已明确只借选区/复制,不借回放;buddy 的会话记录本来就是"终端滚屏 + `session.jsonl` 本身就是记录",不需要再录一份视频。

**失败如实显示,不假装成功**:
- **claude 未装 / `.cmd` 探不到**:`claude_bin::resolve_claude_binary` 找不到候选路径时落回裸 `"claude"` 走 PATH,PATH 也没有就 spawn 失败;▶开工按钮可点,点了终端里直接看到"claude: command not found"式原始报错,状态点保持"空闲",绝不误标"运行中"。
- **信任对话框挡住**:claude/agent 首次进新目录的信任确认会吃掉自动发送的首条消息(Orca 的 `preflightTrust` 专门处理过这个坑,`bw-engine` 今天没有等价处理)。V4 第一版如实显示终端画面(人手动按一下确认即可),不做自动按键模拟。
- **PTY 死**:`run_skill_pty` 返回错误或子进程异常退出,状态点变灰、状态词写"已停止"、`summary` 带真实错误原文,人可重新点▶开工——`pty_smoke` 的 `--teardown`/`--abort` 已验证收尾不留孤儿(见 §5)。
- **worktree 建不出**:`provision_issue_worktree` 失败(磁盘满、`git worktree add` 冲突等)时▶开工禁用,会话行显示"worktree 未就绪"+ 真实错误原文,不生成假会话行。
- **Open Design 未起**:`discover_web_url()` 返回 `None`,标签显示"未探测到本机 Open Design,请先启动它",不是空白 iframe。
- **Cursor 未装**:探活(`agent --version`)失败时配置面标"未安装";活选中 Cursor 但探活失败,▶开工前置校验直接拒绝报错,**不悄悄退回 Claude CLI**——静默换工具会让人误以为在用一个工具、实际在用另一个,失去可预测性,不做。

## 5 · 验收与读回

- **`cargo run -p bw-engine --example pty_smoke`** 三种模式,原样沿用、不重做:默认模式起 `bash -c 'echo pty-ok'` 读回字节里确有 `pty-ok`;`-- --teardown` 模拟"用户关掉运行、App 丢掉输入端",断言 5 秒内整个进程组(含一个 `nohup` 出去的孙进程)被连坐清空;`-- --abort` 模拟 `CancelRun` 真实走的 `JoinHandle::abort()`,断言 3 秒内顶层与孙进程都消失。这条验收证明"内嵌终端在这台机器上能真起子进程、真读到输出、真收尾、中止不留孤儿"——会话屏的地基,V4 不改,只是列进本篇验收清单。
- **深链**:`BW_OPEN=<项目名> BW_PANEL=session BW_SEL=issue:<uuid>` 直接打开指定活的会话屏,stderr 的 `[BW_OPEN]` 日志就是"真的渲染到了"的证明。
- **SQL 读回**(核对左列会话身份/最近时间/顶部状态词,均非编造;蒸馏产物已不落库,改用文件读回):
  ```bash
  sqlite3 <db> "SELECT claude_session_id, workspace_path, branch_name, last_opened_at FROM claude_conversation WHERE issue_id='<uuid>';"   # 会话身份 + 左列"最近时间"的数据源(workflow_run 已取消,见 §3)
  sqlite3 <db> "SELECT status, pr_number FROM issue WHERE id='<uuid>';"   # 顶部状态词;这张活"跑成没成"看这一行的 pr_number 对应远端 MR 是否已合入,不查任何库表的 outcome 字段
  grep -rl "<来源活的远端号或标识>" <ws>/.claude/skills/*/SKILL.md   # 蒸馏产物来源(skill 表已取消,见 §2.5);具体标记来源的字段/写法以 04 篇定的技能包格式为准
  ```
- **截图**:会话屏三栏各截一张——左列能看到多个分组、中栏终端里有真实字节滚动、右栏文件树 + MR 卡都有数据,存进 `docs/v4-prototype/`(具体目录跟 10 篇的验收清单一起定,05 不抢先造目录结构)。

## 6 · 开放问题

1. **xterm 升级路径**:5.3.0 → 6 要不要跟着升级配套插件(今天只 bundle 了 `addon-fit`)?复制功能要不要顺带接 `addon-search`/`addon-web-links`?建议先只做复制,其余按需再加。
2. **hooks 端口策略**:今天固定 `51790` + OS 分配兜底。加了 `Notification`/`PreToolUse` 后这两类事件比 `SessionStart`/`Stop` 高频得多(每次工具调用都可能触发),要不要在 `hook_listener` 这一侧做节流/合并,避免刷屏式地发 `AgentStatusChanged`?
3. **Cursor 状态退化的文案**:"未知"要不要拆成"进程活着但不知道在干什么"/"完全没有信号源"两种,还是一句就够?以后再接同样没有 hook 的工具(比如 DSH),这个退化文案要不要做成通用的。
4. **"+ 新会话"要不要限并发**:同一个 worktree 底下人可以一直开新的纯 shell PTY,要不要设数量上限?建议先不设,等有真实使用数据再定。
5. **单文件 diff 该 diff 什么**:working tree 相对上次提交的改动,还是活的分支相对主干合并基点的改动?进行中的活两者接近,但评审中的活人可能更想看"这次 MR 一共改了什么"而非"工作树此刻脏了什么",两者数据源不同,需要用户拍板默认展示哪个。
