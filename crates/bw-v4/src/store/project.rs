//! `project` 表:项目定位 + 项目墙显示缓存。

use super::{enum_from_db, enum_to_db, now_ts, Result, V4Store};
use crate::derive::DerivedHealth;
use crate::model::{Project, ProjectId, Signal};
use sqlx::Row;
use uuid::Uuid;

/// 一行 `project` 的全部列,读侧只写一次。
const COLS: &str = "id, slug, name, workspace_path, provider, remote_host, remote_path, \
                    signal, weekly_signal, signal_derived_at, sort_order, created_at, updated_at";

fn row_to_project(row: &sqlx::sqlite::SqliteRow) -> Result<Project> {
    let signal_str: Option<String> = row.try_get("signal")?;
    let weekly_str: Option<String> = row.try_get("weekly_signal")?;
    let parse = |s: Option<String>| -> Result<Option<Signal>> {
        match s {
            None => Ok(None),
            Some(v) => Ok(Some(enum_from_db::<Signal>("project.signal", &v)?)),
        }
    };
    let id: String = row.try_get("id")?;
    Ok(Project {
        id: ProjectId::from_uuid(Uuid::parse_str(&id).unwrap_or_default()),
        slug: row.try_get("slug")?,
        name: row.try_get("name")?,
        workspace_path: row.try_get("workspace_path")?,
        provider: row.try_get("provider")?,
        remote_host: row.try_get("remote_host")?,
        remote_path: row.try_get("remote_path")?,
        signal: parse(signal_str)?,
        weekly_signal: parse(weekly_str)?,
        signal_derived_at: row.try_get("signal_derived_at")?,
        sort_order: row.try_get("sort_order")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

impl V4Store {
    /// 按 slug 建项目;已存在同 slug 就原样返回既有行(幂等,指挥器重跑靠它)。
    pub async fn upsert_project(&self, p: &Project) -> Result<Project> {
        if let Some(existing) = self.project_by_slug(&p.slug).await? {
            return Ok(existing);
        }
        let ts = now_ts();
        sqlx::query(
            "INSERT INTO project (id, slug, name, workspace_path, provider, remote_host, \
             remote_path, signal, weekly_signal, signal_derived_at, sort_order, created_at, updated_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,NULL,NULL,NULL,?8,?9,?9)",
        )
        .bind(p.id.uuid().to_string())
        .bind(&p.slug)
        .bind(&p.name)
        .bind(&p.workspace_path)
        .bind(&p.provider)
        .bind(&p.remote_host)
        .bind(&p.remote_path)
        .bind(p.sort_order)
        .bind(ts)
        .execute(self.pool())
        .await?;
        Ok(Project {
            created_at: ts,
            updated_at: ts,
            signal: None,
            weekly_signal: None,
            signal_derived_at: None,
            ..p.clone()
        })
    }

    /// 把某个项目的仓路径钉死成一个绝对路径。
    ///
    /// 只有一个调用方:改工作区根目录之前,把那些「没填过仓路径、一直靠根目录
    /// 现拼」的老项目钉在原位。**`upsert_project` 干不了这件事** —— 它是「没有
    /// 才插入,有了原样返回」,一个字段都不改。
    pub async fn set_project_workspace_path(&self, id: ProjectId, path: &str) -> Result<()> {
        sqlx::query("UPDATE project SET workspace_path = ?2, updated_at = ?3 WHERE id = ?1")
            .bind(id.uuid().to_string())
            .bind(path)
            .bind(now_ts())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn project_by_slug(&self, slug: &str) -> Result<Option<Project>> {
        let sql = format!("SELECT {COLS} FROM project WHERE slug = ?1");
        let row = sqlx::query(&sql)
            .bind(slug)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_project).transpose()
    }

    pub async fn project(&self, id: ProjectId) -> Result<Option<Project>> {
        let sql = format!("SELECT {COLS} FROM project WHERE id = ?1");
        let row = sqlx::query(&sql)
            .bind(id.uuid().to_string())
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_project).transpose()
    }

    /// 项目墙的一整屏 —— 按 `sort_order` 再按建立时间。
    pub async fn projects(&self) -> Result<Vec<Project>> {
        let sql = format!("SELECT {COLS} FROM project ORDER BY sort_order, created_at");
        let rows = sqlx::query(&sql).fetch_all(self.pool()).await?;
        rows.iter().map(row_to_project).collect()
    }

    /// 改项目的定位字段(接入两卡、设置工作区)。名片字段不在这里——它们
    /// 住 `PROJECT.md`。
    pub async fn update_project_location(
        &self,
        id: ProjectId,
        name: &str,
        workspace_path: &str,
        provider: &str,
        remote_host: &str,
        remote_path: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE project SET name=?2, workspace_path=?3, provider=?4, remote_host=?5, \
             remote_path=?6, updated_at=?7 WHERE id=?1",
        )
        .bind(id.uuid().to_string())
        .bind(name)
        .bind(workspace_path)
        .bind(provider)
        .bind(remote_host)
        .bind(remote_path)
        .bind(now_ts())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// 把现算出来的健康灯写进显示缓存。
    ///
    /// 参数类型是密封的 [`DerivedHealth`]——它只能由
    /// [`crate::derive::derive_project_health`] 从真实判据造出来,所以这个
    /// 方法在类型上就无法被用来「手工点绿一个项目」。没数据时灯是
    /// `Signal::Unknown`,照实写进去,界面显示灰。
    pub async fn cache_project_health(&self, id: ProjectId, health: &DerivedHealth) -> Result<()> {
        sqlx::query(
            "UPDATE project SET signal=?2, weekly_signal=?3, signal_derived_at=?4, updated_at=?4 \
             WHERE id=?1",
        )
        .bind(id.uuid().to_string())
        .bind(enum_to_db(&health.signal()))
        .bind(enum_to_db(&health.weekly_signal()))
        .bind(now_ts())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// 把一个项目从工作台上移走:项目行、它的活、它的会话身份,三张表一起删。
    ///
    /// **只动库,绝不动仓**。仓是正本,里面是真实的劳动成果 —— 「我不想在工作
    /// 台上看见它了」不构成删掉一个仓的授权。活自己的那些 worktree 同理留着。
    ///
    /// 回删掉了几张活,给回执用 —— 人得知道自己刚才丢掉了多少账。
    pub async fn delete_project(&self, id: ProjectId) -> Result<u64> {
        let key = id.uuid().to_string();
        let issues = sqlx::query("DELETE FROM issue WHERE project_id=?1")
            .bind(&key)
            .execute(self.pool())
            .await?
            .rows_affected();
        sqlx::query("DELETE FROM claude_conversation WHERE project_id=?1")
            .bind(&key)
            .execute(self.pool())
            .await?;
        sqlx::query("DELETE FROM project WHERE id=?1")
            .bind(&key)
            .execute(self.pool())
            .await?;
        Ok(issues)
    }
}
