//! 接入项目、改名片、配项目群。
//!
//! 名片四字段(名称 / 想做什么 / 最像的对标 / 北极星)的正本是 `PROJECT.md`
//! 与 `.bw/project.toml`,**不落库**——`project` 表只有定位与显示缓存。所以
//! 改名片就是改仓文件,得走一张轻量活与一次 MR,不是直接 UPDATE 一行。

use super::{App, AppError, Result};
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
            return Ok(vec![Event::ProjectCreated {
                id: existing.id,
                slug,
                adopted: true,
            }]);
        }

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

        // 意图落仓文件。仓还不存在就先把目录建出来——`.bw/project.toml` 是
        // 接入之后一切的起点,没有它总览什么都读不到。
        let ws = self.workspace_of(id).await?;
        std::fs::create_dir_all(&ws).map_err(|e| crate::repo::RepoFileError::Io {
            path: ws.display().to_string(),
            source: e,
        })?;
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

        Ok(vec![Event::ProjectCreated {
            id: row.id,
            slug,
            adopted,
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
        crate::repo::write_file(&ws, "PROJECT.md", &body)?;

        // 改仓的动作都走 MR:建一张轻量活背这次改动。
        let events = self
            .create_issue(
                project_id,
                format!("编辑项目名片 · {}", intent.name),
                "名片四字段改动:名称 / 想做什么 / 最像的对标 / 北极星。改的是 \
                 PROJECT.md 与 .bw/project.toml 两份仓文件,等人评审合入。"
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
