# 17 · run 调度重做：解冻 + worktree 隔离 + 中止 + 串行锁

> 本文件是 **bug①④⑤③ 这一窗的执行 plan**（设计层唯一事实源仍是 `plan/06`；产品命题 `plan/07`）。
> 决定是和用户一问一答 grill 出来的，不是凭设计猜着改。偏差处都标了。
> 实践日志一手记录在 `iterations/PRACTICE-buddy.md` §2 步4/5/6 + §4.3/§6.7。

---

## 0. 这一窗要解决什么（四件事 + 分类）

| 编号 | 现象 | 分类 | 依据 |
|---|---|---|---|
| **① 冻死** | 点「▶ 跑」后界面冻死，run 跑完才解冻（找指标冻 21 分钟） | **bug（实现背离设计意图）** | 设计要 per-phase 流式（`kernel.rs:496` 注释 + `plan/03` `subscribe()->Stream<Event>`）；实现把长 IO 内联在 `dispatch.await` 占着 `&mut app`，Vm 只在 dispatch 返回后重发（`kernel.rs:645/648`）→ 打废流式意图。受一个**特意设计约束**塑形：App 单线程独占、no `Arc<Mutex>`（`kernel.rs:632`） |
| **④ 无 worktree** | 同项目连续/并行跑多个 issue，两个 MR 内容重叠（都从 master 出分支、都改同一文件） | **实践暴露的真设计缺陷** | 设计只画了**项目级**并行（`plan/05 §3.5:149`「两项目=两工作区」），**从没画过单项目内多 issue 隔离**；bw/issue-N 分支隔离是特意设计（C5 PR quarantine，`lib.rs:3320-3328`），但只隔离 commit、不隔离工作树文件；`checkout_issue_branch`（`github.rs:421` `git checkout -b` 从当前 HEAD）连"InReview 窗口"都撞 |
| **⑤ 发送框 mock** | 发送框只回写死【mock】 | **特意设计（诚实留白）** | 原型契约 `plan/01:481`「固定 Agent 回复」；Tier C 真对话显式 out of scope（`plan/00:123`+`plan/04:116`）；Session Resumption 被 `plan/05:89` 显式拒绝（选 baton）；代码带【mock】自标 |
| **③ 三件套串行依赖** | 绑数据要读找指标的产出，要不要在调度层强制 | **待定设计决定** | 三件套是流水线（绑数据读 metrics.toml），但 buddy 不知 issue 间依赖；用户决议：不在调度层强制，依赖意识交给 skill/agent |

**不在本窗**：bug② 联网墙（GLM 网关不支持内置 web 工具，留竞品分析实践时再说）；bug③ codehub MR 回流（已在 `0c70775`+`b02047b`+`a76c6c5` 修稳，本窗只透传 worktree 路径进其 call-site，不重写）。

---

## 1. 决定总览（grill 收口）

### ① 冻死 → 三段拆（"正常项目该有的样子"，非特殊 fix）

把"跑一件活"拆三段：
1. **起手**（快，主线程）：issue 翻 InProgress、建 issue worktree、构 executor、emit RunStarted → dispatch 返回，主线程空出，Vm 重发 → 界面活。
2. **中间长活**（后台，不占主线程）：`run_workflow_inner` 的 round loop（spawn claude + 等出活，最长 30 分钟）甩 `tokio::spawn`。这段只用 `Arc<dyn Store>` + `broadcast::Sender` + 可 move 的 `Engine`（`lib.rs:1457` 注释自承"loop 里碰 self 全是共享借用，从不独占"），不需要 `&mut App`。round 行落账、阶段进度事件照常流。
3. **收尾**（快，回主线程）：后台干完，outcome 经 oneshot 回灌成内部命令 → 主线程跑 tail（`stage_commit_push` + `create_mr`/`open_pr` + transition InReview + `refresh_issues` + 拆 worktree）→ Vm 重发。

- **不动 App 所有权**（保 `kernel.rs:632` 的单线程 no-`Arc<Mutex>` 设计）。后台任务在同 `current_thread` runtime，靠 await 点与主循环 `select!` 交错推进，零锁。
- **收尾 tail 正是 bug③ 修稳的路径**（`stage_commit_push`/`create_mr`/`merge_mr`/`Adopted`）——逻辑不动，只换执行时机/位置 + 透传 worktree 路径。
- **Vm 在 run 期间的重发**：起手一帧、收尾一帧；进度 toast 走原有 notes 通道（`kernel.rs:521` forwarder + `main.rs:162` notes use_future + `NoteState`，另一线程，run 期间本就活）。

### ① 附带：中止（CancelRun）+ 去掉 `--no-session-persistence`

- **CancelRun 命令**：后台任务有 `JoinHandle`，`abort` 它 → spawn future 返回（exit 0xffffffff，同杀进程现状）→ run 结算 `failed`/`cancelled` → issue 停 `in_progress`、`settled_at` 空 → **不自动 Done，铁律守住**。UI 在 issue 卡「进行中」时显「⬇ 终止」按钮。
- **去掉 `claude_cli.rs:251` 的 `--no-session-persistence`**：**只为留审计日志**——claude 每次调用会在 `~/.claude/projects/...` 写一份 `session.jsonl`，事后能翻看 agent 到底干了啥。**不传 `--resume`、不启用续跑**，run 行为不变（每次还是一次性调用，baton 接力照旧）。
  - **偏差注记（对 `plan/05:89`）**：`plan/05` 当年选 `--no-session-persistence` 是要"显式可审计交接（baton）vs 黑盒上下文恢复（session）"二选一。去掉 flag 让 session.jsonl 存在，**严格讲弱化了那条原意**。但 buddy 不传 `--resume`、不使用 session 续接，baton 仍是唯一接力机制——只是多留个**只读审计文件**，不改 run 模型。用户 2026-07-31 认可此偏差（理由：调试/审计可见性）。续跑（真用 session 续接）是 ⑤b，不在本窗。

### ④ worktree 隔离（串行之外的第二道必需）

- **主工作区留着**（`workspaces/<project>-<id>/`，停 master）：给代码仓连接器探针（commits/docs）、版本面板（`git log`）、`SyncMetricsFile` 读 **merge 后**的 `.bw/metrics.toml` 装 metric 表——提供"master 视角"，不跟某个 issue 的 worktree 跑。
- **每个 issue 一个 worktree**：放主工作区**平级**目录 `workspaces/<project>-<id>-issue-<n>/`（与 `provision_workspace` 命名同构），从 master 出 `bw/issue-<n>`。run 在这里干活、commit、push、开 MR。
- **生命周期**：起手 `git worktree add`，settle（push+开完 MR）后 `git worktree remove`；分支留远端供 review/merge。失败/取消也 `remove --force`（清未提交改动）。
- **迁移点**（SubAgent 查实，`run_issue_now` 硬编码 `proj.workspace_path` 的地方都要改用 issue worktree 路径）：
  - `lib.rs:3351-3357` workspace_hint
  - `lib.rs:3431-3451` `checkout_issue_branch` → 改 `provision_issue_worktree`
  - `lib.rs:3486` 附近 PR 创建路径（`stage_commit_push`/`open_pr`/`create_mr` 的 workspace 参数）
  - `evidence::head_commit`（`lib.rs:1406-1410`）的 change-window 采集
  - run 内任何 `metrics_file` 读取（`lib.rs:2705` 附近）——**只 run 内读用 worktree 路径**；`SyncMetricsFile`（merge 后装表）仍读主工作区，不动。
- **不碰 bug③ 的 engine 层**（`remote.rs`/`codehub.rs`/`github.rs` 的 `create_mr`/`merge_mr`/`Adopted` 实现）——只把 worktree 路径透传进 `stage_commit_push`/`create_mr`/`open_pr` 的 call-site 参数。

### ⑤ 发送框（收窄：只做 A + 去 flag，C/B 不做）

- **A 中止**：随 ① 三段拆带上（见上 CancelRun）。
- **去掉 `--no-session-persistence`**：随 ①（审计日志）。
- **C 续跑 / B 插话**：**不做**，单独立项。理由：续跑要传 `--resume` + 存 claude 的 session-id + 反转 `plan/05` Session Resumption 决定，是产品哲学级决定，非本窗。**用户 2026-07-31 决定：先不搞续跑，回头和 buddy 原作者对思路**。
- **发送框 mock 保持**：【mock】诚实标注不动。这是"plan/05 设计意图决定这波不真化"，**不是遗留 bug**，是了结的决定。

### ③ 串行/依赖（不强制依赖，只锁并发）

- **同一项目内串行锁**：`AppState` 加 `active_run: Option<(ProjectId, IssueId)>`（和 `active_session` 同构，零新锁，符合单线程模型）。`RunIssue` 起手前检查"同 project 是否已有 active run"，有则拒绝（toast「该项目有活正在跑」）。settle 时清。
  - 理由：**预算**（别同时烧两个 claude，$1000 封顶会被双开吃穿）+ **简单**（设计只画了跨项目并行）。**不是撞文件**（worktree 已解）、**不是依赖**（见下）。
  - 跨项目并行**不禁止**（设计本来就要，三段拆修好后自然恢复，不画蛇添足加项目间锁）。
- **不强制 issue 依赖顺序**：buddy 不知 issue 间依赖，不当依赖图管家。依赖意识交给 skill/agent——skill 正文告诉 agent"我需要找指标产出的 metrics.toml"，缺了 agent 自己处理（老实失败/标出来）。三件套"上游合入才下一个"是**正确用法**，但 buddy 不强制；用户用错（绑数据先于找指标 merge 跑），agent 按依赖处理。这符合"活让 agent 干"。

---

## 2. 实施步骤（按依赖序，每步独立 commit）

> 开发在独立 worktree `bug1-4-5-run-scheduling`（off `main`）里做——同目录另一窗口在搞 step3 收尾，worktree 隔离不撞。**只 commit 不 push**（用户在另一环境打 bundle 提交）。

### S1 · `active_run` 串行锁（最小、先落）
- `AppState`（`lib.rs:1025`）加 `active_run: Option<(ProjectId, IssueId)>`。
- `RunIssue` arm（`lib.rs:5097`）起手前：若 `self.state.active_run` 同 project 已有 → `Err(AppError::Invalid("该项目有活正在跑，等它到评审中/干完"))`。
- `run_issue_now` 起手设 `active_run = Some((p, id))`；收尾（成功/失败/取消）清。
- E2E 读回：同项目第二张卡点▶被拒，sqlite `active_run` 读写一致。

### S2 · worktree 隔离（④ 主体）
- 新 `bw_engine::workspace::provision_issue_worktree(main_ws, issue_n) -> PathBuf`：`git worktree add <主工作区平级 -issue-<n>> -b bw/issue-<n> master`。
- `run_issue_now`：`checkout_issue_branch`（`lib.rs:3431`）换 `provision_issue_worktree`；executor 的 workspace、`stage_commit_push`、`create_mr`/`open_pr`、`evidence::head_commit`、run 内 metrics_file 读，全用 worktree 路径。
- 生命周期：起手 add，收尾（含失败/取消）`git worktree remove --force`。
- E2E 读回：run 起手 worktree 目录出现、settle 后消失；分支 push 远端、MR 从该分支开；`SyncMetricsFile` 仍读主工作区 master（merge 后装表，读回 metric 表）。

### S3 · 三段拆 + CancelRun（① 主体，最大一步）
- `run_workflow_inner`（`lib.rs:1366`）拆：把 round loop（engine.run_phase_range + settle round 行 + 落 phase message）提成可 `tokio::spawn` 的 self-contained async fn（吃 `Arc<dyn Store>` + `broadcast::Sender` + `Engine` + `spec` 等 by value，返回 outcome + last_run_log）。
- `run_issue_now` 起手段（transition + worktree + emit RunStarted）在主线程；spawn IO loop；不 await，把 `JoinHandle` 存进 `active_run`（扩成 `{(ProjectId, IssueId), JoinHandle, CancelToken}`）。
- 收尾：IO loop 完成经 oneshot 把 outcome 发回 → kernel 主循环 `select!` 加一臂收 → 跑 tail（`stage_commit_push` + `create_mr` + transition InReview + `refresh_issues` + 拆 worktree + 清 `active_run`）→ Vm 重发。
- `Command::CancelRun { id }`：`JoinHandle::abort()` → spawn future 返 → 走 failed/cancelled 收尾路径（issue 停 InProgress、settled_at 空、拆 worktree）。
- UI：issue 卡「进行中」显「⬇ 终止」按钮 → `CancelRun`。
- E2E 读回：run 期间界面不冻（点别的卡能进、Vm 重发）；进度 toast 活；「⬇ 终止」能中止，issue 停 InProgress 可重试，settled_at 空（铁律）；后台 claude 子进程被 kill（`kill_on_drop`，无泄漏）。

### S4 · 去掉 `--no-session-persistence`（审计日志）
- `claude_cli.rs:251` 删 `.arg("--no-session-persistence")`。
- E2E：跑一次后 `~/.claude/projects/...` 见 session.jsonl；run 行为不变（不传 --resume）。

### S5 · 门禁 + E2E + 记账
- 门禁全绿（`cargo fmt --all --check` / `clippy --workspace --exclude app-desktop -D warnings` / `cargo check -p bw-core --target wasm32 --no-default-features` / `cargo check -p ui --target wasm32` / `guard-kernel-ui-free.sh` / `cargo check -p app-desktop`）。
- E2E 读回（深链 + sqlite）：
  - `BW_OPEN=<项目> BW_PANEL=issues cargo run -p app-desktop` → 点找指标▶ → **界面不冻**（能切到别的面板/点别的卡）→ run ok → issue InReview + worktree 消失 + MR 开 + metric 表装（merge 后）。
  - 中途点「⬇ 终止」→ run failed、issue 留 InProgress、settled_at 空、worktree 拆掉、claude 子进程没留。
  - 同项目第二张卡▶ 被串行锁拒。
  - session.jsonl 存在。
- 过 `/code-review`。

---

## 3. 不破的铁律（逐条对账）

| 铁律 | 本窗怎么守 |
|---|---|
| Done 永不自动 | CancelRun/失败只停 InProgress（不自动 Done）；InReview→Done 仍只人 merge 触发 |
| Signal derive-only、无数据=Unknown≠绿 | 不碰 Signal 路径；worktree/metrics 分离不改 `recompute_signals` |
| settle-once | 每 round 的 `workflow_run` 行仍 settle 一次；CancelRun 走 failed settle |
| schema 迁移不崩老库 | 本窗不加列（`active_run` 是内存 AppState 字段，不落库）；若落库另加 `add_column_if_missing` |
| cron 只建不完成 | 不碰 cron |

---

## 4. 偏差与待决（如实记）

- **偏差·对 `plan/05:89`**：去掉 `--no-session-persistence` 让 session.jsonl 存在，弱化"显式可审计 vs 黑盒"的严格二选一。buddy 不传 `--resume`、baton 仍是唯一接力，只多留只读审计文件。用户 2026-07-31 认可。
- **待决·⑤b 续跑**：用户回头和 buddy 原作者对思路。若要做：传 `--resume` + 存 claude session-id（现在只存 buddy SessionId）+ 反转 `plan/05` Session Resumption 决定。单独立项。
- **待决·③ 是否给三件套特例 merge-gate**：当前不强制（依赖交 agent）。若实践发现 agent 处理不好"上游没 merge"（绑数据空转/产空报告），再回头在调度层加"三件套上游未 merge 不让下游跑"。**待 ④ 落地后实测绑数据在隔离 worktree 读不到 metrics.toml 的真实失败信号**。
- **待决·mid-run 插话（B）**：要改 run 骨架（fire-and-forget 多阶段 → 长持续会话），单独立项，非本窗。
- **协调·同目录另一窗口**：step3 收尾窗口在同目录，我用 worktree `bug1-4-5-run-scheduling` 隔离开发；bug③ 的 engine 层不动，只透传 worktree 路径进 call-site，无冲突。

---

## 5. 验证清单（读回为证，DB=`…/BuildersWorkbench/workbench.db`）

```bash
# 不冻：run 期间能切面板/点别的卡（深链 stderr [BW_OPEN] 见渲染）
BW_OPEN=<项目> BW_PANEL=issues cargo run -p app-desktop

# 串行锁：第二张同项目卡▶ 被拒
sqlite3 <db> "SELECT id,project_id,issue_id FROM active_run"   # 若落库；不落库则 UI 读回

# worktree：起手出现、settle 消失
ls workspaces/<project>-<id>-issue-<n>/   # run 中存在；run 后无

# 中止：issue 留 InProgress、settled_at 空
sqlite3 <db> "SELECT number,status,settled_at FROM issue WHERE id=<id>"

# run 记账：workflow_run 行 settle 一次
sqlite3 <db> "SELECT id,status,duration_ms FROM workflow_run WHERE issue_id=<id>"

# 审计日志
ls ~/.claude/projects/   # session.jsonl 存在
```
