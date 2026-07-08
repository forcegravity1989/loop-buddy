//! Design-system tokens (plan `00 §6` / inventory `§6`) — the warm-paper look.
//!
//! Fonts: the prototype loads Noto Serif/Sans SC from Google Fonts; a native
//! desktop app must eventually bundle them via `asset!()` (P3 fidelity pass).
//! Until then the stacks below fall back to the system CJK families that ship
//! with macOS/Windows (Songti/PingFang · SimSun/Microsoft YaHei), which keeps
//! the serif/sans/mono three-face mix correct offline.

/// 底色(暖纸)
pub const PAPER: &str = "#EFEBE2";
/// 图标栏底色
pub const RAIL_BG: &str = "#E9E3D7";
/// 品牌 / clay
pub const CLAY: &str = "#C5654A";
/// 卡片底色
pub const CARD: &str = "#FBFAF6";
pub const CARD_ALT: &str = "#F4F0E7";
/// 边框
pub const BORDER: &str = "#E2DCCF";
pub const BORDER_DEEP: &str = "#DBD4C5";
/// 文字
pub const INK: &str = "#23211C";
pub const INK_2: &str = "#57534A";
pub const INK_3: &str = "#8C867A";
pub const INK_4: &str = "#A19B8D";
/// Agent 紫
pub const AGENT: &str = "#5A4E7A";
/// 警示深红
pub const ALERT_DEEP: &str = "#A33D29";
/// 阴影
pub const SHADOW: &str = "0 8px 26px rgba(35,33,28,.08)";

pub const SERIF: &str = "'Noto Serif SC','Songti SC','STSong','SimSun',serif";
pub const SANS: &str =
    "'Noto Sans SC','PingFang SC','Hiragino Sans GB','Microsoft YaHei',sans-serif";
pub const MONO: &str = "'JetBrains Mono','SF Mono',Menlo,Consolas,monospace";

/// Global stylesheet: selection, scrollbars, resets. Injected once via
/// `document::Style`.
pub const GLOBAL_CSS: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; height: 100%; }
::selection { background: #E7CFC4; }
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb { background: #D8D1C2; border-radius: 6px; border: 3px solid #EFEBE2; }
::-webkit-scrollbar-track { background: transparent; }
button { font-family: inherit; }
input, textarea, select { font-family: inherit; color: #23211C; }
textarea { resize: vertical; }
input:focus, textarea:focus { outline: 1.5px solid #C5654A; outline-offset: 0; }
"#;

/// Signal dot inline style.
pub fn dot(color: &str, size: u32) -> String {
    format!(
        "width:{size}px;height:{size}px;border-radius:50%;background:{color};display:inline-block;flex:none;"
    )
}

/// Standard card shell.
pub fn card() -> String {
    format!("background:{CARD};border:1px solid {BORDER};border-radius:10px;box-shadow:{SHADOW};")
}

/// Small label chip (phase badges etc.).
pub fn chip(bg: &str, fg: &str) -> String {
    format!(
        "display:inline-block;padding:2px 8px;border-radius:6px;background:{bg};color:{fg};font-size:11px;line-height:16px;white-space:nowrap;"
    )
}

/// A primary (clay) button.
pub fn btn_primary() -> String {
    format!(
        "cursor:pointer;background:{CLAY};color:#FFF;border:none;border-radius:8px;padding:10px 22px;font-size:14px;font-weight:500;"
    )
}

/// Text input / textarea base.
pub fn input() -> String {
    format!(
        "width:100%;background:#FFFDF8;border:1px solid {BORDER_DEEP};border-radius:8px;padding:9px 11px;font-size:13px;line-height:1.55;"
    )
}

/// Field label.
pub fn label() -> String {
    format!("font-size:12px;color:{INK_3};margin:0 0 6px;display:block;")
}
