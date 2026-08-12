# 任务代号索引(读到 X1 不知道是什么,查这里)

> **30 秒导读**:本仓库的计划文档习惯给每批任务编代号(P1、W6、R3……)。历史上没有登记表,导致 **P/S/W/R/L/A 六个字母各自被用了 2-4 次、含义完全不同**。本文是唯一的代号登记处:读文档撞到代号来这里查;要新开一批代号,先查这里、避开已用字母、登记后再用。各系列的**权威定义在其所属文件里**,本表只负责指路。

## 使用规则

1. **读**:代号的含义取决于它出现在哪份文件里。先看你在读哪份文件,再对照下表。
2. **写**:新开一批代号前,选一个下表没有的字母(或明确写「本文的 X 系列与 plan/NN 无关」),并在本表加一行。commit message 里用代号时,代号之外必须有人话描述(见 CLAUDE.md「写作纪律」)。

## 各系列一览(按字母)

| 代号系列 | 定义文件 | 含义 | 状态 |
|---|---|---|---|
| **A0–A5** | plan/06 §8 | 对齐后的执行队列条目(A5 在 HANDOFF-2026-07-16-A5 里再细分为 A5-A…A5-H 子项) | 已全部落地 |
| A1/A2/A4、B1/B2 | plan/18 | 「找指标」技能 prompt 的修改项(A=north-star-discovery 的改动,B=metrics-binding 的改动) | 已落地 |
| **Bat A–D** | plan/08 | MVP 执行计划的四个批次(Batch) | 执行中 |
| **C12–C16** | plan/14 | 创建体验批的五件事。C1–C11 **不存在于任何文档**——编号起点延续自当时会话内部的票号 | 已全部验收 |
| **D1–D12** | plan/13 | 创建流盘问(grilling)拍板的十二条决定 | 决定生效中 |
| **G1–G11** | plan/05 首定,plan/06 承接 | 缺口台账:complete-form 设计时清点的十一个缺口 | 多数已闭合,状态见 plan/06 |
| **K0–K4** | plan/10 | 个人看板批的五条工作流 | 已全部落地 |
| **L0–L6** | plan/03 §2.5(**权威定义**) | 度量派生链的七层:L0 观测 → … → L6 项目聚合信号 | 长期有效(代码结构) |
| L1/L2/L3 | plan/18、plan/19 | **另一含义**:具体项目的引领/滞后指标标签(如 L1=报告覆盖率)。与 L0–L6 派生链**同文件混用**,读时注意 | 项目数据,随项目走 |
| **M1/M2** | plan/00 | 早期路线图的两个里程碑(M1 走通脊椎 / M2 保真 MVP) | 历史(plan/00 已归档) |
| **M4**(原 M3) | plan/06 §8 定义时叫 M3,plan/08 起统一叫 **M4** | 「用户一天」端到端验收 | 活跃 |
| **P0–P3** | plan/00(DEVELOPMENT.md 沿用) | 早期路线图的架构里程碑(基座/脊椎/纵切 UI/铺屏) | 历史 |
| **P1–P7** | iterations/HANDOFF-2026-07-15 | 当时提案的七件后续任务(P1=Autopilot 最小化、P2=真实执行环……) | 历史,多数已被后续计划吸收 |
| **P1–P5** | plan/08(**当前活跃的 P 系列**) | MVP「项目的生命周期」线五件事(P1 建项目即建仓、P2 项目标配……) | 活跃 |
| P0/P1 | plan/19 | **另一含义**:技能引入路线的优先级(P0 直接引入 / P1 合入增强) | 已执行 |
| **R1–R4/R5** | plan/06 | multica 融合任务(R1 Issue 层、R2 Skill 复利、R4 绝不重复记账……) | 已落地 |
| **R1–R7** | docs/adr/0001 | 术语改名台账(R1 ProjectCycle→时期……)。**与 plan/06 的 R 系列无关** | 部分完成 |
| **R1–R5** | plan/20 §4 | 资产作用域规则(R1 池只见自有……)。**与上两套 R 系列无关** | 活跃 |
| **S1** | plan/08 | 「归属反转」这一件事(资产按项目归属),2026-08-05 由 plan/20 落地 | 已落地 |
| **S1–S7** | plan/16 | 技能规范的硬性条款(S1 名称格式正则、S2 重名硬拒……) | 长期有效(规范) |
| **S1–S5** | plan/17 | 运行调度重设计的五个实现步骤(S1 串行锁、S2 worktree 隔离……) | 已落地 |
| **S1/S2** | plan/19 | 技能盲测的两个埋陷阱场景(S1 DailyBrief / S2 TraceLens) | 已执行 |
| **T1–T17** | plan/12 | truthful-modeling 批的十七张票(T14–T17 为真实感落地批;T11 在 plan/13/16/19 被引用时指同一张票) | 已全部交付 |
| **W1–W3** | plan/08 | MVP「workflow 的生命周期」线三件事 | 活跃 |
| **W1–W6** | plan/20 §5 | 资产作用域隔离批的六件落地工作(commit 前缀 plan20-W2 等即此)。**与 plan/08 的 W 系列无关** | 已落地 |
| **V1-TermRefactor1–5** | `docs/v1-prototype/issue2-terminal-conversation-refactor.md` §10 | 终端会话重构:1 数据模型 / 2 底座(PTY+路由+xterm+尺寸) / 3 并发切卡 / 4 重启恢复 / 5 咨询态。接续窗口按 §10.1 产品体感切分(非原工程五段) | 1–5 已落地 |
| **V1-TermClose1–3** | `docs/v1-prototype/issue2-all-issues-terminal-runs.md` | 终端会话重构收口:1 路由+prompt(所有 issue ▶跑 走终端、issue 内容作位置 prompt、蒸馏/目录并入系统提示词) / 2 删老路径+UI 门控 / 3 examples+文档 | 1–3 已落地 |
| **V1-TermDemote** | `docs/v1-prototype/issue2-terminal-conversation-refactor.md` §13 | Bug1:合入/Done 后 active_run 不释放 → 交付降级为咨询(放锁、不杀 PTY、不清 worktree) | 已落地 |
| **V1-TermFocus** | `docs/v1-prototype/issue2-terminal-conversation-refactor.md` §13 | Bug2:左侧 session 卡 ↔ 嵌终端焦点双向同步 | 已落地 |
| **V2-②-B** | `docs/v2-prototype/same-project-multiple-workbenches.md` §8 | Phase B:采集补齐近 30 天 observation(`history` series)+ Buddy `collect_stats.py`;与 Issue 3 折线配对;横轴为周结束日 MM-DD | 已落地 |
| **V2-②-A-IntentUX** | `docs/v2-prototype/same-project-multiple-workbenches.md` §6.2 | 后来者选仓后远端探测 project.toml → Intent 只读预填(方案 A);首到者仍手填 | 已落地 |
| **V2-②-I** | `docs/v2-prototype/same-project-multiple-workbenches.md` §6.2 / §3.3 | 仓平台 open Issue 单向读回重建本地行(可 ▶跑);创建收尾+手动同步;远端已关且本机未完成→Cancelled 不上板;本机 Done 保留可续聊;Done 永不自动 | 已落地 |

散见的一次性编号(如 plan/16 的 P8、plan/17 的 C5)不单独列行,以所在文件的上下文为准。

## 撞车警示(同字母、不同含义)

| 字母 | 有几套 | 分别在哪 |
|---|---|---|
| P | 4 | plan/00 里程碑 · HANDOFF-2026-07-15 任务提案 · plan/08 项目线(活跃) · plan/19 优先级 |
| S | 4 | plan/08 归属反转 · plan/16 规范条款 · plan/17 实现步骤 · plan/19 测试场景 |
| R | 3 | plan/06 融合任务 · docs/adr/0001 改名台账 · plan/20 作用域规则 |
| W | 2 | plan/08 workflow 线(活跃) · plan/20 落地工作项 |
| L | 2 | plan/03 派生链层级(代码结构) · plan/18/19 指标标签(项目数据) |
| A | 2 | plan/06 执行队列 · plan/18 技能修改项 |

这些撞车已成历史事实,**不回头改旧文档的编号**(改了反而对不上 commit 历史);靠本表消歧,靠「新开代号先登记」防止再犯。
