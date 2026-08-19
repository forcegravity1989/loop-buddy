//! 老项目历史回填 —— 运作活②「资产盘点」的首次模式(`mode=first`)。
//!
//! **三层流水线里,这个文件是第一层**:buddy 自己把能算的数字全算出来,写成
//! 同格式的正常文件。第二层(agent 读文本、补措辞与 PROJECT.md 草稿)与第三层
//! (人评审确认)不在这里 —— 第二层要起真 agent 会话,还没做,见
//! `docs/LEFTOVERS.md`。
//!
//! 四条防伪规则,每一条都是预研里真踩出来的:
//!
//! 1. **合入按双亲结构判定**(`git log --merges`),不按提交消息里的文字匹配
//!    —— 文字匹配会漏掉手写的合并提交(样例仓 71 条真实合入漏掉 23 条)。
//!    `week_stats` 已经是这么算的,这里直接用。
//! 2. **没有标签就诚实留空**,绝不拿提交日期倒推一个版本号出来。
//! 3. **周目标不倒推**。历史周当时没写过周目标,回填就写「未发现」——从提交
//!    消息里编一句「本周主要在重构 X」是最容易过关也最没有价值的假话。
//! 4. **回填的东西不点灯**。历史周文件与标「回填」的版本行只解释过去,健康灯
//!    的三条判据一条都不读它们(判据读的是当前周的 git 与本周计划文件)。
//!
//! 重跑安全:同一周的文件整份覆盖(和运作活①「这份文件归它管」同一个契约),
//! 发版行按版本号去重。**已经存在的周文件一律不碰** —— 那可能是人自己写的。

use super::{App, Result};
use crate::command::Event;
use crate::isoweek;
use crate::model::ProjectId;
use crate::repo::release_file::{self, ReleaseRow};
use crate::repo::week_plan_file::{self, OpsRow, WeekPlanDraft};

/// 往前最多回填多少周。两年 —— 再往前的周文件对今天的管理没有帮助,只是把
/// `docs/plan/` 撑大。扫到就停,不是"扫不动",这一条如实写进回填报告。
const MAX_WEEKS: usize = 104;

impl App {
    pub(super) async fn backfill_history(&mut self, project_id: ProjectId) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        if !crate::git::is_repo(&ws).await {
            return Ok(vec![Event::HistoryBackfilled {
                project_id,
                weeks: Vec::new(),
                releases: Vec::new(),
                note: "这个目录不是 git 仓,没有历史可回填".into(),
            }]);
        }

        let Some(first) = crate::git::first_commit_date(&ws).await else {
            return Ok(vec![Event::HistoryBackfilled {
                project_id,
                weeks: Vec::new(),
                releases: Vec::new(),
                note: "仓里一条提交都没有,没有历史可回填".into(),
            }]);
        };

        // ── 历史周 ──────────────────────────────────────────
        let today = isoweek::today_local();
        let current = isoweek::current_week();
        let mut week =
            time::Date::parse(&first, &time::format_description::well_known::Iso8601::DATE)
                .map(isoweek::iso_week_of)
                .unwrap_or_else(|_| current.clone());

        let mut weeks = Vec::new();
        let mut truncated = false;
        for n in 0.. {
            if week == current {
                break;
            }
            if n >= MAX_WEEKS {
                truncated = true;
                break;
            }
            let this = week.clone();
            let Some(next) = isoweek::next_week(&this) else {
                break;
            };
            week = next;
            // 那一周之后才建的仓 / 未来的周,week_bounds 解不出来就跳过。
            if isoweek::week_start(&this).is_none_or(|d| d > today) {
                continue;
            }
            // 已经有文件了就不碰 —— 那可能是人自己写的。
            if week_plan_file::exists(&ws, &this) {
                continue;
            }
            let stats = crate::git::week_stats(&ws, &this).await.unwrap_or_default();
            // 那一周什么都没发生,就不要给它凭空造一份文件。
            if stats.commits == 0 {
                continue;
            }

            let dirs = if stats.top_dirs.is_empty() {
                "—".to_string()
            } else {
                stats.top_dirs.join("、")
            };
            let draft = WeekPlanDraft {
                week: this.clone(),
                origin: "backfill".into(),
                // 不倒推。当时没写周目标,回填就说没写。
                goal: Some(
                    "(未发现——这一周是回填的,当时没有周计划记录,buddy 不从提交消息里倒推目标)"
                        .into(),
                ),
                activities: Vec::new(),
                readings: Vec::new(),
                ops: vec![OpsRow {
                    title: "历史回填".into(),
                    status: "回填".into(),
                    note: format!(
                        "提交 {} 条 · 合入 {} 条(本地口径,按双亲结构判定)· 改得最多的目录:{dirs}",
                        stats.commits, stats.merges
                    ),
                }],
                last_week_lines: vec![format!(
                    "复算命令:git log --since=<{this} 周一> --until=<下周一> --pretty=format:%H | wc -l"
                )],
            };
            week_plan_file::write(&ws, &this, &week_plan_file::render(&draft))?;
            weeks.push(this);
        }

        // ── 历史版本 ────────────────────────────────────────
        let existing: Vec<String> = release_file::read(&ws)?
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.version)
            .collect();
        let mut releases = Vec::new();
        for (tag, date) in crate::git::tags_with_dates(&ws).await {
            if existing.contains(&tag) {
                continue;
            }
            let row = ReleaseRow {
                version: tag.clone(),
                released_at: date,
                note: "回填自 git 标签".into(),
                included_numbers: Vec::new(),
                origin: "回填 · git tag".into(),
            };
            if release_file::append_row(&ws, &row)? {
                releases.push(tag);
            }
        }

        let note = match (weeks.len(), releases.len(), truncated) {
            (0, 0, _) => "没有可回填的东西:历史周都已经有文件了,标签也都在发版记录里".into(),
            (w, r, true) => format!(
                "回填了 {w} 份历史周文件、{r} 行历史版本;只回填到 {MAX_WEEKS} 周之前为止,更早的没扫"
            ),
            (w, r, false) => format!("回填了 {w} 份历史周文件、{r} 行历史版本"),
        };
        Ok(vec![Event::HistoryBackfilled {
            project_id,
            weeks,
            releases,
            note,
        }])
    }
}
