# 03 · 规范铺底怎么跑:三步流程、对账、升级

> **30 秒导读**:这篇管运作活③「规范铺底」——项目接入时自动出现的一次性活——具体怎么跑:第 1 步 buddy 自己写模板核心件、第 2 步(成熟仓才有)agent 把 buddy 约定合进已有的 README/CLAUDE.md/AGENTS.md、第 3 步(有历史的仓才有)把老项目自己的历史记录回填成 buddy 认得的文件。外加铺完底之后的日常动作:对账(缺什么、过期什么)、升级(出新版规范怎么走 MR)、`standard/` 正本怎么让同事贡献。**第五轮改动(用户拍板,待拍-05/27 改)**:第 3 步「历史回填」不再是运作活③自己养的一个独立子技能——它就是**运作活②「资产盘点」workflow 的首次模式**(同一个 `asset-audit` workflow 包,`mode=first` 全量回填历史、`mode=weekly` 每周增量盘点),本篇 §2.4 描述的三层流水线架构原样成立,只是挂载的 workflow 包换了,谁触发它、剧本怎么写归 [09-ops-workflows.md](09-ops-workflows.md) §2.2/§2.3 管,本篇只管"探测到历史该不该跑""跑出来的原料/产物/防伪规则长什么样"。**2026-08-20 按用户第二轮回复(六-3)整块重写了 §2.4「五个产物」**:回填产出改成与运作活①同模板的历史周文件(`docs/plan/YYYY-Www.md`,`origin: backfill`),`docs/plan/history.md` 与「界面另开回填块」两种说法全部取消。**2026-08-20 按用户第七轮盘点(库从 20 张表砍到 4 张)再整块重写了 §2.2 第 6 类产出、§2.4「五个产物」的落地细节与防伪规则、§2.6、§3.4**:铺底不复制预置技能包的旧说法作废——预置包随 buddy 出厂,铺底第 1 步就要把它复制进项目仓 `.claude/skills/`;历史回填从"五个产物"收窄成四份仓文件(不再有 `docs/plan/history.md`、`docs/releases.md` 独立"历史运作(回填)"节这类新文件/新分段),库里唯一落地的是 `issue` 缓存表本来就有的 `origin='backfill'` 行,不是新建的表;`workflow_credit`/`release`/`week_plan` 这些第六轮及更早草案里出现过的表全部不复存在,回填因此也不再有"要不要计入战绩"这个问题。**详细设计稿,待用户复核,尚未开工写代码**。给三种人看:复核设计的用户、下一步写代码的会话、以后往 `standard/` 提 PR 的同事。母文档([`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) 第 0 站、§2.6、§6、待拍-02/15/16/20/27/29)与 [`../standard-module-draft.md`](../standard-module-draft.md)(八大类)是设计事实源,冲突时以它们为准;预研 [`../research/legacy-backfill.md`](../research/legacy-backfill.md) + 样例 [`legacy-backfill-sample-buddy.md`](../research/legacy-backfill-sample-buddy.md) 的结论**全部采纳**,本篇的细化/补答另标注。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)——不新开代号系列,三步就叫「第 1/2/3 步」。

---

## 0 · 这篇管什么、不管什么

**管**:①探测——两卡填完后怎么判断运作活③跑几步,空仓例外怎么判定;②第 1 步「写核心件」全流程;③第 2 步「合并调整」agent 任务清单(合并原则,非 SKILL.md 正文);④第 3 步「历史回填」全设计——原料分给脚本/agent/人、四份仓文件产出物字段(+ 一行库缓存例外)、防伪规则、幂等、限时/抽样、缺的函数清单;⑤三步合成一个 MR、人怎么评审;⑥对账与升级流程;⑦`standard/` 正本结构、版本号、同事怎么贡献;⑧本篇命令/事件名字。

**不管**:`standard/` 放哪、怎么进二进制、版本常量叫什么——[01-architecture.md](01-architecture.md) §2.9 已定,本篇直接引用。`.bw/*.toml` 与仓文件**格式**——[02-data-and-files.md](02-data-and-files.md) 已给,本篇只补留白处(`.bw/managed.toml` 指纹算法、回填历史周文件里"累计贡献者"字段落在哪一份文件)。「合并调整」「历史回填」两个技能的 **SKILL.md 正文剧本**——[09-ops-workflows.md](09-ops-workflows.md)管,本篇只给任务清单。计划屏周列表 / 总览发版记录怎么把回填的历史周、历史版本渲染出来(不另开专门块,只带徽记)——[06-plan-screen.md](06-plan-screen.md)/[08-overview-derivation.md](08-overview-derivation.md) 已给,本篇只保证产出的形状(同格式的历史周文件、`docs/releases.md` 历史版本行)能被它们直接消费。开工工具注册、workflow 识别——[04-tools-and-workflows.md](04-tools-and-workflows.md)管。

---

## 1 · 用户看到什么、做什么

**新项目(空仓)**:填完接入两卡点完成的那一刻,`onboard` 屏不用等——运作活③一两秒内跑完第 1 步,核心件直接推上默认分支(空仓例外,见 2.1)。总览的项目信息已经是刚填的内容,仓里已有 `PROJECT.md`/`AGENTS.md`/`.bw/*` 骨架;活列表里能看到「规范铺底 v4.0」,因为没有 PR 可评审,它走 CONTEXT.md 已定义的「没有 PR → 人点『确认完成(人裁)』」这条既有路——不是自动完成,只是不用等合入。

**接入已有仓(成熟项目)**:总览「待人处理」很快多一条「规范铺底 v4.0 · 含合并调整」或「… · 含合并调整 + 历史回填」,标题已写清楚跑几步。会话屏能看到真实 Claude CLI 会话在这个活的 worktree 里跑,完事推到「评审中」,MR 里能看到:`standard/` 骨架文件、被合并的 `AGENTS.md`/`CLAUDE.md`(原文一字未删,buddy 章节插在靠前位置)、`docs/plan/` 下新增的若干份**历史周文件**(`origin: backfill`,有历史才有,和运作活①写的周计划同一模板)、`docs/releases.md` 多出的历史版本行、PROJECT.md 补的草稿字段。人看一眼评审要点(2.5 节)就能合入、点完成,有权限时「合入并完成」一键。合入后总览立刻提示「本周还没有计划 → 开始本周」;计划屏左栏周列表能直接看到这些历史周(带回填徽记),总览发版记录也能看到刚回填的历史版本——**不出现任何专门的"回填"区块**,历史资产就混在正常列表里(第六轮用户拍板,见 §2.4)。

**平时**:知识库屏顶部有一条不打扰的小字提示——对账发现「缺 2 项 / 过期 1 项 / 你改过 1 项」,点开看具体文件、要不要升级。不是弹窗;运作活②每周跑时顺带算一遍,写进那周 `docs/plan/` 尾段。

---

## 2 · 设计

**§2.2–§2.5 三步流程的实现范围,A 刀落地后改。** 这四节原文按「设计出来的样子」平铺直叙三步,读起来像三步都已经跑通。实际 A 刀只跑通了**第 1 步「写核心件」**(`crates/bw-v4/src/standard/bootstrap.rs::write_core_files`,§2.2 描述的落盘规则与其对应)。第 2 步「合并调整」与第 3 步「历史回填」都要起一次真实的 agent 会话去读文本、写产出,这一刀完全没做——`run_standard_bootstrap`(`crates/bw-v4/src/app/bootstrap.rs`)探测到仓需要跑这两步时,只把「还没跑的步骤」如实写进这张活的说明段落,不假装跑过。下面 §2.2 描述的是已经跑通的部分;§2.3/§2.4 描述的是设计,留给后面的刀实现,阅读时按此对待。

### 2.1 触发与探测:这张活要跑几步

两卡填完、项目行写入库、工作区就绪后,`RunStandardBootstrap` 一次创建活并立即执行第 1 步。探测不问用户,现算,复用能复用的既有函数:

| 探测项 | 怎么判 | 函数(现成/新增) |
|---|---|---|
| 空仓例外 | 根提交作者是不是「Builders' Workbench」 | **现成** `workspace::is_owned_workspace()`——今天已用来决定 `write_charter`/`write_component_standards` 该不该写;本篇把它的用途从「写不写」改成「直推还是走 MR」 |
| 触发第 2 步 | README/CLAUDE.md/AGENTS.md 三个路径存在性检查 | 新增,放进 2.2 节要建的 `standard_bootstrap.rs`,不需要新模块 |
| 触发第 3 步 | 以下任一为真:①`commit_count`>1(1 是 buddy 自己的 scaffold 提交);②`git tag -l` 非空(新增);③仓根有 CHANGELOG/RELEASES;④远端有已关闭 issue 或已合并 MR(新增);⑤`.bw/project.toml` 的 `[chat]` 段已配置 | 见 2.4.4 |
| 后来者接入 | `.bw/standard.toml` 已存在 | 复用 `project_file.rs` 同款 `Ok(None)` 惯例,读到就说明有人先接过——不新建活,读正本预填(和 `.bw/project.toml` 后来者逻辑对齐,V2 已有先例) |

**「触发第 3 步」这一行,A 刀落地后改。** 原文列了五条探测依据,实际只实现了前三条:`crates/bw-v4/src/standard/bootstrap.rs::probe()` 判 `has_history` 只看 `commits > 1 || !tags.is_empty() || has_changelog`(提交数、`git tag -l`、仓根 CHANGELOG/RELEASES/CHANGELOG.md 三选一)。「④远端有已关闭 issue 或已合并 MR」与「⑤`.bw/project.toml` 的 `[chat]` 段已配置」这两条没有接——不是判了否,是代码里根本没有这两条查询。设计仍然保留五条(远端信号、群配置都是判断"这个项目是不是已经在正经跑"的合理依据),但 A 刀这一刀只做了本地 git 能现算的三条。

探测结果一次性写死进 Issue 标题与说明(不随后续改动变化,是"打算跑什么"的快照):标题固定前缀「规范铺底 v{STANDARD_VERSION}」+ 命中项后缀「· 含合并调整」「· 含历史回填」;说明段落人话列出证据,如「仓有 71 条提交、0 个标签、已发现 AGENTS.md,将执行:写核心件 → 合并调整」——这是 CLAUDE.md「报告不代答」纪律的落点,评审者不用猜这张活为什么跑了这几步。`kind='ops'`、`origin='auto'`(和运作活②同一档)、`tool='claude_cli'`(仅第 2/3 步用到)。

### 2.2 第 1 步 · 写核心件(buddy 自己写,不起 agent)

同步主机代码逻辑,不经交互式执行器,和今天 `write_charter`/`write_component_standards` 同一技术形状,范围从「章程+四份组件标准文件」扩到规范全部核心件,并套一层分支/MR:

1. 读 `.bw/standard.toml` 的 `enabled` 清单(新仓没有就取二进制默认核心清单:`charter, agents, docs-core, metrics, issue-policy, defaults-core, cadence`,同 02 篇 §2.8 样例)。
2. 按清单逐类从 `standard_assets` 取模板渲染到目标路径:
   - 第 1 类章程 → `PROJECT.md`(复用现成 `charter_md()`,填两卡内容,不是静态模板——从第一天起就"有数据");
   - 第 2 类 → `AGENTS.md`(模板渲染)+ `CLAUDE.md`(仅一行 `@AGENTS.md`,待拍-15);
   - 第 3 类 → `docs/releases.md`(空表头)、`docs/design/README.md`(约定说明);`docs/plan/YYYY-Www.md` 不在这一步写,那是运作活①第一次跑才有的东西;
   - 第 4 类 → `.bw/metrics.toml`/`.bw/connectors.toml`/`.bw/collect_stats.*`/`docs/metrics.md`:创建流已写过的不重写内容,只登记指纹;没有的写空骨架;
   - 第 5 类 → `.bw/issue-policy.toml`(02 篇 §2.8 给的默认三列映射 + review/cadence/kanban 四段);
   - 第 6 类 → 复制预置技能包进 `.claude/skills/`:buddy 自建运作技能三份(更新指标与周计划/资产盘点/规范铺底)+ **业界包(mattpocock-skills、superpowers)**(第七轮改动,待拍-32、02 篇 §2.5 已定:预置包随 buddy 二进制分发,Claude CLI 只在项目仓里找技能,不复制进来就读不到——铺底第 1 步因此必须真的把这些包的文件复制进项目仓,不是只记一个名字假设本机另装了插件);`.bw/issue-policy.toml` 的 `workflow` 列记的就是复制进来的包名。哪些包算"默认要复制的预置件"、哪些留在 `standard/pond/` 鱼塘不复制,由 04 篇的严选清单定,本篇只声明"铺底第 1 步会做复制这个动作";
   - 第 8 类 → 最后写 `.bw/standard.toml`(`version = STANDARD_VERSION`)与 `.bw/managed.toml`。
3. 每写一个文件同时往 `.bw/managed.toml` 追加一条(`path`+`version`+`fingerprint`,算法见 2.6);这份清单**最后写**,保证记的指纹是刚落盘那一刻的真实内容。
4. 人手改过不覆盖:只在**目标路径不存在**或**存在但指纹与记录一致**时才写;两者都不满足就跳过,在 Issue 说明追加一行「`XXX.md` 已存在且非 buddy 管理,跳过」——和第 2 步"不覆盖已有 AGENTS.md"是同一精神在不同文件上的应用。
5. **落盘方式,A 刀落地后重写。** 原文按 2.1 节的空仓判定二选一:owned 直接在当前分支 commit+push;非 owned 开一条 `bw/issue-<n>` 分支、提交在这条分支上但先不开 PR。这个设计的前提是内核已经有「开分支」这个动作——实际没有:`crates/bw-v4/src/git.rs` 里除了只读查询(`is_repo`/`commit_count`/`tags`/`current_branch`/`is_dirty`/`root_commit_author` 等),唯一的写操作是 `commit_paths(workspace, paths, message)`,只做 `git add` 那几个点名的路径 + `git commit`,**没有建分支、没有 push、没有开 PR 这几个函数**。所以不论 owned 与否,`run_standard_bootstrap`(`crates/bw-v4/src/app/bootstrap.rs`)现在做的都是同一件事:在**工作区当前所在的那条分支**上直接提交这次真写下去的文件,提交信息 `docs(bw): 规范铺底 v{版本} · 核心件`;只 add 点名路径、不用 `add -A`,免得把用户手上没写完的改动一起打包进这次提交。没有分支、没有 PR、也就没有「等三步写完再一次开」这件事——本节 §2.5 有更完整的说明。

跑完:只需第 1 步(空仓,或接入的仓既无 README/AGENTS.md 也无历史)→ 这次提交完成后这张活直接走「确认完成(人裁)」既有路(§2.5);需要第 2/3 步 → 这两步 A 刀还没实现(见「## 2·设计」开头的范围说明),设计上是紧接着自动触发一次交互式运行、同一 worktree 继续跑,但目前只把「还没跑的步骤」写进活的说明,不会真的继续跑下去。

### 2.3 第 2 步 · 合并调整(agent,仅成熟仓)

**输入**:已有 `README.md`(仅供判断上下文,不改)、已有 `CLAUDE.md`/`AGENTS.md`(改的对象)、规范第 2 类模板正文、第 3 类"目录约定"内容。

**agent 要做的事(清单,供 09 篇写成剧本)**:

1. 读现有 `AGENTS.md`;没有就看 `CLAUDE.md` 有没有实质内容,再没有就看 `README.md` 里类似"开发约定"的章节。
2. **合并原则**——不是拼接、不是覆盖:已有内容一字不删、一段不改;buddy 固定章节(读什么/活怎么做/指标怎么碰/禁止事项/代码图用法)插在**靠前位置**(标题之后、原文之前)而不是全部追到文件末尾——很多 agent 工具按上下文预算截断长文件,buddy 的强约束必须优先被读到;遇到标题字面撞车(如项目已有"## 活怎么做"但内容不是一回事),不覆盖原标题,buddy 版本改标题加"(buddy 补充)"紧跟其后插入,并在 MR 说明里提醒人核对;项目自定义段(模板固定第 8 段)原样保留已有内容,不清空。
3. `CLAUDE.md` 单独处理:不存在只写一行 `@AGENTS.md`;已存在但是空壳/纯导入行,直接换成标准写法;已存在且有实质内容,在最前面插入导入行+一句分隔说明,原内容整体后移、不删。
4. agent 不判断"合并得对不对"(那是人评审的事),只在这一步结束时把改动了哪些文件、每个按上面第 2/3 条哪种情况处理的,写进这次会话给 MR 说明的草稿段落。

**产出**:合并后的 `AGENTS.md`/`CLAUDE.md`,提交在同一条分支上,不单独开 MR。

### 2.4 第 3 步 · 历史回填(采纳预研,仅有历史的仓;第五轮定性:即运作活②「资产盘点」workflow 的首次模式)

**与运作活②的关系(第五轮用户点破,待拍-05/27 改)**:老项目历史回填不是一条独立产线,它就是「资产盘点」这个 workflow 第一次跑——同一个 workflow 包,`mode=first` 时多产出本节说的四项回填件,`mode=weekly`(每周)只盘变化、不产出这些历史件。触发时机不变:铺底探测到仓有历史(§2.1)时,运作活③在同一张活的分支上另起一次会话跑这个 workflow(SKILL.md 剧本见 09 篇 §2.2);原料仍是三种——①git 本地历史、②仓内已有文档、③远端 issue 与 MR 列表,**群历史不算原料**(与母文档、09 篇口径一致,见下)。

**产出形态(第六轮改动,用户第二轮回复六-3):不再是一份独立的"历史运作(回填)"新文件,而是补出"本该有但老项目没攒出来"的**同格式**正常文件**——回填探测到某个历史 ISO 周有过合入或提交、但当时没有 `docs/plan/YYYY-Www.md`,就按运作活①写周计划**同一套模板**给它补一份,front matter 标 `origin: backfill`;回填探测到某个历史版本(git tag/CHANGELOG)但 `docs/releases.md` 里没有对应行,就按同一张表格式补一行,「来源」列标"回填"。用户原话:「期望资产盘点发现老项目没有周计划 md / 发版本 md,就把这些回溯补起来,总览和计划 UI 不需要特殊处理,本身就是对照资产渲染」——落地就是:计划屏左栏周列表天然会多出这些历史周(06 篇),总览发版记录天然会多出这些历史版本行(08 篇),**不建 `docs/plan/history.md`,界面不为回填开辟任何专门区块**,历史资产与人写的资产用同一套渲染逻辑,只在每一行上带一个小「回填」徽记做区分。全盘采纳 [legacy-backfill.md](../research/legacy-backfill.md) 的结论——双亲结构判定合入、口径 A/B 分开报、无标签无 CHANGELOG 就诚实留空、"不点灯"边界、自动/agent/人确认三分类。这一节把预研落成"谁在什么时机跑什么代码"。

**三层流水线**:

1. **buddy 主机代码算完能算的一切**,写成一份本机 evidence 文件(不进仓不进库,用完即弃,同"上周群消息摘要"待遇)。纯确定性计算,不起 agent:git 本地(提交总数、首末提交时间、作者分布、标签、双亲结构判定的合入记录总数、最近 8-10 个 ISO 周的提交数/合入提交数/目录 Top3,应用黑名单);远端(open+closed issue 与 merged PR/MR 的计数与明细——GitHub 用 `gh` 现成命令,codehub 用新增函数,未在真实环境验证,风险见 §4)。
2. **agent 只读文本,不算数字**:拿到 evidence 文件(任务说明写明"数字照抄,不要自己数,和脚本不一致以脚本为准")+ 项目自己的 README/CHANGELOG/RELEASES(若存在)。要做的事:①从 README 首段提炼"想做什么"填 PROJECT.md 草稿(仅当原字段是"待填"占位);②尝试解析 CHANGELOG/RELEASES,解析不出来的行留空,完全解析不出就写"未发现可识别的版本记录格式";③把 evidence 数字原样填进两份产物的表格(agent 只做排版措辞,不算数);④把"仓里本来就在量的东西"整理成 `.bw/metrics.toml` 候选列表,标"候选,不绑定"。
3. **人确认**:北极星、对标、"在研版本"起点——任何脚本或 agent 都推不出来,不尝试填(见下)。人评审 MR 时确认整体靠不靠谱,是唯一确认动作,不逐字段勾选。

**四份仓文件产出物**(字段定义引用样例文件 [§6](../research/legacy-backfill-sample-buddy.md#6-渲染样例如果对本仓做一次回填产物长什么样),不重复贴;第七轮盘点后从"五个产物"收窄成四份仓文件 + 一行库缓存例外,`docs/plan/history.md`、`docs/releases.md` 独立"历史运作(回填)"节这两种旧说法都不再存在):

| 产出物 | 位置 | 字段 |
|---|---|---|
| a) 历史周文件 | `docs/plan/YYYY-Www.md`(与运作活①写的本周文件**同一套模板**,front matter `origin: backfill`,样例见 02 篇 §2.5) | 周目标(未发现就写"未发现——历史周没有周计划记录,不倒推")、业务活清单(未发现结构化清单就写"未发现";远端已关闭 issue 单独走下面这行)、本周运作(回填周早于 buddy 接入,写"不适用")、按周历史统计(合入 MR 数口径 B、提交数、目录 Top3、关闭 issue 数、当周版本) |
| b) 历史发版行 | `docs/releases.md`(**不新开分段**,直接按现有表头追加行,样例见 02 篇 §2.5) | 版本号、发版日、说明、包含的活、来源(标"回填 · git tag"等) |
| c) PROJECT.md 草稿字段 | `PROJECT.md` 补空字段 | 想做什么(若原为待填)、对标(留空待填)、北极星(留空待填) |
| d) 指标候选 | 写进 MR 说明或 `docs/metrics.md` 候选小节,不直接写入 `.bw/metrics.toml` | 候选指标名、数据来源、能否取到、备注 |

**唯一落库的例外**:回填探测到的远端 issue,原样同步进本机 `issue` 缓存表一行,`origin='backfill'`、`number`、`title`、`status`(照远端原样)、`closed_at`——这是 `issue` 表本来就有的缓存行为(02 篇 §2.1/§2.2),不是历史回填新建的表或列。四份仓文件产出(a-d)全部不写任何库表。

此前给 `docs/plan/history.md` 顶部加"累计贡献者:N 位"的做法随这份文件一起取消(六-3 已定 `history.md` 不建)——目前没有任何界面块要取这个数(总览原有的⑧块也已取消,08 篇已同步),按"没人取的不存"暂不产出;以后若某界面确实要展示贡献者数,应该先在那篇设计里定读哪份文件、怎么取,再回头给这里补产出逻辑,不预先猜一个没人读的字段。

**代码微重构建议也在首次盘点里顺手列为建议活**(第五轮,与 09 篇 §2.2 每周模式同一条规矩):`mode=first` 跑的时候若顺带发现明显的死码/超长文件/命名问题,同样只列成「建议活」草稿(类别「优化」,`origin='agent_split'`)写进 MR 说明,不直接改代码,人评审这次铺底 MR 时一并勾选要建的——不单独为老项目开一条"回填顺便重构"的例外通道。

**防伪规则**(逐条,证据均见样例文件):

1. 合入记录用双亲结构判定(`git log --merges`),不用消息文字匹配——文字匹配会漏掉手写合并提交(样例:71 条真实合入漏掉 23 条)。
2. 远端 MR/PR 数(口径 B)与本地合并提交数(口径 A)分开报——历史周文件「按周历史统计」表的"合入 MR 数"列用口径 B,口径 A 只在完全没有远端连接时当退化替代且要注明"本地口径"(样例两口径分别是 71 和 50,不可混用)。
3. 无标签无 CHANGELOG 就诚实写"未发现可回填的版本记录",绝不拿 commit 日期倒推版本号(buddy 自己仓就是这个真实边界样例)。
4. 批量关闭事件不当速率信号:数字照实呈现,单周关闭数占总关闭数比例 >50% 时附一句提示,不改数字本身(样例:44 个已关闭里 34 个集中一周)。
5. 每个数字标复算命令:evidence 文件里每个字段附一个"用什么命令算出来的"字符串,产物拼装时原样带出(呈现位置由 UI 定)。
6. 回填标记贯穿到底(本篇对 02 篇「回填标记怎么实现」的答案,第七轮改写,替换原 HTML 注释包段方案):历史周文件(`docs/plan/YYYY-Www.md`,`origin: backfill`)整份由回填生成,重跑同一周就整份覆盖那一份文件,不追加重复段落——和运作活①「这份文件归它管」是同一个契约,不需要额外标记机制;`docs/releases.md` 是回填内容与人写内容共存在**同一张表**里(不再有独立的"历史运作(回填)"节可以用 HTML 注释包住),重跑时按"版本号"列去重:该版本号已有一行(不论来源)就跳过或只更新说明字段,没有就追加新行,不产生重复行;库里 `issue.origin='backfill'` 是数据侧对应标记——`release` 表已取消,不再有 `release.origin` 列可打。

**buddy 今天缺的函数**(列名+模块+返回什么,不写实现,取自 legacy-backfill.md 最小集):

| 函数 | 模块 | 返回什么 |
|---|---|---|
| `list_tags`、`read_commits_since`、`commits_by_week`、`merges_by_week`、`author_distribution`、`top_dirs_by_week` | `crates/bw-engine/src/git_log.rs` | 依次:标签+hash+日期;带日期窗口的提交读取;按周提交数;按周合入提交数(口径A);作者→提交数;按周一级目录 Top3(带黑名单) |
| `list_closed_issues`、`list_merged_prs`、`list_tags`、`list_releases` | `crates/bw-engine/src/github.rs` | 依次:已关闭 issue 明细;已合并 PR 明细;远端标签;release 列表 |
| `list_closed_issues`、`list_merged_mrs`、`list_tags` | `crates/bw-engine/src/codehub.rs` | 同 github 侧,**命令未在真实 codehub 环境验证**(§4) |

这些函数只读、零副作用,和 `git_log.rs`/`evidence.rs` 今天"只读子进程、不解释不判断"的风格一致——判断留在流水线第一层的调用方代码。

**人确认与不点灯**:「在研版本」起点(**本篇对 legacy-backfill.md 开放问题 1 的答案**)——有真实标签/CHANGELOG 能识别出当前版本的老项目,`current_version` 直接取那个值;完全没有版本记录的老项目(如 buddy 自己),`current_version` 保持空,**不自动定成 v0.1**——`v0.1` 只属于待拍-04 的"新建项目"场景,老项目找不到版本历史就如实显示"未设置",直到人显式给一个。

回填不产生任何"要不要算战绩"的问题(本篇对 legacy-backfill.md 开放问题 3 的答案,第七轮改写,大幅简化):V4 没有战绩台账这回事(02 篇 §2.3)——"干没干成"永远直接看远端 MR 合没合入,没有一张表可以"记"或"不记"。回填进来的这批老 issue 沿用同一条判据:`origin` 只记"这行数据当初怎么进的库",永久不改;这条老 issue 后来若被真的指派、▶开工、走完一次真实运行,它算不算"干成了"看的是那次运行对应的远端 MR 合没合入,和 `origin` 是什么无关——不需要任何"回填要不要计入战绩"的排除规则,也没有界面特殊处理要做(原「界面影响留 §6」这句一并撤回,不再是开放问题)。

不点灯:回填产出的历史周文件、`docs/releases.md` 里标"回填"的版本行,只解释过去,不参与健康灯推导。唯一流入健康灯、也碰 git 合入记录的信号是**当前**周的"上周有交付"判据(08 篇已定),那是从当前记录实时现算的真实观测,与"回填一段历史给人看"是两次不同的计算,不重复。

### 2.5 一个 MR 与人介入

**A 刀落地后重写——这是这一刀最大的结构性偏差,单独说清楚。** 原文设计是:三步的所有提交落在同一条分支上,所有步骤跑完后统一开一个 MR(标题、评审要点清单、人评审 diff、合入后触发总览提示,一整套流程)。这套设计假定内核已经有「开分支」「开 PR」这两个动作——A 刀里这两个动作**根本不存在**:`crates/bw-v4/src/git.rs` 通篇只有只读查询(`is_repo`/`commit_count`/`tags`/`current_branch`/`is_dirty`/`root_commit_author` 等)和一个写操作 `commit_paths`(add 点名路径 + commit),没有建分支、没有 push、没有开 PR 的函数。这不是铺底这一张活自己的偏差,是**全部会写仓的动作**共有的现状:改名片、发版本、排期回写周计划文件、这里说的规范铺底,统统只是"直接改工作区文件,然后把改动过的路径提交到当前所在的那条分支"——没有分支、没有 MR、没有等人合入这一步。

铺底实际发生的事(A 刀只跑到第 1 步,见「## 2 · 设计」开头的范围说明):

1. `write_core_files` 把核心件写进工作区,`commit_paths` 把这次真写下去的路径提交到工作区**当前所在的那条分支**——不判断是不是空仓例外,两种情况走的是同一段代码。
2. 提交成功与否(`committed: bool`)如实写进 `Event::StandardBootstrapped`,不假装一定提交成功——工作区没有改动、或这条路径被 `.gitignore` 拒收,都会如实是 `false`。
3. 这张运作活③没有 PR 可评审(A 刀压根没有开 PR 这回事),走的是既有的「没有 PR → 人点『确认完成(人裁)』」路径——不是因为空仓例外才走这条路,是因为这一刀所有分支都走这条路。
4. 第 2/3 步(合并调整、历史回填)要真起 agent 会话产出「合并了哪些段」「回填了哪些数字」这类内容供人评审——这些**还没有实现**,自然也没有「三步拼成一份 MR 说明」这件事。

设计本身不改:「凡写仓都是活,写仓一律走分支+MR」仍然是目标状态,后面的刀要把 `git.rs` 补上建分支/push/开 PR 这几个函数,才能把本节原文描述的评审流程真的接起来。本节如实记的是当前偏差,不是新的设计决定。

### 2.6 对账与升级

**对账**(平时任何时候读,不只是铺底那一刻):比对 `.bw/standard.toml` 的 `enabled`/`extensions` 清单、`standard/` 当前版本(`STANDARD_VERSION`)、`.bw/managed.toml` 每个文件的指纹,分三类,一个文件只落一类:**缺**(`enabled` 里的类别没有对应记录,或记录了但磁盘没有该文件)、**过期**(记录版本小于当前 `STANDARD_VERSION` 且磁盘指纹与记录一致——没人碰过,是干净的升级候选)、**人改过**(磁盘指纹与记录不一致,不看版本号,永远不参与自动覆盖,升级时走人工路径)。

指纹算法(本篇对 02 篇开放问题 3 的答案):文件原始字节的 SHA-256,完整 64 位十六进制小写,存成 `"sha256:<64位hex>"`,逐字节哈希比较,不做语义/空白归一化——简单确定,MVP 不需要更聪明的 diff。对账是纯读操作,成本低不需要缓存,触发时机两处:①知识库屏渲染时现算(放进「资产」页签);②运作活②每周固定跑一遍,结论追加进那周 `docs/plan/` 尾段(09 篇负责调用)。

**升级**:人在知识库屏对"过期"文件点「看差异 → 升级」→ buddy 算文本差异(纯 diff),"人改过"里同时落后的文件一起列出并标"需要人工合并"→ 人确认要升级哪些文件(可只选一部分)→ "过期但没人改过"的文件建一张轻量活(和 `EditProjectCard` 同形状:无 agent 会话,建分支写新内容提 MR,`origin='human'`);"过期且人改过"的文件走一次真实 agent 会话(用第 2 步同一套合并原则),纳入同一张升级活一起提 MR → 合入后 `.bw/standard.toml` 与 `.bw/managed.toml` 一起更新。

**`docs/releases.md` 老项目解析健壮性,A 刀落地后改。** 原文说表头对不上标准格式就"整份不碰、不追加一行,只在这次回填 MR 说明里写一行提示"——这句话与 02 篇 §6 已经落地的答案矛盾,统一改成与 02 篇一致的口径:第 1 步「写核心件」已经先挡住了一层——如果铺底探测到项目接入前就有一份 `docs/releases.md`(非 buddy 管理),第 1 步按"人手改过不覆盖"规则直接跳过、不写标准骨架(§2.2 第 4 步)。到历史回填这一步,`release_file.rs`(02 篇 §3.3)只认标准 5 列表头(版本号/发版日/说明/包含的活/来源)下面的行:表头匹配就在这张表里追加回填行,按版本号去重不产生重复行;**表头对不上标准格式,不是"整份不碰",而是在文件末尾另起一段「## buddy 管理的发版记录」**,把回填的行落进这段新起的表里,项目原有的那份发版记录一个字都不动——回归测试 `crates/bw-v4/tests/repo_files.rs::foreign_release_table_is_never_written_into` 守着这条行为(02 篇 §6 已把这条从开放问题改成已答)。两张表要不要合并,留给人在 MR 说明里看到提示后自己决定,不是解析器该做的事。

### 2.7 `standard/` 正本与同事贡献

正本住仓根 `standard/`(01 篇已定,与 `crates/`/`docs/` 平级),结构照 [standard-module-draft.md](../standard-module-draft.md) §3 设计成八大类。

**A 刀落地后改。** 原文的目录树是八大类全部铺开的目标态,读起来像已经建齐。实况(B 刀之后):仓根 `standard/` 有下面这些目录/文件。`06-defaults/ops/` 已经建了——里面是三份自建运作 workflow 的正本;`CHANGELOG.md`、`07-cadence/`、`pond/` 仍然没建。

```
standard/
├── VERSION                    # 纯文本如 "4.0",bw-v4 的 standard::version() 读这个
├── README.md                  # 这套规范是什么,含"还没做的部分"如实说明
├── 01-charter/
│   └── PROJECT.md.tmpl
├── 02-agents/
│   ├── AGENTS.md.tmpl
│   └── CLAUDE.md.tmpl
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
│       └── standard-bootstrap-agent/merge-adjust/SKILL.md   # 运作活③需要 agent 的那一步
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
| `BackfillHistory { project_id }` | 单独重跑历史回填(不重跑第 1/2 步):建一张运作活("历史回填 · 重跑"),开新分支起一次「资产盘点」workflow 会话、传 `mode=first`(第五轮改动,见本篇导读与 09 篇 §2.2——第 2 层仍要 agent 读文本产出候选,不是无会话的轻量活),重走 2.4 节流水线,按 3.4 节的幂等规则重新产出(历史周文件整份覆盖对应周、`docs/releases.md` 按版本号去重更新) |
| `ReconcileStandard { project_id }` | 纯读:按 2.6 节算「缺/过期/人改过」三类,不建活不写仓,给知识库屏渲染用 |
| `UpgradeStandard { project_id, files }` | 人选中要升的文件后触发:按 2.6 节升级流程建轻量活(纯替换)或一次 agent 会话(需合并),最终提 MR |

---

## 3 · 工程对照

### 3.1 探测 + 写核心件落在哪(`crates/bw-v4/src/standard/bootstrap.rs`)

**A 刀落地后重写。** 原文把这两块功能放进 `crates/bw-engine/src/`(`standard_bootstrap.rs`/`managed_file.rs`/`backfill.rs` 三个新文件),同样是建立在「V4 摞在旧 crate 之上」这个不成立的前提上(见 §3.2「工程对照」总说明与 02 篇 §3.3)。实况是探测与写核心件这两件事都在 `crates/bw-v4/src/standard/bootstrap.rs` 一个文件里,真实结构如下(字段名照抄源码,不是示意):

```rust
// crates/bw-v4/src/standard/bootstrap.rs
pub struct BootstrapReport {
    pub written: Vec<String>,            // 真的写进去的路径
    pub skipped: Vec<(String, String)>,  // 跳过的路径 + 为什么跳
    pub skills: Vec<String>,             // 复制进 .claude/skills/ 的预置技能包路径
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

**`backfill.rs` 不存在。** 原文设想的 evidence 采集模块(汇总 `git_log.rs`/`evidence.rs`/`github.rs`/`codehub.rs` 写成本机 JSON,供第 3 步 agent 会话读取)属于历史回填——第 3 步这一刀完全没做(见「## 2 · 设计」开头的范围说明),自然没有对应的代码文件。这部分仍然是设计,不是已经开工又半途而废的模块。

### 3.2 编排落在哪(`crates/bw-v4/src/app/bootstrap.rs`)

**A 刀落地后重写。** 原文把编排放进 `crates/bw-app/src/standard_bootstrap.rs`,同样建立在旧前提上。实况是 `crates/bw-v4/src/app/bootstrap.rs` 里两个方法:

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
        // 没有「owned 直推 / 非 owned 开分支开PR / 自动触发第2/3步」这三分支——
        // §2.5 已经说明:A 刀里这三种情况现在做的是同一件事,提交到工作区
        // 当前所在的那条分支,第2/3步还没有能触发的代码。
        Ok(vec![Event::StandardBootstrapped { project_id, issue_id, files: report.written, committed }])
    }

    /// 纯读的对账:缺 / 过期 / 人改过。不建活、不写仓。
    pub(super) async fn reconcile_standard(&mut self, project_id: ProjectId) -> Result<Vec<Event>> { /* .. */ }
}
```

原文提到的 `auto_start_run`/`open_bootstrap_pr` 这两个分支目前都不存在——没有开分支、开 PR 的底层能力(§2.5),也没有自动触发第 2/3 步交互式运行的代码(§2.2 第 5 步已改写)。`CreateAutopilotTask { auto_run }` 这条给运作活②用的既有能力本身没问题,只是铺底这条编排路径还没有走到需要复用它的那一步。

### 3.3 `.bw/managed.toml` 与对账查询

```toml
[[file]]
path        = "AGENTS.md"
version     = "4.0"
fingerprint = "sha256:9f2a1c7ec3b8...(64位完整hex,此处省略中段仅为文档排版)"
```

对账判定(伪码):`enabled` 无对应 `managed` 记录、或记录了但磁盘无该路径 → `Missing`;磁盘指纹 == 记录指纹 且 `managed.version < STANDARD_VERSION` → `Stale`;磁盘指纹 != 记录指纹(不看版本号)→ `HumanEdited`;其余 → `UpToDate`。

### 3.4 回填产出怎么做到重跑幂等(第七轮改写,替换原 HTML 注释包段方案)

不再需要一套通用的"标记包段落"机制、也不需要一个独立的 `backfill_marker.rs`——四份产出物(§2.4 表格 a-d)各自靠自身文件结构解决幂等:

- **历史周文件**(a):`docs/plan/2026-W32.md` 这类文件本身按 ISO 周一一对应,重跑 `BackfillHistory` 时对某一周"整份重新渲染、原地覆盖"即可——这份文件从创建起就整份归回填管,不存在"文件里一半回填一半人写"的情况(人写的是本周/正在过的周,历史周不会有人手改)。
- **`docs/releases.md`**(b):`release_file.rs`(02 篇 §3.3)解析出的现有行按"版本号"去重——已存在同版本号的行就跳过(或按需要更新"说明"字段),不存在就追加一行,普通 Markdown 表格操作,不需要 HTML 注释包段落。
- **`PROJECT.md`**(c):只在字段仍是"待填"占位时才写(§2.2 第 4 步同一条"人手改过不覆盖"规则的自然复用)——重跑时如果上次已经填过(不管是回填填的还是人后来改的),这次不再覆盖。
- **指标候选**(d):每次都是全新写进这次 MR 说明的一段文字,不是持久文件里的一块,天然不存在"重跑要不要覆盖"的问题。

`managed_file.rs`(02 篇 §3.3/本篇 §3.1 已定义)只负责规范核心件(`AGENTS.md`/`.bw/*.toml` 等)的指纹对账,不管这四份回填产出——两套机制服务两类不同的文件,不合并成一套。

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

1. **第 1 步产物**:深链 `BW_OPEN=<样例项目名> BW_PANEL=kb`,stderr 见 `[BW_OPEN]`;分支(或 owned 情形的默认分支)上出现 `PROJECT.md`/`AGENTS.md`/`CLAUDE.md`/`.bw/issue-policy.toml`/`.bw/standard.toml`/`.bw/managed.toml`,`git show <分支>:.bw/managed.toml` 读到每个文件的指纹。
2. **探测正确性**:`sqlite3 <db> "SELECT title FROM issue WHERE kind='ops' AND workflow LIKE '%铺底%';"` 读回的标题应出现"含历史回填"(buddy 仓有 615 条提交、50 个已合并 PR)。
3. **第 3 步数字对照样例**:`docs/plan/2026-W33.md`/`docs/plan/2026-W34.md`(两份历史周文件)「按周历史统计」表里的"提交数"列应分别为 51/38,与样例文件 §2 一致;`docs/releases.md` 不应新增任何行(该仓无 tag/CHANGELOG,防伪规则 3 已定不倒推版本号),这次回填 MR 的说明里应能看到一句"未发现可回填的版本记录:仓内无 git 标签、无 CHANGELOG/RELEASES 文件"。
4. **回填不产生可查的"战绩"**:`SELECT origin, COUNT(*) FROM issue WHERE project_id='<pid>' GROUP BY origin;` 应见 `backfill` 档非零(对应真实远端 issue #78/#81);`sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name='workflow_credit';"` 应为空结果——库里根本没有这张表可查(02 篇 §2.1 已定),"干没干成"改看远端 MR 状态,不是查库。
5. **对账**:改动一个已铺底文件的一个字符后跑 `ReconcileStandard`,该文件应归类 `HumanEdited`;对账无直接对应表(纯读派生),用界面截图+手工核对指纹字符串读回。
6. **新库 schema 一次到位,不涉及老库迁移**:V4 不给存量库加列(02 篇 §2.7 已定:新库用新文件名,`schema.sql` 直接建全)——`sqlite3 <db> "PRAGMA table_info(issue);"` 对一个全新建出的 V4 库应直接看到 02 篇定义的全部 9 个扩展列,不依赖任何 `add_column_if_missing` 参与;这条验收不涉及"给存量库开新版程序"这类迁移场景(那是试点起才恢复的守卫,见 02 篇 §3.2)。

---

## 6 · 开放问题(≤5)

1. **`origin='backfill'` 的 issue 后续被真实推进后,界面上还要不要区分"这条曾经是回填的"**——本篇处理(2.4)是 `origin` 永久不改、真实进展看远端 MR 合没合入(没有战绩表可记,也没有"记/不记"的选择),数据层自洽,但总览"远端 issue 累计"这类计数会不会因此让人觉得数字对不上直觉,需要用户判断要不要在界面上加一层区分。
2. **目录黑名单(vendor/node_modules 这类噪声)要不要做成项目可配置**,还是本篇给的默认硬编码清单(`vendor/`、`node_modules/`、`dist/`、`build/`、`target/`、`.venv/`)先够用。
3. **决策记录(`docs/decisions/`)回填要不要留一个未来钩子**——本轮明确不做,但值不值得在 schema 或文件格式上预留位置,避免以后要做时多一次迁移。
4. **codehub 侧「已关闭 issue / 已合并 MR」明细列表命令尚未在真实 codehub 环境验证**——谁来协调第一次真实验证,是定稿后立刻做,还是等真有 codehub 老项目接入时再做。
5. **空仓例外下运作活③走"确认完成(人裁)"而非合入 MR 触发完成**——这条路径今天已存在于既有代码,但具体哪个命令/UI 入口触发它,本篇假设复用现成机制,04/05 篇设计会话屏/活详情交互时需要确认这个入口对运作活③同样适配。
