//! `merge_smoke` —— 「合入并完成」这一下的读回证据。
//!
//! ```bash
//! cargo run -p bw-v4 --example merge_smoke -- <工作目录>
//! ```
//!
//! 验四件事,每一件都读回、不听谁说:
//!
//! 1. MR 真合了之后,**主检出被拉到了最新** —— 合进去的文件在工作区里真的
//!    出现了。这是 2026-08-20 试点抓出来的缺口:合是合了,本机主检出还停在旧
//!    提交,合进去的 `.bw/` 那几份件在工作区里根本不存在。
//! 2. **本机那条 `bw/issue-<号>` 分支被收掉了**。
//! 3. **合不成就整条不算数**:活留在「评审中」,没结清。
//! 4. 拉不动的时候**如实说拉不动**,不假装拉过了。
//!
//! 三条取舍,如实写在这里:
//!
//! 1. **远端是本机的一个 bare 仓**(`git init --bare`),不是 GitHub。
//! 2. **PATH 前面放一个假 `gh`**(本例自己生成):`gh pr create` 回一个 PR 链
//!    接,`gh pr merge` 在那个 bare 仓上**真做一次 squash 合入**。验的是我们
//!    这一侧的命令拼没拼对、合完之后本机收没收干净 —— 不是「真在 GitHub 上
//!    合了一个 PR」。两件事别混。
//! 3. **不碰 claude、不碰网络**。agent 干活那一段由本例直接往 worktree 里写文
//!    件模拟。

use bw_v4::app::App;
use bw_v4::command::{Command, Event, ProjectIntent, RemoteRef};
use bw_v4::model::{IssueId, IssueKind, IssueOrigin};
use bw_v4::V4Store;
use std::path::{Path, PathBuf};
use std::process::Command as Cmd;
use std::sync::Arc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    bw_v4::isoweek::init_local_offset();
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| {
        eprintln!("用法:cargo run -p bw-v4 --example merge_smoke -- <工作目录>");
        std::process::exit(2);
    }));
    std::fs::create_dir_all(&root)?;
    let root = std::fs::canonicalize(&root)?;

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

    install_fake_gh(&root)?;

    let store = V4Store::open(root.join("v4.db").to_str().unwrap()).await?;
    let mut app = App::new(
        store.clone(),
        &root,
        Arc::new(v4_engine::MockInteractiveExecutor::new()),
    );
    let events = app
        .dispatch(Command::CreateProject {
            slug: "proj".into(),
            intent: ProjectIntent {
                name: "合入冒烟".into(),
                brief: "验合入之后本机收没收干净".into(),
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

    // ── ①合得成的那一条 ────────────────────────────────────
    let (id, number) = new_issue(&mut app, pid, "合得成的活").await?;
    let branch = v4_engine::github::issue_branch(number);
    app.dispatch(Command::RunIssue { id }).await?;
    let tree = v4_engine::workspace::issue_worktree_path(&ws, number).expect("算不出 worktree");
    std::fs::write(
        tree.join("MERGED_FILE.md"),
        "这是合进主干之后该出现在主检出里的文件\n",
    )?;
    app.dispatch(Command::SubmitIssueWork { id }).await?;
    std::fs::write(root.join("merge_branch"), &branch)?;

    let head_before = out(&ws, &["rev-parse", "HEAD"]).unwrap_or_default();
    say(
        "①合之前 · 主检出",
        &format!(
            "HEAD={} · MERGED_FILE.md 在工作区里={} · 本机分支={:?}",
            &head_before[..7.min(head_before.len())],
            ws.join("MERGED_FILE.md").exists(),
            out(&ws, &["branch", "--list", &branch]).unwrap_or_default()
        ),
    );

    for e in &app.dispatch(Command::MergeAndSettle { id }).await? {
        match e {
            Event::IssueMerged {
                pr_number,
                merged,
                local_note,
                ..
            } => say(
                "①合入并完成",
                &format!("MR号={pr_number} 真合了={merged} 本机收尾={local_note}"),
            ),
            Event::IssueTransitioned { to, settled, .. } => {
                say("①结清", &format!("状态={to:?} 这次真结清了={settled}"))
            }
            _ => {}
        }
    }
    let head_after = out(&ws, &["rev-parse", "HEAD"]).unwrap_or_default();
    say(
        "①合之后 · 主检出",
        &format!(
            "HEAD={} · 挪动了={} · 最后一条提交={:?}",
            &head_after[..7.min(head_after.len())],
            head_before != head_after,
            out(&ws, &["log", "--oneline", "-1"]).unwrap_or_default()
        ),
    );
    say(
        "①合之后 · 合进去的文件真的在工作区里了吗",
        &format!("MERGED_FILE.md 存在={}", ws.join("MERGED_FILE.md").exists()),
    );
    say(
        "①合之后 · 本机活分支",
        &format!(
            "`git branch --list {branch}` = {:?}(空 = 已收掉)· worktree 还在={}",
            out(&ws, &["branch", "--list", &branch]).unwrap_or_default(),
            tree.is_dir()
        ),
    );
    let issue = store.issue(id).await?.expect("活没了");
    say(
        "①合之后 · 库里读回",
        &format!("状态={:?} settled_at={:?}", issue.status, issue.settled_at),
    );

    // ── ②合不成的那一条:整条不算数 ────────────────────────
    let (id2, number2) = new_issue(&mut app, pid, "合不成的活").await?;
    let branch2 = v4_engine::github::issue_branch(number2);
    app.dispatch(Command::RunIssue { id: id2 }).await?;
    let tree2 = v4_engine::workspace::issue_worktree_path(&ws, number2).expect("算不出 worktree");
    std::fs::write(tree2.join("NOT_MERGED.md"), "这份不该进主干\n")?;
    app.dispatch(Command::SubmitIssueWork { id: id2 }).await?;
    std::fs::write(root.join("merge_branch"), &branch2)?;
    // 让假 gh 这一次合失败(模拟冲突 / 没权限 / 没网)。
    std::fs::write(root.join("fail_merge"), "1")?;
    match app.dispatch(Command::MergeAndSettle { id: id2 }).await {
        Ok(e) => say("②合不成", &format!("**没弹回,是 bug** {e:?}")),
        Err(e) => say("②合不成 · 如实弹回", &e.to_string()),
    }
    let issue2 = store.issue(id2).await?.expect("活没了");
    say(
        "②合不成 · 库里读回",
        &format!(
            "状态={:?}(该是 InReview)settled_at={:?}(该是 None)",
            issue2.status, issue2.settled_at
        ),
    );
    say(
        "②合不成 · 本机分支和 worktree",
        &format!(
            "`git branch --list {branch2}` = {:?}(该还在)· worktree 还在={}",
            out(&ws, &["branch", "--list", &branch2]).unwrap_or_default(),
            tree2.is_dir()
        ),
    );
    say(
        "②合不成 · 远端主干",
        &format!(
            "NOT_MERGED.md 进主干了吗={}",
            out(&bare, &["ls-tree", "--name-only", "main"])
                .unwrap_or_default()
                .lines()
                .any(|l| l == "NOT_MERGED.md")
        ),
    );

    // ── ③拉不动的时候如实说拉不动 ──────────────────────────
    // 先照常干活、推分支、开 MR(这时候 origin 还是好的),**再**把主检出的
    // origin 指到一个不存在的路径 —— 于是远端那一下合得成,本机 `git fetch`
    // 必然失败。合入与结清照样生效,只是主检出没拉动,那句话必须原样出现在
    // 回执里,不许写成「已拉到最新」。
    std::fs::remove_file(root.join("fail_merge"))?;
    let (id3, number3) = new_issue(&mut app, pid, "拉不动的活").await?;
    let branch3 = v4_engine::github::issue_branch(number3);
    app.dispatch(Command::RunIssue { id: id3 }).await?;
    let tree3 = v4_engine::workspace::issue_worktree_path(&ws, number3).expect("算不出 worktree");
    std::fs::write(tree3.join("THIRD.md"), "第三张\n")?;
    app.dispatch(Command::SubmitIssueWork { id: id3 }).await?;
    std::fs::write(root.join("merge_branch"), &branch3)?;
    git(
        &ws,
        &["remote", "set-url", "origin", "/nope/not-a-repo.git"],
    )?;
    for e in &app.dispatch(Command::MergeAndSettle { id: id3 }).await? {
        if let Event::IssueMerged {
            merged, local_note, ..
        } = e
        {
            say("③拉不动", &format!("真合了={merged} 本机收尾={local_note}"));
        }
    }
    let issue3 = store.issue(id3).await?.expect("活没了");
    say(
        "③拉不动 · 库里读回",
        &format!(
            "状态={:?} settled_at 有值={} · 主检出最后一条提交={:?}",
            issue3.status,
            issue3.settled_at.is_some(),
            out(&ws, &["log", "--oneline", "-1"]).unwrap_or_default()
        ),
    );
    Ok(())
}

async fn new_issue(
    app: &mut App,
    pid: bw_v4::model::ProjectId,
    title: &str,
) -> Result<(IssueId, u32), Box<dyn std::error::Error>> {
    let events = app
        .dispatch(Command::CreateIssue {
            project_id: pid,
            title: title.into(),
            body: String::new(),
            category: None,
            kind: IssueKind::Business,
            origin: IssueOrigin::Human,
            week_of: bw_v4::isoweek::current_week(),
        })
        .await?;
    Ok(events
        .iter()
        .find_map(|e| match e {
            Event::IssueCreated { id, number } => Some((*id, *number)),
            _ => None,
        })
        .expect("活没建出来"))
}

/// 生成一个假 `gh` 并把它塞到 PATH 最前面。
///
/// `pr create` 回一个 PR 链接(号码是假的,但**格式和真 gh 一样**,走的是同一
/// 条解析);`pr merge` 在 bare 仓上真做一次 squash 合入 —— 真 gh 合 PR 也是
/// squash,所以本机那条分支的提交**不会**成为主干的祖先,`git branch -d` 必然
/// 拒收。这一点正是要验的。
fn install_fake_gh(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let bin = root.join("fakebin");
    std::fs::create_dir_all(&bin)?;
    let script = format!(
        r#"#!/bin/sh
ROOT="{root}"
case "$1 $2" in
  "pr create") echo "https://example.invalid/smoke/proj/pull/101"; exit 0;;
  "pr merge")
    if [ -f "$ROOT/fail_merge" ]; then
      echo "假 gh:这一次故意合不了(模拟冲突/没权限/没网)" >&2
      exit 1
    fi
    br=$(cat "$ROOT/merge_branch")
    wc="$ROOT/gh-merge-wc"
    rm -rf "$wc"
    git clone -q "$ROOT/origin.git" "$wc" || exit 1
    cd "$wc" || exit 1
    git config user.email fake-gh@example.com
    git config user.name fake-gh
    git checkout -q main || exit 1
    git merge --squash -q "origin/$br" || exit 1
    git commit -q -m "squash 合入 $br(假 gh)" || exit 1
    git push -q origin main || exit 1
    echo "merged $br"
    exit 0;;
esac
echo "假 gh 不认识这条命令:$*" >&2
exit 1
"#,
        root = root.display()
    );
    let gh = bin.join("gh");
    std::fs::write(&gh, script)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755))?;
    }
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old}", bin.display()));
    Ok(())
}

fn say(step: &str, detail: &str) {
    println!("[MERGE_SMOKE] {step}:{detail}");
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
