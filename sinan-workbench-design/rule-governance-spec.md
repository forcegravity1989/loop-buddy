# 司南 · Loop 工作台 — 规则治理 Spec(刃B·复利机的「数学闭环」)

> **范围**:本文只解一件事——把 DESIGN.md §4「判断力复利机」从「纸面闭环」推到「数学闭环」,专攻**规则治理(Rule Governance)**。
> **铁律**:只设计、不实现。TS 仅到 interface / 不变式形状,不写函数体。
> **权威依据**:`DESIGN.md`(§4 复利机 / §4.6 三条硬前置 / §5.4 死锁 / §9 五元组 / §13 R-6~R-8)+ `blueteam-r2.md`(终审,逐条解其 R2-A~R2-I)。
> **领域模型对齐**:严格复用既有 `Escalation`(pm-spec §4.2)/ `Rule`(DESIGN §10)/ `Promotion` / `Gate` / 五元组(MD+ModelBinding+Skills+PermissionScope+Gate)。本文只**扩字段、加不变式、加治理面**,不另起炉灶。
>
> **本文要解掉的洞(逐条对账见 §8)**:R2-A(无评审主体)、R2-B(冲突真空)、R2-C(shadow 负投资+长尾)、R2-D(系统性偏差被复利放大)、R2-E(只断言不证明)、R2-F(字段≠真值)、R2-G(缓解≠解决)、R2-H/I(§13 漏 P0)+ R-6 / R-7 / R-8。
>
> **预设**:一个全新蓝军 R3 会逐条攻击本文。我在每个关键决策后内嵌 `// 抗R3` 注记提前堵漏;§9 集中列「我自己仍担心的洞」,不藏。

---

## 0. 第一性:复利机数学成立的三个充要条件(把判决变成可证伪命题)

blueteam-r2 把「复利机数学成立」拆成三条必须**同时**满足的命题,本文逐条给出闭合机制。这是全文的骨架——三大节(§1 定主体 / §2 定仲裁 / §3 定真值)精确对应:

| 条件 | 命题(必须为真) | 谁来闭合 | 闭合机制摘要 |
|---|---|---|---|
| **C-a 生产成本次线性** | 把一次决策固化成可信规则的**人工**,远小于它消除的人工总量;且固化动作本身不是 O(规则数) | **§1 定主体** | `kind × VaR` 分流矩阵:长尾低危**自动毕业(零人评审)**;人评审只落在稀疏的「结构性规则」集 |
| **C-b 维护成本有界** | N 条规则共存,治理(冲突/过期/漂移/抽审)的人工不随 N 线性增长 | **§2 定仲裁** | precedence 引擎(机器裁决冲突,非人逐条)+ rollup 治理面(非 O(n) 平铺)+ 死规则自动回收 |
| **C-c 真值可廉价获得** | 持续知道 active 规则「自动放行的案例人到底同不同意」 | **§3 定真值** | 下游回滚/reopen 率(人之外的信号)为真值源 + 置信下界驱动 demote |

> **抗R3**:这三条是**可证伪**的。§1 给 C-a 的费米不等式 + 证伪条件;§3 给 C-c 的真值源与坏规则熔断;§2 给 C-b 的治理面复杂度论证。任何一条被实测推翻,复利机即退化为 O(n),本文 §7 给出退化时的明牌降级路径。

---

## 1. 定主体 —— Promotion 评审治理模型(解 R2-A / R2-H · 闭合 C-a)

### 1.1 核心论点:人评审是**稀疏集**,不是全集

blueteam-r2-A 的杀招:「每条 Promotion 都要人评审 → O(distinct 决策类),换名 O(n)」。本文的反制不是「减少评审」,而是**把评审从决策流里结构性剥离**——绝大多数规则**根本不进人眼**,人评审只压在「会改变系统结构性行为」的少数规则上。

关键认识:**Promotion 的 VaR ≠ 它来源 Escalation 的 VaR**。一条规则的真实风险 = `kind 的结构破坏力 × scope 覆盖的最高 VaR 档 × 覆盖广度`。auto_approve 一类低危 escalation,即使发生一万次,固化成的规则也只是「让一类本来就该放行的低危流量不再叫人」——它的**下行风险有上界**(最坏 = 放行了一个本该叫人的低危项,被 §3 抽审兜住)。而一条 `blueprint_patch` 改的是成千上万实例的 DAG 结构,**一次错就是 fleet 级**。二者绝不能同一条评审通道。

### 1.2 分流矩阵:`kind × VaR` → 评审等级

这是 §4.6① 的可执行展开。每条 Promotion 入流时由引擎计算 `governanceTier`,**机器分流,不是人分流**(分流本身 O(1)):

| Rule.kind | scope 最高 VaR 档 | governanceTier | 评审主体 | 闸门 |
|---|---|---|---|---|
| `auto_approve` | 低(≤ t_low) | **`auto_graduate`** | **无人**(引擎) | shadow 精度下界达标 + 最小样本量 + **下游回滚率达标** → 自动 active |
| `auto_approve` | 中(t_low~t_high) | **`four_eyes`** | 在控工程师**提**,第二合格工程师**复核** | 四眼;复核人不得 = 提交人(职责分离) |
| `auto_approve` | 高(> t_high) | **`constitutional`** | owner 提 + 四眼 + **拉 PM/QA** | 宪法级评审(§9);高 VaR **禁止自动毕业** |
| `gate` | 任意 | **`constitutional`** | owner 提 + 四眼(质量影响拉 PM/QA) | 改验收标准 = 改宪法 |
| `blueprint_patch` | 任意 | **`constitutional`** | owner 提 + 四眼 | 改 DAG 结构 = 改宪法,影响面预估必填(§9) |
| `model_binding` | 任意 | **`constitutional`** | owner 提 + 四眼 | 改模型档 = 改成本/质量曲线 |
| `skill` | 任意 | **`four_eyes`**(默认) / `constitutional`(若 skill 触碰 protected boundary) | owner 提 + 第二评审 | skill 被多蓝图引用时升 constitutional |

```ts
type GovernanceTier = 'auto_graduate' | 'four_eyes' | 'constitutional';

interface PromotionGovernance {
  tier: GovernanceTier;            // 引擎按 kind × VaR 计算,非人填
  reason: string;                  // 为何落此 tier(可审计)
  reviewers: string[];             // auto_graduate=[]; four_eyes/constitutional 必填且 ≥2
  requiresPmQa: boolean;           // 质量影响类强制
  slaHours: number;                // 见 §1.5
}
```

> **抗R3 ①(分流可被 game?)**:有人可能把高危规则**拆成多条窄 scope 低危规则**绕过 constitutional。反制:`governanceTier` 计算包含**「同源 Promotion 聚集检测」**——同一 origin.decidedBy 在短窗口内提交的、scope 相邻的多条规则,合并计算等效 VaR(见 §2.6 反碎片化)。拆分不降总风险。
> **抗R3 ②(VaR 档自己就是 §13 R-1 的未标定参数)**:`t_low/t_high` 复用 §5 护栏阈值的同一套标定流程(机制就位、参数运营校准),且**默认偏保守**(档位不确定时向上取整 → 落更严的 tier)。fail-safe 方向:分流的歧义永远偏向「更多人看」,不偏向「自动毕业」。

### 1.3 `auto_graduate` 是复利机的主体(C-a 的核心)

**这是整个数学论证的命门**:复利的「量」全在 `auto_graduate`(长尾低危 auto_approve),它**零人评审**。人评审只在 `four_eyes`/`constitutional`——而这两类按定义是**稀疏的结构性规则**(改 gate/blueprint/model 的事件,数量级远低于 escalation 流量)。

`auto_graduate` 的安全性不靠人评审,靠**三重机器闸门 + 事后兜底**:
1. shadow 精度**置信下界** ≥ 阈值(§3,用下界不用点估计 → 小样本天然卡住);
2. 最小样本量 `n_min`(防 5 个样本就 100% 的虚假信心);
3. **下游回滚率达标**(§3,人之外的真值源)——即使 shadow 一致率高,只要下游出事率超标就不毕业;
4. 毕业后**持续抽审 + 下游监控**,破阈即 auto-demote + blast-radius 回溯(§3.4)。

> **抗R3 ③(零人评审的 auto_approve 不就是 R2-A.2「无人把守的免审金牌」?)**:不是。R2-A.2 的恐惧是「把改宪法的权力下放给刷分压力最大的人」。但 `auto_graduate` **够不到宪法**——它按定义只能是 `auto_approve × 低 VaR`,kind 和 VaR 双重封顶,改不了 gate/blueprint/model,scope 也覆盖不了高危档。它能造成的最坏后果(放行一个本该叫人的低危项)与「无人评审」的风险**量级相称**,且被 §3 下游信号兜底。把「结构性权力」与「长尾自动化」用 `kind × VaR` 硬隔离,正是不重蹈 O(n) 又不开后门的唯一解。

### 1.4 人评审负载的量级论证(正面回答 R-6 · 费米估算见 §6)

把人评审拆成两条独立流水线,只有第二条吃人:

```
Promotion 流入
  ├─ auto_graduate 路径 ── 引擎处理 ── 人工 = 0          ← 长尾主体,量大
  └─ 结构性路径(four_eyes/constitutional)── 人评审 ── 人工 > 0  ← 稀疏,量小
```

**论证骨架(完整费米见 §6)**:设 escalation 流入率为 `E`/天,其中可固化为规则的占比 `p_rule`,其中落在结构性路径(非 auto_graduate)的占比 `p_struct`。则人评审负载 ≈ `E × p_rule × p_struct × c_review`。

关键在 `p_struct` **极小且不随规模发散**:结构性规则对应的是「系统骨架的改动」(新增一类 gate、改一个蓝图节点),这类事件的发生率受**蓝图/gate/model 的总数**约束,而非受 escalation 流量约束。fleet 从 1k 涨到 100k loop,escalation 流量线性涨,但「需要新增一类 gate」的事件**不线性涨**——蓝图结构会饱和(§4 的 distinct 决策类饱和假设,在「结构层」比在「全决策层」强得多)。

> **抗R3 ④(p_struct 真的不发散吗?这不就是 R-6 的赌注换个地方押?)**:诚实承认——`p_struct` 的有界性是**领域赌注**,但比 §4.1 原始的「distinct 决策类饱和」赌注**弱得多、可证伪得多**。原赌注押「所有决策类饱和」;本文只押「**结构性**决策类饱和」,把长尾发散的部分甩给零成本的 `auto_graduate`。§6 给 `p_struct` 的可埋点实测指标 + 证伪条件:**若结构性 Promotion 提交率不随 fleet 规模衰减(每千 loop 的结构性规则新增数不下降),则赌注被证伪 → 复利机在结构层退化为 O(n)**,触发 §7 降级。

### 1.5 SLA + 批量评审规则

- **SLA(评审时延上界,防评审本身变新瓶颈)**:
  - `four_eyes`:复核 SLA ≤ 24h;超时**不自动放行**(fail-safe),而是降级为「规则保持 shadow,继续零负载积累」——超时的代价是「晚毕业」,绝不是「未审先放」。
  - `constitutional`:SLA ≤ 72h,超时升级到 owner 上级 + 进 §5.4 队列流量告警(评审 backlog 是一条独立被监控的队列,见 §2.7)。
- **批量评审规则(高危禁止批量,对齐 DESIGN §6.3)**:
  - `auto_graduate`:本就无人,不涉及批量。
  - `four_eyes`:**允许**「同 scopeKey 族、同 kind、同 VaR 档」的规则**成组复核**,但必须展开每条的 shadow 报告(anti-rubber-stamp:未展开不能批);成组 ≤ N 条上限。
  - `constitutional`:**严格禁止批量**——每条 gate/blueprint/model 规则**逐条独立评审 + 独立填理由 + 四眼**。这是 DESIGN §6.3「高危禁一键全批」在规则层的强制落地。

```ts
// 不变式 INV-REVIEW-1:tier 越高,批量上限越低;constitutional 批量上限恒为 1
invariant: batchLimit(constitutional) === 1
invariant: tier === 'auto_graduate' ? reviewers.length === 0
                                    : reviewers.length >= 2 && !reviewers.includes(submittedBy)
// 抗R3:reviewers 不含提交人 = 职责分离硬约束,堵 R2-A.1「运动员给自己发金牌」
```

### 1.6 写进 §1 角色表 + §7 RBAC(对齐 DESIGN,堵 R2-A.1 / R2-I「四眼没挂 Promotion」)

| 角色 | 在 Promotion 上的权限 | 新增能力位 |
|---|---|---|
| PM/Lin | **不提、不批**普通规则;`constitutional` 中质量影响类**作为强制第三方评审参与**(裁判身份) | `promotion:review_qa`(只在质量影响类点亮) |
| 工程师/Wei | 可**提** auto_approve Promotion;可**复核** four_eyes(非自己提交的) | `promotion:propose`、`promotion:review` |
| owner/Mei | 可**提** gate/blueprint/model Promotion;`constitutional` 走 §9 宪法评审 | `promotion:propose_structural`、`asset:publish`(已存在,现明确覆盖 Promotion) |

> **抗R3 ⑤(四眼在稀缺人力下不是 C1 放大器吗?R2-I 提的)**:是,所以四眼**只挂在稀疏的结构性规则上**(§1.2 矩阵),不挂在 `auto_graduate`。结构性规则的量级(§6 估算)远小于人力上限,四眼的额外成本被吸收。**绝不**对长尾 auto_approve 上四眼——那才会把四眼变成 C1 放大器。这与 R2-A.1「最该四眼的地方反而没四眼」不矛盾:最该四眼的是**改宪法**(已上),不是长尾免审(那是 kind/VaR 封顶的安全集)。

---

## 2. 定仲裁 —— 规则冲突裁决 + 治理面(解 R2-B / R2-H · 闭合 C-b)

### 2.1 Rule 扩字段:precedence / specificity / priority

DESIGN §10 的 `Rule` 只有 `scopeKey: string`,行为不确定。扩字段(向后兼容,只增不改):

```ts
interface Rule {
  // ...DESIGN §10 既有字段(id/origin/kind/scopeKey/status/precision/coverage/savedHumanDecisions)
  effect: 'auto_approve' | 'escalate';   // ★ 规则的「断言方向」:放行 or 强制叫人
  scope: RuleScope;                       // ★ 结构化 scope,替代裸 scopeKey 字符串做 specificity 计算
  specificity: number;                    // ★ 由 scope 维度数 + 各维度取值粒度派生(引擎算,非人填)
  priority?: number;                       // ★ 显式优先级(仅 specificity 打平时用,稀有)
  effectiveFrom: string;                   // ★ effective-dating:规则生效起点(冲突裁决的时间维)
  supersedes?: string[];                   // ★ 本规则显式取代的旧规则 id(评审时人确认)
}

interface RuleScope {
  blueprintId?: string;       // 维度1
  gateId?: string;            // 维度2
  changePattern?: string;     // 维度3(改动模式)
  varBand?: 'low'|'mid'|'high'; // 维度4(对齐 approve-all-similar 四元约束)
  // 维度越多 = scope 越窄 = specificity 越高
}
```

> **抗R3 ⑥(scopeKey 从 string 改 struct 是破坏性变更?)**:`scopeKey` 保留为 `scope` 的稳定序列化(provenance/审计仍用它),`scope` 是其结构化镜像。approve-all-similar 的四元 groupKey(同蓝图+同 gate+同模式+同 VaR 档)**正好**是 `RuleScope` 的四个维度——零概念新增,只是把已有四元约束显式化为可计算结构。

### 2.2 冲突裁决语义(机器裁决,人工 = 0 → 这是 C-b 的核心)

当一个 escalation 同时命中多条规则,引擎按**确定性优先级链**裁决,**全程无人**:

```
裁决顺序(短路,前者命中即停):
1. fail-safe 优先:任一命中规则 effect='escalate' → 结果 = escalate  ★ 见 §2.3,永远压倒放行
2. specificity 高者胜:scope 更窄(维度更多/取值更具体)的规则覆盖更宽的
3. priority 高者胜:specificity 打平时,比显式 priority
4. effectiveFrom 新者胜:priority 也打平时,后生效的覆盖先生效的(最新人类意图)
5. 仍无法裁决(理论上不该到这):★ fail-safe 兜底 → escalate(默认叫人)
```

```ts
// 不变式 INV-CONFLICT-1:裁决是全序且确定,同一输入恒得同一输出(no randomness)
// 不变式 INV-CONFLICT-2:裁决全程不触发人工(O(1) per escalation,匹配用 scope 索引)
```

> **抗R3 ⑦(specificity 打平 + priority 打平 + effectiveFrom 打平,真会发生吗?)**:effectiveFrom 是时间戳,实际打平概率≈0;即便人为构造,第 5 步 fail-safe 兜底保证**绝不进入未定义行为**——最坏退化为「叫人」,这是安全侧。裁决永远有确定输出,这是 R2-B「行为不确定 = 击穿 false-autonomy」的正面堵口。

### 2.3 Fail-safe 不变式:「升级人工」永远压倒「自动放行」(引擎层强制)

这是复利机**不可协商**的安全地基,在引擎层用类型 + 运行时双重强制:

```ts
// 不变式 INV-FAILSAFE(刃B 的宪法,DESIGN §4.6② 的引擎落地):
// 任何冲突 / 歧义 / 缺省 / 异常,结果必须偏向 escalate(叫人),绝不偏向 auto_approve(放行)。
//
// 强制点(全部在引擎层,不靠 UI 自觉):
// (a) 裁决链第1步:命中集里只要有一条 escalate 规则,无视 specificity 直接 escalate
// (b) 规则匹配异常 / 引擎降级 / 数据不一致 → fallback = escalate(fail-closed,非 fail-open)
// (c) 规则刚 promote 进 active 但 §3 真值信号尚未回流(冷启动盲区)→ 该规则暂只能 escalate 不能 auto_approve
// (d) auto_approve 规则覆盖的 scope 若与任一 boundary(auth/payment/public_release/data_delete)相交 → 强制 escalate(boundary 不可被规则自动放行,对齐 DESIGN Gate.boundary)
invariant: resolve(matched).effect === 'auto_approve'
           ⟹ (∀ r ∈ matched: r.effect !== 'escalate')
           ∧ (matched.every(r => r.truthSignalReady))
           ∧ (scope ∩ protectedBoundaries === ∅)
```

> **抗R3 ⑧(fail-safe 会不会让一切都退回叫人 → 复利失效?)**:不会。fail-safe 只在**冲突/歧义/盲区**触发;干净命中(单规则、无冲突、真值已回流)正常 auto_approve。fail-safe 是「歧义时的方向」,不是「默认全叫人」。它牺牲的是「歧义那部分流量的自动化」,换「绝不错误放行」——这正是 §5 「护栏破阈一票否决」在规则层的同构。**effect='escalate' 类规则本身也是复利资产**(它复利的是「这类必须叫人」的判断,减少了「漏判该叫人」的 false-autonomy),不是复利的反面。

### 2.4 提升时强制冲突检测(堵 R2-B「规则层复活 D1 fork-drift」)

每条 Promotion 进 canary 前,引擎对**现存 active 规则集**做交集分析:

```ts
interface ConflictReport {
  promotionId: string;
  overlaps: Array<{                    // scope 相交的现存规则
    ruleId: string;
    relation: 'subset' | 'superset' | 'intersect' | 'identical';
    effectClash: boolean;              // ★ effect 相反(一个 auto_approve 一个 escalate)= 语义对立
  }>;
  verdict: 'clean' | 'needs_arbitration';
}
```

- `effectClash === true`(语义对立,如 R2-B 的「工程师A放行 vs 工程师B叫人」)→ **阻断提升**,生成一条 `arbitration` Escalation,routeTo 双方 + owner,人裁决「以哪条为准」(走 §1 评审,且这条裁决本身计入评审负载——诚实并账,不藏)。
- `relation === 'identical'` → 提示重复,合并而非新增(防规则爆炸)。
- `verdict === 'clean'` → 正常进 canary。

> **抗R3 ⑨(冲突检测是 O(规则数) 的全集扫描,不就是新的 O(n)?)**:不是 O(n) 人工——是 O(候选)**机器**计算,且用 §2.5 的 scope 索引把候选集从「全集」剪枝到「scope 相交集」(通常 O(log n) 或近常数)。人工只在 `effectClash` 时介入,而语义对立是稀有事件(同一窄 scope 被两个人反向判),不是常态。这把 D1 的 fork-drift 在规则层**变为可检测、可阻断、可裁决**——R1 时 Agent.md 的 drift 是静默的,这里是显式拦截的,严格优于。

### 2.5 规则集 rollup 治理面(非 O(n) 平铺,对齐 §2 fleet rollup 范式)

R2-B 痛击「§4.5 账本是个列表 = 重蹈 R1 P0-2 的 O(n) 列表」。本文把规则账本升级为**与 fleet rollup 同构**的可治理面:

- **默认视图 = 按 `scope 维度 / kind / 蓝图` 分桶 rollup**,永不线性铺开几千条规则。
  例:`code-fix-v3 · auto_approve · 412 规则 · 覆盖率 78% · 3 冲突 · 7 死规则`,点桶下钻。
- **治理信号(桶级聚合,机器算)**:
  - **覆盖热力图**:哪些 scope 区域被规则覆盖密、哪些是洞(escalation 反复发生但无规则)。
  - **空洞告警**:高频 escalation 区域无 active 规则 → 复利机会点(正向)。
  - **重叠/冲突计数**:桶内 effectClash 数 → 治理债。
  - **死规则**:长期 0 命中 或 长期 0 抽审的 active 规则 → 自动标记待回收(§2.8)。
  - **规则总数随时间曲线**:`规则数无界增长本身 = 退化信号`(对齐 §4.4「走平=退化」的同款警觉)。

```ts
interface RuleSetRollup {       // 套用 §2 FleetRollup 范式
  dimension: 'scope' | 'kind' | 'blueprint';
  buckets: Array<{
    key: string;
    ruleCount: number;
    coverage: number;           // 该桶 escalation 被规则解决占比
    conflictCount: number;      // effectClash 数
    deadRuleCount: number;      // 待回收数
    holes: number;              // 高频未覆盖 escalation 区域数
  }>;
}
```

> **抗R3 ⑩(rollup 视图本身在 10万规则时会不会也卡?)**:rollup 是预聚合(桶级指标增量维护),扫描层永远是 O(桶数) 而非 O(规则数),桶数受 scope 维度基数约束(有界)。下钻才进单规则,虚拟滚动兜底——与 §2 fleet 的 1万→几千实例同款解法,已被路由侧验证可行。

### 2.6 反碎片化(堵 §1.2 抗R3① 的拆分绕过)

防「把一条高危规则拆成 N 条窄规则绕过 constitutional / 制造规则爆炸」:
- **同源聚集检测**:同一 `origin.decidedBy` 在时间窗内提交的、scope 相邻的多条规则,合并计算**等效 VaR 与等效结构破坏力**,按合并结果定 tier。拆分不降评审等级。
- **规则数预算**:每个 scope 桶有「健康规则密度」上限,超出触发治理告警(可能是过度碎片化或 scope 设计有问题),进 §2.5 治理面。

### 2.7 评审队列并入 §5.4 排队论(堵 R2-A.2 / R2-I「QueueFlowHeader 只建模 escalation 流」)

把 DESIGN §5.4 的队列流量模型**从单队列扩为多队列**,显式建模三条耦合的流:

```ts
interface QueueFlows {       // 扩展 DESIGN <QueueFlowHeader> 的数据契约
  escalation: FlowMetrics;   // 既有:人工裁决流
  promotionReview: FlowMetrics; // ★ 新增:规则评审流(four_eyes + constitutional)
  shadowSampling: FlowMetrics;  // ★ 新增:shadow 标注供给流(见 §5 死锁)
}
interface FlowMetrics { inflow: number; throughput: number; net: number; oldestWaitMin: number; }
```

- 三条流**同屏并排**,各自净增独立告警。规则评审流净增持续 >0 → 评审成新瓶颈的早期信号(直接验证 R-6 是否成真)。
- **跨队列协调策略**见 §5(死锁解)。

> **抗R3 ⑪(把评审流并进来,不就证明你新增了一条 O(n) 人工流?)**:并进来是为了**诚实监控**它,不是承认它发散。§6 费米论证 `promotionReview` 流的稳态量级远小于 escalation 流(因 p_struct 极小);把它放进 QueueFlowHeader 正是为了**实测验证这个论断**——若它真发散,告警会先于崩盘点亮,触发 §7 降级。藏起来才是 wishful thinking。

### 2.8 规则集版本 / 快照 / 回滚边界(堵 R2-B.3「群体性坏规则需一致回退」)

- 单条 Rule 可回滚(DESIGN §4.5 已有)。
- **新增规则集快照**:整个 active 规则集可打 `RuleSetSnapshot`(版本化,对齐 §9 资产版本化)。「上周引入的某条规则群体性放行了坏东西」→ 一键回退到一致旧快照,而非逐条回滚(逐条会留下不一致中间态)。
- **死规则回收**:长期 0 命中/0 抽审的规则,经治理面确认后归档(`status: rolled_back` 或新增 `archived`),防规则集无界膨胀侵蚀 C-b。

```ts
interface RuleSetSnapshot { id: string; at: string; ruleIds: string[]; reason: string; }
// 不变式 INV-ROLLBACK-1:回退到快照是原子的,中间不存在「半套规则 active」的不一致窗口
```

---

## 3. 定真值 —— 精度真值源 + 带不确定性的度量(解 R2-D / R2-E / R2-F · 闭合 C-c)

### 3.1 真值源:下游回滚/reopen 率(人之外的信号)—— 堵 R2-D「系统性偏差被复利放大」

R2-D 的杀招:shadow 用「人」当 ground truth,当人**系统性判错**时,规则高度一致地复制错误,100% 精度完美毕业,然后机器速度放行坏东西。两道闸(shadow + 抽审)用同一个有偏真值源 = **一道闸的两个副本**。

**解:引入与「人当场判断」正交的第二真值源——下游、滞后、人之外的事实信号**(复用 DESIGN §5.1 已定义的护栏):

```ts
interface RuleTruthSignals {
  // 真值源 A(易被系统性偏差污染,作参考不作裁决):
  shadowAgreement: { rate: number; n: number; lowerBound: number };  // 人当场一致率(带样本+下界)
  // 真值源 B(难造假,作 active 规则的主真值):
  downstream: {
    rollbackRate14d: { rate: number; n: number; upperBound: number }; // 放行案例 14 天回滚率(用上界,保守)
    reopenRate: { rate: number; n: number; upperBound: number };       // reopen 率
    defectRate: { rate: number; n: number; upperBound: number };       // 合入后缺陷率
  };
  truthSignalReady: boolean;  // 下游信号是否已积累到可裁决(冷启动期为 false → §2.3(c) 强制只 escalate)
}
```

- **毕业判据(`auto_graduate`)**:`shadowAgreement.lowerBound ≥ 阈值` **AND** `downstream.rollbackRate14d.upperBound ≤ 阈值`。两个真值源**都**达标才毕业——shadow 防方差(偶然错),下游防偏差(系统错)。
- **真值源 B 对系统性偏差免疫的原因**:「合入后 14 天是否被回滚/出缺陷」是**事实**,不依赖原班人马是否「自信地判对」。人系统性误判「某鉴权绕过是安全的」→ 规则放行 → 但下游真出了安全事故 → 回滚率飙升 → 规则被 demote。**事实信号穿透人的集体盲区**。

> **抗R3 ⑫(下游信号有滞后,14 天里坏规则已大规模放行)**:对,所以三重缓冲:(1) canary 阶段小流量真执行**专门为了在低 blast-radius 下采集早期下游信号**,不是直接全量;(2) §2.3(c) 真值未回流期规则只能 escalate;(3) §3.4 的 blast-radius 回溯——下游破阈不仅 demote,还**追溯复核已放行历史案例**。滞后的代价被「小流量 canary + 强制回溯」压到有界。
> **抗R3 ⑬(下游回滚率本身会不会被 game,比如不回滚硬扛?)**:回滚率与 reopen/defect 三信号**交叉**,且回滚率是 §5 已有护栏(护栏破阈一票否决在管它)。单一信号被压,其余两个会动;且「硬扛不回滚」会推高 defect/reopen。这是 §5「吞吐强绑多个难刷护栏」的同构防御。

### 3.2 复利度量强制带样本量 + 置信下界(堵 R2-F「采样估计冒充总体真值」)

R2-F:`<RuleCard>` 显示「precision 100%」但只抽审过 5 次 = 灾难性虚假信心。

**铁律:所有复利度量是「带不确定性的估计」,不是标量。** UI 与决策引擎都用**置信下界**:

```ts
interface CompoundMetric {
  pointEstimate: number;    // 点估计(仅展示用,永不单独驱动决策)
  sampleSize: number;       // ★ 强制并列(n 太小则下界自动很宽 → 决策保守)
  lowerBound95: number;     // ★ 95% 置信下界(driver):promote/毕业用它
  upperBound95: number;     // ★ 上界:坏信号(回滚率)用它(保守)
}
// 不变式 INV-METRIC-1:promote/demote 决策只读 lowerBound/upperBound,绝不读 pointEstimate
//   → 5 个样本的 100% 点估计,其 lowerBound95 极低 → 自动卡在 shadow,不毕业
// 不变式 INV-METRIC-2:<RuleCard> 渲染 precision 时,sampleSize 与 lowerBound 同屏,不可只显点估计
```

> **抗R3 ⑭(置信区间假设独立同分布,规则放行的案例可能相关)**:承认样本相关性会让朴素 CI 偏窄。缓解:抽审采样**跨时间/跨实例分层**(对齐 §5.5 self_trade 的分层抽审思路),且阈值留保守余量。这是「比裸点估计严格得多」的改进,不声称统计完美——诚实标注为「保守下界估计」。

### 3.3 decision leverage / savedHumanDecisions 作为反事实估计(禁止单独当北极星)

R2-F:`savedHumanDecisions` 是**反事实**(没这条规则人本会介入多少次),不可直接观测,却被当可累加硬 KPI——而 §5 刚痛斥「把可刷数字供成北极星」。

**解(对齐 DESIGN §4.6③)**:

```ts
interface DecisionLeverage {
  estimate: CompoundMetric;        // ★ 反事实估计,带 CI(不是硬计数)
  counterfactualAssumptions: string[];  // ★ 明示假设(如:被自动解决的案例「若无规则」100% 会进人工)
  pairedGuardrails: {              // ★ 禁止单独展示,强制成对(套用 §5 吞吐强绑护栏铁律)
    falseAutonomyRate: CompoundMetric;   // 该规则放行中「实际应人工」的漏报
    downstreamRollback: CompoundMetric;  // 下游回滚
  };
}
// 不变式 INV-NORTHSTAR-1:savedHumanDecisions / decisionLeverage 永不单独渲染为北极星
//   必须与 falseAutonomyRate + downstreamRollback 三联展示
//   → 「省下 1000 次人工」但「漏报率 15%」= 这是刷分,不是复利
```

> **抗R3 ⑮(反事实假设「若无规则 100% 进人工」高估了 leverage)**:对,所以假设**明示且可调**——更诚实的反事实是「若无规则,这些案例中 `q%` 会进人工(q 来自 shadow 期实测的人工介入率)」。leverage 用 `q` 折算,且取 CI 下界。绝不用「100% 都会进人工」的乐观默认。这把 R2-F 的「估计冒充计数」改为「带假设、带 CI、带配对护栏的诚实估计」。

### 3.4 坏规则检测 + auto-demote / 回滚触发(含 blast-radius 回溯,堵 R2-D.2)

```ts
interface DemoteTriggers {
  // 任一触发 → auto-demote(active → shadow)或 rollback(active → rolled_back)
  shadowAgreementDrop: boolean;        // 持续抽审一致率下界跌破阈值
  downstreamRollbackBreach: boolean;   // ★ 下游回滚率上界破阈(主触发,对系统性偏差有效)
  conflictIntroduced: boolean;         // 新规则使本规则进入 effectClash
  deadRule: boolean;                   // 长期 0 命中(回收,非降级)
}
// blast-radius 回溯(堵 R2-D.2「demote 只止血,没处理已放出的坏合入」):
// downstreamRollbackBreach 触发时,不仅 demote,还要:
//   1. 列出该规则 active 期间自动放行的全部历史案例(coverage 计数对应的案例)
//   2. 强制进复审抽样池(高比例 / 高危全审)
//   3. 生成 blast-radius 报告 → routeTo owner + PM/QA
interface BlastRadiusReport { ruleId: string; autoApprovedCases: string[]; suspectWindow: [string,string]; }
```

> **抗R3 ⑯(回溯几千历史案例 = O(n) 人工爆发)**:回溯是**坏规则触发时的一次性急救**,不是常态流;且回溯案例同样走 VaR 排序 + 抽样(不是全量逐个看,是按风险抽)。坏规则破阈本身应是稀有事件(被 §3.1 双真值源前置拦截过),回溯的 O(n) 是「事故响应」而非「稳态负载」,不计入 C-a/C-b 的稳态复杂度。这是「宁可事故时爆发回溯,不可让坏合入静默存活」的取舍,安全侧。

---

## 4. 解 R-6 / R-7 硬数学(distinct 决策类饱和 + 转换期回正)

### 4.1 R-6:distinct 决策类是否饱和 —— 可埋点实测的增长曲线 + 证伪条件

R-6 的本质:「distinct 决策类」在开放域可能持续发散,复利机就是换名 O(n)。本文不假装「一定饱和」,而是给**可实测的判别指标 + 明确证伪条件**,把领域赌注变成可监控的命题:

**埋点指标(进度量看板,与复利曲线并排)**:

```ts
interface DistinctClassGrowth {
  // 核心:每千 loop 的「新增 distinct 决策类」数,随时间的曲线
  newDistinctClassesPerKLoop: TimeSeries;   // ★ 主判别曲线
  // 拆两层(§1.4 的关键:结构层 vs 全决策层分开看):
  newStructuralClassesPerKLoop: TimeSeries; // 结构性(gate/blueprint/model)新增率 ← 真正吃人的
  newAutoApproveClassesPerKLoop: TimeSeries;// auto_approve 长尾新增率 ← 零成本吸收
  ruleSetSizeOverTime: TimeSeries;          // 规则总数曲线(无界增长=退化信号)
}
```

**收敛的前提(明示)**:复利机次线性**当且仅当** `newStructuralClassesPerKLoop` **随时间衰减**(结构性决策类饱和)。长尾 `auto_approve` 类即便发散也无所谓——它们走 `auto_graduate` 零人评审。

**证伪条件(冷酷、二元)**:
- **证伪 1**:若 `newStructuralClassesPerKLoop` **不衰减**(fleet 翻倍,结构性新增率不降)→ 结构层发散 → 复利机在结构层 = O(n) → **赌注被证伪**,触发 §7 降级。
- **证伪 2**:若 `ruleSetSizeOverTime` 持续线性/超线性增长且覆盖率不升 → 规则在「制造而非消除」复杂度 → 退化信号。
- **证伪 3**:若 §2.7 的 `promotionReview` 流净增持续 >0 且不收敛 → 评审成稳态瓶颈 → R-6 成真。

> **抗R3 ⑰(凭什么信结构类会饱和?)**:不要求「信」,要求「测」。这三条曲线是**可埋点、可在 MVP 阶段就开始采集**的实证。本文的立场是:**把不可证伪的「数学上成立」断言,降级为可证伪的「结构类饱和假设 + 三条监控曲线 + 三个证伪条件」**。这正是 §0 把判决变成可证伪命题的兑现。R3 若攻「假设可能错」——对,假设可能错,但现在它**戴着仪表盘**,错了会先报警再崩盘,而非静默换名 O(n)。

### 4.2 R-7:转换期净增人工 —— 冷启动容量预算 + 「何时回正」判据

R2-C / R-7:shadow + canary 窗口里,规则消除人工 = 0(人还在判),新增人工 = 「决定 promote」+「盯 shadow」。转换期是**纯负投资**;§4.4 的复利曲线在此期只升不降,会误导决策者砍掉复利机。

**解 1:复利曲线必须把「在途投资」单独画出来(堵 R2-C.1)**:

```ts
interface CompoundCurve {
  grossSavedPerKLoop: TimeSeries;     // 已毕业规则省下的人工(正)
  inFlightInvestmentPerKLoop: TimeSeries; // ★ shadow/canary 在途投资(负):shadow 标注 + promote 决策 + 评审
  netHumanPerKLoop: TimeSeries;       // ★ 净 = gross saved − in-flight − 基线;这才是真实曲线
}
```

净曲线在冷启动期为负(投资 > 回报)是**预期且健康**的,不是失败信号——把它显式画出来,决策者才不会在 J 曲线谷底砍掉复利机。

**解 2:冷启动容量预算模型**:

```ts
interface ColdStartBudget {
  steadyStateHumanLoad: number;     // 稳态需人工量(§6 费米估算)
  transientPeakLoad: number;        // ★ 转换期峰值 = 稳态 escalation 判断 + shadow 标注 + promote 评审(三者叠加)
  capacityCeiling: number;          // 人力上限
  bridgingStrategy: string[];       // 对齐 §13 R-5:借作战室 Lead 池 / 临时提阈值 / 限派发
}
// 警戒:transientPeakLoad > capacityCeiling 的时段 = 冷启动缺口,必须用 bridgingStrategy 填,否则队列爆炸
```

**解 3:「何时回正」的两个交叉点判据(正面回答 R-7)**:

```
拐点 A(流量回正 · 决策杠杆 > 1):
  当 decisionLeverage.lowerBound > 1 → 平均每次人工决策消除 >1 次未来介入 → 复利开始净减负
  // 用下界(§3.2),保守确认,不被乐观估计骗

拐点 B(累计回正 · 投资回收):
  累计 grossSaved 曲线 与 累计 inFlightInvestment 曲线的交叉点
  → 此点后,复利机历史总账由负转正
  // 这是「该不该继续投」的财务判据;A 是「机制是否生效」的判据
```

> **抗R3 ⑱(决策杠杆是反事实估计(§3.3),用它定拐点不就建在沙子上?)**:所以拐点 A 用 `decisionLeverage.lowerBound`(保守下界)且要求 **> 1 而非 > 0**(留安全垫),并与拐点 B(基于**可观测的**累计 saved/invested,非反事实)**双重确认**。两个判据一个机制侧一个财务侧,都翻正才算回正。单靠反事实的杠杆不足以宣布胜利——这是对 R2-F 的延续防御。
> **抗R3 ⑲(长尾规则永远凑不够 shadow 样本 → 假性走平,R2-C.2)**:见 §5.3 的长尾专用路径(显式背书 + 加重抽审,绕开「凑样本」),并在 §4.1 的曲线判别里区分「健康走平(无新可固化类)」vs「触顶走平(剩的都是长尾)」——后者用 `newAutoApproveClassesPerKLoop` 是否仍 >0 但毕业率低来识别,触发长尾路径而非误报「退化成人肉运维」。

### 4.3 长尾决策类的专用毕业路径(堵 R2-C.2「长尾够不着」)

高频类靠样本量自动毕业;低频长尾类样本来得慢,可能永远卡 shadow。**把举证责任从「样本量」换成「人的显式背书 + 加重事后抽审」**:

```ts
interface LowSampleGraduation {
  path: 'explicit_endorsement';   // 区别于默认的 'sample_accumulation'
  endorsedBy: string;             // 人显式声明高置信(需 four_eyes,因绕过了样本闸)
  initialAuditRate: number;       // ★ 更高的事后抽审比例(补偿小样本)
  downstreamWatchWindow: string;  // 更长的下游监控窗
}
// 不变式 INV-LONGTAIL-1:explicit_endorsement 路径必须 four_eyes(不能单人背书直升)
//   且抽审比例显著高于 sample_accumulation 路径(举证责任转移,代价是更密的事后核查)
```

> **抗R3 ⑳(显式背书不就是把 R2-D 的人为偏差又请回来了?)**:是引入了人判断,但用「**加重下游抽审 + four_eyes**」对冲——长尾类绝对数量小,加重抽审的成本可吸收;且下游真值源(§3.1)仍在背后兜底偏差。这是「长尾够不着」与「人偏差」之间的工程取舍:宁可对小量长尾付更高抽审成本,也不让复利机对长尾彻底失效(长尾恰是人工成本大头)。

---

## 5. 解 §5.4 死锁 —— 选择性背压保留 shadow 样本下限(解 R2-I 正反馈死锁)

### 5.1 死锁结构(R2-I 揭示的正反馈)

```
队列净增 >0 → 建议「降派发速率」减流量 → shadow 样本来自 escalation 流量 → 样本被饿死
  → 规则更难毕业 → 更依赖人 → 队列更堵 → 更想降派发 → ……(绞杀)
```

DESIGN §4.6④ 已定方向(「背压须选择性,保留 shadow 样本下限」),本文给**可执行策略**。

### 5.2 选择性背压策略:按「学习价值」分级节流

背压时**不一刀切降派发**,而是按流量的**学习价值**差异化节流:

```ts
interface SelectiveBackpressure {
  // 流量按学习价值分三档,节流顺序 = 学习价值升序(先砍最没用的)
  tiers: {
    lowLearningValue: 'throttle_first';   // 已被成熟 active 规则覆盖的流量:节流它不损失学习(规则已毕业)
    nearGraduation: 'protect';            // ★ 喂「临近毕业」规则的 shadow 流量:最高保护,绝不砍
    coldStart: 'reserve_floor';           // 新规则的初始 shadow 流量:保留下限(shadowFloor)
  };
  shadowFloor: number;  // ★ 每条「临近毕业 / 冷启动」规则保证的最小 shadow 样本/单位时间
}
// 不变式 INV-BACKPRESSURE-1:
//   任何降派发动作,必须先保证所有 status∈{shadow,canary} 规则的样本供给 ≥ shadowFloor
//   只有「已被 active 规则覆盖的低学习价值流量」可被无下限节流
//   → 背压砍掉的是「学不到东西的流量」,保住的是「正在喂复利的流量」
```

### 5.3 优先级:优先放行「能喂临近毕业规则」的流量(对齐 §4.6④)

- 计算每条 shadow/canary 规则的「毕业距离」(还差多少样本/多少下游信号到达毕业阈值)。
- 背压期,**优先放行能为「毕业距离最近」规则贡献样本的 escalation**——让临近毕业的规则尽快毕业,从而尽快从源头降流入(这才是结构解)。
- 被节流的是「低学习价值」流量(已成熟覆盖的、或与任何在途规则无关的)。

### 5.4 背压是急救,复利是结构解 —— 二者不在同一动作对冲

明确定位(对齐 DESIGN §4.6④):
- **背压(降派发)= 短期急救**:止血,争取时间。作用在「低学习价值流量」。
- **复利(加速毕业)= 长期结构解**:从源头降流入。作用在「高学习价值流量」。
- 二者作用在**不相交的流量子集**上,故**不对冲** → 死锁解除。

> **抗R3 ㉑(如果连低学习价值流量都不够砍,shadowFloor 和总容量冲突怎么办?)**:此时是真·容量危机(escalation 流入 > 人力上限 + 复利尚未见效),回退到 §4.2 的 `bridgingStrategy`(借 Lead 池/临时提阈值)——这是冷启动缺口问题(R-5/R-7),不是死锁问题。死锁的解是「分级节流让背压与复利不互相饿死」;容量绝对不足是另一个问题,用容量预算解,不混为一谈。把两个问题分开,正是避免「越止血越固化不了」的关键。
> **抗R3 ㉒(shadowFloor 占用的流量会不会让坏规则也被持续喂养？)**:不会——shadowFloor 喂的是 shadow/canary 规则的**样本采集**,这些规则尚未 active(shadow 不执行、canary 小流量),坏规则在毕业前会被 §3.1 双真值源拦下。shadowFloor 保护的是「学习机会」,不是「放行权」。

---

## 6. R-6 费米估算 —— 复利机到底是 O(1) / O(log n) / 还是换名 O(n)?

### 6.1 稳态人工负载等式(兑现 R2-E「写出不等式」)

设(均为可埋点实测量):
- `N` = fleet 规模(loop 数)
- `e` = 单位 loop 的 escalation 率(/loop)
- `p_rule` = escalation 中「可固化为规则」的占比
- `p_struct` = 可固化规则中「落结构性路径(非 auto_graduate)」的占比 ← **命门**
- `c_review` = 单条结构性规则的人评审成本(人·时)
- `c_residual` = 未被任何规则覆盖的 escalation 的人工裁决成本(人·时/个)
- `r` = 规则平均覆盖率(一条规则消除的同类 escalation 占比)

**稳态人工负载**(三项相加):

```
人工总负载 ≈  [Ⅰ 残留裁决]            [Ⅱ 规则评审]                 [Ⅲ 治理]
            =  N·e·(1−p_rule·r)·c_residual
             + (结构性规则新增率)·c_review
             + 治理面成本(§2,机器为主,人工≈死规则确认/冲突仲裁,稀疏)
```

### 6.2 三项各自的规模阶

| 项 | 规模阶 | 论证 |
|---|---|---|
| **Ⅰ 残留裁决** | 随 N 线性,但**系数 `(1−p_rule·r)` 随复利推进而下降** | 复利成熟时 `p_rule·r → 高`,残留项被压到一个**远低于 N·e 的水平**。这是复利「弯折 O(n)」的真实含义:不是消灭线性项,是**把线性项的斜率压到接近平**。 |
| **Ⅱ 规则评审** | **取决于 p_struct 是否随 N 衰减** | 若结构类饱和(§4.1 假设)→ 结构性规则新增率随 N 衰减 → Ⅱ **趋于常数 O(1)**。若结构类发散 → Ⅱ ~ O(N) → **换名 O(n)**(证伪)。 |
| **Ⅲ 治理** | **O(桶数)机器 + 稀疏人工** ≈ O(log N) 或近常数 | rollup 桶数受 scope 维度基数约束(有界);人工只在死规则确认/effectClash 仲裁,稀疏。 |

### 6.3 数量级实例(带假设的费米,非精确)

取一组保守假设:`N=10,000`,`e=0.05`/loop·天(每天 500 escalation),`p_rule=0.4`,`r=0.8`,`c_residual=4 分钟`,结构性规则新增率冷启动后 `~5 条/天`,`c_review=2 人·时/条`。

- **Ⅰ 残留裁决** = 500 × (1 − 0.4×0.8) × 4 分钟 = 500 × 0.68 × 4 = **1,360 人·分/天 ≈ 22.7 人·时/天**
  (对比:无复利时 500 × 4 = 2,000 人·分 = 33.3 人·时/天 —— 复利把残留斜率压低 ~32%,且 `p_rule·r` 随时间继续升 → 斜率继续压平)
- **Ⅱ 规则评审** = 5 条/天 × 2 人·时 = **10 人·时/天**(且若结构类饱和,新增率随 N 涨而**每千 loop 递减** → 不随 N 发散)
- **Ⅲ 治理** = 稀疏,假设 **~2 人·时/天**(死规则确认 + 偶发仲裁)

**稳态 ≈ 34.7 人·时/天 ≈ 4–5 个全职**(管 10,000 loop)。关键:**N 从 1万 → 10万,Ⅰ 的系数继续被复利压低、Ⅱ 若饱和则不动、Ⅲ 走 log** → 人力**不随 N 线性翻 10 倍**。

### 6.4 结论:**O(N) 但斜率被复利持续压低,且其上叠加 O(log N) 治理**;在结构类饱和假设下趋近 **「斜率收敛的 O(N) + O(log N)」**,工程上表现为「几个人管几千→几万 loop」

诚实的复杂度判定(不吹 O(1)):

> **复利机不是 O(1)。** 残留裁决项(Ⅰ)对未覆盖流量天然是 O(N) ——只要还有 escalation 没被规则覆盖,人就得看,这是物理下限,任何机制消灭不了。
>
> **复利机也不是「换名 O(n)」**(反驳 R-6 的最坏论断)——**前提是结构类饱和假设成立**(§4.1 三条曲线实测)。其真实复杂度是:
>
> **`O(N)·(1−p_rule·r) + O(log N)`,其中复利的作用是让线性项的有效斜率 `(1−p_rule·r)·c_residual` 随时间单调下降、并把评审/治理项压到 O(log N) 量级。**
>
> 通俗结论:**复利机把「O(N) 的陡线性」弯折为「O(N) 的缓线性 + O(log N) 治理」**。它不消灭线性(那是 escalation 物理量决定的),但**持续压低线性的斜率**,使「人力随规模的增长」从「线性陡增」变成「近平的缓增 + 对数治理」——这就是「几个人管几千 agent 在机制上可能收敛」的**精确含义**。
>
> **决定性变量是 `p_struct`(结构类是否饱和)**:饱和 → 评审项 O(1)、整体是「斜率收敛的缓线性」(胜);发散 → 评审项 O(N)、整体退回「换名 O(n)」(败,触发 §7 降级)。这把 R-6 从「哲学争论」变成「一个可埋点实测的标量 `newStructuralClassesPerKLoop` 是否衰减」。

---

## 7. 退化时的明牌降级路径(若赌注被证伪,不藏)

若 §4.1 的证伪条件触发(结构类发散 / 规则集无界膨胀 / 评审流不收敛),复利机在该区域退化为 O(n)。**预定的降级动作**(不等崩盘):
1. **缩小自动化野心**:把发散区域的规则从 `auto_graduate` 收回到 `four_eyes`,接受该区域是人力密集区,用容量预算硬扛。
2. **提高聚类粒度**:发散常因 scope 切得太细(规则碎片化),回 §2.6 反碎片化,用更粗的 scope 换更少的 distinct 类(牺牲精度换可治理性)。
3. **诚实对外**:在任何材料里把该区域标为「复利未收敛区」,绝不宣称「数学上成立」——对齐 §13 R-6 的诚实措辞。

---

## 8. 逐条对账:本文解掉的蓝军 R2 洞 + DESIGN R-6~R-8

| 蓝军编号 | 洞 | 本文解 | 闭合条件 |
|---|---|---|---|
| **R2-A** | Promotion 无评审主体 → O(n) 搬家 | §1 `kind×VaR` 分流矩阵;人评审只压结构性稀疏集;§6 费米证 Ⅱ 项不发散(饱和假设下) | C-a |
| **R2-B** | 规则冲突/爆炸治理真空 | §2 precedence/specificity/priority + fail-safe 不变式 + 强制冲突检测 + rollup 治理面 + 规则集快照 | C-b |
| **R2-C** | shadow 负投资 + 长尾毕不了业 | §4.2 净曲线单独画在途投资 + 拐点判据;§4.3 长尾专用路径(显式背书+加重抽审) | — |
| **R2-D** | 系统性偏差被复利放大 | §3.1 下游回滚率(人之外)作主真值,穿透人集体盲区;§3.4 blast-radius 回溯 | C-c |
| **R2-E** | 「数学上成立」只断言不证明 | §6 写出稳态负载等式 + 费米数量级 + 复杂度判定;降级为「机制上可能收敛」 | — |
| **R2-F** | precision/savedHumanDecisions 字段≠真值 | §3.2 强制带样本量+置信下界(下界驱动决策);§3.3 leverage 标注反事实+禁单独北极星 | C-c |
| **R2-G** | §13「缓解」记成「现状」自指循环 | 本文用冷酷二元判定(机制设计完成/未完成),不用「缓解」糊弄;§9 列真实残留 | — |
| **R2-H** | §13 漏 P0(评审+冲突) | §1+§2 正面给出机制;建议 §13 把 R-6/R-7/R-8 措辞改为「机制设计完成,待运营验证」 | — |
| **R2-I** | 三新机制二阶失效 + 限流vs复利死锁 | §2.7 多队列建模;§5 选择性背压(分级节流,背压与复利不对冲) | — |
| **DESIGN R-6** | 复利机=O(n) 改名? | §6 费米结论 + §4.1 可证伪曲线 + 证伪条件 | C-a |
| **DESIGN R-7** | 转换期净增人工 | §4.2 冷启动容量预算 + 拐点 A(杠杆>1)/拐点 B(累计交叉) | — |
| **DESIGN R-8** | 真值/冲突/评审三件套 | §3/§2/§1 分别给完整机制 | C-a/b/c |

---

## 9. 我自己仍担心的洞(留给 R3,不藏 —— 反 R2-G 的自指循环)

诚实声明:以下是本文**机制已设计、但仍依赖未验证假设或运营校准**的点,明牌列出,不用「缓解」糊弄:

1. **`p_struct` 有界性是领域赌注(虽比原赌注弱)**。本文把它变成可监控的 `newStructuralClassesPerKLoop`,但「结构类一定饱和」未被证明。**这是 R3 最可能攻、也是我最担心的洞**——见摘要。
2. **下游真值源有滞后**。14 天回滚窗意味着坏规则在窗口内已放行;§3.4 blast-radius 回溯是事后补救,不是事前拦截。canary 小流量缓冲了 blast-radius,但未消除滞后本身。
3. **VaR 档阈值 / 精度阈值 / shadowFloor / n_min 全是待标定参数**(同 §13 R-1)。机制就位,参数运营校准;参数定错会让分流矩阵失真(过松 → 该评审的自动毕业了;过紧 → 都堆给人)。
4. **置信区间假设的样本独立性**在规则放行场景可能不成立(§3.2 抗R3⑭),朴素 CI 可能偏窄。已用分层抽样 + 保守余量缓解,未做严格统计建模。
5. **反事实假设的可操纵性**:`decisionLeverage` 的反事实折算率 `q` 若被乐观设定,仍能虚高(虽已用下界 + 配对护栏 + 双拐点对冲)。
6. **冷启动 J 曲线谷底的组织风险**:§4.2 容量预算和净曲线在技术上成立,但「决策者在投资回收点(拐点 B)之前是否有耐心不砍掉复利机」是组织/产品问题,非本文技术机制能完全保证。

---

## 附:本文新增 / 扩展的领域模型一览(供实现对齐,严格基于 DESIGN/pm-spec 既有实体)

| 实体/接口 | 性质 | 关系 |
|---|---|---|
| `Rule`(扩) | 扩 `effect`/`scope`/`specificity`/`priority`/`effectiveFrom`/`supersedes` | DESIGN §10 既有 Rule 增字段,`scopeKey` 保留为 `scope` 序列化 |
| `RuleScope` | 新 | = approve-all-similar 四元 groupKey 的结构化(零概念新增) |
| `Promotion`(扩) | 挂 `PromotionGovernance` | DESIGN §3 既有 Promotion 增治理字段 |
| `PromotionGovernance` | 新 | `GovernanceTier` 由 `kind×VaR` 引擎计算 |
| `RuleTruthSignals` | 新 | 复用 DESIGN §5.1 已定义的下游护栏(回滚/reopen/defect) |
| `CompoundMetric` | 新 | 所有复利度量的统一带 CI 形状 |
| `DecisionLeverage` | 新 | 反事实估计 + 配对护栏,禁单独北极星 |
| `ConflictReport` / `RuleSetRollup` / `RuleSetSnapshot` | 新 | 治理面,套用 §2 fleet rollup 范式 |
| `QueueFlows`(扩) | 扩 DESIGN `<QueueFlowHeader>` 数据契约 | escalation + promotionReview + shadowSampling 三流 |
| `SelectiveBackpressure` | 新 | §5.4 死锁解的数据契约 |
| `DistinctClassGrowth` / `CompoundCurve` / `ColdStartBudget` | 新 | R-6/R-7 的可埋点实测指标 |
| 一组 `invariant`(INV-*) | 新 | fail-safe / 职责分离 / 下界驱动 / 背压下限等引擎层强制不变式 |

> **组件层**(对齐 DESIGN §10,新增数据契约,不新增视觉范式):`<PromotionReviewQueue>`(分 tier)、`<ConflictArbitrationCard>`、`<RuleSetRollup>`(套 FleetRollup)、`<RuleCard>`(扩:强制显示 sampleSize+lowerBound)、`<CompoundCurve>`(扩:画在途投资)、`<QueueFlowHeader>`(扩:三流)。
