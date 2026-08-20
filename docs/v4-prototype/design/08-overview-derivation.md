# 08 · 总览推导

> **30 秒导读**:这篇讲总览屏一列七块,每块的每个数字从哪来、怎么算、没数据时显示什么;健康大灯的判定算法;项目名片(含「项目群」一行)怎么编辑。**总览一个数都不存**——指标定义读 `.bw/metrics.toml`,读数读 `.bw/plan/`,发版记录读 `.bw/releases.md`,其余现场算;没数据就是灰,不假装绿。给接着做 V4 的会话看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

## 0 · 这篇管什么、不管什么

**管**:总览屏一列七个横块,每块字段的来源、刷新时机、空态文案、读回方式(对应母文档 [`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) §5「总览」行、第 1 站全部);health 大灯 + 状态词 + 三条理由的**可执行**推导算法(待拍-03);名片(含新增的「项目群」行)编辑走「轻量活 + MR」(§2.6、待拍-26);「预览:未合入」在总览的读取来源(待拍-21,切换机制留给 06 篇)。

**不管**:计划屏看板逻辑、拖拽排期、周计划怎么引导出来(06 篇);规范铺底、写开发手册、历史回填「怎么做」,本篇只消费产出(03 篇);项目群适配工厂的实现、WeLink 对接,本篇只说总览显示「配了 / 没配」(07 篇);会话屏、通知入口本身(05、07 篇);数据模型总账与迁移守卫写法,02 篇是正本,冲突以 02 篇为准。

## 1 · 用户看到什么、做什么

Builder 在项目栏点进一个项目,默认落在总览。一列横块从上到下:先看名片 + health——一眼知道这个项目要不要管;往下是北极星、滞后、引领三层指标,每张卡下面挂着「本周谁在推它」的活;再往下是纯只读的代码仓统计;然后是本周计划进度条 + 运作活①②③三个状态点,一个「去计划 →」按钮带人到计划屏细看;再往下是在研版本与发版记录。**老项目没有额外的块**——回填出来的历史周文件与历史发版行和新项目的产出同格式,⑦块的发版记录里混排(带「回填」小字),历史周在计划屏左栏的「历史周」分组里,总览与计划屏都不做特殊处理(定,待拍-27 改写)。

总览本身几乎不可操作——按母文档「待人处理不在总览」的决定,合入 / 点完成这些动作在左栏「通知」入口,总览这一屏只有两处可点:①名片的「编辑」;②本周还没有计划时出现的「开始本周」横幅按钮。

**编辑名片**:点「编辑」进入表单态,点「保存」不是直接写库——后台建一张轻量活(不起 agent 会话)、开分支写 `.bw/PROJECT.md` 与 `.bw/project.toml`、开 MR,横幅「修改已提 MR · 待合入」;人点「合入」→「点完成」才生效,合入前旧值原样显示,和「写仓一律走 MR」的全局规矩一致(母文档 §2.6 用户四问之(4))。

**没有周计划时**:当前周没有 `.bw/plan/YYYY-Www.md`,顶部就有横幅「本周还没有计划 → 开始本周」;点了就建运作活①并跳到会话屏 ▶开工,和其它活走同一条命令路径,总览的按钮只是最顺手的入口,不是唯一入口。

## 2 · 设计

### 2.1 四层关系怎么落到总览(复述,细节以母文档 §2.5 为准)

北极星(1 个)→ 滞后指标(≤3)→ 引领指标(≤3)→ 周目标 + 业务活。**活推动引领指标 → 引领指标带动滞后指标 → 滞后指标逼近北极星**。总览把这条链「可视化」:每个指标卡下面列出本周挂着它的活,活完成后指标的下一次观测就是效果——不靠解释,靠把活挂上去。

### 2.2 八块总表

| 序号 | 块名 | 点灯吗 | 主数据源 | 刷新时机 |
|---|---|---|---|---|
| ① | 项目信息 + health 面板 | health 灯是,名片本身不是 | 仓文件(`.bw/PROJECT.md`、`.bw/project.toml` 的 `[chat]` 段)+ 库(`project` 表既有列,仅作名称/想做什么/对标/北极星一句的显示缓存)+ 现算(health) | 每次打开总览现读 + 现算 |
| ② | 北极星 | 是(自己一枚 Signal) | 仓文件(`.bw/metrics.toml` 定义)+ 现算(可重算读数)/ 仓文件(周计划文件「本周指标读数」段,不可重算读数) | 每次打开总览现读 + 现算 |
| ③ | 滞后指标(≤3) | 是 | 同上 | 同上 |
| ④ | 引领指标(≤3) | 是 | 同上 | 同上 |
| ⑤ | 项目指标 · 代码仓级 | 否(只是现状数) | 现算(git 子进程 / codegraph) | 打开总览时现算,另有「立即采集」手动刷新 |
| ⑥ | 本周计划进度 | 否 | 仓文件(`.bw/plan/YYYY-Www.md`,正本)+ 库(`issue` 缓存表,离线快速渲染) | 每次打开总览现读 + 查缓存 |
| ⑦ | 在研版本与发版记录 | 否 | 仓文件(`.bw/project.toml` 的 `current_version`、`.bw/releases.md`)+ 库(`issue.version` 列,活挂哪个版本) | 每次打开总览现读 |

### 2.3 逐块设计

每块给一张字段表:**字段 / 来源(仓文件 · 库表 · 现算)/ 刷新时机 / 空态文案 / 读回**。

#### ①项目信息 + health 面板

| 字段 | 来源 | 刷新时机 | 空态文案 |
|---|---|---|---|
| 名称 | 仓 `.bw/PROJECT.md` 标题行(库缓存 `project.name`) | 打开总览时读库缓存;`.bw/PROJECT.md` 合入后由铺底/编辑流程同步 | — (创建时必填,不会空) |
| 想做什么 | `.bw/PROJECT.md`「你想做什么」段(库缓存 `project.descr`) | 同上 | 「(待填)」 |
| 最像的对标 | `.bw/PROJECT.md`「定位与机会 - 对标」段(库缓存 `project.benchmark`) | 同上 | 「(待填)」 |
| 三个月长成什么样(北极星一句) | `.bw/PROJECT.md`「北极星」段(库缓存 `project.north_star` / `project.ns_def`) | 同上 | 「(待填)」——接入时必填,理论不会空;老库迁移期间可能空 |
| **项目群**(V4 新增) | 正本 `.bw/project.toml` 的 `[chat]` 段(**不入库**——02 篇 §2.1/§2.6 明确 `project` 表结构不新增列,不会有 `chat_provider`/`chat_group_id` 这两列) | 打开总览时现读现解析该文件;配置屏改群号后同样是改文件,下次读到新值 | 「未配 · 配置」一键跳配置屏;配了显示群名(取 provider 侧展示名,拿不到就显示群号本身)+「通知同步中」小字 |
| health 大灯 + 状态词 + 三条理由 | 现算,见 §3.3 | **每次打开总览现算,不缓存** | 灰「无数据」+ 理由「还没有任何真实数据」 |

读回:

```bash
sqlite3 <db> "SELECT name, descr, benchmark, north_star FROM project WHERE id='<pid>';"
cat <workspace>/.bw/PROJECT.md
cat <workspace>/.bw/project.toml   # 看 [chat] 段——这四个字段不在库里,只能这样核对
```

#### ②北极星(1 张宽卡)

| 字段 | 来源 | 刷新时机 | 空态文案 |
|---|---|---|---|
| 目标 / 定义 | 仓文件 `.bw/metrics.toml` 的 `[north_star]` 段(`name`/`def`/`target`/`collect`,02 篇 §2.5) | 打开总览现读现解析 | 不会空(接入必填,03 篇铺底保证骨架存在) |
| 现值 | 按 `collect.kind` 分两路:可重算的(如 git / 仓统计)每次打开总览现算;不可重算的(外部读数 / 手填)取当前周 `.bw/plan/YYYY-Www.md`「本周指标读数」段里指标名匹配的那一行(02 篇 §2.5 样例) | 每次打开总览现算 / 现读 | 两路都拿不到 → 「—」+「无观测 · Unknown ≠ 绿」 |
| 保鲜期 | 固定 `Cadence::Weekly`(V4 决定,见 3.3) | — | — |
| 趋势(近 8 周) | 可重算指标:对过去 8 个 ISO 周窗口分别现算;不可重算指标:回扫过去 8 份 `.bw/plan/YYYY-Www.md`「本周指标读数」段里同名指标那一行,没有的周留空 | 打开总览现算 / 现读(老仓可能慢,见 02 篇 §6 开放问题 4) | 全部拿不到 → 不画线 |
| 本周哪些活在推它 | `issue` 缓存表 `week_of=本周 AND metric_key='<该指标标识>'` 的行(正本是周计划文件业务活清单「预期推动的指标」列,02 篇 §2.2/§2.5) | 每次打开总览现算 | 无 → 「本周没有活推动它」 |
| 手填徽记 | 该指标当周读数那一行「来源」列写的是「手填」而不是可重算的现算方式 | 同上 | — |

**「该指标标识」怎么来**:`.bw/metrics.toml` 里每条指标目前靠 `name` 辨认(见 02 篇 §2.5 真实样例),`issue.metric_key` 与周计划文件「预期推动的指标」列存的就是这个名字(或后续细化出的 slug);具体标识格式留给 02 篇 / 实现敲定,本篇只保证不会凭空引入一个库里不存在的 `metric` 表主键。

读回:

```bash
cat <workspace>/.bw/metrics.toml            # 看 [north_star] 段
cat <workspace>/.bw/plan/<本周>.md          # 看「本周指标读数」段有没有这条指标当周的行
sqlite3 <db> "SELECT title FROM issue WHERE project_id='<pid>' AND week_of='<本周>' AND metric_key='<指标标识>';"
```

#### ③滞后指标(≤3,横排)/ ④引领指标(≤3,横排)

字段与②相同(目标 / 定义 / 现值 / 保鲜期 / 趋势 / 本周推动它的活 / 手填徽记),来源是仓文件 `.bw/metrics.toml` 的 `[[lagging]]` / `[[leading]]` 数组(02 篇 §2.5),按文件里数组出现的顺序取。母文档「≤3」是软上限——超过 3 条只截断总览显示,数组本身仍完整,多出的在知识库「资产」页签能看全(截断规则细节留给实现;V4 已经没有 `archived`/`created_at` 这类库端过滤列可用,因为 `metric` 表整个不存在,02 篇 §2.1)。

空态区分两种「没有」:①**还没定出来**——`.bw/metrics.toml` 里 `[[lagging]]`/`[[leading]]` 数组连一条都没有,留一句「还没有引领 / 滞后指标」+「去运作活①补一个」;②**定了但没测过**——数组里有这条,但可重算 / 文件读数两路都拿不到,显示「—」+「无观测 · Unknown ≠ 绿」。母文档 §2.5 明确「早期定不出指标怎么办:允许」,①是正常状态,不是错误。

读回:

```bash
cat <workspace>/.bw/metrics.toml       # 数 [[lagging]]/[[leading]] 条数
cat <workspace>/.bw/plan/<本周>.md    # 看有没有对应指标的读数行
```

#### ⑤项目指标 · 代码仓级(不点灯)

V3 已有的那块,原样带过来。字段:近 30 天提交数、主语言文件数 / 行数、`docs/` 篇数、近 30 天 PR 数、开放 PR 数、技能数——来自 `bw_engine::evidence::collect()`(只读子进程,`crates/bw-engine/src/evidence.rs`)与 codegraph 索引(有就用,没有跳过)。**不点灯、不上卷进 health**——工程虚荣指标,和业务指标(②③④)分层,`CONTEXT.md`「业务指标 / 项目指标」两层区分原样沿用。

刷新时机:打开总览现算一次(子进程有真实开销,不建议每次滚动都跑)+ 「↻ 立即采集」手动按钮,**不带定时自动刷新**(理由见第 6 节)。

读回:

```bash
git -C <workspace> rev-list --count HEAD; git -C <workspace> ls-files | wc -l
```

#### ⑥本周计划进度

| 字段 | 来源 | 刷新时机 | 空态 |
|---|---|---|---|
| 周目标一句 | 仓文件 `.bw/plan/YYYY-Www.md`「## 周目标」段第一段非空文本(`week_plan_file.rs::extract_goal`,02 篇 §3.3);没有库内索引表,周列表靠扫 `.bw/plan/` 目录得到(02 篇 §2.5) | 打开总览现读现解析 | 当前周文件不存在 → 「(未开始本周)」——此时①下方或本块顶部出现「开始本周」横幅 |
| 进度条(待办 / 进行中 / 评审中 / 完成 计数) | 库 `issue` 缓存表 `week_of=本周 AND kind='business'` 按 `status` 分组计数(02 篇 §2.2:这 9 列是缓存,正本是周计划文件业务活清单那张表;不一致时以文件为准,靠 `RefreshIssueCacheFromPlan` 命令追上,02 篇 §3.4) | 现算(查缓存表,不解析 Markdown) | 全 0 → 空进度条 |
| 运作活①②③三个状态点 | 库 `issue` 缓存表 `week_of=本周 AND kind='ops'` 按 `workflow` 名归类,取各自 `status`(同上,缓存) | 同上 | 该运作活本周还没建 → 灰点「未建」 |

读回:

```bash
cat <workspace>/.bw/plan/<本周>.md
sqlite3 <db> "SELECT status, count(*) FROM issue WHERE project_id='<pid>' AND week_of='<本周>' AND kind='business' GROUP BY status;"
sqlite3 <db> "SELECT workflow, status FROM issue WHERE project_id='<pid>' AND week_of='<本周>' AND kind='ops';"
```

#### ⑦在研版本与发版记录

| 字段 | 来源 | 刷新时机 | 空态 |
|---|---|---|---|
| 在研版本 | 仓文件 `.bw/project.toml` 的 `current_version` 字段(**不入库**——02 篇 §2.1/§3.3 明确 `project` 表不新增这一列) | 打开总览现读现解析 | 新项目默认 `v0.1`(待拍-04),不会空 |
| 发版记录(最近几行) | 仓文件 `.bw/releases.md`(唯一正本,`release_file.rs` 解析,02 篇 §2.5/§3.3);「包含的活」列按号去 `issue` 缓存表查标题展开,活挂哪个版本另看 `issue.version` 列(02 篇 §2.2) | 打开总览现读现解析 | 无 → 「还没有发版记录」 |

读回:

```bash
cat <workspace>/.bw/project.toml   # 看 current_version
cat <workspace>/.bw/releases.md
sqlite3 <db> "SELECT id, title, version FROM issue WHERE project_id='<pid>' AND version != '' ORDER BY updated_at DESC LIMIT 5;"
```

#### 老项目的历史怎么显示——不单开块(改写,原第⑧块整块删除)

老项目 = 铺底时探测到仓有历史(提交 / 标签 / 远端 issue·MR / CHANGELOG / 名片配了群 之一为真),运作活③「规范铺底」多跑一步「历史回填」(= 运作活②「资产盘点」workflow 的首次模式,03 篇)。**回填的产出不是一块新 UI,而是同格式的仓文件**:历史周文件(`.bw/plan/YYYY-Www.md`,front matter `origin: backfill`)与 `.bw/releases.md` 里标「回填 · git tag」的历史行。因此总览这边**不多一块**——历史发版行就混在⑦块的发版记录里(带「回填」小字区分),历史周在计划屏左栏单独成一个「历史周」分组(06 篇),两处都是既有渲染路径,不写老项目专用代码。**新项目什么都不缺,老项目也不留一个永远空着的坑。**

**不点灯**——历史事实的陈列,不参与 health 推导(2.4 节判据只看接入之后的真实数据)。`origin='backfill'` 的 `issue` 行 `workflow` 列默认为空(这些是接入 buddy 之前就已关闭的历史活,从没真的用过哪个 workflow),天然不进 02 篇 §2.3 现算的「用了几次」统计——V4 不维护单独的战绩账本(母文档 §6.3,活干没干成看远端 MR 合没合入),这里不需要额外的过滤代码,是查询默认值结构性满足的。

读回:

```bash
grep -n "回填" <workspace>/.bw/releases.md
grep -rl "origin: backfill" <workspace>/.bw/plan/
sqlite3 <db> "SELECT count(*) FROM issue WHERE project_id='<pid>' AND origin='backfill';"
```

## 3 · 工程对照

**模块位置**:总览屏的界面代码在 `crates/app-shell/src/screens/overview/mod.rs`;健康与指标的现算逻辑在 `crates/bw-v4/src/app/health.rs`(`collect_health_inputs`/`recompute_health`)与 `crates/bw-v4/src/derive/mod.rs`(纯函数 `derive_project_health`,密封类型见 `derive/sealed.rs`);把这些现算结果拼成界面能直接渲染的结构,发生在 `crates/app-shell/src/bridge/vm_build.rs`(总体拼装)与 `vm_panels.rs`(计划屏/指标/会话等分块拼装);ViewModel 类型是 `crates/app-shell/src/vm.rs` 里全新定义的一批结构体(`HealthVm`/`MetricsVm`/`MetricCardVm`/`WeekVm`/`ReleaseVm` 等)。`crates/ui` 这个 V3 的 crate 在这条链路里一行没用到——V4 的 ViewModel 不与它共享任何类型。总览屏只通过命令 / 事件与内核通信,不直接碰任何 SQL。

### 3.1 复用现有(不重写)

- `bw_core::derive::measure` / `evaluate_metric` / `Derived<Signal>`(`crates/bw-core/src/derive/`):每条业务指标(②③④)自身的 Signal 沿用这条已跑通、wasm 可编译的 L1→L2 链,V4 只改输入来源(从查 `metric`/`observation` 两张表改成读 `.bw/metrics.toml` 定义 + 现算 / 文件读数,见 3.3)。
- **指标卡 ViewModel**:是 `crates/app-shell/src/vm.rs::MetricCardVm`(与 `MetricsVm` 一起),全新定义,不复用 `crates/ui` 的任何类型或函数——`ui` 是 V3 的 crate,它的 ViewModel 全部架在旧库的二十张表上。走势与环比那两个「输入一串数值算出走势 / 环比」的算法要在 `app-shell` 这一侧自己写,喂给它的数据按 2.3 节②的口径:要么对过去 8 周现算、要么回扫 8 份周计划文件。
- `bw_engine::evidence::collect()`(`crates/bw-engine/src/evidence.rs`):⑤块的仓统计子进程读取。
- `bw_engine::metrics_file::read()` / `bw_engine::project_file::read()`:铺底与名片编辑读写正本用的既有解析器;`.bw/project.toml` 的 `[chat]` 段要同步加进 `ProjectFile` struct(`deny_unknown_fields` 已开,漏改会让老版本 buddy 读新文件解析失败)。
- `bw_engine::week_plan_file.rs::extract_goal()` / `extract_activities()` / `extract_front_matter()`(02 篇 §3.3 新增,本篇直接消费):⑥块的周目标、业务活清单,以及判断某份周文件是不是回填(front matter `origin`),都靠这三个函数。
- `bw_engine::release_file.rs::read()`(02 篇 §3.3 新增,本篇直接消费):⑦块发版记录解析(历史回填行同一条路径)。
- `dispatch.rs` 的 `write_charter` / `project_toml_content`:V4 复用其渲染部分,但**提交路径要改**——今天是「拥有的工作区直接 commit」,V4 名片编辑要求全走分支 + MR(2.5 节),这是**行为变更**不是纯增量,实现时别沿用旧的直提交路径。

### 3.2 数据结构:不新增任何库表 / 列(与 02 篇对齐,冲突以 02 篇为准)

本篇涉及的库结构已经由 02 篇 §2.1/§2.2/§2.7 定死——库最终只有 `project`/`issue`/`claude_conversation`/`app_meta` 四张表,`issue` 只加 02 篇 §2.2 定义的 8 个缓存列,`project` 表**结构不新增列**。08 篇不重复定义 schema,只列各块直接消费的字段 / 文件出处,避免和 02 篇出现两份互相打架的 schema:

| 块 | 消费的字段 / 文件(定义见 02 篇) |
|---|---|
| ①⑥⑦ | `issue` 表 8 个缓存列——`week_of` / `version` / `tool` / `kind` / `origin` / `workflow` / `sort_order` / `metric_key`(02 篇 §2.2/§3.1) |
| ②③④ | `.bw/metrics.toml`(02 篇 §2.5)、`.bw/plan/YYYY-Www.md`「本周指标读数」段(02 篇 §2.5) |
| ①⑦ | `.bw/project.toml` 的 `[chat]` / `current_version` / `standard_version`(02 篇 §2.5/§3.3;**均不入库**) |
| ⑥ | `.bw/plan/YYYY-Www.md`「## 周目标」段与「业务活」表格(02 篇 §2.5/§3.3) |
| ⑦ | `.bw/releases.md`(02 篇 §2.5/§3.3;历史回填行混排其中) |
| ⑦ | `issue` 缓存表 `origin='backfill'` 的行计数(只用于历史行的小字标注) |

**本篇不再保留任何库增量表**:早期草案曾设想过的 `metric.role='north_star'` 一行、新建 `release`/`release_issue`/`week_plan` 三张表、`project` 新增 `standard_version`/`current_version`/`chat_provider`/`chat_group_id` 四列、新建 `chat_outbox` 表,**盘点之后全部取消**(02 篇 §2.1「其余 16 张……以及早期草案曾计划新建、后来取消的 3 张」)。本篇原来那版增量表已按此删除,不再保留。

迁移守则见 02 篇 §2.7/§3.2:开发期改 `schema.sql` 直接删库重建;试点起恢复 `add_column_if_missing` 双守卫。本篇没有新增任何列,不需要单独交代迁移步骤。

### 3.3 health 现算怎么实现(与今天 `recompute_signals` 的关系)

今天(V3)是「写透缓存」模式:`Store::recompute_signals`(`crates/bw-store/src/sqlite.rs:1411`)在相关命令执行后被调用,把算好的 Signal 写进 `metric.signal` / `op_stage.routine_signal` / `project.signal` / `project.weekly_signal` 四个缓存列,界面读缓存。`project.weekly_signal` 听起来像「周快照」,但读代码可见它和 `signal` 每次都被同一条 UPDATE 写成相同值(`sqlite.rs:1538` 附近),从未真正按周冻结,只是历史遗留命名。

V4 总览的 health(2.4 节)**不走这条缓存链**:母文档明确「读时现算,库里不存灯」;(a)(b)(c) 本来就按时间窗口现算,换参数即可回算历史周。`recompute_signals` 原本依赖 `op_stage.routine_schedule` 取保鲜期,但 `op_stage` 表在 V4 已经整表不存在(阶段降级为活的类别标签,02 篇 §2.1),这条路径本来就走不通了——新算法直接改成**固定 `Cadence::Weekly`**(是否按指标细分见第 6 节)。`metric` 表(连同它的 `signal`/`hit` 两列)在 V4 也整表不存在(02 篇 §2.1),指标卡小圆点不再有一张表能缓存它的结果。`project.signal`/`weekly_signal` 两列结构不变(02 篇 §2.1),但如 2.4 节末段所说,它们只服务项目墙的显示缓存,总览屏自己从不读它们当输入,只在算完一次 health 后顺手写回。

**判定分两处,不在一个函数里算完**:输入采集在 `crates/bw-v4/src/app/health.rs::collect_health_inputs`,只读不判断;唯一判定颜色的纯函数在 `crates/bw-v4/src/derive/mod.rs::derive_project_health`,输入之外没有任何隐藏状态,同样的输入永远同样的输出。判定顺序也不是设计里写的「先判绿」:

```rust
// crates/bw-v4/src/app/health.rs —— 只负责从仓现场取三条判据的输入
pub async fn collect_health_inputs(workspace: &Path, week: &str) -> HealthInputs {
    let last_week = isoweek::previous_week(week).unwrap_or_default();
    let this_plan = week_plan_file::read(workspace, week).ok().flatten();
    let last_plan = week_plan_file::read(workspace, &last_week).ok().flatten();

    // 「这个仓 git 读得动吗」是一条独立输入,不是"零提交"的旁注——读不动的
    // 时候零提交说明不了任何事,新接入的项目不该因此第一眼就看到红灯。
    let git_readable = crate::git::is_repo(workspace).await;
    let committed_this_week = crate::git::has_commits_in_week(workspace, week).await.unwrap_or(false);
    let committed_last_week = crate::git::has_commits_in_week(workspace, &last_week).await.unwrap_or(false);
    let merged_last_week = crate::git::has_merges_in_week(workspace, &last_week).await.unwrap_or(false);
    let released = /* .bw/releases.md 上周有没有新增一行,02 篇 §2.5 的发版记录 */;

    HealthInputs {
        has_week_goal: this_plan.as_ref().is_some_and(|p| p.has_goal()),
        committed_this_week,
        committed_last_week,
        git_readable,
        has_metric_reading: this_plan.as_ref().is_some_and(|p| p.has_reading())
            || last_plan.as_ref().is_some_and(|p| p.has_reading()),
        // 读数是否越线要按 `.bw/metrics.toml` 的目标比对 —— A 刀还没接这一步,
        // 如实给 false,不假装判过。见下方说明②。
        any_metric_red: false,
        delivered_last_week: merged_last_week || released,
    }
}

// crates/bw-v4/src/derive/mod.rs —— 唯一判定颜色的纯函数
pub fn derive_project_health(inputs: &HealthInputs) -> DerivedHealth {
    let (a, b, c) = (inputs.has_week_goal && inputs.committed_this_week,  // (a) 本周定了目标且真有提交
        inputs.has_metric_reading,                            // (b)
        inputs.delivered_last_week,                           // (c));
    // 判红排在最前面:三条判据齐了但指标越线,那是红,不是绿。
    let stalled = inputs.git_readable && !inputs.committed_this_week && !inputs.committed_last_week;
    let signal = if inputs.any_metric_red || stalled {
        Signal::Red   // 指标越线,或者读得动 git 而连着两周一条提交都没有——真的停了
    } else if a && b && c {
        Signal::Green
    } else if !a && !b && !c {
        Signal::Unknown   // 三条判据都不成立,又没有"确实停了"的证据:就是没数据
    } else {
        Signal::Amber
    };
    DerivedHealth { signal, reasons: /* 三条判据各自一句人话理由 */ }
}
```

两条特别说明:

1. **「git 读不读得动」是一条独立输入,不是零提交的同义词。** `git_readable` 单独判一次(`crate::git::is_repo`)——没配工作区、目录不是 git 仓、机器上没装 git,这一条是假,此时"两周零提交"这句话本身没有意义,不能拿去判红。没数据的项目显示的是灰(`Signal::Unknown`),不是红,也不是绿。
2. **「指标越线」这条输入目前恒为 `false`。** `.bw/metrics.toml` 的目标比对(某条读数按目标算下来是不是超线)这一刀还没接,`any_metric_red` 如实写死 `false`——这意味着灯现在**不会**因为指标读数难看而变红,只会因为"两周零提交"这一条硬判据变红。这是已知留白,不是这段代码的 bug,接上目标比对是后面的刀要做的事。

## 4 · 边界与失败

**不做**:

- 阶段轴与阶段舱(五阶段进度面板)——不带,方法论内容已并入规范扩展件 `docs/method/`,阶段降级为活的类别标签。
- 「进度趋势」的手工维护字段——V4 所有趋势都来自真实数据序列,没有一处允许人手描一条趋势线。
- 灯手动设置——总览没有任何「设为绿」的界面路径,信号类型的构造入口在 V3 就已经密封(`crates/bw-core/src/derive/sealed.rs`,类型名见第 3 节),V4 沿用这个约束。
- ⑤块定时自动刷新——只有打开时现算 + 手动「立即采集」,不额外起定时任务采集仓统计(和采集业务指标的定时任务是两回事)。

**失败如实**:

| 场景 | 显示 |
|---|---|
| `.bw/PROJECT.md` 缺某一段(如没有「北极星」段) | 「章程不完整」+ 具体缺哪段(措辞沿用 `dispatch.rs` 「章程未补写(PROJECT.md 北极星段可能缺)」的风格) |
| `.bw/project.toml` 解析失败(格式错、未知字段) | 名片区整体灰 + 「配置文件解析失败:{错误}」,不猜测、不用旧缓存顶上 |
| `.bw/metrics.toml` 解析失败 | ②③④三块灰 + 同样的解析错误原文——结构性错误,不是内容问题,和 `docs/buddy/standards/metrics.md`「坏文件只报错不写库」一致 |
| `.bw/plan/YYYY-Www.md` 解析失败(结构错、「## 周目标」或「业务活」表格格式坏了) | ⑥块整体灰 + 「周计划文件解析失败:{错误}」,不假装进度是 0 |
| `.bw/releases.md` 解析失败 | ⑦块整体灰 + 同样报错,不用旧值顶上 |
| ⑤块 git 子进程失败(非 git 目录 / git 未安装) | 整块显示「无法读取仓统计:{git 原文错误}」,不是空白也不是假数据 |
| 历史回填某类原料缺失(如无远端 issue 访问权限) | 那一行单独显示「—」+「该来源未取到」,不影响块内其它行——回填允许「原料没有就空着」 |
| health 计算中途文件解析或 git 子进程失败 | 大灯直接灰 + 「health 计算失败:{错误}」,绝不吞掉错误凑一个颜色 |

## 5 · 验收与读回

**深链**:

```bash
BW_DB=<临时db路径> BW_OPEN=<项目名> BW_PANEL=overview ./target/debug/builders-workbench
# stderr 出现 [BW_BOOT] 与 [BW_OPEN] = 渲染成功,无 panic
```

**每块一条读回**:命令已经在 2.3/2.4 节每块的「读回」段给全,验收时按①→⑦顺序逐条跑一遍、配一张截图存档,不重复贴一遍。

**铁律核验**:杀掉进程重开,同一份 DB 上再跑一遍 SQL 与深链截图,数字与 health 结论必须一致——health 是纯函数现算,输入没变结论就不该变,这就是「杀进程重开,数字前后一致」这条铁律在总览的体现。

**验收场景清单**:①全新项目:总览灰,①②③④空态文案,⑦的发版记录为空态,⑥显示「开始本周」横幅。②跑完一轮运作活①②后:⑥有真实进度,①health 不再灰,三条理由至少一条为真。③名片编辑一次:MR 横幅 → 合入 → 完成 → 新值生效,`sqlite3`/`cat` 读回已更新。④老项目接入(母文档 §8 验收 7):⑦块里带「回填」小字的历史发版行、计划屏左栏「历史周」分组里的每个数字,都能对回 `git log` 或远端 API;总览**没有**多出任何老项目专用的块;`origin='backfill'` 的 issue `workflow` 列默认为空,天然不进 02 篇 §2.3「用了几次」的现算统计(见 2.3 节末段)。⑤配了项目群的项目:群名显示,「未配 · 配置」消失。

## 6 · 开放问题(≤5)

1. **「周信号快照」存不存**:倾向「不存」——(a)(b)(c) 都按时间戳现算,查「上周/本周」换参数即可,不需要额外快照表。但以后若要「过去 8 周灯色时间线」,现算意味着判据算法一改历史灯色跟着变,不是冻结在当时的判断——这是否可接受需要拍板,不接受则要重新引入类似快照的机制。
2. **「进展」判据够不够严**:(a) 目前只认「本周有真实 git 提交」(`git log --all --since=<本周一>`),对构建类活自然,但原型类活、运作活①(产出周计划算不算进展)可能需要补充——要不要扩大到「MR 新评论 / 活状态前进」这类较弱但覆盖面更广的信号,需要拍板。
3. **⑤块刷新频率**:定为「打开现算 + 手动采集」,没有定时;仓大了子进程会变慢,要不要加本机缓存 + 定时后台刷新,先跑起来看真实耗时再说。
4. **指标改名后,历史读数怎么对得上**:②③④块靠 `.bw/metrics.toml` 里指标的 `name`(或后续细化出的 slug)去匹配 `issue.metric_key` 和历史周计划文件「本周指标读数」段的行(2.3 节②)——指标一旦改名,历史行还是用旧名字写的,新名字就对不上。要不要引入一个不随改名变化的稳定标识(哪怕只是「第一次出现时生成一个短 id,写进 metrics.toml 该条目」),留给 02 篇 / 实现按真实需要再定,本篇只指出这条一致性风险。
5. **health 判据 (a) 要不要把运作活①算进去**:目前运作活明确不算业务活,接入后第一周若只做了运作活,health 会偏黄/红——要不要给接入第一周一次豁免,需要拍板。
