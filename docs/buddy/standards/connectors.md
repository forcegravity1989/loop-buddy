# `.bw/connectors.toml` 格式规范

> **30 秒导读**:这是脚本连接器定义文件的唯一格式正本,给「绑数据」Skill 和任何搭采集装置的 Issue 参考。现在作数。

脚本连接器(script connector)的正本。住在项目 git 工作区根下的
`.bw/connectors.toml`,和 `.bw/metrics.toml` 一样过 PR 审核门槛;BW 只读
它、同步进 SQLite 作缓存,绝不反向改写这份文件。

绑数据 Skill(metrics-binding)引导用户在交互式 claude 会话里搭采集装置
时,产出符合这个格式的文件;BW 的 `sync_connectors_file_for`(bw-app)负责
读它、同步它。

## 放在哪

```
<项目工作区根>/.bw/connectors.toml
```

文件不存在是合法状态(还没搭装置),`sync_connectors_file_for` 对不存在的文
件零动作零噪音——不写库、不报错、不发事件,和 `metrics.toml` 完全一致。

## 结构

```toml
[[connector]]
name = "..."
kind = "script"
script = "..."
command = "..."
output = "..."
```

`[[connector]]` 是 TOML 数组表——零至多条,每条定义一个 script connector。

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `name` | string | 是 | 连接器名称,同一项目内唯一(upsert 按 `(project_id, name)` 身份) |
| `kind` | string | 是 | 固定词表,目前只接受 `"script"`。其他值会让整份文件解析失败 |
| `script` | string | 是 | 采集脚本的路径,相对工作区根(如 `scripts/derive_leading.py` 或 `governance/derive_leading.py`)。不能用绝对路径 |
| `command` | string | 否(默认空) | 跑脚本的命令(`python` / `ts-node` / `node` …)。空在采集时默认 `python`(Windows 上找不到 `python` 会自动退而求其次试 `py` 启动器) |
| `output` | string | **事实上必填**(默认空) | 脚本输出文件路径(相对工作区根)。buddy 跑完脚本后**只读这个文件**取 JSON,再按各 `script` 指标的 `collect_query` 字段路径取值 |

> **⚠ `output` 留空 = 永远采不到数(2026-08-06 真实事故)**:`collect_project_metrics`
> 跑完脚本**只看 `output` 指向的文件,完全丢弃脚本的 stdout**(哪怕脚本 print
> 出了正确的 JSON)。真实发生过:agent 写的脚本只 `print()` 到 stdout、
> `connectors.toml` 里 `output` 留空,PR 合入 + sync 全部"成功",但指标一直
> Unknown——因为脚本必须真的把结果**写进** `output` 指向的文件,不是打印到
> 终端。绑数据时务必让脚本落一次盘再收工。

## `kind` 的词表

| `kind` | 语义 | 采集器 v1 状态 |
|---|---|---|
| `"script"` | 项目仓里的采集脚本(如 `derive_*.py` 机械解析真实数据源、产出 `data.json`),buddy shell-out 调它读结果 | **已接**:plan18-③ + Phase 3 `sync_connectors_file_for` 正规化。cron 到点自动跑 script connector → 读输出 → 按 `collect_query` 字段路径取值写 observation |

`kind` 是固定词表——文件里出现 `"script"` 之外的值,整份文件解析失败(结
构性错误,不是"未知类型就忽略"式的静默宽容)。`github`/`codehub`/`bw`/
`connector` 是 `collect_kind` 的 legacy inline arm(见
`docs/buddy/standards/metrics.md`),不是 `.bw/connectors.toml` 的 `kind` 词
表——它们正在退休进 `script`(采数/总览窗口收尾)。

## 同步语义(`sync_connectors_file_for`)

- **merge 后自动**:`MergeIssuePr`(bw-app)merge PR 后,工作区收拢回默认
  分支,自动调 `sync_connectors_file_for`(与 `sync_metrics_file_for` 并
  列)。
- **upsert-by-name**:按 `(project_id, name)` 身份 upsert——文件没有 id
  概念,name 就是这条连接器的身份。已存在则原地更新 `config`(JSON
  `{script, command, output}`);不存在则新建一行 `connector`(`kind =
  'script'`、`scope = ''`、`status = 'disconnected'`)。
- **幂等**:同一份文件重复同步,`connector` 表行数不变(upsert 命中已存在
  的行,不会插入重复定义)。
- **正本里删掉的连接器**:本 phase **不删库**——SQLite 缓存里的行原样保
  留。是否在界面上提示"这条已经不在正本里了"是后续 UI 票的事。
- **坏文件**:解析失败(结构错误、`kind` 不在词表内、缺 `name`/`script`…)
  只报错、不写库——文件必须整份解析成功才会有任何 SQLite 写入,不存在
  "写一半"的中间态。
- **一个字节不碰**:`observation` 表、`Signal` 派生链、
  `recompute_signals` 全部不涉及——这个命令只同步*连接器定义*,不产生
  *值*(把这些定义变成真实观测是 `CollectMetrics` cron 的事)。

## 与 `.bw/metrics.toml` 的关系

- `.bw/metrics.toml` 的指标定义里,`collect.kind = "script"` 的指标的
  `collect.query` 是字段在脚本输出 JSON 里的点分路径(如 `leading.L1`)。
- `.bw/connectors.toml` 的 script connector 定义了脚本路径 + 输出文件——
  buddy 跑脚本、读输出、按 `collect_query` 取值。
- 两个文件各自独立同步,都过 PR 审核,merge 后自动 sync。script connector
  的 `name` 和 metric 的 `collect_query` 没有直接引用关系——buddy 跑所有
  项目的 script connector,把输出 JSON 缓存,再逐条 metric 按 `query` 取值。

## 完整样例

```toml
# .bw/connectors.toml

[[connector]]
name = "leading-indicators"
kind = "script"
script = "scripts/derive_leading.py"
command = "python"
output = "data.json"

[[connector]]
name = "north-star-adoption"
kind = "script"
script = "governance/derive_ns.py"
# command 省略 → 采集时默认 python
output = "ns_data.json"
```
