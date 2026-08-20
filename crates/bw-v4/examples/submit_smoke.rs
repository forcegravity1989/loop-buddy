//! `submit_smoke` —— 第 4 站到第 5 站那一下的读回证据。
//!
//! 验的是「提交并开 MR」(`Command::SubmitIssueWork`):agent 在活自己的 worktree
//! 里改完文件之后,这一下有没有真的**提交、推分支、开 MR、把活推进「评审中」**。
//!
//! ```bash
//! cargo run -p bw-v4 --example submit_smoke -- <工作目录>
//! ```
//!
//! 三条取舍,如实写在这里:
//!
//! 1. **远端是本机的一个 bare 仓**(`git init --bare`),不是 GitHub。所以
//!    「推分支」是真推、能 `git -C <bare> log` 读回来;`gh pr create` 那一步会
//!    如实失败,正好用来验「开 MR 没成」时**不假装进了评审**这条。
//! 2. **要连 `gh` 那一步一起验**,在 PATH 前面放一个假 `gh`(见脚本注释),
//!    它把收到的参数记下来并回一个 PR 链接 —— 验的是**我们把命令拼对了没有**,
//!    不是「真在 GitHub 上开出了 PR」。两件事别混。
//! 3. **不碰 claude、不碰网络**。agent 干活这一段由脚本直接往 worktree 里写文件
//!    模拟,因为这一刀要验的是它**之后**那一下。

use bw_v4::app::App;
use bw_v4::command::{Command, Event, ProjectIntent, RemoteRef};
use bw_v4::model::{IssueKind, IssueOrigin, IssueStatus};
use bw_v4::V4Store;
use std::path::{Path, PathBuf};
use std::process::Command as Cmd;
use std::sync::Arc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bw_v4::isoweek::init_local_offset();
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| {
        eprintln!("用法:cargo run -p bw-v4 --example submit_smoke -- <工作目录>");
        std::process::exit(2);
    }));
    std::fs::create_dir_all(&root)?;

    // ── 造一个真仓 + 一个本机 bare「远端」 ───────────────────
    let bare = root.join("origin.git");
    git(&root, &["init", "--bare", "-b", "main", "origin.git"])?;
    let ws = root.join("proj");
    std::fs::create_dir_all(&ws)?;
    git(&ws, &["init", "-b", "main"])?;
    git(&ws, &["config", "user.name", "smoke"])?;
    git(&ws, &["config", "user.email", "smoke@example.com"])?;
    std::fs::write(ws.join("README.md"), "# smoke\n")?;
    git(&ws, &["add", "-A"])?;
    git(&ws, &["commit", "-m", "init"])?;
    git(&ws, &["remote", "add", "origin", bare.to_str().unwrap()])?;
    git(&ws, &["push", "-u", "origin", "main"])?;

    let store = V4Store::open(root.join("v4.db").to_str().unwrap()).await?;
    let mut app = App::new(
        store.clone(),
        &root,
        Arc::new(bw_engine::MockInteractiveExecutor::new()),
    );

    // provider=github + 一个假 owner/repo:开 MR 那一步会走 `gh`。
    let events = app
        .dispatch(Command::CreateProject {
            slug: "proj".into(),
            intent: ProjectIntent {
                name: "冒烟项目".into(),
                brief: "验第 4 站到第 5 站".into(),
                benchmark: "—".into(),
                north_star: "—".into(),
            },
            remote: RemoteRef {
                provider: "github".into(),
                host: "github.com".into(),
                path: "smoke/proj".into(),
            },
            workspace_path: ws.display().to_string(),
        })
        .await?;
    let pid = events
        .iter()
        .find_map(|e| match e {
            Event::ProjectCreated { id, .. } => Some(*id),
            _ => None,
        })
        .expect("项目没建出来");

    // ── 建一张业务活 ───────────────────────────────────────
    let events = app
        .dispatch(Command::CreateIssue {
            project_id: pid,
            title: "把冒烟活干出来".into(),
            body: "随便改点东西".into(),
            category: None,
            kind: IssueKind::Business,
            origin: IssueOrigin::Human,
            week_of: bw_v4::isoweek::current_week(),
        })
        .await?;
    let (id, number) = events
        .iter()
        .find_map(|e| match e {
            Event::IssueCreated { id, number } => Some((*id, *number)),
            _ => None,
        })
        .expect("活没建出来");

    // 主检出此刻的样子当基线 —— 接入项目那一步本来就会往仓里写 `.bw/`,
    // 那是「接入」干的,和这一刀无关。要验的是**这一刀之后主检出有没有变**。
    let baseline = out(&ws, &["status", "--porcelain"]).unwrap_or_default();

    // ── ▶跑:开出这张活的 worktree(替身执行器,不碰 claude) ──
    let ran = app.dispatch(Command::RunIssue { id }).await?;
    say("步骤 1 · ▶跑", &format!("{ran:?}"));

    let tree =
        bw_engine::workspace::issue_worktree_path(&ws, number).expect("算不出 worktree 路径");
    say(
        "步骤 2 · worktree",
        &format!(
            "{} · 存在={} · 分支={}",
            tree.display(),
            tree.is_dir(),
            head_branch(&tree)
        ),
    );

    // ── 树上没干出东西就点提交:必须如实弹回,不许假装进评审 ──
    // 另建一张活、只推到「进行中」**不跑**,它的树就是一棵干净的检出。
    let events = app
        .dispatch(Command::CreateIssue {
            project_id: pid,
            title: "一张没干过活的活".into(),
            body: String::new(),
            category: None,
            kind: IssueKind::Business,
            origin: IssueOrigin::Human,
            week_of: bw_v4::isoweek::current_week(),
        })
        .await?;
    let empty_id = events
        .iter()
        .find_map(|e| match e {
            Event::IssueCreated { id, .. } => Some(*id),
            _ => None,
        })
        .expect("活没建出来");
    app.dispatch(Command::TransitionIssue {
        id: empty_id,
        to: IssueStatus::InProgress,
    })
    .await?;
    match app
        .dispatch(Command::SubmitIssueWork { id: empty_id })
        .await
    {
        Ok(e) => say("步骤 3 · 干净树点提交", &format!("**没弹回,是 bug** {e:?}")),
        Err(e) => say("步骤 3 · 干净树点提交", &format!("如实弹回:{e}")),
    }
    say(
        "步骤 3 · 它的状态没被改动",
        &format!("{:?}(该是 InProgress)", status_of(&store, empty_id).await?),
    );

    // ── 模拟 agent 在这棵树里干活(写文件,不提交) ──────────
    std::fs::write(tree.join("AGENT_WORK.md"), "agent 干出来的东西\n")?;
    let events = app.dispatch(Command::SubmitIssueWork { id }).await?;
    for e in &events {
        if let Event::IssueSubmitted {
            branch,
            commits,
            pr_number,
            note,
            ..
        } = e
        {
            say(
                "步骤 4 · 提交并开 MR",
                &format!("分支={branch} 提交数={commits} MR号={pr_number} 说明={note}"),
            );
        }
    }
    say(
        "步骤 4 · 活的状态",
        &format!("{:?}", status_of(&store, id).await?),
    );

    // ── 读回:远端 bare 仓上真有这条分支和这个提交吗 ──────────
    let branch = bw_engine::github::issue_branch(number);
    say(
        "步骤 5 · 远端读回",
        &out(&bare, &["log", "--oneline", "-1", &branch]).unwrap_or_else(|e| format!("读不到:{e}")),
    );
    say(
        "步骤 5 · 远端那个提交带了哪些文件",
        &out(&bare, &["show", "--stat", "--format=%h %s", &branch])
            .unwrap_or_default()
            .replace('\n', " | "),
    );
    let after = out(&ws, &["status", "--porcelain"]).unwrap_or_default();
    say(
        "步骤 5 · 主检出",
        &format!(
            "分支={} · 和这一刀之前一模一样={}",
            head_branch(&ws),
            after == baseline
        ),
    );

    // ── 「合入并完成」:没有真 MR 时必须如实说没合 ────────────
    let events = app.dispatch(Command::MergeAndSettle { id }).await?;
    for e in &events {
        match e {
            Event::IssueMerged {
                pr_number,
                merged,
                local_note,
                ..
            } => say(
                "步骤 6 · 合入并完成",
                &format!("MR号={pr_number} 真合了={merged} 本机收尾={local_note:?}"),
            ),
            Event::IssueTransitioned { to, settled, .. } => say(
                "步骤 6 · 结清",
                &format!("状态={to:?} 这次真结清了={settled}"),
            ),
            _ => {}
        }
    }
    say(
        "步骤 7 · 结清后 worktree",
        &format!(
            "{} 还在={}(干净才收;这棵树里 agent 的改动已经提交,所以该被收掉)",
            tree.display(),
            tree.is_dir()
        ),
    );
    Ok(())
}

async fn status_of(
    store: &V4Store,
    id: bw_v4::model::IssueId,
) -> Result<IssueStatus, Box<dyn std::error::Error>> {
    Ok(store.issue(id).await?.expect("活没了").status)
}

fn say(step: &str, detail: &str) {
    println!("[SUBMIT_SMOKE] {step}:{detail}");
}

fn head_branch(dir: &Path) -> String {
    out(dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn git(dir: &Path, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let st = Cmd::new("git").current_dir(dir).args(args).output()?;
    if !st.status.success() {
        return Err(format!("git {args:?} 失败:{}", String::from_utf8_lossy(&st.stderr)).into());
    }
    Ok(())
}

fn out(dir: &Path, args: &[&str]) -> Result<String, String> {
    let o = Cmd::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).trim().to_string())
}
