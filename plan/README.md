# plan/ 目录导读(先读这篇,30 秒知道哪些还作数)

> 本目录只留**现在还作数**的计划与规范(7 篇)。历史批次的计划——早期路线选型(00-05)与做完即历史的执行记录(09-12、14、17-19、21)——2026-08-17 起统一归档到 [`../docs/archive/plan/`](../docs/archive/plan/),编号语义不变(源码注释与文档里的 `plan/NN §M` 锚点按号去那里找)。**编号是时间序,不是重要度**。
>
> 看不懂的词查 [`../CONTEXT.md`](../CONTEXT.md) 词表;看不懂的代号(P2、W6、R1……)查 [`../docs/code-schemes.md`](../docs/code-schemes.md)。**当前迭代在做什么**(V1 产品化 → V2 调度/多人 → V3 内嵌 Open Design)不在本目录,在 [`../docs/v1-prototype/`](../docs/v1-prototype/) → [`../docs/v2-prototype/`](../docs/v2-prototype/) → [`../docs/v3-prototype/`](../docs/v3-prototype/);全仓文档地图见 [`../docs/README.md`](../docs/README.md)。

## 现在还作数的

| 文件 | 是什么 | 怎么用 |
|---|---|---|
| [06-overall-alignment.md](06-overall-alignment.md) | **设计唯一事实源**。含缺口台账(G 系列)与执行队列;末尾持续追加「转向(用户拍板)」记录 | 设计层面拿不准就查它;新决定以最晚一条「转向」为准 |
| [07-product-proposition.md](07-product-proposition.md) | **产品命题**:原型引子页原文 + 用户语言拆解 + 工程对照表 | 全目录的写作范本;命题正文只用人话 |
| [08-mvp-execution-plan.md](08-mvp-execution-plan.md) | MVP 的**定义**(项目的生命周期 × workflow 的生命周期)与当时的执行队列 | §1 定义仍作数;**执行队列已被 `docs/v1~v3-prototype/` 接管**(顶部横幅写明),别再照它排活 |
| [13-github-mainline-creation-flow.md](13-github-mainline-creation-flow.md) | GitHub 为正本的创建流(D1-D12 十二条拍板) | 决定仍生效:issue = GitHub issue、验收 = merge、`.bw/metrics.toml` 正本 |
| [15-acceptance-flow-workflow.md](15-acceptance-flow-workflow.md) | 验收流(考卷/真点击/证据报告)与 `scripts/*flow*` 工具链的权威说明 | 用 `scripts/run-flow.py` 之前读它;注意 ▶跑 已改走内嵌终端,`e2e/flows/core/02` 那张考卷可能过时(见 `docs/LEFTOVERS.md` 减负-8) |
| [16-skill-standard-spec.md](16-skill-standard-spec.md) | 技能规范(S1-S7 硬规) | **规范长期有效**;`audit_skills` 例子按它巡检 |
| [20-asset-scope-isolation.md](20-asset-scope-isolation.md) | 资产按项目归属的三层隔离(R1-R5 规则) | **规则活跃**;`scoped_pick` 是唯一按名解析器 |

## 已归档到 `docs/archive/plan/` 的(去那里读,各文件顶部有横幅)

| 文件 | 一句话 | 还有效的部分 |
|---|---|---|
| 00-PLAN | 最初的总路线图(七控制点/双团队时期) | §6 设计系统 token(暖纸底/clay 主色/三态信号色)仍被 CLAUDE.md 引用 |
| 01-prototype-inventory | HTML 原型逐项转录 | 原型仍是产品命题出处 |
| 02-rust-stack-evaluation | UI 技术选型评估 | **结论至今成立**(Dioxus 0.7 pin =0.7.9) |
| 03-architecture-and-engine | 架构与引擎设计 | §2.5 L0-L6 派生链定义**至今是权威定义** |
| 04-effort-and-mvp | 工作量估算与 MVP 切线 | 无(前提已被推翻) |
| 05-complete-form-design | 完整形态设计(multica 融合分支) | G 系列缺口台账由 06 接管 |
| 09-aihot-practice-run | 用真实项目「aihot 日报」从零践行 | cron 设计已被 11 §L5 推翻 |
| 10-personal-kanban-and-real-run | 个人看板 + 真执行首跑(K0-K4) | 已完工 |
| 11-boards-process-cards-and-real-aihot-loop | 看板/流程卡 + aihot 双环 | 自我纠错范本 |
| 12-skill-agent-workflow-cron-truthful-modeling | 技能/队友/工作流/定时任务的真实建模(T1-T17) | 已全部交付 |
| 14-creation-experience | 创建体验批(C12-C16) | 已验收 |
| 17-run-scheduling-redesign | 运行调度重设计(串行锁/worktree 隔离) | 已落地;项目代号自此从 aihot 换为 buddy |
| 18-step3-metric-loop-tie-up | 指标环收尾 | 已落地 |
| 19-metric-skills-evaluation | 业界找指标技能盲测与引入 | 已执行;方法论沉淀进 `docs/skills/north-star-discovery/` |
| 21-metric-render-skill-evaluation | 业界指标渲染技能选型测评 | 自陈仅第一轮,如实标未测 |

## 相关目录

- [`../docs/README.md`](../docs/README.md):全仓文档地图(现役 / 运行时资产 / 伙伴迭代线 / 归档)。
- `../iterations/`:交接记录已归档到 `../docs/archive/iterations/`;[`../iterations/PRACTICE-buddy.md`](../iterations/PRACTICE-buddy.md) 是唯一持续更新的实践日志。还没干的活只认 [`../docs/LEFTOVERS.md`](../docs/LEFTOVERS.md)。文档边界见 [`../docs/doc-boundaries.md`](../docs/doc-boundaries.md);版本出包见 [`../docs/releases.md`](../docs/releases.md)。
- [`../DEVELOPMENT.md`](../DEVELOPMENT.md):开发指南;[`../CLAUDE.md`](../CLAUDE.md):给 AI 的工作说明,含写作纪律。
