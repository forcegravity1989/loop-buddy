# 21 · 指标渲染 skill 选型测评(拿来主义 · 第一轮)

> 2026-08-05。起因(用户原话):「指标找寻的效果还可以;……指标可视化似乎也需要一个**有效可靠的渲染 skill**」。
> 按 `plan/19` 立下的拿来主义方法办:先全渠道搜、亲读原文核验,再决定引入 / 改编 / 自建——不先动手造。
>
> **本文是第一轮(搜索 + 内容维度核验)的结论。缺一半:`plan/19` 的另一半是同模盲测(独立评委子代理各自打分),本轮没有跑** —— 执行环境不起子代理,自己产出自己评不算数,如实标注为**未测**,不拿内容分冒充盲测分。

---

## 0. 结论先行

| 类别 | 业界最佳 | 能不能直接装 | 处置 |
|---|---|---|---|
| **图表语法**(选什么图、怎么配色) | **anthropics/knowledge-work-plugins · `data-visualization`**(Apache-2.0,23,309★,10,830 装) | 能装,但**输出形态不对**——它产出的是 Python/matplotlib 画的 PNG,BW 要的是数字能被 `sqlite3` 独立复核的页面 | **改编合入**:选型表 + 「什么图别用」 + 色盲调色板进 references |
| **自包含 HTML 看板** | **anthropics/knowledge-work-plugins · `build-dashboard`**(Apache-2.0,7,221 装) | **不能原样装**——它的工作流明写「没有数据就造一份真实感样例数据」,与 BW 铁律正面冲突(见 §3) | **只借形态**(单文件自包含 / KPI 卡布局),**明确反转它的造数行为** |
| **KPI 设计建议** | wshobson · `kpi-dashboard-design`(MIT,38,502★,**12,299 装 = 全场装机量第一**) | **不对题**——它是「找指标」侧的东西,不是渲染器 | 不引入(§4) |
| **远端图表 API** | antvis · `chart-visualization`(MIT,454★,5,511 装) | **三重不合**(§5) | 不引入 |

**总结论**:**这个生态位是空的**。有成熟的图表语法,有成熟的自包含 HTML 看板生成器,**但没有任何一件带「真实性护栏」的渲染 skill**——无数据 = Unknown 不假绿、手填戴徽、每个数字可独立读回、绿色隐身只有红黄出声。最接近 BW 需求的那一件(`build-dashboard`)在这一点上恰恰**反向**。

这与 `plan/19` 的结论同构:**改编合入 > 原样引入 > 推倒自造**。

---

## 1. 搜索覆盖面(如实)

| 渠道 | 做了什么 | 结果 |
|---|---|---|
| **A · skills.sh 全量扫** | 公开搜索 API `skills.sh/api/search?q=`,14 个功能关键词(dashboard / data visualization / chart / metrics dashboard / kpi dashboard / html report / visualization / report rendering / sparkline / d3 / observability / grafana / 报表 / 可视化),按 `installs` 排序去重 | 高装榜逐个功能核验。**装机量前 3 全部不对题**:`azure-observability`(98,266)、`google-agents-cli-observability`(72,964)、`golang-observability`(34,231)——都是**运维可观测性**,不是指标渲染。真正对题的最高装机是 `kpi-dashboard-design`(12,299),而它是设计建议不是渲染器(§4) |
| **B · GitHub 内容级** | `gh api search/code` 按仓 + `filename:SKILL.md` 定位候选路径;`gh api repos/…` 亲自读回 stars / license / pushed | 见 §2 表格,数字全部当场读回,不引二手 |
| **C · 官方市场 / 聚合库** | `anthropics/knowledge-work-plugins`(官方,Apache-2.0)逐目录列举:`data/skills/` 下 10 件,其中 `data-visualization` / `build-dashboard` / `create-viz` 三件对题 | 官方仓这次**有货**(与 plan/19 北极星那轮「官方仓无任何指标类 skill」相反) |
| **D · 本机已装** | 本会话可用的 Anthropic `dataviz` skill(带可运行的配色校验器 + `references/palette.md`) | craft 最高的一件,但同样是**图表**层面,不含指标看板语义 |

**缺口(如实)**:①**未跑盲测**(见文首);② 中文渠道只扫了 skills.sh 的中文关键词,没做中文社区口碑面;③ `create-viz`、`claude-office-skills/chart-designer`、`report-generator` 只读了描述,未逐字亲读原文。

---

## 2. 候选全景(数字均为 2026-08-05 `gh api` 当场读回)

| Skill | 仓库 | Stars | 安装 | License | 形态 |
|---|---|---|---|---|---|
| `data-visualization` | anthropics/knowledge-work-plugins | 23,309 | 10,830 | Apache-2.0 | 选型表 + Python 代码样板 → PNG |
| `build-dashboard` | anthropics/knowledge-work-plugins | 23,309 | 7,221 | Apache-2.0 | 自包含交互式 HTML 看板 |
| `kpi-dashboard-design` | wshobson/agents | 38,502 | 12,299 | MIT | KPI 选型与分层的**设计建议** |
| `chart-visualization` | antvis/chart-visualization-skills | 454 | 5,511 | MIT | curl 调远端 API 换图片 URL |
| `dashboard-builder` | affaan-m/ECC | 237,806 | 4,436 | MIT | 未逐字读(下一轮) |
| `chart-designer` / `report-generator` | claude-office-skills/skills | 357 | 4,105 / 4,115 | MIT | 未逐字读(下一轮) |
| `charting` / `chart` | Starchild-ai-agent/official-skills | 21 | 4,424 / 3,767 | **NO-LICENSE** | **授权洁癖:绕开** |

---

## 3. 决定性发现:`build-dashboard` 会自己造数据

形态上离 BW 最近的一件,原文工作流第 2 步「Gather the Data」写着:

> **If working from a description without data:**
> 1. Create a realistic sample dataset matching the described schema
> 2. Note in the dashboard that it uses sample data

**这与 BW 的核心铁律正面冲突。** `CLAUDE.md` 与 `plan/06` 反复钉死的是:

> **无数据 = Unknown ≠ 绿**;观测只追加、信号只派生;**永远不替用户捏造健康**。

「造一份真实感样例数据 + 注明是样例」在通用场景下是体面的做法(它确实标注了),但在 BW 的场景下是**最危险的一种行为**:一块本该整片灰的看板,会因为「有数据总比空着好看」而被填满看似合理的数字。本次三个模块的真实产出恰恰是全灰的——16 条指标零观测——**而那个灰正是当前最重要的信息**(aihot 的北极星是 0,是「没数据」的 0,不是「产品不好」的 0)。

**处置**:借它的形态(单文件自包含、无服务器、KPI 卡布局、筛选器),**明确反转它的造数条款**——渲染器遇到零观测必须渲染 Unknown 灰,并且**渲染不出来的东西不许补**。

---

## 4. 装机量第一名不对题(采用度≠对题质量,第三次)

`kpi-dashboard-design` 是本轮装机量冠军(12,299),宿主仓 38,502★。亲读原文:它讲的是 KPI 分层(Strategic / Tactical / Operational)、SMART、「限制在 5-7 条」、展示上下文——**这些是「找指标」的活,BW 已经由 `north-star-discovery` + mohit 两件覆盖**,而且覆盖得更严(它没有反指标、没有强制判定题、没有质量门)。

它的 Do's 里还有一条与 BW 直接相反:

> **Use consistent colors — Red=bad, green=good**

BW 的设计系统是「**绿色隐身,只有红黄出声**」(`plan/00` §6):健康概览只浮出进行中且 `signal≠green` 的项。把绿当成要庆祝的颜色,会让一块健康的看板变得吵闹,正好丢掉这条规则要买的东西。

**不引入。** 这是 `plan/19` §5-1 那条发现的第三个实例:安装量高只说明它解决了一个**普遍**问题,不说明它解决**我们这个**问题。

---

## 5. `chart-visualization` 的三重不合

antvis 是严肃的可视化团队(Ant Group),skill 本身写得干净。但它的实现是 `curl -X POST https://antv-studio.alipay.com/api/gpt-vis` 换回一个图片 URL:

1. **数据出境**:把项目的真实指标数据 POST 给第三方服务。
2. **CSP 挡死**:BW 的产出页面(artifact / 桌面 WebView)禁外链资源,拿回来的图片 URL 渲染不出来。
3. **不可复核**:一张远端生成的图片,数字无法 `sqlite3` 独立查证——直接违反「读回为证」。

**不引入。**

---

## 6. 建议:自建一件薄的 `metrics-render`,改编合入两件强项

生态没有现成货能直接装,但**方法论可以拿**。建议自建一件薄 skill,它的价值不在画图技巧(那部分整段借),而在**把 BW 的真实性护栏做成渲染的硬约束**:

**从 `data-visualization`(Apache-2.0,可原样再分发)改编合入**
- 按数据关系选图的选型表(趋势→折线 / 排名→横向条 / 达标对比→bullet chart / 多 KPI→small multiples)
- 「什么图别用」:3D 从不、饼图 <6 类才考虑、双轴慎用
- 色盲友好调色板

**从 `build-dashboard`(Apache-2.0)只借形态,反转其造数条款**
- 单文件自包含、无服务器、浏览器直开
- KPI 卡 + 趋势 + 表格的三段布局
- **反转**:零观测 → Unknown 灰,不造样例数据、不补插值

**BW 自己的硬约束(这才是这件 skill 存在的理由,生态里没有)**
1. **无数据 = Unknown 灰**,绝不假绿;`observation` 为空的指标必须显式渲染成「无数据」,不许用 0、不许用上一次的值、不许插值。
2. **来源徽记**:`collect_kind` 逐条显示;`manual` 戴「手填」徽,`bw`/`connector` 未接的显式标「v1 未接」。
3. **数字可独立复核**:页面上每个数字旁边(或页脚)给出能跑出这个数的 `sqlite3` 命令——渲染器的输出自带自证方式。
4. **绿色隐身**:green 不高亮、不庆祝;只有 amber/red/Unknown 出声。
5. **观测稀疏是常态**:1 个点不画趋势线(画了就是插值),明确渲染成单点 + 「仅 1 个观测点」。
6. **数据来源只有两个**:`<workspace>/.bw/metrics.toml`(定义正本)+ `observation` 表(值)。**不接受手输数字进渲染器**。

**输入契约**建议直接就是本轮三个模块已经产出的东西:`.bw/metrics.toml` + `metric`/`observation` 两张表——不新造中间格式。

---

## 7. 下一轮该补什么(按优先级)

1. **盲测那一半**(本轮欠账):起独立评委,用埋陷阱的场景测——最关键的陷阱就是 §3 那条:**给一个零观测的项目**,看候选会不会造数据、会不会把空当绿。这条陷阱能一次性把 `build-dashboard` 类的候选筛出来。
2. 逐字亲读剩下 3 件(`create-viz`、`chart-designer`、`report-generator`、ECC `dashboard-builder`)。
3. 中文社区口碑面(本轮只扫了 skills.sh 的中文关键词)。
4. 本机 `dataviz` skill 的原文核验——它带可运行的配色校验器,是 craft 最高的一件,值得单独评估能否整段借用。
