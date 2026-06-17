# 司南 · Loop 工作台 — 设计冲刺 Brief（多 Agent 共享 ground truth）

> 这份文件是 PM / 设计 / 蓝军 三个 sub-agent 的唯一权威输入。开工前先 Read 它,并 Read 下面两份 HTML 设计稿。

## 0. 任务与铁律
- 用 PM / 设计 / 蓝军三个 sub-agent + 编排者(主控)形成**对抗循环**,产出"多角色 TS Loop 工作台"的**最终设计**。
- **铁律一:只设计,不实现。** TS 只体现为「组件架构 + 类型/接口 + 状态归属」,不写实现代码。可视化产出为 HTML 设计稿(沿用司南设计系统),HTML 属于"设计稿"不算"实现"。
- **铁律二:工作台必须同时服务多角色**(见 §3),不是单角色工具。
- **铁律三:对抗是真的。** PM/设计要预判蓝军攻击并主动化解;蓝军要从第一性原理把方案打穿,给"必须解决项"而非空泛吐槽。

## 1. 产品语境
- 司南 = AI-Native 研发控制面。已有 v1「作战室」单屏 = 监督视图(工程负责人视角)。
  文件:`/Users/gravity/projects/sinan-control-plane-v1.html` —— **只有这一屏设计完成,其余为占位。**
- Loop 工作台 = 操作者驾驶舱,比作战室下钻一层。已有 v1 草稿(仅第一屏可看):
  文件:`/Users/gravity/projects/sinan-loop-workbench-v1.html`
- **两份 HTML 都必须 Read,吸收设计系统(配色/字体/纹理/双视角 lens)与信息架构。**

## 2. Loop 的定义与机制
- Loop = Loop Engineering = 一条贯穿研发全流程的 Workflow。七环节:需求/设计 → 规划/进度 → 开发 → 评审 → 测试/验证 → 集成/发布 → 运维/事故。
- 已上线两类 Loop:**问题单提交(file)** + **问题单修复(fix)**。其余环节待建。
- 资产:Skill Hub(~214 skills)、洞察雷达(竞品情报 skill + 站点 → 产出"自规划提单")、运维 MaaS 看板 + CI、Agent.md。
- 需求来源:① 用户上报 issue;② 竞品对标自规划。
- 派发:工作台把 Issue 派发到"绑定了模型"的 Loop(复杂→Opus,简单→Medium/Haiku)。
- 每个 Loop = 状态机 + 工序时间线。示例 code-fix loop 有 5 个 Agent:`trigger`、`test-runner`、`developer`、`reviewer`、`orchestrator`(主编排器)。同一时刻通常仅一个在执行,其余待命/已交班(但见蓝军②:别把"单活"硬编死)。
- 每个 Loop = 一间独立**虚拟办公室**(对标腾讯 Mavis,拟人化、交互友好)。
- 编排总控 = **Symphony**(对标 OpenAI):处理异常与人工介入(loop 需与人交互 / loop 因异常停止)。
- 规模目标:最终管理**成千上万个 Agent**干活。

## 3. 多角色(必须同时服务,这是硬需求)
| 角色 | 核心 JTBD | 关注 |
|---|---|---|
| 质量 / 项目管理(PM/QA) | 项目健康度、看板指标、需求进度、每日趋势 | 度量看板 + 编排台总览 |
| 工程师 | 派发 issue、盯 Loop 执行、卡住时接管 | 虚拟办公室 + Symphony |
| 资产 owner | 改 Agent.md、换模型、组 Loop 蓝图、管 Skill | 资产库 |
**关键:同一工作台、多角色同时在用 → 必须设计:角色化视图 + 共享实时状态 + 权限/可见性模型 + 通知/介入路由。**

## 4. 指标(度量看板核心)
- AI 提单接收率 = accepted / filed(issue 分类:accepted / non-issue / duplicate)。
- AI 解单合入率 = merged MR / opened MR(含关闭)。
- 净 issue 趋势(healthy = 日下降)。
- **必须设计"护栏指标"**(见蓝军④):健康 = 吞吐 × 质量,只看吞吐会被 game。

## 5. 蓝军已埋的 6 个第一性问题(PM/设计须化解;蓝军须继续加深 + 找新的)
1. 拟人化办公室在规模上会崩 → office 必须是"下钻详情",列表层必须高密度。UI 第一性职责 = 把稀缺的人类注意力路由到那 ~5% 真正需人的 loop。
2. "同一时刻只有一个 Agent 跑"是会漏的假设 → 状态机要支持并行/分支,别硬编单活。
3. "Agent.md 是 Workflow 唯一可替换部分"under-model 了 → 真正配置面 = MD + 模型绑定 + skills + 权限边界 + 验收门(gate)。
4. 健康度指标可被反向优化 → 接收率↑可能因挑"安全单";合入率↑可能因合低质 MR;issue↓可能因关成"非问题"或产品停滞。必须配护栏:合入后回滚/缺陷率、reopen 率、AI-MR 人工改写率、复审抽样命中率。
5. Symphony 人工瓶颈 + 橡皮图章 → 几千 Agent 升级到几个人会成瓶颈且疲劳盖章。升级须按 value-at-risk 排序 + 可批量(approve-all-similar)+ 高危强制看 diff/填理由。
6. 对标 Mavis/Symphony 的 cargo-cult 风险 → 抄它们解决的 job(注意力路由 + 舰队治理),别抄 UI 隐喻。

## 6. 司南设计系统(tokens 摘要;细节以 HTML 为准)
- 配色:`--ink #1C1813`(深色 rail/卡) `--rice #EEE6D2`(底) `--paper #FAF5E9 / #F4ECDA`(卡) `--cinnabar #C23A22`(朱砂=主操作/待人工/异常) `--celadon #5E7D6C`(青瓷=运行/健康) `--ochre #C18A2C`(赭石=等待/试运行) `--muted #7A6C57 / #9A8B72`(灰) `--hairline #DAD0B7`。
- 字体:display=Fraunces+Noto Serif SC;正文=Noto Sans SC;标签/指标/EN=IBM Plex Mono。
- 母题:paper-grain 噪声叠加;左 248px 深色 rail;圆角 10px;hairline 描边;双视角 lens(人类/Agent)。

## 7. 交付物(全部写到 `/Users/gravity/projects/sinan-workbench-design/`)
- `pm-spec.md`(PM)、`ux-spec.md`(设计)、`blueteam-r1.md` / `blueteam-r2.md`(蓝军)。
- 主控负责:`DESIGN.md`(最终设计)、`decision-log.md`(每个蓝军问题的裁决记录)、多屏 HTML mockup(覆盖多角色)。
