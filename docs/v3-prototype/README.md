# docs/v3-prototype/ · V3 设计导读

> **30 秒导读**:V3 的设计与使用修复入口。**当前阶段：V3 修 bug / 推广，V4 特性另见 [`../v4-prototype/`](../v4-prototype/README.md)。** 出包与版本号见 [`../releases.md`](../releases.md)；还没干的活见 [`../LEFTOVERS.md`](../LEFTOVERS.md)。Open Design 穿刺过程仍在 V2 的 spike 文，那是史实。

看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md)；代号查 [`../code-schemes.md`](../code-schemes.md)。

## 现在作数

| 文件 | 是什么 | 状态 |
|---|---|---|
| 本 README | V3 范围入口 | 作数 |
| [cursor-agent-executor.md](cursor-agent-executor.md) | **V3-cursor-cli**：Issue 调度接 Cursor Agent CLI；配置面 + 最小代价接法 | **设计已记，未落地** |
| [cowelink-web-sidecar.md](cowelink-web-sidecar.md) | **V3-cowelink-sidecar**：cowelink 长出本机网页旁路，buddy iframe 内嵌（不弹窗） | **设计已记，未落地** |
| [onboard-list-and-claude-resolve.md](onboard-list-and-claude-resolve.md) | **V3-use-fix**：安装器/探测认 `claude.cmd` + 创建流拉 999、可搜索下拉、程序图标 | **本轮落地** |

穿刺事实源（Open Design，已落地到原型进度）：[`../v2-prototype/open-design-embed-spike.md`](../v2-prototype/open-design-embed-spike.md)

本轮已见行为:项目里点阶段轴「原型」、停在「进度」→ 中间是本机 Open Design 首页。总览和其它阶段进度不动。Open Design 没开着就显示空态。完成清单收到底栏。

未落地两篇同时挂在 [`../LEFTOVERS.md`](../LEFTOVERS.md)（V3 可排）。

## 未落地两篇怎么读

- 想知道「Issue 用 Cursor 还是 Claude、今天在哪配」→ [cursor-agent-executor.md](cursor-agent-executor.md) §1–§2。今天没地方配；落地时是设置里的本机默认 + 智能体卡「执行引擎」。
- 想知道「WeLink 为什么不弹窗、怎么嵌」→ [cowelink-web-sidecar.md](cowelink-web-sidecar.md)。第一张穿刺打在 cowelink 仓，buddy 等有 URL 再嵌。
- orca 整窗内嵌：**不做**。多会话已在 Issue 终端里。
