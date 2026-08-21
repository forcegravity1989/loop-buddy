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

/// 运作活①的 `workflow` 字段值,对应剧本 `week-planning`。两者的对应关系在
/// `standard/06-defaults/ops/README.md` 那张表里。
pub const OPS1_WORKFLOW: &str = "更新指标与周计划";
pub const OPS2_WORKFLOW: &str = "资产盘点";
pub const OPS3_WORKFLOW: &str = "规范铺底";

/// 剧本名。开工时按这个 slug 在 **buddy 自己的技能目录**里找到那份
/// `SKILL.md`,把**名字 + 一句话 + 路径**给 agent(正文它自己按需读,见
/// [`crate::standard::skills`])。
/// 运作活①这一周那张活的标题。**它同时是建活的幂等键和「本周开没开过工」
/// 的判据** —— 两者必须是同一个东西:按 `week_of` 查的话,人把这张卡从本周拖
/// 回待办池,判据就落空了,而建活按标题去重又拿回同一张活,于是「开始本周」
/// 会对一张已经在评审中的活重新开工,弹一句「不是能开工的状态」。定时那条路
/// 早踩过同一个坑(见 [`App::tick_scheduler`]),这里不重踩。
pub fn ops1_title(week: &str) -> String {
    format!("更新指标 + 制定本周计划 {week}")
}

pub fn skill_slug(workflow: &str) -> Option<&'static str> {
    match workflow {
        OPS1_WORKFLOW => Some("week-planning"),
        OPS2_WORKFLOW => Some("asset-audit"),
        // 规范铺底**没有 agent 步骤**(2026-08-20:只铺 `.bw/`,仓根一个字不写),
        // 所以它没有剧本可挂。给这个项目写开发手册的那份剧本已经挪进资产盘点,
        // 见 `standard/06-defaults/ops/asset-audit/skills/project-handbook/`。
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
    /// 没挂远端或者这张活确实没有 MR,就只走「完成」那一步(本地项目本来就没
    /// 有可合的东西),并在事件里如实说没合。
    ///
    /// 库里 `pr_number = 0` **不等于**远端没有 MR:队友自己在会话里
    /// `gh pr create` 开的那个 buddy 从没记过。所以合之前先拿分支去远端问一次,
    /// 问到了顺手记进库。
    ///
    /// 真合成了还要做两件本机收尾:把主检出快进到远端最新
    /// ([`Self::pull_main_after_merge`]),把那条活分支收掉
    /// ([`Self::drop_issue_branch`])。两件都是尽力而为,做不成不回滚这次合入
    /// 与结清,但下落一律写进事件的 `local_note`。
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
        // 已经完成的活直接短路。不短路的话下面还会再打一次远端:对着一个
        // 已经 merged 的 PR 再合一次必然报错,人看到一条像出了大事的红字,
        // 而这张活其实好好的。
        if issue.status == IssueStatus::Done {
            return Ok(vec![Event::IssueMerged {
                id,
                pr_number: issue.pr_number,
                merged: false,
                local_note: String::new(),
            }]);
        }
        if !issue.status.can_transition_to(IssueStatus::Done) {
            return Err(AppError::IllegalTransition {
                from: issue.status.label().to_string(),
                to: IssueStatus::Done.label().to_string(),
            });
        }

        // **按 provider 分发**。以前这里写死 `github::merge_pr`,codehub 项目
        // 点「合入并完成」就是拿 `gh` 去打一个 gh 根本不认识的仓。
        let remote = if project.has_remote() {
            Some(
                v4_engine::remote::Remote::for_project(
                    &project.provider,
                    &project.remote_host,
                    &project.remote_path,
                )
                .map_err(|e| AppError::Exec(format!("认不出远端类型:{e}")))?,
            )
        } else {
            None
        };

        // 库里没记 MR 号不等于远端没有 MR —— 队友自己在会话里 `gh pr create`
        // 开的那个,buddy 从来没记过。**去远端问一次**(读回为证,不是听谁说
        // 的);问到了顺手记进库,下次不用再问。
        let mut pr_number = issue.pr_number;
        if pr_number == 0 && !issue.branch.is_empty() {
            if let Some(r) = &remote {
                if let Ok(Some(n)) = r.open_mr_for_branch(&issue.branch).await {
                    pr_number = n;
                    self.store
                        .set_issue_remote(id, &issue.branch, n, issue.remote_number)
                        .await?;
                }
            }
        }

        let merged = match (&remote, pr_number) {
            (Some(r), n) if n > 0 => {
                r.merge_mr(n)
                    .await
                    .map_err(|e| AppError::Exec(format!("合入 MR #{n} 没成:{e}")))?;
                true
            }
            // 没挂远端、或者这张活确实没有 MR:只走「完成」那一步,并在事件里
            // 如实说没合。**不假装合过**。
            _ => false,
        };

        // 合入之后的本机收尾。两步都是尽力而为,失败都不回滚这次合入与结清 ——
        // 但每一步的下落都记下来,原话端到界面上。
        let mut notes: Vec<String> = Vec::new();
        let mut main_has_it = false;
        if merged {
            let (ok, note) = self.pull_main_after_merge(issue.project_id).await;
            main_has_it = ok;
            notes.push(note);
        }
        // 结清必须排在删分支前面:分支这会儿还被这张活的 worktree 检出着,
        // git 不让删,而收 worktree 是结清那一步做的。
        let settled = self.transition_issue(id, IssueStatus::Done).await?;
        // **主检出没拉动就不收这条分支**。拉不动的时候,本机主干里没有这次
        // 合入的内容,这条 `bw/issue-<号>` 就是本机唯一还够得着它的地方 ——
        // 这时候删掉它,人想在本机看一眼刚合了什么都得先 fetch。内容确实没
        // 丢(远端主干和 `origin/bw/issue-<号>` 上都在),但留着更省事。
        if merged && main_has_it {
            notes.push(self.drop_issue_branch(&issue).await);
        } else if merged {
            notes.push(format!(
                "本机分支 {} 先留着:主检出没拉动,它是本机唯一还够得着这次合入的地方",
                v4_engine::github::issue_branch(issue.number)
            ));
        }

        // 合的是运作活①、而且主检出真的拉到了这次合入 —— 周计划文件这会儿才第
        // 一次出现在主检出里,照着它对一次账:文件里列的业务活建成卡片、已有
        // 的活把排期与工具刷到文件说的样子。**只在真拉到了才做**,否则读的还是
        // 旧文件(或压根没有),对出来的账是假的。
        let mut refreshed = Vec::new();
        if merged && main_has_it && issue.workflow == OPS1_WORKFLOW && !issue.week_of.is_empty() {
            let week = issue.week_of.clone();
            match self.refresh_issue_cache(issue.project_id, week).await {
                Ok(e) => refreshed = e,
                // 对账没跑成不回滚已经发生的合入与结清 —— 如实记一句,人可以
                // 在计划屏点「按文件刷新」重来一次。原话端出去,不吞。
                Err(e) => notes.push(format!(
                    "周计划对账没跑成(可在计划屏点「按文件刷新」重来):{}",
                    one_line(&e.to_string())
                )),
            }
        }

        let mut events = vec![Event::IssueMerged {
            id,
            pr_number,
            merged,
            local_note: notes.join("；"),
        }];
        // 合入这件事排在前面,界面上那句话说的才是人刚做的动作;结清事件跟在
        // 后面,读回时两条都在。
        events.extend(settled);
        events.extend(refreshed);
        // 群通知排在最后:发不出去也不影响上面已经记完的账。
        if let Some(e) = self.chat_notify_issue(id, "merged").await {
            events.push(e);
        }
        Ok(events)
    }

    /// 合入成功之后,把主检出快进到远端最新。
    ///
    /// **这是 2026-08-20 试点抓出来的真缺口**:人在界面上点了合入、MR 真的合进
    /// 了远端主干,可本机主检出还停在旧提交 —— 合进去的 `.bw/` 那几份件在工作
    /// 区里根本不存在,于是节律、类别→工具映射、规范对账全都读不到,而且会一
    /// 直读不到,直到有人自己去终端敲一句 `git pull`。
    ///
    /// 拉不动(工作区脏、没挂远端、没网、本机主干和远端岔开)就**如实说拉不
    /// 动**,不影响这次合入与结清 —— 绝不假装拉过了。
    /// 返回「主检出现在有没有这次合入的内容」+ 一句人话回执。
    ///
    /// 那个 bool 不只是给界面看的 —— 收不收本机那条活分支要看它(见
    /// [`Self::drop_issue_branch`] 的调用处)。
    async fn pull_main_after_merge(&self, project_id: ProjectId) -> (bool, String) {
        let Ok(ws) = self.workspace_of(project_id).await else {
            return (false, "主检出没拉:算不出这个项目的工作区路径".into());
        };
        match crate::git::pull_ff(&ws).await {
            Ok(true) => (true, "主检出已拉到最新".into()),
            Ok(false) => (true, "主检出本来就是最新的".into()),
            // git 的原话是多行的,压成一行再端出去 —— 压行不是删字,一个字
            // 都不省。
            Err(e) => (
                false,
                format!(
                    "主检出没拉动(合入与结清本身已经生效,可以自己去仓里 git pull):{}",
                    one_line(&e.to_string())
                ),
            ),
        }
    }

    /// 结清之后收掉本机那条已经合过的活分支。
    ///
    /// 只认 `bw/issue-<号>` 这个名字。活的 `branch` 字段在没隔出 worktree 的项目
    /// 上记的是主分支名(不是 git 仓、或者还没有第一条提交的空仓),照着它删就
    /// 是拿主分支开刀 —— 认死名字,不认那个字段说什么。
    async fn drop_issue_branch(&self, issue: &crate::model::Issue) -> String {
        let branch = v4_engine::github::issue_branch(issue.number);
        if issue.branch != branch {
            return "没有单独的活分支要收(这张活的改动不在 bw/issue- 分支上)".into();
        }
        let Ok(ws) = self.workspace_of(issue.project_id).await else {
            return format!("本机分支 {branch} 没收:算不出这个项目的工作区路径");
        };
        if !crate::git::branch_exists(&ws, &branch).await {
            return format!("本机已经没有 {branch} 这条分支了");
        }
        match crate::git::delete_merged_branch(&ws, &branch).await {
            Ok(()) => format!("本机分支 {branch} 已收掉"),
            // 最常见的一种:活的 worktree 里还有没提交的改动,结清那一步按纪律
            // 没收它,分支就还被检出着,删不动。如实说,不硬删。
            Err(e) => format!(
                "本机分支 {branch} 没收掉(不影响合入与结清):{}",
                one_line(&e.to_string())
            ),
        }
    }
}

/// 把 git 的多行原话压成一行。**压行不是删字** —— 一个字都不省,只是让它能塞
/// 进界面上那一条回执。
pub(super) fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
