//! **The real desktop shell (P2).** Replaces the P0 throwaway ramp app.
//!
//! One rule: this crate renders and forwards intent — it computes nothing.
//! State lives in the kernel thread ([`kernel`]); every pixel of "truth"
//! (signals, trends, feeds, transcripts) arrives pre-derived in the [`Vm`].

#![forbid(unsafe_code)]

mod kernel;
mod screens;
mod templates;
mod theme;

use bw_app::{Command, View};
use bw_core::ProjectId;
use dioxus::prelude::*;
use kernel::{RunVm, UiNote, Vm};
use screens::chrome::{BootFrame, FatalFrame, Hub, HubStub, IconRail, Toast};
use screens::op::Op;
use screens::wall::Wall;
use screens::wizard::Wizard;
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
    let mut hub = use_signal(|| Hub::Workspace);
    let mut creating = use_signal(|| false);
    let mut toast = use_signal(|| None::<String>);
    let mut run = use_signal(RunVm::default);

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

    // Transient notes: live run progress + dispatch errors.
    use_future({
        let kernel = kernel.clone();
        move || {
            let mut rx = kernel.notes();
            async move {
                loop {
                    match rx.recv().await {
                        Ok(note) => match &note {
                            UiNote::Error(e) => toast.set(Some(e.clone())),
                            UiNote::RunFailed(e) => {
                                toast.set(Some(format!("工作流失败:{e}")));
                                run.with_mut(|r| r.apply(&note));
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

    let on_start = {
        let kernel = kernel.clone();
        move |(name, kind, desc): (String, String, String)| {
            kernel.send(Command::CreateProject {
                id: ProjectId::new(),
                name,
                kind,
                desc,
            });
            kernel.send(Command::SetWizardStep { step: 1 });
            creating.set(false);
        }
    };

    let show_wizard = creating() || v.view == View::Wizard;
    let wizard_vm = if v.view == View::Wizard {
        v.wizard.clone()
    } else {
        None
    };
    let show_op = !show_wizard && v.view == View::App;

    rsx! {
        GlobalChrome {}
        div {
            style: "display:flex;height:100vh;background:{paper};color:{ink};font-family:{sans};font-size:14px;overflow:hidden;",
            IconRail { hub: hub(), on_pick: move |h| hub.set(h) }
            div {
                style: "flex:1;min-width:0;height:100vh;overflow-y:auto;",
                if !v.ready {
                    BootFrame {}
                } else if v.fatal.is_some() {
                    FatalFrame { msg: v.fatal.clone().unwrap_or_default() }
                } else if hub() != Hub::Workspace {
                    HubStub { hub: hub() }
                } else if show_wizard {
                    Wizard {
                        vm: wizard_vm,
                        on_start,
                        on_cancel: move |_| creating.set(false),
                    }
                } else if show_op {
                    if v.op.is_some() {
                        Op { op: v.op.clone().unwrap(), run: run() }
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
