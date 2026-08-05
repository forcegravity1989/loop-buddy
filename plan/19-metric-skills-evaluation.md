# 19 · 北极星 / 引领滞后指标 skill 业界选型测评(拿来主义)

> 2026-08-04。目标(用户原话):最重要的两个 skill——「找到北极星指标」「找到引领性指标和滞后性指标」——业界可能已存在,**不要自己创建,找评分很高的现成 skill,拿来主义,做测评**。
> 本文是测评结果正本。一手证据(调研报告、盲测场景、两轮 22 份盲产原文、16 份盲评裁决 JSON、匿名映射、聚合 summary)归档于本机 `verification/skill-eval-2026-08-04/`——**拍板(2026-08-05):证据包不入仓**,该路径已进 `.gitignore`,只存本机、不随仓分发;§7/§8.4 的读回命令在持有本机存档的机器上照跑。

---

## 0. 结论先行

| 类别 | 业界最佳(盲测) | 采用度最佳 | 拿来主义建议 |
|---|---|---|---|
| **北极星指标** | **amplitude/builder-skills · `north-star-metric`**(Amplitude 官方,北极星方法论原创方;盲测 **9.0/10 四票全 9** 全场第一) | phuryn/pm-skills · `north-star-metric`(宿主仓 24,829 stars、skills.sh 1.9K 安装)——**但盲测垫底 6.75,低于无技能对照 7.25** | Amplitude 仓**无 LICENSE**:①走 plan/16 T11 改编路线,把其 6 判据打分/坏候选对照/BDFE 输入结构/系统校验合入自建 `north-star-discovery`(SelfBuilt+「改编自」留痕);②并行向上游发 issue 请求补 license,补齐后可原样导入。**不引入 phuryn**(见 §5 关键发现 1) |
| **引领/滞后指标** | **mohitagw15856/pm-claude-skills · `metrics-framework` + `metric-tree-builder`**(盲测 **8.75/10** 第一;kouko/monkey-skills · `4dx-d2-lead-measures` 8.25 紧随) | phuryn 同上(其 input metrics 即引领指标)——盲测 6.0,倒数第二 | **P0 直接引入 mohit 姊妹两件**(MIT,1,252 stars,活跃):`ImportSkillPackage`,`official_library="mohitagw15856/pm-claude-skills"`,原文保留;**P1 合入 kouko 的 standards 三件套**(MIT:两轴打分/五类伪 lead 排除/Goodhart 自检)作方法论增强 |

**自建基线站位**(如实):自建 `north-star-discovery` 在北极星侧盲测 7.75(第二,高于无技能对照),站得住;在引领/滞后侧 6.25(防作弊 3.0/5、可影响性 3.5/5),**是真短板,正该拿来补**。

---

## 1. 候选全景(调研)

方法:`gh search code --filename SKILL.md` 内容级搜索 + `gh api` 读回 stars/license + raw 原文逐篇阅读 + skills.sh 安装量 + 官方仓/awesome 合集核查。全文见 `verification/skill-eval-2026-08-04/research-north-star-skill.md` 与 `research-lead-lag-skill.md`(每条结论带 URL)。

### 北极星类(合格候选 6,取前 3 进盲测)

| Skill | 仓库 | Stars | 安装 | License | 特征 |
|---|---|---|---|---|---|
| `north-star-metric` | amplitude/builder-skills | 140 | 4 | **无** | 方法论原创方操作化:6 步流程、故意的坏候选对照、6 判据打分(含**难造假+反指标**)、Breadth/Depth/Frequency/Efficiency 输入结构、NSM↔商业 KPI 系统校验 |
| `north-star-metric` | phuryn/pm-skills | 24,829 | 1.9K | MIT | 三游戏分类+7 判据+3-5 input metrics,74 行简化版 |
| `north-star-metrics` | RefoundAI/lenny-skills | 1,214 | 106 | MIT | Lenny's Podcast 14 位嘉宾 39 条洞见语料+护栏反指标,教练式 |

### 引领/滞后类(合格候选 6,取前 2 进盲测;phuryn/自建覆盖三层,同产出复用进本组评审)

| Skill | 仓库 | Stars | 安装 | License | 特征 |
|---|---|---|---|---|---|
| `metrics-framework`+`metric-tree-builder` | mohitagw15856/pm-claude-skills | 1,252 | 50 | MIT | 指标树逐条强制 Leading/Lagging 判定+成对反指标+0-40 质量门(明令惩罚「全标 leading 的不诚实」) |
| `4dx-d2-lead-measures` | kouko/monkey-skills | 5 | 未上架 | MIT | 4DX 正典:predictive×influenceable 双轴 1-5 打分(任一≤3 出局)、五类伪 lead 排除、Goodhart 自检,protocols/standards/worksheets 全套包 |

**生态空白(如实)**:anthropics/skills 官方仓与两大 awesome 合集(71.7k / 14.5k stars)均无任何指标类 skill;「高采用 × 专门 lead/lag 推导」的现成货**不存在**——按调研原文:「用 4DX 两判据推导指标」这个生态位实质空着,BW 引入并做好反而是差异化机会。授权风险:lyndonkl/claude `metrics-tree`(138 stars)流程完整但**无 license**,绕开。

---

## 2. 测评方法

- **双轨**:内容维度(方法论/防线/可操作/BW 适配)基于**原文亲读核验**打分;产出维度用**同模盲测**。
- **盲测设计**:7 种配置(3 北极星候选 + 2 引领滞后候选 + 自建 `docs/skills/north-star-discovery` 基线 + 无技能对照)× 2 个埋陷阱场景 = 14 份产出。场景 S1「DailyBrief」埋虚荣指标陷阱(cron 可刷绿的「连续产出」、痛点在上限却守下限);S2「TraceLens」埋既有体系陷阱(该对齐 adoption_rate/L1/L2 不该另起炉灶、脚本自采不该降级手填)。
- **盲评**:4 个评审组(北极星组/引领滞后组 × 2 场景),每组 5 份样本匿名为 A-E(映射封存 `bench/mapping.json`,两场景乱序防位置偏置),每组 2 名独立评委按 6 判据(1-5)+ overall(1-10)+ 排名裁决,共 8 份裁决 JSON。
- **诚实注记(局限)**:①产出与评审统一用 Sonnet 5,隔离模型差异;但首波额度事故留下 2 份 Fable 产出(bw-baseline-s2、mohit-s2)混入,如实标注——两者在各自组内一高一低,未见系统性抬分;②每组仅 2 评委 2 场景(n=4 票),分差 <0.5 不足断言;本文只对 ≥1.0 的分差下结论;③评委间存在真实分歧(如 phuryn 产出引用「TLDR 完读率 30-50% 行业区间」,一位评委记为编造简报外事实扣分、另一位记为有据校准加分),裁决原文都在,不裁平。

---

## 3. 盲测结果(4 票 = 2 场景 × 2 评委,満分 10)

### 北极星组

| 配置 | overall 均分 | 四票 | 判据均分(1-5):落位/自动化免疫/场景契合/定义精确/采集诚实/否决质量 |
|---|---|---|---|
| **amplitude** | **9.00** | 9,9,9,9 | 5 / 4.75 / 4.5 / 4.5 / 5 / 5 |
| 自建基线 | 7.75 | 7,7,9,8 | 4.5 / 4.75 / 4.5 / 4.5 / 5 / 5 |
| 无技能对照 | 7.25 | 5,8,8,8 | 5 / 4.25 / 4.25 / 4.75 / 4.5 / 4.5 |
| refound | 7.00 | 8,7,7,6 | 3.75 / 4 / 4.5 / 4.5 / 5 / 4.75 |
| phuryn | 6.75 | 6,8,6,7 | 5 / 4.25 / 4 / 4.25 / 4.5 / 4.25 |

评委证据(摘,全文在 `bench/verdicts/`):
- **amplitude 赢在哪**:「把『太长看不完』直接编码进北极星,还设满意度净值做交叉哨兵防『注水拖慢』式造假」(S1);「唯一给出正式抗博弈对照表…明确点出『挑简单案子刷分』的漏洞并用报告覆盖率对冲,末尾如实承认因果链尚未用真实数据验证,全场自省最深」(S2)。
- **amplitude 唯一代价**:「把既有 north_star 字段整体降级重命名,对齐让位于严谨」(S2)——恰是自建基线的强项:「唯一保留团队原生 adoption_rate 身份不改名(换名等于历史观测跟丢),最贴合『对齐而非另起炉灶』」。两者短板互为长板,支撑 §0 的改编合入路线。
- **自建基线扣分点**:「耗费大量篇幅引用不存在的 skill 文件与 .bw/metrics.toml 内部 schema,稀释了对交付本身的聚焦」(S1)——BW 语境外的噪音,改编时无碍(真实使用恰在 BW 语境内)。

### 引领/滞后组

| 配置 | overall 均分 | 四票 | 判据均分(1-5):可影响/预测性/阈值校准/分类诚实/防作弊/采集诚实(S2=体系对齐) |
|---|---|---|---|
| **mohit** | **8.75** | 9,9,9,8 | 4.5 / 5 / 5 / 4.75 / 5 / 4.75 |
| kouko | 8.25 | 8,8,8,9 | 5 / 5 / 4.25 / 5 / 5 / 4.5 |
| 自建基线 | 6.25 | 7,7,6,5 | 3.5 / 3.5 / 4.25 / 4 / **3** / 5 |
| phuryn | 6.00 | 6,6,5,7 | 3.75 / 4 / 3.5 / 4 / 3 / 4 |
| 无技能对照 | 5.75 | 5,5,7,6 | 3.75 / 3.75 / 3.5 / 3.75 / 2.5 / 4.25 |

评委证据(摘):
- **mohit 赢在哪**:「北极星到引领是严格乘法恒等式(有效定界率=报告覆盖率×报告采纳率×定界正确率)…『防作弊速查表』把每条指标的『刷法』与『被谁抓住』逐一对应」(S2);「滞后阈值基于 58/60 真实数据收紧,唯一把『定时任务送达率』放进滞后而非引领,北极星故意不给 target 并注明『不替他拍板一个没有基线支撑的数字』」(S1)。
- **kouko 赢在哪/输在哪**:「两轴 PASS/FAIL 审计表+Goodhart 自检+频率/质量配对,给出可证伪数值预测…L2 因『无法一句话讲通因果』被明确证伪剔除」;输在教练式装置略游离简报事实、S1 两条拉新滞后目标无基线支撑。
- **自建基线短板实锤**:「把 leading.L2 原始计数直接留作引领指标,与其余四份样本各自独立否决『L2 当引领』的意见相悖;四条阈值全部贴着现状设,不构成驱动改善的目标」;「反刷意识只停留在北极星层面,没有下沉给引领指标配独立质量护栏」。

---

## 4. 五维度综合(内容维度=原文核验主观分,盲测=上表换算)

权重:方法论 30 / 防伪防线 20 / 可操作性 20 / 盲测产出 20 / BW 适配 10。采用度如实报告不计分(细分 skill 普遍很新)。

**北极星类**:

| | 方法论 | 防线 | 可操作 | 盲测 | BW适配 | 总分 |
|---|---|---|---|---|---|---|
| amplitude | 28 | 18 | 17 | 18.0 | 6(无 license、无采集方案概念) | **87.0** |
| 自建基线(对照) | 25 | 19(唯一有自动化免疫上位原则) | 18(DoD+完整样例) | 15.5 | 10 | 87.5 |
| refound | 24 | 15 | 14(语料库形态,流程弱) | 14.0 | 8 | 75.0 |
| phuryn | 22 | 13 | 15 | 13.5 | 8 | 71.5 |

北极星侧结论:**业界最佳=Amplitude 版**——综合分与自建基线打平(87.0 vs 87.5,差距在 BW 适配这个自建天然满分的维度),但**纯产出质量上明确胜出**(9.00 vs 7.75)。二者防线互补(它有反指标/坏候选对照,自建有自动化免疫检验/体系对齐纪律),所以是「改编合入」而不是「替换」。

**引领/滞后类**:

| | 方法论 | 防线 | 可操作 | 盲测 | BW适配 | 总分 |
|---|---|---|---|---|---|---|
| **mohit** | 26 | 18 | 18(生成器形态+0-40 自评门+模板) | 17.5 | 8 | **87.5** |
| kouko | 29(唯一四项全有:可影响/预测/滞后验证/防 Goodhart) | 20 | 14(教练式多轮,自动化嵌入需改造;非 4DX 语境主动让路) | 16.5 | 6 | 85.5 |
| 自建基线(对照) | 20(引领/滞后仅两步,无判据打分) | 14(无反指标/Goodhart) | 18 | 12.5 | 10 | 74.5 |
| phuryn | 16(滞后侧缺席) | 10 | 15 | 12.0 | 8 | 61.0 |

引领/滞后侧结论:**业界最佳=mohit 姊妹两件**,综合与盲测双第一,MIT 可直接装;kouko 方法论最深,作 standards 增强合入。自建基线落后 13 分,主要输在方法论与防线——正是引入对象的强项。

---

## 5. 关键发现

1. **采用度≠质量,盲测拆穿了「评分很高」的默认假设**。安装量/star 双冠军 phuryn 在两组盲测里分别垫底(6.75,低于无技能对照 7.25)与倒数第二(6.00)——它是合格的简化版,但同模型裸跑都不输它。**拿来主义必须测评后再拿,本次流程本身证明了这一步不可省。**
2. **两组冠军都是「防线密度最高」的候选**:Amplitude 的难造假判据+反指标+坏候选对照、mohit 的防作弊速查表+成对护栏——盲测判据里 c5(防作弊/采集诚实)正是拉开分差最大的轴。这与 BW「健康难造假」哲学同构,验证了选型判据没有跑偏。
3. **无技能对照揭示了 skill 的真实增量**:北极星侧 Sonnet 裸跑 7.25,弱 skill(phuryn/refound)是负增量;引领/滞后侧裸跑 5.75(防作弊 2.5),强 skill 增量 +2.5~3.0。**skill 只在带独有方法论装置时才值得装。**
4. **自建基线的画像清晰**:北极星侧有独有优势(自动化免疫检验、既有体系对齐、采集五枚举诚实),引领/滞后侧是结构性短板(无判据、无反指标、阈值贴现状)。改编合入优于推倒重来,也优于原样并存。

## 6. 拿来主义落地(建议,待人批)

| 优先级 | 动作 | 依据 |
|---|---|---|
| P0 | `ImportSkillPackage` 引入 mohit `metrics-framework` + `metric-tree-builder`(`official_library="mohitagw15856/pm-claude-skills"`,MIT,原文保留,plan/16 分域规则下违规仅提示) | 盲测+综合双冠,直接可装 |
| P0 | 自建 `north-star-discovery` 改编合入 Amplitude 四件套:①6 判据逐项打分(含难造假+反指标)②故意坏候选对照 ③BDFE 输入结构 ④NSM↔商业 KPI 系统校验;source 转 SelfBuilt+「改编自 amplitude/builder-skills」留痕(T11) | 盲测 9.0 全票;无 license 不能原样再分发,方法论以自措辞合入 |
| P0 并行 | 向 amplitude/builder-skills 发 issue 请求补 LICENSE,补齐后原样导入替换改编版 | 授权洁癖 |
| P1 | kouko standards 三件套(两轴打分/五类伪 lead 排除/Goodhart 自检,MIT)作为 references 合入引领/滞后推导链 | 方法论四项全有唯一者;教练形态不适合直接当 BW 自动化 skill |
| 不引入 | phuryn(盲测垫底)、refound(语料可选读,不进标配)、lyndonkl(无 license) | §5-1 / §1 授权风险 |

## 7. 读回与复核(报告不代答)

```bash
cat verification/skill-eval-2026-08-04/bench/summary.json
```

```bash
for f in verification/skill-eval-2026-08-04/bench/verdicts/*.json; do echo "== $f"; python3 -c "import json;v=json.load(open('$f'));print(v['panel'],'ranking:',v['ranking'],{k:s['overall'] for k,s in v['scores'].items()})"; done
```

```bash
cat verification/skill-eval-2026-08-04/bench/mapping.json   # 匿名字母 → 真实配置
```

14 份盲产原文在 `verification/skill-eval-2026-08-04/bench/raw/`(文件名即配置),两份调研报告(含全部候选 URL/stars/安装量出处)在同目录——以上均为**本机存档,不入仓**(见文首拍板)。第三方 SKILL.md 原文包同样不入仓(amplitude 无 license;其余以 URL 溯源),留在会话 scratchpad 存档。

---

## 8. 第二轮:全渠道功能等价扩搜 + 终审盲测(2026-08-05)

用户复核意见:第一轮搜索面偏「同名关键词」,要求按**功能等价**放宽、多渠道充分搜索(find-skills/skills.sh、GitHub、市场、社区),从大量源里找可靠 skill 再测。第二轮照此执行,结论:**扩搜找到了 30+ 新候选、4 个入围终审,但没有任何新候选掀翻第一轮冠军;第一轮结论维持,新增一个干净授权的北极星备选**。

### 8.1 扩搜覆盖面(4 渠道并行)

- **渠道B · GitHub 代码级关键词矩阵**:28 个功能关键词(英文 "success metric"/"lead measure"/"counter-metric"/"KPI tree"/"OKR"/"measure what matters"/WIG…+中文 北极星/指标体系/先行指标…,`--filename SKILL.md` 内容搜索)→ 21 个新仓/约 25 个新 skill;镜像/衍生/译本溯源归并(About-Intelligence、Osirs→phuryn;liqiongyu、wuwu119→RefoundAI);中文关键词「引领指标」0 命中如实记录。
- **渠道C · 插件市场+聚合库反查**:逐行解析 **anthropics/claude-plugins-community 官方市场 marketplace.json(24,080 行)** 挖出 3 个渠道B/D 都没发现的候选(tarunccet/pm-skills 等);majiayu000 聚合库(533★)按 metadata.json 溯源,~70% north-star 条目归并回已知四仓;aitmpl.com(30k★)扫过;claudeskills.directory/skillsmp/clawhub 不存在,mcpmarket.com/claudskills.com 被 429 挡,如实记。
- **渠道D · 社区口碑**:HN/Reddit/PH/X/PM 圈一手源;最大发现=anthropics/knowledge-work-plugins(官方,23,297★ gh api 读回,Apache-2.0)的 `metrics-review`。
- **渠道A · skills.sh 全量扫**(2026-08-05 补齐,首次代理中断的缺口已消):打通抓取方法=公开搜索 API `skills.sh/api/search?q=`(返回 JSON 含 installs),24 关键词扫出 **1,791 个唯一 skill**;高装榜逐个功能核验**零漏网**(50 万装的 lark-okr=飞书连接器、8,889 装的 wshobson startup-metrics-framework=CAC/LTV 财务计算手册,均不对题);全部候选安装量读回(冠军们都是个位/两位数:amplitude 4、mohit 50、kouko 1、tarunccet 11;phuryn 1,948 仍盲测垫底)。全文见 `round2/channel-a-skillssh.md`。

### 8.2 筛选(长名单 → 短名单)

45 个候选目录合并去重后,筛选官按「gh api 亲自读回数字 + 原文四问抽读 + license 硬门槛」收敛。两个高星宣称核实**为真但均不对题、不入围**——anthropics/knowledge-work-plugins `metrics-review`(23,297★)本质是"已有指标的周期性复盘",非从零推导;alirezarezvani/claude-skills `cpo-advisor`(23,816★)话题过宽无判据装置。**星数≠对题质量再添两例。**

入围终审 4 家(gh api 读回):borghei/Claude-Skills north-star-metric(451★,**NOASSERTION 无 license**,纸面四问 5/5/5/5 全场最高)、tarunccet/pm-skills metric-definition+/north-star(5★,MIT,收录于 anthropics/claude-plugins-community——**社区提交型市场:经自动安全扫描+分发审核,收录≠官方背书**,官方自维护仓是另一个 claude-plugins-official;此处只作分发合规与发现渠道信号)、gvkhosla/founder-skills north-star-definer(8★,MIT)、nWave-ai/nWave nw-outcome-kpi-framework(589★,MIT)。

### 8.3 终审盲测(新入围 × 2 场景,混入第一轮冠军样本重判;4 评审组 × 2 评委 = 8 裁决)

**北极星组**(overall/10,4 票):自建基线(r1 锚)8.75 > amplitude(r1 锚)8.25 > **tarunccet 8.00** > gvkhosla 7.00 > borghei 6.50
**引领滞后组**:mohit(r1 锚)9.25 = kouko(r1 锚)9.25 ≫ borghei 7.25 > tarunccet 6.00 > nWave 5.00

两轮合并(锚样本 8 票均值):北极星组 amplitude 8.63 / 自建基线 8.25(差 <0.5,按 §2 纪律视为持平区间;两轮内部各有胜负,评委组间方差真实存在);引领滞后组 mohit 9.00 / kouko 8.75,冠军地位在重判下反而加强。

评委证据(摘):
- **tarunccet 为何是合格备选**:「完读率用『该篇预估阅读时长的 80%』做相对阈值,不会随篇幅压缩自相矛盾;否决清单四条全踩要害」(S1);「metric-card 式定义最精确(分子/分母/排除项/算例一应俱全)」(S2)。弱点:引领滞后侧仅 6.00(c5 防作弊 2.5),只能当北极星单项备选。
- **borghei 纸面≠产出的实锤**:S1 评委抓到其产出「声称已在本机实跑 metric_tree_builder.py 退出码 0——这是编造的工具执行证据」;组合公式重复计入、滞后层混入漏斗分量。纸面满分挡不住产出翻车,且无 license,**不引入**。
- **nWave 高星但判据失手**:「引领指标全是读者行为的更快读数,不是作者本周可直接改的内容杠杆」——把 lagging 的快代理当 leading,c1 仅 2.75。

### 8.4 第二轮后的最终结论(替代 §0 中受影响的行)

| 类别 | 业界最佳(两轮盲测) | 干净授权可直接装的业界最佳 | 变化 |
|---|---|---|---|
| 北极星 | amplitude/builder-skills(8 票 8.63;无 license→T11 改编合入,已落地 1aefe50) | **tarunccet/pm-skills `metric-definition`**(MIT,社区市场过审收录——非官方背书,4 票 8.00)——取代第一轮「phuryn 采用度最佳但不推荐」的空缺 | 新增备选;可作 P1 引入观察 |
| 引领/滞后 | mohit metrics-framework+metric-tree-builder(8 票 9.00,已真实引入 73ff197)+ kouko standards 增强 | 同左(MIT) | 不变,重判加强 |

第二轮证据:`verification/skill-eval-2026-08-04/round2/`(4 份渠道报告+shortlist+8 份新产出+8 份裁决+mapping+summary,本机存档不入仓);读回:`cat verification/skill-eval-2026-08-04/round2/bench/summary.json`。局限同 §2(每组 n=4 票)。
