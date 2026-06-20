---
name: create-loop
description: 把一句大白话目标变成一条可运行的 Loop(= Query + 固定 Workflow 模板 + Agent Team + Goal),对齐 Claude Workflow 工具、动态执行。当 Builder 想从已接入的 skills/agents 创建一条自主 workflow 时用。产出:一份 Loop 规格 + 一个能被 Workflow 工具直接运行的动态脚本,并标好两道闸(验收认证 / 副作用审批)与类型(执行器 / 监测器)。
---

# create-loop · 把目标拼成一条 Loop

这是「创建 workflow」这个**建立过程本身**,做成一个 skill。给一句目标,它按既有架构把一条 **Loop** 拼出来,并输出成对齐 Claude Workflow 工具的**动态脚本**。不是再画一张 UI,是产出能跑的东西。

## Loop 的固定架构(不可变)

> **Loop = 清晰的 Query/Prompt + 固定的 Workflow 模板 + 一个 Agent Team + 清晰的 Goal**

| 部件 | 含义 | 落到产物 |
|---|---|---|
| **Query** | 把大白话目标翻成给 Loop 的清晰 prompt | 脚本里的 `LOOP.query` |
| **固定 Workflow 模板** | 阶段是固定骨架,不是每次重画的自由画布 | `templates/loop.workflow.js` 的阶段循环 |
| **Agent Team** | 给每个阶段配 agent(角色 + 模型档) | `LOOP.stages[].{name,prompt,model}` |
| **Goal** | 清晰、可观测、抗漂移的达成标准;**它同时就是验收闸的判据** | `LOOP.acceptance`,由验收 agent 逐轮判定 |

## 建立过程(7 步)

1. **澄清 Query + Goal**:把目标写成清晰 prompt;把「怎样算做对了」写成可观测、抗漂移的验收标准(Goal)。Goal 含糊 → 不开工。
2. **选能力**:从已接入家底(skills/plugins)里挑这条 Loop 用得上的,标明「复用了哪个」,不重复造。
3. **配 Agent Team**:给固定 workflow 的每个阶段派 agent + 模型档(轻活 Haiku、常规 Sonnet、难活 Opus)。
4. **套固定 workflow 模板**:用 `templates/loop.workflow.js`,不自由发挥阶段结构。
5. **判类型**:
   - **执行器(闭合)**:跑到 Goal 验收自停 → loop-until-goal。
   - **监测器(开放)**:常驻不终止,产出「投喂新 Goal + 浮出决策」→ 用周期触发,不求终止。
6. **写验收标准 + 主动唱反调**:列出你担心会漂移的点 + 反例(抗橡皮图章)。
7. **标副作用**:识别对外 / 花钱 / 不可逆动作,标红进副作用闸。

## 两道闸(写进 Loop 契约,不可省)

- **验收闸**:Goal 的达成标准必须由人认证「**充分 + 客观 + 抗漂移**」后,Loop 才能自主跑。命中已认证模式 → 飞轮自动放行;新模式 → 浮出待认证。
- **副作用闸**:对外 / 花钱 / 不可逆动作运行时 → 人批 + kill switch + 日志。花钱/不可逆即便命中飞轮仍二次确认。

机器查不了「验收够不够充分」(领域特定),所以这两道闸是人的稀缺判断的落点,**不能内联进脚本自动通过** —— 它们由工作台在脚本外围强制。

## 执行器 vs 监测器

| | 执行器 | 监测器 |
|---|---|---|
| 终止 | 跑到 Goal 验收**自停** | **不求终止**,常驻 |
| 模板 | `loop.workflow.js`(loop-until-goal) | 同模板单轮 + `ScheduleWakeup`/cron 周期触发 |
| 产出 | 完成的交付物 | 投喂新 Goal 给执行器 + 向人浮出决策 |
| 进待办 | 失败/卡住/待认证才进 | 不计入待办;只在「投喂/浮出」时出声 |

## 输出格式(本 skill 的产物)

1. **Loop 规格**(YAML/JSON):`{ query, goal, acceptance, stages[], team[], kind, sideEffects[], gates }`(见 `examples/`)。
2. **动态 workflow 脚本**:把规格作为 `args` 喂给 `templates/loop.workflow.js`,即可被 Claude Workflow 工具运行(`/workflows` 看进度)。
3. **标注**:类型(执行器/监测器)+ 两道闸的判据。

## 对齐 Claude Workflow 工具

产物用 Workflow 工具的原语:`export const meta`(纯字面量)+ `agent()` / `pipeline()` / `phase()` / `log()`,schema 强制结构化输出;**执行器用 loop-until-goal**(while 未达成则迭代,达成 return)。监测器用 `ScheduleWakeup` 做动态步进(开放、不终止)。脚本里不可用 `Date.now()`/`Math.random()`(会破坏 resume)。

## 怎么用这个 skill

给我目标(+ 可选:指定要复用的能力)。我会走完 7 步,先把 Goal/验收标准摆出来让你认证(验收闸),再输出 Loop 规格 + 可运行脚本。低风险(只读/可逆/不花钱)可跳过强制认证直接给可跑脚本;含副作用的,先认证 + 建议 dry-run。
