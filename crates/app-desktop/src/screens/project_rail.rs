//! Project-scoped component rail (plan/10 K1, reworked plan/11 L1). When a
//! project is open, this renders as a second column between the global icon
//! rail (`chrome::IconRail` — the marketplace, unfiltered) and the content
//! area, listing *this project's own* skill/agent/workflow/cron/connector
//! rows.
//!
//! Grouped by real `project_id` on each VM row: 本项目自建 (`Some(active)`)
//! vs 共享/引入 (`None`, borrowed from the global library). An empty own-group
//! says so honestly instead of hiding — this is a real gap in what a project
//! has built for itself, not a rendering bug.
//!
//! L1(plan/11): clicking a row used to jump to the matching global Hub
//! (marketplace) — that conflated "this project's own components" with "the
//! whole catalog". Now a click opens `ComponentDetail` in place: the full
//! shape of *that one component*, not a detour into everything else. The
//! group header is a plain label (no click target) since there's no "browse
//! all" concept left in this rail — browsing the full catalog is the icon
//! rail's job.

use crate::screens::component_detail::ComponentSel;
use crate::theme;
use bw_core::ProjectId;
use dioxus::prelude::*;
use ui::vm::{AgentCardVm, ConnectorCardVm, SkillCardVm, WorkflowHubRowVm};

#[derive(Clone, PartialEq)]
struct RailItem {
    name: String,
    meta: String,
    sel: ComponentSel,
}

#[component]
pub fn ProjectRail(
    project_id: ProjectId,
    hub: crate::kernel::HubVm,
    on_pick: EventHandler<ComponentSel>,
) -> Element {
    let border = theme::BORDER;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;

    let own_skills: Vec<&SkillCardVm> = hub
        .skills
        .iter()
        .filter(|s| s.project_id == Some(project_id))
        .collect();
    let shared_skills = hub.skills.len() - own_skills.len();
    let skill_items: Vec<RailItem> = own_skills
        .iter()
        .map(|s| RailItem {
            name: s.name.clone(),
            meta: s.maturity_label.to_string(),
            sel: ComponentSel::Skill(s.id),
        })
        .collect();

    let own_agents: Vec<&AgentCardVm> = hub
        .agents
        .iter()
        .filter(|a| a.project_id == Some(project_id))
        .collect();
    let shared_agents = hub.agents.len() - own_agents.len();
    let agent_items: Vec<RailItem> = own_agents
        .iter()
        .map(|a| RailItem {
            name: a.name.clone(),
            meta: a.maturity_label.to_string(),
            sel: ComponentSel::Agent(a.id),
        })
        .collect();

    let own_workflows: Vec<&WorkflowHubRowVm> = hub
        .workflows
        .iter()
        .filter(|w| w.project_id == Some(project_id))
        .collect();
    let shared_workflows = hub.workflows.len() - own_workflows.len();
    let workflow_items: Vec<RailItem> = own_workflows
        .iter()
        .map(|w| RailItem {
            name: w.name.clone(),
            meta: w.source_label.to_string(),
            sel: ComponentSel::Workflow(w.id),
        })
        .collect();

    let own_crons: Vec<_> = hub
        .cron_tasks
        .iter()
        .filter(|c| c.project_id == Some(project_id))
        .collect();
    let cross_project_crons = hub.cron_tasks.len() - own_crons.len();
    let cron_items: Vec<RailItem> = own_crons
        .iter()
        .map(|c| RailItem {
            name: c.name.clone(),
            meta: c.status_label.to_string(),
            sel: ComponentSel::Cron(c.id),
        })
        .collect();

    let own_connectors: Vec<&ConnectorCardVm> = hub
        .connectors
        .iter()
        .filter(|c| c.project_id == Some(project_id))
        .collect();
    let global_connectors = hub.connectors.len() - own_connectors.len();
    let connector_items: Vec<RailItem> = own_connectors
        .iter()
        .map(|c| RailItem {
            name: c.name.clone(),
            meta: c.status_label.to_string(),
            sel: ComponentSel::Connector(c.id),
        })
        .collect();

    rsx! {
        div {
            style: "width:198px;flex:none;background:{theme::RAIL_BG};border-right:1px solid {border};padding:16px 12px;overflow-y:auto;",
            div {
                style: "font-family:{mono};font-size:10.5px;letter-spacing:.06em;color:{ink3};margin-bottom:12px;",
                "本项目组件"
            }
            RailGroup {
                label: "技能",
                items: skill_items,
                shared_count: shared_skills,
                shared_label: "共享",
                empty_hint: "本项目还没有自建的技能",
                on_pick,
            }
            RailGroup {
                label: "智能体",
                items: agent_items,
                shared_count: shared_agents,
                shared_label: "共享",
                empty_hint: "本项目还没有自建的智能体",
                on_pick,
            }
            RailGroup {
                label: "工作流",
                items: workflow_items,
                shared_count: shared_workflows,
                shared_label: "共享",
                empty_hint: "本项目还没有自建的工作流",
                on_pick,
            }
            RailGroup {
                label: "定时",
                items: cron_items,
                shared_count: cross_project_crons,
                shared_label: "全部项目",
                empty_hint: "本项目还没有定时任务",
                on_pick,
            }
            RailGroup {
                label: "连接器",
                items: connector_items,
                shared_count: global_connectors,
                shared_label: "全局",
                empty_hint: "本项目还没有连接器",
                on_pick,
            }
        }
    }
}

#[component]
fn RailGroup(
    label: &'static str,
    items: Vec<RailItem>,
    shared_count: usize,
    shared_label: &'static str,
    empty_hint: &'static str,
    on_pick: EventHandler<ComponentSel>,
) -> Element {
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let border = theme::BORDER;
    let n = items.len();
    rsx! {
        div {
            style: "margin-bottom:16px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;margin-bottom:6px;",
                span { style: "font-size:12.5px;font-weight:500;color:{ink2};", "{label}" }
                span { style: "font-size:11px;color:{ink3};", "{n}" }
            }
            if items.is_empty() {
                div { style: "font-size:11px;color:{ink3};line-height:1.5;margin-bottom:4px;", "{empty_hint}" }
            } else {
                div {
                    style: "display:flex;flex-direction:column;gap:3px;",
                    for item in items.iter() {
                        {
                            let sel = item.sel;
                            rsx! {
                                div {
                                    key: "{item.name}",
                                    style: "cursor:pointer;font-size:11.5px;color:{ink2};line-height:1.4;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;",
                                    title: "{item.name} · {item.meta}",
                                    onclick: move |_| on_pick.call(sel),
                                    "{item.name}"
                                }
                            }
                        }
                    }
                }
            }
            if shared_count > 0 {
                div {
                    style: "font-size:10.5px;color:{ink3};margin-top:4px;padding-top:4px;border-top:1px dashed {border};",
                    "+ {shared_count} {shared_label}"
                }
            }
        }
    }
}
