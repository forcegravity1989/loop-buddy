//! The 10 inline stroke-SVG rail icons, ported verbatim from the prototype
//! (`01 §6`): stroke-based, 1.7px, round caps/joins, 21×21 in a 0..24 viewBox.
//! Color comes from `currentColor`, so the rail sets `color:` on the wrapper and
//! the icon follows (active = ink, idle = ink-3).
//!
//! These are intentionally dumb leaf components — no state, no events. The rail
//! ([`crate::shell`]) wraps them with the clickable tile + tooltip.

use dioxus::prelude::*;

/// 工作台 — 4-grid.
pub fn workspace() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            rect { x: "4", y: "4", width: "7", height: "7", rx: "1.6" }
            rect { x: "13", y: "4", width: "7", height: "7", rx: "1.6" }
            rect { x: "4", y: "13", width: "7", height: "7", rx: "1.6" }
            rect { x: "13", y: "13", width: "7", height: "7", rx: "1.6" }
        }
    }
}

/// 技能 — diamond (skill).
pub fn skill() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M12 3 L20 12 L12 21 L4 12 Z" }
            path { d: "M12 8.2 L15.8 12 L12 15.8 L8.2 12 Z" }
        }
    }
}

/// 智能体 — robot.
pub fn agent() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            rect { x: "5", y: "8", width: "14", height: "11", rx: "3" }
            line { x1: "12", y1: "8", x2: "12", y2: "4.5" }
            circle { cx: "12", cy: "3.3", r: "1.2" }
            circle { cx: "9.6", cy: "13", r: "1.05" }
            circle { cx: "14.4", cy: "13", r: "1.05" }
        }
    }
}

/// 例程 — 3-node graph (routine).
pub fn routine() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            circle { cx: "6", cy: "6", r: "2.3" }
            circle { cx: "6", cy: "18", r: "2.3" }
            circle { cx: "18", cy: "12", r: "2.3" }
            path { d: "M8.2 6.7 L15.6 11.2" }
            path { d: "M8.2 17.3 L15.6 12.8" }
        }
    }
}

/// 定时 — clock (cron).
pub fn cron() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            circle { cx: "12", cy: "12", r: "8.3" }
            path { d: "M12 7.4 L12 12 L15.4 13.6" }
        }
    }
}

/// 连接器 — dumbbell (connector).
pub fn connector() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            circle { cx: "6", cy: "12", r: "2.6" }
            circle { cx: "18", cy: "12", r: "2.6" }
            line { x1: "8.6", y1: "12", x2: "15.4", y2: "12" }
        }
    }
}

/// 知识 — book.
pub fn knowledge() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            rect { x: "5", y: "4", width: "14", height: "16", rx: "2" }
            line { x1: "9", y1: "4", x2: "9", y2: "20" }
            line { x1: "12", y1: "8.5", x2: "16", y2: "8.5" }
            line { x1: "12", y1: "12", x2: "16", y2: "12" }
        }
    }
}

/// 活动 — pulse line (activity).
pub fn activity() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            polyline { points: "3,13 7.5,13 10,5.5 14,18.5 16.5,11 21,11" }
        }
    }
}

/// 通知 — bell (notify).
pub fn notify() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M6.5 17 V11 a5.5 5.5 0 0 1 11 0 V17 l1.4 2 H5.1 Z" }
            path { d: "M10 19.5 a2 2 0 0 0 4 0" }
        }
    }
}

/// 设置 — two sliders (settings). The two knob fills use the rail bg so the line
/// reads as passing behind them, matching the prototype.
pub fn settings() -> Element {
    rsx! {
        svg {
            width: "21", height: "21", view_box: "0 0 24 24", fill: "none",
            stroke: "currentColor", stroke_width: "1.7",
            stroke_linecap: "round", stroke_linejoin: "round",
            line { x1: "4", y1: "8", x2: "20", y2: "8" }
            circle { cx: "15", cy: "8", r: "2.5", fill: "{crate::theme::RAIL_BG}" }
            line { x1: "4", y1: "16", x2: "20", y2: "16" }
            circle { cx: "9", cy: "16", r: "2.5", fill: "{crate::theme::RAIL_BG}" }
        }
    }
}
