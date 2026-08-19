# 02 · 数据与文件:库里加什么列、仓里的文件长什么样、信息住哪一层

> **30 秒导读**:这篇管三件事——SQLite 库要新增哪些表/列、项目代码仓里要新增或改哪些文件(每个给完整样例)、以及一样东西该住在「仓 / 本机文件 / 本机库」三层里的哪一层。给谁看:下一步写代码的会话(照着改 `schema.sql` 与 `bw-engine` 的文件解析器)、复核设计的用户。**现在作数吗**:详细设计稿,待用户复核,尚未开工写代码。母文档([`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) §6/§7、[`../standard-module-draft.md`](../standard-module-draft.md) §2)与本文冲突时以母文档为准,本文只是把它们落到列级/文件级。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)——本文不新开代号系列。

## 0 · 这篇管什么、不管什么

**管**:①`crates/bw-store/src/schema.sql` 要加哪些列/表,配套的 `add_column_if_missing` 迁移守卫怎么写;②项目代码仓里 `.bw/*.toml`、`docs/plan/*.md`、`docs/releases.md`、`PROJECT.md` 的完整格式,每个给一份可以直接抄的样例;③一样信息该住在仓、本机文件、还是本机库,丢了能不能重建。

**不管**:规范铺底怎么把这些文件第一次写进仓、怎么升级、怎么对账(见 [03-standard-and-backfill.md](03-standard-and-backfill.md));开工工具怎么注册、workflow 怎么识别与注入(见 [04-tools-and-workflows.md](04-tools-and-workflows.md));会话屏、计划屏、通知屏的界面结构(见 05/06/07 篇);项目群适配的工厂实现细节(母文档 §7 已定接口是「发消息/拉历史」两个函数,具体留给 07)。本文只管数据落在哪、文件长什么样,不管谁在什么时机去读写它们。

对应母文档:[mvp-blueprint-draft.md](../mvp-blueprint-draft.md) §6「数据模型增量」、第 0/1/2-3/5 站「留下什么」、§2.5/§2.6;[standard-module-draft.md](../standard-module-draft.md) §2 八大类「文件」一栏。涉及待拍:04(在研版本单一)、06(周计划正本进仓)、21(运作活变更走 MR)、24(战绩记在 workflow/技能上)、26(项目群工厂)、27(老项目历史回填)。

## 1 · 用户看到什么、做什么

用户不直接"看"这篇管的东西——这些是底层数据与仓文件,用户从各屏看到的是它们的呈现。但有几处会**直接打开这些文件**:评审运作活①的 MR 时,diff 里就是 `.bw/metrics.toml`、`.bw/project.toml`、`docs/plan/2026-W34.md` 的改动;评审运作活③「规范铺底」的 MR 时,看到仓里新增的 `standard/`(见 03)、`AGENTS.md`、`.bw/issue-policy.toml`、`.bw/standard.toml` 一整套骨架;打开知识库屏「知识」页签看到 `docs/plan/`、`docs/releases.md`、`docs/plan/history.md`(老项目才有);在「配置」屏改开工工具映射并保存,写回的就是 `.bw/issue-policy.toml`;用 `sqlite3` 读回验证数字时,查的就是本文描述的库表。

不会看到、也不需要关心的:`week_plan` 这类索引表本身(正本是文件)、`chat_outbox`(通知账本)、`workflow_credit`(战绩账本,呈现在配置屏的 workflow/skill 表里,不是原始表)。

## 2 · 设计

### 2.1 库 schema 增量总览

沿用双守卫纪律(CLAUDE.md「schema 迁移双守卫」):每加一列同时改 `schema.sql`(供新库)并在 `sqlite.rs` 的 `SqliteStore::open()` 里加一条 `add_column_if_missing(...)`(供存量库);新表 `CREATE TABLE IF NOT EXISTS` 即为充分守卫,不需要配 `add_column_if_missing`(参照现有 `app_meta`/`claude_conversation` 两张新表的先例)。

| 表 | 改动 | 为什么 |
|---|---|---|
| `issue` | 加 7 列:`week_of`/`version`/`tool`/`kind`/`origin`/`workflow`/`sort_order` | 挂周、挂在研版本、记开工工具、区分业务活/运作活、记来源、记账用的 workflow/技能名、看板列内排序 |
| `project` | 加 5 列:`standard_version`/`current_version`/`chat_provider`/`chat_group_id`/`chat_notify` | 规范版本与在研版本(库存副本,正本在仓)、项目群配置(正本在 `.bw/project.toml` 的 `[chat]` 段) |
| `release`(新)| 版本记账 | 第 5 站「发版本」与第 0 站「历史回填」都要落一行 |
| `release_issue`(新)| 关联表 | 一个版本包含哪些活,避免在 `release` 行里塞 JSON |
| `issue_metric`(新)| 关联表 | 一张活可标 0..n 个「预期推动的指标」,替代 `metric_ids` JSON(理由见 2.2) |
| `week_plan`(新)| 周索引 | `docs/plan/YYYY-Www.md` 是正本,这张表只存「文件在哪、周目标一句」给列表页用 |
| `workflow_credit`(新)| 替代队友战绩 | 主体从队友(agent)换成 workflow/技能,用**数据库唯一约束**保证同一主体对同一件活绝不记两次 |
| `chat_outbox`(新)| 通知账本 | 项目群发过什么、成败,防止同一件事往群里发两遍 |
| `agent`(含 `runs`/`wins`/`win_rate`)| **硬删,`DROP TABLE IF EXISTS`** | 队友战绩由 `workflow_credit` 接管;不可逆决定见 2.4 与 00-handshake 第 2 条 |

### 2.2 `issue` 表增量(列级)

```
week_of      TEXT    NOT NULL DEFAULT ''   -- ISO 周,如 "2026-W34";'' = 待办池(未排进任何一周)
version      TEXT    NOT NULL DEFAULT ''   -- 在研版本标签,如 "v0.3";'' = 未挂版本(常见于运作活)
tool         TEXT    NOT NULL DEFAULT ''   -- 开工工具:'claude_cli' | 'cursor' | 'open_design';'' = 未定
kind         TEXT    NOT NULL DEFAULT 'business'  -- 'business'(业务活) | 'ops'(运作活) | 'light'(轻量活:无 agent 会话,只有 buddy 写仓 + MR;名片编辑、发版本用它;设计期统一,07 篇提议、已定采纳)
origin       TEXT    NOT NULL DEFAULT 'human'      -- 'human' | 'auto' | 'agent_split' | 'backfill'
workflow     TEXT    NOT NULL DEFAULT ''   -- 该活实际用的 workflow / 技能名(记账用)
sort_order   REAL    NOT NULL DEFAULT 0   -- 看板同列内排序,用浮点数支持插入排序(待拍-25 拖拽排期;设计期统一:采纳 06 篇/kanban-drag-dioxus 预研的浮点数插入排序方案,新卡片可插进两张卡之间取中间值,不必整列重排)
```

补充几点列上注释没说完的:`week_of` 不外键指到 `week_plan`——用 ISO 周文本软关联,一件活可以先标好周、文件还没建出来。`version` 落的是母文档 §6「里程碑不单建实体,版本就是里程碑」这句话。`origin` 里 `backfill`(待拍-27)状态照远端、**不算任何战绩**。`workflow` 同时是 `workflow_credit.subject_id` 的来源。

**为什么不做 `metric_ids` JSON 而是关联表 `issue_metric`**:总览的核心画法是「每个指标卡下面列本周哪些活在推它」(§2.5)——这是「给定 metric_id 反查哪些 issue 推它」的查询,JSON 文本列做不到索引查询;仓库里已有同类先例 `skill_stage(skill_id, stage)`,不是 `skill.stages` JSON。跟随先例:

```sql
CREATE TABLE IF NOT EXISTS issue_metric (
    issue_id  TEXT NOT NULL REFERENCES issue(id),
    metric_id TEXT NOT NULL REFERENCES metric(id),
    PRIMARY KEY (issue_id, metric_id)
);
CREATE INDEX IF NOT EXISTS idx_issue_metric_by_metric ON issue_metric(metric_id);
```

本文采纳这个方案(设计期统一,已定:00-handshake 第 11 条)。

### 2.3 `project` 表增量(列级)

```
standard_version  TEXT NOT NULL DEFAULT ''      -- 规范版本(库存副本,正本是 .bw/standard.toml 的 version)
current_version   TEXT NOT NULL DEFAULT ''      -- 在研版本(计划屏顶部可切;新建项目默认 'v0.1')
chat_provider     TEXT NOT NULL DEFAULT ''      -- 项目群提供方:'welink' | ''(未配)
chat_group_id     TEXT NOT NULL DEFAULT ''      -- 群号;空 = 未配
chat_notify       TEXT NOT NULL DEFAULT '[]'    -- JSON [String],同步到群的事件子集
```

四列都是**库存副本**,遵循今天 `north_star`/`north_star_collect_kind` 已经在用的模式——正本在仓文件,新增的 `SyncStandardFile`/`SyncProjectFile`(见 3.4)读文件后 upsert,BW 从不反向改写文件。`chat_notify` 用 JSON 而不是关联表:它只在"展示是否勾选"这一个场景被用到,没有反查需求,和 `issue_metric` 的取舍结论不同是因为使用模式不同,不是标准不一致。

### 2.4 战绩:`workflow_credit` 新表,`agent` 表怎么处理

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

**`agent` 表怎么处理(设计期统一:硬删,与 04 篇一致)**:同一次迁移里 `DROP TABLE IF EXISTS agent`,并删 `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` 三条命令与 `agent_import.rs`(04 篇 §2.10 已给出具体理由);界面不再展示"队友库"(母文档 §5 已定)。理由:①CLAUDE.md「发现过时的实现路径,直接移除它」——留着只读本身就是一条没人再写、迟早被遗忘的旧路径;②冻结的战绩比没有战绩更容易误导人;③`issue.assignee` 同步退役(见下),"选类别→工具→workflow"完全取代"指派队友"。**这是一次数据丢失操作**(存量 V1-V3 项目的队友战绩历史会消失),不可逆,已提请用户点头——见 00-handshake 第 2 条。

**`issue.assignee` 怎么处理(设计期统一,消掉与 04 篇互相甩锅)**:新壳不读不写,同一次迁移物理删除该列(SQLite ≥ 3.35 用 `ALTER TABLE issue DROP COLUMN assignee`,更早版本走「建新表→拷数据→改名」的重建套路)——"选类别→工具→workflow"完全取代"指派队友",02 篇与 04 篇 §2.10 口径一致。

### 2.5 `release` 与 `release_issue`

```sql
CREATE TABLE IF NOT EXISTS release (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES project(id),
    version      TEXT NOT NULL,               -- 如 "v0.3"
    released_at  INTEGER NOT NULL,            -- unix 秒
    note         TEXT NOT NULL DEFAULT '',
    origin       TEXT NOT NULL DEFAULT 'human',  -- 'human'(第 5 站人发版) | 'backfill'(老项目回填)
    created_at   INTEGER NOT NULL,
    UNIQUE(project_id, version)
);
CREATE TABLE IF NOT EXISTS release_issue (
    release_id TEXT NOT NULL REFERENCES release(id),
    issue_id   TEXT NOT NULL REFERENCES issue(id),
    PRIMARY KEY (release_id, issue_id)
);
```

"包含的活"用关联表而不是 JSON,理由与 `issue_metric` 一致(总览「在研版本与发版记录」块要展开某版本包含哪些活)。`origin` 区分人发版还是老项目回填,回填的版本行来自 `docs/releases.md` 历史段(标签/CHANGELOG 解析)。

### 2.6 `week_plan`(仓文件为正本,库存索引)

```sql
CREATE TABLE IF NOT EXISTS week_plan (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    week_of     TEXT NOT NULL,               -- "2026-W34"
    goal        TEXT NOT NULL DEFAULT '',    -- 周目标一句(展示用镜像)
    file_path   TEXT NOT NULL,               -- "docs/plan/2026-W34.md"
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    UNIQUE(project_id, week_of)
);
```

存在的唯一理由是**列表页不用每次扫文件系统**——计划屏左栏「周列表」要快速渲染,查这张索引表比逐个读文件快。真内容永远以 `docs/plan/YYYY-Www.md` 为准,`goal` 只是镜像,冲突时以文件覆盖库缓存。

### 2.7 `chat_outbox`(项目群通知账本)

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

### 2.8 仓文件格式与完整样例

样例项目用 buddy 自己仓里已经在跑的真实项目 **WorkflowHub**(`.bw/metrics.toml` 真实内容见 [`/.bw/metrics.toml`](../../../.bw/metrics.toml));没有真实数据的地方标「演示」。

#### `.bw/project.toml`(现有五字段 + 新增 `[chat]`)

```toml
name = "WorkflowHub"
kind = "看板 / 网页应用"
brief = "把 agent 会话里长出的工作流沉淀成可复用资产"
benchmark = "Linear"
opportunity = "被持续复用、效率可量化提升"

# 新增(待拍-26):项目群配置。空着不写这一段 = 未配群,总览名片显示
# 「未配 · 配置」。provider 今天只有 "welink" 一个真实实现,外部提供方
# 留空占位(工厂设计见 07-notify-and-chat-group.md,未成稿)。
[chat]
provider = "welink"
group_id = "638201"          # 演示群号
notify = ["review", "merged", "release"]
```

`[chat]` 是可选表,旧文件(没有这段)照样解析成功,和现有三个可选字段同一惯例;`ChatConfig` 沿用 `deny_unknown_fields`,写错键名报错而不是静默丢弃。

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

#### `docs/plan/2026-W34.md`(周计划,规范第 3 类)

```markdown
# 2026-W34 周计划

> 正本文件。buddy 读它驱动计划屏与总览「本周计划进度」块;库里 `week_plan`
> 表只存索引,真内容以此文件为准,冲突时以文件覆盖库缓存。

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
- 引领指标「本周合入活数」:4(演示值,真实数按 `sqlite3` 读回 `workflow_credit`/`release_issue`)

## 运作活②盘点尾段(自动追加,示例)

<!-- 由运作活②「资产盘点」workflow 在 MR 里追加,格式细节见 09-ops-workflows.md -->
- 新增文档 3 篇,均已登记进知识库资产页
- `docs/plan/`、`docs/releases.md` 齐全
- 规范对账:全部件版本一致,无过期、无人改过
- 指标数据新鲜度:全部在保鲜期内
- 代码图大文件榜:本周未发现超 1500 行的文件
- 可做可不做的微重构:无(第五轮改动:发现了也只列建议活,不在这里直接改代码)
```

#### `docs/plan/history.md`(老项目回填,规范第 3 类扩展,待拍-27)

```markdown
# 历史运作(回填)

> **回填自 git / 远端,只解释历史,不算任何人 / workflow 的战绩、不点灯。**
> 由运作活③「规范铺底」的「历史回填」步骤生成;每次重跑**整段覆盖**(幂等,
> 不追加重复段落)。人写的周计划在 `docs/plan/YYYY-Www.md`,不要手改本文件。

## 按周历史运作

| 周 | 合入 MR 数 | 提交数 | 动过的目录 Top3 | 关闭 issue 数 | 当周版本 |
|---|---|---|---|---|---|
| 2026-W32 | 3 | 21 | `crates/bw-app`、`docs`、`crates/bw-store` | 5 | v0.3.0-v3 |
| 2026-W31 | 2 | 14 | `crates/bw-engine`、`docs`、`crates/ui` | 2 | — |
| … | | | | | |

数据来源:git 合入记录(`git log --merges`)、`git log --numstat` 按目录聚合、远端 issue/MR 列表。没有的字段留空,不发明数据。
```

#### `docs/releases.md`(发版记录,沿用本仓 `/docs/releases.md` 的表格式)

```markdown
# 版本登记(出包与运作)

> **30 秒导读**:一行一个已发布或在研的版本——版本号、发版日、这一版是
> 什么、包含哪些活。回填的行带来源徽记,不代表 buddy 里真发生过评审流程。

| 版本号 | 发版日 | 说明 | 包含的活 | 来源 |
|---|---|---|---|---|
| v0.2 | 2026-08-10 | 找指标/绑数据走嵌入终端 | #88 #91 #93 | 人发 |
| v0.1 | 2026-06-01 | 首个可跑版本(标签回填) | — | 回填 · git tag |
```

表头与本仓自己的 [`/docs/releases.md`](../../releases.md) 一致,新增「来源」列区分「人发」与「回填」——本仓自己目前全是人发,不需要这一列;项目仓的版本记录可能两种都有。

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
- 在研版本:v0.3(来自 `.bw/project.toml` 的 `current_version`,库存副本)
- 项目群:WorkflowHub 日常群(WeLink;群号见 `.bw/project.toml` 的 `[chat]` 段,登录态在本机设置)
```

#### `.bw/metrics.toml` —— 不变,只引用

指标正本格式不动,沿用 `docs/buddy/standards/metrics.md` 已有规范(北极星恰好 1 个、`[[lagging]]`/`[[leading]]` 各 0..N、每条必带 `collect`)。真实样例见本仓 [`/.bw/metrics.toml`](../../../.bw/metrics.toml)(WorkflowHub 项目的指标正本,工作区就是 BW 仓自己)。`issue_metric` 的 `metric_id` 就指向这份文件同步进库的 `metric.id`。

### 2.9 信息三层住法表

按「丢了能不能重建」逐样列出(比母文档 §6 三行汇总表更细):

| 东西 | 住哪 | 丢了能不能重建 |
|---|---|---|
| PROJECT.md 章程 | 仓 | 能,北极星在 `.bw/metrics.toml` 有底稿 |
| AGENTS.md / CLAUDE.md | 仓 | 能重铺正本;项目自定义段是人写的,真丢了就是丢了 |
| `.bw/project.toml`(含 `[chat]`)| 仓 | 能,前提是改动一直走 MR 合入仓 |
| `.bw/metrics.toml` / `.bw/connectors.toml` | 仓 | 能,是正本 |
| `.bw/issue-policy.toml` / `.bw/standard.toml` / `.bw/managed.toml` | 仓 | 能,是正本 |
| `docs/plan/YYYY-Www.md` | 仓 | 能,是正本;库 `week_plan` 只是索引 |
| `docs/plan/history.md`(老项目回填)| 仓 | 能部分,重跑历史回填重生成,前提是 git/远端数据还在 |
| `docs/releases.md` | 仓 | 能,是正本 |
| 技能 / workflow(`.claude/skills/`)、产物 | 仓 | 能,是正本;`artifact` 表只是登记 |
| claude / Cursor / Open Design 路径与预算 | 本机文件 | 丢了重配,不影响项目 |
| codehub / GitHub / 聊天工具登录态(新)| 本机文件 | 丢了重登,不影响项目 |
| 每活的 worktree | 本机文件(路径)+ 仓(分支内容)| 目录丢了能从分支重建 |
| 终端会话缓存 | 本机文件 | 回滚缓冲没了;claude 自己的 `session.jsonl` 可能仍在 |
| 上周群消息摘要(新)| 本机文件 | 用完可删,丢了下次运作活①重拉即可 |
| `project` 行(库)| 库 | 能,大部分字段能从仓重新 sync;`signal` 随时重算 |
| `issue` 行(库)| 库 | 挂远端仓的能重建标题/状态/号码;`week_of`/`tool`/`kind`/`origin`/`workflow` 这些 buddy 专属属性远端没有,重建不出来 |
| 观测、运行记录、产物登记(库,只追加)| 库 | **不能重建**,唯一来源是当时的采集/手填/运行过程 |
| `workflow_credit`(新)| 库 | 不能重建(除非重跑一遍所有活,不现实) |
| `release` 行(库)| 库 | 能,`docs/releases.md` 可重新同步;`release_issue` 细节可能丢 |
| `week_plan` 索引(库)| 库 | 能,扫一遍 `docs/plan/*.md` 重建 |
| `chat_outbox`(新)、定时触发记录(库)| 库 | 不能重建,过程数据 |
| 信号缓存(库)| 库 | 能,`recompute_signals` 随时重算 |

## 3 · 工程对照

### 3.1 `schema.sql` 增量

`issue` 表(`updated_at` 列之后追加):

```sql
week_of TEXT NOT NULL DEFAULT '', version TEXT NOT NULL DEFAULT '', tool TEXT NOT NULL DEFAULT '',
kind TEXT NOT NULL DEFAULT 'business', origin TEXT NOT NULL DEFAULT 'human',
workflow TEXT NOT NULL DEFAULT '', sort_order REAL NOT NULL DEFAULT 0,
```

`project` 表追加:

```sql
standard_version TEXT NOT NULL DEFAULT '', current_version TEXT NOT NULL DEFAULT '',
chat_provider TEXT NOT NULL DEFAULT '', chat_group_id TEXT NOT NULL DEFAULT '',
chat_notify TEXT NOT NULL DEFAULT '[]',
```

新表(`schema.sql` 末尾追加,`CREATE TABLE IF NOT EXISTS` 即充分守卫):`release`、`release_issue`、`week_plan`、`issue_metric`、`workflow_credit`、`chat_outbox`——DDL 见 2.2–2.7,不重复贴。

### 3.2 `sqlite.rs` 迁移守卫调用点(存量库)

```rust
add_column_if_missing(&pool, "issue", "week_of", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "issue", "version", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "issue", "tool", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "issue", "kind", "TEXT NOT NULL DEFAULT 'business'").await?;
add_column_if_missing(&pool, "issue", "origin", "TEXT NOT NULL DEFAULT 'human'").await?;
add_column_if_missing(&pool, "issue", "workflow", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "issue", "sort_order", "REAL NOT NULL DEFAULT 0").await?;
add_column_if_missing(&pool, "project", "standard_version", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "project", "current_version", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "project", "chat_provider", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "project", "chat_group_id", "TEXT NOT NULL DEFAULT ''").await?;
add_column_if_missing(&pool, "project", "chat_notify", "TEXT NOT NULL DEFAULT '[]'").await?;
```

`workflow_credit`/`chat_outbox` 的唯一索引不需要单独的 Rust 迁移函数——`CREATE INDEX IF NOT EXISTS` 本身幂等,放在 `schema.sql` 里,新库旧库都会在 `open()` 重放整份 `schema.sql` 时补上(和现有 `idx_observation_metric_ts` 等索引待遇一致)。

`agent` 表硬删(设计期统一:与 04 篇一致):同一次迁移里 `DROP TABLE IF EXISTS agent`,新账全走 `workflow_credit`;`issue.assignee` 同一次迁移物理删除(见 2.4)。对照今天真实的退役先例——`sqlite.rs` 里 `weekly_review`/`message` 两张表的 `DROP TABLE IF EXISTS`,各带注释说明"为什么现在真删"——`agent` 表这次适用同一处理:功能(队友名单/指派)连命令一起整链删除。

### 3.3 新增仓文件解析器模块(`crates/bw-engine/src/`)

跟随 `project_file.rs`/`metrics_file.rs`/`connectors_file.rs` 已立好的模式:只读+解析,`deny_unknown_fields`,`Ok(None)` = 文件不存在(诚实无事发生),解析失败是 `Err` 且绝不写半份缓存。四个新增点:

- `issue_policy_file.rs`(新):`IssuePolicyFile { schema_version, mappings: Vec<CategoryMapping{category,tool,workflow}>, review, cadence, kanban }`,对照 2.8 样例的四个表逐段声明;`read(workspace) -> Result<Option<IssuePolicyFile>, _>` 同 `project_file::read` 骨架。
- `standard_file.rs`(新):`StandardFile { version, enabled: Vec<String>, extensions: Vec<String>, source }`,对照 `.bw/standard.toml` 样例。
- `week_plan_file.rs`(新):Markdown 不走 serde,只有一个 `extract_goal(markdown) -> Option<String>`,取「## 周目标」后第一段非空文本给 `week_plan.goal` 索引用——不解析业务活表格,活的正本已经是 `issue` 行,反解回结构化数据会制造两份正本互相打架的风险。
- `release_file.rs`(新):Markdown 表格,只解析「版本号/发版日/说明/来源」四列供 upsert `release` 表;「包含的活」列的 issue 号解析成 `release_issue` 关联行,号找不到对应 issue 时跳过并记警告,不是解析失败。
- `project_file.rs`(改现有文件,不新开):`ProjectFile` 加一个字段 `#[serde(default)] pub chat: Option<ChatConfig>`;新增 `ChatConfig { provider: String, group_id: String, #[serde(default)] notify: Vec<String> }`,同款 `deny_unknown_fields`。

### 3.4 `bw-app` 新增命令(草拟,精确签名留待各屏设计篇定)

- `SyncIssuePolicyFile` / `SyncStandardFile`:语义对齐 `SyncMetricsFile`/`sync_connectors_file_for`——文件不存在零动作、坏文件只报错不写库、幂等 upsert。
- `RecordChatOutbox { project_id, issue_id, event_type }`:发消息前先查 `uq_chat_outbox_sent_once` 覆盖的条件(是否已有一行 `status='ok'`),命中则跳过;未命中才真调项目群适配模块发送,无论成败都插一行。
- `RecordWorkflowCredit { project_id, subject_kind, subject_id, issue_id, outcome }`:在活结算(`settled_at` 首次写入)的同一事务里对每个挂在这件活上的 workflow/技能各插一行;`origin='backfill'` 的 issue **不触发**(调用方判断 `issue.origin != 'backfill'` 才调用)。
- `RecordRelease { project_id, version, note, issues }`:第 5 站发版本与第 0 站历史回填共用,靠 `origin` 参数区分。

## 4 · 边界与失败

**不做什么**:不建成员/角色表(committer 靠 `.bw/issue-policy.toml` 的 `who_can_merge = "repo_write"` 一句话表达权限,不建 `member`/`role` 实体——CONTEXT.md 已明确"buddy 里不建成员、权限、群聊、收件箱")。不给里程碑单独建表(`release` 一行就是一个里程碑)。群消息正文不进库(`chat_outbox` 只记事件+成败,不存消息文本;运作活①的群摘要是本机文件,读完即用,不进仓不进库)。不给 `metric` 定义加任何 V4 字段(`issue_metric` 只是新增关联)。

**失败如实显示**:三个新 TOML 解析器沿用 `deny_unknown_fields` + all-or-nothing,报错不写库,和 `project_file.rs`/`connectors_file.rs` 今天行为一致。`chat_outbox` 发送失败落一行 `status='failed'`(不是不记也不是假装成功),下次因"没有 `status='ok'` 的行"而重试,重试次数不设上限(连续失败的交互留给 07)。`release_file.rs` 遇到"包含的活"列里找不到对应 issue 的号,跳过这条关联并记警告、不让整份文件解析失败。老库升级(`add_column_if_missing`)全程幂等。

## 5 · 验收与读回

`<db>` 替换成 E2E 用的深链启动数据库路径,`<pid>` 替换成项目 id。

| 核验什么 | SQL | 预期 |
|---|---|---|
| 老库升级不崩 | `PRAGMA table_info(issue);` / `PRAGMA table_info(project);` | 看到 `week_of`/`version`/`tool`/`kind`/`origin`/`workflow`/`sort_order`;`standard_version`/`current_version`/`chat_provider`/`chat_group_id`/`chat_notify` |
| 回填 issue 的来源分布 | `SELECT origin, COUNT(*) FROM issue GROUP BY origin;` | 老项目接入后 `backfill` 一档非零;新项目只有 `human`/`auto`/`agent_split` |
| 本周业务活数与仓文件一致 | `SELECT COUNT(*) FROM issue WHERE project_id='<pid>' AND kind='business' AND week_of='2026-W34';` | 等于 `docs/plan/2026-W34.md`「业务活」表格行数 |
| 项目群配置库存副本与仓文件一致 | `SELECT chat_provider, chat_group_id, chat_notify FROM project WHERE id='<pid>';` | 等于 `.bw/project.toml` `[chat]` 段的 provider/group_id/notify |
| 战绩绝不记两次(数据库约束,不是代码审查)| `SELECT subject_kind, subject_id, issue_id, COUNT(*) FROM workflow_credit GROUP BY subject_kind, subject_id, issue_id HAVING COUNT(*) > 1;` | 空结果——尝试重复 `INSERT` 应被 `UNIQUE` 约束物理拒绝,不只是查出来没有 |
| 回填的活不进战绩 | `SELECT COUNT(*) FROM workflow_credit wc JOIN issue i ON wc.issue_id=i.id WHERE i.origin='backfill';` | `0` |
| 项目群通知不重发 | `SELECT project_id, issue_id, event_type, COUNT(*) FROM chat_outbox WHERE status='ok' GROUP BY project_id, issue_id, event_type HAVING COUNT(*) > 1;` | 空结果 |
| 发版记录与仓文件一致 | `SELECT version, released_at, origin FROM release WHERE project_id='<pid>' ORDER BY released_at;` | 行数与顺序等于 `docs/releases.md` 表格 |
| 指标挂活反查(验证 §2.5 画法)| `SELECT i.title FROM issue i JOIN issue_metric im ON im.issue_id=i.id WHERE im.metric_id='<metric_id>';` | 与总览该指标卡下方展示的活标题一一对应 |
| `agent` 表已被物理删除 | `SELECT name FROM sqlite_master WHERE type='table' AND name='agent';` | 空结果——老库升级后表已被 `DROP TABLE IF EXISTS agent` |

## 6 · 开放问题(≤5)

1. ~~`issue_metric` 用关联表还是 `issue.metric_ids` JSON 数组?~~ **已定:关联表(00-handshake 第 11 条)**——支持"指标反查活"的索引查询,有 `skill_stage` 先例(2.2);06/08 两篇早期草案里出现的 `issue.metric_ids` JSON 写法已改成本文这张 `issue_metric` 表。
2. ~~`agent` 表最终物理退役方式,与 `issue.assignee` 的存亡~~ **已定(设计期统一,与 04 篇一致)**:`agent` 表 `DROP TABLE IF EXISTS`,`issue.assignee` 同一次迁移物理删除,不可逆,已提请用户点头(00-handshake 第 2 条);具体理由见 2.4/3.2。
3. **`.bw/managed.toml` 的指纹算法与对账触发时机。** 本文只定文件形状(2.8),具体摘要算法、比对时机留给 [03-standard-and-backfill.md](03-standard-and-backfill.md)。
4. **`release_file.rs` 解析老项目已有 `docs/releases.md` 的鲁棒性边界。** 如果项目铺底前就有一份格式不同的发版记录,历史回填要规范化它、并行开新文件、还是兼容多种表头?属于回填流程细节,留给 03。
5. **`chat_outbox` 的失败重试策略(退避、上限、还是无限重试等到成功)。** 本文只给了数据库层"不重复发送成功通知"的约束(2.7),重试调度策略留给 [07-notify-and-chat-group.md](07-notify-and-chat-group.md)(要先定项目群适配工厂的两个函数接口)。
