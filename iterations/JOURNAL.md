# 25-Iteration Journal — 五角色五阶段环自举 WorkflowHub

> 每轮 = 一次完整五角色环。证据 = 真实代码 + 真实 git log + 真实可跑测试。
> 门禁每轮跑:`fmt --check && clippy -D warnings && cargo test`。

---

## Iter 01 · 运行结果追踪(数据基座 1/5)

**环位**:Prototype → Build → Optimize → Growth → Ops → (回流 Prototype)

- **原型师 · 求真(假设驱动探索)**:现状只记 run 发生过(`uses` 计数 + session 消息),不记成败/耗时/触发源。**假设**:没有"这次跑了多久、成了没"的颗粒度,任何"该不该优化"的判断都是瞎猜。**DoD**:每次运行(manual + 定时)落一条 append-only 记录,带 status/duration/phases/trigger;失败也如实记。
- **构建师 · 求成(规格驱动交付)**:新增 `WorkflowRunId` + `RunStatus{Running,Ok,Failed}` + `RunTrigger{Manual,Scheduled}` + `WorkflowRun` 结构;schema 加 `workflow_run` 表(append-only,start 入一条,engine 返回时 settle 一次);Store 加 `record_workflow_run_start`/`settle_workflow_run`/`list_workflow_runs`/`list_all_workflow_runs`;`run_workflow_inner` 加 `trigger` 形参,用 `Instant` 计真实耗时,3 个调用点(RunWorkflow/RunHubWorkflow/tick_scheduler)分别传 Manual/Manual/Scheduled。
- **优化师 · 求简(度量驱动打磨)**:settle 幂等(已终态的行不再被改写——dogfood 重跑不污染历史);`phases_completed` 在 move 前捕获;失败路径也走 settle(不全跑就崩也能记)。新增 4 个测试覆盖 start/settle 幂等 / 成功记 Ok+duration / 失败记 Failed+partial phases / 定时触发归因 Scheduled。
- **运营推广师 · 求增(增长实验)**:验证真实场景——用户跑一个工作流,历史里能看到"成功·2阶段·真毫秒";定时任务自动触发的运行被正确归因为 Scheduled(可与手动区分,后续分析能分渠道)。`workflow_name` 快照防 spec 改名后历史失真。
- **运维师 · 求稳(可靠性工程)**:新表 `CREATE TABLE IF NOT EXISTS` 对老库安全(不碰既有列,无需 add_column 守卫);`project_id`/`session_id` 可空(hub 运行不必绑项目);外键有意省略(run 可比 spec 长寿);**回流**:这批数据是 iter 2(运行分析)要聚合的粮食——环闭。

**门禁**:fmt clean · clippy clean · **91 tests pass / 0 fail**(基线 87 → +4)。
**提交**:见 git log `workflow_run` telemetry。

## Iter 02 · 工作流运行分析(数据基座 2/5)

- **原型师**:有了运行记录,但仍是行级原始数据,没有聚合视图。**假设**:优化决策要的是"这个工作流成功率 75%、典型耗时 250ms"这种一句话判断,不是逐行翻日志。**DoD**:`workflow_analytics(id)` 一调出 per-workflow 聚合。
- **构建师**:`WorkflowRunAnalytics` 结构(total/ok/failed/running 计数、success_rate、avg+median duration、last_run_at/status);单条 SQL 聚合 + Rust 算 median(中位数,抗单个慢离群点)。
- **优化师**:median 用中位数而非均值(一个慢 run 不污染"典型成本");`success_rate` 在无 settled 记录时为 `None` 而非 0("未知"≠"总失败",镜像 `Signal::Unknown`)。+1 测试覆盖 3ok/1fail → 0.75 + median 250。
- **运营推广师**:一句话能说清"这个工作流健康吗"——成功率 + 典型耗时 + 上次状态,是后续冷热榜/健康信号的直接输入。
- **运维师**:无 settled 时返回 total=0 而非报错(调用方能诚实显示"未运行");只读查询,无写入风险。**回流**:这聚合喂给 iter 6 冷热榜 + iter 11 健康信号。

**门禁**:fmt clean · clippy clean · **92 tests pass**(+1)。

## Iter 03 · 参数捕获(数据基座 3/5)

- **原型师**:运行记录了成败,但不知道"这次跑的 spec 长什么样"。**假设**:spec 会被优化(改阶段/改 loop),改完之后历史就看不出"当时跑的是哪个版本"——版本演化断了链。**DoD**:每次运行冻结一份 spec 形状快照。
- **构建师**:`run_params_snapshot(spec, trigger)` 纯函数产出 JSON(phases/phase_count/loop/agents/skills/stage_ref/trigger/kind 含版本号);`record_workflow_run_start` 重构为收 `NewWorkflowRun` 参数结构体(clippy 推的 8→1 参数收敛,顺带对齐 `New*` 模式);run 开始时写入 params_json。
- **优化师**:clippy `too many arguments` 8/7 → 重构成 `NewWorkflowRun<'a>` 结构(对齐 NewProject/NewSession 模式);serde_json::Value 保证字段增删是加法不改历史行;`kind` 带 `static:v{version}` 让动态/静态可辨。+1 测试验证 phase_count/max_iter/stage_ref/trigger 都被快照。
- **运营推广师**:真实场景——一个工作流被优化了 3 次,历史里每条 run 仍能说出"我跑的是 v2、4 阶段、max_iter=5"。这是 iter 5 版本快照 + iter 14 A/B 对比的地基。
- **运维师**:`params_json` 默认 '' 对 iter 1 老行向后兼容;纯函数无 IO 无副作用;id 仍在 store 内部生成(调用方丢不了 settle 句柄)。**回流**:喂给 iter 8 参数频率分析。

**门禁**:fmt clean · clippy clean · **93 tests pass**(+1)。

## Iter 04 · 调度有效性度量(数据基座 4/5)

- **原型师**:定时任务跑了,但"跑完有没有用"答不上来。**假设**:一个每天触发的巡检任务若连续失败却没人知道,就是白烧 run。**DoD**:每个 cron task 有"有效性分"(自动触发次数/成功数/典型耗时),且**只算定时触发**、不被手动运行污染。
- **构建师**:`workflow_run` 加 `cron_task_id` 列(`add_column_if_missing` 守卫,老库安全);`run_workflow_inner` 多透一个 `cron_task_id: Option<CronTaskId>`,调度器传 `Some(c.id)`、手动两路传 `None`;`CronEffectiveness` 结构 + `cron_effectiveness(task_id)` 聚合(trigger='scheduled' AND cron_task_id=? 过滤)。
- **优化师**:`effectiveness` 未触发时 `None`(无证据≠0%);last_fire_ok 二次查询读最近一次(避开 window 函数依赖);非 FK 约束(run 比 task 长寿,task 删了 run 仍是诚实证据)。+1 测试验证手动运行不计入、定时触发后 effectiveness=1.0。
- **运营推广师**:一句话判断"这个定时任务该不该留着"——fires / 成功率 / 典型成本。直接喂 iter 10 节奏自调 + iter 18 闭环自驱。
- **运维师**:迁移守卫(真实事故教训:`CREATE TABLE IF NOT EXISTS` 不加列);cron_task_id 可空,手动行 NULL;老 DB 开 open() 自动加列。**回流**:调度有效性的"成功/失败"回流到 iter 7 失败模式检测。

**门禁**:fmt clean · clippy clean · **94 tests pass**(+1)。

## Iter 05 · 版本快照(数据基座 5/5)—— Arc 1 闭环

- **原型师**:`UpdateWorkflowSpec` 优化工作流时直接覆盖旧内容——版本号 +1 但旧 prompt/goal/phases 永久丢失。**假设**:优化是演化的,丢掉"改之前是什么样"就没法回答"这次优化到底改了啥、为什么、有没有变好"。**DoD**:每次优化前冻结一份历史版本 + 记原因。
- **构建师**:`workflow_version` append-only 表(version/name/prompt/goal/phases/agents/skills/loop/note/created_at);`update_workflow_spec` 改为先 SELECT 当前内容 → INSERT 进历史 → 再覆盖;`WorkflowEdit` + `Command::UpdateWorkflowSpec` 加 `note`(改的原因);`list_workflow_versions(id)` 读回演化链。
- **优化师**:`WorkflowKind::Static` 重建时保留全部既有字段(maturity/uses/scope/source/trigger),只 bump version——不丢元数据;version 号语义清晰("version N 的内容,在写 N+1 时被冻结")。+1 测试验证两次优化 → 2 条历史(version 1、2),live=v3,note 各自冻结。
- **运营推广师**:真实场景——一个工作流被优化了 5 次,用户能回看每一次改了什么、为什么("失败率 12%→加回归检查")。这是 iter 14 A/B 对比 + iter 23 PM 模板"演化叙事"的地基。
- **运维师**:历史表 append-only(永不改写);非 FK(version 比 spec 长寿);`note` 默认 '' 向后兼容;examples(verify_goal)+ desktop UI 同步加 note。**回流**:演化链喂给 iter 9 优化建议("上次为什么改")+ iter 20 成效 delta。**Arc 1 数据基座完成——五角色环闭。**

**门禁**:fmt clean · clippy clean · 全测试 0 失败 · dogfood 端到端仍跑通(27 session / 5 闭环交接)。

## Iter 06 · 使用频率分析(优化智能 1/7)

- **原型师**:hub 里有几十个工作流,但不知道哪些真在被用、哪些是僵尸。**假设**:一个从不被跑的工作流是认知负担 + 维护税,该退役或优化;一个高频的该被爱护。**DoD**:全局冷热榜,从最热到最冷。
- **构建师**:`UsageRank` 结构 + `hub_usage_ranking()` Store 方法;LEFT JOIN workflow_spec↔workflow_run,按真实运行数降序,`cold=true` 标记零运行。
- **优化师**:排名用 append-only 日志的真实计数,不用可能漂移的 `uses` 计数器;冷工作流 `success_rate=None`(无证据≠差);LEFT JOIN 保证零运行也上榜(否则隐身)。+1 测试验证 热(3)>中(1)>冷(0,cold=true)。
- **运营推广师**:一句话运营判断——"这 3 个工作流这周一次没跑,考虑退役;这 2 个高频但成功率才 60%,优先优化"。喂 iter 9 建议 + iter 17 推荐。
- **运维师**:只读聚合,无写入;GROUP BY ws.id 稳定。**回流**:冷热是 iter 11 健康信号的输入之一。

**门禁**:fmt clean · clippy clean · **run_outcome 9 tests pass**(+1)。

## Iter 07 · 失败模式检测(优化智能 2/7)

- **原型师**:失败是一团散沙——每条 run 一个 error 串,看不出"根因"。**假设**:若 10 次失败里 7 次是同一个根因,修那一个就消掉 70% 失败;但散沙状态下看不出来。**DoD**:按归一化根因聚类,频次降序。
- **构建师**:新建 `bw-core::analysis` 纯函数层(无 IO/无 async,wasm-clean);`failure_modes(runs) -> Vec<FailureMode>` 按 `normalize_cause`(去 `:`/`—`/`(`/换行 + 小写)聚类根因,带 count + affected_workflows(跨工作流=系统性问题)+ last_seen。
- **优化师**:纯函数 → 合成数据单测,零 DB 开销;归一化剥离 ` (retry N)` 等易变后缀让同根因合并;affected_workflows 区分"单工作流问题"vs"系统性问题";空输入返回 `[]` 不报错。+2 测试。
- **运营推广师**:一句话——"修'模拟·第二步失败'(7次,1工作流)能消掉 70% 失败"。喂 iter 9 优化建议的"失败驱动"输入。
- **运维师**:纯函数无副作用无写入;`analysis` 模块挂 bw-core(复用 derive 链"原始值进、判断出"的同构)。**回流**:失败模式 → iter 11 健康信号的 Red 判据之一。

**门禁**:fmt clean · clippy clean · analysis +2 测试通过。

## Iter 08 · 参数频率分析(优化智能 3/7)

- **原型师**:每次运行的 params 快照有了(iter 3),但谁也不看原始 JSON。**假设**:用户的"习惯"藏在多数 run 的 phase_count/loop 配置里——若 70% 的 run 都用 3 阶段,那"3 阶段"就是该工作流的自然形状,默认值该跟它走。**DoD**:从 run 群里抽出主导形状 + 置信度。
- **构建师**:`RunShapeProfile`(dominant_phase_count / dominant_loop / trigger_split)+ `run_shape_profile(runs)` 纯函数:解析每条 params_json,直方图取众数 + 占比;malformed JSON 容错跳过。
- **优化师**:`mode()` 泛型取众数,平局按最小键(确定性);share 用"解析成功数"作分母(不被 malformed 稀释);serde_json 进 bw-core deps(wasm-clean,已验证 wasm32 keepalive 仍过)。+2 测试。
- **运营推广师**:"这个工作流 70% 的运行是 3 阶段·retries=1·手动触发"——这是 iter 19 习惯驱动默认参数的直接依据,也是 iter 12 习惯画像的组成。
- **运维师**:解析容错(bad JSON 不崩,只跳过);wasm32 编译验证通过(Web 留口不破)。**回流**:形状 → iter 12 习惯画像 → iter 19 默认参数推断。

**门禁**:fmt clean · clippy clean · wasm32 keepalive 过 · analysis +2 测试。

## Iter 09 · 优化建议生成(优化智能 4/7)

- **原型师**:有了成败/频率/失败模式/形状,但仍是数据,不是"该做什么"。**假设**:用户的认知负荷在看数字上,不在做决定上——系统该把数据翻译成"先修 X(因为 Y)"的可执行建议。**DoD**:给定 analytics+usage+failures+effectiveness,产出排好序、带证据链的建议。
- **构建师**:`OptimizationProposal`(kind/title/rationale/priority)+ 5 种 `ProposalKind`(Retire/FixFailure/Simplify/TuneCadence/PromoteTemplate);`propose_optimizations(...)` 纯函数按阈值生成建议并按严重度排序。
- **优化师**:每条建议**必有证据**(引用具体数字:成功率/失败次数/耗时),不是空话"该优化";阈值文档化(成功率<80%、冷=0运行、定时<50%、中位>5s、热+可靠≥95%);"健康且温暖"时不产建议(不制造噪音)。+3 测试。
- **运营推广师**:一句话行动——"先修'网络超时'(7次,消 70% 失败)" > "成功率 60%"。这是 iter 13 建议应用管线的输入,也是 PM 模板"优化决策"的样例。
- **运维师**:纯函数无副作用;阈值集中可调;PromoteTemplate 是失败检查的正面镜像(对称设计)。**回流**:建议 → iter 13 应用管线 → iter 18 闭环自驱。

**门禁**:fmt clean · clippy clean · analysis +3 测试(共 7)。

## Iter 10 · 节奏自调建议(优化智能 5/7)

- **原型师**:定时任务的 cadence 写死,但需求在变。**假设**:用户在两次定时之间反复手动重跑同一工作流 = "节奏太慢"的信号;反之一直没人碰 = 可能刚好。**DoD**:基于有效性 + 手动补跑信号,给出"加密一步/保持"的建议。
- **构建师**:`CadenceSuggestion`(current/suggested/reason)+ `suggest_cadence(current, eff, manual_re_runs)` 纯函数;`more_frequent()` 一步加密(Weekly→Daily→RealTime);Cadence 加 `PartialEq/Eq` 派生(支持比较)。
- **优化师**:**失败的任务不调节奏**(先修,否则是噪音);只移动一步(不 Weekly 直跳 RealTime);**不主动建议降频**(静默少跑会藏回归,保持是安全默认);RealTime/Cron 已到顶时给"拆任务"提示。+3 测试。
- **运营推广师**:一句话——"用户在两次巡检间手动跑了 3 次,建议从周级提到日级"。喂 iter 18 闭环自驱的节奏调整。
- **运维师**:纯函数;Cadence 加派生是加法不破坏现有;增量一步保守。**回流**:节奏建议 → iter 13 应用管线 → iter 18 自驱。

**门禁**:fmt clean · clippy clean · analysis +3 测试(共 10)。

## Iter 11 · 工作流健康信号(优化智能 6/7)

- **原型师**:工作流自己没有 Green/Amber/Red——只有指标有。**假设**:用户扫一眼 hub 该能立刻知道"哪个工作流现在坏了",不该逐个翻运行历史。**DoD**:工作流派生健康信号。
- **构建师**:`workflow_health(analytics) -> Signal` 纯函数,**复用** derive 链的 `Signal{Green,Amber,Red,Unknown}`——工作流和指标同语义。
- **优化师**:**无证据=Unknown**(绝不给新工作流假绿,<2 settled 也 Unknown,样本为 1 不算记录);"成功率<50%=Red、50-80%=Amber、≥80%=Green";**最近一次失败=Amber**(回归早于均值暴露,即使长期率好看)。+3 测试。
- **运营推广师**:hub 墙上每个工作流带一个色点——和项目健康概览同一套"绿色隐身、红黄出声"的过滤逻辑直接复用。喂 iter 17 推荐("优先推荐绿色的")。
- **运维师**:复用 Signal 类型(零新枚举);纯函数无写入;Unknown 规则承载"无数据≠绿"的核心不变量。**回流**:健康信号 → iter 12 习惯画像的组成 → iter 20 成效报告。

**门禁**:fmt clean · clippy clean · analysis +3 测试(共 13)。
