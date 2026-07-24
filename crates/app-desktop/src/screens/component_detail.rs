//! L1(plan/11): the project-rail's "click a component → see its full shape"
//! surface. `ProjectRail` used to jump straight to the global marketplace
//! Hub on click (`Hub::Skill` etc) — this replaces that jump with a real
//! detail page rendered right where you are, so a project's own two skills
//! stay two skills, not a detour into the full catalog.
//!
//! Four independent shapes, one per component kind — a skill's "what makes
//! it worth using" is not an agent's, is not a workflow's, is not a cron
//! task's. Each `*DetailCard` reads straight off the same `HubVm` the rail
//! and marketplace hubs already share — no second data source, no second
//! truth.

use crate::kernel::{HubVm, Kernel};
use crate::screens::agent_hub::workflows_using_agent;
use crate::screens::markdown::MarkdownView;
use crate::screens::skill_hub::{workflows_using_skill, SkillFileBrowser};
use crate::screens::workflow_flow::WorkflowFlow;
use crate::theme;
use bw_app::Command;
use bw_core::{AgentId, ConnectorId, CronTaskId, SkillId, WorkflowId};
use dioxus::prelude::*;
use ui::vm::{CronEffectivenessVm, ProjectCardVm};

/// Which component the project rail currently has open. `Copy` — this is a
/// cheap id-sized selection, not owned data (the actual VM is looked up out
/// of `HubVm` at render time, same convention as `expanded`/`editing`
/// signals elsewhere in this crate).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ComponentSel {
    Skill(SkillId),
    Agent(AgentId),
    Workflow(WorkflowId),
    Cron(CronTaskId),
    Connector(ConnectorId),
}

fn owner_project_name(
    pid: Option<bw_core::ProjectId>,
    projects: &[ProjectCardVm],
) -> Option<String> {
    pid.and_then(|id| projects.iter().find(|p| p.id == id))
        .map(|p| p.name.clone())
}

#[component]
pub fn ComponentDetail(
    sel: ComponentSel,
    hub: HubVm,
    projects: Vec<ProjectCardVm>,
    cron_effectiveness: Option<(CronTaskId, CronEffectivenessVm)>,
    on_close: EventHandler<()>,
    // T16 (plan/12 §10 v1.1#3): a workflow phase's agent/skill chip click,
    // re-pointing this very screen at a different `sel` — same `sel`/`hub`
    // Root signals this screen's own `on_close` already reaches into.
    on_select: EventHandler<ComponentSel>,
) -> Element {
    let paper = theme::PAPER;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:18px;",
                button {
                    style: "cursor:pointer;background:transparent;border:none;color:{ink3};font-size:13px;padding:0;",
                    onclick: move |_| on_close.call(()),
                    "← 返回项目"
                }
                span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "本项目组件 · 完整详情" }
            }
            match sel {
                ComponentSel::Skill(id) => rsx! { SkillDetailCard { id, hub, projects } },
                ComponentSel::Agent(id) => rsx! { AgentDetailCard { id, hub, projects } },
                ComponentSel::Workflow(id) => rsx! { WorkflowDetailCard { id, hub, projects, on_select } },
                ComponentSel::Cron(id) => rsx! { CronDetailCard { id, hub, projects, cron_effectiveness } },
                ComponentSel::Connector(id) => rsx! { ConnectorDetailCard { id, hub, projects } },
            }
        }
    }
}

#[component]
fn SkillDetailCard(id: SkillId, hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let Some(s) = hub.skills.iter().find(|x| x.id == id).cloned() else {
        return rsx! { div { style: "{card} padding:20px;color:{ink3};", "这个技能已不存在(可能被删除)。" } };
    };
    let used_by = workflows_using_skill(&hub, &s.name);
    let owner = owner_project_name(s.project_id, &projects);
    let origin_agent_name = s
        .origin_agent
        .and_then(|aid| hub.agents.iter().find(|a| a.id == aid))
        .map(|a| a.name.clone());
    rsx! {
        div {
            style: "{card} padding:22px 26px;max-width:760px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:6px;",
                span { style: "font-family:{theme::SERIF};font-size:20px;font-weight:600;", "{s.name}" }
                if let Some(p) = &owner {
                    span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)}", "◇ {p}" }
                }
                span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{s.maturity_label}" }
            }
            if !s.desc.is_empty() {
                div { style: "font-size:13.5px;color:{ink2};line-height:1.7;margin-bottom:12px;", "{s.desc}" }
            }
            div {
                style: "font-size:12px;color:{ink3};font-family:{theme::MONO};margin-bottom:10px;",
                "{s.uses} 次引用 · 被 {used_by.len()} 个工作流使用"
            }
            div {
                style: "display:flex;align-items:center;gap:8px;font-size:12px;color:{ink3};margin-bottom:14px;flex-wrap:wrap;",
                span { "{s.category}" }
                span { "·" }
                span { "{s.source_label}" }
                // T11(plan/12 §7):同 SkillHub 卡片——脱离源头后如实留痕。
                if let Some(lib) = &s.adapted_from {
                    span { style: "color:{ink3};font-style:italic;", "改编自 {lib}" }
                }
                if s.distilled_from_issue.is_some() {
                    span {
                        style: "{theme::chip(\"#EAF0E2\", \"#4A5E42\")}",
                        if let Some(a) = &origin_agent_name {
                            "⚗ 蒸馏自实战 · {a}"
                        } else {
                            "⚗ 蒸馏自实战"
                        }
                    }
                }
            }
            SkillFileBrowser { s: s.clone() }
            div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "被这些工作流使用" }
            if used_by.is_empty() {
                div { style: "font-size:12.5px;color:{ink3};", "还没有工作流引用这个技能。" }
            } else {
                div {
                    style: "display:flex;flex-wrap:wrap;gap:6px;",
                    for (i , wname) in used_by.iter().enumerate() {
                        span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", theme::CLAY)}", "{wname}" }
                    }
                }
            }
        }
    }
}

#[component]
fn AgentDetailCard(id: AgentId, hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let agent_color = theme::AGENT;
    let Some(a) = hub.agents.iter().find(|x| x.id == id).cloned() else {
        return rsx! { div { style: "{card} padding:20px;color:{ink3};", "这个智能体已不存在(可能被删除)。" } };
    };
    let used_by = workflows_using_agent(&hub, &a.name);
    let owner = owner_project_name(a.project_id, &projects);
    rsx! {
        div {
            style: "{card} padding:22px 26px;max-width:760px;",
            div {
                style: "display:flex;align-items:center;gap:12px;margin-bottom:10px;",
                div {
                    style: "width:44px;height:44px;border-radius:11px;background:{agent_color};color:#FFF;display:flex;align-items:center;justify-content:center;font-family:{theme::SERIF};font-weight:700;font-size:17px;flex:none;",
                    "{a.initial}"
                }
                div {
                    span { style: "font-family:{theme::SERIF};font-size:20px;font-weight:600;display:block;", "{a.name}" }
                    span { style: "font-size:12.5px;color:{ink3};", "{a.role}" }
                }
                if let Some(p) = &owner {
                    span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)} margin-left:auto;", "◇ {p}" }
                }
                span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{a.maturity_label}" }
            }
            div {
                style: "display:flex;align-items:center;gap:10px;font-size:12px;color:{ink3};font-family:{theme::MONO};margin:8px 0 14px;",
                span { "{a.runs} 次运行" }
                span { "·" }
                if a.win_rate.is_empty() {
                    span { "成功率 —(无运行证据)" }
                } else {
                    span { "成功率 {a.win_rate}" }
                }
                span { "·" }
                span { "被 {used_by.len()} 个工作流使用" }
            }
            div {
                style: "display:flex;align-items:center;gap:6px;flex-wrap:wrap;margin-bottom:14px;",
                span { style: "{theme::chip(\"#EFE9DA\", ink2)} font-family:{theme::MONO};", "{a.model}" }
                for (i , s) in a.skills.iter().enumerate() {
                    span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", ink2)}", "{s}" }
                }
            }
            div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "常驻指令(角色系统提示;{{var}} 槽位在运行时按项目填充)" }
            div {
                style: "margin-bottom:14px;",
                MarkdownView {
                    content: a.instructions.clone(),
                    empty_label: "目录引用 · 无本地指令".to_string(),
                }
            }
            div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "被这些工作流使用" }
            if used_by.is_empty() {
                div { style: "font-size:12.5px;color:{ink3};", "还没有工作流引用这个智能体。" }
            } else {
                div {
                    style: "display:flex;flex-wrap:wrap;gap:6px;",
                    for (i , wname) in used_by.iter().enumerate() {
                        span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", theme::CLAY)}", "{wname}" }
                    }
                }
            }
        }
    }
}

#[component]
fn WorkflowDetailCard(
    id: WorkflowId,
    hub: HubVm,
    projects: Vec<ProjectCardVm>,
    on_select: EventHandler<ComponentSel>,
) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let Some(d) = hub
        .workflow_details
        .iter()
        .find(|x| x.row.id == id)
        .cloned()
    else {
        return rsx! { div { style: "{card} padding:20px;color:{ink3};", "这个工作流已不存在,或是一次性临时任务(没有持久详情)。" } };
    };
    let row = d.row.clone();
    let owner = owner_project_name(row.project_id, &projects);
    let has_content = !row.content.trim().is_empty();
    // T16 (plan/12 §10 v1.1#3): 文档⇄流程图双视图,默认流程图(与
    // WorkflowHub 展开态默认一致)。
    let mut show_doc = use_signal(|| false);
    rsx! {
        div {
            style: "{card} padding:22px 26px;max-width:800px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:6px;",
                span { style: "font-family:{theme::SERIF};font-size:20px;font-weight:600;", "{row.name}" }
                if let Some(p) = &owner {
                    span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)}", "◇ {p}" }
                }
                span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{row.maturity_label}" }
            }
            div { style: "font-size:11px;color:{ink3};margin-bottom:4px;", "解决" }
            div { style: "font-size:12.5px;color:{ink2};margin-bottom:10px;", "{row.goal}" }
            div {
                style: "font-family:{mono};font-size:12px;color:{ink3};margin-bottom:10px;",
                if row.last_run_label.is_empty() {
                    "{row.version_label} · {row.uses} 次复用 · {row.record_label}"
                } else {
                    "{row.version_label} · {row.uses} 次复用 · {row.record_label} · {row.last_run_label}"
                }
            }
            div {
                style: "display:flex;align-items:center;gap:8px;margin-bottom:16px;flex-wrap:wrap;",
                span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{row.source_label}" }
                if let Some(t) = &row.trigger {
                    span { style: "{theme::chip(\"#F4F0E7\", theme::CLAY)} font-family:{mono};", "{t}" }
                }
                span { style: "font-size:11.5px;color:{ink3};", "主责 {row.primary_agent}" }
            }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;",
                span { style: "font-size:11.5px;color:{ink3};", if show_doc() { "文档" } else { "全流程" } }
                div {
                    style: "display:flex;gap:4px;align-items:center;",
                    // T17(plan/12 §10 v1.1#4):显式触发的解析动作——content 为空
                    // (结构化定义/未撰写正文)诚实禁用,绝不假装能解析一份不存在
                    // 的文档;失败走 Kernel 通用错误 toast(`UiNote::Error`),
                    // 可再次点击重试——从不自动重跑。
                    button {
                        style: if has_content {
                            "cursor:pointer;background:transparent;border:1px solid {theme::CLAY};color:{theme::CLAY};border-radius:6px;padding:2px 10px;font-size:10.5px;"
                        } else {
                            "cursor:not-allowed;background:transparent;border:1px solid {theme::BORDER};color:{ink3};border-radius:6px;padding:2px 10px;font-size:10.5px;opacity:.55;"
                        },
                        disabled: !has_content,
                        title: if has_content { "读文档,真实执行解析,成功后覆盖流程图(先留版本快照)" } else { "无原始文档,无可解析" },
                        onclick: move |_| {
                            if has_content {
                                k.send(Command::ParseWorkflowContent { workflow_id: id });
                                show_doc.set(false);
                            }
                        },
                        "🔍 解析为流程图"
                    }
                    button {
                        style: if show_doc() {
                            "cursor:pointer;background:transparent;border:1px solid {theme::BORDER};color:{ink3};border-radius:6px;padding:2px 10px;font-size:10.5px;"
                        } else {
                            "cursor:pointer;background:{theme::CLAY};border:1px solid {theme::CLAY};color:#FFF;border-radius:6px;padding:2px 10px;font-size:10.5px;"
                        },
                        onclick: move |_| show_doc.set(false),
                        "流程图"
                    }
                    button {
                        style: if show_doc() {
                            "cursor:pointer;background:{theme::CLAY};border:1px solid {theme::CLAY};color:#FFF;border-radius:6px;padding:2px 10px;font-size:10.5px;"
                        } else {
                            "cursor:pointer;background:transparent;border:1px solid {theme::BORDER};color:{ink3};border-radius:6px;padding:2px 10px;font-size:10.5px;"
                        },
                        onclick: move |_| show_doc.set(true),
                        "文档"
                    }
                }
            }
            div {
                style: "margin-bottom:14px;",
                if show_doc() {
                    MarkdownView {
                        content: row.content.clone(),
                        empty_label: "结构化定义,无原始文档".to_string(),
                    }
                } else {
                    WorkflowFlow {
                        phases: row.phase_metas.clone(),
                        loop_retries: row.loop_retries,
                        loop_max_iter: row.loop_max_iter,
                        agents: hub.agents.clone(),
                        skills: hub.skills.clone(),
                        on_select,
                    }
                }
            }
            if !d.agents.is_empty() {
                div { style: "font-size:11.5px;color:{ink3};margin:10px 0 6px;", "涉及智能体" }
                div { style: "margin-bottom:8px;",
                    for (i , (name , def , _from)) in d.agents.iter().enumerate() {
                        span { key: "ag{i}", title: "{def}", style: "{theme::chip(\"#EDE8F5\", theme::AGENT)} margin-right:6px;", "◆ {name}" }
                    }
                }
            }
            if !d.skills.is_empty() {
                div { style: "font-size:11.5px;color:{ink3};margin:10px 0 6px;", "涉及技能" }
                div { style: "margin-bottom:8px;",
                    for (i , (name , def , _from)) in d.skills.iter().enumerate() {
                        span { key: "sk{i}", title: "{def}", style: "{theme::chip(\"#EFE9DA\", ink2)} margin-right:6px;", "🧩 {name}" }
                    }
                }
            }
        }
    }
}

#[component]
fn CronDetailCard(
    id: CronTaskId,
    hub: HubVm,
    projects: Vec<ProjectCardVm>,
    cron_effectiveness: Option<(CronTaskId, CronEffectivenessVm)>,
) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let Some(c) = hub.cron_tasks.iter().find(|x| x.id == id).cloned() else {
        return rsx! { div { style: "{card} padding:20px;color:{ink3};", "这个定时任务已不存在(可能被删除)。" } };
    };
    let owner = owner_project_name(c.project_id, &projects);
    let eff = cron_effectiveness
        .filter(|(eid, _)| *eid == id)
        .map(|(_, e)| e);
    // T10 (plan/12 §5): the raw `target` column is a real `SkillId`/full
    // prompt text for the two new modes — never show that opaque payload as
    // "目标"; show the honest human-facing reading `CronRowVm` already
    // derived instead (skill name / "(技能已删除)" / prompt preview).
    let target_display = if let Some(skill_label) = &c.skill_target_label {
        skill_label.clone()
    } else if let Some(preview) = &c.prompt_preview {
        preview.clone()
    } else {
        c.target.clone()
    };
    rsx! {
        div {
            style: "{card} padding:22px 26px;max-width:680px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:6px;",
                span { style: "font-family:{theme::SERIF};font-size:20px;font-weight:600;", "{c.name}" }
                if let Some(p) = &owner {
                    span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)}", "◇ {p}" }
                } else {
                    span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{c.project_label}" }
                }
                span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{c.status_label}" }
            }
            div { style: "font-size:13.5px;color:{ink2};margin-bottom:12px;",
                if !c.mode_icon.is_empty() {
                    "到点:{c.mode_icon} {c.mode_label} · 目标「{target_display}」"
                } else {
                    "到点:{c.mode_label} · 目标「{target_display}」"
                }
            }
            div {
                style: "font-family:{mono};font-size:12px;color:{ink3};margin-bottom:6px;",
                "{c.schedule_label} · 上次 {c.last_run} · 下次 {c.next_run}"
            }
            if let Some(stage) = c.issue_stage_label {
                div {
                    style: "font-size:12px;color:{ink3};margin-bottom:14px;",
                    if let Some(who) = &c.issue_assignee {
                        "建活作用阶段:{stage} · 指派:{who}"
                    } else {
                        "建活作用阶段:{stage} · 未指派"
                    }
                }
            } else {
                div { style: "margin-bottom:14px;", "" }
            }
            div { style: "font-size:11px;color:{ink3};margin-bottom:8px;border-top:1px dashed {theme::BORDER};padding-top:12px;", "真实有效性(cron_effectiveness · 按真实触发记录算)" }
            match eff {
                Some(e) => rsx! {
                    div {
                        style: "font-family:{mono};font-size:12.5px;color:{ink2};line-height:1.9;",
                        div { "触发 {e.fires} 次 · 成功 {e.ok_fires} · 失败 {e.failed_fires} · 有效性 {e.effectiveness_label}" }
                        div { "平均耗时 {e.avg_duration_label}" }
                        if !e.last_fire_label.is_empty() {
                            div { "{e.last_fire_label}" }
                        }
                    }
                },
                None => rsx! {
                    button {
                        style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12px;",
                        onclick: move |_| k.send(Command::LoadCronEffectiveness(id)),
                        "读取有效性"
                    }
                },
            }
        }
    }
}

#[component]
fn ConnectorDetailCard(id: ConnectorId, hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let Some(c) = hub.connectors.iter().find(|x| x.id == id).cloned() else {
        return rsx! { div { style: "{card} padding:20px;color:{ink3};", "这个连接器已不存在(可能被删除)。" } };
    };
    let owner = owner_project_name(c.project_id, &projects);
    rsx! {
        div {
            style: "{card} padding:22px 26px;max-width:680px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:6px;",
                span { style: "font-family:{theme::SERIF};font-size:20px;font-weight:600;", "{c.name}" }
                if let Some(p) = &owner {
                    span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)}", "◇ {p}" }
                }
                span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{c.status_label}" }
            }
            div { style: "font-family:{mono};font-size:12.5px;color:{ink2};margin-bottom:10px;", "{c.kind}" }
            div { style: "font-size:12.5px;color:{ink3};margin-bottom:6px;", "{c.scope}" }
            div { style: "font-size:11.5px;color:{ink3};", "最近同步:{c.last_sync}" }
            if !c.syncable {
                div { style: "font-size:11.5px;color:{ink3};margin-top:8px;", "目录引用条目 · 没有真实探针,不冒充已连接。" }
            }
        }
    }
}
