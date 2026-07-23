# `.bw/metrics.toml` 格式说明(plan/13 D5+D6 · C6)

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
def = "..."
target = "..."
collect = { kind = "...", query = "..." }

[[leading]]
name = "..."
def = "..."
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
collect = { kind = "github" | "connector" | "bw" | "manual", query = "..." }
```

**每条指标(含北极星)必须带 `collect`** —— 这是「对的指标」的硬约束
(D6):没有采集方案不等于指标不对,但必须如实标注"这个数字暂时怎么来"。

| `kind` | 语义 | `query` 怎么写 | 采集器 v1 状态 |
|---|---|---|---|
| `"github"` | GitHub 查询(issue/PR/release 等) | `gh` 风格查询串,如 `"repo:{owner}/{repo} is:pr is:merged merged:>=@{7d}"` | **已接**:C7 采集器跑 `gh api search/issues` 真取 total_count,`{owner}/{repo}`、`@{Nd}` 占位符按语义展开(release 等其它面 v1 未采) |
| `"connector"` | 走已配置的 BW Connector | Connector 的名字/scope,如 `"content-analytics"` | **v1 未接,如实 Unknown**:采集器不碰,无观测、signal 保持 Unknown,绝不假绿 |
| `"bw"` | BW 自己的记账(issue 结算数、run 遥测等),不经外部系统 | 内部口径的简短描述,如 `"issue.settled_at within 7d"` | **v1 未接,如实 Unknown**:同上,留给后续票接 BW 自记账口径 |
| `"manual"` | 暂时没有采集器,靠人手填 | 允许留空字符串 `""` | 不归采集器管;值靠界面手填,戴「手填」徽记 |

`kind` 是固定词表——文件里出现这四个之外的值,整份文件解析失败(结构性
错误,不是"未知类型就忽略"式的静默宽容)。`query` 对非 `manual` 的 kind
虽然不强制非空(解析器不做语义校验,只做结构校验),但一条采集不到值的
"github"/"connector"/"bw" 指标是内容问题,留给「找指标」/「绑数据」
Skill 处理,不是文件格式问题。

**采集器 v1(C7)只真采 `github`**(外加既有 workspace evidence 覆盖的部分);
`bw`/`connector` 两类如实留白——不采集、不写零值,看板上这些指标的 signal
保持 Unknown 灰,徽记标「v1 未接」。「无数据 = Unknown ≠ 绿」是硬约束,采不到
就如实说采不到,绝不为了点亮而伪造观测。

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
- **正本里删掉的指标**:本票**不删库**——SQLite 缓存里的行原样保留,
  也不额外打"已移除"标记(那是一个新状态位,超出本票"指标定义 + 采集
  方案落库"的范围)。理由:观测/信号链路完全不碰,数据没丢;要不要在
  界面上提示"这条已经不在正本里了"是后续 UI/采集器票的事,不在这里静
  默改写。
- **坏文件**:解析失败(结构错误、`kind` 不在词表内、缺 `collect`……)
  只报错、不写库——文件必须整份解析成功才会有任何 SQLite 写入,不存在
  "写一半"的中间态。
- **一个字节不碰**:`observation` 表、`Signal` 派生链、
  `recompute_signals` 全部不涉及——这个命令只同步*定义*,不产生*值*(把
  这些定义变成真实观测是 C7 票「采集器」的事)。

## 完整样例

见 [`docs/examples/metrics.toml.sample`](examples/metrics.toml.sample)——
每个字段都带注释,可以直接复制到 `<项目工作区>/.bw/metrics.toml` 使用。
