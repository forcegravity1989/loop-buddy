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
| D5 | 旧 `skill.stage_ref` 单值列 | **彻底迁到关联表**（→ D8 追加：不留死列，真删） |
| D6 | 归类怎么到达执行现场 | **目录进 prompt + 正文物化到工作区 `.claude/skills/`** |
| D7 | uses 记账 | **本轮不做**。用户原话：「uses 使用率解析现在不是重点，不用考虑这么复杂，事实上真实解析确实是最好的方案」 |
| D8 | schema 姿态 | 用户追加：「**我们不能无限制扩展表格，不要害怕修改旧表，有需要就大胆重做**」→ 旧列真删而非留死列，新增字段以「净增 0 列」为目标 |
| D9 | bw-standard 8 条 | 用户追加：「**指标类那 8 条也扩挂，口径统一**」→ 指标类技能按统一口径多角色扩挂（§6.1） |

## 3. 数据模型（D8：老表大胆重做，净增一表零列）

### 3.1 旧列真删，不留死列

`skill.stage_ref` 单值列迁移后**真删**，不采用「保留但不读」的保守做法（D8）。已在日常库副本上**实测通过**：

```sql
DROP INDEX idx_skill_stage;               -- 老索引:CREATE INDEX idx_skill_stage ON skill(stage_ref)
ALTER TABLE skill DROP COLUMN stage_ref;  -- SQLite ≥3.35 支持
```

实测记录（2026-08-05，`workbench.db` 副本）：两条语句成功，`PRAGMA table_info(skill)` 读回 `stage_ref` 已消失；`sqlite_master` 查过，全库**无** view/trigger 引用该列。版本余量充足——本机 CLI 3.43.2，`libsqlite3-sys` 0.30.1 更新，均远高于 3.35 门槛。

**顺带纠一个命名坑**：老索引名 `idx_skill_stage` 与新表名 `skill_stage` 撞脸。老索引本轮删除，新索引改叫 `idx_skill_stage_by_stage`，不复用老名字。

### 3.2 净增：一张表、零列

```sql
CREATE TABLE IF NOT EXISTS skill_stage (
  skill_id TEXT NOT NULL,
  stage    INTEGER NOT NULL,          -- 1..=5，与 StageKind::index 同一口径
  PRIMARY KEY (skill_id, stage)
);
CREATE INDEX IF NOT EXISTS idx_skill_stage_by_stage ON skill_stage(stage);

ALTER TABLE skill ADD COLUMN stage_origin TEXT NOT NULL DEFAULT '';
```

`stage_origin ∈ {'', 'table', 'distilled', 'manual'}`——**归类这个动作的出处**。skill 表因此**净增 0 列**（去掉 `stage_ref`、加上 `stage_origin`）。

比上一版设计少一样东西：原方案的 `stage_manual` 布尔列被 `stage_origin` 取代，后者一列同时承担三件事——「判过没有」「谁判的」「Boot 要不要跳过」——且**读侧不必再回查静态表**（上一版的四态判据要查静态表才能区分「已判定」和「没人管」，现在不用）。

### 3.3 迁移守卫

- `add_column_if_missing(pool, "skill", "stage_origin", "TEXT NOT NULL DEFAULT ''")`——既有守卫
- **新增迁移原语** `drop_column_if_present(pool, table, column, dependent_indexes)`：先删依赖索引、再删列，列不存在即 no-op。对称于 `add_column_if_missing`。D8 既然把「大胆重做旧表」定为常态姿态，这个原语就该常备在 `sqlite.rs` 里，而不是这次手写一段一次性代码
- **迁移顺序**（幂等，可重复跑）：① 建 `skill_stage` ② 若 `skill.stage_ref` 列仍在，把非 NULL 值搬进 `skill_stage` 并置 `stage_origin='table'` ③ `drop_column_if_present` ④ 静态表对账
- `crates/bw-store/schema.sql` 与 `sqlite.rs` **必须同改**（CLAUDE.md 双守卫纪律：`CREATE TABLE IF NOT EXISTS` 对存量表不会加列）

### 3.4 归属状态：四态（对已批准三态的修正）

批准设计时说的是**三态**、由关联表行数天然表达（0 行=未归类 / 1–4 行=挂这些阶段 / 5 行=全阶段通用）。**做归类草案时发现这不够用**：`obsidian-vault`（Obsidian 笔记工具）、`scaffold-exercises`（Matt 自己课程仓的练习脚手架）、`writing-skills`（写技能的元技能）这类技能，**不是「没人管」，而是「判过了，它跟项目五阶段无关」**——挂成「全阶段通用」会让每次 run 都物化+列目录，是噪音；留成「未归类」又跟真正没人管的混成一格，正是 D3 要避免的事。

修正为**四态**，判据全在 `skill_stage` + `stage_origin` 两处，读侧零回查：

| 状态 | 判据 | UI 显示 |
|---|---|---|
| 挂 N 个阶段 | `skill_stage` 有 1–4 行 | 对应角色 chip |
| 全阶段通用 | `skill_stage` 有 5 行 | 「全阶段通用」 |
| **不属任何阶段（已判定）** | 0 行，且 `stage_origin ≠ ''` | 「不属任何阶段」 |
| 未归类（Unknown） | 0 行，且 `stage_origin = ''` | 「未归类」 |

## 4. 归类的三条来源

优先级递增，后者覆盖前者：

| 来源 | 覆盖 | 机制 |
|---|---|---|
| **静态归类表** `bw-core`，`name → &[StageKind]` | 65 条随包发行/vendored 技能（本文 §6 全表） | Boot 按名幂等对账，落 `stage_origin='table'`。进 git，可 diff 可 review |
| **蒸馏派生** | 有 `distilled_from_issue` 的技能 | 由出处 Issue 的 `stage` 直接派生，落 `stage_origin='distilled'`——这正是 `distilled_skills_block` 今天已在用的口径，不新造判据 |
| **人工覆盖** | 任何技能、任何库 | SkillHub 编辑面板多选五角色；落 `stage_origin='manual'`，Boot 回填从此整条跳过 |

「不属任何阶段」在静态表里表达为**空集** `&[]`——Boot 照样落 `stage_origin='table'`、`skill_stage` 零行，于是四态里正确读成「已判定」而非「没人管」。

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
- 整块字符上限 **4000**（原型段候选最多，29 条 × ~110 字符 ≈ 3200，留余量）
- 超限按 `uses` 降序截断，并**如实写明**「另有 N 条未列出」——no silent caps

### 5.3 物化

`materialize_stage_skills(workspace, candidates)`：

- 写 `<workspace>/.claude/skills/<name>/SKILL.md` = `skill.content` **原样**。**不**做 `demote_headings`——那是嵌套进 prompt 块才需要的；独立文件必须保持 `#` 开头的 SKILL.md 原形，否则 CLI 认不出
- `skill_file` 支撑文件按其相对路径一并写到同目录
- 同目录写 `.bw-managed`（内含 skill id + rev）作托管标记
- **幂等**：`.bw-managed` 的 rev 与库中一致则整条跳过
- **绝不覆盖用户手写**：同名目录存在但**没有** `.bw-managed` → 整条跳过，并在 run 记录里如实留痕（用户自己的 skill 优先）
- 工作区路径为空（未配置真实工作区的项目）→ no-op，不报错

磁盘量级：候选最多的原型段 29 条，正文合计约 160 KB。可接受（磁盘不是 prompt）。

### 5.4 uses 记账（本轮明确不动）

目录列出的技能**不记 uses**。理由：目录列了 27 条、agent 实际可能只读 2 条，把 27 条都记 uses 是造假，会稀释 uses「真被用了」的语义——而「越用越强」是产品四条主张之一，不能为省事掺水。

**已验证可行的正解，留待后续**：`claude` CLI 的 session jsonl 里真有 skill 加载留痕，实测本次会话的 transcript：

```json
{"type":"tool_use","name":"Skill","input":{"skill":"superpowers:brainstorming", ...}}
```

文件位置：`~/.claude/projects/<slug>/<session-id>.jsonl`，`crates/bw-engine/src/claude_cli.rs:252` 的注释已记着它。run 结束后解析这个文件、只给真被加载的技能记 uses，是「用一次记一次、难造假」的诚实实现。用户明确说本轮不是重点，不做。

## 6. 归类草案全表（65 条）

**读法**：角色列写「原型/构建/优化/运营/运维」= 挂这些阶段；「全阶段通用」= 五个都挂（每个阶段的候选集都含它）；「不属任何阶段」= 已判定，不进任何候选。

### 6.0 指标类技能的统一口径（D9）

上一版把 bw-standard 的指标类技能单挂原型段、却给 mohit 两件挂了三个阶段，**两组口径打架**。统一为一条按「指标的生命周期」切分的规则，四条指标类技能一律照它归：

| 指标生命周期 | 阶段 | 含义 |
|---|---|---|
| **定** | 原型 | 建体系、推北极星、拆驱动树——指标是什么，在原型段定 |
| **用来打磨** | 优化 | 拿指标选打磨对象、看回归——度量驱动打磨的输入 |
| **用来增长** | 运营 | 拿指标设增长实验、看漏斗 |
| **用来守稳** | 运维 | 接真数据源点亮 SLI/健康灯——**仅当**该技能真涉及可观测性接入 |

### 6.1 bw-standard（8 条）— 全部按「实际适用面」扩挂（2026-08-06 拍板推翻本节原判断）

> 本节结论已被用户推翻，见下方新表与说明；原判断原文见 §11 偏差第 3 条，不删，如实记录这是被推翻的判断。

方法论招牌技能不是需要保护的特殊类。用户原话：「方法论我在找业界最佳实践，理论上也是要挂靠的；它的区别只在于官方提供，还是用户上传的」——出处（官方 vs 用户上传）不构成归类规则的例外，五条方法论技能与其它 57 条技能同一把尺子：「它实际在哪些阶段被用」，按各自 desc 里写明的适用面扩挂。指标/对标类仍按 §6.0 口径不变。

| 技能 | 角色 | 理由 |
|---|---|---|
| `evidence-first` | **全阶段通用** | desc 自己写着「或**任何**需要引用事实与数字的产出」——本就是跨阶段品质，与本仓「读回为证」同构 |
| `competitive-analysis` | 原型 · 运营 | 对标名单+各家北极星猜测（原型段起手活）+ **可借鉴打法**（增长的直接输入） |
| `north-star-discovery` | 原型 · 优化 · 运营 | 按 §6.0：推三层指标=定，打磨选对象=优化，增长实验=运营。不涉可观测性接入，无运维 |
| `metrics-binding` | 原型 · 优化 · 运营 · 运维 | 按 §6.0 全四段：它就是**接真数据源点亮 Unknown 健康灯**的活，SLI 接入是 SRE 本职 |
| `spec-to-tests` | 构建 · 优化 | desc：「构建段从 SPEC 落实现之前，**以及评审时逐条核对验收标准**」——评审发生在优化 |
| `baseline-before-touch` | 优化 · 运维 | desc：「优化段动手重构或调性能之前」+ 改线上东西前先量基线是 SRE 本职 |
| `fresh-eyes-funnel` | 优化 · 运营 | desc：「运营推广段诊断漏斗，**或对照验证上线改动**」——后半句是优化 |
| `breaking-drill` | 构建 · 运维 | desc：「运维段事故演练、健康检查脚本落地，**或发布前的稳健性检查**」——发布前属构建 |

它们仍是 `playbook::stage_skills(kind)` 的正本（单件招牌技能展示不变）；本表扩挂的是**注入候选集**，两者是不同的读者——前者答「这个阶段的招牌方法论是哪条」，后者答「这个阶段 run 时该把哪些技能目录列进 prompt」，同一件技能在两处可以有不同的答案。

### 6.2 mohit/pm-claude-skills（2 条）— PR #74 升的基础技能

| 技能 | 角色 | 理由 |
|---|---|---|
| `metrics-framework` | 原型 · 优化 · 运营 | 按 §6.0：从零建指标体系=定，选打磨对象=优化，设增长实验=运营 |
| `metric-tree-builder` | 原型 · 优化 · 运营 | 按 §6.0 与上条同口径。上一版给的是「优化·运营」（理由：北极星已定才轮到它），但拆驱动树本就是标配起手活「找指标」的一部分，仍在原型段——D9 统一后补上原型 |

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
| `obsidian-vault` | 原型 · 运营（2026-08-06 改判，原判「不属任何阶段」见 §11 偏差第 2 条） | 组织笔记与知识：探索期积累素材（原型）+ 内容生产的素材库（运营） |
| `prototype` | 原型 | 造一次性原型回答设计问题 |
| `qa` | 优化 · 运维 | 用户报 bug、agent 建单 = 缺陷收集：打磨（优化）+ 稳态（运维） |
| `request-refactor-plan` | 优化 | 重构计划（小 commit） |
| `research` | 原型 · 运营 | 对高信源调研落 Markdown：技术调研（原型）+ 市场/对标调研（运营） |
| `resolving-merge-conflicts` | 构建 | 解合并冲突 = 交付路上的活 |
| `scaffold-exercises` | 构建 · 运营（2026-08-06 改判，原判「不属任何阶段」见 §11 偏差第 2 条） | 按规格生成练习目录骨架（构建）+ 产出的是教学内容（运营） |
| `setup-matt-pocock-skills` | 构建（2026-08-06 改判，原判「不属任何阶段」见 §11 偏差第 2 条） | 一次性把仓配置成可用形态（issue tracker / 标签词表 / 文档布局），是搭项目基础形态 |
| `setup-pre-commit` | 构建 · 运维 | Husky 门禁：交付门禁（构建）+ 防破坏护栏（运维） |
| `setup-ts-deep-modules` | 优化 | dependency-cruiser 接进仓做深模块 = 架构打磨 |
| `tdd` | 构建 | 测试驱动开发 |
| `teach` | 原型（2026-08-06 改判，原判「不属任何阶段」见 §11 偏差第 2 条） | 学一个新概念是为了做决定——属于假设驱动探索的前置 |
| `to-questionnaire` | 原型 | 把答不了的决策变成问卷 = 探索未知 |
| `to-spec` | 原型 · 构建 | 会话综合成 spec：规格产出（原型）+ 进 tracker（构建） |
| `to-tickets` | 原型 · 构建 | 计划/规格拆成 tracer-bullet 票 |
| `triage` | 构建 · 运维 | issue/PR 分诊状态机：流入分诊（构建）+ 日常维护流转（运维） |
| `ubiquitous-language` | 原型 · 构建 | 同 `domain-modeling`（本仓 2026-07-22 术语沉淀用的就是这条） |
| `wayfinder` | 原型 · 构建 | 超大块工作规划成决策票地图：规划（原型）+ 执行地图（构建） |
| `wizard` | 运维 | 生成交互式 bash 向导走手工流程（第三方配置、一次性迁移）= 运维动作 |
| `writing-beats` | 运营 | 写作三件套之一：素材组装成节奏 |
| `writing-fragments` | 运营 | 写作三件套之一：挖原始碎片 |
| `writing-great-skills` | 优化（2026-08-06 改判，原判「不属任何阶段」见 §11 偏差第 2 条） | 写技能的参考：把做过的事提炼得更简，是求简 |
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
| `writing-skills` | 构建 · 优化（2026-08-06 改判，原判「不属任何阶段」见 §11 偏差第 2 条） | 创建/编辑/验证技能并部署：造出来（构建）+ 提炼求简（优化） |

### 6.5 本地自建 2 条（不进静态表）

代码里的静态表只放随包发行/vendored 的技能；本机产物走另两条来源：

| 技能 | 来源 | 归类 |
|---|---|---|
| `per-source-volume-cap` | **蒸馏派生** | 出处 Issue「裁剪与耗时优化：落实 max_items_per_source」`stage=optimize` → **优化** |
| `keyword-focus-scoring` | 无蒸馏出处 | **未归类**，等 UI 人工补（建议：优化） |

### 6.6 统计

（2026-08-06 拍板：6 条「不属任何阶段」全部挂靠 + 5 条方法论招牌技能同口径扩挂后重算。数字由 `cargo run -p bw-app --example verify_stage_catalog` 从本表本身统计得出，非手数——该 example 的输出即本节数字的来源。）

| 阶段 | 直接挂的条数 | + 全阶段通用 7 条 = 该阶段候选集 |
|---|---|---|
| 原型 | 24 | 31 |
| 构建 | 25 | 32 |
| 优化 | 20 | 27 |
| 运营 | 13 | 20 |
| 运维 | 10 | 17 |

| 特殊档 | 条数 | 名单 |
|---|---|---|
| 全阶段通用 | 7 | `evidence-first` `ask-matt` `claude-handoff` `handoff` `dispatching-parallel-agents` `using-superpowers` `verification-before-completion` |
| 不属任何阶段（已判定） | 0 | （原 6 条已各自挂靠阶段，见 §6.3 对应行；静态表不再生产这一档——该状态本身未废，仍可由人工在 UI 提交空集产生，见 `stage_catalog.rs` 里 `StageOrigin` 上的说明） |

归类后「未归类」仍是 **1 条**（`keyword-focus-scoring`，本轮未动，等人工补）。

## 7. UI

- `SkillEdit` 加 `stages: Option<Vec<StageKind>>`（`None` = 本次编辑不改归类，保持既有行为）
- SkillHub 编辑面板：五角色多选 + 「全阶段通用」/「不属任何阶段」两个快捷；提交时命令层重写 `skill_stage` 并置 `stage_origin='manual'`
- `ui::vm` 的 `RoleFilter::matches` / `role_chip_counts` 改吃 `&[StageKind]`；agent / workflow 侧传单元素切片——三个 Hub 屏共用一个筛选谓词的格局保住，不分叉
- 卡片上显示归属 chip；「未归类」与「不属任何阶段」用不同措辞，不混

## 8. 文档

`crates/bw-core/src/standards.rs` 的 `SKILL_STANDARDS_MD` 字段表补一行阶段归属说明——今天 workflow 标准写了 `stage_ref`、skill 标准整个漏了。按 standards.rs 自己的纪律（「每个字段列表都对着真实 schema 核过」），写明这是多值、四态、以及三条来源的优先级。

## 9. 本轮明确不做

| 不做 | 理由 |
|---|---|
| **agent 侧同病** | 67 条 ECC agent 的 `stage_ref` 全 NULL，结构与 skill 完全同形。留口不做，本文记在案。本轮做完后配方现成（`drop_column_if_present` + `agent_stage` 关联表 + 静态表），照抄一遍即可 |
| **uses 真实解析 transcript** | 用户明确说不是重点。正解已验证可行（§5.4），待后续 |
| **五角色 agent 的 `skills` 列表由归类派生** | 用户选的是注入路线，不是挂 agent 名下 |
| **`workflow_spec.stage_ref` 跟进多值** | 本轮只动 skill；workflow 侧单值不变，`RoleFilter` 用单元素切片兼容 |

## 10. 验收（E2E 读回，不写单测）

按 CLAUDE.md「读回为证」：

```bash
# 1. 归类真的落库，五阶段分布与 §6.6 一致(2026-08-06 拍板后:31/32/27/20/17 含全阶段通用)
sqlite3 <db> "SELECT stage, COUNT(*) FROM skill_stage GROUP BY stage ORDER BY stage;"
# 四态各自可数：未归类应为 1(keyword-focus-scoring)，已判定不属任何阶段应为 0(6 条已各自挂靠，见 §11 偏差第 2 条)
sqlite3 <db> "SELECT CASE WHEN n=0 AND stage_origin='' THEN '未归类'
                          WHEN n=0 THEN '已判定不属任何阶段'
                          WHEN n=5 THEN '全阶段通用' ELSE '挂'||n||'个阶段' END st, COUNT(*)
              FROM (SELECT s.id, s.stage_origin, (SELECT COUNT(*) FROM skill_stage x WHERE x.skill_id=s.id) n FROM skill s)
              GROUP BY st;"

# 2. 老库不崩 + 旧列真删：开 PR#74 之前的备份库，PRAGMA 读回
sqlite3 <老库副本> "PRAGMA table_info(skill);" | grep -c stage_ref     # 必须为 0(已删)
sqlite3 <老库副本> "PRAGMA table_info(skill);" | grep stage_origin      # 必须有
sqlite3 <老库副本> ".tables" | grep skill_stage
sqlite3 <老库副本> "SELECT COUNT(*) FROM skill_stage;"                  # 老库的 8 条 bw-standard 值搬过来了
sqlite3 <老库副本> "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_skill_stage';"  # 必须空(老索引已删)

# 3. 人工覆盖不被 Boot 冲掉：UI 改一条 → 重启 → 读回仍是人工值
sqlite3 <db> "SELECT s.name, s.stage_origin, group_concat(x.stage) FROM skill s LEFT JOIN skill_stage x ON x.skill_id=s.id WHERE s.stage_origin='manual' GROUP BY s.id;"

# 4. 物化真发生且不覆盖用户手写
ls <workspace>/.claude/skills/*/SKILL.md | wc -l
ls <workspace>/.claude/skills/*/.bw-managed | wc -l   # 两数相等 = 全是 BW 写的；差值 = 用户自己的，被正确跳过

# 5. 深链渲染证明
BW_DB=<db> BW_HUB=skill target/debug/builders-workbench   # stderr 见 [BW_OPEN] 且无 panic
```

外加 `/code-review` 过一遍（本仓质量门，不靠测试基线）。

## 11. 偏差与待确认

1. **三态 → 四态**（§3.4）。**已实现**。批准设计时说的是三态、靠关联表行数天然表达。做归类草案时发现 `obsidian-vault` / `scaffold-exercises` / `writing-skills` 这类技能需要「已判定：不属任何阶段」这一档，否则要么污染候选集、要么与「没人管」混淆。**D8 之后这条修正的代价降为零**：`stage_origin` 一列同时承担四态判据，读侧不必回查静态表，且 skill 表净增 0 列。`stage_origin` 落地为 `bw_core::stage_catalog::StageOrigin`，四态判据在真实日常库副本（68 件）读回验证：未归类 1 / 已判定不属任何阶段 6 / 全阶段通用 6 / 挂 1~4 阶段 55（每阶段候选 1=30 / 2=27 / 3=23 / 4=17 / 5=15）。
2. **65 条里有 6 条判为「不属任何阶段」**，等于承认这批外部库里约 9% 的技能对 BW 的五阶段管理体系没有位置。这是诚实结论而非偷懒，但若你认为「都该硬挂一个」，§6.3/§6.4 对应行需要改。
   > **用户 2026-08-06 拍板：全部挂靠。** 原话「都需要挂靠阶段」——静态表里不再有空集条目。上面这条判断被推翻，原文保留不删，如实记录这是被推翻的判断；改判结果见 §6.3/§6.4 对应 6 行、§6.6 统计与 `crates/bw-core/src/stage_catalog.rs`。「已判定不属任何阶段」这一**状态**本身没有废，只是静态表不再生产它，仍可由人工在 UI 提交空集产生。
3. **五条方法论招牌技能仍不扩挂**（§6.1）。D9 只扩了指标/对标类 4 条（`competitive-analysis` `north-star-discovery` `metrics-binding` 及 mohit 两件同口径对齐）。`evidence-first` / `spec-to-tests` / `baseline-before-touch` / `fresh-eyes-funnel` / `breaking-drill` 保持单挂，理由见 §6.1 正文——它们是 `playbook::stage_skills(kind)` 的正本，外扩会让「阶段=角色=方法论」的一一对应失效。若你要求这五条也扩，一句话即可改。
   > **用户 2026-08-06 拍板：方法论同样扩挂。** 原话「方法论我在找业界最佳实践，理论上也是要挂靠的；它的区别只在于官方提供，还是用户上传的」——方法论技能不是需要保护的特殊类，差别只在出处（官方 vs 用户上传），不在归类规则；按与其它技能同一把尺子（它实际在哪些阶段被用）判。上面这条判断被推翻，原文保留不删，如实记录这是被推翻的判断；改判结果见 §6.1 新表与 `crates/bw-core/src/stage_catalog.rs`。`playbook::stage_skills(kind)` 的单件招牌展示不受影响，扩挂的只是注入候选集。
4. **prompt 目录块上限 4000 字符**是按原型段 29 条候选估的（29 × ~110 ≈ 3200）。若后续技能库继续膨胀，这个数要跟着调，或改成按 `uses` 排序取前 N 条 + 如实标注未列出数量。
5. **agent / workflow 侧仍是单值 `stage_ref`**（已知中间态，保留）。本轮做完后，skill 走关联表、agent 与 workflow 走单列，是一个不齐的中间态。D8 的姿态本该一并铲平，但用户此前已把 agent 侧划在本轮之外（§9），且 workflow 的单值今天是**正确且在读**的（不是死列），不属于「旧表债」。§9 已把迁移配方（`drop_column_if_present` + 关联表）备好，后续要拉齐是照抄一遍的事。
6. **`StageOrigin::Legacy` 是执行期新增的第五档，spec 原文只设计了四态**（对应 `stage_origin` 空 / `table` / `distilled` / `manual` 四值）。真删 `skill.stage_ref` 前跑保值搬迁时发现，日常库里有一行（`metrics-render`，`stage_ref=1`）不是本分支静态表回填、也不是蒸馏或人工，而是搬自另一条未合入本分支的产品线（c14932d）——原始归类出处已不可考。若直接照 §3.3 的删列步骤执行，这行数据会随列一起被悄悄抹掉。修复（"Critical 修复"提交 `bdc759a`）新增 `StageOrigin::Legacy`：真删列之前先把这类 `stage_ref IS NOT NULL` 的行原样搬进 `skill_stage`、标 `stage_origin='legacy'`，而不是 `table`（静态表并未为它背书）或 `manual`（没人在 UI 里点过它）。四态判据不受影响——`Legacy` 和 `Table`/`Distilled`/`Manual` 一样，都落在「关联表非空」或「非空 origin」的判据里，不新增第五种 UI 状态，只是让「这行数据的出处诚实」这件事多一档可表达的答案。
