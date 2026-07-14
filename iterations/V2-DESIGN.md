# V2 设计综合 — 完整形态 Builders' Workbench = multica 优点 × BW 设计初衷

> 起点的一次纠正:我最初只读 README 就断言「问题在运行时」——太武断。这份设计建立在对
> multica **真实源码**的核验之上:真实的 dashboard 路由(导航 IA)、真实的 handler 操作面、
> 真实的 Issue 状态生命周期(`packages/core/issues/config/status.ts`,经 GitHub API 核验)、
> 真实的 board 组件目录(`packages/views/issues/components/*`)。不是文字猜测。

---

## 1. 两者各自的核心(实地核验,非 README 复述)

### Builders' Workbench(BW)的设计初衷 —— 方法论骨干
- **产品/项目**为顶层单元,在一个**五阶段方法论环**上运营:
  原型(求真)→ 构建(求成)→ 优化(求简)→ 运营推广(求增)→ 运维(求稳)→ 回流原型(线闭成环)。
  每段=一个角色=一套方法论(`StageKind` 静态元数据:核心问题/方法循环/DoD/AI 编队/反模式)。
- **6 层度量派生链**:观测 → 指标信号 → 例行 → 阶段 → 项目,`Signal{Green,Amber,Red,Unknown}`
  **永远派生、永不手设**;「无数据 ≠ 绿」是不可妥协的诚实约束。
- **项目生命周期**(探索/扩张/成熟)决定阶段配比;诚实创建流(意图→快问→起草→审阅)。
- Hub 库:Workflow/Skill/Agent/Connector/Knowledge/Cron;真实 cron 调度器(`tick_scheduler`)。
- UI 无关内核(Command/Event)+ 可热插拔 `Executor` trait(Mock + ClaudeCli)。

### multica 的优点 —— 真实 agent 队友执行
(核验源:`apps/web/app/[workspaceSlug]/(dashboard)/*` 真实路由 + `packages/views/issues/*` 真实组件 + `server/internal/handler/*` 真实操作)
- **Issue = 可分配的工作单元**,真实状态机:
  `backlog → todo → in_progress → in_review → done`(+ 旁支 `blocked`/`cancelled`)。
  带 assignee(人/agent/squad)、**stage**、priority、due/start-date、labels、parent/child 分解、
  comments、execution-log、pull-requests。Board/List/Gantt/Swimlane 多视图。
- **Agent = 真实队友**:profile + runtime 绑定 + provider/model + system prompt + skills +
  permissions + env + work-dir;领任务、执行、报阻塞、发评论、提 PR。
- **Runtime**:本地 daemon + 云;CLI 自动探测(claude/codex/copilot/...);进度 WebSocket 流。
- **Autopilot**:cron/webhook/manual → **自动建 Issue 并路由给 assignee** → 执行 → 失败监控。
- **Skill 复利**:每个完成的解决方案沉淀为可复用 skill(file-based frontmatter,挂到 agent)。
- **Squad**:leader agent + 成员,leader 决定谁来接(稳定路由)。
- **Inbox/Chat/Activity/Members/Usage/Projects**:团队感与可观测面。

## 2. 融合的核心洞察(「完整形态」是什么)

> BW 用**方法论环**思考(这个项目现在在哪个阶段、该怎么演进、健不健康);
> multica 用**分配给 agent 队友的 Issue**思考(这件活谁接、跑到哪了、产出什么)。
>
> **完整工作台 = 把两者焊在一起**:
>
> 一个项目在其五阶段环上运营;**每个阶段里的真实工作,被分解成 Issue,
> 分配给真实的 agent 队友,由他们在 runtime 上执行、回报进度,
> 完成的方案复利成可复用 Skill;项目的度量/信号从这些**真实 Issue 运行结果**派生 —— 不是 mock。**

换句话说:**BW 的五角色(原型师/构建师/优化师/运营推广师/运维师)= multica 的真实 agent 队友**;
阶段方法论告诉你「这段做什么性质的工作」,Issue 是「这件活的实际可分配单元」,由真实 agent 执行。
单看任何一个都不完整:multica 没有方法论骨干(为什么做、在哪个阶段、健不健康答不上);
BW 没有真实执行(没有可分配任务、没有真实 agent)。合起来:**方法论驱动 × agent 执行 × 度量诚实**。

## 3. 完整工作台的 IA(目标态)

```
Workspace
├─ 项目墙(2 列卡 + 新建) · 创建流(意图→快问→起草→审阅)
└─ 项目运营视图
      ├─ 五阶段环轴(活跃阶段高亮 · 配比 mix · DoD · 交棒)
      ├─ Issue 看板 ★[新]  —— 作用域=当前阶段;列=backlog/todo/in_progress/in_review/done
      │     每张卡:assignee(agent 队友)· priority · due · 进度
      └─ 工具栏:进度 | 工作流 | 例行 | 产物 | 版本
AgentHub ★[升级]  真实 agent 队友(五角色 + 自建),绑定 runtime,真实 runs/win-rate
Autopilots ★[演进自 CronHub]  cron/webhook → 建 Issue → 路由 agent
SkillHub ★[升级]  skill 从真实完成的 Issue 复利而来(带 provenance)
Activity/Inbox ★  真实事件流(Issue/Run/Handoff)
Connectors · Knowledge · Settings
```
(★ = 相对现状的新增/升级;非 ★ 的项沿用现状。Squad 列为「设计已留口、本轮不建」。)

## 4. 本轮真正要建 vs 仅设计不建(诚实边界)

| 建(真实代码 + 真实测试 + 真实 commit) | 仅设计不建(留口,不假装) |
|---|---|
| **R1 · Issue 层**:可分配工作单元 + 状态机 + 阶段作用域 + agent assignee | Squad(leader 路由):模型留 stage/assignee 口,不建队长委派逻辑 |
| **R2 · Skill 复利**:完成的 Issue 沉淀为带 provenance 的 Skill | 云 Runtime / WebSocket 实时流:Executor trait 已在,runtime 仍本地 |
| 真实端到端演示:建 Issue → 分配 agent → 真实子 agent 执行 → Done → 复利 skill → 真实度量 | Gantt/Swimlane 视图:本轮只 Board/List;多视图留口 |

> 诚实约束不变:绝不编造;无数据 ≠ 绿;破坏性永不自动。

## 5. 两个真实需求(0→1,作样例跑通五角色五流程)

### R1 · Issue 层(可分配工作单元 × 阶段作用域 × agent 队友)
- **原型师假设**:BW 的「工作」单元是 Workflow(可复用模板)和 Session(对话),没有「**这件活谁接、跑到哪一步**」的可分配单元 —— 这是与 multica 最大的 IA 缺口。没有它,五阶段环是空的:「构建段该做什么」没有落点。
- **DoD**:`Issue{项目·阶段·标题·描述·状态·assignee·优先级·时间}` 落库(迁移守卫);状态机
  `Backlog→Todo→InProgress→InReview→Done`(+`Blocked`/`Cancelled`);可按 项目/阶段/状态 列出;
  可分配给 agent;幂等建/转。真实单测覆盖状态转移 + 迁移守卫 + 列表过滤。

### R2 · Skill 复利(完成的 Issue → 可复用 Skill,带 provenance)
- **原型师假设**:BW 的 Skill 是静态目录条目,不从真实工作里长出来 —— 与 multica「每个解决方案复利成 skill」相反。复利链断裂:做了真活,经验没沉淀。
- **DoD**:一个 `Done` 的 Issue(有真实 assignee + 真实方案)可 `DistillSkill` 成 SkillHub 条目,
  带 provenance(`distilled_from_issue` + `origin_agent`);复利是加法(不覆盖既有 skill);
  真实单测覆盖 provenance 正确 + 幂等 + 加法不破坏。

## 6.5 用 multica 真实源校验设计(sonnet5 子 agent 读了 models.go + SQL 迁移)

子 agent 读了 multica 的 `server/pkg/db/generated/models.go` + `server/migrations/*.sql` + handler/service,
逐字段核实。结论:**上面的融合设计被真实源验证,而非臆测**。要点:

- **`issue.stage`(nullable INT4,迁移 123)在 multica 真实存在** —— 我把 Issue 作用域到 BW 阶段,
  不是发明,是 multica 已有的字段。区别:multica 的 stage 是一个裸整数;BW 的 `StageKind` 是带
  角色/方法论/DoD/反模式的富类型 —— **BW 在此处增强 multica**(裸 stage → 方法论 stage)。
- **真实 Issue 状态机 = `backlog/todo/in_progress/in_review/done` + `blocked/cancelled`**
  (与我从 `status.ts` 核验的一致)。**`backlog` 不是第一列那么简单 —— 它是「抑制触发」的停车场**:
  把 issue 分配进 backlog 不会启动 agent 运行;从 backlog 移出才是独立的触发源。R1 照此建模。
- **multica 的 Skill 是手工创建/挂载的,不从完成的工作自动捕获**(子 agent 明确:「no automatic
  skill-capture from completed tasks」)。所以 **R2(完成的 Issue → 带 provenance 的 Skill)
  是 BW 相对 multica 的真实增量**,不是抄。诚实标注。
- 真实任务队列是 **pull-based**(daemon 轮询 `/tasks/claim`,`queued→dispatched→running→completed`),
  进度经 WebSocket 流。本轮不建 daemon/WebSocket(留口),但**任务记录的形态**
  (`queued→running→done` + 真实 duration/status)与 BW 已有的 `workflow_run` 同构 —— R1 的 Issue
  完成态复用这条已有真实记录通道。
- multica 的 `Autopilot` 两态(`create_issue` | `run_only`)、`Squad`(leader 即执行者)、
  `Chat`(每页浮窗,直接对话某 agent)、`Inbox`(notification feed:action_required/attention/info)、
  `Project`(issue 的容器/epic,status: planned/in_progress/paused/completed/cancelled)——
  全部记入「目标态 IA」,本轮只建 R1/R2,其余留口不假装。

## 6.6 R1 状态机(照 multica 真实语义)

```
Backlog(停车场·分配不触发)→ Todo → InProgress → InReview → Done
                                   ↑                ↓
                                 Blocked        Cancelled(终态)
```
- `Backlog`:分配到此**不**自动触发运行(multica 真实语义;BW 本轮如实标注,运行触发仍由显式
  「▶ 运行」/cron 驱动,不越界假装自动)。
- 终态:`Done` / `Cancelled` / `Blocked`(Blocked 可恢复)。
- 转移可由人或 agent 发起;本轮 agent 完成真实工程后,如实把 Issue 推到 `InReview`/`Done`。

## 6. 真实执行基底(为什么不是 mock)
- `claude -p` 在本机经 GLM 网关,探测得 **529 过载** —— 不稳,**演示不依赖它**。
- **真实执行 = sonnet5 子 agent 做真实工程**(写真实 Rust、跑真实 `cargo` 门禁),BW store 如实记账。
- 这比「Rust 进程 shell-out `claude -p` 生成文本」更贴合 multica「agent as teammate」的本意,也更可控、更诚实地「实跑」。
