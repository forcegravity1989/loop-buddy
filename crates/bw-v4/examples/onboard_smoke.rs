//! `onboard_smoke` —— 「完成接入」那一下到底做了什么的读回证据。
//!
//! ```bash
//! cargo run -p bw-v4 --example onboard_smoke -- <工作目录> [owner/repo]
//! ```
//!
//! 给了 `owner/repo` 才跑真 clone 那一档(要 `gh auth login` 过);不给就只跑
//! 不碰网络的四档。不碰 claude、不碰网关。工作目录每次跑之前会被清空。
//!
//! 五档,每档都是「接入」会遇到的一种真实局面:
//!
//! 1. 目录不在 + 没填远端 → 建个空目录当仓,**并说明它是空的**
//! 2. 目录里已经是个 git 仓 → 直接用,顺带把 origin 报出来
//! 3. 目录里有东西、又不是 git 仓 → **弹回**,而且库里不许留下半个项目
//! 4. 同一个 slug 再接一次 → 幂等,不建第二行
//! 5. 填了远端、目录不在 → 真 clone 下来,HEAD 能读回
//!
//! 第 3 档是这个例子最要紧的一条:V4 没有「删项目」这条命令,接入中途失败要是
//! 在库里留了一行,那一行就永远赖在项目墙上了。

use bw_v4::app::{App, ProgressLine};
use bw_v4::command::{Command, ProjectIntent, RemoteRef};
use bw_v4::V4Store;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn say(step: &str, detail: &str) {
    println!("[ONBOARD_SMOKE] {step}:{detail}");
}

fn intent(n: &str) -> ProjectIntent {
    ProjectIntent {
        name: n.to_string(),
        brief: String::new(),
        benchmark: String::new(),
        north_star: String::new(),
    }
}

async fn git(dir: &Path, args: &[&str]) {
    tokio::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .expect("跑 git");
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bw_v4::isoweek::init_local_offset();
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(args.next().unwrap_or_else(|| {
        eprintln!("用法:cargo run -p bw-v4 --example onboard_smoke -- <工作目录> [owner/repo]");
        std::process::exit(2);
    }));
    let real_repo = args.next();
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root)?;

    let workspaces = root.join("workspaces");
    std::fs::create_dir_all(&workspaces)?;
    let store = V4Store::open(root.join("v4.db").to_str().unwrap()).await?;
    let (prog_tx, mut prog_rx) = tokio::sync::broadcast::channel::<ProgressLine>(64);
    let mut app = App::new(
        store.clone(),
        workspaces.clone(),
        Arc::new(bw_engine::MockInteractiveExecutor::default()),
    )
    .with_progress(prog_tx);

    // 每一档跑完就把这一档报的进度行倒出来 —— 界面上看到的就是这些。
    let mut drain = |label: &str, rx: &mut tokio::sync::broadcast::Receiver<ProgressLine>| {
        while let Ok(l) = rx.try_recv() {
            println!(
                "[ONBOARD_SMOKE]   {label} 进度 · 第{}步 {:?} {}",
                l.step, l.state, l.text
            );
        }
    };

    // ── 1 · 没填远端 ────────────────────────────────────────
    app.dispatch(Command::CreateProject {
        slug: "no-remote".into(),
        intent: intent("没远端的项目"),
        remote: RemoteRef::default(),
        workspace_path: String::new(),
    })
    .await?;
    drain("档1", &mut prog_rx);
    say(
        "档 1 · 空目录建出来了吗",
        &format!("{}", workspaces.join("no-remote").is_dir()),
    );

    // ── 2 · 目录里已经是个 git 仓 ────────────────────────────
    let already = workspaces.join("already");
    std::fs::create_dir_all(&already)?;
    git(&already, &["init", "-q"]).await;
    git(
        &already,
        &["remote", "add", "origin", "https://example.com/a/b.git"],
    )
    .await;
    app.dispatch(Command::CreateProject {
        slug: "already".into(),
        intent: intent("本机已有仓"),
        remote: RemoteRef::default(),
        workspace_path: String::new(),
    })
    .await?;
    drain("档2", &mut prog_rx);
    say(
        "档 2 · 那个仓被动过吗",
        &format!(
            "{}(.git 还在,没被 clone 覆盖)",
            already.join(".git").is_dir()
        ),
    );

    // ── 3 · 目录里有东西、又不是 git 仓 ───────────────────────
    let junk = workspaces.join("junk");
    std::fs::create_dir_all(&junk)?;
    std::fs::write(junk.join("别人的文件.txt"), "不许动我")?;
    let refused = app
        .dispatch(Command::CreateProject {
            slug: "junk".into(),
            intent: intent("撞上别人目录"),
            remote: RemoteRef {
                provider: "github".into(),
                host: "github.com".into(),
                path: "someone/else".into(),
            },
            workspace_path: String::new(),
        })
        .await;
    drain("档3", &mut prog_rx);
    say("档 3 · 弹回了吗", &format!("{refused:?}"));
    say(
        "档 3 · 库里留下项目了吗",
        &format!(
            "{:?}(该是 None —— 留下就删不掉了)",
            store.project_by_slug("junk").await?.map(|p| p.slug)
        ),
    );
    say(
        "档 3 · 别人的文件还在吗",
        &format!("{}", junk.join("别人的文件.txt").is_file()),
    );

    // ── 4 · 幂等 ────────────────────────────────────────────
    let before = store.projects().await?.len();
    app.dispatch(Command::CreateProject {
        slug: "no-remote".into(),
        intent: intent("再接一次"),
        remote: RemoteRef::default(),
        workspace_path: String::new(),
    })
    .await?;
    drain("档4", &mut prog_rx);
    say(
        "档 4 · 项目条数",
        &format!(
            "接之前 {before} → 接之后 {}(该一样)",
            store.projects().await?.len()
        ),
    );

    // ── 5 · 真 clone ────────────────────────────────────────
    match real_repo {
        None => say("档 5 · 真 clone", "没给 owner/repo,跳过"),
        Some(slug_path) => {
            let name = slug_path.rsplit('/').next().unwrap_or("cloned").to_string();
            let r = app
                .dispatch(Command::CreateProject {
                    slug: name.clone(),
                    intent: intent(&name),
                    remote: RemoteRef {
                        provider: "github".into(),
                        host: "github.com".into(),
                        path: slug_path.clone(),
                    },
                    workspace_path: String::new(),
                })
                .await;
            drain("档5", &mut prog_rx);
            say("档 5 · 接入结果", &format!("{r:?}"));
            let ws = workspaces.join(&name);
            say(
                "档 5 · clone 下来了吗",
                &format!(
                    ".git={} · .bw/project.toml={}",
                    ws.join(".git").is_dir(),
                    ws.join(".bw/project.toml").is_file()
                ),
            );
            let head = tokio::process::Command::new("git")
                .args(["log", "-1", "--oneline"])
                .current_dir(&ws)
                .output()
                .await?;
            say(
                "档 5 · 仓里最新一条提交",
                String::from_utf8_lossy(&head.stdout).trim(),
            );
        }
    }
    Ok(())
}
