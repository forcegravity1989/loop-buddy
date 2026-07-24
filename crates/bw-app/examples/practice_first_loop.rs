//! **practice_first_loop — 首次全真闭环践行指挥器(headless,零 stub)。**
//!
//! plan/13+14 合流后的第一次真实践行:对**真实日常库**里一个停在创建流
//! (`ColdStart`)的真实项目,把管理闭环的第一圈真正转起来——
//! 续流落地(`CompleteCreation`)→ 标配三件套真开 GitHub issue →
//! 竞品分析票真跑(真 `claude`,预算封顶)→ 自动提报告 PR →
//! **merge 永远留给人**(`merge` 模式是给人手验收用的入口,指挥器自己
//! 绝不自动调它)。
//!
//! 诚实约束(与 real_demo / verify_c8 同一血统):
//! 1. **零 stub**:`gh` / `claude` 都是 PATH 上用户自己的真实 CLI;网关
//!    抖动/配额/预算腰斩都是合法结果,如实打印、绝不吞掉。
//! 2. **结论读回为证**:每步之后从 store 读回真实状态打印;独立复核用
//!    `sqlite3` 对同一个库再查一遍(本文件结尾打印可直接复制的查询)。
//! 3. **Done 永不自动**:run 成功只到评审中(InReview + 真 PR);
//!    `merge` 模式存在的唯一意义是让人从 BW 真实命令层(`MergeIssuePr`)
//!    验收,避免绕过 BW 直接在网页 merge 造成状态漂移(C11 #42 未建)。
//!
//! 用法(db/ws-root 用真实日常库的那套路径):
//!   cargo run -p bw-app --example practice_first_loop -- <db> <ws-root> <项目名> status
//!   cargo run -p bw-app --example practice_first_loop -- <db> <ws-root> <项目名> complete
//!   cargo run -p bw-app --example practice_first_loop -- <db> <ws-root> <项目名> run <issue-number>
//!   cargo run -p bw-app --example practice_first_loop -- <db> <ws-root> <项目名> merge <issue-number>   # 人手验收专用
//!
//! 环境变量:`BW_CLAUDE_BIN`(默认 PATH 上的 claude)·
//! `BW_CLAUDE_MAX_BUDGET_USD`(默认 0.30,探针实测检索一轮 ~$0.15)。

use bw_app::{App, Command, Event};
use bw_core::model::{Cadence, StageKind};
use bw_core::SessionId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SessionKind, SqliteStore, Store};
use std::path::PathBuf;
use std::sync::Arc;

async fn dump_state(store: &Arc<dyn Store>, name: &str) {
    let Some(proj) = store
        .list_projects()
        .await
        .expect("list projects")
        .into_iter()
        .find(|p| p.name == name)
    else {
        println!("  (项目「{name}」不存在)");
        return;
    };
    println!(
        "  项目「{}」 phase={:?} remote={:?}",
        proj.name, proj.phase, proj.github_remote
    );
    let issues = store.list_issues(proj.id, None, None).await.unwrap();
    if issues.is_empty() {
        println!("  issue: 0 张(创建流未落地即如此,如实留白)");
    }
    for i in &issues {
        println!(
            "  #{} [{}] status={:?} github#={} pr#={} skill={} settled={:?}",
            i.number,
            i.title,
            i.status,
            i.github_number,
            i.pr_number,
            i.standard_skill,
            i.settled_at
        );
        for r in store.list_runs_for_issue(i.id).await.unwrap() {
            println!(
                "      run {:?} phases={} dur={}ms err={:?}",
                r.status,
                r.phases_completed,
                r.duration_ms.unwrap_or(0),
                if r.error.is_empty() { "-" } else { &r.error }
            );
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage =
        "usage: practice_first_loop <db> <ws-root> <项目名> <status|complete|run N|merge N>";
    let db_path = args.get(1).cloned().expect(usage);
    let ws_root = PathBuf::from(args.get(2).cloned().expect(usage));
    let name = args.get(3).cloned().expect(usage);
    let mode = args.get(4).cloned().expect(usage);

    let budget: f64 = std::env::var("BW_CLAUDE_MAX_BUDGET_USD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.30);
    let binary = std::env::var("BW_CLAUDE_BIN").ok();
    println!(
        "[cfg] db={db_path} ws={} claude={} budget=${budget}",
        ws_root.display(),
        binary.as_deref().unwrap_or("(PATH)")
    );

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig {
            binary,
            max_budget_usd: budget,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    )
    .with_workspaces_root(ws_root);
    app.dispatch(Command::Boot).await.expect("boot");

    // 事件旁听:把真实 gh/claude 调用的 pending→ok/fail 与失败 toast 原样打出。
    let mut rx = app.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            match ev {
                Event::ActionProgress { name, state } => {
                    println!("  [action] {name} · {state:?}");
                }
                Event::ConnectorSynced { name, ok, detail } => {
                    println!("  [toast] {name} · ok={ok} · {detail}");
                }
                Event::WorkflowFailed(msg) => println!("  [toast] WorkflowFailed: {msg}"),
                _ => {}
            }
        }
    });

    let proj = store
        .list_projects()
        .await
        .expect("list projects")
        .into_iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("项目「{name}」不存在于 {db_path}"));

    println!("\n══ 起点状态(读回) ══");
    dump_state(&store, &name).await;

    match mode.as_str() {
        "status" => {}
        "complete" => {
            println!("\n══ OpenProject(续上中断的创建流)→ CompleteCreation ══");
            app.dispatch(Command::OpenProject(proj.id))
                .await
                .expect("open project");
            app.dispatch(Command::CompleteCreation {
                cadence: Cadence::Weekly,
                run_first: false, // 开工用显式 run 模式分步走,失败可独立重试
            })
            .await
            .expect("complete creation");
        }
        "run" => {
            let n: u32 = args.get(5).and_then(|s| s.parse().ok()).expect(usage);
            let issue = store
                .list_issues(proj.id, None, None)
                .await
                .unwrap()
                .into_iter()
                .find(|i| i.number == n)
                .unwrap_or_else(|| panic!("项目「{name}」没有 #{n}"));
            println!(
                "\n══ RunIssue #{} 「{}」(真 claude,预算 ${budget} 封顶) ══",
                issue.number, issue.title
            );
            app.dispatch(Command::OpenProject(proj.id))
                .await
                .expect("open project");
            let session = SessionId::new();
            app.dispatch(Command::StartSession {
                id: session,
                stage_kind: Some(StageKind::Prototype),
                kind: SessionKind::Create,
                title: format!("践行 · #{} {}", issue.number, issue.title),
            })
            .await
            .expect("start session");
            if let Err(e) = app
                .dispatch(Command::RunIssue {
                    session,
                    id: issue.id,
                })
                .await
            {
                println!("  RunIssue 返回错误(活按设计停在原地可重试):{e:?}");
            }
        }
        "merge" => {
            let n: u32 = args.get(5).and_then(|s| s.parse().ok()).expect(usage);
            let issue = store
                .list_issues(proj.id, None, None)
                .await
                .unwrap()
                .into_iter()
                .find(|i| i.number == n)
                .unwrap_or_else(|| panic!("项目「{name}」没有 #{n}"));
            println!(
                "\n══ MergeIssuePr #{} 「{}」(人手验收:merge PR #{} + 结账) ══",
                issue.number, issue.title, issue.pr_number
            );
            app.dispatch(Command::OpenProject(proj.id))
                .await
                .expect("open project");
            app.dispatch(Command::MergeIssuePr { id: issue.id })
                .await
                .expect("merge issue pr");
        }
        other => panic!("未知模式 {other:?} — {usage}"),
    }

    println!("\n══ 终点状态(读回) ══");
    dump_state(&store, &name).await;

    println!("\n══ 独立复核(sqlite3 直接对库再查) ══");
    println!("  sqlite3 \"{db_path}\" \"SELECT number,title,status,github_number,pr_number FROM issue;\"");
    println!("  sqlite3 \"{db_path}\" \"SELECT workflow_name,status,phases_completed,duration_ms,error FROM workflow_run ORDER BY started_at DESC LIMIT 5;\"");
}
