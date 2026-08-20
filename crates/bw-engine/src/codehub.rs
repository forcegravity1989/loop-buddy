//! CodeHub remote — shell-out `codehub-cli`(Go CLI,GitLab v4 同构封装,默认
//! JSON 输出,token 存 OS keyring)。与 [`crate::github`] 对称:无状态自由
//! 函数 + `tokio::process` shell-out,**零 HTTP 依赖**。token 不管(keyring
//! 里 `auth login` 存好),`host`/`path` 按 project 的 `(remote_host,
//! remote_path)` 显式带 `-H`/`-p`——绿/黄/内源三 host 同名 path 也不会混,
//! 因为每次调用都带显式 host,不存在「猜 host」的歧义分支,也没有对应的
//! 错误类型(`CodehubError` 只认 NotInstalled/Command/Parse)。
//!
//! 不进 `Executor` 体系——`codehub-cli` 是 VCS 远端 API 客户端(对标 `gh`),
//! 不是 agent 执行器(对标 `claude`)。两种 shell-out 模式别混。

use crate::workspace::{commit_initial, stage_commit_push_msg};
use std::path::Path;
use std::process::Stdio;
use time::Date;

#[derive(Debug, thiserror::Error)]
pub enum CodehubError {
    #[error("codehub-cli 未安装或不在 PATH")]
    NotInstalled,
    #[error("codehub-cli 失败:{0}")]
    Command(String),
    #[error("解析 codehub-cli 输出失败:{0}")]
    Parse(String),
}

/// A freshly-minted codehub repo's identity (the parity of
/// [`crate::github::GithubRepoRef`]). `path` = `path_with_namespace`
/// (e.g. `z30026659/my-service`); `host` = the API host alias
/// (`open`/`green`/`yellow`); `visibility` = `private`/`public`/`internal`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodehubRepoRef {
    pub host: String,
    pub path: String,
    pub visibility: String,
}

/// One row of `codehub-cli project list --mine` — the parity of
/// [`crate::github::GithubRepoSummary`] for the「接入已有仓」picker. `path`
/// = `path_with_namespace`; `pushed_at` is populated from codehub's
/// `last_activity_at` field (codehub has no `pushedAt`; same semantic —
/// the repo's last real activity timestamp).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodehubRepoSummary {
    pub path: String,
    pub visibility: String,
    pub default_branch: String,
    pub pushed_at: String,
    pub description: String,
}

fn spawn_err(e: std::io::Error) -> CodehubError {
    if e.kind() == std::io::ErrorKind::NotFound {
        CodehubError::NotInstalled
    } else {
        CodehubError::Command(e.to_string())
    }
}

fn stderr_text(o: &std::process::Output) -> String {
    String::from_utf8_lossy(&o.stderr).trim().to_string()
}

/// `codehub-cli project view -p <path> -H <host>` → 一行人话详情
/// (`path · 可见性[ · 已归档] · 最近活跃`)。Read-only,零仓副作用。
pub async fn probe(host: &str, path: &str) -> Result<String, CodehubError> {
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args(["project", "view", "-p", path, "-H", host])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| CodehubError::Parse(format!("project view JSON:{e}")))?;
    let pns = v["path_with_namespace"].as_str().unwrap_or(path);
    let vis = v["visibility"].as_str().unwrap_or("未知");
    let archived = v["archived"].as_bool().unwrap_or(false);
    let last = v["last_activity_at"].as_str().unwrap_or("未知");
    let arch = if archived { " · 已归档" } else { "" };
    Ok(format!("{pns} · {vis}{arch} · 最近活跃 {last}"))
}

/// `codehub-cli issue create -p <path> -H <host> --title … --description … --jq .iid`
/// → 新 issue 的 `iid`(这就是这张 Issue 的跨系统身份,对标 github 的 issue
/// 号)。调它会在 codehub 仓里**真开一个 issue**——只在创建流/trio 同步路径
/// 调,绝不自动。`body` 进 `--description`。
pub async fn create_issue(
    host: &str,
    path: &str,
    title: &str,
    body: &str,
) -> Result<u32, CodehubError> {
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "issue",
            "create",
            "-p",
            path,
            "-H",
            host,
            "--title",
            title,
            "--description",
            body,
            "--jq",
            ".iid",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    text.parse::<u32>()
        .map_err(|_| CodehubError::Parse(format!("无法解析 codehub issue iid:{text:?}")))
}

/// V2-②-I: `codehub-cli issue list --state opened -l 0` — read-only list of
/// open issues. Never creates. `-l 0` = 全量(同 [`collect_count`]).
pub async fn list_open_issues(
    host: &str,
    path: &str,
) -> Result<Vec<crate::github::RemoteOpenIssue>, CodehubError> {
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "issue",
            "list",
            "-p",
            path,
            "-H",
            host,
            "--state",
            "opened",
            "-l",
            "0",
            "--jq",
            r#"[.[] | {number: .iid, title: (.title // ""), body: (.description // "")}]"#,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    parse_codehub_open_issues(&out.stdout)
}

#[derive(serde::Deserialize)]
struct CodehubIssueJson {
    number: u32,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
}

fn parse_codehub_open_issues(
    bytes: &[u8],
) -> Result<Vec<crate::github::RemoteOpenIssue>, CodehubError> {
    let rows: Vec<CodehubIssueJson> = serde_json::from_slice(bytes)
        .map_err(|e| CodehubError::Parse(format!("无法解析 codehub issue list JSON:{e}")))?;
    Ok(rows
        .into_iter()
        .map(|r| crate::github::RemoteOpenIssue {
            number: r.number,
            title: r.title,
            body: r.body,
        })
        .collect())
}

/// `codehub-cli mr create` — the codehub parity of [`crate::github::open_pr`]:
/// stage + commit + push the run's edits on `bw/issue-<n>` (shared
/// [`crate::workspace::stage_commit_push`]), then open a merge request from
/// `bw/issue-<n>` → the project's default branch. The MR body carries
/// `Closes #<n>` so merging it auto-closes codehub issue `<n>` (GitLab
/// standard, same role as github's `Closes #<n>`); `--issue-nums issue<n>`
/// additionally links the MR↔issue (visible in codehub's "linked issues",
/// but the link alone does NOT auto-close — 2026-07-31 实测 issue 31/32/33
/// merge 后仍 opened,故靠 body 的 Closes). Returns the new MR's iid as
/// `PrOpened::Created`.
///
/// **No `Adopted` path** (unlike github): if an MR for the branch already
/// exists, `mr create` fails and the issue stays `InProgress` retryable —
/// honest, never fabricates success. The full buddy-run E2E uses a fresh
/// branch (`bw/issue-<n>` per issue), so a retry hits no pre-existing MR.
///
/// `target_branch` is resolved at runtime from `origin/HEAD` — **not**
/// hardcoded (maas is `master`, other projects `main`/`develop`). Smoke-
/// tested 2026-07-30: `git symbolic-ref refs/remotes/origin/HEAD` →
/// `refs/remotes/origin/master`; `--short` wrongly yields `origin/master`
/// (codehub-cli 404s on "origin/master"), so we strip the `refs/remotes/
/// origin/` prefix by hand. **Never merges** — only opens the MR.
pub async fn create_mr(
    host: &str,
    path: &str,
    workspace: &Path,
    issue_number: u32,
    title: &str,
) -> Result<crate::github::PrOpened, CodehubError> {
    let branch = format!("bw/issue-{issue_number}");
    crate::workspace::stage_commit_push(workspace, &branch, issue_number, title)
        .await
        .map_err(|e| CodehubError::Command(format!("git 准备失败:{e}")))?;
    let body = format!(
        "BW 执行器为 Issue #{issue_number} 提交的改动,等待人工 merge 验收。\n\nCloses #{issue_number}"
    );
    create_mr_on_branch(
        host,
        path,
        workspace,
        &branch,
        title,
        &body,
        Some(issue_number),
    )
    .await
}

/// 在**已经推上去的分支**上开一个 MR —— `codehub-cli mr create` 就这一处实现。
///
/// 调用方各自准备分支的方式不同([`create_mr`] 走 `stage_commit_push`、
/// [`create_project_init_mr`] 先 checkout 再提交、V4 的规范铺底自己在 worktree
/// 里只提交点名的那几个文件),开 MR 这一步是同一段代码,只留一份。
///
/// `body` 与 `issue_number` 都由调用方给:**带不带 `Closes` / `--issue-nums`
/// 是调用方的决定**。V4 的活是本机号,和远端 issue 号没有对应关系,自作主张
/// 挂上去会把那个仓里毫不相干的一号活关掉。
///
/// target-branch = 项目默认分支,运行时从 `origin/HEAD` 解析(maas 是
/// `master`,别的项目可能是 `main`/`develop`)。`symbolic-ref` 在没设
/// `origin/HEAD` 的克隆上会非零退出且 stdout 为空,这时退回 `master` —— 基线
/// 猜错会由 codehub-cli 的「branch not found」如实报出来,不会假装成功。
pub async fn create_mr_on_branch(
    host: &str,
    path: &str,
    workspace: &Path,
    branch: &str,
    title: &str,
    body: &str,
    issue_number: Option<u32>,
) -> Result<crate::github::PrOpened, CodehubError> {
    let tgt = crate::win_cmd::tokio_cmd("git")
        .current_dir(workspace)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    let target = String::from_utf8_lossy(&tgt.stdout)
        .trim()
        .strip_prefix("refs/remotes/origin/")
        .unwrap_or("master")
        .to_string();
    // P7-7A parity: adopt an already-open MR for this branch if one exists.
    // A prior run (or a manual smoke) may have opened an MR that BW didn't
    // record — `mr create` would then fail "already exists". Read the real
    // iid back (never guessed, same honesty as github's `adopt_existing_pr`).
    // 读回失败(网络/权限)就落到下面的 `mr create`,由它如实报错。
    if let Ok(Some(iid)) = open_mr_for_branch(host, path, branch).await {
        return Ok(crate::github::PrOpened::Adopted(iid));
    }
    let issue_nums = issue_number.map(|n| format!("issue{n}"));
    let mut args: Vec<&str> = vec![
        "mr",
        "create",
        "-p",
        path,
        "-H",
        host,
        "--source-branch",
        branch,
        "--target-branch",
        &target,
        "--title",
        title,
        "--description",
        body,
    ];
    if let Some(nums) = issue_nums.as_deref() {
        args.push("--issue-nums");
        args.push(nums);
    }
    args.push("--jq");
    args.push(".iid");
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let iid = text
        .parse::<u32>()
        .map_err(|_| CodehubError::Parse(format!("无法解析 codehub MR iid:{text:?}")))?;
    Ok(crate::github::PrOpened::Created(iid))
}

/// `codehub-cli mr list --source-branch <branch> --state opened --jq .[0].iid`
/// → the iid of the first open MR sourced from `branch`, or `None` if no
/// such MR exists. Read-only, zero repo side effects. Used by the InReview
/// detection poller (V1 Issue2 Phase2a) to detect an MR the agent opened
/// in-session — buddy doesn't create it (the agent does), buddy just reads
/// it back (读回为证, not agent self-report).
///
/// Mirrors the existing `create_mr` adoption path's `mr list` call exactly
/// (same flags, same `--jq .[0].iid` extraction). `Ok(None)` = no open MR
/// for that branch — the honest "nothing to review yet" answer, not an
/// error.
pub async fn open_mr_for_branch(
    host: &str,
    path: &str,
    branch: &str,
) -> Result<Option<u32>, CodehubError> {
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "mr",
            "list",
            "-p",
            path,
            "-H",
            host,
            "--source-branch",
            branch,
            "--state",
            "opened",
            "--jq",
            ".[0].iid",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() || text == "null" {
        return Ok(None);
    }
    text.parse::<u32>()
        .map(Some)
        .map_err(|_| CodehubError::Parse(format!("无法解析 codehub MR iid:{text:?}")))
}

/// `codehub-cli mr merge <iid> --squash -y` — the codehub parity of
/// [`crate::github::merge_pr`]: the human验收 action that integrates the
/// source branch into the target. Squash-merges (matches github's `--squash`).
/// The caller ([`crate::Remote::merge_mr`] ← `MergeIssuePr`) settles the
/// Issue `Done` on `Ok` via the existing `TransitionIssue` InReview→Done path;
/// on `Err` the Issue stays `InReview` retryable — never reverse-settled,
/// never fabricated. **Only ever called from `MergeIssuePr` (a human click),
/// never from any run/executor path** (plan/13 D3+D11).
pub async fn merge_mr(host: &str, path: &str, mr_iid: u32) -> Result<(), CodehubError> {
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "mr",
            "merge",
            &mr_iid.to_string(),
            "-p",
            path,
            "-H",
            host,
            "--squash",
            "-y",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    let merge_stderr = stderr_text(&out);
    if !out.status.success() {
        return Err(CodehubError::Command(merge_stderr));
    }
    // 读回为证: codehub-cli mr merge 的退出码靠不住——2026-07-31 实测 403
    // (protected-branch / 无 merge 权限)时,首次调用可退出 0(error 只打到
    // stderr)、MR 实际没合(state 仍 "opened")。只信「MR state 真变 merged」
    // 才算成功,绝不假 Done。复跑 mr view 拿 state(`--jq .merged` 对未合 MR
    // 返 null,故用 `.state`)。
    let verify = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "mr",
            "view",
            &mr_iid.to_string(),
            "-p",
            path,
            "-H",
            host,
            "--jq",
            ".state",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !verify.status.success() {
        return Err(CodehubError::Command(format!(
            "merge 后读回 MR 状态失败:{} | merge stderr: {merge_stderr}",
            stderr_text(&verify)
        )));
    }
    let state = String::from_utf8_lossy(&verify.stdout)
        .trim()
        .trim_matches('"')
        .to_string();
    if state == "merged" {
        Ok(())
    } else {
        Err(CodehubError::Command(format!(
            "merge 未生效(MR state={state},应 merged):{merge_stderr}"
        )))
    }
}

/// The branch `.bw/project.toml` rides on for a codehub project — mirrors
/// [`crate::github::PROJECT_INIT_BRANCH`].
const PROJECT_INIT_BRANCH: &str = "bw/project-init";

/// V2-② Phase A (§7): open an MR for `.bw/project.toml` on the
/// `bw/project-init` branch — the codehub parity of
/// [`crate::github::open_project_init_pr`]. Parallels [`create_mr`] but
/// without an issue number: the branch is `bw/project-init`, the commit
/// message is `chore: …`, and the MR body carries no `Closes` keyword。开 MR
/// 那一步和 [`create_mr`] 共用 [`create_mr_on_branch`],所以**同样会认领**
/// 分支上已经存在的那个 MR(读回真 iid,不猜)。**Never merges** —
/// the caller auto-merges via [`merge_mr`] on success, or surfaces a tip.
pub async fn create_project_init_mr(
    host: &str,
    path: &str,
    workspace: &Path,
    title: &str,
) -> Result<crate::github::PrOpened, CodehubError> {
    let branch = PROJECT_INIT_BRANCH;
    // Checkout the branch, creating it at HEAD the first time.
    let checkout = crate::win_cmd::tokio_cmd("git")
        .current_dir(workspace)
        .args(["checkout", "-b", branch])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !checkout.status.success() {
        crate::win_cmd::tokio_cmd("git")
            .current_dir(workspace)
            .args(["checkout", branch])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(spawn_err)?;
    }
    stage_commit_push_msg(
        workspace,
        branch,
        "chore: project intent (.bw/project.toml)",
    )
    .await
    .map_err(|e| CodehubError::Command(format!("git 准备失败:{e}")))?;
    let body = "BW 创建流写入的项目意图正本,自动合入落仓(配置文件,非 Issue)。";
    create_mr_on_branch(host, path, workspace, branch, title, body, None).await
}

/// `codehub-cli issue|mr list -p <path> -H <host> --state <state> -l 0 --jq length`
/// → 计数。codehub 无 GitHub 的 `search/issues total_count`,改用分页 list 的
/// `--jq length` 让 CLI 端计数(P3 实测:maas opened issues=6、merged MRs=9)。
///
/// **`-l 0` = 全量**(实测:codehub-cli 把 0 当"不限"取全部页,不是"取 0 条")。
/// 若 CLI 升级改了语义(0 被解读成 limit=0 → 计数恒 0 静默出错),要复核。
///
/// **查询口径**(P5 定稿):P3 只认最小词汇 `issues:<state>` / `mrs:<state>`
/// (state=opened|closed|merged|all)。复杂窗口(本周合入、按标签筛)留 P5
/// ——口径等 P4 用户导入 maas 看真实需求再定,骨架先搭。`today` 参数留给
/// P5 的日期窗口,这里 `_today` 暂不用。
pub async fn collect_count(
    host: &str,
    path: &str,
    query: &str,
    _today: Date,
) -> Result<u64, CodehubError> {
    let q = query.trim();
    let (kind, state) = q.split_once(':').ok_or_else(|| {
        CodehubError::Parse(format!(
            "codehub 查询需 'issues:<state>' 或 'mrs:<state>':{q:?}"
        ))
    })?;
    let list_cmd = match kind.trim() {
        "issues" => "issue",
        "mrs" => "mr",
        other => {
            return Err(CodehubError::Parse(format!(
                "未知 codehub 查询类型 {other:?}"
            )))
        }
    };
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            list_cmd,
            "list",
            "-p",
            path,
            "-H",
            host,
            "--state",
            state.trim(),
            "-l",
            "0",
            "--jq",
            "length",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    text.parse::<u64>()
        .map_err(|_| CodehubError::Parse(format!("无法解析 codehub 计数:{text:?}")))
}

/// Clone a codehub repo into `dest` over **SSH**. codehub 是局域网平台,HTTPS
/// `git clone` 经代理隧道被拦(实测 504);SSH 走 SSH key、不经代理、不要
/// token——开发者手敲 `git clone ssh://git@szv-open...:2222/.../maas.git` 就是
/// 这条路,常规、不特立独行。
///
/// SSH host(`szv-open.codehub.huawei.com:2222`)≠ API host
/// (`open.codehub.huawei.com`),不能从 (host,path) 手拼——从 `project view`
/// 的 `ssh_url_to_repo` 字段取准确地址(`--template` 出裸串;`--jq` 带引号且
/// 无 `-r`)。拿到后 raw `git clone`(codehub-cli 的 `repo clone` 对 SSH 输入
/// 是纯透传,无增益,故绕过)。
///
/// `GIT_SSH_COMMAND` 带 `accept-new`(首次 host key 自动接受写 known_hosts,
/// 免非交互卡死)+ `BatchMode=yes`(只走 key、不弹密码,密码会挂死非交互进程)。
pub async fn clone_repo(host: &str, path: &str, dest: &Path) -> Result<(), CodehubError> {
    // 1. 取 ssh_url(--template 出裸串,无引号)
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "project",
            "view",
            "-p",
            path,
            "-H",
            host,
            "--template",
            "{{.ssh_url_to_repo}}",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(format!(
            "取 ssh_url 失败(project view):{}",
            stderr_text(&out)
        )));
    }
    let ssh_url = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // --template 对 null 字段输出 "<no value>";空/非 ssh:// 不喂给 git
    // (老仓/无 SSH 配置的仓 ssh_url_to_repo 会是空)。
    if !ssh_url.starts_with("ssh://") {
        return Err(CodehubError::Command(format!(
            "codehub 仓无 SSH clone 地址(ssh_url_to_repo 为空或老仓?):{ssh_url:?}"
        )));
    }
    // 2. raw git clone(SSH key 认证,不经代理)
    let out = crate::win_cmd::tokio_cmd("git")
        .args(["clone", &ssh_url, &dest.to_string_lossy()])
        .env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new",
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    Ok(())
}

// ─────────────── V1 · 新建仓 + 列仓 (对仗 github.rs create_repo/list_repos) ───────────────
//
// `codehub-cli project create --name <name> --visibility <vis> --namespace-id <nsid>`
// + `project list --mine` — the codehub parity of `gh repo create`/`gh repo list`.
// Both are stateless shell-outs with `tokio::process`, same as every other fn in
// this module. Token goes via keyring (`auth login`), not passed here.

/// Resolve the current user's personal namespace ID via `codehub-cli project list
/// --mine --json namespace`. The user view doesn't expose `namespace.id` (only
/// `id`/`username`), so the only reliable source is a project the user already
/// owns under their personal namespace (`namespace.kind == "user"`). If
/// `namespace` param is non-empty, match `namespace.full_path` against it;
/// otherwise take the first `kind == "user"` namespace. Returns `None` when the
/// user has no personal projects yet (caller falls back to `project create`
/// without `--namespace-id`, which GitLab defaults to the user's personal
/// namespace — same as `gh repo create` defaulting to the authenticated user).
async fn resolve_personal_namespace_id(
    host: &str,
    namespace: &str,
) -> Result<Option<u32>, CodehubError> {
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "-H",
            host,
            "project",
            "list",
            "--mine",
            "--limit",
            "50",
            "--json",
            "namespace",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        // Not fatal — fall back to no-namespace-id create. Log via stderr.
        eprintln!(
            "[BW] codehub namespace 解析失败(project list):{}",
            stderr_text(&out)
        );
        return Ok(None);
    }
    let rows: Vec<NamespaceJson> = serde_json::from_slice(&out.stdout)
        .map_err(|e| CodehubError::Parse(format!("project list namespace JSON:{e}")))?;
    // Prefer an exact full_path match when `namespace` is provided; else first
    // `kind == "user"`.
    let want = namespace.trim();
    if !want.is_empty() {
        // The user typed a namespace — honour it or say why not. Silently
        // falling through to "first personal namespace" would build the repo
        // somewhere they didn't ask for and never tell them (group namespaces
        // aren't supported in V1, so a group name lands here too).
        return match rows
            .iter()
            .find(|r| r.namespace.kind == "user" && r.namespace.full_path == want)
            .map(|r| r.namespace.id)
        {
            Some(nsid) => Ok(Some(nsid)),
            None => Err(CodehubError::Parse(format!(
                "namespace `{want}` 不在你的个人 namespace 里(V1 只支持建到个人 namespace,\
                 group 未做)。留空走默认,或填你自己的 namespace。"
            ))),
        };
    }
    Ok(rows
        .iter()
        .find(|r| r.namespace.kind == "user")
        .map(|r| r.namespace.id))
}

/// `codehub-cli -H <host> project create --name <name> --visibility <vis>
/// --namespace-id <nsid>` → 取 ssh_url → raw `git clone` → 调
/// [`crate::workspace::commit_initial`] 写 BW root commit(让
/// `is_owned_workspace` = true,后续 charter/standards 才会写)。
///
/// 个人 `namespace-id` 由 [`resolve_personal_namespace_id`] 解析;解析不到
/// (用户无个人仓)时退化为不带 `--namespace-id` 创建(GitLab 默认建到个人
/// namespace,对标 `gh repo create` 的默认行为)。`namespace` 空串 = 同样
/// 走默认。group namespace 选择 V1 不做(§6 偏差),如实标。
///
/// `readme_title`/`readme_body` 进 BW 自有的开仓 commit,与 github 新建仓
/// 同一作者(`Builders' Workbench`),让两个 provider 的 owned 判定一致。
pub async fn create_repo(
    host: &str,
    namespace: &str,
    name: &str,
    visibility: &str,
    dest: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<CodehubRepoRef, CodehubError> {
    // 1. 解析个人 namespace-id(可能 None → 不带 --namespace-id 创建)
    let nsid = resolve_personal_namespace_id(host, namespace).await?;
    // 2. project create --json ssh_url_to_repo,path_with_namespace,visibility
    let mut args: Vec<String> = vec![
        "-H".into(),
        host.into(),
        "project".into(),
        "create".into(),
        "--name".into(),
        name.into(),
        "--visibility".into(),
        visibility.into(),
    ];
    if let Some(id) = nsid {
        args.push("--namespace-id".into());
        args.push(id.to_string());
    }
    args.push("--json".into());
    args.push("ssh_url_to_repo,path_with_namespace,visibility".into());
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(format!(
            "project create 失败:{}",
            stderr_text(&out)
        )));
    }
    let v: CreatedRepoJson = serde_json::from_slice(&out.stdout)
        .map_err(|e| CodehubError::Parse(format!("project create JSON:{e}")))?;
    let ssh_url = v.ssh_url_to_repo.trim().to_string();
    if !ssh_url.starts_with("ssh://") {
        return Err(CodehubError::Command(format!(
            "新建仓无 SSH clone 地址(ssh_url_to_repo 为空或老仓?):{ssh_url:?}"
        )));
    }
    // 3. raw git clone(SSH key 认证,同 clone_repo)
    let out = crate::win_cmd::tokio_cmd("git")
        .args(["clone", &ssh_url, &dest.to_string_lossy()])
        .env(
            "GIT_SSH_COMMAND",
            "ssh -o BatchMode=yes -o StrictHostKeyChecking=accept-new",
        )
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    // 4. 写 BW root commit(让 is_owned_workspace=true)
    commit_initial(dest, readme_title, readme_body)
        .await
        .map_err(|e| CodehubError::Command(format!("初始提交失败:{e}")))?;
    Ok(CodehubRepoRef {
        host: host.to_string(),
        path: v.path_with_namespace,
        visibility: v.visibility,
    })
}

/// V2-② Intent UX (§6.2): fetch `.bw/project.toml` from the remote without
/// cloning. Used by the creation flow after the user picks「接入已有仓」so
/// Intent can readonly-prefill later-comers. `Ok(None)` = file absent (404)
/// → first-comer; `Err` = network/auth/parse failure → UI stays editable and
/// does **not** pretend later-comer. Final trio/write gate still uses the
/// local file after clone.
pub async fn fetch_project_toml(
    host: &str,
    path: &str,
    git_ref: &str,
) -> Result<Option<crate::project_file::ProjectFile>, CodehubError> {
    let git_ref = if git_ref.trim().is_empty() {
        "main"
    } else {
        git_ref.trim()
    };
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "-H",
            host,
            "repo",
            "file",
            "raw",
            "-p",
            path,
            "--file-path",
            crate::project_file::PROJECT_FILE_REL_PATH,
            "--ref",
            git_ref,
            "--no-cache",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        let err = stderr_text(&out);
        let lower = err.to_lowercase();
        // **分支找不到**和**这个仓里没有这份文件**是两件事,但两边都是 404。
        // 前者是「没查成」(内部仓的默认分支常是 master,见 `create_mr_on_branch`
        // 的注释),后者才是「这个仓还没被接管过」。混成一个 `Ok(None)`,人就会
        // 看到「首来者,请填」然后一路盖掉仓里已有的名片。
        if lower.contains("branch not found") || lower.contains("ref not found") {
            return Err(CodehubError::Command(err));
        }
        if lower.contains("404") || lower.contains("not found") || lower.contains("does not exist")
        {
            return Ok(None);
        }
        return Err(CodehubError::Command(err));
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    match crate::project_file::parse(raw.trim()) {
        Ok(f) => Ok(Some(f)),
        Err(e) => Err(CodehubError::Parse(e.to_string())),
    }
}

/// `codehub-cli -H <host> project list --mine --limit N --json` → 仓列表
/// (对仗 `gh repo list`)。Read-only,无副作用。解析
/// `path_with_namespace`/`visibility`/`default_branch`/`last_activity_at`/
/// `description` 成 [`CodehubRepoSummary`]。
pub async fn list_repos(host: &str, limit: u32) -> Result<Vec<CodehubRepoSummary>, CodehubError> {
    let out = crate::win_cmd::tokio_cmd("codehub-cli")
        .args([
            "-H",
            host,
            "project",
            "list",
            "--mine",
            "--limit",
            &limit.to_string(),
            "--json",
            "path_with_namespace,visibility,default_branch,last_activity_at,description",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(spawn_err)?;
    if !out.status.success() {
        return Err(CodehubError::Command(stderr_text(&out)));
    }
    let rows: Vec<CodehubRepoJson> = serde_json::from_slice(&out.stdout)
        .map_err(|e| CodehubError::Parse(format!("project list JSON:{e}")))?;
    Ok(rows
        .into_iter()
        .map(|r| CodehubRepoSummary {
            path: r.path_with_namespace,
            visibility: r.visibility,
            default_branch: r.default_branch,
            pushed_at: r.last_activity_at,
            description: r.description.unwrap_or_default(),
        })
        .collect())
}

// ─────────────── JSON deserialization helpers ───────────────

#[derive(serde::Deserialize)]
struct NamespaceJson {
    namespace: NamespaceInner,
}

#[derive(serde::Deserialize)]
struct NamespaceInner {
    id: u32,
    kind: String,
    full_path: String,
}

#[derive(serde::Deserialize)]
struct CreatedRepoJson {
    ssh_url_to_repo: String,
    path_with_namespace: String,
    visibility: String,
}

#[derive(serde::Deserialize)]
struct CodehubRepoJson {
    path_with_namespace: String,
    visibility: String,
    #[serde(default)]
    default_branch: String,
    #[serde(default)]
    last_activity_at: String,
    #[serde(default)]
    description: Option<String>,
}
