//! `.bw/metrics.toml` —— 指标定义的正本。
//!
//! **只认 `schema_version = 2`。** 旧格式(V3 那套五种采集方式 + 连接器中间层)
//! 读到就整份拒收并如实说「这份是旧格式,运作活①会带你重定一遍」——
//! **不猜、不半读、不迁移**。理由在
//! [`design.md`](../../../../docs/v4-prototype/design.md) §10.8:退场的那三种采集
//! 方式**从来没有实现过**,留着只会让人以为写上就能采;而试点期只有一个真实
//! 项目,重定一次指标比写迁移便宜。
//!
//! 格式的权威说明在同一章 §10.1-§10.4,以及模板 `standard/04-metrics/`
//! 的注释。**这里不复述字段表** —— 同一件事写两遍必然漂移。
//!
//! 和 V3 那份的一处实质差别:**不做 `deny_unknown_fields`**。指标文件是人手写
//! 的,写多一个字段就整份解析失败、指标卡全灰,代价远大于收益。多出来的字段
//! 直接忽略。

use super::RepoFileError;
use super::{parse_toml, read_to_string, Result};
use serde::Deserialize;
use std::path::Path;

pub const REL_PATH: &str = ".bw/metrics.toml";

/// 这一版格式。读到别的值一律拒收。
pub const SCHEMA_VERSION: u32 = 2;

/// 这条指标属于哪一类。**整个采集设计只有这一条分水岭**:今天能采到这个数,
/// 那么「上周三那天的值」现在还能算出来吗?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricClass {
    /// A 类 · 可回溯:能。**一个字都不存**,要看几周就现算几个窗口。
    Retro,
    /// B 类 · 不可回溯:过了那一刻就没了,必须把每次读数追加进读数流水。
    /// **那条落地路径今天还没接**,界面上如实说。
    PointInTime,
    /// C 类 · 手填:压根没有采集路径。
    Manual,
}

/// 这个数怎么来的。**只有脚本和手填两种,没有第三种。**
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct CollectPlan {
    #[serde(default)]
    pub kind: String,
    /// argv 数组:第一项是可执行文件,其余是参数。**不走 shell** —— 管道、
    /// 重定向、`&&` 要用就写进脚本里面。
    ///
    /// 写成数组而不是一行命令,是为了在 Windows 上也能明确指定解释器:
    /// `["python", …]` 和 `["python3", …]` 是两回事,让人自己写清楚,
    /// buddy 不去猜。
    #[serde(default)]
    pub run: Vec<String>,
    /// 历史还能不能重新算出来。见 [`MetricClass`]。
    #[serde(default)]
    pub retro: bool,
}

impl CollectPlan {
    pub fn class(&self) -> MetricClass {
        match self.kind.trim() {
            "script" if self.retro => MetricClass::Retro,
            "script" => MetricClass::PointInTime,
            // 空的 `kind` 也算手填:没写采集方案的指标,事实上就是没人会自动
            // 填它。报成「自动采集」比报成「手填」危险得多 —— 前者会让人以为
            // 数字在自己更新。
            _ => MetricClass::Manual,
        }
    }

    pub fn is_manual(&self) -> bool {
        self.class() == MetricClass::Manual
    }

    /// 这条指标能不能真的被起起来采一次。手填不行;脚本但 `run` 是空的也不行
    /// ——**那是一条写了一半的定义,不是一条能采的指标**。
    pub fn runnable(&self) -> bool {
        !self.is_manual() && !self.run.is_empty()
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

impl MetricsFile {
    /// 三层按顺序摊平:北极星、滞后、引领。给采集和界面共用一个遍历顺序。
    pub fn all(&self) -> Vec<&MetricDef> {
        self.north_star
            .iter()
            .chain(self.lagging.iter())
            .chain(self.leading.iter())
            .collect()
    }
}

/// 读 `<workspace>/.bw/metrics.toml`。
///
/// `Ok(None)` = 这个项目还没有指标文件(新接入的项目就是这样)。**这不是错误**
/// ——界面据此显示「还没定出来」。解析失败与旧格式才是 `Err`,那时候整块灰 +
/// 原话,绝不当成「没有指标」。
pub fn read(workspace: &Path) -> Result<Option<MetricsFile>> {
    let Some(raw) = read_to_string(workspace, REL_PATH)? else {
        return Ok(None);
    };
    let file = parse_toml::<MetricsFile>(REL_PATH, &raw)?;
    if file.schema_version != SCHEMA_VERSION {
        return Err(RepoFileError::Shape {
            path: REL_PATH.into(),
            why: format!(
                "这份指标文件是旧格式(schema_version = {},当前是 {})。\
                 旧格式里那几种采集方式从来没有实现过,不做迁移 —— \
                 运作活①「更新指标 + 制定本周计划」会带你重定一遍。",
                file.schema_version, SCHEMA_VERSION
            ),
        });
    }
    Ok(Some(file))
}
