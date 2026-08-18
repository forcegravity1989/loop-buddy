# 2026-08-17 · 减负重构设计(删冗余 · 拆大文件 · 理文档 · 重排目录)

> **30 秒导读**:这篇是 2026-08-17 一次自主会话的设计稿:用户定的目标是「删冗余代码、做重构、把仓库负债降下来;冗余功能转成后续 issue 滞后处理;核心是做出一个**可用的工作台**(执行态、工作态,不是结果态);同时清理冗余文档、刷新文档、重构目录」。会话是无人值守模式,所有取舍都写在这里,**假设写明,方便事后推翻**。给两类人看:回来审核这次改动的用户,和下一个接手的会话。**现在作数**(执行结果见文末「执行实录」)。

## 0. 三条假设(自主会话替用户做的判断,如不同意请推翻)

1. **对象是 `main` 上的现有工程**(`crates/` 六个 crate),不是 `claude/opc-control-plane-review-a48e9c` 分支上的 `next/` 重建工程。理由:本会话在 `main` HEAD 的 worktree 里开工;`next/` 是另一条未合入的线,不动它,只借一件器官(见 §3 终端切片)。
2. **「可用的工作台」= 这条主环真的能在用户机器(macOS)上跑通**:项目墙 → 建/接项目 → Issue 看板 → ▶跑(内嵌终端里的真实 `claude`) → 评审 → 人点完成 → 蒸馏成技能。界面清查(见 §1)证实这条链是仓里唯一的「工作态」;其余屏幕是结果态或配套。所以本次不做新功能,唯一例外是把 macOS 上缺的 PTY 后端补上——没有它,主环在这台机器上第一步就断。
3. **仍然可达的旧执行引擎(WorkflowHub「⚡临时任务」/「确认导入」、CronHub「▶立即执行」→ 非交互聊天式运行)不在本次删除范围**,登记为后续 issue 第一条(见 `docs/BACKLOG.md`)。理由:它是「冗余功能」而非「死代码」,牵连 `Engine`/`MockExecutor`/`session`/`message` 表与 `RunIssue` 的无仓回退路径,一次自主会话里连根拔掉风险过高;用户原话「冗余功能可以作为后续演进的 issue,但滞后处理」。

## 1. 依据(三份清查,原件在会话 scratchpad,结论摘要)

- **代码**:Rust 60,195 行。`bw-app/src/lib.rs` 11,934 行,其中 `dispatch()` 一个 match 3,778 行;`bw-app/examples/` 41 个文件 13,504 行,34 个是历史批次的一次性验证脚本,无任何脚本/文档引用。确认死代码:`checkout_issue_branch`(零调用)、`Command::SyncProjectFile`/`RefreshIssues`(零派发)、`RunDraftWorkflow`(唯一入口——创建流「起草」卡——已被伙伴 V1 砍掉)、`RunStagePlaybook`(只剩注释提到)。一次性迁移 `legacy_migration.rs`(293 行 + dispatch 里 438 行 handler):真实日常库 `app_meta` 四个 done 标记全在;新库本就 no-op。
- **界面**:80 个 `Command`,68 个界面可达,10 个只有 examples 派发,2 个无人派发。工作态只有 Issue 看板 + Issue 详情 + 内嵌终端 + DoD/交棒卡;`ProgressAll`/阶段进度/产物/版本/例行 六面板全是只读派生视图。**内嵌终端 `run_skill_pty` 整个挂 `#[cfg(windows)]`,macOS 上 ▶跑 直接报错**(`docs/v1-prototype/LEFTOVERS.md` V1-P1 已如实登记)。
- **文档**:326 个非源码文件。`plan/06-08` 与 `docs/v1~v3-prototype/` 是两套互不相连的导航;根目录没有 README;`plan/00-05,09-21`、`iterations/`(除 PRACTICE-buddy)、`design/`、`verification/`、`docs/superpowers/` 全部冻结在 7 月;`docs/buddy/`、`docs/skills/` 是被 `include_str!` 编进二进制的**运行时资产**,不是文档。

## 2. 留 / 删 / 缓 三张表

### 2.1 代码:删(本次)

| 项 | 依据 | 做法 |
|---|---|---|
| `bw-engine::github::checkout_issue_branch` | 零调用者 | 删函数 |
| `Command::SyncProjectFile` / `RefreshIssues` | 零派发点 | 删变体 + handler(底层 `sync_project_file_for` 仍被创建流用,留) |
| `Command::RunDraftWorkflow` + `MockExecutor` 起草锁 | 创建流起草卡已砍,只剩两个验证脚本用 | 删变体 + handler + `verify_c12/c13` |
| `Command::RunStagePlaybook` | 名存实亡 | 删变体 + handler |
| `legacy_migration.rs` + `MigrateLegacyShellsIfNeeded` + kernel 启动派发 + `sqlite::migrate_legacy_skill_stage_ref` | 一次性存量迁移,真实库已全部 done,新库 no-op;CLAUDE.md「不为向后兼容留旧路径」 | 整模块删;`Event::LegacyShellsMigrated` 一并删 |
| 34 个一次性验证/践行示例 | 无引用;是「已发货 commit 的收据」,不是回归守卫;git 历史可找回 | 删文件;随之无人派发的 `Command`(`UpdateWeekPlan`/`AnnotateWeeklyReview`/`RefreshHubs`/`CreateAutopilotTask` 等)按「零派发即删」处理,逐个核实 |
| `crates/app-web/` 空目录 | 0 文件的占位 | 删目录,Cargo.toml 注释保留一句「Web 版=以后也许」 |
| `bw-engine` 两条编译警告(`timeout` 未读、`await_child` 未用) | 编译器已指出 | 随终端切片一起处理 |

**保留的示例**(每个都有现役用途):`real_demo`(唯一指挥器)、`seed_demo`/`seed_fixture`(e2e 种子库再生)、`verify_migration`(老库迁移读回)、`audit_skills`(技能规范巡检)、`import_skill_library`/`import_skill_package`/`import_ecc_agents`(应用内无导入 UI,这是灌库的唯一路径)、`sync_metrics_files`/`render_metrics`(指标正本同步与渲染,文档引用)、`build_aihot_fixture`(样板间库再生)、`verify_stage_catalog`。

### 2.2 代码:留但重构(本次)

| 项 | 做法 |
|---|---|
| `bw-app/src/lib.rs` 11.9k 行 | 纯机械拆分为子模块(`impl App` 允许分散多文件):命令/事件枚举、状态与运行体、Issue 运行生命周期、调度器、指标采集、远端同步、旧工作流引擎、dispatch。**不改任何逻辑**,编译过 + 门禁绿即验收 |
| `app-desktop/src/screens/op.rs` 4.1k 行 | 把 Issue 看板/详情、内嵌终端拆成独立文件;`ProgressStageLegacy` 改名(它是四个阶段的现役渲染器,不是遗留) |
| 内联单元测试(约 1,960 行,伙伴 V1/V2 引入) | **保留**。CLAUDE.md 2026-07-17「不再写/留单元测试」与现状矛盾,本次改成如实表述:不要求写、现存的随 CI 跑、改到就顺手维护、不建回归大坝 |

### 2.3 代码:缓(转 `docs/BACKLOG.md`)

旧聊天式执行引擎退役、Autopilot 建活无 UI、技能/队友批量导入无 UI、ConnectorHub/KnowledgeHub 降级、Routine 面板并入 Progress、Artifact+Version 合并、ProgressAll 减法、字体打包、`sqlite.rs` 按域拆分、`dispatch` 大臂提取、`SELECT` 列表去重、`ProgressStageLegacy` 之外的命名清理、`hook_listener::uninstall_hooks_config` 接线。

### 2.4 终端切片(唯一「加」的东西)

从 `claude/opc-control-plane-review-a48e9c:next/crates/bw-engine/src/pty_backend.rs` 移植 PTY 平台接缝:Windows 分支保持 conpty-oxide 原逻辑,非 Windows 用已在依赖里的 `portable-pty`。验收:headless 例子起 `bash -c 'echo pty-ok'` 读回字节;`cargo check -p app-desktop` 过。这一步之后 macOS 上 ▶跑 才有可能真跑;真跑 `claude` 仍受信任对话框/网关影响,不作为门禁。

## 3. 文档与目录:目标形态

原则:**动冻结的,不动伙伴正在改的**(`docs/v1~v3-prototype/`、`docs/buddy/`、`docs/guide/`、`iterations/PRACTICE-buddy.md`、`CONTEXT.md`、`docs/code-schemes.md` 原地不动,避免与伙伴分支冲突);被 `include_str!` 编进二进制的 `docs/buddy/`、`docs/skills/` 原地不动,只在索引里标明「运行时资产」。

```
README.md                    新建:仓库唯一入口(是什么 · 怎么跑 · 文档地图)
CLAUDE.md / AGENTS.md / CONTEXT.md / DEVELOPMENT.md   刷新(导航补 docs/v*-prototype、单测口径、macOS 现状)
Cargo.toml / Cargo.lock / .github / .codegraph / .bw / .gitignore(补 next/、.superpowers/)
crates/                      bw-core bw-engine bw-store bw-app ui app-desktop(app-web 删)
plan/
  README.md                  重写导读:只列现役 06/07/08/13/15/16/20,其余指向 docs/archive/plan/
  06 07 08 13 15 16 20       现役(08 顶部加注:执行队列已被 docs/v1~v3-prototype 接管,§1 定义仍作数)
docs/
  README.md                  新建:文档地图(现役 / 运行时资产 / 伙伴迭代线 / 归档)
  BACKLOG.md                 新建:本次缓做的冗余功能与后续 issue 清单
  buddy/ skills/             运行时资产(不动)
  guide/ metrics/ examples/ adr/ code-schemes.md   现役(不动)
  v1-prototype/ v2-prototype/ v3-prototype/        伙伴迭代线(不动;三处 README 补前后向指针与「未 push」纠正)
  superpowers/specs/         只留 2026-07-22 入门设计、2026-08-05 五角色设计(源码/plan/13 引用)+ 本篇
  archive/
    README.md                归档规则(只加不改;顶部横幅指向现状)
    plan/                    ← plan/00-05, 09-12, 14, 17-19, 21
    iterations/              ← HANDOFF-*, V2-DESIGN, TAKEOVER-REPORT-GLM52, PRACTICE-AIHOT, AIHOT-EVIDENCE.json, evidence/
    superpowers/             ← docs/superpowers/plans/* 与已被取代的 specs
    design/                  ← design/(2026-07-15 冻结的原型稿与截图)
    verification/            ← verification/ + docs/*.png(历史演示报告与截图)
    scripts/                 ← make_demo_video.py(产物已归档)
e2e/ scripts/ examples/      不动(scripts 少一个)
iterations/                  只剩 PRACTICE-buddy.md(伙伴的实践日志,原地)
```

`plan/NN` 编号语义保留(归档后路径变 `docs/archive/plan/NN-…`,源码注释里的 `plan/NN §M` 锚点仍能按号找到,不逐条改注释)。

## 4. 执行顺序与验收

每片一个 commit,标题人话;每片过门禁(`cargo fmt --check` · `clippy --workspace --exclude app-desktop -D warnings` · wasm32 两项 · `guard-kernel-ui-free.sh` · `cargo check -p app-desktop` · `cargo test`)。

1. 死码切片(§2.1 前四行 + `checkout_issue_branch`)
2. 示例切片(删 34 个示例 + 随之孤儿化的 Command)
3. 迁移切片(删 legacy_migration 全链)
4. 终端切片(PTY 接缝移植 + macOS 后端 + 烟测例子)
5. 文档切片(§3 目录重排 + README/CLAUDE.md/DEVELOPMENT.md/plan README/docs README/BACKLOG)
6. 拆分切片(lib.rs / op.rs 机械拆分)
7. 终审:全门禁 + 新库深链启动(`[BW_OPEN]` 日志)+ `/code-review`

## 5. 执行实录(2026-08-17,回填)

分支 `claude/debt-reduction-refactor-2026-08-17`,**未 push、未开 PR**(用户没要求;下一步由用户决定)。全仓 Rust 60,195 行 → 49,555 行(−10,640,约 −18%;含评审跟进补回的约 150 行守卫/写线程/烟测);`git diff --shortstat main...HEAD`:157 files, +12,260 / −22,596。八个 commit(七个减负切片 + 一个评审跟进),每个都过全门禁(fmt / clippy -D warnings / wasm32×2 / guard-kernel-ui-free / app-desktop check / cargo test)。

| 片 | commit | 做了什么 | 读回证据 |
|---|---|---|---|
| 设计稿 | 8afe395 | 本文 | — |
| 死码切片 | 838590c | 删 `checkout_issue_branch`、`Command::SyncProjectFile`/`RefreshIssues`/`RunDraftWorkflow`、GitHub 漂移采集器、`drafting_workflow`;−931 行 | 门禁绿;grep 零引用 |
| mock 写守卫 | 1b5bca1 | 无工作区时 MockInteractiveExecutor 不再往进程 cwd 写 `.bw/metrics.toml`(`cargo test` 曾在 `crates/bw-app/` 留下脏文件) | 重跑 `cargo test` 后 `git status` 干净 |
| 示例切片 | 159359c | `crates/bw-app/examples/` 41 → 12 个 .rs(删 29 个一次性验证脚本)+ 随之孤儿化的 `UpdateWeekPlan`/`RefreshHubs`/`AnnotateWeeklyReview` 与 `weekly_review` 表(无读者);−9,682 行 | 保留清单进 DEVELOPMENT.md;删前逐个 grep 零引用 |
| 迁移切片 | 1ed62db | 删 `legacy_migration.rs` + `MigrateLegacyShellsIfNeeded` + kernel 启动派发;−843 行 | 真实日常库 `app_meta` 四个 done 标记全在(只读查询);新库本就 no-op;`[BW_MIGRATE]` 日志改为 `[BW_BOOT] skills=N agents=M` |
| 终端切片 | 748e514 | 新 `bw-engine/src/pty_backend.rs`(PtyBackend trait;Windows conpty 整段搬入;Unix portable-pty + `nix::killpg` 进程组收尾);`run_skill_pty` 全平台委托;两后端 `env_clear()` 让 `plan.env` 真成唯一环境来源;修「读循环先结束再 await JoinHandle 会 panic」;新 `examples/pty_smoke.rs`(+`--teardown`);LEFTOVERS V1-P1 追加处置段 | macOS:`pty_smoke` 读回 `pty-ok`(8 字节);`--teardown` ~700ms 返回、`pgrep` 无孙进程残留;`cargo check --target x86_64-pc-windows-gnu -p bw-engine` 过(Windows 未真机) |
| 文档切片 | 2182b2c | 新 README.md / docs/README.md / docs/BACKLOG.md / docs/archive/README.md;`plan/` 只留 7 篇,其余 + iterations(除 PRACTICE-buddy)+ design + verification + docs/*.png + superpowers/plans + make_demo_video.py → `docs/archive/`(git mv);无横幅的归档件补横幅;plan/README 重写、plan/08 加状态注;CLAUDE.md 导航三层化 + 单测口径如实化 + 门禁补 cargo test + 架构表刷新;DEVELOPMENT.md 加 headless 例子清单;AGENTS/Cargo.toml/CONTEXT.md(产品名条目)/.gitignore;三处「未 push」纠正;`crates/app-web/` 删 | 仓内 markdown 相对链接全查:现役文档零断链 |
| 拆分切片 | 9aa0cbf | `bw-app/src/lib.rs` 11,100 → 1,680 行,拆成 command/dispatch/issue_run/terminal/scheduler/metrics/project_sync/prompts/workflow_engine 九个子模块(`use super::*` + 分散 `impl App`,被搬私有方法改 `pub(crate)`);`op.rs` 4,148 → 2,805 行,拆出 `op/issues.rs`(993)与 `op/terminal_widget.rs`(363);`ProgressStageLegacy` → `ProgressStageGeneric` | 行多重集比对:旧 lib.rs 与新十文件逐行 1:1(仅 10 个签名因 `pub(crate)` 超长被 rustfmt 换行);cargo test 通过数与拆前一致 |
| 评审跟进 | 9d1c7f2 | `/code-review` 抓出的 15 条处理 13 条(下节);新 `pty_smoke -- --abort` 场景;`weekly_review` 旧表 DROP 迁移;两平台 PTY 写线程;`ChildGuard` 收尾守卫 | `--abort`:abort() 后 87ms 顶层+孙进程全消失(pgrep 读回);demo.db 副本:插 weekly_review 行 → 删项目撞外键(错误 19)→ 新代码开一次 → 表没了、项目 2 条原样;门禁绿 + Windows 交叉编译 check |

**终审(2026-08-17)**:
- 全门禁绿(含 `cargo test --workspace --exclude app-desktop`,66 个测试通过)。
- 新库深链启动:`BW_DB=<空库> BW_HUB=skill` → stderr `[BW_BOOT] skills=11 agents=5` + `[BW_HUB] "skill" -> Skill`;sqlite 读回 21 张表、skill=11、agent=5、无 `weekly_review` 表;无 panic。
- 老库(`e2e/fixtures/demo.db`,2026-07-25)深链:`BW_OPEN=linkcheck-md BW_PANEL=issues` → `[BW_OPEN] "linkcheck-md" -> view=App panel=Issues projects=2 issues=3`;`PRAGMA table_info(issue)` 读回后加的列(github_number/pr_number/standard_skill…)全在(add_column_if_missing 守卫在工作);issue 状态 todo/in_review/done 各 1 未被自动推进;`settled_at` 只有 done 那条非空。
- 拆分脚本踩过一次坑:按「fn 行到下一 fn 行」切被多行签名坑(切在参数中间),改成「见到第一个 `{` 后配对到 depth 0」+ 空隙检查后零空隙落盘;写进 commit message 供下次拆分借鉴。
- `/code-review`:见下「评审结果」——抓出 3 条真 bug,已在第八个 commit(9d1c7f2)修掉。

**评审结果(`/code-review`,xhigh:7 个发现角度 → 逐条核实 → 一轮补漏;子代理全用 sonnet)**:

15 条上报,按严重度排,前四条是真 bug,全部有读回证据;第 5、7 条如实留下;其余是文档/清理。

| # | 在哪 | 是什么(人话) | 核实 | 处置 |
|---|---|---|---|---|
| 1 | `bw-store/sqlite.rs` `delete_project` | 「周评注」表退役后老库里表还在,外键开着,删项目就撞外键 | 确认(sqlite3 复现错误 19) | 修:open() 里 `DROP TABLE IF EXISTS weekly_review`,老库真删旧表 |
| 2 | `bw-app/issue_run.rs` `cancel_run` | macOS 上「中止」用 abort() 丢弃 future,Unix 后端收尾代码跑不到,`claude` 变孤儿 | 确认(读源码 + portable-pty 源码;conpty-oxide 侧托管会话本就 kill-on-drop) | 修:`ChildGuard` 的 Drop 在独立线程按进程组收尾;`pty_smoke -- --abort` 87ms 内全消失 |
| 3 | `bw-engine/pty_backend.rs` 写键盘字节 | 同步阻塞写跑在桌面壳的 current_thread 运行时上,子进程不读 + 大段粘贴 = 整个内核卡死 | 确认(kernel.rs `new_current_thread` + portable-pty 无 O_NONBLOCK) | 修:两平台都改写线程 |
| 4 | `pty_backend.rs` 早退错误路径 | 只 kill 不 wait,留僵尸 | 确认 | 修:统一走 `ChildGuard` |
| 5 | `pty_backend.rs` 读循环 | 读错误与正常 EOF 都返回 completed:true、退出码不看,上层记成队友一场胜利 | 大概率(评审中/完成判定不受影响,由 PR 轮询推导) | **留**:BACKLOG 第 17 条,不在本次改语义 |
| 6 | `interactive_cli.rs` `run_skill` 回退路径 | Windows/Linux `tokio::process` 分支没 `env_clear()`,剥掉的嵌套会话变量漏回子进程 | 确认 | 修:补 `env_clear()` |
| 7 | `app-desktop/kernel.rs` | 一次性 legacy 迁移删除后没有手动触发口 | 确认机制(真实日常库 done 标记全在;样板库无可迁移内容) | **留**:§2 的既定决定,未迁移旧库只是 Hub 留几条空壳 |
| 8 | `terminal_manager.rs` 文档 | 还说「Unix adapter 本阶段不实现」 | 确认 | 修 |
| 9 | `github.rs` `open_pr_for_branch` 文档 | 说它为已删的 RefreshIssues 服务,下次会被当死码删掉(现役调用方是评审中轮询) | 确认(补漏轮抓出) | 修:写明现役调用链 |
| 10 | `pty_backend.rs` 收尾 | 固定睡 200ms,哪怕子进程早退了 | 确认 | 修:先 try_wait,已退零等待 |
| 11-15 | `pty_backend.rs` / `pty_smoke.rs` | 手工 `read_finished` 标志、2000ms 常量重复、`bytes_tx.clone()` 多余、RunCtx 字面量重复、模块文档漏列一处改动 | 确认 | 全修 |

被驳回的两条(没上报):「输出通道无背压会涨内存」——TerminalManager 有每会话有界环形缓冲(64 批 × 8KB,满丢最老),搬迁前就是这样;「mock 写守卫是创可贴」——空 cwd 只在无工作区且已选 mock 执行器时出现,守卫就在唯一的文件系统副作用点上,是正确深度。

评审的另一个副产品:两平台 PTY 后端的 select! 主循环长得几乎一样但**刻意不抽公共骨架**(类型完全不同,硬抽会变一堆泛型),这条取舍写进了模块文档,免得下一位又提。

**偏差(与 §2/§3 的计划相比,如实记)**:
- §2.1 说 `RunStagePlaybook` 删——**没删**:`real_demo` 指挥器还在用它驱动五阶段环;转成 BACKLOG 第 1 条的一部分。
- §2.1 说 `CreateAutopilotTask` 按「零派发即删」——**没删**:它是产品命题「定时任务只自动建活」的执行体,缺的是表单不是命令;登记为 BACKLOG 第 2 条。
- 进程组杀用 `nix`(安全封装)而不是 next 分支那样的 `libc` 裸调用:bw-engine 整 crate `#![forbid(unsafe_code)]`,不为一个 syscall 开口子。
- 示例实际删 29 个(41 → 12),不是 §1 估的 34:`sync_metrics_files`/`render_metrics`/`build_aihot_fixture`/`verify_stage_catalog`/`audit_skills` 逐个核实有现役用途,留下。
- `CONTEXT.md` 计划里说不动(伙伴在改),实际加了一条「Builders' Workbench / BW / buddy / loop-buddy 是一件东西」词条——docs/README 里用了这句,按写作纪律第 3 条得先进词表。
- 已知 flaky:bw-store `sync_connectors_file_empty_is_noop` 一次偶发失败(临时库文件名纳秒撞名),重跑即过,与本次改动无关,未动。

---

## 6. 第二轮:真删旧聊天式执行引擎(2026-08-18,用户拍板「删更能体现能力」)

**为什么第一轮没删**:第一轮把所有"界面够得着"的东西都归到「冗余功能滞后处理」,结果清扫做完了(示例/文档/归档),跑起来的程序只净减 753 行。用户看过截图后指出「删 2W+ 对使用没影响、前端几乎不变——那算什么瘦身」,并明确「重构我认可,删这个动作更能体现能力,我相信你」。这一轮只做一件事:把与主环平行的那条旧执行路径整根拔掉。

**证据(真实日常库,只读)**:定时任务只有 `collect_metrics` 1 条、`create_issue` 2 条,`run_workflow`/`run_skill`/`run_prompt` **零条**;旧引擎写的 `session`/`message` 最后一条 2026-07-28——8 月初内嵌终端主环落地后再没人碰过它。

**依赖图(sonnet 子代理逐符号核过,纠正了 BACKLOG 第 1 条两处过时说法)**:
- 「无仓项目走 `mock_engine` 回退」——**过时**:无仓项目早已走 `MockInteractiveExecutor`,`App.mock_engine` 在 Issue 路径里从未被读。
- `session` 表**不能删**:主环拿它当左栏「进行中 · 待你介入」的索引(每张 Issue ▶跑 前先 `StartSession`);能删的是 `message`(纯聊天记录,零读者)。

### 6.1 决定表

| # | 项 | 决定 | 理由 |
|---|---|---|---|
| 1 | `Engine`/`Executor` trait/`MockExecutor`/`ClaudeCliExecutor`(执行体)/`UnsupportedCliExecutor`/`contract.rs`/`PhaseNode`/`RunEvent`… | **删** | 只有旧链在用;`Engine::run_workflow` 甚至连旧链自己都没调用 |
| 2 | `ClaudeCliConfig`/`PermissionMode` | **留**(`claude_cli.rs` 精简成只有配置) | 交互式路径读 `binary`;`Command::SetClaudeConfig` 在用 |
| 3 | `crates/bw-app/src/workflow_engine.rs` 整文件 + `PreparedRun`/`LoopEnd`/`forward_progress`/`cron_prompt_workflow`/`run_params_snapshot`/`stage_workflow` | **删** | 全是旧链内部;`finalize_run` 与交互式的 `finalize_run_interactive` 是各自独立实现的兄弟,不是共用 |
| 4 | `Command::RunWorkflow`/`RunHubWorkflow`/`RunStagePlaybook`/`ParseWorkflowContent`/`SendSessionMessage`;`Event::WorkflowProgress`/`WorkflowDone`/`SessionMessageAdded`/`OptimizationCycleReported` | **删** | 派发点全在旧链的界面按钮;`ParseWorkflowContent` 解析出的 phases 只有旧引擎的阶段循环会执行,Issue 路径一次不读 |
| 5 | 定时任务 `CronMode::RunWorkflow`/`RunSkill`/`RunPrompt` | **删**;只剩 `CreateIssue`(建活)与 `CollectMetrics`(采集);CronHub 表单改成这两型(顺手关闭 BACKLOG #2「Autopilot 建活无界面」);「▶ 立即执行」删;老库 `mode IN ('run_*')` 迁移为 `create_issue` | 产品铁律「定时任务只自动建活」+ 真实库零行;"到点跑"的执行体就是旧引擎 |
| 6 | `message` 表 | **DROP TABLE 迁移** | 零读者;与 `weekly_review` 同一条规矩「不为向后兼容留旧路径」 |
| 7 | `session` 表 + `ensure_session`/`list_sessions`/`delete_session` | **留** | 主环左栏索引;换成按 Issue 键的导航是另一张票 → BACKLOG |
| 8 | `workflow_run` 表 | **留,并让交互式运行开始/结算写行** | 删了引擎后它零写入者却仍有读者(Issue 详情「运行记录」、WorkflowHub 运行数、产物归属);产品承诺「每次运行的成败与耗时自动入账」目前在交互路径上是空的——填上比删掉更对 |
| 9 | `real_demo` 指挥器 | **重写**成主环驱动器:每阶段 CreateIssue → AssignIssue → RunIssue(mock 交互执行器,无仓)→ 脚本代人 TransitionIssue Done(输出里明写「脚本代人点完成」)→ DistillIssue → 证据/DoD → HandoffStage;幂等按 Issue 标题;`--mock` 旗标取消(永远 mock;真跑只在桌面内嵌终端) | 它以前跑的是旧引擎的"剧本",连 Issue 都不建;重写后跑的才是产品真正的环 |
| 10 | `seed_demo` 例子、`scripts/supervise-real-demo.sh` | **删** | 前者价值被重写后的 `real_demo` 覆盖;后者是给真跑 `claude -p` 网关抖动用的重试监理,mock 不需要 |
| 11 | 桌面视图:op.rs `Chat`/`RunOutputs`/`RunBanner`/`PhaseTrack` 与 chat_area 分支;kernel.rs `ChatVm`/`MsgVm`/`RunVm` 与相关 `UiNote`;WorkflowHub「⚡ 临时任务」表单、「确认导入·运行」的运行半截、两处「解析正文」按钮;main.rs `pending_cron` | **删** | 全是旧链的显示面;`RunBanner` 对 Issue 运行本就从不渲染(只有 `WorkflowProgress` 会填 phases) |
| 12 | `run_optimization_cycle`/`OptimizationReport` | **删** | 零调用者(25 轮自举那次的遗物) |
| 13 | `WorkflowSpec.phases`/`LoopConfig` 数据字段与 WorkflowHub 的展示 | **留** | 是目录元数据的一部分;精简 WorkflowHub 是产品取舍 → BACKLOG |

### 6.2 切片(每片门禁绿)

S1 定时任务收敛 → S2 桌面旧视图 → S3a `real_demo` 重写 + `seed_demo` 删 → S3b bw-app 命令/引擎胶水删 + `App::new` 去 `mock_engine` + 交互式运行写 `workflow_run` → S4 bw-engine 旧执行器删 → S5 store `message` DROP → S6 文档。验收:`real_demo` 新版跑出的库深链截图(Issue 看板五阶段各一张 Done、蒸馏出的技能、交棒记录),`sqlite3` 读回,`/code-review`。

### 6.3 执行实录(2026-08-18 回填)

分支 `claude/cut-legacy-engine-2026-08-18`(基于第一轮分支尖 `54dc38c`),八个 commit,每片门禁全绿(fmt / clippy -D warnings / wasm32 bw-core+ui / guard / app-desktop / cargo test)。

| 片 | commit | 做了什么 | 读回证据 |
|---|---|---|---|
| 登记 | `d549fab` | 本节 §6.1/6.2 先登记再动手 | — |
| S1 | `0f4428f` | `CronMode` 只剩「建活」「采集」;CronHub 表单两型;老库三种旧模式 `UPDATE … SET mode='create_issue'` | fixture 副本:2 行 run_workflow → create_issue |
| S2 | `d9ed28b` | 桌面删 Chat/RunOutputs/RunBanner/PhaseTrack、ChatVm/MsgVm/RunVm、「⚡ 临时任务」/「确认导入·运行」/「解析正文」/「▶ 立即执行」 | 四次深链 `[BW_OPEN]`/`[BW_HUB]` 无 panic |
| S3a | `d2b4cbf` | `real_demo` 重写成主环指挥器;删 `seed_demo`、`supervise-real-demo.sh` | fresh DB:issue done/settled 10、handoff 10、蒸馏技能 10(uses 4/3/2/0/0);重跑幂等 |
| S3b | `5fdbcce` | 删 `workflow_engine.rs`(894 行)+ 6 命令 + 5 事件 + `PreparedRun/LoopEnd/RunOutcome/OptimizationReport/forward_progress/review_tail/run_params_snapshot/skills_prompt_block`;`App::new` 两参;**交互式运行写 `workflow_run`**(开工 record_workflow_run_start+set_run_issue,结算 Ok/Failed+heads,中止/执行器失败结 Failed,降级咨询结 Ok;resume 不开新行) | `SELECT status,count(*),sum(issue_id IS NOT NULL),sum(phases_completed) FROM workflow_run` → ok\|10\|10\|10;heads 全 NULL(mock 不编);同库重跑仍 10 行 |
| S4 | `26805f8` | bw-engine 删 `mock.rs/contract.rs/unsupported_cli.rs`、`Engine/Executor/PhaseNode/PhaseOutput/RunEvent/RunSummary`、`ClaudeCliExecutor` 执行体;`ClaudeCliConfig` 缩成 `binary`(删预算/权限旋钮 + 设置页 + `BW_CLAUDE_MAX_BUDGET_USD`);bw-core 删裁决/解析契约(~230 行)与 `analysis.rs`(546 行) | real_demo ok\|10;`BW_HUB=settings` 深链无 panic |
| S5 | `ea7800d` | store `DROP TABLE IF EXISTS message` 迁移 + 删 12 个无调用方法 + `MessageRow/Author/WorkflowRunAnalytics/WorkflowVersion` | 老 fixture:打开前 message 42 行 → 打开后 sqlite_master 无 message、cron 全 create_issue、session 10 行未动 |
| 修账 | `788b80c` | 队友战绩一件活只记一次(结算记败 / Done 记胜 / 同一身份规则 `credited_agent`) | 十件 Done:每位队友 runs=1 wins=1(改前 2/2) |
| S6 | (本 commit) | CLAUDE.md / DEVELOPMENT.md / CONTEXT.md / e2e/fixtures/README.md / BACKLOG(#1 #2 收据、新增 18-21)/ 本节 | 12 个 headless 例子逐个实跑通过;BWDev.app 深链截图:Issue 看板 5 张 Done + 定时任务自动建的 #6、技能库 21 条含蒸馏出的 `demo-linkcheck-md-prototype`(4 次使用)、设置页只剩二进制一项 |

**行数(`crates/*/src`,`git ls-tree` 逐文件 `wc -l`)**:第一轮尖 `54dc38c` 45,938 → 本轮尖 41,159,**−4,779**;其中 bw-app 12,889→11,320、bw-engine 6,232→5,479、bw-core 4,951→4,081、bw-store 5,577→5,182、app-desktop 13,730→12,614、ui 2,559→2,483。`examples/` 3,546→3,207。与 main 合并基线相比两轮合计 src 46,691→41,159(−5,532)、examples 13,659→3,207(−10,452)。

**与 §6.1 决定表的偏差(全是「删得比表多」)**:
- 第 2 行说 `ClaudeCliConfig`/`PermissionMode` **留**——实际只留了 `binary`:`max_budget_usd`/`default_mode`/`commands_mode` 只被删掉的一次性执行器消费,交互式会话按设计不设单次预算、恒 `--dangerously-skip-permissions`,留着就是设置页三个不起作用的旋钮。CLAUDE.md 反命题「单次花费封顶」随之改口为「全程可见、可中止,花费由用户把握」。**这是产品口径的变化,请用户过目**。
- 第 4 行之外补删 `PromoteWorkflow`(按钮 S2 已删,唯一其它调用方 `seed_demo` 已删)。
- 第 1/12 行之外补删 bw-core `Verdict/PhaseOutcome/verdict_contract_suffix/parse_phase_outcome/workflow_parse_contract_suffix/parse_workflow_phases`(评审门与「解析为流程图」的机器契约)与 `analysis.rs`(优化分析层)——引擎没了,它们零读者。
- 第 6 行之外补删 10 个 Store 方法(其中 `get_app_meta/set_app_meta/delete_workflow_spec/refresh_workflow_template_phases` 是本分支之前就已无人调用、第一轮漏删)。
- 表外顺手修了队友战绩双记账(`788b80c`),因为 S3a 重写指挥器后 SQL 读回第一次把它暴露出来。
- 未动:项目级 `allow_commands`(同样是死旋钮,但牵涉 project 表列删除)→ BACKLOG #20。

**/code-review**:见文末「6.4 评审」。
