# Builders' Workbench — 开发指南 (P0 + P1 + P2)

Rust 桌面应用,按 [`plan/00-PLAN.md`](plan/00-PLAN.md) 路线图实现。当前进度:**P0 · 基座**、**P1 · 架构脊椎**、**P2 · 纵切 UI(创建引导流 + 监控运行流)** 均已落地。真 Dioxus 桌面壳已架上已证脊椎;原型中的模拟项目**没有**被移植 —— 项目墙从空态开始,一切数据都由真实输入产生。

## 工作区布局

```
crates/
  bw-core/      ✅ 领域内核 + 度量派生链(P0)+ parse_magnitude(P2,趋势数值化)
  bw-engine/    ✅ Executor 契约 + MockExecutor(P2:可配延迟,驱动实时进度流)
  bw-store/     ✅ SQLite 持久化;P2 增读侧:list_stages / list_observations /
                   metric 全字段;写侧:set_brief / update_week_plan / set_stage_progress
  bw-app/       ✅ Command/Event 总线;P2 新命令:Boot / UpdateBrief / UpdateWeekPlan /
                   RecordObservation / SetStageProgress / SelectSession;RunWorkflow 实时 emit
  ui/           ✅ 纯函数 selector + P2 ViewModel 层(ui::vm:项目卡/环节轴/MetricVm/
                   周计划/观测 feed/时间标签,全部单测)
  app-desktop/  ✅ P2 真壳【Dioxus 0.7 desktop】:kernel 桥(独立 tokio 线程) +
                   项目墙 / 8步向导 / 运营视图(进度·工作流·定时任务)
  app-web/      —  非成员,"以后也许" 留口(Tier E)
```

`default-members` 只含无头内核 + ui,故日常 `cargo test` / `cargo check` **不编译 Dioxus**。桌面壳需显式 `-p app-desktop`。

## 常用命令

```bash
cargo test                       # 内核 + selector + vm + compile-fail doctests
cargo test -p bw-app             # 脊椎(spine)+ 监控回路(monitor)集成测试
cargo run -p app-desktop         # 启动桌面应用(BW_DB=path 可覆盖数据库位置)

# CI 同款门禁(本地可跑):
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
```

## 两条不可妥协 —— 已钉进类型(P0 起不变)

1. **UI 无关内核**:内核 crate 禁依赖 dioxus/tauri/wry/leptos(CI guard);wasm32 check 保活 Web 留口。
2. **健康永远 derive**:`Signal` 只能经封口的 `Derived<Signal>` 进入缓存;store 无 `set_signal`;`recompute_signals` 是唯一信号写入者。

## P2 · 两条真实流(本阶段主交付)

**创建引导流**(`view=wizard`,8 步全真输入 —— 原型连项目名都是写死的,这里不是):

| 步 | 收集 | 落库命令 |
|---|---|---|
| 0 引子 | 项目名 / 类型 / 一句话描述 | `CreateProject` |
| 1 竞品洞察 | 对标竞品名单 | `UpdateBrief` |
| 2 差距分析 | 机会缺口 | `UpdateBrief` |
| 3 北极星 | 指标 + 计算口径 | `UpdateNorthStar` |
| 4 引领指标 | name/def/当前值/目标 ×n | `UpsertManualMetric`(值→Manual 观测) |
| 5 滞后指标 | 同上 | `UpsertManualMetric` |
| 6 原型即规格 | (方法论,无输入) | — |
| 7 周计划+自评 | 本周目标/抓手;green/amber/red 自评 | `UpdateWeekPlan` → `CompleteWizard` → `AnnotateWeeklyReview` |

自评映射保持诚实:绿=不覆写;黄/红=更悲观 override 连理由入 `weekly_review` 审计。
`UpsertManualMetric` 带幂等守卫:重复确认同一步不会刷重复观测,只有**变化的值**才是新事实。

**监控运行流**(`view=app`):

- **监控回路**:指标卡「记录本周值」→ `RecordObservation`(append-only)→ `recompute_signals` → 信号翻转沿 L2→L4→L6 上卷,项目墙圆点同步变色。sparkline/feed 全部来自真实观测史(`list_observations`),一个观测=一个点,绝不插值。
- **运行回路**:环节「▶ 运行」→ `StartSession` + `RunWorkflow`(每环节内置标准工作流模板,方法论内容非模拟数据)→ MockExecutor(450ms/阶段)→ 进度事件**实时**流向 UI 横幅 → 产出落为会话消息,chat 可继续对话(mock 回复;真执行器=同事团队经 `Executor` trait 热插拔)。
- **周期性诚实**:`Boot` 与 `OpenProject` 都对运营中项目重算信号 —— 过期观测按节奏窗口封顶 Amber,绝不把陈旧缓存当新鲜真相。
- 环节进度是**计划数据**(非信号),`SetStageProgress` 手动维护并累计真实趋势史。

## P1 出口闸门(不变)+ P2 新增测试

- [`spine.rs`](crates/bw-app/tests/spine.rs):建项目→向导→workflow→落库→杀进程重开,持久化信号==独立重算。
- [`monitor.rs`](crates/bw-app/tests/monitor.rs)(P2):RecordObservation 翻转信号且上卷到项目级;UpdateWeekPlan 移动目标后重派生;运行进度事件先于持久化消息(证实时流);Boot 重算 + brief 持久化。
- `ui::vm` 单测:趋势=观测史、无数据=Unknown 不冒充绿、feed 最新条回声当前信号、周计划只取引领指标。

## 已知待办(P3 及以后)

- 产物 / 版本面板、工作流库、9 个 Hub 全屏、右栏目录树 —— P3 铺屏(占位已注明,不放模拟数据)。
- CJK 字体 `asset!()` 本地 bundle(当前用系统字族回退:Songti/PingFang 等)—— P3 保真税。
- Connector/Cron 真喂指标 —— Tier D;签名/打包 —— Tier B。
