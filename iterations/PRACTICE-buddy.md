# 践行日志 · buddy(我的使用 + 维护 + 认知演进)

> 一份**活日志**,每个会话更新。不是开发文档(那是 `plan/` + 源码 + commit),是
> **我用 + 维护 buddy 的真实实践**:前置装/配啥、撞到啥问题改了啥、正式怎么用每步
> 背后干了啥能看到啥、对 buddy 越用越成熟的认知。目标:自己用顺了 → 能推广给更多人。
>
> 每轮如实记:假设→动作→真实输出→结论。不改设计决定;偏差/新墙照实记,不擅自扩 scope。

---

## 0. 这份日志怎么用

- 每个会话(用 buddy 或维护 buddy)结束前,让 Claude 把本轮更新进这份文件:
  - 撞到的问题 + 根因 + 改了啥(进 §2)。
  - 正式用的步骤 + 背后动作 + 看到啥(进 §3)。
  - 对 buddy 的新认知(进 §5)。
- §1 前置随环境变化更新(装了新东西、配了新 env 就记)。
- §4 留给持续扩充(临时发现、待办想法)。

---

## 1. 前置:用 buddy 前要装/配啥

### 运行时必装必配(干活绕不开)

| 项 | 装啥/配啥 | 为啥 |
|---|---|---|
| **git** | 任何 clone/workspace 操作都要 | 通用 |
| **claude CLI** + **`BW_CLAUDE_BIN`** | AI 干活(issue 执行器)shell-out `claude -p`,要给全路径(Windows 上 Rust `Command::new("claude")` 不做 PATHEXT,只认 .exe/.cmd,见 §2) | `BW_CLAUDE_BIN=C:\Users\<你>\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe`(持久 User 环境变量) |
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
- `BW_CLAUDE_MAX_BUDGET_USD`(单次 agent 花费封顶)

### 启动

```bash
cargo run -p app-desktop   # 别直接跑 target/debug/builders-workbench.exe(Windows 崩 0xC0000135 无窗口)
```

---

## 2. 撞到的问题 + 改了啥

### 2026-07-28/29 · codehub 对接(步1)

- **clone HTTPS 504**:buddy `codehub::clone_repo` 走 `codehub-cli repo clone <path>` 重建 HTTPS → `git clone` HTTPS 经代理隧道 504。根因:codehub 是局域网,HTTPS 被拦。
  - **改**:`clone_repo` 改 SSH——`project view --template .ssh_url_to_repo` 取 SSH URL(SSH host `szv-open:2222` ≠ API host `open`,不手拼),raw `git clone` + `GIT_SSH_COMMAND` accept-new+BatchMode。scratch + 真实录双验证。
- **claude CLI spawn "program not found"**:`BW_CLAUDE_BIN` 没设 → 默认裸 `"claude"` → Windows Rust `Command::new` 不做 PATHEXT → 命中无扩展名 POSIX shim(os error 193)→ "program not found"。我 claude 能跑是 cmd/PS/git-bash 各靠 PATHEXT/shim 绕过了这条裸名路径,buddy 这条没绕。
  - **改**(不碰代码):设 `BW_CLAUDE_BIN=…\claude-code\bin\claude.exe`(持久 User env)。根因是 config gap(逃生口本就提供),不是代码 bug。自动探测(首跑找 claude)留后续,本轮不扩范围。
- **path 空也能提交**:UI path 输入框 placeholder 不是值,空着 IntentCard create 仍能点 → clone 空 repo 失败 + remote 空 + trio 不建。
  - **改**:IntentCard `can_send` 加 `platform==codehub 时 codehub_path 非空` 校验。
- **example 跨平台编不了**:5 个 example 用 `std::os::unix::PermissionsExt::from_mode`(unix-only),Windows 编不了,挡住跑 verify_c16 证迁移。
  - **改**:`make_executable` helper cfg-gate(unix +x / 非 unix no-op)。
- **auto-mint 行为**(clone 失败后悄悄本地 mint,像接上了):**决议对齐 github**——github Existing clone 失败在 CompleteCreation 也 auto-mint(`lib.rs` provision_workspace 对所有 workspace 空项目),codehub 同样即「对齐」无特例;clone SSH 修好后 workspace 非空 auto-mint 不触发。真「失败就停」需持久化「尝试过远端」标志位 → 长期 TODO。

### 环境坑(非 buddy 代码)

- **wasm32 target 没装** → commit 门禁 wasm 项红(预存,非代码)。修:`rustup target add wasm32-unknown-unknown`。
- **mingw 缺 as** → `cargo test`/example 链接 windows-sys 失败(预存)。修:装 WinLibs mingw。
- **rustup 官方源卡死**(华为内网) → 用清华镜像。
- **`codehub-cli --jq` 带引号、无 `-r`** → 用 `--template '{{.field}}'` 出裸串(写 codehub.rs 时踩的)。

### 2026-07-29 · 创建流按钮 gate 位置错(Q1,已修)

- **现象**:Repo 卡 codehub path 没填,「下一步」**能点**(过去了);到 Intent 填完名+brief,「确认·建立项目」反而**置灰**。用户想不出是上一步 path 没填。
- **根因**:RepoCard `can_send`(create.rs:283)`is_codehub && !codehub_path.empty || is_new || existing_ready` —— `||` 优先级漏:codehub 下 `is_new`(默认"新建")为真就盖过 path 要求,空 path 放行。IntentCard `can_send`(create.rs:557-559)写对了(codehub 必须 path 非空),所以挡在 Intent、且无提示。
- **已改(2026-07-29)**:RepoCard `can_send` 改 `if is_codehub { path 必填 } else { is_new||existing_ready }`,gate 放 Repo「下一步」源头;置灰时显示"codehub 仓库 path 未填(输入框要填值,不是 placeholder)"。过门禁(fmt/check app-desktop/guard 全绿),**已 live 验证 + 提交 `f66814b`**(用户重启重跑确认)。

### 2026-07-30 · 连接器 syncable 漏认(codehub/github 漏改)+ zombie exe 坑

- **syncable 漏认(已修+已验证)**:`connector_card`(ui/vm.rs:1373)`syncable` 只认 `git-repo`/`claude-cli`,**漏认 `github-repo`/`codehub-repo`** → 后两者被当"登记项·无真实探针"、无同步按钮、永远未连接。但 `probe_connector`(lib.rs:2277/2291)其实有 github/codehub 真探针(`gh repo view`/`codehub-cli project view`)。kind+探针加了、UI syncable 没跟上——codehub/github 特性漏改。**已修**:syncable match 补认两 kind。**已 live 验证**:CodeHub 连接器出「立即同步」按钮、点后探活翻已连接。
- **zombie exe 坑(env,非 buddy 代码)**:TaskStop 杀 `cargo run -p app-desktop` 父进程后,子 `builders-workbench.exe` **可能不死**(锁住 `target/debug/builders-workbench.exe`)→ 下次 `cargo run` rebuild 报 `failed to remove file ... 拒绝访问 (os error 5)`。修法:`taskkill //F //IM builders-workbench.exe` 显式杀 exe 再 rebuild。

---

## 3. 正式怎么用:每步操作后 buddy 干了什么 + 能看到啥

> 风格:**你做啥** → **buddy 系统做了啥可看见的事**(建文件 / 写 DB / 唤起 claude / 记过程)→ **你能看到啥**。不讲代码内部。
>
> 颗粒度要细:每步记到「在哪个卡填了哪个字段、点了哪个按钮、buddy 后台建了哪条 DB 行/哪个文件、看到啥」——太粗回头对照不上。

### 录一个 codehub 项目(以 maas 为例)

**步1·启动** `cargo run -p app-desktop` → 项目墙。
- buddy 干:起桌面壳,读 `workbench.db`,渲染项目墙。
- 看到:已有项目列表(空就是空,不假装)。

**步2·创建流 → Repo 卡**:平台选 CodeHub;host = `open.codehub.huawei.com`(内源;绿区改 `codehub-g.huawei.com`);path 手填 `innersource/AI-Coding_G/maas`(placeholder 不是值,不填下一步禁用);起点选「接入已有仓」。
- buddy 干(点「下一步」时):SSH clone 真 maas 仓进 `workspaces/maas-locate-<id>`(远程 `git clone ssh://`,不经代理、不要 token)。
- 看到:工作区目录真出现 maas 文件(`governance/`/`docs/`/`agents/`/`skills/`/`AGENTS.md`…),不是只有 `PROJECT.md` 的空仓。

**步3·Intent 卡**:填项目名(maas-locate)+ 一句话 brief → 点「确认·建立项目」(末卡另有一个「立即开工竞品分析」勾选框,见步5)。
- buddy 干(点「确认」时**一气做完**):
  1. 在 `workbench.db` 写一条 project 记录(name + `remote_host` + `remote_path`)。
  2. 建两个 connector 写进 DB:代码仓(workspace 路径)+ CodeHub(`host/path`)。
  3. 建 3 张 BW 侧标配 Issue 卡(竞品分析 / 找指标 / 绑数据)写进 DB,**同时往 maas codehub 仓真开 3 个 issue**(iid 连号);DB 里 issue 的 `github_number` = codehub iid。
  4. 若勾了「立即开工」框:顺手唤起 claude 跑第一张卡(见步5)。
- 看到:issues 看板 3 张卡;connector 列表有 `· 代码仓` + `· CodeHub`;`codehub-cli issue list …` 见 3 条(iid 跟 DB 对得上)。
- **坑**:点第二遍「建立项目」会**再建一套 trio + 再 clone 一个 workspace + 再开 3 个 codehub issue**(非幂等,见 §4 实测)。

### 实采呈现(issue/MR 计数点亮)

- 你做:等 cron tick 或手动触发 CollectMetrics。
- buddy 干:shell-out `codehub-cli issue|mr list --state X` 取计数 → 写进 `observation` 表(source=codehub,无「手填」徽)→ 重算 signal → 指标卡点亮。公共指标(开放 Issue 数 / 已合入 MR 数)开机默认 seed,不用自己定义。
- 看到:指标卡有数;`sqlite3 … "SELECT source,metric,value FROM observation WHERE source='codehub'"` 读回一致;raw `codehub-cli issue list --state opened --jq length` 跟看板数一致。

### 「立即开工」跑 issue(AI 干活)

- 你做:点某张 issue 的「立即开工」(或创建流末卡勾「立即开工竞品分析」)。
- buddy 干:
  1. 唤起 claude(`BW_CLAUDE_BIN` 指的全路径 claude.exe,shell-out `claude -p`),把 issue 的技能 + 工作区路径喂给它。
  2. agent 在项目工作区真改文件/跑测试;每花一分钱计进 `BW_CLAUDE_MAX_BUDGET_USD`(默认 $0.5)上限,超了就停。
  3. 跑完推到 `InReview`;**Done 永远由你点**(铁律)。
- 看到:issue 状态 → InReview(或预算超了停在 `in_progress` 可重试);工作区里 agent 留的真改动。
- 前提:`BW_CLAUDE_BIN` 配对 + **权限给够**(web_search 要预批或用 bypassPermissions)+ 预算够这活。竞品分析要联网检索,acceptEdits 下 web_search 被拒 → 烧预算空转(见 §4 实测)。

### 创建后 Op 侧边栏:看到啥 + 为什么 + 各干啥(maas-locate 实测)

建完项目进 Op,左侧栏分组。**读回 DB 核实**(非猜,DB=`…/BuildersWorkbench/workbench.db`,project_id=`cce215a7…`):

- **技能 +8 共享 / 智能体 +5 共享 / 工作流 +5 共享**:都是**全局库**(`project_id=NULL`,Boot 时 seed,跨项目共享,本项目只有使用权无所有权)。"+共享"= 本项目 0 条自有,8/5/5 全是全局借来的。
  - 8 技能 = 5 阶段方法论 + 3 标配(competitive-analysis / north-star-discovery / metrics-binding)。
  - 5 智能体 = 五阶段角色 agent(全局单例);5 工流 = 五阶段 template workflow。
  - **本项目没自有的原因**:归属反转(plan/08 S1)未做,项目级 skill/agent/workflow 还不创建(见 §5 + 设计地图 GAP#1)。
- **定时 · maas-locate 指标采集(normal)**:**本项目自有的 cron**(创建时 seed,`project_id=本项目`)。mode=collect_metrics,到点(每日)对挂的 codehub/github 远端跑 `codehub-cli issue|mr list --jq length` 取"开放 Issue 数/已合入 MR 数"→ observation → 重算 signal。**靠 CodeHub 连接器的 host/path** 才知道去哪采。
- **连接器(本项目自有 2 个)**:
  - **代码仓(git-repo,已连接)**:本地工作区 git 探针。Hub 点「立即同步」→ 数 commits/docs/dirty → 翻已连接 + 喂"工作区真实提交数""剧本产物文档数"指标(source=Connector,无「手填」徽)。已同步过→connected。
  - **CodeHub(codehub-repo,未连接)**:codehub 远端探针。同步 = `codehub-cli project view` 探活翻已连接(**只看活不活,不喂指标**)。指标靠上面 cron 采。还没同步→disconnected。
- **知识源**:0(未建)。

**一句话**:技能/智能体/工作流 = 全局共享库(只读借用);定时 + 连接器 = 本项目自有;代码仓同步喂工作区指标、CodeHub 同步只探活(指标由 cron 采)。

**Hub 资产存哪 / 怎么统计**:全局库实体存 DB 表(`skill`/`agent`/`workflow_spec`/`cron_task`/`connector`/`knowledge_source`);App 启动 + 每次相关命令后 `refresh_*` 重读 → state → `build_vm` → HubVm/OpVm.hub → 侧边栏计数。**侧边栏的数 = DB 表行数,`sqlite3` 直查可对**。

---

## 4. 持续扩充(每会话往这加)

### 2026-07-29 · live 端到端录 maas(步1 验证)

> **【2026-07-30 归正】** 本块结论后被实测推翻/完成,原文保留作过程记录,以本注为准:
> - Bug A 真根因**不是权限**,是 **GLM 网关不支持内置 web 工具**(WebSearch 返空/WebFetch 报 blocking claude.ai);`bypassPermissions`=**假成功**(模型用训练记忆冒充检索)、预批 WebSearch 也没用。修法待定(诚实降级/接 web-access skill/换真 Anthropic 端点),见 §6.3。
> - Bug B(多点 bug)**已修+提交 `1dbd76c`**(创建按钮 pending 守卫)。
> - 529 误判**已撤回**(预算错说明网关在响应、真花钱,非 529);冻结机制已定论:多点→多 spawn CompleteCreation→内核单线程被占窗口内堵(见 §4 干净重跑块 + Bug B 修)。

读回为证(DB = `…/BuildersWorkbench/workbench.db`):

- ✅ **claude 能 spawn 了**:`BW_CLAUDE_BIN` 配对后,创建流勾「立即开工竞品分析」成功 shell-out claude.exe + 打到 GLM 网关,"program not found" 消失。步5 核心问题 = **通**。
- ⚠️ **$0.5 预算错(表象,根因是权限——见下)**:报错 `engine: executor failed: Reached maximum budget ($0.5)(subtype=error_max_budget_usd)`。但竞品分析**根本没真检索**——web_search 被权限拒了,agent 在"探活被拒→降级生成"里把 $0.5 烧光。issue 留 `in_progress`(可重试,**没自动 Done,铁律守住**)。
- 🐞 **真根因·权限配错(用户点破,撤回先前 529 误判)**:先前说"撞 GLM 529 重试退避→泄漏→冻结"**是误判**——预算错说明网关其实在响应(真花钱生成了),不是 529。真根因:竞品分析 skill 硬性要 `web_search`(`docs/skills/competitive-analysis/PROBE.md:82`),但 buddy 跑它时 `--permission-mode acceptEdits`(`claude_cli.rs:61`,buddy **从不用** `BypassPermissions`)+ `--allowedTools` 只有 `Bash`(trio agent 卡没声明 WebSearch,`agent_import.rs:17` 样例 `["Read","Write","Edit","Grep","Glob"]`;全仓只有 vendor marketing/seo agent 声明了 WebSearch)。`claude -p` 非交互,acceptEdits 下未预批工具直接拒 → web_search 用不了 → agent 烧预算空转 / 产出全是"未核实"的空报告。用户别处用 `claude -p` 没事是因为带了全权限(`--dangerously-skip-permissions`)。`claude_cli.rs:46-48` 注释本就标"acceptEdits+Bash 能否解锁命令执行**未验证**"——本次实测 = **不解锁,web_search 被拒**。修法:agent run 用 `bypassPermissions`,或至少把 WebSearch 加进 agent `tools` 让 `--allowedTools` 预批。
- ⚠️ **重复点「建立项目」非幂等(实测比想的更糟)**:点了 N 遍 → codehub 上 BW 建了 **5 套 trio(iid 16-30,15 个 issue)**,但 BW DB 只落库 2 套(iid 19-24);其余 3 套是 **codehub 孤儿**(远端建成功、BW 回滚没落库——sync 非原子)。清理:关掉 12 个(iid 16-18、22-30),只留 iid 19-21 对应 BW issue 1-3。BW DB 的 issue 4-6(指向已关的 iid 22-24)**留 UI 删**(被 110 个 artifact + 1 workflow_run 引用,裸 SQL DELETE 太险)。
- ✅ **预算已调高 `BW_CLAUDE_MAX_BUDGET_USD=1000`**(User 持久,新终端继承)。claude CLI `--max-budget-usd` 无「0=无限」语义(0 = 允许花 $0 立即报错),1000 = 实际不设防 + 兜底防 runaway。**但警告:权限没修前别重跑竞品分析**——web_search 仍被拒,重跑只会烧更多钱 / 产出空报告,不会真对标。
- 🐞 **卡死(机制待查,不是 529)**:权限是竞品分析失败的根因;但"UI 冻结点不进卡片"的机制还没定论,疑似多次连点 → 多个 `CompleteCreation` 并发 spawn claude → 内核线程被占 + 30 分钟 attempt 超时窗口内子进程不退 → UI 阻塞。`claude_cli.rs:274` 有 30 分钟超时 + `kill_on_drop`,所以不是永久泄漏(超时会杀),但窗口内会冻。开发窗口单列 Bug 查。
- 🐞 **多点 bug(无防连点)**:`create.rs` 的「确认·建立项目」按钮 `onclick: confirm` **无 pending 守卫**;confirm 只 `k.send(CompleteCreation)` 后**不导航走**,Review 卡按钮一直可点 → 上述重复 seed 全因它。buddy 现无全局 busy/pending 态(全靠乐观 send + `on_created` 立即翻页,如 IntentCard);但 confirm 这条没翻页也没守卫。修法见 §3 风格:**confirm 起手置 `creating=true`,按钮置灰 + 文案「建立中…」,收 CompleteCreation 完成/失败事件后翻页到项目墙 / 失败 toast + 解禁**。待实现(>2 行,设计决定,留下一棒)。
- ✅ **claude 能 spawn 了**:`BW_CLAUDE_BIN` 配对后,shell-out claude.exe 成功,"program not found" 消失。步5 的 spawn 问题 = **通**;但"能 spawn"≠"能干活",权限是下一道坎(见上)。
- ✅ **SSH clone 真 maas 成功(P4-fix 坐实)**:绑定的 workspace `maas-locate-b845921b` 有真 maas 内容(`governance/`/`docs/`/`skills/`/`AGENTS.md`),不是空仓。SSH 绕开了 HTTPS 504。
- ⬜ **未做**:实采 CollectMetrics(observation 表 codehub 为空)——等权限修好、竞品分析真跑通后再验计数点亮。

### 2026-07-29 · 干净环境重跑 · codehub 闭环验证(步1 收口)

清空 BW DB(0 项目/issue/connector)+ 删 `workspaces/` 5 个孤儿后重跑创建(不勾立即开工)。读回为证:

- ✅ **SSH clone**:`workspaces/maas-locate-cce215a7/` 有真 maas 内容(AGENTS/CLAUDE/governance/docs/skills…)。
- ✅ **project 行**:`maas-locate` / phase=`running` / remote=`open.codehub.huawei.com`+`innersource/AI-Coding_G/maas` / workspace 落对。
- ✅ **2 connector**:代码仓(git-repo)+ CodeHub(codehub-repo),都挂本项目。
- ✅ **3 trio 落库 + codehub 远端一一对应**:DB #1 竞品分析↔iid31、#2 找指标↔iid32、#3 绑数据↔iid33,全 backlog(没跑,对);`codehub-cli issue list --state opened` 远端实见 31/32/33 标题对得上。
- ✅ 没跑 AI 小队:workflow_run=1 是 Drafting 卡 mock 起草,非真 claude。
- ⚠️ codehub 上还留着上次 trio iid 19/20/21(未清)+ maas 自己的 DTS 单 iid 10/15(别动)。
- **结论:步1 对接 codehub 闭环成立**(clone + project + connector + trio DB↔远端),不跑 AI 小队也成立。

### 2026-07-29 · Q2 gh leak / Q3 连接器同步背后 / Q4 hub-Op 边界

**Q2 · codehub 录入却碰 github(已修 2026-07-30)**:Repo 卡 `platform` 默认 `"github"`(create.rs:80);"接入已有仓" chip 点击即 fire `ListGithubRepos`→`gh repo list`(create.rs:333,仅 `!is_codehub` 时)。codehub 用户没先切平台就点了"接入已有仓" → gh 未装 → 诚实报错 toast。**但 codehub 流本不该碰 github**。根因=platform 默认 github + gh list 急触发(chip 一点就拉)。**已修**:`ListGithubRepos` 从 chip 急触发改「↻ 刷新列表」按钮懒触发(codehub 用户根本不碰 github 块)+ stale 注释归正。**已 live 验证 + 提交**。

**Q3 · 连接器「立即同步」背后干了啥(Hub 屏,connector_hub.rs:89 按钮)**:`SyncConnector`→`probe_connector`(lib.rs:2227)。
- **git-repo 连接器**:探工作区 git → 收 `WorkspaceEvidence`(commit_count/tracked_files/docs_files/dirty_paths)→ 翻状态"已连接"+last_sync → `feed_workspace_metrics` 把 `工作区真实提交数`=commits、`剧本产物文档数`=docs 写进 observation(source=Connector,无「手填」徽)→ recompute signal。**实测点了一次:39 提交 / 106 追踪文件**。
- **UI 变化(易漏看)**:连接器卡→已连接;Op 屏进度/阶段指标里"工作区真实提交数"卡点亮(=39,真数据,非手填)。
- **codehub/github-repo 连接器同步**:探 `codehub-cli project view`/`gh repo view`,只翻状态、不喂指标(指标走 collect arm 的 collect_count)。

**Q4 · hub vs Op 边界(认知 + GAP,待理清)**:实际设计——**Hub=全局库**(skills/agents/workflows/cron/connectors/knowledge_sources,公共可浏览);**Op=活跃项目的运营面**(本项目的 stages/issues/metrics + connectors 只读卡)。实体有 `project_id`(可空):NULL=全局,set=项目级。plan/09 §墙B 明说"Hub 全局视图、复制共享等全量归属反转**不在本次**"——即"项目级的东西完全在 Op 管、Hub 只留全局"这个反转未做。**缝点**:连接器是项目级数据,但它的管理动作(同步)在 Hub 而非 Op——这是用户在 Op 找不到同步按钮的原因。用户心智模型(hub=公共库、op=本项目操作闭环)与设计意图一致;差异在连接器这个项目级实体的动作位置。**待决定**:要不要把本项目连接器的同步动作挪到 Op(项目闭环),Hub 只留全局库浏览。

### 待记(后续会话补)

- _待记:步3 agent 真跑——权限修好后(bypassPermissions / WebSearch 预批)竞品分析能不能真联网出报告 + 产出 PR?_
- _待记:步2 check buddy 可视化全貌(项目墙/Op 屏/过程件/9-Hub)缺啥_
- _待记:推广给别人时,别人的前置装/配跟我的差异_

---

## 5. 对 buddy 越来越成熟的认知(演进中)

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

- **接口层 `Remote` enum + 工厂**:provider 分叉收敛在工厂一处(github → `Remote::Github(path)`、codehub → `Remote::Codehub{host,path}`),call-site 不各自长 match 臂,加 provider 漏改编译期报错。对标 Java `interface + 2 impl + 工厂`。
- **远端身份 `(host, path)` 均匀**:不是「github 不需要 host」,是当时把 `github.com` 隐式默认漏存一列。github 存 path+host=github.com、codehub 存 path+域名。
- **codehub.rs 走 shell-out `codehub-cli`**(不直调 GitLab v4 HTTP API):CLI 是 v4 同构封装、默认 JSON、token 在 keyring、零 Rust HTTP 依赖,与 `github.rs` shell-out `gh` 对称。clone 用 raw `git clone ssh://`(SSH 不经代理、不要 token,常规)。
- **API 活(probe/issue/mr)必须 codehub-cli**(GitLab v4 API+token,raw git 干不了);clone 是 git 活,raw git 即可,codehub-cli 的 clone 包装对 SSH 是纯透传无增益故绕过。

### 反命题(buddy 不是什么)

- 不是团队协作平台(无成员/群聊/收件箱)。
- 不是通用看板(无拖拽/甘特;回退不给 UI)。
- 不是审批系统(交棒只留痕不拦人)。
- 不是云服务(AI 执行=本机 `claude` CLI,单次花费封顶)。
- 永远不替用户捏造健康。

---

## 6. 往下推进的未决事项(讨论有价值、但非当前主要矛盾,记下待实践想明白再回头)

> 原则:正常事项当场处理;这里只记「现在做了也不一定对、得等实践推进到那一步才能正确决定」的事。每条记:讨论了什么 + 当前决议 + 待什么条件回头。

### 6.1 创建流 UI 该不该收窄(2026-07-29 讨论)

- **讨论**:创建末卡现在让用户填对标、三个月后算成、北极星+口径、引领/滞后指标(名+当前值+目标)。
  - 对标 + 算成 = AI 小队的种子输入(人意图,人填得准);竞品分析 skill 明文读 `benchmark`/`opportunity`。
  - 北极星 + 引领/滞后指标 = 要推导的(「找指标」issue 本职就是推北极星+三层指标);人填不准、创建时填了也没反馈、用户不知道填了能干啥。
  - 一条候选线:**人能清楚知道的意图留创建;要推导的移到「人主动触发那张 issue 卡」时再收(带反馈上下文)**。
- **关键事实(供回头用)**:对标/算成/北极星都存 project 行,每次 `run_issue_now` 经 `PlaybookCtx` 全喂给 agent;创建后可在 Op 屏「进度·全部」→「编辑项目」卡改(P9 建的,benchmark/opportunity/north_star 都能改);Drafting 卡 mock 生成体系草案(北极星/指标草案)。
- **当前决议:先不动**。`run_first` 默认不勾已是现状(不勾就好);指标值选填可空。等竞品分析/找指标/绑数据三个 AI 小队实践中想明白「输入实际怎么被用、哪填合适」,再回头正确调 UI——**不基于猜测改设计**。

### 6.2 run_first 在创建时 auto-run

- point1 原则:竞品分析不该创建时默认跑,由人创建后在 issue 卡触发。已对齐:`run_first` 默认 false(`create.rs`)。保留末卡「立即开工」框、默认不勾(人想创建即跑再勾)。

### 6.3 Bug A(竞品分析跑不动)修法

- 真根因 = GLM 网关不支持内置 web 工具 + glm-5.2 不老实降级、烧预算到 $0.5。修法待定(诚实降级 prompt 硬停止 / 接 web-access skill / 换真 Anthropic 端点)。**留到实践「竞品分析」卡时再调**(AI 小队调试不在创建阶段)。权限没修前别重跑竞品分析——重跑只会烧更多钱 / 产出空报告。

### 6.4 issue 看板要不要从仓库取(2026-07-29 困惑)

- **现状(by design,非 bug)**:buddy issue 看板 = `store.list_issues(DB)`,只列 buddy 自建/管的 issue 卡(trio + 用户后建的);**不导入** codehub 上项目原有的 issue(如 maas 的 DTS 单 iid 10/15)。项目原有 issue 只进**指标**(`collect_count` 数"开放 Issue 数"),不进看板。
- **困惑**:用户期望"看板从仓库 issue 列表取",实际不是镜像。
- **决议:先不动**,记 GAP。要不要把仓库原有 issue 拉进看板(当 buddy 卡还是只读参考?)待实践想明白——和"issue 卡是 buddy 工作单元"的设计命题相关,别轻改。

### 6.5 连接器"未连接"= 未同步过(by design)+ probe-at-creation GAP

- **现状**:连接器状态 Connected/Syncing/Error/**Disconnected**(model.rs:1797-1800);新建默认 Disconnected。翻成"已连接"要 `SyncConnector` 真探针(`codehub-cli project view`/`gh repo view`)跑过(lib.rs:4495-4500),由 cron collect_metrics 或手动同步触发。刚建完没同步 → 显示"未连接"。**不是坏了**。
- **作用**:代码仓(git-repo)= 本地工作区 git,供版本日志/变更对比/推送;CodeHub(codehub-repo)= codehub 远端,供 issue/MR 计数采集。
- **GAP(体验)**:建连接器时就地探一次活翻"已连接"更友好(现在得等同步/手动触发)。待实践想明白要不要做。
