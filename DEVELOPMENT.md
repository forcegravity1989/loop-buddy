# Builders' Workbench — 开发指南

> **30 秒导读**:本文是给开发者(人或 AI)的日常操作手册:工作区布局、常用命令、验证方式。产品是什么见 `plan/07`;设计事实源见 `plan/06`;当前执行计划见 `plan/08`;给 AI 的工作纪律(含写作纪律)见 `CLAUDE.md`;术语见 `CONTEXT.md`。
>
> 2026-08-05 重写:旧版正文是「七控制点→五阶段」迁移期的记录,其中「出口闸门」测试(spine.rs/monitor.rs)已随 2026-07-17 的「不留单元测试」纪律删除,`ProjectCycle` 等类型也已改名——旧版描述不再可信,历史见文末沿革一段。

## 工作区布局

```
crates/
  bw-core/      领域内核:StageKind(五阶段,每阶段自带角色/方法论/DoD/交棒文案/
                反模式等静态元数据)+ MaturityPeriod(时期)+ Issue 状态机与合法
                转移表 + 度量派生链类型。零 IO 零 UI,必须 wasm32 可编译。
  bw-engine/    Executor trait + MockExecutor(演示替身,可配延迟)+
                ClaudeCliExecutor(shell 出 `claude -p`,真实读写文件)+
                evidence.rs(从工作区采集 git/docs/测试真实状态,回流成观测)
  bw-store/     SQLite(sqlx):schema.sql + add_column_if_missing 迁移守卫;
                交棒/观测等只追加表;store 不做业务判断(哑存储)
  bw-app/       编排大脑:App + Command/Event 总线,所有用例与守卫都在这层
  ui/           纯函数 selector + ViewModel(state → 可渲染 DTO)
  app-desktop/  真壳(Dioxus 0.7,hard-pin =0.7.9):kernel 桥(独立 tokio 线程)+ 各屏
  app-web/      非 workspace 成员,「以后也许」的预留位,不编译
```

`default-members` 只含无头内核 + ui,故日常 `cargo check` **不编译 Dioxus**;桌面壳需显式 `-p app-desktop`。

## 常用命令

```bash
cargo check -p bw-app             # 日常最快的编译检查(内核+应用,不编 Dioxus)
cargo run -p app-desktop          # 启动桌面应用(BW_DB=path 可覆盖数据库位置)

# 提交前门禁(与 CI 完全一致):
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop

# headless 走完整生命周期(不开界面):
cargo run -p bw-app --example real_demo -- <db-path> <workspaces-root> [--mock] [--only <slug>]
```

## 共享代码图谱

仓库内的 `.codegraph/codegraph.db` 是给全体成员共用的代码关系快照。先安装
`scripts/codegraph-version` 指定的 CodeGraph 版本；理解跨文件调用链或影响面时,
优先运行 `codegraph explore`、`callers`、`callees`、`impact`,不要先把大量源码整份
读进上下文。改代码后运行 `codegraph sync`,让本地查询保持最新。

功能分支不要提交数据库:PR 上的 `CodeGraph / verify shared graph` 会独立增量更新并
检查图谱和 SQLite 完整性;合入 `main` 且常规 `CI` 通过后,
`CodeGraph / publish shared graph` 由 GitHub Actions 串行刷新数据库并提交。这样只有
机器人写共享的二进制快照,并行 PR 不会围绕数据库产生合并冲突。需要本地复核同一套
门禁时运行:

```bash
./scripts/verify-codegraph.sh
```

## 怎么验证(本仓库不写单元测试)

2026-07-17 起的纪律:**行为正确性靠端到端验证,不靠测试基线**。具体做法、深链环境变量、computer-use 的坑,全在 `CLAUDE.md`「核心纪律」一节,此处只记住三句话:

1. 任何「已完成/数字是 X」的说法,必须能用 `sqlite3` 从数据库独立查出来核对(「读回」);
2. 深链启动(`BW_OPEN=<项目名> BW_PANEL=<面板>`)后 stderr 出现 `[BW_OPEN]` 日志,就是「界面真的渲染了」的证明;
3. 产品的不可违反约束(完成永远由人点、信号绝不手设等)钉在类型和守卫里,编译过就守住了,配合 `/code-review` 与 E2E 抽查。

## 两条不可妥协(已钉进类型与 CI)

1. **UI 无关内核**:五个内核 crate 禁依赖 dioxus/tauri/wry/leptos(`guard-kernel-ui-free.sh` 强制);wasm32 check 保住将来出 Web 版的可能性。UI 改动只准进 `app-desktop`。
2. **健康永远推导**:信号只能从真实观测推导,不能手动设置(`Derived<Signal>` 密封、store 无 `set_signal`、`recompute_signals` 是唯一写入者);观测只追加,一个观测=一个点;**无数据 = Unknown ≠ 绿**。

## 五阶段方法论(长期有效)

项目分五个持续运营的阶段,每段一个主持角色、一套打法,首尾相接成环(运维复盘回流原型):

| # | 阶段 | 角色 | 方法论 | 求什么 | 循环节奏 |
|---|---|---|---|---|---|
| 1 | 原型 | 原型师 | 假设驱动探索 | 求真 | 小时级 · 48h 一圈 |
| 2 | 构建 | 构建师 | 规格驱动交付 | 求成 | 天级 · Spec→合入 |
| 3 | 优化 | 优化师 | 度量驱动打磨 | 求简 | 天—周级 · 基线→回归 |
| 4 | 运营推广 | 运营推广师 | 增长实验 | 求增 | 周级 · 实验批次 |
| 5 | 运维 | 运维师 | 可靠性工程 SRE | 求稳 | 持续 · 无终点 |

每阶段的核心问题/方法循环/DoD 清单/AI 编队/反模式,全是 `bw-core` 里 `StageKind` 的**静态方法论元数据**——通用打法,不随项目现编。从一个阶段推进到下一阶段叫「交棒」(`HandoffStage` 命令):DoD 没勾满不会拦你,但会记成「带险交棒」永久留痕。

## 创建流(概要)

新项目走「仓 → 意图 → 快速问题 → 起草 → 确认」的引导流,快、能退、能续;起草那一步**永久用演示替身(mock)跑**——这是设计不是降级,真正费时间的活交给创建落地时自动配好的标配 Issue 三件套。细节与拍板记录见 `plan/13`(GitHub 为正本)与 `plan/14`(体验),术语见 `CONTEXT.md`「创建流」一节。

## 历史沿革(一段话)

本仓库 2026-07 上旬完成过一次大迁移:从原型的「七控制点线性向导」整体迁移到「五阶段=角色=方法论 · 交棒制 · 闭环回流」模型(过程记录在 `plan/00~05` 与 `iterations/`,均已标注为历史档案);2026-07-17 删除单元测试基线,转向端到端验证;2026-07-22 确立领域语言词表(`CONTEXT.md` + `docs/adr/0001`),`ProjectCycle→MaturityPeriod` 等改名即出自那次决定。旧版本文的「P0/P1/P2 里程碑」编号属于 `plan/00` 的旧路线图,现已不再使用(代号消歧见 `docs/code-schemes.md`)。
