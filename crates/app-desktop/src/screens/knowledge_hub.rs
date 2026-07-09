//! `Hub::Knowledge` — knowledge sources: a single-column list, matching the
//! prototype's Knowledge hub (real fields, no per-row actions).

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::KnowledgeSourceId;
use dioxus::prelude::*;

#[component]
pub fn KnowledgeHub(hub: HubVm) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let card = theme::card();
    let n = hub.knowledge_sources.len();

    let mut creating = use_signal(|| false);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "KNOWLEDGE" }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin:4px 0 18px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "知识源" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 来源" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 添加来源" }
                }
            }
            if creating() {
                CreateKnowledgeForm { on_done: move |_| creating.set(false) }
            }
            if hub.knowledge_sources.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有知识源——点「+ 添加来源」录入第一个。" }
            } else {
                for k in hub.knowledge_sources.clone() {
                    div {
                        key: "{k.id.uuid()}",
                        style: "{card} padding:14px 18px;margin-bottom:8px;display:flex;align-items:center;gap:14px;",
                        div { style: "flex:1;min-width:0;",
                            div { style: "font-size:13.5px;font-weight:500;", "{k.name}" }
                            div { style: "font-size:11.5px;color:{ink3};", "{k.kind} · {k.chunks_label}" }
                        }
                        span { style: "font-size:11.5px;color:{ink2};", "用于 {k.used_by}" }
                        span { style: "font-size:11px;color:{ink3};", "{k.updated_label}" }
                    }
                }
            }
        }
    }
}

#[component]
fn CreateKnowledgeForm(on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();

    let mut name = use_signal(String::new);
    let mut kind = use_signal(String::new);
    let mut used_by = use_signal(String::new);

    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        k.send(Command::CreateKnowledgeSource {
            id: KnowledgeSourceId::new(),
            name: n,
            kind: kind().trim().to_string(),
            used_by: used_by().trim().to_string(),
        });
        name.set(String::new());
        kind.set(String::new());
        used_by.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 产品 PRD 库",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "格式" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 Notion / Markdown / OpenAPI",
                value: "{kind}",
                oninput: move |e| kind.set(e.value()),
            }
            div { style: "{label}", "使用者(agent 名)" }
            input {
                style: "{input} margin-bottom:12px;",
                placeholder: "哪个 agent 会用到它",
                value: "{used_by}",
                oninput: move |e| used_by.set(e.value()),
            }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                onclick: save,
                "保存"
            }
        }
    }
}
