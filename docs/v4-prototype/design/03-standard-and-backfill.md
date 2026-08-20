# 03 · 规范铺底怎么跑:铺 `.bw/`、老项目回填、对账、升级

> **30 秒导读**:这篇管运作活③「规范铺底」——项目接入时自动出现的一次性活——具体怎么跑:第 1 步 buddy 自己写模板核心件(**全落在 `.bw/` 下,仓根一个字不写**),**第 2 步已经取消**(见 §2.3:给别人的仓写开发手册是「建议改造你的项目」,归资产盘点去问人)、第 3 步(有历史的仓才有)把老项目自己的历史记录回填成 buddy 认得的文件;外加铺完之后的日常动作:对账、升级、`standard/` 正本怎么让同事贡献。**历史回填不是这里养的一个独立技能**,它是运作活②「资产盘点」的首次模式(同一个 `asset-audit` 包,`mode=first` 全量、`mode=weekly` 增量),剧本归 [09-ops-workflows.md](09-ops-workflows.md) 管,本篇只管「探测到历史该不该跑、原料与产物长什么样、怎么防伪」。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 给三种人看:接着做 V4 的会话、往 `standard/` 提 PR 的同事、要核对铺底行为的人。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

---

## 0 · 这篇管什么、不管什么

**管**:①探测——两卡填完后怎么判断运作活③跑几步(空仓例外这一项 2026-08-20 已取消,见 §2.1);②第 1 步「写核心件」全流程;③第 2 步为什么被取消、它去哪了(§2.3);④第 3 步「历史回填」全设计——原料分给脚本/agent/人、四份仓文件产出物字段(+ 一行库缓存例外)、防伪规则、幂等、限时/抽样、缺的函数清单;⑤三步合成一个 MR、人怎么评审;⑥对账与升级流程;⑦`standard/` 正本结构、版本号、同事怎么贡献;⑧本篇命令/事件名字。

**不管**:`standard/` 放哪、怎么进二进制、版本常量叫什么——[01-architecture.md](01-architecture.md) §2.9 已定,本篇直接引用。`.bw/*.toml` 与仓文件**格式**——[02-data-and-files.md](02-data-and-files.md) 已给,本篇只补留白处(`.bw/managed.toml` 指纹算法、回填历史周文件里"累计贡献者"字段落在哪一份文件)。「写开发手册」(已并入资产盘点,技能名 `project-handbook`)「历史回填」两个技能的 **SKILL.md 正文剧本**——[09-ops-workflows.md](09-ops-workflows.md)管,本篇只给任务清单。计划屏周列表 / 总览发版记录怎么把回填的历史周、历史版本渲染出来(不另开专门块,只带徽记)——[06-plan-screen.md](06-plan-screen.md)/[08-overview-derivation.md](08-overview-derivation.md) 已给,本篇只保证产出的形状(同格式的历史周文件、`.bw/releases.md` 历史版本行)能被它们直接消费。开工工具注册、workflow 识别——[04-tools-and-workflows.md](04-tools-and-workflows.md)管。

---

## 1 · 用户看到什么、做什么

**新项目(空仓)**:填完接入两卡点完成的那一刻,`onboard` 屏不用等——运作活③一两秒内跑完第 1 步。核心件落在这张活自己的分支 `bw/issue-<号>` 上(2026-08-20 起**不再有"空仓直推默认分支"这条例外**,新仓老仓走同一条路),活停在「评审中」。挂了远端就有一个 MR 可以点开看;没挂远端就是一条本机分支,人自己 merge 再点完成。合上之后总览的项目信息、仓里的 `.bw/PROJECT.md`/`AGENTS.md`(仓根)/`.bw/*` 骨架就都到位了。

**接入已有仓(成熟项目)**:总览「待人处理」很快多一条「规范铺底 v{当前规范版本}」,**要跑哪几步写在这张活的正文里,不写进标题**——标题是幂等键,跟着探测结果变就会重复建活(§2.1)。会话屏能看到真实 Claude CLI 会话在这个活的 worktree 里跑,完事推到「评审中」,MR 里能看到:`.bw/` 骨架文件(**仓根一个字不动**,§2.3)、`.bw/plan/` 下新增的若干份**历史周文件**(`origin: backfill`,有历史才有,和运作活①写的周计划同一模板)、`.bw/releases.md` 多出的历史版本行、PROJECT.md 补的草稿字段。人看一眼评审要点(2.5 节)就能合入、点完成,有权限时「合入并完成」一键。合入后总览立刻提示「本周还没有计划 → 开始本周」;计划屏左栏周列表能直接看到这些历史周(带回填徽记),总览发版记录也能看到刚回填的历史版本——**不出现任何专门的"回填"区块**,历史资产就混在正常列表里(用户拍板,见 §2.4)。

**平时**:知识库屏顶部有一条不打扰的小字提示——对账发现「缺 2 项 / 过期 1 项 / 你改过 1 项」,点开看具体文件、要不要升级。不是弹窗;运作活②每周跑时顺带算一遍,写进那周 `.bw/plan/` 尾段。

---

## 2 · 设计

**铺底今天只有第 1 步,而且这就是全部了 —— 第 2 步已经取消(§2.3)。** 已落地的是**第 1 步「写核心件」**(`crates/bw-v4/src/standard/bootstrap.rs::write_core_files`,§2.2 描述的落盘规则与其对应)。第 3 步「历史回填」的 agent 那一半要起一次真实会话去读文本、写产出,**还没做**——`run_standard_bootstrap`(`crates/bw-v4/src/app/bootstrap.rs`)探测到仓需要跑这两步时,只把「还没跑的步骤」如实写进这张活的说明段落,不假装跑过。下面 §2.2 是已经跑通的部分;§2.3/§2.4 是设计,阅读时按此对待。

### 2.1 触发与探测:这张活要跑几步

两卡填完、项目行写入库、工作区就绪后,`RunStandardBootstrap` 一次创建活并立即执行第 1 步。探测不问用户,现算,复用能复用的既有函数:

| 探测项 | 怎么判 | 函数(现成/新增) |
|---|---|---|
| ~~空仓例外~~ | ~~根提交作者是不是「Builders' Workbench」~~ | **2026-08-20 取消**。原本想用 `workspace::is_owned_workspace()` 决定「直推默认分支还是走 MR」,现在新仓老仓一律走 worktree + 分支 + MR,这个判断没有用武之地,`bw-v4` 里一次都没调过 |
| ~~触发第 2 步~~ | 已取消(§2.3)。`probe.has_own_conventions` 仍在探,但只写进活的正文当证据,不触发任何一步 | 已实现的 `standard_bootstrap.rs`,不需要新模块 |
| 触发第 3 步 | 以下任一为真:①`commit_count`>1(1 是 buddy 自己的 scaffold 提交);②`git tag -l` 非空(新增);③仓根有 CHANGELOG/RELEASES;④远端有已关闭 issue 或已合并 MR(新增);⑤`.bw/project.toml` 的 `[chat]` 段已配置 | 见 2.4.4 |
| 后来者接入 | `.bw/standard.toml` 已存在 | 复用 `project_file.rs` 同款 `Ok(None)` 惯例,读到就说明有人先接过——不新建活,读正本预填(和 `.bw/project.toml` 后来者逻辑对齐,V2 已有先例) |

**「触发第 3 步」的探测依据设计了五条,今天只实现了前三条**:`crates/bw-v4/src/standard/bootstrap.rs::probe` 判 `has_history` 只看 `commits > 1 || !tags.is_empty || has_changelog`(提交数、`git tag -l`、仓根 CHANGELOG/RELEASES/CHANGELOG.md 三选一)。「④远端有已关闭 issue 或已合并 MR」与「⑤`.bw/project.toml` 的 `[chat]` 段已配置」这两条**没有接**——不是判了否,是代码里根本没有这两条查询。设计仍然保留五条(远端信号、群配置都是判断「这个项目是不是已经在正经跑」的合理依据),今天只做了本地 git 能现算的三条。

探测结果一次性写死进 Issue 标题与说明(不随后续改动变化,是"打算跑什么"的快照):标题**固定就是**「规范铺底 v{STANDARD_VERSION}」,**命中了哪几步不拼进标题**——标题是幂等键,第一次铺底自己写了 `CLAUDE.md`,第二次探测结论就变了,标题跟着变则幂等失效、重跑多建一张活(踩过,`crates/bw-v4/src/app/bootstrap.rs` 里有这条注释)。要跑哪几步写在说明段落里,连同证据一起,如「仓有 71 条提交、0 个标签、已发现 AGENTS.md,将执行:写核心件 → 写开发手册」——这是 CLAUDE.md「报告不代答」纪律的落点,评审者不用猜这张活为什么跑了这几步。`kind='ops'`、`origin='auto'`(和运作活②同一档)、`tool='claude_cli'`(仅第 2/3 步用到)。

### 2.2 第 1 步 · 写核心件(buddy 自己写,不起 agent)

同步主机代码逻辑,不经交互式执行器,和今天 `write_charter`/`write_component_standards` 同一技术形状,范围从「章程+四份组件标准文件」扩到规范全部核心件,并套一层分支/MR:

1. 读 `.bw/standard.toml` 的 `enabled` 清单(新仓没有就取二进制默认核心清单:`charter, agents, docs-core, metrics, issue-policy, defaults-core, cadence`,同 02 篇 §2.8 样例)。
2. 按清单逐类从 `standard_assets` 取模板渲染到目标路径:
   - 第 1 类章程 → `.bw/PROJECT.md`(复用现成 `charter_md()`,填两卡内容,不是静态模板——从第一天起就"有数据");
   - 第 2 类 → **不写**(2026-08-20 取消,§2.3):仓根 `AGENTS.md` / `CLAUDE.md` 是项目自己的文件,归资产盘点去问人;模板已从仓里删除;
   - 第 3 类 → `.bw/releases.md`(空表头)、`.bw/design/README.md`(约定说明);`.bw/plan/YYYY-Www.md` 不在这一步写,那是运作活①第一次跑才有的东西;
   - 第 4 类 → `.bw/metrics.toml`/`.bw/connectors.toml`/`.bw/collect_stats.*`/`docs/metrics.md`:创建流已写过的不重写内容,只登记指纹;没有的写空骨架;
   - 第 5 类 → `.bw/issue-policy.toml`(02 篇 §2.8 给的默认三列映射 + review/cadence/kanban 四段);
   - 第 6 类 → **不复制任何技能进项目仓**(2026-08-20 试点第一天推翻,原方案是「复制预置技能包进 `.claude/skills/`」)。buddy 自带的十三份(九篇方法论 + 四份运作剧本)住在 buddy 自己的资产目录,开工时只把这张活挂的那一份的名字、一句话、完整路径写进系统提示词,正文让 agent 按需读——理由与新机制见 [04 篇](04-tools-and-workflows.md) §2.7。当时撑着旧方案的那句「Claude CLI 只在项目仓里找技能,不复制进来就读不到」**已被证伪**:给绝对路径 + `--add-dir` 一样读得到。业界包(mattpocock-skills、superpowers)仍未接入,`.bw/issue-policy.toml` 的 `workflow` 列写着它们的名字是设计目标,配置屏如实标「不在册」,见 `docs/LEFTOVERS.md`;
   - 第 8 类 → 最后写 `.bw/standard.toml`(`version = STANDARD_VERSION`)与 `.bw/managed.toml`。
3. 每写一个文件同时往 `.bw/managed.toml` 追加一条(`path`+`version`+`fingerprint`,算法见 2.6);这份清单**最后写**,保证记的指纹是刚落盘那一刻的真实内容。
4. 人手改过不覆盖:只在**目标路径不存在**或**存在但指纹与记录一致**时才写;两者都不满足就跳过,在 Issue 说明追加一行「`XXX.md` 已存在且非 buddy 管理,跳过」——和第 2 步"不覆盖已有 AGENTS.md"是同一精神在不同文件上的应用。
5. **落盘方式(2026-08-20 重写,已实现)。** 不再看是不是空仓例外——两种情况走同一段代码:先给这张活开一棵自己的 git worktree(主仓的兄弟目录 `<仓名>-issue-<号>`,分支 `bw/issue-<号>`,供给复用 `bw_engine::workspace::provision_issue_worktree`,和 V3 同一份实现),核心件写进这棵树,`commit_paths` 只 add 这次真写下去的路径(不用 `add -A`,免得把人手上没写完的改动一起打包),提交信息 `docs(bw): 规范铺底 v{版本} · 核心件`。**人的主检出一个字节都不碰**,两张活同时在跑也不会撞在一起。工作区不是 git 仓时开不了 worktree,就地写,并在活的正文里如实标出来。

跑完:真提交出东西了就推分支、开 MR、把活推到「评审中」等人合(§2.5);挂着远端才有 MR,没挂远端就只有一条本机分支,活照样进「评审中」,人自己 merge 那条分支再点完成。需要第 2/3 步 → 这两步还没实现(见「## 2·设计」开头的范围说明),设计上是紧接着自动触发一次交互式运行、**同一棵 worktree** 继续跑,但目前只把「还没跑的步骤」写进活的说明,不会真的继续跑下去。

### 2.3 没有第 2 步了 —— 2026-08-20 傍晚整节重写(第二次)

**规范铺底只有一步:把 buddy 自己的资产写进 `.bw/`。仓根一个字不写。**

这一节今天被推翻了两次,记在这里免得再来第三次。

| 时间 | 这一步是什么 | 为什么不对 |
|---|---|---|
| 原设计 | 「合并调整」:把 buddy 的固定章节合并进已有 `AGENTS.md` | 那些章节全是 buddy 自己的运作规矩,今天已由系统提示词每场会话注入,往用户仓再写一份就是第二个会漂移的副本 |
| 08-20 下午 | 「写开发手册」:仓根 `AGENTS.md` 改写成这个项目自己的开发手册,铺底时铺 | 落点对了,**时机不对**(见下) |
| 08-20 傍晚 | **取消**。铺底不碰仓根;写手册归资产盘点 | —— |

**为什么时机不对**:仓根 `AGENTS.md` / `CLAUDE.md` 是**这个项目自己的**文件。
给别人的仓写一份开发手册,性质是「**建议改造你的项目**」,不是「铺 buddy 自己的
东西」。而接项目那一刻 buddy 才刚 clone 完:既没读过这个仓,也没人问过人家
愿不愿意。

代码里当时那半步自己就把这件事说破了:

- 成熟仓(常见情况)仓根已经有 `AGENTS.md`,按「人手改过的不覆盖」**整份跳过**
  —— 该起作用的地方一个字也没写;
- 空仓那边**写得出来**,但只能写成一张七成是「(还没填)」的表 —— 那时候仓里
  还没有目录可导览、没有规矩可总结。

**一个功能在该起作用的地方是空转,在不该起作用的地方产出占位符**,那就不是时机
问题的边角,是这一步本身放错了地方。

**它去哪了**:资产盘点(运作活②)的**首次模式**,和历史回填同一次会话。理由是
那次会话本来就在干「读一遍这个仓、看它有什么、缺什么」,而且它是一次**真的
agent 会话**,能做铺底做不到的两件事——**真读懂这个仓**,以及**先问人愿不愿意**。
剧本 `project-handbook` 在
`standard/06-defaults/ops/asset-audit/skills/project-handbook/SKILL.md`,写法见
[09 篇](09-ops-workflows.md) §2.2;人说不要就零改动,只留一句回执。

**规范第 2 类「agent 工作约定」的模板已经删了**(`standard/02-agents/` 整个目录
不在了),`crates/bw-v4/src/standard/detect.rs`(从仓里探构建命令与目录列表)也
删了 —— 它唯一的消费者就是那半步。要探,是那次会话自己去读,agent 读得比
「看文件在不在」细得多。本仓规矩:过时的实现路径直接移除,不留兼容层。

**铺底的探测还留着一条相关的**:`probe.has_own_conventions`(仓根有没有
README / CLAUDE.md / AGENTS.md)。它**不触发铺底的任何一步**,只写进这张活的
正文当证据,顺带留给将来那次盘点当输入。活的正文里会如实写上「提议给这个项目
写一份开发手册 —— 那是在改你的项目,得先问过你才写,不在接项目这一步做」。

### 2.4 第 3 步 · 历史回填(采纳预研,仅有历史的仓;用户定性:即运作活②「资产盘点」workflow 的首次模式)

**与运作活②的关系(用户点破,待拍-05/27 改)**:老项目历史回填不是一条独立产线,它就是「资产盘点」这个 workflow 第一次跑——同一个 workflow 包,`mode=first` 时多产出本节说的四项回填件,`mode=weekly`(每周)只盘变化、不产出这些历史件。触发时机不变:铺底探测到仓有历史(§2.1)时,运作活③在同一张活的分支上另起一次会话跑这个 workflow(SKILL.md 剧本见 09 篇 §2.2);原料仍是三种——①git 本地历史、②仓内已有文档、③远端 issue 与 MR 列表,**群历史不算原料**(与母文档、09 篇口径一致,见下)。

**产出形态**(用户拍板):不再是一份独立的"历史运作(回填)"新文件,而是补出"本该有但老项目没攒出来"的**同格式**正常文件**——回填探测到某个历史 ISO 周有过合入或提交、但当时没有 `.bw/plan/YYYY-Www.md`,就按运作活①写周计划**同一套模板**给它补一份,front matter 标 `origin: backfill`;回填探测到某个历史版本(git tag/CHANGELOG)但 `.bw/releases.md` 里没有对应行,就按同一张表格式补一行,「来源」列标"回填"。用户原话:「期望资产盘点发现老项目没有周计划 md / 发版本 md,就把这些回溯补起来,总览和计划 UI 不需要特殊处理,本身就是对照资产渲染」——落地就是:计划屏左栏周列表天然会多出这些历史周(06 篇),总览发版记录天然会多出这些历史版本行(08 篇),**不建 `.bw/plan/history.md`,界面不为回填开辟任何专门区块**,历史资产与人写的资产用同一套渲染逻辑,只在每一行上带一个小「回填」徽记做区分。全盘采纳 [legacy-backfill.md](../../archive/v4-prototype/research/legacy-backfill.md) 的结论——双亲结构判定合入、口径 A/B 分开报、无标签无 CHANGELOG 就诚实留空、"不点灯"边界、自动/agent/人确认三分类。这一节把预研落成"谁在什么时机跑什么代码"。

**三层流水线(第 1 层已落地,第 2、3 层没做)**:

1. **buddy 主机代码算完能算的一切** —— **已落地**,在 `crates/bw-v4/src/app/backfill.rs`。
   算完的数字**直接写成同格式的正常仓文件**(`.bw/plan/YYYY-Www.md` 与 `.bw/releases.md` 的行),
   中间不落一份本机 evidence 文件 —— 那份文件唯一的读者本来就是第 2 层的 agent,第 2 层没做,它就是没人读的中间产物。
   实际算的是:首末提交时间、按 ISO 周的提交数、按周合入数(**双亲结构判定**,`git log --merges`)、
   带日期的 git 标签。**没算**的是:作者分布、按周目录 Top3、远端 issue 与 MR 明细
   —— 前两样今天没有界面读,后一样要连远端(见下)。
2. **agent 只读文本、不算数字** —— **没做**。要起一次真的 agent 会话(运作活②的 `mode=first`),
   剧本已经写好在 `standard/06-defaults/ops/asset-audit/SKILL.md` 里,缺的是真跑一次。
   因此:PROJECT.md 的草稿字段、CHANGELOG 解析、指标候选清单,这三样今天一个都没产出。
3. **人确认** —— 没有可确认的 MR(第 2 层没跑,回填直接写进工作区)。今天的回填是
   `Command::BackfillHistory` 一条命令直接写文件,人看到的是写完之后的结果。

**四份仓文件产出物 —— 今天真产出的只有前两份**:

| 产出物 | 位置 | 状态 |
|---|---|---|
| a) 历史周文件 | `.bw/plan/YYYY-Www.md`,front matter `origin: backfill` | **已落地**。周目标一律写「(未发现——这一周是回填的,当时没有周计划;不从提交消息里倒推目标)」;「本周运作」写「不适用」;按周统计只有提交数与合入数两列 |
| b) 历史发版行 | `.bw/releases.md` 现有表格追加行,「来源」列标"回填" | **已落地**,按版本号去重,重跑不长记录 |
| c) PROJECT.md 草稿字段 | `.bw/PROJECT.md` | **没做** —— 要第 2 层的 agent 会话 |
| d) 指标候选 | MR 说明或 `docs/metrics.md` | **没做** —— 同上 |

**回填多久**:往前最多 104 周(两年)。再往前的周文件对今天的管理没有帮助,只会把 `.bw/plan/` 撑大;
扫到这个数就停,这一条如实写进回填回执,不假装"全都回填了"。

**唯一落库的例外今天也没发生**:远端 issue 同步进 `issue` 缓存表这件事需要连远端,C 刀没做
(演示项目 `git clone --local` 出来,压根没有远端)。库里因此一行 `origin='backfill'` 的 issue 都没有。

**已经存在的周文件一律不碰** —— 那可能是人自己写的。只有"这一周有过提交、但没有对应周文件"才补。

**代码微重构建议也在首次盘点里顺手列为建议活**(与 09 篇 §2.2 每周模式同一条规矩):`mode=first` 跑的时候若顺带发现明显的死码/超长文件/命名问题,同样只列成「建议活」草稿(类别「优化」,`origin='agent_split'`)写进 MR 说明,不直接改代码,人评审这次铺底 MR 时一并勾选要建的——不单独为老项目开一条"回填顺便重构"的例外通道。

**防伪规则**(逐条,证据均见样例文件):

1. 合入记录用双亲结构判定(`git log --merges`),不用消息文字匹配——文字匹配会漏掉手写合并提交(样例:71 条真实合入漏掉 23 条)。
2. 远端 MR/PR 数(口径 B)与本地合并提交数(口径 A)分开报——历史周文件「按周历史统计」表的"合入 MR 数"列用口径 B,口径 A 只在完全没有远端连接时当退化替代且要注明"本地口径"(样例两口径分别是 71 和 50,不可混用)。
3. 无标签无 CHANGELOG 就诚实写"未发现可回填的版本记录",绝不拿 commit 日期倒推版本号(buddy 自己仓就是这个真实边界样例)。
4. 批量关闭事件不当速率信号:数字照实呈现,单周关闭数占总关闭数比例 >50% 时附一句提示,不改数字本身(样例:44 个已关闭里 34 个集中一周)。
5. 每个数字标复算命令:evidence 文件里每个字段附一个"用什么命令算出来的"字符串,产物拼装时原样带出(呈现位置由 UI 定)。
6. 回填标记贯穿到底(本篇对 02 篇「回填标记怎么实现」的答案,替换原 HTML 注释包段方案):历史周文件(`.bw/plan/YYYY-Www.md`,`origin: backfill`)整份由回填生成,重跑同一周就整份覆盖那一份文件,不追加重复段落——和运作活①「这份文件归它管」是同一个契约,不需要额外标记机制;`.bw/releases.md` 是回填内容与人写内容共存在**同一张表**里(不再有独立的"历史运作(回填)"节可以用 HTML 注释包住),重跑时按"版本号"列去重:该版本号已有一行(不论来源)就跳过或只更新说明字段,没有就追加新行,不产生重复行;库里 `issue.origin='backfill'` 是数据侧对应标记——`release` 表已取消,不再有 `release.origin` 列可打。

**buddy 今天缺的函数**(列名+模块+返回什么,不写实现,取自 legacy-backfill.md 最小集):

| 函数 | 模块 | 返回什么 |
|---|---|---|
| `list_tags`、`read_commits_since`、`commits_by_week`、`merges_by_week`、`author_distribution`、`top_dirs_by_week` | `crates/bw-engine/src/git_log.rs` | 依次:标签+hash+日期;带日期窗口的提交读取;按周提交数;按周合入提交数(口径A);作者→提交数;按周一级目录 Top3(带黑名单) |
| `list_closed_issues`、`list_merged_prs`、`list_tags`、`list_releases` | `crates/bw-engine/src/github.rs` | 依次:已关闭 issue 明细;已合并 PR 明细;远端标签;release 列表 |
| `list_closed_issues`、`list_merged_mrs`、`list_tags` | `crates/bw-engine/src/codehub.rs` | 同 github 侧,**命令未在真实 codehub 环境验证**(§4) |

这些函数只读、零副作用,和 `git_log.rs`/`evidence.rs` 今天"只读子进程、不解释不判断"的风格一致——判断留在流水线第一层的调用方代码。

**人确认与不点灯**:「在研版本」起点(**本篇对 legacy-backfill.md 开放问题 1 的答案**)——有真实标签/CHANGELOG 能识别出当前版本的老项目,`current_version` 直接取那个值;完全没有版本记录的老项目(如 buddy 自己),`current_version` 保持空,**不自动定成 v0.1**——`v0.1` 只属于待拍-04 的"新建项目"场景,老项目找不到版本历史就如实显示"未设置",直到人显式给一个。

回填不产生任何"要不要算战绩"的问题(本篇对 legacy-backfill.md 开放问题 3 的答案,大幅简化):V4 没有战绩台账这回事(02 篇 §2.3)——"干没干成"永远直接看远端 MR 合没合入,没有一张表可以"记"或"不记"。回填进来的这批老 issue 沿用同一条判据:`origin` 只记"这行数据当初怎么进的库",永久不改;这条老 issue 后来若被真的指派、▶开工、走完一次真实运行,它算不算"干成了"看的是那次运行对应的远端 MR 合没合入,和 `origin` 是什么无关——不需要任何"回填要不要计入战绩"的排除规则,也没有界面特殊处理要做(原「界面影响留 §6」这句一并撤回,不再是开放问题)。

不点灯:回填产出的历史周文件、`.bw/releases.md` 里标"回填"的版本行,只解释过去,不参与健康灯推导。唯一流入健康灯、也碰 git 合入记录的信号是**当前**周的"上周有交付"判据(08 篇已定),那是从当前记录实时现算的真实观测,与"回填一段历史给人看"是两次不同的计算,不重复。

### 2.5 一个 MR 与人介入

**2026-08-20 整块重写(此前这里记的是「开分支/开 PR 这两个动作根本不存在」的偏差,已经不成立)。** 铺底现在走完了完整一站:

1. 给这张活开一棵 worktree 和一条 `bw/issue-<号>` 分支(§2.2 第 5 条),核心件写在里面。
2. `commit_paths` 只提交这次真写下去的路径。提交成功与否(`committed: bool`)如实进 `Event::StandardBootstrapped`:工作区没有改动、或路径被 `.gitignore` 拒收,都会如实是 `false`。
3. 真提交出东西了,就 `git push -u origin <分支>`,然后按项目的 provider 开 MR(`Remote::create_mr_on_branch`,github 走 `gh pr create`、codehub 走 `codehub-cli mr create`),MR 号写回这张活。
4. 活推到「**评审中**」——最远只到这里,「完成」永远是人点的那一下。人在 MR 上评审,点通知屏的「合入并完成」:先真的合 MR,合成了才推「完成」;合入没成整条不算数,活留在评审中可以重试。
5. 每一种"没有 MR"的情况都在活的正文里写清楚为什么:没挂远端(分支只在本机,人自己 merge 那条分支)、这次没有新东西要提交、推分支失败(带原话)、开 MR 失败(带原话)。**不摆一个来历不明的空号。**
6. 仓的 `.gitignore` 拒收的件(buddy 自己的仓就忽略 `.claude/`)属于本机检出、不属于分支:它们同步一份到主工作区(已有同名文件就不覆盖),活的正文里逐条写明每一件的下落。不这么做的话,预置技能包只存在于这一张活的 worktree 里,下一张活开新树就读不到剧本了。

**还没做的**:历史回填的 agent 那一半要真起会话,产出「回填了哪些数字、从哪算出来的」供人评审;写开发手册已挪进资产盘点,同样没起过会话。所以今天的 MR 里只有写核心件那一步的产出,「几步拼成一份 MR 说明」这件事还不存在。

### 2.6 对账与升级

**对账**(平时任何时候读,不只是铺底那一刻):比对 `.bw/standard.toml` 的 `enabled`/`extensions` 清单、`standard/` 当前版本(`STANDARD_VERSION`)、`.bw/managed.toml` 每个文件的指纹,分三类,一个文件只落一类:**缺**(`enabled` 里的类别没有对应记录,或记录了但磁盘没有该文件)、**过期**(记录版本小于当前 `STANDARD_VERSION` 且磁盘指纹与记录一致——没人碰过,是干净的升级候选)、**人改过**(磁盘指纹与记录不一致,不看版本号,永远不参与自动覆盖,升级时走人工路径)。

指纹算法(本篇对 02 篇开放问题 3 的答案):文件原始字节的 SHA-256,完整 64 位十六进制小写,存成 `"sha256:<64位hex>"`,逐字节哈希比较,不做语义/空白归一化——简单确定,MVP 不需要更聪明的 diff。对账是纯读操作,成本低不需要缓存,触发时机两处:①知识库屏渲染时现算(放进「资产」页签);②运作活②每周固定跑一遍,结论追加进那周 `.bw/plan/` 尾段(09 篇负责调用)。

**升级**:人在知识库屏对"过期"文件点「看差异 → 升级」→ buddy 算文本差异(纯 diff),"人改过"里同时落后的文件一起列出并标"需要人工合并"→ 人确认要升级哪些文件(可只选一部分)→ "过期但没人改过"的文件建一张轻量活(和 `EditProjectCard` 同形状:无 agent 会话,建分支写新内容提 MR,`origin='human'`);"过期且人改过"的文件走一次真实 agent 会话(用第 2 步同一套合并原则),纳入同一张升级活一起提 MR → 合入后 `.bw/standard.toml` 与 `.bw/managed.toml` 一起更新。

**老项目已经有一份 `.bw/releases.md` 时怎么办**(口径与 02 篇 §6 一致):第 1 步「写核心件」已经先挡住了一层——如果铺底探测到项目接入前就有一份 `.bw/releases.md`(非 buddy 管理),第 1 步按"人手改过不覆盖"规则直接跳过、不写标准骨架(§2.2 第 4 步)。到历史回填这一步,`release_file.rs`(02 篇 §3.3)只认标准 5 列表头(版本号/发版日/说明/包含的活/来源)下面的行:表头匹配就在这张表里追加回填行,按版本号去重不产生重复行;**表头对不上标准格式,不是"整份不碰",而是在文件末尾另起一段「## buddy 管理的发版记录」**,把回填的行落进这段新起的表里,项目原有的那份发版记录一个字都不动——回归测试 `crates/bw-v4/tests/repo_files.rs::foreign_release_table_is_never_written_into` 守着这条行为(02 篇 §6 已把这条从开放问题改成已答)。两张表要不要合并,留给人在 MR 说明里看到提示后自己决定,不是解析器该做的事。

### 2.7 `standard/` 正本与同事贡献

正本住仓根 `standard/`(01 篇已定,与 `crates/`/`docs/` 平级),结构照 [standard-module-draft.md](../standard-module-draft.md) §3 设计成八大类。

仓根 `standard/` **今天真有**下面这些目录 / 文件(八大类不是全都铺开了):`06-defaults/ops/` 已经建了,里面是两份自建运作 workflow 的正本(规范铺底不起 agent,没有剧本);**`02-agents/` 已于 2026-08-20 整个删除**(§2.3);`CHANGELOG.md`、`07-cadence/`、`pond/` 仍然没建。

```
standard/
├── VERSION                    # 纯文本如 "5.0",bw-v4 的 standard::version() 读这个
├── README.md                  # 这套规范是什么,含"还没做的部分"如实说明
├── 01-charter/
│   └── PROJECT.md.tmpl
├── 03-docs/
│   ├── plan/
│   │   ├── README.md
│   │   └── WEEK.md.tmpl
│   ├── design/README.md
│   └── releases.md.tmpl
├── 04-metrics/
│   └── metrics.toml.tmpl
├── 05-issue-policy/
│   └── issue-policy.toml.tmpl
├── 06-defaults/
│   └── ops/                          # B 刀建的:三份自建运作 workflow 正本
│       ├── README.md                 # 三张活的总说明 + 版本怎么走
│       ├── week-planning/SKILL.md    # 运作活①入口
│       ├── week-planning/skills/metrics-refresh/SKILL.md
│       │                             #   子技能,由 north-star-discovery +
│       │                             #   metrics-binding 两份合并而成
│       ├── asset-audit/SKILL.md      # 运作活②入口(mode=weekly / first 分支)
│       └── asset-audit/skills/project-handbook/SKILL.md     # 盘点首次模式里的一个提议:给这个项目写开发手册
└── 08-meta/
    └── standard.toml.tmpl

# 还没建的(不是遗漏,是如实标注未建):
# CHANGELOG.md   —— 每次改动一行,含"试点里用没用上"的证据
# 07-cadence/    —— 运作节律文字说明(实际配置目前在 05-issue-policy 一起铺)
# pond/          —— 鱼塘:未严选的技能/workflow
```

`05-issue-policy/issue-policy.toml.tmpl` 里「类别→工具→workflow」那张映射是**五行**,对应五阶段方法论(原型/构建/优化/运维/运营推广)各一行,不是六行——模板文件自己的注释写得很直白:「铺底默认给五个活的类别……各配一行默认映射……这里只有五行,没有第六类:五阶段方法论本身就是五个阶段,不无中生有出一个第六类别」。

**同事怎么贡献**:一条 PR 直接改 `standard/`,和改代码一样过评审(人 + `/code-review`)、合入、`CHANGELOG.md` 加一行(建好之后)、`VERSION` 按需 +0.1——buddy 不需要为此改任何 Rust 代码。最常见场景是往 `06-defaults/` 加一份新的内置 workflow(B 刀之后这个目录真的在了,三份运作 workflow 就住在 `06-defaults/ops/`,照着加第四份即可),或往 `pond/`(还没建)添一条"值得进鱼塘"的第三方技能记录。评审判据怎么进 CHANGELOG(§3 已定,本篇引用):每类 README 写"为什么要它、不要它会怎样";试点两周后把"用没用上、agent 读没读、人改没改"记进 CHANGELOG,决定下一版降为扩展还是进鱼塘——和技能严选同一逻辑。

### 2.8 命令 / 事件(名字 + 一句话)

与 01 篇 §2.6 已列的 `RunStandardBootstrap` 对齐,本篇补齐内部编排需要的另外三个:

| 命令 | 一句话 |
|---|---|
| `RunStandardBootstrap { project_id }` | 一次性运作活③入口:探测 → 建活 → 第 1 步同步写核心件 → 按探测结果决定要不要自动触发第 2/3 步的交互式运行 → 一个 MR(01 篇已列,本篇细化内部编排) |
| `BackfillHistory { project_id }` | 单独重跑历史回填(不重跑第 1/2 步):建一张运作活("历史回填 · 重跑"),开新分支起一次「资产盘点」workflow 会话、传 `mode=first`(用户拍板改动,见本篇导读与 09 篇 §2.2——第 2 层仍要 agent 读文本产出候选,不是无会话的轻量活),重走 2.4 节流水线,按 3.4 节的幂等规则重新产出(历史周文件整份覆盖对应周、`.bw/releases.md` 按版本号去重更新) |
| `ReconcileStandard { project_id }` | 纯读:按 2.6 节算「缺/过期/人改过」三类,不建活不写仓,给知识库屏渲染用 |
| `UpgradeStandard { project_id, files }` | 人选中要升的文件后触发:按 2.6 节升级流程建轻量活(纯替换)或一次 agent 会话(需合并),最终提 MR |

---

## 3 · 工程对照

### 3.1 探测 + 写核心件落在哪(`crates/bw-v4/src/standard/bootstrap.rs`)

探测与写核心件这两件事都在 `crates/bw-v4/src/standard/bootstrap.rs` 一个文件里(V4 为什么不摞在旧 crate 上,见 01 篇 §2.1)。真实结构如下,字段名照抄源码:

```rust
// crates/bw-v4/src/standard/bootstrap.rs
pub struct BootstrapReport {
    pub written: Vec<String>,            // 真的写进去的路径
    pub skipped: Vec<(String, String)>,  // 跳过的路径 + 为什么跳
    // 原来这里还有一行 `skills: Vec<String>`(复制进 .claude/skills/ 的预置包路径)。
    // 技能不再进用户仓,这个字段已删,不留空字段占位。
}
pub fn write_core_files(workspace: &Path, vars: &BootstrapVars) -> Result<BootstrapReport, RepoFileError>

pub struct BootstrapProbe {
    pub owned: bool,             // 根提交作者是不是「Builders' Workbench」
    pub has_agent_docs: bool,    // README/CLAUDE.md/AGENTS.md 三选一存在,触发第2步
    pub has_history: bool,       // 触发第3步,判据见 §2.1 已改写的三条
    pub reasons: Vec<String>,    // 写进 Issue 说明的证据句子
}
pub async fn probe(workspace: &Path) -> BootstrapProbe
pub fn issue_title() -> String                            // 幂等键,只跟版本号走,不跟探测结果走
pub fn planned_steps(probe: &BootstrapProbe) -> String    // 写进活正文的"打算跑什么"
```

`.bw/managed.toml` 的读写 + 指纹算法 + 对账判定不在这个文件里,是独立的一份仓文件解析器 `crates/bw-v4/src/repo/managed_file.rs`(02 篇 §3.3 已介绍):

```rust
// crates/bw-v4/src/repo/managed_file.rs
pub struct ManagedEntry { pub path: String, pub version: String, pub fingerprint: String }
pub struct ManagedFile { /* entry(path) / upsert(entry) 两个方法 */ }
pub enum Reconcile { Missing, Stale, HumanEdited, UpToDate }
pub fn fingerprint(bytes: &[u8]) -> String                 // sha256
pub fn reconcile(entry: Option<&ManagedEntry>, disk: Option<&[u8]>, version: &str) -> Reconcile
pub fn read(workspace: &Path) -> Result<Option<ManagedFile>>
pub fn write(workspace: &Path, f: &ManagedFile) -> Result<String>
```

历史回填的代码在 `crates/bw-v4/src/app/backfill.rs`,只做了 §2.4 三层流水线的第 1 层(自己算 git、直接写仓文件);**没有**「先汇总成一份本机 evidence JSON 再交给 agent 读」这样一个中间模块,理由见 §2.4。

### 3.2 编排落在哪(`crates/bw-v4/src/app/bootstrap.rs`)

编排在 `crates/bw-v4/src/app/bootstrap.rs` 的两个方法里:

```rust
// crates/bw-v4/src/app/bootstrap.rs
impl App {
    pub(super) async fn run_standard_bootstrap(&mut self, project_id: ProjectId) -> Result<Vec<Event>> {
        let probe = boot::probe(&ws).await;
        // 幂等键是标题(只跟版本号走),探测到什么写进正文,不写进标题——
        // 否则第一次铺底自己写出 CLAUDE.md 后,第二次探测结论一变,标题跟着
        // 变,幂等失效,重跑会多建一张活。
        let issue_id = /* 建一张 kind=Ops、origin=Auto 的活,标题固定「规范铺底 v{版本}」 */;
        let report = boot::write_core_files(&ws, &vars)?;   // 第1步,同步
        // 跳过的件、写下去但被 .gitignore 拒收的件都追加进这张活的说明。
        let committed = crate::git::commit_paths(&ws, &report.written, &msg).await?.committed;
        // 2026-08-20:worktree + 分支 + push + 开 MR 已经接上(§2.5),新仓老仓
        // 走同一段代码,不再有「owned 直推」这一分支。第 2/3 步仍然没有能触发
        // 的代码,只把「还没跑的步骤」写进活的说明。
        Ok(vec![Event::StandardBootstrapped { project_id, issue_id, files: report.written, committed }])
    }

    /// 纯读的对账:缺 / 过期 / 人改过。不建活、不写仓。
    pub(super) async fn reconcile_standard(&mut self, project_id: ProjectId) -> Result<Vec<Event>> { /* .. */ }
}
```

`auto_start_run`/`open_bootstrap_pr` 这两个分支目前都不存在 —— 没有自动触发第 2/3 步交互式运行的代码(§2.2 第 5 步)。`CreateAutopilotTask { auto_run }` 这条给运作活②用的既有能力本身没问题,只是铺底这条编排路径还没有走到需要复用它的那一步。

### 3.3 `.bw/managed.toml` 与对账查询

```toml
[[file]]
path        = "AGENTS.md"
version     = "4.0"
fingerprint = "sha256:9f2a1c7ec3b8...(64位完整hex,此处省略中段仅为文档排版)"
```

对账判定(伪码):`enabled` 无对应 `managed` 记录、或记录了但磁盘无该路径 → `Missing`;磁盘指纹 == 记录指纹 且 `managed.version < STANDARD_VERSION` → `Stale`;磁盘指纹 != 记录指纹(不看版本号)→ `HumanEdited`;其余 → `UpToDate`。

### 3.4 回填产出怎么做到重跑幂等(替换原 HTML 注释包段方案)

不再需要一套通用的"标记包段落"机制、也不需要一个独立的 `backfill_marker.rs`——四份产出物(§2.4 表格 a-d)各自靠自身文件结构解决幂等:

- **历史周文件**(a):`.bw/plan/2026-W32.md` 这类文件本身按 ISO 周一一对应,重跑 `BackfillHistory` 时对某一周"整份重新渲染、原地覆盖"即可——这份文件从创建起就整份归回填管,不存在"文件里一半回填一半人写"的情况(人写的是本周/正在过的周,历史周不会有人手改)。
- **`.bw/releases.md`**(b):`release_file.rs`(02 篇 §3.3)解析出的现有行按"版本号"去重——已存在同版本号的行就跳过(或按需要更新"说明"字段),不存在就追加一行,普通 Markdown 表格操作,不需要 HTML 注释包段落。
- **`.bw/PROJECT.md`**(c):只在字段仍是"待填"占位时才写(§2.2 第 4 步同一条"人手改过不覆盖"规则的自然复用)——重跑时如果上次已经填过(不管是回填填的还是人后来改的),这次不再覆盖。
- **指标候选**(d):每次都是全新写进这次 MR 说明的一段文字,不是持久文件里的一块,天然不存在"重跑要不要覆盖"的问题。

`managed_file.rs`(02 篇 §3.3/本篇 §3.1 已定义)只负责规范核心件(`AGENTS.md`(仓根)/`.bw/*.toml` 等)的指纹对账,不管这四份回填产出——两套机制服务两类不同的文件,不合并成一套。

---

## 4 · 边界与失败

**不做什么**:不发明数据——git/远端/群里没有的东西产物里就是空,不启发式硬猜;不覆盖人手改过的文件——指纹对不上只提示不写;不跳过成熟仓——待拍-20 已定,照样铺全套核心件,只多一步合并;不做 monorepo 拆分——整仓当一个项目,如实标注局限;群历史不进仓不进库——只用于给运作活①生成本机摘要,和历史回填这条产线不搭界;决策记录本轮不回填(commit 信息和文档正文里的线索零散、没有统一格式)。

**失败如实显示**:
- **远端拉不到**(未认证/网络断/codehub 探针失败):对应那一行/那一块显示"—"+"该来源未取到",不阻塞其余部分继续生成——延续 `github::probe_repo`"探不通就如实报错"的既有原则。
- **codehub 侧明细列表命令未经真实环境验证**:输出格式与预研假设不一致时函数应返回 `Err`,不能返回一个"看起来正常实际全错"的空列表。
- **agent 合并/回填会话失败或中断**:活如实停在"进行中"(不自动判失败),人可重试;已提交部分保留不回滚,重试在同一分支继续。
- **MR 打不开**(远端权限问题):提交留在本地分支,活说明如实写"提 MR 失败:<错误原文>",处理权限后重试开 MR 这一步,不需要重跑整条流水线。
- **仓太大**(万级提交):`top_dirs_by_week` 只对最近 8-10 个 ISO 周精算,更早历史只给累计数字不细分到周——不做全仓扫描。

---

## 5 · 验收与读回

用 buddy 自己的仓(样例文件已手工跑过同款命令,数字可直接对照)当"接入老项目"的真实样例,mock 交互式执行器(无真实工作区时的自标注替身,标「流程演示」)跑一遍运作活③:

1. **第 1 步产物**:深链 `BW_OPEN=<样例项目名> BW_PANEL=kb`,stderr 见 `[BW_OPEN]`;分支(或 owned 情形的默认分支)上出现 `.bw/PROJECT.md`/`AGENTS.md`(仓根)/`CLAUDE.md`/`.bw/issue-policy.toml`/`.bw/standard.toml`/`.bw/managed.toml`,`git show <分支>:.bw/managed.toml` 读到每个文件的指纹。
2. **探测正确性**:`sqlite3 <db> "SELECT title FROM issue WHERE kind='ops' AND workflow LIKE '%铺底%';"` 读回的标题应出现"含历史回填"(buddy 仓有 615 条提交、50 个已合并 PR)。
3. **第 3 步数字对照样例**:`.bw/plan/2026-W33.md`/`.bw/plan/2026-W34.md`(两份历史周文件)「按周历史统计」表里的"提交数"列应分别为 51/38,与样例文件 §2 一致;`.bw/releases.md` 不应新增任何行(该仓无 tag/CHANGELOG,防伪规则 3 已定不倒推版本号),这次回填 MR 的说明里应能看到一句"未发现可回填的版本记录:仓内无 git 标签、无 CHANGELOG/RELEASES 文件"。
4. **回填不产生可查的"战绩"**:`SELECT origin, COUNT(*) FROM issue WHERE project_id='<pid>' GROUP BY origin;` 应见 `backfill` 档非零(对应真实远端 issue #78/#81);`sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name='workflow_credit';"` 应为空结果——库里根本没有这张表可查(02 篇 §2.1 已定),"干没干成"改看远端 MR 状态,不是查库。
5. **对账**:改动一个已铺底文件的一个字符后跑 `ReconcileStandard`,该文件应归类 `HumanEdited`;对账无直接对应表(纯读派生),用界面截图+手工核对指纹字符串读回。
6. **新库 schema 一次到位,不涉及老库迁移**:V4 不给存量库加列(02 篇 §2.7 已定:新库用新文件名,`schema.sql` 直接建全)——`sqlite3 <db> "PRAGMA table_info(issue);"` 对一个全新建出的 V4 库应直接看到 02 篇定义的全部 9 个扩展列,不依赖任何 `add_column_if_missing` 参与;这条验收不涉及"给存量库开新版程序"这类迁移场景(那是试点起才恢复的守卫,见 02 篇 §3.2)。

---

## 6 · 开放问题(≤5)

1. **`origin='backfill'` 的 issue 后续被真实推进后,界面上还要不要区分"这条曾经是回填的"**——本篇处理(2.4)是 `origin` 永久不改、真实进展看远端 MR 合没合入(没有战绩表可记,也没有"记/不记"的选择),数据层自洽,但总览"远端 issue 累计"这类计数会不会因此让人觉得数字对不上直觉,需要用户判断要不要在界面上加一层区分。
2. **目录黑名单(vendor/node_modules 这类噪声)要不要做成项目可配置**,还是本篇给的默认硬编码清单(`vendor/`、`node_modules/`、`dist/`、`build/`、`target/`、`.venv/`)先够用。
3. **决策记录(`.bw/decisions/`)回填要不要留一个未来钩子**——本轮明确不做,但值不值得在 schema 或文件格式上预留位置,避免以后要做时多一次迁移。
4. **codehub 侧「已关闭 issue / 已合并 MR」明细列表命令尚未在真实 codehub 环境验证**——谁来协调第一次真实验证,是定稿后立刻做,还是等真有 codehub 老项目接入时再做。
5. ~~空仓例外下运作活③走"确认完成(人裁)"~~ —— **2026-08-20 已答,不再是开放问题**:空仓例外整条取消(§2.1),运作活③和别的活一样停在「评审中」,入口就是通知屏/总览的「合入并完成」(有 MR 就先真的合,没有 MR 就只走完成并如实说没合)。
