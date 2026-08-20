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
//! 6. 每一档接完都自动建了那张「规范铺底」的活,而且**它最远只到评审中**
//! 7. 从工作台移走一个项目:库里干干净净,**仓一个字节都不动**
//! 8. **成熟仓**(自己已经有 CLAUDE.md / AGENTS.md / README):铺底不许盖掉它们
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

    // ── 6 · 接入自动建的那张「规范铺底」活 ────────────────────
    // 母文档 §2 第 0 站:接入完 buddy 自己建一张一次性运作活③。这里读回它
    // 真的建了、来源是 auto、而且**状态最远只到评审中** —— 自动建的活绝不
    // 自动完成,这条是铁律。
    for slug in ["no-remote", "already"] {
        let pid = store.project_by_slug(slug).await?.unwrap().id;
        let rows = store.issues(pid).await?;
        let shown: Vec<String> = rows
            .iter()
            .map(|i| {
                format!(
                    "#{} 「{}」 来源={:?} 状态={:?}",
                    i.number, i.title, i.origin, i.status
                )
            })
            .collect();
        say(
            &format!("档 6 · {slug} 接完自动建的活"),
            &format!("{} 张:{}", rows.len(), shown.join(" | ")),
        );
        say(
            &format!("档 6 · {slug} 有没有活被推到「完成」"),
            &format!(
                "{}(该是 false —— 自动建的活绝不自动完成)",
                rows.iter()
                    .any(|i| i.status == bw_v4::model::IssueStatus::Done)
            ),
        );
    }

    // ── 8 · 成熟仓不许被盖 ──────────────────────────────────
    // loop-buddy 自己就是这种仓:根目录有一份人写了很久的 CLAUDE.md。铺底
    // 要往仓里写 AGENTS.md 和 CLAUDE.md,**盖掉就是事故**。
    let mature = workspaces.join("mature");
    std::fs::create_dir_all(&mature)?;
    git(&mature, &["init", "-q"]).await;
    std::fs::write(
        &mature.join("CLAUDE.md"),
        "# 人写了很久的 CLAUDE.md\n别动我\n",
    )?;
    std::fs::write(&mature.join("AGENTS.md"), "# 人写的 AGENTS.md\n也别动我\n")?;
    std::fs::write(&mature.join("README.md"), "# 人写的 README\n")?;
    // 有构建文件,才验得了「怎么建怎么测」那一节是真探出来的还是编的。
    std::fs::write(
        &mature.join("Cargo.toml"),
        "[package]\nname = \"mature\"\nversion = \"0.1.0\"\n",
    )?;
    std::fs::create_dir_all(mature.join("src"))?;
    std::fs::write(mature.join("src/lib.rs"), "")?;
    git(&mature, &["add", "-A"]).await;
    git(
        &mature,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    )
    .await;
    app.dispatch(Command::CreateProject {
        slug: "mature".into(),
        intent: intent("成熟仓"),
        remote: RemoteRef::default(),
        workspace_path: String::new(),
    })
    .await?;
    drain("档8", &mut prog_rx);
    for f in ["CLAUDE.md", "AGENTS.md", "README.md"] {
        let body = std::fs::read_to_string(mature.join(f)).unwrap_or_default();
        say(
            &format!("档 8 · 主检出的 {f}"),
            &format!(
                "首行「{}」· 还是人写的那份={}",
                body.lines().next().unwrap_or("(空)"),
                body.contains("人写")
            ),
        );
    }

    // ── 9 · 名片没在主检出里留一份「未跟踪的双胞胎」 ──────────
    // 接入那一步为了让界面立刻有东西看,把名片写进主检出;铺底又把同一份提交
    // 进分支。两份都留着的话,人合完 MR 一 `git pull`,git 会一句
    // 「untracked working tree files would be overwritten by merge」顶回来,
    // 拉不动。所以提交成功之后主检出那份要没了 —— 正本在分支上。
    let twin = mature.join(bw_v4::repo::project_file::REL_PATH);
    let on_branch = std::process::Command::new("git")
        .args([
            "-C",
            mature.to_str().unwrap(),
            "show",
            "bw/issue-1:.bw/project.toml",
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    say(
        "档 9 · 名片",
        &format!(
            "主检出里还留着吗={}(该是 false)· 分支上有吗={}(该是 true)",
            twin.exists(),
            on_branch
        ),
    );

    // ── 10 · 开发手册是**探出来的**,不是模板里编的 ──────────
    // 仓根 AGENTS.md 是给这个项目自己的开发手册。buddy 第 1 步不起 agent,只写
    // 读文件就能确定的部分:有 Cargo.toml 就写 cargo 那几条,顶层目录照实列。
    // 这个仓的仓根本来就有人写的 AGENTS.md,所以 buddy 一个字都不该写 ——
    // 补齐是第 2 步 agent 会话的活。用空仓那个项目验探测本身。
    say(
        "档 10 · 成熟仓的仓根 AGENTS.md",
        &format!(
            "还是人写的那份={}(该是 true —— 已存在就一个字不覆盖)",
            std::fs::read_to_string(mature.join("AGENTS.md"))
                .unwrap_or_default()
                .contains("人写的")
        ),
    );
    let detected = bw_v4::standard::detect::build_commands(&mature);
    say(
        "档 10 · 从这个仓探出来的构建命令",
        &format!(
            "认出 cargo={} · 认出 CI={} · 编了一条命令={}",
            detected.contains("cargo test"),
            detected.contains("门禁以 CI 为准") || !mature.join(".github").exists(),
            detected.contains("npm") || detected.contains("pytest")
        ),
    );

    // ── 7 · 从工作台移走 ────────────────────────────────────
    // 项目卡右上角那个 ×。只动库,绝不动仓 —— 这条要能读回来,不然「移走」
    // 就成了「删掉我的代码」。
    let gone = store.project_by_slug("already").await?.unwrap().id;
    let ws_before = already.join(".git").is_dir();
    let ev = app
        .dispatch(Command::RemoveProject { project_id: gone })
        .await?;
    say("档 7 · 移走的回执", &format!("{ev:?}"));
    say(
        "档 7 · 库里还有它吗",
        &format!(
            "项目={:?} · 活={} 张(都该是空)",
            store.project_by_slug("already").await?.map(|p| p.slug),
            store.issues(gone).await?.len()
        ),
    );
    say(
        "档 7 · 仓还在吗",
        &format!(
            "移走前 .git={ws_before} → 移走后 .git={} · 目录还在={}",
            already.join(".git").is_dir(),
            already.is_dir()
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
