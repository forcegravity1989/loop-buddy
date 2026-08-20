# 11 · 知识库

> **30 秒导读**:知识库屏(左栏第六入口)的设计——三个页签(知识 / 代码图 / 资产)各自数据从哪来、怎么刷新、空时显示什么、命令叫什么。**三个页签一张登记表都不读**:现扫仓内 `docs/`、现跑 codegraph、现读 `git log` 与 `.bw/releases.md`。给接着做 V4 的会话看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 代码图只做了大文件榜,符号搜索与依赖图没做。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

## 0 · 这篇管什么、不管什么

管:三页签——**知识**(仓内文档树,只读渲染)、**代码图**(装了 codegraph 就现跑三样,没装就如实灰)、**资产**(项目自有 / 蒸馏的技能、workflow、产物登记、发版记录、仓统计,五类的来源与刷新时机);顶部贯穿三页签的规范对账条。

不管:对账算法(指纹怎么算、三类怎么判定)——[03 篇](03-standard-and-backfill.md) §2.6 已定,本篇只讲 UI 呈现;规范文件内容——见 [standard-module-draft.md](../standard-module-draft.md);仓文件格式细节——见 [02 篇](02-data-and-files.md) §2.5;技能 / workflow 具体怎么注册、`.claude/skills/` 目录结构怎么定——见 [04 篇](04-tools-and-workflows.md),本篇只讲怎么摆出来(没有登记表可查,见 §2.4);写仓的具体流程(名片编辑、规范升级怎么建活提 MR)——见 08/03 篇。

## 1 · 用户看到什么、做什么

从左栏「知识库」进,默认落在「知识」页签(同进程内记住上次停留)。顶部横幅贯穿三页签:「规范 v4.0 · 对账 缺 0 / 过期 1 / 你改过 0 · 看差异 · 升级」,数字来自 [03 篇](03-standard-and-backfill.md) 的对账结果,不是装饰。

**知识**:左边一棵按分组的树,点了才现读渲染成只读 markdown;右边预览区。没有编辑按钮——想改内容,总览「编辑」名片、计划屏改本周文件,写仓永远走活 + MR。

**代码图**:本机装了 `codegraph` 就现跑三样查询(模块依赖概览、大文件榜、符号搜索);没装就整块置灰,给一句怎么装。

**资产**:自上而下五个区块——项目自有技能、蒸馏技能(带「来源活」链接)、workflow(含 buddy 自建的三张运作 workflow)、产物登记、发版记录,最下一条仓统计小字(与总览第⑤块同源,不重复起子进程)。

## 2 · 设计

### 2.1 顶部对账条:三页签共用

渲染时调一次 `ReconcileStandard`(纯读,不建活不写仓),拿三类计数(缺 / 过期 / 人改过)和文件清单渲成横幅。「看差异」展开列表;「升级」对选中文件触发 `UpgradeStandard`,按 [03 篇](03-standard-and-backfill.md) §2.6 流程走(纯替换建轻量活,需合并走一次真实 agent 会话),最终一个 MR,人在通知里评审合入——流程本篇不重复,只摆入口。

### 2.2 知识页签:仓内文档树

**树从哪来**:不是扫全仓,是按规范八大类固定分组、每组按约定路径找文件——`.bw/PROJECT.md`/`AGENTS.md`(仓根)(章程)、`.bw/metrics.toml`/`.bw/project.toml`/`.bw/issue-policy.toml`/`.bw/standard.toml`(规范件)、`.bw/plan/YYYY-Www.md`(周计划,倒序)、`.bw/releases.md`(发版记录)、`.bw/decisions/*.md`(决策记录,扩展,可能不存在)、`.bw/design/`(设计产物,扩展)。**老项目的历史回填周不是单独一组**:回填的 `.bw/plan/YYYY-Www.md` 与人写的本周文件**同目录、同格式**,靠 front matter `origin: backfill` 与树上的小徽记区分,不是两套渲染逻辑——盘点之后已取消单独的 `.bw/plan/history.md` 文件(格式见 [02 篇](02-data-and-files.md) §2.5)。

**懒加载**:打开页签只拿文件清单(是否存在),点了才现读那一个文件、渲染进预览区——`.bw/plan/*.md` 老项目回填后可能几十个,没必要一次全读。

**只读渲染**:markdown 渲染,TOML 等按代码块原文显示;不提供编辑——母文档「三条不变的规矩」第①条的直接推论。

| 字段 | 来源 | 刷新时机 | 空态文案 | 读回 |
|---|---|---|---|---|
| 文件清单(树) | 仓文件(按分组固定路径枚举) | 打开页签现枚举,不缓存 | 某组无文件不显示;整棵树空显示「还没有铺底 → 去接入项目完成第 0 步」 | 树上文件数与 `git -C <工作区> ls-files .bw/ CLAUDE.md` 手数对比 |
| 单文件内容 | 仓文件(懒加载现读) | 每次点击现读,不缓存 | 读取失败(被删/权限)显示「读取失败:{原文错误}」 | 预览渲染文本与 `cat <工作区>/<路径>` 逐字对比 |

### 2.3 代码图页签:装了就现跑,没装就如实灰

**探测**:每次打开先探测 `codegraph` 命令在不在 PATH。探测不到 → 整块置灰,「未安装 codegraph → `npm install --global @colbymchenry/codegraph@1.5.0` 然后 `codegraph init`」(版本号是本仓 CI 钉住的版本,`scripts/codegraph-version`)。装了但该仓 `.codegraph/` 不存在(没跑过 `codegraph init`)→「未建索引 → codegraph init」。

**装了之后现跑三样**(预研 [`research/codegraph.md`](../../archive/v4-prototype/research/codegraph.md) §3/§5 实测过):①**大文件榜**——`codegraph files -j`,按 `size`/`nodeCount` 排序取前若干行,预研实测几十毫秒级返回;②**符号搜索**——提交后跑 `codegraph node -f <文件> --symbols-only` 拿符号列表,或对某符号跑 `codegraph callers/impact <符号> -j`(预研实测确认 BW 大量用 `dyn Trait` 动态派发会让 `callers` 漏边即假阴性,结果如实展示原始数字,不产出「零调用者=死代码」结论);③**模块依赖概览**——现跑 `codegraph explore`(官方定位「一个强工具」的泛用查询),**如实说明**:预研未逐字段核实该命令是否有稳定 `--json` 模块依赖输出,首版按文本块原样展示,渲染形式留第 6 节开放问题。

每次打开页签或提交一次搜索都是新的子进程调用,**不缓存**——和 03 篇「对账是纯读操作不需要缓存」同一取舍。**不做**死代码判定(见第 4 节)。

| 字段 | 来源 | 刷新时机 | 空态文案 | 读回 |
|---|---|---|---|---|
| 探测结果 | 现跑(`which codegraph` / 检测 `.codegraph/`) | 每次打开现探测 | 未装:「未安装 codegraph → {装法}」;未建索引:「未建索引 → codegraph init」 | 终端手跑同一探测命令,结果与页面灰/亮状态一致 |
| 大文件榜、符号搜索、模块依赖概览 | 现跑(`codegraph` 子进程,不入库不入仓) | 打开页签 / 每次提交搜索现跑一次 | 搜索框为空不显示结果区;查无结果显示「—」 | 终端手跑同一条 `codegraph …` 命令,数字与页面一致 |

### 2.4 资产页签:五个区块

**没有登记表可查**——盘点之后 `skill`/`skill_package`/`artifact`/`release` 这些登记表全部取消(02 篇 §2.1/§2.6):库里只剩 `project`/`issue`/`claude_conversation`/`app_meta` 四张表。资产页签五个区块因此全部改成现扫仓目录、解析仓文件、或复用 `issue` 缓存表的现算查询,不是查库表。

**技能清单来自两处**(02 篇 §2.5,都不建索引):buddy 自带的十三份编在二进制里,`origin` 标「buddy 自带」;项目自有的扫仓里 `.claude/skills/**/SKILL.md` 得到,`origin` 标「项目自有」,同名以仓里那份为准。项目自有的再按文件内容分两组——项目自己写的/人手加的一组,蒸馏产出(蒸馏时把「来自哪件活」写进 SKILL.md 正文或 front matter,具体字段格式留 [04 篇](04-tools-and-workflows.md)定)一组。蒸馏技能带「来源活」链接,点击触发 `OpenDistillSource`,跳到会话屏定位当初蒸馏它的那张活——链接目标解析自文件内容,不是关联表查询。

**workflow**:同一份清单里符合 SOP 类技能包结构的条目(04 篇定义识别规则),列名称 / 来源(buddy 自带 / 项目自有 / 不在册)/ 入口技能 / 用过几次。**没有胜率数字**——盘点之后"战绩"这个持久账本概念本身被取消(02 篇 §2.3):"用了几次"是现算查询(`SELECT workflow, COUNT(*) FROM issue WHERE kind='business' AND workflow!='' GROUP BY workflow`),"成没成"改看远端 MR 合没合入,本页签不展示胜率。buddy 自建的三张运作 workflow(更新指标与周计划 / 资产盘点(含首次模式=历史回填) / 规范铺底)混在同一份清单里,来源标「内置」,不单独开区块——它们和业务 workflow 走同一套扫描逻辑、同一套字段。

**产物登记**:不建表,`git log --name-only` 就是产物登记(02 篇 §2.6)——列文件路径 / 登记时 git commit / 提交信息里能解析到的关联活号(commit message 或标题里的 `#<号>`,解析不到就不关联,不强凑)。CLAUDE.md「产物登记」这句老描述在 V4 的新落点:不再是活推 Done 边时自动写库表,是 git 提交本身就是记录,查询时现扫现算。

**发版记录**:解析 `.bw/releases.md`(02 篇 §2.5 唯一正本,不建 `release`/`release_issue` 表),列版本号 / 日期 / 说明 / 包含的活(文件里「包含的活」列是活号自由文本,渲染时按号去查 `issue` 表拿标题展开,找不到对应活的号跳过并记警告)/ 来源(`人发`/`回填`)。总览第⑦块已展示同一份文件,这里是第二个消费点,不重复维护。

**仓统计**:复用 `bw_engine::evidence::collect()`——和总览第⑤块**同一次调用逻辑**,不额外起子进程、不额外定时任务;打开页签时现算,支持手动「立即采集」,无后台定时刷新。

| 字段 | 来源 | 刷新时机 | 空态文案 | 读回 |
|---|---|---|---|---|
| 技能(buddy 自带 + 项目自有) | buddy 二进制里那十三份 + 仓目录 `.claude/skills/**/SKILL.md`(现扫,不落库) | 打开页签现扫 | 「暂无」 | `cargo run -p bw-v4 --example prompt_smoke -- <目录>` 报的 13 份 + `find <ws>/.claude/skills -name SKILL.md \| wc -l`,两数之和与页面条数一致(同名只算一次) |
| workflow 与用过几次 | 仓目录(现扫技能包)+ `issue` 缓存表现算(02 篇 §2.3) | 打开页签现扫/现查,不缓存 | 「暂无」 | `sqlite3 <db> "SELECT workflow, COUNT(*) FROM issue WHERE project_id='<pid>' AND kind='business' AND workflow!='' GROUP BY workflow;"` 与页面「用过几次」列一致 |
| 产物登记 | `git log --name-only`(现算,02 篇 §2.6) | 打开页签现算 | 「暂无登记产物」 | `git -C <ws> log --name-only --pretty=format:'%H'` 按提交去重统计的文件条数与页面一致 |
| 发版记录 | 仓文件 `.bw/releases.md`(02 篇 §2.5 唯一正本) | 打开页签现读现解析 | 「暂无发版记录」 | `cat <ws>/.bw/releases.md` 表格行数与页面条数一致 |
| 仓统计 | 现跑(`evidence::collect()`,与总览⑤同源) | 打开页签现算 + 手动「立即采集」,无后台定时 | 「无法读取仓统计:{git 原文错误}」 | 终端手跑对应 `git` 命令,数字与页面一致 |

### 2.5 命令 / 事件(名字 + 一句话)

| 命令 | 一句话 |
|---|---|
| `OpenKbFile { project_id, path }` | 知识页签点树上一个文件,懒加载现读内容并渲染到预览区 |
| `RunCodegraphQuery { project_id, kind, args }` | 代码图页签三样查询与符号搜索的统一入口,按 `kind` 分发到 `adapters/codegraph/` |
| `ReconcileStandard { project_id }` | 引用 [03 篇](03-standard-and-backfill.md) §2.8:纯读,算缺 / 过期 / 人改过三类,给顶部横幅用 |
| `UpgradeStandard { project_id, files }` | 引用 [03 篇](03-standard-and-backfill.md) §2.8:人选中要升的文件后触发,建轻量活或一次 agent 会话,最终提 MR |
| `OpenDistillSource { issue_id }` | 资产页签蒸馏技能「来源活」链接,跳到会话屏定位那张活 |

事件:`KbFileLoaded`(文件加载完成或失败,失败带原文错误)、`CodegraphQueryFinished` / `CodegraphUnavailable`(查询完成,或探测不到命令 / 索引未建的降级回执)。

## 3 · 工程对照

**模块位置**:界面在 `crates/app-shell/src/screens/kb/`(三个页签在同一个文件里,
还没到要拆子模块的体量);数据拼装在 `crates/app-shell/src/bridge/vm_kb.rs`(**不是** `bw-app/src/kb.rs` ——
V4 不动旧 crate,而且这三个页签全是现扫文件/现跑子进程,没有一句业务判断,放 ViewModel 拼装层正合适);
`codegraph` 子进程封装在 `crates/app-shell/src/adapters/codegraph/`(带 README,三段:借了什么、没借什么)。

```rust
// crates/app-shell/src/adapters/codegraph/mod.rs(已落地)
pub enum Availability { NotInstalled, NotIndexed, Ready }
pub fn detect(workspace: &Path) -> Availability;
pub fn big_files(workspace: &Path, top: usize) -> Result<Vec<FileRow>, String>;
```

**代码图三样只做了一样**:大文件榜(`codegraph files -j`,按体积排序取前 20)。**符号搜索**与
**模块依赖概览**(`codegraph explore`)没做 —— 前者要一个搜索框加一条查询分发,后者连有没有稳定的结构化
输出都还没核实过(§6 开放问题 1)。两样都记进 `docs/LEFTOVERS.md`,界面上没有占位框。

**这两个页签不跟着每次重拼 ViewModel 跑**:代码图要起 codegraph 子进程,资产要走 `git log --name-only`
再采一次仓统计,加起来好几个子进程。人在别的屏点一下就把它们跑一遍是不能接受的,所以只在**点页签或点
「重新跑一次」那一刻**跑,结果留在壳自己的导航状态里。这不违背「不缓存」的取舍 —— 每次点开都是全新的
子进程调用,只是不替人自动重跑。

**读回口子**:`BW_KB_DUMP=1` 启动时把三个页签的数字打进 stderr(每组几个文件、代码图头一名是谁多大、
资产五个区块各几条),好让人拿 `ls` / `codegraph files -j` / `cat .bw/releases.md` 当场对。截图对不了数,
这个能。

**没有新增数据模型,也不查任何登记表**——资产页签五个区块全部现列 buddy 自带的技能 + 现扫仓目录(`.claude/skills/`)、现算 git(`git log --name-only`)、或解析仓文件(`.bw/releases.md`);02 篇 §2.6「信息住哪」总表已把这些数据点全部划给"仓正本"或"现算",本篇只新增只读查询/扫描路径。知识 / 代码图页签同样不落库,现算现显。

**与旧壳的关系**:今天(V3)对应功能分散在「产物面板」「版本面板」「Hub → 知识」三处,V4 删前两个、把「Hub → 知识」文档树搬进这一屏(见 [01 篇](01-architecture.md) §2.7,`BW_HUB=knowledge` 退役,并入 `BW_PANEL=kb`)。

## 4 · 边界与失败

**不做**:①**编辑文档**——只读,不提供编辑框。改 `.bw/PROJECT.md` 走总览「编辑」名片(08 篇 `EditProjectCard`);改本周计划走计划屏 / 运作活①嵌入终端;升级规范文件走 2.1 节「升级」按钮——一律走活 + MR,不在本屏直接改仓。②**全文搜索**——首版没有跨文件文本检索,只有分组树浏览 + 符号名精确搜索,文档量级小时够用。③**代码图死码删除判断**——预研([`research/codegraph.md`](../../archive/v4-prototype/research/codegraph.md) §2/§5)已实测 `dyn Trait` 动态派发会让 `callers` 漏边,「零调用者」不能直接当死代码结论;本屏只如实展示查询原始数字,运作活②要用这类数据做微重构时同样需人工复核(09 篇负责)。

**失败如实显示**:

| 场景 | 怎么显示 |
|---|---|
| `codegraph` 不在 PATH | 代码图页签置灰 + 「未安装 codegraph → {装法}」,不影响另外两页签 |
| 已装但未建索引 | 置灰 + 「未建索引 → codegraph init」 |
| `codegraph` 子进程超时 / 非 0 退出 | 原样显示 `stderr` 文本,不留空白、不静默重试 |
| 知识页签读取失败(已删/权限) | 「读取失败:{原文错误}」 |
| `.bw/standard.toml` 不存在 | 沿用 [03 篇](03-standard-and-backfill.md):视为未铺底,提示「一键铺底」而非报错 |

## 5 · 验收与读回

1. **三页签深链**:`BW_DB=<db> BW_OPEN=<项目> BW_PANEL=kb ./target/debug/bw-v4-dev`,stderr 见 `[BW_OPEN]`;三页签各截一张图。
2. **对账读回**:对照 `.bw/standard.toml` 的 `enabled`/`version` 与 `standard/VERSION`——改动一个已铺底文件的一个字符后重开知识库屏,顶部条应从「对账 ✓」变成「你改过 1 项」,同 [03 篇](03-standard-and-backfill.md) §5 第 5 条验证手法。
3. **codegraph 一次真跑读回**:BW 自己仓(已有 `.codegraph/codegraph.db`)打开代码图页签,大文件榜前几行应与终端手跑 `codegraph files -j | jq 'sort_by(-.size)' | head` 一致;临时清空 `PATH` 重开应显示灰态「未安装」,不 panic。
4. **资产数字读回**:`git -C <ws> log --name-only --pretty=format:'%H'` 按提交去重统计的文件条数与「产物登记」区块条数一致;`cat <ws>/.bw/releases.md` 表格行数与「发版记录」区块条数一致;`sqlite3 <db> "SELECT workflow, COUNT(*) FROM issue WHERE project_id='<pid>' AND kind='business' AND workflow!='' GROUP BY workflow;"`(02 篇 §2.3 现算查询)与 workflow 区块「用过几次」列一致——**没有 `win_rate`/`runs`/`wins` 可查**,V4 不展示胜率。

## 6 · 开放问题(≤3)

1. **`codegraph explore` 的模块依赖概览渲染形式未定**——预研未核实这条命令是否有稳定结构化输出,首版按文本块展示,以后要不要自己解析画图留待有真实需要时定。
2. **知识树的分组顺序**——本篇按规范八大类的直觉顺序排,未对照 `standard/` manifest 是否已有权威顺序字段,若 03 篇后续定义了顺序,这里应跟着走。
3. **回填多周后 `.bw/plan/` 目录的渲染量级**——首版假设内容量小,直接整篇渲染;老项目跑久了 `.bw/plan/` 下可能积累几十上百个回填周文件(与本周文件混在同一目录、同一份周列表里,靠 `origin: backfill` 徽记区分,不是单独一组——见 §2.2),要不要分页/懒加载得更激进,留给试点两周(10 篇)按真实文件大小定。
