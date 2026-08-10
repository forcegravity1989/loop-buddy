# 22 · vNext 重建计划——以 Connector 组合成熟能力的 OPC 项目控制面

> **横幅(2026-08-10)**:本文是 codex 的重建原案。经逐条代码核验,§1 的诊断全部成立;产品框架(OPC、六段链、控制面+连接器)已被采纳。执行方式由 [`23-opc-stitching-rebuild.md`](23-opc-stitching-rebuild.md) 修订:同样重头构建,但已验证模块整体移植不重写、不建成员体系、不恢复单元测试、嵌入终端随 agentcli 层移植保留。动工前读 23,别照本文开工。
>
> **30 秒导读**：本文是 Builders' Workbench vNext 的产品、领域、架构与执行计划，面向产品负责人、实现者和验收者。它建立在对当前全部 Rust/SQL 代码、`plan/`、`docs/`、`iterations/` 与外部候选组件的重新审计之上。自本文起，vNext 的现行方向以本文为准；旧工程和 00–21 号计划继续保留为实践证据，不再作为新架构的兼容目标。
>
> **一句话定位**：Bench 是以 Rust 为主体的 OPC 项目控制面，通过 Connector 组合成熟工具，让团队按“项目目标 → 五角色责任 → 引领指标 → 当前 Loop → 风险与决策 → 交付证据”推进项目，并把 Agent 的详细执行过程留在上游工具中。

## 1. 为什么重建，而不是继续修旧应用

旧工程证明了很多重要原则，但其中心仍是单人 Builder、Hub、Issue、Workflow、Agent 会话和嵌入终端。继续在原结构上增加成员、长期 Loop、十个并行运行和多种成熟产品接入，会把新的产品中心塞进旧的页面与大对象里。

代码审计得到的事实：

- `bw-app/src/lib.rs` 约 9,872 行，一个命令分发器同时处理项目、指标、Issue、工作流、技能、队友、定时任务、仓平台、PTY 和迁移；
- `Store` 有近百个方法，SQLite 实现同时承担存储、迁移和信号派生；
- `app-desktop` 直接依赖所有内核 crate，并在界面层参与运行调度，不是薄壳；
- 当前只有一个活动运行句柄，不能可靠表达同项目 10+ 并行 Loop；
- 当前没有 Human Member、责任分配、Objective、Risk、Decision 和通用 Evidence；
- 当前 `LoopConfig` 指工作流内部重试，与 Loop Engineering 的长期 Loop 冲突；
- 当前 Connector 只是若干字符串分支、脚本和直接进程调用，不是统一能力层。

因此采用以下策略：

1. 在仓库内建立隔离的 `next/` Cargo workspace；
2. 旧应用保持可运行，作为行为事实源和数据导出源；
3. 新数据库、新 crate、新领域模型，不复制旧 crate；
4. 只逐项提炼经过验证的规则和验收场景；
5. 第一条真实纵向链路跑通前，不追求旧功能对等。

## 2. 从旧工程继承的资产

这些是已经通过真实项目和故障修复验证过的产品资产，必须进入 vNext：

- 五阶段方法论：原型、构建、优化、运营推广、运维；
- 五角色的核心问题、方法步骤、完成清单和反模式；
- 观测只追加，原始事实不覆盖；
- 指标信号只能由观测推导；
- 无数据和数据过期不能显示绿色；
- 业务指标与工程活动指标分开；
- 外部执行结束最多到“待评审”，最终完成必须由人确认；
- 同一件活、运行和交付不能重复结算；
- 项目资产优先于共享默认，其他项目资产不能隐式泄漏；
- 产品定义住在项目仓库，Bench 保存过程账和统一引用；
- Mock、手填、未知、失败必须如实标注；
- 所有界面结论都能从数据库、项目工作区或上游系统独立读回；
- 真实项目演练、独立验收和证据报告优先于 Agent 自述。

以下内容不迁移：

- ProjectRail、SessionRail 和旧七面板布局；
- Workflow、Skill、Agent、Cron、Connector、Knowledge 等一级 Hub；
- Agent Chat、Session/Message 和嵌入式 PTY/xterm；
- 自研 Worktree、Diff、代码编辑器、原型画布；
- 旧 WorkflowSpec/Executor/内部对抗循环；
- 旧 Hub 的播种、导入、兼容和运行时迁移路径；
- Mock 运行、PTY 会话号和聊天状态。

## 3. 新的领域语言

### 3.1 项目成员和五角色

`Member` 只表示真实的人。一个项目可以由一人负责，也可以由两三人共享。

五角色是责任视角，不是五个 Agent：

- 原型师：问题、用户和方案是否真实；
- 构建师：是否形成可交付增量；
- 优化师：质量、效率和复杂度是否改善；
- 运营推广师：价值是否触达用户并形成增长；
- 运维师：可靠性、成本、反馈和复盘是否闭环。

每个角色必须有一个 `Accountable` 负责人，可以有多个 `Collaborator`。同一个人可以承担多个角色；Agent、Skill、Connector 和外部服务不能成为最终责任人。

### 3.2 Loop Engineering

`Operating Loop` 是围绕项目目标长期运行的机制，是 vNext 中 `Loop` 的正式含义。它关联目标、责任角色、引领指标、触发方式、执行 Connector、验收门、风险和证据。

- `Loop`：长期机制；
- `Run`：Loop 的一次执行；
- `RetryCycle`：Provider 内部的重试细节；
- `Transcript`：Provider 内部过程，Bench 默认不保存正文；
- `Attention`：需要人处理的阻塞、审批、评审、未知或偏航。

旧文档中“Loop = 工作流内部重试”的定义从 vNext 起废止。

## 4. 产品信息架构

一级界面只保留四个：

1. **Portfolio**：十个项目、Owner、目标健康度、引领指标偏航、活动 Loop 和首要风险；
2. **Project Cockpit**：严格按六段链条组织项目；
3. **Loop Console**：展示并行 Loop 的规范化状态、评审门、产物与外部跳转，不展示 Agent 对话墙；
4. **Attention Inbox**：跨项目的风险、决策、审批、评审和 Connector 异常。

项目驾驶舱固定顺序：

```text
项目目标
→ 五角色责任
→ 引领指标
→ 当前 Loop
→ 风险与决策
→ 交付证据
```

绿色保持安静；红、黄、Unknown、待审批和待评审才进入 Attention。

## 5. Rust 新架构

```text
next/
├── Cargo.toml
├── AGENTS.md
├── apps/
│   └── bench-desktop
├── crates/
│   ├── bench-domain
│   ├── bench-ports
│   ├── bench-control-plane
│   ├── bench-store-sqlite
│   ├── bench-connector-sdk
│   ├── bench-connector-runtime
│   ├── bench-projections
│   └── bench-legacy-import
└── connectors/
    ├── github-cli
    ├── codehub-cli
    ├── open-design
    ├── multica
    └── orca
```

依赖方向：

```text
bench-domain
    ↑
bench-ports
    ↑
bench-control-plane ← bench-projections
    ↑                         ↑
bench-store-sqlite    bench-connector-runtime
                               ↑
                         connector adapters
                               ↑
                         bench-desktop
```

边界纪律：

- `bench-domain`：纯领域类型与规则，不知道 SQL、Dioxus、CLI、GitHub、Claude、Orca；
- `bench-ports`：按聚合声明仓储、事务、时钟、ID 和外部能力端口；
- `bench-control-plane`：唯一能改变领域状态的层；
- `bench-store-sqlite`：只持久化和查询，不判断健康与完成；
- `bench-connector-sdk`：语言无关的 manifest、请求、响应、事件和 Schema；
- `bench-connector-runtime`：进程、HTTP、MCP、ACP、Daemon、Deep Link、超时、权限和健康检查；
- Connector 不得直接写 Bench 数据库；
- 桌面应用只能调用控制面和查询投影，禁止依赖 SQLx 和具体 Connector；
- 第三方复杂适配器放独立进程，通过 JSONL/JSON-RPC 通信，不加载动态库 ABI。

## 6. 核心领域模型

### 6.1 项目与责任

```text
Project
Objective
Member
ProjectMembership
RoleResponsibility
```

关键规则：

- 每个项目至少一名 Human Member；
- 每个项目只有一个当前主 Objective，可保留历史版本；
- 五角色均须有唯一 Accountable Human；
- 同一角色允许多个 Collaborator；
- 每次修改记录 Actor、时间和来源；
- 五角色不再等于项目阶段，也不等于 Agent Persona。

### 6.2 指标与观测

```text
IndicatorDefinition
├── NorthStar
├── Leading
└── Lagging

Observation → IndicatorEvaluation → Signal
```

关键规则：

- 每条指标必须有定义、目标、窗口、新鲜度策略和采集计划；
- 每个 Observation 关联 Evidence 或明确标记 Manual；
- Connector 只能上报事实，不能直接写 Signal；
- 严格上卷优先级为 `Red > Amber > Unknown > Green`；
- 另行展示数据覆盖率，已知绿色不能掩盖未知责任面；
- Leading Indicator 必须允许声明 Countermetric，防止 Goodhart 式优化。

### 6.3 Loop、风险、决策和证据

```text
OperatingLoop
LoopRun
Risk
Decision
Attention
Evidence
ArtifactRef
Delivery
```

LoopRun 状态：

```text
Queued → Running → WaitingExternal → NeedsReview → Accepted
                    ↘ Blocked
                    ↘ Failed
                    ↘ Cancelled
```

关键规则：

- 上游执行完成只能进入 `NeedsReview`；
- `Accepted` 只能由显式人类动作或被策略认可的人工合并事件进入；
- 同一项目及跨项目至少支持 10 个并行 Run；
- 活动运行按 `RunId` 管理，不允许单一全局运行句柄；
- Connector 崩溃、断连、版本不兼容只产生 Attention/Risk，不写假零；
- Evidence 与 Objective、Indicator、LoopRun、Risk、Decision、Delivery 是多对多关联；
- 原始外部事件保留 checksum、上游 revision、发生时间和接收时间；
- 归一化规则有版本，升级后可从原始事件重放。

## 7. Connector 协议

### 7.1 Connector 的四层身份

```text
ConnectorPackage       上游适配包与 manifest
ConnectorInstallation  本机安装、版本和健康状态
ConnectorBinding       项目绑定、外部项目身份和凭证引用
ConnectorRuntime       某次调用或订阅的运行连接
```

CLI 只是 Transport，不是单独的产品实体。

支持的 Transport：

- 结构化 CLI JSON/JSONL；
- HTTP API；
- MCP；
- ACP；
- 受监管本地 Daemon/Sidecar；
- Deep Link 或外部应用启动。

只有交互式 TUI 的工具只能提供 `ui.open` / `loop.workspace.open`。Bench 不抓取屏幕文本，状态通过 Git、PR、Hook、API 或结构化事件回收。

### 7.2 首版能力

```text
health.probe
project.read
member.list
issue.list / issue.create / issue.update / issue.assign / issue.subscribe
repo.read / repo.pr.open / repo.pr.merge / repo.checks.subscribe
loop.start / loop.cancel / loop.resume / loop.status / loop.subscribe
loop.workspace.open
indicator.collect
design.prototype.create / design.prototype.open / design.critique.run
artifact.list / artifact.open / artifact.export
evidence.collect / evidence.subscribe
approval.list / approval.resolve
ui.open
```

每项能力必须声明：

- invoke、subscribe 或 open；
- 输入/输出/事件 JSON Schema；
- read、write、destructive、external communication 或 local execution 效果等级；
- 是否幂等、是否需要人确认；
- 超时、重试和取消行为；
- Transport 与最低上游版本。

### 7.3 协议铁律

- 所有可重试写操作带 `idempotency_key`；
- stdout 只承载机器协议，stderr 承载诊断；
- 凭证只传 OS Keychain/本地凭证引用，不进入项目仓或普通 payload；
- 未知能力明确返回 `unsupported`，不静默换 Provider；
- Deep Link 只代表打开外部 UI，不能冒充已同步；
- 订阅至少一次投递，支持游标、心跳、重连和事件去重；
- 健康检查分 Installation、Protocol、Authentication、Binding、Capability 五层；
- “能运行 `--version`”只能叫已安装，不能叫已连接；
- Connector 不得直接宣告 Bench 项目健康、Loop 已验收或角色责任已完成。

## 8. 成熟组件的接入顺序

### 第一批：打通三种 Transport

1. **GitHub/CodeHub**：结构化 CLI，复用已经验证的建仓、Issue、PR/MR 和证据读取能力，但改用通用 ExternalRef；
2. **Open Design**：通过 MCP/Headless CLI 接原型、Critique 和 Artifact，不自研画布；
3. **Orca**：先做 Deep Link/外部应用启动，并通过 Git/PR/Hook 收事实，不复制终端、Worktree 和 Diff。

### 第二批：协作和执行 Provider

4. **Multica**：Issue、Member、Squad 和协作 Provider；保持外部服务/CLI 边界，避免商业嵌入许可证风险；
5. **Kandev**：作为机器协议和 10+ 并行执行的对照组，保持独立服务边界并单独评估 AGPL；
6. Orca 与 Kandev 穿刺后只选一个长期主执行面，判断标准是机器协议、游标恢复、Attention、PR/Artifact 归属、版本承诺和许可证。

### 第三批：通用能力

7. **OpenWork**：验证 Daemon/API/SSE、Skills、权限与远程 Worker；
8. **OpenWorker**：验证非代码 OPC、连接器、审批和定时任务；
9. **WorkBuddy**：继续作为项目首页和团队共享体验标杆，有正式 API/SDK 后再作为云 Provider。

正式 Connector 必须保存：上游仓库、版本范围、许可证/NOTICE、协议握手、契约测试、脱敏 fixture、升级 Canary、降级策略。

## 9. 明确禁止继续自研

- PTY、终端模拟器和 Agent TUI；
- Worktree 编排器；
- 代码编辑器、Diff 和浏览器预览；
- 通用 IssueBoard、拖拽 Kanban 和 Jira 替代品；
- 原型画布、设计系统目录和设计 Critique 引擎；
- 通用 Agent Harness、多模型适配和逐条对话中心；
- SaaS Connector/OAuth 市场；
- 通用团队聊天、手机远控和云执行沙箱；
- 独立 Skill 市场；
- 坐标点击式外部应用控制。

Bench 可以拥有 Attention Inbox，但不重建邮件和聊天系统，只汇总 Provider 的风险、审批、评审和决策事件。

## 10. 首条纵向切片

首条切片必须用一个真实仓库证明完整控制链，而不是先重做看板：

1. 创建真实项目和唯一 Objective；
2. 添加两名 Human Member；
3. 五角色全部有 Accountable，其中至少一个角色由两人共同承担；
4. 绑定 GitHub/CodeHub Connector 并读取真实仓身份；
5. 建立 1 个 North Star、至少 2 个 Leading 和 1 个 Lagging 指标；
6. 至少一条指标由 Connector 采集真实 Observation；
7. 导入或关联十个真实 Issue，建立十个并行 LoopRun；
8. 界面只显示 Loop 的规范化状态和外部跳转，不显示十个 Agent 对话；
9. 发现开放 PR 后进入 `NeedsReview` 并产生 Attention；
10. 人 merge/确认后进入 `Accepted`；
11. Issue、PR、commit、CI、原型 Artifact 成为可追溯 Evidence；
12. 断连、陈旧数据和版本不兼容都不会显示绿色。

首条切片只做 Project Cockpit 和 Attention Inbox；Portfolio 和完整 Loop Console 在第二条切片补齐。

## 11. 按依赖顺序执行的工作包

每个工作包由独立 subagent 实现，前一包通过审查和门禁后才启动下一包。每包一个可读的 commit，不使用重复字母代号。

### 工作包：计划与工程宪法

- 本文成为 vNext 单一计划事实源；
- 建立 `next/AGENTS.md`，写明新领域语言、依赖方向、禁止自研和验收纪律；
- 旧计划在 `plan/README.md` 标为旧应用事实源；
- 完成代码、文档和集成审计记录。

完成标准：新实现者只读 vNext 文档即可说清产品中心、所有权边界和第一条切片。

### 工作包：领域内核与 Connector SDK

- 建立独立 Cargo workspace；
- 建立 typed IDs 和核心实体；
- 实现 Human Member 与五角色多对多责任；
- 实现严格信号派生；
- 实现 LoopRun 人工验收状态机；
- 建立 Connector manifest、capability、request/response/event/evidence 协议；
- 生成或校验 JSON Schema；
- 添加领域不变量、属性测试和 Connector 协议测试。

完成标准：核心不依赖 Tokio、SQLx、Dioxus 或任何具体 Provider；外部完成不能直达 Accepted；一个绿色不能覆盖 Unknown。

### 工作包：存储、控制面和投影

- 新建 vNext SQLite schema；
- 按聚合实现 Repository 与明确事务；
- 原始事件、Observation、Evidence 追加存储；
- 实现 Project Cockpit 和 Attention 的查询投影；
- 活动运行用 `HashMap<RunId, ActiveRun>` 管理；
- 用确定性 Stub Connector 跑通两人、五角色、指标、十个并行 Run、评审和证据。

完成标准：重启后所有投影可从事实重建；重复事件不重复结算；Connector 崩溃不污染领域状态。

### 工作包：三种 Transport 穿刺

- 结构化 CLI Driver + GitHub/CodeHub；
- MCP Driver + Open Design；
- Deep Link Driver + Orca；
- 健康检查五层状态；
- 凭证引用、超时、取消、版本不匹配和脱敏 fixture。

完成标准：Rust 核心没有 `match connector_id == ...`；删除任一 Connector 后领域测试仍成立。

### 工作包：桌面驾驶舱

- 建立薄桌面壳；
- Project Cockpit 六段链；
- Attention Inbox；
- 外部应用跳转；
- 真实 DB/API/工作区读回和截图验收。

完成标准：不开 Agent 对话也能判断目标、责任、指标、Loop、风险和交付状态。

### 工作包：真实项目与迁移

- 选择两个真实项目，其中一个由两三人共享；
- 同时运行至少十个 Run；
- 穿刺 Multica 和 Kandev，并决定长期协作/执行 Provider；
- 做一次性旧库导出和 vNext 导入；
- 只迁移 Project、Objective/North Star、Metric、Observation、Issue、Artifact/Evidence、Handoff 和 Scope；
- 不迁移 Chat、PTY、旧 Hub 布局、Mock 运行和运行时迁移标记；
- 输出逐实体计数、checksum 和 Provenance 报告，由人确认后接受。

完成标准：至少一个真实项目完整跑通六段链条后，才允许归档旧应用。

## 12. 验证体系

vNext 撤销旧工程“不写单元测试”的限制，按风险分层验证：

- 领域不变量：单元测试和属性测试；
- Connector：manifest/Schema/幂等/崩溃/超时/非法输出契约测试；
- 控制面：确定性 Stub Connector 的端到端测试；
- 真实 Provider：独立 Smoke Test，不进入常绿离线门禁；
- 桌面：语义验收流、数据库/API/工作区读回、截图和独立复核；
- 迁移：脱敏真实 Golden Fixture、计数、checksum 和 Provenance。

常绿验收必须证明：

- 两人可共享角色，且有唯一 Accountable；
- 同项目十个 Run 不互相覆盖；
- 上游完成不能自动 Accepted；
- 重放同一事件不重复记账；
- 断连、无数据、陈旧和不兼容都不绿色；
- 跨项目资源绝不隐式泄漏；
- 每个 Delivery 和健康结论都能打开原始 Evidence。

## 13. 迁移和发布边界

- vNext 使用全新数据库，旧库不在启动时自动升级；
- 一次性导入器是独立工具，可在迁移结束后删除；
- Connector 协议必须版本协商，不能套用“内部旧路径一律不兼容”的规则；
- 项目仓保存可共享声明和外部项目 ID；本机路径、登录 Profile 和 Token 属于本地覆盖；
- 在首条真实切片验收前，旧应用继续作为可回退的只读参考；
- 不在没有许可证结论的情况下打包 Multica、Kandev 或其他上游代码。

## 14. 本轮执行终点

本轮不是把整个 vNext 一次写完，而是完成三个可验证结果：

1. 本计划和工程宪法落库；
2. `next/` 的领域内核与 Connector SDK 可编译、可测试；
3. 一个确定性的无 UI 纵向示例证明：两名成员、五角色责任、严格 Unknown、十个并行 Run、外部完成待人评审、Evidence 和 Attention 能在同一条链中成立。

达到这三个结果后，再进入真实 Connector 和桌面驾驶舱，避免新工程再次从外壳和 Agent 会话开始。
