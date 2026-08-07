# V1 指标采集板块 · 对齐 + 设计(本会话开发)

> 30 秒导读:窗口二(指标采集/派生链)对齐+设计。基于 `docs/v1-prototype/legacy-analysis-metrics.md`。本批做 W3-1 + W3-2 第一步 + W1-3 关闭;W3-4 不做(用户决议接受低风险,不加代码);W3-8 已做(quickfix `last_week_has_real_obs`)。守铁律,逐 commit 不 push。

## 范围(对齐拍板)

| # | 遗留 | 本批 | 方案 |
|---|---|---|---|
| W3-1 | 北极星采不到(无 metric 行) | 做 | `sync_metrics_file` 写 project 列后 upsert 北极星 metric 行(role=Leading, stage_kind=NULL),采集器/派生链/cron 零改动,project 两列保留作缓存 |
| W3-2 第一步 | collect_kind 收口(删 inline legacy arm) | 做 | 删 `collect_project_metrics` 的 github/codehub/bw\|connector 三臂(legacy 一律 deferred,不再直采,强走 script connector);第二步(改枚举+DB 迁移)放 V1+ |
| W1-3 | signal 过期降级读 routine_schedule | 关闭 | `recompute_signals` 已读 `routine_schedule` 做降级(代码实况已解),标记关闭 |
| W3-4 | 白名单撞名 | 不做 | 用户决议:非必现不管,不加代码(接受低风险 edge case) |
| W3-8 | weekly delta 伪没变 | 已做 | quickfix:`last_week_has_real_obs` 守末周桶 |

## W3-1 设计

**现状**:`sync_metrics_file`(`sqlite.rs:1031`)写 project 表的 `north_star/ns_def/north_star_collect_kind/north_star_collect_query` 四列(1038-1049),lagging/leading 走 `sync_one_metric_definition`(1051-1056)写 metric 行。北极星**没有 metric 行** → 采集器 `collect_project_metrics`(`lib.rs:4134` 只遍历 metric 表)扫不到 → observation 无点 → signal 恒 Unknown → 总览灰卡。

**改法**:在 sync_metrics_file 写完 project 列(1049)后、lagging/leading 循环前,加一段「upsert 北极星 metric 行」:按 `(project_id, name)` upsert,`role=Leading`、`stage_kind=NULL`(项目级)、`name=north_star_name`、`def=north_star_def`、`collect_kind=north_star_collect_kind`、`collect_query=north_star_collect_query`、`origin='file'`。复用 `sync_one_metric_definition`(若签名兼容)或直接 upsert SQL。

**采集器/派生链/cron 零改动**:`collect_project_metrics` 已遍历 metric 行,北极星行自然被遍历;`recompute_signals` 的 `by_project` 分支(`sqlite.rs:1417-1419`)已支持 `stage_kind=NULL`;`CollectMetrics` Daily cron 自动覆盖。

**project 两列保留**(用户决议:避免有地方读它)。sync 时同写 project 列 + metric 行;读只读 metric 行(正本 sync 两条都写,行为双写,读单源)。

**同名合并**:用户若手建了和北极星同名的 metric 行,upsert 按 `(project_id, name)` 覆盖(origin='file' 覆盖 origin='manual')。这和 lagging/leading 的 sync 行为一致(sync 覆盖 manual 同名)。

**铁律**:不动(signal derive-only,北极星行进 recompute 正常派生;观测只追加,采集走 append_observation;Done 无关)。不需 schema 双守卫(metric 列已齐)。

## W3-2 第一步设计

**现状**:`collect_project_metrics`(`lib.rs:4134-4269`)有 inline arm:`github`/`codehub`/`script`/`bw|connector`(deferred)/`manual`(跳过)。github/codehub 臂直接调 `remote.collect_count`(github/codehub CLI),没走 script connector。UI `collect_label`(`vm.rs:253-266`)已 forward-correct 标 legacy。

**改法(第一步,非破坏)**:删 `collect_project_metrics` 的 `github`/`codehub`/`bw|connector` 三条 inline arm——这些 kind 一律走 `deferred`(不再直采,采集器不处理,留待用户迁 script connector)。`script`/`manual` 臂不动。`collect_label` 已标「legacy·迁script」。

**第二步(放 V1+)**:改 `CollectKind` 枚举(`metrics_file.rs:40`)只剩 Script/Manual + DB `UPDATE metric SET collect_kind='script' WHERE collect_kind IN ('github','codehub','bw','connector')` 一次性迁移。破坏性(老 metrics.toml `kind=github` 解析失败),本批不做。

**铁律**:不动(采集器 inline arm,不碰 signal/observation schema)。第一步不需 schema 双守卫。

## W1-3 关闭

`recompute_signals`(`sqlite.rs:1329-1338`)已读 `op_stage.routine_schedule` 解析成 `Cadence` → `measure.rs:70` staleness → `eval.rs:48-53` `stale && Green → Amber`。W1-3 记的「改法未做」代码实况已做。标记 LEFTOVERS W1-3 关闭。`stage_done` 列从未加,不用清。

## 开发顺序

W3-1(sync upsert)→ W3-2 第一步(删 inline arm)→ W1-3 标记关闭。每件逐 commit 不 push,门禁 + cargo test。
