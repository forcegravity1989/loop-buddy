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
    let out = tokio::process::Command::new("codehub-cli")
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
    let out = tokio::process::Command::new("git")
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
