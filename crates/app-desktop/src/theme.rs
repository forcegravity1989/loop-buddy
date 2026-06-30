//! Design tokens (plan `01 §6`) as Rust consts — the single warm-paper palette
//! the whole shell formats into inline `style:` strings, faithfully porting the
//! prototype's look.
//!
//! Signal colors are **not** here on purpose: they live in [`ui::signal_color`]
//! (the derive-driven palette, with `Unknown` → grey, never green). Reuse that;
//! never duplicate a signal hex in the UI.
//!
//! This is the *complete* token set from `01 §6`, deliberately exhaustive so
//! P2-B/P2-C reach for a named const instead of pasting a hex. A handful
//! (`AGENT_PURPLE*`, `SHADOW`, `SELECTION`, `CARD_BG_3`, `BORDER_3`, `ALERT_RED`)
//! aren't consumed by the P2-A wall yet — hence the module-wide `dead_code`
//! allow. Remove it once the later screens land.
#![allow(dead_code)]

// ── surfaces ───────────────────────────────────────────────────────────────
/// 暖纸 — the app background.
pub const PAPER: &str = "#EFEBE2";
/// 图标栏底色 — the 64px left icon rail.
pub const RAIL_BG: &str = "#E9E3D7";

/// 卡片底色 — primary card fill.
pub const CARD_BG: &str = "#FBFAF6";
/// 卡片底色 — secondary (muted) card fill.
pub const CARD_BG_2: &str = "#F4F0E7";
/// 卡片底色 — tertiary (warm) card fill.
pub const CARD_BG_3: &str = "#F7F2EC";

// ── lines ────────────────────────────────────────────────────────────────
/// 边框 — default card border.
pub const BORDER: &str = "#E2DCCF";
/// 边框 — stronger / interactive border.
pub const BORDER_2: &str = "#DBD4C5";
/// 边框 — faint inner divider.
pub const BORDER_3: &str = "#ECE6DA";
/// Rail hairline divider between icon groups.
pub const RAIL_DIVIDER: &str = "#D3CBBA";
/// Dashed "新建项目" card border.
pub const DASH_BORDER: &str = "#CFC7B6";

// ── brand ────────────────────────────────────────────────────────────────
/// 品牌 / clay — the brand accent (also the cold-start badge + clay progress).
pub const CLAY: &str = "#C5654A";

// ── ink ────────────────────────────────────────────────────────────────
/// 文字主色.
pub const INK: &str = "#23211C";
/// 文字次色.
pub const INK_2: &str = "#57534A";
/// 文字辅色.
pub const INK_3: &str = "#8C867A";
/// 文字占位.
pub const PLACEHOLDER: &str = "#A19B8D";

// ── accents ────────────────────────────────────────────────────────────────
/// 警示深红 (band edge).
pub const ALERT_RED: &str = "#A33D29";
/// Agent 紫 (primary).
pub const AGENT_PURPLE: &str = "#5A4E7A";
/// Agent 紫 (deep).
pub const AGENT_PURPLE_2: &str = "#4B4660";

// ── stage badge (运营中 green / 冷启动中 clay) ───────────────────────────────
/// Running badge background (绿).
pub const BADGE_RUNNING_BG: &str = "#E7EDE2";
/// Running badge foreground.
pub const BADGE_RUNNING_FG: &str = "#4A5E42";

// ── decoration ────────────────────────────────────────────────────────────
/// 阴影 — the card lift.
pub const SHADOW: &str = "0 8px 26px rgba(35,33,28,.08)";
/// 选区色.
pub const SELECTION: &str = "#E7CFC4";
/// 滚动条 thumb.
pub const SCROLL_THUMB: &str = "#D8D1C2";
/// Empty progress-bar track on the project wall.
pub const PROGRESS_TRACK: &str = "#EBE5D9";

// ── radius helpers ────────────────────────────────────────────────────────
/// 6px — small chips / buttons.
pub const RADIUS_SM: &str = "6px";
/// 10px — cards.
pub const RADIUS_MD: &str = "10px";
/// 12px — large cards.
pub const RADIUS_LG: &str = "12px";

// ── font stacks (see global CSS in `main.rs`) ───────────────────────────────
/// Headings — Songti / serif (CJK-aware fallback until binaries are bundled).
pub const FONT_SERIF: &str = "'Noto Serif SC','Songti SC',serif";
/// Body — Sans (CJK-aware fallback).
pub const FONT_SANS: &str = "'Noto Sans SC','PingFang SC',system-ui,sans-serif";
/// Numbers / mono.
pub const FONT_MONO: &str = "'JetBrains Mono','SF Mono',ui-monospace,monospace";
