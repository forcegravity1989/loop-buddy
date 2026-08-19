//! Issue 看板与 Issue 详情浮层(▶跑 / 评审 / 人点完成 / 蒸馏 的界面入口)。
//! 从 op.rs 机械拆出(2026-08-17),逻辑未改;`IssuesPanel` 由 op.rs 的 `Center` 挂载。

use super::*;

/// Provider-aware web URL for a remote issue. codehub →
/// `https://{domain}/{path}/issues/{iid}`; github → the canonical `github.com`
/// path. Empty path = no remote attached → empty string (caller renders plain
/// text). Bug③+UI: was a hardcoded `github.com` URL even for codehub projects.
fn remote_issue_url(provider: &str, host: &str, path: &str, n: u32) -> String {
    if path.trim().is_empty() {
        return String::new();
    }
    match provider.trim() {
        "codehub" => format!(
            "https://{}/{path}/issues/{n}",
            bw_core::codehub_alias_to_domain(host)
        ),
        _ => format!("https://github.com/{path}/issues/{n}"),
    }
}

/// Provider-aware web URL for a PR/MR. codehub → GitLab-style
/// `/-/merge_requests/{iid}`; github → `/pull/{n}`. Empty path = no remote.
fn remote_mr_url(provider: &str, host: &str, path: &str, n: u32) -> String {
    if path.trim().is_empty() {
        return String::new();
    }
    match provider.trim() {
        "codehub" => {
            format!(
                "https://{}/{path}/-/merge_requests/{n}",
                bw_core::codehub_alias_to_domain(host)
            )
        }
        _ => format!("https://github.com/{path}/pull/{n}"),
    }
}

fn next_issue_status(s: IssueStatus) -> Option<IssueStatus> {
    match s {
        IssueStatus::Backlog => Some(IssueStatus::Todo),
        IssueStatus::Todo => Some(IssueStatus::InProgress),
        IssueStatus::InProgress => Some(IssueStatus::InReview),
        IssueStatus::InReview => Some(IssueStatus::Done),
        IssueStatus::Done | IssueStatus::Blocked | IssueStatus::Cancelled => None,
    }
}

/// `true` for the three states `can_transition_to(Blocked)` actually allows
/// (bw-core's table) — only these get the "⛔ 阻塞" action.
fn can_block(s: IssueStatus) -> bool {
    matches!(
        s,
        IssueStatus::Todo | IssueStatus::InProgress | IssueStatus::InReview
    )
}

/// The Issue board (R1): real assignable work units grouped by status into
/// columns, each card carrying its stage + agent teammate + a one-click
/// advance to the next status. The create strip scopes a new issue to a
/// chosen stage. Every card is a real `issue` row — nothing invented.
///
/// A5-H adds: a real assign dropdown (was static text), a Blocked column
/// (previously invisible on the board — a stuck issue used to vanish from
/// view), and the only path to/from Blocked (reason required going in,
/// two explicit outs coming back). Cancelled stays off-board by design
/// (dropped work, not a state to manage from here).
#[component]
pub(super) fn IssuesPanel(op: OpVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let border = theme::BORDER;
    let ink = theme::INK;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let alert = theme::ALERT_DEEP;
    let mono = theme::MONO;
    let initial_stage = op.active_stage;
    let mut new_title = use_signal(String::new);
    let mut new_stage = use_signal(move || initial_stage);
    // P3: 关联技能选择器只列 content 非空的行 —— 空壳技能选了也注入不了
    // (`standard_skill_block` 的诚实降级口径),不该出现在选项里。
    // plan/20 R1: 池 = 本项目行 + 全局基础库行,他项目的行绝不出现;
    // 种A(工作区登记行)照旧排除。
    let skill_choices: Vec<_> = op
        .hub
        .skills
        .iter()
        .filter(|s| {
            bw_core::scope::in_scope(s.project_id, Some(op.id))
                && !s.content.trim().is_empty()
                && !s.is_project_assets
        })
        .cloned()
        .collect();
    let mut new_skill = use_signal(String::new);
    // plan/20 R1(plan/08 S1 完成标准原文):「指派下拉只出现自己的五个
    // 角色」——严格只列本项目自有队友(W1 出生/补种保证每个项目都有)。
    let agents: Vec<_> = op
        .hub
        .agents
        .iter()
        .filter(|a| a.project_id == Some(op.id) && !a.is_project_assets)
        .cloned()
        .collect();
    // Board-wide: at most one card is "entering a block reason" at a time.
    // Fully qualified: `Signal` bare would resolve to `bw_core::model::Signal`
    // (the derived-health enum), already imported unqualified above.
    let mut blocking: dioxus::prelude::Signal<Option<IssueId>> = use_signal(|| None);
    let mut block_reason = use_signal(String::new);
    // V1-TermRefactor4: 点卡立刻亮「恢复中…」(dispatch 返回前 Vm 还不更新);
    // 与 op.pty_restoring 合并;焦点已活且 App 恢复标记已清 → 不再显示。
    let mut restoring_issue: dioxus::prelude::Signal<Option<IssueId>> = use_signal(|| None);
    // Bug B: merge takes seconds; Vm only rebuilds after dispatch returns, so
    // a local busy flag is the only immediate feedback. Shared with the
    // detail overlay so board + popup stay in sync. Cleared when a merge
    // result toast arrives (success or fail) — not merely when status moves,
    // because a failed merge stays InReview.
    let merging_issue: dioxus::prelude::Signal<Option<IssueId>> = use_signal(|| None);
    let mut merge_note_listener = use_signal(|| false);
    if !merge_note_listener() {
        merge_note_listener.set(true);
        let mut rx = k.notes();
        let mut merging_clear = merging_issue;
        spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(crate::kernel::UiNote::ConnectorSynced { name, detail, .. }) => {
                        let merge_related = name.contains("· merge") || name.contains("· 验收");
                        if merge_related && !detail.contains("正在合入") {
                            merging_clear.set(None);
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
    }

    let cols: [(IssueStatus, &str); 6] = [
        (IssueStatus::Backlog, "待办池"),
        (IssueStatus::Todo, "待办"),
        (IssueStatus::InProgress, "进行中"),
        (IssueStatus::InReview, "评审中"),
        (IssueStatus::Done, "已完成"),
        (IssueStatus::Blocked, "阻塞"),
    ];
    // Precompute the columns outside rsx so the board stays borrow-clean.
    let grouped: Vec<_> = cols
        .iter()
        .map(|(st, label)| {
            (
                *label,
                op.issues
                    .iter()
                    .filter(|i| i.status == *st)
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        })
        .collect();

    rsx! {
        div { style: "max-width:1120px;",
            div {
                style: "{card} padding:12px 16px;margin-bottom:16px;display:flex;gap:10px;align-items:center;flex-wrap:wrap;",
                input {
                    value: "{new_title}",
                    placeholder: "新 Issue 标题(作用域到选中阶段)…",
                    style: "flex:1;min-width:220px;border:1px solid {border};border-radius:7px;padding:8px 11px;font-size:13px;background:#FFF;",
                    oninput: move |e| new_title.set(e.value()),
                }
                for s in StageKind::ALL {
                    {
                        let sel = new_stage() == s;
                        let (bg, fg) = if sel { (clay, "#FFF") } else { ("transparent", ink2) };
                        rsx! {
                            button {
                                key: "{s:?}",
                                style: "cursor:pointer;border:1px solid {border};border-radius:20px;background:{bg};color:{fg};padding:5px 12px;font-size:12px;",
                                onclick: move |_| new_stage.set(s),
                                "{s.label()}"
                            }
                        }
                    }
                }
                select {
                    style: "border:1px solid {border};border-radius:7px;padding:7px 9px;font-size:12px;background:#FFF;color:{ink2};max-width:240px;",
                    title: "关联技能(可空)——不选时开工用本阶段默认方法,选了则替换默认",
                    value: "{new_skill}",
                    onchange: move |e| new_skill.set(e.value()),
                    {
                        let default_slug = bw_core::playbook::stage_skills(new_stage())
                            .first()
                            .map(|s| s.name)
                            .unwrap_or("");
                        rsx! {
                            option { value: "", "不选(用本阶段默认: {default_slug})" }
                        }
                    }
                    for s in skill_choices.iter() {
                        option {
                            key: "{s.id:?}",
                            value: "{s.name}",
                            title: "{s.desc}",
                            "{s.name}"
                        }
                    }
                }
                button {
                    style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:8px 16px;font-size:13px;flex:none;",
                    onclick: move |_| {
                        let t = new_title().trim().to_string();
                        if !t.is_empty() {
                            k.send(Command::CreateIssue {
                                id: IssueId::new(),
                                stage: new_stage(),
                                title: t,
                                desc: String::new(),
                                priority: IssuePriority::Medium,
                                standard_skill: new_skill(),
                            });
                            new_title.set(String::new());
                            new_skill.set(String::new());
                        }
                    },
                    "＋ 创建 Issue"
                }
                {
                    let has_remote = !op.remote_path.trim().is_empty();
                    let k_sync = k.clone();
                    rsx! {
                        if has_remote {
                            button {
                                style: "cursor:pointer;border:1px solid {border};border-radius:7px;background:#FFF;color:{ink2};padding:8px 14px;font-size:12px;flex:none;",
                                title: "从仓平台拉 open Issue 到本地看板(不新建远端;不改本地完成态)",
                                onclick: move |_| k_sync.send(Command::SyncRemoteIssues),
                                "↻ 从仓同步 Issue"
                            }
                        }
                    }
                }
            }
            // P4: evidence overlay — covers the Issue board center only.
            // Must NOT use viewport `fixed;inset:0`: that painted over the
            // left session rail too, so after opening a card from the board
            // the sidebar looked dead (clicks hit the dimmer). Absolute
            // within this relative board root keeps LeftRail clickable.
            div { style: "position:relative;min-height:60vh;",
            if let Some(d) = op.issue_detail.clone() {
                IssueDetailOverlay {
                    can_consult: op.consultable_issues.contains(&d.id),
                    sessions: op.sessions.clone(),
                    active_run: op.active_run,
                    project_id: op.id,
                    merging_issue: merging_issue,
                    d: d,
                }
            }
            div { style: "display:flex;gap:12px;align-items:flex-start;",
                for (label, list) in grouped {
                    div { key: "{label}", style: "flex:1;min-width:190px;",
                        div { style: "font-size:11.5px;color:{ink3};margin-bottom:9px;letter-spacing:.04em;", "{label} · {list.len()}" }
                        for i in list {
                            {
                                // One clone per closure below — each `move`
                                // closure needs to independently own a
                                // `Kernel`, since only one of a card's several
                                // buttons ever fires but Rust still has to
                                // typecheck every branch.
                                let k_select = k.clone();
                                let k_a = k.clone();
                                let k_b = k.clone();
                                let k_run = k.clone();
                                let k_merge = k.clone();
                                let k_detail = k.clone();
                                let k_cancel = k.clone();
                                let agents = agents.clone();
                                let i_id = i.id;
                                // plan/17 S3: is THIS card's run in flight?
                                // (`active_run` carries (project, issue).) And is
                                // a same-project sibling in flight (serial lock
                                // → 「▶ 跑」 greyed)? A run on another project
                                // doesn't block this card.
                                let is_running = op.active_run == Some((op.id, i_id));
                                let is_focused = op.focused_issue == Some(i_id);
                                let can_consult = op.consultable_issues.contains(&i_id);
                                let can_resume = op.resumable_issues.contains(&i_id);
                                let resume_ready = op.focused_issue == Some(i_id)
                                    && op.pty_active
                                    && op.pty_restoring.is_none();
                                let is_restoring = (restoring_issue() == Some(i_id)
                                    && !resume_ready)
                                    || (op.pty_restoring.is_some()
                                        && op.focused_issue == Some(i_id));
                                let same_project_busy =
                                    op.active_run.map(|(p, _)| p) == Some(op.id);
                                // plan/17 S3: the 「▶ 跑」 button's label /
                                // cursor / color when a same-project run is in
                                // flight (serial lock — RunIssue is rejected
                                // server-side; the UI just tells the truth).
                                let (run_label, run_cursor, run_color) = if same_project_busy {
                                    ("▶ 跑(排队中)".to_string(), "not-allowed", ink3)
                                } else {
                                    ("▶ 跑".to_string(), "pointer", clay)
                                };
                                // P3: only work not yet under review / settled
                                // can be started from the board — same states
                                // `RunIssue` itself accepts (guard lives in
                                // bw-app; this just hides a doomed button).
                                let runnable = matches!(
                                    i.status,
                                    IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
                                );
                                let run_stage = i.stage;
                                let run_sess_title = format!("#{} {}", i.number, i.title);
                                let op_sessions = op.sessions.clone();
                                let advance = next_issue_status(i.status);
                                let advance_label = advance.map(|s| s.label()).unwrap_or("");
                                let is_blocked = i.status == IssueStatus::Blocked;
                                let entering_reason = blocking() == Some(i_id);
                                let card_left = if is_focused { clay } else { i.status_color };
                                let card_extra = if is_focused {
                                    format!("box-shadow:inset 0 0 0 1px {clay};")
                                } else {
                                    String::new()
                                };
                                rsx! {
                                    div {
                                        key: "{i.number}",
                                        style: "{card} padding:10px 12px;margin-bottom:9px;border-left:3px solid {card_left};{card_extra}",
                                        div { style: "font-size:11px;color:{ink3};font-family:{mono};display:flex;align-items:center;gap:6px;",
                                            span { "#{i.number} · {i.stage.label()}" }
                                            if is_focused {
                                                span { style: "color:{clay};border:1px solid {clay};border-radius:4px;padding:0 5px;font-size:10px;", "当前会话" }
                                            }
                                            if is_restoring {
                                                span { style: "color:{ink3};border:1px solid {border};border-radius:4px;padding:0 5px;font-size:10px;", "恢复中…" }
                                            }
                                        }
                                        if i.github_number != 0 && i.pr_number == 0
                                            && i.status == IssueStatus::InProgress
                                        {
                                            div {
                                                style: "font-size:10.5px;color:{ink3};font-family:{mono};margin-top:2px;",
                                                "开放 MR 检出后进评审中（通常十几秒内）"
                                            }
                                        }
                                        // C4 · issue 身份映射: 号非 0 才渲染。
                                        // Bug③+UI: provider-aware link to the
                                        // remote issue (codehub `{host}/{path}/issues`
                                        // / github `github.com/.../issues`), shown
                                        // as a short label not a raw URL. Empty path
                                        // = no remote → plain text.
                                        if i.github_number != 0 {
                                            div {
                                                style: "font-size:10.5px;color:{ink3};font-family:{mono};margin-top:1px;",
                                                if op.remote_path.trim().is_empty() {
                                                    "远端 #{i.github_number}"
                                                } else {
                                                    a {
                                                        href: "{remote_issue_url(&op.provider, &op.remote_host, &op.remote_path, i.github_number)}",
                                                        target: "_blank",
                                                        style: "color:{ink3};text-decoration:none;",
                                                        "远端 #{i.github_number} ↗"
                                                    }
                                                }
                                            }
                                        }
                                        // C5 · PR 验收环: 有 PR 号才渲染,如实展示
                                        // 「PR #N」——验收=人 merge,号非 0 即有开放 PR。
                                        // Bug③+UI: link to the MR/PR web URL.
                                        if i.pr_number != 0 {
                                            div {
                                                style: "font-size:10.5px;color:{clay};font-family:{mono};margin-top:1px;",
                                                if op.remote_path.trim().is_empty() {
                                                    "PR #{i.pr_number}"
                                                } else {
                                                    a {
                                                        href: "{remote_mr_url(&op.provider, &op.remote_host, &op.remote_path, i.pr_number)}",
                                                        target: "_blank",
                                                        style: "color:{clay};text-decoration:none;",
                                                        "PR #{i.pr_number} ↗"
                                                    }
                                                }
                                            }
                                        }
                                        // P4: the title opens the evidence
                                        // overlay (runs / diffs / artifacts).
                                        div {
                                            style: "font-size:13px;margin:3px 0 4px;color:{ink};cursor:pointer;",
                                            onclick: move |_| {
                                                // 重启后点卡:立刻亮「恢复中…」,内核走 OpenIssueDetail 唤醒。
                                                if can_resume {
                                                    restoring_issue.set(Some(i_id));
                                                }
                                                k_detail.send(Command::OpenIssueDetail(i_id));
                                            },
                                            "{i.title}"
                                        }
                                        div { style: "font-size:11px;color:{ink2};margin-bottom:5px;", "{i.priority_label}" }
                                        select {
                                            style: "font-size:11.5px;border:1px solid {border};border-radius:5px;padding:3px 5px;background:#FFF;max-width:100%;",
                                            onchange: move |e| {
                                                let v = e.value();
                                                let assignee = v
                                                    .parse::<usize>()
                                                    .ok()
                                                    .and_then(|idx| agents.get(idx))
                                                    .map(|a| a.id);
                                                k_select.send(Command::AssignIssue { id: i_id, assignee });
                                            },
                                            option { value: "", selected: i.assignee_name.is_none(), "未分配" }
                                            for (idx , a) in agents.iter().enumerate() {
                                                option {
                                                    key: "{idx}",
                                                    value: "{idx}",
                                                    selected: i.assignee_name.as_deref() == Some(a.name.as_str()),
                                                    "{a.name}({a.role})"
                                                }
                                            }
                                        }
                                        if is_blocked {
                                            div {
                                                style: "margin-top:7px;padding:6px 8px;background:#F2E4DD;border-radius:6px;font-size:11.5px;color:{alert};",
                                                "⛔ {i.blocked_reason.clone().unwrap_or_default()}"
                                            }
                                            div { style: "margin-top:6px;display:flex;gap:10px;",
                                                button {
                                                    style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;",
                                                    onclick: move |_| k_a.send(Command::TransitionIssue { id: i_id, status: IssueStatus::Todo }),
                                                    "解除→待办"
                                                }
                                                button {
                                                    style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;",
                                                    onclick: move |_| k_b.send(Command::TransitionIssue { id: i_id, status: IssueStatus::InProgress }),
                                                    "解除→进行中"
                                                }
                                            }
                                        } else if entering_reason {
                                            div { style: "margin-top:7px;",
                                                input {
                                                    value: "{block_reason}",
                                                    placeholder: "阻塞原因(必填)…",
                                                    style: "width:100%;font-size:11.5px;border:1px solid {border};border-radius:5px;padding:4px 7px;background:#FFF;",
                                                    oninput: move |e| block_reason.set(e.value()),
                                                }
                                                div { style: "margin-top:5px;display:flex;gap:10px;",
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{alert};font-size:11.5px;padding:0;",
                                                        onclick: move |_| {
                                                            let reason = block_reason().trim().to_string();
                                                            if !reason.is_empty() {
                                                                k_a.send(Command::BlockIssue { id: i_id, reason });
                                                                blocking.set(None);
                                                            }
                                                        },
                                                        "确认阻塞"
                                                    }
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:11.5px;padding:0;",
                                                        onclick: move |_| blocking.set(None),
                                                        "取消"
                                                    }
                                                }
                                            }
                                        } else {
                                            div { style: "margin-top:6px;display:flex;gap:12px;",
                                                // P3: really start the work —
                                                // the same StartSession +
                                                // RunIssue path, real or mock
                                                // per project config. Mock
                                                // projects run self-labeled.
                                                // plan/17 S3 (① 中止): when this
                                                // card's run is in flight, show
                                                // 「⬇ 终止」 instead of 「▶ 跑」
                                                // (aborts the backgrounded run,
                                                // issue stays InProgress, never
                                                // auto-Done — 铁律). When a
                                                // same-project sibling is
                                                // running, grey 「▶ 跑」 (serial
                                                // lock — RunIssue would be
                                                // rejected anyway; honest UI).
                                                if is_running {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{alert};font-size:11.5px;padding:0;font-weight:700;",
                                                        onclick: move |_| {
                                                            k_cancel.send(Command::CancelRun { id: i_id });
                                                        },
                                                        "⬇ 终止"
                                                    }
                                                } else if runnable {
                                                    button {
                                                        style: "cursor:{run_cursor};background:transparent;border:none;color:{run_color};font-size:11.5px;padding:0;font-weight:700;",
                                                        disabled: same_project_busy,
                                                        onclick: move |_| {
                                                            if can_resume {
                                                                restoring_issue.set(Some(i_id));
                                                            }
                                                            let sid = existing_issue_session(
                                                                &op_sessions,
                                                                run_stage,
                                                                &run_sess_title,
                                                            )
                                                            .unwrap_or_else(SessionId::new);
                                                            k_run.send(Command::StartSession {
                                                                id: sid,
                                                                stage_kind: Some(run_stage),
                                                                kind: SessionKind::Create,
                                                                title: run_sess_title.clone(),
                                                            });
                                                            k_run.send(Command::RunIssue { session: sid, id: i_id });
                                                            // Jump the user straight to the session/terminal that just
                                                            // started (or resumed) — otherwise the board gives no visible
                                                            // feedback at all that anything happened, which is exactly
                                                            // what made repeat clicks feel like they were silently
                                                            // spawning duplicates instead of reusing the existing run.
                                                            k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                                            k_run.send(Command::SetPanel(Panel::Workflow));
                                                            k_run.send(Command::SelectSession(Some(sid)));
                                                        },
                                                        "{run_label}"
                                                    }
                                                } else if can_consult {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;font-weight:700;",
                                                        onclick: move |_| {
                                                            restoring_issue.set(Some(i_id));
                                                            let sid = existing_issue_session(
                                                                &op_sessions,
                                                                run_stage,
                                                                &run_sess_title,
                                                            )
                                                            .unwrap_or_else(SessionId::new);
                                                            k_run.send(Command::StartSession {
                                                                id: sid,
                                                                stage_kind: Some(run_stage),
                                                                kind: SessionKind::Create,
                                                                title: run_sess_title.clone(),
                                                            });
                                                            k_run.send(Command::RunIssue { session: sid, id: i_id });
                                                            k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                                            k_run.send(Command::SetPanel(Panel::Workflow));
                                                            k_run.send(Command::SelectSession(Some(sid)));
                                                        },
                                                        "续聊"
                                                    }
                                                }
                                                // C5 · PR 验收环: InReview + 有 PR 时,
                                                // merge 是首选验收路径(人 merge → 关单)。
                                                // 不硬拦下面的 →已完成(只留痕不拦人)。
                                                if i.status == IssueStatus::InReview && i.pr_number != 0 {
                                                    {
                                                        let is_merging = merging_issue() == Some(i_id);
                                                        let pr_n = i.pr_number;
                                                        let (merge_label, merge_cursor, merge_color) =
                                                            if is_merging {
                                                                (
                                                                    format!("正在合入 PR #{pr_n}…"),
                                                                    "not-allowed",
                                                                    ink3,
                                                                )
                                                            } else {
                                                                (
                                                                    format!("⬇ merge PR #{pr_n}"),
                                                                    "pointer",
                                                                    clay,
                                                                )
                                                            };
                                                        let mut merging_set = merging_issue;
                                                        rsx! {
                                                            button {
                                                                style: "cursor:{merge_cursor};background:transparent;border:none;color:{merge_color};font-size:11.5px;padding:0;font-weight:700;",
                                                                disabled: is_merging,
                                                                onclick: move |_| {
                                                                    if merging_set() == Some(i_id) {
                                                                        return;
                                                                    }
                                                                    merging_set.set(Some(i_id));
                                                                    k_merge.send(Command::MergeIssuePr { id: i_id });
                                                                },
                                                                "{merge_label}"
                                                            }
                                                        }
                                                    }
                                                }
                                                if let Some(ns) = advance {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{clay};font-size:11.5px;padding:0;",
                                                        onclick: move |_| k_a.send(Command::TransitionIssue { id: i_id, status: ns }),
                                                        "→ {advance_label}"
                                                    }
                                                }
                                                if can_block(i.status) {
                                                    button {
                                                        style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:11.5px;padding:0;",
                                                        onclick: move |_| {
                                                            block_reason.set(String::new());
                                                            blocking.set(Some(i_id));
                                                        },
                                                        "⛔ 阻塞"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            }
        }
    }
}

/// P4: the Issue-detail overlay — the review gate's evidence surface. Every
/// number shown is a stored fact: real runs (status/duration/phases), the
/// files each run really changed (diff between its recorded HEAD pair), and
/// registered artifact versions. Nothing is synthesized; a missing record
/// says so instead of pretending "no changes". Actions dispatch the same
/// guarded commands the board uses — 「确认完成」 is the human's call, here
/// as everywhere.
#[component]
fn IssueDetailOverlay(
    d: ui::vm::IssueDetailVm,
    sessions: Vec<SessionCardVm>,
    active_run: Option<(ProjectId, IssueId)>,
    project_id: ProjectId,
    can_consult: bool,
    mut merging_issue: dioxus::prelude::Signal<Option<IssueId>>,
) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let border = theme::BORDER;
    let ink = theme::INK;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let alert = theme::ALERT_DEEP;
    let mono = theme::MONO;
    let id = d.id;
    let k_close = k.clone();
    let k_close_x = k.clone();
    let k_done = k.clone();
    let k_back = k.clone();
    let k_run = k.clone();
    let k_merge = k.clone();
    let k_distill = k.clone();
    let k_cancel = k.clone();
    let k_promote = k.clone();
    let mut distilling = use_signal(|| false);
    let mut skill_name = use_signal(|| format!("{} · 做法", d.title));
    let mut skill_desc = use_signal(|| format!("来自 Issue #{} 的实战沉淀", d.number));
    let mut skill_content = use_signal(String::new);
    let runnable = matches!(
        d.status,
        IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
    );
    let in_review = d.status == IssueStatus::InReview;
    let done = d.status == IssueStatus::Done;
    let run_stage = d.stage;
    let run_sess_title = format!("#{} {}", d.number, d.title);
    let assignee = d.assignee_name.clone().unwrap_or_else(|| "未分配".into());
    // P2 (2026-08-06 cowelink 验证): 对仗看板卡片(op.rs 列表行 :769-780,930-
    // 941)的同一段 active_run/串行锁判断——弹窗此前完全不看 active_run,
    // 「▶ 跑」永远可点、永远不知道这件活(或同项目另一件)是不是已经在跑。
    let is_running = active_run == Some((project_id, d.id));
    let same_project_busy = active_run.map(|(p, _)| p) == Some(project_id);
    let (run_label, run_cursor, run_color) = if same_project_busy {
        ("▶ 跑(排队中)".to_string(), "not-allowed", ink3)
    } else {
        ("▶ 跑".to_string(), "pointer", clay)
    };

    rsx! {
        div {
            style: "position:absolute;inset:0;background:rgba(35,33,28,.38);z-index:60;display:flex;align-items:flex-start;justify-content:center;padding:48px 16px;",
            // Backdrop click closes — left rail stays outside this absolute
            // layer (parent is the board center `position:relative` root).
            onclick: move |_| k_close.send(Command::CloseIssueDetail),
            div {
                style: "{card} width:720px;max-width:96vw;max-height:82vh;overflow-y:auto;padding:18px 22px;",
                onclick: move |e| e.stop_propagation(),
                // ── header ──
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    div { style: "font-size:11.5px;color:{ink3};font-family:{mono};", "#{d.number} · {d.stage_label} · {d.status_label}" }
                    div { style: "flex:1;" }
                    button {
                        style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:14px;",
                        onclick: move |_| k_close_x.send(Command::CloseIssueDetail),
                        "✕"
                    }
                }
                div { style: "font-size:16px;color:{ink};margin:4px 0 2px;", "{d.title}" }
                div { style: "font-size:12px;color:{ink2};margin-bottom:6px;", "指派:{assignee} · {d.priority_label}" }
                // V2-①: 显式技能 vs 阶段默认——让用户看懂「默认用什么、选自己的会发生什么」。
                {
                    let skill_line = if !d.standard_skill.trim().is_empty() {
                        format!("技能: {} (用户选择,替换阶段默认)", d.standard_skill)
                    } else {
                        let default_slug = bw_core::playbook::stage_skills(d.stage)
                            .first()
                            .map(|s| s.name)
                            .unwrap_or("无");
                        format!("技能: {} (本阶段默认,不选技能时用此)", default_slug)
                    };
                    rsx! {
                        div { style: "font-size:11.5px;color:{ink3};font-family:{mono};margin-bottom:6px;", "{skill_line}" }
                    }
                }
                // C5 · PR 验收环: 有 PR 号如实展示,验收=人 merge。
                if d.pr_number != 0 {
                    div { style: "font-size:11.5px;color:{clay};font-family:{mono};margin-bottom:6px;", "PR #{d.pr_number} · 等待人工 merge 验收" }
                }
                if let Some(reason) = d.blocked_reason.clone() {
                    div { style: "margin:6px 0;padding:6px 9px;background:#F2E4DD;border-radius:6px;font-size:12px;color:{alert};", "⛔ {reason}" }
                }
                if !d.desc.trim().is_empty() {
                    div { style: "font-size:12.5px;color:{ink2};white-space:pre-wrap;margin:6px 0 10px;line-height:1.7;", "{d.desc}" }
                }

                // ── runs + real changes ──
                div { style: "font-size:12px;color:{ink3};letter-spacing:.05em;margin:12px 0 6px;", "运行史({d.runs.len()})" }
                if d.runs.is_empty() {
                    if d.is_interactive {
                        // P2: 交互式活(找指标/绑数据)不写 workflow_run——
                        // 「还没有运行」对已经跑过的交互式活是假话,过程在
                        // 嵌入终端/claude 会话里,不在这张运行史列表里。
                        div { style: "font-size:12px;color:{ink3};", "交互式活不写运行史——过程在下方嵌入终端 / claude 会话里,不是没跑过。" }
                    } else {
                        div { style: "font-size:12px;color:{ink3};", "还没有运行——「▶ 跑」会真实开工并留痕。" }
                    }
                }
                for (ri , r) in d.runs.iter().enumerate() {
                    div {
                        key: "{ri}",
                        style: "border:1px solid {border};border-radius:8px;padding:8px 11px;margin-bottom:8px;",
                        div { style: "font-size:12px;color:{ink};font-family:{mono};",
                            if r.ok {
                                span { style: "color:#5F7355;", "● {r.status_label}" }
                            } else {
                                span { style: "color:{alert};", "● {r.status_label}" }
                            }
                            span { style: "color:{ink3};", " · {r.trigger_label} · {r.duration_label} · {r.phases_label}" }
                        }
                        if !r.error.is_empty() {
                            div { style: "font-size:11.5px;color:{alert};margin-top:4px;white-space:pre-wrap;", "{r.error}" }
                        }
                        if let Some(why) = r.changes_unavailable.clone() {
                            div { style: "font-size:11.5px;color:{ink3};margin-top:5px;", "变更:{why}" }
                        } else if r.changes.is_empty() {
                            div { style: "font-size:11.5px;color:{ink3};margin-top:5px;", "变更:本次运行没有提交任何文件改动(如实)。" }
                        } else {
                            div { style: "margin-top:5px;",
                                for (ci , (path , add , del)) in r.changes.iter().enumerate() {
                                    div {
                                        key: "{ci}",
                                        style: "font-size:11.5px;font-family:{mono};color:{ink2};display:flex;gap:8px;",
                                        span { style: "flex:1;overflow:hidden;text-overflow:ellipsis;", "{path}" }
                                        span { style: "color:#5F7355;", "+{add}" }
                                        span { style: "color:{alert};", "-{del}" }
                                    }
                                }
                            }
                        }
                    }
                }

                // ── artifacts ──
                div { style: "font-size:12px;color:{ink3};letter-spacing:.05em;margin:12px 0 6px;", "产物登记({d.artifacts.len()})" }
                if d.artifacts.is_empty() {
                    div { style: "font-size:12px;color:{ink3};", "尚无登记——确认完成时会扫描工作区并登记(带险不登)。" }
                }
                for (ai , (path , commit , bytes)) in d.artifacts.iter().enumerate() {
                    div {
                        key: "{ai}",
                        style: "font-size:11.5px;font-family:{mono};color:{ink2};display:flex;gap:10px;",
                        span { style: "flex:1;overflow:hidden;text-overflow:ellipsis;", "{path}" }
                        span { style: "color:{ink3};", "{commit} · {bytes}B" }
                    }
                }

                // ── actions(status-gated;same guarded commands as the board)──
                div { style: "display:flex;gap:14px;margin-top:16px;align-items:center;flex-wrap:wrap;",
                    if is_running {
                        // P2: 对仗看板卡片 :930-937 — 这件活正在跑,给「⬇ 终止」
                        // 而不是一个假装可点的「▶ 跑」。
                        button {
                            style: "cursor:pointer;border:none;border-radius:7px;background:transparent;border:1px solid {alert};color:{alert};padding:6px 15px;font-size:12.5px;font-weight:700;",
                            onclick: move |_| k_cancel.send(Command::CancelRun { id }),
                            "⬇ 终止"
                        }
                    } else if runnable {
                        button {
                            style: "cursor:{run_cursor};border:none;border-radius:7px;background:{run_color};color:#FFF;padding:7px 16px;font-size:12.5px;",
                            disabled: same_project_busy,
                            onclick: move |_| {
                                if same_project_busy {
                                    return;
                                }
                                let sid = existing_issue_session(&sessions, run_stage, &run_sess_title)
                                    .unwrap_or_else(SessionId::new);
                                k_run.send(Command::StartSession {
                                    id: sid,
                                    stage_kind: Some(run_stage),
                                    kind: SessionKind::Create,
                                    title: run_sess_title.clone(),
                                });
                                k_run.send(Command::RunIssue { session: sid, id });
                                // Same as the board's 「▶ 跑」 — jump straight to the
                                // session/terminal instead of leaving the user staring at
                                // the (now stale) detail overlay with no visible sign
                                // anything started.
                                k_run.send(Command::CloseIssueDetail);
                                k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                k_run.send(Command::SetPanel(Panel::Workflow));
                                k_run.send(Command::SelectSession(Some(sid)));
                            },
                            "{run_label}"
                        }
                    } else if can_consult {
                        button {
                            style: "cursor:pointer;border:none;border-radius:7px;background:transparent;border:1px solid {clay};color:{clay};padding:7px 16px;font-size:12.5px;",
                            onclick: move |_| {
                                let sid = existing_issue_session(&sessions, run_stage, &run_sess_title)
                                    .unwrap_or_else(SessionId::new);
                                k_run.send(Command::StartSession {
                                    id: sid,
                                    stage_kind: Some(run_stage),
                                    kind: SessionKind::Create,
                                    title: run_sess_title.clone(),
                                });
                                k_run.send(Command::RunIssue { session: sid, id });
                                k_run.send(Command::CloseIssueDetail);
                                k_run.send(Command::SetScope(Scope::Stage(run_stage)));
                                k_run.send(Command::SetPanel(Panel::Workflow));
                                k_run.send(Command::SelectSession(Some(sid)));
                            },
                            "续聊"
                        }
                        // V1-TermRefactor5 · 咨询态:显式「转成新活」(不做自动意图分类)。
                        {
                            let promote_stage = d.stage;
                            let promote_title = d.title.clone();
                            let promote_number = d.number;
                            rsx! {
                                button {
                                    style: "cursor:pointer;border:none;border-radius:7px;background:transparent;border:1px solid {border};color:{ink2};padding:7px 14px;font-size:12.5px;",
                                    title: "在同项目新建一件活,承接咨询里冒出的新交付诉求",
                                    onclick: move |_| {
                                        k_promote.send(Command::CreateIssue {
                                            id: IssueId::new(),
                                            stage: promote_stage,
                                            title: format!("来自咨询：{promote_title}"),
                                            desc: format!(
                                                "从 #{} 「{}」的咨询会话转来。",
                                                promote_number, promote_title
                                            ),
                                            priority: IssuePriority::Medium,
                                            standard_skill: String::new(),
                                        });
                                        k_promote.send(Command::CloseIssueDetail);
                                        k_promote.send(Command::SetPanel(Panel::Issues));
                                    },
                                    "转成新活"
                                }
                            }
                        }
                    }
                    if in_review {
                        // C5 · PR 验收环 (D3): 有 PR → 首选「merge PR」(人 merge →
                        // 关单 → 走现有 InReview→Done 记账)。无 PR(存量/无仓活)→
                        // 保留裸「确认完成」路径(全活 PR 化是纪律不是硬闸)。
                        if d.pr_number != 0 {
                            {
                                let is_merging = merging_issue() == Some(id);
                                let pr_n = d.pr_number;
                                let (merge_label, merge_bg, merge_cursor) = if is_merging {
                                    (
                                        format!("正在合入 PR #{pr_n}…"),
                                        "#B89A8E".to_string(),
                                        "not-allowed",
                                    )
                                } else {
                                    (
                                        format!("⬇ merge PR #{pr_n}(验收)"),
                                        clay.to_string(),
                                        "pointer",
                                    )
                                };
                                rsx! {
                                    button {
                                        style: "cursor:{merge_cursor};border:none;border-radius:7px;background:{merge_bg};color:#FFF;padding:7px 16px;font-size:12.5px;",
                                        disabled: is_merging,
                                        onclick: move |_| {
                                            if merging_issue() == Some(id) {
                                                return;
                                            }
                                            merging_issue.set(Some(id));
                                            k_merge.send(Command::MergeIssuePr { id });
                                            k_merge.send(Command::OpenIssueDetail(id));
                                        },
                                        "{merge_label}"
                                    }
                                }
                            }
                        } else {
                            button {
                                style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:7px 16px;font-size:12.5px;",
                                onclick: move |_| {
                                    k_done.send(Command::TransitionIssue { id, status: IssueStatus::Done });
                                    k_done.send(Command::OpenIssueDetail(id));
                                },
                                "✓ 确认完成(人裁)"
                            }
                        }
                        button {
                            style: "cursor:pointer;border:1px solid {border};border-radius:7px;background:transparent;color:{ink2};padding:7px 14px;font-size:12.5px;",
                            onclick: move |_| {
                                k_back.send(Command::TransitionIssue { id, status: IssueStatus::InProgress });
                                k_back.send(Command::OpenIssueDetail(id));
                            },
                            "↩ 打回"
                        }
                    }
                    if done && !distilling() {
                        button {
                            style: "cursor:pointer;border:1px solid {border};border-radius:7px;background:transparent;color:{clay};padding:7px 14px;font-size:12.5px;",
                            onclick: move |_| distilling.set(true),
                            "⚗ 蒸馏为技能"
                        }
                    }
                    if d.settled {
                        span { style: "font-size:11px;color:{ink3};", "已记账(同一件活绝不记两次)" }
                    }
                }

                // ── distill form(content is the human's judgment — required)──
                if distilling() {
                    div { style: "margin-top:12px;border-top:1px dashed {border};padding-top:12px;",
                        input {
                            value: "{skill_name}",
                            style: "width:100%;font-size:12.5px;border:1px solid {border};border-radius:6px;padding:6px 9px;background:#FFF;margin-bottom:6px;",
                            oninput: move |e| skill_name.set(e.value()),
                        }
                        input {
                            value: "{skill_desc}",
                            style: "width:100%;font-size:12.5px;border:1px solid {border};border-radius:6px;padding:6px 9px;background:#FFF;margin-bottom:6px;",
                            oninput: move |e| skill_desc.set(e.value()),
                        }
                        textarea {
                            value: "{skill_content}",
                            placeholder: "正文(必填,人写):这件活的可复用做法——下次同类活会被真实注入…",
                            style: "width:100%;min-height:110px;font-size:12.5px;border:1px solid {border};border-radius:6px;padding:8px 10px;background:#FFF;font-family:inherit;line-height:1.7;",
                            oninput: move |e| skill_content.set(e.value()),
                        }
                        div { style: "display:flex;gap:12px;margin-top:8px;",
                            button {
                                style: "cursor:pointer;border:none;border-radius:7px;background:{clay};color:#FFF;padding:6px 14px;font-size:12px;",
                                onclick: move |_| {
                                    let content = skill_content().trim().to_string();
                                    let name = skill_name().trim().to_string();
                                    if !content.is_empty() && !name.is_empty() {
                                        k_distill.send(Command::DistillSkillFromIssue {
                                            skill_id: SkillId::new(),
                                            issue_id: id,
                                            name,
                                            desc: skill_desc().trim().to_string(),
                                            category: "孵化沉淀".into(),
                                            content,
                                        });
                                        distilling.set(false);
                                    }
                                },
                                "确认蒸馏"
                            }
                            button {
                                style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:12px;",
                                onclick: move |_| distilling.set(false),
                                "取消"
                            }
                        }
                    }
                }
            }
        }
    }
}
