//! **Version panel test.** Real `git log` against a real repo — this
//! worktree itself — proving the whole vertical slice (bw-engine shell-out →
//! bw-app dispatch → `AppState`) works against actual git output, not just
//! the synthetic-string parser unit tests in `bw-engine`.

use bw_app::{App, Command};
use bw_core::model::Cadence;
use bw_core::ProjectId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

fn tmp_db() -> String {
    std::env::temp_dir()
        .join(format!("bw_version_log_{}.db", uuid::Uuid::new_v4()))
        .to_string_lossy()
        .into_owned()
}

/// This crate's own manifest dir sits inside a real, non-empty git
/// worktree — reuse it as the "real repo" fixture rather than `git init`
/// a throwaway one, which would still need at least one real commit to be
/// a meaningful test of the parsing path.
fn this_repo_root() -> String {
    let manifest_dir = env!("CARGO_MANIFEST_DIR"); // .../crates/bw-app
    std::path::Path::new(manifest_dir)
        .parent() // crates/
        .and_then(|p| p.parent()) // repo root
        .expect("bw-app lives two levels under the repo root")
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
async fn load_version_log_reads_real_commits_from_a_real_git_repo() {
    let path = tmp_db();
    let project = ProjectId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );

    app.dispatch(Command::CreateProject {
        id: project,
        name: "真实仓库测试".into(),
        kind: "y".into(),
        desc: String::new(),
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();

    app.dispatch(Command::SetWorkspace {
        path: this_repo_root(),
        allow_commands: false,
    })
    .await
    .unwrap();

    app.dispatch(Command::LoadVersionLog).await.unwrap();

    let (logged_project, result) = app
        .snapshot()
        .version_log
        .clone()
        .expect("version log was fetched");
    assert_eq!(logged_project, project);
    let commits = result.expect("this worktree is a real git repo with real commits");
    assert!(!commits.is_empty(), "this worktree has real commit history");
    // Every field here is real `git log` output — nothing derived/invented.
    assert!(commits[0].hash.len() >= 7, "a real full commit hash");
    assert!(!commits[0].short_hash.is_empty());
    assert!(!commits[0].author.is_empty());
    assert!(!commits[0].subject.is_empty());

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn load_version_log_is_honest_when_workspace_unconfigured() {
    let path = tmp_db();
    let project = ProjectId::new();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );

    app.dispatch(Command::CreateProject {
        id: project,
        name: "未配置工作目录".into(),
        kind: "y".into(),
        desc: String::new(),
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();

    // Never called SetWorkspace — workspace_path stays empty.
    app.dispatch(Command::LoadVersionLog).await.unwrap();

    let (_, result) = app.snapshot().version_log.clone().unwrap();
    let err = result.expect_err("no workspace configured ⇒ no fabricated commit history");
    assert!(err.contains("未配置"));

    let _ = std::fs::remove_file(&path);
}
