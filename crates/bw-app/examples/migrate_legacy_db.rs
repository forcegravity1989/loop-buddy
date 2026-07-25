//! T14 (2026-07-24, plan/12 §10 v1.1) headless E2E: drive the real
//! `App`/`Command` layer through `Command::Boot` then
//! `Command::MigrateLegacyShellsIfNeeded` against a real SQLite file — the
//! same command-layer path `app-desktop`'s `kernel.rs` drives at real boot,
//! no mocked assertions.
//!
//! T16.5 (2026-07-24, GH#54) folded a second pass into this same command:
//! the five built-in stage-template `workflow_spec` rows' `phases`/
//! `phase_prompts` get refreshed from the current playbook if (and only if)
//! they're still the pure pre-T8 legacy shape — see
//! `bw_app::legacy_migration::is_pure_legacy_phases`'s doc comment. This
//! example's printed `refreshed_templates` list and the
//! `app_meta[template_phase_refresh_v1]` readback are that pass's own
//! independent proof, alongside the pre-existing shell-migration tally.
//!
//! T14.5 (2026-07-24, GH#59) folds in a THIRD pass: directory-import
//! (ECC/Adopted) `workflow_spec` catalog shells with zero real trace (no
//! `workflow_run`, `uses=0`, unreferenced by any `run_workflow`-mode
//! `cron_task`) get deleted outright — see
//! `bw_app::legacy_migration::is_directory_import_source`'s doc comment.
//! This example's printed `purged_workflows` list, the `workflow_spec`
//! before/after row count, and the `app_meta[workflow_shell_purge_v1]`
//! readback are that pass's own independent proof.
//!
//! **This is meant to run against a throwaway COPY of a real daily DB**, per
//! this ticket's own acceptance criterion: the caller is responsible for
//! `cp`-ing `~/Library/Application Support/BuildersWorkbench/workbench.db`
//! (macOS) to a scratch path first — this example never touches the
//! original path itself, it only opens whatever path it's given.
//!
//! Run once (should migrate):
//!   `cargo run -p bw-app --example migrate_legacy_db -- <db-path>`
//! Run again on the SAME path (should be a real zero-op):
//!   `cargo run -p bw-app --example migrate_legacy_db -- <db-path>`
//!
//! Follow up with `sqlite3 <db-path>` reads for independent proof — this
//! repo's core discipline (报告不代答,读回为证): every number this example
//! prints should also be readable straight from the DB file it leaves
//! behind (plus the sibling `.bak-<timestamp>` file it wrote).

use bw_app::{App, Command, Event};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: migrate_legacy_db <db-path> (a COPY of a real DB, never the original)");
        std::process::exit(2);
    });

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );

    let mut events = app.subscribe();
    let report_task = tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(Event::LegacyShellsMigrated { report }) => return Some(report),
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    });

    println!("================ T14 MigrateLegacyShellsIfNeeded E2E ================");
    println!("db: {db_path}");

    let skills_before = store.list_skills().await.unwrap();
    let agents_before = store.list_agents().await.unwrap();
    let workflows_before = store.list_workflow_specs().await.unwrap();
    println!(
        "before Boot: skill={} agent={} workflow_spec={}",
        skills_before.len(),
        agents_before.len(),
        workflows_before.len()
    );

    app.dispatch(Command::Boot).await.unwrap();

    app.dispatch(Command::MigrateLegacyShellsIfNeeded {
        db_path: db_path.clone(),
    })
    .await
    .expect("MigrateLegacyShellsIfNeeded should not error");

    // Give the event task a moment to drain (dispatch already awaited to
    // completion, so the emit already happened synchronously before this
    // point — this is just collecting it off the broadcast channel).
    drop(app); // drop the sender-holding App so the receiver task can end honestly if nothing arrived
    let report = tokio::time::timeout(std::time::Duration::from_millis(200), report_task)
        .await
        .ok()
        .and_then(|r| r.ok())
        .flatten();

    let skills_after = store.list_skills().await.unwrap();
    let agents_after = store.list_agents().await.unwrap();
    let workflows_after = store.list_workflow_specs().await.unwrap();
    let skill_files_after = {
        let mut n = 0usize;
        for s in &skills_after {
            n += store.list_skill_files(s.id).await.unwrap().len();
        }
        n
    };
    let done_flag = store
        .get_app_meta("legacy_shells_migration_v1")
        .await
        .unwrap();
    let phase_refresh_flag = store
        .get_app_meta("template_phase_refresh_v1")
        .await
        .unwrap();
    let shell_purge_flag = store.get_app_meta("workflow_shell_purge_v1").await.unwrap();

    println!("----------------------------------------------------------");
    match &report {
        Some(r) => {
            println!("migration RAN this dispatch — real tally:");
            println!("  backup_path: {:?}", r.backup_path);
            println!(
                "  deleted_skills={} kept_skills_with_trace={}",
                r.deleted_skills, r.kept_skills_with_trace
            );
            println!(
                "  deleted_agents={} kept_agents_with_trace={}",
                r.deleted_agents, r.kept_agents_with_trace
            );
            println!(
                "  imported_skills={} imported_agents={}",
                r.imported_skills, r.imported_agents
            );
            if !r.skipped_sources.is_empty() {
                println!("  skipped_sources:");
                for s in &r.skipped_sources {
                    println!("    - {s}");
                }
            }
            println!(
                "  refreshed_templates ({}): {:?}",
                r.refreshed_templates.len(),
                r.refreshed_templates
            );
            println!(
                "  purged_workflows ({}): {:?}",
                r.purged_workflows.len(),
                r.purged_workflows
            );
        }
        None => println!("migration did NOT run this dispatch (already-done / no-op path)"),
    }
    println!("----------------------------------------------------------");
    println!(
        "skill: before={} after={}",
        skills_before.len(),
        skills_after.len()
    );
    println!(
        "agent: before={} after={}",
        agents_before.len(),
        agents_after.len()
    );
    println!(
        "workflow_spec: before={} after={}",
        workflows_before.len(),
        workflows_after.len()
    );
    println!("skill_file rows after: {skill_files_after}");
    println!("app_meta[legacy_shells_migration_v1] = {done_flag:?}");
    println!("app_meta[template_phase_refresh_v1] = {phase_refresh_flag:?}");
    println!("app_meta[workflow_shell_purge_v1] = {shell_purge_flag:?}");
    if let Some(r) = &report {
        if let Some(bak) = &r.backup_path {
            println!(
                "backup file exists on disk: {}",
                std::path::Path::new(bak).is_file()
            );
        }
    }
    println!("==========================================================");
}
