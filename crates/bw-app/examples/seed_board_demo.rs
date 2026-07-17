//! Seed a demo project + real agent teammates + real Issues (spread across
//! the kanban statuses) + one distilled Skill into a DB, so the desktop app's
//! Issue board has **real** data to render on launch. Nothing here is mock —
//! every row goes through the same `Command` path the live UI uses.
//!
//! Usage: `cargo run -p bw-app --example seed_board_demo -- <db-path>`
//! Then:  `BW_DB=<db-path> BW_OPEN='Builders' Workbench · 完整形态' cargo run -p app-desktop`

use bw_app::{App, Command};
use bw_core::model::{Cadence, IssuePriority, IssueStatus, ProjectCycle, StageKind};
use bw_core::{AgentId, IssueId, ProjectId, SkillId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

/// Create one Issue, optionally assign it, optionally park it past Backlog.
/// Returns the minted id (the distill step needs a `Done` one).
async fn mk(
    app: &mut App,
    stage: StageKind,
    title: &str,
    prio: IssuePriority,
    agent: Option<AgentId>,
    status: IssueStatus,
) -> IssueId {
    let id = IssueId::new();
    app.dispatch(Command::CreateIssue {
        id,
        stage,
        title: title.into(),
        desc: String::new(),
        priority: prio,
    })
    .await
    .unwrap();
    if let Some(a) = agent {
        app.dispatch(Command::AssignIssue {
            id,
            assignee: Some(a),
        })
        .await
        .unwrap();
    }
    // A5-F: walk the legal chain from Backlog rather than jumping straight to
    // the target (Backlog→Done etc. are no longer legal single hops).
    const FORWARD: [IssueStatus; 4] = [
        IssueStatus::Todo,
        IssueStatus::InProgress,
        IssueStatus::InReview,
        IssueStatus::Done,
    ];
    if let Some(target_idx) = FORWARD.iter().position(|s| *s == status) {
        for st in &FORWARD[..=target_idx] {
            app.dispatch(Command::TransitionIssue { id, status: *st })
                .await
                .unwrap();
        }
    } else if status != IssueStatus::Backlog {
        // Cancelled (or anything else non-forward) is legal directly from
        // Backlog.
        app.dispatch(Command::TransitionIssue { id, status })
            .await
            .unwrap();
    }
    id
}

#[tokio::main]
async fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/bw_workbench_demo.db".into());
    let _ = std::fs::remove_file(&path); // clean demo seed each run.
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    let name = "Builders' Workbench · 完整形态";
    let project = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: project,
        name: name.into(),
        kind: "开发者工作台 · multica × BW".into(),
        desc: "完整形态:五角色环 × 真实 agent 队友 × 度量诚实".into(),

        workspace: None,
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
    app.dispatch(Command::OpenProject(project)).await.unwrap();

    // Two real agent teammates (the five-role ring: Fable orchestrates,
    // sonnet5 builds).
    let fable = AgentId::new();
    app.dispatch(Command::CreateAgent {
        id: fable,
        name: "Fable · 调度(原型/运营/运维)".into(),
        role: "PM + 环协调".into(),
        skills: vec![],
        model: "fable".into(),
        instructions: "五角色环协调:拆 Issue、指派队友、验收 DoD、记账交棒;不 mock,不代答。".into(),
    })
    .await
    .unwrap();
    let sonnet = AgentId::new();
    app.dispatch(Command::CreateAgent {
        id: sonnet,
        name: "sonnet5 · 构建师".into(),
        role: "构建".into(),
        skills: vec![],
        model: "sonnet".into(),
        instructions: "构建师:接 Issue 后写真实代码与测试,门禁全绿才交付;产出如实登记。".into(),
    })
    .await
    .unwrap();

    // Real Issues across the kanban — each a unit of real work, scoped to a
    // stage, owned by the agent that did it.
    let _i_design = mk(
        &mut app,
        StageKind::Prototype,
        "设计综合:multica × BW 融合方案",
        IssuePriority::Urgent,
        Some(fable),
        IssueStatus::Done,
    )
    .await;
    let _i_r1 = mk(
        &mut app,
        StageKind::Build,
        "R1 · Issue 层(可分配工作单元 × 阶段 × agent)",
        IssuePriority::High,
        Some(sonnet),
        IssueStatus::InProgress,
    )
    .await;
    let _i_r2 = mk(
        &mut app,
        StageKind::Build,
        "R2 · Skill 复利(Done Issue → 带 provenance 的 Skill)",
        IssuePriority::High,
        Some(sonnet),
        IssueStatus::InReview,
    )
    .await;
    let i_test = mk(
        &mut app,
        StageKind::Optimize,
        "App 级 Issue→Skill 全链路集成测试",
        IssuePriority::Medium,
        Some(sonnet),
        IssueStatus::Done,
    )
    .await;
    let _i_board = mk(
        &mut app,
        StageKind::Build,
        "Issue 看板桌面 UI(本面板)",
        IssuePriority::Medium,
        Some(sonnet),
        IssueStatus::Todo,
    )
    .await;
    let _i_squad = mk(
        &mut app,
        StageKind::Build,
        "Squad leader 路由委派(本轮留口)",
        IssuePriority::Low,
        None,
        IssueStatus::Backlog,
    )
    .await;

    // R2: a Done + assigned Issue compounds into a reusable Skill (provenance).
    app.dispatch(Command::DistillSkillFromIssue {
        skill_id: SkillId::new(),
        issue_id: i_test,
        name: "五角色环 · 真实交付法".into(),
        desc: "从一件真实完成的 Issue 蒸馏:五角色各一轮、门禁每圈绿、不 mock。".into(),
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

    println!("✅ 已播种到 {path}");
    println!("   项目「{name}」· 2 真实 agent · 6 真实 Issue(跨 5 态)· 1 蒸馏 Skill");
    println!("   启动看板: BW_DB=\"{path}\" BW_OPEN=\"{name}\" cargo run -p app-desktop");
}
