//! **collector_demo — C7 采集器 headless E2E 指挥器(plan/13 D7)。**
//!
//! 不开 UI,把标配采集器的完整行为走一遍,全部经真实命令层(`dispatch` /
//! `tick_scheduler`),结论一律 `sqlite3` 读回为证:
//!   ① 创建一个挂 GitHub 仓的项目(`CreateProject` github=New)→ 标配每日
//!      采集 `cron_task` 随之自动建立;
//!   ② 同步样例 `.bw/metrics.toml`(`SyncMetricsFile`)→ github/bw/connector/
//!      manual 四类指标入库;
//!   ③ 「立即采集」(`CollectMetrics`)→ 只有 github 指标被真采,signal 由
//!      Unknown 变派生值;bw/connector 如实留白(零观测、Unknown);
//!   ④ 同窗口重复采集 → change-guard 生效,observation 不涨;值变了才 +1 点;
//!   ⑤ gh 失败 → 零新观测、ok:false toast、signal 不假绿;
//!   ⑥ 到点 tick(`tick_scheduler`)→ 标配 cron 真实触发,再采一点。
//!
//! **gh 全程被一个自带的【mock】stub 顶替**(本例运行时把它写进
//! `<ws_root>/.stub-bin/gh` 并前置进 PATH)——它自我标注为 mock,绝不冒充
//! 真实 GitHub。计数值由 `STUB_GH_COUNT` 控,失败由 `STUB_GH_FAIL` 控,本例
//! 在各阶段之间自己切换,得到确定的观测序列 [7, 9, 11]。真实账号 gh 的端到端
//! 是另一票(plan/13 测试拍板:真账号 E2E 单独成票)。
//!
//! 用法:
//!   cargo run -p bw-app --example collector_demo -- <db-path> <workspaces-root>

use bw_app::{App, Command, Event, GithubOrigin};
use bw_core::ProjectId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SqliteStore, Store};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

const SLUG: &str = "collector-demo";

/// A self-labeled 【mock】 `gh` — NOT real GitHub. Handles exactly the three
/// subcommands this scenario reaches: `api user` (login), `repo create …
/// --clone` (a fully-offline bare+clone so the real create_repo push
/// succeeds), and `api -X GET search/issues … --jq .total_count` (the count,
/// from `STUB_GH_COUNT`, or a forced failure via `STUB_GH_FAIL`).
const STUB_GH: &str = r#"#!/bin/sh
# 【mock】stub gh for C7 collector E2E — self-labeled, NOT real GitHub.
if [ "$1" = "api" ] && [ "$2" = "user" ]; then
  echo "testowner"
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "create" ]; then
  slug="$3"
  bare="$STUB_GH_REMOTES/$slug.git"
  mkdir -p "$STUB_GH_REMOTES"
  git init --bare -q "$bare"
  git clone -q "$bare" "$slug"
  echo "https://github.com/testowner/$slug"
  exit 0
fi
if [ "$1" = "api" ] && [ "$2" = "-X" ]; then
  # api -X GET search/issues -f q=... --jq .total_count
  if [ -n "$STUB_GH_FAIL" ]; then
    echo "【mock】gh stub forced failure (STUB_GH_FAIL)" 1>&2
    exit 1
  fi
  echo "${STUB_GH_COUNT:-7}"
  exit 0
fi
# any other gh call: benign no-op
exit 0
"#;

const METRICS_TOML: &str = r#"schema_version = 1

[north_star]
name = "周活跃创作者数"
def  = "过去 7 天内至少发布过一篇正式内容的注册用户数"
collect = { kind = "connector", query = "content-analytics" }

[[lagging]]
name   = "月流失率"
def    = "上月活跃、本月零发布的用户占比"
target = "≤8%"
collect = { kind = "connector", query = "content-analytics" }

[[leading]]
name   = "每周合并 PR 数"
def    = "过去 7 天内 merge 进 main 的 PR 数"
target = "≥5"
collect = { kind = "github", query = "repo:{owner}/{repo} is:pr is:merged merged:>=@{7d}" }

[[leading]]
name   = "队友结算 Issue 数"
def    = "过去 7 天内被人 merge 关闭的 Issue 数"
target = "≥3"
collect = { kind = "bw", query = "issue.settled_at within 7d" }

[[leading]]
name   = "手填留存"
def    = "暂无埋点,人手填"
target = "≥35%"
collect = { kind = "manual", query = "" }
"#;

async fn dump_signals(store: &Arc<dyn Store>, project: ProjectId, tag: &str) {
    let sigs = store.persisted_signals(project).await.unwrap();
    println!("  ── signals @ {tag} ──");
    for m in &sigs.metrics {
        println!(
            "    [{:>9}] {:<14} signal={:?} value={:?}",
            m.collect_kind, m.name, m.signal, m.value_raw
        );
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let db_path = args
        .get(1)
        .cloned()
        .expect("usage: collector_demo <db-path> <workspaces-root>");
    let ws_root = PathBuf::from(
        args.get(2)
            .cloned()
            .expect("usage: collector_demo <db-path> <workspaces-root>"),
    );
    std::fs::create_dir_all(&ws_root).unwrap();

    // Write the self-contained 【mock】 gh stub and put it first on PATH.
    let stub_bin = ws_root.join(".stub-bin");
    std::fs::create_dir_all(&stub_bin).unwrap();
    let gh = stub_bin.join("gh");
    std::fs::write(&gh, STUB_GH).unwrap();
    std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", stub_bin.display(), old_path));
    std::env::set_var(
        "STUB_GH_REMOTES",
        ws_root.join(".stub-remotes").display().to_string(),
    );
    println!("[stub] gh → {} (【mock】, NOT real GitHub)", gh.display());

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig {
            binary: None,
            max_budget_usd: 0.0,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    )
    .with_workspaces_root(ws_root.clone());
    app.dispatch(Command::Boot).await.expect("boot");

    // Print every honest toast (ConnectorSynced) so ok:true / ok:false shows.
    let mut rx = app.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let Event::ConnectorSynced { name, ok, detail } = ev {
                println!("  [toast] {} · ok={} · {}", name, ok, detail);
            }
        }
    });

    // ① CreateProject (github=New) — stub gh mints the repo offline; the
    // standard daily collect cron is auto-created as a side effect.
    println!("\n① CreateProject github=New …");
    let project = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: project,
        name: "采集器演示".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C7 采集器 E2E".into(),
        workspace: None,
        github: Some(GithubOrigin::New {
            slug: SLUG.into(),
            private: true,
        }),
    })
    .await
    .expect("create project");
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let proj = store.get_project(project).await.unwrap().unwrap();
    println!("  github_remote = {:?}", proj.github_remote);
    println!("  workspace     = {:?}", proj.workspace_path);

    // ② SyncMetricsFile — write the sample metrics.toml into the real workspace.
    println!("\n② SyncMetricsFile …");
    let bw_dir = PathBuf::from(&proj.workspace_path).join(".bw");
    std::fs::create_dir_all(&bw_dir).unwrap();
    std::fs::write(bw_dir.join("metrics.toml"), METRICS_TOML).unwrap();
    app.dispatch(Command::SyncMetricsFile).await.expect("sync");
    dump_signals(&store, project, "post-sync (before any collect)").await;

    // ③ CollectMetrics — count 7. Only the github metric is really pulled.
    println!("\n③ CollectMetrics (STUB_GH_COUNT=7) …");
    std::env::set_var("STUB_GH_COUNT", "7");
    app.dispatch(Command::CollectMetrics)
        .await
        .expect("collect");
    dump_signals(&store, project, "post-collect #1 (count=7)").await;

    // ④a repeat, same count → change-guard, no new observation.
    println!("\n④a CollectMetrics repeat (STUB_GH_COUNT=7) — change-guard …");
    app.dispatch(Command::CollectMetrics)
        .await
        .expect("collect");

    // ④b value changed → new observation point.
    println!("\n④b CollectMetrics (STUB_GH_COUNT=9) — value changed …");
    std::env::set_var("STUB_GH_COUNT", "9");
    app.dispatch(Command::CollectMetrics)
        .await
        .expect("collect");

    // ⑤ gh failure → zero new observations, ok:false toast, no fake green.
    println!("\n⑤ CollectMetrics (STUB_GH_FAIL=1) — failure must write nothing …");
    std::env::set_var("STUB_GH_FAIL", "1");
    app.dispatch(Command::CollectMetrics)
        .await
        .expect("collect");
    std::env::remove_var("STUB_GH_FAIL");
    dump_signals(
        &store,
        project,
        "post-failure (signal must stay derived, not green-0)",
    )
    .await;

    // ⑥ tick_scheduler — the standard cron fires for real (count 11).
    println!("\n⑥ tick_scheduler (STUB_GH_COUNT=11) — standard cron auto-fires …");
    std::env::set_var("STUB_GH_COUNT", "11");
    let fired = app.tick_scheduler().await.expect("tick");
    println!("  fired cron tasks: {}", fired.len());
    dump_signals(&store, project, "post-cron (count=11)").await;

    // Final observation series for the github metric (the load-bearing readback).
    let sigs = store.persisted_signals(project).await.unwrap();
    let gh_metric = sigs
        .metrics
        .iter()
        .find(|m| m.collect_kind == "github")
        .expect("github metric");
    let obs = store.list_observations(project).await.unwrap();
    let mut series: Vec<String> = obs
        .iter()
        .filter(|o| o.metric_id == gh_metric.id)
        .map(|o| o.raw.clone())
        .collect();
    println!("\n══ github metric observation series (append-only) ══");
    println!(
        "  {:?}  (expect [\"7\", \"9\", \"11\"] — 7-repeat guarded, failure wrote nothing)",
        series
    );
    series.dedup();
    assert_eq!(
        series,
        vec!["7", "9", "11"],
        "change-guard / no-fake-on-failure invariant violated"
    );
    println!("\n✓ collector_demo done — project_id = {}", project.uuid());
    println!("  DB: {db_path}");
}
