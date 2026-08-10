//! SQLite implementation of [`IssueStore`] / [`RunStore`] (sqlx,
//! runtime-checked queries).
//!
//! Single-connection pool: a desktop app has one writer, so serializing
//! access sidesteps `SQLITE_BUSY` without ceremony (same shape as v1
//! `crates/bw-store/src/sqlite.rs`).

use crate::{
    metric_tier_text, parse_metric_tier, parse_run_end_kind, parse_run_kind, run_end_kind_text,
    run_kind_text, HandoffRow, HandoffStore, IncomingMetricDef, IssueRow, IssueStore, MetricRow,
    MetricStore, MetricSyncReport, MetricTier, NewIssue, NewObservation, NewProject, NewRun,
    ObservationRow, ObservationStore, ProjectRow, Result, RunEndKind, RunRow, RunStore, StoreError,
};
use async_trait::async_trait;
use bw_core::{
    HandoffId, IssueId, IssueStatus, MetricId, ObservationId, ProjectId, RunId, RunState, StageKind,
};
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
        for stmt in schema_statements(&cleaned) {
            sqlx::query(&stmt).execute(&pool).await?;
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

        // next 切片五B(design §2.1):三列加列,双守卫第二次真实使用——
        // 与上面 run.demoted_at 那次不同,这三列是**真正**这次迁移新增
        // 的字段(切片四A 的 schema 里没有它们)。对每一个真实数据库都
        // 安全:字段已存在就是 no-op,不存在才补。
        add_column_if_missing(&pool, "project", "active_stage", "INTEGER").await?;
        add_column_if_missing(&pool, "issue", "stage", "INTEGER").await?;
        add_column_if_missing(&pool, "issue", "body", "TEXT NOT NULL DEFAULT ''").await?;

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

/// 把整份 schema.sql(已经剥掉 `--` 行注释)切成一条条可独立执行的语句。
///
/// naive 的 `cleaned.split(';')`(旧写法)对绝大多数语句都对,但对
/// `CREATE TRIGGER … BEGIN … END` 这种形状不对——触发器体自己内部也用
/// `;` 分隔每条语句(SQLite 的触发器语法要求最后一条语句同样要有分号),
/// naive 切法会把一条触发器从中间切成两截,`sqlx::query` 各执行一半时报
/// 语法错误。next 切片五-1 修复轮第一次真的在 schema.sql 里写触发器
/// (`trg_observation_no_update`/`trg_observation_no_delete`),这个函数把
/// 「`CREATE TRIGGER` 到独立一行 `END`」之间认成一整条语句,其余语句的切
/// 法与旧写法逐字节一致(照样按 `;` 切,空白语句照样跳过)。
fn schema_statements(cleaned: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut pending: Option<String> = None;
    for part in cleaned.split(';') {
        match pending.take() {
            Some(mut acc) => {
                acc.push(';');
                acc.push_str(part);
                if ends_with_end_keyword(&acc) {
                    statements.push(acc);
                } else {
                    pending = Some(acc);
                }
            }
            None => {
                if part.trim().is_empty() {
                    continue;
                }
                if starts_with_create_trigger(part) && !ends_with_end_keyword(part) {
                    pending = Some(part.to_string());
                } else {
                    statements.push(part.to_string());
                }
            }
        }
    }
    // 如果整份 schema.sql 写歪了(`CREATE TRIGGER` 没有配对的 `END`),把
    // 剩下的残片原样交给 sqlx 执行——它会报一个真实的 SQL 语法错误,而不
    // 是在这里默默吞掉,让开库流程假装成功。
    if let Some(acc) = pending {
        if !acc.trim().is_empty() {
            statements.push(acc);
        }
    }
    statements
}

fn starts_with_create_trigger(stmt: &str) -> bool {
    stmt.trim_start()
        .to_ascii_uppercase()
        .starts_with("CREATE TRIGGER")
}

fn ends_with_end_keyword(stmt: &str) -> bool {
    stmt.split_whitespace()
        .last()
        .map(|w| w.eq_ignore_ascii_case("END"))
        .unwrap_or(false)
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
        let row = sqlx::query(
            "SELECT id, name, root_path, active_stage, created_at FROM project WHERE id = ?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else {
            return Ok(None);
        };
        // `active_stage` 是 INTEGER(1..5,design §2.1),与 `handoff` 表
        // from_stage/to_stage 那两列同一制式(2026-08-11 主控裁决统一
        // INTEGER 后),都走 `StageKind::index()`/`from_index()` 这条
        // 1-based 编号互转。
        let active_stage_num: Option<i64> = r.get("active_stage");
        let active_stage = active_stage_num
            .map(|n| {
                StageKind::from_index(n as u8)
                    .ok_or_else(|| StoreError::Other(format!("bad project.active_stage {n:?}")))
            })
            .transpose()?;
        Ok(Some(ProjectRow {
            id: parse_uuid(&r.get::<String, _>("id"), ProjectId::from_uuid)?,
            name: r.get("name"),
            root_path: r.get("root_path"),
            active_stage,
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
            "SELECT id, project_id, number, title, status, settled_at, stage, body, \
             created_at, updated_at FROM issue WHERE id = ?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(r) = row else {
            return Ok(None);
        };
        let status_text: String = r.get("status");
        // `issue.stage` 同 `project.active_stage`:INTEGER(1..5),走
        // `StageKind::from_index`,不是 TEXT。
        let stage_num: Option<i64> = r.get("stage");
        let stage = stage_num
            .map(|n| {
                StageKind::from_index(n as u8)
                    .ok_or_else(|| StoreError::Other(format!("bad issue.stage {n:?}")))
            })
            .transpose()?;
        Ok(Some(IssueRow {
            id: parse_uuid(&r.get::<String, _>("id"), IssueId::from_uuid)?,
            project_id: parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid)?,
            number: r.get("number"),
            title: r.get("title"),
            status: parse_issue_status(&status_text)?,
            settled_at: r.get("settled_at"),
            stage,
            body: r.get("body"),
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

fn metric_row_from_sqlx(r: &sqlx::sqlite::SqliteRow) -> Result<MetricRow> {
    let tier_text: String = r.get("tier");
    Ok(MetricRow {
        id: parse_uuid(&r.get::<String, _>("id"), MetricId::from_uuid)?,
        project_id: parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid)?,
        tier: parse_metric_tier(&tier_text)?,
        name: r.get("name"),
        def: r.get("def"),
        target_raw: r.get("target_raw"),
        collect_kind: r.get("collect_kind"),
        collect_query: r.get("collect_query"),
        origin: r.get("origin"),
        archived_at: r.get("archived_at"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

#[async_trait]
impl MetricStore for SqliteStore {
    #[allow(clippy::too_many_arguments)]
    async fn create_manual_metric(
        &self,
        id: MetricId,
        project_id: ProjectId,
        tier: MetricTier,
        name: &str,
        def: &str,
        target_raw: &str,
        collect_kind: &str,
        collect_query: &str,
        at: i64,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO metric (id, project_id, tier, name, def, target_raw, collect_kind, \
             collect_query, origin, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'manual', ?, ?)",
        )
        .bind(id.uuid().to_string())
        .bind(project_id.uuid().to_string())
        .bind(metric_tier_text(tier))
        .bind(name)
        .bind(def)
        .bind(target_raw)
        .bind(collect_kind)
        .bind(collect_query)
        .bind(at)
        .bind(at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_metric(&self, id: MetricId) -> Result<Option<MetricRow>> {
        let row = sqlx::query(
            "SELECT id, project_id, tier, name, def, target_raw, collect_kind, collect_query, \
             origin, archived_at, created_at, updated_at FROM metric WHERE id = ?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.as_ref().map(metric_row_from_sqlx).transpose()
    }

    async fn list_metrics(&self, project_id: ProjectId) -> Result<Vec<MetricRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, tier, name, def, target_raw, collect_kind, collect_query, \
             origin, archived_at, created_at, updated_at FROM metric \
             WHERE project_id = ? ORDER BY tier, name",
        )
        .bind(project_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(metric_row_from_sqlx).collect()
    }

    async fn sync_metrics_from_file(
        &self,
        project_id: ProjectId,
        incoming: Vec<IncomingMetricDef>,
        at: i64,
    ) -> Result<MetricSyncReport> {
        // 整个同步在一个事务里完成——要么全部生效,要么一行都不改
        // (design §2.2「一个字节不碰」的姊妹保证)。
        let mut tx = self.pool.begin().await?;
        let mut report = MetricSyncReport::default();

        // 先取出这个项目当前所有 origin='file' 的行(id/tier/name/是否
        // 已停用),按「层级+名字」建索引——这就是「按项目+层级+名字对上
        // 就原地更新、保住原来的行 id」那条同步语义的查找表。同步完成后
        // 这个表里剩下的键就是「正本里已经没有的」,对应自动停用。
        let existing_rows = sqlx::query(
            "SELECT id, tier, name, archived_at FROM metric \
             WHERE project_id = ? AND origin = 'file'",
        )
        .bind(project_id.uuid().to_string())
        .fetch_all(&mut *tx)
        .await?;
        let mut existing: std::collections::HashMap<(String, String), (MetricId, bool)> =
            std::collections::HashMap::new();
        for r in &existing_rows {
            let tier_text: String = r.get("tier");
            let name: String = r.get("name");
            let id = parse_uuid(&r.get::<String, _>("id"), MetricId::from_uuid)?;
            let archived: Option<i64> = r.get("archived_at");
            existing.insert((tier_text, name), (id, archived.is_some()));
        }

        for def in &incoming {
            let key = (metric_tier_text(def.tier).to_string(), def.name.clone());
            if let Some((id, was_archived)) = existing.remove(&key) {
                sqlx::query(
                    "UPDATE metric SET def = ?, target_raw = ?, collect_kind = ?, \
                     collect_query = ?, archived_at = NULL, updated_at = ? WHERE id = ?",
                )
                .bind(&def.def)
                .bind(&def.target_raw)
                .bind(&def.collect_kind)
                .bind(&def.collect_query)
                .bind(at)
                .bind(id.uuid().to_string())
                .execute(&mut *tx)
                .await?;
                report.updated.push(id);
                if was_archived {
                    report.restored.push(id);
                }
            } else {
                let id = MetricId::new();
                sqlx::query(
                    "INSERT INTO metric (id, project_id, tier, name, def, target_raw, \
                     collect_kind, collect_query, origin, created_at, updated_at) \
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'file', ?, ?)",
                )
                .bind(id.uuid().to_string())
                .bind(project_id.uuid().to_string())
                .bind(metric_tier_text(def.tier))
                .bind(&def.name)
                .bind(&def.def)
                .bind(&def.target_raw)
                .bind(&def.collect_kind)
                .bind(&def.collect_query)
                .bind(at)
                .bind(at)
                .execute(&mut *tx)
                .await?;
                report.inserted.push(id);
            }
        }

        // 剩在查找表里的键 = 这次同步之前是 origin='file'、但这次不在
        // incoming 里的行——正本里已经删掉了,自动停用(不物删)。已经
        // 停用过的不重复记进 report(幂等,不重复告知)。
        for (id, was_archived) in existing.into_values() {
            if !was_archived {
                sqlx::query("UPDATE metric SET archived_at = ? WHERE id = ?")
                    .bind(at)
                    .bind(id.uuid().to_string())
                    .execute(&mut *tx)
                    .await?;
                report.archived.push(id);
            }
        }

        tx.commit().await?;
        Ok(report)
    }

    async fn archive_metric(&self, id: MetricId, at: i64) -> Result<bool> {
        let outcome =
            sqlx::query("UPDATE metric SET archived_at = ? WHERE id = ? AND archived_at IS NULL")
                .bind(at)
                .bind(id.uuid().to_string())
                .execute(&self.pool)
                .await?;
        Ok(outcome.rows_affected() == 1)
    }
}

#[async_trait]
impl ObservationStore for SqliteStore {
    async fn insert_observation(&self, o: NewObservation) -> Result<()> {
        sqlx::query(
            "INSERT INTO observation (id, metric_id, project_id, ts, raw_value, source, \
             source_hint, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(o.id.uuid().to_string())
        .bind(o.metric_id.uuid().to_string())
        .bind(o.project_id.uuid().to_string())
        .bind(o.ts)
        .bind(&o.raw_value)
        .bind(&o.source)
        .bind(&o.source_hint)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_observations(&self, metric_id: MetricId) -> Result<Vec<ObservationRow>> {
        let rows = sqlx::query(
            "SELECT id, metric_id, project_id, ts, raw_value, source, source_hint, created_at \
             FROM observation WHERE metric_id = ? ORDER BY ts DESC",
        )
        .bind(metric_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                Ok(ObservationRow {
                    id: parse_uuid(&r.get::<String, _>("id"), ObservationId::from_uuid)?,
                    metric_id: parse_uuid(&r.get::<String, _>("metric_id"), MetricId::from_uuid)?,
                    project_id: parse_uuid(
                        &r.get::<String, _>("project_id"),
                        ProjectId::from_uuid,
                    )?,
                    ts: r.get("ts"),
                    raw_value: r.get("raw_value"),
                    source: r.get("source"),
                    source_hint: r.get("source_hint"),
                    created_at: r.get("created_at"),
                })
            })
            .collect()
    }
}

#[async_trait]
impl HandoffStore for SqliteStore {
    async fn set_active_stage(
        &self,
        project_id: ProjectId,
        stage: StageKind,
        at: i64,
    ) -> Result<()> {
        let _ = at; // project 表没有独立的 updated_at 列(design §2.2 只给了 created_at)。
        sqlx::query("UPDATE project SET active_stage = ? WHERE id = ?")
            .bind(stage.index() as i64)
            .bind(project_id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn handoff_stage(
        &self,
        project_id: ProjectId,
        from: StageKind,
        to: StageKind,
        risky: bool,
        note: &str,
        at: i64,
    ) -> Result<HandoffId> {
        // 原子操作:插一行交棒流水 + 把 project.active_stage 推到 `to`
        // ——原样搬 v1 `handoff_stage` 的语义(v1 `crates/bw-store/src/
        // sqlite.rs` 约 1248 行,原子写这个形状本身不变)。**列类型不是
        // 逐字移植**:v1 的 `handoff.from_stage`/`to_stage` 是 TEXT,这里
        // 是 INTEGER,与 `project.active_stage`/`issue.stage` 同一制式,
        // 走 `StageKind::index()`——2026-08-11 主控裁决,理由见
        // schema.sql `handoff` 表上方的注记(设计稿原文的 TEXT 口径与
        // §2.1 自相矛盾,会让 §3.2 第⑤条查询假绿式恒空)。
        let mut tx = self.pool.begin().await?;
        let id = HandoffId::new();
        sqlx::query(
            "INSERT INTO handoff (id, project_id, from_stage, to_stage, risky, note, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.uuid().to_string())
        .bind(project_id.uuid().to_string())
        .bind(from.index() as i64)
        .bind(to.index() as i64)
        .bind(risky as i64)
        .bind(note)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE project SET active_stage = ? WHERE id = ?")
            .bind(to.index() as i64)
            .bind(project_id.uuid().to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(id)
    }

    async fn list_handoffs(&self, project_id: ProjectId) -> Result<Vec<HandoffRow>> {
        let rows = sqlx::query(
            "SELECT id, project_id, from_stage, to_stage, risky, note, created_at \
             FROM handoff WHERE project_id = ? ORDER BY created_at DESC",
        )
        .bind(project_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                // INTEGER(1..5),同 `project.active_stage`/`issue.stage`
                // ——2026-08-11 主控裁决(见 handoff_stage 上方注记),不
                // 经 TEXT 互转。
                let from_num: i64 = r.get("from_stage");
                let to_num: i64 = r.get("to_stage");
                let risky_num: i64 = r.get("risky");
                Ok(HandoffRow {
                    id: parse_uuid(&r.get::<String, _>("id"), HandoffId::from_uuid)?,
                    project_id: parse_uuid(
                        &r.get::<String, _>("project_id"),
                        ProjectId::from_uuid,
                    )?,
                    from_stage: StageKind::from_index(from_num as u8).ok_or_else(|| {
                        StoreError::Other(format!("bad handoff.from_stage {from_num:?}"))
                    })?,
                    to_stage: StageKind::from_index(to_num as u8).ok_or_else(|| {
                        StoreError::Other(format!("bad handoff.to_stage {to_num:?}"))
                    })?,
                    risky: risky_num != 0,
                    note: r.get("note"),
                    created_at: r.get("created_at"),
                })
            })
            .collect()
    }
}
