//! T5 (plan/12 §3) headless E2E: drive the real `App`/`Command` layer to
//! batch-import all 67 vendored ECC (everything-claude-code) AGENT.md files
//! via `Command::ImportAgentDefinition`, one dispatch per file — the same
//! command-layer path the desktop UI will eventually drive, no mocked
//! assertions — then read the result back from the store.
//!
//! Source of the 67 files: github.com/affaan-m/everything-claude-code
//! (MIT-licensed), `agents/` directory, fetched one file at a time via the
//! GitHub API + raw.githubusercontent.com (never a full clone — the repo is
//! 39.7MB, `agents/` alone is ~500KB) and vendored into this repo at
//! `crates/bw-store/vendor/ecc-agents/` so this example has no network
//! dependency and no dependency on the caller's local disk layout.
//!
//! Run: `cargo run -p bw-app --example import_ecc_agents -- <output-db-path> [vendor-dir]`
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
            .join("bw_import_ecc_agents.db")
            .to_string_lossy()
            .into_owned()
    });
    let vendor_dir = args.next().unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../bw-store/vendor/ecc-agents").to_string()
    });

    let _ = std::fs::remove_file(&db_path);

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    let before = store.list_agents().await.unwrap();
    println!("================ ImportAgentDefinition ECC batch E2E ================");
    println!("db: {db_path}");
    println!("vendor_dir: {vendor_dir}");
    println!(
        "agents before import: {} (expect 5 built-in stage-role agents)",
        before.len()
    );
    // Baseline snapshot of the 5 built-in stage-role agents' real accounting
    // fields — the acceptance criterion this batch must not disturb them.
    let baseline: Vec<(String, u32, String, String)> = before
        .iter()
        .map(|a| {
            (
                a.name.clone(),
                a.runs,
                a.win_rate.clone(),
                a.instructions.clone(),
            )
        })
        .collect();

    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&vendor_dir)
        .unwrap_or_else(|e| panic!("cannot read vendor_dir {vendor_dir}: {e}"))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    files.sort();
    println!("vendor .md files found: {}", files.len());

    let mut imported = 0u32;
    let mut failed: Vec<(String, String)> = Vec::new();
    for f in &files {
        let source_path = f.to_string_lossy().into_owned();
        match app
            .dispatch(Command::ImportAgentDefinition {
                source_path: source_path.clone(),
                official_library: Some("ecc".to_string()),
            })
            .await
        {
            Ok(()) => imported += 1,
            Err(e) => failed.push((source_path, e.to_string())),
        }
    }

    println!("imported ok: {imported} / {}", files.len());
    if !failed.is_empty() {
        println!("FAILED imports:");
        for (path, err) in &failed {
            println!("  {path}: {err}");
        }
    }

    let after = store.list_agents().await.unwrap();
    let ecc_rows: Vec<_> = after
        .iter()
        .filter(|a| {
            matches!(&a.source, HubSource::Official { official_library } if official_library == "ecc")
        })
        .collect();
    let ecc_with_tools = ecc_rows.iter().filter(|a| !a.tools.is_empty()).count();

    println!("----------------------------------------------------------");
    println!("agents after import: {}", after.len());
    println!(
        "ECC rows (source=Official{{official_library=ecc}}): {}",
        ecc_rows.len()
    );
    println!(
        "ECC rows with non-empty tools: {ecc_with_tools} / {}",
        ecc_rows.len()
    );

    // 5 built-in stage-role agents must be byte-identical to the pre-import
    // snapshot — ImportAgentDefinition only ever INSERTs new rows, it must
    // never touch an existing one.
    let mut role_agents_untouched = true;
    for (name, runs, win_rate, instructions) in &baseline {
        match after.iter().find(|a| &a.name == name) {
            Some(a)
                if &a.runs == runs
                    && &a.win_rate == win_rate
                    && &a.instructions == instructions => {}
            Some(a) => {
                role_agents_untouched = false;
                println!(
                    "MISMATCH role agent {name:?}: runs {}->{} win_rate {:?}->{:?}",
                    runs, a.runs, win_rate, a.win_rate
                );
            }
            None => {
                role_agents_untouched = false;
                println!("MISSING role agent {name:?} after import");
            }
        }
    }

    let total_ok = after.len() == 72;
    let count_ok = imported == 67 && files.len() == 67;
    let source_ok = ecc_rows.len() == 67;
    let tools_ok = ecc_with_tools == 67;

    println!("total agents == 72: {total_ok}");
    println!("67 files found + 67 imported ok: {count_ok}");
    println!("67 ECC rows source=Official/ecc: {source_ok}");
    println!("67 ECC rows tools non-empty: {tools_ok}");
    println!("5 built-in role agents untouched: {role_agents_untouched}");
    println!("==========================================================");

    if !(total_ok && count_ok && source_ok && tools_ok && role_agents_untouched) {
        std::process::exit(1);
    }
}
