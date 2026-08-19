# 09 · 实践版 MVP:以「aihot 日报」从零到一真实践行(零 mock)

> 2026-07-20 用户拍板,推翻此前"功能清单式"切法:**MVP = 一次真实践行,不是一张功能清单**。
> 以 GitHub 代码仓为信息入口;workflow 底座**选型引入 superpowers**(不自造方法论内容);
> 真实走完项目创建全流程,真实立起引领性/过程性指标;**全程零 mock,不接受任何 mock 数据**。
> 实践撞到哪堵墙,哪堵墙才是队列——队列由践行产生,不由设计推演产生。
>
> **执行者**:sonnet5(用户手动切模型)。本文自足,不读对话也能接棒。**接棒入口:§5 日程,从墙 A 开始。**
> 正文用人话;源码锚点只进「工程对照」。行号是写作日锚点,漂移以源码为准;偏差写进 commit message。

---

## 0. 践行对象(用户定的真实项目)

**aihot 日报**:一个 web 应用,解决 AI 热点圈信息过多的问题——按用户自己的关注面,每天生成一份自己的 AI 热点日报。

- 仓名建议 `aihot-daily`,GitHub **private 起步**(可后改 public)。**建仓建在用户 GitHub 账号上,建仓那一步用户必须在场确认**。
- 工作区:`BW_WORKSPACES` 根下,由创建流出生即开仓(已有能力)。
- 本机前置已核验(2026-07-20):`gh` 2.95.0 已装已登录(forcegravity1989);superpowers **未装**(安装是墙 B 的第一步)。

## 1. 践行剧本(七步;标注哪步今天可真跑、哪步撞墙)

1. **建项目 = 真在 GitHub 开仓**。本地开仓+章程写进仓已真实可跑;差"开到 GitHub 并推上去"一步。→ 墙 A
2. **superpowers 装进这个项目,登记为它的 workflow 底座**,工作台里看得见、归属本项目。→ 墙 B
3. **创建时定指标**:北极星 + 引领性 + 结果/滞后性,创建流里已能真实录入(手填带徽记);过程性指标由工作台记账自生。今天可真跑,默认值见 §3。
4. **每天从 GitHub 拉真数记一笔**,拉不到就灰色「未知」,绝不编。→ 墙 C
5. **日常派活**:建 issue → 指派 → agent 在真仓真跑 → 停「评审中」→ 人点完成。今天可真跑(链路已有)。
6. **跑活过程留档**:每次运行的对话史存下来,审活可翻,优化 workflow 拿史料说话。→ 墙 D
7. **看趋势判断正向**:指标位移 × 本周结算的活并置,人来判断是否偏离初衷。→ 墙 E

## 2. 五堵墙(+一堵后置)

### 墙 A · 仓开到 GitHub(约半天)

- **人话目标**:在工作台建项目时,仓不只开在本地,同时开到 GitHub 并把创建流的提交推上去。
- **完成标准**:创建 aihot 项目后 `gh repo view <owner>/aihot-daily` 真实存在,本地创建流提交(开仓→章程各节)已推上;`gh` 不可用/未登录时**如实降级为仅本地**并记事件,绝不假装已推。
- **工程对照**:`CreateProject` 开仓块(crates/bw-app/src/lib.rs 1586 一带,mint 逻辑在 crates/bw-engine/src/workspace.rs)后追加远端步:shell 出 `gh repo create <name> --private --source <workspace> --push`(子进程模式照 claude_cli.rs);成功→在既有 git-repo connector 上记 remote 与状态(或补一行 kind=`github-repo`),失败→事件+状态如实。**不重试建仓(非幂等),失败留给人**。

### 墙 B · superpowers 引入与登记(约 1 天)

- **人话目标**:superpowers 是一条现成的、贯穿开发周期的 workflow(头脑风暴→计划→实现→评审)。把它装进机器与项目;工作台的 aihot 组件清单里看得见它的 workflow 与技能,**归属 aihot、来源标「选型引入 · superpowers」**。方法论内容在 superpowers 里,我们只引用,不复制不改写。
- **完成标准**:① 在 aihot 工作区真实调用一次 superpowers 技能并留档;② 工作台 aihot 项目组件清单列出其 workflow/技能;③ sqlite 读回对应 workflow_spec/skill 行带 project_id=aihot 与来源尾注。
- **工程对照**:
  1. 安装:以 `claude plugin` 实际命令为准(marketplace:obra/superpowers);装到对 aihot 工作区可见的层级;命令与输出原样留档进践行日志(iterations/PRACTICE-AIHOT.md,新建)。
  2. 最小归属:`agent`/`skill`/`workflow_spec` 三表各加**可空** `project_id TEXT REFERENCES project(id)`——schema.sql + `add_column_if_missing` 双守卫(本仓踩过的坑:少一半老库直接崩),老库 `PRAGMA table_info` 读回验证。**只做加列+登记+项目内查询收窄;Hub 全局视图、复制共享等全量归属反转不在本次。**
  3. 登记:新命令(建议名 `RegisterProjectComponents { project_id }`)扫描工作区与已装插件清单(`.claude/` skills/agents + plugin 清单),按发现结果写本项目组件行;desc 尾注「选型引入 · superpowers · <日期>」。
  4. aihot 主 workflow:建一条 workflow_spec「superpowers 主流程」,phases=计划→实现→自检,各 phase prompt **显式调用对应 superpowers 技能**;aihot 的 `RunIssue` 用它。
  5. 项目内组件查询(指派下拉、技能注入、剧本选择)对 aihot 按 project_id 收窄;project_id 为 NULL 的存量行照旧全局可见,**不迁移不伪造归属**。

### 墙 C · GitHub 每日取数入账(1–1.5 天)

- **人话目标**:每天从 GitHub 拉真数记一笔:本周合并 PR 数、关闭 issue 数、提交活跃天、stars。拉到才亮灯,拉不到当天就没有记录,信号按既有规则过期降灰。
- **完成标准**:观测表出现来源为 connector 的行,数字与 `gh api` 直查一致;同日重拉不重复记;登出 gh 后当天无新观测、面板如实灰,全程无假绿。
- **工程对照**:槽位早已预留——`observation.source_kind` 含 'connector'(schema.sql:67),`metric.role` 已是 'leading'|'lagging'(schema.sql:44),**派生链一行不改**。新增:
  1. `crates/bw-engine/src/github_stats.rs` 薄采集器:shell 出 `gh api repos/{owner}/{repo}`、PR/issue 计数查询,JSON 解析照 claude_cli.rs 模式;`gh` 缺席→明确错误,不静默。
  2. 指标↔取数模板绑定走最小约定:`metric.def` 以模板 key 开头(如 `gh:merged_prs_week` / `gh:closed_issues_week` / `gh:commit_days_week` / `gh:stars`),不加新表。
  3. `cron_task` 加 `mode='pull_github'`(照当年加 create_issue 的同一先例与守卫);到点 tick → 对绑定了 gh: 模板的指标各记**每日至多一条**观测(幂等:同指标同自然日已有则跳过)→ `recompute_signals` 原样。
  4. 创建流录入的两条手填指标,在绑定 gh: 模板并出现第一条 connector 观测后,「手填」徽记按既有规则自然让位——不删旧观测,不改历史。

### 墙 D · 运行过程留档(0.5–1 天)

- **人话目标**:agent 每次跑活的对话过程(每阶段给了什么指令、回了什么)存进库,审活可翻全文,workflow 好不好拿史料说话。
- **完成标准**:跑一件活后 sqlite 能读回该次运行每阶段 prompt 与输出全文;活详情/运行史能点开看;老库打开不炸。
- **工程对照**:材料引擎已捧在手里——`RunEvent::PhaseCompleted { output }`(crates/bw-engine/src/lib.rs:80,PhaseOutput 含 final_output)。新 append-only 表 `run_phase_log(id, run_id, phase_idx, prompt, output, created_at)`(schema.sql+双守卫);bw-app 收 PhaseCompleted 事件时落一行;活详情页与运行史展开处加只读展示。**已存史料绝不改写、绝不截断**(截断只发生在喂下一阶段 prompt 的既有逻辑里,与留档无关)。

### 墙 E · 趋势 × 活 并置(约 1 天)

- **人话目标**:周复盘卡加一栏:本周每条指标动了多少 × 本周结算了哪些活,摆在一起。**只并置事实,不编因果**——是否正向推进、有没有偏离初衷,人看人判。
- **完成标准**:该栏每个数字 sqlite 直查可对上(指标位移=本周首末观测差;活清单=本周 settled 行);无数据显示「未知」,不显示 0。
- **工程对照**:周复盘卡已在进度面板顶部(纯函数读回,2026-07-17 落地);在其 selector/vm 扩一栏,数据全部来自既有表(observation 差值 + issue settled_at 窗口筛选),**零新记录、零新写入路径**。

### 墙 F(后置,不在本周)· 创建流真起草

创建流中「按方法论起草」那步今天是演示替身。**本次践行替身不出场**:你的真实回答经既有路径直接写入章程(P1 已实现)。若要 agent 真起草:项目出生即真仓,执行器会自动走真——等网关稳定窗口再开这堵墙。

## 3. 建议指标(创建流里用户真答,可改;这只是默认草案)

| 类 | 指标 | 口径 | 数据来源 |
|---|---|---|---|
| 北极星 | 日报连续按时产出天数 | 日报文件按日入仓的连续天数 | 仓内真实文件(git 可数) |
| 引领性 | 本周合并 PR 数 ≥2;本周提交活跃天 ≥4 | gh:merged_prs_week / gh:commit_days_week | 墙 C 落地前手填带徽记,落地后 GitHub 真取 |
| 过程性 | 本周结算活数;跑活成功率 | 工作台记账自生 | 已自动真实入账,不用填 |
| 结果/滞后 | stars;订阅/读者数 | gh:stars;订阅数无真实来源前**手填带徽记** | GitHub / 手填 |

## 4. 建议首批活(工作台建卡,经「superpowers 主流程」跑;人审 Done;此为草单,内容以用户确认为准)

1. **技术选型与骨架**:用 superpowers 的头脑风暴+计划技能产出选型决定(建议方向:静态生成或轻后端,先不做账号体系),落可运行骨架。
2. **信息源接入第一批**:3–5 个 RSS/API,源清单为仓内配置文件(它本身就是「接入源数」这个未来指标的真实底账)。
3. **抓取→筛选→生成日报 md 的最小管线**:每天一份,入仓。
4. **日报页面渲染 + 本地跑通**:README 记启动方式。
5. (第二周)按用户关注面做筛选规则第一版。

## 5. 日程(交错拆墙与践行;sonnet5 执行)

| 日 | 拆墙 | 践行 |
|---|---|---|
| D1 | 上午 墙 A;下午 墙 B | 中午**用户在场**:真走创建流——建仓到 GitHub(确认账号)、真答北极星与指标 |
| D2 | 墙 D(先落:史料从第一件活就开始攒) | 建首批活,#1 真跑、人审 |
| D3 | 墙 C(取数入账+挂每日 cron) | #2/#3 跑;指标开始由真数点亮 |
| D4 | 墙 E | 首次真实周复盘:趋势×活并置 |

每墙一个独立 commit,代号前缀(`墙A · …`);取舍与偏差如实写进 commit message。

## 6. 执行纪律(不可妥协;与 CLAUDE.md 一致,此处按本践行重申)

1. **零 mock 条款**:aihot 项目里的一切数字来自 GitHub、工作台记账或真仓文件;演示替身不出场;凡带【mock】标注的路径不允许出现在 aihot 的数据里。
2. **aihot 的开发不许绕过工作台手写**:一律 建活→指派→真跑→人审。绕过=践行作废,这是本计划的全部意义。
3. **Done 永不自动,破坏性永不自动**(产品铁律,原样)。
4. schema 每加一列:schema.sql + `add_column_if_missing` 双守卫,老库 `PRAGMA` 读回。
5. 门禁每 commit 前全过(fmt / clippy / 两个 wasm32 check / guard-kernel-ui-free / app-desktop check,照 CLAUDE.md §常用命令)。
6. 验证=E2E 读回:`BW_OPEN=<项目名> BW_PANEL=…` 深链 stderr 证渲染;数字一律 `sqlite3` 直查;GitHub 侧 `gh api` 直查对数。
7. 网关 529 抖动:幂等重试(照 scripts/supervise-real-demo.sh 模式),**绝不因抖动改走 mock**。
8. 践行日志:`iterations/PRACTICE-AIHOT.md` 逐日如实记(干了什么、真输出摘录、撞到的新墙);新墙进本文件 §2 末尾追加,不擅自扩scope。

## 7. 第一周验收(全部读回,不看口头汇报)

- `gh repo view` aihot-daily 真实存在;PROJECT.md 章程在仓里,内容=用户真实回答。
- sqlite:aihot 组件清单含 superpowers 来源行且 project_id 归属正确;metric 的 role 正确;observation 含 connector 来源行并与 `gh api` 对数;run_phase_log 有真史料;结算活 settled_at 在。
- 周复盘卡:指标位移 × 结算活并置,每个数字可直查对上。
- 全库无一处 mock 标注出现在 aihot 项目的数据里。
