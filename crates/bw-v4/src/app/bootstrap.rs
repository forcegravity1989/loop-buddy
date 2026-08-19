//! 规范铺底(运作活③)与规范对账。
//!
//! A 刀只做**第 1 步**:buddy 自己把核心件写进项目仓、复制预置技能包、记指纹,
//! 然后提交。第 2 步「合并调整」与第 3 步「历史回填」要起 agent 会话,留给后
//! 面的刀——这张活的标题会如实带上「· 含合并调整」「· 含历史回填」后缀,说明
//! 探测到了什么,但那两步还没跑。

use super::{App, AppError, Result};
use crate::command::Event;
use crate::model::{Issue, IssueKind, IssueOrigin, ProjectId};
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

        let events = self
            .create_issue(
                project_id,
                title,
                body.clone(),
                None,
                IssueKind::Ops,
                IssueOrigin::Auto,
                String::new(),
            )
            .await?;
        let issue_id = events
            .iter()
            .find_map(|e| match e {
                Event::IssueCreated { id, .. } => Some(*id),
                _ => None,
            })
            .ok_or_else(|| AppError::Refused("铺底的运作活没建出来".into()))?;

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
        let report = boot::write_core_files(&ws, &vars)?;

        // 空仓例外(仓是 buddy 自己建的)直接提交当前分支;否则也提交在当前
        // 分支上,开分支与 MR 是后面刀的事——这里如实回报提交没提交。
        // 只提交这次真写下去的那些件。用户点铺底的时候工作区多半是脏的,
        // 把他手上没写完的改动一起打包进「规范铺底」那个提交是不能接受的。
        let outcome = crate::git::commit_paths(
            &ws,
            &report.written,
            &format!("docs(bw): 规范铺底 v{} · 核心件", standard::version()),
        )
        .await
        .unwrap_or_default();
        let committed = outcome.committed;

        // 跳过的件如实写进这张活的说明,评审的人不用猜为什么少了一份。
        // 注意是**从头拼一遍再整体覆盖**,不是往现有正文后面追加 —— 建活是按
        // 标题幂等的,重跑会拿到同一张活,追加就会把「跳过的件」滚成两份。
        let mut b = body;
        if !report.skipped.is_empty() {
            b.push_str("\n\n跳过的件:\n");
            for (path, why) in &report.skipped {
                b.push_str(&format!("- `{path}`:{why}\n"));
            }
        }
        if !outcome.refused.is_empty() {
            b.push_str(
                "\n\n写下去了但**没进版本控制**的件(这个仓的 .gitignore 忽略了它们,\
                 buddy 不用 -f 顶回去 —— 那是项目自己的决定):\n",
            );
            for path in &outcome.refused {
                b.push_str(&format!("- `{path}`\n"));
            }
        }
        self.store.set_issue_body(issue_id, &b).await?;

        Ok(vec![Event::StandardBootstrapped {
            project_id,
            issue_id,
            files: report.written,
            committed,
        }])
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
