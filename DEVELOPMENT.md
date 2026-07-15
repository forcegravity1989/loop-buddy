# Builders' Workbench — 开发指南 (P0 + P1 + P2 + 五阶段方法论迁移 + 剧本执行)

> **完整形态增量（2026-07-13）**:五角色从静态元数据升级为**可真实执行的剧本**
> (`bw-core::playbook`):每阶段的方法循环 phase 各带一条真实指令(注入项目上下文
> 与上一棒交接词),经 `Command::RunStagePlaybook` → `ClaudeCliExecutor` 在项目
> 工作区真实产出文件/提交/测试;`bw-engine::evidence` 把工作区真实状态(git 提交数/
> docs 产物/测试通过率)经 `Command::RecordCollectedObservation`(Ci/GitPr 来源,
> 绝不 Manual)回流度量派生链 —— 首个非手填 L0 生产者。`WorkflowSpec.phase_prompts`
> 平行字段(serde default + `add_column_if_missing` 双守卫)承载 per-phase 指令,
> 空 = 旧行为字节不变。phase 间接力:上一 phase 输出尾部注入下一 phase(relay baton)。
> headless 指挥器 `cargo run -p bw-app --example real_demo` 驱动完整 0→1 生命周期。

Rust 桌面应用,按 [`plan/00-PLAN.md`](plan/00-PLAN.md) 路线图实现,叠加**体系重构 v2 · 阶段=角色=方法论**的架构迁移。当前进度:**P0 · 基座**、**P1 · 架构脊椎**、**P2 · 纵切 UI**均已落地,并已从「7 控制点线性向导」整体迁移到「5 阶段=角色=方法论 · 交棒制 · 线闭成环」模型。原型中的模拟项目**没有**被移植 —— 项目墙从空态开始,一切数据都由真实输入产生。

## 工作区布局

```
crates/
  bw-core/      ✅ StageKind(5 阶段,每个自带角色/方法论/求什么/循环节奏/核心问题/
                   方法循环/DoD/交棒文案/AI编队/反模式静态元数据)+ ProjectCycle
                   (探索/扩张/成熟,含配比 mix())+ HandoffRecord + 度量派生链
  bw-engine/    ✅ Executor 契约 + MockExecutor(可配延迟,驱动实时进度流)
  bw-store/     ✅ SQLite 持久化;project 加 cycle/active_stage;op_stage 加 dod
                   (JSON bool[]);新增 handoff 审计表(append-only,唯一信号=
                   active_stage 的唯一写入源)
  bw-app/       ✅ Command/Event 总线;创建流命令(CreateProject/SetCycle/
                   UpdateBrief/UpdateNorthStar/UpsertManualMetric/
                   CompleteCreation{cadence});运营命令(RecordObservation/
                   UpdateWeekPlan/SetStageProgress/ToggleDod/HandoffStage)
  ui/           ✅ 纯函数 selector + ViewModel 层(ui::vm:项目卡/阶段轴/MetricVm/
                   周计划/观测 feed/StageDetailVm,全部单测)
  app-desktop/  ✅ 真壳【Dioxus 0.7 desktop】:kernel 桥(独立 tokio 线程) +
                   项目墙 / 创建卡片流(意图→快问→起草→审阅)/ 运营视图
                   (5 阶段轴 · 阶段详情卡 · 工作流 · 定时任务)
  app-web/      —  非成员,"以后也许" 留口(Tier E)
```

`default-members` 只含无头内核 + ui,故日常 `cargo test` / `cargo check` **不编译 Dioxus**。桌面壳需显式 `-p app-desktop`。

## 常用命令

```bash
cargo test                       # 内核 + selector + vm + compile-fail doctests
cargo test -p bw-app             # 脊椎(spine)+ 监控回路(monitor,含交棒/回流)集成测试
cargo run -p app-desktop         # 启动桌面应用(BW_DB=path 可覆盖数据库位置)

# CI 同款门禁(本地可跑):
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
```

## 两条不可妥协 —— 已钉进类型(不变)

1. **UI 无关内核**:内核 crate 禁依赖 dioxus/tauri/wry/leptos(CI guard);wasm32 check 保活 Web 留口。
2. **健康永远 derive**:`Signal` 只能经封口的 `Derived<Signal>` 进入缓存;store 无 `set_signal`;`recompute_signals` 是唯一信号写入者。派生链逻辑不变(worst-of),只是现在跑在 5 个阶段上。

## 五阶段 = 角色 = 方法论(体系重构 v2)

旧 7 控制点(竞品洞察/需求导入/北极星/引领/滞后/原型创建/进度管理)被替换为 5 个**持续运营的阶段**,每段一个主持角色、一套方法论,首尾相接成环:

| # | 阶段 | 角色 | 方法论 | 求什么 | 循环节奏 |
|---|---|---|---|---|---|
| 1 | 原型 | 原型师 | 假设驱动探索 | 求真 | 小时级 · 48h 一圈 |
| 2 | 构建 | 构建师 | 规格驱动交付 | 求成 | 天级 · Spec→合入 |
| 3 | 优化 | 优化师 | 度量驱动打磨 | 求简 | 天—周级 · 基线→回归 |
| 4 | 运营推广 | 运营推广师 | 增长实验 | 求增 | 周级 · 实验批次 |
| 5 | 运维 | 运维师 | 可靠性工程 SRE | 求稳 | 持续 · 无终点 |

北极星/引领/滞后指标仍是纵贯全程的**同一门度量语言**(项目级字段不变),换的只是每段的打法。每个阶段的核心问题/方法循环/DoD 交棒清单/AI 编队/反模式全部是 `StageKind` 的**静态方法论元数据**(`crates/bw-core/src/model.rs`),不是项目自定义文本 —— 这是通用方法论,不是逐项目编造的内容。

**交棒(取代关口)**:`Command::HandoffStage{risky, note}` 把 `active_stage` 推进到 `.next()`;DoD 未勾满**不会静默拦截**,只会记 `risky:true` 留痕(`handoff` 审计表,append-only)。`运维 → 原型` 是特殊的一跳:复盘产出的洞察回流原型段,线闭成环。

## 创建流程(取代 8 步表单向导)

原型里连项目名都是硬编码演示数据;新流程收集的每一步都是真实输入,单页对话式卡片依次展开:

| 卡片 | 收集 | 落库命令 |
|---|---|---|
| 1 意图 | 项目名/类型 + 一句话到多句的自由 brief | `CreateProject`(项目在此刻就建立,而非等到最后确认——中断可续) |
| 2 快速问题 | 周期(探索/扩张/成熟,chip)+ 对标(自由文本)+ 三月成功标准(自由文本)+ 复盘节奏(chip) | `SetCycle` + `UpdateBrief`;节奏留在本地,随 `CompleteCreation` 一起落库 |
| 3 起草中 | 一次真实(mock)工作流运行,阶段=`周期判定/北极星起草/指标框架/阶段激活`,复用运营视图同一条 `Engine`/`Executor` 通道,进度实时流 | `StartSession` + `RunWorkflow` |
| 4 审阅确认 | 可编辑北极星候选(候选内容全部来自用户真实输入的排列组合,绝不编造具体数字)+ 引领/滞后指标表单(留空由用户自己填,不预填假数据)+ 5 阶段 chip + 周期配比预览 | `UpdateNorthStar` + `UpsertManualMetric`×n + `CompleteCreation{cadence}` |

`UpsertManualMetric` 带幂等守卫:重复提交同一行不会刷重复观测,只有**变化的值**才是新事实。

## 监控运行流(`view=app`)

- **监控回路**:指标卡「记录本周值」→ `RecordObservation`(append-only)→ `recompute_signals` → 信号翻转沿 L2→L4→L6 上卷,项目墙圆点同步变色。sparkline/feed 全部来自真实观测史(`list_observations`),一个观测=一个点,绝不插值。
- **运行回路**:阶段「▶ 运行」→ `StartSession` + `RunWorkflow`(每阶段内置标准工作流模板,直接取自该阶段的**方法循环**,方法论内容非模拟数据)→ MockExecutor(450ms/阶段)→ 进度事件**实时**流向 UI 横幅 → 产出落为会话消息,chat 可继续对话(mock 回复;真执行器=同事团队经 `Executor` trait 热插拔)。
- **阶段详情卡**(`StageDetailCard`):核心问题 / 方法循环 / 默认视图+引领焦点 / AI 编队 / 反模式(全部静态方法论文本)+ 真实 DoD 勾选状态 + 交棒按钮(仅当前活跃阶段可交棒)+ 已交棒次数(来自审计日志计数)。
- **周期性诚实**:`Boot` 与 `OpenProject` 都对运营中项目重算信号 —— 过期观测按节奏窗口封顶 Amber,绝不把陈旧缓存当新鲜真相。
- 阶段进度是**计划数据**(非信号),`SetStageProgress` 手动维护并累计真实趋势史。

## 出口闸门

- [`spine.rs`](crates/bw-app/tests/spine.rs):建项目→创建流→workflow→交棒→落库→杀进程重开,持久化信号/active_stage/交棒审计==独立重算。
- [`monitor.rs`](crates/bw-app/tests/monitor.rs):RecordObservation 翻转信号且上卷到项目级;UpdateWeekPlan 移动目标后重派生;运行进度事件先于持久化消息(证实时流);DoD 勾选 + 带险交棒 + 走完一圈回流原型(证线闭成环);Boot 重算。
- `bw-core` 单测:5 阶段 index/label/next()(含 Ops→Prototype 回流)、每阶段 3 条 DoD/3 个 AI 编队、周期配比求和=100。
- `ui::vm` 单测:趋势=观测史、无数据=Unknown 不冒充绿、feed 最新条回声当前信号、StageDetailVm 勾选态与 dod_all_checked。

## 已知待办(P3 及以后)

- 产物 / 版本面板、工作流库、9 个 Hub 全屏、右栏目录树 —— P3 铺屏(占位已注明,不放模拟数据)。
- CJK 字体 `asset!()` 本地 bundle(当前用系统字族回退:Songti/PingFang 等)—— P3 保真税。
- Connector/Cron 真喂指标 —— Tier D;签名/打包 —— Tier B。
- 「洞察链路重构」(界定→采集→结构化→分析→洞察 五节点证据链 + 工作流执行 GATE 暂停/恢复)是另一条独立探索,与本次 5 阶段迁移方向不同,未纳入本次改动 —— 若要采纳,需要先设计 `Engine::run_workflow` 的暂停/恢复语义。
