# 05 · 完整形态设计 — multica 融合 + 真实五角色执行

> 目标：把 builders-workbench 从「五阶段生命周期**管理**工作台」推进到「五角色真实**执行**的工作台」——
> 角色不再是静态元数据，而是真实跑在 `claude` CLI 上的 agent；阶段工作流产出真实文件/提交/测试；
> 证据自动回流度量派生链。**绝不 mock 数据，一切实跑。**

---

## 1. 现状盘点（2026-07-13，分支 claude/builders-workbench-multica-0517f4）

### 已有且扎实（保留）

| 层 | 能力 | 证据 |
|---|---|---|
| bw-core | 5阶段=角色=方法论环（核心问题/方法循环/DoD/AI编队/反模式全套静态元数据），Ops→Prototype 回流闭环 | `model.rs` StageKind + 单测 |
| bw-core | 度量派生链 L0→L6：Signal 四态（Unknown 诚实态）、封口 `Derived<Signal>`、append-only observation、`recompute_signals` 唯一写入者 | `derive/` + monitor.rs 测试 |
| bw-core | analysis 纯函数层：运行分析/失败模式/优化建议/节奏自调/健康信号/习惯画像（glm 25轮的可保留资产） | `analysis.rs` 13+ 测试 |
| bw-engine | Executor 冻结契约 + MockExecutor + **ClaudeCliExecutor**（真实 `claude -p` 子进程，权限模式保守，预算帽） | `claude_cli.rs`（端到端未实跑验证） |
| bw-store | SQLite 全量持久化：项目/阶段/指标/观测/会话/工作流/运行遥测/版本快照/交棒审计；OMC+ECC 真实目录种子（443 skills/agents/workflows） | `sqlite.rs` `seed.rs` |
| bw-app | Command/Event 总线；创建流（意图→快问→起草→审阅）；运营命令；真实调度器 `tick_scheduler`；自优化循环 `run_optimization_cycle` | 6 个集成测试文件 |
| app-desktop | Dioxus 0.7 桌面壳，14 屏（墙/创建/运营/9 Hub/设置/通知） | `screens/` |

### glm5.2 25轮迭代的判定

**可保留**：Arc 1-2 的数据基座与分析纯函数（运行遥测、聚合、失败聚类、建议生成）——类型干净、有测试、门禁绿。
**失败之处（本次修正的靶子）**：
1. **iter 21-22 的"闭环演示"靠确定性场景仿真器**——合成运行流，正是「mock 数据」，违背"绝不编造"的立项红线；
2. 25 轮全部在优化"元层"（分析工作流运行的机器），**真实的 0→1 项目创建一次都没发生**；
3. **真实 agent 执行一次都没跑**（ClaudeCliExecutor 在别的分支被造出来，但从未被端到端驱动）；
4. 产出堆在自我指涉上（WorkflowHub 分析 WorkflowHub），用户真正的产品面（项目管理五角色五流程）没有被演示。

### 完整形态的缺口

| # | 缺口 | 现状 | 完整形态 |
|---|---|---|---|
| G1 | **角色是展示品** | `ai_crew()` display-only；执行时无角色系统提示 | 每阶段工作流由该角色的真实 agent 执行（角色 system prompt + 阶段方法论注入） |
| G2 | **phase 无独立 prompt** | `WorkflowSpec.phases: Vec<String>` 只有名字，全部 phase 共用 `spec.prompt` | per-phase 真实指令，注入项目上下文（brief/北极星/上一棒交接/工作区状态） |
| G3 | **创建起草是 Mock** | 起草 run 走 MockExecutor | 起草可走真实执行器（项目配置 workspace 后） |
| G4 | **真实执行未验证** | ClaudeCliExecutor 只有解析层测试 | 本机端到端实跑（本机 claude CLI 已认证） |
| G5 | **产物/证据不回流** | 产物面板占位；观测只有手填 | run 后采集真实证据（文件/git/测试/clippy）→ Ci/GitPr 观测 → 派生链变色 |
| G6 | **无 headless 驱动** | 只有桌面 UI 或测试能驱动 | headless conductor（CLI/example）可全自动跑完整生命周期——也是 multica 类工具的接入面 |

---

## 2. multica 研究（已回填 · 基于 2026-07-14 源码克隆，commit e1d0d68c）

> 研究方式：`git clone --depth 1` 后通读 README / docs/product-overview.md（官方 30 分钟
> 全景文档，自述「每一条描述都能在代码、schema 或 API 里找到对应」）/ LICENSE /
> `server/internal/daemon/prompt.go`（438 行，四类任务提示词全文）/
> `server/internal/daemon/execenv/`（上下文注入层）。
> **时间线诚实注**：本文档 §3-§6 的设计在读到 multica 源码**之前**定稿（分类器故障窗口
> 离线完成），本节是「对照印证」——收敛处为独立同构，差异处为定位使然。

### 2.1 multica 是什么

「Your next 10 hires won't be human.」——开源 managed-agents 平台：把 coding agent
变成看板上的真实队友（被指派 issue、自己认领、写码、发评论、汇报阻塞、更新状态）。
Go 后端（Chi + gorilla/websocket + sqlc）+ Next.js 16 前端 + PostgreSQL 17 + 本地
daemon（3s 轮询认领 / 15s 心跳 / spawn agent CLI 子进程），支持 14 种 agent CLI
（Claude Code、Codex、Cursor Agent…）。Electron 桌面 + iOS 客户端。28 张表。
License：修改版 Apache 2.0（Dify 式附加条件：不得未授权对第三方提供托管/嵌入式商业
服务；组织内部使用免费；前端 LOGO 不可移除）——**BW 只借鉴概念，未复制任何代码**。

### 2.2 概念映射表（multica ↔ BW）

| multica | BW 对应物 | 关系 |
|---|---|---|
| Workspace（多租户容器） | 单用户单库 | 定位差异：BW 单人桌面 |
| Issue（原子工作单元，人/agent 同等可指派） | 无 issue 粒度；工作单元=「项目×阶段」 | **定位差异的根源** |
| Project（issue 的里程碑容器） | Project（五阶段生命周期实体） | 同名不同物：BW 项目自带方法论 |
| Agent（配置化工作者：instructions/env/args/MCP） | 阶段角色（StageKind 元数据+可执行剧本） | multica 通用工作者 vs BW 方法论角色 |
| Runtime+Daemon（分布式轮询认领） | 内嵌 ClaudeCliExecutor（进程内 spawn） | multica 分布式 vs BW 本地内嵌 |
| agent_task_queue（queued→…→settled 状态机） | workflow_run + settle 幂等 | **收敛（独立同构）** |
| task_message 流水 | session_message + 事件总线 | 收敛 |
| Session Resumption（复用 CLI session_id+workdir） | relay baton（≤1500 字摘要注入下一 phase）+ `--no-session-persistence` | 取舍不同：完整上下文恢复 vs 显式可审计交接 |
| Skill（静态 markdown 注入 `.claude/skills/` 等 provider 原生位置） | playbook phase_prompts（内核渲染、索引对齐、版本冻结） | multica 静态知识 vs BW 可执行方法论 |
| Handoff note（MUL-3375：指派人留的 scoping instruction，「treat it as the assigner's scoping instruction」） | 交棒词（HandoffStage note → 注入下一阶段 PlaybookCtx） | **高度收敛** |
| Autopilot（cron/webhook → create_issue/run_only；skip/queue/replace 并发策略） | Cron Hub + App::tick_scheduler | 收敛（BW 已有真实调度器） |
| runtime_config.md 注入（Agent Identity / Workspace Context / Available Commands 分节，见 execenv/runtime_config_sections.go） | role_preamble（角色身份+方法论+反模式+诚实约束+项目上下文分节） | **同一手法**：结构化分节 markdown 注入 |
| daemon env 过滤（防 agent 覆盖 MULTICA_TOKEN） | executor env_remove 会话级凭据（本次新增） | 收敛：spawn 层 env 卫生 |
| task_usage（token 记账） | `--max-budget-usd` 硬顶/phase | 层不同：记账 vs 限额（BW 应补记账） |
| activity_log / Timeline | handoff 审计 + append-only observation | BW 观测直接进派生链，multica 仅展示 |
| — 无度量体系 | **L0→L6 派生链 + `Derived<Signal>` 封口 + DoD 证据谓词** | **BW 独有** |
| — 无生命周期方法论 | **五阶段=角色=方法论环 + 回流** | **BW 独有** |

### 2.3 结论：吸收什么、不吸收什么

**印证/吸收**（对照后确认方向正确或本次落地）：结构化上下文注入、交棒词语义
（连 prompt 措辞都收敛：「scoping instruction，follow it before doing anything broader」
↔ BW 剧本的交棒注入）、任务状态机幂等、工作目录隔离、spawn 层 env 卫生、cron 自动化。

**明确不吸收**（定位使然）：多租户/成员权限、分布式 daemon、issue 看板粒度、多 CLI
provider 抽象、Chat/Inbox。BW 是**单人桌面五阶段方法论工具**（MVP=生命周期管理），
差异化恰在 multica 的空白处：**度量派生链、DoD 证据强制、方法论环**。一句话：
multica 管「谁去做」（指派与执行基础设施），BW 管「怎么做对」（方法论与证据）。

---

## 3. 设计：把五角色变成真实执行

### 3.1 RolePlaybook（bw-core，纯数据）

每个 `StageKind` 一份**可执行剧本**：角色系统提示 + per-phase 指令模板。
模板变量：`{project_name}` `{project_desc}` `{north_star}` `{benchmark}` `{opportunity}`
`{handoff_note}`（上一棒交接词）`{prior_phase_summary}`（本工作流内上一 phase 产出摘要）。

- 剧本是**方法论内容**（通用），不是逐项目编造——与 `StageKind` 静态元数据同性质、同位置。
- `stage_workflow_for(kind, ctx: &PlaybookCtx) -> WorkflowSpec`：渲染出带真实 per-phase prompt 的 spec。
- 兼容：`WorkflowSpec` 增加 `phase_prompts: Vec<String>`（与 `phases` 等长；空=沿用旧行为共用 `prompt`）。
  存储层 `add_column_if_missing` 守卫（事故教训模式）。

### 3.2 Engine 上下文链（bw-engine）

- `PhaseNode.prompt` 改为取 per-phase prompt（无则回退 spec.prompt——旧行为字节不变）。
- **phase 间传递**：`RunCtx` 增加 `prior_output: Option<String>` 或 Engine 在循环里把上一 phase 的
  `PhaseOutput.text` 摘要注入下一 phase prompt（`{prior_phase_summary}`）。真实的接力，而非五次孤立调用。

### 3.3 证据采集器（bw-engine::evidence，只读命令）

run settle 后对 workspace 采集**真实证据**：
- `git log --oneline -n`（新提交数）、`git status --porcelain`（变更文件清单）
- `ls`/文件树 diff（新产物）
- 可选：`cargo test`/`cargo clippy` 结果（若 workspace 是 Rust 项目且 allow_commands）
产出 → `artifact` 登记 + `RecordObservation`（SourceKind::GitPr / Ci）→ `recompute_signals`。
**度量派生链首次吃到非手填的真实来源**（Tier D 的最小兑现）。

### 3.4 headless conductor（bw-cli 或 bw-app example）

`real_demo` 驱动器：创建项目 → 创建流 → 逐阶段（渲染剧本 → RunWorkflow 实跑 → 证据采集 →
观测落库 → DoD → HandoffStage{note}）→ Ops 回流 → 导出全周期证据 JSON。
- 它就是「演示报告」的数据源：一切数字都从 DB 读回，绝不在报告里手写。

### 3.5 多需求并行（multica 映射的最小体现）

两个真实需求 = 两个项目 = 两个 workspace，conductor 可依次/并行驱动——
「一个工作台管理多个真实 agent 项目」正是 multica 与 BW 的重叠带。

---

## 4. 演示需求（真实、小、0→1）

| | 需求 #1 `linkcheck-md` | 需求 #2 `standup-digest` |
|---|---|---|
| 一句话 | 扫描 Markdown 死链的 Rust CLI | git log → 每日站会摘要的 Rust CLI |
| 真实性 | 本仓库 plan/ 文档就能用它检查 | 本仓库 git 历史就是它的真实输入 |
| 五阶段 | 原型（假设：文档死链是真痛点→最小可跑）→构建（spec+完整实现+测试）→优化（clippy/简化/基线）→推广（README/示例/安装路径）→运维（错误处理/CI 脚本/复盘回流） | 同构，验证流程可复制性 |
| 产出 | 真实 crate + git 历史 + 测试 + README | 同 |

每阶段的 DoD、交棒 note、观测值全部来自真实运行结果。

---

## 5. 门禁（不变，每步过）

`cargo fmt --check` · `clippy --workspace --exclude app-desktop -D warnings` · `cargo test` ·
wasm32 keepalive · `guard-kernel-ui-free.sh`。schema 改动必须走 `add_column_if_missing`。
