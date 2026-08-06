//! 正本删掉一条指标 → 同步后自动停用 E2E (headless):走真
//! `Command::SyncMetricsFile`(桌面「↻ 同步指标文件」按钮的同一条路径),
//! 验规则两边对称:
//!
//! 1. 正本里删掉一条 `origin='file'` 的指标 → 同步后该行 `archived=1`,
//!    **observation 一条未删**;
//! 2. 同一次同步里,`origin='manual'` 的行**一条都不被碰**——正本里本来就
//!    没有它们,"正本删了它"对它们不成立,不能被沉默清场;
//! 3. 把那条写回正本再同步 → 自动恢复(`archived=0`),不需要人再点一次。
//!
//! 把项目工作区临时指向一个**副本**目录(绝不动用户真实仓里的正本文件)。
//!
//! Run: `cargo run -p bw-app --example verify_metric_sync_archive -- <db-path> <项目名> <临时工作区> <要删的指标名>`

use bw_app::{App, Command};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

/// 库里此刻的在用/停用底账,按 (origin, archived) 分组计数 + 指定行的状态。
async fn snapshot(store: &Arc<dyn Store>, p: bw_core::ProjectId, watch: &str) -> (u32, u32, i32) {
    let sigs = store.persisted_signals(p).await.unwrap();
    let manual_archived = sigs
        .metrics
        .iter()
        .filter(|m| m.origin == bw_store::MetricOrigin::Manual && m.archived)
        .count() as u32;
    let file_archived = sigs
        .metrics
        .iter()
        .filter(|m| m.origin == bw_store::MetricOrigin::File && m.archived)
        .count() as u32;
    let watched = sigs
        .metrics
        .iter()
        .find(|m| m.name == watch)
        .map(|m| m.archived as i32)
        .unwrap_or(-1);
    (manual_archived, file_archived, watched)
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db = args
        .next()
        .expect("usage: <db> <项目名> <临时工作区> <指标名>");
    let project_name = args
        .next()
        .expect("usage: <db> <项目名> <临时工作区> <指标名>");
    let ws = args
        .next()
        .expect("usage: <db> <项目名> <临时工作区> <指标名>");
    let drop_name = args
        .next()
        .expect("usage: <db> <项目名> <临时工作区> <指标名>");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    let projects = store.list_projects().await.unwrap();
    let p = projects
        .iter()
        .find(|p| p.name == project_name)
        .unwrap_or_else(|| panic!("找不到项目 {project_name}"));
    app.dispatch(Command::OpenProject(p.id)).await.unwrap();
    // 指到副本工作区——真实仓里的正本文件一个字节不碰。
    app.dispatch(Command::SetWorkspace {
        path: ws.clone(),
        allow_commands: false,
    })
    .await
    .unwrap();

    let file = std::path::Path::new(&ws).join(".bw/metrics.toml");
    let full = std::fs::read_to_string(&file).expect("副本工作区里没有 .bw/metrics.toml");

    println!("================ 正本删指标 → 自动停用 E2E ================");
    let (m0, f0, w0) = snapshot(&store, p.id, &drop_name).await;
    println!("同步前:manual 已停用 {m0} · file 已停用 {f0} ·「{drop_name}」archived={w0}");

    // ① 从正本副本里整块删掉那条指标(按 [[lagging]]/[[leading]] 块切分)。
    let trimmed: String = full
        .split_inclusive("\n\n")
        .filter(|chunk| !chunk.contains(&format!("name   = \"{drop_name}\"")))
        .collect();
    assert!(
        trimmed.len() < full.len(),
        "没能从正本里删掉「{drop_name}」——检查名字是否逐字匹配"
    );
    std::fs::write(&file, &trimmed).unwrap();
    app.dispatch(Command::SyncMetricsFile).await.unwrap();
    let (m1, f1, w1) = snapshot(&store, p.id, &drop_name).await;
    println!("\n① 正本里删掉「{drop_name}」后同步:");
    println!(
        "   「{drop_name}」archived {w0} → {w1}   {}",
        if w1 == 1 { "✓ 自动停用" } else { "✗" }
    );
    println!(
        "   file 已停用 {f0} → {f1}   ·   manual 已停用 {m0} → {m1}   {}",
        if m0 == m1 {
            "✓ 手建行一条未被碰"
        } else {
            "✗ 手建行被误伤"
        }
    );

    // ② 写回正本再同步 → 自动恢复。
    std::fs::write(&file, &full).unwrap();
    app.dispatch(Command::SyncMetricsFile).await.unwrap();
    let (m2, _f2, w2) = snapshot(&store, p.id, &drop_name).await;
    println!("\n② 写回正本再同步:");
    println!(
        "   「{drop_name}」archived {w1} → {w2}   {}",
        if w2 == 0 { "✓ 自动恢复" } else { "✗" }
    );
    println!(
        "   manual 已停用 {m1} → {m2}   {}",
        if m1 == m2 {
            "✓ 手建行仍未被碰"
        } else {
            "✗ 手建行被误伤"
        }
    );
}
