//! 项目内 · 配置。三段:类别映射、技能与 workflow、连接器与节律。
//!
//! 「用过几次」是**现算**的 —— 扫活的 workflow 列聚合,没有战绩表可查。技能清
//! 单也是扫项目仓 `.claude/skills/` 目录得到的,没有登记表:目录里有就是有。

use crate::bridge::Bridge;
use crate::theme;
use crate::vm::{MappingVm, ProjectVm};
use bw_v4::command::Command;
use bw_v4::model::category_from_key;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    rsx! {
        div {
            style: "max-width:960px;margin:0 auto;display:flex;flex-direction:column;gap:16px;",
            {mapping_block(p, bridge)}
            {skill_block(p)}
            {connector_block(p, bridge)}
            StandardBlock { p: p.clone(), bridge: bridge.clone() }
        }
    }
}

/// ④ 规范件与在研版本。对账是**纯读**的:只回报缺 / 过期 / 人改过,不动仓、
/// 不建活 —— 要不要补由人决定。
#[component]
fn StandardBlock(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    let b_rec = bridge.clone();
    let b_ver = bridge.clone();
    let pid = p.id;
    let mut version = use_signal(|| p.card.current_version.clone());
    let std_ver = if p.card.standard_version.is_empty() {
        "—(这个仓还没铺过规范件)".to_string()
    } else {
        p.card.standard_version.clone()
    };
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div { style: "font-family:{theme::SERIF};font-size:18px;margin-bottom:14px;", "④ 规范件与在研版本" }
            div {
                style: "display:flex;gap:8px;font-size:13px;padding:6px 0;align-items:center;",
                div { style: "width:88px;flex:none;color:{theme::INK_4};", "规范版本" }
                div { style: "color:{theme::INK_2};flex:1;", "{std_ver}" }
                button {
                    style: "{theme::btn_ghost()}",
                    onclick: move |_| b_rec.cmd(Command::ReconcileStandard { project_id: pid }),
                    "对一遍账"
                }
            }
            div {
                style: "font-size:12px;color:{theme::INK_3};line-height:1.8;padding-bottom:8px;",
                "对账只看不改:缺哪几份、哪几份过期了、哪几份被人手改过,结果显示在页脚回执里。"
            }
            div {
                style: "display:flex;gap:8px;font-size:13px;padding:6px 0;align-items:center;                        border-top:1px solid {theme::BORDER};",
                div { style: "width:88px;flex:none;color:{theme::INK_4};", "在研版本" }
                input {
                    style: "{theme::input()}width:140px;",
                    value: "{version}",
                    oninput: move |e| version.set(e.value()),
                }
                button {
                    style: "{theme::btn_ghost()}",
                    onclick: move |_| b_ver.cmd(Command::SetCurrentVersion {
                        project_id: pid,
                        version: version.read().trim().to_string(),
                    }),
                    "保存"
                }
            }
        }
    }
}

fn mapping_block(p: &ProjectVm, bridge: &Bridge) -> Element {
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div { style: "font-family:{theme::SERIF};font-size:18px;margin-bottom:4px;", "① 开工工具映射" }
            div {
                style: "font-size:12px;color:{theme::INK_3};margin-bottom:14px;line-height:1.8;",
                "建一张活的时候,类别决定它默认用哪个工具、挂哪个 workflow。正本是仓里的 \
                 .bw/issue-policy.toml —— 改这里就是改那份文件。"
            }
            if p.config.mappings.is_empty() {
                div {
                    style: "font-size:13px;color:{theme::INK_4};",
                    "这个项目还没有 .bw/issue-policy.toml。去总览点「规范铺底」把它铺出来。"
                }
            }
            for m in p.config.mappings.iter() {
                MappingRow { key: "{m.category_key}", m: m.clone(), pid: p.id, bridge: bridge.clone() }
            }
        }
    }
}

/// 一行映射。**必须是组件**:它有自己的 `use_signal`,而它是在 `for` 循环里
/// 渲染的 —— 做成普通函数的话,hook 数量会随映射条数变,行与行的输入框内容
/// 会串位(改 A 类别却存到 B 类别上)。
#[component]
fn MappingRow(m: MappingVm, pid: bw_v4::model::ProjectId, bridge: Bridge) -> Element {
    let mut tool = use_signal(|| m.tool.clone());
    let mut workflow = use_signal(|| m.workflow.clone());
    let b = bridge.clone();
    let key = m.category_key.clone();
    let save = move |_| {
        let Some(cat) = category_from_key(&key) else {
            return;
        };
        b.cmd(Command::SaveToolMapping {
            project_id: pid,
            category: cat,
            tool: normalize_tool(&tool.read()),
            workflow: workflow.read().trim().to_string(),
        });
    };
    rsx! {
        div {
            style: "display:flex;gap:10px;align-items:center;padding:9px 0;\
                    border-top:1px solid {theme::BORDER};",
            div { style: "width:88px;flex:none;font-size:13px;", "{m.category_label}" }
            select {
                style: "{theme::input()}width:150px;",
                value: "{tool}",
                onchange: move |e| tool.set(e.value()),
                option { value: "Claude CLI", "Claude CLI" }
                option { value: "Cursor", "Cursor" }
                option { value: "Open Design", "Open Design" }
                option { value: "—", "—(未定)" }
            }
            input {
                style: "{theme::input()}flex:1;",
                value: "{workflow}",
                placeholder: "workflow / 技能包名,留空 = 无默认",
                oninput: move |e| workflow.set(e.value()),
            }
            button { style: "{theme::btn_ghost()}", onclick: save, "保存" }
        }
    }
}

fn normalize_tool(label: &str) -> String {
    match label.trim() {
        "Claude CLI" => "claude_cli".into(),
        "Cursor" => "cursor".into(),
        "Open Design" => "open_design".into(),
        _ => String::new(),
    }
}

fn skill_block(p: &ProjectVm) -> Element {
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div { style: "font-family:{theme::SERIF};font-size:18px;margin-bottom:4px;", "② 技能与 workflow" }
            div {
                style: "font-size:12px;color:{theme::INK_3};margin-bottom:14px;line-height:1.8;",
                "清单是扫项目仓 .claude/skills/ 得到的 —— 目录里有就是有,没有登记表。\
                 「用过几次」每次现算(按活挂的 workflow 聚合),不缓存汇总数。"
            }
            if p.config.skills.is_empty() {
                div {
                    style: "font-size:13px;color:{theme::INK_4};",
                    "这个仓的 .claude/skills/ 是空的。规范铺底会把预置包复制进去。"
                }
            }
            div {
                style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:10px;",
                for s in p.config.skills.iter() {
                    div {
                        key: "{s.slug}",
                        style: "background:{theme::CARD_ALT};border:1px solid {theme::BORDER};\
                                border-radius:8px;padding:12px 14px;",
                        div { style: "font-size:13px;line-height:1.6;", "{s.title}" }
                        div {
                            style: "font-size:11px;color:{theme::INK_4};margin-top:6px;",
                            if s.uses == 0 { "还没被任何活用过" } else { "用过 {s.uses} 次" }
                        }
                    }
                }
            }
        }
    }
}

fn connector_block(p: &ProjectVm, bridge: &Bridge) -> Element {
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div { style: "font-family:{theme::SERIF};font-size:18px;margin-bottom:14px;", "③ 连接器与节律" }
            div {
                style: "display:flex;gap:8px;font-size:13px;padding:6px 0;",
                div { style: "width:88px;flex:none;color:{theme::INK_4};", "远端仓" }
                div { style: "color:{theme::INK_2};", "{p.config.remote}" }
            }
            div {
                style: "display:flex;gap:8px;font-size:13px;padding:6px 0;",
                div { style: "width:88px;flex:none;color:{theme::INK_4};", "节律" }
                div { style: "color:{theme::INK_2};", "{p.config.cadence}" }
            }
            div {
                style: "margin-top:12px;padding-top:12px;border-top:1px solid {theme::BORDER};",
                div { style: "font-size:12px;color:{theme::INK_3};margin-bottom:10px;", "开工工具探活" }
                div {
                    style: "display:flex;gap:10px;flex-wrap:wrap;",
                    for t in p.config.tools.iter() {
                        {probe_chip(&t.name, &t.label, t.ok, &t.detail, bridge)}
                    }
                }
            }
        }
    }
}

fn probe_chip(name: &str, label: &str, ok: Option<bool>, detail: &str, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let n = name.to_string();
    let color = match ok {
        Some(true) => "#5C8A5E",
        Some(false) => "#A33D29",
        None => "#A19B8D",
    };
    rsx! {
        button {
            key: "{name}",
            style: "cursor:pointer;background:{theme::CARD_ALT};border:1px solid {theme::BORDER};\
                    border-radius:8px;padding:8px 12px;display:flex;align-items:center;gap:7px;\
                    font-size:12px;color:{theme::INK_2};",
            onclick: move |_| b.cmd(Command::ProbeTool { name: n.clone() }),
            div { style: "{theme::dot(color, 8)}" }
            span { "{label}" }
            span { style: "color:{theme::INK_4};", "测一下" }
            span { style: "color:{theme::INK_4};max-width:220px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;", "{detail}" }
        }
    }
}
