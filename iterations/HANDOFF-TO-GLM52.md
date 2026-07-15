# 交接报告:完整形态第二轮(fable → glm5.2)

> 写于 2026-07-14,分支 `claude/builders-workbench-multica-0517f4`
> (worktree `romantic-grothendieck-e9bb5c`)。
> 本文自足:不依赖会话上下文,每条陈述可对源码/DB/日志核验。

## 1. 本轮目标与结论

用户指令:瞄准**完整的工作台**——五角色、五流程、定时任务、工作流、agent、
skill、connector、产物、版本**均有实际内容**,遵循最初方法论原则
(绝不编造、派生链封口、append-only、DoD 证据、schema 迁移守卫),
默认 **all in one codebase**(每个项目=一个代码仓)。

**结论:九项实体全部落到实体+执行+记账,门禁全绿。** 唯一受外部制约的是
「真实 agent 执行的端到端闭环」——GLM 网关持续 529(见 §5)。

## 2. 本轮五个提交(在 3d3049e 之上)

| 提交 | 内容 |
|---|---|
| 4b46128 | 数据基座:artifact 表(project×path×commit 幂等注册=版本史);skill.content/agent.instructions·wins/connector.project_id+config 迁移守卫列;五角色+五技能实体播种(`seed_stage_entities_if_missing`,按名幂等,老库也补);记账 API |
| 06b2c9d | 编排:run settle 后自动 (a)按名记账 agents/skills (b)扫描工作区注册产物(绑 run id+阶段);非剧本 spec 运行时注入技能库正文;`CompleteCreation` 自动开仓+绑 git-repo 连接器;`SyncConnector` 真探针(git-repo→evidence→Connector 源观测喂 `METRIC_WS_COMMITS/DOCS`;claude-cli→--version);修 `run_optimization_cycle` cron 效果悬空 |
| b6608f5 | UI:产物面板真身(原「评估后留白」的理由已被 evidence+开仓消解);连接器同步按钮+探针色;技能/agent 详情正文(空=「目录引用」如实标注);win_rate 无证据显示「—」;`BW_WORKSPACES` |
| 703221c | real_demo:连接器接管指标喂入(替代脚本直写 GitPr);claude-cli 探针开场;产物采集兜底;evidence JSON 增 artifacts/connectors/role_agents/stage_skills |
| (本提交) | 文档+本交接报告 |

## 3. 九项实体的「实际内容」现状(逐项核验点)

1. **五角色**:`bw-core::playbook` 可执行剧本 + `role_agents()` 实体投影;
   Agent Hub 里五角色行带真实指令模板;每次 run 按名记账。
   核验:`sqlite3 <db> "SELECT name,runs,win_rate FROM agent WHERE name IN ('原型师','构建师','优化师','运营推广师','运维师')"`。
2. **五流程**:StageKind 环 + method_loop 每 phase 真指令 + relay baton +
   Ops→Prototype 回流。核验:`bw-app/tests/run_outcome.rs`、handoff 表。
3. **定时任务**:`App::tick_scheduler`(桌面 5s tick 真调度)+ cron_due 纯函数 +
   effectiveness 聚合;real_demo 给运维段绑每日巡检。
4. **工作流**:spec/run(append-only settle)/version 冻结/analytics/usage 排名 +
   phase_prompts。
5. **agent**:见 1;目录 104 条(OMC/ECC)诚实标注「目录引用」。
6. **skill**:五阶段方法技能带真实正文,烘焙进 phase prompt(单测钉死);
   非剧本 spec 从库解析正文注入(`skills_prompt_block`,6000 字上限);
   `uses` 真实计数。核验:`bw-core::playbook::tests`、`complete_form.rs`。
7. **connector**:git-repo/claude-cli 真探针;`SourceKind::Connector` 观测
   首次落地(Tier D「Connector 真喂指标」);状态只由探针写;其余类型拒绝同步。
8. **产物**:artifact 表 + run 后自动登记 + 手动采集 + 面板真身;
   同路径多提交=版本史。
9. **版本**:workflow_version(优化前冻结)+ 真实 git log 面板 + 产物版本。

**all-in-one-codebase**:`CompleteCreation` 自动开仓(真实 git init+README(项目
brief)+首提交),绑定 git-repo 连接器;桌面默认根=DB 旁 `workspaces/`
(`BW_WORKSPACES` 覆盖);未配 root 的调用方(全部存量测试)行为字节不变。

## 4. 门禁(全部通过,任何后续改动必须保持)

```
cargo fmt --check
cargo clippy --workspace --exclude app-desktop --all-targets -- -D warnings
cargo test --workspace --exclude app-desktop   # 120+ 测试
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
bash scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop                      # 桌面壳编译
```
schema 改动只许走 `add_column_if_missing`(sqlite.rs `open()`,事故教训)。

## 5. 真实执行现状(唯一未闭合项,如实)

- 链路:RunStagePlaybook → ClaudeCliExecutor(`claude -p --output-format json
  --no-session-persistence --max-budget-usd 0.75 --permission-mode acceptEdits
  [--allowedTools Bash]`,会话级凭据 env_remove,529/502/503/504 退避
  30/90/180s 已内建)。
- 制约:GLM 网关(open.bigmodel.cn)持续 529「访问量过大」。2026-07-14 15:32
  直接探测仍 529;既往两次 run 失败均为此(demo-workspaces/run-linkcheck-md.log)。
- 已发起:`scripts/supervise-real-demo.sh linkcheck-md` 后台监理
  (8 次外层重试×120s,幂等续跑,成功过的阶段绝不重跑)。
  查看:`tail -f demo-workspaces/run-linkcheck-md.log`;
  数据以 `demo-workspaces/bw-demo.db` 的 run 行为准,**报告绝不代答**。
- glm5.2 接手动作:网关恢复后重跑 supervisor(两个需求各一次);
  然后 `evidence-*.json` 会带上 artifacts/role_agents 真实数据,
  可据此重生成 HTML 报告(素材源:iterations/COMPLETE-FORM-NOTES.md §0)。

## 6. 已知留白(诚实清单,勿假装完成)

1. **SendSessionMessage 是 mock 回复**(带【mock】标注)——聊天面走真实执行器
   尚未做;若做,复用 run_workflow_inner 的一次性 executor 模式。
2. **创建流程的「起草」run 仍走 Mock**(标注清晰);真实起草=把 drafting_workflow
   跑在项目工作区上,产出可编辑草案。
3. **知识库 Hub** 仍是登记表(用户本轮未列;若做,参照 connector 模式:
   真实来源探针+chunks 真实计数)。
4. **Cadence::Cron 表达式**不支持自动触发(cron_due 诚实返回 false)。
5. mock 模式下 Connector 观测为 0 条是**值未变化的 change-guard 生效**
   (初值=真实当前态,mock 执行不产生新提交),非缺陷;真实执行后每阶段
   提交数增长即有 Connector 源观测流入。
6. 桌面端 `cargo check` 过;完整 `cargo run -p app-desktop` 的人工走查
   (九 Hub 点开)本轮未做,建议 glm5.2 开一次桌面核对产物面板/同步按钮。

## 7. 方法论提醒(本仓库红线)

- 绝不 mock 数据冒充真实;mock 路径必须自我标注。
- Signal 只能经 `Derived<Signal>` 派生;观测 append-only;
  机器观测与手填在类型层分流(`RecordCollectedObservation` 拒绝 Manual)。
- DoD 只按证据谓词勾;核实不了的如实不勾+险交棒留痕。
- 报告数字一律从 DB/工作区读回,不手写。
