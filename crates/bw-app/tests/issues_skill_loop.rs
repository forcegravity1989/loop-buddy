//! **Issue→Skill loop test.** The R1+R2 end-to-end path, headless, through
//! `App::dispatch` exclusively (the App-level integration of what the store-
//! level tests in `bw-store/tests/issues.rs` + `skills.rs` check directly):
//!
//!   CreateProject → SetCycle → CompleteCreation → CreateAgent → CreateIssue
//!   → (read back: Backlog, Build) → AssignIssue → TransitionIssue(Done)
//!   → DistillSkillFromIssue → RefreshHubs
//!   → assert the new skill carries provenance (issue + agent), seeded skills
//!     do not, and exactly one skill has provenance.
//!
//!   Then the guard: distilling from a still-Backlog issue is rejected — the
//!   store's "issue is not Done" check surfaces through the command as an
//!   `AppError`.

use bw_app::{App, Command};
use bw_core::model::{Cadence, IssuePriority, IssueStatus, ProjectCycle, StageKind};
use bw_core::{AgentId, IssueId, ProjectId, SkillId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_issues_skill_loop_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn issue_to_skill_full_loop_through_app_commands() {
    let path = tmp_db();
    let project = ProjectId::new();
    let agent = AgentId::new();
    let issue = IssueId::new();
    let skill = SkillId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    // ── creation flow: project → cycle → complete (materializes 5 stages) ──
    app.dispatch(Command::CreateProject {
        id: project,
        name: "环演示项目".into(),
        kind: "自举".into(),
        desc: String::new(),
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCycle {
        cycle: ProjectCycle::Explore,
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();

    // ── agent teammate ─────────────────────────────────────────────────────
    app.dispatch(Command::CreateAgent {
        id: agent,
        name: "构建师 · sonnet5".into(),
        role: "构建".into(),
        skills: vec![],
        model: "sonnet".into(),
        instructions: "构建师:接 Issue 后写真实代码与测试,门禁全绿才交付。".into(),
    })
    .await
    .unwrap();

    // ── issue: create, read back, assign, transition to Done ───────────────
    app.dispatch(Command::CreateIssue {
        id: issue,
        stage: StageKind::Build,
        title: "App 级 Issue→Skill 集成测试".into(),
        desc: "真实 agent 队友完成的可验证工作".into(),
        priority: IssuePriority::High,
    })
    .await
    .unwrap();

    // Read back the created issue from the snapshot, assert initial state.
    let created = app
        .snapshot()
        .issues
        .iter()
        .find(|i| i.title == "App 级 Issue→Skill 集成测试")
        .expect("created issue is in the snapshot");
    assert_eq!(created.id, issue);
    assert_eq!(created.status, IssueStatus::Backlog);
    assert_eq!(created.stage, StageKind::Build);

    // Assign to the builder agent.
    app.dispatch(Command::AssignIssue {
        id: issue,
        assignee: Some(agent),
    })
    .await
    .unwrap();
    let assigned = app
        .snapshot()
        .issues
        .iter()
        .find(|i| i.id == issue)
        .unwrap();
    assert_eq!(assigned.assignee, Some(agent));

    // Transition to Done — the precondition for distillation.
    app.dispatch(Command::TransitionIssue {
        id: issue,
        status: IssueStatus::Done,
    })
    .await
    .unwrap();
    let done = app
        .snapshot()
        .issues
        .iter()
        .find(|i| i.id == issue)
        .unwrap();
    assert_eq!(done.status, IssueStatus::Done);

    // ── distill a skill from the completed issue ───────────────────────────
    app.dispatch(Command::DistillSkillFromIssue {
        skill_id: skill,
        issue_id: issue,
        name: "五角色环 · 真实交付法".into(),
        desc: "从完成的 Issue 蒸馏,归因到做活的 agent".into(),
        category: "方法论".into(),
        content: "\
## 五角色环 · 真实交付法(从真实完成的 Issue 蒸馏)\n\
1. 把阶段目标拆成可分配的 Issue(标题=可验收的动词短语,含 DoD)。\n\
2. 指派给对应角色的真实 agent 队友;Backlog 只停车,Todo 起才算承诺。\n\
3. agent 真做工程:写真实代码+测试,门禁(fmt/clippy/test)每圈过。\n\
4. 完成推 InReview→Done,产物/提交如实登记,不可核验的不勾。\n\
5. Done 后蒸馏方法为 Skill(带 provenance),下一件活直接复用。"
            .into(),
    })
    .await
    .unwrap();

    // RefreshHubs reloads the hub library (skills included) from the store.
    app.dispatch(Command::RefreshHubs).await.unwrap();

    // The distilled skill is now in the snapshot with full provenance.
    let distilled = app
        .snapshot()
        .skills
        .iter()
        .find(|s| s.id == skill)
        .expect("distilled skill is in the snapshot after RefreshHubs");
    assert_eq!(distilled.distilled_from_issue, Some(issue));
    assert_eq!(distilled.origin_agent, Some(agent));

    // Exactly one skill carries provenance — the one we just distilled. Every
    // pre-existing/seeded skill (Boot seeds the hub via seed_hub_if_empty) has
    // no provenance: they were never distilled from a real Issue.
    let with_provenance: Vec<_> = app
        .snapshot()
        .skills
        .iter()
        .filter(|s| s.distilled_from_issue.is_some())
        .collect();
    assert_eq!(
        with_provenance.len(),
        1,
        "exactly one skill has provenance (the distilled one)"
    );
    assert_eq!(with_provenance[0].id, skill);

    // Every other skill is provenance-free.
    assert!(app
        .snapshot()
        .skills
        .iter()
        .filter(|s| s.id != skill)
        .all(|s| s.distilled_from_issue.is_none()));

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn distill_command_rejects_non_done_issue() {
    let path = tmp_db();
    let project = ProjectId::new();
    let agent = AgentId::new();
    let issue = IssueId::new();
    let skill = SkillId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    // Same setup as the happy path, but we do NOT transition the issue to Done.
    app.dispatch(Command::CreateProject {
        id: project,
        name: "环演示项目 · 守卫测试".into(),
        kind: "自举".into(),
        desc: String::new(),
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCycle {
        cycle: ProjectCycle::Explore,
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();

    app.dispatch(Command::CreateAgent {
        id: agent,
        name: "构建师 · sonnet5".into(),
        role: "构建".into(),
        skills: vec![],
        model: "sonnet".into(),
        instructions: "构建师:接 Issue 后写真实代码与测试,门禁全绿才交付。".into(),
    })
    .await
    .unwrap();

    app.dispatch(Command::CreateIssue {
        id: issue,
        stage: StageKind::Build,
        title: "守卫测试 · 仍在 Backlog".into(),
        desc: "这个 Issue 不会被推进到 Done".into(),
        priority: IssuePriority::High,
    })
    .await
    .unwrap();

    app.dispatch(Command::AssignIssue {
        id: issue,
        assignee: Some(agent),
    })
    .await
    .unwrap();

    // The issue is still Backlog (assigned, but not Done). Distilling must fail
    // — the store's guard surfaces through the command as an AppError.
    let result = app
        .dispatch(Command::DistillSkillFromIssue {
            skill_id: skill,
            issue_id: issue,
            name: "不该成功的蒸馏".into(),
            desc: "Issue 未完成".into(),
            category: "方法论".into(),
            content: "(不会入库:蒸馏前置校验应失败)".into(),
        })
        .await;

    assert!(
        result.is_err(),
        "distilling from a non-Done issue must return Err through App::dispatch"
    );
    let msg = format!("{}", result.unwrap_err());
    assert!(
        msg.contains("Done"),
        "error message should mention Done, got: {msg}"
    );

    // No skill with that id was created.
    assert!(app.snapshot().skills.iter().all(|s| s.id != skill));

    let _ = std::fs::remove_file(&path);
}
