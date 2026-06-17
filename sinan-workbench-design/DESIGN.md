# 司南 · Loop 工作台 — 最终设计(DESIGN.md · v1,双轮对抗收敛)

> **产出方式**:PM / 设计 / 蓝军三个 sub-agent 跑完 **R1** 独立产出(`pm-spec.md` / `ux-spec.md` / `blueteam-r1.md`),蓝军给出 5×P0 + 1×方法论 P0 裁决。**R2** 收敛由**编排者**综合完成(两个 worker agent 因 infra 看门狗连续 stall,编排者持全量上下文代行;见 §13 诚实声明)。
> **铁律**:只设计、不实现。TS 仅到 interface / 架构。HTML 设计稿算设计。
> **权威输入**:`00-brief.md` + `sinan-control-plane-v1.html`(作战室)+ `sinan-loop-workbench-v1.html`(工作台 v1)。
> 本文是唯一权威最终设计;R1 三份 spec 为其详细附录。
>
> **🔴 独立终审判决(`blueteam-r2.md`):「半活」** —— 刃A·路由侧已活,可进高保真 / 实现准备;**刃B·复利机(§4)纸面闭环但数学未闭环,补齐 §4.6 三条硬前置前不得进实现、不得宣称「数学上成立」。** 终审实证了"编排者自审"的盲区:R2 的明星机制(复利机)恰恰最少被自我质疑,其地基洞躲过了 R1 点名(见 §4.6 / §13 R-6~R-8)。

---

## 0. 第一性定位 —— 经蓝军 P0-1 重写

把罗盘、拟人办公室、桌位动画都剥掉后,这个工作台**唯一不可替代的 job**:

> **在成千上万个自主 Loop 里,把稀缺的人类判断力精确路由到「真正需要、且此刻需要」的那 ~2–5% 决策点(刃A · 路由);并让每一次人类判断都沉淀成可复用资产(规则/门/技能/蓝图),使下一次同类决策不再需要人(刃B · 复利)。**

一句话:**注意力路由器 + 判断力复利机**,不是「agent 监控大屏」。这把尺子有两个刃,贯穿全文:

- **刃 A(路由)**:任何不服务"把对的决策推给对的人/此刻"的 UI 元素,在 10k 规模下都是会淹死产品的噪声。
- **刃 B(复利)**:任何一次人工介入,若做完就蒸发、没变成"下次不用人"的资产,产品就是**线性人肉运维**,随规模线性增加人力 → 商业崩盘。**R1 全员把刃A做透、刃B几乎缺席 —— R2 的核心就是补上刃B(见 §4)。**

**北极星(取代裸"零干预率")**:**受质量约束的自主率** ⊗ **决策杠杆**。
- 受质量约束的自主率 = 自主闭环率,但**仅在 false-autonomy rate(该升级却没升级的漏报率)与回滚率受控时才计数**(蓝军 B2)。
- 决策杠杆 = 每次人工决策平均消除的未来同类介入数(刃B 的产出,见 §4)。

像素预算铁律:**默认首屏 = 排好序的决策队列 + 一键固化为规则的入口**;拟人办公室降为 L3 下钻,不占扫描层。

---

## 1. 多角色模型(brief §3 硬需求)

三角色共用**一个**工作台、**同一份**实时状态;差异只在"默认落地 + 聚合粒度 + 可执行动作权限",不在数据可见性(涉密除外)。

| 角色 | JTBD 一句话 | 默认落地 | 默认 lens | 写权限边界 |
|---|---|---|---|---|
| 质量 / PM(Lin) | "系统今天更健康还是更脆?是不是被刷出来的?" | 度量看板 | 监督镜 | 只读运行;配阈值/抽样打标/转资产工单。**不批合入、不改资产**(裁判≠运动员) |
| 工程师(Wei) | "该救哪个 loop?为什么卡?怎么接管?" | 编排台(决策队列) | 执行镜 | approve/reject/接管/退回/派发;受保护边界需对应权限位 |
| 资产 owner(Mei) | "哪条蓝图在漏?改 MD/模型/Gate 影响多少在跑实例?" | 资产库·蓝图 | 资产镜 | 改蓝图五元组 + 灰度/回滚;**改基线=改宪法,走评审** |

**共存四机制**(详见 `pm-spec.md §2`):
1. **一套 IA + 角色化默认落地页 + 个人可覆盖**;rail 结构对所有人一致,徽标计数同源。
2. **lens ≠ role**:lens 是"看同一份数据的镜头"(监督/执行/资产),任何人可切任意 lens;**写操作才按 role 点亮**。lens 是注意力工具,不是权限围栏(蓝军 #6)。
3. **深链 + 同一 ID 串场**:三人围同一对象协作时看的是同一页。
4. **看无锁全员一致 / 写抢占式软锁 + 服务器对账**(并发治理见 §7)。

---

## 2. 信息架构 —— 经蓝军 P0-2 重写(杀死"全量列表"默认范式)

**三层密度,密度随确定性反向变化:**

1. **决策层(默认首屏)**:只渲染"need-human-now"的 N 条(2–5% 且只取此刻卡住的子集,自然是小数),按 value-at-risk 降序。**不是 fleet 全量。**
2. **Fleet 聚合层(可下钻)**:成千上万 LoopRun **永不线性铺开**,默认按 `蓝图/状态/模型/风险/owner` **分桶 rollup**(`code-fix-v3 · 4,210 实例 · 38 待人工 · 2 异常`),点桶再下钻。虚拟滚动兜底。
3. **Office 详情层(L3 下钻才进)**:拟人化办公室只在这层、只为"看懂单个"服务。

**Surfaces(沿用 v1 两段式 rail):**
```
执行 EXECUTION
  ◎ 编排台 Symphony   决策队列(VaR 排序)+ Fleet rollup + 队列流量头(§5.4)
  ⬡ 虚拟办公室 Office  单 LoopRun 下钻:Agent 工位 + DAG 状态机 + 介入面
  ⤴ 复利 · 规则账本   ★R2 新增:人工决策→规则/门/技能,覆盖率/精度/回滚(§4)
  ⤵ Issue 派发         用户上报 / 竞品雷达 → 分诊 → 选蓝图/模型 → 派发
  ▤ 度量看板           吞吐↔护栏对照墙 + 复合健康灯 + 复利曲线(§5)
资产库 · 共享底座 ASSETS
  Loop 蓝图(DAG+Gate+绑定;版本/灰度/A-B) · Agent·MD · Skill Hub · 模型/MaaS
```

---

## 3. 领域模型(实体 / 状态机)

完整定义见 `pm-spec.md §4`;此处给最终态要点 + R2 新增实体。

- **LoopBlueprint**(可复用模板):`dag`(支持 parallel/branch)、`gates[]`、`defaultBindings[]`、`version`、`status: draft→canary→active→deprecated→archived`。配置面=**五元组**(MD+model+skills+perms+gate),不止 MD(蓝军 D1/F3)。
- **AgentSpec / AgentInstance**:`AgentState: idle|active|handed_off|waiting|blocked|failed`。**同一 LoopRun 可多个 `active`**(并行),界面"单活"仅是当前态渲染,非模型约束(蓝军 F2)。
- **LoopRun**(=一间办公室):`state: queued|running|at_gate|needs_human|human_holding|error|merged|closed_done|closed_dropped|rolled_back`;`holder?`(软锁)。`closed_dropped/rolled_back` **反哺护栏指标**。
- **Issue**:`source: user_report|competitive_radar`;`triage: new|accepted|non_issue|duplicate`。
- **Gate**(验收门):`check: machine|human|hybrid`;`boundary?: auth|payment|public_release|data_delete|...`。`check=machine` 才能自动闭环;补 Gate = 直接抬自主率。
- **Escalation**(把"卡住"实体化):`reason`、`valueAtRisk`(排序键)、`routeTo`、`groupKey`(approve-all-similar)、`state`。
- **★ Rule(R2 新增 · 刃B 的载体)**:一次人工决策固化出的可复用资产。见 §4。
- **★ Promotion(R2 新增)**:`Escalation/decision → Rule` 的提升流程实例,带 shadow→canary→active 生命周期 + 精度。

---

## 4. ★ 判断力复利机(刃B)—— R2 核心新增,蓝军 P0-1 后半的解

> R1 把"路由"做透,但"一次人工判断做完就蒸发"。没有复利,系统是 O(n) 人肉运维,商业上必崩。复利机把"人类判断"变成系统的**可累积资本**。

### 4.1 核心闭环
```
人工决策(Escalation 裁决 / approve-all-similar / 接管手改)
  → [一等动作:固化为规则 Promote-to-Rule]
  → Rule(草案) → shadow(影子运行:只预测不执行,比对人会怎么判)
  → 精度达标 → canary(小流量真执行) → active(全量自动解决同类)
  → 持续抽审精度;掉阈值 → auto-demote / 一键回滚
```
**这把人工负载从 O(n) 弯折为 O(distinct 未解决决策类)** —— 每条高价值规则消除"一类"未来介入,趋于次线性。这是"几个人管几千 agent"在**机制上可能收敛**的唯一通道(续上蓝军 C1)。**但"收敛"是领域赌注、不是保证**:仅当 distinct 决策类增长饱和时才次线性;开放产品域里它可能持续发散,届时这只是"换了名字的 O(n)"(蓝军 R2 实证,见 §4.6④ / §13 R-6)。

### 4.2 一次决策能固化成什么(5 类资产,统一走 Promotion)
| 资产类型 | 触发场景 | 例 |
|---|---|---|
| **Auto-approve 规则** | approve-all-similar 批量批准 = 天然一条规则 | "同蓝图+同 gate+同改动模式+VaR≤t → 自动放行" |
| **Gate 验收标准** | 人反复为"缺可机检标准"而被叫 | 把人脑里的验收转成 machine-checkable gate(作战室 9/13→13/13) |
| **Skill** | 重复的人工修法模式 | 抽成 Skill Hub 里一个 skill,蓝图引用 |
| **蓝图结构改动** | 某节点反复升级同类问题 | 加一个 auth 预检节点 / 加一条分支 |
| **ModelBinding 调整** | 某类 medium 产出反复被改写 | "该类升 Opus" |

> 与资产版本化(§9)和 D1 的 **base + override「promote 回基线」** 是**同一套机制**:个人的有效改进 → 提升为全体能力。这就是刃B 的复利路径,也根治了 D1 的"fork → drift / 无共享学习"。

### 4.3 防过拟合(关键:一条从单次决策学的规则可能是错的)
- **Shadow-first 强制**:任何新规则先影子运行——它预测决策,**人仍然亲自决定**,系统度量"规则会怎么判 vs 人怎么判"的一致率;一致率 ≥ 阈值才允许进 canary。
- **精度持续抽审**:active 规则自动解决的案例,仍按比例进**复审抽样池**(§5.2);精度掉阈值 → auto-demote 回 shadow 或回滚。**刃B 绝不能变成绕过护栏的刷分后门。**
- **保守聚类**:approve-all-similar 的 `groupKey` 用"同蓝图+同 gate+同模式+同 VaR 档"四元约束,宁可少批、不可错批(残留风险见 §13)。

### 4.4 复利度量(进度量看板 + 规则账本)
- **决策杠杆 Decision Leverage** = 每次人工决策平均消除的未来同类介入数 —— 北极星因子。
- **规则覆盖率** = 被规则自动解决的 escalation / 总 escalation。
- **规则精度** = 规则自动解决时,人会同意的比例(来自 shadow + 抽审)。
- **复利曲线** = 每千 loop 所需人工决策数随时间的趋势。**持续下降 = 健康复利;走平 = 退化成线性人肉运维**(看板首要趋势)。

### 4.5 规则账本(Compounding / Rule Ledger,新 surface)
一等屏:列出所有由决策固化的规则/门/技能,各自**覆盖率 / 精度 / canary 状态 / 累计省下的人工次数 / 一键回滚**。让刃B **可见、可量化、可治理**——被度量的东西才会被投资。组件:`<CompoundingLedger>` / `<RuleCard>` / `<PromoteToRuleAction>`(挂在每张决策卡上)。

### 4.6 复利机的三条硬前置 + 一个死锁修复(蓝军 R2 终审补丁)
> 独立终审判定:§4.1–4.5 纸面闭环,但漏掉"谁评审、听谁的、真值哪来",且把 C1 的瓶颈从"批 escalation"原地搬到"批规则"。以下为**进实现前必须先定**的三条 + 一个自埋死锁的解。

**① 定主体 —— Promotion 评审权与四眼(原 §4 缺失)。** 谁批准规则提升,按 kind/VaR 分流,**人评审只压在结构性/高危的少数规则上**(这正是不重蹈 O(n) 的关键):
- `auto_approve · 低 VaR` → **自动毕业**:shadow 精度 ≥ 阈值 + 最小样本量 + 下游回滚率达标,无需人评审。这是量大长尾、可自动化的部分。
- `gate / blueprint_patch / model_binding / 高 VaR` → **强制走 §9 宪法级评审 + 四眼**(owner 提、第二合格评审复核;质量影响类拉 PM/QA)。这是少数、结构性、改"宪法"的部分。
- 写进 §1 角色表:owner 可提 Promotion;`asset:publish` + 四眼 才能让结构性规则 active。

**② 定仲裁 —— 规则冲突裁决 + 治理面(原 `Rule` 只有 scopeKey、行为不确定)。**
- `Rule` 增 `precedence`(specificity 优先:scope 更窄者胜)+ 显式 priority;**提升时强制冲突检测**,与现存 active 规则矛盾则不得 active,先裁决。
- **Fail-safe 不变式:「升级人工」永远压倒「自动放行」。** 规则只能**降低**人工负载;任何冲突/歧义**默认回退到升级人工**(安全侧)。auto-approve 绝不覆盖一个本应 escalate 的信号。
- 规则账本升级为**可 rollup 的治理面**(按 scope/kind/蓝图聚合),不是 O(n) 平铺列表——与 §2 fleet rollup 同范式。

**③ 定真值 —— 精度真值源 + 带不确定性的度量(原精度只靠"原班人马当场点头")。**
- 规则精度必须挂**下游信号**(自动解决案例的回滚率/reopen 率),不只是 shadow 当场一致率;并入 §5 复审抽样。
- 所有复利度量**强制带样本量 + 置信下界**;`决策杠杆 / savedHumanDecisions` 是**反事实估计**,须带置信区间呈现,**禁止单独充当北极星**。
- 全局把"数学上成立"降级为"**机制上可能收敛**",附费米估算与前提(distinct 决策类增长饱和)。

**④ 解死锁 —— shadow 样本饥饿(§5.4 自埋的正反馈绞杀)。** §5.4 同时建议"加速复利降流入"与"降派发速率",但降流量会**饿死 shadow 样本** → 规则更难毕业 → 更依赖人 → 更想降流量。解:**背压必须选择性,不是一刀切**——紧急降派发时**保留 shadow 样本下限**(优先放行能喂"临近毕业"规则的流量),只节流"低学习价值"流量。背压是短期急救,复利是长期结构解,二者不在同一动作上对冲。

---

## 5. 指标 = 奖励函数 —— 经蓝军 P0-3 重写(健康 = 吞吐 × 质量)

> 在几千自治 agent 系统里,**你度量什么,agent 就把什么推到极致(含退化方式)**。Goodhart 是工程事实。

### 5.1 吞吐↔护栏强绑(禁止任何吞吐指标单独变绿)
| 吞吐(可被刷) | 强绑护栏(下游/滞后/难刷) | 护栏防的退化 |
|---|---|---|
| 提单接收率 `accepted/filed` | non_issue+dup 占比 ↑ 告警、**翻案率** | 把难单标 non_issue 压分母 / 挑安全单 |
| 解单合入率 `merged/opened` | **合入后 14 天回滚/缺陷率**、**AI-MR 人工改写率** | 合低质 MR / 挑评审最松的 agent |
| 净 issue 趋势(日降) | **reopen 率**、**关单分类占比**、**产品停滞探针**(雷达提单也异常↓?) | 关成 non_issue / 产品停滞伪装健康 |

### 5.2 复合健康定义(看板顶部一盏灯)
```
HEALTHY(绿)   ⇔ 三主指标达方向 AND 全部护栏未破红阈
AT_RISK(琥珀) ⇔ 主指标达标 但 ≥1 护栏进黄区
UNHEALTHY(朱砂)⇔ ≥1 护栏破红阈 OR ≥2 主指标反向
```
**铁律:护栏破阈一票否决** —— 吞吐再漂亮,护栏红 = 不健康。复审抽样池对一切 auto-pass / 一键批准 / 规则自动解决做事后人审,让"刷"有事后代价。

### 5.3 false-autonomy rate(蓝军 B2)
事后抽审中"agent 自闭环但实际应人工裁决"的漏报率。**裸自主率无意义,北极星 = 受 false-autonomy + 回滚率约束的自主率。** 看板把自主率与漏报率**并排**。

### 5.4 队列即流量系统(蓝军 C2)
"待人工"不是静态计数,是排队论 backlog:**流入速率 / 消化速率 / 净增 / 最老等待 / 按 VaR 分布的积压**。净增持续 >0 → 主动告警 + 建议(提自动通过阈值 / 加人 / **选择性**降派发 / 加速复利以从源头降流入)。**注意死锁:降派发与加速复利在"shadow 样本供给"上对冲——背压须保留 shadow 样本下限,只节流低学习价值流量(见 §4.6④)。** 组件 `<QueueFlowHeader>`。

### 5.5 file/fix 解耦防自我交易(蓝军 B1)
提单 Loop 与解单 Loop 指标**解耦**;"自规划提单→自己解"的闭环**单独标记 `self_trade` + 强制独立抽审**,度量 self-trade 闭环占比与其抽审命中率,防"自我交易的指标永动机"。

---

## 6. 人工瓶颈解法(蓝军 P0-4:O(n) 升级 vs O(1) 人)
三条按 VaR 分流,缺一不可:
1. **VaR 排序 + 截断**:低于风险阈值的**不进人脑**,走"自动通过 + 抽审"。
2. **低危批量 approve-all-similar**:把 O(n) 决策压成 O(类别)。**且每次批量 = 产出一条规则(喂刃B §4)** —— 瓶颈解法与复利机在此合流。
3. **高危强制摩擦(anti-rubber-stamp)**:未展开 diff 不能批 + 必填理由 + **禁一键全批**。**批量(2)与摩擦(3)严格按 VaR 分流:低危才可批量,高危禁止批量。**

---

## 7. 多角色并发治理(蓝军 P0-5)
- **RBAC 能力可见性**:UI 按能力渲染,无权动作隐藏/禁用+原因,不是人人一套按钮。`approve:protected` 与 `asset:publish` 受**四眼原则**(高 VaR 强制二人)。
- **共享资产乐观锁**:Agent.md/蓝图编辑用版本号 CAS + 在编提示("Mei 正在编辑");冲突强制 rebase。**禁 last-write-wins 静默覆盖**(配置脏写 = fleet 级故障)。
- **loop 单一在控人 claim 锁**:进入人工裁决后第一个认领者"在控",其余人见"Wei 处理中(02:14)",动作转"请求接手"。状态机命令串行化 + 幂等,杜绝双批/批了又退。
- **"接管"事务语义**(蓝军 E2):接管 = 获独占控制权 + **暂停所有 agent 自主写**,引擎到安全检查点让权并置 `human_holding`;UI 明示"正在交接/已接管,agent 已暂停"。绝不前端假装。
- **通知按角色+认领关系定向**:PM 收健康/护栏告警,工程师收自己 owns/claims 的 loop,owner 收基线下游影响。每人队列只装"该他的"。

---

## 8. 状态机:并行/分支(蓝军 F2)
DAG 时间线:`fork/join`(自测∥静态审查,皆绿才进 review)、`branch`(评审→合入 / 退回 developer 回边)、多 `desk.active` 并行泳道。Agent 数与角色**由蓝图决定**,组件按数据渲染,**不写死 5**。模板态(蓝图)与运行态共用同一 `<LoopGraph>`。

---

## 9. 配置面五元组 + 治理(蓝军 D1/F3)
配置面 = **MD + ModelBinding + Skills + PermissionScope + Gate**,作为整体**版本化 + provenance + 回滚 + 影响面预估 + base/override**。"只改 MD"是抽象泄漏——UI 明示"改哪项改变什么行为"。**改基线 = 改宪法,走轻量评审**(影响成千上万 agent)。base/override 的 promote 通道 = §4 复利机。

---

## 10. TS 组件架构(蓝军 G1:这是 app,不是"一个组件")
**交付物 = 一个工作台 app(feature 模块),由一组有清晰状态归属的可复用 TS 组件组合而成。** 边界沿**状态归属 + 角色权限 + 数据生命周期**切,不沿视觉区块切。

- **应用层(持路由/角色/实时订阅/权限计算)**:`<WorkbenchApp>` + 各 `*Screen`。"数据从哪来 / 你是谁 / 你能做什么"只活在这层。
- **组件层(纯展示+受控交互,数据全由 props 进)**:
  - 可复用原子:`<DecisionQueue>`/`<EscalationCard>`(VaR+批量+强制摩擦)、`<FleetRollup>`/`<FleetBucket>`、`<LoopOffice>`(工程师介入/PM 只读/全屏三处复用,差异靠 `permissions`)、`<LoopGraph>`(模板/运行双态)、`<MetricVsGuardrail>`、`<RiskMeter>`、`<RoleGatedAction>`(横切 RBAC 渲染)、`<RealtimeStatus>` store(单一事实源+多 lens 订阅)。
  - **★R2 复利原子**:`<PromoteToRuleAction>`、`<CompoundingLedger>`/`<RuleCard>`、`<QueueFlowHeader>`。
- 关键类型签名见 `ux-spec.md §7.3`(`LoopInstance`/`LoopGraphNode/Edge`/`LoopOfficeProps`/`EscalationCardProps`/`AgentConfig` 五元组等)+ 新增:
```ts
interface Rule {
  id: string;
  origin: { escalationId: string; decidedBy: string; at: string }; // provenance
  kind: 'auto_approve' | 'gate' | 'skill' | 'blueprint_patch' | 'model_binding';
  scopeKey: string;            // 同 approve-all-similar 的 groupKey
  status: 'shadow' | 'canary' | 'active' | 'demoted' | 'rolled_back';
  precision: number;           // shadow/抽审 一致率
  coverage: number;            // 累计自动解决数
  savedHumanDecisions: number; // decision leverage 累计
}
```

---

## 11. MVP + 路线图
- **M0(MVP)**:把已上线的 **file + fix** 两类 Loop 完整包进来 —— 决策队列 + fleet rollup + 虚拟办公室(DAG)+ Issue 派发 + 度量看板(吞吐↔护栏+健康灯+false-autonomy+队列流量)+ **复利机 MVP(approve-all-similar→auto-approve 规则 + 规则账本 + shadow/canary)** + 多角色(RBAC+lens+软锁+路由)+ 资产库(五元组改+灰度/回滚)。七环节其余环节 rail 占位"待建"。
- **M1** 评审/测试·验证成熟(补 Gate 9/13→13/13)、并行 DAG 落地多泳道办公室、复利扩到 Gate/Skill 类规则。
- **M2** 规划/进度纳管;蓝图 DAG 可视化编辑器。
- **M3** 集成/发布(权限层+对外四眼)、需求/设计。
- **M4** 运维/事故(接 MaaS/CI);全七环节闭环。
- **横切** 规模化:fleet 虚拟化 + 路由策略可配置化 + 复利曲线驱动的容量规划。

---

## 12. 蓝军 R1 P0 对账(逐条)
见 `decision-log.md`。R1 六个地基级问题 **5 个判为"已结构性解决",1 个(护栏阈值标定)判为"机制就位、参数待运营校准"**。

---

## 13. 诚实声明 + 未决风险(R2 残留,待 R3 / 运营验证)
**流程诚实**:R2 收敛由编排者代行(两个 worker agent 连续 infra stall)。已另起**全新独立蓝军**对本文终审(`blueteam-r2.md`),判决 **「半活」**:刃A 可进实现;**刃B 复利机补齐 §4.6 三条硬前置前不得进实现**。终审实证了"编排者自审"的盲区——R2 的明星机制(复利机)恰恰最少被自我质疑,其地基洞集体躲过了下表 R1 的点名,故新增 R-6~R-8。

| # | 未决风险 | 现状 | 待办 |
|---|---|---|---|
| R-1 | **护栏红阈标定**(回滚率 8%/改写率 25% 等为占位) | 机制就位,参数是猜的 | 用真实基线数据校准;否则护栏自身会被 game(偷偷调松) |
| R-2 | **接管让权 SLA**:引擎到安全检查点的时间窗未定;agent 跑飞如何强制夺权 | 语义已定,SLA 未定 | 定最大让权时延 + 强制 kill 通道 |
| R-3 | **复利规则过拟合**:单次决策学出的规则可能错 | shadow-first + 精度抽审 + auto-demote 缓解 | 运营验证精度阈值;盯"规则把错的自动放行"的尾部风险 |
| R-4 | **approve-all-similar 聚类边界**:`scopeKey` 划错会错批一批高危 | 四元保守约束 + 仅低危可批 | 聚类需可审计 + 错批可追溯回滚 |
| R-5 | **冷启动人力缺口**:复利见效前,稳态"需人工"可能 > 人力上限 | 北极星已含队列流量告警 | 过渡期策略(借作战室 Lead 池 / 临时提阈值 / 限派发) |
| **R-6** 🔴 | **复利机=O(n) 改名?** 瓶颈从"批 escalation"搬到"批规则";distinct 决策类在开放域可能持续发散 | 蓝军 R2 新发现 · **P0** | §4.6① 把人评审压到少数结构性规则;但"distinct 类是否饱和"是未验证的领域赌注,需埋点实测增长曲线 |
| **R-7** | **复利转换期净增人工**:shadow 期人仍要判,见效前是负投资 | 蓝军 R2 新发现 · P1 | 需冷启动容量预算 + 明确"何时回正"的判据(决策杠杆 > 1 的拐点) |
| **R-8** | **精度真值 / 规则冲突 / 评审主体** 三件套 | 蓝军 R2 · P0/P1 | §4.6①②③ 已给设计解,待实现与运营验证 |

---

## 附:产物清单(`sinan-workbench-design/`)
`00-brief.md`(共享输入) · `pm-spec.md`/`ux-spec.md`/`blueteam-r1.md`(R1 全文) · `blueteam-r2.md`(独立终审) · **`DESIGN.md`(本文,权威)** · `decision-log.md`(P0 裁决) · HTML 多屏 mockup。
