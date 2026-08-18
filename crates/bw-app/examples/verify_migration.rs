//! **Existing-DB migration verifier — opens only, never deletes.**
//!
//! Why this example exists (2026-08-05, Task 2 rework): `verify_goal.rs` was
//! being used to "verify" that a stock daily-driver DB survives a schema
//! migration, but it can't do that job — two independent reasons stacked:
//!
//! 1. `verify_goal.rs:45` reads `std::env::args().nth(1)`, a **positional**
//!    argument, not `BW_DB`. A command written as `BW_DB=<db> cargo run …
//!    --example verify_goal` silently runs against
//!    `$TMPDIR/bw_verify_goal.db` and never touches the target path at all.
//! 2. Even fixed to pass the path positionally, `verify_goal.rs:51` still
//!    does `let _ = std::fs::remove_file(&path);` **before** opening the
//!    store — it deletes whatever was there and always migrates a brand-new
//!    file. It is structurally incapable of exercising an existing-DB
//!    migration path, no matter how it's invoked.
//!
//! Task 2's own migration report fell for exactly this: it claimed "10
//! existing skill rows survived migration on the daily DB", when the real
//! daily DB has 65+ skill rows. 10 is precisely `bw-standard` (8) +
//! `mohit` (2) — the seed count a **fresh** DB gets on `Boot`. The number
//! matched a new DB, not the old one; the "old-DB migration" claim was a
//! false positive.
//!
//! **Hard tell for the same false positive**: if this tool prints
//! `skill total = 10` against a DB you believe is your real daily driver,
//! you are not looking at that DB — you are looking at a fresh one. A real
//! daily DB should read back 65+.
//!
//! This example is deliberately dumb: take a path, open it (which alone
//! runs every migration guard in `SqliteStore::open`), boot the real `App`
//! once, print real readback. No deletion, no fixture-building, no
//! assertions — the caller reads the numbers and independently cross-checks
//! them with `sqlite3` per this repo's "报告不代答,读回为证" discipline.
//!
//! **Always run this against a `cp` of the real DB, never the original
//! file** — `Command::Boot` and the store's own migrations do write to
//! whatever path you give them.
//!
//! Run: `cargo run -p bw-app --example verify_migration -- <db-path>`

use bw_app::{App, Command};
use bw_engine::ClaudeCliConfig;
use bw_store::{SqliteStore, Store};
use sqlx::Row;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!(
            "usage: verify_migration <db-path>  (a COPY of a real DB — this tool opens and \
             migrates in place, never deletes, but never point it at the original file)"
        );
        std::process::exit(2);
    });

    println!("================ verify_migration(只开不删) ================");
    println!("db: {db_path}");

    // Opening alone runs every add_column_if_missing/drop_column_if_present
    // guard in SqliteStore::open — this is the actual migration path the
    // desktop app takes on every real launch, not a simulation of it.
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(store.clone(), ClaudeCliConfig::default());
    app.dispatch(Command::Boot).await.unwrap();

    let skills = store.list_skills().await.unwrap();
    let skill_total = skills.len();
    println!("----------------------------------------------------------");
    println!("skill 总行数: {skill_total}");
    if skill_total == 10 {
        println!(
            "  ⚠ 命中假阳性硬指标(=10 = bw-standard 8 + mohit 2,全新库 Boot 播种量)—— \
             你大概率验的是一个全新库,不是存量老库。"
        );
    }

    let stage_map = store.list_skill_stages().await.unwrap();
    let skill_stage_rows: usize = stage_map.values().map(|v| v.len()).sum();
    let skill_stage_owners = stage_map.len();
    println!(
        "skill_stage 关联表: {skill_stage_rows} 行 · 有归属的技能数(distinct skill_id)= {skill_stage_owners}"
    );

    // stage_origin distribution and stage_ref column presence: `SkillCard`
    // deliberately doesn't carry these (Task 2's own scope call — see
    // task-2-report.md), so read them straight off the table like
    // audit_skills.rs's S6 raw scan does.
    let url = format!("sqlite://{db_path}");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();

    let origin_rows = sqlx::query(
        "SELECT stage_origin, COUNT(*) AS n FROM skill GROUP BY stage_origin ORDER BY n DESC",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    println!("skill.stage_origin 取值分布:");
    for r in &origin_rows {
        let tag: String = r.get("stage_origin");
        let n: i64 = r.get("n");
        let label = if tag.is_empty() {
            "''(未归类)"
        } else {
            tag.as_str()
        };
        println!("  {label}: {n}");
    }

    let table_info = sqlx::query("PRAGMA table_info(skill)")
        .fetch_all(&pool)
        .await
        .unwrap();
    let has_stage_ref = table_info
        .iter()
        .any(|r| r.get::<String, _>("name") == "stage_ref");
    println!(
        "skill.stage_ref 列是否仍在: {has_stage_ref}(SR4 起应为 false —— 旧列已真删,\
         连同它的老索引 idx_skill_stage;读到 true 说明迁移没跑到)"
    );

    println!("=================================================");
}
