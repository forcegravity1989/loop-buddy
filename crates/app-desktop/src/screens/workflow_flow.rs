//! L3(plan/11): the workflow's real full-process picture — not a bare chip
//! string, a pipeline that says where the generator sits, where the
//! evaluator sits, and where the loop-back really goes. Role classification
//! is an honest keyword read on each phase's own real name: a phase that
//! doesn't match anything renders neutral, never a guessed role. The
//! loop-back note only appears when `loop_max_iter > 1` — BW's own playbook
//! convention sets `max_iter: 1` for "one honest attempt, no blind rerun",
//! and that draws as a straight line with no loop, honestly.

use crate::theme;
use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq)]
enum PhaseRole {
    Generator,
    Evaluator,
    Optimizer,
    Neutral,
}

impl PhaseRole {
    fn classify(name: &str) -> Self {
        let has = |words: &[&str]| words.iter().any(|w| name.contains(w));
        if has(&["实现", "原型", "起草", "生成"]) {
            PhaseRole::Generator
        } else if has(&["评审", "验证", "测试", "回归", "verification"]) {
            PhaseRole::Evaluator
        } else if has(&["优化", "删减", "重构", "refine"]) {
            PhaseRole::Optimizer
        } else {
            PhaseRole::Neutral
        }
    }

    fn label(self) -> &'static str {
        match self {
            PhaseRole::Generator => "生成器",
            PhaseRole::Evaluator => "评估器",
            PhaseRole::Optimizer => "优化器",
            PhaseRole::Neutral => "",
        }
    }

    /// (bg, fg) — reuses the same family already established for these
    /// roles elsewhere in this app: CLAY for generation (matches the
    /// artifact panel's "代码" tint), the distillation-chip green for
    /// evaluation (matches L4's "⚗ 蒸馏自实战" tint), a warm amber for
    /// optimization (Build stage's own family).
    fn colors(self) -> (&'static str, &'static str) {
        match self {
            PhaseRole::Generator => ("#F2E4DD", theme::CLAY),
            PhaseRole::Evaluator => ("#EAF0E2", "#4A5E42"),
            PhaseRole::Optimizer => ("#F4E9D6", "#8C5A17"),
            PhaseRole::Neutral => ("#EFE9DA", theme::INK_2),
        }
    }
}

#[component]
pub fn WorkflowFlow(phases: Vec<String>, loop_retries: u8, loop_max_iter: u8) -> Element {
    let ink3 = theme::INK_3;
    let border = theme::BORDER;
    if phases.is_empty() {
        return rsx! { div { style: "font-size:12px;color:{ink3};", "没有定义的阶段。" } };
    }
    let roles: Vec<PhaseRole> = phases.iter().map(|p| PhaseRole::classify(p)).collect();
    let has_loop = loop_max_iter > 1;
    let loop_target = roles
        .iter()
        .position(|r| *r == PhaseRole::Generator)
        .and_then(|i| phases.get(i).cloned())
        .or_else(|| phases.first().cloned());

    rsx! {
        div {
            div {
                style: "display:flex;align-items:center;flex-wrap:wrap;gap:2px;",
                for (i , p) in phases.iter().enumerate() {
                    {
                        let role = roles[i];
                        let (bg, fg) = role.colors();
                        let role_label = role.label();
                        rsx! {
                            div {
                                key: "{i}",
                                style: "display:flex;align-items:center;",
                                div {
                                    style: "background:{bg};color:{fg};border-radius:8px;padding:8px 12px;",
                                    div { style: "font-size:12px;font-weight:500;", "{i + 1}. {p}" }
                                    if !role_label.is_empty() {
                                        div { style: "font-size:10px;opacity:.75;margin-top:2px;", "{role_label}" }
                                    }
                                }
                                if i + 1 < phases.len() {
                                    span { style: "color:{ink3};padding:0 6px;font-size:14px;", "→" }
                                }
                            }
                        }
                    }
                }
            }
            if has_loop {
                div {
                    style: "margin-top:10px;padding:8px 12px;border:1px dashed {border};border-radius:8px;font-size:11.5px;color:{ink3};",
                    if let Some(target) = &loop_target {
                        "↺ 未通过就退回「{target}」重来 · 最多 {loop_max_iter} 轮(每轮重试 {loop_retries} 次)"
                    } else {
                        "↺ 未通过就重来 · 最多 {loop_max_iter} 轮(每轮重试 {loop_retries} 次)"
                    }
                }
            } else {
                div {
                    style: "margin-top:10px;font-size:11px;color:{ink3};",
                    "直线管线 · 无循环(每步一次诚实尝试,不盲目重跑)"
                }
            }
        }
    }
}
