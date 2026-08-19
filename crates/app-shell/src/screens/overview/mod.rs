//! 项目内 · 总览。一列横块,不分阶段视角。
//!
//! 每一块的数字都是现算的:名片来自 `PROJECT.md` / `.bw/project.toml`,健康来
//! 自仓文件与 git 的三条判据,指标读数来自周计划文件的「本周指标读数」段,
//! 发版记录来自 `docs/releases.md`。**没有读数就显示「无数据」,不显示 0**。

use crate::bridge::Bridge;
use crate::theme;
use crate::vm::{MetricCardVm, ProjectVm};
use bw_v4::command::Command;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    rsx! {
        div {
            style: "max-width:1000px;margin:0 auto;display:flex;flex-direction:column;gap:16px;",
            {card_block(p, bridge)}
            {health_block(p)}
            {metrics_block(p)}
            {week_block(p, bridge)}
            {release_block(p)}
        }
    }
}

fn card_block(p: &ProjectVm, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let pid = p.id;
    let c = &p.card;
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div {
                style: "display:flex;align-items:baseline;gap:10px;margin-bottom:14px;",
                div { style: "font-family:{theme::SERIF};font-size:19px;", "项目名片" }
                div { style: "flex:1;" }
                button {
                    style: "{theme::btn_ghost()}",
                    onclick: move |_| b.cmd(Command::RunStandardBootstrap { project_id: pid }),
                    "规范铺底"
                }
            }
            {field("想做什么", &c.brief)}
            {field("最像的对标", &c.benchmark)}
            {field("三个月长成什么样", &c.north_star)}
            div {
                style: "display:flex;gap:22px;flex-wrap:wrap;margin-top:12px;padding-top:12px;\
                        border-top:1px solid {theme::BORDER};font-size:12px;color:{theme::INK_3};",
                span { "仓:{c.remote}" }
                span { "在研版本:{c.current_version}" }
                span { "规范版本:{c.standard_version}" }
                span { "项目群:{c.chat}" }
            }
        }
    }
}

fn field(label: &str, value: &str) -> Element {
    rsx! {
        div {
            style: "margin-bottom:10px;",
            div { style: "font-size:12px;color:{theme::INK_3};margin-bottom:3px;", "{label}" }
            div { style: "font-size:14px;line-height:1.75;color:{theme::INK};", "{value}" }
        }
    }
}

fn health_block(p: &ProjectVm) -> Element {
    let color = theme::signal_color(p.health.signal);
    let label = theme::signal_label(p.health.signal);
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:12px;",
                div { style: "{theme::dot(color, 14)}" }
                div { style: "font-family:{theme::SERIF};font-size:19px;", "健康 · {label}" }
                div { style: "flex:1;" }
                div {
                    style: "font-size:11px;color:{theme::INK_4};",
                    "每次打开现算,库里不存这盏灯"
                }
            }
            if p.health.reasons.is_empty() {
                div { style: "color:{theme::INK_3};font-size:13px;", "还没有任何真实数据。" }
            }
            for (ok, text) in p.health.reasons.iter() {
                {reason_row(*ok, text)}
            }
        }
    }
}

fn reason_row(ok: bool, text: &str) -> Element {
    let ink = if ok { theme::INK } else { theme::INK_2 };
    let mark_color = if ok { "#5C8A5E" } else { theme::INK_4 };
    let mark = if ok { "✓" } else { "✗" };
    rsx! {
        div {
            style: "display:flex;gap:8px;align-items:flex-start;padding:5px 0;font-size:13px;\
                    line-height:1.7;color:{ink};",
            span { style: "color:{mark_color};flex:none;", "{mark}" }
            span { "{text}" }
        }
    }
}

fn metrics_block(p: &ProjectVm) -> Element {
    let m = &p.metrics;
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div { style: "font-family:{theme::SERIF};font-size:19px;margin-bottom:14px;", "指标" }
            if let Some(note) = &m.note {
                div { style: "color:{theme::INK_3};font-size:13px;", "{note}" }
            }
            if let Some(ns) = &m.north_star {
                {metric_card(ns, "北极星", true)}
            }
            if !m.lagging.is_empty() {
                div { style: "font-size:12px;color:{theme::INK_3};margin:14px 0 8px;", "滞后指标" }
                div {
                    style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:12px;",
                    for x in m.lagging.iter() { {metric_card(x, "滞后", false)} }
                }
            }
            if !m.leading.is_empty() {
                div { style: "font-size:12px;color:{theme::INK_3};margin:14px 0 8px;", "引领指标" }
                div {
                    style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(240px,1fr));gap:12px;",
                    for x in m.leading.iter() { {metric_card(x, "引领", false)} }
                }
            }
        }
    }
}

fn metric_card(m: &MetricCardVm, kind: &str, wide: bool) -> Element {
    let has = m.reading.is_some();
    let value = m.reading.clone().unwrap_or_else(|| "无数据".into());
    let width = if wide { "100%" } else { "auto" };
    let size = if wide { 26 } else { 20 };
    let value_color = if has { theme::INK } else { theme::INK_4 };
    rsx! {
        div {
            key: "{m.id}",
            style: "background:{theme::CARD_ALT};border:1px solid {theme::BORDER};border-radius:9px;\
                    padding:14px 16px;width:{width};",
            div {
                style: "display:flex;align-items:baseline;gap:8px;",
                div { style: "{theme::chip(theme::CARD, theme::INK_3)}", "{kind}" }
                div { style: "font-size:13px;color:{theme::INK_2};", "{m.name}" }
            }
            div {
                style: "font-family:{theme::MONO};font-size:{size}px;margin-top:8px;color:{value_color};",
                "{value}"
            }
            if has {
                div {
                    style: "font-size:11px;color:{theme::INK_4};margin-top:4px;",
                    "{m.source} · {m.collected_at}"
                }
            } else {
                div {
                    style: "font-size:11px;color:{theme::INK_4};margin-top:4px;",
                    "本周与上周的周计划文件里都没有这条读数"
                }
            }
            if !m.driving.is_empty() {
                div {
                    style: "margin-top:10px;padding-top:8px;border-top:1px solid {theme::BORDER};\
                            font-size:11px;color:{theme::INK_3};line-height:1.8;",
                    "本周在推它的活:"
                    for t in m.driving.iter() {
                        div { style: "color:{theme::INK_2};", "· {t}" }
                    }
                }
            }
        }
    }
}

fn week_block(p: &ProjectVm, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let pid = p.id;
    let week = p.current_week.clone();
    let this_week = p.weeks.iter().find(|w| w.week == p.current_week);
    let done = p
        .board
        .columns
        .iter()
        .find(|c| c.status == bw_v4::model::IssueStatus::Done)
        .map(|c| c.cards.len())
        .unwrap_or(0);
    let total: usize = p
        .board
        .columns
        .iter()
        .filter(|c| c.status != bw_v4::model::IssueStatus::Backlog)
        .map(|c| c.cards.len())
        .sum();
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div {
                style: "display:flex;align-items:baseline;gap:10px;margin-bottom:12px;",
                div { style: "font-family:{theme::SERIF};font-size:19px;", "本周 · {p.current_week}" }
                div { style: "flex:1;" }
                if this_week.is_none() {
                    button {
                        style: "{theme::btn_primary()}",
                        onclick: move |_| b.cmd(Command::StartWeekPlanning {
                            project_id: pid,
                            week: week.clone(),
                        }),
                        "开始本周"
                    }
                }
            }
            match this_week {
                None => rsx! {
                    div {
                        style: "color:{theme::INK_3};font-size:13px;line-height:1.9;",
                        "本周还没有周计划文件。点「开始本周」会在仓里写出 docs/plan/{p.current_week}.md,\
                         复盘上周、更新指标、引导出这一周要干的活。"
                    }
                },
                Some(w) => rsx! {
                    div {
                        style: "font-size:13px;line-height:1.85;color:{theme::INK};margin-bottom:10px;",
                        {w.goal.clone().unwrap_or_else(|| "(这一周还没写周目标)".into())}
                    }
                    div {
                        style: "font-size:12px;color:{theme::INK_3};",
                        "排了 {total} 张活,完成 {done} 张。"
                    }
                },
            }
        }
    }
}

fn release_block(p: &ProjectVm) -> Element {
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div { style: "font-family:{theme::SERIF};font-size:19px;margin-bottom:12px;", "发版记录" }
            if p.releases.is_empty() {
                div {
                    style: "color:{theme::INK_3};font-size:13px;",
                    "还没有发过版。仓里的 docs/releases.md 是这份记录的唯一正本,库里不存副本。"
                }
            } else {
                table {
                    style: "width:100%;border-collapse:collapse;font-size:13px;",
                    thead {
                        tr {
                            style: "color:{theme::INK_3};font-size:12px;text-align:left;",
                            th { style: "padding:6px 8px;font-weight:400;", "版本" }
                            th { style: "padding:6px 8px;font-weight:400;", "发版日" }
                            th { style: "padding:6px 8px;font-weight:400;", "说明" }
                            th { style: "padding:6px 8px;font-weight:400;", "包含的活" }
                            th { style: "padding:6px 8px;font-weight:400;", "来源" }
                        }
                    }
                    tbody {
                        for r in p.releases.iter() {
                            tr {
                                key: "{r.version}",
                                style: "border-top:1px solid {theme::BORDER};",
                                td { style: "padding:8px;font-family:{theme::MONO};", "{r.version}" }
                                td { style: "padding:8px;color:{theme::INK_2};", "{r.released_at}" }
                                td { style: "padding:8px;color:{theme::INK_2};", "{r.note}" }
                                td { style: "padding:8px;font-family:{theme::MONO};color:{theme::INK_3};", "{r.included}" }
                                td { style: "padding:8px;", span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{r.origin}" } }
                            }
                        }
                    }
                }
            }
        }
    }
}
