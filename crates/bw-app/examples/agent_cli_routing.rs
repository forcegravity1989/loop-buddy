//! **agent_cli_routing — T6 agent_cli 真实路由 + tools→--allowedTools 的
//! headless E2E 指挥器。**
//!
//! plan/12 §3 拍板:Agent 声明的执行引擎(`agent_cli`)与工具白名单(`tools`,
//! ==AllowedTools)必须真实生效,不是展示标签。本指挥器在 MockExecutor 项目
//! (无真实工作区 —— 真实 `claude -p` 不作为验证手段,网关抖动)上全真跑通
//! `RunIssue` 链路,针对四个真实 Agent 逐一验证:
//!
//!   A `claude-code` + tools=["Read","Grep"] → 走通(InReview),
//!     run 记录 params_json 里 tools/allowed_tools_arg 真实在场
//!   B `claude-code` + tools=[]              → 走通,params_json 里
//!     allowed_tools_arg=null(不传参数,行为与现状一致)
//!   C `codex`                                → 诚实失败(run 行 status=failed,
//!     error 含「暂不支持/未安装」),Issue 仍 InProgress(RunIssue 返回 Err)
//!   D `cursor`                               → 同 C
//!
//! 用法:agent_cli_routing <db-path>
//!   跑完后用 sqlite3 <db> 独立复核(读回为证,见文件末尾打印的 SQL)。

use bw_app::{App, Command};
use bw_core::model::{HubSource, IssuePriority, Maturity, StageKind};
use bw_core::{AgentId, IssueId, ProjectId, SessionId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{NewAgent, NewSession, SessionKind, SqliteStore, Store};
use std::sync::Arc;

/// Direct `Store::create_agent` — T6's target Agent-fixture fields
/// (`agent_cli`/`tools`) have no `Command::CreateAgent` surface yet (that
/// command predates T6 and still hard-codes `agent_cli: "claude-code"`; T6's
/// scope is routing + the CLI adapter, not the creation UI). Same pattern
/// `adversarial_loop.rs` already uses for `ensure_session` — scaffolding a
/// fixture directly against the store, then exercising the REAL behavior
/// under test (`RunIssue`) through `App::dispatch`.
async fn make_agent(
    store: &Arc<dyn Store>,
    name: &str,
    agent_cli: &str,
    tools: Vec<String>,
) -> AgentId {
    let id = AgentId::new();
    store
        .create_agent(NewAgent {
            id,
            name: name.to_string(),
            role: format!("T6 E2E · {agent_cli}"),
            stage_ref: None,
            maturity: Maturity::Fresh,
            skills: Vec::new(),
            model: "sonnet".to_string(),
            instructions: String::new(),
            tools,
            agent_cli: agent_cli.to_string(),
            source: HubSource::SelfBuilt,
            project_id: None,
        })
        .await
        .expect("create agent");
    id
}

/// Mint a real session row (RunIssue appends messages to it — the FK target
/// must exist first).
async fn new_session(store: &Arc<dyn Store>, pid: ProjectId, title: &str) -> SessionId {
    let session = SessionId::new();
    store
        .ensure_session(NewSession {
            id: session,
            project_id: pid,
            stage_kind: Some(StageKind::Build),
            kind: SessionKind::Optimize,
            title: title.into(),
            snippet: String::new(),
        })
        .await
        .expect("ensure session");
    session
}

/// Create a Backlog Build-stage issue, assign it to `agent`, run it through
/// the real `RunIssue` command path, then print the settled `workflow_run`
/// row's real fields — the same evidence a later `sqlite3` read-back checks
/// independently.
async fn run_one(
    app: &mut App,
    store: &Arc<dyn Store>,
    pid: ProjectId,
    label: &str,
    agent: AgentId,
) -> IssueId {
    let issue_id = IssueId::new();
    app.dispatch(Command::CreateIssue {
        id: issue_id,
        stage: StageKind::Build,
        title: format!("T6 · {label}"),
        desc: "agent_cli 路由 E2E 演示件".into(),
        priority: IssuePriority::Medium,
    })
    .await
    .expect("create issue");
    app.dispatch(Command::AssignIssue {
        id: issue_id,
        assignee: Some(agent),
    })
    .await
    .expect("assign issue");

    let session = new_session(store, pid, label).await;
    let res = app
        .dispatch(Command::RunIssue {
            session,
            id: issue_id,
        })
        .await;

    let issue = store
        .get_issue(issue_id)
        .await
        .expect("issue")
        .expect("row");
    let runs = store
        .list_runs_for_issue(issue_id)
        .await
        .expect("runs for issue");
    let last = runs.first().expect("at least one run row");
    println!("── {label} ──────────────────────────────────────");
    println!(
        "  RunIssue dispatch = {}",
        if res.is_ok() {
            "Ok"
        } else {
            "Err(诚实失败)"
        }
    );
    println!("  Issue #{} 现状 = {}", issue.number, issue.status.label());
    println!(
        "  workflow_run · status={} · error={:?}",
        last.status.text(),
        last.error
    );
    println!("  params_json = {}", last.params_json);
    println!();
    issue_id
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let db = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: agent_cli_routing <db-path>");
        std::process::exit(2);
    });

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.expect("open db"));

    // Unscripted mock: a plain Build-stage playbook run auto-passes its
    // Evaluator gate (mock.rs's documented default-PASS), so a claude-code
    // route completes in one round with no scripting needed — the point
    // here is `agent_cli` routing + `tools` plumbing, not the review loop
    // (T9's concern, already covered by `adversarial_loop.rs`).
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.expect("boot");

    let pid = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: pid,
        name: "T6 agent_cli 路由演示项目".into(),
        kind: "demo".into(),
        desc: "agent_cli routing E2E — 全程 MockExecutor(无真实工作区)".into(),
        workspace: None,
    })
    .await
    .expect("create project");
    app.dispatch(Command::OpenProject(pid))
        .await
        .expect("open project");

    println!("== T6 agent_cli 路由 + tools→--allowedTools · headless E2E ==\n");

    let claude_tools = make_agent(
        &store,
        "Claude 真身 · Read+Grep",
        "claude-code",
        vec!["Read".to_string(), "Grep".to_string()],
    )
    .await;
    let claude_no_tools = make_agent(&store, "Claude 真身 · 无限制", "claude-code", vec![]).await;
    let codex_agent = make_agent(&store, "Codex 队友(未接入)", "codex", vec![]).await;
    let cursor_agent = make_agent(&store, "Cursor 队友(未接入)", "cursor", vec![]).await;

    let issue_a = run_one(
        &mut app,
        &store,
        pid,
        "A · claude-code + tools=[Read,Grep]",
        claude_tools,
    )
    .await;
    let issue_b = run_one(
        &mut app,
        &store,
        pid,
        "B · claude-code + tools=[]（现状一致,不传参数）",
        claude_no_tools,
    )
    .await;
    let issue_c = run_one(
        &mut app,
        &store,
        pid,
        "C · codex（诚实报错,不装真）",
        codex_agent,
    )
    .await;
    let issue_d = run_one(
        &mut app,
        &store,
        pid,
        "D · cursor（诚实报错,不装真）",
        cursor_agent,
    )
    .await;

    println!("== 全部路径跑完 —— 用下面的 sqlite3 独立读回复核 ==\n");
    println!("  sqlite3 {db} \"SELECT id,name,agent_cli,tools FROM agent WHERE id IN \\");
    println!(
        "    ('{}','{}','{}','{}');\"",
        claude_tools.uuid(),
        claude_no_tools.uuid(),
        codex_agent.uuid(),
        cursor_agent.uuid()
    );
    println!(
        "  sqlite3 {db} \"SELECT i.number,i.status,r.status,r.error,\\
       json_extract(r.params_json,'$.agent_cli') AS agent_cli,\\
       json_extract(r.params_json,'$.tools') AS tools,\\
       json_extract(r.params_json,'$.allowed_tools_arg') AS allowed_tools_arg\\
     FROM workflow_run r JOIN issue i ON i.id=r.issue_id\\
     WHERE r.issue_id IN ('{}','{}','{}','{}') ORDER BY r.created_at;\"",
        issue_a.uuid(),
        issue_b.uuid(),
        issue_c.uuid(),
        issue_d.uuid()
    );
}
