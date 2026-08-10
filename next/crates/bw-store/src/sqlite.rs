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

        // 迁移双守卫的第一次真实调用(next 切片四D,design §2.4/§5.2 第
        // 9 条)。**这不是本次新加的列**——`run.demoted_at` 本来就在上面
        // 的 `CREATE TABLE`(四A 一步到位)里,不是「这次迁移新增的字
        // 段」。挑它当第一次真实调用点,是因为它是这张表里唯一「可空、
        // 无 NOT NULL 约束、ALTER TABLE ADD COLUMN 补起来最干净」的一
        // 列,适合把这条此前 `#[allow(dead_code)]` 的守卫真正接进开库流
        // 程,作为往后任何一次真实加列的活样板。对当下的每一个真实数据
        // 库(不管是不是这个字段的老版本)都安全:字段已存在就是
        // no-op,不存在才补——`bw-app` `examples/run_races.rs`「附 · 存量
        // 库迁移双守卫」一节会真的造一个缺这一列的老库、走这条开库流
        // 程,读回验证补列生效。
        add_column_if_missing(&pool, "run", "demoted_at", "INTEGER").await?;

        Ok(Self { pool })
    }
}

/// 迁移双守卫(design §2.4,原样移植自 v1 `crates/bw-store/src/sqlite.rs`
/// 约 410 行,零改写):先 `PRAGMA table_info(表)` 查列在不在,不在才
/// `ALTER TABLE … ADD COLUMN`;列已存在即 no-op,可以在每次开库时安全重复
/// 调用。规矩不变:每加一列,必须同时改 `schema.sql` 并在开库流程里加一行
/// 这样的调用,否则存量库直接崩(本仓库踩过的真坑)。
///
/// 本片三张新表本身没有列需要迁移(`CREATE TABLE` 一步到位)——但
/// `SqliteStore::open` 现在真的调用它一次(`run.demoted_at`,见调用点文
/// 档),不再是死代码。
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

    async fn mark_issue_in_progress(&self, id: IssueId, at: i64) -> Result<()> {
        // 无条件写(design-s4-runmanager.md §3.6):「进行中」是运行管理器
        // 唯一改活状态的落点,没有比较并置——重复调用(诚实失败后重试)
        // 无害,仍然一路写到 `in_progress`。
        sqlx::query("UPDATE issue SET status = 'in_progress', updated_at = ? WHERE id = ?")
            .bind(at)
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn transition_issue_status(
        &self,
        id: IssueId,
        from: IssueStatus,
        to: IssueStatus,
        at: i64,
    ) -> Result<bool> {
        // 纯机械的比较并置写——合法性由调用方(`bw-app::App::
        // transition_issue`)查 `bw_core::can_transition_to` 后再调用这里,
        // 这一层自己不判断。
        let outcome =
            sqlx::query("UPDATE issue SET status = ?, updated_at = ? WHERE id = ? AND status = ?")
                .bind(issue_status_text(to))
                .bind(at)
                .bind(id.uuid().to_string())
                .bind(issue_status_text(from))
                .execute(&self.pool)
                .await?;
        Ok(outcome.rows_affected() == 1)
    }
}

/// `issue.status` 列的文本编码——与 [`parse_issue_status`] 互为逆运算。
/// `transition_issue_status` 的比较并置需要把 `IssueStatus` 编回文本才能
/// 拼进 `WHERE status = ?`/`SET status = ?`。
fn issue_status_text(s: IssueStatus) -> &'static str {
    match s {
        IssueStatus::Backlog => "backlog",
        IssueStatus::Todo => "todo",
        IssueStatus::InProgress => "in_progress",
        IssueStatus::InReview => "in_review",
        IssueStatus::Done => "done",
        IssueStatus::Blocked => "blocked",
        IssueStatus::Cancelled => "cancelled",
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

    async fn find_live_delivery_run(&self, issue_id: IssueId) -> Result<Option<RunId>> {
        // 直接查唯一索引覆盖的那一行——跨重启也成立,不靠进程内缓存
        // (design §3.4 第二行)。
        let row = sqlx::query(
            "SELECT id FROM run WHERE issue_id = ? AND ended_at IS NULL AND kind = 'delivery'",
        )
        .bind(issue_id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|r| parse_uuid(&r.get::<String, _>("id"), RunId::from_uuid))
            .transpose()
    }

    async fn mark_run_started(&self, id: RunId, upstream_session: &str, at: i64) -> Result<bool> {
        let _ = at; // schema 没有独立的「确认起工」时刻列(design §2.2 只有一个 started_at,插行时已落定)。
        let outcome = sqlx::query(
            "UPDATE run SET state = 'running', upstream_session = ? \
             WHERE id = ? AND state = 'starting'",
        )
        .bind(upstream_session)
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn demote_run(&self, id: RunId, at: i64) -> Result<bool> {
        // 比较并置(design §3.5①):谓词同时守住「还是交付」与「还活着」
        // 两半——已经降级过、或者已经关门的运行,这次调用诚实空转。
        let outcome = sqlx::query(
            "UPDATE run SET kind = 'consultation', demoted_at = ? \
             WHERE id = ? AND kind = 'delivery' AND ended_at IS NULL",
        )
        .bind(at)
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn reap_open_runs(&self, at: i64) -> Result<Vec<RunId>> {
        // 一次 UPDATE 覆盖全部还开着的运行(design §9「集合式 UPDATE」)。
        // `end_kind`/`end_detail` 保持原值(插行时 end_kind 恒 NULL、
        // end_detail 恒 ''——如实标注「不知道」,不填一个猜的);
        // `settled_at` 不动,账没结如实欠着。SQLite 的 `RETURNING` 让这仍
        // 是一条语句,不是「先 SELECT 再逐条 UPDATE」的 N 次往返。
        let rows = sqlx::query(
            "UPDATE run SET ended_at = ?, state = 'orphaned' \
             WHERE ended_at IS NULL RETURNING id",
        )
        .bind(at)
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| parse_uuid(&r.get::<String, _>("id"), RunId::from_uuid))
            .collect()
    }
}
