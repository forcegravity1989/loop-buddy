//! `settings_smoke` —— 「工作区根目录可以改」的读回证据。
//!
//! ```bash
//! cargo run -p bw-v4 --example settings_smoke -- <工作目录>
//! ```
//!
//! 验三件事,都是这条设置的承诺:
//!
//! 1. 改完**存进了本机库**(`app_meta` 的 `workspaces_root` 一行),重开进程还在。
//! 2. 改完**立刻对新接进来的项目生效** —— 不用重启。
//! 3. 改完**不动已接入的项目**:老项目的仓还在老地方。这条最容易写错,
//!    写错的表现是改一下根目录、已有项目集体找不到仓。
//!
//! 不碰网络、不碰 claude;重复跑不产生重复数据(项目按 slug 幂等)。

use bw_v4::app::App;
use bw_v4::command::{Command, ProjectIntent, RemoteRef};
use bw_v4::V4Store;
use std::path::PathBuf;
use std::sync::Arc;

fn say(step: &str, detail: &str) {
    println!("[SETTINGS_SMOKE] {step}:{detail}");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bw_v4::isoweek::init_local_offset();
    let root = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("用法:cargo run -p bw-v4 --example settings_smoke -- <工作目录>");
        std::process::exit(2);
    }));
    std::fs::create_dir_all(&root)?;

    let db = root.join("v4.db");
    let old_root = root.join("workspaces-old");
    let new_root = root.join("workspaces-new");
    std::fs::create_dir_all(&old_root)?;

    let store = V4Store::open(db.to_str().unwrap()).await?;
    let mut app = App::new(
        store.clone(),
        old_root.clone(),
        Arc::new(bw_engine::MockInteractiveExecutor::default()),
    );

    let intent = |n: &str| ProjectIntent {
        name: n.to_string(),
        brief: String::new(),
        benchmark: String::new(),
        north_star: String::new(),
    };

    // ── 老根目录下先接一个项目 ───────────────────────────────
    app.dispatch(Command::CreateProject {
        slug: "before".into(),
        intent: intent("改之前接的"),
        remote: RemoteRef::default(),
        workspace_path: String::new(),
    })
    .await?;
    let before_id = store.project_by_slug("before").await?.unwrap().id;
    let before_ws = app.workspace_of(before_id).await?;
    say(
        "步骤 1 · 改之前接的项目落在",
        &before_ws.display().to_string(),
    );

    // ── 改根目录 ────────────────────────────────────────────
    let events = app
        .dispatch(Command::SetWorkspacesRoot {
            path: new_root.display().to_string(),
        })
        .await?;
    say("步骤 2 · 改根目录", &format!("{events:?}"));
    say(
        "步骤 2 · 目录被建出来了吗",
        &format!("{}", new_root.is_dir()),
    );

    // ── 存进库了没有 ────────────────────────────────────────
    let saved = store.meta(bw_v4::store::WORKSPACES_ROOT_KEY).await?;
    say(
        "步骤 3 · 库里 app_meta 存的是",
        &format!("{saved:?}(该等于新根目录)"),
    );

    // ── 改完之后接的项目,该落在新根目录 ──────────────────────
    app.dispatch(Command::CreateProject {
        slug: "after".into(),
        intent: intent("改之后接的"),
        remote: RemoteRef::default(),
        workspace_path: String::new(),
    })
    .await?;
    let after_id = store.project_by_slug("after").await?.unwrap().id;
    let after_ws = app.workspace_of(after_id).await?;
    say(
        "步骤 4 · 改之后接的项目落在",
        &format!(
            "{} · 在新根目录下={}",
            after_ws.display(),
            after_ws.starts_with(&new_root)
        ),
    );

    // ── 老项目一动没动 ──────────────────────────────────────
    let before_now = app.workspace_of(before_id).await?;
    say(
        "步骤 5 · 老项目还在原处吗",
        &format!(
            "{} · 和改之前一样={}",
            before_now.display(),
            before_now == before_ws
        ),
    );

    // ── 空路径 / 相对路径要如实弹回 ──────────────────────────
    for bad in ["", "  ", "relative/path"] {
        match app
            .dispatch(Command::SetWorkspacesRoot { path: bad.into() })
            .await
        {
            Ok(_) => say("步骤 6 · 坏路径", &format!("「{bad}」**被收了,是 bug**")),
            Err(e) => say("步骤 6 · 坏路径", &format!("「{bad}」如实弹回:{e}")),
        }
    }

    // ── 重开一次进程:设置还在吗 ─────────────────────────────
    drop(app);
    let store2 = V4Store::open(db.to_str().unwrap()).await?;
    let reread = store2.meta(bw_v4::store::WORKSPACES_ROOT_KEY).await?;
    say(
        "步骤 7 · 重开库再读",
        &format!("{reread:?}(重启后设置不该丢)"),
    );
    Ok(())
}
