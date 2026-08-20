# 09 · 运作活的 workflow 剧本

> **30 秒导读**:这篇写三张「运作活」(buddy 自己发起的标准动作,不是用户的业务需求)各自的剧本——触发条件、给 agent 塞什么材料、SKILL.md 长什么样、agent 和人什么时候说话、干出什么、怎么保证只能停在「评审中」、干砸了怎么办。三张:①「更新指标 + 制定本周计划」②「资产盘点」(范围 = 仓内全部资产;微重构只出建议活草稿、人勾选才建;老项目历史回填是它的**首次模式**)③「规范铺底」(本篇只写其中需要 agent 的一步:写开发手册)。也管这三份包放在 `standard/` 哪里、版本怎么钉。给接着做 V4 的会话、接手运作活这块的同事看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

---

## 0 · 这篇管什么、不管什么

**管**:对应母文档 §2.5、§2.6、第 0/2-3/6 站,[standard-module-draft.md](../standard-module-draft.md) 第 7 类。具体是三张运作活各自的触发判据、五层渐进加载之上额外注入什么、入口 SKILL.md 节标题与每节要求、对话节点(首/中/尾)、产出(仓文件·库行·MR)、如何保证最远只到「评审中」、失败与边界、一次会话大概多久多少轮(标「估计」)。也管三张运作 workflow 在 `standard/` 的正本位置、版本怎么随 `standard/VERSION` 走、与 `docs/skills/north-star-discovery`、`docs/skills/metrics-binding` 的合并关系、buddy 系统提示词(第 0 层)要不要加一句。

**不管**:运作活③第 1 步(buddy 写模板不起 agent)、老项目历史探测判据是 [03-standard-and-backfill.md](03-standard-and-backfill.md) 的事(已成稿,本篇 2.3/3.2 与它对齐、有分歧处以它为准),本篇只写运作活③里需要 agent 的一步「写开发手册」怎么写、怎么停在评审中——**用户拍板改动**:「历史回填」不再是运作活③自己的一步,它是运作活②「资产盘点」workflow 的**首次模式**(见 2.2/2.3,三层流水线"buddy 先算数、agent 只抄"的架构仍全盘采纳 03 篇 §2.4,本篇只改"这套架构挂在哪个 workflow 包下面");计划屏的看板拖拽、「预览·未合入」切换是 [06-plan-screen.md](06-plan-screen.md) 的事(已成稿,本篇不重复);总览「本周运作」栏、历史运作(回填)块怎么渲染是 [08-overview-derivation.md](08-overview-derivation.md) 的事(已成稿,本篇只引用结论);项目群适配工厂实现是 [07-notify-and-chat-group.md](07-notify-and-chat-group.md) 的事(已成稿),本篇只消费它产出的「上周群摘要」文件(**只喂给运作活①,运作活②/③都不读群历史**);开工工具注册、workflow 识别与导入是 [04-tools-and-workflows.md](04-tools-and-workflows.md) 的事(已成稿);数据模型表结构以 [02-data-and-files.md](02-data-and-files.md) 为正本,本篇第 3 节只引用列名——**盘点之后战绩这件事本身已经取消(02 篇 §2.3):不建战绩台账表,"用了几次"改成现算查询,"干没干成"看远端 MR 合没合入,本篇 §3.4 按此改写,不再与 04 篇(仍是旧写法)对齐,04 篇的同步留给它自己下一轮**。**2026-08-20 按用户信息住哪那次盘点整块重写了 §2.2 定时判据与 §3.2/§3.4**:`cron_task` 表取消后,运作活②的"本周建过没有"判据改成直接查 `issue` 表;`workflow_credit` 表取消后,§3.4 的记账挂载点整段改写成 02 篇 §2.3 的现算方案。

---

## 1 · 用户看到什么、做什么(旅程视角)

周一(或任何时候)Builder 打开总览,若本周还没有 `.bw/plan/YYYY-Www.md`,顶部有横幅「本周还没有计划 → 开始本周」。点一下,buddy 建运作活①、跳到会话屏自动 ▶开工——agent 先复盘上周,再把绑不上数据的指标接上最便宜的采集路径,最后和人一起敲定这周要干的几张活。人不写周目标,只在开头点「开始本周」、结尾看草稿改几个字、点「确认」——buddy 真建这几张活(本地+远端都有),`.bw/metrics.toml` 和计划文件走一个 MR,人再点合入、点完成,一次会话约二三十分钟。

周五晚八点(默认,可改)buddy 自己建运作活②「资产盘点」、自己开工——盘点这周仓内**全部资产**:文档、产物、技能与 workflow 登记、`.bw/plan/`/`.bw/releases.md` 齐不齐、规范对账、指标数据新不新鲜、代码图大文件榜,写份盘点报告追加进计划文件、提个 MR;发现可做可不做的代码微重构(死码、格式、命名、该拆的大文件),**不再直接动手**,只列成「建议活」草稿(类别「优化」),人下周一打开总览时报告已经停在「评审中」,评审报告、合入、点完成的同时勾选要建的建议活。

项目第一次接入(或老项目重铺规范)时人只填两张卡片——若是老仓,buddy 接着自动起一次 agent 会话,把 buddy 约定合进已有文件;若仓有历史,同一张活再多一步——起**运作活②「资产盘点」workflow 的首次模式**,把能找到的历史整理成带「回填」标记的文档(用户定性:老项目历史回填就是资产盘点第一次跑,以后每周跑的是同一个 workflow 的增量模式),一个 MR 全带上,人评审、合入、点完成——总览多出「历史运作(回填)」块,老项目瞬间有了和新项目一样的骨架。

三张活共同的体感:**agent 干活的过程始终看得见,人可以随时插话、随时点停;但推进到「完成」永远是人自己点的那一下**,不管是不是自动建、自动开工。

---

## 2 · 设计

### 2.1 运作活①「更新指标 + 制定本周计划」

**触发与判据**:人触发,两个入口——总览横幅「开始本周」(最顺手),或计划/会话屏手动建一张 workflow 选「更新指标与周计划」的运作活(同一条命令,横幅只是快捷方式)。命令 `StartWeekPlanning`(见 §3)。判据:**当前 ISO 周还没有 `.bw/plan/YYYY-Www.md`**;已有则不再出现横幅,手动入口也被同一判据挡住(幂等,不会建出第二张)。

**注入清单**(五层之上):第 0 层 buddy 系统提示词(固定、短,内容见 2.4);第 1 层仓内 `AGENTS.md`(仓根)(指向先读 PROJECT.md/上周 plan/metrics.toml);第 2 层本活技能 `week-planning`(见下);第 3 层规范第 4 类「指标与数据」件、第 5 类「活的约定」件(`.bw/issue-policy.toml`)、第 7 类「运作节律」件;第 4 层 `.bw/PROJECT.md`、上一周 `.bw/plan/`(若存在)、`.bw/releases.md`、`.bw/metrics.toml`、codegraph 索引。**①独有**:配了项目群时,一份上周群消息摘要本机文件(`FetchChatDigest` 提前生成)——不进仓不进库。

**入口 SKILL.md 大纲**:

> `name: week-planning` · `description: 复盘上周、补齐 .bw/metrics.toml、和人交流出本周目标与业务活草稿` · `category: 运作`
>
> - **何时用**:只在运作活①被开工时用。`.bw/metrics.toml` 不存在(首次接入后第一次跑)则走完整起草而非补齐。
> - **第一步·复盘上周**:读上一周 `.bw/plan/` 与 `.bw/releases.md`;没有上一周文件就如实说明「首次制定周计划,无历史可复盘」,不假装有数据;只读不改。
> - **第二步·更新指标**:调用子技能 `metrics-refresh`(合并自 north-star-discovery/metrics-binding,见 2.4)——补缺的引领/滞后、给绑不上的找最便宜采集路径、改 `.bw/connectors.toml`、跑一次采集;绝不伪造观测、绝不为点亮改指标定义。改动先不提交,和第四步合成一次提交。
> - **第三步·引导出本周**:结合指标现状、代码现状(codegraph)、上周记录(及群摘要),交流出周目标一句 + 业务活草稿(每张:标题/说明/类别/预期推动的指标/建议工具与 workflow)。**不许自己下结论**——写完草稿必须停下,等待人明确的确认信号才能进第四步。
> - **第四步(需人确认后才能做)·落文件**:写入 `.bw/metrics.toml`/`.bw/plan/YYYY-Www.md`/`docs/metrics.md`,`.bw/plan/YYYY-Www.md` 里除周目标、业务活清单外新增一段**「本周指标读数」**(新增,待拍-29:每个已绑定的引领/滞后指标各一行——数字 · 来源 · 采集时间,把这一步刚更新完的指标现状抄一份进周计划文件,随 MR 进仓,让别人打开仓就能看到数,不用装 buddy 或连库);正常提交,`gh pr create`(允许)/`gh pr merge`(禁止,与所有 Issue 共用铁律一致),把 PR 地址打屏。
> - **DoD**:三份文件结构合法;改动是活分支真实提交;仍 Unknown 的指标有诚实说明。
> - **常见坑**:把「引导」做成「代写」;跳过复盘直接进第三步;给绑不上的指标编假 query。

**对话节点表**:

| 时刻 | agent 说什么 | 人做什么 | 不许什么 |
|---|---|---|---|
| 首(触发) | 会话刚打开 | 点「开始本周」 | — |
| 中(复盘+指标播报) | 上周完成 N 个活;A 指标已接采集、B 暂只能手填 | 看,可插话/换路径 | 不许跳过复盘;不许造假观测点亮 |
| 中(草稿讨论) | 呈现周目标 + N 张活草稿 | 改文字、增删活 | 未确认前不许继续 |
| 尾(确认建活) | 「已确认,建 N 张活并提 MR」 | 说「确认」 | 不许自己 merge |
| 尾(评审,属第 5 站)| — | 看 diff、合入、完成 | — |

**产出**:仓——`.bw/metrics.toml`/`.bw/connectors.toml`(若新增采集)/`docs/metrics.md`/新建 `.bw/plan/YYYY-Www.md`(含新增的「本周指标读数」段,待拍-29);库——运作活①本身一行(`kind='ops'`/`origin='human'`/`workflow='更新指标与周计划'`)+ 每张确认业务活各一行(`origin='agent_split'`,待拍-08,`week_of`/`version`/`tool`/`metric_key` 等 8 个缓存列随建活一起写入,02 篇 §2.2)——**没有 `issue_metric`/`week_plan` 这类关联表或索引表**(已取消):指标挂哪由 `issue.metric_key` 单列表达,周列表靠扫 `.bw/plan/` 目录得到(02 篇 §2.1/§2.6)。MR——一个,标题「周计划 2026-Www」,挂在运作活①上。

**停在评审中怎么保证**:会话收尾走既有 `finalize_run_interactive`(`issue_run.rs`)→ `open_pr`(`github.rs`,codehub 走 `create_mr`)——暂存+提交+推送+开 PR;若 agent 已自己跑过 `gh pr create`,遇「already exists」诚实认领(`adopt_existing_pr`)不重复开。随后 hook 的 `Stop` 事件触发 `poll_interactive_inreview`,探测到分支有开着的 PR 就推 `InReview`——这是「评审中」的**唯一**来源,`Done` 仍须人手动 `TransitionIssue`。

**失败与边界**:采集失败 → 指标保持灰,原因写进 `docs/metrics.md`;远端 issue 建失败 → 本地行仍建、标「未同步」,不阻塞其余;MR 开不出来 → 停在 `InProgress`;agent 中途断 → 停原地可重试。不做:agent 不许在未确认前建活;不许替人「写」周目标;群消息只做参考不落库不落仓。

**时长与轮次(估计)**:约 20-40 分钟,人机对话 3-6 轮。首次运行(指标从零起草)明显更长,不设固定估计。

---

### 2.2 运作活②「资产盘点」(用户改名,含首次模式 = 老项目历史回填)

**改动说明(用户拍板)**:原名「资产盘点与代码微重构」改为「资产盘点」——范围从「文档+代码」扩到仓内**全部资产**;代码微重构不再由这个 workflow 直接动手,只产出「建议活」草稿,人勾选才真建。老项目历史回填(原设计里挂在运作活③下的独立子技能)**改为这个 workflow 的首次模式**——同一个 workflow 包 `asset-audit`,读一个 `mode` 参数:`mode=weekly`(默认,定时触发)只盘这一周的变化;`mode=first`(接入老项目时,由运作活③第 3 步或 `BackfillHistory` 命令触发一次)全量回填历史。两种模式**共用同一份 SKILL.md 入口**,第一步先判断 `mode`,后续步骤按模式分支——不是两份文档、两套战绩挂载点。

**触发与判据**:两条触发路径,读同一个 workflow:
- **`mode=weekly`(默认)**:**定时**,默认每周五 20:00(`.bw/issue-policy.toml` 的 `[cadence] ops2_schedule`,可改),`tick_scheduler` 到点自动建 issue(`origin='auto'`)**并自动 ▶开工**——三张运作活里唯一不需要人点一下才开工的一张。**判据(02 篇 §2.1/§2.6 已定)**:到了 `ops2_schedule` 那一刻,查**本周(`week_of` = 当前 ISO 周)有没有一张 `kind='ops' AND workflow='asset-audit'` 的活**——查的是 `issue` 表这一行在不在,不是查一张 `cron_task` 表里"上次触发过没有"的状态(V4 没有 `cron_task` 表,§3.2 展开)。前提 buddy 当时在运行;错过不需要额外补建逻辑——同一条"本周有没有"判据下次启动的第一次 tick 天然成立,自动补建,不需要另一张表记"错过了没有"。
- **`mode=first`**:铺底(运作活③)探测到仓有历史时,不再是"运作活③自己的一步",而是另起一次会话跑这个 `asset-audit` workflow、传 `mode=first`(触发机制细节见 2.3 与 3.2);也可以事后单独由 `BackfillHistory{project_id}` 命令重跑——**这条命令名字不变,但语义收窄为"给 `asset-audit` workflow 传一次 `mode=first`",不是另开一条独立流水线**(03 篇 §2.4 的三层流水线架构原样适用,只是现在挂在这个 workflow 包下面)。**`mode` 怎么传**:不新增 `Command::RunIssue` 字段(04 篇已定该命令签名不变)——写进这张②活自己的说明(body)里一句结构化文字(如 `mode: first`),复用既有"活的说明进初始 prompt"机制,`asset-audit` SKILL.md 第一步读自己的 issue 说明即可判断模式;定时触发建的②活恒为 `mode: weekly`(不写也是默认值)。

**注入清单**:第 0 层系统提示词、第 1 层 `AGENTS.md`(仓根)、第 2 层本活技能 `asset-audit`(含 `mode` 参数)、第 3 层规范第 3 类「目录与知识结构」件+第 6 类「默认件与鱼塘」件(`mode=first` 额外注入 03 篇 §2.4 描述的本机 evidence 文件,不进仓不进库,同「上周群消息摘要」待遇)、第 4 层 `.bw/PROJECT.md`+本周 `.bw/plan/`(盘点报告要追加进去)+codegraph 索引。**②排除**项目群摘要——不管哪种模式都不读群历史,内部盘点/历史回填都不需要「群里聊了什么」(母文档 §2.6 用户四问第 2 条已定,03 篇 §4 同步排除)。

**入口 SKILL.md 大纲**:

> `name: asset-audit` · `description: 盘点仓内全部资产(mode=weekly)或回填老项目历史(mode=first),找该清理的东西只写建议、不动手改` · `category: 运作`
>
> - **何时用**:`mode=weekly` 只由定时触发,**这次会话很可能无人在场**,不要假设有人立刻回应终端;`mode=first` 只由运作活③探测到历史、或 `BackfillHistory` 命令触发,一个项目通常只跑一次。
> - **第一步·判断模式**:读传入的 `mode`。`weekly` 走下面第二至五步;`first` 转到 03 篇 §2.4 的三层流水线(buddy 先算 evidence → agent 只读文本填产物 → 人确认),产出四份仓文件(历史周文件 `.bw/plan/YYYY-Www.md`、`.bw/releases.md` 历史版本行、PROJECT.md 草稿、指标候选)+ 一行库缓存例外(回填的 issue 行,`origin='backfill'`),细节以 03 篇为准,本处不重复。
> - **第二步(`mode=weekly`)·盘点仓内全部资产**:①文档——`.bw/plan/`、`.bw/releases.md` 是否齐全,新增文档是否登记进知识库资产页;②产物与技能/workflow——新增的有没有登记;③规范对账——对照 `.bw/managed.toml` 指纹,检查规范件版本是否落后、有没有人改过,只记录差异**不擅自升级**;④指标数据新鲜度——哪些指标超过保鲜期没有真实观测;⑤代码图大文件榜——`codegraph files -j` 找超行数上限的文件。**不做「零调用者就当死码删」**——`codegraph` 对 `dyn Trait` 分发看不见,会误判;疑似未使用的只写进报告不动手删。
> - **第三步(`mode=weekly`)·把可做可不做的代码微重构列成建议活,不动手**:格式/命名/疑似死码/该拆的大文件这类"可做可不做"的改动,**不在这个 workflow 里直接改代码**——每条整理成一张「建议活」草稿(标题、说明、类别「优化」、`origin='agent_split'`),连同盘点报告一起进这次的 MR 说明,人在周一评审时勾选要建的,勾选的那些才真的调 `CreateIssue` 建成正式活;需要动业务逻辑或公开接口签名的一律只写建议、连草稿都不生成。
> - **第四步(`mode=weekly`)·写盘点报告**:追加进本周 `.bw/plan/` 尾段;没有可重构的东西是正常结果,如实写「无」不硬造改动。
> - **第五步**:提交+MR,打屏(多半没人看,仍要打)。
> - **DoD**:`mode=weekly` 时报告确实追加、建议活草稿都在 MR 说明里、这个 workflow 自己没有直接改动业务代码(`git diff --stat` 应只见格式/命名类小改动或完全没有代码改动);`mode=first` 时 DoD 见 03 篇。
> - **常见坑**:把「疑似未使用」直接删掉而不是先写成建议活;报告写空话;把微重构悄悄做了却不在 MR 说明里说清楚它是"这次自己动手改的"还是"一条建议";无人应答就卡住等待——**没人在场按"能做的先做、拿不准写进报告"推进**。

**对话节点表**(与①最大不同:不能假设人在场):

| 时刻 | agent 说什么 | 人做什么 | 不许什么 |
|---|---|---|---|
| 首(定时触发,或运作活③/`BackfillHistory` 传 `mode=first`) | 「本周运作」栏出现「已自动开工」(`mode=weekly`)或总览显示「历史回填进行中」(`mode=first`)| 通常不在场,随时能点进看 | — |
| 中(自主推进) | 逐步播报盘点结果,拿不准写进报告;`mode=weekly` 遇到可微重构的地方只说"已列为建议活" | 若在场可插话 | 不许因无人应答卡住不推进;`mode=weekly` 不许直接动业务代码 |
| 尾(周一评审) | — | 看报告/diff、合入、完成、勾选要建的建议活 | 不属于本次会话 |

**产出**:仓——`mode=weekly`:盘点报告(追加进 plan 尾段)+ 建议活草稿(写进 MR 说明,不直接落库)+ 极小范围微重构改动(若有,限定格式/命名);`mode=first`:见 03 篇五项产物。库——运作活②一行(`origin='auto'`,`workflow='asset-audit'`)、定时触发记录(`mode=weekly`)或历史回填相关 issue 行(`mode=first`,`origin='backfill'`);人勾选建议活草稿后才新增业务活行(`origin='agent_split'`,类别「优化」)。MR——一个,`mode=weekly` 通常周一才被看到,`mode=first` 见 03 篇「一个 MR 与人介入」。

**停在评审中**:与①同一条机制(`finalize_run_interactive` → `open_pr` → hook `Stop` → `poll_interactive_inreview` 推 `InReview`)——自动开工与人工开工用同一条交互式执行器和状态机通路,没有特殊待遇,两种模式同样适用。

**失败与边界**:工作区不可用 → 如实跳过不建活,不产生任何库记录(没有 `cron_task`/`cron_run` 这类表可写,§3.2)——下次 tick 用同一条"本周有没有"判据重试,自然补建;无东西可重构 → 报告写「无」;agent 中途断 → 停 `InProgress` 可重试;`mode=first` 的失败与边界以 03 篇 §4 为准,不重复。

**时长与轮次(估计)**:`mode=weekly` 约 10-25 分钟,人机对话通常 0 轮;`mode=first` 差异大,见 2.3 节「历史回填」原有的估计段(03 篇/legacy-backfill.md 已提示大仓可能明显更长)。

---

### 2.3 运作活③「规范铺底」——agent 的那一步:写开发手册(历史回填在 2.2)

> **2026-08-20 整节重写。** 这一步原来叫「合并调整」,做的是「把 buddy 的固定
> 章节合并进已有 AGENTS.md」。那个方向是反的,已经推翻——理由与新分工写在
> [03 篇](03-standard-and-backfill.md) §2.3,本节只写剧本。

三步整体流程(判据、探测逻辑、第 1 步无 agent 的模板写入)属于
[03-standard-and-backfill.md](03-standard-and-backfill.md),本节只写第 2 步需要
agent 的部分。**历史回填不在本节**:探测到仓有历史时,运作活③改为触发运作活②
「资产盘点」workflow、传 `mode=first`(见 2.2),复用同一份 `asset-audit`
SKILL.md;03 篇 §2.4 描述的三层流水线原样成立,只是挂载的 workflow 包换了。

**这一步到底在干什么**:给这个项目写一份**开发手册**——怎么建、怎么跑、怎么
测、目录里什么在哪、这个项目和别处不一样在哪。落点是**仓根 `AGENTS.md`**
(不在 `.bw/` 里:那是 Claude Code / Cursor / Codex 共同约定的位置,塞进 `.bw/`
谁都读不到;它是项目的资产,不是 buddy 的资产)。buddy 自己的铁律**一个字都
不往这儿写**,它们由 `agent_system_prompt` 每场会话新鲜注入。

**触发与判据**:人填两卡完成接入 → buddy **自动建**一次性运作活③
(`RunStandardBootstrap`,`origin='auto'`)。第 1 步(写模板 + 现探构建命令与
目录列表)不起 agent,由 Rust 代码完成。**第 2 步在两种情况下都要跑**:

| 仓的样子 | 第 1 步做了什么 | 第 2 步这个会话的活 |
|---|---|---|
| 仓根没有 `AGENTS.md` | 写了一份模板,里面有几节写着「还没填」 | 把「还没填」那几节填成真的 |
| 仓根已有人写的 `AGENTS.md` / `CLAUDE.md` | **一个字没写**,跳过的路径记在活的正文里 | 补进人写的那份:原有内容一字不删,缺的几节补上,`bw:managed` 那段加在最前面 |

**注入清单**:第 0 层系统提示词;第 1 层这张活自己的正文(里面写着第 1 步跳过
了哪些路径);第 2 层本活技能 `merge-adjust`(目录名沿用,内容已整篇重写);
第 3 层仓根 `AGENTS.md` 现状(模板初稿或人写的原文)、`README.md`、构建文件、
CI 配置、顶层目录。

**入口 SKILL.md 大纲**:

> `name: merge-adjust` · **写开发手册**——①先读仓根 `AGENTS.md`:是 buddy 刚写的
> 模板,就找里面写着「还没填」的几节;是人写的,就通读一遍,记住原文一字不能删。
> ②读 `README.md`、构建文件(`Cargo.toml` / `package.json` / `pyproject.toml` /
> `go.mod` / `Makefile`)、CI 配置、逐个顶层目录,把「怎么建 / 跑 / 测」「目录导览」
> 「提交与评审」几节填成真的。③**每条命令真跑过一次才写进去**——写错一条构建
> 命令比留空更糟,人照着跑一次失败才发现是编的;跑不通就照实留「还没填」并说明
> 为什么。④`bw:managed` 那一段**不许改一个字**,它随规范版本整段替换;人写的
> 原文不许删、不许改写,只在缺的地方补。⑤`CLAUDE.md`:不存在就写一行
> `@AGENTS.md`;已经有内容就在最前插一行导入,原内容后移不删。⑥MR 说明里一节
> 一行交代:这一节的内容是从哪个文件读来的、哪条命令真跑过。**DoD**:没有一节
> 还写着「还没填」而其实读得出来;写进去的命令都真跑过;人写的原文一字未删。
> **常见坑**:凭经验编一条构建命令;把人写的段落"顺手改通顺";把 buddy 的铁律
> 抄进仓里(那是系统提示词的事,抄进来就是第二个会漂移的副本)。

**对话节点表**:

| 时刻 | agent 说什么 | 人做什么 | 不许什么 |
|---|---|---|---|
| 首(自动触发) | 总览显示「规范铺底进行中」 | 通常不在场 | — |
| 中(播报) | 「正在读 CI 配置,试跑 `cargo test`……」 | 可旁观 | 不许把没跑过的命令写进手册 |
| 尾(评审) | — | 一次性看完整个 MR(可能含 `asset-audit(mode=first)` 那次会话的改动)、合入、完成 | — |

**产出**:仓——仓根 `AGENTS.md` / `CLAUDE.md`;库——本节不产生
`origin='backfill'` 的行(那是 2.2 节 `asset-audit(mode=first)` 的产出)。
MR——一个:第 1 步写核心件 → 第 2 步写开发手册 → (命中才有)资产盘点首次模式,
三段提交同一条分支,合入前统一由「一次性开 PR」的既有机制收口,不产生两个通知。

**停在评审中怎么保证**:**没有例外**——2026-08-20 起「空仓/buddy 自己的仓直推
默认分支」这条例外已经取消(03 篇 §2.2),新仓老仓走同一条路:第 1 步文件先提交
在 `bw/issue-<n>` 分支上**先不开 PR**;有 agent 步骤就在同一分支继续提交,全部
写完后由会话收尾的 `finalize_run_interactive` → `open_pr` 一次性开 PR,遇
「already exists」→ `adopt_existing_pr` 认领不重复开;没有 agent 步骤就由
`open_bootstrap_pr` 直接开。之后同样靠 hook/轮询推 `InReview`。

**失败与边界**:会话如实停在「进行中」、可重试;资产盘点首次模式(历史回填)的
失败与边界以 2.2 节/03 篇 §4 为准,不重复。

**时长与轮次(估计)**:纯写手册约 10-20 分钟,人机对话通常 0 轮;历史回填
(资产盘点首次模式)的估计见 2.2 节末尾。

---

### 2.4 三张运作 workflow 住哪、版本、与两个现有技能的合并、系统提示词要不要加一句

**放哪**:仓根 `standard/` 目录(01 篇 §2.9 已定),规范第 6 类「默认件与鱼塘」:

```
standard/06-defaults/ops/
├── README.md                     # 三张运作 workflow 总说明 + 指向 CHANGELOG
├── week-planning/
│   ├── SKILL.md                  # 入口①
│   └── skills/metrics-refresh/SKILL.md  # 子技能,合并自 north-star-discovery+metrics-binding
├── asset-audit/SKILL.md          # 入口②——读 mode 参数分支:weekly(默认,每周)/ first(老项目历史回填,
│                                  #   用户拍板改动:原挂在③下的「历史回填」子技能并进这里,不再单独存在)
└── standard-bootstrap-agent/
    └── merge-adjust/SKILL.md     # 入口③需要 agent 的唯一一步(用户拍板改动:history-backfill 已迁出,见 asset-audit)
```

**不进项目仓**:这三份(含子技能 `metrics-refresh`)和九篇方法论技能一起编在 buddy 二进制里,开工前展开到 buddy 自己的资产目录,系统提示词只给名字+一句话+路径。原来定的「铺底时整棵目录物化进项目仓 `.claude/skills/`」已于 2026-08-20 推翻,见 [04 篇](04-tools-and-workflows.md) §2.7。

**版本**:不单开版本线——版本号就是 `standard/VERSION`,随 buddy 整体发布走。内容改了 = `standard/CHANGELOG.md` 记一行 + 规范整体版本 +0.1(与 [standard-module-draft.md](../standard-module-draft.md) §3 一致)。项目侧 `.bw/managed.toml` 记这几份 SKILL.md 的指纹,对账时能测出落后并提示升级,不需要特例。

**与 `north-star-discovery`/`metrics-binding` 的合并**:两份技能今天独立存在于 `docs/skills/`(编译进 `bw_library.rs`),挂在"标配 Issue 三件套"(竞品分析→找指标→绑数据)触发链上。母文档第 0 站「不带」清单已写明"找指标+绑数据并入运作活①",这条独立触发链在 V4 不再存在。本篇判断:**内容合并进 `week-planning/skills/metrics-refresh/SKILL.md`,原两份文件退役**(按 CLAUDE.md「发现过时的实现路径,直接移除它」,不留并行旧链当兼容层)。

- **原样沿用**:north-star-discovery 的三段拆解、虚荣指标黑名单、自动化免疫检验、北极星判据打分 6 项、反指标机制、BDFE 输入结构、NSM↔商业 KPI 校验;metrics-binding 的硬性约束(绝不伪造、绝不为点亮改定义)、按 `collect.kind` 分支的诊断表、`script` kind 搭装置流程。
- **改写语境**:两份旧技能写的前置条件是「标配 Issue『找指标 / 绑数据』」,这个东西在 V4 里不存在,改成「运作活①第二步」;新增判断——`.bw/metrics.toml` 已有完整三层指标时只做**增量校准**,只有首次跑运作活①(文件不存在)才走完整起草流程。
- **迁移落地**(旧文件物理删除、`bw_library.rs` 常量去留)是代码改动,留给实现,本篇只定"内容去哪、哪些段落沿用"。

**系统提示词(第 0 层)要不要加一句**:今天的「所有 Issue 共用的铁律」天然覆盖运作活(它们也是 `issue` 表的行),①③不需要重复。但运作活②引入了今天不存在的新场景——**完全无人在场的 agent 会话**。本篇建议加一句:

> 「这次会话可能是定时自动触发、当下无人在场——按你能做的先做、拿不准的写进报告等人看的原则推进,不要因为没人回应就等待或卡住流程。」

具体措辞与是否采纳留用户拍板(见 §6)。

---

## 3 · 工程对照

### 3.1 `issue` 字段取值

| 活 | `kind` | `origin` | `tool` | `workflow` |
|---|---|---|---|---|
| ①更新指标 + 制定本周计划 | `ops` | `human` | `claude_cli` | `更新指标与周计划` |
| ②资产盘点(`mode=weekly`) | `ops` | `auto` | `claude_cli` | `资产盘点` |
| ②资产盘点(`mode=first`,老项目历史回填)| `ops` | `auto` | `claude_cli` | `资产盘点` |
| ③规范铺底 | `ops` | `auto` | `claude_cli`(仅追加 agent 步骤时才有一次交互式会话)| `规范铺底` |

① 引导出的每张业务活各自是独立 `issue` 行(`kind='business'`,`origin='agent_split'`,待拍-08);②`mode=weekly` 里人勾选的建议活同样是 `origin='agent_split'`、类别「优化」;探测到远端已有 issue 回填成的行 `origin='backfill'`(用户拍板改动:这条来自②的 `mode=first`,不再是③自己产的),与运作活②/③本身这两张 `kind='ops'` 行是两回事。

### 3.2 触发命令(伪码,未拍板)

```rust
// ①判据是文件存在性,不是库表——02 篇 §2.1/§2.6 已定:没有 week_plan 表,
// 「有没有本周计划」直接看 .bw/plan/YYYY-Www.md 这份文件在不在。命中即拒绝,
// 幂等,不建出第二张本周①。
pub async fn start_week_planning(&mut self, project_id: ProjectId) -> Result<IssueId, AppError> {
    let week_of = current_iso_week();
    let plan_path = format!("{workspace}/.bw/plan/{week_of}.md");
    if std::path::Path::new(&plan_path).exists() {
        return Err(AppError::WeekPlanAlreadyExists(week_of));
    }
    let issue_id = self.dispatch(Command::CreateIssue {
        id: IssueId::new(), stage: current_stage,
        title: format!("更新指标 + 制定本周计划 {week_of}"),
        desc: String::new(), priority: IssuePriority::Normal,
        standard_skill: "week-planning".into(),
    }).await?;
    self.store.set_issue_kind_origin(issue_id, IssueKind::Ops, IssueOrigin::Human).await?;
    self.store.set_issue_workflow(issue_id, "更新指标与周计划").await?;
    self.dispatch(Command::RunIssue { id: issue_id, session: None }).await // 同人点▶开工的路径
}

// ②V4 只有这一条定时(02 篇 §2.1「cron_task 表已取消」/握手清单 七-6):
// 不再是"tick_scheduler 遍历 cron_task 表里到点的任意配置行",而是每次 tick
// 直接读 .bw/issue-policy.toml 的 [cadence] 段、查一条 SQL。判据是"本周
// (week_of=当前 ISO 周)有没有一张 kind='ops' AND workflow='asset-audit' 的活",
// 不是查一张记"上次触发时间"的表——这条 SQL 本身就是幂等锁,错过一次 tick
// 不需要补建逻辑,下次 tick 天然成立。函数名/参数是本篇伪码,未拍板;精确的
// 时刻解析("fri 20:00"落在哪个具体时间戳)留给实现。
pub async fn maybe_fire_asset_audit(&mut self, pid: ProjectId, now_ts: i64) -> Result<(), AppError> {
    let Some(policy) = bw_engine::issue_policy_file::read(&workspace).await? else { return Ok(()) };
    let week_of = current_iso_week();
    if !policy.cadence.ops2_due(now_ts, &week_of) {
        return Ok(()); // 还没到 ops2_schedule("fri 20:00" 默认值)这一刻
    }
    // 判据本身,不是另开一张状态表——对应 SQL:
    //   SELECT COUNT(*) FROM issue WHERE project_id=? AND kind='ops'
    //   AND workflow='asset-audit' AND week_of=?
    let already = self.store.count_ops_issue(pid, "资产盘点", &week_of).await?;
    if already > 0 {
        return Ok(()); // 本周已建过(正常一次,或上次没错过的一次),不重复建
    }
    let issue_id = self.dispatch(Command::CreateIssue {
        id: IssueId::new(), stage: current_stage,
        title: format!("资产盘点 {week_of}"), desc: "mode: weekly".into(),
        priority: IssuePriority::Normal, standard_skill: "asset-audit".into(),
    }).await?;
    self.store.set_issue_kind_origin(issue_id, IssueKind::Ops, IssueOrigin::Auto).await?;
    self.store.set_issue_workflow(issue_id, "资产盘点").await?;
    self.store.set_issue_week_of(issue_id, &week_of).await?; // 供下次 tick 的判据 SQL 查到这一行
    // mode 不进 Command::RunIssue 签名(04 篇已定该命令「签名不变」)——已经写进
    // 这张 issue 自己的说明(body,上面的 desc)里一句结构化文字("mode: weekly"),
    // asset-audit SKILL.md 第一步读自己的 issue 说明即可判断模式,不新增命令
    // 字段。定时触发这里恒为 weekly。
    self.dispatch(Command::RunIssue { id: issue_id, session: None }).await?;
    self.emit(Event::OpsWorkflowAutoFired { id: issue_id }); // 01 篇字段:{ id: IssueId }
    Ok(())
}

// ③第 1 步无 agent,buddy 直接写模板(含现探构建命令与目录列表);是否追加 agent
// 步骤由探测结果分两条独立分支——写开发手册(运作活③自己的一步)与历史回填(改为
// 触发运作活②同一个 asset-audit workflow、传 mode=first,不是运作活③的子技能)。
// 函数名与探测结构体照抄 03 篇 §3.1/§3.2(probe/BootstrapProbe 已在那边定义)。
pub(crate) async fn run_standard_bootstrap(&mut self, p: ProjectId) -> Result<(), AppError> {
    let probe = bw_engine::standard_bootstrap::probe(&workspace).await;
    let issue_id = self.create_ops_issue(p, &title_for(&probe), &body_for(&probe)).await?;
    self.write_standard_core_files(p, &probe).await?; // 第1步,同步,不起 agent
    {
        // 2026-08-20 起没有"空仓直推"这条例外了(03 篇 §2.2):新仓老仓同一条路。
        // 写开发手册与历史回填各自独立判断是否需要,命中的按顺序在同一分支继续提交
        // (先写手册、再资产盘点首次模式,见 2.3),都不命中则直接开 PR。
        let need_agent_step = probe.has_agent_docs || probe.has_history;
        if need_agent_step {
            // 复用运作活②同一条"自动▶开工"能力(auto_start_run,底层即 01 篇
            // CreateAutopilotTask{auto_run}),同一分支内按需依次起两次交互式会话:
            if probe.has_agent_docs {
                // 技能目录名沿用 merge-adjust,内容已整篇重写成「写开发手册」。
                self.auto_start_run_with_skill(issue_id, "merge-adjust").await?;   // 2.3 节
            }
            if probe.has_history {
                // 不是"标准铺底"自己的技能——起同一个 asset-audit workflow(2.2 节),
                // 把 mode: first 写进这张②活自己的说明(body)里(同上,不加命令字段),
                // 复用同一个 workflow 包,不是另开一条平行流水线。
                self.auto_start_run_with_skill(issue_id, "asset-audit").await?;
            }
            // 收尾走既有 finalize_run_interactive → open_pr,"already exists" 时诚实认领。
        } else {
            // 两条探测都为假:没有 agent 步骤,buddy 直接开 PR——评审中靠下次
            // 兜底轮询探测到(没有 hook Stop 事件可触发)
            self.open_bootstrap_pr(p, issue_id).await?;
        }
    }
    Ok(())
}
```

### 3.3 活草稿 → 真建活(①第四步)

人在终端确认草稿后两条动作并行(细节留实现时与计划屏交互一并定稿):①agent 继续在自己的会话里提交 `.bw/metrics.toml`/`.bw/plan/`、开 PR(2.1 第四步);②buddy(不经过 agent)批量调既有 `Command::CreateIssue` 为每张确认的业务活各建一行(含远端 issue 创建),`week_of`/`version`/`tool`/`metric_key` 等 8 个缓存列创建后一并写入——**没有 `issue_metric` 关联表**(已取消):一张活推动的指标就是 `issue.metric_key` 这一列,不需要另插一行关联表(02 篇 §2.2)。两条动作各自失败互不阻塞,失败表现按 §4 处理。

### 3.4 "用了几次"怎么算:02 篇 §2.3 的现算方案(取代原 `workflow_credit` 台账)

**与 04 篇的口径分歧本篇直接按 02 篇改写,不再等两篇互相对齐**:02 篇盘点之后连"战绩"这个持久账本概念本身都取消了(母文档 §6.3)——不建 `workflow_credit` 表,也不在 `skill_package`/`skill` 上存 `runs`/`wins`/`win_rate`。"用了几次"改成现算查询,"干没干成"不再由 buddy 自己判定和记账,看的是**远端 MR 合没合入**。04 篇目前仍是"以 `workflow_credit` 为事实源"的旧写法,与 02 篇不一致——按 CLAUDE.md「不为向后兼容留旧路径」的原则,以 02 篇(更新的正本)为准;04 篇自身的同步留给它下一轮修订,不在本篇处理范围。

**"用过几次"怎么算**(直接引用 02 篇 §2.3 的查询,不重新定义):

```sql
SELECT workflow, COUNT(*) AS uses
FROM issue WHERE project_id = ? AND kind = 'business' AND workflow != ''
GROUP BY workflow;
```

三张运作活各自的 `workflow` 取值见 §3.1;配置屏「用过几次」就是这条查询按 `workflow` 名字过滤后的结果,不缓存汇总数,每次现查——不需要"挂载点""结算时机"这类设计,因为没有一次插入动作要做。

**"干没干成"怎么判**:不再有 `TransitionIssue` Done 边/run 失败两处的记账挂载点——这两个时机今天仍然触发状态机原有的职责(推进/停留),但不再附带"往战绩表插一行"这个副作用。一件活(业务活或运作活)真正"干成了"看的是它对应的远端 PR/MR 合没合入,这条判据直接读 git/远端,不读库,和母文档 §6.3「代价」一节的表述一致。

**回填的活自然不参与"用过几次"**:`origin='backfill'` 的历史 issue 行 `workflow` 列通常是空字符串(远端老 issue 没有关联到任何本地 workflow),上面查询的 `WHERE workflow != ''` 条件天然把它们排除在外——不需要一条专门的"回填跳过记账"规则,这本来就是空值过滤的自然结果(03 篇同步简化了对应描述)。

---

## 4 · 边界与失败

**不做什么**:

- **自动完成**——三张活不管触发方式,最远只能到「评审中」,`Done` 永远需人手动 `TransitionIssue`/`MergeIssuePr`。
- **运作活②(`mode=weekly`)直接改业务代码**——用户拍板改动:连"小范围重构"也不再由它动手,可做可不做的改动(死码、格式、命名、该拆的大文件)一律只产出「建议活」草稿,人勾选才真建;它自己在这次会话里的改动范围只有盘点报告本身(追加进 `.bw/plan/` 尾段)。
- **运作活①替人写周目标、不确认就建活**——第三步做完必须停下等确认,第四步只在收到确认后才执行。
- **群消息进库**——①的上周摘要是本机文件参考,读完即用,不落库不落仓,与健康信号无关;探测要不要追加②的 `mode=first`(历史回填,用户拍板改动:不再是"运作活③第 3 步",而是触发②同一个 workflow)时,"项目群已配置"只是五个判据之一(03 篇 §2.1),群历史本身不作为回填内容的输入源(03 篇 §4 已排除),`mode=weekly`/`mode=first` 都不读群历史。

**失败如实(汇总,逐活细节见 §2)**:

| 场景 | 表现 |
|---|---|
| 采集脚本失败(①)| 对应指标保持灰,`docs/metrics.md` 写明原因 |
| 远端 issue 建失败(①确认建活)| 本地行仍建、标"未同步",不阻塞其余 |
| 定时触发但工作区不可用(②)| 如实跳过不建活,不产生任何库记录(没有 `cron_task`/`cron_run` 这类表可写);下次 tick 用同一条"本周有没有"判据重试 |
| 远端未认证(②`mode=first` 历史回填)| 只完成 git 本地部分,远端字段留空 |
| agent 中途断(通用)| 停 `InProgress`,`settled_at` 留空,可重试 |
| MR 开不出来(通用)| 停原状态,不假装到了「评审中」|

---

## 5 · 验收与读回

> 三张运作活不另建指挥器:`cargo run -p bw-v4 --example real_demo_v4` 的主线本身就覆盖了
> ①和②,③在步骤 2 就跑掉了。下面是真实跑得出来的读回,不是建议。

跑一遍(从空库开始,重跑不产生重复数据):

```bash
cargo run -p bw-v4 --example real_demo_v4 -- <db> <workspaces-root>
```

| 核验什么 | 命令 / SQL | 真实结果 |
|---|---|---|
| 三张运作活各自的身份 | `sqlite3 <db> "SELECT number,kind,origin,workflow,week_of,status FROM issue WHERE kind='ops';"` | `1\|ops\|auto\|规范铺底\|\|backlog` · `2\|ops\|human\|更新指标与周计划\|2026-W34\|in_review` · `6\|ops\|auto\|资产盘点\|2026-W34\|in_review` |
| ①判据生效(不建第二张) | 指挥器跑第二遍 | 日志:「步骤 3 · 本周文件已存在,跳过(重跑不产生重复数据)」 |
| ①产出文件 | `test -f <ws>/.bw/plan/2026-W34.md && echo ok` | `ok`,里面有「本周指标读数」段 |
| ②定时真的到点建活并开工 | 指挥器步骤 8 | 日志:「本周的『资产盘点』在了 —— #6 来源 定时自动建 状态『评审中』」 |
| **自动建的活绝不被自动推进到完成** | `sqlite3 <db> "SELECT COUNT(*) FROM issue WHERE kind='ops' AND settled_at IS NOT NULL;"` | `0` |
| 三份剧本真摊在 buddy 自己的目录里,且没进用户仓 | `cargo run -p bw-v4 --example prompt_smoke -- <目录>` | 13 份全落盘;用户仓里没有 `.claude/skills/`;提示词里只有路径没有正文 |
| 剧本记了指纹(规范对账认得出) | `grep -c '^\[\[file\]\]' <ws>/.bw/managed.toml` | `18` |
| 「用过几次」现算(没有台账表) | `sqlite3 <db> "SELECT workflow, COUNT(*) FROM issue WHERE kind='business' AND workflow!='' GROUP BY workflow;"` | 按 workflow 名分组的实数 |
| 会话屏能打开不炸 | `BW_DB=<db> BW_OPEN=<项目名> BW_PANEL=session ./target/debug/bw-v4-dev` | stderr 见 `[BW_OPEN] … panel=Session`,无 panic |

**关于「到点」怎么验**:判据是本机时间的「星期几 + 几点几分」有没有越过 `.bw/issue-policy.toml`
里 `[cadence] ops2_schedule` 那一刻(默认 `"fri 20:00"`)。指挥器**不改系统时间**——它把演示
项目自己的这一行改成 `"mon 00:00"`,让那一刻真的已经过去,并在日志里明说改了。正式项目的默认
值不变。

**②的这两条还没法读回**,因为 `mode=weekly` 的盘点报告与建议活草稿要靠真 agent 会话产出,B 刀
跑的是自我标注的替身:盘点报告落地(`grep` 报告尾段)、微重构只出建议不改代码
(`git diff --stat`)。`mode=first` 的历史回填整条是 C 刀的事。

## 6 · 开放问题(≤5)

1. ~~母文档与 03 篇在"群历史算不算回填原料"上不一致~~ **已定(握手清单 第 4 条「回填不主动喂群历史」)**:母文档 §2.6/第 0 站现已改成三种原料(git 本地历史、仓内文档、远端 issue/MR)并明确排除群历史,与 03 篇 §4、本篇 2.2/2.3 口径一致,不用再改。
2. ~~02 篇 `workflow_credit` 表与 04 篇 `skill_package`/`skill` 战绩列口径不一致~~ **已定,且盘点之后进一步简化**:原「以 02 篇 `workflow_credit` 台账表为事实源」的设计期统一结论已被取代——02 篇盘点之后连 `workflow_credit` 表本身也取消,"用了几次"改成现算查询、"干没干成"看远端 MR,不再有任何持久战绩表,见 §3.4。04 篇仍是"以 `workflow_credit` 为事实源"的旧写法,留给它自己下一轮同步,不在本篇处理范围。
3. ~~`north-star-discovery`/`metrics-binding` 旧文件的迁移时机~~ **如实更新**:内容已经合并进 `standard/06-defaults/ops/week-planning/skills/metrics-refresh/SKILL.md`,但**旧的两份文件还在** `docs/skills/` 里、也仍然被 `bw_core::bw_library` 编进二进制、仍然随铺底复制进项目仓。也就是说现在两份并存,违反「不为向后兼容留旧路径」。删除动作记在 `docs/LEFTOVERS.md`,没做就是没做,不在这里写成"已迁移"。
4. **系统提示词是否真要加 2.4 建议的那一句**——具体措辞是否合适、是否采纳,需用户确认。
5. **运作活③纯模板路径(无 agent)时"评审中"怎么被探测到**——§3.2 提到这种情况没有 hook `Stop` 事件,只能靠既有 5 分钟兜底轮询。这条路径此前只服务"project-init"特殊场景,V4 里第一次成为常规路径,轮询节律要不要为此加速,留待实现时评估。
