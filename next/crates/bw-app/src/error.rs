//! 编排层的错误类型。切片四A 时只包一层存储错误;切片四B 加上
//! [`App::transition_issue`](crate::App::transition_issue) 这条守卫(§3.6
//! 「完成永远由人点」的编排层落点)之后,补一个「非法转移」变体——运行
//! 管理器自己的错误(`RunError`,「这件活已经有一个交付运行在跑」等)住
//! 在 [`crate::run`] 模块,当时不并进这里(设计上运行管理器的错误面与编
//! 排层的其它错误面是两回事,`RunManager` 的公开方法直接返回 `RunError`,
//! 不经这层转换)——那是切片四唯一调用方(`run_races` 指挥器)自己直接
//! 持有 `RunManager` 时的判断。
//!
//! next 切片五C(design-s5-hexpanel.md §4.2)补五个变体——`cmd::project`/
//! `cmd::metric` 两个聚合的用例层合法性判断(项目不存在 / 已经开过棒 /
//! 从未开棒不能交棒 / 指标不存在 / 正本文件解析失败)。
//!
//! **next 切片七C 更正上面那条「不并进这里」的判断**(design-s7-real-
//! project.md §1.1):`cmd::issue::run_issue`/`cmd::issue::cancel_run` 是第
//! 一个把「读活/项目 + 造工作树 + 起工」三步串成一个用例的调用点——壳与
//! `issue_kickoff` 指挥器都要能用**同一种**错误类型做 §1.5 的「失败如实
//! 显示的四种形态」分类,分开反而制造两套错误面要各自处理,不是「运行
//! 管理器自己的错误面」这条边界原本要防的那种揉合。这里补一条透传变体
//! ([`AppError::Run`]),`RunError` 这个类型本身不变、不缩小,只是多了一
//! 条 `?` 能自动转换的路。

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

    /// next 切片七C(design-s7-real-project.md §1.2①):`cmd::issue::
    /// run_issue` 第一步——项目没有配本地检出根(`root_path` 空串),开不
    /// 了工。**这一步失败,一条运行行都不会落库**(design §6.1 读回清单
    /// 第 1 条)——四种「如实失败形态」里唯一「一条运行行都没有」的那一
    /// 种(design §1.5「开不了工(前置不满足)」)。
    #[error("项目 {0:?} 还没有本地检出,开不了工")]
    ProjectNoCheckoutRoot(bw_core::ProjectId),

    /// next 切片七C(§1.2②):造工作树失败(不是 git 仓库/磁盘满/分支名
    /// 撞了等)——**如实回原文,不重试、不换路径**(design 原话)。这一步
    /// 失败同样不会有运行行落库(工作树是 `RunManager::start` 的入参之
    /// 一,没造出来就轮不到它)。
    #[error("造工作树失败:{0}")]
    WorktreeProvision(String),

    /// next 切片七C——`cmd::issue::run_issue`/`cmd::issue::cancel_run` 把
    /// [`crate::run::RunError`] 透传到这里(见本文件顶部模块文档「更正上
    /// 面那条判断」一节)。涵盖 design §1.2③ 的三个如实拒绝(已经有一个
    /// 交付运行在跑 / 工作区被别的运行占着 / 连接器零条或多条)。
    #[error(transparent)]
    Run(#[from] crate::run::RunError),
}
