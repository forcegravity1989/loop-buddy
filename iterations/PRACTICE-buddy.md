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

### 2026-07-30 · 竞品分析真跑 → 两个 bug 现形(UI 冻死 + 联网墙复现)

> 真实 codehub 项目 maas-locate 上单次点「▶ 跑」的实测。读回为证(DB=`…/BuildersWorkbench/workbench.db`)。本轮只取证 + 落 PRACTICE,**不改代码**。
> 注:PR #67(skill 标准 v1)已合入,未碰 `bw-engine`/`kernel.rs`/`claude_cli.rs`(step1 窗口已核 + 本窗 git 侧证实),本块冻死链路/联网墙结论不受影响。

- **你做**:issues 面板 →「竞品分析」卡(codehub iid 31)→ 点「▶ 跑」。
- **后台真干啥(读回)**:
  - 竞品分析 issue `backlog → in_progress`(状态机起手转移)。
  - `workflow_run` 落新行 `000f3fb2…` 状态 `running`;`params_json` = `force_mock:false`(真执行器非 mock)、`allowed_tools_arg:Bash`(只给了 Bash,**没给联网工具**)、5 阶段(证据/洞察/假设/原型/验证)、`max_iter 3 retries 1`。
  - 执行器 spawn 真 `claude.exe`(父进程 = builders-workbench),在工作区目录里跑,带 `--permission-mode acceptEdits --allowedTools Bash`,预算封顶 $1000 往上烧。
  - 工作区 git 干净、**没冒 `docs/competitive-analysis.md`**——run 卡在头几阶段(「证据」阶段联网检索被拒、反复重试空转)。
- **🐞 bug①·界面冻死(新,比联网墙更要先修)**:点完跑,侧边栏冒出「竞品分析 进行中」一帧,然后**整个界面卡死**,后续点啥都没反应。根因:buddy 的「大脑」跑在**一条单线程**上(`kernel.rs:479` 故意只用一个线程——App 状态单线程独占);主循环里 `kernel.rs:641` 那行 `app.dispatch(cmd).await` 会**一直等到整件事干完才返回**,而 RunIssue 最长 30 分钟。期间 UI 后续点的按钮全堆通道里没人取 → 界面拿不到新状态 → 冻死;定时调度也停。执行器本身是异步的(`claude_cli.rs:231` 用 `tokio::process`),所以进度 toast 理论上能在 await 点流出,但**驱动列表/状态/导航的 Vm 只在 dispatch 返回后重发**(`kernel.rs:644`)→ 视觉上就是冻死。`kernel.rs:627-633` 注释自己也承认「App 单线程独占,只能 select 交错,不能 spawn 独立 task」——这就是代价。**这不是 GLM 的锅,是 buddy 自己的架构问题。**
- **🐞 bug②·联网墙(Bug A 复现,预期内)**:真执行器只给 Bash、没给联网工具,claude 在「检索被拒→硬编」里空转烧预算——和 §6.3 钉的根因一致,这次用真实 codehub 项目复现了一遍(不是 mock、不是猜测)。
- **处置**:杀掉 buddy 起的那个 claude 子进程(**只杀父进程=builders-workbench 的那个,不动别的 Claude 会话**——用 `Get-CimInstance Win32_Process` 查 ParentProcessId 精确定位,别裸 `taskkill claude.exe` 误杀)。杀完 run 结算 `failed`,error = `executor failed: claude CLI exited with exit code: 0xffffffff`(这是杀进程造成的 exit code,不是自然报错;自然报错应是「烧到封顶」或冒幻觉报告)。竞品分析 issue 停 `in_progress`、`settled_at` 空——**没自动 Done,铁律守住**,可重试。界面解冻。
- **决议**:**先修 bug①(界面冻死)**——不修它,根本没法在 UI 里驱动小队(一跑就冻)。bug② 联网墙留 §6.3 待定,bug① 修好后再回头调(到时界面不冻了才能在 UI 里观察 bug② 的真实终端报错)。

### 2026-07-30 · 找指标 run 实跑全链路(读回为证)+ 三个 bug 现形

> 真实 codehub 项目 maas-locate 上单次点「▶ 跑」找指标卡(north-star-discovery,不联网)。run `ok`(约 21 分钟,7 commit)。DB=`…/BuildersWorkbench/workbench.db`、工作区=`…/workspaces/maas-locate-cce215a7`、分支 `bw/issue-32`。

**跑通的(成功):**
- ✅ **联网墙绕开**:north-star-discovery 不联网,只用 Read/Write/Bash;`--allowedTools Bash` + acceptEdits 下 **Read/Write 真能写文件**(不是只有 Bash)——这道工具白名单墙没中,bug② 对不联网技能不成立。
- ✅ **agent 真出活**:产出 `.bw/metrics.toml`(机读指标定义:北极星=定界结论采纳率 manual + 滞后未采纳率/闭环时长)+ `docs/metrics-rationale.md`(11.7KB 推导)+ `evidence/insights/hypothesis/validation/competitive-analysis(Path B)`;agent 自己 git commit 7 个(规范信息带技能名+issue 号),分支 push 到 codehub(`origin/bw/issue-32`)。
- ✅ **message 落账是通的(更正早前过早结论)**:我 run 中途查 session 0 message,以为是 bug;**run 跑完后 session `#2 找指标` 有 5 条 message**(每阶段一条 agent 自述)。所以 message 表是按 phase 完成增量落账的,不是不落账——只是 run 期间(界面冻死)看不到增量。

**三个 bug/Gap 现形:**
- 🐞 **bug① 冻死(已知,坐实)**:run 期间界面整场冻死 ~21 分钟。根因 `Command::RunIssue` handler(`lib.rs:4948-4950`)直接 `self.run_issue_now().await`、没 `tokio::spawn` 甩后台;`run_issue_now` 是 `&mut self`,整段(含 spawn claude + 等输出,最长 30 分钟)内联在 kernel 单线程命令循环 `app.dispatch(cmd).await`(`kernel.rs:641`)里,循环一卡、Vm 不重发(`kernel.rs:644` 只在 dispatch 返回后发)→ 界面冻死。**反证它不该这样**:run 中段(真长的 claude spawn)其实只用共享借用(`lib.rs:1457` 注释「never &mut self」),不需要独占 App——冻死是「连不需要 &mut 的 IO 段也内联占着」造成,不是单线程 App 设计的必然代价。修法:把 run 的 IO 段甩出去、起手/收尾回 kernel 线程改状态;拦路虎是 App 单线程独占非 Arc<Mutex>(`kernel.rs:627`)。**>2 行,留下一棒**。
- 🐞 **bug③ codehub MR 回流断(新)**:run `ok` 但 issue 卡 `in_progress`、pr_number=0、没推 InReview。根因:`github::open_pr`(`bw-engine/src/github.rs:474`)干 `git add -A + commit + push + gh pr create`;对 codehub,push(SSH)成功 → 分支上 codehub,但 **`gh pr create` 失败**(gh 不认 codehub)→ open_pr 返 Err → 没回写 pr_number、没 InReview(`lib.rs:3391 opened_pr=false`)。UI toast 如实报「🔌 #2 · PR 同步 ✕ · 提 PR 失败,活留在进行中可重试:gh 未安装或不在 PATH」。**为什么 step1 工厂改造漏了它**:`Remote` 工厂(`bw-engine/src/remote.rs`)只有 `for_project`/`probe`/`create_issue`/`collect_count` 4 个方法,**没有 `open_pr`/`create_mr`**——MR 回流是老的 `bw_engine::github::open_pr` 直接调(`lib.rs:3339`),从没折进工厂;step1 范围是 clone+project+connector+issue 同步(全「出」方向),MR「回」方向不在范围。修法:给 `Remote` 加 `create_mr`,codehub arm 走 `codehub-cli mr create`(对称 `gh pr create`)。
- 🐞 **bug⑤ 发送框是 mock 占位(新,留白)**:工作流面板下面那个发送框 = `Command::SendSessionMessage`(`lib.rs:791`),handler(`lib.rs:6297`)只把你的话存进 session 然后回写死的 `【mock】已收到:{text}`——注释明说「真 agent 回复走 Tier C,未实现」。**所以「跟 agent 对话调整指标」没建**:run 本身是 `claude -p --no-session-persistence` 每阶段一次性调用、无持久对话,没有「同一上下文」可续;要 agent 重调得重新 RunIssue(retry 一次性新调用)。如实标注的留白(带【mock】),不是坏。

**认知(三件套流水线 + 监控时机)**:找指标只「定义」指标(写 `.bw/metrics.toml`,大部分 manual 因 codehub 远端+评估器留白采不到);**真监控在绑数据之后**——绑数据给每条指标接点亮路径、cron/连接器采 → observation → recompute_signals → 健康灯亮。找指标跑完 ≠ 开始监控。北极星(采纳率)是 manual,只能人定期填亮,这诚实。

### 2026-07-31 · bug③ 修复(codehub PR/MR 全生命周期)+ UI + 新发现

> bug③ = codehub MR 回流断(`github::open_pr`/`merge_pr` 写死 gh,codehub 上 gh 失败 → issue 卡 InProgress、走不到 InReview、SyncMetricsFile 不跑、trio 指标不进表)。**已修+提交 `0c70775`**。读回为证:烟测 `codehub-cli mr create` 真开 MR(iid 11/12),`merge_mr` 真打到 codehub 拿真实 403(证明命令对)。

**修了什么(7 文件):**
- `Remote` 工厂加 `create_mr`(github 透传 `open_pr` / codehub `codehub-cli mr create --source-branch --target-branch <动态取> --issue-nums issue<n> --jq .iid`)+ `merge_mr`(github `gh pr merge` / codehub `codehub-cli mr merge <iid> --squash -y`);codehub `create_mr` 加 **Adopted**(`mr list --source-branch` 认领已存在 MR,parity github 的 `adopt_existing_pr`)。
- 抽 `workspace::stage_commit_push`(add+幂等 commit+push,F5 逻辑一份),`open_pr` 与 `codehub::create_mr` 共用,不复制踩 F5 坑。
- `run_issue_now` / `MergeIssuePr` 收尾改走 Remote 工厂;`MergeIssuePr` 的 gh issue 补关 gate 到 github provider(codehub 用 `--issue-nums` 关联,merge 自动关单)。
- **UI**:`OpVm` 加 `provider`;issue 卡 issue 地址 + PR 号改 provider-aware 可点击 link(codehub `{host}/{path}/issues`|`/-/merge_requests`,github `github.com/...`),不再写死 `github.com` 裸 URL(`op.rs` 旧代码硬编 github URL + 纯文本无 link)。

**实践操作(对账让找指标/绑数据走到 InReview,绕开 bug① 冻死):**
- 找指标(#32)烟测时已建 codehub MR 11;绑数据(#33)无 MR → 手动 `codehub-cli mr create` 建 MR 12(同 create_mr 命令)。
- 停 app → `sqlite3` 对账:`UPDATE issue SET pr_number=11/12, status='in_review'`(诚实:MR 真存在,legal 转移 in_progress→in_review)→ 重编重启 app → 两张卡显 InReview + merge 按钮。
- **背后**:这是「手动对账」快捷路径,避 bug① 的 20min retry 冻死;正路是 retry(等 bug① 修好),retry 会走 create_mr/Adopted 自动建/认领 MR。

**新发现(不是 bug③ 的 bug,是治理/工作流/UI):**
- 🐞 **UI① issue 卡 issue 地址写死 `github.com` + 裸 URL**:已修(provider-aware link)。codehub issue URL = `https://{host}/{path}/issues/{iid}`(实测 issue_link)。
- 🐞 **UI② PR #N 纯文本无 link**:已修(link 到 `/-/merge_requests/{iid}`)。
- ⚠️ **merge 403 不是 buddy bug**:`merge_mr` 命令对(`codehub-cli mr merge 11 --squash -y` 真打 codehub 拿 403「target branch is protected, you do not have MERGE permission」)——是 maas master 保护分支 + CLI token 无 merge 权限的治理问题。buddy 如实报错、issue 留 InReview 可重试。解法在 codehub 侧:网页有权限账号 merge / 解保 master / target 真实开发分支(`a_develop`)。
- 🐞 **④ 两个 MR 内容重叠(工作流)**:三件套是流水线(绑数据读找指标产出),但**并行跑了**找指标+绑数据(都从 master 出分支、都改 `.bw/metrics.toml`)→ MR 冲突。正路串行:先 merge 找指标,master 拿到 metrics.toml,再跑绑数据。buddy 没强制依赖 + 没 worktree 隔离(我以为开发搞了 worktree 隔离,实际没有——run 共用一个 workspace 目录)。**归类 bug① 窗口一起看**(见 §6.7)。

**验证状态**:烟测 + 编译门禁全绿;完整 buddy retry E2E 延后(撞 bug①);**UI fix 已 live 验证(2026-07-31,user 重编重启后):issue 卡「远端 #N ↗」「PR #N ↗」provider-aware 可点击 link,跳 codehub issue/MR 页**。

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

### 2026-07-30 实测·竞品分析真跑(maas-locate,读回为证)

> 单次点「▶ 跑」的真实记录,不是创建流 run_first、不是 mock。**结论:当前别在 UI 里点真 run——会冻死(见 §2 bug①)。**

- **你做**:issues 面板 →「竞品分析」卡 → 点「▶ 跑」。
- **buddy 后台干了啥(可看见)**:
  1. 起一个新 session(标题带「竞品分析」),侧边栏冒一行「进行中」——这是 run 起手推出去的**最后一帧 Vm**,之后就再没新帧。
  2. issue 状态 `backlog → in_progress`;`workflow_run` 表落一行 `running`(params:`force_mock:false`、`allowed_tools_arg:Bash`、5 阶段)。
  3. spawn 真 `claude.exe`(在工作区目录里),把竞品分析技能正文 + 项目意图喂给它,`--allowedTools Bash`、预算封顶 $1000、`acceptEdits`。
  4. claude 在工作区干活——但联网检索被拒,空转烧预算,没产出文件。
- **你看到啥**:侧边栏「竞品分析 进行中」一帧 → **整个界面冻死**(后续点啥都没反应)。没有 toast、没有 PR、没有 InReview。
- **杀 claude 后**:run 结算 `failed`;issue 停 `in_progress` 可重试;界面解冻。
- **结论**:「点跑 → 界面冻死」是当前确定行为,根因 §2 bug①(单线程命令循环被长 run 阻塞)。bug① 修好前,要复现/调 bug② 得走 headless(example),别在 UI 里点。

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

### 2026-07-30 · UI 冻死根因钉死(归正旧猜)+ 杀进程验证

- **归正**:§4 2026-07-29 块对冻死的猜是「多次连点 → 多个 CompleteCreation 并发 spawn → 内核线程被占窗口内堵」。**今天单次点「▶ 跑」(非创建流、非连点)照样冻死** → 推翻「多连点」假设。真根因:**任意长 RunIssue 都会冻**——`app.dispatch(cmd).await`(`kernel.rs:641`)内联在单线程命令循环里,run 不返回循环就不转,UI 状态一帧不更新。多连点只是让事情更早发生,不是根因。
- **执行器是异步的**(`claude_cli.rs:231` `tokio::process`),所以进度 toast 理论上能在 await 点流出;但驱动界面列表/状态/导航的 Vm 只在 dispatch 返回后重发(`kernel.rs:644`)→ 视觉上=冻死。buddy 进程仍标 `Responding=True`(Windows 还能回消息),但画面不动、输入不响应。
- **杀进程验证(证明冻死=等 run 返回,不是死锁)**:精确定位父进程=builders-workbench 的 claude 子进程(`Get-CimInstance Win32_Process` 查 ParentProcessId,别裸 `taskkill claude.exe` 误杀别的会话)→ `Stop-Process -Id <pid> -Force` → spawn future 返回(exit `0xffffffff`)→ run 结算 `failed` → 循环解冻、界面能动。**没自动 Done**(`settled_at` 空,铁律守住)。
- **修法方向(待定,本轮不动)**:RunIssue 得甩后台 detached,但 `&mut App` 单线程独占是拦路虎(`kernel.rs:627` 注释点明「不能 spawn 独立 task」);退一步,Vm 在 run 期间按事件/定时重发也能解冻视觉。**>2 行,设计决定,留下一棒**。先修它,再回头调 bug② 联网墙(界面不冻了才能在 UI 里观察 bug② 的真实终端报错)。

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

- 真根因 = GLM 网关不支持内置 web 工具 + glm-5.2 不老实降级、烧预算到 $0.5。修法待定(诚实降级 prompt 硬停止 / 接 web-access skill / 换真 Anthropic 端点)。
- **【2026-07-30 归正·已实践】**:今天在真实 codehub 项目 maas-locate 上单次点「▶ 跑」复现了 bug②(联网墙:真执行器只给 Bash、没给联网工具,claude 空转烧预算,见 §2)。但实践暴露出**更该先修的 bug①(界面冻死)**:单次 RunIssue 就把单线程命令循环堵死、UI 冻死(根因 §2/§4 钉死)。**bug① 不修,根本没法在 UI 里驱动小队,也观察不到 bug② 的真实终端报错**。所以顺序调:**先修 bug① 冻死,再回头调 bug② 联网墙**。bug② 修法仍待定,但要在 bug① 修好后、界面不冻时,才能在 UI 里真跑观察终端报错(烧到封顶 / 冒幻觉报告)来定方案。
- 权限没修前别在 UI 里重跑竞品分析——会冻死 + 烧预算空转。要复现走 headless example。
- **【2026-07-30 决议·绕法 + 跳过】**:竞品分析模块**暂时跳过**,不碰原竞品分析技能(它的联网墙留 bug② 待定)。绕法已想清(暂不实施):用「项目集自身技能」——在 Hub 技能屏用 `CreateSkill` 建一个「不联网版」竞品分析技能(人喂对标材料 + agent 整理成报告,只用 Read/Write/Bash)→ 建新 issue 时「关联技能」选它 → 跑。这样绕开联网墙。**注意:不能改已 seed 的竞品分析卡的技能**——没有 `UpdateIssue` 命令,「关联技能」下拉只在 `CreateIssue` 表单里(见 §6.6 GAP)。所以真要做竞品分析,得建新卡绑新技能,不是改老卡。
- **实践顺序调整**:先在**不联网**的 `找指标`(north-star-discovery)卡上练「issue 处理 + 驱动 AI + 界面显示」全链路——它只读项目意图 + 工作区文件、写 `docs/metrics-rationale.md`,不碰 web,天然绕开 bug②。bug①(冻死)仍会中,但短 run 冻的时间短。跑通后再回头处理竞品分析(走上面的绕法)和 bug① 修法。

### 6.6 issue 技能绑死·无 UpdateIssue 命令(GAP,2026-07-30)

- **现状**:issue 的 `standard_skill` 在 `CreateIssue` 时一次性写入(`lib.rs:752`),之后**没有命令改**——Command 枚举里 issue 变更只有 `CreateIssue`/`TransitionIssue`/`AssignIssue`/`BlockIssue`/`MergeIssuePr`/`RefreshIssues`,无 `UpdateIssue`/`SetIssueSkill`。UI「关联技能」下拉只在建新 issue 的表单里(`op.rs:704`)。
- **影响**:seed 出来的三件套卡(竞品分析/找指标/绑数据)技能绑死后改不了;想给已有 issue 换技能(如把竞品分析换成不联网版)做不到,只能建新卡。
- **决议:先不动,记 GAP**。当前实践用建新卡绑新技能绕;要不要加 `UpdateIssue` 命令留后续(和「issue 卡是 buddy 工作单元」的设计命题相关,别轻改)。

### 6.4 issue 看板要不要从仓库取(2026-07-29 困惑)

- **现状(by design,非 bug)**:buddy issue 看板 = `store.list_issues(DB)`,只列 buddy 自建/管的 issue 卡(trio + 用户后建的);**不导入** codehub 上项目原有的 issue(如 maas 的 DTS 单 iid 10/15)。项目原有 issue 只进**指标**(`collect_count` 数"开放 Issue 数"),不进看板。
- **困惑**:用户期望"看板从仓库 issue 列表取",实际不是镜像。
- **决议:先不动**,记 GAP。要不要把仓库原有 issue 拉进看板(当 buddy 卡还是只读参考?)待实践想明白——和"issue 卡是 buddy 工作单元"的设计命题相关,别轻改。

### 6.5 连接器"未连接"= 未同步过(by design)+ probe-at-creation GAP

- **现状**:连接器状态 Connected/Syncing/Error/**Disconnected**(model.rs:1797-1800);新建默认 Disconnected。翻成"已连接"要 `SyncConnector` 真探针(`codehub-cli project view`/`gh repo view`)跑过(lib.rs:4495-4500),由 cron collect_metrics 或手动同步触发。刚建完没同步 → 显示"未连接"。**不是坏了**。
- **作用**:代码仓(git-repo)= 本地工作区 git,供版本日志/变更对比/推送;CodeHub(codehub-repo)= codehub 远端,供 issue/MR 计数采集。
- **已改(2026-07-30)**:CompleteCreation 末尾对建完的项目连接器就地 `probe_connector` 一遍——git-repo 顺带喂工作区指标(commits/docs)、codehub/github-repo 翻 Connected。用户建完项目即健康 + 即有工作区数据,不用再去 Hub 点「立即同步」(Hub 的同步留作后续手动刷新)。探活失败软降级留 Error,不倒灌创建。过门禁,**待 live 验证**(下次创建项目看连接器是否自动 Connected、工作区指标是否当场点亮)。

### 6.7 并行 run 不支持 + 无 worktree 隔离(2026-07-31,归类 bug① 窗口)

- **发现**:跑找指标+绑数据两件活,两个 MR(bw/issue-32 / bw/issue-33)内容大量重叠——都从 master 出分支、都改 `.bw/metrics.toml`。根因:**run 共用一个 workspace 目录、无 worktree 隔离**;三件套是流水线(绑数据读找指标产出)但没强制串行,并行跑就互相踩。
- **以为开发搞了 worktree 隔离,实际没有**——每个 issue 的 run 在同一个 `workspaces/<project>-<id>` 目录里干活、切 `bw/issue-<n>` 分支,分支隔离了 commit 但**工作区文件不隔离**(找指标写的 metrics.toml 留在工作区,绑数据接着改 → 两个分支的 diff 都含对 metrics.toml 的改动 → MR 重叠)。
- **与 bug① 同源**:bug① = 单线程命令循环内联 await 堵死 UI;这条 = 单 workspace 无隔离导致并行 run 互相踩。**两件都在「run 调度」层**,归类 bug① 窗口一起设计:甩后台 + worktree-per-run 隔离 + (可选)三件套串行依赖。

**给 bug① 窗口的 prompt(可直接粘过去):**
> 在 buddy(loop-buddy 仓)修 bug①「RunIssue 内联 await 堵死单线程 UI」时,一并看这个同源问题:run 共用一个 workspace 目录、无 worktree-per-run 隔离,导致并行/连续跑多个 issue(如三件套 找指标→绑数据)时 MR 内容重叠(都从 master 出分支、都改同一文件)。请一起设计:① RunIssue 甩后台 detached(解冻 UI,根因见 PRACTICE §2 bug① + §4「UI 冻死根因钉死」);② 每个 run 用独立 git worktree 隔离(`git worktree add` per issue,不在主 workspace 切分支);③ 三件套串行依赖(绑数据要求找指标先 merge)要不要在状态机/调度层强制——待实践定,别基于猜测改设计。codehub PR 回流(create_mr/merge_mr/Adopted)已在 `0c70775` 修好,本窗口不用重做;只看 run 调度层。背景见 `iterations/PRACTICE-buddy.md` §2(2026-07-31 块)+ §6.7。
