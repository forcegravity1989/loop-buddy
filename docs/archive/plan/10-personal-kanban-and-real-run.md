# plan/10 · 个人级看板 + 真执行器实跑刷新(执行计划)

> 由 fable 出计划,交 sonnet-5-max 执行。目标用人话写,源码锚点只进各件「工程对照」。
> 承 `plan/08-mvp-execution-plan.md`(两线四棒)与 `plan/09-aihot-practice-run.md`(aihot 践行);
> 本计划是当前接棒入口。**验证纪律不变**:深链启动(stderr `[BW_OPEN]` = 渲染证明)+ `sqlite3`
> 读回为证 + computer-use 抽查;UI 编译过即可,行为在 bw-app 命令层 + E2E 兜底,不假装 UI 测试。

## 0. 一句话 + 为什么现在做

**把工作台从「全局组件市场」补成「个人级项目看板」:打开一个项目,能看见并管理它自己的
skill/agent/workflow/cron/connector;让 agent 真跑一件活,产物/定时/版本/进展真实刷新进来;
把 aihot 的健康灯从「issue 解决数」这种工作量代理,换成从日报管线真实 telemetry 派生的产品指标。**

三个触发条件同时到位,所以现在做:
1. **一级侧边栏是 marketplace,不是项目看板**(用户点破的核心):`IconRail`(`chrome.rs:57`)的
   10 个 Hub 全是全局的——`SkillHub`/`AgentHub` 渲染 `state.skills`/`state.agents`
   **不按项目过滤**(`kernel.rs:586-587`,`build_vm` 无条件全量)。打开项目后点侧栏图标反而**跳出**
   项目上下文进全局库。每个项目自有 skill/agent 的维度**根本没有入口**。
2. **`project_id` 切片已在库里但没上 VM**:昨夜 `4adba65` 给 agent/skill/workflow_spec 加了可空
   `project_id`,但 `SkillCardVm`/`AgentCardVm`(`vm.rs:774/801`)**不带 `project_id` 字段**,UI 无从过滤。
3. **真执行器刚被用户解锁**:昨夜 `claude -p` 被账号配额(429,2026-07-24 才重置)全程挡住,只做了
   一次诚实探测就 Block(见 `iterations/PRACTICE-AIHOT.md` §0)。用户今日明确「搞定了 Claude -p 的
   登录,可以实际去执行了」——「让 agent loop 干活」第一次能端到端真跑,是本计划 K0/K3 的地基。

## 1. 五条工作线(按依赖排序,逐条独立 commit)

### K0 · 真执行器实跑验证(打底,先做,快)

**目标(人话)**:确认用户说的「Claude -p 登录搞定了」现在真成立——在 aihot 项目里让 agent 真跑一件活
(真开子进程 `claude -p`、真改文件、真跑测试),留下一条真实的 Done issue + 真实 run 记录 + 真实产物,
作为 K3「实跑后刷新」的地基。**这一步先于一切 UI 改动**:它既验证用户的解锁,又给后面每个刷新面板
提供真数据;万一配额/登录仍有问题,早暴露、不在假设可跑的前提上盖楼。

**工程对照**:
- 真实执行器 = `ClaudeCliExecutor`(`crates/bw-engine/src/claude_cli.rs`),经 `Command::RunIssue`
  触发(`bw-app/src/lib.rs`);`run_issue`/`run_workflow_inner` 按 `proj.workspace_path` 非空**热插拔**
  真实执行器(见 `[[bw-real-executor-pending-verification]]`:配了 workspace + Run* = 真网关)。
- 指挥器已就绪:`crates/bw-app/examples/practice_aihot.rs` 的 `probe-run` 子命令走真实 `RunIssue`;
  aihot 项目 workspace = `practice-aihot/workspaces/aihot-b7971eca`(真本地 git 仓,已在 `.gitignore`)。
- 有界执行:本机**无 `timeout`/`gtimeout`**(macOS 无 coreutils)——用后台起进程 + 轮询 `kill -0` +
  超时 `kill` 的模式(昨夜已验证可行),别用 `timeout`。

**DoD(读回为证)**:
- `sqlite3 practice-aihot/bw-aihot.db "SELECT status, error FROM workflow_run ORDER BY started_at DESC LIMIT 1;"`
  读回 `status=ok`(不再是配额 429 的 `failed`);对应 issue 经**显式** `TransitionIssue` 到 Done,
  `settled_at` 非空(Done 永不自动,`can_transition_to` 守卫)。
- 真实产物:`CollectArtifacts` 后 `sqlite3 … "SELECT COUNT(*) FROM artifact WHERE …"` 有真行;
  工作区 `git -C practice-aihot/workspaces/aihot-b7971eca log --oneline | wc -l` 较跑前 +≥1。
- 若登录/配额**仍**不可用:如实 Block 这一件,记进 `iterations/PRACTICE-AIHOT.md`,K3 退回「用昨夜
  已有的真实产物验证刷新链路」——**不假装跑过**。

### K1 · 项目二级侧边栏 + `project_id` 上 VM(看板骨架)

**目标(人话)**:打开一个项目后,在最左全局图标栏右侧多出一列「本项目组件」侧边栏——分组列出**这个
项目自有的** skill/智能体/工作流/定时/连接器(按 `project_id` 过滤),点进去是详情卡(K2)。全局
marketplace 图标栏**原样保留**,点 SkillHub 仍看全部。共享组件(`project_id IS NULL`)单列一组「共享
/ 引入」,诚实区分「本项目建的」和「从市场借的」,不混淆归属。

**工程对照**:
- **先上 VM**:给 `SkillCardVm`/`AgentCardVm`(`vm.rs:774/801`)、`WorkflowHubRowVm`(`vm.rs:497`)、
  `ConnectorCardVm`(`vm.rs:987`)加 `pub project_id: Option<ProjectId>` 字段;`CronRowVm`
  (`vm.rs:937`)已带 `project_id`(`main.rs:172` 在用),照抄。构造点:`skill_card`/`agent_card`
  (`kernel.rs:586-587` 调用处)从 `state.skills`/`state.agents` 的行里透传 `project_id`。
- **侧边栏组件**:新增 `crates/app-desktop/src/screens/project_rail.rs`(参照 `chrome.rs` 的
  `IconRail` 写法与 `theme` token),在 `main.rs:203` 的最外层 flex 里、`IconRail` 之后、内容区之前,
  **仅当 `v.view == View::App`(项目已开)** 渲染。数据源:在 `build_vm`(`kernel.rs:658` 之后,
  `active_project` 已知那段)按 `active_project` 过滤 `hub.skills/agents/workflows/cron_tasks/
  connectors`,装进 `OpVm` 的新字段 `pub project_components: ProjectComponentsVm { skills, agents,
  workflows, crons, connectors, shared_* }`,或直接在侧边栏组件里对 `v.hub` 现有列表做 `project_id`
  过滤(二选一,后者改动更小,推荐)。
- **分组**:`project_id == Some(active)` = 本项目;`project_id == None` = 共享/引入(单列一组)。
  空组诚实显示「本项目还没有自建的 X」,不隐藏、不塞占位。
- **不动**:`IconRail`(`chrome.rs`)、各全局 `*_hub.rs` 屏、`Op` 现有六标签(进度/工作流/定时/产物/
  版本/Issue)——项目侧边栏是**并列新增**,不改现有导航。

**DoD**:
- `BW_OPEN="aihot 日报" BW_PANEL=progress` 深链启动,`[BW_OPEN]` 出现即渲染成功、无 panic;
  computer-use 截图见二级侧边栏,「技能 2 / 智能体 1 / 工作流 1 / 定时 1 / 连接器 1」计数与
  `sqlite3 … "SELECT COUNT(*) FROM agent WHERE project_id='b7971eca…'"` 等读回一致。
- 关闭项目(回 Wall)侧边栏消失;全局 SkillHub 仍显示全部(含其它项目/共享),证明「两个维度并存」。

### K2 · 业界风格的组件详情卡(完善展示)

**目标(人话)**:现在的 skill/agent/workflow 卡太简短(名字+成熟度+一句描述)。参照业界(GitHub
组件卡 / 插件市场卡:徽章 + 统计 + 出处 + 关系)把详情卡补厚——但**每个字段都必须有真实来源**,
店里有的(runs/win_rate/uses/version/产物/出处/被谁用)才显示,没有的诚实标「—(无运行证据)」,
**绝不为了好看编数字**(健康难造假的铁律延伸到组件卡)。同一个卡组件,全局 Hub 和项目侧边栏共用。

**工程对照**:
- 卡片字段(全部来自真实 store 列,先确认列在不在,缺的补 VM 透传,**不新造**):
  - **头**:名称 + 类型徽章 + 成熟度/`maturity`(skill/agent 已有)+ **出处 chip**——
    skill=`LibSource`、workflow=`HubSource`(含昨夜加的 `Adopted`「选型引入」,`model.rs`)、
    agent 的来源。
  - **统计行**:agent=`runs`/`win_rate`(已有,`win_rate` 空→「—(无运行证据)」,`agent_hub.rs:143`
    的诚实规则保留);skill=`uses`(已有);**新增** last-run 时间、本组件关联的**产物数**——
    从 `workflow_run`/`artifact` 派生(参照 `hub_usage_ranking`,`kernel.rs:569`)。
  - **版本**:workflow/skill 若有版本轨迹,接 `version_log`(`OpVm.version_log`,`kernel.rs:842`)。
  - **关系**:「被这些工作流使用」反查已实现(`skill_hub.rs:88`/`agent_hub.rs:86`),保留并前移到卡面。
- 落点:抽出共享详情卡组件(可放 `screens/mod.rs` 旁新增 `component_card.rs`),`SkillHub`/`AgentHub`/
  `WorkflowHub` 与 K1 的项目侧边栏都调它,避免两套卡漂移。
- **诚实边界**:凡 store 无此数据的字段→「—」或「无运行证据」,**不 `unwrap_or(0)` 假装 0%**
  (`win_rate` 已树立此规则,推广到所有新字段)。

**DoD**:
- 深链到 aihot 项目侧边栏,点开「日报编辑」agent 卡:computer-use 截图见 runs/成功率/出处/被谁用,
  且成功率与 `sqlite3 … "SELECT runs, win_rate FROM agent WHERE name='日报编辑'"` 读回一致(K0/K3
  真跑前应显示「—(无运行证据)」,真跑后才有数——证明卡面数字是派生不是编的)。

### K3 · 实跑后刷新全链路(产物/定时/版本/进展/工作量)

**目标(人话)**:K0 真跑一件活之后,项目页的**产物、定时任务、版本、进展、工作量**面板要**真实刷新**
——不是手动录,是 run 结算时自动记账进去。大部分链路已存在,这一步是**端到端核验 + 补缺口**:让
「agent 干完活 → 面板自己更新」这条产品命题第一次真实可见。

**工程对照**(已存在的记账链路,逐个核验从真实 run 到面板刷新):
- **产物**:`scan_and_register_artifacts`(`bw-app/src/lib.rs:1249`)run 后自动扫工作区登记;
  `Command::CollectArtifacts`(`:1993`)+ `OpVm.artifacts`(`kernel.rs:863`,`LoadArtifacts` 后填);
  UI = `ArtifactPanel`(`op.rs:371`)。settle-once:同一件活产物绝不记两次。
- **定时**:真实调度器 `App::tick_scheduler`(`[[bw-real-scheduler-shipped]]`)到点自动**建** issue
  (no-hijack,绝不自动完成);`CronRowVm.last_run`/`next_run`(`vm.rs:937`);UI = `RoutineAll`
  (`op.rs:343`)。**今日已实测**:9:33 启动那刻调度器真自动建了第 29 号 issue,状态停 `in_progress`
  (未被自动置完成)——这正是要在面板上刷新可见的行为。
- **版本**:`OpVm.version_log`(`kernel.rs:842`);UI = `VersionPanel`(`op.rs:459`)。
- **进展**:指标 `signal` 经 `recompute_signals` 唯一写入(derive-only);run 结算喂
  `SourceKind::Telemetry` 观测(`:1147`);UI = `ProgressAll`(`op.rs:1124`)。
- **工作量**:`本周结算活数` 等——本计划 K4 会把它降级为旁注,但刷新链路一并核验。
- **connector 真喂**:`feed_workspace_metrics`(`:1217`)按名匹配 `METRIC_WS_COMMITS`/`METRIC_WS_DOCS`
  写 `SourceKind::Connector` 观测,`SyncConnector` 触发——K0 真跑后 `工作区真实提交数` 应自动 +1。

**DoD(每个面板一条读回)**:
- K0 真跑 + settle 后,`sqlite3` 分别读回:`artifact` 新行、`workflow_run` 新 `ok` 行、`observation`
  新 `Telemetry`/`Connector` 点、`工作区真实提交数` 指标读数 = `git log|wc -l`;深链到各面板
  computer-use 截图见新数据。**任一面板 run 后没刷新 = 缺口**,读源码定位补上(systematic 排查,
  别猜),补完再读回。

### K4 · aihot 指标重定:产出连续性为纲

**目标(人话)**:把 aihot 的健康灯从「issue 解决数」换成真产品信号(用户已拍板「产出连续性为纲」):
- **滞后性(结果)= 连续产出日报天数**:我到底有没有每天真的在出这个产品。
- **引领性(预测)= 每日命中率 = 命中数 / 原始条目**:信噪比会不会恶化(预示产品变水)。
两个都从 `aihot/main.py` 管线的**真实 telemetry 自动喂**(命中/原始/产出天数是管线已经在 stderr 打的真数,
见 `main.py:build_digest` 的 `命中=/去重后=/按源限量后=` 与 `digests/*.md` 落盘),**不手填、不 mock**。
`本周结算活数`/`阶段完成 Issue 数` 降级为**工作量旁注**(仍显示,但不再 derive 进项目健康信号)。

**工程对照**:
- **新指标定义**:在 aihot 项目建两条 metric——`连续产出日报天数`(lagging)、`每日命中率`(leading)。
  经 `practice_aihot.rs` 的 `setup`/`record-metric` 子命令建(真实 Command 路径,`create_metric`)。
- **真喂机制**(关键,零 mock):管线跑完把真数写进工作区一个 telemetry 文件(如
  `digests/telemetry.json`:`{date, raw, hit, deduped, items, days}`),`aihot/main.py` 落盘;BW 侧
  经 connector 采集或 `RunIssue` 的 evidence(`crates/bw-engine/src/evidence.rs`,`feed_workspace_metrics`
  的按名匹配模式)把 `hit/raw → 命中率`、数字digest 文件数 `→ 连续产出天数` 喂为 `SourceKind::Connector`/
  `Telemetry` 观测。**复用现成按名匹配**:给这两个指标起管线认得的名,`feed_workspace_metrics` 加两行
  映射即可,别为它写一次性代码。
- **降级 issue 指标**:把 `本周结算活数`/`阶段完成 Issue 数` 的 `role`/展示改为「工作量旁注」——不进
  `recompute_signals` 的项目健康派生(或标注 driver=工作量),UI 上与产品信号分区显示。
- **阈值**:命中率绿≥8%(今日实测 30/238≈12.6%)、连续产出天数「逐日增即绿、断档转黄」——
  阈值写进 metric 的 `amber_kind`/`amber_value`,不硬编 UI。

**DoD**:
- `sqlite3 … "SELECT name, role, signal FROM metric WHERE project_id='b7971eca…' AND name IN
  ('连续产出日报天数','每日命中率');"` 读回两条,`signal` 由真实观测派生(非 unknown、非手填);
  跑一次真实 `main.py` 生成当日日报后,`每日命中率` 观测值 = `命中/原始` 真数(`source_kind` 非 manual)。
- 深链到 aihot 进度面板 computer-use 截图:产品信号区显示这两条,issue 数落到工作量旁注区。

## 2. 验证纪律(每条工作线都照此,不靠单测)

- **深链启动 = 渲染证明**:`BW_DB=practice-aihot/bw-aihot.db BW_OPEN="aihot 日报" BW_PANEL=<panel>
  cargo run -p app-desktop`,stderr `[BW_OPEN]` 出现即渲染成功、无 panic。
- **sqlite 读回为证**:界面上每个数字都能 `sqlite3` 独立查回,读回不一致 = bug。
- **computer-use 抽查**:UI 结构/新面板用 computer-use 截图存档(注意:未打包 debug 二进制可能被
  `request_access` 拒,见今日实测——那就以深链 stderr + sqlite 读回为准,截图尽力而为)。
- **门禁全过**(每 commit 前,与 CI 一致):`cargo fmt --all --check` / `cargo clippy --workspace
  --exclude app-desktop -D warnings` / wasm32 双 check / `guard-kernel-ui-free.sh` /
  `cargo check -p app-desktop` / `/code-review`。UI 改动只准进 `app-desktop`(内核 UI-free 守卫)。
- **老库不崩**:任何 schema 动作(K1 若给 VM 加字段不涉及 schema;若涉及新列)必须 `schema.sql` +
  `add_column_if_missing` 双守卫,开 `demo-workspaces/bw-demo.db` 老库 `PRAGMA` 读回新列不崩。

## 3. 交接 / 风险 / 留白

- **K0 是地基,先做**:真执行器若仍被登录/配额挡,如实 Block,K3 退回「已有真实产物验证刷新」,
  **不假装跑过**;把真实探测结果(status/error)记进 `iterations/PRACTICE-AIHOT.md` 并更新
  `[[bw-real-executor-pending-verification]]`。
- **project_id 查询收窄(plan/08 P2)本计划只做「读侧过滤」**:侧边栏按 `project_id` 过滤展示即可;
  指派下拉/技能注入的全量收窄仍**留口不做**,如实标注(避免 scope 蔓延)。
- **共享 vs 自有的归属**:`project_id IS NULL` = 共享/市场引入,侧边栏单列一组,**不把共享组件冒充
  项目自建**(归属如实,append-only 审计精神)。
- **卡片字段诚实**:K2 凡无真实来源的字段一律「—/无运行证据」,**绝不 mock 组件战绩**。
- **commit 约定**:每件独立 commit,代号前缀 `K0-/K1-/…`,信息如实描述取舍与偏差;与源码冲突
  以源码为准,拿不准写进 commit message 偏差段留下一棒。
