//! 开始本周、排期、发版本、缓存对账。
//!
//! 一条时序贯穿这个文件:**缓存先动、文件随后追上**。拖一张卡片先改库里的
//! 缓存列(界面立刻反映),再驱动一次仓文件改动;两边分歧时**以文件为准**,
//! 下一次对账用文件内容覆盖缓存,不是反过来。

use super::{App, AppError, Result};
use crate::command::Event;
use crate::isoweek;
use crate::model::{IssueId, IssueKind, IssueOrigin, IssueStatus, ProjectId};
use crate::repo::week_plan_file::{self, ActivityRow, MetricReading, OpsRow, WeekPlanDraft};
use crate::repo::{project_file, release_file};
use std::path::Path;

impl App {
    /// 总览「开始本周」。判据是**本周的周计划文件在不在**——查文件,不查任何
    /// 索引表。已经有了就什么都不做,如实回一条「已存在」。
    pub(super) async fn start_week_planning(
        &mut self,
        project_id: ProjectId,
        week: String,
    ) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        if week_plan_file::exists(&ws, &week) {
            return Ok(vec![Event::WeekPlanAlreadyExists { project_id, week }]);
        }

        let last_week = isoweek::previous_week(&week).unwrap_or_default();
        let last_lines = last_week_lines(&ws, &last_week).await;
        let readings = collect_readings(&ws, &week).await;

        // A 刀这一步是替身:真正的运作活①会起一次 agent 会话,和人一起复盘上
        // 周、更新指标、聊出本周目标与活。这里产出的草稿活标自带【mock】字样,
        // 绝不冒充真实对话的结论。
        // 标题里必须带周号:建活是按标题幂等的,两周用同样的标题的话,第二周
        // 确认草稿拿回的是上周那两张活的 id,一张新活都建不出来。
        let draft_titles = vec![
            format!("【mock】{week} 第一件业务活(草稿,等人确认)"),
            format!("【mock】{week} 第二件业务活(草稿,等人确认)"),
        ];

        let draft = WeekPlanDraft {
            week: week.clone(),
            origin: "human".into(),
            goal: Some(
                "【mock】本周目标待人确认——这一行由流程演示写出,不是真实对话的结论。".into(),
            ),
            activities: Vec::new(),
            readings,
            ops: vec![OpsRow {
                title: format!("运作活①更新指标 + 制定本周计划 {week}"),
                status: "进行中".into(),
                note: "复盘上周、更新指标、引导出本周目标与活".into(),
            }],
            last_week_lines: last_lines,
        };
        week_plan_file::write(&ws, &week, &week_plan_file::render(&draft))?;

        // 真正干这件事的是一次 agent 会话:复盘上周、更新指标、和人聊出本周
        // 要干什么。buddy 只负责把活建出来、把骨架文件写下去,然后立刻开工
        // —— 人点的那一下「开始本周」就到此为止,剩下的在会话屏里发生。
        let (issue_id, _, mut events) = self
            .create_ops_issue(
                project_id,
                format!("更新指标 + 制定本周计划 {week}"),
                "复盘上周、补齐 .bw/metrics.toml、和人交流出本周目标与业务活草稿。\n\n草稿没经人确认之前不许建活、不许落文件。"
                    .into(),
                super::OPS1_WORKFLOW,
                IssueOrigin::Human,
                week.clone(),
            )
            .await?;
        events.extend(self.run_issue(issue_id).await?);
        events.push(Event::WeekPlanStarted {
            project_id,
            week,
            issue_id,
            draft_titles,
        });
        Ok(events)
    }

    /// 人确认草稿之后真的建活,并把活写进周计划文件的活清单。
    pub(super) async fn confirm_week_draft(
        &mut self,
        project_id: ProjectId,
        week: String,
        titles: Vec<String>,
    ) -> Result<Vec<Event>> {
        let mut events = Vec::new();
        for title in titles {
            let created = self
                .create_issue(
                    project_id,
                    title,
                    String::new(),
                    Some(crate::model::StageKind::Build),
                    IssueKind::Business,
                    IssueOrigin::AgentSplit,
                    week.clone(),
                )
                .await?;
            events.extend(created);
        }
        self.rewrite_week_activities(project_id, &week).await?;
        Ok(events)
    }

    pub(super) async fn schedule_issue(
        &mut self,
        id: IssueId,
        week_of: Option<String>,
    ) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(id).await?;
        let target = week_of.unwrap_or_default();
        let order = self.next_sort_order(issue.project_id, &target).await?;
        self.store.set_issue_week(id, &target, order).await?;

        // 排进某一周 = 进「待办」;移回待办池 = 回 Backlog。只在状态机允许时动,
        // 不合法就只改排期不动状态。
        let want = if target.is_empty() {
            IssueStatus::Backlog
        } else {
            IssueStatus::Todo
        };
        if issue.status != want && issue.status.can_transition_to(want) {
            self.store.set_issue_status(id, want).await?;
        }

        // 文件随后追上:两周的清单都可能变(从这周挪到那周)。
        if !issue.week_of.is_empty() {
            self.rewrite_week_activities(issue.project_id, &issue.week_of.clone())
                .await?;
        }
        if !target.is_empty() {
            self.rewrite_week_activities(issue.project_id, &target)
                .await?;
        }

        Ok(vec![Event::IssueScheduled {
            id,
            week_of: target,
        }])
    }

    pub(super) async fn reorder_issue(
        &mut self,
        id: IssueId,
        after: Option<IssueId>,
    ) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(id).await?;
        let siblings = self
            .store
            .issues_in_week(issue.project_id, &issue.week_of)
            .await?;
        // 浮点排序值取中间:插进两张卡之间不必整列重排。
        let new_order = match after {
            None => {
                siblings
                    .iter()
                    .map(|i| i.sort_order)
                    .fold(f64::INFINITY, f64::min)
                    - 1.0
            }
            Some(a) => {
                let anchor = siblings
                    .iter()
                    .find(|i| i.id == a)
                    .ok_or(AppError::NoSuchIssue)?;
                let next = siblings
                    .iter()
                    .filter(|i| i.sort_order > anchor.sort_order && i.id != id)
                    .map(|i| i.sort_order)
                    .fold(f64::INFINITY, f64::min);
                if next.is_finite() {
                    (anchor.sort_order + next) / 2.0
                } else {
                    anchor.sort_order + 1.0
                }
            }
        };
        let new_order = if new_order.is_finite() {
            new_order
        } else {
            0.0
        };
        self.store.set_issue_sort_order(id, new_order).await?;
        if !issue.week_of.is_empty() {
            self.rewrite_week_activities(issue.project_id, &issue.week_of.clone())
                .await?;
        }
        Ok(vec![Event::IssueReordered { id }])
    }

    pub(super) async fn set_current_version(
        &mut self,
        project_id: ProjectId,
        version: String,
    ) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let mut file = project_file::read(&ws)?.unwrap_or_default();
        file.current_version = version.clone();
        project_file::write(&ws, &file)?;
        Ok(vec![Event::CurrentVersionChanged { version }])
    }

    /// 发版本:给选中的活写版本号,往 `docs/releases.md` 追加一行。
    /// 按版本号幂等 —— 已经有这一行就不写第二行。
    pub(super) async fn cut_release(
        &mut self,
        project_id: ProjectId,
        version: String,
        note: String,
        included: Vec<IssueId>,
    ) -> Result<Vec<Event>> {
        let version = version.trim().to_string();
        if version.is_empty() || version.contains('(') {
            // 「(待填)」这类占位文案是给人看的,不是版本号。放进去的后果是
            // 发版记录里多一行叫「(待填)」的版本,而且按版本号幂等 —— 这个
            // 项目以后再也发不出版。
            return Err(AppError::Refused(format!(
                "「{version}」不像一个版本号。先在配置屏把在研版本填好再发版。"
            )));
        }
        let ws = self.workspace_of(project_id).await?;
        release_file::write_default_if_missing(&ws)?;

        // **先写发版记录,写成了再回填版本号。** 反过来的话,用一个已经存在的
        // 版本号再发一次,活会被打上这个标签(还覆盖掉它上一次的版本号),而
        // 发版记录里根本没有这一次 —— 账就对不上了。
        let mut numbers = Vec::new();
        for id in &included {
            numbers.push(self.issue_or_err(*id).await?.number);
        }

        let today = crate::isoweek::today_local().to_string();
        let rows_written = release_file::append_row(
            &ws,
            &release_file::ReleaseRow {
                version: version.clone(),
                released_at: today,
                note,
                included_numbers: numbers,
                origin: "人发".into(),
            },
        )?;
        if rows_written {
            for id in &included {
                self.store.set_issue_version(*id, &version).await?;
            }
        }

        Ok(vec![Event::ReleaseCut {
            version,
            rows_written,
        }])
    }

    /// 用周计划文件覆盖库里的缓存列 —— **文件说了算**。
    ///
    /// 幂等,而且只更新、不删除:缓存里有文件里没有的活,原样留着(那可能是
    /// 刚建出来还没排进周计划的),只是不再被这次刷新碰。
    pub(super) async fn refresh_issue_cache(
        &mut self,
        project_id: ProjectId,
        week: String,
    ) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let Some(plan) = week_plan_file::read(&ws, &week)? else {
            return Ok(vec![Event::IssueCacheRefreshed { week, updated: 0 }]);
        };
        let issues = self.store.issues(project_id).await?;
        let mut updated = 0;
        for row in &plan.activities {
            let Some(issue) = issues.iter().find(|i| i.title == row.title) else {
                continue;
            };
            self.store
                .set_issue_week(issue.id, &week, row.order)
                .await?;
            self.store
                .set_issue_dispatch(issue.id, &row.tool, &row.workflow, &row.metric_key)
                .await?;
            updated += 1;
        }
        Ok(vec![Event::IssueCacheRefreshed { week, updated }])
    }

    /// 把库里这一周的活清单写回周计划文件的「业务活」表格。文件里别的段落
    /// (周目标、指标读数、运作、上周完成情况)原样保留。
    pub(crate) async fn rewrite_week_activities(
        &self,
        project_id: ProjectId,
        week: &str,
    ) -> Result<()> {
        let ws = self.workspace_of(project_id).await?;
        let Some(existing) = week_plan_file::read(&ws, week)? else {
            return Ok(());
        };
        let issues = self.store.issues_in_week(project_id, week).await?;
        let activities: Vec<ActivityRow> = issues
            .iter()
            .filter(|i| i.kind == IssueKind::Business)
            .map(|i| ActivityRow {
                order: i.sort_order,
                title: i.title.clone(),
                category: i.category,
                tool: i.tool.clone(),
                workflow: i.workflow.clone(),
                metric_key: i.metric_key.clone(),
                remote_number: i.remote_number,
            })
            .collect();
        let ops: Vec<OpsRow> = issues
            .iter()
            .filter(|i| i.kind == IssueKind::Ops)
            .map(|i| OpsRow {
                title: i.title.clone(),
                status: i.status.label().to_string(),
                note: String::new(),
            })
            .chain(existing.ops.iter().cloned())
            .collect();
        let ops = dedupe_ops(ops);

        // **原地换两张表,别的一个字不动。** 周计划文件是正本,人会往里写周目标
        // 的第二段、写风险、加自己的小节;而拖一张卡片就会触发一次回写。整份
        // 重渲染等于每拖一下就把人这周写的东西抹掉一次,而且顺手 commit 了。
        let mut raw = existing.raw.clone();
        if let Some(next) = week_plan_file::replace_table(
            &raw,
            "业务活",
            &week_plan_file::render_activity_table(&activities),
        ) {
            raw = next;
        }
        if let Some(next) =
            week_plan_file::replace_table(&raw, "本周运作", &week_plan_file::render_ops_table(&ops))
        {
            raw = next;
        }
        if raw != existing.raw {
            week_plan_file::write(&ws, week, &raw)?;
        }
        Ok(())
    }

    async fn next_sort_order(&self, project_id: ProjectId, week: &str) -> Result<f64> {
        let siblings = self.store.issues_in_week(project_id, week).await?;
        // 从 `f64::NEG_INFINITY` 起 fold,不是从 0:排序值可以是负的
        // (`reorder_issue` 往队首插就会产生负值),从 0 起会让新活挤到中间。
        let max = siblings
            .iter()
            .map(|i| i.sort_order)
            .fold(f64::NEG_INFINITY, f64::max);
        Ok(if max.is_finite() { max + 1.0 } else { 1.0 })
    }
}

fn dedupe_ops(rows: Vec<OpsRow>) -> Vec<OpsRow> {
    let mut seen = std::collections::HashSet::new();
    rows.into_iter()
        .filter(|r| seen.insert(r.title.clone()))
        .collect()
}

/// 上周真发生了什么 —— 现问 git,不查库。查不到就返回空,由渲染那一步写一句
/// 留白,不编。
async fn last_week_lines(ws: &Path, last_week: &str) -> Vec<String> {
    if last_week.is_empty() {
        return Vec::new();
    }
    let Ok(stats) = crate::git::week_stats(ws, last_week).await else {
        return Vec::new();
    };
    if stats.commits == 0 && stats.merges == 0 {
        return Vec::new();
    }
    let mut v = vec![format!(
        "上周({last_week})真实提交 {} 条、合入 {} 次(来源:git log,可在终端复算)",
        stats.commits, stats.merges
    )];
    if !stats.top_dirs.is_empty() {
        v.push(format!("动得最多的目录:{}", stats.top_dirs.join("、")));
    }
    v
}

/// 本周的指标读数。A 刀先把上一份周计划文件里的读数原样带过来(这就是「多台
/// 机器看到同一份数字」那条路);真去采集是运作活①那一刀的事,这里绝不替它
/// 编一个数出来。
async fn collect_readings(ws: &Path, week: &str) -> Vec<MetricReading> {
    let Some(prev) = isoweek::previous_week(week) else {
        return Vec::new();
    };
    week_plan_file::read(ws, &prev)
        .ok()
        .flatten()
        .map(|p| p.readings)
        .unwrap_or_default()
}
