//! 两屏,不多不少(design-s5-hexpanel.md §1.1)。

pub mod attention;
pub mod hex;
// next 切片 5.5(design-s5-hexpanel.md §5.3):嵌入终端组件——不是第三
// 屏,挂在六段总控「当前 Loop」段的运行卡里(`hex::LoopSegmentView` →
// `hex::EscapeHatchCard` → `terminal::TerminalWidget`)。
pub mod terminal;
