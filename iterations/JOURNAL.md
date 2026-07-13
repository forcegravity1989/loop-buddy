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

## Iter 12 · 习惯画像(优化智能 7/7)—— Arc 2 闭环

- **原型师**:前面 6 个分析是分散的——失败、形状、健康、建议各自独立。**假设**:用户要的是"这个工作流整体怎么用"的一句话画像,不是 6 个数字。**DoD**:组合画像 + 人话摘要。
- **构建师**:`HabitProfile`(health/tier/shape/trigger_split/summary)+ `UsageTier{Hot,Warm,Cold}`(≥10/1-9/0)+ `habit_profile(analytics, usage, shape)` 纯函数组合 iter 8/11 + 一行人话摘要("热门·绿色·3阶段·主要手动触发·典型耗时 200ms")。
- **优化师**:tier 三档粗粒度(人能推理的桶,非连续分);摘要按"冷热·健康·形状·触发·耗时"五维,可读性优先;冷+Unknown 一致(无运行=无证据)。+2 测试。
- **运营推广师**:hub 每个工作流一句话画像——这是 iter 19 习惯驱动默认参数的直接依据,也是 PM 模板"使用现状"的样例数据。
- **运维师**:纯组合无副作用;tier 阈值集中可调。**Arc 2 优化智能完成——五角色环闭。** 回流:画像 → iter 19 默认推断 + iter 24 旅程可视化。

**门禁**:fmt clean · clippy clean · analysis 共 15 测试。

## Iter 13 · 建议评审/应用管线(自改进闭环 1/8)

- **原型师**:iter 9 能产建议,但建议不会被应用——闭环断在"知道了不行动"。**假设**:但全自动应用危险(改 prompt/退役是外向操作)。需要"度量门控":哪些可自动、哪些必须人工。**DoD**:建议 → 门控决策(AutoApply/DeferToHuman/Reject)。
- **构建师**:`ApplyDecision{AutoApply, DeferToHuman(reason), Reject(reason)}` + `ApplyPolicy{min_sample, cadence_demand_floor}`(可调的"自治度旋钮")+ `review_proposal(proposal, settled, policy)` 纯函数。
- **优化师**:样本地板(min_sample=5):低于此一律 Reject(一条 100% 是噪声);**只有 PromoteTemplate 自动**(正向·低风险·只增选项不删);FixFailure/Simplify/Retire 一律人工(改内容/破坏性);TuneCadence 可逆但仍建议人工确认(影响下次触发时机)。+2 测试。
- **运营推广师**:这是 iter 18 闭环自驱的"刹车"——系统能自己跑优化循环,但破坏性动作必须人确认。PM 模板"度量门控决策"的样例。
- **运维师**:门控纯函数可单测;policy 可调(min_sample/地板旋钮);默认保守。**回流**:决策 → iter 18 自驱循环执行 AutoApply、搁置 Defer。

**门禁**:fmt clean · clippy clean · analysis 共 17 测试。

## Iter 14 · 版本 A/B 对比(自改进闭环 2/8)

- **原型师**:iter 5 存了版本史,iter 13 能应用建议——但"优化后到底变好没"答不上来。**假设**:无反馈的优化是盲改,A/B 对比给"该回滚还是继续"。**DoD**:版本前后 run 切片对比 → delta + 判定。
- **构建师**:`VersionDelta`(前后 settled/成功率/中位耗时 + delta)+ `AbVerdict{Improved,Regressed,Inconclusive}` + `ab_compare(before, after)` 纯函数;`slice_stats()` 提取每侧统计。
- **优化师**:**双侧各需 ≥3 settled** 才下判定(否则 Inconclusive,绝不在薄数据上喊"改善了");成功率是主信号(±10% 阈),平局时耗时打破(±500ms);Regressed 明确给出(支持回滚决策)。+3 测试。
- **运营推广师**:一句话——"v2 比 v1 成功率 +50pp,耗时 -300ms → 改善,保留"。喂 iter 20 成效报告 + iter 18 闭环(回归则触发告警)。
- **运维师**:纯函数;Inconclusive 不等于"没问题"(诚实区分"没变化"和"看不出");回滚判定可执行。**回流**:A/B → iter 18 自驱的回归检测 + iter 20 成效。

**门禁**:fmt clean · clippy clean · analysis 共 20 测试。

## Iter 15 · 场景聚类(自改进闭环 3/8)

- **原型师**:iter 8 的形状是"平均形状",但用户可能用 N 种方式跑同一工作流。**假设**:平均掩盖多场景;优化要服务"真实场景"不是"平均"。**DoD**:按 (phase_count, trigger) 签名聚类出场景。
- **构建师**:`Scenario`(label/count/success_rate/median)+ `cluster_scenarios(runs)` 纯函数;按 (params phase_count, trigger) 签名分桶,每桶算自己的统计;RunTrigger 加 `Hash` 派生(做 HashMap 键)。
- **优化师**:每场景独立成功率("3阶段·手动 90% vs 5阶段·定时 40%"——揭示定时那条线在拖后腿);最大场景在前;phase_count 缺失归"未知阶段"不丢。+1 测试。
- **运营推广师**:一句话——"这个工作流有 2 种用法,定时的那种总失败"——精准定位优化目标场景。喂 iter 19 默认推断("默认走成功率高的场景")。
- **运维师**:RunTrigger 加 Hash 是加法不破坏;纯函数;聚类维度可扩。**回流**:场景 → iter 17 推荐("当前像哪个场景,推那个场景的绿色工作流")。

**门禁**:fmt clean · clippy clean · analysis 共 21 测试。

## Iter 16 · 跨阶段复用智能(自改进闭环 4/8)

- **原型师**:5 阶段标准模板是"方法论资产",但不知哪个阶段的模板真在被复用、哪个沉睡。**假设**:某阶段模板零复用=方法论落地失败(或该阶段无需求)。**DoD**:每阶段复用统计。
- **构建师**:`StageReuse`(stage/workflow_count/total_runs/run_share)+ `cross_stage_reuse(ranking)` 纯函数,从 UsageRank 按 stage_ref 聚合;零工作流的阶段也出现(冷阶段是信号不是隐藏)。
- **优化师**:五阶段全覆盖(ALL 遍历,冷的也在);run_share 占比量化"哪个方法论最重";unscoped(metrics 层)单独计。+1 测试。
- **运营推广师**:一句话——"原型阶段模板最热(占 60%),运维阶段零复用——要么补运维场景,要么这阶段不该有模板"。PM 模板"方法论落地度"的样例。
- **运维师**:纯函数;冷阶段不隐藏(诚实)。**回流**:复用度 → iter 17 推荐 + iter 23 PM 模板的方法论覆盖度。

**门禁**:fmt clean · clippy clean · analysis 共 22 测试。

## Iter 17 · 推荐引擎(自改进闭环 5/8)

- **原型师**:用户面对一堆工作流不知下一步跑哪个。**假设**:给定当前阶段,系统该主动推荐"放心的那个",而不是让用户猜。**DoD**:按阶段+健康推荐,带理由。
- **构建师**:`Recommendation`(workflow_id/name/why)+ `recommend_for_stage(stage, candidates)` 纯函数;选优规则:green>unknown>amber(按健康序),同档取最热,never red。
- **优化师**:**红色绝不推荐**(坏了再热也不推);**给 unknown 机会**(新工作流不因没数据被永远饿死——跑一次给它积累证据);why 必带信号("同阶段·绿色·已跑 20 次")。+2 测试。
- **运营推广师**:hub 顶部一句话"下一步:跑「构建·巡检」(同阶段·绿色·20次)"。降低选择摩擦,自然导向健康工作流。
- **运维师**:纯函数;确定性排序(健康→热度→名字);红色排除是硬规则。**回流**:推荐 → iter 18 自驱循环可"自动跑推荐项"+ iter 22 仿真器。

**门禁**:fmt clean · clippy clean · analysis 共 24 测试。

## Iter 18 · 闭环定时优化运行(自改进闭环 6/8)—— 核心引擎

- **原型师**:前 17 轮造了所有零件,但没组装成"自己跑起来的循环"。**假设**:目标的"通过 schedule 不断优化 workflow 本身"需要一个可被定时驱动的 measure→propose→gate 闭环。**DoD**:`App::run_optimization_cycle()` 一调,全 hub 自动度量+建议+门控+出报告。
- **构建师**:`OptimizationReport`(scanned/proposals/auto_applied/defer/rejected)+ `Event::OptimizationCycleReported` + `App::run_optimization_cycle()`:遍历 hub → 每工作流取 analytics+usage+runs+failures → `propose_optimizations` → `review_proposal` 门控 → 分类 AutoApply/Defer/Reject → 报告 + 事件。
- **优化师**:修复门控 bug——Retire 是"关于零运行"的建议,样本地板不该拦它(0 run 不是证据不足,而是建议本身的前提)→ Retire 在地板前分流;冷工作流 analytics 名为空 → 从 spec 补名(诚实);AutoApply 仅 PromoteTemplate(正向),其余人工。+1 集成测试验证 热+可靠→自动、冷→人工。
- **运营推广师**:一句话——"扫了 N 个工作流,发现 M 个机会,自动标记 K 个达标,余下 J 个待人"。这是目标"自驱优化"的可运行证据。iter 22 仿真器会定时驱动它。
- **运维师**:门控保守(破坏性永不自动);报告是只读 Receipt(真实计数);事件可订阅。**回流**:闭环 → iter 20 成效(多轮跑下来 hub 整体变好了吗)+ iter 22 定时驱动。

**门禁**:fmt clean · clippy clean · **全 121 测试通过**(基线 87 → +34)。

## Iter 19 · 习惯驱动默认参数(自改进闭环 7/8)

- **原型师**:新建工作流的默认参数写死,不随习惯变。**假设**:用户 70% 用 3 阶段,新工作流默认就该 3 阶段——"贴近习惯"从默认值开始。**DoD**:从习惯画像推断新工作流默认。
- **构建师**:`SuggestedDefaults` + `infer_defaults(profile)` 纯函数;LoopConfig 加 PartialEq/Eq。
- **优化师**:**60% 主导阈值**——只有清晰主导才成默认(无主导→None,不瞎猜);50/50 触发比 neutral。+2 测试。
- **运营推广师**:新建工作流预填"3阶段·loop(1,3)·定时"——用户改得越少=越贴合。目标"贴近习惯"在创建入口落地。
- **运维师**:纯函数;阈值集中;None 诚实。**回流**:默认推断 → iter 21 仿真器 + iter 24 旅程。

**门禁**:fmt clean · clippy clean · analysis 共 26 测试。

## Iter 20 · 优化成效报告(自改进闭环 8/8)—— Arc 3 闭环

- **原型师**:单工作流 A/B(iter 14)有了,但"hub 整体在变好吗"答不上来。**假设**:自驱循环的价值需要 scoreboard 验证——改善数 > 回归数才证明循环在挣钱。**DoD**:hub 级成效汇总。
- **构建师**:`EffectivenessSummary`(compared/improved/regressed/inconclusive/avg_rate_delta/avg_duration_delta_ms/verdict)+ `summarize_effectiveness(deltas)` 纯函数,聚合多工作流 VersionDelta。
- **优化师**:Inconclusive **不计入均值**(无 delta 不能稀释)但仍计数(不藏"看不出");avg_rate_delta 正=变好;verdict 一行可读("改善 3 / 回归 1 · 平均成功率 +12pp")。+1 测试。
- **运营推广师**:一句话——"过去 N 次优化:3 改善 1 回归,平均成功率 +12pp"——循环价值的量化证明。喂 iter 24 旅程可视化 + iter 25 报告。
- **运维师**:纯聚合;Inconclusive 诚实计数不藏。**Arc 3 自改进闭环完成。** 回流:成效 → iter 24 旅程指标曲线 + iter 25 报告结论。

**门禁**:fmt clean · clippy clean · analysis 共 27 测试。

## Iter 21 · 真实场景仿真器(演示与模板 1/5)

- **原型师**:要演示自驱循环(iter 18),但 MockExecutor 只能全成功——造不出"失败/冷门/回归"的真实场景。**假设**:需要一个确定性播种器,合成真实形状的运行史(热/温/失败/冷),让所有分析函数有真数据可嚼。**DoD**:可跑的仿真器,播种已知场景。
- **构建师**:`examples/simulate_hub.rs`:4 工作流(原型/构建/优化/运维各一阶段)+ 两周合成运行史;`seed_run()` 直接经 Store 写入受控结果的 `workflow_run` 行(诚实:是真行,只是合成);末尾打印每个工作流的健康派生。
- **优化师**:直接写 Store 行(而非走 MockExecutor 全成功路径)以精确控制成败比;单调合成时钟(1 run=1h)确定性可复现;同一根因"执行超时"复用于失败行(测 iter 7 聚类)。运行验证:原型=Green、构建=Amber(75%)、优化=Red(28%)、运维=Unknown(0)——全部正确派生。
- **运营推广师**:这是 iter 22 跑通自驱闭环的"用户习惯输入"。目标的"贴近用户场景"有了可重复的样本。
- **运维师**:确定性(无 RNG,可复现);直接写真实行不破坏不变量;example 不进 default-members 保持内核测试快。**回流**:仿真器 → iter 22 驱动循环 + iter 25 报告截图。

**门禁**:fmt clean · clippy clean · examples 编译通过 · 仿真运行输出正确健康派生。

## Iter 22 · 跑通自优化闭环(演示与模板 2/5)

- **原型师**:iter 18 造了循环引擎,iter 21 造了仿真器——但没端到端演示"循环真的让 hub 变好"。**假设**:目标要的证据是"度量→建议→修复→再度量→改善"的可跑闭环。**DoD**:example 跑完整弧。
- **构建师**:`examples/self_optimize_demo.rs`:播种失败工作流(29%)→ 跑循环得 FixFailure 建议 → 人工 UpdateWorkflowSpec(记 note)→ 播种修复后一周(57%)→ A/B 对比 + 成效汇总 → 第 2 轮循环复检。
- **优化师**:诚实处理边界——改善(29%→57%,A/B=Improved,+57pp)但未达绿(57%<80%),循环第 2 轮**仍如实标记待优化**(不撒谎说"已完成");cutoff 按时间切 before/after。运行验证全链路真实。
- **运营推广师**:这是目标的"通过不断执行优化 workflow 本身"的可运行证据——一个失败工作流被诊断、修复、证明改善的全过程。喂 iter 25 报告的核心演示。
- **运维师**:确定性可复现;直接写真行;A/B 切分用时间(版本切换点)诚实。**回流**:演示输出 → iter 25 报告的"最终形态演示"。

**门禁**:fmt clean · clippy clean · 演示端到端跑通(A/B Improved +57pp)。

## Iter 23 · PM 模板抽取(演示与模板 3/5)

- **原型师**:25 轮跑下来,过程本身是"可复用的项目管理方法",但还没抽象成模板。**假设**:把"五角色五阶段环 + 每圈清单 + 诚实约束"抽成文档,别人能照着跑。**DoD**:可套用的 PM 模板,以 WorkflowHub 自身为样例。
- **构建师**:`iterations/PM-TEMPLATE.md`:环总览(线闭成环图)+ 每阶段(角色/方法论/心法/输入/动作/出口/反模式/样例)+ 每圈最小清单 + 三条诚实约束 + 套用四步。
- **优化师**:样例列每条都指向真实 iter(可核对,非空话);清单是 checkbox 可复制;反模式成对(该做什么 vs 不该做什么);模板从复盘来非理论。
- **运营推广师**:这份模板的可信度来自"WorkflowHub 用 25 轮证明过它"——样例列就是证据。喂 iter 25 报告的"PM 模板演示"章节。
- **运维师**:模板文档化(不随代码漂移);每阶段出口可检验;诚实约束不可妥协。**回流**:模板 → iter 25 报告。

**门禁**:模板成文,样例可核对。
