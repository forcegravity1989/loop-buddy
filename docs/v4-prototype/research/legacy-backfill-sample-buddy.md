# 老项目历史回填 · 样例证据(在 buddy 自己的仓上真跑一遍,2026-08-19)

> **30 秒导读**:这是 [`legacy-backfill.md`](legacy-backfill.md)(老项目历史回填预研正文)第 5 条要求的**证据文件**——不是设计,是「拿 buddy 自己的仓(`forcegravity1989/loop-buddy`,GitHub 公开仓)当一个『老项目』,把预研正文第 2 节说的几个回填产物真的渲染出来看看长什么样」。**每个数字都标了它是用哪条命令算出来的,复制粘贴就能重算**;算不出来的字段(比如版本时间线)如实留空,不编。跑的时间:2026-08-19,仓当时的 `HEAD` 是 `211178c`(main 分支,PR #103 刚合入)。给谁看:复核预研结论的人、以后写 [`../design/03-standard-and-backfill.md`](../design/03-standard-and-backfill.md) 详细设计的会话。

---

## 0 · 基本事实(git 本地,零远端依赖)

| 字段 | 数值 | 命令 |
|---|---|---|
| 累计提交数 | **615** | `git rev-list --count HEAD` |
| 首次提交 | **6e8495e**,2026-06-21T00:35:39+08:00,作者 Gravity,「Builders' Workbench: 初始化」 | `git log --reverse --format="%H %ad %an %s" --date=iso-strict \| head -1` |
| 最近一次提交 | **211178c**,2026-08-19T10:52:08+08:00,作者 forcegravity1989,「Merge pull request #103 …」 | `git log -1 --format="%H %ad %an %s" --date=iso-strict` |
| 标签(tag) | **0 个** | `git tag -l \| wc -l` |
| 作者分布 | Gravity 346、yuzhup 211、forcegravity1989 42、Claude 9、github-actions[bot] 6、dong 1(合计 615,与累计提交数一致) | `git log --format=%an \| sort \| uniq -c \| sort -rn` |
| 跟踪文件总数 | 522(其中 `docs/*.md` 86 个) | `git ls-files \| wc -l`;`git ls-files \| grep -c '^docs/.*\.md$'` |

**没有标签,是真的没有,不是没拉到**——`git tag -l` 空输出。连带确认:`git log --format=%s` 里也搜不到任何 `v数字.数字.数字` 或 `chore(release)` 风格的提交(`git log --format=%s | grep -icE "v[0-9]+\.[0-9]+\.[0-9]+"` → `0`),仓根也没有 `CHANGELOG.md`/`ROADMAP.md`/`TODO.md`。**这意味着 git 原生能回填出的「版本时间线」在这个仓上是空的**——见 §3。

## 1 · 合入记录:两种口径,数字不一样,别混着报

**口径 A(git 本地,按提交的双亲结构判定,不看提交信息文字)**:

```
git log --merges --oneline | wc -l          # → 71(总的「合并提交」数)
git log --grep="^Merge pull request" --format=%H | wc -l   # → 48(GitHub 网页/CLI 走 squash 或 merge 按钮留下的标准文案)
git log --grep="^Merge branch" --format=%H | wc -l          # → 8(本地 `git merge` 留下的标准文案)
```

71 − 48 − 8 = **15** 条既不是「Merge pull request」也不是「Merge branch」的合并提交——都是真实存在的合入,只是提交信息是手写的,比如:

```
合入 origin/main(d6678d2,含 PR #101 V3 修复与文档边界):代码冲突以拆分结构为骨、V3 改动逐段搬进新文件,文档冲突以 main 的文档边界为准
Merge origin/main into v1 · 73 commit 并入，6 处冲突逐一核对解决
merge: bw-complete-form(multica R1 Issue层+R2 技能复利)并入九实体主线
```

**这就是任务背景说的「千奇百怪的记录」的一个真实样本**:如果回填脚本只认「消息以 `Merge pull request` 开头」这一种模式,71 条合入记录会漏掉 23 条(8+15)。**buddy 该用双亲结构(`git log --merges`,即"这个提交有两个父提交"这一 git 底层事实)判定"是不是一次合入",不该用消息文字匹配**——后者认不出手写的合并提交。

**口径 B(GitHub 远端,`gh pr list --state merged`)**:

```
gh pr list --repo forcegravity1989/loop-buddy --state merged --limit 300 --json number,mergedAt
```

→ **50 个已合并 PR**。50 ≠ 71,差的 21 条是本地 `git merge`/手工冲突合并但没走 GitHub PR 流程的合入(常见于两个功能分支互相合并、或直接推 `main`)。**「本周合了几个 MR」这个字段,回填要用口径 B(远端 merged PR/MR 计数),不要用口径 A(本地合并提交数)**——口径 A 会把「同一个人自己两条分支互相合一下」也算成一次「合入」,和团队理解的「合了一个 MR」不是一回事;口径 A 只在完全没有远端(纯本地仓、没配 GitHub/codehub)时当退化替代品用。

## 2 · 按 ISO 周的吞吐(最近 10 周,2026-W25 ~ 2026-W34)

**ISO 周**是国际通用的周编号规则(周一到周日算一周,写作 `年-W周数`,如 `2026-W34`)——用它而不是自然月,是因为 buddy 现有的「本周计划」本来就按 ISO 周分,回填的周吞吐要能直接对上同一套编号。这个仓从第一次提交到现在(2026-08-19)刚好跨 10 个 ISO 周,所以下表就是这个仓的全部生命周期,没有截断:

| ISO 周 | 日期范围 | 提交数(git) | 合入提交(git,口径 A) | 合并 PR(远端,口径 B) |
|---|---|---:|---:|---:|
| 2026-W25 | 06-15~06-21 | 3 | 0 | 1 |
| 2026-W26 | 06-22~06-28 | 14 | 1 | 0 |
| 2026-W27 | 06-29~07-05 | 2 | 1 | 1 |
| 2026-W28 | 07-06~07-12 | 17 | 2 | 3 |
| 2026-W29 | 07-13~07-19 | 85 | 5 | 3 |
| 2026-W30 | 07-20~07-26 | 130 | 17 | 12 |
| 2026-W31 | 07-27~08-02 | 82 | 10 | 7 |
| 2026-W32 | 08-03~08-09 | 193 | 25 | 17 |
| 2026-W33 | 08-10~08-16 | 51 | 5 | 3 |
| 2026-W34(进行中)| 08-17~08-23 | 38 | 5 | 3 |

提交数校验:3+14+2+17+85+130+82+193+51+38 = 615,与 §0 累计提交数一致。合并 PR 校验:1+0+1+3+3+12+7+17+3+3 = 50,与口径 B 总数一致。

```
git log --format="%ad" --date=format:"%G-W%V" | sort | uniq -c              # 提交数按周
git log --merges --format="%ad" --date=format:"%G-W%V" | sort | uniq -c     # 合入提交按周(口径 A)
gh pr list --repo forcegravity1989/loop-buddy --state merged --limit 300 \
  --json number,mergedAt   # 拉全量后按 mergedAt 的 ISO 周分组(见下方 python 片段)
```

按周分组 `mergedAt` 用的 python(命令行里没有内置的「按 ISO 周分组」,这段脚本本身就是回填要写的那部分逻辑,贴出来是为了证明这条数字怎么来的,不是凭空报的):

```python
import json, collections
from datetime import datetime
rows = json.load(open("merged_prs.json"))   # gh pr list ... > merged_prs.json 的输出
weekly = collections.Counter()
for r in rows:
    dt = datetime.strptime(r["mergedAt"], "%Y-%m-%dT%H:%M:%SZ")
    iso = dt.isocalendar()
    weekly[f"{iso[0]}-W{iso[1]:02d}"] += 1
```

## 3 · 每周动过的一级目录 Top3(最近 4 周,2026-W31 ~ 2026-W34)

方法:`git log --name-only`(不产出总结,只吐每个提交改了哪些文件)按 ISO 周分组后,把每个改动路径的第一段(`crates/foo.rs` → `crates`)计数——**这是「该周提交里出现过的文件路径次数」,同一个文件被多次提交改到会被算多次,是活跃度的代理指标,不是「本周有多少个不同文件被改过」这种去重后的数字**,回填产物里要把这条方法学写清楚,不能让人误读成后者。真跑时要加 `-c core.quotepath=false`,否则含中文字符的路径会被 git 加引号,统计出来的「目录名」会是带引号的乱码(实测踩到过,已修正)。

```
git -c core.quotepath=false log --pretty=format:"§%H\t%ad" --date=format:"%G-W%V" --name-only
```

（`§` 是记录分隔符,取一个不会出现在文件路径或提交信息里的字符;拿到输出后按 `§` 切开、每段的第一行是 `hash\t周`、后面每行是该提交改的一个文件路径,按第一段目录聚合计数。)

| ISO 周 | 提交数 | Top3 一级目录(改动路径出现次数)|
|---|---:|---|
| 2026-W31 | 82 | `examples`(256)、`crates`(210)、`docs`(27) |
| 2026-W32 | 193 | `crates`(277)、`docs`(125)、`plan`(22) |
| 2026-W33 | 51 | `crates`(89)、`docs`(75)、`iterations`(4) |
| 2026-W34 | 38 | `crates`(165)、`docs`(131)、`plan`(7) |

## 4 · 远端 issue:开 / 关数,标签

```
gh issue list --repo forcegravity1989/loop-buddy --state open   --limit 500 --json number   # → 2 个
gh issue list --repo forcegravity1989/loop-buddy --state closed --limit 500 --json number   # → 44 个
```

已关闭的 44 个按关闭时间(`closedAt`)分周,只落在 3 个周里:

| ISO 周 | 关闭 issue 数 |
|---|---:|
| 2026-W30 | 34 |
| 2026-W31 | 6 |
| 2026-W32 | 4 |
| 2026-W33 / W34 | 0(真实的零,不是没拉到——两周里 `closedAt` 落在这两周的记录数就是 0)|

W30 一次关掉 34 个,是一批「文档可读性」清理票集中关闭造成的(`gh issue list --state closed --json number,title,closedAt` 能看到这批标题都带「文档可读性」前缀)。**这是「issue 关闭速率」这类周度指标的一个真实陷阱**:如果不看具体分布就直接拿「本周关了几个」当健康信号,W30 会显得异常活跃、W33/W34 会显得停滞,但实际上只是历史遗留票被集中批量清理——回填产物只如实呈现这个数字,不替它加解释、更不能让它流入健康信号灯(呼应正文 §4 的「不点灯」规则)。

标签:46 个 issue 里 40 个带标签,但标签是 `ready-for-agent`/`model:sonnet-5`/`model:opus-4.8`/`model:fable-5` 这类**给 agent 路由用的标签,不是「bug/feature/chore」这类分类标签**(`gh issue list --state all --limit 500 --json number,labels` 实测)。这说明「远端 issue 标签」这个原料不能假设一种统一含义,回填时原样带进去就行,不要试图把标签翻译成某种通用分类。

milestone:`gh api repos/forcegravity1989/loop-buddy/milestones --jq length` → **0**,如实没有,不编。

## 5 · docs/releases.md 里已有的版本行

本仓 [`../../releases.md`](../../releases.md)「现在用的」表里只有一行:

| 版本号 | 出包日 | 说明 |
|---|---|---|
| 0.3.0-v3 | 2026-08-14 首包;2026-08-17 起按 V3-use-fix 重出 | 第一份给同事的 Windows 安装包 |

**这一行是人工维护的现状记录,不是本次回填能力生成的**——本仓从头到尾没有 git tag、没有 CHANGELOG、commit 信息里也没有版本号模式(§0 已核实),所以如果今天对这个仓跑「历史回填」,git 原生能产出的版本时间线是**空的**,唯一的版本信息来自这一行人工文档,而人工维护的现状记录不属于回填要生成的内容(回填生成的是标「回填」的历史段,不能覆盖或冒充人工现状表)。**这是一个诚实的边界样例**:老项目不一定有干净的版本历史可捞,捞不到就该显示空,不能因为找不到 tag 就拿 commit 日期硬造一个版本号出来。

顺带一个巧合但有意义的对照:2026-08-14(0.3.0-v3 首包日)落在 §2 的 **2026-W33**,2026-08-17(重出日)落在 **2026-W34**——如果这个仓真的有 tag,`docs/plan/history.md` 按周渲染时,W33/W34 那两行的「当周版本」字段就应该显示 `0.3.0-v3`;因为本仓没有 tag,§6 的渲染样例里这一列如实留空。

## 6 · 渲染样例:如果对本仓做一次回填,产物长什么样

### 6.1 `docs/releases.md` 历史段(样例)

因为没有 tag、没有 CHANGELOG,这个仓的历史段渲染出来是**空的**——如实展示:

```markdown
## 历史运作(回填)

未发现可回填的版本记录:仓内无 git 标签、无 CHANGELOG/RELEASES 文件、
提交信息里也没有识别出版本号模式。上面「现在用的」表是人工维护的现状记录,
不受本段管理。
```

### 6.2 `docs/plan/history.md`(样例,最近 4 周)

```markdown
# 历史运作(回填)

> 本文件由老项目历史回填生成,只解释历史,不是本周计划,不算任何人 /
> workflow 的战绩。数据来源标在每行末尾;回填绝不发明数据,拉不到的字段留空。
> 生成时间 2026-08-19,数据截至 HEAD 211178c。

| 周 | 合入 MR 数 | 提交数 | 动过的目录 Top3 | 关闭 issue 数 | 当周版本 | 来源 |
|---|---:|---:|---|---:|---|---|
| 2026-W31(07-27~08-02) | 7 | 82 | examples、crates、docs | 6 | — | 回填自 github / git |
| 2026-W32(08-03~08-09) | 17 | 193 | crates、docs、plan | 4 | — | 回填自 github / git |
| 2026-W33(08-10~08-16) | 3 | 51 | crates、docs、iterations | 0 | — | 回填自 github / git |
| 2026-W34(08-17~08-23,进行中) | 3 | 38 | crates、docs、plan | 0 | — | 回填自 github / git |
```

（「当周版本」全部留空,理由见 §5——不是模板漏填,是这个仓确实没有可回填的版本标记。若某仓有 tag 或 CHANGELOG,这一列会显示落在该周的版本号。)

### 6.3 PROJECT.md 草稿(样例,从 README 首段提)

本仓 [`README.md`](../../../README.md) 第一句实际写的是:

> 单人构建者的 Rust 原生桌面工作台(Dioxus 0.7 / wry WebView,macOS + Windows)。产品命题一句话:**用 AI 时代的方式,一步步把一个项目的管理体系搭起来;走完,你拥有一套可复制的项目管理方法,而不只是一块看板**。

回填草稿把这句摘出来当「想做什么」,「对标」「三个月长成什么样(北极星)」两段人工留空:

```markdown
# PROJECT.md(回填草稿,待人补全)

- 名称:loop-buddy(Builders' Workbench)
- 想做什么:单人构建者的 Rust 原生桌面工作台;用 AI 时代的方式一步步把项目管理体系
  搭起来,走完拥有一套可复制的项目管理方法,而不只是一块看板。
  （摘自 README.md 首段,回填自仓内文档)
- 最像的对标:__待填__
- 三个月长成什么样(北极星):__待填__
- 项目信息:仓 = github.com/forcegravity1989/loop-buddy;在研版本 = __待填__
  （本仓无 tag,无法回填「在研版本」起点,需人指定)
```

### 6.4 `.bw/metrics.toml` 候选(样例,只列候选不绑定)

| 候选指标 | 数据来源 | 本仓能不能取到 | 备注 |
|---|---|---|---|
| CI 是否通过 | `.github/workflows/ci.yml`、`codegraph.yml` 两条流水线 | 能(workflow 文件存在,真实通过率要接 GitHub Actions API,本次未拉) | 候选,不绑定 |
| 测试规模 | 仓自述「约 2,000 行内联测试」(`CLAUDE.md`「核心纪律」第 6 条原文) | 能(文档自陈,未独立核实具体行数) | 候选,不绑定 |
| 发布频率 | `docs/releases.md`「现在用的」表 | 目前只有 1 行,样本太小算不出频率 | 候选,不绑定 |
| issue 关闭速率 | 远端 issue `closedAt` | 能(见 §4),但如 §4 所述容易被批量关闭事件扭曲 | 候选,不绑定,标注「批量关闭会失真」|
| 跟踪文件规模 | `git ls-files` | 能,522 个文件、86 个 docs md | 候选,不绑定 |

### 6.5 库里 issue 行(样例,两条真实远端 issue)

| 来源字段 | number | title | status | closed_at |
|---|---|---|---|---|
| origin=backfill | 78 | 文档可读性 · plan/12 补两处小对齐:T 系列票号无定义、技能数 22→41 漂移 | closed | 2026-08-06T02:13:07Z |
| origin=backfill | 81 | 术语治理 · 完成 ADR0001 遗留改名:R5(OpStage→阶段实例)与 R7(「卡」仅指界面卡片) | open | — |

（`origin` 是库里 issue 表要新增的一列,标这行 issue 是回填来的,不是 buddy 里建的活;状态与关闭时间照远端原样抄,不重新判断、不算任何人的战绩——细节见正文 §4。)

---

用到的全部命令已在各节内联;`gh` 版本 `2.95.0`、认证账号 `forcegravity1989`(`gh auth status` 已核实为登录状态);仓为 GitHub 公开仓,查询不需要额外权限。`codehub-cli` 本机未安装,codehub 侧的等价命令在正文 §1 里给出但标注「未在真 codehub 上验证」。
