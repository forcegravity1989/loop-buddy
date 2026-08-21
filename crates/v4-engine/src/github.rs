//! GitHub shell-out — mints or adopts a GitHub repo via the `gh` CLI, same
//! subprocess pattern `workspace.rs` uses for local git. Relies entirely on
//! the user's own `gh auth login` on this machine; no token handling here.

/// 项目名片在仓里的相对路径。底座只拿它拼远端 API 的 URL,**不解析这份文件**
/// —— 解析是上层的事(`bw-v4` 有自己的一份)。
pub const PROJECT_FILE_REL_PATH: &str = ".bw/project.toml";
use std::path::Path;
use std::process::Stdio;

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

/// V2-② Intent UX (§6.2): fetch `.bw/project.toml` via `gh api` raw contents
/// without cloning. Same contract as [`crate::codehub::fetch_project_toml`]:
/// `Ok(None)` = absent → first-comer; `Err` = soft-fail (stay editable).
pub async fn fetch_project_toml(
    owner: &str,
    repo: &str,
    git_ref: &str,
) -> Result<Option<String>, GithubError> {
    let git_ref = if git_ref.trim().is_empty() {
        "main"
    } else {
        git_ref.trim()
    };
    let endpoint = format!(
        "repos/{owner}/{repo}/contents/{}?ref={git_ref}",
        PROJECT_FILE_REL_PATH
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
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_string(),
    ))
}

// ── 走势图要的两份远端流水 ──────────────────────────────
//
// 都是**一趟拉全、把原始时刻交给上层**,不在这里按周查、也不在这里算数。
// 原先那条按周发搜索查询的路已经删掉,两个理由:
//
// 1. 画四周就要发四次、画八周就八次,而搜索接口本身不稳 —— 真机上撞到过
//    `Get https://api.github.com/search/issues…: EOF`,四条线里断一条。
// 2. 「issue 未处理数」这种存量值,本来就得拿全量流水才算得出「某周末还开着
//    几张」,按周查根本查不出来。
//
// 一趟拉全之后,要几周就在上层分几个桶,采多少和画多少永远一致。

/// 一趟能拉回来的最大条数。够不够由调用方自己判:**拿回来的条数正好等于它,
/// 就说明可能被截断了**,那时候该如实说「没查全」,不是端出一个少算的数。
pub const LIST_CAP: u32 = 1000;

/// 已合并 PR 的合并时刻,新的在前。RFC3339 原文,**不在这一层解析时间** ——
/// 底座只负责起进程、读 JSON,按周分桶是上层的事。
///
/// 复算:`gh pr list -R <仓> --state merged --json mergedAt --limit 1000`
pub async fn merged_pr_times(owner_repo: &str) -> Result<Vec<String>, GithubError> {
    let rows: Vec<GhMergedPrJson> =
        gh_list_json(owner_repo, &["pr", "list", "--state", "merged"], "mergedAt").await?;
    Ok(rows.into_iter().filter_map(|r| r.merged_at).collect())
}

/// 每一张 issue 的建立时刻与关闭时刻(没关就是 `None`),新的在前。
/// **不含 PR** —— `gh issue list` 本身就把 PR 排除在外。
///
/// 复算:`gh issue list -R <仓> --state all --json createdAt,closedAt --limit 1000`
pub async fn issue_times(owner_repo: &str) -> Result<Vec<(String, Option<String>)>, GithubError> {
    let rows: Vec<GhIssueTimeJson> = gh_list_json(
        owner_repo,
        &["issue", "list", "--state", "all"],
        "createdAt,closedAt",
    )
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.created_at, r.closed_at))
        .collect())
}

#[derive(serde::Deserialize)]
struct GhMergedPrJson {
    #[serde(rename = "mergedAt")]
    merged_at: Option<String>,
}

#[derive(serde::Deserialize)]
struct GhIssueTimeJson {
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "closedAt")]
    closed_at: Option<String>,
}

/// `gh <子命令…> --repo <仓> --limit <上限> --json <字段>` 的公共壳。
async fn gh_list_json<T: serde::de::DeserializeOwned>(
    owner_repo: &str,
    subcommand: &[&str],
    fields: &str,
) -> Result<Vec<T>, GithubError> {
    let cap = LIST_CAP.to_string();
    let mut args: Vec<&str> = subcommand.to_vec();
    args.extend_from_slice(&["--repo", owner_repo, "--limit", &cap, "--json", fields]);
    let output = crate::win_cmd::tokio_cmd("gh")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !output.status.success() {
        return Err(GithubError::Command(stderr_text(&output)));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|e| GithubError::Command(format!("无法解析 gh {} JSON:{e}", subcommand.join(" "))))
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
