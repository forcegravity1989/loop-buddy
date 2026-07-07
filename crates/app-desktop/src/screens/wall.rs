//! `view=projects` — the project wall. Starts EMPTY: the prototype's demo
//! projects were simulated and are deliberately not ported. Every card here is
//! a real project the user created through the wizard.

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
                    "还没有项目。产品的每个项目都从七个控制点的创建引导开始 —— 从右侧虚线卡起步。"
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
    rsx! {
        div {
            onclick: move |_| k.send(Command::OpenProject(id)),
            style: "{shell} padding:18px 20px;cursor:pointer;",
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:12px;",
                span { style: "{chip}", "{card.phase_label}" }
                span { style: "{dot}" }
            }
            div { style: "font-family:{serif};font-size:19px;font-weight:600;margin-bottom:6px;", "{card.name}" }
            if !card.desc.is_empty() {
                div { style: "font-size:13px;color:{ink2};line-height:1.6;margin-bottom:10px;", "{card.desc}" }
            }
            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "{card.meta}" }
            div {
                style: "height:6px;border-radius:3px;background:#E6E0D2;overflow:hidden;",
                div { style: "height:100%;width:{progress}%;background:{bar_color};border-radius:3px;" }
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
