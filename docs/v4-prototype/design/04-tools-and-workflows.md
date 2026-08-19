# 04 · 开工工具与 workflow 怎么接

> **30 秒导读**:这篇回答两件事——**开工工具怎么注册与分发**(终端类如 Claude CLI/Cursor,与本机网页内嵌类如 Open Design,怎么声明、探活、起、停),**workflow(SOP 类技能包)与单技能怎么识别、导入、存、物化到活的 worktree、注入给开工工具**。母文档待拍-09/10/24 的落地稿。给下一步写代码的会话看,也给要往规范里贡献技能包的同事看。**现在作数,待用户复核,尚未开工写代码**。会话屏三栏怎么摆是 [05 篇](05-session-screen.md)的事,三张运作 workflow 的正文是 [09 篇](09-ops-workflows.md)的事。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。文内代码路径都是真读出来的,不是猜的。

---

## 0 · 这篇管什么、不管什么

**管**:母文档第 4 站「执行:每张活按类别选开工工具」的映射表、「agent 去哪了」段、待拍-09/10/24;[`../../standard-module-draft.md`](../../standard-module-draft.md) 第 5 类(`.bw/issue-policy.toml`)与第 6 类(默认件与鱼塘);预研 [`../../research/workflow-skill-packages.md`](../../research/workflow-skill-packages.md)、[`../../research/deepseek-harness.md`](../../research/deepseek-harness.md)。具体是:①`[[tool]]` 怎么声明一个开工工具、新增工具要改哪里;②▶开工按 `tool` 字段怎么路由到 [01 篇](01-architecture.md) 的 `adapters/*` 模块;③workflow/单技能怎么建模、导入、物化;④映射三列(`类别→工具→workflow`)怎么落文件落库;⑤战绩从「记在 agent」搬到「记在 workflow/技能」;⑥配置屏四段的数据来源;⑦相关命令/事件,只列名字。

**不管**:会话屏交互细节、终端标签页、代码结构侧栏(05 篇);运作 workflow 正文(09 篇);`issue-policy.toml` 之外的仓文件/库 schema 增量(02 篇);规范铺底流程与老项目历史回填(03 篇);codegraph 子命令(05 篇提,这篇只说它是 `adapters/codegraph` 模块,不是开工工具)。

---

## 1 · 用户看到什么、做什么

**配置一次开工方式**:配置屏第①段「开工工具映射」是一张表——六行(原型/构建/优化/运维/运营推广/运作)三列(类别/工具/workflow)。铺底时已按规范默认值填好(原型→Open Design→原型设计 workflow;构建/优化/运维→Claude CLI→superpowers 或 mattpocock-skills 二选一;运营推广→Claude CLI→空,提示"从鱼塘挑";运作→Claude CLI→三张自建运作 workflow)。想换(比如把某类改用 Cursor),下拉选一下、保存。

**导入一个新 workflow 包**:第②段「workflow 表」右上「导入」,三选一(本机目录/git 仓地址/从另一个项目)。buddy 探测这是不是一个「包」(有没有 `plugin.json`、`skills/` 下是不是多个技能目录),是就整包导入进表(名称/来源/入口/自带 agent 数/用过几次),不是就退回单技能、进第③段「skill 表」。猜不出入口技能就显示"未标注",可手动点一个补标。

**给一张活换开工方式**:活详情面板「开工工具」「workflow」两个下拉,默认来自映射表,活上能单独换(比如这张活想只用 grillme 打磨需求,不走整套 superpowers)。

**点▶开工**:终端类(Claude CLI/Cursor)→ buddy 把选中的 workflow(或单技能)整包写进这张活 worktree 的 `.claude/skills/`,起嵌入终端,agent 自己发现并读;本机网页内嵌类(Open Design)→ 探活成功就在会话屏嵌一个 iframe,失败就灰态"未装·怎么装"。活到「评审中」再到人点「完成」的那一刻,挂的 workflow(或单技能)战绩 `+1`——配置屏「用过几次」和只读胜率就是这条战绩读出来的,不是人手填的。

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
3. `terminal`(Claude CLI/Cursor):解析 `issue.workflow`(§2.4)→ 物化技能到 `<worktree>/.claude/skills/`(§2.7)→ 按工具名选 `TuiAgentConfig` → `build_startup_plan`(沿用,`interactive_cli.rs:219`)起 PTY 会话 → 出现在会话屏左列该 worktree 分组下。
4. `web_embed`(Open Design):按 socket 探活拿 URL,拿到就中栏开标签页 iframe(注入方式见 §2.7 开放问题),拿不到就灰态"未装·怎么装",活停在原地不算开工失败。

停止:终端类走既有 `Command::CancelRun`(不变);网页内嵌类的"停"只是关掉标签页——它本来就不是 buddy 起的子进程,不产生运行记录(§4 再说明这条边界)。

### 2.4 `issue.tool` / `issue.workflow`:存名字,按就近优先解析

`issue.workflow` 存**名字**,不是 uuid——沿用 `issue.standard_skill` 已在用的模式(`schema.sql:472-475`:「stable Skill-Hub slug」,按名关联)。这样活详情面板换 workflow 不用先查 id,`docs/plan/YYYY-Www.md` 里读到的就是可读名字。

解析规则**照抄** CONTEXT.md 已有的「就近优先 / Most specific wins」规则(技能注入同一条规则,不新造):先查本项目的 `skill_package`/`skill` 行,查不到再查全局(`project_id IS NULL`)行,项目行遮蔽全局同名行;两边都查不到,如实记「名字对不上,不记账」,绝不错记到别的行上。

### 2.5 映射三列:`[[mapping]]`

同一份文件,每行一个 `[[mapping]]`(`category`/`tool`/`workflow` 三列):

| category | tool | workflow |
|---|---|---|
| prototype(原型) | open_design | 原型设计 |
| build(构建) | claude_cli | superpowers |
| optimize(优化) | claude_cli | superpowers |
| maintain(运维) | claude_cli | superpowers |
| growth(运营推广) | claude_cli | ""(无默认,活上从鱼塘挑) |
| ops(运作) | claude_cli | ""(三张运作活各自指定,不共用一行) |

建活(既有 `Command::CreateIssue`)时按类别标签查这张表,填 `issue.tool`/`issue.workflow` 默认值,活上可再单独换。运作活①②③不查这张表,建活时由 buddy 直接指定固定 workflow 名。

### 2.6 workflow(SOP 类技能包)与单技能:模型

**定义**(采纳预研核实版):**workflow = 一个技能容器**——满足判据 A(顶层有 `.claude-plugin/plugin.json`)或判据 B(`skills/` 下 ≥2 个独立 `<name>/SKILL.md`)之一,通常有一份「入口」技能(判据 C,弱判定,`disable-model-invocation: true` 且正文引用其他技能名)用正文散文把其余技能串成带分支的流程,**该用哪个 agent 由入口/沿途技能正文临时决定**(现场调用 Claude Code 内置 Agent/Task 工具,常见 `Explore`/`general-purpose`),**不是**包自带一份持久的 agent 人设文件——官方支持这个能力(插件根目录 `agents/`),但预研实读的两个真实包(mattpocock-skills 1.2.0、superpowers 6.1.1)都没用。**单技能**只做一件事,判据 D(没有 `agents/`)在两个真实样本上恒真但不构成有效判据,主要靠"不满足 A/B"判定。**采纳预研,不改动**:结构判据 A/B/D 自动可信;判据 C 只做弱判定,导入后允许人工补标"入口技能"。

**表结构建议(回答预研开放问题 1)**:新建 `skill_package` 表,不是只在 `skill` 加一列分组字段。理由:①配置屏「workflow 表」要的信息(名称/来源/入口/用过几次/胜率)是**包级**事实——只加一列的话 mattpocock-skills 22 个技能行各自维护一份包级战绩,`rev`/`updated_at` 会漂移,记账要小心同步 22 行而不是 1 行;②延续 schema.sql "一个概念一张表"的既有约定(`workflow_spec`/`skill_file` 都是这个模式);③`skill.package_id`(可空 FK)做归属:`NULL`=单技能(grillme、buddy 自带的 9 篇 `docs/skills/*`),非空=某包成员。

```sql
CREATE TABLE IF NOT EXISTS skill_package (
    id                TEXT PRIMARY KEY,
    name              TEXT NOT NULL,                    -- "superpowers"/"mattpocock-skills"/"原型设计"
    project_id        TEXT REFERENCES project(id),      -- NULL=全局/个人导入,非空=项目自有(§2.8)
    source            TEXT NOT NULL DEFAULT 'imported',  -- 'builtin' | 'imported'
    official_library  TEXT NOT NULL DEFAULT '',
    entry_skill_id    TEXT REFERENCES skill(id),         -- 判据 C 弱判定 + 人工补标,允许空
    pkg_version       TEXT NOT NULL DEFAULT '',          -- 来源包自带版本号,导入时钉住(如 "1.2.0")
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, rev INTEGER NOT NULL DEFAULT 0
);
-- skill 表加两列(双守卫:add_column_if_missing,同 skill.content/stage_origin 的模式):
--   package_id TEXT REFERENCES skill_package(id)  -- NULL=单技能
--   is_entry   INTEGER NOT NULL DEFAULT 0           -- 人工标记的入口技能
-- 战绩不存列(设计期统一:以 02 篇 workflow_credit 台账表为事实源,一活一主体
-- 一行,唯一键保证绝不记两次)——runs/wins/win_rate 不落在 skill_package/skill
-- 上,配置屏渲染时由 SQL 从 workflow_credit 现算(视图或查询,见 §2.9)。
```

导入时(`ImportSkillLibrary` 批量扫描库根)一次性判断结构:命中 A 或 B,先插一行 `skill_package`,再把扫到的每个技能各自插进 `skill` 并把 `package_id` 指过去;不满足就照今天的行为——各自独立、`package_id` 留空。判据 C 落空 `entry_skill_id` 留空,配置屏显示"未标注",提供 `Command::MarkEntrySkill` 手动补。

### 2.7 物化与注入

▶开工时,按 `workflow` 解析出的技能集合(整个包或单个 `skill`)**整包一次性物化**到 `<worktree>/.claude/skills/`——CLI 原生发现路径,**不塞进系统提示词**。理由是实测数字已把这条路堵死:`--append-system-prompt` 单条注入护栏上限约 6000 字符,superpowers 单个技能正文实测均值 8732 字符(2026-08-05 实测,`skill_materialize.rs` 文档注释),一整包更不可能塞进一个字符串参数。

**沿用现有机制,补一层"保留包边界"**:今天 `skill_materialize::materialize_stage_skills` 已有整包物化的底子——SKILL.md + 支撑文件都能落盘,`.bw-managed` 标记(内容 `skill.id\nskill.rev`)防覆盖用户手写同名目录,冲突记进 `skipped_foreign`。V4 要补的是:按 `skill.package_id` 一次查出整包成员,一次性物化,而不是像今天这样各管各的。

**buddy 系统提示词(第 0 层)不受影响**:`--append-system-prompt` 仍承载衔接层(Bridge prompt)——产出格式契约与铁律,不再往里塞 workflow 整包正文。呼应母文档渐进加载五层:第 0 层系统提示词 → 第 1 层仓内 `AGENTS.md` → **第 2 层本活技能(就是本节说的物化这一步)** → 第 3 层规范件按需 → 第 4 层项目知识。

**Cursor 侧对应机制(如实标注:未逐句核对)**:预研只做了一次网页文档摘要抓取,核实到 Cursor 支持从 git 仓库导入 `.mdc` 规则文件("Remote Rules"),但没找到 SKILL.md 风格的多文件包概念,包级语义大概率会丢。今天 `CURSOR.supported=false`,这条注入机制是**设计留白**,列进第 6 节开放问题 4。

**Open Design 的注入**:今天嵌入的是通用首页(`op.rs:2041-2118` 的 `<iframe src="{src}/">`),URL 不带"打开哪个 worktree"的参数(2026-07 V3-OD-embed 的现状,当时只是看一眼原型进度)。V4 需要它打开这张活的 worktree,具体参数怎么带过去没核实过,列进第 6 节开放问题 3。

### 2.8 导入的三条路

| 路径 | 怎么做 | 落点 |
|---|---|---|
| 本机目录 | 选一个含 `SKILL.md` 或 `plugin.json` 的本机路径,走 `ImportSkillPackage`(单技能)或 `ImportSkillLibrary`(库根) | 项目级→复制进项目仓 `.claude/skills/<name>/`(随 MR 可见);全局/个人→只落 buddy 本机表(`project_id` NULL)|
| git 仓地址 | `git clone` 到临时路径,复用「本机目录」的判断与落点,临时目录用完即删 | 同上 |
| 从另一个项目 | 选另一个已纳入的项目,列出它仓里 `.claude/skills/` 下的包/技能,复制一份(不是引用)进当前项目仓 | 只支持项目级 |

**采纳预研的推荐**:复用"资产在仓"原则——项目级落进项目仓,随 MR 可见;全局/个人留在 buddy 本机表,但接受"别的 committer 看不到"的代价——与 V4"仓是正本、库只记账"的既定分工一致,配置屏导入弹窗需要如实提示这一句。

### 2.9 战绩:从 agent 搬到 workflow / 技能

**设计期统一(与 02 篇 §2.4 对齐):以 `workflow_credit` 台账表为事实源,不在 `skill_package`/`skill` 上存 `runs`/`wins`/`win_rate` 列**——一件活一个主体一行,`UNIQUE(subject_kind, subject_id, issue_id)` 保证同一 workflow(或技能)对同一件活最多记一次,由数据库物理拒绝第二次插入,不依赖应用代码小心检查(建表 DDL 见 02 篇 §2.4)。「胜率永远派生,不缓存手设」与「健康信号只能从数据推导」同一条精神:

```sql
-- 结算时插入一行(02 篇 §2.4 DDL),不是 UPDATE 累加列
INSERT INTO workflow_credit (id, project_id, subject_kind, subject_id, issue_id, outcome, settled_at)
VALUES (?, ?, 'workflow', ?, ?, ?, ?);

-- 配置屏「用过几次」「胜率」现算,不读缓存列
SELECT subject_id, COUNT(*) AS uses, SUM(outcome='done') AS wins
FROM workflow_credit WHERE subject_kind='workflow' GROUP BY subject_id;
-- 裸单技能(subject_kind='skill')同款查询
```

**挂载点不变**:`dispatch.rs` 的 `TransitionIssue` Done 边(`newly_done`)与 run 失败两处(`finalize_run_interactive`),"同一件活绝不记两次"这条判据原样复用——主体从 `credited_agent`(按 `issue.assignee`)换成 `credited_workflow`(按 §2.4 解析 `issue.workflow`)。取消/重开不重复记账,沿用既有注释精神("dropping an issue is not evidence…")。

`skill.uses`(既有,`record_skill_use`,`sqlite.rs:2192-2199`)不受影响、继续存在:它记"被物化/引用了几次",`workflow_credit` 记"跟着它的活成功了几次"——一个包被物化时,包内每个成员技能的 `uses` 各自照常 `+1`,但只有包本身在 `workflow_credit` 上记一行(`subject_kind='workflow'`)。

**回填的活不进战绩**:`origin=backfill` 的 issue(待拍-27,03 篇细化)Done 边直接跳过记账——它们是照抄远端历史,不是 buddy 里真跑出来的活。

### 2.10 `agent` 表去留(回答预研开放问题 4)

**已定(设计期统一):硬删,同一次改动里 `DROP TABLE IF EXISTS agent`**,连带 `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` 三条命令与 `agent_import.rs` 整个模块一起移除(01 篇已定为硬删,04 篇细化)。理由:①**CLAUDE.md 既定原则**——「发现过时的实现路径,直接移除它」,待拍-24 已明确"配置里也没有 agent 表",留着只读本身就是一条没人再写、迟早被遗忘的旧路径;②**冻结的战绩比没有战绩更容易误导**——`record_agent_run` 停调后 `runs`/`wins`/`win_rate` 永远停在退役那一刻,正是 CONTEXT.md「空壳」词条警告的"看着像库存实际用不了";③`issue.assignee`(AgentId 外键)同步失去语义——V4 的活不再"指派给队友",而是"配开工工具+workflow",同一次迁移物理删除该列(设计期统一,与 02 篇一致,见 02 篇 §2.4)。

**权衡与不可逆性**:这是一次数据丢失操作(存量 V1-V3 项目的队友战绩历史会消失)。列进第 6 节开放问题,不在本篇直接拍板——建议用户二选一:①接受丢失,直接 `DROP`;②退役前跑一次性导出,把每个存量项目的战绩写成一段文字追加进该项目 `docs/plan/history.md`(03 篇「历史回填」同一个文件),库里的表照样删。

### 2.11 配置屏四段:数据来源一览

| 段 | 内容 | 数据来源 |
|---|---|---|
| ①开工工具映射 | 类别→工具→workflow 三列表 | `.bw/issue-policy.toml` 的 `[[mapping]]`(§2.5),保存走 `SaveToolMapping` |
| ②workflow 表 | 名称/来源/入口/自带 agent 数/用过几次 | `skill_package`(§2.6);"用过几次"/胜率由 SQL 从 `workflow_credit` 现算(§2.9,视图或查询,不存列);"自带 agent 数"如实显示 0 |
| ③skill 表 | 单技能名称/来源/用过几次 | `skill` 表里 `package_id IS NULL` 的行 |
| ④连接器+定时 | codehub/GitHub/项目群 + 定时任务 | 沿用既有连接器/`cron_task` 表;项目群一行见 [07 篇](07-notify-and-chat-group.md)(尚未落笔,先引用母文档 §6 `chat_outbox` 与 §2.6 项目群行) |

---

## 3 · 工程对照

### 3.1 `Command` 增量(只列名字和一句话,与 [01 篇](01-architecture.md) 已列的对齐、不重复)

| 命令 | 一句话 | 标注 |
|---|---|---|
| `ImportSkillPackage{source_path,project_id,official_library}` | 单目录导入(探到包结构自动升级为整包导入);字段不变 | 沿用,语义扩展 |
| `ImportSkillLibrary{root_path,official_library,project_id}` | 库根批量扫描,命中判据 A/B 的目录群组落一行 `skill_package` | 沿用,语义扩展 |
| `SetIssueWorkflow{id: IssueId, workflow: String}` | 活详情面板换 workflow/单技能,写 `issue.workflow`(设计期统一:字段名从早期 `workflow_ref` 改成 `workflow`,与 `issue.workflow` 列名对齐,06 篇同步) | 新 |
| `SaveToolMapping{project_id, category, tool, workflow}` | 配置屏第①段保存一行映射,写回 `[[mapping]]`(走活+MR 还是直接写,§6 开放问题;字段名统一为 `workflow`) | 新 |
| `ProbeTool{name: String}` | 手动探活一次(配置屏/项目墙"测一下"复用) | 新 |
| `MarkEntrySkill{skill_id, package_id}` | 人工补标"这是入口技能" | 新 |
| `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` | 退场(§2.10) | 删除 |

### 3.2 `Event` 增量

| 事件 | 一句话 |
|---|---|
| `ToolProbed{name, ok, reason}` | 一次探活的真实结果,配置屏/项目墙刷新用 |
| `SkillPackageImported{package_id, name, skill_count}` | 一次整包导入真实落库完成 |
| `IssueWorkflowChanged{id}` | 某活的 workflow/工具真实改了 |
| `ToolMappingSaved{category}` | 某一行映射真实保存完成 |
| `WorkflowRunCredited{workflow, win: bool}` | 一次战绩记账真实发生(Done 边或 run 失败) |

### 3.3 数据模型增量(与 02 篇分工:`issue` 表其余新列如 `week_of`/`version`/`kind`/`origin` 归 02 篇,推动指标归 `issue_metric` 关联表(02 篇 §2.2),这里只交代 `tool`/`workflow` 两列 + 本篇新增的表/列)

```
issue.tool      TEXT NOT NULL DEFAULT ''   -- 'claude_cli' | 'cursor' | 'open_design'
issue.workflow  TEXT NOT NULL DEFAULT ''   -- workflow/技能名,§2.4 就近优先解析,不是 uuid

skill_package(新表,§2.6 完整定义,不含 runs/wins/win_rate)
skill.package_id / is_entry   -- 均 add_column_if_missing;战绩不落列,见 §2.9(workflow_credit)

agent 表 · agent_import.rs · CreateAgent/UpdateAgent/ImportAgentDefinition   -- 整体退场(§2.10)
```

迁移守则不变(CLAUDE.md 纪律 5):每加一列同步改 `schema.sql` 并加 `add_column_if_missing`;新表 `CREATE TABLE IF NOT EXISTS` 已是充分守卫;退役表用 `DROP TABLE IF EXISTS`。

### 3.4 与 [01 篇](01-architecture.md) `adapters/` 的接缝

`claude_cli`/`cursor`/`open_design` 三个适配模块的 `README.md`(借自哪、借了什么、没借什么)按本篇 §2.1 的三种 `probe` 填:`claude_cli` 借 `claude_bin.rs` 的路径候选算法;`cursor` 借 `cursor-agent-executor.md` 设计稿;`open_design` 借 `open_design.rs` 的 socket 握手算法。`adapters/claude_cli`/`adapters/cursor` 内部直接复用 `interactive_cli.rs` 的 `TuiAgentConfig`/`build_startup_plan`,不重写。

---

## 4 · 边界与失败

**不做的事**:不建 agent 名单(workflow 包自己决定用哪个内置子代理,"自带 agent 数"如实显示实测的 0);不做技能市场界面(鱼塘只在配置屏走§2.8 导入,不做浏览/搜索 UI);不整体嵌 DSH(deepseek-harness 结论已定,只借三条接口判断;将来接 DSH 一类网页 agent 走新增一条 `web_embed` 声明,和接 Open Design 同一条路);不塞整包进系统提示词(§2.7 实测数字已堵死这条路)。

**失败如实显示,不假装**:工具未装(探活失败)→ 灰态+"怎么装→",不隐藏该行;导入的目录/仓不满足判据 A/B → 如实按单技能导入并提示"看起来不是 workflow 包";`issue.workflow` 名字解析不到 → 如实跳过、不记账、不错记,活照常能开工;Open Design 打开通用首页而非本活 worktree(§2.7 开放问题)→ 中栏标注"未定位到本活工作区(设计中)";Cursor 今天 `supported=false` → 下拉里仍出现,选中后▶开工如实报错"Phase 1 仅 Claude CLI",不从下拉里拿掉。

---

## 5 · 验收与读回

1. **导入 superpowers 后 SELECT 包与技能行、入口标记**:
   ```sql
   SELECT id, name, source, pkg_version, entry_skill_id FROM skill_package WHERE name = 'superpowers';
   SELECT id, name, package_id, is_entry FROM skill WHERE package_id = (SELECT id FROM skill_package WHERE name = 'superpowers');
   ```
   预期:`skill_package` 一行;`skill` 14 行(预研实读 superpowers 14 个技能),`using-superpowers` 一行 `is_entry=1`(人工补标后)。mattpocock-skills 同理,预期 22 行(预研实读的 22 条 promoted 技能)。
2. **一次▶开工后 worktree 里 `.claude/skills/` 出现整包**:深链 `BW_OPEN=<项目名> BW_PANEL=session` 后对一张挂 superpowers 的活点▶开工,`ls <worktree>/.claude/skills/` 应看到 14 个子目录,各带 `SKILL.md` 与 `.bw-managed` 标记。
3. **Done 后战绩 +1 且只 +1**:合入前后各查一次 `SELECT COUNT(*) AS uses, SUM(outcome='done') AS wins FROM workflow_credit WHERE subject_kind='workflow' AND subject_id=(SELECT id FROM skill_package WHERE name='superpowers')`;点「完成」后 `uses` 应 `+1`(`wins` 视本次是不是"赢"同步 `+1`);重开重走一遍不应再变(`UNIQUE(subject_kind,subject_id,issue_id)` 物理拒绝重复插入,同一件活绝不记两次)。
4. **探活如实**:本机没装 Cursor 时 `ProbeTool{name:"cursor"}` 应返回 `ok=false`;卸载/改名 `claude` 二进制后重探也应翻灰,不应有"曾装过所以现在也算装了"的假阳性。
5. **深链 `BW_PANEL=config` 截图**:`[BW_OPEN]` 证据行打出后,截图应看到四段——①映射三列表格②workflow 表(含 superpowers/mattpocock-skills/三张运作 workflow/原型设计 workflow)③skill 表(`package_id IS NULL` 的行,如 grillme、buddy 自带 9 篇)④连接器+定时小段。

---

## 6 · 开放问题

1. **`.bw/issue-policy.toml` 改动怎么落盘**:母文档"写仓一律走活+MR"的规矩严格照办就该走轻量活(类比 `EditProjectCard`),但这份文件调参可能相当高频,每次都建活走 MR 会不会太重?本篇倾向"走活+MR",但交由用户按试点体感定,或退一步先直接写本机、下次运作活②的资产盘点里被动核对。
2. **`agent` 表退役,存量战绩要不要先归档**:§2.10 建议直接 `DROP`,但不可逆——是否需要先跑一次性导出脚本,把存量战绩写进 `docs/plan/history.md` 留一份人可读的历史存根。
3. **Open Design 怎么定位到具体活的 worktree**:今天嵌入的是通用首页,V4 需要活级路由,机制(URL query、还是走 socket 协议加新消息类型)没核实过,需要一次穿刺确认。
4. **Cursor 侧 workflow/技能注入的真实机制**:§2.7 给的方案(AGENTS.md 承载衔接层,`.claude/skills/` 指望 Cursor 也能读到)是推断,不是穿刺验证过的事实,需要 Cursor 真正接通那天补一次真实穿刺。
5. **判据 C(入口技能)的人工补标交互细节**:导入向导里的必填一步,还是导入后随时可补——留给写代码时定,或等 05/配置屏细化时一起定。

---

## 与代码的关系

这篇不改 `crates/`。开工顺序建议:①`.bw/issue-policy.toml` 的 `[[tool]]`/`[[mapping]]` 解析器(`bw-app` 里的小模块,不需要新 crate);②`skill_package` 新表 + `skill` 表四个新列(双守卫);③`ImportSkillLibrary` 补"包分组";④`skill_materialize` 补"整包一次性物化";⑤`record_workflow_run` + `TransitionIssue`/`finalize_run_interactive` 挂载点改造;⑥`agent` 表退场(含 `issue.assignee`,与 02 篇协调);⑦配置屏四段与三个新 `adapters/` 目录(与 01、05 篇协调)。第 5 节就是这条链路的验收清单。
