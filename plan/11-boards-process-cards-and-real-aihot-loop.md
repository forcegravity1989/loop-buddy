# plan/11 · 看板/过程件分栏 · 项目组件完整详情 · 工作流可视化 · 业界卡片 · aihot 真实双环

> 接棒入口。承 [plan/10](10-personal-kanban-and-real-run.md)(K0-K4 已交付)。
> 本轮五条工作线 L1–L5,对应用户 2026-07-21 的五点指令 + 一次拷问。
> **fable 出计划(本文档),sonnet 5 执行。** 设计决策已在本文拍板,执行按「工程对照」落地,DoD 一律 sqlite 读回 / 深链 stderr 核验(computer-use 对本 debug 二进制拿不到窗口,见 [[bw-desktop-render-computeruse]])。

---

## 现状锚点(读过源码,不是猜)

- **一级 `IconRail`**(`screens/chrome.rs`):全局 marketplace,不按项目过滤 —— 保留。
- **二级 `ProjectRail`**(`screens/project_rail.rs`,plan/10 K1 建):按 `project_id==Some(active)` 过滤 skills/agents/workflows/cron/connectors,**但点击 `on_pick.call(Hub::X)` 跳回全局 marketplace hub**(`main.rs:234`)—— 这正是本轮 L1 要改掉的。
- **`Op` 六面板**(`screens/op.rs:153` `PANELS`):进度/工作流/定时任务/产物/版本/Issue看板,**平铺一条 toolbar**;`HealthOverview`(`op.rs:208`)只作 `scope==All` 时的左栏挂件。
- **卡片数据其实齐,是呈现稀**:`SkillCardVm` 有 content/used_by/distilled_from_issue;`AgentCardVm` 有 model/skills/win_rate/runs/instructions;`WorkflowDetailVm` 有 phases/phase_prompts/agents/skills/loop/runs/版本。缺的是「把这些铺成让人愿意用的卡」,不是缺字段。
- **aihot 现装**:cron「每日日报生成」= `CreateIssue`@Build(每天建一件开发活);主 workflow = 头脑风暴→写计划→TDD→评审(`practice_aihot.rs:278`)。**运行态与开发态被混成一件事** —— L5 的靶。

---

## L1 · 二级栏 = 项目组件的完整详情(不跳市场)

**目标(人话)**:打开一个项目,左二栏只列这个项目自己的组件(有两个 skill 就只列这两个)。点其中一个,右边**完整铺开这个组件的结构与内容**——skill 一种卡、agent 一种卡、workflow 一种卡、cron 一种卡,各是各的形状。永不跳去 marketplace(市场走一级 `IconRail`)。

**工程对照**:
- `main.rs::Root` 新增 `sel: Signal<Option<ComponentSel>>`;`enum ComponentSel { Skill(SkillId), Agent(AgentId), Workflow(WorkflowId), Cron(CronTaskId), Connector(ConnectorId) }`。
- `ProjectRail` 的 `on_pick` 由 `EventHandler<Hub>` 改成 `EventHandler<ComponentSel>`(点条目=选组件,不再 `hub.set`)。空组仍如实写「本项目还没有自建的 X」。
- 新建 `screens/component_detail.rs`:`SkillDetail` / `AgentDetail` / `WorkflowDetail` / `CronDetail` 四个**独立形状**的详情卡。内容直接复用已加载的 `hub.*` VM(按 id 反查),不新拉数据。把 `skill_hub.rs`/`workflow_hub.rs` 里已有的「展开态详情正文」抽成这四个可独立渲染的组件,marketplace 与项目栏共用同一套详情(避免两份真相)。
- `main.rs` 渲染分支:`view==App && sel.is_some()` → 渲染对应 `*Detail`(占据内容区,盖在 `Op` 之上或替换 Center);`sel` 为 `None` → 现有 `Op`。一级 IconRail 点任意 marketplace 图标时清 `sel`。
- 四种卡各自的「完整属性」(L4 给信息架构,这里定形状差异):
  - **Skill**:正文(运行时注入点标注)· 分类/来源 · 被哪些 workflow 引用(反查)· 蒸馏来源 Issue+产出 agent · 引用次数。
  - **Agent**:角色 · model · 装备技能(chip)· 真实胜率+战绩(runs/win_rate,`""`=无证据不写0%)· instructions 系统提示词。
  - **Workflow**:**流程图**(L3)· loop_config · 解决什么问题(goal/prompt)· 涉及 agents/skills · 运行史(runs/成功率/耗时)· 版本演进。
  - **Cron**:schedule · mode(RunWorkflow/CreateIssue)· target · 下次运行(`cron_next_run_label`)· 有效性(`CronEffectiveness`:fires/ok/成功率)· 最近几次触发结果。

**DoD**:`BW_HUB` 之外,ProjectRail 点击后内容区渲染对应详情卡、无 panic;四类卡字段各不相同(截图/深链 stderr 记录 `sel` 变化);详情数字与 `sqlite3` 读回一致(skill uses / agent runs / workflow run 计数 / cron fires)。

---

## L2 · 看板 / 过程件 分栏(采纳用户提议,含一处细化)

**我的判断(答"你觉得呢")**:**赞成分栏,它正好落在产品自己的心智上** —— 看板=对外可验证的整体进展 + 难造假的健康(引子页四控制点里"每周在正常演进 / 目标清晰难造假");过程件=达成这些的内部机制。分开后看板变干净,过程件是想看细节才进。**一处细化**:不要把两组做成会丢阶段上下文的两个顶级目的地;保留 `StageAxis`,把 toolbar 改成**两段式分组**。健康概览**合并进看板**,做成「进度」面板顶部的一张健康概览卡,而不是第 7 个 tab。

**工程对照**(`screens/op.rs`):
- `PANELS` 重组为两组带标签的分段:
  - **看板**:进度 `Progress` · Issue看板 `Issues` · 版本 `Version`
  - **过程件**:工作流 `Workflow` · 定时任务 `Routine` · 产物 `Artifact`
- `Toolbar` 渲染两段(中间一道分隔 + 组名 `看板`/`过程件`)。`Panel` 枚举不变(bw-app),纯 UI 分组。
- **健康概览合并**:把 `HealthOverview`(`op.rs:208`,现为 `scope==All` 左栏)的内容——「进行中·待你介入」+「阶段信号·需关注」+「N 平稳/M 归档」——提成 `ProgressAll` 面板顶部的一张卡(接在「本周复盘」上方或并排)。左栏在 `scope==All` 下可空出或改放阶段总览;`scope==Stage` 仍是 `StageSessions`。绿色隐身规则不变(全绿只留一行「一切安静」)。

**DoD**:深链 `BW_PANEL=progress` 渲染出「健康概览」卡且数字与 `sqlite3` 读回一致;toolbar 两组标签可见;六面板全部可达无 panic。

---

## L3 · 工作流全流程可视化(核心)

**目标(人话)**:工作流是我们的核心,得把它的**全流程画出来**——有哪些阶段、循环/loop 在哪、哪一步是生成器、哪一步是评估期/优化器、这个 workflow 到底解决什么问题。现在只是 `1. x 2. y` 的 chip,画不出环。

**工程对照**:
- 新建 `screens/workflow_flow.rs`:`WorkflowFlow` 组件,入参 `phases` + `phase_prompts` + `loop_config` + `goal`。渲染有向管线(节点=phase,箭头=顺序),**loop-back 边**从末节点回首节点,边上标 `max_iter`/`retries`(来自 `loop_config`,真实值,非装饰)。
- **生成器/评估器/优化器角色标注**:用 phase 名关键词做**诚实分类**——含「实现/原型/起草/生成」→ 生成器;含「评审/验证/测试/回归/verification」→ 评估器;含「优化/删减/重构/refine」→ 优化器;认不出就中性节点,**不硬贴一个假角色**(宁可不标,不造假)。对 BW 五阶段 workflow,`StageKind::method_loop()` 是权威环(末步 `↺` 已在语义里),直接据此画。
- 顶部横幅显式「解决什么问题」= `goal`(+ `prompt` 副文)。
- 复用点:`WorkflowDetail`(L1)与 marketplace `WorkflowHub` 展开态都渲染 `WorkflowFlow`,替换现在的 chip 串(`workflow_hub.rs:268`、`op.rs` WorkflowPanel)。
- 渲染手段:Dioxus 内联 SVG 或 flex 节点+CSS 箭头(不引外部图库,守 wasm/无 UI 依赖内核那条线——本组件只在 app-desktop)。

**DoD**:深链进 workflow 详情,`WorkflowFlow` 渲染出节点+顺序箭头+loop-back 边、loop 计数与 `sqlite3` 读回的 `loop_config` 一致;aihot 主 workflow 画出「实现(生成器)→评审(评估器)↺」;确定性管线(L5 的运行态 workflow)如实画成**直线无环**(它没有 loop,别造)。

---

## L4 · 业界风格组件卡信息架构(小红书/workbuddy 等推广站的留存逻辑)

**目标(人话)**:一个 skill/agent 卡上要让人看到什么,才愿意留下来用它。参考推广站的卡:它们把"为什么值得用"堆在最显眼处。

**信息架构(按优先级,组件卡从上到下)**:
1. **一句话价值主张**——解决什么问题(skill/agent 的 `desc`、workflow 的 `goal`)。现在埋得太深,提到标题下第一行。
2. **社会证明**——引用/复用次数、真实胜率/成功率、"被 N 个 workflow 使用"、成熟度。数据都在(`uses`/`win_rate`/`success_rate`/`used_by`/`maturity`),**现在几乎没露**。`""`/`None` 一律"无证据",绝不显示 0%。
3. **出处可信度**——来源(官方/自建/选型引入 + 哪个 marketplace,`HubSource::Adopted`/`LibSource`)、蒸馏来源 Issue(`distilled_from_issue`,"来自哪件真活")。
4. **怎么用**——触发词/slash(workflow `trigger`)、注入点(skill content 注入 prompt)。
5. **新鲜度**——最近运行/更新(`last_run`/`registered_at`)。
6. **结构预览**——workflow 给流程图(L3)、agent 给装备技能+model、skill 给正文首几行。

**工程对照**:按此 IA 重排 `SkillCard`(`skill_hub.rs`)、`AgentCard`(`agent_hub.rs`)、`WorkflowHub` 行头 与 L1 的四张详情卡(共用组件,marketplace 与项目栏一致)。**只重排既有真实字段,不新造社会证明数字。** 若某字段无数据,如实留白或"暂无运行/无证据"。

**DoD**:卡上出现价值主张行 + 社会证明区;所有数字 `sqlite3` 读回一致;无数据处显示诚实留白而非 0/绿。

---

## L5 · aihot 到底该有的定时任务与工作流(答拷问)

**拷问的答案:aihot 现在把「运行态」和「开发态」混成了一件事,这是根子上的错。** 一个已建成的日报产品,有两个**互不相同**的环:

### 环一 · 运行态(产品在自运转)—— 每天
`python -m aihot.main` → 抓取 → 打分 → 去重 → 渲染 → 写 `telemetry.json`。**确定性管线,不是 agent 创作**。产出 = 当天 digest + telemetry;telemetry 自动喂 K4 的「每日命中率」「连续产出日报天数」。这是**运维段**的"产品自运转"。

### 环二 · 开发态(改进 aihot 本身)—— 事件驱动,不是每天
只有当**命中率跌破阈值 或 连续产出断更**时,才起一件开发 Issue(加源/调关注面打分/修 bug),走五阶段方法论 + aihot 主 workflow(头脑风暴→计划→TDD→评审)。这才是构建/优化段。

### 现装错在哪
cron「每日日报生成」被设成 `CreateIssue`@Build —— **每天建一件开发活**。对已建成的产品,每天不该"建开发任务",该"运行产品并采集 telemetry"。开发活应当**事件/阈值触发**,不是无脑每日。

### BW 的真实缺口(诚实,不假装已支持)
cron 只有两种 mode:`RunWorkflow`(跑 agent 工作流)和 `CreateIssue`(建开发活)。aihot 每日运行**两个都不是**——它是确定性脚本 + telemetry 采集。**BW 缺一个"运行确定性命令 + 采集 telemetry"的调度能力。** 本轮不硬造复杂机制,二选一:
- **(推荐,最小切片)** 复用已有的 `feed-telemetry` 子命令 + git-repo connector:运行态由外部/脚本每日跑 `main.py`,BW 侧 cron 做一件"采集 telemetry → 喂命中率/连续产出天数"的活(mode 用新增的轻量 `CollectTelemetry`,或先用现有 connector 同步路径),**如实标注"运行本体在工作区脚本,BW 负责采集与判健康"**,不假装 BW 在编排 Python。
- (更重,不做) 给 executor 加"跑任意 shell 命令"的能力——越出本轮范围,留口不假装。

### 落地的真实四件套
- **Cron A(运行态·每日)**:采集当日 telemetry → 更新「每日命中率」「连续产出日报天数」。DoD:`telemetry.json` 当日更新、连续产出 +1 可 `sqlite3` 读回。
- **Cron B(治理态·每周或阈值)**:若 7 日命中率均值 `<8%` 或断更,`CreateIssue`@Optimize 指派「日报编辑」——**阈值触发,不是每天建**。这才是 `CreateIssue` autopilot 的正确用法(no-hijack:只建活不自动跑)。
- **Workflow 1(运行态·线性管线)**:「每日摘要生成」= fetch→score→dedupe→render→telemetry。L3 如实画成**直线无环**(没有 generator/evaluator 循环,别硬画)。
- **Workflow 2(开发态·带 loop)**:「aihot 主 workflow」= 头脑风暴→计划→实现(生成器)→评审/验证(评估器)↺。这个才有 L3 的 loop。

### 指标三层(收束用户"issue 解决数不完全是引领/滞后指标")
- **引领(leading,预示未来健康)**:每日命中率(命中/抓取原始)、关注面覆盖度。命中率下滑 → 预示日报要没内容 → 触发环二。
- **滞后(lagging,已发生的结果)**:连续产出天数、累计日报数。
- **工作量/过程(既非引领也非滞后)**:issue 解决数、commit 数。它衡量"我改了多少",不衡量"产品对用户好不好"。K4 已把 issue 计数降为工作量参考,方向对;**看板判健康只用前两层**,工作量层只作过程参考。

**DoD**:`practice_aihot.rs` 的 `cmd_cron` 拆成 A/B 两件(B 带阈值判定),运行态 workflow 与开发态 workflow 分别登记且 `source`/`kind` 如实;三层指标在看板上分区呈现,`sqlite3` 读回口径与定义一致。

---

## 执行顺序建议

L2(分栏,纯 UI 重组,风险最低,先立骨架)→ L4(卡片 IA,数据现成)→ L1(项目组件详情,新 nav 概念,复用 L4 卡)→ L3(流程图,喂进 L1/marketplace 详情)→ L5(aihot 双环,含 BW cron 缺口的最小切片 + 指标三层)。每件独立 commit,代号前缀 `L1-` … `L5-`,门禁全过(fmt/clippy/wasm/guard/app-desktop check),行为靠深链 stderr + sqlite 读回核验。
