# 11 · 知识库

> **30 秒导读**:知识库屏(左栏第六入口,原「项目空间」)的详细设计——三个页签(知识 / 代码图 / 资产)各自数据从哪来、怎么刷新、空时显示什么、命令叫什么。**详细设计稿,待用户复核**,不改代码。母文档 [`mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) §5 把版本面板、产物面板、技能盘点、仓统计并进了这一屏的「资产」页签,评审子代理已指出这屏一直没有专篇([`REVIEW-2026-08-19.md`](REVIEW-2026-08-19.md) 6.1.1),本篇补上。

## 0 · 这篇管什么、不管什么

管:三页签——**知识**(仓内文档树,只读渲染)、**代码图**(装了 codegraph 就现跑三样,没装就如实灰)、**资产**(项目自有 / 蒸馏的技能、workflow、产物登记、发版记录、仓统计,五类的来源与刷新时机);顶部贯穿三页签的规范对账条。

不管:对账算法(指纹怎么算、三类怎么判定)——[03 篇](03-standard-and-backfill.md) §2.6 已定,本篇只讲 UI 呈现;规范文件内容——见 [standard-module-draft.md](../standard-module-draft.md);仓文件格式细节——见 [02 篇](02-data-and-files.md) §2.8;技能 / workflow 表结构与战绩记账——见 [04 篇](04-tools-and-workflows.md) §2.6-2.9,本篇只讲怎么摆出来;写仓的具体流程(名片编辑、规范升级怎么建活提 MR)——见 08/03 篇。

## 1 · 用户看到什么、做什么

从左栏「知识库」进,默认落在「知识」页签(同进程内记住上次停留)。顶部横幅贯穿三页签:「规范 v4.0 · 对账 缺 0 / 过期 1 / 你改过 0 · 看差异 · 升级」,数字来自 [03 篇](03-standard-and-backfill.md) 的对账结果,不是装饰。

**知识**:左边一棵按分组的树,点了才现读渲染成只读 markdown;右边预览区。没有编辑按钮——想改内容,总览「编辑」名片、计划屏改本周文件,写仓永远走活 + MR。

**代码图**:本机装了 `codegraph` 就现跑三样查询(模块依赖概览、大文件榜、符号搜索);没装就整块置灰,给一句怎么装。

**资产**:自上而下五个区块——项目自有技能、蒸馏技能(带「来源活」链接)、workflow(含 buddy 自建的三张运作 workflow)、产物登记、发版记录,最下一条仓统计小字(与总览第⑤块同源,不重复起子进程)。

## 2 · 设计

### 2.1 顶部对账条:三页签共用

渲染时调一次 `ReconcileStandard`(纯读,不建活不写仓),拿三类计数(缺 / 过期 / 人改过)和文件清单渲成横幅。「看差异」展开列表;「升级」对选中文件触发 `UpgradeStandard`,按 [03 篇](03-standard-and-backfill.md) §2.6 流程走(纯替换建轻量活,需合并走一次真实 agent 会话),最终一个 MR,人在通知里评审合入——流程本篇不重复,只摆入口。

### 2.2 知识页签:仓内文档树

**树从哪来**:不是扫全仓,是按规范八大类固定分组、每组按约定路径找文件——`PROJECT.md`/`AGENTS.md`(章程)、`.bw/metrics.toml`/`.bw/project.toml`/`.bw/issue-policy.toml`/`.bw/standard.toml`(规范件)、`docs/plan/YYYY-Www.md`(周计划,倒序)、`docs/releases.md`(发版记录)、`docs/decisions/*.md`(决策记录,扩展,可能不存在)、`docs/design/`(设计产物,扩展)——老项目多一组「历史回填」,只有 `docs/plan/history.md`(第 0 站生成,格式见 [02 篇](02-data-and-files.md) §2.8)。

**懒加载**:打开页签只拿文件清单(是否存在),点了才现读那一个文件、渲染进预览区——`docs/plan/*.md` 老项目回填后可能几十个,没必要一次全读。

**只读渲染**:markdown 渲染,TOML 等按代码块原文显示;不提供编辑——母文档「三条不变的规矩」第①条的直接推论。

| 字段 | 来源 | 刷新时机 | 空态文案 | 读回 |
|---|---|---|---|---|
| 文件清单(树) | 仓文件(按分组固定路径枚举) | 打开页签现枚举,不缓存 | 某组无文件不显示;整棵树空显示「还没有铺底 → 去接入项目完成第 0 步」 | 树上文件数与 `git -C <工作区> ls-files docs/plan/ .bw/ PROJECT.md AGENTS.md` 手数对比 |
| 单文件内容 | 仓文件(懒加载现读) | 每次点击现读,不缓存 | 读取失败(被删/权限)显示「读取失败:{原文错误}」 | 预览渲染文本与 `cat <工作区>/<路径>` 逐字对比 |

### 2.3 代码图页签:装了就现跑,没装就如实灰

**探测**:每次打开先探测 `codegraph` 命令在不在 PATH。探测不到 → 整块置灰,「未安装 codegraph → `npm install --global @colbymchenry/codegraph@1.5.0` 然后 `codegraph init`」(版本号是本仓 CI 钉住的版本,`scripts/codegraph-version`)。装了但该仓 `.codegraph/` 不存在(没跑过 `codegraph init`)→「未建索引 → codegraph init」。

**装了之后现跑三样**(预研 [`research/codegraph.md`](../research/codegraph.md) §3/§5 实测过):①**大文件榜**——`codegraph files -j`,按 `size`/`nodeCount` 排序取前若干行,预研实测几十毫秒级返回;②**符号搜索**——提交后跑 `codegraph node -f <文件> --symbols-only` 拿符号列表,或对某符号跑 `codegraph callers/impact <符号> -j`(预研实测确认 BW 大量用 `dyn Trait` 动态派发会让 `callers` 漏边即假阴性,结果如实展示原始数字,不产出「零调用者=死代码」结论);③**模块依赖概览**——现跑 `codegraph explore`(官方定位「一个强工具」的泛用查询),**如实说明**:预研未逐字段核实该命令是否有稳定 `--json` 模块依赖输出,首版按文本块原样展示,渲染形式留第 6 节开放问题。

每次打开页签或提交一次搜索都是新的子进程调用,**不缓存**——和 03 篇「对账是纯读操作不需要缓存」同一取舍。**不做**死代码判定(见第 4 节)。

| 字段 | 来源 | 刷新时机 | 空态文案 | 读回 |
|---|---|---|---|---|
| 探测结果 | 现跑(`which codegraph` / 检测 `.codegraph/`) | 每次打开现探测 | 未装:「未安装 codegraph → {装法}」;未建索引:「未建索引 → codegraph init」 | 终端手跑同一探测命令,结果与页面灰/亮状态一致 |
| 大文件榜、符号搜索、模块依赖概览 | 现跑(`codegraph` 子进程,不入库不入仓) | 打开页签 / 每次提交搜索现跑一次 | 搜索框为空不显示结果区;查无结果显示「—」 | 终端手跑同一条 `codegraph …` 命令,数字与页面一致 |

### 2.4 资产页签:五个区块

**项目自有 / 蒸馏技能**:查 `skill` 表 `project_id=当前项目`,按 `distilled_from_issue` 是否为空分两组(空=项目自己导入,非空=蒸馏出来的;见 [04 篇](04-tools-and-workflows.md) §2.6——`package_id` 非空的成员随所属 workflow 在下一区块列,不重复)。蒸馏技能带「来源活」链接,点击触发 `OpenDistillSource`,跳到会话屏定位当初蒸馏它的那张活。

**workflow**:查 `skill_package` 表(`project_id=当前项目 OR project_id IS NULL`),列名称 / 来源(`builtin`/`imported`)/ 入口技能 / 用过几次(`runs`)/ 胜率(`win_rate`,永远现算不手写)。buddy 自建的三张运作 workflow(更新指标与周计划 / 资产盘点与微重构 / 规范铺底)混在同一张表里,来源显示 `builtin`,不单独开区块——它们和业务 workflow 是同一张表、同一套字段。

**产物登记**:查既有 `artifact` 表(`crates/bw-store/src/schema.sql` 已有,V4 不改结构),列路径 / 类型 / 字节数 / 登记时 git commit / 关联的活。这张表今天已是「活推 Done 边自动写入」的记账表,V4 只是从旧的独立「产物面板」搬进这一页签。

**发版记录**:查 [02 篇](02-data-and-files.md) §2.5 的 `release` 表,列版本号 / 日期 / 说明 / 包含的活(`release_issue` 展开)/ 来源(`human`/`backfill`)。总览第⑦块已展示同一张表,这里是第二个消费点,不重复维护。

**仓统计**:复用 `bw_engine::evidence::collect()`——和总览第⑤块**同一次调用逻辑**,不额外起子进程、不额外定时任务;打开页签时现算,支持手动「立即采集」,无后台定时刷新。

| 字段 | 来源 | 刷新时机 | 空态文案 | 读回 |
|---|---|---|---|---|
| 技能(自有/蒸馏) | 库表 `skill` | 打开页签现查 | 「暂无」 | `sqlite3 <db> "SELECT name,distilled_from_issue FROM skill WHERE project_id='<pid>';"` |
| workflow | 库表 `skill_package` | 打开页签现查 | 「暂无」 | `sqlite3 <db> "SELECT name,source,runs,win_rate FROM skill_package WHERE project_id='<pid>' OR project_id IS NULL;"` |
| 产物登记 | 库表 `artifact`(既有) | 打开页签现查 | 「暂无登记产物」 | `sqlite3 <db> "SELECT path,kind,bytes FROM artifact WHERE project_id='<pid>' ORDER BY registered_at DESC;"` |
| 发版记录 | 库表 `release`(02 篇新) | 打开页签现查 | 「暂无发版记录」 | `sqlite3 <db> "SELECT version,released_at,origin FROM release WHERE project_id='<pid>';"` |
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

**模块位置**(遵循 [01 篇](01-architecture.md) §2.3「一屏一模块」):界面代码在 `crates/app-shell/src/screens/kb/`(01 篇目录树已列),按三页签再分子模块;数据查询(selector)在 `bw-app` 新增只读模块(建议 `bw-app/src/kb.rs`),风格对齐 [08 篇](08-overview-derivation.md) §3 的 `overview.rs`;`codegraph` 子进程封装在 `crates/app-shell/src/adapters/codegraph/`(01 篇 §2.2 已列位置,本篇是第一个真实消费者)。

```rust
// crates/app-shell/src/adapters/codegraph/mod.rs(新)
pub fn detect() -> CodegraphAvailability { /* which codegraph;探测到再看 .codegraph/ 是否存在;
    返回 NotInstalled | NotIndexed | Ready,三态映射 2.3 节灰态文案 */ }
pub fn run_query(workspace: &Path, kind: QueryKind, args: &[String]) -> Result<String, CodegraphError> {
    /* std::process::Command 起 `codegraph <kind> ... -j`(或 --symbols-only),stdout 原样返回;
       非 0 退出把 stderr 原文包进 CodegraphError,不吞错误 */ }
```

**没有新增数据模型**——资产页签用的 `skill`/`skill_package`(04 篇)、`artifact`(既有)、`release`(02 篇)都已有归属篇章,本篇只新增只读查询路径;知识 / 代码图页签不落库,现算现显。

**与旧壳的关系**:今天(V3)对应功能分散在「产物面板」「版本面板」「Hub → 知识」三处,V4 删前两个、把「Hub → 知识」文档树搬进这一屏(见 [01 篇](01-architecture.md) §2.7,`BW_HUB=knowledge` 退役,并入 `BW_PANEL=kb`)。

## 4 · 边界与失败

**不做**:①**编辑文档**——只读,不提供编辑框。改 `PROJECT.md` 走总览「编辑」名片(08 篇 `EditProjectCard`);改本周计划走计划屏 / 运作活①嵌入终端;升级规范文件走 2.1 节「升级」按钮——一律走活 + MR,不在本屏直接改仓。②**全文搜索**——首版没有跨文件文本检索,只有分组树浏览 + 符号名精确搜索,文档量级小时够用。③**代码图死码删除判断**——预研([`research/codegraph.md`](../research/codegraph.md) §2/§5)已实测 `dyn Trait` 动态派发会让 `callers` 漏边,「零调用者」不能直接当死代码结论;本屏只如实展示查询原始数字,运作活②要用这类数据做微重构时同样需人工复核(09 篇负责)。

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
4. **资产数字读回**:`sqlite3 <db> "SELECT COUNT(*) FROM artifact WHERE project_id='<pid>';"` 与「产物登记」条数一致;`skill_package.win_rate` 显示值与 `runs`/`wins` 现算结果一致。

## 6 · 开放问题(≤3)

1. **`codegraph explore` 的模块依赖概览渲染形式未定**——预研未核实这条命令是否有稳定结构化输出,首版按文本块展示,以后要不要自己解析画图留待有真实需要时定。
2. **知识树的分组顺序**——本篇按规范八大类的直觉顺序排,未对照 `standard/` manifest 是否已有权威顺序字段,若 03 篇后续定义了顺序,这里应跟着走。
3. **`docs/plan/history.md` 回填多周后的渲染量级**——首版假设内容量小,直接整篇渲染;老项目跑久了可能积累几十上百周表格行,要不要分页,留给试点两周(10 篇)按真实文件大小定。
