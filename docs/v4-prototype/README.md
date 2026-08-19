# docs/v4-prototype/ · V4 特性规划

> **30 秒导读**：V4 的规划入口。2026-08-17 进入特性规划期；**2026-08-18 全貌草案经三轮 grilling 收口，转为设计事实源**（见下表），设计篇（逐屏规格 / 高保真）接着做。不要往这里塞 V3 的 bug 修法。当前出包与 V3 修 bug 见 [`../releases.md`](../releases.md)；还没干的活见 [`../LEFTOVERS.md`](../LEFTOVERS.md)；文档写哪见 [`../doc-boundaries.md`](../doc-boundaries.md)。

看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md)。

## 现在作数

| 文件 | 是什么 | 状态 |
|---|---|---|
| 本 README | V4 规划入口 | 作数；特性清单未齐 |
| [intake-user-brief.md](intake-user-brief.md) | 2026-08-17 用户口述原文（grilling 原料） | 原样落盘；**不是设计、不是范围** |
| [mvp-blueprint-draft.md](mvp-blueprint-draft.md) | 2026-08-18 从口述 + V1-V3 实践 + 代码事实推出的 V4 MVP 全貌草案（七站旅程 / 指标·周目标·活关系 / §2.6 触发与介入全景表 / 信息架构 / 数据 / 建法 / 验收）；第三轮：两轮反馈 22 条全部已定，评审子代理对照口述与反馈复核过、13 处修订已落 | **作数**（V4 设计事实源）；同名 .html 是给人审阅的交互版，内容以 .md 为准 |
| [standard-module-draft.md](standard-module-draft.md) | 规范铺底模块设计：八大类、每类怎么做、正本住 `standard/`、版本与升级、如何被人评审持续优化；三张运作活 | 已定（随母文档转作数） |
| [research/](research/) | 三篇源码级预研（子代理产出）：[orca.md](research/orca.md)（终端内嵌 / 复制 / 右侧栏；结论借模式）、[deepseek-harness.md](research/deepseek-harness.md)（插件框架；结论只借接口设计判断，Open Design 与它零引用）、[codegraph.md](research/codegraph.md)（本仓已在 CI 用；结论 AGENTS.md 一段 + 侧栏留口 + 运作活②找大文件） | 预研，已融入草案 |
| [handoff-prompt-legacy-cut-merge.md](handoff-prompt-legacy-cut-merge.md) | 给减负线（PR #102 + cut-legacy-engine）作者的收尾合入 prompt | 一次性，用完可归档 |
| [review-pack/](review-pack/) | 内部专家评审包：[brief.md](review-pack/brief.md)（5 分钟一页纸：愿景 / 旅程 / 十二条决策 / 特性 / 收益 / 未来 / 请专家拍的五问）、[deck.html](review-pack/deck.html)（20 分钟翻页讲稿）、README（按时长排的阅读顺序） | 评审用；内容以草案为准 |
| [hifi/](hifi/) | 高保真可点击原型：[README.md](hifi/README.md) 是简报（逐屏内容 + 样例数据出处，样例 = buddy 自己的仓 2026-W34），[index.html](hifi/index.html) 是单文件原型（浏览器直接开；顶部常驻「演示数据」横幅，✱ = 假设值） | 用户拍板顺序：**先高保真 → 反馈 → 再详细设计**；原型收反馈中 |

## 已点名、尚未立项的意向

这些只是实践里否过「塞进 V3」的题目，**不是 V4 范围**，等规划时再决定进不进：

- 一个项目、多条在跑的版本线（总览先要诚实回答「看哪条线」）。过程见 PRACTICE §4.16，不在这里展开。

有了第一条正式特性，在本目录开设计篇，并改本表状态。走 `buddy-feature-dev`。
