# 01 · 新壳怎么搭

> **30 秒导读**:这篇回答一个问题——**V4 的界面壳落在哪个 crate、目录怎么分、模块怎么防止互相纠缠、命令 / 事件总线长什么样、旧壳什么时候能删**。不讲某一屏具体长什么样(那是 02–11 篇的事)。给接着做 V4 的会话看。**现在还作数吗**:作数,而且已经落地——V4 的内核 `crates/bw-v4` 与新壳 `crates/app-shell` 都在 `main` 上,第 3 节「工程对照」写的是真代码的结构。还没做完的部分只认 [`../../LEFTOVERS.md`](../../LEFTOVERS.md) 的 V4A–V4E 五组。 看不懂的词查 [`../../../CONTEXT.md`](../../../CONTEXT.md);代号查 [`../../code-schemes.md`](../../code-schemes.md)。

---

## 0 · 这篇管什么、不管什么

**管**:对应母文档 [`../../mvp-blueprint-draft.md`](../mvp-blueprint-draft.md) §7「建法」、§5「信息架构」,待拍-11(借模式、模块管控)、17(新壳+旧内核)、18(减负线先合)、19(高保真=可点击 HTML)、24(开工工具三列、不维护 agent 名单)、26(项目群工厂)。具体是:新壳是新开 crate 还是塞进 `app-desktop`、叫什么、目录长什么样;「一屏一模块」「一个外部能力一个适配模块」靠什么机制钉住(不是靠自觉);`bw-app` 的 `Command`/`Event` 要新增哪些名字(只列名字+一句话);深链环境变量在六入口下怎么改;文件行数守卫;新壳与 `standard/` 的接缝(只管放哪/怎么进二进制/版本号从哪读);旧壳共存与删除判据。

**不管**:每屏具体交互与视觉规格(05-09 篇)、`.bw/*.toml` 与仓文件格式(02 篇)、规范铺底流程本身(03 篇)、开工工具怎么注册与 workflow 怎么注入(04 篇)、验收怎么跑(10 篇)。`standard/` 内部结构是 03 篇的事,这篇只钉外壳怎么读它。

---

## 1 · 用户看到什么、做什么

**终端用户**:整个建设期感知不到"背后新旧两壳并存"——`BW_OPEN=<项目名>` 深链照常能打开应用,数据库、仓文件格式不因换壳而变。等六入口全部跑通、旧壳被删的那次更新后,用户会看到界面从"左侧十图标+阶段轴"变成"总览/计划/会话/通知/配置/知识库"六入口,但打开的还是同一批项目、同一份数据库,不需要迁移。

**写代码的会话**:这篇要回答"该在哪个目录建文件、这个文件能不能 `use` 那个文件、命令总线要不要加东西"——每节给可执行的答案,不是原则性的话。

---

## 2 · 设计

### 2.1 V4 新开哪两个 crate

V4 开了**两个** crate。为什么不摞在旧内核上:本机库缩到四张表之后,`bw-store` 的二十张表与 `bw-app` 架在这些表上的用例整片接不上,改它等于把旧库拆了重装,还会连累仍在跑的 V3。

- **`crates/bw-v4`(V4 内核)**:四张表的库 + 仓文件解析 + 现算推导 + 命令/事件总线 + 规范铺底。复用 `bw-core`(五阶段元数据、活的状态机、id)与 `bw-engine`(执行器、工作区、`.bw/metrics.toml` 解析),**不依赖 `bw-store` / `bw-app`**。它是第六个内核 crate,已经加进 `scripts/guard-kernel-ui-free.sh` 的稽查名单(该脚本原本管 `bw-core bw-engine bw-store bw-app ui` 五个)。
- **`crates/app-shell`(V4 新壳)**:九屏 + theme + 桥。跟现有 `app-desktop` 同属"UI 相关、允许依赖 dioxus/wry"一层,不进稽查名单,继续用 `app-` 前缀一眼认出"这是壳"。

用"shell"不用"desktop":这次真正变的是分层方式(六入口+一屏一模块+适配模块化),不是出新平台;叫 `app-v4` 会把版本号焊进一个要长期活下去的名字里,不合"不为向后兼容留旧路径"——旧壳删除后这个 crate 自然是唯一桌面壳,不必再改名。`bw-v4` 这个名字里的版本号同理,删旧内核那次一并改。

**旧 crate 一行没改**:`bw-core` / `bw-engine` / `bw-store` / `bw-app` / `ui` / `app-desktop` 六个包在 A 刀里零改动(只有测试辅助函数为修门禁偶发红改过),`cargo check -p app-desktop` 一直是绿的。

**两个 package,不用同 crate 内 feature 切换**:待拍-17 已定"旧屏幕一律不搬",物理分开比拉 feature gate 更彻底——新旧代码不出现在同一份 `Cargo.toml`/依赖树里,删旧壳时只需在 `members` 去掉一行。`crates/app-shell` 加进根 `Cargo.toml` 的 `members`,**不**加进 `default-members`(和 `app-desktop` 今天待遇一样),日常 `cargo check`/`cargo test` 不因多一个 UI crate 变慢。

**bin 名字冲突**:`app-desktop` 的 `[[bin]] name = "builders-workbench"`(`crates/app-desktop/Cargo.toml:9-11`)。Cargo 不允许同一 workspace 里两个包的 bin 同名("output filename collision"),共存期 `app-shell` 的 bin 建议叫 `bw-v4-dev`,删旧壳那次改动里改回 `builders-workbench`。

### 2.2 目录树

```
crates/app-shell/
├── Cargo.toml
├── src/
│   ├── main.rs              # 入口:起 wry 窗口、起内核桥线程、读深链环境变量
│   ├── bridge/               # 内核桥(独立 tokio 线程,沿用 app-desktop/src/kernel.rs 的做法)
│   ├── theme/                 # 从 V3 抄来的 token 与原子样式函数(2.5 节)
│   ├── screens/                # 一屏一目录,九个:三顶层 + 六项目内入口
│   │   ├── mod.rs              # 唯一知道"有哪些屏"的地方(路由/深链分发)
│   │   ├── wall/                # 顶层·项目墙
│   │   ├── onboard/             # 顶层·接入项目两卡
│   │   ├── settings/            # 顶层·设置(齿轮)
│   │   ├── overview/            # 项目内·总览
│   │   ├── plan/                 # 项目内·计划(周视角 + 六列看板)
│   │   ├── session/              # 项目内·会话(Orca 式三栏)
│   │   ├── notify/               # 项目内·通知
│   │   ├── config/               # 项目内·配置
│   │   └── kb/                   # 项目内·知识库
│   └── adapters/                # 一个外部能力一个适配模块(2.4 节)
│       ├── terminal_xterm/       # 嵌入终端(xterm.js + PTY)
│       ├── claude_cli/            # Claude CLI 开工工具
│       ├── cursor/                # Cursor 开工工具
│       ├── open_design/           # Open Design(本机探活+WebView 内嵌)
│       ├── codegraph/             # codegraph 子进程调用
│       └── chat_group/            # 项目群工厂(发消息/拉历史)
└── public/                        # 沿用 app-desktop/public 的 xterm.js/css 等静态资源
```

**为什么九个屏幕目录不是六个**:项目内六入口是给已纳入项目的用户看的;项目墙、接入项目两卡、设置齿轮是不在任何项目里时的三个顶层视图,今天分别是 `app-desktop` 的 `screens/wall.rs`、`create.rs`、`settings_hub.rs` 三个独立文件——新壳延续"顶层三屏+项目内六屏"共九屏,一屏一目录。

### 2.3 「一屏一模块」怎么钉死

待拍-11 原话:"每个借鉴/缝合的外部能力是一个独立模块,以后持续借鉴也不会散弹修改"。落到工程手段,三层:

1. **每屏只有一个公开出口**:`screens/<name>/mod.rs` 只 `pub` 一个函数(签名见 §3),接收这一屏的 ViewModel + 往内核发命令的回调,返回 Dioxus `Element`。屏内部随便拆文件,但除这一个函数外目录下不再有第二个能被外部引用的 `pub` 项。
2. **共享数据只能经一个共用的 ViewModel 模块**:`plan` 屏和 `overview` 屏都要展示"活的推动指标"时,这个 DTO 类型定义在共用模块里,两屏各自 `use crate::vm::...`,**永远不允许** `plan` 里出现 `use crate::screens::overview::...`——每层只能向下依赖,不能跨并列层互相 reach into。

   **这个共用模块不是 `crates/ui`**:`ui` 是 V3 的 crate,里面的 ViewModel 全部架在旧库的二十张表上,V4 一条也用不上,而 V4 说好了不动旧 crate。V4 的 ViewModel 因此就近放在新壳自己的 `crates/app-shell/src/vm.rs`,`crates/ui` 一行没改。等旧壳删掉、`ui` 跟着退场,这块要不要单独成 crate 再说 —— 现在多一个 crate 只是多一层没人用的间接。
3. **guard 脚本兜底**(§3 给完整脚本):新增 `scripts/guard-no-cross-screen-import.sh`,对 `crates/app-shell/src/screens/*/` 逐目录 grep `crate::screens::`,命中除自己以外的屏幕名就报错。和 `guard-kernel-ui-free.sh` 并列进门禁清单(建议,不改 CLAUDE.md,由用户后续拍板)。

### 2.4 「一个外部能力一个适配模块」

`adapters/` 每个目录对应一个外部能力,目录里必须有 `README.md`,固定三段:**借自哪个项目/文件**、**借了什么**、**没借什么**(待拍-11 要求的记录形式,预研已按这个粒度做过,结论原样搬进各自 README):

| 模块 | 借什么(已核实的判断,不是代码) | 出处 |
|---|---|---|
| `terminal_xterm` | xterm 6 原生选区 API + 选中即复制;暗色终端条只用这一块 | [orca.md](../research/orca.md) §2(a);v3-ui-reference.md §4.9 |
| `claude_cli` | agent 状态判定改「CLI 官方 hooks/statusLine 主动上报」,不猜终端输出(BW 已有 hook 回收,对齐即可,不抄 Orca 的多 agent 归一化层) | orca.md §2(d)、§5(B) |
| `cursor` | `docs/v3-prototype/cursor-agent-executor.md` 设计稿(未落地,V4 落) | 待拍-09 |
| `open_design` | 沿用 `app-desktop/src/open_design.rs` 的"本机探活+WebView 内嵌 URL",已核实 Open Design 与 DSH 的关系不影响这条接法 | [deepseek-harness.md](../../archive/v4-prototype/research/deepseek-harness.md) §3、§5(B) |
| `codegraph` | 右侧代码结构侧栏首版只文件树+diff(待拍-22),`codegraph` 子进程调用留增量口子,不做符号级大纲 | 母文档「codegraph 怎么用」段;orca.md §3 |
| `chat_group` | 工厂模式,核心两个函数——发一条消息到群 / 拉一段时间群历史;提供方今天两位:内部 WeLink(同事实现)、外部待定(先放「未配置」) | 待拍-26 |

**开工工具的声明式清单**(呼应 deepseek-harness.md §5(C) 借鉴的三条判断——声明式工具清单、能力显式依赖、安装期权限对人可见):`claude_cli`/`cursor`/`open_design` 各自声明接法类型(终端类=PTY / 本机网页内嵌类=探活拿 URL 后嵌 WebView)与需要什么(工作区路径/网络/本机已装某二进制)。机读正本住项目仓的 `.bw/issue-policy.toml`(03 篇「规范大类 5」),新壳这边只是"声明的实现",新增开工工具只需 `adapters/` 加一个目录 + `issue-policy.toml` 加一行,不碰 `bw-app` 内核。

### 2.5 从 V3 抄什么(不推倒视觉与好组件)

待拍-17/23 已定不推倒 V3 视觉(「待拍-NN」= 母文档 §11 决策台账里的编号)。设计期曾把 V3 的结构与样式逐个组件抄成一份参照文档,新壳建成之后那份抄录已经作废——**今天的视觉正本是高保真原型 [`../hifi/index.html`](../hifi/index.html) 与从它整体搬过来的 `crates/app-shell/assets/hifi.css`**,要看 V3 原样就直接读 `crates/app-desktop/` 的源码。当初列的、确实抄过来的清单:

1. **Token 与原子样式函数**(`theme::chip/card/dot/btn_primary/input/label`,来源 `crates/app-desktop/src/theme.rs`):暖纸底色 `#EFEBE2`、clay 主色 `#C5654A`、四级灰阶、三态信号色+Unknown 灰,直接照抄,不另起一套。
2. **项目墙卡片**`ProjectCard` + 本机环境条 `LocalEnvBar` + 健康概览条 `HealthOverviewBar`(均 `wall.rs`)——进 `screens/wall/`。
3. **指标卡两形态**`MetricCard`/`BizMetricCard` 与「已停用」折叠区(均 `op.rs`)——进 `screens/overview/`。
4. **Issue 列表行与六列看板卡片**(`op.rs` 的 `IssuesPanel`,尤其按钮语义分层——同一按钮位置按状态互斥切换文案颜色,不堆砌常驻按钮)——进 `screens/plan/`。
5. **嵌入终端面板** `TerminalWidget`(含暗色标题条与离屏保活写法)——进 `terminal_xterm` 适配模块,被 `screens/session/` 用。
6. **Hub 列表行**(SkillHub/AgentHub 卡片网格与 `SkillFileBrowser` 双栏文件浏览器)——拆进 `screens/config/`(workflow/skill 清单区)与 `screens/kb/`(资产页签)。**这两个清单区不再有库表可查**——盘点之后 `skill`/`skill_package` 等表全部取消,清单现扫 `.claude/skills/**/SKILL.md` 目录得到(02 篇 §2.6),"哪怕只有一个文件也套文件树外壳"这个模板决定原样保留。
7. **创建流两卡**(`create.rs` 全部,含 `RemoteProjectProbe` 探活三态)——进 `screens/onboard/`,待拍-01 的四字段意图卡在这基础上改字段,不改骨架。
8. **`ActionsBanner` 后台动作条**——三态+阈值门槛(秒级完成不显示),新壳任何后台命令(建 MR、拉群历史、跑历史回填)都复用,建议放 `theme/` 或独立小模块共享,不是 `onboard` 专属。

**明确不抄**:阶段轴 `StageAxis`(V4 总览是一列横块不分阶段视角)、`chrome.rs` 的十图标 `IconRail`(V4 顶层是六入口)、`project_rail.rs` 的项目内左栏组件列表(职责被 `config`/`kb` 吸收)。

### 2.6 命令 / 事件总线增量

`Command` / `Event` 是 `crates/bw-v4/src/command.rs` 里**全新的一对枚举**,和 `bw-app` 的那对没有继承关系、不互相引用(理由同 §2.1)。通路本身不变:UI 只发 `Command`、只收 `Event`,`bw-v4` 是唯一执行用例的地方(`crates/bw-v4/src/app/`)。

下表是**设计目标全表**,不是落地清单——哪些还没接,见表后一段与 `docs/LEFTOVERS.md`。实现细节留 02、04–09 篇:

| 屏 | 命令/事件 | 一句话 | 标注 |
|---|---|---|---|
| 计划 | `ScheduleIssue{id, week_of: Option<String>}` | 排进(或移出)本周待办(设/清空 `week_of`),待拍-25「待办池⇄待办」的排进方向(设计期统一:06 篇把早期分开的 `ScheduleIssue`+`UnscheduleIssue` 合并成一条,本表据此回填) | 新 |
| 计划 | `ReorderIssue` | 待办池/待办列内排先后,纯展示,不动状态机 | 新 |
| 计划 | `CutRelease` | 「发版本」:选本周完成的活→填版本号→确认——建一张轻量活「发版本 vX」+分支提交 `docs/releases.md` 一行+MR(待拍-12;06 篇 §2.4 已定:`docs/releases.md` 新增一行与可选 tag 落在这张活「合入并完成」的那一刻,不在 `CutRelease` 命令本身返回时;`docs/releases.md` 是发版记录**唯一正本**,不建 `release` 表存副本——02 篇 §2.1/§2.5) | 新 |
| 计划/总览 | `TogglePreview{id: Option<IssueId>}` | 合入前预览某活 worktree 里的 `.bw/metrics.toml`/`docs/plan/`(待拍-21,形态留 06 篇;设计期统一:06 篇把早期只能「开」的 `PreviewIssueWorktree` 改成能开能关的 `TogglePreview`,本表据此改名) | 新 |
| 计划 | `SetCurrentVersion{version}` | 切在研版本,纯本机动作,不建活(06 篇新增,本表据此回填) | 新 |
| 计划/总览 | `IssueScheduled` / `ReleaseCut`(Event) | 排期/发版真实发生的回执 | 新 |
| 总览 | `SetProjectChat` | 编辑「项目群」配置,写 `.bw/project.toml` 的 `[chat]` 段 | 新 |
| 总览 | `EditProjectCard{brief, benchmark, north_star, chat: Option<ChatConfig>}` | 名片「编辑→保存」:建轻量活+分支+MR(母文档 §2.6 用户四问第 4 条),字段按 02/08 两篇一致的名片字段改写(设计期统一,去掉母文档待拍-01 已删的「类型」字段与早期 `descr`) | 新(复用既有 `UpdateProjectIdentity` 字段写入,套一层「建活+MR」外壳)|
| 总览 | `ProjectChatChanged`(Event) | 项目群配置真实改了 | 新 |
| 总览 | `ProjectCardEditPending`(Event) | 名片编辑的轻量活已建、MR 已开,总览横幅可以显示了(08 篇新增,本表据此回填) | 新 |
| 总览 | `ProjectCardMerged`(Event) | 名片 MR 已合入,库缓存已同步,总览可以刷新显示新值了(08 篇新增,本表据此回填) | 新 |
| 通知 | `MergeAndComplete` | 一键做完「合入」+「完成」——**内部仍两步**:先 `MergeIssuePr` 再走既有 `TransitionIssue` InReview→Done 记账路径,同一件活绝不记两次不因按钮合并而改变 | 新(组合既有命令,不新开记账路径)|
| 通知 | `SyncNotifyToChat{issue_id, event_type}` | 评审中/已合入/发版且配了群时,直接调用 `chat_group` 适配器发送——**不写去重账本**,发送即完成(信息住哪那次盘点:`chat_outbox` 表取消,"没人取的不存";重发一条能忍,是知情代价而非 bug,见 02 篇 §2.4;字段名以 07 篇为准) | 新 |
| 通知 | `FetchChatDigest{project_id, since, until}` | 拉一段时间群历史,生成本机摘要文件(不进仓不进库;设计期统一:字段以 07 篇为准,补上原缺的 `project_id`) | 新 |
| 通知 | `MarkNotifySeen{project_id, at}` | 记「这个项目的事件流看到哪个时间点」,只影响视觉状态,不参与待处理徽章计数(07 篇新增,本表据此回填) | 新 |
| 通知 | `NotifySyncedToChat`(Event) | 一条通知真实发到群了(或失败带原因) | 新 |
| 运作活 | `StartWeekPlanning` | 总览「开始本周」:建运作活①并跳到会话屏 ▶开工(母文档 §2.6 用户四问第 2 条) | 新 |
| 运作活 | `RunStandardBootstrap` | 一次性运作活③「规范铺底」,内部按检测结果决定要不要多跑「合并调整」「历史回填」,最终一个 MR(待拍-20/27) | 新 |
| 运作活 | `BackfillHistory{project_id}` | 单独重跑历史回填(不重跑第 1/2 步),覆盖上次带回填标记的段落(03 篇新增,本表据此回填) | 新 |
| 运作活 | `ReconcileStandard{project_id}` | 纯读:按「缺/过期/人改过」三类对账,不建活不写仓,给知识库屏渲染用(03 篇新增,本表据此回填) | 新 |
| 运作活 | `UpgradeStandard{project_id, files}` | 人选中要升的文件后触发升级流程,最终提 MR(03 篇新增,本表据此回填) | 新 |
| 运作活 | `CreateAutopilotTask` 加 `auto_run: bool` | 运作活②「资产盘点」到点**自动建活并自动▶开工**(默认 mode=weekly)——今天该命令(`command.rs:558-565`)只建活不跑,复用它加一个字段,不另开平行调度路径 | 改 |
| 运作活 | `OpsWorkflowAutoFired`(Event) | 区别于既有 `CronAutoFired`(只覆盖建活):这条标"建活+自动开工"真的发生了 | 新 |
| 会话 | `OpenRootShell` | 打开/聚焦仓根常驻 shell 会话(待拍-11 借 Orca:不用自己开 PowerShell) | 新 |
| 会话 | `OpenFileTab{issue_id, path}` | 右栏点文件→中栏打开只读代码视图(设计期统一:05 篇是会话屏正主,把早期的 `OpenFile{path}` 改名改签名,本表据此回填) | 新 |
| 会话 | `OpenDiffTab{issue_id, path}` | 中栏打开该活改动文件的 diff(设计期统一:同上,05 篇把早期的 `ShowDiff` 改名改签名) | 新 |
| 会话 | `ExpandTreeDir{issue_id, dir_path}` | 懒加载展开文件树某目录(05 篇新增,本表原缺,据此回填) | 新 |
| 会话 | `RunIssue`/`CancelRun`/`AssignIssue`/`BlockIssue`/`TransitionIssue`/`MergeIssuePr`/`DistillSkillFromIssue`(既有)| 干活/评审/蒸馏语义不变;"按活类别选开工工具、分发到哪个 `adapters/` 模块"是新加的路由层,命令本身不变。**新增触发路径(06 篇 §2.3 定义,待拍-25 改)**:计划屏拖一张活到进行中/评审中/已完成/阻塞列,松手弹确认框,确认后发的就是这几条既有命令——拖拽不新增命令、不绕过 `can_transition_to`,只是给这几条命令多一条触发路径(另一条是详情面板按钮),两条路径最终调用同一套用例 | 沿用 |
| 配置 | `CreateSkill`/`UpdateSkill`/`ImportSkillLibrary`(既有)| 写 `.claude/skills/**/SKILL.md` 技能文件(蒸馏、人手加、从技能库单独导入一个技能)——**不落库表**,"有没有这个技能"扫目录即得(02 篇 §2.6);字段增删留 04 篇 | 沿用,字段留口 |
| 配置 | `ImportSkillPackage`(既有)| **取消**——预置技能包(mattpocock-skills / superpowers / buddy 自建运作 workflow)随 `RunStandardBootstrap`(运作活③规范铺底)一次性复制进项目仓 `.claude/skills/`(02 篇 §2.5/待拍-32),不需要单独的导入命令,更不需要登记表(`skill_package` 表取消) | 删除 |
| 配置 | `SetIssueWorkflow{id, workflow}` | 活详情面板换 workflow/单技能,写 `issue.workflow`(04 篇新增;字段名统一为 `workflow`——04 篇早期草案叫 `workflow_ref`,与 `issue.workflow` 列名对齐后本表据此回填) | 新 |
| 配置 | `SaveToolMapping` | 配置屏第①段保存一行「类别→工具→workflow」映射(04 篇新增,本表据此回填) | 新 |
| 配置 | `ProbeTool` | 手动探活一次(配置屏/项目墙"测一下"复用,04 篇新增,本表据此回填) | 新 |
| 配置 | `MarkEntrySkill` | 人工补标"这是入口技能"(04 篇新增,本表据此回填) | 新 |
| 配置 | `CreateAgent`/`UpdateAgent`/`ImportAgentDefinition`(既有)| 待拍-24 已定"不再单独维护 agent 名单,agent 随 workflow 包走"——配置屏不再有独立 agent 表,这三条**同一次迁移里硬删**(设计期统一:与 02/04 篇一致;V4 用的是**新的库文件**,`schema.sql` 从未定义过 `agent` 表——不是"删表",是"新库从未建过",效果同样是存量队友定义不迁移、不可逆,已提请用户点头,见 握手清单 第 2 条) | 删除 |

**A 刀真接了哪些**(`crates/bw-v4/src/command.rs`,20 条):`CreateProject` / `EditProjectCard` / `SetProjectChat` / `RunStandardBootstrap` / `ReconcileStandard` / `StartWeekPlanning` / `ConfirmWeekDraft` / `CreateIssue` / `ScheduleIssue` / `ReorderIssue` / `SetIssueWorkflow` / `SetCurrentVersion` / `CutRelease` / `RefreshIssueCacheFromPlan` / `RunIssue` / `TransitionIssue` / `BlockIssue` / `SaveToolMapping` / `ProbeTool` / `MarkNotifySeen`。签名与上表有两处出入:`EditProjectCard` 不含 `chat` 字段(项目群走独立的 `SetProjectChat`);`ConfirmWeekDraft` 是上表没有的一条 —— 「开始本周」返回的是草稿活标,人确认之后才真的建活,这一步需要自己的命令。

**表里有、A 刀没接的**:`TogglePreview`、`MergeAndComplete`、`SyncNotifyToChat`、`FetchChatDigest`、`BackfillHistory`、`UpgradeStandard`、`CreateAutopilotTask{auto_run}`、`OpenRootShell`、`OpenFileTab`、`OpenDiffTab`、`ExpandTreeDir`、`MarkEntrySkill`、`SetIssueMetric`,以及所有 `MergeIssuePr` / `DistillSkillFromIssue` 这类「沿用 `bw-app` 既有命令」的行 —— `bw-v4` 没有继承它们,要用得自己写一遍。这些都记在 `docs/LEFTOVERS.md`。

没有一条要求改内核的状态机、「信号只能从数据推导」这条铁律、或只追加表的机制:`bw-v4` 复用 `bw-core` 的 `IssueStatus::can_transition_to`,健康灯由 `bw-v4` 自己那个密封的 `DerivedHealth` 守着(`crates/bw-v4/src/derive/`),store 层没有任何设置信号的方法。

### 2.7 深链环境变量:六入口怎么映射

现状(`DEVELOPMENT.md`「环境变量」;代码见 `app-desktop/src/main.rs:85-127`、`kernel.rs:379-566`):`BW_DB` · `BW_OPEN=<项目名>+BW_PANEL=progress|workflow|routine|artifact|version|issues` · `BW_HUB=skill|agent|workflow|cron|connector|knowledge|activity|notify|settings` · `BW_SEL=<kind>:<uuid>` · `BW_WORKSPACES`/`BW_CLAUDE_BIN`/`BW_FLOW`。

V4 改法:
- **`BW_PANEL` 改枚举值**:`progress|workflow|routine|artifact|version|issues` → `overview|plan|session|notify|config|kb`。`BW_OPEN` 语义不变。
- **`BW_HUB` 退役**:十图标里技能/定时/连接器收进项目内 `config` 屏,知识收进 `kb`,活动/通知收进 `notify`——不再有独立全局 Hub,`BW_HUB` 直接删除,不留兼容层。
- **`BW_SEL` 沿用语法、收窄语义**:`<kind>:<uuid>` 写法保留,但 `agent` 值退场(待拍-24),`cron`/`connector` 并入 `config` 屏内部段落选中,跳转目标从"某 Hub 页面"变成"项目打开后 `config`/`kb` 屏内滚动定位"。
- **新增 `BW_VIEW`(可选,仅顶层三屏)**:`onboard|settings`——项目墙是默认视图不需要变量,深链直跳到不依赖 `BW_OPEN` 的顶层屏时用它。
- **`BW_SCOPE` 去留待定**:今天对应五阶段全部/单阶段视角(`kernel.rs:543`),V4 计划屏无此概念,初步判断退役,留第 6 节开放问题确认。

`[BW_OPEN]`/`[BW_BOOT]` 这两行 stderr 证据日志机制不变,新壳照抄同一套"启动即打读回证据"写法。

### 2.8 文件行数守卫

`op.rs` 今天 2524 行(`crates/app-desktop/src/screens/op.rs`)——正是母文档点名要避免重演的"老 buddy `lib.rs` 一万多行"。建议:

- **上限 1500 行**,不含特殊排除(简单粗暴比"聪明排除注释"更不容易被绕过)。
- **只查新 crate**(`crates/app-shell/src/`),不追溯 `app-desktop`——旧壳反正要删,没必要为将死代码返工;新壳从第一天起就该守规矩。
- **建议阻断,不是只报警**——和 `guard-kernel-ui-free.sh` 同样严格度;新壳是重写不是迁移,没有"存量文件一时改不完"的过渡借口。
- 脚本 `scripts/guard-file-lines.sh`,和另两条并列进门禁清单——**这里只给建议,不改 CLAUDE.md**。

### 2.9 与 `standard/` 的接缝

内容与八大类是 [`../../standard-module-draft.md`](../standard-module-draft.md) 的事,这里只钉三件:

- **放哪**:仓根 `standard/`,和 `crates/`/`docs/` 平级(03 篇 §3 已定)。
- **怎么进二进制**:今天 `docs/buddy/`、`docs/skills/` 都经 `crates/bw-core/src/buddy_assets.rs` 与 `bw_library.rs` 在编译期 `include_str!` 读进常量,零 IO、wasm32 可编译——`standard/` 照这个已验证的模式。**A 刀落地后改落点**:规范素材放在 `crates/bw-v4/src/standard/mod.rs`,不放 `bw-core`。理由是 `bw-core` 里那些资产是 V3 也在用的,而 `standard/` 只有 V4 用;放进 `bw-v4` 既不动旧 crate,也让"往项目仓写规范文件"这个用例和它的素材待在一起。预置技能包是个例外:它复用 `bw_core::bw_library::bw_standard_skill_docs` 已经编进去的那几篇,不再抄一份。
- **版本号从哪读**:仓根放纯文本 `standard/VERSION`(现在是 `4.0`),`include_str!` 读入 `trim()`,铺底/对账命令都读这一个常量(`bw_v4::standard::version()`)。**别和项目仓生成的 `.bw/standard.toml` 混**(03 篇「规范大类 8」)——后者记"某项目用的是哪版"(项目侧,随项目走),前者记"当前 buddy 二进制自带哪版"(buddy 侧,随发布走),数值常相等但含义不同,不合并成一个字段。

### 2.10 与旧壳共存

跑通前 `app-desktop` 整个保留、正常编译发布——这段时间事实上有两个可运行程序:`cargo run -p app-desktop`(十图标+阶段轴,`BW_HUB`/旧 `BW_PANEL` 值继续有效,给未搬功能兜底)、`cargo run -p app-shell`(六入口新壳,未落地的屏如实显示「未建」,不铺占位假数据)。

**computer-use 用的 `~/Applications/BWDev.app` 怎么办**:`scripts/point-bwdev-here.sh` 今天固定拷 `target/debug/builders-workbench` 进这个长期稳定的 bundle(同一次 computer-use 会话里现造的新 app 身份认不出来,复用已注册 bundle id 是唯一路径)。共存期建议验证新壳时复用同一 bundle,只换拷贝 `target/debug/bw-v4-dev`——给脚本加个可选参数,不新注册第二个 app bundle(会重踩"新身份认不出来"的坑)。**A 刀已经改了**:`./scripts/point-bwdev-here.sh v3|v4`,默认 `v3`;两个壳共用同一个 bundle 身份,同一时刻只能接一个,换壳再跑一次。

### 2.11 删除旧壳的判据(逐条核对过,**今天一条都不满足,旧壳不能删**)

把待拍-17"跑通后删旧壳"落成可核对的判据。C 刀收尾时逐条对了一遍实况:

| # | 判据 | 今天满足没有 | 差在哪 |
|---|---|---|---|
| 1 | 04-09 篇每篇第 5 节「验收与读回」跑过至少一次,证据进 `docs/v4-prototype/` | **否** | 读回跑过(深链 + `sqlite3` + `[BW_KB]` / `[BW_CHAT_SENT]` / `pty_smoke`),但**证据没有落成文件进仓**,而且按用户这次的要求没做截图 —— 全旅程由用户自己点 |
| 2 | 母文档 §8 整体验收 8 条全满足 | **否** | 差 5 条:两个真实周循环(要真过两周)、真跑 `claude` 各跑通一张活(受信任对话框/网关影响,不作门禁)、第二台 buddy 纳入同仓审核合入(没做)、Windows 安装包(见下)、项目群真连(只有 mock) |
| 3 | 没有标"未建"的功能挡在日常路径上 | **否** | 至少五处:蒸馏按钮、MR「检查过没过」那一列、Cursor 开工工具、内嵌 Open Design 页签、会话四态(今天只有运行中/空闲) |
| 4 | 满足以上后**同一次改动**删干净 | 不适用 | 前三条没满足 |

**Windows 安装包的实情(C 刀查过)**:`BuildersWorkbench-Setup.exe` 的 Inno 脚本**不在这个仓里** ——
仓里只有 `scripts/bundle-desktop.sh`(macOS `.app`),`.iss` 当初是在 Windows 机器上单独维护的。
所以 `0.4.0-v4` 这一包这台机器上出不了,C 刀能做的只是把 macOS 那条路补齐:
`./scripts/bundle-desktop.sh v4` 打出 `dist/BW-V4.app`(版本号写 `0.4.0-v4`),
`./scripts/point-bwdev-here.sh v4` 接 computer-use 的稳定壳。Windows 包记进 `docs/LEFTOVERS.md`。

**删的时候一次删干净**:删 `crates/app-desktop`、`Cargo.toml` 去掉那一行(顺带去掉 `crates/ui`、
`bw-app`、`bw-store` —— 新壳一个都不依赖)、`app-shell` 的 bin 名改回 `builders-workbench`、
`point-bwdev-here.sh` 只指新壳、`BW_HUB` 读取代码彻底删除、`real_demo` 指挥器退役。
**不分批半删** —— 半吊子共存本身就是要避免的过渡态。

### 2.12 Web 版留门(不多做)

现状:`bw-core`/`ui` 有 wasm32 CI 检查,根 `Cargo.toml` 注释"以后也许"、"架构靠 wasm32 keepalive 与 Store trait 留着门"。新壳不改这个决定:`app-shell` 和 `app-desktop` 一样只用 desktop feature,不为 Web 版新增抽象;新增的 ViewModel 类型(六入口各自的 DTO)一样要过 wasm32 门禁,这就是留门的全部代价——不多花力气,也不倒退。

---

## 3 · 工程对照

### 3.1 workspace `Cargo.toml` 增量(A 刀实况)

```toml
[workspace]
members = [
    "crates/bw-core", "crates/bw-engine", "crates/bw-store",
    "crates/bw-app", "crates/ui",
    "crates/bw-v4",         # V4 内核(新增)
    "crates/app-desktop",   # 共存期保留,删除旧壳时去掉
    "crates/app-shell",     # V4 新壳(新增)
]
default-members = [ /* …原有的无头内核 + ui… */, "crates/bw-v4" ]
# bw-v4 进 default-members(它不编 Dioxus,日常 cargo check 该覆盖它);
# 两个 UI crate 都不进。
```

```toml
# crates/app-shell/Cargo.toml —— 依赖只有 bw-v4,没有 bw-store / bw-app / ui
[[bin]]
name = "bw-v4-dev"   # 共存期用这个名字;删除旧壳时改回 "builders-workbench"

[dependencies]
dioxus = { workspace = true, features = ["desktop"] }
bw-core = { workspace = true, features = ["idgen"] }
bw-engine = { workspace = true }
bw-v4 = { workspace = true }
tokio = { workspace = true }
pulldown-cmark = { version = "0.13.4", default-features = false, features = ["html"] }
```

新壳只依赖 `bw-v4`(外加 `bw-core` / `bw-engine` 两个共用件),`bw-store` / `bw-app` / `ui` 三个旧包一个都不进依赖树 —— 删旧壳那次因此只用在 `members` 去掉两行,不必先拆依赖。

### 3.2 每屏唯一出口(A 刀实况)

```rust
// crates/app-shell/src/screens/overview/mod.rs —— 唯一允许被外部调用的东西
pub fn view(p: &crate::vm::ProjectVm, bridge: &crate::bridge::Bridge) -> Element {
    // 内部随便拆多少私有函数,除 view 外本模块不再 pub 任何东西
}
```

两处值得点名:①ViewModel 类型来自 `crate::vm`,不是 `ui` crate(见 §2.3);②发命令不走 `on_command` 回调,走一个可 clone 的 `Bridge` 句柄(`bridge.cmd(Command::…)`)—— 回调在 Dioxus 的事件闭包里要处处借用,句柄 clone 一份进闭包更顺手,通路语义没变:UI 只发命令、只收 ViewModel。

### 3.3–3.4 两道守门脚本(已落地,正本在 `scripts/`)

两个脚本都已经在 `scripts/` 里,**以脚本本身为准**,这里只记它们和当初设想的差异:

- **`scripts/guard-no-cross-screen-import.sh`**:逐个 `crates/app-shell/src/screens/<屏>/` 目录 grep `crate::screens::`,命中除自己以外的屏幕名就报错、退出码非零。和建议稿的唯一出入是错误提示里那句"共享数据请经 `ui` crate"改成了"经 ViewModel 或命令/事件绕一圈"(见 §2.3:V4 的 ViewModel 不在 `ui` crate)。
- **`scripts/guard-file-lines.sh`**:1500 行硬上限(阻断)+ 600 行软目标(只提醒不阻断,建议稿没有这一档)。查 `crates/app-shell/src` 与 `crates/bw-v4/src` 两处,不只查新壳 —— 新内核同样是从零写的,同样该守规矩。目录不存在时退出码 0(方便在还没建 crate 的分支上跑门禁)。

- **`scripts/guard-screen-hooks.sh`**(建议稿没有,A 刀评审抓到真 bug 后补的):用了 `use_signal` / `use_future` 这类 Dioxus hook 的函数,必须是组件(`#[component]`)。普通函数在 `rsx!` 里被直接调用时,它的 hook 落在**调用方**的 hook 表上按序号取;切一次屏,序号还在、类型换了(接入屏那格是 `String`,计划屏那格是 `Option<CardItemVm>`),Dioxus 取 hook 时 downcast 失败,整次渲染 panic —— 用户看到的是一块空白面板,编译期一点征兆都没有。放在 `for` 循环里更糟:hook 数量随列表长度变,行与行的输入框内容会串位。

三条都已进 `DEVELOPMENT.md` 的门禁清单。A 刀交付时的实际水位:没有文件超过 1500 行,超过 600 行软目标的 0 个(`vm_build.rs` 长到 609 行时按软目标拆成了 `vm_build.rs` + `vm_panels.rs`)。

### 3.5 `Command`/`Event`(A 刀实况:全新一对枚举)

`crates/bw-v4/src/command.rs` 里是**全新的一对枚举**(为什么见 §2.1),`bw-app` 那对一行没动。真写进去的签名:

```rust
// crates/bw-v4/src/command.rs —— 只列 A 刀真接了实现的
pub enum Command {
    CreateProject { intent: ProjectIntent, remote: Option<RemoteRef> },
    EditProjectCard { project_id: ProjectId, intent: ProjectIntent },   // 项目群不在这里,走下面那条
    SetProjectChat { project_id: ProjectId, provider: String, group_id: String, notify: Vec<String> },
    RunStandardBootstrap { project_id: ProjectId },
    ReconcileStandard { project_id: ProjectId },
    StartWeekPlanning { project_id: ProjectId, week: String },
    ConfirmWeekDraft { project_id: ProjectId, week: String, titles: Vec<String> },  // 原表没有:草稿要人确认才建活
    CreateIssue { project_id: ProjectId, title: String, body: String,
                  category: Option<Category>, kind: IssueKind, origin: IssueOrigin, week_of: String },
    ScheduleIssue { id: IssueId, week_of: Option<String> },   // None = 移回待办池
    ReorderIssue { id: IssueId, after: Option<IssueId> },
    SetIssueWorkflow { id: IssueId, workflow: String },
    SetCurrentVersion { project_id: ProjectId, version: String },
    CutRelease { project_id: ProjectId, version: String, note: String, included: Vec<IssueId> },
    RefreshIssueCacheFromPlan { project_id: ProjectId, week: String },
    RunIssue { id: IssueId },
    TransitionIssue { id: IssueId, to: IssueStatus },
    BlockIssue { id: IssueId, reason: String },
    SaveToolMapping { project_id: ProjectId, category: Category, tool: String, workflow: String },
    ProbeTool { name: String },
    MarkNotifySeen { project_id: ProjectId },
}
```

`Event` 同样是新枚举,20 个变体,一条命令对一到几条回执(`IssueCreated` / `IssueScheduled` / `IssueRan` / `IssueTransitioned{settled}` / `ReleaseCut{rows_written}` / `StandardBootstrapped{files,committed}` / `HealthDerived{signal}` 等)。回执里带的是**真发生了什么**,不是"命令收到了":比如 `ReleaseCut.rows_written` 为假就表示这个版本号已经在发版记录里、这次没写第二行。

**表里有、A 刀没接的那批**见 §2.6 表后一段。`CreateAgent` / `UpdateAgent` / `ImportAgentDefinition` / `ImportSkillPackage` 四条"硬删"自然消失了 —— 新枚举里从来没有它们,不存在删的动作;V4 新库的 `schema.sql` 也从未定义过 `agent` 表,存量队友定义不迁移、不可逆(握手清单 第 2 条已提请用户点头)。

### 3.6 深链读取点(伪码,对照 `kernel.rs:526-566` 改写,细节见 2.7)

```rust
// crates/app-shell/src/bridge/mod.rs —— BW_PANEL 枚举值改新六入口,BW_HUB 不再读取
let panel = match pl.as_str() {
    "overview" => Panel::Overview, "plan" => Panel::Plan, "session" => Panel::Session,
    "notify" => Panel::Notify, "config" => Panel::Config, "kb" => Panel::Kb,
    other => { eprintln!("[BW_OPEN] 未知 BW_PANEL={other:?}"); return; }
};
```

---

## 4 · 边界与失败

**不做的事**:不改内核五个 crate 的既有类型系统或铁律机制——状态机、密封的信号类型(信号只能从数据推导,类型名见第 3 节)、只追加表不动,只在 `Command`/`Event` 两个枚举上加法;不定义每屏具体交互(04-09 篇);不定义 `standard/` 内部结构(03 篇);不做符号级大纲(待拍-22 已定首版不做,只留适配模块口子);不做 Web 版实现。

**失败如实显示,不假装**:新 crate `cargo check -p app-shell` 失败就是失败,不用 `#[allow(dead_code)]` 掩盖没写完的屏——没做的屏在路由里直接给「未建」占位文案,不放模拟数据;两条 guard 脚本命中就是 CI 红,给出具体文件和行,不做"警告但放行"的软处理(除非用户在第 6 节拍板要 warn-only);深链遇到未落地的 `BW_PANEL`/`BW_VIEW` 值,stderr 如实打印"未知值",不静默 fallback 到某个默认屏;共存期若某命令还没接内核实现,`bw-app` 侧应诚实报错或标"未接",新壳不允许自己在 UI 层伪造成功事件。

---

## 5 · 验收与读回

1. `cargo check -p app-shell` 通过(骨架阶段:九屏目录+六适配模块目录都存在且能编译,哪怕内容是 `todo!()`)。
2. `./scripts/guard-kernel-ui-free.sh` 仍全绿(新增 `app-shell` 不在稽查名单,但不能因它的存在把某内核 crate 意外拉出 UI 依赖)。
3. `guard-no-cross-screen-import.sh`/`guard-file-lines.sh`(建议新增)骨架阶段应是绿的——空目录/空文件天然不跨屏引用、不超行数,这条验收的意义是"守卫脚本本身写对了、能跑"。
4. `BW_OPEN=<项目名> BW_PANEL=overview` 深链能启动,stderr 打出 `[BW_OPEN]` 证据行——即便 `overview` 屏此时只是占位内容,深链链路本身(env 读取→内核桥→事件回传→路由到对应屏)要走通。
5. `cargo check -p app-desktop` 依旧通过——共存期旧壳不受影响,证明这次改动是纯增量。
6. `cargo tree -p app-shell --edges normal | grep -Ei 'dioxus|wry'` 能看到依赖;`cargo tree -p bw-app --edges normal | grep -Ei 'dioxus|wry'` 应零命中——证明新增的 `Command`/`Event` 变体没有让内核反向依赖 UI。

---

## 6 · 开放问题

1. **新 crate 最终叫什么名字**:本篇推荐 `app-shell`(理由见 2.1),备选 `app-v4`(缺点:把版本号焊进一个将来要长期活下去的名字里)。请用户拍板。
2. **文件行数守卫阻断还是只报警**:本篇推荐直接阻断(2.8 节理由),这是条新门禁,建议用户确认后再正式写进 `CLAUDE.md` 门禁清单。
3. **旧壳删除的时间点判据**(2.11 节四条)是否采纳,还是要更严格/更宽松的标准——例如是否要求"内部试点跑完两周"(母文档 §8)也算进删除判据,而不是"六屏验收过了就删"。
4. **`BW_SCOPE` 环境变量去留**:五阶段视角切换在 V4 计划屏无直接对应物,初步判断退役,但计划屏是否需要新的"按类别标签筛选"深链变量,待 06 篇确认后再回来改这条。
5. ~~`CreateAgent`/`UpdateAgent`/`ImportAgentDefinition` 退场后存量数据怎么处理~~ **已定(设计期统一)**:硬删,存量队友定义不迁移——V4 新库 `schema.sql` 从未定义过 `agent` 表(不是"删表",是"新库从未建过",效果同样不可逆),与 02/04 篇一致,理由见 02 篇 §2.3、04 篇 §2.10。

---

## 与代码的关系

这篇不改 `crates/`。开工时按 2.1-2.11 的顺序建 `crates/app-shell`;第 3 节就是这一步的开工清单;第 5 节就是这一步的验收清单。各屏具体内容等 05-09 篇稿子出来后再填进对应 `screens/<name>/`。
