//! 设计系统 token 与原子样式函数 —— 从 V3 原样抄来,不另起一套。
//!
//! 暖纸底色、clay 主色、四级灰阶、三态信号色 + Unknown 灰。**绿色隐身,只有
//! 红黄出声**:健康好的时候界面不该跳出来邀功,出问题才抢眼。

/// 底色(暖纸)
pub const PAPER: &str = "#EFEBE2";
/// 左栏 / 次级底色
pub const RAIL_BG: &str = "#E9E3D7";
/// 品牌 / clay
pub const CLAY: &str = "#C5654A";
/// 卡片底色
pub const CARD: &str = "#FBFAF6";
pub const CARD_ALT: &str = "#F4F0E7";
/// 边框
pub const BORDER: &str = "#E2DCCF";
pub const BORDER_DEEP: &str = "#DBD4C5";
/// 文字四级
pub const INK: &str = "#23211C";
pub const INK_2: &str = "#57534A";
pub const INK_3: &str = "#8C867A";
pub const INK_4: &str = "#A19B8D";
/// 警示深红
pub const ALERT_DEEP: &str = "#A33D29";
pub const SHADOW: &str = "0 8px 26px rgba(35,33,28,.08)";

pub const SERIF: &str = "'Noto Serif SC','Songti SC','STSong','SimSun',serif";
pub const SANS: &str =
    "'Noto Sans SC','PingFang SC','Hiragino Sans GB','Microsoft YaHei',sans-serif";
pub const MONO: &str = "'JetBrains Mono','SF Mono',Menlo,Consolas,monospace";

/// 三态信号色 + Unknown 灰。**没数据是灰,不是绿** —— 这条在配色上也守住。
pub fn signal_color(s: Option<bw_v4::Signal>) -> &'static str {
    match s {
        Some(bw_v4::Signal::Green) => "#5C8A5E",
        Some(bw_v4::Signal::Amber) => "#C9922F",
        Some(bw_v4::Signal::Red) => "#A33D29",
        Some(bw_v4::Signal::Unknown) | None => "#A19B8D",
    }
}

pub fn signal_label(s: Option<bw_v4::Signal>) -> &'static str {
    match s {
        Some(bw_v4::Signal::Green) => "正常",
        Some(bw_v4::Signal::Amber) => "注意",
        Some(bw_v4::Signal::Red) => "有问题",
        Some(bw_v4::Signal::Unknown) | None => "无数据",
    }
}

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
[draggable="true"] { cursor: grab; }
"#;

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

/// 「还没建」的占位块。**不放模拟数据** —— 留白如实标注是纪律,不是偷懒。
pub fn not_built() -> String {
    format!(
        "padding:28px;border:1px dashed {BORDER_DEEP};border-radius:10px;color:{INK_3};\
         font-size:13px;line-height:1.8;text-align:center;background:{CARD_ALT};"
    )
}
