//! `Hub::Settings` — the real, process-wide `ClaudeCliConfig`. No new table:
//! this value already lived only in memory (env-var-seeded once at boot);
//! this screen just makes it editable for the rest of the process's
//! lifetime via `Command::SetClaudeConfig`, mirroring `op.rs`'s
//! `WorkspaceConfig` display/edit-toggle pattern.

use crate::kernel::Kernel;
use crate::theme;
use bw_app::Command;
use bw_engine::PermissionMode;
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
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let clay = theme::CLAY;
    let alert = theme::ALERT_DEEP;
    let input_style = theme::input();
    let label_style = theme::label();

    let mut editing = use_signal(|| false);
    let mut binary = use_signal(|| settings.binary_raw.clone());
    let mut budget = use_signal(|| format!("{:.2}", settings.max_budget_usd));
    let mut bypass_default = use_signal(|| settings.bypass_default);
    let mut bypass_commands = use_signal(|| settings.bypass_commands);
    let mut error = use_signal(|| None::<String>);

    if !editing() {
        let settings0 = settings.clone();
        rsx! {
            div {
                style: "{card} padding:18px 22px;margin-bottom:16px;",
                div { style: "font-size:11px;color:{ink3};letter-spacing:.08em;text-transform:uppercase;margin-bottom:14px;", "模型与额度" }
                Row { label: "claude 二进制", value: settings.binary_label.clone() }
                Row { label: "单次预算上限", value: settings.max_budget_label.clone() }
                Row {
                    label: "默认权限模式",
                    value: if settings.bypass_default { "bypassPermissions".to_string() } else { "acceptEdits".to_string() },
                }
                Row {
                    label: "命令执行权限模式",
                    value: if settings.bypass_commands { "bypassPermissions".to_string() } else { "acceptEdits".to_string() },
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{clay};border:1px solid {clay};border-radius:7px;padding:6px 14px;font-size:12.5px;margin-top:6px;",
                    onclick: move |_| {
                        binary.set(settings0.binary_raw.clone());
                        budget.set(format!("{:.2}", settings0.max_budget_usd));
                        bypass_default.set(settings0.bypass_default);
                        bypass_commands.set(settings0.bypass_commands);
                        error.set(None);
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
                div { style: "font-size:11px;color:{ink3};letter-spacing:.08em;text-transform:uppercase;margin-bottom:14px;", "模型与额度" }
                div { style: "{label_style}", "claude 二进制路径" }
                input {
                    style: "{input_style} margin-bottom:12px;font-family:{mono};",
                    placeholder: "留空 = 自动从 PATH 解析",
                    value: "{binary}",
                    oninput: move |e| binary.set(e.value()),
                }
                div { style: "{label_style}", "单次调用预算上限(USD)" }
                input {
                    style: "{input_style} margin-bottom:12px;",
                    r#type: "number",
                    step: "0.10",
                    value: "{budget}",
                    oninput: move |e| budget.set(e.value()),
                }
                button {
                    style: "cursor:pointer;background:transparent;border:none;padding:0;margin-bottom:8px;font-size:12.5px;color:{ink2};display:flex;align-items:center;gap:6px;",
                    onclick: move |_| bypass_default.set(!bypass_default()),
                    span { if bypass_default() { "☑" } else { "☐" } }
                    "默认模式允许绕过权限检查(bypassPermissions)"
                }
                button {
                    style: "cursor:pointer;background:transparent;border:none;padding:0;margin-bottom:6px;font-size:12.5px;color:{ink2};display:flex;align-items:center;gap:6px;",
                    onclick: move |_| bypass_commands.set(!bypass_commands()),
                    span { if bypass_commands() { "☑" } else { "☐" } }
                    "命令执行模式允许绕过权限检查(bypassPermissions)"
                }
                if bypass_default() || bypass_commands() {
                    div { style: "font-size:11.5px;color:{alert};margin-bottom:10px;",
                        "⚠ bypassPermissions 会跳过 claude CLI 自身的操作确认——仅在你信任该工作目录下会执行的一切时开启"
                    }
                }
                if let Some(e) = error() {
                    div { style: "font-size:12px;color:{alert};margin-bottom:10px;", "{e}" }
                }
                div {
                    style: "display:flex;gap:8px;",
                    button {
                        style: "cursor:pointer;background:{clay};color:#FFF;border:none;border-radius:7px;padding:6px 14px;font-size:12px;",
                        onclick: move |_| {
                            let parsed: Result<f64, _> = budget().trim().parse();
                            let Ok(max_budget_usd) = parsed else {
                                error.set(Some("预算上限必须是数字".to_string()));
                                return;
                            };
                            if max_budget_usd <= 0.0 {
                                error.set(Some("预算上限必须大于 0".to_string()));
                                return;
                            }
                            let raw = binary().trim().to_string();
                            k.send(Command::SetClaudeConfig {
                                binary: if raw.is_empty() { None } else { Some(raw) },
                                max_budget_usd,
                                default_mode: if bypass_default() { PermissionMode::BypassPermissions } else { PermissionMode::AcceptEdits },
                                commands_mode: if bypass_commands() { PermissionMode::BypassPermissions } else { PermissionMode::AcceptEdits },
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
