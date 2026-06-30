//! `view = Projects` — the project wall (prototype rows 630–668).
//!
//! Faithful port: brand mark + 「我的项目」 header, a 2-column grid of project
//! cards read from [`ViewModel::projects`], and a trailing dashed 「+ 新建项目」
//! card. Cards click through to `OpenProject`; the dashed card mints a fresh
//! project via `CreateProject` (which routes the kernel to `View::Wizard`).
//!
//! Health is read-only here: the signal dot uses [`ui::signal_color`] over the
//! derived `ProjectCardVM::signal` (`Unknown` → grey, never green). The UI never
//! sets a signal.

use bw_app::Command;
use bw_core::model::ProjectPhase;
use bw_core::ProjectId;
use dioxus::prelude::*;
use ui::{progress_color, signal_color};

use crate::bridge::{CommandBus, ProjectCardVM, ViewModel};
use crate::theme;

/// Hover-lift for cards (Dioxus 0.7 has no inline `:hover`, so a tiny scoped
/// stylesheet supplies the transition the prototype faked with `style-hover`).
const WALL_CSS: &str = "
.bw-card{transition:transform .14s ease, box-shadow .14s ease, border-color .14s ease;}
.bw-card:hover{transform:translateY(-2px);box-shadow:0 8px 26px rgba(35,33,28,.08);border-color:#DBD4C5;}
.bw-new{transition:transform .14s ease, border-color .14s ease, background .14s ease;}
.bw-new:hover{transform:translateY(-2px);border-color:#C5654A;background:#FBFAF6;}
";

#[component]
pub fn ProjectWall() -> Element {
    let vm = use_context::<Signal<ViewModel>>();
    let projects = vm().projects;
    let count = projects.len();

    rsx! {
        style { {WALL_CSS} }
        div {
            style: "max-width:1080px;margin:0 auto;padding:60px 40px 100px;",

            // ── brand row ──────────────────────────────────────────────────
            div {
                style: "display:flex;align-items:center;gap:11px;margin-bottom:46px;",
                div {
                    style: "width:30px;height:30px;border-radius:7px;background:{theme::CLAY};\
                            display:flex;align-items:center;justify-content:center;color:#fff;\
                            font:700 15px/1 {theme::FONT_MONO};",
                    "B"
                }
                div { style: "font:600 16px/1.2 {theme::FONT_SANS};", "Builders 工作台" }
                div { style: "width:1px;height:16px;background:{theme::SCROLL_THUMB};" }
                div {
                    style: "font:400 14px/1.2 {theme::FONT_SANS};color:{theme::INK_3};",
                    "项目管理体系 · 多项目"
                }
            }

            // ── title + subcopy ────────────────────────────────────────────
            div {
                style: "display:flex;align-items:flex-end;justify-content:space-between;gap:20px;\
                        flex-wrap:wrap;margin-bottom:14px;",
                h1 {
                    style: "font:600 40px/1.2 {theme::FONT_SERIF};margin:0;",
                    "我的项目"
                }
                div {
                    style: "font:400 13.5px/1.6 {theme::FONT_SANS};color:{theme::PLACEHOLDER};",
                    "工作台管理 {count} 个项目 · 每个项目有独立进度"
                }
            }
            p {
                style: "font:400 15px/1.85 {theme::FONT_SANS};color:{theme::INK_2};\
                        max-width:50em;margin:0 0 38px;",
                "每个项目都从 0→1 的冷启动向导建立体系,成熟后进入运营态。进入任意项目,"
                b { style: "color:{theme::INK};",
                    "左侧是会话 / 环节历史,中央是该环节的产物画布、对话或观测看板"
                }
                " —— 万变不离其宗。"
            }

            // ── card grid ──────────────────────────────────────────────────
            div {
                style: "display:grid;grid-template-columns:repeat(2,1fr);gap:16px;",
                for card in projects.iter().cloned() {
                    ProjectCard { key: "{card.id.uuid()}", card }
                }
                NewProjectCard {}
            }
        }
    }
}

/// One project card. Click dispatches `OpenProject(id)`.
#[component]
fn ProjectCard(card: ProjectCardVM) -> Element {
    let bus = use_context::<CommandBus>();

    // Stage badge: 运营中 green / 冷启动中 clay (prototype `phaseBg`/`phaseColor`).
    let (badge_bg, badge_fg, badge_label) = match card.phase {
        ProjectPhase::Running => (theme::BADGE_RUNNING_BG, theme::BADGE_RUNNING_FG, "运营中"),
        ProjectPhase::ColdStart => ("#F2E4DD", theme::CLAY, "冷启动中"),
    };

    // Meta line: cold-start shows the wizard step, running shows env count + kind.
    let meta = match card.phase {
        ProjectPhase::ColdStart => format!("第 {}/7 步", card.cold_step),
        ProjectPhase::Running => format!("7 个环节 · {}", card.kind),
    };

    let prog_color = progress_color(card.progress);
    let prog = card.progress;
    let dot = signal_color(card.signal);
    let id: ProjectId = card.id;
    // Render an empty desc as a non-breaking space so the min-height block holds.
    let desc = if card.desc.is_empty() {
        "\u{00A0}".to_string()
    } else {
        card.desc.clone()
    };

    rsx! {
        div {
            class: "bw-card",
            onclick: move |_| bus.send(Command::OpenProject(id)),
            style: "cursor:pointer;background:{theme::CARD_BG};border:1px solid {theme::BORDER};\
                    border-radius:{theme::RADIUS_LG};padding:24px 26px;",

            // badge row + signal dot
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:16px;",
                span {
                    style: "font:500 11px/1 {theme::FONT_SANS};background:{badge_bg};color:{badge_fg};\
                            border-radius:{theme::RADIUS_SM};padding:5px 10px;",
                    "{badge_label}"
                }
                div { style: "width:9px;height:9px;border-radius:50%;background:{dot};" }
            }

            // name + desc
            div {
                style: "font:600 19px/1.35 {theme::FONT_SERIF};color:{theme::INK};margin-bottom:9px;",
                "{card.name}"
            }
            div {
                style: "font:400 13.5px/1.7 {theme::FONT_SANS};color:{theme::INK_3};\
                        margin-bottom:22px;min-height:46px;",
                "{desc}"
            }

            // meta + progress label
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;",
                span {
                    style: "font:500 12px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};",
                    "{meta}"
                }
                span {
                    style: "font:600 13px/1 {theme::FONT_MONO};color:{prog_color};",
                    "{prog}%"
                }
            }
            // progress bar
            div {
                style: "height:6px;background:{theme::PROGRESS_TRACK};border-radius:3px;overflow:hidden;",
                div { style: "width:{prog}%;height:100%;background:{prog_color};" }
            }
        }
    }
}

/// The trailing dashed 「+ 新建项目」 card. Mints a fresh `ProjectId` and
/// dispatches `CreateProject`; the kernel then routes to `View::Wizard`.
#[component]
fn NewProjectCard() -> Element {
    let bus = use_context::<CommandBus>();
    rsx! {
        div {
            class: "bw-new",
            onclick: move |_| bus.send(Command::CreateProject {
                id: ProjectId::new(),
                name: "未命名产品".into(),
                kind: "看板 / 网页应用".into(),
            }),
            style: "cursor:pointer;border:1.5px dashed {theme::DASH_BORDER};\
                    border-radius:{theme::RADIUS_LG};padding:24px 26px;display:flex;\
                    flex-direction:column;align-items:center;justify-content:center;min-height:200px;",
            div {
                style: "width:44px;height:44px;border-radius:50%;border:1.5px solid {theme::DASH_BORDER};\
                        display:flex;align-items:center;justify-content:center;\
                        font:300 28px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};margin-bottom:13px;",
                "+"
            }
            div {
                style: "font:600 15px/1 {theme::FONT_SANS};color:{theme::INK_2};margin-bottom:6px;",
                "新建项目"
            }
            div {
                style: "font:400 12.5px/1 {theme::FONT_SANS};color:{theme::PLACEHOLDER};",
                "走一遍 0→1 冷启动向导"
            }
        }
    }
}
