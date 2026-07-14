# 完整形态构建笔记（报告素材源 · 全部可对源码核验）

> 本文件是演示报告的静态素材：每条陈述都指向仓库内真实存在的代码/文档。
> 动态证据（真实运行数据）由 `real_demo` 导出的 evidence JSON 提供，两者合成最终报告。

## 1. glm5.2 25 轮迭代的判定（背景）

**保留**（真实资产，本次继续使用）：
- 运行遥测数据基座：`workflow_run` 表 + settle 幂等（iter 1-5，`crates/bw-store`）
- 分析纯函数层：failure_modes / propose_optimizations / workflow_health 等（`crates/bw-core/src/analysis.rs`，13+ 单测）
- 版本快照 / 交棒审计 / 真实调度器（`App::tick_scheduler`）

**判定为失败的部分**（本次修正的靶子）：
1. iter 21「真实场景仿真器」= 确定性合成运行流 —— **本质是 mock 数据**，违背「绝不编造」红线；
2. iter 22 的「端到端闭环」跑在仿真数据上 —— 自我指涉（WorkflowHub 分析 WorkflowHub），
   **真实的 0→1 项目创建一次都没发生**；
3. **真实 agent 执行零次** —— ClaudeCliExecutor 从未被端到端驱动；
4. 25 轮全部堆在「优化元层」，用户要的产品面（五角色五流程管理真实项目）未被演示。

## 2. 完整形态的六个缺口收敛（本次新增代码）

| 缺口 | 收敛 | 代码 |
|---|---|---|
| G1 角色是展示品 | 五角色**可执行剧本**：每阶段方法循环 phase 各带真实指令（角色身份+方法论+反模式+诚实约束+项目上下文注入） | `crates/bw-core/src/playbook.rs`（新，4 单测） |
| G2 phase 无独立 prompt | `WorkflowSpec.phase_prompts` 平行字段；serde default + `add_column_if_missing` 双守卫；空=旧行为字节不变 | `model.rs` / `schema.sql` / `sqlite.rs`（含 workflow_version 冻结） |
| G2b phase 间无接力 | relay baton：上一 phase 真实输出尾部（≤1500 字符）注入下一 phase prompt | `bw-engine/src/lib.rs::relay_tail` + `PhaseNode.prior_summary`（2+1 单测） |
| G3/G4 真实执行未验证 | `Command::RunStagePlaybook`：内核组装剧本（读 ProjectRow + 最新交棒词 + 工作区状态），走 `run_workflow_inner` → `ClaudeCliExecutor` | `bw-app/src/lib.rs`；桌面 ▶运行 同步切换（`op.rs`） |
| G5 证据不回流 | `bw-engine::evidence` 采集器（git 提交数/追踪文件/docs 产物/脏文件，只读命令）+ `Command::RecordCollectedObservation`（Ci/GitPr 来源，**拒绝 Manual 伪装**）—— 度量派生链首个非手填 L0 生产者 | `evidence.rs`（新）+ `bw-app` 新命令 |
| G6 无 headless 驱动 | `real_demo` 指挥器：创建流→五阶段环（真实执行+证据回流+DoD 证据谓词+险交棒如实）→回流闭环→证据 JSON 导出；阶段级幂等（失败会重试，成功才跳过） | `bw-app/examples/real_demo.rs`（新） |

**设计不变量全程守住**：
- 信号仍只能经 `Derived<Signal>` 封口派生（本次零改动派生链）；
- 机器观测与手填观测在类型层分流（`RecordCollectedObservation` 拒绝 `SourceKind::Manual`）；
- DoD 勾选只按证据谓词（`real_demo::dod_evidence`），核实不了的如实不勾 + 险交棒留痕；
- schema 变更走 `add_column_if_missing`（事故教训模式，老库安全）。

## 3. 执行安全设计（真实 agent 的缰绳）

- 预算帽：`--max-budget-usd`（conductor 设 0.75/phase）；
- 权限模式：默认 `acceptEdits` + `--allowedTools Bash`；检测到 `[权限提示]`（真实权限拒绝）
  才升级 `BypassPermissions`（claude_cli.rs 文档中的既定退路），升级动作**响亮记录，绝不静默**；
- 单阶段 30min 超时；超时/失败的 run 如实停留在 `started, never settled` / `Failed`；
- 工作区隔离：每需求一个独立 git 仓库，evidence 采集只读。

## 4. multica 研究（完成 · 全部来自真实源码克隆）

- **来源**：`git clone --depth 1 https://github.com/multica-ai/multica`，
  commit `e1d0d68c`（2026-07-14, "Fix public host root redirect (#5363)"）。
  通读 README、docs/product-overview.md（官方全景文档）、LICENSE、
  server/internal/daemon/prompt.go（438 行）、execenv/ 注入层。
- **是什么**：开源 managed-agents 平台（Go+Next.js+PG17+本地 daemon），把 coding
  agent 变成看板队友——被指派 issue、认领、执行、评论、汇报。支持 14 种 agent CLI。
- **License**：修改版 Apache 2.0（Dify 式）：禁止未授权托管/嵌入商用，内部使用免费。
  BW 仅借鉴概念，零代码复制。
- **完整映射表**：见 `plan/05-complete-form-design.md` §2.2（17 行概念对照）。
- **三大收敛（独立同构的印证）**：
  1. 交棒词：multica MUL-3375 handoff note（assigner's scoping instruction）↔ BW 交棒词注入下一阶段剧本；
  2. 结构化上下文注入：multica runtime_config.md 分节（Agent Identity/Workspace Context/…）↔ BW role_preamble 分节（角色+方法论+反模式+诚实约束+项目上下文）;
  3. 任务状态机 + workdir 隔离 + spawn 层 env 卫生（multica 防覆盖 MULTICA_TOKEN ↔ BW env_remove 会话凭据）。
- **两大差异（BW 的存在理由）**：multica 无度量派生链（无信号封口、无 DoD 证据强制）、
  无生命周期方法论（issue 是平的，没有阶段=角色=方法论的环）。
  multica 管「谁去做」，BW 管「怎么做对」。
- **时间线诚实注**：BW 完整形态设计定稿于读到 multica 源码之前（分类器故障窗口），
  本节是对照印证，非先研究后设计。
