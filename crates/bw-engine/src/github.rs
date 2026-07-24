//! GitHub shell-out — mints or adopts a GitHub repo via the `gh` CLI, same
//! subprocess pattern `workspace.rs` uses for local git. Relies entirely on
//! the user's own `gh auth login` on this machine; no token handling here.

use crate::workspace::{commit_initial, git_in};
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
    let output = tokio::process::Command::new("gh")
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
    let output = tokio::process::Command::new("gh")
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
    let output = tokio::process::Command::new("gh")
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

/// merge 后把本地工作区收拢回默认分支(plan/13 D5:merge 后同步指标正本
/// 需要读到 merge 进主干的 `.bw/metrics.toml`,而 run 结束后工作区还停在
/// `bw/issue-N` 活分支上)。fetch → 解析 origin/HEAD(拿不到就依次试
/// main/master)→ checkout → `pull --ff-only`。只 ff,绝不在这里制造
/// merge commit——工作区的主干只由远端事实前进。
pub async fn sync_default_branch(dir: &Path) -> Result<(), GithubError> {
    git_in(dir, &["fetch", "origin"])
        .await
        .map_err(|e| GithubError::Command(format!("fetch 失败:{e}")))?;
    let head = tokio::process::Command::new("git")
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
                git_in(dir, &["pull", "--ff-only", "origin", b])
                    .await
                    .map_err(|e| GithubError::Command(format!("pull {b} 失败:{e}")))?;
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
    let output = tokio::process::Command::new("gh")
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
    let view = tokio::process::Command::new("gh")
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

/// C4 · issue 身份映射: 经 `gh issue create` 真开一个 GitHub issue,返回
/// `gh` 铸造的 issue 号(这就是这张 Issue 的跨系统身份)。`gh issue create`
/// 成功时把新 issue 的 URL 打到 stdout(如
/// `https://github.com/owner/repo/issues/42`),号即 URL 末段。只做 create
/// ——close/PR 是另一票的事。
pub async fn create_issue(owner_repo: &str, title: &str, body: &str) -> Result<u32, GithubError> {
    let output = tokio::process::Command::new("gh")
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

fn git_err(prefix: &str, e: crate::workspace::ProvisionError) -> GithubError {
    GithubError::Command(format!("{prefix}:{e}"))
}

/// Quarantine a run's work onto the Issue's branch **before** the executor
/// touches anything (plan/13 D3: the executor must never advance the base
/// branch — only a human merge does). Checks out `bw/issue-<n>`, creating it
/// at the current HEAD the first time and re-using it on a retry. All of the
/// run's edits then land on this branch by construction, whatever the executor
/// does (dirty tree or its own commits), leaving the base branch untouched.
pub async fn checkout_issue_branch(
    workspace: &Path,
    github_number: u32,
) -> Result<String, GithubError> {
    let branch = issue_branch(github_number);
    // First run: create the branch at HEAD. Retry: the branch already exists,
    // so `-b` fails and we plain-checkout it (keeping any prior branch work).
    if git_in(workspace, &["checkout", "-b", &branch])
        .await
        .is_err()
    {
        git_in(workspace, &["checkout", &branch])
            .await
            .map_err(|e| git_err("切到活分支失败", e))?;
    }
    Ok(branch)
}

/// 提 PR (plan/13 D3): commit whatever the run produced on the Issue branch,
/// push it, and open a pull request whose body carries `Closes #<github_number>`
/// so a later human merge auto-closes the Issue — one action验收. Returns the
/// PR number `gh` minted (parsed from the PR URL it prints, same idiom as
/// `create_issue`). Every step is fallible and the caller treats any failure as
/// "提 PR 失败不炸 run": the run's own accounting stands, `pr_number` stays 0,
/// the Issue is retryable. **Never merges** — this only opens the PR.
pub async fn open_pr(
    workspace: &Path,
    github_number: u32,
    title: &str,
) -> Result<u32, GithubError> {
    let branch = issue_branch(github_number);
    // Stage + commit the run's edits. The executor may have left a dirty tree
    // (the common `acceptEdits` case) or committed itself; either way this
    // makes the branch carry a real, mergeable diff. "nothing to commit" is the
    // idempotent already-committed case, not a failure.
    git_in(workspace, &["add", "-A"])
        .await
        .map_err(|e| git_err("暂存活分支改动失败", e))?;
    let commit = tokio::process::Command::new("git")
        .current_dir(workspace)
        .args([
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            &format!("issue #{github_number}: {title}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        if !(stderr.contains("nothing to commit") || stderr.contains("no changes")) {
            return Err(GithubError::Command(format!(
                "提交活分支改动失败:{}",
                stderr.trim()
            )));
        }
    }
    git_in(workspace, &["push", "-u", "origin", &branch])
        .await
        .map_err(|e| git_err("推送活分支失败", e))?;
    // gh infers the base repo + default base branch from the origin remote in
    // `workspace`; `Closes #<n>` in the body is what auto-closes the Issue on
    // merge (D3: issue 关闭是 merge 的后果).
    let body = format!(
        "BW 执行器为 Issue #{github_number} 提交的改动,等待人工 merge 验收。\n\nCloses #{github_number}"
    );
    let output = tokio::process::Command::new("gh")
        .current_dir(workspace)
        .args([
            "pr", "create", "--head", &branch, "--title", title, "--body", &body,
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
        .ok_or_else(|| GithubError::Command(format!("无法从 gh 输出解析 PR 号:{url:?}")))
}

/// 查 PR 状态 (plan/13 D3; C7 之前本票自用): `gh pr view --json state` → the
/// raw state string (`OPEN` / `MERGED` / `CLOSED`). Read-only, no side effects
/// — used to detect drift (a PR merged on the web) without ever rewriting it.
pub async fn pr_state(owner_repo: &str, pr_number: u32) -> Result<String, GithubError> {
    gh_json_field(&[
        "pr",
        "view",
        &pr_number.to_string(),
        "--repo",
        owner_repo,
        "--json",
        "state",
        "--jq",
        ".state",
    ])
    .await
}

/// merge PR (plan/13 D3): the **human** verification action — merges the PR,
/// which (via `Closes #<n>`) closes the Issue. Called only from bw-app's
/// `MergeIssuePr` command, never from any executor/run path. Squash-merge keeps
/// the base branch history one-commit-per-Issue.
pub async fn merge_pr(owner_repo: &str, pr_number: u32) -> Result<(), GithubError> {
    let output = tokio::process::Command::new("gh")
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

/// Idempotent补关: close the GitHub issue directly. Only called after a merge
/// when `issue_state` still reads `OPEN` (the `Closes` keyword should have done
/// it). `gh issue close` on an already-closed issue is a no-op success.
pub async fn close_issue(owner_repo: &str, github_number: u32) -> Result<(), GithubError> {
    let output = tokio::process::Command::new("gh")
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
    let output = tokio::process::Command::new("gh")
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
    let output = tokio::process::Command::new("gh")
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

/// C7 · 采集器: run one `kind = "github"` metric query as a real count.
/// Expands BW placeholders against `remote` (`owner/repo`) + `today`, then asks
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
    let output = tokio::process::Command::new("gh")
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
