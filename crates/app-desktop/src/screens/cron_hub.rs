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

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::model::{Cadence, CronStatus};
use bw_core::CronTaskId;
use dioxus::prelude::*;
use ui::vm::ProjectCardVm;

#[component]
pub fn CronHub(
    hub: HubVm,
    projects: Vec<ProjectCardVm>,
    on_trigger: EventHandler<CronTaskId>,
) -> Element {
    let k = use_context::<Kernel>();
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
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
                "真实调度:应用运行期间,「正常」状态且已绑定项目、目标名与某个 Hub 工作流同名的任务,到期后无需点击就会在后台自动触发(每几秒检查一次)——不是应用完全关闭时也在跑的常驻守护进程。「▶ 立即执行」是同一条真实路径的手动版,不等到期就立刻跑;「⏸ 暂停/▶ 恢复」是真实的人工介入,暂停的任务永远不会被自动触发。"
            }
            if creating() {
                CreateCronForm { projects: projects.clone(), on_done: move |_| creating.set(false) }
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
                            let k = k.clone();
                            let can_run = c.project_id.is_some()
                                && hub.workflows.iter().any(|w| w.name == c.target);
                            let cron_id = c.id;
                            let paused = c.status == CronStatus::Paused;
                            let status_color = match c.status {
                                CronStatus::Failed => theme::ALERT_DEEP,
                                CronStatus::Running => theme::CLAY,
                                CronStatus::Paused => ink3,
                                CronStatus::Normal => ink2,
                            };
                            rsx! {
                                div {
                                    key: "{c.id.uuid()}",
                                    style: "display:grid;grid-template-columns:1.3fr .9fr .9fr .8fr .8fr 1.4fr;gap:10px;padding:10px 16px;font-size:12.5px;align-items:center;border-bottom:1px dashed {theme::BORDER};",
                                    div {
                                        div { style: "font-weight:500;", "{c.name}" }
                                        if !c.target.is_empty() {
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
                                            title: if can_run { "" } else { "需先绑定项目,且目标名与 WorkflowHub 里某个工作流同名" },
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
                    }
                }
            }
        }
    }
}

#[component]
fn CreateCronForm(projects: Vec<ProjectCardVm>, on_done: EventHandler<()>) -> Element {
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
        k.send(Command::CreateCronTask {
            id: CronTaskId::new(),
            name: n,
            target: target().trim().to_string(),
            schedule: schedule(),
            project_id,
        });
        name.set(String::new());
        target.set(String::new());
        project_choice.set(0);
        schedule.set(Cadence::Weekly);
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
                    div { style: "{label}", "绑定项目(需要绑定才能「▶ 立即执行」)" }
                    select {
                        style: "{input}",
                        onchange: move |e| {
                            if let Ok(i) = e.value().parse::<usize>() {
                                project_choice.set(i);
                            }
                        },
                        option { value: "0", "全部项目(不可立即执行)" }
                        for (i , p) in projects.iter().enumerate() {
                            option { key: "{i}", value: "{i + 1}", "{p.name}" }
                        }
                    }
                }
            }
            div { style: "{label}", "运行目标(需与 WorkflowHub 里某个工作流名称完全一致,才能「▶ 立即执行」/自动触发)" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "跑什么——一个工作流/routine 的名字",
                value: "{target}",
                oninput: move |e| target.set(e.value()),
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
