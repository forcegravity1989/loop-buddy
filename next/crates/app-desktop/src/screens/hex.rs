//! 六段总控屏(`BW_PANEL=hex`,design-s5-hexpanel.md §1)。**屏上不做任
//! 何推导**——每一处显示的都是 `bw_app::view::hex::HexView`(或从它筛出
//! 来的既成事实,如逃生舱)里已经算好的字段,组件只管排版与格式化。没
//! 有数据的段照 §1.2 如实空(灰卡/说明文案),绝不假装绿。

use bw_app::view::hex::{
    EvidenceSegment, FiveRolesSegment, LoopSegment, MetricCard, NorthStarView, RiskDecisionSegment,
};
use bw_core::Signal;
use dioxus::prelude::*;

use crate::kernel::Vm;
use crate::theme;

#[component]
pub fn HexScreen(vm: Vm) -> Element {
    let Some(hex) = vm.hex.clone() else {
        return rsx! {
            div { style: "color:{theme::INK_3};padding:24px;", "没有可显示的项目——先在顶栏选一个,或深链 BW_OPEN 指定一个真实存在的项目名。" }
        };
    };
    rsx! {
        div {
            style: "display:flex;flex-direction:column;gap:18px;max-width:1040px;margin:0 auto;",
            NorthStarSegment { view: hex.north_star.clone() }
            FiveRolesSegmentView { seg: hex.five_roles.clone() }
            MetricsSegmentView { cards: hex.metrics.cards.clone() }
            LoopSegmentView { seg: hex.loop_segment.clone(), open_runs: vm.open_runs.clone() }
            RiskDecisionSegmentView { seg: hex.risk_decision.clone() }
            EvidenceSegmentView { seg: hex.evidence.clone() }
        }
    }
}

/// 会话号前 8 位,空 = 「—」(展示层格式化,不是推导——这条指标本身
/// 已经在存储层是既成事实,这里只决定怎么显示)。**不能内联进 rsx! 的
/// 字符串插值**:`"...{if a { b } else { c }}..."` 这种块表达式嵌在带引
/// 号的插值文本里,dioxus 的 rsx! 解析器认不出来(实测过,报「Failed to
/// parse formatted segment」),必须先拆成一个简单表达式(函数调用)。
fn short_session(upstream_session: &str) -> String {
    if upstream_session.is_empty() {
        "—".to_string()
    } else {
        upstream_session.chars().take(8).collect()
    }
}

/// 同上,观测出处提示的「 · 提示文本」后缀,空提示时不加后缀。
fn hint_suffix(source_hint: &str) -> String {
    if source_hint.is_empty() {
        String::new()
    } else {
        format!(" · {source_hint}")
    }
}

fn section_title(n: &str, title: &str) -> Element {
    rsx! {
        div {
            style: "display:flex;align-items:baseline;gap:8px;margin-bottom:2px;",
            span { style: "font-family:{theme::SERIF};font-weight:700;color:{theme::CLAY};", "{n}" }
            span { style: "font-family:{theme::SERIF};font-weight:700;font-size:15px;color:{theme::INK};", "{title}" }
        }
    }
}

/// 段①:项目目标(北极星)。
#[component]
fn NorthStarSegment(view: NorthStarView) -> Element {
    rsx! {
        div {
            {section_title("①", "项目目标")}
            match &view {
                NorthStarView::Defined(card) => rsx! { MetricCardView { card: (**card).clone(), headline: true } },
                NorthStarView::Undefined => rsx! {
                    div {
                        style: "{theme::card_dashed_gray()}",
                        div { style: "font-weight:600;margin-bottom:4px;", "北极星尚未定稿" }
                        div { style: "font-size:12px;", "怎么定:在项目仓写 .bw/metrics.toml 的 [north_star] 一节。" }
                    }
                },
            }
        }
    }
}

/// 段②:五角色责任卡。前七项零数据库,全部来自 `bw_core::StageKind` 静
/// 态元数据(design §1.2②)——这里只挂当前一棒的高亮与活数。
#[component]
fn FiveRolesSegmentView(seg: FiveRolesSegment) -> Element {
    rsx! {
        div {
            {section_title("②", "五角色责任")}
            if seg.active_stage.is_none() {
                div { style: "font-size:12px;color:{theme::INK_3};margin-bottom:8px;", "尚未开棒——方法论本来就与项目无关,五张卡照常显,只是没有一张高亮。" }
            }
            div {
                style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(190px,1fr));gap:10px;",
                for card in seg.cards {
                    StageCardView { card }
                }
            }
            if seg.unclassified_issue_count > 0 {
                div { style: "font-size:12px;color:{theme::INK_3};margin-top:6px;", "未归类的活:{seg.unclassified_issue_count} 件(issue.stage 为空)。" }
            }
        }
    }
}

#[component]
fn StageCardView(card: bw_app::view::hex::StageCard) -> Element {
    let stage = card.stage;
    let border = if card.is_current {
        format!("2px solid {}", stage.color())
    } else {
        format!("1px solid {}", theme::BORDER)
    };
    rsx! {
        div {
            style: "background:{theme::CARD};border:{border};border-radius:10px;padding:12px 14px;",
            div {
                style: "display:flex;align-items:center;gap:6px;margin-bottom:4px;",
                span { style: "{theme::dot(stage.color(), 8)}" }
                span { style: "font-weight:600;font-size:13px;", "{stage.role_short()}" }
                if card.is_current {
                    span { style: "{theme::chip(stage.color(), \"#FFF\")}", "当前一棒" }
                }
            }
            div { style: "font-size:11px;color:{theme::INK_3};margin-bottom:6px;", "{stage.methodology()} · {stage.cycle_rhythm()}" }
            div { style: "font-size:11px;color:{theme::INK_2};margin-bottom:6px;", "{stage.core_question()}" }
            div { style: "font-size:11px;color:{theme::INK_2};", "这个阶段的活:{card.issue_count} 件" }
        }
    }
}

/// 段③:引领指标(北极星→滞后→引领,顺序固定;design §1.2③)。
#[component]
fn MetricsSegmentView(cards: Vec<MetricCard>) -> Element {
    rsx! {
        div {
            {section_title("③", "引领指标")}
            if cards.is_empty() {
                div { style: "{theme::card_dashed_gray()}", "这个项目还没有任何指标——同步项目仓的 .bw/metrics.toml,或手建一条。" }
            } else {
                div {
                    style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:10px;",
                    for card in cards {
                        MetricCardView { card, headline: false }
                    }
                }
            }
        }
    }
}

#[component]
fn MetricCardView(card: MetricCard, headline: bool) -> Element {
    let color = theme::signal_color(card.signal);
    let style = if card.has_observation {
        theme::card()
    } else {
        theme::card_dashed_gray()
    };
    let tier_label = match card.tier {
        bw_store::MetricTier::NorthStar => "北极星",
        bw_store::MetricTier::Lagging => "滞后",
        bw_store::MetricTier::Leading => "引领",
    };
    rsx! {
        div {
            style: "{style}",
            div {
                style: "display:flex;align-items:center;gap:6px;margin-bottom:4px;",
                span { style: "{theme::dot(color, 9)}" }
                span { style: if headline { "font-family:{theme::SERIF};font-weight:700;font-size:15px;" } else { "font-weight:600;font-size:13px;" }, "{card.name}" }
                span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{tier_label}" }
                if card.latest_is_manual {
                    span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "手填 · 未接入度量源" }
                }
            }
            if !card.def.is_empty() {
                div { style: "font-size:11px;color:{theme::INK_3};margin-bottom:6px;", "{card.def}" }
            }
            match card.latest_raw_value.clone() {
                Some(v) => rsx! {
                    div {
                        style: "font-family:{theme::MONO};font-size:20px;color:{theme::INK};",
                        "{v}"
                    }
                    if !card.target_raw.is_empty() {
                        div { style: "font-size:11px;color:{theme::INK_3};", "目标:{card.target_raw}" }
                    }
                },
                None => rsx! {
                    div { style: "font-family:{theme::MONO};font-size:20px;color:{theme::INK_4};", "—" }
                    div { style: "font-size:11px;color:{theme::INK_3};", "无观测 · Unknown ≠ 绿" }
                },
            }
            if !card.collect_kind.is_empty() {
                div { style: "font-size:10px;color:{theme::INK_4};margin-top:4px;", "采集方案:{card.collect_kind}" }
            }
        }
    }
}

/// 段④:当前 Loop。一行聚合数,只列例外(design §1.2④)。「进行中」的
/// 运行卡带逃生舱(design §5.2)。
#[component]
fn LoopSegmentView(seg: LoopSegment, open_runs: Vec<bw_store::RunRow>) -> Element {
    rsx! {
        div {
            {section_title("④", "当前 Loop")}
            if !seg.has_any_run {
                div { style: "{theme::card_dashed_gray()}", "尚无运行——定时机制(自动建活、定时采集)在新工程里还没建,这里目前只显示人点开工的运行。" }
            } else {
                div {
                    style: "display:flex;gap:18px;font-size:13px;color:{theme::INK_2};margin-bottom:10px;",
                    span { "在跑 " span { style: "font-weight:700;color:{theme::INK};", "{seg.running}" } }
                    span { "评审中 " span { style: "font-weight:700;color:{theme::INK};", "{seg.in_review}" } }
                    span { "最近失败 " span { style: "font-weight:700;color:{theme::signal_color(Signal::Red)};", "{seg.recent_failed}" } }
                    span { "遗留未结账 " span { style: "font-weight:700;color:{theme::signal_color(Signal::Amber)};", "{seg.unsettled}" } }
                }
                if !seg.exceptions.is_empty() {
                    div {
                        style: "display:flex;flex-direction:column;gap:6px;",
                        for run in seg.exceptions {
                            div {
                                style: "font-size:12px;color:{theme::INK_2};padding:6px 10px;background:{theme::CARD_ALT};border-radius:6px;",
                                "运行 {run.id.uuid().to_string().chars().take(8).collect::<String>()} · {run.state:?}"
                            }
                        }
                    }
                }
            }
            if !open_runs.is_empty() {
                div {
                    style: "margin-top:10px;display:flex;flex-direction:column;gap:6px;",
                    for run in open_runs {
                        EscapeHatchCard { run }
                    }
                }
            }
        }
    }
}

#[component]
fn EscapeHatchCard(run: bw_store::RunRow) -> Element {
    let eh = app_desktop::escape_hatch::build(&run);
    rsx! {
        div {
            style: "font-size:12px;font-family:{theme::MONO};background:{theme::CARD};border:1px solid {theme::BORDER};border-radius:8px;padding:8px 10px;",
            div { style: "color:{theme::INK_2};margin-bottom:4px;font-family:{theme::SANS};", "运行 {eh.run_label} · 进行中" }
            match eh.resume_command {
                Some(cmd) => rsx! {
                    div { style: "color:{theme::INK_3};font-family:{theme::SANS};", "上游会话:{eh.upstream_session.clone().unwrap_or_default()} · 暂无嵌入终端;在你自己的终端里续接:" }
                    div { style: "color:{theme::INK};margin-top:2px;", "{cmd}" }
                },
                None => rsx! {
                    div { style: "color:{theme::INK_3};font-family:{theme::SANS};", "这家不支持指派会话号,续接方式未知。" }
                },
            }
        }
    }
}

/// 段⑤:风险与决策。交棒记录流水,带险的置顶标红;决策栏本片不建,如
/// 实留白(design §1.2⑤)。
#[component]
fn RiskDecisionSegmentView(seg: RiskDecisionSegment) -> Element {
    rsx! {
        div {
            {section_title("⑤", "风险与决策")}
            if !seg.has_any_handoff {
                div { style: "{theme::card_dashed_gray()}", "一次交棒都没有。" }
            } else {
                div {
                    style: "display:flex;flex-direction:column;gap:6px;",
                    for h in seg.handoffs {
                        div {
                            style: if h.risky {
                                format!("font-size:12px;padding:8px 10px;border-radius:6px;background:#F6E7E2;color:{};border:1px solid {};", theme::signal_color(Signal::Red), theme::signal_color(Signal::Red))
                            } else {
                                format!("font-size:12px;padding:8px 10px;border-radius:6px;background:{};color:{};", theme::CARD_ALT, theme::INK_2)
                            },
                            span { style: "font-weight:600;", if h.risky { "带险交棒 · " } else { "交棒 · " } }
                            "{h.from_stage.label()} → {h.to_stage.label()}"
                            if !h.note.is_empty() {
                                span { " · {h.note}" }
                            }
                        }
                    }
                }
            }
            div { style: "font-size:11px;color:{theme::INK_4};margin-top:6px;", "{RiskDecisionSegment::DECISION_NOTE}" }
        }
    }
}

/// 段⑥:交付证据。三栏——运行账/观测出处/工作区现状(design §1.2⑥)。
#[component]
fn EvidenceSegmentView(seg: EvidenceSegment) -> Element {
    rsx! {
        div {
            {section_title("⑥", "交付证据")}
            div {
                style: "display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:12px;",
                div {
                    style: "{theme::card()}",
                    div { style: "font-weight:600;font-size:12px;margin-bottom:6px;", "运行账 · {seg.runs.len()} 条" }
                    if seg.runs.is_empty() {
                        div { style: "font-size:12px;color:{theme::INK_3};", "暂无运行。" }
                    } else {
                        for r in seg.runs.iter().take(8) {
                            div { style: "font-size:11px;color:{theme::INK_2};margin-bottom:3px;", "{r.state:?} · {r.connector_name} · 会话 {short_session(&r.upstream_session)}" }
                        }
                    }
                }
                div {
                    style: "{theme::card()}",
                    div { style: "font-weight:600;font-size:12px;margin-bottom:6px;", "观测出处 · {seg.observations.len()} 条" }
                    if seg.observations.is_empty() {
                        div { style: "font-size:12px;color:{theme::INK_3};", "暂无观测。" }
                    } else {
                        for o in seg.observations.iter().rev().take(8) {
                            div { style: "font-size:11px;color:{theme::INK_2};margin-bottom:3px;", "{o.raw_value} · {o.source}{hint_suffix(&o.source_hint)}" }
                        }
                    }
                }
                div {
                    style: "{theme::card()}",
                    div { style: "font-weight:600;font-size:12px;margin-bottom:6px;", "工作区此刻的真状态" }
                    match &seg.workspace_evidence {
                        Some(ev) => rsx! {
                            div { style: "font-size:11px;color:{theme::INK_2};", "提交数 {ev.commit_count} · 跟踪文件 {ev.tracked_files} · 未提交路径 {ev.dirty_paths} · docs/ 下 {ev.docs_files} 份文档" }
                            for s in ev.recent_subjects.iter().take(3) {
                                div { style: "font-size:11px;color:{theme::INK_3};margin-top:3px;", "· {s}" }
                            }
                        },
                        None => rsx! {
                            div { style: "font-size:12px;color:{theme::INK_3};", "工作区未配置。" }
                        },
                    }
                }
            }
        }
    }
}
