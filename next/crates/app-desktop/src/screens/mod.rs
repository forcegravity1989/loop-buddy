//! 两屏,不多不少(design-s5-hexpanel.md §1.1)。

pub mod attention;
pub mod hex;
// next 切片 5.5(design-s5-hexpanel.md §5.3):嵌入终端组件——不是第三
// 屏,挂在六段总控「当前 Loop」段的运行卡里(`hex::LoopSegmentView` →
// `hex::EscapeHatchCard` → `terminal::TerminalWidget`)。
pub mod terminal;

/// 会话号前 8 位,空 = 「—」(展示层格式化,不是推导——这条指标本身已
/// 经在存储层是既成事实,这里只决定怎么显示)。`hex`/`attention` 两屏共
/// 用同一份格式化(此前只有 `hex.rs` 私有一份,`attention.rs` 需要同一
/// 条格式化时不该另抄一遍),因此挂在这个两屏共同的父模块上,不是随便
/// 找个地方放。
pub(crate) fn short_session(upstream_session: &str) -> String {
    if upstream_session.is_empty() {
        "—".to_string()
    } else {
        upstream_session.chars().take(8).collect()
    }
}

/// 「结束事实」的展示层格式化(design-s7-real-project.md §1.5「失败如实
/// 显示的四种形态」)——**这里没有任何推导**:`end_detail`(上游报错原
/// 文/结束事实原文)、`started_at`/`ended_at`(耗时)都已经是运行行里的
/// 既成事实,这个函数只把它们拼成一行人话;`task-s7b-review.md`
/// Important-4 点名"卡片当场变红 + 上游报错原文"(形态 2)/"结束事实原
/// 文 + 耗时"(形态 3)此前没有接进任何一屏,这是它们的第一个消费者。
/// **没结束**(`ended_at` 为空,运行仍在跑)→ 空串:那不是这个函数该管
/// 的场景,调用方(逃生舱卡片)自己有一套「进行中」的文案。
pub(crate) fn run_end_summary(r: &bw_store::RunRow) -> String {
    let Some(ended_at) = r.ended_at else {
        return String::new();
    };
    let secs = (ended_at - r.started_at).max(0);
    let detail = if r.end_detail.trim().is_empty() {
        "(没有结束事实原文)"
    } else {
        r.end_detail.trim()
    };
    format!("耗时 {secs}s · {detail}")
}
