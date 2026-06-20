# Builders' Workbench

面向**资深独立开发者 / 一人公司(OPC)**的 AI 原生项目工作台。

> 一句话主线:说一句目标 → Claude 用你已有的 skills/agents 拼出一条 **workflow** → 你只认它的**验收标准** → 开跑后它自治、派生成百上千 agent 去跑,异常才浮到你面前。

核心信念:把你对「怎样算做对了」的一次判断,固化成可复用、可自动放行的**认证模式(认证策略飞轮)**——你写得越多,系统越安静。这是手写脚本攒不出的复利资产。

## 文件

| 文件 | 是什么 |
|---|---|
| [`framework.md`](framework.md) | 世界观:传统 PM → AI 原生 PM,6+1 控制点,先行/滞后指标 |
| [`workbench.html`](workbench.html) | 原始设计系统 · 组合概览/单项目(clay/Tiempos/暖纸,三栏) |
| [`summary.html`](summary.html) · [`comparison.html`](comparison.html) | 早期说明 / 对比稿 |
| [`builders-workbench-opc-v1.html`](builders-workbench-opc-v1.html) | **主原型** · OPC 工作台:收件箱(两道闸)/ 舰队(真实 skill 链)/ 看板(引领→滞后)/ 飞轮。数据已接真实的 6 条 Loop + 156 skill |
| [`builders-workbench-genesis-v1.html`](builders-workbench-genesis-v1.html) | 冷启动第一步:识别/导入家底 → 建项目 → 建第一条 workflow |
| [`landing-v1.html`](landing-v1.html) | 产品落地页 · 由「落地页 Loop」用真实 skill 实跑产出(design-review 9.2 过闸) |
| [`skills/create-loop/`](skills/create-loop/) | 把「创建 workflow」做成的 skill:Query + 固定 Workflow 模板 + Agent Team + Goal,对齐 Claude Workflow 工具 |

## 设计原则

- **绿色隐身,只有红/黄出声**:界面只显示需要人的事;1000 个 agent 正常跑 = 收件箱零条目。
- **两道闸**:验收闸(人认证达标线)+ 副作用闸(对外/花钱/不可逆人批 + kill switch)。
- **项目管理只看两类指标**:引领(因·快·你能拨)→ 驱动 → 滞后(果·慢);重点是引导用户自己建立指标。
- **基于已有资产**:workflow 由你已有的 skills/agents 拼成;创建的产物只用真实存在的 skill。
