//! **The real desktop shell (P2).** Replaces the P0 throwaway ramp app.
//!
//! One rule: this crate renders and forwards intent — it computes nothing.
//! State lives in the kernel thread ([`kernel`]); every pixel of "truth"
//! (signals, trends, feeds, transcripts) arrives pre-derived in the [`Vm`].

#![forbid(unsafe_code)]

mod kernel;
mod screens;
mod theme;

use bw_app::{Command, View};
use bw_core::model::{CronStatus, HubKind, StageKind};
use bw_core::{CronTaskId, SessionId};
use bw_store::SessionKind;
use dioxus::prelude::*;
use kernel::{RunVm, UiNote, Vm};
use screens::activity_hub::ActivityHub;
use screens::agent_hub::AgentHub;
use screens::chrome::{BootFrame, FatalFrame, Hub, IconRail, Toast};
use screens::component_detail::{ComponentDetail, ComponentSel};
use screens::connector_hub::ConnectorHub;
use screens::create::Create;
use screens::cron_hub::CronHub;
use screens::knowledge_hub::KnowledgeHub;
use screens::notify_hub::NotifyHub;
use screens::op::Op;
use screens::project_rail::ProjectRail;
use screens::settings_hub::SettingsHub;
use screens::skill_hub::SkillHub;
use screens::wall::Wall;
use screens::workflow_hub::WorkflowHub;
use tokio::sync::broadcast::error::RecvError;

fn main() {
    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::new().with_window(
                dioxus::desktop::WindowBuilder::new()
                    .with_title("Builders' Workbench")
                    .with_inner_size(dioxus::desktop::LogicalSize::new(1440.0, 920.0)),
            ),
        )
        .launch(Root);
}

/// Static head elements. A prop-less child never re-renders, so the Style
/// node is created exactly once (diffing `document::Style` is unsupported).
#[component]
fn GlobalChrome() -> Element {
    rsx! {
        document::Title { "Builders' Workbench" }
        document::Style { {theme::GLOBAL_CSS} }
    }
}

#[component]
fn Root() -> Element {
    let kernel = use_context_provider(kernel::spawn);
    let mut vm = use_signal(Vm::default);
    // BW_HUB=skill|agent|workflow|cron|connector|knowledge|activity|notify|settings
    // deep-links straight to a rail Hub screen — same verification discipline
    // as BW_OPEN/BW_PANEL (CLAUDE.md), extended because those two only reach
    // per-project panels; the marketplace Hub screens are rail-click-only and
    // were otherwise unreachable without computer-use.
    let initial_hub = std::env::var("BW_HUB")
        .ok()
        .and_then(|v| match v.as_str() {
            "skill" => Some(Hub::Skill),
            "agent" => Some(Hub::Agent),
            "workflow" => Some(Hub::Workflow),
            "cron" => Some(Hub::Cron),
            "connector" => Some(Hub::Connector),
            "knowledge" => Some(Hub::Knowledge),
            "activity" => Some(Hub::Activity),
            "notify" => Some(Hub::Notify),
            "settings" => Some(Hub::Settings),
            _ => None,
        })
        .unwrap_or(Hub::Workspace);
    if let Ok(v) = std::env::var("BW_HUB") {
        eprintln!("[BW_HUB] {v:?} -> {initial_hub:?}");
    }
    let mut hub = use_signal(|| initial_hub);
    // L1(plan/11): the project rail's open component detail, `None` when
    // closed. Cleared whenever the icon rail navigates to a marketplace hub
    // (visiting the full catalog and viewing one project's own component
    // detail are two different intents — one shouldn't leave the other
    // stuck open underneath it).
    // BW_SEL=skill:<uuid>|agent:<uuid>|workflow:<uuid>|cron:<uuid>|connector:<uuid>
    // deep-links straight into a `ComponentDetail` — same verification
    // discipline as BW_HUB, extended because a rail click's resulting `sel`
    // state is pure client state with no store trace to read back.
    let initial_sel = std::env::var("BW_SEL").ok().and_then(|v| {
        let (kind, id_str) = v.split_once(':')?;
        let uuid = uuid::Uuid::parse_str(id_str).ok()?;
        match kind {
            "skill" => Some(ComponentSel::Skill(bw_core::SkillId::from_uuid(uuid))),
            "agent" => Some(ComponentSel::Agent(bw_core::AgentId::from_uuid(uuid))),
            "workflow" => Some(ComponentSel::Workflow(bw_core::WorkflowId::from_uuid(uuid))),
            "cron" => Some(ComponentSel::Cron(bw_core::CronTaskId::from_uuid(uuid))),
            "connector" => Some(ComponentSel::Connector(bw_core::ConnectorId::from_uuid(
                uuid,
            ))),
            _ => None,
        }
    });
    if let Ok(v) = std::env::var("BW_SEL") {
        eprintln!("[BW_SEL] {v:?} -> {initial_sel:?}");
    }
    let mut sel = use_signal(move || initial_sel);
    let mut creating = use_signal(|| false);
    let mut toast = use_signal(|| None::<String>);
    let mut run = use_signal(RunVm::default);
    // Set right before a Cron Hub "▶ 立即执行" trigger fires; consumed (and
    // cleared) by the notes listener below once that run's real `RunDone`/
    // `RunFailed` arrives, closing the loop with a real `MarkCronRun`. Lives
    // here (not in `RunVm`) because "which cron task triggered this" is
    // client-side orchestration knowledge the kernel doesn't have.
    let mut pending_cron = use_signal(|| None::<CronTaskId>);

    // Latest kernel snapshot → the one rendering source of truth.
    use_future({
        let kernel = kernel.clone();
        move || {
            let mut rx = kernel.vm();
            async move {
                let first = rx.borrow().clone();
                vm.set(first);
                while rx.changed().await.is_ok() {
                    let next = rx.borrow().clone();
                    vm.set(next);
                }
            }
        }
    });

    // Transient notes: live run progress + dispatch errors. Also the one
    // place that closes the Cron Hub trigger loop: if this run was fired by
    // "▶ 立即执行" (`pending_cron` is set), the real `RunDone`/`RunFailed`
    // that ends it becomes a real `Command::MarkCronRun` with the real
    // outcome — never optimistically marked at trigger time.
    use_future({
        let kernel = kernel.clone();
        move || {
            let mut rx = kernel.notes();
            let kernel = kernel.clone();
            async move {
                loop {
                    match rx.recv().await {
                        Ok(note) => match &note {
                            UiNote::Error(e) => toast.set(Some(e.clone())),
                            UiNote::RunFailed(e) => {
                                toast.set(Some(format!("工作流失败:{e}")));
                                run.with_mut(|r| r.apply(&note));
                                if let Some(cid) = pending_cron() {
                                    kernel.send(Command::MarkCronRun {
                                        id: cid,
                                        status: CronStatus::Failed,
                                    });
                                    pending_cron.set(None);
                                }
                            }
                            UiNote::RunDone => {
                                run.with_mut(|r| r.apply(&note));
                                if let Some(cid) = pending_cron() {
                                    kernel.send(Command::MarkCronRun {
                                        id: cid,
                                        status: CronStatus::Normal,
                                    });
                                    pending_cron.set(None);
                                }
                            }
                            // A real, unattended scheduler fire (not this
                            // click-driven `pending_cron` flow at all) — a
                            // toast, deliberately never a navigation. See
                            // `App::tick_scheduler`'s own doc comment for why
                            // it must not touch the user's current screen.
                            UiNote::CronAutoFired { name, ok } => {
                                let mark = if *ok { "✓" } else { "✕" };
                                toast.set(Some(format!("⏰ 定时任务自动运行 {mark} · {name}")));
                            }
                            UiNote::ArtifactsRegistered { fresh } => {
                                toast.set(Some(format!("📦 新登记 {fresh} 个产物版本")));
                            }
                            UiNote::ConnectorSynced { name, ok, detail } => {
                                let mark = if *ok { "✓" } else { "✕" };
                                toast.set(Some(format!("🔌 {name} 同步 {mark} · {detail}")));
                            }
                            _ => run.with_mut(|r| r.apply(&note)),
                        },
                        Err(RecvError::Lagged(_)) => continue,
                        Err(RecvError::Closed) => break,
                    }
                }
            }
        }
    });

    let v = vm();
    let paper = theme::PAPER;
    let ink = theme::INK;
    let sans = theme::SANS;

    // `creating` is a one-shot local bridge for the gap between clicking "+
    // 新建项目" and the kernel confirming `CreateProject`. Once the kernel
    // catches up, drop the override so a later `BackToProjects`/
    // `CompleteCreation` isn't stuck showing Create forever.
    if creating() && v.view == View::Create {
        creating.set(false);
    }
    let show_create = creating() || v.view == View::Create;
    let show_op = !show_create && v.view == View::App;

    // Cron Hub's "▶ 立即执行": resolve the real project + workflow from this
    // render's hub snapshot, dispatch the exact same real Command sequence
    // WorkflowHub's "确认导入" uses, mark the task Running for real, then
    // navigate to go watch it (same as any other real run).
    let hub_for_cron = v.hub.clone();
    let kernel_for_cron = kernel.clone();
    let on_trigger_cron = move |cron_id: CronTaskId| {
        let Some(c) = hub_for_cron.cron_tasks.iter().find(|x| x.id == cron_id) else {
            return;
        };
        let Some(pid) = c.project_id else {
            return;
        };
        let Some(wf) = hub_for_cron.workflows.iter().find(|w| w.name == c.target) else {
            return;
        };
        let session = SessionId::new();
        kernel_for_cron.send(Command::OpenProject(pid));
        kernel_for_cron.send(Command::StartSession {
            id: session,
            stage_kind: wf
                .stage_ref
                .and_then(|n| StageKind::ALL.into_iter().find(|s| s.index() == n)),
            kind: SessionKind::Optimize,
            title: format!("⏰ 定时触发 · {}", c.name),
        });
        kernel_for_cron.send(Command::MarkCronRun {
            id: cron_id,
            status: CronStatus::Running,
        });
        kernel_for_cron.send(Command::RunHubWorkflow {
            session,
            workflow_id: wf.id,
        });
        kernel_for_cron.send(Command::SelectSession(Some(session)));
        pending_cron.set(Some(cron_id));
        hub.set(Hub::Workspace);
    };

    rsx! {
        GlobalChrome {}
        div {
            style: "display:flex;height:100vh;background:{paper};color:{ink};font-family:{sans};font-size:14px;overflow:hidden;",
            IconRail {
                hub: hub(),
                on_pick: move |h| {
                    hub.set(h);
                    sel.set(None);
                },
            }
            if v.view == View::App {
                if let Some(op) = v.op.clone() {
                    ProjectRail {
                        project_id: op.id,
                        hub: v.hub.clone(),
                        on_pick: move |s: ComponentSel| {
                            sel.set(Some(s));
                            hub.set(Hub::Workspace);
                        },
                    }
                }
            }
            div {
                style: "flex:1;min-width:0;height:100vh;overflow-y:auto;",
                if !v.ready {
                    BootFrame {}
                } else if v.fatal.is_some() {
                    FatalFrame { msg: v.fatal.clone().unwrap_or_default() }
                } else if hub() == Hub::Workflow {
                    WorkflowHub {
                        hub: v.hub.clone(),
                        projects: v.projects.clone(),
                        on_run: move |_| hub.set(Hub::Workspace),
                    }
                } else if hub() == Hub::Skill {
                    SkillHub { hub: v.hub.clone(), projects: v.projects.clone() }
                } else if hub() == Hub::Agent {
                    AgentHub { hub: v.hub.clone(), projects: v.projects.clone() }
                } else if hub() == Hub::Cron {
                    CronHub {
                        hub: v.hub.clone(),
                        projects: v.projects.clone(),
                        on_trigger: on_trigger_cron,
                    }
                } else if hub() == Hub::Connector {
                    ConnectorHub { hub: v.hub.clone() }
                } else if hub() == Hub::Knowledge {
                    KnowledgeHub { hub: v.hub.clone() }
                } else if hub() == Hub::Activity {
                    ActivityHub { hub: v.hub.clone() }
                } else if hub() == Hub::Notify {
                    NotifyHub { hub: v.hub.clone() }
                } else if hub() == Hub::Settings {
                    SettingsHub { settings: v.settings.clone() }
                } else if hub() == Hub::Workspace && sel().is_some() {
                    ComponentDetail {
                        sel: sel().unwrap(),
                        hub: v.hub.clone(),
                        projects: v.projects.clone(),
                        cron_effectiveness: v.cron_effectiveness.clone(),
                        on_close: move |_| sel.set(None),
                    }
                } else if show_create {
                    Create {
                        vm: v.create.clone(),
                        run: run(),
                        github_repos: v.github_repos.clone(),
                        // C12(plan/14): every card/state of the creation flow
                        // routes its exit here — including after a project row
                        // already exists (kernel `state.view` is already
                        // `View::Create` by then, so flipping the local
                        // `creating` bridge alone wouldn't leave the screen).
                        // `BackToProjects` is the real "回项目墙" semantics:
                        // clears `active_project`/`active_session`, never
                        // touches the store — the project (if minted) stays on
                        // the wall exactly as `Command::CreateProject` left it,
                        // resumable via cold-start `OpenProject`.
                        on_cancel: move |_| {
                            kernel.send(Command::BackToProjects);
                            creating.set(false);
                        },
                    }
                } else if show_op {
                    if v.op.is_some() {
                        Op {
                            op: v.op.clone().unwrap(),
                            run: run(),
                            on_pick_hub: move |hk: HubKind| {
                                hub.set(match hk {
                                    HubKind::Workflow => Hub::Workflow,
                                    HubKind::Skill => Hub::Skill,
                                    HubKind::Agent => Hub::Agent,
                                })
                            },
                        }
                    } else {
                        BootFrame {}
                    }
                } else {
                    Wall {
                        projects: v.projects.clone(),
                        on_new: move |_| creating.set(true),
                    }
                }
            }
        }
        if toast().is_some() {
            Toast {
                msg: toast().unwrap_or_default(),
                onclose: move |_| toast.set(None),
            }
        }
    }
}
