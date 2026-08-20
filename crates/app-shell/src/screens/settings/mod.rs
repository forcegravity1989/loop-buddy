//! 顶层 · 设置。本机相关的几件事:工作区根目录、库文件、各个开工工具在不在。
//!
//! 版式照 `docs/v4-prototype/hifi/index.html` 的 `renderSettings`:一个描边列表,
//! 每行「名字 / 取值 / 右侧一个灯点」。
//!
//! **没有登录**。V4 不管任何账号:远端凭证在系统钥匙串、群登录态由用户自己在
//! 本机登好,buddy 只探活。
//!
//! **工作区根目录可以在这里改**(存本机库的 `app_meta`,立刻生效):它只决定
//! 「以后新接进来的项目默认落在哪」。已接入的项目各自记着自己的仓路径,一个都
//! 不会被牵着走 —— 不然改一下根目录,已有项目就集体找不到仓了。
//!
//! **库文件仍然是只读的**:进程起来之后库连接已经建好,改它得重启,所以这里给
//! 的是一句「启动前设哪个环境变量」的实话,不是一个按不动的按钮。

use crate::bridge::Bridge;
use crate::chrome::light_dot;
use crate::vm::{ToolProbeVm, Vm};
use bw_v4::command::Command;
use bw_v4::Signal as HealthSignal;
use dioxus::prelude::*;

#[component]
pub fn View(vm: Vm, bridge: Bridge, close: EventHandler<MouseEvent>) -> Element {
    let vm = &vm;
    let s = &vm.settings;
    rsx! {
        section {
            div { class: "ob-head",
                h1 { style: "font-size:20px;margin:0;", "设置" }
                button { class: "btn btn-ghost btn-sm", onclick: close, "← 项目墙" }
            }
            div { class: "settings-list",
                root_row { value: s.workspaces_root.clone(), bridge: bridge.clone() }
                {path_row(
                    "本机数据库",
                    &s.db_path,
                    "只放定位与显示缓存。删掉它,从仓和远端能重建。改它:启动前设 BW_DB。",
                )}
                for t in vm.env.iter() {
                    {probe_row(t)}
                }
            }
            div { style: "margin-top:14px;color:var(--ink-3);font-size:11.5px;line-height:1.9;",
                "这里没有登录 —— buddy 不管账号。远端凭证在系统钥匙串里,项目群的登录态由你自己在本机登好。"
                br {}
                "灰灯是「这项探活还没接」,不是「有问题」;红灯才是「本机路径里真的没找到」。"
            }
        }
    }
}

/// 工作区根目录:可改。填全路径,存本机库,立刻生效。
#[component]
fn root_row(value: String, bridge: Bridge) -> Element {
    // 输入框的初值取当前值;人改完点「保存」才发命令,边打字边存会把
    // 「D:\\pro」这种打了一半的路径也建成目录。
    let mut draft = use_signal(|| value.clone());
    let shown = value.clone();
    let b = bridge.clone();
    rsx! {
        div { class: "settings-row",
            div { style: "flex:1;min-width:0;",
                div { class: "k", "工作区根目录" }
                div { style: "display:flex;gap:8px;align-items:center;margin-top:4px;",
                    input {
                        class: "input mono",
                        style: "flex:1;min-width:0;",
                        value: "{draft}",
                        placeholder: "Windows 填 D:\\buddy\\workspaces,macOS 填 /Users/你/projects",
                        oninput: move |e| draft.set(e.value()),
                    }
                    button {
                        class: "btn btn-sm btn-primary",
                        onclick: move |_| b.cmd(Command::SetWorkspacesRoot { path: draft.read().clone() }),
                        "保存"
                    }
                }
                div { style: "font-size:11px;color:var(--ink-4);margin-top:4px;",
                    "新接入的项目默认落在这个目录下,填全路径。"
                    br {}
                    "改它"
                    strong { "只影响以后新接进来的项目" }
                    " —— 已接入的项目各自记着自己的仓路径,不会跟着搬,现在的值是 "
                    span { class: "mono", "{shown}" }
                    "。"
                }
            }
        }
    }
}

fn path_row(label: &str, value: &str, hint: &str) -> Element {
    let shown = if value.trim().is_empty() {
        "—"
    } else {
        value
    };
    rsx! {
        div { class: "settings-row",
            div { style: "flex:1;min-width:0;",
                div { class: "k", "{label}" }
                div { class: "v mono", style: "word-break:break-all;", "{shown}" }
                div { style: "font-size:11px;color:var(--ink-4);margin-top:3px;", "{hint}" }
            }
        }
    }
}

fn probe_row(t: &ToolProbeVm) -> Element {
    let (signal, state) = match t.ok {
        Some(true) => (Some(HealthSignal::Green), "已找到"),
        Some(false) => (Some(HealthSignal::Red), "没找到"),
        None => (None, "还没接探活"),
    };
    rsx! {
        div { key: "{t.name}", class: "settings-row",
            div { style: "flex:1;min-width:0;",
                div { class: "k", "{t.label}" }
                div { class: "v mono", style: "word-break:break-all;", "{t.detail}" }
            }
            div { style: "display:flex;align-items:center;gap:8px;flex:none;",
                span { style: "font-size:11.5px;color:var(--ink-3);", "{state}" }
                {light_dot(signal, true)}
            }
        }
    }
}
