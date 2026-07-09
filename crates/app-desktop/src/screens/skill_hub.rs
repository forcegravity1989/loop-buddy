//! `Hub::Skill` — the skill library: a flat card grid, matching the
//! prototype's own SkillHub (3-column grid, no per-card expand/filter — those
//! only exist on the Workflow hub). Real store-backed CRUD, not a stub.

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::model::LibSource;
use bw_core::SkillId;
use dioxus::prelude::*;

#[component]
pub fn SkillHub(hub: HubVm) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.skills.len();

    let mut creating = use_signal(|| false);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            div {
                style: "display:flex;align-items:baseline;gap:12px;margin-bottom:4px;",
                span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "SKILLHUB" }
            }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:18px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "技能库" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 技能" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 新建技能" }
                }
            }
            if creating() {
                CreateSkillForm { on_done: move |_| creating.set(false) }
            }
            if hub.skills.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有技能——点「+ 新建技能」录入第一个。" }
            } else {
                div {
                    style: "display:grid;grid-template-columns:repeat(3,1fr);gap:14px;",
                    for s in hub.skills.clone() {
                        SkillCard { key: "{s.id.uuid()}", s }
                    }
                }
            }
        }
    }
}

#[component]
fn SkillCard(s: ui::vm::SkillCardVm) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let (chip_bg, chip_fg) = ("#EFE9DA", theme::INK_2);
    let chip = theme::chip(chip_bg, chip_fg);
    rsx! {
        div {
            style: "{card} padding:16px 18px;",
            div {
                style: "display:flex;align-items:center;gap:8px;margin-bottom:8px;",
                span { style: "font-family:{theme::MONO};font-size:13px;font-weight:500;", "{s.name}" }
                span { style: "{chip} margin-left:auto;", "{s.maturity_label}" }
            }
            if !s.desc.is_empty() {
                div { style: "font-size:12px;color:{ink2};line-height:1.6;margin-bottom:10px;", "{s.desc}" }
            }
            div {
                style: "display:flex;align-items:center;gap:8px;font-size:11px;color:{ink3};",
                span { "{s.category}" }
                span { "·" }
                span { "{s.source_label}" }
                span { style: "margin-left:auto;", "{s.uses} 次引用" }
            }
        }
    }
}

#[component]
fn CreateSkillForm(on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;

    let mut name = use_signal(String::new);
    let mut desc = use_signal(String::new);
    let mut category = use_signal(String::new);

    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        k.send(Command::CreateSkill {
            id: SkillId::new(),
            name: n,
            desc: desc().trim().to_string(),
            category: category().trim().to_string(),
            source: LibSource::SelfBuilt,
        });
        name.set(String::new());
        desc.set(String::new());
        category.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 web-scan",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "描述" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "这个技能做什么",
                value: "{desc}",
                oninput: move |e| desc.set(e.value()),
            }
            div { style: "{label}", "分类" }
            input {
                style: "{input} margin-bottom:12px;",
                placeholder: "如 检索 / 数据 / 前端",
                value: "{category}",
                oninput: move |e| category.set(e.value()),
            }
            div {
                style: "display:flex;align-items:center;gap:10px;",
                button {
                    style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                    onclick: save,
                    "保存"
                }
                span { style: "font-size:11.5px;color:{ink3};", "新建的技能默认「打磨中」。" }
            }
        }
    }
}
