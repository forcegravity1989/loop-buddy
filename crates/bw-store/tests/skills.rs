//! R2 Skill compounding — store-level checks: distilling a skill from a real
//! completed Issue sets provenance (distilled_from_issue + origin_agent),
//! non-Done / unassigned issues are rejected, catalog skills stay provenance-
//! free, and distilling is additive (same issue twice → two skills).

use bw_core::model::{IssuePriority, IssueStatus, LibSource, Maturity, StageKind};
use bw_core::{AgentId, IssueId, ProjectId, SkillId};
use bw_store::{NewIssue, NewProject, NewSkill, SqliteStore, Store};

fn tmp_db() -> String {
    let p = std::env::temp_dir().join(format!("bw_store_skill_test_{}.db", uuid::Uuid::new_v4()));
    p.to_string_lossy().into_owned()
}

/// Set up a project + a single Issue, then leave the Issue in a caller-chosen
/// state (assignee + status). Returns `(store, project_id, issue_id,
/// optional_assignee)`.
async fn setup_with_issue(
    assignee: Option<AgentId>,
    status: IssueStatus,
) -> (SqliteStore, ProjectId, IssueId, Option<AgentId>) {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();
    let project = ProjectId::new();
    store
        .create_project(NewProject {
            id: project,
            name: "R2 技能测试项目".into(),
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
            title: "实现某个功能".into(),
            desc: "一个真实 Issue".into(),
            priority: IssuePriority::Medium,
        })
        .await
        .unwrap();

    if let Some(a) = assignee {
        store.assign_issue(issue, Some(a)).await.unwrap();
    }
    store.transition_issue(issue, status).await.unwrap();

    (store, project, issue, assignee)
}

#[tokio::test]
async fn distill_from_done_assigned_issue_sets_provenance() {
    let agent = AgentId::new();
    let (store, _project, issue, _) = setup_with_issue(Some(agent), IssueStatus::Done).await;

    let skill = SkillId::new();
    store
        .distill_skill_from_issue(
            NewSkill {
                id: skill,
                name: "解决方案提炼".into(),
                maturity: Maturity::Polishing, // ignored — distill always sets Polishing
                desc: "从真实 Issue 沉淀的技能".into(),
                category: "构建".into(),
                source: LibSource::Official, // ignored — distill always sets SelfBuilt
                content: "蒸馏正文:五角色环真实交付法(测试用)".into(),
            },
            issue,
        )
        .await
        .unwrap();

    // The distilled skill carries provenance.
    let got = store.get_skill(skill).await.unwrap().unwrap();
    assert_eq!(got.distilled_from_issue, Some(issue));
    assert_eq!(got.origin_agent, Some(agent));
    // Distill always sets SelfBuilt / Polishing / uses=0 regardless of input.
    assert_eq!(got.source, LibSource::SelfBuilt);
    assert_eq!(got.maturity, Maturity::Polishing);
    assert_eq!(got.uses, 0);

    // It appears in list_skills.
    let listed = store.list_skills().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, skill);
    assert_eq!(listed[0].distilled_from_issue, Some(issue));
}

#[tokio::test]
async fn distill_from_non_done_issue_errors() {
    let agent = AgentId::new();
    let (store, _project, issue, _) = setup_with_issue(Some(agent), IssueStatus::InProgress).await;

    let result = store
        .distill_skill_from_issue(
            NewSkill {
                id: SkillId::new(),
                name: "不该成功".into(),
                maturity: Maturity::Polishing,
                desc: String::new(),
                category: String::new(),
                source: LibSource::SelfBuilt,
                content: "蒸馏正文:五角色环真实交付法(测试用)".into(),
            },
            issue,
        )
        .await;

    assert!(
        result.is_err(),
        "distilling from a non-Done issue must fail"
    );

    // No skill was created.
    assert!(store.list_skills().await.unwrap().is_empty());
}

#[tokio::test]
async fn distill_from_done_unassigned_issue_errors() {
    // Done but never assigned — no agent to attribute.
    let (store, _project, issue, _) = setup_with_issue(None, IssueStatus::Done).await;

    let result = store
        .distill_skill_from_issue(
            NewSkill {
                id: SkillId::new(),
                name: "不该成功".into(),
                maturity: Maturity::Polishing,
                desc: String::new(),
                category: String::new(),
                source: LibSource::SelfBuilt,
                content: "蒸馏正文:五角色环真实交付法(测试用)".into(),
            },
            issue,
        )
        .await;

    assert!(
        result.is_err(),
        "distilling from a Done but unassigned issue must fail"
    );
    assert!(store.list_skills().await.unwrap().is_empty());
}

#[tokio::test]
async fn catalog_skill_has_no_provenance() {
    let path = tmp_db();
    let store = SqliteStore::open(&path).await.unwrap();

    let skill = SkillId::new();
    store
        .create_skill(NewSkill {
            id: skill,
            name: "手动创建的技能".into(),
            maturity: Maturity::Polishing,
            desc: String::new(),
            category: "检索".into(),
            source: LibSource::SelfBuilt,
            content: "蒸馏正文:五角色环真实交付法(测试用)".into(),
        })
        .await
        .unwrap();

    // A manually-created (catalog/seeded-style) skill has no provenance —
    // backward-compatible with pre-R2 behavior.
    let got = store.get_skill(skill).await.unwrap().unwrap();
    assert!(got.distilled_from_issue.is_none());
    assert!(got.origin_agent.is_none());

    let _ = std::fs::remove_file(&path);
}

#[tokio::test]
async fn distill_is_additive_same_issue_twice_makes_two_skills() {
    let agent = AgentId::new();
    let (store, _project, issue, _) = setup_with_issue(Some(agent), IssueStatus::Done).await;

    let s1 = SkillId::new();
    let s2 = SkillId::new();
    store
        .distill_skill_from_issue(
            NewSkill {
                id: s1,
                name: "第一次提炼".into(),
                maturity: Maturity::Polishing,
                desc: String::new(),
                category: String::new(),
                source: LibSource::SelfBuilt,
                content: "蒸馏正文:五角色环真实交付法(测试用)".into(),
            },
            issue,
        )
        .await
        .unwrap();
    // A second distill of the same issue is NOT an error — each call mints a
    // new skill row.
    store
        .distill_skill_from_issue(
            NewSkill {
                id: s2,
                name: "第二次提炼".into(),
                maturity: Maturity::Polishing,
                desc: String::new(),
                category: String::new(),
                source: LibSource::SelfBuilt,
                content: "蒸馏正文:五角色环真实交付法(测试用)".into(),
            },
            issue,
        )
        .await
        .unwrap();

    let listed = store.list_skills().await.unwrap();
    assert_eq!(listed.len(), 2, "two distills = two skill rows");
    // Both carry the same provenance.
    for s in &listed {
        assert_eq!(s.distilled_from_issue, Some(issue));
        assert_eq!(s.origin_agent, Some(agent));
    }
}
