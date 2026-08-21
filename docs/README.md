# docs/ — 全仓文档地图

> **30 秒导读**:这一页告诉你**仓库里的每一堆文档是干什么的、现在还作不作数、该不该动**。给新接手的人和 AI 会话看。**现在作数**(2026-08-17 减负重构会话建,随目录变动维护)。仓库根的 [`README.md`](../README.md) 是总入口;这里只管文档。

## 一张图

```
仓库根
├── README.md          总入口:是什么 · 怎么跑 · 文档地图
├── CLAUDE.md          给 AI 会话的工作说明(产品命题 · 写作纪律 · 门禁 · 核心纪律)← 规则的正本
├── AGENTS.md          给通用 agent 工具的 CodeGraph 导航,一句话指回 CLAUDE.md
├── CONTEXT.md         领域语言词表(队友/交棒/观测/蒸馏……唯一规范用词来源)
├── DEVELOPMENT.md     开发指南:工作区布局 · 常用命令 · headless 例子 · CodeGraph
├── plan/              现役计划与规范 7 篇(06 设计事实源 · 07 命题 · 08 MVP 定义 · 13/15/16/20 规则)
├── iterations/        只剩 PRACTICE-buddy.md(伙伴的实践日志,持续更新)
├── docs/              ← 你在这
├── e2e/               验收流考卷(toml)与种子库
├── examples/          样板间库(aihot)+ 三个 vendor 技能库
└── scripts/           门禁脚本 · 验收流工具链 · 本机联调脚本
```

## docs/ 里的四类东西

### 1. 现役文档(改东西前读;过时了要么改要么加横幅)

| 路径 | 是什么 | 谁在维护 |
|---|---|---|
| [`v1-prototype/`](v1-prototype/) → [`v2-prototype/`](v2-prototype/) → [`v3-prototype/`](v3-prototype/) | **当前迭代线**:V1 产品化(纳入项目简化 / 交互式指标环 / 内嵌终端 / 总览重构)→ V2(调度统一 / 最简多人)→ V3(原型进度页内嵌 Open Design)。**2026-08-20 归档整理**:三个目录里已落地、且没有被现役文档引用为当前权威的 8 篇设计事实源搬去了 [`archive/v1-prototype/`](archive/v1-prototype/)/[`v2-prototype/`](archive/v2-prototype/)/[`v3-prototype/`](archive/v3-prototype/)(按篇判断,不按目录整批搬);每个目录自带 README 与逐文件状态表,现在只列仍留在现役目录的篇章。**V4 见 [`v4-prototype/`](v4-prototype/)**——2026-08-21 收敛成四件套:[设计正本一篇](v4-prototype/design.md) + [高保真原型](v4-prototype/hifi/) + [产品指南](guide/v4-产品指南.html) + [对外宣讲](guide/v4-对外宣讲.html);原来的母文档与 14 篇细分设计已归档到 [`archive/v4-prototype/`](archive/v4-prototype/)。代码已建完合进 main(内核 `crates/bw-v4` + 能力底座 `crates/v4-engine` + 新壳 `crates/app-shell`,与 V3 旧壳并存、互不依赖),试点在跑。遗留清单已升格到 `docs/LEFTOVERS.md`(下一行),`v1-prototype/LEFTOVERS.md` 只剩指针 | 伙伴会话(V1/V2/V3/V4 分支已全部合入 main) |
| [`LEFTOVERS.md`](LEFTOVERS.md) | **离「V4 的 MVP 做完了」还差什么**的唯一清单,13 处,按 `design.md` §13.1 的验收八条分组。2026-08-21 从 537 行重写成 91 行:做完的史实、有意的留白、V3 的欠账一律不记 | 2026-08-21 试点会话重写;加条目前先回答「验收八条里哪一条过不去」 |
| [`doc-boundaries.md`](doc-boundaries.md) | 文档边界:什么写到哪、不写到哪 | 现在作数 |
| [`releases.md`](releases.md) | 版本登记:出包与运作 | 现在作数 |
| [`code-schemes.md`](code-schemes.md) | 代号索引(P/S/W/R/L/A/D/G/K/M/T/V……),新开一批代号先来这里登记 | 谁开代号谁登记 |
| [`adr/0001-ubiquitous-language.md`](adr/0001-ubiquitous-language.md) | 确立 `CONTEXT.md` 为领域语言规范的决定 + 改名台账 | 改术语时更新 |
| [`superpowers/specs/`](superpowers/specs/) | 仍作数的设计稿三篇:2026-07-22 GitHub 主体化入门(plan/13 引用)、2026-08-05 技能五角色归类(`bw-core/src/stage_catalog.rs` 引用)、2026-08-17 减负重构(本次) | 过时即搬 `archive/superpowers/` |
| [`guide/`](guide/) | 给同事用的两篇:[`v4-产品指南.html`](guide/v4-产品指南.html)(上半「怎么用」跟着伴飞长、**只写已实践**;下半「怎么给 buddy 开发新特性」)、[`v4-对外宣讲.html`](guide/v4-对外宣讲.html)(讲给同事听的讲稿,零实现术语零代号)+ V1 期留下的指南 HTML 原型与 `填写规范.md` | V4 指南:伴飞中,每走完一站补一站 |
| [`metrics/workflowhub/`](metrics/workflowhub/) | **WorkflowHub 这个项目**的指标正本 [`metrics.toml`](metrics/workflowhub/metrics.toml) + 推导记录 [`metrics-rationale.md`](metrics/workflowhub/metrics-rationale.md)。正本 2026-08-21 从仓根 `.bw/` 移来:它装的是 WorkflowHub 的五条指标,不是 loop-buddy 自己的,留在 `.bw/` 会被 V4 当成本仓的指标读出来 | 指标变动时 |
| [`examples/metrics.toml.sample`](examples/metrics.toml.sample) | `.bw/metrics.toml` 可复制样例 | 格式变动时 |

### 2. 运行时资产(被 `include_str!` 编进二进制——**改它就是改产品行为**,不是改文档)

| 路径 | 编进哪 |
|---|---|
| [`buddy/system-prompt.md`](buddy/system-prompt.md)、[`buddy/standards/metrics.md`](buddy/standards/metrics.md)、[`buddy/standards/connectors.md`](buddy/standards/connectors.md) | `crates/bw-core/src/buddy_assets.rs`——注入交互式会话的系统提示词、`.bw/metrics.toml` / `.bw/connectors.toml` 的格式规范正本 |
| [`skills/*/SKILL.md`](skills/) | `crates/bw-core/src/bw_library.rs`——**V3 自带的 9 个基础技能包正本**(evidence-first / spec-to-tests / baseline-before-touch / fresh-eyes-funnel / breaking-drill / competitive-analysis / north-star-discovery / metrics-binding / metrics-render)。**V4 不再读这个目录**:V4 自带的技能与运作剧本收敛在仓根 `standard/skills/` 一个平铺目录里 |

`buddy/README.md` 有这批资产的自述。**不要搬这两个目录**——路径是编译期硬绑定。

### 3. 历史档案([`archive/`](archive/),只加不改)

早期路线选型(`plan/00-05`)、做完即历史的执行批次(`plan/09-12,14,17-19,21`)、交接记录与 aihot 践行日志(`iterations/`)、superpowers 实施计划原件、Rust 重写前的 HTML 原型稿(`design/`)、2026-07 的演示报告与动图(`verification/`)、V1/V2/V3 迭代线里已落地的设计事实源(`v1~v3-prototype/`,2026-08-20 归档,8 篇)。规则与目录见 [`archive/README.md`](archive/README.md)。**编号语义保留**:源码注释里的 `plan/09 §2` 去 `archive/plan/09-…` 找。

### 4. 不在 docs/ 但容易找错地方的

| 你在找 | 在哪 |
|---|---|
| 「现在到底该按哪份文档干活」 | `docs/v1-prototype/README.md` 起,顺着 v2 → v3;设计层面拿不准查 `plan/06`;产品命题查 `plan/07` |
| 门禁命令 / 深链启动 / 读回纪律 | `CLAUDE.md`「常用命令」「核心纪律」;`DEVELOPMENT.md` |
| 领域词(队友、交棒、观测、蒸馏……) | `CONTEXT.md` |
| headless 例子(指挥器 / 灌库 / 巡检 / PTY 烟测) | `DEVELOPMENT.md`「headless 例子」一节;源码在 `crates/bw-app/examples/`、`crates/bw-engine/examples/` |
| 验收流(考卷 / 真点击 / 证据报告) | `plan/15` + `e2e/` + `scripts/run-flow.py` |
| 样板间库怎么再生 | `examples/README.md` |
| 产品名 | 仓库真名 **loop-buddy**(GitHub);产品名 **Builders' Workbench(BW)**;`buddy` 是产品里那个 AI 队友/程序的自称,三者是一件东西的三个称呼 |
