//! 设计系统:高保真原型那张样式表 + 两个 Rust 侧要用到的取值函数。
//!
//! **正本是 `docs/v4-prototype/hifi/index.html` 的 `<style>` 块**,整体搬进了
//! `assets/hifi.css`(搬法与两处改动见那个文件的头注)。屏里一律写 `class`,
//! 不写行内样式。
//!
//! 这个文件曾经还有 13 个色号常量和 6 个拼行内样式的函数,是上一版的做法 ——
//! 它让壳和高保真越走越远。九个屏逐个改成用样式表的类名之后,最后一个调用者
//! 也消失了,那一整段跟着删掉,不留两套并行的写法。要颜色就写
//! `var(--clay)` / `var(--ink-3)`,取值在样式表的 `:root` 里;要字体就写
//! `var(--serif)` / `var(--mono)`。
//!
//! 剩下这两件是 Rust 必须做的:把信号枚举翻成颜色变量名和中文标签。
//! **没数据是灰,不是绿** —— 这条在配色上也守住。

/// 整张样式表。挂在 `document::Style` 上,全局生效。
pub const GLOBAL_CSS: &str = include_str!("../../assets/hifi.css");

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
