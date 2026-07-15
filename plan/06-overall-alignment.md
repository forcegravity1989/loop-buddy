# 06 · 整体方案对齐 — multica × workbench 有机结合(合并后的单一事实)

> 写于 2026-07-15。此前存在两份平行设计:`plan/05-complete-form-design.md`(主线,九实体
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
| 留白 | 聊天 mock 回复 / 知识库登记表 / cron 表达式 / Autopilot 建 Issue / Squad / 多视图 | 诚实留口 | — |

## 6. 实跑纪律(「一切实跑」在本仓库的操作定义)

1. 演示数字一律从 demo DB / 工作区读回,报告不代答(`real_demo` evidence JSON 模式)。
2. mock 路径自我标注(【mock】前缀/文档注明),绝不冒充真实执行。
3. 信号只能经 `Derived<Signal>` 派生;观测 append-only;无数据 = Unknown ≠ 绿。
4. 真实 agent 执行被网关 529 挡住时:supervisor 幂等重试 + **编排后端真实工程**
   (真代码/真测试/真 commit)经工作台公开记账 API 落账——产物与度量全真,执行方
   如实标注(TAKEOVER-REPORT §4.1 先例)。
5. 门禁每步过:fmt / clippy -D warnings / test / wasm32×2 / kernel-ui-free。

## 7. 下一步优先级(对齐后的单一队列)

1. **网关恢复即收 G4**:supervisor 跑通 app 自带 claude -p 五阶段环(链路已通,等外部)。
2. **Autopilot 最小化**:cron 触发 → 自动建 Issue 并指派(multica `create_issue` 语义,
   BW 已有真实调度器,只差建 Issue 这步)。
3. G3 起草走真实执行器(工作区已默认存在)。
4. 知识库实体化(参照 connector 真探针模式)。
5. 桌面点击走查(需辅助访问授权;深链+截图已可免点击核验)。
