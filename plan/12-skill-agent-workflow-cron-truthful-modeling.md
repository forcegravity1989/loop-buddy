# 12 · Skill / Agent / Workflow / Cron 真实建模与展示(v1)

> 2026-07-23 grilling 会话逐条拍板(用户逐问确认,22 条共识,本文如实记录)。
> 起因(用户原话大意):**当前 Skill 的展示是错的**——Skill 是一个标准组件,本体是一个
> 文件夹(SKILL.md + references + scripts + …),我们只存了一个扁平 content 字段;
> Agent、Cron 同理都应遵循"如实呈现真实结构"的原则;Workflow 不同,它是 n 个 skill、
> n 个 agent 串联的流程,且包含对抗式评审 agent 的打回循环(loop)。
> **自足接棒件**:行号是写作日锚点,漂移以源码为准;偏差写进 commit message,不改设计决定。

---

## 0. 概念澄清(先说清楚词,再动代码)

1. **"Workflow Cron" 的 Cron 就是定时任务**,不是第四个新实体。对标 Claude Code 里的
   `/loop`、`/scheduler`:核心是提供定时执行能力,背后调用某个 Skill 或某段 Prompt,
   能力依托于 CLI 组件(现用 claude CLI)。
2. **五个阶段角色(原型师/构建师/优化师/运营推广师/运维师)不是 "Agent" 概念**,
   它们对应的是**用户/职责**概念——Workbench 面向这五类角色,今天用户一人承接多个角色,
   能否承接取决于其 Workflow 和 Agent 是否覆盖该角色的职责。落到代码:它们是
   **分类维度**——Skill/Agent/Workflow 三类实体都按这五个角色归类(见 §2/§3 的
   `stage_ref`)。`role_agents()` 产出的 5 条 agent 行**不动**:表结构、RunIssue 驱动、
   win_rate 记账全部保持,只做词汇澄清。
3. **"Agent" = AGENT.md**:一个明确了角色与流程的 markdown 定义文件
   (frontmatter `name`/`description`/`tools`/`model` + 指令正文),即 Claude Code 的
   subagent 定义格式。
4. **"Agent CLI" 是第三层概念**:Cursor / Claude Code / Codex 这类"智能体框架/CLI"。
   本次纳入范围:Agent 要能声明"用哪个 Agent CLI 执行",且真实路由(见 §3)。

---

## 1. 预置库换血:ECC/OMC 目录退场,真实源文件进场

**拍板**:取消 ECC/OMC 的 skill 目录预置(271+73 条"目录引用"空壳,`seed.rs` 里只有
名字和描述,没有正文——徒有其形)。换成两个**本机已安装、拿得到真实源文件**的库:

| 库 | 真实源(写作日路径) | 数量 |
|---|---|---|
| mattpocock-skills 1.2.0 | `~/.claude/plugins/cache/mattpocock/mattpocock-skills/1.2.0/skills/**`(plugin.json 列 22 个) | 22 skills |
| superpowers 6.1.1 | `~/.claude/plugins/cache/superpowers-dev/superpowers/6.1.1/skills/**` | 14 skills |

每个 skill 的真实形态(实测):一个文件夹,`SKILL.md`(frontmatter `name`+`description`
+正文)为必备,其余文件**任意结构**——mattpocock 每个 skill 带 `agents/openai.yaml`
(跨平台调用包装,不是 subagent 人格),部分带 `references/`、`scripts/`、平铺的
`GLOSSARY.md`/`mocking.md` 等;superpowers 部分带 `scripts/`、`references/`、`examples/`。
官方规范(agentskills.io/specification)只强制 `name`+`description` 两个 frontmatter 字段,
支撑文件 "only if needed",**没有固定子目录分类**——所以我们不预设分类,如实存路径。

**Agent 一侧**:mattpocock/superpowers 两库均无独立的真实 AGENT.md。真实来源改为
**ECC 仓库的 agents/ 目录**(github.com/affaan-m/everything-claude-code,MIT,23 万+ star):
**67 个真实 subagent .md 文件**,每个 2–14KB,frontmatter 统一为
`name`/`description`/`tools: ["Read",…]`/`model: sonnet|opus` + 指令正文。
写作日已经 GitHub API 逐文件拉齐 67 个校验过格式(会话 scratchpad 临时目录,实施时需
vendor 进仓库或重拉;来源 URL 即上,`agents/` 目录合计约 500KB,不 clone 整仓 39.7MB)。
**OMC 的 37 条 agent 目录引用没有真实源支撑,彻底删除。**

**Agent Hub 最终构成** = 5 个内置阶段角色(SelfBuilt)+ 67 个 ECC 真实 Agent(Official/ecc)。
**Skill Hub 最终构成** = 36 个真实导入(Official/mattpocock-skills、Official/superpowers)
+ 用户自建/蒸馏(SelfBuilt)。

---

## 2. Skill:从扁平字段到真实文件夹

**现状**:`SkillCard`(`crates/bw-core/src/model.rs:976`)只有一个 `content: String`;
展示时正文塞一个 `<pre>`,子文件概念不存在——**与 skill 的真实形态不符,这就是"展示是错的"**。

**改法**:

- 新增 `skill_file` 子表,不预设分类(去掉 kind,UI 按扩展名现推图标/高亮):

  ```sql
  CREATE TABLE skill_file (
      id         TEXT PRIMARY KEY,
      skill_id   TEXT NOT NULL REFERENCES skill(id),
      rel_path   TEXT NOT NULL,   -- 如实相对路径:"references/mocking.md"、"agents/openai.yaml"
      content    TEXT NOT NULL,
      created_at INTEGER NOT NULL
  );
  ```

  `SkillCard.content` 仍只存 SKILL.md 正文;其余文件全进 `skill_file`。

- 新增两个导入命令(**copy-on-import,导入后与源目录解耦**,与 plan/08 归属决定一致):
  - `ImportSkillPackage { source_path, project_id }`——读单个 skill 文件夹:解析
    SKILL.md frontmatter(name/description)+正文,递归收集其余全部文件进 `skill_file`。
    不硬编码 mattpocock/superpowers 路径,任何符合 SKILL.md 约定的文件夹都能导。
  - `ImportSkillLibrary { root_path, official_library }`——批量:walkdir 找出所有含
    SKILL.md 的目录(node_modules 等自动跳过,因为它们没有 SKILL.md),逐个走单包导入。

- `SkillCard` 新增 `stage_ref: Option<StageKind>`(None=跨阶段通用),与
  `WorkflowSpec.stage_ref` 同一套枚举;原 `category` 自由文本保留作细分类补充。

**UI(拍板)**:详情页改**双栏**——左侧文件树(SKILL.md 固定置顶默认选中,其余按真实
目录结构列出),右侧选中文件内容预览;点击树节点切换,接近 IDE 浏览体验。

---

## 3. Agent:AGENT.md 建模 + Agent CLI 真实路由

**现状**:`AgentCard`(`model.rs:1014`)只有扁平 `instructions: String`;无 tools、
无执行引擎概念;`model: String` 是"诚实标签,不驱动行为"。

**改法(AgentCard 新增三个字段)**:

| 字段 | 语义 | 拍板要点 |
|---|---|---|
| `stage_ref: Option<StageKind>` | 属于哪个阶段角色(分类维度) | 与 Skill/Workflow 对齐 |
| `tools: Vec<String>` | **就是 AllowedTools**,与 claude CLI `--allowedTools` 同一定义 | **真实生效**:执行时经 CLI 适配器传给底层 CLI;字段本身 CLI 无关,适配层负责翻译(解耦) |
| `agent_cli: String` | 用哪个 Agent CLI 执行(claude-code / codex / cursor / …) | **真实路由**,不是标签(见下) |

**Agent CLI 路由(拍板:诚实展示"只支持 Claude Code,后期可扩展")**:
`bw-engine` 已有 `Executor` trait(`crates/bw-engine/src/lib.rs:66`)。本次建立
按 `agent_cli` 分发的路由机制;首版**只有** `ClaudeCliExecutor` 真实可跑,选了
codex/cursor 的诚实返回"本机未安装 codex/cursor CLI"错误——**不伪造成功**。
思路同 Connector:今天只支持 GitHub,但入口解耦,未来可平移到同类实现。
(写作日实测:本机仅 `claude` 在 PATH,codex/cursor 均未安装。)

**导入**:`ImportAgentDefinition { source_path }` 解析 AGENT.md 格式
(frontmatter name/description/tools/model + 正文→instructions)。67 个 ECC agent
用它入库,source=Official/ecc。

**UI(拍板,参考业界 subagent 目录站的卡片模式)**:tools 是关键 metadata,
**直接上卡片面**做紧凑 chip 行(与 skills chips 平级);`agent_cli` 相对次要,
折进展开详情("执行引擎: Claude Code"一行);model chip 维持现状。

---

## 4. Workflow:对抗式评审门与打回循环

**现状**:`LoopConfig { retries, max_iter }`(`model.rs:544`)只表达"整个 workflow
从头重来 N 次";`workflow_flow.rs` 靠**阶段名中文关键词**猜测 Generator/Evaluator,
仅用于画图不驱动行为。

**目标形态(用户定义)**:Workflow 本身是大循环(大 loop),内部有对抗式评审 agent
把守的小循环(小 loop)——TDD 模式下,测试 agent 随时验收开发产出,可打回到第一阶段
**或某个中间阶段**;循环几次由评审 agent 的验收结果决定。

**改法**:

- `WorkflowSpec.phases` 从 `Vec<String>` 扩展为结构化 `Vec<PhaseMeta>`:

  ```rust
  pub struct PhaseMeta {
      pub name: String,
      pub role: PhaseRole,                  // Generator | Evaluator | Optimizer | Neutral
      pub reject_to_phase: Option<u8>,      // 仅 Evaluator 有意义
  }
  ```

  哪个阶段是评审门由**真实字段声明**,不再靠关键词猜;`workflow_flow.rs` 的展示改读
  该字段。

- **打回目标的静态/动态双轨(拍板)**:
  - `WorkflowKind::Static`(已沉淀):`reject_to_phase` 作者设计时写死——评审阶段
    知道回退到哪一阶,定死的。
  - `WorkflowKind::Dynamic`(未沉淀,首次创建):用户未指定回退目标时,由评审 agent
    在真实执行中动态决定——其输出必须含结构化裁决
    `PhaseOutcome { verdict: Pass | RejectToPhase(u8), reason }`,BW 解析真实输出
    决定跳回哪阶。动态决定期不会太长,沉淀成 Static 后就固定了。

- **小 loop 安全上限(拍板)**:达到 `max_iter` 仍被评审打回 → **不自动判 Failed**,
  关联 Issue 转 **Blocked** + 真实 `blocked_reason`(如"对抗循环 3/3 仍未通过,需人工
  介入"),交人决定继续重试/改工作流/放弃——与"Done 永不自动"铁律同一精神。

**UI(拍板)**:阶段方块横排;Evaluator 阶段特殊图标(🛡️/⚖️)标记;其下弧线回退
箭头指向 `reject_to_phase` 对应方块——**实线=静态固定目标,虚线+「?」=动态待定**
(运行时真实发生后回填实际路径);弧上标注"循环 N/max"计数。

---

## 5. Cron:四种模式,全部真实执行

**现状**:`CronMode { RunWorkflow, CreateIssue }`(`model.rs:1051`);
`tick_scheduler` 真实调度已在(`crates/bw-app/src/lib.rs:1315`)。

**改法(拍板:两个都加)**:

```rust
pub enum CronMode {
    RunWorkflow,                           // 现有,不变
    RunSkill  { skill_id: SkillId },       // 定时跑一个 Skill:content 作 prompt 交 agent_cli
    RunPrompt { prompt: String },          // 定时跑一段裸 Prompt,不依赖任何实体
    CreateIssue,                           // 现有 autopilot,不变(绝不自动跑)
}
```

两种新模式均走 agent_cli(首版 Claude Code)真实执行,证据同样记入
`CronEffectiveness`。`RunSkill` 存**真实 `skill_id` 引用**(不再自由文本名字匹配,
避免同名冲突)。

**UI(拍板)**:每种 mode 配匹配的目标展示+行前图标区分(🔄 Workflow / ⚙️ Skill /
💬 Prompt);RunPrompt 的 target 栏显示 prompt 前 40 字预览,点击展开全文;
**RunSkill 选择器用可搜索弹窗,不用下拉框**——Skill 库必然长到百/千条级市场规模,
下拉不可用(弹窗组件是否有现成可复用,实施时确认)。

---

## 6. HubSource 重构:选型是持续动作,不该各开枚举

**拍板逻辑(用户原话大意)**:OMC/ECC/MattPocock/SuperPowers 都是"我们持续选型引入的
高分精品插件",这个集合只会越来越发散,不该每选一个就加一个枚举变体——它们是同一件事:
**官方选型预置**,用子标签区分具体库。

```rust
pub enum HubSource {
    Official,        // 官方选型预置;配 official_library: String 子标签
                     //   写作日真实取值:"ecc"(67 agent)、"mattpocock-skills"、
                     //   "superpowers"(36 skill);"omc" 暂无实例,未来可再用
    Adopted,         // 语义改为:后期用户/会话自行选型引入官方集之外的第三方插件。
                     //   今天无具体入口,预留
    SelfBuilt,       // 不变:个人自建(含蒸馏)
    WithinSession,   // 不变:会话内产生
}
```

UI 筛选栏:`[官方选型 ▾]` 可展开按 official_library 二次分组
(现 `vm.rs:618` 的硬编码 chip 序列相应改写)。

---

## 7. 编辑与版本冲突:编辑即脱离源头

**拍板**:Official 导入的内容**允许编辑**(本质都是 MD,且引入的库全是开源的)。
冲突处理:一旦用户改过实质内容(content/instructions/tools),该条目自动
`source → SelfBuilt`,与原库解耦——BW 版本更新重新导入同名库时**不会覆盖**已脱离的
本地副本。机制与蒸馏同精神:一旦有人写的内容,就不再是纯目录引用。

---

## 8. 工程对照(锚点表,写作日行号)

| 改动 | 锚点 |
|---|---|
| SkillCard / AgentCard / WorkflowSpec / LoopConfig / CronTask / CronMode / HubSource | `crates/bw-core/src/model.rs:976 / 1014 / 564 / 544 / 1085 / 1051 / 494` |
| skill_file 新表 + 各表新列 | `crates/bw-store/src/schema.sql` + `sqlite.rs` `add_column_if_missing` **双守卫**(铁律:CREATE TABLE IF NOT EXISTS 不给存量表加列) |
| 删 OMC/ECC 目录种子 | `crates/bw-store/src/seed.rs`(OMC_SKILLS:46 / OMC_AGENTS:414 / ECC_SKILLS:602 / ECC_AGENTS:1960 / ECC_WORKFLOWS:2298) |
| 导入命令 Import{SkillPackage,SkillLibrary,AgentDefinition} | `crates/bw-app/src/lib.rs` Command 枚举(参考 DistillSkillFromIssue:314) |
| agent_cli 路由 + tools→--allowedTools | `crates/bw-engine/src/lib.rs:66`(Executor trait)、`claude_cli.rs` |
| PhaseOutcome 解析 / Blocked 上限 | `bw-app` run_workflow_inner 一线;Issue.blocked_reason 已有(`model.rs:1524`) |
| Skill 双栏 / Agent chips / Workflow 弧线 / Cron 图标行 | `app-desktop/src/screens/{skill_hub,agent_hub,workflow_flow,cron_hub}.rs` + `ui/src/vm.rs`(SkillCardVm:783 / AgentCardVm:822 / WorkflowHubRowVm:497 / CronRowVm:961) |
| ECC 67 agent 源 | github.com/affaan-m/everything-claude-code `agents/*.md`(逐文件 API 拉取,勿 clone 整仓) |

验证照旧走核心纪律:临时 DB + 深链启动(`BW_OPEN`/`BW_PANEL`,stderr `[BW_OPEN]`)
+ `sqlite3` 读回(导入后核 36/67 条数、skill_file 行数、编辑后 source 翻转)+ 截图存档;
`/code-review` 把质量。

---

## 9. 留白(未建,不假装)

- **组织级资产导入**:类似 SelfBuilt 但属团队/组织级资产,与个人自建有别——未建,留口。
- **加密/商业资产保护**:自建能力经 Official 渠道分发时可能加密不可编辑——未建,留口。
- **Codex / Cursor 真实执行**:路由已留,本机无二进制,诚实报错——未建真实路径。
- **Adopted 入口**:语义已定(用户自选引入),无 UI 入口——未建。
- **plan/08 §避免重做**:cron 暂停/恢复/上次/下次已完整,本文不动它。
