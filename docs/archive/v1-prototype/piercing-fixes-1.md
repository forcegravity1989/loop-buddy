# V1 穿刺修复批次 1 · cowelink W1 穿刺 7 条反馈 — 开发事实源

> ⚠️ **历史档案(V1 穿刺修复批次 1 开发事实源,2026-08-20 归档)**。记录的是 cowelink 项目做 W1 穿刺后 7 条反馈的根因定位与修复设计(含项目指标卡片区 Round 3-5 最终定型),已落地。遗留问题现状以 [`docs/LEFTOVERS.md`](../../LEFTOVERS.md) 为准;产品现状以仓根 `CLAUDE.md` 为准。

> V1 三窗口(W1 纳入 / W2 找指标·绑数据 / W3 总览)已合入 v1(HEAD `a2b914c`)。用户用 cowelink 项目做 W1 穿刺,反馈 7 条问题。主编排已全部定位根因 + 修复设计 + 代码锚点。本文是该修复批次的**设计事实源**,供后续 dev/review SubAgent 照此建代码。
>
> 纪律:本文只写设计,不动代码。5 步法:scope delta ✅(本文)→ 开发 → 验证 → 填指南。逐 commit 不 push。门禁与铁律见 §5、§6。

## 0. 反馈清单(7 条,照用户口径)

| # | 反馈(用户口径) | 严重度 | 窗口 |
|---|---|---|---|
| 1 | GitHub/CodeHub 新建仓 UI 不一致——一边在第二步填仓名,一边在第一步填 | 中 | UI 一致性 |
| 3a | 定时器卡片看不明白(要具体,不要通用) | 中 | UI 可读性 |
| 3b | 连接器卡片分不清(对外连接器 vs 脚本连接器) | 中 | UI 可读性 |
| 4 | 总览看不到仓指标(值全「—」,采集时序竞态) | 高 | 数据正确性 |
| 5 | yellow 报错(host 选择器黄区 + toast 不自动清) | 中 | UX |
| 6 | 指南 U2:去两个已知坑 + 加竞品分析章节 | 低 | 指南 |
| 7 | 指南 U2:加创建后截图位 | 低 | 指南 |

> 编号沿用主编排反馈清单(缺 2,因 #2 在主编排口径里并入 #1,无独立项)。

## 1. scope delta(读现状取证,不基于猜)

### 点 1 · 新建仓 UI 不一致(现状)

- **GitHub 新建仓**:只选可见性,仓名在第二步 IntentCard 填(`create.rs` IntentCard 内 L924-939 有「GitHub 仓名(可改)」slug 输入槽;提交闭包 L798-861 用 `slug`/`slugify(&name())`)。
- **CodeHub 新建仓**:仓名在第一步 RepoCard 填(`create.rs` L372-398 namespace+name+可见性)。
- 两侧入口不对称:用户从 GitHub 进,第二步才见仓名;从 CodeHub 进,第一步就填仓名。

### 点 3a · cron 卡看不明白(现状)

- cron 名创建时叫「`<项目> · 指标采集`」太通用(`lib.rs:6033` `format!("{} · 指标采集", proj.name)`)。
- cron 卡(`cron_hub.rs` CronTaskRowView L112-228)对 collect_metrics 模式**无专属分支**——靠通用槽位 `c.mode_icon`(CronMode::icon 返 `"📈"`,L144)+ `c.mode_label` 渲染,没有「干什么」的可读行(RunSkill/RunPrompt 才有专属分支 L149-170)。
- `CronMode::CollectMetrics` 的 label 还是 GitHub 口径「采集指标(pull GitHub → 观测)」(`model.rs:1686`),与 codehub 也走这条的事实不符。

### 点 3b · 连接器卡片分不清(现状)

- 连接器卡(`connector_hub.rs` ConnectorCard L57-101)**直接显示原生 kind 字符串**(`{c.kind}`,L81):`codehub-repo` / `script` / `github-repo` / `claude-cli` / `git-repo`。用户分不清「对外连接器」vs「采集脚本」。
- **关键心智**:在 buddy 体系里,**脚本就是连接器**(kind=`script`,commit `fa2e3bb 18-③script-connector` 把它塞进 connector 表当一种 kind)——两个都是连接器,只是角色不同(对外连代码仓 vs 内部采集)。

### 点 4 · 总览看不到仓指标(现状 + 时序竞态取证)

- 截图实证:cowelink 项目指标条**在渲染**(开放 Issue / 已合入 MR / 阶段完成),值全「—」因为零观测。
- cowelink cron 状态 normal、collect 脚本 JSON **没生成**、脚本手动跑通(`{"open_issues": 3, "merged_mrs": 0}`)。
- **矛盾**:脚本若跑了,`|| echo 0` 兜底也会写 JSON;JSON 不存在 + cron normal = **脚本臂被跳过**。
- **时序**:cron 创建 08:21:32,cron 触发 08:21:34(+2s)。新 cron `last_run_at=0` → 第一 tick 立即触发,但此时 clone 还没完成、`workspace_path` 还是空的 → `collect_project_metrics` 脚本臂条件(`lib.rs:3704-3720` `!proj.workspace_path.trim().is_empty() && sigs.metrics.iter().any(|m| m.collect_kind == "script")`)不满足 → 跳过,记 normal → 下次 tick 等明天。

### 点 5 · yellow 报错(现状)

- 已与用户确认:创建时在 host 选择器(绿区/内源/黄区)点过黄区 + 刷新列表 → `ListCodehubRepos{host:yellow}` → codehub-cli 黄区未登录 → 原始 CLI 错误 → `ConnectorSynced` toast。
- toast 在 `main.rs:425-429` 渲染,**不自动消失**(只能点叉,`toast.set(None)` 在 onclose),留了 8 小时。
- 用户的仓是 open 内源,yellow 报错是外来的。代码里**无**自动探 yellow(codehub-repo 探活用 config host=open)。
- toast 设值侧在 `main.rs:208-219`(L211 定时任务 / L214 产物 / L218 connector 同步)。

### 点 6 / 点 7 · 指南 U2(现状)

- U2(`buddy-guide.html:270-324`)末尾 L317-320 有一个 `callout warn`「两个已知坑」:① UI 冻死 ② 竞品分析别点真跑。
- U2「得到什么」段(L307-315)四条纯文字,**无图**。

## 2. 修复设计(本批次做什么)

### 点 1 · 统一新建仓 UI(仓名都在 RepoCard 填)

**目标**:两边「新建仓」都在当前 UI(RepoCard)填仓名;仓名=仓库名,项目名=显示名(两者分离)。

- **GitHub 新建分支**(RepoCard github-new,`create.rs:440-520` 的 `is_new` 分支)加「仓库名」输入(像 codehub 那样),不再在 IntentCard 填。
- **IntentCard 去掉「GitHub 仓名」slug 槽**(L924-939 整段移除);`slug` signal 声明(L757)随槽一并退场。
- **`github_slug` 信号**在 create 流程顶层声明(对仗 `codehub_name`,L81-82 那块 signal 声明区),RepoCard 输入、IntentCard 提交闭包(L798-861)用。
- 提交闭包:`CreateProject` 的 `slug` 字段(L824-827 `slug: if slug().trim().is_empty() { slugify(&name()) } else { slug()... }`)改读 `github_slug` 信号;空时仍 `slugify(&name())` 兜底(项目名→仓名 fallback,与现状一致)。

### 点 3a · cron 卡具体化(不通用化)

用户明确要**具体化不要通用化**——卡要说清这个定时器就是采代码仓的 Issue/MR。

- ① **cron 名创建时改具体**:`lib.rs:6033` `format!("{} · 采集代码仓指标", proj.name)`(不是通用「指标采集」)。
- ② **cron 卡对 collect_metrics 模式显式显示一行具体描述**(`cron_hub.rs` CronTaskRowView L112-228 新增 collect_metrics 分支,对仗 RunSkill L149-170 的专属分支):文案「采集代码仓指标(开放 Issue / 已合入 MR)· 每日」,从该项目的 script-kind 指标名派生(读 metric 表 `collect_kind='script'` 的 name 列表拼进描述)。
- ③ **`CronMode::CollectMetrics` label 改中立具体**(`model.rs:1686`):`"采集指标(脚本 → 观测)"`(去掉 GitHub 口径,codehub 也走这条)。

### 点 3b · 连接器卡 kind→人话标签

- **kind→角色人话标签映射**(放 **ui 层**,不进 bw-core——见 §6 铁律):
  - `codehub-repo` → 「对外连接器 · 连 CodeHub 仓」
  - `github-repo` → 「对外连接器 · 连 GitHub 仓」
  - `script` → 「脚本连接器 · 采集 Issue/MR」
  - `claude-cli` → 「对外连接器 · claude CLI」
  - `git-repo` → 「本地工作区(legacy)」
  - 其余 kind 原样显示。
- 映射放 `crates/ui/src/vm.rs`(`connector_card` L1517 附近,新增 helper 或在 `connector_card` 内改 `kind` 字段为人话标签);卡片(`connector_hub.rs:81`)显示人话标签而非原生 kind。
- **注意**:VM 字段名 `kind` 是 `ConnectorCardVm.kind`(字符串),改它的值为人话标签即可,不动 `Connector` 实体的 `kind` 列(存储仍原 kind)。

### 点 4 · 修采集时序竞态(cron 不抢跑 + 创建即采一次)

- ① **cron 创建时 `last_run_at=now()`**(`lib.rs:6031-6041` cron 创建处,`NewCronTask` 的 `last_run_at` 字段填当前时间):不再立即触发,避免在 setup 前抢跑。
- ② **`CompleteCreation` 末尾(setup 全完成后)触发一次 `collect_project_metrics`**(`lib.rs:6317` CompleteCreation handler 末尾,probe connector 之后):让创建后指标立刻有值。可在 handler 末尾直接调 `self.collect_project_metrics(pid).await`(或发 `Command::CollectMetrics`)。
- 两者结合:cron 不抢跑 + 创建即采一次 → 新项目总览指标条不再全「—」。

### 点 5 · yellow 报错三管

- ① **host 选择器黄区未登录时禁点或预检**(`create.rs:580` `codehub_host_selector`):灰置 + 「未登录」标,如实(对仗 issue1 §6 yellow 标注)。
- ② **toast 自动清**(`main.rs:425-429` toast 渲染 / `main.rs:208-219` 设值侧):几秒后或下一个 note 来时清,非关键错误不留。机制:toast 设值时记一个 timeout(如 6s),timeout 到 `toast.set(None)`;新 toast 来则替换并重置 timeout。
- ③ **`ListCodehubRepos` 失败若错因含 credentials/token/secret → 映射人话**(`lib.rs:6089` handler):「`<host>` 域未登录:先本机 `codehub-cli -H <host> auth login`」。

### 点 6 · 指南 U2:去两个已知坑 callout + 加竞品分析章节

- ① **去掉 U2 L317-320 那个 `callout warn`**(整段移除——UI 冻死坑随点 4 时序修复一并缓解,竞品分析坑移到专属章节)。
- ② **加一个「竞品分析」小节/章节**(U2 末尾或紧跟):内容=暂不支持无法访问 claude webFetch 的机器环境,后续改造支持。(三件套之一竞品分析的现状说明。)

### 点 7 · 指南 U2:加创建后截图位

- 在 U2「得到什么」(L307-315)加 3 个 `<img>` 占位(指向 `docs/guide/img/u2-*.png`,gitignored 本地):
  1. 创建后侧边栏是资产(技能/智能体/工作流)
  2. 主页总览能看到项目代码仓指标
  3. 三个 issue
- 验证时补图(占位先留,验证阶段截图填入)。

## 3. 文件锚点(实际行号,SubAgent 建到此)

### `crates/app-desktop/src/screens/create.rs`
- RepoCard github 新建分支(L440-520,`is_new` 在 L459):加「仓库名」输入。
- RepoCard codehub 新建分支(L372-398):现状参考(不动,作为对称模板)。
- IntentCard 函数(L725):去掉 L924-939 「GitHub 仓名」slug 槽。
- IntentCard 提交闭包(L798-861,`CreateProject` 在 L838):`slug` 字段 L824-827 改读 `github_slug` 信号。
- `codehub_host_selector`(L580):yellow 未登录灰置 + 标注。
- 上层 `Create` 组件 signal 声明区(L81-82 `codehub_name` 等):加 `github_slug` 信号。

### `crates/bw-app/src/lib.rs`
- cron 创建处(L6031-6041,name 在 L6033):name 改「`<项目> · 采集代码仓指标`」;`last_run_at` 字段填 `now()`(避免抢跑)。
- `collect_project_metrics`(L3679,脚本臂条件 L3704-3720):不动(条件本身正确,是时序问题)。
- `tick_scheduler` CollectMetrics 臂(L4064):不动。
- `CompleteCreation` handler(L6317,probe connector 在 L6352):末尾触发一次 `collect_project_metrics`。
- `ListCodehubRepos` handler(L6089,`bw_engine::codehub::list_repos` 在 L6099):失败错因含 credentials/token/secret → 映射人话。

### `crates/bw-core/src/model.rs`
- `CronMode` 枚举(L1656-1674,`CollectMetrics` 在 L1673):不动枚举。
- `CronMode::label`(L1680-1688,L1686):改 `"采集指标(脚本 → 观测)"`。**安全**——它是 domain label,改字符串不影响状态机(见 §6 铁律)。
- `CronMode::icon`(L1695-1704,L1702 `"📈"`):不动。

### `crates/app-desktop/src/screens/cron_hub.rs`
- `CronTaskRowView`(L112-228,通用槽 L144 `mode_icon`/`mode_label`):新增 `collect_metrics` 专属分支(对仗 RunSkill L149-170),显示「采集代码仓指标(开放 Issue / 已合入 MR)· 每日」,指标名从该项目 metric 表 `collect_kind='script'` 派生。

### `crates/app-desktop/src/screens/connector_hub.rs`
- `ConnectorCard`(L57-101,显示 `{c.kind}` 在 L81):显示人话标签(VM 层 `kind` 字段已改人话,这里不动或只改显示文案)。

### `crates/ui/src/vm.rs`
- `connector_card`(L1517,`kind: c.kind.clone()` 在 L1527):加 kind→人话映射 helper,`kind` 字段输出人话标签(存储仍原 kind)。

### `crates/app-desktop/src/main.rs`
- toast 渲染(L425-429):加 auto-clear(设值时启 timeout,6s 后清;新 toast 替换重置)。
- toast 设值侧(L208-219,L211/L214/L218):设值时同时启 timeout。

### `docs/guide/buddy-guide.html`
- U2(L270-324):
  - L317-320 `callout warn`(两个已知坑):整段移除。
  - L307-315「得到什么」段:加 3 个 `<img>` 占位(`img/u2-result-*.png`)。
  - U2 末尾(L323 foot 之前或之后):加「竞品分析」小节(暂不支持 + 后续改造说明)。

## 4. 验证(读回为证)

- **门禁**:`cargo fmt --all --check` + `cargo clippy --workspace --exclude app-desktop -- -D warnings` + `cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features` + `cargo check -p ui --target wasm32-unknown-unknown` + `./scripts/guard-kernel-ui-free.sh` + `cargo check -p app-desktop`。**`cargo test`**(CI 跑,本地门禁漏)。
- **深链**:`BW_OPEN=cowelink BW_PANEL=progress cargo run -p app-desktop`(Windows 不跑 exe,崩 0xC0000135)→ stderr `[BW_OPEN]` = 渲染证。
- **sqlite 读回**:
  - `SELECT name,last_run_at,mode FROM cron_task WHERE project_id=(SELECT id FROM project WHERE name='cowelink');`(验 name 改具体 + last_run_at 非 0)
  - `SELECT metric_id,ts,raw FROM observation ORDER BY ts DESC LIMIT 5;`(验创建后即采一次,有值)
  - `SELECT name,collect_kind,signal FROM metric;`(验指标条数字)
- **读图**:`claude -p --model haiku "读取图片 <path> 并简短描述"`(主模型不看图)。截图 gitignored 本地 `docs/guide/img/`。
- **诚实口径**:无数据=Unknown≠绿;Done 永不自动;manual 戴徽;数字 sqlite 可查。

## 5. 偏差 / 未决 → 主编排决议(2026-08-05 拍)

SubAgent 列了 8 个拿不准点,主编排决议如下(dev SubAgent 照此建,仍可在 commit 偏差段记实操出入):

1. **点 3a 指标名派生来源 → VM 层**:`CronRowVm` 加 `collect_targets: Vec<String>` 字段,kernel 从该项目 `collect_kind='script'` 的 metric name 派生填入(对仗 `mode_label`/`mode_icon` 同源)。cron 卡显示。不在卡内联(卡只有 VM,无 store 访问)。
2. **点 3a name 改具体后存量项目 → 不回填**:存量项目(cowelink)cron 名保持旧「指标采集」(append-only 审计口径,不改存量);新建项目用新名「采集代码仓指标」。**关键**:cron 卡的 `collect_targets` 副标题对存量/新建都生效(从 metric 派生),所以存量项目卡片也会显示具体指标名——副标题才是主修复,cron 名改具体是次要美化。
3. **点 4 `last_run_at=now()` 字段类型 → i64 unix sec**:dev 用 `now().unix_timestamp()`(对仗 `tick_scheduler` 读 `last_run_at` 的逻辑,`cron_due` 比较的是 unix sec)。`NewCronTask` 的 `last_run_at` 字段填这个值。
4. **点 4 触发方式 → 直接调**:`CompleteCreation` 末尾(probe connector 之后)直接 `let _ = self.collect_project_metrics(pid).await;`(best-effort,失败不 block 创建,不发 `Command::CollectMetrics` 避免往返/toast——创建流有自己的 ActionsBanner 进度)。
5. **点 5 toast auto-clear → 所有 toast 8s 自动清**(简化,不区分关键/非关键):toast 设值时 spawn 一个 8s timer,到点 `toast.set(None)`;新 toast 来则替换并重置 timer。用户要解决的是「留 8 小时」,8s 足够;持久错误需求另开。不搞 credentials/token 判别(过度设计)。
6. **点 5 yellow 预检方式 → 不硬编码禁点,失败映射人话 + 保留现有警告**:yellow 保持可点(它是真实域,用户日后可能登录/有仓),不硬编码「未登录」(会过时)。修法=① `ListCodehubRepos` 失败若错因含 credentials/token/secret → 映射人话「`<host>` 域未登录:先本机 `codehub-cli -H <host> auth login`」;② host 选择器已有的 tooltip + 注保留(yellow 那条 p 文本);③ toast 8s 自动清(点 5)。三管后 yellow 报错=短暂 + 人话 + 不滞留。
7. **点 6 竞品分析章节位置 → U2 内小节**:竞品分析是三件套之一,U2 是纳入入口,放 U2 末尾(「3 张起手 Issue」之后)承接自然。不独立 section。
8. **点 7 截图文件名 → 按内容命名**:`u2-result-sidebar-assets.png`(侧边栏资产)/ `u2-result-overview-metrics.png`(主页总览指标)/ `u2-result-issues.png`(三个 issue)。语义清晰,不按序号。

**仍记不擅定**:dev 实操中若发现决议与代码相撞(如 `now()` 取法、`CronRowVm` 字段位置),按代码事实建,在 commit message「偏差」段如实记。

## 6. 铁律提醒(本批次边界)

- **UI 无关内核**:点 3b 的 kind→人话映射放 ui 层(`vm.rs`),**不进 bw-core**(guard-kernel-ui-free.sh 强制);点 3a 的 `CronMode::label` 在 bw-core(`model.rs`),它是 domain label,改字符串安全,**不影响状态机**(`CronMode` 无状态转移,只是展示 label)。
- **不动 schema**:本批次不改 `schema.sql`,不加字段。点 3b 人话映射是 VM 层派生,存储仍原 kind。
- **不动 Signal 派生**:点 4 修时序,不碰 `recompute_signals` / `Derived<Signal>` / store 写入路径。`collect_project_metrics` 仍走 append-only observation → recompute 老路。
- **不动状态机**:`CronMode` 枚举不动,只改 label 字符串。
- **Done 永不自动**:本批次不碰 Issue 状态机;点 4 的 `collect_project_metrics` 是采集,不是结算。
- **逐 commit 不 push**:本批次每个点(或每组相关点)独立 commit,代号前缀(如 `PF1-1 · 统一新建仓 UI` / `PF1-3a · cron 卡具体化` 等),信息如实描述取舍,不吹。偏差写进 commit message「偏差」段。

## 7. Round 2 修复(重启验证后两条反馈)

> Round 1 已落地(commits `4f38acf`..`2faa0c4`,设计事实源即本文 §1-§6)。用户重启 app 验证:点 1 ✅;点 2(cron 卡)与点 4(总览)仍有问题,反馈 round 2 两条。主编排已定位根因 + 修复设计,照实记于此。纪律同前:只写设计,逐 commit 不 push,门禁含 `cargo test`。

### R2-1 · cron 详情卡还通用 + 存量 cron 名不清楚

**根因(取证)**:Round 1 的 PF1-3a 只改了 cron **列表行**(`cron_hub.rs` CronTaskRowView `L171-188` 的 `is_collect_metrics` 分支),**漏了 cron 详情卡**(`crates/app-desktop/src/screens/component_detail.rs:405-411`)。详情卡那条「到点:」行还是通用模板:

```
到点:{c.mode_icon} {c.mode_label} · 目标「{target_display}」
```

对 collect_metrics 模式,`mode_label` = `CronMode::CollectMetrics` 的 label(PF1-3a ③ 已改中立「采集指标(脚本 → 观测)」,`model.rs` label),`target_display` 走 `c.target.clone()`(L390)——而 collect_metrics 的 `target` 列是空串,于是渲染出「到点:📈 采集指标(脚本 → 观测)· 目标「」」(通用 label + 空 target)。用户看不清这个定时器干什么。

另外:存量 cowelink 的 collect_metrics cron **存储名**还是旧「`<项目> · 指标采集`」(PF1-3a ① 只改新建名,§5 决议 2 明确不回填存量——append-only 审计口径)。但用户要存量卡的名字也清楚。用户还点出:后面会有业务指标采集和代码仓指标采集两种,代码仓这个要名字清楚以示区分。

**取证锚点**:
- `crates/app-desktop/src/screens/component_detail.rs:405-411`(详情卡通用「到点:」行,未分支)
- `crates/app-desktop/src/screens/component_detail.rs:374`(`c` 即 `CronRowVm`,来自 `hub.cron_tasks`——VM 已带 `is_collect_metrics`/`collect_targets` 字段,无需新字段)
- `crates/ui/src/vm.rs:1394`(`collect_targets: Vec<String>`)、`L1398`(`is_collect_metrics: bool`)——PF1-3a 已加
- `crates/app-desktop/src/screens/cron_hub.rs:171-188`(列表行 collect_metrics 分支,PF1-3a 已落地的模板)
- `crates/app-desktop/src/kernel.rs:856-867`(kernel build_vm 里给 collect_metrics cron 派生 `collect_targets` 的位置——R2-1② VM 派生名也加在这里)

**修复设计**:

① **详情卡 collect_metrics 专属分支**(`component_detail.rs:405-411`):`is_collect_metrics` 时显示具体「到点:📈 采集代码仓指标(开放 Issue / 已合入 MR)· 每日」——副标题文案与列表行 PF1-3a 对仗,指标名从 `c.collect_targets` 拼入(非空时 `采集代码仓指标({targets})· 每日`,空时兜底固定文案);**不显示空「· 目标「」」**(`target` 对 collect_metrics 无意义,`is_collect_metrics` 时整段 `· 目标「…」` 省略)。实现上对仗 `cron_hub.rs:171-188` 的 `else if c.is_collect_metrics` 分支:把详情卡 L405-411 的 `if !c.mode_icon.is_empty() { … } else { … }` 改成三分支(`is_collect_metrics` / `mode_icon` 非空 / 兜底),collect_metrics 分支用 `c.collect_targets.join(" / ")` 拼指标名。

② **存量 cron 名清楚**:让存量 collect_metrics cron 的**显示名**也变「`<项目> · 采集代码仓指标`」。两条实现路径,dev 钉(都 acceptable):
- **(a) VM 派生(倾向)**:kernel build_vm(`crates/app-desktop/src/kernel.rs:856-867`,已经在派生 `collect_targets` 的同一 `if matches!(c.mode, CronMode::CollectMetrics)` 块里)加一行 `row.name = format!("{} · 采集代码仓指标", project_name);`——`project_name` 从已声明的 `project_names`(L845-849)按 `c.project_id` 查。**存储名不动**(审计口径,与 §5 决议 2 一致),只覆盖 VM 显示;新建项目的存储名(PF1-3a ① 已改「`<项目> · 采集代码仓指标`」)与派生名一致,不冲突;存量/新建显示一致。
- **(b) 一次性数据迁移**:store 启动迁移,把 `mode='collect_metrics'` 且 name 像「· 指标采集」的 rename 成新名。比 (a) 重(动存量数据),且与 §5 决议 2「不回填存量」口径相撞——除非用户改口径。

**倾向 (a)**:无迁移、存量与新建显示一致、存储保留审计、改动最小(同一 `if` 块加一行)。

**偏差 / 未决**:
- 若用 (a),存量 cowelink 的**存储名**仍是旧「· 指标采集」,sqlite 读回 `SELECT name FROM cron_task` 会看到旧名,而 UI 显示新名——这是「VM 派生 vs 存储审计」的诚实差异,在 commit「偏差」段记。用户要的是「UI 看清楚」,存储审计不动;(b) 路径若日后用户改了口径再上。
- 详情卡副标题文案「采集代码仓指标(开放 Issue / 已合入 MR)· 每日」与列表行完全一致——刻意对仗,不是抄重。

### R2-2 · 总览项目指标条无数据 UX 看不清 + 缺立即采集入口

**根因(取证)**:用户重启后看 cowelink(存量)总览,项目指标条(`op.rs` `ProjectMetricStrip` / `StripItem`,`L1908-1977`)值全「—」。原因:存量 cron round 1 已抢跑被 PF1-4 ① 修掉(`last_run_at=now()`,不再第一 tick 触发),但**存量 cowelink 的 cron 是 round 1 之前建的**(存储 `last_run_at` 仍是旧值,已抢跑过一次),且 `CompleteCreation` 即采一次(PF1-4 ②)只对**新建**项目生效——存量项目没走 `CompleteCreation`,要等明天 cron tick 才有观测。于是存量 cowelink 总览此刻零观测,值全「—」。

无数据状态 UX 两个问题(`op.rs:1943-1977` StripItem):
- 值空时显「—」(`L1956` / `L1973`,mono 字体),在「开放 Issue — 〔脚本采〕」一行里「—」像分隔符不像「无值」;
- 徽记「〔脚本采〕/〔机器记〕」(`L1949-1950`,clay 色 `#4A6723` / `#5A4E7A`)与值挤在 `gap:5px`(`L1962`)里,糊。

且总览**无「立即采集」入口**:C7 立即采集按钮只在阶段视图 `ProgressStage`(`op.rs:2299-2303`,`onclick: move |_| k_collect.send(Command::CollectMetrics)`),存量项目总览没法手动触发采集看数据——只能等明天 tick。

**取证锚点**:
- `crates/app-desktop/src/screens/op.rs:1908-1939`(`ProjectMetricStrip`,条头「项目指标 · 代码仓级」在 `L1929`,footnote 在 `L1933-1936`)
- `crates/app-desktop/src/screens/op.rs:1943-1977`(`StripItem`,值空「—」在 `L1955-1959` 与 `L1973`,徽记在 `L1948-1954`,挤在 `L1962-1965`)
- `crates/app-desktop/src/screens/op.rs:2299-2303`(ProgressStage 的 C7「立即采集」按钮,R2-2① 对仗模板)
- `crates/bw-app/src/lib.rs:6798`(`Command::CollectMetrics` handler,调 `self.collect_project_metrics(p).await`,R2-2① 发到此)

**修复设计**:

① **项目指标条加「↻ 立即采集」按钮**(`op.rs` `ProjectMetricStrip` `L1908-1939`):条头或条尾加一个小按钮(对仗 ProgressStage `L2299-2303` 的样式:`background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:5px 12px;font-size:12px;`),`onclick` 发 `Command::CollectMetrics`(对 active 项目,handler 在 `lib.rs:6798`)。`ProjectMetricStrip` 当前只收 `intrinsic` + `active_stage`,需要拿到 `k_collect`(发送端,像 ProgressStage 那样 `use_context::<Kernel>()` 的 `k_collect` channel)——照 ProgressStage 的 `k_collect.send(Command::CollectMetrics)` 抄。这样存量 cowelink 点一下就有数据,不用等明天。

② **无数据 UX**(`op.rs` `StripItem` `L1943-1977`):
- 值空(`m.value_raw.is_empty()`,`L1955`)时显「待采集」(muted 色 `ink3`)不显「—」——「待采集」是「无值」的诚实口径,不像「—」会混成分隔符。`None`(指标未建)分支(`L1969-1975`)也显「待采集」,与 `Some` 但无值分支口径一致(都是「无数据」)。
- 徽记「〔脚本采〕/〔机器记〕」保留(诚实标来源),但与值之间间距从 `gap:5px` 加大到 `gap:8px` 或在徽记前加 `margin-left:4px`,确保和值有清晰分隔。文案可酌情精简成「脚本」「机器」(去「〔〕」括号,省字),但保留来源口径。
- footnote(`L1933-1936`,「只当现状数 · 不上卷健康……不点健康灯,不参与项目健康派生。来源徽:〔脚本采〕/〔机器记〕。」)长可酌情精简,但**保留「不点健康灯」口径**——这是诚实标注,不能丢。

**偏差 / 未决**:
- `ProjectMetricStrip` 当前签名 `(intrinsic: Vec<MetricVm>, active_stage: StageKind)`,加按钮要引入 `k_collect` channel——照 ProgressStage 抄,但 `ProjectMetricStrip` 调用处(`L1676`)需把 `k_collect` 传进来或在组件内 `use_context::<Kernel>()` 取。dev 选哪种实现都 acceptable,但**不能破坏 `ProjectMetricStrip` 的纯函数性**(ui 层 selector 不发 Command——按钮的 `onclick` 是 app-desktop 层的事,若 `ProjectMetricStrip` 在 ui crate 则不能直接发,需在 app-desktop 包一层)。**核查**:`op.rs` 在 `app-desktop`(不是 `ui` crate),所以可直接 `use_context::<Kernel>()` + `k_collect.send`,不违反 UI 无关内核铁律。dev 实操时确认 `k_collect` 在 `ProjectMetricStrip` 作用域可见,若不可见则照 ProgressStage 的取法抄。
- 「待采集」文案是 UX 用词,不是状态机口径——状态机层无「待采集」状态,只是 UI 对「无观测」的诚实显示(对仗「无数据=Unknown≠绿」)。

### §6 铁律延续(R2 批次边界)

- **UI 无关内核**:R2-1① 详情卡分支在 `app-desktop`(`component_detail.rs`),不进 bw-core;R2-1② VM 派生名在 `app-desktop`(`kernel.rs` build_vm,可读 store 的 kernel 桥层),**不进 bw-core**(guard-kernel-ui-free.sh 强制);R2-2 全在 `app-desktop`(`op.rs`)。R2-1① 用的是 PF1-3a 已加的 `CronRowVm.is_collect_metrics`/`collect_targets`(VM 字段,已在 `ui` crate),不新增内核字段。
- **不动 schema**:R2-1② 若用 VM 派生路径 (a),存储名不动,无 schema 改;路径 (b) 数据迁移走 store 但**不**改 `schema.sql`(只 rename 存量行),仍受 §5「不回填存量」口径约束——倾向 (a) 正是为避开口径相撞。
- **不动 Signal 派生**:R2-2 修 UX + 加采集入口,不碰 `recompute_signals` / `Derived<Signal>` / store 写入路径。`Command::CollectMetrics`(R2-2①)走老 handler(`lib.rs:6798` → `collect_project_metrics` → append-only observation → recompute),是已验证路径。
- **不动状态机**:R2 不碰 Issue / Cron 状态机。
- **Done 永不自动**:R2-2① 的「立即采集」是采集,不是结算——只写 observation,不推 Issue 状态。
- **逐 commit 不 push**:R2-1 / R2-2 各自独立 commit,代号前缀(如 `PF1-R2-1 · cron 详情卡具体化 + 存量名清楚` / `PF1-R2-2 · 总览项目指标条加立即采集 + 无数据 UX`),信息如实描述取舍,不吹。偏差写进 commit message「偏差」段。

### 验证(R2 批次,读回为证)

- **门禁**(与 §4 一致 + `cargo test`):`cargo fmt --all --check` + `cargo clippy --workspace --exclude app-desktop -- -D warnings` + `cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features` + `cargo check -p ui --target wasm32-unknown-unknown` + `./scripts/guard-kernel-ui-free.sh` + `cargo check -p app-desktop` + **`cargo test`**(CI 跑,本地门禁漏)。
- **深链**:`BW_OPEN=cowelink BW_PANEL=progress cargo run -p app-desktop`(Windows 不跑 exe,崩 0xC0000135)→ stderr `[BW_OPEN]` = 渲染证。重启 app 后验证(存量 cowelink,不走 CompleteCreation)。
- **R2-1 读回**:
  - sqlite:`SELECT name FROM cron_task WHERE project_id=(SELECT id FROM project WHERE name='cowelink');`(存储名**应仍是旧**「· 指标采集」——审计不动,VM 派生只在 UI 层)
  - 深链到 cron 详情卡(`BW_PANEL=...` 进 cron hub 点详情):UI 显示名应「cowelink · 采集代码仓指标」,详情卡「到点:📈 采集代码仓指标(开放 Issue / 已合入 MR)· 每日」,无空「· 目标「」」。
  - 读图:`claude -p --model haiku "读取图片 <screenshot> 并简短描述"`(主模型不看图)——截图 cron 详情卡。
- **R2-2 读回**:
  - 深链到 cowelink 总览(重启 app 后):项目指标条应有「↻ 立即采集」按钮;点前值全「待采集」(muted)不显「—」;点按钮后 toast 出采集结果。
  - sqlite(点后):`SELECT metric_id,ts,raw FROM observation ORDER BY ts DESC LIMIT 5;`——应有新观测(开放 Issue / 已合入 MR 有值),`SELECT name,collect_kind,signal FROM metric;` 验指标条数字。
  - 读图:截图总览(点前 + 点后各一张),haiku 读图确认「待采集」→ 有值的 UX 变化。
- **诚实口径**:无数据=「待采集」≠ 绿;Done 永不自动;manual 戴徽;数字 sqlite 可查;存储名与显示名的诚实差异在 commit「偏差」段记。

## 8. Round 3 修复(W3 项目指标改卡片,面聊对齐 2026-08-06)

> 用户重启验证 round 2 后点出 W3 真问题:项目指标(开放 Issue/已合入 MR/阶段完成)做成 compact strip 是错的——看不清、无 delta/趋势、无数据时全局样式垮。**面聊对齐(不开 SubAgent)后定:改卡片。** 本节是 round 3 设计事实源。

### 根因
- W3(issue3-overview-refactor.md §1)把 intrinsic 指标做成 `ProjectMetricStrip`(compact 一行小条,无 delta/趋势/无灯),round 2(R2-2)还在给小条绣花(待采集/立即采集)。实践:看不清、无数据「—」像分隔符、全局样式垮。
- 数据早有:`MetricVm.weekly_delta`/`weekly_spark`(kernel 从 observation 时序聚周算,W3 已建),strip 没用上。`BizMetricCard`(op.rs:2006)已有 delta + 按周折线 + 无数据 dashed 边框 + 「—」delta。

### 修复设计(面聊对齐 3 点)
1. **项目指标改卡片**:`ProgressAll`(op.rs:1673-1747)删 `ProjectMetricStrip { intrinsic }`(L1676),改成卡片 grid 渲染 intrinsic 指标,用 `BizMetricCard`。
2. **不点灯(决议 a)**:`BizMetricCard`(L2006)里 `m.is_intrinsic` 时不渲染信号灯 `dot`(代码仓 Issue/MR 是工程数不是健康,signal 恒 Unknown 无信息量)。其余(值/delta/折线/collect 徽)照常。
3. **两区保留(决议 b)**:`ProgressAll` 保留「项目指标·代码仓级」+「业务指标」两区,都用卡片。项目指标区头带「↻ 立即采集」按钮(从 R2-2 strip 挪来,`use_context::<Kernel>()` 取法照搬,发 `Command::CollectMetrics`)+ 标「只当现状数·不点健康灯」。业务指标区不动。
4. **无数据样式(决议 c)**:卡片框架照常渲染,值/delta 显「-」/「—」,折线空(占位)。`BizMetricCard` 已有 `grey_css`(dashed 边框)处理无观测——复核 intrinsic 无数据时也走这条,不垮样式。

### 清理
- 删 `ProjectMetricStrip` + `StripItem`(op.rs:1908-1995)。
- R2-2 的 strip「待采集」文案退场(无数据走 BizMetricCard 的 dashed + 「—」);「立即采集」按钮挪到项目指标区头。

### 文件锚点
- `crates/app-desktop/src/screens/op.rs`:
  - `ProgressAll`(~L1673-1747):`ProjectMetricStrip { intrinsic }`(L1676)→ 卡片 grid;区头加「立即采集」+「不点健康灯」标。
  - `BizMetricCard`(L2006):`m.is_intrinsic` 时不渲染 `dot`。
  - `ProjectMetricStrip`/`StripItem`(L1908-1995):删。
- `crates/app-desktop/src/kernel.rs`:`OpVm.metrics` 不动(intrinsic 仍在,is_intrinsic 字段已有)。

### 铁律(§6 延续)
- UI 无关内核:改动只在 `app-desktop`(op.rs),不进 bw-core/ui crate。`BizMetricCard` 用 `m.is_intrinsic`(VM 字段)判别,不碰 Signal 派生。
- 不动 schema/Signal 派生/状态机。逐 commit 不 push。

### 验证(读回为证)
- 门禁含 `cargo test`。
- 深链 `BW_OPEN=cowelink BW_PANEL=progress cargo run -p app-desktop` → haiku 读图:总览项目指标是**卡片**(值/delta/折线),无数据时卡片框架在、值「-」;点「立即采集」后 sqlite `SELECT raw FROM observation ORDER BY ts DESC LIMIT 3;` 有新值、卡片有数。
- 诚实口径:无数据 dashed + 「-」≠ 绿;不点灯;数字 sqlite 可查。

## 9. Round 4 修复(项目指标区定型 + Spark 1 点显点,面聊 2026-08-06)

> Round 3 把项目指标全改卡片(7 张含 5 阶段完成),用户反馈:阶段完成不该是大卡(一行就好)、项目指标区只要 2 张代码仓卡、1 周数据也该在折线显个点。A(R4 派生 bug)/B(maas 料乱)用户 deferred(要删 maas 重建)。本节 round 4 设计。

### R4-1 · 项目指标区定型(2 仓卡 + 阶段完成一行)
- **2 张代码仓卡**:`ProgressAll`(op.rs:1683-1703)项目指标区只渲染 `is_intrinsic && (name=="开放 Issue 数" || name=="已合入 MR 数")` 的 metric(2 张 BizMetricCard,不点灯——round 3 已实现 dot 抑制)。**阶段完成不进卡片区**。
- **阶段完成一行**(op.rs,项目指标区 2 卡下方加一行小字):显 active stage 的阶段完成数,文案「阶段完成({active_stage}):N · 机器记」,只显当前阶段那一条(`stage_kind == Some(op.active_stage)`)。对仗旧 strip 的 StripItem 一行口径,但不复活 strip 组件——内联一行 div。
- **删除 round 3 的全量 intrinsic 渲染**:`for m in intrinsic.iter()` 改成上面两段(仓卡 2 张 + 阶段完成一行)。

### R4-2 · Spark 1 点显点(sparkline_path n==1 居中)
- **根因**:`sparkline_path`(ui/src/lib.rs:87)对 n==1 时 `x_at(0)=0.0`(左边缘),circle 画在 (0, y) 被裁一半;且 area 退化成零宽竖线「M 0,h L 0,y L 0,h Z」。用户 1 周数据(1 个桶)看不到点。
- **修**:`sparkline_path` n==1 时 `x_at(0)=w/2`(居中),area 留空(`String::new()`),polyline 仍 1 点(画不出线,但 Spark 的 circle 在 (w/2, y) 可见)。≥2 点照常线。
- **效果**:0 观测 → Spark 显「尚无观测」(polyline 空,既有);1 观测(1 周)→ 居中一个点;≥2 周 → 线 + 末点。
- **文件**:`crates/ui/src/lib.rs`(sparkline_path,~L87-135)。注意:ui crate 是 wasm32 可编译的纯函数层,改它过 `cargo check -p ui --target wasm32`。

### 铁律(§6 延续)
- UI 无关内核:R4-1 在 app-desktop(op.rs);R4-2 在 ui crate(lib.rs sparkline_path,纯函数,不碰内核/Signal)。都不进 bw-core,不改 schema/Signal 派生/状态机。
- 逐 commit 不 push。

### 验证
- 门禁含 `cargo test` + `cargo check -p ui --target wasm32-unknown-unknown`。
- 深链 cowelink/maas 总览 → haiku 读图:项目指标区 = 2 仓卡 + 阶段完成一行;maas 1 周数据的仓卡折线显一个居中点(不空);cowelink 0 观测显「尚无观测」。
- 诚实口径:0 观测≠绿;不点灯;数字 sqlite 可查。

## 10. 事实源

- 现状代码:`create.rs`(RepoCard github-new L440-520 / codehub-new L372-398 / IntentCard L725 / slug 槽 L924-939 / 提交闭包 L798-861 / host 选择器 L580 / signal 声明 L81-82)、`lib.rs`(cron 创建 L6031-6041 / `collect_project_metrics` L3679 脚本臂 L3704-3720 / tick L4064 / CompleteCreation L6317 / ListCodehubRepos L6089)、`model.rs`(CronMode L1656-1674 / label L1680-1688 / icon L1695-1704)、`cron_hub.rs`(CronTaskRowView L112-228 通用槽 L144 / RunSkill 分支 L149-170)、`connector_hub.rs`(ConnectorCard L57-101 显示 L81)、`vm.rs`(`connector_card` L1517)、`main.rs`(toast 渲染 L425-429 / 设值 L208-219)。
- 指南:`docs/guide/buddy-guide.html` U2(L270-324 / callout L317-320 / 得到什么 L307-315)。
- 纪律:`CLAUDE.md`、`docs/guide/填写规范.md`、`.claude/skills/buddy-feature-dev/SKILL.md`(功能) / `.claude/skills/buddy-bugfix/SKILL.md`(缺陷);旧名 `v1-product-delivery` 已归档跳转。
- 关联设计:`docs/v1-prototype/issue1-onboard-simplify.md`(W1 纳入,yellow 标注 §6)、`docs/v1-prototype/issue3-overview-refactor.md`(W3 总览,项目指标条)。
