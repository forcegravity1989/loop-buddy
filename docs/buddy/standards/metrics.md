# `.bw/metrics.toml` 格式规范

> **30 秒导读**:这是项目指标定义文件的唯一格式正本,给「找指标」「绑数据」Skill 和任何修改指标的 Issue 参考。现在作数。从 `docs/metrics-toml-format.md` 迁入 `docs/buddy/standards/metrics.md`,旧路径不留兼容副本。

指标定义的正本。住在项目 git 工作区根下的 `.bw/metrics.toml`,和代码一样过
PR 审核门槛;BW 只读它、同步进 SQLite 作缓存,绝不反向改写这份文件
(plan/13 D1「产品信息正本在仓,过程信息在 BW」)。

这份文档就是「找指标」Skill(后续票)的产出契约——它写出符合这个格式的文
件,BW 的 `SyncMetricsFile` 命令(`bw-app::Command::SyncMetricsFile`,解析
器在 `bw-engine::metrics_file`)负责读它、校验它、同步它。

## 放在哪

```
<项目工作区根>/.bw/metrics.toml
```

文件不存在是合法状态(还没起草指标),`SyncMetricsFile` 对不存在的文件零
动作零噪音——不写库、不报错、不发事件,和这个特性上线前的行为完全一致。

## 三层结构

一个项目恰好一个北极星,零至多条滞后指标,零至多条引领指标:

```toml
schema_version = 1

[north_star]
name = "..."
def  = "..."
collect = { kind = "...", query = "..." }

[[lagging]]
name = "..."
def  = "..."
target = "..."
collect = { kind = "...", query = "..." }

[[leading]]
name = "..."
def  = "..."
target = "..."
collect = { kind = "...", query = "..." }
```

| 层 | TOML 键 | 基数 | 含义 |
|---|---|---|---|
| 北极星 | `[north_star]` | 恰好 1 个(必填表) | 项目唯一的顶层目标 |
| 滞后指标 | `[[lagging]]` | 0..N(数组表) | 结果性指标——滞后于动作才看得出好坏 |
| 引领指标 | `[[leading]]` | 0..N(数组表) | 过程性指标——当下可控、驱动滞后指标的先行量 |

`schema_version` 目前恒为 `1`,省略时按 `0` 处理(不校验版本号,留作未来
格式演进的读取口)。

## 字段

### `north_star`(表)

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `name` | string | 是 | 北极星名称 |
| `def` | string | 否(默认空) | 北极星的精确定义——"怎么算作达成" |
| `collect` | 采集方案(见下) | 是 | 这条指标的采集方案 |

### `lagging` / `leading`(数组表,元素字段相同)

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `name` | string | 是 | 指标名称 |
| `def` | string | 否(默认空) | 指标定义 |
| `target` | string | 否(默认空) | mini-DSL 目标值,和 BW 现有 `metric.target_raw` 同一套写法:`"≥5"` `"≤24h"` `"清零"` 等 |
| `collect` | 采集方案(见下) | 是 | 这条指标的采集方案 |

### 采集方案 `collect`(内联表)

```toml
collect = { kind = "github" | "connector" | "bw" | "manual" | "script", query = "..." }
```

**每条指标(含北极星)必须带 `collect`** —— 这是「对的指标」的硬约束
(D6):没有采集方案不等于指标不对,但必须如实标注"这个数字暂时怎么来"。

| `kind` | 语义 | `query` 怎么写 | 采集器 v1 状态 |
|---|---|---|---|
| `"github"` | GitHub 查询(issue/PR/release 等) | `gh` 风格查询串,如 `"repo:{owner}/{repo} is:pr is:merged merged:>=@{7d}"` | **已接**:C7 采集器跑 `gh api search/issues` 真取 total_count,`{owner}/{repo}`、`@{Nd}` 占位符按语义展开(release 等其它面 v1 未采) |
| `"connector"` | 走已配置的 BW Connector | Connector 的名字/scope,如 `"content-analytics"` | **v1 未接,如实 Unknown**:采集器不碰,无观测、signal 保持 Unknown,绝不假绿 |
| `"bw"` | BW 自己的记账(issue 结算数、run 遥测等),不经外部系统 | 内部口径的简短描述,如 `"issue.settled_at within 7d"` | **v1 未接,如实 Unknown**:同上,留给后续票接 BW 自记账口径 |
| `"manual"` | 暂时没有采集器,靠人手填 | 允许留空字符串 `""` | 不归采集器管;值靠界面手填,戴「手填」徽记 |
| `"script"` | 项目仓里既有采集脚本(如 `derive_*.py` 机械解析真实数据源、产出 `data.json`),buddy shell-out 调它读结果 | 字段在脚本输出 JSON 里的点分路径,如 `"leading.L1"` / `"north_star.adoption_rate"`(脚本路径 + 输出文件由项目的 `script` connector 配置,buddy 跑脚本后按此路径取值) | **plan18-③ 已接**:`CollectMetrics` 跑项目的 `script` connector 配置的脚本、读输出、按 `query` 字段路径取值写 observation。脚本自身依赖(Playwright/SSO/Chrome 等)由项目侧保证可独立跑,buddy 只 shell-out 调 |

`kind` 是固定词表——文件里出现这五个之外的值,整份文件解析失败(结构性
错误,不是"未知类型就忽略"式的静默宽容)。`query` 对非 `manual` 的 kind
虽然不强制非空(解析器不做语义校验,只做结构校验),但一条采集不到值的
"github"/"connector"/"bw"/"script" 指标是内容问题,留给「找指标」/「绑数据」
Skill 处理,不是文件格式问题。

> **`kind` 的方向(Forward-correct,V1 Issue2)**:`github`/`codehub`/`bw`/
> `connector` 是 legacy inline arm,正在退休进 `script`——它们只是不同的
> 脚本 instance(脚本不同但都是脚本),不是并列 kind。新写优先 `script`
> (自动)或 `manual`(人填);legacy kind 的 inline 采集代码归采数/总览
> 窗口收进 `script`。文件格式仍列五值兼容(解析器接受全部五值),但 skill
> 和 guide 按两 kind 写。

**采集器 v1(C7)真采 `github` + `script`**(外加既有 workspace evidence 覆盖
的部分);`bw`/`connector` 两类如实留白——不采集、不写零值,看板上这些指标
的 signal 保持 Unknown 灰,徽记标「v1 未接」。「无数据 = Unknown ≠ 绿」是硬
约束,采不到就如实说采不到,绝不为了点亮而伪造观测。

## 同步语义(`SyncMetricsFile` 命令)

- **北极星**:`name`/`def` 走既有 `project.north_star`/`project.ns_def` 两
  列(和创建流手填北极星同一套字段);`collect` 落两个新列
  `north_star_collect_kind`/`north_star_collect_query`。
- **滞后/引领指标**:按 `(项目, 层级, name)` upsert——文件没有 id 概念,
  name 就是这条指标的身份。已存在则原地更新定义(`def`/`target`/
  `collect`),**保留原有 metric id**(挂在这条指标下的观测历史不受影
  响);不存在则新建一行,一周计划相关字段(`last_target`/`driver`/
  `amber`)取和界面手建同款的默认值。
- **来源标注**:同步写入的每一行 `metric.origin = 'file'`;界面
  `UpsertManualMetric` 手建的行维持 `origin = 'manual'`(这是老库/老行为
  的默认值,不是新发明的语义)。
- **幂等**:同一份文件重复同步,`metric` 表行数不变(upsert 命中已存在
  的行,不会插入重复定义)。
- **改了再同步**:文件里的 `def`/`target`/`collect` 变了,重新同步会原
  地覆盖对应行,如实反映最新正本。
- **正本里删掉的指标 → 自动停用**(取代原先的"本票不删库、什么都不做"
  ——那条推迟在 aihot 日报上真出了后果:换了新正本之后,被「找指标」
  明确判定为坏候选的旧指标仍并排显示,产品内无路可拿掉)。规则是**两边
  对称的一句话:正本里有 = 在用,正本里没有 = 停用**。
  - 正本里已经没有、且这行 `origin='file'`(当初就是从正本同步进来的)
    → 标 `archived=1` + 盖 `archived_at`,同步回执如实报「正本里已删除的
    N 条自动停用」。
  - 反过来,一条曾被停用的指标重新写回正本 → 下次同步自动恢复
    (`archived=0`),不需要人再去界面点一次。
  - **`origin='manual'` 的行永不被这条规则碰**:界面手建的指标正本里本来
    就没有,"正本删了它"这个判断对它们根本不成立,不能被沉默清场。要停用
    它们,人在运营视图的指标卡上显式点「停用」(`SetMetricArchived`)。
    界面因此也**不给正本来源的指标停用按钮**——那种按钮会被下次同步推翻,
    等于摆一个假开关;正本行的卡片上给的是一句"从该文件里删掉即停用"。
- **停用 ≠ 删除**:`metric` 行留着,`observation` 一条不删(append-only 不
  可破:硬删 metric 行要么级联抹掉真实测量历史、要么留下孤儿观测)。停用
  的效果是三退:退出界面默认视图(收进「已停用 (N) ▾」折叠区,可展开、
  可恢复)、退出健康灯上卷与自身派生(`recompute_signals` 跳过归档行,其
  `signal` 因此**冻结**在停用那一刻,界面上如实标注)、退出自动采集与
  「久没数据」统计。全仓没有物理删除单条指标的路径,这是有意的。
- **坏文件**:解析失败(结构错误、`kind` 不在词表内、缺 `collect`……)
  只报错、不写库——文件必须整份解析成功才会有任何 SQLite 写入,不存在
  "写一半"的中间态。自动停用也在同一个事务里,坏文件不会误伤任何一行。
- **一个字节不碰**:`observation` 表、`Signal` 派生链、
  `recompute_signals` 全部不涉及——这个命令只同步*定义*与*在用/停用*状态,
  不产生*值*(把这些定义变成真实观测是 C7 票「采集器」的事)。

## 完整样例

见 [`docs/examples/metrics.toml.sample`](../../examples/metrics.toml.sample)——
每个字段都带注释,可以直接复制到 `<项目工作区>/.bw/metrics.toml` 使用。
