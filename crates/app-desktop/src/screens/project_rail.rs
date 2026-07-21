//! Project-scoped component rail (plan/10 K1). When a project is open, this
//! renders as a second column between the global icon rail (`chrome::IconRail`
//! — the marketplace, unfiltered) and the content area, listing *this
//! project's own* skill/agent/workflow/cron/connector rows.
//!
//! Grouped by real `project_id` on each VM row: 本项目自建 (`Some(active)`)
//! vs 共享/引入 (`None`, borrowed from the global library). An empty own-group
//! says so honestly instead of hiding — this is a real gap in what a project
//! has built for itself, not a rendering bug.
//!
//! Clicking any row jumps to the matching global Hub (same `Hub` rail target
//! the icon rail already uses) — this sidebar is a filtered index into the
//! same data, not a second source of truth or a second detail view.

use crate::kernel::HubVm;
use crate::screens::chrome::Hub;
use crate::theme;
use bw_core::ProjectId;
use dioxus::prelude::*;

#[component]
pub fn ProjectRail(project_id: ProjectId, hub: HubVm, on_pick: EventHandler<Hub>) -> Element {
    let border = theme::BORDER;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;

    let own_skills: Vec<_> = hub
        .skills
        .iter()
        .filter(|s| s.project_id == Some(project_id))
        .collect();
    let shared_skills = hub.skills.len() - own_skills.len();

    let own_agents: Vec<_> = hub
        .agents
        .iter()
        .filter(|a| a.project_id == Some(project_id))
        .collect();
    let shared_agents = hub.agents.len() - own_agents.len();

    let own_workflows: Vec<_> = hub
        .workflows
        .iter()
        .filter(|w| w.project_id == Some(project_id))
        .collect();
    let shared_workflows = hub.workflows.len() - own_workflows.len();

    let own_crons: Vec<_> = hub
        .cron_tasks
        .iter()
        .filter(|c| c.project_id == Some(project_id))
        .collect();
    let cross_project_crons = hub.cron_tasks.len() - own_crons.len();

    let own_connectors: Vec<_> = hub
        .connectors
        .iter()
        .filter(|c| c.project_id == Some(project_id))
        .collect();
    let global_connectors = hub.connectors.len() - own_connectors.len();

    rsx! {
        div {
            style: "width:198px;flex:none;background:{theme::RAIL_BG};border-right:1px solid {border};padding:16px 12px;overflow-y:auto;",
            div {
                style: "font-family:{mono};font-size:10.5px;letter-spacing:.06em;color:{ink3};margin-bottom:12px;",
                "本项目组件"
            }
            RailGroup {
                label: "技能",
                items: own_skills.iter().map(|s| (s.name.clone(), s.maturity_label.to_string())).collect::<Vec<_>>(),
                shared_count: shared_skills,
                shared_label: "共享",
                empty_hint: "本项目还没有自建的技能",
                on_click: move |_| on_pick.call(Hub::Skill),
            }
            RailGroup {
                label: "智能体",
                items: own_agents.iter().map(|a| (a.name.clone(), a.maturity_label.to_string())).collect::<Vec<_>>(),
                shared_count: shared_agents,
                shared_label: "共享",
                empty_hint: "本项目还没有自建的智能体",
                on_click: move |_| on_pick.call(Hub::Agent),
            }
            RailGroup {
                label: "工作流",
                items: own_workflows.iter().map(|w| (w.name.clone(), w.source_label.to_string())).collect::<Vec<_>>(),
                shared_count: shared_workflows,
                shared_label: "共享",
                empty_hint: "本项目还没有自建的工作流",
                on_click: move |_| on_pick.call(Hub::Workflow),
            }
            RailGroup {
                label: "定时",
                items: own_crons.iter().map(|c| (c.name.clone(), c.status_label.to_string())).collect::<Vec<_>>(),
                shared_count: cross_project_crons,
                shared_label: "全部项目",
                empty_hint: "本项目还没有定时任务",
                on_click: move |_| on_pick.call(Hub::Cron),
            }
            RailGroup {
                label: "连接器",
                items: own_connectors.iter().map(|c| (c.name.clone(), c.status_label.to_string())).collect::<Vec<_>>(),
                shared_count: global_connectors,
                shared_label: "全局",
                empty_hint: "本项目还没有连接器",
                on_click: move |_| on_pick.call(Hub::Connector),
            }
        }
    }
}

#[component]
fn RailGroup(
    label: &'static str,
    items: Vec<(String, String)>,
    shared_count: usize,
    shared_label: &'static str,
    empty_hint: &'static str,
    on_click: EventHandler<()>,
) -> Element {
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let border = theme::BORDER;
    let n = items.len();
    rsx! {
        div {
            style: "margin-bottom:16px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;cursor:pointer;margin-bottom:6px;",
                onclick: move |_| on_click.call(()),
                span { style: "font-size:12.5px;font-weight:500;color:{ink2};", "{label}" }
                span { style: "font-size:11px;color:{ink3};", "{n}" }
            }
            if items.is_empty() {
                div { style: "font-size:11px;color:{ink3};line-height:1.5;margin-bottom:4px;", "{empty_hint}" }
            } else {
                div {
                    style: "display:flex;flex-direction:column;gap:3px;",
                    for (name , meta) in items.iter() {
                        div {
                            key: "{name}",
                            style: "cursor:pointer;font-size:11.5px;color:{ink2};line-height:1.4;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;",
                            title: "{name} · {meta}",
                            onclick: move |e| {
                                e.stop_propagation();
                                on_click.call(());
                            },
                            "{name}"
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
