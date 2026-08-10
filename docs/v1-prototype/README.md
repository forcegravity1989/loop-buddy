# docs/v1-prototype/ · V1 产品化设计文档导读

> **30 秒导读**:本目录是 V1 产品化(把穿刺验证过的系统收口成最小可用集)的设计事实源集合,给接手开发的会话或人看。**现在作数**——只列当前指导实现的文档。本目录没有单独的「进度实况」表,进度以 `plan/08-mvp-execution-plan.md` 顶部「进度实况」为准;遗留问题以 `LEFTOVERS.md` 为唯一完整清单。
>
> 看不懂的词查 [`../../CONTEXT.md`](../../CONTEXT.md) 词表;看不懂的代号(P2、W6、R1……)查 [`../code-schemes.md`](../code-schemes.md) 代号索引;写作纪律见 [`../../CLAUDE.md`](../../CLAUDE.md)。

## 现在作数的设计文档

| 文件 | 是什么 | 状态 |
|---|---|---|
| [LEFTOVERS.md](LEFTOVERS.md) | **遗留问题唯一完整清单**(W1-1~4 / W2-1~7 / W3-1~9 / V1-P1 + 穿刺修复批索引 + 任务 A 收口 P1-P13)。每条带产生窗口、现象、未决点、处置、事实源 | 持续更新,读遗留只看此文件 |
| [issue1-onboard-simplify.md](issue1-onboard-simplify.md) | 窗口一:纳入项目(创建流简化)设计事实源 | 设计活跃,主体已落地 |
| [issue2-metrics-interactive-loop.md](issue2-metrics-interactive-loop.md) | 窗口二 Issue 2 主体:交互式 claude CLI 引擎 + 采集装置归位 + skill 重写 + guide。含心智模型 / 采集两 kind / 执行两轨 / Phase 拆分 / 文件级 / 偏差 | Issue 2 主体;W2-1 终端/会话部分已被下方重构篇归正 |
| [issue2-terminal-conversation-refactor.md](issue2-terminal-conversation-refactor.md) | 终端会话重构(活交付与 Claude 会话解耦)设计事实源——W2-1 的归正:四个生命周期拆开、多会话、切卡、重启恢复、咨询态、窄窗错行 | **当前活跃**,接手终端这块以此篇为准 |
| [issue2-all-issues-terminal-runs.md](issue2-all-issues-terminal-runs.md) | 终端会话重构收口:所有 issue ▶跑 走嵌入终端,老脚本调度路径退场,多 agent 转 prompt 驱动 | **当前活跃**,接续上一篇 |
| [issue3-overview-refactor.md](issue3-overview-refactor.md) | 窗口三:总览页(ProgressAll)重构设计事实源;项目指标区最终态以 `piercing-fixes-1.md` §8-10 为准 | 设计活跃,主体已落地 |
| [issue3-overview-mockup.html](issue3-overview-mockup.html) | 总览重构的高保真原型(toggle 现状/提议),视觉事实源 | 作数,作视觉对照 |
| [orca-terminal-session-reference.md](orca-terminal-session-reference.md) | orca-main 终端多会话架构摘要(可借鉴机制 + 源码锚点 + buddy 取舍),供终端重构参考 | 作数,参考对照 |
| [piercing-fixes-1.md](piercing-fixes-1.md) | 穿刺修复批次 1(cowelink W1 穿刺 7 条反馈)开发事实源;含项目指标区最终定型(Round 3-5) | 作数,被 issue3 引用为最终态权威 |
| [legacy-analysis-engine.md](legacy-analysis-engine.md) | 交互式引擎 / PTY 架构组(V1-P1 / W1-2 / W2-3 / W2-7)深度分析 | 作数;W2-1 段落已被归正,见篇首注记 |

## 已删除的文档(2026-08-07 整理)

以下文档已在本轮整理中删除,git 历史保留,需要可 `git show <hash>` 找回:

- `metrics-batch.md` / `lifecycle-batch.md` / `quickfix-batch.md`——三批「本会话开发」设计件,对应工作已落地(见 commits `f0a187a` / `5f81d6c`+`8863703`+`b689f9a` / `4cdf9e7`),结论已并入 `LEFTOVERS.md`。
- `legacy-analysis-metrics.md` / `legacy-analysis-lifecycle.md`——指标组 / 生命周期组的深度分析,工作已落地,结论并入 `LEFTOVERS.md`。

## 相关目录

- [`../../plan/`](../../plan/):整体设计与执行计划(`plan/06-08` 活跃,`plan/00-05` + `09-21` 历史档案,见 `plan/README.md` 导读)。
- [`../../iterations/`](../../iterations/):实践日志与交接记录;`PRACTICE-buddy.md` 是唯一持续更新的活文档,其余为历史档案。
