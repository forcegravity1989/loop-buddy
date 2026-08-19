# docs/v4-prototype/ · V4 特性规划

> **30 秒导读**：V4 的规划入口。2026-08-17 进入特性规划期；**2026-08-18 全貌草案经三轮 grilling 收口，转为设计事实源**；2026-08-19 内部专家评审后草案进第四轮（映射三列 / 看板拖拽 / 项目群 / 老项目回填 / 易用性守则），高保真升 v4，**详细设计开写（`design/`）**；**2026-08-20 用户回二次握手清单 17 条，草案进第五轮**（拖拽统一 / 默认 mattpocock-skills / 运作活②改「资产盘点」且历史回填 = 它的首次模式 / 库本机 + 周计划记指标读数 / 试点 = buddy 自己的仓），高保真升 v5、design/ 同步（见下表）。不要往这里塞 V3 的 bug 修法。当前出包与 V3 修 bug 见 [`../releases.md`](../releases.md)；还没干的活见 [`../LEFTOVERS.md`](../LEFTOVERS.md)；文档写哪见 [`../doc-boundaries.md`](../doc-boundaries.md)。

看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md)。

## 现在作数

| 文件 | 是什么 | 状态 |
|---|---|---|
| 本 README | V4 规划入口 | 作数；特性清单未齐 |
| [intake-user-brief.md](intake-user-brief.md) | 2026-08-17 用户口述原文（grilling 原料） | 原样落盘；**不是设计、不是范围** |
| [mvp-blueprint-draft.md](mvp-blueprint-draft.md) | 2026-08-18 从口述 + V1-V3 实践 + 代码事实推出的 V4 MVP 全貌草案（七站旅程 / 指标·周目标·活关系 / §2.6 触发与介入全景表 / 信息架构 / 数据 / 建法 / 验收）；第三轮 22 条全部已定；**第四轮（2026-08-19 专家评审）加 待拍-24~28 与 §3.5 易用性审视；第五轮（2026-08-20 用户回握手）改 待拍-10/12/25/27 并加 待拍-29** | **作数**（V4 设计事实源）；同名 .html 是第三轮的审阅版（顶部有第四轮说明），内容以 .md 为准 |
| [standard-module-draft.md](standard-module-draft.md) | 规范铺底模块设计：八大类、每类怎么做、正本住 `standard/`、版本与升级、如何被人评审持续优化；三张运作活 | 已定（随母文档转作数） |
| [research/](research/) | 源码级预研（子代理产出）。第三轮三篇：[orca.md](research/orca.md)（终端内嵌 / 复制 / 右侧栏；结论借模式）、[deepseek-harness.md](research/deepseek-harness.md)（插件框架；只借接口设计判断）、[codegraph.md](research/codegraph.md)（AGENTS.md 一段 + 侧栏留口 + 运作活②找大文件）。第四轮四篇（专家反馈补）：[workflow-skill-packages.md](research/workflow-skill-packages.md)（workflow = SOP 类技能包怎么识别 / 注入 / 记账）、[chat-group.md](research/chat-group.md)（项目群适配工厂：发消息 / 拉历史）、[legacy-backfill.md](research/legacy-backfill.md)（老项目历史回填的原料与产物；附 buddy 自己仓的样例）、[kanban-drag-dioxus.md](research/kanban-drag-dioxus.md)（Dioxus/wry 下看板拖拽可行性） | 预研，已 / 正在融入草案与设计 |
| [handoff-prompt-legacy-cut-merge.md](handoff-prompt-legacy-cut-merge.md) | 给减负线（PR #102 + cut-legacy-engine）作者的收尾合入 prompt | 一次性，用完可归档 |
| [review-pack/](review-pack/) | 内部专家评审包：[brief.md](review-pack/brief.md)（5 分钟一页纸：愿景 / 旅程 / 十二条决策 / 特性 / 收益 / 未来 / 请专家拍的五问）、[deck.html](review-pack/deck.html)（20 分钟翻页讲稿）、README（按时长排的阅读顺序） | 评审用；内容以草案为准 |
| [hifi/](hifi/) | 高保真可点击原型 v5：[README.md](hifi/README.md) 是简报（逐屏内容 + 样例数据出处，样例 = buddy 自己的仓 2026-W34；§0 三张「反馈 → 决定」表），[index.html](hifi/index.html) 是单文件原型（浏览器直接开；顶部常驻「演示数据」横幅，✱ = 假设值） | 用户拍板顺序：**先高保真 → 反馈 → 再详细设计**；v4 = 专家评审反馈已折入；v5 = 用户第五轮拍板已折入（所有列可拖 + 状态动作确认弹窗、卡面无按钮、默认 mattpocock-skills、资产盘点） |
| [design/](design/) | **详细设计稿**（第一版，2026-08-19 夜）：[README](design/README.md) 是目录 + 七节模板；01 架构与模块管控 · 02 数据与文件格式 · 03 规范铺底与老项目回填 · 04 开工工具与 workflow · 05 会话屏 · 06 计划屏 · 07 通知与项目群 · 08 总览推导 · 09 运作活剧本 · 10 验收与 E2E · 11 知识库；`00-handshake.md` 是二次握手清单（顶部是用户 2026-08-20 的回复与处置），`REVIEW-2026-08-19.md` 是交叉复核与开放问题汇总 | **代码已照着开工并跑完三刀**（A 骨架+数据+主环 / B 运作活+会话屏 / C 回填+项目群+知识库），全在 PR #105、未合；各篇第 3 节已按实况整块重写（标「X 刀落地后重写」的是实况）。没做完的只认 [`../LEFTOVERS.md`](../LEFTOVERS.md) 的 V4A/V4B/V4C |

## 已点名、尚未立项的意向

这些只是实践里否过「塞进 V3」的题目，**不是 V4 范围**，等规划时再决定进不进：

- 一个项目、多条在跑的版本线（总览先要诚实回答「看哪条线」）。过程见 PRACTICE §4.16，不在这里展开。

有了第一条正式特性，在本目录开设计篇，并改本表状态。走 `buddy-feature-dev`。
