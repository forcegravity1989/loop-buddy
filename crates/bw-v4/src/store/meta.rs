//! `app_meta`(key/value)与 `claude_conversation`(活↔会话↔worktree↔分支)。
//!
//! 两张小表放一个文件:它们都只有「写一行、读一行」这点内容,各自单开一个
//! 文件只会多两层目录。

use super::{now_ts, Result, V4Store};
use crate::model::{Conversation, ConversationId, IssueId, ProjectId};
use sqlx::Row;
use uuid::Uuid;

/// 「工作区根目录」这条本机设置的 key。**本机设置存这里,不进仓** —— 仓是给
/// 所有人共用的正本,「我这台机器上项目放哪」只是我这台机器的事。
pub const WORKSPACES_ROOT_KEY: &str = "workspaces_root";

/// 通知屏「事件流看到哪个时间点」的 key 前缀。不为它开第五张表。
pub fn notify_seen_key(project_id: ProjectId) -> String {
    format!("notify_seen:{}", project_id.uuid())
}

impl V4Store {
    pub async fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_meta (key, value, updated_at) VALUES (?1,?2,?3) \
             ON CONFLICT(key) DO UPDATE SET value=?2, updated_at=?3",
        )
        .bind(key)
        .bind(value)
        .bind(now_ts())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    pub async fn meta(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM app_meta WHERE key = ?1")
            .bind(key)
            .fetch_optional(self.pool())
            .await?;
        Ok(row.map(|(v,)| v))
    }

    /// 一件活最多一个会话(`issue_id` 有 UNIQUE 约束);已经有了就返回既有行。
    pub async fn upsert_conversation(&self, c: &Conversation) -> Result<Conversation> {
        if let Some(existing) = self.conversation_for_issue(c.issue_id).await? {
            return Ok(existing);
        }
        let ts = now_ts();
        sqlx::query(
            "INSERT INTO claude_conversation (id, project_id, issue_id, claude_session_id, \
             workspace_path, branch_name, created_at, last_opened_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",
        )
        .bind(c.id.uuid().to_string())
        .bind(c.project_id.uuid().to_string())
        .bind(c.issue_id.uuid().to_string())
        .bind(&c.claude_session_id)
        .bind(&c.workspace_path)
        .bind(&c.branch_name)
        .bind(ts)
        .execute(self.pool())
        .await?;
        Ok(Conversation {
            created_at: ts,
            last_opened_at: ts,
            ..c.clone()
        })
    }

    pub async fn conversation_for_issue(&self, issue_id: IssueId) -> Result<Option<Conversation>> {
        let row = sqlx::query(
            "SELECT id, project_id, issue_id, claude_session_id, workspace_path, branch_name, \
             created_at, last_opened_at FROM claude_conversation WHERE issue_id = ?1",
        )
        .bind(issue_id.uuid().to_string())
        .fetch_optional(self.pool())
        .await?;
        let Some(row) = row else { return Ok(None) };
        let id: String = row.try_get("id")?;
        let pid: String = row.try_get("project_id")?;
        let iid: String = row.try_get("issue_id")?;
        Ok(Some(Conversation {
            id: ConversationId::from_uuid(Uuid::parse_str(&id).unwrap_or_default()),
            project_id: ProjectId::from_uuid(Uuid::parse_str(&pid).unwrap_or_default()),
            issue_id: IssueId::from_uuid(Uuid::parse_str(&iid).unwrap_or_default()),
            claude_session_id: row.try_get("claude_session_id")?,
            workspace_path: row.try_get("workspace_path")?,
            branch_name: row.try_get("branch_name")?,
            created_at: row.try_get("created_at")?,
            last_opened_at: row.try_get("last_opened_at")?,
        }))
    }

    /// 会话屏「按活列会话」的数据源。
    pub async fn conversations(&self, project_id: ProjectId) -> Result<Vec<Conversation>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT issue_id FROM claude_conversation WHERE project_id = ?1 \
             ORDER BY last_opened_at DESC",
        )
        .bind(project_id.uuid().to_string())
        .fetch_all(self.pool())
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for (iid,) in rows {
            let issue_id = IssueId::from_uuid(Uuid::parse_str(&iid).unwrap_or_default());
            if let Some(c) = self.conversation_for_issue(issue_id).await? {
                out.push(c);
            }
        }
        Ok(out)
    }

    /// 恢复会话时 claude CLI 回传的 `--resume` id。空 = 还没捕获到,如实留空。
    pub async fn set_conversation_session_id(
        &self,
        issue_id: IssueId,
        session_id: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE claude_conversation SET claude_session_id=?2, last_opened_at=?3 \
             WHERE issue_id=?1",
        )
        .bind(issue_id.uuid().to_string())
        .bind(session_id)
        .bind(now_ts())
        .execute(self.pool())
        .await?;
        Ok(())
    }
}
