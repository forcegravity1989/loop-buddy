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

### 10.1 阶段切分(2026-08-07 接续窗口拍板)

原 §10 五段里阶段 2「先只做身份路由、人为只留一个活 PTY」是工程可审切法,对用户体感几乎无增益;尺寸同步又和重启恢复拆在两段。接续窗口按产品体感重切为四段,**可用底线 = 底座 + 并发切卡 + 重启 resume**(咨询态后置)。不单独 verify 阶段 1(局部未整体可用时局部 verify 无意义)。

- **阶段 1 · 概念解耦 + 数据模型**(✅ 已落地,V1-TermRefactor1):建 `claude_conversation` 表 + 存量迁移 + `TerminalManager` 骨架(只含 conversation 身份,无 PTY)。业务读路径收口到新表,`issue` 旧列物理退场。实施细节见 §13。
- **阶段 · 底座**(✅ 已落地,V1-TermRefactor2):`TerminalManager` 接 PTY(spawn/resume/input/resize/events);字节带 `conversation_id` 路由;每卡独立 xterm(`window.__bw_term_sessions[id]` Map,修掉全局单例);每会话有界输出环(64 批×8KB);尺寸同步链(fit→`TerminalResize` 带 id→`note_fit_size`/`attach` 初始 Resize;remount 重 fit)。`delete_project` 清 `claude_conversation`。底座仍「同一时刻只一个活 PTY」(`attach` 关其余;新交付仍走 `active_run` 串行),但身份路由与多 xterm Map 已就位——不造之后要删的单槽兼容层。
- **阶段 · 并发切卡**(✅ 已落地,V1-TermRefactor3):A 交付 + B 咨询并发;切卡只切显示/键盘,不杀 PTY;UI 标「当前会话」+ Done/InReview「续聊」;多 xterm 常驻(非焦点 `display:none` 仍收字节)。咨询 PTY 走 `open_conversation` → `consultation_runs`,不占 `active_run`、不 settle、不改状态。
- **阶段 · 重启恢复**(✅ 已落地,V1-TermRefactor4):重启后点卡 → 重建 worktree + `--resume`;点卡到首包显示「恢复中…」;**不在启动时批量唤醒**。到此达到「能用」底线(含重启后卡能 resume)。
- **阶段 · 咨询态**(✅ 已落地,V1-TermRefactor5):Done/InReview 注入咨询 prompt;「转成新活」按钮。
(原 W2-1 三现象归正落点修订:现象一「切走丢字」→ 底座有界 mpsc + 并发切卡不杀 PTY;现象二「重启黑框」→ 重启恢复段;现象三「绑指标看到绑数据 / 窄窗错行」→ 底座多 xterm+尺寸同步 + 并发切卡身份路由。W2-1 由本篇承接。)

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
- **接续窗口阶段切分(2026-08-07)**:原阶段 2–5 按产品体感重切为「底座 → 并发切卡 → 重启恢复 → 咨询态」,见 §10.1;可用底线含重启 resume;不单独 verify 阶段 1;不提 issue(干完验证 commit 后再看)。
- **阶段1 实施决定(2026-08-07)**:
  - 旧列 `interactive_started`/`claude_session_id` **物理删除**(非留空),用 `drop_column_if_present`(SQLite 3.35+ `ALTER TABLE DROP COLUMN`,本仓 bundled sqlite 3.44+ 远高于门槛,环境 sqlite3 CLI 3.53.4 验证通过)。迁移顺序:先 `migrate_claude_conversations` 搬运(INSERT OR IGNORE,issue_id UNIQUE 兜底幂等),再 DROP 两列——搬完 DROP 不丢数据(读回为证:claude_conversation 行仍在,issue 表无旧两列)。
  - conversation 行建行时机:首次 spawn 前 `ensure_conversation`(INSERT OR IGNORE,行存在 = 旧 `interactive_started` 语义)+ hook SessionStart `set_conversation_session_id`(UPDATE,claude_session_id 非空 = 旧 `claude_session_id` 非空语义)。is_resume 改读 `conv.claude_session_id 非空`;is_interactive 改读 `conv 行存在`。
  - **迁移 edge case(存量 F1 失败态)**:迁移只搬 `claude_session_id != ''` 的行。存量里 `interactive_started=1 && claude_session_id=''` 的 issue(spawn 尝试过但 hook 未捕获 session_id)不会被搬 → 迁移后无 conv 行 → `is_interactive` 从 true 临时变 false(直到用户再点 ▶ 触发 `ensure_conversation` 建行自愈)。`is_resume` 两边等价(都 false)。严重度低:自愈、poll 只到 InReview 不到 Done、不碰铁律。上方「行存在 = 旧 `interactive_started` 语义」对 going-forward 成立,对存量该 edge case 不完全成立(已知,接受)。
  - 迁移时 `workspace_path`/`branch_name` 尽力推:branch = `bw/issue-<github_number>`(github_number 非0);workspace_path 推 worktree 兄弟路径 `<parent>/<stem>-issue-<github_number>`(project.workspace_path 非空 + github_number 非0),推不出留空(阶段4 resume 时回填)。
  - `TerminalManager` 骨架在 bw-engine(无 PTY,阶段2 接入),阶段1 无调用点,`#[allow(dead_code)]` 注明。
  - `issue_detail_vm` 纯函数加 `is_interactive: bool` 参数(不读 issue 旧字段),调用链 OpenIssueDetail → IssueDetailData.is_interactive → kernel.rs → vm。
- **阶段·底座实施决定(2026-08-07 接续窗口)**:
  - `AppState` 去掉全局 `pty_input_tx`/`pty_bytes_rx`,改持 `TerminalManager`;`ensure_conversation` 返回 `ConversationId` 交给 `attach`。
  - kernel `pty_rx` 载荷改为 `Vec<(ConversationId, Vec<u8>)>`;UI 按 id 写对应 xterm。有界环在 Manager 侧(满丢最老);kernel 仍用 watch 通知(并发段再考虑无订阅时不 drain)。
  - JS:`window.__bw_term_sessions[id]` 取代 `__bw_term` 单例;`TerminalWidget { conversation_id }` + `div#__bw_terminal_<uuid>`。
  - 尺寸:xterm fit 后立刻 stash resize;Rust `TerminalResize` 带 id;`attach` 用 `last_fit_size` 入队初始 Resize(无历史仍短暂 80×24,fit 到即纠正)。ConPTY spawn 本体未改(阶段外),靠 spawn 后首条 Resize。
  - `delete_project` 先删 `claude_conversation`(阶段1 缺口补上)。`delete_issue` 级联仍未决。
- **阶段·并发切卡实施决定(2026-08-07 接续)**:
  - `open_conversation`:Done/InReview + 非空 `claude_session_id` → `prepare_issue_run(resume)` + `attach` + `consultation_runs`;settle 用 `ConsultationEnded`(关 PTY + drop guard,不碰状态/settle-once)。
  - `attach` 不杀 peer;`ConversationMeta.issue_id` 供焦点回落;`focused_conversation`/`focused_issue` 驱动 UI。
  - UI:看板「当前会话」徽 + 「续聊」;`pty_live_ids` 多 xterm 常驻(非焦点 `display:none`);`OpenIssueDetail` 有活 PTY 只切焦点。
  - 咨询 prompt /「转成新活」→ 阶段·咨询态(TermRefactor5)已落地。
- **阶段·重启恢复实施决定(2026-08-07)**:
  - Boot **确认不唤醒**:`Command::Boot` 只重算信号/播种/对账,不 spawn 任何历史会话;注释钉死。
  - 点卡唤醒复用现路径,不造第二条:`OpenIssueDetail` 在 `!is_live` + 非空 `claude_session_id` 时调 `run_issue_now` → Done/InReview 进已有 `open_conversation`,InProgress 进 `run_issue_interactive` 的 `is_resume` 分支;`▶`/`续聊` 仍走 `RunIssue`。
  - 「恢复中…」:`AppState.pty_restoring: Option<ConversationId>` 在 resume 起点置位,首包字节(`drain_pty_events`)或 settle/cancel 清;kernel pty_ticker 清后重建 Vm。UI 另有板级 local signal(点卡立刻亮,盖住 dispatch 返回前的空窗),与 Vm 字段合并显示;工作流面板焦点区也有文案。
  - 空 `workspace_path`/`branch_name` 回填:store `update_conversation_workspace_if_empty`(SQL 只改空列)+ resume attach 成功后调用(阶段1 迁移推不出留空的缺口闭合)。
- **阶段·咨询态实施决定(2026-08-07)**:
  - 新增 `build_consultation_resume_plan`:`--resume`/`--continue` 同时 `--append-system-prompt` 注入 §6.2 咨询规则(`CONSULTATION_APPEND_PROMPT`)。仅 `open_conversation` 路径使用;交付 resume(`run_issue_interactive` → `build_resume_plan`)不注入。
  - 诚实口径:行为约定不是技术只读;UI 文案写「咨询中 · 新交付请另开一件活」,不写「只读模式」。
  - 「转成新活」:详情 overlay(续聊旁)+ 工作流终端区(焦点属于 `consultable_issues` 时)显式按钮 → 复用 `Command::CreateIssue`,标题预填「来自咨询：…」、描述带源 issue 编号。不做自动意图分类;不加新 Command。
  - `--allowedTools` 硬只读仍否决(§6.1/§13 既有未决保留)。
- **全量检视修复(2026-08-07,五阶段落地后 4 路并行检视)**:
  - **修**:① 窄窗错行根治缺口——`term_init_js` 仅初次/remount fit,缺 `display:none`↔visible / 窗口缩放 / 侧栏变化 / 字体就绪的 re-fit(§7.6);加 `ResizeObserver`(观察 `term.element`,跨 remount 稳定)+ `window.resize` 监听,fit 触发 onResize→stash→Rust drain 发 `TerminalResize`。display:none 下尺寸为 0 跳过,避免 FitAddon 零宽框抛错。② `open_conversation` 入口加 `consultation_runs.contains_key` 短路(覆盖活 PTY + PTY 刚死等 settle 清理两种),防双 spawn——否则旧 handle 的 `ConsultationEnded` settle 会误清新 handle(HashMap insert 覆盖旧 key 后 remove 取新 cr)。③ 清 hook doc 注释里残留旧方法名 `set_issue_claude_session_id`。
  - **defer(记此处,不擅加脆弱路径)**:`restoring_issue` 板级 local signal 在 resume 失败时卡死「恢复中…」——PTY 死→`pty_restoring` 清、`pty_active` 仍 false→`resume_ready` false→local signal 无显式 clear 路径,永久贴到切项目/面板。严重度低(自愈;触发为 resume 失败,主要首次配环境)、纯文案。正确修法需 board 层订阅 `pty_restoring`/`pty_active` 的 reactive effect,但 `OpVm` 是渲染期读值的平结构、无裸 signal 可订,加 plumbing 风险/收益不划算(守「不为向后兼容留旧路径」不加脆弱 reactive 路径)。后续若动 board reactive 层再顺手清。
  - **defer(§8 seam 粒度偏差,非阻塞)**:§8 呼唤的 `PtyBackend` trait seam 实际未提取——平台分叉直接写在 `InteractiveCliExecutor::run_skill_pty` 内(`#[cfg(windows)]` conpty-oxide,非 Windows trait default `Err`)。§8 核心「上层不感知平台」已满足(`TerminalManager` 不碰平台类型);但 seam 粒度是「换整个 executor」而非「只换 PTY 后端」。macOS 适配时可二选一:在 `run_skill_pty` 内加 `#[cfg(target_os="macos")]` 分支,或提取 `PtyBackend` trait。当前不提取(不擅扩 scope)。
- **收口阶段(V1-TermClose,2026-08-07,见 `issue2-all-issues-terminal-runs.md`)**:用户点 ▶跑 自建无技能 issue 没进终端,根因 `is_interactive_skill` 只放行 north-star-discovery/metrics-binding 两技能。拍板:所有 issue ▶跑 都走嵌入终端,issue 脚本调度路径退场;多 agent 不删,转 prompt 驱动(技能方法论讲清 claude 用 SubAgent 调度,per-agent 战绩不适用于 issue 活,PTY 看不见);非 issue 命令(RunStagePlaybook/hub workflow/cron)仍用阶段循环机器。三阶段全落地:
  - **阶段1(✅ V1-TermClose1,commit c4d5b24)**:① `run_issue_now` 删 `is_interactive_skill` 交付门 + 咨询整块去技能门(任何有 conversation 行的 Done/InReview 可续聊);② 无技能 issue 用标题+描述作位置 prompt(auto-submit),技能正文+蒸馏块+目录块并入系统提示词(经验复利不静默丢失);③ `build_bridge_system_prompt` 空 slug 显「未关联技能」、非空未知仍显「你正在执行技能」;④ `poll_interactive_inreview` 去技能门;⑤ 删 `run_issue_body`/`run_issue_backgrounded`/`SettleOutcome::PhaseLoop`/PhaseLoop settle 臂/`FinalizeCtx.heads_workspace`+`head_before`(死字段)。产品可见变化:issue 的 InReview 改由 agent 自提 MR + poll 检测,无技能/无 MR 的活诚实停在 InProgress。
  - **阶段2(✅ V1-TermClose2,commit d427203)**:删 `issue_run_tail` 整函数(create_mr/transition InReview/Blocked)+ `run_issue_settle` 的 else 臂 + `interactive` 变量(删后恒真,去 if/else 只留 interactive 块,保留 ConsultationEnded 早返回)+ `ActiveRun` 的 `proj`/`issue_ws`/`pr_eligible` 三字段(只服务已删的 issue_run_tail,grep 确认无别的读取方)+ `IssueRunPrep` 的 `pr_eligible` 字段(删 ActiveRun 字段后无读取方)+ `prepare_issue_run` 的 `issue_brief`/`extra`/`spec.prompt`/`spec.phase_prompts` 注入(issue 全转终端后对 interactive 死代码;保留 `spec.name`/`spec.skills` 的 uses 记账 + `standard_refs`/`distilled_refs` 计算)。UI 门控:方法循环卡(来自 `stage_workflow`)加 `!op.pty_active` 门控——issue 终端会话不显(误导——issue 无 phase loop),阶段循环会话显;Chat 臂/沉淀按钮/RunOutputs/TerminalWidget 保留不改(语义已对)。无 schema 变更。
  - **阶段3(✅ V1-TermClose3)**:examples retarget + 文档回写。`adversarial_loop.rs` A/B/C 从 `RunIssue` retarget 到 `RunStagePlaybook`(阶段循环机器仍在,对抗循环还在那;断言改读 `list_all_workflow_runs` 按 session 过滤,不再断言 issue 状态);`agent_cli_routing.rs` 删除(issue→agent 路由前提消失——issue 终端会话不用 Engine 的 MockExecutor,agent_cli 路由是阶段循环概念,不适用于 issue);`verify_c8_standard_trio.rs` 断言从 `list_runs_for_issue`(workflow_run 行)改读 issue 状态(run_first=true → InProgress,run_first=false → 不转;interactive 不再留 workflow_run 行);`verify_skill_materialize.rs`/`verify_c13_draft_mock_lock.rs`/`practice_aihot.rs`/`practice_first_loop.rs` 改 stale doc 注释;`incubate_issue.rs` 不 dispatch RunIssue,无改。指南 u6/m4 + code-schemes + 铁律表回写。
  - **铁律影响(收口阶段)**:Done 仍人点(issue 终端会话不自动 Done,InReview 改 agent 自提 MR + poll 检测,无 MR 的活诚实停在 InProgress);咨询不 settle(ConsultationEnded 早返回,不碰 active_run/状态机/settle-once);MR 改 agent 自提 + poll(不再 buddy 脚本 create_mr);无 schema 变更(三个阶段都不碰 schema)。
  - **偏差/未决**:默认系统提示词/默认 skill 是后续 V1 催熟设计点(进维护指南 m4,本次不做);多 agent 在会话内由 claude 用 SubAgent 调度的写法靠技能方法论 prompt 讲清(PTY 看不见 claude 内部调度,per-agent 战绩不适用于 issue 活);`restoring_issue` 板级 local signal 在 resume 失败时卡死「恢复中…」的 defer 仍保留(见上方全量检视 defer 段)。
- **Bug1 降级修复(V1-TermDemote,2026-08-07)**:合入(Done)/转评审中(InReview)后 active_run 不释放 → 同项目别的 issue 跑不了。根因:`poll_interactive_inreview`(转 InReview)和 `TransitionIssue`(Done 边)都不碰 active_run;claude 提完 MR 往往不退出,PTY 活着 → active_run 一直挂 → `run_issue_now` 串行锁挡住同项目别的 issue。修复:issue 离开 InProgress(→InReview 或→Done)且仍持锁 + PTY 仍活时,降级交付为咨询(`demote_delivery_to_consultation`):清 active_run(放锁)、handle + worktree guard 迁到 `consultation_runs`,PTY + worktree 都留(不杀、不清,用户明确要求)。降级时补记 skill uses(首次 run;resume 不记 settle-once);不记 agent(Done 边 8845 已记 / InReview 留到 Done 边记 —— agent 永远只在 Done 边记一次)。触发点:`poll_interactive_inreview` 转 InReview 后 + `TransitionIssue` 的 `newly_done` 边(`MergeIssuePr` 内部 dispatch 到这,自动覆盖)。降级后 PTY 退出时 spawn 闭包仍发 `Interactive`(不是 ConsultationEnded)→ `run_issue_settle` 的 None 分支 / straggler 分支调 `cleanup_demoted_consultation` 按咨询退出收尾(close PTY + drop guard + forget session),skill_output 忽略(降级时已记账)。PTY 已死时不降级(restore active_run),让待处理 settle 走正常 finalize。铁律:Done 永不自动(降级不改状态);settle-once(降级后退出不重复 finalize);agent 只在 Done 边记一次。
  - **诚实缺口(记此处,不擅改)**:降级后的 PTY 是按交付起手的(`build_startup_plan` / `build_resume_plan`),没带 §6.2 咨询 prompt(「这活已验收,新交付请另开一件活」)。这条要等下次「续聊」resume(`build_consultation_resume_plan`)才注入。降级只放锁不杀进程,PTY 里的 claude 仍有完整 CLI 能力——与 §6.3 诚实口径一致(行为约定不是技术只读)。不假装已注入。
  - **范围**:`→InReview` + `→Done` 两个触发点(降级不杀进程,无损,§7.2 要评审中也不占锁)。`→Blocked` 等其他出 InProgress 的边暂不处理(follow-up:Blocked 的 PTY 该杀该留需另拍,本次不擅扩)。
- **Bug2 焦点同步(V1-TermFocus,2026-08-07)**:左侧 session 卡 ↔ 嵌终端焦点串台。根因:`SelectSession` 只设 `active_session`(驱动老 Chat + 左侧高亮),不调 `focus_conversation`;终端可见性由 `focused_conversation` 驱动,只有 issue 看板点卡 / 续聊 / ▶跑 调 `focus_conversation`。而 `focus_conversation` 也不回写 `active_session`。双向脱节。修复:左→终端——`SelectSession` 处理时解析 session→issue(session title 是 `#N 标题`,按 number+title 匹配 issue),有活 PTY → `focus_conversation`,无活 PTY 但有 `claude_session_id` → 走 `run_issue_now`(与 `OpenIssueDetail` 一致),解析不到(纯阶段记录)→ 保持原行为。终端→左——`focus_conversation` 切焦点时回写 `active_session` 到该 issue 对应的阶段记录(按 `run_sess_title` 反查 SessionId)。不杀 peer PTY(切卡只切显示/键盘);不动交付锁/merge 语义。
- **Bug2 收口补洞(2026-08-10)**:V1 合入后用户仍报「阶段记录切换看不到会话、看板切换正常」。根因不是焦点双向本身失效,而是左→终端分支**覆盖不全**:旧 `sync_session_to_terminal` 只在「已有 `claude_conversation` 且(活 PTY | 非空 `claude_session_id`)」时动作;缺行或 hook 未回填 session_id 时静默 no-op,同时 `SelectSession` 用 `let _ = sync` 吞掉 resume 错误 → 左侧高亮了、工作流区仍是空 Chat/无终端、无 toast。看板因直接 `RunIssue` 不受影响。修法:匹配到 issue 后**一律** `run_issue_now`(与 ▶跑/续聊等价);错误上浮打 `UiNote::Error`;纯 stage-playbook 标题仍 early Ok。命令层回归:`select_session_focus_tests`(活 PTY 切焦点 / 空 session_id 起手 / resume 错误上浮)。
- **Bug2 再发(2026-08-10 下午)**:重启后侧栏能切;一经看板点开某张 issue,侧栏再点「没用」。两层半套叠在一起:① UI——`IssueDetailOverlay` 用 `position:fixed;inset:0` 盖住整窗(含左侧阶段记录),点卡开弹层后侧栏点击落在遮罩上;② 命令——`OpenIssueDetail` 仍留旧窄门(只在活 PTY|非空 session_id 时切终端),与已收口的 `SelectSession→run_issue_now` 不等价。修法:弹层改 `absolute` 挂在看板中栏 `position:relative` 根上(不再盖 LeftRail)+ 点遮罩可关;`OpenIssueDetail` 有 conversation 行时一律 `run_issue_now`;`SelectSession` 成功后清 `issue_detail`。回归:`select_session_still_switches_after_open_issue_detail`。
- **Bug2 再发·侧栏命令链仍短(2026-08-10 傍晚)**:用户反馈修遮罩后仍「点几次侧栏 CLI 没了、看板还能唤」。SubAgent 对照:侧栏只发 `SetPanel+SelectSession`,看板 ▶跑发 `StartSession+RunIssue+SetScope+SetPanel+SelectSession`;终端非焦点用 `display:none` 且不 remount → 侧栏只翻 focused 时 xterm 易停在 0×0;另有 `active_run`/`consultation_runs` 在 PTY 已死时仍 early-return focus 死 id。修法:侧栏 `wake_session_like_board` 与看板同一条命令链;焦点回来强制 `__bw_term_refocus`;zombie 锁/`consultation_runs` 无 live 时放行再 spawn。回归:`run_issue_now_respawns_when_active_run_zombie`。
- **Bug2 再发·跨阶段宿主生命周期(2026-08-11)**:用户钉清复现——侧栏 A→B 正常;进构建点 C 可见;再点 A/B 无 CLI;去 Issue 看板后再点 A/B 又好。根因:终端挂在 `WorkflowStage` 里,`SetScope` 仍走同一 `Panel::Workflow` 臂;看板治愈是 Issues↔Workflow **换了不同组件 render_fn** 整树卸载再 `term_init` re-attach。
  - **假修(已撤回)**:`LiveTerminalHost` 挪出阶段树——flex 底槽把终端裁出可视区(Hook toast 仍响)。
  - **假修(空操作)**:给唯一子节点 `WorkflowStage { key: stage }`——Dioxus 0.7 的 `diff_vcomponent` **不读** lone child 的 key,等于没写。
  - **现行修法(SubAgent 对照 dioxus-core 0.7.9)**:① `WorkflowPanel` 用 keyed `for` 包 `WorkflowStage`(走 Fragment→`diff_keyed_children`,跨阶段真 remount);② `TerminalWidget` 的 key 带上 `stage_kind`(cid-only key 会跨 SetScope 保活、留下游离 xterm)。`__bw_term_refocus` 保留 re-home+尺寸重试。命令链不改(侧栏已与看板对齐;缺口在 UI 生命周期)。
  - **诊断日志(2026-08-11,已撤)**:排查期曾写 `term_focus_log` → `%TEMP%/bw-term-focus.log`。复验通过后已删除,不留开关/模块。
  - **半修收口(2026-08-11 傍晚)**:复现日志钉死——回原型再点侧栏时 **kernel 已 focus 对 cid**,但之后 **零条** TerminalWidget UI 日志。根因:Dioxus 0.7 的 `use_effect` 不订阅裸 `focused: bool` prop,跨阶段 remount 把非焦点终端以 `display:none`/0×0 挂上后,侧栏再点只翻焦点、`__bw_term_refocus` 永不重跑。看板治愈仍是 Issues↔Workflow 换 `render_fn` 整树 remount。修法:`TerminalWidget` 用 `use_reactive((&focused,&cid_str), …)` 包住 refocus effect。keyed-for 仍保留(跨 SetScope remount 必要,但不够)。
  - **再发(2026-08-11 17:45)**:`use_reactive` 已绿(focus 后有 TerminalWidget 日志且 refocus 返回 ok),用户仍「看不到」。根因升级:`display:none` 下 `term.open`/FitAddon 落在 0×0,之后 fit/refocus 假成功、canvas 仍空。修法:① 非焦点改为离屏固定 800×360 保尺寸(仍挂字节泵);② TerminalWidget key 带 `f`/`h`,焦点切换强制 remount 到可见 DOM(对齐看板治愈);③ refocus/re-attach 补 `refresh`。用户复验通过后撤掉临时 `term_focus_log`(%TEMP%/bw-term-focus.log)。
  - **工作流纵向占满(2026-08-11)**:焦点终端横向已满、纵向停在 `min-height:320`。修法:Workflow 活 PTY 时中栏→阶段树改 flex 列吃高;`TerminalWidget` 焦点态 `flex:1;min-height:0`,xterm 宿主 `height:100%`;有 PTY 时不再渲染空 Chat 提示占位。

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
