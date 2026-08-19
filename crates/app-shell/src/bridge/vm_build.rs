//! 把内核状态拼成一份可直接渲染的 ViewModel。
//!
//! 这里是**现算**发生的地方:健康三条判据、指标读数、周列表、workflow 用过
//! 几次,全部当场从库、仓文件、git 取。**没有的就留空**,绝不为了界面好看
//! 填一个 0 或者一个假名字。

use crate::vm::*;
use bw_v4::app::App;
use bw_v4::command::Event;
use bw_v4::model::{IssueStatus, Project, ProjectId};
use bw_v4::repo::{issue_policy_file, project_file, release_file, week_plan_file};
use bw_v4::V4Store;
use std::path::Path;

/// 壳这边的纯导航状态(打开了哪个项目、在看哪一周)。不进库。
pub struct UiState {
    pub open: Option<ProjectId>,
    pub viewing_week: String,
    pub open_doc: Option<String>,
    pub note: Option<String>,
    pub db_path: String,
    pub workspaces_root: String,
}

/// 深链按 slug 找,找不到再按显示名找。
pub async fn find_project(store: &V4Store, want: &str) -> Option<Project> {
    if let Ok(Some(p)) = store.project_by_slug(want).await {
        return Some(p);
    }
    store
        .projects()
        .await
        .ok()?
        .into_iter()
        .find(|p| p.name == want)
}

pub fn note_of(events: &[Event]) -> Option<String> {
    events.first().map(|e| match e {
        Event::ProjectCreated { slug, .. } => format!("项目 {slug} 已接入"),
        Event::ProjectCardEditPending { .. } => "名片改动已建成一张轻量活,等评审合入".into(),
        Event::ProjectChatChanged { .. } => "项目群配置已写进 .bw/project.toml".into(),
        Event::StandardBootstrapped {
            files, committed, ..
        } => format!(
            "规范铺底:落盘 {} 个文件,{}",
            files.len(),
            if *committed {
                "已提交"
            } else {
                "无改动可提交"
            }
        ),
        Event::StandardReconciled {
            missing,
            stale,
            human_edited,
            ..
        } => format!(
            "规范对账:缺 {} 份、过期 {} 份、人改过 {} 份",
            missing.len(),
            stale.len(),
            human_edited.len()
        ),
        Event::WeekPlanStarted { week, .. } => format!("{week} 的周计划文件已写出,等你确认草稿"),
        Event::WeekPlanAlreadyExists { week, .. } => format!("{week} 已经有周计划文件了,没有重写"),
        Event::IssueCreated { number, .. } => format!("建了一张活 #{number}"),
        Event::IssueScheduled { week_of, .. } => {
            if week_of.is_empty() {
                "这张活挪回待办池了".into()
            } else {
                format!("这张活排进了 {week_of}")
            }
        }
        Event::IssueReordered { .. } => "顺序改好了".into(),
        Event::IssueRan { ok, summary, .. } => {
            if *ok {
                format!("跑完了,推到「评审中」。执行器原话:{summary}")
            } else {
                format!("这次没跑成,活留在原地可以重试。原话:{summary}")
            }
        }
        Event::IssueTransitioned { to, settled, .. } => {
            if *settled {
                "这张活结清了(只结这一次)".into()
            } else {
                format!("状态改成「{}」", to.label())
            }
        }
        Event::IssueBlocked { .. } => "已标成阻塞,原因写进了活的说明".into(),
        Event::ReleaseCut {
            version,
            rows_written,
        } => {
            if *rows_written {
                format!("发版记录新增一行 {version}")
            } else {
                format!("{version} 已经在发版记录里了,没写第二行")
            }
        }
        Event::CurrentVersionChanged { version } => format!("在研版本切到 {version}"),
        Event::ToolMappingSaved { category } => format!("「{}」的映射保存了", category.label()),
        Event::ToolProbed { name, result } => format!("{name} 探活:{result:?}"),
        Event::IssueCacheRefreshed { week, updated } => {
            format!("按 {week} 的周计划文件刷新了 {updated} 张活的缓存")
        }
        Event::NotifySeenMarked { .. } => "通知已读到这里".into(),
        Event::HealthDerived { signal, .. } => format!("健康现算完成:{signal:?}"),
    })
}

pub async fn build(app: &App, ui: &UiState) -> Vm {
    let store = app.store();
    let projects = store.projects().await.unwrap_or_default();

    let mut cards = Vec::with_capacity(projects.len());
    for p in &projects {
        let ws = app.workspace_of(p.id).await.unwrap_or_default();
        let file = project_file::read(&ws).ok().flatten().unwrap_or_default();
        let week = bw_v4::isoweek::current_week();
        let in_week = store.issues_in_week(p.id, &week).await.unwrap_or_default();
        cards.push(ProjectCardVm {
            id: p.id,
            slug: p.slug.clone(),
            name: p.name.clone(),
            brief: file.brief.clone(),
            signal: p.signal,
            workspace_path: ws.display().to_string(),
            remote: remote_label(p),
            week_total: in_week.len() as u32,
            week_done: in_week
                .iter()
                .filter(|i| i.status == IssueStatus::Done)
                .count() as u32,
        });
    }

    let open = match ui.open {
        None => None,
        Some(id) => build_project(app, id, ui).await,
    };

    Vm {
        ready: true,
        fatal: None,
        projects: cards,
        env: probe_env(),
        open,
        settings: SettingsVm {
            workspaces_root: ui.workspaces_root.clone(),
            db_path: ui.db_path.clone(),
            claude_binary: bw_engine::resolve_claude_binary(None),
        },
        note: ui.note.clone(),
    }
}

fn remote_label(p: &Project) -> String {
    if p.remote_path.is_empty() {
        "—(没挂远端)".into()
    } else {
        format!("{} · {}", p.provider, p.remote_path)
    }
}

/// 本机环境条。探不到就是探不到;还没接实现的返回「不知道」显示灰,不是红。
fn probe_env() -> Vec<ToolProbeVm> {
    let claude = bw_engine::resolve_claude_binary(None);
    vec![
        ToolProbeVm {
            name: "claude_cli".into(),
            label: "Claude CLI".into(),
            ok: Some(claude.is_some()),
            detail: claude.unwrap_or_else(|| "本机路径里找不到 claude".into()),
        },
        ToolProbeVm {
            name: "cursor".into(),
            label: "Cursor".into(),
            ok: None,
            detail: "点「测一下」现探(探活要起子进程,不在开屏时做)".into(),
        },
        ToolProbeVm {
            name: "open_design".into(),
            label: "Open Design".into(),
            ok: None,
            detail: "点「测一下」现探".into(),
        },
        ToolProbeVm {
            name: "welink_cli".into(),
            label: "WeLink CLI".into(),
            ok: None,
            detail: "还没接,探不出结果".into(),
        },
    ]
}

async fn build_project(app: &App, id: ProjectId, ui: &UiState) -> Option<ProjectVm> {
    let store = app.store();
    let p = store.project(id).await.ok()??;
    let ws = app.workspace_of(id).await.ok()?;
    let file = project_file::read(&ws).ok().flatten().unwrap_or_default();
    let issues = store.issues(id).await.unwrap_or_default();
    let current_week = bw_v4::isoweek::current_week();
    let viewing_week = if ui.viewing_week.is_empty() {
        current_week.clone()
    } else {
        ui.viewing_week.clone()
    };

    // 健康:三条判据当场从仓文件与 git 取,不读库里那两个缓存列。
    let inputs = bw_v4::app::collect_health_inputs(&ws, &current_week).await;
    let derived = bw_v4::derive::derive_project_health(&inputs);

    let policy = issue_policy_file::read(&ws).ok().flatten();
    let plan = week_plan_file::read(&ws, &viewing_week).ok().flatten();

    Some(ProjectVm {
        id,
        slug: p.slug.clone(),
        name: p.name.clone(),
        card: CardVm {
            brief: or_blank(&file.brief),
            benchmark: or_blank(&file.benchmark),
            north_star: or_blank(&file.opportunity),
            remote: remote_label(&p),
            current_version: or_blank(&file.current_version),
            standard_version: or_blank(&file.standard_version),
            chat: match &file.chat {
                None => "未配".into(),
                Some(c) => format!("{} · 群号 {}", c.provider, c.group_id),
            },
        },
        health: HealthVm {
            signal: Some(derived.signal()),
            reasons: derived
                .reasons()
                .iter()
                .map(|r| (r.ok, r.text.clone()))
                .collect(),
        },
        metrics: build_metrics(&ws, plan.as_ref(), &issues),
        weeks: build_weeks(&ws),
        current_week,
        viewing_week: viewing_week.clone(),
        board: build_board(&issues, &viewing_week, policy.as_ref()),
        releases: release_file::read(&ws)
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .map(|r| ReleaseVm {
                version: r.version,
                released_at: r.released_at,
                note: r.note,
                included: if r.included_numbers.is_empty() {
                    "—".into()
                } else {
                    r.included_numbers
                        .iter()
                        .map(|n| format!("#{n}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                },
                origin: r.origin,
            })
            .collect(),
        sessions: build_sessions(store, id, &issues).await,
        notify: NotifyVm {
            in_review: issues
                .iter()
                .filter(|i| i.status == IssueStatus::InReview)
                .map(card_item)
                .collect(),
            blocked: issues
                .iter()
                .filter(|i| i.status == IssueStatus::Blocked)
                .map(card_item)
                .collect(),
            seen_at: store
                .meta(&bw_v4::store::notify_seen_key(id))
                .await
                .ok()
                .flatten()
                .and_then(|v| v.parse().ok()),
        },
        config: build_config(store, id, &ws, policy.as_ref(), &p).await,
        kb: build_kb(&ws, ui.open_doc.as_deref()),
    })
}

fn or_blank(s: &str) -> String {
    if s.trim().is_empty() {
        "(待填)".into()
    } else {
        s.to_string()
    }
}

fn card_item(i: &bw_v4::Issue) -> CardItemVm {
    CardItemVm {
        id: i.id,
        number: i.number,
        title: i.title.clone(),
        category: i.category.map(|c| c.label()).unwrap_or("—").into(),
        tool: week_plan_file::tool_label(&i.tool).into(),
        workflow: if i.workflow.is_empty() {
            "—".into()
        } else {
            i.workflow.clone()
        },
        kind: i.kind.label().into(),
        origin: i.origin.label().into(),
        week_of: i.week_of.clone(),
        version: i.version.clone(),
        metric_key: i.metric_key.clone(),
        settled: i.settled_at.is_some(),
        status: i.status,
    }
}

/// 六列看板。待办池那一列装的是没排进任何一周的活,其余五列按状态分。
fn build_board(
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
fn build_weeks(ws: &Path) -> Vec<WeekVm> {
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
fn build_metrics(
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
    let reading_of = |name: &str| -> Option<&week_plan_file::MetricReading> {
        plan?.readings.iter().find(|r| r.name.contains(name))
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

async fn build_sessions(store: &V4Store, id: ProjectId, issues: &[bw_v4::Issue]) -> Vec<SessionVm> {
    store
        .conversations(id)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|c| {
            let issue = issues.iter().find(|i| i.id == c.issue_id)?;
            Some(SessionVm {
                issue_id: c.issue_id,
                issue_number: issue.number,
                issue_title: issue.title.clone(),
                branch: c.branch_name,
                workspace_path: c.workspace_path,
                session_id: c.claude_session_id,
            })
        })
        .collect()
}

async fn build_config(
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

fn build_kb(ws: &Path, open: Option<&str>) -> KbVm {
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
