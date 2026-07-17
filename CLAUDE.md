# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 这个仓库在做什么

**Builders' Workbench(BW)**:单人构建者的 Rust 原生桌面工作台(Dioxus 0.7 / wry WebView,macOS+Windows)。

**产品命题**(原型引子页原文,完整拆解见 `plan/07-product-proposition.md`):

> **用 AI 时代的方式,一步步把一个项目的管理体系搭起来。** 走完,你拥有一套**可复制的项目管理方法,而不只是一块看板**。

针对的痛:传统项目管理要 5 个专职角色、10 道流程,信息靠开会和口头汇报流动。Builders 模式换成 **1 个 Builder + Agent Loop**:PRD+评审 → 原型即规格;甘特图 → 每周可验证增量;人工实现 → agent 产出 80%、人审 20%;状态周会 → 真实 telemetry,难造假。四个控制点(产品哲学,自始未变):**知道对标谁 / 每周在正常演进 / 让 agent loop 干活(人只守质量门与验收)/ 目标清晰且难造假**。

落到今天的实现,是四件互锁的事(括号内是代码里的对应物):

1. **管理体系自带,不用用户发明**:项目分五个阶段——原型→构建→优化→运营推广→运维——每阶段自带打法:该问什么、什么节奏、做到什么算完(DoD)、常见的坑(`StageKind` 静态元数据,通用方法论,不随项目现编);运维复盘回流原型,项目是环不是流水线;阶段推进=交棒,清单未勾完可以交但强制标「带险」留痕(append-only 审计)。
2. **活让 agent 干,人守验收门**:活=Issue 卡,指派给 AI 队友,一键真实开工(`RunIssue`,在项目真实目录里改文件跑测试);干完只到「评审中」,**「完成」永远由人显式点**(状态机 `can_transition_to` 里 Done 的入边只有 InReview);干砸如实停在原地可重试;定时任务只自动**建**活(Autopilot),绝不自动**完成**活。
3. **健康难造假**:健康灯只能被真实数据点亮——观测只追加、信号只派生(封口 `Derived<Signal>`,store 无 set_signal),**无数据=Unknown,绝不假装绿**,数据过期降级,手填带「手填」徽记;干活自动留痕:队友战绩、产物登记、运行成败耗时、阶段吞吐指标,全自动入账且同一件活绝不记两次(settle-once);任何界面数字都能 `sqlite3` 独立查证。
4. **经验复利,越用越强**:做完的 Issue 一键蒸馏成带正文的技能(记着来自哪件活),下次同类活自动注入、用一次记一次;队友胜率由真实战绩派生,绝不手设。

**反命题(防蔓延)**:不是团队协作平台(没有成员/群聊/收件箱)、不是通用看板(无拖拽/甘特;回退不给 UI)、不是审批系统(交棒只留痕不拦人)、不是云服务(AI 执行=本机 `claude` CLI,单次花费封顶);永远不替用户捏造健康。

## 常用命令

```bash
cargo check -p bw-app             # 日常:编译内核+应用(不编 Dioxus,快)
cargo run -p app-desktop          # 启动桌面应用(见下方环境变量)
# E2E 验证(核心纪律,取代单元测试):
BW_DB=<db> BW_OPEN=<项目名> BW_PANEL=<panel> ./target/debug/builders-workbench  # 深链启动,stderr [BW_OPEN] = 渲染证明
sqlite3 <db> "SELECT …"           # 数字一律 SQL 读回(读回为证,比截图更硬)
```

**门禁(每个 commit 前全过,与 CI 完全一致)**:

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop        # 桌面壳编译过
# 行为正确性靠 E2E(深链启动 + sqlite 读回 + computer-use)+ /code-review,不靠测试基线
```

**headless 实跑指挥器**(不开 UI 走完完整生命周期):

```bash
cargo run -p bw-app --example real_demo -- <db-path> <workspaces-root> [--mock] [--only <slug>]
./scripts/supervise-real-demo.sh <slug>   # 网关抖动期的幂等重试监理
```

**环境变量**:`BW_DB`(覆盖数据库路径)· `BW_OPEN=<项目名>` + `BW_PANEL=progress|workflow|routine|artifact|version|issues`(启动深链,stderr 打 `[BW_OPEN]` 日志,是桌面渲染的可靠证明)· `BW_WORKSPACES` · `BW_CLAUDE_BIN` / `BW_CLAUDE_MAX_BUDGET_USD`(真执行器配置)。

## 架构(crate 一览与数据流)

```
bw-core     领域内核:StageKind 五阶段元数据 / Issue 状态机与合法转移表 / 度量派生链类型
            (零 IO 零 UI,必须 wasm32 可编译;默认无 idgen 特性)
bw-engine   Executor trait + MockExecutor(可配延迟)+ ClaudeCliExecutor(shell 出 `claude -p`,
            真实读写文件)+ evidence.rs(从工作区采集 git/docs/测试真状态回流观测)
bw-store    SQLite(sqlx):schema.sql + add_column_if_missing 迁移守卫;handoff/observation 等
            append-only 表;store 无业务判断(哑存储)
bw-app      编排大脑:App + Command/Event 总线,所有用例与守卫都在这层;E2E 的命令层主战场
ui          纯函数 selector + ViewModel(state→可渲染 DTO),可单测/E2E 核验
app-desktop 真壳(Dioxus 0.7 hard-pin =0.7.9):kernel 桥(独立 tokio 线程)+ 各屏
app-web     非 workspace 成员,"以后也许"留口,不编译
```

数据流:UI 只发 `Command`、收 `Event`;`bw-app` 执行用例 → store 落库 → `recompute_signals` 重算 → 事件流回 UI。执行器按项目热插拔:未配置真实工作区的项目走 MockExecutor(产出自我标注为演示),配置了的每次调用新建 ClaudeCliExecutor。

**两条不可妥协(已钉进类型与 CI)**:

1. **UI 无关内核**:五个内核 crate 禁依赖 dioxus/tauri/wry/leptos(`guard-kernel-ui-free.sh` 强制);wasm32 check 保活 Web 留口。UI 相关改动只准进 `app-desktop`。
2. **健康永远 derive**:`Signal` 只能经封口的 `Derived<Signal>` 进缓存,store 无 `set_signal`,`recompute_signals` 是唯一写入者。观测 append-only,一个观测=一个点,绝不插值;**无数据 = Unknown ≠ 绿**。

## 核心纪律:一切实跑(验证你做的东西是"真"的)

这个仓库最大的风险不是编译不过,而是**做出徒有其形的东西**:面板渲染了但数字是编的、流程走通了但记账没发生。以下纪律定义了本仓库里"真实"的操作含义。**2026-07-17 起核心纪律转向:不再写/留单元测试——行为正确性靠 E2E(computer-use:深链启动 + screencapture + sqlite 读回)+ `/code-review` 把质量;产品铁律由类型与守卫在编译期守住,E2E 读回抽查。**

1. **报告不代答,读回为证**。任何"已完成/数字是 X"的陈述必须能从 DB 或工作区独立复核:
   ```bash
   sqlite3 demo-workspaces/bw-demo.db "PRAGMA table_info(issue);"     # 结构核验
   sqlite3 <db> "SELECT ... "                                          # 数字一律 SQL 读回
   BW_OPEN=<项目名> BW_PANEL=issues target/debug/builders-workbench   # 深链 stderr 日志 = 渲染证明
   ```
   演示/报告里的每个数字都从真实 DB 读出,绝不硬编码(`real_demo` 的 evidence JSON 模式)。
2. **mock 必须自我标注**。MockExecutor 路径的产出带【mock】/「流程演示」字样,文档如实注明;mock 存在的唯一目的是廉价验证管线本身,绝不冒充真实执行。
3. **E2E 验证绝不依赖网关**。验证动作 = 临时/演示 DB + 深链启动到目标面板(stderr 见 `[BW_OPEN]` 即渲染成功、无 panic)→ `sqlite3` 读回核数 → 截图存档;必要时 computer-use 驱动交互。真实 `claude -p` 执行受 GLM 网关 529 抖动影响,只在 example/监理脚本里跑,幂等可重试,**不作为常绿验证手段**。
4. **Done 永不自动,破坏性永不自动**(产品铁律)。run 成功只推 InReview;InReview→Done 必须来自显式 `TransitionIssue` 命令(状态机 `can_transition_to` 守卫锁死,E2E 读回 `settled_at` 抽查)。
5. **schema 迁移双守卫**(踩过的真坑):`CREATE TABLE IF NOT EXISTS` 对存量表**不会**加新列 —— 每加一列必须同时改 `schema.sql` 并在 `sqlite.rs` 加 `add_column_if_missing(...)`,否则存量 DB 直接崩。
6. **代码质量靠 `/code-review`,不靠测试基线**。每件功能实现后过 `/code-review`;产品铁律(Done 永不自动、settle-once、Signal derive-only、状态机合法转移表)由类型/守卫在编译期守住,E2E 读回抽查。UI(Dioxus 组件)编译过即可,行为在 bw-app 命令层 + E2E 兜底 —— 如实,不假装 UI 测试。
7. **留白如实标注**。未建的功能(Squad/多视图/Gantt 等)在文档里写"未建,留口不假装",占位 UI 不放模拟数据。

**产品铁律(原由出口闸门测试锁死,2026-07-17 起改由 类型/守卫/`/code-review`/E2E 读回 共同守住)**:

| 铁律 | 怎么守 |
|---|---|
| 持久化==独立重算 | 杀进程重开,`recompute_signals` 由 store 重算;E2E 重启后 sqlite 读回一致 |
| Signal derive-only、无数据=Unknown | `Derived<Signal>` 封口、store 无 `set_signal`;E2E 读回 signal |
| Done 永不自动、settle-once 记账 | 状态机 `can_transition_to` 守卫、Done 入边仅 InReview;E2E 读回 `settled_at` |
| Issue/run/产物归属与记账 | store 读回核验,绝不硬编;蒸馏/注入/uses 复利链 E2E 读回 |
| schema 迁移不崩老库 | `schema.sql` + `add_column_if_missing` 双守卫;开老库 `PRAGMA` 读回新列 |
| cron 真实调度 tick、自动建单 no-hijack | E2E:到点 tick 后 sqlite 读回新建 Issue,状态 Normal |

## 文档与协作约定

- **设计唯一事实源**:`plan/06-overall-alignment.md`(缺口台账 G1-G11/R1-R4 + 执行队列);**产品命题**:`plan/07-product-proposition.md`(引子页命题原文 + 用户语言拆解 + 工程对照表——命题正文用人话,源码/测试锚点只进对照表);**MVP 执行计划**:`plan/08-mvp-execution-plan.md`(MVP=项目的生命周期 × workflow 的生命周期,两线四棒 P/W 队列,目标用人话、锚点在各件「工程对照」——当前接棒入口)。`DEVELOPMENT.md` 是开发指南;`plan/00~05` 是路线与选型背景;`iterations/HANDOFF-*.md` 是棒次交接件(自足,不读对话也能接手)。
- **commit 约定**:每件独立 commit,代号前缀(如 `A5-F · 转移守卫…`),信息如实描述取舍,不吹。交接件与实况冲突时**以源码为准,如实记录偏差,不擅改设计决定**;拿不准的写进 commit message 的「偏差」段留给下一棒。
- 设计系统 token(暖纸底色 `#EFEBE2`、clay 主色 `#C5654A`、三态信号色+Unknown 灰、Noto Serif/Sans SC + JetBrains Mono)见 `plan/00-PLAN.md` §6;绿色隐身、只有红黄出声。
