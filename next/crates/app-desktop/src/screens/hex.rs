//! 六段总控屏(`BW_PANEL=hex`,design-s5-hexpanel.md §1)。**屏上不做任
//! 何推导**——每一处显示的都是 `bw_app::view::hex::HexView`(或从它筛出
//! 来的既成事实,如逃生舱)里已经算好的字段,组件只管排版与格式化。没
//! 有数据的段照 §1.2 如实空(灰卡/说明文案),绝不假装绿。
//!
//! **next 切片七D**(design-s7-real-project.md §1「壳的活卡加『开工』
//! 『取消』」):段④加了两处点击——「待开工」列表里每件活一个「开工」
//! 按钮,逃生舱卡片(进行中的运行)加一个「取消」按钮。同待人处理屏
//! 「标记完成」按钮的既有分工:组件只把「点了哪件活/哪条运行」这个事
//! 实通过 `EventHandler` 报给上层,真正发命令的是 `main.rs::Root` 组装
//! 好的闭包(`ShellCommand::RunIssue`/`ShellCommand::CancelRun`,不是
//! `bw_app::Command`——理由见 `bw-app/src/cmd/issue.rs` 模块文档),壳这
//! 一层依旧零业务判断。
//!
//! **next 切片七-2 修**(评审 `task-s7b-review.md` Important-3):「待开
//! 工」列表里状态是「进行中」的那些活(一次运行结束/失败/被收拾之后仍
//! 然「进行中」,却没有一条活着的运行——见 `bw_app::view::hex::
//! LoopSegment::startable_issues` 字段文档)多一个「转回待办」按钮,只
//! 发**既有**命令(`bw_app::Command::TransitionIssue { to: Todo }`,同
//! 待人处理屏「标记完成」按钮走的同一条 `Dispatch` 路径),不发明新的
//! 状态转移——`(InProgress, Todo)` 本来就是 `bw_core::IssueStatus::
//! can_transition_to` 表里的合法边。壳依旧只把「点了哪件活」这个事实报
//! 给上层,合法性判断留给状态机自己守。

use bw_app::view::hex::{
    EvidenceSegment, FiveRolesSegment, LoopSegment, MetricCard, NorthStarView, RiskDecisionSegment,
    WorkspaceEvidenceState,
};
use bw_core::{IssueId, IssueStatus, RunId, Signal};
use bw_store::IssueRow;
use dioxus::prelude::*;

use crate::kernel::Vm;
use crate::screens::{run_end_summary, short_session};
use crate::theme;

#[component]
pub fn HexScreen(
    vm: Vm,
    on_run_issue: EventHandler<IssueId>,
    on_cancel_run: EventHandler<RunId>,
    on_revert_to_todo: EventHandler<IssueId>,
) -> Element {
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
            LoopSegmentView {
                seg: hex.loop_segment.clone(),
                open_runs: vm.open_runs.clone(),
                startable_issues: vm.startable_issues.clone(),
                on_run_issue,
                on_cancel_run,
                on_revert_to_todo,
            }
            RiskDecisionSegmentView { seg: hex.risk_decision.clone() }
            EvidenceSegmentView { seg: hex.evidence.clone() }
        }
    }
}

/// 观测出处提示的「 · 提示文本」后缀,空提示时不加后缀——**不能内联进
/// rsx! 的字符串插值**:`"...{if a { b } else { c }}..."` 这种块表达式
/// 嵌在带引号的插值文本里,dioxus 的 rsx! 解析器认不出来(实测过,报
/// 「Failed to parse formatted segment」),必须先拆成一个简单表达式
/// (函数调用)。
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
/// 运行卡带逃生舱(design §5.2)。**next 切片七D**加两处点击:待开工列
/// 表(design §1「壳的活卡加『开工』」)+ 逃生舱卡片上的「取消」。
#[component]
fn LoopSegmentView(
    seg: LoopSegment,
    open_runs: Vec<bw_store::RunRow>,
    startable_issues: Vec<IssueRow>,
    on_run_issue: EventHandler<IssueId>,
    on_cancel_run: EventHandler<RunId>,
    on_revert_to_todo: EventHandler<IssueId>,
) -> Element {
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
                // design-s7-real-project.md §1.5 第三行:「活的状态一个
                // 字不改」——运行结束不会把活推到评审中,真实使用时会
                // 看到「一次跑完了,活还停在进行中」。评审
                // task-s7b-review.md Important-4 点名这行说明此前没有
                // (免得被当成 bug)。
                div { style: "font-size:10px;color:{theme::INK_4};margin-bottom:10px;", "结束不等于干成:一次运行跑完(无论成没成),活的状态不会自动变——这是如实,不是漏做,「完成」永远等你显式点。" }
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
            if !startable_issues.is_empty() {
                div {
                    style: "margin-top:10px;display:flex;flex-direction:column;gap:6px;",
                    div { style: "font-size:11px;color:{theme::INK_3};", "待开工 · {startable_issues.len()} 件" }
                    for issue in startable_issues {
                        {
                            let id = issue.id;
                            // next 切片七-2 修(Important-3):「进行中且没
                            // 有活着的运行」的活多一个「转回待办」——只发
                            // 既有的 `TransitionIssue { to: Todo }`(合法
                            // 性由 `can_transition_to` 守,壳不判断),不
                            // 是给「开工」加条件、也不是发明新状态。
                            let stalled_in_progress = issue.status == IssueStatus::InProgress;
                            rsx! {
                                div {
                                    style: "font-size:12px;padding:7px 10px;border-radius:6px;background:{theme::CARD};border:1px solid {theme::BORDER};display:flex;align-items:center;gap:10px;",
                                    div {
                                        style: "flex:1;color:{theme::INK_2};",
                                        "#{issue.number} {issue.title}"
                                        if stalled_in_progress {
                                            span { style: "color:{theme::INK_4};", " · 进行中(没有活着的运行——上一次结束不等于干成,可重新开工或转回待办)" }
                                        }
                                    }
                                    if stalled_in_progress {
                                        button {
                                            style: "border:1px solid {theme::BORDER_DEEP};border-radius:6px;padding:4px 10px;font-size:12px;background:{theme::CARD_ALT};color:{theme::INK_2};",
                                            onclick: move |_| on_revert_to_todo.call(id),
                                            "转回待办"
                                        }
                                    }
                                    button {
                                        style: "border:none;border-radius:6px;padding:4px 10px;font-size:12px;font-weight:600;background:{theme::CLAY};color:#FFF;",
                                        onclick: move |_| on_run_issue.call(id),
                                        "开工"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !open_runs.is_empty() {
                div {
                    style: "margin-top:10px;display:flex;flex-direction:column;gap:6px;",
                    for run in open_runs {
                        EscapeHatchCard { run, on_cancel_run }
                    }
                }
            }
        }
    }
}

/// **逃生舱行保留,终端可用时也显示**(next 切片 5.5,design-s5-
/// hexpanel.md §5.2/§5.3,主控裁决 7:「终端是增强不是替代」)——续接命
/// 令是外部路径(在自己的终端里 `claude --resume`),嵌入终端是内部路径
/// (卡片里直接看/敲),两者并存如实,不因为有了嵌入终端就拿掉逃生舱那
/// 一行文案。`TerminalWidget` 自己决定要不要渲染任何东西(没有活会话
/// 时渲染空,§5.3 表「隐藏不等于停收」的另一面——这里连挂都不挂,不是
/// 挂了再隐藏),这张卡因此对没有活会话的运行(今天的默认情形,壳的连
/// 接器注册表是空的)是零回归的:界面上看到的和切片五收官时一模一样。
/// **next 切片七D**加一个「取消」按钮——只发 `ShellCommand::CancelRun`,
/// 合法性/幂等语义全部住在 `bw_app::cmd::issue::cancel_run` 里(design
/// §1「壳的活卡加『开工』『取消』」)。
#[component]
fn EscapeHatchCard(run: bw_store::RunRow, on_cancel_run: EventHandler<RunId>) -> Element {
    let eh = app_desktop::escape_hatch::build(&run);
    let run_id = run.id;
    rsx! {
        div {
            style: "font-size:12px;font-family:{theme::MONO};background:{theme::CARD};border:1px solid {theme::BORDER};border-radius:8px;padding:8px 10px;",
            div {
                style: "display:flex;align-items:center;gap:10px;margin-bottom:4px;",
                div { style: "flex:1;color:{theme::INK_2};font-family:{theme::SANS};", "运行 {eh.run_label} · 进行中" }
                button {
                    style: "border:1px solid {theme::BORDER_DEEP};border-radius:6px;padding:3px 10px;font-size:11px;background:{theme::CARD_ALT};color:{theme::INK_2};font-family:{theme::SANS};",
                    onclick: move |_| on_cancel_run.call(run_id),
                    "取消"
                }
            }
            match eh.resume_command {
                Some(cmd) => rsx! {
                    div { style: "color:{theme::INK_3};font-family:{theme::SANS};", "上游会话:{eh.upstream_session.clone().unwrap_or_default()} · 在你自己的终端里续接:" }
                    div { style: "color:{theme::INK};margin-top:2px;", "{cmd}" }
                },
                None => rsx! {
                    div { style: "color:{theme::INK_3};font-family:{theme::SANS};", "这家不支持指派会话号,续接方式未知。" }
                },
            }
            crate::screens::terminal::TerminalWidget { run: run.clone() }
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
                            div {
                                style: "margin-bottom:6px;",
                                div { style: "font-size:11px;color:{theme::INK_2};", "{r.state:?} · {r.connector_name} · 会话 {short_session(&r.upstream_session)}" }
                                // next 切片七-2 修(design-s7-real-project.md
                                // §1.5「失败如实显示的四种形态」,评审
                                // task-s7b-review.md Important-4)——结束事
                                // 实原文/耗时:数据本身(`end_detail`/
                                // `started_at`/`ended_at`)早就在运行行
                                // 里,此前没有任何一屏接进来显示;还在跑
                                // 的运行(`ended_at` 为空)没有这一行,
                                // `run_end_summary` 对这种情况如实返回空
                                // 串。
                                if r.ended_at.is_some() {
                                    div { style: "font-size:10px;color:{theme::INK_3};margin-top:2px;", "{run_end_summary(r)}" }
                                }
                            }
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
                        WorkspaceEvidenceState::Present(ev) => rsx! {
                            div { style: "font-size:11px;color:{theme::INK_2};", "提交数 {ev.commit_count} · 跟踪文件 {ev.tracked_files} · 未提交路径 {ev.dirty_paths} · docs/ 下 {ev.docs_files} 份文档" }
                            for s in ev.recent_subjects.iter().take(3) {
                                div { style: "font-size:11px;color:{theme::INK_3};margin-top:3px;", "· {s}" }
                            }
                        },
                        WorkspaceEvidenceState::NotConfigured => rsx! {
                            div { style: "font-size:12px;color:{theme::INK_3};", "工作区未配置。" }
                        },
                        // 终审必修 Minor s5c-4:这是与「未配置」不同的另一种空
                        // 态——项目配了检出根,但这次采证真的失败了,如实说
                        // 「采证失败」,不再假冒成「未配置」误导用户去检查一个
                        // 没问题的配置;错误原文摘进文案,方便定位。
                        WorkspaceEvidenceState::CollectFailed(err) => {
                            let amber = theme::signal_color(Signal::Amber);
                            rsx! {
                                div { style: "font-size:12px;color:{amber};", "工作区采证失败:{err}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
