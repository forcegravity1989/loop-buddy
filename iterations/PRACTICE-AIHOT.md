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

（后续每完成一件事在下面追加一条,格式:`### N. 假设 —— 结论`）
