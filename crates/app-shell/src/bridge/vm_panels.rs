//! 计划屏、指标、会话、配置、知识库这几块的 ViewModel 拼装。
//!
//! 从 `vm_build.rs` 拆出来的下半截 —— 拆的理由只有一个:单文件超过 600 行的
//! 软目标了(`scripts/guard-file-lines.sh` 会提醒)。上半截管「一个项目整体
//! 长什么样」,这半截管「各块面板长什么样」。

use crate::vm::*;
use bw_v4::model::{IssueStatus, Project, ProjectId};
use bw_v4::repo::{issue_policy_file, project_file, week_plan_file};
use bw_v4::V4Store;
use std::path::Path;

use super::vm_build::{card_item, probe_env, remote_label};
use super::vm_kb::{managed_paths, skill_origin};

/// 六列看板。待办池那一列装的是没排进任何一周的活,其余五列按状态分。
/// `week` 传 `None` = 「全部活」视图,不按周过滤(左栏点「全部」时)。
pub(super) fn build_board(
    issues: &[bw_v4::Issue],
    week: Option<&str>,
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

    let in_week = |i: &&bw_v4::Issue| match week {
        None => true,
        Some(w) => i.week_of == w,
    };
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
    // `target` / `def` / 采集方式来自指标定义文件;读数来自周计划文件。两边
    // 各是各的正本,这里只是拼到同一张卡上。
    let mk = |name: &str, def: &str, target: &str, manual: bool| MetricCardVm {
        id: name.to_string(),
        name: name.to_string(),
        reading: reading_of(name).map(|r| r.value.clone()),
        target: target.to_string(),
        def: def.to_string(),
        manual,
        source: reading_of(name)
            .map(|r| r.source.clone())
            .unwrap_or_default(),
        collected_at: reading_of(name)
            .map(|r| r.collected_at.clone())
            .unwrap_or_default(),
        driving: driving_of(name),
    };
    let is_manual =
        |k: bw_engine::metrics_file::CollectKind| k == bw_engine::metrics_file::CollectKind::Manual;
    MetricsVm {
        // 北极星没有 target 字段 —— 它的目标就是它自己那句定义。
        north_star: Some(mk(
            &m.north_star.name,
            &m.north_star.def,
            "",
            is_manual(m.north_star.collect.kind),
        )),
        lagging: m
            .lagging
            .iter()
            .map(|d| mk(&d.name, &d.def, &d.target, is_manual(d.collect.kind)))
            .collect(),
        leading: m
            .leading
            .iter()
            .map(|d| mk(&d.name, &d.def, &d.target, is_manual(d.collect.kind)))
            .collect(),
        note: None,
    }
}

/// 本周四段计数。待办池不算进去 —— 它按定义就是「没排进任何一周」。
pub(super) fn build_week_counts(board: &BoardVm) -> WeekCountsVm {
    let n = |st: IssueStatus| {
        board
            .columns
            .iter()
            .find(|c| c.status == st)
            .map(|c| c.cards.len())
            .unwrap_or(0)
    };
    WeekCountsVm {
        todo: n(IssueStatus::Todo),
        doing: n(IssueStatus::InProgress),
        review: n(IssueStatus::InReview),
        done: n(IssueStatus::Done),
    }
}

/// 「本周运作」那张表。周计划文件里没有这一段就是空的 —— 不替它编三行。
pub(super) fn build_ops(plan: Option<&week_plan_file::WeekPlan>) -> Vec<OpsChipVm> {
    plan.map(|p| {
        p.ops
            .iter()
            .map(|r| OpsChipVm {
                title: r.title.clone(),
                status: r.status.clone(),
                note: r.note.clone(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// 名片改动那张在途的轻量活。名片是仓文件,改它走分支 + MR,所以总览要能
/// 看见「改了、还没合」。只认最新的一张。
pub(super) fn build_card_mr(issues: &[bw_v4::Issue]) -> Option<CardMrVm> {
    let i = issues
        .iter()
        .filter(|i| i.title.starts_with("编辑项目名片"))
        .filter(|i| {
            matches!(
                i.status,
                IssueStatus::InReview | IssueStatus::InProgress | IssueStatus::Todo
            )
        })
        .max_by_key(|i| i.number)?;
    Some(CardMrVm {
        issue_id: Some(i.id),
        number: i.number,
        status: i.status.label().to_string(),
        pr_number: i.pr_number,
        mergeable: i.status == IssueStatus::InReview,
    })
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
    let managed = managed_paths(ws);
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
                        origin: skill_origin(&managed, &slug),
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
                origin: "不在本仓".into(),
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
        chat: chat_label(ws),
    }
}

/// 项目群一行话。没配就明说没配 —— 没配群不是错,是诚实状态。
fn chat_label(ws: &Path) -> String {
    let Some(cfg) = project_file::read(ws)
        .ok()
        .flatten()
        .and_then(|f| f.chat)
        .filter(|c| !c.provider.trim().is_empty() && c.provider.trim() != "none")
    else {
        return "—(没配项目群。配了就在 .bw/project.toml 里加一段 [chat])".into();
    };
    // 整行没写 = 默认那三样;写了空数组 = 静音,如实说「一件都不发」。
    let notify = match &cfg.notify {
        None => bw_v4::chat::DEFAULT_NOTIFY
            .iter()
            .map(|e| bw_v4::chat::event_label(e))
            .collect::<Vec<_>>()
            .join(" / "),
        Some(list) if list.is_empty() => "一件都不发(静音)".to_string(),
        Some(list) => list
            .iter()
            .map(|e| bw_v4::chat::event_label(e))
            .collect::<Vec<_>>()
            .join(" / "),
    };
    format!(
        "{} · 群 {} · 同步 {notify}",
        cfg.provider,
        blank_dash(&cfg.group_id)
    )
}

fn blank_dash(s: &str) -> &str {
    if s.trim().is_empty() {
        "—"
    } else {
        s
    }
}

/// 代码仓级指标。**每一项都注明从哪采的**;采不到就整块给出原话,不填 0。
///
/// 高保真那张网格上还有几项是远端来的(合入的 PR、远端 issue、开放 PR),
/// 那要走 GitHub / codehub 的接口,今天还没接 —— 与其编几个数,不如只列采得
/// 到的,并在界面上说清楚少了哪几项(见 `docs/LEFTOVERS.md`)。
pub(super) async fn collect_repo_stats(ws: &Path) -> RepoStatsVm {
    let e = match bw_engine::evidence::collect(&ws.display().to_string()).await {
        Ok(e) => e,
        Err(err) => {
            return RepoStatsVm {
                items: Vec::new(),
                error: format!("读不到仓统计:{err}"),
            }
        }
    };
    let mut items = vec![
        (e.commit_count.to_string(), "累计提交".into(), "git".into()),
        (
            e.tracked_files.to_string(),
            "跟踪的文件".into(),
            "git".into(),
        ),
        (
            e.dirty_paths.to_string(),
            "没提交的改动".into(),
            "git".into(),
        ),
        (
            e.docs_files.to_string(),
            "docs/ 下的文件".into(),
            "git".into(),
        ),
    ];
    if let Some(d) = bw_v4::git::first_commit_date(ws).await {
        items.push((d, "首次提交".into(), "git".into()));
    }
    if let Ok(tags) = bw_v4::git::tags(ws).await {
        items.push((tags.len().to_string(), "打过的标签".into(), "git".into()));
    }
    RepoStatsVm {
        items,
        error: String::new(),
    }
}
