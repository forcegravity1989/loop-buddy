# 04 · 开工工具与 workflow 怎么接

> **30 秒导读**:这篇回答两件事——**开工工具怎么注册与分发**(终端类如 Claude CLI / Cursor,与本机网页内嵌类如 Open Design,怎么声明、探活、起、停),**workflow(SOP 类技能包)与单技能怎么识别、铺进项目仓、物化到活的 worktree、注入给开工工具**。「用了几次」一律现算(扫 `.claude/skills/` + 数活的 `workflow` 属性),不建任何战绩表。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4G 七组。 会话屏三栏怎么摆是 [05 篇](05-session-screen.md)的事,三张运作 workflow 的正文是 [09 篇](09-ops-workflows.md)的事。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

---

## 0 · 这篇管什么、不管什么

**管**:母文档第 4 站「执行:每张活按类别选开工工具」的映射表、「agent 去哪了」段、待拍-09/10/24/32;[`../../standard-module-draft.md`](../standard-module-draft.md) 第 5 类(`.bw/issue-policy.toml`)与第 6 类(默认件与鱼塘);预研 [`../../research/workflow-skill-packages.md`](../../archive/v4-prototype/research/workflow-skill-packages.md)、[`../../research/deepseek-harness.md`](../../archive/v4-prototype/research/deepseek-harness.md)。具体是:①`[[tool]]` 怎么声明一个开工工具、新增工具要改哪里;②▶开工按 `tool` 字段怎么路由到 [01 篇](01-architecture.md) 的 `adapters/*` 模块;③workflow/单技能怎么现场识别(判据 A/B/C/D)、怎么铺底/导入进项目仓、怎么物化到活的 worktree;④映射三列(`类别→工具→workflow`)怎么落文件(`.bw/issue-policy.toml` 本身不入库,02 篇 §2.6 已定);⑤「用过几次」怎么从(早期草案原计划的)战绩记账改成现算查询——盘点之后连"战绩"这个概念本身都被取消,不只是换一个记账主体那么简单;⑥配置屏四段的数据来源;⑦相关命令/事件,只列名字。

**不管**:会话屏交互细节、终端标签页、代码结构侧栏(05 篇);运作 workflow 正文(09 篇);`issue-policy.toml` 之外的仓文件/库 schema 增量(02 篇);规范铺底流程与老项目历史回填(03 篇);codegraph 子命令(05 篇提,这篇只说它是 `adapters/codegraph` 模块,不是开工工具)。

---

## 1 · 用户看到什么、做什么

**配置一次开工方式**:配置屏第①段「开工工具映射」是一张表——六行(原型/构建/优化/运维/运营推广/运作)三列(类别/工具/workflow)。铺底时已按规范默认值填好(原型→Open Design→原型设计 workflow;构建/优化/运维→Claude CLI→**mattpocock-skills**(用户拍板改的默认,superpowers 在下拉里可换);运营推广→Claude CLI→空,提示"从鱼塘挑";运作→Claude CLI→三张自建运作 workflow)——这几个 workflow 包本身也是在**同一次铺底**里被真实复制进项目仓 `.claude/skills/` 的(02 篇 §2.5、母文档待拍-32),不是只在这份映射表里点个名字就算数:Claude CLI 只在项目仓里找技能,映射表指到哪个名字,仓里就必须真有那个目录。想换(比如把某类改用 Cursor,或把默认 workflow 换回 superpowers),下拉选一下、保存。

**导入一个新 workflow 包**:第②段「workflow 表」右上「导入」,三选一(本机目录/git 仓地址/从另一个项目,§2.8)。buddy 探测这是不是一个「包」(判据 A/B,§2.6),是就把整个目录复制进项目仓 `.claude/skills/<name>/`(走一张轻量活 + MR,不写任何库表);不是包就退回单技能,复制进同一棵目录树。配置屏②段「workflow 表」这一行不是"导入"这个动作本身往哪张表插了一行——它是**下次渲染这个配置屏时重新扫一遍目录**现场长出来的。猜不出入口技能就显示"未标注";这项没有持久的人工覆盖入口(§2.6),每次都是现场猜。

**给一张活换开工方式**:活详情面板「开工工具」「workflow」两个下拉,默认来自映射表,活上能单独换(比如这张活想只用 grillme 打磨需求,不走整套 superpowers)。

**点▶开工**:终端类(Claude CLI/Cursor)→ buddy 把这张活挂的那一份技能的名字、一句话、完整路径写进系统提示词(正文让 agent 按需读,§2.7),起嵌入终端;本机网页内嵌类(Open Design)→ 探活成功就在会话屏嵌一个 iframe,失败就灰态"未装·怎么装"。**这一步不再触发任何记账**(取代原来"活到评审中/完成那一刻战绩 +1"的设计,§2.9):配置屏「用过几次」是随时现查 `issue.workflow` 分组计数(02 篇 §2.3)得到的——只要这张活的 `workflow` 属性定下来就已经算在数里,不需要等它走到「评审中」或「完成」;也不再有"胜率"这一栏——"干没干成"改看远端 MR 合没合入,不由 buddy 自己判定。

---

## 2 · 设计

### 2.1 开工工具注册表:`[[tool]]` 声明

正本住项目仓 `.bw/issue-policy.toml`(标准草案第 5 类文件,03 篇铺底时写默认值)。每个工具一段:

```toml
[[tool]]
name = "claude_cli"
kind = "terminal"        # terminal(PTY 终端类) | web_embed(本机网页内嵌类)
probe = "path_candidates"
capabilities = ["inject_skills", "resume", "hooks"]

[[tool]]
name = "cursor"
kind = "terminal"
probe = "version_cmd"
version_cmd = "agent --version"   # 真实二进制名是 agent,不是 cursor.exe
capabilities = []             # Phase 1 未支持,如实留空

[[tool]]
name = "open_design"
kind = "web_embed"
probe = "socket"
capabilities = ["inject_skills"]   # 见 §2.7 开放问题
```

三个字段采纳 deepseek-harness 预研的三条接口设计判断(声明式工具清单/能力显式依赖/安装期权限对人可见):

- **`kind`**(设计期统一:与 `[[mapping]]` 段的 `category` 字段名分开,避免同一份 `.bw/issue-policy.toml` 文件里「工具接法类型」和「活的类别标签」撞用同一个字段名)只有两类——终端类(真实 PTY 子进程)与本机网页内嵌类(探活拿 URL 嵌 WebView)。将来 dsh 一类的 agent 网页也走后者,先进鱼塘不进 MVP。
- **`probe`** 对照今天已有的三套真实探活机制,不是重写:`path_candidates` 对照 `crates/bw-engine/src/claude_bin.rs::claude_binary_candidates`(12-25 行)——按序试候选路径(显式配置→`BW_CLAUDE_BIN`→npm 安装路径),第一个真实存在的文件赢(Cursor 候选应指向真实的 `agent` 二进制,`docs/v3-prototype/cursor-agent-executor.md` §4 已强调不要用 `cursor.exe`);`version_cmd` 对照 `TuiAgentConfig.detect_cmd`(`interactive_cli.rs:71-91`);`socket` 对照 `open_design.rs::discover_web_url`(20-28 行)——命名管道/unix socket 握手 `{"type":"status"}`,400ms 超时,`looks_loopback_http` 校验。
- **`capabilities`** 把散在 `TuiAgentConfig.supported`/`resume_flag`/`yolo_flag` 里的能力判断声明化:`inject_skills`(能不能物化技能进去)、`resume`(能不能续接会话)、`hooks`(能不能靠官方 hooks/statusLine 主动回报 agent 状态,给会话屏用)。Cursor 今天 `supported=false`(`interactive_cli.rs:121-130`),如实空数组。

**新增一个开工工具**:①`.bw/issue-policy.toml` 加一段 `[[tool]]`;②`crates/app-shell/src/adapters/<name>/` 新建目录(01 篇"一个外部能力一个适配模块"规矩);③终端类要真支持时,`interactive_cli.rs` 加一个新 `TuiAgentConfig` 静态实例并把 `supported` 置真;④配置屏下拉自动多一个选项(读声明列表渲染,不改配置屏代码)。**不需要碰 `Command`/`Event`**——路由读的是声明,不是硬编码分支。

### 2.2 探活如实

**未装就是灰,不是隐藏、不是假装能点**。探活失败的那一格灰底+"未装·怎么装→",跳项目墙「指南」抽屉对应章节(沿用高保真原型 v3 已定的交互)。探活时机:①项目墙"测一下"触发全量探活;②配置屏打开时探当前项目声明的工具(缓存);③活详情面板下拉展开时,缓存超过新鲜度阈值(建议 5 分钟)就重探——阈值留给开工时按体感调。

### 2.3 ▶开工分发流程

沿用既有 `Command::RunIssue`(签名不变,`command.rs:351` 起),内部加一层路由,不新开命令:

1. 读 `issue.tool`;空则按类别标签查 `[[mapping]]`(§2.5)默认工具,写回。
2. 按 `tool` 名字查 `[[tool]]` 拿到 `kind`。
3. `terminal`(Claude CLI/Cursor):解析 `issue.workflow`(§2.4)→ 在 buddy 自己的技能目录里挑出这一份、把名字+一句话+路径写进系统提示词(§2.7)→ 按工具名选 `TuiAgentConfig` → `build_startup_plan`(沿用,`interactive_cli.rs:219`)+ `--add-dir <buddy 技能目录>` 起 PTY 会话 → 出现在会话屏左列该 worktree 分组下。
4. `web_embed`(Open Design):按 socket 探活拿 URL,拿到就中栏开标签页 iframe(注入方式见 §2.7 开放问题),拿不到就灰态"未装·怎么装",活停在原地不算开工失败。

停止:终端类走既有 `Command::CancelRun`(不变);网页内嵌类的"停"只是关掉标签页——它本来就不是 buddy 起的子进程,不产生运行记录(§4 再说明这条边界)。

### 2.4 `issue.tool` / `issue.workflow`:存名字,解析靠扫目录

`issue.workflow` 存**名字**,不是 uuid——这样活详情面板换 workflow 不用先查 id,`.bw/plan/YYYY-Www.md` 里读到的就是可读名字,与周计划文件「业务活」表格的 `workflow` 列(02 篇 §2.5)天然对得上。

**解析规则,V4 简化为单层扫描**:CONTEXT.md「就近优先 / Most specific wins」词条描述的是 V1-V3 时代「项目行遮蔽全局行」的两层数据库解析,建立在 `project_id` 可空的 `skill`/`skill_package` 表之上——V4 里这两张表都不建了(§2.6),workflow/技能只有一处:**这张活 worktree 的 `.claude/skills/**/SKILL.md`**(铺底复制进来的预置包、蒸馏产出、人手加的,全在同一棵目录树里,02 篇 §2.5),不再存在「本项目行 vs 全局行」这层可比较的东西。▶开工时按 `issue.workflow` 存的名字去匹配 `.claude/skills/` 下顶层目录名(整包)或某个 `<name>/SKILL.md` 所在目录名(单技能);两边都命中是仓里同名冲突,属于铺底/导入阶段就该拦住的问题(§2.8),不是解析时该处理的事;两边都不命中,如实跳过、不猜、不记账、不错记到别的目录上,活照常能开工(§4)。

### 2.5 映射三列:`[[mapping]]`

同一份文件,每行一个 `[[mapping]]`(`category`/`tool`/`workflow` 三列):

| category | tool | workflow |
|---|---|---|
| prototype(原型) | open_design | 原型设计 |
| build(构建) | claude_cli | **mattpocock-skills**(用户改定为默认;superpowers 可在活上换) |
| optimize(优化) | claude_cli | mattpocock-skills(同构建默认;活上可换成 diagnosing-bugs/systematic-debugging 这类单技能起手) |
| maintain(运维) | claude_cli | mattpocock-skills(同构建默认) |
| growth(运营推广) | claude_cli | ""(无默认,活上从鱼塘挑) |
| ops(运作) | claude_cli | ""(三张运作活各自指定,不共用一行) |

**用户拍板改动**:build/optimize/maintain 三个类别的默认 workflow 从 superpowers 改成 **mattpocock-skills**(用户拍板;两者都是完整开发工作流,superpowers 仍是下拉里的常规备选,不是被淘汰)。

建活(既有 `Command::CreateIssue`)时按类别标签查这张表,填 `issue.tool`/`issue.workflow` 默认值,活上可再单独换。运作活①②③不查这张表,建活时由 buddy 直接指定固定 workflow 名。

### 2.6 workflow(SOP 类技能包)与单技能:模型

**定义(采纳预研核实版,不变)**:**workflow = 一个技能容器**——满足判据 A(顶层有 `.claude-plugin/plugin.json`)或判据 B(`skills/` 下 ≥2 个独立 `<name>/SKILL.md`)之一,通常有一份「入口」技能(判据 C,弱判定,`disable-model-invocation: true` 且正文引用其他技能名)用正文散文把其余技能串成带分支的流程,**该用哪个 agent 由入口/沿途技能正文临时决定**(现场调用 Claude Code 内置 Agent/Task 工具,常见 `Explore`/`general-purpose`),**不是**包自带一份持久的 agent 人设文件——官方支持这个能力(插件根目录 `agents/`),但预研实读的两个真实包(mattpocock-skills 1.2.0、superpowers 6.1.1)都没用。**单技能**只做一件事,判据 D(没有 `agents/`)在两个真实样本上恒真但不构成有效判据,主要靠"不满足 A/B"判定。**采纳预研,不改动**:结构判据 A/B/D 自动可信;判据 C 只做弱判定。

**不建登记表,判定现场做(取代原「新建 `skill_package` 表」方案)**:预研当时的开放问题 1(要不要给"包"单独建表)已经被母文档 §6.3 更上一层的决定盖过——技能/workflow 整个"登记"这件事本身都不需要持久化。判据 A/B/C 每次要用(▶开工解析 `issue.workflow`、配置屏渲染②③段)都对 `.claude/skills/` 现场扫一遍现场判定,不缓存判定结果、不给"包"分配 id:

- 扫 `.claude/skills/*/`,每个顶层目录各自套一次判据 A/B——命中就是一个 workflow(包),目录名即包名;没命中(该目录下只有一份 `SKILL.md`,没有子技能群)就是一个单技能。
- 包内再套一层判据 C——找 `disable-model-invocation: true` 且正文提到其它技能名的那份 `SKILL.md`,猜作入口;猜不出就是"未标注",**不提供持久的人工覆盖**——`skill.is_entry` 列随 `skill` 表一起取消,这项从"可编辑登记"降级成"只读的现场猜测",呼应"没人取的不存":这份猜测结果本身没有第二处消费者,只在配置屏这一次渲染里用一次,不值得为它单独开一个可写的存储位置。
- "自带 agent 数"照样现读:数一下该包目录下 `agents/*` 有几个文件,两个真实样本都是 0,如实显示。

这条设计与 02 篇 §2.3「技能/workflow 用了几次:现算,不建战绩表」同一个精神——判据 A/B/C 判出来的不是"账",是"结构事实",结构事实永远能从文件重新读出来,不需要另开一张表去缓存一个可以重新计算的结论。

### 2.7 技能怎么到 agent 手里(2026-08-20 试点第一天推翻重写)

**原来定的是「整包物化进 `<worktree>/.claude/skills/`」。试点第一天当场证明这条路
走不通,已经改掉,这一节按新做法重写。**

推翻它的是一次真实接入:buddy 自己的仓把 `.claude/` 写进了 `.gitignore`,铺底复制进去
的十三份 `SKILL.md` **一个都没进第一个 MR**,而界面上还显示铺好了。用户仓怎么写
`.gitignore` 不该由 buddy 决定;何况每个项目复制一份 buddy 自带的东西,本来就没有道理。

**新做法,两句话**:

1. **buddy 自带的技能住在 buddy 自己的资产目录**(`<库文件所在目录>/assets/skills/`,
   代码在 `crates/bw-v4/src/standard/skills.rs`)。开工前展开一次,内容一致就不写。
   **不进任何用户仓**。
2. **系统提示词里只给这张活挂的那一份技能的名字、一句话、完整路径**,正文让 agent
   自己去读;同时把 buddy 的资产目录用 `--add-dir` 声明给 CLI,让它读得到。

这正是渐进式加载,和 buddy 自己的系统提示词(`docs/buddy/system-prompt.md`)一个套路:
提示词里放索引,正文按需读。原来那条「注入护栏上限约 6000 字符,superpowers 单个技能
正文实测均值 8732 字符」的实测数字仍然成立——它只是更不该往里塞整包正文的理由,不再是
「所以必须复制进仓」的理由。

**开工时那份系统提示词一共四段**(读回:`cargo run -p bw-v4 --example prompt_smoke -- <目录>`
把整份打印出来):

| 段 | 内容 | 从哪来 |
|---|---|---|
| 身份与这张活 | 你是谁、活号标题、类别、挂的 workflow | `issue` 行 |
| 铁律 | 最远到「评审中」;合并永远是人;指标读数只能来自真实采集;干砸了如实停 | 固定文本 |
| 规范索引 | `.bw/` 下**这个仓里真有**的那几份,一份一行「路径 —— 一句话」 | `CORE_TEMPLATES` 逐个 `exists()` |
| 本活的剧本 | 名字 + frontmatter 那句 `description` + 完整路径 | buddy 资产目录 |

规范索引**只列真存在的文件**:铺底还没跑的项目一条都不列。列一条不存在的路径,agent
读一次失败一次,比不列更糟。

**项目自有的技能还是在仓里**。蒸馏产出、人手加的、从别处导入的技能,正本仍是项目仓
`.claude/skills/**/SKILL.md` —— 那是项目自己的资产,该进它自己的版本控制。配置屏与
知识库把两个来源合成一张表,`origin` 列如实写「buddy 自带」还是「项目自有」;同名时以
仓里那份为准。

**Cursor 侧对应机制(如实标注:未逐句核对)**:预研只做了一次网页文档摘要抓取,核实到
Cursor 支持从 git 仓库导入 `.mdc` 规则文件("Remote Rules"),但没找到 SKILL.md 风格的
多文件包概念,包级语义大概率会丢。今天 `CURSOR.supported=false`,这条注入机制是**设计
留白**,列进第 6 节开放问题 4。

**Open Design 的注入**:今天嵌入的是通用首页(`op.rs:2041-2118` 的 `<iframe src="{src}/">`),
URL 不带"打开哪个 worktree"的参数(2026-07 V3-OD-embed 的现状,当时只是看一眼原型进度)。
V4 需要它打开这张活的 worktree,具体参数怎么带过去没核实过,列进第 6 节开放问题 3。

### 2.8 铺底/导入的三条路

| 路径 | 怎么做 | 落点 |
|---|---|---|
| 本机目录 | 选一个含 `SKILL.md` 或 `plugin.json` 的本机路径,套 §2.6 判据 A/B 现场判一次是包还是单技能 | 复制进项目仓 `.claude/skills/<name>/`(随 MR 可见) |
| git 仓地址 | `git clone` 到临时路径,复用「本机目录」的判断与落点,临时目录用完即删 | 同上 |
| 从另一个项目 | 选另一个已纳入的项目,列出它仓里 `.claude/skills/` 下的目录(现场扫描它的仓,不查任何表),复制一份(不是引用)进当前项目仓 | 同上 |

**导入的落点是项目仓,只有项目级**:三条路都把技能复制进当前项目仓
`.claude/skills/<name>/`,走一张轻量活 + MR(与 §2.5 保存映射同一条「改仓一律走活+MR」
的规矩)。任何 committer clone 下来都能看到这个项目自己有哪些技能。

**这和 §2.7 说的「buddy 自带的技能不进用户仓」不矛盾,两者是两批东西**:

| | buddy 自带的十三份 | 项目自己导入/蒸馏的 |
|---|---|---|
| 正本在哪 | 编在 buddy 二进制里,展开到 buddy 的资产目录 | 项目仓 `.claude/skills/` |
| 进不进用户的版本控制 | 不进 | 进,随 MR 可见 |
| 每个项目一份吗 | 不,全局一份 | 是,各是各的 |
| agent 怎么拿到 | 系统提示词给路径 + `--add-dir` | CLI 在仓里原生发现 |

原来那条「Claude CLI 只在项目仓里找技能,所以不进项目仓的导入对▶开工毫无用处」的推理
**已经不成立**:给绝对路径 + `--add-dir` 一样读得到,这是 §2.7 那次推翻的直接结果。
它现在只对**项目自己的**技能成立——那些本来就该待在项目仓里。

### 2.9 workflow / 技能用了几次:现算,不记战绩

母文档 §6.3 与 02 篇 §2.3 已经把这件事定了性——不只是"记账主体从 agent 换成 workflow"这么简单,是**整个"战绩"账本概念本身被取消**:"干没干成"不再由 buddy 自己判定和记账,看的是**远端 MR 合没合入**,这条判据造不了假。库里因此不需要 `workflow_credit` 表,也不需要 `outcome`/`settled_at` 这类结算事件,不需要在 Done 边或 run 失败两处挂一次记账动作。

"用过几次"完全现算,与 02 篇 §2.3 同一条查询,04 篇不另设计一套:

```sql
SELECT workflow, COUNT(*) AS uses
FROM issue WHERE project_id = ? AND kind = 'business' AND workflow != ''
GROUP BY workflow;
```

配置屏「用过几次」就是这条查询的结果,每次现查,不缓存汇总数——这条纪律和「健康信号只能从数据推导、绝不手设」(CLAUDE.md「健康永远推导」)同一个精神。

**不再显示胜率**:没有 `outcome` 记录,`wins`/`win_rate` 这两样随 `workflow_credit` 表一起消失,配置屏②③两段直接不放"胜率"这一列,不是显示"暂无"占位——因为连"暂无"背后那套判定逻辑(结算事件)都不存在了。

**`skill.uses` 这类旧计数机制一并取消**:它依附在已删除的 `skill` 表上(`record_skill_use`,`sqlite.rs:2192-2199`);`issue.workflow` 单列 + `GROUP BY` 已经是唯一需要的计数口径,不需要额外在技能行上维护一个 `uses` 计数器。

**挂载点消失**:`dispatch.rs` 的 `TransitionIssue` Done 边、`finalize_run_interactive` 里原来打算挂的"结算时插入一行"动作,V4 不需要——没有战绩表可插。

**回填的活也会被计入用量**:02 篇 §2.3 的查询按 `kind='business' AND workflow != ''` 过滤,没有排除 `origin='backfill'`——历史上真跑过某个 workflow 的活,回填后一样该算进"用过几次",04 篇采纳这个口径,不再像战绩年代那样把回填活单独摘出来不计数。

### 2.10 `agent` 表:不建(与 02 篇对齐措辞)

**已定(与 02 篇 §2.3/§2.7 对齐)**:V4 新库 `schema.sql` 里**从未出现过** `agent` 表——不是"先建后迁移删除",是新库从第一天起就没有这张表这回事,连同 `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` 三条命令与 `agent_import.rs` 整个模块一起不存在(01 篇已定为不建,04 篇细化)。理由:

1. **CLAUDE.md 既定原则**——「发现过时的实现路径,直接移除它」:V1-V3「队友」这套记账机制依附的聊天式旧引擎已经在 2026-08-18 那次减负里被拆掉了大半,`agent` 表撑的是一套已经不存在的执行模型,留着只读本身就是一条没人再写、迟早被遗忘的旧路径。
2. **V4 的活不再"指派给队友",而是"配开工工具 + workflow"**——`issue.assignee`(AgentId 外键)同步失去语义,新库 `issue` 表定义里**不出现**这一列(与 02 篇 §2.3 一致)。
3. **这是一次数据丢失决定**(存量 V1-V3 项目的队友战绩历史不迁移,不可逆)——但因为 V4 本身就是"不兼容老库、新壳用新库文件"(§2.7=02 篇 §2.7)这个更大决定,这条丢失只是它的自然结果,不是 04 篇另外单独做的一个取舍,已提请用户点头(握手清单 第 2 条,与 02 篇 §2.3 同一处引用)。

存量战绩要不要在切换前做一次性人可读导出,列进第 6 节开放问题 2,不在本篇拍板。

### 2.11 配置屏四段:数据来源一览

| 段 | 内容 | 数据来源 |
|---|---|---|
| ①开工工具映射 | 类别→工具→workflow 三列表 | `.bw/issue-policy.toml` 的 `[[mapping]]`(§2.5),保存走 `SaveToolMapping` |
| ②workflow 表 | 名称/入口/自带 agent 数/用过几次(**不再有"胜率"列**) | 现场扫描 `<worktree>/.claude/skills/`,按 §2.6 判据 A/B/C 分类出的包目录;"用过几次"按 02 篇 §2.3 的 SQL 从 `issue.workflow` 现算,不缓存;"自带 agent 数"现数该目录 `agents/*` 文件数 |
| ③skill 表 | 单技能名称/用过几次 | 同一次扫描里没有命中判据 A/B 的顶层技能目录 |
| ④连接器+定时 | codehub/GitHub/项目群 + 定时任务 | 连接器地址在 `.bw/project.toml`,连不连得通是即时探活、不存结果(02 篇 §2.1/§2.6,`connector` 表已取消);定时只有一档(资产盘点,周五晚,写在 `.bw/issue-policy.toml` 的 `[cadence]` 段,判据现查本周有没有这张活,`cron_task` 表已取消);项目群一行见 [07 篇](07-notify-and-chat-group.md),配置读 `.bw/project.toml` 的 `[chat]` 段(02 篇 §2.4/§2.6),不做发送去重(`chat_outbox` 表已取消)。**这块里还有一张「连接器」表是孤儿**:它读 `.bw/connectors.toml` 摆出来给人看,而 V4 没有任何采集在消费它(采集按 [14 篇](14-metrics-collection.md) 只认脚本 / 手填两种,不经「连接器」这一层)。这张表要删,记在 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) |

---

## 3 · 工程对照

### 3.1 `Command` 增量(只列名字和一句话,与 [01 篇](01-architecture.md) 已列的对齐、不重复)

| 命令 | 一句话 | 标注 |
|---|---|---|
| `ImportSkillPackage{source_path,project_id}` | 单目录导入,套 §2.6 判据 A/B 现场判一次是包还是单技能,复制进项目仓 `.claude/skills/<name>/`;**不写任何库表**,走一张轻量活+MR | 沿用,改为纯文件操作 |
| `ImportSkillLibrary{root_path,project_id}` | 库根批量扫描,每个顶层目录各自套一次判据 A/B,全部复制进项目仓 `.claude/skills/`;**不写任何库表** | 沿用,改为纯文件操作 |
| `SetIssueWorkflow{id: IssueId, workflow: String}` | 活详情面板换 workflow/单技能,写 `issue.workflow`(字段名与 06 篇一致) | 新 |
| `SaveToolMapping{project_id, category, tool, workflow}` | 配置屏第①段保存一行映射,写回 `[[mapping]]`(走活+MR 还是直接写,§6 开放问题;字段名统一为 `workflow`) | 新 |
| `ProbeTool{name: String}` | 手动探活一次(配置屏/项目墙"测一下"复用) | 新 |
| `MarkEntrySkill{skill_id, package_id}` | 退场(§2.6):没有 `skill`/`skill_package` 表就没有稳定 id 可指,入口技能改成每次现场用判据 C 猜,猜不出就是"未标注",不提供持久人工覆盖 | 删除 |
| `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` | 不建(§2.10) | 不存在 |

### 3.2 `Event` 增量

| 事件 | 一句话 |
|---|---|
| `ToolProbed{name, ok, reason}` | 一次探活的真实结果,配置屏/项目墙刷新用 |
| `SkillsCopiedIntoRepo{path, skill_count}` | 一次导入(本机目录/git 仓/另一个项目)真实把文件复制进项目仓 `.claude/skills/` 完成——取代原「整包落库」的 `SkillPackageImported` 事件,因为没有库行可落 |
| `IssueWorkflowChanged{id}` | 某活的 workflow/工具真实改了 |
| `ToolMappingSaved{category}` | 某一行映射真实保存完成 |

**不再有 `WorkflowRunCredited` 事件**(取消,见 §2.9)——战绩不记账,没有"一次结算真实发生"这件事可广播。

### 3.3 数据模型增量(与 02 篇分工:`issue` 表其余新列如 `week_of`/`version`/`kind`/`origin` 归 02 篇,推动指标归 `issue.metric_key` 单列(02 篇 §2.2,不是关联表),这里只交代 `tool`/`workflow` 两列 + 本篇涉及的表/列取舍)

```
issue.tool      TEXT NOT NULL DEFAULT ''   -- 'claude_cli' | 'cursor' | 'open_design'
issue.workflow  TEXT NOT NULL DEFAULT ''   -- workflow/技能名,§2.4 按目录名匹配解析,不是 uuid

skill_package / skill / skill_file / skill_stage / workflow_credit
  -- 均不建(§2.6/§2.9,与 02 篇 §2.1 的删除清单同一批,不是本篇单独决定)

agent 表 · agent_import.rs · CreateAgent/UpdateAgent/ImportAgentDefinition · issue.assignee
  -- 均不建/不出现(§2.10,与 02 篇 §2.3 对齐)
```

迁移守则不变(CLAUDE.md 纪律 5):每加一列同步改 `schema.sql` 并加 `add_column_if_missing`(开发期例外见 02 篇 §2.7/§3.2);新表 `CREATE TABLE IF NOT EXISTS` 已是充分守卫。

### 3.4 与 [01 篇](01-architecture.md) `adapters/` 的接缝

`claude_cli`/`cursor`/`open_design` 三个适配模块的 `README.md`(借自哪、借了什么、没借什么)按本篇 §2.1 的三种 `probe` 填:`claude_cli` 借 `claude_bin.rs` 的路径候选算法;`cursor` 借 `cursor-agent-executor.md` 设计稿;`open_design` 借 `open_design.rs` 的 socket 握手算法。`adapters/claude_cli`/`adapters/cursor` 内部直接复用 `interactive_cli.rs` 的 `TuiAgentConfig`/`build_startup_plan`,不重写。

---

## 4 · 边界与失败

**不做的事**:不建 agent 名单(workflow 包自己决定用哪个内置子代理,"自带 agent 数"如实显示实测的 0)——**用户原话定性**:agent 暂不作为单体维护,小事单技能干,大事 workflow 带着自己的 agent 与脚本干(握手清单 第 2 条);不做技能市场界面(鱼塘只在配置屏走 §2.8 导入,不做浏览/搜索 UI);不整体嵌 DSH(deepseek-harness 结论已定,只借三条接口判断;将来接 DSH 一类网页 agent 走新增一条 `web_embed` 声明,和接 Open Design 同一条路);不塞整包进系统提示词(§2.7 实测数字已堵死这条路);不建技能/workflow 用量或战绩登记表(§2.6/§2.9,现算)。**workflow 自己产的文档不额外管**(用户拍板,待拍-10 改):mattpocock-skills、superpowers 这类业界包物化进项目仓后,会往仓里写自己的东西(研究笔记、领域模型、决策记录……),这些内容与 buddy 规范的知识库天然有重叠——**MVP 不过度设计**,规范只约束 buddy 自己必需的基础限制(`.bw/PROJECT.md`/`AGENTS.md`(仓根)、`.bw/*`、`.bw/plan/`、`.bw/releases.md`),workflow 自产的目录不管、不收编、不搬家;知识库「知识」页签把它们当普通仓内文档树展示;运作活②(资产盘点)盘点时只登记不整理;第一版试点实践后再看要不要收编。

**失败如实显示,不假装**:工具未装(探活失败)→ 灰态+"怎么装→",不隐藏该行;导入的目录/仓不满足判据 A/B → 如实按单技能导入并提示"看起来不是 workflow 包";`issue.workflow` 名字在 `.claude/skills/` 里解析不到 → 如实跳过、不猜、不错记,活照常能开工;Open Design 打开通用首页而非本活 worktree(§2.7 开放问题)→ 中栏标注"未定位到本活工作区(设计中)";Cursor 今天 `supported=false` → 下拉里仍出现,选中后▶开工如实报错"Phase 1 仅 Claude CLI",不从下拉里拿掉。

---

## 5 · 验收与读回

1. **导入 superpowers 后仓里出现整包,目录树核验**(取代原来查 `skill_package`/`skill` 两张表的验收——这两张表已经不存在,技能正本在仓不在库,验收也就从"查库"改成"看仓"):
   ```bash
   ls <buddy 资产目录>/skills/
   find <buddy 资产目录>/skills -name SKILL.md | wc -l   # 该是 13
   ```
   预期:能看到 `.claude-plugin/plugin.json`(判据 A 命中)或 `skills/` 下多个技能目录(判据 B);`SKILL.md` 文件数应为 14(预研实读 superpowers 14 个技能)。mattpocock-skills 同理,预期 22 个 `SKILL.md`。
2. **开工时给 agent 的是索引不是正文**(§2.7):`cargo run -p bw-v4 --example prompt_smoke -- <目录>` 一次跑完打印完整的系统提示词 —— 13 份技能全落在 buddy 自己的资产目录、用户仓里没有 `.claude/skills/`、提示词里那条技能路径真是个文件、技能正文最长那行在提示词里找不到。
3. **"用过几次"现算,建活即变、不等 Done**(取代原「Done 后战绩 +1」验收——战绩概念已取消,§2.9):
   ```sql
   -- 建一张挂 workflow='superpowers' 的业务活前后各查一次
   SELECT COUNT(*) AS uses FROM issue
   WHERE project_id=? AND kind='business' AND workflow='superpowers';
   ```
   预期:`CreateIssue`(或 `SetIssueWorkflow`)把某张活的 `workflow` 设成 `superpowers` 后,这条查询立刻 `+1`,不需要等它走到「评审中」或「完成」——这条数字统计的是"挂了这个 workflow 的活有几张",不是"成功了几次"。反复 `TransitionIssue` 推拖同一张活的状态,这个数字**不应该变化**(它跟状态无关),证明"战绩记账"这个挂载点真的已经从状态机上摘掉了。
4. **探活如实**:本机没装 Cursor 时 `ProbeTool{name:"cursor"}` 应返回 `ok=false`;卸载/改名 `claude` 二进制后重探也应翻灰,不应有"曾装过所以现在也算装了"的假阳性。
5. **深链 `BW_PANEL=config` 截图**:`[BW_OPEN]` 证据行打出后,截图应看到四段——①映射三列表格②workflow 表(数据来自现场扫描 `.claude/skills/`,不是查表;应看到 superpowers/mattpocock-skills/三张运作 workflow/原型设计 workflow,每行只有名称/入口/自带 agent 数/用过几次四列,**没有胜率列**)③skill 表(同一次扫描里没有命中包判据的技能,如 grillme、buddy 自带 9 篇)④连接器+定时小段。

---

## 6 · 开放问题

1. **`.bw/issue-policy.toml` 改动怎么落盘**:母文档"写仓一律走活+MR"的规矩严格照办就该走轻量活(类比 `EditProjectCard`),但这份文件调参可能相当高频,每次都建活走 MR 会不会太重?本篇倾向"走活+MR",但交由用户按试点体感定,或退一步先直接写本机、下次运作活②的资产盘点里被动核对。
2. **V1-V3 存量项目的队友战绩历史,要不要在切换到 V4 前做一次性人可读导出**:§2.10 的立场是接受不迁移、直接不建表,但这是不可逆操作——是否值得先跑一次导出脚本,把每个存量项目的战绩写成一段文字留个人可读的历史存根?落在哪份仓文件里也还没定(**不是** `.bw/plan/history.md`——02 篇 §2.5 已明确没有这份单独文件,历史周与本周混在同一个 `.bw/plan/` 目录里),需要另找地方或者干脆放弃导出。
3. **Open Design 怎么定位到具体活的 worktree**:今天嵌入的是通用首页,V4 需要活级路由,机制(URL query、还是走 socket 协议加新消息类型)没核实过,需要一次穿刺确认。
4. **Cursor 侧 workflow/技能注入的真实机制**:§2.7 给 claude 的那套(系统提示词给路径 + `--add-dir`)不一定适用 Cursor —— 它有没有等价的「加一个可读目录」开关没核实过,需要 Cursor 真正接通那天补一次真实穿刺。
5. **判据 C(入口技能)猜不出时要不要给一个人工覆盖的落点**:现在的立场是"不提供"(§2.6,因为没有稳定 id 可挂);如果试点发现"未标注"太影响体验,需要另找一个持久化位置——比如约定 SKILL.md 自己的 front matter 加一个字段,而不是回头再为这一项小展示开一张表。

---

## 与代码的关系

这篇不改 `crates/`。开工顺序建议:①`.bw/issue-policy.toml` 的 `[[tool]]`/`[[mapping]]` 解析器(`bw-app` 里的小模块,不需要新 crate);②`.claude/skills/` 现场扫描 + 判据 A/B/C 判定模块(取代原计划的 `skill_package`/`skill` 新表,§2.6);③`ImportSkillPackage`/`ImportSkillLibrary` 改成纯文件复制 + 现场判定,不再写库(§2.8);④buddy 自带技能的展开与指路(`standard::skills` + `App::ensure_skill_assets`,§2.7 —— **已落地**;原计划是给 `skill_materialize` 补「按包目录整体一次性物化」,那条路已推翻);⑤配置屏②③段改成基于扫描结果现场渲染,不再查表(§2.11);⑥`agent` 表不建(含 `issue.assignee` 不出现,与 02 篇协调,§2.10)。第 5 节就是这条链路的验收清单。
