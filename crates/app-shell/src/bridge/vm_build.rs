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

use super::vm_derive::{build_card_mr, build_notify_events, build_ops, build_week_counts};
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
    /// 计划屏左栏点的是「全部」。
    pub view_all: bool,
    pub open_doc: Option<String>,
    pub note: Option<String>,
    /// 这是第几条回执。toast 靠它判断「这条我关过没有」—— 按正文判断的话,
    /// 同一句失败第二次出现会被当成已关过的那条,静默不弹。
    pub note_seq: u64,
    /// 「开始本周」回来的草稿活标,连同是哪一周。人确认前只存在这里,不进库。
    pub pending_drafts: Option<(String, Vec<String>)>,
    /// 会话屏选中哪个会话、开着哪个页签、展开了哪些目录、中栏开着哪个文件。
    /// 纯导航状态,一律不进库。
    pub session_open: Option<bw_v4::model::IssueId>,
    /// 计划屏详情抽屉开着哪张活。纯导航状态,不进库。
    pub selected_issue: Option<bw_v4::model::IssueId>,
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
    /// 总览那块「项目指标 · 代码仓级」上一次采到的数。同理:采一次要起好几个
    /// git 子进程,只在人点「立即采集」那一刻跑。
    pub repo_stats: Option<crate::vm::RepoStatsVm>,
    pub db_path: String,
    pub workspaces_root: String,
    /// 接入屏那份仓列表的状态。**不进库** —— 它是「现在去平台问了一次」的结果,
    /// 关掉接入屏就该忘掉。
    pub repos: crate::vm::RepoPickerVm,
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
        Event::SessionReopened { live, .. } => {
            if *live {
                "这场会话本来就开着".into()
            } else {
                "把上次那场会话接回来了 —— 它不会自己动手,你说话它才动".into()
            }
        }
        Event::ProjectRemoved {
            slug,
            issues,
            workspace,
        } => format!(
            "{slug} 已从工作台移走(连同 {issues} 张活的账)。**仓一个字节都没动**,还在 {workspace}"
        ),
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
        Event::WeekPlanStarted {
            week,
            draft_titles,
            ..
        } => {
            if draft_titles.is_empty() {
                // 真跑:文件由 agent 会话产出、走 MR,这里没有骨架也没有草稿。
                format!("运作活①已开工:{week} 的周计划由这场会话产出,提交走 MR,人合入后总览才亮")
            } else {
                format!("{week} 的周计划文件已写出(流程演示),等你确认草稿")
            }
        }
        Event::HistoryBackfilled { note, .. } => note.clone(),
        Event::WorkspacesRootChanged { path, pinned } => {
            let tail = if *pinned > 0 {
                format!(",已接入的 {pinned} 个项目已就地钉在原位置、不会跟着搬")
            } else {
                ",已接入的项目都在原处".to_string()
            };
            format!("工作区根目录改成了 {path}{tail}")
        }
        Event::IssueSubmitted {
            branch,
            commits,
            pr_number,
            note,
            ..
        } => {
            if *pr_number > 0 {
                format!("`{branch}` 上 {commits} 个提交已推上去,MR {note},活进「评审中」")
            } else {
                // 没有 MR 的时候必须把原因原样端出来 —— 只说「已提交」会让人
                // 以为远端已经有东西等着评审了。
                format!("`{branch}` 上 {commits} 个提交已提交,活进「评审中」。没有 MR:{note}")
            }
        }
        Event::IssueMerged {
            pr_number,
            merged,
            local_note,
            ..
        } => {
            if *merged {
                // 本机收尾的下落必须跟着一起说 —— 拉没拉到最新决定了工作区里
                // 那几份 `.bw/` 件读不读得到,人得当场知道。
                format!("MR #{pr_number} 已合入,活已完成。{local_note}")
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

impl UiState {
    /// 写一条回执,并把序号往前推一格。**所有写 `note` 的地方都走这里**,
    /// 漏掉一处就会出现「关过一次之后同样的话再也不弹」。
    pub fn set_note(&mut self, note: Option<String>) {
        self.note = note;
        self.note_seq = self.note_seq.wrapping_add(1);
    }
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
        // 未读 = 评审中/阻塞里、更新时间晚于「读到这里」那一下的。没点过通知屏
        // 就是全部算未读。这个数是现算的,库里没有未读表。
        let seen_at: i64 = store
            .meta(&bw_v4::store::notify_seen_key(p.id))
            .await
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let unread = store.count_unseen(p.id, seen_at).await.unwrap_or(0);
        let week_goal = week_plan_file::read(&ws, &week)
            .ok()
            .flatten()
            .filter(|pl| pl.has_goal())
            .and_then(|pl| pl.goal)
            .unwrap_or_default();
        // 上次交付 = 发版记录最后一行。仓里没有那份表就留空,不拿最近一次
        // commit 冒充「交付」——提交不是交付。
        let last_delivery = release_file::read(&ws)
            .ok()
            .flatten()
            .and_then(|rows| rows.last().cloned())
            .map(|r| format!("{} {}", r.released_at, r.version))
            .unwrap_or_default();
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
            version: file.current_version.clone(),
            week,
            unread,
            week_goal,
            last_delivery,
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
        repos: ui.repos.clone(),
        open,
        settings: SettingsVm {
            workspaces_root: ui.workspaces_root.clone(),
            db_path: ui.db_path.clone(),
            claude_binary: bw_engine::resolve_claude_binary(None),
        },
        note: ui.note.clone(),
        note_seq: ui.note_seq,
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
    //
    // 六项分成两类,**界面上分得清**:能在 `PATH` 里当场找出来的(claude /
    // cursor-agent / codehub / gh)给 `Some(..)`,红绿都是真的;还没接实现的
    // (Open Design 内嵌、welink-cli)给 `None` —— 灰,不是绿,也不是红。
    let claude = crate::adapters::claude_cli::detect();
    let cursor = bw_engine::which_on_path("cursor-agent");
    let codehub = bw_engine::which_on_path("codehub");
    let gh = bw_engine::which_on_path("gh");
    vec![
        ToolProbeVm {
            name: "claude_cli".into(),
            label: "claude".into(),
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
            label: "cursor-agent".into(),
            ok: Some(cursor.is_some()),
            detail: cursor.unwrap_or_else(|| "本机路径里找不到 cursor-agent".into()),
        },
        ToolProbeVm {
            name: "codehub".into(),
            label: "codehub-cli".into(),
            ok: Some(codehub.is_some()),
            detail: codehub.unwrap_or_else(|| "本机路径里找不到 codehub".into()),
        },
        ToolProbeVm {
            name: "gh".into(),
            label: "GitHub CLI".into(),
            ok: Some(gh.is_some()),
            // 装没装能当场看出来;**登录没登录看不出来** —— 那要跑
            // `gh auth status`,起子进程的事没接,就别拿「装了」冒充「登录了」。
            detail: gh
                .map(|p| format!("{p}(登录态没探,探它要起子进程)"))
                .unwrap_or_else(|| "本机路径里找不到 gh".into()),
        },
        ToolProbeVm {
            name: "open_design".into(),
            label: "Open Design".into(),
            ok: None,
            detail: "内嵌还没接,探不出结果".into(),
        },
        ToolProbeVm {
            name: "welink_cli".into(),
            label: "welink-cli".into(),
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
    let conversations = store.conversations(id).await.unwrap_or_default();
    // 「读到这里」是哪一刻。通知屏和项目轨的红点用的是同一个值,读一次就够。
    let notify_seen: Option<i64> = store
        .meta(&bw_v4::store::notify_seen_key(id))
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok());
    let sessions = build_sessions(app, id, &issues).await;
    // 详情抽屉里那两条链接的前缀。读 `.git/config`,不起子进程。
    let browse_base = bw_v4::git::browse_base(&ws).unwrap_or_default();
    let current_week = bw_v4::isoweek::current_week();
    let viewing_week = if ui.viewing_week.is_empty() {
        current_week.clone()
    } else {
        ui.viewing_week.clone()
    };

    // 健康:三条判据当场从仓文件与 git 取,不读库里那两个缓存列。
    let inputs = bw_v4::app::collect_health_inputs(&ws, &current_week).await;
    let derived = bw_v4::derive::derive_project_health(&inputs);
    // 算完顺手写回显示缓存。**项目墙只有这一个数据来源** —— 它要在不打开项目
    // 的情况下列出 N 个项目的灯,不能每次启动扫 N 个仓。此前壳里没有一处写过
    // 这两列,于是项目墙的灯永远是灰的,哪怕总览上算出来是黄的。写的是刚算出
    // 来的那个值,不重算(重算要再起一遍 git 子进程)。
    let _ = store.cache_project_health(id, &derived).await;

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

    // 本周计数与本周运作点在构造之前算好 —— `current_week` 会被 move 进结构体。
    let week_counts_now = build_week_counts(&issues, Some(current_week.as_str()));
    let ops_now = build_ops(&issues, current_week.as_str());

    // 周列表 = 扫 `.bw/plan/` 目录 ∪ 库里活排过的周(设计 06 §2.1 的并集),
    // 且**本周永远在列表里**——空的本周才有地方触发「开始本周」,真跑路径下
    // 文件要等 MR 合入才落地,列表不能因此没有本周。
    let mut weeks = build_weeks(&ws);
    for w in issues.iter().map(|i| i.week_of.as_str()) {
        if !w.is_empty() && !weeks.iter().any(|x| x.week == w) {
            weeks.push(WeekVm {
                week: w.to_string(),
                backfill: false,
                goal: None,
                activity_count: 0,
            });
        }
    }
    if !weeks.iter().any(|x| x.week == current_week) {
        weeks.push(WeekVm {
            week: current_week.clone(),
            backfill: false,
            goal: None,
            activity_count: 0,
        });
    }
    // 新的在前;没有未来周,所以本周自然排在最上面。
    weeks.sort_by(|a, b| b.week.cmp(&a.week));

    // 横幅判据:文件在不在(不看列表),以及本周运作活①走到哪了。已完成的
    // 不算「在途」——文件若还是没有,人应该能再点一次「开始本周」。
    let week_file_exists = week_plan_file::exists(&ws, &current_week);
    let ops1_status = issues
        .iter()
        .filter(|i| {
            i.kind == bw_v4::model::IssueKind::Ops
                && i.workflow == bw_v4::app::OPS1_WORKFLOW
                && i.week_of == current_week
                && i.status != IssueStatus::Done
        })
        .max_by_key(|i| i.number)
        .map(|i| i.status.label().to_string());
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
        week_file_exists,
        ops1_status,
        weeks,
        current_week,
        viewing_week: viewing_week.clone(),
        view_all: ui.view_all,
        board: build_board(
            &issues,
            if ui.view_all {
                None
            } else {
                Some(viewing_week.as_str())
            },
            policy.as_ref(),
        ),
        // 两份计数,各有各的归属:总览那块标题写着「本周」,就只能是本周;
        // 计划屏的进度条画在它正在看的那个看板上面,就跟着看板的范围走。
        // **共用一份的后果**是人在计划屏点了历史周,总览的「本周」下面摆着
        // 那一周的数。
        week_counts: week_counts_now,
        board_counts: build_week_counts(
            &issues,
            if ui.view_all {
                None
            } else {
                Some(viewing_week.as_str())
            },
        ),
        ops: ops_now,
        board_ops: build_ops(&issues, viewing_week.as_str()),
        // 采不采由界面点 —— 现算一次要起好几个 git 子进程,不能每次重拼
        // ViewModel 都跑一遍(人打字时每 30ms 就重拼一次)。
        repo_stats: ui.repo_stats.clone(),
        card_mr: build_card_mr(&issues),
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
        selected_issue: ui.selected_issue,
        browse_base: browse_base.clone(),
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
            // 评审中 **且真有 MR**。没 MR 的评审中活不是通知 —— 它在计划屏的
            // 评审中那一列里,人在那儿点完成。
            to_merge: issues
                .iter()
                .filter(|i| i.status == IssueStatus::InReview && i.pr_number > 0)
                .map(card_item)
                .collect(),
            seen_at: notify_seen,
            events: build_notify_events(&issues, &conversations),
            unread: store
                .count_unseen(id, notify_seen.unwrap_or(0))
                .await
                .unwrap_or(0),
        },
        config: build_config(store, id, &ws, policy.as_ref(), &p, &file).await,
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
        pr_number: i.pr_number,
        remote_number: i.remote_number,
        branch: i.branch.clone(),
        body: i.body.clone(),
    }
}
