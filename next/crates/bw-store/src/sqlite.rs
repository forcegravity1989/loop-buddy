//! SQLite implementation of [`IssueStore`] / [`RunStore`] (sqlx,
//! runtime-checked queries).
//!
//! Single-connection pool: a desktop app has one writer, so serializing
//! access sidesteps `SQLITE_BUSY` without ceremony (same shape as v1
//! `crates/bw-store/src/sqlite.rs`).

use crate::{
    parse_run_end_kind, parse_run_kind, run_end_kind_text, run_kind_text, IssueRow, IssueStore,
    NewIssue, NewProject, NewRun, ProjectRow, Result, RunEndKind, RunRow, RunStore, StoreError,
};
use async_trait::async_trait;
use bw_core::{IssueId, IssueStatus, ProjectId, RunId, RunState};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;
use uuid::Uuid;

const SCHEMA: &str = include_str!("schema.sql");

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (creating if missing) a SQLite database at `path` and apply the
    /// schema.
    pub async fn open(path: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        // Apply schema statement-by-statement. Strip `--` line comments first
        // so a `;` inside a comment can't split a statement mid-sentence
        // (same parsing shape as v1's `open()`).
        let cleaned: String = SCHEMA
            .lines()
            .map(|line| match line.find("--") {
                Some(i) => &line[..i],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n");
        for stmt in cleaned.split(';') {
            if stmt.trim().is_empty() {
                continue;
            }
            sqlx::query(stmt).execute(&pool).await?;
        }

        // 迁移双守卫的第一次真实调用留给切片五加列时(design-s4-runmanager.md
        // §2.4)。本片三张表的每一列都在上面的 `CREATE TABLE` 里一步到位,
        // 没有旧库列缺口需要补——`add_column_if_missing` 因此原样移植进来
        // (v1 `crates/bw-store/src/sqlite.rs` 约 410 行,零改写)但暂无调用
        // 点,见函数自己的文档注释;不为了演示而演示。

        Ok(Self { pool })
    }
}

/// 迁移双守卫(design §2.4,原样移植自 v1 `crates/bw-store/src/sqlite.rs`
/// 约 410 行,零改写):先 `PRAGMA table_info(表)` 查列在不在,不在才
/// `ALTER TABLE … ADD COLUMN`;列已存在即 no-op,可以在每次开库时安全重复
/// 调用。规矩不变:每加一列,必须同时改 `schema.sql` 并在开库流程里加一行
/// 这样的调用,否则存量库直接崩(本仓库踩过的真坑)。
///
/// 本片三张新表本身没有列需要迁移(`CREATE TABLE` 一步到位),第一次真实
/// 调用点留给切片五加列时——如实标注留白,不假装现在就用得上。
#[allow(dead_code)]
async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    ddl: &str,
) -> Result<()> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let exists = rows.iter().any(|r| r.get::<String, _>("name") == column);
    if !exists {
        sqlx::query(&format!("ALTER TABLE {table} ADD COLUMN {column} {ddl}"))
            .execute(pool)
            .await?;
    }
    Ok(())
}

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn parse_uuid<T>(s: &str, f: impl Fn(Uuid) -> T) -> Result<T> {
    Uuid::parse_str(s)
        .map(f)
        .map_err(|e| StoreError::Other(format!("bad uuid {s:?}: {e}")))
}

pub(crate) fn parse_issue_status(s: &str) -> Result<IssueStatus> {
    match s {
        "backlog" => Ok(IssueStatus::Backlog),
        "todo" => Ok(IssueStatus::Todo),
        "in_progress" => Ok(IssueStatus::InProgress),
        "in_review" => Ok(IssueStatus::InReview),
        "done" => Ok(IssueStatus::Done),
        "blocked" => Ok(IssueStatus::Blocked),
        "cancelled" => Ok(IssueStatus::Cancelled),
        other => Err(StoreError::Other(format!("bad issue.status {other:?}"))),
    }
}

#[async_trait]
impl IssueStore for SqliteStore {
    async fn create_project(&self, p: NewProject) -> Result<()> {
        sqlx::query("INSERT INTO project (id, name, root_path, created_at) VALUES (?, ?, ?, ?)")
            .bind(p.id.uuid().to_string())
            .bind(&p.name)
            .bind(&p.root_path)
            .bind(now_unix())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_project(&self, id: ProjectId) -> Result<Option<ProjectRow>> {
        let row = sqlx::query("SELECT id, name, root_path, created_at FROM project WHERE id = ?")
            .bind(id.uuid().to_string())
            .fetch_optional(&self.pool)
            .await?;
        let Some(r) = row else {
            return Ok(None);
        };
        Ok(Some(ProjectRow {
            id: parse_uuid(&r.get::<String, _>("id"), ProjectId::from_uuid)?,
            name: r.get("name"),
            root_path: r.get("root_path"),
            created_at: r.get("created_at"),
        }))
    }

    async fn create_issue(&self, i: NewIssue) -> Result<()> {
        let now = now_unix();
        sqlx::query(
            "INSERT INTO issue (id, project_id, number, title, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(i.id.uuid().to_string())
        .bind(i.project_id.uuid().to_string())
        .bind(i.number)
        .bind(&i.title)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_issue(&self, id: IssueId) -> Result<Option<IssueRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, number, title, status, settled_at, created_at, updated_at \
             FROM issue WHERE id = ?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else {
            return Ok(None);
        };
        let status_text: String = r.get("status");
        Ok(Some(IssueRow {
            id: parse_uuid(&r.get::<String, _>("id"), IssueId::from_uuid)?,
            project_id: parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid)?,
            number: r.get("number"),
            title: r.get("title"),
            status: parse_issue_status(&status_text)?,
            settled_at: r.get("settled_at"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    async fn settle_issue(&self, id: IssueId, at: i64) -> Result<()> {
        // COALESCE 保住第一次结算的时刻——原样移植 v1 `mark_issue_settled`
        // (v1 `crates/bw-store/src/sqlite.rs` 约 3049 行)。调用方不需要知
        // 道自己是不是第一个,恒 `Ok`;这是「结算一次」在 issue 侧的形态,
        // 与 run 侧的比较并置语义相同、形态不同(design §2.3)。
        sqlx::query("UPDATE issue SET settled_at = COALESCE(settled_at, ?) WHERE id = ?")
            .bind(at)
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[async_trait]
impl RunStore for SqliteStore {
    async fn create_run(&self, r: NewRun) -> Result<()> {
        // 先插行占名额(design §3.4)。撞
        // `uq_run_live_delivery_per_issue`(schema.sql)时,sqlx 把 SQLite 的
        // UNIQUE 约束冲突包成 `sqlx::Error::Database`,调用方用
        // `StoreError::is_unique_violation()` 分类。
        sqlx::query(
            "INSERT INTO run (id, project_id, issue_id, kind, connector_name, req_id, \
             workspace, branch, state, started_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(r.id.uuid().to_string())
        .bind(r.project_id.uuid().to_string())
        .bind(r.issue_id.uuid().to_string())
        .bind(run_kind_text(r.kind))
        .bind(&r.connector_name)
        .bind(&r.req_id)
        .bind(&r.workspace)
        .bind(&r.branch)
        .bind(r.state.as_str())
        .bind(r.started_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_run(&self, id: RunId) -> Result<Option<RunRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, issue_id, kind, connector_name, req_id, upstream_session, \
             workspace, branch, state, end_kind, end_detail, started_at, ended_at, settled_at, \
             demoted_at FROM run WHERE id = ?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else {
            return Ok(None);
        };
        let state_text: String = r.get("state");
        let state = RunState::parse(&state_text)
            .ok_or_else(|| StoreError::Other(format!("bad run.state {state_text:?}")))?;
        let end_kind: Option<String> = r.get("end_kind");
        let end_kind: Option<RunEndKind> = end_kind.map(|s| parse_run_end_kind(&s)).transpose()?;
        let kind_text: String = r.get("kind");
        Ok(Some(RunRow {
            id: parse_uuid(&r.get::<String, _>("id"), RunId::from_uuid)?,
            project_id: parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid)?,
            issue_id: parse_uuid(&r.get::<String, _>("issue_id"), IssueId::from_uuid)?,
            kind: parse_run_kind(&kind_text)?,
            connector_name: r.get("connector_name"),
            req_id: r.get("req_id"),
            upstream_session: r.get("upstream_session"),
            workspace: r.get("workspace"),
            branch: r.get("branch"),
            state,
            end_kind,
            end_detail: r.get("end_detail"),
            started_at: r.get("started_at"),
            ended_at: r.get("ended_at"),
            settled_at: r.get("settled_at"),
            demoted_at: r.get("demoted_at"),
        }))
    }

    async fn close_run(
        &self,
        id: RunId,
        ended_at: i64,
        state: RunState,
        end_kind: Option<RunEndKind>,
        end_detail: &str,
    ) -> Result<bool> {
        // 比较并置(design §2.3②):谓词是 `ended_at IS NULL`,不是普通
        // `WHERE id = ?` 那种谁写都成功的幂等更新。受影响行数就是「是不是
        // 第一个抵达」的信号——第二次到达的调用受影响 0 行,诚实空转。
        let outcome = sqlx::query(
            "UPDATE run SET ended_at = ?, state = ?, end_kind = ?, end_detail = ? \
             WHERE id = ? AND ended_at IS NULL",
        )
        .bind(ended_at)
        .bind(state.as_str())
        .bind(end_kind.map(run_end_kind_text))
        .bind(end_detail)
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn settle_run(&self, id: RunId, at: i64) -> Result<bool> {
        // 同一把比较并置机制,守的是「结算一次」而不是「关门一次」
        // (design §2.3②)。
        let outcome =
            sqlx::query("UPDATE run SET settled_at = ? WHERE id = ? AND settled_at IS NULL")
                .bind(at)
                .bind(id.uuid().to_string())
                .execute(&self.pool)
                .await?;
        Ok(outcome.rows_affected() == 1)
    }
}
