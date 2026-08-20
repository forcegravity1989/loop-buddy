//! 接入项目、改名片、配项目群。
//!
//! 名片四字段(名称 / 想做什么 / 最像的对标 / 北极星)的正本是 `.bw/PROJECT.md`
//! 与 `.bw/project.toml`,**不落库**——`project` 表只有定位与显示缓存。所以
//! 改名片就是改仓文件,得走一张轻量活与一次 MR,不是直接 UPDATE 一行。

use super::{App, AppError, ProgressLine, Result};
use crate::command::{Event, ProjectIntent, RemoteRef};
use crate::model::{IssueKind, IssueOrigin, Project, ProjectId};
use crate::repo::project_file::{self, ChatConfig, ProjectFile};
use crate::standard;

impl App {
    pub(super) async fn create_project(
        &mut self,
        slug: String,
        intent: ProjectIntent,
        remote: RemoteRef,
        workspace_path: String,
    ) -> Result<Vec<Event>> {
        // 幂等:同名项目已存在就原样返回,不建第二行。
        if let Some(existing) = self.store.project_by_slug(&slug).await? {
            self.step(ProgressLine::ok(
                1,
                format!("库里已经有 {slug} 这个项目了,没重复建"),
            ));
            return Ok(vec![Event::ProjectCreated {
                id: existing.id,
                slug,
                adopted: true,
            }]);
        }

        // **先把仓弄到手,再建项目行。** 反过来的话,clone 失败会在库里留下
        // 一个指向空目录的项目 —— 而 V4 没有「删项目」这条命令,那一行就永远
        // 赖在项目墙上了。
        let ws = self.workspace_at(&slug, &workspace_path);
        self.fetch_repo(&ws, &remote).await?;

        let id = ProjectId::new();
        let row = Project {
            id,
            slug: slug.clone(),
            name: intent.name.clone(),
            workspace_path: workspace_path.clone(),
            provider: remote.provider.clone(),
            remote_host: remote.host.clone(),
            remote_path: remote.path.clone(),
            signal: None,
            weekly_signal: None,
            signal_derived_at: None,
            sort_order: 0.0,
            created_at: 0,
            updated_at: 0,
        };
        let row = self.store.upsert_project(&row).await?;
        self.step(ProgressLine::ok(3, format!("项目 {slug} 已落库")));

        // 意图落仓文件。
        self.step(ProgressLine::doing(4, "读仓里的名片(.bw/project.toml)…"));
        // **仓里已经有 `.bw/project.toml` 就以它为准**(后来者接入:同事先接过
        // 这个项目,或者开发期删库重建之后重新接入)。人手填过的东西一个字都
        // 不覆盖,只补空着的字段;读不出来(文件坏了)也不覆盖,如实往上报。
        let existing = project_file::read(&ws)?;
        let adopted = existing.is_some();
        let mut file = existing.unwrap_or_else(|| ProjectFile {
            standard_version: standard::version().to_string(),
            // 新建项目默认在研版本 v0.1。
            current_version: "v0.1".into(),
            ..Default::default()
        });
        fill_if_blank(&mut file.name, &intent.name);
        fill_if_blank(&mut file.brief, &intent.brief);
        fill_if_blank(&mut file.benchmark, &intent.benchmark);
        fill_if_blank(&mut file.opportunity, &intent.north_star);
        project_file::write(&ws, &file)?;
        self.step(ProgressLine::ok(
            4,
            if adopted {
                "仓里本来就有名片,一个字没覆盖,只补了空着的字段".to_string()
            } else {
                "名片写好了,下一步铺底会把它提交到这张活的分支上".to_string()
            },
        ));

        // 第 0 站的后半截:**接入完自动建那张一次性运作活③「规范铺底」**
        // (母文档 §2 第 0 站)。它建分支、往仓里写全套规范骨架、开一个 MR,
        // **停在评审中等人合** —— 不自动合、不自动完成,一条铁律都没松。
        //
        // 铺底没成**不推翻接入**:项目已经建好了、仓已经在本机了,这两件事是
        // 真的。铺底失败(没登录、推不上去)如实报一行,人回头在配置屏点那颗
        // 按钮重来一次就是了 —— 它按标题幂等,重跑不会多建一张活。
        let mut events = vec![Event::ProjectCreated {
            id: row.id,
            slug,
            adopted,
        }];
        match self.run_standard_bootstrap(row.id, false).await {
            Ok(more) => events.extend(more),
            Err(e) => self.step(ProgressLine::fail(
                5,
                format!("规范铺底没成:{e} —— 项目已经接进来了,回头在配置屏点「规范铺底」重来一次"),
            )),
        }
        Ok(events)
    }

    /// 把仓弄到本机。接入这一步真正花时间的就是它。
    ///
    /// 四种局面,每种都如实说清楚,**一种都不许静悄悄地当成成功**:
    ///
    /// - 目录里已经是个 git 仓 → 直接用,顺带把它的 origin 报出来给人核对
    /// - 目录不在或是空的 + 填了远端 → `gh repo clone` / codehub clone
    /// - 目录不在或是空的 + 没填远端 → 建个空目录,并说明这是个空仓
    /// - 目录里有东西、又不是 git 仓 → **弹回**,既不敢往里 clone,也不敢
    ///   把别人的目录当成这个项目的仓
    async fn fetch_repo(&self, ws: &std::path::Path, remote: &RemoteRef) -> Result<()> {
        let shown = ws.display().to_string();
        if ws.join(".git").exists() {
            let origin = bw_engine::github::origin_remote_url(ws)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "没配 origin".into());
            self.step(ProgressLine::ok(
                2,
                format!("本机已经有这个仓,直接用:{shown}(origin = {origin})"),
            ));
            return Ok(());
        }

        let occupied = std::fs::read_dir(ws)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
        if occupied {
            let why = format!(
                "{shown} 里已经有东西了,又不是个 git 仓 —— 不敢往里 clone,也不敢当成这个项目的仓。换个路径,或者先把它清空。"
            );
            self.step(ProgressLine::fail(2, why.clone()));
            return Err(AppError::Refused(why));
        }

        if remote.path.trim().is_empty() {
            std::fs::create_dir_all(ws).map_err(|e| crate::repo::RepoFileError::Io {
                path: shown.clone(),
                source: e,
            })?;
            self.step(ProgressLine::ok(
                2,
                format!("没填远端地址,先建了个空目录当仓:{shown}"),
            ));
            return Ok(());
        }

        self.step(ProgressLine::doing(
            2,
            format!("把 {} clone 到 {shown}…", remote.path),
        ));
        // clone 自己会建目录。先建出来的话 gh 会因为「目标目录已存在」拒绝。
        if let Some(parent) = ws.parent() {
            std::fs::create_dir_all(parent).map_err(|e| crate::repo::RepoFileError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        let _ = std::fs::remove_dir(ws); // 空目录才删得掉,正合适
        let got = if remote.provider == "codehub" {
            bw_engine::codehub::clone_repo(&remote.host, &remote.path, ws)
                .await
                .map_err(|e| e.to_string())
        } else {
            match remote.path.split_once('/') {
                Some((owner, repo)) => bw_engine::github::clone_repo(owner, repo, ws)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                None => Err(format!(
                    "「{}」不是 owner/repo 的样子,clone 不了",
                    remote.path
                )),
            }
        };
        match got {
            Ok(()) => {
                let head = crate::git::head_sha(ws)
                    .await
                    .map(|h| h.chars().take(7).collect::<String>())
                    .unwrap_or_default();
                self.step(ProgressLine::ok(
                    2,
                    if head.is_empty() {
                        format!("clone 好了:{shown}")
                    } else {
                        format!("clone 好了:{shown}(当前提交 {head})")
                    },
                ));
                Ok(())
            }
            Err(e) => {
                let why = format!("clone 没成:{e}");
                self.step(ProgressLine::fail(2, why.clone()));
                Err(AppError::Refused(why))
            }
        }
    }

    /// 把一个项目从工作台上移走。
    ///
    /// **只动库,不动仓**:仓里是真实的劳动成果,活自己的 worktree 也是 ——
    /// 「我不想在工作台上看见它了」不构成删掉它们的授权。回执里如实报出仓
    /// 还在哪儿,人真要删自己去删。
    pub(super) async fn remove_project(&mut self, project_id: ProjectId) -> Result<Vec<Event>> {
        let project = self
            .store
            .project(project_id)
            .await?
            .ok_or_else(|| AppError::NoSuchProject(project_id.uuid().to_string()))?;
        let ws = self.workspace_of(project_id).await?;
        let issues = self.store.delete_project(project_id).await?;
        Ok(vec![Event::ProjectRemoved {
            slug: project.slug,
            issues,
            workspace: ws.display().to_string(),
        }])
    }

    pub(super) async fn edit_project_card(
        &mut self,
        project_id: ProjectId,
        intent: ProjectIntent,
    ) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let mut file = project_file::read(&ws)?.unwrap_or_default();
        file.name = intent.name.clone();
        file.brief = intent.brief.clone();
        file.benchmark = intent.benchmark.clone();
        file.opportunity = intent.north_star.clone();
        project_file::write(&ws, &file)?;

        // 章程也要跟着改 —— PROJECT.md 才是给人读的那一份。
        let body = standard::render(
            standard::CHARTER_TMPL,
            &[
                ("name", &intent.name),
                ("brief", &intent.brief),
                ("benchmark", &intent.benchmark),
                ("north_star", &intent.north_star),
                ("current_version", &file.current_version),
                ("chat", &chat_label(&file.chat)),
            ],
        );
        crate::repo::write_file(&ws, standard::CHARTER_REL_PATH, &body)?;

        // 改仓的动作都走 MR:建一张轻量活背这次改动。
        let events = self
            .create_issue(
                project_id,
                format!("编辑项目名片 · {}", intent.name),
                "名片四字段改动:名称 / 想做什么 / 最像的对标 / 北极星。改的是 \
                 .bw/PROJECT.md 与 .bw/project.toml 两份仓文件,等人评审合入。"
                    .into(),
                None,
                IssueKind::Light,
                IssueOrigin::Human,
                String::new(),
            )
            .await?;
        let issue_id = events
            .iter()
            .find_map(|e| match e {
                Event::IssueCreated { id, .. } => Some(*id),
                _ => None,
            })
            .ok_or_else(|| AppError::Refused("名片编辑的轻量活没建出来".into()))?;

        // 库里的名字是项目墙的显示缓存,跟着改一次。
        let p = self
            .store
            .project(project_id)
            .await?
            .ok_or_else(|| AppError::NoSuchProject(project_id.uuid().to_string()))?;
        self.store
            .update_project_location(
                project_id,
                &intent.name,
                &p.workspace_path,
                &p.provider,
                &p.remote_host,
                &p.remote_path,
            )
            .await?;

        Ok(vec![Event::ProjectCardEditPending { issue_id }])
    }

    pub(super) async fn set_project_chat(
        &mut self,
        project_id: ProjectId,
        provider: String,
        group_id: String,
        notify: Vec<String>,
    ) -> Result<Vec<Event>> {
        let ws = self.workspace_of(project_id).await?;
        let mut file = project_file::read(&ws)?.unwrap_or_default();
        // "none" = 明确不配群:整段不写,而不是写一个空 provider 假装配过。
        file.chat = if provider == "none" || provider.is_empty() {
            None
        } else {
            Some(ChatConfig {
                provider,
                group_id,
                // 命令带过来的名单原样写回。空数组 = 静音,是一个明确的选择,
                // 不折成「没写」。
                notify: Some(notify),
            })
        };
        project_file::write(&ws, &file)?;
        Ok(vec![Event::ProjectChatChanged { project_id }])
    }

    pub(super) async fn mark_notify_seen(
        &mut self,
        project_id: ProjectId,
        at: i64,
    ) -> Result<Vec<Event>> {
        self.store
            .set_meta(&crate::store::notify_seen_key(project_id), &at.to_string())
            .await?;
        Ok(vec![Event::NotifySeenMarked { project_id }])
    }
}

/// 名片上「项目群」那一行怎么写。没配就如实说没配。
pub(super) fn chat_label(chat: &Option<ChatConfig>) -> String {
    match chat {
        None => "未配".into(),
        Some(c) => format!("{}(群号 {})", c.provider, c.group_id),
    }
}

/// 只在原来是空的时候填 —— 人手写过的一个字都不动。
fn fill_if_blank(slot: &mut String, value: &str) {
    if slot.trim().is_empty() {
        *slot = value.to_string();
    }
}
