//! Component standards — the "template capability" (用户 2026-07-20 拍板:
//! 「我们要提供的就是模板能力…这些标准文件有利于agent创建workflow,规范项目的
//! 基础形态」)。
//!
//! Four generic reference documents describing the canonical shape of the
//! four component kinds a project accumulates: agent / skill / workflow /
//! cron。Written into every new project's owned workspace at creation
//! (`.claude/standards/*.md`, alongside where real agent/skill files live so
//! any agent — including a selection-imported workflow engine like
//! superpowers — finds them by convention), so **anyone creating a new
//! component in this repo, human or AI, has a real, accurate reference for
//! what a well-formed one looks like** — not a hand-wavy convention, the
//! actual persisted schema.
//!
//! Same discipline as [`crate::playbook`]: generic across projects, versioned
//! in code, never per-project content; every field list here is checked
//! against the real `bw-store` schema (including guarded
//! `add_column_if_missing` migrations — schema.sql's `CREATE TABLE` text
//! alone is not always current) — a doc that invents a field that doesn't
//! exist, or omits one that does, is worse than no doc.

/// `.claude/standards/agent-standards.md`
pub const AGENT_STANDARDS_MD: &str = r##"# Agent 标准(BW 组件规范 · agent)

这个项目由 **Builders' Workbench** 管理。它的看板把活(Issue)指派给 **agent**(队友)
真实执行。这份文件定义"什么是一个合格的 agent"——不是习惯,是 `agent` 表的真实字段。
新建一个 agent 前(不管是人手动建,还是 AI 在跑 workflow 时创建),对照这份表。

## 字段:作者填 vs 系统派生

BW 的核心纪律是**健康/战绩永远派生,不能手设**——agent 也一样。创建一个 agent 时,
你只填「身份」四个字段;「战绩」四个字段永远从真实运行记录算出来,新建时必须留空/零值,
**不允许手填一个非零的 runs 或 win_rate**——那是编数据。

| 字段 | 谁填 | 说明 |
|---|---|---|
| `name` | 作者 | 稳定短名,是指派下拉、技能注入、蒸馏溯源的联合键。项目自带五角色用职业称呼\
(如「构建师」);自建 agent 建议同样用「像一个真实职业头衔」的短名,不要用编号。 |
| `role` | 作者 | 一句话定位,格式建议「方法论 · 段执行者」,如「规格驱动交付 · 构建段执行者」。 |
| `skills` | 作者 | JSON 字符串数组,是技能**名字**(join key,见 skill-standards.md 的 `name`),\
不是技能正文——正文在 skill 行里。 |
| `model` | 作者 | 诚实标签,不是路由配置。BW 不按角色钉死模型,真实执行走已配置的 \
`claude` CLI;这个字段就写「claude CLI · 跟随执行器配置」,除非这个 agent 真的绑定了\
不同的执行路径。 |
| `instructions` | 作者 | 站立指令(system-prompt 档)。允许为空(纯目录条目,还没写实操内容);\
有内容就必须是这个 agent 每次真实被指派时都成立的通用指令,不要写只对某一件活\
才对的话——那种话属于 Issue 的 `desc`,不属于 agent。 |
| `maturity` | **系统** | `fresh`(新沉淀)/ `polishing`(打磨中)/ `mature`(成熟)。\
新建一律 `fresh`;没有自动升级规则,人工判断到了再手动改,但不能新建就填 `mature`。 |
| `runs` | **系统派生** | 这个 agent 被指派并真实跑完(含成败)的次数。新建 = 0,永远不手填。 |
| `win_rate` | **系统派生** | 预格式化字符串(如 `"94%"`),从 `runs`/`wins` 算出。新建 = \
空字符串(无数据 = 未知,不是 0%)。 |
| `wins` | **系统派生** | 真实成功次数。新建 = 0。 |

## 项目自带的五角色(参照范例)

每个新项目出生自带五个角色 agent,是这份标准的**活范例**(`bw-core::playbook::role_agents()`
的真实产出,不是文档臆造):

| name | role | skills | instructions 来源 |
|---|---|---|---|
| 原型师 | 假设驱动探索 · 原型段执行者 | evidence-first | `role_preamble(Prototype)` + 项目上下文 |
| 构建师 | 规格驱动交付 · 构建段执行者 | spec-to-tests | `role_preamble(Build)` + 项目上下文 |
| 优化师 | 度量驱动打磨 · 优化段执行者 | baseline-before-touch | `role_preamble(Optimize)` + 项目上下文 |
| 运营推广师 | 增长实验 · 运营推广段执行者 | fresh-eyes-funnel | `role_preamble(Growth)` + 项目上下文 |
| 运维师 | 可靠性工程 SRE · 运维段执行者 | breaking-drill | `role_preamble(Ops)` + 项目上下文 |

## 何时该新建一个 agent,而不是重用这五个

五角色是**通用方法论角色**,不是这个项目专属知识的载体。当你发现某类活需要的不是\
"下一个方法论阶段",而是**这个项目特有的专精**(例如:"这个项目的抓取源专家"\
"这个项目的输出格式校验员"),才新建 agent——把项目特有的知识写进它的 `instructions`,\
而不是硬塞进某个通用角色的通用剧本里。

## 创建前自查清单

1. `name` 是否已被占用(同项目内查一遍指派下拉/组件清单)?
2. `role` 一句话是否讲清楚"这个 agent 何时该被指派"?
3. `instructions` 里有没有混入只对某一件具体活成立的内容(该放 Issue,不该放这里)?
4. 战绩四字段是不是全部留空/零值——**没有例外**?
"##;

/// `.claude/standards/skill-standards.md`
pub const SKILL_STANDARDS_MD: &str = r##"# Skill 标准(BW 组件规范 · skill)

`skill` 是**可执行的方法**,不是收藏夹书签。BW 的技能库分两类出身:项目自带的\
「方法论技能」(如 evidence-first)和从真实完成的 Issue **蒸馏**出来的「复利技能」——\
后者是这个项目独有的、从真活里长出来的经验。这份文件是 `skill` 表的真实字段说明。

## 字段:作者填 vs 系统派生

| 字段 | 谁填 | 说明 |
|---|---|---|
| `name` | 作者 | 稳定短名(kebab-case 建议,如 `evidence-first`),是 workflow 的 \
`SkillRef` 与 agent 的 `skills` 列表的联合键。 |
| `descr` | 作者 | 一句话说清"这个技能是什么、什么时候用",不超过一行。 |
| `category` | 作者 | 归类标签(如「方法论」),自由文本,用于人浏览技能库时分组。 |
| `content` | 作者(或蒸馏时由证据生成) | **正文——真实可执行的指令**,注入到用到它的 \
每个 phase prompt 里。空字符串 = 纯目录条目(占位,还没写实操内容,允许存在但\
`/code-review` 时应被质疑)。蒸馏出来的技能**不允许留空**——一个从真活蒸馏出来\
的技能,正文必须是从那件真活里提炼的具体做法,不是空壳。 |
| `source` | 作者 | `self_built`(自建)等 `HubSource` 枚举值(T2 起与 Workflow \
共用同一套 4 档:Official{official_library}/Adopted/SelfBuilt/WithinSession)——\
标注这条技能的来路,不是编造出处。 |
| `maturity` | **系统** | 同 agent:`fresh` / `polishing` / `mature`。新建一律 `fresh`。 |
| `uses` | **系统派生** | 真实被注入使用的次数。新建 = 0,永不手填。 |
| `distilled_from_issue` | **系统派生**(仅蒸馏路径) | 指回蒸馏它的那个真实已完成 Issue 的\
id。`NULL` = 目录/预置技能(没有真活出处)——**这不是可选的美化字段,是诚实来源标记**,\
自建技能就该是 `NULL`,不要伪造一个 issue id。 |
| `origin_agent` | **系统派生**(仅蒸馏路径) | 完成那件 Issue 的 agent id。同上,伪造=作弊。 |

## 蒸馏一条技能的正确姿势(`DistillSkillFromIssue`)

技能蒸馏只能从**真实、已 Done 的 Issue**发生(命令层校验:Issue 必须存在、有\
assignee、状态为 Done)。`content` 应该回答:「做完这件事之后,下次再遇到同类活,\
我会怎么做得更快/更好?」——具体步骤,不是这件事的复述。参照本仓库真实蒸馏过的例子\
(`同源双分支合并消解法`):开头一句定场,然后 3-6 条编号步骤,每条是一个可执行动作,\
不是感想。

## 项目自带的五条方法论技能(参照范例)

| name | descr |
|---|---|
| evidence-first | 证据先行:只写站得住的内容,标注未核实 |
| spec-to-tests | 规格即测试:每条验收标准落成一个可跑的用例 |
| baseline-before-touch | 先测基线再动手:无基线不优化,删减优先 |
| fresh-eyes-funnel | 新用户漏斗走查:亲手走一遍,只记录真实摩擦 |
| breaking-drill | 破坏性演练:拿坏输入砸,坏行为当场修 |

## 创建前自查清单

1. `content` 是不是空的?如果是,这条技能对现在的 workflow 有实际价值吗——\
还是该等真活跑完再蒸馏?
2. 如果是蒸馏:`distilled_from_issue` 指向的 Issue 真的是 Done 状态吗?
3. `uses` 是不是留空/零——**没有例外**?
"##;

/// `.claude/standards/workflow-standards.md`
pub const WORKFLOW_STANDARDS_MD: &str = r##"# Workflow 标准(BW 组件规范 · workflow)

一个 workflow(`workflow_spec`)是一串**有序 phase**,每个 phase 是一条真实指令,\
交给一个 Executor(真实场景下是 `claude` CLI 子进程)真实执行,产出真实文件与提交。\
这份文件是 `workflow_spec` 表的真实字段说明,也是新建/引入一条 workflow 前的检查表。

## 字段:作者填 vs 系统派生

| 字段 | 谁填 | 说明 |
|---|---|---|
| `name` | 作者 | 人看的名字。 |
| `kind` | 作者(创建时定,之后基本不变) | `Static`(沉淀进库,可复用、可翻旧账)或 \
`Dynamic`(会话内一次性,不进库)。项目贯穿全程的主 workflow 应该是 `Static`。 |
| `prompt` / `goal` | 作者 | 整条 workflow 的共享提示与目标——**只在 `phase_prompts` \
为空时才生效**(旧行为的回落)。 |
| `stage_ref` | 作者 | 1..=5,对应哪个阶段(见 cron-standards.md 同款五阶段);跨阶段/\
不挂靠具体阶段可留空。 |
| `phases` | 作者 | 有序的 phase 名字数组,如 `["计划", "实现", "自检"]`。 |
| `phase_prompts` | 作者 | **与 `phases` 逐项对齐**的真实指令数组——这是 workflow 真正\
的"方法论正文"。每条指令必须可执行、可核验(参照 `bw-core::playbook` 的五阶段真实\
instruction:每条都指明"在工作区做什么真实动作、产出哪个真实文件")。**空数组是合法的\
过渡态**(退回共享 `prompt`),但一条贯穿全程的主 workflow 不该长期停在空数组。 |
| `agents` / `skills` | 作者 | `AgentRef`/`SkillRef` 列表(`{name, def, from}`)——\
声明这条 workflow 期望哪些 agent/skill 参与,是声明性引用,不是运行时强绑定。 |
| `loop_retries` / `loop_max_iter` | 作者 | 单 phase 失败重试次数、单 phase 最大迭代数。 |

## `Static` 独有的子字段(`WorkflowKind::Static`)

| 字段 | 谁填 |
|---|---|
| `maturity` | 系统/人工判断,同 agent/skill 三态 |
| `version` | **系统派生**——每次 `UpdateWorkflowSpec` 自动 +1,旧版本冻结进 \
`workflow_version` 表,可翻旧账 |
| `uses` | **系统派生**——真实被运行的次数 |
| `scope` | 作者——可见范围标注 |
| `source` | 作者——`HubSource`:`Official { official_library }`(BW 自己持续选型引入的\
官方精品库,子标签标具体是哪个库,如 "ecc"/"mattpocock-skills"/"superpowers")/ \
`SelfBuilt`(自建)/ `WithinSession`(会话内)/ `Adopted`(预留:后期用户自选引入\
官方集之外的插件,今天无入口)。**选型引入的官方库 workflow 标 `Official`,不要标\
`SelfBuilt`**——具体来源名放 `official_library` 子标签,`scope` 字段或对应\
`AgentRef`/`SkillRef.from` 可以补充更细的版本/来源信息。 |
| `trigger` | 作者(可选)——如 `/security-review` 这样的斜杠命令触发词。 |

## 一个项目"贯穿全程的主 workflow"该怎么定义

如果这个项目的开发经由一条选型引入的现成方法论驱动(例如 superpowers 的\
「头脑风暴 → 写计划 → 按计划实现 → 评审」),**不要把它的方法论正文抄进这里重写一遍**——\
`phase_prompts` 里对应 phase 直接指向"调用 <来源> 的 <具体技能/命令>",`source` 如实标\
选型来源,`agents_json`/`skills_json` 按需引用。workflow 的价值在于 BW 记得住它\
「跑没跑、跑了几次、多久、改了什么」,方法论本身不必重新发明。

## 创建前自查清单

1. `phase_prompts` 每一条是否都指明了"真实要做什么、产出哪个真实文件"(而不是\
一句空洞的阶段名复述)?
2. 如果这条 workflow 引用了外部方法论(如 superpowers),`source` 与 `agents_json`/\
`skills_json` 有没有如实标注来源,而不是假装自建?
3. `version` / `uses` 是不是新建时保持系统默认——**没有例外**?
"##;

/// `.claude/standards/cron-standards.md`
pub const CRON_STANDARDS_MD: &str = r##"# Cron 标准(BW 组件规范 · cron_task)

`cron_task` 是这个项目的**例行节奏**。BW 的铁律是「定时任务只自动建活,绝不自动完活」——\
这份文件先讲字段,再讲这条铁律具体怎么落在字段上。

## 字段:作者填 vs 系统派生

| 字段 | 谁填 | 说明 |
|---|---|---|
| `name` | 作者 | 人看的任务名。 |
| `target` | 作者 | 自由文本——到点要跑的东西(通常是某条 workflow 的名字);不是硬外键,\
因为目标可能是 hub workflow、也可能是一次 connector 同步。 |
| `schedule` | 作者 | `Cadence`(如 `weekly` / `daily` / `real_time`)。 |
| `project_id` | 作者 | `None` = 全部项目;通常新建时填当前项目。 |
| `mode` | 作者 | **只有两个合法值,别的都是编造**:`run_workflow`(到点跑一条 workflow,\
默认)、`create_issue`(到点只建一张 Issue,不跑任何东西——autopilot 的 no-hijack \
设计)。 |
| `issue_stage` | 作者(仅 `mode=create_issue` 时有意义) | 新建 Issue 挂哪个阶段。 |
| `issue_assignee` | 作者(仅 `mode=create_issue` 时,可选) | 按 agent **名字**指派\
(到点解析,找不到就诚实建一张未指派的 Issue,不是失败)。 |
| `status` | 半系统 | `running` / `normal` / `failed` / `paused`——`paused` 是人工\
介入的唯一手柄;其余三态由真实调度结果驱动,不是随手改的展示字段。 |
| `last_run` / `last_run_at` | **系统派生** | 真实上次触发时间(`last_run` 是显示串,\
`last_run_at` 是拿来跟"到期没到期"比较的真实时钟)。新建都留空/0。 |
| `next_run` | **系统派生/展示** | 由 `schedule` + `last_run_at` 算出的下次预期时间,\
不是手填的承诺。 |

## 「no-hijack」到底是什么意思(字段层面)

`mode=create_issue` 的任务,到点**只会**执行一次 `CreateIssue`(状态永远是 \
`Backlog`/`Todo` 起点)——它没有能力把 Issue 一路推到 `Done`,那条路径在代码里\
根本不存在。如果你想让"到点自动生成一份产出"(比如 aihot 的每日日报),正确设计是:\
cron 到点建一张「生成今日日报」的 Issue,由人(或人配置的自动指派 agent)在看板上\
走 `RunIssue` 真实执行,完成后仍然是人点 `TransitionIssue → Done`。**不存在\
"cron 直接把活标记完成"这条路**——这不是当前实现的疏漏,是故意不做。

## 创建前自查清单

1. `mode` 是不是这两个合法值之一,没有杜撰第三个?
2. 如果 `mode=create_issue`:有没有误期待它会"自动跑完"这件事——它只负责\
"到点提醒有活要干",不负责干活?
3. `last_run` / `last_run_at` / `next_run` 是否留给系统,没有手填一个假的\
"看起来已经跑过"的时间戳?
"##;
