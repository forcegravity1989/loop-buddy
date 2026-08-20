//! 规范铺底(运作活③)与规范对账。
//!
//! 只做**第 1 步**:buddy 自己把核心件写进项目仓、复制预置技能包、记指纹,
//! 在**这张活自己的 worktree 和分支上**提交,推上去,开一个 MR 等人合。第 2
//! 步「合并调整」与第 3 步「历史回填」要起 agent 会话,还没做——探测到了什么
//! 如实写进这张活的正文,但那两步没跑就是没跑。
//!
//! **不在主检出上动手**。写文件、提交、开分支全在
//! [`worktree`](super::worktree) 供给的那棵树里发生,人自己的工作目录一个字节
//! 都不会被碰;两张活同时在跑也不会撞在一起。

use super::worktree;
use super::{App, AppError, Result};
use crate::command::Event;
use crate::model::{Issue, IssueOrigin, IssueStatus, ProjectId};
use crate::repo::managed_file::{self, Reconcile};
use crate::repo::project_file;
use crate::standard;
use crate::standard::bootstrap::{self as boot, BootstrapVars};

impl App {
    pub(super) async fn run_standard_bootstrap(
        &mut self,
        project_id: ProjectId,
    ) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let project = self
            .store
            .project(project_id)
            .await?
            .ok_or_else(|| AppError::NoSuchProject(project_id.uuid().to_string()))?;
        let file = project_file::read(&ws)?.unwrap_or_default();

        let probe = boot::probe(&ws).await;
        // 幂等键是标题,所以标题**只能是「规范铺底 v<版本>」**,不能随探测结果
        // 变。之前把「· 含合并调整」拼进标题,而第一次铺底自己写了 CLAUDE.md、
        // 提交之后第二次探测结论就变了 —— 标题跟着变,幂等失效,重跑多建一张
        // 活。探测到了什么写进正文,不写进标题。
        let title = boot::issue_title();
        let body = format!(
            "探测结果:{}。\n\n本次要跑的步骤:{}\
             \n\n本次执行:写核心件(buddy 自己写,不起 agent)。\
             \n\n还没跑的步骤如实列在这里,不假装做过:{}",
            probe.reasons.join("、"),
            boot::planned_steps(&probe),
            pending_steps(&probe)
        );

        let (issue_id, _, _) = self
            .create_ops_issue(
                project_id,
                title,
                body.clone(),
                super::OPS3_WORKFLOW,
                IssueOrigin::Auto,
                String::new(),
            )
            .await?;

        let remote = if project.has_remote() {
            format!("{} · `{}`", project.provider, project.remote_path)
        } else {
            "—(还没挂远端)".to_string()
        };
        let vars = BootstrapVars {
            name: file.name.clone(),
            brief: file.brief.clone(),
            benchmark: file.benchmark.clone(),
            north_star: file.opportunity.clone(),
            remote,
            owner: "—(单人项目,Builder 本人)".into(),
            current_version: if file.current_version.is_empty() {
                "v0.1".into()
            } else {
                file.current_version.clone()
            },
            chat: super::project::chat_label(&file.chat),
        };
        // 这张活自己的一棵树、自己的一个分支。人的主检出不动。
        let issue = self.issue_or_err(issue_id).await?;
        let tree = worktree::provision(&ws, issue.number).await?;
        let report = boot::write_core_files(&tree.path, &vars)?;

        // 只提交这次真写下去的那些件,不用 `add -A`:这棵树是干净的检出,但
        // 规矩就是规矩 —— 提交里出现的每一个文件都该是 buddy 自己写的。
        let outcome = crate::git::commit_paths(
            &tree.path,
            &report.written,
            &format!("docs(bw): 规范铺底 v{} · 核心件", standard::version()),
        )
        .await
        .unwrap_or_default();
        let committed = outcome.committed;

        // 仓的 .gitignore 拒收的件属于本机检出,不属于分支 —— 放一份到主工作
        // 区,否则技能包只存在于这一张活的 worktree 里,下一张活开新树就读不到
        // 剧本了。已经有同名文件就不覆盖。
        let (mirrored, kept) = if tree.isolated {
            worktree::mirror_ignored(&ws, &tree.path, &outcome.refused)
        } else {
            (Vec::new(), Vec::new())
        };

        // 推分支 + 开 MR。没挂远端、没隔出树、或者压根没提交出东西,就都不做
        // ——如实记下为什么没有 MR,不摆一个空号。
        let mr = self
            .push_and_open_mr(&project, &tree, committed, &title_for_mr())
            .await;

        // 跳过的件如实写进这张活的说明,评审的人不用猜为什么少了一份。
        // 注意是**从头拼一遍再整体覆盖**,不是往现有正文后面追加 —— 建活是按
        // 标题幂等的,重跑会拿到同一张活,追加就会把「跳过的件」滚成两份。
        let mut b = body;
        b.push_str(&format!(
            "\n\n干活的地方:`{}`\n分支:`{}`",
            tree.path.display(),
            tree.branch
        ));
        if !tree.isolated {
            b.push_str("\n(这个工作区不是 git 仓,开不了 worktree,是就地写的)");
        }
        if !report.skipped.is_empty() {
            b.push_str("\n\n跳过的件:\n");
            for (path, why) in &report.skipped {
                b.push_str(&format!("- `{path}`:{why}\n"));
            }
        }
        if !outcome.refused.is_empty() {
            b.push_str(
                "\n\n写下去了但**没进版本控制**的件(这个仓的 .gitignore 忽略了它们,\
                 buddy 不用 -f 顶回去 —— 那是项目自己的决定)。它们属于本机检出,\
                 不属于分支,合 MR 不会带上:\n",
            );
            // 一条路径只出现一次,后面跟它的下落 —— 三份互相重复的清单谁都不会看。
            for path in &outcome.refused {
                let disposition = if mirrored.contains(path) {
                    "已放进你的工作目录"
                } else if kept.contains(path) {
                    "你的工作目录里已经有同名文件,没覆盖"
                } else {
                    "只在这张活的目录里"
                };
                b.push_str(&format!("- `{path}` —— {disposition}\n"));
            }
        }
        b.push_str(&format!("\n\nMR:{}", mr.note));
        self.store.set_issue_body(issue_id, &b).await?;
        self.store
            .set_issue_remote(issue_id, &tree.branch, mr.number, issue.remote_number)
            .await?;

        let mut events = vec![Event::StandardBootstrapped {
            project_id,
            issue_id,
            files: report.written,
            committed,
        }];
        // 真提交出东西了才推「评审中」—— 该人看一眼了。什么都没写下去的时候
        // 原地不动,不假装干过活。**最远只到评审中**,「完成」永远由人点。
        if committed && issue.status.can_transition_to(IssueStatus::InProgress) {
            events.extend(
                self.transition_issue(issue_id, IssueStatus::InProgress)
                    .await?,
            );
            events.extend(
                self.transition_issue(issue_id, IssueStatus::InReview)
                    .await?,
            );
        }
        Ok(events)
    }

    /// 推分支 + 开 MR。**每一条不做的理由都说出来**,界面上不会出现一个来历
    /// 不明的空 MR 号。
    async fn push_and_open_mr(
        &self,
        project: &crate::model::Project,
        tree: &worktree::IssueTree,
        committed: bool,
        title: &str,
    ) -> MrOutcome {
        if !committed {
            return MrOutcome::none("这次没有新东西要提交,没开");
        }
        if !tree.isolated {
            return MrOutcome::none("这个工作区不是 git 仓,没有分支可提");
        }
        if !project.has_remote() {
            return MrOutcome::none("这个项目还没挂远端,分支只在本机(合的时候直接 merge 这个分支)");
        }
        if let Err(e) = crate::git::push_branch(&tree.path, &tree.branch).await {
            return MrOutcome::none(&format!("推分支没成,原话:{e}"));
        }
        let remote = match bw_engine::remote::Remote::for_project(
            &project.provider,
            &project.remote_host,
            &project.remote_path,
        ) {
            Ok(r) => r,
            Err(e) => return MrOutcome::none(&format!("认不出远端类型:{e}")),
        };
        let body = "buddy 的「规范铺底」写下的核心件。合入之后这个项目就有管理体系的正本了。\n\n                    这些件全是 buddy 自己写的,没有起 agent。";
        match remote
            .create_mr_on_branch(&tree.path, &tree.branch, title, body)
            .await
        {
            Ok(pr) => MrOutcome {
                number: pr.number(),
                note: format!("#{}", pr.number()),
            },
            Err(e) => MrOutcome::none(&format!("开 MR 没成,原话:{e}")),
        }
    }

    /// 纯读的对账:缺 / 过期 / 人改过。不建活、不写仓。
    pub(super) async fn reconcile_standard(&mut self, project_id: ProjectId) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let managed = managed_file::read(&ws)?.unwrap_or_default();
        let version = standard::version();

        let mut missing = Vec::new();
        let mut stale = Vec::new();
        let mut human_edited = Vec::new();

        let targets: Vec<String> = standard::CORE_TEMPLATES
            .iter()
            .map(|t| t.target.to_string())
            .chain(
                standard::preset_skill_packages()
                    .into_iter()
                    .map(|(p, _)| p),
            )
            .chain(
                standard::ops_workflow_packages()
                    .into_iter()
                    .map(|(p, _)| p),
            )
            .collect();

        for target in targets {
            let disk = std::fs::read(ws.join(&target)).ok();
            match managed_file::reconcile(managed.entry(&target), disk.as_deref(), version) {
                Reconcile::Missing => missing.push(target),
                Reconcile::Stale => stale.push(target),
                Reconcile::HumanEdited => human_edited.push(target),
                Reconcile::UpToDate => {}
            }
        }

        Ok(vec![Event::StandardReconciled {
            project_id,
            missing,
            stale,
            human_edited,
        }])
    }
}

/// 开 MR 的结果。`number == 0` = 没有 MR,`note` 说明为什么 —— 两件事永远
/// 一起走,不会出现「没号也没理由」。
struct MrOutcome {
    number: u32,
    note: String,
}

impl MrOutcome {
    fn none(why: &str) -> MrOutcome {
        MrOutcome {
            number: 0,
            note: why.to_string(),
        }
    }
}

fn title_for_mr() -> String {
    format!("规范铺底 v{} · 核心件", standard::version())
}

fn pending_steps(probe: &boot::BootstrapProbe) -> String {
    let mut v = Vec::new();
    if probe.has_agent_docs {
        v.push("合并调整(把 buddy 的固定章节并进已有 AGENTS.md / CLAUDE.md)");
    }
    if probe.has_history {
        v.push("历史回填(把老项目的历史周与历史版本补成同格式的正常文件)");
    }
    if v.is_empty() {
        "无 —— 这个仓既没有 agent 约定文件也没有历史,第 1 步就是全部".into()
    } else {
        v.join(";")
    }
}

/// ▶开工 时注入给 agent 的系统提示词。
///
/// 两段:这张活是什么 + 两条铁律,然后是这张活挂的 workflow 剧本正文
/// ([`workflow_body`] 从项目仓里读)。剧本读不到就如实少这一段,不编。
pub(crate) fn agent_system_prompt(issue: &Issue, workflow_body: Option<&str>) -> String {
    let mut prompt = base_prompt(issue);
    if let Some(body) = workflow_body {
        prompt.push_str("\n以下是这件活挂的 workflow 剧本正文,照它干:\n\n---\n\n");
        prompt.push_str(body);
    }
    prompt
}

/// 这张活挂的 workflow 剧本正文。正本是**项目仓里**的
/// `.claude/skills/<slug>/SKILL.md`——铺底时复制进去的那一份,不是 buddy 自己
/// 安装目录里那一份。读不到就返回 `None`,调用方如实少注入一段。
pub(crate) fn workflow_body(workspace: &std::path::Path, workflow: &str) -> Option<String> {
    let slug = super::ops::skill_slug(workflow)?;
    std::fs::read_to_string(workspace.join(format!(".claude/skills/{slug}/SKILL.md"))).ok()
}

fn base_prompt(issue: &Issue) -> String {
    format!(
        "你在 Builders' Workbench 管理的项目仓里干一件活。\n\
         \n活:#{} {}\n类别:{}\n用的 workflow:{}\n\
         \n先读仓根的 AGENTS.md,那是这个项目对 agent 的工作约定。\n\
         \n两条不许破的规矩:\n\
         1. 你干完最远只能把活推到「评审中」,「完成」永远由人点。\n\
         2. 指标读数只能来自真实采集,不许为了让灯变绿手工改数。\n",
        issue.number,
        issue.title,
        issue.category.map(|c| c.label()).unwrap_or("—(没定类别)"),
        if issue.workflow.is_empty() {
            "—(没挂 workflow)"
        } else {
            &issue.workflow
        },
    )
}
