# V1 遗留问题深度分析 · 项目生命周期 / 治理规范组

> **30 秒导读**：这份报告覆盖 W1-1/W1-4、W2-2/W2-4/W2-5、W3-5/W3-6/W3-9 共八条遗留，集中在「buddy 怎么管用户的项目仓、组件规范、会话与指南」这一组。每条给出现状根因（带 `文件:行号`）、2-3 个方案选项含取舍、推荐、工作量评估、是否动铁律。读者：制定下一步策略的产品/开发决策者。作数：基于 v1 分支当前代码（2026-08-06）。

---

## W1-1 · buddy 自动写并 push 用户项目仓提交

### 现状根因

创建流程在两个时机往用户项目仓自动写文件并推送远端：

| 时机 | 函数 | 调用点 | 错误处理 |
|---|---|---|---|
| CreateProject（建仓/接入后） | `write_charter(…, "开篇")` | `crates/bw-app/src/lib.rs:6498` | `let _ =` 静默吞 |
| CreateProject | `write_component_standards(…)` | `crates/bw-app/src/lib.rs:6502` | `let _ =` 静默吞 |
| CompleteCreation（完成创建） | `write_charter(…, "完成创建")` | `crates/bw-app/src/lib.rs:6866` | `let _ =` 静默吞 |
| CompleteCreation | `push_head(workspace_path)` | `crates/bw-app/src/lib.rs:6880` | `match`，失败有 toast |

- `write_charter`（`lib.rs:9573`）和 `write_component_standards`（`lib.rs:9599`）只在 `is_owned_workspace(dir)` 返回 true 时才写——该函数（`crates/bw-engine/src/workspace.rs:332-351`）检查根提交作者是否为 `"Builders' Workbench"`，bound（用户绑定已有仓）永不写。
- `commit_file`（`workspace.rs:98-140`）提交时硬设作者 `Builders' Workbench` + `workbench@local`。
- `push_head`（`crates/bw-engine/src/github.rs:113-117`）执行 `git push origin HEAD`，只在 `remote_path` 非空时调用。
- **UI 侧无 opt-in 开关**：`crates/app-desktop/src/screens/create.rs` 的 `IntentCard` 确认闭包（`create.rs:859`）硬编 `workspace: None`，`CompleteCreation` 的 `run_first` 硬编 `false`（`create.rs:872`）。用户无法通过界面选择「不写章程/不推送」。
- `let _ =` 意味着章程/标准写失败时用户完全无感——既不 toast 也不记日志。

### 方案选项

| 方案 | 描述 | 优点 | 缺点 | 工作量 |
|---|---|---|---|---|
| **A. 默认开 + 可关** | 创建流加一个 opt-out 开关「buddy 在仓里写章程与组件标准并推送远端」，默认勾选 | 保持当前行为不变；给有自己 commit 规范的项目一条退路 | UI 改动（create.rs 加 checkbox）；用户可能不理解含义 | 小（~2h） |
| **B. 默认关 + 可开** | 同上但默认不勾选；用户要主动选才写+推 | 最保守，绝不撞用户 commit 规范 | 新项目默认没有章程/标准文件，agent 创建组件时缺参照；降低模板能力的价值 | 小（~2h） |
| **C. 写不推 + 手动推** | 默认仍自动写章程/标准（本地 commit），但 `push_head` 改为不自动推，用户自行 `git push` | 仓里有文件（agent 可参照），但不撞远端历史 | 用户忘记推 → 远端没有这些文件 → PR base 缺标准 → 下游可能找不到 | 中（改 push 逻辑 + UI 提示） |

### 推荐

**方案 A（默认开 + 可关）**。理由：
1. 模板能力（章程 + 组件标准）是产品核心价值之一，默认关会削弱它。
2. 有自己 commit 规范的项目是少数，opt-out 够用。
3. `let _ =` 静默吞失败应同时改为至少记一条 `eprintln!` + `ActionProgress::Fail` toast——当前 `push_head` 已经有 toast，但前面的 `write_charter`/`write_component_standards` 失败时用户完全无感，这是「报告不代答」纪律的违反。

### 并行 push 分叉风险

buddy 在自己的 workspace clone（`workspaces_root` 下的 `<slug>-<uuid8>` 目录）里 commit + push；用户若在独立 worktree 里也 push 同一 main 分支，可能产生分叉。但实际风险低：
- buddy 的 push 发生在 CompleteCreation（创建流结束时），不是高频操作。
- buddy push 的是章程 + 标准文件，与用户开发内容不重叠。
- 若分叉，`git push` 会报 non-fast-forward 错误，buddy 的 `push_head` match 会 toast 失败提示用户手动处理，不会强推。

**worktree 感知**：buddy 不监听用户 worktree 状态，无法主动告知「我往 main 推了东西」。缓解：CompleteCreation 的 toast 已经说「已推送」，用户看到后知道要 pull。做主动感知成本高（需监听 git 事件），收益低，不建议 V1 做。

### 是否动铁律

不动。写章程/标准是「在 owned workspace 里做事」，不违反任何铁律。`let _ =` 静默吞失败违反「报告不代答，读回为证」纪律——改 toast 是补齐纪律，不是动铁律。

---

## W3-9 · DeleteProject 不清磁盘 workspace clone

### 现状根因

- `Command::DeleteProject` handler（`crates/bw-app/src/lib.rs:9231-9241`）：只调 `self.store.delete_project(id)` + 切走 active project + 刷新列表。
- `delete_project`（`crates/bw-store/src/sqlite.rs:793-878`）：事务内删 issue/artifact/workflow_run/cron_task/connector/agent/skill/workflow_spec/metric/op_stage/session/message/weekly_review/handoff/observation/project 全表行。**不碰磁盘**。
- buddy 自动建的 clone 目录在 `workspaces_root`（`crates/app-desktop/src/kernel.rs:483-492`，`BW_WORKSPACES` 环境变量或 `<db_parent>/workspaces`）下，命名为 `workspace_slug(name, id)` = `<slug>-<uuid8hex>`（`lib.rs:9378-9394`，8 个十六进制字符，不是 LEFTOVERS 说的 6 个）。
- **判别 buddy 自建 vs 用户绑定的关键**：DB 的 `project` 表没有「is_bound」列（`schema.sql:18-`）。`set_workspace(id, path, allow_commands)`（`sqlite.rs:947-958`）第三参数是 `allow_commands`，不是 bound 标记。判别只能靠路径：
  - buddy 自建：`workspace_path` 在 `workspaces_root` 下 + 目录名匹配 `<slug>-<uuid8>` 模式
  - 用户绑定：`workspace_path` 不在 `workspaces_root` 下（用户自己的目录）
- `is_owned_workspace`（`workspace.rs:332-351`）检查根提交作者 == `"Builders' Workbench"`——buddy 自建的仓（含 codehub/github 新建 + 本地 mint）都满足，但用户绑定的已有仓不满足。可用作二次确认。
- **当前 UI 无 workspace_path 输入**：`create.rs` 硬编 `workspace: None`（`create.rs:859`），所有从 UI 创建的项目都走自动建仓路径。用户绑定已有目录的路径目前只从代码/示例可达。
- 全代码库唯一 `remove_dir_all` 在 `workspace.rs:311`，是清 worktree sibling 目录，与删项目无关。

### 判别伪代码

```
fn is_buddy_built_clone(project: &ProjectRow, workspaces_root: &Path) -> bool {
    let ws = Path::new(project.workspace_path.trim());
    // 1. 路径在 workspaces_root 下
    if !ws.starts_with(workspaces_root) { return false; }
    // 2. 目录名匹配 <slug>-<uuid8hex> 模式
    let name = ws.file_name()?.to_str()?;
    let slug = workspace_slug(&project.name, project.id); // 可重算
    name == slug
    // 3. 可选二次确认：is_owned_workspace(ws) == true
}
```

### 方案选项

| 方案 | 描述 | 优点 | 缺点 | 工作量 |
|---|---|---|---|---|
| **A. 只删 buddy 自建 + 确认弹窗** | DeleteProject handler 里判别：`workspace_path` 在 `workspaces_root` 下 + 目录名匹配 `workspace_slug` → 删目录。否则不动。删前 UI 弹确认「将一并删除工作目录 `<path>`，不可恢复」 | 清理孤儿；用户绑定目录绝不删；二次确认防误操作 | 需在 app-desktop 层加确认 UI；需在 bw-app handler 传 `workspaces_root` | 中（~4h） |
| **B. 只删 buddy 自建 + 不弹窗** | 同上但不加确认，直接删 | 简单 | 不可逆操作不确认太危险；用户可能想保留产物取证 | 小（~2h） |
| **C. 不删 + 只提示** | 不自动删，DeleteProject 后 toast 提示「工作目录 `<path>` 仍保留在磁盘，如需清理请手动删除」 | 最保守；用户完全掌控 | 孤儿目录继续累积；用户不知道路径在哪 | 小（~1h） |

### 推荐

**方案 A（只删 buddy 自建 + 确认弹窗）**。理由：
1. 用户实测残留 3 个孤儿目录是真实痛点。
2. 删目录不可逆，必须确认。
3. 路径判别 + 目录名匹配 `workspace_slug` 可靠区分 buddy 自建 vs 用户绑定。
4. 用户绑定目录（不在 `workspaces_root` 下）绝不删——用户可能想保留取证。

实现要点：
- `DeleteProject` handler 需在 `store.delete_project(id)` 之后、刷新列表之前，读 project 行的 `workspace_path`（注意：要在 delete_project 之前读，或 handler 里提前缓存）。
- `workspaces_root` 已在 `AppState`（`App::workspaces_root: Option<PathBuf>`）。
- 确认 UI 用 `ProjectCard` 已有的 `confirming_delete` 模式（`wall.rs:180-204`），把单行 `×` → 两步 `确认删除? / 确认` 改成三步 `确认删除? / 确认（含工作目录）/ 确认（仅数据库）`，或者直接在确认文案里写清「将删除工作目录 `<path>`」。

### 是否动铁律

不动。删磁盘目录不是铁律管辖范围。但要注意：删目录前必须先删 DB 行（否则 DB 行没了但目录还在，或反过来目录删了但 DB 行还在指向不存在的工作区）——事务顺序是先 `delete_project`（DB）成功后再删目录，目录删失败不回滚 DB（DB 行已删，目录残留可后续手动清，比「DB 还在但目录没了」安全）。

---

## W2-2 · DeleteSession 命令设计

### 现状根因

- 重复点「▶ 跑」曾堆积重复「阶段记录」卡，根因是每次 mint 新 `SessionId`。
- 已修：`existing_issue_session`（`crates/app-desktop/src/screens/op.rs:573-582`）按 `(stage_kind, title)` 去重，复用既有 session id，不再堆积新的。
- **历史脏数据无清理路径**：没有 `DeleteSession` 命令、没有 store 方法、没有 UI 按钮。
- `session` 表（`crates/bw-store/src/schema.sql:122-133`）：id, project_id, stage_kind, kind, title, snippet, status, created_at, updated_at, rev。`status` 有 `'active'|'archived'|'done'` 三值，但没有路径把它翻成 done（P11 遗留）。
- `message` 表（`schema.sql:135-142`）引用 `session_id`，有索引 `idx_message_session_seq`。
- `delete_project` 已删 session + message（`sqlite.rs:807, 861`），但那是项目级清理，不是单会话清理。

### 方案选项

| 方案 | 描述 | 优点 | 缺点 | 工作量 |
|---|---|---|---|---|
| **A. 硬删 session + message** | store 加 `delete_session(id)`：事务内先删 message 再删 session。UI 在会话卡加 `×` 按钮 → `Command::DeleteSession(id)` | 彻底清理脏数据 | 硬删不可逆；可能误删有用的会话记录 | 小（store ~20 行 + UI ~30 行 + Command 枚举 1 行） |
| **B. 归档（soft delete）** | store 加 `set_session_status(id, Archived)`。UI 把会话卡标记为已归档（灰显或折叠） | 可恢复；非破坏 | 脏数据仍在列表里占位（只是灰显），用户体感不清爽 | 小（store ~10 行 + UI ~20 行） |
| **C. 不做，靠删项目带走** | 不加 DeleteSession；用户重建项目时 delete_project 自然带走所有会话 | 零工作量 | 存量项目上脏数据无法清理；用户不想删项目就清不了 | 零 |

### 推荐

**方案 A（硬删 session + message）+ 限定只清自己名下会话**。理由：
1. 会话卡是「阶段记录」轨的工作产物，不是只追加的事实记录（observation/issue 那种）。删掉一张空壳会话卡不违反任何铁律。
2. `delete_session` 只删 session 行 + 其名下 message 行，不动 issue、不动 workflow_run。issue 的 `session_id` 列是可空的（`schema.sql:379`），session 删了 issue 行不受影响。
3. 确认弹窗：会话卡 `×` → 二步确认「删除此会话记录？不可恢复」。
4. 这是用户明确需要的功能（LEFTOVERS W2-2 处置段写「如果后续在存量项目上还是想清理，需要单独排 DeleteSession 这个功能」）。

### 是否动铁律

不动。session 不是 observation（只追加），不是 issue 状态机。session 是工作产物（会话记录），删掉不影响健康推导链、不影响 issue 状态机、不影响记账（settle-once 是 issue 级的，不是 session 级的）。

---

## W2-5 · Hub 四组件完整规范草案

### 现状根因

当前 `.bw/` 目录约定和四组件（skill/connector/agent/cron）的管理现状：

| 组件 | 源正本（source of truth） | DB 表 | 管理入口 | 完整规范状态 |
|---|---|---|---|---|
| **skill** | `docs/skills/<slug>/SKILL.md`（官方包）或 DB 行（自建/蒸馏） | `skill` + `skill_file` + `skill_stage` | SkillHub UI | ✅ 有 standards.md（`bw-core/src/standards.rs:84-160`），字段/蒸馏/正文规范齐全 |
| **connector** | `.bw/connectors.toml` + `.bw/scripts/` | `connector` | Hub 侧栏 + SyncConnectors | ✅ 有 `connectors_file.rs` 解析器 + `docs/connectors-toml-format.md` 契约；ConnectorDef 有 name/kind/script/command/output，`deny_unknown_fields` |
| **agent** | DB 行（无源正本文件） | `agent` | agent 管理 UI（有限） | ✅ 有 standards.md（`standards.rs:23-81`），字段/五角色/自查清单齐全 |
| **cron** | DB 行（无源正本文件） | `cron_task` | cron 面板 | ✅ 有 standards.md（`standards.rs:224-267`），字段/no-hijack/自查清单齐全 |

**「完整规范未定」的真实含义**不是四份 standards 没写（它们已写且内容详实），而是：
1. 四组件的**全生命周期**（创建/编辑/删除/归档）没有统一定义——skill 有蒸馏+SkillHub 编辑，connector 有 toml+Sync，agent 只有 DB 直写，cron 只有 DB 直写。
2. `.bw/` 目录约定只覆盖了 connector 和 metrics，agent 和 cron 没有 source-of-truth 文件（它们是 DB-only 的）。
3. connector 的 `schedule` 字段（`connectors_file.rs` 的 `ConnectorDef` 没有这个字段——它只在 `docs/connectors-toml-format.md` 里提到，buddy 不读它建 cron 行，P9 遗留）。

### 方案选项

| 方案 | 描述 | 优点 | 缺点 | 工作量 |
|---|---|---|---|---|
| **A. 统一为 DB-only + standards 文档** | agent 和 cron 维持 DB-only（不做 source-of-truth 文件）；standards.md 已有；规范补「全生命周期」段（怎么创建/编辑/删除/归档） | 最小改动；与当前实现一致 | agent/cron 无 PR 审查能力（改了不进 git 历史） | 小（补 standards 文档 ~100 行） |
| **B. agent + cron 也进 `.bw/`** | agent 写 `.bw/agents.toml`，cron 写 `.bw/crons.toml`，走 PR 审查 + Sync | 四组件统一 source-of-truth 模式；全进 git 历史 | 工作量大（解析器 + Sync 命令 + UI）；agent/cron 变更频率低，PR 价值有限 | 大（~2 天） |
| **C. 只补 connector schedule 接线** | 不动四组件规范大框架，只把 `connectors.toml` 的 `schedule` 字段接上（读它建/更新独立 cron） | 解决 P9 遗留；用户可按指标配独立采集周期 | 改变「一条 Daily 全覆盖」的当前形态 | 中（~4h） |

### 推荐

**方案 A（统一为 DB-only + 补 standards 文档全生命周期段）**。理由：
1. 四份 standards.md 已经很详实（字段表 + 自查清单 + 范例），真正缺的是「怎么在 UI 里管理」的操作说明。
2. agent 和 cron 变更频率低（一个项目五个角色 agent 不常加，cron 也不常改），给它们做 source-of-truth 文件 + PR 审查的收益不抵成本。
3. connector 的 PR 审查有价值（脚本是代码），保持当前 `.bw/connectors.toml` + `.bw/scripts/` 模式。
4. P9（connector schedule 接线）单独排，不在 W2-5 范围内做决定——它是产品未决点（「一条 Daily 全覆盖」vs「按 schedule 拆独立 cron」），需用户拍板。

### 是否动铁律

不动。四组件规范是文档/设计层面的事，不碰类型/守卫/派生链。

---

## W2-4 · Phase 5 guide 补全清单（m6 + u3/u4 + 信号色 token）

### 现状根因

实际检查 `docs/guide/buddy-guide.html` 后发现，W2-4 描述的「partial」比实际情况更严重——大部分内容**已经填好了**：

| 条目 | 现状 | 实际缺啥 |
|---|---|---|
| **m6（指标与健康）** | `buddy-guide.html:832-921`，已有采数链主图、计划≠有数表、装表 Sync* 流程、关键表字段表、总览分层、信号四态灯、Example、事实源脚注 | m6 内容基本完整。一处需校准：L861「总览手填入口」提到 P10 未关——P10 已在 V1 任务 A 收口（BizMetricCard 嵌入 RecordInline），这句要改。另 L858 `collect_kind = script|manual` 是 forward-correct 目标态，代码里枚举还是 5 kind（W3-2），guide 写目标态没问题但脚注应注明 |
| **u3（找指标）** | `buddy-guide.html:350-403`，已有五步操作 + pipe 主图 + 你会得到 + merge PR 表 + 交互终端提示 + 截图 | 内容已完整。可补：P1（窄窗 ANSI）/ P3（单 PTY 无法回看历史会话）/ P11（Done 后会话状态不消失）的已知 UX 限制——但这些是 bug/遗留，不是 guide 该写的操作步骤，放进 callout 或不写 |
| **u4（绑数据）** | `buddy-guide.html:406-453`，已有四步操作 + pipe 主图 + 三件事核对表 + 定时器解释 + 截图 | 内容已完整。P13 副标题文案已改（中性前缀），guide 的 Example（L448）已反映 |
| **信号色 token** | `buddy-guide.html:16` CSS `:root` 里 `--sig-green:#6E8F62 / --sig-amber:#C98A2B / --sig-red:#B5462E / --sig-unknown:#9A938A`，与 CLAUDE.md 设计系统一致 | **已对齐**，不缺 |

### 结论

W2-4 实际**大部分已解决**，只余两处需校准：

1. **m6 L861**：「总览手填入口」行说 P10 未关——P10 已在 V1 任务 A 收口，该行需改成「BizMetricCard / 北极星灰卡已嵌入 RecordInline」。
2. **m6 脚注**：`collect_kind = script|manual` 是 forward-correct 目标态，应加一句脚注说明「代码枚举尚含 legacy kind，UI 已 forward-correct，迁移见 W3-2」。

### 推荐

只做上述两处校准，不补新内容。工作量极小（~30min 改两行 HTML）。

### 是否动铁律

不动。指南是文档，不碰代码/类型/守卫。

---

## W3-5 · wall HealthOverviewBar 下钻交互

### 现状根因

- `HealthOverviewBar`（`crates/app-desktop/src/screens/wall.rs:57-117`）：跨项目信号分布——green/amber/red/unknown 计数，green 隐身折成计数，非 green 出声。**无点击下钻**，是纯被动展示。
- `ProjectCard`（`wall.rs:120-207`）：点击整卡 → `Command::OpenProject(id)` 打开单个项目。没有「按信号分组过滤」的交互。
- `StageAxis`（`op.rs:126-188`）：进入单个项目后，per-stage 信号点在阶段轴上展示。点击 stage 切 scope，不是跨项目。

### 方案选项

| 方案 | 描述 | 优点 | 缺点 | 工作量 |
|---|---|---|---|---|
| **A. 保持现状** | wall = 跨项目分布概览；op = per-project 阶段轴细节。下钻靠 ProjectCard 点击进单项目 | 分工清晰；当前形态够用 | 想看「所有 red 项目」需逐个扫 | 零 |
| **B. 信号分组下钻** | HealthOverviewBar 的每个计数 chip 可点 → 过滤项目列表只显示该信号组 | 快速定位问题项目 | wall 的项目网格变成可过滤列表，交互复杂度上升 | 中（~3h） |
| **C. 加「需要关注」快捷区** | wall 顶部 HealthOverviewBar 下面加一行「需要关注的项目」横滚卡片（amber+red），点击进单项目 | 不改交互模式，只加信息密度 | 如果项目多，横滚卡片占空间 | 小（~2h） |

### 推荐

**方案 A（保持现状）**。理由：
1. V1 项目数量少（单人构建者通常同时 2-5 个项目），逐个扫不痛。
2. wall 的价值是「一眼知道全局」，不是「筛选管理」。过滤交互适合团队版（多 PC 协作），不适合 V1 单人。
3. 下钻交互是「锦上添花」，不是「不用不行」，不排优先级。

### 是否动铁律

不动。

---

## W3-6 · stats trio 去留

### 现状根因

- `OpVm.stats`（`crates/app-desktop/src/kernel.rs:205`）声明为 `ui::vm::StatCardsVm`，在 `build_vm`（`kernel.rs:1212-1223`）里填充。
- `StatCardsVm`（`crates/ui/src/vm.rs:517-525`）三字段：`workflows_total`（create 会话总数）、`routines_active`（已建 materialized stage 数）、`optimizing`（活跃 optimize 会话数）。
- **op.rs 零引用**：v2 ProgressAll（`op.rs:1692` 起）不读 `.stats`。数据留在 VM 里但不显示，是事实上的死字段。

### 方案选项

| 方案 | 描述 | 优点 | 缺点 | 工作量 |
|---|---|---|---|---|
| **A. 退场** | 从 `OpVm` 删 `stats` 字段，从 `build_vm` 删填充逻辑，从 `vm.rs` 删 `StatCardsVm` + `stat_cards()` | 清死代码；减 VM 体积 | 如果以后想回来要重写 | 小（~30min） |
| **B. 移到 workflow panel** | 在工作流面板（Panel::Workflow）的某处显示这三个数 | 信息有用（多少会话跑过、多少阶段已建、多少在优化中） | workflow panel 已有嵌入终端，空间紧张 | 小（~1h） |
| **C. 保持留白** | 数据留 VM 不删，不显示，注释标「保留待用」 | 零改动 | 死代码 | 零 |

### 推荐

**方案 A（退场）**。理由：
1. 「不为向后兼容留旧路径」是仓库明确原则。数据留 VM 但没人读，就是旧路径。
2. 三个数（会话总数/阶段数/优化中数）在当前 UI 里没有自然归属位置——它们不是项目健康指标（不进总览），不是单 Issue 状态（不进看板），不是工作流运行状态（不进 workflow panel 的终端区）。
3. 如果以后真需要，从 store 重算这几个数成本极低（几个 COUNT 查询）。

### 是否动铁律

不动。stats 是展示字段，不参与推导链。

---

## W1-4 · 四份组件 standards 内容打磨

### 现状根因

四份 standards 定义在 `crates/bw-core/src/standards.rs`：
- `AGENT_STANDARDS_MD`（L23-81）：字段表（name/role/skills/model/instructions/maturity + 派生 runs/wins/win_rate）、五角色范例、何时新建、自查清单。**内容详实**。
- `SKILL_STANDARDS_MD`（L84-160）：字段表（name/descr/category/stages/stage_origin/content/source/maturity/uses/distilled_from_issue/origin_agent）、蒸馏姿势、五方法论技能、正文规范、自查清单。**内容详实**。
- `WORKFLOW_STANDARDS_MD`（L163-221）：字段表（name/kind/prompt/goal/stage_ref/phases/phase_prompts/agents/skills/loop_*）、Static 子字段、主 workflow 定义方法、自查清单。**内容详实**。
- `CRON_STANDARDS_MD`（L224-267）：字段表（name/target/schedule/project_id/mode/issue_stage/issue_assignee/status/last_run*/next_run）、no-hijack 说明、自查清单。**内容详实**。

### 评估

四份 standards 已经是**高质量内容**：每份都有字段表（标明作者填 vs 系统派生）、范例、自查清单，与 `bw-store/schema.sql` 的真实字段核对过（注释 L17-20 明说）。W1-4 说的「内容打磨」更像是「随实践推进发现遗漏再补」，不是当前有明确缺口。

**可能需要补的内容**（依赖 W2-5 四组件完整规范落地后再定）：
1. cron standard 可补 `CollectMetrics` 这条路径说明（当前只讲 `run_workflow` 和 `create_issue` 两个 mode，没讲「一条 Daily CollectMetrics 覆盖项目全部 script connector」这个实际形态）。
2. workflow standard 可补交互式 PTY 路径说明（当前只讲 Executor 子进程执行，没讲 `run_issue_interactive` 的 PTY 路径）。
3. agent standard 的 `model` 字段说明已标注「诚实标签，不是路由配置」——这已够好，不需改。

### 推荐

**不在本轮打磨**。四份 standards 内容已足够 V1 使用。等 W2-5 四组件完整规范落地后，再统一补全生命周期段（创建/编辑/删除/归档的操作说明）。工作量评估：若要补上述 3 点，约 2h；若只维持现状，零。

### 是否动铁律

不动。standards 是写进项目仓的文档，不碰类型/守卫。

---

## 优先级建议排序

| 优先级 | 条目 | 理由 | 工作量 | 是否动铁律 |
|---|---|---|---|---|
| **P1** | **W3-9**（DeleteProject 清磁盘 clone） | 用户实测残留 3 个孤儿目录，不可逆操作需确认弹窗；判别逻辑明确（路径 + 目录名匹配） | 中（~4h） | 否 |
| **P2** | **W1-1**（`let _ =` 改 toast + opt-out 开关） | 静默吞失败违反「报告不代答」纪律；opt-out 开关保护有自己 commit 规范的项目 | 小（~2h） | 否 |
| **P3** | **W2-2**（DeleteSession 命令） | 用户明确需要（存量项目脏数据无法清理）；store + Command + UI 三层改动清晰 | 小（~2h） | 否 |
| **P4** | **W2-4**（guide 两处校准） | 工作量极小；m6 L861 P10 已关需纠偏 | 极小（~30min） | 否 |
| **P5** | **W3-6**（stats trio 退场） | 死代码清理，符合「不留旧路径」原则 | 小（~30min） | 否 |
| **P6** | **W2-5**（Hub 四组件规范补全生命周期） | 需设计判断；standards 已有，补操作说明段即可 | 小（~2h 文档） | 否 |
| **P7** | **W3-5**（wall 下钻交互） | 锦上添花，V1 不痛；单人项目少，逐个扫够用 | 零（保持现状） | 否 |
| **P8** | **W1-4**（standards 打磨） | 内容已足够；等 W2-5 落地后再统一补 | 零（本轮不做） | 否 |

### 总结

这组遗留**全部不动铁律**——没有一条碰类型/守卫/派生链/状态机。最痛的是 W3-9（孤儿目录）和 W1-1（静默吞失败），最易做的是 W2-4（改两行 HTML）和 W3-6（删死字段）。W2-5 和 W1-4 是文档/规范层面的事，不阻塞功能，可在功能遗留清完后再统一打磨。
