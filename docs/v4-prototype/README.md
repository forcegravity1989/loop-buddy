# docs/v4-prototype/ · V4

> **30 秒导读**:V4 的入口。V4 是 buddy 的 MVP 版本——**仓是正本、库只留四张表、健康与指标全部现算**,界面收成六个入口。**现在到哪一步了**:设计已定稿,代码分七刀(A–G)建完并合进 `main`(V4 内核 `crates/bw-v4` + 新壳 `crates/app-shell`,与 V3 的旧壳并存、互不依赖);**试点在跑第 1-3 站**,一边跑一边改。还没干的活只认 [`../LEFTOVERS.md`](../LEFTOVERS.md) 的 V4A–V4G 七组;出包与版本号见 [`../releases.md`](../releases.md);文档写哪见 [`../doc-boundaries.md`](../doc-boundaries.md)。看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md),代号查 [`../code-schemes.md`](../code-schemes.md)。

## 这里有什么

| 文件 | 是什么 | 怎么用 |
|---|---|---|
| [mvp-blueprint-draft.md](mvp-blueprint-draft.md) | **母文档**:V4 是什么、一个项目一周怎么转一圈(第 0 站 + 每周六站)、指标 / 周目标 / 活的关系、信息架构、信息住哪、验收标准,以及 §11 的决策台账「待拍-01~32」 | 要理解 V4 想干什么、某条决定为什么这么定,来这里。与 design/ 冲突时以本篇为准 |
| [design/](design/) | **详细设计 13 篇**:架构、数据与文件、规范铺底、工具与 workflow、会话屏、计划屏、通知与项目群、总览推导、运作活剧本、验收、知识库、建法记录、壳与高保真的对齐记录 | 要改代码、要知道某个数字怎么算出来的,来这里;各篇第 3 节写的是真代码 |
| [standard-module-draft.md](standard-module-draft.md) | **规范铺底模块**:八个大类各是什么、落成哪些文件、谁写谁更新、`standard/` 正本怎么演进 | 要往项目仓的规范里加东西、要理解铺底铺的是什么,来这里 |
| [hifi/](hifi/) | **高保真原型**:[index.html](hifi/index.html) 浏览器直接打开、可点击;[README.md](hifi/README.md) 是逐屏简报与「反馈 → 决定」的五版变更表 | **视觉正本**——桌面壳的样式表 `crates/app-shell/assets/hifi.css` 就是从 index.html 整体搬过去的,改样式先看它 |
| [research/](research/) | 还在用的两篇源码级预研:[orca.md](research/orca.md)(终端内嵌、右侧栏;`crates/app-shell/src/adapters/` 的 README 引它)、[chat-group.md](research/chat-group.md)(项目群接口;WeLink 尚未实现,这是给同事的对接底稿) | 结论已采纳进设计的其余预研在 [`../archive/v4-prototype/research/`](../archive/v4-prototype/research/) |

设计期的一次性材料(用户口述原文、二次握手问答、交叉复核记录、内部专家评审一页纸)已归档在 [`../archive/v4-prototype/`](../archive/v4-prototype/)——正文里出现的「握手清单」「交叉复核」指的就是那两份,决定本身的正本是母文档 §11。

## 已点名、尚未立项的意向

不是 V4 范围,等规划时再决定进不进:

- 一个项目、多条在跑的版本线(总览先要诚实回答「看哪条线」)。过程见 `iterations/PRACTICE-buddy.md` §4.16。
