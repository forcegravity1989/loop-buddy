# V4 MVP 详细设计:目录与读法

> **30 秒导读**:这个目录把母文档 [`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md)(V4 的全貌与决策台账)里的每一站、每一屏、每一条规矩,落到「模块怎么分、数据怎么存、文件长什么样、命令叫什么、失败怎么显示、怎么读回核对」的粒度。给两种人看:**接着做 V4 的会话**(照着改代码)、**同事**(接 WeLink 群适配、往 `standard/` 贡献件)。**现在还作数吗**:作数,而且**代码已经照着建出来了**——`crates/bw-v4`(V4 内核)与 `crates/app-shell`(V4 新壳)都在 `main` 上,各篇第 3 节「工程对照」写的是真代码。**还没做完的部分不在这里找,只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4G 七组**。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

正文里偶尔提到的「握手清单」「交叉复核」是设计期两份已归档的问答与复核记录,在 [`../../archive/v4-prototype/`](../../archive/v4-prototype/) 下;决策本身的正本是母文档 §11 的「待拍-NN」台账。

## 目录(按建议阅读顺序)

| 编号 | 文件 | 管什么 |
|---|---|---|
| 01 | [01-architecture.md](01-architecture.md) | 新壳怎么搭:两个新 crate、一屏一模块、一个外部能力一个适配模块、命令与事件总线、文件行数守卫、旧壳什么时候能删 |
| 02 | [02-data-and-files.md](02-data-and-files.md) | 库只有四张表(`project` / `issue` / `claude_conversation` / `app_meta`)、仓里那几份文件的格式与完整样例、一样信息该住哪一层 |
| 03 | [03-standard-and-backfill.md](03-standard-and-backfill.md) | 规范铺底三步(写核心件 → 写开发手册 → 老项目历史回填)、对账、升级;回填的原料 / 产物 / 可信度 |
| 04 | [04-tools-and-workflows.md](04-tools-and-workflows.md) | 开工工具注册与探活、workflow(SOP 类技能包)怎么识别与注入、「用了几次」怎么现算 |
| 05 | [05-session-screen.md](05-session-screen.md) | 会话屏:按 worktree 分组的会话、内嵌终端、文件树 / diff / MR 卡、agent 状态回报 |
| 06 | [06-plan-screen.md](06-plan-screen.md) | 计划屏:周视角 + 六列看板一个界面、拖拽规则、周头、发版本、预览未合入 |
| 07 | [07-notify-and-chat-group.md](07-notify-and-chat-group.md) | 通知入口(待处理 / 事件流 / 「合入并完成」)+ 项目群适配工厂(发消息 / 拉历史两函数) |
| 08 | [08-overview-derivation.md](08-overview-derivation.md) | 总览每块的数据来源与推导、健康大灯的判定规则、名片编辑 |
| 09 | [09-ops-workflows.md](09-ops-workflows.md) | 三张运作活的剧本:①更新指标 + 制定本周计划 ②资产盘点(首次模式 = 老项目历史回填)③规范铺底的写开发手册 |
| 10 | [10-e2e-acceptance.md](10-e2e-acceptance.md) | 验收怎么做:检查项总表、headless 指挥器、深链、SQL 读回清单、试点两周计划 |
| 11 | [11-knowledge-base.md](11-knowledge-base.md) | 知识库屏三页签(知识 / 代码图 / 资产)的数据来源与动作;规范对账条 |
| 12 | [12-build-plan.md](12-build-plan.md) | 建法与交付记录:七刀各干了什么、代号怎么读、细节该去哪里查 |
| 13 | [13-shell-hifi-gap.md](13-shell-hifi-gap.md) | 桌面壳照高保真重排的完成记录:补上了哪些功能位、哪几处故意没照抄、还差什么 |
| 14 | [14-metrics-collection.md](14-metrics-collection.md) | **指标采集与读数**(2026-08-21 新增):一条指标的数字从哪来、存不存、存哪;`.bw/metrics.toml` 的新格式;采集脚本的输入输出;读数文件。**是设计,还没落地**;指标这块与其它篇冲突时以本篇为准 |

还在用的两篇预研在 [`../research/`](../research/):[orca](../research/orca.md)(终端内嵌与右侧栏,源码注释里引它)、[chat-group](../research/chat-group.md)(项目群接口,WeLink 还没实现,是给同事的对接底稿)。其余预研结论已进设计与代码,归档在 [`../../archive/v4-prototype/research/`](../../archive/v4-prototype/research/)。

## 每篇的写法(改到哪篇就照这个骨架改)

```
# NN · 标题
> 30 秒导读:这篇是什么 / 给谁看 / 现在作数吗
## 0 · 这篇管什么、不管什么(对应母文档哪几节、待拍-NN;与其它篇的边界)
## 1 · 用户看到什么、做什么(旅程视角,人话)
## 2 · 设计(结构、流程、规则;文字 + 必要的文本图 / 表)
## 3 · 工程对照(crate / 模块 / 命令 / 事件 / 表 / 文件;Rust 伪码只准出现在这一节)
## 4 · 边界与失败(不做什么;失败如何如实显示,绝不假装)
## 5 · 验收与读回(深链启动 + SQL 读回;每条能复算)
## 6 · 开放问题(≤5,给用户拍)
```

写作规矩(来自仓根 `CLAUDE.md`「写作纪律」):正文人话;实现术语只进第 3 节;代号第一次出现要带一句解释;**不新开代号系列**(要编号就写「第 N 步」「第 N 块」);引用决策写「待拍-NN」;不编数字,样例数字标来源或标「演示」;未建的功能写「未建」。

**发现文档与代码不符**:整块删了按实况重写,不在旁边追加一段「原来是 X,现在是 Y」的补丁——两层叙述并存,读的人下次还得再判一次哪层作数。
