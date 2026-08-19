//! 健康三判据的现算:从仓文件与 git 现场取输入,交给密封的推导函数。
//!
//! 这一层只负责**收集输入**——判什么颜色是 [`crate::derive`] 的事,收集与判断
//! 分开,才能保证「灯只可能是算出来的」。

use super::{App, Result};
use crate::derive::{derive_project_health, DerivedHealth, HealthInputs};
use crate::isoweek;
use crate::model::ProjectId;
use crate::repo::week_plan_file;
use std::path::Path;

/// 从一个仓现场取三条判据的输入。git 读不到(不是仓、没提交)一律当「没有」,
/// 不报错——没数据本来就是合法状态,结果是灰灯而不是崩溃。
pub async fn collect_health_inputs(workspace: &Path, week: &str) -> HealthInputs {
    let last_week = isoweek::previous_week(week).unwrap_or_default();

    let this_plan = week_plan_file::read(workspace, week).ok().flatten();
    let last_plan = if last_week.is_empty() {
        None
    } else {
        week_plan_file::read(workspace, &last_week).ok().flatten()
    };

    let committed_this_week = crate::git::has_commits_in_week(workspace, week)
        .await
        .unwrap_or(false);
    let committed_last_week = if last_week.is_empty() {
        false
    } else {
        crate::git::has_commits_in_week(workspace, &last_week)
            .await
            .unwrap_or(false)
    };
    let merged_last_week = if last_week.is_empty() {
        false
    } else {
        crate::git::has_merges_in_week(workspace, &last_week)
            .await
            .unwrap_or(false)
    };
    // 上周发过版也算交付。发版记录的正本是仓文件,不查库。
    let released = crate::repo::release_file::read(workspace)
        .ok()
        .flatten()
        .is_some_and(|rows| {
            rows.iter()
                .any(|r| week_of_date(&r.released_at).as_deref() == Some(last_week.as_str()))
        });

    HealthInputs {
        has_week_goal: this_plan.as_ref().is_some_and(|p| p.has_goal()),
        committed_this_week,
        committed_last_week,
        has_metric_reading: this_plan.as_ref().is_some_and(|p| p.has_reading())
            || last_plan.as_ref().is_some_and(|p| p.has_reading()),
        // 读数是否越线要按 `.bw/metrics.toml` 的目标判 —— A 刀还没接指标目标
        // 比对,这里如实给 false(不假装判过),接上之前灯不会因此变红。
        any_metric_red: false,
        delivered_last_week: merged_last_week || released,
    }
}

/// `2026-08-20` → `2026-W34`。认不出格式就返回 `None`,不猜。
fn week_of_date(s: &str) -> Option<String> {
    let d = time::Date::parse(
        s.trim(),
        &time::format_description::well_known::Iso8601::DATE,
    )
    .ok()?;
    Some(isoweek::iso_week_of(d))
}

impl App {
    /// 现算一次健康,并把结果写回项目墙的显示缓存。
    ///
    /// 总览屏自己从不读那两个缓存列当输入——它每次都调这个函数重算。缓存只
    /// 服务「不打开任何项目也要看见灯」的项目墙。
    pub async fn recompute_health(&self, project_id: ProjectId) -> Result<DerivedHealth> {
        let ws = self.workspace_of(project_id).await?;
        let week = isoweek::current_week();
        let inputs = collect_health_inputs(&ws, &week).await;
        let health = derive_project_health(&inputs);
        self.store.cache_project_health(project_id, &health).await?;
        Ok(health)
    }
}
