//! `Hub::Cron` — scheduled tasks: a table, matching the prototype's CronHub
//! (real fields, no per-row actions — the prototype itself has none here).
//! Real store-backed records; this app has no actual cron *scheduler* (Tier D
//! territory — `Connector`/timer-driven observation), so a task recorded here
//! is a real reference entry, not a live-firing job.

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::model::Cadence;
use bw_core::CronTaskId;
use dioxus::prelude::*;

#[component]
pub fn CronHub(hub: HubVm) -> Element {
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
                style: "display:flex;align-items:center;justify-content:space-between;margin:4px 0 18px;",
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
            if creating() {
                CreateCronForm { on_done: move |_| creating.set(false) }
            }
            if hub.cron_tasks.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有定时任务——点「+ 新建定时」录入第一个。" }
            } else {
                div {
                    style: "{theme::card()} overflow:hidden;",
                    div {
                        style: "display:grid;grid-template-columns:1.6fr 1fr 1fr .8fr .8fr;gap:10px;padding:10px 16px;font-size:11px;color:{ink3};border-bottom:1px solid {theme::BORDER};",
                        span { "任务/目标" }
                        span { "频率" }
                        span { "项目" }
                        span { "上次/下次" }
                        span { "状态" }
                    }
                    for c in hub.cron_tasks.clone() {
                        div {
                            key: "{c.id.uuid()}",
                            style: "display:grid;grid-template-columns:1.6fr 1fr 1fr .8fr .8fr;gap:10px;padding:10px 16px;font-size:12.5px;align-items:center;border-bottom:1px dashed {theme::BORDER};",
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
                            span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{c.status_label}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn CreateCronForm(on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();

    let mut name = use_signal(String::new);
    let mut target = use_signal(String::new);

    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        k.send(Command::CreateCronTask {
            id: CronTaskId::new(),
            name: n,
            target: target().trim().to_string(),
            schedule: Cadence::Weekly,
            project_id: None,
        });
        name.set(String::new());
        target.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 每夜竞品扫描",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "运行目标" }
            input {
                style: "{input} margin-bottom:12px;",
                placeholder: "跑什么——一个工作流/routine 的名字",
                value: "{target}",
                oninput: move |e| target.set(e.value()),
            }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                onclick: save,
                "保存"
            }
        }
    }
}
