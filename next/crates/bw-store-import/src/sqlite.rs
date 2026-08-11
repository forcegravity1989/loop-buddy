//! SQLite implementation of [`crate::ImportStore`]。
//!
//! **自己开一条独立的 sqlite 连接**,连到与主存储(`bw_store::SqliteStore`)
//! 同一个数据库文件——不共用 `bw_store::SqliteStore` 内部的连接池(那个
//! 字段是私有的,这个 crate 也没有理由伸手进去拿:两个连接各自独立、各
//! 自单连接池,SQLite 的文件级锁天然处理"同一时刻只有一个写者"这件事,
//! `bw-app` 的 `import_legacy` 指挥器本来就是顺序调用,不存在两个连接同
//! 时写的竞争)。

use crate::{
    ImportArtifactRow, ImportHandoffRow, ImportIssueRow, ImportLedgerEntry, ImportMetricRow,
    ImportObservationRow, ImportProjectRow, ImportStore,
};
use async_trait::async_trait;
use bw_core::IssueStatus;
use bw_store::{metric_tier_text, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use time::OffsetDateTime;
use uuid::Uuid;

const SCHEMA_IMPORT: &str = include_str!("schema_import.sql");

pub struct ImportSqliteStore {
    pool: SqlitePool,
}

impl ImportSqliteStore {
    /// 打开导入专用的写入面——**这是唯一一处会建 `import_ledger`/
    /// `artifact_archive` 这两张表的代码**(design §2.4 硬约束 1 的另一
    /// 半:另一半是 `app-desktop` 永远不依赖这个 crate,见 crate 文档)。
    /// `create_if_missing(false)`——数据库文件本身必须已经存在,这个方法
    /// 不负责"从无到有"建一个新库。调用方(`bw-app::examples::
    /// import_legacy`)总是先调 `bw_store::SqliteStore::open` 建好基础五
    /// 张表(project/metric/observation/issue/handoff)再调这个方法;如果
    /// 有人绕过那一步直接调这里,应当得到一个响亮的"文件不存在"错误,而
    /// 不是悄悄建一个只有导入两张表、缺基础五表的半成品库。
    pub async fn open(path: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        // schema_import.sql 里只有 CREATE TABLE / CREATE INDEX,没有任何
        // CREATE TRIGGER——`bw_store::sqlite` 那份 schema.sql 解析器要处理
        // 触发器体内部的分号(那份文件真的有触发器),这份没有,朴素按
        // `;` 切分是安全的:如果将来这份文件真的加了触发器又没人记得同
        // 步改这个切分逻辑,SQLite 会在下面这个循环里对着被切断的半条语
        // 句报一个真实的语法错误——响亮地失败,不是静默吞掉建错表这件
        // 事。
        for stmt in schema_statements(SCHEMA_IMPORT) {
            sqlx::query(&stmt).execute(&pool).await?;
        }

        Ok(Self { pool })
    }
}

/// 剥掉 `--` 行注释,按 `;` 切分——`schema_import.sql` 没有触发器,不需要
/// `bw_store::sqlite` 里那份「CREATE TRIGGER … END 认成一条语句」的复杂
/// 解析(那是给 `schema.sql` 里真实存在的触发器体准备的)。
fn schema_statements(sql: &str) -> Vec<String> {
    sql.lines()
        .map(|line| match line.find("--") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .split(';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// `issue.status` 文本编码——这个 crate 自己需要一份(导入的是历史状态原
/// 文,不经过 `bw_core::IssueStatus::can_transition_to`),不复用
/// `bw-store` 内部私有的状态编码函数(那些只给"合法转移之后落盘"的路径
/// 用,这里刻意分开命名,免得将来有人把导入路径错接到状态机守卫上,见
/// crate 文档「为什么这不算开后门」)。
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

/// 每个 `import_*` 方法都是 `INSERT OR IGNORE`(design §2.6「撞了就跳
/// 过」):`rows_affected() == 1` = 这次真插入了新的一行;`0` = 撞了主键
/// (同一个旧编号第二次导入),数据库自己拒绝重复插入,调用方据此区分
/// "新增"与"重复",不需要先查一遍存不存在。**没有任何 UPDATE/DELETE**
/// (design §2.4 硬约束 2)——观测表的两条数据库触发器因此照旧生效,这里
/// 走的还是同一张 `observation` 表,不是绕过去的第二条路。
#[async_trait]
impl ImportStore for ImportSqliteStore {
    async fn import_project(&self, row: ImportProjectRow) -> Result<bool> {
        let outcome = sqlx::query(
            "INSERT OR IGNORE INTO project (id, name, root_path, active_stage, created_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(row.id.uuid().to_string())
        .bind(&row.name)
        .bind(&row.root_path)
        .bind(row.active_stage.map(|s| s.index() as i64))
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn import_metric(&self, row: ImportMetricRow) -> Result<bool> {
        let outcome = sqlx::query(
            "INSERT OR IGNORE INTO metric (id, project_id, tier, name, def, target_raw, \
             collect_kind, collect_query, origin, archived_at, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.uuid().to_string())
        .bind(row.project_id.uuid().to_string())
        .bind(metric_tier_text(row.tier))
        .bind(&row.name)
        .bind(&row.def)
        .bind(&row.target_raw)
        .bind(&row.collect_kind)
        .bind(&row.collect_query)
        .bind(&row.origin)
        .bind(row.archived_at)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn import_observation(&self, row: ImportObservationRow) -> Result<bool> {
        let outcome = sqlx::query(
            "INSERT OR IGNORE INTO observation (id, metric_id, project_id, ts, raw_value, \
             source, source_hint, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.uuid().to_string())
        .bind(row.metric_id.uuid().to_string())
        .bind(row.project_id.uuid().to_string())
        .bind(row.ts)
        .bind(&row.raw_value)
        .bind(&row.source)
        .bind(&row.source_hint)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn import_issue(&self, row: ImportIssueRow) -> Result<bool> {
        let outcome = sqlx::query(
            "INSERT OR IGNORE INTO issue (id, project_id, number, title, status, \
             settled_at, stage, body, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.uuid().to_string())
        .bind(row.project_id.uuid().to_string())
        .bind(row.number)
        .bind(&row.title)
        .bind(issue_status_text(row.status))
        .bind(row.settled_at)
        .bind(row.stage.map(|s| s.index() as i64))
        .bind(&row.body)
        .bind(row.created_at)
        .bind(row.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn import_handoff(&self, row: ImportHandoffRow) -> Result<bool> {
        let outcome = sqlx::query(
            "INSERT OR IGNORE INTO handoff (id, project_id, from_stage, to_stage, risky, \
             note, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.uuid().to_string())
        .bind(row.project_id.uuid().to_string())
        .bind(row.from_stage.index() as i64)
        .bind(row.to_stage.index() as i64)
        .bind(row.risky as i64)
        .bind(&row.note)
        .bind(row.created_at)
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn import_artifact(&self, row: ImportArtifactRow) -> Result<bool> {
        let outcome = sqlx::query(
            "INSERT OR IGNORE INTO artifact_archive (id, project_id, issue_id, \
             stage_index, path, kind, bytes, git_commit, registered_at, imported_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(row.id.uuid().to_string())
        .bind(row.project_id.uuid().to_string())
        .bind(row.issue_id.map(|i| i.uuid().to_string()))
        .bind(row.stage.map(|s| s.index() as i64))
        .bind(&row.path)
        .bind(&row.kind)
        .bind(row.bytes)
        .bind(&row.git_commit)
        .bind(row.registered_at)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(outcome.rows_affected() == 1)
    }

    async fn record_import_ledger(&self, entry: ImportLedgerEntry) -> Result<()> {
        sqlx::query(
            "INSERT INTO import_ledger (id, legacy_db_path, legacy_db_fingerprint, \
             project_count, metric_count, observation_count, issue_count, handoff_count, \
             artifact_count, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&entry.legacy_db_path)
        .bind(&entry.legacy_db_fingerprint)
        .bind(entry.project_count)
        .bind(entry.metric_count)
        .bind(entry.observation_count)
        .bind(entry.issue_count)
        .bind(entry.handoff_count)
        .bind(entry.artifact_count)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
