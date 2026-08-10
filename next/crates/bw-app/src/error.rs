//! 编排层的错误类型。切片四A 时只包一层存储错误;切片四B 加上
//! [`App::transition_issue`](crate::App::transition_issue) 这条守卫(§3.6
//! 「完成永远由人点」的编排层落点)之后,补一个「非法转移」变体——运行
//! 管理器自己的错误(`RunError`,「这件活已经有一个交付运行在跑」等)住
//! 在 [`crate::run`] 模块,不并进这里(设计上运行管理器的错误面与编排层
//! 的其它错误面是两回事,`RunManager` 的公开方法直接返回 `RunError`,不
//! 经这层转换)。
//!
//! next 切片五C(design-s5-hexpanel.md §4.2)补五个变体——`cmd::project`/
//! `cmd::metric` 两个聚合的用例层合法性判断(项目不存在 / 已经开过棒 /
//! 从未开棒不能交棒 / 指标不存在 / 正本文件解析失败)。

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
    #[error("项目不存在:{0:?}")]
    ProjectNotFound(bw_core::ProjectId),
    /// `cmd::project::set_active_stage`——已经开过棒的项目不能再「首次开
    /// 棒」,要往下一棒走该用交棒(`cmd::project::handoff_stage`)。
    #[error(
        "项目 {project:?} 已经开棒在 {current:?},不能再次「首次开棒」——要往下一棒走,用交棒命令"
    )]
    StageAlreadySet {
        project: bw_core::ProjectId,
        current: bw_core::StageKind,
    },
    /// `cmd::project::handoff_stage`——从未开棒的项目没有「从」这个非空
    /// 起点,`handoff.from_stage` 是 `NOT NULL`。
    #[error("项目 {0:?} 从未开棒,不能交棒(没有「从」这个起点)")]
    NoActiveStage(bw_core::ProjectId),
    #[error("指标不存在:{0:?}")]
    MetricNotFound(bw_core::MetricId),
    /// `cmd::metric::sync_metrics_file`——项目仓 `.bw/metrics.toml` 存在但
    /// 解析失败(语法错/字段缺)。**这一步失败,函数在这里就返回**,不会
    /// 走到 `MetricStore::sync_metrics_from_file`——「文件必须整份解析成
    /// 功才会有任何 SQLite 写入」这条语义(`docs/metrics-toml-format.md`)
    /// 在编排层这一侧的落点(五-1 report concern 3 点名的那半句)。
    #[error("项目仓 .bw/metrics.toml 解析失败:{0}")]
    MetricsFile(String),
}
