//! `Hub::Agent` — the agent library: a 2-column card grid, matching the
//! prototype's AgentHub. Real store-backed CRUD.

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::AgentId;
use dioxus::prelude::*;

#[component]
pub fn AgentHub(hub: HubVm) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.agents.len();

    let mut creating = use_signal(|| false);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "AGENTHUB" }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin:4px 0 18px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "智能体库" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 智能体" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 配置智能体" }
                }
            }
            if creating() {
                CreateAgentForm { on_done: move |_| creating.set(false) }
            }
            if hub.agents.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有智能体——点「+ 配置智能体」录入第一个。" }
            } else {
                div {
                    style: "display:grid;grid-template-columns:repeat(2,1fr);gap:14px;",
                    for a in hub.agents.clone() {
                        AgentCard { key: "{a.id.uuid()}", a }
                    }
                }
            }
        }
    }
}

#[component]
fn AgentCard(a: ui::vm::AgentCardVm) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let agent_color = theme::AGENT;
    let chip = theme::chip("#EFE9DA", theme::INK_2);
    rsx! {
        div {
            style: "{card} padding:16px 18px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:10px;",
                div {
                    style: "width:36px;height:36px;border-radius:9px;background:{agent_color};color:#FFF;display:flex;align-items:center;justify-content:center;font-family:{theme::SERIF};font-weight:700;font-size:14px;flex:none;",
                    "{a.initial}"
                }
                div { style: "flex:1;min-width:0;",
                    div { style: "font-size:13.5px;font-weight:500;", "{a.name}" }
                    div { style: "font-size:11.5px;color:{ink3};line-height:1.5;", "{a.role}" }
                }
                span { style: "{chip} flex:none;", "{a.maturity_label}" }
            }
            if !a.skills.is_empty() {
                div {
                    style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:10px;",
                    for (i , s) in a.skills.iter().enumerate() {
                        span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", ink2)}", "{s}" }
                    }
                }
            }
            div {
                style: "display:flex;align-items:center;gap:10px;font-size:11px;color:{ink3};font-family:{mono};",
                span { "{a.model}" }
                span { "·" }
                span { "{a.runs} 次运行" }
                span { style: "margin-left:auto;", "采纳 {a.win_rate}" }
            }
        }
    }
}

#[component]
fn CreateAgentForm(on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();

    let mut name = use_signal(String::new);
    let mut role = use_signal(String::new);
    let mut model = use_signal(|| "claude-sonnet".to_string());
    let mut skills_text = use_signal(String::new);

    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        let skills: Vec<String> = skills_text()
            .split(&[',', '、'][..])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        k.send(Command::CreateAgent {
            id: AgentId::new(),
            name: n,
            role: role().trim().to_string(),
            skills,
            model: model().trim().to_string(),
        });
        name.set(String::new());
        role.set(String::new());
        skills_text.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 竞品分析 Agent",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "角色描述" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "这个 agent 擅长什么、有什么约束",
                value: "{role}",
                oninput: move |e| role.set(e.value()),
            }
            div { style: "{label}", "绑定模型" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{model}",
                oninput: move |e| model.set(e.value()),
            }
            div { style: "{label}", "技能(逗号分隔)" }
            input {
                style: "{input} margin-bottom:12px;",
                placeholder: "如 web-scan, 对比矩阵",
                value: "{skills_text}",
                oninput: move |e| skills_text.set(e.value()),
            }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                onclick: save,
                "保存"
            }
        }
    }
}
