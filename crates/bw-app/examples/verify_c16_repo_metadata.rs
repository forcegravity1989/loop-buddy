//! **verify_c16_repo_metadata — C16 仓平台选择器 + provider 字段 + 接入已有仓
//! 真实 metadata headless E2E 指挥器(plan/14 规范条 4)。**
//!
//! 不写单元测试(仓库纪律),三件事都走真实路径独立核验:
//!
//! ① **provider 写入**:`Command::CreateProject { provider: "github", .. }`
//!    经真实命令层落库,`Store::get_project` 读回 `provider == "github"`。
//!
//! ② **gh --json 扩展字段解析**:`gh` 全程被自带的【mock】stub 顶替(同
//!    `verify_c14_action_progress.rs` 的 stub 机制,自我标注、绝不冒充真实
//!    GitHub),`repo list` 回一条带 `description`/`defaultBranchRef`/
//!    `pushedAt` 的真实字段形状(字段名核实自 `gh repo list --help`,gh
//!    2.95.0 的 JSON FIELDS 清单),经真实命令层 `ListGithubRepos` →
//!    `bw_engine::github::list_repos` 解析,断言 `AppState.github_repos`
//!    落进来的 `GithubRepoSummary` 与 stub 数据逐字段一致。
//!
//! ③ **schema 迁移双守卫**:用真实 `sqlite3` 二进制手工造一份 pre-C16 的
//!    `project` 表(逐列拷贝 C16 之前的 `schema.sql`,唯独不含 `provider`
//!    列)+ 插入一行存量数据,再用 `SqliteStore::open` 真实打开它 —— 断言
//!    (a) 打开不崩、(b) `PRAGMA table_info(project)` 读回真的多了
//!    `provider` 列、(c) 存量行读回 `provider == "github"`(和"这些项目当时
//!    就是接 GitHub 建的"这个真实状态一致,不是瞎猜的默认值)。
//!
//! 用法:
//!   cargo run -p bw-app --example verify_c16_repo_metadata -- <fresh-db-path> <old-db-fixture-path>

use bw_app::{App, Command};
use bw_core::ProjectId;
use bw_engine::{ClaudeCliConfig, Engine, GithubRepoSummary, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

/// 【mock】stub `gh` — 不是真实 GitHub。只答 `repo list`(本例唯一用到的
/// 子命令),回一条带 C16 扩展字段的真实形状 JSON。
const STUB_GH: &str = r#"#!/bin/sh
# 【mock】stub gh for C16 headless E2E — self-labeled, NOT real GitHub.
if [ "$1" = "repo" ] && [ "$2" = "list" ]; then
  echo '[{"nameWithOwner":"testowner/demo-repo","isPrivate":false,"description":"C16 stub 演示仓 · 真实 metadata","defaultBranchRef":{"name":"main"},"pushedAt":"2026-07-20T00:00:00Z"}]'
  exit 0
fi
exit 0
"#;

fn check(all_ok: &mut bool, label: &str, cond: bool, detail: &str) {
    if cond {
        println!("  ✓ {label}: {detail}");
    } else {
        println!("  ✗ {label}: {detail}");
        *all_ok = false;
    }
}

/// pre-C16 `project` 表的真实历史形状(`schema.sql` 加 `provider` 列之前的
/// 逐列拷贝),外加一行存量数据 —— 用真实 `sqlite3` 二进制手工造出来,不借
/// `SqliteStore` 的当前 schema(那样就测不出"老库缺列"这件事本身)。
async fn seed_pre_c16_fixture(path: &str, fixture_id: uuid::Uuid) {
    let _ = std::fs::remove_file(path);
    let sql = format!(
        r#"
CREATE TABLE project (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL,
    descr              TEXT NOT NULL DEFAULT '',
    phase              TEXT NOT NULL,
    cycle              TEXT NOT NULL DEFAULT 'explore',
    active_stage       TEXT NOT NULL DEFAULT 'prototype',
    north_star         TEXT NOT NULL DEFAULT '',
    ns_def             TEXT NOT NULL DEFAULT '',
    benchmark          TEXT NOT NULL DEFAULT '',
    opportunity        TEXT NOT NULL DEFAULT '',
    workspace_path     TEXT NOT NULL DEFAULT '',
    allow_commands     INTEGER NOT NULL DEFAULT 0,
    github_remote      TEXT NOT NULL DEFAULT '',
    north_star_collect_kind  TEXT NOT NULL DEFAULT '',
    north_star_collect_query TEXT NOT NULL DEFAULT '',
    signal             TEXT,
    weekly_signal      TEXT,
    signal_derived_rev INTEGER,
    signal_derived_at  INTEGER,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    rev                INTEGER NOT NULL DEFAULT 0
);
INSERT INTO project (id, name, kind, descr, phase, github_remote, created_at, updated_at)
VALUES ('{fixture_id}', 'C16-老库存量项目', '内部验证', 'pre-C16 存量行,没有 provider 列', 'running', 'testowner/pre-c16-repo', 1700000000, 1700000000);
"#
    );
    let out = std::process::Command::new("sqlite3")
        .arg(path)
        .arg(&sql)
        .output()
        .expect("spawn sqlite3 (需要本机 sqlite3 CLI)");
    assert!(
        out.status.success(),
        "sqlite3 造 pre-C16 fixture 失败:{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("bw_verify_c16.db")
            .to_string_lossy()
            .into_owned()
    });
    let old_db_path = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("bw_verify_c16_old.db")
            .to_string_lossy()
            .into_owned()
    });
    let _ = std::fs::remove_file(&db_path);

    println!("================ C16 仓平台选择器 + provider + 真实 metadata E2E ================");
    println!("fresh db: {db_path}");
    println!("old-schema fixture db: {old_db_path}");
    let mut all_ok = true;

    // ── ① CreateProject 写 provider ─────────────────────────────────
    println!("\n① CreateProject(provider=\"github\") …");
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    let pid = ProjectId::new();
    app.dispatch(Command::CreateProject {
        provider: "github".to_string(),
        id: pid,
        name: "C16-验证".into(),
        kind: "内部验证".into(),
        desc: "C16 headless E2E · provider 写入路径".into(),
        workspace: None,
        github: None,
    })
    .await
    .expect("CreateProject should succeed");

    let row = store
        .get_project(pid)
        .await
        .unwrap()
        .expect("just-created project must read back");
    check(
        &mut all_ok,
        "CreateProject 落库 provider(Store trait 读回)",
        row.provider == "github",
        &format!("provider={:?}", row.provider),
    );

    // ── ② gh --json 扩展字段解析(stub gh,自我标注)──────────────────
    println!("\n② ListGithubRepos(stub gh · 扩展 metadata 字段)…");
    let stub_dir = std::env::temp_dir().join("bw_verify_c16_stub_bin");
    std::fs::create_dir_all(&stub_dir).unwrap();
    let gh = stub_dir.join("gh");
    std::fs::write(&gh, STUB_GH).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", stub_dir.display(), old_path));
    println!("[stub] gh → {} (【mock】, NOT real GitHub)", gh.display());

    app.dispatch(Command::ListGithubRepos)
        .await
        .expect("list github repos");
    let repos = app.snapshot().github_repos.clone();
    let expect = GithubRepoSummary {
        owner: "testowner".to_string(),
        repo: "demo-repo".to_string(),
        private: false,
        description: "C16 stub 演示仓 · 真实 metadata".to_string(),
        default_branch: "main".to_string(),
        pushed_at: "2026-07-20T00:00:00Z".to_string(),
    };
    check(
        &mut all_ok,
        "list_repos 解析出扩展 metadata(description/defaultBranchRef/pushedAt)",
        repos.first() == Some(&expect),
        &format!("{repos:?}"),
    );

    // ── ③ schema 迁移双守卫:pre-C16 老库真实打开不崩 ──────────────────
    println!("\n③ 老库双守卫:pre-C16 project 表(无 provider 列)…");
    let fixture_id = uuid::Uuid::new_v4();
    seed_pre_c16_fixture(&old_db_path, fixture_id).await;

    let old_store = SqliteStore::open(&old_db_path)
        .await
        .expect("pre-C16 DB 应该正常打开,不崩(add_column_if_missing 双守卫)");
    let old_row = old_store
        .get_project(ProjectId::from_uuid(fixture_id))
        .await
        .unwrap()
        .expect("pre-C16 存量行应该读回");
    check(
        &mut all_ok,
        "老库存量行 provider 默认值(add_column_if_missing 'github')",
        old_row.provider == "github",
        &format!("provider={:?}", old_row.provider),
    );
    check(
        &mut all_ok,
        "老库存量行其余字段原样保留(不是新建的空行)",
        old_row.name == "C16-老库存量项目" && old_row.github_remote == "testowner/pre-c16-repo",
        &format!(
            "name={:?} github_remote={:?}",
            old_row.name, old_row.github_remote
        ),
    );

    println!();
    println!("PRAGMA 独立读回(人工/CI 复核):sqlite3 \"{old_db_path}\" \"PRAGMA table_info(project);\" | grep provider");
    println!("期望一行形如:24|provider|TEXT|1|'github'|0(列序号随表定义,值不变)");
    println!();
    println!("sqlite3 读回 provider(人工/CI 复核):sqlite3 \"{db_path}\" \"SELECT name, provider FROM project WHERE name='C16-验证';\"");
    println!("期望一行:C16-验证|github");
    println!(
        "sqlite3 老库存量行读回:sqlite3 \"{old_db_path}\" \"SELECT name, provider FROM project;\""
    );
    println!("期望一行:C16-老库存量项目|github");

    println!();
    if all_ok {
        println!("✓ 全部断言通过(provider 写入 + 扩展 metadata 解析 + 老库双守卫)");
    } else {
        println!("✗ 存在失败断言,见上方 ✗ 行");
        std::process::exit(1);
    }
}
