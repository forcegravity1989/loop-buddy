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
use bw_core::{
    HandoffId, IssueId, IssueStatus, MetricId, ObservationId, ProjectId, RunId, RunState, StageKind,
};

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
    /// next 切片五B:五阶段方法论的当前一棒(1..5,`bw_core::StageKind` 已
    /// 解析)。`None` = 尚未开棒——不默认成 `Prototype`,「还没开始」和
    /// 「在原型阶段」是两件事(design §2.1)。`create_project` 不填这一
    /// 列,新项目恒为 `None`;要翻它,走 [`HandoffStore::set_active_stage`]
    /// (首次开棒)或 [`HandoffStore::handoff_stage`](推进到下一棒)。
    pub active_stage: Option<StageKind>,
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

// next 切片五D:`PartialEq` 是 `hex_readback` 指挥器「重启前后同一查询
// 逐字节比对」(design-s5-hexpanel.md §7.2 第 4 节,裁决 4 的新验收形
// 态)要用的——两次调用同一条组装用例产出的视图要能直接 `==` 比较,不
// 靠格式化字符串比对这种更脆的手法。只加派生,不改字段/行为。
#[derive(Clone, Debug, PartialEq)]
pub struct IssueRow {
    pub id: IssueId,
    pub project_id: ProjectId,
    pub number: i64,
    pub title: String,
    pub status: IssueStatus,
    pub settled_at: Option<i64>,
    /// next 切片五B:这件活归哪个阶段。`None` = 未归类(同
    /// `ProjectRow::active_stage` 的「不默认」原则)。`create_issue` 不填
    /// 这一列,新活恒为 `None`——本片没有把它推到某个值的用例(§8 范围
    /// 裁剪),这是留给下一片的真实缺口,不是遗漏(见 commit 正文)。
    pub stage: Option<StageKind>,
    /// next 切片五B:活的描述正文。`create_issue` 不填这一列,靠 schema
    /// 的 `DEFAULT ''` 落空串——同上,本片没有写它的用例。
    pub body: String,
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

    /// 活状态的比较并置转移——纯机械写,**不判断合法性**:合法性由调用方
    /// (编排层 `bw-app::App::transition_issue`)在写之前查
    /// `bw_core::IssueStatus::can_transition_to` 决定,这里只管写它被告知
    /// 要写的值。这不是给 `RunManager` 用的「便捷方法」——`RunManager` 的
    /// 代码路径里没有一处调用它(那条铁律靠 `RunManager` 自己只用上面的
    /// `mark_issue_in_progress` 一条窄方法维持,不是靠这个方法自己判
    /// 断)。`WHERE status = ?`(调用方读到的当前值)是比较并置:返回
    /// `false` = 调用方手里的「当前状态」已经不是数据库里的当前状态,没
    /// 有发生写,调用方应该重读后再决定,不要盲目重试同一次写。
    async fn transition_issue_status(
        &self,
        id: IssueId,
        from: IssueStatus,
        to: IssueStatus,
        at: i64,
    ) -> Result<bool>;

    /// next 切片五C/D(design-s5-hexpanel.md §3.2②):状态匹配的活列表,
    /// 按更新时刻升序(先进先出——待人处理②「评审中等你点」用这个顺序,
    /// 六段视图「当前 Loop」的评审中计数与 [`crate::run::RunSnapshot`] 的
    /// `in_review` 字段共用同一条查询,不重复发明第二本账)。`project=None`
    /// 时横跨全库——裁决 10:投影/查询层不内含单项目假设。
    async fn list_issues_by_status(
        &self,
        project: Option<ProjectId>,
        status: IssueStatus,
    ) -> Result<Vec<IssueRow>>;

    /// next 切片五C(design §1.2②):一个项目里按阶段分组的活数——五角色
    /// 责任卡「这个阶段有几件活」的真实数据源。`None` 那一档是「未归类」
    /// (`issue.stage IS NULL`),不计入任何 `StageKind`。
    async fn count_issues_by_stage(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<(Option<StageKind>, i64)>>;
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

#[derive(Clone, Debug, PartialEq)] // next 切片五D:同 `IssueRow` 上方注记
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

    /// 降级为咨询:`kind` 从 `'delivery'` 翻成 `'consultation'` + `demoted_at`
    /// 落值,比较并置(`WHERE kind = 'delivery' AND ended_at IS NULL`,
    /// design §3.5①)。返回 `true` = 这次真降级了;`false` = 没什么可降的
    /// (已经不是活着的交付运行——幂等)。翻面之后这一行退出
    /// `uq_run_live_delivery_per_issue` 的谓词,交付名额当场释放。
    async fn demote_run(&self, id: RunId, at: i64) -> Result<bool>;

    /// 重启后收拾遗留:所有还开着(`ended_at IS NULL`)的运行行,**一次
    /// UPDATE** 全部标成 `orphaned`(design §9「集合式 UPDATE」,不是按编
    /// 号挨个循环——百级并行不含小 N 假设)。`end_kind`/`end_detail` 如实
    /// 标注「不知道怎么结束的」,`settled_at` 不动(账没结,如实欠着)。
    /// 返回被标注的运行编号列表,供 `ReapReport`/「不批量唤醒」断言用。
    async fn reap_open_runs(&self, at: i64) -> Result<Vec<RunId>>;

    /// next 切片五C/D(design §1.2⑥/§3.2):一个项目(或全库,`project=None`
    /// ——裁决 10)的全部运行,按开工时刻倒序。「交付证据」段①(每次运行
    /// 的账:结束事实原文、耗时、上游会话号、结没结账)与 hex 视图组装用
    /// 例共用同一条数据源。
    async fn list_runs(&self, project: Option<ProjectId>) -> Result<Vec<RunRow>>;

    /// next 切片五D(design §3.2①):关了门但没结账的运行——待人处理①/
    /// hex 视图「当前 Loop」段「遗留未结账」计数共用同一条查询。
    async fn list_unsettled_runs(&self, project: Option<ProjectId>) -> Result<Vec<RunRow>>;

    /// next 切片五D(design §3.2③):失败/遗留,且这件活之后没有更晚开的
    /// 运行接手——待人处理③「停在原地的运行」。
    async fn list_stalled_runs(&self, project: Option<ProjectId>) -> Result<Vec<RunRow>>;

    /// next 切片五C(design §1.2④「最近失败」聚合数的真实数据源):
    /// `state='failed'` 的运行数——**不叠加**「没有更晚运行」那条收窄
    /// (那是 [`RunStore::list_stalled_runs`] 专属的更严格判据)。两个数字
    /// 故意不同:一个答「发生过多少次失败」,一个答「现在还悬着没人管的
    /// 失败」。
    async fn count_failed_runs(&self, project: Option<ProjectId>) -> Result<i64>;
}

// ───────────────────────────── metric(next 切片五B)─────────────────────────────
//
// design-s5-hexpanel.md §1.3/§2.2:指标三层同构——北极星和滞后、引领走
// 完全同一张表、同一条派生链,不是特例。正本永远是项目仓的
// `.bw/metrics.toml`;这里只是本地缓存,同步器只碰 `origin='file'` 的
// 行,界面手建的行(`origin='manual'`)同步器不碰。

/// `metric.tier`——三层同构的层级取值。北极星不是独立类型,是这张表里
/// 至多一行(`uq_metric_north_star` 部分唯一索引钉死)的一条特殊指标。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MetricTier {
    NorthStar,
    Lagging,
    Leading,
}

pub(crate) fn metric_tier_text(t: MetricTier) -> &'static str {
    match t {
        MetricTier::NorthStar => "north_star",
        MetricTier::Lagging => "lagging",
        MetricTier::Leading => "leading",
    }
}

pub(crate) fn parse_metric_tier(s: &str) -> Result<MetricTier> {
    match s {
        "north_star" => Ok(MetricTier::NorthStar),
        "lagging" => Ok(MetricTier::Lagging),
        "leading" => Ok(MetricTier::Leading),
        other => Err(StoreError::Other(format!("bad metric.tier {other:?}"))),
    }
}

/// 正本同步的输入——**不是** `bw_workspace::metric_shape::FlatMetricDef`
/// 本身(那个类型住在 `bw-workspace`,这个 crate 只依赖 `bw-core`,见
/// crate 文档「store 无业务判断」那条边界)。调用方(编排层,或本片的
/// `store_guards` 指挥器)把 `bw-workspace` 归一出来的扁平定义逐字段搬
/// 成这个类型再调用 [`MetricStore::sync_metrics_from_file`]。
#[derive(Clone, Debug)]
pub struct IncomingMetricDef {
    pub tier: MetricTier,
    pub name: String,
    pub def: String,
    pub target_raw: String,
    pub collect_kind: String,
    pub collect_query: String,
}

#[derive(Clone, Debug, PartialEq)] // next 切片五D:同 `IssueRow` 上方注记
pub struct MetricRow {
    pub id: MetricId,
    pub project_id: ProjectId,
    pub tier: MetricTier,
    pub name: String,
    pub def: String,
    pub target_raw: String,
    pub collect_kind: String,
    pub collect_query: String,
    /// `"file"` | `"manual"`。同步器只碰 `"file"` 的行。
    pub origin: String,
    /// 停用时刻;`None` = 在用。整条退出派生链,历史观测原样留着——永不
    /// 物理删除(`docs/metrics-toml-format.md` 的「停用 ≠ 删除」同一条规矩)。
    pub archived_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 一次 [`MetricStore::sync_metrics_from_file`] 调用的结果——每一类动作
/// 各一份编号列表,调用方(指挥器/未来的命令回执)据此打印「正本里已删
/// 除的 N 条自动停用」这类如实汇报,不用自己再去 diff 前后状态。
#[derive(Clone, Debug, Default)]
pub struct MetricSyncReport {
    pub inserted: Vec<MetricId>,
    pub updated: Vec<MetricId>,
    /// 正本里已经没有、且这行原是 `origin='file'` → 自动停用(不物删)。
    pub archived: Vec<MetricId>,
    /// 曾被停用、这次又出现在正本里 → 自动恢复(`archived_at` 清空)。
    pub restored: Vec<MetricId>,
}

/// 管指标的一条 trait(裁决 #7 的延续:按聚合拆,不是往 `IssueStore`/
/// `RunStore` 上堆方法)。
#[async_trait]
pub trait MetricStore: Send + Sync {
    /// 界面手建一条指标(`origin='manual'`)。同步器(`sync_metrics_from_file`)
    /// 永不碰这一行——`docs/metrics-toml-format.md`「界面手建的行同步器
    /// 不碰」那条老规矩,新工程原样守。
    #[allow(clippy::too_many_arguments)]
    async fn create_manual_metric(
        &self,
        id: MetricId,
        project_id: ProjectId,
        tier: MetricTier,
        name: &str,
        def: &str,
        target_raw: &str,
        collect_kind: &str,
        collect_query: &str,
        at: i64,
    ) -> Result<()>;

    async fn get_metric(&self, id: MetricId) -> Result<Option<MetricRow>>;

    /// 一个项目的全部指标行(含已停用的——过滤是调用方的事,store 不替
    /// 调用方拿主意)。
    async fn list_metrics(&self, project_id: ProjectId) -> Result<Vec<MetricRow>>;

    /// 正本同步(design §2.2「同步语义」,原样移植 v1 的判断,形态改成
    /// 三层同构):`incoming` 是这一次同步要生效的**全部** `origin='file'`
    /// 定义(通常是 `bw_workspace::metric_shape::flatten` 的结果逐字段搬
    /// 过来的)。
    ///
    /// - 按「项目 + 层级 + 名字」对上 → 原地更新 `def`/`target_raw`/
    ///   `collect_kind`/`collect_query`/`updated_at`,**保住原来的行
    ///   id**(挂在它下面的观测历史不受影响);若这行此前被停用,顺带
    ///   恢复(`archived_at` 清空)。
    /// - 对不上 → 新建一行,`origin='file'`。
    /// - 已存在、`origin='file'`、这次不在 `incoming` 里 → 自动停用
    ///   (`archived_at` 落值,不物删)。
    /// - `origin='manual'` 的行永远不在这次比对范围内——正本里从来就没
    ///   有它,「正本删了它」这个判断对它不成立。
    ///
    /// 整个过程在一个事务里完成(design §2.2「一个字节不碰」的姊妹保证:
    /// 这次同步要么全部生效,要么一行都不改)。北极星那一行如果撞上
    /// `uq_metric_north_star`(比如界面手建过一条 `origin='manual'` 的北
    /// 极星,正本又来同步一条)会如实报 `StoreError`(`is_unique_violation`
    /// 可分类),不悄悄吞掉——「一个项目至多一个北极星」是数据库钉死的
    /// 铁律,不是这层的业务判断。
    async fn sync_metrics_from_file(
        &self,
        project_id: ProjectId,
        incoming: Vec<IncomingMetricDef>,
        at: i64,
    ) -> Result<MetricSyncReport>;

    /// 显式停用一条指标(界面手建行的停用入口——正本行不给这个按钮,见
    /// `docs/metrics-toml-format.md`「从该文件里删掉即停用」)。`true` =
    /// 这次真停用了;`false` = 已经停用过,幂等空转。
    async fn archive_metric(&self, id: MetricId, at: i64) -> Result<bool>;

    /// next 切片五D(design §3.2④a):从来没有观测过的指标(未停用的才
    /// 算)——待人处理④「指标没数 / 数据过期」的第一半。
    async fn list_metrics_without_observation(
        &self,
        project: Option<ProjectId>,
    ) -> Result<Vec<MetricRow>>;

    /// next 切片五D(design §3.2④b):每条指标(未停用)最新一条观测的时
    /// 刻。**过不过期不在这里判断**——那是内核按这条指标的节奏窗口算的
    /// (`bw_core::derive::measure`),SQL 只负责把「最新是什么时候」老实
    /// 交出来,在 SQL 里再判一遍阈值就是给同一条规则开第二本账。
    async fn list_metric_latest_observation(
        &self,
        project: Option<ProjectId>,
    ) -> Result<Vec<MetricLatestObservation>>;
}

/// [`MetricStore::list_metric_latest_observation`] 的一行——指标身份 + 它
/// 最新一条观测的时刻(`None` = 一条观测都没有,与
/// [`MetricStore::list_metrics_without_observation`] 是同一件事的两种问
/// 法,数据来源相同,取用场景不同:前者要「从没采过」的清单,这里要「每
/// 条指标各自最新一次是什么时候」以便调用方按节奏窗口自己判过期)。
#[derive(Clone, Debug)]
pub struct MetricLatestObservation {
    pub id: MetricId,
    pub project_id: ProjectId,
    pub tier: MetricTier,
    pub name: String,
    pub latest_ts: Option<i64>,
}

// ───────────────────────────── observation(next 切片五B)─────────────────────────────

#[derive(Clone, Debug)]
pub struct NewObservation {
    pub id: ObservationId,
    pub metric_id: MetricId,
    pub project_id: ProjectId,
    pub ts: i64,
    pub raw_value: String,
    pub source: String,
    pub source_hint: String,
}

#[derive(Clone, Debug, PartialEq)] // next 切片五D:同 `IssueRow` 上方注记
pub struct ObservationRow {
    pub id: ObservationId,
    pub metric_id: MetricId,
    pub project_id: ProjectId,
    pub ts: i64,
    pub raw_value: String,
    pub source: String,
    pub source_hint: String,
    pub created_at: i64,
}

/// 观测只追加,一个观测一个点(design §2.3)。**这不是纪律,是结构约
/// 束**——这条 trait 上只有 insert 与 query,编译期就没有 update/delete
/// 方法可调,`绝不修改、绝不插值` 因此在类型层面写不出来。
#[async_trait]
pub trait ObservationStore: Send + Sync {
    async fn insert_observation(&self, o: NewObservation) -> Result<()>;

    /// 一条指标的全部观测,按时间倒序(最新的在前)——`view/` 里画折线图
    /// 或取「最新一条」都是这个顺序上的切片,不需要调用方自己再排一次。
    async fn list_observations(&self, metric_id: MetricId) -> Result<Vec<ObservationRow>>;
}

// ───────────────────────────── handoff(next 切片五B)─────────────────────────────

#[derive(Clone, Debug, PartialEq)] // next 切片五D:同 `IssueRow` 上方注记
pub struct HandoffRow {
    pub id: HandoffId,
    pub project_id: ProjectId,
    pub from_stage: StageKind,
    pub to_stage: StageKind,
    pub risky: bool,
    pub note: String,
    pub created_at: i64,
}

/// 阶段交棒的审计流水,只追加(design §2.4,字段与索引逐字移植 v1 同名
/// 表)。清单没勾完也允许交,带险交棒被永久记下——事后不改、不删。
#[async_trait]
pub trait HandoffStore: Send + Sync {
    /// 首次「开棒」:`project.active_stage` 从 `NULL` 推到某一棒,不写
    /// `handoff` 流水(没有「从」——`from_stage` 需要一个非空的起点,首次
    /// 开棒没有这个起点,交棒表因此不适合记这一步)。无条件写。
    async fn set_active_stage(
        &self,
        project_id: ProjectId,
        stage: StageKind,
        at: i64,
    ) -> Result<()>;

    /// 交棒:插入交棒审计流水的同时把 `project.active_stage` 推到 `to`
    /// ——原子操作(同一个事务),原样搬 v1 `handoff_stage` 的语义。**这
    /// 不是判断合法性的地方**——从哪棒交到哪棒(是不是 `StageKind::next()`
    /// 那一棒)由调用方决定,这里只管写,同 `RunStore`/`IssueStore` 里
    /// 「合法性由调用方查、这层只机械写」的既有分工。返回新插入的交棒行
    /// 编号。
    async fn handoff_stage(
        &self,
        project_id: ProjectId,
        from: StageKind,
        to: StageKind,
        risky: bool,
        note: &str,
        at: i64,
    ) -> Result<HandoffId>;

    /// 一个项目的**全部**交棒流水,按时间倒序——「风险与决策」段的数据
    /// 来源(带险的置顶标红,决策栏如实留白)。
    async fn list_handoffs(&self, project_id: ProjectId) -> Result<Vec<HandoffRow>>;

    /// next 切片五D(design §3.2⑤,2026-08-11 主控裁决:INTEGER 对 INTEGER
    /// 直接比较,不经 `CAST`;裁决 8:范围收窄成「当前这一棒」,消失条件
    /// 是「阶段再往前推一棒」;裁决 10:不写死单项目——子查询按
    /// `h.project_id` 自身相关联,不绑定某一个固定项目,`project=None`
    /// 时对全库每个项目各自的「当前一棒」分别判断)。**这不是**
    /// [`HandoffStore::list_handoffs`] 的子集查询——它专门回答「当前这一
    /// 棒身上还压着哪些带险交棒欠账」,更早棒次的带险交棒不在这份结果
    /// 里(它们仍然躺在 `list_handoffs` 的全量流水里永久可查,「更早的欠
    /// 账在风险与决策段永久可查」)。
    async fn list_current_stage_risky_handoffs(
        &self,
        project: Option<ProjectId>,
    ) -> Result<Vec<HandoffRow>>;
}

// next 切片五-1 修复轮:`stage_kind_text`/`parse_stage_kind`(TEXT↔StageKind
// 互转)随 `handoff.from_stage`/`to_stage` 改成 INTEGER 一起删掉——它们唯一
// 的调用点就是这两列,统一走 `StageKind::index()`/`from_index()` 之后这两
// 个函数变成死代码。`CLAUDE.md`「不为向后兼容留旧路径」:没有别的调用点
// 要留,就不留一个没人用的兼容层。
