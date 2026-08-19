//! 运作活:buddy 自己发起的三张标准动作。
//!
//! 三张活的 `workflow` 字段取值是它们的身份证——定时判据、注入哪份剧本、配置
//! 屏「用过几次」全靠它:
//!
//! | 活 | `kind` | `origin` | `workflow` |
//! |---|---|---|---|
//! | ①更新指标 + 制定本周计划 | `ops` | `human` | `更新指标与周计划` |
//! | ②资产盘点 | `ops` | `auto` | `资产盘点` |
//! | ③规范铺底 | `ops` | `auto` | `规范铺底`([`super::bootstrap`] 那边建) |
//!
//! **没有定时任务表**。「这周该不该建②」这个问题的答案不存在任何一张状态表
//! 里,它是现查出来的:本周有没有一张 `workflow='资产盘点'` 的活。这条查询
//! 本身就是幂等锁,也是补建逻辑——错过一次 tick(那会儿 buddy 没开着)下次
//! 启动的第一次 tick 天然成立,自动补上。

use super::{App, AppError, Result};
use crate::command::Event;
use crate::isoweek;
use crate::model::{IssueId, IssueKind, IssueOrigin, IssueStatus, ProjectId};
use crate::repo::issue_policy_file;

/// 运作活①的 `workflow` 字段值,同时是它在 `.claude/skills/` 里的剧本名
/// (`week-planning`)对应的中文名。两者的对应关系在
/// `standard/06-defaults/ops/README.md` 那张表里。
pub const OPS1_WORKFLOW: &str = "更新指标与周计划";
pub const OPS2_WORKFLOW: &str = "资产盘点";
pub const OPS3_WORKFLOW: &str = "规范铺底";

/// 剧本名(项目仓 `.claude/skills/<slug>/SKILL.md`)。开工时按这个 slug 把正
/// 文注入给 agent。
pub fn skill_slug(workflow: &str) -> Option<&'static str> {
    match workflow {
        OPS1_WORKFLOW => Some("week-planning"),
        OPS2_WORKFLOW => Some("asset-audit"),
        OPS3_WORKFLOW => Some("merge-adjust"),
        _ => None,
    }
}

impl App {
    /// 建一张运作活。标题幂等(同一周重跑拿回同一张),建完立刻回 id。
    pub(super) async fn create_ops_issue(
        &mut self,
        project_id: ProjectId,
        title: String,
        body: String,
        workflow: &str,
        origin: IssueOrigin,
        week_of: String,
    ) -> Result<(IssueId, bool, Vec<Event>)> {
        // 建活是按标题幂等的。调用方**必须知道这一张是不是新建的** —— 定时那
        // 条路会在建完之后立刻开工,拿到一张老活还去开工,就会把上一次正跑着
        // 的会话掐掉重开。
        let created = self
            .store
            .issue_by_title(project_id, &title)
            .await?
            .is_none();
        let events = self
            .create_issue(
                project_id,
                title,
                body,
                None,
                IssueKind::Ops,
                origin,
                week_of,
            )
            .await?;
        let id = events
            .iter()
            .find_map(|e| match e {
                Event::IssueCreated { id, .. } => Some(*id),
                _ => None,
            })
            .ok_or_else(|| AppError::Refused("运作活没建出来".into()))?;
        // 类别为空 ⇒ `create_issue` 查不到三列映射,`tool`/`workflow` 都是空的。
        // 运作活的 workflow 不由映射决定,是这张活的身份,建完补上。
        let issue = self.issue_or_err(id).await?;
        if issue.workflow != workflow {
            self.store
                .set_issue_dispatch(id, "claude_cli", workflow, &issue.metric_key)
                .await?;
        }
        Ok((id, created, events))
    }

    /// 定时:到点就自己建运作活②并自己开工。
    ///
    /// 三张运作活里唯一不需要人点一下才开工的一张——所以它也是唯一一处
    /// buddy 自动**建**活的地方。**自动建不等于自动完成**:这张活和别的活
    /// 走同一条状态机,最远只到「评审中」。
    pub(super) async fn tick_scheduler(&mut self, project_id: ProjectId) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let Some(policy) = issue_policy_file::read(&ws).ok().flatten() else {
            // 没有约定文件 = 这个项目还没铺规范,不替它安排节律。
            return Ok(vec![]);
        };
        // 没有 [cadence] 段 = 这个项目没配节律,不替它安排。
        let Some(cadence) = policy.cadence else {
            return Ok(vec![]);
        };
        let spec = cadence.ops2_schedule.trim().to_string();
        if spec.is_empty() || !isoweek::schedule_passed_this_week(&spec) {
            return Ok(vec![]);
        }

        let week = isoweek::current_week();
        let title = format!("资产盘点 {week}");
        // **判据必须和建活的幂等键是同一个东西**(都是标题)。以前这里查的是
        // 「本周有没有 kind=ops 且 workflow=资产盘点 的活」—— 人把那张卡从本周
        // 拖回待办池,`week_of` 变空,判据就查不到了;而建活按标题去重又拿回同
        // 一张老活,于是每分钟对它重开一次会话。
        if self
            .store
            .issue_by_title(project_id, &title)
            .await?
            .is_some()
        {
            return Ok(vec![]);
        }

        let (id, created, mut events) = self
            .create_ops_issue(
                project_id,
                title,
                // mode 不进 `Command::RunIssue` 的签名——写在这张活自己的说明
                // 里,剧本第一步读自己的说明就知道走哪条路。定时建的恒为 weekly。
                "mode: weekly\n\n盘点这一周仓内的全部资产,微重构只写建议不动手。".into(),
                OPS2_WORKFLOW,
                IssueOrigin::Auto,
                week.clone(),
            )
            .await?;
        if !created {
            // 上面那条判据已经挡住了,走到这里说明有别的路径先建了同名活。
            // 不开工 —— 开工的责任归建它的那条路径。
            return Ok(events);
        }
        events.push(Event::OpsAutoFired {
            id,
            workflow: OPS2_WORKFLOW.into(),
            week,
        });
        events.extend(self.run_issue(id).await?);
        Ok(events)
    }
}

impl App {
    /// 「合入并完成」:先真的把 MR 合了,再把活推到「完成」。
    ///
    /// 顺序不能反。先推完成再合入的话,合入失败就留下一张「已完成」但改动还
    /// 挂在分支上的活 —— 账目和仓对不上,而且完成是结清过的,回不去。
    ///
    /// **合入失败就整条失败**,活留在「评审中」原地,人看得见原话、可以重试。
    /// 没挂远端或者这张活没有 MR,就只走「完成」那一步(本地项目本来就没有
    /// 可合的东西),并在事件里如实说没合。
    pub(super) async fn merge_and_settle(&mut self, id: IssueId) -> Result<Vec<Event>> {
        let issue = self.issue_or_err(id).await?;
        let project = self
            .store
            .project(issue.project_id)
            .await?
            .ok_or_else(|| AppError::NoSuchProject(issue.project_id.uuid().to_string()))?;

        // **先问状态机答不答应,再动远端**。反过来的话:MR 在 GitHub 上真的被
        // squash 合了,而 `InProgress → Done` 不合法,错误抛出去,人看到「没做
        // 成」以为什么都没发生 —— 实际远端已经合了,再点一次还会因为 PR 已
        // merged 报错,这张活再也走不到完成。
        if issue.status != IssueStatus::Done && !issue.status.can_transition_to(IssueStatus::Done) {
            return Err(AppError::IllegalTransition {
                from: issue.status.label().to_string(),
                to: IssueStatus::Done.label().to_string(),
            });
        }

        let merged = if issue.pr_number > 0 && project.has_remote() {
            bw_engine::github::merge_pr(&project.remote_path, issue.pr_number)
                .await
                .map_err(|e| AppError::Exec(format!("合入 MR #{} 没成:{e}", issue.pr_number)))?;
            true
        } else {
            false
        };

        let mut events = vec![Event::IssueMerged {
            id,
            pr_number: issue.pr_number,
            merged,
        }];
        // 合入这件事排在前面,界面上那句话说的才是人刚做的动作;结清事件跟在
        // 后面,读回时两条都在。
        events.extend(self.transition_issue(id, IssueStatus::Done).await?);
        // 群通知排在最后:发不出去也不影响上面已经记完的账。
        if let Some(e) = self.chat_notify_issue(id, "merged").await {
            events.push(e);
        }
        Ok(events)
    }
}
