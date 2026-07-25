# plan/15 · 验收动作流与对抗式验收工作流

> 2026-07-24 brainstorming 拍板。编号 15:plan/14 已被创建体验批(未合分支)占用,如实跳号不抢位。

## 0 · 缘起与命题(人话)

系统已经大到"操作逻辑复杂、全靠人手点验收不可持续"。这一批要建的是**验收循环本体**:

> **点击动作流是核心验收件。** Sonnet subagent 负责实现;Fable 作为最强验收 Reviewer,亲手驾驶真实应用跑完点击动作流,产出一份给人看的 HTML 证据报告;用户靠证据链终审,不靠手点。开发过程本身就是一个 workflow——对抗式的验收循环,不是单向的产出流水线。

**四条拍板(2026-07-24)**:

1. **两层制验收件**:少量「常青主干流」锁铁律路径,每批全绿;「每票验收流」交付时必须绿,之后并入主干或退役。不搞永久累积的回归大坝(会烂成假绿)。
2. **自证 + 对抗审,断言不是防线**:实现方随时能绕过断言,防线是**证据链**——审核方独立跑流亲眼确认结果,报告里永远给原值(截图、SQL 读回),预期只是陈述,证据优先于断言。
3. **基建 + 常青流先行**:本批只建基建和 3-5 条常青流并出首份报告;「每票必附验收流」纪律等基建自身被验收后、下一批开启。
4. **技术路线 B:OS 级 computer-use**。真窗口、真鼠标、真截图——验收证据链就该是人眼看到的东西,不是应用内部的自说自话。历史暗礁(裸二进制拿不到窗口、截图权限降级)用第 0 阶段闸门一次性排掉。

## 1 · 总架构

> 下图为原始设计;驱动环节按 §2.1 转向后为:**Fable 用深链 + `BW_FLOW` 应用内注入驱动,用 MCP 真窗口截图取证**。

```
流文件(考卷 TOML, e2e/flows/) ──> Fable 亲自驾驶打包后的 BW.app
                                    │  每步:截图→定位→动作→截图→对照预期→记 verdict
                                    ▼
                    e2e/reports/<UTC时间戳>-<批次>/ (gitignored)
                      步骤截图 PNG · run.log 逐步 verdict · readback.txt SQL原值
                                    │
                                    ▼
                    report.html(单文件自包含证据报告,SendUserFile 送用户)
                    报告**不进 git**(2026-07-25 拍板,见 §5.1)
```

**执行者是 Agent 不是脚本**:流文件写语义步骤("点左侧图标栏的 Workflow 图标"),由 Fable 在验收 session 里通过 computer-use 逐步执行。审核方亲手操作实现方交付的应用 = 对抗性的本体,实现方无法在考卷执行层做手脚。代价如实:每次验收跑消耗 Fable 的视觉循环 token,验收跑发生在每批审核环节,不是免费 cron 回归。

## 2 · 第 0 阶段可行性闸门(不过闸不铺开)

| 关 | 实测内容 | 历史暗礁 | 过关证据 |
|---|---|---|---|
| G-1 打包 | `dx bundle` 产出 BW.app,`scripts/bundle-desktop.sh` 固化;启动方式=终端直启 `BW.app/Contents/MacOS/<bin>`(保留 BW_DB/BW_OPEN 等 env 深链,`open` 不传 env) | 裸 debug 二进制拿不到窗口 | BW.app 启动,窗口被 computer-use 看见 |
| G-2 驱动 | `request_access` + 实际点击一个按钮 | 点击曾被阻 | 点击后 UI 状态变化的前后截图 |
| G-3 截图落盘 | `screencapture -l <窗口ID>` 出 PNG 文件 | 终端宿主无屏幕录制权限→只拍到墙纸 | 含 BW 窗口内容的 PNG 文件;需用户在系统设置一次性授权 |

任何一关卡死:停下,拿实测证据找用户定夺(备选=A 路线应用内自截做落盘、B 路线做驱动的混合),**绝不静默换路线**。

### 2.1 实测结果与路线转向(2026-07-24 Fable 亲驾,证据 `iterations/evidence/gate-2026-07-24/GATE.md`)

| 关 | 裁决 | 实况 |
|---|---|---|
| G-1 | ✓ 过 | 打包 .app 后 MCP `screenshot` 清晰拍到 BW 真窗口(标题栏+首页文案可读)。**历史「裸 debug 二进制拿不到窗口」暗礁排除**——相对过去会话的真实进展 |
| G-2 | ✗ 环境受阻 | 对窗口内**任意**坐标点击一律被拦:`lands on "程序坞", not in allowed applications`。穷尽自查无效(BW 已授权 tier=full、`open_application` 激活 frontmost、改 `open --env` 以 .app 身份启动、多坐标、试授权程序坞但系统组件不可授权)。根因=本机 computer-use 点击 hit-test 把 BW 窗口区 ownership 误判归 Dock,非 BW/打包侧;与既往跨会话「clicks blocked」复现一致 |
| G-3 | ✗ 权限降级 | CLI `screencapture -l<窗口ID>` 报 could not create image;`-R`/全屏虽落盘但**内容是纯墙纸**(全屏图菜单栏显示「Claude」=宿主进程缺屏幕录制权限)。对照:MCP `screenshot` 有独立 compositor 级授权,拍得到真窗口 |

**用户拍板(2026-07-24):A+B 混合。** 理由:B 路线的初衷「证据链就该是人眼看到的东西」由 **MCP 真窗口截图**完全满足(G-1 已证),卡住的只是模拟点击这一环——那就把驱动换成应用内,证据仍留在真窗口。

修订后的路线:

- **证据(人眼)= MCP computer-use `screenshot` 拍打包后 BW.app 真窗口**,每个检查点一张。
- **驱动 = 应用内**:既有深链(`BW_DB/BW_OPEN/BW_PANEL/BW_HUB/BW_SEL`)直达面板,外加**新增 `BW_FLOW` 注入口**(`document::eval` 在 webview 内派发真实 DOM 事件,走与人手点击完全相同的 onclick→Command→kernel→DB 链路)。
- **破例记账**:原计划「本批零 Rust 改动」作废——`BW_FLOW` 驱动需要 app-desktop 侧少量新代码。如实记入偏差,不假装无改动。
- **不做的**:不修 computer-use 点击环境(跨会话顽疾,不在本批射程)。

### 2.2 截图再转向:应用内自截(2026-07-25 实施期,证据链闭环)

首份报告交付后发现**截图这一环仍是空的**,与「证据链决定用户怎么验收」的原意不符。实测把两条 OS 路径都判死:

- MCP `screenshot` 能拍到真窗口,但**不落盘**(`save_to_disk` 无产物,搜遍容器/缓存/临时目录;后续调用还超时);报告要内嵌图必须有磁盘 PNG。
- CLI `screencapture` 能落盘,但内容是**纯墙纸**(宿主进程缺屏幕录制权限)。

**定论:改由应用自己截图** —— 驱动新增 `snap <名字>` 命令,webview 内把 DOM 序列化进 SVG `foreignObject`、画到 canvas、取 PNG data URL,Rust 解码后写 `<命令文件目录>/snaps/<名字>.png`。**零 OS 权限、直接落盘、拍的是真实渲染结果**;考卷里早就写好的 `snap` 字段就此生效(由 `run-flow.py` 下发)。首张成品实测 1440×806、210KB、4291 色,Issue 看板五列计数与三张 fixture 卡清晰可读。

代价如实:这不是「桌面窗口的相机照片」,是应用自身渲染的快照(不含窗口边框/系统 UI)。就本仓的验收目的而言,要证明的恰是**界面渲染出的内容**,这一点它证得比屏幕照片更干净。

**读报告须知(避免误读为 bug)**:本机制对**未变化的 DOM 是逐字节确定性**的 —— 同一屏内容拍两次,PNG md5 必然相同。所以一条流里相邻两个 checkpoint 的图完全一样,通常意味着「该动作在自己的 `wait_s` 内就已到终态,后一张只是再确认一次同一终态」,不是串图、不是缓存。已用两组实验坐实:同屏连拍 → md5 相同(确定性);紧凑竞态下拍两个不同 Issue 详情 → md5 不同(seq 匹配+清空有效,绝不串号)。

## 3 · 流文件格式(考卷)

位置:`e2e/flows/core/`(常青主干)· `e2e/flows/tickets/<票号>/`(每票,下一批启用)。TOML,人类可读可审——考卷本身要经得起对抗审查。

```toml
name = "issue-run-review-done"
purpose = "铁律路径:跑 Issue → InReview → 人点 Done → settled_at 落账"
db = "fixture:demo"            # fixture:<名>=种子库副本 | copy:daily=真实日常库副本
launch = { BW_OPEN = "示例项目", BW_PANEL = "issues" }

[[step]]
do    = "click"
where = "Issue 卡「示例任务」上的 ▶ 开工按钮"   # 语义定位,给 Agent 眼睛看的
snap  = "run-clicked"                            # 该步前后各存一张 PNG

[[step]]
do        = "wait"
until     = "该卡状态徽记变为「评审中」"
timeout_s = 60

[[step]]
do    = "click"
where = "评审中卡片上的「完成」按钮"
snap  = "human-done"

[[verify]]
sql    = "SELECT status, settled_at FROM issue WHERE title='示例任务'"
expect = "Done 且 settled_at 非空"               # 预期陈述;报告永远同时给 SQL 原值
```

要点:
- **语义定位,不是坐标不是选择器**——UI 改版不脆断,考卷不依赖实现细节。
- `db` 两态:①-④类流用 fixture 种子副本(确定性、Mock 执行器、绝不依赖网关,CLAUDE.md 纪律 3);真实库冒烟流用 `copy:daily`(plan/12 §10 真实感纪律),只读巡检。
- fixture 来源:`real_demo --mock` 一次性生成种子库存入 `e2e/fixtures/`,每次跑流复制到临时路径经 `BW_DB` 注入——原 fixture 与真实日常库都绝不被流触碰。
- 每票考卷由 Fable 起草(派工时考卷先行,实现前定验收标准);实现方可补步骤不可删步骤。

## 4 · 常青五条流(本批交付)

| # | 流 | 锁的铁律 |
|---|---|---|
| 1 | 建项目向导就绪(止步不可逆边界) | 创建链路 UI 端到端可达;**不点「开始 ↑」** |
| 2 | 跑 Issue(Mock) | run 只推 InReview,绝不自动 Done |
| 3 | 人点 Done | Done 入边仅 InReview;settled_at 读回非空 |
| 4 | 蒸馏成技能 | 复利链:skill 行 + 来源 issue 归属读回 |
| 5 | 真实库冒烟巡检 | copy:daily 上各 Hub 导航+截图,真实数据渲染不空壳 |

**流 1 的边界(2026-07-24 实施期发现,如实记账)**:源码核实 `Command::CreateProject` 的 `github` 恒非空,新建路径会真调 `bw_engine::github::create_repo` —— **在用户 GitHub 账号下真开仓**。常青流每跑一次就建一个垃圾仓,且依赖网络与 gh 认证,既不可接受也违背 BW 自己的铁律「破坏性永不自动」。故流 1 止步于不可逆动作之前:驱动到向导就绪、验证「开始 ↑」在位,**绝不点击**。完整建项目保持人工监督下进行,刻意不自动化 —— 与产品哲学同构。

## 5 · 执行协议与证据报告

**逐步协议**:截图→定位→动作→截图→与流文件预期对照→记 verdict(`ok/fail/skipped`)+耗时。失败如实停在原地,**绝不补拍"看起来对"的截图**,绝不跳步续跑(后续步全记 skipped)。

**报告** `report.html`:`scripts/gen-flow-report.py` 装配(python 先例:make_demo_video.py),单文件自包含(PNG base64 内嵌),设计 token 用暖纸底/clay 主色(plan/00 §6)。三态如实:**绿=全过 / 红=失败(附失败现场截图) / 灰=未跑或环境中断**——绝不假绿,与健康信号灯同一哲学。每条 verify 展示 SQL 语句、原值、预期陈述三列,读者自行对照。

### 5.1 报告不进 git(2026-07-25 拍板)

**报告是给人看的,不是给 agent 当上下文的。** 仓里的东西都会变成 agent 的上下文,而带图报告是 4MB+ 的人工终审大件——放进去只会挤占上下文、拖慢 clone,且每批一份会迅速膨胀。

去处按场景选:

| 去处 | 适用 | 代价 |
|---|---|---|
| `e2e/reports/<批次>/`(本地,gitignored) | 默认。跑完即在,随时重看 | 只在本机 |
| **PR/MR 里挂链接或附件** | 要别人评审这一批 | 需先有个可访问的 URL |
| **GitHub Release 资产**(`gh release create <tag> report.html replay.mp4`) | 要一个持久可下载的归档 | 资产不进 git 历史,但**公开仓=世界可读** |
| GitHub Pages | 想要可浏览的网页 | 文件仍要进某个分支(照样膨胀)+ 改仓设置 |

**发布前必查(公开仓尤其)**:流 05 的截图拍的是**真实日常库**——十个 Hub 里是真实的技能名、智能体名、工作流、定时任务、连接器、通知。这些一旦发到公开 Release 就是世界可读。要对外发,先出**脱敏版**(只保留 fixture 流 01-04,去掉流 05 的真实库画面)。

报告可随时用两条命令从证据目录再生,不必永久保存:`gen-flow-report.py <run-dir>` / `make-flow-frames.py` + `encode-mp4.swift`。

## 6 · 开发工作流本体(每批标准循环)

```
① 派工:票面目标 + Fable 起草验收考卷(考卷先行)
② Sonnet 实现 + 自证(门禁全过 + 按考卷自跑通过并自报)
③ Fable 对抗验收:代码 review + 亲手跑流 + sqlite 读回 → HTML 证据报告
④ 用户终审:看报告点头 = 这票 Done(Done 永远人点,与产品铁律同构)
⑤ 常青流全绿才允许合批(回归保护)
```

失败路径:③ 红 → 票打回 Sonnet 重做,报告留档;考卷本身错 → Fable 修考卷并在报告里留痕(考卷变更也是证据)。本工作流本批先在设计文档层运行,验收循环自身被 ④ 验收后,下一批写入 CLAUDE.md 成为纪律。

## 7 · 错误处理

- 流执行失败:报告红 + 现场截图,停在原地。
- computer-use 中途权限/窗口丢失:该流记灰「环境中断」,不算红也绝不算绿。
- 闸门卡死:实测证据找用户定夺,不静默换路线。
- 真实库冒烟流只读:`copy:daily` 副本上跑,原库绝不触碰。

## 8 · 工程对照表

| 命题 | 锚点 |
|---|---|
| 考卷 | `e2e/flows/core/*.toml` |
| 打包 | `scripts/bundle-desktop.sh`(dx bundle) |
| 报告生成 | `scripts/gen-flow-report.py` |
| 证据落盘 | `e2e/reports/`(gitignored)+ `iterations/evidence/`(归档进 git) |
| 深链启动 | 复用 `BW_DB/BW_OPEN/BW_PANEL/BW_HUB/BW_SEL`(main.rs 既有) |
| 点击驱动 | `BW_FLOW` 应用内注入(§2.1 转向后新增,app-desktop 侧) |
| 人眼证据 | MCP computer-use `screenshot` 拍 BW.app 真窗口 |
| 读回 | `sqlite3` 直查,CLAUDE.md 纪律 1 |

## 9 · 反蔓延(本批不做)

- 不做 CI 常驻回归(Agent 驾驶,无 headless 依赖假设)。
- 不做坐标录制/回放器(语义考卷,不是像素脚本)。
- 不自研应用内截图(除非 G-3 卡死再议,那是备选不是主线)。
- 「每票必附验收流」纪律与 CLAUDE.md 修订:下一批。
- 不写 UI 单测(维持 2026-07-17 E2E-only 纪律)。

## 10 · 本批 DoD

- [ ] 闸门三关实测过,证据入 `e2e/reports/`(G-1 窗口截图 / G-2 点击生效前后图 / G-3 落盘 PNG)
- [ ] `scripts/bundle-desktop.sh` + `e2e/flows/core/` 五条考卷 + `scripts/gen-flow-report.py`
- [ ] 五条常青流全跑,首份 `report.html` SendUserFile 交付
- [ ] 用户看报告终审通过(工作流第 ④ 步在本批自身上走通)
