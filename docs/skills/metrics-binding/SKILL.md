---
name: metrics-binding
description: 为 .bw/metrics.toml 里绑不上的指标找到点亮的最便宜路径——绝不伪造数据、绝不为了点亮而改指标定义。适用:标配 Issue「绑数据」,或健康灯长期 Unknown 需要接真数据源的时点
category: 标配
---

# 绑数据(metrics-binding)

标配 Issue 三件套(竞品分析 → 找指标 → **绑数据**,plan/13 D8)里的第三
件,紧跟在「找指标」(north-star-discovery)之后。找指标 Skill 已经把"对
的指标"写进了 `.bw/metrics.toml`,但很多条当下采不到——本 Skill 的活是
让能点亮的先点亮,点不亮的如实说明最便宜的下一步是什么,**不是把定义改
得"看起来能采"**。

> **buddy 契约**(产出格式、`collect_kind` 词表、通用铁律)由衔接层 system
> prompt(`build_bridge_system_prompt`)唯一持有——本 SKILL 只讲方法论(怎么
> 诊断和搭采集装置),不重复格式细节。这样换业界 skill 当 prefill 产出仍
> 对得上契约。

## 何时用 / 前置条件

- `<workspace>/.bw/metrics.toml` 已存在。**不存在就不该跑这个 Skill**——
  先补跑找指标(north-star-discovery),那边负责"指标是什么",这里只负
  责"指标怎么采"。
- 读一遍 `docs/metrics-toml-format.md`,确认对 `collect_kind` 词表、占位
  符语法(`{owner}` `{repo}` `@{Nd}`)、以及"改了再同步"的 upsert 语义(按
  `(层级, name)` 身份,改 `collect` 不影响历史观测)没有理解错。

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

> **先扫项目仓既有采集脚本(在按下表分支前)**:遍历 `governance/`、
> `derive_*.py`/`derive_*.sh`、`connectors/`、`data-sources/`、`cron/`。
> 若某条标着 `manual` 或 legacy kind 采不到的指标,其实有项目侧自动脚本
> 在机械解析真实数据源(产出 `data.json` 之类),**这是 `script`
> kind,不是 `manual`**——按下表 `script` 行处理(保持不降级,`query` 只写
> 字段在脚本输出 JSON 里的点分路径)。**项目侧自采脚本 ≠ 人手填后台**,
> 把前者降级成后者等于把自动采谎报成人填,会让看板平白多「手填」徽、且
> 掩盖"项目其实已自动化"的事实。这一步是避免误降级的关键——很多项目
> (maas 就是)早就有 `derive_leading.py` 这类脚本在机械采集,只是 buddy
> 没读它就标了 manual。

采集 kind 有两值——`script`(自动:机械解析数据源产出 JSON,`query`=字段
在 JSON 里的点分路径)和 `manual`(人手填,戴「手填」徽)。`github`/
`codehub`/`bw`/`connector` 是 legacy inline arm,正退休进 `script`——新
写优先 `script`/`manual`,不写 legacy kind。

| 现状 | 诊断 | 最便宜路径建议 |
|---|---|---|
| `kind = "github"`,`query` 为空或明显查不出预期结果 | 查询串没写对,不是数据源的问题 | 按格式的占位符语法重写 `query`(`repo:{owner}/{repo} …`、时间窗用 `@{Nd}`),优先复用北极星/其它指标里已验证过的查询模式;写完在本地用 `gh api search/issues -f q="<展开后的真实查询串>"` 真实跑一次确认有意义结果(不是本 Skill 的采集职责,只是校验查询没写错)。或改用 `script` kind + 项目侧脚本(见下)。 |
| `kind = "connector"` 或 `"bw"` | legacy inline arm,采集器 v1 未接,不是配置问题 | 如实标注"等 legacy kind 退休进 script 后接上"。**如果项目其实已经有自动采集脚本在机械解析真实数据源(如 `derive_*.py` 解析缺陷 reason 字段、产出 `data.json`),这不是 `manual`——改成 `script`,`query` 只写字段在脚本输出 JSON 里的点分路径(如 `leading.L1`),保持自动采集语义不降级**。只有"人定期从后台手抄一个数"才降级 `manual`。 |
| `kind = "script"` | 项目侧自采脚本(buddy shell-out 调它读结果) | 确认脚本能独立跑(脚本自身依赖如 Playwright/SSO 由项目侧管,buddy 只调);`query` 只写字段在脚本输出 JSON 里的点分路径(如 `leading.L1` / `north_star.adoption_rate`)。脚本路径 + 输出文件由项目的 `script` connector 配置(见 `.bw/connectors.toml`),`query` 不含脚本路径、不带 `script:`/`;`/`field:` 前缀。这是已自动采集的指标,点亮只待 buddy 到点 shell-out。 |
| `kind = "manual"` | 靠人手填,可能没有节奏 | 给出一个具体、可持续的手填节奏建议(如"每周一 5 分钟,从 XX 后台截一次数填进指标手填框"),并在 `docs/metrics-rationale.md` 记录这个节奏——手填节奏本身也是"点亮路径",不是无解。 |
| 任何 kind 但对应的外部系统压根不存在(比如没有真实竞品数据源) | 指标定义本身的问题,不是绑定问题 | 如实标注"这条指标的采集依赖尚不存在,建议改 `manual` 过渡或回头找指标 Skill 重新评估这条指标是否成立",**不代为改写定义**——只指出问题,决策权留给下一轮找指标。 |

## 执行

1. **只改 `collect`**(必要时也改 `manual` 的 `query` 说明,但 `query` 对
   `manual` 本就允许留空,不强制填)。`name`/`def`/`target` 一律不动——改
   这些是找指标 Skill 的职责边界,越界即违反上面的硬性约束。
2. **按 `(层级, name)` 原地更新**——不新建重复条目、不改名字(改名字等于
   在 BW 侧新建一条指标,历史观测会跟丢,见 `docs/metrics-toml-format.md`
   "同步语义"一节)。
3. **`kind` 保持在合法词表内**——写出词表外的值会让整份文件解析失败、
   零写入,不是"未知类型忽略"式的容错。词表见衔接层 system prompt +
   `docs/metrics-toml-format.md`。
4. **搭装置**(`script` kind 的指标):在交互式会话里和用户一起写采集脚本
   到 `.bw/scripts/<slug>.py`(buddy 自带 instance 包 codehub/github CLI,或
   项目侧 `derive_*.py` 留原位)+ 写连接器清单 `.bw/connectors.toml`(格式
   见 `docs/connectors-toml-format.md`)+ 给 metric 配 `collect_kind=
   'script'`+`collect_query=字段路径`(在 `.bw/metrics.toml`)。**PR 合入后
   buddy 感知**:`.bw/connectors.toml` → `connector` 行 upsert;cron 到点
   自动跑 script connector → 取字段 → observation → signal 点亮。agent 不
   调 buddy API——靠文件正本 + buddy 感知 sync(像 skills 分 buddy 自带 +
   项目仓自带)。
5. **落一段"绑定进度"到 `docs/metrics-rationale.md`**:每条指标此前的
   `collect` 是什么、改成了什么、为什么这是"最便宜路径"、还剩哪些指标
   仍然 Unknown(以及为什么——采集器 v1 未接 / 数据源不存在 / 待人工排
   期),让下一次跑这个 Skill 的人不用重新调查一遍现状。
6. **交付**:同 north-star-discovery——改动落在活分支上的真实提交,提 PR
   走执行器既有机制,合并永远是人手动作(铁律见衔接层 system prompt)。

## 样例:一条指标绑定前后

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

再看一条真的能自动化的例子(项目侧脚本 → `script` kind):

```toml
# 绑定前
[[leading]]
name   = "L1 自动规范问题数"
def    = "本周扫描出的自动规范问题数"
target = "≤20"
collect = { kind = "manual", query = "" }

# 绑定后——项目已有 derive_leading.py 机械解析真实数据源、产出 data.json
[[leading]]
name   = "L1 自动规范问题数"
def    = "本周扫描出的自动规范问题数"
target = "≤20"
collect = { kind = "script", query = "leading.L1" }
```

`query` 只写字段在脚本输出 JSON 里的点分路径(`leading.L1`),脚本路径 +
输出文件由 `.bw/connectors.toml` 的 script connector 配置,不在 `query` 里。

## 完成的标准(DoD)

- `.bw/metrics.toml` 改动后仍能被衔接层 system prompt 引用的格式 doc 无错
  解析,且指标条数、`name`、`def`、`target` 与改动前完全一致(只有 `collect`
  变了)。
- 每条从 `manual`/空 `query` 改成 `script` 的指标,新 `collect` 都是**真实
  可执行**的方案,不是编出来的。`script` 的 `query` 只写字段路径,不含脚
  本路径。
- 仍然 Unknown 的指标在 `docs/metrics-rationale.md` 里有诚实的现状说明和
  下一步建议,不是被沉默略过。
- 没有任何一条指标的 `name`/`def`/`target` 被本次改动动过。
- 没有在任何地方手填/伪造一个观测值来让看板临时变绿——这个 Skill 只产
  出"怎么点亮"的方案,不产出假的亮。

## 常见坑

- **顺手把"采不到"的指标改成另一条采得到的指标**:哪怕新指标看起来"差
  不多",这也是变相退化,回头交给找指标 Skill 重新评估,不在这里代劳。
- **给 legacy kind 指标编一个假 `query` 掩盖"采集器 v1 未接"**:legacy
  kind 目前无论 `query` 写什么都不会被采,写一个"看起来很专业"的 query
  只会误导后来者以为已经接通。应改用 `script` + 项目侧脚本(若项目有),
  或诚实标 `manual`。
- **把项目侧自采脚本误降级 `manual`**:项目仓里 `derive_*.py` 这类机械解
  析真实数据源的脚本是自动采集,该用 `script`,`query` 只写字段在脚本输
  出 JSON 里的点分路径,别降级 `manual`。`script` 的 `query` 必须真指向
  脚本输出 JSON 里存在的字段,别编一个不存在的路径。
- **忘记「改了再同步」的幂等语义**:`.bw/metrics.toml` 是唯一正本,改完
  只需要正常提交 + PR + merge,`SyncMetricsFile` 会在下一次同步时原地覆
  盖对应行——不需要、也不应该手动去改 SQLite。
