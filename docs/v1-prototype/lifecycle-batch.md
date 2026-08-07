# V1 生命周期板块 · 对齐 + 设计(本会话开发)

> 30 秒导读:本文件是第三板块(项目生命周期/治理)在本会话的对齐 + 设计,覆盖 5 件(3 代码 + 2 文档)。基于 `docs/v1-prototype/legacy-analysis-lifecycle.md` 深度分析的推荐方案 A。守 CLAUDE.md 铁律,逐 commit 不 push。

## 范围(对齐)

| # | 遗留 | 类型 | 方案 | 工作量 |
|---|---|---|---|---|
| 1 | W3-9 删项目留孤儿目录 | 代码+UI | 只删 buddy 自建 + 确认文案标工作目录 | 中 |
| 2 | W1-1 创建偷推仓+静默吞失败 | 代码+UI | `let _=` 改 toast + opt-out 开关(默认开) | 中 |
| 3 | W2-2 历史会话卡删不掉 | 代码+UI | DeleteSession(硬删 session+message,清 issue 外键引用) | 中 |
| 4 | W2-4 m6 脚注 | 文档 | collect_kind 行加 legacy 脚注 | 小 |
| 5 | W2-5 Hub 规范全生命周期段 | 文档 | standards 补段 | 小-中(可选,本批后做) |

W3-5(wall 下钻)/ W1-4(standards 打磨):保持现状,本轮不做。

---

## 1. W3-9 · 删项目清孤儿目录

**现状**:`Command::DeleteProject` handler(`lib.rs:9231-9241`)只调 `store.delete_project(id)`,不碰磁盘。buddy 自建 clone 在 `workspaces_root` 下,命名 `workspace_slug(name,id)` = `<slug>-<uuid8hex>`(`lib.rs:9378`)。

**改法**:
- handler 改:**先缓存** project 行的 `name`/`workspace_path`(delete_project 后行就没了),再 `store.delete_project(id)`,再判别删目录。
- 判别 `is_buddy_built_clone(path, name, id, workspaces_root)`:`path.starts_with(workspaces_root)` && `path.file_name() == workspace_slug(name,id)`。用户绑的目录(不在 workspaces_root 下)绝不删。
- 删目录用 `std::fs::remove_dir_all`,失败只 `eprintln!`(DB 已删,目录残留可手动清,比反向安全)。
- `workspaces_root` 从 `App` 读(`App::workspaces_root: Option<PathBuf>`)。
- UI(`wall.rs:180-204` 两步确认):确认文案从「删除后不可恢复」改成「将删除项目数据 + 工作目录 `<path>`(不可恢复)」;若不是 buddy 自建(用户绑目录),文案显「仅删除项目数据(工作目录 `<path>` 保留)」。

**铁律**:不动。删磁盘不属铁律;先删 DB 后删目录(事务顺序)。

## 2. W1-1 · 创建偷推仓 + 静默吞失败

**现状**:`write_charter`/`write_component_standards`(`lib.rs:6498/6502` CreateProject,`6866` CompleteCreation)用 `let _ =` 静默吞失败。UI(`create.rs:859`)硬编 `workspace: None`,无 opt-out 开关。

**改法**:
- `let _ =` 改成 `match`:失败 `emit Event::ActionProgress { name:"写入章程/组件标准", state: Fail }` toast(「报告不代答」补齐)。
- opt-out:`Command::CreateProject` 加字段 `write_standards: bool`(默认 true)。handler 在 `write_charter`/`write_component_standards` 外包 `if write_standards`。`create.rs` IntentCard 加 checkbox「buddy 在仓里写章程与组件标准并推送」(默认勾选),传 `write_standards`。
- push_head(CompleteCreation)已有 toast,不动。

**铁律**:不动。写章程是 owned workspace 内操作;`let _=` 改 toast 是补齐纪律不是动铁律。

## 3. W2-2 · DeleteSession

**现状**:无 `delete_session` store 方法、无 Command、会话卡(op.rs `SessionCard`:347)无 ×。`session` 表(`schema.sql:122`),`issue.session_id` 可空(`schema.sql:379`)。

**改法**:
- store 加 `delete_session(id)`(`sqlite.rs`):事务内 `DELETE FROM message WHERE session_id=?` → `UPDATE issue SET session_id=NULL WHERE session_id=?`(清 dangling 外键,不删 issue)→ `DELETE FROM session WHERE id=?`。
- `Command::DeleteSession(id)` + handler:`store.delete_session` + `refresh_*` + emit。
- UI(op.rs `SessionCard`):加 × → 二步确认「删除此会话记录?不可恢复」→ `Command::DeleteSession`。

**铁律**:不动。session 是工作产物(非 observation 只追加、非 issue 状态机);清 issue.session_id 外键引用不算动 issue。

## 4. W2-4 · m6 脚注

`buddy-guide.html:858` `collect_kind = script|manual` 行加脚注:「代码枚举尚含 legacy kind(github/codehub/bw/connector),UI 已 forward-correct 标 legacy·迁 script,枚举收口见 W3-2」。

## 5. W2-5 · Hub 规范(可选,本批后做)

`crates/bw-core/src/standards.rs` 四份 standards 补「全生命周期(创建/编辑/删除/归档)」操作说明段。纯文档,不阻塞功能,本批时间紧可单独排。

---

## 开发顺序

W2-2(最独立)→ W3-9(最痛)→ W1-1 → W2-4 脚注 → W2-5(若时间够)。每件逐 commit 不 push,门禁 + cargo test。
