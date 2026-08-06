//! **set_project_workspace — 给一个存量项目挂/换工作区的 headless 入口。**
//!
//! `Command::SetWorkspace` 只有 UI 触发点,而 click 在本仓实测永久受阻(见
//! CLAUDE.md「核心纪律」),于是「给一个建好但没挂工作区的项目补上目录」这件
//! 事在 headless 下没有路——而没有工作区,它的 `.bw/metrics.toml` 就无处安放、
//! `SyncMetricsFile` 也无从读起。这个例子补上那条路。
//!
//! 走的是真命令层(`OpenProject` + `SetWorkspace`),所以 `SetWorkspace` 自带
//! 的守卫照常生效:路径非空但不是真实存在的目录 → 当场失败,不写库。
//!
//! `allow_commands` 默认 `false`(保守档:不让 agent 在这个工作区里跑 shell
//! 命令),要放开显式传 `--allow-commands`。
//!
//! 用法:
//!   cargo run -p bw-app --example set_project_workspace -- <db-path> <项目名> <目录> [--allow-commands]
//!
//! 它会真的改你的库——先在副本上跑一遍。

use bw_app::{App, Command};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!(
            "用法: cargo run -p bw-app --example set_project_workspace -- \
             <db-path> <项目名> <目录> [--allow-commands]"
        );
        std::process::exit(2);
    }
    let (db_path, name, dir) = (&args[0], &args[1], &args[2]);
    let allow_commands = args.iter().any(|a| a == "--allow-commands");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(db_path).await.expect("open db"));
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
    let Some(p) = projects.iter().find(|p| &p.name == name) else {
        eprintln!(
            "找不到项目:{name}(库里有:{:?})",
            projects.iter().map(|p| &p.name).collect::<Vec<_>>()
        );
        std::process::exit(1);
    };
    println!(
        "项目 {} · 原 workspace_path = {:?}",
        p.name, p.workspace_path
    );

    app.dispatch(Command::OpenProject(p.id))
        .await
        .expect("open project");
    if let Err(e) = app
        .dispatch(Command::SetWorkspace {
            path: dir.clone(),
            allow_commands,
        })
        .await
    {
        // 目录不存在等守卫失败:如实报错、一个字节没写库。
        eprintln!("✗ 设置失败(未写库):{e}");
        std::process::exit(1);
    }

    // 读回为证——不信任 dispatch 的返回,重新从库里取。
    let fresh = store.get_project(p.id).await.expect("get").expect("row");
    println!(
        "✓ 读回:workspace_path = {:?} · allow_commands = {}",
        fresh.workspace_path, fresh.allow_commands
    );
    let file = std::path::Path::new(&fresh.workspace_path)
        .join(".bw")
        .join("metrics.toml");
    println!(
        "  指标正本 {} · {}",
        file.display(),
        if file.exists() {
            "已就位,可跑 sync_metrics_files"
        } else {
            "尚不存在(文件到位后再跑 sync_metrics_files)"
        }
    );
}
