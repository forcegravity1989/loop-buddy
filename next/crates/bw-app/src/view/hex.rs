//! 六段总控屏的推导(design-s5-hexpanel.md §1.2/§1.3/§2.6):**纯函数**
//! ——见 [`super`] 模块文档「不读时钟、不查库、不发命令」这条约束。
//!
//! 逐段数据来源照 §1.2 那张表接线;没有数据的段如实空(「北极星尚未定
//! 稿」灰卡 / 「尚未开棒」/ 决策栏留白),绝不假装绿、绝不拿别的东西冒
//! 充。信号(①③段)经内核派生链[`bw_core::derive::evaluate_metric`]读时
//! 现算——**这个模块里没有任何一处把算出来的 `Signal` 写回任何持久结
//! 构**,`HexView` 本身只是一次调用返回、用完即弃的数据,不是缓存。

use std::collections::{HashMap, HashSet};

use bw_core::derive::{
    evaluate_metric, measure, parse_magnitude, parse_target, Measurement, Target,
};
use bw_core::{Cadence, IssueId, IssueStatus, MetricId, ProjectId, Signal, SourceKind, StageKind};
use bw_store::{HandoffRow, IssueRow, MetricRow, MetricTier, ObservationRow, RunRow};
use bw_workspace::evidence::WorkspaceEvidence;
use time::OffsetDateTime;

/// 六段视图②③段(以及待人处理④b,`super::attention` 复用同一个常量,不
/// 重新定义第二份默认值)统一使用的默认节奏窗口——喂给
/// [`bw_core::derive::measure`] 判断一条观测是否「过期」(§2.6「信号只能
/// 推导」:过期的数据源把一个本该 Green 的信号压到 Amber,「新鲜的绿不
/// 能掩盖一个失联的数据源」)。
///
/// **如实标注的简化,不是钉死的产品口径**:`metric` 表(design §2.2)今
/// 天没有「这条指标多久刷新一次」的列——`.bw/metrics.toml` 的格式
/// (`docs/metrics-toml-format.md`)也没有约定这个字段。在有这一列之前,
/// 全体指标共用同一个保守默认(`Weekly`,与滞后/引领指标最常见的复盘节
/// 奏对齐);登记为已知缺口(plan/23 §10),不是本片私自拍板的产品决定
/// ——真要按指标定制节奏,需要先给 `.bw/metrics.toml` 加字段、`metric`
/// 表跟着加列,那是后续切片的事。
pub const DEFAULT_METRIC_CADENCE: Cadence = Cadence::Weekly;

/// `observation.source` 是自由文本(schema 注释:「manual/script/github/
/// codehub/connector/…」),不是一个受 CHECK 约束的枚举列——新连接器要能
/// 加一种来源不必等 schema 迁移。这里把已知的几种文本映到
/// `bw_core::SourceKind`(供 [`bw_core::derive::measure`] 判断「手填」徽
/// 记与来源展示);未知文本**不默认成 `Manual`**——那会让一个本来不是手
/// 填的来源被误标成手填,如实映到 `SourceKind::Connector`(最保守的「反
/// 正不是人手填的」归类,不点亮手填徽记,也不假装认得这个来源)。
pub(super) fn parse_source_kind(raw: &str) -> SourceKind {
    match raw {
        "manual" => SourceKind::Manual,
        "script" => SourceKind::Script,
        "github" => SourceKind::Github,
        "codehub" => SourceKind::Codehub,
        "connector" => SourceKind::Connector,
        "gateway_log" => SourceKind::GatewayLog,
        "ci" => SourceKind::Ci,
        "git_pr" => SourceKind::GitPr,
        "telemetry" => SourceKind::Telemetry,
        _ => SourceKind::Connector,
    }
}

fn unix_to_odt(ts: i64) -> OffsetDateTime {
    OffsetDateTime::from_unix_timestamp(ts).unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

/// 一条指标(北极星 / 滞后 / 引领,三层同构)算出来的可渲染卡片——**这不
/// 是一张缓存表**,是 [`build`] 每次调用现算一遍、用完即弃的值。
#[derive(Clone, Debug, PartialEq)]
pub struct MetricCard {
    pub id: MetricId,
    pub tier: MetricTier,
    pub name: String,
    pub def: String,
    pub target_raw: String,
    pub collect_kind: String,
    pub origin: String,
    pub has_observation: bool,
    pub latest_raw_value: Option<String>,
    pub latest_source: Option<String>,
    pub latest_source_hint: Option<String>,
    pub latest_is_manual: bool,
    pub latest_observed_at: Option<i64>,
    /// §2.6:读时现算,`bw_core::derive::evaluate_metric` 是唯一出生
    /// 地——这里只是把算出来的值存进这次调用要返回的那份 `MetricCard`
    /// 里,函数返回之后这份 `HexView` 整个被丢弃,不是任何持久结构。
    pub signal: Signal,
}

/// 段①:北极星。没有这一节 → 灰卡「北极星尚未定稿」,绝不显绿
/// (design §1.2①)。
#[derive(Clone, Debug, PartialEq)]
pub enum NorthStarView {
    // clippy::large_enum_variant:`MetricCard` 比 `Undefined`(零字节)大
    // 得多,`Box` 一下——这个视图每次调用只构造一份,装箱的堆分配成本可
    // 以忽略,换来枚举本身不必按最大变体的尺寸预留栈空间。
    Defined(Box<MetricCard>),
    /// 「北极星尚未定稿」灰卡 + 「怎么定:在项目仓写 .bw/metrics.toml」提示。
    Undefined,
}

/// 段②:五角色责任卡。前七项(角色/方法论/一字诀/核心问题/方法环节/完
/// 成清单/常见的坑/节奏)零数据库,全部来自 `bw_core::StageKind` 静态元
/// 数据(design §1.2②)——这里只挂当前一棒的高亮与每阶段的活数。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StageCard {
    pub stage: StageKind,
    pub is_current: bool,
    pub issue_count: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FiveRolesSegment {
    /// `None` = 尚未开棒——五张卡照常显(方法论本来就与项目无关),只是
    /// 没有一张高亮(design §1.2②「没数据时」)。
    pub active_stage: Option<StageKind>,
    pub cards: [StageCard; 5],
    /// `issue.stage IS NULL` 的活数——未归类,不计入任何 `StageCard`。
    pub unclassified_issue_count: i64,
}

/// 段③:引领指标(北极星→滞后→引领,顺序固定;design §1.2③)。**这里
/// 再列一次北极星**——段①是它的专属headline(定义+采集方案+最近值的
/// 散文式呈现),段③是它作为「三层里的第一层」出现在指标墙上,两处内容
/// 有重叠是设计原文如此(§1.2③表格原文「三层,顺序固定:北极星→滞后→
/// 引领」),不是本实现的重复。
#[derive(Clone, Debug, PartialEq)]
pub struct MetricsSegment {
    pub cards: Vec<MetricCard>,
}

/// 段④:当前 Loop。四个聚合数从 [`bw_app::run::RunSnapshot`](crate::run::RunSnapshot)
/// 传入(那条查询/内存表的真实数据源见该类型文档);`exceptions` 是「只
/// 列例外」的那部分明细(失败/遗留、且没有更晚运行接手——与待人处理③
/// 同一条判据,design §1.2④「正常在跑的折成一个数」)。
#[derive(Clone, Debug, PartialEq)]
pub struct LoopSegment {
    pub running: usize,
    pub in_review: usize,
    pub recent_failed: usize,
    pub unsettled: usize,
    pub exceptions: Vec<RunRow>,
    /// 一次运行都没有 → 如实加一句「定时机制…在新工程里还没建」
    /// (design §1.2④「没数据时」)。
    pub has_any_run: bool,
    /// next 切片七-2 修(design-s7-real-project.md §1「壳的活卡加『开
    /// 工』『取消』」,`task-s7b-review.md` Important-3 收口):能点『开
    /// 工』的活——判据原样照抄 `run::RunManager::start` 的既有判据
    /// (design §3.4①「待办池/待办/进行中都能开工,『进行中』是诚实失
    /// 败后的重试路径」),现在**只有这一处**在算,壳(`app-desktop::
    /// kernel::build_vm`)不再自己维护一份平行判断——这是「壳内零业务
    /// 判断」这条边界唯一破口的收口。
    ///
    /// **修复的死角**:此前壳里的版本只列「待办池/待办」两档,注释写
    /// 「『进行中』已经在 `open_runs` 里,不重复列」——这条理由不成立:
    /// `open_runs` 是「当前有一条活着的运行」,而「进行中」的活在一次运
    /// 行正常结束/失败/被收拾之后**依旧是「进行中」**(§1.5 第三行「结
    /// 束不等于干成」的既定语义),却没有落进 `open_runs`。漏了这一档
    /// 的后果是:活跑完一次之后,界面上既没有「开工」也没有「取消」按
    /// 钮,成了死局(评审的深链实测复现过这个状态)。这里的判据因此是
    /// 「待办池/待办 全部 ∪ 进行中里没有一条活着的运行(`ended_at` 为
    /// 空)的那些」——用 `all_runs` 现算,同 `open_runs` 展示层筛选用的
    /// 同一批数据,不新增查询。
    pub startable_issues: Vec<IssueRow>,
}

/// 段⑤:风险与决策。交棒记录流水,带险的置顶标红;决策栏本片不建,如实
/// 留白(design §1.2⑤)。
#[derive(Clone, Debug, PartialEq)]
pub struct RiskDecisionSegment {
    pub handoffs: Vec<HandoffRow>,
    /// 一次交棒都没有 → 如实说明。
    pub has_any_handoff: bool,
}

impl RiskDecisionSegment {
    /// 决策栏的留白说明——正本住哪没定(design §1.2 两段留白说明 / 附
    /// 主控裁决 #1/#2),不拿交棒记录冒充决策记录。
    pub const DECISION_NOTE: &'static str =
        "决策记录本片不建——正本住哪(项目仓 ADR 目录?BW 本地表?)还没定,如实留白,不拿交棒记录冒充决策记录。";
}

/// 段⑥:交付证据。三栏——①运行账(运行表)②观测出处(观测表)③工作区
/// 现状(**现采不落库**,design §1.2⑥/§2.6)。
#[derive(Clone, Debug, PartialEq)]
pub struct EvidenceSegment {
    pub runs: Vec<RunRow>,
    pub observations: Vec<ObservationRow>,
    /// `None` = 工作区未配置(design §1.2⑥「没数据时」)。
    pub workspace_evidence: Option<WorkspaceEvidence>,
}

/// 六段总控屏——一次调用的产物,用完即弃,不是任何持久缓存。
#[derive(Clone, Debug, PartialEq)]
pub struct HexView {
    pub project_id: ProjectId,
    pub project_name: String,
    pub north_star: NorthStarView,
    pub five_roles: FiveRolesSegment,
    pub metrics: MetricsSegment,
    pub loop_segment: LoopSegment,
    pub risk_decision: RiskDecisionSegment,
    pub evidence: EvidenceSegment,
}

/// [`build`] 的运行段(④)输入——四个聚合数 + 例外明细,与
/// `crate::run::RunSnapshot` 字段一一对应(调用方从 `RunManager::
/// snapshot` 真实查回,`build` 本身不查库、不碰 `RunManager`)。
pub struct LoopInputs {
    pub running: usize,
    pub in_review: usize,
    pub recent_failed: usize,
    pub unsettled: usize,
    pub exceptions: Vec<RunRow>,
    pub has_any_run: bool,
}

/// 组装六段总控视图——**纯函数**:所有入参要么是已经从存储读出来的行,
/// 要么是调用方现采/现查的外部证据(工作区证据、Loop 段的聚合数),要么
/// 是传进来的时刻;函数体内不发生任何 IO/查库/发命令(design §4.3)。
#[allow(clippy::too_many_arguments)]
pub fn build(
    project_id: ProjectId,
    project_name: &str,
    active_stage: Option<StageKind>,
    issue_stage_counts: &[(Option<StageKind>, i64)],
    metrics: &[MetricRow],
    observations_by_metric: &HashMap<MetricId, Vec<ObservationRow>>,
    loop_inputs: LoopInputs,
    handoffs: &[HandoffRow],
    all_runs: &[RunRow],
    // next 切片七-2 修(Important-3):调用方已经按 `RunManager::start`
    // 允许开工的三档状态(Backlog/Todo/InProgress)查好的候选活——这个
    // 纯函数只负责把「进行中且当前有一条活着的运行」的那部分从候选里
    // 减掉,不重新判断「哪些状态允许开工」这条业务规则本身(那条规则
    // 的唯一真身仍是 `run::RunManager::start`,这里只是原样复述判据)。
    startable_candidates: &[IssueRow],
    workspace_evidence: Option<WorkspaceEvidence>,
    now: OffsetDateTime,
) -> HexView {
    let mut cards: Vec<MetricCard> = metrics
        .iter()
        .filter(|m| m.archived_at.is_none())
        .map(|m| build_metric_card(m, observations_by_metric.get(&m.id), now))
        .collect();
    // §1.2③「顺序固定:北极星→滞后→引领」——`MetricStore::list_metrics`
    // 的 SQL 按 `tier` 文本字母序排(design 未钉死这一点),字母序不是三
    // 层顺序('lagging' < 'leading' < 'north_star');排序放在这个纯函数
    // 里做,不依赖调用方传入的行序,也不去改 SQL 的 `ORDER BY`(那条
    // `ORDER BY` 服务的是别的读回场景,不该为这一处视图专门收窄)。
    cards.sort_by(|a, b| {
        tier_rank(a.tier)
            .cmp(&tier_rank(b.tier))
            .then(a.name.cmp(&b.name))
    });

    let north_star = cards
        .iter()
        .find(|c| c.tier == MetricTier::NorthStar)
        .cloned()
        .map(|c| NorthStarView::Defined(Box::new(c)))
        .unwrap_or(NorthStarView::Undefined);

    // `StageKind::ALL` 本身就是五阶段固定顺序的 `[StageKind; 5]`——数组
    // `.map()` 保长度,`cards_arr` 因此天然恰好五张卡,不多不少,不需要
    // 经过 `Vec` 再收窄。
    let cards_arr: [StageCard; 5] = StageKind::ALL.map(|stage| StageCard {
        stage,
        is_current: active_stage == Some(stage),
        issue_count: issue_stage_counts
            .iter()
            .find(|(s, _)| *s == Some(stage))
            .map(|(_, n)| *n)
            .unwrap_or(0),
    });
    let unclassified_issue_count = issue_stage_counts
        .iter()
        .find(|(s, _)| s.is_none())
        .map(|(_, n)| *n)
        .unwrap_or(0);

    // 能点『开工』的活(LoopSegment::startable_issues 字段文档细说判
    // 据)——「进行中」候选里,当前有一条活着的运行(`ended_at` 为空)
    // 的要被减掉,同 `open_runs`(壳的展示层筛选)用的同一批 `all_runs`
    // 数据,不新增查询。
    let live_issue_ids: HashSet<IssueId> = all_runs
        .iter()
        .filter(|r| r.ended_at.is_none())
        .map(|r| r.issue_id)
        .collect();
    let startable_issues: Vec<IssueRow> = startable_candidates
        .iter()
        .filter(|i| i.status != IssueStatus::InProgress || !live_issue_ids.contains(&i.id))
        .cloned()
        .collect();

    HexView {
        project_id,
        project_name: project_name.to_string(),
        north_star,
        five_roles: FiveRolesSegment {
            active_stage,
            cards: cards_arr,
            unclassified_issue_count,
        },
        metrics: MetricsSegment { cards },
        loop_segment: LoopSegment {
            running: loop_inputs.running,
            in_review: loop_inputs.in_review,
            recent_failed: loop_inputs.recent_failed,
            unsettled: loop_inputs.unsettled,
            exceptions: loop_inputs.exceptions,
            has_any_run: loop_inputs.has_any_run,
            startable_issues,
        },
        risk_decision: RiskDecisionSegment {
            handoffs: handoffs.to_vec(),
            has_any_handoff: !handoffs.is_empty(),
        },
        evidence: EvidenceSegment {
            runs: all_runs.to_vec(),
            observations: sorted_observations(observations_by_metric),
            workspace_evidence,
        },
    }
}

/// 交付证据段②「每个数字的原始出处」——**不能**直接
/// `observations_by_metric.values().flatten()`:`HashMap::values()` 的遍历
/// 顺序不保证跨实例稳定(同一批数据、两次各自新建一个 `HashMap`,顺序可
/// 能不同),会让「杀进程重开,数字一致」(§7.2 第 4 节的新验收形态)偶发
/// 假红——`hex_readback` 指挥器真的抓到过这个偶发失败(重开前后 `Vec`
/// 里两条观测的顺序对调),不是猜测的风险。按 `ts` 升序(时间线读起来顺)
/// 排定,`id` 兜底(理论上 `ts` 也可能撞,`id` 保证任何输入下都有一个确
/// 定的全序),让输出与调用者传进来的 `HashMap` 内部实现细节彻底无关。
fn sorted_observations(by_metric: &HashMap<MetricId, Vec<ObservationRow>>) -> Vec<ObservationRow> {
    let mut all: Vec<ObservationRow> = by_metric.values().flatten().cloned().collect();
    all.sort_by(|a, b| a.ts.cmp(&b.ts).then_with(|| a.id.uuid().cmp(&b.id.uuid())));
    all
}

fn tier_rank(t: MetricTier) -> u8 {
    match t {
        MetricTier::NorthStar => 0,
        MetricTier::Lagging => 1,
        MetricTier::Leading => 2,
    }
}

/// 一条指标(+它已经按 `ts DESC` 排好的观测历史,可能没有)→ 一张
/// `MetricCard`。信号经 [`evaluate_metric`] 读时现算(§2.6)——`target_raw`
/// 为空或解析失败时按「跟踪 · 不评判」处理(`bw_core::derive::Target::
/// TrackOnly`,恒 `Unknown`,不是「解析失败就报错崩掉」),因为「找指标
/// v1 可能先起草一个还没定目标的指标」是既有的正常状态
/// (`bw-workspace::metrics_file::MetricDef.target` 本来就是可选字段)。
fn build_metric_card(
    metric: &MetricRow,
    observations: Option<&Vec<ObservationRow>>,
    now: OffsetDateTime,
) -> MetricCard {
    let empty: Vec<ObservationRow> = Vec::new();
    let observations = observations.unwrap_or(&empty);
    let latest = observations.first(); // 已按 ts DESC 排(`ObservationStore::list_observations`)

    let target = parse_target(&metric.target_raw).unwrap_or(Target::TrackOnly);
    let measurement = match latest {
        Some(o) => measure(
            &o.raw_value,
            unix_to_odt(o.ts),
            parse_source_kind(&o.source),
            &DEFAULT_METRIC_CADENCE,
            now,
        ),
        None => Measurement::Missing,
    };
    // 趋势:按时间正序(观测表本身是倒序)取全部可解析的数值点,喂给
    // `Target::DirectionUp` 判「比上一个点高不高」——本片没有任何指标真
    // 的配「↑」目标,这条只是不让派生链在真的撞上「↑」目标时无从判断。
    let trend: Vec<f64> = observations
        .iter()
        .rev()
        .filter_map(|o| parse_magnitude(&o.raw_value))
        .collect();
    let eval = evaluate_metric(&measurement, &target, &trend);

    MetricCard {
        id: metric.id,
        tier: metric.tier,
        name: metric.name.clone(),
        def: metric.def.clone(),
        target_raw: metric.target_raw.clone(),
        collect_kind: metric.collect_kind.clone(),
        origin: metric.origin.clone(),
        has_observation: latest.is_some(),
        latest_raw_value: latest.map(|o| o.raw_value.clone()),
        latest_source: latest.map(|o| o.source.clone()),
        latest_source_hint: latest.map(|o| o.source_hint.clone()),
        latest_is_manual: latest
            .map(|o| parse_source_kind(&o.source).is_manual())
            .unwrap_or(false),
        latest_observed_at: latest.map(|o| o.ts),
        signal: eval.signal(),
    }
}
