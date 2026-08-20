# docs/v1-prototype/ · V1 产品化设计文档导读

> **30 秒导读**:本目录是 **V1 史实**(把穿刺验证过的系统收口成最小可用集时的设计)。给接手这块的人看。2026-08-20 做过一次归档整理:**已落地、纯历史的设计篇搬去了 [`../archive/v1-prototype/`](../archive/v1-prototype/)**;本目录只剩仍在被现役文档引用为"当前权威"或包含未关闭欠账线索的几篇。遗留只认 [`../LEFTOVERS.md`](../LEFTOVERS.md);当前出包与节奏见 [`../releases.md`](../releases.md);文档边界见 [`../doc-boundaries.md`](../doc-boundaries.md)。
>
> 看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md) 词表;看不懂的代号(P2、W6、R1……)查 [`../code-schemes.md`](../code-schemes.md) 代号索引;写作纪律见 [`../../CLAUDE.md`](../../CLAUDE.md)。

## 现在作数的设计文档(留在本目录)

| 文件 | 是什么 | 状态 |
|---|---|---|
| [LEFTOVERS.md](LEFTOVERS.md) | 已改成指针,正文迁到 [`../LEFTOVERS.md`](../LEFTOVERS.md) | 不再追加 |
| [issue2-terminal-conversation-refactor.md](issue2-terminal-conversation-refactor.md) | 终端会话重构(活交付与 Claude 会话解耦)设计事实源——四个生命周期(活/交付运行/Claude 会话/终端连接)拆开、多会话、切卡、重启恢复、咨询态、窄窗错行 | **当前活跃**,接手内嵌终端这块以此篇为准 |
| [issue2-all-issues-terminal-runs.md](issue2-all-issues-terminal-runs.md) | 终端会话重构收口:所有 issue ▶跑 走嵌入终端,老脚本调度路径退场,多 agent 转 prompt 驱动 | **当前活跃**,接续上一篇;`docs/LEFTOVERS.md`「V2·阶段默认Skill」条的根因分析事实源在此 |
| [issue3-overview-mockup.html](issue3-overview-mockup.html) | 总览重构的高保真原型(toggle 现状/提议),视觉事实源 | **当前界面的视觉事实源**(`docs/archive/design/README.md` 也这样指) |
| [legacy-analysis-engine.md](legacy-analysis-engine.md) | 交互式引擎 / PTY 架构组深度分析。W2-1 段落已被归正(见篇首注记,以上面 issue2-terminal-conversation-refactor.md 为准);**其余四条(V1-P1 macOS、W1-2 clone 堵命令循环、W2-3 无预算封顶、W2-7 诊断 spike 清理)作者自述"仍作数"**——注意 V1-P1 已于 2026-08-17 修复,该条实际已过时,W1-2/W2-7 仍是 `docs/LEFTOVERS.md` 里开着的条目 | 部分仍作数,部分已过时(见上) |

## 已归档(2026-08-20,纯历史 · 主体已落地)

以下几篇已落地、且没有被任何现役文档引用为"当前权威",搬去了 [`../archive/v1-prototype/`](../archive/v1-prototype/)(顶部各加了历史档案横幅,正文未改):

- `issue1-onboard-simplify.md` —— 窗口一:纳入项目(创建流简化)开发事实源
- `issue2-metrics-interactive-loop.md` —— 窗口二 Issue 2 主体:交互式 claude CLI 引擎 + 采集装置归位 + skill 重写 + guide
- `issue3-overview-refactor.md` —— 窗口三:总览页(ProgressAll)重构开发事实源
- `piercing-fixes-1.md` —— 穿刺修复批次 1(cowelink W1 穿刺 7 条反馈)
- `orca-terminal-session-reference.md` —— orca-main 终端多会话架构摘要(参考对照,已让位给 issue2-terminal-conversation-refactor.md)

## 已删除的文档(2026-08-07 整理)

以下文档已在更早一轮整理中删除,git 历史保留,需要可 `git show <hash>` 找回:

- `metrics-batch.md` / `lifecycle-batch.md` / `quickfix-batch.md`——三批「本会话开发」设计件,对应工作已落地(见 commits `f0a187a` / `5f81d6c`+`8863703`+`b689f9a` / `4cdf9e7`),结论已并入 `LEFTOVERS.md`。
- `legacy-analysis-metrics.md` / `legacy-analysis-lifecycle.md`——指标组 / 生命周期组的深度分析,工作已落地,结论并入 `LEFTOVERS.md`。

## 相关目录

- [`../../plan/`](../../plan/):整体设计与执行计划(现役 7 篇:06/07/08/13/15/16/20,见 `plan/README.md` 导读;历史批次 2026-08-17 起归档在 [`../archive/plan/`](../archive/plan/),编号不变)。
- [`../../iterations/`](../../iterations/):只剩 `PRACTICE-buddy.md`(唯一持续更新的实践日志);历史交接记录归档在 [`../archive/iterations/`](../archive/iterations/)。全仓文档地图见 [`../README.md`](../README.md);还没干的活只认 [`../LEFTOVERS.md`](../LEFTOVERS.md)。
- 工作流 skill(常青):[`.claude/skills/buddy-feature-dev`](../../.claude/skills/buddy-feature-dev/SKILL.md)(功能开发)、[`.claude/skills/buddy-bugfix`](../../.claude/skills/buddy-bugfix/SKILL.md)(缺陷);旧 `v1-product-delivery` 仅作跳转。W1/W2/W3 等窗口号是史实,不进 skill。
- V2 规划入口:[`../v2-prototype/`](../v2-prototype/README.md)(维护节奏 + 调度简化 / 最简多人初始录入)。
