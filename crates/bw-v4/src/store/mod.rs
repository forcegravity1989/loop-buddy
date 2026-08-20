//! V4 本机库:四张表的哑存储。
//!
//! 「哑」是字面意思——这一层不做任何业务判断(该不该转移状态、健康是什么
//! 颜色、该不该建活),只负责把行读出来写回去。判断全在 `crate::app`。
//!
//! 这里**没有** `set_signal` 方法:项目行的灯只能由
//! [`crate::derive::derive_project_health`] 现算出一个密封值,再经
//! [`V4Store::cache_project_health`] 写回缓存列。手工把某个项目点成绿,在
//! 类型上就做不到。

mod issue;
mod meta;
mod project;

pub use meta::{notify_seen_key, WORKSPACES_ROOT_KEY};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

const SCHEMA: &str = include_str!("schema.sql");

/// 新壳默认的库文件名。与旧壳的 `workbench.db` 同目录不同名,互不相扰。
pub const DEFAULT_DB_FILENAME: &str = "workbench-v4.db";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] sqlx::Error),
    #[error("库里读到无法识别的枚举值 {field}={value:?}")]
    BadEnum { field: &'static str, value: String },
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// 四张表的 SQLite 存储。
#[derive(Clone, Debug)]
pub struct V4Store {
    pool: SqlitePool,
}

impl V4Store {
    /// 打开(不存在就建)一个 V4 库。
    ///
    /// 开发期不写任何 `add_column_if_missing`:改了 `schema.sql` 就删库重建。
    /// 等第一个真实用户的 V4 库出现(内部试点),再恢复
    /// CLAUDE.md「schema 迁移双守卫」纪律。
    pub async fn open(path: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        // 逐条执行。先剥掉 `--` 行注释,免得注释里的 `;` 把语句劈成两半。
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

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// 库里到底有哪几张表 —— 验收读回「恰好四张」用的就是它。
    pub async fn table_names(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }
}

/// 当前 UTC 秒。库里所有时间戳都是这一个刻度。
pub fn now_ts() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// 把带 `#[serde(rename_all = "snake_case")]` 的枚举写成库里的字符串。
/// 手写 match 会和 serde 名字慢慢漂移,这里让 serde 当唯一事实源。
/// 序列化不出一个字符串就直接炸。今天这几个枚举都是纯 unit variant,炸不了;
/// 哪天有人给枚举加了负载,静默写一个空串进 `NOT NULL` 列会让整行以后都读
/// 不出来 —— 那是把坏数据藏起来,不如当场停。
pub(crate) fn enum_to_db<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|x| x.as_str().map(str::to_string))
        .expect("枚举必须能序列化成一个字符串(带负载的变体不能进库这一列)")
}

/// [`enum_to_db`] 的逆。库里读到不认识的值就报错,不静默 fallback 到某个
/// 默认档——那样会把坏数据伪装成正常数据。
pub(crate) fn enum_from_db<T: serde::de::DeserializeOwned>(
    field: &'static str,
    s: &str,
) -> Result<T> {
    serde_json::from_value(serde_json::Value::String(s.to_string())).map_err(|_| {
        StoreError::BadEnum {
            field,
            value: s.to_string(),
        }
    })
}
