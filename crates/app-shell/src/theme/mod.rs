//! 设计系统:高保真原型那张样式表 + 几个 Rust 侧要用到的取值函数。
//!
//! **正本是 `docs/v4-prototype/hifi/index.html` 的 `<style>` 块**,整体搬进了
//! `assets/hifi.css`(搬法与两处改动见那个文件的头注)。屏里一律写 `class`,
//! 不写行内样式 —— 行内样式是上一版的做法,它让壳和高保真越走越远,正在逐屏
//! 退役。
//!
//! 颜色不在这里定义,在样式表的 `:root` 里。这里只留两件 Rust 必须做的事:
//! 把信号枚举翻成颜色变量名和中文标签。**没数据是灰,不是绿** —— 这条在配色
//! 上也守住。

/// 整张样式表。挂在 `document::Style` 上,全局生效。
pub const GLOBAL_CSS: &str = include_str!("../../assets/hifi.css");

pub const SERIF: &str = "'Noto Serif SC','Songti SC','STSong','SimSun',serif";
pub const MONO: &str = "'JetBrains Mono','SF Mono',Menlo,Consolas,monospace";

/// 三态信号 + Unknown 灰,取样式表 `:root` 里的变量,不另写一套色号。
pub fn signal_color(s: Option<bw_v4::Signal>) -> &'static str {
    match s {
        Some(bw_v4::Signal::Green) => "var(--green)",
        Some(bw_v4::Signal::Amber) => "var(--amber)",
        Some(bw_v4::Signal::Red) => "var(--red)",
        Some(bw_v4::Signal::Unknown) | None => "var(--gray)",
    }
}

/// 高保真上健康那一栏用的词。项目墙的健康概览条用的也是这四个。
pub fn signal_label(s: Option<bw_v4::Signal>) -> &'static str {
    match s {
        Some(bw_v4::Signal::Green) => "平稳",
        Some(bw_v4::Signal::Amber) => "需要关注",
        Some(bw_v4::Signal::Red) => "阻塞",
        Some(bw_v4::Signal::Unknown) | None => "无数据",
    }
}

// ── 以下是上一版的行内样式函数,正在逐屏退役 ──────────────────
//
// 每把一个屏改成用样式表的类名,就把它在那个屏里的调用删掉;最后一个调用者
// 消失时,函数本身一起删。不要给新代码用。

pub const CLAY: &str = "#C5654A";
pub const CARD: &str = "#FBFAF6";
pub const CARD_ALT: &str = "#F4F0E7";
pub const BORDER: &str = "#E2DCCF";
pub const BORDER_DEEP: &str = "#DBD4C5";
pub const INK: &str = "#23211C";
pub const INK_2: &str = "#57534A";
pub const INK_3: &str = "#8C867A";
pub const INK_4: &str = "#A19B8D";
pub const ALERT_DEEP: &str = "#A33D29";
pub const SHADOW: &str = "0 8px 26px rgba(35,33,28,.08)";

pub fn dot(color: &str, size: u32) -> String {
    format!(
        "width:{size}px;height:{size}px;border-radius:50%;background:{color};display:inline-block;flex:none;"
    )
}

pub fn card() -> String {
    format!("background:{CARD};border:1px solid {BORDER};border-radius:10px;box-shadow:{SHADOW};")
}

pub fn chip(bg: &str, fg: &str) -> String {
    format!(
        "display:inline-block;padding:2px 8px;border-radius:6px;background:{bg};color:{fg};font-size:11px;line-height:16px;white-space:nowrap;"
    )
}

pub fn btn_primary() -> String {
    format!(
        "cursor:pointer;background:{CLAY};color:#FFF;border:none;border-radius:8px;padding:10px 22px;font-size:14px;font-weight:500;"
    )
}

pub fn btn_ghost() -> String {
    format!(
        "cursor:pointer;background:transparent;color:{INK_2};border:1px solid {BORDER_DEEP};border-radius:8px;padding:7px 14px;font-size:13px;"
    )
}

pub fn input() -> String {
    format!(
        "width:100%;background:#FFFDF8;border:1px solid {BORDER_DEEP};border-radius:8px;padding:9px 11px;font-size:13px;line-height:1.55;"
    )
}

pub fn label() -> String {
    format!("font-size:12px;color:{INK_3};margin:0 0 6px;display:block;")
}
