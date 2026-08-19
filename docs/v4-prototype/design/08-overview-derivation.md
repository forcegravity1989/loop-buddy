# 08 · 总览推导

> **30 秒导读**:这篇讲总览屏一列八块,每块的每个数字从哪来、怎么算、没数据时显示什么;health(健康)大灯的判定算法;老项目才有的「历史运作(回填)」块;名片(含新增的「项目群」行)怎么编辑。**详细设计稿,待用户复核,尚未开工写代码**。给三种人看:复核设计的用户、下一步写代码的会话、接手总览这块的同事。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md)。

## 0 · 这篇管什么、不管什么

**管**:总览屏一列八个横块,每块字段的来源、刷新时机、空态文案、读回方式(对应母文档 [`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) §5「总览」行、第 1 站全部);health 大灯 + 状态词 + 三条理由的**可执行**推导算法(待拍-03);第⑧块「历史运作(回填)」,只有老项目才有(第 0 站、待拍-27);名片(含新增的「项目群」行)编辑走「轻量活 + MR」(§2.6、待拍-26);「预览:未合入」在总览的读取来源(待拍-21,切换机制留给 06 篇)。

**不管**:计划屏看板逻辑、拖拽排期、周计划怎么引导出来(06 篇);规范铺底、合并调整、历史回填「怎么做」,本篇只消费产出(03 篇);项目群适配工厂的实现、WeLink 对接,本篇只说总览显示「配了 / 没配」(07 篇);会话屏、通知入口本身(05、07 篇);数据模型总账与迁移守卫写法,02 篇是正本,冲突以 02 篇为准。

## 1 · 用户看到什么、做什么

Builder 在项目栏点进一个项目,默认落在总览。一列横块从上到下:先看名片 + health——一眼知道这个项目要不要管;往下是北极星、滞后、引领三层指标,每张卡下面挂着「本周谁在推它」的活;再往下是纯只读的代码仓统计;然后是本周计划进度条 + 运作活①②③三个状态点,一个「去计划 →」按钮带人到计划屏细看;再往下是在研版本与发版记录。**老项目**在最底部多一块「历史运作(回填)」,帮刚接手的人一眼看到过去的节奏,不用自己翻 git log。

总览本身几乎不可操作——按母文档「待人处理不在总览」的决定,合入 / 点完成这些动作在左栏「通知」入口,总览这一屏只有两处可点:①名片的「编辑」;②本周还没有计划时出现的「开始本周」横幅按钮。

**编辑名片**:点「编辑」进入表单态(想做什么 / 对标 / 北极星一句 / 项目群),点「保存」不是直接写库——后台建一张轻量活(不起 agent 会话)、开分支写 `PROJECT.md` 与 `.bw/project.toml`、开 MR,横幅「修改已提 MR · 待合入」;人点「合入」→「点完成」才生效,合入前旧值原样显示,和「写仓一律走 MR」的全局规矩一致(母文档 §2.6 用户四问之(4))。

**没有周计划时**:当前周没有 `docs/plan/YYYY-Www.md`,顶部就有横幅「本周还没有计划 → 开始本周」;点了就建运作活①并跳到会话屏 ▶开工,和其它活走同一条命令路径,总览的按钮只是最顺手的入口,不是唯一入口。

## 2 · 设计

### 2.1 四层关系怎么落到总览(复述,细节以母文档 §2.5 为准)

北极星(1 个)→ 滞后指标(≤3)→ 引领指标(≤3)→ 周目标 + 业务活。**活推动引领指标 → 引领指标带动滞后指标 → 滞后指标逼近北极星**。总览把这条链「可视化」:每个指标卡下面列出本周挂着它的活,活完成后指标的下一次观测就是效果——不靠解释,靠把活挂上去。

### 2.2 八块总表

| 序号 | 块名 | 点灯吗 | 主数据源 | 刷新时机 |
|---|---|---|---|---|
| ① | 项目信息 + health 面板 | health 灯是,名片本身不是 | 仓文件(`PROJECT.md`)+ 库(`.bw/project.toml` 的 `[chat]` 段缓存)+ 现算(health) | 每次打开总览现算 |
| ② | 北极星 | 是(自己一枚 Signal) | 库(`metric` + `observation`) | 每次打开总览现算 |
| ③ | 滞后指标(≤3) | 是 | 同上 | 同上 |
| ④ | 引领指标(≤3) | 是 | 同上 | 同上 |
| ⑤ | 项目指标 · 代码仓级 | 否(只是现状数) | 现算(git 子进程 / codegraph) | 打开总览时现算,另有「立即采集」手动刷新 |
| ⑥ | 本周计划进度 | 否 | 库(`issue` + `week_plan`) | 每次打开总览现算 |
| ⑦ | 在研版本与发版记录 | 否 | 库(`project.current_version` + `release`) | 每次打开总览现算 |
| ⑧ | 历史运作(回填,老项目才有) | 否 | 仓文件(`docs/releases.md` 历史段、`docs/plan/history.md`)+ 库(`origin='backfill'` 的 `issue`/`release` 行) | 一次性(回填 MR 合入时写定),之后只在再次回填时更新 |

### 2.3 逐块设计

每块给一张字段表:**字段 / 来源(仓文件 · 库表 · 现算)/ 刷新时机 / 空态文案 / 读回**。

#### ①项目信息 + health 面板

| 字段 | 来源 | 刷新时机 | 空态文案 |
|---|---|---|---|
| 名称 | 仓 `PROJECT.md` 标题行(库缓存 `project.name`) | 打开总览时读库缓存;`PROJECT.md` 合入后由铺底/编辑流程同步 | — (创建时必填,不会空) |
| 想做什么 | `PROJECT.md`「你想做什么」段(库缓存 `project.descr`) | 同上 | 「(待填)」 |
| 最像的对标 | `PROJECT.md`「定位与机会 - 对标」段(库缓存 `project.benchmark`) | 同上 | 「(待填)」 |
| 三个月长成什么样(北极星一句) | `PROJECT.md`「北极星」段(库缓存 `project.north_star` / `project.ns_def`) | 同上 | 「(待填)」——接入时必填,理论不会空;老库迁移期间可能空 |
| **项目群**(V4 新增) | 正本 `.bw/project.toml` 的 `[chat]` 段(库缓存 `project.chat_provider` + `project.chat_group_id`) | 打开总览读库缓存;配置屏改群号后同步 | 「未配 · 配置」一键跳配置屏;配了显示群名(取 provider 侧展示名,拿不到就显示群号本身)+「通知同步中」小字 |
| health 大灯 + 状态词 + 三条理由 | 现算,见 2.4 | **每次打开总览现算,不缓存** | 灰「无数据」+ 理由「还没有任何真实数据」 |

读回:

```bash
sqlite3 <db> "SELECT name, descr, benchmark, north_star, chat_provider, chat_group_id FROM project WHERE id='<pid>';"
cat <workspace>/PROJECT.md
cat <workspace>/.bw/project.toml   # 看 [chat] 段
```

#### ②北极星(1 张宽卡)

| 字段 | 来源 | 刷新时机 | 空态文案 |
|---|---|---|---|
| 目标 / 定义 | `project.north_star` / `project.ns_def`(正本 `PROJECT.md`) | 同①名称类字段 | 不会空(接入必填) |
| 现值 | `metric` 表 `role='north_star'` 那一行(见 3.2 节「北极星 metric 化」建议)的最新 `observation` | 每次打开总览现算 | 无观测 → 「—」+「无观测 · Unknown ≠ 绿」 |
| 保鲜期 | 固定 `Cadence::Weekly`(V4 决定,见 3.3) | — | — |
| 趋势 | 该 `metric_id` 的近 8 周 observation(沿用 `weekly_spark` 算法) | 同上 | 空数组 → 不画线 |
| 本周哪些活在推它 | `issue` join `issue_metric` 关联表(02 篇 §2.2),`week_of=本周` 且 `issue_metric.metric_id=该 metric_id` 的行 | 同上 | 无 → 「本周没有活推动它」 |
| 手填徽记 | 最新 observation 的 `source_kind='manual'` | 同上 | — |

读回:

```bash
sqlite3 <db> "SELECT m.id, m.target_raw, o.raw, o.ts, o.source_kind
  FROM metric m LEFT JOIN observation o ON o.metric_id=m.id
  WHERE m.project_id='<pid>' AND m.role='north_star'
  ORDER BY o.ts DESC LIMIT 1;"
sqlite3 <db> "SELECT i.id, i.title FROM issue i JOIN issue_metric im ON im.issue_id=i.id WHERE i.project_id='<pid>' AND i.week_of='<本周>' AND im.metric_id='<metric_id>';"
```

#### ③滞后指标(≤3,横排)/ ④引领指标(≤3,横排)

字段与②相同(目标 / 现值 / 保鲜期 / 趋势 / 本周推动它的活 / 手填徽记),来源是 `metric` 表 `role='lagging'` / `role='leading'` 且 `project_id=当前项目` 且 `archived=0` 的行,`ORDER BY created_at` 最多各取 3 条(母文档「≤3」是软上限,超过时按创建时间截断,多出的在知识库「资产」页签能看全)。

空态区分两种「没有」:①**还没定出来**——`metric` 表里连行都没有,留一句「还没有引领 / 滞后指标」+「去运作活①补一个」;②**定了但没测过**——行存在但无 `observation`,显示「—」+「无观测 · Unknown ≠ 绿」。母文档 §2.5 明确「早期定不出指标怎么办:允许」,①是正常状态,不是错误。

读回:

```bash
sqlite3 <db> "SELECT id, name, role, target_raw, archived FROM metric WHERE project_id='<pid>' AND role IN ('lagging','leading') AND archived=0 ORDER BY created_at;"
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
| 周目标一句 | 库 `week_plan` 索引(正本是仓 `docs/plan/YYYY-Www.md`) | 现算 | 「(未开始本周)」——此时①下方或本块顶部出现「开始本周」横幅 |
| 进度条(待办 / 进行中 / 评审中 / 完成 计数) | `issue` 表 `week_of=本周 AND kind='business'` 按 `status` 分组计数 | 现算 | 全 0 → 空进度条 |
| 运作活①②③三个状态点 | `issue` 表 `week_of=本周 AND kind='ops'` 按 `workflow` 名归类,取各自 `status` | 现算 | 该运作活本周还没建 → 灰点「未建」 |

读回:

```bash
sqlite3 <db> "SELECT status, count(*) FROM issue WHERE project_id='<pid>' AND week_of='<本周>' AND kind='business' GROUP BY status;"
sqlite3 <db> "SELECT workflow, status FROM issue WHERE project_id='<pid>' AND week_of='<本周>' AND kind='ops';"
cat <workspace>/docs/plan/<本周>.md
```

#### ⑦在研版本与发版记录

| 字段 | 来源 | 刷新时机 | 空态 |
|---|---|---|---|
| 在研版本 | `project.current_version` | 现算 | 新项目默认 `v0.1`,不会空 |
| 发版记录(最近几行) | 库 `release` 表(正本是仓 `docs/releases.md`) | 现算 | 无 → 「还没有发版记录」 |

读回:

```bash
sqlite3 <db> "SELECT current_version FROM project WHERE id='<pid>';"
sqlite3 <db> "SELECT version, released_at, note FROM release WHERE project_id='<pid>' ORDER BY released_at DESC LIMIT 5;"
cat <workspace>/docs/releases.md
```

#### ⑧历史运作(回填)——只有老项目才有

老项目 = 铺底时探测到仓有历史(提交 / 标签 / 远端 issue·MR / CHANGELOG / 名片配了群 之一为真),同一张运作活③多跑一步「历史回填」(03 篇管流程),产出全标「回填」,总览据此渲染。**新项目这一块整个不显示,不显示成空态**——「没有历史」和「有历史但还没回填」是两种事实,前者不留一个永远空着的坑。

| 字段 | 来源 | 刷新时机 | 空态 / 徽记 |
|---|---|---|---|
| 版本时间线 | 库 `release` 表 `origin='backfill'` 的行(来自标签 / CHANGELOG) | 只在回填 MR 合入时写定;之后只在再次跑历史回填时更新(不常发生) | 每行带「回填自 git / CHANGELOG」小字 |
| 近 N 周合入吞吐 | 仓 `docs/plan/history.md`(按周的历史运作记录,来自 git 合入记录) | 同上 | 带「回填自 git」小字 |
| 远端 issue 累计 | 库 `issue` 表 `origin='backfill'` 的行计数 | 同上 | 带「回填自 codehub / GitHub」小字 |
| 贡献者数 | 仓历史回填时算出、写进 `docs/plan/history.md` 或 PROJECT.md 草稿的一个数 | 同上 | 带「回填自 git」小字 |

**不点灯**——历史事实的陈列,不参与 health 推导(2.4 节判据只看接入之后的真实观测)。`origin='backfill'` 的 `issue` 行**不进任何人 / workflow 的战绩**(待拍-27 硬约束),过滤发生在战绩统计链路(超出本篇范围)。

读回:

```bash
sqlite3 <db> "SELECT version, released_at, origin FROM release WHERE project_id='<pid>' AND origin='backfill' ORDER BY released_at;"
sqlite3 <db> "SELECT count(*) FROM issue WHERE project_id='<pid>' AND origin='backfill';"
cat <workspace>/docs/plan/history.md
```

### 2.4 health 推导规则(可执行算法)

**输入三项,全部现算,不查任何提前算好的缓存**:

- **(a) 本周有周目标且业务活有进展**:「有周目标」= `week_plan` 表存在 `week_of=本周` 一行(`docs/plan/YYYY-Www.md` 已合入)。「有进展」= 本周 `kind='business'` 的活里至少一张有 `workflow_run` 行满足 `head_before<>head_after AND started_at 落在本周`——真有一次运行产生了真实提交,不是「状态被点了一下」这类容易造假的信号,复用既有的运行记账比新开活动日志表更省、更难造假。
- **(b) 业务指标在保鲜期内有真实观测**:`role IN ('lagging','leading') AND archived=0` 的行里,至少一条最新 `observation.ts` 落在保鲜期内(V4 固定 `Cadence::Weekly`,7 天)。**没有任何滞后 / 引领指标定义时视为「假」**——早期项目的灯因此长期停在黄不是绿,与「早期定不出指标:允许,但如实」一致。
- **(c) 上周有交付(合入或发版)**:满足其一——①上周 `release` 新增一行;②上周有活 `settled_at`(完成前必经「评审中」= 存在开放 PR,再经合入);③**老项目兜底**:库记账不足时读 `git log --since=<上周一> --until=<本周一> --merges --oneline` 行数 > 0,母文档明确「老项目可从 git 合入记录推」。

**四色规则**:

| 结果 | 条件 |
|---|---|
| **绿** | (a)(b)(c) 三项都真 |
| **黄** | 三项里恰好缺一项(不满足红的条件) |
| **红** | 连续两周 (a) 都为假(判「上周」「本周」是同一段代码换时间窗口,不需另存快照),**或**任一业务指标(③④)自身 Signal 为 Red(见下) |
| **灰** | (a)(b)(c) 三项都假,即完全没有真实数据 |

**指标自身的 Signal**(卡片小圆点,和大灯两回事):每条 `metric` 行按 V3 已有的 L1→L2 派生链算——`measure()` 把最新 observation 转成 `MeasuredValue`(带 `stale` 标记),`evaluate_metric()` 拿它和 `target_raw` 比出一个信号结果(密封类型,只能从数据推导,类型名见第 3 节;过期的绿降级成黄)。这条已经 wasm 编译检查的纯函数 V4 直接复用,只是**调用方从「recompute_signals 预算写缓存」改成「渲染时现算」**(第 3 节详述)。

**理由文案模板**(三条全部可见,顺序固定 a→b→c):(a)真「✓ 本周 {周目标} · {活标题} 有真实提交」/ 假「○ 本周还没有活推进」;(b)真「✓ 业务指标在保鲜期内有观测({指标名} {时间} 前测过)」/ 假「○ 业务指标都没有 7 天内的观测」;(c)真「✓ 上周合入了 {N} 个 / 发了版本 {版本号}」/ 假「○ 上周没有合入或发版」。手填的观测带「手填」徽记,理由文案不因手填降级——手填也是真实数据,只标注来源。

**灯与理由读时现算,库里不存灯**是母文档第 1 站的决定:V4 不复用今天 `project.signal`/`metric.signal` 两个写透缓存列,避免忘了调 recompute 显示过期值;`project.weekly_signal` 今天只是 `signal` 的同步复制(3.3 节),不是真按周冻结的快照,V4 不再需要它。

### 2.5 名片编辑(含项目群)

1. 人在①块点「编辑」→ 表单态,四个字段(想做什么 / 对标 / 北极星一句 / 项目群)可改;项目群一行下拉选提供方(内部 WeLink / 外部待定)+ 群号输入框。
2. 点「保存」→ 命令 `EditProjectCard`:buddy 建一张轻量活(`kind='ops'`,不起 agent 会话)、开分支、把新值渲染进 `PROJECT.md`(想做什么/对标/北极星三段)与 `.bw/project.toml`(既有五字段 + 新增 `[chat]` 段)、提交、开 MR。项目群只落 `.bw/project.toml` 的 `[chat]` 段,不进 `PROJECT.md`(群号是配置,不是章程)。
3. ①块出现「修改已提 MR · 待合入」横幅,人点「合入」(`MergeIssuePr`)→「点完成」(`TransitionIssue{Done}`);合入前仍显示旧值,横幅带「预览」按钮先看一眼新值(2.6 节)。
4. 完成后下次打开总览,①块从库缓存(合入后 `SyncProjectFile` 同步)读到新值。

### 2.6 预览未合入(待拍-21)

当前选中周存在至少一张运作活(①或③的合并调整/历史回填)处于评审中且 MR 未合入时,总览才可切到「预览:活 X 的 worktree」(设计期统一,与 06 篇 §2.5 口径对齐:覆盖运作活①和③,不只①)——横幅「预览 · 未合入」,①③④⑥⑦块改从该活 worktree 读 `.bw/metrics.toml`、`docs/plan/`(选择器入参多一个「读哪个目录」);**⑤⑧、health 的 (b)(c) 判据不预览**——依赖库里只追加的 observation / release 记账,预览态照实读主态的库。切换入口由 06 篇拍板,本篇只说明总览侧「读哪个目录」这条规则。

### 2.7 命令与事件(名字对齐 01 篇,这里只列名 + 一句话)

| 名字 | 一句话 |
|---|---|
| 命令 `EditProjectCard` | 人保存名片编辑表单,触发 2.5 节流程,建轻量活 + MR |
| 命令 `SetProjectChat` | 配置屏改项目群提供方 / 群号(与名片编辑走同一条 MR 路径,还是并入 `EditProjectCard` 的一个字段,见第 6 节) |
| 命令 `StartWeekPlanning` | 总览「开始本周」按钮,建运作活①并跳会话屏 ▶开工 |
| 命令 `TogglePreview` | 切换总览 / 计划屏的「预览:未合入」态 |
| 事件 `ProjectCardEditPending` | 名片编辑的轻量活已建、MR 已开,总览横幅可以显示了 |
| 事件 `ProjectCardMerged` | 名片 MR 已合入,库缓存已同步,总览可以刷新显示新值了 |

## 3 · 工程对照

**模块位置**(遵循 §7「一屏一模块」):界面代码在 `app-desktop` 新目录(建议 `app-desktop/src/screens/overview/`),数据计算(selector)在 `bw-app` 新增只读查询模块(建议 `bw-app/src/overview.rs`),ViewModel 在 `crates/ui/src/vm.rs` 追加(沿用今天 `MetricVm`/`metric_vm` 模式,不新开平行类型)。总览屏只通过命令 / 事件与内核通信,不直接碰 `bw-store`。

### 3.1 复用现有(不重写)

- `bw_core::derive::measure` / `evaluate_metric` / `Derived<Signal>`(`crates/bw-core/src/derive/`):每条业务指标(②③④)自身的 Signal 沿用这条已跑通、wasm 可编译的 L1→L2 链,V4 只改调用时机(见 3.3)。
- `crates/ui/src/vm.rs::MetricVm` / `metric_vm()` / `weekly_spark()` / `weekly_delta()`:指标卡字段结构、近 8 周走势算法、`manual` 手填徽记字段直接沿用。
- `bw_engine::evidence::collect()`(`crates/bw-engine/src/evidence.rs`):⑤块的仓统计子进程读取。
- `bw_engine::metrics_file::read()` / `bw_engine::project_file::read()`:铺底与名片编辑读写正本用的既有解析器;`.bw/project.toml` 的 `[chat]` 段要同步加进 `ProjectFile` struct(`deny_unknown_fields` 已开,漏改会让老版本 buddy 读新文件解析失败)。
- `dispatch.rs` 的 `write_charter` / `project_toml_content`:V4 复用其渲染部分,但**提交路径要改**——今天是「拥有的工作区直接 commit」,V4 名片编辑要求全走分支 + MR(2.5 节),这是**行为变更**不是纯增量,实现时别沿用旧的直提交路径。

### 3.2 数据结构增量(与 02 篇「数据与文件」对齐,冲突以 02 篇为准)

| 表 | 增量 | 用途 |
|---|---|---|
| `issue` | 新增 `week_of` / `version` / `tool` / `kind`('business'\|'ops'\|'light') / `origin`('human'\|'auto'\|'agent_split'\|'backfill') / `workflow`;推动指标用 `issue_metric` 关联表(02 篇 §2.2,不在本表存 `metric_ids` JSON) | ⑥⑧块查询、2.4 节 (a) 判据 |
| `metric` | **建议**新增 `role='north_star'` 取值(今天只有 `'leading'`/`'lagging'`);`project.north_star`/`ns_def` 仍管名称/定义文字,这一行管目标值/现值/observation 链/id,让②块复用③④同一套「本周哪些活在推它」逻辑(工作量取舍见第 6 节问题 4) | ②块 join 对三层统一 |
| `release`(新表) | `id` / `project_id` / `version` / `released_at`(INTEGER,unix 秒)/ `note` / `origin`('human'\|'backfill');包含的活用 `release_issue` 关联表(02 篇 §2.5,不在本表存 `issue_ids` JSON;设计期统一,与 `issue_metric` 同一理由) | ⑦⑧块 |
| `week_plan`(新表,仓文件为正本) | `id` / `project_id` / `week_of` / `goal` / `file_path` | ⑥块 |
| `project` | 新增 `standard_version` / `current_version` / `chat_provider` / `chat_group_id` / `chat_notify` | ①⑦块 |
| `chat_outbox`(新表,小) | `id` / `project_id` / `event_type` / `issue_id` / `sent_at` / `status` | 不是总览本身用,但①块「通知同步中」小字依赖它有没有发送记录,归属 07 篇 |

**迁移守则不变**:每加一列同时改 `schema.sql` 并加 `add_column_if_missing`;新表 `CREATE TABLE IF NOT EXISTS` 即是充分守卫。

### 3.3 health 现算怎么实现(与今天 `recompute_signals` 的关系)

今天(V3)是「写透缓存」模式:`Store::recompute_signals`(`crates/bw-store/src/sqlite.rs:1411`)在相关命令执行后被调用,把算好的 Signal 写进 `metric.signal` / `op_stage.routine_signal` / `project.signal` / `project.weekly_signal` 四个缓存列,界面读缓存。`project.weekly_signal` 听起来像「周快照」,但读代码可见它和 `signal` 每次都被同一条 UPDATE 写成相同值(`sqlite.rs:1538` 附近),从未真正按周冻结,只是历史遗留命名。

V4 总览的 health(2.4 节)**不走这条缓存链**:母文档明确「读时现算,库里不存灯」;(a)(b)(c) 本来就按时间窗口现算,换参数即可回算历史周;`recompute_signals` 依赖 `op_stage.routine_schedule` 取保鲜期,而 V4 业务指标是项目级、不挂某个 `op_stage`(五阶段降级为类别标签),这条路径不成立,改成**固定 `Cadence::Weekly`**(是否按指标细分见第 6 节)。`op_stage` 表与 `metric.signal`/`hit`、`project.signal`/`weekly_signal` 几列因此只剩指标卡小圆点这一处消费者——是否退役留给 02 篇,本篇只保证总览读取路径不依赖它们。

伪码(仅本节允许出现,只示意结构,真实 SQL 见 2.4 节读回段):

```rust
// bw-app/src/overview.rs(新)
pub async fn compute_overview_health(store: &dyn Store, pid: ProjectId, now: OffsetDateTime) -> HealthResult {
    let (week, last_week) = (iso_week_of(now), iso_week_of(now - Duration::weeks(1)));
    let has_goal = store.week_plan_exists(pid, &week).await?;
    let progressed = has_real_commit_progress(store, pid, &week).await?;       // workflow_run.head_before<>head_after
    let progressed_last = has_real_commit_progress(store, pid, &last_week).await?;
    let (mut any_red, mut fresh_observation) = (false, false);
    for m in store.list_metrics(pid, &["lagging", "leading"]).await? {
        let measurement = bw_core::derive::measure(&latest_raw(&m)?, ts, source, &Cadence::Weekly, now);
        fresh_observation |= !matches!(measurement, Measurement::Missing);
        any_red |= bw_core::derive::evaluate_metric(&measurement, &m.target, &m.trend).signal() == Signal::Red;
    }
    let (a, b, c) = (has_goal && progressed, fresh_observation, had_merge_or_release(store, pid, &last_week).await?);
    let level = if a && b && c { Signal::Green }
        else if !progressed && !progressed_last { Signal::Red }
        else if any_red { Signal::Red }
        else if !a && !b && !c { Signal::Unknown }
        else { Signal::Amber };
    HealthResult { level, reasons: build_reasons(a, b, c) }
}
```

## 4 · 边界与失败

**不做**:

- 阶段轴与阶段舱(五阶段进度面板)——不带,方法论内容已并入规范扩展件 `docs/method/`,阶段降级为活的类别标签。
- 「进度趋势」的手工维护字段——V4 所有趋势都来自真实 observation 序列,没有一处允许人手描一条趋势线。
- 灯手动设置——总览没有任何「设为绿」的界面路径,信号类型的构造入口在 V3 就已经密封(`crates/bw-core/src/derive/sealed.rs`,类型名见第 3 节),V4 沿用这个约束。
- ⑤块定时自动刷新——只有打开时现算 + 手动「立即采集」,不额外起定时任务采集仓统计(和采集业务指标的定时任务是两回事)。

**失败如实**:

| 场景 | 显示 |
|---|---|
| `PROJECT.md` 缺某一段(如没有「北极星」段) | 「章程不完整」+ 具体缺哪段(措辞沿用 `dispatch.rs` 「章程未补写(PROJECT.md 北极星段可能缺)」的风格) |
| `.bw/project.toml` 解析失败(格式错、未知字段) | 名片区整体灰 + 「配置文件解析失败:{错误}」,不猜测、不用旧缓存顶上 |
| `.bw/metrics.toml` 解析失败 | ②③④三块灰 + 同样的解析错误原文——结构性错误,不是内容问题,和 `docs/buddy/standards/metrics.md`「坏文件只报错不写库」一致 |
| ⑤块 git 子进程失败(非 git 目录 / git 未安装) | 整块显示「无法读取仓统计:{git 原文错误}」,不是空白也不是假数据 |
| ⑧块历史回填某类原料缺失(如无远端 issue 访问权限) | 那一行单独显示「—」+「该来源未取到」,不影响块内其它行——回填允许「原料没有就空着」 |
| health 计算中途查询失败 | 大灯直接灰 + 「health 计算失败:{错误}」,绝不吞掉错误凑一个颜色 |

## 5 · 验收与读回

**深链**:

```bash
BW_DB=<临时db路径> BW_OPEN=<项目名> BW_PANEL=overview ./target/debug/builders-workbench
# stderr 出现 [BW_BOOT] 与 [BW_OPEN] = 渲染成功,无 panic
```

**每块一条读回**:命令已经在 2.3/2.4 节每块的「读回」段给全,验收时按①→⑧顺序逐条跑一遍、配一张截图存档,不重复贴一遍。

**铁律核验**:杀掉进程重开,同一份 DB 上再跑一遍 SQL 与深链截图,数字与 health 结论必须一致——health 是纯函数现算,输入没变结论就不该变,这就是「杀进程重开,数字前后一致」这条铁律在总览的体现。

**验收场景清单**:①全新项目:总览灰,①②③④空态文案,⑧不出现,⑥显示「开始本周」横幅。②跑完一轮运作活①②后:⑥有真实进度,①health 不再灰,三条理由至少一条为真。③名片编辑一次:MR 横幅 → 合入 → 完成 → 新值生效,`sqlite3` 读回已更新。④老项目接入(母文档 §8 验收 7):⑧块每个数字能对回 `git log` 或远端 API,`backfill` 的 issue 不进战绩。⑤配了项目群的项目:群名显示,「未配 · 配置」消失。

## 6 · 开放问题(≤5)

1. **「周信号快照」存不存**:倾向「不存」——(a)(b)(c) 都按时间戳现算,查「上周/本周」换参数即可,不需要额外快照表。但以后若要「过去 8 周灯色时间线」,现算意味着判据算法一改历史灯色跟着变,不是冻结在当时的判断——这是否可接受需要拍板,不接受则要重新引入类似被删的 `weekly_review` 的快照机制。
2. **「进展」判据够不够严**:(a) 目前只认「本周有 `workflow_run` 真实提交」,对构建类活自然,但原型类活、运作活①(产出周计划算不算进展)可能需要补充——要不要扩大到「MR 新评论 / 活状态前进」这类较弱但覆盖面更广的信号,需要拍板。
3. **⑤块刷新频率**:定为「打开现算 + 手动采集」,没有定时;仓大了子进程会变慢,要不要加本机缓存 + 定时后台刷新,先跑起来看真实耗时再说。
4. **北极星「metric 化」现在做还是缓一步**:3.2 节建议的 `role='north_star'` 一行是不小的改动,不做的话②块「推动它的活」先留空或用保留字符串顶一阵,需要和 02 篇一起看工作量。
5. **health 判据 (a) 要不要把运作活①算进去**:目前运作活明确不算业务活,接入后第一周若只做了运作活,health 会偏黄/红——要不要给接入第一周一次豁免,需要拍板。
