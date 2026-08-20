//! `issue` 表:远端 issue 的本机缓存 + 九个扩展列。
//!
//! 这一层不判断状态转移合不合法(那是 `crate::app` 拿 `can_transition_to`
//! 守的),也不判断该不该结清——只负责写。唯一一条在这里的纪律是
//! [`V4Store::settle_issue`] 的短路:`settled_at` 已经非空就不再写第二次,
//! 同一件活绝不结两次。

use super::{enum_from_db, enum_to_db, now_ts, Result, V4Store};
use crate::model::{category_from_key, category_key, Issue, IssueKind, IssueOrigin, IssueStatus};
use crate::model::{IssueId, ProjectId};
use sqlx::Row;
use uuid::Uuid;

const COLS: &str = "id, project_id, number, remote_number, title, body, status, branch, \
                    pr_number, week_of, version, tool, kind, origin, workflow, category, \
                    sort_order, metric_key, created_at, updated_at, settled_at";

fn row_to_issue(row: &sqlx::sqlite::SqliteRow) -> Result<Issue> {
    let id: String = row.try_get("id")?;
    let pid: String = row.try_get("project_id")?;
    let status: String = row.try_get("status")?;
    let kind: String = row.try_get("kind")?;
    let origin: String = row.try_get("origin")?;
    let category: String = row.try_get("category")?;
    Ok(Issue {
        id: IssueId::from_uuid(Uuid::parse_str(&id).unwrap_or_default()),
        project_id: ProjectId::from_uuid(Uuid::parse_str(&pid).unwrap_or_default()),
        number: row.try_get::<i64, _>("number")? as u32,
        remote_number: row.try_get::<i64, _>("remote_number")? as u32,
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        status: enum_from_db::<IssueStatus>("issue.status", &status)?,
        branch: row.try_get("branch")?,
        pr_number: row.try_get::<i64, _>("pr_number")? as u32,
        week_of: row.try_get("week_of")?,
        version: row.try_get("version")?,
        tool: row.try_get("tool")?,
        kind: enum_from_db::<IssueKind>("issue.kind", &kind)?,
        origin: enum_from_db::<IssueOrigin>("issue.origin", &origin)?,
        workflow: row.try_get("workflow")?,
        category: category_from_key(&category),
        sort_order: row.try_get("sort_order")?,
        metric_key: row.try_get("metric_key")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        settled_at: row.try_get("settled_at")?,
    })
}

impl V4Store {
    /// 项目内的下一个本机活号。没挂远端时活也得有号可引用(周计划文件的
    /// 活清单、发版记录的「包含的活」列都写这个号)。
    pub async fn next_issue_number(&self, project_id: ProjectId) -> Result<u32> {
        let row: (Option<i64>,) =
            sqlx::query_as("SELECT MAX(number) FROM issue WHERE project_id = ?1")
                .bind(project_id.uuid().to_string())
                .fetch_one(self.pool())
                .await?;
        Ok(row.0.unwrap_or(0) as u32 + 1)
    }

    /// 建活。调用方负责先算好号(`next_issue_number`)与排序值。
    pub async fn insert_issue(&self, i: &Issue) -> Result<Issue> {
        let ts = now_ts();
        sqlx::query(
            "INSERT INTO issue (id, project_id, number, remote_number, title, body, status, \
             branch, pr_number, week_of, version, tool, kind, origin, workflow, category, \
             sort_order, metric_key, created_at, updated_at, settled_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?19,NULL)",
        )
        .bind(i.id.uuid().to_string())
        .bind(i.project_id.uuid().to_string())
        .bind(i.number as i64)
        .bind(i.remote_number as i64)
        .bind(&i.title)
        .bind(&i.body)
        .bind(enum_to_db(&i.status))
        .bind(&i.branch)
        .bind(i.pr_number as i64)
        .bind(&i.week_of)
        .bind(&i.version)
        .bind(&i.tool)
        .bind(enum_to_db(&i.kind))
        .bind(enum_to_db(&i.origin))
        .bind(&i.workflow)
        .bind(i.category.map(category_key).unwrap_or(""))
        .bind(i.sort_order)
        .bind(&i.metric_key)
        .bind(ts)
        .execute(self.pool())
        .await?;
        Ok(Issue {
            created_at: ts,
            updated_at: ts,
            settled_at: None,
            ..i.clone()
        })
    }

    pub async fn issue(&self, id: IssueId) -> Result<Option<Issue>> {
        let sql = format!("SELECT {COLS} FROM issue WHERE id = ?1");
        let row = sqlx::query(&sql)
            .bind(id.uuid().to_string())
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_issue).transpose()
    }

    /// 按标题找活 —— 指挥器的幂等键(重跑不产生重复数据)。
    pub async fn issue_by_title(
        &self,
        project_id: ProjectId,
        title: &str,
    ) -> Result<Option<Issue>> {
        let sql = format!("SELECT {COLS} FROM issue WHERE project_id = ?1 AND title = ?2");
        let row = sqlx::query(&sql)
            .bind(project_id.uuid().to_string())
            .bind(title)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_issue).transpose()
    }

    pub async fn issues(&self, project_id: ProjectId) -> Result<Vec<Issue>> {
        let sql = format!(
            "SELECT {COLS} FROM issue WHERE project_id = ?1 ORDER BY sort_order, created_at"
        );
        let rows = sqlx::query(&sql)
            .bind(project_id.uuid().to_string())
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_issue).collect()
    }

    /// 某一周的活。`week` 传空串就是待办池那一列。
    pub async fn issues_in_week(&self, project_id: ProjectId, week: &str) -> Result<Vec<Issue>> {
        let sql = format!(
            "SELECT {COLS} FROM issue WHERE project_id = ?1 AND week_of = ?2 \
             ORDER BY sort_order, created_at"
        );
        let rows = sqlx::query(&sql)
            .bind(project_id.uuid().to_string())
            .bind(week)
            .fetch_all(self.pool())
            .await?;
        rows.iter().map(row_to_issue).collect()
    }

    /// 该人看一眼、而且人还没看过的活有几张:评审中或阻塞,且更新时间晚于
    /// 「读到这里」那一下。**在库里数**,不要把整张 issue 表取回内存再 filter
    /// ——项目墙上每个项目每次重拼界面都要这个数。
    pub async fn count_unseen(&self, project_id: ProjectId, seen_at: i64) -> Result<u32> {
        // 口径和通知屏那一类**必须一样**:评审中 + 真有 MR。两边各写一套的
        // 话,红点上写着 3 而列表里只有 1,人会以为界面坏了。
        //
        // 状态值走 `enum_to_db`,不在 SQL 里写死字面量 —— 写死的话哪天枚举的
        // serde 名字改了,这条查询会静默返回 0,而不是报错。
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM issue WHERE project_id = ?1 \
             AND status = ?2 AND pr_number > 0 AND updated_at > ?3",
        )
        .bind(project_id.uuid().to_string())
        .bind(enum_to_db(&IssueStatus::InReview))
        .bind(seen_at)
        .fetch_one(self.pool())
        .await?;
        Ok(row.0 as u32)
    }

    /// 只改状态。**不校验合不合法** —— 那是 `crate::app` 的事。
    pub async fn set_issue_status(&self, id: IssueId, status: IssueStatus) -> Result<()> {
        sqlx::query("UPDATE issue SET status=?2, updated_at=?3 WHERE id=?1")
            .bind(id.uuid().to_string())
            .bind(enum_to_db(&status))
            .bind(now_ts())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// 结清一件活(人点了「完成」的那一刻)。
    ///
    /// 已经结过就直接返回 `false`,不写第二次——**同一件活绝不记两次**。
    /// 这条短路在 SQL 的 `WHERE settled_at IS NULL` 里,不靠调用方自觉。
    pub async fn settle_issue(&self, id: IssueId) -> Result<bool> {
        let ts = now_ts();
        let res = sqlx::query(
            "UPDATE issue SET settled_at=?2, updated_at=?2 WHERE id=?1 AND settled_at IS NULL",
        )
        .bind(id.uuid().to_string())
        .bind(ts)
        .execute(self.pool())
        .await?;
        Ok(res.rows_affected() == 1)
    }

    /// 排期:设(或清空)`week_of`,同时给一个同列内的排序值。
    pub async fn set_issue_week(&self, id: IssueId, week_of: &str, sort_order: f64) -> Result<()> {
        sqlx::query("UPDATE issue SET week_of=?2, sort_order=?3, updated_at=?4 WHERE id=?1")
            .bind(id.uuid().to_string())
            .bind(week_of)
            .bind(sort_order)
            .bind(now_ts())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn set_issue_sort_order(&self, id: IssueId, sort_order: f64) -> Result<()> {
        sqlx::query("UPDATE issue SET sort_order=?2, updated_at=?3 WHERE id=?1")
            .bind(id.uuid().to_string())
            .bind(sort_order)
            .bind(now_ts())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// 改「这张活用哪个工具、哪个 workflow、挂哪个指标」。
    pub async fn set_issue_dispatch(
        &self,
        id: IssueId,
        tool: &str,
        workflow: &str,
        metric_key: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE issue SET tool=?2, workflow=?3, metric_key=?4, updated_at=?5 WHERE id=?1",
        )
        .bind(id.uuid().to_string())
        .bind(tool)
        .bind(workflow)
        .bind(metric_key)
        .bind(now_ts())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// 改活的说明正文(阻塞原因、草稿补充这类)。
    pub async fn set_issue_body(&self, id: IssueId, body: &str) -> Result<()> {
        sqlx::query("UPDATE issue SET body=?2, updated_at=?3 WHERE id=?1")
            .bind(id.uuid().to_string())
            .bind(body)
            .bind(now_ts())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// 给活挂版本标签(发版本那一刻写)。
    pub async fn set_issue_version(&self, id: IssueId, version: &str) -> Result<()> {
        sqlx::query("UPDATE issue SET version=?2, updated_at=?3 WHERE id=?1")
            .bind(id.uuid().to_string())
            .bind(version)
            .bind(now_ts())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// 记下这张活的分支与 MR 号。`0` = 没有,绝不编造。
    pub async fn set_issue_remote(
        &self,
        id: IssueId,
        branch: &str,
        pr_number: u32,
        remote_number: u32,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE issue SET branch=?2, pr_number=?3, remote_number=?4, updated_at=?5 WHERE id=?1",
        )
        .bind(id.uuid().to_string())
        .bind(branch)
        .bind(pr_number as i64)
        .bind(remote_number as i64)
        .bind(now_ts())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// 配置屏「用过几次」——现算,没有战绩表可查。
    pub async fn workflow_usage(&self, project_id: ProjectId) -> Result<Vec<(String, u32)>> {
        let rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT workflow, COUNT(*) FROM issue \
             WHERE project_id = ?1 AND kind = 'business' AND workflow != '' \
             GROUP BY workflow ORDER BY COUNT(*) DESC, workflow",
        )
        .bind(project_id.uuid().to_string())
        .fetch_all(self.pool())
        .await?;
        Ok(rows.into_iter().map(|(w, n)| (w, n as u32)).collect())
    }
}
