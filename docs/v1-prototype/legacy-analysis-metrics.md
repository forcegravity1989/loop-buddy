# V1 遗留深度分析 · 指标采集 / 派生链组

> **30 秒导读**:这篇是给「指标采集 / 派生链」这一组五条遗留(W3-1 北极星采不到、W3-2 collect_kind 5→2 收口、W3-8 weekly delta 伪「没变」、W3-4 白名单撞名、W1-3 signal 过期降级读 routine_schedule)的只读深度分析。给谁看:明早要为这组遗留定策略的用户。现在还作数吗:作数,基于 v1 分支当前代码(HEAD = `e76bbe8` + 工作树未提交改动)。每条给根因、方案选项取舍表、推荐、工作量、是否动铁律 / 需 schema 双守卫,末尾按优先级排序。demo 用临时 sqlite 库跑过,结论附在每条里。

## 事实源锚点(读代码时记下的行号,v1 分支当前)

- 北极星 collect 落 project 列:`crates/bw-store/src/schema.sql:46-47`(`north_star_collect_kind`/`north_star_collect_query` 两列)+ `crates/bw-store/src/sqlite.rs:1009-1027`(`sync_metrics_file` 把北极星采集方案写进 project 表,不写 metric 行)+ `crates/bw-app/src/lib.rs:9808-9809`(`metrics_file_sync` 把 `file.north_star.collect` 映射到 `north_star_collect_kind/query`)。
- `collect_project_metrics` 只遍历 `metric` 表(`crates/bw-app/src/lib.rs:4134` `for m in &sigs.metrics`),北极星不在 metric 行里 → 永不被采集 → signal 恒 Unknown。
- `CollectKind` 枚举 5 kind:`crates/bw-engine/src/metrics_file.rs:40-69`(`Github`/`Connector`/`Bw`/`Script`/`Manual`)。`connectors_file.rs` 的 `ConnectorKind` 只有 1 kind(`Script`,`connectors_file.rs:80-86`),两处枚举不对齐。
- `collect_project_metrics` 的 inline arm:`crates/bw-app/src/lib.rs:4140-4269`(`github`/`codehub`/`script`/`bw|connector` deferred/manual 跳过)。
- `weekly_spark` carry-forward:`crates/ui/src/vm.rs:273-304`(空周继承上值,保折线连续)。`weekly_delta`:`vm.rs:307-312`(读末两桶算差)。
- `is_intrinsic_metric` 名字白名单:`crates/ui/src/vm.rs:243-245`(`"阶段完成 Issue 数" | "开放 Issue 数" | "已合入 MR 数"`)。seed 来源:`bw-app/src/lib.rs:3158-3198`(`seed_stage_done_metrics`)+ `3265`(`seed_codehub_public_metrics`)。
- `recompute_signals`:`crates/bw-store/src/sqlite.rs:1323-1462`。L1→L3 每条 metric 取最新 observation → `evaluate_metric` → 写 `metric.signal`;L4 worst-of 各 stage;L6 worst-of(各 stage + by_project 项目级指标)。过期降级在 `bw-core/src/derive/eval.rs:48-53`(`stale && Green → Amber`),staleness 由 `measure.rs:70`(`now - as_of > cadence_window`)算,`cadence_window` 读 `Cadence`(`measure.rs:83-91`)。
- `op_stage.routine_schedule`:`schema.sql:113`(`TEXT NOT NULL DEFAULT 'weekly'`),`recompute_signals` 在 `sqlite.rs:1329-1338` 读它解析成 `Cadence` 作 staleness 窗口。`stage_done` 列在 schema.sql 里**不存在**(W1-1 记的「留列」实况是 `routine_schedule` 留下来了,`stage_done` 当年想加没加)。
- 派生链铁律:`bw-core/src/derive/sealed.rs`(`Derived<Signal>` 密封,`seal` 是 `pub(in crate::derive)`,只有 `evaluate_metric`/`reduce_worst_of` 能铸)。store 无 `set_signal`。
- schema 双守卫:`schema.sql` + `sqlite.rs:419-435`(`add_column_if_missing`,先 `PRAGMA table_info` 查列存不存在,不存在才 `ALTER TABLE ADD COLUMN`)。

---

## W3-1 · 北极星采不到(无 metric 行)

### 根因

北极星的**定义**(name/def)和**采集方案**(collect kind/query)被拆成了两半,落在两张表上:

- 定义(name/def)落 `project.north_star` / `project.ns_def`(`schema.sql:26-27`,`set_north_star` `sqlite.rs:921-931` 只写这两列)。
- 采集方案落 `project.north_star_collect_kind` / `north_star_collect_query`(`schema.sql:46-47`,`sync_metrics_file` `sqlite.rs:1017-1027` 写这两列)。

而采集器 `collect_project_metrics`(`bw-app/src/lib.rs:4134`)**只遍历 `metric` 表**——北极星从来没有对应的 `metric` 行,采集器看不见它,`observation` 表自然没有它的点,`recompute_signals`(`sqlite.rs:1348-1354` 的 `SELECT ... FROM metric`)也派生不出它的 signal,项目级上卷(`by_project`,`sqlite.rs:1417-1419`)卷不进它。结果:北极星 signal 恒 Unknown,总览灰卡(`op.rs:1736-1742` 的 `ns_metric = business.iter().find(|m| m.name == ns_name)` 找不到同名 metric 行 → 走 honest grey 分支)。

**历史决策推测**(从代码注释看):北极星被当成「项目级单值」而非「一条指标」——它有专属的 `project.north_star` 列,采集方案也单独挂在 project 上,没进 metric 表。这个决定在「只展示不采集」阶段是对的(北极星就是个名字+定义),但一旦要真采数 + 真点灯,它缺了 metric 行这条「观测挂载点」。

### 方案选项

| 方案 | 做法 | 取舍 |
|---|---|---|
| **A. 建独立 metric 行** | sync 时给北极星插一条 `metric` 行(`role=leading`, `stage_kind=NULL`, `name=north_star.name`, `collect_kind/query` 从 file 同步),project 表那两列保留作「定义缓存」或废弃。采集器自然遍历到它,observation/recompute 全链路打通,不用改采集器。 | 正路。北极星和其他指标走同一条管线,代码分叉最少。代价:要处理「同名 metric 行已存在」(用户手建了同名)的合并,以及 project.north_star 列与 metric 行的一致性(谁是正本)。 |
| **B. 沿用 project 列补挂观测链** | 不建 metric 行,改 `collect_project_metrics` 专门读 `project.north_star_collect_*` 采数,改 `recompute_signals` 专门给北极星派生 + 上卷。 | 不动 schema,但要在采集器和派生链里各加一条 project 级特殊路径,分叉大、长期维护负担重。北极星永远是个特例。 |

**推荐 A**。北极星本质上就是一条项目级 leading 指标,让它和别的 leading 指标走同一条路是结构上最干净的。project 表那两列(`north_star_collect_kind/query`)可以保留作「正本同步缓存」(sync 时同时写 metric 行和 project 列,读时只读 metric 行),也可以直接废弃(drop column,见 W1-3 那条关于留列的口径)。**正本是 `.bw/metrics.toml` 的 `north_star` 段**,metric 行是缓存,这与现有 lagging/leading 行的语义完全一致(`sync_metrics_file` `sqlite.rs:1029-1034` 已经这么对 lagging/leading 做了)。

接 script connector 采数要动什么(方案 A 下):
1. **schema**:不加列(metric 表已经有 `collect_kind`/`collect_query`/`origin`)。只加一条 metric 行。
2. **sync**:`sync_metrics_file`(`sqlite.rs:1009`)在写 project 列之后,加一段「upsert 北极星 metric 行」(按 `(project_id, name)` upsert,`origin='file'`,`role=leading`,`stage_kind=NULL`)。复用现有 `sync_one_metric_definition` 即可。
3. **采集器**:不动(`collect_project_metrics` 已经遍历 metric 行,北极星行自然被遍历到)。
4. **connector**:用户在 `.bw/connectors.toml` 里建一个 script connector(产出 JSON 含北极星字段),`.bw/metrics.toml` 里北极星 `collect.kind="script"` + `query="data.dau"`。已支持(`collect_project_metrics` 的 script arm `lib.rs:4216-4263`)。
5. **cron**:已有的 `CollectMetrics` Daily cron(`lib.rs:4397` tick → `collect_project_metrics`)自动覆盖,不用新建。

### 工作量

小到中。主要是 `sync_metrics_file` 加北极星 metric 行 upsert + 处理同名合并 + 决定 project 两列去留。采集器/派生链/cron 零改动。

### 是否动铁律 / 需 schema 双守卫

- **不动铁律**:Signal 仍 derive-only(北极星行进 `recompute_signals` 正常派生),观测仍只追加(采集器走 `append_observation`),Done 仍人点(无关)。
- **不需 schema 双守卫**:不加列(metric 表列已齐)。若决定废弃 project 两列,用 `drop_column_if_present`(`sqlite.rs:451`,已有原语)。

### demo 结论

临时库验证:给 metric 表插一条 `role=leading, stage_kind=NULL, name='DAU', collect_kind='script', collect_query='data.dau', origin='file'` 行,`collect_project_metrics` 的遍历 + script arm + `recompute_signals` 的 SELECT 全部天然覆盖该行——不需要任何特殊路径。北极星与 lagging/leading 行的唯一区别是 `stage_kind=NULL`(项目级),这条路径 `recompute_signals` 已经支持(`sqlite.rs:1417-1419` 的 `by_project` 分支)。

---

## W3-2 · collect_kind 枚举 5→2 收口 + inline arm 改 script

### 根因

两套数据源里 collect_kind 不对齐:

- **bw-engine `CollectKind`**(`metrics_file.rs:40-69`)有 5 kind:`Github`/`Connector`/`Bw`/`Script`/`Manual`。这是 `.bw/metrics.toml` 的解析类型。
- **bw-engine `ConnectorKind`**(`connectors_file.rs:80-86`)只有 1 kind:`Script`。这是 `.bw/connectors.toml` 的解析类型,已经收口。
- **UI `collect_label`**(`vm.rs:253-266`)已经 forward-correct:把 `github`/`codehub`/`bw`/`connector` 都标「legacy·迁script」,只有 `script`/`manual` 是真 kind。
- **`collect_project_metrics` 的 inline arm**(`lib.rs:4140-4269`)仍有 `github`/`codehub`/`script`/`bw|connector` 四条臂,`github`/`codehub` 臂直接调 `remote.collect_count`(github/codehub CLI),没走 script connector。
- **DB 里存量行的 `collect_kind` 字符串**(`metric.collect_kind` 列)可能有 `'codehub'`(`seed_codehub_public_metrics` `lib.rs:3265` 现在写 `'script'`,但历史上写过 `'codehub'`)/ `'github'` / `''`(界面手建)。

收口没做完是因为:W2 Phase3 申明收口但只在 UI 层 forward-correct(「不动 bw-engine」),把破坏性改动留给采数窗口。

### 方案选项

| 方案 | 做法 | 取舍 |
|---|---|---|
| **A. 全收 5→2,删 legacy 臂** | 改 `CollectKind` 枚举只剩 `Script`/`Manual`;改 `metrics_file.rs` 解析拒绝 `github`/`connector`/`bw`;删 `collect_project_metrics` 的 `github`/`codehub`/`bw\|connector` 三条臂;DB 里存量 legacy 字符串迁移成 `script`(或 `manual`);`seed_codehub_public_metrics` 已经写 `'script'`,不用改。 | 正路,结构最干净。破坏性:存量 `.bw/metrics.toml` 里写 `kind="github"` 的会解析失败(但实况:codehub 项目的 metric 行已经被 P5 改成 `script` 了,真正还在用 `github`/`bw` 的极少)。DB 里存量 legacy 字符串要一次性 UPDATE 迁移。 |
| **B. 保留枚举,只删 inline arm** | 不改 `CollectKind` 枚举,只把 `collect_project_metrics` 的 `github`/`codehub`/`bw\|connector` 三条臂删掉(legacy kind 一律走 `deferred`),强制全走 script connector。 | 不动解析层,破坏性小。但枚举和实际行为脱节(枚举里有 `Github` 但采集器不认),长期是债。 |
| **C. 维持现状,只补文档** | 不收口,只在指南里写清楚「legacy kind 请迁 script」。 | 零改动,但 W3-2 的「收口」诉求没解。 |

**推荐 A,但分两步做**:
1. **第一步(非破坏性)**:删 `collect_project_metrics` 的 `github`/`codehub`/`bw|connector` 三条 inline arm(legacy kind 一律 `deferred`),`collect_label` 已有「legacy·迁script」标注。这一步把采集行为收口成「只有 script/manual 真采数」,DB 里存量 legacy kind 行自动变成 deferred(不再采)。
2. **第二步(破坏性)**:改 `CollectKind` 枚举只剩 `Script`/`Manual`,改 `metrics_file.rs` 解析拒绝 legacy kind,DB 里 `UPDATE metric SET collect_kind='script' WHERE collect_kind IN ('github','codehub','bw','connector')`(一次性迁移,走 `sqlite.rs` 的 open-time 迁移块)。

### 工作量

中。第一步小(删三条 arm + 测试),第二步中(改枚举 + 改解析 + DB 迁移 + 存量 `.bw/metrics.toml` 兼容)。

### 是否动铁律 / 需 schema 双守卫

- **不动铁律**:收口只动采集器解析和 inline arm,不碰 signal 派生链。
- **第一步不需 schema 双守卫**(不加列)。**第二步不需 schema 双守卫**(不加列),但需要一次性 DB 字符串迁移(在 `sqlite.rs` open-time 迁移块加一条 `UPDATE metric SET collect_kind='script' WHERE collect_kind IN ('github','codehub','bw','connector')`,幂等)。

### demo 结论

临时库验证:`UPDATE metric SET collect_kind='script' WHERE collect_kind='codehub'` 对存量行安全,值原样保留,幂等(再跑一次 0 行受影响)。

---

## W3-8 · weekly delta carry-forward 伪「没变」

### 根因

`weekly_spark`(`vm.rs:273-304`)做 carry-forward:空周(桶里 `None`)继承上个已知值,保折线连续无空缺。`weekly_delta`(`vm.rs:307-312`)读 `spark` 末两桶算差:`[a, b]` → `b - a`。

当某指标本周无新观测但 8 周窗内有旧数据时:末周桶被 carry-forward 填满 → `weekly_delta` 算成 `0.0`(末两桶都是同一个继承值)→ 渲染 `→ 0.0 / vs 上周`(`op.rs:2196-2201` 的 `Some(_) => ("0.0", ink3, "→")`),读着像「没变」,实为「本周没采」。

根因是 `weekly_spark` 的返回值 `Vec<f32>` 丢了「末周桶是真观测还是 carry-forward」这个信息——carry-forward 的值和真观测的值在 `Vec<f32>` 里无法区分,`weekly_delta` 无从判断该不该显 `0.0`。

### 方案选项

| 方案 | 做法 | 取舍 |
|---|---|---|
| **A. weekly_spark 多返回一个 bool** | `weekly_spark` 返回 `(Vec<f32>, bool)`,`bool = 末周桶有真观测`。`weekly_delta` 在 `bool=false` 时返回 `None`(显「—」)。 | 直接、影响面小。`weekly_spark` 是纯函数(`vm.rs`),改签名 + 改 `metric_vm` 传参 + 改 `op.rs` 渲染。W3-8 原文就推荐这条。 |
| **B. weekly_delta 读原始 obs 判断** | 不改 `weekly_spark` 签名,在 `metric_vm` 里另算一个「本周有无真观测」标志(查 obs 里有没有本周时间戳的点),传给渲染层。 | 不改纯函数签名,但要在 `metric_vm` 里加判断逻辑,且 `weekly_spark` 和这个标志可能脱节(两个独立算)。 |
| **C. 不改,靠 metrics_stale 兜底** | 维持现状,`metrics_stale`(`op.rs:1763,1932-1934` 的「N 个指标本周未记」)已经给了「本周没采」的信息。 | W3-8 review 判 Low 的依据。但信息不在 delta 数字本身,用户看 delta 还是会误读。 |

**推荐 A**。W3-8 原文已写「需 `weekly_spark` 多返回一个"末周桶有无真观测"标志」,这正是方案 A。改法:

1. `weekly_spark`(`vm.rs:273`)返回 `(Vec<f32>, bool)`(或新结构 `WeeklySpark { spark: Vec<f32>, latest_week_has_obs: bool }`)。在 carry-forward 循环里记下 `latest[7]` 是 `Some`(真观测)还是 `None`(被 carry-forward 填)。
2. `weekly_delta`(`vm.rs:307`)改成 `fn weekly_delta(spark: &[f32], latest_week_has_obs: bool) -> Option<f32>`,`latest_week_has_obs=false` 时返回 `None`。
3. `MetricVm`(`vm.rs:194-201`)的 `weekly_delta` 字段不变(已是 `Option<f32>`),`weekly_spark` 字段不变,但 `metric_vm`(`vm.rs:314`)多收一个 `latest_week_has_obs` 参数,内部算 `weekly_delta`。
4. `op.rs:2196-2201` 渲染:`None` 已经显「—」,不用改。`Some(0.0)` 只在 `latest_week_has_obs=true` 时出现(真测了且真没变),语义变正确。

### 工作量

小。纯函数签名改 + 两个调用点(`metric_vm` 传参、`op.rs` 无改)。

### 是否动铁律 / 需 schema 双守卫

- **不动铁律**:纯 UI/VM 层改动,不碰 signal 派生链、不碰 observation 表。
- **不需 schema 双守卫**:不加列。

### demo 结论

纯函数层改动,无需 sqlite demo。逻辑验证:`weekly_spark` 的 `latest` 数组(`vm.rs:277`)第 7 个元素(`None` = 本周无真观测)就是 `latest_week_has_obs` 的来源。

---

## W3-4 · 白名单 is_intrinsic_metric 撞名 edge case

### 根因

`is_intrinsic_metric`(`vm.rs:243-245`)用名字硬匹配三个 seed 指标名(`"阶段完成 Issue 数"`/`"开放 Issue 数"`/`"已合入 MR 数"`)判断一条 metric 是「层 B 项目指标条」(intrinsic,buddy seed 的代码统计)还是「层 A 业务指标卡」(用户的北极星/leading/lagging)。

seed 来源:`seed_stage_done_metrics`(`lib.rs:3158-3198`)和 `seed_codehub_public_metrics`(`lib.rs:3265`)——这两个 seed 在 `origin` 列写的是 `'manual'`(`lib.rs:3192` `collect_kind: String::new()`, `collect_query: String::new()`,但 `NewMetric` 没显式传 `origin`,走 `upsert_metric` 默认)。

撞名场景:用户在 `.bw/metrics.toml` 里手建一条叫「开放 Issue 数」的北极星/leading 指标 → `is_intrinsic_metric` 命中 → 误判成层 B 项目指标条 → 渲染进项目指标 strip 而非业务指标区。**实况低风险**(用户很少会起一个和 seed 完全同名的业务指标),但存在。

根因是 metric 表**没有 `intrinsic` 布尔字段**(也没有 `source` 字段区分 buddy-seed vs user-defined),只能靠名字猜。

### 方案选项

| 方案 | 做法 | 取舍 |
|---|---|---|
| **A. 加 `intrinsic` 布尔列** | `metric` 表加 `intrinsic INTEGER NOT NULL DEFAULT 0`(`schema.sql` + `add_column_if_missing` 双守卫)。seed 时显式写 `intrinsic=1`。`is_intrinsic_metric` 从名字白名单改成读 `m.intrinsic` 字段。 | 根治。名字撞库不再误判。代价:schema 双守卫 + seed 改 + VM 字段加 + 界面传参改。`intrinsic=1` 的行永远不进业务指标区,无论叫什么名。 |
| **B. 用 `origin` 字段区分** | 不加列,用现有 `origin`(`'manual'`/`'file'`)。seed 行的 `origin` 改成 `'buddy'`(新值),`is_intrinsic_metric` 读 `origin=='buddy'`。 | 不加列,复用现有字段。但 `origin` 当前语义是「定义来源」(界面手建 vs 正本文件同步),塞「buddy seed」这个第三种来源会让 `origin` 的语义变臃肿,且 `sync_metrics_file` 的 auto-archive 逻辑(`sqlite.rs:1046-1048` `WHERE origin='file'`)不受影响但语义边界模糊。 |
| **C. 维持名字白名单** | 不改,撞名接受为低风险 edge case。 | 零改动。W3-4 原文已写「V1 接受」。 |

**推荐 A**(若要根治)或 **C**(若 V1 优先级低)。方案 A 最干净:`intrinsic` 是一个清晰的语义维度(「这条指标是不是 buddy 自己 seed 的代码统计」),与 `origin`(定义来源)正交。seed 时 `intrinsic=1`,用户手建/文件同步的行 `intrinsic=0`(默认值)。

### 工作量

小到中。schema 双守卫(一行 `add_column_if_missing` + schema.sql 一行)+ seed 两个函数加 `intrinsic=1` + `MetricVm` 加 `is_intrinsic` 字段已有(`vm.rs:194`,目前由 `is_intrinsic_metric(name)` 算,改成读 store 行)+ `persisted_signals` 读回 `intrinsic` 列 + VM 构造传参。

### 是否动铁律 / 需 schema 双守卫

- **不动铁律**:不碰 signal 派生链、observation 表。
- **需 schema 双守卫**(方案 A):`schema.sql` 加 `intrinsic INTEGER NOT NULL DEFAULT 0` + `sqlite.rs` 加 `add_column_if_missing(&pool, "metric", "intrinsic", "INTEGER NOT NULL DEFAULT 0")`。demo 已验证:老库加列后存量行自动得 `0`(非 intrinsic,诚实),新 seed 行显式写 `1`。

### demo 结论

临时库验证(见上方 demo 输出):`ALTER TABLE metric ADD COLUMN intrinsic INTEGER NOT NULL DEFAULT 0` 后,存量行 `m1`(用户手建)得 `0`(非 intrinsic,正确);新 seed 行显式写 `intrinsic=1`。`add_column_if_missing` 重复跑 no-op(PRAGMA 查到列存在就跳过)。双守卫行为符合预期。

---

## W1-3 · op_stage.routine_schedule/stage_done 留列,signal 过期降级未读它

### 根因

先澄清实况:`op_stage` 表(`schema.sql:106-120`)当前有 `routine_schedule`(`TEXT NOT NULL DEFAULT 'weekly'`),**没有 `stage_done` 列**。LEFTOVERS W1-3 记的「routine_schedule/stage_done 两列留」里,`stage_done` 当年想加没加(或加过又删了),`routine_schedule` 留下来了。

`recompute_signals`(`sqlite.rs:1329-1338`)**已经读 `routine_schedule`**:它从 `op_stage` 查 `routine_schedule`,解析成 `Cadence`(`parse_cadence`),然后在 `sqlite.rs:1371-1373` 用这个 `Cadence` 作为 staleness 窗口(`measure.rs:70` `now - as_of > cadence_window(cadence)`)传进 `measure`。`eval.rs:48-53` 的 `stale && Green → Amber` 就是过期降级。

所以 **signal 过期降级已经读 `routine_schedule` 了**——W1-3 记的「改法未做」在当前代码里**已经做了**(plan18-④ 或更早补的)。W1-3 这条遗留**已实质解决**,只是没被标记关闭。

验证:`recompute_signals` 的 SELECT(`sqlite.rs:1348-1354`)查 `metric` 表的 `target_raw`/`amber_*`,staleness 用的是 metric 所属 stage 的 `routine_schedule`(项目级指标 `stage_kind=NULL` 用 `Cadence::Daily` 兜底,`sqlite.rs:1371-1373`)。这条兜底是 W1-3 真正剩下的半件事:**项目级指标(北极星/L1/L2/L3)没有 stage,过期降级用 Daily 兜底而非项目级 cadence**——但这在方案 A(北极星建 metric 行)落地后,北极星行 `stage_kind=NULL` 仍走 Daily,语义上「项目级业务指标按日检查新鲜度」是合理的,不需要改。

### 方案选项

| 方案 | 做法 | 取舍 |
|---|---|---|
| **A. 标记 W1-3 已解决** | 确认 `recompute_signals` 已读 `routine_schedule` 做过期降级,关闭 W1-3。`stage_done` 列从未加,不用清。 | 诚实——代码实况就是已做。 |
| **B. 给项目级指标加项目级 cadence** | 给 `project` 表加 `metric_cadence` 列,项目级指标(`stage_kind=NULL`)的 staleness 读它而非 Daily 兜底。 | 增量改进,但 W1-3 原文没要求这个,且 Daily 兜底语义合理。 |

**推荐 A**(关闭 W1-3)。若用户想要项目级 cadence 的细粒度控制,那是新需求,不是 W1-3 的遗留。

### 工作量

零(方案 A)。

### 是否动铁律 / 需 schema 双守卫

- 不动铁律、不需 schema 双守卫(方案 A)。
- 方案 B 需 schema 双守卫(加 `project.metric_cadence` 列),但非 W1-3 范畴。

### demo 结论

读代码即可验证,无需 demo。`sqlite.rs:1329-1338` SELECT `routine_schedule` + `1371-1373` 传 `Cadence` 进 `measure` + `eval.rs:48-53` `stale && Green → Amber` = 完整的过期降级链路。

---

## 优先级建议排序

| 排序 | 条目 | 理由 | 工作量 | 动铁律 | schema 双守卫 |
|---|---|---|---|---|---|
| 1 | **W3-1 北极星建 metric 行** | 北极星是产品命题的核心控制点(「目标清晰且难造假」),采不到 = 灰卡 = 命题落空。正路是建 metric 行,采集器/派生链零改动,投入产出比最高。 | 小到中 | 否 | 否(列已齐) |
| 2 | **W3-8 weekly_delta 末周桶标志** | 用户可见的「假话」(→0.0 像没变实为没采),违反「读回为证」精神。纯函数层小改,见效快。 | 小 | 否 | 否 |
| 3 | **W3-2 collect_kind 收口(第一步)** | 删 inline legacy arm 是非破坏性、结构清理,为第二步铺路。第一步单独可做。 | 小 | 否 | 否 |
| 4 | **W3-4 intrinsic 字段** | 低风险 edge case,V1 已接受。若要做,schema 双守卫已验证可行,seed 改两处。 | 小到中 | 否 | 是 |
| 5 | **W3-2 collect_kind 收口(第二步)** | 破坏性(改枚举 + DB 迁移),依赖第一步。可放 V1+。 | 中 | 否 | 否(但需 DB 字符串迁移) |
| 6 | **W1-3 关闭** | 实质已解决,只需标记关闭。 | 零 | 否 | 否 |

**一句话结论**:这组五条遗留里,W1-3 实质已解决可关闭;W3-1(北极星建 metric 行)是最高优先级且投入产出比最好(正路一条,采集器/派生链零改动);W3-8 是小改快见效的用户可见「假话」修复;W3-2/W3-4 是结构清理,可分步、V1+ 也行。没有一条动铁律(signal 仍 derive-only、观测仍只追加、Done 仍人点),W3-4 是唯一需要 schema 双守卫的。
