//! `Hub::Workflow` — the workflow library: grouped by the 5-stage lifecycle
//! plus a 6th cross-cutting "指标层" bucket, with independent stage/source
//! filter chips, matching the prototype's WorkflowHub (its richest, most
//! fully-realized hub — 50 real sample rows, not a stub). Real store-backed
//! CRUD, and real *execution* reachable from here, not just cataloging:
//!
//! - **创建**(`CreateWorkflowForm`)/**优化**(`OptimizeWorkflowForm`, "优化 →"
//!   on any row) both go through a real skill/agent picker
//!   (`SkillAgentPicker`) backed by the real Skill/AgentHub catalog — a
//!   workflow's `agents`/`skills` are real `AgentRef`/`SkillRef`, not always
//!   empty.
//! - **导入到项目**("确认导入") and the new **⚡ 临时任务** (ad-hoc `Dynamic`
//!   workflow) both really run (`RunHubWorkflow`/`RunWorkflow`) *and*
//!   navigate the caller to go watch it (`on_run`) — running a workflow from
//!   here no longer fires-and-forgets silently.
//! - **设为定时任务** dispatches straight into Cron Hub's own
//!   `Command::CreateCronTask` (same `schedule: Weekly, project_id: None`
//!   defaults Cron Hub's own create form uses).

use crate::kernel::{HubVm, Kernel};
use crate::screens::component_detail::ComponentSel;
use crate::screens::markdown::MarkdownView;
use crate::screens::workflow_flow::WorkflowFlow;
use crate::theme;
use bw_app::Command;
use bw_core::model::{
    AgentRef, Cadence, LoopConfig, PhaseMeta, SkillRef, StageKind, WorkflowKind, WorkflowSpec,
};
use bw_core::{CronTaskId, SessionId, WorkflowId};
use bw_store::SessionKind;
use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use ui::vm::{AgentCardVm, ProjectCardVm, SkillCardVm, WorkflowDetailVm, WorkflowHubRowVm};

#[component]
pub fn WorkflowHub(
    hub: HubVm,
    projects: Vec<ProjectCardVm>,
    on_run: EventHandler<()>,
    // T16 (plan/12 §10 v1.1#3): a phase's agent/skill chip click, bubbled up
    // to `main.rs` Root — reuses the exact `sel`/`hub` navigation
    // `ProjectRail`'s `on_pick` already drives, no second mechanism.
    on_select: EventHandler<ComponentSel>,
) -> Element {
    let k = use_context::<Kernel>();
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let card = theme::card();

    let mut creating = use_signal(|| false);
    let mut adhoc = use_signal(|| false);
    // T7 (plan/12 §0/§2/§3): shared `RoleFilter` — same "全部/五角色/通用"
    // dimension `SkillHub`/`AgentHub` now filter by too (`ui::vm::RoleFilter`),
    // replacing the bare `Option<StageKind>` this signal used to be (which had
    // no way to select "只看通用" — `None` meant "no filter" instead).
    let mut role_filter = use_signal(|| ui::vm::RoleFilter::All);
    let mut source_filter = use_signal(|| None::<&'static str>);
    let mut expanded = use_signal(|| None::<WorkflowId>);
    let mut importing = use_signal(|| None::<WorkflowId>);
    let mut optimizing = use_signal(|| None::<WorkflowId>);
    let mut import_target = use_signal(|| 0usize);
    let mut cron_added = use_signal(HashSet::<WorkflowId>::new);
    // T16: per-row 文档⇄流程图 view toggle. Keyed by row id so switching one
    // row's view doesn't affect any other expanded row; defaults to 流程图
    // (unchanged pre-T16 layout for the common "no content yet" case).
    let mut doc_view = use_signal(HashSet::<WorkflowId>::new);

    let n = hub.workflows.len();
    let chip_counts = ui::vm::source_chip_counts(&hub.workflows);
    let (role_stage_counts, role_general_count) = ui::vm::role_chip_counts(
        &hub.workflows
            .iter()
            .map(|r| r.stage_ref.and_then(StageKind::from_index))
            .collect::<Vec<_>>(),
    );
    let details_by_id: HashMap<WorkflowId, WorkflowDetailVm> = hub
        .workflow_details
        .iter()
        .cloned()
        .map(|d| (d.row.id, d))
        .collect();

    let filtered: Vec<WorkflowHubRowVm> = hub
        .workflows
        .iter()
        .filter(|r| role_filter().matches(r.stage_ref.and_then(StageKind::from_index)))
        .filter(|r| {
            source_filter()
                .map(|sf| r.source_label == sf)
                .unwrap_or(true)
        })
        .cloned()
        .collect();
    let groups = ui::vm::group_by_stage(&filtered);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "WORKFLOWHUB" }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin:4px 0 14px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "工作流库" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 工作流" }
                }
                div {
                    style: "display:flex;gap:8px;",
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink2};border:1px solid {theme::BORDER};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                        onclick: move |_| {
                            adhoc.set(!adhoc());
                            creating.set(false);
                        },
                        if adhoc() { "取消" } else { "⚡ 临时任务" }
                    }
                    button {
                        style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                        onclick: move |_| {
                            creating.set(!creating());
                            adhoc.set(false);
                        },
                        if creating() { "取消" } else { "+ 新建工作流" }
                    }
                }
            }
            if adhoc() {
                AdHocWorkflowForm {
                    skills: hub.skills.clone(),
                    agents: hub.agents.clone(),
                    projects: projects.clone(),
                    on_run: move |_| {
                        adhoc.set(false);
                        on_run.call(());
                    },
                }
            }
            if creating() {
                CreateWorkflowForm {
                    skills: hub.skills.clone(),
                    agents: hub.agents.clone(),
                    on_done: move |_| creating.set(false),
                }
            }

            div {
                style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:8px;",
                {
                    let active = role_filter() == ui::vm::RoleFilter::All;
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| role_filter.set(ui::vm::RoleFilter::All),
                            "全部"
                        }
                    }
                }
                for (sk , count) in role_stage_counts {
                    {
                        let active = role_filter() == ui::vm::RoleFilter::Stage(sk);
                        let (bg, fg): (&str, &str) = if active { (sk.color(), "#FFF") } else { ("#EFE9DA", ink2) };
                        rsx! {
                            button {
                                key: "{sk.index()}",
                                style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                                onclick: move |_| role_filter.set(ui::vm::RoleFilter::Stage(sk)),
                                "{sk.role_short()} · {count}"
                            }
                        }
                    }
                }
                {
                    let active = role_filter() == ui::vm::RoleFilter::General;
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| role_filter.set(ui::vm::RoleFilter::General),
                            "通用 · {role_general_count}"
                        }
                    }
                }
            }
            div {
                style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:16px;",
                {
                    let active = source_filter().is_none();
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| source_filter.set(None),
                            "全部来源"
                        }
                    }
                }
                for (label, count) in chip_counts {
                    {
                        let active = source_filter() == Some(label);
                        let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                        rsx! {
                            button {
                                key: "{label}",
                                style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                                onclick: move |_| source_filter.set(Some(label)),
                                "{label} · {count}"
                            }
                        }
                    }
                }
            }

            if filtered.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "没有符合筛选的工作流。" }
            }

            for (stage_opt, rows) in groups {
                if !rows.is_empty() {
                    {
                        let group_key = stage_opt.map(|s| s.index() as i32).unwrap_or(-1);
                        let (glabel, gcolor): (&str, &str) = match stage_opt {
                            Some(sk) => (sk.label(), sk.color()),
                            None => ("指标层", ink3),
                        };
                        let rows_len = rows.len();
                        rsx! {
                            div {
                                key: "{group_key}",
                                style: "margin-bottom:22px;",
                                div {
                                    style: "display:flex;align-items:center;gap:8px;margin-bottom:10px;",
                                    span { style: "{theme::dot(gcolor, 8)}" }
                                    span { style: "font-size:13.5px;font-weight:600;", "{glabel}" }
                                    span { style: "font-size:11.5px;color:{ink3};", "{rows_len} 个工作流" }
                                }
                                for row in rows {
                                    {
                                        let k = k.clone();
                                        let on_run = on_run;
                                        let projects = projects.clone();
                                        let row_id = row.id;
                                        let is_open = expanded() == Some(row_id);
                                        let picker_open = importing() == Some(row_id);
                                        let editing = optimizing() == Some(row_id);
                                        let stage_ref = row.stage_ref;
                                        let row_name = row.name.clone();
                                        let detail = details_by_id.get(&row_id).cloned();
                                        let skills_pool = hub.skills.clone();
                                        let agents_pool = hub.agents.clone();
                                        // 真实项目名(从 project_id 反查)——`None` = 共享/内建阶段模板。
                                        let owner_project = row
                                            .project_id
                                            .and_then(|pid| projects.iter().find(|p| p.id == pid))
                                            .map(|p| p.name.clone());
                                        rsx! {
                                            div {
                                                key: "{row_id.uuid()}",
                                                style: "{card} padding:14px 16px;margin-bottom:8px;",
                                                // ── 1. 身份行 ──
                                                div {
                                                    style: "display:flex;align-items:center;gap:12px;cursor:pointer;",
                                                    onclick: move |_| expanded.set(if is_open { None } else { Some(row_id) }),
                                                    span { style: "font-size:13px;font-weight:500;flex:1;min-width:0;", "{row.name}" }
                                                    if let Some(p) = &owner_project {
                                                        span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)}", "◇ {p}" }
                                                    }
                                                    span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{row.maturity_label}" }
                                                }
                                                // ── 2. 一句话价值主张:这个工作流解决什么问题(L3 详情页画流程图前的文字版)──
                                                div { style: "font-size:12px;color:{ink2};margin-top:6px;", "解决:{row.goal}" }
                                                // ── 3. 社会证明:真实复用数 + 真实运行战绩(暂无运行=诚实冷,绝不 0%)──
                                                div {
                                                    style: "font-family:{mono};font-size:11.5px;color:{ink3};margin-top:6px;",
                                                    if row.last_run_label.is_empty() {
                                                        "{row.version_label} · {row.uses} 次复用 · {row.record_label}"
                                                    } else {
                                                        "{row.version_label} · {row.uses} 次复用 · {row.record_label} · {row.last_run_label}"
                                                    }
                                                }
                                                // ── 4. 出处可信度 + 怎么用:来源 · 触发词 · 主责 agent ──
                                                div {
                                                    style: "display:flex;align-items:center;gap:8px;margin-top:6px;flex-wrap:wrap;",
                                                    span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{row.source_label}" }
                                                    if let Some(t) = &row.trigger {
                                                        span { style: "{theme::chip(\"#F4F0E7\", theme::CLAY)} font-family:{mono};", "{t}" }
                                                    }
                                                    span { style: "font-size:11.5px;color:{ink3};", "{row.primary_agent}" }
                                                }
                                                if is_open {
                                                    div {
                                                        style: "margin-top:12px;padding-top:12px;border-top:1px dashed {theme::BORDER};",
                                                        if editing {
                                                            if let Some(d) = detail.clone() {
                                                                OptimizeWorkflowForm {
                                                                    skills: skills_pool,
                                                                    agents: agents_pool,
                                                                    detail: d,
                                                                    on_done: move |_| optimizing.set(None),
                                                                }
                                                            }
                                        } else {
                                                            // T16(plan/12 §10 v1.1#3):文档⇄流程图双视图——
                                                            // 折叠态摘要行的「解决:」一句话仍是纯文本(同
                                                            // desc/role 的既有约定),这里切的是展开态的
                                                            // 正文/全流程主区。
                                                            {
                                                                let is_doc = doc_view().contains(&row_id);
                                                                rsx! {
                                                                    div {
                                                                        style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;",
                                                                        span { style: "font-size:11.5px;color:{ink3};", if is_doc { "文档" } else { "全流程" } }
                                                                        div {
                                                                            style: "display:flex;gap:4px;",
                                                                            button {
                                                                                style: if is_doc {
                                                                                    "cursor:pointer;background:transparent;border:1px solid {theme::BORDER};color:{ink3};border-radius:6px;padding:2px 10px;font-size:10.5px;"
                                                                                } else {
                                                                                    "cursor:pointer;background:{theme::CLAY};border:1px solid {theme::CLAY};color:#FFF;border-radius:6px;padding:2px 10px;font-size:10.5px;"
                                                                                },
                                                                                onclick: move |_| { doc_view.write().remove(&row_id); },
                                                                                "流程图"
                                                                            }
                                                                            button {
                                                                                style: if is_doc {
                                                                                    "cursor:pointer;background:{theme::CLAY};border:1px solid {theme::CLAY};color:#FFF;border-radius:6px;padding:2px 10px;font-size:10.5px;"
                                                                                } else {
                                                                                    "cursor:pointer;background:transparent;border:1px solid {theme::BORDER};color:{ink3};border-radius:6px;padding:2px 10px;font-size:10.5px;"
                                                                                },
                                                                                onclick: move |_| { doc_view.write().insert(row_id); },
                                                                                "文档"
                                                                            }
                                                                        }
                                                                    }
                                                                    div {
                                                                        style: "margin-bottom:10px;",
                                                                        if is_doc {
                                                                            MarkdownView {
                                                                                content: row.content.clone(),
                                                                                empty_label: "结构化定义,无原始文档".to_string(),
                                                                            }
                                                                        } else {
                                                                            WorkflowFlow {
                                                                                phases: row.phase_metas.clone(),
                                                                                loop_retries: row.loop_retries,
                                                                                loop_max_iter: row.loop_max_iter,
                                                                                agents: agents_pool.clone(),
                                                                                skills: skills_pool.clone(),
                                                                                on_select,
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if let Some(d) = &detail {
                                                                if !d.agents.is_empty() {
                                                                    div { style: "font-size:11.5px;color:{ink3};margin:10px 0 6px;", "涉及智能体 · 悬停查看角色" }
                                                                    for (i , (name , def , _from)) in d.agents.iter().enumerate() {
                                                                        span {
                                                                            key: "ag{i}",
                                                                            title: "{def}",
                                                                            style: "{theme::chip(\"#EDE8F5\", theme::AGENT)} margin-right:6px;",
                                                                            "◆ {name}"
                                                                        }
                                                                    }
                                                                }
                                                                if !d.skills.is_empty() {
                                                                    div { style: "font-size:11.5px;color:{ink3};margin:10px 0 6px;", "涉及技能 · 悬停查看效果" }
                                                                    for (i , (name , def , _from)) in d.skills.iter().enumerate() {
                                                                        span {
                                                                            key: "sk{i}",
                                                                            title: "{def}",
                                                                            style: "{theme::chip(\"#EFE9DA\", ink2)} margin-right:6px;",
                                                                            "🧩 {name}"
                                                                        }
                                                                    }
                                                                }
                                                            } else if !row.skills.is_empty() {
                                                                div { style: "font-size:11.5px;color:{ink3};margin:10px 0 6px;", "涉及技能" }
                                                                for (i , s) in row.skills.iter().enumerate() {
                                                                    span { key: "{i}", style: "{theme::chip(\"#EFE9DA\", ink2)} margin-right:6px;", "{s}" }
                                                                }
                                                            }
                                                            div {
                                                                style: "display:flex;align-items:center;gap:10px;margin-top:12px;flex-wrap:wrap;",
                                                                if picker_open {
                                                                    select {
                                                                        style: "{theme::input()} width:auto;",
                                                                        onchange: move |e| {
                                                                            if let Ok(i) = e.value().parse::<usize>() {
                                                                                import_target.set(i);
                                                                            }
                                                                        },
                                                                        for (i , p) in projects.iter().enumerate() {
                                                                            option { value: "{i}", "{p.name}" }
                                                                        }
                                                                    }
                                                                    button {
                                                                        style: "{theme::btn_primary()} padding:6px 14px;font-size:12px;",
                                                                        onclick: move |_| {
                                                                            if let Some(target) = projects.get(import_target()) {
                                                                                let session = SessionId::new();
                                                                                k.send(Command::OpenProject(target.id));
                                                                                k.send(Command::StartSession {
                                                                                    id: session,
                                                                                    stage_kind: stage_ref
                                                                                        .and_then(|n| StageKind::ALL.into_iter().find(|s| s.index() == n)),
                                                                                    kind: SessionKind::Create,
                                                                                    title: format!("{row_name} · 导入"),
                                                                                });
                                                                                k.send(Command::RunHubWorkflow { session, workflow_id: row_id });
                                                                                k.send(Command::SelectSession(Some(session)));
                                                                                importing.set(None);
                                                                                on_run.call(());
                                                                            }
                                                                        },
                                                                        "确认导入 · 运行"
                                                                    }
                                                                    button {
                                                                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {theme::BORDER};border-radius:7px;padding:6px 12px;font-size:12px;",
                                                                        onclick: move |_| importing.set(None),
                                                                        "取消"
                                                                    }
                                                                } else {
                                                                    button {
                                                                        style: "{theme::btn_primary()} padding:6px 14px;font-size:12px;",
                                                                        disabled: projects.is_empty(),
                                                                        onclick: move |_| {
                                                                            importing.set(Some(row_id));
                                                                            optimizing.set(None);
                                                                        },
                                                                        "导入到项目 →"
                                                                    }
                                                                    button {
                                                                        style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 12px;font-size:12px;",
                                                                        onclick: move |_| {
                                                                            optimizing.set(Some(row_id));
                                                                            importing.set(None);
                                                                        },
                                                                        "优化 →"
                                                                    }
                                                                    if cron_added().contains(&row_id) {
                                                                        span { style: "font-size:11.5px;color:{ink3};", "✓ 已加入 Cron Hub · 每周" }
                                                                    } else {
                                                                        button {
                                                                            style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {theme::BORDER};border-radius:7px;padding:6px 12px;font-size:12px;",
                                                                            onclick: move |_| {
                                                                                k.send(Command::CreateCronTask {
                                                                                    id: CronTaskId::new(),
                                                                                    name: format!("{row_name} · 定时执行"),
                                                                                    target: row_name.clone(),
                                                                                    schedule: Cadence::Weekly,
                                                                                    project_id: None,
                                                                                });
                                                                                cron_added.write().insert(row_id);
                                                                            },
                                                                            "设为定时任务 →"
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Shared by create/optimize/ad-hoc forms — toggle real Skill/AgentHub
/// entries into a workflow's `agents`/`skills`. Selection is by name (these
/// are free-text `AgentRef`/`SkillRef`, not hard FKs, matching how the rest
/// of the hub already references them).
#[component]
fn SkillAgentPicker(
    skills: Vec<SkillCardVm>,
    agents: Vec<AgentCardVm>,
    selected_skills: Signal<HashSet<String>>,
    selected_agents: Signal<HashSet<String>>,
) -> Element {
    let mut selected_skills = selected_skills;
    let mut selected_agents = selected_agents;
    let ink3 = theme::INK_3;
    let mut filter = use_signal(String::new);
    let f = filter().to_lowercase();
    let shown_skills: Vec<SkillCardVm> = skills
        .iter()
        .filter(|s| f.is_empty() || s.name.to_lowercase().contains(&f))
        .take(60)
        .cloned()
        .collect();
    let shown_agents: Vec<AgentCardVm> = agents
        .iter()
        .filter(|a| f.is_empty() || a.name.to_lowercase().contains(&f))
        .take(60)
        .cloned()
        .collect();
    let picked = selected_skills().len() + selected_agents().len();

    rsx! {
        div {
            div { style: "{theme::label()}", "涉及技能 / 智能体(可选 · 输入筛选 · 点击切换)" }
            input {
                style: "{theme::input()} margin-bottom:8px;",
                placeholder: "筛选技能/智能体名称…",
                value: "{filter}",
                oninput: move |e| filter.set(e.value()),
            }
            div {
                style: "display:flex;flex-wrap:wrap;gap:5px;max-height:120px;overflow-y:auto;padding:8px;background:{theme::CARD_ALT};border-radius:7px;margin-bottom:6px;",
                if shown_skills.is_empty() && shown_agents.is_empty() {
                    span { style: "font-size:11.5px;color:{ink3};", "没有匹配的技能/智能体" }
                }
                for s in shown_skills {
                    {
                        let name = s.name.clone();
                        let toggle_name = name.clone();
                        let active = selected_skills().contains(&name);
                        let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", theme::INK_2) };
                        rsx! {
                            span {
                                key: "sk-{name}",
                                title: "{s.desc}",
                                style: "{theme::chip(bg, fg)} cursor:pointer;",
                                onclick: move |_| {
                                    selected_skills.with_mut(|set| {
                                        if !set.remove(&toggle_name) {
                                            set.insert(toggle_name.clone());
                                        }
                                    });
                                },
                                "🧩 {name}"
                            }
                        }
                    }
                }
                for a in shown_agents {
                    {
                        let name = a.name.clone();
                        let toggle_name = name.clone();
                        let active = selected_agents().contains(&name);
                        let (bg, fg): (&str, &str) = if active { (theme::AGENT, "#FFF") } else { ("#EFE9DA", theme::INK_2) };
                        rsx! {
                            span {
                                key: "ag-{name}",
                                title: "{a.role}",
                                style: "{theme::chip(bg, fg)} cursor:pointer;",
                                onclick: move |_| {
                                    selected_agents.with_mut(|set| {
                                        if !set.remove(&toggle_name) {
                                            set.insert(toggle_name.clone());
                                        }
                                    });
                                },
                                "◆ {name}"
                            }
                        }
                    }
                }
            }
            if picked > 0 {
                div { style: "font-size:11px;color:{ink3};margin-bottom:10px;", "已选 {picked} 项" }
            }
        }
    }
}

fn resolve_refs(
    skills: &[SkillCardVm],
    agents: &[AgentCardVm],
    selected_skills: &HashSet<String>,
    selected_agents: &HashSet<String>,
) -> (Vec<AgentRef>, Vec<SkillRef>) {
    let agent_refs = agents
        .iter()
        .filter(|a| selected_agents.contains(&a.name))
        .map(|a| AgentRef {
            name: a.name.clone(),
            def: a.role.clone(),
            from: "AgentHub".into(),
        })
        .collect();
    let skill_refs = skills
        .iter()
        .filter(|s| selected_skills.contains(&s.name))
        .map(|s| SkillRef {
            name: s.name.clone(),
            def: s.desc.clone(),
            from: "SkillHub".into(),
        })
        .collect();
    (agent_refs, skill_refs)
}

#[component]
fn CreateWorkflowForm(
    skills: Vec<SkillCardVm>,
    agents: Vec<AgentCardVm>,
    on_done: EventHandler<()>,
) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;

    let mut name = use_signal(String::new);
    let mut prompt = use_signal(String::new);
    let mut goal = use_signal(String::new);
    let mut phases_text = use_signal(String::new);
    let mut trigger = use_signal(String::new);
    let mut stage_ref = use_signal(|| None::<StageKind>);
    let mut selected_skills = use_signal(HashSet::<String>::new);
    let mut selected_agents = use_signal(HashSet::<String>::new);

    let skills_for_save = skills.clone();
    let agents_for_save = agents.clone();
    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        let phases: Vec<String> = phases_text()
            .split(['→', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let trig = trigger().trim().to_string();
        let (agent_refs, skill_refs) = resolve_refs(
            &skills_for_save,
            &agents_for_save,
            &selected_skills(),
            &selected_agents(),
        );
        k.send(Command::CreateWorkflowSpec {
            id: WorkflowId::new(),
            name: n,
            prompt: prompt().trim().to_string(),
            goal: goal().trim().to_string(),
            stage_ref: stage_ref().map(|s| s.index()),
            phases,
            // The hub's create form authors a single shared prompt; per-phase
            // playbook prompts come from `stage_workflow_with_playbook` runs.
            phase_prompts: vec![],
            agents: agent_refs,
            skills: skill_refs,
            loop_config: LoopConfig {
                retries: 1,
                max_iter: 3,
            },
            maturity: bw_core::model::Maturity::Polishing,
            scope: String::new(),
            source: bw_core::model::HubSource::SelfBuilt,
            trigger: if trig.is_empty() { None } else { Some(trig) },
        });
        name.set(String::new());
        prompt.set(String::new());
        goal.set(String::new());
        phases_text.set(String::new());
        trigger.set(String::new());
        selected_skills.write().clear();
        selected_agents.write().clear();
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div {
                style: "display:grid;grid-template-columns:1.4fr 1fr;gap:12px;margin-bottom:10px;",
                div {
                    div { style: "{label}", "名称" }
                    input {
                        style: "{input}",
                        placeholder: "如 深度访谈 → 问题定义",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div {
                    div { style: "{label}", "关联阶段(可选)" }
                    select {
                        style: "{input}",
                        onchange: move |e| {
                            stage_ref.set(StageKind::ALL.into_iter().find(|s| s.label() == e.value()));
                        },
                        option { value: "", "不关联特定阶段(指标层)" }
                        for sk in StageKind::ALL {
                            option { key: "{sk.index()}", value: "{sk.label()}", "{sk.label()}" }
                        }
                    }
                }
            }
            div { style: "{label}", "Prompt(任务定义)" }
            textarea {
                style: "{input} min-height:60px;margin-bottom:10px;",
                value: "{prompt}",
                oninput: move |e| prompt.set(e.value()),
            }
            div { style: "{label}", "验收目标" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{goal}",
                oninput: move |e| goal.set(e.value()),
            }
            div { style: "{label}", "阶段流程(用「→」或逗号分隔)" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 访谈提纲 → 深挖场景 → 矛盾识别",
                value: "{phases_text}",
                oninput: move |e| phases_text.set(e.value()),
            }
            div { style: "{label}", "触发词(可选)" }
            input {
                style: "{input} margin-bottom:12px;",
                placeholder: "如 /security-review",
                value: "{trigger}",
                oninput: move |e| trigger.set(e.value()),
            }
            SkillAgentPicker { skills, agents, selected_skills, selected_agents }
            div {
                style: "display:flex;align-items:center;gap:10px;",
                button {
                    style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                    onclick: save,
                    "保存"
                }
                span { style: "font-size:11.5px;color:{ink3};", "新建的工作流默认「打磨中」· v1 · 0 次复用。" }
            }
        }
    }
}

/// "优化" an existing **Static** hub workflow in place — prefilled from its
/// real `WorkflowDetailVm`, dispatches `Command::UpdateWorkflowSpec` (bumps
/// `version`; `uses`/`maturity`/`source` are untouched server-side).
#[component]
fn OptimizeWorkflowForm(
    skills: Vec<SkillCardVm>,
    agents: Vec<AgentCardVm>,
    detail: WorkflowDetailVm,
    on_done: EventHandler<()>,
) -> Element {
    let k = use_context::<Kernel>();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;
    let workflow_id = detail.row.id;

    let mut prompt = use_signal(|| detail.prompt.clone());
    let mut goal = use_signal(|| detail.row.goal.clone());
    let mut phases_text = use_signal(|| detail.row.phases.join(" → "));
    let selected_skills = use_signal(|| {
        detail
            .skills
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect::<HashSet<_>>()
    });
    let selected_agents = use_signal(|| {
        detail
            .agents
            .iter()
            .map(|(name, _, _)| name.clone())
            .collect::<HashSet<_>>()
    });

    let skills_for_save = skills.clone();
    let agents_for_save = agents.clone();
    let save = move |_| {
        let phases: Vec<String> = phases_text()
            .split(['→', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let (agent_refs, skill_refs) = resolve_refs(
            &skills_for_save,
            &agents_for_save,
            &selected_skills(),
            &selected_agents(),
        );
        k.send(Command::UpdateWorkflowSpec {
            id: workflow_id,
            prompt: prompt().trim().to_string(),
            goal: goal().trim().to_string(),
            phases,
            // This form edits a single shared prompt — saving through it
            // honestly reverts the spec to shared-prompt mode (the version
            // snapshot has already frozen any per-phase prompts it had; a
            // per-phase editor is P3 UI work).
            phase_prompts: vec![],
            agents: agent_refs,
            skills: skill_refs,
            note: String::new(),
        });
        on_done.call(());
    };

    rsx! {
        div {
            style: "background:{theme::CARD_ALT};border:1px solid {theme::BORDER_DEEP};border-radius:9px;padding:14px 16px;",
            div { style: "font-size:12px;color:{theme::CLAY};margin-bottom:10px;font-weight:600;", "优化「{detail.row.name}」→ 保存后 {detail.row.version_label} 变为下一版" }
            div { style: "{label}", "Prompt(任务定义)" }
            textarea {
                style: "{input} min-height:60px;margin-bottom:10px;",
                value: "{prompt}",
                oninput: move |e| prompt.set(e.value()),
            }
            div { style: "{label}", "验收目标" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{goal}",
                oninput: move |e| goal.set(e.value()),
            }
            div { style: "{label}", "阶段流程(用「→」或逗号分隔)" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{phases_text}",
                oninput: move |e| phases_text.set(e.value()),
            }
            SkillAgentPicker { skills, agents, selected_skills, selected_agents }
            div {
                style: "display:flex;align-items:center;gap:10px;",
                button {
                    style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                    onclick: save,
                    "保存优化"
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

/// The "dynamic workflow creation" surface: author a one-off `WorkflowKind::
/// Dynamic` spec (prompt/phases/crew, no hub entry) and run it for real
/// against a chosen project — the same real `Command::RunWorkflow` path
/// `WorkflowStage`'s "▶ 运行" already uses for the built-in stage template,
/// just with a user-authored spec instead of `stage_workflow(kind)`. Once it
/// runs, the session's normal "沉淀为静态工作流" action (in the operating
/// view) is the "运维" half of this loop — promote it, or let it stay a
/// one-off.
#[component]
fn AdHocWorkflowForm(
    skills: Vec<SkillCardVm>,
    agents: Vec<AgentCardVm>,
    projects: Vec<ProjectCardVm>,
    on_run: EventHandler<()>,
) -> Element {
    let k = use_context::<Kernel>();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;

    let mut name = use_signal(String::new);
    let mut prompt = use_signal(String::new);
    let mut goal = use_signal(String::new);
    let mut phases_text = use_signal(String::new);
    let mut project_idx = use_signal(|| 0usize);
    let mut stage_ref = use_signal(|| None::<StageKind>);
    let mut selected_skills = use_signal(HashSet::<String>::new);
    let mut selected_agents = use_signal(HashSet::<String>::new);

    let skills_for_run = skills.clone();
    let agents_for_run = agents.clone();
    let projects_for_run = projects.clone();
    let run = move |_| {
        let n = name().trim().to_string();
        let p = prompt().trim().to_string();
        if n.is_empty() || p.is_empty() {
            return;
        }
        let Some(target) = projects_for_run
            .get(project_idx())
            .map(|p: &ProjectCardVm| p.id)
        else {
            return;
        };
        let mut phases: Vec<String> = phases_text()
            .split(['→', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if phases.is_empty() {
            phases.push("执行".into());
        }
        let (agent_refs, skill_refs) = resolve_refs(
            &skills_for_run,
            &agents_for_run,
            &selected_skills(),
            &selected_agents(),
        );
        let kind = stage_ref();
        let spec = WorkflowSpec {
            id: WorkflowId::new(),
            name: n.clone(),
            kind: WorkflowKind::Dynamic {
                origin: "临时任务".into(),
                stage: kind
                    .map(|s| s.label().to_string())
                    .unwrap_or_else(|| "指标层".into()),
            },
            prompt: p,
            goal: goal().trim().to_string(),
            stage_ref: kind.map(|s| s.index()),
            // Ad-hoc text form — no role-editing UI, so every phase is
            // honestly `Neutral` (same rule as `CreateWorkflowSpec`/
            // `UpdateWorkflowSpec`'s handlers in bw-app).
            phases: phases.into_iter().map(PhaseMeta::neutral).collect(),
            phase_prompts: vec![],
            agents: agent_refs,
            skills: skill_refs,
            loop_config: LoopConfig {
                retries: 1,
                max_iter: 1,
            },
            // 临时任务不进 Hub 库(见下方 UI 文案),这个字段对持久化查询没有
            // 意义——但它确实是为 `target` 这个项目跑的,如实标注。
            project_id: Some(target),
            // 临时任务文本表单没有正文录入——如实留空。
            content: String::new(),
        };
        let session = SessionId::new();
        k.send(Command::OpenProject(target));
        k.send(Command::StartSession {
            id: session,
            stage_kind: kind,
            kind: SessionKind::Create,
            title: format!("⚡ {n}"),
        });
        k.send(Command::RunWorkflow { session, spec });
        k.send(Command::SelectSession(Some(session)));
        name.set(String::new());
        prompt.set(String::new());
        goal.set(String::new());
        phases_text.set(String::new());
        selected_skills.write().clear();
        selected_agents.write().clear();
        on_run.call(());
    };

    rsx! {
        div {
            style: "background:{theme::CARD_ALT};border:1px dashed {theme::BORDER_DEEP};border-radius:9px;padding:14px 16px;margin-bottom:16px;",
            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;line-height:1.6;",
                "临时任务是一次性的动态工作流(WorkflowKind::Dynamic)——不进入库,跑完只留在会话记录里。\
                 觉得好用,可在运行结果里点「沉淀为静态工作流」升格进 WorkflowHub。"
            }
            div {
                style: "display:grid;grid-template-columns:1.4fr 1fr;gap:12px;margin-bottom:10px;",
                div {
                    div { style: "{label}", "名称" }
                    input {
                        style: "{input}",
                        placeholder: "如 排查一次性能回退",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div {
                    div { style: "{label}", "在哪个项目跑" }
                    select {
                        style: "{input}",
                        disabled: projects.is_empty(),
                        onchange: move |e| {
                            if let Ok(i) = e.value().parse::<usize>() {
                                project_idx.set(i);
                            }
                        },
                        for (i , p) in projects.iter().enumerate() {
                            option { key: "{i}", value: "{i}", "{p.name}" }
                        }
                    }
                }
            }
            div { style: "{label}", "关联阶段(可选)" }
            select {
                style: "{input} margin-bottom:10px;",
                onchange: move |e| {
                    stage_ref.set(StageKind::ALL.into_iter().find(|s| s.label() == e.value()));
                },
                option { value: "", "不关联特定阶段" }
                for sk in StageKind::ALL {
                    option { key: "{sk.index()}", value: "{sk.label()}", "{sk.label()}" }
                }
            }
            div { style: "{label}", "Prompt(这次要做什么)" }
            textarea {
                style: "{input} min-height:60px;margin-bottom:10px;",
                value: "{prompt}",
                oninput: move |e| prompt.set(e.value()),
            }
            div { style: "{label}", "验收目标(可选)" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{goal}",
                oninput: move |e| goal.set(e.value()),
            }
            div { style: "{label}", "步骤(用「→」或逗号分隔,留空默认单步「执行」)" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 复现 → 定位 → 修复 → 验证",
                value: "{phases_text}",
                oninput: move |e| phases_text.set(e.value()),
            }
            SkillAgentPicker { skills, agents, selected_skills, selected_agents }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                disabled: projects.is_empty(),
                onclick: run,
                "▶ 立即运行"
            }
        }
    }
}
