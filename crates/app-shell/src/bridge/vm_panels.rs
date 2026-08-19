//! 计划屏、指标、会话、配置、知识库这几块的 ViewModel 拼装。
//!
//! 从 `vm_build.rs` 拆出来的下半截 —— 拆的理由只有一个:单文件超过 600 行的
//! 软目标了(`scripts/guard-file-lines.sh` 会提醒)。上半截管「一个项目整体
//! 长什么样」,这半截管「各块面板长什么样」。

use crate::vm::*;
use bw_v4::model::{IssueStatus, Project, ProjectId};
use bw_v4::repo::{issue_policy_file, week_plan_file};
use bw_v4::V4Store;
use std::path::Path;

use super::vm_build::{card_item, probe_env, remote_label};

/// 六列看板。待办池那一列装的是没排进任何一周的活,其余五列按状态分。
pub(super) fn build_board(
    issues: &[bw_v4::Issue],
    week: &str,
    policy: Option<&issue_policy_file::IssuePolicyFile>,
) -> BoardVm {
    let labels = policy.and_then(|p| p.kanban.clone());
    let pool_label = labels
        .as_ref()
        .map(|k| k.pool_label.clone())
        .unwrap_or_else(|| "待办池 · 未排进任何一周".into());
    let todo_label = labels
        .as_ref()
        .map(|k| k.todo_label.clone())
        .unwrap_or_else(|| "待办 · 已排进本周,等开工".into());

    let in_week = |i: &&bw_v4::Issue| i.week_of == week;
    let columns = vec![
        ColumnVm {
            status: IssueStatus::Backlog,
            title: pool_label.clone(),
            // 待办池按定义就是「没排进任何一周」,不按正在看的那一周过滤。
            cards: issues
                .iter()
                .filter(|i| i.week_of.is_empty())
                .map(card_item)
                .collect(),
        },
        ColumnVm {
            status: IssueStatus::Todo,
            title: todo_label.clone(),
            cards: issues
                .iter()
                .filter(|i| in_week(i) && i.status == IssueStatus::Todo)
                .map(card_item)
                .collect(),
        },
        ColumnVm {
            status: IssueStatus::InProgress,
            title: "进行中".into(),
            cards: issues
                .iter()
                .filter(|i| in_week(i) && i.status == IssueStatus::InProgress)
                .map(card_item)
                .collect(),
        },
        ColumnVm {
            status: IssueStatus::InReview,
            title: "评审中".into(),
            cards: issues
                .iter()
                .filter(|i| in_week(i) && i.status == IssueStatus::InReview)
                .map(card_item)
                .collect(),
        },
        ColumnVm {
            status: IssueStatus::Done,
            title: "已完成".into(),
            cards: issues
                .iter()
                .filter(|i| in_week(i) && i.status == IssueStatus::Done)
                .map(card_item)
                .collect(),
        },
        ColumnVm {
            status: IssueStatus::Blocked,
            title: "阻塞".into(),
            cards: issues
                .iter()
                .filter(|i| in_week(i) && i.status == IssueStatus::Blocked)
                .map(card_item)
                .collect(),
        },
    ];
    BoardVm {
        columns,
        pool_label,
        todo_label,
    }
}

/// 周列表靠扫 `docs/plan/` 得到 —— 没有索引表。
pub(super) fn build_weeks(ws: &Path) -> Vec<WeekVm> {
    week_plan_file::list_weeks(ws)
        .into_iter()
        .map(|w| {
            let plan = week_plan_file::read(ws, &w).ok().flatten();
            WeekVm {
                backfill: plan
                    .as_ref()
                    .and_then(|p| p.front_matter.as_ref())
                    .is_some_and(|f| f.is_backfill()),
                goal: plan.as_ref().and_then(|p| p.goal.clone()),
                activity_count: plan
                    .as_ref()
                    .map(|p| p.activities.len() as u32)
                    .unwrap_or(0),
                week: w,
            }
        })
        .collect()
}

/// 指标卡:定义来自 `.bw/metrics.toml`,读数来自周计划文件的「本周指标读数」段。
/// **没有读数就显示「无数据」,不显示 0**。
pub(super) fn build_metrics(
    ws: &Path,
    plan: Option<&week_plan_file::WeekPlan>,
    issues: &[bw_v4::Issue],
) -> MetricsVm {
    let Ok(Some(m)) = bw_engine::metrics_file::read(&ws.display().to_string()) else {
        return MetricsVm {
            note: Some("这个项目还没有 .bw/metrics.toml,指标是空的(不是 0)".into()),
            ..Default::default()
        };
    };
    // 精确按名字对,不用 contains:指标名互为前缀时会认错,空名字会命中第一条。
    let reading_of = |name: &str| -> Option<&week_plan_file::MetricReading> {
        if name.trim().is_empty() {
            return None;
        }
        plan?.readings.iter().find(|r| r.name.trim() == name.trim())
    };
    let driving_of = |key: &str| -> Vec<String> {
        issues
            .iter()
            .filter(|i| i.metric_key == key)
            .map(|i| i.title.clone())
            .collect()
    };
    // `.bw/metrics.toml` 里的指标没有单独的 id 字段 —— 名字就是它的键。
    // `issue.metric_key` 存的因此是指标的名字。
    let mk = |name: &str| MetricCardVm {
        id: name.to_string(),
        name: name.to_string(),
        reading: reading_of(name).map(|r| r.value.clone()),
        source: reading_of(name)
            .map(|r| r.source.clone())
            .unwrap_or_default(),
        collected_at: reading_of(name)
            .map(|r| r.collected_at.clone())
            .unwrap_or_default(),
        driving: driving_of(name),
    };
    MetricsVm {
        north_star: Some(mk(&m.north_star.name)),
        lagging: m.lagging.iter().map(|d| mk(&d.name)).collect(),
        leading: m.leading.iter().map(|d| mk(&d.name)).collect(),
        note: None,
    }
}

pub(super) async fn build_sessions(
    app: &bw_v4::app::App,
    id: ProjectId,
    issues: &[bw_v4::Issue],
) -> Vec<SessionVm> {
    app.store()
        .conversations(id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let issue = issues.iter().find(|i| i.id == c.issue_id)?;
            Some(SessionVm {
                issue_id: c.issue_id,
                conversation_id: c.id,
                issue_number: issue.number,
                issue_title: issue.title.clone(),
                issue_status: issue.status.label().to_string(),
                branch: c.branch_name,
                workspace_path: c.workspace_path,
                session_id: c.claude_session_id,
                // 唯一真实的信号:这个会话的 PTY 进程还在不在。
                live: app.pty_live(c.id),
            })
        })
        .collect()
}

/// 会话屏右栏 + 中栏。全部现算:文件树点开哪层读哪层、改动文件问 git、
/// diff 也问 git。**没选中会话就整块是空的**,不摆一个假的工作台。
pub(super) async fn build_workbench(
    sessions: &[SessionVm],
    issues: &[bw_v4::Issue],
    open: Option<bw_v4::model::IssueId>,
    tab: SessionTab,
    expanded: &[String],
    open_path: &str,
) -> WorkbenchVm {
    let Some(s) = open.and_then(|id| sessions.iter().find(|s| s.issue_id == id)) else {
        return WorkbenchVm::default();
    };
    let ws = std::path::PathBuf::from(&s.workspace_path);
    let pr_number = issues
        .iter()
        .find(|i| i.id == s.issue_id)
        .map(|i| i.pr_number)
        .unwrap_or(0);

    let mut expanded: Vec<String> = expanded.to_vec();
    if !expanded.iter().any(|d| d.is_empty()) {
        expanded.push(String::new());
    }
    let tree: Vec<(String, Vec<TreeEntryVm>)> = expanded
        .iter()
        .map(|dir| {
            let entries = bw_v4::git::list_dir(&ws, dir)
                .into_iter()
                .map(|e| TreeEntryVm {
                    rel: e.rel,
                    name: e.name,
                    is_dir: e.is_dir,
                })
                .collect();
            (dir.clone(), entries)
        })
        .collect();

    let changed: Vec<ChangedFileVm> = bw_v4::git::changed_files(&ws)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|c| ChangedFileVm {
            label: c.label(),
            path: c.path,
        })
        .collect();

    // 中栏正文:文件页签给全文(只读),diff 页签问 git 要 diff。读不出来就
    // 把读不出来这件事本身摆出来,不显示空白。
    let open_body = if open_path.is_empty() {
        String::new()
    } else if tab == SessionTab::Diff {
        bw_v4::git::file_diff(&ws, open_path)
            .await
            .unwrap_or_else(|e| format!("(diff 取不到:{e})"))
    } else {
        std::fs::read_to_string(ws.join(open_path))
            .unwrap_or_else(|e| format!("(这个文件读不出来:{e})"))
    };

    WorkbenchVm {
        workspace: s.workspace_path.clone(),
        branch: s.branch.clone(),
        ahead_behind: bw_v4::git::ahead_behind(&ws, "main").await,
        dirty: !changed.is_empty(),
        expanded,
        tree,
        changed,
        pr_number,
        tab,
        open_path: open_path.to_string(),
        open_body,
    }
}

pub(super) async fn build_config(
    store: &V4Store,
    id: ProjectId,
    ws: &Path,
    policy: Option<&issue_policy_file::IssuePolicyFile>,
    project: &Project,
) -> ConfigVm {
    let usage = store.workflow_usage(id).await.unwrap_or_default();
    let mappings = policy
        .map(|p| {
            p.mappings
                .iter()
                .map(|m| MappingVm {
                    category_key: m.category.clone(),
                    category_label: m
                        .category()
                        .map(|c| c.label().to_string())
                        .unwrap_or_else(|| m.category.clone()),
                    tool: week_plan_file::tool_label(&m.tool).into(),
                    workflow: if m.workflow.is_empty() {
                        "—(无默认,从鱼塘挑)".into()
                    } else {
                        m.workflow.clone()
                    },
                })
                .collect()
        })
        .unwrap_or_default();

    // 技能清单扫目录得到 —— 没有登记表可查,目录就是唯一判据。
    let mut skills: Vec<SkillVm> = std::fs::read_dir(ws.join(".claude/skills"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().join("SKILL.md").is_file())
                .map(|e| {
                    let slug = e.file_name().to_string_lossy().to_string();
                    SkillVm {
                        uses: usage
                            .iter()
                            .find(|(w, _)| *w == slug)
                            .map(|(_, n)| *n)
                            .unwrap_or(0),
                        title: slug.clone(),
                        slug,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    // 项目里没有目录、但活上挂了名字的 workflow(比如整包的 mattpocock-skills)
    // 也如实列出来,用量是真的,只是包不在这个仓里。
    for (w, n) in &usage {
        if !skills.iter().any(|s| &s.slug == w) {
            skills.push(SkillVm {
                slug: w.clone(),
                title: format!("{w}(包不在本仓 .claude/skills/)"),
                uses: *n,
            });
        }
    }
    skills.sort_by(|a, b| b.uses.cmp(&a.uses).then(a.slug.cmp(&b.slug)));

    ConfigVm {
        mappings,
        skills,
        tools: probe_env(),
        remote: remote_label(project),
        cadence: policy
            .and_then(|p| p.cadence.clone())
            .map(|c| {
                format!(
                    "运作活①:{};运作活②:{} {}",
                    blank_dash(&c.ops1_trigger),
                    blank_dash(&c.ops2_trigger),
                    blank_dash(&c.ops2_schedule)
                )
            })
            .unwrap_or_else(|| "—(.bw/issue-policy.toml 里没有节律段)".into()),
    }
}

fn blank_dash(s: &str) -> &str {
    if s.trim().is_empty() {
        "—"
    } else {
        s
    }
}

pub(super) fn build_kb(ws: &Path, open: Option<&str>) -> KbVm {
    let mut docs = Vec::new();
    collect_docs(&ws.join("docs"), ws, &mut docs, 0);
    docs.sort();
    KbVm {
        open_doc: open.and_then(|rel| {
            std::fs::read_to_string(ws.join(rel))
                .ok()
                .map(|body| (rel.to_string(), body))
        }),
        docs,
    }
}

fn collect_docs(dir: &Path, root: &Path, out: &mut Vec<String>, depth: usize) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            collect_docs(&path, root, out, depth + 1);
        } else if path.extension().is_some_and(|x| x == "md") {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.display().to_string());
            }
        }
    }
}
