# 01 · 原型清单与领域模型 — 迁移规格书

> 唯一依据:`Builders工作台-项目管理向导.dc.html`(3283 行)+ `support.js`(dc-runtime)。不引用任何其它文件。

---

## 目录
1. [运行时本质](#1-运行时本质)
2. [导航结构 + 状态机](#2-导航结构--状态机)
3. [屏幕逐一说明](#3-屏幕逐一说明)
4. [完整数据模型](#4-完整数据模型)
5. [视图派生逻辑(buildApp 关键)](#5-视图派生逻辑)
6. [设计系统 token](#6-设计系统-token)
7. [关键交互清单](#7-关键交互清单)

---

## 1. 运行时本质

### `support.js` — dc-runtime
- **编译产物**:由 `dc-runtime/src/*.ts` 经 bun build 生成,**基于 React**(`window.React`+`window.ReactDOM`,运行时从 CDN 加载)。
- **自定义元素** `<x-dc>` 是应用根,其 `innerHTML` 就是模板。
- **模板 DSL**:

| 语法 | 语义 |
|---|---|
| `{{ expr }}` | 文本/属性插值(getter 返回值) |
| `onClick="{{ fn }}"` | 事件绑定(fn 是 getter 返回的函数) |
| `<sc-if value="{{ bool }}">` | 条件渲染 |
| `<sc-for list="{{ arr }}" as="x">` | 列表渲染 |
| `style-hover="css"` | 伪 hover 内联样式(JS mouseenter 模拟) |
| `<helmet>` | 注入到 `<head>`(字体/全局 CSS) |

- **逻辑**:一个 `class Component extends DCLogic`,包含:
  - `state = {}` — 唯一可变状态树
  - `setState({...})` — 浅合并更新
  - `buildApp()` — `state` → 运营视图 ViewModel
  - `buildHubs()` — `state` → Hub 全屏 ViewModel
  - `renderVals()` — 总入口,合并两者 + 向导派生值,返回给模板绑定的大对象
- **流式占位**:生成期间 `html.sc-dc-streaming` class 激活 `sc-shine` shimmer 动画(css `@keyframes`)。

---

## 2. 导航结构 + 状态机

### 2.1 三层正交选择器

```
hub          = 'workspace' | 'skill' | 'agent' | 'routine' | 'cron'
               | 'connector' | 'knowledge' | 'activity' | 'notify' | 'settings'
view         = 'projects' | 'wizard' | 'app'       (仅 hub=workspace 时有效)
panel        = 'progress' | 'workflow' | 'routine' | 'artifact' | 'version'  (仅 view=app)
activeScope  = 'all' | 1..7                         (仅 view=app)
```

`isWorkspace = hub==='workspace'`；切到任一其它 hub → 主区换成该 Hub 全屏列表。

### 2.2 文字版状态机

```
全局图标栏(64px 竖栏,永远可见)
  B  [工作台] | [Skill][Agent][Routine][Cron] | [Connector][Knowledge][Activity] | [Notify][⚙][用]

  hub=workspace
    view=projects  ── 点项目卡 ──► openProject(id)
      coldStep有值 ──► view=wizard, step=coldStep
      否则        ──► view=app
    + 新建项目 ──► view=wizard, step=0

    view=wizard  (step 0..7)
      confirm() step<7 ──► completed[step]=true, go(step+1)
      confirm() step=7 ──► view=app

    view=app  (5 panel × 8 scope 矩阵)
      环节轴:◎全部 | 01..07 七环节
      工具栏:进度 | 工作流 | 定时任务 | 产物 | 版本
      ← 全部项目 ──► view=projects

  hub=skill/agent/routine/cron/connector/knowledge/activity/notify/settings
    → 各自全屏列表(无子导航)
```

---

## 3. 屏幕逐一说明

### 3.1 项目主页 `view=projects` (行 630–668)
- 顶部:品牌 logo + 标题「我的项目」+ 副文案
- 2列网格的项目卡片 `projectRows[]`:
  - 阶段徽章(`运营中`绿 / `冷启动中`橙) + 信号圆点(green/amber/red)
  - 项目名(衬线体) + 描述 + meta(冷启动:"第 N/7 步"; 运营:"N 个环节 · kind")
  - 进度条
- 末位虚线「+ 新建项目」卡 → `newProject()`

---

### 3.2 新建产品向导 `view=wizard` (行 88–627)

**顶部固定**:品牌 + 步骤条(`steps[]`,8个圆点 step0 引子+step1–7,状态 done/current/todo 三色)

**Step 0 · 引子** (行 110–185)
- 大标题 + 介绍文案
- 两栏对比卡:左「传统项目管理 · ~10流程 / 5角色」(有删除线);右「AI时代 Builders 模式」(角色收敛 + 流程精简)
- 4个控制点卡片(2×2网格):01 知道对标谁 / 02 每周在正常演进 / 03 让 agent loop 干活 / 04 目标清晰且难造假
- 按钮「开始创建项目体系 →」→ `start()` → go(1)

**Step 1 · 竞品洞察** (行 187–314)
- 左列 sticky:方法说明 + GATE 红色卡片(「发现→洞察 由人把关」)
- 右列:流程条(界定→采集→结构化→分析 →GATE→ 洞察) + 竞品矩阵 table(产品×维度,●◐○)

**Step 2 · 竞品差距分析** (行 315–357)
- 继续竞品洞察的产物展示(差距矩阵,机会缺口)

**Step 3 · 北极星指标** (行 358–395)
- 可编辑 textarea: `northStar`(北极星指标) + `nsDef`(计算口径)

**Step 4 · 引领指标** (行 396–447)
- `leading[]` 列表,每条 name/def/cur/source/ok;target 可编辑 input

**Step 5 · 滞后指标** (行 448–493)
- `lagging[]` 列表,每条 name/def/cur;target 可编辑 input

**Step 6 · 原型创建** (行 494–561)
- 说明原型即规格的方法论

**Step 7 · 进度管理** (行 562–627)
- `weekPlan[]` 表格(引领指标 × 上周目标/上周实际/本周目标/依据):target 和 driver 可编辑 input
- 本周健康信号选择器:正常演进(green) / 需要关注(amber) / 阻塞(red),更新 `weeklySignal`
- `sigMeta` 描述卡
- 按钮「完成,生成项目看板 →」→ `confirm()` → view=app

---

### 3.3 运营视图 `view=app` (行 670–1925)

**固定顶栏**(行 675–683):← 全部项目 / 项目名 / 阶段徽章

**环节轴**(行 685–702):◎全部环节·总览 + `stageNav[]`(7个环节按钮,含信号点+进行中优化数徽章)

**工具栏**(行 704–717):进度 / 工作流 / 定时任务 / 产物 / 版本 (选中 bg=#23211C)

**左侧栏 230px**(行 722–800):
- `sideTask=true`(工作流panel下选中环节):create/optimize session卡分组
- `overviewHealth=true`(全部环节):「进行中·待你介入」+ 「环节信号·需关注」,归档沉到脚注
- `sideRoutine=true`(定时任务panel):routine feed历史卡
- `sideEmpty`:「该环节暂无记录」

**中央主区**(`showXxx` 标志矩阵):

| panel \ scope | all | stage |
|---|---|---|
| progress | `showProgAll` | `showProgStage` |
| workflow | `showWfLib` | `showWfStage`/`showWfDetail` |
| routine | `showRoutAll` | `showRoutStage` |
| artifact | `showArtAll` | `showArtStage` |
| version | `showVerAll` | `showVerStage` |

- **showProgAll**:总进度条+分段条 / 3统计卡(工作流累计/定时任务运行中/优化中待验收) / 本周计划表(可编辑) / 环节行列表
- **showProgStage**:环节KPI sparklines(wsMetrics) / owns·accept·control卡 / 进度趋势大sparkline + WoW涨跌
- **showWfLib**:静态(沉淀可复用)+ 动态(即用即弃)两栏 / Hub导入卡(WorkflowHub/SkillHub/AgentHub)
- **showWfStage**:环节绑定的工作流卡列表 / 选中session后切到 chat(viewMode=chat) 或产物(viewMode=artifact) / 右栏显示工作流目录树
- **showWfDetail**:工作流解剖(phases流程/prompt/agents/skillList/goal/loop retries&maxIter)
- **showRoutAll**:定时任务按节奏分组(实时/每日/每周)
- **showRoutStage**:本环节routine的watches列表 + feed观测卡 + method(principle/logic/lead/lag/funnel) + metrics sparklines + 「开始优化」按钮
- **showArtAll**:产物画廊(各环节卡片,点进去 → enterStagePanel)
- **showArtStage**:产物画布(stage1=竞品矩阵多视图;stage6=网页应用;其余=文档卡片)
- **showVerAll**:全局 commit timeline + issues(open/closed)
- **showVerStage**:该环节 commits + issues

**右侧栏(可折叠)**:
- `railWide`(展开):工作流模式→工作流目录树(query/goal/agents/skills/loop.yaml);产物模式→文件列表+工具调用日志
- `railStrip`(收起):图标+计数+竖排标题,点击展开

**Chat 模式**(viewMode=chat + activeSessionId):
- session 标题 + 状态标签
- 消息列表(Builder=右暗/Agent=左白 气泡 + label)
- 底部composer textarea + 发送

---

### 3.4 Hub 全屏列表 (行 1927–2195)

| Hub | hub 值 | 主要内容 |
|---|---|---|
| SkillHub | `skill` | 技能卡片 3列网格,name/desc/分类/来源/成熟度/使用次数 |
| AgentHub | `agent` | 智能体卡片 2列,name/role/skills chip/model/runs/采纳率 |
| Routines | `routine` | 例程卡片列表,name/maturity/版本/验收goal/phases/loop/agent/uses |
| CronHub | `cron` | 定时任务表格,任务/频率/上次·下次/项目/状态 |
| Connectors | `connector` | 连接器 3列,name/type/状态/最后同步 |
| Knowledge | `knowledge` | 知识来源列表,name/type/chunks/usedBy/更新时间 |
| Activity | `activity` | 运行记录列表,routine/agent/项目/时长/迭代/结果/时间 |
| Notifications | `notify` | 通知列表,label(类型)/title/detail/时间;未读红点 |
| Settings | `settings` | 账户卡 / 模型与额度 / 通知与审批 toggle / 数据与隐私 toggle |

---

## 4. 完整数据模型

### 4.1 顶层 State

```typescript
interface State {
  view: 'projects' | 'wizard' | 'app'
  hub: 'workspace' | 'skill' | 'agent' | 'routine' | 'cron'
     | 'connector' | 'knowledge' | 'activity' | 'notify' | 'settings'
  panel: 'progress' | 'workflow' | 'routine' | 'artifact' | 'version'
  activeScope: 'all' | number   // 1..7
  step: number                  // 0..7 向导步骤
  completed: Record<number, boolean>
  viewMode: 'artifact' | 'chat' | 'observe'
  railOpen: boolean
  wfDetailId: string | null
  activeSessionId: string | null
  activeProjectId: string
  hubOpen: boolean
  composerText: string
  // 向导填写值
  projectName: string
  benchmark: string
  opportunity: string
  northStar: string
  nsDef: string
  weeklySignal: 'green' | 'amber' | 'red'
  // 领域数据
  projects: Project[]
  opStages: OpStage[]      // 7 条
  workflows: Workflow[]
  hubs: HubCard[]
  leading: LeadingMetric[]
  lagging: LaggingMetric[]
}
```

### 4.2 Project

```typescript
interface Project {
  id: string
  name: string
  kind: string               // '看板 / 网页应用' | '对话应用' | 'Design / 无限画布' ...
  desc: string
  phase: '运营中' | '冷启动中'
  signal: 'green' | 'amber' | 'red'
  progress: number           // 0..100
  envCount?: number          // 运营中时:环节数
  coldStep?: number          // 冷启动中时:当前向导步骤
}
```

### 4.3 OpStage (7个控制点环节)

```typescript
interface OpStage {
  n: number                  // 1..7
  name: string               // '竞品洞察'|'需求导入'|'北极星指标'|'引领指标'|'滞后指标'|'原型创建'|'进度管理'
  phase: '已定稿' | '迭代中' | '监测中' | '持续运行'
  progress: number
  trend: number[]            // 近6周进度值
  metrics: StageMetric[]
  routine: Routine
  method?: StageMethod       // 仅部分环节有(竞品洞察有完整 method)
  owns: string               // 该环节「我负责什么」
  accept: string             // 验收信号
  control: string            // 控制点说明
  create: Session[]
  optimize: Session[]
}

interface StageMetric {
  name: string
  val: string                // 当前值(字符串,如 '60%' '5/7')
  unit: string
  target: string
  trend: number[]
  signal: 'green' | 'amber' | 'red'
}

interface Routine {
  schedule: string           // '每日' | '每周' | '每 30s' ...
  signal: 'green' | 'amber' | 'red'
  watches: string[]          // 监测项名称列表
  feed: FeedItem[]
}

interface FeedItem {
  t: string                  // 时间描述 '今日' '本周' '2min前'
  l: 'info' | 'warn' | 'err'
  x: string                  // 内容文本
}

interface StageMethod {
  principle: string
  logic: Array<{ k: string; d: string; c: string }>   // 证据→发现→洞察链节点
  lead: Array<{ name: string; val: string; unit: string; target: string; note: string }>
  lag: Array<{ name: string; val: string; unit: string; target: string; note: string }>
  funnel: Array<{ label: string; n: number }>
}
```

### 4.4 Session (创建/优化任务)

```typescript
interface Session {
  id: string
  title: string
  snippet: string
  status: '进行中' | '已归档' | '已完成'
  msgs: Array<{
    r: 'b' | 'a'             // b=Builder(右), a=Agent(左)
    x: string
  }>
}
```

### 4.5 Workflow (静态/动态)

```typescript
type Workflow = StaticWorkflow | DynamicWorkflow

interface StaticWorkflow {
  id: string
  kind: 'static'
  name: string
  scope: string              // '跨项目复用' | '本类项目'
  maturity: '成熟' | '打磨中' | '新沉淀'
  version: string            // 'v4' 'v7' ...
  uses: number
  agent: string
  skills: string[]
  note: string
  source: string             // 'WorkflowHub' | '自建'
  stageRef: number           // 关联的环节编号
  prompt: string
  agents: AgentRef[]
  skillList: SkillRef[]
  phases: string[]
  goal: string
  loop: { retries: number; maxIter: number }
}

interface DynamicWorkflow {
  id: string
  kind: 'dynamic'
  name: string
  stage: string
  from: string               // '定时任务触发' | '值班发现'
  note: string
  prompt: string
  agents: AgentRef[]
  skillList: SkillRef[]
  phases: string[]
  goal: string
  loop: { retries: number; maxIter: number }
}

interface AgentRef { name: string; def: string; from: string }
interface SkillRef { name: string; def: string; from: string }
```

### 4.6 HubCard

```typescript
interface HubCard {
  id: 'workflow' | 'skill' | 'agent'
  name: string               // 'WorkflowHub' | 'SkillHub' | 'AgentHub'
  kind: string               // '完整工作流' | '可插拔技能' | '优化好的智能体'
  count: number
  color: string
  desc: string
  items: string[]            // 示例名称列表
}
```

### 4.7 指标

```typescript
interface LeadingMetric {
  name: string
  def: string
  cur: string
  target: string
  source: string
  ok: string                 // '可控 · 难造假'
  lastTarget: string
  hit: boolean
  driver: string             // 本周抓手
}

interface LaggingMetric {
  name: string
  def: string
  cur: string
  target: string
}
```

---

## 5. 视图派生逻辑

`buildApp()` 里几个最关键的派生规则(比模板更重要,Rust 必须复刻):

**健康概览左栏(overviewHealth=true)**:默认**不平铺全部记录**,只露出:
1. `status==='进行中'` 的 session → 「进行中·待你介入」
2. `routine.signal !== 'green'` 的环节 → 「环节信号·需关注」
3. 其余归档记录 → 脚注一行统计「N个环节平稳 · M条已归档」

**健康信号颜色** `sigColor(s)`: green=`#5F7355` / amber=`#B5862F` / red=`#B0503A`

**阶段徽章颜色** `phaseStyle(p)`:
- `'已定稿'` → bg=`#E7EDE2`, color=`#4A5E42`
- `'迭代中'` → bg=`#F2E4DD`, color=`#B0503A`
- `'监测中'` → bg=`#F5ECD6`, color=`#8A6720`
- 其它     → bg=`#EDE8DE`, color=`#6B6557`

**进度颜色**: progress≥100 → `#5F7355`,否则 → `#C5654A`

**工作流目录树(右栏 railWfMode)**:把当前 session 对应的工作流定义渲染成文件树:
`<root>/` → `query.md` + `goal.md` + `agents/<name>.md` + `skills/<name>.md` + `loop.yaml`

**版本面板 commit**:把 `create[]` sessions 映射为 `feat` commits(已合并),`optimize[]` 映射为 `fix/feat` PR(待验收);用 stage 名生成 commit message 前缀(`research:/requirements:/…`)

**Sparkline** 计算(`wsMetrics`):对 `metric.trend[]` 做归一化 → SVG polyline points + area path,带当前端点坐标。`进度趋势大 sparkline` 同理(trX/trY 函数)

---

## 6. 设计系统 token

| Token | 值 |
|---|---|
| 底色(暖纸) | `#EFEBE2` |
| 图标栏底色 | `#E9E3D7` |
| 品牌/clay | `#C5654A` |
| 卡片底色 | `#FBFAF6` / `#F4F0E7` / `#F7F2EC` |
| 边框 | `#E2DCCF` / `#DBD4C5` / `#ECE6DA` |
| 信号 green | `#5F7355` |
| 信号 amber | `#B5862F` |
| 信号 red | `#B0503A` |
| 警示深红 | `#B0503A` / `#A33D29` |
| 文字主色 | `#23211C` |
| 文字次色 | `#57534A` |
| 文字辅色 | `#8C867A` |
| 文字占位 | `#A19B8D` |
| Agent 紫 | `#5A4E7A` / `#4B4660` |
| 标题字体 | `Noto Serif SC` (wght 400/500/600/700) |
| 正文字体 | `Noto Sans SC` (wght 300/400/500/700) |
| 等宽字体 | `JetBrains Mono` (wght 400/500/700) |
| 圆角 | 6px(小) / 8–10px(卡片) / 11–12px(大卡) |
| 阴影 | `0 8px 26px rgba(35,33,28,.08)` |
| 选区色 | `#E7CFC4` |
| 滚动条 | thumb `#D8D1C2`,border `3px solid #EFEBE2` |
| streaming shimmer | `rgba(217,119,87,0)→rgba(247,225,211,.95)→rgba(217,119,87,0)` 1.4s |

**SVG 图标**:左侧图标栏10个图标均为内联 SVG(stroke-based,1.7px,linecap round,linejoin round):工作台(4格)/技能(菱形)/智能体(robot)/例程(三节点图)/定时(时钟)/连接器(哑铃)/知识(书册)/活动(脉搏线)/通知(铃铛)/设置(两横线)

---

## 7. 关键交互清单

| 方法 | 触发 | 效果 |
|---|---|---|
| `newProject()` | + 新建项目卡 | view=wizard, step=0 |
| `openProject(id)` | 项目卡点击 | phase=冷启动→wizard; 否则→app |
| `backToProjects()` | ← 全部项目 | view=projects |
| `go(n)` | 步骤条点击 | step=clamp(0,7,n) |
| `confirm()` | 下一步/完成按钮 | completed[step]=true; step<7→next; step=7→app |
| `prev()` | ← 上一步 | go(step-1) |
| `start()` | 开始创建按钮 | go(1) |
| `setScope(n)` | 环节轴按钮 | activeScope=n |
| `showOverview()` | ◎全部环节 | activeScope='all', panel='progress', wfDetailId=null |
| `setPanel(p)` | 工具栏 tab | panel=p, wfDetailId=null |
| `openWf(id)` | 工作流卡「查看构成」 | wfDetailId=id |
| `closeWf()` | 工作流详情关闭 | wfDetailId=null |
| `selectSession(id)` | 左栏 session 卡 | activeSessionId=id, panel=workflow |
| `openTask(n,id)` | 左栏 session 卡(health overview) | activeScope=n, panel=workflow, activeSessionId=id, viewMode=chat |
| `enterEnv(n)` | 环节行点击 | activeScope=n, panel=workflow, viewMode=artifact |
| `enterStagePanel(n,p)` | routine/artifact 跳转 | activeScope=n, panel=p |
| `setMode(m)` | artifact/chat/observe tab | viewMode=m |
| `toggleRail()` | 右栏收/展 | railOpen=! |
| `clickRoutine()` | 定时任务观测按钮 | viewMode=observe |
| `sendMsg()` | 发送 composer | 向当前 session 的 msgs 追加 Builder消息 + 固定 Agent 回复 |
| `promoteWorkflow(id)` | 「↑ 沉淀为静态」按钮 | 将 dynamic workflow 升级为 static |
| `startOptimizeFromRoutine()` | 「开始优化」按钮 | 向当前环节 optimize[] 追加新 session |
| `setSignalGreen/Amber/Red()` | step7 信号选择 | weeklySignal='green'/'amber'/'red' |
| `toggleHub()` | Hub 导入「展开」 | hubOpen=! |
