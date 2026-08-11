//! 设计系统 token(`CLAUDE.md`「设计系统 token」一节 / 仓根旧壳
//! `crates/app-desktop/src/theme.rs`)——暖纸底色、clay 主色、三态信号色
//! + Unknown 灰、标题衬线/正文无衬线/数字等宽三体混排。
//!
//! **色值逐字复用旧工程**(design-s5-hexpanel.md §4.5「这些值在旧工程里
//! 分住两处…新工程统一放进壳的主题模块一处」):暖纸/clay/边框/文字等来
//!自 `crates/app-desktop/src/theme.rs`;三态信号色 + Unknown 灰来自
//! `crates/ui/src/lib.rs` 的 `signal_color`(next/ 没有 `ui` crate,这里
//! 是它在新工程里唯一的落点)。五阶段各自的品牌色**不复制**——直接从内
//! 核读 `bw_core::StageKind::color()`(§4.5「`StageKind` 自带的五色照旧
//! 从内核读,不复制」)。
//!
//! **字体本地打包的如实降级**(concern,详见 commit 正文/报告):`.bw`
//! 词表/plan/00 §6 要求把 Noto Serif/Sans SC + JetBrains Mono 三套字体随
//! 包打进去。这个仓库里没有任何字体二进制文件可用(`find` 过,零命
//! 中),而这个任务的执行环境没有可靠的、经许可的路径去获取字体二进制
//! 并核实授权——**不臆造**一份没有真实来源的字体资源。因此本片与仓根旧
//! 壳一样,退回系统 CJK 字体栈兜底(Songti/PingFang · SimSun/微软雅
//! 黑),字体族名与旧壳逐字一致,行为在这台机器上离线正确;真正的随包
//! 打包留给下一次有字体资源可用时补(不是本片假装做了)。

/// 底色(暖纸)
pub const PAPER: &str = "#EFEBE2";
/// 图标栏/顶栏底色
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
/// 警示深红(toast/致命错误)
pub const ALERT_DEEP: &str = "#A33D29";
/// 阴影
pub const SHADOW: &str = "0 8px 26px rgba(35,33,28,.08)";

pub const SERIF: &str = "'Noto Serif SC','Songti SC','STSong','SimSun',serif";
pub const SANS: &str =
    "'Noto Sans SC','PingFang SC','Hiragino Sans GB','Microsoft YaHei',sans-serif";
pub const MONO: &str = "'JetBrains Mono','SF Mono',Menlo,Consolas,monospace";

/// 三态信号色 + Unknown 灰——逐字复用旧工程 `crates/ui/src/lib.rs`
/// `signal_color` 的色值(不是重新配色)。**绿色隐身**这条规则不在这里
/// 落地(那是「正常折成一个数、只浮出例外」的聚合规则,§9),这个函数
/// 只管「画一个信号点该用什么颜色」,画不画、画在哪由调用方决定。
pub fn signal_color(s: bw_core::Signal) -> &'static str {
    match s {
        bw_core::Signal::Green => "#5F7355",
        bw_core::Signal::Amber => "#B5862F",
        bw_core::Signal::Red => "#B0503A",
        bw_core::Signal::Unknown => "#A19B8D",
    }
}

/// Global stylesheet:selection、滚动条、resets(逐字搬旧壳同名常量)。
pub const GLOBAL_CSS: &str = r#"
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; height: 100%; }
::selection { background: #E7CFC4; }
::-webkit-scrollbar { width: 10px; height: 10px; }
::-webkit-scrollbar-thumb { background: #D8D1C2; border-radius: 6px; border: 3px solid #EFEBE2; }
::-webkit-scrollbar-track { background: transparent; }
button { font-family: inherit; cursor: pointer; }
select { font-family: inherit; }
"#;

/// 信号点的内联样式。
pub fn dot(color: &str, size: u32) -> String {
    format!(
        "width:{size}px;height:{size}px;border-radius:50%;background:{color};display:inline-block;flex:none;"
    )
}

/// 标准卡片外壳。
pub fn card() -> String {
    format!("background:{CARD};border:1px solid {BORDER};border-radius:10px;box-shadow:{SHADOW};padding:16px 18px;")
}

/// 虚线灰卡——「无观测 · Unknown ≠ 绿」的诚实空态外壳(design §9「诚实
/// 灰」,搬 v1 版面判断,不搬代码)。
pub fn card_dashed_gray() -> String {
    format!(
        "background:{CARD_ALT};border:1.5px dashed {BORDER_DEEP};border-radius:10px;padding:16px 18px;color:{INK_3};"
    )
}

/// 小标签 chip。
pub fn chip(bg: &str, fg: &str) -> String {
    format!(
        "display:inline-block;padding:2px 8px;border-radius:6px;background:{bg};color:{fg};font-size:11px;line-height:16px;white-space:nowrap;"
    )
}
