# V1 Issue 3 · 总览页重构 — 开发事实源

> 走 (c):现有 buddy app 当底。本文是设计 + 开发唯一事实源(SubAgent 照此建)。5 步法:scope delta ✅ → 对齐原型 ✅(HTML 高保真已对齐)→ 提 issue ✅(Issue 3)→ 开发 → 验证 → 填指南。
> ⚡ 遗留问题统一归拢于 `docs/v1-prototype/LEFTOVERS.md`(唯一完整清单,W1-1~4/W2-1~7/W3-1~9/V1-P1);本文 §3 边界表及各「留窗口」条目为设计过程原文留存,读遗留以 LEFTOVERS 为准。
>
> 基线:worktree 在 v1(`a4f8339` = W1 全量)。W2 代码未合 v1;W2 终态按 `docs/v1-prototype/issue2-metrics-interactive-loop.md`(Phase 1+2a+2b ✅,Phase 3+4 ⬜)对齐——W2 申明的活 W3 不碰(见 §3)。
>
> 视觉事实源:`docs/v1-prototype/issue3-overview-mockup.html`(高保真原型,toggle 现状/提议,default 提议)。
>
> **穿刺修正(2026-08-06)**:本文 §1 的「项目指标 compact 一行小条」实践后被推翻——用户要卡片(值+delta+按周趋势),见 `docs/v1-prototype/piercing-fixes-1.md` §8(Round 3 strip→卡片)/§9(Round 4 定型:2 仓卡+阶段完成一行)/§10(Round 5+5c)。项目指标区最终态:2 张代码仓卡(BizMetricCard,intrinsic 不点灯)+ 阶段完成一行五阶段。以 piercing-fixes-1.md 为准。

## 0. scope delta(读现状取证,不基于猜)

**指标→signal→总览 闭环在 v1 上已通**(两 Explore agent 取证):
- cron→script connector→observation→recompute→UI 全接:`collect_project_metrics`(`bw-app/src/lib.rs:3184`,cron tick `:3569` 调)→ `recompute_signals`(`bw-store/src/sqlite.rs:996`,observation 写后调 `lib.rs:3449`)→ `MetricCard`(`op.rs:1908`)。
- W1 seed:`seed_stage_done_metrics`(`lib.rs:2498`,名「阶段完成 Issue 数」,Leading,stage-scoped,`feed_stage_done_count` `:2607` 用 Telemetry 喂)+ `seed_codehub_public_metrics`(`lib.rs:2544`,开放 Issue 数 / 已合入 MR 数,Lagging,project-level,`collect_kind=script`)+ `.bw/collect_stats.sh`(`lib.rs:7758`)+ script connector(`lib.rs:5132`)+ Daily `CollectMetrics` cron(`lib.rs:5171`)。
- **L6 上卷改已做**(plan18-④):`sqlite.rs:1107-1126`,L6 把项目级业务指标卷进 `reduce_worst_of`,北極星 Green 拉亮项目卡。**W3 不做**。
- `collect_kind` 枚举 5 kind(`Github/Connector/Bw/Script/Manual`,`bw-engine/src/metrics_file.rs:40`)——W2 收两 kind 只 forward-correct 了文档,代码没收(留 W2 Phase3)。
- **北極星采不到**:collect 落 `project` 列(`north_star_collect_kind/query`)非 metric 行,挂不上观测 → v1 留白,signal Unknown(`lib.rs:3143`)。**W3 如实渲染灰,不越界建采数(窗口二/采数的活)**。
- `SyncMetricsFile`(`lib.rs:3149`,store `sqlite.rs:762`):metrics.toml→metric 表(`origin=file`),设计上不触发 recompute(`lib.rs:342`)。merge 后自动 fire(`lib.rs:7468`)+ op.rs 手动按钮(`:1793-1797`)。

**总览(ProgressAll)现状 = 扁平 10 卡竖堆**(`op.rs:1646-1845`):健康概览 / 本周复盘 tile / 编辑项目 / 工作目录 / 接入仓库 / 总进度 / 3 stat / 本周计划(leading-only) / 项目级业务指标 grid(混 leading+lagging 无标注 + ↻同步按钮) / 阶段 list。
病:配置卡抢主视觉、业务+固有混一锅、lagging 无标注、计划和指标没联动、↻按钮、健康概览卡在总览里。

## 1. 设计(v2 总览重构,高保真已对齐)

走 c:现有 app 当底重构,**不照搬** `design/ops-page-reduction/` 原型(它的"关口收件箱"不在 plan/18/brief、4 阶段轴动 `StageKind` 域出 scope),只借它"引领/滞后拆分"思路。

**v2 总览(ProgressAll)布局,顶到底**(对照 `issue3-overview-mockup.html` 提议视图):
1. **顶栏**(`op.rs` TopBar `:88-121`):项目名 + 健康灯 + [运营中] + 周期 + **北極星一句话常驻**(不截断)。
2. **阶段轴**(`op.rs` StageAxis `:124-186`):◎全部·总览 + 5 stage chip(已带信号点,**不再单列阶段 list**)。
3. **工具栏**(`op.rs` Toolbar `:191-242`):6 panel 按钮(进度 active)。
4. **【项目指标】**(代码仓级·所有项目都有·compact 一行小条,置顶):开放 Issue 数 / 已合入 MR 数 / 阶段完成 / 每周结算 / 每周合并 PR —— 用**白名单**分流到这层(§2);标「只当现状数·不上卷健康·带来源徽」。**不点灯**。
5. **【业务指标】**(顺序固定 **北極星 → 滞后 → 引领**;每条一个值卡片,并排放 grid):每卡 = 当前值+目标+信号灯 → delta 上周变化 → 按周折线(从 observation 时序聚周)。北極星卡大/高亮 + 采集链状态行。**引领卡额外带「本周目标+达成」**(本周计划折进引领卡,计划指指标·指标验计划)。
6. **一条诚实灰 case**:无观测指标显灰 + 折线空 + delta「—」+ Unknown≠绿,不假装。
7. **【buddy 情况·一行】**(非卡):需人关注的一行(阶段 / 评审中 Issue / 北極星连两周未动建议复盘)。
8. **【▾配置】**(折叠收次级):编辑项目 / 工作目录 / 接入仓库。

**移走/退场/拿掉**:
- `HealthOverviewCard`(`op.rs:307-349`,现 `:1671`)→ 移到 `wall.rs`(项目列表入口墙,现无概览)。总览不重复。
- ↻同步按钮(`op.rs:1793-1797`):**W3 不删**(W2 Phase3 已申明退场,按窗口边界留 W2;v2 总览暂保留在不碍眼处,W2 P3 退场)。**偏差**:HTML 原型显退场,按用户窗口边界纪律 W3 不动 W2 申明的活。
- 阶段 list / 总进度 / 本周复盘 tile:从总览拿掉(阶段轴已显信号、buddy 情况行替复盘)。

## 2. 项目指标 vs 业务指标 区分(白名单,不加字段)

用户定:metric 表已膨胀,不加字段;项目代码仓指标本质是 buddy 自带 → **白名单**(已知 seed 名字集合)区分,不动 schema。
- 白名单 = W1 seed 的 metric 名字常量:`「阶段完成 Issue 数」`(`seed_stage_done_metric_name` `lib.rs:2491`)+ `「开放 Issue 数」`/`「已合入 MR 数」`(codehub `PAIR` `lib.rs:2566-2577`)。
- `is_intrinsic_metric(name) -> bool`(ui 层静态集合;命中=项目指标层 B,未命中=业务指标层 A)。
- VM/kernel:`op.metrics` 按白名单分流 `intrinsic`(项目指标条)/ `business`(业务指标卡)。
- 边界:用户手建指标若名字撞白名单会误判(低风险,V1 接受);新 seed 指标需同步白名单。

## 3. W2/W3 边界(不碰 W2 申明的活)

| 项 | 归属 | W3 做? |
|---|---|---|
| 总览 UI 重构(op.rs/wall.rs/ui vm) | W3 | ✅ |
| 项目/业务指标白名单分流(UI 层) | W3 | ✅ |
| 采集来源徽两 kind 呈现(script\|manual + legacy 标) | W3(总览 UI badge,W2 未申明) | ✅ |
| 周 delta + 按周折线(VM,从 observation) | W3 | ✅ |
| 采集链状态呈现(VM:connector/cron tick/observation) | W3 | ✅ |
| collect_kind 枚举收 5→2 + inline arm 改 script | W2 Phase3(申明) | ❌ 不碰 |
| 绑数据 skill + `.bw/scripts/` 正规化 | W2 Phase3 | ❌ |
| ↻同步按钮退场 | W2 Phase3(申明) | ❌ 留 W2 |
| 北極星采数(建 metric 行+接 connector) | 窗口二/采数 | ❌ 如实灰 |
| 嵌入终端(xterm/hook/resume) | W2 Phase2b(已成,W2 分支) | v1 基线无,merge 时 W2 自解 |

## 4. phase 拆分(逐 commit 不 push)

- **Phase 1 · VM/kernel 数据契约**:`ui/src/vm.rs` `MetricVm`(`:167-190`)加 `is_intrinsic`/`weekly_delta`/`weekly_spark`(按周聚 observation)/`collection_chain`(connector+cron tick+has_obs);`is_intrinsic_metric` 白名单 helper;`kernel.rs`(OpVm `:1132-1179`/`build_vm` `:677`)从 store(`list_observations` 时序聚周、`cron_task.last_run_at`、connector)填;来源徽两 kind forward-correct(`Github/Connector/Bw`→「legacy 迁 script」标、`Script`→script、`Manual`→manual)。**不动 schema、不动 Signal 派生**。
- **Phase 2 · UI 重构**:`op.rs` ProgressAll(`:1646-1845`)重排为 v2(项目指标条置顶 / 业务指标值卡 北極星→滞后→引领 / 本周计划折进引领卡 / buddy 情况一行 / 配置折叠);移除阶段 list+总进度+本周复盘 tile;`wall.rs` 加 `HealthOverviewCard`(从 `op.rs:307-349` 移植);↻按钮保留(W2 退场)。**守 UI 无关内核**(改只在 app-desktop+ui,过 `guard-kernel-ui-free.sh`)。
- **Phase 3 · 指南**(= S6,见 §6)。

## 5. 文件级改动 + 锚点

- `crates/ui/src/vm.rs`(`MetricVm:167-190`/`week_plan_rows:249-`/`metric_vm:197-229`):加字段 + 白名单 + 两 kind 徽。
- `crates/ui/src/lib.rs`(`signal_color:15-22`/`overview_attention:162-176`):复用,不动 signal 逻辑。
- `crates/bw-app/src/kernel.rs`(`OpVm:1132-1179`/`week_plan:926`/`build_vm:677`):填新 VM 字段(observation 时序聚周、`cron_task.last_run_at`、connector)。
- `crates/app-desktop/src/screens/op.rs`(`ProgressAll:1646-1845`/TopBar `:88-121`/StageAxis `:124-186`/Toolbar `:191-242`/MetricCard `:1908-1954`):重排 v2。
- `crates/app-desktop/src/screens/wall.rs`:加 `HealthOverviewCard`。
- **不动**:`bw-store/src/sqlite.rs`(recompute/L6 已对、sync 不动)、`bw-engine/src/metrics_file.rs`(`CollectKind` 枚举留 W2)、`bw-core/src/model.rs`(Signal/StageKind 不动)、`schema.sql`(不加字段)。

## 6. 填指南(S6)

- **u5 总览呈现**(`docs/guide/buddy-guide.html:343-348`,现占位):四段式 操作→背后→得到什么 + callout(坑) + linker(`go('m6')`/`go('m2')`)+ foot。背后用白话三轴(远端代码仓/用户机器/buddy 数据库),不写 `Command::`。截图按步放(gitignored 本地 `docs/guide/img/`)。
- **m6 指标与健康**(`buddy-guide.html:457-465`,现 W1 一张卡「纳入时已落」):**保住 W1 卡,在后面补** 指标定义 / 采集(script connector→cron→observation)/ 重算·点亮(`recompute_signals` 三态+Unknown+stale 降级)/ 健康灯(derive-only·绿隐身·sqlite 可查)概念卡。`go('m2')` 反链 connector/script/cron 不重抄。script kind「计划中」→「已接」校准(plan18-③ 已接)。
- **LEFTOVERS**(`docs/v1-prototype/LEFTOVERS.md`):北極星采数(→窗口二/采数);collect_kind 枚举收口(→W2 P3/采数);↻按钮退场(→W2 P3);白名单名字撞名 edge case;W2 Phase2b 嵌入终端 merge 时 coexist(→W2 账)。

## 7. 验证(S5,读回为证)

- 门禁:`cargo fmt --all --check` + `cargo clippy --workspace --exclude app-desktop -- -D warnings` + `cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features` + `cargo check -p ui --target wasm32-unknown-unknown` + `./scripts/guard-kernel-ui-free.sh` + `cargo check -p app-desktop`。**`cargo test`**(CI 跑,本地漏)。
- 深链:`BW_OPEN=<项目> BW_PANEL=progress cargo run -p app-desktop`(Windows,不跑 exe 崩 0xC0000135)→ stderr `[BW_OPEN]` = 渲染证。
- sqlite 读回:`SELECT name,collect_kind,signal FROM metric`(白名单名字→项目指标条,验 UI 分流);`SELECT metric_id,ts,raw FROM observation ORDER BY ts DESC LIMIT 5`(验周折线数据源);`SELECT signal FROM project`(验上卷)。
- 读图:`claude -p --model haiku "读取图片 <path> 并简短描述"`(主模型不看图)。截图 gitignored 本地 `docs/guide/img/`。
- 诚实口径:无数据=Unknown≠绿;Done 永不自动;manual 戴徽;数字 sqlite 可查。

## 8. 事实源

- 现状代码:`op.rs`(ProgressAll `:1646-1845`/HealthOverview `:307-349`/本周计划 `:1743-1778`/业务指标 grid `:1784-1807`/↻按钮 `:1793-1797`/MetricCard `:1908-1954`/TopBar `:88-121`/StageAxis `:124-186`/Toolbar `:191-242`)、`wall.rs`(项目列表,无概览)、`ui/vm.rs`(`MetricVm:167-190`/`week_plan_rows:249-`/`metric_vm:197-229`)、`ui/lib.rs`(`signal_color:15-22`/`overview_attention:162-176`)、`bw-app/kernel.rs`(`OpVm:1132-1179`/`week_plan:926`/`build_vm:677`)、`bw-app/lib.rs`(`seed_stage_done:2498`/`seed_codehub:2544`/`collect_project_metrics:3184`/`sync_metrics_file_for:3149`/`MergeIssuePr:7456`/`feed_stage_done_count:2607`/`json_field_by_path:7652`)、`bw-store/sqlite.rs`(`recompute:996`/L6 `:1107-1126`/`sync_metrics_file:762`/`persisted_signals:1176`)、`bw-engine/metrics_file.rs`(`CollectKind:40`)、`schema.sql`(metric `:57-83`/observation `:86-94`)。
- W2 终态:`docs/v1-prototype/issue2-metrics-interactive-loop.md`(Phase 1+2a+2b ✅,Phase 3+4 ⬜)。
- 视觉:`docs/v1-prototype/issue3-overview-mockup.html`。
- 纪律:`CLAUDE.md`、`docs/guide/填写规范.md`、`.claude/skills/v1-product-delivery/SKILL.md`、`plan/18-step3-metric-loop-tie-up.md`。
