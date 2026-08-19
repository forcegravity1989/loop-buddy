# Builders' Workbench — 开发指南

> **30 秒导读**:本文是给开发者(人或 AI)的日常操作手册:工作区布局、常用命令、headless 例子清单、验证方式。产品是什么见 `plan/07`;设计事实源见 `plan/06`;**现在在做什么**见 `docs/v1-prototype/` → `v2-prototype/` → `v3-prototype/`;给 AI 的工作纪律(含写作纪律)见 `CLAUDE.md`;术语见 `CONTEXT.md`;全仓文档地图见 `docs/README.md`。**现在作数**(2026-08-17 随减负重构刷新)。

## 工作区布局

```
crates/
  bw-core/      领域内核:StageKind(五阶段,每阶段自带角色/方法论/DoD/交棒文案/
                反模式等静态元数据)+ MaturityPeriod(时期)+ Issue 状态机与合法
                转移表 + 度量派生链类型 + 技能规范机检 + 自带技能包正本
                (include_str! docs/skills、docs/buddy)。零 IO 零 UI,必须 wasm32 可编译。
  bw-engine/    InteractiveExecutor trait:InteractiveCliExecutor(交互式 `claude`,内嵌
                终端走 pty_backend.rs:Windows conpty-oxide / macOS·Linux portable-pty)+
                MockInteractiveExecutor(无工作区时的自标注替身)+ workspace.rs(项目仓 /
                issue worktree 供给)+ evidence.rs(从工作区采集 git/docs/测试真实状态,
                回流成观测)+ github/codehub/metrics_file/connectors_file(gh/codehub CLI
                与 .bw/*.toml)。2026-07 的 `claude -p` 按阶段循环旧引擎已于 2026-08-18 删除。
  bw-store/     SQLite(sqlx):schema.sql + add_column_if_missing 迁移守卫;
                交棒/观测等只追加表;store 不做业务判断(哑存储)
  bw-app/       编排大脑:App + Command/Event 总线,所有用例与守卫都在这层;
                hook_listener(claude hook 回报)、调度器 tick、PTY 字节抽干
  ui/           纯函数 selector + ViewModel(state → 可渲染 DTO)
  app-desktop/  真壳(Dioxus 0.7,hard-pin =0.7.9):kernel 桥(独立 tokio 线程)+ 各屏
                (wall/create/op/各 Hub)+ 深链 + BW_FLOW 进程内点击脚本

  ── 以下两个是 V4 新起的,和上面的 V3 链条并存、互不影响 ──
  bw-v4/        V4 内核:四张表的本机库(project/issue/claude_conversation/app_meta)+
                仓文件解析(PROJECT.md、.bw/*.toml、docs/plan/周文件、docs/releases.md)+
                现算推导(健康灯、指标读数、周列表)+ 命令/事件总线 + 标准件铺设。
                复用 bw-core 与 bw-engine,不依赖 bw-store / bw-app。
  app-shell/    V4 新壳(Dioxus 0.7):九屏(项目墙/总览/计划/会话/通知/配置/知识库/
                落地/设置)+ theme(视觉沿用 V3)+ bridge(独立 tokio 线程跑 bw-v4)。
                二进制名 `bw-v4-dev`,和老壳 `builders-workbench` 各跑各的。
```

`default-members` 只含无头内核 + ui,故日常 `cargo check` **不编译 Dioxus**;桌面壳需显式 `-p app-desktop`。Web 版是「以后也许」——架构靠 wasm32 keepalive 与 `Store` trait 留着门,仓里没有 app-web crate。

## 常用命令

```bash
cargo check -p bw-app             # 日常最快的编译检查(内核+应用,不编 Dioxus)
cargo run -p app-desktop          # 启动 V3 桌面应用(BW_DB=path 可覆盖数据库位置)

# V4 新壳(和老壳并存,库文件也是另一份 workbench-v4.db):
cargo check -p bw-v4              # V4 内核日常编译检查(不编 Dioxus,最快)
cargo run -p app-shell            # 启动 V4 新壳(二进制名 bw-v4-dev)
cargo run -p bw-v4 --example real_demo_v4 -- <db> <workspaces-root>   # V4 指挥器:不开界面跑完主环

# 提交前门禁(与 CI 完全一致,一条不能少):
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
./scripts/guard-no-cross-screen-import.sh      # V4:一屏一模块,屏与屏之间不互相 import
./scripts/guard-file-lines.sh                  # V4:单文件 1500 行硬上限、600 行软提醒
cargo check -p app-desktop
cargo check -p app-shell
cargo test --workspace --exclude app-desktop --exclude app-shell   # CI 也跑;现存内联测试要过(见「怎么验证」)
```

**环境变量**:`BW_DB`(数据库路径;默认 macOS `~/Library/Application Support/BuildersWorkbench/workbench.db`)· `BW_OPEN=<项目名>` + `BW_PANEL=progress|workflow|routine|artifact|version|issues`(启动深链到指定项目/面板,stderr 打 `[BW_OPEN]` 即渲染证明)· `BW_HUB=skill|agent|workflow|cron|connector|knowledge|activity|notify|settings` / `BW_SEL=skill|agent|workflow|cron|connector:<uuid>`(深链到 Hub / 组件详情)· `BW_WORKSPACES`(工作区根)· `BW_CLAUDE_BIN`(覆盖 `claude` 二进制路径)· `BW_FLOW=<command-file>`(进程内点击/断言脚本,验收流用)。

**V4 新壳的环境变量**(自成一套,别和上面那套混用):`BW_DB`(默认 `~/Library/Application Support/BuildersWorkbench/workbench-v4.db`)· `BW_OPEN=<项目 slug 或名字>` + `BW_PANEL=overview|plan|session|notify|config|kb` · `BW_VIEW=onboard|settings`(直接开落地页/设置页)· `BW_WORKSPACES`。启动时 stderr 打两行:`[BW_OPEN] …`(深链解析结果)与 `[BW_BOOT] projects=N db=…`(界面已渲染的证明)。

## headless 例子(不开界面直接驱动内核;每个都有现役用途)

| 例子 | 干什么 | 跑法 |
|---|---|---|
| `real_demo` | **唯一指挥器**:两个演示项目各走一圈产品主环——每阶段建活 → 指派阶段角色 → ▶跑(mock 交互执行器,项目无真实工作区)→ 代人点「完成」→ 蒸馏成技能 → 交棒到下一阶段;产出 evidence JSON。不碰 claude、不碰网关;同库重跑幂等 | `cargo run -p bw-app --example real_demo -- <db> <workspaces-root> [--only <slug>]` |
| `seed_fixture` | 给 e2e 种子库(`e2e/fixtures/demo.db`)补三张 fixture Issue | 见 `e2e/fixtures/README.md` |
| `verify_migration` | 打开一份**存量**库验证 schema 迁移不崩(只开不删) | `cargo run -p bw-app --example verify_migration -- <db>` |
| `verify_stage_catalog` | 技能五角色静态归类表自证(条数/重名/五阶段计数) | `cargo run -p bw-app --example verify_stage_catalog` |
| `audit_skills` | 按 `plan/16` 技能规范巡检整库(`--fix` 只做台账允许的修补) | `cargo run -p bw-app --example audit_skills -- <db> [--fix]` |
| `import_skill_library` / `import_skill_package` / `import_ecc_agents` | 灌库:从目录/单包/ECC 仓导入技能与队友(**应用内无导入界面,这是唯一路径**) | 各文件头有用法 |
| `sync_metrics_files` / `render_metrics` | 把各项目 `.bw/metrics.toml` 同步进库 / 把库里指标渲染成一页 HTML(配 `docs/skills/metrics-render`) | 各文件头有用法 |
| `build_aihot_fixture` | 从真实日常库再生 `examples/aihot/bw-aihot.db` 样板间 | 见 `examples/README.md` |
| `real_demo_v4`(bw-v4) | **V4 指挥器**:建项目 → 铺标准件 → 起周计划 → 建活 → ▶跑(mock 交互执行器)→ 代人点完成 → 发版 → 现算健康灯;产出 evidence JSON。不碰 claude、不碰网关;同库重跑幂等 | `cargo run -p bw-v4 --example real_demo_v4 -- <db> <workspaces-root> [--only <slug>]` |
| `pty_smoke`(bw-engine) | 走 ▶跑 同一条 `run_skill_pty` 路径起 `bash -c 'echo pty-ok'` 读回;`-- --teardown` 验证丢输入端后进程组被连坐;`-- --abort` 验证 `JoinHandle::abort()` 丢弃 future 后子进程照样被收尾(bw-app 中止走的就是这条) | `cargo run -p bw-engine --example pty_smoke [-- --teardown\|--abort]` |

2026-08-17 前 `crates/bw-app/examples/` 有 41 个例子,29 个是历史批次的一次性验证脚本(「已发货 commit 的收据」),已删,git 历史可找回。

## 共享代码图谱

仓库内的 `.codegraph/codegraph.db` 是给全体成员共用的代码关系快照。先安装
`scripts/codegraph-version` 指定的 CodeGraph 版本;理解跨文件调用链或影响面时,
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

## 怎么验证

**行为正确性靠端到端验证,不靠测试基线**(2026-07-17 起的纪律)。具体做法、深链环境变量、computer-use 的坑,全在 `CLAUDE.md`「核心纪律」一节,此处只记住三句话:

1. 任何「已完成/数字是 X」的说法,必须能用 `sqlite3` 从数据库独立查出来核对(「读回」);
2. 深链启动(`BW_OPEN=<项目名> BW_PANEL=<面板>`)后 stderr 出现 `[BW_OPEN]` 日志,就是「界面真的渲染了」的证明;
3. 产品的不可违反约束(完成永远由人点、信号绝不手设等)钉在类型和守卫里,编译过就守住了,配合 `/code-review` 与 E2E 抽查。

**关于内联单元测试(如实表述)**:仓里现存约 2,000 行内联测试(伙伴 V1/V2 引入),CI 的 `cargo test` 在跑,它们必须过。纪律是:**不要求写、现存的随 CI 跑、改到就顺手维护、不建回归大坝**——别把「补测试」当成交付物,也别删掉在跑的。

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

新项目走「仓从哪来 → 意图与确认」两卡向导(伙伴 V1 `docs/v1-prototype/issue1-onboard-simplify.md` 简化后的形态),快、能退、能续;创建落地时自动配好标配 Issue。拍板记录见 `plan/13`(GitHub 为正本:issue = GitHub issue、验收 = merge、`.bw/metrics.toml` 正本),术语见 `CONTEXT.md`「创建流」一节。早先「起草那一步永久用演示替身跑」的设计已随起草卡一起被 V1 砍掉,`RunDraftWorkflow` 命令 2026-08-17 删除。

## 历史沿革(一段话)

本仓库 2026-07 上旬完成过一次大迁移:从原型的「七控制点线性向导」整体迁移到「五阶段=角色=方法论 · 交棒制 · 闭环回流」模型(过程记录在 `docs/archive/plan/00~05` 与 `docs/archive/iterations/`,均已标注为历史档案);2026-07-17 删除单元测试基线,转向端到端验证;2026-07-22 确立领域语言词表(`CONTEXT.md` + `docs/adr/0001`),`ProjectCycle→MaturityPeriod` 等改名即出自那次决定;2026-08 上旬伙伴会话按 `docs/v1-prototype/` → `v2` → `v3` 迭代产品化(交互式指标环、内嵌终端、调度统一、最简多人、内嵌 Open Design);2026-08-17 减负重构(删死码与一次性脚本、补 macOS PTY 后端、归档冻结文档、拆大文件),取舍全在 `docs/superpowers/specs/2026-08-17-debt-reduction-refactor-design.md`。旧版本文的「P0/P1/P2 里程碑」编号属于 `plan/00` 的旧路线图,现已不再使用(代号消歧见 `docs/code-schemes.md`)。
