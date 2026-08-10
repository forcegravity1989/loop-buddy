//! `bw-store` — next 切片四A:最小本地存储(design-s4-runmanager.md §2)。
//!
//! 三张表(项目/活/运行),接口按聚合拆成两条 trait(裁决 #7):
//! [`IssueStore`] 管活,[`RunStore`] 管运行。**只依赖 `bw-core`**——编译期
//! 看不见 `bw-connector` 的协议类型(`ExecState`/`ExecTicket`/...),想在这
//! 层写一句「如果执行状态是 X 就把活推到 Y」都写不出来(§2.1)。
//!
//! 两把结算/关门守卫,**语义照搬 v1、形态各不相同**(§2.3):
//! - `issue.settled_at`:原样移植 v1 的 `COALESCE(settled_at, ?)`——调用方
//!   不需要知道自己是不是第一个,只要「结过一次就不会再结」。
//! - `run.ended_at` / `run.settled_at`:升级成**比较并置**
//!   (`WHERE … IS NULL`),受影响行数告诉调用方自己是不是第一个抵达——
//!   取消与完成撞车时,这是唯一能分出胜负的信号。
//!
//! 「一件活一个活着的交付运行」钉在**数据库的部分唯一索引**上
//! (`uq_run_live_delivery_per_issue`),不是 if 判断——见 `schema.sql`。
//!
//! 「完成永远由人点」这条铁律在存储层的体现:这个 crate 里没有任何一个
//! 把 `issue.status` 推到 `Done` 的便捷方法。合法转移表(`can_transition_to`)
//! 住在 `bw-core`,店存只管写它被告知要写的值。

#![forbid(unsafe_code)]

mod sqlite;
pub use sqlite::SqliteStore;

use async_trait::async_trait;
use bw_core::{IssueId, IssueStatus, ProjectId, RunId, RunState};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

impl StoreError {
    /// 数据库报错分类:是不是唯一约束冲突。部分唯一索引把第二个交付运行
    /// 挡下时,SQLite 报的正是这一类——调用方(将来是运行管理器)靠这个
    /// 稳定的判别点区分「这件活已经有一个交付运行在跑」与其它随机失败,
    /// 不用各自解析错误原文。
    pub fn is_unique_violation(&self) -> bool {
        match self {
            StoreError::Sqlx(sqlx::Error::Database(e)) => e.is_unique_violation(),
            _ => false,
        }
    }
}

// ───────────────────────────── project / issue ─────────────────────────────

#[derive(Clone, Debug)]
pub struct NewProject {
    pub id: ProjectId,
    pub name: String,
    /// 本地 git 检出根;空 = 未配置(design §2.2 `project.root_path`)。
    pub root_path: String,
}

#[derive(Clone, Debug)]
pub struct ProjectRow {
    pub id: ProjectId,
    pub name: String,
    pub root_path: String,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct NewIssue {
    pub id: IssueId,
    pub project_id: ProjectId,
    /// 项目内序号,调用方分配(1, 2, 3…)。
    pub number: i64,
    pub title: String,
}

#[derive(Clone, Debug)]
pub struct IssueRow {
    pub id: IssueId,
    pub project_id: ProjectId,
    pub number: i64,
    pub title: String,
    pub status: IssueStatus,
    pub settled_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 管活的一条 trait(裁决 #7:按聚合拆,不是一个 trait 堆方法)。切片五加
/// 观测/交棒/风险/决策等聚合是加第三条 trait,不是往这条上堆方法。
#[async_trait]
pub trait IssueStore: Send + Sync {
    async fn create_project(&self, p: NewProject) -> Result<()>;
    async fn get_project(&self, id: ProjectId) -> Result<Option<ProjectRow>>;

    async fn create_issue(&self, i: NewIssue) -> Result<()>;
    async fn get_issue(&self, id: IssueId) -> Result<Option<IssueRow>>;

    /// 活的结算——**原样移植** v1 `mark_issue_settled` 的 COALESCE 语义
    /// (design §2.3①):一件活重开又重做,不会结算两次;调用方不需要
    /// 知道自己是不是第一个,恒 `Ok`。**这不是**把活推到 Done 的方法——
    /// 那条转移由 `bw-core::can_transition_to` 守,这里只记「结过账了」
    /// 这个事实。
    async fn settle_issue(&self, id: IssueId, at: i64) -> Result<()>;

    /// 开工时把活推到「进行中」——运行管理器(切片四B)**唯一**改活状态的
    /// 写(design-s4-runmanager.md §3.6)。**签名上没有 `IssueStatus` 形
    /// 参**:这是「运行管理器没有任何写 Done 的路径」这条硬约束在类型层
    /// 面的一部分落实——这个方法天生写不出「进行中」之外的任何值,不需
    /// 要靠代码审查记住「别传 Done 进来」。无条件写(不比较并置):「进
    /// 行中」是这条状态机的合法起点,也是诚实失败后重试的合法落点
    /// (design §3.4①),重复调用无害。
    async fn mark_issue_in_progress(&self, id: IssueId, at: i64) -> Result<()>;
}

// ───────────────────────────── run ─────────────────────────────

/// `run.kind`——'delivery' | 'consultation'。降级为咨询 = 这一列翻面
/// (design §3.5)。不进 `bw-core`:它是这张表自己的列值,不是 design §7
/// 点名要下沉进内核的两小件之一。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunKind {
    Delivery,
    Consultation,
}

pub(crate) fn run_kind_text(k: RunKind) -> &'static str {
    match k {
        RunKind::Delivery => "delivery",
        RunKind::Consultation => "consultation",
    }
}

pub(crate) fn parse_run_kind(s: &str) -> Result<RunKind> {
    match s {
        "delivery" => Ok(RunKind::Delivery),
        "consultation" => Ok(RunKind::Consultation),
        other => Err(StoreError::Other(format!("bad run.kind {other:?}"))),
    }
}

/// `run.end_kind`——上游/BW 对「这次怎么结束的」的诚实分类。`NULL`(不是
/// 这个类型的一个变体)才是「不知道」——重启遗留就是 `NULL`,绝不填一个
/// 猜的(design §2.2 注释)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RunEndKind {
    ProcessExit,
    StoppedByBw,
    ContactLost,
    Canceled,
    StartFailed,
}

pub(crate) fn run_end_kind_text(k: RunEndKind) -> &'static str {
    match k {
        RunEndKind::ProcessExit => "process_exit",
        RunEndKind::StoppedByBw => "stopped_by_bw",
        RunEndKind::ContactLost => "contact_lost",
        RunEndKind::Canceled => "canceled",
        RunEndKind::StartFailed => "start_failed",
    }
}

pub(crate) fn parse_run_end_kind(s: &str) -> Result<RunEndKind> {
    match s {
        "process_exit" => Ok(RunEndKind::ProcessExit),
        "stopped_by_bw" => Ok(RunEndKind::StoppedByBw),
        "contact_lost" => Ok(RunEndKind::ContactLost),
        "canceled" => Ok(RunEndKind::Canceled),
        "start_failed" => Ok(RunEndKind::StartFailed),
        other => Err(StoreError::Other(format!("bad run.end_kind {other:?}"))),
    }
}

#[derive(Clone, Debug)]
pub struct NewRun {
    pub id: RunId,
    pub project_id: ProjectId,
    pub issue_id: IssueId,
    pub kind: RunKind,
    pub connector_name: String,
    /// 连接器那次调用的请求编号(证据链要能对上);**不是**晚到消息的钥
    /// 匙(design §4.3——那个钥匙是 `RunId` 本身)。
    pub req_id: String,
    pub workspace: String,
    pub branch: String,
    /// 起插时通常是 `RunState::Starting`——先占名额,再外呼开工
    /// (design §3.4 第三行)。
    pub state: RunState,
    pub started_at: i64,
}

#[derive(Clone, Debug)]
pub struct RunRow {
    pub id: RunId,
    pub project_id: ProjectId,
    pub issue_id: IssueId,
    pub kind: RunKind,
    pub connector_name: String,
    pub req_id: String,
    pub upstream_session: String,
    pub workspace: String,
    pub branch: String,
    pub state: RunState,
    pub end_kind: Option<RunEndKind>,
    pub end_detail: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    pub settled_at: Option<i64>,
    pub demoted_at: Option<i64>,
}

/// 管运行的一条 trait(裁决 #7)。两把守卫的落点都在这里:
/// [`RunStore::create_run`] 撞的是部分唯一索引;[`RunStore::close_run`] /
/// [`RunStore::settle_run`] 用的是比较并置,返回值告诉调用方自己是不是
/// 第一个抵达的——运行管理器(下一任务)靠这个区分「真结这次账」还是
/// 「诚实空转」。
#[async_trait]
pub trait RunStore: Send + Sync {
    /// 先插行占名额(design §3.4 第三行「先插行再外呼」的「插行」这一半;
    /// 「再外呼」是运行管理器的事,不在这个 trait 里)。撞
    /// `uq_run_live_delivery_per_issue` 时返回的 `StoreError` 用
    /// [`StoreError::is_unique_violation`] 分类——不是普通失败,是「这件
    /// 活已经有一个交付运行在跑」的如实信号。
    async fn create_run(&self, r: NewRun) -> Result<()>;
    async fn get_run(&self, id: RunId) -> Result<Option<RunRow>>;

    /// 关门(结束回写)的比较并置:`WHERE ended_at IS NULL`。返回
    /// `true` = 这次调用是第一个抵达的,真的写了 `ended_at`/`state`/
    /// `end_kind`/`end_detail` 这几列;`false` = 已经有人先关过门,本次
    /// 诚实空转,一个字段都没改(design §2.3②)。
    async fn close_run(
        &self,
        id: RunId,
        ended_at: i64,
        state: RunState,
        end_kind: Option<RunEndKind>,
        end_detail: &str,
    ) -> Result<bool>;

    /// 结算的比较并置:`WHERE settled_at IS NULL`。返回值同
    /// [`RunStore::close_run`]——取消与完成同时到达时,由「谁抢到」决定
    /// 谁去做后续记账动作(design §2.3②)。
    async fn settle_run(&self, id: RunId, at: i64) -> Result<bool>;

    /// 一件活当前活着的交付运行编号(若有)。**跨重启也成立**——不靠运行
    /// 管理器进程内的 `by_issue` 缓存,直接查
    /// `uq_run_live_delivery_per_issue` 索引覆盖的那一行(design §3.4 第
    /// 二行「存储才是真正的守卫」)。`create_run` 撞唯一索引之后,调用方
    /// 靠这个查询把「已经有一个交付运行在跑」的错误消息补上具体是哪一条
    /// ——包括进程刚重启、内存缓存还是空的那种情形。
    async fn find_live_delivery_run(&self, issue_id: IssueId) -> Result<Option<RunId>>;

    /// 起工成功:`state` 从 `starting` 推到 `running`,记下上游会话号。比
    /// 较并置(`WHERE state = 'starting'`)——设计上这一步不存在第二个写
    /// 家会跟它抢,但仍然用 CAS 防御式写,不假设「只有我会调」。
    async fn mark_run_started(&self, id: RunId, upstream_session: &str, at: i64) -> Result<bool>;

    /// 重启后收拾遗留:所有还开着(`ended_at IS NULL`)的运行行,**一次
    /// UPDATE** 全部标成 `orphaned`(design §9「集合式 UPDATE」,不是按编
    /// 号挨个循环——百级并行不含小 N 假设)。`end_kind`/`end_detail` 如实
    /// 标注「不知道怎么结束的」,`settled_at` 不动(账没结,如实欠着)。
    /// 返回被标注的运行编号列表,供 `ReapReport`/「不批量唤醒」断言用。
    async fn reap_open_runs(&self, at: i64) -> Result<Vec<RunId>>;
}
