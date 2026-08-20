# 06 · 计划屏

> **30 秒导读**:这篇管 V4 计划屏——一个界面里同时装下「这周要干什么」和「V3 那种六列看板」,以及拖拽的产品规则(六列全都能拖:排期直接生效,状态动作弹确认框,不合法的转移松手弹回)。给接着做 V4 的会话看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 字段名与命令名以 [02-data-and-files.md](02-data-and-files.md) 为准,本篇只应用、不另造一套。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

## 0 · 这篇管什么、不管什么

**管**:计划屏——左栏周列表怎么算、中栏六列看板每列是什么/卡片长什么样/按钮各走哪条命令、拖拽的范围与替代路径、周头(周目标/进度/运作活 chip/新建活/发版本/预览开关)、「全部」视角的筛选条、右侧活详情滑出、发版本三步、预览未合入。对应母文档第 2/3 站、§2.5(指标/周目标/活的关系)、§2.6 全景表里排期相关部分、§5 计划入口、§6「信息住哪」;数据落点见 02 篇——`issue` 表 9 个扩展列(排期/版本/工具/种类/来源/workflow/类别/顺序/挂的指标),**没有 `week_plan`/`release` 这两张表**,周列表靠扫 `.bw/plan/` 目录、发版记录唯一正本是 `.bw/releases.md`;待拍-04/05/06/07/08/12/21/25。

**不管**:运作活①会话里 agent 具体做什么——[09-ops-workflows.md](09-ops-workflows.md),本篇只消费它挂了 `week_of` 的产出。开工工具/workflow 怎么注入——[04-tools-and-workflows.md](04-tools-and-workflows.md)。会话屏三栏、终端——[05-session-screen.md](05-session-screen.md)。项目群通知——[07-notify-and-chat-group.md](07-notify-and-chat-group.md)。规范铺底/`.bw/issue-policy.toml` 正本——[03-standard-and-backfill.md](03-standard-and-backfill.md)。

**与 01 篇的边界**:01 篇(未定案)给了「一屏一模块」等建法规矩和 `Command`/`Event` 总线第一版草案(含 `ScheduleIssue`/`UnscheduleIssue`/`ReorderIssue`/`CutRelease`/`PreviewIssueWorktree`)。**本篇是这几条命令真正落地的地方**,§3 对草案做了几处收窄改名,标「较 01 篇改动」,01 篇日后据此回填。

## 1 · 用户看到什么、做什么

人从左栏点「计划」,或总览「本周计划进度」块点「去计划 →」,或总览空态点「开始本周」,进的是同一个屏。

**左边**是一列窄周列表:最上面是「全部」,往下按时间倒序(本周置顶,标「· 本周」)、再是历史周。最下面是「在研版本」下拉,只影响默认值,不影响周列表本身。

**中栏**平时是「选中一周」的样子:顶上一条「周头」——周目标一句话、一条按业务活状态分段的进度条、运作活①②③各一个状态 chip、右边「新建活」「发版本」和「预览·未合入」开关。下面是**六列看板**:待办池、待办、进行中、评审中、已完成、阻塞,列头下一行小字写清这一列是什么意思,卡片按 V3 样子(标题整行不省略、操作区按状态互斥)。业务活和三张运作活同在一个看板,运作活的卡多一个「运作」标记。**待办池不管左边选哪一周,永远显示全项目所有未排期的活**——设计决定,理由见 §2.2。选「全部」时周头换成一条筛选条(类别·版本·来源·状态·关键字),看板照旧,只是六列里装的是筛完的全部活。点卡片标题,右边滑出 360px 详情面板(字段见 §2.6),底部「去会话 →」「蒸馏」。

**拖拽**统一到六列都能拖(用户拍板改的,待拍-25 已按此改;用户原话「拖动要统一,都能拖」):待办池⇄待办之间(或列内部)是「排期」动作,松手直接生效,不改任何状态;拖到进行中/评审中/已完成/阻塞是「状态动作」,松手弹一个确认框才真正发生——拖到进行中 = ▶开工(框里显示开工工具/workflow,确认才真起 agent 会话);拖到评审中 = 推到评审中;拖到已完成 = ✓完成(仍是铁律里「人显式点」的那一下,只是点在弹窗上,且只有评审中的活能被拖进这一列);拖到阻塞 = ⛔阻塞(框里填原因,必填)。不合法的转移(状态机不允许的,比如待办池直接拖到已完成)松手即弹回原位并提示原因,不静默失败。**卡面因此不摆按钮**——拖拽+确认弹窗覆盖日常操作,卡片只留标题与 1-2 个 chip;§2.2 给的全部按钮语义原样保留在右侧详情面板(§2.6),当作不想拖时的替代路径。第一轮「不拖拽」、「只拖排期」两种写法都已作废,见 §2.3。

**发版本**(细节见 §2.4):**是一个按钮,一次点击直接生效**——不是三步向导。点下去自动取本周「已完成」列的全部活、按在研版本算出下一个版本号、说明写死成「本周完成的活」,直接往 `.bw/releases.md` 追加一行并给这些活回填版本号;不建活、不开分支、不开 PR、没有等人处理这一步。版本号认不出格式(比如还没设过在研版本)按钮就是灰的,内核也会拒收。**预览未合入**只在当前周有一张运作活①(或③的合并调整/历史回填)还在评审中、MR 未合入时能点开,从它的 worktree 读文件渲染周头,顶部横幅「预览 · 未合入」,关掉恢复正常(§2.5)。

## 2 · 设计

### 2.1 周模型

**`week_of`**:ISO 8601 周文本,形如 `2026-W34`(周一为起点),按机器本地时区判定「现在是哪一周」(BW 单机应用,与总览/运行记录等既有时间戳字段口径一致)。存文本不存时间戳,是本篇对预研 §9 开放问题②的答案:①直接对应仓文件名 `.bw/plan/YYYY-Www.md`(03 篇已定),免去格式转换;②避免「周边界算哪个时区」的歧义;③展示位置本来就吃这个文本。

**周列表怎么算(没有索引表,现算)**:「全部」下面列出的周 = 扫 `.bw/plan/` 目录得到的文件名(`.bw/plan/YYYY-Www.md`,取文件名里的 `YYYY-Www` 部分;可选读一下 front matter 的 `origin` 供小徽记区分本周/回填,见 02 篇 §2.5)∪ `issue.week_of` 缓存列上出现过的周(哪怕这周还没有正式文件,比如刚被拖进一个新周但运作活①还没跑),取并集去重。**本周永远置顶**,哪怕它还没有 `.bw/plan/` 文件、也没有活排进去——空的本周才有地方触发「开始本周」。这条扫描每次打开计划屏现跑一次,不缓存进库(母文档 §6.3 代价①已知情:buddy 自己仓几百个文件是毫秒级,万级历史周的老项目将来可能要加内存缓存,留 02 篇 §6 开放问题)。

**周头数据来源(全部现读仓文件,没有一样经过库内索引表)**:
- **周目标**:现读 `.bw/plan/YYYY-Www.md`,用 02 篇 §3.3 定义的解析器 `week_plan_file.rs::extract_goal(markdown)` 取「## 周目标」后第一段非空文本——每次打开计划屏/切周都现读现解析,不经过任何库内缓存,`issue` 表 9 列里也没有一列装周目标。
- **进度条**:按 `week_of` 匹配到的业务活+运作活状态分段,查的是 `issue` 缓存表(§2.2),这条不变。
- **运作活①②③ chip**:直接查这周 `kind='ops'` 的三张活状态,查 `issue` 缓存表,这条不变。
- **发版默认版本号**:现读 `.bw/project.toml` 的 `current_version` 字段(02 篇 §2.5/§3.3 的 `project_file.rs` 解析器新增字段)——**这个值不在 `project` 表里**,02 篇明确「这四项(连同现有五字段)都不落库存副本」(02 篇 §2.5),`project` 表结构没有新增列(02 篇 §3.1)。计划屏每次要显示在研版本都现解析这份文件,和「配置」屏读同一个解析结果。

**空态**:本周没有 `.bw/plan/` 文件(母文档 §2.6 用户四问第 2 条已定的判据)时,周头整块换成「本周还没有计划」+「开始本周」按钮,看板照常(待办池永远有内容)。点按钮发 `StartWeekPlanning`(01 篇已定义),建运作活①并跳会话屏 ▶开工——和总览空态、其它入口点的是同一个按钮语义。

### 2.2 六列看板

**列 = `IssueStatus` 六个真实状态**(`crates/bw-core/src/model.rs:1585-1593`):

| 列 | `IssueStatus` | 列头下一行定义 |
|---|---|---|
| 待办池 | `Backlog` | 未排进任何一周 |
| 待办 | `Todo` | 已排进本周,等开工 |
| 进行中 | `InProgress` | agent 在干 |
| 评审中 | `InReview` | MR 开着,等人审 |
| 已完成 | `Done` | 人点过完成 |
| 阻塞 | `Blocked` | 填了原因 |

第七个 `IssueStatus::Cancelled` **不占列**——V3 原本就把它挡在看板外(`issues.rs:143-150` 的 `cols` 数组只列六个,注释「dropped work, not a state to manage from here」),本篇沿用。但要「筛选可见」:「全部」视角的筛选条新增「状态」下拉(hifi 原型目前只有类别/版本/来源/关键字,没有状态项,这是本篇在 hifi 基础上补的一格),选「已取消」时能单独看到这批活;选中周视角不提供这个入口。

**待办池不按选中周过滤——设计决定**:待办池的定义就是「`week_of` 为空」,若按选中周过滤,结果永远是空集(不可能同时「等于某周」又「为空」)。这一列存在的意义就是给一个跨周、随时能看见「有哪些活没排期」的地方,不该因为切了个周就看不见某张待拖的卡。**其余五列只显示这一周 `week_of` 匹配的活**(包括已推进到 `InProgress`/`InReview`/`Done`/`Blocked` 的,只要当初排的是这一周就一直留在这周的看板里,能看出「这周排的活走到哪一步」)。运作活①②③同样按 `week_of` 过滤,不进待办池。

**卡片字段**:沿用 V3 骨架的信息部分(`issues.rs:335-338` 顶行 mono + 标题 + 左边框 3px 状态色),**不沿用操作行**——拖拽统一之后卡面不摆按钮(§2.3),V4 扩展:顶行 `#编号 · 类别`(类别 = `issue.stage` 复用的五个标签:原型/构建/优化/运营推广/**运维**——这个「运维」是 `StageKind::Ops` 类别标签,和下面「运作活」的「运」字不是一回事,前者是活的性质,后者是 `issue.kind='ops'` 标记「buddy 自己的三张运作活之一」,同一张卡可能同时出现两个不相关的「运」字,别混)+ 来源徽记(`kind='ops'` 显示「运作」、`kind='light'` 显示「轻量」——二者与 `kind='business'` 三选一,不会同时出现在一张卡上;`kind='business'` 时 `origin` 非「人建」再显示「agent 拆」/「自动建」/「回填」,「人建」不加徽记减少噪音)。标题整行不省略。1-2 个 chip 从「工具/workflow」「推动指标」里挑:`tool` 非空显示开工工具简称,`metric_key`(02 篇 §2.2 单列,一活最多挂一个指标,不是关联表)非空时加一个「→指标名」chip(查 `.bw/metrics.toml` 定义拿显示名,`metric_key` 本身只存那条指标在 `.bw/metrics.toml` 里的 `id`),`workflow` 与默认值不同时优先显示 workflow 名。类别已在顶行,不重复做 chip。`pr_number` 非零加一行 mono 的 `MR #N` 徽记。

**按钮语义(照抄 V3 真实代码 `crates/app-desktop/src/screens/op/issues.rs`,不是照抄高保真原型的简化演示——两者有出入,下表已用真代码口径修正)**:拖拽统一之后,这套语义**不再画在卡面上**——同样的命令现在有两条触发路径:①拖到对应列 + 确认弹窗(§2.3,状态动作专用,「排进本周」这类排期动作不经过这张表);②右侧详情面板(§2.6)按钮,当键盘可达性/不想拖时的替代路径。下表描述的是命令本身的前置条件与忙态,两条路径共用同一套判断。

| 列/状态 | 按钮 | 命令 | 前置条件 / 忙态 |
|---|---|---|---|
| 待办池/待办 | ▶ 开工 | `RunIssue{session,id}`(带 `StartSession`)| `status∈{Backlog,Todo,InProgress}`(`issue_run.rs:271-276`,内部把 `Backlog`/`Todo` 转成 `InProgress`);同项目无活在跑(串行锁)。忙态:文案变「▶开工(排队中)」、灰、禁用(`issues.rs:305-308`)|
| 待办池 | **没有 ⛔ 阻塞** | — | `can_transition_to(Backlog,Blocked)` 不成立(`model.rs:1636-1661` 无此边,`can_block()` 只认 `Todo\|InProgress\|InReview`)。hifi 原型 `index.html:1640-1642` 把待办池/待办合并处理、误画了这个按钮——**原型简化,与真实规则不符,本篇按真规则设计** |
| 待办 | ⛔ 阻塞 | `BlockIssue{id,reason}` | `(Todo,Blocked)` 合法;先弹必填原因框,输入中即忙态 |
| 进行中(正跑)| ■ 停止 | `CancelRun{id}` | `is_running`;中止后留在 `InProgress` 原地,不假装完成/失败 |
| 进行中(会话不活)| ▶ 开工 | 同上 `RunIssue` | `InProgress` 也在 `runnable` 里,再点是接回会话不是重开(`issue_run.rs:205-221`)|
| 进行中 | ⛔ 阻塞 | `BlockIssue` | `(InProgress,Blocked)` 合法 |
| 评审中(`pr_number≠0`)| ⬇ 合入 MR #N | `MergeIssuePr{id}` | 非合入中(合入中文案变灰,`issues.rs:556-587`)。**这一下同时完成合入+完成**——内部 dispatch 到 `TransitionIssue{Done}`(`dispatch.rs:2701`),不会再冒出第二个「✓点完成」|
| 评审中(`pr_number=0`)| ✓ 点完成 | `TransitionIssue{Done}` | `(InReview,Done)` 合法(唯一入 `Done` 边)——`CONTEXT.md`「完成」词条「没有 PR → 人点确认完成(人裁)」那一下 |
| 评审中 | ⛔ 阻塞 | `BlockIssue` | `(InReview,Blocked)` 合法 |
| 已完成 | 无按钮,「已完成·日期」| — | 重开(`Done→Todo/InProgress` 合法,用于返工)不放卡片,走详情面板多一步确认,避免误触 |
| 阻塞 | 「⛔原因」+「解除→待办」+「解除→进行中」| `TransitionIssue{Todo}`/`{InProgress}` | `(Blocked,Todo)`/`(Blocked,InProgress)` 均合法 |

**这套语义不依赖任何被砍掉的表(数据盘点核实)**:`can_transition_to` 是纯状态机函数,判断合法转移只看 `IssueStatus` 本身,不查 `workflow_run`/`agent`;上表里的「同项目无活在跑(串行锁)」「`is_running`」这两处忙态判断,来源是 `claude_conversation`(会话是否仍挂着活的进程/session,05 篇已把这张表定为"会话与活的对应关系正本")而不是已删除的 `workflow_run`——`workflow_run` 装的是"每次运行的成败与耗时",这条记账铁律在 V4 没有持久载体了(02 篇 §2.3,取代它的判据是远端 MR 合没合入),但"当前是否在跑"这个即时状态本就不需要历史记账表,`claude_conversation` 一张表就够。

**没照搬的**:V3 的「裸推进」按钮(`issues.rs:590-596`,不真起会话、纯手动推状态)。母文档 §5 钦定计划屏卡片就是六个语义(▶开工/■停止/续聊/⬇合入/✓点完成/⛔阻塞),本篇按此收窄:`Backlog↔Todo` 被拖拽/右键菜单接管(本来就是「排期」不是「开工」);`InProgress→InReview` 的裸推进(V3 叫「推到评审」)移到会话屏顶部(05 篇已定)。**「续聊」和「▶开工」是同一条命令 `RunIssue`**——`Done`/`InReview` 且已有 `claude_conversation` 行时(`issue_run.rs:190-200` resume 分支)文案换成「续聊」,语义是不改状态的后续对话,只出现在这两列,`InProgress` 下永远是「▶开工」。

### 2.3 拖拽(采纳预研的技术结论;产品规则用户改为全列统一)

预研结论:「能做,是小活」——Dioxus 0.7.9 拖放事件齐全,框架已在 JS 层全局 `preventDefault()` 了 `dragover`/`drop`(预研事实 2),不存在时序坑;跨列传卡片 id 不用 `DataTransfer`(桌面壳上是空操作),改用 `Signal<Option<IssueId>>`。**本篇全盘采纳预研的技术结论**;产品规则由用户改拍板为「所有列都能拖」(待拍-25 改,用户原话「拖动要统一,都能拖;动作性的拖完弹窗二次确认;卡面更清爽」),「只拖排期」的写法作废,落成新规则:

- **可拖 = 六列全部的卡**,不再限定只有待办池/待办两列带 `draggable`。
- **排期动作(待办池⇄待办、列内排序)**:效果不变,直接生效,不改任何状态——跨列发 `ScheduleIssue{id,week_of}`(拖进待办 `week_of=Some(选中周)`,拖回待办池 `week_of=None`);同列发 `ReorderIssue{id,after}`(列内调先后)。**落点是三处,不是只写库**——02 篇 §2.2/§3.4 把这条时序的具体设计留给本篇,下面单独展开(见「排期的三层写入」)。

**排期落到三个地方(缓存 / 文件 / 远端标签)**,其中远端标签那层今天还没接:

1. **缓存层(立即,同步)**:命令一发出,先 upsert `issue` 表对应的 `week_of`/`sort_order` 两列,立刻发 `IssueScheduled{id}`/`IssueReordered{id}` 事件,看板马上跟手反映拖拽结果——这一步是乐观更新,不等文件写完。
2. **文件层(同步,原地换表,不走 MR)**:`schedule_issue`/`reorder_issue`(`crates/bw-v4/src/app/plan.rs`)在更新完缓存之后,直接调用 `rewrite_week_activities` 把目标周 `.bw/plan/YYYY-Www.md` 里「业务活」「本周运作」两节的表格**原地换掉**(`week_plan_file::replace_table`),文件里人自己写的周目标第二段、风险记录、自定义小节等其它内容一个字不动。这一步是**同步直接写工作区文件**,不建活、不开分支、不开 PR、不提交 git;没有「MR 开着」这个中间状态,拖拽落定的那一刻文件已经是新内容。这是一处已知的欠账 —— 母文档规矩①说「凡写仓都是活 + MR」,排期是今天还没走上这条路的三处之一(另两处是改名片、发版本),登记在 `docs/LEFTOVERS.md` 的 V4A-15。
3. **远端标签(镜像,尽力而为)**:给活对应的远端 issue 打或挪一个 `bw/week:2026-W34` 这类标签,让不开 buddy 的人在平台上也看得出排期。**今天一行没做** —— `crates/bw-v4/src/git.rs` 里没有远端标签相关的函数。设计保留(母文档 §6.1「远端标签是镜像不是正本」仍然成立),只是还没接,和「远端一条没接」是同一条线(`docs/LEFTOVERS.md` V4A-11)。

**界面上不显示任何「排期待合入」小标**:文件层是同步直接写的,拖拽落定的那一刻文件已经是最新内容,没有「待合入」这个中间态要提示,看板与详情面板都不为此加一行小字。

**文件层写入失败时**:`rewrite_week_activities` 走的是本机文件系统写入(`std::fs::write` 一层),失败原因通常是磁盘/权限问题而不是网络;失败时缓存不回滚(缓存已经是当前 UI 的事实来源),人看到的是一条如实的报错;没有「重试开 MR」这件事可做,因为压根没有 MR。

- **状态动作(拖到进行中/评审中/已完成/阻塞)**:松手**不直接改状态**,先弹一个确认框,确认才真正发生对应命令——
  - 拖到**进行中** = ▶开工:框里显示这张活、开工工具与 workflow(默认按类别填好,可改),确认才发既有 `RunIssue`(真起 agent 会话,不是免费动作,不能只靠松手就触发)。
  - 拖到**评审中** = 推到评审中:确认后发既有的推评审命令(05 篇定义)。
  - 拖到**已完成** = ✓完成:确认框文案就是铁律里「人显式点」的那一下,只是点在弹窗上;确认后发既有 `TransitionIssue{Done}`。**只有评审中的活能被拖进这一列**——`can_transition_to` 的唯一 Done 入边是 `InReview`(§2.2 已给的合法表),这一条不因为改成拖拽而放松。
  - 拖到**阻塞** = ⛔阻塞:确认框要求填原因(**必填**,不填不能确认),确认后发既有 `BlockIssue{id,reason}`。
  - 取消确认框:不发任何命令,卡片弹回原列原位置,和什么都没发生一样。
- **不合法的转移**(状态机不允许的,如待办池直接拖到已完成、待办直接拖到已完成):松手**即弹回原位**并提示原因(如「待办池不能直接拖到已完成——先经过进行中和评审中」),不静默失败、不出现确认框。
- **状态永远只经 `can_transition_to` 判断**:无论走确认框还是详情面板按钮,发出的都是同一批既有命令(`RunIssue`/推评审/`TransitionIssue`/`BlockIssue`),`can_transition_to` 继续是唯一判分依据——拖拽只是给已有状态机多了一个触发入口,不新开一条判断逻辑,这是对「Done 永不自动」「同一件活绝不重复记账」两条铁律在拖拽场景下的落实。
- **卡面不摆按钮**:拖拽+确认弹窗覆盖了绝大多数日常操作,卡片只留标题、顶行徽记与 1-2 个 chip(§2.2);§2.2 给出的全部按钮语义原样保留在右侧详情面板(§2.6),当键盘可达性/不想拖时的替代路径——两条路径最终都是同一批命令,不是两套逻辑。
- **误触防护**:HTML5 原生拖放自带隐式阈值,不需要 BW 自己实现(预研 §7);状态动作还多一层防护——从来不会「松手即生效」,永远先过确认框。
- **Windows**:wry 默认的文件拖放处理器会让 WebView2 屏蔽页面内拖放,**必须**在 `Config::new()` 链加 `.with_disable_drag_drop_handler(true)`(`dioxus-desktop-0.7.9/src/config.rs:164-166` 官方注释原话)——落在 01 篇 §3.1 的 `main.rs` 骨架,01 篇当前没有,**本篇提出要求,01 篇据此回填**;BW 未用「文件拖进窗口」,加这行没代价。
- **macOS**:预研没查到会拦截页面内拖放的框架级机制,但「WKWebView 桌面端对 `draggable` 的支持程度」没有权威一手文档确认——**如实标注:还没有真机验证过一次**,§5 补一条。

### 2.4 发版本:一次点击直接写,不是三步向导

发版本是周头上的一个按钮,点一下直接写仓,没有向导也没有 MR:

- **没有选活步骤**:按钮点下去自动取**当前看板里「已完成」列的全部卡片**(`crates/app-shell/src/screens/plan/mod.rs::board_head`——`p.board.columns` 里 `status == Done` 那一列的所有卡),不弹框让人勾选,也不区分业务活/运作活。
- **没有版本号输入框**:版本号是从 `.bw/project.toml` 的 `current_version` 按 `主.次` 递增算出来的下一个版本(`next_version()`:取最后一个 `.` 之后的数字 +1),人不填、也改不了这次要发哪个号。
- **说明写死**:传给 `CutRelease` 的 `note` 固定是字符串「本周完成的活」,没有「用 agent 起草说明」这个可选动作,也没有别的文案入口。
- **没有活、没有分支、没有 PR**:点下去直接调用 `bw_v4::app::plan::cut_release`——先给 `.bw/releases.md` 补空骨架(如果还没有这份文件),再把选中的活挨个查出编号,把这一行(版本号、今天日期、写死的说明、活号列表、来源"人发")追加进这份文件,**成功写进文件之后才把这些活的 `issue.version` 回填成这个版本号**——先写发版记录、写成了再回填版本号,顺序不能反,否则重复点一个已经存在的版本号会把活打上标签、发版记录里却没有这一次,账就对不上。整个过程不建 `kind='light'` 的轻量活,不开分支,不开 PR,没有"停在评审中等人合入"这一步——写完这一刻就是发布完成。

**两条护栏**:①版本号认不出「数字.数字」格式(最常见的是还没设过在研版本、`current_version` 还是「(待填)」这类占位文案)时,`next_version` 返回空字符串,按钮直接置灰点不动;`cut_release` 内核侧同样拒收——版本号是空的或包含 `(` 这种明显是占位文案的字符,直接报错拒绝,双重防线不给发版记录写进一行叫「(待填)」的版本。②`release_file::append_row` 按版本号幂等:同一个版本号已经存在的行不会追加第二次,也就不会给活重复打标签。

**没有「合入才生效」这一步**:没有分支就没有合入,写文件的那一刻就是主干上的内容。「包含哪些活」由 `.bw/releases.md` 那一行的自由文本(活号列表)携带,活挂哪个版本只用 `issue.version` 一列。

### 2.5 预览未合入

**能不能点**:当前选中周存在至少一张 `kind='ops'` 的活处于 `InReview` 且 MR 未合入才能点开——覆盖运作活①(最常见)和③的「合并调整」/「历史回填」两步(它们的产出也动 `.bw/plan/YYYY-Www.md`/`.bw/PROJECT.md` 草稿,待拍-21 原话「运作活的 MR 合入前」没限定死在①,本篇按更宽口径设计)。**没有单独的 `.bw/plan/history.md` 文件**——02 篇信息住哪那次盘点已明确取消:回填的历史周和本周文件同目录、同格式,只靠 front matter 的 `origin: backfill` 区分(02 篇 §2.5 样例),历史回填动的还是 `.bw/plan/YYYY-Www.md` 本身,不是另一份历史文件。没有这样的活时开关灰,悬浮提示原因。

**打开后**:从该活 worktree(`<主工作区>-issue-<n>` 约定,`bw-engine::workspace::provision_issue_worktree`)读 `.bw/metrics.toml` 和这周的 `.bw/plan/`,用同一套解析代码重渲染周头——纯粹换个目录读文件,顶部横幅「预览 · 未合入」。

**指标现值要不要跟着预览切换,盘点之后口径变了**:V4 已经没有 `observation` 表(02 篇 §2.1)——可重算的指标每次现场算(比如「本周合入活数」查 `git log --merges`),不可重算的手填读数直接写进 `.bw/plan/YYYY-Www.md` 的「本周指标读数」段(02 篇 §2.5 样例),**和周目标、活清单同一份文件**。这意味着旧口径「观测数据不预览、指标卡现值保持读库真实值」已经不成立——库里根本没有一份独立于这份文件的「真实值」可以保持。新口径分两半:
- **可现算的指标**(查 `git log`/`git log --numstat` 这类):不受预览开关影响,永远显示对主干现算的结果——这类数字来自 git 历史而非某份草稿文件,预览开关切的是"读哪个目录",不改变"现算查询打在哪条历史线上"(现算默认打在主干,不追进未合入的分支)。
- **手填读数**(周文件「本周指标读数」段里的数值):平时显示"最近一次合入主干的周文件"里记的数;**打开预览后额外展示这份草稿即将写入的新读数**(即该 worktree 里「本周指标读数」段的内容)——这正是评审运作活①/③的 MR 时最需要看的东西之一(这次更新的读数对不对),不再有旧口径里"observation 和 worktree 无关"这层技术限制。两份读数(主干现有 vs. 草稿待合入)预览时并排标注来源,不互相覆盖。

**关闭**立刻恢复正常来源(只显示主干现有读数),预览本身没写过任何东西,没有副作用。

### 2.6 右侧详情

| 字段 | 来源 | 可改吗 → 走哪条命令 |
|---|---|---|
| 标题、说明 | `issue.title`/`desc` | 可改(沿用既有编辑路径)|
| 类别 | `issue.stage` | 一般创建时定,改动影响默认映射,留 04 篇决定要不要开放改 |
| 开工工具 | `issue.tool` | 可改 → 04 篇范围的命令,本篇只留入口 |
| workflow/加挂技能 | `issue.workflow` | 可改 → `SetIssueWorkflow{id,workflow}`(新命令,§3)|
| 推动指标 | `issue.metric_key` 单列(02 篇 §2.2 改定:一活一指标,不建 `issue_metric` 关联表;要挂多个就拆活)| **未做**:设计是可改、单选一个已定义的引领/滞后指标;实际没有对应命令(见 §3——`SetIssueMetric` 没有接进 `crates/bw-v4/src/command.rs`),建活时这一列恒为空字符串,详情面板这一行目前只能显示"未挂" |
| 周 | `issue.week_of` | 可改 → `ScheduleIssue{id,week_of}`——和拖拽同一条命令,详情面板给个下拉,不强迫必须靠拖拽(可达性,也让「全部」视角选中的卡能改周);走 §2.3「排期的三层写入」同一条路径,不是详情面板专属的另一套逻辑 |
| 版本 | `issue.version` | 由所属周/发版决定,本篇不在这里开放改 |
| 来源 | `issue.origin` | 只读,历史事实不允许事后改 |
| 远端 issue/MR | `remote_number`/`pr_number` | 只读 |
| 会话记录 | `claude_conversation` 按 `issue_id` 查(05 篇已把这张表定为"会话与活的对应关系"正本;02 篇删除了 `workflow_run`,这里不再有"每次运行的成败与耗时"这类字段——那条记账铁律在 V4 没有持久载体了,判据换成远端 MR 合没合入,母文档 §6.3 已知情)| 只读,时间倒序,展示 `claude_session_id`/`workspace_path`/`branch_name` 这类会话身份线索,不展示逐次运行成败 |
| 产物 | 现算,`git log --name-only` 查该活分支/worktree 改过的文件(02 篇 §2.6「现算」表,没有产物登记表)| 只读 |

**拖拽统一之后,详情面板是按钮语义(§2.2)唯一常驻的落脚点**——▶开工/■停止/⬇合入/✓点完成/⛔阻塞这一整套按钮原样摆在这里,和拖拽+确认弹窗(§2.3)是同一批命令的两条触发路径,不想拖、或在「全部」视角选中一张卡时都能用。底部另有「去会话 →」(未开工先发 `RunIssue` 再跳,已开工就是跳转+聚焦)、「蒸馏」复用 05 篇已定的 `DistillSkillFromIssue`。重开一张 `Done` 的活放在详情面板、要一次确认,不放卡片一键——密集卡片容易手滑。

### 2.7 模块边界

`screens/plan/`(01 篇目录树已预留)**只做布局与状态**——周列表/看板/周头/详情面板几个 Dioxus 组件、从 `Command`/`Event` 拼出来的 ViewModel;「这周有没有 `.bw/plan/` 文件」这类推导、`week_of` 的日期算法都下沉到内核,UI 只消费结果。

命令/事件(只列名字+一句话,与 01 篇对齐;标「较 01 篇改动」的见 §3):

| 命令/事件 | 一句话 |
|---|---|
| `ScheduleIssue` | 把活排进(或移出)某一周;跨列拖拽/右键菜单/详情面板改周都发它;`crates/bw-v4` 内部触发§2.3「排期的三层写入」——缓存立即动,文件层同步原地换表追上,UI 不额外发命令(A 刀落地后改:不再是"轻量活+MR",见 §2.3)|
| `ReorderIssue` | 待办池/待办列内调先后,不碰状态;同样触发§2.3 三层写入 |
| `CreateIssue`(含默认映射填充)| 「新建活」按钮,创建时按类别自动填工具/workflow 默认值 |
| `StartWeekPlanning` | 「开始本周」:建运作活①并跳会话屏 ▶开工 |
| `CutRelease` | 「发版本」按钮一次点击直接生效:自动取本周已完成的活、算出下一个版本号、写死说明,直接往 `.bw/releases.md` 追加一行并给这些活回填 `issue.version`;不建活、不开分支、不开 PR(A 刀落地后改,见 §2.4)|
| `SetCurrentVersion` | 切在研版本。母文档把它归成「纯本机动作,不建活」,但 `current_version` 只存在 `.bw/project.toml` 这份仓文件里,所以这一下也是写仓 —— 这处张力本篇不选边,列在 §6 开放问题 |
| `TogglePreview` | 开/关「预览·未合入」,换一次读的来源 |

拖到状态列(进行中/评审中/已完成/阻塞)不新增命令——确认框确认后发的是既有 `RunIssue`/推评审命令(05 篇)/`TransitionIssue`/`BlockIssue`,和详情面板按钮走同一条路径(§2.3/§2.6),此处不重复列出。

## 3 · 工程对照

**crate/目录**:`crates/app-shell/src/screens/plan/`(01 篇目录树已画出位置)。

计划屏用的是 `bw_v4::command`(`Command`/`Event` 全新一对枚举)、`bw_v4::store::V4Store::open`、`bw_v4::model`——一个字都不与旧内核共享,旧 crate 一行没改。

**`issue` 表增量(照抄 02 篇 §2.2/§3.1,本篇不再另定一套字段名)**:

```sql
week_of TEXT NOT NULL DEFAULT '', version TEXT NOT NULL DEFAULT '', tool TEXT NOT NULL DEFAULT '',
kind TEXT NOT NULL DEFAULT 'business', origin TEXT NOT NULL DEFAULT 'human',
workflow TEXT NOT NULL DEFAULT '', sort_order REAL NOT NULL DEFAULT 0,
metric_key TEXT NOT NULL DEFAULT ''
-- week_of: ISO 周文本 "2026-W34",''=待办池 · kind: business|ops|light(轻量活,发版本/编辑名片用)
-- origin: human|agent_split|auto|backfill · tool: claude_cli|cursor|open_design
-- workflow: 实际用的 workflow/技能名,现算用量统计用 · sort_order: 待办池/待办列内排序,浮点插入排序
-- metric_key: 这张活预期推动的指标键(`.bw/metrics.toml` 里那条指标的 id),单列;一活一指标,要挂多个就拆活(02 篇 §2.2,不建 issue_metric 关联表)
```

**这 9 列全部是缓存,不是正本**——排期/版本/工具/种类/来源/workflow/类别/顺序/挂的指标的正本是 `.bw/plan/YYYY-Www.md` 活清单那一行,写入顺序、对账时机见 §2.3(02 篇 §2.2 已定性,本篇负责落地时序)。

**V4 是新库,直接写全,不是给存量表加列**(02 篇 §2.7/§3.1/§3.2):开发期 `schema.sql` 每次改了删库重建,`V4Store::open()`(`crates/bw-v4/src/store/mod.rs`) **不需要**为这 9 列写 `add_column_if_missing`——它们随新库首次创建就已经在 `issue` 的 `CREATE TABLE` 语句里,不是增量 diff。`add_column_if_missing` 双守卫从"第一个真实用户开始用 V4 库存了数据"(内部试点)那一刻起恢复执行,此前不适用(CLAUDE.md「schema 迁移双守卫」保护的是存量库,V4 开发期还没有这样的库)。读活的 `SELECT`(`crates/bw-v4/src/store/issue.rs`)一次列全这 9 列,`ORDER BY` 是「先 `sort_order` 再 `number` 兜底」;领域结构体是 `crates/bw-v4/src/model.rs` 的 `Issue`,看板 VM 在 `crates/app-shell/src/vm.rs`。memory 里有真实踩过的坑(`project_id` 进了 schema 但读侧全链路没接上)——schema → 领域结构体 → SELECT → VM 四处要一起改。

**没有 `week_plan`/`release` 两张表——02 篇信息住哪那次盘点已删除,本篇不再新建**:
- **周列表**靠扫 `.bw/plan/` 目录现算(§2.1),不需要一张索引表告诉 buddy 有哪些周。
- **发版记录**唯一正本是 `.bw/releases.md`(02 篇 §2.5「唯一正本,库不存副本」),活挂哪个版本只用 `issue.version` 一列,不需要 `release`/`release_issue` 两张表或任何关联表(§2.4)。

**`project` 表不新增列——`current_version`/`standard_version` 不进库**:02 篇 §2.5/§3.1 明确这两个值(连同 `[chat]` 配置)只住在 `.bw/project.toml`,总览/计划屏每次要显示都现解析这份文件,`project` 表结构沿用现有定义,不新增任何列。本篇早前版本写过「`project` 表加 `current_version TEXT`」,按 02 篇改正。

**命令签名(较 01 篇改动,理由写在这里)**:

```rust
// 较 01 篇改动:合并 01 篇的 ScheduleIssue + UnscheduleIssue 为一条——
// 排进/移出是同一字段的写入,拆两条违反最小化原则。01 篇的 Unschedule
// 退场。
ScheduleIssue { id: IssueId, week_of: Option<String> },  // None = 移出回待办池

// 字段名统一用 after(与预研 §6.2 一致;01 篇早期伪码用 before,语义
// 等价,01 篇据此改名)。
ReorderIssue { id: IssueId, after: Option<IssueId> },    // None = 排到列首

// 直接写仓,没有中间的活/分支/MR(欠账见 LEFTOVERS V4A-15):
CutRelease { project_id: ProjectId, version: String, note: String, included: Vec<IssueId> },

// 较 01 篇改动:01 篇早期叫 PreviewIssueWorktree{id}(只有「开」)。
// 本篇需要能开能关,改名 TogglePreview、id 包进 Option,01 篇据此改名。
TogglePreview { id: Option<IssueId> },

// current_version 只存在 .bw/project.toml(一份仓文件),不在库里。
// 所以「切在研版本」这一下也是写仓:继续"不建活"就等于绕开"凡写仓都是
// 活"的规矩,还是并入轻量活+MR,本篇不选边,列在 §6 开放问题里。
SetCurrentVersion { version: String },

SetIssueWorkflow { id: IssueId, workflow: String },  // 已实现,详情面板改 workflow 用

// SetIssueMetric 未做:crates/bw-v4/src/command.rs 里没有这一条,
// issue.metric_key 建活时恒为空字符串,今天没有任何命令能改它。
// 详情面板上的「推动指标」因此是未做,见 §2.6/§4。

// CreateIssue「含默认映射填充」不是加字段:crates/bw-v4/src/app/issue.rs
// 处理命令时按 category 查 issue-policy.toml 映射自动填 tool/workflow
// (已实现),UI 不自己传这两个字段(避免和 04 篇映射表出现两份真相)。只
// 新增 week_of: Option<String>(周头「新建活」传选中周,其它入口传 None)。
```

事件(与 `crate::command::Event` 对齐):`IssueScheduled{id, week_of}`、`IssueReordered{id}`(§2.3 缓存层立即发出;文件层是同步写,不另发一个"文件已同步"事件)、`ReleaseCut{version, rows_written}`(改:`CutRelease` 命令处理完就同步发出,不是等谁合入才发——A 刀没有"合入"这一步;`rows_written` 是这次追加有没有真的落地,已经存在的版本号会是 `false`)、`CurrentVersionChanged{version}`、`IssueCacheRefreshed{week, updated}`(`RefreshIssueCacheFromPlan` 跑完发出)。`PreviewToggled`/`IssueWorkflowChanged`/`IssueMetricChanged` 这几条这几条设计里有的事件目前都没有对应的命令接进来(`TogglePreview`/`SetIssueMetric` 未实现,`SetIssueWorkflow` 已实现但目前复用其它事件通知界面刷新,没有专属事件)。PTY/终端事件不属于本篇(05 篇范围)。

**`sort_order` 类型**:本篇选浮点数插入排序(新卡片插进两张卡之间取中间值),不是每次拖动重排整列——多数拖动只改一行,代价是精度用尽后需要一次性重新铺号,再平衡算法本篇不展开。预研 §9 开放问题②没选定方案,本篇选定了类型但没展开再平衡算法。

## 4 · 边界与失败

**不做**:甘特图/里程碑实体(待拍-04 已定,版本就是里程碑);拖拽绕过确认弹窗直接改状态(§2.3 确认框是状态动作的唯一入口,拖拽本身不允许跳过它直接发 `TransitionIssue`/`RunIssue`/`BlockIssue`);多版本线(待拍-04);「全部」视角下的跨列拖拽(§2.3 排除在首版范围外,`draggable` 只在选中某周模式下挂,见 §6 开放问题 4)。

**失败如实标注**:
- **远端 issue 建失败**(网络/权限/仓未挂载):本地活照样落库、显示在看板,只带「未同步」小标,`remote_number` 留 `0`(既有「创建不破」口径,`model.rs:1699-1706`)——不因远端失败就整张活创建失败,也不假装已同步。
- **预览 worktree 缺文件**:该 worktree 缺 `.bw/metrics.toml`/`.bw/plan/`(比如运作活①还没跑到写这一步)时周头对应字段显示灰态「预览:暂无内容」,不报错崩溃、也不悄悄回退去读主干正式文件。
- **拖拽到不合法的状态转移**:不发命令、只弹回原位并提示原因,不静默失败也不报错(§2.3)。
- **状态动作的确认框被取消**:不发任何命令,卡片留在原列原位置,和什么都没发生一样(§2.3)。
- **发版时勾的活其实已不是 `Done`**(并发场景理论可能):`CutRelease` 建轻量活那一刻应校验 `included_issue_ids` 每一张仍是 `Done`,不是就诚实拒绝这次发版、不悄悄摘掉再继续——这个校验只在建活时做一次,`.bw/releases.md` 那一行内容随 MR 一起冻结,合入时不重新校验这份名单(合入只负责让这份已经冻结的 `.bw/releases.md` 行成为主干正本,库里没有 `release` 表要落)。
- **`SetCurrentVersion` 切到没出现过的版本号**:允许(在研版本本是自由文本),只是从此成为新的在研版本。

## 5 · 验收与读回

- **拖拽排期,SQL 读回(缓存层)**:`sqlite3 <db> "SELECT week_of, sort_order FROM issue WHERE id='<uuid>';"` ——拖一张活到某周待办列,读回 `week_of` 变成目标周文本;再做一次列内排序,读回 `sort_order` 顺序符合拖动结果。这条同时验证「杀进程重开顺序一致」:`sort_order` 落库而不是像 `hifi/index.html:1123` 那样只活在前端内存的 `state.kanbanOrder` 里,重启后 `ORDER BY sort_order, number` 读出来的顺序应与关闭前一致;顺带确认左栏周列表顺序、待办池是否仍显示全部未排期活也和重开前一致。

- **拖拽排期,文件层读回(§2.3 的第②层)**:文件层是同步直写,拖拽命令返回时文件应该已经是新内容,不需要等谁跑完——`cat <ws>/.bw/plan/2026-W34.md` 的「业务活」表格「顺序」列应和刚才读到的 `sort_order` 对应,文件里周目标、风险这类人手写的其它内容应该一个字没变;`git -C <ws> status --porcelain <ws>/.bw/plan/2026-W34.md` 应显示这份文件有未提交的改动(这一层不提交 git,只是工作区文件写入);`sqlite3 <db> "SELECT count(*) FROM issue WHERE kind='light' AND title LIKE '排期调整 %';"` 应为 0——不建"排期调整"这张活。再拖第二张活到同一周,`.bw/plan/2026-W34.md` 的表格应该整体反映两次拖拽后的最新结果,不产生历史痕迹。

- **状态动作走确认框,SQL 读回**:拖一张 `InReview` 的活到已完成列,确认框点确认后 `sqlite3 <db> "SELECT status, settled_at FROM issue WHERE id='<uuid>';"` 应变成 `Done` 且 `settled_at` 非空;拖一张 `Todo` 的活到进行中列,确认框点确认后应出现一行新的 `claude_conversation`(真起了会话,不是假装;V4 没有 `workflow_run` 表,「真起了会话」的证据是这张表而不是一行运行记账,见 §2.2)。

- **确认框取消,不发命令**:拖一张活到状态列后在确认框点取消,`status` 前后查两次应完全没变,且不产生新的 `claude_conversation` 行。

- **`can_transition_to` 守卫读回(非法转移)**:故意拖一张待办池的活到已完成列(模拟误拖),应松手即弹回、不出现确认框,`sqlite3 <db> "SELECT status FROM issue WHERE id='<uuid>';"` 前后各查一次应完全没变;再故意构造一次非法转移(比如给 `Backlog` 的活发 `TransitionIssue{Blocked}`),应被拒绝且 `status` 原地不动。

- **发版本,一次性读回**:点一下按钮,写入与回填版本号在同一次命令处理里同步完成,没有「评审中等合入」这个中间态,所以读回也是一次性的:
  ```bash
  tail -n 3 <ws>/.bw/releases.md   # 点完按钮那一刻这一行已经在主干仓文件里,不需要等第二步
  sqlite3 <db> "SELECT number, version FROM issue WHERE version != '' AND project_id='<pid>' ORDER BY updated_at DESC LIMIT 5;"   # 这次选中的活应该已经回填了这个版本号
  sqlite3 <db> "SELECT name FROM sqlite_master WHERE type='table' AND name='release';"   # 空结果——V4 schema 里从未定义过这张表(02 篇 §2.1/§2.7)
  sqlite3 <db> "SELECT count(*) FROM issue WHERE title LIKE '发版本 %';"   # 应为 0——不建活,这条查询是反证:不该有一张叫「发版本 vX」的 issue
  ```
  再点一次同一个版本号(模拟重复点击):`.bw/releases.md` 不应该多出重复行,已经回填过版本号的活也不会被再处理一次(`release_file::append_row` 按版本号幂等)。

- **深链截图**:`BW_OPEN=<项目名> BW_PANEL=plan` 启动,stderr 打出 `[BW_OPEN]` 即渲染证据;「选中本周」和「全部」两种视角各截一张,点一张卡再截一次详情面板,确认渲染的是真实数据而非占位假数据。

- **macOS 拖拽真机验证**(§2.3 已标注源码分析没能覆盖):真机上做一次「待办池拖到待办」的完整鼠标操作,确认卡片跟手移动、松手后 `week_of` 真的改变——不能只凭源码分析结论判定放行。

## 6 · 开放问题

1. **`sort_order` 的再平衡算法**:本篇选定浮点数插入排序(§3),没展开「精度用尽后怎么批量重新铺号」,留到实现阶段按需处理,还是现在就定一个方案(比如每隔 N 次拖动整列重算)?
2. **「agent 起草发版说明」的底层机制**:这次会话不挂在任何一张 `Issue` 上,但要真的起一次 agent 读文件生成文本——是复用 `RunIssue` 相关机制开一个不落库的临时会话,还是需要一种新的「无活会话」通道?04/05 篇目前都没覆盖这种用法。
3. **`TogglePreview` 的并发场景**:待拍-21 只定了「合入前可以预览」,没定「同一周有两张评审中的运作活同时挂着」时预览开关该切哪一张、要不要给选择器。
4. **「全部」视角下要不要也开放拖拽**:§2.3 排除在首版范围外(「全部」不按周分列,拖拽改哪一周不直观),但用户是否希望至少能把一张待办池的卡直接拖进当前在研的这一周,本篇没有验证过这个需求。
5. **`SetCurrentVersion` 该不该建活,母文档与 02 篇之间有没消解的张力**:母文档「三条不变的规矩」母文档把「切在研版本」和「合入、点完成」并列为「只动本机库、不碰仓的纯人工动作」,归类不建活;但 02 篇已把 `current_version` 移出 `project` 表、只存进 `.bw/project.toml`——这是一份仓文件,按同一条规矩「凡写仓都是活,写仓一律走分支+MR,唯一例外是新建空仓首提交」,切版本号已经不再"只动本机库"了。本篇不擅自替母文档拍板(既不单方面加一条新的"直接写仓不经 MR"例外,也不单方面把它并入常规轻量活+MR 流程),按 02 篇的存储位置如实设计存根签名(§3),具体走哪条路径留给用户裁决。
6. **排期文件层要不要防抖**:每次 `ScheduleIssue`/`ReorderIssue` 都同步触发一次 `rewrite_week_activities`,连续快速拖拽会连续触发多次文件写入——要不要为高频拖拽加一个防抖(攒够几次改动或几百毫秒再写一次文件),还是接受"逐次直写"现在这个更简单的实现,留到实现阶段按真实拖拽频率决定。

(原第 2 条「发版本是否要走活+MR」已按母文档 §2.6 规矩①改正,不再是开放问题,见 §2.4。)

## 与代码的关系

这篇不改 `crates/`。开工时按 §2/§3 顺序在 `crates/app-shell/src/screens/plan/` 建文件,`issue` 表的 9 列增量随 V4 新库的 `schema.sql` 一次写全(§3——开发期不需要 `add_column_if_missing`,试点起才恢复双守卫);**没有 `week_plan`/`release` 需要落地**,周列表靠扫目录、发版记录靠解析 `.bw/releases.md`,都是只读解析器(02 篇 §3.3),不是 schema。第 3 节是开工清单,第 5 节是验收清单。01 篇需据此回填 §2.6 命令表的几处改名(`ScheduleIssue`/`UnscheduleIssue` 合并、`PreviewIssueWorktree`→`TogglePreview`、补 `SetCurrentVersion`/`SetIssueMetric`)和 `main.rs` 的 Windows 拖放配置。
