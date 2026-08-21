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

use std::path::Path;
use std::process::Stdio;

#[derive(Debug, thiserror::Error)]
pub enum CodehubError {
    #[error("codehub-cli 未安装或不在 PATH")]
    NotInstalled,
    #[error("codehub-cli 失败:{0}")]
    Command(String),
    #[error("解析 codehub-cli 输出失败:{0}")]
    Parse(String),
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

// ─────────────────────────────── 列仓 ───────────────────────────────
//
// `codehub-cli project list --mine` —— 接入屏「接入已有仓」那张列表的数据源,
// 对仗 `github::list_repos`(`gh repo list`)。无状态 shell-out,和本模块其余
// 函数一样;token 走 keyring(`auth login`),不从这里传。
//
// **「新建仓」两个平台都没有**:V4 的接入只有「接入已有仓」这一条路,建仓在
// 平台网页上做。原来这里写着一段 `project create --namespace-id …` 的说明,
// 而那个函数从来没被拷过来 —— 已删,不留一段指着空气的注释。

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
) -> Result<Option<String>, CodehubError> {
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
            crate::github::PROJECT_FILE_REL_PATH,
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
    Ok(Some(
        String::from_utf8_lossy(&out.stdout).trim().to_string(),
    ))
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
