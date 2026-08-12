# V1 产品化 · 遗留问题汇总

> V1 三个窗口（纳入项目 / 找指标·绑数据 / 总览重构）实践过程中冒出、但**不在当前窗口解**的问题。
> 每条标产生窗口（W1/W2/W3）+ 现象 + 未决点 + 处置。三窗口一把合入后，把这里的条目转成 issue 挂到库上。
> **唯一完整清单**（2026-08-06 归拢）：issue1/issue2/issue3 三份 plan 里散落的遗留/未决条目已全部并入此文件，各 plan 顶部加横幅指向本文，读遗留只看此文件。

---

## W1-1 · 创建时 buddy 自动写并 push 用户项目仓提交

**产生窗口**：W1 纳入项目（`docs/v1-prototype/issue1-onboard-simplify.md`）

**现象**：`CreateProject` / `CompleteCreation` 在用户项目的 owned workspace 自动 `commit_file` 写两类文件，并在 `CompleteCreation` 末 `push_head` 推远端：
- `PROJECT.md` 章程（`docs(bw): 项目章程 · 开篇` + `… · 完成创建`，两次提交）——`crates/bw-app/src/lib.rs` `write_charter`
- `.claude/standards/{agent,skill,workflow,cron}-standards.md` 四份组件标准（`docs(bw): 模板能力 · 组件标准文件`）——`write_component_standards`

buddy 在自己 workspace_path（`BW_WORKSPACES` 下的 clone）里提交，再 push 到用户项目远端 main。

**未决点**：
1. **必要性 / 健壮性**——buddy 动用户项目 git 历史是否必须？章程 + 组件标准该自动写进仓，还是该由用户主动生成 / opt-in？`docs(bw):` 提交约定是否撞用户项目自有的 commit 规范（用户项目可能有自己的 conventional-commits / 签名要求）？
2. **worktree 感知**——buddy 在自己的 workspace clone 提交并 push；用户若在项目的独立 worktree 工作，需 `git pull` 才能感知这些自动提交，存在同步感知缺口（用户不知道 buddy 往 main 推过东西）。
3. **PR 独立性**——三件套（竞品分析 / 找指标 / 绑数据）各产 PR；charter + standards 已在 base（main）上，PR 从该 base 分支基本不碍独立性。但 buddy `push_head` 与用户 worktree 并行 push 同一 main 可能产生分叉 / 冲突 / 强推风险。

**处置**：✅ 已修（2026-08-07）。用户决议：仓库是 buddy 建的、建之初推初始规范没问题，**保留自动写章程+组件标准+本地 commit+push main（不走 PR）**，不加 opt-out 勾选。只补「报告不代答」缺口：三处 `let _ =` 静默吞失败（CreateProject 章程/标准 + CompleteCreation 章程）改成失败 toast；push 成功的「已推送」toast 原本就有。失败不再静默。

**事实源**：`crates/bw-app/src/lib.rs`（`write_charter` L7829 / `write_component_standards` L7855 / `push_head` 调用 L5522）。

---

## W1-2 · codehub clone 同步堵命令循环 → Intent 提交后 UI 冻死

**产生窗口**：W1 纳入项目（`issue1-onboard-simplify.md` §6 bug①）。

**现象**：`CreateProject`/`CompleteCreation` 里 codehub `git clone` 同步执行，堵住 buddy 单线程命令循环，Intent 卡提交后 UI 卡死。PF1 穿刺批次（`piercing-fixes-1.md` 点④）修了 cron 抢跑时序（clone 完成前 cron 不跑、`CompleteCreation` 即采一次），缓解了「采集抢在 clone 完成前」的竞态，但 **clone 同步堵命令循环的根因未解**。

**未决点**：clone 是否该改异步/后台执行释放命令循环？

**处置**：留后续。PF1 已缓解时序竞态，根因未解。

**事实源**：`docs/v1-prototype/issue1-onboard-simplify.md` §6 bug①、`docs/v1-prototype/piercing-fixes-1.md` 点④。

---

## W1-3 · op_stage.routine_schedule/stage_done 留列，signal 过期降级未读它

**产生窗口**：W1 纳入项目（`issue1-onboard-simplify.md` §6）。

**现象**：`op_stage` 表的 `routine_schedule` / `stage_done` 两列 W1 求同存异留了下来；signal 过期降级本想读这列，但改法未做（碰派生链）。

**未决点**：signal 过期降级是否读 `routine_schedule`？

**处置**：✅ 已实质解决，2026-08-07 关闭。读代码确认 `recompute_signals`（`sqlite.rs:1329-1338`）已读 `op_stage.routine_schedule` 解析成 `Cadence` → `measure.rs:70` staleness → `eval.rs:48-53` `stale && Green → Amber`，过期降级链路已完整接通。原记「改法未做」标记错。`stage_done` 列从未加进 schema，不用清。

**事实源**：`docs/v1-prototype/issue1-onboard-simplify.md` §6。

---

## W1-4 · 组件标准内容打磨（依赖项）

**产生窗口**：W1 纳入项目（`issue1-onboard-simplify.md` §6）。

**现象**：`write_component_standards` 写四份组件标准（agent/skill/workflow/cron），内容质量打磨是依赖事项。

**未决点**：四份 standards 的内容打磨。

**处置**：留写入（模板已落），内容后续打磨。

**事实源**：`crates/bw-app/src/lib.rs`（`write_component_standards`）、`docs/v1-prototype/issue1-onboard-simplify.md` §6。

---

## W3 · 总览重构窗口遗留

**产生窗口**：W3 总览重构（`docs/v1-prototype/issue3-overview-refactor.md`）。v2 总览（ProgressAll）已落地（dev `b166929`），以下问题冒出但不在 W3 解，按窗口边界留给对应窗口或后续。

### W3-1 · 北極星采不到（无 metric 行）

**现象**：北極星指标在 v1 上没有 `metric` 行——采集配置落在 `project` 列（`north_star_collect_kind` / `north_star_query`），挂不上 `observation`，signal 恒 `Unknown`。W3 总览如实渲染灰卡（折线空 + delta「—」+ 底注「Unknown≠绿」），不越界建采数。

**未决点**：北極星该建独立 `metric` 行（role=leading，挂 `north_star` 标记）+ 接 script connector 采数，还是沿用 project 列配置补挂观测链？

**处置**：留窗口二 / 采数。W3 只 UI forward-correct，不动 schema 与采集链。

**事实源**：`crates/bw-app/src/lib.rs:3143`（北極星 collect 落 project 列）、`crates/app-desktop/src/screens/op.rs`（BizMetricCard 灰卡分支）。

### W3-2 · collect_kind 枚举收 5→2 + inline arm 改 script

**现象**：`CollectKind` 枚举现 5 kind（Github / Connector / Bw / Script / Manual，`bw-engine/src/metrics_file.rs:40`）。W2 Phase3 申明收成 2 kind（script / manual），代码未收。W3 只在 UI 层 forward-correct：`collect_label` 把 github/codehub/bw/connector 标「legacy·迁 script」，script→script、manual→manual，不动枚举。

**未决点**：枚举收口 + inline arm 改 script 的迁移落在 W2 Phase3，需同步改采数链与 metrics_file 解析；绑数据 skill + `.bw/scripts/` 正规化同归窗口二/采数。

**处置**：W2 Phase3 / 采数账。W3 只 UI forward-correct，不碰 `bw-engine`。

**事实源**：`crates/ui/src/vm.rs`（collect_label · forward-correct）、`crates/bw-engine/src/metrics_file.rs:40`（CollectKind 枚举 · 待 W2 收）。

### W3-3 · ↻同步指标文件按钮退场

**现象**：v2 总览保留「↻ 同步指标文件」按钮（`op.rs` ProgressAll 业务指标区头）。HTML 原型显退场，W2 Phase3 也申明退场——按窗口边界 W3 不删 W2 申明的活。

**未决点**：W2 Phase3 采数链正规化后，<code>SyncMetricsFile</code> 由 PR merge auto-fire 兜底，手动按钮不再需要即可退场。

**处置**：✅ 已决（2026-08-06 review）。**按钮保留，不退场**。此前两份文档互指对方负责、谁都没删，实况是：`76c7d0e`（W2 Phase3 review-fixup）确实删过，`a2b914c`（W3 总览重构）又加回，今天仍在。这次把口径钉死为保留——merge auto-fire 覆盖不了「人手改了 `.bw/metrics.toml` 但还没走 PR」的补采场景，手动补一刀有真实用途。W2 设计文档 §3.2/§4 Phase3/§5 三处「已退场」申明同步纠偏。

**事实源**：`crates/app-desktop/src/screens/op.rs`（v2 业务指标区 ↻ 按钮）、`docs/v1-prototype/issue3-overview-refactor.md §3`（W2/W3 边界表）。

### W3-4 · 白名单撞名 edge case

**现象**：项目指标 vs 业务指标用 `is_intrinsic_metric` 名字白名单分流（命中 = 层 B 项目指标条，未命中 = 层 A 业务指标卡）。若用户手建指标名字撞 W1 seed 名（「阶段完成 Issue 数」/「开放 Issue 数」/「已合入 MR 数」），会被误判为 intrinsic。

**未决点**：名字撞库低风险但存在。根治需给 metric 表加 intrinsic 布尔字段（或 source 字段区分 buddy-seed vs user-defined），动 schema。

**处置**：V1 接受。后续可加 `intrinsic` 字段根治（新 seed 指标需同步白名单）。

**事实源**：`crates/ui/src/vm.rs`（is_intrinsic_metric · 名字集合）、`docs/v1-prototype/issue3-overview-refactor.md §2`（白名单设计）。

### W3-5 · wall HealthOverviewBar 非逐字移植 op HealthOverviewCard

**现象**：v1 总览的 `HealthOverviewCard`（`op.rs:307-349`，per-project 跨阶段信号 + 点击跳 stage）被移到 `wall.rs` 成 `HealthOverviewBar`。但 wall 无 `OpVm`，改成<strong>跨项目信号分布</strong>（green/amber/red/unknown 计数，green 隐身折成计数，非 green 出声），不是逐字移植。per-project 跨阶段细节留在 ProgressAll 的阶段轴（每 stage 一个信号点）。

**未决点**：wall 跨项目概览 vs op per-project 细节的分工是否最终形态？wall 是否需要点击下钻到某项目的阶段？

**处置**：W3 接受当前形态（wall 跨项目分布 + op 阶段轴细节）。下钻交互待后续。

**事实源**：`crates/app-desktop/src/screens/wall.rs`（HealthOverviewBar）、`crates/app-desktop/src/screens/op.rs`（StageAxis · per-stage 信号点）。

### W3-6 · stats trio 从总览显示拿掉

**现象**：v1 总览有 stats trio（工作流累计 / 定时任务运行中 / 优化中待验收，`op.rs` 原 ProgressAll）。v2 拿掉显示（总览聚焦指标，不堆工程计数）。数据留 `OpVm.stats` 不删（`kernel.rs`），后续可回。

**未决点**：stats 是否该在别处（如 workflow panel）显示？还是退场？

**处置**：W3 从总览拿掉显示，数据留 VM 不删，后续窗口可回。

**事实源**：`crates/app-desktop/src/kernel.rs`（OpVm.stats · 保留）、`crates/app-desktop/src/screens/op.rs`（v2 ProgressAll · 无 stats）。

### W3-7 · W2 Phase2b 嵌入终端 merge 时与 W3 总览 op.rs 共存

**现象**：W2 Phase2b 嵌入终端（xterm / hook / resume）在 W2 分支已成，v1 基线无。W3 总览重构改了 `op.rs` ProgressAll。三窗口合入时，W2 的嵌入终端改动与 W3 的 op.rs v2 布局需共存。

**未决点**：merge 时 op.rs 是否冲突？嵌入终端挂在哪个 panel / 区段？

**处置**：✅ 已解（2026-08-06）。W2 分支上 op.rs 已同时带着嵌入终端（`TerminalWidget`/`WorkflowStage`）与 W3 的 v2 `ProgressAll` 布局，两者是同一文件里不同区段（Center 的 `match (op.panel, stage)` 分支），未见结构冲突。嵌入终端挂在 `Panel::Workflow` 分支下的 `WorkflowStage`；`ProgressAll` 是 `Panel::Progress` 分支——各自独立。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md`（Phase 2b · 嵌入终端 · §9/§10 stdin 修复）、`crates/app-desktop/src/screens/op.rs`（`Center` 组件的 panel 分支）。

### W3-8 · weekly delta carry-forward 伪"没变"（review Low）

**现象**：`weekly_spark` 先做 carry-forward（空周继承上个已知值，保折线连续无空缺），`weekly_delta` 再读末两桶算 delta。当某指标本周无新观测但 8 周窗内有旧数据时，末周桶被 carry-forward 填满 → delta 算成 `0.0`，渲染"→ 0.0 / vs 上周"——读着像"没变"，实为"本周没采"。

**未决点**：delta 该不该在"末周桶是 carry-forward 而非真观测"时显「—」（无数据）而非 `0.0`？需 `weekly_spark` 多返回一个"末周桶有无真观测"标志。

**处置**：W3 不解（review 判 Low + 缓解已在：buddy 情况行的 `metrics_stale` 计数标"N 个指标本周未记·建议复盘" + 北極星卡 collection_chain 显"cron 未跑"——信息在，只是不在 delta 数字本身）。后续增强。

**事实源**：`crates/ui/src/vm.rs`（`weekly_spark` carry-forward / `weekly_delta`）、`crates/app-desktop/src/screens/op.rs`（buddy 情况行 `metrics_stale`）。

---

## W3-9 · DeleteProject 不清磁盘 workspace clone —— 删项目后磁盘残留

**产生窗口**：W3 总览重构窗口后、用户穿刺准备期（删 omhwcc 重加）实地撞出。

**现象**：`Command::DeleteProject` → `store.delete_project` 只清 DB 行（issue/artifact/connector/cron_task/metric/op_stage/session/handoff/observation/message/skill_file/workflow_version 等全表，清理本身近期已补强，见 `6dce307`），**不删 `workspaces_root` 下该项目的 clone 目录**。物证：用户实际库 `%APPDATA%\BuildersWorkbench\workspaces` 下残留 3 个 omhwcc 孤儿 clone（`aa-c3a908ab` / `ohmycc-739ee884` / `proj-f442abd9`，remote 均 `ssh://git@szv-open.codehub.huawei.com:2222/innersource/AI-Coding_G/omhwcc.git`，DB project 表已无对应行）。全代码库唯一 `remove_dir_all` 是 `workspace.rs:293` 的兄弟目录清理，与删项目无关。

**未决点**：删除是否连带删 workspace clone？边界：用户可能手动把 `workspace_path` 指到自有目录（不该删，如 `/d/2026/code/omhwcc` 是用户自己的工作副本，非 buddy 建的）；只有 buddy 自动建的（`workspaces_root` 下 `<slug>-<uuid6>`）才该删。删目录不可逆，需谨慎（用户可能想保留产物取证）。

**处置**：穿刺后转 issue。重加同仓前，孤儿目录人工清掉。产品指南 U2 **不**写此说明（2026-08-06 决议：进本 LEFTOVERS，不进用户指南）。

**事实源**：`crates/bw-app/src/lib.rs:8458`（DeleteProject handler）、`crates/bw-store/src/sqlite.rs:624`（delete_project）、`crates/app-desktop/src/kernel.rs:483`（workspaces_root）。

---

## W2-1 · 嵌入终端离开面板期间可能丢字节；buddy 重启后黑窗口整块消失(无提示)

**产生窗口**：W2 找指标/绑数据 stdin bug 修复棒次（`docs/v1-prototype/issue2-metrics-interactive-loop.md` §10.6/§10.7），用户实测导航 + 重启两个场景冒出。

**现象**：
1. 同一次 buddy 运行中，切到别的面板再切回工作流，终端曾显示为空——**这部分已修**（§10.6①：`TERM_INIT_JS` 的旧 guard 只判断"存在即返回"，不会把 xterm 的渲染 DOM 搬到 Dioxus 重建出的新 `div` 上；改成"存在就搬家 + 重绑 div 级监听器，`onData` 不重绑"）。
2. buddy 重启后再进工作流，终端整块（连黑框都）消失——这是`pty_active`(`state.pty_input_tx.is_some()`) 纯内存状态在进程重启后天然为 false 的诚实表现，**不是本次要修的 bug**，但用户体验上没有任何"会话已断开，点▶跑可恢复"的提示,略生硬。
3. 即便①修好后，如果用户离开工作流面板的时间跨过多个 100ms 采集批次，中间批次的 PTY 输出可能被静默丢弃——PTY 字节走 `watch::channel<Vec<u8>>`(单槽、非队列)从内核线程送到 UI，`TerminalWidget` 卸载时没人 `.changed().await`,内核线程仍按 100ms 一批覆盖发送，只有卸载期间的*最后一批*会被下次挂载时的新 receiver 捞到。

**未决点**：
- ②要不要在 UI 上加"会话已断开(buddy 重启)，点▶跑用 `--resume` 接回"的提示,而不是让黑框默默消失？
- ③要不要把单槽 `watch` 换成有界队列/服务端整段 scrollback 缓冲,彻底堵死导航期间丢字节的窗口？这块工作量较大(涉及 kernel.rs 的 pty 分发结构),本次 bugfix 没做。

**处置**：①已修（见 issue2-metrics-interactive-loop.md §10.6，含 Node harness 验证）。②③留后续窗口评估，不在 W2 本棒动。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md` §10.6/§10.7、`crates/app-desktop/src/screens/op.rs`（`TERM_INIT_JS`/`TerminalWidget`）、`crates/app-desktop/src/kernel.rs`（`pty_bytes` watch channel）。

---

## W2-2 · Issue 板重复点「▶ 跑」堆积重复「阶段记录」卡 —— 已修，历史脏数据未清

**产生窗口**：W2 找指标/绑数据 stdin bug 修复棒次，用户实测冒出（`docs/v1-prototype/issue2-metrics-interactive-loop.md` §10.5）。

**现象**：同一个 issue（如「找指标」）每点一次「▶ 跑」，工作流「阶段记录」轨就多一张同名卡片，用户删不掉，内容看起来是同一个（空）会话。根因是「▶ 跑」onclick 每次都铸新 `SessionId` 再 `StartSession`，跟 `run_issue_interactive` 真正用来判断 resume 的 `claude_session_id` 完全无关——UI 侧堆积纯粹是重复插入的空壳。

**未决点**：本次按 `(stage_kind, title)` 去重复用既有 session id 解决了"以后不再堆积"，但**历史已经堆积出来的重复卡片没有批量清理路径**（没有 `DeleteSession` 命令）。

**处置**：新增去重（`existing_issue_session()`，op.rs）已修"以后"；"过去"的脏数据不补（没有 `DeleteSession`，加会涉及 store 新方法 + UI 按钮，本次不擅自扩范围）。用户下一步计划重建 cowelink 项目验证，删项目会带走它名下所有会话记录，不需要额外清理动作；如果后续在存量项目上还是想清理，需要单独排 `DeleteSession` 这个功能。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md` §10.5、`crates/app-desktop/src/screens/op.rs`（`existing_issue_session`）。

---

## W2-3 · 交互式无 per-token 预算封顶（显式偏差，已接受）

**产生窗口**：W2 交互式引擎（`issue2-metrics-interactive-loop.md` §2.5 R1）。

**现象**：`--max-budget-usd` 只配合 `--print`，交互式 PTY 路径（`run_skill_pty`）没有 per-token 硬 cap，也**不靠超时兜底**——只在会话 EOF / App 丢输入端 / 用户取消时收尾，无 wall-clock deadline。对 `CLAUDE.md`「单次花费封顶」是显式偏差。

**未决点**：交互式是否要补封顶机制？

**处置**：已接受（2026-08-06 review）。封顶是防后台 runaway 的，后台 one-shot 轨（`ClaudeCliExecutor`）照旧 `--max-budget-usd 0.5` + `ATTEMPT_TIMEOUT_SECS`，一行没动。待后续评估是否给交互式补。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md` §2.5、`crates/bw-engine/src/interactive_cli.rs`（`run_skill_pty` 无 deadline）。

---

## W2-4 · Phase 5 guide 校准 partial（m6 + u3/u4 待补）

**产生窗口**：W2 交互式引擎（`issue2-metrics-interactive-loop.md` §4 Phase 5、§2.6 #6）。

**现象**：Phase 5 guide 校准只 partial——m4/m5 已改（Phase 1 实态），**m6**（指标采集链 + 表/字段 + script kind「计划中」→「已接」校准）未动，**u3/u4** 阶段屏（交互式用户旅程）待补，信号色 token 对齐 plan/00 §6 未做。

**未决点**：m6 + u3/u4 + 信号色 token 对齐。

**处置**：留 Phase 5 / 后续窗口。altitude 用系统×CRUD。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md` §4 Phase 5、§2.6 #6。

---

## W2-5 · Hub 四组件完整规范未定（最简规范已定）

**产生窗口**：W2 交互式引擎（`issue2-metrics-interactive-loop.md` §3.2、§6 遗留②）。

**现象**：Phase 3 只定了最简规范（`.bw/scripts/` 目录约定 + `.bw/connectors.toml` 清单格式 + sync 感知规则）。Hub 四大组件（skill/connector/agent/cron）的**完整规范**未定。

**未决点**：四组件完整规范。

**处置**：留遗留单独定。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md` §3.2、§6 遗留②。

---

## W2-6 · 多人协作（多 PC）= V1+ 特性

**产生窗口**：W2 交互式引擎（`issue2-metrics-interactive-loop.md` §6 遗留①）。

**现象**：完整多人协作（多 PC 并行工作）是 V1+ 特性，V1 不接。

**未决点**：协作模型。

**处置**：V1 不接，留 V1+。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md` §6 遗留①。

---

## W2-7 · §9.7 诊断 spike 清理待核实

**产生窗口**：W2 交互式引擎（`issue2-metrics-interactive-loop.md` §9.7、§8）。

**现象**：§9.7 列了开发阶段清理项——删 `crates/bw-engine/examples/` 下诊断 spike 源文件（pty_spike/conpty_direct/conpty_test/conpty_oxide_test/conpty_oxide_claude）、删 `[target.'cfg(windows)'.dev-dependencies]` 的 conpty/conpty-oxide/winapi、删 `interactive_cli.rs` 的 `[pty-diag]` 诊断日志、删 pty-diag.log。§8 述两处**预研 spike 文档目录**已删（`3d7b6ca`），但 examples/ 下**源文件清理是否做完未核实**。

**未决点**：examples/ 诊断 spike 源文件 + dev-deps + 诊断日志是否已清干净？

**处置**：待核实。本棒未核。

**事实源**：`docs/v1-prototype/issue2-metrics-interactive-loop.md` §9.7、§8。

---

## V1-P1 · macOS 上交互式跑不了（V1 实际是 Windows-only）

**产生窗口**：W2 交互式引擎（`docs/v1-prototype/issue2-metrics-interactive-loop.md` §9 PTY），2026-08-06 整体 review 时点出、用户要求先记为遗留。

**现象**：`CLAUDE.md` 顶上写的是「macOS+Windows」，但 V1 的交互式两件套（找指标 / 绑数据）在 macOS 上**跑不成**：

- 嵌入终端那条真路径 `InteractiveCliExecutor::run_skill_pty` 整个函数挂着 `#[cfg(windows)]`，PTY 后端是 Windows 专有的 `conpty-oxide`。非 Windows 上这个方法不存在，走 trait 默认实现 → `Err("PTY not supported by this executor (use run_skill instead)")`。
- **桌面壳上没有回落**：`app-desktop/src/kernel.rs` 建 App 时无条件 `.with_pty()`，`run_issue_interactive` 只在 `pty_enabled == false` 时才走 `run_skill`。所以 macOS 上点「▶跑(交互)」= 这个 run 立刻以那句**英文报错**结算失败，用户看不懂也没得跑。引擎侧注释写的「caller falls back to run_skill」只对 `pty_enabled=false` 的 headless/example 路径成立，**对桌面不成立**。
- 那条 `run_skill` 回落路径本身在 macOS 上也不体面：`osascript` 叫 Terminal.app 开窗口，`osascript` 进程叫完就退、拿不到 claude 句柄，代码于是 `sleep(self.timeout)` 睡满 1 小时再**宣告 `completed = true`** —— 谁都没验证过 claude 退没退，这是**谎报完成**，违反「读回为证」。
- `portable-pty` 目前只作为非 Windows 的 keepalive 依赖挂在 `bw-engine/Cargo.toml` 里，**没有接**任何代码路径。

（注：`CLAUDE.md` 记的「computer-use 在 macOS 上 screenshot 能用、click/key 永久受阻」是**验证侧**的另一回事，与本条运行侧的问题不是一件事，别混谈。）

**未决点**：
1. 给 `run_skill_pty` 补一个 Unix PTY 后端（`portable-pty` 已在依赖里，或换 `pty-process`/`nix`），让 macOS 也走嵌入终端 —— 这是让 macOS 真正可用的唯一正路。
2. 在补后端之前，`run_skill` 那条路径的「睡满超时 → completed = true」语义要改：它现在会**谎报完成**，违反「读回为证」。至少该标成未验证完成，或干脆在非 Windows 上如实拒绝启动交互式，而不是假装跑完。
3. 桌面壳那句英文 `Err` 直接怼到用户脸上，没有人话映射（对比 codehub 那套错误映射）。补后端前至少该说人话：「本机（macOS）暂不支持嵌入式交互终端，V1 仅 Windows」。

**处置**：V1 不解。**V1 的实际目标平台按落成情况是 Windows-only**，文档不要再宣称 macOS 上交互式可用；转 issue 时按「macOS 交互式 PTY 后端」单独排一件，连带处理上面第 2、3 点。

**事实源**：`crates/bw-engine/src/interactive_cli.rs`（`run_skill_pty` 的 `#[cfg(windows)]` L686、trait 默认实现「PTY not supported」L517、macOS `osascript` 分支 L608、`wait_child` 超时宣告 completed L809）、`crates/bw-app/src/lib.rs`（`run_issue_interactive` 按 `pty_enabled` 二选一 L5225）、`crates/app-desktop/src/kernel.rs:573`（无条件 `.with_pty()`）、`crates/bw-engine/Cargo.toml`（`portable-pty` non-Windows keepalive）。

---

## 索引 · 穿刺修复批次 1（cowelink W1 穿刺 7 条反馈）

**产生窗口**：V1 三窗口合入后、用户用 cowelink 做 W1 穿刺实地冒出。**本批次修**（见 `docs/v1-prototype/piercing-fixes-1.md`）。

7 条：① GitHub/CodeHub 新建仓 UI 不一致 ③a cron 卡看不明白 ③b 连接器卡分不清 ④ 总览看不到仓指标（采集时序竞态）⑤ yellow 报错（host 黄区 + toast 不自动清）⑥ 指南 U2 去两个已知坑 + 加竞品分析章节 ⑦ 指南 U2 加创建后截图位。

**与 LEFTOVERS 的交叉**：
- 点 ④ 的「UI 冻死」callout 与 W1-1 无直接重叠（W1-1 是 buddy 自动 push 用户仓，点 ④ 是 cron 抢跑时序），但点 ⑥ 删的「UI 冻死」callout 是 issue1 §6 bug① 的 UI 表现——**本批次只删指南 callout（点 4 时序修复缓解 cron 抢跑，但 clone 同步堵单线程的根因 issue1 bug① 未解，留 issue1 §6）**。
- 点 ⑤ yellow 未登录标注与 issue1 §6「yellow 未登录」一致，本批次落地的是**标注**：host 选择器三个 alias 都可点，chip 上挂 tooltip、下方常驻一行「green/open 已登录可直用；yellow 需先在本机 `codehub-cli -H yellow auth login`」，选中未登录的 yellow 时靠 CLI 调用失败回人话报错。**没有做灰置**——buddy 不探测 `codehub-cli` 的登录态，探不到就不能替用户判定「没登录」，灰置会在人其实已登录时挡住路（事实源：`crates/app-desktop/src/screens/create.rs` `CodehubHostPicker`）。

**事实源**：`docs/v1-prototype/piercing-fixes-1.md`（设计事实源，未改代码）。

---

## 索引 · V1 产品化任务 A 收口（cowelink 验证 P1–P13，2026-08-06）

**产生窗口**：`.claude/cowelink-verify-2026-08-06.md`（不提交、事实源）记录的 W2/W3 真实实践问题台账 P1–P13，本窗按「高痛/契约硬错优先」分诊后动代码。**尚未 commit**（工作树改动，等用户明确要求再提交）。

### 已修（本窗，✅）

| ID | 一句话 | 落地方式 |
|---|---|---|
| **P4** | 网页合 MR 后点「已完成」不同步指标/连接器 | 把 `MergeIssuePr` 尾部的 pull+`SyncMetrics`+`SyncConnectors` 挪进 `TransitionIssue` 的 `newly_done` 记账块，两个入口（merge 内部 dispatch / 网页手点已完成）共用一条路径，不重复跑 |
| **P5** | connectors.toml 错键静默吞空 + 脚本只 print 不写 output + Windows 找不到 python | `ConnectorsFile`/`ConnectorDef` 加 `deny_unknown_fields`（错键直接报错，含回归测试）；衔接层 system prompt + `connectors-toml-format.md` + `metrics-binding/SKILL.md` 三处加"只读 output 文件、不看 stdout"硬提示；`script_interpreter_candidates` 给 Windows 加 `py` 候选 |
| **P10** | 总览业务卡（`BizMetricCard`）没有手填框 | `collect_kind=="manual"` 时卡内嵌入既有 `RecordInline`（复用组件）；北极星若命中同名 `metric` 行走同一路径自动生效，无 `metric` 行的灰卡（W3-1 缺口）不动 |
| **P12** | 所有 `kind=="script"` 连接器副标题硬编码「采集 Issue/MR」 | `connector_kind_label` 按 name/config 派生：仓统计脚本保留原文案，其余从 config 挖脚本文件名生成专属标签 |
| **P13** | cron 卡副标题「采集代码仓指标」误导（业务脚本也被这条 Daily 调度，但名字看不出来） | 只改前缀措辞为中性的「本项目全部 script 指标(...)· 每日」，**不做**按 `connectors.toml schedule` 拆独立 cron（用户明确留待后续） |
| **P2** | Issue 详情弹窗「▶ 跑」不知道看板已在跑；交互式活运行史恒空时显示「还没有运行」（假话） | 弹窗复用看板卡片同一段 `is_running`/`same_project_busy`/`run_label` 判断（传 `active_run`+`project_id`）；`IssueDetailVm` 加 `is_interactive`（读 `issue.interactive_started`），运行史为空且是交互式活时换成「过程在下方嵌入终端/会话里」的诚实文案 |

验证：六步门禁 + `cargo test --workspace --exclude app-desktop`（35 测试）全绿；未跑 sqlite/深链读回（本窗改动多是纯前端文案/组件复用/同步路径合并，无新 schema，读回价值有限，留給下一棒需要时再核）。

### 核实结论（不算修复）

**P7 · 创建未见 PROJECT.md / standards**：读代码确认路径存在——`write_charter`/`write_component_standards`（`lib.rs:9573`/`9599`）只在 `is_owned_workspace(dir)` 为真（工作区 `.git` 存在且根提交作者可读）时才写，且是 best-effort（`let _ = …`，失败被静默吞掉，不阻断创建流、也不提示用户）。**未复现**，不确认验证日志里"未见"是因为路径条件不满足（如目标仓判定成 bound 而非 owned）还是真的静默失败。若要根治静默失败这半个缺口，需要把 `let _ =` 改成至少记一条日志/toast，而不是假装成功——这条不在本窗改，留给下一棒决定要不要做。

### 转移 / 新增条目（本窗不修，非 top 优先级 + 需要设计判断）

**P1 · 窄窗嵌入终端 ANSI 错乱、底栏双 prompt**：xterm 在窄容器下的 `Fit`/列宽重算与 ANSI 转义重绘冲突，本窗未查——工作量在 xterm.js `fit addon` 与容器尺寸监听那层，不是一次性小改。留后续窗口专项处理（截图见验证日志 §2.1）。事实源：`crates/app-desktop/src/screens/op.rs`（`TerminalWidget`）。

**P3 · 单例 PTY 无法回看历史会话**：app 级只有一路活跃 PTY（`kernel.rs` pty watch），切到某个 issue 的历史会话卡时，看到的仍是当前 live 终端的内容，不能"只看"旧会话的 scrollback。需要 per-session scrollback 缓冲或明确的"live 在 #N，你正在看的是历史只读快照"提示，属于 W2-1 提到的"单槽 watch channel"架构限制的延伸，非本窗小改能解。事实源：`crates/app-desktop/src/kernel.rs`（pty watch）、`crates/app-desktop/src/screens/op.rs`（`WorkflowStage`）。

**P3b · Done 后交互式活无 `--resume` 入口**：`runnable` 判断只覆盖 backlog/todo/in_progress（`op.rs`），Done/InReview 状态下即便设计允许 `--resume`（`prepare_issue_run resume=true`），看板上也没有"打开会话/resume"的可点入口。需要在 runnable 之外单独给交互式 Done/InReview 活加一个不改变状态机、只读打开会话的按钮。事实源：`crates/app-desktop/src/screens/op.rs`（`runnable`）、`crates/bw-app/src/lib.rs`（`prepare_issue_run`）。

**P6 · Done toast「N 个产物版本」文案吓人**：`scan_and_register_artifacts` 对 owned workspace 做全量 tracked 文件快照登记，一次 Done 常报出几十上百个"新增产物版本"，用户第一反应是"是不是哪里跑飞了"。这是设计如实行为（整仓快照，不是 bug），本窗不改代码，留给任务 B 产品指南写清楚"这是整仓快照，不代表改了这么多文件"。

**P9 · cron 不随 `connectors.toml` 的 `schedule` 字段增减独立定时器**：设计上"一条 Daily `CollectMetrics` 覆盖项目下全部 `kind=script` 连接器"（见 §9 代码事实：`lib.rs` tick→`collect_project_metrics`→按 `kind==script && project_id` 过滤，`~3979-3993`），`connectors.toml` 里写的 `schedule` 字段目前**只是文档性的，buddy 不读它建 cron 行**。是否要接线（按 schedule 建/更新独立 cron）还是保持"一条 Daily 全覆盖"的现状只需把面板文案说清楚（P13 已解决一半），是产品未决点，本窗不擅自接线。事实源：`crates/bw-app/src/lib.rs`（`collect_project_metrics`）、`crates/bw-engine/src/connectors_file.rs`（`schedule` 字段仅解析不接调度）。

**P11 · Issue Done 后阶段记录区变回旧 Chat + 会话卡「进行中」不消失**：两个独立现象——① PTY 结束后 `pty_active=false`，工作流面板退回旧的 Chat 发送框（交互式活不写 `message`，所以是空壳），视觉上像"退步"；② `session.status=Active` 插入后没有任何路径把它翻成 Done，与 issue 本身是否 Done 完全脱钩，导致早已完成的活在会话列表里永远显示"进行中"。需要 session 状态跟随 issue 状态或显式归档动作，属于状态机层面的改动，本窗不擅自扩。事实源：`crates/app-desktop/src/screens/op.rs`（`WorkflowStage` Chat 回退分支）、`session` 表（`status` 字段无 Done 写入路径）。

**P8**：不新增条目——已是 `W3-1`（北极星无 `metric` 行 · 灰卡）的既有决议，采数窗口另议，本窗未碰。

---

## V2 · 阶段默认 Skill / 系统提示词与规范手册（2026-08-10 拍板延期）

**产生**：用户实测构建阶段无技能测试 issue ▶跑 进嵌入终端后，claude cli 里可见注入几乎只有 issue 标题+描述，没有构建板块 AI 小队 / 方法循环怎么干活。

**根因（已分析，见会话；设计事实源 `issue2-all-issues-terminal-runs.md`）**：V1-TermClose 把 issue 从「buddy 脚本调度阶段循环」改成「prompt 驱动」；`stage_workflow_with_playbook` 的 `phase_prompts`（构建师规格→任务→实现→评审）**故意不再进** interactive 系统提示词；多 agent 能力约定落在「技能方法论讲清 SubAgent 调度」。无 `standard_skill` 时 `fetch_skill_body` 为空 → 没有载体承载小队流程；m4 已诚实留口「默认系统提示词 / 默认 skill = 后续催熟」。

**用户拍板的 V2 统一概念（本窗不开发）**：

1. **维护好 buddy 系统提示词 + 一帮规范手册**（大提示词；按场景渐进加载文档——例如指标类额外加载 metrics/connectors 契约，才能被 buddy 托管对）。
2. **搞好有价值的 skill + 五大板块默认 skill**——选了某板块 = 装载该板块默认 skill；agent 小队调度本身就是 skill（认可「装载 skill」路线，而不是把旧 phase-loop 脚本调度搬回 issue）。

**处置**：✅ 记入 V2 整改队列，**本窗不改代码**。落地时走 `buddy-feature-dev`，设计归档到 [`docs/v2-prototype/`](../v2-prototype/README.md)(初始节奏与意向见 [`roadmap.md`](../v2-prototype/roadmap.md))，勿再堆进已发版的 V1 窗口号叙事。

**事实源**：`docs/v1-prototype/issue2-all-issues-terminal-runs.md`（prompt 模型 + 多 agent 转 prompt）；`crates/bw-app/src/lib.rs` `run_issue_interactive` / `prepare_issue_run`（`spec.prompt`/`phase_prompts` 不再服务 issue）；`docs/guide/buddy-guide.html` m4「默认系统提示词 / 默认 skill」留口。

---

## V2 · 手填观测不跨 Buddy / 不进仓（多人过程缺口）

**产生**：V2-② cowelink E2E（后来者纳管 + 绑数据）。北极星「累计总用户数」、滞后「周安装用户数」等 `collect=manual` 指标在本机总览手填后有数；其它机器上的 Buddy 读不到这些 observation。

**机制**：观测落本机 SQLite（过程信息）；Buddy 之间不同步库。手填不会写回 `.bw/metrics.toml`，也不会推远端。仓里正本只声明「这是 manual」，不承载数值。与「产品信息在仓、过程信息在本地」一致——但多人各自纳管时，非脚本采集的数等于「只在原始 Builder 那台机器上」。

**产品判断（本轮）**：可接受为已知边界。理想态是指标尽量 script 自动采；manual 仅过渡。若将来要共享手填值，需另设计仓内正本或共享源（本轮不做）。

**处置**：记遗留。不改 schema；指南/验收如实说「手填 = 本机过程数据」。

**事实源**：`docs/v2-prototype/same-project-multiple-workbenches.md` §3（正本在仓 / 过程在本地）；cowelink 读回 `observation.source_kind=manual`。

---

## Bug · 提 MR 后看板迟迟不进评审中 + merge 无忙态（cowelink 找指标 E2E）

**产生**：V1 实践 · cowelink 找指标真 E2E（会话已停、MR 已开，看板约两分钟才变评审中；合入成功但点击后数秒无反馈）。

**根因（已修）**：
1. InReview 兜底轮询固定 5 分钟；Stop hook 若在 MR 尚不可见时查空，下一轮要等满 5 分钟。
2. 半套刷新：`tick_scheduler` 里 poll 已改库并 toast，但桌面壳只在「本轮有 cron 触发」时重建 Vm → 看板状态可长期陈旧。
3. `MergeIssuePr` 按钮无本地 busy；Vm 在命令返回后才刷新，等待远端 merge 的几秒里像没点上。

**处置**：✅ 已修。有候选时约 15s 轮询 + `scheduler_ui_dirty` 强制重建 Vm；merge 点击即禁用并 toast「正在合入…」，完成/失败后再恢复；二次合入 Done 短电路提示。`SessionEnd` hook 仍未接（设计 md 已记），短周期轮询覆盖「会话关了但最后一次 Stop 没查到」场景。

**事实源**：`crates/bw-app/src/lib.rs`（`poll_interactive_inreview` / `INREVIEW_POLL_*` / `MergeIssuePr`）；`crates/app-desktop/src/kernel.rs`（tick Vm rebuild）；`crates/app-desktop/src/screens/op.rs`（merge busy）；指南 `buddy-guide.html`「触发查 MR」。
