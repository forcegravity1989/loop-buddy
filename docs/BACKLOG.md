# BACKLOG · 缓做的冗余功能与后续 issue 清单

> **30 秒导读**:2026-08-17 减负重构会话(设计稿 `docs/superpowers/specs/2026-08-17-debt-reduction-refactor-design.md`)把仓里的负债分成三堆:**死代码**(已删)、**冗余功能**(还能从界面走到、但与主环重复或没做完——本文)、**结构债**(大文件拆分——已做一部分,剩下的也在本文)。用户原话:「冗余功能可以作为后续演进的 issue,但滞后处理」。这里就是那份 issue 清单:每条写清**它是什么、为什么现在不动、动的时候从哪下手**。给下一个接手的会话看。**现在作数**;每条做掉后在行首打 ✅ 并写 commit,不删行。**2026-08-18 第二轮**(用户拍板「删更能体现能力」)已把第 1、2 条做掉,新增第 18-21 条。
>
> 词表见 `CONTEXT.md`;代号见 `docs/code-schemes.md`。本文不新开代号,按序号引用即可(「BACKLOG 第 3 条」)。

## A. 冗余功能(界面可达,但与主环重复;退役需要产品拍板)

主环 = 项目墙 → 建/接项目 → Issue 看板 → ▶跑(内嵌终端里的真实 `claude`)→ 评审 → 人点完成 → 蒸馏成技能。下面这些是主环之外**还能走到**的执行路径。

| # | 项 | 现状(读回自源码) | 为什么现在不动 | 动的时候 |
|---|---|---|---|---|
| ✅ 1 | **旧聊天式执行引擎退役**(2026-08-18 六片做完:`0f4428f` 定时任务收敛 → `d9ed28b` 桌面旧视图 → `d2b4cbf` real_demo 重写 → `5fdbcce` bw-app 引擎胶水 → `26805f8` bw-engine 执行器 + bw-core 契约/分析层 → `ea7800d` store message 表) | 已删:`Engine`/`Executor`/`MockExecutor`/`ClaudeCliExecutor`/`UnsupportedCliExecutor`/`contract.rs`、`workflow_engine.rs`、命令 RunWorkflow/RunHubWorkflow/RunStagePlaybook/ParseWorkflowContent/SendSessionMessage/PromoteWorkflow、事件 RunStarted/WorkflowProgress/WorkflowDone/SessionMessageAdded/OptimizationCycleReported、`Chat`/`RunOutputs`/`RunBanner`/`PhaseTrack` 视图、`message` 表(老库 DROP)、bw-core 评审裁决/流程解析契约与 `analysis.rs`。**留下的**:`session` 表(左栏「阶段记录」索引,见第 18 条)、`WorkflowSpec.phases/LoopConfig` 数据(见第 19 条)。交互式 ▶跑 现在写 `workflow_run` 行 | — | — |
| ✅ 2 | **Autopilot 建活无界面**(2026-08-18 `0f4428f`) | CronHub 表单改成两型「到点建活(不自动跑)」/「到点采集指标」,派发 `CreateAutopilotTask`;老库三种旧模式行迁移归并到「建活」(读回:fixture 2 行 run_workflow → create_issue) | — | — |
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
| 18 | `session` 表与左栏「阶段记录」索引换成 Issue 键 | 旧聊天式引擎删掉后,`session` 表只剩两个用途:左栏「阶段记录」按 `SessionId` 索引会话、`RunIssue{session,id}`/`workflow_run.session_id` 带一个几乎无意义的会话 id(`CompleteCreation`「立即开工」现场 ensure 一条)。会话正文(`message`)已删,`session` 成了只有标题的壳 | 左栏改按 Issue/conversation(`claude_conversation` 表)索引;`RunIssue` 去掉 `session` 参数;`workflow_run.session_id` 停写;`session` 表 DROP(老库迁移) |
| 19 | `WorkflowSpec.phases` / `phase_prompts` / `LoopConfig` 精简 | 没有阶段循环引擎后,这些字段只剩展示用途(WorkflowHub 卡片的阶段名、op.rs 工作流面板的方法环预览);`stage_workflow_with_playbook` 仍为每次 ▶跑 渲染整套 `phase_prompts`(没人读)。`PhaseMeta.reject_to_phase`、`LoopConfig.retries/max_iter` 已无消费者 | 把 `WorkflowSpec` 收成「名字 + 目标 + 阶段名列表 + 技能引用」;`playbook::rendered_phase_prompts` 删掉;老库列保留不动(读回可查) |
| 20 | 项目级 `allow_commands` 是死旋钮 | `project.allow_commands`(op.rs 工作区卡「允许执行命令」)只被删掉的 `ClaudeCliExecutor` 消费(`--allowedTools Bash` 与权限模式选择);交互式 ▶跑 恒 `--dangerously-skip-permissions`,这个开关现在什么都不控制 | 删 UI 开关 + `Command::SetWorkspace.allow_commands` + `ProjectRow.allow_commands`;列删除走 `drop_column_if_present`(SR4 同款) |
| 21 | bw-store 内联测试 `sync_connectors_file_empty_is_noop` 偶发失败 | `tempdb_path()` 用「进程 id + 纳秒」取名,`cargo test --workspace` 并行时三个测试撞名(2026-08-18 一次全量跑到 `assert!(connectors.is_empty())` 失败,单跑与三次重跑均过) | 改用 `tempfile::NamedTempFile` 或加原子计数器后缀 |

## C. 已删(不是 backlog,是收据;找不到东西时先看这里)

**2026-08-18 第二轮**(分支 `claude/cut-legacy-engine-2026-08-18`,设计稿 §6):

- 旧聊天式执行引擎整链(第 1 条的收据):`crates/bw-app/src/workflow_engine.rs`、`crates/bw-engine/src/{mock,contract,unsupported_cli}.rs`、`bw_engine::{Engine,Executor,PhaseNode,PhaseOutput,RunEvent,RunSummary,ClaudeCliExecutor,allowed_tools_arg,build_prompt}`、`bw_core::model::{Verdict,PhaseOutcome,verdict_contract_suffix,parse_phase_outcome,workflow_parse_contract_suffix,parse_workflow_phases,Author,WorkflowRunAnalytics,WorkflowVersion}`、`crates/bw-core/src/analysis.rs`;命令 `RunWorkflow/RunHubWorkflow/RunStagePlaybook/ParseWorkflowContent/SendSessionMessage/PromoteWorkflow`、事件 `RunStarted/WorkflowProgress/WorkflowDone/SessionMessageAdded/OptimizationCycleReported`(`WorkflowFailed` 改名 `RunFailed`);`CronMode::{RunWorkflow,RunSkill,RunPrompt}`;桌面 `Chat/RunOutputs/RunBanner/PhaseTrack` 视图、`ChatVm/MsgVm/RunVm`、WorkflowHub「⚡ 临时任务」/「确认导入(运行)」/「解析为流程图」按钮、CronHub「▶ 立即执行」;`ClaudeCliConfig` 的 `max_budget_usd/default_mode/commands_mode` 与 `PermissionMode`(设置页三个不起作用的旋钮)、`BW_CLAUDE_MAX_BUDGET_USD`;Store 方法 `append_message/session_messages/promote_workflow/record_workflow_use/refresh_workflow_template_phases/delete_workflow_spec/list_workflow_runs/list_all_workflow_runs/workflow_analytics/list_workflow_versions/get_app_meta/set_app_meta`;`message` 表(老库 `DROP TABLE IF EXISTS`);`crates/bw-app/examples/seed_demo.rs`;`scripts/supervise-real-demo.sh`。
- 顺手修的真 bug(`788b80c`):队友战绩一件活记两次(运行结算 + Done 边各记 run+win)→ 失败在结算记败、胜在人点完成记胜、同一条身份规则。
- 评审跟进(2026-08-18 `/code-review` 12 条,详见设计稿 §6.4):队友记账再收紧——只记 **被指派且真跑过**(库里有绑到这张 Issue 的 `workflow_run` 行)的队友,不再回落到阶段角色队友(此前从没跑过的活被人点完成也会给「原型师」记胜);开工后任何早退(绑 Issue 失败、执行器起不来)都把 run 行结成失败,不留永远「运行中」的行;`credit_skill_uses` 一处共用。**再删两块死路径**:CronHub 详情页「真实有效性」面板整链(`Store::cron_effectiveness`、`CronEffectiveness`、`Command::LoadCronEffectiveness`、`Event::CronEffectivenessChanged`、`CronEffectivenessVm`)——它按 `trigger='scheduled'` 的 `workflow_run` 行统计,定时任务只建活/采集后再没有这种行,面板会永远显示「触发 0 次」;WorkflowHub 卡片与详情的「N 次复用」字段(`WorkflowHubRowVm.uses`)——`record_workflow_use` 已删,数字冻结。若以后要「定时任务有效性」,正解是给每次到点触发追加一行只追加的触发日志(如 `cron_fire` 表),不是复活旧统计。

**2026-08-17 第一轮**删掉的东西,git 历史可找回(`git log --diff-filter=D --summary`):

- 死代码:`bw_engine::github::checkout_issue_branch`;`Command::SyncProjectFile`/`RefreshIssues`/`RunDraftWorkflow`/`UpdateWeekPlan`/`RefreshHubs`/`AnnotateWeeklyReview`/`MigrateLegacyShellsIfNeeded` 及其 handler;`Event::WeeklyReviewAnnotated`/`LegacyShellsMigrated`;`weekly_review` 表(无读者);`bw_core::model::drafting_workflow`;`run_workflow_inner` 的 `force_mock` 参数。
- 一次性存量迁移 `bw-app/src/legacy_migration.rs` 全链(真实日常库 `app_meta` 四个 done 标记全在,新库本就 no-op)。
- `crates/bw-app/examples/` 41 → 12 个:删掉的 29 个是历史批次的一次性验证脚本(是「已发货 commit 的收据」,不是回归守卫)。保留清单见 `DEVELOPMENT.md`。
- `crates/app-web/`(0 行代码的占位 README);Cargo.toml 注释保留一句「Web 版=以后也许」。
- 文档:没有删任何文档,只搬进 `docs/archive/`(见 `docs/archive/README.md`)。
