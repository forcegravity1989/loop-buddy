//! `.bw/metrics.toml` —— 指标定义的正本。
//!
//! **这是 V4 自己的第 8 个仓文件解析器。** 在此之前 V4 读指标借的是 V3 那份
//! (`bw_engine::metrics_file`),而 `repo/` 下另外 7 份都是自己的 —— 那处不
//! 一致本身就说明「V4 做到指标这里就停了」,和「一条采集都没接」是同一件事的
//! 两面。2026-08-21 断掉对 V3 的依赖时一并补上。
//!
//! **今天只认现行格式(`schema_version = 1`)**,够总览的指标卡显示定义、目标
//! 和「这个数是不是手填的」。新格式(`schema_version = 2`:采集方式收成两种、
//! 新增 `window` 表达可回溯性)的设计在
//! [`design.md`](../../../../docs/v4-prototype/design.md) 第 10 章「指标采集与读数」,
//! **还没落地**;等它落地时改的是这个文件,不是再开第九个解析器。
//!
//! 和 V3 那份的一处实质差别:**不做 `deny_unknown_fields`**。指标文件是人手写
//! 的,写多一个字段就整份解析失败、指标卡全灰,代价远大于收益。多出来的字段
//! 直接忽略。

use super::{parse_toml, read_to_string, Result};
use serde::Deserialize;
use std::path::Path;

pub const REL_PATH: &str = ".bw/metrics.toml";

/// 这个数是怎么来的。今天只用来判断要不要在卡片上打「手填」徽记 ——
/// **真正按它去采集是 14 篇那一刀的事,今天没有任何一处执行它**。
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct CollectPlan {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub query: String,
}

impl CollectPlan {
    /// 手填 = 没有任何采集路径,这个数只能靠人去看一眼再填进来。
    ///
    /// 空的 `kind` 也算手填:没写采集方案的指标,事实上就是没人会自动填它。
    /// 报成「自动采集」比报成「手填」危险得多 —— 前者会让人以为数字在自己更新。
    pub fn is_manual(&self) -> bool {
        let k = self.kind.trim();
        k.is_empty() || k == "manual"
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct MetricDef {
    pub name: String,
    #[serde(default)]
    pub def: String,
    /// 迷你门槛,如 `≥5` `≤24h`。空 = 还没定目标,界面显示「目标未设」。
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub collect: CollectPlan,
}

/// 一份解析出来的指标定义。**北极星可以缺席** —— 刚接入、还没定指标的项目
/// 就是这样,那时候整份文件可能压根不存在(读回 `Ok(None)`)。
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct MetricsFile {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub north_star: Option<MetricDef>,
    #[serde(default)]
    pub lagging: Vec<MetricDef>,
    #[serde(default)]
    pub leading: Vec<MetricDef>,
}

/// 读 `<workspace>/.bw/metrics.toml`。
///
/// `Ok(None)` = 这个项目还没有指标文件(新接入的项目就是这样)。**这不是错误**
/// ——界面据此显示「指标是空的(不是 0)」。解析失败才是 `Err`,那时候整块灰 +
/// 原话,绝不当成「没有指标」。
pub fn read(workspace: &Path) -> Result<Option<MetricsFile>> {
    let Some(raw) = read_to_string(workspace, REL_PATH)? else {
        return Ok(None);
    };
    parse_toml::<MetricsFile>(REL_PATH, &raw).map(Some)
}
