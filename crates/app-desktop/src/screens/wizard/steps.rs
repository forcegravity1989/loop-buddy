//! The eight wizard step bodies (prototype rows 110–625). Presentational steps
//! (0/1/2/6) render static warmth; input steps (3/4/5/7) bind editable fields to
//! the parent's [`WizState`] signal. The shared next/back controls live in the
//! parent's pinned frame footer, not in these bodies (step 0 supplies its own
//! clay CTA in-body).
//!
//! Styling mirrors `screens/projects.rs`: `theme::` consts + inline `style:`.
//! A few prototype-specific hexes that aren't in the token set (warm accent
//! reds `#B0503A`/`#7A3D2D`, the dark insight panel `#23211C` surfaces, the
//! prototype's tint backgrounds) are inlined here verbatim to keep the port
//! faithful — they live only in this presentational module.

// `bw_core::Signal` (the derived health enum) is aliased so the bare `Signal<T>`
// always means the Dioxus reactive signal.
use bw_app::Command;
use bw_core::model::{LoopConfig, Maturity, Role, StageKind, WorkflowKind, WorkflowSpec};
use bw_core::Signal as HealthSignal;
use bw_core::{SessionId, WorkflowId};
use bw_store::SessionKind;
use dioxus::prelude::*;

use super::WizState;
use crate::bridge::{CommandBus, SessionMsgVM, ViewModel};
use crate::theme;

// ── shared little building blocks ────────────────────────────────────────────

/// The clay step-eyebrow ("步骤 0N · …") used by every non-intro step's left rail.
#[component]
fn Eyebrow(text: String) -> Element {
    rsx! {
        div {
            style: "font:600 12px/1 {theme::FONT_MONO};letter-spacing:.18em;text-transform:uppercase;\
                    color:{theme::CLAY};margin-bottom:14px;",
            "{text}"
        }
    }
}

/// The clay 「控制点」 card that closes each step's sticky left rail.
#[component]
fn ControlPoint(text: String) -> Element {
    rsx! {
        div {
            style: "background:#F2E4DD;border-radius:{theme::RADIUS_SM};padding:16px 18px;",
            div {
                style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.12em;text-transform:uppercase;\
                        color:#B0503A;margin-bottom:8px;",
                "控制点"
            }
            div {
                style: "font:500 14px/1.6 {theme::FONT_SANS};color:#7A3D2D;",
                "{text}"
            }
        }
    }
}

/// A collapsed-by-default 「为什么」 drawer: demotes a step's method teaching out
/// of the always-on rail so the action stays above the fold (plan: 保留但降级).
/// Click the summary to expand; default closed.
#[component]
fn Disclosure(summary: String, children: Element) -> Element {
    let mut open = use_signal(|| false);
    let chevron = if open() { "收起 ▴" } else { "▾" };
    rsx! {
        div { style: "margin:2px 0 16px;",
            button {
                onclick: move |_| open.with_mut(|o| *o = !*o),
                style: "display:flex;align-items:center;justify-content:space-between;gap:8px;width:100%;\
                        background:{theme::CARD_BG};border:1px solid {theme::BORDER};\
                        border-radius:{theme::RADIUS_SM};padding:11px 13px;cursor:pointer;text-align:left;\
                        color:{theme::INK_2};",
                span { style: "font:500 12.5px/1.3 {theme::FONT_SANS};", "{summary}" }
                span { style: "font:500 11px/1 {theme::FONT_MONO};color:{theme::INK_3};", "{chevron}" }
            }
            if open() {
                div { style: "padding:14px 2px 0;", {children} }
            }
        }
    }
}

/// The two-column shell (340px sticky rail + content) shared by steps 1–7.
const TWO_COL: &str = "max-width:1180px;margin:0 auto;padding:48px 40px 56px;display:grid;\
                       grid-template-columns:340px 1fr;gap:56px;align-items:start;";
const STICKY: &str = "position:sticky;top:24px;";

// ════════════════════════════════════════════════════════════════════════════
// STEP 0 · 引子
// ════════════════════════════════════════════════════════════════════════════

#[component]
pub fn Step0Intro() -> Element {
    let bus = use_context::<crate::bridge::CommandBus>();

    // The four control-point cards (2×2).
    let points: [(&str, &str, &str); 4] = [
        (
            "01",
            "知道对标谁",
            "每个项目都有明确的竞品与差距,否则不进入开发。",
        ),
        (
            "02",
            "每周在正常演进",
            "健康信号来自真实数据,不靠口头汇报,一眼可判断。",
        ),
        (
            "03",
            "让 agent loop 干活",
            "人只在质量门 / 验收处介入,不微管理;验收信号必须足够可信。",
        ),
        (
            "04",
            "目标清晰且难造假",
            "北极星唯一;引领指标可控、可被真实统计。",
        ),
    ];

    rsx! {
        div {
            style: "max-width:1000px;margin:0 auto;padding:72px 40px 96px;",
            div {
                style: "font:600 12px/1 {theme::FONT_MONO};letter-spacing:.2em;text-transform:uppercase;\
                        color:{theme::CLAY};margin-bottom:22px;",
                "从零开始 · 不是看板,是方法"
            }
            h1 {
                style: "font:600 46px/1.18 {theme::FONT_SERIF};margin:0 0 24px;letter-spacing:-.01em;max-width:18em;",
                "用 AI 时代的方式,"
                br {}
                "一步步把一个项目的管理体系搭起来"
            }
            p {
                style: "font:400 18px/1.85 {theme::FONT_SANS};color:{theme::INK_2};max-width:42em;margin:0 0 14px;",
                "我们会以一个真实样板项目 —— "
                b { style: "color:{theme::INK};font-weight:600;", "「模型 API 服务 · 运维运营看板」" }
                " —— 从头走一遍:竞品洞察(证据→发现→洞察)→ 需求导入 → 北极星 → 引领 / 滞后指标 → 原型 → 进度管理。每一步都先讲「为什么」,再动手做。走完,你就拥有了一套可复制的项目管理方法,而不只是一块看板。"
            }

            // ── comparison card (传统 vs Builders) ──────────────────────────
            div {
                style: "margin-top:56px;display:grid;grid-template-columns:1fr 1fr;gap:0;\
                        border:1px solid {theme::BORDER};border-radius:{theme::RADIUS_SM};\
                        overflow:hidden;background:{theme::CARD_BG};",
                // 传统 (left, with strike-throughs)
                div {
                    style: "padding:32px 34px;border-right:1px solid {theme::BORDER};",
                    div {
                        style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.16em;text-transform:uppercase;\
                                color:{theme::PLACEHOLDER};margin-bottom:18px;",
                        "传统项目管理 · 示意 ~10 流程 / 5 角色"
                    }
                    div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:10px;", "角色" }
                    div {
                        style: "font:400 14px/1.9 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:22px;",
                        "产品经理 · 项目经理 · 技术负责人 · 设计师 · 测试 QA"
                        br {}
                        span { style: "font-size:12.5px;color:{theme::PLACEHOLDER};",
                            "信息要靠这些角色之间反复开会、人工汇报才能流动。"
                        }
                    }
                    div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:12px;", "流程" }
                    div {
                        style: "display:flex;flex-direction:column;gap:9px;",
                        TradLine { text: "① 需求收集", struck: false }
                        TradLine { text: "② 撰写 10 页 PRD", struck: true }
                        TradLine { text: "③ 层层需求评审", struck: true }
                        TradLine { text: "④ 排期 · 甘特图", struck: true }
                        TradLine { text: "⑤ 设计交付", struck: false }
                        TradLine { text: "⑥ 人工逐行开发", struck: false }
                        TradLine { text: "⑦ 测试 QA", struck: false }
                        TradLine { text: "⑧ 发布", struck: false }
                        TradLine { text: "⑨ 状态周会 · 口头汇报", struck: true }
                        TradLine { text: "⑩ 复盘", struck: false }
                    }
                }
                // Builders (right)
                div {
                    style: "padding:32px 34px;background:#F7F2EC;",
                    div {
                        style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.16em;text-transform:uppercase;\
                                color:{theme::CLAY};margin-bottom:18px;",
                        "AI 时代 · Builders 模式"
                    }
                    div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:10px;", "角色收敛" }
                    div {
                        style: "font:400 14px/1.9 {theme::FONT_SANS};color:{theme::INK_2};margin-bottom:22px;",
                        b { style: "font-weight:600;color:{theme::INK};",
                            "1 个 Builder(系统设计者,端到端 own)+ Agent Loop。"
                        }
                        br {}
                        span { style: "font-size:12.5px;color:{theme::INK_3};",
                            "每个人职责范围更大;OpenAI 把团队当「分形小创业公司」,Cursor 50 人没有专职 PM。"
                        }
                    }
                    div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:12px;", "流程精简(被划掉的,靠 agent 与真实数据替代)" }
                    div {
                        style: "display:flex;flex-direction:column;gap:11px;",
                        ReplaceLine { keep: "② ③ PRD + 评审", to: "→ 原型即规格" }
                        ReplaceLine { keep: "④ 甘特图", to: "→ 每周可验证增量(≤90 天视野)" }
                        ReplaceLine { keep: "⑥ 人工实现", to: "→ agent loop 产出 80%,人审 20%" }
                        ReplaceLine { keep: "⑨ 状态周会", to: "→ 真实 telemetry,难造假" }
                        div { style: "height:1px;background:{theme::BORDER};margin:6px 0;" }
                        div {
                            style: "font:400 13.5px/1.5 {theme::FONT_SANS};color:{theme::INK_3};",
                            "保留并强化 → 竞品对标 · 北极星对齐 · 引领 / 滞后指标 · 原型 · 复盘"
                        }
                    }
                }
            }

            // ── 4 control-point cards (2×2) ─────────────────────────────────
            div {
                style: "margin-top:52px;",
                div {
                    style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.16em;text-transform:uppercase;\
                            color:{theme::PLACEHOLDER};margin-bottom:18px;",
                    "新管理方式下的 4 个控制点 · 辅助,不限制"
                }
                div {
                    style: "display:grid;grid-template-columns:1fr 1fr;gap:14px;",
                    for (n , title , body) in points {
                        div {
                            style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};\
                                    border-radius:{theme::RADIUS_SM};padding:22px 24px;",
                            div {
                                style: "font:700 18px/1 {theme::FONT_MONO};color:{theme::CLAY};margin-bottom:12px;",
                                "{n}"
                            }
                            div {
                                style: "font:600 16px/1.4 {theme::FONT_SERIF};margin-bottom:8px;",
                                "{title}"
                            }
                            div {
                                style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_3};",
                                "{body}"
                            }
                        }
                    }
                }
            }

            // ── CTA → step 1 ────────────────────────────────────────────────
            div {
                style: "margin-top:48px;display:flex;align-items:center;gap:18px;",
                button {
                    onclick: move |_| bus.send(bw_app::Command::SetWizardStep { step: 1 }),
                    style: "background:{theme::CLAY};color:#fff;border:none;border-radius:{theme::RADIUS_SM};\
                            padding:15px 30px;font:600 15px/1 {theme::FONT_SANS};cursor:pointer;letter-spacing:.02em;",
                    "开始创建项目体系 →"
                }
                span {
                    style: "font:400 13.5px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};",
                    "约 7 步 · 全程以样板项目示范"
                }
            }
        }
    }
}

#[component]
fn TradLine(text: String, struck: bool) -> Element {
    let color = if struck { "#B6AE9E" } else { theme::INK_2 };
    let deco = if struck { "line-through" } else { "none" };
    rsx! {
        div {
            style: "font:400 14px/1.4 {theme::FONT_SANS};color:{color};text-decoration:{deco};",
            "{text}"
        }
    }
}

#[component]
fn ReplaceLine(keep: String, to: String) -> Element {
    rsx! {
        div {
            style: "font:400 14px/1.5 {theme::FONT_SANS};color:{theme::INK};",
            "{keep} "
            span { style: "color:{theme::CLAY};", "{to}" }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// STEP 1 · 竞品洞察 (workflow-driven: this control point's session IS the deliverable)
// ════════════════════════════════════════════════════════════════════════════

/// The WorkflowHub-recommended 竞品洞察 workflow seeded for this control point
/// (`WorkflowKind::Static` → 成熟 · v3 · 复用 12 次). Its `phases` drive the
/// executor: each completed phase appends one Agent message to the session
/// transcript. With the MockExecutor that text is a canned stub; the real
/// per-phase content arrives when the colleague team's executor swaps in.
fn competitor_insight_workflow() -> WorkflowSpec {
    WorkflowSpec {
        id: WorkflowId::new(),
        name: "竞品洞察工作流".into(),
        kind: WorkflowKind::Static {
            maturity: Maturity::Mature,
            version: 3,
            uses: 12,
            scope: "竞品洞察".into(),
        },
        prompt: "界定 → 采集 → 结构化 → 分析 → 洞察".into(),
        goal: "产出可证伪的竞品洞察(证据 → 发现 → 洞察)".into(),
        stage_ref: Some(1),
        phases: CI_PHASES.iter().map(|p| (*p).into()).collect(),
        agents: Vec::new(),
        skills: Vec::new(),
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
    }
}

/// The five control-point phases — the strip shown in the picker (upcoming) and
/// the canvas (done), and the source of the workflow's `phases` above.
const CI_PHASES: [&str; 5] = ["界定", "采集", "结构化", "分析", "洞察"];

/// Step 1 · 竞品洞察. The teaching rail demotes its method into a [`Disclosure`];
/// the content column is no longer a static essay but the **workflow session**
/// itself — picked from WorkflowHub (or 自建), run on demand, its transcript the
/// deliverable. Reads `active_session` / `session_msgs` from the [`ViewModel`].
#[component]
pub fn Step1Insight() -> Element {
    let vm = use_context::<Signal<ViewModel>>();
    let v = vm();
    let session = v.active_session;
    let msgs = v.session_msgs;

    rsx! {
        // Full-height step: teaching rail (left) + session canvas (right). The
        // canvas transcript is the ONLY inner scroll — the step fills the frame
        // body exactly, so the page never grows (plan: 不滑屏 + 会话即交付).
        div {
            style: "height:100%;max-width:1180px;margin:0 auto;padding:22px 40px;display:grid;\
                    grid-template-columns:316px 1fr;gap:36px;align-items:stretch;min-height:0;",

            // ── teaching rail (identity stays; method folds into a drawer) ──────
            div {
                style: "min-height:0;overflow-y:auto;display:flex;flex-direction:column;",
                Eyebrow { text: "步骤 01 · 竞品洞察" }
                h2 {
                    style: "font:600 27px/1.32 {theme::FONT_SERIF};margin:0 0 14px;",
                    "从证据,"
                    br {}
                    "一步步爬到判断"
                }
                Disclosure { summary: "为什么这么做 · 方法讲解",
                    p {
                        style: "font:400 14px/1.8 {theme::FONT_SANS};color:{theme::INK_2};margin:0 0 14px;",
                        "竞品分析是一条 "
                        b { style: "color:{theme::INK};font-weight:600;", "证据 → 发现 → 洞察" }
                        " 的链条:先共情采集证据,再结构化摆事实,追问「所以呢」提炼发现,最后收敛成可证伪的判断。洞察是被推导出来的,不是被宣布的。"
                    }
                    div {
                        style: "border-left:2px solid {theme::SCROLL_THUMB};padding:4px 0 4px 16px;",
                        div { style: "font:500 12px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};margin-bottom:6px;", "传统 → AI" }
                        div {
                            style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_2};",
                            "市场团队数周访谈 + 厚报告 → Builder 用 agent 跑通采集与结构化,人只在「发现 → 洞察」这一跳介入。"
                        }
                    }
                }
                // The key human-intervention constraint stays pinned (short).
                div {
                    style: "background:#F2E4DD;border-radius:{theme::RADIUS_SM};padding:15px 17px;margin-top:auto;",
                    div {
                        style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.12em;text-transform:uppercase;\
                                color:#B0503A;margin-bottom:8px;",
                        "GATE · 人工介入点"
                    }
                    div {
                        style: "font:500 13.5px/1.6 {theme::FONT_SANS};color:#7A3D2D;",
                        "「发现 → 洞察」由你把关;采集与结构化交给 agent。"
                    }
                }
            }

            // ── session canvas = this control point's deliverable ───────────────
            match session {
                Some(sess) => rsx! { SessionCanvas { session: sess, msgs } },
                None => rsx! { WorkflowPicker {} },
            }
        }
    }
}

/// Pre-run state: the recommended workflow + a source toggle + a 运行 button.
/// Clicking 运行 starts the session and runs the workflow — explicit, not
/// auto-run, so the builder confirms Hub-vs-自建 before anything executes.
#[component]
fn WorkflowPicker() -> Element {
    let bus = use_context::<CommandBus>();
    rsx! {
        div {
            style: "display:flex;flex-direction:column;min-height:0;background:{theme::CARD_BG};\
                    border:1px solid {theme::BORDER};border-radius:10px;overflow:hidden;",

            div {
                style: "flex:none;padding:14px 18px;border-bottom:1px solid {theme::BORDER};\
                        display:flex;align-items:center;justify-content:space-between;gap:10px;",
                div { style: "font:600 15px/1.3 {theme::FONT_SERIF};", "竞品洞察工作流" }
                div {
                    style: "display:flex;gap:6px;flex:none;",
                    span {
                        style: "background:#F2E4DD;color:#B0503A;border:1px solid #E6D2C8;border-radius:6px;\
                                padding:5px 10px;font:600 11.5px/1 {theme::FONT_SANS};",
                        "WorkflowHub · 推荐"
                    }
                    span {
                        style: "background:{theme::CARD_BG_2};color:{theme::INK_3};border:1px solid {theme::BORDER};\
                                border-radius:6px;padding:5px 10px;font:500 11.5px/1 {theme::FONT_SANS};",
                        "自建"
                    }
                }
            }

            div {
                style: "flex:1;min-height:0;overflow-y:auto;padding:18px;",
                div {
                    style: "font:500 11px/1 {theme::FONT_MONO};color:{theme::INK_3};margin-bottom:14px;",
                    "成熟 · v3 · 复用 12 次 · 来源 WorkflowHub"
                }
                div {
                    style: "display:flex;align-items:center;flex-wrap:wrap;gap:7px;margin-bottom:18px;",
                    for (i , name) in CI_PHASES.iter().enumerate() {
                        div {
                            key: "{i}",
                            style: "display:flex;align-items:center;gap:7px;",
                            if i > 0 {
                                span { style: "color:{theme::SCROLL_THUMB};", "›" }
                            }
                            span {
                                style: "background:{theme::CARD_BG_2};border:1px solid {theme::BORDER};border-radius:6px;\
                                        padding:6px 11px;font:500 12.5px/1 {theme::FONT_SANS};color:{theme::INK_2};",
                                "{name}"
                            }
                        }
                    }
                }
                div {
                    style: "font:400 13px/1.7 {theme::FONT_SANS};color:{theme::INK_3};",
                    "运行后,agent 按 5 个阶段推进,产物会落进下面这场会话 —— "
                    b { style: "color:{theme::INK_2};font-weight:600;", "这场会话就是本环节的交付件" }
                    "。需要时可在「发现 → 洞察」处接管。"
                }
            }

            div {
                style: "flex:none;padding:13px 18px;border-top:1px solid {theme::BORDER};\
                        display:flex;align-items:center;justify-content:space-between;gap:12px;",
                span { style: "font:400 12px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};", "用 Hub 推荐工作流开跑,也可改为自建" }
                button {
                    onclick: move |_| {
                        let sess = SessionId::new();
                        bus.send(Command::StartSession {
                            id: sess,
                            stage_kind: Some(StageKind::CompetitorInsight),
                            kind: SessionKind::Create,
                            title: "竞品洞察 · 首轮".into(),
                        });
                        bus.send(Command::RunWorkflow {
                            session: sess,
                            spec: competitor_insight_workflow(),
                        });
                    },
                    style: "background:{theme::CLAY};color:#fff;border:none;border-radius:{theme::RADIUS_SM};\
                            padding:12px 22px;font:600 14px/1 {theme::FONT_SANS};cursor:pointer;",
                    "运行工作流 →"
                }
            }
        }
    }
}

/// Post-run state: the workflow header + the transcript (the deliverable) + a
/// reply box. Each phase output and builder reply is a persisted message read
/// back through the [`ViewModel`].
#[component]
fn SessionCanvas(session: SessionId, msgs: Vec<SessionMsgVM>) -> Element {
    let bus = use_context::<CommandBus>();
    let mut draft = use_signal(String::new);
    let dv = draft();

    rsx! {
        div {
            style: "display:flex;flex-direction:column;min-height:0;background:{theme::CARD_BG};\
                    border:1px solid {theme::BORDER};border-radius:10px;overflow:hidden;",

            div {
                style: "flex:none;padding:13px 18px;border-bottom:1px solid {theme::BORDER};",
                div {
                    style: "display:flex;align-items:center;justify-content:space-between;gap:10px;margin-bottom:9px;",
                    div { style: "font:600 15px/1.3 {theme::FONT_SERIF};", "竞品洞察工作流" }
                    span {
                        style: "background:#F2E4DD;color:#B0503A;border:1px solid #E6D2C8;border-radius:6px;\
                                padding:4px 9px;font:600 11px/1 {theme::FONT_SANS};",
                        "WorkflowHub · 推荐"
                    }
                }
                div {
                    style: "display:flex;align-items:center;flex-wrap:wrap;gap:8px;",
                    span { style: "font:500 10.5px/1 {theme::FONT_MONO};color:{theme::INK_3};", "成熟 · v3" }
                    for (i , name) in CI_PHASES.iter().enumerate() {
                        span { key: "{i}", style: "font:500 11px/1 {theme::FONT_SANS};color:#5F7355;", "✓ {name}" }
                    }
                }
            }

            div {
                style: "flex:1;min-height:0;overflow-y:auto;padding:15px 16px;display:flex;\
                        flex-direction:column;gap:11px;",
                for (i , m) in msgs.iter().enumerate() {
                    MsgBubble { key: "{i}", role: m.role, text: m.text.clone() }
                }
            }

            div {
                style: "flex:none;padding:11px 13px;border-top:1px solid {theme::BORDER};\
                        display:flex;align-items:center;gap:9px;background:{theme::CARD_BG_2};",
                input {
                    value: "{dv}",
                    oninput: move |e| draft.with_mut(|d| *d = e.value()),
                    placeholder: "追问、补充证据,或在 GATE 处下判断…",
                    style: "flex:1;background:#fff;border:1px solid {theme::BORDER_2};border-radius:8px;\
                            padding:10px 13px;font:400 13px/1.3 {theme::FONT_SANS};color:{theme::INK};outline:none;",
                }
                button {
                    onclick: move |_| {
                        let text = draft();
                        if !text.trim().is_empty() {
                            bus.send(Command::SendSessionMessage { session, text });
                            draft.with_mut(|d| d.clear());
                        }
                    },
                    style: "flex:none;width:36px;height:36px;border-radius:8px;background:{theme::CLAY};\
                            color:#fff;border:none;cursor:pointer;font:700 16px/1 {theme::FONT_MONO};",
                    "↑"
                }
            }
        }
    }
}

/// One transcript bubble: Agent = left / white, Builder = right / dark ink.
#[component]
fn MsgBubble(role: Role, text: String) -> Element {
    let style = match role {
        Role::Agent => format!(
            "max-width:86%;align-self:flex-start;background:#fff;border:1px solid {};\
             border-radius:10px 10px 10px 3px;padding:10px 13px;font:400 12.5px/1.65 {};color:#3A3833;",
            theme::BORDER, theme::FONT_SANS
        ),
        Role::Builder => format!(
            "max-width:82%;align-self:flex-end;background:{};color:#F3EEE6;\
             border-radius:10px 10px 3px 10px;padding:10px 13px;font:400 12.5px/1.65 {};",
            theme::INK, theme::FONT_SANS
        ),
    };
    rsx! {
        div { style: "{style}", "{text}" }
    }
}


// ════════════════════════════════════════════════════════════════════════════
// STEP 2 · 需求导入 (presentational)
// ════════════════════════════════════════════════════════════════════════════

#[component]
pub fn Step2Requirement() -> Element {
    let stories: [&str; 3] = [
        "作为值班工程师,我希望在一个视图里看到可用性 / 延迟 / 成本 / 进行中事故,以便不再切 4 个工具。",
        "作为值班工程师,我希望异常发生时拿到 agent 给的根因建议,以便更快定位。",
        "作为负责人,我希望看到成本按模型 / 租户归因,以便判断哪个用法在烧钱。",
    ];

    rsx! {
        div { style: "{TWO_COL}",
            div { style: "{STICKY}",
                Eyebrow { text: "步骤 02 · 需求导入" }
                div {
                    style: "display:inline-flex;align-items:center;gap:6px;background:#F2E4DD;border-radius:5px;\
                            padding:5px 10px;margin-bottom:14px;",
                    span { style: "font:600 9px/1 {theme::FONT_MONO};letter-spacing:.06em;color:#B0503A;", "侧输入流" }
                    span { style: "font:400 11px/1 {theme::FONT_SANS};color:#7A3D2D;", "与竞品发现做 reconcile" }
                }
                h2 { style: "font:600 30px/1.3 {theme::FONT_SERIF};margin:0 0 18px;", "把需求收敛成「问题」" }
                Disclosure { summary: "为什么这么做 · 方法讲解",
                    p {
                        style: "font:400 15px/1.85 {theme::FONT_SANS};color:{theme::INK_2};margin:0 0 16px;",
                        "不写 10 页 PRD。「原型即规格」——需求以一句问题陈述 + 几条用户故事的轻量形式导入,足够让 agent 直接开始做原型。"
                    }
                    div {
                        style: "border-left:2px solid {theme::SCROLL_THUMB};padding:4px 0 4px 16px;margin:6px 0 0;",
                        div { style: "font:500 12px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};margin-bottom:6px;", "传统 → AI" }
                        div { style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_2};", "10 页 PRD + 层层评审 → 一句问题陈述 + 用户故事 + 一个可验证的验收信号。" }
                    }
                }
                ControlPoint { text: "需求收敛为「可被验证的问题」,而不是一张功能清单。" }
            }

            div {
                div {
                    style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};border-radius:8px;padding:26px 28px;",
                    div { style: "font:500 12px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:10px;", "问题陈述 · 一句话" }
                    div {
                        style: "font:400 16px/1.7 {theme::FONT_SERIF};color:{theme::INK};",
                        "值班工程师要跨 4 个工具,才能判断服务是否健康、异常在哪、花了多少钱;定位一次异常平均要 38 分钟。"
                    }
                    div { style: "height:1px;background:{theme::PROGRESS_TRACK};margin:18px 0;" }
                    div { style: "font:500 12px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:14px;", "用户故事" }
                    div {
                        style: "display:flex;flex-direction:column;gap:12px;",
                        for (i , story) in stories.iter().enumerate() {
                            div {
                                style: "display:flex;gap:12px;align-items:flex-start;",
                                div {
                                    style: "width:20px;height:20px;border-radius:5px;background:#F2E4DD;color:{theme::CLAY};\
                                            font:600 11px/20px {theme::FONT_MONO};text-align:center;flex:none;",
                                    "{i + 1}"
                                }
                                div { style: "font:400 14.5px/1.7 {theme::FONT_SANS};color:#3A3833;", "{story}" }
                            }
                        }
                    }
                    div { style: "height:1px;background:{theme::PROGRESS_TRACK};margin:18px 0;" }
                    div {
                        style: "display:flex;align-items:center;gap:10px;background:#F7EDE7;border-radius:{theme::RADIUS_SM};padding:14px 16px;",
                        div {
                            style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.1em;text-transform:uppercase;color:#B0503A;",
                            "验收信号"
                        }
                        div { style: "font:500 14px/1.5 {theme::FONT_SANS};color:#7A3D2D;", "值班工程师能在单个视图、15 分钟内定位一次异常的根因。" }
                    }
                }            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// STEP 3 · 北极星指标 (INPUT: north_star + ns_def)
// ════════════════════════════════════════════════════════════════════════════

#[component]
pub fn Step3NorthStar(state: Signal<WizState>) -> Element {
    rsx! {
        div { style: "{TWO_COL}",
            div { style: "{STICKY}",
                Eyebrow { text: "步骤 03 · 北极星指标" }
                h2 { style: "font:600 30px/1.3 {theme::FONT_SERIF};margin:0 0 18px;", "一个项目,只能有一个北极星" }
                Disclosure { summary: "为什么这么做 · 方法讲解",
                    p {
                        style: "font:400 15px/1.85 {theme::FONT_SANS};color:{theme::INK_2};margin:0 0 16px;",
                        "北极星是全队对齐的那一颗——它必须是用户价值导向、唯一、可量化,且"
                        b { style: "color:{theme::INK};", "从真实数据计算、难以人为修饰" }
                        "。"
                    }
                    div {
                        style: "border-left:2px solid {theme::SCROLL_THUMB};padding:4px 0 4px 16px;margin:6px 0 0;",
                        div { style: "font:500 12px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};margin-bottom:6px;", "传统 → AI" }
                        div { style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_2};", "一堆 KPI 各自为政 → 一个北极星统领,其余指标都为它服务。" }
                    }
                }
                ControlPoint { text: "有且仅有一个北极星;它衡量「用户得到的价值」,不是产出量。" }
            }

            div {
                div {
                    style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};border-radius:8px;padding:30px 32px;",
                    div {
                        style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.16em;text-transform:uppercase;\
                                color:{theme::CLAY};margin-bottom:18px;",
                        "本项目北极星"
                    }
                    input {
                        value: "{state().north_star}",
                        oninput: move |e| state.with_mut(|s| s.north_star = e.value()),
                        style: "width:100%;border:none;background:transparent;font:600 28px/1.4 {theme::FONT_SERIF};\
                                color:{theme::INK};padding:0 0 10px;outline:none;border-bottom:2px solid #E2C9BF;",
                    }
                    div { style: "font:500 12px/1 {theme::FONT_SANS};color:{theme::INK_3};margin:22px 0 8px;", "为什么它是个好北极星(可编辑)" }
                    textarea {
                        value: "{state().ns_def}",
                        oninput: move |e| state.with_mut(|s| s.ns_def = e.value()),
                        style: "width:100%;height:56px;border:1px solid {theme::BORDER};border-radius:{theme::RADIUS_SM};\
                                background:#fff;font:400 14px/1.7 {theme::FONT_SANS};color:#3A3833;padding:12px 14px;outline:none;",
                    }
                    div {
                        style: "display:flex;gap:10px;margin-top:20px;flex-wrap:wrap;",
                        for chip in ["✓ 用户价值导向", "✓ 唯一", "✓ 可量化", "✓ 从真实日志算 · 难造假"] {
                            div {
                                style: "background:#E7EDE2;color:#4A5E42;border-radius:20px;padding:7px 14px;\
                                        font:500 12.5px/1 {theme::FONT_SANS};",
                                "{chip}"
                            }
                        }
                    }
                }            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// STEP 4 · 引领指标 (INPUT: leading[].target)
// ════════════════════════════════════════════════════════════════════════════

#[component]
pub fn Step4Leading(state: Signal<WizState>) -> Element {
    let rows = state().leading;

    rsx! {
        div { style: "{TWO_COL}",
            div { style: "{STICKY}",
                Eyebrow { text: "步骤 04 · 引领性指标" }
                h2 { style: "font:600 30px/1.3 {theme::FONT_SERIF};margin:0 0 18px;", "本周我能控制的先行动作" }
                Disclosure { summary: "为什么这么做 · 方法讲解",
                    p {
                        style: "font:400 15px/1.85 {theme::FONT_SANS};color:{theme::INK_2};margin:0 0 16px;",
                        "北极星是结果,引领指标是你"
                        b { style: "color:{theme::INK};", "本周能主动推动" }
                        "的先行动作。每周设一次目标,让 agent loop 去推进。"
                    }
                    div {
                        style: "border-left:2px solid {theme::SCROLL_THUMB};padding:4px 0 4px 16px;margin:6px 0 0;",
                        div { style: "font:500 12px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};margin-bottom:6px;", "三条铁律" }
                        div {
                            style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_2};",
                            b { "可控" } "(你本周推得动)· " b { "可统计" } "(系统自动出数)· " b { "难造假" } "(来源是真实日志,不是手填)。"
                        }
                    }
                }
                ControlPoint { text: "引领指标必须同时满足「可控 / 可统计 / 难造假」,否则不要它。" }
            }

            div {
                div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:16px;", "本周引领指标 · 目标可编辑" }
                div {
                    style: "display:flex;flex-direction:column;gap:14px;",
                    for (i , r) in rows.iter().enumerate() {
                        div {
                            key: "{i}",
                            style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};border-radius:8px;\
                                    padding:20px 24px;display:grid;grid-template-columns:1fr auto;gap:20px;align-items:center;",
                            div {
                                div { style: "font:600 16px/1.4 {theme::FONT_SANS};color:{theme::INK};margin-bottom:6px;", "{r.name}" }
                                div { style: "font:400 13px/1.6 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:10px;", "{r.def}" }
                                div {
                                    style: "display:flex;gap:8px;flex-wrap:wrap;",
                                    span {
                                        style: "background:#EEF0EA;color:#5F7355;border-radius:5px;padding:4px 9px;\
                                                font:500 11.5px/1 {theme::FONT_SANS};",
                                        "来源 · {r.source}"
                                    }
                                    span {
                                        style: "background:#F2E4DD;color:#B0503A;border-radius:5px;padding:4px 9px;\
                                                font:500 11.5px/1 {theme::FONT_SANS};",
                                        "{r.ok}"
                                    }
                                }
                            }
                            div {
                                style: "display:flex;align-items:center;gap:16px;flex:none;",
                                div {
                                    style: "text-align:right;",
                                    div { style: "font:500 11px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};margin-bottom:5px;", "本周" }
                                    div { style: "font:500 18px/1 {theme::FONT_MONO};color:{theme::INK_3};", "{r.cur}" }
                                }
                                div { style: "font:400 16px/1 {theme::FONT_MONO};color:#C2BBAB;", "→" }
                                div {
                                    style: "text-align:right;",
                                    div { style: "font:500 11px/1 {theme::FONT_SANS};color:{theme::CLAY};margin-bottom:5px;", "目标" }
                                    input {
                                        value: "{r.target}",
                                        oninput: move |e| state.with_mut(|s| s.leading[i].target = e.value()),
                                        style: "width:64px;border:none;border-bottom:1px dashed {theme::CLAY};background:transparent;\
                                                font:700 18px/1 {theme::FONT_MONO};color:{theme::CLAY};text-align:right;outline:none;padding:0 0 2px;",
                                    }
                                }
                            }
                        }
                    }
                }            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// STEP 5 · 滞后指标 (INPUT: lagging[].target)
// ════════════════════════════════════════════════════════════════════════════

#[component]
pub fn Step5Lagging(state: Signal<WizState>) -> Element {
    let rows = state().lagging;

    rsx! {
        div { style: "{TWO_COL}",
            div { style: "{STICKY}",
                Eyebrow { text: "步骤 05 · 滞后性指标" }
                h2 { style: "font:600 30px/1.3 {theme::FONT_SERIF};margin:0 0 18px;", "用来验证,不用来下周度命令" }
                Disclosure { summary: "为什么这么做 · 方法讲解",
                    p {
                        style: "font:400 15px/1.85 {theme::FONT_SANS};color:{theme::INK_2};margin:0 0 16px;",
                        "滞后指标反映结果,验证你的引领指标是否真的有效,但你无法直接操控它。它是「验收」,不是「本周行动目标」。"
                    }
                    div {
                        style: "border-left:2px solid {theme::SCROLL_THUMB};padding:4px 0 4px 16px;margin:6px 0 0;",
                        div { style: "font:500 12px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};margin-bottom:6px;", "因果链" }
                        div {
                            style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_2};",
                            "引领指标 " b { style: "color:{theme::CLAY};", "驱动" } " 北极星,滞后指标 " b { style: "color:#5F7355;", "验证" } " 是否真的发生。"
                        }
                    }
                }
                ControlPoint { text: "滞后指标只用于验证;若它不动,回头质疑引领指标选错了。" }
            }

            div {
                // causal chain bar
                div {
                    style: "background:{theme::CARD_BG_3};border:1px solid {theme::BORDER};border-radius:8px;padding:18px 22px;\
                            margin-bottom:18px;display:flex;align-items:center;gap:14px;flex-wrap:wrap;",
                    span {
                        style: "background:#fff;border:1px solid #E6D2C8;color:{theme::CLAY};border-radius:{theme::RADIUS_SM};\
                                padding:8px 14px;font:600 13px/1 {theme::FONT_SANS};",
                        "引领指标"
                    }
                    span { style: "font:400 16px/1 {theme::FONT_MONO};color:{theme::CLAY};", "→ 驱动 →" }
                    span {
                        style: "background:#23211C;color:#fff;border-radius:{theme::RADIUS_SM};padding:8px 14px;font:600 13px/1 {theme::FONT_SANS};",
                        "北极星"
                    }
                    span { style: "font:400 16px/1 {theme::FONT_MONO};color:#5F7355;", "→ 体现为 →" }
                    span {
                        style: "background:#fff;border:1px solid #CFD8C8;color:#5F7355;border-radius:{theme::RADIUS_SM};\
                                padding:8px 14px;font:600 13px/1 {theme::FONT_SANS};",
                        "滞后指标"
                    }
                }
                div {
                    style: "display:grid;grid-template-columns:1fr 1fr;gap:14px;",
                    for (i , r) in rows.iter().enumerate() {
                        div {
                            key: "{i}",
                            style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};border-radius:8px;padding:20px 22px;",
                            div { style: "font:600 15px/1.4 {theme::FONT_SANS};color:{theme::INK};margin-bottom:5px;", "{r.name}" }
                            div { style: "font:400 12.5px/1.6 {theme::FONT_SANS};color:{theme::PLACEHOLDER};margin-bottom:16px;", "{r.def}" }
                            div {
                                style: "display:flex;align-items:baseline;gap:12px;",
                                div {
                                    div { style: "font:500 11px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};margin-bottom:5px;", "当前" }
                                    div { style: "font:500 22px/1 {theme::FONT_MONO};color:{theme::INK_3};", "{r.cur}" }
                                }
                                div { style: "font:400 15px/1 {theme::FONT_MONO};color:#C2BBAB;", "→" }
                                div {
                                    div { style: "font:500 11px/1 {theme::FONT_SANS};color:#5F7355;margin-bottom:5px;", "目标" }
                                    input {
                                        value: "{r.target}",
                                        oninput: move |e| state.with_mut(|s| s.lagging[i].target = e.value()),
                                        style: "width:88px;border:none;border-bottom:1px dashed #5F7355;background:transparent;\
                                                font:700 22px/1 {theme::FONT_MONO};color:#5F7355;outline:none;padding:0 0 2px;",
                                    }
                                }
                            }
                        }
                    }
                }            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// STEP 6 · 原型创建 (presentational; mini dashboard mock)
// ════════════════════════════════════════════════════════════════════════════

#[component]
pub fn Step6Prototype() -> Element {
    // 24h call-volume bars (height%, color).
    let bars: [(&str, &str); 8] = [
        ("38%", "#4A453C"),
        ("52%", "#4A453C"),
        ("46%", "#4A453C"),
        ("68%", "#4A453C"),
        ("82%", "#6E8A60"),
        ("74%", "#4A453C"),
        ("90%", "#C5654A"),
        ("60%", "#4A453C"),
    ];
    // cost-by-model bars (label, width%, color).
    let cost: [(&str, &str, &str); 3] = [
        ("opus", "62%", "#C5654A"),
        ("sonnet", "30%", "#7FA56F"),
        ("haiku", "12%", "#B5862F"),
    ];

    rsx! {
        div { style: "{TWO_COL}",
            div { style: "{STICKY}",
                Eyebrow { text: "步骤 06 · 原型创建" }
                h2 { style: "font:600 30px/1.3 {theme::FONT_SERIF};margin:0 0 18px;", "原型即规格,agent 产出 80%" }
                Disclosure { summary: "为什么这么做 · 方法讲解",
                    p {
                        style: "font:400 15px/1.85 {theme::FONT_SANS};color:{theme::INK_2};margin:0 0 16px;",
                        "不靠文档对齐,直接做可点击原型、内部 dogfood。让 agent loop 跑出 80% 的初稿,你只审最后 20%——保持干净的 git checkpoint,随时可回退。"
                    }
                    div {
                        style: "border-left:2px solid {theme::SCROLL_THUMB};padding:4px 0 4px 16px;margin:6px 0 0;",
                        div { style: "font:500 12px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};margin-bottom:6px;", "传统 → AI" }
                        div { style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_2};", "设计稿评审数轮才开发 → agent 几小时出可点击原型,用真实使用代替评审。" }
                    }
                }
                ControlPoint { text: "用原型而非文档对齐;人只在关键逻辑 / 验收处介入。" }
            }

            div {
                div {
                    style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:14px;display:flex;align-items:center;gap:8px;",
                    span { style: "display:inline-block;width:8px;height:8px;border-radius:50%;background:#5F7355;" }
                    "原型初稿 · 由 agent loop 生成,人审关键逻辑"
                }
                // dark mini dashboard
                div {
                    style: "background:#23211C;border-radius:10px;padding:22px;box-shadow:0 8px 30px rgba(35,33,28,.14);",
                    div {
                        style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:18px;",
                        div {
                            style: "display:flex;align-items:center;gap:10px;",
                            div { style: "width:9px;height:9px;border-radius:50%;background:#7FA56F;box-shadow:0 0 0 3px rgba(127,165,111,.2);" }
                            div { style: "font:600 14px/1 {theme::FONT_SANS};color:#F3EEE6;", "模型 API 服务 · 运营总览" }
                        }
                        div { style: "font:400 11px/1 {theme::FONT_MONO};color:{theme::INK_3};", "实时 · 30s 刷新" }
                    }
                    // 4 KPI tiles
                    div {
                        style: "display:grid;grid-template-columns:repeat(4,1fr);gap:12px;margin-bottom:14px;",
                        KpiTile { bg: "#2D2A24", label: "有效可用性", label_c: "#9A9388", value: "99.4%", value_c: "#7FA56F" }
                        KpiTile { bg: "#2D2A24", label: "P95 延迟", label_c: "#9A9388", value: "842ms", value_c: "#F3EEE6" }
                        KpiTile { bg: "#2D2A24", label: "每千次成本", label_c: "#9A9388", value: "¥2.30", value_c: "#F3EEE6" }
                        KpiTile { bg: "#3A2A24", label: "进行中事故", label_c: "#E0A78F", value: "1", value_c: "#E08B6F" }
                    }
                    div {
                        style: "display:grid;grid-template-columns:1.4fr 1fr;gap:12px;",
                        // call-volume bars
                        div {
                            style: "background:#2D2A24;border-radius:8px;padding:16px;",
                            div { style: "font:400 11px/1 {theme::FONT_SANS};color:#9A9388;margin-bottom:14px;", "调用量 · 24h" }
                            div {
                                style: "display:flex;align-items:flex-end;gap:5px;height:64px;",
                                for (h , c) in bars {
                                    div { style: "flex:1;height:{h};background:{c};border-radius:3px 3px 0 0;" }
                                }
                            }
                        }
                        // cost by model
                        div {
                            style: "background:#2D2A24;border-radius:8px;padding:16px;",
                            div { style: "font:400 11px/1 {theme::FONT_SANS};color:#9A9388;margin-bottom:12px;", "成本归因 · 按模型" }
                            div {
                                style: "display:flex;flex-direction:column;gap:9px;",
                                for (name , w , c) in cost {
                                    div {
                                        style: "display:flex;align-items:center;gap:8px;",
                                        div { style: "font:400 11px/1 {theme::FONT_MONO};color:#C9C2B6;width:64px;", "{name}" }
                                        div {
                                            style: "flex:1;height:7px;background:#3A372F;border-radius:4px;overflow:hidden;",
                                            div { style: "width:{w};height:100%;background:{c};" }
                                        }
                                    }
                                }
                            }
                            div {
                                style: "margin-top:14px;padding-top:12px;border-top:1px solid #3A372F;\
                                        font:400 11px/1.5 {theme::FONT_SANS};color:#E0A78F;",
                                "⚑ agent 根因建议:opus 流量异常集中于租户 #A12"
                            }
                        }
                    }
                }
                // human review checklist
                div {
                    style: "margin-top:16px;background:{theme::CARD_BG};border:1px solid {theme::BORDER};border-radius:8px;\
                            padding:16px 20px;display:flex;align-items:center;gap:14px;",
                    div {
                        style: "font:600 11px/1 {theme::FONT_MONO};letter-spacing:.1em;text-transform:uppercase;color:#5F7355;",
                        "人审清单"
                    }
                    div {
                        style: "font:400 13.5px/1.6 {theme::FONT_SANS};color:{theme::INK_2};",
                        "✓ 可用性算法口径 · ✓ 成本归因取数 · ◻ 根因建议的误报阈值(待你确认)"
                    }
                }            }
        }
    }
}

#[component]
fn KpiTile(bg: String, label: String, label_c: String, value: String, value_c: String) -> Element {
    rsx! {
        div {
            style: "background:{bg};border-radius:8px;padding:16px;",
            div { style: "font:400 11px/1 {theme::FONT_SANS};color:{label_c};margin-bottom:10px;", "{label}" }
            div { style: "font:700 24px/1 {theme::FONT_MONO};color:{value_c};", "{value}" }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// STEP 7 · 进度管理 (INPUT: leading[].target + leading[].driver + weekly_signal)
// ════════════════════════════════════════════════════════════════════════════

#[component]
pub fn Step7Progress(state: Signal<WizState>) -> Element {
    let rows = state().leading;
    let sig = state().weekly_signal;

    // sigMeta description card (prototype rows 3245–3249).
    let (sig_bg, sig_desc) = match sig {
        HealthSignal::Green => (
            "#E7EDE2",
            "本周引领指标按计划推进,可以继续放手让 agent loop 执行。",
        ),
        HealthSignal::Amber => ("#F5ECD6", "部分引领指标落后,本周内需要人为介入一个控制点。"),
        HealthSignal::Red => (
            "#F3E0DA",
            "引领指标停滞,触发复盘:是目标定错了,还是 agent loop 卡住了?",
        ),
        HealthSignal::Unknown => ("#EDE8DE", "尚无足够数据判断本周演进。"),
    };

    // weekly cadence cards (周一复盘 / 周一定目标 / 周中执行 / 周五观测).
    let cadence: [(&str, &str, &str, &str, &str, &str); 4] = [
        (
            "周一 · 复盘",
            theme::CLAY,
            "看上周目标 vs 实际",
            theme::INK,
            "哪条达成、哪条没达成,看板直接给数。",
            theme::INK_3,
        ),
        (
            "周一 · 定目标",
            "#B0503A",
            "手动设本周目标",
            "#7A3D2D",
            "据上周实际 + 本周交付的特性推导,每条挂依据。",
            "#9A6A58",
        ),
        (
            "周中 · 执行",
            theme::CLAY,
            "agent loop 推进",
            theme::INK,
            "放手让 agent 跑,人只审关键逻辑与验收。",
            theme::INK_3,
        ),
        (
            "周五 · 观测",
            theme::CLAY,
            "真实数据出健康信号",
            theme::INK,
            "达成了吗?北极星在动吗?回到周一复盘。",
            theme::INK_3,
        ),
    ];

    rsx! {
        div { style: "{TWO_COL}",
            div { style: "{STICKY}",
                Eyebrow { text: "步骤 07 · 过程 / 进度管理" }
                h2 {
                    style: "font:600 30px/1.3 {theme::FONT_SERIF};margin:0 0 18px;",
                    "看板是观测,"
                    br {}
                    "每周还得定目标"
                }
                Disclosure { summary: "为什么这么做 · 方法讲解",
                    p {
                        style: "font:400 15px/1.85 {theme::FONT_SANS};color:{theme::INK_2};margin:0 0 16px;",
                        "进度看板是" b { style: "color:{theme::INK};", "客观观测" } "——它只告诉你「现在是多少」。但按周推进,还要主动回答看板答不了的问题:"
                        b { style: "color:{theme::INK};", "根据上周的真实数据 + 本周要交付的特性,本周目标应该定成多少。" }
                        "这一步是手动的,也是「计划」的核心。"
                    }
                    div {
                        style: "background:{theme::CARD_BG};border:1px solid {theme::BORDER};border-radius:8px;padding:16px 18px;margin:0 0 14px;",
                        div { style: "font:600 10px/1 {theme::FONT_MONO};letter-spacing:.12em;text-transform:uppercase;color:{theme::PLACEHOLDER};margin-bottom:9px;", "一个例子" }
                        div {
                            style: "font:400 13.5px/1.75 {theme::FONT_SANS};color:#3A3833;",
                            "上周「服务停机」" b { style: "color:#B0503A;", "3 次" } "。本周上线了熔断与自动回滚,于是把本周目标从 3 次手动改为 "
                            b { style: "color:#4A5E42;", "0 次" } "——目标的变化由数据和已交付的特性推导,而不是拍脑袋。"
                        }
                    }
                    div {
                        style: "border-left:2px solid {theme::SCROLL_THUMB};padding:4px 0 4px 16px;margin:6px 0 0;",
                        div { style: "font:500 12px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};margin-bottom:6px;", "观测 ↔ 计划" }
                        div { style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_2};", "看板回答「是多少」(观测);周一复盘回答「该是多少」(计划)。两者闭环,才不是只看不动的甘特图。" }
                    }
                }
                ControlPoint { text: "每个本周目标都要挂一条依据(上周实际 + 本周交付的特性)。改目标可以,但必须说得出为什么。" }
            }

            div {
                div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:14px;", "本周节奏 · 一个闭环(观测 → 复盘 → 定目标 → 执行 → 回到观测)" }
                div {
                    style: "display:grid;grid-template-columns:repeat(4,1fr);gap:10px;margin-bottom:12px;",
                    for (i , (tag , tag_c , head , head_c , body , body_c)) in cadence.iter().enumerate() {
                        div {
                            key: "{i}",
                            style: if i == 1 {
                                "background:#F7EDE7;border:1px solid #ECD9D0;border-radius:8px;padding:16px 15px;".to_string()
                            } else {
                                format!("background:{};border:1px solid {};border-radius:8px;padding:16px 15px;", theme::CARD_BG, theme::BORDER)
                            },
                            div { style: "font:600 11px/1 {theme::FONT_MONO};color:{tag_c};margin-bottom:9px;", "{tag}" }
                            div { style: "font:600 13.5px/1.4 {theme::FONT_SANS};color:{head_c};margin-bottom:5px;", "{head}" }
                            div { style: "font:400 12px/1.55 {theme::FONT_SANS};color:{body_c};", "{body}" }
                        }
                    }
                }
                div {
                    style: "display:flex;align-items:center;gap:8px;margin-bottom:26px;font:500 11px/1.5 {theme::FONT_MONO};color:{theme::PLACEHOLDER};",
                    span { style: "color:{theme::CLAY};font-size:14px;", "↻" }
                    "每周一循环一次 · 上一圈的「观测」就是这一圈「复盘」的输入"
                }

                // weekPlan grid header
                div {
                    style: "display:flex;align-items:baseline;justify-content:space-between;gap:12px;margin-bottom:8px;flex-wrap:wrap;",
                    div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK};", "本周定目标 · 复盘上周 → 设本周(手动,可编辑)" }
                    div { style: "font:400 11.5px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};", "这一步看板替你不了" }
                }
                div {
                    style: "border:1px solid {theme::BORDER};border-radius:10px;overflow:hidden;background:{theme::CARD_BG};margin-bottom:28px;",
                    div {
                        style: "display:grid;grid-template-columns:1.25fr 0.8fr 1.05fr 0.95fr 1.5fr;gap:12px;padding:11px 18px;\
                                background:{theme::CARD_BG_2};border-bottom:1px solid {theme::BORDER};\
                                font:600 9.5px/1.3 {theme::FONT_MONO};letter-spacing:.06em;text-transform:uppercase;color:{theme::PLACEHOLDER};",
                        div { "引领指标" }
                        div { "上周目标" }
                        div { "上周实际" }
                        div { "本周目标" }
                        div { "依据 · 本周交付" }
                    }
                    for (i , r) in rows.iter().enumerate() {
                        WeekPlanRow {
                            key: "{i}",
                            idx: i,
                            name: r.name.clone(),
                            last_target: if r.last_target.is_empty() { "—".to_string() } else { r.last_target.clone() },
                            last_actual: r.cur.clone(),
                            hit: r.hit,
                            target: r.target.clone(),
                            driver: r.driver.clone(),
                            state,
                        }
                    }
                }

                // weekly health signal selector
                div { style: "font:500 13px/1 {theme::FONT_SANS};color:{theme::INK_3};margin-bottom:14px;", "本周健康信号 · 基于真实数据,一眼判断是否正常演进" }
                div {
                    style: "display:flex;gap:12px;margin-bottom:18px;",
                    SignalChoice { sel: sig, kind: HealthSignal::Green, dot: "#5F7355", label: "正常演进", label_c: "#4A5E42", on_bg: "#E7EDE2", on_border: "#5F7355", state }
                    SignalChoice { sel: sig, kind: HealthSignal::Amber, dot: "#B5862F", label: "需要关注", label_c: "#8A6720", on_bg: "#F5ECD6", on_border: "#B5862F", state }
                    SignalChoice { sel: sig, kind: HealthSignal::Red, dot: "#B0503A", label: "阻塞", label_c: "#8A3D2A", on_bg: "#F3E0DA", on_border: "#B0503A", state }
                }
                div {
                    style: "background:{sig_bg};border-radius:8px;padding:16px 20px;font:500 14px/1.6 {theme::FONT_SANS};color:#3A3833;",
                    "{sig_desc}"
                }            }
        }
    }
}

/// One editable row in the step-7 weekPlan grid (本周目标 + 依据 inputs).
#[component]
fn WeekPlanRow(
    idx: usize,
    name: String,
    last_target: String,
    last_actual: String,
    hit: bool,
    target: String,
    driver: String,
    state: Signal<WizState>,
) -> Element {
    let (hit_label, hit_color, hit_bg) = if hit {
        ("达成", "#4A5E42", "#E7EDE2")
    } else {
        ("未达成", "#B0503A", "#F2E4DD")
    };
    rsx! {
        div {
            style: "display:grid;grid-template-columns:1.25fr 0.8fr 1.05fr 0.95fr 1.5fr;gap:12px;padding:13px 18px;\
                    border-bottom:1px solid {theme::BORDER_3};align-items:center;",
            div { style: "font:600 13px/1.35 {theme::FONT_SANS};color:{theme::INK};", "{name}" }
            div { style: "font:500 13px/1 {theme::FONT_MONO};color:{theme::PLACEHOLDER};", "{last_target}" }
            div {
                style: "display:flex;align-items:center;gap:7px;",
                span { style: "font:600 13px/1 {theme::FONT_MONO};color:{theme::INK};", "{last_actual}" }
                span {
                    style: "font:600 9px/1 {theme::FONT_MONO};color:{hit_color};background:{hit_bg};\
                            border-radius:4px;padding:3px 6px;white-space:nowrap;",
                    "{hit_label}"
                }
            }
            input {
                value: "{target}",
                oninput: move |e| state.with_mut(|s| s.leading[idx].target = e.value()),
                style: "width:100%;background:#fff;border:1px solid #E0D7C7;border-radius:{theme::RADIUS_SM};\
                        padding:8px 10px;font:700 14px/1 {theme::FONT_MONO};color:#B0503A;",
            }
            input {
                value: "{driver}",
                oninput: move |e| state.with_mut(|s| s.leading[idx].driver = e.value()),
                style: "width:100%;background:#fff;border:1px solid #E0D7C7;border-radius:{theme::RADIUS_SM};\
                        padding:8px 10px;font:400 12px/1.4 {theme::FONT_SANS};color:#3A3833;",
            }
        }
    }
}

/// One weekly-signal choice tile. Selected → colored bg + border; else paper.
#[component]
fn SignalChoice(
    sel: HealthSignal,
    kind: HealthSignal,
    dot: String,
    label: String,
    label_c: String,
    on_bg: String,
    on_border: String,
    state: Signal<WizState>,
) -> Element {
    let selected = sel == kind;
    let (bg, border) = if selected {
        (on_bg, on_border)
    } else {
        (theme::CARD_BG.to_string(), theme::BORDER.to_string())
    };
    rsx! {
        div {
            onclick: move |_| state.with_mut(|s| s.weekly_signal = kind),
            style: "flex:1;cursor:pointer;border-radius:8px;padding:18px 20px;border:2px solid {border};background:{bg};",
            div {
                style: "display:flex;align-items:center;gap:9px;",
                div { style: "width:11px;height:11px;border-radius:50%;background:{dot};" }
                div { style: "font:600 15px/1 {theme::FONT_SANS};color:{label_c};", "{label}" }
            }
        }
    }
}
