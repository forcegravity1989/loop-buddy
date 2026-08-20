//! 顶层 · 设置。本机相关的几件事:工作区根目录、库文件、各个开工工具在不在。
//!
//! 版式照 `docs/v4-prototype/hifi/index.html` 的 `renderSettings`:一个描边列表,
//! 每行「名字 / 取值 / 右侧一个灯点」。
//!
//! **没有登录**。V4 不管任何账号:远端凭证在系统钥匙串、群登录态由用户自己在
//! 本机登好,buddy 只探活。
//!
//! **两个路径是只读的**:工作区根目录和库文件都由启动时的环境变量决定
//! (`BW_WORKSPACES` / `BW_DB`),进程起来之后改它没有意义 —— 库连接已经建好、
//! 已接入项目的路径已经按老根目录算过了。高保真上那个「更改…」按钮真做出来
//! 会是一颗哑弹,所以这里给的是一句怎么改的实话,不是一个按不动的按钮。

use crate::chrome::light_dot;
use crate::vm::{ToolProbeVm, Vm};
use bw_v4::Signal as HealthSignal;
use dioxus::prelude::*;

#[component]
pub fn View(vm: Vm, close: EventHandler<MouseEvent>) -> Element {
    let vm = &vm;
    let s = &vm.settings;
    rsx! {
        section {
            div { class: "ob-head",
                h1 { style: "font-size:20px;margin:0;", "设置" }
                button { class: "btn btn-ghost btn-sm", onclick: close, "← 项目墙" }
            }
            div { class: "settings-list",
                {path_row(
                    "工作区根目录",
                    &s.workspaces_root,
                    "新接入的项目默认落在这个目录下。改它:启动前设 BW_WORKSPACES。",
                )}
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
