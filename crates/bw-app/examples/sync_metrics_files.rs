//! **sync_metrics_files — 把每个项目工作区里的 `.bw/metrics.toml` 同步进库
//! 的 headless 入口。**
//!
//! `Command::SyncMetricsFile` 此前只有两个触发点:`MergeIssuePr` 之后
//! auto-fire,以及运营视图上的一个按钮(plan/18 ⑧)。两个都要开 UI,而
//! computer-use 在本仓的实测结论是 click 永久受阻(见 CLAUDE.md「核心纪律」),
//! 于是「改完 metrics.toml 之后让看板真的看到它」这件事在 headless 下没有路。
//! 这个例子补上那条路:遍历所有挂了工作区的项目,逐个 `OpenProject` +
//! `SyncMetricsFile`,然后把 metric 表原样读回打印——报告不代答,读回为证。
//!
//! 只读 `.bw/metrics.toml`、只写 metric 定义行(`origin='file'`)。**不产生任何
//! 观测、不碰 observation 表、不动 Signal 派生链**——`SyncMetricsFile` 本身就
//! 只同步定义不产生值(见 `docs/buddy/standards/metrics.md`「同步语义」),这里没有
//! 任何额外动作。执行器一律 MockExecutor:本例不跑活,挂它只是为了构造 `App`。
//!
//! 用法:
//!   cargo run -p bw-app --example sync_metrics_files -- <db-path> [项目名 …]
//!
//! 不给项目名 = 同步所有 `workspace_path` 非空的项目。**先在库的副本上跑一遍
//! 再跑真库**——它会真的写你的 metric 表。

use bw_app::{App, Command};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = match args.next() {
        Some(p) => p,
        None => {
            eprintln!(
                "用法: cargo run -p bw-app --example sync_metrics_files -- <db-path> [项目名 …]"
            );
            std::process::exit(2);
        }
    };
    let only: Vec<String> = args.collect();

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig {
            binary: None,
            max_budget_usd: 0.0,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    );
    app.dispatch(Command::Boot).await.expect("boot");

    let projects = store.list_projects().await.expect("list_projects");
    let mut synced = 0usize;

    for p in &projects {
        if !only.is_empty() && !only.contains(&p.name) {
            continue;
        }
        if p.workspace_path.is_empty() {
            println!("— {} · 跳过:没有工作区(workspace_path 空)", p.name);
            continue;
        }
        let file = std::path::Path::new(&p.workspace_path)
            .join(".bw")
            .join("metrics.toml");
        if !file.exists() {
            println!("— {} · 跳过:{} 不存在", p.name, file.display());
            continue;
        }

        app.dispatch(Command::OpenProject(p.id))
            .await
            .expect("open project");
        match app.dispatch(Command::SyncMetricsFile).await {
            Ok(()) => {
                synced += 1;
                println!("✓ {} · 已同步 {}", p.name, file.display());
            }
            // 解析失败只报错、不写库(整份文件必须解析成功才有任何写入),
            // 一个项目坏文件不该带倒其它项目的同步。
            Err(e) => println!("✗ {} · 同步失败(未写库):{e}", p.name),
        }
    }

    println!("\n=== 读回为证 · metric 表(同步后)===");
    for p in &projects {
        if !only.is_empty() && !only.contains(&p.name) {
            continue;
        }
        let fresh = store.get_project(p.id).await.expect("get").expect("row");
        println!(
            "\n【{}】北极星 = {:?} · collect = {}/{}",
            fresh.name,
            fresh.north_star,
            fresh.north_star_collect_kind,
            fresh.north_star_collect_query
        );
        let sigs = store.persisted_signals(p.id).await.expect("signals");
        // 只打项目级(stage_kind=None)的业务指标——阶段卡那些是 plan/18 §1.1
        // 的层 B 通用管理指标,不在本例范围。
        for m in sigs.metrics.iter().filter(|m| m.stage_kind.is_none()) {
            println!(
                "  [{:?}] {} · target={:?} · collect={}/{} · origin={:?} · signal={:?}",
                m.role, m.name, m.target_raw, m.collect_kind, m.collect_query, m.origin, m.signal
            );
        }
    }
    println!("\n同步了 {synced} 个项目。");
}
