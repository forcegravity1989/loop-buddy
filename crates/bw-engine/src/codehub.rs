//! CodeHub remote — shell-out `codehub-cli`(Go CLI,GitLab v4 同构封装,默认
//! JSON 输出,token 存 OS keyring)。与 [`crate::github`] 对称:无状态自由
//! 函数 + `tokio::process` shell-out,**零 HTTP 依赖**。token 不管(keyring
//! 里 `auth login` 存好),`host`/`path` 按 project 的 `(remote_host,
//! remote_path)` 显式带 `-H`/`-p` 消歧(绿/黄/内源三 host 同名 path 会抛
//! `AmbiguousRemoteError`,显式 host 守住)。
//!
//! 不进 `Executor` 体系——`codehub-cli` 是 VCS 远端 API 客户端(对标 `gh`),
//! 不是 agent 执行器(对标 `claude`)。两种 shell-out 模式别混。

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
    let out = tokio::process::Command::new("codehub-cli")
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
    let out = tokio::process::Command::new("codehub-cli")
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

/// `codehub-cli issue|mr list -p <path> -H <host> --state <state> -l 0 --jq length`
/// → 计数。codehub 无 GitHub 的 `search/issues total_count`,改用分页 list 的
/// `--jq length` 让 CLI 端计数(P3 实测:maas opened issues=6、merged MRs=9)。
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
    let out = tokio::process::Command::new("codehub-cli")
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

/// `codehub-cli repo clone <path> <dest> -H <host>` — 把 codehub 仓 clone 进
/// `dest`。token 走 keyring(profile/CODEHUB_TOKEN),Rust 侧不管。身份(host+path)
/// 调用方已知,这里只 clone,**不回远端身份**(区别于 github::clone_repo 回
/// GithubRepoRef:codehub 身份从用户输入来,不需要 clone 告诉你)。
pub async fn clone_repo(host: &str, path: &str, dest: &Path) -> Result<(), CodehubError> {
    let out = tokio::process::Command::new("codehub-cli")
        .args(["repo", "clone", path, &dest.to_string_lossy(), "-H", host])
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
