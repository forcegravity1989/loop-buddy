# 通用 skill 按五角色归类 · 设计

- 日期：2026-08-05
- 分支：`claude/generic-skill-five-roles-f4e016`
- 起因：用户 2026-08-05 拍板「通用的 skill 应该被划分到对应的五角色中」
- 承接：PR #74（plan/19 指标 skill 测评 + mohit 两件升基础技能，Boot 幂等自动引入）

## 一句话

技能库里 88% 的技能今天没有阶段归属，五角色的筛选 chip 形同虚设、外部技能从不被注入；这件事给 65 条随包发行/vendored 的技能建立**多角色归类**（新建 `skill_stage` 关联表），并让归类真正有牙齿——run 时按当前 Issue 的阶段，把本阶段技能的**目录写进 prompt、正文物化到工作区 `.claude/skills/`**，由 `claude` CLI 用原生机制按需加载。

## 1. 现状读回（真实数字，非估计）

来源：日常库副本 `~/Library/Application Support/BuildersWorkbench/workbench.db`（2026-08-05 读回）。

```
skill 总数 65 · stage_ref 为 NULL(通用) 57 条 = 87.7%
  mattpocock-skills  41 条  全部未归类
  superpowers        14 条  全部未归类
  (自建)              2 条  全部未归类
  bw-standard         8 条  已归类(原型 4 / 构建 1 / 优化 1 / 运营 1 / 运维 1)
skill_file 支撑文件 100 条
正文长度：mattpocock 平均 3795 字符(最长 11520) · superpowers 平均 8732 字符(最长 26187) · 无一条空正文
```

三条约束事实，决定了这件事的形状：

1. **字段在、填值的人不在**。`SkillCard.stage_ref: Option<StageKind>`、store 索引、三个 Hub 屏的角色筛选 chip 都是 T7（plan/12 §0/§2）建好的。缺的是写入路径：今天只有 `seed_bw_standard_skills_if_missing` 按名回填 bw-standard 那 8 条，`SkillEdit` 结构体里**没有** stage 字段，UI 也没有归类入口。
2. **归类今天没有牙齿**。真实注入路径 `distilled_skills_block`（`crates/bw-app/src/lib.rs:2371`）只挑**蒸馏出来的**技能，且它的「同阶段优先」读的是**出处 Issue 的 stage**，不是 `skill.stage_ref`。那 57 条外部技能今天**从不被注入**。
3. **正文塞不进 prompt**。注入护栏 `skills_prompt_block` 硬上限 6000 字符（`crates/bw-app/src/lib.rs:2313`），而 superpowers 技能正文平均 8732 字符——**单独一条就撑爆整个预算**。BW 今天不往工作区写技能文件（`scan_project_skills_dir` 只读不写），所以「注入」只有 prompt 正文这一条路。

第 3 条是关键：如果只按阶段挑正文塞 prompt，归类完的 55 条外部技能里绝大多数永远塞不进去，牙齿是假的。

## 2. 已定决策（2026-08-05 用户逐项拍板）

| # | 问题 | 拍板 |
|---|---|---|
| D1 | 归类起什么作用 | **归类 + 接上真实注入**（不止筛选标签） |
| D2 | stage 值由谁填、正本在哪 | **静态归类表（进 git）+ UI 人工覆盖** |
| D3 | 「通用」桶存不存在 | **多态：某阶段 / 明确全阶段通用 / 未归类**——不把「没人管」和「确实通用」混成一格 |
| D4 | 单角色还是多角色 | **多角色，新建关联表** |
| D5 | 旧 `skill.stage_ref` 单值列 | **彻底迁到关联表，列废弃**（sqlite 不删列，保留但不读） |
| D6 | 归类怎么到达执行现场 | **目录进 prompt + 正文物化到工作区 `.claude/skills/`** |
| D7 | uses 记账 | **本轮不做**。用户原话：「uses 使用率解析现在不是重点，不用考虑这么复杂，事实上真实解析确实是最好的方案」 |

## 3. 数据模型

### 3.1 新表

```sql
CREATE TABLE IF NOT EXISTS skill_stage (
  skill_id TEXT NOT NULL,
  stage    INTEGER NOT NULL,          -- 1..=5，与 StageKind::index 同一口径
  PRIMARY KEY (skill_id, stage)
);
CREATE INDEX IF NOT EXISTS idx_skill_stage_stage ON skill_stage(stage);
```

### 3.2 新列（走 `add_column_if_missing` 双守卫）

```sql
ALTER TABLE skill ADD COLUMN stage_manual INTEGER NOT NULL DEFAULT 0;
```

`stage_manual=1` 表示「人工在 UI 里归过类」，Boot 的静态表回填**从此整条跳过这条技能**。这是 D2「人工覆盖优先」的唯一判据。

按 CLAUDE.md 的 schema 迁移双守卫纪律：新表/新列必须**同时**改 `crates/bw-store/schema.sql` 与 `crates/bw-store/src/sqlite.rs` 的 `add_column_if_missing(...)`，否则存量 DB 直接崩。

### 3.3 归属状态：四态（对已批准设计的一处修正）

批准设计时说的是**三态**、由关联表行数天然表达（0 行=未归类 / 1–4 行=挂这些阶段 / 5 行=全阶段通用）。**做归类草案时发现这不够用**：`obsidian-vault`（Obsidian 笔记工具）、`scaffold-exercises`（Matt 自己课程仓的练习脚手架）、`writing-skills`（写技能的元技能）这类技能，**不是「没人管」，而是「判过了，它跟项目五阶段无关」**——把它们挂成「全阶段通用」会让每次 run 都物化+列目录，是噪音；留成「未归类」又跟真正没人管的混成一格，正是 D3 要避免的事。

修正为**四态，仍然零额外存储**——读侧派生：

| 状态 | 判据 | UI 显示 |
|---|---|---|
| 挂 N 个阶段 | `skill_stage` 有 1–4 行 | 对应角色 chip |
| 全阶段通用 | `skill_stage` 有 5 行 | 「全阶段通用」 |
| **不属任何阶段（已判定）** | 0 行，且 `name` 在静态表里（值为空集）**或** `stage_manual=1` | 「不属任何阶段」 |
| 未归类（Unknown） | 0 行，且不在静态表，且 `stage_manual=0` | 「未归类」 |

静态表是纯函数、在 `bw-core` 内，读侧查它零成本。这条修正不新增列、不新增表，只是把「已判定」这个信息从**已有的两处**（静态表命中、`stage_manual`）读出来。

### 3.4 旧列处置

`skill.stage_ref` 迁移后不再被任何读侧代码引用。sqlite 不删列（避免碰老库，与 `add_column_if_missing` 同一保守立场）。Boot 一次性迁移：某 skill 有 `stage_ref` 且 `skill_stage` 无行 → 插一行。实际上 8 条 bw-standard 也全在静态表里，这条迁移只是兜底。

## 4. 归类的三条来源

优先级递增，后者覆盖前者：

| 来源 | 覆盖 | 机制 |
|---|---|---|
| **静态归类表** `bw-core`，`name → &[StageKind]` | 65 条随包发行/vendored 技能（本文 §6 全表） | Boot 按名幂等对账。进 git，可 diff 可 review |
| **蒸馏派生** | 有 `distilled_from_issue` 的技能 | 由出处 Issue 的 `stage` 直接派生——这正是 `distilled_skills_block` 今天已在用的口径，不新造判据 |
| **人工覆盖** | 任何技能、任何库 | SkillHub 编辑面板多选五角色；落库同时置 `stage_manual=1`，Boot 回填从此跳过 |

**归类不触发 T11「编辑即脱离源头」**——`source` 不翻 `SelfBuilt`、`adapted_from` 不写。理由：阶段归属是 BW 自己的组织维度，不是对上游正文的改编；把 mattpocock 的 `tdd` 归到构建段，不该让它失去官方徽记。命令层调用 `SkillEdit` 时 `flip_to_self_built: false`。

新库导入仍诚实落「未归类」，不猜。

## 5. 注入：目录进 prompt，正文物化到工作区

### 5.1 候选集

run 时按当前 Issue 的 `stage`：

```
候选 = { s | s 挂了 stage } ∪ { s | s 挂满五阶段(全阶段通用) }
```

「不属任何阶段」与「未归类」都**不进候选**——诚实：前者判过了不属于，后者没人判过，两者都不该被当成本阶段的推荐技能。

### 5.2 prompt 块

```
## 本阶段可用技能（已物化到 .claude/skills/，按需自行加载）
- tdd — Test-driven development. Use when the user wants to build features or fix bugs test-first…
- implement — Implement a piece of work based on a spec or set of tickets.
…
```

- 每行 `- <name> — <desc 首句，截断至 80 字符>`
- 整块字符上限 **4000**（构建段候选最多，27 条 × ~110 字符 ≈ 3000，留余量）
- 超限按 `uses` 降序截断，并**如实写明**「另有 N 条未列出」——no silent caps

### 5.3 物化

`materialize_stage_skills(workspace, candidates)`：

- 写 `<workspace>/.claude/skills/<name>/SKILL.md` = `skill.content` **原样**。**不**做 `demote_headings`——那是嵌套进 prompt 块才需要的；独立文件必须保持 `#` 开头的 SKILL.md 原形，否则 CLI 认不出
- `skill_file` 支撑文件按其相对路径一并写到同目录
- 同目录写 `.bw-managed`（内含 skill id + rev）作托管标记
- **幂等**：`.bw-managed` 的 rev 与库中一致则整条跳过
- **绝不覆盖用户手写**：同名目录存在但**没有** `.bw-managed` → 整条跳过，并在 run 记录里如实留痕（用户自己的 skill 优先）
- 工作区路径为空（未配置真实工作区的项目）→ no-op，不报错

磁盘量级：构建段 27 条候选，正文合计约 150 KB。可接受（磁盘不是 prompt）。

### 5.4 uses 记账（本轮明确不动）

目录列出的技能**不记 uses**。理由：目录列了 27 条、agent 实际可能只读 2 条，把 27 条都记 uses 是造假，会稀释 uses「真被用了」的语义——而「越用越强」是产品四条主张之一，不能为省事掺水。

**已验证可行的正解，留待后续**：`claude` CLI 的 session jsonl 里真有 skill 加载留痕，实测本次会话的 transcript：

```json
{"type":"tool_use","name":"Skill","input":{"skill":"superpowers:brainstorming", ...}}
```

文件位置：`~/.claude/projects/<slug>/<session-id>.jsonl`，`crates/bw-engine/src/claude_cli.rs:252` 的注释已记着它。run 结束后解析这个文件、只给真被加载的技能记 uses，是「用一次记一次、难造假」的诚实实现。用户明确说本轮不是重点，不做。

## 6. 归类草案全表（65 条）

**读法**：角色列写「原型/构建/优化/运营/运维」= 挂这些阶段；「全阶段通用」= 五个都挂（每个阶段的候选集都含它）；「不属任何阶段」= 已判定，不进任何候选。

### 6.1 bw-standard（8 条）— 保持现状不动

这 8 条是五阶段方法论技能，是角色的**定义性技能**，多角色能力本轮不用在它们身上（外扩会稀释「阶段=角色=方法论」的绑定）。如需扩挂，后续人工在 UI 里点。

| 技能 | 角色 | 理由 |
|---|---|---|
| `evidence-first` | 原型 | 原型段方法论技能，`playbook::stage_skills(Prototype)` 的正本 |
| `competitive-analysis` | 原型 | 创建后标配起手活「竞品分析」 |
| `north-star-discovery` | 原型 | 标配起手活「找指标」，北极星在原型段定 |
| `metrics-binding` | 原型 | 标配起手活「绑数据」，与找指标同期 |
| `spec-to-tests` | 构建 | 构建段方法论技能 |
| `baseline-before-touch` | 优化 | 优化段方法论技能 |
| `fresh-eyes-funnel` | 运营 | 运营推广段方法论技能 |
| `breaking-drill` | 运维 | 运维段方法论技能 |

### 6.2 mohit/pm-claude-skills（2 条）— PR #74 升的基础技能

| 技能 | 角色 | 理由 |
|---|---|---|
| `metrics-framework` | 原型 · 优化 · 运营 | 从零建指标体系：原型段定北极星时起手；优化段拿它选打磨对象；运营段拿它设增长实验 |
| `metric-tree-builder` | 优化 · 运营 | 前提是北极星已定（原型段产出），它做的是往下拆驱动、找杠杆——那是打磨与增长的动作 |

### 6.3 mattpocock-skills（41 条）

| 技能 | 角色 | 理由 |
|---|---|---|
| `ask-matt` | 全阶段通用 | 技能路由器，任何阶段都可能问「该用哪件」 |
| `batch-grill-me` | 原型 | 一轮问完的拷问，打磨计划/设计——设计期动作 |
| `claude-handoff` | 全阶段通用 | 会话交棒给后台 agent，任何阶段都发生 |
| `code-review` | 构建 · 优化 | 交付前评审（构建）+ 质量打磨（优化） |
| `codebase-design` | 原型 · 优化 | 深模块设计词汇：设计接口（原型）+ 改善既有架构（优化） |
| `design-an-interface` | 原型 | 并行子代理生成多种接口设计 = 探索多方案 |
| `diagnosing-bugs` | 优化 · 运维 | 明说是「难 bug 与性能回归」；构建期的普通失败走 tdd/spec-to-tests |
| `domain-modeling` | 原型 · 构建 | 领域建模/统一语言/ADR：建模（原型）+ 术语落码（构建） |
| `edit-article` | 运营 | 编辑打磨文章 = 对外内容 |
| `git-guardrails-claude-code` | 运维 | 阻断危险 git 命令的 hooks = 护栏/防破坏 = 可靠性工程 |
| `grill-me` | 原型 | 拷问计划/设计 |
| `grill-with-docs` | 原型 | 同上，附带产出 ADR/术语表 |
| `grilling` | 原型 | 同上（本仓 2026-07-23 用它拷问过创建流设计） |
| `handoff` | 全阶段通用 | 会话压缩成交接件，任何阶段都发生 |
| `implement` | 构建 | 按 spec/tickets 实现 = 规格驱动交付 |
| `improve-codebase-architecture` | 优化 | 扫描深化机会 + 报告 + 拷问 = 度量驱动打磨 |
| `loop-me` | 原型 | 拷问要建的 workflow 的规格 |
| `migrate-to-shoehorn` | 优化 | TS 测试从 `as` 迁到 shoehorn = 存量改造 |
| `obsidian-vault` | 不属任何阶段 | Obsidian 笔记库工具，与项目五阶段无关。判过了，不是没人管 |
| `prototype` | 原型 | 造一次性原型回答设计问题 |
| `qa` | 优化 · 运维 | 用户报 bug、agent 建单 = 缺陷收集：打磨（优化）+ 稳态（运维） |
| `request-refactor-plan` | 优化 | 重构计划（小 commit） |
| `research` | 原型 · 运营 | 对高信源调研落 Markdown：技术调研（原型）+ 市场/对标调研（运营） |
| `resolving-merge-conflicts` | 构建 | 解合并冲突 = 交付路上的活 |
| `scaffold-exercises` | 不属任何阶段 | Matt 自己课程仓的练习脚手架，对本仓/一般项目无阶段归属 |
| `setup-matt-pocock-skills` | 不属任何阶段 | 一次性安装配置，不是阶段动作 |
| `setup-pre-commit` | 构建 · 运维 | Husky 门禁：交付门禁（构建）+ 防破坏护栏（运维） |
| `setup-ts-deep-modules` | 优化 | dependency-cruiser 接进仓做深模块 = 架构打磨 |
| `tdd` | 构建 | 测试驱动开发 |
| `teach` | 不属任何阶段 | 教用户概念，对象是人不是项目 |
| `to-questionnaire` | 原型 | 把答不了的决策变成问卷 = 探索未知 |
| `to-spec` | 原型 · 构建 | 会话综合成 spec：规格产出（原型）+ 进 tracker（构建） |
| `to-tickets` | 原型 · 构建 | 计划/规格拆成 tracer-bullet 票 |
| `triage` | 构建 · 运维 | issue/PR 分诊状态机：流入分诊（构建）+ 日常维护流转（运维） |
| `ubiquitous-language` | 原型 · 构建 | 同 `domain-modeling`（本仓 2026-07-22 术语沉淀用的就是这条） |
| `wayfinder` | 原型 · 构建 | 超大块工作规划成决策票地图：规划（原型）+ 执行地图（构建） |
| `wizard` | 运维 | 生成交互式 bash 向导走手工流程（第三方配置、一次性迁移）= 运维动作 |
| `writing-beats` | 运营 | 写作三件套之一：素材组装成节奏 |
| `writing-fragments` | 运营 | 写作三件套之一：挖原始碎片 |
| `writing-great-skills` | 不属任何阶段 | 写技能的参考，元技能层，不是项目阶段动作 |
| `writing-shape` | 运营 | 写作三件套之一：素材塑成文章 |

### 6.4 superpowers（14 条）

| 技能 | 角色 | 理由 |
|---|---|---|
| `brainstorming` | 原型 | 创作前探索意图/需求/设计 |
| `dispatching-parallel-agents` | 全阶段通用 | 并行派发独立任务，与阶段无关 |
| `executing-plans` | 构建 | 按 written plan 在独立会话执行 |
| `finishing-a-development-branch` | 构建 | 实现完成后决定合并/PR/清理 |
| `receiving-code-review` | 构建 · 优化 | 接收评审反馈并落改 |
| `requesting-code-review` | 构建 · 优化 | 合并前请评审 |
| `subagent-driven-development` | 构建 | 当前会话内用子代理执行计划 |
| `systematic-debugging` | 构建 · 优化 · 运维 | 明说「any bug, test failure」——构建期测试失败也在内，比 `diagnosing-bugs` 覆盖更宽 |
| `test-driven-development` | 构建 | 测试驱动 |
| `using-git-worktrees` | 构建 | 开工前建隔离工作区 |
| `using-superpowers` | 全阶段通用 | 会话起手，建立如何找/用技能 |
| `verification-before-completion` | 全阶段通用 | 任何阶段声称「完成」之前都该守——与本仓「读回为证」纪律同构 |
| `writing-plans` | 原型 · 构建 | 有 spec 后写实现计划：规格收口（原型）+ 交付计划（构建） |
| `writing-skills` | 不属任何阶段 | 创建/编辑/验证技能，元技能层 |

### 6.5 本地自建 2 条（不进静态表）

代码里的静态表只放随包发行/vendored 的技能；本机产物走另两条来源：

| 技能 | 来源 | 归类 |
|---|---|---|
| `per-source-volume-cap` | **蒸馏派生** | 出处 Issue「裁剪与耗时优化：落实 max_items_per_source」`stage=optimize` → **优化** |
| `keyword-focus-scoring` | 无蒸馏出处 | **未归类**，等 UI 人工补（建议：优化） |

### 6.6 统计

| 阶段 | 直接挂的条数 | + 全阶段通用 6 条 = 该阶段候选集 |
|---|---|---|
| 原型 | 22 | 28 |
| 构建 | 21 | 27 |
| 优化 | 14 | 20 |
| 运营 | 8 | 14 |
| 运维 | 8 | 14 |

| 特殊档 | 条数 | 名单 |
|---|---|---|
| 全阶段通用 | 6 | `ask-matt` `claude-handoff` `handoff` `dispatching-parallel-agents` `using-superpowers` `verification-before-completion` |
| 不属任何阶段（已判定） | 6 | `obsidian-vault` `scaffold-exercises` `setup-matt-pocock-skills` `teach` `writing-great-skills` `writing-skills` |

归类后「未归类」从 57 条降到 **1 条**（`keyword-focus-scoring`，等人工补）。

## 7. UI

- `SkillEdit` 加 `stages: Option<Vec<StageKind>>`（`None` = 本次编辑不改归类，保持既有行为）
- SkillHub 编辑面板：五角色多选 + 「全阶段通用」/「不属任何阶段」两个快捷；提交时命令层落 `skill_stage` 并置 `stage_manual=1`
- `ui::vm` 的 `RoleFilter::matches` / `role_chip_counts` 改吃 `&[StageKind]`；agent / workflow 侧传单元素切片——三个 Hub 屏共用一个筛选谓词的格局保住，不分叉
- 卡片上显示归属 chip；「未归类」与「不属任何阶段」用不同措辞，不混

## 8. 文档

`crates/bw-core/src/standards.rs` 的 `SKILL_STANDARDS_MD` 字段表补一行阶段归属说明——今天 workflow 标准写了 `stage_ref`、skill 标准整个漏了。按 standards.rs 自己的纪律（「每个字段列表都对着真实 schema 核过」），写明这是多值、四态、以及三条来源的优先级。

## 9. 本轮明确不做

| 不做 | 理由 |
|---|---|
| **agent 侧同病** | 67 条 ECC agent 的 `stage_ref` 全 NULL，结构与 skill 完全同形。留口不做，本文记在案 |
| **uses 真实解析 transcript** | 用户明确说不是重点。正解已验证可行（§5.4），待后续 |
| **五角色 agent 的 `skills` 列表由归类派生** | 用户选的是注入路线，不是挂 agent 名下 |
| **`workflow_spec.stage_ref` 跟进多值** | 本轮只动 skill；workflow 侧单值不变，`RoleFilter` 用单元素切片兼容 |

## 10. 验收（E2E 读回，不写单测）

按 CLAUDE.md「读回为证」：

```bash
# 1. 归类真的落库，四态分布正确
sqlite3 <db> "SELECT stage, COUNT(*) FROM skill_stage GROUP BY stage ORDER BY stage;"
sqlite3 <db> "SELECT COUNT(*) FROM skill s WHERE NOT EXISTS(SELECT 1 FROM skill_stage x WHERE x.skill_id=s.id) AND s.stage_manual=0;"

# 2. 老库不崩：开 PR#74 之前的备份库，PRAGMA 读回新表新列
sqlite3 <老库副本> "PRAGMA table_info(skill);" | grep stage_manual
sqlite3 <老库副本> ".tables" | grep skill_stage

# 3. 人工覆盖不被 Boot 冲掉：UI 改一条 → 重启 → 读回仍是人工值
sqlite3 <db> "SELECT s.name, s.stage_manual, group_concat(x.stage) FROM skill s LEFT JOIN skill_stage x ON x.skill_id=s.id WHERE s.stage_manual=1 GROUP BY s.id;"

# 4. 物化真发生且不覆盖用户手写
ls <workspace>/.claude/skills/*/SKILL.md | wc -l
ls <workspace>/.claude/skills/*/.bw-managed | wc -l   # 两数相等 = 全是 BW 写的；差值 = 用户自己的，被正确跳过

# 5. 深链渲染证明
BW_DB=<db> BW_HUB=skill target/debug/builders-workbench   # stderr 见 [BW_OPEN] 且无 panic
```

外加 `/code-review` 过一遍（本仓质量门，不靠测试基线）。

## 11. 偏差与待确认

1. **三态 → 四态**（§3.3）。批准设计时说的是三态、靠关联表行数天然表达。做归类草案时发现 `obsidian-vault` / `scaffold-exercises` / `writing-skills` 这类技能需要「已判定：不属任何阶段」这一档，否则要么污染候选集、要么与「没人管」混淆。修正后仍不新增列/表，只是读侧多查一次静态表。**这是对已批准设计的实质修改，实现前需确认。**
2. **65 条里有 6 条判为「不属任何阶段」**，等于承认这批外部库里约 9% 的技能对 BW 的五阶段管理体系没有位置。这是诚实结论而非偷懒，但若用户认为「都该硬挂一个」，§6.3/§6.4 对应行需要改。
3. **bw-standard 8 条保持单角色不动**（§6.1）。多角色能力本轮只用在外部技能上。若希望 `north-star-discovery` / `metrics-binding` 同时挂优化+运营（它们与 mohit 两件是同类活），需另行拍板。
4. **prompt 目录块上限 4000 字符**是按构建段 27 条候选估的（27 × ~110）。若后续技能库继续膨胀，这个数要跟着调，或改成按 `uses` 排序取前 N 条 + 如实标注未列出数量。
