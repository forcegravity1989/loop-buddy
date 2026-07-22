//! **archive_aihot_evidence — 归档 aihot 试点的真实证据快照。**
//!
//! `practice-aihot/`(真实 SQLite + 带嵌套 `.git` 的真实项目工作区)按
//! `.gitignore` 的既有决定不进本仓——不是源码,且嵌套 `.git` 会造成 gitlink
//! 混乱。但试点的真实产出理应可核验、可长期留存,不能只活在某个人本地磁盘上。
//! 这个 example 把它读回成一份**可提交的 JSON 快照**:只读、零 mock、零编造——
//! 每个字段都来自 `Store` trait 的真实查询或 `bw_engine::evidence::collect`
//! 对真实工作区跑的只读 git 子命令,不解释、不派生、不代答。
//!
//! 用法:
//! ```text
//! cargo run -p bw-app --example archive_aihot_evidence -- \
//!     <db-path> <workspace-path> <output-json-path>
//! ```
//!
//! 详见 `plan/09-aihot-practice-run.md`、`iterations/PRACTICE-AIHOT.md`(叙事汇总)
//! ——本文件产出的 JSON 是它们的数字侧证据存档,两者互补,不重复。

use bw_core::model::{IssueStatus, ProjectCycle, ProjectPhase, RunStatus, Signal, StageKind};
use bw_core::ProjectId;
use bw_store::{SqliteStore, Store};
use serde::Serialize;
use std::collections::BTreeMap;

const PROJECT_NAME: &str = "aihot 日报";

#[derive(Serialize)]
struct EvidenceSnapshot {
    generated_at_unix: i64,
    source_db: String,
    source_workspace: String,
    project: ProjectEvidence,
    issues: IssueEvidence,
    agents: Vec<AgentEvidence>,
    skills: Vec<SkillEvidence>,
    workflow_runs: WorkflowRunEvidence,
    metrics: Vec<MetricEvidence>,
    observations_count: u32,
    handoffs_count: u32,
    workspace: WorkspaceEvidence,
}

#[derive(Serialize)]
struct ProjectEvidence {
    name: String,
    kind: String,
    desc: String,
    phase: String,
    cycle: String,
    active_stage: String,
    north_star: String,
    ns_def: String,
    benchmark: String,
    opportunity: String,
    created_at_unix: i64,
}

#[derive(Serialize)]
struct IssueEvidence {
    total: u32,
    by_status: BTreeMap<String, u32>,
}

#[derive(Serialize)]
struct AgentEvidence {
    name: String,
    model: String,
    runs: u32,
    win_rate: String,
}

#[derive(Serialize)]
struct SkillEvidence {
    name: String,
    category: String,
    uses: u32,
    distilled_from_a_real_issue: bool,
}

#[derive(Serialize)]
struct WorkflowRunEvidence {
    total: u32,
    by_status: BTreeMap<String, u32>,
}

#[derive(Serialize)]
struct MetricEvidence {
    name: String,
    value_raw: String,
    target_raw: String,
    driver: String,
    signal: String,
}

/// Local mirror of `bw_engine::evidence::WorkspaceEvidence` (that type has no
/// `Serialize` — adding one here keeps the engine crate untouched for a
/// one-off archival example).
#[derive(Serialize)]
struct WorkspaceEvidence {
    commit_count: u32,
    tracked_files: u32,
    dirty_paths: u32,
    docs_files: u32,
    recent_subjects: Vec<String>,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (db_path, workspace_path, output_path) = match (args.get(1), args.get(2), args.get(3)) {
        (Some(db), Some(ws), Some(out)) => (db.clone(), ws.clone(), out.clone()),
        _ => {
            eprintln!(
                "用法: cargo run -p bw-app --example archive_aihot_evidence -- \
                 <db-path> <workspace-path> <output-json-path>"
            );
            std::process::exit(1);
        }
    };

    let store: std::sync::Arc<dyn Store> = std::sync::Arc::new(
        SqliteStore::open(&db_path)
            .await
            .unwrap_or_else(|e| panic!("打开 DB 失败 {db_path:?}: {e}")),
    );

    let projects = store.list_projects().await.expect("list_projects");
    let project = projects
        .iter()
        .find(|p| p.name == PROJECT_NAME)
        .unwrap_or_else(|| panic!("DB 里找不到项目「{PROJECT_NAME}」:{db_path:?}"));
    let project_id: ProjectId = project.id;

    let project_evidence = ProjectEvidence {
        name: project.name.clone(),
        kind: project.kind.clone(),
        desc: project.desc.clone(),
        phase: match project.phase {
            ProjectPhase::Running => "running".into(),
            ProjectPhase::ColdStart => "cold_start".into(),
        },
        cycle: match project.cycle {
            ProjectCycle::Explore => "explore".into(),
            ProjectCycle::Expand => "expand".into(),
            ProjectCycle::Mature => "mature".into(),
        },
        active_stage: stage_key(project.active_stage).into(),
        north_star: project.north_star.clone(),
        ns_def: project.ns_def.clone(),
        benchmark: project.benchmark.clone(),
        opportunity: project.opportunity.clone(),
        created_at_unix: project.created_at,
    };

    let issues = store
        .list_issues(project_id, None, None)
        .await
        .expect("list_issues");
    let mut issues_by_status = BTreeMap::new();
    for i in &issues {
        *issues_by_status
            .entry(issue_status_key(i.status).to_string())
            .or_insert(0u32) += 1;
    }
    let issue_evidence = IssueEvidence {
        total: issues.len() as u32,
        by_status: issues_by_status,
    };

    let agents = store.list_agents().await.expect("list_agents");
    let agent_evidence: Vec<AgentEvidence> = agents
        .iter()
        .filter(|a| a.project_id == Some(project_id))
        .map(|a| AgentEvidence {
            name: a.name.clone(),
            model: a.model.clone(),
            runs: a.runs,
            win_rate: a.win_rate.clone(),
        })
        .collect();

    let skills = store.list_skills().await.expect("list_skills");
    let skill_evidence: Vec<SkillEvidence> = skills
        .iter()
        .filter(|s| s.project_id == Some(project_id))
        .map(|s| SkillEvidence {
            name: s.name.clone(),
            category: s.category.clone(),
            uses: s.uses,
            distilled_from_a_real_issue: s.distilled_from_issue.is_some(),
        })
        .collect();

    let runs = store
        .list_all_workflow_runs(1000)
        .await
        .expect("list_all_workflow_runs");
    let project_runs: Vec<_> = runs
        .iter()
        .filter(|r| r.project_id == Some(project_id))
        .collect();
    let mut runs_by_status = BTreeMap::new();
    for r in &project_runs {
        *runs_by_status
            .entry(run_status_key(r.status).to_string())
            .or_insert(0u32) += 1;
    }
    let workflow_run_evidence = WorkflowRunEvidence {
        total: project_runs.len() as u32,
        by_status: runs_by_status,
    };

    let sigs = store.persisted_signals(project_id).await.expect("signals");
    let metric_evidence: Vec<MetricEvidence> = sigs
        .metrics
        .iter()
        .map(|m| MetricEvidence {
            name: m.name.clone(),
            value_raw: m.value_raw.clone(),
            target_raw: m.target_raw.clone(),
            driver: m.driver.clone(),
            signal: signal_key(m.signal).to_string(),
        })
        .collect();

    let observations_count = store
        .list_observations(project_id)
        .await
        .expect("list_observations")
        .len() as u32;
    let handoffs_count = store
        .list_handoffs(project_id)
        .await
        .expect("list_handoffs")
        .len() as u32;

    let ws = bw_engine::evidence::collect(&workspace_path)
        .await
        .unwrap_or_else(|e| panic!("采集工作区证据失败 {workspace_path:?}: {e}"));
    let workspace_evidence = WorkspaceEvidence {
        commit_count: ws.commit_count,
        tracked_files: ws.tracked_files,
        dirty_paths: ws.dirty_paths,
        docs_files: ws.docs_files,
        recent_subjects: ws.recent_subjects,
    };

    let snapshot = EvidenceSnapshot {
        generated_at_unix: time::OffsetDateTime::now_utc().unix_timestamp(),
        source_db: canonical_label(&db_path),
        source_workspace: canonical_label(&workspace_path),
        project: project_evidence,
        issues: issue_evidence,
        agents: agent_evidence,
        skills: skill_evidence,
        workflow_runs: workflow_run_evidence,
        metrics: metric_evidence,
        observations_count,
        handoffs_count,
        workspace: workspace_evidence,
    };

    let json = serde_json::to_string_pretty(&snapshot).expect("serialize snapshot");
    if let Some(parent) = std::path::Path::new(&output_path).parent() {
        std::fs::create_dir_all(parent).expect("create output dir");
    }
    std::fs::write(&output_path, &json).expect("write evidence json");

    println!("╔══ aihot 试点 · 证据归档 ══╗");
    println!("  项目:{}", snapshot.project.name);
    println!(
        "  Issue:{} 条(按状态:{:?})",
        snapshot.issues.total, snapshot.issues.by_status
    );
    println!(
        "  workflow_run:{} 次(按状态:{:?})",
        snapshot.workflow_runs.total, snapshot.workflow_runs.by_status
    );
    println!("  项目自有 agent:{} 个", snapshot.agents.len());
    println!("  项目自有 skill:{} 条", snapshot.skills.len());
    println!(
        "  真实观测:{} 条 · 真实交棒:{} 次",
        snapshot.observations_count, snapshot.handoffs_count
    );
    println!(
        "  工作区(真实 git):{} 次提交 · {} 个受追踪文件 · {} 个未提交路径",
        snapshot.workspace.commit_count,
        snapshot.workspace.tracked_files,
        snapshot.workspace.dirty_paths
    );
    println!("  已写入:{output_path}");
    println!("╚═══════════════════════════╝");
}

/// Strips the run-time machine-local prefix down to the canonical
/// `practice-aihot/...` path documented in `.gitignore` — the source path
/// actually passed on the CLI is wherever this one-off archival happened to
/// find the real data (a specific worktree's absolute path), which is
/// meaningless to anyone else and leaks a local username into a committed
/// file. What's worth recording is *which* pilot dataset this is, not *where
/// on disk* it happened to sit at archive time.
fn canonical_label(path: &str) -> String {
    match path.find("practice-aihot") {
        Some(i) => path[i..].to_string(),
        None => path.to_string(),
    }
}

fn stage_key(s: StageKind) -> &'static str {
    match s {
        StageKind::Prototype => "prototype",
        StageKind::Build => "build",
        StageKind::Optimize => "optimize",
        StageKind::Growth => "growth",
        StageKind::Ops => "ops",
    }
}

fn issue_status_key(s: IssueStatus) -> &'static str {
    match s {
        IssueStatus::Backlog => "backlog",
        IssueStatus::Todo => "todo",
        IssueStatus::InProgress => "in_progress",
        IssueStatus::InReview => "in_review",
        IssueStatus::Done => "done",
        IssueStatus::Blocked => "blocked",
        IssueStatus::Cancelled => "cancelled",
    }
}

fn run_status_key(s: RunStatus) -> &'static str {
    match s {
        RunStatus::Running => "running",
        RunStatus::Ok => "ok",
        RunStatus::Failed => "failed",
    }
}

/// `None` (never evaluated) is kept distinct from `Signal::Unknown` (evaluated,
/// no real data yet) — collapsing them would lose a real distinction.
fn signal_key(s: Option<Signal>) -> &'static str {
    match s {
        None => "none",
        Some(Signal::Green) => "green",
        Some(Signal::Amber) => "amber",
        Some(Signal::Red) => "red",
        Some(Signal::Unknown) => "unknown",
    }
}
