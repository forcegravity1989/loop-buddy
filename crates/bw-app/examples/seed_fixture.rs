//! **seed_fixture — 给已存在的 fixture 库补齐三态 Issue（真 Command 播种）。**
//!
//! `real_demo --mock` 驱动的是五阶段工作流环（`op_stage`/`session`/
//! `workflow_run` 表），从不创建 Issue 卡——`e2e/fixtures/demo.db` 因此长期
//! 是「2 项目 · 0 issue」。plan/15 §4 的常青验收流（②跑 Issue/③人点 Done/
//! ④蒸馏为技能）都需要 fixture 里已有 Issue 卡才能跑。
//!
//! 这个例程**打开一个已存在的库**（不新建项目、不碰工作流环），对准一个
//! 已存在的项目派生三张 Issue 卡，全部经真实 `Command` 走：
//!   - `Command::CreateIssue`（落 Backlog，项目内自动编号）
//!   - `Command::TransitionIssue`（沿 `can_transition_to` 的合法边逐格走，
//!     Done 只能从 InReview 进——不抄近道）
//!
//! 绝不写裸 SQL INSERT/UPDATE：数据是内核状态机 + 记账路径的真实产物，这正
//! 是「可信 fixture」的意义所在——settle-once 记账、`settled_at` 打点、
//! `IssuesChanged` 事件，全部走真代码路径。
//!
//! 幂等：按标题识别——已存在的三张 fixture Issue 只补齐到目标状态缺的那几
//! 步，不重复创建、不越过目标状态继续往前走。
//!
//! 用法：
//!   cargo run -p bw-app --example seed_fixture -- <db-path>

use bw_app::{App, Command};
use bw_core::model::{IssuePriority, IssueStatus, StageKind};
use bw_core::IssueId;
use bw_engine::{ClaudeCliConfig, PermissionMode};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

/// The project every fixture Issue is scoped to. Picked because its
/// `workspace_path` is empty (verified via `sqlite3` before writing this
/// example) — every run this fixture's Issues could trigger stays on
/// `MockExecutor`, never the flaky real gateway.
const TARGET_PROJECT: &str = "linkcheck-md";

/// One fixture Issue: title doubles as the idempotency key (so re-running
/// this example against an already-seeded db tops up rather than duplicates)
/// and is prefixed so nobody mistakes it for real backlog.
struct FixtureIssue {
    title: &'static str,
    desc: &'static str,
    stage: StageKind,
    priority: IssuePriority,
    target: IssueStatus,
}

const FIXTURE_ISSUES: &[FixtureIssue] = &[
    FixtureIssue {
        title: "【fixture】支持 --ignore-pattern 排除白名单路径的死链检查",
        desc: "e2e/fixtures/demo.db 的种子数据（seed_fixture 例程产出，非真实开发计划）。\
               留在 Todo，供「▶ 跑」验收流使用：一张可开工、尚未开工的 Issue 卡。",
        stage: StageKind::Build,
        priority: IssuePriority::Medium,
        target: IssueStatus::Todo,
    },
    FixtureIssue {
        title: "【fixture】修复相对路径链接在反斜杠路径分隔符下的误报",
        desc: "e2e/fixtures/demo.db 的种子数据（seed_fixture 例程产出，非真实开发计划）。\
               停在 InReview，供「✓ 确认完成（人裁）」验收流使用：验证 Done 的入边只有\
               InReview——本卡不会被本例程自动推进到 Done。",
        stage: StageKind::Build,
        priority: IssuePriority::High,
        target: IssueStatus::InReview,
    },
    FixtureIssue {
        title: "【fixture】为死链检查器加 --version 参数",
        desc: "e2e/fixtures/demo.db 的种子数据（seed_fixture 例程产出，非真实开发计划）。\
               走完 Backlog→Todo→InProgress→InReview→Done 全部真实转移，供「⚗ 蒸馏为技能」\
               验收流使用：settle-once 记账真实触发，settled_at 非空。",
        stage: StageKind::Build,
        priority: IssuePriority::Low,
        target: IssueStatus::Done,
    },
];

/// Legal forward path every Issue is created on (`Backlog` is the create
/// default) — the only sequence `can_transition_to` allows end to end.
const PATH: &[IssueStatus] = &[
    IssueStatus::Backlog,
    IssueStatus::Todo,
    IssueStatus::InProgress,
    IssueStatus::InReview,
    IssueStatus::Done,
];

/// Create the fixture issue if missing (idempotent by title), then walk it
/// forward from wherever it currently sits to `target` — one real
/// `TransitionIssue` dispatch per edge, never skipping a step.
async fn ensure_issue(app: &mut App, store: &Arc<dyn Store>, spec: &FixtureIssue) {
    let existing = store
        .list_issues(
            app.snapshot().active_project.expect("project open"),
            None,
            None,
        )
        .await
        .expect("list issues")
        .into_iter()
        .find(|i| i.title == spec.title);

    let (id, mut status) = match existing {
        Some(i) => {
            println!(
                "  [已存在] #{} 「{}」当前 {}",
                i.number,
                spec.title,
                i.status.label()
            );
            (i.id, i.status)
        }
        None => {
            let id = IssueId::new();
            app.dispatch(Command::CreateIssue {
                id,
                stage: spec.stage,
                title: spec.title.to_string(),
                desc: spec.desc.to_string(),
                priority: spec.priority,
                standard_skill: String::new(),
            })
            .await
            .expect("create issue");
            println!("  [已创建] 「{}」→ Backlog", spec.title);
            (id, IssueStatus::Backlog)
        }
    };

    let cur_idx = PATH
        .iter()
        .position(|s| *s == status)
        .expect("known status");
    let target_idx = PATH
        .iter()
        .position(|s| *s == spec.target)
        .expect("known target");
    assert!(
        target_idx >= cur_idx,
        "fixture spec asks to move #{:?} backward from {:?} to {:?} — not supported",
        id,
        status,
        spec.target
    );
    for next in &PATH[cur_idx + 1..=target_idx] {
        app.dispatch(Command::TransitionIssue { id, status: *next })
            .await
            .unwrap_or_else(|e| panic!("transition 「{}」→{:?} failed: {e}", spec.title, next));
        status = *next;
        println!("    → {}", status.label());
    }

    // plan/15 §4 flow-4 dry-run gap (2026-07-25): `distill_skill_from_issue`
    // hard-requires `issue.assignee` (crates/bw-store/src/sqlite.rs) — this
    // example never assigned one, which silently makes the Done fixture
    // issue impossible to distill through the real UI ("确认蒸馏" dispatches
    // a Command that errors "distill: issue has no assignee", never inserting
    // a skill row). No raw-SQL shortcut: assign for real via
    // `Command::AssignIssue`, matched to the built-in stage-role agent whose
    // `stage_ref` equals this fixture Issue's stage — same "real Command,
    // never a裸 SQL patch" discipline as everything else in this file.
    let current = store.get_issue(id).await.expect("get issue");
    if current.and_then(|i| i.assignee).is_none() {
        if let Some(agent) = store
            .list_agents()
            .await
            .expect("list agents")
            .into_iter()
            .find(|a| a.stage_ref == Some(spec.stage))
        {
            app.dispatch(Command::AssignIssue {
                id,
                assignee: Some(agent.id),
            })
            .await
            .expect("assign issue");
            println!("    [指派] → {}", agent.name);
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let db_path = args.get(1).cloned().expect("usage: seed_fixture <db-path>");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        ClaudeCliConfig {
            binary: None,
            max_budget_usd: 0.75,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    );
    app.dispatch(Command::Boot).await.expect("boot");

    println!("=== seed_fixture · db={db_path} ===\n");

    let project = store
        .list_projects()
        .await
        .expect("list projects")
        .into_iter()
        .find(|p| p.name == TARGET_PROJECT)
        .unwrap_or_else(|| {
            panic!(
                "target project 「{TARGET_PROJECT}」not found — run real_demo --mock against \
                 this db first"
            )
        });
    // Sanity: this fixture's whole point is staying off the real gateway.
    // If someone points seed_fixture at a db where the target project has a
    // real workspace wired up, fail loudly rather than silently seeding
    // Issues that could later fire real `claude -p` runs.
    assert!(
        project.workspace_path.trim().is_empty(),
        "target project 「{TARGET_PROJECT}」has a non-empty workspace_path ({:?}) — refusing to \
         seed Issues onto a project wired to a real workspace (would leave MockExecutor and hit \
         the real gateway on RunIssue)",
        project.workspace_path
    );
    app.dispatch(Command::OpenProject(project.id))
        .await
        .expect("open project");
    println!("[项目] 「{TARGET_PROJECT}」workspace_path 为空 → 确认留在 MockExecutor\n");

    for spec in FIXTURE_ISSUES {
        ensure_issue(&mut app, &store, spec).await;
    }

    // ── 真实读回：不从内存断言，从 store 重新查 ──
    println!("\n  ── 真实读回 ──");
    let issues = store
        .list_issues(project.id, None, None)
        .await
        .expect("list issues");
    let mut by_status: Vec<(IssueStatus, u32)> = Vec::new();
    for i in &issues {
        match by_status.iter_mut().find(|(s, _)| *s == i.status) {
            Some((_, c)) => *c += 1,
            None => by_status.push((i.status, 1)),
        }
    }
    for (status, count) in &by_status {
        println!("  {} × {}", status.label(), count);
    }
    for i in issues.iter().filter(|i| i.status == IssueStatus::Done) {
        println!(
            "  Done: #{} 「{}」settled_at={:?}",
            i.number, i.title, i.settled_at
        );
    }
    println!("\n=== seed_fixture 结束 ===");
}
