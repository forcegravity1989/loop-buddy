# V1 Issue 2 · 终端会话重构 — 活交付与 Claude 会话解耦(设计事实源)

> **30 秒导读**:这篇是 W2 之前没做到位的「终端/会话架构」重构的设计事实源,给接手这块的开发看,**现在作数**。原 `issue2-metrics-interactive-loop.md` 保留为 Issue 2 主体(已落地的交互式引擎 + 绑装置 + skill + guide),本篇只管「终端多会话 + 切卡 + 重启恢复 + 咨询态 + 窄窗错行」这块 W2-1 的归正。读遗留以 `LEFTOVERS.md` 为准;本篇与 LEFTOVERS W2-1 / V1-P1 交叉处以本篇为准(归正)。
>
> **代号说明**:会话(Conversation)= 可持续的 claude 对话记忆,有独立身份,可跨多次点开;终端连接(PtyAttachment)= 对一个 PTY 进程的临时连接,可断可重连;交付运行(DeliveryRun)= 一件活此刻正在交付中,同项目唯一;活(Issue)= 一次交付承诺,有状态机和验收门。铁律、读回为证、schema 双守卫等工程操作词见 `CLAUDE.md`。

---

## 1. 为什么要重构(根因)

W2-1 原记的三现象(切走丢字、重启黑框、窄窗错行)经两轮独立架构评估,**根因不是「单槽 watch 丢字节」这一个机制,而是更深的:当前 buddy 把四个本应独立的生命周期错误地绑成了一件。**

四个被绑死的概念(每行带代码证据):

| 概念 | 应该是 | 现状(代码证据) | 绑死后果 |
|---|---|---|---|
| **活 / Issue** | 一次交付承诺,有状态机 Backlog→Done 和验收门 | `issue` 表带 `claude_session_id` 列(`schema.sql:509`) | 活完成 = 会话也跟着"结束" |
| **交付运行 / DeliveryRun** | 一次真实交付执行,有成败/耗时/产物,同项目唯一 | `ActiveRun` 结构(`lib.rs:1203`)+ `active_run: Option<ActiveRun>`(`lib.rs:1266`);交互式不走 `issue_run_tail`(`lib.rs:5536` 注释) | 交互式跳过 MR 记账,交付运行概念模糊 |
| **Claude 会话 / Conversation** | 可持续的 claude 对话,有独立身份,可跨多次点开 | 身份被钉死在 `issue.claude_session_id` 一列上;`run_issue_interactive` 收的 `SessionId` 参数被下划线忽略(`lib.rs:5330`) | 一个活只能有一个会话;会话无法独立于活存在 |
| **终端连接 / PtyAttachment** | 对一个 PTY 进程的临时连接,可断可重连 | `pty_input_tx: Option<...>` 全局单例(`lib.rs:1331`);`pty_bytes_rx` 同样单例(`lib.rs:1336`);kernel 侧 `pty_tx` 是单槽 watch(`kernel.rs:525`) | 同一时刻只能有一个 PTY;切活必须 drop 旧 PTY;重启全丢 |

绑死的工程补丁:`op.rs:613 existing_issue_session` 用 `(stage, title)` 去**猜**哪个 session 对应哪个活——概念层缺失时 UI 层硬凑。这就是用户撞到「切到绑指标卡却看到绑数据的 CLI”的根因:终端不绑你正在看的卡,全局只有一个。

用户用 orca(`D:\2026\code\orca-main`,Electron+node-pty+xterm)做体感对比后判断:orca 把这四个分得很清(每会话一 PTY 子进程、daemon 持有 `Map<sessionId, Session>`、字节按 sessionId 路由、每 pane 独立 xterm、多会话并发、切卡是 attach 仍活 PTY + 快照 replay)。buddy 的产品定位跟 orca 不同(单人构建者工作台 vs 多 agent 工作台),但「内嵌 claude cli 的多会话切换」这块交互可以吸纳。**用户决定:一波到位,把活和会话真正拆开,不再用局部补丁修 W2-1。**

---

## 2. 目标设计(用户旅程)

### 2.1 A 交付中,切到历史 B 咨询

1. A 是当前交付运行中的活(InProgress,占着项目唯一的交付名额 `active_run`)。
2. 用户点 B 卡。**B 的 claude 进程若仍活着** → 终端立即接到 B 的 PTY,显示 B 的实时输出;**A 在后台继续跑,不受影响**。
3. **B 的 claude 进程若不在了**(buddy 重启过 / B 之前没点开过)→ buddy 按原路径重建 B 的 issue worktree(`provision_issue_worktree` 幂等,`workspace.rs:297-312`),在相同 cwd 执行 `claude --resume <B 的 claude_session_id>`,spawn 一个新 PTY。
4. 用户在 B 里咨询。B 的 claude 有完整历史上下文(同 cwd → claude 的 encoded-cwd 一致 → 能 resume,见 §5)。
5. 切回 A,立即看到 A 在后台产生的新输出(有界队列缓冲,见 §7.4)。
6. 全程**只有一个活(A)是"交付运行"**;B 的咨询不重新记账、不改 B 的状态、不触发 settle。

### 2.2 buddy 重启后

1. 所有 PTY 进程确实死了(纯内存的连接全清),**不假装还活着**。
2. 每张交互式活卡仍记得自己的 `claude_session_id`(落库的)。
3. 用户点哪张卡,buddy 就按原路径重建 worktree + `--resume` 那张卡的会话。
4. **不在启动时批量唤醒全部历史会话**(避免一次起 N 个 claude 进程)。

### 2.3 咨询态(Done / InReview 活)继续对话

1. 活 Done 后,交付账已结清(`settled_at` 标记,同一件活绝不记两次)。
2. 对应 claude 会话**仍可点开继续聊**(咨询)。
3. 已完成活的会话**不再代表一次新交付**,不占交付名额,不改变活状态。
4. 若用户在咨询里要改代码/做新交付,靠 prompt 规则引导"请新建一件活"。**这是行为约定,不是技术只读**——见 §6 诚实口径。

---

## 3. 概念模型(四个独立生命周期)

```
活 / Issue                  交付承诺,有状态机,有验收门
  └─ 回答:这次交付是什么、是否已由人验收

交付运行 / DeliveryRun       一件活此刻正在交付中,同项目唯一
  └─ 回答:此刻是否有一件活在交付中
  └─ 约束:同项目 0:1(串行锁,run_issue_now lib.rs:4831)

Claude 会话 / Conversation   可持续的 claude 对话,有独立身份,可跨多次点开
  └─ 回答:这段对话记忆是谁、还能不能继续
  └─ 关系:一件交互式活 1:1 一个会话(但不绑死生命周期)

终端连接 / PtyAttachment     对一个 PTY 进程的临时连接,可断可重连
  └─ 回答:此刻窗口接到哪个 claude 进程
  └─ 关系:一个会话 0:1 一个活 PTY(进程死了,会话身份还在)
```

**关键解耦**:活 Done 只结束交付,不结束会话;PTY 进程死了只丢实时连接,不丢会话身份。

---

## 4. 数据模型

### 4.1 新表 `claude_conversation`(持久身份 + 恢复所需事实)

不往 `issue` 行里继续塞字段。新表只存**持久身份和恢复所需事实**:

```sql
-- schema.sql 新增(CREATE TABLE IF NOT EXISTS,新表,无需 add_column_if_missing)
CREATE TABLE IF NOT EXISTS claude_conversation (
    id                TEXT PRIMARY KEY,           -- buddy 自己的稳定会话 id
    project_id        TEXT NOT NULL REFERENCES project(id),
    issue_id          TEXT NOT NULL UNIQUE REFERENCES issue(id), -- 一件交互式活最多一个会话
    claude_session_id TEXT NOT NULL DEFAULT '',   -- claude CLI 的 --resume id(从 SessionStart hook 取)
    workspace_path    TEXT NOT NULL DEFAULT '',   -- 首次建立会话的固定 worktree 路径(用于重建)
    branch_name       TEXT NOT NULL DEFAULT '',   -- 该活分支(bw/issue-N)
    created_at        INTEGER NOT NULL,
    last_opened_at    INTEGER NOT NULL
);
```

**不进库的**(纯内存,进程内 TerminalManager 持有):PTY input channel、子进程句柄、当前 cols/rows、当前是否显示、xterm 实例、未刷的输出批次。buddy 崩/重启后如实消失,从 `claude_conversation` 恢复身份。

### 4.2 旧列退场(不保留双读兼容)

`issue.claude_session_id`、`issue.interactive_started` 这条旧实现路径退出使用。守 `CLAUDE.md` 铁律「不为向后兼容留旧路径」——业务代码只能有一条新路径。

- 存量数据一次性搬到新表(`issue.claude_session_id` 非空的活各建一行 `claude_conversation`)。
- schema 双守卫:`schema.sql` 加新表 + `sqlite.rs` 建表 + 迁移函数;`issue` 表旧列**已定:物理删除**(用 `drop_column_if_present`,见 §13 实施决定),**业务读路径只认新表**(不保留读旧列的 fallback)。
- 注:`session` / `message` 表是遗留的"聊天 UI"概念(从非交互式时代遗留,`schema.sql:122`),与交互式 claude 会话无关,本重构**不动它们**;W2-2 已给它们加了 DeleteSession 清理路径,继续用。

### 4.3 hook 路由调整

`interactive_sessions: HashMap<cwd, IssueId>`(`lib.rs:1319`)当前按 cwd 路由 SessionStart hook 到 issue。重构后 hook listener 把 session_id 写进 `claude_conversation` 表(而非 `issue.claude_session_id`),路由 key 仍是 cwd(会话首次 spawn 时 cwd = worktree 路径,稳定)。

---

## 5. 工作目录语义

每个会话继续用**原 issue worktree 路径**(`<主工作区>-issue-<编号>`),不引入新概念。

### 5.1 现状(两轮评估确认)

- 首次 run 在该 worktree 建 claude 会话(`prepare_issue_run` → `provision_issue_worktree`,`lib.rs:5344` / `workspace.rs:274`)。
- run 收尾时 `IssueWorktreeGuard` drop → `git worktree remove --force`(`workspace.rs:243-264`,RAII,**所有退出路径都 drop**:正常完成 `lib.rs:5632` / 取消 `lib.rs:5670` / 失败 `lib.rs:5619`)。
- `provision_issue_worktree` 幂等:resume 时同路径重建,从已有分支 `bw/issue-N` checkout(`workspace.rs:297-312`)。

### 5.2 关键事实:claude session 跨 cwd resume

claude 把会话持久化到 `~/.claude/projects/<encoded-cwd>/<session_id>.jsonl`,encoded-cwd 是 cwd 路径的编码。

| 重建方式 | cwd | encoded-cwd | 能否 resume |
|---|---|---|---|
| 同路径重建 worktree | `<主>-issue-N`(原路径) | 同 | ✅ 找到历史会话 |
| 主工作区 | `<主>` | 不同 | ❌ 找不到 |
| 路径变了(用户移动/重命名) | 新路径 | 不同 | ❌ 找不到 |

**结论:resume 流程 = 按原路径重建 worktree → 在相同 cwd `claude --resume <session_id>`。** 会话看到的是 B 那条活分支完成时的代码现场(分支未变),这是"历史会话"的正确语义——不是 bug。若用户要基于最新主分支做新交付,新建活(走文件接上下文,薄编排器原则,见 `issue2-metrics-interactive-loop.md §2.6 #7`)。

### 5.3 并发隔离

A 在 `<主>-issue-A` 改代码,B 在 `<主>-issue-B` 咨询,两个独立 worktree,**不互踩文件**。这是 git worktree 的天然隔离,buddy 现有机制已具备,无需新增。

---

## 6. 咨询态守门(prompt-only,诚实口径)

### 6.1 技术现状(为什么不硬拦)

两轮评估确认:
- claude CLI 有 `--permission-mode`(acceptEdits / bypassPermissions / default),**没有 readOnly**。
- `--disallowedTools` 能禁特定工具(如 `Bash(gh pr merge)`),但禁不掉"所有写入类工具"——`Bash(cat > file << EOF)` 仍能写。
- `--allowedTools` 能列白名单,但 buddy 看不到结构化 tool call(PTY 是原始字节流),且**用户已明示只靠 prompt 规则**(不依赖 `--allowedTools`,故不实测它)。
- hook listener(`bw-app/src/hook_listener.rs`)抓 PreToolUse,但 claude 的 hook 机制不能让 buddy 阻止执行(只能事后检测)。

### 6.2 守门方式(prompt 规则)

resume 一个 Done / InReview 活的会话时,在 bridge system prompt 追加咨询规则:

> 这件活已经完成并由人验收。你可以继续回答历史决策、代码解释、后续讨论。如果用户提出新的文件修改、代码开发或其他会产生交付的工作,请建议用户在 buddy 中新建一件活来处理,不要把新交付继续记在这件已完成的活上。

### 6.3 诚实口径(必须写进设计 + 维护指南)

- 这是**行为约定,不是技术只读**。
- claude 仍有完整 CLI 能力,理论上可能越界改文件。
- buddy **不宣称**"已完成活的会话被硬性隔离"。
- UI 给一个显式的"转成新活"动作(用户自己决定何时把咨询里的新诉求转成交付),**不做自动意图分类**(buddy 看不到 tool call,意图分类不可靠)。

---

## 7. 终端内部架构

### 7.1 集中模块 TerminalManager(深模块 seam)

不再让多会话逻辑散在 `bw-app/lib.rs`、`kernel.rs`、`op.rs` 三处。**新建集中模块(放 `bw-engine`,非 UI,过 `guard-kernel-ui-free.sh`)**:

```rust
// crates/bw-engine/src/terminal_manager.rs(新建)
pub struct TerminalManager {
    sessions: HashMap<ConversationId, TerminalSession>,
    // 纯内存;进程退出即清空
}

pub struct TerminalSession {
    conversation_id: ConversationId,
    pty_input_tx: mpsc::UnboundedSender<PtyInput>,
    pty_bytes_rx: mpsc::Receiver<Vec<u8>>,   // 有界,见 7.4
    current_size: (u16, u16),                 // cols, rows
    // child handle 在 PtyBackend adapter 里
}

impl TerminalManager {
    pub fn open(&mut self, conversation_id, plan: LaunchPlan) -> Result<()>;
    pub fn resume(&mut self, conversation_id, session_id: &str, cwd: &Path) -> Result<()>;
    pub fn input(&self, conversation_id, input: PtyInput) -> Result<()>;
    pub fn resize(&mut self, conversation_id, cols, rows) -> Result<()>;
    pub fn close(&mut self, conversation_id);
    pub fn events(&mut self) -> Vec<(ConversationId, TerminalEvent)>;  // 输出/退出/恢复中/失败,全带 id
}
```

**外部调用者只认识稳定的 `conversation_id`**,不直接碰 child handle、channel、平台 PTY 类型。这是 `codebase-design` 的深模块:大量行为(PTY spawn / 字节路由 / resize / 重连)藏在小接口后面。

### 7.2 多会话并发 + 字节路由

- `TerminalManager` 内部 `Map<conversation_id, TerminalSession>`,每个会话一个 PTY、一个有界输出 channel、一个当前尺寸。
- A 是交付运行(占 `active_run` 名额),B/C 是咨询会话(不占名额)。多个 PtyAttachment **可并发活**。
- 字节流**必须带 `conversation_id` 标签**,消费端按 id 路由到对应 xterm,不再是一条无身份的全局流(修掉 `kernel.rs:743-748` 的单槽分发)。
- 同项目"只一件交付运行"的锁(`run_issue_now lib.rs:4831`)只拦"新交付"(InProgress 活的 spawn);Done/InReview 活的咨询 spawn 不走 `active_run`,不撞锁。

### 7.3 xterm:每会话一个独立实例

照 orca 成熟做法:`Map<conversation_id, xterm Terminal>`。切卡只换可见容器 + 键盘焦点,**不销毁隐藏终端**。各自滚动位置、尺寸天然隔离。修掉 `op.rs:3158` 的全局 `window.__bw_term` 单例。

### 7.4 离开卡片期间输出缓冲

`kernel.rs:526` 的 watch 单槽 → **有界 mpsc(每会话独立,64 批,每批 ≤ 8KB,共 ≤ 512KB)**。切走期间字节在队列排着不丢;切回一次性读出。满了**丢最老**(背压会让 claude 卡住,更糟)。这同时治好 W2-1 现象一(切走丢字)。

### 7.5 应用重启恢复

重启后 PTY 进程全死(如实)。每卡从 `claude_conversation` 读 `claude_session_id`;点卡 → 重建 worktree(`provision_issue_worktree`)+ `--resume` spawn 新 PTY。**不在启动时批量唤醒**。UI 在点卡到 PTY 就绪之间显示"恢复中…"。

### 7.6 窄窗错行根治(从源头修)

截图确认:不是丢字、不是乱码(无 U+FFFD),是 **PTY 列数 ≠ xterm 实际宽度 → 宽行塞窄框横向堆叠**。新架构每会话独立尺寸:
- xterm 初次 `fit()` 拿真实 cols/rows;
- PTY spawn **直接用该初始尺寸**,不再默认 80×24(修 `interactive_cli.rs:716` 不设初始 size);
- 卡片重新显示 / 窗口缩放 / 侧栏变化 / 字体就绪都重新 `fit()` + 把最新尺寸发给对应 PTY;
- resize 事件带 `conversation_id`;
- 不再用一个全局尺寸控制所有会话。

---

## 8. macOS 处置(留 seam,不加兼容路径)

**用户决定:允许 macOS 交互式暂不可用,不为兼容让代码架构变复杂,记为遗留开发工作,让 macOS 环境的开发按本架构方案补适配。**

落地:
- PTY 后端抽成清晰接缝 `PtyBackend`:Windows 用现有 `conpty-oxide` adapter;Unix(`portable-pty`)adapter 后续由 macOS 开发在 macOS 机上补。
- 上层多会话 / 路由 / xterm / 生命周期 / 尺寸同步逻辑**不感知平台**——这部分一次做完,macOS 补 adapter 时不用重做。
- **不加外部 Terminal(osascript)临时回落**——它会造第二条体验不同、无法判定完成的旧路径,污染架构(见 `legacy-analysis-engine.md` 对 osascript 的诊断)。
- 在 `LEFTOVERS.md` V1-P1 钉死:macOS 交互式 = 后续适配项,按本架构补 Unix adapter;`CLAUDE.md` 顶部「macOS+Windows」口径纠偏为「V1 交互式 Windows-only,macOS 适配待补」。

---

## 9. 铁律影响分析

| 铁律(人话) | 影响 | 怎么守 |
|---|---|---|
| 杀进程重开,数字能重算、前后一致 | 无影响 | 会话身份落库,重启从库恢复 |
| 信号只能从数据推导;无数据=Unknown≠绿 | 无影响 | 终端重构不碰信号派生链 |
| **完成永远由人点;同一件活绝不重复记账** | **需澄清** | 咨询态(Done 后)继续对话**不算新交付**,不占 `active_run`、不触发 settle、不改状态。只有 InProgress 活的 PTY 是"交付运行";Done/InReview 活的 PTY 是"咨询会话"。`settled_at` 仍是 settle-once 唯一标记 |
| 每件活/运行/产物归属真实可查 | 无影响 | 会话身份和 worktree 路径落库可查 |
| schema 迁移不崩老库 | 需守 | 新表 + 双守卫;旧列退场不保留双读 |
| 定时任务真实到点;自动建活绝不被自动推进 | 无影响 | 终端重构不碰 cron |

**关键澄清**:「同一项目只一件活处于交付运行中」保留——它限制的是"新交付",不限制"历史会话咨询"。A 交付 + B 咨询并发是允许的;只有 A 占 `active_run`。

---

## 10. 分阶段落地

每阶段让最终架构更接近,**不造要删除的临时兼容层**(守 CLAUDE.md「不为向后兼容留旧路径」)。

- **阶段 1 · 概念解耦 + 数据模型**(✅ 已落地,V1-TermRefactor1 系列 commit):建 `claude_conversation` 表 + 存量迁移 + `TerminalManager` 骨架(只含 conversation 身份,无 PTY)。业务读路径(`is_resume` 判断、hook 路由)收口到新表,`issue` 旧列物理退场。实施细节见 §13。
- **阶段 2 · 每会话独立 PTY + 字节路由 + xterm**:`TerminalManager` 实现 spawn/resume/input/resize/events;字节带 `conversation_id`;每卡独立 xterm;watch 单槽 → 有界 mpsc。此时仍是"同一时刻只一个活 PTY",但已带身份路由。
- **阶段 3 · 多会话并发 + 切卡**:A 交付 + B 咨询并发;切卡只切显示/键盘,不杀 PTY;UI 标清"当前显示哪个会话"(修现象三:绑指标卡看到绑数据 CLI)。
- **阶段 4 · 重启恢复 + 窄窗尺寸根治**:重启后点卡重建 worktree + resume;每会话真实尺寸同步链(spawn 用 fit 真值 + remount 重 fit + resize 带 id)。修现象一(切走丢字)和窄窗错行。
- **阶段 5 · 咨询态 prompt 规则 + "转成新活"动作**:Done/InReview 会话注入咨询 prompt;transcript/会话视图里"转成新活"按钮(从消息提取标题/描述建新 issue)。

(原 W2-1 三现象的归正落点:现象一「切走丢字」→ 阶段 2/4 的有界 mpsc;现象二「重启黑框无提示」→ 阶段 4 的"点卡重建+resume";现象三「窄窗错行」→ 阶段 4 的尺寸同步链。W2-1 由本篇承接,`LEFTOVERS.md` 标"归正中"。)

---

## 11. 验证策略(读回为证)

- **阶段 1**:门禁 6 步 + `cargo test`;`sqlite3` 读回 `claude_conversation` 表结构(`PRAGMA table_info`) + 存量迁移正确性(老 issue 的 `claude_session_id` 搬到新表)。
- **阶段 2-4**:深链 `BW_OPEN=<项目> BW_PANEL=issues` + 点卡 → 截图(`claude -p --model haiku` 读图)+ `sqlite3` 读回 conversation 身份;多卡快速切换不丢字、不错行;重启后点卡能恢复。
- **阶段 5**:Done 活点开咨询,验证 prompt 注入 + "转成新活"建新 issue 读回。
- **macOS**:不在本机验(Unix adapter 未建);记为遗留。
- 真实 `claude -p` 交互式 E2E 受 GLM 网关抖动影响,只在 example/监理脚本里跑,不作为常绿验证手段(守 CLAUDE.md 核心纪律 3)。

---

## 12. 与既有文档 / LEFTOVERS 关系

- **原 `issue2-metrics-interactive-loop.md`**:Issue 2 主体(交互式引擎 + 绑装置 + skill + guide),保留。本篇是 W2 终端会话这块的归正,原 md 顶部加指针指本篇;§10.6/§10.7 的 W2-1 三现象由本篇承接归正。
- **`legacy-analysis-engine.md`**:它对 W2-1 的诊断(单槽 watch 丢字节)只盖了三现象之一,且把「切走丢字」和「窄窗错行」混为一谈。本篇以源码 + 截图为准确诊,归正之;该 md 相关段落加归正注记指向本篇。
- **`LEFTOVERS.md` W2-1**:由本篇承接,标「归正中」。
- **`LEFTOVERS.md` V1-P1(macOS)**:钉死为「按本架构补 Unix adapter」的后续适配项,不加外部 Terminal 回落。
- **`LEFTOVERS.md` W1-2(clone 堵 UI)**:用户定不处理(clone 阻塞主循环是正确语义,clone 完成是一切后续前提),继续标「不处理」。

---

## 13. 偏差 / 未决(记不擅定,留给接手)

- **专家建议「lazy resume(切卡只显 transcript,输入时才 spawn)」被用户决定改为「点击就唤醒」**。理由:用户要 orca 式体感;`claude --resume` 点击只 spawn 进程加载历史,不发消息不产生模型调用,成本可接受;多卡常驻 PTY 的内存代价在单人工作台可接受。lazy resume 作为「重启恢复前的过渡显示」保留价值(点卡到 PTY 就绪之间先显历史),但不是主交互。
- **`--allowedTools` 能否硬性只读未实测**——用户已决定只靠 prompt 规则,不依赖它,故不测。若日后要加技术强制,先实测 `claude --allowedTools "Read"` 能否阻止 `Bash(cat > file)` 绕过。
- **会话数量上限**:单人工作台同时活几个 claude 进程合理?默认不限,靠用户显式关。若实测内存爆,再加闲置回收(当前不做,守「不擅自扩 scope」)。
- **`claude_conversation` 表的 `issue_id` 外键 + issue 删除时的级联**:实施阶段定(store 已有 `delete_project` 全表清理模式,`delete_issue` 若有则加级联)。
- **transcript 缓存解析(`~/.claude/projects/<encoded-cwd>/*.jsonl`)**:用于重启过渡显示。claude 的 jsonl 格式未公开文档,实施时参考 orca 的 `claude-usage/scanner.ts` 逆向,只读不写。
- **阶段1 实施决定(2026-08-07)**:
  - 旧列 `interactive_started`/`claude_session_id` **物理删除**(非留空),用 `drop_column_if_present`(SQLite 3.35+ `ALTER TABLE DROP COLUMN`,本仓 bundled sqlite 3.44+ 远高于门槛,环境 sqlite3 CLI 3.53.4 验证通过)。迁移顺序:先 `migrate_claude_conversations` 搬运(INSERT OR IGNORE,issue_id UNIQUE 兜底幂等),再 DROP 两列——搬完 DROP 不丢数据(读回为证:claude_conversation 行仍在,issue 表无旧两列)。
  - conversation 行建行时机:首次 spawn 前 `ensure_conversation`(INSERT OR IGNORE,行存在 = 旧 `interactive_started` 语义)+ hook SessionStart `set_conversation_session_id`(UPDATE,claude_session_id 非空 = 旧 `claude_session_id` 非空语义)。is_resume 改读 `conv.claude_session_id 非空`;is_interactive 改读 `conv 行存在`。
  - **迁移 edge case(存量 F1 失败态)**:迁移只搬 `claude_session_id != ''` 的行。存量里 `interactive_started=1 && claude_session_id=''` 的 issue(spawn 尝试过但 hook 未捕获 session_id)不会被搬 → 迁移后无 conv 行 → `is_interactive` 从 true 临时变 false(直到用户再点 ▶ 触发 `ensure_conversation` 建行自愈)。`is_resume` 两边等价(都 false)。严重度低:自愈、poll 只到 InReview 不到 Done、不碰铁律。上方「行存在 = 旧 `interactive_started` 语义」对 going-forward 成立,对存量该 edge case 不完全成立(已知,接受)。
  - 迁移时 `workspace_path`/`branch_name` 尽力推:branch = `bw/issue-<github_number>`(github_number 非0);workspace_path 推 worktree 兄弟路径 `<parent>/<stem>-issue-<github_number>`(project.workspace_path 非空 + github_number 非0),推不出留空(阶段4 resume 时回填)。
  - `TerminalManager` 骨架在 bw-engine(无 PTY,阶段2 接入),阶段1 无调用点,`#[allow(dead_code)]` 注明。
  - `issue_detail_vm` 纯函数加 `is_interactive: bool` 参数(不读 issue 旧字段),调用链 OpenIssueDetail → IssueDetailData.is_interactive → kernel.rs → vm。

---

## 14. 事实源锚点

| 概念 | 文件 | 行号 |
|---|---|---|
| Issue 表 / `claude_session_id` 列 | `crates/bw-store/src/schema.sql` | 465-512 / 509 |
| Session 表(遗留聊天 UI,不动) | `crates/bw-store/src/schema.sql` | 122-133 |
| `run_issue_interactive`(收的 SessionId 被忽略) | `crates/bw-app/src/lib.rs` | 5330-5333 |
| `is_resume` 判断 | `crates/bw-app/src/lib.rs` | 5343 |
| `prepare_issue_run` → worktree | `crates/bw-app/src/lib.rs` | 5344 |
| `active_run` 全局单例 | `crates/bw-app/src/lib.rs` | 1266 |
| `pty_input_tx` / `pty_bytes_rx` 单例 | `crates/bw-app/src/lib.rs` | 1331 / 1336 |
| `interactive_sessions` cwd→IssueId 路由 | `crates/bw-app/src/lib.rs` | 1319 |
| PTY spawn + 读循环(Windows conpty-oxide) | `crates/bw-engine/src/interactive_cli.rs` | 696-810 |
| worktree provisioning(幂等重建) | `crates/bw-engine/src/workspace.rs` | 274-326 / 297-312 |
| `IssueWorktreeGuard` drop(RAII 删 worktree) | `crates/bw-engine/src/workspace.rs` | 243-264 |
| kernel `pty_tx` 单槽 watch | `crates/app-desktop/src/kernel.rs` | 525 |
| kernel `pty_ticker` 单槽分发 | `crates/app-desktop/src/kernel.rs` | 743-748 |
| `TerminalWidget` + 全局 `window.__bw_term` | `crates/app-desktop/src/screens/op.rs` | 3332-3442 / 3158 |
| `existing_issue_session` (stage,title) 硬凑 | `crates/app-desktop/src/screens/op.rs` | 613 |
| hook listener | `crates/bw-app/src/hook_listener.rs` | — |
| orca 可借鉴机制(对照) | `docs/v1-prototype/orca-terminal-session-reference.md` | — |

---

_本篇为设计事实源;接手开发按 §10 分阶段走,设计决定边做边记回本篇对应小节,拿不准写进 §13。守 `CLAUDE.md` 铁律 + `practice-buddy-landing` 四步纪律。_
