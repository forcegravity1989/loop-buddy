//! `Hub::Cron` — scheduled tasks. Real store-backed records, and a real
//! in-process scheduler (`App::tick_scheduler`, ticked every few seconds by
//! `app-desktop/src/kernel.rs`): while this app is running, a `Normal`-
//! status task bound to a project, whose target names a real Hub workflow,
//! really auto-fires on its own once `bw_core::model::cron_due` says so — no
//! click required. What's still honestly *not* here: a background daemon
//! that fires while the app is fully closed (Tier D territory — that belongs
//! to a `Connector`/server-side piece, not a desktop process) — see
//! `tick_scheduler`'s own doc comment in `bw-app/src/lib.rs`.
//!
//! "▶ 立即执行" is the human-initiated twin of the same real path: it fires
//! the task's target workflow right now instead of waiting for it to become
//! due, through the same `Command` sequence WorkflowHub uses, and records
//! the real outcome (`Command::MarkCronRun`). "⏸ 暂停/▶ 恢复" is real human
//! intervention (`Command::SetCronStatus`) — a paused task is the one thing
//! `tick_scheduler` will never auto-fire, checked first, every tick.
//!
//! T10 (plan/12 §5): two more real modes alongside `RunWorkflow`/`CreateIssue`
//! — `RunSkill` (a real `SkillId` reference; content becomes the prompt) and
//! `RunPrompt` (a bare prompt, no entity). Both really execute on
//! `tick_scheduler`'s auto-fire, same as `RunWorkflow`; "▶ 立即执行" stays
//! wired to `RunWorkflow` only for now (honestly disabled with a clear reason
//! otherwise — no silent no-op). The row-front icon (🔄/⚙/💬) and `RunPrompt`'s
//! 40-char preview + "点击展开全文" come from `CronRowVm` (`ui::vm::cron_row`);
//! `RunSkill`'s picker (`SkillPicker` below) is a real searchable popup over
//! the live Skill Hub, not a dropdown — the hub is meant to grow to
//! market-scale, and a `<select>` doesn't scale past a few dozen rows.

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::model::{Cadence, CronStatus};
use bw_core::CronTaskId;
use dioxus::prelude::*;
use ui::vm::{CronRowVm, ProjectCardVm, SkillCardVm};

#[component]
pub fn CronHub(
    hub: HubVm,
    projects: Vec<ProjectCardVm>,
    on_trigger: EventHandler<CronTaskId>,
) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.cron_tasks.len();

    let mut creating = use_signal(|| false);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "CRONHUB" }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin:4px 0 8px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "定时任务" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 任务" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 新建定时" }
                }
            }
            p { style: "color:{ink3};font-size:11.5px;line-height:1.6;margin:0 0 14px;",
                "真实调度:应用运行期间,「正常」状态且已绑定项目的任务,到期后无需点击就会在后台自动触发(每几秒检查一次)——不是应用完全关闭时也在跑的常驻守护进程。四种模式(🔄 运行工作流 / ⚙ 运行技能 / 💬 运行 Prompt / 建活 autopilot)到点都真实执行、真实记账。「▶ 立即执行」目前只接了 🔄 运行工作流这一种模式的手动版;「⏸ 暂停/▶ 恢复」是真实的人工介入,暂停的任务永远不会被自动触发。"
            }
            if creating() {
                CreateCronForm { hub: hub.clone(), projects: projects.clone(), on_done: move |_| creating.set(false) }
            }
            if hub.cron_tasks.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有定时任务——点「+ 新建定时」录入第一个。" }
            } else {
                div {
                    style: "{theme::card()} overflow:hidden;",
                    div {
                        style: "display:grid;grid-template-columns:1.3fr .9fr .9fr .8fr .8fr 1.4fr;gap:10px;padding:10px 16px;font-size:11px;color:{ink3};border-bottom:1px solid {theme::BORDER};",
                        span { "任务/目标" }
                        span { "频率" }
                        span { "项目" }
                        span { "上次/下次" }
                        span { "状态" }
                        span { "操作" }
                    }
                    for c in hub.cron_tasks.clone() {
                        {
                            let can_run = c.project_id.is_some()
                                && hub.workflows.iter().any(|w| w.name == c.target);
                            rsx! {
                                CronTaskRowView {
                                    key: "{c.id.uuid()}",
                                    c: c.clone(),
                                    can_run,
                                    on_trigger,
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One `CronHub` row — a real component (not an inline `rsx!` block) so the
/// `RunPrompt` "点击展开全文" toggle gets its own `use_signal`, isolated per
/// row and keyed by `CronTaskId` (the outer `for` loop's `key`), never
/// bleeding state between rows on re-render.
#[component]
fn CronTaskRowView(c: CronRowVm, can_run: bool, on_trigger: EventHandler<CronTaskId>) -> Element {
    let k = use_context::<Kernel>();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mut expanded = use_signal(|| false);
    let cron_id = c.id;
    let paused = c.status == CronStatus::Paused;
    let status_color = match c.status {
        CronStatus::Failed => theme::ALERT_DEEP,
        CronStatus::Running => theme::CLAY,
        CronStatus::Paused => ink3,
        CronStatus::Normal => ink2,
    };
    // T10: an honest reason for "▶ 立即执行" being disabled — the pre-T10
    // wiring only ever fires a RunWorkflow task's target right now, so a
    // RunSkill/RunPrompt task gets a specific "not wired yet" reason instead
    // of the generic (and, for these two modes, simply wrong) "目标名与某个
    // 工作流同名" message.
    let run_now_title = if can_run {
        String::new()
    } else if c.skill_target_label.is_some() || c.prompt_full.is_some() {
        "「▶ 立即执行」目前只接了 🔄 运行工作流模式的手动触发——这条到点仍会真实自动执行,只是还没有手动立即执行的入口".to_string()
    } else {
        "需先绑定项目,且目标名与 WorkflowHub 里某个工作流同名".to_string()
    };

    rsx! {
        div {
            style: "display:grid;grid-template-columns:1.3fr .9fr .9fr .8fr .8fr 1.4fr;gap:10px;padding:10px 16px;font-size:12.5px;align-items:center;border-bottom:1px dashed {theme::BORDER};",
            div {
                div {
                    style: "font-weight:500;display:flex;align-items:center;gap:6px;",
                    if !c.mode_icon.is_empty() {
                        span { title: "{c.mode_label}", "{c.mode_icon}" }
                    }
                    span { "{c.name}" }
                }
                if let Some(skill_label) = c.skill_target_label.clone() {
                    div {
                        style: if c.skill_missing {
                            format!("font-size:11px;color:{};", theme::ALERT_DEEP)
                        } else {
                            format!("font-size:11px;color:{ink3};")
                        },
                        if c.skill_missing { "⚠ {skill_label}" } else { "{skill_label}" }
                    }
                } else if let Some(preview) = c.prompt_preview.clone() {
                    div {
                        style: "font-size:11px;color:{ink3};cursor:pointer;text-decoration:underline dotted;",
                        title: "点击展开全文",
                        onclick: move |_| expanded.set(!expanded()),
                        "{preview}"
                    }
                    if expanded() {
                        div {
                            style: "font-size:11px;color:{ink2};white-space:pre-wrap;background:{theme::CARD_ALT};border-radius:6px;padding:8px;margin-top:4px;max-width:100%;",
                            "{c.prompt_full.clone().unwrap_or_default()}"
                        }
                    }
                } else if !c.target.is_empty() {
                    div { style: "font-size:11px;color:{ink3};", "{c.target}" }
                }
            }
            span { style: "color:{ink2};", "{c.schedule_label}" }
            span { style: "color:{ink2};", "{c.project_label}" }
            div {
                div { style: "font-size:11px;color:{ink3};", "{c.last_run}" }
                div { style: "font-size:11px;color:{ink3};", "{c.next_run}" }
            }
            span { style: "{theme::chip(\"#EFE9DA\", status_color)}", "{c.status_label}" }
            div {
                style: "display:flex;gap:6px;flex-wrap:wrap;",
                button {
                    style: if can_run {
                        format!("{} padding:5px 10px;font-size:11.5px;", theme::btn_primary())
                    } else {
                        format!(
                            "cursor:not-allowed;background:transparent;color:{ink3};border:1px solid {};border-radius:7px;padding:5px 10px;font-size:11.5px;opacity:.55;",
                            theme::BORDER,
                        )
                    },
                    disabled: !can_run,
                    title: "{run_now_title}",
                    onclick: move |_| {
                        if can_run {
                            on_trigger.call(cron_id);
                        }
                    },
                    "▶ 立即执行"
                }
                if paused {
                    button {
                        style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:5px 10px;font-size:11.5px;",
                        onclick: move |_| {
                            k.send(Command::SetCronStatus {
                                id: cron_id,
                                status: CronStatus::Normal,
                            });
                        },
                        "▶ 恢复"
                    }
                } else {
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {theme::BORDER};border-radius:7px;padding:5px 10px;font-size:11.5px;",
                        onclick: move |_| {
                            k.send(Command::SetCronStatus {
                                id: cron_id,
                                status: CronStatus::Paused,
                            });
                        },
                        "⏸ 暂停"
                    }
                }
            }
        }
    }
}

/// The three modes `CreateCronForm` can author today. `CreateIssue`
/// (autopilot) has no create-form entry yet — a pre-T10 gap this ticket
/// doesn't touch (未建,不假装).
#[derive(Clone, Copy, PartialEq)]
enum CronModeChoice {
    RunWorkflow,
    RunSkill,
    RunPrompt,
}

#[component]
fn CreateCronForm(hub: HubVm, projects: Vec<ProjectCardVm>, on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;

    let mut name = use_signal(String::new);
    let mut target = use_signal(String::new);
    // 0 = 全部项目 (None); 1..=projects.len() maps to projects[i-1].
    let mut project_choice = use_signal(|| 0usize);
    let mut schedule = use_signal(|| Cadence::Weekly);
    let mut mode_choice = use_signal(|| CronModeChoice::RunWorkflow);
    let mut picked_skill = use_signal(|| None::<SkillCardVm>);
    let mut picker_open = use_signal(|| false);
    let mut prompt_text = use_signal(String::new);

    let projects_for_save = projects.clone();
    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        let project_id = (project_choice() > 0)
            .then(|| projects_for_save.get(project_choice() - 1))
            .flatten()
            .map(|p| p.id);
        match mode_choice() {
            CronModeChoice::RunWorkflow => {
                k.send(Command::CreateCronTask {
                    id: CronTaskId::new(),
                    name: n,
                    target: target().trim().to_string(),
                    schedule: schedule(),
                    project_id,
                });
            }
            CronModeChoice::RunSkill => {
                let Some(skill) = picked_skill() else {
                    return; // no skill picked yet — nothing honest to save
                };
                k.send(Command::CreateRunSkillCronTask {
                    id: CronTaskId::new(),
                    name: n,
                    schedule: schedule(),
                    project_id,
                    skill_id: skill.id,
                });
            }
            CronModeChoice::RunPrompt => {
                let p = prompt_text().trim().to_string();
                if p.is_empty() {
                    return;
                }
                k.send(Command::CreateRunPromptCronTask {
                    id: CronTaskId::new(),
                    name: n,
                    schedule: schedule(),
                    project_id,
                    prompt: p,
                });
            }
        }
        name.set(String::new());
        target.set(String::new());
        project_choice.set(0);
        schedule.set(Cadence::Weekly);
        mode_choice.set(CronModeChoice::RunWorkflow);
        picked_skill.set(None);
        prompt_text.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div {
                style: "display:grid;grid-template-columns:1.3fr 1fr;gap:12px;margin-bottom:10px;",
                div {
                    div { style: "{label}", "名称" }
                    input {
                        style: "{input}",
                        placeholder: "如 每夜竞品扫描",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div {
                    div { style: "{label}", "绑定项目(需要绑定才能自动/立即执行)" }
                    select {
                        style: "{input}",
                        onchange: move |e| {
                            if let Ok(i) = e.value().parse::<usize>() {
                                project_choice.set(i);
                            }
                        },
                        option { value: "0", "全部项目(不可自动触发)" }
                        for (i , p) in projects.iter().enumerate() {
                            option { key: "{i}", value: "{i + 1}", "{p.name}" }
                        }
                    }
                }
            }
            div { style: "{label}", "到点做什么" }
            select {
                style: "{input} width:auto;margin-bottom:10px;",
                onchange: move |e| {
                    mode_choice.set(match e.value().as_str() {
                        "run_skill" => CronModeChoice::RunSkill,
                        "run_prompt" => CronModeChoice::RunPrompt,
                        _ => CronModeChoice::RunWorkflow,
                    });
                },
                option { value: "run_workflow", "🔄 运行工作流" }
                option { value: "run_skill", "⚙ 运行技能" }
                option { value: "run_prompt", "💬 运行 Prompt" }
            }
            match mode_choice() {
                CronModeChoice::RunWorkflow => rsx! {
                    div {
                        div { style: "{label}", "运行目标(需与 WorkflowHub 里某个工作流名称完全一致,才能「▶ 立即执行」/自动触发)" }
                        input {
                            style: "{input} margin-bottom:10px;",
                            placeholder: "跑什么——一个工作流/routine 的名字",
                            value: "{target}",
                            oninput: move |e| target.set(e.value()),
                        }
                    }
                },
                CronModeChoice::RunSkill => rsx! {
                    div {
                        div { style: "{label}", "选择技能(真实 id 引用,不是名字匹配)" }
                        div { style: "display:flex;align-items:center;gap:8px;margin-bottom:10px;",
                            button {
                                style: "cursor:pointer;background:{theme::CARD_ALT};color:{theme::INK};border:1px solid {theme::BORDER};border-radius:7px;padding:7px 12px;font-size:12.5px;text-align:left;flex:1;",
                                onclick: move |_| picker_open.set(true),
                                {
                                    match picked_skill() {
                                        Some(s) => format!("⚙ {} · {}", s.name, s.category),
                                        None => "点击选择技能…".to_string(),
                                    }
                                }
                            }
                        }
                        if picker_open() {
                            SkillPicker {
                                skills: hub.skills.clone(),
                                on_pick: move |s: SkillCardVm| {
                                    picked_skill.set(Some(s));
                                    picker_open.set(false);
                                },
                                on_close: move |_| picker_open.set(false),
                            }
                        }
                    }
                },
                CronModeChoice::RunPrompt => rsx! {
                    div {
                        div { style: "{label}", "Prompt 全文(直接跑这段文本,不依赖任何 skill/workflow 实体)" }
                        textarea {
                            style: "{input} min-height:84px;margin-bottom:10px;",
                            placeholder: "写下要定时执行的完整 prompt…",
                            value: "{prompt_text}",
                            oninput: move |e| prompt_text.set(e.value()),
                        }
                    }
                },
            }
            div { style: "{label}", "频率(真实调度——满足条件后无需点击,后台自动触发)" }
            select {
                style: "{input} width:auto;margin-bottom:6px;",
                onchange: move |e| {
                    schedule.set(match e.value().as_str() {
                        "realtime" => Cadence::RealTime,
                        "daily" => Cadence::Daily,
                        _ => Cadence::Weekly,
                    });
                },
                option { value: "daily", "每日(24 小时)" }
                option { value: "weekly", selected: true, "每周(7 天)" }
                option { value: "realtime", "实时(每次调度检查都触发)" }
            }
            p { style: "font-size:11px;color:{ink3};margin:0 0 12px;line-height:1.6;",
                "从未运行过的任务视为已到期,保存后的下一次后台检查(≤5 秒)就会真实触发一次。"
            }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                onclick: save,
                "保存"
            }
        }
    }
}

/// T10 (plan/12 §5): a real searchable popup over the live Skill Hub — the
/// issue explicitly calls out that a `<select>` dropdown doesn't scale once
/// the hub grows to market size, so this filters `name`/`desc` as you type
/// and returns on click, instead of listing every row in a closed dropdown.
/// References `workflow_hub.rs`'s `SkillAgentPicker` for the filter
/// convention (lowercase substring match, a result cap) but isn't the same
/// component: that one is an inline multi-select toggle embedded directly in
/// a form; this is a single-select modal overlay (pick one, close) — the
/// shape T10 actually asked for.
#[component]
fn SkillPicker(
    skills: Vec<SkillCardVm>,
    on_pick: EventHandler<SkillCardVm>,
    on_close: EventHandler<()>,
) -> Element {
    let ink3 = theme::INK_3;
    let mut filter = use_signal(String::new);
    let f = filter().to_lowercase();
    let total = skills.len();
    let shown: Vec<SkillCardVm> = skills
        .iter()
        .filter(|s| {
            f.is_empty() || s.name.to_lowercase().contains(&f) || s.desc.to_lowercase().contains(&f)
        })
        .take(80)
        .cloned()
        .collect();
    let shown_count = shown.len();

    rsx! {
        div {
            style: "position:fixed;inset:0;background:rgba(35,33,28,.38);z-index:70;display:flex;align-items:flex-start;justify-content:center;padding:56px 16px;",
            onclick: move |_| on_close.call(()),
            div {
                style: "{theme::card()} width:520px;max-width:94vw;max-height:70vh;overflow:hidden;display:flex;flex-direction:column;padding:16px 18px;",
                onclick: move |e| e.stop_propagation(),
                div {
                    style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:10px;",
                    span { style: "font-family:{theme::SERIF};font-size:15px;font-weight:600;", "选择技能" }
                    button {
                        style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:14px;",
                        onclick: move |_| on_close.call(()),
                        "✕"
                    }
                }
                input {
                    style: "{theme::input()} margin-bottom:10px;",
                    placeholder: "按名称/描述搜索…",
                    value: "{filter}",
                    oninput: move |e| filter.set(e.value()),
                }
                div {
                    style: "overflow-y:auto;flex:1;",
                    if shown.is_empty() {
                        div { style: "color:{ink3};font-size:12.5px;padding:20px 0;text-align:center;", "没有匹配的技能" }
                    }
                    for s in shown {
                        {
                            let picked = s.clone();
                            rsx! {
                                div {
                                    key: "{s.id.uuid()}",
                                    style: "cursor:pointer;padding:8px 10px;border-radius:7px;border-bottom:1px dashed {theme::BORDER};",
                                    onclick: move |_| on_pick.call(picked.clone()),
                                    div { style: "font-size:13px;font-weight:500;", "{s.name}" }
                                    div { style: "font-size:11px;color:{ink3};", "{s.category} · {s.desc}" }
                                }
                            }
                        }
                    }
                }
                div { style: "font-size:11px;color:{ink3};margin-top:8px;", "共 {total} 个技能 · 显示 {shown_count} 条匹配" }
            }
        }
    }
}
