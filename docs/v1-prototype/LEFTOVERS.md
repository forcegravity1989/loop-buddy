# V1 产品化 · 遗留问题汇总

> V1 三个窗口（纳入项目 / 找指标·绑数据 / 总览重构）实践过程中冒出、但**不在当前窗口解**的问题。
> 每条标产生窗口（W1/W2/W3）+ 现象 + 未决点 + 处置。三窗口一把合入后，把这里的条目转成 issue 挂到库上。

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

**处置**：W1 不解，暂留。待三窗口合入后与各窗口遗留汇总转 issue。

**事实源**：`crates/bw-app/src/lib.rs`（`write_charter` L7829 / `write_component_standards` L7855 / `push_head` 调用 L5522）。

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

**未决点**：枚举收口 + inline arm 改 script 的迁移落在 W2 Phase3，需同步改采数链与 metrics_file 解析。

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

## 索引 · 穿刺修复批次 1（cowelink W1 穿刺 7 条反馈）

**产生窗口**：V1 三窗口合入后、用户用 cowelink 做 W1 穿刺实地冒出。**本批次修**（见 `docs/v1-prototype/piercing-fixes-1.md`）。

7 条：① GitHub/CodeHub 新建仓 UI 不一致 ③a cron 卡看不明白 ③b 连接器卡分不清 ④ 总览看不到仓指标（采集时序竞态）⑤ yellow 报错（host 黄区 + toast 不自动清）⑥ 指南 U2 去两个已知坑 + 加竞品分析章节 ⑦ 指南 U2 加创建后截图位。

**与 LEFTOVERS 的交叉**：
- 点 ④ 的「UI 冻死」callout 与 W1-1 无直接重叠（W1-1 是 buddy 自动 push 用户仓，点 ④ 是 cron 抢跑时序），但点 ⑥ 删的「UI 冻死」callout 是 issue1 §6 bug① 的 UI 表现——**本批次只删指南 callout（点 4 时序修复缓解 cron 抢跑，但 clone 同步堵单线程的根因 issue1 bug① 未解，留 issue1 §6）**。
- 点 ⑤ yellow 未登录标注与 issue1 §6「yellow 未登录」一致，本批次落地（host 选择器灰置 + 标注）。

**事实源**：`docs/v1-prototype/piercing-fixes-1.md`（设计事实源，未改代码）。
