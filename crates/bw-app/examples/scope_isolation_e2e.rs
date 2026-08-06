//! plan/20 W6 · 三层隔离 E2E 指挥器:两个项目 + 全局库,全部走真实命令层
//! (与 UI 同一条 `Command` 路径),每个断言都能用 `sqlite3 <db>` 独立复核
//! (读回为证——本文件打印的每个数字都来自 store 读回,零硬编码)。
//!
//! Usage: `cargo run -p bw-app --example scope_isolation_e2e -- <db-path>`
//! 复核:  `sqlite3 <db> "SELECT name,project_id,runs FROM agent WHERE name='构建师'"`

use bw_app::{AdoptTarget, App, Command};
use bw_core::model::{Cadence, IssuePriority, IssueStatus, MaturityPeriod, StageKind};
use bw_core::{IssueId, ProjectId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let path = std::env::args().nth(1).expect("用法: … -- <db-path>");
    let _ = std::fs::remove_file(&path);
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    // ── W1:两个项目,出生即带自有五角色 ──
    let a = ProjectId::new();
    let b = ProjectId::new();
    for (pid, name) in [(a, "隔离甲"), (b, "隔离乙")] {
        app.dispatch(Command::CreateProject {
            provider: "github".into(),
            id: pid,
            name: name.into(),
            kind: "e2e".into(),
            desc: String::new(),
            workspace: None,
            github: None,
            codehub: None,
        })
        .await
        .unwrap();
        app.dispatch(Command::SetCycle {
            cycle: MaturityPeriod::Explore,
        })
        .await
        .unwrap();
        app.dispatch(Command::CompleteCreation {
            cadence: Cadence::Weekly,
            run_first: false,
        })
        .await
        .unwrap();
    }
    let agents = store.list_agents().await.unwrap();
    let own = |p: ProjectId| agents.iter().filter(|x| x.project_id == Some(p)).count();
    println!("[W1] 甲自有 agent {} 行,乙自有 {} 行", own(a), own(b));
    assert!(own(a) >= 5 && own(b) >= 5, "出生未种五角色副本");

    // W1 幂等:再 Boot 一次,补种不重复。
    let before = own(a);
    app.dispatch(Command::Boot).await.unwrap();
    let after_boot = store
        .list_agents()
        .await
        .unwrap()
        .iter()
        .filter(|x| x.project_id == Some(a))
        .count();
    assert_eq!(after_boot, before, "Boot 补种不幂等");
    println!("[W1] 二次 Boot 幂等:甲仍 {after_boot} 行");

    // ── R1 命令层守卫:跨项目指派被拒,本项目指派放行 ──
    app.dispatch(Command::OpenProject(a)).await.unwrap();
    let issue = IssueId::new();
    app.dispatch(Command::CreateIssue {
        id: issue,
        stage: StageKind::Build,
        title: "隔离验证".into(),
        desc: String::new(),
        priority: IssuePriority::Medium,
        standard_skill: String::new(),
    })
    .await
    .unwrap();
    let b_builder = agents
        .iter()
        .find(|x| x.project_id == Some(b) && x.name == "构建师")
        .expect("乙的构建师副本");
    let cross = app
        .dispatch(Command::AssignIssue {
            id: issue,
            assignee: Some(b_builder.id),
        })
        .await;
    assert!(cross.is_err(), "跨项目指派竟被放行");
    println!("[R1] 跨项目指派被拒:{}", cross.unwrap_err());
    let a_builder = agents
        .iter()
        .find(|x| x.project_id == Some(a) && x.name == "构建师")
        .expect("甲的构建师副本")
        .clone();
    app.dispatch(Command::AssignIssue {
        id: issue,
        assignee: Some(a_builder.id),
    })
    .await
    .unwrap();

    // ── R3:Done 边战绩只落甲的那一行(by-id;同名的乙副本/全局行零污账)──
    for st in [
        IssueStatus::Todo,
        IssueStatus::InProgress,
        IssueStatus::InReview,
        IssueStatus::Done,
    ] {
        app.dispatch(Command::TransitionIssue {
            id: issue,
            status: st,
        })
        .await
        .unwrap();
    }
    let after = store.list_agents().await.unwrap();
    let runs_of = |p: Option<ProjectId>| {
        after
            .iter()
            .filter(|x| x.project_id == p && x.name == "构建师")
            .map(|x| x.runs)
            .sum::<u32>()
    };
    println!(
        "[R3] Done 边后 构建师 runs:甲={} 乙={} 全局={}",
        runs_of(Some(a)),
        runs_of(Some(b)),
        runs_of(None)
    );
    assert_eq!(runs_of(Some(a)), 1, "甲的构建师未记到战绩");
    assert_eq!(runs_of(Some(b)), 0, "乙的同名副本被污账");
    assert_eq!(runs_of(None), 0, "全局共享行被污账");

    // ── R5:收录 = 复制归我(尾注/支撑文件/source 保真/新账)──
    let skills = store.list_skills().await.unwrap();
    let global_skill = skills
        .iter()
        .find(|s| s.project_id.is_none() && s.name == "evidence-first")
        .expect("全局 evidence-first(bw-standard 种子)");
    app.dispatch(Command::AdoptIntoProject {
        target: AdoptTarget::Skill(global_skill.id),
        project_id: a,
    })
    .await
    .unwrap();
    let skills2 = store.list_skills().await.unwrap();
    let copy = skills2
        .iter()
        .find(|s| s.project_id == Some(a) && s.name == "evidence-first")
        .expect("收录副本未落库");
    assert!(copy.desc.contains("引入自"), "描述尾注缺失:{}", copy.desc);
    assert_eq!(copy.content, global_skill.content, "正文未复制");
    assert_eq!(copy.uses, 0, "uses 未清零");
    assert_eq!(copy.source, global_skill.source, "source 未保真");
    println!("[R5] 收录副本:desc=「{}」", copy.desc);

    // R4:重复收录同池同名被拒;他项目行不可被收录。
    let dup = app
        .dispatch(Command::AdoptIntoProject {
            target: AdoptTarget::Skill(global_skill.id),
            project_id: a,
        })
        .await;
    assert!(dup.is_err(), "重复收录竟被放行");
    println!("[R4] 重复收录被拒:{}", dup.unwrap_err());
    let steal = app
        .dispatch(Command::AdoptIntoProject {
            target: AdoptTarget::Skill(copy.id),
            project_id: b,
        })
        .await;
    assert!(steal.is_err(), "项目自有行竟可被他项目收录");
    println!("[R5] 项目自有行收录被拒:{}", steal.unwrap_err());

    // ── R2:就近遮蔽——同一个名字,甲解析到甲副本,乙解析到全局行 ──
    // (scoped_pick 正是注入链/记账/cron 共用的那一个函数,不是平行实现。)
    let pick_a = bw_core::scope::scoped_pick(
        skills2.iter(),
        Some(a),
        |s| s.project_id,
        |s| s.name == "evidence-first",
    )
    .unwrap();
    let pick_b = bw_core::scope::scoped_pick(
        skills2.iter(),
        Some(b),
        |s| s.project_id,
        |s| s.name == "evidence-first",
    )
    .unwrap();
    assert_eq!(pick_a.id, copy.id, "甲未遮蔽到项目副本");
    assert_eq!(pick_b.id, global_skill.id, "乙未兜底到全局行");
    println!("[R2] 就近遮蔽:甲→副本行,乙→全局行");

    println!("[OK] plan/20 三层隔离 E2E 全部读回通过 · db={path}");
}
