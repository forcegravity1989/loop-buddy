//! 把 [`crate::metrics_file::MetricsFile`] 的三种形状——`north_star` 单表、
//! `lagging`/`leading` 两张数组表——归一成同一种扁平形状,供上层(`bw-store`
//! 的 `metric` 表)按「项目 + 层级 + 名字」reconcile(next 切片五B,
//! design-s5-hexpanel.md §1.3/§2.2)。
//!
//! **这是本片新写的归一逻辑,不是移植件**——`.bw/metrics.toml` 本身三层
//! 形状不同是文件格式的既有事实(`metrics_file.rs` 的移植内容),但「北极
//! 星也是一条指标,和滞后引领走同一张表」是 v1 后来自己修过的形态
//! (`f0a187a`),旧路径不带进来。
//!
//! **落在 `bw-workspace` 而不是 `bw-store`**:归一只需要
//! [`crate::metrics_file::MetricsFile`] 这一份已经解析好的数据,不碰任何
//! 存储层类型——`bw-store` 因此不需要为了消费这份归一结果而反过来依赖
//! `bw-workspace`(design 的分层图里两者是平级的兄弟 crate,都只被
//! `bw-app` 同时依赖)。真正把 [`FlatMetricDef`] 的字段搬进 `bw-store` 自
//! 己的 `IncomingMetricDef` 并调用同步用例,是调用方(编排层,或本片的
//! `store_guards` 指挥器)的事,这份归一本身保持零下游耦合。

use crate::metrics_file::{MetricDef, MetricsFile};
use crate::metrics_lenient::LenientMetricsFile;

/// 三层——与 `.bw/metrics.toml` 的三个键、`bw-store` `metric.tier` 列的三
/// 个取值一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricTier {
    NorthStar,
    Lagging,
    Leading,
}

impl MetricTier {
    pub fn as_str(self) -> &'static str {
        match self {
            MetricTier::NorthStar => "north_star",
            MetricTier::Lagging => "lagging",
            MetricTier::Leading => "leading",
        }
    }
}

/// 归一后的一条指标定义——不管来自 `[north_star]` 单表还是 `[[lagging]]`/
/// `[[leading]]` 数组表,落到这里都是同一种扁平形状,字段名直接对应
/// `bw-store` `metric` 表的列名(方便调用方逐字段搬)。
#[derive(Debug, Clone)]
pub struct FlatMetricDef {
    pub tier: MetricTier,
    pub name: String,
    pub def: String,
    /// mini-DSL 目标值。北极星本身没有 `target` 字段(它是项目唯一目标,
    /// 不是「朝着某个目标努力的一条指标」)——归一时补空串,和滞后/引领
    /// 的 `target_raw` 走同一列,`bw-store` 侧不需要为北极星单独开一列。
    pub target_raw: String,
    pub collect_kind: &'static str,
    pub collect_query: String,
}

/// 归一入口:一份 `.bw/metrics.toml`(已解析)→ 一串扁平定义,顺序恒为
/// 北极星在前、滞后其次、引领最后(与文件里的段落顺序一致,不是必需的
/// 语义,只是让打印/读回顺序可预期)。
pub fn flatten(file: &MetricsFile) -> Vec<FlatMetricDef> {
    let mut out = Vec::with_capacity(1 + file.lagging.len() + file.leading.len());
    out.push(FlatMetricDef {
        tier: MetricTier::NorthStar,
        name: file.north_star.name.clone(),
        def: file.north_star.def.clone(),
        target_raw: String::new(),
        collect_kind: file.north_star.collect.kind.as_str(),
        collect_query: file.north_star.collect.query.clone(),
    });
    push_tier_array(&mut out, MetricTier::Lagging, &file.lagging);
    push_tier_array(&mut out, MetricTier::Leading, &file.leading);
    out
}

/// 宽松归一入口(next 切片五-1 修复轮,评审 Important-4):同一份归一逻
/// 辑,喂给 [`crate::metrics_lenient::LenientMetricsFile`]——`north_star`
/// 是 `None` 时不产出那一条(`metric` 表因此没有 `tier='north_star'` 那一
/// 行,下游据此显示「尚未定稿」灰卡),滞后/引领两段照常归一,与
/// [`flatten`] 走同一段 [`push_tier_array`] 逻辑,不重复一份。
pub fn flatten_lenient(file: &LenientMetricsFile) -> Vec<FlatMetricDef> {
    let north_star_count = usize::from(file.north_star.is_some());
    let mut out = Vec::with_capacity(north_star_count + file.lagging.len() + file.leading.len());
    if let Some(ns) = &file.north_star {
        out.push(FlatMetricDef {
            tier: MetricTier::NorthStar,
            name: ns.name.clone(),
            def: ns.def.clone(),
            target_raw: String::new(),
            collect_kind: ns.collect.kind.as_str(),
            collect_query: ns.collect.query.clone(),
        });
    }
    push_tier_array(&mut out, MetricTier::Lagging, &file.lagging);
    push_tier_array(&mut out, MetricTier::Leading, &file.leading);
    out
}

/// [`flatten`]/[`flatten_lenient`] 共用:把 `[[lagging]]`/`[[leading]]` 这
/// 两张数组表的条目逐个归一,追加进 `out`。
fn push_tier_array(out: &mut Vec<FlatMetricDef>, tier: MetricTier, defs: &[MetricDef]) {
    for m in defs {
        out.push(FlatMetricDef {
            tier,
            name: m.name.clone(),
            def: m.def.clone(),
            target_raw: m.target.clone(),
            collect_kind: m.collect.kind.as_str(),
            collect_query: m.collect.query.clone(),
        });
    }
}
