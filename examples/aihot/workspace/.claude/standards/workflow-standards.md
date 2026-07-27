# Workflow 标准(BW 组件规范 · workflow)

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
| `source` | 作者——`HubSource`:`Omc` / `Ecc` / `SelfBuilt`(自建)/ `WithinSession` / \
`Adopted`(选型引入的外部 workflow 引擎/插件市场,如 superpowers)。\
**选型引入的 workflow 标 `Adopted`,不要标 `SelfBuilt`**——具体来源名(如\
「superpowers@superpowers-dev」)放 `scope` 字段或对应 `AgentRef`/`SkillRef.from`。 |
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
