//! M3 "用户一天" acceptance test (plan/06 §8 M3 / HANDOFF-2026-07-16-A5 §4).
//! One long walk through a real day, headless, through `App::dispatch`
//! exclusively — every number is a store read-back, never taken on faith.
//!
//! Deliberately never configures a project workspace: `run_workflow_inner`
//! upgrades ANY project with a non-empty `workspace_path` to a real
//! `ClaudeCliExecutor` for every run, regardless of what `Engine` `App::new`
//! was given (see its doc comment) — that's what "all-in-one-codebase"
//! actually buys you, but it also means this run would depend on the real
//! agent gateway, which this suite never does (`complete_form.rs`'s own
//! split between `mk_app(Some(root))`-no-runs and `mk_app(None)`-runs tests
//! is the same rule applied there). Real-workspace + real-artifact-scan is
//! already proven end to end by `complete_form.rs`;this test stays on
//! `MockExecutor` throughout and focuses on the issue lifecycle instead.
//!
//! 项目起步 → 看墙(0 开放)→ Autopilot 晨间建单 → RunIssue(Mock)→ 人确认 Done
//! (记账 + 指标两线落地)→ 复利闭环(蒸馏 → 下一件活的 run 注入 → uses+1)
//! → 交棒(留 1 件未完 → 强制 risky)→ 守卫抽查(非法直跳 / 空原因阻塞双双被拒)。

use bw_app::{App, Command};
use bw_core::model::{Cadence, IssuePriority, IssueStatus, ProjectCycle, Signal, StageKind};
use bw_core::{AgentId, CronTaskId, IssueId, ProjectId, SessionId, SkillId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SessionKind, SqliteStore, Store};
use std::sync::Arc;

const AGENT_NAME: &str = "构建师 · 一天";
// A fresh project's active_stage defaults to Prototype — every issue in this
// test is scoped there so the HandoffStage open-issue guard (which counts
// against `proj.active_stage`) actually sees them.
const STAGE: StageKind = StageKind::Prototype;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_user_day_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn a_users_day_end_to_end_every_number_reads_back_from_the_store() {
    let db = tmp_db();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    // ── 1. 创建流:意图 → 阶段配比 → CompleteCreation(确认起步)。 ──
    let project = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: project,
        name: "用户一天 · 验收项目".into(),
        kind: "CLI 工具".into(),
        desc: "验收 A5 全链路".into(),
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

    let proj = store.get_project(project).await.unwrap().unwrap();
    assert_eq!(proj.active_stage, STAGE, "新项目的活跃段是原型");

    // A4: 每阶段的「阶段完成 Issue 数」指标已播种,目标空 ⇒ 信号不可能是 Green。
    let seeded = store.persisted_signals(project).await.unwrap().metrics;
    let done_metrics: Vec<_> = seeded
        .iter()
        .filter(|m| m.name == "阶段完成 Issue 数")
        .collect();
    assert_eq!(
        done_metrics.len(),
        StageKind::ALL.len(),
        "每阶段各播种一条机器指标"
    );
    for m in &done_metrics {
        assert!(m.target_raw.trim().is_empty(), "目标留空 = 诚实待设");
        assert_ne!(m.signal, Some(Signal::Green), "无目标的计数不可能自称绿");
    }

    // ── 2. 看墙:刚起步,没有任何开放 Issue。 ──
    assert_eq!(
        store.count_open_issues(project).await.unwrap(),
        0,
        "刚起步的项目没有开放 Issue"
    );

    // Real agent teammate for the day's work.
    let agent = AgentId::new();
    app.dispatch(Command::CreateAgent {
        id: agent,
        name: AGENT_NAME.into(),
        role: "构建".into(),
        skills: vec![],
        model: "sonnet".into(),
        instructions: "构建师:接 Issue 后写真实代码与测试,门禁全绿才交付。".into(),
    })
    .await
    .unwrap();

    // ── 3. Autopilot 晨间建单:cron create_issue 模式到期 → 自动建 Todo Issue。 ──
    let cron_task = CronTaskId::new();
    app.dispatch(Command::CreateAutopilotTask {
        id: cron_task,
        name: "晨间建单".into(),
        schedule: Cadence::Daily,
        project_id: Some(project),
        stage: STAGE,
        assignee: Some(AGENT_NAME.into()),
    })
    .await
    .unwrap();
    let fired = app.tick_scheduler().await.unwrap();
    assert_eq!(fired.len(), 1, "到期任务自动触发,no-hijack");

    let issues = store.list_issues(project, Some(STAGE), None).await.unwrap();
    assert_eq!(issues.len(), 1, "Autopilot 建了一件活");
    let issue_a = issues[0].id;
    assert_eq!(
        issues[0].status,
        IssueStatus::Todo,
        "自动建单 = 承诺的活,不是 Backlog 停车场"
    );
    assert_eq!(issues[0].assignee, Some(agent), "已按名路由给真实 agent");
    assert_eq!(
        store.count_open_issues(project).await.unwrap(),
        1,
        "开放计数即时反映新 Issue"
    );

    // ── 4. RunIssue:一键跑活(Mock),真实推进到 InReview。 ──
    let session = SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: Some(STAGE),
        kind: SessionKind::Optimize,
        title: "原型 · 一天的活".into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunIssue {
        session,
        id: issue_a,
    })
    .await
    .unwrap();

    let after_run = store.get_issue(issue_a).await.unwrap().unwrap();
    assert_eq!(
        after_run.status,
        IssueStatus::InReview,
        "run 成功只推到 InReview —— Done 永远是人的显式确认"
    );
    let runs = store.list_runs_for_issue(issue_a).await.unwrap();
    assert_eq!(runs.len(), 1, "run 绑定到这件 Issue");
    assert_eq!(runs[0].issue_id, Some(issue_a));
    let params: serde_json::Value = serde_json::from_str(&runs[0].params_json).unwrap();
    assert_eq!(
        params["playbook"], true,
        "跑的是阶段角色剧本(per-phase prompts),不是裸提示词"
    );

    // ── 5. 人确认 Done:记账 / 产物 / 指标三线同时落地。 ──
    let agent_before = store.get_agent(agent).await.unwrap().unwrap();
    assert_eq!(agent_before.runs, 0, "确认前:还没有任何记账");

    app.dispatch(Command::TransitionIssue {
        id: issue_a,
        status: IssueStatus::Done,
    })
    .await
    .unwrap();

    let done_a = store.get_issue(issue_a).await.unwrap().unwrap();
    assert!(done_a.settled_at.is_some(), "Done 边沿留下 settle 戳");

    let agent_after = store.get_agent(agent).await.unwrap().unwrap();
    assert_eq!(agent_after.runs, 1, "agent 记账 +1");
    assert_eq!(agent_after.win_rate, "100%", "一次真实的赢,派生的 win_rate");

    // Artifact reflux on the Done edge is gated on a configured workspace
    // (this test has none — see the module doc); real-workspace artifact
    // registration is proven separately by `complete_form.rs`. Honest empty,
    // not a fabricated "some artifact".
    assert!(
        store
            .list_artifacts_for_issue(issue_a)
            .await
            .unwrap()
            .is_empty(),
        "no workspace configured ⇒ nothing to scan, honestly"
    );

    let after_done_metrics = store.persisted_signals(project).await.unwrap().metrics;
    let stage_done = after_done_metrics
        .iter()
        .find(|m| m.name == "阶段完成 Issue 数" && m.stage_kind == Some(STAGE))
        .expect("原型阶段的完成计数指标存在");
    assert_eq!(stage_done.value_raw, "1", "机器观测 +1");
    assert_ne!(
        stage_done.signal,
        Some(Signal::Green),
        "没设目标,一个计数不会自己变绿"
    );

    // ── 6. 复利闭环:蒸馏技能 → 下一件活的 RunIssue 注入正文 → uses+1。 ──
    let skill = SkillId::new();
    app.dispatch(Command::DistillSkillFromIssue {
        skill_id: skill,
        issue_id: issue_a,
        name: "一天验收 · 交付法".into(),
        desc: "从今天完成的活蒸馏".into(),
        category: "方法论".into(),
        content: "## 一天验收 · 交付法\n1. 拆 Issue\n2. 真做\n3. 门禁绿才交付".into(),
    })
    .await
    .unwrap();
    let skill_before = store.get_skill(skill).await.unwrap().unwrap();
    assert_eq!(skill_before.uses, 0, "刚蒸馏,还没被骑过");

    let issue_b = IssueId::new();
    app.dispatch(Command::CreateIssue {
        id: issue_b,
        stage: STAGE,
        title: "第二件活(该复用刚蒸馏的方法)".into(),
        desc: String::new(),
        priority: IssuePriority::Medium,
    })
    .await
    .unwrap();
    app.dispatch(Command::AssignIssue {
        id: issue_b,
        assignee: Some(agent),
    })
    .await
    .unwrap();
    app.dispatch(Command::RunIssue {
        session,
        id: issue_b,
    })
    .await
    .unwrap();

    let skill_after = store.get_skill(skill).await.unwrap().unwrap();
    assert_eq!(
        skill_after.uses,
        skill_before.uses + 1,
        "复利闭环:蒸馏技能骑上了下一件活的 run"
    );

    // ── 7. 交棒:段内还留着 issue_b(InReview,非终态)→ 强制 risky 且留痕。 ──
    assert_eq!(
        store.count_open_issues(project).await.unwrap(),
        1,
        "issue_b 还在 InReview —— 算开放"
    );

    app.dispatch(Command::HandoffStage {
        risky: false,
        note: String::new(),
    })
    .await
    .unwrap();

    let handoffs = store.list_handoffs(project).await.unwrap();
    let latest = handoffs.first().expect("交棒已记录");
    assert!(
        latest.risky,
        "留有未完 Issue → 强制 risky,人不能假装干净离场"
    );
    assert!(
        latest.note.contains("留 1 件未完"),
        "交棒记录如实标注未完件数,实际记的是: {}",
        latest.note
    );

    // ── 8. 守卫抽查:非法直跳与空原因阻塞都必须被拒,状态原地不动。 ──
    let issue_c = IssueId::new();
    app.dispatch(Command::CreateIssue {
        id: issue_c,
        stage: STAGE,
        title: "守卫抽查用".into(),
        desc: String::new(),
        priority: IssuePriority::Low,
    })
    .await
    .unwrap();
    app.dispatch(Command::TransitionIssue {
        id: issue_c,
        status: IssueStatus::Todo,
    })
    .await
    .unwrap();

    let illegal_jump = app
        .dispatch(Command::TransitionIssue {
            id: issue_c,
            status: IssueStatus::Done,
        })
        .await;
    assert!(illegal_jump.is_err(), "Todo→Done 非法直跳必须被拒");

    let empty_block = app
        .dispatch(Command::BlockIssue {
            id: issue_c,
            reason: "   ".into(),
        })
        .await;
    assert!(empty_block.is_err(), "空原因的阻塞必须被拒");

    let unchanged = store.get_issue(issue_c).await.unwrap().unwrap();
    assert_eq!(unchanged.status, IssueStatus::Todo, "两次被拒,状态原地不动");

    let _ = std::fs::remove_file(&db);
}
