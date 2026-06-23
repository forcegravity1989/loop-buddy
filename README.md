# Builders' Workbench

面向**资深独立开发者 / 一人公司(OPC)**的 AI 原生项目工作台。

> 一句话主线:说一句目标 → Claude 用你已有的 skills/agents 拼出一条 **workflow** → 你只认它的**合格线** → 开跑后它自治、拉起成百上千个 AI 实例去干,异常才浮到你面前。

**核心信念**:把你对「怎样算做对了」的一次判断,固化成可复用、能自动通过的**规则**——你写得越多,系统越安静。一次判断越用越省,这是手写脚本攒不出的资产。

**产品主张(开卷作业)**:Claude 的 **Workflow 已是成熟功能、有真实的 Tool 定义**(`agent()` / `pipeline()` / `phase()` / 实时 journal / loop-until-goal,见 [`claude-workflow-standard.md`](claude-workflow-standard.md))。本产品不另造抽象,而是把这个**真实原语可视化成低门槛、高交互的产品**——见「运行」面。灵魂三条:**低门槛 · 高可用 · 高交互**(资深开发者好用,先驱新手也看得懂)。

## 文件

| 文件 | 是什么 |
|---|---|
| [`builders-workbench-opc-v3.html`](builders-workbench-opc-v3.html) | **主原型(当前)**。5 个面默认进**项目**:**项目**(北极星横幅 + 「因 → 驱动(领先时长·因果强度)→ 果」配对因果链,引领/滞后显眼前置)/ **待办**(两道关)/ **流水线** / **运行**(实时运行图 + journal)/ **自动通过**。新增 **「+ 新建产品」分步向导**:起名 → 洞察 → 竞品分析 → 北极星 → 原型设计 → 先行指标 → 上线,逐步把指标搭起来(度量内建、绝不编造)。全量大白话、零主题化黑话(卡口/分身/投喂/放行/复利…已清)。 |
| [`builders-workbench-opc-v2.html`](builders-workbench-opc-v2.html) | 上一版主原型(保留作里程碑;v3 在其基础上加新建项目向导 + 重构项目面因果链 + 彻底去黑话) |
| [`builders-workbench-opc-v1.html`](builders-workbench-opc-v1.html) | 更早一版(术语较专业,保留作里程碑) |
| [`framework.md`](framework.md) | 世界观:传统 PM → AI 原生 PM,6+1 控制点,先行/结果指标 |
| [`claude-workflow-standard.md`](claude-workflow-standard.md) | **Workflow 标准实现方案**:Claude Workflow 工具的执行模型 / API / 标准骨架(本产品的底层引擎,跨 4 次真跑实测,标注〔文档〕vs〔实测〕) |
| [`skills/create-loop/`](skills/create-loop/) | 把「创建 workflow」做成的 skill:清晰 Query + 固定 Workflow 模板 + Agent Team + 清晰 Goal,对齐 Claude Workflow 工具 |
| [`builders-workbench-genesis-v1.html`](builders-workbench-genesis-v1.html) | 冷启动第一步:识别 / 导入已有家底(skills+agents)→ 建项目 → 建第一条 workflow |
| [`landing-v1.html`](landing-v1.html) | 产品落地页 · 由「落地页 workflow」用真实 skill 实跑产出(design-review 9.2 过闸) |
| [`workbench.html`](workbench.html) | 原始设计系统(clay / Tiempos / 暖纸,三栏);[`summary.html`](summary.html) · [`comparison.html`](comparison.html) 为早期说明 / 对比稿 |

## 设计原则

- **绿色隐身,只有红 / 黄出声**:界面只显示需要人拍板的事;上千个 AI 实例正常跑 = 待办零条目。
- **两道关**:**定标准**(认一次合格线,以后同类自动通过)+ **要你批 · 花钱**(花钱 / 对外 / 撤不回 → 每次人批 + 可急停)。机器判不了「合格线够不够」,所以放行权永远在人 + 已认可规则手上,不在脚本里自动通过。
- **项目只看两类指标**:**先行**(因 · 快 · 你今天能动)→ 驱动 → **结果**(果 · 慢 · 改不动);重点是引导你自己**找到并建立**这两组指标。新建项目时就把它们埋好(度量内建)。
- **基于已有资产**:workflow 由你已有的 skills/agents 拼成;产物只引用真实存在的 skill(已用 156 个真实 skill 实测)。
- **开卷作业**:「运行」面把 Claude 真实的 Workflow 原语(阶段 / agent 节点 / pipeline 拓扑 / 实时 journal / 自动重跑)直接画出来,所见即真跑——不另造抽象、不做可拖拽编辑画布。
- **大白话**:面向资深开发者也面向先驱新手,界面不搞主题化隐喻(不要"舰队/卡口/分身/投喂/放行/复利");用「动词 + 结果」说话。

## 状态

主原型在 **v3**(项目面因果链 + 新建项目向导 + 全量去黑话,已 Preview 实跑自查、零报错)。`loop-buddy` 仓的 `builders-workbench` 分支同步留档。

**待办(下一步)**:把「运行」面再往 OpenAI Symphony / 腾讯 Mavis workroom 那种**活体工作间**推一版 —— 阶段泳道、活动边流动粒子、两类关口用形状区分、缓存命中「⚡秒过」、journal↔节点联动高亮、原语图例(详见交付说明)。
