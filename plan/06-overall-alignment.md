# 06 · 整体方案对齐 — multica × workbench 有机结合(合并后的单一事实)

> 写于 2026-07-15,§7-§8 同日复盘修订。此前存在两份平行设计:`plan/05-complete-form-design.md`(主线,九实体
> 完整形态,G1-G11)与 `iterations/V2-DESIGN.md`(`claude/bw-complete-form` 分支,multica
> Issue 层融合,R1/R2)。**本文档是两者的对齐收口**:代码已在 merge `c723480` 合为一体,
> 记账联动在 `fc5bded` 焊死。从本文件起,「完整形态」只有一个方案,不再有两条线。
> 每条陈述可对源码/测试/DB 核验;缺口如实标注,不假装。

---

## 0. 对齐结论(一句话)

**BW 的五阶段方法论环回答「为什么做、现在在哪、健不健康」;multica 式 Issue 层回答
「这件活谁接、跑到哪了」;两者经同一组真实记账函数(agent runs/wins、artifact 登记、
skill uses/蒸馏)汇进同一条度量派生链——这就是有机结合,不是两个面板并排摆放。**

## 1. 两份设计如何对齐(谱系)

| 来源 | 贡献 | 归宿 |
|---|---|---|
| `plan/05`(fable5+glm5.2,主线) | 九实体全实体化:skill 正文/agent 指令与记账/connector 真探针/artifact 版本史/all-in-one-codebase 自动开仓/五角色剧本/证据回流 | **保留为基座**,§G 台账仍有效 |
| `iterations/V2-DESIGN.md`(glm5.2,融合分支) | 融合洞察(§2)、Issue 状态机照 multica 真实语义(backlog=停车场)、R1 Issue 层、R2 Skill 复利(带 provenance)、Issue 看板 | **整体并入主线**(merge `c723480`) |
| 本轮新增(fable5) | 蒸馏技能必须带 `content` 正文(R2×G7 交汇);**Issue Done 边沿 = issue 侧 settle**(R1×G8/G5/G10 交汇);BW_PANEL 深链纳入 issues | commit `c723480` + `fc5bded` |

对照 multica 的诚实边界不变(V2-DESIGN §6.5 经真实源码核验):`issue.stage` 是 multica
真实存在的字段(BW 把裸 stage 增强为方法论 stage);R2 自动复利是 BW 相对 multica 的
**真实增量**(multica 的 skill 是手工创建的);BW 只借鉴概念,零代码复制。

## 2. 统一概念模型:十实体一张图

```
                    ┌────────────────────────────────────────────────┐
                    │  Project(五阶段方法论环:原型→构建→优化→增长→运维) │
                    │  cycle 配比 · DoD 证据谓词 · 交棒审计(append-only) │
                    └──────────┬─────────────────────┬───────────────┘
             作用域(stage)      │                     │ all-in-one-codebase
                    ┌──────────▼──────────┐   ┌──────▼───────────────┐
                    │ Issue(R1,可分配工作单元)│   │ Workspace(真实 git 仓) │
                    │ 7态:Backlog→…→Done    │   │ 自动开仓+绑 connector    │
                    │ assignee = Agent 队友   │   └──────┬───────────────┘
                    └───┬────────────┬──────┘          │ 真实文件/提交
        …→Done 边沿      │            │ DistillSkill     │
   (issue 侧 settle)     │            ▼ (带 content 正文) ▼
                        │       ┌─────────┐      ┌──────────────┐
                        │       │  Skill   │      │   Artifact    │
                        │       │ 正文(G7) │      │ project×path× │
                        │       │ 溯源(R2) │      │ commit=版本史  │
                        │       │ uses 记账 │      └──────▲───────┘
                        │       └────▲────┘             │ 幂等登记
                        ▼            │ 按名 uses+1        │
                 ┌────────────┐      │            ┌──────┴────────┐
                 │ Agent 队友   │◄─────┼────────────┤ WorkflowRun    │
                 │ instructions│ 按名记 runs/wins   │ settle(幂等)   │
                 │ runs/wins → │      │            │ ok/failed 如实  │
                 │ win_rate 派生│      │            └──────▲────────┘
                 └────────────┘      │                   │ 执行
                 ┌────────────┐  ┌───┴─────────┐  ┌──────┴────────┐
                 │ Connector   │  │ WorkflowSpec │  │ Cron Hub       │
                 │ 真探针喂观测  │  │ 五角色剧本    │  │ tick_scheduler │
                 └──────┬─────┘  │ phase_prompts│  │ 真实自动触发     │
                        │        └─────────────┘  └───────────────┘
                        ▼
              append-only Observation → recompute_signals → Derived<Signal>
              (L0→L6 派生链,Unknown 诚实态,无数据 ≠ 绿)
```

**两条 settle 路径,一组记账函数**(有机结合的技术核心):

| | workflow-run settle(主线原有) | Issue Done 边沿(本轮焊接) |
|---|---|---|
| 触发 | `RunWorkflow`/`RunStagePlaybook`/cron 真实执行落定 | `TransitionIssue` 读前态守卫的 …→Done 边沿 |
| agent 记账 | spec.agents 按名 `record_agent_run_by_name(ok)` | assignee 按名 `record_agent_run_by_name(true)` |
| artifact | 工作区扫描登记(绑 run id+阶段) | 工作区扫描登记(绑 issue 阶段) |
| skill | spec.skills 按名 `uses+=1` | Done→`DistillSkillFromIssue`(新技能,带正文+溯源) |
| 幂等 | settle 幂等 | 重复 Done 不重计;Cancelled 不记(弃活≠agent 表现证据) |
| 测试 | `complete_form.rs` 等 | `issues_skill_loop.rs::issue_done_edge_settles_agent_accounting_exactly_once` |

复利闭环:**Done Issue → 蒸馏 Skill(带 content)→ 下一次 run 经 `skills_prompt_block`
把正文真实注入 prompt → 该 skill `uses+1`** —— multica 的「经验沉淀」接上了 BW 的
「技能可执行」,沉淀物不是死卡片。

## 3. 统一 IA(桌面实景,非目标态畅想)

```
项目墙(2列卡+新建)· 创建流(意图→快问→起草→审阅;CompleteCreation 自动开仓)
└─ 项目运营视图
     ├─ 五阶段环轴(活跃阶段高亮 · 信号灯 · DoD · 交棒)
     └─ 工具栏:进度 | 工作流 | 例行 | 产物 | 版本 | Issue 看板
          · Issue 看板 = 5 列(待办池/待办/进行中/评审中/已完成),
            建卡默认作用域=当前活跃阶段,卡片带 assignee/优先级
          · 产物面板 = 真实登记行(路径/类型/字节/commit/版本数)
Hubs:Workflow / Skill(正文+溯源)/ Agent(指令+真实 runs·win_rate)/
      Connector(真探针)/ Knowledge(登记表,诚实留白)/ Cron(真实调度)
深链:BW_OPEN=<项目名> + BW_PANEL=progress|workflow|routine|artifact|version|issues
```

与 V2-DESIGN §3「目标态 IA」的差异如实列:Autopilots(cron→自动建 Issue 并路由)、
Activity/Inbox、Squad、多视图(Gantt/Swimlane)——**均未建**,留口不假装。

## 4. 分层职责(为什么两者缺一不可)

- **方法论环(BW 独有)**:阶段=角色=方法论(核心问题/方法循环/DoD/反模式),回答
  「这段该做什么性质的工作、做到什么算完、什么信号该警觉」。没有它,看板只是任务清单,
  「构建段该做什么」没有答案(multica 的空白)。
- **Issue 层(multica 借鉴)**:阶段里的实际工作拆成可分配单元,指派给真实 agent 队友,
  状态机推进。没有它,五阶段环是空转的仪表盘——「活」没有落点(BW 此前的空白)。
- **记账与派生链(BW 独有,两者的汇合点)**:干了活必留痕(runs/wins/artifact/uses),
  痕迹只进 append-only 观测,信号只经 `Derived<Signal>` 派生。**度量诚实是焊缝本身**:
  队友干活(multica 语义)必然变成证据(BW 语义),两个世界在此不可分。

## 5. 缺口台账合一(2026-07-15 快照)

| # | 缺口 | 状态 | 证据 |
|---|---|---|---|
| G1 角色可执行 | ✅ | playbook.rs + 五角色 agent 实体+按名记账 |
| G2 per-phase prompt | ✅ | phase_prompts + relay baton |
| G3 起草真实 | ◐ 诚实 Mock | 起草 run 仍 MockExecutor(标注清晰);项目出生即真工作区 |
| G4 真实 agent 端到端 | ◐ 外部制约 | 链路全通;GLM 网关 529;supervisor 重试;编排后端真实工程已落账(`c9920ae`) |
| G5 证据回流 | ✅ | evidence.rs + artifact 自动登记 + connector 喂观测 |
| G6 headless 驱动 | ✅ | real_demo + real_team_loop + supervise-real-demo.sh |
| G7 skill 正文+记账 | ✅ | content 列 + 注入 + uses 计数 |
| G8 agent 指令+记账 | ✅ | instructions + runs/wins/win_rate 派生 |
| G9 connector 同步 | ✅ | git-repo/claude-cli 真探针 |
| G10 产物实体 | ✅ | artifact 表 + 幂等版本史 + 面板真身 |
| G11 all-in-one-codebase | ✅ | CompleteCreation 自动开仓 |
| R1 Issue 层 | ✅ **已并主线** | merge `c723480`;7态状态机/per-project number/看板 |
| R2 Skill 复利 | ✅ **已并主线** | distill(Done+assignee 校验)+ 溯源 + **content 正文(本轮)** |
| R3 Issue↔记账联动 | ✅ **本轮新增** | Done 边沿 settle(`fc5bded`),测试锁死 |
| R4 settle-once | ✅ **本轮新增** | `issue.settled_at` 持久标记(DB 层 COALESCE 双保险):Done→重开→Done 不重复记账;测试 `reopened_issue_settles_only_once` |
| 留白 | 聊天 mock 回复 / 知识库登记表 / cron 表达式 / Autopilot 建 Issue / Squad / 多视图 | 诚实留口 | — |

## 6. 实跑纪律(「一切实跑」在本仓库的操作定义)

1. 演示数字一律从 demo DB / 工作区读回,报告不代答(`real_demo` evidence JSON 模式)。
2. mock 路径自我标注(【mock】前缀/文档注明),绝不冒充真实执行。
3. 信号只能经 `Derived<Signal>` 派生;观测 append-only;无数据 = Unknown ≠ 绿。
4. 真实 agent 执行被网关 529 挡住时:supervisor 幂等重试 + **编排后端真实工程**
   (真代码/真测试/真 commit)经工作台公开记账 API 落账——产物与度量全真,执行方
   如实标注(TAKEOVER-REPORT §4.1 先例)。
5. 门禁每步过:fmt / clippy -D warnings / test / wasm32×2 / kernel-ui-free。

## 7. 融合未补全清单(2026-07-15 复盘 · 以融合命题为尺)

> 尺子 = §0 的命题:「队友干活(multica)**必然**变成证据与信号(BW)」。
> 逐条问:断在哪、为什么算断、补法是什么。✅=本轮已修,其余入 §8 队列。

| # | 断链 | 为什么算断 | 补法(具体) |
|---|---|---|---|
| A | **Issue 可分配、不可执行** —— 没有 `RunIssue`,分配 ≠ 队友动工;工作流执行与 Issue 是两个并行世界,只在记账处汇合,正向(Issue→跑活)缺失 | multica 的核心是「指派即触发执行」;BW 现在 Done 是人/外部编排推的,app 内的 agent 从不因被指派而干活 | `Command::RunIssue{id}`:标题/desc + 阶段角色 preamble(playbook)+ `skills_prompt_block`(含蒸馏技能→**复利闭环补全**)拼 prompt → 走 `run_workflow_inner` 同链路 → 起跑推 `InProgress`,成功推 `InReview`(**agent 提议、人确认 Done**——保留 DoD 式人裁,Done 边沿记账不变);失败如实留在 InProgress + 事件。Mock 路径先行自检,网关回来即全真 |
| B | ~~Done→重开→Done 重复记账~~ | 公开命令面可达(桌面只给前进,但 API/未来 UI 能倒退);一件活两份账=假度量 | ✅ 已修(R4 settle-once,`settled_at` + COALESCE) |
| C | **Issue ↔ run/产物无数据关联** —— run 表没有 issue_id,artifact 也没有;「这件活跑到哪、产出什么」在 Issue 卡上答不出 | multica 的 issue 挂 task 流水与 PR;BW 的 Issue 详情面无从聚合 | `workflow_run.issue_id`、`artifact.issue_id` 两列(`add_column_if_missing`);Done 边沿扫描与 RunIssue 落 run 时写入;Issue 详情 = 它的 runs + 产物 |
| D | **度量派生链吃不到 Issue(L0 断供)** —— §0 说「信号从真实 Issue 结果派生」,实际 Done 边沿只记账不落观测,阶段信号对看板全盲 | 这是命题级违约:方法论环的健康度看不见 Issue 吞吐 | Done 边沿按 `feed_workspace_metrics` 的 change-guard 模式,给「阶段完成件数」类指标追加机器源观测(值变才追加);**无目标则信号诚实 Unknown**,绝不因喂数变绿 |
| E | **交棒对开放 Issue 视而不见** —— 阶段交棒时该段还有未完 Issue,交棒词可以只字不提 | BW 自己的诚实纪律(险交棒留痕)没吃到 Issue 数据 | `HandoffStage` 统计离段非终态 Issue:>0 则强制 `risky=true` 并在 note 自动追加「留 N 件未完」;不阻止交棒(人有权险交),只不许无痕 |
| F | **状态机无合法性守卫;Blocked 无原因** | `transition_issue` 接受任意跳转(Cancelled→Done 也行);阻塞没有 blocker 记录=不可行动 | App 层合法转移表(前进/重开/阻塞往返/取消;拒绝无意义跳转);`issue.blocked_reason` 列,转 Blocked 必填 |
| G | **Autopilot 缺位**(cron→自动建单) | multica `create_issue` 模式;BW 调度器已真,只差建 Issue 一步 | 规格已冻结:`iterations/HANDOFF-2026-07-15.md` P-A1(三守卫列/CronMode/tick 分支/四测试) |
| H | **看板交互半身** —— 卡片只有「前进」;不能从 UI 指派、不能带原因阻塞 | 可分配工作台的「分配」动作反而要走 headless | 卡片加指派下拉(五角色实体)+ Blocked(带原因输入);**倒退/重开故意不给 UI**(API 层有 settle-once 兜底) |

**定位性留白(不补,防蔓延)**:Squad、多 CLI provider、云 runtime/WebSocket、Gantt/泳道、
Inbox——multica 的多人/分布式面,与 BW 单人桌面定位相斥,见 §2.3。

## 8. 修订执行队列(v3 · 两轨,接棒照此走)

> v3 修订理由:v2 把 Autopilot 排在 RunIssue 前——**自动化建单在「活还不能被执行」时
> 上线,只会自动堆积待办**。执行顺序改为「先让一件 Issue 端到端跑通,再自动化创建」。
> 条目代号(A1-A5)不变,按下面顺序执行。

**即时可全真轨**(不等任何外部),执行顺序 **A2 → A3 → A0 → A4 → A1 → A5**:
1. **A2 · 关联列 C**(小,半日):`workflow_run.issue_id` + `artifact.issue_id`。
2. **A3 · RunIssue 结构**(1-1.5 天):正向执行环 + Mock 自检。**两个此前未定的设计点,现写死**:
   - **技能注入选择规则**:同项目内 `distilled_from_issue IS NOT NULL` 的技能,同阶段优先、
     按蒸馏时间倒序,**上限 3 条**,经现有 `skills_prompt_block` 注入(超限如实截断并记事件)。
   - **人确认 Done**:run 成功只推到 `InReview`;`Done` 必须由既有 `TransitionIssue` 显式触发
     (UI 按钮/命令),**绝不自动**。确认者身份(actor)本轮不建模,列为留口——但 InReview→Done
     的转移必须来自命令面而非 settle 内部,这一点用测试锁死。
3. **A0 · 复利闭环端到端测试**(30 分钟,可与 A3 同步):蒸馏技能 → 后续 run prompt 含其
   content → 该技能 `uses+1`,单测一条链锁死(现在就能写,不依赖 A3)。
4. **A4 · D+E 点灯**(1 天):Issue 观测喂入 + 交棒开放件检查。**前置漏洞,现写死**:
   「阶段完成件数」指标此前并不存在,观测无家可归——**`CompleteCreation` 起为每阶段播种
   一条机器喂养的 leading 指标**(名称常量化,如 `bw_app::METRIC_STAGE_ISSUES_DONE`;
   目标留空=信号诚实 Unknown,用户设目标后才可能变绿);存量项目 Boot 时补种(幂等,同
   `seed_stage_entities_if_missing` 模式)。Done 边沿按 change-guard 追加观测。
5. **A1 · Autopilot 建单**(半天,规格已冻结):此时建出的单已是「可执行的活」。
6. **A5 · F+H**(1 天):转移守卫/Blocked 必填原因/看板指派;项目墙卡片顺手带开放 Issue 计数。

**等外部轨**(用户动作后立即接):
- **B1 · G4 网关闭合**(等 token;401 判定与步骤见交接件)。**前置**:先定两条 stale
  `running` 行的收口策略(建议:标 Failed+注记「529 时代中断」),再跑新 run,免得口径混乱。
- **B2 · G3 起草真实 → B3 聊天真实回复**(依赖 B1)。**fallback**:若 token 长期不可用,
  B2/B3 可按编排后端先例(TAKEOVER §4.1)先行全真——真实工程+公开 API 落账+执行方如实
  标注;只有 G4 本身(app 内建 claude -p)必须等网关。
- **B4 · 知识库实体化 · B5 cron 表达式**(独立,随时可插)。
- **B6 · 像素核验 + 合并版 HTML 报告**(需屏幕录制授权;数字一律 DB 读回)。

> **2026-07-16 后续**:A 队列(A2/A3/A0/A4/A1/A5+M3)已全部落地;真实孵化已启动
> (linkcheck-md Issue #1 全真生命周期,`incubate_issue` 指挥器)。**MVP 收口队列
> (两线:项目的生命周期 P1-P5 × workflow 的生命周期 W1-W3)已冻结在
> [`plan/08-mvp-execution-plan.md`](08-mvp-execution-plan.md),接棒照那里走;
> 分支隔离/成本预算等挪进其 §5 加固层,不在 MVP。**

**M3 验收补一条「用户一天」场景**(纯读回,防构建者视角自嗨):
打开工作台 → 项目墙即见各项目信号灯与开放 Issue 数 → 进入某项目,五阶段环显示活跃段
与真实完成率 → 看板见 Autopilot 晨间建的单 → 一键 RunIssue → InReview 人审 → 确认 Done
→ 阶段指标观测+1、agent 记账+1、产物登记 → 交棒时未完件数如实拦截。**每一步的数字都
必须能用 sqlite3 独立复核**——这一趟跑通,「完整工作台」才算对用户成立。
