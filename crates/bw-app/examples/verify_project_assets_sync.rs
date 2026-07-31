//! plan/渠道6 E2E (headless): trigger `SyncConnector` on a project's
//! git-repo connector and read back whether the project's own `skills/` +
//! `agents/` were scanned in as 种A (project-owned, source=project-assets).
//! Replaces a GUI「立即同步」click — same command-layer path, no mock
//! assertions, the real scan + reconcile + store.
//!
//! Run: `cargo run -p bw-app --example verify_project_assets_sync -- <db-path>`
//! Every number this prints is also independently readable via `sqlite3 <db>`.

use bw_app::{App, Command, Event};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() {
    let db = std::env::args()
        .nth(1)
        .expect("usage: verify_project_assets_sync <db-path>");
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    let mut sub = app.subscribe();

    // Pick the first project-bound git-repo connector (the one whose sync
    // runs `evidence::collect` + `sync_project_assets`).
    let connectors = store.list_connectors().await.unwrap();
    let git = connectors
        .iter()
        .find(|c| c.kind == "git-repo" && c.project_id.is_some())
        .expect("no project-bound git-repo connector in this DB");
    let pid = git.project_id.unwrap();
    println!(
        "================ 渠道6 sync E2E ================"
    );
    println!("git-repo connector: {} (project={})", git.id.uuid(), pid.uuid());

    let before_skills = store.list_skills().await.unwrap();
    let before_owned = before_skills
        .iter()
        .filter(|s| s.project_id == Some(pid))
        .count();
    println!("before sync: project-owned skills = {before_owned}");

    // SyncConnector runs probe_connector → git-repo arm → evidence + sync_project_assets.
    // sync_project_assets is synchronous inside dispatch (refresh+emit all await),
    // so by the time this returns the scan is done.
    match app.dispatch(Command::SyncConnector { id: git.id }).await {
        Ok(_) => {}
        Err(e) => println!("SyncConnector returned error (probe may have failed; sync is best-effort): {e}"),
    }

    // Drain any pending events so the channel doesn't back up.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        match sub.try_recv() {
            Ok(Event::SkillsChanged) | Ok(Event::AgentsChanged) | Ok(_) => continue,
            Err(_) => break,
        }
    }

    let skills = store.list_skills().await.unwrap();
    let agents = store.list_agents().await.unwrap();
    let p_skills: Vec<_> = skills.iter().filter(|s| s.project_id == Some(pid)).collect();
    let p_agents: Vec<_> = agents.iter().filter(|a| a.project_id == Some(pid)).collect();
    println!("after sync: project-owned skills = {}", p_skills.len());
    for s in &p_skills {
        println!("  skill: {} (source={:?})", s.name, s.source);
    }
    println!("after sync: project-owned agents = {}", p_agents.len());
    for a in &p_agents {
        println!("  agent: {} (source={:?})", a.name, a.source);
    }
    println!("================================================");

    if p_skills.is_empty() && p_agents.is_empty() {
        println!("WARN: no project-assets scanned — workspace may have no skills/ or agents/");
        std::process::exit(1);
    }
}
