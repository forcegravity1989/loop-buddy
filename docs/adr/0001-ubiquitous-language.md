# 0001 · 确立领域语言 (CONTEXT.md) 为规范,代码改名列台账后续执行

- 状态:已接受 (2026-07-22) · R1/R2/R3/R4/R6 已执行(2026-07-22,同日)
- 相关:[`CONTEXT.md`](../../CONTEXT.md)、[`plan/07-product-proposition.md`](../../plan/07-product-proposition.md)

## 背景

代码与文档里同一概念多词、同一词多义("黑话太多")。逐词在 `bw-core` 里核证后,确认 6 处真实重载。若放任,新棒次接手时"环""周期""Agent""卡""来源"各指多物,命题语言与代码标识越漂越远。

## 决策

1. **`CONTEXT.md` 是唯一规范词表**——新代码、新文档、界面文案、对话一律用其选定词与 `_Avoid_` 约束。
2. **首棒只沉淀语言,不动代码**;同日用户改口"按术语表进行代码修正",R1/R2/R3/R4/R6 遂在同一 commit 前窗口执行完毕(全仓库机改 + 门禁 + E2E),R5/R7 仍按原计划留待后续(见下)。
3. **规范收敛的关键取舍**(命题语言优先于代码现状):
   - **队友 (Teammate)** 为可指派 AI 人格的规范词;`Agent` 只留给运行时执行体与命题层 "Agent Loop"。
   - **"环/Loop"** 归工作流内部微循环(`LoopConfig`);五阶段不叫"环",其成环性叫"**闭环回流**"属性。依据:用户明确"五角色、五阶段非常清晰;环更多指 workflow 内部对抗式 agent 自闭环的小循环",而 `LoopConfig{retries,max_iter}` 正是该微循环的落点。
   - **时期 (Maturity Period)** / **就绪状态 (Readiness)** 分别接管 `ProjectCycle` / `ProjectPhase`,让"周期""阶段"不再撞。

## 改名台账

| # | 现状标识 | 问题 | 规范词 | 影响面 | 状态 |
|---|---|---|---|---|---|
| R1 | `ProjectCycle`(Explore/Expand/Mature) | "周期/Cycle"与五阶段闭环、工作流循环三方撞 | **时期 / MaturityPeriod** | core + store + app + ui + examples(全仓库) | ✅ 已执行 2026-07-22:类型名全改,变体名(Explore/Expand/Mature)、字段名(`cycle`)、SQL 列名(`cycle`)、TEXT 编码值("explore"/"expand"/"mature")全部保留不动——sqlite 读回确认老库兼容,零迁移需要 |
| R2 | `ProjectPhase`(Running/ColdStart) | "Phase"与"阶段 Stage"撞,其实是状态非阶段 | **就绪状态 / Readiness** | core + store + app + ui | ✅ 已执行 2026-07-22:同 R1,仅类型名改,字段/列名/TEXT 值("running"/"cold_start")不动 |
| R3 | `Role{Builder,Agent}` | "Role"被五角色占用;这里其实是聊天发言方 | **发言方 / Author** | core + store + app + ui | ✅ 已执行 2026-07-22:仅类型名改,变体名 Builder/Agent 保留(与词表"人(Builder)/AI(Agent)"一致)、字段名 `role`、SQL 列 `role`、TEXT 值("builder"/"agent")不动;`playbook.rs`/`workflow_flow.rs` 里两处同形散文("Role playbooks"/"Role classification",指五角色和 workflow 阶段分类,与此枚举无关)核实后未动 |
| R4 | 构建阶段 role_full `"构建师 · Builder"` | 英文 "Builder" 与人类主角 Builder 撞(三义) | 构建师 · Constructor | 展示字符串 | ✅ 已执行 2026-07-22:`model.rs` 的 `role()` 标签改;顺带发现 `seed.rs` 134 处技能 `category: "Builder"`(纯文本字段,同一撞名)一并改为「构建师」,`op.rs` 聊天气泡的字面 `"Builder"`(人类发言方标签,词表明确保留)核实后未动 |
| R5 | `OpStage` | "Op"读作运维,实为"阶段的运行时实例" | StageState / 阶段实例(命名待定) | core + app + ui | ▢ 未执行——规范词未定,非本次范围 |
| R6 | 文档/注释"operating loop""项目是环"单指五阶段 | 与工作流"循环"撞 | 改述为"五阶段闭环回流" | model.rs:74 注释 | ✅ 已执行 2026-07-22(仅 `model.rs:74` 代码注释;`CLAUDE.md`/`plan/*` 是治理文档,不算"代码",未动——`plan/07` §0 命题原文本就有"不许改写"纪律) |
| R7 | `HubCard`/`AgentCard`/`SkillCard`/"卡" 泛化 | "卡"同时指 Issue(活)与目录展示行 | 领域实体用 队友/技能/工作流;"卡"仅指 UI 展示 | ui + 文档 | ▢ 未执行——纯词义约束,无需改代码标识 |

> 台账不含 `Derived<T>` / settle-once / 封口 / recompute_signals —— 这些是实现机制,不进领域语言,保持在代码里。
> 台账也不含 `MetricRole`(Leading/Lagging)、`RoleAgent`、`AgentCard.role: String` —— 这三处也含"role"字样但已被前缀/上下文消歧,核实后确认不与本次改名冲突,未列入。

## 取舍与代价

- **R1/R2/R3/R4/R6 选"只改类型名,不动字段名/SQL 列名/TEXT 编码值"**:三者都通过显式 `xxx_text`/`parse_xxx` 转换函数落库(不是 serde 自动派生),类型的 Rust 标识符从不出现在序列化产物里——所以只改类型名对存量 DB 零风险,不需要 `add_column_if_missing` 或任何迁移守卫。sqlite 读回已验证老库字段值原样("running"/"expand"/"builder"/"agent"),这是零迁移代价换到位改名的关键前提,不是巧合。
- **选命题语言(队友/五角色)为准** 而非代码现状(Agent):产品对用户主张的是"派活给队友",代码得向语言靠,不是反过来。
- **R5/R7 留白不做**:R5 规范词本身未定(不是"该不该改"而是"改成什么"没有答案),抢先拍板一个命名比不改风险更大;R7 是纯词义约束,不需要动标识符,留给未来任何时候顺手处理即可。

## 验证记录(2026-07-22)

R1/R2/R3/R4/R6 执行后跑过:`cargo fmt --all --check` · `cargo clippy --workspace --exclude app-desktop -- -D warnings` · `cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features` · `cargo check -p ui --target wasm32-unknown-unknown` · `./scripts/guard-kernel-ui-free.sh` · `cargo check -p app-desktop` · `cargo check -p bw-app --examples`,全过。E2E:`seed_demo` 示例走完整 `App`/`Command` 路径生成真实 DB,深链 `BW_OPEN=<项目名> BW_PANEL=progress` 启动,stderr 输出 `[BW_OPEN] "智能客服知识库" -> view=App panel=Progress projects=2 issues=0`,进程存活无 panic;sqlite 读回确认 `project.phase`/`project.cycle` 与 `skill.category` 的 TEXT 值符合预期、且新库里裸 `"Builder"` category 已清零。

## 后续

R5、R7 仍待后续棒次;R5 命名一旦拍定,按同一"只改类型名"原则执行。
