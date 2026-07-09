//! `view=projects` — the project wall. Starts EMPTY: the prototype's demo
//! projects were simulated and are deliberately not ported. Every card here is
//! a real project the user created through the creation flow.

use crate::theme;
use bw_app::Command;
use dioxus::prelude::*;
use ui::vm::ProjectCardVm;

#[component]
pub fn Wall(projects: Vec<ProjectCardVm>, on_new: EventHandler<()>) -> Element {
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let clay = theme::CLAY;
    let empty = projects.is_empty();
    rsx! {
        div {
            style: "max-width:1060px;margin:0 auto;padding:44px 40px 60px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:26px;",
                div {
                    style: "width:26px;height:26px;border-radius:7px;background:{clay};color:#FFF;display:flex;align-items:center;justify-content:center;font-family:{serif};font-weight:700;font-size:14px;",
                    "B"
                }
                span { style: "font-size:13px;color:{ink2};", "Builders' 工作台" }
            }
            h1 { style: "font-family:{serif};font-weight:600;font-size:30px;margin:0 0 6px;", "我的项目" }
            p { style: "color:{ink2};font-size:13px;margin:0 0 30px;",
                if empty {
                    "还没有项目。每个项目都从一句话意图开始,走完五段一环的创建引导 —— 从右侧虚线卡起步。"
                } else {
                    "每张卡的信号都由指标观测值派生 —— 绿点从不手设。"
                }
            }
            div {
                style: "display:grid;grid-template-columns:repeat(2, minmax(0,1fr));gap:18px;",
                for p in projects {
                    ProjectCard { card: p }
                }
                NewCard { on_new }
            }
        }
    }
}

#[component]
fn ProjectCard(card: ProjectCardVm) -> Element {
    let k = use_context::<crate::kernel::Kernel>();
    let shell = theme::card();
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let sig = ui::signal_color(card.signal);
    let dot = theme::dot(sig, 9);
    let (chip_bg, chip_fg) = if card.running {
        ("#E7EDE2", "#4A5E42")
    } else {
        ("#F2E4DD", "#B0503A")
    };
    let chip = theme::chip(chip_bg, chip_fg);
    let bar_color = ui::progress_color(card.progress);
    let progress = card.progress;
    let id = card.id;
    let desc_preview: String = card.desc.chars().take(72).collect();
    let mut confirming_delete = use_signal(|| false);
    rsx! {
        div {
            onclick: {
                let k = k.clone();
                move |_| {
                    if !confirming_delete() {
                        k.send(Command::OpenProject(id));
                    }
                }
            },
            style: "{shell} padding:18px 20px;cursor:pointer;",
            div {
                style: "display:flex;align-items:center;gap:6px;margin-bottom:12px;",
                span { style: "{chip}", "{card.phase_label}" }
                span { style: "font-size:11px;color:{ink3};", "{card.cycle_label}" }
                span { style: "margin-left:auto;{dot}" }
                button {
                    title: "删除项目",
                    style: "background:transparent;border:none;color:{ink3};cursor:pointer;font-size:14px;padding:0 0 0 8px;line-height:1;",
                    onclick: move |e| {
                        e.stop_propagation();
                        confirming_delete.set(true);
                    },
                    "×"
                }
            }
            div { style: "font-family:{serif};font-size:19px;font-weight:600;margin-bottom:6px;", "{card.name}" }
            if !desc_preview.is_empty() {
                div { style: "font-size:13px;color:{ink2};line-height:1.6;margin-bottom:10px;", "{desc_preview}" }
            }
            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "{card.meta}" }
            div {
                style: "height:6px;border-radius:3px;background:#E6E0D2;overflow:hidden;",
                div { style: "height:100%;width:{progress}%;background:{bar_color};border-radius:3px;" }
            }
            if confirming_delete() {
                div {
                    style: "margin-top:12px;padding-top:12px;border-top:1px dashed {ink3};display:flex;align-items:center;gap:8px;",
                    span { style: "font-size:11.5px;color:{ink3};flex:1;", "删除后不可恢复" }
                    button {
                        style: "cursor:pointer;background:{theme::ALERT_DEEP};color:#FFF;border:none;border-radius:6px;padding:5px 11px;font-size:11.5px;",
                        onclick: {
                            let k = k.clone();
                            move |e| {
                                e.stop_propagation();
                                k.send(Command::DeleteProject(id));
                            }
                        },
                        "确认删除"
                    }
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {ink3};border-radius:6px;padding:5px 11px;font-size:11.5px;",
                        onclick: move |e| {
                            e.stop_propagation();
                            confirming_delete.set(false);
                        },
                        "取消"
                    }
                }
            }
        }
    }
}

#[component]
fn NewCard(on_new: EventHandler<()>) -> Element {
    let clay = theme::CLAY;
    let ink3 = theme::INK_3;
    let border = theme::BORDER_DEEP;
    rsx! {
        button {
            // Not a project yet — step 0 collects the real basics locally and
            // only 开始创建 dispatches CreateProject.
            onclick: move |_| on_new.call(()),
            style: "min-height:170px;border:1.6px dashed {border};border-radius:10px;background:transparent;cursor:pointer;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:8px;color:{ink3};",
            span { style: "font-size:26px;color:{clay};line-height:1;", "+" }
            span { style: "font-size:13px;", "新建项目" }
        }
    }
}
