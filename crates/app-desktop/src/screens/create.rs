//! `view=create` — the creation card-flow (V1 Issue 1 slimmed to 2 cards):
//! Repo (仓从哪来) → Intent (意图 + 确认建立). The old 5-card flow
//! (Questions/Drafting/Review) is cut — cycle/cadence use DB defaults,
//! brief/benchmark/win are optional, and the Intent card's confirm button
//! fires the full CreateProject → UpdateBrief → CompleteCreation sequence.
//!
//! Nothing here fabricates project-specific content. The project row is
//! minted at Intent's confirm (fresh) or already exists (resume). The
//! Bug-B submitting guard lives on the Intent confirm button: set true on
//! click, released by `main.rs` on a real `UiNote::Error` (failure → retry),
//! when Create is not showing (success / cancel / back to wall — the signal
//! lives on the parent, so unmount alone does not reset it), or on "+ 新建".

use crate::kernel::{
    ActionItem, CreateVm, Kernel, ACTION_FAIL_LINGER, ACTION_OK_LINGER, ACTION_PENDING_THRESHOLD,
};
use crate::theme;
use bw_app::{
    CodehubOrigin, Command, GithubOrigin, Panel, RemoteProjectProbe, Scope, ONBOARD_REPO_LIST_LIMIT,
};
use bw_core::model::Cadence;
use bw_core::ProjectId;
use bw_engine::{CodehubRepoSummary, GithubRepoSummary};
use dioxus::prelude::*;

/// Which card of the flow is showing. Local UI navigation only — the real
/// draft lives in [`CreateVm`], sourced from the store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Card {
    Repo,
    Intent,
}

/// The Repo 卡片's local choice — turned into a `GithubOrigin`/`CodehubOrigin`
/// only at `IntentCard`'s confirm time. The `private`/`owner`/`repo` fields
/// are github-specific; codehub uses its own signals (`codehub_namespace`/
/// `codehub_name`/`codehub_visibility` for New, `codehub_path` for Existing).
#[derive(Clone, Debug, PartialEq)]
enum RepoChoice {
    New { private: bool },
    Existing { owner: String, repo: String },
}

#[component]
pub fn Create(
    vm: Option<CreateVm>,
    // Bug B: the Intent card's confirm-button pending guard. Set true on
    // click; released by `main.rs` on a real `UiNote::Error`, when Create
    // is not showing, or on "+ 新建". The signal lives on the parent —
    // unmounting this screen does not reset it.
    submitting: Signal<bool>,
    // plan/14 C14: raw Started/Ok/Fail facts for this flow's background
    // actions (建仓/克隆/仓列表加载/标配建单/落地推送) — rendered by
    // `ActionsBanner` below, visible across every card since the action that
    // started it may finish after the user has already moved on.
    actions: Vec<ActionItem>,
    github_repos: Vec<GithubRepoSummary>,
    codehub_repos: Vec<CodehubRepoSummary>,
    /// V2-② Intent UX: remote `.bw/project.toml` probe for「接入已有仓」.
    remote_project_probe: RemoteProjectProbe,
    on_cancel: EventHandler<()>,
) -> Element {
    let has_project = vm.is_some();
    // Resuming an interrupted creation (OpenProject on a cold-start project)
    // skips straight to Intent — the project row (and its repo, if any)
    // already exists, so only the Intent fields (editable) + confirm remain.
    let mut card = use_signal(move || {
        if has_project {
            Card::Intent
        } else {
            Card::Repo
        }
    });
    let repo_choice = use_signal(|| RepoChoice::New { private: true });
    // C16(plan/14 规范条 4): 仓平台选择器的选中值 —— 默认 "github";
    // codehub 用户点 CodeHub chip 切换即可。纯 UI 状态,与 `repo_choice`
    // (起点:新建/接入)并存、互不影响。
    let platform = use_signal(|| "github".to_string());
    // P4+V1:codehub 平台的仓身份。host = API 域名 alias(green/open/yellow,
    // 引擎用 -H <alias> shell-out codehub-cli);path = org/repo(接入已有);
    // namespace/name/visibility = 新建仓参数。
    let codehub_host = use_signal(|| "open".to_string());
    let codehub_path = use_signal(String::new);
    let codehub_namespace = use_signal(String::new);
    let codehub_name = use_signal(String::new);
    let codehub_visibility = use_signal(|| "private".to_string());
    // PF1-1: GitHub 新建仓的仓名在 RepoCard 填(对仗 codehub_name),不再在
    // IntentCard 第二步填。空时提交闭包仍 slugify(&name()) 兜底(项目名→仓名)。
    let github_slug = use_signal(String::new);

    let serif = theme::SERIF;
    let ink2 = theme::INK_2;

    rsx! {
        div {
            style: "max-width:640px;margin:0 auto;padding:36px 24px 120px;display:flex;flex-direction:column;gap:12px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;margin-bottom:8px;",
                span { style: "font-family:{serif};font-size:17px;font-weight:600;", "新建项目" }
                // C12(plan/14 规范条 1): 永远退得出去 —— 全卡、全状态都有
                // 这条路。`on_cancel` 语义见 main.rs:只清活跃项目指针,不删
                // 项目、不回滚已落库进度——已建的项目留在墙上,重开走
                // cold-start 续。
                button {
                    style: "background:transparent;border:none;color:{ink2};cursor:pointer;font-size:13px;",
                    onclick: move |_| on_cancel.call(()),
                    "← 返回项目墙"
                }
            }
            // plan/14 C14(规范条 2): 后台动作永远有状态回显 —— 建仓/克隆/仓
            // 列表加载/标配建单/落地推送不管走完哪张卡都在这里能看见。
            ActionsBanner { items: actions }
            match card() {
                Card::Repo => rsx! {
                    RepoCard {
                        platform,
                        choice: repo_choice,
                        github_repos: github_repos.clone(),
                        codehub_repos: codehub_repos.clone(),
                        codehub_host,
                        codehub_path,
                        codehub_namespace,
                        codehub_name,
                        codehub_visibility,
                        github_slug,
                        remote_project_probe: remote_project_probe.clone(),
                        on_next: move |_| card.set(Card::Intent),
                    }
                },
                Card::Intent => rsx! {
                    IntentCard {
                        vm: vm.clone(),
                        platform,
                        repo_choice,
                        codehub_host,
                        codehub_path,
                        codehub_namespace,
                        codehub_name,
                        codehub_visibility,
                        github_slug,
                        submitting,
                        remote_project_probe: remote_project_probe.clone(),
                    }
                },
            }
        }
    }
}

// ─────────────────── plan/14 C14 · 后台动作进度条 ───────────────────

/// One [`ActionItem`] resolved into what the strip actually shows —
/// recomputed every render against `Instant::now()`, never stored: see
/// `visible_actions`.
#[derive(Clone, PartialEq)]
enum ActionView {
    Pending { elapsed_secs: u64 },
    Ok,
    Fail(String),
}

/// Render-time visibility gate (体验规范条 2 + 阈值门槛,plan/14 C14): an
/// action that starts *and* resolves inside `ACTION_PENDING_THRESHOLD` never
/// appears at all — 秒级内完成的动作不闪烁噪音. One that does cross the
/// threshold shows `Pending` with a live elapsed count, then transitions to
/// `Ok`(短暂淡出) or `Fail`(带原因,停留更久 —— 与既有 `ConnectorSynced`
/// toast 互为记录,那条不受这里的淡出影响) until its own linger window
/// elapses.
fn visible_actions(items: &[ActionItem], now: std::time::Instant) -> Vec<(String, ActionView)> {
    items
        .iter()
        .filter_map(|it| match &it.resolved {
            None => {
                let elapsed = now.saturating_duration_since(it.started_at);
                (elapsed >= ACTION_PENDING_THRESHOLD).then(|| {
                    (
                        it.name.clone(),
                        ActionView::Pending {
                            elapsed_secs: elapsed.as_secs(),
                        },
                    )
                })
            }
            Some((ok, detail, resolved_at)) => {
                let was_shown = resolved_at.saturating_duration_since(it.started_at)
                    >= ACTION_PENDING_THRESHOLD;
                if !was_shown {
                    return None; // resolved before ever crossing the noise threshold
                }
                let since_resolved = now.saturating_duration_since(*resolved_at);
                let linger = if *ok {
                    ACTION_OK_LINGER
                } else {
                    ACTION_FAIL_LINGER
                };
                (since_resolved < linger).then(|| {
                    let view = if *ok {
                        ActionView::Ok
                    } else {
                        ActionView::Fail(detail.clone())
                    };
                    (it.name.clone(), view)
                })
            }
        })
        .collect()
}

/// The always-mounted-while-creating strip that turns raw `ActionItem`s into
/// live "正在 X … 已 N 秒" / "✓ X" / "✕ X · 原因" rows (plan/14 C14,规范条
/// 2). Self-ticks on a local signal — not driven by the parent re-rendering
/// — because elapsed-seconds/linger-window visibility is wall-clock, not
/// event-driven: without its own clock this would only refresh whenever a
/// *new* Started/Ok/Fail note arrives, freezing the "已 N 秒" count between
/// real events. Bounded lifetime: only mounted while the creation flow is on
/// screen.
#[component]
fn ActionsBanner(items: Vec<ActionItem>) -> Element {
    let mut tick = use_signal(|| 0u32);
    use_future(move || async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            tick.with_mut(|t| *t = t.wrapping_add(1));
        }
    });
    let _ = tick(); // subscribe: forces this component to re-render on tick
    let visible = visible_actions(&items, std::time::Instant::now());
    let ink2 = theme::INK_2;
    let clay = theme::CLAY;

    rsx! {
        if !visible.is_empty() {
            div {
                style: "display:flex;flex-direction:column;gap:4px;margin:-2px 0 6px;",
                for (name, view) in visible {
                    {
                        match view {
                            ActionView::Pending { elapsed_secs } => rsx! {
                                div {
                                    key: "{name}",
                                    style: "display:flex;align-items:center;gap:8px;font-size:11.5px;color:{ink2};",
                                    span { style: "width:6px;height:6px;border-radius:50%;background:{clay};flex:none;", "" }
                                    span { "正在{name} … 已 {elapsed_secs} 秒" }
                                }
                            },
                            ActionView::Ok => rsx! {
                                div {
                                    key: "{name}",
                                    style: "display:flex;align-items:center;gap:8px;font-size:11.5px;color:#5F7355;",
                                    span { "✓ {name}" }
                                }
                            },
                            ActionView::Fail(detail) => {
                                // plan/14 C15(规范条 3): 人话优先,原文不丢 ——
                                // `explain_failure` 的 headline 上屏,`raw`(=
                                // 原始 `detail`,逐字未改)进 `title` 悬浮,鼠标
                                // 停留即见,不隐藏也不占版面。
                                let x = ui::explain_failure(&detail);
                                rsx! {
                                    div {
                                        key: "{name}",
                                        title: "{x.raw}",
                                        style: "display:flex;align-items:center;gap:8px;font-size:11.5px;color:#B0503A;",
                                        span { "✕ {name} · {x.headline}" }
                                    }
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────── 0 · 仓从哪来 ─────────────────────────

#[component]
fn RepoCard(
    platform: Signal<String>,
    choice: Signal<RepoChoice>,
    github_repos: Vec<GithubRepoSummary>,
    codehub_repos: Vec<CodehubRepoSummary>,
    codehub_host: Signal<String>,
    codehub_path: Signal<String>,
    codehub_namespace: Signal<String>,
    codehub_name: Signal<String>,
    codehub_visibility: Signal<String>,
    github_slug: Signal<String>,
    remote_project_probe: RemoteProjectProbe,
    on_next: EventHandler<()>,
) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let label = theme::label();
    let input = theme::input();
    let is_new = matches!(choice(), RepoChoice::New { .. });
    let is_codehub = platform() == "codehub";
    let selected_gh = match &choice() {
        RepoChoice::Existing { owner, repo } => format!("{owner}/{repo}"),
        RepoChoice::New { .. } => String::new(),
    };
    let combo_ch: Vec<RepoComboItem> = codehub_repos
        .iter()
        .map(|r| {
            let branch = if r.default_branch.trim().is_empty() {
                "?"
            } else {
                r.default_branch.as_str()
            };
            RepoComboItem {
                value: r.path.clone(),
                title: r.path.clone(),
                meta: format!("{} · {branch}", r.visibility),
                haystack: format!("{} {} {}", r.path, r.description, r.default_branch)
                    .to_lowercase(),
            }
        })
        .collect();
    let combo_gh: Vec<RepoComboItem> = github_repos
        .iter()
        .map(|r| {
            let value = format!("{}/{}", r.owner, r.repo);
            let vis = if r.private { "private" } else { "public" };
            let branch = if r.default_branch.trim().is_empty() {
                "?"
            } else {
                r.default_branch.as_str()
            };
            RepoComboItem {
                value: value.clone(),
                title: value,
                meta: format!("{vis} · {branch}"),
                haystack: format!(
                    "{}/{} {} {}",
                    r.owner, r.repo, r.description, r.default_branch
                )
                .to_lowercase(),
            }
        })
        .collect();

    // can_send gate: github = same as before (new always ok, existing needs
    // owner); codehub new = name+namespace non-empty; codehub existing =
    // path selected.
    let can_send = if is_codehub {
        if is_new {
            !codehub_name().trim().is_empty() && !codehub_namespace().trim().is_empty()
        } else {
            !codehub_path().trim().is_empty()
        }
    } else {
        let existing_ready =
            matches!(&choice(), RepoChoice::Existing { owner, .. } if !owner.is_empty());
        is_new || existing_ready
    };
    let opacity = if can_send { "1" } else { ".45" };
    let hint: &str = if can_send {
        ""
    } else if is_codehub {
        if is_new {
            if codehub_name().trim().is_empty() {
                "codehub 仓名未填"
            } else {
                "namespace 未填(个人或 group)"
            }
        } else {
            "选一个 codehub 仓(点刷新列表加载)"
        }
    } else {
        "选一个已有仓,或点「接入已有仓」从列表选"
    };

    // C16: github 接入已有仓时,在下拉下方回显完整真实 metadata。
    let selected_gh_meta = if let RepoChoice::Existing { owner, repo } = &choice() {
        if is_codehub {
            None
        } else {
            github_repos
                .iter()
                .find(|r| &r.owner == owner && &r.repo == repo)
                .cloned()
        }
    } else {
        None
    };
    // codehub 接入已有仓时,同样回显 metadata(对仗 github)。
    let selected_ch_meta = if is_codehub && !is_new {
        codehub_repos
            .iter()
            .find(|r| r.path == codehub_path())
            .cloned()
    } else {
        None
    };

    let probe_hint: Option<&str> = match &remote_project_probe {
        RemoteProjectProbe::Probing => Some("正在识别是否已是 Buddy 项目…"),
        RemoteProjectProbe::Present(_) => {
            Some("已识别：仓里有 .bw/project.toml（后来者 · 下一步只读预填）")
        }
        RemoteProjectProbe::Absent => Some("仓里尚无 .bw/project.toml（首到者 · 下一步手填意图）"),
        RemoteProjectProbe::Failed(_) => {
            Some("正本探测失败，下一步仍可手填（确认后以 clone 为准）")
        }
        RemoteProjectProbe::Idle => None,
    };

    rsx! {
        div { style: "font-family:{serif};font-size:22px;font-weight:600;margin:14px 0 4px;", "仓从哪来？" }
        p { style: "font-size:12.5px;color:{ink3};margin:0 0 14px;line-height:1.7;", "每个项目背后是一个真实的代码仓 —— 新建一个,或者接入你已有的。" }

        {platform_selector(platform)}

        if is_codehub {
            // codehub host 选择器:绿区 green / 内源 open(默认) / 黄区 yellow
            {codehub_host_selector(codehub_host, codehub_path)}

            // 起点 chip:两边都有(新建/接入)
            {chip_question(
                "起点",
                vec![("新建仓", is_new), ("接入已有仓", !is_new)],
                {
                    let k = k.clone();
                    move |i| {
                        if i == 0 {
                            choice.set(RepoChoice::New { private: true });
                        } else {
                            choice.set(RepoChoice::Existing {
                                owner: String::new(),
                                repo: String::new(),
                            });
                        }
                        k.send(Command::ClearRemoteProjectProbe);
                    }
                },
            )}

            div {
                style: "{card} padding:18px 20px;margin-top:8px;",
                if is_new {
                    // codehub 新建仓:host(上方选)+ namespace + name + 可见性
                    label { style: "{label}", "namespace(个人或 group)" }
                    input {
                        style: "{input}",
                        placeholder: "z30026659",
                        value: "{codehub_namespace}",
                        oninput: move |e| codehub_namespace.set(e.value()),
                    }
                    p { style: "font-size:11px;color:{ink3};margin:4px 0 12px;line-height:1.6;", "group namespace 选择 V1 不做(留口);个人 namespace 填你的工号或账号。" }
                    label { style: "{label};margin-top:4px;", "仓库名" }
                    input {
                        style: "{input};margin-top:6px;",
                        placeholder: "my-service",
                        value: "{codehub_name}",
                        oninput: move |e| codehub_name.set(e.value()),
                    }
                    {
                        let vis_private = codehub_visibility() == "private";
                        rsx! {
                            {chip_question(
                                "可见性",
                                vec![("Private", vis_private), ("Public", !vis_private)],
                                move |i| codehub_visibility.set(if i == 0 { "private".to_string() } else { "public".to_string() }),
                            )}
                        }
                    }
                } else {
                    // codehub 接入已有:host(上方选)+ 下拉(codehub_repos)
                    div {
                        style: "display:flex;align-items:center;justify-content:space-between;gap:8px;",
                        label { style: "{label}", "选一个仓" }
                        button {
                            style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:6px;padding:3px 10px;font-size:11px;",
                            onclick: {
                                let k = k.clone();
                                move |_| k.send(Command::ListCodehubRepos { host: codehub_host() })
                            },
                            "↻ 刷新列表"
                        }
                    }
                    RepoCombobox {
                        selected: codehub_path(),
                        placeholder: "搜索 path / 描述 / 默认分支，点一条选中".to_string(),
                        loaded: codehub_repos.len(),
                        empty_hint: format!("没读到仓库列表 —— 点「↻ 刷新列表」加载(需本机 codehub-cli 已登录:codehub-cli -H {} auth status)。", codehub_host()),
                        items: combo_ch,
                        on_select: {
                            let k = k.clone();
                            let codehub_repos = codehub_repos.clone();
                            move |path: String| {
                                codehub_path.set(path.clone());
                                if path.trim().is_empty() {
                                    k.send(Command::ClearRemoteProjectProbe);
                                } else {
                                    let branch = codehub_repos
                                        .iter()
                                        .find(|r| r.path == path)
                                        .map(|r| r.default_branch.clone())
                                        .unwrap_or_default();
                                    k.send(Command::ProbeRemoteProjectToml {
                                        provider: "codehub".into(),
                                        host: codehub_host(),
                                        path,
                                        default_branch: branch,
                                    });
                                }
                            }
                        },
                    }
                    if let Some(meta) = selected_ch_meta {
                        {codehub_repo_metadata_block(&meta)}
                    }
                }
            }
        } else {
            // github:新建仓 RepoCard 填仓名+可见性 / 接入 gh repo list 下拉
            {chip_question(
                "起点",
                vec![("新建仓", is_new), ("接入已有仓", !is_new)],
                {
                    let k = k.clone();
                    move |i| {
                        if i == 0 {
                            choice.set(RepoChoice::New { private: true });
                        } else {
                            choice.set(RepoChoice::Existing {
                                owner: String::new(),
                                repo: String::new(),
                            });
                        }
                        k.send(Command::ClearRemoteProjectProbe);
                    }
                },
            )}

            div {
                style: "{card} padding:18px 20px;margin-top:8px;",
                if is_new {
                    // PF1-1: github 新建仓也在 RepoCard 填仓名(对仗 codehub),
                    // 不再延后到 IntentCard。空时提交闭包 slugify(&name()) 兜底。
                    label { style: "{label}", "仓库名" }
                    input {
                        style: "{input};margin-top:6px;",
                        placeholder: "growth-kanban(空则用项目名)",
                        value: "{github_slug}",
                        oninput: move |e| github_slug.set(e.value()),
                    }
                    p { style: "font-size:11px;color:{ink3};margin:4px 0 12px;line-height:1.6;", "GitHub 仓名 = 仓库名;留空系统用项目名 slugify 兜底。" }
                    {
                        let private = matches!(choice(), RepoChoice::New { private: true });
                        rsx! {
                            {chip_question(
                                "可见性",
                                vec![("Private", private), ("Public", !private)],
                                move |i| choice.set(RepoChoice::New { private: i == 0 }),
                            )}
                        }
                    }
                } else {
                    div {
                        style: "display:flex;align-items:center;justify-content:space-between;gap:8px;",
                        label { style: "{label}", "选一个仓" }
                        button {
                            style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:6px;padding:3px 10px;font-size:11px;",
                            onclick: {
                                let k = k.clone();
                                move |_| k.send(Command::ListGithubRepos)
                            },
                            "↻ 刷新列表"
                        }
                    }
                    RepoCombobox {
                        selected: selected_gh.clone(),
                        placeholder: "搜索 owner/repo / 描述 / 默认分支，点一条选中".to_string(),
                        loaded: github_repos.len(),
                        empty_hint: "没读到仓库列表 —— 点「↻ 刷新列表」加载(需本机 gh 已登录:gh auth status)。".to_string(),
                        items: combo_gh,
                        on_select: {
                            let k = k.clone();
                            let github_repos = github_repos.clone();
                            move |value: String| {
                                if let Some((owner, repo)) = value.split_once('/') {
                                    let owner = owner.to_string();
                                    let repo = repo.to_string();
                                    choice.set(RepoChoice::Existing {
                                        owner: owner.clone(),
                                        repo: repo.clone(),
                                    });
                                    let branch = github_repos
                                        .iter()
                                        .find(|r| r.owner == owner && r.repo == repo)
                                        .map(|r| r.default_branch.clone())
                                        .unwrap_or_default();
                                    k.send(Command::ProbeRemoteProjectToml {
                                        provider: "github".into(),
                                        host: String::new(),
                                        path: format!("{owner}/{repo}"),
                                        default_branch: branch,
                                    });
                                } else {
                                    k.send(Command::ClearRemoteProjectProbe);
                                }
                            }
                        },
                    }
                    if let Some(meta) = selected_gh_meta {
                        {repo_metadata_block(&meta)}
                    }
                }
            }
        }

        div {
            style: "display:flex;justify-content:flex-end;align-items:center;gap:10px;margin-top:14px;",
            if let Some(h) = probe_hint {
                span { style: "font-size:11.5px;color:{ink3};margin-right:auto;max-width:340px;line-height:1.5;", "{h}" }
            }
            if !can_send {
                span { style: "font-size:11.5px;color:#B0503A;", "{hint}" }
            }
            button {
                style: "{theme::btn_primary()} opacity:{opacity};",
                disabled: !can_send,
                onclick: {
                    let k = k.clone();
                    let github_repos = github_repos.clone();
                    let codehub_repos = codehub_repos.clone();
                    move |_| {
                        // Re-fire probe on next if still Idle for an existing
                        // pick (e.g. list refreshed after selection).
                        if !is_new {
                            if is_codehub {
                                let path = codehub_path();
                                if !path.trim().is_empty() {
                                    let branch = codehub_repos
                                        .iter()
                                        .find(|r| r.path == path)
                                        .map(|r| r.default_branch.clone())
                                        .unwrap_or_default();
                                    k.send(Command::ProbeRemoteProjectToml {
                                        provider: "codehub".into(),
                                        host: codehub_host(),
                                        path,
                                        default_branch: branch,
                                    });
                                }
                            } else if let RepoChoice::Existing { owner, repo } = choice() {
                                if !owner.is_empty() {
                                    let branch = github_repos
                                        .iter()
                                        .find(|r| r.owner == owner && r.repo == repo)
                                        .map(|r| r.default_branch.clone())
                                        .unwrap_or_default();
                                    k.send(Command::ProbeRemoteProjectToml {
                                        provider: "github".into(),
                                        host: String::new(),
                                        path: format!("{owner}/{repo}"),
                                        default_branch: branch,
                                    });
                                }
                            }
                        } else {
                            k.send(Command::ClearRemoteProjectProbe);
                        }
                        on_next.call(());
                    }
                },
                "下一步 →"
            }
        }
    }
}

/// C16(plan/14 规范条 4): 仓平台选择器 —— GitHub / CodeHub 两个可点选项
/// (github 默认选中)。V1 Issue 1:去 GitLab/Gitcode 占位(如实,不假装
/// 「未接」;真接时再加)。纯 UI 状态(`platform` 信号),与「起点」chip
/// 并存、互不影响。
fn platform_selector(mut platform: Signal<String>) -> Element {
    let ink2 = theme::INK_2;
    let gh = platform() == "github";
    let ch = platform() == "codehub";
    let chip = |sel: bool| -> (&'static str, &'static str, &'static str) {
        if sel {
            ("1.5px solid #C5654A", "#C5654A", "#fff")
        } else {
            ("1px solid #DDD5C5", "transparent", "#57534A")
        }
    };
    let (gbd, gbg, gfg) = chip(gh);
    let (cbd, cbg, cfg) = chip(ch);
    rsx! {
        div {
            style: "margin-bottom:6px;",
            div { style: "font-size:12.5px;font-weight:600;color:{ink2};margin-bottom:8px;", "仓平台" }
            div {
                style: "display:flex;gap:6px;flex-wrap:wrap;",
                div {
                    onclick: move |_| platform.set("github".to_string()),
                    style: "cursor:pointer;border:{gbd};background:{gbg};color:{gfg};border-radius:15px;padding:6px 13px;font-size:12px;font-weight:500;",
                    "GitHub"
                }
                div {
                    onclick: move |_| platform.set("codehub".to_string()),
                    style: "cursor:pointer;border:{cbd};background:{cbg};color:{cfg};border-radius:15px;padding:6px 13px;font-size:12px;font-weight:500;",
                    "CodeHub"
                }
            }
        }
    }
}

/// V1 Issue 1:codehub host 选择器 —— 三域名 alias(green/open/yellow),
/// 引擎用 `-H <alias>` shell-out codehub-cli。green/open 默认可直用;
/// yellow 需先 `codehub-cli -H yellow auth login`(tooltip 如实标)。
/// 切换 host 时清空 path(旧 host 的仓列表不再适用)。
fn codehub_host_selector(mut host: Signal<String>, mut path: Signal<String>) -> Element {
    let k = use_context::<Kernel>();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let chip = |sel: bool| -> (&'static str, &'static str, &'static str) {
        if sel {
            ("1.5px solid #C5654A", "#C5654A", "#fff")
        } else {
            ("1px solid #DDD5C5", "transparent", "#57534A")
        }
    };
    let green = host() == "green";
    let open = host() == "open";
    let yellow = host() == "yellow";
    let (grbd, grbg, grfg) = chip(green);
    let (obd, obg, ofg) = chip(open);
    let (ybd, ybg, yfg) = chip(yellow);
    rsx! {
        div {
            style: "margin-bottom:6px;",
            div { style: "font-size:12.5px;font-weight:600;color:{ink2};margin-bottom:8px;", "CodeHub 域" }
            div {
                style: "display:flex;gap:6px;flex-wrap:wrap;",
                div {
                    onclick: {
                        let k = k.clone();
                        move |_| {
                            host.set("green".to_string());
                            path.set(String::new());
                            k.send(Command::ClearRemoteProjectProbe);
                        }
                    },
                    style: "cursor:pointer;border:{grbd};background:{grbg};color:{grfg};border-radius:15px;padding:6px 13px;font-size:12px;font-weight:500;",
                    "绿区 green"
                }
                div {
                    onclick: {
                        let k = k.clone();
                        move |_| {
                            host.set("open".to_string());
                            path.set(String::new());
                            k.send(Command::ClearRemoteProjectProbe);
                        }
                    },
                    style: "cursor:pointer;border:{obd};background:{obg};color:{ofg};border-radius:15px;padding:6px 13px;font-size:12px;font-weight:500;",
                    "内源 open"
                }
                div {
                    title: "需先 `codehub-cli -H yellow auth login`",
                    onclick: {
                        let k = k.clone();
                        move |_| {
                            host.set("yellow".to_string());
                            path.set(String::new());
                            k.send(Command::ClearRemoteProjectProbe);
                        }
                    },
                    style: "cursor:pointer;border:{ybd};background:{ybg};color:{yfg};border-radius:15px;padding:6px 13px;font-size:12px;font-weight:500;",
                    "黄区 yellow"
                }
            }
            p { style: "font-size:11px;color:{ink3};margin-top:6px;", "green/open 已登录可直用;yellow 需先在本机 `codehub-cli -H yellow auth login`。" }
        }
    }
}

/// C16(plan/14 规范条 4): 接入已有仓时的完整真实 metadata 回显 —— 描述/
/// 可见性/默认分支/最近推送,全部来自 `gh repo list` 的真实字段(见
/// `bw_engine::github::list_repos`),不是"owner/repo · private"一行糊弄。
fn repo_metadata_block(meta: &GithubRepoSummary) -> Element {
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let desc = if meta.description.trim().is_empty() {
        "(无描述)".to_string()
    } else {
        meta.description.clone()
    };
    let vis = if meta.private { "private" } else { "public" };
    let branch = if meta.default_branch.trim().is_empty() {
        "(未知)".to_string()
    } else {
        meta.default_branch.clone()
    };
    let pushed = if meta.pushed_at.trim().is_empty() {
        "(未知)".to_string()
    } else {
        meta.pushed_at.clone()
    };
    rsx! {
        div {
            style: "margin-top:10px;padding:11px 13px;background:#F7F3EA;border:1px solid #E5DDCB;border-radius:8px;",
            div { style: "font-family:{mono};font-size:12px;font-weight:600;color:#3A3833;margin-bottom:6px;", "{meta.owner}/{meta.repo}" }
            div { style: "font-size:11.5px;color:{ink3};line-height:1.9;",
                div { "描述:{desc}" }
                div { "可见性:{vis} · 默认分支:{branch}" }
                div { "最近推送:{pushed}" }
            }
        }
    }
}

/// V1 Issue 1:codehub 接入已有仓时的 metadata 回显 —— 对仗 github 的
/// `repo_metadata_block`,但 `CodehubRepoSummary.visibility` 是字符串
/// (private/public/internal),不是 bool。
fn codehub_repo_metadata_block(meta: &CodehubRepoSummary) -> Element {
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let desc = if meta.description.trim().is_empty() {
        "(无描述)".to_string()
    } else {
        meta.description.clone()
    };
    let branch = if meta.default_branch.trim().is_empty() {
        "(未知)".to_string()
    } else {
        meta.default_branch.clone()
    };
    let pushed = if meta.pushed_at.trim().is_empty() {
        "(未知)".to_string()
    } else {
        meta.pushed_at.clone()
    };
    rsx! {
        div {
            style: "margin-top:10px;padding:11px 13px;background:#F7F3EA;border:1px solid #E5DDCB;border-radius:8px;",
            div { style: "font-family:{mono};font-size:12px;font-weight:600;color:#3A3833;margin-bottom:6px;", "{meta.path}" }
            div { style: "font-size:11.5px;color:{ink3};line-height:1.9;",
                div { "描述:{desc}" }
                div { "可见性:{meta.visibility} · 默认分支:{branch}" }
                div { "最近推送:{pushed}" }
            }
        }
    }
}

// ───────────────────────── 1 · 意图 ─────────────────────────

const KINDS: [&str; 5] = [
    "看板 / 网页应用",
    "对话应用",
    "Design / 无限画布",
    "数据 / API 服务",
    "其他",
];

/// GitHub 仓名要求 ASCII + 连字符;项目显示名允许中文。两个独立字段(用户
/// 已确认),这个纯函数只给"新建仓"分支的兜底用——真正发去 `gh` 的值
/// 是用户在 RepoCard 填的 `github_slug` 信号(空时才用本函数兜底)。
fn slugify(name: &str) -> String {
    let base: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if base.is_empty() {
        "project".to_string()
    } else {
        base
    }
}

#[component]
fn IntentCard(
    // V1 Issue 1: resuming reads vm.name/brief/benchmark/win for initial
    // values; fresh (vm=None) starts all fields blank.
    vm: Option<CreateVm>,
    platform: Signal<String>,
    repo_choice: Signal<RepoChoice>,
    codehub_host: Signal<String>,
    codehub_path: Signal<String>,
    codehub_namespace: Signal<String>,
    codehub_name: Signal<String>,
    codehub_visibility: Signal<String>,
    github_slug: Signal<String>,
    submitting: Signal<bool>,
    remote_project_probe: RemoteProjectProbe,
) -> Element {
    let k = use_context::<Kernel>();

    // Extract initial values before hooks (can't move Option<CreateVm> into
    // multiple use_signal closures). Prefer remote正本 when probe already
    // returned Present (later-comer Intent UX).
    let v = vm.as_ref();
    let (seed_name, seed_kind, seed_brief, seed_benchmark, seed_win) =
        if let RemoteProjectProbe::Present(f) = &remote_project_probe {
            (
                f.name.clone(),
                f.kind.clone(),
                f.brief.clone(),
                f.benchmark.clone(),
                f.opportunity.clone(),
            )
        } else {
            (
                v.map(|v| v.name.clone()).unwrap_or_default(),
                v.map(|v| v.kind.clone())
                    .unwrap_or_else(|| KINDS[0].to_string()),
                v.map(|v| v.brief.clone()).unwrap_or_default(),
                v.map(|v| v.benchmark.clone()).unwrap_or_default(),
                v.map(|v| v.win.clone()).unwrap_or_default(),
            )
        };
    let is_resuming = vm.is_some();

    let mut name = use_signal(move || seed_name);
    let mut kind = use_signal(move || seed_kind);
    let mut brief = use_signal(move || seed_brief);
    let mut benchmark = use_signal(move || seed_benchmark);
    let mut win = use_signal(move || seed_win);

    // If probe finishes after Intent mounts (user clicked 下一步 while
    // Probing), apply Present fields once.
    use_effect({
        let probe = remote_project_probe.clone();
        move || {
            if let RemoteProjectProbe::Present(f) = &probe {
                name.set(f.name.clone());
                kind.set(f.kind.clone());
                brief.set(f.brief.clone());
                benchmark.set(f.benchmark.clone());
                win.set(f.opportunity.clone());
            }
        }
    });

    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let input = theme::input();
    let label = theme::label();
    let is_codehub = platform() == "codehub";
    let is_new_repo = matches!(repo_choice(), RepoChoice::New { .. });
    let later_readonly = matches!(&remote_project_probe, RemoteProjectProbe::Present(_));
    let probing = matches!(&remote_project_probe, RemoteProjectProbe::Probing);
    let probe_failed = if let RemoteProjectProbe::Failed(e) = &remote_project_probe {
        Some(e.clone())
    } else {
        None
    };
    let field_style = if later_readonly {
        format!("{input} background:#F5F1E8;color:#57534A;")
    } else {
        input.clone()
    };

    // can_send: name 非空(+ codehub 必要字段)。brief 改不强制。
    // Probing: block confirm until we know later vs first (avoid blank submit).
    let codehub_ok = if is_codehub {
        if is_new_repo {
            !codehub_name().trim().is_empty() && !codehub_namespace().trim().is_empty()
        } else {
            !codehub_path().trim().is_empty()
        }
    } else {
        true
    };
    let can_send = !probing && !name().trim().is_empty() && codehub_ok;

    // Bug B: pending guard — 防连点(后台建项目非幂等)。点即置 true;
    // 离开创建屏 / 「+ 新建」/ UiNote::Error 由 main.rs 复位。
    let pending = submitting();
    let (btn_label, btn_bg, btn_shadow, btn_cursor) = if pending {
        ("建立中…", "#B89A8E", "none", "not-allowed")
    } else if probing {
        ("识别中…", "#B89A8E", "none", "not-allowed")
    } else {
        (
            "确认 · 建立项目",
            "#C5654A",
            "0 3px 10px rgba(197,101,74,.25)",
            "pointer",
        )
    };
    let opacity = if can_send { "1" } else { ".45" };

    let send = move |_| {
        if !can_send || submitting() {
            return;
        }
        submitting.set(true);

        // Fresh: mint project row + repo. Resume: project already exists,
        // skip CreateProject (would create a duplicate row with a new ID).
        if !is_resuming {
            let (github, codehub) = if is_codehub {
                let origin = if is_new_repo {
                    CodehubOrigin::New {
                        host: codehub_host().trim().to_string(),
                        namespace: codehub_namespace().trim().to_string(),
                        name: codehub_name().trim().to_string(),
                        visibility: codehub_visibility().trim().to_string(),
                    }
                } else {
                    CodehubOrigin::Existing {
                        host: codehub_host().trim().to_string(),
                        path: codehub_path().trim().to_string(),
                    }
                };
                (None, Some(origin))
            } else {
                (
                    match repo_choice() {
                        RepoChoice::New { private } => Some(GithubOrigin::New {
                            slug: if github_slug().trim().is_empty() {
                                slugify(&name())
                            } else {
                                github_slug().trim().to_string()
                            },
                            private,
                        }),
                        RepoChoice::Existing { owner, repo } => {
                            Some(GithubOrigin::Existing { owner, repo })
                        }
                    },
                    None,
                )
            };
            k.send(Command::CreateProject {
                provider: platform(),
                id: ProjectId::new(),
                name: name().trim().to_string(),
                kind: kind(),
                desc: brief().trim().to_string(),
                workspace: None,
                github,
                codehub,
            });
        }

        // Both fresh and resume: 对标+成功标准落库 + stage+三件套+push+probe
        k.send(Command::UpdateBrief {
            benchmark: benchmark().trim().to_string(),
            opportunity: win().trim().to_string(),
        });
        k.send(Command::CompleteCreation {
            cadence: Cadence::Weekly,
            run_first: false,
        });
        k.send(Command::SetPanel(Panel::Progress));
        k.send(Command::SetScope(Scope::All));
    };

    rsx! {
        div { style: "font-family:{serif};font-size:22px;font-weight:600;margin:14px 0 4px;", "你想做什么？" }
        p { style: "font-size:12.5px;color:{ink3};margin:0 0 14px;line-height:1.7;",
            if later_readonly {
                "已是 Buddy 项目：意图来自仓里 .bw/project.toml 正本（只读）。确认后读回本地，不重开三件套、不重推正本。"
            } else if probing {
                "正在识别仓里是否已有 .bw/project.toml…"
            } else {
                "一个名字、一句你想做的事。对标和成功标准不强制,留空系统照常兜底。"
            }
        }
        if later_readonly {
            div {
                style: "display:inline-block;font-family:{theme::MONO};font-size:11px;font-weight:600;letter-spacing:.04em;color:#5F7355;background:#E8F0E4;border:1px solid #C5D6BE;border-radius:6px;padding:4px 10px;margin:0 0 12px;",
                "已是 Buddy 项目 · 正本只读"
            }
        }
        if let Some(err) = probe_failed.as_ref() {
            p { style: "font-size:11.5px;color:#B5862F;margin:0 0 12px;line-height:1.6;",
                "正本探测失败（{err}）。可先手填；确认建立后仍以 clone 后的仓文件为准。"
            }
        }
        div {
            style: "{card} padding:18px 20px;",
            div {
                style: "display:grid;grid-template-columns:1.4fr 1fr;gap:12px;margin-bottom:12px;",
                div {
                    label { style: "{label}", "项目名称 *" }
                    input {
                        style: "{field_style}",
                        placeholder: "例:增长实验看板",
                        value: "{name}",
                        readonly: later_readonly,
                        oninput: move |e| {
                            if !later_readonly {
                                name.set(e.value());
                            }
                        },
                    }
                }
                div {
                    label { style: "{label}", "项目类型" }
                    select {
                        style: "{field_style}",
                        value: "{kind}",
                        disabled: later_readonly,
                        onchange: move |e| {
                            if !later_readonly {
                                kind.set(e.value());
                            }
                        },
                        for kd in KINDS {
                            option { value: "{kd}", "{kd}" }
                        }
                    }
                }
            }
            label { style: "{label}", "你想做什么" }
            textarea {
                style: "{field_style} min-height:70px;",
                placeholder: "一句话即可,不强制。例:把 agent 会话里长出的工作流沉淀成可复用资产,导入即跑。",
                value: "{brief}",
                readonly: later_readonly,
                oninput: move |e| {
                    if !later_readonly {
                        brief.set(e.value());
                    }
                },
            }
            div {
                style: "display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-top:12px;",
                div {
                    label { style: "{label}", "最像的对标(不强制)" }
                    textarea {
                        style: "{field_style} min-height:52px;",
                        placeholder: "例:\nLinear\nHeight",
                        value: "{benchmark}",
                        readonly: later_readonly,
                        oninput: move |e| {
                            if !later_readonly {
                                benchmark.set(e.value());
                            }
                        },
                    }
                }
                div {
                    label { style: "{label}", "三个月后怎样算成了(不强制)" }
                    textarea {
                        style: "{field_style} min-height:52px;",
                        placeholder: "例:被持续复用、效率可量化提升……",
                        value: "{win}",
                        readonly: later_readonly,
                        oninput: move |e| {
                            if !later_readonly {
                                win.set(e.value());
                            }
                        },
                    }
                }
            }
            // PF1-1: GitHub 仓名已在上一步 RepoCard 填(github_slug);接入已有
            // 仓时回显将接入的 owner/repo。codehub 的仓名也在 RepoCard 填。
            if !is_new_repo && !is_codehub {
                if let RepoChoice::Existing { owner, repo } = repo_choice() {
                    p { style: "font-size:11.5px;color:{ink3};margin-top:10px;", "将接入 {owner}/{repo} ↗" }
                }
            }
            div {
                style: "display:flex;justify-content:flex-end;margin-top:14px;",
                button {
                    disabled: pending || probing,
                    style: "cursor:{btn_cursor};background:{btn_bg};color:#fff;border:none;border-radius:8px;padding:10px 22px;font:600 13px/1 inherit;box-shadow:{btn_shadow};opacity:{opacity};",
                    onclick: send,
                    "{btn_label}"
                }
            }
        }
        p { style: "font-size:11.5px;color:{ink3};margin:10px 2px 0;",
            if later_readonly {
                "确认后 clone + 读回正本；不建三件套、不推 project.toml。进度见上方条。"
            } else {
                "提交后系统自动建仓 + connector + cron + 标配三件套;进度见上方条,完成自动进项目墙。"
            }
        }
    }
}

// ───────────────────────── helpers ─────────────────────────

/// How many matching repos the combobox panel paints. Fetch is
/// [`ONBOARD_REPO_LIST_LIMIT`]; search filters that loaded set.
const ONBOARD_REPO_DROPDOWN_CAP: usize = 30;

#[derive(Clone, PartialEq)]
struct RepoComboItem {
    value: String,
    title: String,
    meta: String,
    haystack: String,
}

/// One field: type to filter the already-loaded list, click a row to pick.
/// Native `<select>` cannot do this; a separate search box + select is not
/// the conventional combobox the create flow needs.
#[component]
fn RepoCombobox(
    selected: String,
    placeholder: String,
    loaded: usize,
    empty_hint: String,
    items: Vec<RepoComboItem>,
    on_select: EventHandler<String>,
) -> Element {
    let mut query = use_signal(String::new);
    let mut open = use_signal(|| false);
    let mut highlight = use_signal(|| 0usize);
    let ink3 = theme::INK_3;
    let input = theme::input();
    let q = query().trim().to_lowercase();
    let shown: Vec<RepoComboItem> = items
        .iter()
        .filter(|r| q.is_empty() || r.haystack.contains(&q))
        .take(ONBOARD_REPO_DROPDOWN_CAP)
        .cloned()
        .collect();
    let hi = if shown.is_empty() {
        0
    } else {
        highlight().min(shown.len() - 1)
    };
    let display = if open() {
        query()
    } else if !selected.is_empty() {
        selected.clone()
    } else {
        String::new()
    };
    let field_placeholder = if selected.is_empty() {
        placeholder.clone()
    } else {
        selected.clone()
    };

    let pick = {
        let mut query = query;
        let mut open = open;
        move |value: String| {
            query.set(String::new());
            open.set(false);
            on_select.call(value);
        }
    };

    rsx! {
        div { style: "position:relative;margin-top:8px;",
            input {
                style: "{input}",
                placeholder: "{field_placeholder}",
                value: "{display}",
                onfocus: move |_| open.set(true),
                oninput: move |e| {
                    query.set(e.value());
                    open.set(true);
                    highlight.set(0);
                },
                onkeydown: {
                    let shown = shown.clone();
                    let mut pick = pick.clone();
                    move |e: KeyboardEvent| {
                        let key = e.key();
                        match key {
                            Key::Escape => {
                                query.set(String::new());
                                open.set(false);
                            }
                            Key::ArrowDown => {
                                e.prevent_default();
                                if shown.is_empty() {
                                    return;
                                }
                                let next = (highlight() + 1).min(shown.len() - 1);
                                highlight.set(next);
                            }
                            Key::ArrowUp => {
                                e.prevent_default();
                                if highlight() > 0 {
                                    highlight.set(highlight() - 1);
                                }
                            }
                            Key::Enter => {
                                e.prevent_default();
                                if let Some(item) = shown.get(highlight()) {
                                    pick(item.value.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                },
                onblur: move |_| {
                    open.set(false);
                    query.set(String::new());
                },
            }
            if open() && !items.is_empty() {
                div {
                    style: "position:absolute;left:0;right:0;top:100%;z-index:20;margin-top:4px;max-height:240px;overflow:auto;background:#FFFDF8;border:1px solid {theme::BORDER_DEEP};border-radius:8px;box-shadow:{theme::SHADOW};",
                    if shown.is_empty() {
                        div { style: "padding:10px 12px;font-size:12px;color:{ink3};",
                            "已加载 {loaded} 个，没有匹配「{query}」。换关键词；仍没有 = 不是 member，或排在 {ONBOARD_REPO_LIST_LIMIT} 以外。"
                        }
                    } else {
                        for (i, item) in shown.iter().enumerate() {
                            {
                                let value = item.value.clone();
                                let title = item.title.clone();
                                let meta = item.meta.clone();
                                let active = i == hi;
                                let is_sel = value == selected;
                                let bg = if active {
                                    "#F4E4DC"
                                } else if is_sel {
                                    "#F7F1E8"
                                } else {
                                    "transparent"
                                };
                                let mut pick = pick.clone();
                                rsx! {
                                    div {
                                        key: "{value}",
                                        style: "padding:8px 12px;cursor:pointer;background:{bg};",
                                        onmousedown: move |e| {
                                            e.prevent_default();
                                            pick(value.clone());
                                        },
                                        onmouseenter: move |_| highlight.set(i),
                                        div { style: "font-size:13px;line-height:1.4;", "{title}" }
                                        div { style: "font-size:11px;color:{ink3};margin-top:2px;", "{meta}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if items.is_empty() {
            p { style: "font-size:11.5px;color:{ink3};margin-top:8px;", "{empty_hint}" }
        } else {
            p { style: "font-size:11px;color:{ink3};margin-top:8px;",
                "已加载 {loaded} 个（最多 {ONBOARD_REPO_LIST_LIMIT}）。框里搜索，下拉最多 {ONBOARD_REPO_DROPDOWN_CAP} 条。仍没有 = 不是 member，或排在 {ONBOARD_REPO_LIST_LIMIT} 以外。"
            }
        }
    }
}

/// A row of selectable chips for one question. `options` = (label, selected).
fn chip_question(
    title: &'static str,
    options: Vec<(&'static str, bool)>,
    on_pick: impl FnMut(usize) + Clone + 'static,
) -> Element {
    let ink2 = theme::INK_2;
    rsx! {
        div {
            style: "margin-bottom:6px;",
            div { style: "font-size:12.5px;font-weight:600;color:{ink2};margin-bottom:8px;", "{title}" }
            div {
                style: "display:flex;gap:6px;flex-wrap:wrap;",
                for (i, (opt_label, sel)) in options.into_iter().enumerate() {
                    {
                        let (bd, bg, fg) = if sel {
                            ("1.5px solid #C5654A", "#C5654A", "#fff")
                        } else {
                            ("1px solid #DDD5C5", "transparent", "#57534A")
                        };
                        let mut on_pick = on_pick.clone();
                        rsx! {
                            div {
                                key: "{i}",
                                onclick: move |_| on_pick(i),
                                style: "cursor:pointer;border:{bd};background:{bg};color:{fg};border-radius:15px;padding:6px 13px;font-size:12px;font-weight:500;",
                                "{opt_label}"
                            }
                        }
                    }
                }
            }
        }
    }
}
