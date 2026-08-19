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

use super::vm_kb::build_kb;
use super::vm_panels::{
    build_board, build_config, build_metrics, build_sessions, build_weeks, build_workbench,
};

/// 读一份仓文件:读不出来就把原话记进 `warnings`,再退回默认值。
/// 「退回默认值」本身没问题,**不说话**才是问题。
pub(super) fn read_or_warn<T>(
    what: &str,
    r: Result<Option<T>, bw_v4::repo::RepoFileError>,
    warnings: &mut Vec<String>,
) -> Option<T> {
    match r {
        Ok(v) => v,
        Err(e) => {
            warnings.push(format!("{what} 读不出来:{e}"));
            None
        }
    }
}

/// 壳这边的纯导航状态(打开了哪个项目、在看哪一周)。不进库。
pub struct UiState {
    pub open: Option<ProjectId>,
    pub viewing_week: String,
    pub open_doc: Option<String>,
    pub note: Option<String>,
    /// 「开始本周」回来的草稿活标,连同是哪一周。人确认前只存在这里,不进库。
    pub pending_drafts: Option<(String, Vec<String>)>,
    /// 会话屏选中哪个会话、开着哪个页签、展开了哪些目录、中栏开着哪个文件。
    /// 纯导航状态,一律不进库。
    pub session_open: Option<bw_v4::model::IssueId>,
    pub session_tab: crate::vm::SessionTab,
    pub expanded_dirs: Vec<String>,
    pub open_file: String,
    /// 知识库屏在看哪个页签,以及代码图/资产两个页签**上一次点开时**跑出来的
    /// 结果。这两样各要起好几个子进程(codegraph、`git log --name-only`、仓
    /// 统计),每重拼一次 ViewModel 就重跑一遍会把界面拖垮 —— 所以只在人点
    /// 页签或点「重新跑一次」那一刻跑,结果放在这里。
    pub kb_tab: crate::vm::KbTab,
    pub kb_codegraph: Option<crate::vm::CodeGraphVm>,
    pub kb_assets: Option<crate::vm::AssetsVm>,
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

/// 一次命令回来一串事件,界面上那条回执只有一行 —— 取第一条,那是人刚做的
/// 那个动作。
///
/// **发群失败是个例外**:它永远排在末尾(合入、结清、发版这些账先记完,群通知
/// 是末尾一个失败也不影响主干的旁支),取第一条就把它盖掉了,人得到的回音会是
/// 完全的沉默。所以失败时把它附在主句后面 —— 这一条不显示的话,那句
/// 「发群失败」的文案就是死代码。
pub fn note_of(events: &[Event]) -> Option<String> {
    let mut note = primary_note(events)?;
    // 第一条就是它的时候(人主动点「同步到群」),主句已经说过了,别说两遍。
    if matches!(events.first(), Some(Event::ChatNotifySent { .. })) {
        return Some(note);
    }
    if let Some(Event::ChatNotifySent { note: why, .. }) = events
        .iter()
        .find(|e| matches!(e, Event::ChatNotifySent { ok: false, .. }))
    {
        note.push_str(&format!(";但发群没成:{why}(不会自动重发)"));
    }
    Some(note)
}

fn primary_note(events: &[Event]) -> Option<String> {
    events.first().map(|e| match e {
        Event::ProjectCreated { slug, adopted, .. } => {
            if *adopted {
                format!("接手了已有项目 {slug} —— 仓里原有的 .bw/project.toml 一个字没动")
            } else {
                format!("项目 {slug} 已接入")
            }
        }
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
        Event::HistoryBackfilled { note, .. } => note.clone(),
        Event::IssueMerged {
            pr_number, merged, ..
        } => {
            if *merged {
                format!("MR #{pr_number} 已合入,活已完成")
            } else {
                "这张活没有可合的 MR,只标了完成".into()
            }
        }
        Event::RunCancelled { was_live, .. } => {
            if *was_live {
                "已停止。这张活还停在「进行中」,再点▶跑就接回去。".into()
            } else {
                "本来就没有活着的终端可停。".into()
            }
        }
        Event::ChatNotifySent {
            event_type,
            ok,
            note,
            ..
        } => {
            let label = bw_v4::chat::event_label(event_type);
            if *ok {
                format!("已发到项目群:{label}")
            } else {
                // 发群失败不回滚已经发生的合入/完成/发版,只在这里说一次;
                // 没有下一次自动重试,如实说清楚。
                format!("发群失败(合入/完成本身已经生效,不会自动重发):{note}")
            }
        }
        Event::OpsAutoFired { workflow, week, .. } => {
            format!("定时到点:{week} 的「{workflow}」已自动建活并开工")
        }
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
    let mut warnings: Vec<String> = Vec::new();

    let mut cards = Vec::with_capacity(projects.len());
    for p in &projects {
        let ws = app.workspace_of(p.id).await.unwrap_or_default();
        let file = read_or_warn(
            &format!("{} 的 .bw/project.toml", p.slug),
            project_file::read(&ws),
            &mut warnings,
        )
        .unwrap_or_default();
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
        Some(id) => build_project(app, id, ui, &mut warnings).await,
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
        warnings,
    }
}

pub(super) fn remote_label(p: &Project) -> String {
    if p.remote_path.is_empty() {
        "—(没挂远端)".into()
    } else {
        format!("{} · {}", p.provider, p.remote_path)
    }
}

/// 本机环境条。探不到就是探不到;还没接实现的返回「不知道」显示灰,不是红。
pub(super) fn probe_env() -> Vec<ToolProbeVm> {
    // 探活与「要什么才能用」都问适配模块要,不在这里重写一遍 —— 加一个新的
    // 开工工具应该只是加一个适配模块目录,不改这个文件。
    let claude = crate::adapters::claude_cli::detect();
    vec![
        ToolProbeVm {
            name: "claude_cli".into(),
            label: "Claude CLI".into(),
            ok: Some(claude.is_some()),
            detail: claude.unwrap_or_else(|| {
                format!(
                    "本机路径里找不到 claude。要用它得先有:{}",
                    crate::adapters::claude_cli::REQUIRES.join("、")
                )
            }),
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

async fn build_project(
    app: &App,
    id: ProjectId,
    ui: &UiState,
    warnings: &mut Vec<String>,
) -> Option<ProjectVm> {
    let store = app.store();
    let p = store.project(id).await.ok()??;
    let ws = app.workspace_of(id).await.ok()?;
    let file =
        read_or_warn(".bw/project.toml", project_file::read(&ws), warnings).unwrap_or_default();
    let issues = store.issues(id).await.unwrap_or_default();
    let sessions = build_sessions(app, id, &issues).await;
    let current_week = bw_v4::isoweek::current_week();
    let viewing_week = if ui.viewing_week.is_empty() {
        current_week.clone()
    } else {
        ui.viewing_week.clone()
    };

    // 健康:三条判据当场从仓文件与 git 取,不读库里那两个缓存列。
    let inputs = bw_v4::app::collect_health_inputs(&ws, &current_week).await;
    let derived = bw_v4::derive::derive_project_health(&inputs);

    let policy = read_or_warn(
        ".bw/issue-policy.toml",
        issue_policy_file::read(&ws),
        warnings,
    );
    let plan = read_or_warn(
        &format!("docs/plan/{viewing_week}.md"),
        week_plan_file::read(&ws, &viewing_week),
        warnings,
    );

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
            current_version_raw: file.current_version.clone(),
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
        pending_drafts: match &ui.pending_drafts {
            Some((week, titles)) if *week == viewing_week => titles.clone(),
            _ => Vec::new(),
        },
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
        sessions: sessions.clone(),
        session_open: ui.session_open,
        workbench: build_workbench(
            &sessions,
            &issues,
            ui.session_open,
            ui.session_tab,
            &ui.expanded_dirs,
            &ui.open_file,
        )
        .await,
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
        kb: KbVm {
            codegraph: ui.kb_codegraph.clone(),
            assets: ui.kb_assets.clone(),
            ..build_kb(&ws, ui.kb_tab, ui.open_doc.as_deref())
        },
    })
}

pub(super) fn or_blank(s: &str) -> String {
    if s.trim().is_empty() {
        "(待填)".into()
    } else {
        s.to_string()
    }
}

pub(super) fn card_item(i: &bw_v4::Issue) -> CardItemVm {
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
