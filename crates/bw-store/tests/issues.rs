//! R1 Issue layer — store-level checks: per-project numbering, default status,
//! transitions, assignment, filtered listing, and full field round-trip.

use bw_core::model::{ArtifactKind, IssuePriority, IssueStatus, StageKind};
use bw_core::{AgentId, ArtifactId, IssueId, ProjectId};
use bw_store::{NewArtifact, NewIssue, NewProject, SqliteStore, Store};

fn tmp_db() -> String {
    let p = std::env::temp_dir().join(format!("bw_store_issue_test_{}.db", uuid::Uuid::new_v4()));
    p.to_string_lossy().into_owned()
}

#[tokio::test]
async fn issue_linkage_columns_round_trip_and_null_for_old_rows() {
    // A2: workflow_run.issue_id + artifact.issue_id link work to the Issue it
    // belongs to. An issue-bound artifact (the Done-edge case) carries its
    // issue_id back on read; a pre-A2 / non-issue row stays NULL; the per-issue
    // query returns only the bound versions.
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let project = ProjectId::new();
    store
        .create_project(NewProject {
            id: project,
            name: "A2 关联列测试".into(),
            kind: "y".into(),
            desc: String::new(),
        })
        .await
        .unwrap();
    let issue = IssueId::new();
    store
        .create_issue(NewIssue {
            id: issue,
            project_id: project,
            stage: StageKind::Build,
            title: "可验收的活".into(),
            desc: String::new(),
            priority: IssuePriority::Medium,
        })
        .await
        .unwrap();

    let bound = NewArtifact {
        id: ArtifactId::new(),
        project_id: project,
        workflow_run_id: None,
        issue_id: Some(issue),
        stage_kind: Some(StageKind::Build),
        path: "docs/done-edge.md".into(),
        kind: ArtifactKind::Doc,
        bytes: 128,
        git_commit: "abc".into(),
        registered_at: 1_700_000_000,
    };
    let legacy = NewArtifact {
        id: ArtifactId::new(),
        project_id: project,
        workflow_run_id: None,
        issue_id: None,
        stage_kind: None,
        path: "README.md".into(),
        kind: ArtifactKind::Doc,
        bytes: 256,
        git_commit: "abc".into(),
        registered_at: 1_700_000_001,
    };
    assert_eq!(
        store.register_artifacts(vec![bound, legacy]).await.unwrap(),
        2
    );

    // Round-trip: the bound version carries its issue_id; the legacy one is NULL.
    let arts = store.list_artifacts(project).await.unwrap();
    let bound_back = arts
        .iter()
        .find(|a| a.path == "docs/done-edge.md")
        .expect("bound artifact registered");
    assert_eq!(bound_back.issue_id, Some(issue));
    let legacy_back = arts
        .iter()
        .find(|a| a.path == "README.md")
        .expect("legacy artifact registered");
    assert_eq!(legacy_back.issue_id, None);

    // Per-issue query returns ONLY the bound version.
    let for_issue = store.list_artifacts_for_issue(issue).await.unwrap();
    assert_eq!(for_issue.len(), 1);
    assert_eq!(for_issue[0].id, bound_back.id);

    // No runs are issue-bound yet (RunIssue lands in A3) — honest empty, and no
    // panic reading the new column.
    assert!(store.list_runs_for_issue(issue).await.unwrap().is_empty());

    let _ = std::fs::remove_file(&path);
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
