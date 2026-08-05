# 通用 skill 按五角色归类 · 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把技能库里 57 条无阶段归属的技能按五角色多值归类，并让归类真正到达执行现场——run 时按 Issue 阶段把本阶段技能目录写进 prompt、正文物化到工作区 `.claude/skills/`。

**Architecture:** 阶段归属从 `skill.stage_ref` 单值列迁到 `skill_stage` 关联表（多值），旧列真删；`skill.stage_origin` 一列表达「归类出处」，与关联表行数共同派生四态（挂 N 阶段 / 全阶段通用 / 已判定不属任何阶段 / 未归类）。归类值有三条来源：`bw-core` 里进 git 的静态表（65 条随包技能）、蒸馏技能按出处 Issue 派生、UI 人工覆盖（`stage_origin='manual'` 后 Boot 不再回填）。注入不塞正文（superpowers 单条正文 8732 字符就撑爆 6000 护栏），改为「目录进 prompt + 正文物化到工作区」，由 `claude` CLI 原生 skill 机制按需加载。

**Tech Stack:** Rust workspace（bw-core 零 IO/wasm32 可编译 · bw-store sqlx+SQLite · bw-app 编排 · ui 纯函数 selector · app-desktop Dioxus 0.7 hard-pin =0.7.9）。

**Spec:** [`docs/superpowers/specs/2026-08-05-skill-five-role-classification-design.md`](../specs/2026-08-05-skill-five-role-classification-design.md)

## Global Constraints

每个任务的要求都隐含包含本节。

- **不写、不留单元测试**（CLAUDE.md 核心纪律，2026-07-17 转向）。行为正确性靠 E2E：深链启动 + `sqlite3` 读回 + `/code-review`。本计划每个任务的「验证」步骤都是真实门禁命令 + SQL 读回，**不是** `cargo test`。
- **门禁（每个 commit 前全过，与 CI 完全一致）**：
  ```
  cargo fmt --all --check
  cargo clippy --workspace --exclude app-desktop -- -D warnings
  cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
  cargo check -p ui --target wasm32-unknown-unknown
  ./scripts/guard-kernel-ui-free.sh
  cargo check -p app-desktop
  ```
- **UI 无关内核**：`bw-core`/`bw-store`/`bw-engine`/`bw-app`/`ui` 五个 crate 禁依赖 dioxus/tauri/wry/leptos（`guard-kernel-ui-free.sh` 强制）。UI 改动只准进 `app-desktop`。
- **bw-core 零 IO**：`stage_catalog.rs` 只能是纯数据 + 纯函数，不得有文件/网络访问，必须 wasm32 可编译。
- **schema 迁移双守卫**：每改一处结构必须**同时**改 `crates/bw-store/src/schema.sql` 与 `crates/bw-store/src/sqlite.rs` 的迁移段。`CREATE TABLE IF NOT EXISTS` 对存量表**不会**加列。
- **schema.sql 的 blob 在迁移守卫之前无条件重放**：所以任何 `CREATE INDEX ... ON skill(<retrofitted column>)` 都**不能**写进 schema.sql（会让老库崩在 "no such column"）。索引一律写在 `sqlite.rs` 的 `add_column_if_missing` 之后。这条是 T7 踩过的真坑，`schema.sql:245-252` 有原始注释。
- **归类不触发 T11「编辑即脱离源头」**：任何写 stage 的路径，`SkillEdit.flip_to_self_built` 一律 `false`，`official_library` 列不动。
- **归类不记 uses**：目录注入的技能**不得**进 `spec.skills`（那个 Vec 会被 `run_workflow_inner` 拿去 `record_skill_use_by_name`）。
- **commit 约定**：每件独立 commit，代号前缀（如 `SR1 · …`），信息如实描述取舍。末尾附 `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>`。
- **数据库读回一律用真实库副本**，不改用户的日常库：
  ```bash
  cp ~/Library/Application\ Support/BuildersWorkbench/workbench.db /tmp/bw-verify.db
  ```
- **迁移验证只用 `verify_migration` example，绝不用 `verify_goal`**（2026-08-05 执行期发现的真坑，代价是 Task 2 交了一次假阳性证据）：
  - `verify_goal.rs:45` 读的是 `std::env::args().nth(1)` **位置参数**，不读 `BW_DB` 环境变量 —— 写成 `BW_DB=<db> … verify_goal` 会在 `$TMPDIR/bw_verify_goal.db` 上跑完全部用例，**根本没碰目标库**；
  - 更致命的是 `verify_goal.rs:51` 会 `remove_file(&path)` **先删库再建**，所以即使改用位置参数，"老库迁移"路径依然从未被验证——它验的永远是个全新库。
  - **识别假阳性的硬指标**：迁移后 `SELECT COUNT(*) FROM skill` 若等于 **10**（= bw-standard 8 + mohit 2，全新库 Boot 的播种量），说明你验的是新库不是老库。真实日常库副本应有 **65+** 条。
  - 正确工具是 `crates/bw-app/examples/verify_migration.rs`（Task 2 补建）：**只开不删**，跑 `Command::Boot`，打印迁移后的真实计数。用法 `cargo run -p bw-app --example verify_migration -- <db-path>`。

---

## File Structure

| 文件 | 职责 | 任务 |
|---|---|---|
| **Create** `crates/bw-core/src/stage_catalog.rs` | `StageOrigin` 枚举 + 65 条静态归类表 + 查表纯函数。零 IO、wasm32 可编译 | T1 |
| **Create** `crates/bw-app/examples/verify_stage_catalog.rs` | 静态表自证 example（条数/重名/五阶段计数），与既有 `verify_goal.rs` 同族 | T1 |
| **Create** `crates/bw-app/src/skill_materialize.rs` | 把候选技能写成工作区 `.claude/skills/<name>/SKILL.md` + 支撑文件 + `.bw-managed` 托管标记 | T7 |
| Modify `crates/bw-core/src/lib.rs` | 注册新模块 | T1 |
| Modify `crates/bw-core/src/model.rs` | `SkillCard`：`stage_ref: Option<StageKind>` → `stages: Vec<StageKind>` + `stage_origin: StageOrigin` | T3 |
| Modify `crates/bw-core/src/standards.rs` | `SKILL_STANDARDS_MD` 字段表补阶段归属行 | T8 |
| Modify `crates/bw-store/src/schema.sql` | 新增 `skill_stage` 表；skill 表加 `stage_origin`、去 `stage_ref` | T2 / T4 |
| Modify `crates/bw-store/src/sqlite.rs` | `drop_column_if_present` 原语、迁移顺序、`skill_row`、`list_skills`/`get_skill`/`create_skill`/`update_skill`、两个新 Store 方法 | T2 / T3 / T4 / T6 |
| Modify `crates/bw-store/src/lib.rs` | `NewSkill`/`SkillEdit` 字段、Store trait 方法增删 | T2 / T3 / T6 |
| Modify `crates/bw-store/src/seed.rs` | `CanonicalSkill.stage_ref` → `stages`，seed 逻辑改走关联表 | T3 |
| Modify `crates/ui/src/vm.rs` | 新增 `RoleTag`；`RoleFilter` 五档化；`role_chip_counts` 返回结构体；`SkillCardVm` 字段 | T3 |
| Modify `crates/app-desktop/src/screens/skill_hub.rs` | chip 行适配四态；编辑面板加五角色多选 | T3 / T6 |
| Modify `crates/app-desktop/src/screens/agent_hub.rs` | 适配 `RoleFilter`/`role_chip_counts` 新签名（agent 侧仍是单值） | T3 |
| Modify `crates/app-desktop/src/screens/workflow_hub.rs` | 同上 | T3 |
| Modify `crates/bw-engine/src/workspace.rs` | 新增 `write_file`（纯写盘、不 commit，区别于既有 `commit_file`） | T7 |
| Modify `crates/bw-app/src/lib.rs` | Boot 迁移搬值 / 静态表对账 / 蒸馏派生；`Command::UpdateSkill` 带 stages；`stage_catalog_block`；`prepare_issue_run` 接上注入 | T3 / T5 / T6 / T7 |

---

## Task 1: bw-core 静态归类表与 StageOrigin

**Files:**
- Create: `crates/bw-core/src/stage_catalog.rs`
- Modify: `crates/bw-core/src/lib.rs:28`（模块声明块末尾）
- Create: `crates/bw-app/examples/verify_stage_catalog.rs`

**Interfaces:**
- Consumes: `bw_core::model::StageKind`（已存在，五变体 + `ALL` + `index()`/`from_index()`）
- Produces:
  - `bw_core::stage_catalog::StageOrigin`（`Unclassified` / `Table` / `Distilled` / `Manual`，`Copy + PartialEq + Serialize + Deserialize + Default`，default = `Unclassified`）
  - `bw_core::stage_catalog::ALL_FIVE: &'static [StageKind]`
  - `bw_core::stage_catalog::SKILL_STAGE_CATALOG: &'static [(&'static str, &'static [StageKind])]`
  - `bw_core::stage_catalog::stages_for(name: &str) -> Option<&'static [StageKind]>`

**本任务不动 `SkillCard`**——纯新增，门禁保持全绿。

- [ ] **Step 1: 创建 `crates/bw-core/src/stage_catalog.rs`**

```rust
//! 技能的五角色归属:静态归类表 + 归类出处枚举。
//!
//! 用户 2026-08-05 拍板「通用的 skill 应该被划分到对应的五角色中」。归类值
//! 有三条来源(优先级递增):**本表**(随包发行/vendored 的 65 件技能,进 git
//! 可 diff 可 review)、蒸馏派生(有 `distilled_from_issue` 的技能按出处 Issue
//! 的 stage)、UI 人工覆盖(`StageOrigin::Manual` 之后 Boot 不再回填)。
//!
//! **本模块守 bw-core 的零 IO / wasm32 约束**:只有 `const` 数据和纯函数,
//! 不读盘、不查库。Boot 侧的对账逻辑在 bw-app,不在这里。
//!
//! 设计依据见 `docs/superpowers/specs/2026-08-05-skill-five-role-classification-design.md`。

use crate::model::StageKind::{self, Build, Growth, Ops, Optimize, Prototype};
use serde::{Deserialize, Serialize};

/// 一条技能的**归类动作从哪来**——与 `skill_stage` 关联表的行数共同派生四态:
///
/// | 状态 | 判据 |
/// |---|---|
/// | 挂 N 个阶段 | 关联表 1..=4 行 |
/// | 全阶段通用 | 关联表 5 行 |
/// | 已判定「不属任何阶段」 | 0 行 且 `origin != Unclassified` |
/// | 未归类(Unknown) | 0 行 且 `origin == Unclassified` |
///
/// 第三、四档的区分是这个枚举存在的全部理由:`obsidian-vault`(笔记工具)、
/// `scaffold-exercises`(课程脚手架)这类技能**不是没人管**,是判过了、跟项目
/// 五阶段无关——把它们和真没人管的混成一格,就是仓里「无数据=Unknown,绝不
/// 假装」那条纪律的反面。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageOrigin {
    /// 还没人归过类。DB 里是 `''`。
    #[default]
    Unclassified,
    /// 由 [`SKILL_STAGE_CATALOG`] 静态表回填。
    Table,
    /// 由蒸馏出处 Issue 的 stage 派生。
    Distilled,
    /// 人工在 SkillHub 里改过——Boot 的静态表回填从此整条跳过这件技能。
    Manual,
}

/// 「全阶段通用」的展开值。挂满五阶段 = 每个阶段的注入候选集都含它。
pub const ALL_FIVE: &[StageKind] = &[Prototype, Build, Optimize, Growth, Ops];

/// 空集 = **已判定**「不属任何阶段」(区别于「还没人判」——后者根本不在本表里)。
const NO_STAGE: &[StageKind] = &[];

/// 65 件随包发行 / vendored 技能的阶段归属正本。
///
/// 口径(指标类技能,spec §6.0):定=原型 / 用来打磨=优化 / 用来增长=运营 /
/// 用来守稳=运维(末项仅当该技能真涉及可观测性接入)。
///
/// 本地自建/蒸馏技能**不进本表**——它们是本机产物,归类走蒸馏派生或人工。
pub const SKILL_STAGE_CATALOG: &[(&str, &[StageKind])] = &[
    // ── bw-standard(8):五条方法论招牌技能单挂本阶段(它们是
    //    `playbook::stage_skills(kind)` 的正本,外扩会让「阶段=角色=方法论」
    //    的一一对应失效);指标/对标类按 §6.0 口径扩挂。
    ("evidence-first", &[Prototype]),
    ("competitive-analysis", &[Prototype, Growth]),
    ("north-star-discovery", &[Prototype, Optimize, Growth]),
    ("metrics-binding", &[Prototype, Optimize, Growth, Ops]),
    ("spec-to-tests", &[Build]),
    ("baseline-before-touch", &[Optimize]),
    ("fresh-eyes-funnel", &[Growth]),
    ("breaking-drill", &[Ops]),
    // ── mohit/pm-claude-skills(2):PR #74 升的基础技能,按 §6.0 同口径。
    ("metrics-framework", &[Prototype, Optimize, Growth]),
    ("metric-tree-builder", &[Prototype, Optimize, Growth]),
    // ── mattpocock-skills(41)
    ("ask-matt", ALL_FIVE),
    ("batch-grill-me", &[Prototype]),
    ("claude-handoff", ALL_FIVE),
    ("code-review", &[Build, Optimize]),
    ("codebase-design", &[Prototype, Optimize]),
    ("design-an-interface", &[Prototype]),
    ("diagnosing-bugs", &[Optimize, Ops]),
    ("domain-modeling", &[Prototype, Build]),
    ("edit-article", &[Growth]),
    ("git-guardrails-claude-code", &[Ops]),
    ("grill-me", &[Prototype]),
    ("grill-with-docs", &[Prototype]),
    ("grilling", &[Prototype]),
    ("handoff", ALL_FIVE),
    ("implement", &[Build]),
    ("improve-codebase-architecture", &[Optimize]),
    ("loop-me", &[Prototype]),
    ("migrate-to-shoehorn", &[Optimize]),
    ("obsidian-vault", NO_STAGE),
    ("prototype", &[Prototype]),
    ("qa", &[Optimize, Ops]),
    ("request-refactor-plan", &[Optimize]),
    ("research", &[Prototype, Growth]),
    ("resolving-merge-conflicts", &[Build]),
    ("scaffold-exercises", NO_STAGE),
    ("setup-matt-pocock-skills", NO_STAGE),
    ("setup-pre-commit", &[Build, Ops]),
    ("setup-ts-deep-modules", &[Optimize]),
    ("tdd", &[Build]),
    ("teach", NO_STAGE),
    ("to-questionnaire", &[Prototype]),
    ("to-spec", &[Prototype, Build]),
    ("to-tickets", &[Prototype, Build]),
    ("triage", &[Build, Ops]),
    ("ubiquitous-language", &[Prototype, Build]),
    ("wayfinder", &[Prototype, Build]),
    ("wizard", &[Ops]),
    ("writing-beats", &[Growth]),
    ("writing-fragments", &[Growth]),
    ("writing-great-skills", NO_STAGE),
    ("writing-shape", &[Growth]),
    // ── superpowers(14)
    ("brainstorming", &[Prototype]),
    ("dispatching-parallel-agents", ALL_FIVE),
    ("executing-plans", &[Build]),
    ("finishing-a-development-branch", &[Build]),
    ("receiving-code-review", &[Build, Optimize]),
    ("requesting-code-review", &[Build, Optimize]),
    ("subagent-driven-development", &[Build]),
    ("systematic-debugging", &[Build, Optimize, Ops]),
    ("test-driven-development", &[Build]),
    ("using-git-worktrees", &[Build]),
    ("using-superpowers", ALL_FIVE),
    ("verification-before-completion", ALL_FIVE),
    ("writing-plans", &[Prototype, Build]),
    ("writing-skills", NO_STAGE),
];

/// 查表。`None` = 这件技能不在静态表里(本地自建/外部新库)——诚实的「本表
/// 管不着」,**不是**「不属任何阶段」(后者在表里,值为空集)。
///
/// 线性扫描:65 条 × 每次 Boot 的技能数,量级微不足道,不值得为它引入 HashMap
/// (那会让本模块从 `const` 数据变成需要 lazy 初始化的东西)。
pub fn stages_for(name: &str) -> Option<&'static [StageKind]> {
    SKILL_STAGE_CATALOG
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, stages)| *stages)
}
```

- [ ] **Step 2: 注册模块**

在 `crates/bw-core/src/lib.rs` 的模块声明块（第 20-28 行，按字母序）中，`pub mod skill_spec;` 之后、`pub mod standards;` 之前插入一行：

```rust
pub mod stage_catalog;
```

- [ ] **Step 3: 内核门禁（含 wasm32 —— 本模块必须 wasm 可编译）**

```bash
cargo fmt --all --check && cargo clippy --workspace --exclude app-desktop -- -D warnings && cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
```

Expected: 三条全部通过，零 warning。

- [ ] **Step 4: 写自证 example `crates/bw-app/examples/verify_stage_catalog.rs`**

```rust
//! 静态归类表自证:条数 / 重名 / 五阶段计数 —— 数字从表本身算出,不硬编。
//! 与 `verify_goal.rs` 同族:仓里不留单元测试,可核验的事实靠 example 读回。
//!
//! 跑法:cargo run -p bw-app --example verify_stage_catalog

use bw_core::model::StageKind;
use bw_core::stage_catalog::{ALL_FIVE, SKILL_STAGE_CATALOG};
use std::collections::HashSet;

fn main() {
    let total = SKILL_STAGE_CATALOG.len();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut dups: Vec<&str> = Vec::new();
    for (name, _) in SKILL_STAGE_CATALOG {
        if !seen.insert(name) {
            dups.push(name);
        }
    }

    let universal = SKILL_STAGE_CATALOG
        .iter()
        .filter(|(_, s)| s.len() == ALL_FIVE.len())
        .count();
    let no_stage = SKILL_STAGE_CATALOG
        .iter()
        .filter(|(_, s)| s.is_empty())
        .count();

    println!("静态归类表 条数={total} 重名={dups:?} 全阶段通用={universal} 不属任何阶段={no_stage}");
    for kind in StageKind::ALL {
        let direct = SKILL_STAGE_CATALOG
            .iter()
            .filter(|(_, s)| s.contains(&kind) && s.len() < ALL_FIVE.len())
            .count();
        let candidates = direct + universal;
        println!(
            "  {:<10} 直接挂 {:>2} + 全阶段通用 {} = 候选集 {}",
            kind.label(),
            direct,
            universal,
            candidates
        );
    }
    assert!(dups.is_empty(), "静态归类表有重名:{dups:?}");
}
```

- [ ] **Step 5: 跑 example，核对数字与 spec §6.6 一致**

```bash
cargo run -p bw-app --example verify_stage_catalog
```

Expected（逐字对上，对不上就是表写错了，回 Step 1 修）：

```
静态归类表 条数=65 重名=[] 全阶段通用=6 不属任何阶段=6
  原型         直接挂 23 + 全阶段通用 6 = 候选集 29
  构建         直接挂 21 + 全阶段通用 6 = 候选集 27
  优化         直接挂 16 + 全阶段通用 6 = 候选集 22
  运营推广     直接挂 11 + 全阶段通用 6 = 候选集 17
  运维         直接挂  9 + 全阶段通用 6 = 候选集 15
```

> 若 `kind.label()` 的实际返回值与上面对齐宽度不符，只调整 `{:<10}` 宽度，**不要**改数字去迁就输出。

- [ ] **Step 6: Commit**

```bash
git add crates/bw-core/src/stage_catalog.rs crates/bw-core/src/lib.rs crates/bw-app/examples/verify_stage_catalog.rs
git commit -m "$(cat <<'EOF'
SR1 · 五角色归类静态表进内核(65 条,零 IO/wasm32 可编译)

用户 2026-08-05 拍板「通用的 skill 应该被划分到对应的五角色中」的归类正本。
StageOrigin 四值(''/table/distilled/manual)与 skill_stage 关联表行数共同派生
四态——「已判定不属任何阶段」与「还没人归类」必须分开,否则就是仓里「无数据
=Unknown 绝不假装」纪律的反面。

指标类按 spec §6.0 统一口径(定=原型/打磨=优化/增长=运营/守稳=运维,末项仅
当真涉可观测性接入):metrics-binding 四段全占(它就是点亮 Unknown 健康灯的
活);north-star-discovery/metrics-framework/metric-tree-builder 三段同口径。
五条方法论招牌技能不外扩(它们是 playbook::stage_skills 的正本)。

数字由 verify_stage_catalog example 从表本身算出,非手数:65 条无重名,候选集
原型 29/构建 27/优化 22/运营 17/运维 15。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: skill_stage 关联表 + stage_origin 列 + drop_column_if_present 原语

**Files:**
- Modify: `crates/bw-store/src/schema.sql:237-252`（`skill_file` 表之后、`agent` 表之前）
- Modify: `crates/bw-store/src/sqlite.rs:260-285`（`add_column_if_missing`/索引段）
- Modify: `crates/bw-store/src/sqlite.rs:306-322`（迁移原语区，`add_column_if_missing` 之后）
- Modify: `crates/bw-store/src/lib.rs:837-877`（Store trait 的 skill 区）

**Interfaces:**
- Consumes: `bw_core::stage_catalog::StageOrigin`（T1）
- Produces:
  - SQL：`skill_stage(skill_id TEXT, stage INTEGER, PRIMARY KEY(skill_id, stage))` + `idx_skill_stage_by_stage`；`skill.stage_origin TEXT NOT NULL DEFAULT ''`
  - `async fn drop_column_if_present(pool: &SqlitePool, table: &str, column: &str, dependent_indexes: &[&str]) -> Result<()>`
  - Store trait：`async fn list_skill_stages(&self) -> Result<HashMap<SkillId, Vec<StageKind>>>`
  - Store trait：`async fn set_skill_stages(&self, id: SkillId, stages: &[StageKind], origin: StageOrigin) -> Result<()>`

**本任务不删旧列、不改 `SkillCard`**——纯扩，门禁保持全绿，老库照常开。

- [ ] **Step 1: `schema.sql` 新增 skill_stage 表**

在 `crates/bw-store/src/schema.sql` 中，`CREATE INDEX IF NOT EXISTS idx_skill_file_skill ON skill_file(skill_id);`（第 244 行）之后、原 T7 注释块（第 245 行起）之前，插入：

```sql
-- 五角色归类(2026-08-05):一件技能可挂多个阶段,所以归属是关联表而不是
-- skill 行上的一个值。行数本身是语义的一半 —— 0 行 / 1..=4 行 / 5 行分别是
-- 「未判定或已判定不属任何阶段」/「挂这些阶段」/「全阶段通用」,另一半由
-- skill.stage_origin 提供(见 sqlite.rs 的迁移段)。
CREATE TABLE IF NOT EXISTS skill_stage (
    skill_id TEXT NOT NULL REFERENCES skill(id),
    stage    INTEGER NOT NULL,
    PRIMARY KEY (skill_id, stage)
);
-- 这个索引可以安全地待在 schema.sql 里(与下面 skill.stage_ref 的情况不同):
-- 它索引的是本文件自己刚 CREATE 的新表的列,不是往存量表上补的列。
CREATE INDEX IF NOT EXISTS idx_skill_stage_by_stage ON skill_stage(stage);
```

- [ ] **Step 2: `sqlite.rs` 加 stage_origin 列**

在 `crates/bw-store/src/sqlite.rs` 中，`add_column_if_missing(&pool, "agent", "stage_ref", "INTEGER").await?;`（约第 261 行）之后插入：

```rust
        // 五角色归类(2026-08-05):归类**动作**的出处。'' = 还没人归过类;
        // 'table' = bw-core 静态表回填;'distilled' = 按蒸馏出处 Issue 派生;
        // 'manual' = 人工在 SkillHub 改过(此后 Boot 的静态表回填整条跳过)。
        // 与 skill_stage 的行数共同派生四态 —— 单看行数分不出「判过了、不属
        // 任何阶段」和「还没人管」,而这两件事在本仓是必须分开的。
        add_column_if_missing(&pool, "skill", "stage_origin", "TEXT NOT NULL DEFAULT ''").await?;
```

- [ ] **Step 3: 新增 `drop_column_if_present` 迁移原语**

在 `crates/bw-store/src/sqlite.rs` 的 `add_column_if_missing` 函数（约第 306-322 行）之后插入：

```rust
/// `add_column_if_missing` 的对称件:删一列,先删掉依赖它的索引。
///
/// SQLite 的 `ALTER TABLE ... DROP COLUMN`(3.35+,本仓 libsqlite3-sys 0.30.1
/// 远高于门槛)在列被索引时会直接拒绝,所以 `dependent_indexes` 必须列全 ——
/// 调用方自己知道该列有哪些索引,这里不去猜。列不存在即 no-op,可在每次
/// `open()` 上安全重复调用。
///
/// 用户 2026-08-05 拍板「不能无限制扩展表格,不要害怕修改旧表,有需要就大胆
/// 重做」——把这条做成常备原语而不是一次性代码,是那句话的落地。
async fn drop_column_if_present(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    dependent_indexes: &[&str],
) -> Result<()> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let exists = rows.iter().any(|r| r.get::<String, _>("name") == column);
    if !exists {
        return Ok(());
    }
    for idx in dependent_indexes {
        sqlx::query(&format!("DROP INDEX IF EXISTS {idx}"))
            .execute(pool)
            .await?;
    }
    sqlx::query(&format!("ALTER TABLE {table} DROP COLUMN {column}"))
        .execute(pool)
        .await?;
    Ok(())
}
```

> 本任务只**定义**它，不调用（调用在 Task 4）。为避免 `dead_code` 让 `-D warnings` 失败，Task 4 之前先在函数上加 `#[allow(dead_code)]`，Task 4 接上调用时删掉该属性。

- [ ] **Step 4: Store trait 加两个方法**

在 `crates/bw-store/src/lib.rs` 中，删掉 `async fn set_skill_stage_ref(...)`（约第 855 行）**先不删**——它还有调用方（`seed.rs`），Task 3 才移除。本步只**新增**，在 `async fn list_skill_files(&self, skill_id: SkillId) -> Result<Vec<SkillFileRow>>;`（约第 847 行）之后插入：

```rust
    /// 五角色归类:一次读回全库的技能阶段归属。`list_skills` 用它给每张
    /// `SkillCard` 补齐 `stages`,避免每行一次查询的 N+1。缺席的 skill_id =
    /// 零行 = 「没挂任何阶段」(是未归类还是已判定不属任何阶段,由该行的
    /// `stage_origin` 分辨,不在本方法的职责里)。
    async fn list_skill_stages(&self)
        -> Result<std::collections::HashMap<SkillId, Vec<StageKind>>>;

    /// 五角色归类:重写一件技能的阶段归属(先删后插,幂等),并同时写下这次
    /// 归类的出处。空 `stages` + 非 `Unclassified` 的 `origin` = 「已判定:
    /// 不属任何阶段」;空 `stages` + `Unclassified` = 回到「未归类」。
    ///
    /// 这里**不碰** `source`/`official_library` —— 归类是 BW 自己的组织维度,
    /// 不是对上游正文的改编,不触发 T11「编辑即脱离源头」。
    async fn set_skill_stages(
        &self,
        id: SkillId,
        stages: &[StageKind],
        origin: StageOrigin,
    ) -> Result<()>;
```

同文件顶部 `use` 区补上 `StageOrigin`：

```rust
use bw_core::stage_catalog::StageOrigin;
```

- [ ] **Step 5: `sqlite.rs` 实现两个方法**

在 `crates/bw-store/src/sqlite.rs` 的 `impl Store for SqliteStore` 中，`async fn list_skill_files` 实现之后插入：

```rust
    async fn list_skill_stages(&self) -> Result<HashMap<SkillId, Vec<StageKind>>> {
        let rows = sqlx::query("SELECT skill_id, stage FROM skill_stage ORDER BY skill_id, stage")
            .fetch_all(&self.pool)
            .await?;
        let mut out: HashMap<SkillId, Vec<StageKind>> = HashMap::new();
        for r in rows {
            let id = parse_uuid(&r.get::<String, _>("skill_id"), SkillId::from_uuid)?;
            // 越界值(理论上进不来 —— 写侧只写 StageKind::index)如实丢弃,
            // 绝不映射成某个「差不多的」阶段。
            if let Some(k) = StageKind::from_index(r.get::<i64, _>("stage") as u8) {
                out.entry(id).or_default().push(k);
            }
        }
        Ok(out)
    }

    async fn set_skill_stages(
        &self,
        id: SkillId,
        stages: &[StageKind],
        origin: StageOrigin,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM skill_stage WHERE skill_id=?")
            .bind(id.uuid().to_string())
            .execute(&mut *tx)
            .await?;
        for k in stages {
            sqlx::query("INSERT INTO skill_stage (skill_id, stage) VALUES (?, ?)")
                .bind(id.uuid().to_string())
                .bind(i64::from(k.index()))
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE skill SET stage_origin=?, updated_at=? WHERE id=?")
            .bind(stage_origin_tag(origin))
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
```

> 注意 `UPDATE` **不动 `rev`** —— `rev` 是内容版本号，归类不是内容变更。

在同文件的自由函数区（`skill_row` 附近，约第 2798 行前）加一对标签转换：

```rust
/// `skill.stage_origin` 列 ↔ 域枚举。未知/空值一律读成 `Unclassified` ——
/// 诚实降级,绝不猜一个归类出处出来。
fn parse_stage_origin(tag: &str) -> StageOrigin {
    match tag {
        "table" => StageOrigin::Table,
        "distilled" => StageOrigin::Distilled,
        "manual" => StageOrigin::Manual,
        _ => StageOrigin::Unclassified,
    }
}

fn stage_origin_tag(origin: StageOrigin) -> &'static str {
    match origin {
        StageOrigin::Unclassified => "",
        StageOrigin::Table => "table",
        StageOrigin::Distilled => "distilled",
        StageOrigin::Manual => "manual",
    }
}
```

> `parse_stage_origin` 在 Task 3 才被 `skill_row` 用上；本任务先给它加 `#[allow(dead_code)]`，Task 3 接上时删掉。

同文件顶部 `use` 区补 `StageOrigin` 与 `HashMap`（若尚未引入）。

- [ ] **Step 6: 门禁**

```bash
cargo fmt --all --check && cargo clippy --workspace --exclude app-desktop -- -D warnings && cargo check -p app-desktop
```

Expected: 全绿，零 warning。

- [ ] **Step 7: 老库真跑读回（新表新列真的建出来了，且老库没崩）**

```bash
cp ~/Library/Application\ Support/BuildersWorkbench/workbench.db /tmp/bw-verify.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-verify.db 2>&1 | tail -5
sqlite3 /tmp/bw-verify.db ".tables" | tr ' ' '\n' | grep -x skill_stage
sqlite3 /tmp/bw-verify.db "PRAGMA table_info(skill);" | grep stage_origin
sqlite3 /tmp/bw-verify.db "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_skill_stage_by_stage';"
```

Expected：
- `verify_goal` 不 panic（老库开得起来）
- `skill_stage`
- `15|stage_origin|TEXT|1|''|0`（序号可能不同，有这一行即可）
- `idx_skill_stage_by_stage`

> 若 `verify_goal` 需要额外参数而跑不起来，改用任一现成 example 或直接 `BW_DB=/tmp/bw-verify.db ./target/debug/builders-workbench`（stderr 见 `[BW_OPEN]` 即可），目的只是「让 `open()` 的迁移路径在这个老库上真跑一遍」。

- [ ] **Step 8: Commit**

```bash
git add crates/bw-store/src/schema.sql crates/bw-store/src/sqlite.rs crates/bw-store/src/lib.rs
git commit -m "$(cat <<'EOF'
SR2 · skill_stage 关联表 + stage_origin 列 + drop_column_if_present 原语

纯扩,不删旧列不改 SkillCard —— 老库照常开,门禁全绿。旧列的真删在 SR4,
排在所有读侧迁完之后,否则删完当场崩。

drop_column_if_present 做成常备迁移原语(对称于 add_column_if_missing,先删
依赖索引再删列、列不存在即 no-op),是用户「不要害怕修改旧表,大胆重做」那句
话的落地——下次再遇到旧表债,不必重写一遍一次性代码。

set_skill_stages 刻意不动 rev(归类不是内容变更)、不动 source/official_library
(归类是 BW 自己的组织维度,不是对上游正文的改编,不触发 T11 编辑即脱离源头)。

读回为证:日常库副本上跑过迁移路径,skill_stage 表/stage_origin 列/
idx_skill_stage_by_stage 索引三项 sqlite3 读回齐全,老库未崩。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: SkillCard 换字段 + 全部读侧迁到关联表

**Files:**
- Modify: `crates/bw-core/src/model.rs:1487-1553`（`SkillCard`）
- Modify: `crates/bw-store/src/lib.rs:236-262`（`NewSkill`）、`:855`（删 `set_skill_stage_ref`）
- Modify: `crates/bw-store/src/sqlite.rs:2048-2069`（`list_skills`/`get_skill`）、`:2143`（`create_skill` 绑定）、`:2798-2835`（`skill_row`）
- Modify: `crates/bw-store/src/seed.rs:146-215`（`CanonicalSkill` + seed 逻辑）
- Modify: `crates/ui/src/vm.rs:657-702`（`RoleFilter`/`role_chip_counts`）、`:880-950`（`SkillCardVm`）
- Modify: `crates/app-desktop/src/screens/skill_hub.rs:42,46`
- Modify: `crates/app-desktop/src/screens/agent_hub.rs:37,41`
- Modify: `crates/app-desktop/src/screens/workflow_hub.rs`（chip 行同款两处）
- Modify: `crates/bw-app/src/lib.rs`（Boot：一次性搬值）

**Interfaces:**
- Consumes: T1 的 `StageOrigin`；T2 的 `list_skill_stages`/`set_skill_stages`/`parse_stage_origin`
- Produces:
  - `SkillCard.stages: Vec<StageKind>` + `SkillCard.stage_origin: StageOrigin`（`stage_ref` 字段消失）
  - `NewSkill.stages: Vec<StageKind>` + `NewSkill.stage_origin: StageOrigin`（`stage_ref` 字段消失）
  - `ui::vm::RoleTag`（`Stages(Vec<StageKind>)` / `Universal` / `NoStage` / `Unclassified`）+ `RoleTag::from_skill` / `RoleTag::from_single`
  - `ui::vm::RoleFilter`（`All` / `Stage(StageKind)` / `Universal` / `NoStage` / `Unclassified`）、`RoleFilter::matches(self, tag: &RoleTag) -> bool`
  - `ui::vm::RoleChipCounts { per_stage, universal, no_stage, unclassified }` + `role_chip_counts(tags: &[RoleTag]) -> RoleChipCounts`
  - `ui::vm::SkillCardVm.role_tag: RoleTag`（`stage_ref` 字段消失）

**这是本计划最大的一个任务**——Rust 的类型改动会同时打断 store/ui/desktop 三侧，无法再切小而仍保持每步编译通过。做完门禁必须全绿。

- [ ] **Step 1: `SkillCard` 换字段**

`crates/bw-core/src/model.rs`，把 `SkillCard` 的 `stage_ref` 字段（含其整段 T7 doc comment，约 1497-1512 行）整体替换为：

```rust
    /// 这件技能挂在哪几个阶段角色下(2026-08-05,用户拍板「通用的 skill 应该
    /// 被划分到对应的五角色中」)。多值:`code-review` 真的既属构建也属优化。
    /// 五个全挂 = 「全阶段通用」,对每个阶段的注入候选集都算命中。
    ///
    /// 空 `Vec` 有两种含义,靠 [`Self::stage_origin`] 分辨:origin 非
    /// `Unclassified` = **已判定**不属任何阶段(如 `obsidian-vault`);origin 为
    /// `Unclassified` = 还没人归过类。这两件事必须分开 —— 混成一格就是本仓
    /// 「无数据 = Unknown,绝不假装」纪律的反面。
    ///
    /// 存储在 `skill_stage` 关联表,不在 skill 行上(前身是 T7 的单值
    /// `stage_ref` 列,已随本次改动删除)。`WorkflowSpec.stage_ref` /
    /// `AgentCard.stage_ref` 本轮不动,仍是单值。
    #[serde(default)]
    pub stages: Vec<StageKind>,
    /// 上面那次归类**从哪来**——静态表 / 蒸馏派生 / 人工。见
    /// [`bw_core::stage_catalog::StageOrigin`]。
    #[serde(default)]
    pub stage_origin: StageOrigin,
```

同文件顶部 `use` 区补：

```rust
use crate::stage_catalog::StageOrigin;
```

- [ ] **Step 2: `NewSkill` 换字段**

`crates/bw-store/src/lib.rs`，把 `NewSkill` 的 `stage_ref` 字段（含 doc comment，约 241-246 行）替换为：

```rust
    /// 建行时的阶段归属(多值)。见 `bw_core::model::SkillCard::stages`。
    pub stages: Vec<StageKind>,
    /// 这次归类的出处。手工新建(`CreateSkill`)一律 `Unclassified` + 空
    /// `stages` —— 诚实的「还没归类」,绝不替用户猜一个阶段。
    pub stage_origin: StageOrigin,
```

同文件删掉 Store trait 里的 `async fn set_skill_stage_ref(...)` 声明（约 852-855 行，连同其 doc comment）。

- [ ] **Step 3: `skill_row` 与两个查询**

`crates/bw-store/src/sqlite.rs`：

`skill_row` 中，把

```rust
    let stage_ref = r
        .get::<Option<i64>, _>("stage_ref")
        .and_then(|n| StageKind::from_index(n as u8));
```

替换为

```rust
    // 阶段归属不在 skill 行上——它在 skill_stage 关联表里(多值)。行读只带
    // 归类出处;`stages` 由调用方(list_skills / get_skill)按 skill_id 补齐,
    // 避免每行一次查询的 N+1。
    let stage_origin = parse_stage_origin(&r.get::<String, _>("stage_origin"));
```

并把结构体字面量里的 `stage_ref,` 换成：

```rust
        stages: Vec::new(),
        stage_origin,
```

删掉 `parse_stage_origin` 上的 `#[allow(dead_code)]`（T2 Step 5 加的）。

`list_skills` 整体替换为：

```rust
    async fn list_skills(&self) -> Result<Vec<SkillCard>> {
        let rows = sqlx::query(
            "SELECT id, name, maturity, descr, category, stage_origin, source, official_library, uses, content,
                    distilled_from_issue, origin_agent, project_id
             FROM skill ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut cards: Vec<SkillCard> = rows
            .into_iter()
            .map(skill_row)
            .collect::<Result<Vec<_>>>()?;
        // 一次查关联表、按 id 分发 —— 不是每张卡一次查询。
        let by_id = self.list_skill_stages().await?;
        for c in cards.iter_mut() {
            if let Some(stages) = by_id.get(&c.id) {
                c.stages = stages.clone();
            }
        }
        Ok(cards)
    }
```

`get_skill` 整体替换为：

```rust
    async fn get_skill(&self, id: SkillId) -> Result<Option<SkillCard>> {
        let row = sqlx::query(
            "SELECT id, name, maturity, descr, category, stage_origin, source, official_library, uses, content,
                    distilled_from_issue, origin_agent, project_id
             FROM skill WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(mut card) = row.map(skill_row).transpose()? else {
            return Ok(None);
        };
        let stages = sqlx::query("SELECT stage FROM skill_stage WHERE skill_id=? ORDER BY stage")
            .bind(id.uuid().to_string())
            .fetch_all(&self.pool)
            .await?;
        card.stages = stages
            .into_iter()
            .filter_map(|r| StageKind::from_index(r.get::<i64, _>("stage") as u8))
            .collect();
        Ok(Some(card))
    }
```

`create_skill`（约 2143 行）：把 `INSERT` 语句里的 `stage_ref` 列名换成 `stage_origin`，绑定行

```rust
        .bind(skill.stage_ref.map(|k| i64::from(k.index())))
```

换成

```rust
        .bind(stage_origin_tag(skill.stage_origin))
```

并在同一函数的 INSERT 之后、返回之前，写入关联表：

```rust
        // 建行与阶段归属在同一次调用里落地 —— 让调用方没有「忘了写关联表」
        // 的机会(seed/import 路径都靠这一点)。
        for k in &skill.stages {
            sqlx::query("INSERT OR IGNORE INTO skill_stage (skill_id, stage) VALUES (?, ?)")
                .bind(skill.id.uuid().to_string())
                .bind(i64::from(k.index()))
                .execute(&self.pool)
                .await?;
        }
```

删掉 `impl Store for SqliteStore` 里 `set_skill_stage_ref` 的实现。

- [ ] **Step 4: `seed.rs` 改走关联表**

`crates/bw-store/src/seed.rs`：

`CanonicalSkill` 的 `stage_ref: StageKind` 字段（约 158-159 行，含 doc comment）替换为：

```rust
    /// 每件 bw-standard 技能的阶段归属(多值)。五条方法论技能单挂本阶段;
    /// 指标/对标类按 spec §6.0 口径扩挂 —— 真值由 bw-core 静态表提供,这里
    /// 只是搬运。
    pub stages: Vec<StageKind>,
```

`seed_bw_standard_skills_if_missing` 中的回填分支（约 181-190 行）替换为：

```rust
            // 回填只认**确实是我们这一行**的行(Official {bw-standard}),且只
            // 在它还没被归过类时动手(`StageOrigin::Unclassified`)。纯按名匹配
            // 会把用户自建的同名技能误当成「已种下的标配」,悄悄改掉他的归类
            // ——用户什么都没做,自己的技能被系统动了。人工归过类的行
            // (`Manual`)同理绝不覆盖。
            if existing.stage_origin == StageOrigin::Unclassified
                && matches!(
                    &existing.source,
                    HubSource::Official { official_library }
                        if official_library == BW_STANDARD_LIBRARY
                )
            {
                store
                    .set_skill_stages(existing.id, &c.stages, StageOrigin::Table)
                    .await?;
            }
```

`create_skill` 调用里 `stage_ref: Some(c.stage_ref),` 替换为：

```rust
                stages: c.stages.clone(),
                stage_origin: StageOrigin::Table,
```

同文件顶部 `use` 补 `StageOrigin`。

> **同时修好 `CanonicalSkill` 的构造方**：`crates/bw-app/src/lib.rs` 里构造 `CanonicalSkill` 的地方（由 `bw_library::BwSkillDocKind` 派生 `stage_ref`）改为派生 `stages` —— 用 `bw_core::stage_catalog::stages_for(&name)` 查静态表，查不到则退回原有的 `BwSkillDocKind` 单值语义包成 `vec![k]`。用 `rg -n "CanonicalSkill" crates/bw-app/src/lib.rs` 定位。

- [ ] **Step 5: `ui/src/vm.rs` 引入 RoleTag，RoleFilter 五档化**

把 `RoleFilter` 与 `role_chip_counts`（约 657-702 行）整段替换为：

```rust
/// 一行 Hub 记录在五角色维度上的**归属状态**,三个 Hub 屏共用。
///
/// Skill 侧是多值(2026-08-05 起,`skill_stage` 关联表);Agent / Workflow 侧
/// 本轮仍是单值 `Option<StageKind>`,经 [`RoleTag::from_single`] 收敛到同一
/// 个类型 —— 三屏共用一个筛选谓词的格局因此保住,不分叉。
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum RoleTag {
    /// 挂 1..=4 个阶段。
    Stages(Vec<StageKind>),
    /// 五个全挂 = 全阶段通用。
    Universal,
    /// **已判定**不属任何阶段(判过了,跟五阶段无关)。
    NoStage,
    /// 还没人归过类(Unknown)。
    Unclassified,
}

impl RoleTag {
    /// Skill 侧:多值 + 「判过没有」。`classified` 来自
    /// `SkillCard::stage_origin != StageOrigin::Unclassified`。
    pub fn from_skill(stages: &[StageKind], classified: bool) -> RoleTag {
        if stages.len() >= StageKind::ALL.len() {
            RoleTag::Universal
        } else if stages.is_empty() {
            if classified {
                RoleTag::NoStage
            } else {
                RoleTag::Unclassified
            }
        } else {
            RoleTag::Stages(stages.to_vec())
        }
    }

    /// Agent / Workflow 侧:单值。`None` 一律是「还没人归类」——那两侧今天
    /// 没有「已判定不属任何阶段」这个概念,不假装有。
    pub fn from_single(stage: Option<StageKind>) -> RoleTag {
        match stage {
            Some(k) => RoleTag::Stages(vec![k]),
            None => RoleTag::Unclassified,
        }
    }
}

/// 五角色筛选 chip 的选中态。`Universal`/`NoStage`/`Unclassified` 都是**可选
/// 中的真实状态**,不只是「无筛选」——用户可以专门问「只给我看还没归类的」。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoleFilter {
    All,
    Stage(StageKind),
    Universal,
    NoStage,
    Unclassified,
}

impl RoleFilter {
    pub fn matches(self, tag: &RoleTag) -> bool {
        match (self, tag) {
            (RoleFilter::All, _) => true,
            (RoleFilter::Stage(k), RoleTag::Stages(v)) => v.contains(&k),
            // 全阶段通用的技能对每个阶段 chip 都算命中 —— 它确实属于那个阶段,
            // 这与注入候选集的口径(本阶段的 + 全阶段通用的)是同一条规则。
            (RoleFilter::Stage(_), RoleTag::Universal) => true,
            (RoleFilter::Universal, RoleTag::Universal) => true,
            (RoleFilter::NoStage, RoleTag::NoStage) => true,
            (RoleFilter::Unclassified, RoleTag::Unclassified) => true,
            _ => false,
        }
    }
}

/// chip 行的真实计数。
///
/// `per_stage` 里已包含「全阶段通用」的行(与 `RoleFilter::matches` 同口径),
/// 因此各项之和会大于总行数 —— 这在多值维度下是正确的,一件挂两个阶段的技能
/// 本来就该在两个 chip 里各数一次。
pub struct RoleChipCounts {
    pub per_stage: Vec<(StageKind, usize)>,
    pub universal: usize,
    pub no_stage: usize,
    pub unclassified: usize,
}

pub fn role_chip_counts(tags: &[RoleTag]) -> RoleChipCounts {
    let mut per_stage: Vec<(StageKind, usize)> = StageKind::ALL.iter().map(|&k| (k, 0)).collect();
    let mut universal = 0usize;
    let mut no_stage = 0usize;
    let mut unclassified = 0usize;
    for tag in tags {
        match tag {
            RoleTag::Stages(v) => {
                for k in v {
                    if let Some(slot) = per_stage.iter_mut().find(|(s, _)| s == k) {
                        slot.1 += 1;
                    }
                }
            }
            RoleTag::Universal => {
                universal += 1;
                for slot in per_stage.iter_mut() {
                    slot.1 += 1;
                }
            }
            RoleTag::NoStage => no_stage += 1,
            RoleTag::Unclassified => unclassified += 1,
        }
    }
    RoleChipCounts {
        per_stage,
        universal,
        no_stage,
        unclassified,
    }
}
```

`SkillCardVm` 的 `stage_ref: Option<StageKind>` 字段（约 886-890 行，含 doc comment）替换为：

```rust
    /// 五角色归属(2026-08-05 起多值)。与 `AgentCardVm`/`WorkflowHubRowVm` 的
    /// 单值 `stage_ref` 经 `RoleTag` 收敛到同一筛选谓词。
    pub role_tag: RoleTag,
```

同文件约 950 行的构造 `stage_ref: s.stage_ref,` 替换为：

```rust
        role_tag: RoleTag::from_skill(
            &s.stages,
            s.stage_origin != bw_core::stage_catalog::StageOrigin::Unclassified,
        ),
```

- [ ] **Step 6: 三个 Hub 屏适配**

`crates/app-desktop/src/screens/skill_hub.rs:42,46` 替换为：

```rust
    let counts = role_chip_counts(
        &hub.skills
            .iter()
            .map(|s| s.role_tag.clone())
            .collect::<Vec<_>>(),
    );
    let filtered: Vec<SkillCardVm> = hub
        .skills
        .iter()
        .filter(|s| role_filter().matches(&s.role_tag))
        .cloned()
        .collect();
```

并把该文件后续用到 `stage_counts` / `general_count` 的地方改读 `counts.per_stage` / `counts.unclassified`，再在 chip 行的「通用」按钮之外补两枚 chip（`counts.universal` → `RoleFilter::Universal`、文案「全阶段通用」；`counts.no_stage` → `RoleFilter::NoStage`、文案「不属任何阶段」），原「通用」chip 文案改为「未归类」、绑 `RoleFilter::Unclassified`。样式复用同文件既有 chip 的 `theme::chip(bg, fg)` 写法。

`crates/app-desktop/src/screens/agent_hub.rs:37,41` 替换为：

```rust
    let counts = role_chip_counts(
        &hub.agents
            .iter()
            .map(|a| RoleTag::from_single(a.stage_ref))
            .collect::<Vec<_>>(),
    );
    let filtered: Vec<AgentCardVm> = hub
        .agents
        .iter()
        .filter(|a| role_filter().matches(&RoleTag::from_single(a.stage_ref)))
        .cloned()
        .collect();
```

同样把 `stage_counts`/`general_count` 的读取点改为 `counts.per_stage`/`counts.unclassified`。**agent 屏不加** Universal/NoStage 两枚 chip —— agent 侧本轮没有这两个状态，加了就是假的。

`crates/app-desktop/src/screens/workflow_hub.rs` 的 chip 行做同款改造（workflow 的 `stage_ref` 是 `Option<u8>`，先 `.and_then(StageKind::from_index)` 再 `RoleTag::from_single`）。

各文件 `use ui::vm::{...}` 里补 `RoleTag`。

- [ ] **Step 7: Boot 一次性搬值（老库的 8 条 bw-standard 归类不能丢）**

`crates/bw-app/src/lib.rs` 的 `Command::Boot` 分支中，在 `skill_canon` 对账循环**之前**插入：

```rust
                // 五角色归类迁移(2026-08-05):老库的 skill.stage_ref 单值搬进
                // skill_stage 关联表。只搬「关联表还没有这件技能的行」的,所以
                // 重复 Boot 是 no-op;列已被 SR4 删掉的库(即已迁过的库)读不到
                // 值,自然跳过。搬完置 stage_origin='table' —— 它们本来就是静
                // 态表管辖的那批行。
                let already: std::collections::HashSet<SkillId> = self
                    .store
                    .list_skill_stages()
                    .await?
                    .keys()
                    .copied()
                    .collect();
                for s in self.store.list_skills().await? {
                    if already.contains(&s.id) || s.stage_origin != StageOrigin::Unclassified {
                        continue;
                    }
                    if let Some(stages) = bw_core::stage_catalog::stages_for(&s.name) {
                        self.store
                            .set_skill_stages(s.id, stages, StageOrigin::Table)
                            .await?;
                    }
                }
```

同文件顶部 `use` 区补：

```rust
use bw_core::stage_catalog::StageOrigin;
```

> 这段同时就是 Task 5 要的「静态表对账」的雏形；Task 5 在它基础上补蒸馏派生与「人工覆盖不回填」的完整判据。**先让老库的既有归类不丢**是本任务的目标。

- [ ] **Step 8: 门禁全过**

```bash
cargo fmt --all --check && \
cargo clippy --workspace --exclude app-desktop -- -D warnings && \
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features && \
cargo check -p ui --target wasm32-unknown-unknown && \
./scripts/guard-kernel-ui-free.sh && \
cargo check -p app-desktop
```

Expected: 六条全绿。

- [ ] **Step 9: 老库读回——8 条 bw-standard 归类真的搬过去了**

```bash
cp ~/Library/Application\ Support/BuildersWorkbench/workbench.db /tmp/bw-verify.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-verify.db 2>&1 | tail -3
sqlite3 -header -column /tmp/bw-verify.db \
  "SELECT s.name, s.stage_origin, group_concat(x.stage) stages
   FROM skill s JOIN skill_stage x ON x.skill_id=s.id
   WHERE s.official_library='bw-standard' GROUP BY s.id ORDER BY s.name;"
```

Expected（8 行，`stage_origin` 全为 `table`；归类值来自静态表，已是扩挂后的多值）：

```
competitive-analysis   table  1,4
metrics-binding        table  1,3,4,5
north-star-discovery   table  1,3,4
evidence-first         table  1
spec-to-tests          table  2
baseline-before-touch  table  3
fresh-eyes-funnel      table  4
breaking-drill         table  5
```

- [ ] **Step 10: 深链渲染证明（三个 Hub 屏没崩）**

```bash
./scripts/point-bwdev-here.sh
BW_DB=/tmp/bw-verify.db BW_HUB=skill ~/Applications/BWDev.app/Contents/MacOS/bwdev-launcher 2>&1 | grep BW_OPEN
```

Expected: stderr 出现 `[BW_OPEN]` 行且进程不 panic。（`BW_HUB=agent` / `BW_HUB=workflow` 各跑一次。）

> **不要**指望点击导航——computer-use 对本应用 click 永久受阻（2026-07-30 结论，两种打包方式都验过）。深链 + 截图是唯一可靠路径。

- [ ] **Step 11: Commit**

```bash
git add -A crates/
git commit -m "$(cat <<'EOF'
SR3 · SkillCard 阶段归属改多值,三个 Hub 屏共用 RoleTag 收敛

SkillCard.stage_ref -> stages: Vec<StageKind> + stage_origin;读侧一次查
skill_stage 按 id 分发(不是每张卡一次查询)。Agent/Workflow 本轮仍单值,经
RoleTag::from_single 收敛到同一个筛选谓词 —— 三屏共用一套 RoleFilter/
role_chip_counts 的格局保住,没分叉。

RoleFilter 由三档扩到五档(All/Stage/Universal/NoStage/Unclassified)。
agent 屏刻意**不加** Universal/NoStage 两枚 chip:agent 侧今天没有这两个状态,
加了就是假的。role_chip_counts 的 per_stage 含全阶段通用行,各项之和大于总数
——多值维度下这是正确的,已在 doc comment 里写明,免得下次被当成 bug。

Boot 加一次性搬值:老库 stage_ref 的既有归类按静态表重建进关联表,幂等。
读回为证:日常库副本上 8 条 bw-standard 全部搬到位且已是扩挂后的多值
(metrics-binding 1,3,4,5),stage_origin=table;三个 Hub 屏深链 [BW_OPEN] 无 panic。

Rust 类型改动无法再切小而仍每步编译通过,故本件较大;旧列的真删在 SR4。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: 旧列真删（`skill.stage_ref`）

**Files:**
- Modify: `crates/bw-store/src/schema.sql:212-216`（skill 表的 `stage_ref` 列定义）、`:245-252`（T7 的「故意不建索引」注释块）
- Modify: `crates/bw-store/src/sqlite.rs:260-261`（`add_column_if_missing(skill, stage_ref)`）、`:282-284`（`idx_skill_stage` 索引创建）、迁移段（接上 `drop_column_if_present`）

**Interfaces:**
- Consumes: T2 的 `drop_column_if_present`；T3 已让全部读侧不再触碰该列
- Produces: 新库与老库的 `skill` 表均无 `stage_ref` 列；`idx_skill_stage` 索引不复存在

**前置**：T3 必须已合入且门禁全绿。此刻删列才安全——还有读侧引用时删列 = 运行时当场崩。

- [ ] **Step 1: `schema.sql` 去掉 skill 表的 stage_ref 列**

删除 `crates/bw-store/src/schema.sql` 中 skill 表的这段（约 212-216 行）：

```sql
    -- T7 (plan/12 §0/§2): which stage role this skill belongs to — same
    -- 1..=5 nullable-INTEGER convention `workflow_spec.stage_ref` already
    -- uses (interop via StageKind::index/from_index). NULL = 通用/跨阶段,
    -- honest for every imported catalog skill (nobody has classified them).
    stage_ref   INTEGER,
```

- [ ] **Step 2: `schema.sql` 换掉过期的 T7 注释块**

把第 245-252 行那段「T7: deliberately NO CREATE INDEX ... ON skill(stage_ref)」注释整体替换为：

```sql
-- 2026-08-05:skill 的阶段归属已迁到上面的 skill_stage 关联表,skill.stage_ref
-- 列连同 idx_skill_stage 索引一并删除(sqlite.rs 的 drop_column_if_present)。
-- T7 当年那条「本 blob 无条件重放在迁移守卫之前,所以补列的索引不能写在这里」
-- 的教训仍然成立,对 agent.stage_ref 依然有效 —— 别把它的索引搬进本文件。
```

- [ ] **Step 3: `sqlite.rs` 删掉 skill.stage_ref 的加列与建索引**

删除（约 260 行）：

```rust
        add_column_if_missing(&pool, "skill", "stage_ref", "INTEGER").await?;
```

删除（约 282-284 行）：

```rust
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_skill_stage ON skill(stage_ref)")
            .execute(&pool)
            .await?;
```

`agent` 的那两行**保留不动**（agent 侧本轮不在范围内）。

- [ ] **Step 4: 接上 `drop_column_if_present`**

在 `sqlite.rs` 迁移段中，**紧接在** `add_column_if_missing(&pool, "skill", "stage_origin", ...)` 之后插入：

```rust
        // 旧列真删(用户 2026-08-05:「不要害怕修改旧表,有需要就大胆重做」)。
        // 位置很关键:必须在 skill_stage 建表(schema blob)与 stage_origin 加列
        // 之后 —— Boot 的搬值逻辑要先能读到这一列,才轮得到删它。搬值本身在
        // bw-app 的 Boot 里,发生在 open() 返回之后,所以这里删列会让**本次**
        // 启动读不到旧值……因此搬值必须幂等且以静态表为准(见 SR3 Step 7 的
        // 实现:它按 name 查静态表重建,不依赖旧列的值)。
        drop_column_if_present(&pool, "skill", "stage_ref", &["idx_skill_stage"]).await?;
```

删掉 `drop_column_if_present` 上的 `#[allow(dead_code)]`（T2 Step 3 加的）。

> **这条注释里的取舍必须落实**：Boot 搬值以**静态表**为准（按 name 查），不读旧列的值。所以「open() 里删列 vs Boot 里搬值」的先后不构成数据丢失——8 条 bw-standard 的归类在静态表里，重建得出来。若某个老库里有**静态表覆盖不到**的手工 stage_ref 值，它会在此丢失；已知的真实库里不存在这种行（读回确认过：仅 8 条 bw-standard 有值，全部在静态表内）。

- [ ] **Step 5: 门禁全过**

```bash
cargo fmt --all --check && \
cargo clippy --workspace --exclude app-desktop -- -D warnings && \
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features && \
cargo check -p ui --target wasm32-unknown-unknown && \
./scripts/guard-kernel-ui-free.sh && \
cargo check -p app-desktop
```

- [ ] **Step 6: 老库读回——列真没了、归类还在**

```bash
cp ~/Library/Application\ Support/BuildersWorkbench/workbench.db /tmp/bw-verify.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-verify.db 2>&1 | tail -3
echo "stage_ref 残留数(必须为 0):"
sqlite3 /tmp/bw-verify.db "PRAGMA table_info(skill);" | grep -c stage_ref || true
echo "老索引残留(必须为空):"
sqlite3 /tmp/bw-verify.db "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_skill_stage';"
echo "归类行数(必须 >= 8):"
sqlite3 /tmp/bw-verify.db "SELECT COUNT(DISTINCT skill_id) FROM skill_stage;"
```

Expected: `0` / 空 / `≥ 8`，且 `verify_goal` 不 panic。

- [ ] **Step 7: 全新库也对（schema.sql 路径）**

```bash
rm -f /tmp/bw-fresh.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-fresh.db 2>&1 | tail -3
sqlite3 /tmp/bw-fresh.db "PRAGMA table_info(skill);" | grep -c stage_ref || true
sqlite3 /tmp/bw-fresh.db "SELECT COUNT(*) FROM skill_stage;"
```

Expected: `stage_ref` 计数 `0`；`skill_stage` 行数 > 0（全新库播种了 bw-standard 八件 + mohit 两件）。

- [ ] **Step 8: Commit**

```bash
git add crates/bw-store/src/schema.sql crates/bw-store/src/sqlite.rs
git commit -m "$(cat <<'EOF'
SR4 · skill.stage_ref 真删(列 + 索引),不留死列

用户 2026-08-05:「不能无限制扩展表格,不要害怕修改旧表,有需要就大胆重做」。
排在 SR3 之后是硬约束——还有读侧引用时删列会当场崩。

schema.sql 与 sqlite.rs 两处同改(CREATE TABLE IF NOT EXISTS 对存量表不加列,
所以新库靠前者、老库靠后者,少一处就有一半的库是错的)。T7 那条「补列的索引
不能写进 schema blob」的教训注释改写保留 —— 它对 agent.stage_ref 依然有效。

取舍留痕:Boot 搬值按 name 查静态表重建,不读旧列的值,所以 open() 删列先于
Boot 搬值不丢数据。若某老库有静态表覆盖不到的手工 stage_ref 值会在此丢失;
真实库读回确认不存在这种行(仅 8 条 bw-standard 有值,全在静态表内)。

读回为证:老库副本 PRAGMA 无 stage_ref、idx_skill_stage 已消失、skill_stage
归类行仍在;全新库同样无该列且播种归类正常。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Boot 完整对账（静态表 + 蒸馏派生 + 人工不覆盖）

**Files:**
- Modify: `crates/bw-app/src/lib.rs`（`Command::Boot` 分支，T3 Step 7 插入的那段）

**Interfaces:**
- Consumes: `bw_core::stage_catalog::stages_for`；`Store::list_skill_stages`/`set_skill_stages`/`list_skills`/`get_issue`
- Produces: `App::reconcile_skill_stages(&self) -> Result<(), AppError>`（Boot 调用一次）

- [ ] **Step 1: 把 T3 的临时搬值段抽成方法**

在 `crates/bw-app/src/lib.rs` 的 `impl App` 中（放在 `seed_stage_done_metrics` 附近，与其他 Boot 幂等 seed 函数为邻）新增：

```rust
    /// 五角色归类的 Boot 对账(2026-08-05)。三条来源按优先级递增:
    ///
    /// 1. **静态表**(`bw_core::stage_catalog`):65 件随包/vendored 技能的归类
    ///    正本。按名对账,幂等 —— 与库中现值不同就改齐(这是自愈:表改了、库
    ///    跟上)。
    /// 2. **蒸馏派生**:有 `distilled_from_issue` 的技能,按出处 Issue 的 stage
    ///    归类。这正是 `distilled_skills_block` 今天已在用的口径,不新造判据。
    /// 3. **人工覆盖**(`StageOrigin::Manual`):整条跳过,永不回填。
    ///
    /// 不在静态表、也没有蒸馏出处的技能如实留在「未归类」——绝不猜。
    async fn reconcile_skill_stages(&self) -> Result<(), AppError> {
        let by_id = self.store.list_skill_stages().await?;
        for s in self.store.list_skills().await? {
            // 人工归过类的行是终点,任何自动来源都不许覆盖。
            if s.stage_origin == StageOrigin::Manual {
                continue;
            }
            if let Some(want) = bw_core::stage_catalog::stages_for(&s.name) {
                let have = by_id.get(&s.id).map(Vec::as_slice).unwrap_or(&[]);
                // 已经一致就不写 —— 每次 Boot 空转一遍 UPDATE 会白白推高
                // updated_at,让「这行最近被动过」这个信号失真。
                let same = have.len() == want.len() && want.iter().all(|k| have.contains(k));
                if !(same && s.stage_origin == StageOrigin::Table) {
                    self.store
                        .set_skill_stages(s.id, want, StageOrigin::Table)
                        .await?;
                }
                continue;
            }
            // 蒸馏技能:出处 Issue 的 stage 就是它的阶段。出处 Issue 查不到
            // (理论上不该发生)如实跳过,不编一个阶段出来。
            if s.stage_origin == StageOrigin::Unclassified {
                if let Some(iid) = s.distilled_from_issue {
                    if let Some(issue) = self.store.get_issue(iid).await? {
                        self.store
                            .set_skill_stages(s.id, &[issue.stage], StageOrigin::Distilled)
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }
```

- [ ] **Step 2: Boot 调用它，替换掉 T3 的临时段**

在 `Command::Boot` 分支中，删掉 T3 Step 7 插入的那段临时搬值代码，改为在 `self.refresh_skills().await?;` **之前**调用：

```rust
                self.reconcile_skill_stages().await?;
```

位置要在 mohit `ImportSkillLibrary` 之后——那次导入会新建两行技能，对账要看得见它们。

- [ ] **Step 3: 门禁**

```bash
cargo fmt --all --check && cargo clippy --workspace --exclude app-desktop -- -D warnings && cargo check -p app-desktop
```

- [ ] **Step 4: 真实库读回——57 条通用降到 1 条**

```bash
cp ~/Library/Application\ Support/BuildersWorkbench/workbench.db /tmp/bw-verify.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-verify.db 2>&1 | tail -3
sqlite3 -header -column /tmp/bw-verify.db \
  "SELECT CASE WHEN n=0 AND stage_origin='' THEN '未归类'
               WHEN n=0 THEN '已判定不属任何阶段'
               WHEN n=5 THEN '全阶段通用'
               ELSE '挂'||n||'个阶段' END st,
          COUNT(*) cnt
   FROM (SELECT s.id, s.stage_origin,
                (SELECT COUNT(*) FROM skill_stage x WHERE x.skill_id=s.id) n
         FROM skill s)
   GROUP BY st ORDER BY st;"
echo "--- 每阶段候选(含全阶段通用) ---"
sqlite3 -header -column /tmp/bw-verify.db \
  "SELECT stage, COUNT(*) FROM skill_stage GROUP BY stage ORDER BY stage;"
```

Expected（真实日常库共 65 条 + Boot 导入 mohit 2 条 = 67 条）：
- `未归类` = **1**（`keyword-focus-scoring`——无蒸馏出处、不在静态表）
- `已判定不属任何阶段` = **6**
- `全阶段通用` = **6**
- 其余落在「挂 N 个阶段」各档
- 每阶段计数与 spec §6.6 一致：stage 1=29 / 2=27 / 3=22 / 4=17 / 5=15，**外加**蒸馏派生的 `per-source-volume-cap`（stage 3 因此为 23）

> 若数字对不上，先 `sqlite3 ... "SELECT name FROM skill WHERE NOT EXISTS(SELECT 1 FROM skill_stage x WHERE x.skill_id=skill.id) AND stage_origin=''"` 列出未归类的行，逐条比对静态表——**不要**改期望值去迁就输出。

- [ ] **Step 5: 幂等验证（跑第二遍，updated_at 不该全体抖动）**

```bash
sqlite3 /tmp/bw-verify.db "SELECT COUNT(*) FROM skill WHERE updated_at > strftime('%s','now') - 120;" 
cargo run -p bw-app --example verify_migration -- /tmp/bw-verify.db 2>&1 | tail -1
sqlite3 /tmp/bw-verify.db "SELECT COUNT(*) FROM skill WHERE updated_at > strftime('%s','now') - 60;"
```

Expected: 第二次 Boot 之后，60 秒内被更新的 skill 行数为 **0**（对账已一致就不写）。

- [ ] **Step 6: Commit**

```bash
git add crates/bw-app/src/lib.rs
git commit -m "$(cat <<'EOF'
SR5 · Boot 归类对账:静态表自愈 + 蒸馏派生 + 人工永不覆盖

三条来源优先级递增。静态表按名对账并自愈(表改了库跟上);蒸馏技能按出处
Issue 的 stage 派生——沿用 distilled_skills_block 今天已在用的口径,不新造
判据;StageOrigin::Manual 整条跳过。不在静态表又没蒸馏出处的如实留「未归类」,
绝不猜。

已一致就不写:每次 Boot 空转一遍 UPDATE 会白白推高 updated_at,让「这行最近
被动过」这个信号失真。幂等已实测(第二次 Boot 后 60 秒内被更新的 skill 行数
为 0)。

读回为证(真实日常库副本):未归类 57 -> 1(keyword-focus-scoring,无蒸馏出处
且不在静态表,如实留白);已判定不属任何阶段 6;全阶段通用 6;每阶段候选与
spec §6.6 一致。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: UI 归类入口（编辑面板多选 + UpdateSkill 带 stages）

**Files:**
- Modify: `crates/bw-app/src/lib.rs:549-555`（`Command::UpdateSkill` 定义）、`:6288-6330`（handler）
- Modify: `crates/bw-store/src/lib.rs`（`SkillEdit` 加字段）
- Modify: `crates/bw-store/src/sqlite.rs`（`update_skill` 实现）
- Modify: `crates/app-desktop/src/screens/skill_hub.rs:577-660`（`EditSkillForm`）

**Interfaces:**
- Consumes: T2 的 `set_skill_stages`；T3 的 `RoleTag`
- Produces: `Command::UpdateSkill` 新增 `stages: Option<Vec<StageKind>>` 字段（`None` = 本次不改归类）

- [ ] **Step 1: 命令加字段**

`crates/bw-app/src/lib.rs` 的 `Command::UpdateSkill` 定义改为：

```rust
    UpdateSkill {
        id: SkillId,
        name: String,
        desc: String,
        category: String,
        content: String,
        /// 五角色归类(2026-08-05)。`None` = 本次编辑不碰归类(保持既有行为,
        /// 让任何不带归类 UI 的调用方原样工作);`Some(v)` = 人工归类,落
        /// `StageOrigin::Manual`,此后 Boot 的静态表回填不再覆盖这件技能。
        /// `Some(vec![])` 是合法且有意义的输入:人工判定「不属任何阶段」。
        stages: Option<Vec<StageKind>>,
    },
```

- [ ] **Step 2: handler 落库**

在 `Command::UpdateSkill` 的 handler 中，把解构加上 `stages`，并在 `self.store.update_skill(...)` 调用**之后**、`refresh_skills` **之前**插入：

```rust
                // 归类与内容编辑分两次写:`SkillEdit` 管内容(且带 T11 的
                // flip_to_self_built),归类走 `set_skill_stages`(刻意不碰
                // source/official_library —— 归类是 BW 自己的组织维度,不是对
                // 上游正文的改编,不该让 mattpocock 的 tdd 因为被归到构建段就
                // 失去官方徽记)。
                if let Some(stages) = stages {
                    self.store
                        .set_skill_stages(id, &stages, StageOrigin::Manual)
                        .await?;
                }
```

> `flip_to_self_built` 的判定逻辑**不变**——它只看 `content`/`desc`/`category`，归类不参与，这正是我们要的。

- [ ] **Step 3: 编辑面板加五角色多选**

`crates/app-desktop/src/screens/skill_hub.rs` 的 `EditSkillForm` 中：

在 `let mut content = use_signal(|| s.content.clone());` 之后加：

```rust
    // 当前归属展开成「五个勾选位」——`Universal` 是五个全勾,`NoStage`/
    // `Unclassified` 是全不勾(两者的区别在保存时无意义:人工提交空集一律
    // 表示「判定为不属任何阶段」,这正是 Manual + 空集的语义)。
    let mut picked = use_signal(|| match &s.role_tag {
        RoleTag::Stages(v) => v.clone(),
        RoleTag::Universal => StageKind::ALL.to_vec(),
        RoleTag::NoStage | RoleTag::Unclassified => Vec::new(),
    });
```

在「分类」输入框之后、「正文」之前插入 chip 组：

```rust
            div { style: "{label}", "五角色归属(可多选;全不选 = 判定为不属任何阶段)" }
            div {
                style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:10px;",
                for kind in StageKind::ALL {
                    {
                        let on = picked().contains(&kind);
                        let (bg, fg): (&str, &str) = if on { (kind.color(), "#FFF") } else { ("#EFE9DA", theme::INK_2) };
                        rsx! {
                            button {
                                style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                                onclick: move |_| {
                                    let mut v = picked();
                                    if let Some(i) = v.iter().position(|k| *k == kind) { v.remove(i); } else { v.push(kind); }
                                    picked.set(v);
                                },
                                "{kind.role_short()}"
                            }
                        }
                    }
                }
            }
```

`save` 闭包的 `Command::UpdateSkill` 调用加一行：

```rust
            stages: Some(picked()),
```

文件顶部 `use` 补 `bw_core::model::StageKind` 与 `ui::vm::RoleTag`。

> `kind.color()` / `kind.role_short()` 是 `StageKind` 上已有的方法（chip 行已在用）。若签名不符，`rg -n "fn color|fn role_short" crates/bw-core/src/model.rs` 核对后按实际调整，**不要**新造方法。

- [ ] **Step 4: 修好其余 `Command::UpdateSkill` 调用点**

```bash
rg -n "Command::UpdateSkill" crates/ --glob '!target'
```

每个调用点补 `stages: None`（表示不碰归类）——**除了** Step 3 改的那个。

- [ ] **Step 5: 门禁全过**

```bash
cargo fmt --all --check && \
cargo clippy --workspace --exclude app-desktop -- -D warnings && \
cargo check -p ui --target wasm32-unknown-unknown && \
./scripts/guard-kernel-ui-free.sh && \
cargo check -p app-desktop
```

- [ ] **Step 6: 人工覆盖不被 Boot 冲掉（真实读回）**

先用 SQL 直接模拟一次人工归类（等价于 UI 提交），再跑 Boot，确认没被覆盖：

```bash
cp ~/Library/Application\ Support/BuildersWorkbench/workbench.db /tmp/bw-verify.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-verify.db 2>&1 | tail -1
# 把 tdd(静态表里是「构建」)人工改成「构建+优化」并标 manual
sqlite3 /tmp/bw-verify.db "
  UPDATE skill SET stage_origin='manual' WHERE name='tdd';
  DELETE FROM skill_stage WHERE skill_id=(SELECT id FROM skill WHERE name='tdd');
  INSERT INTO skill_stage(skill_id,stage) SELECT id,2 FROM skill WHERE name='tdd';
  INSERT INTO skill_stage(skill_id,stage) SELECT id,3 FROM skill WHERE name='tdd';"
# 再 Boot 一次
cargo run -p bw-app --example verify_migration -- /tmp/bw-verify.db 2>&1 | tail -1
sqlite3 /tmp/bw-verify.db "
  SELECT s.name, s.stage_origin, group_concat(x.stage)
  FROM skill s JOIN skill_stage x ON x.skill_id=s.id WHERE s.name='tdd' GROUP BY s.id;"
```

Expected: `tdd|manual|2,3` —— 静态表的 `[Build]` **没有**把人工的 `2,3` 冲掉。

- [ ] **Step 7: 深链看一眼编辑面板没崩**

```bash
./scripts/point-bwdev-here.sh
BW_DB=/tmp/bw-verify.db BW_HUB=skill ~/Applications/BWDev.app/Contents/MacOS/bwdev-launcher 2>&1 | grep BW_OPEN
```

Expected: `[BW_OPEN]` 且无 panic。

> 想看真实像素证据，把上面这条命令原样交给用户在自己屏幕上跑并截图——agent 自身的 `screencapture` 只拿得到壁纸（sandbox 限制，2026-07-30 结论）。

- [ ] **Step 8: Commit**

```bash
git add -A crates/
git commit -m "$(cat <<'EOF'
SR6 · SkillHub 归类入口:五角色多选,人工覆盖后 Boot 永不回填

Command::UpdateSkill 加 stages: Option<Vec<StageKind>>。None = 本次不碰归类
(不带归类 UI 的调用方原样工作);Some(vec![]) 是合法输入 —— 人工判定「不属
任何阶段」,正是 Manual + 空集的语义。

归类与内容编辑分两次写:SkillEdit 管内容(带 T11 flip_to_self_built),归类走
set_skill_stages 且刻意不碰 source/official_library —— 把 mattpocock 的 tdd
归到构建段,不该让它失去官方徽记。flip 判定逻辑一字未动,只看 content/desc/
category。

读回为证:把 tdd 人工改成「构建+优化」并标 manual 后再 Boot,静态表的
[Build] 没有把人工值冲掉(tdd|manual|2,3);技能库屏深链 [BW_OPEN] 无 panic。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: 注入——目录进 prompt + 正文物化到工作区

**Files:**
- Modify: `crates/bw-engine/src/workspace.rs`（新增 `write_file`）
- Create: `crates/bw-app/src/skill_materialize.rs`
- Modify: `crates/bw-app/src/lib.rs`（模块声明；`stage_catalog_block`；`prepare_issue_run` 约 3947-3970 行）

**Interfaces:**
- Consumes: `Store::list_skills`/`list_skill_files`；`bw_core::model::{SkillCard, StageKind}`
- Produces:
  - `bw_engine::workspace::write_file(dir: &Path, rel_path: &str, content: &str) -> Result<()>`
  - `bw_app::skill_materialize::{MaterializeReport, materialize_stage_skills}`
  - `App::stage_catalog_block(&self, stage: StageKind) -> Result<(String, Vec<SkillCard>), AppError>`

- [ ] **Step 1: bw-engine 加纯写盘 helper**

在 `crates/bw-engine/src/workspace.rs` 的 `commit_file`（约 98 行）之后插入：

```rust
/// 纯写盘,**不** git add/commit —— 与 `commit_file` 的区别就在这里。
///
/// 用于 BW 托管的派生文件(技能物化):它们每次 run 都可能重写,提交进 git 会
/// 把工作区的历史刷成噪音,而且这些文件的正本在库里,不在仓里。父目录不存在
/// 就建。
pub async fn write_file(dir: &Path, rel_path: &str, content: &str) -> Result<()> {
    let path = dir.join(rel_path);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&path, content).await?;
    Ok(())
}
```

- [ ] **Step 2: 创建 `crates/bw-app/src/skill_materialize.rs`**

```rust
//! 技能物化:把本阶段的候选技能写成工作区里真实的 `.claude/skills/<name>/`
//! 包,由 `claude` CLI 用它原生的 skill 加载机制按需读。
//!
//! **为什么不直接把正文塞进 prompt**:注入护栏 `skills_prompt_block` 的硬上限
//! 是 6000 字符,而 superpowers 技能正文平均 8732 字符(实测,2026-08-05)——
//! 单独一条就撑爆整个预算。按阶段挑正文塞 prompt 的做法,对绝大多数外部技能
//! 是空转,归类的「牙齿」是假的。
//!
//! **绝不覆盖用户手写**:同名目录若没有 `.bw-managed` 标记,整条跳过并留痕。
//! 用户自己在工作区写的 skill 永远优先于 BW 托管的派生副本。

use bw_core::model::SkillCard;
use bw_store::SkillFileRow;
use std::path::Path;

/// BW 托管标记文件名。内容是 `<skill id>\n<rev>`,既标身份也标版本 —— 版本
/// 一致就整条跳过,不做无谓写盘。
const MANAGED_MARKER: &str = ".bw-managed";

/// 一次物化的真实结果。字段都是**发生过的事**,不是计划 —— 调用方据此留痕。
#[derive(Debug, Default)]
pub(crate) struct MaterializeReport {
    /// 真写了盘的技能名。
    pub written: Vec<String>,
    /// 版本一致、整条跳过的技能数。
    pub unchanged: usize,
    /// 同名目录存在但**不是** BW 托管(没有标记文件)因而跳过的技能名 ——
    /// 这些是用户自己的 skill,优先级高于我们。
    pub skipped_foreign: Vec<String>,
}

/// 把 `skills`(每项 = 技能行 + 它的支撑文件)物化到 `workspace/.claude/skills/`。
///
/// `rev` 用于版本比对:调用方传入的 `SkillCard` 没有 `rev` 字段,所以这里用
/// 「id + 正文长度 + 支撑文件数」拼一个稳定指纹 —— 它对本用途足够:同一件技能
/// 内容没变就不重写,变了必然重写。
pub(crate) async fn materialize_stage_skills(
    workspace: &str,
    skills: &[(SkillCard, Vec<SkillFileRow>)],
) -> MaterializeReport {
    let mut report = MaterializeReport::default();
    let ws = workspace.trim();
    if ws.is_empty() {
        return report; // 未配置真实工作区 —— no-op,零报错
    }
    let root = Path::new(ws);
    for (skill, files) in skills {
        let dir_rel = format!(".claude/skills/{}", skill.name);
        let marker_rel = format!("{dir_rel}/{MANAGED_MARKER}");
        let fingerprint = format!("{}\n{}", skill.id.uuid(), skill.content.len() + files.len());
        let dir_abs = root.join(&dir_rel);
        let marker_abs = root.join(&marker_rel);

        if dir_abs.exists() {
            match tokio::fs::read_to_string(&marker_abs).await {
                Ok(existing) if existing.trim() == fingerprint.trim() => {
                    report.unchanged += 1;
                    continue;
                }
                Ok(_) => { /* BW 托管但版本不同 —— 往下重写 */ }
                Err(_) => {
                    // 目录在、标记不在 = 用户自己的 skill。绝不动。
                    report.skipped_foreign.push(skill.name.clone());
                    continue;
                }
            }
        }

        // 正文**原样**写出,不做 demote_headings —— 那是嵌套进 prompt 块才需要
        // 的变换;独立的 SKILL.md 必须保持 `#` 开头的原形,否则 CLI 认不出。
        let mut ok = bw_engine::workspace::write_file(
            root,
            &format!("{dir_rel}/SKILL.md"),
            &skill.content,
        )
        .await
        .is_ok();
        for f in files {
            if bw_engine::workspace::write_file(
                root,
                &format!("{dir_rel}/{}", f.rel_path),
                &f.content,
            )
            .await
            .is_err()
            {
                ok = false;
            }
        }
        if ok
            && bw_engine::workspace::write_file(root, &marker_rel, &fingerprint)
                .await
                .is_ok()
        {
            report.written.push(skill.name.clone());
        }
        // 写盘失败不炸 run —— 物化是增益,不是运行前提。失败的那条不进
        // `written`,报告因此如实。
    }
    report
}
```

在 `crates/bw-app/src/lib.rs` 顶部模块声明区加：

```rust
mod skill_materialize;
```

> `SkillFileRow` 的字段名（`rel_path` / `content`）以 `crates/bw-store/src/lib.rs:406-415` 的真实定义为准；若不同，按真实字段名调整，**不要**改结构体去迁就本文。

- [ ] **Step 3: `stage_catalog_block` —— 目录块 + 候选行**

在 `crates/bw-app/src/lib.rs` 的 `impl App` 中，紧挨 `distilled_skills_block` 之后插入：

```rust
    /// 本阶段(含全阶段通用)技能的**目录**块 + 需要物化的候选行。
    ///
    /// 只出目录不出正文:正文由 `skill_materialize` 落到工作区
    /// `.claude/skills/`,让 CLI 按需加载。desc 里本来就有触发段(「适用:…」/
    /// "Use when …"),那正是 agent 判断该不该加载一件技能的唯一依据。
    ///
    /// 候选 = 挂了本阶段的 ∪ 挂满五阶段的。「已判定不属任何阶段」与「未归类」
    /// 都**不进**候选 —— 前者判过了不属于,后者没人判过,都不该被当成本阶段的
    /// 推荐技能。
    async fn stage_catalog_block(
        &self,
        stage: StageKind,
    ) -> Result<(String, Vec<SkillCard>), AppError> {
        /// 目录块字符上限。按候选最多的原型段(29 条 × 约 110 字符 ≈ 3200)取,
        /// 留余量。超限按 uses 降序截断并如实写明未列出的条数 —— 静默截断会让
        /// prompt 读起来像「本阶段就这些技能」,那是假的。
        const MAX_BLOCK_CHARS: usize = 4000;
        const DESC_CAP: usize = 80;

        let mut candidates: Vec<SkillCard> = self
            .store
            .list_skills()
            .await?
            .into_iter()
            .filter(|s| !s.content.trim().is_empty() && s.stages.contains(&stage))
            .collect();
        if candidates.is_empty() {
            return Ok((String::new(), Vec::new()));
        }
        candidates.sort_by(|a, b| b.uses.cmp(&a.uses).then_with(|| a.name.cmp(&b.name)));

        let mut lines: Vec<String> = Vec::new();
        let mut total = 0usize;
        let mut listed = 0usize;
        for s in &candidates {
            let line = format!("- {} — {}", s.name, first_sentence_capped(&s.desc, DESC_CAP));
            let n = line.chars().count() + 1;
            if total + n > MAX_BLOCK_CHARS {
                break;
            }
            total += n;
            lines.push(line);
            listed += 1;
        }
        let omitted = candidates.len() - listed;
        let tail = if omitted > 0 {
            format!("\n（另有 {omitted} 件同样已物化，未在此列出——目录到此为止，不是技能到此为止）")
        } else {
            String::new()
        };
        let block = format!(
            "\n\n## 本阶段可用技能（已物化到 .claude/skills/，按需自行加载）\n{}{}\n",
            lines.join("\n"),
            tail
        );
        Ok((block, candidates))
    }
```

并在同文件的自由函数区加：

```rust
/// desc 的第一句,按字符数截断。技能的 description 上限 1024 字符,整句进目录
/// 会把 prompt 撑得没法读;第一句恰好是触发段所在。
fn first_sentence_capped(desc: &str, cap: usize) -> String {
    let head = desc
        .split(['。', '\n'])
        .next()
        .unwrap_or(desc)
        .split(". ")
        .next()
        .unwrap_or(desc)
        .trim();
    if head.chars().count() <= cap {
        return head.to_string();
    }
    let cut: String = head.chars().take(cap).collect();
    format!("{cut}…")
}
```

- [ ] **Step 4: 接进 `prepare_issue_run`**

在 `crates/bw-app/src/lib.rs` 的 `prepare_issue_run` 中，`distilled_skills_block` 调用之后、`spec.name = ...` 之前插入：

```rust
        // 五角色归类的落地(2026-08-05):本阶段技能的目录进 prompt、正文物化到
        // 工作区。与上面两块的关键区别 —— 目录里的技能**不进** `spec.skills`,
        // 因此不记 uses:目录列了二十几条、agent 实际可能只读两条,全都记一笔
        // 就是造假,会稀释 uses「真被用了」的语义。真解析(读 claude CLI 的
        // session jsonl 里的 tool_use=Skill 记录,只给真加载的记账)已验证可行,
        // 用户 2026-08-05 明确说本轮不做。
        let (catalog_block, catalog_skills) = self.stage_catalog_block(issue.stage).await?;
        if !catalog_skills.is_empty() && !proj.workspace_path.trim().is_empty() {
            let mut with_files = Vec::with_capacity(catalog_skills.len());
            for s in catalog_skills {
                let files = self.store.list_skill_files(s.id).await?;
                with_files.push((s, files));
            }
            let report =
                skill_materialize::materialize_stage_skills(&proj.workspace_path, &with_files)
                    .await;
            if !report.skipped_foreign.is_empty() {
                eprintln!(
                    "[BW_SKILL_MATERIALIZE] 跳过 {} 件同名但非 BW 托管的技能目录(用户自己的 skill 优先):{}",
                    report.skipped_foreign.len(),
                    report.skipped_foreign.join(", ")
                );
            }
            eprintln!(
                "[BW_SKILL_MATERIALIZE] 写入 {} · 未变 {}",
                report.written.len(),
                report.unchanged
            );
        }
```

并把 `let extra = format!(...)` 一行改为：

```rust
        let extra = format!("{issue_brief}{standard_block}{distilled_block}{catalog_block}");
```

> **`spec.skills.extend(...)` 两行保持原样**——只 extend `standard_refs` 与 `distilled_refs`。绝不加 catalog。

- [ ] **Step 5: 门禁全过**

```bash
cargo fmt --all --check && \
cargo clippy --workspace --exclude app-desktop -- -D warnings && \
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features && \
cargo check -p ui --target wasm32-unknown-unknown && \
./scripts/guard-kernel-ui-free.sh && \
cargo check -p app-desktop
```

- [ ] **Step 6: 真实物化验证（跑一件活，看工作区）**

用 headless 指挥器跑一圈（MockExecutor 即可——本步验的是物化与目录，不是执行器）：

```bash
rm -rf /tmp/bw-mat && mkdir -p /tmp/bw-mat
cargo run -p bw-app --example real_demo -- /tmp/bw-mat/demo.db /tmp/bw-mat/ws --mock 2>&1 | grep -E "BW_SKILL_MATERIALIZE|错误|panic" | head
WS=$(ls -d /tmp/bw-mat/ws/*/ 2>/dev/null | head -1)
echo "工作区:$WS"
echo "物化出的技能包数:"; ls "$WS/.claude/skills" 2>/dev/null | wc -l
echo "托管标记数(应与上面相等):"; ls "$WS/.claude/skills"/*/.bw-managed 2>/dev/null | wc -l
echo "抽一件看正文原形(必须以 --- 或 # 开头,没有被 demote):"; head -3 "$WS/.claude/skills"/*/SKILL.md 2>/dev/null | head -6
```

Expected: 两个数字相等且 > 0；`SKILL.md` 首行是 frontmatter 或 `#` 一级标题。

- [ ] **Step 7: 不覆盖用户手写（真实验证）**

```bash
mkdir -p "$WS/.claude/skills/tdd" && echo "# 我自己写的 tdd" > "$WS/.claude/skills/tdd/SKILL.md"
rm -f "$WS/.claude/skills/tdd/.bw-managed"
cargo run -p bw-app --example real_demo -- /tmp/bw-mat/demo.db /tmp/bw-mat/ws --mock 2>&1 | grep BW_SKILL_MATERIALIZE
cat "$WS/.claude/skills/tdd/SKILL.md"
```

Expected: stderr 出现「跳过 … 用户自己的 skill 优先」且含 `tdd`；文件内容**仍是**「# 我自己写的 tdd」，一个字没被改。

- [ ] **Step 8: uses 没有被目录注入污染**

```bash
sqlite3 /tmp/bw-mat/demo.db "SELECT name, uses FROM skill WHERE uses > 0 ORDER BY uses DESC LIMIT 10;"
```

Expected: 只有真正进了 `spec.skills` 的标配/蒸馏技能 `uses > 0`；目录里列出的 catalog 技能 `uses` 一律为 **0**。

- [ ] **Step 9: Commit**

```bash
git add -A crates/
git commit -m "$(cat <<'EOF'
SR7 · 归类接上真实注入:目录进 prompt,正文物化到工作区 .claude/skills/

归类到此才有牙齿。不塞正文的硬理由(实测,非推测):注入护栏 skills_prompt_block
上限 6000 字符,superpowers 技能正文平均 8732 —— 单条就撑爆,按阶段挑正文塞
prompt 对绝大多数外部技能是空转。

物化正文**原样**写出,不做 demote_headings —— 那是嵌套进 prompt 块才需要的
变换,独立 SKILL.md 必须保持原形否则 CLI 认不出。同名目录没有 .bw-managed
标记就整条跳过并留痕:用户自己写的 skill 永远优先于 BW 托管的派生副本。
bw-engine 新增 write_file(纯写盘不 commit,区别于 commit_file)——物化文件的
正本在库里不在仓里,提交进 git 只会把工作区历史刷成噪音。

目录里的技能**不进** spec.skills,因此不记 uses:列二十几条、实际可能只读两条,
全记一笔就是造假,会稀释 uses「真被用了」的语义。真解析(读 CLI session jsonl
的 tool_use=Skill)已验证可行,用户明确说本轮不做。

超限如实写明「另有 N 件未列出」,不静默截断。

读回为证:物化包数与托管标记数相等;手写同名 tdd 未被覆盖(stderr 有跳过留痕,
文件一字未改);catalog 技能 uses 全为 0。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: 补 skill 标准文档 + 全量验收

**Files:**
- Modify: `crates/bw-core/src/standards.rs`（`SKILL_STANDARDS_MD` 字段表）
- Modify: `docs/superpowers/specs/2026-08-05-skill-five-role-classification-design.md`（§11 偏差段收口）

**Interfaces:**
- Consumes: 全部前序任务
- Produces: 无新接口——本任务是文档与验收

- [ ] **Step 1: `SKILL_STANDARDS_MD` 字段表补阶段归属行**

在 `crates/bw-core/src/standards.rs` 的 `SKILL_STANDARDS_MD` 字段表中，`category` 那一行之后插入：

```
| `stages` | 作者 | 五角色归属,**可多值**(`code-review` 真的既属构建也属优化)。\
五个全挂 = 全阶段通用,对每个阶段的注入候选都算命中;空 = 见下一行。存在 \
`skill_stage` 关联表,不是 skill 行上的一个值。 |
| `stage_origin` | **系统** | 归类**动作**的出处:空=还没人归类 / `table`=随包\
静态表回填 / `distilled`=按蒸馏出处 Issue 派生 / `manual`=人工改过(此后自动\
回填不再覆盖)。它与 `stages` 是否为空共同决定四态——空 `stages` + 非空 \
`stage_origin` = **已判定「不属任何阶段」**(判过了,跟五阶段无关),空 + 空 = \
**还没人归类**。这两件事必须分开:混成一格就是「无数据 = Unknown,绝不假装」\
的反面。 |
```

同文件把 `## 创建前自查清单` 补一条：

```
5. 阶段归属填了吗?拿不准就留空——留空是诚实的「还没归类」,乱挂一个阶段会让\
它出现在错误阶段的注入目录里。
```

> `standards.rs` 自己的纪律是「每个字段列表都对着真实 schema 核过——发明一个不存在的字段，或漏掉一个存在的，比没有文档更糟」。本步写完后必须用 `sqlite3 /tmp/bw-verify.db "PRAGMA table_info(skill);"` 与 `.schema skill_stage` 逐字核对一遍。

- [ ] **Step 2: 门禁全过**

```bash
cargo fmt --all --check && \
cargo clippy --workspace --exclude app-desktop -- -D warnings && \
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features && \
cargo check -p ui --target wasm32-unknown-unknown && \
./scripts/guard-kernel-ui-free.sh && \
cargo check -p app-desktop
```

- [ ] **Step 3: 全量 E2E 验收（spec §10 全跑一遍）**

```bash
cp ~/Library/Application\ Support/BuildersWorkbench/workbench.db /tmp/bw-final.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-final.db 2>&1 | tail -3

echo "=== 1. 四态分布 ==="
sqlite3 -header -column /tmp/bw-final.db \
  "SELECT CASE WHEN n=0 AND stage_origin='' THEN '未归类'
               WHEN n=0 THEN '已判定不属任何阶段'
               WHEN n=5 THEN '全阶段通用'
               ELSE '挂'||n||'个阶段' END st, COUNT(*)
   FROM (SELECT s.id, s.stage_origin,
                (SELECT COUNT(*) FROM skill_stage x WHERE x.skill_id=s.id) n
         FROM skill s) GROUP BY st;"

echo "=== 2. 每阶段候选 ==="
sqlite3 -header -column /tmp/bw-final.db "SELECT stage, COUNT(*) FROM skill_stage GROUP BY stage ORDER BY stage;"

echo "=== 3. 旧列真删 ==="
sqlite3 /tmp/bw-final.db "PRAGMA table_info(skill);" | grep -c stage_ref || true
sqlite3 /tmp/bw-final.db "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_skill_stage';"

echo "=== 4. 全新库同样对 ==="
rm -f /tmp/bw-fresh2.db
cargo run -p bw-app --example verify_migration -- /tmp/bw-fresh2.db 2>&1 | tail -1
sqlite3 /tmp/bw-fresh2.db "PRAGMA table_info(skill);" | grep -c stage_ref || true
sqlite3 /tmp/bw-fresh2.db "SELECT COUNT(*) FROM skill_stage;"

echo "=== 5. 三屏深链 ==="
./scripts/point-bwdev-here.sh
for h in skill agent workflow; do
  BW_DB=/tmp/bw-final.db BW_HUB=$h ~/Applications/BWDev.app/Contents/MacOS/bwdev-launcher 2>&1 | grep BW_OPEN
done
```

Expected 汇总：未归类 `1` · 已判定不属任何阶段 `6` · 全阶段通用 `6` · 每阶段计数与 spec §6.6 一致（stage 3 因蒸馏派生多 1 = 23）· `stage_ref` 计数 `0` · 老索引查询为空 · 全新库同样无该列且 `skill_stage` 有行 · 三屏各出一行 `[BW_OPEN]` 无 panic。

- [ ] **Step 4: 过 `/code-review`**

本仓质量门是 `/code-review`，不是测试基线。对本分支的全部改动跑一遍，逐条处理反馈。

- [ ] **Step 5: spec §11 偏差段收口**

把 spec 的 §11 第 1 条（三态→四态）标注为「已实现，`stage_origin` 落地」，第 5 条（agent/workflow 不齐）保留为已知中间态。**不改**第 2、3 条——那两条是等用户拍板的开放问题，不能自行关掉。

- [ ] **Step 6: Commit**

```bash
git add -A crates/ docs/
git commit -m "$(cat <<'EOF'
SR8 · skill-standards 补齐阶段归属字段 + 全量 E2E 验收

skill 标准的字段表此前整个漏了阶段归属(workflow 标准写了)。按 standards.rs
自己的纪律,新增的两行逐字对着真实 schema 核过 —— 发明一个不存在的字段,或
漏掉一个存在的,比没有文档更糟。

四态在文档里写清:空 stages + 非空 stage_origin = 已判定「不属任何阶段」,
空 + 空 = 还没人归类。混成一格就是「无数据 = Unknown,绝不假装」的反面。

全量验收(真实日常库副本 + 全新库):未归类 1 / 已判定不属任何阶段 6 / 全阶段
通用 6;每阶段候选与 spec §6.6 一致;skill.stage_ref 与 idx_skill_stage 均已
消失;三个 Hub 屏深链 [BW_OPEN] 无 panic。

spec §11 偏差 1(三态→四态)标记已实现;偏差 2/3(6 条判为不属任何阶段、五条
方法论招牌技能不扩挂)保持开放 —— 那是等用户拍板的问题,不自行关掉。

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
EOF
)"
```

---

## 自查（对着 spec 逐节核）

| spec 节 | 覆盖任务 |
|---|---|
| §1 现状读回 | —（事实陈述，无需实现） |
| §2 D1 归类+接注入 | T7 |
| §2 D2 静态表 + UI 覆盖 | T1（表）· T5（Boot 对账）· T6（UI 覆盖） |
| §2 D3 多态 | T1（`StageOrigin`）· T3（`RoleTag` 四态） |
| §2 D4 多角色关联表 | T2（表）· T3（读侧） |
| §2 D5/D8 旧列真删 | T4 |
| §2 D6 目录+物化 | T7 |
| §2 D7 uses 不动 | T7 Step 4 注释 + Step 8 读回 |
| §2 D9 指标口径统一 | T1（静态表值） |
| §3.1 旧列真删 | T4 |
| §3.2 净增一表零列 | T2（加表加列）· T4（删旧列） |
| §3.3 迁移守卫 | T2 Step 3（`drop_column_if_present`）· T4 Step 4（调用） |
| §3.4 四态 | T1 · T3 |
| §4 三条来源 | T5 |
| §5.1 候选集 | T7 Step 3 |
| §5.2 prompt 块（4000 上限、no silent caps） | T7 Step 3 |
| §5.3 物化（原样、`.bw-managed`、不覆盖用户） | T7 Step 2 |
| §5.4 uses 不动 | T7 Step 4 |
| §6 归类草案 65 条 | T1 Step 1 |
| §7 UI | T3 Step 6（chip）· T6 Step 3（编辑面板） |
| §8 文档 | T8 Step 1 |
| §9 本轮不做 | 全程不触碰 agent/workflow 的 `stage_ref`、不做 transcript 解析 |
| §10 验收 | 各任务的读回步骤 + T8 Step 3 汇总 |

**已知未覆盖（有意）**：spec §9 列的四项本轮不做（agent 侧归类、uses transcript 真解析、五角色 agent skills 派生、workflow 多值化）。spec §11 的第 2、3 条是等用户拍板的开放问题，不在本计划范围内。

**计划外的记账**：本仓惯例是批次做完写 `iterations/HANDOFF-*.md`。是否要写、以及是否在 `plan/` 立新篇（本次工作跨了 plan/12 的 T7 与 plan/16 的技能规范），由用户决定——不自作主张开新 plan 文件。
