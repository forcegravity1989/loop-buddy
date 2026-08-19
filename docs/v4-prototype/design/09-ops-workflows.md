# 09 · 运作活的 workflow 剧本

> **30 秒导读**:这篇写三张「运作活」(buddy 自己发起的标准动作,不是用户的业务需求)各自的 workflow(技术上讲是一份 SKILL.md 入口 + 支撑文件的技能包,agent 读它决定怎么干活)剧本——触发条件、给 agent 塞什么材料、SKILL.md 长什么样、agent 和人什么时候说话、干出什么、怎么保证只能停在「评审中」、干砸了怎么办、大概多久。三张:①「更新指标 + 制定本周计划」②「资产盘点与代码微重构」③「规范铺底」(本篇只写其中需要 agent 的两步:合并调整、历史回填)。也管这三份 workflow 在 `standard/` 放哪、版本怎么钉、和已有的 `north-star-discovery`/`metrics-binding` 两份技能怎么合并、系统提示词要不要加一句。给复核设计的用户、下一步写代码的会话、接手运作活这块的同事看。**现在作数吗**:详细设计稿,待用户复核,尚未开工写代码。与母文档([`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md))冲突时以母文档为准。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)——本文不新开代号系列,分步骤一律写「第 N 步」。

---

## 0 · 这篇管什么、不管什么

**管**:对应母文档 §2.5、§2.6、第 0/2-3/6 站,[standard-module-draft.md](../standard-module-draft.md) 第 7 类。具体是三张运作活各自的触发判据、五层渐进加载之上额外注入什么、入口 SKILL.md 节标题与每节要求、对话节点(首/中/尾)、产出(仓文件·库行·MR)、如何保证最远只到「评审中」、失败与边界、一次会话大概多久多少轮(标「估计」)。也管三张运作 workflow 在 `standard/` 的正本位置、版本怎么随 `standard/VERSION` 走、与 `docs/skills/north-star-discovery`、`docs/skills/metrics-binding` 的合并关系、buddy 系统提示词(第 0 层)要不要加一句。

**不管**:运作活③完整三步(第 1 步 buddy 写模板不起 agent、老项目历史探测判据、三层流水线里"buddy 先算数、agent 只抄"的架构)是 [03-standard-and-backfill.md](03-standard-and-backfill.md) 的事(已成稿,本篇 2.3/3.2 与它对齐、有分歧处以它为准),本篇只写第 2/3 步需要 agent 的两个 SKILL.md 怎么写、怎么停在评审中;计划屏的看板拖拽、「预览·未合入」切换是 [06-plan-screen.md](06-plan-screen.md) 的事(已成稿,本篇不重复);总览「本周运作」栏、历史运作(回填)块怎么渲染是 [08-overview-derivation.md](08-overview-derivation.md) 的事(已成稿,本篇只引用结论);项目群适配工厂实现是 [07-notify-and-chat-group.md](07-notify-and-chat-group.md) 的事(已成稿),本篇只消费它产出的「上周群摘要」文件;开工工具注册、workflow 识别与导入、技能战绩记账机制是 [04-tools-and-workflows.md](04-tools-and-workflows.md) 的事(已成稿,本篇 3.4 记账口径与它对齐);数据模型表结构以 [02-data-and-files.md](02-data-and-files.md) 为正本,本篇第 3 节只引用列名——**设计期统一:战绩以 02 篇 `workflow_credit` 台账表为事实源(04 篇已同步改写),本篇 §3.4 写清挂载点**。

---

## 1 · 用户看到什么、做什么(旅程视角)

周一(或任何时候)Builder 打开总览,若本周还没有 `docs/plan/YYYY-Www.md`,顶部有横幅「本周还没有计划 → 开始本周」。点一下,buddy 建运作活①、跳到会话屏自动 ▶开工——agent 先复盘上周,再把绑不上数据的指标接上最便宜的采集路径,最后和人一起敲定这周要干的几张活。人不写周目标,只在开头点「开始本周」、结尾看草稿改几个字、点「确认」——buddy 真建这几张活(本地+远端都有),`.bw/metrics.toml` 和计划文件走一个 MR,人再点合入、点完成,一次会话约二三十分钟。

周五晚八点(默认,可改)buddy 自己建运作活②、自己开工——盘点这周新增文档有没有登记、代码里有没有该清理的死码或过长文件,写份盘点报告追加进计划文件、提个 MR,人下周一打开总览时它已经停在「评审中」。

项目第一次接入(或老项目重铺规范)时人只填两张卡片——若是老仓,buddy 接着自动起一次 agent 会话,把 buddy 约定合进已有文件、把能找到的历史整理成带「回填」标记的文档,一个 MR 全带上,人评审、合入、点完成——总览多出「历史运作(回填)」块,老项目瞬间有了和新项目一样的骨架。

三张活共同的体感:**agent 干活的过程始终看得见,人可以随时插话、随时点停;但推进到「完成」永远是人自己点的那一下**,不管是不是自动建、自动开工。

---

## 2 · 设计

### 2.1 运作活①「更新指标 + 制定本周计划」

**触发与判据**:人触发,两个入口——总览横幅「开始本周」(最顺手),或计划/会话屏手动建一张 workflow 选「更新指标与周计划」的运作活(同一条命令,横幅只是快捷方式)。命令 `StartWeekPlanning`(见 §3)。判据:**当前 ISO 周还没有 `docs/plan/YYYY-Www.md`**;已有则不再出现横幅,手动入口也被同一判据挡住(幂等,不会建出第二张)。

**注入清单**(五层之上):第 0 层 buddy 系统提示词(固定、短,内容见 2.4);第 1 层仓内 `AGENTS.md`(指向先读 PROJECT.md/上周 plan/metrics.toml);第 2 层本活技能 `week-planning`(见下);第 3 层规范第 4 类「指标与数据」件、第 5 类「活的约定」件(`.bw/issue-policy.toml`)、第 7 类「运作节律」件;第 4 层 `PROJECT.md`、上一周 `docs/plan/`(若存在)、`docs/releases.md`、`.bw/metrics.toml`、codegraph 索引。**①独有**:配了项目群时,一份上周群消息摘要本机文件(`FetchChatDigest` 提前生成)——不进仓不进库。

**入口 SKILL.md 大纲**:

> `name: week-planning` · `description: 复盘上周、补齐 .bw/metrics.toml、和人交流出本周目标与业务活草稿` · `category: 运作`
>
> - **何时用**:只在运作活①被开工时用。`.bw/metrics.toml` 不存在(首次接入后第一次跑)则走完整起草而非补齐。
> - **第一步·复盘上周**:读上一周 `docs/plan/` 与 `docs/releases.md`;没有上一周文件就如实说明「首次制定周计划,无历史可复盘」,不假装有数据;只读不改。
> - **第二步·更新指标**:调用子技能 `metrics-refresh`(合并自 north-star-discovery/metrics-binding,见 2.4)——补缺的引领/滞后、给绑不上的找最便宜采集路径、改 `.bw/connectors.toml`、跑一次采集;绝不伪造观测、绝不为点亮改指标定义。改动先不提交,和第四步合成一次提交。
> - **第三步·引导出本周**:结合指标现状、代码现状(codegraph)、上周记录(及群摘要),交流出周目标一句 + 业务活草稿(每张:标题/说明/类别/预期推动的指标/建议工具与 workflow)。**不许自己下结论**——写完草稿必须停下,等待人明确的确认信号才能进第四步。
> - **第四步(需人确认后才能做)·落文件**:写入 `.bw/metrics.toml`/`docs/plan/YYYY-Www.md`/`docs/metrics.md`,正常提交,`gh pr create`(允许)/`gh pr merge`(禁止,与所有 Issue 共用铁律一致),把 PR 地址打屏。
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

**产出**:仓——`.bw/metrics.toml`/`.bw/connectors.toml`(若新增采集)/`docs/metrics.md`/新建 `docs/plan/YYYY-Www.md`;库——运作活①本身一行(`kind='ops'`/`origin='human'`/`workflow='更新指标与周计划'`)+ 每张确认业务活各一行(`origin='agent_split'`,待拍-08)+ `issue_metric`/`week_plan` 索引行;MR——一个,标题「周计划 2026-Www」,挂在运作活①上。

**停在评审中怎么保证**:会话收尾走既有 `finalize_run_interactive`(`issue_run.rs`)→ `open_pr`(`github.rs`,codehub 走 `create_mr`)——暂存+提交+推送+开 PR;若 agent 已自己跑过 `gh pr create`,遇「already exists」诚实认领(`adopt_existing_pr`)不重复开。随后 hook 的 `Stop` 事件触发 `poll_interactive_inreview`,探测到分支有开着的 PR 就推 `InReview`——这是「评审中」的**唯一**来源,`Done` 仍须人手动 `TransitionIssue`。

**失败与边界**:采集失败 → 指标保持灰,原因写进 `docs/metrics.md`;远端 issue 建失败 → 本地行仍建、标「未同步」,不阻塞其余;MR 开不出来 → 停在 `InProgress`;agent 中途断 → 停原地可重试。不做:agent 不许在未确认前建活;不许替人「写」周目标;群消息只做参考不落库不落仓。

**时长与轮次(估计)**:约 20-40 分钟,人机对话 3-6 轮。首次运行(指标从零起草)明显更长,不设固定估计。

---

### 2.2 运作活②「资产盘点与代码微重构」

**触发与判据**:**定时**,默认每周五 20:00(`.bw/issue-policy.toml` 的 `[cadence] ops2_schedule`,可改),`tick_scheduler` 到点自动建 issue(`origin='auto'`)**并自动 ▶开工**——三张里唯一不需要人点一下才开工的一张。前提 buddy 当时在运行;错过不需要额外补建逻辑——`cron_due` 本来就是「到点即算数」,下次启动的第一次 tick 天然补建。

**注入清单**:第 0 层系统提示词、第 1 层 `AGENTS.md`、第 2 层本活技能 `asset-audit`、第 3 层规范第 3 类「目录与知识结构」件+第 6 类「默认件与鱼塘」件、第 4 层 `PROJECT.md`+本周 `docs/plan/`(盘点报告要追加进去)+codegraph 索引。**②排除**项目群摘要——内部盘点不需要「上周群里聊了什么」。

**入口 SKILL.md 大纲**:

> `name: asset-audit` · `description: 盘点文档产物登记、找该清理的死码/超长文件,小范围微重构后提 MR` · `category: 运作`
>
> - **何时用**:只由定时触发,**这次会话很可能无人在场**,不要假设有人立刻回应终端。
> - **第一步·盘点文档**:`docs/plan/`、`docs/releases.md` 是否齐全,新增文档是否登记进知识库资产页。
> - **第二步·盘点默认件**:对照 `.bw/managed.toml` 指纹,检查规范件版本是否落后;只记录差异**不擅自升级**。
> - **第三步·找大文件**:`codegraph files -j` 找超行数上限的文件。**不做「零调用者就当死码删」**——`codegraph` 对 `dyn Trait` 分发看不见,会误判;疑似未使用的只写进报告不动手删。
> - **第四步·小范围微重构**:只做格式/命名/经反复核实安全的死码清理,**范围限定 `docs/`、`.bw/` 与小范围代码**;需要动业务逻辑或公开接口签名的一律只写建议。
> - **第五步·写盘点报告**:追加进本周 `docs/plan/` 尾段;没有可重构的东西是正常结果,如实写「无」不硬造改动。
> - **第六步**:提交+MR,打屏(多半没人看,仍要打)。
> - **DoD**:报告确实追加;若有代码改动,范围可用 `git diff --stat` 核对确实限定在声明范围内。
> - **常见坑**:把「疑似未使用」升级成删除而未反复核实;报告写空话;无人应答就卡住等待——**没人在场按"能做的先做、拿不准写进报告"推进**。

**对话节点表**(与①最大不同:不能假设人在场):

| 时刻 | agent 说什么 | 人做什么 | 不许什么 |
|---|---|---|---|
| 首(定时触发) | 「本周运作」栏出现「已自动开工」 | 通常不在场,随时能点进看 | — |
| 中(自主推进) | 逐步播报盘点结果,拿不准写进报告 | 若在场可插话 | 不许因无人应答卡住不推进 |
| 尾(周一评审) | — | 看报告/diff、合入、完成 | 不属于本次会话 |

**产出**:仓——盘点报告(追加进 plan 尾段)+微重构改动(若有);库——运作活②一行(`origin='auto'`/`workflow='资产盘点与微重构'`)、定时触发记录;MR——一个,通常周一才被看到。

**停在评审中**:与①同一条机制(`finalize_run_interactive` → `open_pr` → hook `Stop` → `poll_interactive_inreview` 推 `InReview`)——自动开工与人工开工用同一条交互式执行器和状态机通路,没有特殊待遇。

**失败与边界**:工作区不可用 → 如实跳过不建活,`cron_run` 记 `Failed`;无东西可重构 → 报告写「无」;agent 中途断 → 停 `InProgress` 可重试。

**时长与轮次(估计)**:约 10-25 分钟,人机对话通常 0 轮。

---

### 2.3 运作活③「规范铺底」——agent 的两步:合并调整、历史回填

三步整体流程(判据、探测逻辑、第 1 步无 agent 的模板写入)属于 [03-standard-and-backfill.md](03-standard-and-backfill.md)(已成稿),本节只写第 2/3 步需要 agent 的部分,消费 03 篇「已探测到需要合并调整」「已探测到仓有历史」两个判据结果,并把 03 篇 §2.3/§2.4 给的任务清单写成本篇统一的剧本格式。

**触发与判据**:人填两卡完成接入 → buddy **自动建**一次性运作活③(`RunStandardBootstrap`,`origin='auto'`——建活本身是 buddy 做的,虽由表单提交间接触发)。第 1 步(写模板)不起 agent,直接由 Rust 代码完成。**仅当探测到已有手写 README/CLAUDE.md/AGENTS.md** 才追加起一次 agent 会话跑「合并调整」;**仅当探测到仓有历史**(提交/标签/远端 issue·MR/CHANGELOG/群)才追加「历史回填」。两者都不需要时运作活③由第 1 步收尾,本节不适用。

**注入清单**:第 0 层系统提示词;第 1 层尚在写入的 `AGENTS.md` 模板草案;第 2 层本活技能 `standard-bootstrap-agent`(入口)+ 按需子技能 `merge-adjust`/`history-backfill`;第 3 层规范第 2 类「agent 工作约定」件+第 3 类「目录与知识结构」件;第 4 层(合并调整独有)已有 README/CLAUDE.md/AGENTS.md 原文,(历史回填独有)buddy 预先算好的本机 evidence 文件+仓内 README/CHANGELOG/RELEASES 原文——**不含项目群历史**:03 篇 §4 已明确群历史只喂给运作活①生成本机摘要,和历史回填这条产线不搭界。

**入口 SKILL.md 大纲**:

> `name: standard-bootstrap-agent` · `description: 规范铺底里需要 agent 的部分——合并已有文件、回填老项目历史`
>
> - **何时用**:只由 `RunStandardBootstrap` 在探测为真时调用。第 1 步已在同一分支提交过,本技能继续提交,**最终只开一个 MR**。
> - 探测到已有手写文件 → 调用子技能 `merge-adjust`。
> - 探测到仓有历史 → 调用子技能 `history-backfill`。两者都命中时**先合并调整,再历史回填**(回填要读的仓内文档可能含合并后的 AGENTS.md)。
> - **DoD**:两个子技能各自 DoD 都满足;改动在同一分支提交历史里,最终一次开 PR(或被第 1 步的直接 MR 收编,见 §3)。

> `name: merge-adjust` · **合并调整**(任务清单抄自 03 篇 §2.3)——①读现有 `AGENTS.md`;没有就看 `CLAUDE.md`,再没有就看 `README.md` 里类似"开发约定"的章节。②**合并原则,不是拼接不是覆盖**:已有内容一字不删、一段不改;buddy 固定章节(读什么/活怎么做/指标怎么碰/禁止事项/代码图怎么用)插在**靠前位置**(标题之后、原文之前),不追到文件末尾——很多 agent 工具按上下文预算截断长文件,强约束必须优先被读到;标题字面撞车时不覆盖原标题,buddy 版本标题后缀"(buddy 补充)"插入,MR 说明里提醒人核对;项目自定义段(模板第 8 段)原样保留。③`CLAUDE.md` 单独处理:不存在只写一行 `@AGENTS.md`;空壳/纯导入行换成标准写法;有实质内容就在最前插入导入行+分隔说明,原内容后移不删。④agent 不判断"合并得对不对"(人评审的事),只把每个文件按哪种情况处理的写进 MR 说明草稿。**DoD**:原有内容一字未删、七类固定内容都在。**常见坑**:把"合并"做成"覆盖";把原有历史说明当冲突强行改写;追加到文件末尾而非靠前插入。

> `name: history-backfill` · **历史回填**——**三层流水线**(架构抄自 03 篇 §2.4,agent 只担第 2 层):第 1 层 buddy 主机代码在起 agent **之前**把能算的都算完,写成本机 evidence 文件(不进仓不进库,用完即弃,同"上周群摘要"待遇)——git 本地(提交总数、首末提交日、作者分布、标签、双亲结构判定的合入记录、近 8-10 个 ISO 周提交数/合入数/目录 Top3)、远端(open+closed issue、merged PR/MR,探不通就只留本地部分,不阻塞)。**agent 不跑 git/gh 命令、不自己数数**,任务说明写明「数字照抄 evidence,不一致以脚本为准」;agent 只做:①从 README 首段填 PROJECT.md「想做什么」(仅当原字段待填);②尽力解析 CHANGELOG/RELEASES,不出来的留空;③把 evidence 数字原样填进产物表格;④把仓里已有的量整理成指标候选,标"候选,不绑定"。第 3 层"人确认"不归 agent——北极星、对标、「在研版本」起点这类推不出来的字段原样留空等人。落产物(位置抄 03 篇五项表):`docs/releases.md`「历史运作(回填)」节、`docs/plan/history.md`(新文件,顶部一行"累计贡献者:N 位")、PROJECT.md 草稿补空字段、指标候选(**写进 MR 说明或 `docs/metrics.md` 候选小节,不直接写 `.bw/metrics.toml`**)、`issue` 行(`origin='backfill'`)。**防伪**:合入记录按双亲结构不按文字匹配;远端 MR/PR 计数与本地合并提交计数分开报;无标签无 CHANGELOG 就写"未发现",不拿 commit 日期倒推版本号;批量关闭不当活跃信号,只提示不改数字。**DoD**:每个数字对应 evidence 里的复算命令、没数据是诚实的空、`origin=backfill` 照远端映射。**常见坑**:agent 自己跑 git 命令而不读 evidence;回填候选直接写进正式定义;把回填数据当可点灯的真实观测。

**对话节点表**:

| 时刻 | agent 说什么 | 人做什么 | 不许什么 |
|---|---|---|---|
| 首(自动触发) | 总览显示"规范铺底进行中" | 通常不在场 | — |
| 中(合并/回填播报) | 「检测到已有 AGENTS.md,正在合并……」;远端未认证如实说明 | 可旁观 | 不许删除原有说明、不许编造远端数据 |
| 尾(评审) | — | 一次性看完整个 MR、合入、完成 | — |

**产出**:仓——合并后的 AGENTS.md/CLAUDE.md/README(若适用)、`docs/releases.md` 历史段、`docs/plan/history.md`、PROJECT.md 草稿更新、`docs/metrics.md` 回填候选小节;库——远端 issue 同步行(`origin='backfill'`,状态照远端,不进战绩,见 §3.4)、`release` 行(`origin='backfill'`);MR——一个,覆盖模板+合并调整+历史回填全部。

**停在评审中怎么保证**:**先有一个例外**——探测为"空仓/buddy 自己的仓"(`probe.owned`,03 篇 §2.2)时,第 1 步直接在当前分支提交+推送,不开 PR,走「确认完成(人裁)」既有路径,不适用"停在评审中"这条(这类仓通常也不会命中第 2/3 步的探测条件)。其余情况:第 1 步文件先提交在 `bw/issue-<n>` 分支上**先不开 PR**;若命中第 2/3 步,agent 在同一分支继续提交,全部写完后由 `open_bootstrap_pr`(§3,若无 agent 步骤)或会话收尾的 `finalize_run_interactive` → `open_pr`(若有 agent 步骤)一次性开 PR,遇「already exists」→ `adopt_existing_pr` 认领不重复开——复用既有「提 PR 幂等」设计,避免评审者收到"先看一半"的两次通知。之后同样靠 hook/轮询推 `InReview`。

**失败与边界**:仓太大(万级提交)→ 只精细统计最近 N 周,更早给累计数字;CHANGELOG 解析不出来 → 留空;无 tag 无 CHANGELOG(如 buddy 自己的仓)→ 历史段照实写「未发现」;agent 中途断 → 停 `InProgress` 可重试。

**时长与轮次(估计)**:差异大。纯合并调整约 10-20 分钟;历史回填小仓约 10-15 分钟,大仓(万级提交)可能明显更长(legacy-backfill.md §6 已提示按周统计可能跑到分钟级),具体数字留试点验证。人机对话通常 0 轮。

---

### 2.4 三张运作 workflow 住哪、版本、与两个现有技能的合并、系统提示词要不要加一句

**放哪**:仓根 `standard/` 目录(01 篇 §2.9 已定),规范第 6 类「默认件与鱼塘」:

```
standard/06-defaults/ops/
├── README.md                     # 三张运作 workflow 总说明 + 指向 CHANGELOG
├── week-planning/
│   ├── SKILL.md                  # 入口①
│   └── skills/metrics-refresh/SKILL.md  # 子技能,合并自 north-star-discovery+metrics-binding
├── asset-audit/SKILL.md          # 入口②
└── standard-bootstrap-agent/
    ├── SKILL.md                  # 入口③(需要 agent 的部分)
    ├── merge-adjust/SKILL.md
    └── history-backfill/SKILL.md
```

铺底(运作活③第 1 步)时把整棵目录物化进项目仓的 `.claude/skills/`,带 `.bw-managed` 标记,与规范第 6 类既有约定一致。

**版本**:不单开版本线——版本号就是 `standard/VERSION`,随 buddy 整体发布走。内容改了 = `standard/CHANGELOG.md` 记一行 + 规范整体版本 +0.1(与 [standard-module-draft.md](../standard-module-draft.md) §3 一致)。项目侧 `.bw/managed.toml` 记这几份 SKILL.md 的指纹,对账时能测出落后并提示升级,不需要特例。

**与 `north-star-discovery`/`metrics-binding` 的合并**:两份技能今天独立存在于 `docs/skills/`(编译进 `bw_library.rs`),挂在"标配 Issue 三件套"(竞品分析→找指标→绑数据)触发链上。母文档第 0 站「不带」清单已写明"找指标+绑数据并入运作活①",这条独立触发链在 V4 不再存在。本篇判断:**内容合并进 `week-planning/skills/metrics-refresh/SKILL.md`,原两份文件退役**(按 CLAUDE.md「发现过时的实现路径,直接移除它」,不留并行旧链当兼容层)。

- **原样沿用**:north-star-discovery 的三段拆解、虚荣指标黑名单、自动化免疫检验、北极星判据打分 6 项、反指标机制、BDFE 输入结构、NSM↔商业 KPI 校验;metrics-binding 的硬性约束(绝不伪造、绝不为点亮改定义)、按 `collect.kind` 分支的诊断表、`script` kind 搭装置流程。
- **改写语境**:两份原文的"前置条件"是"标配 Issue「找指标/绑数据」",在 V4 里不存在,改成"运作活①第二步";新增判断——`.bw/metrics.toml` 已有完整三层指标时只做**增量校准**,只有首次跑运作活①(文件不存在)才走完整起草流程。
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
| ②资产盘点与代码微重构 | `ops` | `auto` | `claude_cli` | `资产盘点与微重构` |
| ③规范铺底 | `ops` | `auto` | `claude_cli`(仅追加 agent 步骤时才有一次交互式会话)| `规范铺底` |

① 引导出的每张业务活各自是独立 `issue` 行(`kind='business'`,`origin='agent_split'`,待拍-08);③ 探测到远端已有 issue 回填成的行 `origin='backfill'`,与运作活③本身这张 `kind='ops'` 行是两回事。

### 3.2 触发命令(伪码,未拍板)

```rust
// ①判据复用 week_plan 索引表(02 篇 2.6):命中即拒绝,幂等,不建出第二张本周①
pub async fn start_week_planning(&mut self, project_id: ProjectId) -> Result<IssueId, AppError> {
    let week_of = current_iso_week();
    if self.store.week_plan_exists(project_id, &week_of).await? {
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

// ②tick_scheduler 新分支:复用 CronMode::CreateIssue 的 autopilot_fire,只加一行——
// 建完立刻当作"已▶开工"分发,不另开一条"无人值守执行器"
if c.mode == CronMode::CreateIssue && c.auto_run {
    let issue_id = self.autopilot_fire(pid, &c.name, stage, c.issue_assignee.as_deref(), now_ts).await?;
    self.store.set_issue_kind_origin(issue_id, IssueKind::Ops, IssueOrigin::Auto).await?;
    self.store.set_issue_workflow(issue_id, "资产盘点与微重构").await?;
    self.dispatch(Command::RunIssue { id: issue_id, session: None }).await?;
    self.emit(Event::OpsWorkflowAutoFired { id: c.id, issue_id, ok: true });
}

// ③第 1 步无 agent,buddy 直接写模板;是否追加 agent 步骤由探测结果分三支。
// 函数名与探测结构体照抄 03 篇 §3.1/§3.2(probe/BootstrapProbe 已在那边定义,
// 本篇不重复设计、不再自造 write_standard_template/detect_existing_docs 这类名字)。
pub(crate) async fn run_standard_bootstrap(&mut self, p: ProjectId) -> Result<(), AppError> {
    let probe = bw_engine::standard_bootstrap::probe(&workspace).await;
    let issue_id = self.create_ops_issue(p, &title_for(&probe), &body_for(&probe)).await?;
    self.write_standard_core_files(p, &probe).await?; // 第1步,同步,不起 agent
    if probe.owned {
        // 空仓例外:直推,走"确认完成(人裁)"既有路径,不开 PR(03 篇 §3.2)
    } else if probe.has_agent_docs || probe.has_history {
        // 有 agent 步骤(合并调整/历史回填其一或两者):复用运作活②同一条
        // "自动▶开工"能力(auto_start_run,底层即 01 篇 CreateAutopilotTask{auto_run}),
        // 起交互式会话跑 standard-bootstrap-agent,条件调用两个子技能(见 2.3);
        // 收尾走既有 finalize_run_interactive → open_pr,"already exists" 时诚实认领。
        self.auto_start_run(issue_id).await?;
    } else {
        // 两条探测都为假:没有 agent 步骤,buddy 直接开 PR——评审中靠下次
        // 兜底轮询探测到(没有 hook Stop 事件可触发)
        self.open_bootstrap_pr(p, issue_id).await?;
    }
    Ok(())
}
```

### 3.3 活草稿 → 真建活(①第四步)

人在终端确认草稿后两条动作并行(细节留实现时与计划屏交互一并定稿):①agent 继续在自己的会话里提交 `.bw/metrics.toml`/`docs/plan/`、开 PR(2.1 第四步);②buddy(不经过 agent)批量调既有 `Command::CreateIssue` 为每张确认的业务活各建一行(含远端 issue 创建),`week_of`/`version`/`tool` 创建后一并写入,推动指标各插一行 `issue_metric` 关联表(02 篇 §2.2)。两条动作各自失败互不阻塞,失败表现按 §4 处理。

### 3.4 战绩记账:以 02 篇 `workflow_credit` 台账表为事实源(设计期统一)

**两篇口径分歧已收敛**:02 篇 §2.4 给的独立台账表 `workflow_credit`(`subject_kind`/`subject_id`/`issue_id`,`UNIQUE` 三元组防重、一活一主体一行)与 04 篇早先版本「列在 `skill_package`/`skill` 表本身」的 `runs`/`wins`/`win_rate` 三列曾经并存,现已统一:以 02 篇 `workflow_credit` 为准,04 篇 §2.9 已同步改成「配置屏读数由 SQL 从 `workflow_credit` 现算」,不落缓存列。本篇按此写。

记账口径(照 04 篇 §2.4「就近优先」解析规则 + §2.9 挂载点):

- 挂载点不变,复用既有「同一件活绝不记两次」的判定——`dispatch.rs` 的 `TransitionIssue` Done 边(`newly_done`)与 run 失败两处(`finalize_run_interactive`)。
- 记账主体从"按 `issue.assignee` 找 agent"换成"按 `issue.workflow` 名字解析出 `skill_package` 或裸 `skill` 行"(找不到就如实记「名字对不上,不记账」,不错记到别的行),向 `workflow_credit` 插入一行(`subject_kind`='workflow'|'skill')。
- ①「更新指标与周计划」记在同名 `skill_package` 对应的一行 `workflow_credit`(`subject_kind='workflow'`)上;③「规范铺底」用到的 `merge-adjust`/`history-backfill` 若各自是独立技能(未打包)则各自记一行(`subject_kind='skill'`),若归在 `standard-bootstrap-agent` 同一个包里则整包记一行(04 篇 §2.9:「包被物化时成员技能 `uses` 各自 +1,但只有包本身在 `workflow_credit` 上记一行」)。
- 回填出的业务 issue(`origin='backfill'`)Done 边直接跳过记账(04 篇 §2.9「回填的活不进战绩」);运作活③自己(`kind='ops'`)是真实 agent 会话跑出来的,正常记账。

---

## 4 · 边界与失败

**不做什么**:

- **自动完成**——三张活不管触发方式,最远只能到「评审中」,`Done` 永远需人手动 `TransitionIssue`/`MergeIssuePr`。
- **运作活②对业务代码大改**——范围硬限定 `docs/`、`.bw/` 和"小范围"代码重构,需动业务逻辑或公开接口签名的一律只写建议不动手。
- **运作活①替人写周目标、不确认就建活**——第三步做完必须停下等确认,第四步只在收到确认后才执行。
- **群消息进库**——①的上周摘要是本机文件参考,读完即用,不落库不落仓,与健康信号无关;③探测第 3 步要不要跑时,"项目群已配置"只是五个判据之一(03 篇 §2.1),群历史本身不作为回填内容的输入源(03 篇 §4 已排除)。

**失败如实(汇总,逐活细节见 §2)**:

| 场景 | 表现 |
|---|---|
| 采集脚本失败(①)| 对应指标保持灰,`docs/metrics.md` 写明原因 |
| 远端 issue 建失败(①确认建活)| 本地行仍建、标"未同步",不阻塞其余 |
| 定时触发但工作区不可用(②)| 如实跳过不建活,`cron_run` 记 `Failed` |
| 远端未认证(③历史回填)| 只完成 git 本地部分,远端字段留空 |
| agent 中途断(通用)| 停 `InProgress`,`settled_at` 留空,可重试 |
| MR 开不出来(通用)| 停原状态,不假装到了「评审中」|

---

## 5 · 验收与读回

三张活目前没有专用 headless 指挥器(`real_demo` 走业务活主环,不覆盖 `kind='ops'` 支线)。建议给 `real_demo` 加一个可选场景(或另建 `ops_loop_demo`,命名留实现时定,不新开代号),用 `MockInteractiveExecutor` 依次走①→②→③,逐步 SQL 读回。

| 核验什么 | 命令/SQL | 预期 |
|---|---|---|
| ①判据生效 | 连续调用两次 `StartWeekPlanning`(mock)| 第二次返回 `WeekPlanAlreadyExists` |
| ①产出文件 | `test -f <workspace>/docs/plan/<本周>.md && echo ok` | `ok` |
| ①业务活挂 workflow | `sqlite3 <db> "SELECT kind,origin,workflow,week_of FROM issue WHERE project_id='<pid>' AND kind='ops' AND workflow='更新指标与周计划';"` | 一行,`origin='human'` |
| ②定时自动建+自动开工 | 手动调 `tick_scheduler`(时间戳设到 `ops2_schedule` 之后)| 新建 issue id;`sqlite3 <db> "SELECT status FROM issue WHERE id='<id>';"` 不是 `Todo` |
| ②盘点报告落地 | `grep "运作活②盘点尾段" <workspace>/docs/plan/<本周>.md` | 命中 |
| ③三张活最远评审中 | `sqlite3 <db> "SELECT workflow,status,settled_at FROM issue WHERE project_id='<pid>' AND kind='ops';"` | 三行,`status` 落在 `InReview`(或更早的失败态),`settled_at IS NULL` |
| Done 只能人点 | 跑完①②③,不手动调 `TransitionIssue` | `sqlite3 <db> "SELECT COUNT(*) FROM issue WHERE kind='ops' AND settled_at IS NOT NULL;"` 为 `0`,直到显式调用才变化 |
| ③回填不进战绩 | 回填前后各查一次 `sqlite3 <db> "SELECT COUNT(*) FROM workflow_credit WHERE subject_kind='workflow' AND subject_id=(SELECT id FROM skill_package WHERE name='规范铺底');"` | 两次数字相同——插入 `origin='backfill'` 的 issue 行本身不触发 §3.4 的记账挂载点 |
| 深链截图 | `BW_OPEN=<项目名> BW_PANEL=session BW_SEL=issue:<id>` | stderr `[BW_OPEN]` 日志 + 截图存进 `docs/v4-prototype/` |

---

## 6 · 开放问题(≤5)

1. **母文档与 03 篇在"群历史算不算回填原料"上不一致,不是本篇能填的空**——母文档 §2.6 把群历史列为回填四种原料之一,但 03 篇 §4 已明确写"群历史不进仓不进库——只用于给运作活①生成本机摘要,和历史回填这条产线不搭界",[legacy-backfill.md](../research/legacy-backfill.md) 也早把它划出预研范围。本篇 2.3 按 03 篇写(回填不碰群历史),但母文档这处措辞需要用户确认要不要一并更新,避免下次有人照母文档字面另起一条产线。
2. ~~02 篇 `workflow_credit` 表与 04 篇 `skill_package`/`skill` 战绩列口径不一致~~ **已定(设计期统一)**:以 02 篇 `workflow_credit` 台账表为事实源,04 篇已同步改为「配置屏读数由 SQL 现算,不存 runs/wins/win_rate 列」,见 §3.4。
3. **`north-star-discovery`/`metrics-binding` 旧文件的迁移时机**——同一次改动删除还是先并存一个版本周期,按「不为向后兼容留旧路径」倾向前者,落地顺序留实现时的 commit 拆分决定。
4. **系统提示词是否真要加 2.4 建议的那一句**——具体措辞是否合适、是否采纳,需用户确认。
5. **运作活③纯模板路径(无 agent)时"评审中"怎么被探测到**——§3.2 提到这种情况没有 hook `Stop` 事件,只能靠既有 5 分钟兜底轮询。这条路径此前只服务"project-init"特殊场景,V4 里第一次成为常规路径,轮询节律要不要为此加速,留待实现时评估。
