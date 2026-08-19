//! 顶层 · 项目墙。不打开任何项目时看到的那一屏。
//!
//! 卡片上的灯来自库里的显示缓存(项目墙要在不打开项目时列出 N 个项目,不能每
//! 次启动扫 N 个仓)。**没数据的灯是灰的,不是绿的**。

use crate::bridge::{Bridge, Req, TopView};
use crate::theme;
use crate::vm::{ProjectCardVm, ToolProbeVm, Vm};
use dioxus::prelude::*;

#[component]
pub fn View(vm: Vm, bridge: Bridge, go_top: EventHandler<TopView>) -> Element {
    let (vm, bridge) = (&vm, &bridge);
    let go_onboard = go_top;
    let go_settings = go_top;
    rsx! {
        div {
            style: "max-width:1180px;margin:0 auto;padding:32px 24px 60px;",
            div {
                style: "display:flex;align-items:baseline;gap:14px;margin-bottom:6px;",
                div { style: "font-family:{theme::SERIF};font-size:26px;", "项目" }
                div { style: "flex:1;" }
                button {
                    style: "{theme::btn_ghost()}",
                    onclick: move |_| go_settings(TopView::Settings),
                    "设置"
                }
                button {
                    style: "{theme::btn_primary()}",
                    onclick: move |_| go_onboard(TopView::Onboard),
                    "接入项目"
                }
            }
            div {
                style: "color:{theme::INK_3};font-size:13px;margin-bottom:22px;",
                "灰灯是「还没有数据」,不是「一切正常」。"
            }

            {env_bar(&vm.env)}

            if vm.projects.is_empty() {
                div {
                    style: "{theme::card()}padding:40px;text-align:center;color:{theme::INK_3};\
                            font-size:14px;line-height:2;margin-top:20px;",
                    "还没有接入任何项目。"
                    br {}
                    "点右上角「接入项目」,填四个字段,buddy 会把管理体系铺进这个仓。"
                }
            } else {
                div {
                    style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(320px,1fr));\
                            gap:16px;margin-top:20px;",
                    for p in vm.projects.iter() {
                        {project_card(p, bridge)}
                    }
                }
            }
        }
    }
}

fn env_bar(env: &[ToolProbeVm]) -> Element {
    rsx! {
        div {
            style: "{theme::card()}padding:12px 16px;display:flex;align-items:center;gap:18px;\
                    flex-wrap:wrap;font-size:12px;color:{theme::INK_2};",
            div { style: "color:{theme::INK_3};", "本机环境" }
            for t in env.iter() {
                {env_item(t)}
            }
        }
    }
}

fn env_item(t: &ToolProbeVm) -> Element {
    let color = match t.ok {
        Some(true) => "#5C8A5E",
        Some(false) => "#A33D29",
        None => "#A19B8D",
    };
    let dot = theme::dot(color, 8);
    rsx! {
        div {
            key: "{t.name}",
            style: "display:flex;align-items:center;gap:6px;",
            div { style: "{dot}" }
            span { "{t.label}" }
            span { style: "color:{theme::INK_4};", "{t.detail}" }
        }
    }
}

fn project_card(p: &ProjectCardVm, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let id = p.id;
    let color = theme::signal_color(p.signal);
    let label = theme::signal_label(p.signal);
    let progress = if p.week_total == 0 {
        "本周还没排活".to_string()
    } else {
        format!("本周 {}/{} 完成", p.week_done, p.week_total)
    };
    rsx! {
        div {
            key: "{p.slug}",
            style: "{theme::card()}padding:18px;cursor:pointer;",
            onclick: move |_| b.send(Req::Open(Some(id))),
            div {
                style: "display:flex;align-items:center;gap:8px;margin-bottom:8px;",
                div { style: "{theme::dot(color, 10)}" }
                div { style: "font-family:{theme::SERIF};font-size:17px;flex:1;", "{p.name}" }
                div { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{label}" }
            }
            div {
                style: "color:{theme::INK_2};font-size:13px;line-height:1.7;min-height:44px;",
                if p.brief.trim().is_empty() { "(还没填「想做什么」)" } else { "{p.brief}" }
            }
            div {
                style: "margin-top:12px;padding-top:10px;border-top:1px solid {theme::BORDER};\
                        font-size:12px;color:{theme::INK_3};display:flex;gap:14px;flex-wrap:wrap;",
                span { "{progress}" }
                span { "{p.remote}" }
            }
        }
    }
}
