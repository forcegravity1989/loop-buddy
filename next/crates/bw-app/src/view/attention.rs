//! 待人处理的推导投影(design-s5-hexpanel.md §3):**纯函数,零自有表**
//! ——见 [`super`] 模块文档「不读时钟、不查库、不发命令」这条约束。这里
//! 只把已经从存储读出来的行组织成「需要你出手的事」这份清单,清单本身
//! 不落库,任何一行的消失都是底层状态变化的直接结果(§3.1「每项都必须
//! 有自然消失条件」)。
//!
//! **五项,每项的自然消失条件**(实现即断言的落点在 `hex_readback.rs`
//! 「⑦ 待人处理:五项正反各验一次」):
//!
//! 1. 遗留运行:结算一次 → 退出 [`bw_store::RunStore::list_unsettled_runs`]
//!    的结果集。
//! 2. 评审中等你点:人点「完成」(或退回「进行中」)→ 退出
//!    [`bw_store::IssueStore::list_issues_by_status`]`(_, InReview)` 的结果集。
//! 3. 停在原地的运行:对同一件活重开一次(哪怕又失败)→ 有更晚的运行,
//!    旧行退出 [`bw_store::RunStore::list_stalled_runs`] 的结果集。
//! 4. 指标没数/数据过期:采到或填到一条新观测 → 退出
//!    [`bw_store::MetricStore::list_metrics_without_observation`]
//!    或本模块 [`build`] 算出来的「过期」子集。
//! 5. 当前这一棒的带险交棒欠账:阶段再往前推一棒 → 当前这一棒变了,旧
//!    行不再匹配 → 退出
//!    [`bw_store::HandoffStore::list_current_stage_risky_handoffs`] 的结果
//!    集(**该行本身在 `handoff` 表里永久还在**——「更早的欠账在风险与决
//!    策段永久可查」,裁决 8 的防误读文案)。

use bw_core::derive::cadence_window;
use bw_core::{MetricId, ProjectId};
use bw_store::{HandoffRow, IssueRow, MetricLatestObservation, MetricRow, RunRow};
use time::{Duration, OffsetDateTime};

// `DEFAULT_METRIC_CADENCE` 定义在 `super::hex`(六段视图②③段的信号推导先
// 用到它——§2.6「信号只能推导」这条铁律的落点在那一侧),这里复用同一个
// 常量,不重新定义第二份默认值。
use super::hex::DEFAULT_METRIC_CADENCE;

/// 「指标没数 / 数据过期」④b 那一半——未停用、有过观测,但最新一条已经
/// 超过节奏窗口的指标(design §3.2④b「过不过期不写进 SQL」,阈值来自
/// [`bw_core::derive::cadence_window`],不是本模块自己定义的第二份阈值)。
#[derive(Clone, Debug, PartialEq)]
pub struct StaleMetric {
    pub id: MetricId,
    pub tier: bw_store::MetricTier,
    pub name: String,
    pub latest_ts: i64,
}

/// 一项「当前一棒的带险交棒欠账」——直接复用 [`HandoffRow`],不新开一个
/// 只多一个字段的包装类型(裁决 8「文案加一句防误读提示」由调用方在渲染
/// 时附加,不需要改变这条数据本身的形状)。
pub type CurrentStageRiskyHandoff = HandoffRow;

/// 待人处理清单——一次调用的产物,零自有表(§3「投影零自有表」)。
#[derive(Clone, Debug, PartialEq)]
pub struct AttentionView {
    pub project: Option<ProjectId>,
    /// ① 遗留运行:关门没结账。
    pub unsettled_runs: Vec<RunRow>,
    /// ② 评审中等你点。
    pub in_review_issues: Vec<IssueRow>,
    /// ③ 停在原地的运行:失败/遗留,且没有更晚运行接手。
    pub stalled_runs: Vec<RunRow>,
    /// ④a 从来没有观测过的指标。
    pub metrics_without_observation: Vec<MetricRow>,
    /// ④b 有过观测、但最新一条已经过期的指标。
    pub metrics_stale: Vec<StaleMetric>,
    /// ⑤ 当前这一棒的带险交棒欠账(裁决 8:收窄成「当前这一棒」,消失
    /// 条件是「阶段再往前推一棒」;更早的欠账在 [`bw_store::HandoffStore::list_handoffs`]
    /// 的全量流水里永久可查)。
    pub current_stage_risky_handoffs: Vec<CurrentStageRiskyHandoff>,
}

impl AttentionView {
    /// 五项加总——「全部安静时,清单长度 = 0」(design §7.1「不是 0 条告
    /// 警那种带框的空壳,是真的空」)对应的就是这个数。
    pub fn total_count(&self) -> usize {
        self.unsettled_runs.len()
            + self.in_review_issues.len()
            + self.stalled_runs.len()
            + self.metrics_without_observation.len()
            + self.metrics_stale.len()
            + self.current_stage_risky_handoffs.len()
    }

    pub fn is_quiet(&self) -> bool {
        self.total_count() == 0
    }
}

/// 组装待人处理清单——**纯函数**:全部入参是已经从存储读出来的行 + 一
/// 个传进来的时刻,函数体内不发生任何 IO/查库/发命令(design §4.3)。
/// 五个「造」查询各自的入参已经在调用方(`App::attention_view`)按
/// `project: Option<ProjectId>` 参数化好(裁决 10:不写死单项目);这里只
/// 做④b「最新时刻 → 过没过期」这一步需要时钟的判断,以及把
/// [`MetricLatestObservation`] 里没有观测(`latest_ts=None`)的那些行滤
/// 掉——那些行已经在 `metrics_without_observation`(④a)里出现过,不重
/// 复计进④b,避免同一条指标在清单里出现两次。
#[allow(clippy::too_many_arguments)]
pub fn build(
    project: Option<ProjectId>,
    unsettled_runs: Vec<RunRow>,
    in_review_issues: Vec<IssueRow>,
    stalled_runs: Vec<RunRow>,
    metrics_without_observation: Vec<MetricRow>,
    metric_latest_observations: Vec<MetricLatestObservation>,
    current_stage_risky_handoffs: Vec<CurrentStageRiskyHandoff>,
    now: OffsetDateTime,
) -> AttentionView {
    let window: Duration = cadence_window(&DEFAULT_METRIC_CADENCE);
    let metrics_stale: Vec<StaleMetric> = metric_latest_observations
        .into_iter()
        .filter_map(|m| {
            let ts = m.latest_ts?; // None → 从没采过,归④a,不进④b
            let as_of =
                OffsetDateTime::from_unix_timestamp(ts).unwrap_or(OffsetDateTime::UNIX_EPOCH);
            if now - as_of > window {
                Some(StaleMetric {
                    id: m.id,
                    tier: m.tier,
                    name: m.name,
                    latest_ts: ts,
                })
            } else {
                None
            }
        })
        .collect();

    AttentionView {
        project,
        unsettled_runs,
        in_review_issues,
        stalled_runs,
        metrics_without_observation,
        metrics_stale,
        current_stage_risky_handoffs,
    }
}

/// 裁决 8 的防误读文案——第⑤项渲染时附加在旁边,提醒「这不是全部带险交
/// 棒历史」。放在这里(而不是散在调用方各处硬编码字符串)是为了让文案
/// 只有一处定义。
pub const CURRENT_STAGE_RISKY_HANDOFF_CAVEAT: &str =
    "只显示当前这一棒的带险交棒欠账;更早的欠账在风险与决策段永久可查,不代表已经不算数。";
