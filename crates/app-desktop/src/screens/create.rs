//! `view=create` — the creation card-flow (体系重构 v2 · replaces the old
//! 8-step form wizard): 意图 → 快速问题 → 起草中 → 审阅确认.
//!
//! Nothing here fabricates project-specific content. The "起草" step is a
//! real (mock) workflow run through the same `Engine`/`Executor` op.rs uses —
//! its output is a clearly-mock transcript, never injected into the editable
//! north-star/metric fields as fact. Those fields start from the user's own
//! words (the brief) or blank, always editable, only becoming real project
//! state when the user hits 确认.
//!
//! The project row is minted at the *first* card (意图), not deferred to
//! confirm: that gives the drafting run somewhere real to attach a session,
//! and means an interrupted creation resumes instead of vanishing.

use crate::kernel::{CreateVm, Kernel, RunVm};
use crate::theme;
use bw_app::{Command, GithubOrigin, Panel, Scope};
use bw_core::model::{drafting_workflow, Cadence, ProjectCycle, StageKind};
use bw_core::{MetricId, ProjectId, SessionId};
use bw_engine::GithubRepoSummary;
use bw_store::{MetricRole, SessionKind};
use dioxus::prelude::*;
use ui::vm::MetricVm;

/// Which card of the flow is showing. Local UI navigation only — the real
/// draft lives in [`CreateVm`], sourced from the store.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Card {
    Repo,
    Intent,
    Questions,
    Drafting,
    Review,
}

/// The Repo 卡片's local choice — turned into a `GithubOrigin` only at
/// `IntentCard`'s submit time, once a project name exists to slugify.
#[derive(Clone, Debug, PartialEq)]
enum RepoChoice {
    New { private: bool },
    Existing { owner: String, repo: String },
}

#[component]
pub fn Create(
    vm: Option<CreateVm>,
    run: RunVm,
    github_repos: Vec<GithubRepoSummary>,
    on_cancel: EventHandler<()>,
) -> Element {
    let has_project = vm.is_some();
    // Resuming an interrupted creation (OpenProject on a cold-start project)
    // skips straight past Repo/Intent — the project row (and its repo, if
    // any) already exists.
    let mut card = use_signal(move || {
        if has_project {
            Card::Questions
        } else {
            Card::Repo
        }
    });
    let cadence = use_signal(|| Cadence::Weekly);
    let repo_choice = use_signal(|| RepoChoice::New { private: true });

    let serif = theme::SERIF;
    let ink2 = theme::INK_2;

    rsx! {
        div {
            style: "max-width:640px;margin:0 auto;padding:36px 24px 120px;display:flex;flex-direction:column;gap:12px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;margin-bottom:8px;",
                span { style: "font-family:{serif};font-size:17px;font-weight:600;", "新建项目" }
                if card() == Card::Repo || card() == Card::Intent {
                    button {
                        style: "background:transparent;border:none;color:{ink2};cursor:pointer;font-size:13px;",
                        onclick: move |_| on_cancel.call(()),
                        "← 返回项目墙"
                    }
                }
            }
            match (card(), vm) {
                (Card::Repo, _) => rsx! {
                    RepoCard { choice: repo_choice, github_repos: github_repos.clone(), on_next: move |_| card.set(Card::Intent) }
                },
                (Card::Intent, _) => rsx! {
                    IntentCard { repo_choice, on_created: move |_| card.set(Card::Questions) }
                },
                (_, None) => rsx! { div { "…" } },
                (Card::Questions, Some(v)) => rsx! {
                    QuestionsCard { vm: v, cadence, on_next: move |_| card.set(Card::Drafting) }
                },
                (Card::Drafting, Some(_)) => rsx! {
                    DraftingCard { run, on_next: move |_| card.set(Card::Review) }
                },
                (Card::Review, Some(v)) => rsx! { ReviewCard { vm: v, cadence } },
            }
        }
    }
}

// ───────────────────────── 0 · 仓从哪来 ─────────────────────────

#[component]
fn RepoCard(
    choice: Signal<RepoChoice>,
    github_repos: Vec<GithubRepoSummary>,
    on_next: EventHandler<()>,
) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let is_new = matches!(choice(), RepoChoice::New { .. });
    let existing_ready =
        matches!(&choice(), RepoChoice::Existing { owner, .. } if !owner.is_empty());
    let can_send = is_new || existing_ready;
    let opacity = if can_send { "1" } else { ".45" };

    rsx! {
        div { style: "font-family:{serif};font-size:22px;font-weight:600;margin:14px 0 4px;", "仓从哪来？" }
        p { style: "font-size:12.5px;color:{ink3};margin:0 0 14px;line-height:1.7;", "每个项目背后是一个真实的 GitHub 仓 —— 新建一个,或者接入你已有的。" }

        {chip_question(
            "起点",
            vec![("新建仓", is_new), ("接入已有仓", !is_new)],
            move |i| {
                if i == 0 {
                    choice.set(RepoChoice::New { private: true });
                } else {
                    k.send(Command::ListGithubRepos);
                    choice.set(RepoChoice::Existing { owner: String::new(), repo: String::new() });
                }
            },
        )}

        div {
            style: "{card} padding:18px 20px;margin-top:8px;",
            if is_new {
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
                label { style: "{theme::label()}", "选一个仓" }
                select {
                    style: "{theme::input()} margin-top:6px;",
                    value: {
                        if let RepoChoice::Existing { owner, repo } = &choice() {
                            format!("{owner}/{repo}")
                        } else {
                            String::new()
                        }
                    },
                    onchange: move |e| {
                        if let Some((owner, repo)) = e.value().split_once('/') {
                            choice.set(RepoChoice::Existing {
                                owner: owner.to_string(),
                                repo: repo.to_string(),
                            });
                        }
                    },
                    option { value: "", "请选择…" }
                    for r in github_repos.iter() {
                        {
                            let value = format!("{}/{}", r.owner, r.repo);
                            let vis = if r.private { "private" } else { "public" };
                            rsx! {
                                option { key: "{value}", value: "{value}", "{value} · {vis}" }
                            }
                        }
                    }
                }
                if github_repos.is_empty() {
                    p { style: "font-size:11.5px;color:{ink3};margin-top:8px;", "没读到仓库列表 —— 确认本机 gh 已登录(gh auth status)。" }
                }
            }
        }
        div {
            style: "display:flex;justify-content:flex-end;margin-top:14px;",
            button {
                style: "{theme::btn_primary()} opacity:{opacity};",
                disabled: !can_send,
                onclick: move |_| on_next.call(()),
                "下一步 →"
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
/// 已确认),这个纯函数只给"新建仓"分支的实时预览用——真正发去 `gh` 的值
/// 是用户可能手改过的 `slug` 信号,不是每次都重新静默转写。
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
fn IntentCard(repo_choice: Signal<RepoChoice>, on_created: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| KINDS[0].to_string());
    let mut brief = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut slug_touched = use_signal(|| false);

    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let input = theme::input();
    let label = theme::label();
    let can_send = !name().trim().is_empty() && !brief().trim().is_empty();
    let opacity = if can_send { "1" } else { ".45" };
    let is_new_repo = matches!(repo_choice(), RepoChoice::New { .. });

    let send = move |_| {
        if !can_send {
            return;
        }
        let github = match repo_choice() {
            RepoChoice::New { private } => Some(GithubOrigin::New {
                slug: if slug().trim().is_empty() {
                    slugify(&name())
                } else {
                    slug().trim().to_string()
                },
                private,
            }),
            RepoChoice::Existing { owner, repo } => Some(GithubOrigin::Existing { owner, repo }),
        };
        k.send(Command::CreateProject {
            id: ProjectId::new(),
            name: name().trim().to_string(),
            kind: kind(),
            desc: brief().trim().to_string(),
            workspace: None,
            github,
        });
        on_created.call(());
    };

    rsx! {
        div { style: "font-family:{serif};font-size:22px;font-weight:600;margin:14px 0 4px;", "你想做什么？" }
        p { style: "font-size:12.5px;color:{ink3};margin:0 0 14px;line-height:1.7;", "一个名字、一句你想做的事。剩下的问题会帮你补全 —— 答不上的交给系统兜底,不编造具体数字。" }
        div {
            style: "{card} padding:18px 20px;",
            div {
                style: "display:grid;grid-template-columns:1.4fr 1fr;gap:12px;margin-bottom:12px;",
                div {
                    label { style: "{label}", "项目名称 *" }
                    input {
                        style: "{input}",
                        placeholder: "例:增长实验看板",
                        value: "{name}",
                        oninput: move |e| {
                            name.set(e.value());
                            if !slug_touched() {
                                slug.set(slugify(&name()));
                            }
                        },
                    }
                }
                div {
                    label { style: "{label}", "项目类型" }
                    select {
                        style: "{input}",
                        value: "{kind}",
                        onchange: move |e| kind.set(e.value()),
                        for kd in KINDS {
                            option { value: "{kd}", "{kd}" }
                        }
                    }
                }
            }
            label { style: "{label}", "你想做什么 *" }
            textarea {
                style: "{input} min-height:90px;",
                placeholder: "一句话即可,多写几句问题会更少。例:把 agent 会话里长出的工作流沉淀成可复用资产,导入即跑。",
                value: "{brief}",
                oninput: move |e| brief.set(e.value()),
            }
            if is_new_repo {
                div {
                    style: "margin-top:10px;",
                    label { style: "{label}", "GitHub 仓名(可改)" }
                    input {
                        style: "{input} font-family:{theme::MONO};",
                        placeholder: "growth-kanban",
                        value: "{slug}",
                        oninput: move |e| {
                            slug_touched.set(true);
                            slug.set(e.value());
                        },
                    }
                }
            } else if let RepoChoice::Existing { owner, repo } = repo_choice() {
                p { style: "font-size:11.5px;color:{ink3};margin-top:10px;", "将接入 {owner}/{repo} ↗" }
            }
            div {
                style: "display:flex;justify-content:flex-end;margin-top:14px;",
                button {
                    style: "{theme::btn_primary()} opacity:{opacity};",
                    disabled: !can_send,
                    onclick: send,
                    "开始 ↑"
                }
            }
        }
        p { style: "font-size:11.5px;color:{ink3};margin:10px 2px 0;", "提交后即建立项目;之后的问答与起草随时可编辑,确认后才正式生效。" }
    }
}

// ───────────────────────── 2 · 快速问题 ─────────────────────────

#[component]
fn QuestionsCard(vm: CreateVm, cadence: Signal<Cadence>, on_next: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let mut cycle = use_signal(|| vm.cycle);
    let mut bench = use_signal(|| vm.benchmark.clone());
    let mut win = use_signal(|| vm.win.clone());
    let mut cad = use_signal(|| cadence.peek().clone());

    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let input = theme::input();
    let label = theme::label();
    let clay = theme::CLAY;

    let submit = move |_| {
        k.send(Command::SetCycle { cycle: cycle() });
        k.send(Command::UpdateBrief {
            benchmark: bench().trim().to_string(),
            opportunity: win().trim().to_string(),
        });
        cadence.set(cad());

        // Kick off the drafting run — a real (mock) workflow, same Engine as
        // any operating-view run. Its transcript is honestly mock; nothing
        // from it is copied into the editable review fields.
        let session = SessionId::new();
        k.send(Command::StartSession {
            id: session,
            stage_kind: Some(StageKind::Prototype),
            kind: SessionKind::Create,
            title: "创建 · 体系起草".into(),
        });
        k.send(Command::RunWorkflow {
            session,
            spec: drafting_workflow(),
        });
        on_next.call(());
    };

    rsx! {
        div { style: "font-family:{serif};font-size:18px;font-weight:600;margin:10px 0 14px;", "关于「{vm.name}」的几个问题" }

        {chip_question(
            "项目处在什么周期？",
            [ProjectCycle::Explore, ProjectCycle::Expand, ProjectCycle::Mature]
                .map(|c| (c.label(), c == cycle()))
                .to_vec(),
            move |i| cycle.set([ProjectCycle::Explore, ProjectCycle::Expand, ProjectCycle::Mature][i]),
        )}
        p { style: "font-size:11px;color:{ink3};margin:-6px 0 14px;", "{cycle().sub_label()} —— 决定五段的初始精力配比。" }

        {chip_question(
            "多久做一次体检复盘？",
            [Cadence::Weekly, Cadence::Daily, Cadence::RealTime]
                .iter()
                .map(|c| (cadence_chip_label(c), cadence_eq(&cad(), c)))
                .collect(),
            move |i| cad.set([Cadence::Weekly, Cadence::Daily, Cadence::RealTime][i].clone()),
        )}

        div {
            style: "{card} padding:16px 18px;margin-bottom:2px;",
            label { style: "{label}", "最像的对标(每行一个,不确定可留空)" }
            textarea {
                style: "{input} min-height:64px;",
                placeholder: "例:\nLinear\nHeight",
                value: "{bench}",
                oninput: move |e| bench.set(e.value()),
            }
        }
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            label { style: "{label}", "三个月后,怎样算成了？" }
            textarea {
                style: "{input} min-height:64px;",
                placeholder: "例:被持续复用、效率可量化提升……",
                value: "{win}",
                oninput: move |e| win.set(e.value()),
            }
        }

        div {
            style: "display:flex;justify-content:flex-end;",
            button {
                style: "cursor:pointer;background:{clay};color:#fff;border:none;border-radius:8px;padding:10px 20px;font:600 13px/1 inherit;",
                onclick: submit,
                "继续 · 开始起草 →"
            }
        }
    }
}

fn cadence_chip_label(c: &Cadence) -> &'static str {
    match c {
        Cadence::Weekly => "每周",
        Cadence::Daily => "每日",
        Cadence::RealTime => "实时",
        Cadence::Cron(_) => "自定义",
    }
}
fn cadence_eq(a: &Cadence, b: &Cadence) -> bool {
    std::mem::discriminant(a) == std::mem::discriminant(b)
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

// ───────────────────────── 3 · 起草中 ─────────────────────────

#[component]
fn DraftingCard(run: RunVm, on_next: EventHandler<()>) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let clay = theme::CLAY;
    let done = !run.running && run.failed.is_none() && !run.phases.is_empty();

    rsx! {
        div {
            style: "{card} padding:20px 22px;display:flex;flex-direction:column;gap:14px;",
            div {
                style: "display:flex;align-items:center;gap:10px;",
                if run.running {
                    span { style: "width:7px;height:7px;border-radius:50%;background:{clay};", "" }
                    span { style: "font-size:12.5px;color:{ink3};", "正在按方法论起草体系 —— 周期判定 · 北极星起草 · 指标框架 · 阶段激活…" }
                } else if let Some(err) = run.failed.clone() {
                    span { style: "font-size:12.5px;color:#B0503A;", "起草失败:{err}" }
                } else {
                    span { style: "font-size:12.5px;color:{ink2};", "起草完成 —— 以下候选均可编辑,确认前不算数。" }
                }
            }
            div {
                style: "display:flex;gap:8px;flex-wrap:wrap;",
                for (i, (name, ok)) in run.phases.iter().enumerate() {
                    {
                        let color = if *ok { "#5F7355" } else { clay };
                        let mark = if *ok { "✓" } else { "…" };
                        rsx! {
                            span {
                                key: "{i}",
                                style: "border:1.4px solid {color};color:{color};border-radius:7px;padding:3px 10px;font-size:12px;",
                                "{name} {mark}"
                            }
                        }
                    }
                }
            }
            if done {
                div {
                    style: "display:flex;justify-content:flex-end;",
                    button {
                        style: "{theme::btn_primary()}",
                        onclick: move |_| on_next.call(()),
                        "查看起草结果 →"
                    }
                }
            }
        }
    }
}

// ───────────────────────── 4 · 审阅确认 ─────────────────────────

#[derive(Clone, PartialEq)]
struct MetricDraft {
    id: Option<MetricId>,
    name: String,
    def: String,
    current: String,
    target: String,
}

impl MetricDraft {
    fn empty() -> Self {
        MetricDraft {
            id: None,
            name: String::new(),
            def: String::new(),
            current: String::new(),
            target: String::new(),
        }
    }
    fn from_vm(m: &MetricVm) -> Self {
        MetricDraft {
            id: Some(m.id),
            name: m.name.clone(),
            def: m.def.clone(),
            current: m.value_raw.clone(),
            target: m.target_raw.clone(),
        }
    }
}

/// The three north-star candidates, all derived from real user input — no
/// invented specifics. `0` = the brief verbatim, `1` = brief + success
/// criteria, `2` = a blank slate to write fresh.
fn ns_candidate(idx: usize, brief: &str, win: &str) -> (String, String) {
    match idx % 3 {
        0 => (
            brief.to_string(),
            "（请编辑:怎么算、数据从哪来）".to_string(),
        ),
        1 if !win.is_empty() => (
            format!("{brief}(成功标准:{win})"),
            "（请编辑:怎么算、数据从哪来）".to_string(),
        ),
        1 => (brief.to_string(), String::new()),
        _ => (String::new(), String::new()),
    }
}

#[component]
fn ReviewCard(vm: CreateVm, cadence: Signal<Cadence>) -> Element {
    let k = use_context::<Kernel>();
    let mut ns_idx = use_signal(|| 0usize);
    let mut ns = use_signal(|| {
        if vm.north_star.is_empty() {
            vm.brief.clone()
        } else {
            vm.north_star.clone()
        }
    });
    let mut ns_def = use_signal(|| vm.ns_def.clone());
    let leading = use_signal({
        let vm = vm.clone();
        move || {
            if vm.leading.is_empty() {
                vec![MetricDraft::empty()]
            } else {
                vm.leading.iter().map(MetricDraft::from_vm).collect()
            }
        }
    });
    let lagging = use_signal({
        let vm = vm.clone();
        move || {
            if vm.lagging.is_empty() {
                vec![MetricDraft::empty()]
            } else {
                vm.lagging.iter().map(MetricDraft::from_vm).collect()
            }
        }
    });
    // C8 · plan/13 D8 末卡「立即让队友开工第一件?」—— 默认不勾,显式授权
    // 才在落地后对标配三件套里的①竞品分析 dispatch 一次真实 RunIssue。
    let mut run_first = use_signal(|| false);

    let card = theme::card();
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;

    let mix = vm.cycle.mix();
    let brief_for_regen = vm.brief.clone();
    let win_for_regen = vm.win.clone();
    let regen = move |_| {
        let next = ns_idx() + 1;
        ns_idx.set(next);
        let (star, def) = ns_candidate(next, &brief_for_regen, &win_for_regen);
        ns.set(star);
        ns_def.set(def);
    };

    let confirm = {
        let k = k.clone();
        move |_| {
            k.send(Command::UpdateNorthStar {
                value: ns().trim().to_string(),
                def: ns_def().trim().to_string(),
            });
            for row in leading() {
                if row.name.trim().is_empty() {
                    continue;
                }
                k.send(Command::UpsertManualMetric {
                    id: row.id.unwrap_or_else(MetricId::new),
                    name: row.name.trim().to_string(),
                    def: row.def.trim().to_string(),
                    role: MetricRole::Leading,
                    stage_kind: None,
                    target: row.target.trim().to_string(),
                    amber: Default::default(),
                    value: row.current.trim().to_string(),
                });
            }
            for row in lagging() {
                if row.name.trim().is_empty() {
                    continue;
                }
                k.send(Command::UpsertManualMetric {
                    id: row.id.unwrap_or_else(MetricId::new),
                    name: row.name.trim().to_string(),
                    def: row.def.trim().to_string(),
                    role: MetricRole::Lagging,
                    stage_kind: None,
                    target: row.target.trim().to_string(),
                    amber: Default::default(),
                    value: row.current.trim().to_string(),
                });
            }
            k.send(Command::CompleteCreation {
                cadence: cadence(),
                run_first: run_first(),
            });
            k.send(Command::SetPanel(Panel::Progress));
            k.send(Command::SetScope(Scope::All));
        }
    };

    rsx! {
        div {
            style: "{card} padding:18px 20px;margin-bottom:14px;",
            div { style: "display:flex;align-items:center;gap:8px;margin-bottom:10px;",
                span { style: "font-family:{serif};font-size:16px;font-weight:600;", "体系草案" }
                span { style: "font-size:10.5px;color:#B0503A;background:#F2E4DD;border-radius:4px;padding:3px 7px;", "审阅关口" }
                span { style: "margin-left:auto;font-size:11px;color:{ink3};", "改完确认即建立" }
            }

            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:14px;",
                span { style: "font-size:11.5px;color:{ink3};width:52px;flex:none;", "周期" }
                span { style: "font-size:13px;font-weight:600;", "{vm.cycle.label()}" }
                div {
                    style: "flex:1;display:flex;height:7px;border-radius:4px;overflow:hidden;max-width:200px;",
                    for (i, k) in StageKind::ALL.iter().enumerate() {
                        span { key: "{i}", style: "width:{mix[i]}%;background:{k.color()};", "" }
                    }
                }
                span { style: "font-size:11px;color:{ink3};", "{vm.cycle.main_loop_label()}" }
            }

            div {
                style: "background:#23211C;border-radius:10px;padding:15px 17px;margin-bottom:14px;",
                div {
                    style: "display:flex;align-items:center;gap:8px;margin-bottom:7px;",
                    span { style: "font-size:10.5px;letter-spacing:.08em;color:#E0A78F;", "北极星" }
                    button {
                        style: "cursor:pointer;margin-left:auto;background:transparent;color:#E0A78F;border:1px solid #4A453C;border-radius:5px;padding:5px 9px;font-size:11px;",
                        onclick: regen,
                        "↺ 换一版候选"
                    }
                }
                textarea {
                    style: "width:100%;min-height:52px;background:transparent;border:none;outline:none;color:#fff;font-family:{serif};font-size:15px;line-height:1.5;resize:vertical;",
                    value: "{ns}",
                    oninput: move |e| ns.set(e.value()),
                }
                input {
                    style: "width:100%;background:transparent;border:none;outline:none;color:#C9C2B4;font-size:11.5px;margin-top:6px;",
                    placeholder: "计算口径:怎么算、数据从哪来",
                    value: "{ns_def}",
                    oninput: move |e| ns_def.set(e.value()),
                }
            }

            div {
                style: "display:grid;grid-template-columns:1fr 1fr;gap:16px;margin-bottom:6px;",
                MetricList { title: "引领 · 每周推动", rows: leading, mono }
                MetricList { title: "滞后 · 只看不追", rows: lagging, mono }
            }

            div {
                style: "display:flex;align-items:center;gap:10px;margin:14px 0;",
                span { style: "font-size:11.5px;color:{ink3};width:52px;flex:none;", "阶段" }
                div {
                    style: "display:flex;gap:4px;flex-wrap:wrap;",
                    for k in StageKind::ALL {
                        {
                            let hot = k == StageKind::Prototype;
                            let (bg, fg) = if hot { ("#F2E4DD", "#B0503A") } else { ("#EDE8DE", "#8C867A") };
                            rsx! {
                                span {
                                    key: "{k.index()}",
                                    title: "{k.role()}",
                                    style: "font-family:{mono};font-size:10px;background:{bg};color:{fg};border-radius:5px;padding:5px 8px;",
                                    "{k.index()} {k.label()}"
                                }
                            }
                        }
                    }
                }
            }
            div {
                style: "font-size:11.5px;color:{ink3};line-height:1.7;margin-bottom:16px;",
                "每{cadence_chip_label(&cadence())}一次体检定时任务 · 人只在五个交棒点介入(原型→构建→优化→推广→运维→回流原型)"
            }

            if !vm.github_remote.trim().is_empty() {
                {
                    let remote = vm.github_remote.clone();
                    let (box_bg, box_bd, mark) = if run_first() {
                        ("#C5654A", "#C5654A", "✓")
                    } else {
                        ("transparent", "#CFC7B6", "")
                    };
                    rsx! {
                        div {
                            style: "background:#F7F3EA;border:1px solid #E5DDCB;border-radius:8px;padding:12px 14px;margin-bottom:16px;",
                            div {
                                onclick: move |_| run_first.set(!run_first()),
                                style: "cursor:pointer;display:flex;align-items:flex-start;gap:10px;",
                                span { style: "width:16px;height:16px;margin-top:1px;border-radius:4px;border:1.5px solid {box_bd};background:{box_bg};color:#fff;font-size:10px;line-height:14px;text-align:center;flex:none;", "{mark}" }
                                div {
                                    div { style: "font-size:12.5px;font-weight:600;color:#3A3833;", "立即让队友开工第一件?" }
                                    p {
                                        style: "font-size:11px;color:{ink3};margin:4px 0 0;line-height:1.6;",
                                        "落地后自动建「竞品分析 → 找指标 → 绑数据」三张标配 Issue,真开进 {remote} 的 GitHub Issues。勾选后立即对①竞品分析 dispatch 一次真实运行;不勾就只建票,开工时机由你自己定。"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div {
                style: "display:flex;justify-content:flex-end;",
                button {
                    style: "cursor:pointer;background:#C5654A;color:#fff;border:none;border-radius:8px;padding:10px 22px;font:600 13px/1 inherit;box-shadow:0 3px 10px rgba(197,101,74,.25);",
                    onclick: confirm,
                    "确认 · 建立项目"
                }
            }
        }
    }
}

#[component]
fn MetricList(title: &'static str, rows: Signal<Vec<MetricDraft>>, mono: &'static str) -> Element {
    let ink3 = theme::INK_3;
    let input = theme::input();
    let snapshot = rows();
    rsx! {
        div {
            div { style: "font-size:10.5px;color:{ink3};margin-bottom:7px;", "{title}" }
            for (i, row) in snapshot.into_iter().enumerate() {
                div {
                    key: "{i}",
                    style: "border-bottom:1px dashed #EFEAE0;padding:6px 0;margin-bottom:4px;",
                    input {
                        style: "{input} padding:5px 7px;font-size:12px;margin-bottom:3px;",
                        placeholder: "指标名",
                        value: "{row.name}",
                        oninput: move |e| rows.write()[i].name = e.value(),
                    }
                    div {
                        style: "display:flex;gap:4px;",
                        input {
                            style: "{input} padding:5px 7px;font-size:11px;font-family:{mono};",
                            placeholder: "当前值",
                            value: "{row.current}",
                            oninput: move |e| rows.write()[i].current = e.value(),
                        }
                        input {
                            style: "{input} padding:5px 7px;font-size:11px;font-family:{mono};",
                            placeholder: "目标",
                            value: "{row.target}",
                            oninput: move |e| rows.write()[i].target = e.value(),
                        }
                    }
                }
            }
            button {
                style: "cursor:pointer;background:transparent;border:1px dashed #C9B8A4;border-radius:6px;color:{ink3};font-size:11px;padding:5px 10px;width:100%;",
                onclick: move |_| rows.write().push(MetricDraft::empty()),
                "+ 添加一条"
            }
        }
    }
}
