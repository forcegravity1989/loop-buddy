//! `Hub::Agent` — the agent library: a 2-column card grid, matching the
//! prototype's AgentHub. Real store-backed CRUD, and now (like WorkflowHub)
//! a real detail/edit panel: click a card to expand it in place — full role
//! text, a real "被这些工作流使用" reverse lookup computed from the same
//! `hub.workflow_details` WorkflowHub already carries (an `AgentRef.name`
//! match, same by-name convention as everywhere else this hub cross-
//! references agents/skills), and an edit form dispatching
//! `Command::UpdateAgent` — content only, `maturity`/`runs`/`win_rate` stay
//! untouched, same rule `OptimizeWorkflowForm` established for workflows.

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::AgentId;
use dioxus::prelude::*;
use ui::vm::{AgentCardVm, ProjectCardVm};

#[component]
pub fn AgentHub(hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.agents.len();

    let mut creating = use_signal(|| false);
    let mut expanded = use_signal(|| None::<AgentId>);
    let mut editing = use_signal(|| None::<AgentId>);

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
                        {
                            let aid = a.id;
                            let is_open = expanded() == Some(aid);
                            let is_editing = editing() == Some(aid);
                            let used_by = workflows_using_agent(&hub, &a.name);
                            let owner_project = a
                                .project_id
                                .and_then(|pid| projects.iter().find(|p| p.id == pid))
                                .map(|p| p.name.clone());
                            rsx! {
                                AgentCard {
                                    key: "{aid.uuid()}",
                                    a,
                                    is_open,
                                    is_editing,
                                    used_by,
                                    owner_project,
                                    on_toggle: move |_| {
                                        expanded.set(if is_open { None } else { Some(aid) });
                                        editing.set(None);
                                    },
                                    on_edit: move |_| editing.set(Some(aid)),
                                    on_done_edit: move |_| editing.set(None),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Real reverse lookup: which Hub workflows list this agent (by name — the
/// same free-text `AgentRef` convention `SkillAgentPicker` already uses, not
/// a hard FK). Empty is honest ("nothing references this yet"), not hidden.
fn workflows_using_agent(hub: &HubVm, agent_name: &str) -> Vec<String> {
    hub.workflow_details
        .iter()
        .filter(|d| d.agents.iter().any(|(name, _, _)| name == agent_name))
        .map(|d| d.row.name.clone())
        .collect()
}

#[component]
fn AgentCard(
    a: AgentCardVm,
    is_open: bool,
    is_editing: bool,
    used_by: Vec<String>,
    /// 真实项目名(从 project_id 反查)——`None` = 共享/五角色内置 agent。
    owner_project: Option<String>,
    on_toggle: EventHandler<()>,
    on_edit: EventHandler<()>,
    on_done_edit: EventHandler<()>,
) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let agent_color = theme::AGENT;
    let chip = theme::chip("#EFE9DA", theme::INK_2);
    let span_style = if is_open { "grid-column:1/-1;" } else { "" };
    rsx! {
        div {
            style: "{card} padding:16px 18px;{span_style}",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:10px;cursor:pointer;",
                onclick: move |_| on_toggle.call(()),
                div {
                    style: "width:36px;height:36px;border-radius:9px;background:{agent_color};color:#FFF;display:flex;align-items:center;justify-content:center;font-family:{theme::SERIF};font-weight:700;font-size:14px;flex:none;",
                    "{a.initial}"
                }
                div { style: "flex:1;min-width:0;",
                    div { style: "font-size:13.5px;font-weight:500;", "{a.name}" }
                    div {
                        style: if is_open { "font-size:11.5px;color:{ink3};line-height:1.5;".to_string() } else { "font-size:11.5px;color:{ink3};line-height:1.5;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;".to_string() },
                        "{a.role}"
                    }
                }
                if let Some(p) = &owner_project {
                    span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)} flex:none;", "◇ {p}" }
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
                if a.win_rate.is_empty() {
                    span { style: "margin-left:auto;", "成功率 —(无运行证据)" }
                } else {
                    span { style: "margin-left:auto;", "成功率 {a.win_rate}" }
                }
            }
            if is_open {
                div {
                    style: "margin-top:12px;padding-top:12px;border-top:1px dashed {theme::BORDER};",
                    if is_editing {
                        EditAgentForm { a: a.clone(), on_done: move |_| on_done_edit.call(()) }
                    } else {
                        if a.instructions.trim().is_empty() {
                            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "目录引用 · 无本地指令(可「编辑」补充)" }
                        } else {
                            div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "常驻指令(角色系统提示;{{var}} 槽位在运行时按项目填充)" }
                            pre {
                                style: "font-family:{mono};font-size:11.5px;line-height:1.6;color:{ink2};background:{theme::CARD_ALT};border:1px solid {theme::BORDER};border-radius:8px;padding:10px 12px;white-space:pre-wrap;margin:0 0 10px;",
                                "{a.instructions}"
                            }
                        }
                        div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "被这些工作流使用" }
                        if used_by.is_empty() {
                            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "还没有工作流引用这个智能体。" }
                        } else {
                            div {
                                style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:10px;",
                                for (i , wname) in used_by.iter().enumerate() {
                                    span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", theme::CLAY)}", "{wname}" }
                                }
                            }
                        }
                        button {
                            style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12px;",
                            onclick: move |_| on_edit.call(()),
                            "编辑 →"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EditAgentForm(a: AgentCardVm, on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;
    let agent_id = a.id;

    let mut name = use_signal(|| a.name.clone());
    let mut role = use_signal(|| a.role.clone());
    let mut model = use_signal(|| a.model.clone());
    let mut skills_text = use_signal(|| a.skills.join(", "));
    let mut instructions = use_signal(|| a.instructions.clone());

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
        k.send(Command::UpdateAgent {
            id: agent_id,
            name: n,
            role: role().trim().to_string(),
            skills,
            model: model().trim().to_string(),
            instructions: instructions().trim().to_string(),
        });
        on_done.call(());
    };

    rsx! {
        div {
            style: "background:{theme::CARD_ALT};border:1px solid {theme::BORDER_DEEP};border-radius:9px;padding:14px 16px;",
            div { style: "font-size:12px;color:{theme::CLAY};margin-bottom:10px;font-weight:600;", "编辑「{a.name}」" }
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "角色描述" }
            input {
                style: "{input} margin-bottom:10px;",
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
                style: "{input} margin-bottom:10px;",
                value: "{skills_text}",
                oninput: move |e| skills_text.set(e.value()),
            }
            div { style: "{label}", "常驻指令(系统提示;留空=仅目录引用)" }
            textarea {
                style: "{input} margin-bottom:12px;min-height:120px;font-family:{theme::MONO};font-size:11.5px;line-height:1.6;resize:vertical;",
                value: "{instructions}",
                oninput: move |e| instructions.set(e.value()),
            }
            div {
                style: "display:flex;align-items:center;gap:10px;",
                button {
                    style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                    onclick: save,
                    "保存"
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {theme::BORDER};border-radius:7px;padding:7px 14px;font-size:12.5px;",
                    onclick: move |_| on_done.call(()),
                    "取消"
                }
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
    let mut instructions = use_signal(String::new);

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
            instructions: instructions().trim().to_string(),
        });
        name.set(String::new());
        role.set(String::new());
        skills_text.set(String::new());
        instructions.set(String::new());
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
                style: "{input} margin-bottom:10px;",
                placeholder: "如 web-scan, 对比矩阵",
                value: "{skills_text}",
                oninput: move |e| skills_text.set(e.value()),
            }
            div { style: "{label}", "常驻指令(系统提示;留空=仅目录引用)" }
            textarea {
                style: "{input} margin-bottom:12px;min-height:100px;font-family:{theme::MONO};font-size:11.5px;line-height:1.6;resize:vertical;",
                placeholder: "你是…;约束…",
                value: "{instructions}",
                oninput: move |e| instructions.set(e.value()),
            }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                onclick: save,
                "保存"
            }
        }
    }
}
