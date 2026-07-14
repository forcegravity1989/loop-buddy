# V2 Build Journal — 完整形态 Builders' Workbench(multica 优点 × BW 设计初衷)

> 本日志记录一次**全程托管**的构建:Fable 作为唯一调度 agent(=Builders' Workbench 本身),
> 用**五角色五阶段环**方法论,调度真实的 sonnet5 子 agent 队友,把 builders-workbench
> 从「Mock 驱动的项目管理壳」推进到「**真实 agent 队友 + 真实工作 + 真实度量**的完整工作台」。
>
> 一切实跑,绝不 mock。每条证据指向真实代码 / 真实测试 / 真实 commit。

---

## 0. 这次为什么不同于上一轮 25-iter

上一轮(glm-5.2 自举 WorkflowHub)的真实问题,不是「运行时」一个点(我起初武断下的结论,被用户纠正),
而是**整轮工作都跑在 MockExecutor 之上**:`simulate_hub.rs` 把**合成**的 `workflow_run` 行直接写进 store,
`run_optimization_cycle` 在这堆合成历史上优化 workflow 的 spec —— **没有任何真实 agent 做过任何真实工作,
系统在度量并优化「关于 mock 的 mock」**。`ClaudeCliExecutor` 写了但只靠一条 captured **auth-failure** 验证过,
从未真正跑成过一次。

本轮的纠正:**真实执行 = 真实 agent(我 + sonnet5 子 agent)做真实工程**(写真实 Rust、跑真实门禁),
BW 的 store 如实记账(真实 status / 真实 duration / 真实 owner),度量从**真实产物**派生。

## 1. 五角色五阶段环(本次怎么跑,方法论已验证)

每圈(每个需求 0→1)五个真实角色各一轮,门禁每圈跑:

```
原型师(求真)→ 构建师(求成)→ 优化师(求简)→ 运营推广师(求增)→ 运维师(求稳)→ 回流原型
```

- **原型师 · Fable(我)**:假设驱动探索。把现状与目标的差距压成一句话假设 + 可证伪 DoD。
- **构建师 · sonnet5 子 agent**:规格驱动交付。按既有架构模式落地真实代码 + 真实测试。
- **优化师 · sonnet5 子 agent**:度量驱动打磨。跑门禁(fmt/clippy/test),删冗余,找复用。
- **运营推广师 · Fable**:增长实验。验证它服务于谁、值多少,一句话价值。
- **运维师 · Fable**:可靠性工程。迁移守卫 / 幂等 / 重开还原;回流给下一圈。

> 角色映射会按需求微调,但**构建师与优化师由真实子 agent 执行**(multica 的「agent as teammate」落点)。

## 2. 三条不可妥协的诚实约束(贯穿全程)

1. **绝不编造**:每个数字 / 每条证据来自真实可跑的代码 / 测试 / git log。
2. **无数据 ≠ 绿**:未知就是未知,不准用绿色粉饰「没测过」(镜像 `Signal::Unknown`)。
3. **破坏性永不自动**:改内容 / 退役必须人工门控;自动循环只应用正向 / 安全项。

## 3. 关键技术事实(已核实)

- 基线 `cargo test`(default-members,headless)= **绿**(本会话已跑,exit 0)。
- `claude` CLI 在 `/Users/gravity/.local/bin/claude`,但经 **GLM 网关(open.bigmodel.cn)**,
  本会话探测得 **529(模型过载)**——auth 通了,但网关不稳。**演示不依赖 `claude -p`**;
  真实执行 = 子 agent 真实工程 + BW store 如实记账。
- BW 已有的真实基建(本轮继承,不重造):`workflow_run` append-only 遥测、真实 cron 调度器
  (`tick_scheduler`)、6 层度量派生链、`Signal{Green,Amber,Red,Unknown}`、handoff 审计、
  以及诚实自举脚本 `dogfood_workflowhub.rs`。

---

## 4. 设计综合(multica × BW)—— 待 multica 真实源研究完成后填写

> ⏳ 子 agent 正在抓取 multica 真实源码(数据模型 / 屏幕 / 操作流)。下面在结果回来后落定。

_(占位:完整的 IA 融合方案、Issue/Agent/Runtime/Autopilot/Skill/Squad 如何映射进 BW 的
五阶段环与项目生命周期、本轮真正要建的切片 vs 仅设计不建的边界。)_

## 5. 两个真实需求 0→1 —— 待设计锁定后填写

> ⏳ R1 / R2 的假设 + DoD。

## 6. 真实端到端演示 + 最终报告 —— 末尾填写
