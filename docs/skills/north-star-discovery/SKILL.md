---
name: north-star-discovery
slug: north-star-discovery
description: 结合项目意图/类型与竞品分析报告,推导北极星+滞后+引领三层指标;每条必附采集方案;北极星绝不为「采得到」退化成工程虚荣指标。
category: 标配
source: 官方(标配 Issue 三件套第二件,plan/13 D8)
---

# 找指标(north-star-discovery)

标配 Issue 三件套(竞品分析 → **找指标** → 绑数据,plan/13 D8)里的第二件。
接手时项目已经有(或应当先补齐)一份 `docs/competitive-analysis.md`——竞品分
析是找指标的输入,不是并行活。

## 何时用 / 前置条件

- 项目已建好,工作区是一个真实 git 仓(`workspace_path` 非空)。
- 若工作区根下存在 `docs/competitive-analysis.md`,**必读**:对标名单、各家
  北极星猜测、差异定位是起草本项目北极星的第一手依据,不凭空想象。不存在
  就如实在 `docs/metrics-rationale.md` 里写一句「本轮无竞品分析报告输入,
  北极星推导仅基于项目自身意图」,不假装读过。
- 读项目自身意图:项目的 `desc`(项目类型/一句话说明)、`benchmark`(对标
  对象)、`opportunity`(差异化机会)——这些字段就是"为什么做这个项目"的
  真实来源,不是 Skill 自己现编。

## 硬性约束(先对后亮,plan/13 D6 · 白纸黑字)

> **每条指标(含北极星)必须附一个采集方案** —— `collect.kind` 只能是
> `"github"` / `"connector"` / `"bw"` / `"manual"` 四选一,这是「对的指标」
> 的硬约束:没有采集方案不等于指标不对,但必须如实标注"这个数字暂时怎么
> 来"。
>
> **北极星绝不为了"当前采得到"而退化成 commit 数、PR 数、issue 数这类工程
> 虚荣指标。** 工程活动量是引领指标的候选(甚至只是引领指标的引领指标),
> 永远不够格坐在北极星的位置——北极星必须是这个项目对**用户/世界**产生
> 的真实价值(留存、复购、解决的问题、创作产出……视项目类型而定),即使
> 眼下这条指标只能标 `manual` 甚至暂时想不出精确采集方案,也不能因此把它
> 换成一条方便采集但答非所问的指标。这条约束优先于"看起来能立刻点亮"。
>
> 采集器 v1(见 `docs/metrics-toml-format.md`)只真采 `github`;`connector`/
> `bw` 两类如实 Unknown,不是本 Skill 的责任范围——那是"绑数据" Skill
> (metrics-binding)的活。本 Skill 只管指标"对不对",不为了让灯亮而在这一
> 步就把定义写歪。

## 工作步骤

1. **读输入**:项目意图(`desc`/`benchmark`/`opportunity`)+
   `docs/competitive-analysis.md`(若存在)+ 项目现有 `.bw/metrics.toml`
   (若已存在,说明是修订而非首次起草,原地改写、不无理由推倒重来)。
2. **起草北极星**:恰好一条,对应"这个项目存在的理由"。写清 `name` +
   `def`(精确到"怎么算作达成")。按上面的硬性约束,拒绝虚荣指标候选。
3. **起草滞后指标**(0..N,建议 2-4 条):结果性、滞后于动作才看得出好坏
   的指标,通常是北极星的分解或先行验证。每条给 `target`(mini-DSL,同
   `metric.target_raw` 写法:`"≥5"` `"≤24h"` `"清零"` 等)。
4. **起草引领指标**(0..N,建议 2-4 条):过程性、当下可控、驱动滞后指标
   的先行量——研发节奏(合并 PR 数)、交付节奏(结算 Issue 数)等工程活动
   量放在这一层是合适的,不是北极星层。
5. **给每条指标定 `collect`**:诚实选 `kind`。能查 GitHub 的写
   `"github"` + 真实查询串(占位符见下);走 BW 已配置 Connector 的写
   `"connector"` + connector 名字;BW 自己记账(issue 结算数、run 遥测)写
   `"bw"` + 简短口径描述;暂时没有埋点/连接器就诚实写 `"manual"` +
   `query = ""`——**不为了显得"采得到"而编一个查不出真实数字的 query**。
6. **落文件**:把三层指标写进工作区 `<workspace>/.bw/metrics.toml`,严格
   按 `docs/metrics-toml-format.md` 的结构(见下方"输出契约"与完整样例)。
7. **写推导文档**:`<workspace>/docs/metrics-rationale.md`——人读的推导过
   程,至少包含:输入摘要(竞品分析要点/项目意图)、北极星为什么是这条
   (以及被拒绝的虚荣指标候选是什么、为什么被拒)、滞后/引领指标的因果链
   (为什么这几条引领指标真的驱动这几条滞后指标)、每条指标当前采集方案
   的诚实评估(哪些现在采得到、哪些暂时只能 manual)。
8. **交付**:改动落在活分支(`bw/issue-<n>`,由 `RunIssue` 在跑本活前已
   经切好),不需要 Skill 自己操作 git branch/PR——正常提交改动即可,提
   PR 走执行器既有机制(跑完后 `RunIssue` 自动尝试 `open_pr`);**执行器只
   会提 PR,合并永远是人手动作**。

## 输出契约

### `.bw/metrics.toml`

结构、字段、`collect.kind` 四值封闭枚举、`query` 占位符语法,一律遵照
[`docs/metrics-toml-format.md`](../../metrics-toml-format.md)——不要凭记忆
另起一套格式,那份文档就是 BW 的 `SyncMetricsFile` / `bw_engine::metrics_file`
解析器的唯一契约,格式错一个字段整份文件解析失败、零写入。

`github` 查询串支持的占位符(照抄即可,解析器不做语义校验,但采集器 v1
只认这两种):

| 占位符 | 展开为 |
|---|---|
| `{owner}` / `{repo}` | 项目 GitHub 远程仓的 owner/repo |
| `@{Nd}` | 参照今天往前 N 天的 ISO 日期(如 `@{7d}` → 7 天前) |

### `docs/metrics-rationale.md`

无固定 schema(人读文档,不被机器解析),但必须真实反映第 7 步列出的四块
内容,禁止空话套话——每个结论都要能指回一个真实依据(竞品分析里的哪一
条、项目 `desc`/`benchmark` 里的哪句话)。

## 完整样例(可直接改写后使用)

以下是一份完整、结构合法、能通过 `bw_engine::metrics_file` 解析的样例——
虚构项目「个人知识库同步工具」,示范"北极星不退化成虚荣指标"的正确写法
(北极星是用户价值,引领指标才是工程活动量):

```toml
schema_version = 1

[north_star]
name = "周活跃同步用户数"
def  = "过去 7 天内至少成功完成一次跨设备同步的注册用户数——不是安装量,是真的在用"
collect = { kind = "connector", query = "usage-analytics" }

[[lagging]]
name   = "首月留存率"
def    = "注册后 30 天内仍有同步行为的用户占比"
target = "≥30%"
collect = { kind = "connector", query = "usage-analytics" }

[[lagging]]
name   = "同步失败率"
def    = "过去 7 天内同步请求里以失败结束的比例——直接影响留存的产品质量信号"
target = "≤2%"
collect = { kind = "manual", query = "" }

[[leading]]
name   = "每周合并 PR 数"
def    = "过去 7 天内 merge 进 main 的 PR 数——研发产出的先行信号,工程活动量放这里合适"
target = "≥5"
collect = { kind = "github", query = "repo:{owner}/{repo} is:pr is:merged merged:>=@{7d}" }

[[leading]]
name   = "队友结算 Issue 数"
def    = "过去 7 天内被人 merge 关闭(settle)的 Issue 数——交付节奏的先行信号"
target = "≥3"
collect = { kind = "bw", query = "issue.settled_at within 7d" }
```

注意北极星 `周活跃同步用户数` 和引领指标 `每周合并 PR 数` 的位置——后者是
真实存在的工程活动量,但它只配当引领指标(驱动北极星的先行量),换到北极
星位置就是本 Skill 明令禁止的退化。

## 完成的标准(DoD)

- `<workspace>/.bw/metrics.toml` 存在、结构合法(能被
  `bw_engine::metrics_file::read` 无错解析)。
- 恰好一条 `[north_star]`,不是虚荣指标(能对照 `docs/competitive-analysis.md`
  或项目 `desc`/`benchmark` 说清楚"为什么是这条")。
- 每条指标(含北极星)`collect.kind` 落在四值枚举内,`query` 不是为了"看
  起来能采"而瞎写的假查询。
- `<workspace>/docs/metrics-rationale.md` 存在,四块内容都有真实依据支撑。
- 改动是活分支上的真实提交,不是口头描述。

## 常见坑

- **把工程活动量塞进北极星**:commit 数/PR 数/issue 数天然容易采集,诱惑
  最大,也是本 Skill 存在的主要理由——每次起草北极星前重读一遍上面的硬
  性约束段落。
- **为了让 `collect.kind` 显得"高级"而选错**:能诚实标 `manual` 就标
  `manual`,不要为了显得体面而选 `connector`/`bw` 却没有真实对应的接入。
- **`docs/competitive-analysis.md` 存在却没读**:那份报告存在的唯一理由
  就是喂给这一步,跳过它等于自己现编"知道对标谁"。
- **改写已存在的 `.bw/metrics.toml` 时误删历史指标的 id 语义**:文件没有
  id 概念,`(层级, name)` 就是身份——改名字等于新建一条,历史观测会跟丢;
  只想改定义就保持 `name` 不变。
