//! **incubate_issue — Issue 层真实孵化指挥器(headless)。**
//!
//! 用途:在网关不可用(G4)时,按 plan/06 §6.4「编排后端真实工程」先例,
//! 经**公开命令面**驱动一件真实 Issue 走完生命周期——建卡→指派→开工→提审→
//! 人确认完成→蒸馏技能。真实工程产出由编排方在工作区落地(真代码/真测试/
//! 真 commit),执行方如实标注;本指挥器绝不 mock 任何记账,也绝不绕过
//! App 层守卫(Done 只能从 InReview 进、Done 边沿记账、settle-once 全部生效)。
//!
//! 每个动作结束都从 store 读回并打印证据(报告不代答)。
//!
//! 用法:
//!   incubate_issue <db> <项目名> status
//!   incubate_issue <db> <项目名> create <阶段> <标题> <描述>
//!   incubate_issue <db> <项目名> move <编号> <todo|in_progress|in_review|done>
//!   incubate_issue <db> <项目名> distill <编号> <技能名> <技能描述> <正文>

use bw_app::{App, Command};
use bw_core::model::{IssuePriority, IssueStatus, StageKind};
use bw_core::{IssueId, SkillId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

fn parse_stage(s: &str) -> Option<StageKind> {
    match s {
        "prototype" | "原型" => Some(StageKind::Prototype),
        "build" | "构建" => Some(StageKind::Build),
        "optimize" | "优化" => Some(StageKind::Optimize),
        "growth" | "增长" | "运营推广" => Some(StageKind::Growth),
        "ops" | "运维" => Some(StageKind::Ops),
        _ => None,
    }
}

fn parse_status(s: &str) -> Option<IssueStatus> {
    match s {
        "backlog" => Some(IssueStatus::Backlog),
        "todo" => Some(IssueStatus::Todo),
        "in_progress" => Some(IssueStatus::InProgress),
        "in_review" => Some(IssueStatus::InReview),
        "done" => Some(IssueStatus::Done),
        "cancelled" => Some(IssueStatus::Cancelled),
        _ => None,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (db, project_name, action) = match (args.first(), args.get(1), args.get(2)) {
        (Some(a), Some(b), Some(c)) => (a.clone(), b.clone(), c.clone()),
        _ => {
            eprintln!("usage: incubate_issue <db> <项目名> <status|create|move|distill> …");
            std::process::exit(2);
        }
    };

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.expect("boot");

    let project = store
        .list_projects()
        .await
        .expect("list projects")
        .into_iter()
        .find(|p| p.name == project_name)
        .unwrap_or_else(|| panic!("项目 {project_name:?} 不存在"));
    app.dispatch(Command::OpenProject(project.id))
        .await
        .expect("open project");

    let find_by_number = |issues: Vec<bw_core::model::Issue>, n: u32| {
        issues
            .into_iter()
            .find(|i| i.number == n)
            .unwrap_or_else(|| panic!("Issue #{n} 不存在"))
    };

    match action.as_str() {
        "status" => {
            let issues = store
                .list_issues(project.id, None, None)
                .await
                .expect("issues");
            println!("== {} · Issue 看板 ==", project.name);
            for i in &issues {
                println!(
                    "  #{} [{}] {} · 阶段={} · assignee={:?} · settled={}",
                    i.number,
                    i.status.label(),
                    i.title,
                    i.stage.role_short(),
                    i.assignee,
                    i.settled_at.is_some()
                );
            }
            println!("== agent 战绩(真实记账) ==");
            for a in store.list_agents().await.expect("agents") {
                if a.runs > 0 {
                    println!("  {} runs={} win_rate={}", a.name, a.runs, a.win_rate);
                }
            }
        }

        "create" => {
            let (stage, title, desc) = match (args.get(3), args.get(4), args.get(5)) {
                (Some(s), Some(t), Some(d)) => (
                    parse_stage(s).expect("阶段须为 prototype|build|optimize|growth|ops"),
                    t.clone(),
                    d.clone(),
                ),
                _ => panic!("create <阶段> <标题> <描述>"),
            };
            let id = IssueId::new();
            app.dispatch(Command::CreateIssue {
                id,
                stage,
                title,
                desc,
                priority: IssuePriority::Medium,
            })
            .await
            .expect("create issue");
            // 指派给该阶段的主持角色(与 Autopilot 同一路由口径)。
            let agent = store
                .list_agents()
                .await
                .expect("agents")
                .into_iter()
                .find(|a| a.name == stage.role_short())
                .unwrap_or_else(|| panic!("hub 中没有名为 {:?} 的 agent", stage.role_short()));
            app.dispatch(Command::AssignIssue {
                id,
                assignee: Some(agent.id),
            })
            .await
            .expect("assign");
            // 读回为证。
            let created = store
                .get_issue(id)
                .await
                .expect("get")
                .expect("issue 应已落库");
            println!(
                "已建卡(读回):#{} [{}] {} · 指派={}",
                created.number,
                created.status.label(),
                created.title,
                agent.name
            );
        }

        "move" => {
            let n: u32 = args
                .get(3)
                .expect("move <编号> <状态>")
                .parse()
                .expect("编号");
            let to = parse_status(args.get(4).expect("move <编号> <状态>")).expect("非法状态");
            let issues = store
                .list_issues(project.id, None, None)
                .await
                .expect("issues");
            let issue = find_by_number(issues, n);
            app.dispatch(Command::TransitionIssue {
                id: issue.id,
                status: to,
            })
            .await
            .expect("transition(守卫在 App 层,非法会拒绝)");
            let after = store
                .get_issue(issue.id)
                .await
                .expect("get")
                .expect("issue");
            println!(
                "已转移(读回):#{} → [{}] · settled={}",
                after.number,
                after.status.label(),
                after.settled_at.is_some()
            );
        }

        "distill" => {
            let n: u32 = args
                .get(3)
                .expect("distill <编号> <技能名> <描述>")
                .parse()
                .expect("编号");
            let (name, desc, content) = (
                args.get(4).expect("技能名").clone(),
                args.get(5).expect("技能描述").clone(),
                args.get(6).expect("正文(蒸馏技能必须带可执行做法)").clone(),
            );
            let issues = store
                .list_issues(project.id, None, None)
                .await
                .expect("issues");
            let issue = find_by_number(issues, n);
            let skill_id = SkillId::new();
            app.dispatch(Command::DistillSkillFromIssue {
                skill_id,
                issue_id: issue.id,
                name: name.clone(),
                desc,
                category: "孵化沉淀".into(),
                content,
            })
            .await
            .expect("distill(仅 Done 且有 assignee 可蒸馏)");
            let skill = store
                .list_skills()
                .await
                .expect("skills")
                .into_iter()
                .find(|s| s.name == name)
                .expect("蒸馏技能应已落库");
            println!(
                "已蒸馏(读回):{} · 溯源 issue={:?} · content {} 字",
                skill.name,
                skill.distilled_from_issue,
                skill.content.chars().count()
            );
        }

        other => {
            eprintln!("未知动作 {other:?}");
            std::process::exit(2);
        }
    }
}
