//! 编排层的错误类型。切片四A 时只包一层存储错误;切片四B 加上
//! [`App::transition_issue`](crate::App::transition_issue) 这条守卫(§3.6
//! 「完成永远由人点」的编排层落点)之后,补一个「非法转移」变体——运行
//! 管理器自己的错误(`RunError`,「这件活已经有一个交付运行在跑」等)住
//! 在 [`crate::run`] 模块,不并进这里(设计上运行管理器的错误面与编排层
//! 的其它错误面是两回事,`RunManager` 的公开方法直接返回 `RunError`,不
//! 经这层转换)。

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Store(#[from] bw_store::StoreError),
    /// `bw_core::IssueStatus::can_transition_to` 判定不合法的转移——「完
    /// 成永远由人点」这条铁律在编排层的落点:`App::transition_issue` 写
    /// 之前必须查这张表,查不过就到不了 [`bw_store::IssueStore::transition_issue_status`]
    /// 那一步。
    #[error("活的状态不能从 {from:?} 转移到 {to:?}(合法转移表拒绝)")]
    IllegalTransition {
        from: bw_core::IssueStatus,
        to: bw_core::IssueStatus,
    },
    #[error("活不存在:{0:?}")]
    IssueNotFound(bw_core::IssueId),
    /// 转移在读到当前状态之后、真正写之前被别的并发写抢先了——`bw-store`
    /// `transition_issue_status` 的比较并置返回了 `false`。如实报,调用
    /// 方应该重读当前状态再决定,不要盲目重试同一次写。
    #[error("转移未生效(比较并置落空,当前状态已经变了):{from:?} → {to:?}")]
    TransitionRaced {
        from: bw_core::IssueStatus,
        to: bw_core::IssueStatus,
    },
}
