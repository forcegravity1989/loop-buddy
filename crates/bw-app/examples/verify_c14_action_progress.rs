//! **verify_c14_action_progress — C14 全流程状态回显 headless E2E 指挥器
//! (plan/14 规范条 2)。**
//!
//! 不开 UI,把创建流的慢动作(建仓/克隆/仓列表加载/标配建单/落地推送)走一
//! 遍,全部经真实命令层(`dispatch`),订阅 `App::subscribe()` 把
//! `Event::ActionProgress` 按到达顺序记进 `(name, kind, at)`,结论一律
//! **事件顺序断言**(Started 必须先于同名的 Ok/Fail 到达)为证——不是截图,
//! 不是猜测:
//!
//!   ① 建仓(github=New,正常 slug)→ 「{proj} · 建仓」pending → ok,且两者
//!      间真实经过了 stub 注入的人为延迟(证明 pending 不是和 ok 同一瞬间
//!      合成的空壳);
//!   ② 建仓(github=New,人为制造失败的 slug)→ 「{proj} · 建仓」pending →
//!      fail,本地兜底仓仍然落地(`CreateProject` 从不因网络失败整体报错,
//!      同一份既有纪律);
//!   ③ 克隆已有仓(github=Existing)→ 「{proj} · 克隆仓库」pending → ok;
//!   ④ `ListGithubRepos` → 「GitHub 仓库列表」pending → ok;
//!   ⑤ `CompleteCreation` 落地 → 标配三件套「{title} · 建单」×3 各自
//!      pending → ok,以及「{proj} · 落地推送」pending → ok。
//!
//! **gh 全程被自带的【mock】stub 顶替**——写进 `<ws_root>/.stub-bin/gh` 并
//! 前置进 PATH,输出/日志自我标注【mock】,绝不冒充真实 GitHub。`git`
//! 是本机真实二进制,但只操作本地 bare 仓(`.stub-remotes/`),从不联网。
//!
//! 用法:
//!   cargo run -p bw-app --example verify_c14_action_progress -- <db-path> <workspaces-root>

use bw_app::{ActionState, App, Command, Event, GithubOrigin};
use bw_core::model::Cadence;
use bw_core::ProjectId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A self-labeled 【mock】 `gh` — NOT real GitHub. Every subcommand this
/// scenario reaches sleeps `STUB_GH_DELAY_S` seconds before answering, so
/// the Started→Ok/Fail gap this example measures is a real elapsed
/// duration, not two emits back-to-back in the same tick.
/// `STUB_GH_FAIL_SLUG`, if set, makes `repo create <that slug>` fail (the
/// pending→fail scenario).
const STUB_GH: &str = r#"#!/bin/sh
# 【mock】stub gh for C14 action-progress E2E — self-labeled, NOT real GitHub.
delay="${STUB_GH_DELAY_S:-0.9}"
if [ "$1" = "api" ] && [ "$2" = "user" ]; then
  echo "testowner"
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "create" ]; then
  slug="$3"
  sleep "$delay"
  if [ -n "$STUB_GH_FAIL_SLUG" ] && [ "$slug" = "$STUB_GH_FAIL_SLUG" ]; then
    echo "【mock】stub gh: 人为制造的建仓失败(C14 fail-path 用例)" >&2
    exit 1
  fi
  bare="$STUB_GH_REMOTES/$slug.git"
  mkdir -p "$STUB_GH_REMOTES"
  git init --bare -q "$bare"
  git clone -q "$bare" "$slug"
  echo "https://github.com/testowner/$slug"
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "clone" ]; then
  owner_repo="$3"
  dest="$4"
  repo="${owner_repo#*/}"
  sleep "$delay"
  bare="$STUB_GH_REMOTES/$repo.git"
  git clone -q "$bare" "$dest"
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "view" ]; then
  echo '{"isPrivate":true}'
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "list" ]; then
  sleep "$delay"
  echo '[{"nameWithOwner":"testowner/demo-repo","isPrivate":false,"updatedAt":"2026-07-01T00:00:00Z"}]'
  exit 0
fi
if [ "$1" = "issue" ] && [ "$2" = "create" ]; then
  sleep "$delay"
  n=$(cat "$STUB_GH_COUNTER_ISSUE" 2>/dev/null || echo 0)
  n=$((n + 1))
  echo "$n" > "$STUB_GH_COUNTER_ISSUE"
  echo "https://github.com/testowner/stub/issues/$n"
  exit 0
fi
# any other gh call: benign no-op
exit 0
"#;

#[derive(Clone, Debug, PartialEq)]
enum Kind {
    Started,
    Ok(String),
    Fail(String),
}

#[derive(Clone, Debug)]
struct Recorded {
    name: String,
    kind: Kind,
    at: Instant,
}

/// Assert `name`'s first `Started` really precedes its first `Ok`/`Fail` —
/// the pending→ok / pending→fail sequence the ticket asks for — and (for
/// the ok/fail case) that real wall-clock time actually passed between them
/// (`min_gap`), proving the pair isn't a synthetic back-to-back emit.
fn assert_sequence(log: &[Recorded], name: &str, expect_ok: bool, min_gap: Duration) {
    let started = log
        .iter()
        .find(|r| r.name == name && r.kind == Kind::Started)
        .unwrap_or_else(|| panic!("no Started event for {name:?} — event log: {log:#?}"));
    let resolved = log
        .iter()
        .find(|r| {
            r.name == name
                && match &r.kind {
                    Kind::Ok(_) => expect_ok,
                    Kind::Fail(_) => !expect_ok,
                    Kind::Started => false,
                }
        })
        .unwrap_or_else(|| {
            panic!(
                "no {} event for {name:?} — event log: {log:#?}",
                if expect_ok { "Ok" } else { "Fail" }
            )
        });
    assert!(
        resolved.at >= started.at,
        "{name:?}: resolved event arrived before Started — impossible ordering"
    );
    let gap = resolved.at.duration_since(started.at);
    assert!(
        gap >= min_gap,
        "{name:?}: Started→resolved gap {gap:?} < {min_gap:?} — pending looks synthetic, not a real elapsed wait"
    );
    let detail = match &resolved.kind {
        Kind::Ok(d) | Kind::Fail(d) => d.clone(),
        Kind::Started => unreachable!(),
    };
    println!(
        "  ✓ {name}: Started → {} ({gap:?}) · {detail}",
        if expect_ok { "Ok" } else { "Fail" }
    );
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let db_path = args
        .get(1)
        .cloned()
        .expect("usage: verify_c14_action_progress <db-path> <workspaces-root>");
    let ws_root = PathBuf::from(
        args.get(2)
            .cloned()
            .expect("usage: verify_c14_action_progress <db-path> <workspaces-root>"),
    );
    std::fs::create_dir_all(&ws_root).unwrap();

    let stub_bin = ws_root.join(".stub-bin");
    std::fs::create_dir_all(&stub_bin).unwrap();
    let gh = stub_bin.join("gh");
    std::fs::write(&gh, STUB_GH).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", stub_bin.display(), old_path));
    let remotes = ws_root.join(".stub-remotes");
    std::env::set_var("STUB_GH_REMOTES", remotes.display().to_string());
    std::env::set_var(
        "STUB_GH_COUNTER_ISSUE",
        ws_root.join(".stub-gh-issue-n").display().to_string(),
    );
    const FAIL_SLUG: &str = "c14-fail-demo";
    std::env::set_var("STUB_GH_FAIL_SLUG", FAIL_SLUG);
    println!(
        "[stub] gh → {} (【mock】, NOT real GitHub; every call sleeps to make Started→resolved a real gap)",
        gh.display()
    );

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    )
    .with_workspaces_root(ws_root.clone());
    app.dispatch(Command::Boot).await.expect("boot");

    // Collector: every `Event::ActionProgress` this run emits, in arrival
    // order, with a real `Instant` — the sole evidence the assertions below
    // read back from.
    let log: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
    let mut rx = app.subscribe();
    let log_w = log.clone();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let Event::ActionProgress { name, state } = ev {
                let kind = match state {
                    ActionState::Started => Kind::Started,
                    ActionState::Ok(d) => Kind::Ok(d),
                    ActionState::Fail(d) => Kind::Fail(d),
                };
                log_w.lock().unwrap().push(Recorded {
                    name,
                    kind,
                    at: Instant::now(),
                });
            }
        }
    });

    // ── ① 建仓(正常)──────────────────────────────────────────────
    println!("\n① CreateProject(github=New, 正常 slug)…");
    let proj_ok = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_ok,
        name: "C14 建仓正常".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C14 headless E2E · 建仓 pending→ok".into(),
        workspace: None,
        github: Some(GithubOrigin::New {
            slug: "c14-progress-demo".into(),
            private: true,
        }),
    })
    .await
    .expect("create project (github new, ok path)");

    // ── ② 建仓(人为失败)──────────────────────────────────────────
    println!("② CreateProject(github=New, 人为失败 slug={FAIL_SLUG:?})…");
    let proj_fail = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_fail,
        name: "C14 建仓失败".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C14 headless E2E · 建仓 pending→fail".into(),
        workspace: None,
        github: Some(GithubOrigin::New {
            slug: FAIL_SLUG.into(),
            private: true,
        }),
    })
    .await
    .expect("create project (github new, fail path) — CreateProject itself must not error");

    // ── ③ 克隆已有仓 ─────────────────────────────────────────────
    println!("③ CreateProject(github=Existing)…");
    std::fs::create_dir_all(&remotes).unwrap();
    let bare_existing = remotes.join("demo-existing.git");
    let init = std::process::Command::new("git")
        .args(["init", "--bare", "-q", &bare_existing.display().to_string()])
        .status()
        .expect("git init --bare (fixture for clone scenario)");
    assert!(init.success(), "git init --bare failed");
    let proj_clone = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: proj_clone,
        name: "C14 克隆".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C14 headless E2E · 克隆 pending→ok".into(),
        workspace: None,
        github: Some(GithubOrigin::Existing {
            owner: "testowner".into(),
            repo: "demo-existing".into(),
        }),
    })
    .await
    .expect("create project (github existing / clone)");

    // ── ④ 仓列表加载 ─────────────────────────────────────────────
    println!("④ ListGithubRepos…");
    app.dispatch(Command::ListGithubRepos)
        .await
        .expect("list github repos");
    assert_eq!(
        app.snapshot().github_repos.len(),
        1,
        "stub gh repo list 应回一条,真实落进 AppState.github_repos"
    );

    // ── ⑤ 落地(标配三件套 + 落地推送)────────────────────────────
    println!("⑤ CompleteCreation(建仓成功的项目 · run_first=false)…");
    app.dispatch(Command::OpenProject(proj_ok))
        .await
        .expect("open project (proj_ok) before completing its creation");
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
        run_first: false,
    })
    .await
    .expect("complete creation (proj_ok)");

    // Let the collector task fully drain (broadcast + a separate tokio task
    // — give it a beat past the last emit before reading the log back).
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ══════════════════════ 事件顺序断言(读回为证)══════════════════════
    let log = log.lock().unwrap().clone();
    println!(
        "\n[readback] {} 条 ActionProgress 事件,按到达顺序:",
        log.len()
    );
    for r in &log {
        println!("  {:?} · {}", r.kind, r.name);
    }

    let min_gap = Duration::from_millis(500); // stub sleeps ~900ms; leave slack
    println!("\n断言:");
    assert_sequence(&log, "C14 建仓正常 · 建仓", true, min_gap);
    assert_sequence(&log, "C14 建仓失败 · 建仓", false, min_gap);
    assert_sequence(&log, "C14 克隆 · 克隆仓库", true, min_gap);
    assert_sequence(&log, "GitHub 仓库列表", true, min_gap);
    assert_sequence(&log, "竞品分析 · 建单", true, min_gap);
    assert_sequence(&log, "找指标 · 建单", true, min_gap);
    assert_sequence(&log, "绑数据 · 建单", true, min_gap);
    // push_head 是本地 `git push`,没有注入延迟——只断言顺序,不断言 gap。
    assert_sequence(&log, "C14 建仓正常 · 落地推送", true, Duration::ZERO);

    // 建仓失败路径:CreateProject 从不因网络失败整体报错——本地兜底仓仍然
    // 落地(既有纪律,C14 不改),读回验证。
    let proj_fail_row = store.get_project(proj_fail).await.unwrap().unwrap();
    assert!(
        !proj_fail_row.workspace_path.trim().is_empty(),
        "建仓失败后应软降级到本地兜底仓,workspace_path 不应为空"
    );
    assert!(
        proj_fail_row.github_remote.trim().is_empty(),
        "建仓失败的项目不应有 github_remote(本地兜底≠已挂 GitHub 仓)"
    );
    println!(
        "  ✓ 建仓失败项目本地兜底落地:workspace_path={:?} github_remote={:?}",
        proj_fail_row.workspace_path, proj_fail_row.github_remote
    );

    println!("\n✓ verify_c14_action_progress done — every Started really preceded its Ok/Fail, with a real elapsed gap.");
    println!("  DB: {db_path}");
}
