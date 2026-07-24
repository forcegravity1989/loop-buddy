//! **verify_c8_standard_trio — C8 标配 Issue 三件套 + 末卡「问一句就跑」
//! headless E2E 指挥器(plan/13 D8)。**
//!
//! 不开 UI,把创建流落地(`Command::CompleteCreation`)的标配三件套行为走
//! 一遍,全部经真实命令层(`dispatch`),结论一律 `sqlite3` 独立读回为证:
//!   ① 挂仓项目(`CreateProject` github=New)落地(`CompleteCreation`)→ 标配
//!      「竞品分析→找指标→绑数据」三张 Issue 自动建立,依赖序=编号序=1/2/3,
//!      每张都真开一个(stub)GitHub issue、`standard_skill` 关联落库;
//!   ② `run_first: true` → 落地后自动对①竞品分析 dispatch 一次真实
//!      `RunIssue`(stub claude 执行);`run_first: false` → 零 run;
//!   ③ 显式对②找指标(`standard_skill = "north-star-discovery"`,C9 已种)
//!      再跑一次,证明标配 Skill 的真实 content 到达了 executor 收到的 prompt
//!      (stub claude 把收到的 prompt 原样落盘,本例结束后拿真实文件 grep)、
//!      且 `uses` 记账 +1、恰好一次;
//!   ④ 无仓项目(github=None)落地 → 零标配票,如实留白;
//!   ⑤ 标配采集 cron(C7)与标配三件套共存不重复——每个挂仓项目仍然只有一条
//!      `CollectMetrics` cron。
//!
//! **gh / claude 全程被自带的【mock】stub 顶替**——写进
//! `<ws_root>/.stub-bin/{gh,claude}` 并前置进 PATH,输出/日志自我标注
//! 【mock】,绝不冒充真实 GitHub / Anthropic。真实账号/真实执行是另外的票
//! (plan/13 测试拍板)。
//!
//! 用法:
//!   cargo run -p bw-app --example verify_c8_standard_trio -- <db-path> <workspaces-root>
//!   cargo run -p bw-app --example verify_c8_standard_trio -- migrate-check <old-db-path>

use bw_app::{App, Command, Event, GithubOrigin};
use bw_core::{ProjectId, SessionId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SessionKind, SqliteStore, Store};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

/// A self-labeled 【mock】 `gh` — NOT real GitHub. Handles the subcommands
/// this scenario reaches: `api user`(login), `repo create … --clone`
/// (offline bare+clone), `issue create`(标配三件套建单), `pr create`(问一句
/// 就跑那条 RunIssue 的提 PR)。Issue/PR 号来自本地计数器文件,单调递增、
/// 非零即可——不需要真的匹配 GitHub 的全局编号规则。
const STUB_GH: &str = r#"#!/bin/sh
# 【mock】stub gh for C8 standard-trio E2E — self-labeled, NOT real GitHub.
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
if [ "$1" = "issue" ] && [ "$2" = "create" ]; then
  n=$(cat "$STUB_GH_COUNTER_ISSUE" 2>/dev/null || echo 0)
  n=$((n + 1))
  echo "$n" > "$STUB_GH_COUNTER_ISSUE"
  echo "https://github.com/testowner/stub/issues/$n"
  exit 0
fi
if [ "$1" = "pr" ] && [ "$2" = "create" ]; then
  n=$(cat "$STUB_GH_COUNTER_PR" 2>/dev/null || echo 0)
  n=$((n + 1))
  echo "$n" > "$STUB_GH_COUNTER_PR"
  echo "https://github.com/testowner/stub/pull/$n"
  exit 0
fi
# any other gh call: benign no-op
exit 0
"#;

/// A self-labeled 【mock】 `claude` CLI — NOT the real Anthropic API. Logs the
/// exact prompt it received (arg following `-p`) to `$STUB_CLAUDE_LOG`, one
/// phase per call, so the harness can independently grep the real file for
/// proof that skill content actually reached the executor. Touches no
/// workspace files (this scenario only needs to prove the prompt payload,
/// not exercise a full playbook phase).
const STUB_CLAUDE: &str = r#"#!/bin/sh
# 【mock】stub claude CLI for C8 standard-trio E2E — NOT the real Anthropic API.
prompt="$2"
{
  echo "===PHASE $(date +%s%N)==="
  printf '%s\n' "$prompt"
} >> "$STUB_CLAUDE_LOG"
printf '{"result":"【mock】stub claude phase output (C8 verify).\\nVERDICT: PASS\\nREASON: 【mock】E2E stub 放行(T9 评审门要求结构化裁决,合流后补齐)","is_error":false}\n'
exit 0
"#;

async fn dump_trio(store: &Arc<dyn Store>, project: ProjectId, label: &str) {
    let issues = store.list_issues(project, None, None).await.unwrap();
    println!("  ── {label}: {} issue(s) ──", issues.len());
    for i in &issues {
        println!(
            "    #{} [{}] github#={} standard_skill={:?} status={:?}",
            i.number, i.title, i.github_number, i.standard_skill, i.status
        );
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();

    // `migrate-check <old-db-path>` — just open (triggers the add_column_if_
    // missing guards) and exit. Used against a hand-stripped pre-C8 schema to
    // prove the old-DB double-guard migration is real (see the shell wrapper
    // that prepares the stripped schema before calling this mode).
    if args.get(1).map(String::as_str) == Some("migrate-check") {
        let db_path = args
            .get(2)
            .cloned()
            .expect("usage: migrate-check <db-path>");
        let _store = SqliteStore::open(&db_path)
            .await
            .expect("open db (migration)");
        println!("✓ migrate-check: opened {db_path} without error (migrations applied)");
        return;
    }

    // `coldstart <db-path> <ws-root> <project-name>` — CreateProject
    // (github=New) only, deliberately never CompleteCreation, so the project
    // stays `ProjectPhase::ColdStart`. Used to set up a fixture for the
    // desktop shell's `BW_OPEN=<name>` deep-link, which lands `ColdStart`
    // projects on `View::Create` (`Command::OpenProject`'s own rule) — the
    // Repo/Intent-已过、卡在 Questions 那步的 real-world shape a resumed
    // creation takes.
    if args.get(1).map(String::as_str) == Some("coldstart") {
        let db_path = args
            .get(2)
            .cloned()
            .expect("usage: coldstart <db> <ws> <name>");
        let ws_root = PathBuf::from(
            args.get(3)
                .cloned()
                .expect("usage: coldstart <db> <ws> <name>"),
        );
        let name = args
            .get(4)
            .cloned()
            .expect("usage: coldstart <db> <ws> <name>");
        std::fs::create_dir_all(&ws_root).unwrap();
        let stub_bin = ws_root.join(".stub-bin");
        std::fs::create_dir_all(&stub_bin).unwrap();
        let gh = stub_bin.join("gh");
        std::fs::write(&gh, STUB_GH).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let old_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{}", stub_bin.display(), old_path));
        std::env::set_var(
            "STUB_GH_REMOTES",
            ws_root.join(".stub-remotes").display().to_string(),
        );
        std::env::set_var(
            "STUB_GH_COUNTER_ISSUE",
            ws_root.join(".stub-gh-issue-n").display().to_string(),
        );
        let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
        let mut app = App::new(
            store.clone(),
            Engine::new(Arc::new(MockExecutor::new())),
            ClaudeCliConfig::default(),
        )
        .with_workspaces_root(ws_root);
        app.dispatch(Command::Boot).await.expect("boot");
        let id = ProjectId::new();
        app.dispatch(Command::CreateProject {
            id,
            name: name.clone(),
            kind: "CLI 工具 · Rust".into(),
            desc: "C8 深链 fixture · 停在创建流(未 CompleteCreation)".into(),
            workspace: None,
            github: Some(GithubOrigin::New {
                slug: "c8-coldstart-fixture".into(),
                private: true,
            }),
        })
        .await
        .expect("create project (coldstart fixture)");
        let proj = store.get_project(id).await.unwrap().unwrap();
        println!(
            "✓ coldstart fixture ready: name={name:?} phase={:?} github_remote={:?}",
            proj.phase, proj.github_remote
        );
        return;
    }

    let db_path = args
        .get(1)
        .cloned()
        .expect("usage: verify_c8_standard_trio <db-path> <workspaces-root>");
    let ws_root = PathBuf::from(
        args.get(2)
            .cloned()
            .expect("usage: verify_c8_standard_trio <db-path> <workspaces-root>"),
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
    std::env::set_var(
        "STUB_GH_COUNTER_ISSUE",
        ws_root.join(".stub-gh-issue-n").display().to_string(),
    );
    std::env::set_var(
        "STUB_GH_COUNTER_PR",
        ws_root.join(".stub-gh-pr-n").display().to_string(),
    );
    let claude_log = ws_root.join(".stub-claude.log");
    let _ = std::fs::remove_file(&claude_log);
    std::env::set_var("STUB_CLAUDE_LOG", claude_log.display().to_string());
    println!(
        "[stub] gh     → {} (【mock】, NOT real GitHub)",
        gh.display()
    );
    println!(
        "[stub] claude → {} (【mock】, NOT real Anthropic API)",
        claude.display()
    );
    println!("[stub] claude prompt log → {}", claude_log.display());

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig {
            binary: None, // resolved from PATH — picks up the stub above
            max_budget_usd: 1.0,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    )
    .with_workspaces_root(ws_root.clone());
    app.dispatch(Command::Boot).await.expect("boot");

    let mut rx = app.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            match ev {
                Event::ConnectorSynced { name, ok, detail } => {
                    println!("  [toast] {} · ok={} · {}", name, ok, detail);
                }
                Event::WorkflowFailed(msg) => {
                    println!("  [toast] WorkflowFailed: {msg}");
                }
                _ => {}
            }
        }
    });

    // ── 竞品分析/找指标/绑数据 三张标配 Skill 的 seed 基线(C9 种了后两
    // 张,C10 补种第一张,Boot 内三张都已种)──
    let ns_skill_uses_before = store
        .list_skills()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "north-star-discovery")
        .map(|s| s.uses)
        .unwrap_or(0);
    println!("\n[baseline] north-star-discovery.uses = {ns_skill_uses_before} (C9 seed)");
    let ca_skill_uses_before = store
        .list_skills()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "competitive-analysis")
        .map(|s| s.uses)
        .unwrap_or(0);
    println!("[baseline] competitive-analysis.uses = {ca_skill_uses_before} (C10 seed)");

    // ═══════════════ 项目 A · 挂仓 + run_first: true ═══════════════
    println!("\n① 项目 A(挂仓)· CreateProject github=New …");
    let proj_a = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_a,
        name: "标配三件套演示 A".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C8 headless E2E · run_first=true".into(),
        workspace: None,
        github: Some(GithubOrigin::New {
            slug: "c8-trio-demo-a".into(),
            private: true,
        }),
    })
    .await
    .expect("create project A");

    println!("\n② 项目 A · CompleteCreation(run_first: true) …");
    app.dispatch(Command::CompleteCreation {
        cadence: bw_core::model::Cadence::Weekly,
        run_first: true,
    })
    .await
    .expect("complete creation A");
    dump_trio(&store, proj_a, "项目 A(run_first=true)落地后").await;

    // 显式对②找指标(standard_skill = north-star-discovery)再跑一次 ——
    // 独立证明标配 Skill 注入到达了执行器收到的真实 prompt(stub claude 落盘)。
    let issues_a = store.list_issues(proj_a, None, None).await.unwrap();
    let issue2 = issues_a
        .iter()
        .find(|i| i.number == 2)
        .expect("②找指标 issue exists");
    println!(
        "\n③ 项目 A · 显式 RunIssue(#{} {}, standard_skill={:?}) …",
        issue2.number, issue2.title, issue2.standard_skill
    );
    let session2 = SessionId::new();
    app.dispatch(Command::StartSession {
        id: session2,
        stage_kind: Some(bw_core::model::StageKind::Prototype),
        kind: SessionKind::Create,
        title: "verify · 找指标 显式开工".into(),
    })
    .await
    .expect("start session for issue2");
    app.dispatch(Command::RunIssue {
        session: session2,
        id: issue2.id,
    })
    .await
    .expect("run issue2 (north-star-discovery)");

    // ═══════════════ 项目 B · 挂仓 + run_first: false ═══════════════
    println!("\n④ 项目 B(挂仓)· CreateProject github=New …");
    let proj_b = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_b,
        name: "标配三件套演示 B".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C8 headless E2E · run_first=false".into(),
        workspace: None,
        github: Some(GithubOrigin::New {
            slug: "c8-trio-demo-b".into(),
            private: true,
        }),
    })
    .await
    .expect("create project B");

    println!("\n⑤ 项目 B · CompleteCreation(run_first: false) …");
    app.dispatch(Command::CompleteCreation {
        cadence: bw_core::model::Cadence::Weekly,
        run_first: false,
    })
    .await
    .expect("complete creation B");
    dump_trio(&store, proj_b, "项目 B(run_first=false)落地后").await;

    // ═══════════════ 项目 C · 无仓 ═══════════════
    println!("\n⑥ 项目 C(无仓)· CreateProject github=None …");
    let proj_c = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_c,
        name: "标配三件套演示 C · 无仓".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C8 headless E2E · 无仓项目零标配票".into(),
        workspace: None,
        github: None,
    })
    .await
    .expect("create project C");

    println!("\n⑦ 项目 C · CompleteCreation(run_first: true,但无仓——应零标配票) …");
    app.dispatch(Command::CompleteCreation {
        cadence: bw_core::model::Cadence::Weekly,
        run_first: true,
    })
    .await
    .expect("complete creation C");
    dump_trio(&store, proj_c, "项目 C(无仓)落地后").await;

    // ══════════════════════ 汇总断言(program-side sanity;
    //   独立复核以 sqlite3 CLI 另行读回为准,见运行脚本)══════════════════════
    let issues_a_final = store.list_issues(proj_a, None, None).await.unwrap();
    assert_eq!(issues_a_final.len(), 3, "项目 A 应有恰好 3 张标配 Issue");
    assert!(
        issues_a_final.iter().all(|i| i.github_number != 0),
        "项目 A 三张标配 Issue 都应真开了 GitHub issue(github_number != 0)"
    );
    let expect_slugs = [
        "competitive-analysis",
        "north-star-discovery",
        "metrics-binding",
    ];
    let mut got_slugs: Vec<&str> = issues_a_final
        .iter()
        .map(|i| i.standard_skill.as_str())
        .collect();
    got_slugs.sort();
    let mut want_slugs = expect_slugs.to_vec();
    want_slugs.sort();
    assert_eq!(got_slugs, want_slugs, "标配 Skill 关联 slug 不匹配");

    let issues_b_final = store.list_issues(proj_b, None, None).await.unwrap();
    assert_eq!(issues_b_final.len(), 3, "项目 B 应有恰好 3 张标配 Issue");

    let issues_c_final = store.list_issues(proj_c, None, None).await.unwrap();
    assert_eq!(issues_c_final.len(), 0, "无仓项目 C 应零标配票");

    let runs_a_issue1 = store
        .list_runs_for_issue(issues_a_final.iter().find(|i| i.number == 1).unwrap().id)
        .await
        .unwrap();
    assert!(
        !runs_a_issue1.is_empty(),
        "run_first=true 应对①竞品分析真实 dispatch 了一次 RunIssue"
    );

    let runs_b_issue1 = store
        .list_runs_for_issue(issues_b_final.iter().find(|i| i.number == 1).unwrap().id)
        .await
        .unwrap();
    assert!(
        runs_b_issue1.is_empty(),
        "run_first=false 不应有任何 run —— 零摩擦的另一半"
    );

    let ns_skill_uses_after = store
        .list_skills()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "north-star-discovery")
        .map(|s| s.uses)
        .unwrap();
    assert_eq!(
        ns_skill_uses_after,
        ns_skill_uses_before + 1,
        "north-star-discovery 的 uses 应恰好 +1(一次 run 记一次账,不重复)"
    );

    let claude_log_text = std::fs::read_to_string(&claude_log).unwrap_or_default();
    assert!(
        claude_log_text.contains("north-star-discovery"),
        "stub claude 落盘的真实 prompt 里应包含标配 Skill 的名字/正文标记"
    );

    // C10 · 接线验证:项目 A 的 run_first=true 在①②步已经对①竞品分析
    // (standard_skill = "competitive-analysis")真实 dispatch 过一次
    // RunIssue——不需要本例再显式重跑,直接核验那次留下的证据:①stub
    // claude 落盘的真实 prompt 包含 competitive-analysis SKILL.md 的一个
    // 独有标记句(硬约束原文),证明全文注入到达了执行器;②该卡的 uses
    // 恰好 +1,记账 settle-once。
    assert!(
        claude_log_text.contains("绝不由幻觉填充对标事实"),
        "stub claude 落盘的真实 prompt 里应包含 competitive-analysis SKILL.md 的硬约束原文\
         (证明是 C10 落地的真实文件内容,不是占位/裁剪)"
    );
    let ca_skill_uses_after = store
        .list_skills()
        .await
        .unwrap()
        .into_iter()
        .find(|s| s.name == "competitive-analysis")
        .map(|s| s.uses)
        .unwrap();
    assert_eq!(
        ca_skill_uses_after,
        ca_skill_uses_before + 1,
        "competitive-analysis 的 uses 应恰好 +1(项目 A run_first=true 触发的那一次 RunIssue,记一次账,不重复)"
    );
    println!(
        "\n[C10] competitive-analysis: prompt 含硬约束原文 = true · uses {} → {}",
        ca_skill_uses_before, ca_skill_uses_after
    );

    println!("\n✓ verify_c8_standard_trio done");
    println!("  DB: {db_path}");
    println!("  project A (github, run_first=true)  = {}", proj_a.uuid());
    println!("  project B (github, run_first=false) = {}", proj_b.uuid());
    println!("  project C (无仓)                     = {}", proj_c.uuid());
    println!("  stub claude prompt log: {}", claude_log.display());
}
