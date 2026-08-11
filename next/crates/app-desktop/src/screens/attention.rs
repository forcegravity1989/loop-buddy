//! 待人处理屏(`BW_PANEL=attention`,design-s5-hexpanel.md §3)。**推导投
//! 影,零自有表**——这里只把 `bw_app::view::attention::AttentionView`(已
//! 经算好的五项)渲染成列表;每一项的「自然消失条件」是数据层的事(见
//! `bw-app/src/view/attention.rs` 模块文档),这个组件不判断任何东西。
//!
//! **「完成永远人点」在这一屏的落点**(硬约束):②「评审中等你点」每一
//! 行带一个「标记完成」按钮,点击**只发** `bw_app::Command::
//! TransitionIssue { to: IssueStatus::Done }`——没有第二条路径能把一件
//! 活写成「已完成」(编排层那一侧的证据见 `bw-app/examples/
//! hex_readback.rs` §6「完成永远人点」一节的全库扫描断言)。`on_complete`
//! 是一个 `EventHandler<IssueId>`,不是把整个 `Kernel` 传下来——组件只
//! 拿到"点了哪件活"这一个事实,真正发命令的是 `main.rs::Root` 组装好的
//! 闭包(同一处也是唯一一处把 `Command::TransitionIssue` 摆上台面的地
//! 方,复核起来只用看这一处)。

use bw_app::view::AttentionView;
use bw_core::IssueId;
use dioxus::prelude::*;

use crate::kernel::Vm;
use crate::screens::{run_end_summary, short_session};
use crate::theme;

#[component]
pub fn AttentionScreen(vm: Vm, on_complete: EventHandler<IssueId>) -> Element {
    let Some(a) = vm.attention.clone() else {
        return rsx! {
            div { style: "color:{theme::INK_3};padding:24px;", "没有可显示的项目——先在顶栏选一个,或深链 BW_OPEN 指定一个真实存在的项目名。" }
        };
    };
    rsx! {
        div {
            style: "max-width:760px;margin:0 auto;display:flex;flex-direction:column;gap:14px;",
            div {
                style: "font-family:{theme::SERIF};font-weight:700;font-size:16px;",
                "需要你出手的事 · {a.total_count()} 条"
            }
            if a.is_quiet() {
                div {
                    style: "{theme::card_dashed_gray()}",
                    "暂时没有需要你出手的事。"
                }
            } else {
                UnsettledSection { a: a.clone() }
                InReviewSection { a: a.clone(), on_complete }
                StalledSection { a: a.clone() }
                MetricGapSection { a: a.clone() }
                RiskyHandoffSection { a }
            }
        }
    }
}

fn item_row(text: String, accent: &str) -> Element {
    rsx! {
        div {
            style: "font-size:13px;padding:9px 12px;border-radius:8px;background:{theme::CARD};border-left:3px solid {accent};",
            "{text}"
        }
    }
}

#[component]
fn UnsettledSection(a: AttentionView) -> Element {
    if a.unsettled_runs.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            div { style: "font-size:12px;font-weight:600;color:{theme::INK_2};margin-bottom:6px;", "① 遗留运行(关门没结账)· {a.unsettled_runs.len()} 条" }
            div {
                style: "display:flex;flex-direction:column;gap:6px;",
                for r in a.unsettled_runs {
                    {item_row(
                        format!(
                            "运行 {} · {:?} · 结束但未结算 · 会话 {} · {}",
                            r.id.uuid().to_string().chars().take(8).collect::<String>(),
                            r.state,
                            short_session(&r.upstream_session),
                            run_end_summary(&r),
                        ),
                        theme::signal_color(bw_core::Signal::Amber),
                    )}
                }
            }
        }
    }
}

#[component]
fn InReviewSection(a: AttentionView, on_complete: EventHandler<IssueId>) -> Element {
    if a.in_review_issues.is_empty() {
        return rsx! {};
    }
    let accent = theme::signal_color(bw_core::Signal::Amber);
    rsx! {
        div {
            div { style: "font-size:12px;font-weight:600;color:{theme::INK_2};margin-bottom:6px;", "② 评审中等你点 · {a.in_review_issues.len()} 条" }
            div {
                style: "display:flex;flex-direction:column;gap:6px;",
                for i in a.in_review_issues {
                    div {
                        style: "font-size:13px;padding:9px 12px;border-radius:8px;background:{theme::CARD};border-left:3px solid {accent};display:flex;align-items:center;gap:10px;",
                        div { style: "flex:1;", "#{i.number} {i.title} · 评审中" }
                        button {
                            style: "border:none;border-radius:6px;padding:5px 12px;font-size:12px;font-weight:600;background:{theme::CLAY};color:#FFF;",
                            onclick: move |_| on_complete.call(i.id),
                            "标记完成"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StalledSection(a: AttentionView) -> Element {
    if a.stalled_runs.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            div { style: "font-size:12px;font-weight:600;color:{theme::INK_2};margin-bottom:6px;", "③ 停在原地的运行 · {a.stalled_runs.len()} 条" }
            div {
                style: "display:flex;flex-direction:column;gap:6px;",
                for r in a.stalled_runs {
                    {item_row(
                        format!(
                            "运行 {} · {:?} · 没有更晚的运行接手 · 会话 {} · {}",
                            r.id.uuid().to_string().chars().take(8).collect::<String>(),
                            r.state,
                            short_session(&r.upstream_session),
                            run_end_summary(&r),
                        ),
                        theme::signal_color(bw_core::Signal::Red),
                    )}
                }
            }
        }
    }
}

#[component]
fn MetricGapSection(a: AttentionView) -> Element {
    if a.metrics_without_observation.is_empty() && a.metrics_stale.is_empty() {
        return rsx! {};
    }
    let n = a.metrics_without_observation.len() + a.metrics_stale.len();
    rsx! {
        div {
            div { style: "font-size:12px;font-weight:600;color:{theme::INK_2};margin-bottom:6px;", "④ 指标没数 / 数据过期 · {n} 条" }
            div {
                style: "display:flex;flex-direction:column;gap:6px;",
                for m in a.metrics_without_observation {
                    {item_row(format!("{} · 从没采到过观测", m.name), theme::signal_color(bw_core::Signal::Unknown))}
                }
                for m in a.metrics_stale {
                    {item_row(format!("{} · 最新观测已过期", m.name), theme::signal_color(bw_core::Signal::Amber))}
                }
            }
        }
    }
}

#[component]
fn RiskyHandoffSection(a: AttentionView) -> Element {
    if a.current_stage_risky_handoffs.is_empty() {
        return rsx! {};
    }
    rsx! {
        div {
            div { style: "font-size:12px;font-weight:600;color:{theme::INK_2};margin-bottom:6px;", "⑤ 当前这一棒的带险交棒欠账 · {a.current_stage_risky_handoffs.len()} 条" }
            div {
                style: "display:flex;flex-direction:column;gap:6px;",
                for h in a.current_stage_risky_handoffs {
                    {item_row(
                        format!(
                            "{} → {} · {}",
                            h.from_stage.label(),
                            h.to_stage.label(),
                            if h.note.is_empty() { "(无注记)" } else { h.note.as_str() },
                        ),
                        theme::signal_color(bw_core::Signal::Red),
                    )}
                }
            }
            div { style: "font-size:11px;color:{theme::INK_4};margin-top:4px;", "{bw_app::view::attention::CURRENT_STAGE_RISKY_HANDOFF_CAVEAT}" }
        }
    }
}
