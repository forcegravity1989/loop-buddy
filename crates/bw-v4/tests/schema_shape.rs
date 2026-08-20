//! 库形状的读回:恰好四张表、`issue` 九个扩展列齐、被取消的表一张都不在。
//!
//! 这不是「补测试当交付物」——它是 §2.4 第 3 组验收读回的可执行版本,跑在
//! CI 里,免得哪次改 `schema.sql` 悄悄把第五张表加回来。

use bw_v4::V4Store;

async fn open_tmp(name: &str) -> (V4Store, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("bw-v4-test-{name}.db"));
    let _ = std::fs::remove_file(&path);
    let store = V4Store::open(path.to_str().unwrap()).await.unwrap();
    (store, path)
}

#[tokio::test]
async fn exactly_four_tables() {
    let (store, path) = open_tmp("four-tables").await;
    let tables = store.table_names().await.unwrap();
    assert_eq!(
        tables,
        vec![
            "app_meta".to_string(),
            "claude_conversation".to_string(),
            "issue".to_string(),
            "project".to_string()
        ],
        "V4 库只有四张表,别的一律不建"
    );
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn cancelled_tables_never_exist() {
    let (store, path) = open_tmp("cancelled").await;
    let tables = store.table_names().await.unwrap();
    for t in [
        "agent",
        "release",
        "release_issue",
        "week_plan",
        "issue_metric",
        "workflow_credit",
        "chat_outbox",
        "skill_package",
        "observation",
        "metric",
        "workflow_run",
        "artifact",
        "cron_task",
        "connector",
        "skill",
        "skill_file",
        "skill_stage",
        "workflow_spec",
        "workflow_version",
        "op_stage",
        "handoff",
        "session",
        "knowledge_source",
    ] {
        assert!(!tables.iter().any(|n| n == t), "{t} 表在 V4 从未存在过");
    }
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn issue_cache_columns_present_and_no_assignee() {
    let (store, path) = open_tmp("issue-cols").await;
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info('issue')")
        .fetch_all(store.pool())
        .await
        .unwrap();
    let cols: Vec<String> = rows.into_iter().map(|(n,)| n).collect();
    for c in [
        "week_of",
        "version",
        "tool",
        "kind",
        "origin",
        "workflow",
        "category",
        "sort_order",
        "metric_key",
    ] {
        assert!(cols.iter().any(|n| n == c), "issue 缺缓存列 {c}");
    }
    assert!(
        !cols.iter().any(|n| n == "assignee"),
        "V4 不指派队友,issue 不该有 assignee 列"
    );
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn project_has_no_namecard_or_chat_or_version_columns() {
    let (store, path) = open_tmp("project-cols").await;
    let rows: Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info('project')")
        .fetch_all(store.pool())
        .await
        .unwrap();
    let cols: Vec<String> = rows.into_iter().map(|(n,)| n).collect();
    for c in [
        "standard_version",
        "current_version",
        "chat_provider",
        "chat_group_id",
        "chat_notify",
        "descr",
        "benchmark",
        "north_star",
    ] {
        assert!(
            !cols.iter().any(|n| n == c),
            "名片/群/版本的正本在仓文件,project 表不该有 {c} 列"
        );
    }
    let _ = std::fs::remove_file(path);
}
