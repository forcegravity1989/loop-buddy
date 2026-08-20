# 预研:workflow(SOP 类技能包)在文件层面到底长什么样

> ⚠️ **历史档案(2026-08-20 归档)**。这是 V4 设计期的一篇源码级预研,**结论已经采纳进设计与代码**,留档只为考古「当时看了什么才这么定」。现状以 `docs/v4-prototype/design/` 对应篇与 `crates/bw-v4`、`crates/app-shell` 的源码为准;还没干的活只认 `docs/LEFTOVERS.md`。

> **30 秒导读**:这是一篇**预研笔记**,不是设计文档,不改代码。背景:2026-08-19 内部专家评审给 V4 提了一条——配置里的开工工具映射(「活按类别选哪个执行工具」的那张表)去掉 agent 列,只留「类别 → 开工工具 → workflow」三列;**workflow(这个词在 V4 里特指「SOP 类的技能包」——不是旧聊天式引擎里那个「工作流」)的入口是一份 SKILL.md(技能说明文件,Claude Code 读它来决定何时、怎么干一件事),这类包会在流程里自己决定该用哪个 agent(执行任务的子角色)**。这条判断已经写进设计事实源 `docs/v4-prototype/mvp-blueprint-draft.md` §2.6/待拍-24,但没人核实过它在文件层面是否站得住。本文核实。**给谁看**:负责把待拍-24 落成详细设计的人。**现在作数吗**:作为「已核实的事实清单」作数,但文末「对 buddy 的判断」和「推荐」是本文作者的一次性判断,尚未拍板,写进详细设计前建议再走一轮评审。

## 一句话结论

两个真实包证实「workflow = 多技能 SOP 包,自己在正文里现场喊 Claude 起子代理」这条成立;但「包自带 `.claude/agents/` 持久角色定义」这条**不成立**——Claude Code 官方确实支持这个机制,但两个真实包都没用,全靠正文散文指令调用内置的通用子代理类型;buddy 现有的装载链(SKILL.md → 单技能行,AGENT.md → 单独 agent 表)接不住「一整包」,需要新加「这些技能属于同一个包」的分组概念,而不是去接官方的 `--plugin-dir` 整包加载。

---

## 事实

### 事实 1 · 本机两套真实 workflow 包长什么样

**来源**:`~/.claude/plugins/cache/mattpocock/mattpocock-skills/1.2.0/`、`~/.claude/plugins/cache/superpowers-dev/superpowers/6.1.1/`(均为本机已装插件,`ls -R` + 读 `.claude-plugin/plugin.json`/`SKILL.md`/`AGENTS.md` 实读,非记忆复述)。

**mattpocock-skills**:`.claude-plugin/plugin.json` 的 `skills` 数组列了 22 条 promoted 技能路径(`skills/engineering/*`、`skills/productivity/*`),**没有 `agents` 数组字段**。入口是 `skills/engineering/ask-matt/SKILL.md`(frontmatter `disable-model-invocation: true`,只能人工 `/ask-matt` 调用),正文自称「A router over the skills in this repo」,画出一条主线 + 三条外围:

```
ask-matt(路由,人工触发)
 └─ 主线:grill-with-docs(访谈,写 CONTEXT.md/ADR)
       → [可选支线:handoff → 新会话 prototype → handoff 回主线]
       → [分支]多会话构建? → to-spec → to-tickets → implement(逐票,清空上下文)
                单会话构建?→ 直接 implement
       implement 内部驱动 tdd(红绿测试)一路,结束跑 code-review(Standards+Spec 两轴),再 commit
 ├─ on-ramp:triage(处理外部来的 bug/需求)→ 合流进 implement
 ├─ on-ramp:diagnosing-bugs(硬 bug,先要一条能复现的红)→ 若发现无好 seam → improve-codebase-architecture
 ├─ on-ramp:wayfinder(大迷雾/greenfield,产决策票不产交付物)→ 合流进 to-spec
 ├─ 底层词汇(被上面技能按需拉入,不是流程步骤):domain-modeling、codebase-design
 └─ 独立、不在主线上:grill-me(无代码库版访谈)、prototype、research、teach、writing-great-skills
```

**superpowers**:`.claude-plugin/plugin.json` 没有显式 `skills` 数组(不像 mattpocock 那样列举),14 个技能全部平铺在 `skills/*/SKILL.md`,无 bucket 子目录。入口是 `skills/using-superpowers/SKILL.md`——不是一张路由图,而是一条元规则:「任何任务前,只要有 1% 可能某技能适用,必须先用它;process 类技能(brainstorming、systematic-debugging)先于实现类技能」。由其余技能互相引用拼出的典型主线:

```
using-superpowers(元规则,每次会话开头必检查)
 └─ 典型主线:brainstorming → writing-plans → subagent-driven-development
                                                  │
                                    每个任务:implementer 子代理 → task-reviewer 子代理
                                              → 不过关则 fix 子代理,重新 task-reviewer(循环)
                                                  │
                                    全部任务完 → 全分支 code-reviewer 子代理 → finishing-a-development-branch
 ├─ 独立/按需挂入主线:systematic-debugging、dispatching-parallel-agents、
 │   using-git-worktrees、writing-skills、verification-before-completion、
 │   requesting-code-review / receiving-code-review
```

**两包怎么调度自己的 agent**:全仓 `grep -rniE "subagent_type|Task tool|agent tool|dispatch.*agent"`,命中的**全部是 SKILL.md 正文里的散文指令**,例如 `mattpocock-skills/skills/engineering/improve-codebase-architecture/SKILL.md:27`「use the Agent tool with `subagent_type=Explore`」、`codebase-design/DESIGN-IT-TWICE.md:21`「Spawn 3+ sub-agents in parallel using the Agent tool」、`superpowers/skills/subagent-driven-development/SKILL.md:115`「Always specify the model explicitly when dispatching a subagent」。两个包**都没有**插件根目录的 `agents/` 文件夹(Claude Code 官方那种持久 subagent 人设定义,见事实 2)——调度完全靠「正在跑的主线程 Claude 读到这段正文,现场调用 Agent/Task 工具」,不是包自带一份写死的角色文件。

**一个容易踩的坑**:mattpocock-skills 每个技能目录下都有一个 `agents/openai.yaml`(如 `skills/engineering/tdd/agents/openai.yaml`),内容只有 `interface.display_name`/`interface.short_description` 两个字段。读 `.agents/invocation.md` 原文确认:「Every skill also carries an `agents/openai.yaml` beside its `SKILL.md`. It holds **Codex** UI metadata」——这是给 OpenAI Codex 这类**别的 harness** 用的跨平台显示元数据,和 Claude Code 官方插件级 `agents/` 目录(持久 subagent 定义)是两回事,**目录名撞了,含义完全不同**,读源码时被这个误导了一次,记录下来防止详细设计阶段重踩。

**grillme 单技能的样子**:本机没有独立安装的「grillme」包(`~/.claude/plugins/installed_plugins.json` 只有 claude-hud/superpowers/mattpocock-skills/web-access 四个),独立的 grillme 仓库本身**未核实**。但 mattpocock-skills 包内的 `grill-me` 技能是很好的对照样本——`skills/productivity/grill-me/SKILL.md` 全文只有两行:

```
Run a `/grilling` session.
```

`disable-model-invocation: true`。一个目录、一个 SKILL.md、两行正文、指向另一个技能——这正是评审说的「grillme 单个只是一个 skill(引导模型问用户问题)」的典型形状。

### 事实 2 · buddy 今天怎么装载与注入技能

**来源**:`crates/bw-app/src/skill_import.rs`、`skill_materialize.rs`、`agent_import.rs`、`prompts.rs`、`crates/bw-engine/src/interactive_cli.rs`、`docs/skills/`、`crates/bw-core/src/bw_library.rs`;官方能力核实见本节末。

- **唯一的 SKILL.md 解析器**:`skill_import.rs::parse_skill_md`(bw_canon.rs 的内置技能与磁盘导入共用同一条解析路径),要求 frontmatter 有 `name`+`description`,其余字段(`disable-model-invocation` 等)读了就丢,不解释语义。
- **导入粒度 = 一个目录一行**:`Command::ImportSkillPackage{source_path,...}` 读一个含 `SKILL.md` 的目录 → `skill` 表一行(+ 支撑文件进 `skill_file` 表,`rel_path` 原样保留,如 `references/mocking.md`)。批量版 `Command::ImportSkillLibrary{root_path,...}` 递归找库根下**所有**含 `SKILL.md` 的目录(剪掉 `node_modules`/`.git`/`target`,`find_skill_package_dirs`),**每个命中各自独立变成一行 skill**。也就是说:今天把 mattpocock-skills 整个库指给 `ImportSkillLibrary`,得到的是 22 条互不相关的 skill 行——ask-matt 路由图裹挟的「谁是主线、谁是分支、谁被谁引用」关系**完全丢失**,数据库里不存在「这 22 个技能属于同一个包」的表示。
- **Agent 是完全独立的一条链**:`agent_import.rs::import_agent_definition_from_disk` 解析单个 `AGENT.md`(ECC——everything-claude-code 社区库的 subagent 格式:frontmatter `name`/`description`/`tools`/`model` + 正文 `instructions`)进 `agent` 表;`scan_project_agents_dir` 只 flat 扫 `workspace/agents/*.md`,doc comment 原话:「a project that nests agents under subdirectories (e.g. `.claude/agents/`) isn't picked up here」——buddy 现在的 agent 扫描**够不到**官方 `.claude/agents/` 路径。这条缺口目前无害,因为事实 1 已证实 mattpocock-skills/superpowers 两个真实包也根本没有这个目录可扫。
- **注入到 CLI 的路径**:`interactive_cli.rs::build_startup_plan` 用一条命令启动:
  ```
  claude --append-system-prompt <system_prompt> <position_prompt> \
         --dangerously-skip-permissions --disallowedTools "Bash(gh pr merge)"
  ```
  调用方(`issue_run.rs`)把「bridge 系统提示词(项目上下文 + 铁律 + 技能契约)+ 标配技能引用(`standard_skill_block`,只记 uses、不进 prompt 正文)+ 蒸馏技能正文(`distilled_skills_block`,直接拼进 prompt——BW 自产的蒸馏技能通常够短)+ 本阶段技能**目录**(`stage_catalog_block`,只出一行 `name — 一句话描述`,不出正文)」拼成一个字符串塞进这一个 flag。目录为什么只出目录不出正文,`skill_materialize.rs` 文档注释写得很直白:「superpowers 技能正文平均 8732 字符(实测,2026-08-05)——单独一条就撑爆整个 6000 字符的注入护栏上限」。
  - **正文怎么真正到 CLI 跟前**:不是塞进 prompt 字符串,是**物化到磁盘**——`skill_materialize::materialize_stage_skills` 把技能行(+ 它的 `skill_file` 支撑文件)写成 `workspace/.claude/skills/<name>/SKILL.md`(+ 支撑文件原样落盘),靠 Claude CLI **自己原生的** skill 发现机制按需加载;prompt 里只留一句「已物化到 `.claude/skills/`,按需自行加载」的目录提示。有 `.bw-managed` 标记文件(内容 `skill.id\nskill.rev`)防止覆盖用户自己手写的同名技能目录,同名目录若不带这个标记就整条跳过、留痕在 `skipped_foreign`。
- **buddy 今天没用到的官方机制**(全代码库 grep 确认 0 命中):`--plugin-dir`(整包加载一个含 `skills/`+`agents/`+`hooks/`+`.mcp.json` 的插件目录,仅当次会话)、SKILL.md frontmatter 的 `context: fork`+`agent: <subagent_type>`(官方**声明式**的「这份技能要在隔离子代理里跑、用哪个 subagent 类型」绑定,内置 `Explore`/`Plan`/`general-purpose` 或 `.claude/agents/` 里的自定义 subagent)。技能是「数据行 → 按行物化成独立 SKILL.md 目录 → 靠 CLI 自己发现」,agent 是完全独立的一张表、一次 AGENT.md = 一行,两条装载链彼此不知道对方存在。
- **buddy 自带技能**:`docs/skills/*/SKILL.md`(9 个,如 `evidence-first`、`spec-to-tests`、`north-star-discovery`)全部是**单文件单技能**,靠 `crates/bw-core/src/bw_library.rs` 的 `include_str!` 编进二进制常量,没有一个是多 SKILL.md 的包。
- **官方能力核实**(`claude --help` 本机核实 + `WebFetch` 读 `https://code.claude.com/docs/en/skills`、`https://code.claude.com/docs/en/plugins`,2026-08-19 抓取):
  - `claude --help` 输出确认 `--plugin-dir <path>` 真实存在(「Load a plugin from a directory or .zip for this session only」)。
  - 官方插件目录结构表(plugins 文档原文):插件根目录下 `skills/`、`commands/`、`agents/`(「Custom agent definitions」)、`hooks/`、`.mcp.json`、`.lsp.json` 等各自独立,**`agents/` 是官方一等公民目录**,不是 buddy 猜的。
  - 「一个包多个技能」的官方判定标准(plugins 文档原文):「A plugin that ships exactly one skill can place `SKILL.md` directly at the plugin root instead of creating a `skills/` directory... Use the `skills/` layout for plugins that may grow to more than one skill.」——即「`skills/` 子目录下有 ≥2 个独立技能目录」是官方约定的「这是多技能包」信号。
  - `context: fork` + `agent:` 官方原文(skills 文档):「Add `context: fork` to your frontmatter when you want a skill to run in isolation. The skill content becomes the prompt that drives the subagent... The `agent` field specifies which subagent configuration to use. Options include built-in agents (`Explore`, `Plan`, `general-purpose`) or any custom subagent from `.claude/agents/`. If omitted, uses `general-purpose`.」——这条机制**存在且官方文档专门举了例**,但事实 1 已证实两个真实包都没用(全仓 `grep "context: fork"` 零命中)。

### 事实 3 · 单技能 vs workflow 包在文件层面的判据

结合事实 1(真实样本)+ 事实 2(官方结构),给出一套 buddy 可执行的判据草案,逐个真实样本套一遍:

| 判据 | 依据 | mattpocock-skills | superpowers | grill-me(包内单技能) | buddy 自带(`docs/skills/*`) |
|---|---|---|---|---|---|
| A. 顶层有 `.claude-plugin/plugin.json` | 官方插件标识 | 是 | 是 | 否(它是包内一个子目录) | 否 |
| B. `skills/` 下有 ≥2 个各自独立的 `<name>/SKILL.md` | 官方「多技能布局」约定原文(见事实 2) | 是(22 个) | 是(14 个) | 不适用 | 不适用(每个技能各自单目录、互不隶属) |
| C. 存在一份「入口/路由」技能(`disable-model-invocation: true` 且正文引用其他技能名) | ask-matt / using-superpowers 实读 | 是(ask-matt) | 是(using-superpowers,起同等作用但不是路由图) | 否(它自己就是被引用的一个叶子) | 否 |
| D. 插件根目录(不是某技能子目录里)有官方 `agents/` | 官方目录表 | 否 | 否 | 不适用 | 否 |

**结论**:A+B 两条纯结构判据就能把 mattpocock-skills/superpowers 判成「workflow 包」,把 grill-me、buddy 自带技能判成「单技能」——**判得对**。C(自动识别哪份是「入口」)只能弱判定,靠 frontmatter 一个 boolean 加正文关键词扫,容易误判(比如 `research`、`writing-great-skills` 也带 `disable-model-invocation` 但不是路由技能);D 在这两个真实样本上恒为 0,不构成有效判据,但官方机制本身是真的(见事实 2),不能因为两个样本都是 0 就当它不存在。

### 事实 4 · 导入自己的 workflow

**来源**:`crates/bw-app/src/command.rs`(`ImportSkillPackage`/`ImportSkillLibrary`/`ImportAgentDefinition` 三条 Command 定义与 doc comment)、`docs/v4-prototype/mvp-blueprint-draft.md`(「仓 vs 库」两处表述)、`docs/v4-prototype/standard-module-draft.md`(`.bw/issue-policy.toml` 落点)。

- buddy 现有 Command 只有「单技能目录 → 单行」(`ImportSkillPackage`)和「库根批量单技能目录 → N 行互不相关」(`ImportSkillLibrary`,幂等键 `(name, official_library)`),没有「整包导入、保留包内关系」的概念;`ImportAgentDefinition` 同理是单文件单行。
- V4「资产在仓」的既定原则(`mvp-blueprint-draft.md`):「留下什么」分**仓**(项目代码仓,正本;「变更走 MR;人 / agent / leader / committer 全可见;不需要在 buddy 里特别关注」)和**库**(buddy 本机 SQLite,「只做记账与推导」)。按同一原则,项目级导入的 workflow 包理应落进项目仓(比如 `.claude/skills/<pkg>/` 下),让 committer/agent 直接 git 可见;落本机全局 `~/.claude/plugins/` 只有这台机器看得见,协作时不可复现,与「换项目不用换脑子、agent 打开仓就懂」这条命题相悖。
- **一个尚未闭合的缝**:现有 `skill_materialize.rs` 已经具备「整包物化」的底子(SKILL.md + 任意多支撑文件都能落盘、有 `.bw-managed` 标记防覆盖),但它今天的角色是**从 SQLite store 导出的派生缓存**(每次跑 Issue 按阶段重新生成),不是「导入的正本」。如果要让导入的 workflow 包本身进仓当正本(用户 `git commit` 它、随 MR 走),需要一条新的落地路径,不能照搬现在「store → 按需物化」这条单向、可随时重生成的链路;`.bw/issue-policy.toml`(`standard-module-draft.md` 已规划它承载「类别 → 开工工具 → workflow」三列映射)本身倒是明确要进仓的规范铺底件,和技能包正文是否也进仓,是两个可以分开定的问题。
- **Cursor**:`WebFetch` 读 `cursor.com/docs/context/rules` 摘要(**注意:这是一次网页内容的摘要抓取,未逐句核对官方原文,详细设计前建议原文复核**)——Cursor 支持从 GitHub 仓库导入 `.mdc` 规则文件(「Remote Rules」),按 Team/Project/User 三级优先级叠加,但**没有** SKILL.md 风格的多文件包概念,官方文档摘要里也没有「导入一个多步骤 workflow 包」的机制。也就是说 Cursor 这边「导入自己的 workflow」目前只能落到单个规则文件级别,包级别的语义(路由、on-ramp、子代理调度)会丢。Cursor 的自定义 subagent 机制**未核实**(本次抓取没有覆盖到)。

### 事实 5 · 战绩记到 workflow / 技能上

**来源**:`crates/bw-store/src/schema.sql`(`agent`/`skill`/`workflow_spec`/`workflow_run` 表定义)、`crates/bw-store/src/sqlite.rs::record_agent_run`、`crates/bw-app/src/dispatch.rs`(`TransitionIssue`/`credited_agent` 附近)。

- 今天真正有「胜率」概念的是 `agent` 表:`runs`/`wins`/`win_rate` 三列,`win_rate` 是 SQL 里现算的派生字符串——`sqlite.rs::record_agent_run`:
  ```sql
  UPDATE agent SET runs=runs+1, wins=wins+?,
  win_rate = printf('%d%%', (wins+?)*100/(runs+1)),
  updated_at=?, rev=rev+1 WHERE id=?
  ```
  从不单独手写 `win_rate` 列——这正是 CLAUDE.md「队友胜率由真实战绩算出,绝不手工设定」这条铁律的落地点。
- **记账时机**(settle-once 的具体位置):`dispatch.rs` 的 `TransitionIssue` 处理里,只有 Issue 真正从「评审中」被人点到「完成」(`newly_done` = `status==Done && prev.status!=Done && prev.settled_at.is_none()`)时,才按 `issue.assignee`(**按 id 不按名**——plan/20 R3 修过「按名全表 UPDATE 会给所有项目同名队友齐记战绩」的真 bug)解出 `AgentId`,调 `record_agent_run(agent_id, true)`;run 失败时另在 `finalize_run_interactive` 记一次 `record_agent_run(.., false)`。Cancelled/reopen-重做都不重复记账(注释原话:「dropping an issue is not evidence about the agent's work」「the first win stands in the append-only history」)。
- `skill` 表只有 `uses` 一个计数列,**没有** win/loss/win_rate——`record_skill_use` 只是 `uses=uses+1`,今天技能的「战绩」仅仅是「被引用了几次」,不是「跟着它的活成功了几次」。
- `workflow_spec` 表是**今天代码里的 Workflow 实体**(和 V4 新定义的「workflow = SOP 类技能包」不是一回事——今天的 Workflow 是旧的、挂五阶段的流程卡片,带 `agents_json`/`skills_json` 两个 JSON 引用数组);配套的 `workflow_run` 表是纯 append-only 执行日志(`status`/`duration_ms`/`started_at`/`finished_at`),**没有任何代码把它汇总回 `workflow_spec` 上的 win_rate**。

**V4「战绩记到 workflow/技能上」最小改动的结论(只列结论,不改代码)**:
1. 给承载「workflow=SOP 技能包」语义的那张表(多半仍是 `skill` 表的某个子集,或新增一张分组表——见事实 3 的开放问题)照抄 `agent` 表现成的三列模式:加 `runs`/`wins`/`win_rate`,`add_column_if_missing` 双守卫(CLAUDE.md 纪律 5),`win_rate` 同样用 SQL `printf` 现算,绝不手写。
2. 记账调用点从「`credited_agent` + `record_agent_run`」平移成「credited_workflow/skill + 等价的 `record_skill_run`」(不是现在只做计数的 `record_skill_use`),挂载点不变——仍是 Issue 的 Done 边(`newly_done`)+ run 失败两处,settle-once 的判据(`prev.status`/`prev.settled_at`)原样复用。
3. `agent` 表本身要不要整表下线,还是保留给「以后允许高级用户手写自定义 agent」留口,**本预研没有核实到任何 V4 文档给出明确结论**,留作开放问题。

---

## 对 buddy 的判断

- **workflow 的定义(核实后的精确版)**:workflow = 一个**技能容器**(至少满足事实 3 判据 A/B 之一——有 `plugin.json` 且 `skills/` 下多个独立技能目录),其中常有一份「入口」技能(多半 `disable-model-invocation: true`,人工调用)用**正文散文**把其余技能串成带分支/on-ramp 的流程;流程里**该用哪个 agent,由入口/沿途技能的正文临时决定**——调用 Claude Code 内置的 Agent/Task 工具,常见 `subagent_type=Explore`/`general-purpose`,或官方 `context: fork`+`agent:` 声明式绑定——但两个真实样本都没用后者,全靠命令式散文。**不是**包自带一份持久的、有名字的 agent 人设文件(官方支持这个能力,两个真实样本都没用)。
- **识别判据**:结构判据(A/B/D)可自动、可信;「谁是入口」(C)只能弱判定,MVP 不追求全自动分类,导入后允许人工补一个「入口技能」标记,类比 `skill.stage_origin` 已有的 `Manual` 先例。
- **注入方式推荐**:不新增「把整包塞进 `--append-system-prompt`」的路子——6000 字符护栏、superpowers 单条技能均值 8732 字符的实测数字已经说明这条路走不通,一整包更不可能塞下。顺着现有 `skill_materialize` 的物化模式扩展,但要补一层「保留包边界」——目前 `ImportSkillLibrary` 把一个包拍平成 N 条互不相关的 skill 行,需要让这 N 行记住「同源于哪一次导入」,物化时才能整包一次性落进 `workspace/.claude/skills/`(这已经是 CLI 原生发现路径,不需要碰 `--plugin-dir`)。
- **导入方式推荐**:复用「资产在仓」原则——项目级导入落进项目仓(如 `.claude/skills/<name>/`,随 MR 可见);全局/个人导入可以留在 buddy 本机 skill 表(`project_id` NULL),但要接受「别的 committer 看不到」的代价,这本身与 V4「仓是正本、库只记账」的既定分工一致,不是新缺口。
- **战绩记账主体推荐**:见事实 5 的三条结论——照抄 `agent` 表 `runs`/`wins`/`win_rate` 的 SQL 模式搬到承载 workflow 语义的表,记账挂点(Done 边 + run 失败,settle-once)原样复用。

## 推荐(MVP 做什么)

1. 不接官方 `--plugin-dir`,也不追官方 `context: fork`——继续走「物化到 `.claude/skills/` + prompt 里留目录提示」这条已经有实测护栏数字撑腰的老路,只是给它加「包分组」能力。
2. `skill` 表(或新增一张很小的分组表)加一个「属于哪次导入/哪个包」的字段,让 `ImportSkillLibrary` 一次性导入时记住「这 N 条同源」,而不是像今天这样全部拍平成互不相关的行。
3. 用判据 A/B/D 做导入时的自动结构归类,C(入口技能)留人工确认;配置页「workflow 表」如果要展示「自带 agent 数」这一列(`mvp-blueprint-draft.md` §244 已经规划了这一列),**如实显示这台机器实测的两个真实包都是 0**,不要为了让它非零去瞎猜。
4. 照抄 `agent.runs/wins/win_rate` 的 SQL 模式,给 workflow 加同款战绩三列,复用现成的 `credited_*` + settle-once 挂点。
5. 导入落地统一走「资产在仓」路径,复用/扩展现有 `.claude/skills/` 落地层,而不是新开一条本机 plugins 目录的路。

## 不做什么

- 不实现「buddy 自己维护一张 agent 名单/给 agent 表升级」——待拍-24 已明确不再单独维护 agent 名单;导入包判不出「入口技能」就如实留空,不强行猜。
- 不追 Claude Code 官方 `context: fork`/`--plugin-dir` 这两条声明式/整体加载机制——两个真实 workflow 包自己都没用,buddy 跟进它们换不来兼容性收益,反而要多接一层官方插件生命周期(`/reload-plugins` 等)的复杂度。
- 不在 prompt 正文里塞 workflow 包全文——护栏数字已经证明装不下。

## 留给详细设计的开放问题(≤5 条)

1. 「包」这个分组关系落在 `skill` 表新增字段,还是新建 `skill_package`/`workflow` 独立表——决定配置页「workflow 表」是一张新的一等公民表,还是 `skill` 表按分组字段拼出来的视图。
2. 判据 C(自动识别「这是入口/路由技能」)要不要做,还是 MVP 就只做 A/B/D(结构判据)、C 全靠人工登记。
3. 项目级导入的 workflow 包要不要真的 `git commit` 进仓(即算「资产在仓」的正本),还是仍作为 `.bw-managed` 可重新生成的派生缓存——这决定它算不算数据丢失时能找回来的东西。
4. `agent` 表(及其 `runs`/`win_rate`)在 workflow 成为记账主体后是保留(给未来手写自定义 agent 留口)还是整表下线——本预研没有核实到任何 V4 文档给出明确结论。
5. Cursor 导入路径的具体落地(「资产在仓」对 Cursor 的 `.cursor/rules/` 是否同样适用,Cursor 有没有自定义 subagent 机制)——本预研对 Cursor 只做了一次网页摘要抓取,未逐句核对原文,详细设计前建议重新核实。

---

_本篇为一次性预研笔记,不随上游(Claude Code 官方文档、mattpocock-skills/superpowers 插件版本)更新维护。插件锚点对应本机 2026-08-19 已装的版本(mattpocock-skills 1.2.0、superpowers 6.1.1),官方文档锚点对应同日抓取的 `code.claude.com` 页面内容,上游变动后可能漂移,复核时以当次实读为准。_
