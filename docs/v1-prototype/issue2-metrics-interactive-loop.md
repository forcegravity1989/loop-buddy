# V1 Issue 2 · 找指标/绑数据 交互式闭环 + 采集装置归位 — 开发事实源

> 走 (c):现有 buddy app 当底。本文是设计 + 开发唯一事实源(SubAgent 照此建)。5 步法:scope delta ✅(经多轮 grilling 重塑)→ 对齐原型(本文)→ 开发 → 验证 → 填指南。Issue 已提(Issue 2)。
>
> **scope 重塑(边做边发现,如实记)**:原范围「north-star-discovery + metrics-binding 两 skill 文本易用性打磨」。经多轮 grilling 发现 skill 文本是**下游**,真正产品化杠杆在上游三件:① 交互式执行模型(`claude -p` one-shot 出草案 → PR 合不了);② collect_kind 收两 kind(五 kind 是错误心智产物);③ 绑数据 = 搭采集装置(不是只改 metrics.toml collect 字段)。skill 重写 fork 在执行模型上。

## 0. 心智模型(三层 + 采集两 kind + 执行两轨,2026-08-04 钉下,继承 Issue1)

1. **connector = 对外连接器**:codehub-cli/github/claude-cli,提供探活函数(知道怎么连某外部系统 + 探一次通不通)。本地工作区读取不是 connector(`git-repo` 不建,Issue1 已定)。
2. **业务脚本(script)**:干业务活的脚本,可基于某 connector(也可不基于),被定时器调。buddy 自带 instance(包 codehub/github CLI 输出 JSON)或项目侧(`derive_*.py`)。
3. **定时器(cron)**:调度组件,定时调业务脚本(cron→script,script 可选→connector)。
4. **采集两 kind**:`script`(自动:机械解析数据源→产出 JSON,`collect_query`=字段在 JSON 里的点分路径)| `manual`(人填,戴「手填」徽)。`github`/`codehub`/`bw`/`connector` 全是 `script` 的不同 instance(脚本不同但都是脚本),**不是并列 kind**——是历史累积的 inline arm 漏迁,采数/总览窗口收尾收进 script。
5. **执行模型两轨**:`claude -p` one-shot(后台/autopilot 建活,fire-and-forget 对的活)| 交互式 claude CLI(找指标/绑数据,人 in loop 搭装置)。

## 1. 现状(grilling 捋清,取证见行号)

### 1.1 找指标/绑数据 跑通链路(现状)
- 创建流末步 `CompleteCreation`(lib.rs:5342)给项目建三件套 Issue(竞品分析/找指标/绑数据,`seed_standard_issue_trio` lib.rs:2963,全 Backlog/Prototype,挂 `standard_skill` slug,无硬依赖靠软序)+ codehub 2 指标(`seed_codehub_public_metrics` lib.rs:2513,`collect_kind='codehub'`)+ 每日 `CollectMetrics` cron(lib.rs:5028)。
- op.rs Issue 卡点 `▶ 跑`(op.rs:953)→ `Command::RunIssue`(lib.rs:5979)→ `run_issue_now`(lib.rs:3855)→ desktop 走 `run_issue_backgrounded`(lib.rs:4239,`tokio::spawn` 甩后台)。
- `prepare_issue_run`(lib.rs:3885)灌 `PlaybookCtx`(项目 desc/benchmark/opportunity/north_star/ns_def)+ `standard_skill_block`(lib.rs:2435,skill 正文经 `demote_headings` 降二级贴进每 phase prompt)→ 切 `bw/issue-N` worktree(lib.rs:4003)→ `ClaudeCliExecutor`(claude_cli.rs:231)`spawn claude -p`(`--print` 非交互、stdin null、**不传 `--resume`**、每 phase 新 one-shot、`--max-budget-usd` 封顶、`--disallowedTools` 禁 `gh pr merge`)。
- 跑完提 MR(`issue_run_tail` lib.rs:4050,codehub 走 `codehub-cli mr create`)→ **只推 InReview**(lib.rs:4149,绝不自动 Done)。
- **进表**:`.bw/metrics.toml` 在活分支,merge MR 后 `MergeIssuePr` 自动 `sync_default_branch` + `sync_metrics_file_for`(lib.rs:7296)读文件 upsert metric 表(`origin='file'`)。另有运营台手动「↻同步指标文件」按钮(op.rs:1787,plan18-⑦ 补丁)。
- **UI**:IssueDetailOverlay(op.rs:1006)显示运行史(状态/耗时/phases + 文件 diff +N/-M + error)+ 产物登记(artifact 路径+commit+字节)。**工作流屏 WorkflowPanel**(op.rs:411)显示 agent 会话消息产出(op.rs:11/2256/2493,按 phase 配对)。但 `claude -p` 是 one-shot,会话消息是**每 phase 独白**,不是实时来回对话——用户跑的过程插不进话。

### 1.2 采集现状(collect arms)
- `collect_project_metrics`(lib.rs:3108)按 `collect_kind` 分支:
  - `"github"` inline arm(lib.rs:3240 附近):`remote.collect_count` 跑 `gh api search/issues`。
  - `"codehub"` inline arm(lib.rs:3280):`codehub-cli issue|mr list --jq length`(legacy,Issue1 要退休→script)。
  - `"script"` arm(lib.rs:3316):预跑项目 script connector(过滤 `kind==script && project_id`,lib.rs:3138)→ 跑脚本 → 收输出 JSON 进 `script_outputs` → 指标按 `collect_query` 字段路径取值(`json_field_by_path`,lib.rs:3326)→ `append_observation(SourceKind::Script)`。
  - `bw`/`connector`:v1 未接,如实 Unknown。
- `ScriptConnectorConfig{script, output, command}`(lib.rs:7455,command 默认 python)。script connector 探活=查脚本文件在位(lib.rs:2691,不真跑)。
- `CollectKind` 枚举(metrics_file.rs:40)只 5 值(`Github/Connector/Bw/Script/Manual`,**无 Codehub**)——`.bw/metrics.toml` 写 `kind="codehub"` 会 serde 解析失败零写入。`codehub` 只活在 DB inline arm,用户经不到文件。

### 1.3 maas 实态(取证 PRACTICE §3.2)
- 2 条 codehub 指标(开放 Issue 数/已合入 MR 数)`collect_kind='codehub'`(inline arm,未改 script,Issue1 零提交未做)。
- 1 个 script connector「maas·指标脚本」(kind=script,config 指 `governance/.../derive_leading.py`→`data.json`,**SubAgent task2 SQL 直插绕法建,非正规 create_connector**)。
- 4 条 script 指标(北极星/L1/L2/L3)`collect_kind='script'`(接 derive_leading.py 输出)。
- **现状是 makeshift**:script connector 绕过正规创建路径、codehub 指标 inline 不经文件、绑数据靠 SQL 直插不是 skill 引导搭装置。这正是 Issue 2 要正规化的。

### 1.4 guide 现状(穿刺期种子,需 V1 校准)
- u3 找指标 / u4 绑数据 / m4 技能与Prompt / m5 执行与证据 / m6 指标与健康——均已填穿刺期种子内容,需按 V1 实态校准。
- 漂移:m6(L550)写 `script`「计划中」,但契约标 **plan18-③ 已接**;u3/u4 foot 引 `v1-onboarding-overview.html`(仓内不存在);guide `:root` 信号色与 plan/00 §6 不一致。

## 2. 执行模型设计((c) 引擎 + (d) 嵌入层)

### 2.1 引擎((c) 核心,provider 无关)
**保留 `claude -p`(one-shot)** 给 autopilot cron 建活 / fire-and-forget 活(`Executor` trait bw-engine/lib.rs:89 不动,`ClaudeCliExecutor` 只读不改)。

**新增交互式引擎** 给找指标/绑数据:
- `InteractiveCliExecutor`(bw-engine 新文件,impl 一个新交互 trait,见 2.3):PTY spawn `claude`(不带 `-p`,带 `--permission-mode acceptEdits` + `--allowedTools` + `--disallowedTools`,**不传 `--max-budget-usd`**——见偏差 R1),首条 prompt 已注入 skill(经 `playbook.rs:454 skills_block` 同款 `strip_frontmatter`+`demote_headings`,加项目 `PlaybookCtx`)。
- 用户多轮交互(引导式),claude 退出 → 读 `session.jsonl`(claude 交互式必写,`--no-session-persistence` 只配合 `--print`)→ 解析成对话摘要(每轮问什么/答什么/调哪些 tool/产出什么)。
- 收尾复用现有 `issue_run_tail`(lib.rs:4050):提 MR、转 InReview、`scan_and_register_artifacts`。evidence 比 one-shot 硬核(session.jsonl 全对话 vs `CliResult.result` 一段 final text)。
- **后台化**:走现有 `run_issue_backgrounded`(lib.rs:4239,desktop 已 `tokio::spawn`)——R2 满足,不冻 kernel。
- **provider 无关 trait**:`InteractiveCli { binary, prompt_injector, session_trail_parser }`(bw-engine),将来 cursor cli / 别的插进来只配这三个口。

### 2.2 嵌入层((d),在引擎上加 UX,待 orca 研究回流细化)

> **⚠ 归正注记(2026-08-06 review)**:本节是**设计期方案**,落地后有三处被推翻,以 §9/§10 与代码为准 —— ① PTY 库是 `conpty-oxide` 不是 `portable-pty`(§9.2 根因,`3d7b6ca`);② 字节流不走 `Event::TerminalBytes`(该变体 review 时判死码删了,全库零命中),走 kernel 的 `watch` 通道 `pty_bytes()`;③ xterm.js 不用 `<script src>` 加载,`include_str!` 打包进二进制后直接 eval(`0df7897`)。下面四条保留作设计过程记录,别照着施工。

- PTY master 读循环(后台 task)→ `Event::TerminalBytes`(新事件,kernel 事件总线)→ app-desktop xterm.js widget。
- 用户输入 → `Command::TerminalInput`(新命令)→ executor → PTY writer。**(这条成立,是今天的实现)**
- xterm.js 用 `document::eval` 加载(flow.rs:129 已证能跑复杂 JS);无持久双向通道,50-100ms 轮询(同 flow.rs stash 模式)。
- `portable-pty` 非 UI 依赖,过 `guard-kernel-ui-free.sh`(bw-engine 不碰 dioxus)。**(依赖已换 conpty-oxide;portable-pty 只作非 Windows keepalive 留在 Cargo.toml,源码零 import)**
- **(c) 外部终端是 (d) 的真子集**:若 (d) 卡 R1/R2,退路是不嵌终端、开系统终端跑 claude,executor 只 spawn + 等 exit + 读 session.jsonl。

### 2.3 run_phase 契约重设计(预研 verdict #5)
交互式是用户驱动不定时,`run_phase(phase)->PhaseOutput` 阻塞到退出的语义不合。**新 `InteractiveExecutor` trait,一个 skill 一个交互会话,不走 phase 拆分/adversarial loop**。one-shot 路径(`Engine::run_workflow`/`run_phase_range`/`run_adhoc`)零扰动。

### 2.4 orca 研究回流:架构细化(orca-main,Electron+node-pty+xterm;模式可借,代码不同栈)

orca 是 Electron+React+node-pty+xterm.js;buddy 是 Dioxus/wry+Rust。**能借的是模式/协议,不是代码**。五件可借核心:

1. **声明式 CLI 注册表(非 OOP trait)**:orca 用 `TUI_AGENT_CONFIG: Record<TuiAgent, TuiAgentConfig>` 静态表 + `buildAgentStartupPlan` 按 `promptInjectionMode` 分派(argv|flag-prompt|flag-prompt-interactive|flag-interactive|hermes-query|stdin-after-start),32 agent=32 行配置。**buddy:Rust 静态表 `TuiAgentConfig` + `enum PromptInjectionMode` + `build_startup_plan(agent,prompt,skill)->LaunchPlan`**,起步挂 claude + cursor 两条。加新 CLI = 加一行,不是新 impl。
2. **skill 注入走 `--prefill` 不走 PTY paste**:orca 注释明示 paste-after-ready 有 race;claude 用 `--prefill <skill 引导文本>` flag 种入。buddy 复用 `playbook.rs:454 skills_block` 生成 skill 正文,经 `--prefill` 注入,不 paste。
3. **"agent 完成"信号走 claude hook→本地 HTTP→事件**:orca 装 `~/.claude/settings.json` hooks + 极简 curl 脚本 POST 到本地 http server → 转 IPC 事件(PreToolUse/PostToolUse/UserPromptSubmit/Stop)。**buddy:Rust 起一个 127.0.0.1 http server 收 hook → 转 Dioxus event**——这是"agent 完成 → 触发评审门"的实时触发源(比"等进程退出"硬核)。hook 脚本 curl 一发就退,stdin 必须 drain 干净否则 CLI 卡。
4. **evidence 走 session.jsonl collector**:orca 跑完扫 `~/.claude/projects/<slug>/*.jsonl`,readline 流式读,提首 prompt + `message.usage.{input,output,cache_read,cache_creation}_tokens` + 子任务状态。**buddy:加 `ClaudeSessionEvidence` collector 进 `evidence.rs`**(当前只读 git),照 orca `claude-usage/scanner.ts` 抄;不自己重算 token,信 jsonl 的 usage 字段。
5. **PTY trait + 字节流协议**:orca `IPtyProvider`(spawn/write/resize/shutdown/sendSignal/onData/onExit + 可选 pauseProducer/resumeProducer/getBufferSnapshot/getAppliedSize)。buddy Rust trait 一一对照,V1 只 `LocalPtyProvider`(`portable-pty`)。字节流**双向 + 显式 ACK 背压(ackData)+ pre-handler buffer + rendererDispatcherReady 握手 + resize 重断言(getAppliedSize)**——这三条 race 不解决会很难用(窗口 reload 丢字节、fire-and-forget resize 静默丢)。

**buddy (d) 嵌入层落地(wry IPC 吞吐/二进制不如 ws)** —— **⚠ 未按此建(2026-08-06 review 纠偏)**:实际落地里**没有 ws server、没有 JSON-RPC、没有 ACK 背压**(`pty/spawn`|`data`|`write`|`resize`|`ackData`|`exit` 一个都没实现,全库零命中)。真实形态是 kernel 侧 `watch` 通道 + UI 侧 `document::eval` 轮询 drain,§4 Phase2b 里「ACK 背压 skipped for V1」有说明。只有 **127.0.0.1 http server 收 hook** 这一半按设计建了(`bw-app/src/hook_listener.rs`)。下面这段保留作设计过程记录:Rust 侧 `portable-pty` 开 PTY + 127.0.0.1 ws server(JSON-RPC:`pty/spawn` invoke / `pty/data` push / `pty/write`+`pty/resize` send / `pty/ackData` 背压 / `pty/exit`)+ 127.0.0.1 http server 收 hook。WebView 侧 xterm.js + Fit/Serialize/WebLinks addon(`pane-dom-creation.ts` 模式),`onData`→`pty/write`、`onResize`→`pty/resize`、`pty/data`→`terminal.write`;留 `replayIntoTerminal` guard(防重放 scrollback 时 xterm 自动回复 DA1/OSC 污染新 shell),砍掉多 pane/hidden-gate/OSC9999 解析。

**砍掉不借(orca 特有,buddy 单人单机不要)**:远程/SSH PTY+relay、移动 companion、多 worktree 并行、AI Vault 跨 16 CLI 聚合、多 account、CDP 浏览器内嵌、daemon 持久化 PTY 跨重启、团队协作 provider(Linear/Jira/GitLab…)。

### 2.5 收尾决定(2026-08-04,grilling 终态,锁)
- **找指标/绑数据保持两个 Issue**(不合成一个;三件套结构不动)。各跑各的 claude 会话——**不用同一会话/resume**:绑数据会话靠**衔接层 system prompt 灌入"读上游产物文件"**(`.bw/metrics.toml`、`docs/metrics-rationale.md`、`docs/competitive-analysis.md`)接上下文,不新加 resume/continue 设计(用户明示"不要新加特别的设计")。
- **砍掉"对话摘要" collector**:交互式 claude 会话嵌进工作流看板(per-issue),终端 scrollback + `session.jsonl` **本身就是对话记录**,不额外解析摘要成会话消息。buddy 只留**文件级 evidence**(HEAD 对 diff + artifact 登记 + 状态 InReview/PR# + 可选 usage 读回),对话历史靠嵌入终端可重放(`replayIntoTerminal` guard 留)。`ClaudeSessionEvidence` collector 砍,不读 session.jsonl 做摘要(若留预算,只读 usage 字段)。
- **权限**:交互式 `--dangerously-skip-permissions`(流体操作不老问)+ `--disallowedTools "Bash(gh pr merge)"`(守"人 merge"铁律,跳权限≠绕 deny 名单);后台 one-shot 休眠轨留 `acceptEdits`(没人盯要保守)。
- **预算**:交互式 `--max-budget-usd` 只配 `--print` 用不了,丢 per-token 硬 cap;用 **wall-clock 超时 kill + jsonl usage 诚实读回**。对 CLAUDE.md「单次花费封顶」是**显式偏差(已接受)**——交互式 user-in-loop 花费眼看着,不像后台 runaway;orca 同款。
- **CLI 支持**:claude 支持;cursor 占位进声明式表但**先不接**。
- **边界(找指标设计 vs 绑数据实现)**:找指标=推导指标 + 每条**设计采集方案**(manual/script-via-X/要新 connector 对接 Y/cron 节奏建议);绑数据=**实现采集方案**(建 script connector `create_connector`、找/写脚本落 `.bw/scripts/`、配 metric collect、确认 cron;manual 给手填节奏)。采集方案设计在找指标会话里定,搭装置在绑数据会话里做;两会话经文件接上下文(上条)。
- **MR 合入后自动(已自动,非人点同步)**:merge → `MergeIssuePr` 自动 `sync_metrics_file_for`;cron(Daily CollectMetrics)到点自动扫 script connector → observation → signal。**「↻同步」手动按钮曾按此申明退场(`76c7d0e`),但 W3 总览重构又把它加了回来(`a2b914c`),今天仍在 `op.rs` 业务指标区头 —— 现状是「自动为主 + 手动补一刀」,不是「只有自动」(2026-08-06 review 纠偏,决议保留手动按钮,见 LEFTOVERS W3-3)。**

### 2.6 用户回来后的澄清与决定(2026-08-05)

1. **buddy 是薄编排器**:交互式下 buddy 只干两件——唤醒 claude 会话 + 灌入(阶段 system prompt + skill)。会话和用户怎么沟通是 **skill 的活**(skill 方法论驱动交互)。找指标 = 找指标阶段 system prompt + north-star-discovery skill;绑数据同理。衔接层 system prompt **按阶段**(已落地:`build_bridge_system_prompt` 按 `skill_slug` 分支)。
2. **绑数据通用,不为 maas 开后门**:maas 的"采纳率 manual / L1-L3 script"只是举例;绑数据 skill + system prompt **引导用户在 claude cli 里共同开发采集装置**(建 script connector / 脚本 / cron),不是 buddy 为某项目专项。任何项目同一套。
3. **维护指南 3 章范围**(特性向;用户旅程放使用指南 u3/u4):
   - **m4 技能与Prompt** = buddy 自带阶段绑定技能 + 运作/替换机制(衔接层不可换 / skill 可换 / 声明式 CLI 表 / 权限)。
   - **m5 执行与证据** = issue 调度 + claude cli 会话机制(唤醒 / 注入 / **resume / 多轮记忆**)。
   - **m6 指标与健康** = 指标采集链(connector → 构造脚本(可依赖或不依赖 connector) → cron)+ 相关表(`metric`/`observation`/`connector`/`cron_task`)与重要字段(`collect_kind`/`collect_query`/`origin`/`source_kind`)。也可从 skill/agent/定时器/连接器角度讲。
4. **交互式会话 = 持久 + 可 resume(重设计,替 F2)**:交互式 issue ↔ 一个持久 claude 会话 1:1。点 issue 卡在工作流 = **唤醒之前的会话窗口继续聊**,不是"跑完记一行 workflow_run"。**resume 机制坐实(subagent 验)**:`claude --resume <session_id>`(交互,不带 -p,不注新 prompt,会话续)。session_id **从 Claude hook 的 SessionStart 事件 payload 取**——不是从 session.jsonl 文件名(文件名 UUID ≠ session_id);buddy 装一个 hook listener(127.0.0.1 http,像 orca)抓 session_id 存起来,唤醒时 `--resume <id>`。该 hook listener **一物两用**:SessionStart 抓 session_id + Stop/PreToolUse 抓生命周期事件(Phase 2)。备选 `-c`/`--continue`(resume 最近会话,不用存 id,简单 fallback);`--fork-session`(resume 时分支留审计,可选)。**F2"补 workflow_run 行"作废** → 改设计:issue=会话、点卡=resume。交互式 run 不再是"一次性 run 到 InReview",而是持续会话,用户决定何时 finalize 出 PR。
5. **dev 偏差**:#1 `--prefill` **已解决(subagent 验)**——dev 用位置参数 prompt 是**对的**(= orca `promptInjectionMode:'argv'` 主路径,`claude "<prompt>"` auto-submit,文档化稳定);`--prefill` 是 orca 草案路径(预植入输入框、回车前审阅),buddy 不需要,代码里 `draft_prompt_flag:"--prefill"` 字段多余可清。#2 见上(resume 重设计)。#3 预算偏差接受(用户"无所谓";`--max-budget-usd` 确认只配 `--print`)。
6. **m6 待补**:今晚只改了 m4/m5;m6(指标采集链 + 表/字段 + script kind「计划中」→「已接」校准)未动,留 Phase 5。
7. **状态机(用户 2026-08-05 钉)**:InReview 的触发 = **issue 被关联了 PR**(检测到有 PR),不是"跑完"——跑完≠InReview,有 PR 才进评审。PR 合入 → Done(人 merge,铁律)。**Done 后 issue 窗口保持**(点开仍能 `--resume` 唤醒之前的 claude cli 窗口+会话)。**不考虑**"合入后 issue 又新开、继承历史上下文"——新 issue(哪怕同是找指标)靠**读已合入的产物文件**接上下文(薄编排器,文件接),不特殊继承。**一个 issue = 一个 session**(1:1),不做多 session 特殊设计、也不特殊防。
8. **InReview 检测机制(读回为证,2026-08-05 钉 + 2 GAP 核过)**:agent 在会话里提 MR(skill+system prompt 引导,命题:活让 agent 干);buddy **不靠 agent marker**(违反读回为证),而是**自己查 codehub/github** —— `codehub-cli mr list --project <path> --state opened --json iid,source_branch,web_url` 客户端过滤 `source_branch == bw/issue-N`(issue 活分支,字段名建时实测钉)/ github `gh pr list --head bw/issue-N` → 有 open MR → 关联 `pr_number` → InReview。触发:claude `Stop` hook(= agent 一轮答完等用户,频繁 fire,作**防抖查询触发**非"立刻 InReview";codehub 结果权威)+ ~~`SessionEnd`(会话关)fallback~~ **(2026-08-06 review 纠偏:`SessionEnd` 没接。`hook_listener.rs` 的 `BW_HOOK_EVENTS` 只注册 `["SessionStart", "Stop"]`;fallback 实际是 5 分钟一次的 `poll_interactive_inreview` 轮询。要不要补 `SessionEnd` 留后续判断——轮询已经覆盖了「会话关了但最后一次 Stop 没查到 MR」这个场景,只是慢一点。)**。**2 GAP 核过皆 OK**:① Stop 歧义(干完 vs 等用户反馈)不致命——buddy 查 codehub 结果权威,debounce 即可;② codehub-cli 能查 MR(`mr list --source-branch <branch> --state opened`,2a dev 验证 flag 存在、镜像现有 `create_mr` 路径,服务端过滤比客户端过滤好)。系统 prompt 仍可引导 agent "干完提代码 + MR 地址打屏"(给用户看,非给 buddy 检测)。

## 3. collect_kind 收两 kind + 绑数据=搭采集装置

### 3.1 collect_kind 收 `script`|`manual`(forward-correct 文档 + 代码分窗口)
- **本文档/skill/guide 现在按两 kind 写**(forward-correct):`script`(自动,collect_query=字段路径)+ `manual`(人填,戴徽)。`github`/`codehub`/`bw`/`connector` 标注「legacy inline arm,采数/总览窗口迁 script」。
- **代码收枚举**(collapse `CollectKind` 到 `Script`|`Manual` + 把 inline github/codehub/bw arm 都变成 buddy 自带 script connector)归**采数/总览窗口**(Issue1 起了头 codehub→script,收尾在那)。本 issue 不动 inline arm 代码(Q1 决定:绑数据不碰采集器代码,只文档层 forward-correct)。

### 3.2 绑数据 = 搭采集装置(本 issue 核心,正规化 1.3 的 makeshift)
绑数据 skill(交互式)引导用户**实际搭装置**,不只改 metrics.toml collect 字段:
- **能自动采的**:agent 在会话里写采集脚本到 `.bw/scripts/<slug>.py`(buddy 自带 instance 包 codehub/github CLI,或项目侧 `derive_*.py`)+ 写连接器清单 `.bw/connectors.toml`(name/script/command/output 字段结构)+ 给 metric 配 `collect_kind='script'`+`collect_query=字段路径`(在 `.bw/metrics.toml`)。**PR 合入后 buddy 感知**:扩展 `SyncMetricsFile`(或并列 `SyncConnectorsFile`)读 `.bw/connectors.toml` → upsert `connector` 行(kind=script,正规非 SQL 直插)。cron(已有 Daily CollectMetrics)到点自动跑 script connector → 取字段 → `append_observation(SourceKind::Script)` → `recompute_signals` 点亮。**核心是定规范(最简先),agent 不能调 buddy API——靠文件正本 + buddy 感知 sync**(像 skills 分 buddy 自带 + 项目仓自带)。
- **不能自动采的**:`collect_kind='manual'` + 给具体手填节奏(RecordInline op.rs:1866 → `RecordObservation` 戴徽)。
- **登记进 buddy(感知 sync)**:script connector 经 buddy 感知 `.bw/connectors.toml`(merge 后自动 upsert,非 SQL 直插、非 agent 调 API);metric 的 collect 经 `SyncMetricsFile`(merge 后自动)。buddy Hub 能管理(列出/探活/改 config)。**规范先最简**:`.bw/scripts/` 目录约定 + `.bw/connectors.toml` 清单格式 + sync 感知规则;Hub 几大组件(skill/connector/agent/cron)完整规范留遗留单独定(见 §6)。
- **connector.project_id 待核**:NewConnector(bw-store/lib.rs:367)有 project_id,connector 表 schema 未见该列(§6 偏差);P3 sync 前核 + 缺则加(schema 双守卫)。
- **"同步"自动化**:绑数据 issue merge 后 `SyncMetricsFile` 自动(`sync_metrics_file_for`)。~~op.rs「↻同步指标文件」手动按钮退场——它的存在是流程没顺的证物~~ **(2026-08-06 review 纠偏:此申明未最终成立。按钮 `76c7d0e` 删过,`a2b914c` 又加回,今天仍在。决议保留——merge auto-fire 覆盖不了「人手改了 metrics.toml 但还没走 PR」的补采场景。)**

### 3.3 标准目录(script connector 脚本落哪)
- **buddy 自带采集脚本**(包 codehub/github CLI 输出 JSON):落项目仓 `.bw/scripts/`(buddy 管理、随仓 PR)。script connector config 指 `.bw/scripts/<slug>.py` + command + output 字段路径。
- **项目侧既有脚本**(`derive_*.py`):留项目原位(`governance/...`),script connector config 指原路径。
- (具体目录名 SubA 实测钉,本 plan 留口;原则:buddy 自带的进 `.bw/` 受版本控,项目侧的不动。)

## 4. phase 拆分(本 worktree 逐 phase commit,不 push;scope 膨胀→PR 时或拆多 issue)

- **Phase 1 · (c) 引擎 [✅ 已建, commit 11a24b9 + a09e98d]**:`crates/bw-engine/src/interactive_cli.rs`(声明式 CLI 表 claude 支持/cursor 占位 + `build_startup_plan` 位置参数 prompt + `build_bridge_system_prompt` 衔接层 system prompt + `InteractiveCliExecutor` 系统终端 spawn+等退出 + `MockInteractiveExecutor`)+ `bw-app/src/lib.rs` `run_issue_interactive` 分流(零扰 one-shot)+ `SettleOutcome::Interactive`。**无 PTY/xterm/hook、无对话摘要**(§2.6 #2 砍)。验证:门禁+cargo test(6+3 过)+code review(F1/F4 修);真交互式 E2E defer Phase 2。
- **Phase 2a · 引擎补强(resume + InReview 检测 + 状态机)[✅ 已建, commit b4059ee + review fixup]**:resume(`claude --continue` 续最近会话,先不搞 hook/精确 session_id)+ InReview 检测(**轮询** codehub/github 查 issue 分支 `bw/issue-N` 的 open MR,读回为证,挂 `tick_scheduler` 5min 节流)+ 状态机(InProgress→InReview[MR 查到]→Done[人 merge];**砍交互式 `issue_run_tail` 提 MR**,agent 在会话里自提 MR,buddy 检测)+ `interactive_started` 列(schema 双守卫)。**不建 PTY/xterm/hook(2b)**。验证:门禁+cargo test(9+3)+code review 过(F1 defer 2b);真 E2E defer 2b。
- **Phase 2b · 嵌入终端 + hook listener [✅ 已建, commit c2e4099..f8fb8b5,后经 §9/§10 换库与根因修复重做]**:~~`portable-pty`~~ **`conpty-oxide`** PTY + xterm.js widget(`TERM_PRE_HANDLER_JS` 缓冲 + `TERM_INIT_JS` ~~CDN 加载~~ **`include_str!` 打包 eval** + `replayIntoTerminal` guard + 三条 race 解:pre-handler buffer/reload 握手/resize 显式 drain)+ hook listener(`bw-app/src/hook_listener.rs` 127.0.0.1 http + `~/.claude/settings.json` 幂等 hooks,抓 SessionStart→`claude_session_id` + Stop→触发 InReview)+ resume 升级 `--resume <session_id>`(替 `--continue`,**F1 用 session_id fallback 修**)+ InReview hook Stop 触发(替 2a 轮询,5min 兜底)+ `Event::TerminalBytes`/`Command::TerminalInput`/`TerminalResize`(后 review 删 `TerminalBytes` 死码,改 `pty_rx` watch 通道避双消费)+ `App::with_pty()` + block_on panic 修(`bind()` 同步 + `spawn()` 异步)。review fixup(f8fb8b5):PTY 状态会话后清 None + 移除 `poll_pty_bytes` 双消费 + Windows `curl.exe` + 删死码。验证:门禁+cargo test(22)+code review 过;真终端渲染截图 defer 用户(claude+网关)。
- **Phase 3 · 绑数据=搭装置 + collect_kind forward-correct [✅ 已建, commit 7e8fdc4 + 76c7d0e]**:`.bw/connectors.toml` 解析器(`bw-engine/src/connectors_file.rs` 新,对仗 metrics_file.rs,8 单测)+ `docs/connectors-toml-format.md` 规范 + `sync_connectors_file_for`(对仗 sync_metrics_file_for,MergeIssuePr merge 后并列调)+ `connector.project_id`/`config` schema 双守卫补(schema.sql inline 漏两列,补了)+ collect_kind forward-correct(文档层两 kind,枚举代码不动留采数/总览)。review fixup(76c7d0e):SELECT 加 `kind='script'` 过滤(防同名非脚本 connector 被串 kind 静默失败)+ 移除「↻同步」手动按钮(merge 自动 sync 覆盖)。**⚠ 后者已回退**:W3 总览重构(`a2b914c`)把按钮加回 v2 布局,今天仍在;2026-08-06 review 决议保留,本条台账不再算「按钮已退场」(见 §3.2 纠偏 + LEFTOVERS W3-3)。
- **Phase 4 · skill 重写 [✅ 已建, commit 19f2f78]**:north-star + metrics-binding SKILL.md 改**纯方法论**(去掉重复 buddy 契约,加"契约见衔接层 system prompt"指引)+ **衔接层 `build_bridge_system_prompt` 唯一持 buddy 契约**(不可换层,换业界 skill 当 prefill 产出仍对得上契约,§2.6 #1)+ **metrics-binding script query bug 修**(intro/常见坑「query 写脚本路径」→「只写字段路径」,对齐契约 L88)+ bridge prompt 更新绑装置文件规范(.bw/connectors.toml + .bw/scripts/,agent 不调 API)。
- **Phase 5 · guide 校准 [⬜ partial]**:m4/m5 已改(Phase 1 实态);待补 u3/u4 阶段屏(交互式用户旅程)+ m6(指标采集链+表字段+script「计划中」→「已接」)+ 信号色 token 对齐 plan/00 §6。altitude 用系统×CRUD。

## 5. 文件级改动清单 + 契约(SubA 照此建)

### Phase 1((c) 引擎)✅ 已建(commit 11a24b9 + a09e98d)
- `crates/bw-engine/src/interactive_cli.rs`(新,683 行):`TuiAgentConfig` 静态表(CLAUDE supported / CURSOR 占位)+ `PromptInjectionMode::FlagPrefill` + `LaunchPlan` + `build_startup_plan`(位置参数 prompt + `--dangerously-skip-permissions` + `--disallowedTools "Bash(gh pr merge)"`,无 `-p`/无 `--max-budget-usd`)+ `build_bridge_system_prompt`(PlaybookCtx + 按 `skill_slug` 契约 + 铁律)+ `InteractiveExecutor` trait + `InteractiveCliExecutor`(系统终端 spawn + 等退出 + wall-clock)+ `MockInteractiveExecutor`。**无 PTY、无对话摘要 collector**(§2.6 #2 砍)。
- `crates/bw-engine/src/lib.rs`:模块声明 + re-export。`Executor` trait / `ClaudeCliExecutor` / `Engine::run_workflow` **不动**。
- `crates/bw-app/src/lib.rs`:`SettleOutcome` enum(`PhaseLoop`|`Interactive`)+ `run_issue_now` 交互式分流(`is_interactive_skill`)+ `run_issue_interactive` + `finalize_run_interactive`(artifact 登记,不提 MR)+ `fetch_skill_body`。one-shot 路径零扰。

### Phase 2a(引擎补强:resume + InReview 检测 + 状态机)⏳ dev 中
- `crates/bw-engine/src/interactive_cli.rs`:`InteractiveCliExecutor` 加 resume(`claude --continue` 续最近会话;首次走 `build_startup_plan` 位置参数;首次 vs resume 判定靠 `issue.interactive_started`)+ `MockInteractiveExecutor` resume 路径(自标【mock】)。
- `crates/bw-app/src/lib.rs`:InReview 检测挂 `tick_scheduler`(lib.rs:3438)——对交互式 InProgress + `interactive_started` + `pr_number==0` 的 issue 轮询(节流,SubA 定)查 `codehub-cli mr list --state opened --json`/`gh pr list --head bw/issue-N` → 有 open MR → `set_issue_pr_number`(lib.rs:4103)+ `transition_issue(InReview)`。**砍交互式 `issue_run_tail` 提 MR**(agent 在会话里自提,buddy 检测;§2.6 #8)。
- `crates/bw-store/src/schema.sql` + `sqlite.rs` + `bw-core/src/model.rs`:`issue.interactive_started` 列(`add_column_if_missing` 双守卫 + `Issue` struct + SELECT 加列)。

### Phase 2b(嵌入终端 + hook listener)⬜ pending
> **⚠ 归正注记(2026-08-06 review)**:本节写于 Phase 2b 开工前,落地形态见 §4 台账 + §9/§10。三处已推翻:PTY 库换 `conpty-oxide`、`Event::TerminalBytes` 删了改 `watch` 通道、ACK 背压 V1 没做。

- `crates/bw-engine/src/interactive_cli.rs`:~~`portable-pty`~~ **`conpty-oxide`** PTY master 读循环 → ~~`Event::TerminalBytes`~~ **kernel `watch` 通道**;收 `Command::TerminalInput` → PTY writer;resume 升级 `--resume <session_id>`(hook 抓的)替 `--continue`。
- `crates/bw-app/src/lib.rs`:`Event`/`Command` 加 ~~`TerminalBytes`~~/`TerminalInput` 变体(`TerminalBytes` 后被判死码删除);hook listener(127.0.0.1 http,装 `~/.claude/settings.json` hook)抓 SessionStart→`session_id` + Stop→触发 InReview 检测替轮询。
- `crates/app-desktop/src/screens/`:xterm.js widget(`document::eval` 仿 `flow.rs:129`)+ ~~ACK 背压~~(**V1 未做,`document::eval` 同步不需要**)/ reload 握手 / resize 重断言(§2.4 orca)。
- `Cargo.toml`:~~`portable-pty` dep~~ **`conpty-oxide`(Windows)+ `portable-pty`(非 Windows keepalive,源码零 import)**(bw-engine 侧,非 UI)。

### Phase 3(绑装置 + forward-correct)
- `crates/bw-app/src/lib.rs`:绑数据 issue run 内引导建 script connector(正规 `create_connector`)+ 配 metric collect;~~op.rs「↻同步」按钮退场或改 dev-only~~ **(2026-08-06 review 纠偏:退场已回退,按钮保留,见 §3.2)**。
- `crates/bw-engine/src/metrics_file.rs`:`CollectKind` **不动代码**(留采数/总览收),但 doc-comment 标注 legacy 迁移方向。
- `docs/skills/metrics-binding/SKILL.md` + `docs/skills/north-star-discovery/SKILL.md`:两 kind + script query bug 修 + 引导式口径(Phase 4 一起)。

### Phase 4(skill 重写)+ Phase 5(guide)
- 见 §4。guide 改 `docs/guide/buddy-guide.html`(u3/u4/m4/m5/m6)。

## 6. 偏差 / 未决(记不擅定,commit 偏差段如实)

- **R1 预算封顶**:`--max-budget-usd` 只配合 `--print`(claude help 明示),交互式无 CLI flag 级封顶。退路:wall-clock 超时 kill + UI 诚实标注 + jsonl 的 usage 字段事后读回。**orca 先例**:orca 也不自己重算 token,信 flag + jsonl usage + 进程级 deadline(SIGKILL)——即 orca 也是 wall-clock deadline 兜底,无硬 per-token cap。**用户 2026-08-05 已接受 wall-clock 过渡**(交互式 user-in-loop 花费眼看着,不像后台 runaway)。
- **R2 kernel 冻死**:交互式 executor 必须走 `run_issue_backgrounded`(已确认 desktop 走这条,lib.rs:4239 `tokio::spawn`)。
- ~~orca 研究回流待 fold~~ → **已折进 §2.4**(声明式 CLI 表/`IPtyProvider`/PTY ACK 字节流协议/hook→HTTP/session.jsonl collector 模式)。
- **connector.project_id 契约**:NewConnector(bw-store/lib.rs:367)有 project_id,但 connector 表 schema 未见该列(scope 持有?需核)。SubA 建前 `PRAGMA` 读回确认。
- **collect_kind 收枚举代码归采数/总览**:本 issue 只文档 forward-correct,不动 inline arm / CollectKind 枚举(Q1 边界)。
- **F1(2a 已知限制,2b 修,code review Medium)**:`interactive_started` 在 spawn 前置 true(`lib.rs:4743`)。若首次 spawn 失败(claude binary 找不到 / `BW_CLAUDE_BIN` 错 / 终端没开 / claude 崩在会话建立前),会话没建但 flag 已 true → 下次点 ▶跑 走 resume(`--continue` 无 skill 注入)→ 用户卡在无 skill 上下文的会话,且无 UI 重置 flag(得 SQL `UPDATE issue SET interactive_started=0 WHERE id=...`)。2a 仅 spawn 失败(config 错误非正常流)咬人;**2b hook 自然修**(无 session_id 捕获 → fallback `build_startup_plan` 重灌 skill)。小修挪 set 到 spawn 后要动两分支(后台 tokio::spawn + inline)+ settle 错误路径,风险大,defer 2b。临时恢复:SQL 重置 flag。
- **scope 膨胀**:原 Issue 2「skill 易用性」已扩成多 phase 程序。本 worktree 逐 phase commit 不 push;PR 时或拆多 issue(交互式引擎 / 绑装置归位 / skill+guide 各一)。拿不准→问用户。
- **遗留① 多人协作(多 PC)**:多人各自装 buddy、都纳入同 1 项目 → 需协作支持。**至少**:别让三件套 issue 被重复提(同一仓已被 buddy 管过就不重 seed)。buddy 单人构建者命题(反命题:非团队协作),完整协作 = V1+ 特性,本窗口**遗留**,不接。
- **遗留② 制定各规范**:buddy 给各项目带的统一规范(脚本/连接器/skill 等)要**扛得住考验**。`.bw/scripts/` + `.bw/connectors.toml` 的最简规范 P3 先定;Hub 几大组件(skill/connector/agent/cron)完整规范留**遗留单独定**。
- **maas 现态 makeshift 修在 Phase 3**:script connector 从 SQL 直插改正规 create_connector;2 条 codehub 指标 forward-correct 标 legacy(迁 script 留采数/总览)。

## 7. 验证(step 4,读回为证)

- 门禁:`cargo fmt --all --check` + `cargo clippy --workspace --exclude app-desktop -- -D warnings` + `cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features` + `cargo check -p ui --target wasm32-unknown-unknown` + `./scripts/guard-kernel-ui-free.sh` + `cargo check -p app-desktop`。**`cargo test`**(CI 跑,本地门禁漏)。
- 交互式:深链 `BW_OPEN=<项目> BW_PANEL=issues` + 点 ▶跑(交互)→ 系统终端/嵌终端见 `claude` + skill 注入 prompt → 多轮 → 退出 → `sqlite3` 读回 issue 状态 InReview + session.jsonl 解析的会话消息 + artifact 登记 + worktree 文件改动。
- 绑装置:绑数据 issue run(交互)建 script connector → `sqlite3` 读 connector 表(kind=script,config 合法)+ metric 表(collect_kind=script,collect_query=字段路径)+ cron tick 后 observation 有值(SourceKind=Script)+ signal 派生。
- 诚实口径:无数据=Unknown≠绿;Done 永不自动;manual 戴徽;数字 sqlite3 可查。

## 8. 事实源
现状代码:`lib.rs`(run_issue_now:3855/run_issue_backgrounded:4239/prepare_issue_run:3885/standard_skill_block:2435/seed_standard_issue_trio:2963/seed_codehub_public_metrics:2513/collect_project_metrics:3108/github arm:3240/codehub arm:3280/script arm:3316/issue_run_tail:4050/sync_metrics_file_for:7296/MergeIssuePr:7182/ScriptConnectorConfig:7455)、`claude_cli.rs`(231/247-254)、`metrics_file.rs`(CollectKind:40)、`bw-engine/lib.rs`(Executor trait:89)、`op.rs`(IssueDetailOverlay:1006/WorkflowPanel:411/MetricCard:1901/RecordInline:1866/同步按钮:1787)、`schema.sql`(metric/observation/connector/cron_task/project)。
契约:`docs/metrics-toml-format.md`(L88 script query=字段路径)、`docs/skills/north-star-discovery/SKILL.md`、`docs/skills/metrics-binding/SKILL.md`。
预研:~~`docs/v1-prototype/issue2-interactive-cli-spike.md`(A预研 verdict)+ `spike/pty-spike/`(portable-pty spike)~~(**两处均已删除,见 `3d7b6ca` 与 §9.5 的诊断 spike 清理;结论已折进 §9**)+ orca-main 研究(声明式 CLI 表 `TUI_AGENT_CONFIG`、`IPtyProvider` trait、PTY ACK 字节流协议、hook→HTTP→事件、session.jsonl collector——模式可借代码不同栈)。
心智模型物证:commit `fa2e3bb 18-③script-connector` + Issue1 plan(`docs/v1-prototype/issue1-onboard-simplify.md` §0/§6)。
guide 目标态:`docs/guide/buddy-guide.html` u3/u4/m4/m5/m6、`docs/guide/填写规范.md`。

## 9. 根因定位 + 修法(2026-08-06 诊断 spike)

### 9.1 现象(用户穿刺 cowelink 找指标卡)
点「找指标」▶跑后:buddy UI 工作流只显示「进行中」,内嵌终端(claude interactive session)空白,旧 Chat 发送按钮还在,无法交互。DB issue `83052118` in_progress + interactive_started=1 + claude_session_id 空。claude 进程(PID 26852,父 builders-workbench)在但僵死:无网络连接、无子进程、内存恒定、无 session.jsonl。

### 9.2 根因:portable-pty 0.9.0 的 ConPTY 实现读不到程序 stdout
最小颗粒 spike 逐层排除(都在 crates/bw-engine/examples/,临时诊断用,开发阶段清):

| spike | 库 | spawn | reader 读到 |
|---|---|---|---|
| v1 | portable-pty 0.9.0 | claude(直接) | 155B ConPTY 控制序列,无 claude 实际输出 |
| v9 | portable-pty 0.9.0 | powershell(shell) | 4B DSR,无 prompt |
| v11 | portable-pty 0.9.0 | cmd /c echo(直接) | 90B ConPTY 控制序列,无 echo stdout |
| buddy 真实(pty-diag.log) | portable-pty 0.9.0 | claude(直接,issue-2 worktree) | 4B DSR,claude 僵死 |
| v14 | conpty-oxide 0.1.2 | cmd /c echo(直接) | ✅ PTY_OXIDE_OK |
| v15 | conpty-oxide 0.1.2 | claude -p(直接) | ✅ OXIDE_CLAUDE_OK(全链通)|
| 对照 | (无 PTY) | claude -p(直接跑,bash) | ✅ GATEWAY_OK |

排除项:
- ❌ orca 假设错(positional auto-submit):claude 起来 TUI 了,问题更深
- ❌ drop(slave):v10 不 drop 也一样
- ❌ take_writer 破坏 reader:v7 不 take 也 4B
- ❌ 直接 spawn vs shell 中介:v9 portable-pty+shell 也读不到;v15 conpty-oxide 直接 spawn 读到
- ❌ claude/worktree/网关:直接跑 claude -p 在 issue-2 worktree 输出 WORKSPACE_OK
- ✅ **portable-pty 0.9.0 的 ConPTY 实现**:spawn 的程序进程起来了但 stdout 没到 reader(PsuedoCon::new 的 hInput/hOutput 接线对,但实际读不到——0.9.0 实现 bug)

### 9.3 portable-pty 不能升级
crates.io 最新就是 0.9.0(2025-02-11 发布,max_version=0.9.0)。github wezterm/wezterm 的 pty 目录最近 commit 是 "pty: windows: fix kill()"(修 kill,不是修 ConPTY stdout)。所以 git 依赖也没修复。

### 9.4 修法:buddy 换 conpty-oxide
- 换 `portable-pty = "0.9"` → `conpty-oxide = "0.1"`(crates.io,0.1.2,2026-08-04 发布,correctness-first,sync+async API)
- `run_skill_pty` 改用 `conpty_oxide::blocking::Command::new(claude).args(...).spawn()?.into_parts()` 拿 output reader(流式读 claude 输出 → bytes_tx)+ input writer(接 PtyInput::Bytes 用户输入)+ child(kill/wait)
- v15 已验证 conpty-oxide spawn claude + 读 stdout + 网关全链通

### 9.5 叠加问题:claude 交互式 positional 不 auto-submit
claude 交互式(不带 -p)的 positional argv 不会自动提交为首条消息(claude TUI 起来后等用户回车)。buddy 的 build_startup_plan 用 positional 注入 skill body,假设 auto-submit——这个假设跟 orca 共用(orca 也没 e2e 验证过,但 orca 用户跑 stock claude 碰巧成立,buddy 环境 GLM 网关的 claude 不成立)。
修法:开发时调注入方式——spawn 裸 claude(无 positional)+ 等 TUI ready + stdin paste skill body + 回车提交(或用户在嵌入终端手动输入,符合"一个回车就能先用起来"的最低期望)。具体方案开发阶段定。

### 9.6 W2 决策背景:直接 spawn claude(不走 shell 中介)
W2 选直接 spawn claude(CommandBuilder::new(claude).args(...),无 shell)是合理简化——不需 orca 那套 shell-ready 握手(OSC 777 marker)、shell 是 PTY child 再 exec claude。orca 走 shell 是因为它的架构(多 agent、shell 通用、需要 shell-ready marker + writeStartupCommandWhenShellReady)。**这个结构差异不是根因**(v15 conpty-oxide 直接 spawn claude 读到 stdout),只是 portable-pty 0.9.0 的 bug 把它暴露了。

### 9.7 清理(开发阶段做)
- 删诊断 spike:crates/bw-engine/examples/{pty_spike,conpty_direct,conpty_test,conpty_oxide_test,conpty_oxide_claude}.rs
- 删 Cargo.toml 的 [target.'cfg(windows)'.dev-dependencies] conpty/conpty-oxide/winapi
- 删 interactive_cli.rs 的 [pty-diag] 诊断日志(read loop 的文件日志 + spawn 后的日志)
- 删 pty-diag.log

## 10. stdin 不通的真根因 + 修法(2026-08-06,续 §9 之后一棒)

§9 修好了 stdout(conpty-oxide 换库)。用户重启验证后:输出能看到了,但**打字没反应**——键盘敲了终端里什么都不出现。之前四次修法尝试(xterm 打包本地、div keydown workaround、term.focus()、日志)都没解决,因为都在 xterm.js 初始化/键盘事件那一层找,没人怀疑 Rust↔JS 桥接本身。

### 10.1 现象与误导性证据
`pty-stdin-diag.log` 一直显示 `drain ready=false term=false input_len=0`——看起来像 xterm.js 压根没初始化(CDN 失败假说)。但用户手动验证时 claude 的 TUI **确实渲染出来了**(说明 xterm 其实初始化好了、stdout 也写进去了),跟日志矛盾。日志在说谎,得先信实况不信日志。

### 10.2 真根因:`document::eval()` 是 `AsyncFunction` 体,不写 `return` 就永远拿 `undefined`
翻 Dioxus 0.7.9 源码(`dioxus-desktop-0.7.9/src/query.rs`):`document::eval(script)` 把传入的字符串整段包成 `new AsyncFunction("dioxus", script)(dioxus)` 去执行——**是函数体,不是表达式**。之前 `TERM_INIT_JS`/drain 脚本的最后一行是裸的 `(async function() {...})()`(没有 `return`),JS 侧确实跑了(xterm 真初始化了、`window.__bw_term_ready` 真置为 true),但这个 IIFE 的返回值从没被 `return` 出这层 `AsyncFunction`——Rust 侧 `document::eval(...).await` 拿到的永远是 `undefined`。于是:
- 诊断日志读的就是这个 `undefined`(反序列化成 `ready=false`/`term=false`)——**日志假,不是终端假**,这也是 §9.1 现象里"log 说空但 TUI 真渲染"的另一半解释。
- 真正致命的是 `TerminalWidget` 的 50ms 轮询:`window.__bw_term_drain_input()`/`__bw_term_drain_resize()` 的返回值同样从没跨过 Rust 边界——`Command::TerminalInput` 因此**从未被派发过一次**,不管用户在终端里按了什么键。stdout 通、stdin 死,根因是同一层桥接缺 `return`,跟 xterm.js 初始化、CDN、焦点、Dioxus 截键全无关——之前四次修法都在错误的层排查。

### 10.3 修法
1. **每个 `document::eval` 调用前补 `return`**:`TERM_INIT_JS` 顶部加 `return (async function(){...})()`;drain 脚本合并 `__bw_term_drain_input`/`__bw_term_drain_resize` 两个函数为单次 `window.__bw_term_drain()`,一次 eval 拿 `{ input, resize, ready }` 一整个对象(减少每帧 eval 次数,也让 `ready`/`input_len` 日志终于反映真实状态)。
2. **onData 与 keydown 去重**:xterm 的 hidden textarea 聚焦时,`term.onData` 已经把这次按键编码成字节了;容器 `div` 上的 `keydown` 是焦点丢失时的兜底,两者都触发会把每个字符送两遍。修法:`keydown` 处理器判断 `e.target === textarea` 时直接跳过(textarea 有焦点说明 `onData` 会接管)。
3. **按键映射补全**:原来只处理单字符 + Enter/Backspace/Escape/Tab,方向键(`e.key.length > 1`)被 `return` 忽略——claude TUI 靠上下箭头翻历史,箭头送不进去等于交互式白搭。补齐 `KEYS` 映射表(方向键/Home/End/PageUp/PageDown/Delete/Insert 的 CSI 序列)+ Ctrl+字母→`0x01`-`0x1a`(Ctrl+C=`0x03` 中断 TUI)。
4. **诊断日志改门控**:`pty-stdin-diag.log`(硬编码 `D:\...` 绝对路径文件写)和 lib.rs/interactive_cli.rs 里的 `[stdin-*]` 全删,改成 `BW_PTY_DEBUG=1` 门控的 `eprintln!`(默认关,轮询 ~20Hz 不刷屏)。

### 10.4 验证(用户手动确认,2026-08-06)
不能自验(无 Windows GUI/computer-use),请用户重启 buddy 后手动打字确认。结果:**打字有效、Esc 能取消、上箭头能看到输入历史**——等同 PowerShell 里直接开 `claude` CLI 的体验。逻辑正确性额外用 Node.js 起了两个抛弃式 harness(stub DOM,跑真实 `TERM_PRE_HANDLER_JS`/`TERM_INIT_JS` 源码字符串):一个覆盖 drain 契约/按键映射/去重(16 项全过),一个覆盖 §10.6 的组件重挂载(remount)场景(10 项全过)。

### 10.5 顺带修的第二个 bug:Issue 板重复点「▶ 跑」→ 每次新开一张「阶段记录」卡
用户反馈:同一个 issue(如「找指标」)每点一次「▶ 跑」,工作流「阶段记录」轨就多一张同名卡片,删不掉,内容看起来还是同一个会话。

根因:Issue 卡/详情页的「▶ 跑」onclick 一直是 `let sid = SessionId::new()` 后 `StartSession` + `RunIssue`——**每次点击都铸一个全新 `SessionId`**,而 `run_issue_interactive` 侧真正决定"新开 vs resume"是靠 issue 行上的 `claude_session_id`(有值就 `--resume`),跟 UI 传进去的 `SessionId` 完全无关。`session` 表也没有 `issue_id` 外键,`StartSession`(`ensure_session`,`ON CONFLICT(id) DO NOTHING`)对每个新 id 都真插一行——UI 侧的"阶段记录"堆积纯粹是重复插入的空壳,跟真正的 claude 会话是不是 resume 无关(所以用户看到"不同卡片回显同一个会话"——因为交互式 issue 从不写 `session.message` 表,这些卡片本来就都是空的,只是标题一样看起来像同一个)。

修法(不动 schema):`run_sess_title = "#{number} {title}"` 本来就是这个 issue 独有的确定性标题,拿它当去重 key——点「▶ 跑」前先在 `op.sessions`(项目全量会话列表)里找 `(stage_kind, title)` 都匹配的既有会话,找到就复用它的 id(`StartSession` 传旧 id 是幂等 no-op),找不到才 `SessionId::new()`。两处「▶ 跑」(Issue 板列表 op.rs、IssueDetailOverlay)共用同一个 `existing_issue_session()` 辅助函数。效果:同一个 issue 反复点「▶ 跑」只会有一张阶段记录卡,点击就是原来那张(不再新开视图卡片)。

遗留:已经堆积出来的历史重复卡片本次不做批量清理(没有 `DeleteSession` 命令,加会涉及 store 新方法 + UI 按钮,本次不擅自扩);用户下一步要重新创建 cowelink 项目重跑验证,删项目会带走它名下所有会话,不需要额外清理动作。

### 10.6 顺带修的第三个 bug:切到别的面板再切回工作流,终端显示空;重启 buddy 后终端整块消失
用户反馈两种情况:①(同一次运行中)点开其他面板再切回工作流,claude CLI 那块黑窗口是空的;②这时候重启 buddy,再进工作流,黑窗口整个不见了(不是"空",是"没了")。

**① 同进程内导航后变空 —— 真 bug,已修**。`Center`(op.rs)是按 `(op.panel, stage)` 做 `match` 的,切到 Issues/进度等任何其他面板都会让 `WorkflowPanel`/`WorkflowStage`/`TerminalWidget` 整棵子树从 Dioxus 树上摘掉;切回工作流是重新构造一遍这些组件,`TerminalWidget` 的 `div#__bw_terminal` 因此是一个**全新的 DOM 节点**。旧的 `TERM_INIT_JS` 顶部是 `if (window.__bw_term) return`——`window.__bw_term`(xterm 实例 + 它背后的 PTY 会话)确实还活着,但这一行直接短路返回,从不把它挂到新 div 上;xterm 自己的滚屏缓冲区其实什么都没丢,只是渲染出的 DOM 还挂在旧的、已经被摘掉的 div 下面,画面上自然什么都看不到。

修法:把"存在即返回"的旧 guard 换成"存在就搬家"——检查当前 div 是否已经包含 `window.__bw_term.element`(xterm 公开的渲染根节点),不包含就 `div.appendChild(window.__bw_term.element)` 搬过去 + 重新 `fit()`。同时把 `click`/`keydown` 这两个绑在旧 div 上的监听器重新绑到新 div 上(DOM 节点级监听器不会跟着搬家);但 `term.onData` 是绑在 `term` 对象本身(不是绑在 div 上)的,重挂载时**不能**再调一次,否则往后每敲一个字符都会被送两遍——这也是为什么代码把"绑一次的 onData"和"每次挂载都要重绑的 div 监听器"拆成了两段。用 Node harness 模拟了"挂载→卸载(换 div)→重挂载"全过程,确认:xterm 内容正确搬到新 div、`onData` 只注册一次、新 div 上敲键正确产出一个字节(§10.4 的第二个 harness)。

**② buddy 重启后终端整块消失 —— 如实,不是本次要修的 bug**。`pty_active`(决定要不要渲染 `TerminalWidget`)来自 `state.pty_input_tx.is_some()`,这是纯内存状态,buddy 进程重启后天然是 `None`——按 CLAUDE.md「无数据=Unknown,绝不假装绿」的铁律,这里"黑窗口整个不渲染"是**诚实**表现,不是要补的洞。更深一层:`interactive_cli.rs` 里 conpty-oxide 的 session 挂在一个 kill-on-close 的 Windows Job 上,buddy 进程退出时这个 Job 会把子进程 claude.exe 一起杀掉——就算硬做"重启后把旧 PTY 接回来"的 UI,底层那个 PTY 连接本身也已经死了,接不回去。真正的恢复路径是 issue 行上持久化的 `claude_session_id`:重启后再点一次「▶ 跑」,`run_issue_interactive` 会带 `--resume <claude_session_id>` 重新 spawn,claude 自己的会话持久化(不依赖 buddy 进程)让对话历史接得上,只是需要重新走一次「▶ 跑」而不是"自动复活黑窗口"。这个体验(重启后黑窗口悄悄消失,没有任何提示)留作 LEFTOVERS 里的后续打磨项,不在本棒动。

### 10.7 已知但本次不修的残留缝隙(如实记录,别当成修好了)
- **导航离开期间的字节可能丢一部分**:PTY 输出经 `watch::channel<Vec<u8>>` 从内核线程送到 UI(`pty_ticker` 每 100ms 取一批新字节塞进去)。`watch` 只保留"最新一批",不是队列——如果 `TerminalWidget` 因为用户切到别的面板而卸载(停止 `.changed().await`),这段时间里内核线程仍在按 100ms 一批地覆盖发送,只有*最后一批*会留到用户切回来时被捞到,中间那些批次会被静默覆盖丢掉。§10.6 修的是"切回来后能不能看见"(能),没修"切走期间产出的字节是否一批不丢"(不能完全保证,拿之前的行为对比不算新introduce的回归,是 Phase2b 设计里本来就有的性质)。真正堵死这个洞需要把单槽 `watch` 换成有界队列或服务端整段 scrollback 缓冲,工作量超出本次 bugfix,记在这里留给下一棒评估要不要做。
