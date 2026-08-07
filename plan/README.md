# plan/ 目录导读(先读这篇,30 秒知道哪些还作数)

> 本目录是按时间累积的计划文档:早期的路线选型(00-05)→ 对齐后的设计与执行计划(06-08,**当前活跃**)→ 一批批的执行记录(09-21,做完一批留一篇)。**编号是时间序,不是重要度**——直接从 00 读起会先撞上已被取代的旧设计。
>
> 看不懂的词查 [`../CONTEXT.md`](../CONTEXT.md) 词表;看不懂的代号(P2、W6、R1……)查 [`../docs/code-schemes.md`](../docs/code-schemes.md)。

## vNext 现行方向

| 文件 | 是什么 |
|---|---|
| [22-vnext-opc-control-plane-rebuild.md](22-vnext-opc-control-plane-rebuild.md) | **vNext 单一计划事实源**：把 Bench 重建为以 Rust 为主体、通过 Connector 组合成熟组件的 OPC 项目控制面。新实现先读这一篇。 |

## 旧应用仍作数的事实源

以下三篇继续解释当前旧应用的行为和历史约束，但不再约束 vNext 的产品中心与技术骨架。

| 文件 | 是什么 |
|---|---|
| [06-overall-alignment.md](06-overall-alignment.md) | **设计唯一事实源**。含缺口台账(G 系列)与执行队列;末尾持续追加「转向(用户拍板)」记录,新决定以最晚一条为准 |
| [07-product-proposition.md](07-product-proposition.md) | **产品命题**:原型引子页原文 + 用户语言拆解 + 工程对照表。命题正文只用人话——这是全目录的写作范本 |
| [08-mvp-execution-plan.md](08-mvp-execution-plan.md) | **MVP 执行计划,当前接手工作的入口**。开头「进度实况」表持续更新,先看它再看正文 |

## 路线与选型背景(00-05,已是历史,各文件顶部有横幅)

| 文件 | 一句话 | 还有效的部分 |
|---|---|---|
| [00-PLAN.md](00-PLAN.md) | 最初的总路线图(七控制点/双团队时期) | §6 设计系统 token 仍被 CLAUDE.md 引用 |
| [01-prototype-inventory.md](01-prototype-inventory.md) | HTML 原型逐项转录(七步向导时期) | 原型仍是产品命题出处 |
| [02-rust-stack-evaluation.md](02-rust-stack-evaluation.md) | UI 技术选型评估 | **结论至今成立**(Dioxus 0.7 pin =0.7.9) |
| [03-architecture-and-engine.md](03-architecture-and-engine.md) | 架构与引擎设计 | §2.5 L0-L6 派生链定义**至今是权威定义** |
| [04-effort-and-mvp.md](04-effort-and-mvp.md) | 工作量估算与 MVP 切线 | 无(前提已被推翻,仅决策背景) |
| [05-complete-form-design.md](05-complete-form-design.md) | 完整形态设计(multica 融合分支) | G 系列缺口台账由 06 接管;文首谱系注写明 |

## 执行批次记录(09-21,一批一篇;做完即历史,但含当批的权威定义)

| 文件 | 一句话(当批做了什么) | 特别说明 |
|---|---|---|
| [09-aihot-practice-run.md](09-aihot-practice-run.md) | 用真实项目「aihot 日报」从零践行 | 其 cron 设计已被 11 的 §L5 明确推翻 |
| [10-personal-kanban-and-real-run.md](10-personal-kanban-and-real-run.md) | 个人看板 + 真执行首跑(K0-K4) | 已完工 |
| [11-boards-process-cards-and-real-aihot-loop.md](11-boards-process-cards-and-real-aihot-loop.md) | 看板/流程卡 + aihot 双环 | 自我纠错范本:§L5 主动推翻 09 |
| [12-skill-agent-workflow-cron-truthful-modeling.md](12-skill-agent-workflow-cron-truthful-modeling.md) | 技能/队友/工作流/定时任务的真实建模(T1-T17) | 已全部交付 |
| [13-github-mainline-creation-flow.md](13-github-mainline-creation-flow.md) | GitHub 为正本的创建流(D1-D12 十二条拍板) | 决定仍生效 |
| [14-creation-experience.md](14-creation-experience.md) | 创建体验批(C12-C16) | 已验收 |
| [15-acceptance-flow-workflow.md](15-acceptance-flow-workflow.md) | 验收流(考卷/真点击/证据报告) | 结论已吸收进 CLAUDE.md 与词表 |
| [16-skill-standard-spec.md](16-skill-standard-spec.md) | 技能规范(S1-S7 硬规) | **规范长期有效** |
| [17-run-scheduling-redesign.md](17-run-scheduling-redesign.md) | 运行调度重设计(串行锁/worktree 隔离) | 已落地;项目代号自此从 aihot 换为 buddy |
| [18-step3-metric-loop-tie-up.md](18-step3-metric-loop-tie-up.md) | 指标环收尾(找指标技能修正 + L6 聚合缝) | 已落地 |
| [19-metric-skills-evaluation.md](19-metric-skills-evaluation.md) | 业界找指标技能盲测与引入 | 已执行;§8 更新了 §0 的结论 |
| [20-asset-scope-isolation.md](20-asset-scope-isolation.md) | 资产按项目归属的三层隔离(R1-R5 规则) | **规则活跃**;落地了 plan/08 的 S1 |
| [21-metric-render-skill-evaluation.md](21-metric-render-skill-evaluation.md) | 业界指标渲染技能选型测评 | 自陈仅第一轮(搜索+内容核验),同模盲测未跑,如实标未测 |

## 相关目录

- `../iterations/`:交接记录与实践日志,全部是历史档案(顶部有横幅),唯一持续更新的是 `PRACTICE-buddy.md`。
- `../DEVELOPMENT.md`:开发指南(命令与架构速览);`../CLAUDE.md`:给 AI 的工作说明,含写作纪律。
