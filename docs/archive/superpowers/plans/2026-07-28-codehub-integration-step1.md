# CodeHub 对接 · 步1 执行计划

> ⚠️ **历史档案(2026-08-17 归档)**。这是一份已经执行完毕的实施计划,记录的是当时怎么拆步骤,不是现状。结论去向:`crates/bw-engine/src/codehub.rs` 源码与 `docs/buddy/standards/connectors.md`。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Phases use checkbox (`- [ ]`) syntax for tracking.
>
> **本件自足**:不读对话也能接手。所有锚点(文件:行号)已核实。V3 规划收敛见 `three-step-plan-2026-07-28-v3.html` 第六章;本件是步1 的落地执行版。

## Goal

打通 codehub 对接:让 maas codehub 仓能被 buddy **导入 + 实采呈现**(issue/MR 计数点亮)。步1 = 纯脚本活,不靠 agent(三件套真跑、PR 验收环留步3)。

## 已锁定的设计决定(grilling 共识)

1. **接口层 = `Remote` enum + `remote_for` 工厂**(对标 Java `interface RemoteRepo + 2 impl + 工厂`)。provider 分叉收敛在工厂一处 + `Remote::xxx` 每方法一处 `match`(编译期穷尽,加 provider 漏改报错)。
2. **远端身份 = 统一 `(host, path)` 二元组**:不是「github 不需要 host」,是当时把 `github.com` 隐式默认、漏存一列。
   - `github_remote` → 改名 `remote_path`(TEXT NOT NULL,空=无远端)
   - 新增 `remote_host`(TEXT NOT NULL DEFAULT `'github.com'`,存量 github 项目迁移回填 `github.com`)
   - 每条远端均匀 `(host, path)`;`provider` 字段分叉;7 处 gate 改读 `remote_path.is_empty()`
3. **步1 建这 6 件**:`clone_repo` / `probe` / `collect_codehub_count` / collect arm(`kind="codehub"`)+ 公共指标 seed / `create_issue`。PR 环(`open_mr`/`merge_mr`/`issue_state`/`close_issue`)留步3。
4. **trio 注入 OK**:导入 maas 即在 maas 仓建 3 张标配卡(对称 github),不用 throwaway 测试仓绕。
5. **github 零影响是硬 gate**:P1-P2 纯重构,github 路径行为逐字段不变——深链一个 github 项目 + sqlite 读回**证明**,不是嘴上"理论没影响"。
6. **codehub.rs 走 shell-out `codehub-cli`**(不是 Rust 直调 GitLab v4 HTTP API)——CLI 是 GitLab v4 同构封装、默认 JSON 输出、token 在 keyring、零 Rust HTTP 依赖,与 `github.rs` shell-out `gh` 对称。

## 关键事实(已核实)

- **codehub-cli**:v1.3.4,装在 `C:\Users\<你>\bin\codehub-cli.exe`(已在 PATH)。profiles:green→`codehub-g.huawei.com`、open→`open.codehub.huawei.com`(内源);token 存 Windows 凭据管理器(keyring)。yellow profile 建了但 token 没存(不管)。
- **maas = 内源仓**:`innersource/AI-Coding_G/maas` @ `open.codehub.huawei.com`,project_id=1912112。SSH:`ssh://git@szv-open.codehub.huawei.com:2222/innersource/AI-Coding_G/maas.git`。
- **CLI 端到端已验证**:`codehub-cli project view -p innersource/AI-Coding_G/maas` 回完整项目 JSON;`codehub-cli issue list -p <repo> --state opened -l 3` 回真实 issue 数组(iid 15…)。collect 路径(`issue list`/`mr list` 分页计数)真通。
- **CLI 行为**:不带 `-H` 也能查到 maas(按 profile/keyring 解析),但绿/黄/内源三 host 同名 path 会抛 `AmbiguousRemoteError` → **codehub.rs 必须显式带 `-H <host>`** 按项目存的 host 走。认证优先级:`--token > CODEHUB_TOKEN env > auth login(Keychain)`。Rust 侧不管 token(Keychain)。
- **`provider` 字段已持久化**:`schema.sql` `provider TEXT NOT NULL DEFAULT 'github'`;存量项目默认 `github` → Remote 工厂 `match` 默认走 Github 路径,零迁移兼容。
- **只有 1 个默认 seed 的 metric**:`seed_stage_done_metrics`(每阶段「完成 Issue 数」,`kind=Bw`)。**V3 说的「公共指标 issue/MR 默认有、不用定义」是没实现的愿景**——P5 要把它建出来。
- **call-site 真实计数(不是 40)**:`bw_engine::github::xxx()` 调用点 13 行,其中 3 行纯本地 git(provider 无关、共用),**真正要按 provider 分发 = 10 处**;步1 碰 6 处,步3 碰 4 处(PR 环)。另有 7 处 `github_remote` 字段读(改 `remote_path.is_empty()`)+ app-desktop 三屏。

## 同构范本 / 锚点

- **`crates/bw-engine/src/github.rs`**(660 行,无状态 + `gh` shell-out):codehub.rs 同构范本。能照搬(纯本地 git,provider 无关):`issue_branch`:278 / `checkout_issue_branch`:292 / `push_head`:113 / `sync_default_branch`:156 / `expand_query`:585 + `days_ago_iso`:612。改 CLI:`create_repo`:73 / `clone_repo`:199 / `create_issue`:245 / `open_pr`:317 / `pr_state`:396 / `merge_pr`:415 / `issue_state`:440 / `close_issue`:458 / `list_repos`:529 / `collect_github_count`:630 / `probe_repo`:121。
- **`crates/bw-engine/src/metrics_file.rs`**:`.bw/metrics.toml` 解析器,`CollectKind` enum(Github/Connector/Bw/Manual)。P5 加 `Codehub` 变体?或走 Connector 路径——P5 看真实再定。
- **`crates/bw-app/src/lib.rs`** call-site: `probe_connector`:1955(GITHUB_REPO arm 2018 / `probe_repo` 2026)/ `sync_issue_to_github`(`create_issue` 2083,gate 2138)/ `collect_project_metrics`(github arm 2290-2297)/ `seed_standard_issue_trio`:2129(gate 2138)/ `CreateProject`(`create_repo` 3145 / `clone_repo` 3247 / `list_repos` 3390)/ `run`(`checkout_issue_branch` 2925 / `open_pr` 2964)/ `MergeIssuePr`(`merge_pr` 5300 / `issue_state` 5323 / `close_issue` 5326 / `sync_default_branch` 5351)/ `CompleteCreation`(`push_head` 3658)/ `tick_scheduler`:2405(collect cron)。
- **`crates/bw-core/src/model.rs`:~1803** `CONNECTOR_KIND_*`(GITHUB_REPO const ~1812)——P4 加 `CONNECTOR_KIND_CODEHUB_REPO`。
- **`crates/bw-store/src/schema.sql`**(projects 表 `provider` :36, `github_remote` 待改名)+ **`sqlite.rs`**(`set_github_remote`:656 / provider bind 501 / SELECT 1080,1091 / struct 2646)。
- **`crates/app-desktop/src/screens/connector_hub.rs`**:connector syncable UI——**对齐 github-repo 不做**(github-repo 也不 syncable,codehub-repo 同样,非回归,不留 TODO)。
- **CodeHub API 范本**:`D:\2026\code\maas\governance\workspace\codehub\codehub_client.py`(stdlib urllib,GitLab v4 + PRIVATE-TOKEN,已验证)——codehub.rs 不直调,但端点形状/字段(`iid`/`web_url`/分页 `per_page+page` 默认 20 静默截断)可参照拼 CLI 命令。

## 门禁(每 phase commit 前全过)

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop
```
行为靠 E2E(深链启动 + sqlite 读回)+ `/code-review`,不靠单测。启动 buddy:`cargo run -p app-desktop`(别直接跑 exe,崩 0xC0000135)。深链:`BW_OPEN=<项目> BW_PANEL=issues cargo run -p app-desktop`,stderr `[BW_OPEN]` = 渲染证明。

## 执行节奏

- **P1-P5 全部开发完毕**后再交用户做 codehub live 验证(录入 maas)。
- P1/P2/P3 = 开发者自验(cargo + read-only CLI 交叉核对);P4/P5 = 代码写完 + cargo 绿,**live 验证(录 maas、看渲染、看计数)留用户**。
- P5 内部细节(公共指标 seed 具体哪几条、codehub 查询 DSL)等 P4 用户导入后看真实情况再定口径——**代码骨架先搭,口径后填**。

---

## P1 · 地基:schema 改名 + 加列(纯重构,不碰 codehub)

**Files:**
- `crates/bw-store/src/schema.sql`(projects 表:`github_remote` → `remote_path`;新增 `remote_host TEXT NOT NULL DEFAULT 'github.com'`)
- `crates/bw-store/src/sqlite.rs`(`add_column_if_missing("remote_host", ...)` 迁移守卫 + 存量 `github_remote` 回填 `remote_host='github.com'` UPDATE;INSERT/SELECT 列名改;`set_github_remote` → `set_remote_path`(+ `set_remote_host`?或合并);struct 字段改名)
- `crates/bw-store/src/lib.rs`(trait 方法改名)
- `crates/bw-core/src/model.rs`(Project struct 字段改名)
- `crates/bw-app/src/lib.rs`(7 处 gate `github_remote.is_empty()` → `remote_path.is_empty()`;`set_github_remote` 调用点)
- `crates/app-desktop/src/{kernel,create,op}.rs` + examples(字段引用改名)

**偏差留痕:** `set_github_remote` 改名是 trait 方法变更,bw-app 调用处一并改;若决定保留方法名只改语义,留 commit message「偏差」段说明。

- [ ] Step 1: 改 `schema.sql`(改名 + 加列)+ `sqlite.rs` 迁移守卫(`add_column_if_missing` + 存量回填 UPDATE)
- [ ] Step 2: 改 `model.rs` Project struct + `sqlite.rs` INSERT/SELECT/struct + `lib.rs` trait
- [ ] Step 3: 扫 `bw-app/lib.rs` 7 处 gate + `app-desktop` 三屏 + examples 改名
- [ ] Step 4: 门禁全绿 + E2E:**老 DB 打开 `PRAGMA table_info(project)` 见 `remote_path`/`remote_host` 且 github 项目 `remote_host='github.com'`;深链一个 github 项目 + sqlite 读回 `remote_path/remote_host` 跟今天逐字段一致** → github 零影响证明

---

## P2 · 地基:Remote enum + remote_for 工厂(纯重构,github-only)

**Files:**
- Create `crates/bw-engine/src/remote.rs`(`enum Remote { Github(String), Codehub { host: String, path: String } }` + `impl Remote { async fn create_issue/clone_repo/probe/collect_count/list_repos/... }`,每方法一处 `match`,Github 臂调 `github::xxx`、Codehub 臂暂 `todo!()` 不可达)
- `crates/bw-app/src/lib.rs`(`async fn remote_for(&self, proj) -> Result<Remote>` 工厂,match `proj.provider`;6 个 call-site:`probe_connector`/`sync_issue_to_github`/`collect_project_metrics`/`CreateProject`(clone+create+list)/`CompleteCreation`(push_head 纯 git 可不走 Remote)改成 `self.remote_for(&proj).await?.xxx()`)

- [ ] Step 1: 建 `remote.rs`(`Remote` enum + 方法,Github 臂真接、Codehub 臂 `todo!`)
- [ ] Step 2: `bw-app` 加 `remote_for` 工厂 + 6 call-site 改走 Remote
- [ ] Step 3: 门禁全绿 + E2E:github 项目建 issue / 采计数 / 探针 / clone **行为与今天完全一致**(走 Remote);`/code-review`

---

## P3 · codehub.rs 真通(shell-out codehub-cli)

**Files:**
- Create `crates/bw-engine/src/codehub.rs`(`CodehubClient { host, path }` 极薄,token 不管;`clone_repo`/`probe`(project view)/`collect_codehub_count`(issue·mr list 分页累加)/`create_issue`,全 shell-out `tokio::process::Command::new("codehub-cli")` 带 `-H host -p path`,JSON 输出 `serde_json::deserialize`)
- `crates/bw-engine/src/lib.rs`(`pub mod codehub;`)
- `crates/bw-engine/src/remote.rs`(`Remote::Codehub` 臂从 `todo!` 换成调 `codehub::xxx`;`remote_for` 工厂接 `Remote::Codehub { host: proj.remote_host, path: proj.remote_path }`)

**CLI 命令锚**(已验证):
- clone:`codehub-cli repo clone <path>`(或 clone URL;嵌 token?CLI 走 keyring,Rust 侧不嵌)
- probe:`codehub-cli project view -p <path> -H <host>`
- collect:`codehub-cli issue list -p <path> -H <host> --state opened -l 0`(全量,计数=array length);`codehub-cli mr list -p <path> -H <host> --state merged ...`(本周合入需日期过滤,P5 定口径)
- create_issue:`codehub-cli issue create -p <path> -H <host> --title ... --description ...`

- [ ] Step 1: `codehub.rs` 4 个函数 shell-out + JSON 解析
- [ ] Step 2: `remote.rs` Codehub 臂接上 + 工厂接上
- [ ] Step 3: 写 example/bin 交叉核对:从 Rust shell-out → 解析 → 回 maas project info + issue 计数;**计数与裸跑 `codehub-cli issue list -p innersource/AI-Coding_G/maas --state opened -l 0 | jq length` 对得上**;`/code-review`

---

## P4 · 接入 maas:CreateProject + UI + connector

**Files:**
- `crates/bw-app/src/lib.rs`(`GithubOrigin` 泛化成 `RepoOrigin` 加 codehub 变体,或并列 `CodehubOrigin{host,path}`;`CreateProject` 加 codehub 分支:clone maas 进 workspaces → `set_remote_path`+`set_remote_host` → 建 `CONNECTOR_KIND_CODEHUB_REPO` connector(config 存 `host:path` 或 JSON);`CompleteCreation` 触发 `seed_standard_issue_trio` → 调 `Remote::Codehub.create_issue` 真在 maas 建 3 issue)
- `crates/bw-core/src/model.rs`(加 `CONNECTOR_KIND_CODEHUB_REPO` const + `Connector` 文档)
- `crates/bw-app/src/lib.rs` `probe_connector`(加 `CONNECTOR_KIND_CODEHUB_REPO` arm → `Remote::probe`)
- `crates/app-desktop/src/screens/create.rs`(平台选择器点亮 codehub 项;connector syncable 对齐 github-repo 不做)

- [ ] Step 1: model.rs 加 `CONNECTOR_KIND_CODEHUB_REPO`
- [ ] Step 2: `CreateProject` codehub 分支 + origin 泛化 + connector mint
- [ ] Step 3: `probe_connector` 加 codehub arm + `create.rs` 选择器(connector syncable 对齐 github-repo 不做)
- [ ] Step 4: 门禁全绿;**live 验证留用户**(录 maas,见下)

> **P4 用户 live 验证(交用户做):** `cargo run -p app-desktop` → 「接入已有 codehub 仓」导 maas → 看 clone 落地、connector Connected、深链 issues 屏渲染、**maas 仓里真建了 3 张标配卡**。sqlite 读回项目 `remote_path/remote_host` + connector + 3 issue 的 codehub iid。看真实渲染缺口反馈步2。

---

## P5 · 实采呈现:collect arm + 公共指标 seed + probe

**Files:**
- `crates/bw-app/src/lib.rs` `collect_project_metrics`(加 `kind="codehub"` arm:shell-out `collect_codehub_count`,window-guard/change-guard 同 github arm)
- `crates/bw-engine/src/metrics_file.rs`(`CollectKind` 加 `Codehub` 变体?或公共指标走 Connector 路径——**P4 用户导入后看真实再定**)
- `crates/bw-app/src/lib.rs` 加默认公共指标 seed(对镜像 `seed_stage_done_metrics` 幂等套路:给有 codehub 远端的项目 seed open issues / 本周合入 MR 两条 metric,`kind="codehub"`)
- `probe_connector` `CODEHUB_REPO` arm 已在 P4 建,P5 补采集喂指标

- [ ] Step 1: `collect_project_metrics` 加 codehub arm + 公共指标 seed 骨架(口径留 TODO,P4 后填)
- [ ] Step 2: 门禁全绿;**live 验证留用户**:触发采集 → sqlite 读回 observation 行有真计数 → 指标屏 issue/MR 灯亮(非 Unknown)→ **计数与裸 CLI 交叉对得上**

---

## 收敛后

步1(P1-P5)开发完毕 + 用户 live 验证通过 → 进入 V3 步2(check buddy 可视化全貌,看 maas 在 buddy 呈现什么/缺啥)+ 步3(三件套真跑,卡网关)。步3 才补 codehub.rs 的 PR 环(`open_mr`/`merge_mr`/`issue_state`/`close_issue`)——它们卡在 agent run 后面,步1 不建。
