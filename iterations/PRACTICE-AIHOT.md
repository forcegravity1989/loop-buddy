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

（后续每完成一件事在下面追加一条,格式:`### N. 假设 —— 结论`）
