//! The window chrome: a permanent 64px icon rail + the workspace view router.
//!
//! `Shell` reads the [`ViewModel`] from context and routes on `vm.view`:
//! `Projects` → the project wall, `Wizard` → the P2-B stub, `App` → the P2-C
//! stub. The non-workspace hubs (skill/agent/…) are present as rail tiles for
//! visual fidelity but inert in P2-A — there is no hub-switch `Command` yet, so
//! they're non-clickable placeholders. Workspace is the active hub.

use bw_app::View;
use dioxus::prelude::*;

use crate::bridge::ViewModel;
use crate::icons;
use crate::screens::{ops::OpsScreen, projects::ProjectWall, wizard::WizardScreen};
use crate::theme;

/// Root layout: rail on the left, routed main content filling the rest.
#[component]
pub fn Shell() -> Element {
    let vm = use_context::<Signal<ViewModel>>();

    rsx! {
        div {
            style: "display:flex;min-height:100vh;background:{theme::PAPER};color:{theme::INK};",
            IconRail {}
            // Main content column.
            div {
                style: "flex:1;min-width:0;",
                match vm().view {
                    View::Projects => rsx! { ProjectWall {} },
                    View::Wizard => rsx! { WizardScreen {} },
                    View::App => rsx! { OpsScreen {} },
                }
            }
        }
    }
}

/// The always-visible 64px vertical rail (plan `01 §2.2` / `§6`). Workspace is
/// the live hub for P2; the rest are fidelity placeholders.
#[component]
fn IconRail() -> Element {
    // In P2-A the only hub is workspace, so its tile is always active. When a
    // hub-switch Command lands, read the active hub from the VM here.
    rsx! {
        div {
            style: "width:64px;flex:none;position:sticky;top:0;height:100vh;align-self:flex-start;\
                    z-index:60;background:{theme::RAIL_BG};border-right:1px solid {theme::BORDER_2};\
                    display:flex;flex-direction:column;align-items:center;padding:13px 0 14px;gap:5px;",

            // Brand mark.
            div {
                title: "Builders 工作台",
                style: "width:34px;height:34px;border-radius:9px;background:{theme::CLAY};\
                        display:flex;align-items:center;justify-content:center;color:#fff;\
                        font:700 16px/1 {theme::FONT_MONO};margin-bottom:9px;flex:none;",
                "B"
            }

            // Workspace — the active hub for P2.
            RailTile { title: "工作台 · 项目", active: true, icon: icons::workspace() }

            RailDivider {}

            RailTile { title: "SkillHub · 技能库", active: false, icon: icons::skill() }
            RailTile { title: "AgentHub · 智能体", active: false, icon: icons::agent() }
            RailTile { title: "Routines · 例程 / 工作流库", active: false, icon: icons::routine() }
            RailTile { title: "CronHub · 定时任务", active: false, icon: icons::cron() }

            RailDivider {}

            RailTile { title: "Connectors · 连接器 / 数据源", active: false, icon: icons::connector() }
            RailTile { title: "Knowledge · 知识库 / 记忆", active: false, icon: icons::knowledge() }
            RailTile { title: "Activity · 运行记录", active: false, icon: icons::activity() }

            // Push the trailing group to the bottom.
            div { style: "flex:1;" }

            // Notify tile carries the unread dot.
            div {
                title: "通知 · 待审批",
                style: "position:relative;width:42px;height:40px;border-radius:11px;display:flex;\
                        align-items:center;justify-content:center;cursor:default;flex:none;color:{theme::INK_3};",
                {icons::notify()}
                div {
                    style: "position:absolute;top:7px;right:8px;width:7px;height:7px;border-radius:50%;\
                            background:{theme::CLAY};border:1.5px solid {theme::RAIL_BG};",
                }
            }
            RailTile { title: "设置", active: false, icon: icons::settings() }

            // Account avatar.
            div {
                title: "账户",
                style: "width:30px;height:30px;border-radius:50%;background:{theme::INK};\
                        color:{theme::PAPER};display:flex;align-items:center;justify-content:center;\
                        font:600 12px/1 {theme::FONT_SANS};cursor:default;margin-top:6px;flex:none;",
                "用"
            }
        }
    }
}

/// A 42×40 rail tile. `active` paints the workspace-hub highlight; idle tiles use
/// muted ink. (Idle tiles are inert in P2-A — see [`IconRail`] docs.)
#[component]
fn RailTile(title: String, active: bool, icon: Element) -> Element {
    let (bg, fg) = if active {
        (theme::PAPER, theme::INK)
    } else {
        ("transparent", theme::INK_3)
    };
    rsx! {
        div {
            title,
            style: "width:42px;height:40px;border-radius:11px;display:flex;align-items:center;\
                    justify-content:center;cursor:default;flex:none;background:{bg};color:{fg};",
            {icon}
        }
    }
}

/// The hairline group divider between rail sections.
#[component]
fn RailDivider() -> Element {
    rsx! {
        div {
            style: "width:26px;height:1px;background:{theme::RAIL_DIVIDER};margin:5px 0;flex:none;",
        }
    }
}
