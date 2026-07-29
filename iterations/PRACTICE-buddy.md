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

---

## 3. 正式怎么用:每步背后动作 + 能看到啥

### 录一个 codehub 项目(以 maas 为例)

1. **启动** `cargo run -p app-desktop` → 项目墙。
2. **创建流 → Repo 卡**:
   - 平台 chip 选 **CodeHub**(默认 GitHub,点亮 codehub)。
   - host = `open.codehub.huawei.com`(内源默认;绿区仓改 `codehub-g.huawei.com`)。
   - path = `innersource/AI-Coding_G/maas`(**手填**,placeholder 不是值,不填 create 按钮禁用)。
   - 「起点」选「接入已有仓」。
3. **Intent 卡**:填项目名(maas-locate)+ 一句话 brief → 点创建。
   - **背后**:`Command::CreateProject{provider:codehub, codehub:Some(CodehubOrigin{host,path})}` → handler codehub 分支:
     - `codehub::clone_repo` SSH clone maas 进 `workspaces/<name>-<id>`(真 maas 内容,非空仓)。
     - `set_remote(host, path)` 落 `remote_path`+`remote_host`。
     - mint 两个 connector:`git-repo`(workspace 路径)+ `codehub-repo`(config=`host/path`)。
4. **Questions / Review → 完成**:
   - **背后**:`CompleteCreation` → `seed_standard_issue_trio`(gate:`remote_path` 非空 → 过)→ 建 3 张 BW 侧标配卡(竞品分析/找指标/绑数据)+ `sync_issue_to_remote → Remote::Codehub.create_issue` → **往 maas codehub 仓真开 3 个 issue**(iid 连号)。
5. **能看到**:
   - issues 看板有 3 张标配卡(竞品/找指标/绑数据)。
   - connector 列表有 `· 代码仓` + `· CodeHub`。
   - 工作区目录有真 maas 文件(`governance/`/`docs/`/`AGENTS.md` 等,不是 `PROJECT.md` 空仓)。
   - 交叉对:`codehub-cli issue list -p innersource/AI-Coding_G/maas --state all -l 0 --jq '.[].iid'` 应见 BW 建的 3 条(iid 连号,标题是「竞品分析/找指标/绑数据」)。

### 实采呈现(issue/MR 计数点亮)

- 走 codehub `collect arm`(`CollectMetrics` cron tick / 手动触发):`codehub-cli issue|mr list --state X -l 0 --jq length` → 计数落进 observation(`SourceKind::Codehub`,无手填徽)→ 指标卡点亮。
- 公共指标(开放 Issue 数 / 已合入 MR 数)Boot 时默认 seed(`seed_codehub_public_metrics`),**不用定义**。
- 交叉对:raw `codehub-cli issue list --state opened --jq length` 与看板数一致。

### 「立即开工」跑 issue(AI 干活)

- 点 issue 的「立即开工」→ `RunIssue` → `ClaudeCliExecutor` shell-out `claude -p`(用 `BW_CLAUDE_BIN` 全路径)→ agent 在项目工作区真改文件 → 跑完进 `InReview`(**Done 永远由人点**,铁律)。
- **前提**:`BW_CLAUDE_BIN` 配对 + GLM 网关通。没配 → "program not found"(§2)。
- 这是步3(agent 真跑),步1 不碰。

---

## 4. 持续扩充(每会话往这加)

- _待记:步3 agent 真跑撞到啥(网关 529 重试?产出 PR?)_
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
