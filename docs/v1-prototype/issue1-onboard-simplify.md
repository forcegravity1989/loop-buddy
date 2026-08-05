# V1 Issue 1 · 项目纳入简化 + connector 心智模型归位 — 开发事实源

> 走 (c):现有 buddy app 当底简化。本文是 phase1+phase2 开发的唯一事实源(SubAgent 照此建)。
> 5 步法:scope delta ✅ → 对齐原型(本文)→ 开发 → 验证 → 填指南。Issue 已提(Issue 1)。

## 0. 心智模型(三层,用户 2026-08-04 钉下)

1. **connector = 对外连接器**:基础实体,提供「探活函数」(知道怎么连某外部系统 + 探一次看通不通)。如 codehub-cli/github/claude-cli 连接器。**本地工作区读取不是对外连接器**(→ `git-repo` 不该是 connector)。
2. **业务脚本**:干业务活的脚本,**可以基于某 connector**(也可不基于,不强绑),被定时器调。
3. **定时器(cron)**:调度组件,**定时调用业务脚本**(cron→script,script 可选→connector)。

**buddy 现状对照**:三层 buddy 都有,但是**半成品**——commit `fa2e3bb "18-③script-connector"`(用户自己 plan/18)建了 `script` connector kind + collect arm,但:① `git-repo` 错位成 connector(本地不是对外);② `script` 被塞进 connector 表(本该是业务脚本实体,「求同存异」接受它留在 connector 表当一种 kind);③ 采集 cron 不调脚本、inline 调 gh/codehub-cli(`metric.collect_kind='codehub'/'github'`),`connector` collect_kind **留白**(没做完的意图物证)。**phase2 = 收尾自己开的半成品,不是大改。**

## 1. 现状(5 卡)+ 命令层事实(取证,见 commit 锚点)

创建流 5 卡(`crates/app-desktop/src/screens/create.rs`):Repo→Intent→Questions→Drafting→Review。
- `CreateProject`(lib.rs:4699):建 project 行 + clone/`gh repo create` + 建 2 connector(`git-repo`+provider)+ 每日 `CollectMetrics` cron + charter/standards(仅 owned)。
- `CompleteCreation`(lib.rs:5280):建 5 `op_stage`(schedule=cadence)+ `seed_standard_issue_trio`(remote 非空才建,Backlog)+ push remote + probe connector +(run_first→跑①竞品分析)。
- cycle(`explore`)/cadence(`Weekly`)均**展示性**,不触发真 cron。唯一真 cron = CreateProject 那条 Daily `CollectMetrics`。
- Drafting 纯 mock,产出不进 Review,砍安全。
- **砍 Questions+Drafting+Review 指标+run_first 对下游无损**:三件套照常 Backlog,cycle/cadence 用 DB 默认。

## 2. codehub 能力取证(实跑 codehub-cli v1.3.4)

- CLI 有 `project create`(新建仓,需 `--namespace-id`)+ `project list --mine`(列我的仓,对应 `gh repo list`)。buddy `bw-engine/src/codehub.rs` 只接了 `clone_repo` → 割裂根因是代码没接,不是 CLI 不行。
- 三域名:`green`(codehub-g)/`open`(open.codehub,内源,默认)/`yellow`(codehub-y)。token 走 keyring(green/open 已登录,yellow 未登录)。`-H green/open/yellow` alias 指定 host。
- `list --mine` 实跑:open 返 16 仓(含 `innersource/AI-Coding_G/maas` 样本)、green 返 40 仓。**覆盖用户真实仓**。
- `is_owned_workspace`(workspace.rs:314)查 root commit 作者=="Builders' Workbench"。**接入已有仓(maas)不是 owned → charter/standards 不写**(磁盘验证:maas `.claude/` 无 standards/PROJECT.md)。→ codehub 新建仓后须让 BW 写 root commit 才 owned。

## 3. phase 拆分(本 issue 两 commit)

### phase 1 · 创建瘦身(UI + 引擎 shell-out)
- **砍中间卡**:Questions+Drafting+Review 指标+run_first。→ 2 卡:地址→意图。
- **Intent 卡**:名称* + kind(留) + brief(不强制) + 对标 benchmark(不强制,竞品分析输入) + 成功标准 win(不强制,找指标输入)。cycle=`explore`/cadence=`Weekly` 用 DB 默认。
- **提交即连发**:`CreateProject`(建仓+connector+cron+charter)→ `UpdateBrief`(benchmark,opportunity)→ `CompleteCreation`(stage+三件套+push+probe)。建仓/落地进度走 `ActionsBanner`,失败可重试。
- **统一 github/codehub 地址 UI**:去 GitLab/Gitcode 占位;两边「新建/接入」chip 对称;codehub host 选择器(green/open/yellow,yellow 未登录如实标);codehub 接入已有用 `list --mine` 下拉(**不留手填 fallback**;仓不在列表=需先成为 member,如实约束)。
- **codehub 两臂**(引擎 shell-out,对仗 github):`codehub.rs` 加 `create_repo`(`project create`,默认建到个人 namespace 对标 `gh repo create`)+ `list_repos`(`project list --mine`);`remote.rs` + `lib.rs` CreateProject codehub 新建分支 + `Command::ListCodehubRepos` + `CodehubRepoSummary` VM。

### phase 2 · connector 心智模型归位(按 §0 三层)
- **不建 `git-repo` connector**(本地不是对外连接器)。`evidence::collect` + `feed_workspace_metrics` + `sync_project_assets` 逻辑搬去「工作区探活」(创建时调,但不建模成 connector 行)。
- **provider-repo connector 留**(codehub-repo/github-repo = 对外连接器+探活,正是 §0 第 1 层)。
- **建 1 个 `script` connector**(buddy 自带采集脚本,command 跑 codehub/github CLI 输出 JSON `{"open_issues":N,"merged_mrs":M}`)——这是 §0 第 2 层业务脚本(buddy 自带 instance)。
- **2 默认 metric 改 `collect_kind='script'` + `collect_query` 改字段路径**(`open_issues`/`merged_mrs`)——cron 已有 script arm(plan/18-③),现成收尾。
- **`cron_task` 不动**。
- **求同存异(留,不矛盾)**:`op_stage.routine_schedule`(喂 signal 过期降级,砍要碰派生链,心智上承认定时主归 cron、这列辅助);5 条 `stage_done` metric(机器喂的过程指标,不走采集链);charter+standards(留,质量打磨是依赖事项,本窗口不管内容到位)。

## 4. 具体例子:maas(codehub)创建后

**改前/改后表行**:
```
cron_task(不变): name='maas · 指标采集' schedule='daily' mode='collect_metrics' project_id=<maas> last_run_at=0
metric「开放 Issue 数」: 改前 collect_kind='codehub' collect_query='issues:opened' → 改后 'script' 'open_issues'
metric「已合入 MR 数」: 改前 'codehub' 'mrs:merged' → 改后 'script' 'merged_mrs'
connector: 改前 git-repo+codehub-repo → 改后 codehub-repo + script「codehub 仓统计」(git-repo 不建)
```
**tick(改后)**:cron Daily → `collect_project_metrics` → script arm 预跑「codehub 仓统计」connector → 输出 JSON → metric 按 `collect_query` 字段路径取值 → `append_observation(source=Script)` → `recompute_signals`。
**落点**:值在 `observation`(append-only,同窗口同值跳过);signal 是 `metric`/`op_stage`/`project` 缓存列(无 signal 表),`recompute_signals` 唯一写。
**界面/感知**:创建完项目墙出现 maas + 连接 green;两指标先 Unknown 灰(无数据≠绿);每日 cron 后有值(6/9)信号灯亮。用户感知「buddy 自带每天帮我数 issue/PR」;业务级指标(找指标 skill)用户自己来,两层不干扰。

## 5. 文件级改动清单 + 契约(SubAgent 建到此)

### 引擎+handler(SubAgent A)
- `crates/bw-engine/src/codehub.rs`:
  - `pub async fn create_repo(host, namespace, name, visibility, dest) -> Result<CodehubRepoRef, CodehubError>`:调 `codehub-cli -H <host> project create --name <name> --visibility <vis> --namespace-id <nsid>`(个人 namespace-id 由 `codehub-cli user`/`project list --mine` 解析,SubAgent 实测钉字段)→ 拿 ssh_url → raw `git clone` → `commit_initial` 写 BW root commit(owned)。
  - `pub async fn list_repos(host, limit) -> Result<Vec<CodehubRepoSummary>, CodehubError>`:调 `codehub-cli -H <host> project list --mine --limit N --jq`,解析 `path_with_namespace`/`visibility`/`default_branch`/`pushed_at`/`description`。
  - `CodehubRepoSummary { path, visibility, default_branch, pushed_at, description }`(对仗 `GithubRepoSummary`)。
- `crates/bw-engine/src/remote.rs`:无需改(Remote 已有 Codehub arm;create/list 不经 Remote,直接 shell-out,同 github.rs 范式)。
- `crates/bw-app/src/lib.rs`:
  - `CodehubOrigin` 改 enum:`New{host,namespace,name,visibility}` + `Existing{host,path}`(原 {host,path} → Existing)。
  - `Command::CreateProject.codehub: Option<CodehubOrigin>` 类型不变,handler 分 New/Existing:Existing→`clone_repo`(现状);New→`codehub::create_repo`→`set_remote`(host/path)。
  - `Command::ListCodehubRepos{host}`(新,对仗 `ListGithubRepos`)+ handler 调 `codehub::list_repos`。
  - `CodehubRepoSummary` VM 暴露给 UI(对仗 `GithubRepoSummary` 经 `RunVm`/事件回流)。
  - CreateProject:① **删建 `git-repo` connector** 那段;② 建 provider-repo connector(现状);③ 建 1 个 `script` connector(buddy 自带采集脚本,config={command: codehub/github CLI 仓统计, output JSON});④ cron 不变。
  - `seed_codehub_public_metrics`:2 条 metric `collect_kind` 从 `'codehub'`→`'script'`,`collect_query` 从 `'issues:opened'`/`'mrs:merged'`→`'open_issues'`/`'merged_mrs'`。github 侧如有类似 inline arm 同步改(查 `seed_*` 现状)。
  - CompleteCreation 连发时序:Intent 提交后 CreateProject→UpdateBrief→CompleteCreation(SubAgent 钉时序,CreateProject handler 末尾或 kernel 事件触发 CompleteCreation;务必 clone 成功后才 CompleteCreation)。
- `crates/ui/src/vm.rs`:`CodehubRepoSummary` VM(对仗 `GithubRepoSummary` 的 UI 投影)。

### UI(SubAgent B,基于 A 的契约)
- `crates/app-desktop/src/screens/create.rs`:
  - `Card` enum 砍到 `Repo`+`Intent`。删 QuestionsCard/DraftingCard/ReviewCard/MetricDraft/MetricList/ns_candidate/dispatch_draft_run。
  - `RepoCard`:platform chip 去 GitLab/Gitcode(只 GitHub/CodeHub);起点 chip 新建/接入(两边都有);github 新建(slug+可见性,现状)+ 接入(`gh repo list` 下拉,现状);codehub host 选择器(green/open/yellow,yellow 未登录灰置)+ codehub 新建(namespace+name+可见性)+ codehub 接入(`list_repos` 下拉)。
  - `IntentCard`:name* + kind + brief(不强制)+ benchmark(不强制)+ win(不强制);「确认建立」按钮连发 CreateProject+UpdateBrief+CompleteCreation。`can_send` 只 gate name 非空(+ codehub path/namespace 非空)。
  - `ActionsBanner` 留(建仓/落地进度回显)。
- `crates/app-desktop/src/main.rs`:`ListCodehubRepos` 事件接线 + codehub repos 信号传 RepoCard(对仗 github_repos)。

### 指南(SubAgent C,验证后)
- `docs/guide/buddy-guide.html`:
  - 使用指导 u2「纳入项目」:三段式(操作=项目墙→新建→填地址+名称+brief/对标/成功标准→确认;结果=系统×CRUD 身后事务,见 §4;看到=项目墙 green+三件套 Backlog+指标 Unknown→有值)。**预留截图位**(`<img>` 指向 `docs/guide/img/u2-*.png`,验证时填)。
  - 维护指南机制章 m2「Hub 全局库」:更新 connector 概念卡(三层模型:对外连接器/业务脚本(buddy自带+业务级)/本地工作区探活)、cron 概念卡(定时调脚本)、script 概念卡。**特性组件介绍章与使用阶段解耦,example 关联使用阶段**。m6「指标与健康」:metric 表存定义不存值(observation)+ collect_kind/collect_query 语义表。
  - 诚实口径:无数据=Unknown≠绿;Done 永不自动;数字 sqlite3 可查。

## 6. 偏差 / 未决(记不擅定,commit 偏差段如实)

- **bug① UI 冻死**:codehub clone 同步堵单线程命令循环,Intent 提交后 UI 卡。本 issue 砍中间卡不解 bug①。practice skill §4.3 独立未决项。
- **connector 归位未竟**:phase2 只做创建侧(不建 git-repo + 建 script + 改 metric collect_kind)。`collect_project_metrics` 的 inline github/codehub arm 本 issue **不动**,标注后续可砍,留采数/总览窗口。
- **collect_kind 顶层两 kind(2026-08-04 找指标/绑数据窗口 grilling 精炼;本 issue 代码不动,仅文档/指南口径对齐)**:顶层 `collect_kind` 只剩 `script|manual` 两 kind。github/codehub/bw/connector 不是并列 kind,是 script 的不同 instance(脚本包 codehub/github CLI,"是否依赖某 connector"是 script instance 子属性);`collect_query` 统一=字段点分路径。**codehub 不是面向用户的 kind**:CollectKind 枚举(metrics_file.rs:40)无此变体,`.bw/metrics.toml` 写 `kind="codehub"` serde 解析失败零写入,UI MetricCard(op.rs:1912)徽记 match 无 codehub 臂——DB 里只是 legacy inline arm 字符串,该彻底退休不是留兼容。本 issue 已把 2 条 codehub metric 改 `collect_kind='script'`(方向对齐);inline arm 迁 script + CollectKind 枚举清理归采数/总览窗口。绑数据 scope 升级:不只改 metrics.toml collect 字段,是搭 script connector(包 CLI 或项目侧 derive 脚本)+ 落标准目录 + 挂 cron + 登记进 buddy——与 phase2 建 script connector 方向一致,范围更大,归绑数据窗口。
- **codehub 新建 namespace-id**:默认建到个人 namespace(对标 gh repo create)。group namespace 选择留口(V1 不做,如实标)。SubAgent 实测 codehub-cli 钉 namespace-id 取法。
- **op_stage.routine_schedule / stage_done**:求同存异留。signal 过期降级读这列的改法留总览窗口(碰派生链)。
- **standards 质量**:留写入,内容打磨是依赖事项,本窗口不管。
- **yellow 未登录**:host 选择器如实标「未登录」。

## 7. 验证(step 4,读回为证)

- `cargo fmt --all --check` + `cargo clippy --workspace --exclude app-desktop -- -D warnings` + `cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features` + `cargo check -p ui --target wasm32-unknown-unknown` + `./scripts/guard-kernel-ui-free.sh` + `cargo check -p app-desktop`。
- **`cargo test`**(CI 跑,本地门禁漏)。
- 深链 `BW_OPEN=<项目名> BW_PANEL=create` + 截图 + `claude -p --model haiku` 读图 + `sqlite3` 读回(project/connector/cron_task/metric/issue 行)。

## 8. 事实源
现状代码:`create.rs`、`lib.rs`(CreateProject:4699/CompleteCreation:5280/CodehubOrigin:106/seed_codehub_public_metrics:2513/seed_standard_issue_trio:2963/probe_connector:2609/collect_project_metrics:3108/script arm:3316)、`codehub.rs`(clone_repo:377)、`github.rs`(create_repo:73/list_repos:742)、`workspace.rs`(is_owned_workspace:314)、`schema.sql`(metric:57/cron_task:290/connector:314/op_stage:97/observation:86)。
心智模型物证:commit `fa2e3bb 18-③script-connector`。
guide 目标态:`docs/guide/buddy-guide.html` u2(L270)/m2(L453)/m6(L539)、`docs/guide/填写规范.md`。
