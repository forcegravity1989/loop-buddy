# V1 · issue ▶跑 全走嵌入终端(issue 脚本调度路径退场,多 agent 能力转 prompt 驱动)

> **30 秒导读**:这是「终端会话重构」收口阶段设计,接续 `issue2-terminal-conversation-refactor.md`。把 issue ▶跑 从「只有找指标/绑数据两技能走终端、其他走 buddy 脚本调度的多 agent 阶段循环」改成「所有 issue ▶跑 走嵌入终端(一个 claude 会话)」。多 agent(构建师/评审师)能力不删,转成 **prompt 驱动**:技能方法论讲清调度流程,claude 在终端会话里用 SubAgent(独立上下文)调度各 agent。buddy 不再脚本调度 issue 的 agent(per-agent 战绩记账不适用于 issue 活,PTY 看不见 claude 内部调度)。非 issue 命令(stage playbook/hub workflow/cron)仍用 buddy 脚本调度阶段循环机器,不在本次范围。

## Context(为什么做)

用户点 ▶跑 自建无技能 issue「测试issue创建」(构建阶段),工作流屏没出嵌入终端,还显老 Chat UI。读回根因:`run_issue_now`(lib.rs:4884)按 `is_interactive_skill` 分路——只有 `north-star-discovery`/`metrics-binding` 走 `run_issue_interactive`(PTY 嵌入终端),其他 issue 走 `run_issue_backgrounded`/`run_issue_body`(buddy 脚本调度的阶段循环,老 Chat UI)。

用户拍板:**整个 buddy 都按新 claude CLI 走**,老 issue 脚本调度路径(`run_issue_body`/`run_issue_backgrounded`)删掉。多 agent 能力不丢,转 prompt 驱动(技能方法论讲清 claude 怎么用 SubAgent 调度 agent)。

**产品可见变化(要写进人话)**:以前非交互式 issue 靠 `issue_run_tail` 提 MR + 推 InReview;之后全靠 agent 在会话里自提 MR + `poll_interactive_inreview` 检测。无技能/竞品分析类活若 agent 没提 MR,会**诚实停在 InProgress**(不假装前进)。

## 用户拍板的 prompt 模型

- **位置 prompt(auto-submit 首句用户消息)= issue 标题 + 描述**(desc 空则只 title,不带尾空行)。
- **系统提示词(--append-system-prompt)= bridge prompt(项目上下文+铁律+技能契约段)+ 技能正文(方法论)+ 蒸馏技能块(distilled_block)+ 本阶段技能目录块(catalog_block)**。后两块现在只进 phase-loop 的 `spec.prompt`,必须并进 interactive,否则「经验复利」在 issue 交付侧静默失效。
- submit_prompt = true(agent 自启)。
- buddy 默认系统提示词/默认 skill = 后续 V1 催熟设计(进维护指南 m4),本次不做。

## 改动清单

### A. 路由:所有 issue ▶跑 走 `run_issue_interactive`(bw-app/src/lib.rs)

- `run_issue_now`(4837-4891):删 `is_interactive_skill` 交付路由门(4884)+ `run_issue_backgrounded`/`run_issue_body` 分支(4886-4890)。所有 issue 走 `run_issue_interactive`。
- **咨询门(4842-4867 整块)**:整块去 `is_interactive_skill` 外层门(不只改 Done/InReview 条件)——否则无技能 issue 交付中再点 ▶ 会撞「该项目有活正在跑」而不是切焦点。整块改成:有 conversation 行 + is_resume + Done/InReview → `open_conversation`;同卡已在 active_run → 切焦点;咨询 PTY 已活 → 切焦点。这些判断不再受技能门限制。
- `OpenIssueDetail` 切卡唤醒门(7474):去 `is_interactive_skill`,只看 `!c.claude_session_id.is_empty()` + `!is_live`。
- `poll_interactive_inreview` 候选过滤(1977):去 `is_interactive_skill`,改成「所有 InProgress + 有 conversation 行 + 无 PR + 有 github_number」(保留结构性过滤,不误轮询非 repo 项目)。
- `is_interactive_skill`(4823 定义):全部门去掉后无调用方 → 删除。

### B. 无技能 + issue 内容作 prompt + 蒸馏/目录注入(`run_issue_interactive` first-run,5447-5555)

- 删 5547-5551 的空 skill_body 硬报错。
- first-run plan 构建改成:
  - `skill_body = fetch_skill_body(...)`(空则空串,不报错)。
  - `bridge_prompt = build_bridge_system_prompt(&playbook_ctx, &standard_skill)`。
  - 从 `prepare_issue_run` 已算好的 `spec.prompt` 取 `distilled_block`+`catalog_block`(或暴露这两块出 `IssueRunPrep`),拼进系统提示词。
  - `system_prompt = bridge_prompt + "\n\n" + skill_body + "\n\n" + distilled_block + "\n\n" + catalog_block`(空块跳过)。
  - `issue_prompt = if desc.trim().is_empty() { title.clone() } else { format!("{title}\n\n{desc}") }`。
  - `build_startup_plan(&CLAUDE, &issue_prompt, &system_prompt, workspace_cwd)`(签名不变,第 2 参改传 issue_prompt,第 3 参改传组装后的 system_prompt)。
- resume 分支(5498-5514,`build_resume_plan`):不变。
- `build_bridge_system_prompt`(bw-engine/src/interactive_cli.rs:356)文案两档:
  - 空 slug → 「未关联技能,由你驱动或描述要求;项目上下文与铁律已就位」。
  - 非空未知 slug → 保留「你正在执行技能: `{slug}`」(让用户看到 typo 能自查)。

### C. 删 issue 专属老脚本调度路径(守「不为向后兼容留旧路径」)

可安全删(只 issue 用,无其他调用方,grep 验证):
- `run_issue_body`(5288-5313)、`run_issue_backgrounded`(5326-5427)。
- `SettleOutcome::PhaseLoop` 变体(1177)+ `run_issue_settle` 的 PhaseLoop 匹配臂(5902-5916)+ else 分支(5957-5967)。
- `issue_run_tail`(5137-5276)。
- `ActiveRun` 的 `issue_ws`/`pr_eligible`/`proj` 字段(1216-1218,interactive 写但 settle 后无 `issue_run_tail` 读)。保留 `ActiveRun.project`/`is_resume`。
- `run_issue_now` 的 `settle_tx` 分支(4886-4890)。
- `prepare_issue_run` 里只服务 phase-loop issue 的 `issue_brief` 注入(5037-5048,删后变死代码,顺手清)。

**不能删**(服务非 issue 命令 RunStagePlaybook/RunHubWorkflow/RunDraftWorkflow/cron):
- `run_workflow_inner`/`prepare_run`/`run_round_loop`/`finalize_run`/`MockExecutor`/`PreparedRun`/`FinalizeCtx`/`LoopEnd`/`RunOutcome`/`prepare_issue_run`(主体)/`cancel_run`/`stage_workflow`。
- `run_issue_interactive` 的 inline 分支(5699-5722,`settle_tx=None` 的 examples/headless 模式,走 `run_skill` 非 PTY):保留。

### D. UI(工作流屏,app-desktop/src/screens/op.rs + kernel.rs)

**Chat/沉淀/RunOutputs 不删,门控保留给阶段循环会话**(stage playbook/hub workflow/cron 的 session/message 仍经 `op.chat` 显):
- Vm 加判别:焦点会话是「终端会话(conversation)」还是「阶段循环会话(legacy chat session)」。判别 = 焦点是否属于 `pty_live_ids`/`focused_conversation` vs `active_session`(legacy)。
- `chat_area`(2973-2983):`Chat { chat }` 改成「焦点是阶段循环会话时显」(issue/咨询终端会话不显 Chat)。
- 「↑ 沉淀为静态」按钮(2994-3000)+ `Command::PromoteWorkflow`(lib.rs:7768)+ promote callback(2940-2954):**保留给阶段循环会话,不重做**(PromoteWorkflow 收 SessionId 是 legacy chat 行;终端会话的蒸馏另议,本次不做)。
- `RunOutputs`(3002-3005):保留,阶段循环会话显;终端会话无 msgs 自然空。
- 「标准工作流」方法循环卡(2986-3001):issue 终端会话不显(误导);阶段循环会话显。`stage_workflow` 函数保留。
- TerminalWidget 块(3045-3058):`if op.pty_active` 保留(kernel 改后所有 issue run 都 pty_active=true)。
- `RunBanner`(2985):保留;交互式 run 无 phases 显空 span 无害。
- `SelectSession`/`AppState.active_session`/`ChatVm`/`op.chat` plumbing:**保留**(RunWorkflow/CompleteCreation/workflow_hub 还用)。

### E. examples(CI 跑,本地链接失败但要编过)

dispatch `RunIssue` 的 examples 行为变(不再走 buddy 脚本调度):
- `adversarial_loop.rs`(测 issue 对抗循环三结局):前提消失。retarget 到 `RunStagePlaybook`=**重写**(删 issue 状态断言、改读 `workflow_run`);或删,phase-loop 覆盖由 lib 单测保。实施时定。
- `agent_cli_routing.rs`:retarget 到 `RunStagePlaybook`/`RunWorkflow` 或删。
- `practice_aihot.rs`/`practice_first_loop.rs`(真 claude probe):RunIssue 走 inline `run_skill`(系统终端);行为变但 mock/真 claude 仍可跑,调断言;网关依赖重标 defer。
- 无技能 issue example(`incubate_issue.rs` 等):空 skill 不再报错 → 能跑。
- 补:`verify_c8_standard_trio`、`verify_skill_materialize`、`verify_c13_draft_mock_lock` 也 dispatch RunIssue,阶段 3 一并改/retarget。

### F. 文档

- 设计 md `issue2-terminal-conversation-refactor.md` §13:记本次决定(所有 issue ▶跑 走终端、issue 脚本调度路径删、多 agent 转 prompt 驱动(claude 用 SubAgent 调度,per-agent 战绩不适用 issue 活)、非 issue 命令保留脚本调度、蒸馏/目录块并进 interactive 系统提示词、无技能 issue 内容作 prompt、InReview 改 agent+poll、默认系统提示词/默认 skill defer 进 m4)。
- 指南 u6:纠「▶跑 → 嵌终端」——所有 issue ▶跑 进嵌入终端(每卡一会话);无技能也跑(issue 内容驱动);多 agent 在会话内由 claude 用 SubAgent 调度(技能方法论讲清);阶段循环只用于 stage playbook/hub workflow/cron;**无技能/无 MR 的活会诚实停在 InProgress,不假装前进**。
- 维护指南 m4(技能与 Prompt):补默认系统提示词/默认 skill 是后续 V1 催熟设计点;多 agent prompt 驱动调度写法。
- `docs/code-schemes.md`:登记代号 V1-TermClose1–3(收口阶段 commit 前缀,防撞车)。
- 铁律表加行:Done 仍人点 / 咨询不 settle / MR 改 agent 自提+poll / 无 schema 变更。

## 执行分阶段(逐 commit,不 push)

1. **功能解锁**:A(路由 + 咨询整块)+ B(prompt 模型 + 蒸馏/目录注入 + bridge 文案两档 + desc 条件)+ is_interactive_skill 删 + poll 放宽。改完能测无技能 issue ▶跑 进终端且带蒸馏上下文。过门禁 + lib test(改 build_startup_plan 调用方后更新测试)。
2. **删老路径 + UI 门控**:C(删 issue 脚本调度路径 + 清 issue_brief 死代码)+ D(Chat/沉淀/RunOutputs 门控保留给阶段循环会话 + 方法循环卡门控 + Vm 判别)。过门禁 + 读回。
3. **examples + 文档**:E(retarget/删/调断言)+ F(§13 + u6 + m4 + code-schemes + 铁律表)。过门禁 + examples check。

## 验证

1. 门禁 6 步(fmt/clippy/wasm×2/guard/app-desktop)。
2. `cargo test --lib -p bw-app -p bw-engine -p bw-store`:现有 interactive 测试过;改 `build_startup_plan` 调用方后更新/加测试(issue 内容作位置 prompt、无技能不报错、skill_body+蒸馏并入系统提示词)。
3. `cargo check --examples -p bw-app`:retarget 后 adversarial_loop 等编过(CI 跑 test)。
4. 读回为证(sqlite3 `$APPDATA/BuildersWorkbench/workbench.db`):无技能 issue ▶跑 → `claude_conversation` 有新行 + `pty_active` 起(深链 `BW_OPEN=cowelink BW_PANEL=workflow` stderr `[BW_OPEN]` + 截图显终端);Done 后咨询点卡 → open_conversation 不 settle(`settled_at` 不变)。
5. 真 E2E(点卡+PTY+claude)受 GLM 网关抖动影响,标 defer 让用户验(切卡接回、重启点卡恢复、无技能 issue 跑通、多 agent 在会话内被 SubAgent 调度)。

---

_本篇为收口阶段设计;守 `CLAUDE.md` 铁律 + `issue2-terminal-conversation-refactor.md` §13。拿不准写进该篇 §13。_
