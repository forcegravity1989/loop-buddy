//! T2 (plan/12 §2) headless E2E: drive the real `App`/`Command` layer to
//! import one real, on-disk mattpocock-skills folder (`engineering/tdd`,
//! 4 real files: SKILL.md + mocking.md + tests.md + agents/openai.yaml) and
//! read the result back from the store — the same command-layer path the
//! desktop UI will eventually drive, no mocked assertions.
//!
//! Run: `cargo run -p bw-app --example import_skill_package -- <output-db-path> [source-dir]`
//!
//! Follow up with `sqlite3 <output-db-path>` reads for independent proof —
//! this repo's core discipline (报告不代答,读回为证): every number this
//! example prints should also be readable straight from the DB file it
//! leaves behind.

use bw_app::{App, Command};
use bw_core::model::HubSource;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("bw_import_skill_package.db")
            .to_string_lossy()
            .into_owned()
    });
    let source_dir = args.next().unwrap_or_else(|| {
        "/Users/gravity/.claude/plugins/cache/mattpocock/mattpocock-skills/1.2.0/skills/engineering/tdd"
            .to_string()
    });

    let _ = std::fs::remove_file(&db_path);

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    let skills_before = store.list_skills().await.unwrap().len();

    app.dispatch(Command::ImportSkillPackage {
        source_path: source_dir.clone(),
        project_id: None,
        official_library: Some("mattpocock-skills".to_string()),
    })
    .await
    .expect("ImportSkillPackage should succeed against a real SKILL.md folder");

    let skills_after = store.list_skills().await.unwrap();
    let imported = skills_after
        .iter()
        .find(|s| s.name == "tdd")
        .expect("imported skill named \"tdd\" must be findable by list_skills()");

    let files = store.list_skill_files(imported.id).await.unwrap();
    let mut rel_paths: Vec<&str> = files.iter().map(|f| f.rel_path.as_str()).collect();
    rel_paths.sort_unstable();

    println!("================ ImportSkillPackage E2E ================");
    println!("db: {db_path}");
    println!("source_dir: {source_dir}");
    println!(
        "skills: before={skills_before} after={}",
        skills_after.len()
    );
    println!(
        "imported skill: id={} name={:?} maturity={:?} source={:?}",
        imported.id.uuid(),
        imported.name,
        imported.maturity,
        imported.source
    );
    println!(
        "content (SKILL.md body) len={} bytes",
        imported.content.len()
    );
    println!(
        "content starts with: {:?}",
        &imported.content.chars().take(40).collect::<String>()
    );
    println!("skill_file rows: {} -> {:?}", files.len(), rel_paths);

    let source_ok = matches!(
        &imported.source,
        HubSource::Official { official_library } if official_library == "mattpocock-skills"
    );
    let content_ok =
        !imported.content.trim().is_empty() && imported.content.contains("Test-Driven Development");
    let files_ok =
        files.len() == 3 && rel_paths == vec!["agents/openai.yaml", "mocking.md", "tests.md"];

    println!("----------------------------------------------------------");
    println!("source=Official{{official_library=mattpocock-skills}}: {source_ok}");
    println!("content = SKILL.md body, non-empty: {content_ok}");
    println!("skill_file rows == 3, rel_path 与源目录一致: {files_ok}");
    println!("==========================================================");

    if !(source_ok && content_ok && files_ok) {
        std::process::exit(1);
    }
}
