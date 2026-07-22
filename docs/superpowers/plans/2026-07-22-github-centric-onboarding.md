# GitHub 为主体的创建引导流 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 把"创建项目"的起点从本地目录换成 GitHub 仓——引导流第一步选新建/接入 GitHub 仓,之后的方法论设置(周期/北极星/指标/阶段)不变。

**Architecture:** 新 `bw-engine/src/github.rs` shell 出 `gh` CLI(复用 `workspace.rs` 的子进程惯例);`Command::CreateProject` 新增 `github: Option<GithubOrigin>` 字段,handler 按"新建/接入/都没有(今天的本地行为)"三路分叉,所有失败路径复用既有 `Event::ConnectorSynced` → toast 管线,不新增 Event 变体、不新增 loading/spinner 状态——沿用 kernel 现有"乐观推进、后台报告成败"的模型。

**Tech Stack:** Rust / Dioxus 0.7 / sqlx(SQLite)/ tokio::process(`gh` shell-out)/ `gh` CLI(已本机安装并登录,scope 含 `repo`)。

## Global Constraints

- 不写/不留单元测试(2026-07-17 起本仓库核心纪律)——每个任务的"验证"步骤是 `cargo check`/`cargo clippy` + 真实 sqlite 读回或真实 `gh` 调用,不是 `#[test]`。
- schema 迁移双守卫:任何新列必须同时进 `schema.sql` 的 `CREATE TABLE` 和 `sqlite.rs` 的 `add_column_if_missing(...)`。
- `Signal`/健康数据只能 derive,不适用于本计划(本计划不碰 Signal)。
- 不做 GitHub OAuth/token 管理、不做 issue/PR/CI 持续同步、不做"补挂 GitHub"重试面板、不做多仓项目——设计文档 §1 非目标,任何任务里出现这类冲动都是范围蔓延,不做。
- 每个任务结束前跑 `cargo fmt --all` 让新增/被 sed 脚本触碰的文件格式一致。
- 设计文档:`docs/superpowers/specs/2026-07-22-github-centric-onboarding-design.md`——本计划的每个决策可回链到那份文档,冲突时以那份文档的决策为准(不要在实现时静默改设计)。

---

## Task 1: `bw-store` —— `github_remote` 列 + `set_github_remote`

**Files:**
- Modify: `crates/bw-store/src/schema.sql`
- Modify: `crates/bw-store/src/sqlite.rs:130-155`(`add_column_if_missing` 调用列表)、`:388-399`(紧跟 `set_workspace` 加新 impl)、`:772-790`(两条 SELECT)、`:2096-2122`(`project_row()`)
- Modify: `crates/bw-store/src/lib.rs:68-73`(不改,见下方说明)、`:281-306`(`ProjectRow`)、`:424`(`Store` trait,紧跟 `set_workspace` 加新签名)

**Interfaces:**
- Produces: `Store::set_github_remote(&self, id: ProjectId, github_remote: &str) -> Result<()>`(trait 新方法,`SqliteStore` 实现);`ProjectRow.github_remote: String`(空串 = 未挂)。
- `NewProject` **不改**——`github_remote` 和 `workspace_path` 一样,创建后才知道,通过专门的 setter 写,不进初始插入 DTO。

- [ ] **Step 1: `schema.sql` 加列**

在 `crates/bw-store/src/schema.sql` 里找到 `project` 表的 `CREATE TABLE`,把:

```sql
    workspace_path     TEXT NOT NULL DEFAULT '', -- 真执行器目标目录;空=未配置,只跑 Mock
    allow_commands     INTEGER NOT NULL DEFAULT 0, -- 真执行器是否额外放行 Bash(不只编辑文件)
```

改成:

```sql
    workspace_path     TEXT NOT NULL DEFAULT '', -- 真执行器目标目录;空=未配置,只跑 Mock
    allow_commands     INTEGER NOT NULL DEFAULT 0, -- 真执行器是否额外放行 Bash(不只编辑文件)
    github_remote      TEXT NOT NULL DEFAULT '', -- "owner/repo";空=未挂 GitHub(本地仓或还没建)
```

- [ ] **Step 2: `sqlite.rs` 加 `add_column_if_missing` 守卫**

在 `crates/bw-store/src/sqlite.rs` 里,`add_column_if_missing(&pool, "agent", "project_id", "TEXT").await?;` 这一行(约第 154 行,`new()` 函数里最后一条 guard)之后、`Ok(Self { pool })` 之前插入:

```rust
        // GitHub 为主体的创建流(2026-07-22):老库开出来 github_remote 是
        // 空串,和"没挂 GitHub"这个真实状态一致,不需要额外语义。
        add_column_if_missing(&pool, "project", "github_remote", "TEXT NOT NULL DEFAULT ''")
            .await?;
```

- [ ] **Step 3: `ProjectRow` 加字段**

在 `crates/bw-store/src/lib.rs`,把:

```rust
    /// Whether the real executor may also run shell commands (Bash), not
    /// just edit files. Meaningless while `workspace_path` is empty.
    pub allow_commands: bool,
    /// Cached derived signal (read-only; recompute is authoritative).
    pub signal: Option<Signal>,
```

改成:

```rust
    /// Whether the real executor may also run shell commands (Bash), not
    /// just edit files. Meaningless while `workspace_path` is empty.
    pub allow_commands: bool,
    /// "owner/repo" — empty = not attached to GitHub (local-only workspace,
    /// or GitHub attach failed and soft-degraded). Set once, at creation.
    pub github_remote: String,
    /// Cached derived signal (read-only; recompute is authoritative).
    pub signal: Option<Signal>,
```

- [ ] **Step 4: `Store` trait 加方法签名**

在 `crates/bw-store/src/lib.rs`,紧跟：

```rust
    async fn set_workspace(&self, id: ProjectId, path: &str, allow_commands: bool) -> Result<()>;
```

之后插入:

```rust
    /// Record the GitHub remote a project's workspace was created from or
    /// adopted from ("owner/repo"). Called once, right after a successful
    /// `bw_engine::github::create_repo`/`clone_repo` — never touched again.
    async fn set_github_remote(&self, id: ProjectId, github_remote: &str) -> Result<()>;
```

- [ ] **Step 5: `sqlite.rs` 实现新方法**

在 `crates/bw-store/src/sqlite.rs`,紧跟 `set_workspace` 的完整实现:

```rust
    async fn set_workspace(&self, id: ProjectId, path: &str, allow_commands: bool) -> Result<()> {
        sqlx::query(
            "UPDATE project SET workspace_path=?, allow_commands=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(path)
        .bind(allow_commands as i64)
        .bind(now_unix())
        .bind(pid(id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
```

插入:

```rust

    async fn set_github_remote(&self, id: ProjectId, github_remote: &str) -> Result<()> {
        sqlx::query("UPDATE project SET github_remote=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(github_remote)
            .bind(now_unix())
            .bind(pid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
```

- [ ] **Step 6: 两条 SELECT 加列**

在 `crates/bw-store/src/sqlite.rs`,`get_project`(约 772 行)和 `list_projects`(约 783 行)里各有一条几乎相同的 SQL。把两处的:

```
"SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, signal, weekly_signal, created_at
```

改成(每处都改):

```
"SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, github_remote, signal, weekly_signal, created_at
```

(两处的 `FROM project WHERE id=?` / `FROM project ORDER BY created_at` 结尾不变。)

- [ ] **Step 7: `project_row()` 解码加字段**

在 `crates/bw-store/src/sqlite.rs` 的 `project_row()` 函数里,把:

```rust
        workspace_path: r.get("workspace_path"),
        allow_commands: r.get::<i64, _>("allow_commands") != 0,
```

改成:

```rust
        workspace_path: r.get("workspace_path"),
        allow_commands: r.get::<i64, _>("allow_commands") != 0,
        github_remote: r.get("github_remote"),
```

- [ ] **Step 8: 编译检查**

```bash
cargo check -p bw-store
```

Expected: 编译通过(`Store` trait 只有 `SqliteStore` 一个实现者,不会有其他 impl 因缺方法报错)。

- [ ] **Step 9: 真实旧库读回验证**

用一个已存在的真实 demo DB(不是新建的)验证双守卫真的对旧库生效——旧库开出来不能崩,且能读到新列:

```bash
cp demo-workspaces/bw-demo.db /tmp/bw-migration-check.db
cargo run -p bw-app --example seed_demo -- /tmp/bw-migration-check.db --dry-run 2>&1 | head -5 || true
sqlite3 /tmp/bw-migration-check.db "PRAGMA table_info(project);" | grep github_remote
```

Expected:`PRAGMA table_info` 的输出里能看到 `github_remote|TEXT|1||0`(NOT NULL、default 空串)那一行;旧库文件本身没有被删除重建(`ls -la` 时间戳/大小和 cp 前一致,除了新增列)。如果本地没有 `demo-workspaces/bw-demo.db`,改用任意其它已存在的真实 db 文件,或先用 `cargo run -p app-desktop` 跑一次生成一个,再重复这个验证。

- [ ] **Step 10: Commit**

```bash
git add crates/bw-store/src/schema.sql crates/bw-store/src/sqlite.rs crates/bw-store/src/lib.rs
git commit -m "$(cat <<'EOF'
bw-store · project.github_remote 列(双守卫迁移)

新增 github_remote 列记录项目挂的 GitHub 远端("owner/repo"),schema.sql +
add_column_if_missing 双守卫,旧库开启不崩;set_github_remote 与既有
set_workspace 同构。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: `bw-engine` —— `github.rs` shell 出 `gh` CLI

**Files:**
- Modify: `crates/bw-engine/src/workspace.rs`(把首次提交逻辑拆成 `pub(crate) commit_initial`,`git_in` 改 `pub(crate)`)
- Create: `crates/bw-engine/src/github.rs`
- Modify: `crates/bw-engine/src/lib.rs`(加 `pub mod github;` + 重导出)

**Interfaces:**
- Consumes: `crate::workspace::{commit_initial, git_in}`(本任务内新增的 crate 内可见函数)。
- Produces: `bw_engine::github::create_repo(slug: &str, private: bool, dest_root: &Path, readme_title: &str, readme_body: &str) -> Result<GithubRepoRef, GithubError>`、`bw_engine::github::clone_repo(owner: &str, repo: &str, dest: &Path) -> Result<GithubRepoRef, GithubError>`、`bw_engine::github::list_repos(limit: u32) -> Result<Vec<GithubRepoSummary>, GithubError>`;`bw_engine::{GithubError, GithubRepoRef, GithubRepoSummary}`(顶层重导出)。这三个函数签名是 Task 3 里 `bw-app` 唯一会调用的接口。

- [ ] **Step 1: `workspace.rs` 拆出 `commit_initial`,`git_in` 改 `pub(crate)`**

在 `crates/bw-engine/src/workspace.rs`,把:

```rust
async fn git_in(dir: &Path, args: &[&str]) -> Result<(), ProvisionError> {
```

改成:

```rust
pub(crate) async fn git_in(dir: &Path, args: &[&str]) -> Result<(), ProvisionError> {
```

然后把整个 `provision_git_workspace` 函数:

```rust
pub async fn provision_git_workspace(
    dir: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<(), ProvisionError> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| ProvisionError::CreateDir(e.to_string()))?;
    git_in(dir, &["init", "-q"]).await?;
    let readme = format!("# {readme_title}\n\n{readme_body}\n");
    std::fs::write(dir.join("README.md"), readme)
        .map_err(|e| ProvisionError::Write(e.to_string()))?;
    std::fs::write(dir.join(".gitignore"), "/target\n")
        .map_err(|e| ProvisionError::Write(e.to_string()))?;
    git_in(dir, &["add", "-A"]).await?;
    git_in(
        dir,
        &[
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            "chore: workspace 开仓(builders-workbench 托管起点)",
        ],
    )
    .await?;
    Ok(())
}
```

替换成:

```rust
pub async fn provision_git_workspace(
    dir: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<(), ProvisionError> {
    if dir.join(".git").exists() {
        return Ok(());
    }
    std::fs::create_dir_all(dir).map_err(|e| ProvisionError::CreateDir(e.to_string()))?;
    git_in(dir, &["init", "-q"]).await?;
    commit_initial(dir, readme_title, readme_body).await
}

/// Write the workbench's opening README/.gitignore and make the first
/// commit, authored as the workbench. Split out of `provision_git_workspace`
/// so `bw_engine::github::create_repo` can reuse the exact same first-commit
/// authorship on a directory `gh repo create --clone` already initialized
/// (the `.git`-exists early return above doesn't apply there — the repo is
/// real but has zero commits yet).
pub(crate) async fn commit_initial(
    dir: &Path,
    readme_title: &str,
    readme_body: &str,
) -> Result<(), ProvisionError> {
    let readme = format!("# {readme_title}\n\n{readme_body}\n");
    std::fs::write(dir.join("README.md"), readme)
        .map_err(|e| ProvisionError::Write(e.to_string()))?;
    std::fs::write(dir.join(".gitignore"), "/target\n")
        .map_err(|e| ProvisionError::Write(e.to_string()))?;
    git_in(dir, &["add", "-A"]).await?;
    git_in(
        dir,
        &[
            "-c",
            "user.name=Builders' Workbench",
            "-c",
            "user.email=workbench@local",
            "commit",
            "-qm",
            "chore: workspace 开仓(builders-workbench 托管起点)",
        ],
    )
    .await?;
    Ok(())
}
```

- [ ] **Step 2: 编译检查(纯重构,行为不变)**

```bash
cargo check -p bw-engine
```

Expected: 通过。这一步是纯提取重构,本地 mint 路径的行为字节级不变——不需要额外验证,后面 Task 6 的真实 E2E 会连带验证到。

- [ ] **Step 3: 新建 `crates/bw-engine/src/github.rs`**

```rust
//! GitHub shell-out — mints or adopts a GitHub repo via the `gh` CLI, same
//! subprocess pattern `workspace.rs` uses for local git. Relies entirely on
//! the user's own `gh auth login` on this machine; no token handling here.

use crate::workspace::{commit_initial, git_in};
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
    pub updated_at: String,
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

/// Clone an already-existing GitHub repo the user picked into `dest`.
pub async fn clone_repo(owner: &str, repo: &str, dest: &Path) -> Result<GithubRepoRef, GithubError> {
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

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoJson {
    name_with_owner: String,
    is_private: bool,
    updated_at: String,
}

/// List repos owned by the authenticated user — the "接入已有仓" picker's
/// data source. Read-only, no local filesystem side effects.
pub async fn list_repos(limit: u32) -> Result<Vec<GithubRepoSummary>, GithubError> {
    let output = tokio::process::Command::new("gh")
        .args([
            "repo",
            "list",
            "--json",
            "nameWithOwner,isPrivate,updatedAt",
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
                updated_at: r.updated_at,
            })
        })
        .collect())
}
```

- [ ] **Step 4: `lib.rs` 加模块声明 + 重导出**

在 `crates/bw-engine/src/lib.rs`,把:

```rust
pub mod claude_cli;
pub mod contract;
pub mod evidence;
pub mod git_log;
mod mock;
pub mod workspace;

pub use claude_cli::{ClaudeCliConfig, ClaudeCliExecutor, PermissionMode};
pub use evidence::{EvidenceError, WorkspaceEvidence, WorkspaceFile};
pub use git_log::{read_commits, GitCommit, GitLogError};
pub use mock::MockExecutor;
pub use workspace::{provision_git_workspace, ProvisionError};
```

改成:

```rust
pub mod claude_cli;
pub mod contract;
pub mod evidence;
pub mod git_log;
pub mod github;
mod mock;
pub mod workspace;

pub use claude_cli::{ClaudeCliConfig, ClaudeCliExecutor, PermissionMode};
pub use evidence::{EvidenceError, WorkspaceEvidence, WorkspaceFile};
pub use git_log::{read_commits, GitCommit, GitLogError};
pub use github::{GithubError, GithubRepoRef, GithubRepoSummary};
pub use mock::MockExecutor;
pub use workspace::{provision_git_workspace, ProvisionError};
```

- [ ] **Step 5: 编译 + lint 检查**

```bash
cargo check -p bw-engine
cargo clippy -p bw-engine -- -D warnings
```

Expected: 两条都通过,无 warning。

- [ ] **Step 6: 真实 smoke 检查(只读,不改任何账号状态)**

`list_repos` 是纯读操作,可以立即用真实账号验证解析逻辑是否正确,不需要等到 Task 6 的完整流程验证:

```bash
cat > /tmp/github_smoke.rs <<'EOF'
#[tokio::main]
async fn main() {
    match bw_engine::github::list_repos(5).await {
        Ok(repos) => {
            println!("OK, {} repos", repos.len());
            for r in &repos {
                println!("{}/{} private={} updated={}", r.owner, r.repo, r.private, r.updated_at);
            }
        }
        Err(e) => println!("ERR: {e}"),
    }
}
EOF
mkdir -p /tmp/github_smoke_bin/src
cp /tmp/github_smoke.rs /tmp/github_smoke_bin/src/main.rs
cd /tmp/github_smoke_bin
cat > Cargo.toml <<EOF
[package]
name = "github_smoke_bin"
version = "0.0.0"
edition = "2021"
[dependencies]
bw-engine = { path = "/Users/gravity/projects/builders-workbench/crates/bw-engine" }
tokio = { version = "1", features = ["full"] }
EOF
cargo run 2>&1 | tail -10
cd /Users/gravity/projects/builders-workbench
```

Expected: `OK, N repos` with real `owner/repo` lines from the authenticated `forcegravity1989` account — proves the JSON field mapping (`nameWithOwner`/`isPrivate`/`updatedAt` → `RepoJson` → `GithubRepoSummary`) is correct against the real `gh` CLI output shape, not just a guess from `--help` text.

- [ ] **Step 7: Commit**

```bash
git add crates/bw-engine/src/workspace.rs crates/bw-engine/src/github.rs crates/bw-engine/src/lib.rs
git commit -m "$(cat <<'EOF'
bw-engine · github.rs shell 出 gh CLI(create/clone/list)

新模块复用 workspace.rs 的子进程惯例:create_repo 新建仓+clone+首次提交+
push,clone_repo 接入已有仓,list_repos 读仓库列表。workspace.rs 拆出
commit_initial 供 github.rs 复用同一套首次提交作者身份逻辑,行为不变。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: `bw-core` 连接器常量 + `bw-app` Command/handler

**Files:**
- Modify: `crates/bw-core/src/model.rs:1211-1215`(连接器 kind 常量)
- Modify: `crates/bw-app/src/lib.rs`(imports、`GithubOrigin` 类型、`Command::CreateProject` 字段、新 `Command::ListGithubRepos`、`AppState.github_repos`、`CreateProject` handler 重写)
- Modify: 10 个 `crates/bw-app/examples/*.rs`(机械补 `github: None,`)

**Interfaces:**
- Consumes: `bw_engine::github::{create_repo, clone_repo, list_repos}`(Task 2)、`Store::set_github_remote`(Task 1)。
- Produces: `pub enum GithubOrigin { New { slug: String, private: bool }, Existing { owner: String, repo: String } }`、`Command::CreateProject { .., github: Option<GithubOrigin> }`、`Command::ListGithubRepos`、`AppState.github_repos: Vec<bw_engine::GithubRepoSummary>`——Task 4(kernel.rs)读这三个。

- [ ] **Step 1: `bw-core` 加连接器常量**

在 `crates/bw-core/src/model.rs`,把:

```rust
/// The two connector kinds the workbench can *really* sync today — everything
/// else stays a free-text reference entry (recorded, listed, honestly marked
/// unsynced). Matching is by the `Connector.kind` string.
pub const CONNECTOR_KIND_GIT_REPO: &str = "git-repo";
pub const CONNECTOR_KIND_CLAUDE_CLI: &str = "claude-cli";
```

改成:

```rust
/// The two connector kinds the workbench can *really* sync today — everything
/// else stays a free-text reference entry (recorded, listed, honestly marked
/// unsynced). Matching is by the `Connector.kind` string.
pub const CONNECTOR_KIND_GIT_REPO: &str = "git-repo";
pub const CONNECTOR_KIND_CLAUDE_CLI: &str = "claude-cli";
/// GitHub 为主体的创建流(2026-07-22)：记录一个项目挂的 GitHub 远端
/// ("owner/repo" 进 `config`)。目前是诚实标注未同步的引用条目——不接
/// `SyncConnector` 真探针,持续同步(issue/PR/CI 统计)是独立的后续功能。
pub const CONNECTOR_KIND_GITHUB_REPO: &str = "github-repo";
```

- [ ] **Step 2: `bw-app` imports 加新符号**

在 `crates/bw-app/src/lib.rs`,把:

```rust
use bw_core::model::{
    classify_artifact_path, cron_due, stage_workflow, stage_workflow_with_playbook, AgentCard,
    AgentRef, Artifact, Cadence, Connector, ConnectorStatus, CronMode, CronStatus, CronTask,
    HubSource, Issue, IssuePriority, IssueStatus, KnowledgeSource, LibSource, LoopConfig, Maturity,
    ProjectCycle, ProjectPhase, Role, RunStatus, RunTrigger, Signal, SkillCard, SkillRef,
    SourceKind, StageKind, WorkflowKind, WorkflowSpec, CONNECTOR_KIND_CLAUDE_CLI,
    CONNECTOR_KIND_GIT_REPO,
};
```

改成:

```rust
use bw_core::model::{
    classify_artifact_path, cron_due, stage_workflow, stage_workflow_with_playbook, AgentCard,
    AgentRef, Artifact, Cadence, Connector, ConnectorStatus, CronMode, CronStatus, CronTask,
    HubSource, Issue, IssuePriority, IssueStatus, KnowledgeSource, LibSource, LoopConfig, Maturity,
    ProjectCycle, ProjectPhase, Role, RunStatus, RunTrigger, Signal, SkillCard, SkillRef,
    SourceKind, StageKind, WorkflowKind, WorkflowSpec, CONNECTOR_KIND_CLAUDE_CLI,
    CONNECTOR_KIND_GIT_REPO, CONNECTOR_KIND_GITHUB_REPO,
};
```

并把:

```rust
use bw_engine::{
    evidence, ClaudeCliConfig, ClaudeCliExecutor, Engine, GitCommit, PermissionMode, RunCtx,
    RunEvent,
};
```

改成:

```rust
use bw_engine::{
    evidence, ClaudeCliConfig, ClaudeCliExecutor, Engine, GitCommit, GithubRepoSummary,
    PermissionMode, RunCtx, RunEvent,
};
```

- [ ] **Step 3: 定义 `GithubOrigin`,`Command::CreateProject` 加字段**

在 `crates/bw-app/src/lib.rs`,`Command` enum 定义之前(紧挨着 `pub enum Command {` 上方)插入:

```rust
/// Where a newly-created project's git remote comes from — the Repo 卡片的
/// 选择,carried into `Command::CreateProject`. `New` mints a fresh GitHub
/// repo (`gh repo create --clone`); `Existing` clones one the user already
/// owns. `None` on the command (every pre-2026-07-22 caller) keeps every
/// existing behavior — pure local mint or bound-local-path — untouched.
#[derive(Clone, Debug)]
pub enum GithubOrigin {
    New { slug: String, private: bool },
    Existing { owner: String, repo: String },
}

```

然后把 `Command::CreateProject` 变体:

```rust
    CreateProject {
        id: ProjectId,
        name: String,
        kind: String,
        desc: String,
        /// P1: optional pre-existing repo to bind (must contain `.git`). When
        /// `None` and a workspaces root is configured, a fresh repo is minted
        /// at creation. Bound repos are never rewritten by the workbench.
        workspace: Option<String>,
    },
```

改成:

```rust
    CreateProject {
        id: ProjectId,
        name: String,
        kind: String,
        desc: String,
        /// P1: optional pre-existing *local* repo to bind (must contain
        /// `.git`). Mutually exclusive with `github` — the Repo 卡片 is the
        /// sole UI entry point and only ever sets one of the two.
        workspace: Option<String>,
        /// GitHub 为主体(2026-07-22): Repo 卡片的选择. `None` = neither
        /// bound (`workspace` also `None`) → today's local-mint-if-configured
        /// default, unchanged.
        github: Option<GithubOrigin>,
    },
```

再往下找到:

```rust
    /// Creation flow step 2 (快速问题 · 周期).
    SetCycle {
        cycle: ProjectCycle,
    },
```

在它前面插入新命令:

```rust
    /// GitHub 为主体的创建流(2026-07-22): 读一次当前用户可接入的仓列表,
    /// 填充 `AppState.github_repos`(Repo 卡片"接入已有仓"下拉的数据源)。
    /// 显式加载,同 `LoadVersionLog`/`LoadArtifacts` 惯例——不在每次
    /// rebuild 里打 GitHub API。
    ListGithubRepos,
    /// Creation flow step 2 (快速问题 · 周期).
    SetCycle {
        cycle: ProjectCycle,
    },
```

- [ ] **Step 4: `AppState` 加 `github_repos` 字段**

在 `crates/bw-app/src/lib.rs`,把:

```rust
    /// P4: the explicitly-opened Issue detail (board overlay) — same
    /// explicit-load pattern as `artifacts`. `None` = no overlay open.
    pub issue_detail: Option<IssueDetailData>,
}
```

改成:

```rust
    /// P4: the explicitly-opened Issue detail (board overlay) — same
    /// explicit-load pattern as `artifacts`. `None` = no overlay open.
    pub issue_detail: Option<IssueDetailData>,
    /// GitHub 为主体的创建流: last `Command::ListGithubRepos` result. Process-
    /// internal cache of live GitHub data, not persisted — it's a direct
    /// read-through, not one of this app's own derived Signals.
    pub github_repos: Vec<GithubRepoSummary>,
}
```

- [ ] **Step 5: 重写 `Command::CreateProject` handler**

在 `crates/bw-app/src/lib.rs`,找到整个 `Command::CreateProject { .. } => { .. }` 块(约 1596-1671 行,从 `Command::CreateProject {` 开始到匹配的 `}` 结束,`self.emit(Event::ViewChanged(View::Create));` 后一行的 `}` 收尾)。原文:

```rust
            Command::CreateProject {
                id,
                name,
                kind,
                desc,
                workspace,
            } => {
                self.store
                    .create_project(NewProject {
                        id,
                        name,
                        kind,
                        desc,
                    })
                    .await?;
                self.state.active_project = Some(id);
                self.state.view = View::Create;
                // P1: 建项目即建仓 —— 出生那一刻仓就存在(而非走完创建流才有)。
                // 绑定已有仓:只校验含 .git,绝不动原文件;新建仓在 workspaces_root
                // 下 mint,失败沿用既有降级(项目以 Mock 模式活着,创建本身不破)。
                let bound = workspace
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                match bound {
                    Some(path) => {
                        if !std::path::Path::new(path).join(".git").exists() {
                            return Err(AppError::Invalid(format!(
                                "绑定的工作目录不是 git 仓库(无 .git):{path}"
                            )));
                        }
                        self.store.set_workspace(id, path, true).await?;
                    }
                    None => {
                        if let Some(root) = self.workspaces_root.clone() {
                            match provision_workspace(&root, &proj).await {
                                Ok(path) => {
                                    self.store.set_workspace(id, &path, true).await?;
                                    self.store
                                        .create_connector(NewConnector {
                                            id: ConnectorId::new(),
                                            name: format!("{} · 代码仓", proj.name),
                                            kind: CONNECTOR_KIND_GIT_REPO.into(),
                                            scope: proj.name.clone(),
                                            project_id: Some(id),
                                            config: path.clone(),
                                        })
                                        .await?;
                                }
                                Err(e) => {
                                    self.emit(Event::ConnectorSynced {
                                        name: format!("{} · 代码仓", proj.name),
                                        ok: false,
                                        detail: format!("自动开仓失败,项目将以 Mock 模式运行:{e}"),
                                    });
                                }
                            }
                        }
                    }
                }
                // 章程开篇(仅 owned 仓写;bound 仓尊重「不动原文件」)。
                let _ = write_charter(self, id, "开篇").await;
                // 模板能力(用户 2026-07-20 拍板):四份组件标准文件写进仓里,
                // 供人与 agent 之后在这个项目里创建 agent/skill/workflow/cron 时
                // 对照(同一 owned-workspace 门槛,一次性,不随创建流逐步改写)。
                let _ = write_component_standards(self, id).await;
                self.refresh_projects().await?;
                self.refresh_connectors().await?;
                self.emit(Event::ProjectsChanged);
                self.emit(Event::ViewChanged(View::Create));
            }
```

替换为:

```rust
            Command::CreateProject {
                id,
                name,
                kind,
                desc,
                workspace,
                github,
            } => {
                self.store
                    .create_project(NewProject {
                        id,
                        name,
                        kind,
                        desc,
                    })
                    .await?;
                self.state.active_project = Some(id);
                self.state.view = View::Create;
                // P1: 建项目即建仓 —— 出生那一刻仓就存在(而非走完创建流才有)。
                // 绑定已有本地仓:只校验含 .git,绝不动原文件。GitHub 为主体
                // (2026-07-22): github 非空时改走 gh CLI 开仓/接入,新建失败
                // 软降级回本地 mint,接入失败不兜底(不拿无关空仓冒充)。两条
                // 路径都绝不让 CreateProject 本身失败——只有本地 bind 校验例外。
                let bound = workspace
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let proj = self
                    .store
                    .get_project(id)
                    .await?
                    .ok_or(AppError::NotFound)?;
                match (bound, github) {
                    (Some(path), _) => {
                        if !std::path::Path::new(path).join(".git").exists() {
                            return Err(AppError::Invalid(format!(
                                "绑定的工作目录不是 git 仓库(无 .git):{path}"
                            )));
                        }
                        self.store.set_workspace(id, path, true).await?;
                    }
                    (None, Some(GithubOrigin::New { slug, private })) => {
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let body = if proj.desc.trim().is_empty() {
                                    "(创建流程未填写 brief)".to_string()
                                } else {
                                    proj.desc.trim().to_string()
                                };
                                match bw_engine::github::create_repo(
                                    &slug, private, &root, &proj.name, &body,
                                )
                                .await
                                {
                                    Ok(r) => {
                                        let path = root.join(&slug).to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &path, true).await?;
                                        self.store
                                            .set_github_remote(id, &format!("{}/{}", r.owner, r.repo))
                                            .await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · 代码仓", proj.name),
                                                kind: CONNECTOR_KIND_GIT_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: path.clone(),
                                            })
                                            .await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · GitHub", proj.name),
                                                kind: CONNECTOR_KIND_GITHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{}/{}", r.owner, r.repo),
                                            })
                                            .await?;
                                    }
                                    Err(e) => {
                                        let mut detail =
                                            format!("GitHub 建仓失败,已尝试改建本地仓:{e}");
                                        match provision_workspace(&root, &proj).await {
                                            Ok(path) => {
                                                self.store.set_workspace(id, &path, true).await?;
                                                self.store
                                                    .create_connector(NewConnector {
                                                        id: ConnectorId::new(),
                                                        name: format!("{} · 代码仓", proj.name),
                                                        kind: CONNECTOR_KIND_GIT_REPO.into(),
                                                        scope: proj.name.clone(),
                                                        project_id: Some(id),
                                                        config: path.clone(),
                                                    })
                                                    .await?;
                                            }
                                            Err(local_e) => {
                                                detail = format!(
                                                    "GitHub 建仓失败:{e};本地兜底也失败:{local_e}"
                                                );
                                            }
                                        }
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · GitHub", proj.name),
                                            ok: false,
                                            detail,
                                        });
                                    }
                                }
                            }
                            None => {
                                self.emit(Event::ConnectorSynced {
                                    name: format!("{} · GitHub", proj.name),
                                    ok: false,
                                    detail: "未配置本地工作区根目录,无法建仓".into(),
                                });
                            }
                        }
                    }
                    (None, Some(GithubOrigin::Existing { owner, repo })) => {
                        match self.workspaces_root.clone() {
                            Some(root) => {
                                let dir = root.join(workspace_slug(&proj.name, id));
                                match bw_engine::github::clone_repo(&owner, &repo, &dir).await {
                                    Ok(r) => {
                                        let path = dir.to_string_lossy().into_owned();
                                        self.store.set_workspace(id, &path, true).await?;
                                        self.store
                                            .set_github_remote(id, &format!("{}/{}", r.owner, r.repo))
                                            .await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · 代码仓", proj.name),
                                                kind: CONNECTOR_KIND_GIT_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: path.clone(),
                                            })
                                            .await?;
                                        self.store
                                            .create_connector(NewConnector {
                                                id: ConnectorId::new(),
                                                name: format!("{} · GitHub", proj.name),
                                                kind: CONNECTOR_KIND_GITHUB_REPO.into(),
                                                scope: proj.name.clone(),
                                                project_id: Some(id),
                                                config: format!("{}/{}", r.owner, r.repo),
                                            })
                                            .await?;
                                    }
                                    Err(e) => {
                                        // 不兜底本地 mint —— 拿一个跟用户选的仓无关
                                        // 的空仓冒充"已接入",比"暂不挂仓库"更不诚实。
                                        self.emit(Event::ConnectorSynced {
                                            name: format!("{} · GitHub", proj.name),
                                            ok: false,
                                            detail: format!("接入 {owner}/{repo} 失败:{e}"),
                                        });
                                    }
                                }
                            }
                            None => {
                                self.emit(Event::ConnectorSynced {
                                    name: format!("{} · GitHub", proj.name),
                                    ok: false,
                                    detail: "未配置本地工作区根目录,无法接入".into(),
                                });
                            }
                        }
                    }
                    (None, None) => {
                        if let Some(root) = self.workspaces_root.clone() {
                            match provision_workspace(&root, &proj).await {
                                Ok(path) => {
                                    self.store.set_workspace(id, &path, true).await?;
                                    self.store
                                        .create_connector(NewConnector {
                                            id: ConnectorId::new(),
                                            name: format!("{} · 代码仓", proj.name),
                                            kind: CONNECTOR_KIND_GIT_REPO.into(),
                                            scope: proj.name.clone(),
                                            project_id: Some(id),
                                            config: path.clone(),
                                        })
                                        .await?;
                                }
                                Err(e) => {
                                    self.emit(Event::ConnectorSynced {
                                        name: format!("{} · 代码仓", proj.name),
                                        ok: false,
                                        detail: format!("自动开仓失败,项目将以 Mock 模式运行:{e}"),
                                    });
                                }
                            }
                        }
                    }
                }
                // 章程开篇(仅 owned 仓写;bound 仓尊重「不动原文件」)。
                let _ = write_charter(self, id, "开篇").await;
                // 模板能力(用户 2026-07-20 拍板):四份组件标准文件写进仓里,
                // 供人与 agent 之后在这个项目里创建 agent/skill/workflow/cron 时
                // 对照(同一 owned-workspace 门槛,一次性,不随创建流逐步改写)。
                let _ = write_component_standards(self, id).await;
                self.refresh_projects().await?;
                self.refresh_connectors().await?;
                self.emit(Event::ProjectsChanged);
                self.emit(Event::ViewChanged(View::Create));
            }

            Command::ListGithubRepos => {
                match bw_engine::github::list_repos(30).await {
                    Ok(repos) => self.state.github_repos = repos,
                    Err(e) => {
                        self.state.github_repos = Vec::new();
                        self.emit(Event::ConnectorSynced {
                            name: "GitHub 仓库列表".into(),
                            ok: false,
                            detail: e.to_string(),
                        });
                    }
                }
                self.emit(Event::ProjectsChanged);
            }
```

- [ ] **Step 6: 机械修补 10 个 example 文件的 `CreateProject` 构造点**

这些文件都构造 `Command::CreateProject { .. }` 但没有(也不需要)GitHub 字段——它们是既有的本地 mock/真实-本地 demo 指挥器,不涉及本次功能,只需要补上新增的必填字段:

```bash
for f in crates/bw-app/examples/self_optimize_demo.rs \
         crates/bw-app/examples/practice_aihot.rs \
         crates/bw-app/examples/record_fusion_round.rs \
         crates/bw-app/examples/real_team_loop.rs \
         crates/bw-app/examples/dogfood_workflowhub.rs \
         crates/bw-app/examples/verify_goal.rs \
         crates/bw-app/examples/seed_demo.rs \
         crates/bw-app/examples/real_demo.rs \
         crates/bw-app/examples/simulate_hub.rs \
         crates/bw-app/examples/seed_board_demo.rs; do
  sed -i '' 's/workspace: None,/workspace: None,\n        github: None,/' "$f"
done
cargo fmt --all
```

- [ ] **Step 7: 编译检查**

```bash
cargo check -p bw-core
cargo check -p bw-app
cargo check -p bw-app --examples
cargo clippy -p bw-core -- -D warnings
cargo clippy -p bw-app -- -D warnings
```

Expected: 全部通过。`--examples` 这条专门确认 Step 6 的 sed 修补对全部 10 个 example 生效、没有漏改或格式错乱。

- [ ] **Step 8: Commit**

```bash
git add crates/bw-core/src/model.rs crates/bw-app/src/lib.rs crates/bw-app/examples/
git commit -m "$(cat <<'EOF'
bw-core+bw-app · CreateProject 接 GitHub 起源 + ListGithubRepos

CONNECTOR_KIND_GITHUB_REPO 常量;GithubOrigin::{New,Existing} 承载 Repo
卡片的选择;CreateProject handler 三路分叉(本地 bind/GitHub 新建软降级/
GitHub 接入不兜底/都没有→今天行为);新 ListGithubRepos 显式加载仓库列表。
10 个 example 指挥器机械补 github:None,行为不变。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: `app-desktop/kernel.rs` —— `OpVm.github_remote` + `Vm.github_repos`

**Files:**
- Modify: `crates/app-desktop/src/kernel.rs`

**Interfaces:**
- Consumes: `bw_engine::GithubRepoSummary`(Task 2)、`ProjectRow.github_remote`(Task 1)、`AppState.github_repos`(Task 3)。
- Produces: `OpVm.github_remote: String`、`Vm.github_repos: Vec<GithubRepoSummary>`——Task 5(UI)读这两个。

- [ ] **Step 1: import 加 `GithubRepoSummary`**

在 `crates/app-desktop/src/kernel.rs`,把:

```rust
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
```

改成:

```rust
use bw_engine::{ClaudeCliConfig, Engine, GithubRepoSummary, MockExecutor, PermissionMode};
```

- [ ] **Step 2: `Vm` 加 `github_repos` 字段**

把:

```rust
    /// L1(plan/11): last-loaded cron task's real fire history — lives at the
    /// top level (not `OpVm`) because the component-detail overlay that
    /// shows it is rendered outside any one project's `Op` tree, same as
    /// `hub`. `None` until `Command::LoadCronEffectiveness` runs for a task.
    pub cron_effectiveness: Option<(bw_core::CronTaskId, ui::vm::CronEffectivenessVm)>,
}
```

改成:

```rust
    /// L1(plan/11): last-loaded cron task's real fire history — lives at the
    /// top level (not `OpVm`) because the component-detail overlay that
    /// shows it is rendered outside any one project's `Op` tree, same as
    /// `hub`. `None` until `Command::LoadCronEffectiveness` runs for a task.
    pub cron_effectiveness: Option<(bw_core::CronTaskId, ui::vm::CronEffectivenessVm)>,
    /// GitHub 为主体的创建流: last `Command::ListGithubRepos` result — lives
    /// at the top level (not `CreateVm`) because the Repo 卡片 renders before
    /// any project row exists. Empty until the Repo 卡片 first dispatches
    /// `ListGithubRepos` (switching to "接入已有仓").
    pub github_repos: Vec<GithubRepoSummary>,
}
```

- [ ] **Step 3: `build_vm` 的 `Vm { .. }` 字面量加字段**

把:

```rust
    let mut vm = Vm {
        ready: true,
        fatal: None,
        view: state.view,
        projects: cards,
        create: None,
        op: None,
        hub: hub.clone(),
        settings,
        cron_effectiveness,
    };
```

改成:

```rust
    let mut vm = Vm {
        ready: true,
        fatal: None,
        view: state.view,
        projects: cards,
        create: None,
        op: None,
        hub: hub.clone(),
        settings,
        cron_effectiveness,
        github_repos: state.github_repos.clone(),
    };
```

- [ ] **Step 4: `OpVm` 加 `github_remote` 字段**

把:

```rust
    /// Real-executor target directory. Empty = unconfigured — this project
    /// only ever runs `RunWorkflow` on `MockExecutor`.
    pub workspace_path: String,
    pub allow_commands: bool,
```

改成:

```rust
    /// Real-executor target directory. Empty = unconfigured — this project
    /// only ever runs `RunWorkflow` on `MockExecutor`.
    pub workspace_path: String,
    pub allow_commands: bool,
    /// "owner/repo" — empty = this project isn't attached to GitHub (local-
    /// only workspace, or the GitHub attach attempt failed and soft-degraded).
    pub github_remote: String,
```

- [ ] **Step 5: `OpVm { .. }` 构造字面量加字段**

把:

```rust
        workspace_path: row.workspace_path.clone(),
        allow_commands: row.allow_commands,
```

改成:

```rust
        workspace_path: row.workspace_path.clone(),
        allow_commands: row.allow_commands,
        github_remote: row.github_remote.clone(),
```

- [ ] **Step 6: 编译检查**

```bash
cargo check -p app-desktop
cargo fmt --all --check
```

Expected: `app-desktop` 编译过(此时 `create.rs`/`main.rs` 还没吃到新 prop,`cargo check -p app-desktop` 应该已经因为 `Create { .. }` 调用点缺 `github_repos` prop 而报错——这是预期的、Task 5 会修的中间态。如果这一步真的报错,记录下来,不要在这个任务里去改 `create.rs`/`main.rs`,那是 Task 5 的范围;`cargo fmt --all --check` 仍然要过,不受组件 prop 缺失影响。)

- [ ] **Step 7: Commit**

```bash
git add crates/app-desktop/src/kernel.rs
git commit -m "$(cat <<'EOF'
app-desktop/kernel · OpVm.github_remote + Vm.github_repos

kernel.rs 把 Task1-3 新增的 github_remote/github_repos 从 store/AppState
接进 VM 层,与既有 workspace_path 同款写法。app-desktop 整体编译要等
Task 5 补上 Create 组件的新 prop 才会通过,是预期中间态。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: `app-desktop` UI —— `Card::Repo` + `IntentCard` slug 预览 + `op.rs` 徽记

**Files:**
- Modify: `crates/app-desktop/src/screens/create.rs`
- Modify: `crates/app-desktop/src/screens/op.rs:1100-1112`(`WorkspaceConfig` 只读态)
- Modify: `crates/app-desktop/src/main.rs:318-323`(`Create { .. }` 调用点)

**Interfaces:**
- Consumes: `bw_app::{Command, GithubOrigin}`(Task 3)、`bw_engine::GithubRepoSummary`(Task 2)、`Vm.github_repos`/`OpVm.github_remote`(Task 4)。
- Produces: 无(叶子任务,UI 终点)。

- [ ] **Step 1: `create.rs` imports 加新符号**

把:

```rust
use crate::kernel::{CreateVm, Kernel, RunVm};
use crate::theme;
use bw_app::{Command, Panel, Scope};
use bw_core::model::{drafting_workflow, Cadence, ProjectCycle, StageKind};
use bw_core::{MetricId, ProjectId, SessionId};
use bw_store::{MetricRole, SessionKind};
use dioxus::prelude::*;
use ui::vm::MetricVm;
```

改成:

```rust
use crate::kernel::{CreateVm, Kernel, RunVm};
use crate::theme;
use bw_app::{Command, GithubOrigin, Panel, Scope};
use bw_core::model::{drafting_workflow, Cadence, ProjectCycle, StageKind};
use bw_core::{MetricId, ProjectId, SessionId};
use bw_engine::GithubRepoSummary;
use bw_store::{MetricRole, SessionKind};
use dioxus::prelude::*;
use ui::vm::MetricVm;
```

- [ ] **Step 2: `Card` 枚举加 `Repo`,定义 `RepoChoice`**

把:

```rust
/// Which card of the flow is showing. Local UI navigation only — the real
/// draft lives in [`CreateVm`], sourced from the store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Card {
    Intent,
    Questions,
    Drafting,
    Review,
}
```

改成:

```rust
/// Which card of the flow is showing. Local UI navigation only — the real
/// draft lives in [`CreateVm`], sourced from the store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Card {
    Repo,
    Intent,
    Questions,
    Drafting,
    Review,
}

/// The Repo 卡片's local choice — turned into a `GithubOrigin` only at
/// `IntentCard`'s submit time, once a project name exists to slugify.
#[derive(Clone, Debug, PartialEq)]
enum RepoChoice {
    New { private: bool },
    Existing { owner: String, repo: String },
}
```

- [ ] **Step 3: `Create` 组件——默认卡片改 `Repo`,加 `repo_choice` 信号 + `github_repos` prop,路由加 Repo 分支**

把:

```rust
#[component]
pub fn Create(vm: Option<CreateVm>, run: RunVm, on_cancel: EventHandler<()>) -> Element {
    let has_project = vm.is_some();
    // Resuming an interrupted creation (OpenProject on a cold-start project)
    // skips straight past Intent — the project row already exists.
    let mut card = use_signal(move || {
        if has_project {
            Card::Questions
        } else {
            Card::Intent
        }
    });
    let cadence = use_signal(|| Cadence::Weekly);

    let serif = theme::SERIF;
    let ink2 = theme::INK_2;

    rsx! {
        div {
            style: "max-width:640px;margin:0 auto;padding:36px 24px 120px;display:flex;flex-direction:column;gap:12px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;margin-bottom:8px;",
                span { style: "font-family:{serif};font-size:17px;font-weight:600;", "新建项目" }
                if card() == Card::Intent {
                    button {
                        style: "background:transparent;border:none;color:{ink2};cursor:pointer;font-size:13px;",
                        onclick: move |_| on_cancel.call(()),
                        "← 返回项目墙"
                    }
                }
            }
            match (card(), vm) {
                (Card::Intent, _) => rsx! { IntentCard { on_created: move |_| card.set(Card::Questions) } },
                (_, None) => rsx! { div { "…" } },
                (Card::Questions, Some(v)) => rsx! {
                    QuestionsCard { vm: v, cadence, on_next: move |_| card.set(Card::Drafting) }
                },
                (Card::Drafting, Some(_)) => rsx! {
                    DraftingCard { run, on_next: move |_| card.set(Card::Review) }
                },
                (Card::Review, Some(v)) => rsx! { ReviewCard { vm: v, cadence } },
            }
        }
    }
}
```

改成:

```rust
#[component]
pub fn Create(
    vm: Option<CreateVm>,
    run: RunVm,
    github_repos: Vec<GithubRepoSummary>,
    on_cancel: EventHandler<()>,
) -> Element {
    let has_project = vm.is_some();
    // Resuming an interrupted creation (OpenProject on a cold-start project)
    // skips straight past Repo/Intent — the project row (and its repo, if
    // any) already exists.
    let mut card = use_signal(move || {
        if has_project {
            Card::Questions
        } else {
            Card::Repo
        }
    });
    let cadence = use_signal(|| Cadence::Weekly);
    let repo_choice = use_signal(|| RepoChoice::New { private: true });

    let serif = theme::SERIF;
    let ink2 = theme::INK_2;

    rsx! {
        div {
            style: "max-width:640px;margin:0 auto;padding:36px 24px 120px;display:flex;flex-direction:column;gap:12px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;margin-bottom:8px;",
                span { style: "font-family:{serif};font-size:17px;font-weight:600;", "新建项目" }
                if card() == Card::Repo || card() == Card::Intent {
                    button {
                        style: "background:transparent;border:none;color:{ink2};cursor:pointer;font-size:13px;",
                        onclick: move |_| on_cancel.call(()),
                        "← 返回项目墙"
                    }
                }
            }
            match (card(), vm) {
                (Card::Repo, _) => rsx! {
                    RepoCard { choice: repo_choice, github_repos: github_repos.clone(), on_next: move |_| card.set(Card::Intent) }
                },
                (Card::Intent, _) => rsx! {
                    IntentCard { repo_choice, on_created: move |_| card.set(Card::Questions) }
                },
                (_, None) => rsx! { div { "…" } },
                (Card::Questions, Some(v)) => rsx! {
                    QuestionsCard { vm: v, cadence, on_next: move |_| card.set(Card::Drafting) }
                },
                (Card::Drafting, Some(_)) => rsx! {
                    DraftingCard { run, on_next: move |_| card.set(Card::Review) }
                },
                (Card::Review, Some(v)) => rsx! { ReviewCard { vm: v, cadence } },
            }
        }
    }
}
```

- [ ] **Step 4: 新增 `RepoCard` 组件(插在 `IntentCard` 之前,"0 · 仓从哪来" 一节)**

在 `crates/app-desktop/src/screens/create.rs` 里,`// ───────────────────────── 1 · 意图 ─────────────────────────` 这行注释之前插入:

```rust
// ───────────────────────── 0 · 仓从哪来 ─────────────────────────

#[component]
fn RepoCard(
    choice: Signal<RepoChoice>,
    github_repos: Vec<GithubRepoSummary>,
    on_next: EventHandler<()>,
) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let is_new = matches!(choice(), RepoChoice::New { .. });
    let existing_ready = matches!(&choice(), RepoChoice::Existing { owner, .. } if !owner.is_empty());
    let can_send = is_new || existing_ready;
    let opacity = if can_send { "1" } else { ".45" };

    rsx! {
        div { style: "font-family:{serif};font-size:22px;font-weight:600;margin:14px 0 4px;", "仓从哪来？" }
        p { style: "font-size:12.5px;color:{ink3};margin:0 0 14px;line-height:1.7;", "每个项目背后是一个真实的 GitHub 仓 —— 新建一个,或者接入你已有的。" }

        {chip_question(
            "起点",
            vec![("新建仓", is_new), ("接入已有仓", !is_new)],
            move |i| {
                if i == 0 {
                    choice.set(RepoChoice::New { private: true });
                } else {
                    k.send(Command::ListGithubRepos);
                    choice.set(RepoChoice::Existing { owner: String::new(), repo: String::new() });
                }
            },
        )}

        div {
            style: "{card} padding:18px 20px;margin-top:8px;",
            if is_new {
                {
                    let private = matches!(choice(), RepoChoice::New { private: true });
                    rsx! {
                        {chip_question(
                            "可见性",
                            vec![("Private", private), ("Public", !private)],
                            move |i| choice.set(RepoChoice::New { private: i == 0 }),
                        )}
                    }
                }
            } else {
                label { style: "{theme::label()}", "选一个仓" }
                select {
                    style: "{theme::input()} margin-top:6px;",
                    value: {
                        if let RepoChoice::Existing { owner, repo } = &choice() {
                            format!("{owner}/{repo}")
                        } else {
                            String::new()
                        }
                    },
                    onchange: move |e| {
                        if let Some((owner, repo)) = e.value().split_once('/') {
                            choice.set(RepoChoice::Existing {
                                owner: owner.to_string(),
                                repo: repo.to_string(),
                            });
                        }
                    },
                    option { value: "", "请选择…" }
                    for r in github_repos.iter() {
                        {
                            let value = format!("{}/{}", r.owner, r.repo);
                            let vis = if r.private { "private" } else { "public" };
                            rsx! {
                                option { key: "{value}", value: "{value}", "{value} · {vis}" }
                            }
                        }
                    }
                }
                if github_repos.is_empty() {
                    p { style: "font-size:11.5px;color:{ink3};margin-top:8px;", "没读到仓库列表 —— 确认本机 gh 已登录(gh auth status)。" }
                }
            }
        }
        div {
            style: "display:flex;justify-content:flex-end;margin-top:14px;",
            button {
                style: "{theme::btn_primary()} opacity:{opacity};",
                disabled: !can_send,
                onclick: move |_| on_next.call(()),
                "下一步 →"
            }
        }
    }
}

```

- [ ] **Step 5: `IntentCard` 加 `repo_choice` prop + slug 预览 + `github` 字段传入 `CreateProject`**

把整个 `IntentCard` 组件:

```rust
#[component]
fn IntentCard(on_created: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| KINDS[0].to_string());
    let mut brief = use_signal(String::new);

    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let input = theme::input();
    let label = theme::label();
    let can_send = !name().trim().is_empty() && !brief().trim().is_empty();
    let opacity = if can_send { "1" } else { ".45" };

    let send = move |_| {
        if !can_send {
            return;
        }
        k.send(Command::CreateProject {
            id: ProjectId::new(),
            name: name().trim().to_string(),
            kind: kind(),
            desc: brief().trim().to_string(),

            workspace: None,
        });
        on_created.call(());
    };

    rsx! {
        div { style: "font-family:{serif};font-size:22px;font-weight:600;margin:14px 0 4px;", "你想做什么？" }
        p { style: "font-size:12.5px;color:{ink3};margin:0 0 14px;line-height:1.7;", "一个名字、一句你想做的事。剩下的问题会帮你补全 —— 答不上的交给系统兜底,不编造具体数字。" }
        div {
            style: "{card} padding:18px 20px;",
            div {
                style: "display:grid;grid-template-columns:1.4fr 1fr;gap:12px;margin-bottom:12px;",
                div {
                    label { style: "{label}", "项目名称 *" }
                    input {
                        style: "{input}",
                        placeholder: "例:增长实验看板",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div {
                    label { style: "{label}", "项目类型" }
                    select {
                        style: "{input}",
                        value: "{kind}",
                        onchange: move |e| kind.set(e.value()),
                        for kd in KINDS {
                            option { value: "{kd}", "{kd}" }
                        }
                    }
                }
            }
            label { style: "{label}", "你想做什么 *" }
            textarea {
                style: "{input} min-height:90px;",
                placeholder: "一句话即可,多写几句问题会更少。例:把 agent 会话里长出的工作流沉淀成可复用资产,导入即跑。",
                value: "{brief}",
                oninput: move |e| brief.set(e.value()),
            }
            div {
                style: "display:flex;justify-content:flex-end;margin-top:14px;",
                button {
                    style: "{theme::btn_primary()} opacity:{opacity};",
                    disabled: !can_send,
                    onclick: send,
                    "开始 ↑"
                }
            }
        }
        p { style: "font-size:11.5px;color:{ink3};margin:10px 2px 0;", "提交后即建立项目;之后的问答与起草随时可编辑,确认后才正式生效。" }
    }
}
```

整体替换为:

```rust
/// GitHub 仓名要求 ASCII + 连字符;项目显示名允许中文。两个独立字段(用户
/// 已确认),这个纯函数只给"新建仓"分支的实时预览用——真正发去 `gh` 的值
/// 是用户可能手改过的 `slug` 信号,不是每次都重新静默转写。
fn slugify(name: &str) -> String {
    let base: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if base.is_empty() {
        "project".to_string()
    } else {
        base
    }
}

#[component]
fn IntentCard(repo_choice: Signal<RepoChoice>, on_created: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| KINDS[0].to_string());
    let mut brief = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut slug_touched = use_signal(|| false);

    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let input = theme::input();
    let label = theme::label();
    let can_send = !name().trim().is_empty() && !brief().trim().is_empty();
    let opacity = if can_send { "1" } else { ".45" };
    let is_new_repo = matches!(repo_choice(), RepoChoice::New { .. });

    let send = move |_| {
        if !can_send {
            return;
        }
        let github = match repo_choice() {
            RepoChoice::New { private } => Some(GithubOrigin::New {
                slug: if slug().trim().is_empty() {
                    slugify(&name())
                } else {
                    slug().trim().to_string()
                },
                private,
            }),
            RepoChoice::Existing { owner, repo } => Some(GithubOrigin::Existing { owner, repo }),
        };
        k.send(Command::CreateProject {
            id: ProjectId::new(),
            name: name().trim().to_string(),
            kind: kind(),
            desc: brief().trim().to_string(),
            workspace: None,
            github,
        });
        on_created.call(());
    };

    rsx! {
        div { style: "font-family:{serif};font-size:22px;font-weight:600;margin:14px 0 4px;", "你想做什么？" }
        p { style: "font-size:12.5px;color:{ink3};margin:0 0 14px;line-height:1.7;", "一个名字、一句你想做的事。剩下的问题会帮你补全 —— 答不上的交给系统兜底,不编造具体数字。" }
        div {
            style: "{card} padding:18px 20px;",
            div {
                style: "display:grid;grid-template-columns:1.4fr 1fr;gap:12px;margin-bottom:12px;",
                div {
                    label { style: "{label}", "项目名称 *" }
                    input {
                        style: "{input}",
                        placeholder: "例:增长实验看板",
                        value: "{name}",
                        oninput: move |e| {
                            name.set(e.value());
                            if !slug_touched() {
                                slug.set(slugify(&name()));
                            }
                        },
                    }
                }
                div {
                    label { style: "{label}", "项目类型" }
                    select {
                        style: "{input}",
                        value: "{kind}",
                        onchange: move |e| kind.set(e.value()),
                        for kd in KINDS {
                            option { value: "{kd}", "{kd}" }
                        }
                    }
                }
            }
            label { style: "{label}", "你想做什么 *" }
            textarea {
                style: "{input} min-height:90px;",
                placeholder: "一句话即可,多写几句问题会更少。例:把 agent 会话里长出的工作流沉淀成可复用资产,导入即跑。",
                value: "{brief}",
                oninput: move |e| brief.set(e.value()),
            }
            if is_new_repo {
                div {
                    style: "margin-top:10px;",
                    label { style: "{label}", "GitHub 仓名(可改)" }
                    input {
                        style: "{input} font-family:{theme::MONO};",
                        placeholder: "growth-kanban",
                        value: "{slug}",
                        oninput: move |e| {
                            slug_touched.set(true);
                            slug.set(e.value());
                        },
                    }
                }
            } else if let RepoChoice::Existing { owner, repo } = repo_choice() {
                p { style: "font-size:11.5px;color:{ink3};margin-top:10px;", "将接入 {owner}/{repo} ↗" }
            }
            div {
                style: "display:flex;justify-content:flex-end;margin-top:14px;",
                button {
                    style: "{theme::btn_primary()} opacity:{opacity};",
                    disabled: !can_send,
                    onclick: send,
                    "开始 ↑"
                }
            }
        }
        p { style: "font-size:11.5px;color:{ink3};margin:10px 2px 0;", "提交后即建立项目;之后的问答与起草随时可编辑,确认后才正式生效。" }
    }
}
```

- [ ] **Step 6: `op.rs`——`WorkspaceConfig` 只读态加 GitHub 徽记**

在 `crates/app-desktop/src/screens/op.rs`,把:

```rust
                if configured {
                    span {
                        style: "font-family:{mono};font-size:12.5px;color:{ink2};flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                        "{op.workspace_path}"
                    }
                    span { style: "font-size:11px;color:{ink3};flex:none;", "{permission_label}" }
                } else {
```

改成:

```rust
                if configured {
                    span {
                        style: "font-family:{mono};font-size:12.5px;color:{ink2};flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                        "{op.workspace_path}"
                    }
                    if !op.github_remote.trim().is_empty() {
                        span {
                            style: "font-size:11px;color:{ink3};flex:none;",
                            "GitHub · {op.github_remote}"
                        }
                    }
                    span { style: "font-size:11px;color:{ink3};flex:none;", "{permission_label}" }
                } else {
```

- [ ] **Step 7: `main.rs`——`Create { .. }` 调用点补 prop**

在 `crates/app-desktop/src/main.rs`,把:

```rust
                } else if show_create {
                    Create {
                        vm: v.create.clone(),
                        run: run(),
                        on_cancel: move |_| creating.set(false),
                    }
```

改成:

```rust
                } else if show_create {
                    Create {
                        vm: v.create.clone(),
                        run: run(),
                        github_repos: v.github_repos.clone(),
                        on_cancel: move |_| creating.set(false),
                    }
```

- [ ] **Step 8: 编译 + 全量门禁**

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop
```

Expected: 全部通过——这正是 CLAUDE.md 里"每个 commit 前全过"的门禁列表。`cargo fmt --all --check` 如果因为 Step 1-7 手写的 rsx 缩进不完全匹配 rustfmt 输出而失败,直接跑 `cargo fmt --all` 修正再复查一遍 diff 没有语义变化。

- [ ] **Step 9: Commit**

```bash
git add crates/app-desktop/src/screens/create.rs crates/app-desktop/src/screens/op.rs crates/app-desktop/src/main.rs
git commit -m "$(cat <<'EOF'
app-desktop UI · Repo 卡片(新建/接入)+ Intent 卡 slug 预览 + GitHub 徽记

新 Card::Repo 插在意图卡之前:新建仓(可见性开关)/接入已有仓(gh repo
list 下拉,选中即触发 ListGithubRepos)。Intent 卡加 slug 实时预览/可编辑
(项目显示名允许中文,GitHub 仓名要求 ASCII)。op.rs 工作区面板加一行只读
GitHub 徽记。全套门禁(fmt/clippy/双 wasm32 check/guard 脚本/桌面壳编译)过。

Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: 全量门禁 + 真实 E2E 验证

**Files:** 无新增/修改代码——本任务只跑验证命令、必要时回头小修上面任务里的 bug。

**Interfaces:** 无(终点任务)。

**这个任务里两步会在你真实 GitHub 账号(`forcegravity1989`)上创建/操作真实、公开可见的仓库状态——执行前必须先把要建的仓名、可见性告诉用户并等明确确认;完成验证后要问是否 `gh repo delete` 清理测试仓。这不是可以静默跳过的步骤,也不是可以自作主张替换成"假装验证过"的步骤。**

- [ ] **Step 1: 全量门禁复跑(确认前面 5 个任务叠加后仍然全过)**

```bash
cargo fmt --all --check
cargo clippy --workspace --exclude app-desktop -- -D warnings
cargo check -p bw-core --target wasm32-unknown-unknown --no-default-features
cargo check -p ui --target wasm32-unknown-unknown
./scripts/guard-kernel-ui-free.sh
cargo check -p app-desktop
```

Expected: 全部通过。

- [ ] **Step 2: 准备一次性 scratch DB + workspaces 根目录**

```bash
mkdir -p /tmp/bw-github-e2e/workspaces
rm -f /tmp/bw-github-e2e/bw.db
export BW_DB=/tmp/bw-github-e2e/bw.db
export BW_WORKSPACES=/tmp/bw-github-e2e/workspaces
```

- [ ] **Step 3(需要用户确认后再执行): 真实"新建仓"全流程,深链启动 + 界面驱动**

先在对话里明确报告:即将在真实账号 `forcegravity1989` 下创建一个名为 `bw-onboarding-e2e-<当天日期>` 的 **private** 仓库,用于验证本次功能,验证完会询问是否删除。等用户确认后:

```bash
cargo build -p app-desktop
BW_DB=/tmp/bw-github-e2e/bw.db BW_WORKSPACES=/tmp/bw-github-e2e/workspaces \
  target/debug/builders-workbench &
```

用 computer-use 或截图驱动:点"+ 新建项目" → Repo 卡片选"新建仓"(默认 private)→"下一步"→ Intent 卡填项目名(比如"GitHub 引导流验证"),把 GitHub 仓名预览改成 `bw-onboarding-e2e-<当天日期>`,填一句 brief →"开始 ↑"。

Expected:界面乐观跳到 Questions 卡(不等待、无 loading 卡死);几秒内本地 `/tmp/bw-github-e2e/workspaces/bw-onboarding-e2e-<日期>/` 目录出现且含 `.git`、`README.md`、`.claude/standards/*.md` 四份文件、`PROJECT.md`。

- [ ] **Step 4: sqlite 读回核对**

```bash
sqlite3 /tmp/bw-github-e2e/bw.db "SELECT name, workspace_path, github_remote FROM project;"
sqlite3 /tmp/bw-github-e2e/bw.db "SELECT name, kind, config FROM connector WHERE kind='github-repo';"
gh repo view forcegravity1989/bw-onboarding-e2e-<日期> --json isPrivate,url
```

Expected:`project.github_remote` = `forcegravity1989/bw-onboarding-e2e-<日期>`;`connector` 表里有一行 `kind='github-repo'`,`config` 同一个 `owner/repo`;`gh repo view` 证实远端真的创建成功、`isPrivate=true`;本地 `git log`(在克隆目录里跑 `git log --oneline`)能看到 workbench 的首次提交 + 章程 + 标准文件提交,且已推到远端(`git log origin/HEAD..HEAD` 应为空,证明推送成功)。

- [ ] **Step 5: 故意验证失败路径——接入一个不存在的仓**

不需要额外确认(这一步不创建任何真实远端状态,只是让 `clone_repo` 失败):新建第二个项目,Repo 卡片选"接入已有仓",但用一个确定不存在的 `owner/repo`(比如手动构造场景,或直接改一次 kernel 里 `clone_repo` 的入参做临时验证——如果 UI 下拉只能选真实存在的仓,改用一次性小测试:直接跑 `cargo run --example` 风格的最小指挥器调用 `Command::CreateProject` 带一个不存在的 `GithubOrigin::Existing`)。

```bash
sqlite3 /tmp/bw-github-e2e/bw.db "SELECT name, workspace_path, github_remote FROM project ORDER BY created_at DESC LIMIT 1;"
```

Expected:该项目 `workspace_path` 和 `github_remote` 都是空字符串(没有兜底 mint 一个无关本地仓),项目本身仍然正常存在于墙上(`CreateProject` 没有整体失败)。

- [ ] **Step 6(需要用户确认后再执行): 清理测试仓**

验证通过后,在对话里询问是否删除 Step 3 建的真实测试仓,得到明确同意后:

```bash
gh repo delete forcegravity1989/bw-onboarding-e2e-<日期> --yes
```

不同意就保留,把仓名记下来告知用户。

- [ ] **Step 7: `/code-review` 过一遍全部改动**

对本计划 6 个任务累计的 diff(`git diff main...HEAD` 或对应 commit range)跑一次 `/code-review`,按 CLAUDE.md"代码质量靠 /code-review,不靠测试基线"的纪律,处理它给出的发现。

- [ ] **Step 8: 收尾——不需要额外 commit**

如果 Step 1-7 过程中因为真实验证发现了 bug 并回头改了前面任务的代码,那些修复按正常流程各自 commit(小步提交,清楚说明修的是什么);如果一路绿灯没有修复,本任务不产生新 commit。

---

## Self-Review Notes

- **Spec 覆盖**:设计文档 §2(Repo/Intent 卡)→ Task5;§3(github.rs)→ Task2;§4(Command/handler)→ Task3;§5(schema/VM)→ Task1+4;§6(失败表)→ Task3 的三路分叉 + Task6 Step5;§7(验证计划)→ Task6;§8(文件清单)→ 六个任务的 Files 段逐一对应。没有遗漏项。
- **占位符扫描**:全文没有 TBD/"补充"/"类似 Task N"——Task 6 Step 5 的失败路径验证因为依赖 UI 下拉限制,给了两种可执行的具体方式而不是留白。
- **类型一致性**:`GithubOrigin::New{slug,private}` / `Existing{owner,repo}` 在 Task3(定义)、Task5 的 `IntentCard.send`(构造)、Task3 的 handler(解构)三处字段名和顺序一致;`RepoChoice`(UI-only)与 `GithubOrigin`(Command-level)故意是两个独立类型,不混用——`RepoChoice` 在 Task5 定义并只在 create.rs 内部使用,转换发生在 `IntentCard.send` 内,没有跨文件类型不一致的风险。`bw_engine::github::{create_repo, clone_repo, list_repos}` 的签名在 Task2(定义)和 Task3(调用)完全一致(`create_repo(&slug, private, &root, &proj.name, &body)` 五个位置参数顺序、`clone_repo(&owner, &repo, &dir)` 三个位置参数顺序都对得上)。
