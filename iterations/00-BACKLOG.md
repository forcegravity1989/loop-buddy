# 25-Iteration Backlog — 用五角色五阶段环自举 WorkflowHub

> **北极星**:WorkflowHub 既能创建 static workflow,也能通过 schedule 不断执行 workflow、度量效果、优化 workflow 本身,使其贴近用户习惯与场景。
>
> **方法论**:每一轮迭代 = 一次完整五角色环(原型师→构建师→优化师→运营推广师→运维师),Ops 回流 Prototype 闭环。
>
> **dogfooding**:WorkflowHub 的构建过程本身被这套方法论管理 —— 每轮的证据是真实代码 + 真实 git log + 真实可跑的 dogfood。

---

## Arc 1 · 数据基座(iters 1–5):为"优化"先种下它要吃的粮食

| # | 主题 | 缺口(为什么要做) | 出口(DoD) |
|---|---|---|---|
| 1 | 运行结果追踪 | 现在只记 run 发生过,不记成败/耗时 → 无法判断"该不该优化" | `run_outcome` 落 observation,workflow run 记 success/fail/duration |
| 2 | 工作流运行分析 | 没有 per-workflow 聚合统计 | `workflow_analytics()` 汇总 uses/成败率/均耗时 |
| 3 | 参数捕获 | 不知道用户每次用什么参数 → 无法贴近习惯 | 每次运行记录 param 快照 |
| 4 | 调度有效性度量 | 定时任务跑了,但不知"跑完是否改善目标" | 定时运行回链 outcome + effectiveness 分 |
| 5 | 版本快照 | `UpdateWorkflowSpec` 覆盖旧版,无法回看演化 | 优化前存历史版本,diff 可见 |

## Arc 2 · 优化智能(iters 6–12):让数据长出判断

| # | 主题 | 缺口 | 出口(DoD) |
|---|---|---|---|
| 6 | 使用频率分析 | 冷热工作流不分 | 冷热榜 + 低使用预警 |
| 7 | 失败模式检测 | 失败一团散沙 | 按失败原因聚类 + top-N |
| 8 | 参数频率分析 | 习惯是隐性的 | 主导参数提取 + 置信度 |
| 9 | 优化建议生成 | 系统不会自己说话 | `OptimizationProposal` 产出可读建议 |
| 10 | 节奏自调建议 | cadence 写死 | 基于有效性的 cadence 建议 |
| 11 | 工作流健康信号 | 工作流自己没有 Green/Amber/Red | 复用 derive 链给 workflow 派生信号 |
| 12 | 习惯画像 | per-workflow 使用签名缺失 | `HabitProfile` 结构化习惯摘要 |

## Arc 3 · 自改进闭环(iters 13–20):度量→建议→应用(度量门控)

| # | 主题 | 缺口 | 出口(DoD) |
|---|---|---|---|
| 13 | 建议评审/应用管线 | 建议不会被应用 | proposal → 度量门控 → 应用/驳回 |
| 14 | 版本 A/B 对比 | 不知道优化有没有真变好 | 新旧版本指标并排 |
| 15 | 场景聚类 | 运行场景不可分 | 按 param pattern 聚类运行 |
| 16 | 跨阶段复用智能 | 标准模板复用情况不可见 | 跨阶段复用率 + 推荐 |
| 17 | 推荐引擎 | 用户不知下一步跑什么 | next-workflow 推荐 |
| 18 | 闭环定时优化运行 | 优化不是定时自驱的 | 定时任务驱动"度量→建议→应用" |
| 19 | 习惯驱动默认参数 | 默认参数是写死的 | 从画像推断默认 |
| 20 | 优化成效报告 | 看不到"优化了多少" | per-workflow 成效 delta |

## Arc 4 · 演示与 PM 模板(iters 21–25)

| # | 主题 | 缺口 | 出口(DoD) |
|---|---|---|---|
| 21 | 真实场景仿真器 | 没有可重复的"用户习惯"输入 | 场景仿真器产出确定性运行流 |
| 22 | 跑通自优化闭环 | 端到端没演示过 | 仿真器 + 闭环在 dogfood 里真跑 |
| 23 | PM 模板抽取 | workflowhub 自身过程未抽象成模板 | 可复用 PM 模板(5角色×5阶段) |
| 24 | 25 轮旅程可视化 | 过程不可见 | 旅程时间线 + 指标演化 |
| 25 | HTML 报告 + 演示 | 最终交付件 | 25 轮总结 + 最终形态 + PM 模板 HTML |

---

## 节奏与诚实约束

- **每轮必过五角色**:原型师(求真·假设/DoD)→ 构建师(求成·实现)→ 优化师(求简·度量/重构)→ 运营推广师(求增·场景/价值)→ 运维师(求稳·持久/幂等/闭环回流)。
- **绝不编造**:每轮的证据 = 真实可跑的代码/测试 + 真实 git log + dogfood 真实读回。Growth/Ops 不越过真实证据推进。
- **migration 安全**:`CREATE TABLE IF NOT EXISTS` 不加列 —— 用 `add_column_if_missing` 守卫(历史的真实事故教训)。
- **门禁每轮跑**:`cargo fmt --check && cargo clippy --workspace --exclude app-desktop -- -D warnings && cargo test`。
