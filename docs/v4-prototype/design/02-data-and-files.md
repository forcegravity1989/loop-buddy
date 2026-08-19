# 02 · 数据与文件:库里加什么列、仓里的文件长什么样、信息住哪一层

> **30 秒导读**:这篇管三件事——SQLite 库要新增哪些表/列、项目代码仓里要新增或改哪些文件(每个给完整样例)、以及一样东西该住在「仓 / 本机文件 / 本机库」三层里的哪一层。给谁看:下一步写代码的会话(照着改 `schema.sql` 与 `bw-engine` 的文件解析器)、复核设计的用户。**现在作数吗**:详细设计稿,待用户复核,尚未开工写代码。母文档([`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) §6「信息住哪:一张盘点表」、§7、[`../standard-module-draft.md`](../standard-module-draft.md) §2)与本文冲突时以母文档为准,本文只是把它们落到列级/文件级。**2026-08-20 按用户第二轮回复(六-1/六-3/六-6)整块重写了 §2.1-2.7 与 §2.9(现改称「信息住哪」)**——库的增量从"六张新表"收窄成"一个新列+三张小表",`release`/`release_issue`/`week_plan`/`issue_metric` 四张表与 `project` 的五个新列全部取消,V4 改为不兼容老库。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)——本文不新开代号系列。

## 0 · 这篇管什么、不管什么

**管**:①`crates/bw-store/src/schema.sql` 要加哪些列/表,V4 新库怎么从零写(不再是"迁移守卫");②项目代码仓里 `.bw/*.toml`、`docs/plan/*.md`、`docs/releases.md`、`PROJECT.md` 的完整格式,每个给一份可以直接抄的样例;③一样信息该住在仓、本机文件、还是本机库,谁来取、丢了能不能重建。

**不管**:规范铺底怎么把这些文件第一次写进仓、怎么升级、怎么对账(见 [03-standard-and-backfill.md](03-standard-and-backfill.md));开工工具怎么注册、workflow 怎么识别与注入、`skill_package` 表完整定义(见 [04-tools-and-workflows.md](04-tools-and-workflows.md)§2.6,本文只在信息住哪总表里提一笔);会话屏、计划屏、通知屏的界面结构(见 05/06/07 篇);项目群适配的工厂实现细节(母文档 §7 已定接口是「发消息/拉历史」两个函数,具体留给 07)。本文只管数据落在哪、文件长什么样,不管谁在什么时机去读写它们。

对应母文档:[mvp-blueprint-draft.md](../mvp-blueprint-draft.md) §6「信息住哪:一张盘点表」、第 0/1/2-3/5 站「留下什么」、§2.5/§2.6;[standard-module-draft.md](../standard-module-draft.md) §2 八大类「文件」一栏。涉及待拍:04(在研版本单一)、06(周计划正本进仓)、21(运作活变更走 MR)、24(战绩记在 workflow/技能上)、26(项目群工厂)、27(老项目历史回填)、**29(信息住哪盘点、库是本机的)、30(V4 不兼容老库)**。

## 1 · 用户看到什么、做什么

用户不直接"看"这篇管的东西——这些是底层数据与仓文件,用户从各屏看到的是它们的呈现。但有几处会**直接打开这些文件**:评审运作活①的 MR 时,diff 里就是 `.bw/metrics.toml`、`.bw/project.toml`、`docs/plan/2026-W34.md` 的改动;评审运作活③「规范铺底」的 MR 时,看到仓里新增的 `standard/`(见 03)、`AGENTS.md`、`.bw/issue-policy.toml`、`.bw/standard.toml` 一整套骨架,老项目还会看到 `docs/plan/` 里多出几份带 `origin: backfill` 的历史周文件、`docs/releases.md` 里多出几行标"回填"的历史版本;在「配置」屏改开工工具映射并保存,写回的就是 `.bw/issue-policy.toml`;用 `sqlite3` 读回验证数字时,查的就是本文描述的库表——但**项目名片、项目群配置、规范版本、在研版本这几类信息不在库里**,要核对它们直接 `cat` 仓文件(见 §2.6)。

不会看到、也不需要关心的:`workflow_credit`(战绩账本,呈现在配置屏的 workflow/skill 表里,不是原始表)、`chat_outbox`(通知去重账本)。

## 2 · 设计

### 2.1 库 schema 增量总览

**V4 不兼容老库**(第六轮用户拍板,§2.7 展开):新壳用新的库文件,`schema.sql` 按下表直接写全,不存在"给存量库加列"这件事——下表的"改动"栏描述的是新库 `schema.sql` 里长什么样,不是一条条 `ALTER TABLE`。

| 表 | 改动 | 为什么 |
|---|---|---|
| `issue` | 加 8 列:`week_of`/`version`/`tool`/`kind`/`origin`/`workflow`/`sort_order`/`metric_key` | 挂周、挂在研版本、记开工工具、区分业务活/运作活/轻量活、记来源、记账用的 workflow/技能名、看板列内排序、挂一个推动指标 |
| `workflow_credit`(新)| 战绩台账,替代队友战绩 | 主体从队友(agent)换成 workflow/技能,用**数据库唯一约束**保证同一主体对同一件活绝不记两次 |
| `chat_outbox`(新)| 通知账本 | 项目群发过什么、成败,防止同一件事往群里发两遍 |
| `skill_package`(新,小)| workflow(SOP 类技能包)登记 | 完整定义见 [04 篇](04-tools-and-workflows.md) §2.6,本文只在信息住哪总表(§2.6)里登记它的位置,不重复 DDL |
| `agent`(含 `runs`/`wins`/`win_rate`)| **硬删,`DROP TABLE IF EXISTS`** | 队友战绩由 `workflow_credit` 接管;不可逆决定见 2.3 与 00-handshake 第 2 条 |
| `project` | **不加列** | 项目群/规范版本/在研版本原计划落的五个库存副本列(第五轮草案)**第六轮取消**——这些信息只活在仓文件 `.bw/project.toml`,每次要用现读现解析,不做二次正本(§2.6) |
| `release`/`release_issue`/`week_plan`/`issue_metric` | **取消,不建** | 第五轮草案曾计划建这四张表,第六轮用户「盘点一下、没价值的不要复杂化」后核实:发版记录正本是仓文件 `docs/releases.md`,查询需求只需 `issue.version` 一列就够;周列表靠扫 `docs/plan/` 目录;推动指标一活最多挂一个,单列 `metric_key` 足够,要推多个就拆活——四张表都没有"库里存了但没人取"之外的存在理由 |

### 2.2 `issue` 表增量(列级)

```
week_of      TEXT    NOT NULL DEFAULT ''   -- ISO 周,如 "2026-W34";'' = 待办池(未排进任何一周)
version      TEXT    NOT NULL DEFAULT ''   -- 在研版本标签,如 "v0.3";'' = 未挂版本(常见于运作活)
tool         TEXT    NOT NULL DEFAULT ''   -- 开工工具:'claude_cli' | 'cursor' | 'open_design';'' = 未定
kind         TEXT    NOT NULL DEFAULT 'business'  -- 'business'(业务活) | 'ops'(运作活) | 'light'(轻量活:无 agent 会话,只有 buddy 写仓 + MR;名片编辑、发版本用它;设计期统一,07 篇提议、已定采纳)
origin       TEXT    NOT NULL DEFAULT 'human'      -- 'human' | 'auto' | 'agent_split' | 'backfill'
workflow     TEXT    NOT NULL DEFAULT ''   -- 该活实际用的 workflow / 技能名(记账用)
sort_order   REAL    NOT NULL DEFAULT 0   -- 看板同列内排序,用浮点数支持插入排序(待拍-25 拖拽排期;设计期统一:采纳 06 篇/kanban-drag-dioxus 预研的浮点数插入排序方案,新卡片可插进两张卡之间取中间值,不必整列重排)
metric_key   TEXT    NOT NULL DEFAULT ''   -- 这张活预期推动的指标(metric.id),''=不挂任何指标;一活只挂一个,要挂多个就拆活(第六轮改动,见下)
```

补充几点列上注释没说完的:`week_of` 不外键指到任何周索引表——用 ISO 周文本软关联,一件活可以先标好周、文件还没建出来。`version` 落的是母文档「里程碑不单建实体,版本就是里程碑」这句话。`origin` 里 `backfill`(待拍-27)状态照远端、**不算任何战绩**。`workflow` 同时是 `workflow_credit.subject_id` 的来源。

**为什么 `metric_key` 是单列而不是关联表(第六轮改动,替代早期 `issue_metric` 关联表草案)**:总览的核心画法是「每个指标卡下面列本周哪些活在推它」(§2.5)——这本来是「给定 metric_id 反查哪些 issue 推它」的查询,早期版本因此设计了关联表 `issue_metric`(理由是仓库里有 `skill_stage` 先例)。**用户第二轮复核时把这条改了**:「存就是为了取,没价值的不要复杂化」——一张活在实践中通常只服务一个当下最想推的指标,「一活推多个指标」的真实需求没有出现过,而单列 `WHERE metric_key='<id>'` 已经完全能支撑反查这个索引查询,不需要关联表才能做到。取舍结论:**单列 `metric_key`**,要推动多个指标就把这张活拆成几张——粒度变细本身也更符合「一件活=需要一次 agent 会话」的边界定义(§2.5/§2.6)。08 篇「北极星卡片下面挂着推动它的活」这条画法的查询,从 `issue JOIN issue_metric` 简化成 `issue WHERE metric_key='<id>'`,能力不打折扣。

### 2.3 战绩:`workflow_credit` 新表,`agent` 表怎么处理

待拍-24 定了"战绩记在 workflow/技能上",没定数据库怎么落。今天的战绩落在 `agent.runs`/`wins`/`win_rate` 三列,由应用代码在活结算时 `+1`(见 `crates/bw-app/src/issue_run.rs` 的 `credit_skill_uses`)——"同一件活绝不记两次"这条铁律**只靠代码小心保证**,没有数据库约束兜底。新表用唯一约束把它升级成"数据库物理拒绝":

```sql
CREATE TABLE IF NOT EXISTS workflow_credit (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES project(id),
    subject_kind TEXT NOT NULL,               -- 'workflow' | 'skill'
    subject_id   TEXT NOT NULL,               -- workflow_spec.id 或 skill.id
    issue_id     TEXT NOT NULL REFERENCES issue(id),
    outcome      TEXT NOT NULL,               -- 'done' | 'failed'
    settled_at   INTEGER NOT NULL,
    UNIQUE(subject_kind, subject_id, issue_id)
);
CREATE INDEX IF NOT EXISTS idx_workflow_credit_subject ON workflow_credit(subject_kind, subject_id);
```

`UNIQUE(subject_kind, subject_id, issue_id)` 是核心:同一 workflow(或技能)对同一件活最多插入一行,第二次插入被 SQLite 直接拒绝,不依赖应用代码记得检查。一件活可以同时给 workflow 记一行、给它挂的单技能各记一行(`subject_kind` 不同不冲突)——一件活可能同时"用了 mattpocock-skills 这套 workflow"和"加挂了 grillme 单技能",战绩分开记。

**胜率永远派生,不缓存手设**:表里不存 `uses`/`win_rate` 汇总列,配置屏读时聚合算:

```sql
SELECT subject_id, COUNT(*) AS uses, SUM(outcome='done') AS wins
FROM workflow_credit WHERE subject_kind='workflow' GROUP BY subject_id;
```

这条纪律和"健康信号只能从数据推导、绝不手设"(CLAUDE.md「健康永远推导」)同一个精神,只是这里不需要专门的密封类型去锁构造入口——聚合查询本身就是唯一入口,没有旁路能绕过去手写一个胜率。

**`agent` 表怎么处理(设计期统一:硬删,与 04 篇一致)**:同一次 `schema.sql` 编写里不出现 `agent` 表,并删 `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` 三条命令与 `agent_import.rs`(04 篇 §2.10 已给出具体理由);界面不再展示"队友库"(母文档 §5 已定)。理由:①CLAUDE.md「发现过时的实现路径,直接移除它」——留着只读本身就是一条没人再写、迟早被遗忘的旧路径;②冻结的战绩比没有战绩更容易误导人;③`issue.assignee` 同步退役(见下),"选类别→工具→workflow"完全取代"指派队友"。**这是一次数据丢失操作**(存量 V1-V3 项目的队友战绩历史会消失),不可逆,已提请用户点头——见 00-handshake 第 2 条;因 V4 本身不兼容老库(§2.7),这条丢失是「换库文件」这个更大决定的自然结果,不需要单独一次迁移动作。

**`issue.assignee` 怎么处理(设计期统一,消掉与 04 篇互相甩锅)**:新壳不读不写,`schema.sql` 里 `issue` 表定义不再出现这一列——"选类别→工具→workflow"完全取代"指派队友",02 篇与 04 篇 §2.10 口径一致。

### 2.4 `chat_outbox`(项目群通知账本)

```sql
CREATE TABLE IF NOT EXISTS chat_outbox (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id),
    issue_id        TEXT NOT NULL REFERENCES issue(id),  -- 每条通知都挂在一张活上
    event_type      TEXT NOT NULL,           -- 'review' | 'merged' | 'release'
    sent_at         INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'ok',  -- 'ok' | 'failed'
    external_msg_id TEXT NOT NULL DEFAULT '',    -- 群里那条消息的外部 id,空 = 未拿到
    created_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_chat_outbox_sent_once
    ON chat_outbox(project_id, issue_id, event_type)
    WHERE status = 'ok';
```

**列名设计期统一(与 07 篇一致)**:早期版本用 `event_kind`+`subject_kind`+`subject_id` 三列,是为了同时容纳「issue」与「release」两类通知主体;现在发版本已经改成一张「发版本 vX」轻量活(`issue.kind='light'`,见 §2.2),每条通知天然都能挂在一个 `issue_id` 上,不再需要 `subject_kind` 这层区分——07 篇(此表主要消费方)一直用的就是 `issue_id`+`event_type`,本文改列名与它对齐,去重键相应改成 `(project_id, issue_id, event_type)`。

**为什么用部分唯一索引(partial unique index)**:「不重发」有两层——成功发过的事件绝不重发(硬约束),失败的要允许重试(必须留的口子)。若对整表做普通 `UNIQUE`,第一次失败插一行 `status='failed'` 后,重试插第二行就会撞唯一键报错,重试机制直接被卡死。SQLite 支持 `WHERE` 条件的部分索引,只对 `status='ok'` 强制唯一:失败可以插任意多行留痕,但只要有一行成功,第二行"成功"永远插不进去——数据库物理保证不重复发送成功通知,同时不妨碍重试。

**为什么项目群提供方/群号不在这张表里查**:早期草案里 `chat_outbox` 旁边总带着 `project.chat_provider`/`chat_group_id` 两列做联表展示,第六轮这两列已从 `project` 表撤销(§2.1)——发送时该用哪个 provider、群号是多少,现读 `.bw/project.toml` 的 `[chat]` 段(见 §2.6),`chat_outbox` 只管"发过什么、成没成功"这一份过程账,不重复存配置。

### 2.5 仓文件格式与完整样例

样例项目用 buddy 自己仓里已经在跑的真实项目 **WorkflowHub**(`.bw/metrics.toml` 真实内容见 [`/.bw/metrics.toml`](../../../.bw/metrics.toml));没有真实数据的地方标「演示」。

#### `.bw/project.toml`(现有五字段 + 新增 `[chat]` + 新增 `standard_version`/`current_version`)

```toml
name = "WorkflowHub"
kind = "看板 / 网页应用"
brief = "把 agent 会话里长出的工作流沉淀成可复用资产"
benchmark = "Linear"
opportunity = "被持续复用、效率可量化提升"

# 新增(第六轮,§2.6):规范版本与在研版本的库外正本——这两个值不进
# SQLite,总览/计划屏每次要显示都现解析这个文件。standard_version 与
# `.bw/standard.toml` 的 version 字段保持一致(那份文件是规范对账用的权威
# 记录,03 篇已定);这里的字段是给总览快速展示、不用另外打开一份文件的
# 镜像副本。current_version 是计划屏顶部可切的"在研版本",新建项目默认
# "v0.1"(待拍-04)。
standard_version = "4.0"
current_version = "v0.3"

# 新增(待拍-26):项目群配置。空着不写这一段 = 未配群,总览名片显示
# 「未配 · 配置」。provider 今天只有 "welink" 一个真实实现,外部提供方
# 留空占位(工厂设计见 07-notify-and-chat-group.md)。
[chat]
provider = "welink"
group_id = "638201"          # 演示群号
notify = ["review", "merged", "release"]
```

`[chat]` 是可选表,`standard_version`/`current_version` 是可选字段——旧文件(没有这几项)照样解析成功,和现有三个可选字段同一惯例;`ChatConfig` 沿用 `deny_unknown_fields`,写错键名报错而不是静默丢弃。**这四项(连同现有五字段)都不落库存副本**——第六轮明确取消(§2.1),`project` 表只留定位字段。

#### `.bw/issue-policy.toml`(新文件,规范第 5 类「活的约定」)

```toml
schema_version = 1

# 开工工具声明(设计期统一,抄自 04 篇 §2.1):name → 接法类型(kind:terminal 终端类 /
# web_embed 本机网页内嵌类)→ 探活方式 → 能力清单。这里的 kind 与下面 [[mapping]]
# 段的 category(活的类别标签)是两件不同的事,字段名特意分开,不要混。
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

# 三列映射(待拍-24:去掉 agent 列):类别 → 默认开工工具 → 默认 workflow。
# workflow = SOP 类技能包(自己调度 agent),不是单个技能。
[[mapping]]
category = "prototype"
tool     = "open_design"
workflow = "proto-design"

[[mapping]]
category = "build"
tool     = "claude_cli"
workflow = "mattpocock-skills"   # 二选一为默认,项目可换成 superpowers

[[mapping]]
category = "optimize"            # 优化 / 运维两个阶段类别同款,各一行,此处只示范一行
tool     = "claude_cli"
workflow = "mattpocock-skills"

[[mapping]]
category = "growth"             # 运营推广
tool     = "claude_cli"
workflow = ""                    # 无默认,从鱼塘挑(待拍-10)

[review]
who_can_merge  = "repo_write"    # 不存角色,谁有仓权限谁能点(待拍-13)
require_pr_for = ["code", "docs", "prototype"]

[cadence]
ops1_trigger  = "manual"          # 运作活①:人触发,判据 = 当前周没有 docs/plan/ 文件
ops2_trigger  = "scheduled"
ops2_schedule = "fri 20:00"       # 运作活②:定时,默认周五晚,可改

[kanban]
pool_label = "待办池 · 未排进任何一周"
todo_label = "待办 · 已排进本周,等开工"
# 第五轮改动(待拍-25):所有列都能拖——排期(待办池⇄待办)直接生效,拖到
# 进行中/评审中/已完成/阻塞四列是状态动作,弹确认框才真正发生,不合法的转移
# 松手即弹回;不再有 drag_scope 这种"限定能拖哪些列"的开关。规则细节见
# 06-plan-screen.md §2.3。
```

这份文件由铺底(运作活③)首次写入,之后在「配置」屏改「开工工具映射」并保存时写回——保存动作本身也建一张轻量活走 MR(改仓的动作都走 MR)。

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
[[file]]
path        = "AGENTS.md"
version     = "4.0"
fingerprint = "sha256:9f2a1c7e...b31c"

[[file]]
path        = "PROJECT.md"
version     = "4.0"
fingerprint = "sha256:11ab88de...ff02"

[[file]]
path        = ".bw/issue-policy.toml"
version     = "4.0"
fingerprint = "sha256:77cdaa03...9e10"
```

`fingerprint` 只在铺底/升级那一刻算一次,算法留待 03 定——本文只定文件形状。

#### `docs/plan/2026-W34.md`(周计划,规范第 3 类;新增 front matter)

```markdown
---
week: 2026-W34
origin: human
---

# 2026-W34 周计划

> 正本文件。buddy 读它驱动计划屏与总览「本周计划进度」块;**没有库内索引表**——
> 计划屏左栏的周列表靠扫 `docs/plan/` 目录得到(见 §2.6),这份文件本身就是唯一
> 正本,不存在"文件与库不一致"这类问题。

## 周目标

把 V4 详细设计稿(design/ 十篇)写完并过一轮内部评审,为下一步高保真落地铺路。

## 业务活

| 标题 | 类别 | 工具 | workflow | 预期推动的指标 | 远端 issue |
|---|---|---|---|---|---|
| V4 高保真可点击原型 | 原型 | Open Design | 原型设计 workflow | — | #104 |
| 减负线两轮收尾合入 | 构建 | Claude CLI | mattpocock-skills | 本周合入活数 | #102 |
| 规范铺底模块落地 | 构建 | Claude CLI | mattpocock-skills | 已接入并铺底的内部项目数 | #105 |

## 本周指标读数

<!-- 第五轮新增(待拍-29):运作活①第四步落文件时,把这一步刚更新完的指标现状
     抄一份进来,随 MR 进仓——多机一致性靠这段,而不是靠同步库。演示值。 -->

| 指标 | 数值 | 来源 | 采集时间 |
|---|---|---|---|
| 本周合入活数(引领) | 4 | 脚本采集(`connectors.toml`)| 2026-08-17 09:00 |
| 已接入并铺底的内部项目数(滞后) | 1(演示值,试点未跑完前如实) | 手填 | 2026-08-17 09:00 |

## 本周运作

| 活 | 状态 | 说明 |
|---|---|---|
| 运作活①更新指标 + 制定本周计划 2026-W34 | 已完成 08-17 | 复盘上周、更新指标、引导出本周目标与活 |
| 运作活②资产盘点 2026-W33 | 评审中 | 定时(周五 20:00)自动建并自动开工(`mode=weekly`),等人合入 |
| 运作活③规范铺底 v4.0 | 已完成 08-17 | 含合并调整,一次性,不会再出现在下一周 |

## 上周完成情况(2026-W33)

- 减负线第一轮六切片 + V3-use-fix 三张 → PR #101 已合(08-17,来源:git log)
- 引领指标「本周合入活数」:4(演示值,真实数按 `sqlite3` 读回 `workflow_credit`)

## 运作活②盘点尾段(自动追加,示例)

<!-- 由运作活②「资产盘点」workflow 在 MR 里追加,格式细节见 09-ops-workflows.md -->
- 新增文档 3 篇,均已登记进知识库资产页
- `docs/plan/`、`docs/releases.md` 齐全
- 规范对账:全部件版本一致,无过期、无人改过
- 指标数据新鲜度:全部在保鲜期内
- 代码图大文件榜:本周未发现超 1500 行的文件
- 可做可不做的微重构:无(第五轮改动:发现了也只列建议活,不在这里直接改代码)
```

**回填的历史周文件用同一模板**(第六轮改动,待拍-27/29):`docs/plan/2026-W31.md` 这类历史周文件与人写的本周文件**是同一份规范、同一套段落结构**——front matter 只是 `origin: backfill` 而不是 `human`;没有的段落(最常见是「周目标」,老项目的历史周从来没人手动定过目标)就空着或写"未发现",不硬造。**没有单独的 `docs/plan/history.md` 文件**(第五轮草案曾设计过,第六轮用户否定——「不要增量多一坨」),历史周与本周混在同一个 `docs/plan/` 目录、同一份周列表里,靠 front matter 的 `origin` 与界面上的小徽记区分,不是两套渲染逻辑。示例(回填,演示数字):

```markdown
---
week: 2026-W32
origin: backfill
---

# 2026-W32 周计划

> **回填自 git / 远端**,由运作活③「规范铺底」的「历史回填」步骤(= 运作活②
> 「资产盘点」workflow 的首次模式)生成;只解释历史,不算任何人 / workflow
> 的战绩、不点灯。每次重跑**整段覆盖**,不追加重复段落。

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

#### `docs/releases.md`(发版记录,沿用本仓 `/docs/releases.md` 的表格式;**唯一正本,库不存副本**)

```markdown
# 版本登记(出包与运作)

> **30 秒导读**:一行一个已发布或在研的版本——版本号、发版日、这一版是
> 什么、包含哪些活。回填的行带来源徽记,不代表 buddy 里真发生过评审流程。
> **这份文件是版本记录的唯一正本**(第六轮改动:早期草案曾计划同步一份
> `release` 表进库,现已取消——总览/计划屏「发版记录」块每次直接解析这
> 份文件,活挂哪个版本只看 `issue.version` 一列,两者拼起来就是完整信息,
> 不需要第三份数据)。

| 版本号 | 发版日 | 说明 | 包含的活 | 来源 |
|---|---|---|---|---|
| v0.2 | 2026-08-10 | 找指标/绑数据走嵌入终端 | #88 #91 #93 | 人发 |
| v0.1 | 2026-06-01 | 首个可跑版本(标签回填) | — | 回填 · git tag |
```

表头与本仓自己的 [`/docs/releases.md`](../../releases.md) 一致,新增「来源」列区分「人发」与「回填」——本仓自己目前全是人发,不需要这一列;项目仓的版本记录可能两种都有。「包含的活」列是活号的自由文本(如 `#88 #91 #93`),渲染时按号去查 `issue` 表拿标题展开,不是一张关联表——号找不到对应活时按 03 篇 §2.6 的规则跳过并记警告,不是解析失败。

#### `PROJECT.md`(章程,规范第 1 类,四段 + 信息段)

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

#### `.bw/metrics.toml` —— 不变,只引用

指标正本格式不动,沿用 `docs/buddy/standards/metrics.md` 已有规范(北极星恰好 1 个、`[[lagging]]`/`[[leading]]` 各 0..N、每条必带 `collect`)。真实样例见本仓 [`/.bw/metrics.toml`](../../../.bw/metrics.toml)(WorkflowHub 项目的指标正本,工作区就是 BW 仓自己)。`issue.metric_key` 就指向这份文件同步进库的 `metric.id`。

### 2.6 信息住哪:一张盘点表(第六轮用户要求「存就是为了取,没价值的不要复杂化」)

三条原则(与母文档 §6 一致):①**仓是正本**——人、agent、leader、committer、第二台机器都要看的,进仓、走 MR;②**库只放过程数据与推导缓存**——仓里已有的不复制进库;③**没人取的不存**——每一类数据都要说得出哪个界面/命令会来取,说不出就不建表、不加列。下表把母文档 §6 的盘点结论落到具体文件/表名:

| 信息 | 住哪 | 谁来取 | 备注 |
|---|---|---|---|
| 项目名片:想做什么 / 对标 / 北极星 / 三个月长成什么样 | 仓 `PROJECT.md` | 总览①、项目墙卡片 | 打开项目时解析,不入库 |
| 项目群(提供方/群号/是否同步通知)、规范版本、在研版本 | 仓 `.bw/project.toml`(`[chat]` 段 + `standard_version` + `current_version`) | 总览①⑦、通知同步、发版本 | **不入库副本**(第五轮拟的 `project` +5 列取消,§2.1) |
| 指标定义(北极星/滞后/引领、目标、保鲜期、来源) | 仓 `.bw/metrics.toml` | 总览②③④、运作活① | 同 V3 |
| 指标读数(数据点) | **本机库 `observation`**(只追加)+ 周计划文件「本周指标读数」段(随 MR 共享的副本) | 灯推导、指标卡、运作活① | 过程数据;别的机器看文件副本(待拍-29) |
| 健康灯 | 本机库 `signal` 缓存(只能由推导写) | 总览、项目墙 | 铁律;随时可重算 |
| 开工工具映射 / workflow 表 / 节律 / 看板定义 | 仓 `.bw/issue-policy.toml` | 配置屏、▶开工 | 不入库 |
| 规范清单与托管件指纹 | 仓 `.bw/standard.toml` + `.bw/managed.toml` | 铺底、对账 | 不入库 |
| 周计划(周目标、活清单、指标读数、盘点报告尾段) | 仓 `docs/plan/YYYY-Www.md`(回填的历史周**同格式**,front matter `origin: backfill`) | 计划屏左栏周列表与本周目标、总览⑥、知识库 | **不建 `week_plan` 表**;周列表扫目录;「当前周无计划」判据 = 文件不存在 |
| 发版记录(版本 → 活) | 仓 `docs/releases.md`(回填的历史版本**同格式**,标回填) | 总览⑦、发版本 | **不建 `release` / `release_issue` 表**;活挂版本只用 `issue.version` 一列 |
| 活(Issue):标题 / 状态 / 编号 / 远端链接 / 分支 / MR | **远端 issue 是正本**(GitHub / codehub);本机库 `issue` 行 = 镜像 + 本机扩展列(`week_of` / `sort_order` / `tool` / `workflow` / `kind` / `origin` / `version` / `metric_key`) | 计划屏、会话屏、通知 | 没远端的项目只在库里;**不建 `issue_metric` 关联表**,一活推一指标用单列 `metric_key`,要推多个就拆活 |
| 运行记录(每次 ▶跑 的开工 / 结清 / 成败 / 耗时 / 前后 head)、会话登记 | 本机库 `workflow_run` / `claude_conversation` | 会话屏、战绩计数、吞吐指标 | 过程数据 |
| workflow / 技能战绩 | 本机库 `workflow_credit`(台账,计数现算) | 配置屏「用过几次 / 成败」 | 同一活只记一次 |
| 往群发过什么 | 本机库 `chat_outbox` | 通知「已发到群」、重发去重 | 过程数据 |
| 定时任务与触发记录 | 本机库 `cron_*`(沿用 V3) | 配置屏第④段、运作活② | 过程数据 |
| 导入的技能包登记(名 / 路径 / 版本 / 入口技能) | 本机库 `skill_package`(定义见 04 篇 §2.6) | 配置屏 workflow 表 | 包文件本身在本机磁盘 |
| 产物登记、交棒、蒸馏出的技能(沿用 V3) | 本机库 | 铁律 | 不变 |
| 工具路径(claude / Cursor / Open Design / welink-cli)、工作区根目录、codehub / GitHub 令牌 | 本机磁盘(app 配置文件 / 系统钥匙串) | 设置、探活 | 一台机器一份 |
| WeLink 登录态 | **不归 buddy 管**(用户在本机提前登录;buddy 只探活) | 「测一下」 | 第六轮 |
| 项目 clone、issue worktree、回填 evidence 临时文件、上周群摘要 | 本机磁盘(工作区根目录下 / 临时目录) | ▶开工、回填、运作活① | 用完可删 |
| 界面状态(选中周、面板宽度、最近打开的项目) | 本机磁盘(app 状态文件) | 新壳 | 不入库 |
| **不存**:agent 名单、健康快照、群历史、`docs/plan/history.md`、`release` / `week_plan` / `issue_metric` 表、`project` 的名片 / 群 / 版本副本列 | — | — | 第五/六轮删 |

**丢了能不能重建**(比上表多一维,写代码时留意哪些数据一旦丢就永久丢):`observation`/`workflow_run`/产物登记/`workflow_credit`/`chat_outbox`/定时触发记录都是只追加的过程数据,**不能重建**,唯一来源是当时的采集/手填/运行过程;仓文件(`PROJECT.md`/`.bw/*`/`docs/plan/`/`docs/releases.md`)与从它们镜像出的库字段能重建(仓在,重新解析/重新扫描即可);`signal` 随时可由 `recompute_signals` 重算;`skill_package`/技能导入登记如果包文件还在本机磁盘,重新扫一遍能重建,包文件本身没了就丢了。

### 2.7 V4 不兼容老库(第六轮用户拍板:「老库数据我不需要兼容」「表名列名按新设计来」)

新壳用**新的库文件**(默认文件名带 `v4`,与 V3 的库文件并存互不影响),`schema.sql` 按 §2.1-2.4 的设计**直接写全**——新列写在 `CREATE TABLE` 语句里,`agent`/`release`/`release_issue`/`week_plan`/`issue_metric` 这些不出现在新 schema 里的表,根本不需要写 `DROP TABLE`(它们从未在新库存在过)。

**不写任何 V3→V4 数据迁移**:V1-V3 项目的队友战绩、旧版本记录等历史数据不搬运,用户已明确接受这个取舍(00-handshake 六-1)。V4 开发期间每次改 `schema.sql`,直接删库重建(`rm <db> && cargo run ...` 或指挥器自带的临时库路径),不写迁移脚本、不加 `add_column_if_missing` 调用——开发阶段还没有需要保护的真实用户数据。

**`add_column_if_missing` 双守卫纪律从试点起再恢复执行**:一旦有第一个真实用户开始用 V4 库存了数据(内部试点,见 [10 篇](10-e2e-acceptance.md) §2.4),这份库就变成了"需要保护的存量库",此后再给 `schema.sql` 加列,必须回到 CLAUDE.md「schema 迁移双守卫」——同步改 `schema.sql` 并在 `sqlite.rs::SqliteStore::open()` 加 `add_column_if_missing(...)`,不能再靠"删库重建"图省事。这条时间线上的切换点(何时从"开发期删库重建"切到"生产期双守卫")本身要在试点开始那次改动里明确标注,不是自然过渡。

## 3 · 工程对照

### 3.1 `schema.sql`(V4 新库,直接写全,不是增量 diff)

`issue` 表定义(新库从零写,`updated_at` 列之后追加):

```sql
week_of TEXT NOT NULL DEFAULT '', version TEXT NOT NULL DEFAULT '', tool TEXT NOT NULL DEFAULT '',
kind TEXT NOT NULL DEFAULT 'business', origin TEXT NOT NULL DEFAULT 'human',
workflow TEXT NOT NULL DEFAULT '', sort_order REAL NOT NULL DEFAULT 0,
metric_key TEXT NOT NULL DEFAULT '',
```

`project` 表**不追加任何列**——保持今天已有的定位字段(路径/远端/名称/工作区等),`standard_version`/`current_version`/`chat_provider`/`chat_group_id`/`chat_notify` 均不出现在 schema 里。

新表(`schema.sql` 里与其它表并列,`CREATE TABLE IF NOT EXISTS` 语法习惯保留但对新库而言恒为首次创建):`workflow_credit`(DDL 见 §2.3)、`chat_outbox`(DDL 见 §2.4)、`skill_package`(DDL 见 04 篇 §2.6)。**不出现**:`release`、`release_issue`、`week_plan`、`issue_metric`、`agent`。

### 3.2 新库不需要 `add_column_if_missing`(开发期);试点起恢复守卫

开发期(§2.7 已定):`schema.sql` 每次改了就删库重建,`sqlite.rs::SqliteStore::open()` **不需要**为本文列出的任何一列写 `add_column_if_missing` 调用。这与 CLAUDE.md「schema 迁移双守卫」纪律不冲突——那条纪律保护的是"已经有数据的存量库",V4 开发期还没有这样的库。

试点开始后(有第一份真实数据的 V4 库出现),后续任何新加列都要恢复双守卫写法,例如:

```rust
add_column_if_missing(&pool, "issue", "metric_key", "TEXT NOT NULL DEFAULT ''").await?;
```

这只是一个示例格式,试点前的 `issue` 8 列本身不需要这行代码(它们随新库首次创建就已经在 `CREATE TABLE` 语句里)。

### 3.3 新增仓文件解析器模块(`crates/bw-engine/src/`)

跟随 `project_file.rs`/`metrics_file.rs`/`connectors_file.rs` 已立好的模式:只读+解析,`deny_unknown_fields`,`Ok(None)` = 文件不存在(诚实无事发生),解析失败是 `Err` 且绝不写半份缓存。四个新增点:

- `issue_policy_file.rs`(新):`IssuePolicyFile { schema_version, mappings: Vec<CategoryMapping{category,tool,workflow}>, review, cadence, kanban }`,对照 2.5 样例的四个表逐段声明;`read(workspace) -> Result<Option<IssuePolicyFile>, _>` 同 `project_file::read` 骨架。
- `standard_file.rs`(新):`StandardFile { version, enabled: Vec<String>, extensions: Vec<String>, source }`,对照 `.bw/standard.toml` 样例。
- `week_plan_file.rs`(新):Markdown 不走 serde,只有一个 `extract_goal(markdown) -> Option<String>`,取「## 周目标」后第一段非空文本——**不写进任何库表**(第六轮取消 `week_plan` 索引),供计划屏渲染周头时现读现用;另有 `extract_front_matter(markdown) -> Option<WeekFrontMatter{week, origin}>` 供周列表判断某个文件是不是回填(`origin: backfill`)。不解析「业务活」表格,活的正本已经是 `issue` 行。
- `release_file.rs`(新):Markdown 表格解析器,`read(workspace) -> Result<Option<Vec<ReleaseRow{version, released_at, note, included_issue_numbers, origin}>>, _>`——**只读,不 upsert 任何库表**(第六轮取消 `release` 表),供总览⑦块与计划屏发版时直接渲染;「包含的活」列的号解析成 `Vec<u64>`,找不到对应 issue 时该号跳过并记警告,不是解析失败。
- `project_file.rs`(改现有文件,不新开):`ProjectFile` 加三个字段——`#[serde(default)] pub chat: Option<ChatConfig>`、`#[serde(default)] pub standard_version: String`、`#[serde(default)] pub current_version: String`;新增 `ChatConfig { provider: String, group_id: String, #[serde(default)] notify: Vec<String> }`,同款 `deny_unknown_fields`。**这四个新字段读出来只给界面直接用,不再有"同步进库"这一步**(§2.6 已定不入库)。

### 3.4 `bw-app` 新增命令(草拟,精确签名留待各屏设计篇定)

- `SyncIssuePolicyFile` / `SyncStandardFile`:语义对齐 `SyncMetricsFile`/`sync_connectors_file_for`——文件不存在零动作、坏文件只报错不写库、幂等 upsert(这两份文件本身仍需要一部分内容——如规范对账用的指纹——写进库供查询,不是本文§2.6划走的那几类字段)。
- `RecordChatOutbox { project_id, issue_id, event_type }`:发消息前先查 `uq_chat_outbox_sent_once` 覆盖的条件(是否已有一行 `status='ok'`),命中则跳过;未命中才真调项目群适配模块发送,无论成败都插一行。
- `RecordWorkflowCredit { project_id, subject_kind, subject_id, issue_id, outcome }`:在活结算(`settled_at` 首次写入)的同一事务里对每个挂在这件活上的 workflow/技能各插一行;`origin='backfill'` 的 issue **不触发**(调用方判断 `issue.origin != 'backfill'` 才调用)。

**不再需要 `RecordRelease`**(第六轮取消):早期草案里这条命令负责在合入时把一行写进 `release` 表,现在没有这张表——发版本这件事完全靠仓文件(`docs/releases.md` 追加一行,随 [06 篇](06-plan-screen.md) `CutRelease` 的 MR)与库里 `issue.version` 这一列(建那张「发版本 vX」轻量活时,同批把选中的活各自的 `version` 写好)共同表达,合入那一刻不需要再额外写一行库记录。

## 4 · 边界与失败

**不做什么**:不建成员/角色表(committer 靠 `.bw/issue-policy.toml` 的 `who_can_merge = "repo_write"` 一句话表达权限,不建 `member`/`role` 实体——CONTEXT.md 已明确"buddy 里不建成员、权限、群聊、收件箱")。不给里程碑单独建表(`docs/releases.md` 一行就是一个里程碑,库里连一行镜像都不需要)。群消息正文不进库(`chat_outbox` 只记事件+成败,不存消息文本;运作活①的群摘要是本机文件,读完即用,不进仓不进库)。不给 `metric` 定义加任何 V4 字段(`issue.metric_key` 只是新增的单向引用,`metric` 表本身不变)。不做任何 V3→V4 的数据迁移脚本(§2.7)。

**失败如实显示**:三个新 TOML 解析器沿用 `deny_unknown_fields` + all-or-nothing,报错不写库,和 `project_file.rs`/`connectors_file.rs` 今天行为一致。`chat_outbox` 发送失败落一行 `status='failed'`(不是不记也不是假装成功),下次因"没有 `status='ok'` 的行"而重试,重试次数不设上限(连续失败的交互留给 07)。`release_file.rs` 遇到"包含的活"列里找不到对应 issue 的号,跳过这条关联并记警告、不让整份文件解析失败。开发期删库重建全程幂等(删了重建总是得到同一份空 schema);试点起恢复的 `add_column_if_missing` 同样全程幂等。

## 5 · 验收与读回

`<db>` 替换成 E2E 用的深链启动数据库路径(V4 新库文件,与任何 V3 库无关),`<pid>` 替换成项目 id,`<ws>` 替换成项目工作区路径。

| 核验什么 | 读回 | 预期 |
|---|---|---|
| 新库 schema 完整,一次到位 | `sqlite3 <db> "PRAGMA table_info(issue);"` | 直接看到全部 8 列(`week_of`/`version`/`tool`/`kind`/`origin`/`workflow`/`sort_order`/`metric_key`),不需要任何 `add_column_if_missing` 参与——因为这是一份全新写的 `schema.sql` |
| `agent`/`release`/`week_plan`/`issue_metric` 均不存在 | `sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('agent','release','release_issue','week_plan','issue_metric');"` | 空结果——新 schema 里从未定义过这几张表 |
| `project` 表没有新列 | `sqlite3 <db> "PRAGMA table_info(project);"` | 不出现 `standard_version`/`current_version`/`chat_provider`/`chat_group_id`/`chat_notify` |
| 回填 issue 的来源分布 | `sqlite3 <db> "SELECT origin, COUNT(*) FROM issue GROUP BY origin;"` | 老项目接入后 `backfill` 一档非零;新项目只有 `human`/`auto`/`agent_split` |
| 本周业务活数与仓文件一致 | `sqlite3 <db> "SELECT COUNT(*) FROM issue WHERE project_id='<pid>' AND kind='business' AND week_of='2026-W34';"` | 等于 `<ws>/docs/plan/2026-W34.md`「业务活」表格行数 |
| 项目群配置不比对库(不入库)| `cat <ws>/.bw/project.toml`(看 `[chat]` 段)+ 深链 `BW_PANEL=overview` 截图 | `.bw/project.toml` 的 `provider`/`group_id`/`notify` 与总览名片显示的一致;`sqlite3` 查不到任何 `chat_*` 列——这就是预期 |
| 战绩绝不记两次(数据库约束,不是代码审查)| `sqlite3 <db> "SELECT subject_kind, subject_id, issue_id, COUNT(*) FROM workflow_credit GROUP BY subject_kind, subject_id, issue_id HAVING COUNT(*) > 1;"` | 空结果——尝试重复 `INSERT` 应被 `UNIQUE` 约束物理拒绝,不只是查出来没有 |
| 回填的活不进战绩 | `sqlite3 <db> "SELECT COUNT(*) FROM workflow_credit wc JOIN issue i ON wc.issue_id=i.id WHERE i.origin='backfill';"` | `0` |
| 项目群通知不重发 | `sqlite3 <db> "SELECT project_id, issue_id, event_type, COUNT(*) FROM chat_outbox WHERE status='ok' GROUP BY project_id, issue_id, event_type HAVING COUNT(*) > 1;"` | 空结果 |
| 发版记录只在仓文件、不在库 | `tail <ws>/docs/releases.md` + `sqlite3 <db> "SELECT version FROM issue WHERE kind='light' AND title LIKE '发版本 %';"` | 仓文件里的行与「发版本 vX」轻量活的 `issue.version` 一一对应;库里没有独立 `release` 表可查 |
| 指标挂活反查(验证 §2.2 画法)| `sqlite3 <db> "SELECT title FROM issue WHERE metric_key='<metric_id>';"` | 与总览该指标卡下方展示的活标题一一对应 |
| `agent` 表从未存在 | `sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name='agent';"` | 空结果 |

## 6 · 开放问题(≤5)

1. ~~`issue_metric` 用关联表还是 `issue.metric_ids` JSON 数组?~~ **已定(第六轮改):都不是,单列 `issue.metric_key`**(§2.2)——用户复核后判定「一活推多个指标」不是真实需求,单列已能支撑反查查询,详见 §2.2 理由段。
2. ~~`agent` 表最终物理退役方式,与 `issue.assignee` 的存亡~~ **已定(设计期统一,第六轮进一步简化)**:V4 新库 `schema.sql` 里从未出现 `agent` 表与 `issue.assignee` 列,不是"迁移删除",是"新库从未创建过";不可逆决定(存量 V1-V3 项目战绩不迁移)已提请用户点头(00-handshake 第 2 条)。
3. **`.bw/managed.toml` 的指纹算法与对账触发时机。** 本文只定文件形状(§2.5),具体摘要算法、比对时机留给 [03-standard-and-backfill.md](03-standard-and-backfill.md)。
4. **`release_file.rs` 解析老项目已有 `docs/releases.md` 的鲁棒性边界。** 如果项目铺底前就有一份格式不同的发版记录,历史回填要规范化它、并行开新文件、还是兼容多种表头?属于回填流程细节,留给 03(§2.7 附言:这个问题本身在第六轮改动前后没变化,仍然开放)。
5. **`chat_outbox` 的失败重试策略(退避、上限、还是无限重试等到成功)。** 本文只给了数据库层"不重复发送成功通知"的约束(§2.4),重试调度策略留给 [07-notify-and-chat-group.md](07-notify-and-chat-group.md)(要先定项目群适配工厂的两个函数接口)。
