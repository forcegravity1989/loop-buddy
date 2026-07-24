//! **Scenario simulator (iter 21).** Seeds a realistic hub with four
//! workflows and a synthesized "two weeks of usage" run history — hot/green,
//! warm/amber, failing, and cold — so the self-driving optimization loop
//! (iter 18) and every analysis function has *realistic* data to chew on,
//! not just the all-success MockExecutor path.
//!
//! Run history is written **directly via the Store**
//! (record_workflow_run_start + settle_workflow_run) with controlled
//! outcomes, because the point is to exercise analysis on a known scenario,
//! not to actually execute. This is honest: the rows are real `workflow_run`
//! records, just synthesized — same shape a real run produces.
//!
//! Usage: `cargo run --example simulate_hub -- /path/out.db`

use bw_app::{App, Command, View};
use bw_core::model::{HubSource, LoopConfig, Maturity, MaturityPeriod, RunStatus, RunTrigger};
use bw_core::{CronTaskId, ProjectId, WorkflowId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{NewWorkflowRun, SqliteStore, Store};
use std::sync::Arc;
use time::OffsetDateTime;

async fn quick_project(app: &mut App, name: &str) -> ProjectId {
    let id = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id,
        name: name.into(),
        kind: "仿真".into(),
        desc: String::new(),

        workspace: None,
        github: None,
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCycle {
        cycle: MaturityPeriod::Explore,
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: bw_core::model::Cadence::Weekly,
        run_first: false,
    })
    .await
    .unwrap();
    id
}

/// Seed one run record directly into the store with a controlled outcome —
/// the simulator's primitive. `t` is a synthetic monotonic clock (seconds).
#[allow(clippy::too_many_arguments)]
async fn seed_run(
    store: &Arc<dyn Store>,
    wid: WorkflowId,
    name: &str,
    project: ProjectId,
    t: i64,
    status: RunStatus,
    duration_ms: i64,
    phases: u8,
) {
    let id = store
        .record_workflow_run_start(NewWorkflowRun {
            workflow_id: wid,
            workflow_name: name,
            project_id: Some(project),
            session_id: None,
            trigger: RunTrigger::Manual,
            started_at: t,
            params_json: &format!(
                r#"{{"phase_count":{},"loop":{{"retries":1,"max_iter":3}}}}"#,
                phases
            ),
            cron_task_id: None,
        })
        .await
        .unwrap();
    store
        .settle_workflow_run(
            id,
            status,
            t + duration_ms / 1000 + 1,
            duration_ms,
            phases as u32,
            if status == RunStatus::Failed {
                "执行超时"
            } else {
                ""
            },
        )
        .await
        .unwrap();
}

async fn make_spec(app: &mut App, id: WorkflowId, name: &str, stage: Option<u8>, phases: u8) {
    app.dispatch(Command::CreateWorkflowSpec {
        id,
        name: name.into(),
        prompt: "仿真 prompt".into(),
        goal: "仿真 goal".into(),
        stage_ref: stage,
        phases: (0..phases).map(|i| format!("步骤{}", i + 1)).collect(),
        phase_prompts: vec![],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
        maturity: Maturity::Fresh,
        scope: String::new(),
        source: HubSource::SelfBuilt,
        trigger: None,
    })
    .await
    .unwrap();
}

#[tokio::main]
async fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/bw-simulate-hub.db".into());
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&out_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let project = quick_project(&mut app, "仿真项目 · 多场景 hub").await;
    let _ = View::Projects; // silence unused in some configs

    // Four workflows spanning the realistic spread.
    let insight = WorkflowId::new(); // 原型 stage=1
    let deliver = WorkflowId::new(); // 构建 stage=2
    let polish = WorkflowId::new(); // 优化 stage=3
    let patrol = WorkflowId::new(); // 运维 stage=5
    make_spec(&mut app, insight, "原型·洞察", Some(1), 3).await;
    make_spec(&mut app, deliver, "构建·交付", Some(2), 4).await;
    make_spec(&mut app, polish, "优化·打磨", Some(3), 5).await;
    make_spec(&mut app, patrol, "运维·巡检", Some(5), 2).await;

    // Two weeks of synthetic history (monotonic t, 1 run = 1 hour).
    let base = OffsetDateTime::now_utc().unix_timestamp() - 14 * 24 * 3600;
    let h = 3600i64;

    // 原型·洞察: hot + green (8 ok).
    for i in 0..8i64 {
        seed_run(
            &store,
            insight,
            "原型·洞察",
            project,
            base + i * h,
            RunStatus::Ok,
            200 + i * 5,
            3,
        )
        .await;
    }
    // 构建·交付: warm + amber (3 ok, 1 fail).
    for (i, st) in [
        RunStatus::Ok,
        RunStatus::Ok,
        RunStatus::Ok,
        RunStatus::Failed,
    ]
    .iter()
    .enumerate()
    {
        seed_run(
            &store,
            deliver,
            "构建·交付",
            project,
            base + (10 + i as i64) * h,
            *st,
            350,
            4,
        )
        .await;
    }
    // 优化·打磨: failing (2 ok, 5 fail — same root cause 执行超时).
    for (i, st) in [
        RunStatus::Failed,
        RunStatus::Failed,
        RunStatus::Ok,
        RunStatus::Failed,
        RunStatus::Failed,
        RunStatus::Ok,
        RunStatus::Failed,
    ]
    .iter()
    .enumerate()
    {
        seed_run(
            &store,
            polish,
            "优化·打磨",
            project,
            base + (20 + i as i64) * h,
            *st,
            800,
            5,
        )
        .await;
    }
    // 运维·巡检: cold (0 runs) + a scheduled task that will never have fired.
    let _ = patrol;
    let task = CronTaskId::new();
    app.dispatch(Command::CreateCronTask {
        id: task,
        name: "每日巡检".into(),
        target: "运维·巡检".into(),
        schedule: bw_core::model::Cadence::Daily,
        project_id: Some(project),
    })
    .await
    .unwrap();

    println!("=== 仿真 hub 已播种(4 工作流 · 两周运行史)===");
    println!("  原型·洞察  : 8 次,全成功(热门·绿)");
    println!("  构建·交付  : 4 次,3 成 1 败(温·黄)");
    println!("  优化·打磨  : 7 次,2 成 5 败(失败·红,根因=执行超时)");
    println!("  运维·巡检  : 0 次(冷),已绑每日定时任务");
    println!();

    // Snapshot the seeded analysis so iter 22 has a known starting point.
    for (id, name) in [
        (insight, "原型·洞察"),
        (deliver, "构建·交付"),
        (polish, "优化·打磨"),
        (patrol, "运维·巡检"),
    ] {
        let a = store.workflow_analytics(id).await.unwrap();
        let health = bw_core::analysis::workflow_health(&a);
        println!(
            "  [{:?}] {} · {} 次 · 成功率 {:?} · 中位 {}ms",
            health,
            name,
            a.total_runs,
            a.success_rate,
            a.median_duration_ms.unwrap_or(0)
        );
    }
    println!("\nDB written to: {out_path}");
    println!("下一步(iter 22):对此 hub 跑 run_optimization_cycle,看自驱循环的诊断。");
}
