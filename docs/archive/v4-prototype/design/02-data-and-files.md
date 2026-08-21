# 02 · 数据与文件:库里剩什么表、仓里的文件长什么样、信息住哪一层

> **历史档案(2026-08-21 归档,只加不改)**:这篇已被 [`docs/v4-prototype/design.md`](../../../v4-prototype/design.md) 蒸馏取代,**现状以 `docs/v4-prototype/design.md` 为准**。正文一字未改,只加了这行横幅——读它是为了考古「当时为什么这么定」,不要当现状;还没干的活只认 [`docs/LEFTOVERS.md`](../../../LEFTOVERS.md)。

---

> **30 秒导读**:这篇管三件事——SQLite 库里到底有哪几张表(列级定义)、项目代码仓里有哪些 buddy 要读要写的文件(每个给完整样例)、以及一样东西该住在「仓 / 本机文件 / 现算」三层里的哪一层。**一句话结论:仓是正本,库只有四张表(`project` / `issue` / `claude_conversation` / `app_meta`),别的数字全部现算。** 给接着做 V4 的会话看,也给要往项目仓里加文件的人看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4G 七组。 与母文档([`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) §6「信息住哪」)冲突时以母文档为准。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

## 0 · 这篇管什么、不管什么

**管**:①`crates/bw-store/src/schema.sql` 最终只剩哪四张表、`issue` 表要加哪 9 列;②项目代码仓里 `.bw/*.toml`、`.bw/plan/*.md`、`.bw/releases.md`、`.bw/PROJECT.md` 的完整格式,每个给一份可以直接抄的样例;③一样信息该住在仓、本机文件、还是「现算(不存)」,谁来取、丢了能不能重建。

**不管**:规范铺底怎么把这些文件第一次写进仓、怎么升级、怎么对账(见 [03-standard-and-backfill.md](03-standard-and-backfill.md));开工工具怎么注册、技能包(workflow)扫描与注入的实现细节(见 [04-tools-and-workflows.md](04-tools-and-workflows.md),本文只在信息住哪总表里提一笔它住在仓的哪个目录);会话屏、计划屏、通知屏的界面结构(见 05/06/07 篇);项目群适配的工厂实现细节(母文档 §6/§7 已定接口是「发消息/拉历史」两个函数,具体留给 07)。本文只管数据落在哪、文件长什么样,不管谁在什么时机去读写它们。

对应母文档:[mvp-blueprint-draft.md](../mvp-blueprint-draft.md) §6「信息住哪:一张盘点表」、第 0/1/2-3/5 站「留下什么」、§2.5/§2.6;[standard-module-draft.md](../standard-module-draft.md) §2 八大类「文件」一栏。涉及待拍:04(在研版本单一)、06(周计划正本进仓)、21(运作活变更走 MR)、27(老项目历史回填)、**29(信息住哪盘点、库是本机的)、30(V4 不兼容老库)、32(预置技能包随 buddy 出厂、铺底复制进项目仓)**。

## 1 · 用户看到什么、做什么

用户不直接"看"这篇管的东西——这些是底层数据与仓文件,用户从各屏看到的是它们的呈现。但有几处会**直接打开这些文件**:评审运作活①的 MR 时,diff 里就是 `.bw/metrics.toml`、`.bw/project.toml`、`.bw/plan/2026-W34.md` 的改动;评审运作活③「规范铺底」的 MR 时,看到仓里新增的 `standard/`(见 03)、`AGENTS.md`(仓根)、`.bw/issue-policy.toml`、`.bw/standard.toml` 一整套骨架,老项目还会看到 `.bw/plan/` 里多出几份带 `origin: backfill` 的历史周文件、`.bw/releases.md` 里多出几行标"回填"的历史版本;在「配置」屏改开工工具映射并保存,写回的就是 `.bw/issue-policy.toml`;用 `sqlite3` 读回验证数字时,查的就是本文描述的四张表——但**项目名片、项目群配置、规范版本、在研版本、指标读数、技能/workflow 用了几次这些信息不在库里**,要核对它们直接 `cat` 仓文件或现场算(见 §2.6)。

不会在库里看到、也不需要关心的:技能/workflow 战绩账本、群通知去重账本——这两样盘点时判定"没人取的不存",全部改成现算或干脆不追踪(§2.3/§2.4)。

## 2 · 设计

### 2.1 库 schema 总览:四张表,别的都不建

**V4 不兼容老库**(§2.7 展开):新壳用新的库文件,`schema.sql` 按下表直接写全,不存在"给存量库加列"这件事。

| 表 | 装什么 | 备注 |
|---|---|---|
| `project` | 项目定位(路径/远端/id/名称)+ 项目墙用的健康灯显示缓存(既有的 `signal`/`weekly_signal`/`signal_derived_rev`/`signal_derived_at` 四列) | 结构不新增列;推导算法据 §2.6/[08 篇](08-overview-derivation.md)改成现算 git + 仓文件,但缓存列本身沿用 |
| `issue` | 远端 issue 的本机缓存 + 9 个 V4 扩展列(排期/版本/工具/种类/来源/workflow/类别/顺序/挂的指标) | 这 9 列的**正本是周计划文件的活清单行**,库里只是缓存(§2.2) |
| `claude_conversation` | 活 ↔ claude 会话 ↔ worktree ↔ 分支,恢复会话必需 | 结构不变(V3 已有) |
| `app_meta` | schema 版本等 key/value;通知屏「事件流看到哪个时间点」也以 `notify_seen:<project_id>` 为键放这里(07 篇 §2.3),不为它开第五张表 | 结构不变(V3 已有) |

四张表全部可以删掉从仓 + 远端重建——都是定位、缓存、纯本机过程数据,没有一张是"丢了项目就丢了"的正本。

**其余 16 张今天在库里的表,以及早期草案曾计划新建、后来取消的 3 张,V4 全部删除或不建**:

| 表 | 原来干什么 | 数据现在去哪 | 详见 |
|---|---|---|---|
| `observation` | 指标数据点 | 可重算的(周合入数、提交数、目录变动)每次现算;不可重算的(外部读数、手填)按 [14 篇](14-metrics-collection.md) §2.5 追加进 `.bw/metrics/readings.jsonl`。**落点改过**:原先写进周计划文件「本周指标读数」段,那一段现在降级成快照,不再是读数正本 | §2.5/§2.6 |
| `metric` | 指标定义 | 定义在 `.bw/metrics.toml`;读数见上一行 | §2.5 |
| `workflow_run` | 每次 ▶跑 的开工/结清/成败/耗时/前后 git head | 干没干成看**远端 MR 合没合入**;会话线索由 `claude_conversation` 一张表接住 | §2.3/§4 |
| `artifact` | 产物登记 | `git log --name-only` 就是产物登记 | §2.6 |
| `cron_task` | 定时任务 | V4 只有一个定时(周五晚建资产盘点),写在 `.bw/issue-policy.toml` `[cadence]` 段;判据现查「本周有没有这张活」 | §2.5/§2.6 |
| `connector` | 远端连接配置 | 地址在 `.bw/project.toml`,令牌在系统钥匙串;连不连得通是即时探活,不存结果 | §2.6 |
| `skill`/`skill_file`/`skill_stage` | 技能登记 | 正本是文件:buddy 自带的编在二进制里,项目自有的在仓 `.claude/skills/`,两处都扫目录即得 | §2.6、[04 篇](04-tools-and-workflows.md) |
| `workflow_spec`/`workflow_version` | V3 聊天式 workflow 正文与版本 | V4 的 workflow 是技能包文件(SOP 类技能包),不是库里的聊天式正文 | §2.6、[04 篇](04-tools-and-workflows.md) |
| `agent`(含 `runs`/`wins`/`win_rate`) | 队友名单与战绩 | 不维护 agent 名单;`issue.assignee` 也不出现在 schema 里 | §2.3 |
| `op_stage`/`handoff` | 阶段轴与阶段舱 | 阶段降级为活的类别标签;方法论内容进 `docs/method/` 规范扩展件 | [03 篇](03-standard-and-backfill.md)/[09 篇](09-ops-workflows.md) |
| `session` | 会话登记 | 会话屏基于 `claude_conversation` | [05 篇](05-session-screen.md) |
| `knowledge_source` | 知识资产索引 | 知识库 = 仓内文档树 + 代码图,不建索引表 | [11 篇](11-knowledge-base.md) |
| `release`/`release_issue` | 发版记录与关联的活 | 正本 `.bw/releases.md`;活挂哪个版本只用 `issue.version` 一列,不需要关联表 | §2.5 |
| `week_plan` | 周计划索引 | 正本 `.bw/plan/YYYY-Www.md`;周列表靠扫目录 | §2.5 |
| `issue_metric` | 活↔指标关联 | 一活挂一个指标,`issue.metric_key` 单列够用,要挂多个就拆活 | §2.2 |
| `workflow_credit` | 技能/workflow 战绩台账(早期草案) | 用了几次现算,不建战绩表;"成没成"看远端 MR | §2.3 |
| `chat_outbox` | 群通知去重账本(早期草案) | 不做去重,重发一条能忍 | §2.4 |
| `skill_package` | 技能包导入登记(早期草案) | 技能包 = 扫目录即得,不建登记表 | §2.6、[04 篇](04-tools-and-workflows.md) |

### 2.2 `issue` 表增量(列级)

```
week_of      TEXT    NOT NULL DEFAULT ''   -- ISO 周,如 "2026-W34";'' = 待办池(未排进任何一周)
version      TEXT    NOT NULL DEFAULT ''   -- 在研版本标签,如 "v0.3";'' = 未挂版本(常见于运作活)
tool         TEXT    NOT NULL DEFAULT ''   -- 开工工具:'claude_cli' | 'cursor' | 'open_design';'' = 未定
kind         TEXT    NOT NULL DEFAULT 'business'  -- 'business'(业务活) | 'ops'(运作活) | 'light'(轻量活:无 agent 会话,只有 buddy 写仓 + MR;名片编辑、发版本用它)
origin       TEXT    NOT NULL DEFAULT 'human'      -- 'human' | 'auto' | 'agent_split' | 'backfill'
workflow     TEXT    NOT NULL DEFAULT ''   -- 该活实际用的 workflow / 技能名(供现算用量统计)
sort_order   REAL    NOT NULL DEFAULT 0   -- 看板同列内排序(含待办池),浮点数支持插入排序,新卡片能插进两张卡之间取中间值,不必整列重排
metric_key   TEXT    NOT NULL DEFAULT ''   -- 这张活预期推动的指标键(`.bw/metrics.toml` 里那条指标的 id 字段),''=不挂;一活只挂一个,要挂多个就拆活
```

**这 9 列是缓存,不是正本**(定性,比更进一步):排期、版本、工具、种类、来源、workflow、类别、顺序、挂的指标——这些属性的**正本是 `.bw/plan/YYYY-Www.md` 里活清单那一行**(§2.5)。库里这 9 列存在的唯一理由是:①计划屏、会话屏要能离线快速渲染、按列/按周做 SQL 过滤与排序,不能每次开屏都解析 Markdown;②拖卡片要即时视觉反馈,不能等一次 MR 合入才看到卡片动了。写入顺序因此是"缓存先动、文件随后追上"——拖拽/改工具的命令先更新这 9 列(界面立刻反映),再驱动一次仓文件改动(走 MR,见 [06 篇](06-plan-screen.md));**文件与缓存出现分歧时以文件为准**,下一次对账扫描(项目打开、或人工点「刷新」)用文件内容覆盖缓存,不是反过来。这条对账机制的具体触发时机是留待 06/04 篇定的开放问题(见 §6)。

`week_of` 不外键指到任何周索引表——用 ISO 周文本软关联,一件活可以先标好周、文件还没建出来。`version` 落的是母文档「里程碑不单建实体,版本就是里程碑」这句话。`origin` 里 `backfill` 状态照远端、**不影响任何计数或排序特权**。`workflow` 是"这个技能包用了几次"现算查询(§2.3)的唯一数据源。`sort_order` 对应仓文件活清单里的「顺序」列(§2.5)。

**为什么 `metric_key` 是单列而不是关联表**:总览的核心画法是「每个指标卡下面列本周哪些活在推它」——一张活在实践中通常只服务一个当下最想推的指标,单列 `WHERE metric_key='<id>'` 已经完全能支撑这个反查查询,不需要关联表。要推动多个指标就把这张活拆成几张——粒度变细本身也更符合「一件活=需要一次 agent 会话」的边界定义。

### 2.3 技能 / workflow 用了几次:现算,不建战绩表

早期草案给"用了几次"建过 `workflow_credit` 台账表(用数据库唯一约束保证同一 workflow 对同一件活绝不记两次)。盘点之后,连"战绩"这个持久账本概念本身也被取消(母文档 §6.3)——**"干没干成"不再由 buddy 自己判定和记账,看的是远端 MR 合没合入**;库里因此不需要 `outcome`/`settled_at` 这类结算事件表。

"用过几次"完全现算:扫本机 `issue` 缓存表(必要时回溯 `.bw/plan/` 下历史周文件的业务活清单)按 `workflow` 列 `GROUP BY`:

```sql
SELECT workflow, COUNT(*) AS uses
FROM issue WHERE project_id = ? AND kind = 'business' AND workflow != ''
GROUP BY workflow;
```

配置屏「用过几次」就是这条查询的结果,不缓存汇总数,每次现查——这条纪律和「健康信号只能从数据推导、绝不手设」(CLAUDE.md「健康永远推导」)同一个精神。

**`agent` 表怎么处理**:不建。V4 新库 `schema.sql` 里从未出现 `agent` 表(不是"迁移删除",是"新库从未创建过"),连同 `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` 三条命令一起不存在;界面不再展示"队友库"。**这是一次数据丢失决定**(存量 V1-V3 项目的队友战绩历史不迁移,不可逆),已提请用户点头——见 握手清单 第 2 条;因 V4 本身不兼容老库(§2.7),这条丢失是「换库文件」这个更大决定的自然结果。

**`issue.assignee` 怎么处理**:新壳不读不写,`schema.sql` 里 `issue` 表定义不再出现这一列——"选类别→工具→workflow"完全取代"指派队友"。

**代价如实写(母文档 §6.3 三条代价之一)**:CLAUDE.md「干活过程自动留痕……每次运行的成败与耗时,全部自动入账」这条铁律,在 V4 没有持久载体了——`workflow_run` 表连同它的开工/结清/成败/耗时/前后 git head 记录一起消失。取代它的是更硬的东西:**活干没干成看远端 MR 合没合入**,这条判据造不了假。这是产品哲学的改动,用户用户知情拍板。

### 2.4 群通知会不会重发:不做去重账本

早期草案给"不重复推送同一条评审/合入/发版消息"设计过 `chat_outbox` 表(部分唯一索引,只对 `status='ok'` 强制唯一,失败允许重试)。信息住哪那次盘点判定它同样过不了"没人取的不存"这条门槛——项目群适配模块([07 篇](07-notify-and-chat-group.md))调用发送即完成,不写库、不做幂等键查重。

**代价如实写**:极小概率下(比如 buddy 进程在发送前后意外重启)同一事件可能被重复推送一条消息进群。用户已知情接受(母文档 §6.3「重发一条能忍」)。要不要在项目群适配模块内部按内容做一层轻量幂等(比如内存里记最近几条已发送的事件指纹,不落库),留给 07 篇按实践需要再定,不是本文的事。

`[chat]` 配置(提供方 / 群号 / 同步哪些事件)正本仍在 `.bw/project.toml`,不在本节讨论范围——见 §2.5/§2.6。

### 2.5 仓文件格式与完整样例

样例项目用 buddy 自己仓里已经在跑的真实项目 **WorkflowHub**(它那份指标正本的真实内容见 [`docs/metrics/workflowhub/metrics.toml`](../../metrics/workflowhub/metrics.toml)——2026-08-21 从仓根 `.bw/` 移到那里,因为它是 WorkflowHub 的指标、不是 loop-buddy 自己的;**本仓根目录今天没有 `.bw/metrics.toml`**);没有真实数据的地方标「演示」。

#### `.bw/project.toml`(现有五字段 + `[chat]` + `standard_version`/`current_version`)

```toml
name = "WorkflowHub"
kind = "看板 / 网页应用"
brief = "把 agent 会话里长出的工作流沉淀成可复用资产"
benchmark = "Linear"
opportunity = "被持续复用、效率可量化提升"

# 规范版本与在研版本的库外正本——这两个值不进 SQLite,总览/计划屏每次要显示
# 都现解析这个文件。standard_version 与 `.bw/standard.toml` 的 version 字段
# 保持一致(那份文件是规范对账用的权威记录,03 篇已定);这里的字段是给总览
# 快速展示、不用另外打开一份文件的镜像副本。current_version 是计划屏顶部可
# 切的"在研版本",新建项目默认 "v0.1"(待拍-04)。
standard_version = "4.0"
current_version = "v0.3"

# 项目群配置。空着不写这一段 = 未配群,总览名片显示「未配 · 配置」。provider
# 今天只有 "welink" 一个真实实现,外部提供方留空占位(工厂设计见
# 07-notify-and-chat-group.md)。WeLink 登录态不归 buddy 管——用户在本机
# 提前登好,buddy 只在「测一下」里探活(待拍-31)。
[chat]
provider = "welink"
group_id = "638201"          # 演示群号
notify = ["review", "merged", "release"]
```

`[chat]` 是可选表,`standard_version`/`current_version` 是可选字段——旧文件(没有这几项)照样解析成功;`ChatConfig` 沿用 `deny_unknown_fields`,写错键名报错而不是静默丢弃。**这四项(连同现有五字段)都不落库存副本**——`project` 表只留定位字段与显示缓存(§2.1)。

#### `.bw/issue-policy.toml`(新文件,规范第 5 类「活的约定」)

```toml
schema_version = 1

# 开工工具声明:name → 接法类型(kind:terminal 终端类 / web_embed 本机网页内嵌类)
# → 探活方式 → 能力清单。这里的 kind 与下面 [[mapping]] 段的 category(活的
# 类别标签)是两件不同的事,字段名特意分开,不要混。
[[tool]]
name = "claude_cli"
kind = "terminal"
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
capabilities = ["inject_skills"]

# 三列映射:类别 → 默认开工工具 → 默认 workflow。
# workflow = SOP 类技能包(自己调度 agent),不是单个技能。
[[mapping]]
category = "prototype"
tool     = "open_design"
workflow = "proto-design"

[[mapping]]
category = "build"
tool     = "claude_cli"
workflow = "mattpocock-skills"   # 默认,项目可换成 superpowers

[[mapping]]
category = "optimize"            # 优化 / 运维两个阶段类别同款,各一行,此处只示范一行
tool     = "claude_cli"
workflow = "mattpocock-skills"

[[mapping]]
category = "growth"             # 运营推广
tool     = "claude_cli"
workflow = ""                    # 无默认,从鱼塘挑

[review]
who_can_merge  = "repo_write"    # 不存角色,谁有仓权限谁能点
require_pr_for = ["code", "docs", "prototype"]

[cadence]
ops1_trigger  = "manual"          # 运作活①:人触发,判据 = 当前周没有 .bw/plan/ 文件
ops2_trigger  = "scheduled"
ops2_schedule = "fri 20:00"       # 运作活②「资产盘点」:定时,默认周五晚,可改;
                                    # 判据 = 本周有没有这张活(查 issue,不查 cron 表)

[kanban]
pool_label = "待办池 · 未排进任何一周"
todo_label = "待办 · 已排进本周,等开工"
# 所有列都能拖——排期(待办池⇄待办)直接生效,拖到进行中/评审中/已完成/阻塞
# 四列是状态动作,弹确认框才真正发生,不合法的转移松手即弹回;不再有
# drag_scope 这种"限定能拖哪些列"的开关。规则细节见 06-plan-screen.md §2.3。
```

这份文件由铺底(运作活③)首次写入,之后在「配置」屏改「开工工具映射」并保存时写回——保存动作本身也建一张轻量活走 MR(改仓的动作都走 MR)。**这份文件本身不入库**:配置屏、▶开工每次现读现解析,不存在"同步进库"这一步(§2.6)。

#### `.bw/standard.toml` + `.bw/managed.toml`(规范第 8 类「元信息」)

```toml
# .bw/standard.toml
version    = "4.0"
enabled    = ["charter", "agents", "docs-core", "metrics", "issue-policy", "defaults-core", "cadence"]
extensions = ["decisions", "method"]
source     = "builtin"
```

标记文件(母文档称「`.bw-managed` 标记」,本文落成一份清单 `.bw/managed.toml`,而不是给每个被铺文件各配一个同名旁路文件——避免铺一次底就在仓里撒一地隐藏文件):

```toml
# .bw/managed.toml —— 铺底/升级时写,记录哪些文件是它管的、什么版本、铺下去
# 那一刻的内容指纹。下次升级前先比对指纹:变了 = 人手改过,不覆盖只提示差异。
# 记的全是 `.bw/` 底下的件 —— 仓根那些是项目自己的文件,buddy 不铺也不管。
[[file]]
path        = ".bw/PROJECT.md"
version     = "5.0"
fingerprint = "sha256:11ab88de...ff02"

[[file]]
path        = ".bw/issue-policy.toml"
version     = "4.0"
fingerprint = "sha256:77cdaa03...9e10"
```

`fingerprint` 只在铺底/升级那一刻算一次,算法留待 03 定——本文只定文件形状。**这份文件是规范对账的唯一权威记录,同样不入库**(不建 `skill_package`/`skill` 类登记表——技能包本体是文件,扫目录即得,见 §2.6)。

#### `.bw/plan/2026-W34.md`(周计划,规范第 3 类;新增 front matter)

```markdown
---
week: 2026-W34
origin: human
---

# 2026-W34 周计划

> 正本文件。buddy 读它驱动计划屏与总览「本周计划进度」块;**没有库内索引表**——
> 计划屏左栏的周列表靠扫 `.bw/plan/` 目录得到(见 §2.6),这份文件本身就是唯一
> 正本,不存在"文件与库不一致"这类问题——库里 `issue` 表的 8 个缓存列不一致
> 时以这份文件为准(§2.2)。

## 周目标

把 V4 详细设计稿(design/ 十篇)写完并过一轮内部评审,为下一步高保真落地铺路。

## 业务活

| 顺序 | 标题 | 类别 | 工具 | workflow | 预期推动的指标 | 远端 issue |
|---|---|---|---|---|---|---|
| 1 | V4 高保真可点击原型 | 原型 | Open Design | 原型设计 workflow | — | #104 |
| 2 | 减负线两轮收尾合入 | 构建 | Claude CLI | mattpocock-skills | 本周合入活数 | #102 |
| 3 | 规范铺底模块落地 | 构建 | Claude CLI | mattpocock-skills | 已接入并铺底的内部项目数 | #105 |

「顺序」列对应库里 `issue.sort_order` 缓存列(§2.2)——拖拽排期改的就是这一列
与这里的行序,两边同一次改动一起动;可以有跳号或小数(如 1.5),插进两张卡
之间不必整份文件重排。

## 本周指标读数

<!-- 运作活①第四步落文件时,把这一步刚更新完的指标现状抄一份进来,随 MR
     进仓——多机一致性靠这段,而不是靠同步库(待拍-29)。演示值。 -->

| 指标 | 数值 | 来源 | 采集时间 |
|---|---|---|---|
| 本周合入活数(引领) | 4 | `git log --merges` 现算 | 2026-08-17 09:00 |
| 已接入并铺底的内部项目数(滞后) | 1(演示值,试点未跑完前如实) | 手填 | 2026-08-17 09:00 |

## 本周运作

| 活 | 状态 | 说明 |
|---|---|---|
| 运作活①更新指标 + 制定本周计划 2026-W34 | 已完成 08-17 | 复盘上周、更新指标、引导出本周目标与活 |
| 运作活②资产盘点 2026-W33 | 评审中 | 定时(周五 20:00)自动建并自动开工(`mode=weekly`),等人合入 |
| 运作活③规范铺底 v5.0 | 已完成 08-17 | 一次性,不会再出现在下一周 |

## 上周完成情况(2026-W33)

- 减负线第一轮六切片 + V3-use-fix 三张 → PR #101 已合(08-17,来源:git log)
- 引领指标「本周合入活数」:4(演示值,真实数每次从 `git log --merges` 现算,不查库)

## 运作活②盘点尾段(自动追加,示例)

<!-- 由运作活②「资产盘点」workflow 在 MR 里追加,格式细节见 09-ops-workflows.md -->
- 新增文档 3 篇,均已登记进知识库资产页
- `.bw/plan/`、`.bw/releases.md` 齐全
- 规范对账:全部件版本一致,无过期、无人改过
- 指标数据新鲜度:全部在保鲜期内
- 代码图大文件榜:本周未发现超 1500 行的文件
- 可做可不做的微重构:无(发现了也只列建议活,不在这里直接改代码)
```

**回填的历史周文件用同一模板**(待拍-27/29):`.bw/plan/2026-W31.md` 这类历史周文件与人写的本周文件**是同一份规范、同一套段落结构**——front matter 只是 `origin: backfill` 而不是 `human`;没有的段落(最常见是「周目标」,老项目的历史周从来没人手动定过目标)就空着或写"未发现",不硬造。**没有单独的 `.bw/plan/history.md` 文件**——历史周与本周混在同一个 `.bw/plan/` 目录、同一份周列表里,靠 front matter 的 `origin` 与界面上的小徽记区分,不是两套渲染逻辑。示例(回填,演示数字):

```markdown
---
week: 2026-W32
origin: backfill
---

# 2026-W32 周计划

> **回填自 git / 远端**,由运作活③「规范铺底」的「历史回填」步骤(= 运作活②
> 「资产盘点」workflow 的首次模式)生成;只解释历史,不点灯。每次重跑**整段
> 覆盖**,不追加重复段落。

## 周目标

(未发现——历史周没有周计划记录,不倒推)

## 业务活

(未发现结构化的"业务活清单";远端已关闭 issue 已同步进库,`origin='backfill'`)

## 本周运作

(不适用——回填周早于 buddy 接入,没有运作活)

## 按周历史统计(自动生成)

| 合入 MR 数 | 提交数 | 动过的目录 Top3 | 关闭 issue 数 | 当周版本 |
|---|---|---|---|---|
| 3 | 21 | `crates/bw-app`、`docs`、`crates/bw-store` | 5 | v0.3.0-v3 |

数据来源:git 合入记录(`git log --merges`)、`git log --numstat` 按目录聚合、远端 issue/MR 列表。没有的字段留空,不发明数据。
```

#### `.bw/releases.md`(发版记录,沿用本仓 `/docs/releases.md` 的表格式;**唯一正本,库不存副本**)

```markdown
# 版本登记(出包与运作)

> **30 秒导读**:一行一个已发布或在研的版本——版本号、发版日、这一版是
> 什么、包含哪些活。回填的行带来源徽记,不代表 buddy 里真发生过评审流程。
> **这份文件是版本记录的唯一正本**——总览/计划屏「发版记录」块每次直接解析
> 这份文件,活挂哪个版本只看 `issue.version` 一列,两者拼起来就是完整信息,
> 不需要第三份数据。

| 版本号 | 发版日 | 说明 | 包含的活 | 来源 |
|---|---|---|---|---|
| v0.2 | 2026-08-10 | 找指标/绑数据走嵌入终端 | #88 #91 #93 | 人发 |
| v0.1 | 2026-06-01 | 首个可跑版本(标签回填) | — | 回填 · git tag |
```

表头与本仓自己的 [`/docs/releases.md`](../../releases.md) 一致,新增「来源」列区分「人发」与「回填」——本仓自己目前全是人发,不需要这一列;项目仓的版本记录可能两种都有。「包含的活」列是活号的自由文本(如 `#88 #91 #93`),渲染时按号去查 `issue` 表拿标题展开,不是一张关联表——号找不到对应活时按 03 篇 §2.6 的规则跳过并记警告,不是解析失败。

#### `.bw/PROJECT.md`(章程,规范第 1 类,四段 + 信息段)

```markdown
# WorkflowHub

## 想做什么

把 agent 会话里长出的工作流沉淀成可复用资产,让下一次同类活自动带上。

## 最像的对标

Linear(工作流/看板的产品体验对标,不是功能照抄)

## 三个月长成什么样(北极星)

每月被标准工作流带完成的活数——过去 30 天内,状态真实到达 Done、且执行链路
上真实跑过一条 Hub 标准工作流的 Issue 数。完整定义与采集方案见 `.bw/metrics.toml`。

## 项目信息

- 仓:codehub · `workflowhub/hub`(演示路径)
- 负责人:—(单人项目,Builder 本人)
- 在研版本:v0.3(来自 `.bw/project.toml` 的 `current_version`,不入库,现读现解析)
- 项目群:WorkflowHub 日常群(WeLink;群号见 `.bw/project.toml` 的 `[chat]` 段,登录态在本机设置)
```

#### `.bw/metrics.toml` —— 格式正本已移交 [14 篇](14-metrics-collection.md) §2.6

**本篇不再定义这份文件的格式。** 原来这里写的是「格式不动,沿用 `docs/buddy/standards/metrics.md` 已有规范」,那句话已被 14 篇 §2.6 取代:`schema_version` 从 1 跳到 2、采集方式从五种收成两种(脚本 / 手填)、新增 `window` 字段(表达「这条指标的历史现在还能不能重新算出来」)、`query` 换成 `run`,而且**不写迁移**。结构上不变的只有一条:北极星恰好 1 个、`[[lagging]]` / `[[leading]]` 各 0..N。

这是设计,还没落地:今天读这份文件的仍是 V3 的解析器 `bw_engine::metrics_file`(老格式、五种采集方式),新解析器该落在 `crates/bw-v4/src/repo/metrics_file.rs`,进度只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md)。

老格式的真实样例见 [`docs/metrics/workflowhub/metrics.toml`](../../metrics/workflowhub/metrics.toml)(WorkflowHub 项目的指标正本,2026-08-21 从仓根 `.bw/` 移到那里)。**别拿它当「能采到数」的例子**:那五条写的采集方式是 `bw`,而这种采集方式从来没有实现过。

`issue.metric_key` 存的是**指标的名字**,不是什么 id ——`.bw/metrics.toml` 里的指标没有单独的 `id` 字段(`docs/buddy/standards/metrics.md` 原话:「文件没有 id 概念」),代码里也是按名字精确匹配(`crates/app-shell/src/bridge/vm_panels.rs`)。库里没有 `metric` 表,这一列是指向仓文件的一根字符串引用,展示时现读现解析拿名称与口径。指标改名会让历史读数对不上,这条风险记在 [08 篇](08-overview-derivation.md) §6 开放问题 4。

#### 技能与 workflow 住哪(2026-08-20 试点第一天改过,以这一段为准)

**两批东西,两个地方,不要混**:

| | buddy 自带的十三份 | 项目自己的 |
|---|---|---|
| 有哪些 | 九篇方法论技能(`bw_core::bw_library::bw_standard_skill_docs`)+ 四份运作剧本(`standard/06-defaults/ops/`,含子技能 `metrics-refresh`) | 蒸馏产出、人手加的、§2.8 三条路导入的 |
| 正本 | 编在 buddy 二进制里 | 项目仓 `.claude/skills/<name>/SKILL.md` |
| 落在哪 | buddy 自己的资产目录 `<库文件所在目录>/assets/skills/`,开工前展开,内容一致就不写 | 项目仓,随 MR 可见 |
| 进用户的版本控制吗 | **不进** | 进 |

**原来定的是「铺底把两批预置包复制进项目仓 `.claude/skills/`」,已经推翻**。推翻它的是
一次真实接入:buddy 自己的仓把 `.claude/` 写进了 `.gitignore`,复制进去的十三份一个都没进
第一个 MR,界面上却显示铺好了。理由与新机制见 [04 篇](04-tools-and-workflows.md) §2.7。
连带作废的还有当时那句「Claude CLI 只在项目仓里找技能,所以复制这一步是必需的」——给
绝对路径 + `--add-dir` 一样读得到。

因此 `.bw/managed.toml` 里**不再有技能包的指纹**(原来 18 条,现在只剩规范件那几条),
规范对账也只对账 `CORE_TEMPLATES` 那几份:技能不在用户仓里,自然没有「人手改过没有」
这个问题。

**仍然没进去的**:业界包(mattpocock-skills、superpowers)一份都没有,而
`.bw/issue-policy.toml` 的 `[[mapping]]` 段仍按设计目标写着这些名字当默认 workflow ——
这些名字两处都找不到对应目录,配置屏会如实标成「不在册」(见 `docs/LEFTOVERS.md`),
不是错字。

SKILL.md 的格式见 [`plan/16-skill-spec`](../../../plan/16-skill-standard-spec.md)。

### 2.6 信息住哪:一张盘点表

三条原则(与母文档 §6 一致):①**仓是正本**——人、agent、leader、committer、第二台机器都要看的,进仓、走 MR;②**库只放本机过程数据与显示缓存**——仓里已有的不复制进库;③**没人取的不存**——每一类数据都要说得出哪个界面/命令会来取,说不出就不建表、不加列、不缓存。

**住在仓里(正本,走 MR,换机器不丢)**

| 文件 | 装什么 | 谁写 | 谁读 |
|---|---|---|---|
| `.bw/PROJECT.md` | 名称 / 想做什么 / 对标 / 北极星 / 三个月长成什么样 | 接入两卡、改名片 | 总览名片、项目墙一句话 |
| `.bw/project.toml` | 仓与远端地址、规范版本、在研版本、`[chat]` | 接入、配群、发版本 | 总览、通知同步、配置屏 |
| `.bw/metrics.toml` | 指标**定义**:北极星 / 滞后 / 引领、目标、保鲜期、采集方式 | 运作活①、人手改 | 指标卡、灯的推导 |
| `.bw/issue-policy.toml` | 开工工具清单、类别→工具→workflow 映射、节律、看板列定义 | 配置屏 | 配置屏、▶开工、定时判据 |
| `.bw/standard.toml` + `.bw/managed.toml` | 规范清单与版本;托管件指纹 | 铺底、对账 | 资产盘点 |
| 仓根 `AGENTS.md`(+ `CLAUDE.md` 一行导入) | **这个项目自己的**开发手册。**铺底不写这份**(2026-08-20,03 篇 §2.3):写它等于建议改造人家的项目,归资产盘点首次模式去问人,人点头才写 | 资产盘点首次模式的子技能 `project-handbook`(还没起过会话)| 项目自己的 AI 工具读(Claude Code / Cursor / Codex) |
| `.bw/plan/YYYY-Www.md` | 本周目标、活清单(含顺序/类别/工具/workflow/指标)、本周指标读数**快照**([14 篇](14-metrics-collection.md) §2.5:读数正本已移到 `.bw/metrics/readings.jsonl`,这一段只是抄一份给不装 buddy 的人看)、盘点尾段;回填的历史周同格式 | 运作活①②、回填、拖拽排期 | 计划屏、总览、知识库 |
| `.bw/releases.md` | 版本→包含的活;回填的历史版本同格式 | 发版本、回填 | 总览发版记录 |
| `.claude/skills/**/SKILL.md` | **项目自有**技能:蒸馏产出、人手加、导入的。buddy 自带的那十三份**不在这里**(在 buddy 自己的资产目录,§2.5) | 蒸馏、导入、人手加 | 配置屏、知识库;▶开工时 agent 在仓里原生发现 |
| 代码、文档、产物 | 项目本体 | agent / 人 | 知识库、会话屏文件树;**不另建产物登记表**,`git log --name-only` 就是登记 |

**住在本机库(SQLite,一台机器一份,删了能重建)**

| 表 | 为什么它非在库不可 |
|---|---|
| `project` | 项目墙要在**不打开任何项目**时列出 N 个项目的名字与灯——不能每次启动扫 N 个仓。只存定位 + 显示缓存,打开项目时以仓文件为准刷新 |
| `issue` | 远端 issue 的**本机缓存**(离线可看、启动快);没配远端的项目它是唯一落脚点。9 个扩展列正本在周计划文件,这里是缓存(§2.2) |
| `claude_conversation` | 活 ↔ claude 会话 ↔ worktree ↔ 分支,恢复会话必需。纯本机、纯过程 |
| `app_meta` | schema 版本、`notify_seen:<project_id>` 等 key/value |

**现算,完全不存**

| 要看的东西 | 现算方式 |
|---|---|
| 健康灯三判据 | ①本周有周目标(周计划文件存在且有目标)且有真实 git 提交;②本周/上周文件里有指标读数;③上周有合入或发版(git 合入记录 / `.bw/releases.md`)。三条都读仓文件/git 现场判,没数据仍是 Unknown,不是绿 |
| 技能 / workflow 用了几次 | 扫 `issue` 缓存表(必要时回溯历史周文件)按 `workflow` 列聚合(§2.3) |
| 群通知发过没有、成没成功 | 不追踪,发送即完成,不做去重(§2.4) |
| 「本周建过资产盘点没有」 | 查本周(`week_of` = 当前 ISO 周)有没有一张 `kind='ops'` 的这张活 |
| 远端连不连得通 | 探活是即时结果,拿到就用,不存 |
| 产物是什么 | `git log --name-only` |

**不存**:agent 名单、技能/workflow 战绩台账、群通知去重账本、群历史原文、`.bw/plan/history.md`、`release`/`week_plan`/`issue_metric`/`skill_package` 这些表、`project` 的名片/群/版本副本列。

**丢了能不能重建**:四张库表全部可以从仓 + 远端重建——都是定位、缓存、可 resume 的会话线索,没有一张是"丢了项目历史就丢了"的正本。仓文件(`.bw/PROJECT.md`/`.bw/*`/`.bw/plan/`/`.bw/releases.md`/`.claude/skills/`)本身若丢失则真丢——它们是唯一正本,没有第二份。`project.signal`/`weekly_signal` 随时可由现算重算。

**换来的三个代价(知情,母文档 §6.3)**:①打开项目、切周时要现算(扫 git、解析文件)——buddy 自己的仓几百个提交是几十毫秒级,万级提交的老仓将来可能要加内存缓存;②指标读数是周粒度,没有每日曲线;③CLAUDE.md「每次运行的成败与耗时自动入账」这条铁律在 V4 没有持久载体了——被更硬的东西取代:**活干没干成看远端 MR 合没合入**。

### 2.7 V4 不兼容老库

新壳用**新的库文件**(默认文件名带 `v4`,与 V3 的库文件并存互不影响),`schema.sql` 按 §2.1 的四张表设计**直接写全**——新列写在 `CREATE TABLE` 语句里,§2.1 表格里列出的所有其余表(含 `agent`/`release`/`week_plan`/`workflow_credit`/`chat_outbox`/`skill_package` 等)根本不需要写 `DROP TABLE`,它们从未在新 schema 里存在过。

**不写任何 V3→V4 数据迁移**:V1-V3 项目的队友战绩、旧版本记录等历史数据不搬运,用户已明确接受这个取舍(握手清单)。V4 开发期间每次改 `schema.sql`,直接删库重建(`rm <db> && cargo run ...` 或指挥器自带的临时库路径),不写迁移脚本、不加 `add_column_if_missing` 调用——开发阶段还没有需要保护的真实用户数据。

**`add_column_if_missing` 双守卫纪律从试点起再恢复执行**:一旦有第一个真实用户开始用 V4 库存了数据(内部试点,见 [10 篇](10-e2e-acceptance.md) §2.4),这份库就变成了"需要保护的存量库",此后再给 `schema.sql` 加列,必须回到 CLAUDE.md「schema 迁移双守卫」——同步改 `schema.sql` 并在 `sqlite.rs::SqliteStore::open()` 加 `add_column_if_missing(...)`,不能再靠"删库重建"图省事。这条时间线上的切换点本身要在试点开始那次改动里明确标注,不是自然过渡。

## 3 · 工程对照

### 3.1 `schema.sql`(V4 新库,直接写全,不是增量 diff)

`project`/`claude_conversation`/`app_meta` 三张表结构沿用现有 `crates/bw-store/src/schema.sql` 定义,**不新增任何列**。

`issue` 表定义(新库从零写,`updated_at` 列之后追加):

```sql
week_of TEXT NOT NULL DEFAULT '', version TEXT NOT NULL DEFAULT '', tool TEXT NOT NULL DEFAULT '',
kind TEXT NOT NULL DEFAULT 'business', origin TEXT NOT NULL DEFAULT 'human',
workflow TEXT NOT NULL DEFAULT '', sort_order REAL NOT NULL DEFAULT 0,
metric_key TEXT NOT NULL DEFAULT '',
```

`issue` 表定义里**不出现** `assignee` 列(§2.3)。

**`schema.sql` 里不再出现的表**(§2.1 已列全 19 张:`metric`/`observation`/`op_stage`/`session`/`handoff`/`workflow_spec`/`skill`/`skill_file`/`skill_stage`/`agent`/`cron_task`/`connector`/`knowledge_source`/`workflow_run`/`artifact`/`workflow_version`/`release`/`release_issue`/`week_plan`/`issue_metric`/`workflow_credit`/`chat_outbox`/`skill_package` ——不需要 `DROP TABLE`,新库从未创建过这些表)。

### 3.2 新库不需要 `add_column_if_missing`(开发期);试点起恢复守卫

开发期(§2.7 已定):`schema.sql` 每次改了就删库重建,`sqlite.rs::SqliteStore::open()` **不需要**为本文列出的任何一列写 `add_column_if_missing` 调用。这与 CLAUDE.md「schema 迁移双守卫」纪律不冲突——那条纪律保护的是"已经有数据的存量库",V4 开发期还没有这样的库。

试点开始后(有第一份真实数据的 V4 库出现),后续任何新加列都要恢复双守卫写法,例如:

```rust
add_column_if_missing(&pool, "issue", "metric_key", "TEXT NOT NULL DEFAULT ''").await?;
```

这只是一个示例格式,试点前的 `issue` 9 列本身不需要这行代码(它们随新库首次创建就已经在 `CREATE TABLE` 语句里)。

### 3.3 仓文件解析器落在哪(`crates/bw-v4/src/repo/`)

五个解析器全部是 `crates/bw-v4/src/repo/` 下的新文件,读法沿用 `bw-engine` 已经立好的风格(只读+解析,`deny_unknown_fields`,`Ok(None)` = 文件不存在,解析失败是 `Err` 且绝不写半份缓存):

- `issue_policy_file.rs`:`IssuePolicyFile { schema_version, tools: Vec<ToolDecl{name,kind,probe,version_cmd,capabilities}>, mappings: Vec<CategoryMapping{category,tool,workflow}>, review: Option<ReviewPolicy>, cadence: Option<Cadence>, kanban: Option<KanbanLabels> }`,对照 §2.5 样例逐段声明,`mapping_for(category)`/`tool(name)` 两个查表方法。
- `standard_file.rs`:`StandardFile { version, enabled: Vec<String>, extensions: Vec<String>, source }`,对照 `.bw/standard.toml` 样例。
- `managed_file.rs`:`.bw/managed.toml` 的读写(`ManagedFile`/`ManagedEntry`)+ 指纹算法(`fingerprint(bytes) -> String`)+ 对账判定——`reconcile(entry, disk_bytes, version) -> Reconcile`,`Reconcile` 是 `Missing`/`Stale`/`HumanEdited`/`UpToDate` 四态,03 篇 §2.6 的对账分类直接对应这个枚举。
- `week_plan_file.rs`:周计划 Markdown 的解析与改写。`read(workspace, week) -> Result<Option<WeekPlan>, _>` 解出的 `WeekPlan` 带 `has_goal()`/`has_reading()` 两个判据(08 篇健康推导直接用);`replace_table(raw, heading, new_table)` 原地换掉某个标题下的表格,不动文件其它内容(06 篇排期写回用的就是它);`render_activity_table`/`render_ops_table` 是反向渲染;`list_weeks(workspace)` 扫 `.bw/plan/` 目录给周列表。
- `release_file.rs`:`.bw/releases.md` 表格解析与追加。`read(workspace) -> Result<Option<Vec<ReleaseRow{version, released_at, note, included_numbers, origin}>>, _>` 只认 §6 那张五列表头(`HEADER = ["版本号","发版日","说明","包含的活","来源"]`)下面的行,文件里别的表格一概不碰;`append_row` 按版本号幂等,找不到 buddy 那张表就在文件末尾另起一段「## buddy 管理的发版记录」,绝不因为认不出表头就把行塞进一张列对不上的老表(有回归测试守着,见 §6 第 3 条)。

`project_file.rs` 也是 `crates/bw-v4/src/repo/` 下**新写的一份**,不是去改 `crates/bw-engine/src/project_file.rs`——那份是 V3 在用的正本,这一刀说好了不碰旧 crate,两份 `project_file.rs` 并存、互不引用,`ProjectFile` 的 `chat`/`standard_version`/`current_version` 三个新字段只存在于 `bw-v4` 这一份里。唯一的例外是 `.bw/metrics.toml`:这份文件的解析复用了 `bw-engine::metrics_file.rs`,没有另写一份。

### 3.4 命令落在哪:`crates/bw-v4/src/command.rs`,不是 `bw-app`

`Command`/`Event` 在 `crates/bw-v4/src/command.rs` 里是**全新的一对枚举**,和 `bw-app::command` 没有继承关系、不互相引用。命令全表在 01 篇 §2.6,这里不重抄,只说与本文数据设计直接相关的几点:

- `.bw/issue-policy.toml`/`.bw/standard.toml`/`.bw/managed.toml` 全部不入库(§2.6),没有"同步进库"这类命令——配置屏、▶开工、资产盘点都是现读现解析 §3.3 的解析器。项目群通知的去重账本、技能/workflow 战绩台账同理没有对应的记账命令(02 篇 §2.3/§2.4 已定它们不做)。
- `RefreshIssueCacheFromPlan { project_id, week }` 是 A 刀真接了的一条:读 `week_plan_file::read` 解析出的活清单,按标题匹配本机 `issue` 缓存表的行、覆盖它的 9 个扩展列(§2.2);只更新、不删除——缓存里有文件没有的行原样留着(可能是刚建出来还没排进周计划的活)。触发时机(项目打开时自动跑一次 / 人工点「刷新」)留给 [06 篇](06-plan-screen.md)定,本文只定这条命令的语义:幂等、文件说了算。
- `ScheduleIssue`/`ReorderIssue`/`SetIssueWorkflow` 这几条改 9 个扩展列的命令,A 刀都已实现,签名与落地时序见 [06 篇](06-plan-screen.md)——本文只定它们最终落哪 9 列(§2.2)。

## 4 · 边界与失败

**不做什么**:不建成员/角色表(committer 靠 `.bw/issue-policy.toml` 的 `who_can_merge = "repo_write"` 一句话表达权限——CONTEXT.md 已明确"buddy 里不建成员、权限、群聊、收件箱")。不给里程碑单独建表(`.bw/releases.md` 一行就是一个里程碑,库里连一行镜像都不需要)。群消息正文不进库(项目群适配模块调用即完成,不留发送记录,§2.4)。不给 `metric` 定义加任何 V4 字段(`issue.metric_key` 只是新增的单向引用,指标定义完全在 `.bw/metrics.toml`)。不建技能/workflow 使用登记表(§2.3,现算)。不做任何 V3→V4 的数据迁移脚本(§2.7)。

**失败如实显示**:三个新 TOML/Markdown 解析器沿用 `deny_unknown_fields` + all-or-nothing,报错不写库,和 `project_file.rs`/`connectors_file.rs` 今天行为一致。群通知发送失败就是失败,不重试、不记录、不阻塞其它通知继续发(§2.4,重试策略若要加留给 07 篇)。`release_file.rs` 遇到"包含的活"列里找不到对应 issue 的号,跳过这条关联并记警告、不让整份文件解析失败。开发期删库重建全程幂等;试点起恢复的 `add_column_if_missing` 同样全程幂等。`RefreshIssueCacheFromPlan` 遇到文件解析失败(§3.3 all-or-nothing)时整次刷新不生效,缓存维持刷新前的状态,不写半份。

## 5 · 验收与读回

`<db>` 替换成 E2E 用的深链启动数据库路径(V4 新库文件,与任何 V3 库无关),`<pid>` 替换成项目 id,`<ws>` 替换成项目工作区路径。

| 核验什么 | 读回 | 预期 |
|---|---|---|
| 库里恰好四张表 | `sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%';"` | 恰好 `app_meta`、`claude_conversation`、`issue`、`project` 四行,不多不少 |
| `issue` 9 个缓存列齐全,一次到位 | `sqlite3 <db> "PRAGMA table_info(issue);"` | 直接看到全部 9 列(`week_of`/`version`/`tool`/`kind`/`origin`/`workflow`/`category`/`sort_order`/`metric_key`),不需要任何 `add_column_if_missing` 参与 |
| 19 张被取消的表全部不存在 | `sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('agent','release','release_issue','week_plan','issue_metric','workflow_credit','chat_outbox','skill_package','observation','metric','workflow_run','artifact','cron_task','connector','skill','skill_file','skill_stage','workflow_spec','workflow_version','op_stage','handoff','session','knowledge_source');"` | 空结果——新 schema 里从未定义过这些表 |
| `project` 表没有名片/群/版本副本列 | `sqlite3 <db> "PRAGMA table_info(project);"` | 不出现 `standard_version`/`current_version`/`chat_provider`/`chat_group_id`/`chat_notify` |
| 回填 issue 的来源分布 | `sqlite3 <db> "SELECT origin, COUNT(*) FROM issue GROUP BY origin;"` | 老项目接入后 `backfill` 一档非零;新项目只有 `human`/`auto`/`agent_split` |
| 本周业务活数与仓文件一致(缓存对文件) | `sqlite3 <db> "SELECT COUNT(*) FROM issue WHERE project_id='<pid>' AND kind='business' AND week_of='2026-W34';"` | 等于 `<ws>/.bw/plan/2026-W34.md`「业务活」表格行数 |
| 项目群配置不比对库(不入库) | `cat <ws>/.bw/project.toml`(看 `[chat]` 段)+ 深链 `BW_PANEL=overview` 截图 | `.bw/project.toml` 的 `provider`/`group_id`/`notify` 与总览名片显示的一致;`sqlite3` 查不到任何 `chat_*` 列——这就是预期 |
| 健康灯没数据是灰的,不是假绿 | `sqlite3 <db> "SELECT signal, weekly_signal FROM project WHERE id='<pid>';"` + 深链截图 | 新接入、还没有一周记录的项目 `signal` 为 `NULL` 或 `unknown`,界面显示灰卡,不是绿 |
| 技能/workflow 用了几次可现算复核 | `sqlite3 <db> "SELECT workflow, COUNT(*) FROM issue WHERE project_id='<pid>' AND kind='business' AND workflow!='' GROUP BY workflow;"` | 与配置屏「用过几次」栏展示的数字一一对应 |
| 发版记录只在仓文件、不在库 | `tail <ws>/.bw/releases.md` + `sqlite3 <db> "SELECT version FROM issue WHERE kind='light' AND title LIKE '发版本 %';"` | 仓文件里的行与「发版本 vX」轻量活的 `issue.version` 一一对应;库里没有独立 `release` 表可查(已含在第 3 条) |
| 指标挂活反查(验证 §2.2 画法) | `sqlite3 <db> "SELECT title FROM issue WHERE metric_key='<metric_id>';"` | 与总览该指标卡下方展示的活标题一一对应 |
| `agent` 表从未存在 | `sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name='agent';"` | 空结果(已含在第 3 条,单独列出便于快速核对不可逆决定) |

## 6 · 开放问题(≤5)

1. **缓存与仓文件不一致时的对账触发时机。** §3.4 提议了 `RefreshIssueCacheFromPlan` 命令,但"什么时候自动跑一次"(项目打开时?轮询?MR 合入 webhook?)没有定,留给 [04 篇](04-tools-and-workflows.md)/[06 篇](06-plan-screen.md)。
2. **`.bw/managed.toml` 的指纹算法与对账触发时机。** 本文只定文件形状(§2.5),具体摘要算法、比对时机留给 [03-standard-and-backfill.md](03-standard-and-backfill.md)。
3. **万级提交老仓的现算性能。** 母文档 §6.3 代价①提到"将来可能要加内存缓存",具体加在哪一层(git 层的 shallow 统计缓存?issue 缓存表多存几个派生列?)留待试点反馈后再定,不预先设计。
4. **「用了几次」现算要不要跨历史周文件全量扫描。** §2.3 的查询目前只扫本机 `issue` 缓存表(可能只覆盖较近的周);要不要为了拿到"从项目接入以来总共用了几次"这类更长视窗的数字去扫全部历史周文件,还是接受缓存表的滚动窗口,留给 04 篇按实践需要再定。

(「`release_file.rs` 解析老项目已有的 `.bw/releases.md` 时怎么办」**已经答了,不再是开放问题**:`crates/bw-v4/src/repo/release_file.rs` 只认 buddy 自己那张五列表头——版本号、发版日、说明、包含的活、来源——下面的行;认不出这张表头就在文件末尾另起一段「## buddy 管理的发版记录」,项目原有的发版表一个字都不动、也不往里面塞行。回归测试 `crates/bw-v4/tests/repo_files.rs::foreign_release_table_is_never_written_into` 守着这条行为:构造一份列数列名都不同的老发版记录,追加一行后原表原样保留,新内容落进新起的那一段。见 §3.3。)
