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
- PTY master 读循环(后台 task)→ `Event::TerminalBytes`(新事件,kernel 事件总线)→ app-desktop xterm.js widget。
- 用户输入 → `Command::TerminalInput`(新命令)→ executor → PTY writer。
- xterm.js 用 `document::eval` 加载(flow.rs:129 已证能跑复杂 JS);无持久双向通道,50-100ms 轮询(同 flow.rs stash 模式)。
- `portable-pty` 非 UI 依赖,过 `guard-kernel-ui-free.sh`(bw-engine 不碰 dioxus)。
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

**buddy (d) 嵌入层落地(wry IPC 吞吐/二进制不如 ws)**:Rust 侧 `portable-pty` 开 PTY + 127.0.0.1 ws server(JSON-RPC:`pty/spawn` invoke / `pty/data` push / `pty/write`+`pty/resize` send / `pty/ackData` 背压 / `pty/exit`)+ 127.0.0.1 http server 收 hook。WebView 侧 xterm.js + Fit/Serialize/WebLinks addon(`pane-dom-creation.ts` 模式),`onData`→`pty/write`、`onResize`→`pty/resize`、`pty/data`→`terminal.write`;留 `replayIntoTerminal` guard(防重放 scrollback 时 xterm 自动回复 DA1/OSC 污染新 shell),砍掉多 pane/hidden-gate/OSC9999 解析。

**砍掉不借(orca 特有,buddy 单人单机不要)**:远程/SSH PTY+relay、移动 companion、多 worktree 并行、AI Vault 跨 16 CLI 聚合、多 account、CDP 浏览器内嵌、daemon 持久化 PTY 跨重启、团队协作 provider(Linear/Jira/GitLab…)。

### 2.5 收尾决定(2026-08-04,grilling 终态,锁)
- **找指标/绑数据保持两个 Issue**(不合成一个;三件套结构不动)。各跑各的 claude 会话——**不用同一会话/resume**:绑数据会话靠**衔接层 system prompt 灌入"读上游产物文件"**(`.bw/metrics.toml`、`docs/metrics-rationale.md`、`docs/competitive-analysis.md`)接上下文,不新加 resume/continue 设计(用户明示"不要新加特别的设计")。
- **砍掉"对话摘要" collector**:交互式 claude 会话嵌进工作流看板(per-issue),终端 scrollback + `session.jsonl` **本身就是对话记录**,不额外解析摘要成会话消息。buddy 只留**文件级 evidence**(HEAD 对 diff + artifact 登记 + 状态 InReview/PR# + 可选 usage 读回),对话历史靠嵌入终端可重放(`replayIntoTerminal` guard 留)。`ClaudeSessionEvidence` collector 砍,不读 session.jsonl 做摘要(若留预算,只读 usage 字段)。
- **权限**:交互式 `--dangerously-skip-permissions`(流体操作不老问)+ `--disallowedTools "Bash(gh pr merge)"`(守"人 merge"铁律,跳权限≠绕 deny 名单);后台 one-shot 休眠轨留 `acceptEdits`(没人盯要保守)。
- **预算**:交互式 `--max-budget-usd` 只配 `--print` 用不了,丢 per-token 硬 cap;用 **wall-clock 超时 kill + jsonl usage 诚实读回**。对 CLAUDE.md「单次花费封顶」是**显式偏差(已接受)**——交互式 user-in-loop 花费眼看着,不像后台 runaway;orca 同款。
- **CLI 支持**:claude 支持;cursor 占位进声明式表但**先不接**。
- **边界(找指标设计 vs 绑数据实现)**:找指标=推导指标 + 每条**设计采集方案**(manual/script-via-X/要新 connector 对接 Y/cron 节奏建议);绑数据=**实现采集方案**(建 script connector `create_connector`、找/写脚本落 `.bw/scripts/`、配 metric collect、确认 cron;manual 给手填节奏)。采集方案设计在找指标会话里定,搭装置在绑数据会话里做;两会话经文件接上下文(上条)。
- **MR 合入后自动(已自动,非人点同步)**:merge → `MergeIssuePr` 自动 `sync_metrics_file_for`;cron(Daily CollectMetrics)到点自动扫 script connector → observation → signal。**「↻同步」手动按钮退场**(plan18-⑦ 补丁,流程顺了不需要)。

### 2.6 用户回来后的澄清与决定(2026-08-05)

1. **buddy 是薄编排器**:交互式下 buddy 只干两件——唤醒 claude 会话 + 灌入(阶段 system prompt + skill)。会话和用户怎么沟通是 **skill 的活**(skill 方法论驱动交互)。找指标 = 找指标阶段 system prompt + north-star-discovery skill;绑数据同理。衔接层 system prompt **按阶段**(已落地:`build_bridge_system_prompt` 按 `skill_slug` 分支)。
2. **绑数据通用,不为 maas 开后门**:maas 的"采纳率 manual / L1-L3 script"只是举例;绑数据 skill + system prompt **引导用户在 claude cli 里共同开发采集装置**(建 script connector / 脚本 / cron),不是 buddy 为某项目专项。任何项目同一套。
3. **维护指南 3 章范围**(特性向;用户旅程放使用指南 u3/u4):
   - **m4 技能与Prompt** = buddy 自带阶段绑定技能 + 运作/替换机制(衔接层不可换 / skill 可换 / 声明式 CLI 表 / 权限)。
   - **m5 执行与证据** = issue 调度 + claude cli 会话机制(唤醒 / 注入 / **resume / 多轮记忆**)。
   - **m6 指标与健康** = 指标采集链(connector → 构造脚本(可依赖或不依赖 connector) → cron)+ 相关表(`metric`/`observation`/`connector`/`cron_task`)与重要字段(`collect_kind`/`collect_query`/`origin`/`source_kind`)。也可从 skill/agent/定时器/连接器角度讲。
4. **交互式会话 = 持久 + 可 resume(重设计,替 F2)**:交互式 issue ↔ 一个持久 claude 会话 1:1。点 issue 卡在工作流 = **唤醒之前的会话窗口继续聊**(`claude --dangerously-skip-permissions --resume <session-id>`),不是"跑完记一行 workflow_run"。claude 自己写 `session.jsonl`(含 session-id),buddy 存 session-id,唤醒时 `--resume`(orca `buildAgentResumeStartupPlan` 同款)。**F2"补 workflow_run 行"作废** → 改设计:issue=会话、点卡=resume。交互式 run 不再是"一次性 run 到 InReview",而是持续会话,用户决定何时 finalize 出 PR。
5. **dev 偏差**:#1 `--prefill` 注入机制待 orca 研究 + `claude --help` 核(subagent 验);#2 见上(改 resume 重设计);#3 预算偏差接受(用户"无所谓")。
6. **m6 待补**:今晚只改了 m4/m5;m6(指标采集链 + 表/字段 + script kind「计划中」→「已接」校准)未动,留 Phase 5。

## 3. collect_kind 收两 kind + 绑数据=搭采集装置

### 3.1 collect_kind 收 `script`|`manual`(forward-correct 文档 + 代码分窗口)
- **本文档/skill/guide 现在按两 kind 写**(forward-correct):`script`(自动,collect_query=字段路径)+ `manual`(人填,戴徽)。`github`/`codehub`/`bw`/`connector` 标注「legacy inline arm,采数/总览窗口迁 script」。
- **代码收枚举**(collapse `CollectKind` 到 `Script`|`Manual` + 把 inline github/codehub/bw arm 都变成 buddy 自带 script connector)归**采数/总览窗口**(Issue1 起了头 codehub→script,收尾在那)。本 issue 不动 inline arm 代码(Q1 决定:绑数据不碰采集器代码,只文档层 forward-correct)。

### 3.2 绑数据 = 搭采集装置(本 issue 核心,正规化 1.3 的 makeshift)
绑数据 skill(交互式)引导用户**实际搭装置**,不只改 metrics.toml collect 字段:
- **能自动采的**:建 script connector(`store.create_connector(NewConnector{kind:CONNECTOR_KIND_SCRIPT, project_id, config: ScriptConnectorConfig{script, output, command}})`)——buddy 自带 instance(包 codehub/github CLI 输出 JSON,落**标准目录** 见 3.3)或项目侧(`derive_*.py`)。给 metric 配 `collect_kind='script'` + `collect_query=字段路径`。cron(已有 Daily CollectMetrics)到点自动跑 script connector → 取字段 → `append_observation(SourceKind::Script)` → `recompute_signals` 点亮。
- **不能自动采的**:`collect_kind='manual'` + 给具体手填节奏(RecordInline op.rs:1866 → `RecordObservation` 戴徽)。
- **登记进 buddy**:script connector 经正规 `create_connector` 进 connector 表(非 SQL 直插),metric 的 collect 经 `SyncMetricsFile`(merge 后自动)或绑数据 issue run 内更新。buddy Hub 能管理(script_hub/op 列出、探活、改 config)。
- **"同步"自动化、手动按钮退场**:绑数据 issue merge 后 `SyncMetricsFile` 自动(lib.rs:7296),op.rs「↻同步指标文件」手动按钮(plan18-⑦ 补丁)退场——它的存在是流程没顺的证物,流程顺了不需要。

### 3.3 标准目录(script connector 脚本落哪)
- **buddy 自带采集脚本**(包 codehub/github CLI 输出 JSON):落项目仓 `.bw/scripts/`(buddy 管理、随仓 PR)。script connector config 指 `.bw/scripts/<slug>.py` + command + output 字段路径。
- **项目侧既有脚本**(`derive_*.py`):留项目原位(`governance/...`),script connector config 指原路径。
- (具体目录名 SubA 实测钉,本 plan 留口;原则:buddy 自带的进 `.bw/` 受版本控,项目侧的不动。)

## 4. phase 拆分(本 worktree 逐 phase commit,不 push;scope 膨胀→PR 时或拆多 issue)

- **Phase 1 · (c) 引擎**(验证交互闭环):`InteractiveCliExecutor`(PTY spawn claude + session.jsonl 解析)+ `InteractiveExecutor` trait + provider 抽象 + 走 `run_issue_backgrounded`。先用 (c) 外部终端验通"唤起 + 多轮 + 回流 evidence"。(d) 嵌入层此时不开。
- **Phase 2 · (d) 嵌入层**:PTY 字节流 → `Event::TerminalBytes`/`Command::TerminalInput` + xterm.js widget。借 orca 研究回流细化。
- **Phase 3 · 绑数据=搭装置 + collect_kind forward-correct**:绑数据 skill 引导建 script connector(正规 `create_connector`)+ 配 metric collect + 标准目录;skill/guide 按 script|manual 两 kind forward-correct;「↻同步」按钮退场。(inline arm 代码不动,留采数/总览。)
- **Phase 4 · skill 重写**:north-star-discovery + metrics-binding 改成引导式多轮(interactive)口径 + 两 kind + 读项目既有 governance 优先对齐(保留)+ script query 矛盾 bug 修(metrics-binding intro/常见坑「query 写脚本路径」→ 改「query 只写字段路径」,对齐契约 metrics-toml-format.md L88)。
- **Phase 5 · guide 校准**:u3/u4/m4/m5/m6 按 V1 实态(交互式 + 两 kind + 绑装置)校准;m6 `script`「计划中」→「已接」;u3/u4 foot 陈旧外链替换;信号色 token 对齐 plan/00 §6(或单列)。altitude 纪律:描述动作身后事务用「系统×CRUD」不用 `Command::`(已存记忆)。

## 5. 文件级改动清单 + 契约(SubA 照此建)

### Phase 1((c) 引擎)
- `crates/bw-engine/src/interactive_cli.rs`(新):`InteractiveCliExecutor` + `InteractiveCli` provider trait + `SessionTrail` 解析器(读 `~/.claude/projects/<proj>/session.jsonl` → 对话摘要)。PTY 用 `portable-pty`。
- `crates/bw-engine/src/lib.rs`:导出 `InteractiveCliExecutor`、新 trait;`Executor` trait / `ClaudeCliExecutor` / `Engine::run_workflow` **不动**。
- `crates/bw-app/src/lib.rs`:`run_issue_now`(3855)加交互式分支(按 issue 的 standard_skill 或新 `interactive` 标志分流);`issue_run_tail`(4050)复用,加 session.jsonl 摘要喂会话消息。
- `crates/bw-core/src/model.rs`:`IssueRun`/会话消息结构加"对话摘要"字段(可选,forward-correct)。

### Phase 2((d) 嵌入层)
- `crates/bw-engine/src/interactive_cli.rs`:PTY master 读循环 → 发 `Event::TerminalBytes`;收 `Command::TerminalInput` → PTY writer。
- `crates/bw-app/src/lib.rs`:`Event`/`Command` enum 加 `TerminalBytes`/`TerminalInput` 变体。
- `crates/app-desktop/src/screens/`(新 widget 或 op.rs 内):xterm.js 加载 + 轮询桥(`document::eval`,仿 flow.rs:129)+ 输入回写。
- `Cargo.toml`(app-desktop):`portable-pty` dep(bw-engine 侧,非 UI)。

### Phase 3(绑装置 + forward-correct)
- `crates/bw-app/src/lib.rs`:绑数据 issue run 内引导建 script connector(正规 `create_connector`)+ 配 metric collect;op.rs「↻同步」按钮(L1787)退场或改 dev-only。
- `crates/bw-engine/src/metrics_file.rs`:`CollectKind` **不动代码**(留采数/总览收),但 doc-comment 标注 legacy 迁移方向。
- `docs/skills/metrics-binding/SKILL.md` + `docs/skills/north-star-discovery/SKILL.md`:两 kind + script query bug 修 + 引导式口径(Phase 4 一起)。

### Phase 4(skill 重写)+ Phase 5(guide)
- 见 §4。guide 改 `docs/guide/buddy-guide.html`(u3/u4/m4/m5/m6)。

## 6. 偏差 / 未决(记不擅定,commit 偏差段如实)

- **R1 预算封顶**:`--max-budget-usd` 只配合 `--print`(claude help 明示),交互式无 CLI flag 级封顶。退路:wall-clock 超时 kill + UI 诚实标注 + jsonl 的 usage 字段事后读回。**orca 先例**:orca 也不自己重算 token,信 flag + jsonl usage + 进程级 deadline(SIGKILL)——即 orca 也是 wall-clock deadline 兜底,无硬 per-token cap。**所以 wall-clock + 诚实是经 orca 验过的模式**(交互式本质 user-in-loop,花费眼看着,不像后台 runaway)。**预算铁律是否硬挡交互式,仍要用户定**——但 orca 先例让"wall-clock 过渡"站得住。
- **R2 kernel 冻死**:交互式 executor 必须走 `run_issue_backgrounded`(已确认 desktop 走这条,lib.rs:4239 `tokio::spawn`)。
- **orca 研究回流待 fold**:(d) 嵌入层的 PTY/xterm/IPC 桥细节等 orca-main 研究 SubAgent 回来后细化(进程已派)。
- **connector.project_id 契约**:NewConnector(bw-store/lib.rs:367)有 project_id,但 connector 表 schema 未见该列(scope 持有?需核)。SubA 建前 `PRAGMA` 读回确认。
- **collect_kind 收枚举代码归采数/总览**:本 issue 只文档 forward-correct,不动 inline arm / CollectKind 枚举(Q1 边界)。
- **scope 膨胀**:原 Issue 2「skill 易用性」已扩成多 phase 程序。本 worktree 逐 phase commit 不 push;PR 时或拆多 issue(交互式引擎 / 绑装置归位 / skill+guide 各一)。拿不准→问用户。
- **多人参与同项目**:buddy 单人构建者命题(反命题:非团队协作)。第二人接入已有 buddy 项目 = V1+ 特性,本窗口**遗留**,不接。
- **maas 现态 makeshift 修在 Phase 3**:script connector 从 SQL 直插改正规 create_connector;2 条 codehub 指标 forward-correct 标 legacy(迁 script 留采数/总览)。

## 7. 验证(step 4,读回为证)

- 门禁:`cargo fmt --all --check` + `cargo clippy --workspace --exclude app-desktop -- -D warnings` + `cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features` + `cargo check -p ui --target wasm32-unknown-unknown` + `./scripts/guard-kernel-ui-free.sh` + `cargo check -p app-desktop`。**`cargo test`**(CI 跑,本地门禁漏)。
- 交互式:深链 `BW_OPEN=<项目> BW_PANEL=issues` + 点 ▶跑(交互)→ 系统终端/嵌终端见 `claude` + skill 注入 prompt → 多轮 → 退出 → `sqlite3` 读回 issue 状态 InReview + session.jsonl 解析的会话消息 + artifact 登记 + worktree 文件改动。
- 绑装置:绑数据 issue run(交互)建 script connector → `sqlite3` 读 connector 表(kind=script,config 合法)+ metric 表(collect_kind=script,collect_query=字段路径)+ cron tick 后 observation 有值(SourceKind=Script)+ signal 派生。
- 诚实口径:无数据=Unknown≠绿;Done 永不自动;manual 戴徽;数字 sqlite3 可查。

## 8. 事实源
现状代码:`lib.rs`(run_issue_now:3855/run_issue_backgrounded:4239/prepare_issue_run:3885/standard_skill_block:2435/seed_standard_issue_trio:2963/seed_codehub_public_metrics:2513/collect_project_metrics:3108/github arm:3240/codehub arm:3280/script arm:3316/issue_run_tail:4050/sync_metrics_file_for:7296/MergeIssuePr:7182/ScriptConnectorConfig:7455)、`claude_cli.rs`(231/247-254)、`metrics_file.rs`(CollectKind:40)、`bw-engine/lib.rs`(Executor trait:89)、`op.rs`(IssueDetailOverlay:1006/WorkflowPanel:411/MetricCard:1901/RecordInline:1866/同步按钮:1787)、`schema.sql`(metric/observation/connector/cron_task/project)。
契约:`docs/metrics-toml-format.md`(L88 script query=字段路径)、`docs/skills/north-star-discovery/SKILL.md`、`docs/skills/metrics-binding/SKILL.md`。
预研:`docs/v1-prototype/issue2-interactive-cli-spike.md`(A预研 verdict)+ `spike/pty-spike/`(portable-pty spike)+ orca-main 研究(声明式 CLI 表 `TUI_AGENT_CONFIG`、`IPtyProvider` trait、PTY ACK 字节流协议、hook→HTTP→事件、session.jsonl collector——模式可借代码不同栈)。
心智模型物证:commit `fa2e3bb 18-③script-connector` + Issue1 plan(`docs/v1-prototype/issue1-onboard-simplify.md` §0/§6)。
guide 目标态:`docs/guide/buddy-guide.html` u3/u4/m4/m5/m6、`docs/guide/填写规范.md`。
