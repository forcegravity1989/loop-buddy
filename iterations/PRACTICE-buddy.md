# 践行日志 · buddy(操作指南 + 实践记录 + 认知演进)

## 30 秒导读

这是我实际使用、维护 buddy(loop-buddy 项目管理工作台)的真实记录。**它是唯一一份持续更新的实践日志,不是写完就封存的历史档案**——每次会话都可能往上加新内容,旧结论被推翻时保留原文并补一句「归正」说明,不悄悄改写。

给谁看:照着操作步骤把 buddy 接到自己项目上的同事,或者未来想知道某个坑当时怎么解的我自己。

怎么用:全文较长,不必从头读到尾——按下面的目录跳到你需要的部分:装环境看第 1 节,完整流程走一遍看第 2 节,某个具体功能怎么用看第 3 节,已知的坑和还没拍板的事看第 4 节,想理解 buddy 整体设计思路看第 5 节。下面这段引用是更细的写法约定,先看目录也可以。

> 一份**活文档**,每个会话更新。不是开发文档(那是 `plan/` + 源码 + commit),是
> **我用 + 维护 buddy 的真实实践**,兼两个用途:① 给同事/未来开发者照着用 loop-buddy
> (前置、主流程、分支操作,每步在磁盘/UI/数据库/人能读懂的背后动作上做了什么);
> ② 记我遇到什么问题、定了什么方案、哪些没解决被标记(已解的嵌进对应操作步,未解的
> 独立可追踪);③ 对 buddy 越用越成熟的认知。
>
> 写法:假设→动作→真实输出→结论。**已解决的坑**嵌进对应操作步作「问题→我的判断→改了啥」
> 叙述(非流水账;自己修 A 引发 B 不重复申明);**未解决/待修**进 §4 未决,关联指回
> 主流程/分支操作的哪一步。归正注保留原始过程 + 修正,不抹。读回为证,不硬编。
>
> 维护规范见 `.claude/skills/practice-buddy-landing/SKILL.md` §6。文档写哪见 `docs/doc-boundaries.md`；还没干的活见 `docs/LEFTOVERS.md`；版本出包见 `docs/releases.md`。

## 目录

- [1. 前置:用 buddy 前要装/配啥](#1-前置用-buddy-前要装配啥)
  - [运行时必装必配(干活绕不开)](#运行时必装必配干活绕不开)
  - [按项目 provider 选装](#按项目-provider-选装)
  - [开发/验证才需要(跑门禁 + E2E 读回)](#开发验证才需要跑门禁--e2e-读回)
  - [可选 env](#可选-env)
  - [启动](#启动)
- [2. 主流程:创建项目 → 跑三件套(竞品分析 → 找指标 → 绑数据 → 交棒)](#2-主流程创建项目--跑三件套竞品分析--找指标--绑数据--交棒)
  - [步1·启动 → 项目墙](#步1启动--项目墙)
  - [步2·创建流(Repo 卡 + Intent 卡)](#步2创建流repo-卡--intent-卡)
  - [步3·进 Op 侧边栏(看本项目有什么)](#步3进-op-侧边栏看本项目有什么)
  - [步4·跑竞品分析(competitive-analysis) ⚠ 阻塞:见 §4.3 bug① + §4.4 bug②](#步4跑竞品分析competitive-analysis--阻塞见-43-bug--44-bug)
  - [步5·跑找指标(north-star-discovery,能跑通,不联网)](#步5跑找指标north-star-discovery能跑通不联网)
  - [步6·跑绑数据(metrics-binding,实测 run ok ~19min)](#步6跑绑数据metrics-binding实测-run-ok-19min)
  - [步7·交棒 / merge](#步7交棒--merge)
  - [闭环验证(读回为证)](#闭环验证读回为证)
  - [环境坑(非 buddy 代码)](#环境坑非-buddy-代码)
- [3. 分支操作:Hub 全局库 / Op 项目运营](#3-分支操作hub-全局库--op-项目运营)
  - [3.1 Hub 全局库操作(公共可浏览,`project_id=NULL`)](#31-hub-全局库操作公共可浏览project_idnull)
    - [skill(技能库)](#skill技能库)
    - [agent(智能体)](#agent智能体)
    - [workflow(工作流)](#workflow工作流)
    - [cron(定时任务)](#cron定时任务)
    - [connector(连接器)](#connector连接器)
    - [knowledge(知识源)](#knowledge知识源)
  - [3.2 Op 项目运营操作(`project_id=本项目`)](#32-op-项目运营操作project_id本项目)
    - [stages(五阶段环)](#stages五阶段环)
    - [issues(看板)](#issues看板)
    - [metrics(指标 + 健康灯)](#metrics指标--健康灯)
    - [artifacts(产物登记)](#artifacts产物登记)
    - [version(版本日志)](#version版本日志)
    - [sessions(会话)](#sessions会话)
- [4. 未决事项(当周发现;总表在 docs/LEFTOVERS.md)](#4-未决事项按主题关联指回主流程分支操作哪步)
  - [4.1 创建流 UI 该不该收窄(指回步2)](#41-创建流-ui-该不该收窄指回步2)
  - [4.2 创建时不该自动开工(run_first / auto-run,指回步2)](#42-创建时不该自动开工run_first--auto-run指回步2)
  - [4.3 bug① 冻死·RunIssue 甩后台 + 并行 run 无 worktree(指回步4/5/6)](#43-bug-冻死runissue-甩后台--并行-run-无-worktree指回步456)
  - [4.4 bug② 联网墙(指回步4)](#44-bug-联网墙指回步4)
  - [4.5 issue 技能绑死·无 UpdateIssue 命令(指回 §3 issues)](#45-issue-技能绑死无-updateissue-命令指回-3-issues)
  - [4.6 issue 看板要不要从仓库取(指回 §3 issues)](#46-issue-看板要不要从仓库取指回-3-issues)
  - [4.7 连接器动作位置(hub-Op 边界)+ probe-at-creation(指回 §3 connector)](#47-连接器动作位置hub-op-边界-probe-at-creation指回-3-connector)
  - [4.8 skill/agent 渠道6 规范 + 归属反转(指回 §3 skill/agent)](#48-skillagent-渠道6-规范--归属反转指回-3-skillagent)
  - [4.9 cron / workflow / connector(运行体系)定位 gap 搁置(指回 §3)](#49-cron--workflow--connector运行体系定位-gap-搁置指回-3)
  - [4.10 auto-mint「失败就停」需持久化标志位(指回步2)](#410-auto-mint失败就停需持久化标志位指回步2)
  - [4.11 滞后指标 UI 渲染 GAP(指回步6 / §3.2 metrics)](#411-滞后指标-ui-渲染-gap指回步6--32-metrics)
  - [4.12 plan18 step3 收尾·代码侧已交付 + 未决(指回步5/6/7)](#412-plan18-step3-收尾代码侧已交付--未决指回步567)
  - [4.13 V3 两篇方案已记、未落地(指回 §3 issues / 一张工作台)](#413-v3-两篇方案已记未落地指回-3-issues--一张工作台)
  - [4.14 第一包 / 开发包 + 删阶段记录缺列(2026-08-14)](#414-第一包--开发包--删阶段记录缺列2026-08-14)
  - [4.15 采集仍跑旧脚本·合入是否更新主目录(2026-08-14)](#415-采集仍跑旧脚本合入是否更新主目录2026-08-14)
  - [4.16 V3 使用问题(2026-08-17)](#416-v3-使用问题2026-08-17)
  - [待记(后续会话补)](#待记后续会话补)
- [5. 认知(buddy 是什么、能带来什么)](#5-认知buddy-是什么能带来什么)
  - [两个面(buddy = 看板 + AI 小队)](#两个面buddy--看板--ai-小队)
  - [四个铁律(防蔓延,不假装)](#四个铁律防蔓延不假装)
  - [codehub 对接设计(步1 落地的认知)](#codehub-对接设计步1-落地的认知)
  - [连接器同步背后(git-repo vs codehub/github-repo vs collect arm)](#连接器同步背后git-repo-vs-codehubgithub-repo-vs-collect-arm)
  - [skill / agent 形态归宿(2026-07-31 钉死)](#skill--agent-形态归宿2026-07-31-钉死)
  - [plan18 step3 收尾认知(2026-08-03)](#plan18-step3-收尾认知2026-08-03)
  - [run 调度层认知(2026-08-03,plan17 S1-S5 落地钉)](#run-调度层认知2026-08-03plan17-s1-s5-落地钉)
  - [V3 一张工作台 + 执行器预研(2026-08-14)](#v3-一张工作台--执行器预研2026-08-14)
  - [实践收口的一句话价值(2026-08-14)](#实践收口的一句话价值2026-08-14)
  - [反命题(buddy 不是什么)](#反命题buddy-不是什么)

---

## 1. 前置:用 buddy 前要装/配啥

### 运行时必装必配(干活绕不开)

| 项 | 装啥/配啥 | 为啥 |
|---|---|---|
| **git** | 任何 clone/workspace 操作都要 | 通用 |
| **claude CLI** + **`BW_CLAUDE_BIN`** | AI 干活(issue 执行器)shell-out `claude`,要给全路径(Windows 上 Rust `Command::new("claude")` 不做 PATHEXT;`.cmd` 还要包 `cmd.exe /c`,见 §4.16) | 优先 `...\claude-code\bin\claude.exe`;没有 exe 时认 `%APPDATA%\npm\claude.cmd`。安装器与应用同一顺序,写入/解析 `BW_CLAUDE_BIN` |
| **LLM 网关** | `claude -p` 真跑要打到 LLM 网关 | 我的 claude 已指向 GLM first-party(`~/.claude.json` 配置,非 buddy 配);529 间歇,仓里 `claude_cli.rs` 已重试退避 |

### 按项目 provider 选装

| 项目类型 | 装 | 配 |
|---|---|---|
| **codehub 项目**(如 maas) | `codehub-cli`(npm 全局)+ token 进 keyring | `codehub-cli auth login --token <t> --host <区域>`;host 三选一(绿 `codehub-g.huawei.com`/黄/内源 `open.codehub.huawei.com`)。SSH host 是 `szv-open.codehub.huawei.com:2222`(≠ API host,别混) |
| **github 项目** | `gh` CLI | `gh auth login`(全局,不用每项目) |
| **本地仓项目** | 无(只绑本地目录) | 无 |

### 开发/验证才需要(跑门禁 + E2E 读回)

| 项 | 干啥 |
|---|---|
| Rust 工具链 + **wasm32 target** | 编译 + 门禁 wasm 项(`cargo check --target wasm32-unknown-unknown`;`rustup target add wasm32-unknown-unknown`,官方源卡死用清华镜像 `RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup`) |
| **mingw**(WinLibs,dlltool+as) | Windows 上链接测试/example 二进制(rustup self-contained rust-mingw **缺 as 汇编器**,得装真 mingw;放 `C:\Users\<你>\mingw64\bin`,新终端才见) |
| **sqlite3 CLI** | E2E「读回为证」核数(buddy 纪律:数一律 SQL 读回) |

### 可选 env

- `BW_DB`(覆盖 DB 路径,默认 `AppData/Roaming/BuildersWorkbench/workbench.db`)
- `BW_WORKSPACES`(工作区根,默认同 DB 目录下 `workspaces/`)
- `BW_OPEN=<项目名>` + `BW_PANEL=progress|issues|...`(深链启动到指定面板,stderr `[BW_OPEN]` = 渲染证明)
- `BW_CLAUDE_MAX_BUDGET_USD`(单次 agent 花费封顶;1000 = 实际不设防 + 兜底防 runaway;0 在 claude CLI 里 = 允许花 $0 立即报错,不是无限)

### 启动

终端用户跑安装包(版本见 `docs/releases.md`,当前 `0.3.0-v3`)。开发机:

```bash
cargo run -p app-desktop   # 别直接跑 target/debug/builders-workbench.exe(Windows 崩 0xC0000135 无窗口)
```

---

## 2. 主流程:创建项目 → 跑三件套(竞品分析 → 找指标 → 绑数据 → 交棒)

> 完整三件套目标态写在这里;**有阻塞未解决的,在对应步上标 ⚠ 见 §4.X**,不假装能跑通。
> 每步:**你做啥 → buddy 后台在磁盘/UI/DB/人能读懂的背后动作上干了啥 → 你能看到啥**。
> 已解决的坑嵌进步里作「问题→我的判断→改了啥」。读回为证(DB=`…/BuildersWorkbench/workbench.db`)。

### 步1·启动 → 项目墙

- 你做:`cargo run -p app-desktop`。
- buddy 干:起桌面壳(Dioxus/wry),读 `workbench.db`,渲染项目墙。
- 看到:已有项目列表(空就是空,不假装)。

### 步2·创建流(Repo 卡 + Intent 卡)

- 你做:平台选 CodeHub;host = `open.codehub.huawei.com`(内源;绿区 `codehub-g.huawei.com`);path 手填 `innersource/AI-Coding_G/maas`(placeholder 不是值,不填下一步禁用);起点「接入已有仓」→ 下一步 → Intent 卡填项目名(maas-locate)+ 一句话 brief → 点「确认·建立项目」(末卡另有「立即开工竞品分析」勾选框,默认不勾,见 §4.2)。
- buddy 干(点「下一步」时):SSH clone 真 maas 仓进 `workspaces/maas-locate-<id>`(远程 `git clone ssh://`,不经代理、不要 token)。
- buddy 干(点「确认」时**一气做完**):① 写一条 project 行(name + `remote_host` + `remote_path` + `provider`);② 建两个 connector(代码仓 workspace 路径 + CodeHub `host/path`)写进 DB;③ 建 3 张 BW 标配 Issue 卡(竞品分析/找指标/绑数据)写进 DB,**同时往 codehub 仓真开 3 个 issue**(iid 连号),DB issue 的 `github_number` = codehub iid;④ CompleteCreation 末尾对建完的连接器就地 `probe_connector`(代码仓翻 Connected + 喂工作区指标;CodeHub 探活翻 Connected);⑤ 若勾「立即开工」→ 唤起 claude 跑第一张卡(见步4)。
- 看到:issues 看板 3 张卡;connector 列表有 `· 代码仓`(已连接)+ `· CodeHub`;`codehub-cli issue list` 远端实见 3 条(iid 跟 DB 对得上);工作区目录真出现 maas 文件(`governance/`/`docs/`/`skills/`/`AGENTS.md`…)。
- **已解的坑(嵌进步里)**:
  - **clone 504**:`codehub-cli repo clone` 走 HTTPS 经代理 504 → **判断**codehub 是局域网 HTTPS 被拦 → **改** `clone_repo` 走 SSH(`project view --template .ssh_url_to_repo` 取 SSH URL,SSH host `szv-open:2222` ≠ API host `open`,不手拼;raw `git clone` + `GIT_SSH_COMMAND` accept-new+BatchMode)。
  - **path 空能提交**(Q1,`f66814b`):RepoCard `can_send` `||` 优先级漏,codehub 下 `is_new` 盖过 path 要求 → **改** `if is_codehub { path 必填 } else { ... }`,gate 放 Repo「下一步」源头,置灰显「path 未填」。
  - **codehub 录入却碰 github**(Q2,已修):Repo 卡 `platform` 默认 github + chip 一点就 fire `gh repo list` → codehub 用户没切平台就碰 github → **改** `ListGithubRepos` 从 chip 急触发改「↻ 刷新列表」按钮懒触发(codehub 用户根本不碰 github 块)。
  - **claude spawn "program not found"**:默认裸 `"claude"` → Windows Rust `Command::new` 不做 PATHEXT → os error 193 → **改**(不碰代码)设 `BW_CLAUDE_BIN=…\claude.exe`(持久 User env);config gap 非代码 bug。自动探测留后续。
  - **重复点「建立项目」非幂等**(Bug B,`1dbd76c`):confirm 按钮无 pending 守卫 → 多点建多套 trio + codehub 孤儿 issue → **改** confirm 起手置 `submitting=true` 置灰「建立中…」,完成/失败后翻页/解禁。
  - **【归正·Bug B 半套】**(`submitting` 在 `main.rs` 父组件,Create 卸载不清):clone 软失败走 `ConnectorSynced` 不是 `UiNote::Error`,第一次点确认后标志一直为 true。同事修完 SSH 再进创建流,还没点确认按钮已是灰色「建立中」。**改**离开创建屏 / 「+ 新建」/ 返回项目墙时复位。
  - **合入后立刻采一轮**:绑数据 merge / 点「→已完成」原先只装 `.bw/metrics.toml`+`connectors.toml`,不跑采集,总览空着等人发现「立即采集」。**改** Done 记账口在 sync 之后走同一条 `collect_project_metrics` 并 toast 结果;失败不回滚验收。
  - **auto-mint(clone 失败悄悄本地 mint 像接上了)**:**判断**对齐 github(Existing clone 失败 CompleteCreation 也 auto-mint 空项目)→ clone SSH 修好后非空不触发。真「失败就停」需持久化「尝试过远端」标志位 → ⚠ 长期 TODO,见 §4.10。

### 步3·进 Op 侧边栏(看本项目有什么)

建完进 Op,左侧栏分组。**读回 DB 核实**(project_id=`cce215a7…`):
- **技能 +8 共享 / 智能体 +5 共享 / 工作流 +5 共享**:全局库(`project_id=NULL`,Boot seed,跨项目共享,本项目只有使用权)。8 技能 = 5 阶段方法论 + 3 标配(competitive-analysis/north-star-discovery/metrics-binding);5 agent = 五角色全局单例;5 workflow = 五阶段 template。**本项目没自有的原因**:归属反转未做(见 §4.8)。
- **定时 · maas-locate 指标采集(normal)**:本项目自有 cron(创建时 seed)。mode=collect_metrics,每日对 codehub/github 远端跑 `codehub-cli issue|mr list --jq length` 取「开放 Issue 数/已合入 MR 数」→ observation → 重算 signal。靠 CodeHub 连接器的 host/path 才知道去哪采。
- **连接器(本项目自有 2 个)**:代码仓(git-repo,已连接,本地工作区 git 探针)+ CodeHub(codehub-repo,探活翻已连接,只看活不活不喂指标,指标靠 cron 采)。
- **知识源**:0(未建)。
- 一句话:技能/智能体/工作流 = 全局共享库(只读借用);定时 + 连接器 = 本项目自有;代码仓同步喂工作区指标、CodeHub 同步只探活。
- **Hub 资产存哪/怎么统计**:全局库实体存 DB 表(`skill`/`agent`/`workflow_spec`/`cron_task`/`connector`/`knowledge_source`);App 启动 + 每次相关命令后 `refresh_*` 重读 → state → `build_vm` → HubVm/OpVm.hub → 侧边栏计数。**侧边栏的数 = DB 表行数,`sqlite3` 直查可对**。

### 步4·跑竞品分析(competitive-analysis) ⚠ 阻塞:见 §4.3 bug① + §4.4 bug②

> 当前别在 UI 里点真跑——会冻死(§4.3)+ 联网墙空转烧预算(§4.4)。绕法:先跑步5 找指标(不联网能跑通)。

- 你做:issues 面板 →「竞品分析」卡 → 点「▶ 跑」(或创建流勾「立即开工」)。
- buddy 后台干(读回):① issue `backlog→in_progress`(状态机起手);② `workflow_run` 落 `running` 行(`force_mock:false`、`allowed_tools_arg:Bash`、5 阶段证据/洞察/假设/原型/验证、`max_iter 3 retries 1`);③ 把**竞品分析 skill 正文** + 你创建时填的**对标(benchmark)/算成(opportunity)** 经 `PlaybookCtx` 拼进 prompt → spawn 真 `claude.exe`(父进程=builders-workbench,工作区目录里跑,`--permission-mode acceptEdits --allowedTools Bash`,预算封顶 $1000);④ agent 应产出 `docs/competitive-analysis.md`。
- ⚠ **bug① 冻死**:点完侧边栏冒「进行中」一帧 → 整个界面冻死。根因:`Command::RunIssue` handler(`lib.rs:4948`)直接 `self.run_issue_now().await`、没 `tokio::spawn` 甩后台;`run_issue_now` 是 `&mut self`,整段(含 spawn claude + 等输出,最长 30 分钟)内联在 kernel 单线程命令循环 `app.dispatch(cmd).await`(`kernel.rs:641`)里,循环一卡、Vm 只在 dispatch 返回后重发(`kernel.rs:644`)→ 视觉冻死。**反证它不该这样**:run 中段(长 claude spawn)其实只用共享借用(`lib.rs:1457` 注释「never &mut self」),不需要独占 App。修法方向:甩后台 detached,拦路虎是 App 单线程独占非 `Arc<Mutex>`(`kernel.rs:627`)。>2 行,留 §4.3。
- ⚠ **bug② 联网墙**:真执行器只给 Bash、没给联网工具,claude 在「检索被拒→硬编」里空转烧预算,没产出文件。根因 = GLM 网关不支持内置 web 工具(WebSearch 返空/WebFetch 报 blocking claude.ai)+ acceptEdits 下未预批工具直接拒 + trio agent 卡没声明 WebSearch。修法待定(诚实降级/接 web-access skill/换真 Anthropic 端点),见 §4.4。
- 处置(复现时):杀 buddy 起的那个 claude 子进程(`Get-CimInstance Win32_Process` 查 ParentProcessId 精确定位,别裸 `taskkill claude.exe` 误杀别的会话)→ run 结算 `failed`(exit `0xffffffff` 是杀进程造成)→ issue 停 `in_progress`、`settled_at` 空 → **没自动 Done,铁律守住**,界面解冻。
- 结论:「点跑→冻死」当前确定行为。bug① 修好前,要复现/调 bug② 走 headless example,别在 UI 点。

### 步5·跑找指标(north-star-discovery,能跑通,不联网)

> 实测 run `ok`(约 21 分钟,7 commit)。DB=`…/workbench.db`、工作区=`…/workspaces/maas-locate-cce215a7`、分支 `bw/issue-32`。

- 你做:issues 面板 →「找指标」卡 → 点「▶ 跑」。
- buddy 后台干:① 把**找指标 skill 正文** + 你创建时填的**项目意图(brief/north_star)** 经 `PlaybookCtx` 拼进 prompt → spawn claude;② agent 只用 Read/Write/Bash(`--allowedTools Bash` + acceptEdits 下 Read/Write 真能写文件,这道工具白名单墙对不联网技能不成立);③ 产出 `.bw/metrics.toml`(机读指标定义:北极星=定界结论采纳率 manual + 滞后未采纳率/闭环时长)+ `docs/metrics-rationale.md`(11.7KB 推导)+ `evidence/insights/hypothesis/validation/`;④ agent 自己 git commit 7 个(规范信息带技能名+issue 号),分支 push 到 codehub(`origin/bw/issue-32`)。
- 看到:issue `in_progress`;run 期间界面冻死 ~21 分钟(同 bug①);run `ok` 后 session `#2 找指标` 有 5 条 message(每阶段一条 agent 自述,**按 phase 完成增量落账**,不是不落账——只是 run 期间冻死看不到增量)。
- **认知(三件套流水线 + 监控时机)**:找指标只「定义」指标(写 `.bw/metrics.toml`,大部分 manual 因 codehub 远端 + 评估器留白采不到);**真监控在绑数据之后**——绑数据给每条指标接点亮路径、cron/连接器采 → observation → recompute_signals → 健康灯亮。找指标跑完 ≠ 开始监控。北极星(采纳率)是 manual,只能人定期填亮,这诚实。
- **已解的坑(嵌进步里)**:
  - **bug③ codehub MR 回流断**(`0c70775`,7 文件):run `ok` 但 issue 卡 `in_progress`、pr_number=0、没推 InReview。根因:`github::open_pr`/`merge_pr` 写死 `gh`,codehub 上 gh 失败 → **改** `Remote` 工厂加 `create_mr`(github 透传 / codehub `codehub-cli mr create --source-branch --target-branch --issue-nums --jq .iid`)+ `merge_mr`(codehub `codehub-cli mr merge <iid> --squash -y`)+ Adopted(认领已存在 MR);抽 `workspace::stage_commit_push` 共用;`run_issue_now`/`MergeIssuePr` 收尾走 Remote 工厂。读回:烟测 `codehub-cli mr create` 真开 MR(iid 11/12),`merge_mr` 真打 codehub 拿真实 403(命令对)。
  - **bug③ 后续·merge_mr 读回 state==merged**(`b02047b`):`codehub-cli mr merge` 退出码靠不住——403(protected-branch / 无 merge 权限)时首次调用可退出 0(error 只打 stderr)、MR 实际没合;原 `merge_mr` 只看退出码 → 当成成功 → `MergeIssuePr` 结算 Done 但 MR 没合 + SyncMetricsFile 没跑 → **假 Done**(找指标/绑数据实测中招,撞铁律)。**改**:merge 后复跑 `mr view --jq .state`,只有 `merged` 才 Ok,否则 Err(带 merge stderr)。**读回为证,绝不假 Done**。
  - **UI① issue 地址写死 github.com + 裸 URL**(已修):`OpVm` 加 `provider`;issue 卡 issue 地址 + PR 号改 provider-aware 可点击 link(codehub `{host}/{path}/issues`|`/-/merge_requests`,github `github.com/...`)。**已 live 验证**:issue 卡「远端 #N ↗」「PR #N ↗」link 跳 codehub issue/MR 页。
  - **UI② PR #N 纯文本无 link**(已修):同上,link 到 `/-/merge_requests/{iid}`。
- **实践操作(对账绕 bug① 冻死)**:找指标(#32)烟测已建 codehub MR 11;绑数据(#33)无 MR → 手动 `codehub-cli mr create` 建 MR 12 → 停 app → `sqlite3` 对账 `UPDATE issue SET pr_number=11/12, status='in_review'`(诚实:MR 真存在,legal 转移)→ 重编重启 → 两张卡显 InReview + merge 按钮。这是避 bug① 的手动对账快捷路径;正路是 retry(等 bug① 修好),retry 走 create_mr/Adopted 自动建/认领 MR。
- ⚠ **bug⑤ 发送框是 mock 占位**(留白):工作流面板发送框 = `Command::SendSessionMessage`,handler 只回写死 `【mock】已收到:{text}`(真 agent 回复走 Tier C 未实现)。run 是 `claude -p --no-session-persistence` 每阶段一次性调用、无持久对话;要 agent 重调得重新 RunIssue。如实标注留白,不是坏。
- ⚠ **④ 两个 MR 内容重叠**(工作流,见 §4.3):找指标+绑数据并行跑(都从 master 出分支、都改 `.bw/metrics.toml`)→ MR 冲突。正路串行:先 merge 找指标再跑绑数据。buddy 没强制依赖 + 无 worktree 隔离。

### 步6·跑绑数据(metrics-binding,实测 run ok ~19min)

> 让「找指标」定义的指标真正可采、可点亮。实测(2026-07-31,DB/工作区同步5,分支 `bw/issue-33`、MR 12)。
- 你做:issues 面板 →「绑数据」卡 → 点「▶ 跑」。
- buddy 后台干:把**绑数据 skill 正文** + 项目意图经 `PlaybookCtx` 拼进 prompt → spawn claude → agent 给 `.bw/metrics.toml` 里每条指标接**采集方案**(`collect_kind` 四类,见 §3.2 metrics),把 manual 的接成可采的,写回 `metrics.toml`(commit `1d96c3e issue #3: 绑数据`)+ 产出绑数据文档 + commit/push/MR 12(`origin/bw/issue-33`)。
- **实测读回(collect_query 精修进表)**:绑数据把找指标的 collect 字段精修成具体采集节奏——`每周合并 PR 数`(manual)「每周一 `git log --merges --since='7 days ago'` 手填」· `经验资产周增长`(bw)「`governance derive_data 扫描 skills/issue-analysis/{decision-trees,knowledge}`」· `每周结算 Issue 数`(bw)「`issue.settled_at within 7d`」· `客户端问题拦截准确率`(manual)「每月末运维专家核对 auto-ticket 按『Apifabric/网关无会话记录』判...」· `平均定界闭环时长`(manual)「录入+审结时点手填」· `定界结论未采纳率`(manual)「每月末运维专家对照已审结论数驳回数手填」· 北极星(定界结论采纳率)= manual(采纳须人判,评估器留白不假装自动)。
- **所以**:找指标 = 定「指标是什么」(name/def/target),绑数据 = 定「怎么采」(collect_kind + collect_query)。两件都进 `.bw/metrics.toml`,merge 后 `SyncMetricsFile` 装进 metric 表(读回:metric 表 7→13)。
- ⚠ **④ MR 重叠**:绑数据 MR 12 跟找指标 MR 11 冲突(都改 metrics.toml,见步5 ④/§4.3)——实测中手解冲突(codehub 上取绑数据那版=找指标+绑数据合集)后合入。
- ⚠ **滞后指标 UI 渲染 GAP**:trio 指标 `stage_kind=NULL`(项目级),ProgressStage 只显绑阶段的(`kernel.rs:970` `filter(stage_kind==Some)`),ProgressAll 只有「本周计划=引领」——**项目级滞后指标装进表了但 UI 没渲染段**,看不见。北极星在顶栏 TopBar(不在进度正文)。引领能看见是因为 `week_plan` 单独拉。见 §4.11。

### 步7·交棒 / merge

- 你做:InReview 卡点「merge」(或 codehub 网页 merge)。
- buddy 干:`MergeIssuePr` → `Remote.merge_mr`(codehub `codehub-cli mr merge <iid> --squash -y`)→ merge 成功推 Done(InReview→Done,人点的 merge 触发,非自动)。Done 记账口再对**主工作区**(`project.workspace_path`,不是 issue worktree)跑 `sync_default_branch`:fetch → checkout 默认分支 → `pull --ff-only` → 从主目录读 `.bw/metrics.toml` / `connectors.toml` 装进库 → 立刻采一轮。worktree 里的新脚本不会直接拷进主目录,是远端合入后再拉回主目录。`pull` 失败被吞掉仍算收拢成功(见 §4.15)。
- ⚠ **merge 403 不是 buddy bug**:`merge_mr` 命令对(真打 codehub 拿 403「target branch is protected, you do not have MERGE permission」)——是 maas master 保护分支 + CLI token 无 merge 权限的治理问题。buddy 如实报错、issue 留 InReview 可重试。解法在 codehub 侧:网页有权限账号 merge / 解保 master / target 真实开发分支(`a_develop`)。
- **实测(2026-07-31,trio 走通)**:找指标 MR 11 + 绑数据 MR 12(冲突手解)都 codehub 网页合入 → buddy 点「⬇ merge PR」(`MergeIssuePr`)→ `merge_mr` 读回 state==merged(见步5 `b02047b`,不看退出码)→ Done + `SyncMetricsFile` 拉 master + 装 trio 指标(metric 表 7→13,北极星+3滞后+3引领进表,全 unknown 诚实)。**trio 生命周期(跳过竞品分析)端到端走通**:定义(找指标)→采集方案(绑数据)→merge→Done→指标进表。⚠ 实测中撞过 merge_mr 假 Done bug(`codehub-cli mr merge` 退出码靠不住→假成功→Done 但 MR 没合+没装指标),手动 reset 两 issue 回 InReview(清 settled_at),`b02047b` 修好后重试真合。
- **【归正·codehub issue 自动关单】**(`a76c6c5`):实测 issue 31/32/33 merge 后仍 `opened`——**`--issue-nums` 只 link MR↔issue,不触发自动关单**(我之前假设它自动关,错了)。codehub(GitLab)自动关单靠 **MR description 里的 `Closes #<iid>`**(和 github `Closes #n` 一样,merge 时自动关引用的 issue)。**改**:`codehub::create_mr` body 加 `Closes #<iid>`;`MergeIssuePr` 的 gh issue 补关仍 gate github-only(codehub 靠 body Closes,不走 gh 补关)。现有 31/32/33 历史 MR body 没 Closes、issue 仍开(可 codehub 网页手关);未来 codehub MR 会自动关。**未验**(`Closes #` 在 codehub body 是否真触发自动关,待下次 codehub MR merge 实测;GitLab 标准行为,默认 ON)。

### 闭环验证(读回为证)

- **live 端到端录 maas(步1 验证)**:
  > 【2026-07-30 归正】本块早前结论后被实测推翻/完成,原文保留作过程记录,以本注为准:Bug A 真根因不是权限,是 GLM 网关不支持内置 web 工具 + glm-5.2 不老实降级烧预算(见 §4.4);Bug B 多点 bug 已修 `1dbd76c`;529 误判已撤回(预算错说明网关在响应、真花钱,非 529);冻结机制已定论=单线程命令循环被长 run 阻塞(见 §4.3)。
  - ✅ `BW_CLAUDE_BIN` 配对后 spawn claude.exe 成功,"program not found" 消失。
  - ✅ SSH clone 真 maas 成功:`workspaces/maas-locate-cce215a7/` 有真 maas 内容(AGENTS/CLAUDE/governance/docs/skills…),SSH 绕开 HTTPS 504。
  - ⚠ 重复点「建立项目」非幂等:点了 N 遍 → codehub 上建 5 套 trio(iid 16-30),BW DB 只落库 2 套;3 套 codehub 孤儿(远端建成功 BW 回滚没落库,sync 非原子)。清理:关掉 12 个,留 iid 19-21。Bug B 修后此坑已解(防连点)。
  - ✅ 预算 `BW_CLAUDE_MAX_BUDGET_USD=1000`(User 持久,新终端继承)。**但权限没修前别重跑竞品分析**——web_search 仍被拒,重跑只烧钱/产空报告。
- **干净环境重跑(步1 收口)**:清空 BW DB + 删 `workspaces/` 5 孤儿后重跑创建(不勾立即开工):✅ SSH clone / ✅ project 行(maas-locate,phase=running,remote=open+innersource/AI-Coding_G/maas,workspace 落对)/ ✅ 2 connector(代码仓+CodeHub)/ ✅ 3 trio 落库 + codehub 远端一一对应(DB #1↔iid31/#2↔iid32/#3↔iid33,全 backlog)/ ✅ 没跑 AI 小队(workflow_run=1 是 Drafting mock)。**结论:步1 对接 codehub 闭环成立**(clone+project+connector+trio DB↔远端),不跑 AI 小队也成立。

### 环境坑(非 buddy 代码)

- **wasm32 target 没装** → commit 门禁 wasm 项红。修:`rustup target add wasm32-unknown-unknown`。
- **mingw 缺 as** → `cargo test`/example 链接 windows-sys 失败。修:装 WinLibs mingw。
- **rustup 官方源卡死**(华为内网) → 用清华镜像。
- **`codehub-cli --jq` 带引号、无 `-r`** → 用 `--template '{{.field}}'` 出裸串。
- **zombie exe 坑**:TaskStop 杀 `cargo run` 父进程后子 `builders-workbench.exe` 可能不死(锁住 exe)→ 下次 rebuild 报 `os error 5`。修法:`taskkill //F //IM builders-workbench.exe` 显式杀 exe 再 rebuild。

---

## 3. 分支操作:Hub 全局库 / Op 项目运营

> 主三件套之外的日常管理。每个套件:**能干啥 + 背后发生啥 + 已解问题/规范**。未解的进 §4 关联指回。

### 3.1 Hub 全局库操作(公共可浏览,`project_id=NULL`)

#### skill(技能库)
- **能干啥**:SkillHub 浏览全量(全局+项目混排,卡上 `◇ 项目名` chip 区分归属)+ 阶段角色筛 + 规范徽记(plan/16 S1-S7/A1-A3,合规绿色隐声,违规黄「待校正」)+ 新建(全局)/编辑正文/导入包/删除。唯一能 CRUD 的地方。
- **渠道(写入 skill 表)**:① Boot seed bw-standard 库(8 件 = 5 阶段方法论 + 3 标配,正本 `docs/skills/<slug>/SKILL.md` 编进二进制,经 `bw_canon.rs` canon 驱动播种,`Official{bw-standard}`+Mature);② 手建 `CreateSkill`(一律全局);③ 蒸馏 `DistillSkillFromIssue`(人从 Done issue 点,project_id 由源 Issue 派生=项目级,provenance);④ 导入单包 `ImportSkillPackage`(带支撑文件);⑤ 批量导入 `ImportSkillLibrary`(官方库走这条);⑥ 项目仓自带 assets 扫导(见下「项目级 skill」)。
- **项目级 skill(渠道6,种A)**:代码仓连接器同步时顺带扫 `workspace/skills/<slug>/SKILL.md` → 进表 `project_id=本项目` `source=Official{project-assets}`(种A:登记可见,**不进任何注入下拉**)。skill 用 `import_skill_package`(带 references/scripts 支撑文件);agent 单文件 AGENT.md 无 skill_file。全量重建那批(项目仓正本、buddy 镜像,种A 无引用无 uses,id 变无害);清孤儿双重 gate `project_id+source`,不碰蒸馏 SelfBuilt/全局。扫描失败软降级 stderr 留痕不阻断连接器探活。**已开发 `5583381`**。实测 maas 扫进 5 个(auto-ticket/issue-analysis/issue-collect/link-analysis/refresh-indicators,支撑文件 9/8/17/5/5)+ 0 agent(maas agents/ 只 README)。规范冲突分域:`project-assets` 自动算外库(`is_external_official`)→ Advisory 不点黄不强制改;解析层 SKILL.md+name+desc 硬门槛。详见 §4.8。
- **界面**:SkillHub(CRUD)/ Op issue 详情(standard_skill 下拉,种A 不列)/ component_detail(就地只读详情)/ WorkflowHub(crew 选技能,种A 不列)/ CronHub(RunSkill,种A 不列)。

#### agent(智能体)
- **能干啥**:AgentHub 浏览 + 阶段角色筛 + 新建/编辑。五角色 agent 全局单例(`project_id=NULL`,Boot seed,战绩跨项目混算——⚠ 归属反转未做,见 §4.8)。
- **渠道**:Boot seed 五角色(`seed_stage_role_agents_if_missing`,instructions=真实 preamble,`claude-code` 执行,`SelfBuilt`);导入 `ImportAgentDefinition`(ECC AGENT.md 单文件);项目仓自带 agents/ 扫导(种A,同 skill 渠道6,agent 单文件无 skill_file)。
- **界面**:AgentHub(CRUD)/ Op issue 详情(assignee 下拉,种A 不列)/ component_detail / WorkflowHub(crew 选队友,种A 不列)。

#### workflow(工作流)
- **能干啥**:WorkflowHub 浏览 + 新建/优化/临时任务(挂 `SkillAgentPicker` 真实目录,按名落 `SkillRef{from:"SkillHub"`,种A 不列)+ 版本历史。五阶段 template workflow(全局 seed)。
- **背后**:workflow `skills_json` 顶层 + T16 phase 层绑定(`phase_skills`);SkillHub「被这些工作流使用」反查含 phase 层(修了「被 0 个工作流使用」失真)。

#### cron(定时任务)
- **能干啥**:CronHub 浏览 + 新建。mode 分发:`collect_metrics`(指标采集,到点对远端跑 collect_count)/ `run_skill`(RunSkill,选 skill 跑,种A 不列)/ `run_prompt` / `run_stage_playbook`。**cron 只自动建活,永不自动完成活**(铁律)。
- **本项目自有**:创建时 seed 指标采集 cron(`project_id=本项目`,每日)。

#### connector(连接器)
- **能干啥**:ConnectorHub 浏览 + 同步(`SyncConnector`→`probe_connector`)。代码仓(git-repo)= 本地工作区 git 探针,同步喂工作区指标(commits/docs);CodeHub/GitHub(codehub/github-repo)= 远端探活翻 Connected,只看活不活不喂指标(指标走 cron collect arm);claude-cli=claude 版本探针。
- **已解**:syncable 漏认 github-repo/codehub-repo → 补认(两 kind 有真探针 `gh repo view`/`codehub-cli project view`,UI syncable 没跟上)。**已 live 验证**:CodeHub 连接器出「立即同步」按钮、点后探活翻已连接。
- ⚠ **缝点**:连接器是项目级数据,但管理动作(同步)在 Hub 不在 Op——用户在 Op 找不到同步按钮。**hub-Op 边界待挪**,见 §4.7。

#### knowledge(知识源)
- 0,未建。留口不假装。

### 3.2 Op 项目运营操作(`project_id=本项目`)

> 这些套件不在侧边栏显式列(像 skill/agent 那样),而是在 Op 操作过程中隐含感受到的——这里记它们的机制 + 已解问题。

#### stages(五阶段环)
- 原型→构建→优化→运营推广→运维,每段自带打法(该问什么/什么节奏/DoD/常见坑,`StageKind` 静态元数据)。运维复盘回流原型(环不是流水线)。阶段推进=交棒,清单未勾完可交但强制标「带险」留痕(append-only 审计)。

#### issues(看板)
- **看板 = `store.list_issues(DB)`**,只列 buddy 自建/管的 issue 卡(trio + 用户后建的);**不导入** codehub 上项目原有 issue(如 maas 的 DTS 单)。项目原有 issue 只进**指标**(collect_count 数「开放 Issue 数」),不进看板。⚠ by design,要不要拉进看板见 §4.6。
- issue 的 `standard_skill` 在 `CreateIssue` 时一次性写入,**之后没有命令改**(无 `UpdateIssue`/`SetIssueSkill`)。⚠「关联技能」下拉只在建新 issue 表单,见 §4.5。
- 状态机:`can_transition_to` 守卫,Done 入边仅 InReview;Done 永不自动、破坏性永不自动(铁律)。

#### metrics(指标 + 健康灯)
- **采集器四类**(`CollectKind`,bw-engine/metrics_file.rs:40,每条指标必附一个,来自 `.bw/metrics.toml` 的 `CollectPlan`):
  - **github**:远端 issue/mr/release 计数,经 `Remote.collect_count`(codehub-cli/github-cli)。
  - **connector**:BW connector 探针(如 git-repo 喂工作区 commits/docs)。
  - **bw**:BW 自身记账(issue settle-count / run telemetry / stage done count)。
  - **manual**:人手填,无采集器自动填(带「手填」徽)。
- **实采呈现**:等 cron tick 或手动 `CollectMetrics` → 在**主工作区**(`project.workspace_path`)shell-out 跑 script connector → 写 `observation` 表(append-only,一个观测=一个点,绝不插值;window-guard:同窗口同值才跳过)→ `recompute_signals` 重算 → 指标卡点亮。公共指标(开放 Issue 数/已合入 MR 数)开机默认 seed。**「立即采集」不拉主目录、不看 issue worktree**(见 §4.15)。每次采集还会覆盖 buddy 自带的 `.bw/collect_stats.{sh,py}`(项目业务脚本不动)。
- **健康灯 derive-only**:Signal 只能经封口 `Derived<Signal>` 进缓存,store 无 `set_signal`,`recompute_signals` 唯一写入者;**无数据=Unknown≠绿**,数据过期降级,手填带徽。任何界面数字能 `sqlite3` 独立查证。

#### artifacts(产物登记)
- agent run 产出的文件(docs/evidence/...)登记到 artifact 表,归属项目。

#### version(版本日志)
- `LoadVersionLog` shell-out `git log` 取版本日志。

#### sessions(会话)
- run 的 message 按 phase 完成增量落账(每阶段一条 agent 自述);run 是 `claude -p --no-session-persistence` 每阶段一次性调用,无持久对话(⚠ 发送框是 mock,见步5 bug⑤)。

---

## 4. 未决事项(按主题,关联指回主流程/分支操作哪步)

> **还没干完的唯一清单是 [`docs/LEFTOVERS.md`](../docs/LEFTOVERS.md)。** 本节只记当周发现;消化后迁进那份清单或关掉,不在这里养第二份总表。文档边界见 [`docs/doc-boundaries.md`](../docs/doc-boundaries.md)；版本出包见 [`docs/releases.md`](../docs/releases.md)。
>
> 讨论有价值但非当前主要矛盾、现在做了也不一定对的事。每条:讨论啥 + 当前决议 + 待什么条件回头。

### 4.1 创建流 UI 该不该收窄(指回步2)
- 创建末卡让用户填对标/算成/北极星/引领·滞后指标。对标+算成=人意图填得准(竞品分析 skill 读 `benchmark`/`opportunity`);北极星+指标=要推导的(「找指标」issue 本职),人填不准、创建时填了没反馈。**决议:先不动**。`run_first` 默认不勾(§4.2)已是现状;指标值选填可空。等三件套实践中想明白「输入实际怎么被用、哪填合适」再回头——**不基于猜测改设计**。

### 4.2 创建时不该自动开工(run_first / auto-run,指回步2)
- 竞品分析不该创建时默认跑,由人创建后在 issue 卡触发。已对齐:`run_first` 默认 false(`create.rs`)。保留末卡「立即开工」框、默认不勾。

### 4.3 bug① 冻死·RunIssue 甩后台 + 并行 run 无 worktree(指回步4/5/6)

> **【2026-08-03 归正·bug①+④ S1-S5 已实施】**(plan17 worktree 5 commit,合体进整合分支 `merge-run-scheduling-step3-metric-loop`,git auto-merge(ort)零冲突,门禁 6/6 绿,SubAgent 检视「可提交」):bug① 冻死 + ④ worktree 隔离**本窗已落地**,下面的「未决/修法待定」原文保留作过程记录。要点:
> - **① 解冻靠三段拆**:`run_workflow_inner` 拆 `prepare_run`(&self 起手,返 owned `PreparedRun`)/ `Self::run_round_loop`(关联 fn 无 self,长对抗循环;`self.store`→`store:Arc`,`self.emit(X)`→`live.send(X)`)/ `finalize_run`(&mut self 收尾记账);issue 路径 `tokio::spawn(run_round_loop)` 甩后台、经 settle mpsc 回灌主线程跑 finalize+tail,**不动 App 所有权**(单线程 `current_thread` runtime 靠 await 点交错推进,零 `Arc<Mutex>`)。
> - **串行锁 S1**(`198294a`)从 dormant 变真守卫:`active_run` 从 `(ProjectId,IssueId)` 扩成 `ActiveRun{JoinHandle,guard,finalize,issue,proj,issue_ws,...}`(同项目串行,跨项目可并发)。
> - **S2**(`b9b28ad`)每 issue 独立 git worktree 隔离(`IssueWorktreeGuard`,Drop 拆);**S3**(`93c7a33`)三段拆 + `Command::CancelRun`(abort JoinHandle + `kill_on_drop` 杀 claude 子进程,issue 留 InProgress + `settled_at` 空,铁律守);**S4**(`2e607a3`)去 `--no-session-persistence`。
> - **偏差(如实)**:cancel 不走 finalize 记账(无 last_run_log,中止轮 `workflow_run` 行留 started-never-settled 即诚实留痕,agent/skill uses 不+1)——留读回 `artifact_version` 表核复利链是否断。scan_and_register_artifacts 即时登记=0 的复利链 gap:S2 后 run 产物在 issue worktree 未 merge,即时扫主工作区恒 0(诚实),推迟到 merge 后补登记。
> - L1 非阻塞(已修 commit `bacae9a`):`schema.sql` 三处注释(north_star_collect_kind/collect_kind/source_kind)漏列 `script` 枚举值,补注释(schema 双守卫口径,纯注释)。

- **bug①**:任意长 RunIssue 都冻死单线程 UI(根因见步4)。修法:RunIssue 甩后台 detached,拦路虎是 App 单线程独占非 `Arc<Mutex>`(`kernel.rs:627`);退一步 Vm 在 run 期间按事件/定时重发也能解冻视觉。>2 行,设计决定,留下一棒。**先修它,再回头调 bug②**(界面不冻了才能在 UI 观察 bug② 真实终端报错)。
- **并行 run 无 worktree(同源)**:run 共用一个 workspace 目录、无 worktree-per-run 隔离 → 并行/连续跑多个 issue(三件套 找指标→绑数据)时 MR 内容重叠(都从 master 出分支、都改同一文件)。**两件都在「run 调度」层**一起设计:甩后台 + worktree-per-run(`git worktree add` per issue)+ (可选)三件套串行依赖(绑数据要求找指标先 merge)——待实践定,别基于猜测改。
- **给修 bug① 窗口的 prompt(可粘)**:在 buddy 修 bug①「RunIssue 内联 await 堵死单线程 UI」时,一并看这个同源问题:run 共用一个 workspace、无 worktree-per-run 隔离,导致并行/连续跑多个 issue(三件套 找指标→绑数据)时 MR 内容重叠。请一起设计:① RunIssue 甩后台 detached(根因见本节 + 步4);② 每个 run 用独立 git worktree 隔离;③ 三件套串行依赖要不要在状态机/调度层强制——待实践定。codehub PR 回流(create_mr/merge_mr/Adopted)已在 `0c70775` 修好,不用重做;只看 run 调度层。

### 4.4 bug② 联网墙(指回步4)
- 真根因 = GLM 网关不支持内置 web 工具 + glm-5.2 不老实降级、烧预算。修法待定(诚实降级 prompt 硬停止 / 接 web-access skill / 换真 Anthropic 端点)。`bypassPermissions` 也不让内置 web 工具在 GLM 下变可用(只改权限层)。本环境真能联网的是 `web-access` skill(CDP 浏览器自动化,纯本地,gateway-agnostic)——agent 要真联网检索得接它。**权限没修前别重跑竞品分析**——会冻死 + 烧预算空转。要复现走 headless example。绕法(暂不实施):用「项目集自身技能」建不联网版竞品分析(人喂对标材料 + agent 整理,只用 Read/Write/Bash)→ 建新 issue 绑它跑(不能改已 seed 的竞品分析卡,见 §4.5)。

### 4.5 issue 技能绑死·无 UpdateIssue 命令(指回 §3 issues)
- issue `standard_skill` 在 `CreateIssue` 时一次性写入,之后无命令改(Command 枚举只有 Create/Transition/Assign/Block/MergeIssuePr/Refresh,无 Update/SetIssueSkill)。seed 出的三件套卡技能绑死后改不了,想换技能只能建新卡。**决议:先不动,记 GAP**(和「issue 卡是 buddy 工作单元」命题相关,别轻改)。

### 4.6 issue 看板要不要从仓库取(指回 §3 issues)
- by design:看板只列 buddy 自建/管的 issue,不导入 codehub 原有 issue(只进指标)。用户期望「看板从仓库 issue 列表取」,实际不是镜像。**决议:先不动**,要不要把仓库原有 issue 拉进看板待实践想明白。

### 4.7 连接器动作位置(hub-Op 边界)+ probe-at-creation(指回 §3 connector)
- 连接器是项目级数据,但管理动作(同步)在 Hub 不在 Op——用户在 Op 找不到同步按钮。plan/09 墙B「Hub 全局视图/复制共享等全量归属反转不在本次」。**待决定**:要不要把本项目连接器同步动作挪到 Op(项目闭环),Hub 只留全局库浏览。
- probe-at-creation 已做(CompleteCreation 末尾 `probe_connector` 建完即健康);Hub「立即同步」留作手动刷新。

### 4.8 skill/agent 渠道6 规范 + 归属反转(指回 §3 skill/agent)
- **渠道6 已实现 `5583381`**(种A 登记可见不注入,规范见 §3 skill)。**归属反转仍未做**:list_skills/list_agents 不收窄、Hub 无项目筛、agent 战绩跨项目混算。plan/09 墙B「不在本次」,等后续 step。渠道6 不依赖归属反转(project_id 列已在、project_rail 按 project_id 过滤、is_external_official 自动外库),已独立做。

### 4.9 cron / workflow / connector(运行体系)定位 gap 搁置(指回 §3)
- 和 skill/agent(方法文档,登记可见即完事)不同,这三者是**运行体系**(要 buddy 执行/调度),和 buddy 的 `cron_task`/`workflow_spec`/`connector` 表是两套执行模型。maas 的 `cron-registry.yaml`/`connectors/<system>_client.*`/`data-sources/*.yaml` 对不上 buddy 模型,**不硬塞**。**先打结搁置**:等 skill/agent 项目级登记跑通 + 归属反转线理清,再决定接不接(可能根本不接——maas 运行体系留给 maas,buddy 只看它跑得好不好)。

### 4.10 auto-mint「失败就停」需持久化标志位(指回步2)
- clone 失败时 buddy 悄悄本地 mint 一个空 workspace(像接上了)——对齐 github 行为(Existing clone 失败 CompleteCreation 也 auto-mint 空项目),clone SSH 修好后 workspace 非空不触发。真要做到「远端失败就停、不假接上」,需持久化「尝试过远端」标志位(当前无)。**决议:先不动**,记 TODO——低频(clone 修好后不中),等撞到再说。

### 4.11 滞后指标 UI 渲染 GAP(指回步6 / §3.2 metrics)
- trio 指标(北极星+滞后+引领)是项目级(`stage_kind=NULL`,不绑阶段)。`SyncMetricsFile` 装进 metric 表了,但 UI 渲染有缝:
  - **北极星**:在顶栏 TopBar(`op.rs:104`,不在进度面板正文,小字最右易漏看)。
  - **引领**:ProgressAll「本周计划」段显(`week_plan` 单独拉引领,不分 stage)。
  - **滞后**:**没有任何渲染路径**——ProgressStage 只显 `stage_kind==Some(阶段)` 的(`kernel.rs:970` 过滤掉 NULL),ProgressAll 没滞后段。→ 项目级滞后指标装了也看不见,只能 `sqlite3` 查。
- **决议:先不修**(用户 2026-07-31 说先不急)。修法方向:给 ProgressAll 加「滞后指标」段渲染项目级 `role=lagging` MetricCard;或 SyncMetricsFile 给 trio 指标设 stage_kind(但 trio 是项目级不绑阶段,语义不对)。待实践想明白。

### 4.12 plan18 step3 收尾·代码侧已交付 + 未决(指回步5/6/7)

> worktree `plan18-step3-metric-loop`(分支同名),9 commit,门禁全过。2 轮 SubAgent 自检:7 铁律全合规、代码可作底座。

**已交付**:
- 18-① 找指标/绑数据 skill 调:Step1 读项目既有指标体系(governance/derive_*.py)优先对齐不另起炉灶 + script kind(项目侧自采不降级 manual)
- 18-③ 通用脚本 connector(kind=script):probe 文件在位 + collect arm shell-out 项目仓采集脚本→读 JSON→按字段路径取值写 observation。可复制给同事项目
- 18-④ L6 上卷补缝:recompute 把项目级 metric(stage_kind=NULL)卷入项目健康灯(北极星 Green 拉亮项目灯,补缝符合"北极星驱动健康"哲学,非原代码设计)
- 18-⑤ UI 项目级业务指标区段:ProgressAll 显 stage_kind=NULL 的 metric(§4.11 滞后渲染 GAP 解)
- 18-⑦ SyncMetricsFile 按钮:改完 metrics.toml 点按钮同步进表(不必走 PR/命令)
- 18-⑧ 创建流 C/E 部分修:GitHub 建仓失败不兜底本地 mint(缺口E修);CompleteCreation 兜底条件改但**无效**(见下)
- 18-⑩ script 来源徽 + sample 示例(检视补)
- 审反馈修复:SKILL.md query 格式对齐代码(原 `script:;field:` 代码不认会全 deferred)+ 脚本非零退出 stderr 入错因 + 拒绝绝对路径

**未决**(留主窗口/§4):
- 🔴 **北极星 metric 行缝(方案A,留主窗口)**:北极星在 buddy 原设计存 `project.north_star` 列(**非 metric 表行**),collect/L6 都从 metric 表读→正规路径北极星不采集/不上卷/项目灯亮不了。SubAgent task2 验证 SQL 直插北极星 metric 行绕过通了,但正规 SyncMetricsFile 不建北极星 metric 行。修法 A:SyncMetricsFile 给北极星建 metric 行(role=north_star)+ `NorthStarDef` 加 target 字段(metrics.toml 格式契约改)+ EditNorthStar 同步 + UI 两套。**卡点不是逻辑难,是北极星位置迁移(project 列→metric 行)+ 补 target + 界面两套同步**——引领/滞后一开始就是 metric 行所以没卡,北极星位置不同。留主窗口和 plan17 汇总稳做。核心闭环"北极星点亮项目灯"最后一环。
- 创建流缺口 C 没完全修:CompleteCreation 兜底条件 `&& remote_path.is_empty()` 无效(`set_remote` 只成功调,失败 remote_path 本就空→本地 mint 兜底仍触发)。边缘(挂远端失败才触发),正常对接不撞,按"没问题的不强行修"标 §4 留撞到再修。
- task6 墙移植不开发(plan§1.4 vs memory 2026-07-28"项目健康总览先不补"冲突,没对齐不开发);层 B 来源徽未做(补充暂不开发)。**待什么条件回头**:哪天重判"项目健康总览要做"(推翻 07-28 memory 决议)+ 层B徽要值做时补
- task7 UpdateWeekPlan 接 UI 未做(plan 内但非核心——改周计划非改指标值/采集)。**待什么条件回头**:下一会话续或主窗口汇总时做(dioxus inline edit 细活,token 不够稳做时别赶)
- maas 侧:`clouddragon_cache.json` 被 cron 跑空(采集产零,maas 侧重跑 refresh_data.py 恢复)+ `derive_leading.py`/`data.json` 改(adoption_rate+扁平镜像)未 commit maas 仓
- script connector:300s 超时硬编码 / probe 不查 command 可用 / 多 connector 字段重叠语义(非阻塞标后续)

**task2 管线验证**(SubAgent):maas 脚本加 adoption_rate+扁平镜像(因 buddy `json_field_by_path` 只走点分对象键不数数组)→worktree 编译 buddy OK→SQL 建 script connector+4 metric 行(北极星/L1/L2/L3 collect_kind=script)→深链渲染 `[BW_OPEN]` 无 panic→采集点亮留主窗口(GUI CollectMetrics)。**SQL 直改 DB 绕法非终态**,终态走 metrics.toml+SyncMetricsFile 按钮。

**【2026-08-03 合体落定 + maas 回显验证】**:plan17(5 commit)+ plan18(11 commit)合体进整合分支 `merge-run-scheduling-step3-metric-loop`,git auto-merge(ort)**零冲突**(两边唯一重叠 `bw-app/src/lib.rs` use-import 块,对 `PathBuf→{Path,PathBuf}` 做了完全相同编辑被自动接受;plan18 加 `CONNECTOR_KIND_SCRIPT`/plan17 加 `mpsc`/`JoinHandle` 在不同子区;`app-desktop/screens/op.rs` 零重叠——plan17 改 `IssuesPanel`/plan18 改 `ProgressAll`+`MetricCard`;PRACTICE 仅 plan18 碰,plan17 未碰、main 的 +9 归正草稿已回退)。全门禁 6/6 绿(fmt/clippy -D warnings/wasm32 bw-core/wasm32 ui/guard-kernel-ui-free/check app-desktop)。SubAgent 代码检视「可提交」(合体接缝干净:import 两边齐无重复、plan17 run 路径与 plan18 probe/collect 不互引用、op.rs 两函数共字段无错位;四铁律全守——Done 入边仅 InReview、cancel 留 InProgress+settled_at 空、Signal derive-only、settle-once `take()` 防护、UI 无关内核、schema 不加列、cron 不碰;设计决策保全——无硬编项目名、两层分层、skill 读项目体系、创建流 C/E 失败就停、北极星方案A 无半截实现、cancel 不走 finalize 诚实留痕)。

**maas 回显验证**(深链 `BW_OPEN=maas-locate BW_PANEL=progress`,DB=`…/workbench.db`):stderr `[BW_OPEN] "maas-locate" -> view=App panel=Progress projects=1 issues=3` + **无 panic**;启动 recompute_signals 整链跑通(`signal_derived_rev` 从基线累到 40),含 plan18-④ L6 `UPDATE project SET signal` 上卷路径,**无崩**。sqlite 读回真实态(纠正第三窗口不准):
- **17 metric 全 unknown**(非第三窗口报的"13"——17 = plan18 task2 SQL 直插的 4 script metric + 既有 13)。
- observation codehub 8 + telemetry 4 = **12**(非"11/9+1/2"——那是 raw observation 值被误当计数)。
- 3 connector(含 plan18-③ kind=script「maas·指标脚本」connector,config 指 `governance/workspace/clouddragon/derive_leading.py`→`data.json`,SubAgent task2 SQL 直插绕法建,非正规 SyncMetricsFile 路径)。
- 3 issue:**#1 竞品分析 in_progress、#2 找指标 done、#3 绑数据 done**(非第三窗口说的 #31/#32/#33——那是 codehub iid,DB issue id 是 #1/2/3)。
- project.signal=unknown **诚实**(codehub 指标有观测无 target 阈值→unknown;script 指标无观测→unknown;未造假绿,铁律守)。
- plan18-⑤「项目级业务指标」区段 + ⑦ SyncMetricsFile 按钮:**代码级确认渲染**——`op.rs:1771` `if op.metrics.iter().any(|m| m.stage_kind.is_none())` 条件成立(maas 有 4 个 stage_kind=NULL metric:L1/L2/L3/北极星)→区段 + ⑦ 按钮(`op.rs:1787` `k_sync.send(Command::SyncMetricsFile)` "↻ 同步指标文件",在⑤ 区段头)随区段渲染。截图(Powershell `CopyFromScreen` + `claude -p --model haiku` 读图)只抓到 Progress 面板**可视顶部**(健康概览/本周复盘/项目信息/真执行工作目录/总进度+底部计数卡),区段在折叠下方;haiku 读图 180s+ 触发 Windows 自动锁屏致后续全抓锁屏壁纸、SendKeys 滚 buddy webview 发错窗口——**下方折叠区视觉肉眼未核(环境限制非 buddy bug)**,留用户解锁后滚到底核一眼(非阻塞)。正面:连接器导航可见「maas·指标脚本」(plan18-③ script connector 注册可见);haiku 读到「本周未记指标:17 个」与 DB 吻合。

**北极星方案A 维持 deferred**:本次合体**不做**方案A(§4.12 已标)。正规路径北极星不采集/不上卷/项目灯不亮属已知 gap,留主窗口稳做。

### 4.13 V3 两篇方案已记、未落地(指回 §3 issues / 一张工作台)

> 2026-08-14 会话:Open Design 内嵌成立之后,讨论 Cursor CLI 与 cowelink。**只落设计,不改代码。**

- **Issue 用 Cursor 还是 Claude**:今天没地方配。开工写死 `claude`;Hub 智能体「执行引擎」只读。方案与配置面见 [`docs/v3-prototype/cursor-agent-executor.md`](../docs/v3-prototype/cursor-agent-executor.md)(代号 V3-cursor-cli)。
- **cowelink**:不弹窗。要嵌,先让 cowelink 长出本机网页旁路,buddy 再 iframe。见 [`docs/v3-prototype/cowelink-web-sidecar.md`](../docs/v3-prototype/cowelink-web-sidecar.md)(代号 V3-cowelink-sidecar)。第一张穿刺打在 cowelink 仓。
- **orca 整窗**:不做。多会话已在 Issue 终端里。
- **待什么条件回头**:用户点头落地、并先提 issue。Cursor 还要本机 `agent login` 后穿一张「`AGENTS.md` 是否真注入」。

### 4.14 第一包 / 开发包 + 删阶段记录缺列(2026-08-14)

> 卡住的开发窗口半套落地后，本窗收口。安装器在仓外 `D:\2026\buddy-setup`，不进 loop-buddy。

- **删阶段记录**：`delete_session` 仍 `UPDATE issue SET session_id=NULL`，列已 DROP → `no such column: session_id`。点卡能醒是因为走 `claude_conversation`。**已改**：只删 `message` + `session`，不碰 issue / conversation。本机库旧 SQL 复现失败；新 SQL 删 throwaway 行后 issue 行数不变。
- **第一包** `BuildersWorkbench-Setup.exe`：结束页正文改写进 iss（不再 `LoadFromFile` 读无 BOM UTF-8）；exe 编成 Windows 子系统（PE subsystem=2），带 `WebView2Loader.dll`。
- **开发包** `BuildersWorkbench-Dev-Setup.exe`：第一包超集 + MinGW zip + sqlite3 + 装完脚本（Rust 走 rsproxy 现拉 `rustup-init`，clone v3）。`rustup-init.exe` 进不了 payload（本机安全软件隔离）。
- **首次点跑卡住**：不重做 V1 调度。结论见当次对话——更像安装器 `cmd start` + 当时那颗 CUI exe 抢焦点/首开 PTY，不是 settle_tx 丢了。
- **待测试窗验**：结束页能读、立即运行无黑框、项目墙「测一下」未测为灰。
- **出包脚本不进仓（2026-08-14 后补）**：`D:\2026\buddy-setup` 不建独立仓、也不拷进 loop-buddy。打包内部用，都从这台机器出包。脚本和产物都留本机。
- **release 闪 cmd + 唤醒 spawn 报错（2026-08-14）**：不是 V3 产品功能把版本做坏了。出包时为藏主窗口黑框给 release 加了 `windows_subsystem=windows`；子进程没 `CREATE_NO_WINDOW` 就闪 cmd。**已改** `win_cmd` 藏窗。
- **唤醒 `environment variable name must not contain =`（2026-08-14 后补）**：新报错钉死不是找不到 `claude.exe`、也不是工作目录先丢了。ConPTY 把整份进程环境再 `env()` 一遍，windows-spawn 校验名字；Windows 隐藏项（`=C:` / `=ExitCode`，安装器 `cmd start` 的 GUI 父进程里常见）带 `=` 被拒。失败后 `IssueWorktreeGuard` 会拆掉本次 worktree，所以目录随后看起来「没了」。**已改**：子进程继承环境，只摘掉嵌套执行变量，不再整表重放。要新编 release / 重出包再验唤醒。

### 4.15 采集仍跑旧脚本·合入是否更新主目录(2026-08-14)

> 同事反馈:找指标脚本在 worktree 里优化完,界面点采集仍跑旧的。本窗只查代码、不改产品。指回步7 / §3.2 metrics。

- **问1 · merge 会不会更新主目录**:会,这是设计。点 merge(或网页合完再点「→已完成」)走 Done 口,对主工作区 `sync_default_branch`(fetch + checkout 默认分支 + `pull --ff-only`),再装 toml、立刻采一轮。不是把 worktree 目录拷进主目录,是远端合入后再拉回主目录。
- **问2 · 采集按钮要不要默认先更新主目录**:现状**不拉**。`CollectMetrics` 只在主工作区跑已有脚本,不看 worktree。**当前决议:不加**。采集是记观测,更新主目录是验收(merge/Done)的事;没合入的优化本来就不该被采集看见。采集里自动 checkout/pull 也碰「破坏性永不自动」。
- **同事这条更像哪条**:worktree 里改完直接点采集(主目录还是旧的,预期如此);或网页合了但没在 buddy 点 merge/「→已完成」(Done 口没跑,主目录没拉);或 Done 口跑了但 `pull --ff-only` 失败被吞(`github.rs` `let _ = pull`,仍 Ok)——主目录其实没跟上,界面却当收拢成功。
- **旁路**:每次采集会覆盖 `.bw/collect_stats.{sh,py}`(buddy 自带仓统计,注释写明勿手改)。优化若写在这两份文件上,点采集会被 Buddy 内置稿盖回去。项目业务脚本(`derive_*.py` 等)不受这条影响。
- **待什么条件回头**:先对同事那次复盘是「没走 Done」还是「pull 被吞」。若要改,优先让收拢失败诚实报错,不把 pull 塞进采集按钮。动手前提 issue。
- **本机取证(2026-08-14)**:用户本地 `welink-bridge` 主工作区(`…/workspaces/welink-bridge-e25bd532`)在 `dev`,fetch 后**落后 origin/dev 8 个提交**。同事合入的 `bw/metrics-rewrite`(改 `.bw/scripts/derive_shield.py` + `.bw/metrics.toml`)已在远端,本地没拉。工作区脏文件只有 buddy 每次采集会覆盖的 `.bw/collect_stats.{py,sh}`。还原这两份后快进拉到 `eedbbad`。这就是「点采集跑旧脚本」在本机的实锤:不是采集按钮漏了更新,是主目录没跟上远端。Buddy 库里的指标定义还要再点「↻ 同步指标文件」才会换新口径,然后「立即采集」才按新脚本出数。
- **机制怎么补(2026-08-14 傍晚,用户已看到新数后问)**:现有四个「↻」各管一层,不要塞进「从仓同步 Issue」。**当前决议(未落地,动手前提 issue)**:加独立「↻ 收拢工作区」(复用已有 `sync_default_branch`),成功后再跑 project/metrics/connectors 三份正本进库;**不**自动采集、**不**改看板 Issue。`pull` 失败必须诚实报错,不许再吞。采集 / 同步 Issue / 同步指标文件职责不变。这是后来者读回仓里共同事实的缺口,不是把采集变成 `git pull`。

### 4.16 V3 使用问题(2026-08-17)

> 实践里撞到。指回步2 创建流 / 第一包安装器。设计见 [`docs/v3-prototype/onboard-list-and-claude-resolve.md`](../docs/v3-prototype/onboard-list-and-claude-resolve.md)(代号 V3-use-fix)。动手前提 issue。

- **安装器只认一条死路径 `bin\claude.exe`**(指回 §1 前置 / 第一包):Inno 脚本原先只查 `%APPDATA%\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe`。终端里 `claude` 能用(走 `%APPDATA%\npm\claude.cmd`)也不算。同事有主包目录、没有 `bin`,安装包直接中止。`bin\claude.exe` 是 npm `postinstall` 从可选包拷过来的,不是解压自带。
  - **没有 exe,Issue 还能跑吗**:终端能跑 ≠ buddy 能开工。V3 开工是 ConPTY,`CreateProcess` 直接打 `.cmd` 会不是合法 Win32 映像。只放行安装、不包 `cmd.exe /c`,点跑仍失败。
  - **【2026-08-17 归正·本窗落地】**:安装器认 exe **或** `%APPDATA%\npm\claude.cmd`,`BW_CLAUDE_BIN` 写实际找到的那条(优先 exe)。应用启动同一顺序解析;路径以 `.cmd`/`.bat` 结尾时 PTY 与 `tokio_cmd` 都走 `cmd.exe /c`。仓外脚本 `D:\2026\buddy-setup\BuildersWorkbench.iss` + Dev 包同步改。要新编 release / 重出包再生效。
- **绿区「↻ 刷新列表」截前 30 条,成员仓也会丢**(指回步2):buddy 原先调 `codehub-cli -H green project list --mine --limit 30`。`--mine` 与 `--membership` 本机对照都是 79 个(我是成员就算,含组继承;不是「必须我拥有」——`--owned` 只有 2 个)。`aipdu/oh-my-hw-claudecode` 我是组继承 Developer(`access_level=30`),排在 `--mine` 第 **74** 名,被 30 截掉。界面文案「仓不在列表=需先成为 member」不完整。默认排序不是最近活跃(该仓 8/13 还在动)。
  - **【2026-08-17 归正·本窗落地】**:limit 改为 200(codehub/github 对称);下拉上方加搜索,只过滤已加载列表(path/描述/默认分支)。文案改成「已加载 N 个(最多 200);仍没有 = 不是 member,或排在 200 以外」。不恢复手填 path。远端按最近活跃重排、200 以外翻页——未做。
  - **【2026-08-17 归正·拉与画拆开】**:200 仍偏少(人仓数常 >200);搜索补不上没拉下来的。改成**拉 999 / 下拉只画 30**(当前选中始终留在列表)。搜索仍只过滤已加载。999 以外翻页/远端搜索——未做。
  - **【2026-08-17 归正·可搜索下拉 + 图标】**:装包后反馈两件。① 搜索框和原生 `<select>` 是两套控件,打字不能直接点出匹配项——改成一个可搜索下拉(聚焦/打字弹出最多 30 条,点选)。② 程序仍无图标——补几何标记(clay 方砖 + 纸色环 + 小台面),贴 exe / 窗口 / 安装器。要新编 release / 重出包才看见。
- **项目墙「测一下」只打一区**(指回步1):探针写死 `codehub-cli -H open project list`(内源)。黄区(或只登了绿区)token 正常,测一下仍红。文案还写「先 `-H open auth login`」。
  - **【2026-08-17 归正】**:改读 `codehub-cli auth status`(一次列出绿/内源/黄)。任一区 LOGIN=yes 即过,墙上写「已登录 黄区」这类;三区都没有才红,提示 `-H green|open|yellow`。不装绿、不猜默认区。
- **GitHub 下拉只列自己名下的仓**(指回步2,2026-08-17):`gh repo list` 不列 collaborator / org 仓。codehub `--mine` 已含成员仓(组继承也算)。用户问要不要对称成「我参与的」。**当前未做**。改法小:`list_repos` 改 `gh api user/repos?affiliation=owner,collaborator,organization_member`,JSON 字段换一套,两个 verify stub 跟着改。不列只 star / 只看过的仓。「我提交过但不是 member」是另一条、更大。动手前提 issue。
- **纳入时选分支 / 一仓两项目**(指回步2,2026-08-17):有人一个仓两条分支、每条分支当一个项目,希望纳入时能选分支。**当前决议:不接这个产品形态,本轮不动。** buddy 的项目身份是「一个仓 + 一条主干」;产品信息正本(`.bw/project.toml` / 指标)住在仓里,clone / 开工出支 / 合入收拢都按远端默认分支。选分支看起来只是下拉多一项,后面 MR 目标、收拢主干、后来者读正本都得变成「每个 buddy 项目自带一条主干」,等于承认「分支 = 项目」。那是仓治理问题,不是纳入缺口。对方若真是两个项目,应拆成两个仓(或两个 path)再分别纳入。
- **一个项目、多条在跑的版本线**(指回总览 / 版本面板 / 步2,2026-08-17):同一产品同时维护 `main`(2.0)和 `release/1.x`(补丁)是真需求,和「一仓两项目」不是一件事。但「哪条线」会渗进几乎所有控制点,本轮**整条不开发**,另开设计再做。摊开如下。
  - **今天总览看的是谁(诚实)**:进度/健康灯/采集,用的是主工作区当前检出上采上来的数,通常就是远端默认主干。版本面板只是这份工作区的 `git log`,不是版本线。没有「看哪个版本」的切换,也就**不假装**在看全部版本或某一指定版本。
  - **若做成一等能力,至少这些面都要回答「哪条线」**:总览那盏灯(1.x 绿、2.0 红混成一盏=造假);北极星/指标正本(两条线口径可能不同);采集与定时;Issue 从哪条拉出、MR 合回哪条;五阶段/交棒(1.x 已在运维、2.0 还在构建?);版本面板(这才是版本线该住的地方);产物归属;纳入时 clone/探测正本。
  - **现在明确不做**:纳入选分支;总览版本切换;Issue 挂目标版本线;采集/信号按线切开;阶段环按线复制;整仓来回 checkout;把版本面板从提交列表改成版本线管理。第一刀若回头做,应先设计「总览一盏灯怎么诚实」,不是纳入下拉多一项。

### 待记(后续会话补)
- _待补:步3 agent 真跑——bug① 修好后竞品分析能不能真联网出报告 + 产出 PR?_
- _待补:推广给别人时,别人的前置装/配跟我的差异。_
- _待补:第一包/开发包装完由测试窗对照验收。_

---

## 5. 认知(buddy 是什么、能带来什么)

### 两个面(buddy = 看板 + AI 小队)
- **看板层**(呈现):项目墙 + Op 屏(进度/issue/版本)+ 过程件(工作流/定时/产物)+ 按阶段指标 + 9-Hub。**看得见**:项目什么状态、缺啥。
- **AI 小队层**(执行):五阶段每段一帮 agent 处理 issue,autopilot 到点自动**建**活(永不自动**完成**)。**干得动**:agent 真在仓里改文件跑测试。
- 底座:codehub/github 仓(产品信息正本)+ SQLite store(过程信息 append-only)+ `.bw/metrics.toml`(指标定义机读文件)。

### 四个铁律(防蔓延,不假装)
1. **管理体系自带**(不靠用户发明):五阶段(原型→构建→优化→运营推广→运维)每段自带打法,运维回流原型(环不是流水线),阶段推进=交棒(清单未勾可交但标「带险」留痕)。
2. **活让 agent 干,人守验收**:Issue 卡指派 AI,`RunIssue` 真开工;干完只到 `InReview`,**「完成」永远人显式点**;**Done 永不自动、破坏性永不自动**。
3. **健康难造假**:Signal 只能经封口 `Derived<Signal>` 进缓存(store 无 set_signal),无数据=Unknown≠绿;观测 append-only(一个观测=一个点,绝不插值);settle-once(同件活绝不记两次);任何界面数字能 `sqlite3` 独立查证。
4. **经验复利**:做完的 Issue 蒸馏成技能(记着来自哪件活),下次同类活自动注入、用一次记一次;队友胜率由真实战绩派生。

### codehub 对接设计(步1 落地的认知)
- **接口层 `Remote` enum + 工厂**:provider 分叉收敛在工厂一处(github→`Remote::Github(path)`、codehub→`Remote::Codehub{host,path}`),call-site 不各自长 match 臂,加 provider 漏改编译期报错。对标 Java `interface + 2 impl + 工厂`。
- **远端身份 `(host, path)` 均匀**:不是「github 不需要 host」,是当时把 `github.com` 隐式默认漏存一列。github 存 path+host=github.com、codehub 存 path+域名。
- **codehub.rs 走 shell-out `codehub-cli`**(不直调 GitLab v4 HTTP API):CLI 是 v4 同构封装、默认 JSON、token 在 keyring、零 Rust HTTP 依赖,与 `github.rs` shell-out `gh` 对称。clone 用 raw `git clone ssh://`(SSH 不经代理、不要 token,常规)。
- **API 活(probe/issue/mr)必须 codehub-cli**(GitLab v4 API+token,raw git 干不了);clone 是 git 活,raw git 即可,codehub-cli 的 clone 包装对 SSH 纯透传无增益故绕过。

### 连接器同步背后(git-repo vs codehub/github-repo vs collect arm)
- **代码仓(git-repo)同步**:`SyncConnector`→`probe_connector` 探工作区 git → 收 `WorkspaceEvidence`(commit_count/tracked_files/docs_files/dirty_paths)→ 翻已连接 + `feed_workspace_metrics` 把「工作区真实提交数」=commits、「剧本产物文档数」=docs 写进 observation(source=Connector,无「手填」徽)→ recompute signal。实测:39 提交 / 106 追踪文件。UI:连接器卡→已连接;Op 屏「工作区真实提交数」卡点亮(=39,真数据)。
- **CodeHub/GitHub(codehub/github-repo)同步**:探 `codehub-cli project view`/`gh repo view`,只翻状态、**不喂指标**(指标走 cron collect arm 的 collect_count)。
- 一句话:代码仓同步喂工作区指标、CodeHub 同步只探活(指标由 cron 采);三条管线不混——probe 探活 / collect 采计数 / sync_project_assets 扫 skill(见 §3 skill 渠道6)。

### skill / agent 形态归宿(2026-07-31 钉死)
- **三档**:全局共享进 buddy 仓(bw-standard 编译进二进制 / 五角色 agent 全局单例);项目级进**项目仓**(`skills/<slug>/SKILL.md` 文件夹、`agents/<name>.md` 单文件),git 即跨人共享正本;DB 只是运行时副本,导入(4/5)是本地一次性 copy 不构成共享。
- **种A 登记可见不注入**:项目自带 skill/agent 扫进来只登记可见,不进任何注入下拉(issue standard_skill / assignee / workflow crew / cron RunSkill)。执行/战绩归属是归属反转半破口,先不碰;项目级可注入留归属反转后。
- **种B(运行资产 vs 维护资产、维护类可注入)留未来想法**:maas 的 auto-ticket(运行资产)/ refresh-indicators(维护资产)当前都按种A 登记不注入,不预设区分。等归属反转线理清再回头。
- **规范未拉通前两边都不改**:扫到能解析的 SKILL.md(name+desc)就如实呈现字段,缺的按 buddy 诚实空态;规范不符出 Advisory 灰提示不阻断。未来拉通两边规范再收紧。

### plan18 step3 收尾认知(2026-08-03)

- **指标两层分层**(用户定):层 A 业务指标(北极星/引领/滞后,项目级 `stage_kind=NULL`,用项目真实定义)+ 层 B buddy 固有项目管理指标(开放 Issue/已合入 MR/阶段完成,通用,只当现状数不进健康灯)。buddy 固有指标不混进业务卡。
- **通用脚本 connector**(plan18-③):buddy 加 `kind=script` connector,shell-out 项目仓既有采集脚本→读输出 JSON→按字段路径取值。可复制给同事项目(任何有产出指标值脚本的项目能接)。**buddy 不为某项目加功能**(用户哲学:maas 采纳率走 (a) maas 脚本自己加 adoption_rate 字段,buddy 通用采,不特化)。脚本自身依赖(Playwright/SSO)项目侧管。
- **L6 上卷补缝**(plan18-④):北极星驱动项目健康是 buddy 原产品哲学(目标清晰且难造假),但代码当前 L6 只卷阶段 metric、项目级不上卷是缝→补 `by_project` 卷入。非原代码设计,补缝符合哲学,不替它圆场说"原来就这样"。
- **北极星位置差异**(plan18-⑨ 留主窗口):北极星在 buddy 原设计存 `project` 列(项目唯一顶层目标单独存),非 metric 表行。引领/滞后一开始就是 metric 表行→采集/上卷天然通;北极星位置不同→要迁移 + 补 target + 界面两套。这是我 plan §1.2 写"北极星给 metric 行"时低估原设计位置差异的偏差,诚实记。
- **buddy 通用采要项目脚本输出扁平 JSON**:`json_field_by_path` 只走点分对象键不数数组,maas 原 data.json 的 `leading_indicators` 是对象数组取不到,补扁平镜像 `leading.{L1,L2,L3}`。印证"项目侧脚本按 buddy 通用契约适配"——buddy 保持通用,项目侧保证输出扁平可寻址。
- **skill 不读项目既有体系是指标对不上根因**:三件套 agent 造指标和 maas 真实对不上(phantom/误标 manual/漏造),不是 agent 跑错,是 skill Step1 不读项目仓 governance/derive_*.py。调 skill 让 agent 读项目体系优先对齐,可复制。

### run 调度层认知(2026-08-03,plan17 S1-S5 落地钉)

> bug① 冻死 + ④ worktree 隔离在 plan17 落地(§4.3 归正),机理记这里。

- **单线程 `current_thread` runtime 能跑后台 spawn**:`tokio::spawn` 在 `current_thread` 上不另起线程,排同线程队列,在主 `block_on` 的 await 点(`select!` 的 `cmd_rx.recv()`/`settle_rx.recv()`/`ticker.tick()`)交错 poll 推进,无锁无并发。① 解冻的机理——run 甩后台不冻 UI,靠的就是 spawn future 与主循环在同线程 await 点交错推进,不是另起线程。
- **回灌靠 mpsc 不靠 await JoinHandle**:后台 spawn 完成经 `mpsc::UnboundedSender<SettleReq>` 发回,主循环 `select!` 加第三臂收;JoinHandle 只用来 `abort`;双 settle 防护靠 `Option::take()` active_run 先到先做。
- **abort 不发 settle,自走 failed 尾**:`JoinHandle::abort()` drop spawn future → `kill_on_drop(true)` 杀 claude 子进程,无泄漏;abort 后 spawn 不发 SettleReq,`CancelRun` 自造 failed outcome 跑尾,issue 留 InProgress、不自动 Done(铁律守)。
- **worktree guard 跨 spawn→settle 边界**:`IssueWorktreeGuard` 移进 `ActiveRun` 持有,在 `run_issue_settle` 里 `finalize_run` 读 `head_after` 之后、`issue_run_tail` 的 `create_mr`(`stage_commit_push`)之后才 Drop 拆 worktree——保证产物登记/MR 创建读得到 worktree 内容。
- **`settle_tx: Option` 的边界收口**:examples/headless 调 `dispatch(RunIssue).await` 期望同步跑完,不接 settle_tx(default None)→ `run_issue_now` 走 inline `run_issue_body` 字节级不变;只有桌面 kernel `with_settle_channel` 接上才 background,S3 blast radius 收桌面一处。

### V3 一张工作台 + 执行器预研(2026-08-14)

- **一张工作台**:Builders 只在 buddy 里干活。Open Design 能嵌,是因为它有本机网页口。cowelink 今天没有,外开窗口不对;正确补法是 cowelink 自己长出网页旁路(与 Open Design 同构)。orca 整窗不嵌——多会话 Issue 终端已经做了。
- **执行器是本机事实**:Issue 开工今天写死 claude,界面没有「换 Cursor」的开关。落地时应是设置里的本机默认 + 智能体卡「执行引擎」,不在单张 Issue 上再选。Cursor 路径走 `agent` 不是 `cursor.exe`,系统提示词走工作区 `AGENTS.md`,花费封顶第一版没有。

### 实践收口的一句话价值(2026-08-14)

> 用户在 V1 / V2 / V3 实践后口述。不是新命题,是 [`plan/07`](../plan/07-product-proposition.md) 引子页给**传统开发者**听的压缩。原文四个控制点一个没改。

**一句话**:buddy 是 AI Coding 里、一个人的一张工作台——让传统开发者按 Builder 的方式干,用大约三个月把一个小项目从想法管到能验收的结果。

三根柱子:

1. **一张工作台**。设计(嵌 Open Design)、干活(Issue 里的 agent)、通信处理(cowelink 旁路,未落地)、指标与交棒,都在 buddy 里完成。不弹一堆工具窗。
2. **人从开发变成 Builder**。不是多一块看板。角色从「自己写代码」换成「1 个 Builder + Agent Loop」:人守对标、每周能否从真数据看演进、验收门、北极星;活让 agent 干,完成永远人点。
3. **大约三个月的小项目**。命题原文就是「每周可验证增量、≤90 天视野」。一站式管完一个小项目,走完留下可复制的方法,不只是卡片。不是大厂十道流程、五个专职角色;也不是十二个月多人项目的协作平台。

「好的结果」不靠感觉,仍看四个控制点:知道对标谁 / 每周在正常演进 / agent 真干活、人只守门 / 目标清晰且难造假。

### 反命题(buddy 不是什么)
- 不是团队协作平台(无成员/群聊/收件箱)。
- 不是通用看板(无拖拽/甘特;回退不给 UI)。
- 不是审批系统(交棒只留痕不拦人)。
- 不是云服务(AI 执行=本机 CLI;今天是 `claude`,Cursor 路径设计已记未落地)。
- 永远不替用户捏造健康。
