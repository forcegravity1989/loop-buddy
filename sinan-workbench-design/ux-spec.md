# 司南 · Loop 工作台 — UX 设计规格(ux-spec.md)

> 设计师 Agent 产出 · 中文 · 只设计不实现(TS 仅到 interface / 架构层;HTML 设计稿算设计)
> 权威输入:`00-brief.md` + `sinan-control-plane-v1.html`(作战室)+ `sinan-loop-workbench-v1.html`(Loop 工作台 v1)
> 立场:假定蓝军逐条攻击,本规格对 6 个第一性问题逐一前置堵漏(每处标注 `⟦防蓝军#n⟧`)。

---

## 0. 一句话定位

Loop 工作台 = **操作者驾驶舱**,比作战室(监督视图)下钻一层。它的存在不是"展示成千上万个 Agent 在忙",而是**把稀缺的人类注意力,精确路由到那 ~5% 真正需要人介入的 loop**;其余 95% 默认隐身、只在度量层留下统计痕迹。一个工作台、三类角色(PM/QA、工程师、资产 owner)、一套共享实时状态。

---

## 1. 设计原则(排序即优先级)

### 原则一 · 注意力路由优先于全景展示 `⟦防蓝军#1⟧`
> **UI 的第一性职责 = 把稀缺的人类注意力路由到 ~5% 需人介入的 loop。**

- **默认隐藏健康态。** 工作台落地后,默认视图**只渲染异常/待人工/护栏越线**三类对象;运行良好的 loop 折叠成一个统计数字(`312 运行中`)而非 312 张卡片。
- **"零卡片"是成功态,不是空状态。** 当升级队列清空时,显示"当前无需你介入 · N 个 loop 自主运行中",这是系统健康的正信号,而非需要填满的空白。
- **拟人化是奖励,不是默认。** 虚拟办公室(拟人 5 Agent)只在**单个 loop 下钻**时出现;列表/舰队层永远是高密度表格,绝不在规模上铺拟人卡片。这直接回应蓝军#1:Mavis 式拟人办公室在成千上万规模上会崩。
- **每一屏都要能回答"我现在该看什么"。** 顶部恒有"路由摘要"条:`5 待人工 · 2 异常 · 3 护栏越线`,点击即过滤。

### 原则二 · 信息密度分层(扫描层 → 下钻层 → 介入层)
- **扫描层(fleet / 看板 / 资产表):** 高密度、单行即一个对象、机检色点状态、可排序可过滤。目标是"一屏扫上千行,异常自己跳出来"。
- **下钻层(虚拟办公室 / 蓝图详情):** 拟人化、空间化、叙事化。一次只服务一个对象,允许低密度换取可理解性。
- **介入层(Symphony 升级面):** 决策密度最高——把"为什么要你、风险多大、可选动作、可批量与否"压进一个卡片,让单次决策 < 30 秒。
- **密度随确定性反向变化:** 越确定(健康运行)越压缩;越不确定(异常、分歧、高危)越展开。

### 原则三 · 角色感知,而非角色隔离
- 同一份实时状态(loop 状态机、护栏指标、升级队列)对三类角色**同源**,差异只在"默认落地屏 + 信息聚合粒度 + 可执行动作权限",不在数据。
- 角色切换不是登出登入,而是**视角(lens)切换**——复用并演化 v1 的双视角 lens(见 §3)。

### 原则四 · 健康 = 吞吐 × 质量(护栏先行) `⟦防蓝军#4⟧`
- 任何"越高越好"的吞吐指标(接收率、合入率、净 issue 下降),在 UI 上**必须并排显示其护栏指标**(回滚率、reopen 率、AI-MR 人工改写率、复审抽样命中率)。
- 单看吞吐的视图不存在;吞吐卡片若无护栏伴随,视为设计缺陷。

### 原则五 · 决策留痕,抗橡皮图章 `⟦防蓝军#5⟧`
- 高危批准强制留下"人看过什么 + 为什么放行"的痕迹(看 diff 时长、必填理由)。
- 批量批准只对"同类低危"开放,且批量动作本身也留痕(批了哪一类、命中几条)。

---

## 2. 屏幕清单(多角色)

> 共 6 屏 + 1 个共享底座。每屏标注:**目的 / 关键模块 / 服务角色 / 司南 token 对应**。
> 屏 ①②③④ 为 brief 明确要求;⑤⑥ 为支撑屏(派发详情、办公室全屏)。

### 屏 ① 度量看板(Metrics Cockpit)
- **目的:** 给 PM/QA 一眼看清项目健康度 = 吞吐 × 质量 × 趋势,可向上汇报。
- **关键模块:**
  1. **北极星罗盘**(复用作战室 `compass-card` SVG 罗盘):此处指针指向「健康度综合分」,而非单一吞吐。
  2. **吞吐×护栏对照墙** `⟦防蓝军#4⟧`:三组吞吐指标,每组**左吞吐 / 右护栏**并排。
     - 接收率 87.3% ↔ 误派/挑安全单率(non-issue 反弹率)
     - 合入率 71.2% ↔ 合入后 7 日回滚率 + reopen 率 + AI-MR 人工改写率
     - 净 issue −12/日(healthy=日降)↔ "关成非问题"占比 + 产品停滞探针(新 issue 流入是否异常下降)
  3. **分复杂度闭环率**(复用作战室 `tier` 三档条):越难越靠人,L4-L5 低是合理的。
  4. **趋势区**:每日 spark line(复用 v1 metrics `spark`);可切日/周/月。
  5. **护栏告警带**:任一护栏越阈值,顶部出现朱砂告警条,点击下钻到具体 loop。
- **服务角色:** PM/QA(主)、工程负责人(汇报时)、资产 owner(看自己改的蓝图是否拉低质量)。
- **token 对应:** `compass-card` 深色罗盘卡 / `tier` 复杂度条 / `metrics .m` 指标格 / `spark` 折线 / `--celadon`=健康、`--ochre`=警戒、`--cinnabar`=越线。

### 屏 ② 编排台 fleet + 虚拟办公室(Orchestration Cockpit)
> 这是 v1 已成型的主屏,本规格在其上加固。Master-Detail 双栏。
- **目的:** 给工程师一个"管多 × 看一 × 救异常"的驾驶舱:左舰队扫描成千上万 loop,右办公室下钻单个 loop。
- **关键模块:**
  1. **舰队列(Fleet / Symphony 左栏)**:高密度 loop 行,**默认只浮出 待人工/异常/护栏越线**;健康运行折叠为 `+306 运行中`。顶部 `fleet-sum` 路由摘要 + `dispatch` 派发入口。`⟦防蓝军#1⟧`
  2. **舰队筛选/分组栏(新增)**:按 状态 / 蓝图 / 模型 / value-at-risk / owner 过滤;支持"只看需我"。规模解法见 §4。
  3. **虚拟办公室(Office 右栏 = 下钻详情)**:复用 v1 `conductor` + `desks` + `timeline` + 介入 `panel`。这是**唯一允许拟人化的地方**。
  4. **状态机/工序时间线**:升级为支持并行/分支(见 §5)。
  5. **Symphony 介入面**:升级为可批量 + value-at-risk 排序 + 高危强制 diff(见 §6)。
- **服务角色:** 工程师(主)、PM/QA(切到舰队总览看吞吐分布)。
- **token 对应:** `work` 双栏栅格 / `fleet`+`loop` 高密度行 / `office`+`desk`+`conductor` 拟人下钻 / `timeline`+`step.gate` 状态机 / `panel.card.alert` 介入卡。

### 屏 ③ 资产库(Asset Library)— 三个子页签 `⟦防蓝军#3⟧`
> 蓝军#3:"Agent.md 是唯一可替换部分"是 under-model。真正配置面 = **MD + 模型绑定 + skills + 权限边界 + 验收门(gate)**。因此资产库不是单编辑器,而是"Loop 蓝图编排器"统领下的多资产面。
- **目的:** 给资产 owner 一处管全部可配置面:写 Agent.md、组 Loop 蓝图(含模型/skills/权限/gate)、管 Skill Hub。
- **子页签 a · Agent.md 编辑器:**
  - 左 MD 源、右**实时结构预览**(角色定位 / 工具白名单 / 升级触发条件 / 验收标准)。
  - **配置五元组面板**(不止 MD):MD 文本 · 模型绑定(Opus/Medium/Haiku)· skills 引用 · 权限边界(可自动接受 / 需解锁)· 验收门 gate。`⟦防蓝军#3⟧`
  - **变更影响预估**:改动后展示"将影响 N 个在跑 loop / 历史合入率基线",避免盲改拉低质量(联动屏①护栏)。
- **子页签 b · Skill Hub(~214 skills):**
  - 高密度表格:技能名 / 类别 / 被引用次数 / 最近更新 / 风险标记。
  - 每个 skill 显示"被哪些蓝图/Agent 引用"(依赖反查),改 skill 前能看到爆炸半径。
- **子页签 c · Loop 蓝图编排器(Blueprint Composer):**
  - **DAG 画布**:节点 = Agent 工序,边 = 交班/分支/并行(见 §5),节点上挂 gate(验收门)。
  - 蓝图 = 状态机模板;在此定义"哪些节点是 gate(强制人工)、哪些分支并行"。
  - 右侧属性面:选中节点 → 编辑其 Agent.md 引用 + 模型 + skills + 权限。这把蓝军#3 的"完整配置面"在编排层闭合。
- **服务角色:** 资产 owner(主)、工程师(查蓝图为何这样跑)、PM/QA(查蓝图与质量的关系)。
- **token 对应:** `capcard`+`chip`/`chip.lock`(权限边界,复用作战室能力卡)/ `pipeline`+`stage`(蓝图 DAG 借用 7 环节流水线视觉)/ `step.gate`(验收门红点)/ 编辑器沿用 `--f-mono` 代码字体 + `paper` 卡。

### 屏 ④ Issue 派发(Dispatch Console)
- **目的:** 把两类来源的 Issue(用户上报 / 竞品对标自规划)分诊并派发到"绑定了模型"的 Loop(复杂→Opus,简单→Medium/Haiku)。
- **关键模块:**
  1. **进单流**:两泳道——`用户上报` / `洞察雷达自规划`(复用 v1 `file·radar` loop)。
  2. **分诊三态**(对应接收率口径):`accepted / non-issue / duplicate`,可机检预分类 + 人工复核。
  3. **派发矩阵**:Issue × 复杂度(L1-L5)→ 推荐蓝图 + 推荐模型;一键派发或批量派发同类。
  4. **派发前护栏提示** `⟦防蓝军#4⟧`:若某类 issue 近期合入后回滚率偏高,派发面提示"该类历史质量偏低,建议升级模型/加 gate",防止只挑安全单冲接收率。
  5. **回流**:派发后的 issue 进入屏② 舰队,形成闭环。
- **服务角色:** 工程师(主,派发)、PM/QA(看派发结构是否被 game)。
- **token 对应:** `qcard`(分诊卡,复用作战室队列卡)/ `dispatch`(派发条)/ `mdl`/`mdl.opus`(模型标)/ `btn.primary`(派发动作)。

### 屏 ⑤ 虚拟办公室 · 全屏态(Office Fullscreen)
- **目的:** 工程师"接管"某 loop 后,把屏②右栏的办公室升为全屏,获得最大上下文做深度介入。
- **关键模块:** 大尺寸 DAG 状态机 + 全量办公室日志 + 各 Agent 完整产出(diff、测试报告、对话)+ 接管控制台(暂停/改派模型/退回重做/手动推进 gate)。
- **服务角色:** 工程师(深度介入)。
- **token 对应:** 放大版 `office-body` / `timeline` / `log` / `panel.acts`。

### 屏 ⑥ 共享底座(非独立屏,横切所有屏)
- Loop 蓝图 / Agent·MD / Skill Hub / 模型·MaaS / Hooks·权限 / 验证回路——从 v1 作战室"Agent 基础设施"抽出的共享层。任何屏需要时以抽屉/弹层调起,不强制跳转。

---

## 3. 导航与角色切换

### 3.1 多角色如何在同一工作台共存
- **左 rail(248px 深色,复用 v1)分两组导航:**
  - 组一「执行」:编排台 Symphony · 虚拟办公室 · Issue 派发 · 度量看板。
  - 组二「资产库 · 共享底座」:Loop 蓝图 · Agent·MD · Skill Hub · 模型/MaaS。
- **三角色共用同一 rail**,但顺序/默认高亮随角色变(见 3.3)。导航项右侧的 `ct` 计数 + 状态点(`dot.need/live/idle`)对所有角色实时同源。

### 3.2 角色化视图 + 共享实时状态 + 权限模型 + 通知路由(brief §3 硬需求)
- **角色化视图:** 同屏不同默认聚合。例:屏② 工程师默认落"需我介入"过滤;PM/QA 落"舰队吞吐分布";资产 owner 落"我的蓝图相关 loop"。
- **共享实时状态:** 单一事实源 = loop 状态机事件流。三角色看到的是同一流的不同投影。
- **权限/可见性模型:** 复用作战室"能力卡 + 权限边界"语言(`chip` 可执行 / `chip.lock` 需解锁)。工程师可接管/批准;资产 owner 可改蓝图但不能批单个合入;PM/QA 只读吞吐与护栏 + 可下钻但动作受限。
- **通知/介入路由:** 升级事件按 `value-at-risk` + 角色职责路由——合入/接管类 → 工程师;护栏越线/质量类 → PM/QA;蓝图配置类异常(如某蓝图频繁卡 gate)→ 资产 owner。Symphony 是路由器,不是所有人都收到所有告警。

### 3.3 角色感知的默认落地(role-aware landing)
| 角色 | 默认落地屏 | 默认视角(lens) | rail 高亮 |
|---|---|---|---|
| PM/QA | ① 度量看板 | 编排/总览视角 | 度量看板 |
| 工程师 | ② 编排台 fleet+office | 办公室/介入视角 | 编排台 Symphony |
| 资产 owner | ③ 资产库 · 蓝图编排器 | 资产视角 | Loop 蓝图 |
- 落地屏可改,系统记住偏好;但**首次进入按角色,降低"我该看哪"的认知成本**(呼应原则一)。

### 3.4 复用并演化 v1 的双视角 lens
- **v1 现状:** 作战室 = `人类视角 ◑ / Agent 视角 ◐`;Loop 工作台 = `编排视角 ◑ / 办公室视角 ◐`。
- **演化为"双轴 lens":**
  - **轴 A(粒度):** 编排视角(管多)⟷ 办公室视角(看一)——沿用 v1。
  - **轴 B(主体):** 人类视角(我要做的决策)⟷ Agent 视角(它们的能力与边界)——沿用作战室。
- **lens 是轻量叠加,不是路由跳转**:切 lens 只改同屏的"信息侧重 + 默认过滤",URL/上下文不变,降低切换成本。`⟦防蓝军#6⟧`(我们抄的是 Mavis/Symphony 解决的 job=注意力路由+舰队治理,lens 是司南自有隐喻,不 cargo-cult 其 UI)。

---

## 4. 密度解法(规模化扫描 + 只浮异常) `⟦防蓝军#1⟧`

> 核心矛盾:要管成千上万 loop,又要人只看 ~5%。解法 = **虚拟办公室=单下钻 / fleet=高密度 / 默认只浮异常**。

### 4.1 两种密度形态的分工
- **虚拟办公室 = 单个下钻详情(低密度、拟人):** 一次一个 loop,5 Agent + 状态机 + 日志。**绝不在列表层铺开**——这是蓝军#1 的核心要求,本规格硬约束。
- **Fleet = 高密度表格/网格(扫描):** 一行一个 loop,行高紧凑,机检色点(`loop.need/run/err` 左边框朱砂/青瓷)。支持密度切换:`紧凑表格`(默认,适合上千行) / `卡片网格`(适合几十行精读)。

### 4.2 如何一屏扫成千上万 loop 并只浮异常
1. **默认过滤 = 异常优先(Exception-first by default):** 落地即套用 `状态 ∈ {待人工, 异常, 护栏越线}`;健康运行 loop 不渲染为行,折叠成顶部统计 + 一条 `+N 运行中` 占位。
2. **分层聚合(Roll-up):** 上千 loop 先按 `蓝图 / 模型 / 复杂度` 聚合成组行(组内 N 个、几个异常),展开才出明细。避免一次渲染上万 DOM(设计层即声明虚拟滚动 + 分页/分组,见 §7 组件 props)。
3. **value-at-risk 排序:** fleet 默认按"风险×停留时长"降序,最该看的永远在最上面(联动 §6)。
4. **异常自动上浮:** 任一 loop 跨入异常/越护栏,自动插入顶部"需关注"区并高亮一次(`pulse` 动画),无需人去翻。
5. **饱和保护:** 当待人工数过大(如 >50),fleet 顶部出现"批量分诊"入口,引导 approve-all-similar / 批量改派,防止人被淹(联动 §6)。
6. **健康态留在度量层:** 95% 正常 loop 的价值在屏① 的统计与趋势里体现,不占工程师的注意力带宽。

### 4.3 反例(本规格明令禁止)
- ✗ 在 fleet/看板层渲染拟人办公室卡。
- ✗ 默认展示全部 loop(含健康)让人自己找异常。
- ✗ 用分页让人翻几百页找那 5 个待人工。

---

## 5. 状态机 + 工序时间线交互(支持并行/分支) `⟦防蓝军#2⟧`

> 蓝军#2:"同一时刻只有一个 Agent 跑"是会漏的假设。状态机必须支持并行/分支,别把"单活"画死。

### 5.1 v1 的局限与升级
- **v1 现状:** `timeline .steps` 是一条**线性**链(触发→复现→定位→修复→自测→评审 gate→合入),且文案写死"同一时刻仅一名在执行"。这正是蓝军#2 要打的点。
- **升级为 DAG 时间线(有向无环图)**:节点=工序,边=依赖。支持:
  - **并行分支(fork/join):** 例 `自测` 与 `静态审查` 可并行;两者皆绿才进 `评审 gate`(join)。视觉:两条 `step::before` 连接线从一个 fork 节点分出,在 join 节点合并。
  - **条件分支(branch):** `评审` 结果 = 通过→`合入` / 有分歧→`退回 developer`(回边,形成局部循环但整体推进)。
  - **多 Agent 同时活跃:** `desks` 区允许多个 `desk.active`(多个朱砂呼吸框)同时存在,不再假设单活。

### 5.2 状态机的状态集(设计语义)
- 节点态:`pending(待命) / running(执行中) / blocked(卡住·升级) / done(已交班) / skipped(分支未走) / failed(异常)`。
- gate 态:`gate-open(等待人工) / gate-passed / gate-rejected`,gate 用朱砂高亮(复用 `step.gate`)。
- 边态:`active(数据/控制流正在通过) / taken(已走) / not-taken(分支未选,虚线灰显)`。

### 5.3 交互
- **悬停节点:** 浮出该工序的 Agent / 模型 / 输入产物 / 输出产物 / 耗时。
- **点击 gate:** 直接跳到 §6 介入面(diff + 理由)。
- **并行可视化:** 横向时间线 + 纵向泳道——同一时间列若有多节点 running,即并行;泳道让"谁和谁在并行"一目了然。
- **回边/重做:** 退回 developer 时,画一条回边并把目标节点重置为 running,日志记一次"返工",防止把循环画成死直线。
- **蓝图层一致:** 屏③ Loop 蓝图编排器用同一 DAG 语言定义模板,运行时实例只是模板的"点亮"。设计上**模板态与运行态共用同一组件**(见 §7 `<LoopGraph>`)。

### 5.4 对"5 个固定 Agent"的去硬编
- v1 假设固定 5 Agent(trigger/test-runner/developer/reviewer/orchestrator)。本规格把 Agent 数与角色**由蓝图决定**,组件按数据渲染 desks(`agents: AgentRuntime[]`),不写死 5 格;orchestrator(conductor)始终独立置顶,workers 网格自适应列数。

---

## 6. Symphony 介入 UX(抗瓶颈 + 抗橡皮图章) `⟦防蓝军#5⟧`

> 蓝军#5:几千 Agent 升级到几个人 → 瓶颈 + 疲劳盖章。升级须 **按 value-at-risk 排序 + 可批量(approve-all-similar)+ 高危强制看 diff/填理由**。

### 6.1 升级队列(Escalation Queue)
- **value-at-risk 排序(必须):** 队列默认按"价值风险分"降序。风险分 = f(改动面是否触及受保护边界、影响仓库数/用户数、模型置信度、停留时长、历史该类回滚率)。最危险的永远在最上。
- **风险分可视化:** 每条升级卡带一个风险标尺(`--celadon`→`--ochre`→`--cinnabar` 渐变条),数值 + 等级(低/中/高/危)一眼可读。
- **停留时长可视:** 老化的升级(等待越久)轻微升温(边框由 ochre→cinnabar),防止低危被无限期晾着。

### 6.2 批量批准(approve-all-similar)
- **仅对"同类低危"开放:** 系统把队列里**同蓝图 + 同 gate 类型 + 同风险等级(低)+ 改动模式相似**的升级聚成一簇,提供 `批准本簇 N 条` 一键动作。
- **批量也留痕:** 批量批准记录"批了哪一簇、命中规则、N 条 id",可追溯。
- **高危不可批量:** 风险=高/危的升级**强制逐条处理**,批量按钮对其禁用并说明原因。`⟦防蓝军#5⟧`

### 6.3 高危强制看 diff + 填理由(抗橡皮图章)
- **强制 diff:** 高危升级的"批准合入"按钮**默认禁用**,直到人**实际展开过 diff**(记录展开 + 浏览时长);未看 diff 不能批。
- **必填理由:** 高危批准弹出必填"放行理由"(自由文本或选预设原因),写入审计日志。空理由不能提交。
- **反向摩擦设计:** 越危险,UI 摩擦越大(多一步确认/多一个理由)。这是刻意的——低危一键过、高危必须慢,把"疲劳盖章"在交互上变难。

### 6.4 抗瓶颈的结构设计
- **路由分流(见 §3.2):** 不是所有升级都涌向同一个人;按 value-at-risk + 角色职责分给工程师/PM/资产 owner,削峰。
- **批量 + 分诊削峰:** approve-all-similar 把"几千升级→几个人"里的低危长尾一键消化,人力集中在高危头部。
- **升级转配置:** 若某类升级反复出现(如某蓝图频繁卡同一 gate),Symphony 提示"该问题已出现 N 次,是否调整蓝图 gate/权限"——把重复人工转成一次性配置改动(联动屏③),从源头降介入量。
- **介入卡结构(单卡 < 30 秒决策):** 复用 v1 `panel.card.alert` —— `为什么要你(why)` + `谁升级的/多久了(from)` + `风险标尺` + `动作区(批准/看diff/接管/退回)`,高危卡额外挂"理由必填"。

---

## 7. TS 组件架构(设计,不写实现)

> 只到组件树 + 关键 props/类型(TS interface 形式)+ 状态归属 + 原子边界。**不写实现代码。**

### 7.1 "组件 vs 应用"边界
- **组件(可复用原子,无业务编排、无数据获取):** 纯展示 + 受控交互,数据全由 props 进、事件全由回调出。例:`<LoopOffice>`、`<LoopGraph>`、`<FleetTable>`、`<EscalationCard>`、`<MetricVsGuardrail>`、`<BlueprintCanvas>`、`<RiskMeter>`、`<LensToggle>`。
- **应用(屏/容器,负责编排):** 持有路由、角色、实时订阅、权限判定,把状态投影成各组件的 props。例:`<MetricsCockpitScreen>`、`<OrchestrationScreen>`、`<AssetLibraryScreen>`、`<DispatchScreen>`。
- **边界原则:** 组件不知道"当前是谁(角色)"也不知道"数据从哪来";角色/权限/订阅只活在应用层。这样同一个 `<LoopOffice>` 能被工程师介入屏和 PM 只读屏复用,差异由应用层传入的 `permissions` 决定。

### 7.2 组件树(简)
```
<WorkbenchApp>                         // 应用根:路由 + 角色 + 实时状态源
├─ <RoleAwareRail role permissions/>   // 左 248px rail,按角色排序/高亮
├─ <DualAxisLens granularity subject/> // 双轴 lens(粒度×主体)
└─ <Screen>                            // 由路由决定
   ├─ MetricsCockpitScreen
   │   ├─ <CompassGauge metric/>                 // 复用作战室罗盘
   │   ├─ <MetricVsGuardrail pair[]/>            // 吞吐↔护栏对照(防#4)
   │   ├─ <ComplexityTiers tiers/>               // 分复杂度闭环率
   │   └─ <TrendStrip series range/>
   ├─ OrchestrationScreen
   │   ├─ <FleetTable groups filters density/>   // 高密度舰队(防#1)
   │   │   └─ <LoopRow loop/>
   │   ├─ <LoopOffice loop agents graph log/>    // ★ 可复用原子:虚拟办公室
   │   │   ├─ <Conductor orchestrator/>
   │   │   ├─ <DeskGrid agents/>                 // 按数据渲染,不写死5(防#2)
   │   │   ├─ <LoopGraph nodes edges mode/>      // DAG 状态机(防#2)
   │   │   └─ <OfficeLog entries/>
   │   └─ <EscalationPanel queue/>               // Symphony 介入(防#5)
   │       ├─ <EscalationCluster cluster/>       // approve-all-similar
   │       └─ <EscalationCard item/>
   │           ├─ <RiskMeter score level/>
   │           └─ <DiffGate required onReason/>  // 高危强制diff+理由
   ├─ AssetLibraryScreen
   │   ├─ <AgentMdEditor doc binding skills perms gate/> // 五元组(防#3)
   │   ├─ <SkillHubTable skills usageGraph/>
   │   └─ <BlueprintCanvas graph/>               // 与 <LoopGraph> 同语言
   └─ DispatchScreen
       ├─ <IntakeLanes lanes/>
       ├─ <TriageBoard issues/>
       └─ <DispatchMatrix issues blueprints models/>
```

### 7.3 关键类型签名(TS interface)
```ts
// —— 领域核心 ——
type LoopState = 'pending' | 'running' | 'blocked' | 'needHuman' | 'error' | 'done';
type ModelTier = 'opus' | 'medium' | 'haiku';
type Role = 'pm' | 'engineer' | 'assetOwner';

interface LoopInstance {
  id: string;
  kind: 'file' | 'fix';            // 已上线两类;预留扩展
  title: string;
  blueprintId: string;             // 指向蓝图模板
  state: LoopState;
  model: ModelTier;
  complexity: 1 | 2 | 3 | 4 | 5;
  valueAtRisk: number;             // 0–100,驱动排序与摩擦
  ownerAgentId?: string;
  waitingMs?: number;              // 停留时长 → 老化升温
  source: 'user-report' | 'radar-selfplan';
}

// —— 状态机:DAG,支持并行/分支(防#2)——
interface LoopGraphNode {
  id: string;
  procedure: string;               // 工序名:触发/复现/.../合入
  agentId?: string;
  status: 'pending' | 'running' | 'blocked' | 'done' | 'skipped' | 'failed';
  isGate?: boolean;                // 验收门 → 可能需人工
  gateStatus?: 'open' | 'passed' | 'rejected';
}
interface LoopGraphEdge {
  from: string; to: string;
  kind: 'sequence' | 'fork' | 'join' | 'branch' | 'loopback';
  state: 'active' | 'taken' | 'not-taken';
}
interface LoopGraphProps {
  nodes: LoopGraphNode[];
  edges: LoopGraphEdge[];
  mode: 'template' | 'runtime';    // 模板态(蓝图)与运行态共用同一组件
  onGateClick?: (nodeId: string) => void;
}

// —— 虚拟办公室:可复用原子 ——
interface AgentRuntime {
  id: string; name: string; roleLabel: string;  // e.g. developer/开发
  model: ModelTier;
  status: 'idle' | 'active' | 'waiting' | 'done' | 'blocked';
  currentAction?: string;
  mdRef: string;                   // 指向 Agent.md
}
interface LoopOfficeProps {
  loop: LoopInstance;
  orchestrator: AgentRuntime;      // conductor 独立置顶
  agents: AgentRuntime[];          // 数量由蓝图决定,不写死 5(防#2)
  graph: LoopGraphProps;
  log: OfficeLogEntry[];
  permissions: ActionPermissions;  // 由应用层注入 → 决定可执行动作
  onIntervene?: (action: InterventionAction) => void;
}

// —— Symphony 介入(防#5)——
type RiskLevel = 'low' | 'medium' | 'high' | 'critical';
interface EscalationItem {
  id: string; loopId: string;
  reason: string;                  // 为什么要你
  from: { agentId: string; ageMs: number };
  riskScore: number; riskLevel: RiskLevel;
  diffRef?: string;
  similarKey: string;              // 同簇键 → approve-all-similar 分组
}
interface EscalationCardProps {
  item: EscalationItem;
  requireDiffBeforeApprove: boolean;   // 高危=true:未看diff禁用批准
  requireReason: boolean;              // 高危=true:必填理由
  onApprove: (reason?: string) => void;
  onTakeOver: () => void;
  onReject: () => void;
}
interface EscalationClusterProps {     // approve-all-similar
  cluster: { key: string; level: RiskLevel; items: EscalationItem[] };
  batchable: boolean;                  // 仅 level==='low' 为 true
  onApproveAll: (key: string) => void; // 批量动作留痕
}

// —— 度量护栏对照(防#4)——
interface MetricVsGuardrailProps {
  pair: {
    throughput: { label: string; value: number; trend: number };  // 吞吐
    guardrails: { label: string; value: number; breached: boolean }[]; // 护栏
  }[];
}

// —— 资产配置五元组(防#3)——
interface AgentConfig {
  md: string;                          // Agent.md 文本
  model: ModelTier;                    // 模型绑定
  skills: string[];                    // skills 引用
  permissions: 'auto' | 'needsUnlock'; // 权限边界
  gate?: { criteria: string; machineCheckable: boolean }; // 验收门
}

// —— 角色/权限/lens ——
interface ActionPermissions {
  canApproveMerge: boolean;
  canTakeOver: boolean;
  canEditBlueprint: boolean;
  readOnly: boolean;
}
interface LensState { granularity: 'orchestration' | 'office'; subject: 'human' | 'agent'; }
```

### 7.4 状态归属(谁拥有什么状态)
- **应用层(`<WorkbenchApp>` / 各 Screen):** 路由、当前 `Role`、实时 loop 事件订阅、`ActionPermissions` 计算、升级队列的获取与排序、lens 状态。**所有"数据从哪来 / 你是谁 / 你能做什么"在这里。**
- **组件层:** 仅持有纯 UI 局部态(展开/折叠、hover、密度切换、diff 是否已展开这种交互态)。`<DiffGate>` 持有"是否已看 diff"的本地态并据此 enable 批准按钮,但"是否要求看 diff"由 props 注入。
- **单一事实源:** loop 状态机事件流;三角色三屏都是它的投影,杜绝多份副本不一致。

### 7.5 可复用原子标注
- **`<LoopOffice>`** = 核心可复用原子:工程师介入屏、PM 只读下钻、办公室全屏(屏⑤)三处复用,差异仅靠 `permissions`/`readOnly`。
- **`<LoopGraph>`** = 模板态/运行态双复用(蓝图编排器 ↔ 运行时状态机)。
- **`<RiskMeter>` / `<MetricVsGuardrail>` / `<EscalationCard>`** = 跨屏复用的治理原子。
- **`<CompassGauge>` / `<LensToggle>`** = 直接继承自司南既有设计系统(作战室),不重造。

---

## 8. 司南 token 复用说明

> 全程复用作战室/v1 工作台的同一套 tokens,不新增颜色族,只在语义上扩展。

### 8.1 颜色(语义不变)
| token | 值 | 本工作台语义 |
|---|---|---|
| `--ink #1C1813` / `--ink-2` | 深色 | 左 rail、conductor、罗盘卡、深色面 |
| `--rice #EEE6D2` | 米底 | 全局背景 |
| `--paper #FAF5E9` / `--paper-2 #F4ECDA` | 纸 | 卡片、格子底 |
| `--cinnabar #C23A22` / `-bright` | 朱砂 | **主操作 / 待人工 / 异常 / gate / 高危** —— 注意力锚点 |
| `--celadon #5E7D6C` | 青瓷 | 运行中 / 健康 / 护栏达标 |
| `--ochre #C18A2C` | 赭石 | 等待 / 试运行 / 警戒 / 升级老化 |
| `--muted #7A6C57` / `-2` | 灰 | 次要文字、EN 标签 |
| `--hairline #DAD0B7` / `-2` | 发丝线 | 描边、分隔 |

- **关键一致性:** 朱砂 = 稀缺注意力的唯一锚色。全工作台**只有"需要人/异常/高危"用朱砂**,健康态绝不用朱砂——这是原则一(注意力路由)在视觉层的落地。

### 8.2 字体
- display = `Fraunces + Noto Serif SC`(标题、指标大数、北极星值)。
- 正文 = `Noto Sans SC`。
- 标签/指标/英文/代码/loop id = `IBM Plex Mono`(`--f-mono`)——保持作战室"机检数据用 mono"的质感。

### 8.3 母题与构件复用
- paper-grain 噪声叠加(`body::before`,opacity .05,multiply)——全屏沿用。
- 左 248px 深色 rail + 圆角 10px(`--r`)+ hairline 描边 + sticky topbar(`backdrop-filter:blur`)。
- 双视角 lens(`.lens` 胶囊切换)——演化为双轴(§3.4),视觉构件不变。
- 直接复用的构件:`compass-card` 罗盘 / `tier` 复杂度条 / `metrics .m` 指标格 / `loop` 舰队行 / `desk`+`conductor` 办公室 / `timeline`+`step.gate` 状态机 / `panel.card.alert` 介入卡 / `qcard` 队列卡 / `chip`+`chip.lock` 能力/权限 / `mdl.opus` 模型标 / `dot.need/live/idle` 状态点。
- `prefers-reduced-motion` 与 `:focus-visible` 无障碍约定沿用。

### 8.4 不 cargo-cult `⟦防蓝军#6⟧`
- 我们复用司南**自有**的视觉语言(罗盘、lens、纸纹、朱砂锚点),解决 Mavis/Symphony 真正解决的 **job(注意力路由 + 舰队治理)**;拟人办公室只作为"下钻详情"这一受控隐喻保留,不把它当默认列表形态去抄 Mavis 的 UI。

---

## 附:对蓝军 6 问的前置处置一览
| # | 蓝军第一性问题 | 本规格处置位置 |
|---|---|---|
| 1 | 拟人办公室规模会崩 / UI 应路由注意力 | 原则一 · §2屏② · §4(office=单下钻,fleet=高密度,默认只浮异常) |
| 2 | "单活"是会漏的假设 | §5(DAG 状态机:fork/join/branch/loopback,多 desk.active,去硬编 5 Agent) |
| 3 | Agent.md 非唯一配置面 | §2屏③ · §7 `AgentConfig` 五元组(MD+模型+skills+权限+gate)+ 蓝图编排器 |
| 4 | 健康度可被反向 game | 原则四 · §2屏① 吞吐↔护栏对照墙 · 屏④ 派发前护栏提示 |
| 5 | Symphony 瓶颈 + 橡皮图章 | §6(value-at-risk 排序 + approve-all-similar 仅低危 + 高危强制 diff/理由 + 路由削峰 + 升级转配置) |
| 6 | cargo-cult Mavis/Symphony | §3.4 lens 为司南自有隐喻 · §8.4(抄 job 不抄 UI) |
