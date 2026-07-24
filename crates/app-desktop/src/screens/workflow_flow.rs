//! L3(plan/11) + T8(plan/12 §4): the workflow's real full-process picture —
//! not a bare chip string, a pipeline that says where the generator sits,
//! where the evaluator sits, and where the loop-back really goes.
//!
//! T8 deleted the old "guess the role from the phase's Chinese name" keyword
//! heuristic (`实现/原型/起草/生成` ⇒ Generator, `评审/验证/测试/回归` ⇒
//! Evaluator, …). Role is now real, declared data on [`bw_core::model::
//! PhaseMeta`] — built-in stage playbooks (`bw_core::playbook::phase_metas`)
//! or `Neutral` for anything user-authored (the create/edit form is still
//! name-only text, no role-editing UI yet). A phase renders exactly the role
//! it was given, never a guess.
//!
//! The loop-back note only appears when `loop_max_iter > 1` — BW's own
//! playbook convention sets `max_iter: 1` for "one honest attempt, no blind
//! rerun", and that draws as a straight line with no loop, honestly, even if
//! a phase happens to carry the `Evaluator` role.
//!
//! T16 (plan/12 §10 v1.1#3) hangs each phase's real crew under its box: an
//! agent avatar+name and up to a few skill chips, resolved by name against
//! the real Agent/Skill Hub pools (`PhaseMeta.agent`/`.skills`) — nothing for
//! a phase that declares no binding. Clicking either jumps to that
//! component's detail via the same `ComponentSel` navigation `ProjectRail`
//! already drives (no second navigation system).

use crate::screens::component_detail::ComponentSel;
use crate::theme;
use bw_core::model::{PhaseMeta, PhaseRole};
use dioxus::prelude::*;
use ui::vm::{AgentCardVm, SkillCardVm};

fn role_label(role: PhaseRole) -> &'static str {
    match role {
        PhaseRole::Generator => "生成器",
        // The "special icon" the ticket asks for lives right in the label —
        // simplest honest way to put ⚖ on the phase box without a second
        // element to keep in sync.
        PhaseRole::Evaluator => "⚖ 评审门",
        PhaseRole::Optimizer => "优化器",
        PhaseRole::Neutral => "",
    }
}

/// (bg, fg) — reuses the same family already established for these roles
/// elsewhere in this app: CLAY for generation (matches the artifact panel's
/// "代码" tint), the distillation-chip green for evaluation (matches L4's
/// "⚗ 蒸馏自实战" tint — evaluation gates are a "quality passed" signal, same
/// family), a warm amber for optimization (Build stage's own family).
fn role_colors(role: PhaseRole) -> (&'static str, &'static str) {
    match role {
        PhaseRole::Generator => ("#F2E4DD", theme::CLAY),
        PhaseRole::Evaluator => ("#EAF0E2", "#4A5E42"),
        PhaseRole::Optimizer => ("#F4E9D6", "#8C5A17"),
        PhaseRole::Neutral => ("#EFE9DA", theme::INK_2),
    }
}

/// Resolve a phase's `agent` NAME against the real Agent Hub pool —
/// `(id, initial, name)` for the avatar dot + label; `None` for an unset or
/// dangling reference (never a guessed/invented agent).
fn resolve_agent(name: &str, pool: &[AgentCardVm]) -> Option<(bw_core::AgentId, String, String)> {
    pool.iter()
        .find(|a| a.name == name)
        .map(|a| (a.id, a.initial.clone(), a.name.clone()))
}

/// Resolve one skill NAME against the real Skill Hub pool — just the id (the
/// chip's own label text is the phase's real `sname`, not a re-fetched copy).
fn resolve_skill(name: &str, pool: &[SkillCardVm]) -> Option<bw_core::SkillId> {
    pool.iter().find(|s| s.name == name).map(|s| s.id)
}

/// One phase, at what column, evaluating whether it needs a reject-arc row
/// underneath it. `order` is this evaluator's position among all evaluators
/// in the spec (0-based) — used only to stagger arc depth so two reject
/// arcs in the same diagram don't draw directly on top of each other.
struct RejectArc {
    /// 0-based index of the evaluator phase itself.
    from: usize,
    /// `Some(target index)` — a **Static** workflow's fixed reject target.
    /// `None` — role is `Evaluator` but the target is undetermined (a
    /// **Dynamic** spec deferring to the not-yet-built T9 runtime verdict).
    to: Option<usize>,
    order: usize,
}

/// T16 (plan/12 §10 v1.1#3): the two hub pools `agents`/`skills` resolve a
/// phase's real by-NAME `agent`/`skills` references against — same
/// resolve-by-name convention `SkillAgentPicker`/`resolve_refs` already use
/// for `WorkflowSpec.agents`/`skills` itself, just applied per-phase.
/// `on_select` reuses the app's one existing "jump to a component's detail"
/// mechanism (`ComponentSel` + the root `sel`/`hub` signals `ProjectRail`'s
/// `on_pick` already drives) — no second navigation system.
#[component]
pub fn WorkflowFlow(
    phases: Vec<PhaseMeta>,
    loop_retries: u8,
    loop_max_iter: u8,
    agents: Vec<AgentCardVm>,
    skills: Vec<SkillCardVm>,
    on_select: EventHandler<ComponentSel>,
) -> Element {
    let ink3 = theme::INK_3;
    let ink2 = theme::INK_2;
    let border = theme::BORDER;
    if phases.is_empty() {
        return rsx! { div { style: "font-size:12px;color:{ink3};", "没有定义的阶段。" } };
    }
    let n = phases.len();
    let has_loop = loop_max_iter > 1;
    let reject_arcs: Vec<RejectArc> = phases
        .iter()
        .enumerate()
        .filter(|(_, p)| p.role == PhaseRole::Evaluator)
        .enumerate()
        .map(|(order, (i, p))| RejectArc {
            from: i,
            to: p.reject_to_phase.map(|t| t as usize),
            order,
        })
        .collect();

    // viewBox is one unit per phase column wide — `preserveAspectRatio:
    // none` stretches it to exactly match the equal-width grid columns
    // above it, so an arc's x always lands on its phase's real center with
    // no pixel math.
    let view_h: f32 = 16.0;
    let svg_height_px = 30 + reject_arcs.len() * 22;

    rsx! {
        div {
            // Equal-width columns (not the old content-sized flex row) so an
            // SVG overlay below can address each phase by a plain fractional
            // x — no measured-pixel bookkeeping.
            div {
                style: "display:grid;grid-template-columns:repeat({n}, 1fr);gap:0;",
                for (i , p) in phases.iter().enumerate() {
                    {
                        let (bg, fg) = role_colors(p.role);
                        let label = role_label(p.role);
                        // T16 (plan/12 §10 v1.1#3): resolve this phase's
                        // by-name `agent`/`skills` refs against the real hub
                        // pools — `None`/dangling name ⇒ no chip, never a
                        // placeholder (a phase with no real binding hangs
                        // nothing under its box).
                        let resolved_agent = p
                            .agent
                            .as_deref()
                            .and_then(|name| resolve_agent(name, &agents));
                        const MAX_SKILL_CHIPS: usize = 3;
                        let shown_skills: Vec<(usize, &String)> =
                            p.skills.iter().take(MAX_SKILL_CHIPS).enumerate().collect();
                        let extra_skills = p.skills.len().saturating_sub(MAX_SKILL_CHIPS);
                        let has_crew = resolved_agent.is_some() || !p.skills.is_empty();
                        rsx! {
                            div {
                                key: "{i}",
                                style: "padding:0 3px;",
                                div {
                                    style: "background:{bg};color:{fg};border-radius:8px;padding:8px 10px;text-align:center;",
                                    div { style: "font-size:12px;font-weight:500;", "{i + 1}. {p.name}" }
                                    if !label.is_empty() {
                                        div { style: "font-size:10px;opacity:.75;margin-top:2px;", "{label}" }
                                    }
                                }
                                // T16: agent avatar + skill chips — only for
                                // a phase that actually declares a binding;
                                // no placeholder row for one that doesn't.
                                if has_crew {
                                    div {
                                        style: "display:flex;align-items:center;justify-content:center;flex-wrap:wrap;gap:3px;margin-top:4px;",
                                        if let Some((aid, initial, aname)) = resolved_agent {
                                            span {
                                                title: "{aname} · 点击查看详情",
                                                style: "cursor:pointer;display:inline-flex;align-items:center;gap:3px;max-width:100%;",
                                                onclick: move |_| on_select.call(ComponentSel::Agent(aid)),
                                                span {
                                                    style: "width:14px;height:14px;border-radius:50%;background:{theme::AGENT};color:#FFF;display:inline-flex;align-items:center;justify-content:center;font-size:8px;font-weight:700;flex:none;",
                                                    "{initial}"
                                                }
                                                span {
                                                    style: "font-size:9.5px;color:{theme::AGENT};white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:70px;",
                                                    "{aname}"
                                                }
                                            }
                                        } else if let Some(name) = &p.agent {
                                            span {
                                                title: "「{name}」已不在智能体库中",
                                                style: "font-size:9.5px;color:{ink3};",
                                                "◆ {name}"
                                            }
                                        }
                                        for (si , sname) in shown_skills {
                                            {
                                                match resolve_skill(sname, &skills) {
                                                    Some(sid) => rsx! {
                                                        span {
                                                            key: "sk-{i}-{si}",
                                                            title: "{sname} · 点击查看详情",
                                                            style: "cursor:pointer;font-size:9px;padding:1px 5px;border-radius:5px;background:#EFE9DA;color:{ink2};white-space:nowrap;",
                                                            onclick: move |_| on_select.call(ComponentSel::Skill(sid)),
                                                            "{sname}"
                                                        }
                                                    },
                                                    None => rsx! {
                                                        span {
                                                            key: "sk-{i}-{si}",
                                                            title: "「{sname}」已不在技能库中",
                                                            style: "font-size:9px;padding:1px 5px;border-radius:5px;color:{ink3};white-space:nowrap;",
                                                            "{sname}"
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                        if extra_skills > 0 {
                                            span { style: "font-size:9px;color:{ink3};", "+{extra_skills}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if has_loop && !reject_arcs.is_empty() {
                svg {
                    width: "100%",
                    height: "{svg_height_px}",
                    view_box: "0 0 {n} {view_h}",
                    preserve_aspect_ratio: "none",
                    style: "display:block;margin-top:2px;overflow:visible;",
                    for arc in reject_arcs.iter() {
                        {render_reject_arc(arc, view_h)}
                    }
                }
                div {
                    style: "margin-top:6px;display:flex;flex-direction:column;gap:4px;",
                    for arc in reject_arcs.iter() {
                        {
                            let evaluator_name = &phases[arc.from].name;
                            let (border_style, target_text) = match arc.to {
                                Some(t) if t < phases.len() => (
                                    "solid",
                                    format!("退回「{}」(第{}阶)", phases[t].name, t + 1),
                                ),
                                Some(t) => ("solid", format!("退回第{}阶(索引越界,数据有误)", t + 1)),
                                None => (
                                    "dashed",
                                    "退回目标待定 ?(运行时由评审 agent 动态决定)".to_string(),
                                ),
                            };
                            rsx! {
                                div {
                                    key: "note-{arc.from}",
                                    style: "font-size:11px;color:{ink3};border:1px {border_style} {border};border-radius:6px;padding:4px 8px;",
                                    "「{evaluator_name}」未通过 → {target_text} · 循环 {loop_retries}/{loop_max_iter}"
                                }
                            }
                        }
                    }
                }
            } else if has_loop {
                div {
                    style: "margin-top:10px;font-size:11px;color:{ink3};",
                    "↺ 未通过就重来 · 最多 {loop_max_iter} 轮(每轮重试 {loop_retries} 次;本流程没有声明评审门)"
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

/// One reject arc's SVG: a curve from the evaluator phase's column down and
/// back to the target's column (solid, arrowhead pointing into the target
/// box) — or, when the target is undetermined, a small dashed self-loop
/// under the evaluator's own column with a "?" mark.
fn render_reject_arc(arc: &RejectArc, view_h: f32) -> Element {
    let stroke = theme::INK_3;
    let x_from = arc.from as f32 + 0.5;

    let Some(to) = arc.to else {
        // Undetermined target (Dynamic workflow) — a small dashed self-loop
        // hanging under the evaluator's own box, marked "?".
        let bump = (view_h * 0.55).min(6.0);
        return rsx! {
            g {
                key: "arc-{arc.from}",
                path {
                    d: "M {x_from - 0.28},0 Q {x_from},{bump} {x_from + 0.28},0",
                    fill: "none",
                    stroke: "{stroke}",
                    "stroke-width": "0.06",
                    "stroke-dasharray": "0.12,0.1",
                },
                text {
                    x: "{x_from}",
                    y: "{bump + 1.6}",
                    "font-size": "1.3",
                    "text-anchor": "middle",
                    fill: "{stroke}",
                    "?"
                }
            }
        };
    };

    let x_to = to as f32 + 0.5;
    let span = (x_to - x_from).abs();
    let depth = (4.0 + 2.0 * span + arc.order as f32 * 2.5).min(view_h - 2.0);
    let mid = (x_from + x_to) / 2.0;
    // Arrowhead: a small filled triangle, tip touching the target column at
    // y=0 (the phase row's bottom edge), base a little below it — reads as
    // "points up into that box" regardless of which direction the arc came
    // from.
    let dir = if x_to > x_from { 1.0 } else { -1.0 };
    let tip = (x_to, 0.0_f32);
    let base_l = (x_to - 0.16 * dir, 0.55);
    let base_r = (x_to + 0.16 * dir, 0.15);

    rsx! {
        g {
            key: "arc-{arc.from}",
            path {
                d: "M {x_from},0 Q {mid},{depth} {x_to},0",
                fill: "none",
                stroke: "{stroke}",
                "stroke-width": "0.06",
            },
            polygon {
                points: "{tip.0},{tip.1} {base_l.0},{base_l.1} {base_r.0},{base_r.1}",
                fill: "{stroke}",
            }
        }
    }
}
