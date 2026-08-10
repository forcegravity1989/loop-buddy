//! 指标聚合的命令用例(design-s5-hexpanel.md §4.2 `cmd/metric.rs`):同
//! 步指标正本、手填观测、停用指标。`CollectMetrics`(设计稿草案里同属
//! 指标聚合,跑采集连接器)不在本片范围——§3.3/§8「连接器探活自动化本
//! 片不做」的同一条裁剪也适用于「自动跑采集」:本片只做「人显式发起,
//! 一次性」的两条写(正本同步、手填观测),不做后台/自动触发的采集。

use bw_core::{MetricId, ObservationId, ProjectId};
use bw_store::{
    IncomingMetricDef, MetricStore, MetricSyncReport, MetricTier as StoreMetricTier,
    NewObservation, ObservationStore, SqliteStore,
};
use bw_workspace::metric_shape::{self, MetricTier as WorkspaceMetricTier};

use crate::AppError;

fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// `bw_workspace::metric_shape` 归一出来的层级 → `bw_store` 自己的层级
/// 枚举——两个类型分住两个 crate(`bw-store` 不依赖 `bw-workspace`,见
/// `bw_store::IncomingMetricDef` 文档),这里是它们唯一交汇的转换点。与
/// `bw-store` `examples/store_guards.rs` `to_incoming` 同一份映射,逐字段
/// 对应,不是重新发明。
fn to_incoming(defs: Vec<metric_shape::FlatMetricDef>) -> Vec<IncomingMetricDef> {
    defs.into_iter()
        .map(|d| IncomingMetricDef {
            tier: match d.tier {
                WorkspaceMetricTier::NorthStar => StoreMetricTier::NorthStar,
                WorkspaceMetricTier::Lagging => StoreMetricTier::Lagging,
                WorkspaceMetricTier::Leading => StoreMetricTier::Leading,
            },
            name: d.name,
            def: d.def,
            target_raw: d.target_raw,
            collect_kind: d.collect_kind.to_string(),
            collect_query: d.collect_query,
        })
        .collect()
}

/// 同步项目仓正本(`<workspace>/.bw/metrics.toml`)进 `metric` 表(design
/// §1.3「北极星也是一条指标」/ §2.2「同步语义」)。
///
/// **解析失败不写库**(五-1 concern 3 在编排层这一侧的落点):
/// [`bw_workspace::metrics_lenient::read_lenient`] 的 `Err` 分支——文件存
/// 在但语法错/字段缺——在这里直接 `?` 冒泡成 [`AppError`],函数在那一步
/// 就返回,**根本不会走到** [`bw_store::MetricStore::sync_metrics_from_file`]
/// 那一行;不是「写了一半再回滚」,是「压根没有发生第二步」。
///
/// **文件缺席不是错误**(design §1.2①「没数据时」/ `read_lenient` 文
/// 档):`Ok(None)`(工作区没配置,或者项目仓压根没有这份文件)与
/// `Ok(Some(file))` 但 `north_star` 缺节,都是「没数据」这同一件事的两
/// 种程度,不是解析失败——前者归一成空定义集合喂给同步器(正本此刻当作
/// 「什么都没定义」,已有的 `origin='file'` 行照「正本删了它」的既有语
/// 义自动停用,不物删),后者归一时北极星那一条不产出(`metric_shape::
/// flatten_lenient`),下游据此显示「尚未定稿」灰卡。
pub async fn sync_metrics_file(
    store: &SqliteStore,
    project_id: ProjectId,
    workspace: &str,
) -> Result<MetricSyncReport, AppError> {
    let file = bw_workspace::metrics_lenient::read_lenient(workspace)
        .map_err(|e| AppError::MetricsFile(e.to_string()))?;
    let incoming = match file {
        Some(f) => to_incoming(metric_shape::flatten_lenient(&f)),
        None => Vec::new(),
    };
    let report = store
        .sync_metrics_from_file(project_id, incoming, now_unix())
        .await?;
    Ok(report)
}

/// 手填一条观测(design §2.3「手填的值戴徽记」,`Command::
/// RecordManualObservation` 的落地——设计稿草案的字段只有 `{metric,
/// raw}`,`project_id` 由这条指标反查而来,不接受调用方另指定一个可能
/// 对不上的值,同 `bw-app::run::StartRun` 「项目由 issue 反查」的既有先
/// 例)。`source` 恒 `"manual"`——这是这条命令存在的唯一理由,真要记一条
/// 别的来源的观测,走别的写入路径(脚本采集器等),不经过这个函数。
pub async fn record_manual_observation(
    store: &SqliteStore,
    metric: MetricId,
    raw: String,
) -> Result<ObservationId, AppError> {
    let Some(metric_row) = store.get_metric(metric).await? else {
        return Err(AppError::MetricNotFound(metric));
    };
    let id = ObservationId::new();
    store
        .insert_observation(NewObservation {
            id,
            metric_id: metric,
            project_id: metric_row.project_id,
            ts: now_unix(),
            raw_value: raw,
            source: "manual".to_string(),
            source_hint: "人手填".to_string(),
        })
        .await?;
    Ok(id)
}

/// 停用一条指标(`Command::ArchiveMetric`)——整条退出派生链,历史观测
/// 原样留着,永不物理删除(design §2.2)。
pub async fn archive_metric(store: &SqliteStore, metric: MetricId) -> Result<bool, AppError> {
    let archived = store.archive_metric(metric, now_unix()).await?;
    Ok(archived)
}
