# 融合收口轮报告(fable5 · 2026-07-15)

> 接棒 glm5.2(`iterations/TAKEOVER-REPORT-GLM52.md`)。本轮目标(用户原话):
> 「继续完成整体的方案设计对齐,multica 和 workbench 原型做有机结合,一切内容都是实跑。」
> 每条陈述可对源码 / commit / demo DB 核验。

## 1. 一句话结果

**两条平行分支(九实体主线 × multica Issue 融合)已焊成一体:代码合并全绿(167 测试)、
Done 边沿记账联动补上了「队友干活→证据落账」的最后一道焊缝、plan/06 把两份设计收敛为
单一方案;演示库真实落账并经桌面深链核验。**

## 2. 本轮四个动作(全部可核验)

| 动作 | 内容 | 证据 |
|---|---|---|
| 合并 | `claude/bw-complete-form`(R1 Issue 层 + R2 技能复利 + Issue 看板)并入主线;8 文件冲突消解(导入并集 / skill SELECT 三列并集 / 区块两侧共存);**语义交汇点当场焊接**:蒸馏技能必须带 `content` 正文(R2×G7),CreateAgent 演示补 instructions(G8),BW_PANEL 深链纳入 `issues` | commit `c723480` |
| 记账联动(R3) | `TransitionIssue` 的真实 …→Done 边沿 = issue 侧 settle:assignee 按名记 runs/wins(win_rate 同语句派生)+ 工作区产物按 issue 阶段幂等登记;重复 Done 不重计;**Cancelled 拒记**(弃活不是 agent 表现证据,拒造损失) | commit `fc5bded` + 测试 `issue_done_edge_settles_agent_accounting_exactly_once` |
| 方案对齐 | `plan/06-overall-alignment.md`:十实体一张图、两条 settle 路径汇一组记账函数、统一 IA(实景)、缺口台账合一(G1-G11 + R1/R2/R3)、实跑纪律五条;plan/05 与 V2-DESIGN 加谱系注 | commit(docs) |
| 实跑落账 | ① `real_team_loop`(临时库):4 Issue Done → **R3 读回 Fable runs=1 · sonnet5 runs=3,win_rate=100%**(合并前此处恒 0);② `record_fusion_round`(新指挥器,幂等):把**本轮真实工作**记进 `bw-demo.db`——新项目真实开仓(git 仓 `8f30203`)、4 Issue(证据=真 commit)、蒸馏「同源双分支合并消解法」技能(274 字正文+溯源) | 例子源码 + 下方 DB 审计 |

## 3. bw-demo.db 十实体审计(真实读回,2026-07-15)

```
projects=2 (linkcheck-md · 完整形态融合)      issues=4 (done=4)
skills=350 (content=6 · provenance=1)         agents=109 (with_runs=3: 原型师1/构建师3/优化师1)
workflow_specs=97   workflow_runs=5 (ok=1/failed=2/stale-running=2, 529 时代如实遗留)
artifacts=5 (linkcheck 3 @f4f8ed3 + 新工作区 2 @8f30203)
connectors=3 (claude-cli + git-repo×2)        cron=0   observations=3   handoffs=1
```

## 4. 桌面核验(深链实跑)

```
[BW_OPEN] "Builders' Workbench · 完整形态(multica 融合)"
          -> view=App panel=Issues projects=2 issues=4
```
合并后的桌面二进制加载真实 DB、直达 Issue 看板、快照含 4 条真实 Issue——内核→存储→VM
链路端到端验证。**像素级新截图受本会话屏幕录制权限降级所限**(截屏只见壁纸+菜单栏,
任何应用窗口均不可见;`screencapture -l`/`CGWindowListCreateImage` 返回拒绝;裸二进制
无 bundle,computer-use 授权通道无法解析)。看板渲染代码在合并中未改动(op.rs 自动合并),
其真实像素证据为融合分支实跑截图 `docs/board-issues.png`。恢复路径:给运行环境授予
屏幕录制权限,或在 Terminal 会话重跑 `BW_OPEN=… BW_PANEL=issues` + `screencapture`。

## 5. 网关状态变化(G4 外部制约,如实更新)

- 2026-07-14(glm5.2):**529** 模型过载——token 有效,服务过载,值得重试。
- 2026-07-15(本轮直探 `POST /v1/messages`):**401 Invalid bearer token**——token 已失效/
  被撤销。**重试无意义**;`supervise-real-demo.sh` 会快速失败在 401 上。
- 需要用户侧动作:更新网关 token(`~/.claude/settings.json` env `ANTHROPIC_AUTH_TOKEN`)。
  恢复后 app 自带 `claude -p` 环走同一 settle,G4 自然闭合。

## 6. 诚实边界(未做/未变)

- 聊天回复仍【mock】标注;知识库仍登记表;cron 表达式不支持;Autopilot(cron→自动建
  Issue)只在 plan/06 列为下一步——均不假装。
- 桌面像素截图见 §4 说明;两个 HTML 报告(根目录)是历史轮次产物,本轮未重生成。
- `record_fusion_round` 的 Issue 是「把真实完成的工作如实登记」,不是 app 内生执行;
  实际执行者(fable5 本会话)在每条 desc 里署名——与 glm5.2 编排后端先例同一纪律。
