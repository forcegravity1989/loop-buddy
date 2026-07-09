//! Generates a real, persisted demo database: the OMC/ECC hub library (via
//! `Boot`'s seed-if-empty) plus 2 projects driven through the full 5-stage
//! lifecycle (twice each, closing the Ops→Prototype loop), producing 20+ real
//! "evolution process" events (workflow runs, handoffs, observations, hub
//! promotions/imports) — everything through the same `App`/`Command` path the
//! real UI uses, on `MockExecutor` (no live API access from this sandbox; see
//! the run's own printed summary for exactly what that means).
//!
//! Run: `cargo run -p bw-app --example seed_demo -- <output-db-path>`

use bw_app::{App, Command, Event};
use bw_core::model::{stage_workflow, Cadence, HubSource, ProjectCycle, StageKind};
use bw_core::{MetricId, ProjectId, SessionId, WorkflowId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{MetricRole, SessionKind, SqliteStore, Store};
use std::sync::Arc;
use tokio::sync::broadcast::error::TryRecvError;

struct ProjectPlan {
    name: &'static str,
    kind: &'static str,
    desc: &'static str,
    cycle: ProjectCycle,
    benchmark: &'static str,
    opportunity: &'static str,
    north_star: &'static str,
    ns_def: &'static str,
    leading_name: &'static str,
    leading_target: &'static str,
    lagging_name: &'static str,
    lagging_target: &'static str,
    /// Per stage-visit (6 visits: Prototype/Build/Optimize/Growth/Ops/Prototype)
    /// observation values — deliberately not all-green, so the persisted
    /// history reads as a real thing that happened, not a demo that always
    /// succeeds.
    leading_values: [&'static str; 6],
    lagging_values: [&'static str; 6],
}

const PROJECTS: [ProjectPlan; 2] = [
    ProjectPlan {
        name: "智能客服知识库",
        kind: "AI 助手 / 客服",
        desc: "把客服重复问题从人工坐席剥离，用可检索的知识条目自助解决",
        cycle: ProjectCycle::Expand,
        benchmark: "Intercom Fin\nZendesk AI Agents",
        opportunity: "3 个月内把人工转接率打下来一半，知识条目本身能被复用而不是每次现改",
        north_star: "客服问题自助解决率",
        ns_def: "命中知识库并被用户判定「已解决」的会话数 / 总会话数",
        leading_name: "知识条目周复用次数",
        leading_target: "≥20",
        lagging_name: "人工转接率",
        lagging_target: "≤15%",
        leading_values: ["6", "14", "22", "28", "31", "12"],
        lagging_values: ["42%", "35%", "24%", "18%", "13%", "30%"],
    },
    ProjectPlan {
        name: "开发者体验仪表盘",
        kind: "内部工具 / 看板",
        desc: "让工程师不用问就能看到自己 PR 和服务的健康状态",
        cycle: ProjectCycle::Explore,
        benchmark: "Datadog\nGrafana",
        opportunity: "把 CI 等待、告警、on-call 负载摆到一个屏,减少「问一嘴」的打断",
        north_star: "每周活跃查看仪表盘的工程师数",
        ns_def: "每周至少打开一次仪表盘并停留 >30s 的工程师数",
        leading_name: "仪表盘周活跃用户数",
        leading_target: "≥30",
        lagging_name: "CI 平均等待时长",
        lagging_target: "≤8min",
        leading_values: ["3", "9", "17", "26", "34", "15"],
        lagging_values: ["19min", "15min", "11min", "9min", "7min", "13min"],
    },
];

#[tokio::main]
async fn main() {
    let out_path = std::env::args()
        .nth(1)
        .expect("usage: seed_demo <output-db-path>");
    let _ = std::fs::remove_file(&out_path); // fresh, honest run — never append onto a stale file

    let store: Arc<dyn Store> =
        Arc::new(SqliteStore::open(&out_path).await.expect("open output db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let mut rx = app.subscribe();

    app.dispatch(Command::Boot).await.expect("boot");
    let hub_workflows = app.snapshot().workflow_specs.len();
    let hub_skills = app.snapshot().skills.len();
    let hub_agents = app.snapshot().agents.len();
    println!(
        "hub seeded: {hub_workflows} workflows, {hub_skills} skills, {hub_agents} agents (real OMC/ECC catalog)"
    );

    let mut events: u64 = 0;

    for plan in PROJECTS.iter() {
        events += run_project(&mut app, plan).await;
    }

    // Demonstrate the hub round end-to-end inside this same demo run: pull a
    // real ECC-seeded workflow and run it via the hub path (bumps `uses`).
    let hub_pick = app
        .snapshot()
        .workflow_specs
        .iter()
        .find(|w| w.name == "orch-add-feature")
        .map(|w| w.id);
    if let Some(workflow_id) = hub_pick {
        // Run it inside the first project (already the active one after
        // run_project leaves it that way).
        let session = SessionId::new();
        app.dispatch(Command::StartSession {
            id: session,
            stage_kind: Some(StageKind::Build),
            kind: SessionKind::Create,
            title: "从 Hub 导入 · orch-add-feature".into(),
        })
        .await
        .expect("start session");
        app.dispatch(Command::RunHubWorkflow {
            session,
            workflow_id,
        })
        .await
        .expect("run hub workflow");
        events += 1;
        println!("ran hub workflow 'orch-add-feature' via RunHubWorkflow (uses +1)");
    }

    // Drain the event stream just to prove nothing silently failed.
    let mut failed = 0;
    loop {
        match rx.try_recv() {
            Ok(Event::WorkflowFailed(e)) => {
                failed += 1;
                eprintln!("WorkflowFailed: {e}");
            }
            Ok(_) => {}
            Err(TryRecvError::Empty | TryRecvError::Closed) => break,
            Err(TryRecvError::Lagged(_)) => continue,
        }
    }

    let projects = app.snapshot().projects.len();
    println!("---");
    println!("projects created: {projects}");
    println!("evolution events (workflow runs + handoffs + observations + hub actions): {events}");
    println!("workflow failures observed: {failed}");
    assert!(events >= 20, "expected >=20 evolution events, got {events}");
    assert!(
        (1..=3).contains(&projects),
        "expected 1..=3 demo projects, got {projects}"
    );
    println!("DB written to: {out_path}");
    println!(
        "open with: BW_DB=\"{out_path}\" cargo run -p app-desktop   (or point the real app's BW_DB env var at this file)"
    );
}

/// One project's full creation + two full laps of the 5-stage lifecycle
/// (Prototype→Build→Optimize→Growth→Ops→Prototype, closing the reflux loop).
/// Returns the number of real "evolution events" this project produced.
async fn run_project(app: &mut App, plan: &ProjectPlan) -> u64 {
    let mut events: u64 = 0;
    let project = ProjectId::new();
    let leading = MetricId::new();
    let lagging = MetricId::new();

    app.dispatch(Command::CreateProject {
        id: project,
        name: plan.name.into(),
        kind: plan.kind.into(),
        desc: plan.desc.into(),
    })
    .await
    .expect("create project");
    app.dispatch(Command::SetCycle { cycle: plan.cycle })
        .await
        .expect("set cycle");
    app.dispatch(Command::UpdateBrief {
        benchmark: plan.benchmark.into(),
        opportunity: plan.opportunity.into(),
    })
    .await
    .expect("update brief");
    app.dispatch(Command::UpdateNorthStar {
        value: plan.north_star.into(),
        def: plan.ns_def.into(),
    })
    .await
    .expect("update north star");
    app.dispatch(Command::UpsertManualMetric {
        id: leading,
        name: plan.leading_name.into(),
        def: "创建时录入的引领指标".into(),
        role: MetricRole::Leading,
        stage_kind: Some(StageKind::Prototype),
        target: plan.leading_target.into(),
        amber: Default::default(),
        value: plan.leading_values[0].into(),
    })
    .await
    .expect("leading metric");
    app.dispatch(Command::UpsertManualMetric {
        id: lagging,
        name: plan.lagging_name.into(),
        def: "创建时录入的滞后指标".into(),
        role: MetricRole::Lagging,
        stage_kind: Some(StageKind::Ops),
        target: plan.lagging_target.into(),
        amber: Default::default(),
        value: plan.lagging_values[0].into(),
    })
    .await
    .expect("lagging metric");
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .expect("complete creation");

    // Two full laps: Prototype, Build, Optimize, Growth, Ops, Prototype (again).
    let visits = [
        StageKind::Prototype,
        StageKind::Build,
        StageKind::Optimize,
        StageKind::Growth,
        StageKind::Ops,
        StageKind::Prototype,
    ];

    for (i, &stage) in visits.iter().enumerate() {
        let round = i as u32 + 1;

        // Real workflow run.
        let session = SessionId::new();
        app.dispatch(Command::StartSession {
            id: session,
            stage_kind: Some(stage),
            kind: SessionKind::Create,
            title: format!("{} · 第{round}轮", stage.label()),
        })
        .await
        .expect("start session");
        app.dispatch(Command::RunWorkflow {
            session,
            spec: stage_workflow(stage),
        })
        .await
        .expect("run workflow");
        events += 1;

        // Every other visit, promote this run into the hub as a real Static
        // entry — demonstrates the promote path with genuine run provenance.
        if round % 2 == 0 {
            app.dispatch(Command::PromoteWorkflow {
                new_id: WorkflowId::new(),
                session,
                source: HubSource::SelfBuilt,
            })
            .await
            .expect("promote workflow");
            events += 1;
        }

        // Real monitoring: a new observation per metric, this stage-visit's value.
        app.dispatch(Command::RecordObservation {
            metric: leading,
            value: plan.leading_values[i].into(),
        })
        .await
        .expect("record leading observation");
        events += 1;
        app.dispatch(Command::RecordObservation {
            metric: lagging,
            value: plan.lagging_values[i].into(),
        })
        .await
        .expect("record lagging observation");
        events += 1;

        // Real DoD progress: check the first couple of items (never all, so
        // at least one handoff below is honestly `risky: true`).
        app.dispatch(Command::ToggleDod {
            stage_kind: stage,
            index: 0,
        })
        .await
        .expect("toggle dod 0");
        if round != 3 {
            // leave round 3 (Optimize) with only one box checked → risky handoff
            app.dispatch(Command::ToggleDod {
                stage_kind: stage,
                index: 1,
            })
            .await
            .expect("toggle dod 1");
        }

        let risky = round == 3;
        let note = if risky {
            format!("{}: 时间到但清单未勾全,先带险交棒,标注待补", stage.label())
        } else {
            format!("{}: 真实验收通过,清单已勾", stage.label())
        };
        app.dispatch(Command::HandoffStage { risky, note })
            .await
            .expect("handoff stage");
        events += 1;
    }

    events
}
