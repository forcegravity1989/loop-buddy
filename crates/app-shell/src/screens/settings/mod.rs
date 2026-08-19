//! 顶层 · 设置。只有本机相关的三件事:工作区根目录、库文件、开工工具路径。
//!
//! **没有登录**。V4 不管任何账号:远端凭证在系统钥匙串、群登录态由用户自己在
//! 本机登好,buddy 只探活。

use crate::theme;
use crate::vm::Vm;
use dioxus::prelude::*;

pub fn view(vm: &Vm, close: impl FnMut(MouseEvent) + 'static) -> Element {
    let s = &vm.settings;
    rsx! {
        div {
            style: "max-width:760px;margin:0 auto;padding:32px 24px 60px;",
            div {
                style: "display:flex;align-items:baseline;gap:14px;margin-bottom:20px;",
                div { style: "font-family:{theme::SERIF};font-size:24px;", "设置" }
                div { style: "flex:1;" }
                button { style: "{theme::btn_ghost()}", onclick: close, "返回项目墙" }
            }
            div {
                style: "{theme::card()}padding:22px;",
                {row("工作区根目录", &s.workspaces_root, "新接入的项目默认落在这个目录下。改它要设环境变量 BW_WORKSPACES。")}
                {row("本机数据库", &s.db_path, "只放定位与显示缓存。删掉它,从仓和远端能重建。改它要设环境变量 BW_DB。")}
                {row(
                    "Claude CLI",
                    s.claude_binary.as_deref().unwrap_or("—(本机路径里没找到)"),
                    "干活入口。找不到就是找不到,不会悄悄退回到别的东西。",
                )}
            }
            div {
                style: "margin-top:18px;color:{theme::INK_3};font-size:12px;line-height:1.9;",
                "这里没有登录 —— buddy 不管账号。远端凭证在系统钥匙串里,项目群的登录态由你自己在本机登好,buddy 只负责探一下通不通。"
            }
        }
    }
}

fn row(label: &str, value: &str, hint: &str) -> Element {
    rsx! {
        div {
            style: "padding:12px 0;border-bottom:1px solid {theme::BORDER};",
            div { style: "font-size:13px;color:{theme::INK_2};margin-bottom:4px;", "{label}" }
            div {
                style: "font-family:{theme::MONO};font-size:12px;color:{theme::INK};\
                        word-break:break-all;margin-bottom:4px;",
                "{value}"
            }
            div { style: "font-size:11px;color:{theme::INK_4};", "{hint}" }
        }
    }
}
