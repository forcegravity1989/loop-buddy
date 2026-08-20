# V4 MVP 详细设计(第一版,2026-08-19 夜):目录、读法、写法

> **30 秒导读**:这个目录是 V4 MVP 的**详细设计稿**——把 [`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md)(全貌草案,作数的设计事实源,已折入 2026-08-19 内部专家评审的五条反馈)与 [`../standard-module-draft.md`](../standard-module-draft.md)(规范铺底模块)里的每一站、每一屏、每一条规矩,落到「模块怎么分、数据怎么存、文件长什么样、命令叫什么、失败怎么显示、E2E 怎么读回」的粒度。给三种人看:**用户(复核设计)**、**下一步写代码的会话(照着做)**、**同事(接 WeLink 群适配、往 `standard/` 贡献件)**。**状态:代码已按这套设计开工并跑完三刀(A 骨架+主环 / B 运作活+会话屏 / C 回填+项目群+知识库),全在 PR #105、未合;各篇第 3 节「工程对照」已按实况整块重写,标着「X 刀落地后重写」的段落是实况,没标的仍是设计意图。没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A/V4B/V4C 三组**;**2026-08-20 第五轮:用户回握手 17 条已折入各篇**(拖拽统一、默认 workflow 改 mattpocock-skills、运作活②改名「资产盘点」并把老项目历史回填并入它的首次模式、库是本机的、试点用 buddy 自己的仓等,见 [00-handshake.md](00-handshake.md) 顶部「用户回复与处置」表)——用户定的顺序是先高保真 → 反馈 → 再详细设计 → 开发,现在在第三步。**2026-08-20 第七轮:用户逐条盘完数据,库只剩四张表(`project`/`issue`/`claude_conversation`/`app_meta`),各篇受影响小节已整块重写**(母文档 §6)。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

## 目录(按建议阅读顺序)

| 编号 | 文件 | 管什么 | 依赖的预研 | 状态 |
|---|---|---|---|---|
| 01 | [01-architecture.md](01-architecture.md) | 新壳怎么搭:crate / 目录、一屏一模块、一个外部能力一个适配模块、命令与事件总线增量、文件行数守卫、与旧内核的接缝 | [orca](../research/orca.md)、[deepseek-harness](../research/deepseek-harness.md) | 稿 |
| 02 | [02-data-and-files.md](02-data-and-files.md) | 库只剩四张表(`project`/`issue`/`claude_conversation`/`app_meta`,`issue` 8 个缓存列)、仓文件格式(`.bw/project.toml` 含 `[chat]`、`.bw/issue-policy.toml` 三列映射、`.bw/standard.toml`、`docs/plan/YYYY-Www.md`、`docs/releases.md`)、信息住哪一张盘点表 | — | 稿 |
| 03 | [03-standard-and-backfill.md](03-standard-and-backfill.md) | 运作活③「规范铺底」三步(写核心件 → 合并调整 → 老项目历史回填)的流程、对账、升级;回填的原料 / 产物 / 可信度 | [legacy-backfill](../research/legacy-backfill.md) | 稿 |
| 04 | [04-tools-and-workflows.md](04-tools-and-workflows.md) | 开工工具注册(终端类 / 本机网页内嵌类)、workflow(SOP 类技能包)怎么识别 / 注入,预置包随 buddy 出厂、铺底复制进 `.claude/skills/`,用了几次现算(不建战绩表) | [workflow-skill-packages](../research/workflow-skill-packages.md)、deepseek-harness | 稿 |
| 05 | [05-session-screen.md](05-session-screen.md) | 会话屏:按 worktree 分组的会话、嵌入终端(PTY、复制)、文件树 / diff / MR 卡、agent 状态回报、Open Design 内嵌、Cursor 接法 | orca | 稿 |
| 06 | [06-plan-screen.md](06-plan-screen.md) | 计划屏:周视角 + 六列看板一个界面、拖拽统一(第五轮:所有列都能拖,排期直接生效、状态动作弹确认框)、周头、发版、预览未合入 | [kanban-drag-dioxus](../research/kanban-drag-dioxus.md) | 稿 |
| 07 | [07-notify-and-chat-group.md](07-notify-and-chat-group.md) | 通知入口(待处理 / 事件流 / 行内动作 / 「合入并完成」一键)+ 项目群适配工厂(发消息 / 拉历史两函数;WeLink 内部由同事实现、外部待定;通知同步与运作活①群摘要) | [chat-group](../research/chat-group.md) | 稿 |
| 08 | [08-overview-derivation.md](08-overview-derivation.md) | 总览八块每块的数据来源与推导、health 规则、历史运作(回填)块、名片(含项目群)编辑走轻量活 + MR | legacy-backfill | 稿 |
| 09 | [09-ops-workflows.md](09-ops-workflows.md) | 三张运作活的 workflow 剧本(SKILL.md 大纲、注入清单、与人的对话节点、产出、停在评审中):①更新指标 + 制定本周计划 ②资产盘点(第五轮改名,含首次模式 = 老项目历史回填,微重构改为只出建议活)③规范铺底 | workflow-skill-packages、legacy-backfill | 稿 |
| 10 | [10-e2e-acceptance.md](10-e2e-acceptance.md) | 验收怎么做:E2E 指挥器(headless 走完一周)、深链、SQL 读回清单、试点两周计划、老项目与项目群两条新验收 | 全部 | 稿 |
| 11 | [11-knowledge-base.md](11-knowledge-base.md) | 知识库屏三页签(知识 / 代码图 / 资产)的数据来源与动作;规范对账条 | codegraph | 稿 |
| — | [00-handshake.md](00-handshake.md) | **给用户的二次握手清单**:我替用户按默认答案做下去的 17 条判断,请逐条「默认 / 改」 | — | 待用户回 |
| — | [REVIEW-2026-08-19.md](REVIEW-2026-08-19.md) | 子代理的只读交叉复核:命令 / 表名 / 文件格式 / 母文档冲突 / 写作纪律 / 漏项 + 九篇 44 条开放问题汇总(§7,每条带建议默认答案);前 10 条已按 00-handshake 的默认答案修进各篇 | — | 已处理 |

预研五篇在 [`../research/`](../research/):orca、deepseek-harness、codegraph(第三轮已有),workflow-skill-packages、chat-group、legacy-backfill、kanban-drag-dioxus(第四轮补)。

## 每篇的写法(模板;写新篇照抄)

```
# NN · 标题
> 30 秒导读:这篇是什么 / 给谁看 / 现在作数吗(「详细设计稿,待用户复核」)
## 0 · 这篇管什么、不管什么(对应母文档哪几节、待拍-NN;与其它设计篇的边界)
## 1 · 用户看到什么、做什么(旅程视角,人话)
## 2 · 设计(结构、流程、规则;文字 + 必要的文本图 / 表)
## 3 · 工程对照(crate / 模块 / 命令 / 事件 / 表 / 文件;Rust 伪码只准出现在这一节)
## 4 · 边界与失败(不做什么;失败如何如实显示,绝不假装)
## 5 · 验收与读回(E2E:深链启动 + SQL 读回 + 截图;每条能复算)
## 6 · 开放问题(≤5,给用户拍)
```

写作规矩(来自 `CLAUDE.md`「写作纪律」):正文人话;实现术语(settle-once、derive-only 这类)只进第 3 节;代号第一次出现要带一句解释;**不新开代号系列**(要编号就写「第 N 步」「第 N 块」);引用决策写「待拍-NN」;不编数字,样例数字标来源或标「演示」;未建的功能写「未建」。

## 与代码的关系

这些稿子不改 `crates/`。开工时按 01 的顺序建新目录,每篇的第 3 节就是那一块的开工清单;每篇第 5 节就是那一块的验收清单。
