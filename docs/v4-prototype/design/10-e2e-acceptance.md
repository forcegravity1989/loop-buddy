# 10 · 验收怎么做:总表、指挥器、试点

> **30 秒导读**:01–09 篇各自定义了「怎么证明做对了」,这篇把它们收成一张**总表**——按母文档([`../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md),下称母文档)§8 的八条骨架展开成可执行检查项,并给出 headless 指挥器(不开界面、直接驱动内核走完一整套 V4 主环的脚本)、深链操作手册、内部试点两周怎么走。**验证手段只有三样:指挥器 + `sqlite3` 读回 + 深链 stderr**,不做点击巡航。给接着做 V4 的会话、跑试点的人看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 试点还没跑,§2.4 那两周是计划不是记录。看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

## 0 · 这篇管什么、不管什么

**管**:①**验收总表**——母文档 §8 的 8 条拆成可执行检查项,每项给「动作/读回/预期/出处/铁律」,九篇重复条目去重、按 §8 顺序重排;②**headless 指挥器**——`real_demo` 接班人,mock 走完接入→铺底→开始本周→干一张业务活→验证闭环→发版本→定时→群通知一整圈,每步给读回断言;③**深链与截图**——V4 六入口 `BW_PANEL` 取值、computer-use 已知坑;④老项目、项目群两条新验收的复算命令;⑤试点两周计划;⑥门禁增量建议(不改 CLAUDE.md)。

**不管**:每屏验收细节的原始定义(01-09 篇第 5 节是正本,这篇只归并排序);规范铺底/开工工具/数据模型本身(03/04/02 篇);`standard/` 内容评审(03 篇 §2.7);试点后规范件增删判据(`standard-module-draft.md` §3 已定,§2.4 只引用结论)。

## 1 · 用户看到什么、做什么

**复核设计的用户**:看 §2.1 总表——每行都能对着真实数据库或仓文件核对,不是"相信文档说的"。「V4 做完了没有」= 这张表全部打勾。

**写代码的会话**:每完成一块回 §2.1 找对应行,跑读回命令,把结果如实写进那一刀的 PR 正文(五刀走的就是这条路;§5 提过的 `docs/v4-prototype/e2e/` 证据目录**至今没有建**,要不要建见 §6)。九篇第 5 节仍是正本,这篇只是串起来判断整体做完没有。

**跑试点的同事**:看 §2.4——两周每天做什么、问题记哪、结束时对着母文档 §8 第 1 条核对。

## 2 · 设计

### 2.1 验收总表(母文档 §8 八条展开)

「出处」=判据来自哪篇第 5 节(正本仍在那篇,这里只归并);无标注是本篇新提出的整体检查。「铁律」简称:**人点完成**(完成永远人显式点)、**绝不记两次**(settle-once)、**只追加**、**推导健康**(无数据=灰,不假绿)、**迁移不崩**、**到点真触发**(自动建活绝不自动完成)、**回填不点灯**。**「群不重发」已不是铁律**(改判):`chat_outbox` 去重账本盘点之后取消,群通知发送即完成、不做去重,重发一条是知情代价(母文档 §6.3),§8-8 不再核验"不重发",只核验"三类事件各真发出一条"。

#### §8-1 一个真实项目跑完两个完整周循环

| 检查项 | 读回 | 预期 | 出处 | 铁律 |
|---|---|---|---|---|
| 两份周计划文件在仓里 | `ls <ws>/docs/plan/*.md`,逐个 `cat` 看 front matter,只数 `origin: human` 的(排除 `origin: backfill` 回填周——两者同目录同格式,不是靠文件名区分,02 篇 §2.5)| ≥2 个 `YYYY-Www.md`,含「周目标」「业务活」两节 | 06篇§5 | 只追加 |
| 两轮运作活①在库 | `SELECT week_of,status FROM issue WHERE kind='ops' AND workflow='更新指标与周计划' ORDER BY week_of` | ≥2 行,week_of 不同,均 done 或 in_review | 09篇§5-3 | 人点完成 |
| 两轮运作活②在库 | `SELECT week_of,status,origin FROM issue WHERE kind='ops' AND workflow='asset-audit'` | ≥2 行,origin='auto' | 09篇§5-4/6 | 到点真触发 |
| 至少一次发版记录 | `tail <ws>/docs/releases.md`(看「来源」列=人发的行,这是**唯一正本**)+ `sqlite3 <db> "SELECT DISTINCT version FROM issue WHERE version!='';"` | 仓文件≥1 行「人发」;库里 `issue.version` 出现同一版本号,两者对应一致——**没有独立 `release` 表可查**(02 篇 §2.1/§2.5)| 06篇§5 | 绝不记两次 |

以上读回命令均跑在真实仓上,有对应 `git log` 提交,不是口头声称。

#### §8-2 总览灯不是灰的,数字能对回

| 检查项 | 读回 | 预期 | 出处 |
|---|---|---|---|
| health 大灯非灰、三条理由可核对 | 深链 `BW_PANEL=overview` 截图 + 08篇§2.4 的 (a)(b)(c) SQL,理由文字对照理由模板 | ≥1 项为真灯黄或绿;理由与判据值一致非占位 | 08篇§5 |
| 引领指标卡挂活、重开数字一致 | `cat <ws>/.bw/metrics.toml`(找 `[[leading]]` 段取其 `id`)+ `sqlite3 <db> "SELECT title FROM issue WHERE metric_key='<该leading指标id>';"`(无独立 `issue_metric` 关联表,单列反查即可,02 篇 §2.2);杀进程重开同一深链再截图再查一次 | ≥1 行;灯色/理由/数字不变(health 纯函数现算)| 02/08篇§5 |

#### §8-3 三种开工工具至少各一张活

| 检查项 | 读回 | 预期 | 出处 |
|---|---|---|---|
| Claude CLI 开工,产物进仓+技能整包物化 | `BW_PANEL=session` 点▶开工;`git -C <worktree> log --oneline`;`ls <worktree>/.claude/skills/` | 分支有真实提交;挂的 workflow 整包出现,各带 `.bw-managed` | 05篇§5、04篇§5-2 |
| Open Design 开工一张原型活 | 会话屏中栏「Open Design」标签(探活成功前提下) | WebView 有内容非空白 | 05篇§4 |
| Cursor 开工(装了才测) | 同上,tool='cursor' | 探活成功可开工;失败如实报错不悄悄退回 | 05篇§4、04篇§5-4 |
| **干完的活交得出去(第 4 站 → 第 5 站)** | 会话屏点「提交并开 MR」;`git -C <worktree> log --oneline`;`git ls-remote <远端> bw/issue-<n>`;`sqlite3 <db> "SELECT status,branch,pr_number FROM issue WHERE number=<n>;"` | 分支真被推上远端;`status='in_review'`、`branch='bw/issue-<n>'`、开成了 MR 就有非 0 的 `pr_number`,没开成则 `pr_number` 不变且界面上写着原话 | 05篇§2.4/§3 |
| **没干出东西点提交要弹回** | 建一张活只推到「进行中」、**不**▶开工,直接点「提交并开 MR」 | 如实报"这棵树上没有比主检出多出来的提交";活仍是 `in_progress`,`branch` 空、`pr_number=0`,不留分支不留号 | 05篇§2.4 |
| 终端可复制、右栏文件一致、workflow 用量可现算复核 | 复制核对;截图右栏对照 `git status --porcelain`;跑完前后各查一次 `sqlite3 <db> "SELECT COUNT(*) FROM issue WHERE workflow='<该workflow名>' AND kind='business';"`(02 篇 §2.3 现算查询,没有 `skill_package`/`runs` 战绩表可查)| 均一致;这张活挂了该 workflow 且已计入缓存后,这条 COUNT 比跑之前 +1 | 05篇§2.2/§3、04篇§5-3 |

#### §8-4 第二台 buddy 纳入同一仓

| 检查项 | 读回 | 预期 | 出处 |
|---|---|---|---|
| 两台总览一致、committer 见到评审中的活 | 两台各深链 `BW_PANEL=overview` 截图对比;`BW_PANEL=notify` 截图待处理段 | 项目信息/指标/health 灯完全一致;出现「评审中待合入」标题与 builder 端一致 | 08篇§5、07篇§5-3 |
| 「合入并完成」成功、builder 端同步 | `SELECT settled_at FROM issue WHERE id='<id>'`;builder 重新打开总览/计划屏 | 非空且只一次(幂等短路);显示已完成,不两边各记一次账 | 07篇§5-4、CONTEXT.md settle-once |

#### §8-5 Windows 安装包与 GitHub 仓接入

见 §2.3「Windows 检查清单」,结论性检查项:

| 检查项 | 怎么做 | 预期 |
|---|---|---|
| 装得上不崩、`claude.cmd` 能探测到 | 同事机器装 `BuildersWorkbench-Setup.exe`(`0.4.0-v4`)启动;环境条「测一下」 | 出现项目墙无崩溃;探活绿(V3-use-fix 已验证) |
| 拖拽排期能用、GitHub 仓同一流程接入 | 计划屏拖一张活到待办;两卡填 GitHub 地址 | 跟手且 `week_of` 真改变(前提 `.with_disable_drag_drop_handler(true)` 已加,见 §2.3);铺底运作活③与 codehub 无差异 |

#### §8-6 每站至少一条 E2E 读回记录

不是单独检查,是「§2.1 每行跑完之后留一份读回记录」本身。判据:01-09 每篇≥1 条记录;六入口各≥1 条含 `[BW_OPEN]` 的深链证据。记录今天落在各刀的 PR 正文里,不落成仓内文件(见 §6)。

#### §8-7 老项目历史回填

| 检查项 | 读回 | 预期 | 出处 |
|---|---|---|---|
| 探测正确判定有历史 | `SELECT title FROM issue WHERE kind='ops' AND workflow LIKE '%铺底%'` | 标题含「含历史回填」| 03篇§5-2 |
| 按周数字可复算(贡献者数已取消产出,03 篇 §2.4:没界面读就不产) | 回填的 `docs/plan/YYYY-Www.md`(front matter `origin: backfill`,与本周文件同目录同格式,不是单独的 `history.md`——02 篇 §2.5)里「按周历史统计」表格 对照 `git log --merges`/`--numstat` 重算 | 一致;无标签仓如实写「未发现」| 03篇§5-3/§3.4,命令见样本 |
| 版本时间线对回 tag、远端 issue 数对回远端 | `docs/releases.md` 里「来源」=回填的行(唯一正本,无 `release` 表)对照 `git tag -l --sort=creatordate`;`sqlite3 <db> "SELECT COUNT(*) FROM issue WHERE origin='backfill';"` 对照 `gh issue list --state all \| wc -l` | 一一对应无 tag 则空;数字一致(codehub 侧未在真实环境验证,记「未取到」)| 02篇§5、03篇§5-4 |
| 回填不进量、不参与 health(铁律:回填不点灯)| `sqlite3 <db> "SELECT COUNT(*) FROM issue WHERE origin='backfill' AND week_of='<当前 ISO 周>';"`(回填的历史周不应冒充当前周);对比 02 篇 §2.6 health 三判据(本周有周目标且有真实 git 提交/本周或上周文件有指标读数/上周有合入或发版)——三判据只读当前周与仓最新状态,不读 `origin='backfill'` 的历史周文件(没有 `workflow_credit` 战绩表可查) | 计数 `0`;三判据均不受 backfill 行影响 | 02/03/08/09篇§5 |

样本:buddy 自己的仓(03篇§5 已用过);试点时另找≥1 个真内部老仓重复验证,证明逻辑不是只对 buddy 仓调好的。

#### §8-8 项目群通知(WeLink 到位后验)

| 检查项 | 读回 | 预期 | 出处 | C 刀实况 |
|---|---|---|---|---|
| 三类事件各发一条(mock)| 假群每发一条往 stderr 打一行 `[BW_CHAT_SENT]`(不进库不进仓);同一事件重复触发一次 | 三类各触发一次都成功;重复触发**允许再发一条**(不做去重,这是知情代价不是 bug)| 07篇§5-1、02篇§2.4 | **重复这条验过**:指挥器步骤 10 对同一张活连发两次「评审中」,stderr 上真出现两行。三类各一条:合入与发版的触发点已经接上(`MergeAndSettle` / `CutRelease` 末尾),演示项目没挂远端所以只走到了「评审中」这一类 |
| 运作活①引用群摘要、`none` 提供方不崩 | 预置 mock 历史消息触发「开始本周」看 transcript;`provider='none'` 触发合入/运作活① | 出现摘要关键词按天分组;`none` 下发送被跳过、流程正常走完不报错 | 07篇§5-2/§5-5 | **群摘要没做**(`FetchChatDigest` 这条命令没建,见 LEFTOVERS)。`none` 这条**验过**:演示项目全程没配 `[chat]` 段,九个步骤照常走完,stderr 一行 `[BW_CHAT_SENT]` 都没有 |
| WeLink 真实环境(**待同事**)| 真实凭证重复上面四条 | 与 mock 一致;不作常绿验收 | 07篇§2.9 | 没做,也不打算在试点前做 |

#### 回填(§8-7)C 刀这一轮实际验到哪

回填这条**验过一轮**,样本就是 buddy 自己的仓(`git clone --local` 出来的演示项目):
生成 10 份 `origin: backfill` 的历史周文件 + 1 份人写的本周文件 = `docs/plan/` 下 11 份,与 `ls | wc -l` 一致。
**没验到的两条,如实记下**:①「探测正确判定有历史」——铺底活的标题里没有「含历史回填」字样,
今天回填是一条独立命令(`BackfillHistory`),不是铺底活标题的一部分;②远端 issue 计数——演示项目
没有远端,`origin='backfill'` 的 issue 行数恒为 0(这一条本身也是"不该有"的正确结果,但不构成对远端
同步逻辑的验证)。

### 2.2 headless 指挥器:`real_demo_v4`

`real_demo` 走 V3 五阶段主环,不覆盖接入两卡、铺底三步、周计划、运作活②③、发版本、项目群。这篇给答案:**新建 `crates/bw-app/examples/real_demo_v4.rs`**,不加分支(主环形状与 V3 不同)、也不只覆盖运作活(09篇§5 建议的 `ops_loop_demo` 太窄)。共存期两个指挥器并存:`real_demo` 服务旧壳,`real_demo_v4` 服务新壳与这篇总表;旧壳按01篇§2.11 被删那次,`real_demo` 一并退役(见§6)。

```bash
cargo run -p bw-app --example real_demo_v4 -- <db-path> <workspaces-root> [--project <slug>]
```

**覆盖的步骤**(触发方式、读回断言、幂等键):

| # | 步骤 | 怎么做 | 读回断言 | 幂等键 |
|---|---|---|---|---|
| 1 | 接入项目 | `CreateProject`(不配远端,避免依赖网关)+ 两卡四字段 | `SELECT name,north_star FROM project WHERE id='<pid>'` 非空 | 按项目名查重复 |
| 2(含2a合并调整/2b历史回填)| 规范铺底③ | `RunStandardBootstrap`;工作区用 buddy 仓浅拷贝(见下);mock 执行器分别跑两个子技能,**2b 的 git 本地采集是真代码**不受 mock 影响 | `git log` 有提交;`.bw/managed.toml` 出现指纹;`AGENTS.md` 命中约定关键词;`history.md` 数字与直接跑 git 命令一致(见§8-7)| `.bw/standard.toml` 已存在则跳过;2b 每次重跑整段覆盖不追加重复段落 |
| 3-4 | 开始本周①、确认建活 | `StartWeekPlanning`(mock 代替真实对话,产出固定草稿标【mock】)+ 指挥器代人确认(明写"脚本代人确认")| `test -f docs/plan/<周>.md` 且 front matter `week=<周>`(**唯一正本,不查库索引**——02 篇 §2.5 已取消 `week_plan` 表);`issue WHERE week_of='<周>' AND origin='agent_split'` 有行 | "当前周无文件"天然幂等,重跑返回 `WeekPlanAlreadyExists`;建活按标题幂等 |
| 5-6 | 一张业务活▶开工、推评审中、完成 | `RunIssue`(未配工作区,天然落 MockInteractiveExecutor,标【mock】);未配远端不会真开 PR,指挥器代人推 InReview 再推 Done(同 `real_demo` 步骤④模式,明写),即既有"无PR→人点确认完成(人裁)"路径 | `issue.status`;`settled_at` 非空且只一次 | 按状态判断是否重跑,天然幂等 |
| 7 | 发版本 | `CutRelease`(选步骤6完成的活,版本号取 current_version 或首次 v0.1)| 代人推完成后 `tail <ws>/docs/releases.md` 新增一行(**唯一正本,库里无 `release` 表可查**——02 篇 §2.1/§2.5)| 按版本号幂等 |
| 8-9 | 定时触发运作活②、项目群 mock outbox | 手动调 `tick_scheduler`(时钟设到 `ops2_schedule` 之后);`chat_provider='mock'`,步骤6/7各触发一次 `SyncNotifyToChat` | `sqlite3 <db> "SELECT status,origin FROM issue WHERE workflow='asset-audit';"`;mock `chat_group` 适配器调用记录(**不进库**——02 篇 §2.4 已取消 `chat_outbox` 表,指挥器自己打印/断言调用次数)| `origin='auto'` 且 `status` 非 Todo(证明真自动开工);`SyncNotifyToChat` 被调用两次(步骤6/7各一次),重复触发不做去重比对(02 篇 §2.4,重发是已知代价)|
| 10 | evidence 导出 | 全部真实读回写一份 JSON,不手写数字 | `cat evidence-v4-<slug>.json` | 每次重跑覆盖同名文件 |

**四条取舍**:①工作区用 buddy 仓浅拷贝(`git clone --local` 到临时目录)而非空仓——2b 要读真实 git 历史验证"防伪规则",数字可对照样本。②不真开 MR(CLAUDE.md 纪律3)——"开MR才能进评审中"的步骤退化成既有"无PR→人点确认完成(人裁)"路径并代人推进明写;真远端完整合入链路只手动验证一次,不进指挥器(见§4)。③重复跑不产生重复数据——项目按名字、Issue 按标题、周计划按文件是否已存在(`test -f docs/plan/<周>.md`)、发版按版本号(`docs/releases.md` 有没有这行)判断是否跳过;盘点之后 `workflow_credit`/`chat_outbox` 表已取消,没有数据库唯一约束可以兜底——`SyncNotifyToChat` 重复触发允许真的重发一条(02 篇 §2.4 知情代价),幂等只对项目/活/周计划/发版这几类文件与缓存成立,同 `real_demo` 幂等纪律。④evidence JSON 不手写数字,是§2.1 多条读回的现成来源。

### 2.3 深链与截图

**六入口 `BW_PANEL` 映射**(与01篇§2.7对齐):`overview`(总览)/`plan`(计划)/`session`(会话,配 `BW_SEL=issue:<uuid>`)/`notify`(通知)/`config`(配置)/`kb`(知识库),命令统一形如 `BW_DB=<db> BW_OPEN=<项目> BW_PANEL=<屏> ./target/debug/bw-v4-dev`。顶层三屏不依赖 `BW_OPEN`:`BW_VIEW=onboard`/`settings`;项目墙是默认视图。`bw-v4-dev` 是共存期新壳 bin 名(01篇§2.1/§3.1),删旧壳那次改回 `builders-workbench`。每条命令跑完 stderr 出现 `[BW_OPEN]` 即渲染证明,机制不变(01篇§2.7)。

**computer-use 怎么摸**:沿用 CLAUDE.md 已踩坑——`~/Applications/BWDev.app` 长期稳定验证壳,`screenshot` 真实可用、`click`/`key` 永久受阻(Dioxus/wry 窗口限制,与打包方式无关)。共存期按01篇§2.10 不改 `scripts/point-bwdev-here.sh`,开工时该脚本加可选参数换拷贝源为 `bw-v4-dev`;之前手动 `cargo build -p app-shell && cp target/debug/bw-v4-dev ~/Applications/BWDev.app/Contents/MacOS/bwdev-launcher && codesign --sign - --force --deep ~/Applications/BWDev.app`,再终端直调深链命令(`open -a` 传不进环境变量)。只用 `screenshot` 读渲染,不指望点击——agent 自身 `screencapture` 拿不到真实桌面像素(sandbox 只看壁纸),证据靠 computer-use 自己截图或让用户在自己屏幕核验。

**Windows 检查清单**(§8-5 复述):①安装包版本号 `0.4.0-v4`;②「测一下」确认 Claude CLI 认得 `claude.cmd`(V3-use-fix 已解决);③计划屏拖一张活——wry 默认拖放处理器会让 WebView2 屏蔽页面内拖放,新壳 `main.rs` 必须加 `.with_disable_drag_drop_handler(true)`(06篇§2.3 提出、01篇回填,这里核一遍有没有漏加);④GitHub 仓接入走一遍两卡,与 codehub 无功能差异。

### 2.4 试点两周计划

前提:**§8-1 的主试点项目已定(握手清单 C2)——buddy 自己的仓(GitHub),不连 codehub**(开发环境连不到 codehub 就用自己;codehub 项目的接入留给内部试用时验,不阻塞这两周)。§8-7"至少一个内部老仓"仍需用户在试点前指定(buddy 自己的仓已经是一个有效的历史回填样本,03 篇 §5 已跑过,但母文档 §8-7 要求"另找≥1 个真内部老仓重复验证,证明逻辑不是只对 buddy 仓调好的",这条人选未定,见§6)。

| 周 | 谁做 | 每天做什么 | 出问题记哪 |
|---|---|---|---|
| **第1周** | Builder + 一名 committer(第二台 buddy 纳同仓)| D1 接入两卡→铺底 MR 合入→「开始本周」真跑①→建业务活;D2-4 至少一张活 Claude CLI 真开工、一张 Open Design 真开工,每天看一眼 health 灯有无假绿/异常灰;D5 晚运作活②自动建自动开工,不用人守 | 小问题先记**试点日志**(`docs/v4-prototype/e2e/pilot-log-<项目>.md`,按天流水账);确认是设计缺陷/长期待办才升级进 `docs/LEFTOVERS.md` |
| **第2周** | 同上 + committer 实跑评审→合入 | D1 评审②的 MR、合入、完成,第二次「开始本周」产出第二份计划文件;D2-4 继续业务活,至少一次 committer 用第二台 buddy 真实合入(§8-4),配群的确认收到通知(§8-8,WeLink 到位后);D5 第二次②自动触发,周末走「发版本」三步 | 同上;周末收口对照§2.1 逐条打勾,没打上的记原因 |
| **结束判据** | — | §8-1:两份周计划文件、两轮运作活①②、至少一次发版记录都在仓里 | — |
| **试点后** | 规范维护者 | 对照 `standard-module-draft.md` §3——每个规范件"用没用上"记进 `standard/CHANGELOG.md`;没用到的降为扩展或进鱼塘 | `standard/CHANGELOG.md` |

**为什么日志与 `docs/LEFTOVERS.md` 分开**:后者是全产品唯一排期清单,琐碎观察直接写进去会污染用途——先攒日志,收尾筛一遍,需排期的才"消化"进去。

### 2.5 门禁增量(建议,不改 CLAUDE.md)

| 增量 | 内容 | 来源 |
|---|---|---|
| 新守卫脚本×2 | `guard-no-cross-screen-import.sh`(一屏一模块)、`guard-file-lines.sh`(单文件超1500行阻断,只查 `app-shell/src/`)| 01篇§2.3/§2.8 |
| 新 crate 编译检查、wasm32 不变 | `cargo check -p app-shell` 骨架阶段起就应绿,`app-desktop` 保留证明纯增量;`Command`/`Event` 加变体不影响 `bw-core`/`ui` 编译到 wasm32 | 01篇§5-1/§5-5/§2.12 |
| `cargo test` 排除范围 | `app-shell` 与 `app-desktop` 同等排除——UI 行为靠 E2E 不靠单测(CLAUDE.md 纪律6);现存约2,000行内联测试仍须过,挂了顺手修不绕过 | 沿用既有原则 |
| E2E **不进门禁**,进验收 | 依赖真实仓/部分真实 Claude CLI/人工截图,不设 CI required check;`real_demo_v4` 建议合 main 前手跑一次,不做门禁(与 `real_demo` 今天待遇一致)| CLAUDE.md 纪律3精神 |

## 3 · 工程对照

**新增文件**:`crates/bw-app/examples/real_demo_v4.rs`(签名级骨架;与 `real_demo.rs` 并列,非分支非替换):

```rust
// real_demo_v4.rs(新);cargo run -p bw-app --example real_demo_v4 -- <db> <ws-root> [--project <slug>]
type R = Result<(), AppError>; type S = Arc<dyn Store>;
async fn ensure_onboarded(app: &mut App, store: &S, slug: &str) -> ProjectId; // 1
async fn ensure_standard_bootstrap(app: &mut App, p: ProjectId, ws: &Path) -> R; // 2,幂等键 .bw/standard.toml
async fn ensure_week_started(app: &mut App, p: ProjectId) -> Result<IssueId, AppError>; // 3-4,幂等键 week_plan_exists
async fn run_one_business_issue(app: &mut App, issue: IssueId) -> R; // 5-6,代人推 InReview/Done
async fn cut_release(app: &mut App, p: ProjectId, version: &str, issues: Vec<IssueId>) -> R; // 7
async fn fire_ops2(app: &mut App) -> R; // 8,手动调 tick_scheduler
async fn sync_chat_mock(app: &mut App, p: ProjectId) -> R; // 9,provider=mock
async fn export_evidence(store: &S, p: ProjectId, out: &Path); // 10
```

**`docs/v4-prototype/e2e/` 目录(设想,**未建**;§5 给完整格式)**:`README.md` 指向本篇;`pilot-log-<项目slug>.md` 是 §2.4 试点日志;`01-architecture/`…`09-ops-workflows/` 按篇分类一条检查一个文件;`blueprint-s8/<1-8>-<slug>.md` 存§2.1 里无单篇出处的整体检查。

**引用但不重复定义**(均已在01-09篇定义,这篇只是调用方):`RunStandardBootstrap`、`StartWeekPlanning`、`RunIssue`、`TransitionIssue`、`CutRelease`、`MergeAndComplete`、`SyncNotifyToChat`、`CreateAutopilotTask{auto_run}`;`docs/plan/*.md`(周计划正本)、`docs/releases.md`(发版正本)——**这两份是仓文件,不是库表**(`week_plan`/`release`/`chat_outbox`/`workflow_credit` 表盘点之后全部取消,见 02 篇 §2.1/§2.6)。

## 4 · 边界与失败

**不做什么**:不真跑 `claude` 当门禁——`real_demo_v4` 全程 mock,真跑只在试点由人手动做(§2.4)。不依赖网关——指挥器不配真实远端,"开MR"步骤走既有诚实退化路径,WeLink 同理只手动验证一次。不做单元测试大坝——总表全是行为级 E2E,`app-shell` 与 `app-desktop` 同等不要求写 UI 单测。不假装 UI 测试——"界面看到了什么"一律要求深链+`[BW_OPEN]`+截图。不在这篇重新定义任何一篇的判据,有问题回对应那篇改;§8-7/8-8"真实环境"部分如实标注"未验证过"。

**数据库在本机(握手清单 B2/待拍-29,§8-4 验收的前提说明)**:§8-4"第二台 buddy 纳入同一仓"验收的是**仓与远端 issue 的一致性**,不是库同步——库本身是**本机**的(SQLite,只做记账与推导,不是云服务),多人看同一个项目时,每人各自的库只含自己机器上发生过的运作(运行记录、战绩、本机采集/手填的观测值),两台机器的 health 灯可能因此不同(各自数据各自算),这不是 bug;项目仓内文件(`docs/plan/`、`docs/releases.md`、`.bw/*`)才是共享正本,§8-4 的"两台总览一致"验收的是**从同一份仓 + 远端 issue 能重算出一致的名片/指标定义/计划**,不是"两台库的原始数据一模一样"。**MVP 不做远端库、不做库同步**——运作活①把「本周指标读数」段(02 篇模板已加)写进周计划文件随 MR 进仓,是这条铁律下让"别人在仓里也能看到数"的唯一补丁,§2.1「§8-4」那一行的读回口径据此理解,不要误测成"两台库表内容逐行相等"。

**失败如实显示**:`real_demo_v4` 任一步失败都停在那一步、打印真实错误、非零退出码结束,不静默跳过——延续 `real_demo`"如实停在这里,不假装往前"的纪律。

## 5 · 验收与读回

这篇自己的验收 = 两件事:

1. **指挥器一次跑通**:`cargo run -p bw-app --example real_demo_v4 -- <临时db> <临时workspaces-root>` 完整跑完§2.2 十个步骤不中途非预期退出,与 `evidence-v4-*.json` 互相一致;**重跑一次**验证幂等——项目/活/周计划文件/发版记录这几类文件与缓存的计数不因重跑增长(§2.2「四条取舍」③的幂等键判断);`SyncNotifyToChat` 重复触发允许真的重发一条(02 篇 §2.4,没有 `chat_outbox`/`workflow_credit` 去重表兜底,这是知情代价不是 bug),不计入这条幂等断言。

2. **总表每条至少一次读回记录**——§2.1 每行跑完动作后存一份记录。下面是当初设想的仓内格式(目录**未建**,今天记录落在 PR 正文;`docs/v4-prototype/e2e/<篇号或blueprint-s8>/<检查项slug>.md`):

```markdown
# <检查项名字,原样抄自 §2.1「检查项」列>

- 母文档 §8 条目:<1-8>
- 出处:<见 0N 篇 §5-M,或「本篇 §2.1 新增」>
- 日期:<YYYY-MM-DD>
- 动作:<做了什么,一句话或粘贴命令>

## 读回
```
<真实执行的 SQL 或 shell 命令>
```
## 实际输出
```
<原样粘贴的真实输出,不改写不省略关键部分>
```
- 预期:<抄 §2.1「预期」列>
- 结论:通过 / 不通过(说明原因)/ 待办(阻塞原因)
- 截图(可选):`docs/v4-prototype/e2e/screenshots/<文件名>.png`
```

JSON 格式(指挥器 `export_evidence` 或批量脚本产出时用):

```json
{
  "check": "两轮运作活①", "blueprint_item": 1, "source": "09篇§5-3", "date": "2026-08-20",
  "readback_cmd": "sqlite3 <db> \"SELECT week_of,status FROM issue WHERE workflow='更新指标与周计划';\"",
  "actual_output": "2026-W33|done\n2026-W34|in_review",
  "expected": "≥2 行,week_of 不同,均 done 或 in_review", "verdict": "pass"
}
```

两种格式都要求"实际输出"真实粘贴不是转述,对应 CONTEXT.md「读回」词条要避免的反例(拿截图或 agent 自报当读回)。

## 6 · 开放问题(≤5)

1. **`real_demo_v4` 是不是最终名字**:共存期清楚表明"V4 指挥器、real_demo 是 V3 的"。旧壳被删、`real_demo` 一并退役那次,要不要改回 `real_demo`(呼应 `bw-v4-dev`→`builders-workbench` 同一模式)?倾向"改回",但应与旧壳删除同一次改动,不提前做。
2. ~~试点主项目选哪个~~ **已定(握手清单 C2)**:§8-1 的主试点项目 = buddy 自己的仓(GitHub),不连 codehub;codehub 接入留内部试用时验,不用等它才能开始两周试点。
3. **老项目回填验证仓选哪个**:§8-7 要求"至少一个内部老仓",除 buddy 仓外建议再选一个真实内部老项目,人选未定;暂无候选就先只用 buddy 仓过这条,第二仓列进试点后待办。
4. **Windows 试点机器谁提供**:§2.3 需要一台真实 Windows 机器与一位同事跑一遍安装流程,人选未定,建议试点启动时一并确定。
5. **`docs/v4-prototype/e2e/` 到底建不建**:五刀跑完都没有建它——每刀的读回都写在 PR 正文里,够用,也没人回头查过。要不要在试点期改成仓内证据目录,留给人拍;不建的话本篇 §5 那套格式就是废的,该删。原问题的其余部分:用户若想现在用 buddy 仓既有数据(如03篇§5 已跑过的回填样例)补几条,也可以现在建目录。

## 与代码的关系

这篇不改 `crates/`;`docs/v4-prototype/e2e/` 目录至今**没有建**(见 §6 第 5 条)。开工顺序:01-09 篇实现完成后回§2.1 找对应行、跑读回、按§5 存证据;`real_demo_v4.rs` 按§3 骨架实现,建议在04-09篇功能大致落地后再写,写完后§2.2 表格就是实现清单与验收清单合一。
