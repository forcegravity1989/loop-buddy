//! **verify_c13_draft_mock_lock — C13 起草秒出:创建流起草锁 mock 直达 Review
//! headless E2E 指挥器(plan/14 C13, plan/13 D8 回锁)。**
//!
//! 不开 UI,把创建流的「快速问题 → 起草」步骤按 `QuestionsCard` 提交时的真实
//! 命令序列走一遍(`SetCycle` → `UpdateBrief` → `StartSession` →
//! `RunDraftWorkflow`),全部经真实命令层(`dispatch`),结论一律 `sqlite3`
//! 独立读回为证:
//!
//!   ① 项目 A(github=New,挂了一个真实本地工作区——P1「建项目即建仓」+ plan/13
//!      GitHub 主体化的真实结果)· 起草跑完后,自带 PATH 前置的【mock】stub
//!      `claude` 日志文件**零条目**(不存在或为空)—— 证明 `RunDraftWorkflow`
//!      即便面对一个已有真实工作区的项目,也一次真实 `claude -p` 调用都没有
//!      发生(修正 plan/14 §「技术成因」记录的意外路由:旧代码此处会经
//!      `agent_cli=="claude-code" && workspace_path 非空` 走真执行器)。
//!   ② 起草秒级完成(实测 elapsed 打印,期望远低于旧真执行器单相位 30 分钟
//!      超时/ $0.5 预算封顶的量级)。
//!   ③ `workflow_run.params_json` 里 `force_mock` 字段为真、`phases` 数组
//!      恰好三项且不含「周期判定」(该相位已按 grilling 拍板砍掉——周期由用户
//!      在 Questions 卡手选的 chip 决定,机器不重判)。
//!   ④ `message` 表里该 session 的三条留痕消息全部带【mock】自我标注。
//!   ⑤ 项目 B(github=None,无工作区)走同一条起草路径,行为与项目 A 一致
//!      (`force_mock` 恒真、零 stub claude 调用)—— 证明"有无工作区"不再影响
//!      创建流起草的路由决定。
//!
//! **D8 回锁的对照(不在本例内真跑,详见完成报告的代码引用)**:标配 Issue 的
//! `RunIssue` → `run_issue_now` → `run_workflow_inner(..., force_mock=false)`
//! 未被本票触碰,继续按 `agent_cli`/`workspace_path` 走既有真执行器路由。
//!
//! **gh / claude 全程被自带的【mock】stub 顶替**——写进
//! `<ws_root>/.stub-bin/{gh,claude}` 并前置进 PATH,输出/日志自我标注
//! 【mock】,绝不冒充真实 GitHub / Anthropic。
//!
//! 用法:
//!   cargo run -p bw-app --example verify_c13_draft_mock_lock -- <db-path> <workspaces-root>

use bw_app::{App, Command, GithubOrigin};
use bw_core::model::MaturityPeriod;
use bw_core::{ProjectId, SessionId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SessionKind, SqliteStore, Store};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// A self-labeled 【mock】 `gh` — NOT real GitHub. Only handles the
/// subcommands this scenario reaches: `api user`(login)、`repo create …
/// --clone`(offline bare+clone,给项目 A 一个真实本地工作区)。
const STUB_GH: &str = r#"#!/bin/sh
# 【mock】stub gh for C13 draft-mock-lock E2E — self-labeled, NOT real GitHub.
if [ "$1" = "api" ] && [ "$2" = "user" ]; then
  echo "testowner"
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "create" ]; then
  slug="$3"
  bare="$STUB_GH_REMOTES/$slug.git"
  mkdir -p "$STUB_GH_REMOTES"
  git init --bare -q "$bare"
  git clone -q "$bare" "$slug"
  echo "https://github.com/testowner/$slug"
  exit 0
fi
exit 0
"#;

/// A self-labeled 【mock】 `claude` CLI — NOT the real Anthropic API. If this
/// is ever invoked, it logs one line to `$STUB_CLAUDE_LOG` — the test's whole
/// point is that `RunDraftWorkflow` must leave this log file untouched.
const STUB_CLAUDE: &str = r#"#!/bin/sh
# 【mock】stub claude CLI for C13 draft-mock-lock E2E — NOT the real Anthropic API.
{
  echo "===INVOKED $(date +%s%N) args=$*==="
} >> "$STUB_CLAUDE_LOG"
printf '{"result":"【mock】stub claude — should NEVER be reached by RunDraftWorkflow.","is_error":false}\n'
exit 0
"#;

/// Runs the exact `QuestionsCard` submit sequence (create.rs) against
/// project `pid`, timing just the `RunDraftWorkflow` dispatch (the part that
/// must be honestly 秒级). Returns the session the drafting run attached to.
async fn run_drafting(app: &mut App, pid: ProjectId, label: &str) -> SessionId {
    app.dispatch(Command::OpenProject(pid))
        .await
        .unwrap_or_else(|e| panic!("open project {label}: {e}"));
    app.dispatch(Command::SetCycle {
        cycle: MaturityPeriod::Explore,
    })
    .await
    .unwrap_or_else(|e| panic!("SetCycle {label}: {e}"));
    app.dispatch(Command::UpdateBrief {
        benchmark: "Linear".into(),
        opportunity: "三个月内被持续复用".into(),
    })
    .await
    .unwrap_or_else(|e| panic!("UpdateBrief {label}: {e}"));

    let session = SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: Some(bw_core::model::StageKind::Prototype),
        kind: SessionKind::Create,
        title: "创建 · 体系起草".into(),
    })
    .await
    .unwrap_or_else(|e| panic!("StartSession {label}: {e}"));

    let t0 = Instant::now();
    app.dispatch(Command::RunDraftWorkflow {
        session,
        spec: bw_core::model::drafting_workflow(),
    })
    .await
    .unwrap_or_else(|e| panic!("RunDraftWorkflow {label}: {e}"));
    let elapsed = t0.elapsed();
    println!("  [{label}] RunDraftWorkflow elapsed = {elapsed:?}");
    assert!(
        elapsed.as_secs() < 10,
        "{label}: 起草应秒级完成,实测 {elapsed:?} 远超预期(旧真执行器路径单相位可耗时到 30 分钟超时上限)"
    );

    session
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let db_path = args
        .get(1)
        .cloned()
        .expect("usage: verify_c13_draft_mock_lock <db-path> <workspaces-root>");
    let ws_root = PathBuf::from(
        args.get(2)
            .cloned()
            .expect("usage: verify_c13_draft_mock_lock <db-path> <workspaces-root>"),
    );
    std::fs::create_dir_all(&ws_root).unwrap();

    // Write the self-contained 【mock】 gh + claude stubs and put them first on PATH.
    let stub_bin = ws_root.join(".stub-bin");
    std::fs::create_dir_all(&stub_bin).unwrap();
    let gh = stub_bin.join("gh");
    std::fs::write(&gh, STUB_GH).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let claude = stub_bin.join("claude");
    std::fs::write(&claude, STUB_CLAUDE).unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", stub_bin.display(), old_path));
    std::env::set_var(
        "STUB_GH_REMOTES",
        ws_root.join(".stub-remotes").display().to_string(),
    );
    let claude_log = ws_root.join(".stub-claude.log");
    let _ = std::fs::remove_file(&claude_log);
    std::env::set_var("STUB_CLAUDE_LOG", claude_log.display().to_string());
    println!(
        "[stub] gh     → {} (【mock】, NOT real GitHub)",
        gh.display()
    );
    println!(
        "[stub] claude → {} (【mock】, NOT real Anthropic API — RunDraftWorkflow must NEVER touch it)",
        claude.display()
    );
    println!("[stub] claude invocation log → {}", claude_log.display());

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        // A visible per-phase delay (like the real desktop shell's
        // 450ms) so a live subscriber would still see phase-by-phase
        // streaming — proves "秒级" isn't just "zero-delay mock", it is
        // genuinely fast at the shell's own real latency budget too.
        Engine::new(Arc::new(MockExecutor::with_delay(
            std::time::Duration::from_millis(200),
        ))),
        ClaudeCliConfig {
            binary: None, // resolved from PATH — picks up the stub above
            max_budget_usd: 0.5,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    )
    .with_workspaces_root(ws_root.clone());
    app.dispatch(Command::Boot).await.expect("boot");

    // ═══════════════ 项目 A · 挂仓(真实本地工作区)═══════════════
    println!("\n① 项目 A(挂仓,真实本地工作区)· CreateProject github=New …");
    let proj_a = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_a,
        name: "C13 起草锁 mock 演示 A".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C13 headless E2E · 挂仓项目,起草必须仍锁 mock".into(),
        workspace: None,
        github: Some(GithubOrigin::New {
            slug: "c13-draft-mock-demo-a".into(),
            private: true,
        }),
    })
    .await
    .expect("create project A");
    let proj_a_row = store.get_project(proj_a).await.unwrap().unwrap();
    assert!(
        !proj_a_row.workspace_path.trim().is_empty(),
        "前提条件:项目 A 必须有真实工作区(否则本例不构成 D8 回归证明)"
    );
    println!(
        "  premise: 项目 A workspace_path = {:?} (非空 —— 若不加 force_mock,旧代码会路由到真执行器)",
        proj_a_row.workspace_path
    );

    println!("\n② 项目 A · 快速问题 → RunDraftWorkflow …");
    let session_a = run_drafting(&mut app, proj_a, "项目 A(挂仓)").await;

    // ═══════════════ 项目 B · 无工作区 ═══════════════
    // `CreateProject { github: None }` 在配置了 `workspaces_root` 的 App 上
    // 仍会走 `(None, None)` 分支自动本地 mint 一个工作区(P1 的既有默认行为,
    // 与本票无关、不是本票要改的东西)——所以要拿到一个真正"无工作区"的项目
    // 当对照组,显式 `SetWorkspace { path: "" }` 清空它,同真实用户能做的操作
    // 一致(SetWorkspace 文档:"空 path=清空"),诚实模拟旧项目/未配置工作区
    // 的场景,而不是靠不传 workspaces_root 这种测试专属的旁路。
    println!("\n③ 项目 B(无工作区)· CreateProject github=None → SetWorkspace(清空) …");
    let proj_b = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_b,
        name: "C13 起草锁 mock 演示 B · 无工作区".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C13 headless E2E · 无工作区项目,起草行为应与项目 A 一致".into(),
        workspace: None,
        github: None,
    })
    .await
    .expect("create project B");
    app.dispatch(Command::SetWorkspace {
        path: String::new(),
        allow_commands: false,
    })
    .await
    .expect("clear workspace for project B");
    let proj_b_row = store.get_project(proj_b).await.unwrap().unwrap();
    assert!(
        proj_b_row.workspace_path.trim().is_empty(),
        "前提条件:项目 B 必须没有工作区(对照组)"
    );

    println!("\n④ 项目 B · 快速问题 → RunDraftWorkflow …");
    let session_b = run_drafting(&mut app, proj_b, "项目 B(无仓)").await;

    // ══════════════════════ 断言(独立复核以 sqlite3 CLI 另行读回为准)══════════════════════
    let claude_log_text = std::fs::read_to_string(&claude_log).unwrap_or_default();
    assert!(
        claude_log_text.trim().is_empty(),
        "stub claude 日志应为空 —— RunDraftWorkflow 不该发生任何一次(哪怕是 stub 的)claude 调用!实际内容:\n{claude_log_text}"
    );
    println!(
        "\n[proof] stub claude 日志文件存在={} 内容长度={} bytes → 零调用",
        claude_log.exists(),
        claude_log_text.len()
    );

    for (label, session) in [("项目 A", session_a), ("项目 B", session_b)] {
        let msgs = store.session_messages(session).await.unwrap();
        assert_eq!(
            msgs.len(),
            3,
            "{label}: 起草应恰好三条留痕消息(北极星起草/指标框架/阶段激活,无「周期判定」)"
        );
        for m in &msgs {
            assert!(
                m.text.contains("【mock】"),
                "{label}: 每条起草留痕消息都应自我标注【mock】,实际:{}",
                m.text
            );
            assert!(
                !m.text.contains("周期判定"),
                "{label}: 起草留痕消息不应再出现「周期判定」相位,实际:{}",
                m.text
            );
        }
        println!(
            "[proof] {label} session {} 的 3 条留痕消息:{:?}",
            session.uuid(),
            msgs.iter().map(|m| m.text.clone()).collect::<Vec<_>>()
        );
    }

    println!("\n✓ verify_c13_draft_mock_lock done");
    println!("  DB: {db_path}");
    println!(
        "  project A (挂仓, workspace_path={:?}) = {}",
        proj_a_row.workspace_path,
        proj_a.uuid()
    );
    println!(
        "  project B (无仓)                       = {}",
        proj_b.uuid()
    );
    println!("  session A = {}", session_a.uuid());
    println!("  session B = {}", session_b.uuid());
    println!(
        "  stub claude invocation log (应为空) = {}",
        claude_log.display()
    );
}
