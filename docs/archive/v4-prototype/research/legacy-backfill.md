# 老项目历史回填预研:老仓里有什么、能捞出什么、可信到什么程度(2026-08-19)

> ⚠️ **历史档案(2026-08-20 归档)**。这是 V4 设计期的一篇源码级预研,**结论已经采纳进设计与代码**,留档只为考古「当时看了什么才这么定」。现状以 `docs/v4-prototype/design/` 对应篇与 `crates/bw-v4`、`crates/app-shell` 的源码为准;还没干的活只认 `docs/LEFTOVERS.md`。

> **30 秒导读**:这是一篇**预研**,不是设计文档,不改代码。背景:2026-08-19 内部专家评审提了一条期望——「老的项目进来,能根据老项目自己记录的千奇百怪的记录,完成 buddy 规范下的信息回填,总览能看到老项目之前的一些运作情况」,会议结论是「老项目捞回来是为了 MVP 出了之后组内所有项目就都纳入进来方便宣传」。这条已经写进设计事实源 [`../mvp-blueprint-draft.md`](../../../v4-prototype/mvp-blueprint-draft.md)(第 0 站、§2.6、待拍-27,已定)和 [`../standard-module-draft.md`](../../../v4-prototype/standard-module-draft.md)(第 3、7 类),但只定了「有这回事」,没定「老项目里到底有哪些记录、每种怎么捞、捞成什么样、可信度多少」——这篇补这个缺口,给后续详细设计 [`../design/03-standard-and-backfill.md`](../../../v4-prototype/design/03-standard-and-backfill.md)、[`08-overview-derivation.md`](../../../v4-prototype/design/08-overview-derivation.md)、[`09-ops-workflows.md`](../../../v4-prototype/design/09-ops-workflows.md) 打底。**状态:预研,待用户复核,不阻塞其它工作**。回填(本文和母文档共用的词,还没进 `CONTEXT.md`):把老项目自己五花八门的历史记录(提交、标签、远端 issue……)转成 buddy 规范认得的文件和数据行,标清楚「这是补录的历史,不是 buddy 里真做的活」。方法:读 buddy 现有代码(`crates/bw-engine/src/git_log.rs`、`evidence.rs`、`codehub.rs`、`github.rs`)确认哪些已经能读、哪些还没有;在 buddy 自己的仓(`forcegravity1989/loop-buddy`)上真跑一遍 git 和 `gh` 命令验证,证据见姊妹文件 `legacy-backfill-sample-buddy.md`(原件已删,见 git 历史)。看不懂的词查 [`../../../CONTEXT.md`](../../../../CONTEXT.md);代号索引查 [`../../code-schemes.md`](../../../code-schemes.md)(本文不新开任何代号)。

**一句话结论**:老项目里能捞的东西分三层可信度——git 本地历史(提交、合入记录、按周吞吐)和远端 issue/MR 列表这两类是**可复算的硬数据**,今天 buddy 只读了一小半(`git_log.rs`/`evidence.rs` 没有按日期分周、没有标签、没有合入记录识别;`github.rs`/`codehub.rs` 只有「未关闭 issue」,没有「已关闭」「已合并 MR」的列表读法,都要新增);README/CHANGELOG/docs 这类仓内文档要靠 agent 读文本才能提炼成一句话或一行版本记录,格式千奇百怪没有通用解析器;北极星、对标、「在研版本」起点这三样,任何历史记录都推不出来,必须人填。三条防线锁住诚实:**没有就留空,不拿 commit 日期硬造版本号**;**回填的东西全部带「回填」标记,单独一个 MR,人评审**;**回填的数据绝不流入健康信号灯**(唯一的例外——「上周有交付」——本来就是从 git 合入记录推的真实观测,和「回填历史」是两回事,不要混着理解)。在 buddy 自己仓上真跑一遍(615 次提交、0 个标签、50 个已合并 PR、44 个已关闭 issue,数字见样例文件)验证了这套判断,也真的踩到了「消息文字匹配漏掉 23 条手写合并提交」「批量关闭事件让周度指标失真」这两个如果不小心会做错的坑。

---

## 1 · 记录源清单:老项目里有哪些「千奇百怪的记录」,每种怎么捞

### 1.1 git 本地(不需要网络,任何 clone 下来的仓都有)

| 记录源 | 怎么读 | 能得到什么 | 可信度 | buddy 今天的现成读法 |
|---|---|---|---|---|
| 提交总数、首次/最近提交 | `git rev-list --count HEAD`;`git log --reverse \| head -1` / `git log -1` | 一个数字 + 两个时间戳 | 高(git 底层事实,不看提交信息内容) | **半有**:`evidence.rs::collect()` 有 `commit_count`(`git rev-list --count HEAD`),但没有首次提交时间;`git_log.rs::read_commits()` 只读「最近 N 条」,没有「最早一条」的专门读法(得把 `limit` 设成总数或反过来 `--reverse`) |
| 作者分布 | `git log --format=%an \| sort \| uniq -c` | 每个作者的提交数 | 高 | **没有**:两个现成模块都不统计作者分布,`git_log.rs::GitCommit` 结构体里有 `author` 字段但调用方目前只拿来显示单条提交,没有聚合 |
| 标签(版本) | `git tag -l`,配合 `git log -1 --format=%ai <tag>` 拿标签打在哪天 | 版本号 + 打标签日期 | 高,但**很多老项目根本没打过标签**(buddy 自己的仓就是 0 个,见样例文件 §0) | **没有**:两个模块都不读 `git tag`,这是一个纯粹的空白,要新增 |
| 合入记录 | **不要用消息文字匹配**(`grep "^Merge pull request"` 这类),改用 `git log --merges`(靠「这个提交有两个父提交」这一底层结构判定,不管提交信息怎么写) | 合并提交总数 + 每条的时间/作者/信息 | 高,前提是用双亲结构判定而非文字匹配——样例文件 §1 实测:文字匹配会漏掉 23/71 条手写的合并提交 | **没有**:两个模块都不做合入记录识别 |
| 按 ISO 周的提交/合入吞吐 | `git log --format=%ad --date=format:"%G-W%V"` 分组计数(ISO 周 = 周一到周日算一周的国际通用编号,如 `2026-W34`,和 buddy 现有周计划同一套编号) | 每周多少提交、多少合入 | 高(算法确定性,但如果只用 git 本地合入数当「合了几个 MR」会偏高——见 1.2 的口径对照) | **没有**:`git_log.rs::read_commits()` 只支持 `--max-count`,没有日期范围/分组参数 |
| 每周动过的一级目录 Top3 | `git -c core.quotepath=false log --name-only` 按周分组后取路径首段计数(`crates/foo.rs` → `crates`) | 每周「活跃度」最高的几个顶层目录 | 中——是「改动路径出现次数」的代理指标,不是去重后的「改了几个不同文件」,方法学要写清楚,否则容易被误读 | **没有**:两个模块都不做目录聚合;`evidence.rs::list_workspace_files()` 只列当前 HEAD 的文件清单,不是按历史时间切片 |
| 未提交改动(诚实信号) | `git status --porcelain` | 「声称已提交但树是脏的」这种可疑状态 | 高 | **有**:`evidence.rs::collect()` 的 `dirty_paths` |

### 1.2 仓内文件(需要 clone 下来读文件内容,不需要网络)

| 记录源 | 怎么读 | 能得到什么 | 可信度 | buddy 今天的现成读法 |
|---|---|---|---|---|
| README | agent 读取首段/关键句 | 「这项目是什么、想做什么」一句话,喂给 PROJECT.md 草稿 | 中——不是数字,是语言理解,不同项目 README 结构千差万别,agent 摘要质量决定可信度 | 无现成函数(不需要,agent 直接读文件即可,不是「采集器」范畴) |
| CHANGELOG / RELEASES / 已有的 `docs/releases.md` | agent 按行/按标题解析 | 版本号 + 日期 + 一句话说明 | 低到中——**格式千奇百怪,没有统一约定**(有的按 [Keep a Changelog] 规范写,有的就是流水账);解析不出来的行留空,不硬猜 | 无现成函数,需要 agent 尝试解析 |
| `docs/`、wiki 导出、ROADMAP/TODO | agent 扫描 + 摘要 | 已有的设计决策、待办线索 | 低——同上,格式不统一;而且这次范围**不含**决策记录回填(见 §6「不做什么」和开放问题 2) | 无现成函数 |
| `.github/` CI 配置 | 直接列文件名/workflow 名 | 「这个项目本来就在跑什么检查」的线索(CI 存在 ≠ 通过率数字,后者要另接 Actions API) | 中——只能证明「有」,不能直接给出历史通过率 | 无现成函数,列目录即可 |
| `package.json` / `Cargo.toml` 版本号 | 直接读字段 | 当前版本号(单点,不是历史时间线) | 高(如果字段存在) | 无现成函数,读取一个 TOML/JSON 字段的通用能力已有(`project_file.rs`/`metrics_file.rs` 都在读 TOML,可复用同一套 parser 习惯) |
| 已有的 AGENTS.md/CLAUDE.md | 检测存在与否 + 内容 | 「这是不是一个已经有 agent 工作约定的成熟项目」的判断依据 | 高 | 这条不是回填本体,是「规范铺底」运作活③第二步「合并调整」的输入(见 `standard-module-draft.md` 第 2 类),回填只在探测到有历史时接在它后面多做一步 |

### 1.3 远端(需要网络 + 认证,codehub/GitHub 连接器)

| 记录源 | 怎么读(今天已有的函数) | 还缺什么 | 可信度 |
|---|---|---|---|
| GitHub 未关闭 issue | `github.rs::list_open_issues(owner_repo)` → `gh issue list --state open --json number,title,body` | **缺已关闭 issue 列表**(含关闭时间)。`gh` CLI 本身现成支持:`gh issue list --repo <owner>/<repo> --state closed --limit 500 --json number,title,closedAt,createdAt,labels`(样例文件 §4 实测:本仓 44 个已关闭、2 个未关闭) | 高,`gh` 直读 GitHub API |
| GitHub 已合并 PR | 没有专门函数,但 `github.rs::open_pr_for_branch()` 能查单个分支的开放 PR | **缺「按时间列出全部已合并 PR」**。现成命令:`gh pr list --repo <owner>/<repo> --state merged --limit 300 --json number,title,mergedAt,body`(关联的 issue 号要么从 `body` 里的 `Closes #N` 正则提,要么用 `gh pr view <n> --json closingIssuesReferences` 更准) | 高 |
| GitHub tag / release | 完全没有 | `gh api repos/<owner>/<repo>/tags --jq '.[].name'`;`gh release list --repo <owner>/<repo>` | 高,前提是项目真的打过 tag/发过 release(buddy 自己的仓两样都没有——见样例文件 §0/§5) |
| GitHub milestone | 完全没有 | `gh api repos/<owner>/<repo>/milestones` | 高,前提是用了 milestone 功能(buddy 自己的仓实测 0 个) |
| codehub(GitLab 系)未关闭 issue | `codehub.rs::list_open_issues(host, path)` | 同 GitHub,只有「未关闭」 | 高 |
| codehub issue/MR **计数**(含 closed/merged) | `codehub.rs::collect_count(host, path, "issues:closed"/"mrs:merged", today)` ——这个函数其实已经支持任意 state(`opened`/`closed`/`merged`/`all`),只是只返回**一个数字**,不返回明细列表 | **缺明细列表**(标题、时间)。对应 GitLab v4 REST 的端点风格大致是 `GET /projects/:id/issues?state=closed` 和 `GET /projects/:id/merge_requests?state=merged`,`codehub-cli` 大概率有 `issue list --state closed`/`mr list --state merged` 这类子命令(参照 `create_mr` 里已经在用的 `mr list --source-branch ... --state opened` 写法类推)——**这条未在真 codehub 上验证,如实标注**(本机没装 `codehub-cli`,是内网工具,这次预研环境拉不到) |
| codehub tag / release | 完全没有 | 未核实对应命令,GitLab v4 REST 一般是 `/projects/:id/repository/tags` 和 `/projects/:id/releases`——**同样未在真 codehub 上验证** | 未知,标「未验证」而不是假设「能」 |

### 1.4 项目群历史(配了才有)

「上周在讨论什么」这类信息只能从项目群历史里来,是「上周实际发生了什么」的参考,**不进仓、不进库**——另有一篇预研专门写这块([`chat-group.md`](../../../v4-prototype/research/chat-group.md)),本文不展开,只在这里点一句:老项目历史回填的四种原料(git、仓内文档、远端 issue/MR、项目群)里,群历史是唯一「配了才有、且不落地」的一种。

---

## 2 · 回填成什么:每个产物的字段与样例

对齐 `standard-module-draft.md` 定义的规范骨架,回填不新造文件类型,只是把这些已经定好位置的文件/数据行填上「历史」内容,并且**每一处都带「回填」标记**。下面每个产物给字段定义 + 一段用 buddy 自己仓渲染出来的真实样例(完整版在 `legacy-backfill-sample-buddy.md`(原件已删,见 git 历史),这里摘要);buddy 自己的仓恰好是一个「没有 tag、没有 CHANGELOG」的老项目,所以下面能看到「有数据就填、没数据就诚实留空」两种情况都真实发生。

**a) `docs/releases.md` 历史段** —— 字段:版本号、日期、来源徽记(`回填自 tag` / `回填自 CHANGELOG` / `回填自远端 release`)、一句话说明(能摘到才填)。样例(buddy 仓无 tag/无 CHANGELOG,如实为空):
```markdown
## 历史运作(回填)
未发现可回填的版本记录:仓内无 git 标签、无 CHANGELOG/RELEASES 文件、
提交信息里也没有识别出版本号模式。上面「现在用的」表是人工维护的现状记录,
不受本段管理。
```

**b) `docs/plan/history.md`** —— **不是周计划,是「按周的历史运作」**:字段 = 周(ISO 周区间)、合入 MR 数(远端口径,不是本地合并提交数——见样例文件 §1 的口径对照)、提交数(git)、动过的目录 Top3、关闭 issue 数(远端)、当周版本(若该周内 `docs/releases.md` 有新行)、来源尾注。样例(最近 2 周,完整 4 周见样例文件 §6.2):
```markdown
| 周 | 合入 MR 数 | 提交数 | 动过的目录 Top3 | 关闭 issue 数 | 当周版本 | 来源 |
|---|---:|---:|---|---:|---|---|
| 2026-W33(08-10~08-16) | 3 | 51 | crates、docs、iterations | 0 | — | 回填自 github / git |
| 2026-W34(08-17~08-23,进行中) | 3 | 38 | crates、docs、plan | 0 | — | 回填自 github / git |
```

**c) PROJECT.md 草稿** —— 字段:名称、想做什么(从 README 首段提炼)、对标(留空)、北极星(留空)、项目信息(仓地址、在研版本——无 tag 时在研版本也留空待人指定)。样例:
```markdown
- 想做什么:单人构建者的 Rust 原生桌面工作台;用 AI 时代的方式一步步把项目管理体系
  搭起来,走完拥有一套可复制的项目管理方法,而不只是一块看板。（摘自 README.md 首段)
- 最像的对标:__待填__
- 三个月长成什么样(北极星):__待填__
```

**d) `.bw/metrics.toml` 候选** —— 只列「仓里本来就在量的东西」,不绑定成正式指标:CI 是否配置(workflow 文件存在与否)、测试规模(若仓自己有文档说明)、发布频率(样本太小时如实说「算不出」)、issue 关闭速率(标注「批量关闭会失真」——buddy 自己仓的真实数据里,44 个已关闭 issue 有 34 个是同一周批量关的,见样例文件 §4)、跟踪文件规模。

**e) 库里 issue 行** —— 字段:`origin = backfill`(新增列,今天 `bw-store` 的 `issue` 表还没有这一列,V4 落地时要按「schema 迁移双守卫」的既有纪律——同时改 `schema.sql` 和加 `add_column_if_missing` 守卫,不能只改一处)、`number`(远端号)、`title`、`status`(照远端原样映射,不重新判断)、`closed_at`(未关的留空)。样例(buddy 仓两条真实远端 issue):
```
origin=backfill  #78  文档可读性 · plan/12 补两处小对齐……   closed  2026-08-06T02:13:07Z
origin=backfill  #81  术语治理 · 完成 ADR0001 遗留改名……     open    —
```
**这些行不算任何人 / workflow 的战绩**——它们是历史,不是 buddy 里干出来的活。

---

## 3 · 自动 / agent / 人确认:谁来做

| 类别 | 判定标准 | 例子 | 谁来做 |
|---|---|---|---|
| **纯脚本能算** | 输出是确定性的,不需要理解语言含义,同样的输入永远得到同样的输出 | 提交总数、首末提交日、作者分布、标签列表、按双亲结构判定的合入记录、按 ISO 周的目录 Top3、远端 open/closed issue 计数与列表、merged PR/MR 计数与列表 | 脚本 / 连接器直接跑,不需要 agent 介入判断 |
| **需要 agent 读文本判断** | 输入是自然语言,没有统一格式,需要理解语义才能提炼 | README 首段 → 一句话「想做什么」;CHANGELOG/RELEASES 千奇百怪的格式 → 解析成「版本+日期+说明」一行;`docs/` 里可能存在的决策线索 | agent 尝试,产物仍要人在评审 MR 里复核——不是自动合入 |
| **必须人确认** | 任何历史记录都推不出来的主观判断,或者会影响后续记账口径的关键选择 | 北极星(项目长期目标);最像的对标;「在研版本」的起点该定在哪个 commit/tag(没有 tag 的老项目尤其关键——见 §7 开放问题 1);回填整体是否靠谱(MR 评审这一步) | 人,通过评审老项目那一份 MR 来确认(和 `standard-module-draft.md` 待拍-20「纳入管理就要相信管理」的既有原则一致:铺底本身自动,但改动要过评审) |

---

## 4 · 可信度与防伪

1. **每个数字都能重算**:回填不接受「agent 说的」当数据来源,只接受「能用一条 git 或 `gh api` 命令重新跑出来」的数字。样例文件 `legacy-backfill-sample-buddy.md`(原件已删,见 git 历史) 把每一步用到的命令都写在文件里,就是为了证明这条(呼应 `CLAUDE.md`「核心纪律」第 1 条「报告不代答,读回为证」,回填只是把这条纪律用到「历史数据」这个新场景)。
2. **绝不发明数据**:git/远端/群里没有的东西就空着。buddy 自己的仓就是活样例——没有 tag、没有 CHANGELOG,§2(a)的版本时间线渲染出来就是「未发现可回填的版本记录」这句诚实的空,不会因为找不到 tag 就拿 commit 日期硬造一个版本号。
3. **回填标记贯穿到底**:文档段落带「(回填)」角标(如 `docs/plan/history.md` 的标题本身和每行的来源尾注),库里 `issue.origin = backfill`。这个标记还有第二个作用——**支持幂等重跑**:重跑只覆盖上次回填生成、带标记的内容,不碰人后来手改过的部分,这和 `standard-module-draft.md` 第 2 类「AGENTS.md 升级时 buddy 只替换带标记的段」是同一套机制,详细设计可以直接复用,不必另发明一套。
4. **不点灯**:总览「历史运作(回填)」块本身**不参与健康信号推导**——它只呈现,不判断好坏。唯一允许流入 health 灯的信号是第 1 站 health 规则的输入 (c)「上周有交付(合入或发版)」,但这条本来就是从**当前**的 git 合入记录实时推导的真实观测,和「回填一段历史给人看」是两件不同的事,不要因为两者都碰 git 合入记录就混为一谈——回填面向过去、只解释,health 面向现在、真实观测才能点灯。

---

## 5 · 真跑一遍的证据

在 buddy 自己的仓(`forcegravity1989/loop-buddy`)上按 §1、§2 的方法真跑了一遍,完整数字、命令、渲染样例见 `legacy-backfill-sample-buddy.md`(原件已删,见 git 历史)。三个最值得记住的发现已经写进 §1.1(用双亲结构而非文字匹配识别合入记录)、§1.2/§2(a)(没有 tag 的老项目版本时间线该诚实留空)、§2(d)(批量关闭事件会让「issue 关闭速率」这类周度指标失真,不能直接点灯)。

---

## 6 · 失败与边界

- **仓太大(万级提交)**:`commit_count`/首末提交/作者分布/标签这类是 O(1) 或 O(提交数) 的轻量统计,代价可控;但「按周分组的目录 Top3」是 O(提交数 × 每次改动文件数),在真实万级提交的仓上可能要跑到分钟级。建议:只对**最近 N 周**(比如最近 26 或 52 周)做精细化的按周目录统计,更早的历史只给累计数字、不细分到周。这也意味着 `git_log.rs::read_commits()` 现在只有 `limit`(条数上限)、没有 `--since` 日期窗口参数,是要新增的能力。
- **monorepo(单仓多项目)**:MVP 按「整仓 = 一个项目」处理,不做仓内子目录级的项目切分,如实标注这个局限,不假装能自动识别子项目边界。
- **没有远端权限**:探针(`probe_repo`/`codehub.rs::probe`)失败或未认证时,远端相关字段(issue/MR/tag/release)全部留空、不阻塞 git 本地那部分回填仍然完成——延续 `github.rs::probe_repo` 文档注释里「探不通就如实报错,绝不伪造已同步」的既有原则。
- **CHANGELOG 格式认不出来**:留空,交给 §3「需要 agent 判断」那一类去尝试解析,解析不出来的版本行宁可不生成,不能编一行凑数。
- **要不要可重跑**:建议**幂等**——重跑只覆盖上次回填生成、带「(回填)」标记的段落,不动人后来编辑过的内容。详见 §4 第 3 条,和现有的 AGENTS.md 升级机制复用同一套「标记段落、只替换标记内的内容」的做法。

---

## 推荐 MVP 做的最小集 / 不做什么

**最小集(建议第一版做)**:
1. git 本地:提交总数、首末提交日、作者分布、标签列表(有就列、没有就空)、按双亲结构判定的合入记录总数、最近 8-10 个 ISO 周的提交数与目录 Top3。
2. 远端:open + closed issue 的计数与明细列表(标题、开/关时间);merged PR/MR 的计数与明细列表(标题、合入时间)。GitHub 侧现成命令直接可用(`gh issue list --state closed`、`gh pr list --state merged`),buddy 只需要给这两条各包一个函数;codehub 侧 `collect_count` 已经支持 closed/merged 状态的**计数**,缺的是**明细列表**,同样需要新增函数(且要先在真 codehub 环境验证命令,§1.3 已标注未验证)。
3. 产物:`docs/releases.md` 历史段(仅当能读到 tag 或 release 列表时才生成内容,否则如实写「未发现」)、`docs/plan/history.md` 最近 4-8 周、issue 行回填(仅当远端有 issue 时)、PROJECT.md 草稿里「想做什么」一句(agent 读 README)。

**不做(本次范围外,明确留白)**:
1. CHANGELOG/RELEASES 自由格式的通用解析器——先靠 agent 按最佳努力尝试,不建规则引擎。
2. 项目群历史回填——另一篇 chat-group 预研在做,不是这次的范围。
3. issue/MR/README 正文的全文搬运——只要标题、时间、关联号这类结构化字段,不把远端正文整段复制进仓。
4. 决策记录(`docs/decisions/`)回填——commit 信息和文档正文里的决策线索零散、没有统一格式,值不值得做、怎么做,留成开放问题 2,本次不纳入。
5. monorepo 子项目级回填——MVP 整仓当一个项目处理。

---

## 留给详细设计的开放问题(≤5)

1. **「在研版本」起点谁来定、怎么定**——像 buddy 自己这样完全没有 tag 的老项目,回填时「在研版本」要不要默认设成从铺底那天开始的 `v0.1`、把此前的一切历史都归入「回填前史」不算版本?还是要求人从提交历史里指定一个 commit 当「事实上的起点」?
2. **决策记录要不要回填、从哪提炼**——`docs/decisions/` 是扩展件而非核心件,commit 信息和文档正文里的决策线索零散、没有统一格式;本文建议本次不做,但值不值得留一个「以后可以做」的钩子,需要拍板。
3. **`origin=backfill` 的 issue 行以后能不能被继续推进**——老项目一个「远端仍是 open、回填进来」的 issue,后续被指派、跑、走完整生命周期时,状态该不该保留「回填」标记,还是从此变成一张普通 buddy issue?这影响记账口径要不要区分「历史遗留活」和「新纳入的活」。
4. **目录 Top3 统计要不要一个噪声黑名单**——如果某个老仓把 `vendor/`、`node_modules/` 这类通常该被忽略的目录真的 track 进了 git(常见于历史悠久、`.gitignore` 补得晚的项目),按目录聚合出来的 Top3 会被这些噪声目录占满,要不要一份默认黑名单?
5. **回填标记的具体实现**——文档里的「(回填)」是纯文字角标,还是要用 HTML 注释包住整段以支持程序化识别「这段是回填生成的,重跑时可覆盖」?这决定了 `03-standard-and-backfill.md` 详细设计里「幂等重跑」这条要怎么写覆盖逻辑。
