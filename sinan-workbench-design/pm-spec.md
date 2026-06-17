# 司南 · Loop 工作台 — PM Spec（v1）

> 角色:资深 PM Agent。本文是 PM 侧权威输出,供「设计 Agent」与「蓝军 Agent」对抗压测。
> 铁律:**只设计不实现**;紧贴司南设计系统(ink/rice/paper · cinnabar=主操作/待人工/异常 · celadon=运行/健康 · ochre=等待/试运行 · 双视角 lens)。
> 上游权威:`00-brief.md`、`sinan-control-plane-v1.html`(作战室)、`sinan-loop-workbench-v1.html`(工作台 v1 草稿)。
> 阅读约定:文中 `Type`/状态枚举用 `mono` 写法;所有公式给精确口径;每个"会被 game 的指标"都配护栏。

---

## 0. TL;DR(给编排者)

Loop 工作台 = 操作者驾驶舱,比作战室**下钻一层**。作战室回答"我该看哪 ~5%",工作台回答"我现在动手处理这 5%、并养好那 95%"。
核心赌注:**UI 的第一性职责不是展示 Agent,而是把稀缺的人类注意力路由到真正需要人的那 ~5% Loop**(主动化解蓝军 #1)。
三角色(质量/PM、工程师、资产 owner)**共用一个工作台、同一份实时状态**,靠"角色化默认视图 + 统一领域模型 + RBAC + 介入路由"做到同时在用不打架。
MVP:先把已上线的 **file + fix** 两类 Loop 完整包进来,再沿七环节扩。

---

## 1. 定位与边界

### 1.1 一句话定位
> **作战室是"看板 + 决策入口"(监督面,管全局);Loop 工作台是"驾驶舱 + 工位"(操作面,管单体执行 + 养资产)。**

### 1.2 与 v1 作战室的分工(下钻关系,不是平级)

| 维度 | 作战室 Command Room | Loop 工作台 Loop Workbench |
|---|---|---|
| 主用户 | 工程负责人 Heidi(监督) | 操作者三角色:PM/QA、工程师、资产 owner |
| 时间尺度 | 天/周趋势、北极星 | 此刻/分钟级、单 Loop 生命周期 |
| 核心问题 | "整条流水线健康吗?哪 20% 要我裁决?" | "这个卡住的 Loop 怎么救?这条蓝图怎么养?今天指标为什么动?" |
| 主对象 | 七环节流水线 + 全局介入队列 | LoopRun 实例 + Agent 工位 + 资产蓝图 |
| 北极星 | 零干预闭环率(全局) | 提单接收率 / 解单合入率 / 净 Issue 趋势(可下钻到单 Loop) |
| 下钻动作 | 队列卡片「接管」→ **跳工作台对应办公室** | 办公室内「升级」→ 回流作战室介入队列(同一 Issue 同一 ID) |

**边界红线(防止两个面互相侵蚀):**
- 作战室**不做**:改 Agent.md、换模型、组蓝图、单 Loop 工序级操作 → 全部下沉工作台。
- 工作台**不做**:跨七环节的战略态势、对外汇报口径的北极星罗盘 → 留在作战室。
- **同一实体单一事实源**:Issue / LoopRun / Gate 在两个面共享同一 ID 与状态机;作战室是它的"低分辨率投影",工作台是"高分辨率原图"。绝不允许两边各存一份状态(蓝军必查的数据一致性漏洞)。

### 1.3 解决谁的什么问题(痛点 → 价值)
- **PM/QA**:今天没有"项目健康度真相",只能等周报;吞吐数字无法判断质量是否在塌方 → 工作台给**实时复合健康度 + 护栏**。
- **工程师**:几千 Agent 在跑,出事时不知道"该救哪个、为什么卡、怎么接管" → 工作台给**按 value-at-risk 排序的介入队列 + 一键接管的办公室**。
- **资产 owner**:Agent.md / 模型 / Skill / 蓝图散落,改一处不知道影响多少在跑的 Loop → 工作台给**带影响面预估 + 灰度发布的资产库**。

---

## 2. 多角色同时使用模型(硬需求 · 重点)

> 本节是 brief 标注的硬需求与蓝军主攻点。设计目标:**同一工作台,三类角色同时在线操作,互不打架,且"该被叫醒的人"被精确叫醒。**

### 2.1 三类 Persona 与 JTBD

**A. 质量 / 项目管理(PM/QA)— "守门人 Lin"**
- Persona:QA Lead / 项目经理,对交付质量与节奏负责,不写代码但懂流程。每天第一件事看健康度。
- JTBD:
  - 当我早上打开工作台,我想**一眼判断"昨天到现在,系统是更健康还是更脆了"**,以便决定今天要不要拉警报。
  - 当某个指标变好时,我想**确认它不是被 game 出来的**(护栏没破),以便放心向上汇报。
  - 当某条蓝图/某类 Issue 质量下滑,我想**定位到是哪条蓝图、哪个 Agent、哪个 Gate 在漏**,以便派人去修资产。
- 默认落地页:**度量看板(Metrics)**。
- 主要写操作:配置告警阈值、对复审抽样打标、把"系统性问题"转成资产工单派给 owner。
- **只读边界**:不能改 Agent.md / 模型绑定 / 不能批准受保护边界的合入(避免既当裁判又当运动员)。

**B. 工程师 — "救火与接管者 Wei"**
- Persona:研发工程师,对具体 Issue 的修复结果负责。日常被工作台"叫醒",而不是主动巡检。
- JTBD:
  - 当一个 Loop 升级到我,我想**用最少点击看懂"它做了什么、卡在哪、风险多大"**,以便决定批准/退回/接管。
  - 当我决定接管,我想**无缝接手办公室的上下文(diff、用例、日志、Agent 交班记录)**,以便不用从零理解。
  - 当我想主动派活,我想**把一个新 Issue 派给"绑定了合适模型/蓝图"的 Loop**,以便它自跑。
- 默认落地页:**编排台 Symphony(介入队列)** + 单 **虚拟办公室**。
- 主要写操作:approve/reject/接管/退回重做/派发/调整单次运行的模型档位。
- 边界:可操作自己被分派或认领的 Loop;受保护边界(鉴权/支付/对外发布/删数据)需对应权限位。

**C. 资产 owner — "养蓝图的 Mei"**
- Persona:平台/资产工程师,对"Loop 蓝图、Agent.md、Skill、模型绑定"这套**可复用生产资料**的质量负责。不盯单个 Issue,盯"产线本身"。
- JTBD:
  - 当某类 Loop 反复在同一 Gate 卡住或被人工改写,我想**改 Agent.md / 换模型 / 加 Skill / 调 Gate**,以便从根上提自主率。
  - 当我要改一条在跑的蓝图,我想**先看到"影响多少在跑实例 + 历史回归数据"**,以便灰度发布而不是一刀切。
  - 当我发布新版本蓝图,我想**A/B 对照新旧版本的护栏指标**,以便证明改对了。
- 默认落地页:**资产库 · Loop 蓝图列表**。
- 主要写操作:编辑蓝图/MD/ModelBinding/Skill 绑定/Gate 定义;版本发布与灰度;回滚。
- 边界:改资产需 `asset:write`;**发布到生产蓝图需二次确认 + 影响面提示**(蓝军 #3 配置面的安全闸)。

### 2.2 "同一个工作台"里三视图如何共存(不是三个 App)

**机制一:一套 IA + 角色化默认落地页 + 个人可覆盖。**
左侧 248px 深色 rail(沿用设计系统)对所有人**结构相同**,只是:
- 登录后按角色跳到默认落地页(见 §3.4),rail 高亮项不同。
- rail 各导航项右侧 `mono` 计数徽标对所有人是**同一份实时数 字**(如"待人工 5"),保证三人看到的世界一致。
- 角色只影响"默认进哪、能点亮哪些写操作",**不裁剪信息可见性**(除非命中 §2.5 可见性边界)——避免"PM 看到的健康度和工程师看到的不是同一个数"。

**机制二:双视角 lens 升级为"角色 lens"。**
v1 已有「人类视角 / Agent 视角」「编排视角 / 办公室视角」的 lens 母题。工作台把它泛化为**顶栏统一的视角切换器**,但**视角 ≠ 角色**:
- 视角(lens)= 看同一份数据的"镜头":`监督镜`(聚合/趋势)↔ `执行镜`(单体/工序)↔ `资产镜`(蓝图/版本)。
- 角色(role)= 你是谁、能写什么。
- 任何角色都能切任意 lens(PM 也能切执行镜去看某个办公室),但**写操作按 role 决定是否点亮**。这样"角色化视图"是默认+引导,不是牢笼(化解蓝军 #6:抄 job 不抄隐喻——lens 是注意力工具,不是权限围栏)。

**机制三:深链 + 同一 ID 串场。**
任何对象(LoopRun / Issue / Gate / 蓝图)都有稳定 URL。PM 在看板点到"某蓝图异常率高"→ 深链到资产镜的该蓝图;工程师在办公室点"这条蓝图"→ 深链同一对象。三人围着同一对象协作时看的是同一页。

### 2.3 共享的实时状态(Single Source of Truth)

**状态分层(明确归属,蓝军必问"状态住在哪"):**

| 层 | 内容 | 来源 | 推送 | 谁可写 |
|---|---|---|---|---|
| Fleet 聚合层 | 在岗/运行/待命/异常计数、四大指标、护栏 | 后端聚合服务 | 服务器推送(stream),秒级 | 系统(只读快照) |
| LoopRun 实例层 | 每个 Loop 的 `state`、当前工序、自主链步数、绑定模型、Gate 状态 | Loop 引擎事件流 | 服务器推送,事件驱动 | 引擎 + 人工介入动作 |
| Office 协作层 | 谁正在看/接管这个办公室、人工动作、评论 | 协作服务 | 服务器推送 | 在场的人 |
| Asset 资产层 | 蓝图/MD/绑定/Skill/Gate 的版本与生效态 | 资产仓库(版本化) | 拉取 + 失效广播 | `asset:write` |

**前端状态归属(TS 设计层,不写实现):**
- 全局只读流 store:`FleetSnapshot`、`LoopRunIndex`(列表投影)——所有视图共享订阅,单一数据源。
- 局部交互 store:当前选中 LoopRun、当前 lens、抽屉开合——**不进全局**,避免多角色互相把对方的选中态冲掉。
- 乐观更新 + 服务器回执对账:人工动作先本地置 `pending`,收到引擎事件再 settle;冲突以服务器为准(见 §2.6)。

### 2.4 权限 / 可见性模型(RBAC + ABAC 混合)

**主体—角色—权限位:**
```
Role            权限位(capabilities)
─────────────────────────────────────────────────────────
PM/QA           metrics:read, sampling:write, alert:config,
                ticket:create, run:read(all)            // 只读运行,不改资产/不批合入
Engineer        run:read(all), run:operate(scope),
                dispatch:create, takeover:claim,
                approve:standard                        // 标准边界可批
Asset Owner     asset:read, asset:write, asset:publish,
                run:read(all)
Lead(叠加)     approve:protected                       // 鉴权/支付/对外发布/删数据
Admin           role:assign, policy:edit
```
- **角色可叠加**(一个人可同时是 Engineer + Lead)。
- **ABAC 维度**:`run:operate(scope)` 的 scope 由"我被分派 / 我认领 / 我在我负责的蓝图下"决定,避免任何工程师能动所有 Loop。
- **关键约束:approve:protected 与 asset:publish 受"四眼原则"约束**(value-at-risk 高于阈值时强制二人),化解蓝军 #5 橡皮图章。

**可见性默认全开、按需收窄(而非默认收窄):**
- 默认:三角色对运行态 100% 可见(一致世界观)。
- 收窄触发器(ABAC):涉密 Issue(安全漏洞类)→ 仅 `security:read`;含 PII 的日志 → 脱敏展示,原文需 `pii:read`。
- 收窄是**例外**且**显式标注**("此办公室含敏感内容,已脱敏"),不是隐式裁剪。

### 2.5 通知与介入路由(谁该被叫醒)

> 这是"把稀缺人类注意力路由到 5%"的执行层。核心:**默认不打扰,只有越界/异常/分歧才升级,且按 value-at-risk 排序、可批量。**

**路由规则(Routing Policy,资产化、可配置 —— 见 §3 配置面):**
```
事件类型                          → 路由目标                 → 优先级 P
─────────────────────────────────────────────────────────────────
Gate 命中受保护边界(标准)        → 值班工程师队列            P2
Gate 命中受保护边界(高危*)       → Lead + 工程师(四眼)      P0  *鉴权/支付/对外/删数据
Loop 异常停止(基础设施类)        → 平台 oncall              P1
Loop 异常停止(蓝图逻辑类)        → 蓝图 owner               P1
评审 Agent 分歧无法收敛           → 工程师(带两方案 diff)    P1
护栏指标破阈(如回滚率↑)          → PM/QA + 蓝图 owner        P1
新 Issue 待派发                   → 工程师派发队列(非中断)   P3
```
- **优先级 = value-at-risk 排序**:`P = f(改动 blast radius, 受影响用户量, 可逆性, 紧迫度)`。队列严格按 P 排,不按时间排(化解蓝军 #5"几千升级到几个人成瓶颈")。
- **去重 / 合并**:同类升级(如"同一蓝图本批 12 个都触发同一边界")折叠成一组,支持 `approve-all-similar`(带统一理由),而非 12 次盖章。
- **升级而非广播**:一个事件只点亮**一个**责任队列(按上表),不全员弹窗。被 ack 后从他人队列消失。
- **节流与防疲劳**:同一人单位时间内 P0/P1 超过阈值 → 触发"过载保护",提示批量处理或临时降派发速率(防橡皮图章)。
- **通道**:站内介入队列(主)+ 可选外推(高危 P0 才外推 push/IM)。MVP 仅站内 + P0 外推。

### 2.6 多人同时在用,如何不互相打架(并发冲突,蓝军主攻)

**冲突场景与对策:**

| 场景 | 风险 | 对策 |
|---|---|---|
| 两个工程师同时点同一升级 | 双重批准 / 动作打架 | **软锁 claim**:第一个点"接管/处理"的人获得 office 软锁,其余人界面显示"Wei 正在处理(02:14)",按钮转"旁观/请求接手"。锁有 TTL,超时或离开自动释放。 |
| PM 在看板改阈值 ↔ owner 在改同蓝图的 Gate | 配置覆盖 | 资产层**乐观锁 + 版本号**:提交带基版本号,冲突即拒并展示对方变更,要求 rebase。 |
| 引擎自己推进 ↔ 人想接管 | 状态竞争 | 人工"接管"= 向引擎发**暂停意图**,引擎到下一个安全检查点交出控制权并置 `human_holding`;未到检查点前按钮显示"正在交接"。绝不前端假装已接管。 |
| 多人都在看同一 office | 视图打架 | 看是无锁的(只读订阅);**只有写动作要锁**。在场者头像实时显示(协作层),避免"我以为只有我在看"。 |
| 乐观更新与服务器回执不一致 | 界面闪烁/误判 | 动作置 `pending`→ 引擎事件 settle;若被拒,回滚本地态并 toast 原因。**服务器是唯一裁判**。 |

**一句话原则:看(read)永远无锁、全员一致;动(write)永远抢占式软锁 + 服务器对账。**

---

## 3. 信息架构(Surfaces / 导航 / 默认落地页)

### 3.1 Surface 清单(沿用 v1 rail 两段式)
```
执行 EXECUTION
  ◎ 编排台 Symphony      — 介入队列 + Fleet 总览(按 value-at-risk 排序)
  ⬡ 虚拟办公室 Office     — 单 LoopRun 下钻:Agent 工位 + 状态机时间线 + 介入面板
  ⤵ Issue 派发 Dispatch   — 新 Issue → 选蓝图/模型 → 派发
  ▤ 度量看板 Metrics      — 复合健康度 + 护栏 + 趋势 + 抽样

资产库 · 共享底座 ASSETS(从作战室"Agent 基础设施"抽出的共享层)
  Loop 蓝图 Blueprint     — DAG + Gate + 默认绑定;版本/灰度/A-B
  Agent · MD              — Agent.md 编辑 + 影响面
  Skill Hub               — ~214 skills,绑定关系
  模型 / MaaS             — ModelBinding(Opus/Medium/Haiku)+ 用量/成本
```
> rail 结构对三角色一致;徽标计数是共享实时数。沿用 ink rail / cinnabar 高亮 / mono 计数。

### 3.2 三层导航(防止规模崩盘 —— 化解蓝军 #1)
1. **Fleet 列表层(默认、高密度)**:成千上万 LoopRun 不铺办公室,而是**一行一个的高密度列表**,按 P 排序,顶端永远是"待人工/异常"。office 是**下钻详情**,不是默认视图。这是注意力路由的物理保证。
2. **Office 详情层(下钻才进)**:点一行才展开拟人化办公室(Mavis 隐喻只在这层、且只为"看懂单个"服务)。
3. **Asset 资产层(养产线)**:与运行态正交,owner 在此改"模板",改动经灰度回流到运行态。

> 规模数学(预答蓝军 #1):列表层每行约 56px,虚拟滚动;1 万实例下默认只渲染"需人工/异常"子集(通常 <200 行)+ 折叠的"运行中 N"。**人类永远不滚一万行——排序+折叠把注意力压到 O(需介入数) 而非 O(总数)。**

### 3.3 顶栏(对所有角色一致)
- 标题「Loop 工作台 / LOOP WORKBENCH」
- **视角 lens 切换器**:监督镜 / 执行镜 / 资产镜(见 §2.2 机制二)
- 全局搜索(按 Issue#/LoopRun/蓝图/Agent)
- 日期 + 当前用户(rail-foot 显示角色)

### 3.4 各角色默认落地页

| 角色 | 默认落地页 | 默认 lens | 首屏首要信息 |
|---|---|---|---|
| PM/QA | 度量看板 Metrics | 监督镜 | 复合健康度灯 + 三大指标 + 护栏是否破阈 |
| 工程师 | 编排台 Symphony | 执行镜 | 我的介入队列(按 P)+ Fleet 状态条 |
| 资产 owner | 资产库 · Loop 蓝图 | 资产镜 | 蓝图列表 + 各蓝图近 7 日护栏/自主率 + 待审版本 |

---

## 4. 领域模型(实体 / 关系 / 状态机)

> 以 TS 接口"形状"表达(只设计不实现,不写逻辑)。命名沿用 v1 草稿("Loop 蓝图 / 虚拟办公室 / 工序时间线 / Gate / ModelBinding")。

### 4.1 实体关系总览
```
LoopBlueprint 1───* LoopRun *───1 Issue
     │                  │
     │ defines          │ runs-on
     ▼                  ▼
  AgentSpec        AgentInstance(=办公室里的"工位"/desk)
     │ binds            │ executes
     ├─ ModelBinding    └─ produces ──> Artifact(MR / 用例 / 报告)
     ├─ Skill[]
     └─ Gate[]          每个 LoopRun 推进时在 Gate 处可能 ──> Escalation
```

### 4.2 实体定义 + 状态枚举

**① LoopBlueprint(Loop 蓝图)— 可复用模板**
```ts
interface LoopBlueprint {
  id: string;                 // e.g. "code-fix-v3"
  kind: 'file' | 'fix' | ...; // MVP 仅 file/fix
  phase: Phase;               // 七环节定位
  dag: AgentNode[];           // 工序 DAG(支持并行/分支,见 §4.3)
  gates: Gate[];              // 验收门集合
  defaultBindings: ModelBinding[];
  version: SemVer;
  status: BlueprintStatus;
}
type BlueprintStatus = 'draft' | 'canary' | 'active' | 'deprecated' | 'archived';
```
- 状态机:`draft →(发布)→ canary →(灰度达标)→ active →(被新版替代)→ deprecated → archived`;`canary/active →(回滚)→ deprecated`。
- **化解蓝军 #3**:蓝图的"可配置面"= `dag(工序) + gates(验收门) + defaultBindings(模型) + AgentSpec(MD/Skill/权限边界)`,**不只是 Agent.md**。

**② AgentSpec / AgentInstance**
```ts
interface AgentSpec {           // 模板里的角色定义
  role: string;                 // trigger | test-runner | developer | reviewer | orchestrator ...
  md: AgentMdRef;               // Agent.md(可替换)
  modelBinding: ModelBinding;   // Opus/Medium/Haiku
  skills: SkillRef[];
  permissionScope: PermissionScope; // 自动接受边界 / 受保护边界
}
interface AgentInstance {        // 某个 LoopRun 里活的"工位"
  specRole: string;
  state: AgentState;
  currentAction: string;
}
type AgentState = 'idle' | 'active' | 'handed_off' | 'waiting' | 'blocked' | 'failed';
```
- `orchestrator` = Symphony 局内的 conductor(编排器)。
- **化解蓝军 #2**:`dag` 支持并行节点,故同一 LoopRun 可有**多个 `active`**;界面"同一时刻仅一个在执行"只是**当前常见态的呈现**,不是模型约束。模型层默认支持并行/分支,UI 在多 active 时展示并行泳道。

**③ LoopRun(实例)— 一间虚拟办公室**
```ts
interface LoopRun {
  id: string;                 // e.g. "#4821"
  blueprintId: string;        // ← code-fix-v3
  issueId: string;
  state: LoopRunState;
  cursor: GateRef | NodeRef;  // 当前工序/门
  autonomyChain: number;      // 自主链步数(无人工连续步数)
  agents: AgentInstance[];
  modelTier: 'opus' | 'medium' | 'haiku';
  holder?: UserRef;           // 被谁接管(软锁)
}
type LoopRunState =
  | 'queued'        // 已派发未起跑
  | 'running'       // 自主推进中(celadon)
  | 'at_gate'       // 卡在验收门(可能自动过,也可能需人工)
  | 'needs_human'   // 升级人工(cinnabar)
  | 'human_holding' // 人已接管,引擎让出控制
  | 'error'         // 异常停止(cinnabar/ink)
  | 'merged'        // 产出已合入/已交付
  | 'closed_done'   // 闭环成功
  | 'closed_dropped'// 关闭为非问题/放弃(计入护栏!)
  | 'rolled_back';  // 合入后回滚(计入护栏!)
```
状态机(主干):
```
queued → running → at_gate →[自动过]→ running → ... → merged → closed_done
                         └─[需人工]→ needs_human →[approve]→ running
                                              ├─[reject/退回]→ running(回退节点)
                                              └─[接管]→ human_holding →[放行/手改]→ running|merged
running|at_gate →[异常]→ error →[修复/重试]→ running | →[放弃]→ closed_dropped
merged →[线上缺陷/回滚]→ rolled_back   // 反哺护栏指标
```
> 颜色映射沿用设计系统:`running=celadon`、`needs_human/error=cinnabar`、`at_gate/waiting=ochre`、`human_holding`=ink 描边 + cinnabar 点。

**④ Issue**
```ts
interface Issue {
  id: string;
  source: 'user_report' | 'competitive_radar'; // 上报 / 竞品自规划
  triage: TriageState;
  severity: 'low'|'med'|'high'|'crit';
  loopRunId?: string;
}
type TriageState = 'new' | 'accepted' | 'non_issue' | 'duplicate';
```
- 状态机:`new →(分类)→ accepted | non_issue | duplicate`;`accepted →(派发)→ 绑定 LoopRun`。
- **指标耦合点**:接收率分子=`accepted`;护栏要盯 `non_issue/duplicate` 是否被滥用来刷分母(见 §6)。

**⑤ Gate(验收门)— 自主闭环的前提**
```ts
interface Gate {
  id: string;
  nodeRef: NodeRef;           // 挂在哪道工序后
  check: 'machine' | 'human' | 'hybrid'; // 可机检 / 必须人 / 混合
  criteria: string;           // 可机检验收标准(如"失败用例转绿")
  boundary?: ProtectedBoundary; // 是否受保护边界
  result: GateResult;
}
type GateResult = 'pending' | 'passed_auto' | 'passed_human' | 'failed' | 'escalated';
type ProtectedBoundary =
  | 'auth' | 'payment' | 'public_release' | 'data_delete'
  | 'cross_service_schema' | 'push_main' | null;
```
- **化解蓝军 #3 + 接住作战室伏笔**:作战室已点出"9/13 节点绑定可机检验收是瓶颈"。Gate 把"成功标准"实体化:`check=machine` 才能自动闭环;`human/hybrid` 必走升级。补 Gate = 直接抬升自主率,这是 owner 的核心抓手。

**⑥ ModelBinding**
```ts
interface ModelBinding {
  agentRole: string;
  tier: 'opus' | 'medium' | 'haiku'; // 复杂→Opus,简单→Medium/Haiku
  policy: 'fixed' | 'auto_by_complexity';
  costPer1k?: number;
}
```
- 状态:绑定本身无生命周期,但其**变更**走资产版本化(灰度/回滚)。

**⑦ Skill**
```ts
interface Skill { id: string; name: string; status: 'live'|'beta'|'deprecated'; usedBy: string[]; }
```

**⑧ Escalation(升级单)— 把"卡住"实体化,驱动介入路由**
```ts
interface Escalation {
  id: string;
  loopRunId: string;
  reason: 'protected_boundary' | 'agent_disagreement' | 'infra_error' | 'guardrail_breach';
  valueAtRisk: number;        // 排序键 P
  routeTo: 'engineer'|'lead'|'platform_oncall'|'pm_qa'|'blueprint_owner';
  state: EscalationState;
  groupKey?: string;          // 同类合并键(approve-all-similar)
}
type EscalationState = 'open' | 'claimed' | 'resolved' | 'auto_resolved' | 'expired';
```

### 4.3 并行/分支(显式化解蓝军 #2)
- `LoopBlueprint.dag` 为 DAG:节点可声明 `parallel`(同时跑)与 `branch`(条件分叉,如"自测失败→回 developer;通过→reviewer")。
- 因此 `LoopRun.agents` 可同时多个 `active`;`autonomyChain` 按"无人工连续步数"计,与并行无关。
- UI 契约:Office 在并行态时,desks 区从"单 active 高亮"切为"多泳道并行高亮",时间线分叉。**模型从不硬编单活。**

---

## 5. 核心流程

### 5.1 Issue 派发流(Dispatch)
```
来源(用户上报 / 竞品雷达)
  → Issue.new
  → triage(file-loop 或人工):accepted / non_issue / duplicate
  → [accepted] 选蓝图(按 kind/phase) + 选模型档(complexity→Opus/Medium/Haiku)
  → 创建 LoopRun(queued) → 起跑(running)
  → 自主推进:trigger→reproduce→locate→patch→verify→(Gate)review→merge
  → Gate=machine 且 passed_auto → 一路到 closed_done
  → Gate=human/boundary → Escalation → §5.2
```
- 派发入口在 rail「Issue 派发」;工程师可手动派,系统可按规则自动派。
- **预答蓝军 #4(挑安全单)**:派发不允许"只挑简单单"刷接收率——派发面板显示**复杂度分布**,看板用"分复杂度接收率"(沿用作战室 L1–L5 分层),避免均值掩盖。

### 5.2 升级 / 人工介入流(Symphony)
```
触发:Gate 命中 boundary / Agent 分歧 / 异常 / 护栏破阈
  → 生成 Escalation(算 valueAtRisk → P,定 routeTo)
  → 进对应队列(编排台,按 P 排序,同类折叠)
  → 责任人 claim(软锁) →
       ├ approve(标准边界):一键放行 → LoopRun.running
       ├ approve(高危边界):强制看 diff + 填理由 + 四眼 → 放行
       ├ approve-all-similar:对一组同 groupKey 统一放行(填一次理由)
       ├ reject/退回:回退到指定节点重做
       └ 接管:LoopRun→human_holding,引擎到检查点让出,人手改后放行
  → resolved;若 P 是高危 → 记入复审抽样池(见 §6 护栏)
```
- **化解蓝军 #5(瓶颈+橡皮图章)**:① 严格 P 排序;② approve-all-similar 批量;③ 高危**强制** diff+理由+四眼;④ 过载保护节流;⑤ 一键批准会被复审抽样反向核对(见 §6),让"盖章"有事后代价。
- 与作战室联动:同一 Escalation 在作战室"需要你介入"队列与工作台编排台是**同一对象**;在任一处 ack 即同步消失。

### 5.3 竞品自规划提单流(file · radar)
```
洞察雷达(竞品情报 Skill + 站点)
  → 抓取/对标 → 生成"自规划 Issue"(source=competitive_radar, 标注对标依据)
  → 进 triage(同 §5.1),但默认进 PM/QA 审核而非直接派发
  → accepted → 正常 fix/file loop
```
- **预答蓝军 #4(产品停滞伪装成 issue↓)**:竞品雷达是"净 Issue 趋势"的**对冲**——若用户上报↓但雷达提单也↓,可能是产品停滞;看板把两条来源**分列**,不许合并成一个"健康下降"的假象。

---

## 6. 指标框架 + 护栏指标

> 原则:**健康 = 吞吐 × 质量**,任一单看都能被 game。每个吞吐指标**强绑**一个护栏,复合判定才算"健康"。化解蓝军 #4。

### 6.1 主指标(吞吐 / 沿用 brief 口径)

| 指标 | 公式(精确口径) | healthy 方向 | 可被怎么 game |
|---|---|---|---|
| AI 提单接收率 | `accepted / filed`,filed = accepted+non_issue+duplicate(同窗口、同 source 分列) | ↑ | 挑"安全单"、把难单标 non_issue/duplicate 压分母 |
| AI 解单合入率 | `merged_MR / opened_MR`,opened 含已关闭 MR | ↑ | 合低质 MR、把会被打回的 MR 提前关掉不计 opened |
| 净 Issue 趋势 | `new_issues − closed_issues`(按日,user_report 与 radar **分列**) | 日下降 | 把 issue 关成 non_issue、或产品停滞导致 new↓ |

### 6.2 护栏指标(质量 / 反向核对,brief §5.4 点名)

| 护栏 | 公式 | 防的是 | 阈值(red) |
|---|---|---|---|
| 合入后回滚/缺陷率 | `(rolled_back + post_merge_defect) / merged`,窗口=合入后 14 天 | 合低质 MR 刷合入率 | > 8%(示例,待标定) |
| Reopen 率 | `reopened / closed_done`,窗口=关闭后 30 天 | 假闭环、关了又开 | > 6% |
| AI-MR 人工改写率 | `human_edited_lines / ai_total_lines`(接管/退回里人手改的占比) | "自主"实为人擦屁股 | > 25% |
| 复审抽样命中率 | `sampled_with_problem / sampled`,对**一键批准/auto-pass** 的 Gate 随机抽 N% 复审 | 橡皮图章、auto-pass 漏网 | > 10% |
| Non-issue/Dup 申诉翻案率 | `overturned / (non_issue+duplicate) sampled` | 拿误判压接收率分母 | > 8% |

### 6.3 复合"健康"定义(看板顶部一盏灯)
```
HEALTHY(绿) ⇔ 三大主指标达方向  AND  全部护栏未破红阈
AT_RISK(琥珀) ⇔ 主指标达标 但 ≥1 护栏进黄区(red 的 0.7×)
UNHEALTHY(朱砂) ⇔ ≥1 护栏破红阈  OR  ≥2 主指标反向
```
- **铁律:护栏破阈一票否决**——哪怕吞吐再漂亮,护栏红 = 系统不健康。这是反 game 的核心闸。
- 看板每个主指标卡片**贴身显示其护栏**(吞吐数字旁边就是质量数字),不让人只盯吞吐。沿用 metrics strip 设计,但每格扩为"主+护栏"双行。

### 6.4 指标的归属与下钻
- PM/QA 在监督镜看全局;任一指标可**按蓝图/按 source/按复杂度**下钻,定位"是哪条产线在拖后腿"→ 一键转资产工单给 owner。

---

## 7. MVP 范围 + 路线图

### 7.1 MVP(M0):把 file + fix 两类 Loop 完整包进来
**进:**
- Surfaces:编排台(介入队列+Fleet 列表,高密度)、虚拟办公室(file/fix 两类蓝图的 5-Agent 办公室 + 状态机时间线)、Issue 派发、度量看板(三主指标 + 5 护栏 + 复合健康灯)。
- 领域模型:LoopBlueprint(仅 file/fix)、LoopRun、Issue、Gate、AgentSpec/Instance、ModelBinding、Skill、Escalation 全部实体到位(七环节扩展时复用)。
- 多角色:三角色默认落地页 + 三视角 lens + RBAC 权限位 + 介入路由 + 软锁并发。
- 资产库:Loop 蓝图(读 + 改 MD/绑定/Gate,**含灰度/回滚**)、Skill Hub(读+绑定)、模型 MaaS(读+改绑定)。
- 护栏:5 个护栏指标 + 复审抽样池。

**不进(M0 明确不做,留给后续):**
- 七环节里 file/fix 之外的环节(需求/设计、规划、集成/发布、运维)——rail 占位但标"待建"(沿用作战室"待建"斜纹样式)。
- 高危外推通知除 P0 外的多通道;蓝图可视化 DAG 编辑器(M0 用结构化表单改,不做拖拽画布)。
- 跨组织多租户。

### 7.2 路线图(沿七环节扩)
```
M0(MVP) file + fix 两类 Loop 全功能 + 多角色 + 护栏 + 资产灰度
M1  评审 / 测试·验证 环节成熟(对接作战室"9/13 → 13/13"补 Gate),
        并行/分支 DAG 在 UI 落地(多泳道办公室)
M2  规划 / 进度(Mavis 试运行)纳管;蓝图 DAG 可视化编辑器
M3  集成 / 发布(补权限层 + 对外发布四眼)、需求 / 设计 环节
M4  运维 / 事故(接观测栈/MaaS 看板 + CI);全七环节闭环
横切  规模化:Fleet 列表虚拟化 + 升级路由策略可配置化(应对"成千上万 Agent")
```

---

## 8. 主动化解蓝军 6 问(产品相关部分)

| 蓝军问 | PM 侧化解(指向本文) |
|---|---|
| **#1 规模/拟人化会崩** | IA 三层:**列表层默认高密度、office 是下钻**(§3.2);排序+折叠把人类注意力压到 O(需介入数)(§3.2 规模数学);路由只点亮责任队列不广播(§2.5)。**UI 第一性职责 = 注意力路由**,写进 §0/§2.5。 |
| **#2 单活硬编** | `dag` 是 DAG,支持 parallel/branch;`LoopRun` 可多 `active`;UI 多泳道(§4.2③/§4.3)。"单活"仅是当前呈现,非模型约束。 |
| **#3 配置面 under-model** | 配置面 = **MD + ModelBinding + Skill + permissionScope + Gate**(§4.2①⑤⑥);资产层版本化 + 灰度 + 回滚 + 影响面预估(§4.2① / §2.4 publish 闸)。 |
| **#4 指标可反向优化** | 每个吞吐指标强绑护栏(§6.2);**护栏破阈一票否决**(§6.3);接收率按复杂度/来源分列防挑安全单(§5.1/§6.1);竞品雷达对冲"产品停滞伪装 issue↓"(§5.3/§6.1)。 |
| **#5 Symphony 瓶颈+橡皮图章** | value-at-risk 严格排序 + approve-all-similar 批量 + 高危强制 diff/理由/四眼 + 过载节流 + 复审抽样让盖章有事后代价(§2.5/§5.2/§6.2)。 |
| **#6 cargo-cult 隐喻** | lens=注意力镜头而非权限围栏;office 拟人化仅服务"看懂单个"且只在下钻层;抄的是 job(注意力路由+舰队治理)不是 UI(§2.2 机制二 / §3.2)。 |
| **(新增)多角色并发冲突** | 看无锁全员一致、写抢占式软锁 + 服务器对账(§2.6);状态分层与归属明确(§2.3);接管走引擎让权而非前端假装(§2.6)。 |

---

## 附:与设计/蓝军的交接点(本文留给下游的钩子)
- **给设计 Agent**:① 三视角 lens 的切换器形态 + 各 lens 默认布局;② Fleet 高密度列表行的信息密度(56px/行要塞下:名/state/蓝图/模型/工序进度/P);③ 护栏"主+护栏双行" metrics strip;④ 并行泳道办公室的视觉;⑤ 软锁"他人正在处理"的在场态。
- **给蓝军 Agent**:本文已主动堵 §8 七点,请重点压测 §0 末尾"三大未决风险"。
