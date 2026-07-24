---
name: metrics-binding
slug: metrics-binding
description: 读现有 .bw/metrics.toml,为每条绑不上的指标给出点亮它的最便宜路径;绝不伪造数据、绝不为了点亮而改指标定义。
category: 标配
source: 官方(标配 Issue 三件套第三件,plan/13 D8)
---

# 绑数据(metrics-binding)

标配 Issue 三件套(竞品分析 → 找指标 → **绑数据**,plan/13 D8)里的第三
件,紧跟在「找指标」(north-star-discovery)之后。找指标 Skill 已经把"对
的指标"写进了 `.bw/metrics.toml`,但很多条当下采不到——本 Skill 的活是
让能点亮的先点亮,点不亮的如实说明最便宜的下一步是什么,**不是把定义改
得"看起来能采"**。

## 何时用 / 前置条件

- `<workspace>/.bw/metrics.toml` 已存在。**不存在就不该跑这个 Skill**——
  先补跑找指标(north-star-discovery),那边负责"指标是什么",这里只负
  责"指标怎么采"。
- 读一遍 `docs/metrics-toml-format.md`,确认对四值封闭枚举
  (`github`/`connector`/`bw`/`manual`)、占位符语法(`{owner}` `{repo}`
  `@{Nd}`)、以及"改了再同步"的 upsert 语义(按 `(层级, name)` 身份,改
  `collect` 不影响历史观测)没有理解错。

## 硬性约束(白纸黑字)

> **绝不伪造数据。** 本 Skill 的产出是"怎么点亮"的方案和更新后的
> `collect` 字段,**不是**手填一个假的观测值去骗过看板。看板上的每一盏
> 灯必须来自真实观测——Unknown 就是 Unknown,不因为本 Skill 跑过一遍就
> 假装变绿。
>
> **绝不为了点亮而改指标定义。** 如果一条指标真实的 `def`/`target` 采不
> 到,正确动作是找到能采到*那条定义*的路径(换数据源、换查询、换成人工
> 记账节奏),而不是把 `name`/`def` 悄悄改写成另一条方便采集但答非所问的
> 指标——那是伪装成"绑数据"的"找指标"退化,违反 north-star-discovery
> 已经守住的"先对后亮"约束。指标定义本身不对,回头交给找指标 Skill 改,
> 不在本 Skill 里顺手篡改。

## 诊断步骤(按 `collect.kind` 分支)

对 `.bw/metrics.toml` 里每一条指标(含北极星),按当前 `collect.kind` 分
支给"点亮的最便宜路径":

| 现状 | 诊断 | 最便宜路径建议 |
|---|---|---|
| `kind = "github"`,`query` 为空或明显查不出预期结果 | 查询串没写对,不是数据源的问题 | 按 `docs/metrics-toml-format.md` 的占位符语法重写 `query`(`repo:{owner}/{repo} …`、时间窗用 `@{Nd}`),优先复用北极星/其它指标里已验证过的查询模式;写完在本地用 `gh api search/issues -f q="<展开后的真实查询串>"` 真实跑一次确认有意义结果(不是本 Skill 的采集职责,只是校验查询没写错)。 |
| `kind = "connector"` | **采集器 v1 未接**(`docs/metrics-toml-format.md` 明确标注),不是配置问题 | 如实标注"等 BW Connector 采集器接上这一 kind";如果项目其实已经手动接了某个真实数据源(如内部看板 API),评估把 `kind` 诚实改成 `manual`(定期人工从那个数据源抄一次数)是否比等待"未来才有的 connector 采集器"更便宜——**这是"换成真能跑的采集方式",不是伪造**。 |
| `kind = "bw"` | **采集器 v1 未接**,同上 | 如实标注"等 BW 自记账采集器接上这一口径";BW 侧已有的等价真实数据(如 issue 结算数、run 遥测)如果界面/sqlite 已经能查到,可以在 `docs/metrics-rationale.md` 里补一句"当前可用 `sqlite3 <db> \"SELECT …\"` 手动核对,自动采集器接上前先靠这条路径人工核实",但**不要**在 `.bw/metrics.toml` 里编一个假的 collect 方案掩盖"暂未自动化"的事实。 |
| `kind = "manual"` | 靠人手填,可能没有节奏 | 给出一个具体、可持续的手填节奏建议(如"每周一 5 分钟,从 XX 后台截一次数填进指标手填框"),并在 `docs/metrics-rationale.md` 记录这个节奏——手填节奏本身也是"点亮路径",不是无解。 |
| `github`/`connector`/`bw` 但对应的外部系统压根不存在(比如没有真实竞品数据源) | 指标定义本身的问题,不是绑定问题 | 如实标注"这条指标的采集依赖尚不存在,建议改 `manual` 过渡或回头找指标 Skill 重新评估这条指标是否成立",**不代为改写定义**——只指出问题,决策权留给下一轮找指标。 |

## 执行

1. **只改 `collect`**(必要时也改 `manual` 的 `query` 说明,但 `query` 对
   `manual` 本就允许留空,不强制填)。`name`/`def`/`target` 一律不动——改
   这些是找指标 Skill 的职责边界,越界即违反上面的硬性约束。
2. **按 `(层级, name)` 原地更新**——不新建重复条目、不改名字(改名字等于
   在 BW 侧新建一条指标,历史观测会跟丢,见 `docs/metrics-toml-format.md`
   "同步语义"一节)。
3. **`kind` 保持在四值封闭枚举内**——写出第五个值会让 `SyncMetricsFile`
   /`bw_engine::metrics_file` 整份文件解析失败、零写入,不是"未知类型忽
   略"式的容错。
4. **落一段"绑定进度"到 `docs/metrics-rationale.md`**:每条指标此前的
   `collect` 是什么、改成了什么、为什么这是"最便宜路径"、还剩哪些指标
   仍然 Unknown(以及为什么——采集器 v1 未接 / 数据源不存在 / 待人工排
   期),让下一次跑这个 Skill 的人不用重新调查一遍现状。
5. **交付**:同 north-star-discovery——改动落在活分支上的真实提交,提 PR
   走执行器既有机制,合并永远是人手动作。

## 输出契约

同一份 `<workspace>/.bw/metrics.toml`(改的是既有文件,不是新建),格式
契约仍是 [`docs/metrics-toml-format.md`](../../metrics-toml-format.md)。

### 样例:一条指标绑定前后

绑定前(找指标 Skill 留下的诚实占位——暂时没有埋点):

```toml
[[lagging]]
name   = "首月留存率"
def    = "注册后 30 天内仍至少发布一次的用户占比"
target = "≥35%"
collect = { kind = "manual", query = "" }
```

绑定后(评估过项目已经真实接了 GitHub Discussions 计数当作留存代理不合
适,维持诚实的人工路径,但给出具体节奏而不是空占位):

```toml
[[lagging]]
name   = "首月留存率"
def    = "注册后 30 天内仍至少发布一次的用户占比"
target = "≥35%"
collect = { kind = "manual", query = "每周一从用户后台导出注册满 30 天的活跃用户比例,手填" }
```

`name`/`def`/`target` 一字未改,`collect.kind` 也维持 `manual`(诚实评估
下确实没有更便宜的自动化路径)——但从"空占位"变成"有节奏、有出处"的
真实手填计划,这才是本 Skill 定义下的"点亮"。

再看一条真的能自动化的例子:

```toml
# 绑定前
[[leading]]
name   = "每周合并 PR 数"
def    = "过去 7 天内 merge 进 main 的 PR 数"
target = "≥5"
collect = { kind = "manual", query = "" }

# 绑定后——项目已经开了 GitHub 仓,这条其实有现成的 kind="github" 路径
[[leading]]
name   = "每周合并 PR 数"
def    = "过去 7 天内 merge 进 main 的 PR 数"
target = "≥5"
collect = { kind = "github", query = "repo:{owner}/{repo} is:pr is:merged merged:>=@{7d}" }
```

## 完成的标准(DoD)

- `.bw/metrics.toml` 改动后仍能被 `bw_engine::metrics_file::read` 无错解
  析,且指标条数、`name`、`def`、`target` 与改动前完全一致(只有 `collect`
  变了)。
- 每条从 `manual`/空 `query` 改成 `github`/`connector`/`bw` 的指标,新
  `collect` 都是**真实可执行**的方案,不是编出来的。
- 仍然 Unknown 的指标在 `docs/metrics-rationale.md` 里有诚实的现状说明和
  下一步建议,不是被沉默略过。
- 没有任何一条指标的 `name`/`def`/`target` 被本次改动动过。
- 没有在任何地方手填/伪造一个观测值来让看板临时变绿——这个 Skill 只产
  出"怎么点亮"的方案,不产出假的亮。

## 常见坑

- **顺手把"采不到"的指标改成另一条采得到的指标**:哪怕新指标看起来"差
  不多",这也是变相退化,回头交给找指标 Skill 重新评估,不在这里代劳。
- **给 `connector`/`bw` 指标编一个假 `query` 掩盖"采集器 v1 未接"**:这
  两个 kind 目前无论 `query` 写什么都不会被采,写一个"看起来很专业"的
  query 只会误导后来者以为已经接通。
- **忘记「改了再同步」的幂等语义**:`.bw/metrics.toml` 是唯一正本,改完
  只需要正常提交 + PR + merge,`SyncMetricsFile` 会在下一次同步时原地覆
  盖对应行——不需要、也不应该手动去改 SQLite。
