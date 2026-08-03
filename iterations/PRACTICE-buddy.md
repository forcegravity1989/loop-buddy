# 践行日志 · buddy(操作指南 + 实践记录 + 认知演进)

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
> 维护规范见 `.claude/skills/practice-buddy-landing/SKILL.md` §6。

---

## 1. 前置:用 buddy 前要装/配啥

### 运行时必装必配(干活绕不开)

| 项 | 装啥/配啥 | 为啥 |
|---|---|---|
| **git** | 任何 clone/workspace 操作都要 | 通用 |
| **claude CLI** + **`BW_CLAUDE_BIN`** | AI 干活(issue 执行器)shell-out `claude -p`,要给全路径(Windows 上 Rust `Command::new("claude")` 不做 PATHEXT,只认 .exe/.cmd,见 §2 步2) | `BW_CLAUDE_BIN=C:\Users\<你>\AppData\Roaming\npm\node_modules\@anthropic-ai\claude-code\bin\claude.exe`(持久 User 环境变量) |
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
  - **重复点「建立项目」非幂等**(Bug B,`1dbd76c`):confirm 按钮无 pending 守卫 → 多点建多套 trio + codehub 孤儿 issue → **改** confirm 起手置 `creating=true` 置灰「建立中…」,完成/失败后翻页/解禁。
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
- buddy 干:`MergeIssuePr` → `Remote.merge_mr`(codehub `codehub-cli mr merge <iid> --squash -y`)→ merge 成功推 Done(InReview→Done,人点的 merge 触发,非自动)。
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
- **实采呈现**:等 cron tick 或手动 `CollectMetrics` → shell-out 取计数 → 写 `observation` 表(append-only,一个观测=一个点,绝不插值;window-guard:同窗口同值才跳过)→ `recompute_signals` 重算 → 指标卡点亮。公共指标(开放 Issue 数/已合入 MR 数)开机默认 seed。
- **健康灯 derive-only**:Signal 只能经封口 `Derived<Signal>` 进缓存,store 无 `set_signal`,`recompute_signals` 唯一写入者;**无数据=Unknown≠绿**,数据过期降级,手填带徽。任何界面数字能 `sqlite3` 独立查证。

#### artifacts(产物登记)
- agent run 产出的文件(docs/evidence/...)登记到 artifact 表,归属项目。

#### version(版本日志)
- `LoadVersionLog` shell-out `git log` 取版本日志。

#### sessions(会话)
- run 的 message 按 phase 完成增量落账(每阶段一条 agent 自述);run 是 `claude -p --no-session-persistence` 每阶段一次性调用,无持久对话(⚠ 发送框是 mock,见步5 bug⑤)。

---

## 4. 未决事项(按主题,关联指回主流程/分支操作哪步)

> 讨论有价值但非当前主要矛盾、现在做了也不一定对的事。每条:讨论啥 + 当前决议 + 待什么条件回头。

### 4.1 创建流 UI 该不该收窄(指回步2)
- 创建末卡让用户填对标/算成/北极星/引领·滞后指标。对标+算成=人意图填得准(竞品分析 skill 读 `benchmark`/`opportunity`);北极星+指标=要推导的(「找指标」issue 本职),人填不准、创建时填了没反馈。**决议:先不动**。`run_first` 默认不勾(§4.2)已是现状;指标值选填可空。等三件套实践中想明白「输入实际怎么被用、哪填合适」再回头——**不基于猜测改设计**。

### 4.2 run_first 在创建时 auto-run(指回步2)
- 竞品分析不该创建时默认跑,由人创建后在 issue 卡触发。已对齐:`run_first` 默认 false(`create.rs`)。保留末卡「立即开工」框、默认不勾。

### 4.3 bug① 冻死·RunIssue 甩后台 + 并行 run 无 worktree(指回步4/5/6)
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
- task6 墙移植不开发(plan§1.4 vs memory 2026-07-28"项目健康总览先不补"冲突,没对齐不开发);层 B 来源徽未做(补充暂不开发)
- task7 UpdateWeekPlan 接 UI 未做(plan 内但非核心——改周计划非改指标值/采集,标 §4)
- maas 侧:`clouddragon_cache.json` 被 cron 跑空(采集产零,maas 侧重跑 refresh_data.py 恢复)+ `derive_leading.py`/`data.json` 改(adoption_rate+扁平镜像)未 commit maas 仓
- script connector:300s 超时硬编码 / probe 不查 command 可用 / 多 connector 字段重叠语义(非阻塞标后续)

**task2 管线验证**(SubAgent):maas 脚本加 adoption_rate+扁平镜像(因 buddy `json_field_by_path` 只走点分对象键不数数组)→worktree 编译 buddy OK→SQL 建 script connector+4 metric 行(北极星/L1/L2/L3 collect_kind=script)→深链渲染 `[BW_OPEN]` 无 panic→采集点亮留主窗口(GUI CollectMetrics)。**SQL 直改 DB 绕法非终态**,终态走 metrics.toml+SyncMetricsFile 按钮。

### 待记(后续会话补)
- _待补:步3 agent 真跑——bug① 修好后竞品分析能不能真联网出报告 + 产出 PR?_
- _待补:推广给别人时,别人的前置装/配跟我的差异。_

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

### 反命题(buddy 不是什么)
- 不是团队协作平台(无成员/群聊/收件箱)。
- 不是通用看板(无拖拽/甘特;回退不给 UI)。
- 不是审批系统(交棒只留痕不拦人)。
- 不是云服务(AI 执行=本机 `claude` CLI,单次花费封顶)。
- 永远不替用户捏造健康。
