# BACKLOG · 缓做的冗余功能与后续 issue 清单

> **30 秒导读**:2026-08-17 减负重构会话(设计稿 `docs/superpowers/specs/2026-08-17-debt-reduction-refactor-design.md`)把仓里的负债分成三堆:**死代码**(已删)、**冗余功能**(还能从界面走到、但与主环重复或没做完——本文)、**结构债**(大文件拆分——已做一部分,剩下的也在本文)。用户原话:「冗余功能可以作为后续演进的 issue,但滞后处理」。这里就是那份 issue 清单:每条写清**它是什么、为什么现在不动、动的时候从哪下手**。给下一个接手的会话看。**现在作数**;每条做掉后在行首打 ✅ 并写 commit,不删行。
>
> 词表见 `CONTEXT.md`;代号见 `docs/code-schemes.md`。本文不新开代号,按序号引用即可(「BACKLOG 第 3 条」)。

## A. 冗余功能(界面可达,但与主环重复;退役需要产品拍板)

主环 = 项目墙 → 建/接项目 → Issue 看板 → ▶跑(内嵌终端里的真实 `claude`)→ 评审 → 人点完成 → 蒸馏成技能。下面这些是主环之外**还能走到**的执行路径。

| # | 项 | 现状(读回自源码) | 为什么现在不动 | 动的时候 |
|---|---|---|---|---|
| 1 | **旧聊天式执行引擎退役** | `Command::RunWorkflow` / `RunHubWorkflow`(`crates/bw-app/src/lib.rs`)→ `Engine` + `MockExecutor`/`ClaudeCliExecutor` 按 phase 循环 → 写 `session`/`message` 表 → `op.rs::WorkflowPanel` 的 `Chat` 视图。界面入口:WorkflowHub「⚡ 临时任务」(`workflow_hub.rs:986`)、「确认导入」(`workflow_hub.rs:453`),CronHub「▶ 立即执行」与到点自动触发(`main.rs:341`)。`RunStagePlaybook` 只剩 `real_demo` 指挥器在用 | 牵连 `Engine`/`session`/`message` 表/`RunIssue` 的无仓回退路径(`workspace_path` 为空时走 `mock_engine`),一次自主会话连根拔风险过高;且它是 Cron「运行工作流」模式的执行体,拔掉要先给 CronMode::RunWorkflow 定新语义 | 先决定 CronMode::RunWorkflow 的去留 → 把「⚡ 临时任务」改成建一张 Issue 再 ▶跑 → 删 `Chat` 视图 → 删 `Engine` 与 `session`/`message`(schema 迁移:表不删只停写,老库读回不崩)→ `real_demo` 改走 `RunIssue` |
| 2 | **Autopilot 建活无界面** | `Command::CreateAutopilotTask`(`lib.rs:662`)有 handler、有到点自动建 Issue 的调度器分支(产品铁律「定时任务只自动建活,绝不自动完成」),但**没有任何派发点**——`cron_hub.rs` 的 `CronModeChoice` 只有 RunWorkflow/RunSkill/RunPrompt 三型,建不出 `CronMode::CreateIssue` | 是产品命题的一部分(第 2 点),不是冗余;缺的是表单。留着命令是为了不把这条铁律的执行体一起删掉 | CronHub 表单加第四型「建活(不自动跑)」,派发 `CreateAutopilotTask`;E2E:到点 tick 后 sqlite 读回新 Issue 状态 Normal |
| 3 | **技能/队友批量导入无界面** | 灌库唯一路径是三个 headless 例子:`import_skill_library` / `import_skill_package` / `import_ecc_agents`(`crates/bw-app/examples/`);对应 `Command::ImportSkillLibrary`/`ImportSkillPackage`/`ImportAgentDefinition` 在 app-desktop 里不可达 | 用户日常靠例子灌库能用;做界面是新功能不是减负 | SkillHub/AgentHub 加「从目录导入」;或明确「导入永远是命令行动作」并把三个例子写进 DEVELOPMENT.md(现已写) |
| 4 | **ConnectorHub / KnowledgeHub 降级** | `connector_hub.rs` 文件头自述「no actual live sync mechanism yet」,`SyncConnector` 只重读 `.bw/connectors.toml`;`knowledge_hub.rs` 是纯登记单栏列表,无行内动作 | 都是「登记」而非「工作」,但连接器登记被创建流的 GitHub/CodeHub 探针用到;删屏幕会断创建流 | 把 ConnectorHub 收成 SettingsHub 的一节;KnowledgeHub 直接下线(无任何读者) |
| 5 | **Routine 面板并入 Progress** | `op.rs::RoutineAll`/`RoutineStage` 是只读的观测流(observation 表倒序),与 `ProgressAll` 的健康灯/指标卡同源 | 纯展示,不影响主环 | 在 Progress 面板底部加「最近观测」折叠区,删 Routine 面板与 `Panel::Routine` |
| 6 | **Artifact + Version 面板合并** | `op.rs::ArtifactPanel`(产物扫描)与 `VersionPanel`(git log)都是工作区只读视图 | 同上 | 合成一个「工作区」面板:上半 git log,下半产物 |
| 7 | **ProgressAll 减法** | `op.rs::ProgressAll` 是五阶段 × 健康灯 × 指标卡 × 周复盘的总览,`docs/v1-prototype/issue3-overview-refactor.md` 已重构过一轮 | 伙伴 V1 刚改过,再改要与 issue3 设计对齐 | 按 issue3 的「关口收件箱」思路继续减,不新增卡片 |
| 8 | **`e2e/flows/core/02-issue-run-to-review.toml` 考卷可能过时** | plan/15 的验收流按「▶跑 → 旧执行引擎 → 评审中」写;2026-08 起 ▶跑 走内嵌终端(PTY),`run-flow.py` 的 DOM 点击/断言链未在新路径上重跑过 | 验收流本就「不作为常绿手段」;重跑需要真 `claude`(信任对话框/网关) | 用 `BW_FLOW` 在 mock 执行器上重跑一遍五张考卷,过时的重写 |

## B. 结构债(不改行为的机械重构;拆完门禁绿即验收)

| # | 项 | 现状 | 动的时候 |
|---|---|---|---|
| 9 | `crates/bw-app/src/lib.rs` 继续拆 | 2026-08-17 拆分切片把 `impl App` 按职责拆成子模块(见该 commit);`dispatch()` 那个 3,000+ 行的 `match` 仍是一个函数 | 每个 `Command::X =>` 臂提成 `fn handle_x(&mut self, …)`,`dispatch` 只剩路由;逐臂做,每臂一 commit |
| 10 | `crates/bw-store/src/sqlite.rs` 按域拆 | 3,895 行单文件:schema/迁移 + 项目/Issue/技能/队友/工作流/Cron/连接器/观测/记账 全在一起 | 按表族拆成 `sqlite/{schema,project,issue,skill,agent,workflow,cron,connector,observation,ledger}.rs`,`impl Store for SqliteStore` 分散多文件;`SELECT` 列表去重(同一表的列清单出现在多处 query 里,改一列要改 N 处——先抽成 `const COLS_ISSUE: &str`) |
| 11 | `crates/app-desktop/src/screens/op.rs` 继续拆 | 2026-08-17 拆出 Issue 看板/详情与内嵌终端;剩下六个面板仍在一个文件 | 每个面板一文件;`ProgressStageLegacy` 已改名(它是四个阶段的现役渲染器,不是遗留) |
| 12 | 字体打包 | `theme.rs` 字体栈依赖系统装的中文字体(Songti/PingFang/雅黑),未打包 Noto Serif/Sans SC + JetBrains Mono(设计 token 见 `docs/archive/plan/00-PLAN.md` §6) | 打包进 app bundle,或明确「依赖系统字体」写进 README |
| 13 | `hook_listener::uninstall_hooks_config` 未接线 | `crates/bw-app/src/hook_listener.rs:406` 有实现,只有测试在调;应用退出/卸载时不清 `~/.claude/settings.json` 里的 hook 条目 | 桌面壳退出钩子里调一次;或 SettingsHub 加「移除 hook」按钮 |
| 14 | 内嵌终端 Windows 后端未真机验证 | `crates/bw-engine/src/pty_backend.rs::windows` 从原函数体搬入(改动四处:读 `binary` 参数、`env_clear()`、写线程、`is_finished()` 收尾,模块文档逐条列了),只经 `cargo check --target x86_64-pc-windows-gnu` 交叉编译核对;开发机是 macOS | 有 Windows 机器时跑 `cargo run -p bw-engine --example pty_smoke`(需把 `bash -c` 换成 `cmd /C echo pty-ok`) |
| 15 | 内嵌终端首启自动提交是启发式 | `pty_backend.rs` 两平台都在首启 2000ms 后发 `\r` 提交位置 prompt(claude 交互式 TUI 不自动提交),不是就绪侦测 | 改成侦测 TUI 就绪信号(比如读到输入框提示符字节)再发;或让 claude 自己提交(`--print` 之外的官方途径) |
| 16 | 内联单元测试的定位 | 约 1,960 行内联测试(伙伴 V1/V2 引入),CI `cargo test` 在跑;CLAUDE.md 2026-07-17 曾写「不再写/留单元测试」 | 已在 CLAUDE.md 改成如实表述:不要求写、现存的随 CI 跑、改到就顺手维护、不建回归大坝。这条留作提醒,无需再动 |
| 17 | PTY 运行的 `completed` 与队友战绩记账太粗 | `pty_backend.rs` 两平台都在子进程退出(读到 EOF **或读错误**)后返回 `completed: true`,退出码不看;bw-app 结算把它当 `run_ok` 记进 `record_agent_run`(胜场 +1)。于是一次 I/O 断掉或 claude 非零退出也算队友「赢了一场」——「队友胜率由真实战绩算出」这条承诺在这条路径上是粗粒度的(2026-08-17 `/code-review` 抓出,评审中/完成的判定**不受影响**:评审中由 PR 轮询推导) | 收尾时把 `wait()` 拿到的退出码带回 `SkillOutput`(非零 → `completed: false`);读错误与 EOF 分开记;`record_agent_run` 只在真正 `completed` 时计胜 |

## C. 已删(不是 backlog,是收据;找不到东西时先看这里)

2026-08-17 减负重构会话删掉的东西,git 历史可找回(`git log --diff-filter=D --summary`):

- 死代码:`bw_engine::github::checkout_issue_branch`;`Command::SyncProjectFile`/`RefreshIssues`/`RunDraftWorkflow`/`UpdateWeekPlan`/`RefreshHubs`/`AnnotateWeeklyReview`/`MigrateLegacyShellsIfNeeded` 及其 handler;`Event::WeeklyReviewAnnotated`/`LegacyShellsMigrated`;`weekly_review` 表(无读者);`bw_core::model::drafting_workflow`;`run_workflow_inner` 的 `force_mock` 参数。
- 一次性存量迁移 `bw-app/src/legacy_migration.rs` 全链(真实日常库 `app_meta` 四个 done 标记全在,新库本就 no-op)。
- `crates/bw-app/examples/` 41 → 12 个:删掉的 29 个是历史批次的一次性验证脚本(是「已发货 commit 的收据」,不是回归守卫)。保留清单见 `DEVELOPMENT.md`。
- `crates/app-web/`(0 行代码的占位 README);Cargo.toml 注释保留一句「Web 版=以后也许」。
- 文档:没有删任何文档,只搬进 `docs/archive/`(见 `docs/archive/README.md`)。
