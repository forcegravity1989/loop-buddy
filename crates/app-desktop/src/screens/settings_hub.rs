//! `Hub::Settings` — the real, process-wide `ClaudeCliConfig` (today: the
//! `claude` binary override only). No new table: this value lives only in
//! memory (env-var-seeded once at boot); this screen makes it editable for
//! the rest of the process's lifetime via `Command::SetClaudeConfig`,
//! mirroring `op.rs`'s `WorkspaceConfig` display/edit-toggle pattern.
//!
//! The per-call budget cap and permission-mode toggles that used to sit here
//! were removed on 2026-08-18 together with the one-shot `claude -p`
//! executor they configured: interactive sessions are user-paced and always
//! run `--dangerously-skip-permissions`, so showing those knobs would have
//! been settings that do nothing.

use crate::kernel::Kernel;
use crate::theme;
use bw_app::Command;
use dioxus::prelude::*;
use ui::vm::SettingsVm;

#[component]
pub fn SettingsHub(settings: SettingsVm) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;max-width:640px;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "SETTINGS" }
            div {
                style: "display:flex;align-items:baseline;gap:10px;margin:4px 0 18px;",
                span { style: "font-family:{serif};font-size:22px;font-weight:600;", "设置" }
            }
            ClaudeConfigCard { settings }
        }
    }
}

#[component]
fn ClaudeConfigCard(settings: SettingsVm) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let clay = theme::CLAY;
    let input_style = theme::input();
    let label_style = theme::label();

    let mut editing = use_signal(|| false);
    let mut binary = use_signal(|| settings.binary_raw.clone());

    if !editing() {
        let settings0 = settings.clone();
        rsx! {
            div {
                style: "{card} padding:18px 22px;margin-bottom:16px;",
                div { style: "font-size:11px;color:{ink3};letter-spacing:.08em;text-transform:uppercase;margin-bottom:14px;", "队友执行器" }
                Row { label: "claude 二进制", value: settings.binary_label.clone() }
                div { style: "font-size:11.5px;color:{ink3};margin:8px 0 12px;line-height:1.6;",
                    "▶跑 在内嵌终端里启动交互式 claude:全程可见、可中止,由你把握花费与操作;运行不设单次预算上限。"
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:6px 14px;font-size:12.5px;margin-top:6px;",
                    onclick: move |_| {
                        binary.set(settings0.binary_raw.clone());
                        editing.set(true);
                    },
                    "修改"
                }
            }
        }
    } else {
        rsx! {
            div {
                style: "{card} padding:18px 22px;margin-bottom:16px;",
                div { style: "font-size:11px;color:{ink3};letter-spacing:.08em;text-transform:uppercase;margin-bottom:14px;", "队友执行器" }
                div { style: "{label_style}", "claude 二进制路径" }
                input {
                    style: "{input_style} margin-bottom:12px;font-family:{mono};",
                    placeholder: "留空 = 自动从 PATH 解析",
                    value: "{binary}",
                    oninput: move |e| binary.set(e.value()),
                }
                div {
                    style: "display:flex;gap:8px;",
                    button {
                        style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:7px;padding:6px 14px;font-size:12px;",
                        onclick: move |_| {
                            let raw = binary().trim().to_string();
                            k.send(Command::SetClaudeConfig {
                                binary: if raw.is_empty() { None } else { Some(raw) },
                            });
                            editing.set(false);
                        },
                        "保存"
                    }
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid #E2DCCF;border-radius:7px;padding:6px 14px;font-size:12px;",
                        onclick: move |_| editing.set(false),
                        "取消"
                    }
                }
            }
        }
    }
}

#[component]
fn Row(label: &'static str, value: String) -> Element {
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    rsx! {
        div {
            style: "display:flex;align-items:center;padding:9px 0;border-bottom:1px solid #EFEAdf;",
            span { style: "flex:1;font-size:13px;color:{ink3};", "{label}" }
            span { style: "font-size:13px;color:{ink2};", "{value}" }
        }
    }
}
