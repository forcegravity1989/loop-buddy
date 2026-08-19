# 12 · 建法:三刀,一次挂连着做(含今晚任务书)

> **30 秒导读**:这篇是 V4 从设计转开发的**执行件**:把 design/01–11 切成三刀(A 骨架与主环 / B 运作活与会话屏 / C 回填与项目群与包),**一次挂上连着做 A→B→C**,做到哪算哪、每刀独立 commit 与 PR、不停下来等人。§2 是任务书(范围 / 明确不做 / 建法顺序 / 逐条读回命令 / 默认假设 / 卡住预案),§5 是挂任务时直接粘贴的指令。给两种人看:挂任务的用户(看 §0、§2.4、§5),接任务的会话(全文)。**现状:2026-08-20 第七轮盘点后重写(库只剩四张表),用户点头即挂**;每刀做完按块重写本篇对应小节(不追加补丁)。

## 0 · 总则(用户 2026-08-20 两轮回复定下的)

1. **一次挂上连着做 A→B→C,不等人**;每刀做完各自 commit + 开 PR(不合),接着做下一刀;做不完就停在最后一个可编译可读回的状态,PR 正文写清做到哪。刀内可开子代理(不得用 Fable——见全局 `~/.claude/CLAUDE.md`,按难度 haiku / sonnet / opus)。
2. **V4 不兼容老库**:新库文件(新壳默认 `workbench-v4.db`,与旧壳的 `workbench.db` 同目录不同名;`BW_DB` 可覆盖),`schema.sql` 按新设计写全,不写 V3→V4 迁移,开发期换 schema 删库重建;`add_column_if_missing` 守则从试点起再执行(待拍-30)。
3. **存就是为了取**(第七轮盘点定死):**库只有四张表**——`project`(定位 + 项目墙显示缓存)/ `issue`(远端 issue 的本机缓存)/ `claude_conversation`(活 ↔ 会话 ↔ worktree ↔ 分支)/ `app_meta`。别的一律不建(母文档 §6.2/6.3 逐条列了为什么)。数字全部从 git / 远端 / 仓文件现算。
4. **验证 = 指挥器 + sqlite 读回 + 深链 stderr**;不做 computer-use 点击巡航(用户次日自己点全旅程、给反馈);截图可选不强求。
5. **控制代码量、模块规划先行**:一屏一目录、一个外部能力一个适配模块、单文件 1500 行阻断(守卫脚本),软目标单文件 ≤ 600 行;三刀合计新增 Rust 预算 **≤ 12,000 行**(超了先砍范围不砍守卫)。
6. **文档按块重写**:每刀结束把 design/ 里与实际不符的小节整块删了按实况重写;「没干的」只记 `docs/LEFTOVERS.md`。
7. **不被老项目干扰**:新壳按设计建,V3 只抄 token / 组件(01 篇 §2.5 清单),不迁就旧结构;旧壳 `app-desktop` 只保证继续编译,不改它。
8. **门禁不变 + 两个新守卫**:`cargo fmt --check`、`clippy --workspace --exclude app-desktop --exclude app-shell -D warnings`、wasm32 两项、`guard-kernel-ui-free.sh`、`cargo check -p app-desktop`、`cargo check -p app-shell`、`cargo test --workspace --exclude app-desktop --exclude app-shell`、**`guard-no-cross-screen-import.sh`、`guard-file-lines.sh`**。每个 commit 前全过。
9. **合不合**:每刀结束开 PR、门禁全绿、`/code-review` 过,**不自动合**;早上用户看。用户次日自己点全旅程给反馈,不要做 computer-use 点击巡航。

## 1 · 三刀总表

| 刀 | 做什么 | 当晚读回什么 | 不做什么(留后刀) |
|---|---|---|---|
| **A 骨架 + 数据 + 主环** | 数据层(新库、`issue` 八列、三张小表)→ 仓根 `standard/` 核心件 + 铺底第 1 步(写模板,不起 agent)→ `crates/app-shell` 起壳:项目墙 / 接入两卡 / 设置 / 总览 / **计划(六列拖拽 + 确认弹窗)** / 配置(映射三列 + workflow 表)+ 会话 / 通知 / 知识库的真实数据最简列表 → 命令增量 → `real_demo_v4` 步骤 1–7、10 | 门禁全绿;`sqlite3` 读回新列;`docs/plan/2026-W34.md`、`docs/releases.md` 被真实写出;指挥器 evidence JSON;六入口深链 stderr `[BW_OPEN]` | 三张运作 workflow 的 SKILL.md 正本、内嵌终端、定时自动建②、项目群、历史回填、知识库代码图、Windows 包 |
| **B 运作活 + 会话屏** | 运作 workflow 三份 SKILL.md(`standard/06-defaults/ops/`)+ 运作活①真会话(mock 执行器可跑)+ 定时周五晚自动建②自动开工 + 周计划文件「本周指标读数」段 + 会话屏(内嵌终端 + 文件树 + diff 页签,`terminal_xterm` 适配)+ 通知屏「合入并完成」+ `real_demo_v4` 步骤 8 | 同上 + 定时 tick 读回(`origin='auto'` 且状态非待办)+ 内嵌终端 `pty_smoke` | 项目群、历史回填、发版本之外的远端链路 |
| **C 回填 + 项目群 + 知识库 + 包** | 资产盘点首次模式(证据层真代码 + agent 步骤;产出同格式历史周文件与历史版本行)+ `chat_group` 工厂(trait + mock / none,WeLink 函数留位)+ 通知同步(不做发送去重) + 知识库三页签(含 codegraph 大文件榜)+ `real_demo_v4` 步骤 9 + Windows 安装包 `0.4.0-v4` + 删旧壳判据核对 | 同上 + 老项目样例(buddy 自己的仓)回填数字对回 git | 试点期才做的:WeLink 真连、codehub 项目接入 |

## 2 · 任务书(切片 A 详;B / C 见 §3)

### 2.1 范围(按建法顺序,每小步一个 commit)

| 步 | 做什么 | 出处 | 读回 |
|---|---|---|---|
| A1 数据层 | `bw-store`:新库文件 `workbench-v4.db`(旧壳仍开 `workbench.db`,互不相扰);`schema.sql` **从零写全,只四张表**——`project`(id / slug / name 缓存 / 仓路径 / 远端地址 / 灯缓存 + 算出时间 / 排序 / 时间戳)、`issue`(id / project_id / 远端号 / 标题 / 状态 / 分支 / PR 号 + 缓存属性列 `week_of` `version` `tool` `workflow` `kind` `origin` `sort_order` `metric_key` / 时间戳)、`claude_conversation`(沿用今天的列)、`app_meta`;**其余 16 张一律不建**(母文档 §6.3);store 只留这四张表的读写方法,其它方法随表删 | 02 篇 / 母文档 §6.2 | `.tables` 只有四张;`PRAGMA table_info(issue)` 见缓存列 |
| A2 仓根 `standard/` | 核心件模板:`PROJECT.md`、`AGENTS.md`(+ `CLAUDE.md` 一行 `@AGENTS.md`)、`.bw/project.toml`(含 `[chat]` 空位、`standard_version`、`current_version`)、`.bw/metrics.toml`、`.bw/issue-policy.toml`(`[[tool]]` 三个 + `[[mapping]]` 六行,构建 / 优化 / 运维默认 mattpocock-skills)、`.bw/standard.toml`、`.bw/managed.toml`、`docs/plan/`(README + 周文件模板,front matter `week` / `origin` + 「本周指标读数」段)、`docs/releases.md` 模板;**预置技能包**(buddy 自建运作 workflow + 业界包)放 `standard/06-defaults/skills/`,铺底时**复制进项目仓 `.claude/skills/`**;`include_str!` 进 `bw-app`;`RunStandardBootstrap` 第 1 步:建分支写模板 + 提交(空仓 / 自己的仓直接提交当前分支),记指纹进 `managed.toml` | 02 / 03 篇 | 跑完 `ls -R <ws>/<slug>/.bw docs`;`git log` 见提交 |
| A3 命令增量(`bw-app`) | `ScheduleIssue{id,week_of:Option}` / `ReorderIssue{id,after}`(**落点 = 改 `docs/plan/YYYY-Www.md` 的活清单 + 刷新 `issue` 缓存行 + 可选打远端标签 `bw/week:*`)、`SaveToolMapping`(写 `issue-policy.toml`)、`EditProjectCard`(写 `PROJECT.md` + `.bw/project.toml`)、`SetProjectChat`(写 `[chat]`)、`StartWeekPlanning`(判据 = 本周文件不存在;**今晚 mock**:写 `docs/plan/YYYY-Www.md` 含指标读数段 + 产出固定草稿活标【mock】,人确认后建活 `origin=agent_split` + `week_of`)、`CutRelease`(追加 `docs/releases.md` 一段 + 给活写 `version`,走轻量活;未配远端时直接提交)、`ProbeTool`(claude / cursor / open_design / **welink-cli 留位返回 Unknown**)、`CreateIssue` 建活时按映射填 `tool` / `workflow`;`RunIssue` / `TransitionIssue` / `BlockIssue` 不改;**不做任何战绩记账**(第七轮取消);「用了几次」在配置屏按活的 `workflow` 属性现算;对应 `Event` | 01 篇 §2.6 / 03 / 06 / 04 篇 | 指挥器读回(下表) |
| A4 新壳 `crates/app-shell` | `Cargo.toml`(members 加、default-members 不加;bin `bw-v4-dev`)、`main.rs`(wry 窗口;Windows `with_disable_drag_drop_handler(true)`)、`bridge/`(抄 kernel.rs 的独立 tokio 线程做法)、`theme/`(抄 V3 token + 原子样式)、`screens/`:**wall**(抄 V3 ProjectCard + 本机环境条「测一下」含 welink-cli 灰项)/ **onboard**(抄两卡,四字段意图卡)/ **settings**(工作区根目录、工具路径;无聊天登录)/ **overview**(一列横块,全部现算:名片 ← `PROJECT.md`,指标卡 ← `metrics.toml` + 现算 / 周计划文件读数段,health 三判据 ← 周计划文件 + git 提交 + git 合入 / `releases.md`(没数据 = Unknown 灰),本周进度 ← 周计划文件 + `issue` 缓存,发版记录 ← `releases.md`)/ **plan**(左栏扫 `docs/plan/` 出周列表 + 六列看板;**所有列可拖**:排期直接生效,状态动作确认弹窗,非法弹回;卡面无按钮;右侧详情面板放按钮组;新建活)/ **config**(映射三列可编辑;workflow / skill 表 = 扫项目仓 `.claude/skills/` + 从活的 `workflow` 属性现算「用过几次」;连接器段 = 显示 `.bw/project.toml` 里的远端 + 探活;定时段 = 显示 `issue-policy.toml` 节律)/ **session / notify / kb**:真实数据最简列表(按活列会话 ← `claude_conversation` · 评审中与待人处理 ← `issue` 缓存 · 仓内 `docs/` 文档树 + Markdown 渲染),不摆假数据;`adapters/`:`claude_cli`(只声明 + 探活)、`chat_group`(只放 trait 与 none;实现在 C 刀)各带 README 三段;深链 `BW_OPEN` / `BW_PANEL=overview|plan|session|notify|config|kb` / `BW_VIEW=onboard|settings` | 01 / 05 / 06 / 08 篇 | 六入口深链 stderr `[BW_OPEN]`;`cargo check -p app-shell` |
| A5 守卫 + 脚本 | `scripts/guard-no-cross-screen-import.sh`、`scripts/guard-file-lines.sh`(1500 阻断,只查 app-shell);`scripts/point-bwdev-here.sh` 加可选参数拷 `bw-v4-dev`;`DEVELOPMENT.md` 加 app-shell 三条命令 | 01 篇 §3.3–3.4 | 两脚本退出码 0 |
| A6 指挥器 | `crates/bw-app/examples/real_demo_v4.rs`:步骤 1 接入 → 2 铺底第 1 步(含复制预置技能包)→ 3–4 开始本周(mock)写周计划文件 + 代人确认建活 → 5–6 一张活 ▶跑(mock)→ 代人推评审中 → 代人点完成 → 7 发版本(写 `releases.md` + `issue.version`)→ 10 evidence JSON(数字全部真实读回,不硬编);幂等可重跑;工作区用 buddy 仓 `git clone --local` 到临时目录 | 10 篇 §2.2 | 见 §2.4 |
| A7 收尾 | 门禁全绿 → `/code-review` → 修 → design/01/02/03/06 与实况不符的小节按块重写 → `docs/LEFTOVERS.md` 登记今晚没做的 → push → 开 PR(正文含「偏差」段)→ **不合** | 总则 6 / 9 | PR 链接 |

### 2.2 明确不做(今晚)

内嵌终端与 `terminal_xterm`(B);三张运作 workflow SKILL.md 正本与真会话(B,今晚 ① 用 mock 草稿);定时自动建②(B);`chat_group` 实现与通知同步到群(C;不建发件箱表,不做发送去重);历史回填(C);知识库代码图 / 资产页签(C);Windows 打包(C);指南抽屉、问题上报图标、Web 留门、Cursor 真接法(只声明)、Open Design 嵌入(沿用 V3 探活,嵌入留 B)、**动旧库与旧壳**(`app-desktop` 与 `workbench.db` 原样不碰,V4 是全新库文件与全新壳)。

### 2.3 默认假设(用户不反馈就这么做)

- 基线分支:PR #104 若已合,从 `main` 开 `claude/v4-slice-a`;没合就从 `claude/v4-round4-expert-feedback` 开(设计件要在脚下)。
- 本周 = ISO 2026-W34;指挥器项目名 `buddy-v4-demo`,工作区 = buddy 仓本地浅拷贝(不连远端、不真开 PR:代人推评审中 / 完成,evidence 明写「脚本代人」)。
- 所有 ▶跑 走 mock 交互执行器(产出自标【mock】);不碰真 `claude`、不碰网关、不碰 WeLink。
- 子代理分工:主线程做骨架与接缝(Cargo、bridge、`Command` / `Event` 签名、schema);子代理填屏(每屏一个,sonnet)、填指挥器(sonnet)、评审(opus);主线程合并、跑门禁、`/code-review`。
- 看不准的细节按 design/ 对应篇的默认答案;design/ 与母文档冲突以母文档 §6 / §11 为准;再拿不准就按「简单 / 规范」的来并在 PR「偏差」段写明。

### 2.4 次日早上的验收读回(用户照着跑)

```bash
# 1 门禁(与 CI 一致 + 两个新守卫)
cargo fmt --all --check && cargo clippy --workspace --exclude app-desktop --exclude app-shell -- -D warnings && cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features && cargo check -p ui --target wasm32-unknown-unknown && ./scripts/guard-kernel-ui-free.sh && cargo check -p app-desktop && cargo check -p app-shell && cargo test --workspace --exclude app-desktop --exclude app-shell && ./scripts/guard-no-cross-screen-import.sh && ./scripts/guard-file-lines.sh
```

```bash
# 2 指挥器从空库跑一遍(重跑不产生重复数据)
rm -rf /tmp/bw-v4-demo.db /tmp/bw-v4-ws && cargo run -p bw-app --example real_demo_v4 -- /tmp/bw-v4-demo.db /tmp/bw-v4-ws && cargo run -p bw-app --example real_demo_v4 -- /tmp/bw-v4-demo.db /tmp/bw-v4-ws
```

```bash
# 3 数据读回:库里只有四张表 / issue 缓存列齐 / 活的周·来源·工具·workflow·版本 / 完成只结一次
sqlite3 /tmp/bw-v4-demo.db ".tables" && sqlite3 /tmp/bw-v4-demo.db "PRAGMA table_info(issue);" | grep -E "week_of|version|tool|kind|origin|workflow|sort_order|metric_key" && sqlite3 -header /tmp/bw-v4-demo.db "SELECT title,status,week_of,origin,tool,workflow,version,settled_at IS NOT NULL AS settled FROM issue ORDER BY created_at;"
```

```bash
# 4 仓文件读回:周计划文件(活清单 + 指标读数段)、发版记录、规范件与指纹、复制进来的预置技能包
ls -R /tmp/bw-v4-ws/buddy-v4-demo/.bw /tmp/bw-v4-ws/buddy-v4-demo/.claude/skills && cat /tmp/bw-v4-ws/buddy-v4-demo/docs/plan/2026-W34.md && cat /tmp/bw-v4-ws/buddy-v4-demo/docs/releases.md && git -C /tmp/bw-v4-ws/buddy-v4-demo log --oneline | head
```

```bash
# 5 新壳六入口深链各起一次,stderr 见 [BW_OPEN] 且无 panic(看一眼就关)
cargo build -p app-shell && for p in overview plan session notify config kb; do BW_DB=/tmp/bw-v4-demo.db BW_OPEN=buddy-v4-demo BW_PANEL=$p timeout 8 ./target/debug/bw-v4-dev 2>&1 | grep -m1 "\[BW_OPEN\]"; done
```

然后你自己开 `bw-v4-dev` 点全旅程(项目墙 → 接入 → 总览 → 计划拖四下 → 配置改映射 → 设置),把感受回我;这一步不归我。

### 2.5 卡住预案

- 编译 / 门禁卡住:先缩范围(砍 A4 的 session / notify / kb 列表、砍 A3 的 `CutRelease`),不留不编译的代码;每砍一项记 `docs/LEFTOVERS.md`。
- 会话额度打穿:每小步已 commit;恢复后从 §2.1 表里没打勾的步继续;早上拿到的是「A 完整」或「A 到第 N 步」,PR 正文如实写到哪。
- 设计与实况冲突:按母文档 §6 / §11 → 「简单 / 规范」→ 写进 PR「偏差」段,不回头改设计决定。
- 任何需要用户拍板的事:不等、按默认做、写进偏差段。

## 3 · 切片 B / C 任务书

**接着 A 做,不停下来等人**。范围见 §1 总表;做每一刀之前先把 design/ 对应篇再读一遍(B:05 会话屏、09 运作活、04 workflow;C:03 回填、07 项目群、11 知识库、10 验收)。两刀的读回都在 §2.4 那五组之上加:B 加「定时到点后本周出现资产盘点活且 `origin='auto'`」与 `pty_smoke`;C 加「老项目样例回填出的历史周文件与历史版本行,数字对得回 git」。每刀结束照 A7 收尾(门禁 → `/code-review` → 文档按块重写 → PR 不合)。

## 4 · 与代码的关系

本篇本身不改 `crates/`;切片 A 开工的第一个 commit 就是 A1。每刀结束本篇 §2 的表按实况补「做到哪 / 偏差」一列,不另开日志文件。

## 5 · 挂任务时粘贴的指令

> 按 `docs/v4-prototype/design/12-build-plan.md` 开工:先读 §0 总则与 §2.3 默认假设,再按 §2.1 的 A1→A7 做完切片 A,**接着按 §1 总表继续做 B、C,不要停下来等我**。每小步一个 commit(标题人话),每刀结束跑门禁 + `/code-review` + 按块重写 design/ 里与实况不符的小节,然后 push 开 PR(正文含「做到哪 / 偏差 / 早上怎么验」三段)、**不要合**,接着做下一刀。设计依据 design/01–12;与母文档冲突以 `mvp-blueprint-draft.md` §6(信息住哪:库只有四张表)与 §11 为准。子代理一律不用 Fable(见 `~/.claude/CLAUDE.md`),按难度 haiku / sonnet / opus。验证只做 §2.4 那五组读回 + 门禁 + `/code-review`,**不做 computer-use 点击巡航**(用户次日自己点全旅程)。卡住按 §2.5 处理:能绕的绕并记进 `docs/LEFTOVERS.md`,绕不过就停在可编译可读回的状态,如实写进 PR。
