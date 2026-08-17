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

## 5. 执行实录(执行时回填)

(见文末追加。)
