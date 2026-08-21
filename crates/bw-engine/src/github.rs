//! GitHub shell-out — mints or adopts a GitHub repo via the `gh` CLI, same
//! subprocess pattern `workspace.rs` uses for local git. Relies entirely on
//! the user's own `gh auth login` on this machine; no token handling here.

use crate::workspace::{commit_initial, git_in, stage_commit_push, stage_commit_push_msg};
use std::path::Path;
use std::process::Stdio;
use time::Date;

#[derive(Debug, thiserror::Error)]
pub enum GithubError {
    #[error("gh 未安装或不在 PATH")]
    NotInstalled,
    #[error("gh 命令失败:{0}")]
    Command(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubRepoRef {
    pub owner: String,
    pub repo: String,
    pub html_url: String,
    pub private: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubRepoSummary {
    pub owner: String,
    pub repo: String,
    pub private: bool,
    /// C16(plan/14 规范条 4): 仓描述 —— `gh repo list --json description` 的
    /// 原文;空串 = 仓本身没填描述(真实状态),不是"没取到"。
    pub description: String,
    /// 默认分支名(如 `main`)—— `defaultBranchRef.name`;空 = 空仓无提交这类
    /// 边缘情况下 gh 拿不到,如实留白。
    pub default_branch: String,
    /// 最近一次 push 的 ISO8601 时间戳(`pushedAt`);空 = gh 未回(同上边缘
    /// 情况),不臆造一个时间。
    pub pushed_at: String,
}

fn spawn_err(e: std::io::Error) -> GithubError {
    if e.kind() == std::io::ErrorKind::NotFound {
        GithubError::NotInstalled
    } else {
        GithubError::Command(e.to_string())
    }
}

fn stderr_text(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

async fn current_login() -> Result<String, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args(["api", "user", "--jq", ".login"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Mint a brand-new GitHub repo under the authenticated user's account and
/// clone it into `dest_root/<slug>`, then make the same first commit
/// `provision_git_workspace` makes locally (so `is_owned_workspace` correctly
/// reports this repo as workbench-owned) and push it.
pub async fn create_repo(
    slug: &str,
    private: bool,
    dest_root: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<GithubRepoRef, GithubError> {
    let owner = current_login().await?;
    let vis_flag = if private { "--private" } else { "--public" };
    let output = crate::win_cmd::tokio_cmd("gh")
        .current_dir(dest_root)
        .args(["repo", "create", slug, vis_flag, "--clone"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let dir = dest_root.join(slug);
    commit_initial(&dir, readme_title, readme_body)
        .await
        .map_err(|e| GithubError::Command(format!("初始提交失败:{e}")))?;
    git_in(&dir, &["push", "-u", "origin", "HEAD"])
        .await
        .map_err(|e| GithubError::Command(format!("推送失败:{e}")))?;
    Ok(GithubRepoRef {
        owner: owner.clone(),
        repo: slug.to_string(),
        html_url: format!("https://github.com/{owner}/{slug}"),
        private,
    })
}

/// 落地收拢推送(plan/13 D1,#31 记录的缺口):`create_repo` 只推首
/// commit,创建流途中的章程/组件标准等提交一直停在本地。
/// `CompleteCreation` 落地时调这里把 HEAD 一次推齐;无新提交时 push
/// 天然 no-op,幂等可重跑。
pub async fn push_head(dir: &Path) -> Result<(), GithubError> {
    git_in(dir, &["push", "origin", "HEAD"])
        .await
        .map_err(|e| GithubError::Command(format!("推送失败:{e}")))
}

/// plan/13 D12: github-repo 连接器的真探针——`gh repo view` 一次,回
/// 可见性与最近推送时间。探不通就如实报错,绝不伪造"已同步"。
pub async fn probe_repo(owner_repo: &str) -> Result<String, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "repo",
            "view",
            owner_repo,
            "--json",
            "nameWithOwner,isPrivate,pushedAt",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let v: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| GithubError::Command(format!("解析 gh repo view 输出失败:{e}")))?;
    let name = v["nameWithOwner"].as_str().unwrap_or(owner_repo);
    let vis = if v["isPrivate"].as_bool().unwrap_or(true) {
        "private"
    } else {
        "public"
    };
    let pushed = v["pushedAt"].as_str().unwrap_or("未知");
    Ok(format!("{name} · {vis} · 最近推送 {pushed}"))
}

// ─────────────── P1 · 存量项目接仓 (loop-buddy↔aihot 接线 spec) ───────────────
//
// `Command::AttachRepo` 给「绑定本地目录」建的项目补上 GitHub 远端。这里只管
// 本地 `origin` 这一侧的真实读写;探活(`probe_repo`)、写 `remote_path`、
// 补 connector 都是 bw-app 的编排职责,不下沉到这层。

/// 读一个工作区的 `origin` 远端 URL。`Ok(None)` = 没配 `origin`(绑定本地
/// 目录建的项目的常态——`git remote get-url origin` 以非零退出,这不是
/// bug);其余 git 失败照实报错,这是一次真读,不吞错误。
pub async fn origin_remote_url(workspace: &Path) -> Result<Option<String>, GithubError> {
    let output = crate::win_cmd::tokio_cmd("git")
        .current_dir(workspace)
        .args(["remote", "get-url", "origin"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        // "fatal: No such remote 'origin'" — 没配远端,不是错误。
        return Ok(None);
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(if url.is_empty() { None } else { Some(url) })
}

/// 一个已存在的 `origin` URL 是否已经指向 `owner/repo`?兼容 `gh repo
/// create/clone` 常写的 SSH(`git@github.com:owner/repo.git`)与 HTTPS
/// (`https://github.com/owner/repo[.git]`)两种写法,归一化后比较,免得
/// 同一个仓因协议不同被误判成「不符」。
pub fn remote_matches(url: &str, owner: &str, repo: &str) -> bool {
    let normalized = url
        .trim()
        .trim_end_matches(".git")
        .replace("git@github.com:", "github.com/")
        .replace("ssh://git@github.com/", "github.com/")
        .replace("https://github.com/", "github.com/")
        .replace("http://github.com/", "github.com/");
    normalized.eq_ignore_ascii_case(&format!("github.com/{owner}/{repo}"))
}

/// 结果:接线时给本地工作区新加了 `origin`,还是它本来就已经指对了仓。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteReconcile {
    Added,
    AlreadyMatched,
}

#[derive(Debug, thiserror::Error)]
pub enum RemoteReconcileError {
    /// 工作区已有 `origin`,但指向别的仓——**绝不覆盖**用户的 git 配置,
    /// 如实报错让人自己决定。
    #[error("工作区已有 origin({existing}),与目标仓 {owner}/{repo} 不符,拒绝覆盖")]
    Mismatch {
        existing: String,
        owner: String,
        repo: String,
    },
    #[error(transparent)]
    Github(#[from] GithubError),
}

/// P1 核心动作:给一个此前没有 `origin`(或 `origin` 已经指对)的工作区接上
/// `owner/repo`。空 → 真的 `git remote add`;已指对 → no-op 视为就绪;指向
/// 别的仓 → `Mismatch`,调用方据此中止,不静默改写。
pub async fn reconcile_local_remote(
    workspace: &Path,
    owner: &str,
    repo: &str,
) -> Result<RemoteReconcile, RemoteReconcileError> {
    match origin_remote_url(workspace).await? {
        None => {
            git_in(
                workspace,
                &[
                    "remote",
                    "add",
                    "origin",
                    &format!("git@github.com:{owner}/{repo}.git"),
                ],
            )
            .await
            .map_err(|e| git_err("添加 origin 失败", e))?;
            Ok(RemoteReconcile::Added)
        }
        Some(url) if remote_matches(&url, owner, repo) => Ok(RemoteReconcile::AlreadyMatched),
        Some(existing) => Err(RemoteReconcileError::Mismatch {
            existing,
            owner: owner.to_string(),
            repo: repo.to_string(),
        }),
    }
}

/// 当前检出的分支名(`git branch --show-current`)——detached HEAD 时为空,
/// 调用方把「空」当成「没有可推的分支」处理,不是硬错误、更不会瞎编一个
/// 分支名去推。
pub async fn current_branch(workspace: &Path) -> Result<String, GithubError> {
    let output = crate::win_cmd::tokio_cmd("git")
        .current_dir(workspace)
        .args(["branch", "--show-current"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// `push_local=true` 路径:把当前分支推到 `origin` 并建立 tracking
/// (`git push -u origin <branch>`)。
pub async fn push_current_branch(workspace: &Path, branch: &str) -> Result<(), GithubError> {
    git_in(workspace, &["push", "-u", "origin", branch])
        .await
        .map_err(|e| git_err("推送失败", e))
}

/// merge 后把本地工作区收拢回默认分支(plan/13 D5:merge 后同步指标正本
/// 需要读到 merge 进主干的 `.bw/metrics.toml`,而 run 结束后工作区还停在
/// `bw/issue-N` 活分支上)。fetch(尽力) → 解析 origin/HEAD(拿不到就依次试
/// main/master)→ checkout → `pull --ff-only`(尽力)。只 ff,绝不在这里制造
/// merge commit。fetch/pull 失败仍算成功——只要本地已回到默认分支,后续
/// issue worktree 就不会从 `bw/project-init` 开出;远端尚未拉齐时由下次
/// sync / 用户网络恢复补上。
pub async fn sync_default_branch(dir: &Path) -> Result<(), GithubError> {
    // Best-effort: no origin / offline must not leave callers stuck on a
    // config branch. Checkout of a local default branch is the hard requirement.
    let _ = git_in(dir, &["fetch", "origin"]).await;
    let head = crate::win_cmd::tokio_cmd("git")
        .current_dir(dir)
        .args(["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    let mut candidates: Vec<String> = Vec::new();
    if head.status.success() {
        if let Ok(s) = String::from_utf8(head.stdout) {
            // "origin/main" → "main"
            if let Some(b) = s.trim().strip_prefix("origin/") {
                candidates.push(b.to_string());
            }
        }
    }
    candidates.push("main".into());
    candidates.push("master".into());
    let mut last_err = String::new();
    for b in &candidates {
        match git_in(dir, &["checkout", b]).await {
            Ok(()) => {
                let _ = git_in(dir, &["pull", "--ff-only", "origin", b]).await;
                return Ok(());
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    Err(GithubError::Command(format!(
        "找不到可检出的默认分支(试过 {}):{last_err}",
        candidates.join("/")
    )))
}

/// Clone an already-existing GitHub repo the user picked into `dest`.
pub async fn clone_repo(
    owner: &str,
    repo: &str,
    dest: &Path,
) -> Result<GithubRepoRef, GithubError> {
    let owner_repo = format!("{owner}/{repo}");
    let output = crate::win_cmd::tokio_cmd("gh")
        .args(["repo", "clone", &owner_repo, &dest.to_string_lossy()])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let view = crate::win_cmd::tokio_cmd("gh")
        .args(["repo", "view", &owner_repo, "--json", "isPrivate"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    let private = if view.status.success() {
        serde_json::from_slice::<serde_json::Value>(&view.stdout)
            .ok()
            .and_then(|v| v.get("isPrivate").and_then(|b| b.as_bool()))
            .unwrap_or(false)
    } else {
        false
    };
    Ok(GithubRepoRef {
        owner: owner.to_string(),
        repo: repo.to_string(),
        html_url: format!("https://github.com/{owner_repo}"),
        private,
    })
}

/// One open issue on the remote (V2-②-I read-back). `number` is the platform
/// issue id (`gh` number / codehub `iid`); `body` may be empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteOpenIssue {
    pub number: u32,
    pub title: String,
    pub body: String,
}

/// V2-②-I: `gh issue list --state open --json number,title,body` — read-only.
/// Never creates. Cap 200 (gh default max per call); enough for Buddy boards.
pub async fn list_open_issues(owner_repo: &str) -> Result<Vec<RemoteOpenIssue>, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "issue",
            "list",
            "--repo",
            owner_repo,
            "--state",
            "open",
            "--limit",
            "200",
            "--json",
            "number,title,body",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    parse_gh_open_issues(&output.stdout)
}

#[derive(serde::Deserialize)]
struct GhIssueJson {
    number: u32,
    title: String,
    #[serde(default)]
    body: String,
}

fn parse_gh_open_issues(bytes: &[u8]) -> Result<Vec<RemoteOpenIssue>, GithubError> {
    let rows: Vec<GhIssueJson> = serde_json::from_slice(bytes)
        .map_err(|e| GithubError::Command(format!("无法解析 gh issue list JSON:{e}")))?;
    Ok(rows
        .into_iter()
        .map(|r| RemoteOpenIssue {
            number: r.number,
            title: r.title,
            body: r.body,
        })
        .collect())
}

/// C4 · issue 身份映射: 经 `gh issue create` 真开一个 GitHub issue,返回
/// `gh` 铸造的 issue 号(这就是这张 Issue 的跨系统身份)。`gh issue create`
/// 成功时把新 issue 的 URL 打到 stdout(如
/// `https://github.com/owner/repo/issues/42`),号即 URL 末段。只做 create
/// ——close/PR 是另一票的事。
pub async fn create_issue(owner_repo: &str, title: &str, body: &str) -> Result<u32, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "issue", "create", "--repo", owner_repo, "--title", title, "--body", body,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    url.rsplit('/')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| GithubError::Command(format!("无法从 gh 输出解析 issue 号:{url:?}")))
}

// ─────────────────────── C5 · PR 验收环 (plan/13 D3) ───────────────────────
//
// 三件套 + 收尾:提 PR / 查 PR 状态 / merge PR,外加 merge 后的 issue 补关。
// 关键纪律:**执行器只提 PR、永不 merge**——`open_pr` 在执行器路径里被调用,
// `merge_pr` 只从 bw-app 的人手命令(MergeIssuePr)里调用,两者物理隔离。
// 验收=人 merge;issue 关闭是 merge 的后果(PR body 的 `Closes #<n>` 关键字让
// GitHub 自动关单,`merge_pr` 后再幂等核对补关)。BW 绝不反向改写 GitHub:检测
// 到的漂移(PR 已被网页 merge 等)只反映、不 reopen、不改写远端。

/// The work branch a run's changes live on for a given GitHub issue —
/// `bw/issue-<github_number>`. One deterministic branch per Issue so a retry
/// re-uses the same branch (and the same PR), never fans out.
pub fn issue_branch(github_number: u32) -> String {
    format!("bw/issue-{github_number}")
}

/// The branch `.bw/project.toml` rides on when the first Buddy to adopt an
/// existing repo writes it via PR (§7) — `bw/project-init`. There is no issue
/// number (project.toml is a config file, not an Issue), so this branch is
/// named after the action, not an issue. One deterministic branch so a retry
/// re-uses the same branch (and the same PR).
pub const PROJECT_INIT_BRANCH: &str = "bw/project-init";

fn git_err(prefix: &str, e: crate::workspace::ProvisionError) -> GithubError {
    GithubError::Command(format!("{prefix}:{e}"))
}

/// P7-7A: distinguishes a brand-new PR from one `open_pr` merely *adopted*
/// because the executor already opened it itself (executors are allowed
/// `gh pr create` — only `gh pr merge` is disallowed,`claude_cli.rs`'s
/// `--disallowedTools`). Both carry a **real, read-back** PR number — the
/// `Adopted` case is never guessed or constructed, it comes from a fresh
/// `gh pr view` on the branch. Callers that don't care about the distinction
/// can match `Created(n) | Adopted(n) => n`; `run_issue_now` uses it to emit
/// an honest toast ("已提 PR" vs "已认领队友提的 PR") instead of blurring the
/// two into one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrOpened {
    Created(u32),
    Adopted(u32),
}

impl PrOpened {
    pub fn number(self) -> u32 {
        match self {
            PrOpened::Created(n) | PrOpened::Adopted(n) => n,
        }
    }
}

/// 提 PR (plan/13 D3): commit whatever the run produced on the Issue branch,
/// push it, and open a pull request whose body carries `Closes #<github_number>`
/// so a later human merge auto-closes the Issue — one action验收. Returns the
/// PR number `gh` minted (parsed from the PR URL it prints, same idiom as
/// `create_issue`). Every step is fallible and the caller treats any failure as
/// "提 PR 失败不炸 run": the run's own accounting stands, `pr_number` stays 0,
/// the Issue is retryable. **Never merges** — this only opens the PR.
///
/// P7-7A (真实践行暴露的缺口): an executor is allowed to run `gh pr create`
/// itself (only `gh pr merge` is denied) — a teammate that does this before
/// BW calls `open_pr` leaves `gh pr create` here failing with "a pull request
/// ... already exists". That failure is not a real error, it's **提 PR 幂
/// 等**: the PR the run needed already exists, just opened by someone else.
/// Only that one failure shape is adopted — anything else (no permission, no
/// network, branch never pushed, …) still returns `Err` unchanged; this must
/// never turn into "swallow every failure and pretend success" (that would be
/// fabricating success, the thing this whole codebase refuses to do). The
/// adopted number is **read back for real** via `gh pr view <branch>`, never
/// parsed out of the error text and never guessed.
pub async fn open_pr(
    workspace: &Path,
    github_number: u32,
    title: &str,
) -> Result<PrOpened, GithubError> {
    let branch = issue_branch(github_number);
    // Stage + commit + push the run's edits on the Issue branch. The executor
    // may have left a dirty tree (the common `acceptEdits` case) or committed
    // itself; either way this makes the branch carry a real, mergeable diff.
    // "nothing to commit" is the idempotent already-committed case, not a
    // failure (F5 logic lives in `stage_commit_push`, shared with codehub).
    stage_commit_push(workspace, &branch, github_number, title)
        .await
        .map_err(|e| git_err("暂存/提交/推送活分支失败", e))?;
    // `Closes #<n>` in the body is what auto-closes the Issue on merge
    // (D3: issue 关闭是 merge 的后果).
    let body = format!(
        "BW 执行器为 Issue #{github_number} 提交的改动,等待人工 merge 验收。\n\nCloses #{github_number}"
    );
    create_pr_on_branch(workspace, &branch, title, &body).await
}

/// 在**已经推上去的分支**上开一个 PR —— `gh pr create` 就这一处实现。
///
/// 三个调用方各自准备分支的方式不同([`open_pr`] 走 `stage_commit_push`、
/// [`open_project_init_pr`] 先 checkout 再提交、V4 的规范铺底自己在 worktree
/// 里只提交点名的那几个文件),但「开 PR」这一步是同一段代码,所以只留一份。
///
/// `body` 由调用方给:带不带 `Closes #<n>` 是调用方的决定 —— **不能默认带**。
/// V4 的活是本机号,和远端 issue 号没有对应关系,拼一句 `Closes #3` 会去关掉
/// 那个仓里毫不相干的第 3 号 issue。
///
/// gh 从 `workspace` 的 origin 远端推断目标仓与默认基线分支。
pub async fn create_pr_on_branch(
    workspace: &Path,
    branch: &str,
    title: &str,
    body: &str,
) -> Result<PrOpened, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .current_dir(workspace)
        .args([
            "pr", "create", "--head", branch, "--title", title, "--body", body,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        let stderr = stderr_text(&output);
        // 只认领这一种失败形状:gh 的措辞在 2.x 上稳定为
        // `a pull request for branch "<head>" into branch "<base>" already
        // exists:`(gh 不做本地化,英文文案是唯一形态)。命中就说明 PR 真的
        // 已经存在——多半是执行器自己在 run 里跑了 `gh pr create`(允许:
        // 禁的只有 `gh pr merge`)。任何其它失败(无权限/网络/分支没推……)
        // 原样 `Err`,绝不吞掉当"提 PR 幂等"处理。
        if stderr.contains("already exists") {
            match adopt_existing_pr(workspace, branch).await {
                Ok(pr) => return Ok(PrOpened::Adopted(pr)),
                // 读回也失败(罕见的竞态/权限边缘情况)——如实把原始
                // create 失败信息交回,不假装认领成功。
                Err(_) => return Err(GithubError::Command(stderr)),
            }
        }
        return Err(GithubError::Command(stderr));
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    url.rsplit('/')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .map(PrOpened::Created)
        .ok_or_else(|| GithubError::Command(format!("无法从 gh 输出解析 PR 号:{url:?}")))
}

/// P7-7A helper: read back the **real** PR number already open on `branch` —
/// called only from `open_pr`'s "already exists" adoption path. `gh pr view
/// <branch>` resolves the repo from `workspace`'s origin remote, same as the
/// `gh pr create` call that just failed, so this asks the same repo the same
/// question `gh` itself just answered in its error message — but by an
/// independent read, not by parsing that error text.
async fn adopt_existing_pr(workspace: &Path, branch: &str) -> Result<u32, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .current_dir(workspace)
        .args(["pr", "view", branch, "--json", "number", "--jq", ".number"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u32>()
        .map_err(|_| GithubError::Command("认领已存在 PR 时读回号码失败".to_string()))
}

/// merge PR (plan/13 D3): the **human** verification action — merges the PR,
/// which (via `Closes #<n>`) closes the Issue. Called only from bw-app's
/// `MergeIssuePr` command, never from any executor/run path. Squash-merge keeps
/// the base branch history one-commit-per-Issue.
pub async fn merge_pr(owner_repo: &str, pr_number: u32) -> Result<(), GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "pr",
            "merge",
            &pr_number.to_string(),
            "--repo",
            owner_repo,
            "--squash",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    Ok(())
}

/// V2-② Phase A (§7): open a PR for `.bw/project.toml` on the
/// [`PROJECT_INIT_BRANCH`] branch — the first Buddy to adopt an existing repo
/// writes the project intent as a config PR (not an Issue PR) and Buddy
/// auto-merges it. Parallels [`open_pr`] but without an issue number: the
/// branch is `bw/project-init` (not `bw/issue-<n>`), the commit message is
/// `chore: …` (not `issue #<n>: …`), and the PR body carries no `Closes`
/// keyword (there's no Issue to close). Returns the PR number `gh` minted.
/// **Never merges** — the caller (bw-app's creation flow) auto-merges via
/// [`merge_pr`] on success, or surfaces a tip on failure.
pub async fn open_project_init_pr(workspace: &Path, title: &str) -> Result<PrOpened, GithubError> {
    let branch = PROJECT_INIT_BRANCH;
    // Checkout the branch, creating it at HEAD the first time, re-using it
    // on a retry (same idempotent semantics as `checkout_issue_branch`).
    if git_in(workspace, &["checkout", "-b", branch])
        .await
        .is_err()
    {
        git_in(workspace, &["checkout", branch])
            .await
            .map_err(|e| git_err("切到 project-init 分支失败", e))?;
    }
    stage_commit_push_msg(
        workspace,
        branch,
        "chore: project intent (.bw/project.toml)",
    )
    .await
    .map_err(|e| git_err("暂存/提交/推送 project-init 分支失败", e))?;
    let body = "BW 创建流写入的项目意图正本,自动合入落仓(配置文件,非 Issue)。";
    create_pr_on_branch(workspace, branch, title, body).await
}

/// `gh issue view --json state` → `OPEN` / `CLOSED`. Lets `MergeIssuePr` verify
/// the `Closes #<n>` keyword actually closed the Issue and补关 idempotently if
/// GitHub didn't (rare, but honest belt-and-suspenders).
pub async fn issue_state(owner_repo: &str, github_number: u32) -> Result<String, GithubError> {
    gh_json_field(&[
        "issue",
        "view",
        &github_number.to_string(),
        "--repo",
        owner_repo,
        "--json",
        "state",
        "--jq",
        ".state",
    ])
    .await
}

/// P7-7B (plan/13 用户故事 22, D22): read-only probe for whether an Issue's
/// deterministic work branch (`issue_branch`) currently has an OPEN PR
/// against it. **现役调用方**(2026-08-17 起唯一一个):`Remote::open_mr_for_branch`
/// ← bw-app `poll_interactive_inreview`(调度器每次 tick 轮询「评审中」候选,
/// 队友自己 `gh pr create` 的 PR 就靠这条探针发现;最初为它而写的
/// `RefreshIssues` 漂移采集器已随 2026-08-17 减负重构删除——别因为 grep 不到
/// `RefreshIssues` 就把本函数当死码)。Executors are allowed `gh pr create`;
/// only `gh pr merge` is disallowed. Addressed purely via `--repo`, unlike `open_pr`/`adopt_existing_pr`
/// which run from inside a checked-out workspace — this never touches the
/// local git state, so it's safe to call for an issue whose branch the
/// caller hasn't (and may never) check out. `Ok(None)` = no open PR for that
/// branch — the honest "nothing to review yet" answer, not an error.
pub async fn open_pr_for_branch(
    owner_repo: &str,
    head_branch: &str,
) -> Result<Option<u32>, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "pr",
            "list",
            "--repo",
            owner_repo,
            "--head",
            head_branch,
            "--state",
            "open",
            "--json",
            "number",
            "--jq",
            // `// empty` (jq idiom) prints nothing at all when there's no
            // open PR, instead of the literal text "null" — collapses
            // cleanly to `Ok(None)` below without a separate null check.
            ".[0].number // empty",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<u32>()
        .map(Some)
        .map_err(|_| GithubError::Command(format!("无法解析 PR 号:{text:?}")))
}

/// Idempotent补关: close the GitHub issue directly. Only called after a merge
/// when `issue_state` still reads `OPEN` (the `Closes` keyword should have done
/// it). `gh issue close` on an already-closed issue is a no-op success.
pub async fn close_issue(owner_repo: &str, github_number: u32) -> Result<(), GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "issue",
            "close",
            &github_number.to_string(),
            "--repo",
            owner_repo,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    Ok(())
}

/// Run a read-only `gh ... --json ... --jq ...` and return the trimmed stdout.
async fn gh_json_field(args: &[&str]) -> Result<String, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// C16: `defaultBranchRef` comes back as a nested object (`{"name":"main"}`),
/// not a bare string — `gh repo list --json defaultBranchRef` shape per its
/// own JSON FIELDS reference (`gh repo list --help`). An empty repo with no
/// commits has no default branch ref at all, hence `Option`.
#[derive(serde::Deserialize)]
struct DefaultBranchRefJson {
    name: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoJson {
    name_with_owner: String,
    is_private: bool,
    // C16: `description` is nullable in the underlying GraphQL schema (no
    // description set ⇒ JSON `null`, not `""`) — `Option` here, flattened to
    // `""` at the call site (empty-string 是"没填",不是"没读到").
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    default_branch_ref: Option<DefaultBranchRefJson>,
    #[serde(default)]
    pushed_at: Option<String>,
}

/// List repos owned by the authenticated user — the "接入已有仓" picker's
/// data source. Read-only, no local filesystem side effects.
///
/// C16(plan/14 规范条 4): `--json` 字段集从 `nameWithOwner,isPrivate,updatedAt`
/// 扩到 `nameWithOwner,isPrivate,description,defaultBranchRef,pushedAt` ——
/// 字段名核实自 `gh repo list --help`(`gh` 2.95.0)的 JSON FIELDS 清单,均在
/// 表中:`description`、`defaultBranchRef`、`isPrivate`、`pushedAt`、
/// `nameWithOwner`。回显真实 metadata(描述/可见性/默认分支/最近推送),不再
/// 只是干巴巴一个仓名。
pub async fn list_repos(limit: u32) -> Result<Vec<GithubRepoSummary>, GithubError> {
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "repo",
            "list",
            "--json",
            "nameWithOwner,isPrivate,description,defaultBranchRef,pushedAt",
            "--limit",
            &limit.to_string(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let rows: Vec<RepoJson> = serde_json::from_slice(&output.stdout)
        .map_err(|e| GithubError::Command(format!("解析 gh repo list 输出失败:{e}")))?;
    Ok(rows
        .into_iter()
        .filter_map(|r| {
            let (owner, repo) = r.name_with_owner.split_once('/')?;
            Some(GithubRepoSummary {
                owner: owner.to_string(),
                repo: repo.to_string(),
                private: r.is_private,
                description: r.description.unwrap_or_default(),
                default_branch: r.default_branch_ref.map(|b| b.name).unwrap_or_default(),
                pushed_at: r.pushed_at.unwrap_or_default(),
            })
        })
        .collect())
}

// ─────────────────────── C7 · 采集器 (plan/13 D7) ───────────────────────
//
// One `.bw/metrics.toml` `kind = "github"` query → a real count, pulled from
// GitHub's search API via `gh`. Read-only, zero repo side effects. The caller
// (bw-app) turns the count into an append-only observation *only when it
// changed* (change-guard) and never fabricates a value on failure — an errored
// query writes nothing, letting the metric's signal degrade honestly rather
// than flash a fake zero.

/// Expand BW placeholders in a github collect query against a project's
/// `owner/repo` remote and a reference date:
/// - `{owner}` / `{repo}` — from the remote (`{owner}/{repo}` therefore also
///   expands correctly).
/// - `@{<N>d}` — the ISO date `N` days before `today`, a rolling "past N days"
///   window (e.g. `merged:>=@{7d}` on 2026-07-23 → `merged:>=2026-07-16`).
///
/// An unrecognized `@{…}` macro is left literal (a content problem for the
/// 找指标/绑数据 skills, not a hard error here) — the scan advances past it so
/// later valid macros still expand.
fn expand_query(query: &str, remote: &str, today: Date) -> String {
    let (owner, repo) = remote.split_once('/').unwrap_or((remote, ""));
    let mut out = query.replace("{owner}", owner).replace("{repo}", repo);
    let mut search_from = 0;
    while let Some(rel) = out[search_from..].find("@{") {
        let start = search_from + rel;
        let after = start + 2;
        let Some(end_rel) = out[after..].find('}') else {
            break; // unterminated macro — stop, leave the rest literal
        };
        let end = after + end_rel; // index of the closing '}'
        let token = &out[after..end];
        match days_ago_iso(token, today) {
            Some(date) => {
                out.replace_range(start..=end, &date);
                search_from = start + date.len();
            }
            None => {
                search_from = end + 1; // skip an unknown macro, keep scanning
            }
        }
    }
    out
}

/// `"7d"` + a reference date → the ISO date 7 days earlier. `None` for any
/// token that isn't `<digits>d`.
fn days_ago_iso(token: &str, today: Date) -> Option<String> {
    let n: i64 = token.strip_suffix('d')?.parse().ok()?;
    let date = today.checked_sub(time::Duration::days(n))?;
    Some(format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    ))
}

/// V2-② Intent UX (§6.2): fetch `.bw/project.toml` via `gh api` raw contents
/// without cloning. Same contract as [`crate::codehub::fetch_project_toml`]:
/// `Ok(None)` = absent → first-comer; `Err` = soft-fail (stay editable).
pub async fn fetch_project_toml(
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<Option<crate::project_file::ProjectFile>, GithubError> {
    let git_ref = if git_ref.trim().is_empty() {
        "main"
    } else {
        git_ref.trim()
    };
    let endpoint = format!(
        "repos/{owner}/{repo}/contents/{}?ref={git_ref}",
        crate::project_file::PROJECT_FILE_REL_PATH
    );
    let output = crate::win_cmd::tokio_cmd("gh")
        .args(["api", "-H", "Accept: application/vnd.github.raw", &endpoint])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        let err = stderr_text(&output);
        let lower = err.to_lowercase();
        // **分支不存在也是 404**,但它和「这个仓里没有这份文件」是两件事:
        // 前者是「没查成」(多半是默认分支不叫 main),后者才是「没接管过」。
        // 一起当成 Ok(None) 的话,一个默认分支叫 master 的仓会被报成
        // 「还没被 buddy 接管过」,人照着填一遍就会盖掉仓里真正的名片。
        if lower.contains("no commit found for the ref") {
            return Err(GithubError::Command(err));
        }
        if lower.contains("404") || lower.contains("not found") {
            return Ok(None);
        }
        return Err(GithubError::Command(err));
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    crate::project_file::parse(raw.trim())
        .map(Some)
        .map_err(|e| GithubError::Command(e.to_string()))
}

/// C7 · 采集器: run one `kind = "github"` metric query as a real count.
/// Expands BW placeholders against `remote` (`owner/repo`) + `today`, then asks
/// 某个时间窗口内**合入**的 PR 数。
///
/// 窗口由调用方给 —— 这就是「能采到今天的数,就能采到过去任意一周的数」那条
/// 判据的落点:同一个函数换个窗口,过去第八周的值照样算得出来,不需要谁提前
/// 把它存下来。
///
/// `since` / `until` 都是 `YYYY-MM-DD`,而且是**闭区间**(GitHub 的
/// `merged:a..b` 含两端)。注意别直接把 ISO 周的左闭右开边界丢进来 —— 那会把
/// 下周一那天的 PR 也算进这一周。
pub async fn merged_pr_count(
    owner_repo: &str,
    since: &str,
    until: &str,
) -> Result<u32, GithubError> {
    let q = format!("repo:{owner_repo} is:pr is:merged merged:{since}..{until}");
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "api",
            "-X",
            "GET",
            "search/issues",
            "-f",
            &format!("q={q}"),
            "--jq",
            ".total_count",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    text.parse::<u32>()
        .map_err(|_| GithubError::Command(format!("无法解析 gh 计数输出:{text:?}")))
}

/// GitHub's search API for the total number of matches via `gh`. Uses the
/// `search/issues` endpoint — it covers both issues and PRs (a query's own
/// `is:pr` / `is:issue` narrows it); releases and other facets are out of v1
/// scope. Read-only. Returns the count `gh` reported; the caller decides
/// whether that count is a *new fact* worth recording.
pub async fn collect_github_count(
    remote: &str,
    query: &str,
    today: Date,
) -> Result<u64, GithubError> {
    let q = expand_query(query, remote, today);
    let output = crate::win_cmd::tokio_cmd("gh")
        .args([
            "api",
            "-X",
            "GET",
            "search/issues",
            "-f",
            &format!("q={q}"),
            "--jq",
            ".total_count",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    text.parse::<u64>()
        .map_err(|_| GithubError::Command(format!("无法解析 gh 计数输出:{text:?}")))
}

#[cfg(test)]
mod list_open_issues_parse_tests {
    use super::*;

    #[test]
    fn parses_gh_issue_list_json() {
        let raw = r#"[
          {"number":3,"title":"find-metrics","body":"skill note"},
          {"number":7,"title":"manual","body":""}
        ]"#;
        let got = parse_gh_open_issues(raw.as_bytes()).expect("parse");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].number, 3);
        assert_eq!(got[0].title, "find-metrics");
        assert_eq!(got[1].body, "");
    }
}
