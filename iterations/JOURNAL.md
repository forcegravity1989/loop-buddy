# 25-Iteration Journal — 五角色五阶段环自举 WorkflowHub

> 每轮 = 一次完整五角色环。证据 = 真实代码 + 真实 git log + 真实可跑测试。
> 门禁每轮跑:`fmt --check && clippy -D warnings && cargo test`。

---

## Iter 01 · 运行结果追踪(数据基座 1/5)

**环位**:Prototype → Build → Optimize → Growth → Ops → (回流 Prototype)

- **原型师 · 求真(假设驱动探索)**:现状只记 run 发生过(`uses` 计数 + session 消息),不记成败/耗时/触发源。**假设**:没有"这次跑了多久、成了没"的颗粒度,任何"该不该优化"的判断都是瞎猜。**DoD**:每次运行(manual + 定时)落一条 append-only 记录,带 status/duration/phases/trigger;失败也如实记。
- **构建师 · 求成(规格驱动交付)**:新增 `WorkflowRunId` + `RunStatus{Running,Ok,Failed}` + `RunTrigger{Manual,Scheduled}` + `WorkflowRun` 结构;schema 加 `workflow_run` 表(append-only,start 入一条,engine 返回时 settle 一次);Store 加 `record_workflow_run_start`/`settle_workflow_run`/`list_workflow_runs`/`list_all_workflow_runs`;`run_workflow_inner` 加 `trigger` 形参,用 `Instant` 计真实耗时,3 个调用点(RunWorkflow/RunHubWorkflow/tick_scheduler)分别传 Manual/Manual/Scheduled。
- **优化师 · 求简(度量驱动打磨)**:settle 幂等(已终态的行不再被改写——dogfood 重跑不污染历史);`phases_completed` 在 move 前捕获;失败路径也走 settle(不全跑就崩也能记)。新增 4 个测试覆盖 start/settle 幂等 / 成功记 Ok+duration / 失败记 Failed+partial phases / 定时触发归因 Scheduled。
- **运营推广师 · 求增(增长实验)**:验证真实场景——用户跑一个工作流,历史里能看到"成功·2阶段·真毫秒";定时任务自动触发的运行被正确归因为 Scheduled(可与手动区分,后续分析能分渠道)。`workflow_name` 快照防 spec 改名后历史失真。
- **运维师 · 求稳(可靠性工程)**:新表 `CREATE TABLE IF NOT EXISTS` 对老库安全(不碰既有列,无需 add_column 守卫);`project_id`/`session_id` 可空(hub 运行不必绑项目);外键有意省略(run 可比 spec 长寿);**回流**:这批数据是 iter 2(运行分析)要聚合的粮食——环闭。

**门禁**:fmt clean · clippy clean · **91 tests pass / 0 fail**(基线 87 → +4)。
**提交**:见 git log `workflow_run` telemetry。

## Iter 02 · 工作流运行分析(数据基座 2/5)

- **原型师**:有了运行记录,但仍是行级原始数据,没有聚合视图。**假设**:优化决策要的是"这个工作流成功率 75%、典型耗时 250ms"这种一句话判断,不是逐行翻日志。**DoD**:`workflow_analytics(id)` 一调出 per-workflow 聚合。
- **构建师**:`WorkflowRunAnalytics` 结构(total/ok/failed/running 计数、success_rate、avg+median duration、last_run_at/status);单条 SQL 聚合 + Rust 算 median(中位数,抗单个慢离群点)。
- **优化师**:median 用中位数而非均值(一个慢 run 不污染"典型成本");`success_rate` 在无 settled 记录时为 `None` 而非 0("未知"≠"总失败",镜像 `Signal::Unknown`)。+1 测试覆盖 3ok/1fail → 0.75 + median 250。
- **运营推广师**:一句话能说清"这个工作流健康吗"——成功率 + 典型耗时 + 上次状态,是后续冷热榜/健康信号的直接输入。
- **运维师**:无 settled 时返回 total=0 而非报错(调用方能诚实显示"未运行");只读查询,无写入风险。**回流**:这聚合喂给 iter 6 冷热榜 + iter 11 健康信号。

**门禁**:fmt clean · clippy clean · **92 tests pass**(+1)。
