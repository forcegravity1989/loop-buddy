# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> **看不懂词先查这两处**:领域词(队友、交棒、观测、蒸馏……)与工程操作词(读回、门禁、记账……)见 `CONTEXT.md` 词表;字母数字代号(P2、W6、R1……)见 `docs/code-schemes.md` 代号索引。写任何给人看的东西之前,先读下方「写作纪律」。**找文档先看 `docs/README.md`(全仓文档地图);现在在做什么看 `docs/v1-prototype/` → `v2-prototype/` → `v3-prototype/`;缓做的冗余功能看 `docs/BACKLOG.md`。**

## 这个仓库在做什么

**Builders' Workbench(BW)**:单人构建者的 Rust 原生桌面工作台(Dioxus 0.7 / wry WebView,macOS+Windows)。仓库真名 loop-buddy;产品里的 AI 队友/程序自称 buddy——三个名字是一件东西。

**产品命题**(原型引子页原文,完整拆解见 `plan/07-product-proposition.md`):

> **用 AI 时代的方式,一步步把一个项目的管理体系搭起来。** 走完,你拥有一套**可复制的项目管理方法,而不只是一块看板**。

针对的痛:传统项目管理要 5 个专职角色、10 道流程,信息靠开会和口头汇报流动。Builders 模式换成 **1 个 Builder + Agent Loop**:PRD+评审 → 原型即规格;甘特图 → 每周可验证增量;人工实现 → agent 产出 80%、人审 20%;状态周会 → 真实 telemetry,难造假。四个控制点(产品哲学,自始未变):**知道对标谁 / 每周在正常演进 / 让 agent loop 干活(人只守质量门与验收)/ 目标清晰且难造假**。

落到今天的实现,是四件互锁的事:

1. **管理体系自带,不用用户发明**。项目分五个阶段:原型 → 构建 → 优化 → 运营推广 → 运维。每个阶段自带打法——该问什么、什么节奏、做到什么算完(DoD 清单)、常见的坑;这套方法论是内置的(代码里是 `StageKind` 静态元数据),不随项目现编。运维阶段的复盘回流到原型阶段,所以项目是环、不是流水线。从一个阶段推进到下一个阶段叫「交棒」:清单没勾完也允许交,但会强制标成「带险交棒」并永久记下当时缺了什么——这类记录只追加、不修改(append-only),事后抹不掉。
2. **活让 agent 干,人守验收门**。一件活就是一张 Issue 卡,指派给 AI 队友后一键真实开工(`RunIssue` 命令,在项目的真实目录里改文件、跑测试)。队友干完,活最远只能走到「评审中」;**「完成」永远由人显式点**——状态机里 Done 的唯一入口就是「评审中」,由 `can_transition_to` 在代码层面锁死。干砸了就如实停在原地,可以重试,不假装前进。定时任务只会自动**建**活(Autopilot 模式),绝不自动**完成**活。
3. **健康难造假**。健康信号灯只能被真实数据点亮:数据点(观测)只追加、不修改;信号只能从数据推导出来,不能手动设置(`Derived<Signal>` 类型是密封的,store 层根本没有 set_signal 方法)。**没有数据就显示 Unknown 灰,绝不假装绿**;数据过期自动降级;手工填的数据带「手填」徽记。干活过程自动留痕:队友战绩、产物登记、每次运行的成败与耗时、阶段吞吐指标,全部自动入账,且同一件活绝不重复记账(代码里这条约束叫 settle-once)。界面上任何数字都能用 `sqlite3` 直接查库核对。
4. **经验复利,越用越强**。做完的 Issue 可以一键「蒸馏」成一篇带正文的技能(永远记着它来自哪件活);下次干同类活时自动注入给队友,用一次记一次。队友胜率由真实战绩算出,绝不手工设定。

**反命题(防蔓延)**:不是团队协作平台(没有成员/群聊/收件箱)、不是通用看板(无拖拽/甘特;回退不给 UI)、不是审批系统(交棒只留痕不拦人)、不是云服务(AI 执行=本机 `claude` CLI,在内嵌终端里全程可见、可中止,花费由用户自己把握);永远不替用户捏造健康。

## 写作纪律(给人看的东西,先让人看得懂)

管辖范围:**一切写给人读的输出**——对话里的进度汇报、commit message、plan/ 文档、交接记录。背景:用户已四次反馈「输出太黑话、审核很费劲」(2026-07-16 / 07-20 / 07-22 / 08-05),这一节就是那几次反馈的固化。

1. **汇报用人话**。给用户的进度汇报和总结,用完整句子说清「做了什么、结果如何、下一步是什么」。不要甩裸代号链(「T1→T2→T5,剩 T3/T4」这种);代号可以出现,但**每个代号在当次输出里第一次出现时必须带一句人话说明**,例:「T3(把工作流正文接进详情页)还在做」。
2. **实现术语留在代码里**。settle-once、no-hijack、derive-only、`Derived<Signal>` 这类实现机制词,只出现在代码、代码注释和工程对照表里;写给人的正文改说人话:「同一件活绝不记两次」「定时任务绝不自动完成活」「信号只能从数据推导」。这是 2026-07-22 术语沉淀时已定的规矩(`docs/adr/0001-ubiquitous-language.md`),此处重申并扩展到对话汇报。
3. **用词以 `CONTEXT.md` 词表为准**。要新造一个词,先进词表(给出定义和 _Avoid_)再使用;拿不准某个词读者懂不懂,就查词表——词表里没有,多半说明该换成人话。
4. **代号先登记再使用**。要新开一批任务代号(比如下一批活想编成 X1-X5)之前,先查 `docs/code-schemes.md`:该字母已被占用就换一个,然后把新系列登记进去再用。历史上 P/S/W/R/L/A 六个字母都发生过「同字母、不同批次、含义完全不同」的撞车,根因就是没有登记表。
5. **每篇新文档开头给 30 秒导读**:这文档是什么、给谁看、现在还作数吗。文档过时后不删,但必须在顶部加横幅注明「历史档案,现状以 XX 为准」——plan/00-05 和 iterations/ 的历史交接记录都已照此标注,新文档过时时照做。
6. **写作范本**:`docs/buddy/standards/metrics.md`(零裸代号、术语随用随定义、有真实样例)和 `plan/07-product-proposition.md`(命题正文用人话、工程锚点单独进对照表)。写新文档前先看它们长什么样。

## 改代码的原则:不为向后兼容留旧路径

发现过时的实现路径,直接移除它,而不是加兼容层、回退逻辑或迁移流程去迁就它。这条对本仓库的所有代码改动都成立,不只是本次迁移。

## 常用命令

```bash
cargo check -p bw-app             # 日常:编译内核+应用(不编 Dioxus,快)
cargo run -p app-desktop          # 启动桌面应用(见下方环境变量)
# E2E 验证(核心纪律,行为正确性的主要手段):
BW_DB=<db> BW_OPEN=<项目名> BW_PANEL=<panel> ./target/debug/builders-workbench  # 深链启动(用环境变量直接跳到指定项目/面板),stderr [BW_BOOT]/[BW_OPEN] = 启动/渲染证明
sqlite3 <db> "SELECT …"           # 数字一律 SQL 读回(「读回」=把数字从数据库重新查出来核对,比截图更硬)
cargo run -p bw-engine --example pty_smoke [-- --teardown|--abort]   # 内嵌终端 PTY 后端在本机能起子进程/读回/收尾/被中止后不留孤儿(不碰 claude、不碰网关)
```

**门禁(每个 commit 前全过,与 CI 完全一致;「门禁」=提交前必须全部通过的检查组)**:

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop        # 桌面壳编译过
cargo test --workspace --exclude app-desktop   # CI 也跑:现存内联测试必须过(纪律见下「核心纪律」第 6 条)
# 行为正确性靠 E2E(深链启动 + sqlite 读回 + computer-use)+ /code-review;单元测试不是交付物
```

**headless 主环指挥器**(「指挥器」=不开界面、直接驱动内核走完完整生命周期的脚本;2026-08-18 起走的就是产品主环:建活 → 指派 → ▶跑(mock 交互执行器,项目无真实工作区)→ 代人点完成 → 蒸馏成技能 → 交棒;不碰 claude、不碰网关,重复跑不产生重复数据):

```bash
cargo run -p bw-app --example real_demo -- <db-path> <workspaces-root> [--only <slug>]
```

**环境变量**:`BW_DB`(覆盖数据库路径)· `BW_OPEN=<项目名>` + `BW_PANEL=progress|workflow|routine|artifact|version|issues`(启动深链,stderr 打 `[BW_OPEN]` 日志,是桌面渲染的可靠证明)· `BW_HUB=skill|agent|workflow|cron|connector|knowledge|activity|notify|settings` / `BW_SEL=<kind>:<uuid>`(深链到 Hub / 组件详情)· `BW_WORKSPACES` · `BW_CLAUDE_BIN`(覆盖 `claude` 二进制路径)· `BW_FLOW=<command-file>`(进程内点击/断言脚本,验收流用)。

## 架构(crate 一览与数据流)

```
bw-core     领域内核:StageKind 五阶段元数据 / Issue 状态机与合法转移表 / 度量派生链类型
            (零 IO 零 UI,必须 wasm32 可编译;默认无 idgen 特性)
bw-engine   InteractiveExecutor trait:InteractiveCliExecutor(交互式 `claude`;内嵌终端经
            pty_backend.rs:Windows conpty-oxide / macOS·Linux portable-pty)+ MockInteractiveExecutor
            (无工作区时的自标注替身)+ workspace.rs(项目仓/issue worktree 供给)+ evidence.rs
            (从工作区采集 git/docs/测试真状态回流观测)+ github/codehub/metrics_file/connectors_file
            (2026-07 那条 `claude -p` 按阶段循环的旧引擎 Engine/Executor/MockExecutor/
            ClaudeCliExecutor 已于 2026-08-18 整链删除)
bw-store    SQLite(sqlx):schema.sql + add_column_if_missing 迁移守卫;handoff/observation 等
            只追加(append-only)表;store 无业务判断(哑存储)
bw-app      编排大脑:App + Command/Event 总线,所有用例与守卫都在这层;E2E 的命令层主战场
ui          纯函数 selector + ViewModel(state→可渲染 DTO),可单测/E2E 核验
app-desktop 真壳(Dioxus 0.7 hard-pin =0.7.9):kernel 桥(独立 tokio 线程)+ 各屏
(Web 版="以后也许":wasm32 keepalive + Store trait 留着门,仓里没有 app-web crate)
```

数据流:UI 只发 `Command`、收 `Event`;`bw-app` 执行用例 → store 写入数据库 → `recompute_signals` 重算 → 事件流回 UI。**唯一的干活入口是 Issue 的 ▶跑**(`Command::RunIssue`):项目配了真实工作区就在 issue 自己的 git worktree 里起交互式 `claude`(内嵌终端 PTY),没配就落到 MockInteractiveExecutor(产出自我标注为演示);每次运行都写一行 `workflow_run`(开工/结清/成败/耗时/前后 git head)绑到这张 Issue。

**两条不可妥协(已钉进类型与 CI)**:

1. **UI 无关内核**:五个内核 crate 禁依赖 dioxus/tauri/wry/leptos(`guard-kernel-ui-free.sh` 强制);wasm32 check 保住将来出 Web 版的可能性。UI 相关改动只准进 `app-desktop`。
2. **健康永远推导**:`Signal` 只能经密封的 `Derived<Signal>` 进缓存,store 无 `set_signal`,`recompute_signals` 是唯一写入者。观测只追加,一个观测=一个点,绝不插值;**无数据 = Unknown ≠ 绿**。

## 核心纪律:一切实跑(验证你做的东西是"真"的)

这个仓库最大的风险不是编译不过,而是**做出徒有其形的东西**:面板渲染了但数字是编的、流程走通了但记账没发生。以下纪律定义了本仓库里"真实"的操作含义。**2026-07-17 起核心纪律转向:行为正确性靠 E2E(computer-use:深链启动 + screencapture + sqlite 读回)+ `/code-review` 把质量;产品铁律由类型与守卫在编译期守住,E2E 读回抽查。单元测试不是交付物(如实表述见第 6 条)。**

1. **报告不代答,读回为证**。任何"已完成/数字是 X"的陈述必须能从 DB 或工作区独立复核:
   ```bash
   sqlite3 <db> "PRAGMA table_info(issue);"                           # 结构核验(演示库先用 real_demo 指挥器生成)
   sqlite3 <db> "SELECT ... "                                          # 数字一律 SQL 读回
   BW_OPEN=<项目名> BW_PANEL=issues target/debug/builders-workbench   # 深链 stderr 日志 = 渲染证明
   ```
   演示/报告里的每个数字都从真实 DB 读出,绝不硬编码(`real_demo` 的 evidence JSON 模式)。
2. **mock 必须自我标注**。MockInteractiveExecutor 路径的产出带【mock】/「流程演示」字样,文档如实注明;mock 存在的唯一目的是廉价验证管线本身,绝不冒充真实执行。
3. **E2E 验证绝不依赖网关**。验证动作 = 临时/演示 DB + 深链启动到目标面板(stderr 见 `[BW_OPEN]` 即渲染成功、无 panic)→ `sqlite3` 读回核数 → 截图存档;必要时 computer-use 驱动交互。真跑 `claude`(内嵌终端)受信任对话框与网关抖动影响,**不作为常绿验证手段**;`real_demo` 指挥器只走 mock 交互执行器。

   **内嵌终端在 macOS 上能跑(2026-08-17 起)**:▶跑 走的 `run_skill_pty` 在所有平台都有 PTY 后端(`bw-engine/src/pty_backend.rs`),不再是 Windows 专属;不碰 claude 的读回证据是 `cargo run -p bw-engine --example pty_smoke`(起 `bash -c 'echo pty-ok'` 读回)、`-- --teardown`(丢输入端后进程组被连坐)与 `-- --abort`(`abort()` 丢弃 future 后子进程照样收尾——App 的「中止」走的就是这条)。真跑 `claude` 仍受信任对话框/网关影响,不作为门禁。

   **computer-use 摸桌面应用(2026-07-30 踩出来的坑,别重踩)**:`~/Applications/BWDev.app`(bundle id `dev.buildersworkbench.bwdev`)是长期稳定的验证壳,任何 worktree 跑一次 `./scripts/point-bwdev-here.sh`(编译 + 把最新二进制拷进这个 app)即可接上,不需要新建/重注册 app——computer-use 的 `request_access` 认的"已安装应用"名单在同一次会话里现造的新 app 认不出来,必须提前存在。**screenshot 真实可用,click/key 永久受阻**(两种打包方式——exec 转符号链接、直接拷二进制进 bundle——都测过,结果一样,是 Dioxus/wry 这层更深的窗口限制,不是权限或封装问题):验证手段因此是 `BW_HUB=<hub>` / `BW_SEL=skill:<uuid>` 等 env 深链**直接终端调用** `Contents/MacOS/bwdev-launcher`(不要用 `open -a`,env 传不进去)把目标视图摆到位,再截图,不要指望点击导航。另外,agent 自身的 `screencapture`/`Read` 拿不到真实桌面像素(sandbox 只看得到壁纸)——真实证据只能靠 computer-use 自己截图当场看,或者把上面那条命令原样给用户,让用户在自己屏幕上核验。
4. **Done 永不自动,破坏性永不自动**(产品铁律)。run 成功只推「评审中」;「评审中」→「完成」必须来自显式 `TransitionIssue` 命令(状态机 `can_transition_to` 守卫锁死,E2E 读回 `settled_at` 抽查)。
5. **schema 迁移双守卫**(踩过的真坑):`CREATE TABLE IF NOT EXISTS` 对存量表**不会**加新列 —— 每加一列必须同时改 `schema.sql` 并在 `sqlite.rs` 加 `add_column_if_missing(...)`,否则存量 DB 直接崩。
6. **代码质量靠 `/code-review`,不靠测试基线**。每件功能实现后过 `/code-review`;产品铁律由类型/守卫在编译期守住,E2E 读回抽查。UI(Dioxus 组件)编译过即可,行为在 bw-app 命令层 + E2E 兜底 —— 如实,不假装 UI 测试。**关于内联单元测试(2026-08-17 如实表述,取代此前「不再写/留单元测试」的说法)**:仓里现存约 2,000 行内联测试(伙伴 V1/V2 引入),CI 的 `cargo test` 在跑,它们必须过;纪律是不要求写、现存的随 CI 跑、改到就顺手维护、不建回归大坝——别把「补测试」当交付物,也别删掉在跑的。
7. **留白如实标注**。未建的功能(Squad/多视图/Gantt 等)在文档里写"未建,不假装有",占位 UI 不放模拟数据。

**产品铁律**(「铁律」=产品行为的不可违反约束;原由出口闸门测试锁死,2026-07-17 起改由类型/守卫/`/code-review`/E2E 读回共同守住):

| 铁律(人话) | 怎么守 |
|---|---|
| 杀进程重开,所有数字能从库里重算出来、前后一致 | `recompute_signals` 由 store 重算;E2E 重启后 sqlite 读回一致 |
| 信号只能从数据推导;没数据就是 Unknown,不是绿 | `Derived<Signal>` 密封、store 无 `set_signal`;E2E 读回 signal |
| 「完成」永远由人点;同一件活绝不重复记账 | 状态机 `can_transition_to` 守卫、Done 入边仅「评审中」;E2E 读回 `settled_at` |
| 每件活/运行/产物的归属和账目真实可查 | store 读回核验,绝不硬编;蒸馏/注入/使用数这条增益链 E2E 读回 |
| schema 迁移不崩老库 | `schema.sql` + `add_column_if_missing` 双守卫;开老库 `PRAGMA` 读回新列 |
| 定时任务真实到点触发;自动建的活绝不被自动推进 | E2E:到点 tick 后 sqlite 读回新建 Issue,状态 Normal |

## 文档与协作约定

- **先读什么**:`docs/README.md` 是全仓文档地图(现役 / 运行时资产 / 伙伴迭代线 / 归档)。按需要分三层:
  - **现在在做什么**:`docs/v1-prototype/`(V1 产品化)→ `docs/v2-prototype/`(V2 调度/多人)→ `docs/v3-prototype/`(V3 内嵌 Open Design),各有 README 与逐文件状态表;遗留问题唯一完整清单是 `docs/v1-prototype/LEFTOVERS.md`;缓做的冗余功能与结构债在 `docs/BACKLOG.md`。
  - **设计与命题**:`plan/README.md` 说明 plan/ 里 7 篇现役文档各管什么——`plan/06-overall-alignment.md`(设计唯一事实源,含「缺口台账」=持续追加的问题与任务登记表,G1-G11/R1-R4 编号)、`plan/07-product-proposition.md`(产品命题:引子页原文 + 用户语言拆解 + 工程对照表)、`plan/08-mvp-execution-plan.md`(MVP 定义=项目的生命周期 × workflow 的生命周期;其执行队列已被 docs/v1~v3-prototype 接管,顶部有注)、`plan/13`(GitHub 为正本的创建流拍板)、`plan/15`(验收流工具链)、`plan/16`(技能规范)、`plan/20`(资产三层隔离规则)。
  - **运行时资产**:`docs/buddy/`(系统提示词、`.bw/*.toml` 格式规范)与 `docs/skills/`(自带技能包)被 `include_str!` 编进二进制——改它就是改产品行为,不要搬。
- **历史档案**:统一在 `docs/archive/`(规则见其 README):`plan/00~05` 路线与选型背景(七控制点模型、双团队分工等前提已被 06-08 取代)、`plan/09-12,14,17-19,21` 做完即历史的执行批次、`iterations/` 交接记录与 aihot 践行日志、`design/` Rust 重写前的 HTML 原型稿、`verification/` 2026-07 的演示报告。**编号语义保留**:源码注释里的 `plan/09 §2` 去 `docs/archive/plan/09-…` 找。顶部均有横幅,读时别当现状。`DEVELOPMENT.md` 是开发指南(工作区布局、门禁、headless 例子清单、验证方式)。
- **commit 约定**:每件独立 commit。**commit 标题必须让不查文档的人看懂做了什么**——可以带代号前缀(如 `plan20-W6 · E2E 读回指挥器`),但代号之外必须有人话描述,且代号系列须先在 `docs/code-schemes.md` 登记(防同字母撞车)。信息如实描述取舍,不吹。交接件与实况冲突时**以源码为准,如实记录偏差,不擅改设计决定**;拿不准的写进 commit message 的「偏差」段,留给下一个接手的会话。
- 设计系统 token(暖纸底色 `#EFEBE2`、clay 主色 `#C5654A`、三态信号色+Unknown 灰、Noto Serif/Sans SC + JetBrains Mono)见 `docs/archive/plan/00-PLAN.md` §6;绿色隐身、只有红黄出声。
