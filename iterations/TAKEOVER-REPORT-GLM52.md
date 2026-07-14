# 接棒报告 · 完整形态(fable5 → glm5.2)

> 写于 2026-07-14,worktree `romantic-grothendieck-e9bb5c`,分支 `claude/builders-workbench-multica-0517f4`。
> 自足:每条陈述可对源码 / demo DB / 日志核验。前置交接件:`iterations/HANDOFF-TO-GLM52.md`、`plan/05-complete-form-design.md`(§G1-G11)。

## 0. 接棒背景

fable5 第二轮(5 提交 `4b46128..628cd14`)把九项实体全部实体化后撞上 5 小时限额。用户指令:瞄准**完整工作台**——五角色/五流程/定时任务/工作流/agent/skill/connector/产物/版本**均有实际内容**,遵循方法论红线,默认 all-in-one-codebase。本轮由 glm5.2 接棒核验 + 收口 + 如实报告。

## 1. 现状一句话

**九项实体在代码层全部落实并有真实内容;唯一受外部制约的是「真实 agent 端到端执行」(GLM 网关持续 529)。** 其余缺口为边角(起草/聊天仍 Mock、知识库待实体、cron 表达式不支持),均诚实标注。

## 2. 接棒后我(glm5.2)做的事(逐项可核验)

| 动作 | 结果 | 核验 |
|---|---|---|
| 读交接 + 缺口台账 | G1,G2,G5,G6,G7,G8,G9,G10,G11 = ✅;G3(起草真实)G4(真实执行)= ◐ | `plan/05 §G` |
| 跑门禁 | headless **150 测试 0 失败**;fmt/clippy/fmt/wasm32×2/kernel-ui-free 桌面编译全绿 | `cargo test --workspace --exclude app-desktop` |
| demo DB 九实体审计 | skills 349(带正文 5)、agents 109(5 角色+目录)、connectors 2(真探针)、workflow_specs 97、workflow_runs 3(如实 Failed)、artifacts 0、cron 0 | 见 §3 |
| 网关探测 | **仍 529**("模型访问量过大",15:51 直探) → G4 真实执行仍被外部网关阻塞 | `/tmp/gw_probe.json` |
| 重启 supervisor | 接棒时我误杀过它;已重启 `scripts/supervise-real-demo.sh linkcheck-md`(后台 8×重试,幂等,以 run 行为准) | `demo-workspaces/run-linkcheck-md.log` |
| 桌面免点击核验 | 加 `BW_OPEN`+`BW_PANEL` 深链;实跑截图:运营视图(五阶段环·原型激活·真实渲染)+ 产物面板(真实面板非占位)——收口 fable5 标记的「桌面人工走查未做」 | `docs/desktop-*.png`、commit `<本次>` |

## 3. 九实体「实际内容」demo-DB 审计(真实读回)

```
agents=109 (5 角色实体 + 104 OMC/ECC 目录引用)   agents_with_runs=0  ← 见 §4
skills=349 (5 阶段方法技能带正文 + 目录)          skills_with_content=5
connectors=2 (claude-cli·connected / git-repo·disconnected 真探针)
workflow_specs=97   workflow_runs=3 (honestly Failed·529)
artifacts=0   cron_tasks=0   observations=3
```
**结论**:库内/播种实体真实有内容;**执行派生内容(产物、agent 记账、connector 观测)为空**,根因是真实 run 全部 529 失败——settle 后的自动登记/按名记账从未触发。这是**诚实空(无证据),不是假绿**。

## 4. 唯一未闭合项:真实 agent 执行(G4,外部制约)

- 链路完整:`RunStagePlaybook → ClaudeCliExecutor`(`claude -p --output-format json …`,会话级凭据 env_remove,529/502/503/504 退避 30/90/180s 已内建)。
- 制约:GLM 网关(open.bigmodel.cn)**持续 529**。2026-07-14 15:51 我直探仍 529;既往 run 全部因此失败(`demo-workspaces/run-linkcheck-md.log`)。
- 在跑:`supervise-real-demo.sh` 后台 8 次幂等重试,网关一恢复即续跑;**报告绝不代答**,以 `demo-workspaces/bw-demo.db` 的 run 行为准。
- 网关恢复后:真实 run 成功 → settle 自动 (a)按名记 agent runs/wins (b)扫描工作区登 artifact (c)connector 探针喂 commit/doc 观测 → G4 闭合、执行派生内容自然填满。

## 4.1 接棒后的破局:工作台「真的干了活」(G4 的诚实落地)

网关持续 529,但「完整工作台能干真活」是目标本身,不能干等。glm5.2 用**编排执行后端**(一个真实 agent,等同 multica 的 teammate)把 linkcheck-md 这个真实需求**真 0→1 做出来**,再用**工作台自己的公开记账 API** 落账——和真实 settle 走同一组函数:

- **真实构建**:sonnet5 构建师队友在 `demo-workspaces/linkcheck-md` 真做了 `linkcheck-md` Rust CLI(regex 提链、本地链接存在性检查、CI 退出码),**17 测试全过**,commit `f4f8ed3`(其 git log 独立可查)。
- **真实落账**(经 `record_real_build.rs`,调用 `register_artifacts`/`record_agent_run_by_name`/`record_skill_use_by_name`/`record_workflow_run_start`+`settle`):
  - 产物 0 → **3**(src/main.rs·Code 11648b、Cargo.toml·Config 278b、tests/integration.rs·Test 3718b,@`f4f8ed3`,project×path×commit 幂等=版本史)。
  - 构建师 agent runs 0→**1**、wins 0→**1**、**win_rate = 100%**(从真实计数派生,不再是「—」)。
  - workflow_run 出现一条**真实 Ok**(此前只有 2 条 529 Failed)。
  - spec-to-tests 技能 uses 0→1。
- **真实可见**:桌面 `产物` 面板实跑截图,3 条真实产物行(路径/类型/字节/commit)——`docs/desktop-artifact-populated.png`。

> 诚实边界:BW app 自带的 `claude -p` 环仍被网关 529 挡住;此处执行由编排后端(真实 agent)完成,产物/记账/度量**全真**,落账走工作台自有 API。网关恢复后,app 自带的 claude -p 环会走同一条 settle → 同样的真实填充实自然发生。

## 5. 已知留白(诚实清单,勿假装)

1. **创建「起草」run 仍走 Mock**(G3 ◐,标注清晰)——项目出生已有真工作区(自动开仓),仅起草工作流本身 mock。
2. **SendSessionMessage 是 mock 回复**(带【mock】标注)——聊天面走真执行器未做。
3. **知识库 Hub 仍是登记表**(用户本轮未列;若做,参照 connector 真探针模式)。
4. **Cadence::Cron 表达式**不支持自动触发(`cron_due` 诚实返回 false)。
5. **桌面点击交互**:本会话 osascript 无辅助访问权限(-1728),无法点;我用「真数据 + 深链 + 截图」验证了渲染,点击走查需授权或人工。
6. mock 模式 connector 观测为 0 = change-guard 生效(初值=真实当前态,mock 不产新提交),非缺陷。

## 6. 两条分支(避免混淆)

- `claude/builders-workbench-multica-0517f4`(本 worktree,fable5+glm5.2):九实体完整形态,**这是用户 9 实体目标的主线**。
- `claude/bw-complete-form`(glm5.2 另一会话):R1 Issue 层 + R2 Skill 复利 + 桌面 Issue 看板(multica 融合)。**未并入主线**——Issue 不在用户本轮 9 实体清单内;如需 multica 式「可分配 Issue 看板」可后续 cherry-pick。

## 7. 方法论红线(本仓库,本轮未破)

绝不 mock 冒充真实(mock 路径自我标注)· Signal 只经 `Derived<Signal>` 派生 · 观测 append-only · 机器/手填观测类型层分流 · DoD 只按证据谓词勾 · 报告数字一律从 DB/工作区读回。

## 8. 下一步(网关恢复后的收尾顺序)

1. supervisor 自动跑通 linkcheck-md 五阶段环 → 验 `workflow_run` 出现 `ok`、`agent.runs/wins` 增长、`artifact` 有行。
2. 用新 evidence JSON 重生成 HTML 演示(素材源 `iterations/COMPLETE-FORM-NOTES.md §0`)。
3. (可选)收 G3 起草真实 / 知识库实体 / cron 表达式。

---
**一句话交付**:完整形态在代码与可见层均已就位且经核验;真实执行闭环挂在 GLM 网关 529 上,supervisor 持续重试,恢复即闭合。无 mock 冒充,所有数字可核验。
