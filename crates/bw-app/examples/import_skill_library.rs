//! T3 (plan/12 §1/§2) headless E2E: drive the real `App`/`Command` layer to
//! batch-import both real skill libraries — mattpocock-skills and
//! superpowers — via `Command::ImportSkillLibrary`, the same command-layer
//! path the desktop UI will eventually drive, no mocked assertions. Then:
//!
//! 1. read the result back from the store (grouped counts, a multi-file
//!    skill's `skill_file` rows),
//! 2. re-run the exact same two imports a second time to prove the
//!    idempotent-by-`(name, official_library)` semantics this ticket chose
//!    (imported=0, skipped=all on the second pass; total row count
//!    unchanged).
//!
//! Every number this example prints should also be independently readable
//! straight from the DB file it leaves behind via `sqlite3` — this repo's
//! core discipline (报告不代答,读回为证).
//!
//! Run: `cargo run -p bw-app --example import_skill_library -- <output-db-path>`

use bw_app::{App, Command, Event};
use bw_core::model::HubSource;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

/// Real, on-disk library roots (plan/12 §1's table + this ticket's own
/// instruction to trust `find`, not `plugin.json`, for the true count).
const MATTPOCOCK_ROOT: &str =
    "/Users/gravity/.claude/plugins/cache/mattpocock/mattpocock-skills/1.2.0/skills";
const SUPERPOWERS_ROOT: &str =
    "/Users/gravity/.claude/plugins/cache/superpowers-dev/superpowers/6.1.1/skills";

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("bw_import_skill_library.db")
            .to_string_lossy()
            .into_owned()
    });

    let _ = std::fs::remove_file(&db_path);

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    let mut sub = app.subscribe();

    println!("================ ImportSkillLibrary batch E2E (T3) ================");
    println!("db: {db_path}");

    // Ground truth via a real find, independent of any manifest's claimed
    // count (mattpocock's plugin.json says 22 — the real folder count is
    // higher; see this run's own printed number and the final report).
    let mattpocock_real = count_skill_md(MATTPOCOCK_ROOT);
    let superpowers_real = count_skill_md(SUPERPOWERS_ROOT);
    println!(
        "real SKILL.md dirs on disk: mattpocock-skills={mattpocock_real} superpowers={superpowers_real} total={}",
        mattpocock_real + superpowers_real
    );

    let skills_before = store.list_skills().await.unwrap().len();

    // ---------------- pass 1: fresh import of both libraries ----------------
    app.dispatch(Command::ImportSkillLibrary {
        root_path: MATTPOCOCK_ROOT.to_string(),
        official_library: "mattpocock-skills".to_string(),
        project_id: None,
    })
    .await
    .expect("ImportSkillLibrary(mattpocock-skills) pass 1 should succeed");
    let mp_pass1 = drain_import_event(&mut sub);

    app.dispatch(Command::ImportSkillLibrary {
        root_path: SUPERPOWERS_ROOT.to_string(),
        official_library: "superpowers".to_string(),
        project_id: None,
    })
    .await
    .expect("ImportSkillLibrary(superpowers) pass 1 should succeed");
    let sp_pass1 = drain_import_event(&mut sub);

    let skills_after_pass1 = store.list_skills().await.unwrap();
    println!("----------------------------------------------------------");
    println!(
        "skills: before={skills_before} after pass 1={}",
        skills_after_pass1.len()
    );
    println!(
        "pass 1 mattpocock-skills: imported={} skipped={}",
        mp_pass1.0, mp_pass1.1
    );
    println!(
        "pass 1 superpowers: imported={} skipped={}",
        sp_pass1.0, sp_pass1.1
    );

    let mp_rows: Vec<_> = skills_after_pass1
        .iter()
        .filter(|s| {
            matches!(&s.source, HubSource::Official { official_library } if official_library == "mattpocock-skills")
        })
        .collect();
    let sp_rows: Vec<_> = skills_after_pass1
        .iter()
        .filter(|s| {
            matches!(&s.source, HubSource::Official { official_library } if official_library == "superpowers")
        })
        .collect();
    println!(
        "official_library grouped counts: mattpocock-skills={} superpowers={}",
        mp_rows.len(),
        sp_rows.len()
    );

    // Spot check a real multi-file skill: mattpocock's "tdd" carries 3 real
    // support files alongside SKILL.md (mocking.md, tests.md,
    // agents/openai.yaml).
    let tdd = skills_after_pass1
        .iter()
        .find(|s| s.name == "tdd")
        .expect("imported skill named \"tdd\" must be findable by list_skills()");
    let tdd_files = store.list_skill_files(tdd.id).await.unwrap();
    let mut tdd_rel: Vec<&str> = tdd_files.iter().map(|f| f.rel_path.as_str()).collect();
    tdd_rel.sort_unstable();
    println!("tdd skill_file rows: {} -> {:?}", tdd_files.len(), tdd_rel);

    // ---------------- pass 2: re-run both — idempotency proof ----------------
    app.dispatch(Command::ImportSkillLibrary {
        root_path: MATTPOCOCK_ROOT.to_string(),
        official_library: "mattpocock-skills".to_string(),
        project_id: None,
    })
    .await
    .expect("ImportSkillLibrary(mattpocock-skills) pass 2 should succeed");
    let mp_pass2 = drain_import_event(&mut sub);

    app.dispatch(Command::ImportSkillLibrary {
        root_path: SUPERPOWERS_ROOT.to_string(),
        official_library: "superpowers".to_string(),
        project_id: None,
    })
    .await
    .expect("ImportSkillLibrary(superpowers) pass 2 should succeed");
    let sp_pass2 = drain_import_event(&mut sub);

    let skills_after_pass2 = store.list_skills().await.unwrap();
    println!("----------------------------------------------------------");
    println!(
        "pass 2 mattpocock-skills: imported={} skipped={}",
        mp_pass2.0, mp_pass2.1
    );
    println!(
        "pass 2 superpowers: imported={} skipped={}",
        sp_pass2.0, sp_pass2.1
    );
    println!(
        "skills after pass 2: {} (must equal pass 1's {})",
        skills_after_pass2.len(),
        skills_after_pass1.len()
    );

    let count_ok =
        mp_rows.len() as u64 == mattpocock_real && sp_rows.len() as u64 == superpowers_real;
    let pass1_ok = mp_pass1.0 == mattpocock_real as u32
        && mp_pass1.1 == 0
        && sp_pass1.0 == superpowers_real as u32
        && sp_pass1.1 == 0;
    let pass2_ok = mp_pass2.0 == 0
        && mp_pass2.1 == mattpocock_real as u32
        && sp_pass2.0 == 0
        && sp_pass2.1 == superpowers_real as u32;
    let stable_ok = skills_after_pass2.len() == skills_after_pass1.len();
    let tdd_ok =
        tdd_files.len() == 3 && tdd_rel == vec!["agents/openai.yaml", "mocking.md", "tests.md"];

    println!("----------------------------------------------------------");
    println!("grouped counts match real find count: {count_ok}");
    println!("pass 1 imported=all, skipped=0: {pass1_ok}");
    println!("pass 2 imported=0, skipped=all (idempotent re-run): {pass2_ok}");
    println!("total skill count stable across pass 1/2: {stable_ok}");
    println!("tdd skill_file rows == 3, matches source dir: {tdd_ok}");
    println!("==========================================================");

    if !(count_ok && pass1_ok && pass2_ok && stable_ok && tdd_ok) {
        std::process::exit(1);
    }
}

/// Independent ground truth, deliberately re-implemented rather than calling
/// into `bw_app`'s own walk — this is the E2E's outside check on that walk,
/// not a reuse of it.
fn count_skill_md(root: &str) -> u64 {
    fn walk(dir: &std::path::Path, count: &mut u64) {
        if dir.join("SKILL.md").is_file() {
            *count += 1;
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, count);
            }
        }
    }
    let mut count = 0u64;
    walk(std::path::Path::new(root), &mut count);
    count
}

fn drain_import_event(sub: &mut tokio::sync::broadcast::Receiver<Event>) -> (u32, u32) {
    loop {
        match sub.try_recv() {
            Ok(Event::SkillLibraryImported {
                imported, skipped, ..
            }) => return (imported, skipped),
            Ok(_) => continue,
            Err(e) => panic!("expected Event::SkillLibraryImported, got error: {e}"),
        }
    }
}
