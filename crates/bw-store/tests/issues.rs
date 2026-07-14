//! R1 Issue layer — store-level checks: per-project numbering, default status,
//! transitions, assignment, filtered listing, and full field round-trip.

use bw_core::model::{IssuePriority, IssueStatus, StageKind};
use bw_core::{AgentId, IssueId, ProjectId};
use bw_store::{NewIssue, NewProject, SqliteStore, Store};

fn tmp_db() -> String {
    let p = std::env::temp_dir().join(format!("bw_store_issue_test_{}.db", uuid::Uuid::new_v4()));
    p.to_string_lossy().into_owned()
}

#[tokio::test]
async fn issue_create_numbers_transition_assign_and_list_filters() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let project = ProjectId::new();

    store
        .create_project(NewProject {
            id: project,
            name: "Issue 测试项目".into(),
            kind: "y".into(),
            desc: String::new(),
        })
        .await
        .unwrap();

    // Create 2 issues in different stages.
    let i1 = IssueId::new();
    let i2 = IssueId::new();
    store
        .create_issue(NewIssue {
            id: i1,
            project_id: project,
            stage: StageKind::Prototype,
            title: "原型调研".into(),
            desc: "竞品分析 + 用户访谈".into(),
            priority: IssuePriority::High,
        })
        .await
        .unwrap();
    store
        .create_issue(NewIssue {
            id: i2,
            project_id: project,
            stage: StageKind::Build,
            title: "搭建脚手架".into(),
            desc: String::new(),
            priority: IssuePriority::Medium,
        })
        .await
        .unwrap();

    // Numbers are 1 then 2 per project.
    let got1 = store.get_issue(i1).await.unwrap().unwrap();
    let got2 = store.get_issue(i2).await.unwrap().unwrap();
    assert_eq!(got1.number, 1);
    assert_eq!(got2.number, 2);

    // Default status = Backlog.
    assert_eq!(got1.status, IssueStatus::Backlog);
    assert_eq!(got2.status, IssueStatus::Backlog);

    // Full field round-trip on i1.
    assert_eq!(got1.project_id, project);
    assert_eq!(got1.stage, StageKind::Prototype);
    assert_eq!(got1.title, "原型调研");
    assert_eq!(got1.desc, "竞品分析 + 用户访谈");
    assert_eq!(got1.priority, IssuePriority::High);
    assert!(got1.assignee.is_none());
    assert!(got1.created_at > 0);
    assert_eq!(got1.created_at, got1.updated_at);

    // Transition i1: Backlog → InProgress → Done.
    store
        .transition_issue(i1, IssueStatus::InProgress)
        .await
        .unwrap();
    let mid = store.get_issue(i1).await.unwrap().unwrap();
    assert_eq!(mid.status, IssueStatus::InProgress);
    assert!(mid.updated_at >= mid.created_at);
    store.transition_issue(i1, IssueStatus::Done).await.unwrap();
    let done = store.get_issue(i1).await.unwrap().unwrap();
    assert_eq!(done.status, IssueStatus::Done);

    // Assign then unassign i2.
    let agent = AgentId::new();
    store.assign_issue(i2, Some(agent)).await.unwrap();
    let assigned = store.get_issue(i2).await.unwrap().unwrap();
    assert_eq!(assigned.assignee, Some(agent));
    store.assign_issue(i2, None).await.unwrap();
    let unassigned = store.get_issue(i2).await.unwrap().unwrap();
    assert!(unassigned.assignee.is_none());

    // list_issues (all) — ordered by number asc.
    let all = store.list_issues(project, None, None).await.unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].number, 1);
    assert_eq!(all[1].number, 2);

    // Filter by stage.
    let proto_only = store
        .list_issues(project, Some(StageKind::Prototype), None)
        .await
        .unwrap();
    assert_eq!(proto_only.len(), 1);
    assert_eq!(proto_only[0].id, i1);

    let build_only = store
        .list_issues(project, Some(StageKind::Build), None)
        .await
        .unwrap();
    assert_eq!(build_only.len(), 1);
    assert_eq!(build_only[0].id, i2);

    // Filter by status: Done (i1 is Done, i2 is Backlog).
    let done_only = store
        .list_issues(project, None, Some(IssueStatus::Done))
        .await
        .unwrap();
    assert_eq!(done_only.len(), 1);
    assert_eq!(done_only[0].id, i1);

    let backlog_only = store
        .list_issues(project, None, Some(IssueStatus::Backlog))
        .await
        .unwrap();
    assert_eq!(backlog_only.len(), 1);
    assert_eq!(backlog_only[0].id, i2);

    // Combined filter: stage + status.
    let combined = store
        .list_issues(project, Some(StageKind::Prototype), Some(IssueStatus::Done))
        .await
        .unwrap();
    assert_eq!(combined.len(), 1);
    assert_eq!(combined[0].id, i1);

    // Nonexistent issue → None.
    assert!(store.get_issue(IssueId::new()).await.unwrap().is_none());

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn issue_numbering_is_per_project() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let a = ProjectId::new();
    let b = ProjectId::new();

    for p in [a, b] {
        store
            .create_project(NewProject {
                id: p,
                name: "p".into(),
                kind: "y".into(),
                desc: String::new(),
            })
            .await
            .unwrap();
    }

    // Each project gets its own 1-based sequence.
    let a1 = IssueId::new();
    let a2 = IssueId::new();
    let b1 = IssueId::new();
    store
        .create_issue(NewIssue {
            id: a1,
            project_id: a,
            stage: StageKind::Build,
            title: "a1".into(),
            desc: String::new(),
            priority: IssuePriority::None,
        })
        .await
        .unwrap();
    store
        .create_issue(NewIssue {
            id: b1,
            project_id: b,
            stage: StageKind::Build,
            title: "b1".into(),
            desc: String::new(),
            priority: IssuePriority::None,
        })
        .await
        .unwrap();
    store
        .create_issue(NewIssue {
            id: a2,
            project_id: a,
            stage: StageKind::Build,
            title: "a2".into(),
            desc: String::new(),
            priority: IssuePriority::None,
        })
        .await
        .unwrap();

    assert_eq!(store.get_issue(a1).await.unwrap().unwrap().number, 1);
    assert_eq!(store.get_issue(a2).await.unwrap().unwrap().number, 2);
    assert_eq!(store.get_issue(b1).await.unwrap().unwrap().number, 1);

    let _ = std::fs::remove_file(&path);
}
