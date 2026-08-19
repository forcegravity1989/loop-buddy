# 06 · 计划屏

> **30 秒导读**:这篇管 V4 计划屏——一个界面里同时装下「这周要干什么」和「V3 那种六列看板」。给复核设计的用户、以后写代码的会话看。**状态:详细设计稿,待用户复核,尚未开工写代码。** 设计事实源是 [`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md)(下称母文档),与它冲突时以母文档为准;拖拽部分另有专门预研 [`../research/kanban-drag-dioxus.md`](../research/kanban-drag-dioxus.md)(下称预研),已确认技术可行,本篇负责把它落成产品规则。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)——本篇不新开代号系列,沿用「待拍-NN」。

## 0 · 这篇管什么、不管什么

**管**:计划屏——左栏周列表怎么算、中栏六列看板每列是什么/卡片长什么样/按钮各走哪条命令、拖拽的范围与替代路径、周头(周目标/进度/运作活 chip/新建活/发版本/预览开关)、「全部」视角的筛选条、右侧活详情滑出、发版本三步、预览未合入。对应母文档第 2/3 站、§2.5(指标/周目标/活的关系)、§2.6 全景表里排期相关部分、§5 计划入口、§6 `issue` 新列与 `week_plan`/`release` 两张新表、待拍-04/05/06/07/08/12/21/25。

**不管**:运作活①会话里 agent 具体做什么——[09-ops-workflows.md](09-ops-workflows.md),本篇只消费它挂了 `week_of` 的产出。开工工具/workflow 怎么注入——[04-tools-and-workflows.md](04-tools-and-workflows.md)。会话屏三栏、终端——[05-session-screen.md](05-session-screen.md)。项目群通知——[07-notify-and-chat-group.md](07-notify-and-chat-group.md)。规范铺底/`.bw/issue-policy.toml` 正本——[03-standard-and-backfill.md](03-standard-and-backfill.md)。

**与 01 篇的边界**:01 篇(未定案)给了「一屏一模块」等建法规矩和 `Command`/`Event` 总线第一版草案(含 `ScheduleIssue`/`UnscheduleIssue`/`ReorderIssue`/`CutRelease`/`PreviewIssueWorktree`)。**本篇是这几条命令真正落地的地方**,§3 对草案做了几处收窄改名,标「较 01 篇改动」,01 篇日后据此回填。

## 1 · 用户看到什么、做什么

人从左栏点「计划」,或总览「本周计划进度」块点「去计划 →」,或总览空态点「开始本周」,进的是同一个屏。

**左边**是一列窄周列表:最上面是「全部」,往下按时间倒序(本周置顶,标「· 本周」)、再是历史周。最下面是「在研版本」下拉,只影响默认值,不影响周列表本身。

**中栏**平时是「选中一周」的样子:顶上一条「周头」——周目标一句话、一条按业务活状态分段的进度条、运作活①②③各一个状态 chip、右边「新建活」「发版本」和「预览·未合入」开关。下面是**六列看板**:待办池、待办、进行中、评审中、已完成、阻塞,列头下一行小字写清这一列是什么意思,卡片按 V3 样子(标题整行不省略、操作区按状态互斥)。业务活和三张运作活同在一个看板,运作活的卡多一个「运作」标记。**待办池不管左边选哪一周,永远显示全项目所有未排期的活**——设计决定,理由见 §2.2。选「全部」时周头换成一条筛选条(类别·版本·来源·状态·关键字),看板照旧,只是六列里装的是筛完的全部活。点卡片标题,右边滑出 360px 详情面板(字段见 §2.6),底部「去会话 →」「蒸馏」。

**拖拽**统一到六列都能拖(第五轮用户拍板改的,待拍-25 已按此改;用户原话「拖动要统一,都能拖」):待办池⇄待办之间(或列内部)是「排期」动作,松手直接生效,不改任何状态;拖到进行中/评审中/已完成/阻塞是「状态动作」,松手弹一个确认框才真正发生——拖到进行中 = ▶开工(框里显示开工工具/workflow,确认才真起 agent 会话);拖到评审中 = 推到评审中;拖到已完成 = ✓完成(仍是铁律里「人显式点」的那一下,只是点在弹窗上,且只有评审中的活能被拖进这一列);拖到阻塞 = ⛔阻塞(框里填原因,必填)。不合法的转移(状态机不允许的,比如待办池直接拖到已完成)松手即弹回原位并提示原因,不静默失败。**卡面因此不摆按钮**——拖拽+确认弹窗覆盖日常操作,卡片只留标题与 1-2 个 chip;§2.2 给的全部按钮语义原样保留在右侧详情面板(§2.6),当作不想拖时的替代路径。第一轮「不拖拽」、第四轮「只拖排期」两种写法都已作废,见 §2.3。

**发版本**是三步——勾这周完成的活(默认全勾)→ 填版本号(默认接在研版本,新项目默认 `v0.1`)→ 确认;确认后 buddy 建一张轻量活「发版本 vX」(不起 agent 会话)、开分支把这行说明提交进 `docs/releases.md`、开 MR,这张活停在评审中等人处理——真正写 `release` 表一行、可选打 tag,是有权限的人后续点「合入并完成」的那一刻(§2.4)。可选「让 agent 起草说明」。**预览未合入**只在当前周有一张运作活①(或③的合并调整/历史回填)还在评审中、MR 未合入时能点开,从它的 worktree 读文件渲染周头,顶部横幅「预览 · 未合入」,关掉恢复正常(§2.5)。

## 2 · 设计

### 2.1 周模型

**`week_of`**:ISO 8601 周文本,形如 `2026-W34`(周一为起点),按机器本地时区判定「现在是哪一周」(BW 单机应用,与总览/运行记录等既有时间戳字段口径一致)。存文本不存时间戳,是本篇对预研 §9 开放问题②的答案:①直接对应仓文件名 `docs/plan/YYYY-Www.md`(03 篇已定),免去格式转换;②避免「周边界算哪个时区」的歧义;③展示位置本来就吃这个文本。

**周列表怎么算**:「全部」下面列出的周 = `week_plan` 索引表里有行的周(有 `docs/plan/` 文件、运作活①真跑完过)∪ `issue.week_of` 上出现过的周(哪怕还没有正式文件),取并集去重。**本周永远置顶**,哪怕它还没有 `week_plan` 行、也没有活排进去——空的本周才有地方触发「开始本周」。

**周头数据来源**:周目标读 `week_plan` 索引表的 `goal` 字段(正本是 `docs/plan/YYYY-Www.md`,索引表是运作活①的 MR 合入时写入的库内缓存,不是另一份权威);进度条按 `week_of` 匹配到的业务活+运作活状态分段;运作活①②③ chip 直接查这周 `kind='ops'` 的三张活状态;发版默认版本号读 `project.current_version`。

**空态**:本周没有 `docs/plan/` 文件(母文档 §2.6 用户四问第 2 条已定的判据)时,周头整块换成「本周还没有计划」+「开始本周」按钮,看板照常(待办池永远有内容)。点按钮发 `StartWeekPlanning`(01 篇已定义),建运作活①并跳会话屏 ▶开工——和总览空态、其它入口点的是同一个按钮语义。

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

**卡片字段**:沿用 V3 骨架的信息部分(`issues.rs:335-338` 顶行 mono + 标题 + 左边框 3px 状态色),**不沿用操作行**——第五轮拖拽统一后卡面不摆按钮(§2.3),V4 扩展:顶行 `#编号 · 类别`(类别 = `issue.stage` 复用的五个标签:原型/构建/优化/运营推广/**运维**——这个「运维」是 `StageKind::Ops` 类别标签,和下面「运作活」的「运」字不是一回事,前者是活的性质,后者是 `issue.kind='ops'` 标记「buddy 自己的三张运作活之一」,同一张卡可能同时出现两个不相关的「运」字,别混)+ 来源徽记(`kind='ops'` 显示「运作」优先;否则 `origin` 非「人建」时显示「agent 拆」/「自动建」/「回填」,「人建」不加徽记减少噪音)。标题整行不省略。1-2 个 chip 从「工具/workflow」「推动指标数」里挑:`tool` 非空显示开工工具简称,`issue_metric` 关联表(02 篇 §2.2)里有行时加「→N 指标」,`workflow` 与默认值不同时优先显示 workflow 名。类别已在顶行,不重复做 chip。`pr_number` 非零加一行 mono 的 `MR #N` 徽记。

**按钮语义(照抄 V3 真实代码 `crates/app-desktop/src/screens/op/issues.rs`,不是照抄高保真原型的简化演示——两者有出入,下表已用真代码口径修正)**:第五轮拖拽统一后,这套语义**不再画在卡面上**——同样的命令现在有两条触发路径:①拖到对应列 + 确认弹窗(§2.3,状态动作专用,「排进本周」这类排期动作不经过这张表);②右侧详情面板(§2.6)按钮,当键盘可达性/不想拖时的替代路径。下表描述的是命令本身的前置条件与忙态,两条路径共用同一套判断。

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

**没照搬的**:V3 的「裸推进」按钮(`issues.rs:590-596`,不真起会话、纯手动推状态)。母文档 §5 钦定计划屏卡片就是六个语义(▶开工/■停止/续聊/⬇合入/✓点完成/⛔阻塞),本篇按此收窄:`Backlog↔Todo` 被拖拽/右键菜单接管(本来就是「排期」不是「开工」);`InProgress→InReview` 的裸推进(V3 叫「推到评审」)移到会话屏顶部(05 篇已定)。**「续聊」和「▶开工」是同一条命令 `RunIssue`**——`Done`/`InReview` 且已有 `claude_conversation` 行时(`issue_run.rs:190-200` resume 分支)文案换成「续聊」,语义是不改状态的后续对话,只出现在这两列,`InProgress` 下永远是「▶开工」。

### 2.3 拖拽(采纳预研的技术结论;产品规则第五轮改为全列统一)

预研结论:「能做,是小活」——Dioxus 0.7.9 拖放事件齐全,框架已在 JS 层全局 `preventDefault()` 了 `dragover`/`drop`(预研事实 2),不存在时序坑;跨列传卡片 id 不用 `DataTransfer`(桌面壳上是空操作),改用 `Signal<Option<IssueId>>`。**本篇全盘采纳预研的技术结论**;产品规则第五轮由用户改拍板为「所有列都能拖」(待拍-25 改,用户原话「拖动要统一,都能拖;动作性的拖完弹窗二次确认;卡面更清爽」),第四轮「只拖排期」的写法作废,落成新规则:

- **可拖 = 六列全部的卡**,不再限定只有待办池/待办两列带 `draggable`。
- **排期动作(待办池⇄待办、列内排序)**:效果不变,直接生效,不改任何状态——跨列发 `ScheduleIssue{id,week_of}`(拖进待办 `week_of=Some(选中周)`,拖回待办池 `week_of=None`);同列发 `ReorderIssue{id,after}`(列内调先后)。
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

### 2.4 发版本三步

周头「发版本」三步(形态留 04 篇/高保真定,本篇只定步骤与数据):

1. **选活**:默认全勾「这周状态是已完成的活」(`week_of=选中周 AND status=Done`,业务活+运作活都算),可取消个别不想计入的。
2. **版本号**:默认 `project.current_version`;从没设过时(新项目第一次)默认 `v0.1`(待拍-04/§3 第 0 站)。可改。
3. **确认**:不直接写仓——按母文档 §2.6「三条不变的规矩」第①条「凡写仓都是活,写仓一律走分支+MR」,buddy 建一张轻量活「发版本 vX」(不起 agent 会话,和「编辑项目信息」同一种轻量活+MR 模式)、开分支、把这次选的活/版本号/说明写成 `docs/releases.md` 的一行提交上去、开 MR。这张活停在评审中,出现在待人处理里。「用 agent 起草说明」是确认前的可选动作——起一次读了 `docs/plan/YYYY-Www.md` 和合入记录的简短会话生成草稿给人改,不选就人自己写这行说明;这次会话只影响说明文字的来源,不改变这张轻量活本身不带 agent 会话这件事。

**真正落库、打 tag 在合入那一刻,不在确认这一步**:有权限的人对这张「发版本 vX」活点「合入并完成」(01 篇已定义的 `MergeAndComplete`,内部仍是 `MergeIssuePr` 接 `TransitionIssue{Done}` 两步,同一件活绝不重复记账)——**这一下**才真正在库里写 `release` 表一行(`origin='人发'`)、可选打同名 git tag,和这张活自己的合入+完成记账同一时刻发生。这是母文档 §2.6 一处自相矛盾(规矩①「凡写仓都是活」vs 早先「发版本」表格行写的「纯 UI」)被按规矩①改正后的口径,本篇不再有张力需要留进开放问题。

### 2.5 预览未合入

**能不能点**:当前选中周存在至少一张 `kind='ops'` 的活处于 `InReview` 且 MR 未合入才能点开——覆盖运作活①(最常见)和③的「合并调整」/「历史回填」两步(它们的产出也动 `docs/plan/history.md`/`PROJECT.md` 草稿,待拍-21 原话「运作活的 MR 合入前」没限定死在①,本篇按更宽口径设计)。没有这样的活时开关灰,悬浮提示原因。

**打开后**:从该活 worktree(`<主工作区>-issue-<n>` 约定,`bw-engine::workspace::provision_issue_worktree`)读 `.bw/metrics.toml` 和这周的 `docs/plan/`,用同一套解析代码重渲染周头——纯粹换个目录读文件,顶部横幅「预览 · 未合入」。**观测数据不预览**——`observation` 只追加、和 worktree 无关,指标卡「现值」保持读库真实值,只有直接对应仓文件的部分(周目标、活清单)随预览切换。**关闭**立刻恢复正常来源,预览本身没写过任何东西,没有副作用。

### 2.6 右侧详情

| 字段 | 来源 | 可改吗 → 走哪条命令 |
|---|---|---|
| 标题、说明 | `issue.title`/`desc` | 可改(沿用既有编辑路径)|
| 类别 | `issue.stage` | 一般创建时定,改动影响默认映射,留 04 篇决定要不要开放改 |
| 开工工具 | `issue.tool` | 可改 → 04 篇范围的命令,本篇只留入口 |
| workflow/加挂技能 | `issue.workflow` | 可改 → `SetIssueWorkflow{id,workflow}`(新命令,§3)|
| 推动指标 | `issue_metric` 关联表(02 篇 §2.2,`issue_id`+`metric_id`;设计期统一:关联表方案,不用 `metric_ids` JSON 列)| 可改(多选已定义的引领/滞后指标;命令留 02/04 篇范围)|
| 周 | `issue.week_of` | 可改 → `ScheduleIssue{id,week_of}`——和拖拽同一条命令,详情面板给个下拉,不强迫必须靠拖拽(可达性,也让「全部」视角选中的卡能改周)|
| 版本 | `issue.version` | 由所属周/发版决定,本篇不在这里开放改 |
| 来源 | `issue.origin` | 只读,历史事实不允许事后改 |
| 远端 issue/MR | `github_number`/`pr_number` | 只读 |
| 运行记录 | `workflow_run` 按 `issue_id` 查(05 篇已用同一张表)| 只读,时间倒序 |
| 产物 | 产物登记表按 `issue_id` 查 | 只读 |

**第五轮拖拽统一后,详情面板是按钮语义(§2.2)唯一常驻的落脚点**——▶开工/■停止/⬇合入/✓点完成/⛔阻塞这一整套按钮原样摆在这里,和拖拽+确认弹窗(§2.3)是同一批命令的两条触发路径,不想拖、或在「全部」视角选中一张卡时都能用。底部另有「去会话 →」(未开工先发 `RunIssue` 再跳,已开工就是跳转+聚焦)、「蒸馏」复用 05 篇已定的 `DistillSkillFromIssue`。重开一张 `Done` 的活放在详情面板、要一次确认,不放卡片一键——密集卡片容易手滑。

### 2.7 模块边界

`screens/plan/`(01 篇目录树已预留)**只做布局与状态**——周列表/看板/周头/详情面板几个 Dioxus 组件、从 `Command`/`Event` 拼出来的 ViewModel;「这周有没有 `docs/plan/` 文件」这类推导、`week_of` 的日期算法都下沉到内核,UI 只消费结果。

命令/事件(只列名字+一句话,与 01 篇对齐;标「较 01 篇改动」的见 §3):

| 命令/事件 | 一句话 |
|---|---|
| `ScheduleIssue` | 把活排进(或移出)某一周;跨列拖拽/右键菜单/详情面板改周都发它 |
| `ReorderIssue` | 待办池/待办列内调先后,不碰状态 |
| `CreateIssue`(含默认映射填充)| 「新建活」按钮,创建时按类别自动填工具/workflow 默认值 |
| `StartWeekPlanning` | 「开始本周」:建运作活①并跳会话屏 ▶开工 |
| `CutRelease` | 发版三步的第三步:建轻量活「发版本 vX」+分支提交 `docs/releases.md` 一行+MR;`release` 表一行与可选 tag 留到合入时才写(见 §2.4)|
| `SetCurrentVersion` | 切在研版本,纯本机动作,不建活 |
| `TogglePreview` | 开/关「预览·未合入」,换一次读的来源 |

拖到状态列(进行中/评审中/已完成/阻塞)不新增命令——确认框确认后发的是既有 `RunIssue`/推评审命令(05 篇)/`TransitionIssue`/`BlockIssue`,和详情面板按钮走同一条路径(§2.3/§2.6),此处不重复列出。

## 3 · 工程对照

**crate/目录**:`crates/app-shell/src/screens/plan/`(01 篇目录树已画出位置),内核侧不新开 crate,沿用 `bw-app::command`/`bw-store::sqlite`/`bw-core::model`。

**`issue` 表增量**:

```sql
week_of TEXT, version TEXT, kind TEXT NOT NULL DEFAULT 'business',
origin TEXT NOT NULL DEFAULT 'human', tool TEXT, workflow TEXT,
sort_order REAL
-- week_of: ISO 周文本 "2026-W34",NULL=待办池 · kind: business|ops
-- origin: human|agent_split|auto|backfill · tool: claude_cli|cursor|open_design
-- workflow: 实际用的 workflow/技能名,记账用 · sort_order: 待办池/待办列内排序
-- 推动指标:issue_metric 关联表(02 篇 §2.2),不在 issue 表上存 metric_ids(设计期统一:关联表方案)
```

同一批改动要在 `sqlite.rs` 加对应七行 `add_column_if_missing`(存量库不会自动加列,CLAUDE.md 核心纪律第 5 条)。`list_issues`(`sqlite.rs:2716-2747`)的 `SELECT` 加这七列,`ORDER BY` 从写死的 `number ASC` 改成「先 `sort_order`(非空时)再 `number` 兜底」;`Issue` 结构体(`model.rs:1694-`)与 `ui` crate 的看板 VM 按需加。memory 里有真实踩过的坑(`project_id` 进了 schema 但读侧全链路没接上)——schema → 领域结构体 → SELECT → VM 四处要一起改。

**新表**:

```sql
CREATE TABLE IF NOT EXISTS week_plan (
    id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(id),
    week_of TEXT NOT NULL, goal TEXT NOT NULL DEFAULT '', file_path TEXT NOT NULL,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_week_plan_project_week ON week_plan(project_id, week_of);

CREATE TABLE IF NOT EXISTS release (
    id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(id),
    version TEXT NOT NULL, released_at INTEGER NOT NULL, note TEXT NOT NULL DEFAULT '',
    origin TEXT NOT NULL DEFAULT 'human', created_at INTEGER NOT NULL
);
-- 包含的活:release_issue 关联表(02 篇 §2.5,release_id+issue_id),不在 release
-- 行上存 issue_ids JSON(设计期统一:关联表方案,与 issue_metric 同一理由)。
-- 时间列:released_at INTEGER(unix 秒),不是 date TEXT(设计期统一,与 02 篇一致)。
```

`project` 表加 `current_version TEXT`(在研版本)与 `standard_version TEXT`(02 篇范围,不重复)。

**命令签名(较 01 篇改动,理由写在这里)**:

```rust
// 较 01 篇改动:合并 01 篇的 ScheduleIssue + UnscheduleIssue 为一条——
// 排进/移出是同一字段的写入,拆两条违反最小化原则。01 篇的 Unschedule
// 退场。
ScheduleIssue { id: IssueId, week_of: Option<String> },  // None = 移出回待办池

// 字段名统一用 after(与预研 §6.2 一致;01 篇早期伪码用 before,语义
// 等价,01 篇据此改名)。
ReorderIssue { id: IssueId, after: Option<IssueId> },    // None = 排到列首

// 较 01 篇改动:语义收窄为「建轻量活+分支提交+MR」,不直接写 release 表——
// release 行改到合入那一刻由 MergeAndComplete/MergeIssuePr 路径写(母文档
// §2.6 规矩①被按「凡写仓都是活」改正后的口径,取代本篇较早版本按「纯 UI」
// 表格行设计的直接写仓方案)。version/note/included_issue_ids 三项随这张
// 轻量活的标题/说明落地,供合入时的写入路径读出来拼 release 行。
CutRelease { version: String, note: String, included_issue_ids: Vec<IssueId> },

// 较 01 篇改动:01 篇早期叫 PreviewIssueWorktree{id}(只有「开」)。
// 本篇需要能开能关,改名 TogglePreview、id 包进 Option,01 篇据此改名。
TogglePreview { id: Option<IssueId> },

// 母文档 §6「切在研版本 = 改 project.current_version,不建活」——
// 01 篇未列,本篇新增,01 篇据此回填。
SetCurrentVersion { version: String },

SetIssueWorkflow { id: IssueId, workflow: String },  // 新增,详情面板改 workflow 用

// CreateIssue「含默认映射填充」不是加字段:bw-app 处理命令时按
// issue.stage 查 04 篇 issue-policy.toml 映射自动填 tool/workflow,UI 不
// 自己传这两个字段(避免和 04 篇映射表出现两份真相)。只新增
// week_of: Option<String>(周头「新建活」传选中周,其它入口传 None)。
```

事件:`IssueScheduled{id}`、`IssueReordered{id}`(新)、`ReleaseCut{version}`(改:现在在 `MergeAndComplete` 合入这张「发版本 vX」轻量活成功那一刻才发出,不是 `CutRelease` 命令本身返回时)、`CurrentVersionChanged{version}`(新)、`PreviewToggled{id: Option<IssueId>}`(新,替代 01 篇只覆盖「开」的旧事件)、`IssueWorkflowChanged{id}`(新)。PTY/终端事件不属于本篇(05 篇范围)。

**`sort_order` 类型**:本篇选浮点数插入排序(新卡片插进两张卡之间取中间值),不是每次拖动重排整列——多数拖动只改一行,代价是精度用尽后需要一次性重新铺号,再平衡算法本篇不展开。预研 §9 开放问题②没选定方案,本篇选定了类型但没展开再平衡算法。

## 4 · 边界与失败

**不做**:甘特图/里程碑实体(待拍-04 已定,版本就是里程碑);拖拽绕过确认弹窗直接改状态(§2.3 确认框是状态动作的唯一入口,拖拽本身不允许跳过它直接发 `TransitionIssue`/`RunIssue`/`BlockIssue`);多版本线(待拍-04);「全部」视角下的跨列拖拽(§2.3 排除在首版范围外,`draggable` 只在选中某周模式下挂,见 §6 开放问题 4)。

**失败如实标注**:
- **远端 issue 建失败**(网络/权限/仓未挂载):本地活照样落库、显示在看板,只带「未同步」小标,`github_number` 留 `0`(既有「创建不破」口径,`model.rs:1699-1706`)——不因远端失败就整张活创建失败,也不假装已同步。
- **预览 worktree 缺文件**:该 worktree 缺 `.bw/metrics.toml`/`docs/plan/`(比如运作活①还没跑到写这一步)时周头对应字段显示灰态「预览:暂无内容」,不报错崩溃、也不悄悄回退去读主干正式文件。
- **拖拽到不合法的状态转移**:不发命令、只弹回原位并提示原因,不静默失败也不报错(§2.3)。
- **状态动作的确认框被取消**:不发任何命令,卡片留在原列原位置,和什么都没发生一样(§2.3)。
- **发版时勾的活其实已不是 `Done`**(并发场景理论可能):`CutRelease` 建轻量活那一刻应校验 `included_issue_ids` 每一张仍是 `Done`,不是就诚实拒绝这次发版、不悄悄摘掉再继续——这个校验只在建活时做一次,`docs/releases.md` 那一行内容随 MR 一起冻结,合入时不重新校验这份名单(合入只负责把已经冻结的内容落成 `release` 表行)。
- **`SetCurrentVersion` 切到没出现过的版本号**:允许(在研版本本是自由文本),只是从此成为新的在研版本。

## 5 · 验收与读回

- **拖拽排期,SQL 读回**:`sqlite3 <db> "SELECT week_of, sort_order FROM issue WHERE id='<uuid>';"` ——拖一张活到某周待办列,读回 `week_of` 变成目标周文本;再做一次列内排序,读回 `sort_order` 顺序符合拖动结果。这条同时验证「杀进程重开顺序一致」:`sort_order` 落库而不是像 `hifi/index.html:1123` 那样只活在前端内存的 `state.kanbanOrder` 里,重启后 `ORDER BY sort_order, number` 读出来的顺序应与关闭前一致;顺带确认左栏周列表顺序、待办池是否仍显示全部未排期活也和重开前一致。

- **状态动作走确认框,SQL 读回**:拖一张 `InReview` 的活到已完成列,确认框点确认后 `sqlite3 <db> "SELECT status, settled_at FROM issue WHERE id='<uuid>';"` 应变成 `Done` 且 `settled_at` 非空;拖一张 `Todo` 的活到进行中列,确认框点确认后应出现一行新的 `workflow_run`(真起了会话,不是假装)。

- **确认框取消,不发命令**:拖一张活到状态列后在确认框点取消,`status` 前后查两次应完全没变,且不产生新的 `workflow_run`/`claude_conversation` 行。

- **`can_transition_to` 守卫读回(非法转移)**:故意拖一张待办池的活到已完成列(模拟误拖),应松手即弹回、不出现确认框,`sqlite3 <db> "SELECT status FROM issue WHERE id='<uuid>';"` 前后各查一次应完全没变;再故意构造一次非法转移(比如给 `Backlog` 的活发 `TransitionIssue{Blocked}`),应被拒绝且 `status` 原地不动。

- **发版本,两段式读回**:确认三步后先读回这张轻量活确实是「建活+MR」而不是直接写仓——评审中、有 PR、没有 agent 会话:
  ```bash
  sqlite3 <db> "SELECT status, pr_number FROM issue WHERE title LIKE '发版本 %' ORDER BY created_at DESC LIMIT 1;"   # 应为 in_review 且 pr_number 非零
  sqlite3 <db> "SELECT count(*) FROM claude_conversation WHERE issue_id=(SELECT id FROM issue WHERE title LIKE '发版本 %' ORDER BY created_at DESC LIMIT 1);"   # 应为 0——轻量活不起会话
  sqlite3 <db> "SELECT count(*) FROM release WHERE project_id='<pid>';"   # 合入前应保持合入前的计数,不多不少
  ```
  「合入并完成」之后再读一次,确认 `release` 表一行和仓文件同一时刻落地:
  ```bash
  sqlite3 <db> "SELECT version, released_at, note FROM release WHERE project_id='<pid>' ORDER BY created_at DESC LIMIT 1;"
  sqlite3 <db> "SELECT issue_id FROM release_issue WHERE release_id=(SELECT id FROM release WHERE project_id='<pid>' ORDER BY created_at DESC LIMIT 1);"
  tail -n 3 <workspace>/docs/releases.md   # 追加的一行确实在仓文件里
  ```

- **深链截图**:`BW_OPEN=<项目名> BW_PANEL=plan` 启动,stderr 打出 `[BW_OPEN]` 即渲染证据;「选中本周」和「全部」两种视角各截一张,点一张卡再截一次详情面板,确认渲染的是真实数据而非占位假数据。

- **macOS 拖拽真机验证**(§2.3 已标注源码分析没能覆盖):真机上做一次「待办池拖到待办」的完整鼠标操作,确认卡片跟手移动、松手后 `week_of` 真的改变——不能只凭源码分析结论判定放行。

## 6 · 开放问题

1. **`sort_order` 的再平衡算法**:本篇选定浮点数插入排序(§3),没展开「精度用尽后怎么批量重新铺号」,留到实现阶段按需处理,还是现在就定一个方案(比如每隔 N 次拖动整列重算)?
2. **「agent 起草发版说明」的底层机制**:这次会话不挂在任何一张 `Issue` 上,但要真的起一次 agent 读文件生成文本——是复用 `RunIssue` 相关机制开一个不落库的临时会话,还是需要一种新的「无活会话」通道?04/05 篇目前都没覆盖这种用法。
3. **`TogglePreview` 的并发场景**:待拍-21 只定了「合入前可以预览」,没定「同一周有两张评审中的运作活同时挂着」时预览开关该切哪一张、要不要给选择器。
4. **「全部」视角下要不要也开放拖拽**:§2.3 排除在首版范围外(「全部」不按周分列,拖拽改哪一周不直观),但用户是否希望至少能把一张待办池的卡直接拖进当前在研的这一周,本篇没有验证过这个需求。

(原第 2 条「发版本是否要走活+MR」已按母文档 §2.6 规矩①改正,不再是开放问题,见 §2.4。)

## 与代码的关系

这篇不改 `crates/`。开工时按 §2/§3 顺序在 `crates/app-shell/src/screens/plan/` 建文件,`issue`/`week_plan`/`release` 三处 schema 增量按 §3 的双守卫模式落地;第 3 节是开工清单,第 5 节是验收清单。01 篇需据此回填 §2.6 命令表的几处改名(`ScheduleIssue`/`UnscheduleIssue` 合并、`PreviewIssueWorktree`→`TogglePreview`、补 `SetCurrentVersion`)和 `main.rs` 的 Windows 拖放配置。
