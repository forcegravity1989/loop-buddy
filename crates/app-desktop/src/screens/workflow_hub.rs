//! `Hub::Workflow` — the workflow library: grouped by the 5-stage lifecycle
//! plus a 6th cross-cutting "指标层" bucket, with independent stage/source
//! filter chips, matching the prototype's WorkflowHub (its richest, most
//! fully-realized hub — 50 real sample rows, not a stub). Real store-backed
//! CRUD; "导入到项目" and "设为定时任务" are real actions (the latter's actual
//! scheduling mechanism is Cron Hub's territory, a later round — the button
//! here is an honest placeholder, not a fake toggle).

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::model::StageKind;
use bw_core::{SessionId, WorkflowId};
use bw_store::SessionKind;
use dioxus::prelude::*;
use ui::vm::{ProjectCardVm, WorkflowHubRowVm};

#[component]
pub fn WorkflowHub(hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let k = use_context::<Kernel>();
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let card = theme::card();

    let mut creating = use_signal(|| false);
    let mut stage_filter = use_signal(|| None::<StageKind>);
    let mut source_filter = use_signal(|| None::<&'static str>);
    let mut expanded = use_signal(|| None::<WorkflowId>);
    let mut importing = use_signal(|| None::<WorkflowId>);
    let mut import_target = use_signal(|| 0usize);

    let n = hub.workflows.len();
    let chip_counts = ui::vm::source_chip_counts(&hub.workflows);

    let filtered: Vec<WorkflowHubRowVm> = hub
        .workflows
        .iter()
        .filter(|r| {
            stage_filter()
                .map(|sf| r.stage_ref == Some(sf.index()))
                .unwrap_or(true)
        })
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
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 新建工作流" }
                }
            }
            if creating() {
                CreateWorkflowForm { on_done: move |_| creating.set(false) }
            }

            div {
                style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:8px;",
                {
                    let active = stage_filter().is_none();
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| stage_filter.set(None),
                            "全部阶段"
                        }
                    }
                }
                for sk in StageKind::ALL {
                    {
                        let active = stage_filter() == Some(sk);
                        let (bg, fg): (&str, &str) = if active { (sk.color(), "#FFF") } else { ("#EFE9DA", ink2) };
                        rsx! {
                            button {
                                key: "{sk.index()}",
                                style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                                onclick: move |_| stage_filter.set(Some(sk)),
                                "{sk.label()}"
                            }
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
                                        let projects = projects.clone();
                                        let row_id = row.id;
                                        let is_open = expanded() == Some(row_id);
                                        let picker_open = importing() == Some(row_id);
                                        let stage_ref = row.stage_ref;
                                        let row_name = row.name.clone();
                                        rsx! {
                                            div {
                                                key: "{row_id.uuid()}",
                                                style: "{card} padding:14px 16px;margin-bottom:8px;",
                                                div {
                                                    style: "display:flex;align-items:center;gap:12px;cursor:pointer;",
                                                    onclick: move |_| expanded.set(if is_open { None } else { Some(row_id) }),
                                                    span { style: "font-size:13px;font-weight:500;flex:1;min-width:0;", "{row.name}" }
                                                    span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{row.source_label}" }
                                                    span { style: "{theme::chip(\"#EFE9DA\", ink2)}", "{row.maturity_label}" }
                                                    if let Some(t) = &row.trigger {
                                                        span { style: "{theme::chip(\"#F4F0E7\", theme::CLAY)} font-family:{mono};", "{t}" }
                                                    }
                                                    span { style: "font-size:11.5px;color:{ink3};", "{row.primary_agent}" }
                                                    span { style: "font-family:{mono};font-size:11.5px;color:{ink3};", "{row.version_label} · {row.uses} 次复用" }
                                                }
                                                div { style: "font-size:12px;color:{ink2};margin-top:6px;", "验收:{row.goal}" }
                                                if is_open {
                                                    div {
                                                        style: "margin-top:12px;padding-top:12px;border-top:1px dashed {theme::BORDER};",
                                                        div { style: "font-size:11.5px;color:{ink3};margin-bottom:6px;", "方法循环" }
                                                        for (i , p) in row.phases.iter().enumerate() {
                                                            span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", ink2)} margin-right:6px;", "{i + 1}. {p}" }
                                                        }
                                                        if !row.skills.is_empty() {
                                                            div { style: "font-size:11.5px;color:{ink3};margin:10px 0 6px;", "涉及技能" }
                                                            for (i , s) in row.skills.iter().enumerate() {
                                                                span { key: "{i}", style: "{theme::chip(\"#EFE9DA\", ink2)} margin-right:6px;", "{s}" }
                                                            }
                                                        }
                                                        div { style: "font-size:11.5px;color:{ink3};margin-top:10px;", "{row.loop_label}" }
                                                        div {
                                                            style: "display:flex;align-items:center;gap:10px;margin-top:12px;",
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
                                                                            importing.set(None);
                                                                        }
                                                                    },
                                                                    "确认导入"
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
                                                                    onclick: move |_| importing.set(Some(row_id)),
                                                                    "导入到项目 →"
                                                                }
                                                                span { style: "font-size:11.5px;color:{ink3};", "设为定时任务 · 属 Cron Hub · 后续轮次" }
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

#[component]
fn CreateWorkflowForm(on_done: EventHandler<()>) -> Element {
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
        k.send(Command::CreateWorkflowSpec {
            id: WorkflowId::new(),
            name: n,
            prompt: prompt().trim().to_string(),
            goal: goal().trim().to_string(),
            stage_ref: stage_ref().map(|s| s.index()),
            phases,
            agents: vec![],
            skills: vec![],
            loop_config: bw_core::model::LoopConfig {
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
