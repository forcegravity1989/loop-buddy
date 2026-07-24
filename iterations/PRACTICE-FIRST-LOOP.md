# 践行日志 · 首次全真闭环(2026-07-24)

> plan/08 ② 的执行记录:三流合并后的 main 上,第一次把管理闭环从「建项目」
> 一路真转到「报告 PR 等人 merge」。零 mock;所有结论可用文末查询独立复核。
> 指挥器:`crates/bw-app/examples/practice_first_loop.rs`(本批新增)。

## 践行对象(不造假题)

**「个人画像」**——用户本人 2026-07-24 上午 10:45 用创建流亲手真开的项目
(注意力画像推荐系统,机会栏引用 aihot 日报 99% 接受度真实基线),GitHub 仓
`forcegravity1989/project` 当时已真建、章程已起草,但流程停在 `cold_start`。
**停住的直接原因**(库里可查):10:46 的 `创建 · 体系起草` run 死于
`error_max_budget_usd ($0.5)`——旧二进制的起草走了真执行器,正是 plan/14
抓到、C13 已修的根因。本次践行 = 在修好的 main 上把用户中断的创建流续完。

## 时间线(全部真实发生)

| 步骤 | 结果 | 证据 |
|---|---|---|
| `complete` 续流落地 | 标配三件套真开 GitHub issue #1/#2/#3;11 个章程提交推上远端;项目转 Running | `gh issue list` OPEN×3;`git log origin/main` |
| `run 1` 第 1 跑($0.3) | **诚实失败**:107s 预算腰斩,phases=0,票停 InProgress 可重试 | run 行 err=`error_max_budget_usd ($0.3)` |
| `run 1` 第 2 跑($0.75) | **诚实失败**:533s 预算腰斩;但工作区里 agent 已产出 evidence/insights/hypothesis/prototype 并自行 commit | run 行 + `bw/issue-1` 提交历史 |
| 路径 B 喂料 | 检索被拒(见 F3),按技能降级路:真实检索的对标材料(带来源+核实状态)入 `docs/inbox/` | 提交 `8095ee9` |
| `run 1` 第 3 跑($0.75) | phases=5 全过(368s),报告「路径 B 人工材料整理版」落盘;**提 PR 失败,错误文本为空**(F5) | run Ok + toast 原文 |
| F5 修复后第 4 跑 | phases=5(189s)→ **真 PR #4 开出(`Closes #1`)**,票转 InReview | `gh pr view 4`;issue 行 pr#=4 |

现状:**PR #4 OPEN,等人 merge 验收**;#2 找指标、#3 绑数据仍 Backlog(次序
即依赖序:找指标要读 merge 进主干的 `docs/competitive-analysis.md`)。

## 摩擦台账(喂下一轮 grilling 的原料,按发现顺序)

- **F1 · phase 成本口径 ≠ 探针口径**。探针($0.30,单轮检索问答实测 ~$0.15)
  的绿灯不能外推到剧本 phase:全量注入(剧本+票+标配 Skill 全文+复利技能)的
  phase,$0.30 一个都撑不起。$0.75(real_demo 口径)可过。
- **F2 · phase 边界失守**。票简介「请用本阶段方法论完成它」+ 技能全文,让
  phase 1 的一个 call 把 证据→洞察→假设→原型 全干了(含真写真编译的 Rust
  原型 `aihot-tracker`),引擎却记 phases=0——干的活与记的账脱节;预算在
  「一个 call 干五个 phase 的活」下必然腰斩。修法要 grilling:是收 prompt
  (只许干本 phase)还是认账(把 call 内的真实产出记进多个 phase)。
- **F3 · `allow_commands=1` ⇒ `--allowedTools Bash` ⇒ WebSearch 被拒**。
  白名单一旦存在就是收窄语义,检索类工具全被挡;探针没传白名单所以是绿的
  ——「探活通 ≠ 执行器通」。证据:run 快照 `params_json.allowed_tools_arg
  ="Bash"`;agent 两轮都如实报「权限未被授予」并拒绝编造对标事实(技能硬
  约束真的守住了,这是本次践行最硬的产品验证之一)。
- **F4 · 创建流遗留**:仓 slug 掉到默认值 `project`(中文项目名没有可用
  slug 派生);PROJECT.md 里北极星已起草但 DB `north_star` 字段为空(章程
  写盘与字段落库两条路没对齐)。
- **F5 · open_pr 幂等误判(本批已修,commit `aabc989`)**。执行器自己
  commit 过后树是干净的,git 把 "nothing to commit" 打在 stdout,原实现只查
  stderr → 幂等情形被判失败且错误文本为空,PR 环整段卡死。
- **F6 · 真实花费不入账**。`claude` CLI 返回 `total_cost_usd`,引擎丢弃;
  `workflow_run` 无成本列。四次 run 的真实总花费只能估上界(≤ $0.3+$0.75×3
  = $2.55,实际低于此)。对「真实 telemetry 难造假」而言,钱是最该记的
  telemetry 之一。
- **F7 · 复利记账口径**:`competitive-analysis.uses=4`——预算腰斩的失败
  run 也各记了一次「用」。注入确实发生过,但「用了没用成」要不要计入胜率
  口径,待 grilling。

## 交接(下一棒从这里接)

1. **人手验收**(用户,勿在网页 merge——C11 漂移采集未建):
   `practice_first_loop <db> <ws> 个人画像 merge 1`,或桌面 UI 里点 merge。
2. merge 后:`run 2`(找指标,产出 `.bw/metrics.toml` 正本)→ 人 merge →
   `run 3`(绑数据)→ 采集器点亮看板(plan/08 ② 后半)。
3. 债务池新增:F2/F3/F6/F7 待 grilling 立票;F4 归创建流回显健壮性一族
   (C17 #55 旁边);C11 #42 优先级因本次践行上涨(验收动作真实发生了)。

## 独立复核

```bash
DB="$HOME/Library/Application Support/BuildersWorkbench/workbench.db"
sqlite3 "$DB" "SELECT number,title,status,github_number,pr_number FROM issue \
  WHERE project_id='7b190570-f43d-4231-b8d1-7337c3e628df';"
sqlite3 "$DB" "SELECT status,phases_completed,duration_ms,error FROM workflow_run \
  WHERE workflow_name LIKE '#1%' ORDER BY started_at;"
sqlite3 "$DB" "SELECT name,uses FROM skill WHERE name='competitive-analysis';"
gh pr view 4 --repo forcegravity1989/project --json state,title,files
```
