//! 顶层 · 接入项目。两张卡:先说清「这是个什么项目」,再指到一个真实的仓。
//!
//! 四个字段全部落仓文件(`PROJECT.md` 与 `.bw/project.toml`),库里只记路径与
//! 显示用的名字 —— 名片的正本在仓里,换台机器拉下来就有。

use crate::bridge::{Bridge, Req};
use crate::theme;
use crate::vm::Vm;
use bw_v4::command::{Command, ProjectIntent, RemoteRef};
use dioxus::prelude::*;

#[component]
pub fn View(vm: Vm, bridge: Bridge, close: EventHandler<MouseEvent>) -> Element {
    let (vm, bridge) = (&vm, &bridge);
    let mut name = use_signal(String::new);
    let mut brief = use_signal(String::new);
    let mut benchmark = use_signal(String::new);
    let mut north_star = use_signal(String::new);
    let mut slug = use_signal(String::new);
    let mut workspace = use_signal(String::new);
    let mut remote = use_signal(String::new);

    let b = bridge.clone();
    let root = vm.settings.workspaces_root.clone();
    let submit = move |_| {
        let s = if slug.read().trim().is_empty() {
            slugify(&name.read())
        } else {
            slug.read().trim().to_string()
        };
        if s.is_empty() || name.read().trim().is_empty() {
            return;
        }
        let r = remote.read().trim().to_string();
        b.cmd(Command::CreateProject {
            slug: s,
            intent: ProjectIntent {
                name: name.read().trim().to_string(),
                brief: brief.read().trim().to_string(),
                benchmark: benchmark.read().trim().to_string(),
                north_star: north_star.read().trim().to_string(),
            },
            remote: if r.is_empty() {
                RemoteRef::default()
            } else {
                RemoteRef {
                    provider: "github".into(),
                    host: "github.com".into(),
                    path: r,
                }
            },
            workspace_path: workspace.read().trim().to_string(),
        });
    };

    rsx! {
        div {
            style: "max-width:840px;margin:0 auto;padding:32px 24px 60px;",
            div {
                style: "display:flex;align-items:baseline;gap:14px;margin-bottom:20px;",
                div { style: "font-family:{theme::SERIF};font-size:24px;", "接入项目" }
                div { style: "flex:1;" }
                button { style: "{theme::btn_ghost()}", onclick: close, "返回项目墙" }
            }

            // ── 卡一:这是个什么项目 ───────────────────────
            div {
                style: "{theme::card()}padding:22px;margin-bottom:16px;",
                div { style: "font-family:{theme::SERIF};font-size:17px;margin-bottom:4px;", "① 这是个什么项目" }
                div {
                    style: "color:{theme::INK_3};font-size:12px;margin-bottom:16px;line-height:1.8;",
                    "四个字段会写进仓里的 PROJECT.md 与 .bw/project.toml —— 换台机器拉下来就有,不是只存在你这台电脑上。"
                }
                label { style: "{theme::label()}", "名称" }
                input {
                    style: "{theme::input()}", value: "{name}",
                    placeholder: "例如 WorkflowHub",
                    oninput: move |e| name.set(e.value()),
                }
                div { style: "height:12px;" }
                label { style: "{theme::label()}", "想做什么(一句话)" }
                textarea {
                    style: "{theme::input()}", rows: 2, value: "{brief}",
                    placeholder: "把 agent 会话里长出的工作流沉淀成可复用资产",
                    oninput: move |e| brief.set(e.value()),
                }
                div { style: "height:12px;" }
                label { style: "{theme::label()}", "最像的对标" }
                input {
                    style: "{theme::input()}", value: "{benchmark}",
                    placeholder: "Linear",
                    oninput: move |e| benchmark.set(e.value()),
                }
                div { style: "height:12px;" }
                label { style: "{theme::label()}", "三个月长成什么样(北极星一句话)" }
                textarea {
                    style: "{theme::input()}", rows: 2, value: "{north_star}",
                    placeholder: "每月被标准工作流带完成的活数",
                    oninput: move |e| north_star.set(e.value()),
                }
            }

            // ── 卡二:仓在哪 ───────────────────────────────
            div {
                style: "{theme::card()}padding:22px;margin-bottom:20px;",
                div { style: "font-family:{theme::SERIF};font-size:17px;margin-bottom:4px;", "② 仓在哪" }
                div {
                    style: "color:{theme::INK_3};font-size:12px;margin-bottom:16px;line-height:1.8;",
                    "工作区留空就用 {root}/<目录名>。远端留空也能用 —— 没挂远端的项目一样能建活、能干活,只是没有 MR 可评审。"
                }
                label { style: "{theme::label()}", "目录名(留空按名称自动生成)" }
                input {
                    style: "{theme::input()}", value: "{slug}",
                    placeholder: "workflowhub",
                    oninput: move |e| slug.set(e.value()),
                }
                div { style: "height:12px;" }
                label { style: "{theme::label()}", "本机仓路径(留空用工作区根目录)" }
                input {
                    style: "{theme::input()}", value: "{workspace}",
                    placeholder: "留空即可",
                    oninput: move |e| workspace.set(e.value()),
                }
                div { style: "height:12px;" }
                label { style: "{theme::label()}", "远端仓(owner/repo,可留空)" }
                input {
                    style: "{theme::input()}", value: "{remote}",
                    placeholder: "forcegravity1989/loop-buddy",
                    oninput: move |e| remote.set(e.value()),
                }
            }

            button { style: "{theme::btn_primary()}", onclick: submit, "接入" }

            if !vm.projects.is_empty() {
                div {
                    style: "margin-top:28px;color:{theme::INK_3};font-size:12px;",
                    "已接入 {vm.projects.len()} 个项目。接入之后记得在总览点「规范铺底」,把管理体系写进这个仓。"
                }
            }
        }
    }
}

/// 名称 → 目录名。中文与空格换成短横,认不出的字符丢掉。
fn slugify(name: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.chars().all(|c| c == '-') {
        String::new()
    } else {
        s
    }
}

/// 让 `Req` 的导入有用武之地:接入成功后由内核推回新的 ViewModel,壳不需要
/// 自己跳转——这里保留一个显式的刷新入口给后续用。
#[allow(dead_code)]
fn refresh(bridge: &Bridge) {
    bridge.send(Req::Refresh);
}
