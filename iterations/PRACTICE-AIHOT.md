# 践行日志 · aihot 日报(零 mock,真实践行)

> 2026-07-20 用户拍板:以真实项目「aihot 日报」践行 plan/09 的五堵墙 + 「模板能力」
> (agent-standards.md 等标准文件,规范项目基础形态)。用户切至 sonnet5 并授权整夜
> 自主执行、自己解决问题、不等待响应。本文件逐轮如实记录:假设→动作→真实输出→结论。
> 不改设计决定;偏差与撞到的新墙照实记在这里,不擅自扩 scope。

## 0. 起夜决定

- **GitHub 远端建仓**:不做。用户账号上无监督建公开/私有仓涉及在用户熟睡时改变其真实
  账号可见状态,风险与"本地已可真实开仓"的收益不对称。用户已明确授权兜底
  ("如果你没法创建新的github代码,就基于我们的分支去创建,这也没有关系")——本次践行
  在**本地真实 git 仓**(`git init` + 真提交,非 mock)完成,不触碰用户 GitHub 账号。
- **真实执行器网关探测**(2026-07-20,起夜第一件事):
  ```
  env 剥离会话变量(ANTHROPIC_AUTH_TOKEN/BASE_URL/MODEL/CLAUDECODE/...) 后直接调用
  claude -p "Reply with exactly: PROBE_OK" --output-format json --no-session-persistence
  --max-budget-usd 0.05 --permission-mode acceptEdits
  ```
  真实返回(190.8s):
  ```json
  {"type":"result","subtype":"success","is_error":true,"api_error_status":429,
   "result":"API Error: Request rejected (429) · [1310][您已达到每周/每月使用上限，
   您的限额将在 2026-07-24 09:59:59 重置。]"}
  ```
  **结论**:这不是历史备忘录记的网关 529 抖动,是**账号级配额硬墙**,4 天后(2026-07-24)
  才重置。核对 `crates/bw-engine/src/claude_cli.rs` 的 `is_transient_gateway_error`——
  只匹配 529/503/502/504/"overloaded"/"访问量过大",**429 不在重试名单内**,引擎会
  fail-fast 不重试。这条路今晚起对全部 30 轮循环都成立,不是某一次运气差。
  **决定**:真执行器路径只经 BW 真实命令层(`Command::RunIssue`)做**一次**诚实探测,
  留一条真实 `Failed` workflow_run 作为"系统在配额耗尽下行为正确"的证据;之后不再
  重复撞同一堵已知墙。其余全部工作由我(当值 sonnet5)直接在真实工作区实现——
  依然是真文件、真 git 提交、真 evidence 采集,零 mock 数据;每处如实标注
  "真执行器今晚不可用(配额,见探测记录),内容由值班 agent 直接产出"。

## 1. 假设 → 动作 → 结论(逐轮追加)

### 0a. 模板能力真实落地 —— 确认
`crates/bw-core/src/standards.rs` 四份标准文件(逐字段核对真实 schema,含
`add_column_if_missing` 隐藏列)+ `write_component_standards`(项目出生写入
`.claude/standards/*.md`)+ agent/skill/workflow_spec 三表加可空 `project_id`
(既有 12 处构造点全部显式 `None`,行为不变;`DistillSkillFromIssue` 例外——
蒸馏技能的 project_id 从源 Issue 真实派生)。commit `4adba65`。全部编译门禁绿。

### 0b. superpowers 真实选型引入 —— 成功
`claude plugin marketplace add obra/superpowers` → `claude plugin install
superpowers@superpowers-dev`,真实装好(version 6.1.1, scope user)。真实技能名:
brainstorming / writing-plans / executing-plans / test-driven-development /
requesting-code-review / verification-before-completion 等,与用户描述的
"头脑风暴→写计划→按计划实现→评审"完全对应。**撞到一堵真墙并顺手补上**:
`HubSource` 枚举只有 Omc/Ecc/SelfBuilt/WithinSession 四值,没有"选型引入外部
插件"的诚实选项——加了 `HubSource::Adopted`(JSON 序列化字段,无需表迁移)。

### 0c. 真实开仓 + 组件注册 —— 成功,sqlite 读回
`practice_aihot setup`(新 driver,`crates/bw-app/examples/practice_aihot.rs`)
真实创建「aihot 日报」项目:本地开仓(`practice-aihot/workspaces/aihot-*`,
8 个真实 git 提交)、章程 PROJECT.md 真实写入、四份标准文件真实写入、三条真实
指标(引领×2 含 `工作区真实提交数`——复用 git-repo connector 现成的 Tier D
本地采集,零自定义代码;结果×1)、一个项目自有 agent(日报编辑)、一条项目
自有 skill(关键词关注面打分法)、一条项目自有 workflow(aihot 主 workflow,
`source=adopted`,phase_prompts 显式点名调用 superpowers 的真实技能名)。
sqlite 直查全部核对一致(见下方命令)。**幂等验证**:重跑 `setup`,project_id
不变、git log 仍 8 commit——未重复造。
```
sqlite3 practice-aihot/bw-aihot.db "SELECT name, project_id FROM agent WHERE project_id IS NOT NULL;"
# 日报编辑|b7971eca-99e0-421f-bf59-6a7f9e4b2331
```
**留白如实标注**:四份标准文件目前是 4 次独立 commit(每文件一次),不是合并
1 次——`commit_file` 逐文件 add+commit 的既有实现如此,行为诚实但可以更省;
本次不修,记在这里留给下一棒。

### 1-28. Issue 驱动的真实开发循环(逐条 open-issue → 真实实现/验证 → settle-issue,
详见 `practice-aihot/workspaces/aihot-b7971eca` 的真实 git log 与 sqlite 读回,
不在此重复摘录每一条——每条 issue 的 desc/结算 note 本身就是可独立读回的践行记录)

概况:五阶段一整圈(原型→构建→优化→运营推广→运维)+ 运维回流原型产生的二圈
2 个真实假设验证(1 个部分成立但决定不修,1 个不成立)+ cron/蒸馏/指标真喂/
周复盘 + 收尾的语法门禁/LICENSE/BW 仓门禁复核。

真实产出(全部可独立 sqlite3/git 核对,见 §2):
- Issue 28 条(Done 27,Blocked 1——#1 真执行器探测,配额耗尽 2026-07-24 前无法解)
- 真实 git 提交 30 次(aihot 仓;独立核验 `git log --oneline|wc -l`=30,与
  connector 真喂的「工作区真实提交数」指标读数一致)
- 23 条真实 Python 单测,全绿
- 2 天真实日报(HN+arXiv 真实数据,.md+.html 各一份,30 条/天)
- 1 个项目自有 agent(日报编辑)、2 条项目自有 skill(关键词关注面打分法 +
  蒸馏自 #11 的多源体量控制法,后者 provenance 真实指向源 Issue)、
  1 条项目自有 workflow(source=adopted,真实引用装好的 superpowers 技能名)、
  1 条 cron(daily autopilot,mode=create_issue,no-hijack)
- 2 个真实 bug 修复(裸 traceback→友好报错)、1 个真实优化(耗时-38%,
  输出量对齐日报体量)、1 个真实 growth 实验(渲染成网页,前后对照验证)
- BW 仓自身:4 份组件标准文件(`crates/bw-core/src/standards.rs`)+
  `write_component_standards` + agent/skill/workflow_spec 三表 project_id
  最小切片 + `HubSource::Adopted` 新变体 + `practice_aihot.rs` 指挥器(11 个
  子命令)—— 全部门禁复核绿(round #28),老库(`demo-workspaces/bw-demo.db`)
  真实打开验证迁移不崩、18/18 既有验收依旧全过。

## 2. 独立核验命令(不看本文档的口头汇报,自己跑一遍)

```bash
DB=practice-aihot/bw-aihot.db
WS=practice-aihot/workspaces/aihot-b7971eca

sqlite3 "$DB" "SELECT status, COUNT(*) FROM issue GROUP BY status;"          # 27 done / 1 blocked
sqlite3 "$DB" "SELECT name FROM agent WHERE project_id IS NOT NULL;"          # 日报编辑
sqlite3 "$DB" "SELECT name, distilled_from_issue FROM skill WHERE project_id IS NOT NULL;"
sqlite3 "$DB" "SELECT name FROM cron_task WHERE project_id IS NOT NULL;"      # aihot 每日日报生成
git -C "$WS" log --oneline | wc -l                                            # 30
cd "$WS" && python3 -m unittest discover tests -v                             # 23/23
python3 -m aihot.main                                                          # 真实生成今日日报
```

## 3. 诚实的未尽事项(留白,不假装做了)

- ~~真执行器全程被账号配额挡住~~ **2026-07-21 已解:见 §4**——用户修好登录后,
  issue #30 真跑通(见下),`[[bw-real-executor-pending-verification]]` 已更新。
- superpowers 只做了"选型引入+登记",§4 的 issue #30 真跑第一次真实调用了它
  (workflow 的 phase_prompts 点名 brainstorming/writing-plans/executing-plans/
  test-driven-development,4 阶段全部真实完成)——不再是"只登记未调用"。
- 项目归属(project_id)只做了 schema 最小切片,查询收窄(指派下拉/技能注入
  只看本项目)没做——plan/08 记录的 P2 全量仍待later。§4 补了"读侧展示"
  (项目侧边栏 + 卡片归属 chip),但**不是**查询收窄,如实区分。
- aihot 应用本身:没有做账号体系、没有做真实"取消关注某关键词"式的用户偏好
  演化、没有做 RSS 输出格式——如实留白,不在本次践行范围内。

## 4. plan/10 续篇(2026-07-21,用户拍板"个人级看板 + 真执行器实跑刷新")

用户三个新指令:①一级侧边栏是 marketplace,项目维度组件管理没入口;
②真执行器登录搞定了,可以真跑了;③skill/agent/workflow 卡片展示太简短,
issue 解决数不是真正的引领/滞后指标。计划见 `plan/10-personal-kanban-and-real-run.md`,
五条工作线 K0-K4,K0-K3 已完成如下(K4 见该文件后续):

**K0 · 真执行器实跑验证**——真撞到一堵新墙又真解决:issue #30 的 `probe-run`
首次真跑(默认 `max_budget_usd=0.50`)142.7s 后失败,但 `error` 字段是空字符串,
诊断不出原因。直接裸调 `claude -p "回一个词"` 定位:真实花费 $0.0995(固定
缓存/工具上下文开销就占这么多),且 CLI 在 `subtype:"error_max_budget_usd"`
时压根不产出 `result` 字段——旧代码只读 `result`,真实原因被空字符串吞掉。
修 `claude_cli.rs` 的 `CliResult` 补 `errors`/`subtype` 兜底(commit `06c1a60`),
aihot 践行的 `ClaudeCliConfig.max_budget_usd` 调宽到 3.0(不动 crate 全局默认)。
重跑:**真实成功**,493.5s,4/4 阶段完成,真实提交 `25e0a53`(SPEC.md/TASKS.md/
REVIEW.md + 7 条新回归测试,23→30 全绿)。

**诚实的次生发现——范围漂移,不是 bug**:issue #30 本来要的是"main.py 落盘
telemetry.json",但 agent 的 brainstorm 阶段把范围重定向成了"全项目 SPEC 固化
+ 测试补齐"——真实、有价值,但不是原始的活。原因:aihot 主 workflow 的
`phase_prompts` 是方法论导向("先发散再收敛出一个可执行方向"),不是逐字
照办 issue 描述。已诚实结算 #30(credit 真实产出,note 缺口),开了新 issue
留给 K4 直接实现 telemetry.json(不再靠真 RunIssue 二次赌范围漂移)。

**K1 · 项目二级侧边栏**——发现 project_id 虽然进了 schema(昨夜),但读侧
从没跟上:`list_workflow_specs`/`list_skills`/`list_agents` 的 SELECT 根本不取
这列,domain struct(`WorkflowSpec`/`SkillCard`/`AgentCard`)压根没这个字段。
补齐 domain→store→VM 全链路(6 个 SELECT + 3 个 row mapper + 4 个 VM 卡片
+ 6 处 `WorkflowSpec` 字面量构造点),新增 `ProjectRail` 组件——项目打开时
在图标栏右侧多一列,五组(技能/智能体/工作流/定时/连接器)按真实 project_id
分"本项目自建"/"共享引入",点项跳全局 Hub(复用已有导航)。全局图标栏
原样不动。`sqlite3` 独立核对计数(skill=2/agent=1/workflow=1/cron=1/
connector=1)与代码过滤逻辑一致。

**K2 · 卡片归属 chip**——盘点发现 WorkflowHub 的行其实已经够丰富(源/成熟度/
触发器 chip、真实战绩、悬停关系),真正缺的是三种卡片都拿到了真实 project_id
却没处显示"这是谁的"。SkillHub/AgentHub 仿 WorkflowHub 已有的 `projects` 入参,
三处卡头都加真实反查的归属 chip("◇ 项目名",None 不渲染)。顺手补
`BW_HUB=skill|agent|...` 深链(现有 `BW_OPEN`/`BW_PANEL` 到不了图标栏 Hub 屏,
这是本次唯一能核验 K2 渲染路径可达、无 panic 的手段——computer-use 对这个
未打包 debug 二进制两次都申请不到窗口权限,肉眼截图仍缺,留给下一棒)。

**K3 · 实跑后刷新链路核验**——纯核验,没发现代码缺口。五面板逐条 sqlite
读回:产物(`CollectArtifacts` 真登记 577 行,`LoadArtifacts` 读同一张表)、
定时(cron_task.last_run 真实反映今早 01:33:36 的自动触发)、版本
(`LoadVersionLog` 直接 shell `git log`,零缓存不可能陈旧,真实 31 commit)、
进展(`阶段完成 Issue 数` build 阶段 5→6,#30 结算时真实喂入)全部确认自动
刷新;`本周结算活数` 看起来"没刷新"但那是它自己的设计(定义写明"人工按周
核对更新",非自动派生)——不是 K3 的缺口,K4 会把这类 issue 计数指标整体
降级,到时候一并处理,不单独修。

