//! SQLite implementation of [`Store`] (sqlx, runtime-checked queries).
//!
//! Single-connection pool: a desktop app has one writer, so serializing access
//! sidesteps `SQLITE_BUSY` without ceremony.

use crate::{
    cadence_text, connector_status_text, cron_mode_text, cron_status_text, cycle_text,
    hub_source_columns, issue_priority_text, issue_status_text, maturity_text, parse_adapted_from,
    parse_cadence, parse_connector_status, parse_cron_mode, parse_cron_status, parse_cycle,
    parse_hub_source, parse_issue_priority, parse_issue_status, parse_maturity,
    parse_session_status, parse_sig, parse_stage_kind, session_status_text, sig_text,
    stage_kind_text, AgentEdit, ConnectorDefSync, ConnectorsFileSync, ConnectorsFileSyncSummary,
    GlobalHandoffRow, HandoffRow, MessageRow, MetricDefSync, MetricOrigin, MetricRole,
    MetricSignal, MetricsFileSync, MetricsFileSyncSummary, NewAgent, NewArtifact, NewConnector,
    NewCronTask, NewIssue, NewKnowledgeSource, NewMetric, NewProject, NewSession, NewSkill,
    NewSkillFile, NewStage, NewWorkflowRun, NewWorkflowSpec, ObservationRow, PersistedSignals,
    ProjectFileSync, ProjectRow, Result, SessionKind, SessionRow, SkillEdit, SkillFileRow,
    StageRow, StageSignal, Store, StoreError, WorkflowEdit,
};
use async_trait::async_trait;
use bw_core::derive::{
    evaluate_metric, measure, parse_target_with, reduce_worst_of, AmberBand, Measurement,
};
use bw_core::model::{
    AgentCard, AgentRef, AgentSkillTag, Artifact, ArtifactKind, Author, ClaudeConversation,
    Connector, ConnectorStatus, CronEffectiveness, CronStatus, CronTask, HubSource, Issue,
    IssueStatus, KnowledgeSource, LoopConfig, Maturity, MaturityPeriod, PhaseMeta, Readiness,
    RunStatus, RunTrigger, Signal, SkillCard, SkillRef, SourceKind, StageKind, UsageRank,
    WorkflowKind, WorkflowRun, WorkflowRunAnalytics, WorkflowSpec, WorkflowVersion,
};
use bw_core::stage_catalog::StageOrigin;
use bw_core::{
    AgentId, ArtifactId, ConnectorId, ConversationId, CronTaskId, IssueId, KnowledgeSourceId,
    MetricId, ProjectId, SessionId, SkillFileId, SkillId, WorkflowId, WorkflowRunId,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use time::OffsetDateTime;
use uuid::Uuid;

const SCHEMA: &str = include_str!("schema.sql");

pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Open (creating if missing) a SQLite database at `path` and apply the schema.
    pub async fn open(path: &str) -> Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        // Apply schema statement-by-statement. Strip `--` line comments first so
        // a `;` inside a comment can't split a statement mid-sentence.
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

        // `CREATE TABLE IF NOT EXISTS` above is a no-op against a real,
        // pre-existing on-disk DB whose `cron_task` table predates a new
        // column — exactly the class of bug that already crashed this app
        // once (see archive/workbench-pre-5stage-migration.db history).
        // Guarded, additive `ADD COLUMN` migrations belong here so old real
        // databases keep opening instead of requiring another manual reset.
        add_column_if_missing(
            &pool,
            "cron_task",
            "last_run_at",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        // A1: autopilot mode — what a due task does. Defaults to run_workflow
        // so pre-A1 rows keep their semantics; create_issue mints an Issue.
        add_column_if_missing(
            &pool,
            "cron_task",
            "mode",
            "TEXT NOT NULL DEFAULT 'run_workflow'",
        )
        .await?;
        add_column_if_missing(&pool, "cron_task", "issue_stage", "TEXT").await?;
        add_column_if_missing(&pool, "cron_task", "issue_assignee", "TEXT").await?;
        // iter 4: link scheduled runs to the cron task that fired them. Old
        // DBs (pre-iter-4) opened before this column existed get it added here;
        // manual-run rows simply stay NULL.
        add_column_if_missing(&pool, "workflow_run", "cron_task_id", "TEXT").await?;
        // P4: workspace HEAD at run start/settle — feeds the Issue detail's
        // "这次运行改了什么" diff. Mock runs (no workspace) stay NULL.
        add_column_if_missing(&pool, "workflow_run", "head_before", "TEXT").await?;
        add_column_if_missing(&pool, "workflow_run", "head_after", "TEXT").await?;
        // Playbook upgrade: per-phase real instructions. Old DBs get the
        // column with `'[]'` — every existing workflow keeps its shared-prompt
        // behavior byte-for-byte.
        add_column_if_missing(
            &pool,
            "workflow_spec",
            "phase_prompts",
            "TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        add_column_if_missing(
            &pool,
            "workflow_version",
            "phase_prompts",
            "TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        // 完整形态: skills/agents grow executable bodies, agents grow real
        // win accounting, connectors grow a project binding + live config.
        // All guarded — a pre-完整形态 DB opens unchanged, with honest empty
        // defaults ('' = catalog reference, 0 wins = no evidence).
        add_column_if_missing(&pool, "skill", "content", "TEXT NOT NULL DEFAULT ''").await?;
        add_column_if_missing(&pool, "agent", "instructions", "TEXT NOT NULL DEFAULT ''").await?;
        add_column_if_missing(&pool, "agent", "wins", "INTEGER NOT NULL DEFAULT 0").await?;
        add_column_if_missing(&pool, "connector", "project_id", "TEXT").await?;
        add_column_if_missing(&pool, "connector", "config", "TEXT NOT NULL DEFAULT ''").await?;
        // R2: skill provenance — link a distilled skill back to the real
        // completed Issue (+ the agent that did the work) it was distilled
        // from. NULL = catalog/seeded skill (no real-work origin). Old DBs
        // opened before R2 get these columns added here; fresh DBs define them
        // in the `skill` CREATE TABLE.
        add_column_if_missing(&pool, "skill", "distilled_from_issue", "TEXT").await?;
        add_column_if_missing(&pool, "skill", "origin_agent", "TEXT").await?;
        // R3 settle-once: issues opened before this column exist unsettled —
        // honest for them (their Done predates issue-side accounting).
        add_column_if_missing(&pool, "issue", "settled_at", "INTEGER").await?;
        // A2: link runs and artifacts back to the Issue they belong to. Old DBs
        // opened before A2 get these columns (NULL = no issue binding, honest
        // for pre-A2 rows); fresh DBs also define them inline in CREATE TABLE.
        add_column_if_missing(&pool, "workflow_run", "issue_id", "TEXT").await?;
        add_column_if_missing(&pool, "artifact", "issue_id", "TEXT").await?;
        // A5-F: issues opened before this column exist get no blocked reason
        // (NULL = never blocked under this scheme — honest for pre-A5 rows).
        add_column_if_missing(&pool, "issue", "blocked_reason", "TEXT").await?;
        // 践行最小切片(2026-07-20,plan/09 墙 B):hub 三表加可空 project_id.
        // NULL = 沿用既有全局/共享语义,老库/老行为一律不变。
        add_column_if_missing(&pool, "workflow_spec", "project_id", "TEXT").await?;
        add_column_if_missing(&pool, "skill", "project_id", "TEXT").await?;
        add_column_if_missing(&pool, "agent", "project_id", "TEXT").await?;
        // codehub 对接(2026-07-28):远端身份二元组 (host, path) 均匀对称
        // —— 不是 github 不需要 host,是当时把 github.com 隐式默认了、漏存
        // 一列。老库若仍有 github_remote 列(2026-07-22 那波 ADD COLUMN 加的),
        // 改名成 remote_path(仓库首次列改名迁移,RENAME COLUMN,sqlite ≥3.25,
        // 数据原样保留:owner/repo 值不动,只是列名换了)。新库 schema.sql 直接
        // 建 remote_path,这里 no-op。
        rename_column_if_exists(&pool, "project", "github_remote", "remote_path").await?;
        // remote_host:github 隐含 github.com(NOT NULL DEFAULT 让老行直接拿到,
        // 和"这些仓当时就是接 GitHub 建的"这个真实状态一致);codehub 各填自己
        // 的域名(绿/黄/内源)。
        add_column_if_missing(
            &pool,
            "project",
            "remote_host",
            "TEXT NOT NULL DEFAULT 'github.com'",
        )
        .await?;
        // C4 · issue 身份映射: 老库开出来 github_number 是 0,和"未映射"这个
        // 真实状态一致(存量无仓项目 Issue 保持本地身份,如实留白)。
        add_column_if_missing(
            &pool,
            "issue",
            "github_number",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        // C5 · PR 验收环: 老库开出来 pr_number 是 0,和"还没有 PR"这个真实
        // 状态一致(存量 Issue、无仓项目 Issue 从不映射 PR,如实留白)。
        add_column_if_missing(&pool, "issue", "pr_number", "INTEGER NOT NULL DEFAULT 0").await?;
        // C6(plan/13 D5+D6):指标正本 `.bw/metrics.toml` 同步进来的采集方案 +
        // 来源标注。老库开出来全是空串/'manual' —— 和"从未同步过正本文件、
        // 这行是界面手建的"这个真实状态完全一致。
        add_column_if_missing(
            &pool,
            "project",
            "north_star_collect_kind",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        add_column_if_missing(
            &pool,
            "project",
            "north_star_collect_query",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        add_column_if_missing(&pool, "metric", "collect_kind", "TEXT NOT NULL DEFAULT ''").await?;
        add_column_if_missing(&pool, "metric", "collect_query", "TEXT NOT NULL DEFAULT ''").await?;
        add_column_if_missing(&pool, "metric", "origin", "TEXT NOT NULL DEFAULT 'manual'").await?;
        // 指标停用/归档。存量库全部行开出来 archived=0 / archived_at=NULL,
        // 和"这些指标都还在用、从没被停用过"这个真实状态完全一致。
        add_column_if_missing(&pool, "metric", "archived", "INTEGER NOT NULL DEFAULT 0").await?;
        add_column_if_missing(&pool, "metric", "archived_at", "INTEGER").await?;
        // C8(plan/13 D8):标配 Issue 三件套与标配 Skill 的稳定关联。老库开出
        // 来是空串,和"这张 Issue 没有标配 Skill 关联"这个真实状态完全一致
        // ——存量 Issue 全部是手建/Autopilot 建,从未挂过标配 Skill。
        add_column_if_missing(&pool, "issue", "standard_skill", "TEXT NOT NULL DEFAULT ''").await?;
        // V1 终端会话重构(阶段1): 存量搬运 —— 把 issue.claude_session_id
        // 非空的行搬进新表 claude_conversation(幂等:INSERT OR IGNORE,
        // issue_id UNIQUE 兜底,重复 open() 不产生重复行)。workspace_path/
        // branch_name 能从 issue + project 推就推(worktree 兄弟路径 +
        // bw/issue-<github_number>),推不出留空等首次 open 填(阶段4 resume
        // 时回填)。
        migrate_claude_conversations(&pool).await?;
        // 阶段1: 旧列退场(物理 DROP,先搬后删——搬在上一行,删在这)。
        // interactive_started / claude_session_id 业务零读(读路径收口到
        // claude_conversation),守「不为向后兼容留旧路径」。列不存在即
        // no-op(很老的库 Phase2a 之前没这两列;DROP 过的库二次 open 也 no-op)。
        drop_column_if_present(&pool, "issue", "interactive_started", &[]).await?;
        drop_column_if_present(&pool, "issue", "claude_session_id", &[]).await?;
        // T2 (plan/12 §6): Skill's source unified onto HubSource. Old rows'
        // bare `source='official'`/`'self_built'` text values already match
        // the new tag vocabulary 1:1 (no rewrite needed) — only the new
        // `official_library` sub-tag column is missing on a pre-T2 DB.
        // '' = no library sub-tag, which `parse_hub_source` reads as
        // "pre-T2 official row" → reclassified `SelfBuilt` (honest, see its
        // doc comment) for any row that predates this column.
        add_column_if_missing(
            &pool,
            "skill",
            "official_library",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        // T5 (2026-07-23, plan/12 §3): "Agent" == AGENT.md real modeling —
        // AllowedTools + which Agent CLI executes it, plus the same
        // HubSource provenance T2 gave `skill`. A pre-T5 DB's existing 5
        // built-in stage-role agent rows get '[]' tools, 'claude-code'
        // agent_cli (the only real executor either way), and 'self_built'
        // source (the acceptance-criterion default) — none of their
        // runs/win_rate/instructions data is touched by these ADD COLUMNs.
        add_column_if_missing(&pool, "agent", "tools", "TEXT NOT NULL DEFAULT '[]'").await?;
        add_column_if_missing(
            &pool,
            "agent",
            "agent_cli",
            "TEXT NOT NULL DEFAULT 'claude-code'",
        )
        .await?;
        add_column_if_missing(
            &pool,
            "agent",
            "source",
            "TEXT NOT NULL DEFAULT 'self_built'",
        )
        .await?;
        add_column_if_missing(
            &pool,
            "agent",
            "official_library",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        // T7 (2026-07-23, plan/12 §0/§2/§3): Agent gains the same stage-role
        // classification `workflow_spec.stage_ref` already has. NULL on
        // every pre-T7 row (including the five built-in stage-role agents)
        // is honest until the boot-time by-name backfill
        // (`seed_stage_role_agents_if_missing`) fills the real ones in;
        // every imported catalog row stays NULL = 通用/跨阶段 — nobody has
        // manually classified those, so this never guesses.
        add_column_if_missing(&pool, "agent", "stage_ref", "INTEGER").await?;
        // 五角色归类(2026-08-05):归类**动作**的出处。'' = 还没人归过类;
        // 'table' = bw-core 静态表回填;'distilled' = 按蒸馏出处 Issue 派生;
        // 'manual' = 人工在 SkillHub 改过(此后 Boot 的静态表回填整条跳过)。
        // 与 skill_stage 的行数共同派生四态 —— 单看行数分不出「判过了、不属
        // 任何阶段」和「还没人管」,而这两件事在本仓是必须分开的。
        add_column_if_missing(&pool, "skill", "stage_origin", "TEXT NOT NULL DEFAULT ''").await?;
        // Critical 修复(2026-08-06):上一版在这里直接删列,理由是「Boot 的
        // 搬值逻辑按 name 查静态表重建,不读旧列的值,所以删列不丢数据」——
        // 这句话对静态表**覆盖得到**的行是对的,但对覆盖不到的行是假的:静态
        // 表按名查不到的行,Boot 的按名回填从头到尾不会碰它,它的旧
        // `stage_ref` 单值就随列一起被删列语句静默抹掉。真实日常库独立核实
        // 命中过一次:`metrics-render`(`stage_ref=1`,来自另一条未合入本分支
        // 的产品线,静态表里没有这个名字)。删列之前必须先把这类「静态表管不
        // 着」的行搬进 `skill_stage`,标 `StageOrigin::Legacy`——静态表管得到
        // 的行不搬,原样留给下面 Boot 的按名回填去写正确的**多值**(搬单值会
        // 让它们停在旧的单值上,是计划内的中间态倒退)。
        migrate_legacy_skill_stage_ref(&pool).await?;
        // 旧列真删(用户 2026-08-05:「不要害怕修改旧表,有需要就大胆重做」)。
        // 位置很关键:必须排在上面的保值搬迁之后 —— 搬迁函数自己按
        // `PRAGMA table_info` 判断列是否还在,删列之后再跑第二次 `open()` 会
        // 直接 no-op,天然幂等。
        drop_column_if_present(&pool, "skill", "stage_ref", &["idx_skill_stage"]).await?;
        // T16 (plan/12 §10 v1.1#3): workflow's main MD document, aligned
        // with `skill.content`. '' on every pre-T16 row = honestly "no
        // original document" (real pre-T16 workflows never had one to lose).
        add_column_if_missing(
            &pool,
            "workflow_spec",
            "content",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        // The index on `agent.stage_ref` deliberately lives here, *after* the
        // `add_column_if_missing` call above, instead of in `schema.sql`'s
        // `CREATE INDEX` blob (that whole blob replays unconditionally,
        // before this guard runs, against the real on-disk `agent` table —
        // a pre-T7 DB doesn't have the column yet at that point, so an index
        // on it there crashes with "no such column: stage_ref", caught by
        // this ticket's own migration E2E against an old fixture DB).
        // `workflow_spec.stage_ref`'s schema.sql-embedded index never hit
        // this because that column has been part of the table's initial
        // `CREATE TABLE` since before this ticket, not retrofitted.
        // (`skill.stage_ref`'s twin index, `idx_skill_stage`, was dropped
        // 2026-08-05 along with the column — see `drop_column_if_present`
        // above.)
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_agent_stage ON agent(stage_ref)")
            .execute(&pool)
            .await?;
        // C16(plan/14 规范条 4):仓平台选择器落库。老库开出来是 'github' ——
        // 和"这些存量项目当时就是接 GitHub 建的"这个真实状态一致(pre-C16
        // 没有别的平台可选,不是编出来的默认值)。
        add_column_if_missing(
            &pool,
            "project",
            "provider",
            "TEXT NOT NULL DEFAULT 'github'",
        )
        .await?;
        // PF1-R2-1②: rename legacy collect_metrics cron names ending in
        // "· 指标采集" (pre-PF1-3a default) to the specific "· 采集代码仓指标"
        // form, so `sqlite3 SELECT name` reads back the clear name too (UI +
        // DB 一致 — honest, not just a VM-layer display override). Idempotent:
        // only matches rows still ending in "· 指标采集"; new rows already use
        // the specific name (PF1-3a ①) and don't match. Storage name 是审计
        // 口径的活物,这次用户明确要 DB 读回也清楚,故走数据迁移而非 VM 派生。
        migrate_cron_collect_metrics_name(&pool).await?;

        Ok(Self { pool })
    }
}

/// 删 `skill.stage_ref` 之前的保值搬迁(Critical 修复,2026-08-06)。
///
/// 静态表([`bw_core::stage_catalog::SKILL_STAGE_CATALOG`])永远不可能覆盖每
/// 个用户库里的每一行——它只随本分支发行,另一条产品线/另一次手填都可能
/// 留下静态表查不到名字的 `stage_ref` 值。这些行**不会**被下面 Boot 的按名
/// 回填接住(那段逻辑本身就是按名查这张静态表),所以旧列一删,值就真没了。
///
/// 做法:只在列还在时跑(`PRAGMA table_info` 判据,与 [`drop_column_if_present`]
/// 同款,可安全重复调用);对每一行 `stage_ref IS NOT NULL` 的技能,**只在**
/// `stages_for(name)` 返回 `None`(静态表管不着)时才搬——搬法是往
/// `skill_stage` 插一行同值、把 `stage_origin` 标成 `Legacy`。静态表管得到的
/// 行原样不动,留给调用方之后紧接着跑的 Boot 按名回填去写正确的**多值**
/// (那才是这些行的正本;这里抢先写单值会造成计划内的中间态倒退)。
///
/// 越界的 `stage_ref` 值(理论上写不出来,但读侧 [`crate::sqlite::skill_row`]
/// 一贯宁可丢弃也不瞎猜)按同一条纪律处理:不认识的整数不搬,直接丢弃。
async fn migrate_legacy_skill_stage_ref(pool: &SqlitePool) -> Result<()> {
    let table_info = sqlx::query("PRAGMA table_info(skill)")
        .fetch_all(pool)
        .await?;
    let has_stage_ref = table_info
        .iter()
        .any(|r| r.get::<String, _>("name") == "stage_ref");
    if !has_stage_ref {
        // 已经真删过列的库(本函数在更早的一次 open() 里跑过)——no-op。
        return Ok(());
    }
    let legacy_rows =
        sqlx::query("SELECT id, name, stage_ref FROM skill WHERE stage_ref IS NOT NULL")
            .fetch_all(pool)
            .await?;
    for r in legacy_rows {
        let id: String = r.get("id");
        let name: String = r.get("name");
        let stage_ref: i64 = r.get("stage_ref");
        // 静态表管得到的行不搬——交给紧随其后的 Boot 按名回填写多值。
        if bw_core::stage_catalog::stages_for(&name).is_some() {
            continue;
        }
        // 越界值如实丢弃,不硬塞进关联表(读侧同一条纪律:不认识就不猜)。
        if StageKind::from_index(stage_ref as u8).is_none() {
            continue;
        }
        sqlx::query("INSERT OR IGNORE INTO skill_stage (skill_id, stage) VALUES (?, ?)")
            .bind(&id)
            .bind(stage_ref)
            .execute(pool)
            .await?;
        sqlx::query("UPDATE skill SET stage_origin=? WHERE id=?")
            .bind(stage_origin_tag(StageOrigin::Legacy))
            .bind(&id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// `ALTER TABLE ... ADD COLUMN` has no `IF NOT EXISTS` clause in SQLite, so
/// check `PRAGMA table_info` first. Safe to call on every `open()` — a no-op
/// once the column exists.
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

/// `add_column_if_missing` 的对称件:删一列,先删掉依赖它的索引。
///
/// SQLite 的 `ALTER TABLE ... DROP COLUMN`(3.35+,本仓 libsqlite3-sys 0.30.1
/// 远高于门槛)在列被索引时会直接拒绝,所以 `dependent_indexes` 必须列全 ——
/// 调用方自己知道该列有哪些索引,这里不去猜。列不存在即 no-op,可在每次
/// `open()` 上安全重复调用。
///
/// 用户 2026-08-05 拍板「不能无限制扩展表格,不要害怕修改旧表,有需要就大胆
/// 重做」——把这条做成常备原语而不是一次性代码,是那句话的落地。
///
/// 非原子:`DROP INDEX` 与 `ALTER TABLE ... DROP COLUMN` 是各自独立的
/// autocommit 语句,中途(比如进程被杀)可能只有索引没了、列还在。这是可以
/// 接受的——每一步都是幂等的(索引不存在就 no-op、列不存在整个函数直接
/// 提前返回),下次 `open()` 重跑会自愈到两者都删掉的终态,不需要事务包裹。
async fn drop_column_if_present(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    dependent_indexes: &[&str],
) -> Result<()> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let exists = rows.iter().any(|r| r.get::<String, _>("name") == column);
    if !exists {
        return Ok(());
    }
    for idx in dependent_indexes {
        sqlx::query(&format!("DROP INDEX IF EXISTS {idx}"))
            .execute(pool)
            .await?;
    }
    sqlx::query(&format!("ALTER TABLE {table} DROP COLUMN {column}"))
        .execute(pool)
        .await?;
    Ok(())
}

/// V1 终端会话重构(阶段1): 存量搬运 —— 把 `issue.claude_session_id` 非空
/// 的行搬进新表 `claude_conversation`。幂等:INSERT OR IGNORE(issue_id
/// UNIQUE 兜底),重复 `open()` 不产生重复行。`workspace_path`/
/// `branch_name` 能从 issue + project 推就推(worktree 兄弟路径
/// `<主>-issue-<github_number>` + 分支 `bw/issue-<github_number>`),推
/// 不出留空等首次 open 填。旧列已在本阶段 DROP(见下方
/// `drop_column_if_present`,先搬后删,数据不丢);DROP 后这函数从 PRAGMA
/// 检测到列不在直接 no-op(二次 open 安全)。
async fn migrate_claude_conversations(pool: &SqlitePool) -> Result<()> {
    // 本阶段已 DROP 旧列 → 搬无可搬,no-op。
    let has_legacy = sqlx::query("PRAGMA table_info(issue)")
        .fetch_all(pool)
        .await?
        .iter()
        .any(|r| r.get::<String, _>("name") == "claude_session_id");
    if !has_legacy {
        return Ok(());
    }
    let rows = sqlx::query(
        "SELECT i.id AS issue_id, i.project_id, i.claude_session_id, i.github_number, p.workspace_path
         FROM issue i JOIN project p ON i.project_id = p.id
         WHERE i.claude_session_id != ''",
    )
    .fetch_all(pool)
    .await?;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    for r in rows {
        let issue_id_str: String = r.get("issue_id");
        let project_id_str: String = r.get("project_id");
        let claude_session_id: String = r.get("claude_session_id");
        let github_number: i64 = r.get("github_number");
        let main_ws: String = r.get("workspace_path");
        // branch_name: github_number 非0 推 bw/issue-<n>;否则留空。
        let branch_name = if github_number != 0 {
            format!("bw/issue-{github_number}")
        } else {
            String::new()
        };
        // workspace_path: 推 worktree 兄弟路径 <parent>/<stem>-issue-<n>;
        // 主工作区空 / 无 parent / github_number=0 → 留空(阶段4 resume 回填)。
        let conv_workspace = if github_number != 0 && !main_ws.trim().is_empty() {
            let main = std::path::Path::new(&main_ws);
            match (main.parent(), main.file_name().and_then(|n| n.to_str())) {
                (Some(parent), Some(stem)) => parent
                    .join(format!("{stem}-issue-{github_number}"))
                    .to_string_lossy()
                    .to_string(),
                _ => String::new(),
            }
        } else {
            String::new()
        };
        // buddy 自己的稳定会话 id(new uuid);claude_session_id 是 claude CLI 的。
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT OR IGNORE INTO claude_conversation
             (id, project_id, issue_id, claude_session_id, workspace_path, branch_name, created_at, last_opened_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&project_id_str)
        .bind(&issue_id_str)
        .bind(&claude_session_id)
        .bind(&conv_workspace)
        .bind(&branch_name)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// `ALTER TABLE ... RENAME COLUMN` has no `IF EXISTS` clause, and a fresh DB
/// (whose `schema.sql` already defines the *new* column name) has no *old*
/// column to rename — so check `PRAGMA table_info` first: rename only when the
/// old column is still present. Safe to call on every `open()`. The codebase's
/// first column-rename migration (codehub 对接 2026-07-28: `github_remote`→
/// `remote_path`) — prior migrations were all additive `ADD COLUMN`.
async fn rename_column_if_exists(
    pool: &SqlitePool,
    table: &str,
    old: &str,
    new: &str,
) -> Result<()> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await?;
    let old_exists = rows.iter().any(|r| r.get::<String, _>("name") == old);
    if old_exists {
        sqlx::query(&format!("ALTER TABLE {table} RENAME COLUMN {old} TO {new}"))
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// PF1-R2-1②: data migration (not a schema change — no new column, only
/// renames rows in `cron_task`). Legacy collect_metrics crons built before
/// PF1-3a ① stored `name = "<project> · 指标采集"` (too generic; user read it
/// as unclear what this cron actually does). Rename them to the specific
/// "<project> · 采集代码仓指标" form so `SELECT name` reads back honestly —
/// UI + DB 一致, not a VM-layer display override. Idempotent: the LIKE
/// suffix `'· 指标采集'` only matches rows still on the old generic name; new
/// crons (PF1-3a ①) already use the specific name and are not touched, so
/// re-running on an already-migrated DB is a no-op. Project name is joined
/// from `project` via `project_id` (keeps the name in sync with the project's
/// real display name at migration time; later project renames are out of
/// scope — same append-only name-stability cron_task has always had).
async fn migrate_cron_collect_metrics_name(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query(
        r#"SELECT cron_task.id AS id, project.name AS pname
           FROM cron_task
           JOIN project ON cron_task.project_id = project.id
           WHERE cron_task.mode = 'collect_metrics'
             AND cron_task.name LIKE '% · 指标采集'"#,
    )
    .fetch_all(pool)
    .await?;
    for r in rows {
        let id: String = r.get("id");
        let pname: String = r.get("pname");
        let new_name = format!("{pname} · 采集代码仓指标");
        sqlx::query("UPDATE cron_task SET name = ? WHERE id = ?")
            .bind(&new_name)
            .bind(&id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

fn now_unix() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn pid(p: ProjectId) -> String {
    p.uuid().to_string()
}

fn parse_uuid<T, F: Fn(Uuid) -> T>(s: &str, f: F) -> Result<T> {
    Uuid::parse_str(s)
        .map(f)
        .map_err(|e| StoreError::Other(format!("bad uuid {s:?}: {e}")))
}

fn phase_text(p: Readiness) -> &'static str {
    match p {
        Readiness::Running => "running",
        Readiness::ColdStart => "cold_start",
    }
}
fn parse_phase(s: &str) -> Readiness {
    match s {
        "running" => Readiness::Running,
        _ => Readiness::ColdStart,
    }
}
fn role_text(r: Author) -> &'static str {
    match r {
        Author::Builder => "builder",
        Author::Agent => "agent",
    }
}
fn parse_role(s: &str) -> Author {
    match s {
        "agent" => Author::Agent,
        _ => Author::Builder,
    }
}
fn source_text(s: SourceKind) -> &'static str {
    match s {
        SourceKind::GatewayLog => "gateway_log",
        SourceKind::Ci => "ci",
        SourceKind::GitPr => "git_pr",
        SourceKind::Telemetry => "telemetry",
        SourceKind::Connector => "connector",
        SourceKind::Github => "github",
        SourceKind::Codehub => "codehub",
        SourceKind::Script => "script",
        SourceKind::Manual => "manual",
    }
}
fn parse_source(s: &str) -> SourceKind {
    match s {
        "gateway_log" => SourceKind::GatewayLog,
        "ci" => SourceKind::Ci,
        "git_pr" => SourceKind::GitPr,
        "telemetry" => SourceKind::Telemetry,
        "connector" => SourceKind::Connector,
        "github" => SourceKind::Github,
        "codehub" => SourceKind::Codehub,
        "script" => SourceKind::Script,
        _ => SourceKind::Manual,
    }
}
fn role_metric_text(r: MetricRole) -> &'static str {
    match r {
        MetricRole::Leading => "leading",
        MetricRole::Lagging => "lagging",
    }
}
fn parse_metric_role(s: &str) -> MetricRole {
    match s {
        "lagging" => MetricRole::Lagging,
        _ => MetricRole::Leading,
    }
}
fn metric_origin_text(o: MetricOrigin) -> &'static str {
    match o {
        MetricOrigin::Manual => "manual",
        MetricOrigin::File => "file",
    }
}
fn parse_metric_origin(s: &str) -> MetricOrigin {
    match s {
        "file" => MetricOrigin::File,
        _ => MetricOrigin::Manual,
    }
}
fn session_kind_text(k: SessionKind) -> &'static str {
    match k {
        SessionKind::Create => "create",
        SessionKind::Optimize => "optimize",
    }
}
fn parse_session_kind(s: &str) -> SessionKind {
    match s {
        "optimize" => SessionKind::Optimize,
        _ => SessionKind::Create,
    }
}
fn amber_parts(a: AmberBand) -> (&'static str, f64) {
    match a {
        AmberBand::RelPct(v) => ("rel", v),
        AmberBand::AbsPoints(v) => ("abs", v),
    }
}
fn amber_from(kind: &str, value: f64) -> AmberBand {
    match kind {
        "abs" => AmberBand::AbsPoints(value),
        _ => AmberBand::RelPct(value),
    }
}

/// C6: upsert-by-name for one `.bw/metrics.toml` metric. Looks the row up by
/// `(project, role, name)` — the file's own identity, not a caller-minted
/// id — and either UPDATEs its definition fields in place (keeping the
/// existing row's id, so observation history stays attached) or INSERTs a
/// fresh row with default operational fields (empty target-week plan, rel
/// 10% amber — the same defaults a brand-new `UpsertManualMetric` row gets).
/// Either way `origin` is stamped `File`.
async fn sync_one_metric_definition(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project_id: &str,
    role: MetricRole,
    m: &MetricDefSync,
    t: i64,
) -> Result<()> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM metric WHERE project_id=? AND role=? AND name=?")
            .bind(project_id)
            .bind(role_metric_text(role))
            .bind(&m.name)
            .fetch_optional(&mut **tx)
            .await?;

    match existing {
        Some(id) => {
            // `archived=0` 让规则两边对称:**正本里有 = 在用,正本里没有 =
            // 停用**。上面的自动停用是"没有"那一半,这里是"有"那一半——一条
            // 曾被停用(手动或自动)的指标重新写回正本,下次同步它就回来,
            // 不需要人再去界面上点一次"恢复"。界面因此不给正本来源的指标
            // 停用按钮(点了会被下次同步推翻),只给手建行——见 op.rs 的
            // MetricCard。
            sqlx::query(
                "UPDATE metric SET def=?, target_raw=?, collect_kind=?, collect_query=?,
                    origin=?, archived=0, archived_at=NULL, updated_at=?, rev=rev+1 WHERE id=?",
            )
            .bind(&m.def)
            .bind(&m.target_raw)
            .bind(&m.collect_kind)
            .bind(&m.collect_query)
            .bind(metric_origin_text(MetricOrigin::File))
            .bind(t)
            .bind(&id)
            .execute(&mut **tx)
            .await?;
        }
        None => {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO metric
                    (id, project_id, role, stage_kind, name, def, target_raw, amber_kind,
                     amber_value, last_target, driver, pos, collect_kind, collect_query,
                     origin, created_at, updated_at, rev)
                 VALUES (?, ?, ?, NULL, ?, ?, ?, 'rel', 0.10, '', '', 0, ?, ?, ?, ?, ?, 0)",
            )
            .bind(&id)
            .bind(project_id)
            .bind(role_metric_text(role))
            .bind(&m.name)
            .bind(&m.def)
            .bind(&m.target_raw)
            .bind(&m.collect_kind)
            .bind(&m.collect_query)
            .bind(metric_origin_text(MetricOrigin::File))
            .bind(t)
            .bind(t)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

/// V1 Issue2 Phase 3: upsert-by-name for one `.bw/connectors.toml` connector
/// definition. Parallel to [`sync_one_metric_definition`] — looks the row up
/// by `(project_id, name, kind='script')` and either UPDATEs its `config` in
/// place (keeping the existing row's id, so connector history stays attached)
/// or INSERTs a fresh row with `kind = 'script'` and default operational
/// fields. Only `kind = 'script'` connectors are synced from the file; the
/// `kind='script'` filter on the SELECT ensures a file script connector never
/// collides with an existing non-script connector of the same name (e.g. a
/// `codehub-repo` / `git-repo` connector) — same name + different kind →
/// new INSERT, script row and legacy row coexist, each handled by its own
/// processing path.
async fn sync_one_connector_definition(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project_id: &str,
    c: &ConnectorDefSync,
    t: i64,
) -> Result<()> {
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM connector WHERE project_id=? AND name=? AND kind='script'",
    )
    .bind(project_id)
    .bind(&c.name)
    .fetch_optional(&mut **tx)
    .await?;

    match existing {
        Some(id) => {
            sqlx::query("UPDATE connector SET config=?, updated_at=?, rev=rev+1 WHERE id=?")
                .bind(&c.config)
                .bind(t)
                .bind(&id)
                .execute(&mut **tx)
                .await?;
        }
        None => {
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO connector
                    (id, name, kind, status, last_sync, scope, project_id, config,
                     created_at, updated_at, rev)
                 VALUES (?, ?, 'script', 'disconnected', '', '', ?, ?, ?, ?, 0)",
            )
            .bind(&id)
            .bind(&c.name)
            .bind(project_id)
            .bind(&c.config)
            .bind(t)
            .bind(t)
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

#[async_trait]
impl Store for SqliteStore {
    async fn create_project(&self, p: NewProject) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO project (id, name, kind, descr, phase, cycle, active_stage, provider, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, 'cold_start', 'explore', 'prototype', ?, ?, ?, 0)",
        )
        .bind(pid(p.id))
        .bind(&p.name)
        .bind(&p.kind)
        .bind(&p.desc)
        .bind(&p.provider)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_project(&self, id: ProjectId) -> Result<()> {
        let p = pid(id);
        let mut tx = self.pool.begin().await?;
        // Children-of-children first, then direct project_id children, then
        // the project row itself — explicit order (not ON DELETE CASCADE) so
        // this works the same regardless of which schema.sql version created
        // the on-disk file.
        sqlx::query(
            "DELETE FROM observation WHERE metric_id IN (SELECT id FROM metric WHERE project_id=?)",
        )
        .bind(&p)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM message WHERE session_id IN (SELECT id FROM session WHERE project_id=?)",
        )
        .bind(&p)
        .execute(&mut *tx)
        .await?;
        // A project-owned skill's extra files, and a project-owned workflow's
        // version history — children of rows this fn deletes below, so they
        // have to go first or they outlive their parent as orphans. Global
        // (`project_id IS NULL`) skills/workflows are the shared library and
        // are never touched here.
        sqlx::query(
            "DELETE FROM skill_file WHERE skill_id IN (SELECT id FROM skill WHERE project_id=?)",
        )
        .bind(&p)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM workflow_version WHERE workflow_id IN \
             (SELECT id FROM workflow_spec WHERE project_id=?)",
        )
        .bind(&p)
        .execute(&mut *tx)
        .await?;
        // Direct `project_id` children that this fn used to leave behind
        // entirely: deleting a project stranded its Issues, artifacts, runs,
        // connectors, cron tasks and project-owned components as rows whose
        // `project_id` pointed at a project that no longer existed. They were
        // invisible in the UI (every read filters by a live project) but real
        // in the file — real Issue titles and artifact paths surviving a
        // "delete", which is both wrong and a privacy leak when a DB is
        // shared. Global agents/skills/workflows (`project_id IS NULL`) stay.
        // 会话行挂在 issue 上:先于 issue 删,避免孤儿(阶段1缺口,底座补)。
        sqlx::query("DELETE FROM claude_conversation WHERE project_id=?")
            .bind(&p)
            .execute(&mut *tx)
            .await?;
        for table in [
            "issue",
            "artifact",
            "workflow_run",
            "cron_task",
            "connector",
            "agent",
            "skill",
            "workflow_spec",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE project_id=?"))
                .bind(&p)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM metric WHERE project_id=?")
            .bind(&p)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM op_stage WHERE project_id=?")
            .bind(&p)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM session WHERE project_id=?")
            .bind(&p)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM weekly_review WHERE project_id=?")
            .bind(&p)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM handoff WHERE project_id=?")
            .bind(&p)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM project WHERE id=?")
            .bind(&p)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn delete_session(&self, id: SessionId) -> Result<()> {
        let sid = id.uuid().to_string();
        let mut tx = self.pool.begin().await?;
        // message.session_id has a REFERENCES session(id) FK — must go first.
        sqlx::query("DELETE FROM message WHERE session_id=?")
            .bind(&sid)
            .execute(&mut *tx)
            .await?;
        // issue.session_id has no FK constraint but a dangling pointer is
        // wrong — clear it so a later run re-mints its own session.
        sqlx::query("UPDATE issue SET session_id=NULL WHERE session_id=?")
            .bind(&sid)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM session WHERE id=?")
            .bind(&sid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn set_project_phase(&self, id: ProjectId, phase: Readiness) -> Result<()> {
        sqlx::query("UPDATE project SET phase=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(phase_text(phase))
            .bind(now_unix())
            .bind(pid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_project_cycle(&self, id: ProjectId, cycle: MaturityPeriod) -> Result<()> {
        sqlx::query("UPDATE project SET cycle=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(cycle_text(cycle))
            .bind(now_unix())
            .bind(pid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_project_identity(
        &self,
        id: ProjectId,
        name: &str,
        kind: &str,
        descr: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE project SET name=?, kind=?, descr=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(name)
        .bind(kind)
        .bind(descr)
        .bind(now_unix())
        .bind(pid(id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_north_star(&self, id: ProjectId, north_star: &str, ns_def: &str) -> Result<()> {
        sqlx::query(
            "UPDATE project SET north_star=?, ns_def=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(north_star)
        .bind(ns_def)
        .bind(now_unix())
        .bind(pid(id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_brief(&self, id: ProjectId, benchmark: &str, opportunity: &str) -> Result<()> {
        sqlx::query(
            "UPDATE project SET benchmark=?, opportunity=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(benchmark)
        .bind(opportunity)
        .bind(now_unix())
        .bind(pid(id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_workspace(&self, id: ProjectId, path: &str, allow_commands: bool) -> Result<()> {
        sqlx::query(
            "UPDATE project SET workspace_path=?, allow_commands=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(path)
        .bind(allow_commands as i64)
        .bind(now_unix())
        .bind(pid(id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_remote(&self, id: ProjectId, host: &str, path: &str) -> Result<()> {
        sqlx::query(
            "UPDATE project SET remote_path=?, remote_host=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(path)
        .bind(host)
        .bind(now_unix())
        .bind(pid(id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_metric(&self, m: NewMetric) -> Result<()> {
        let (ak, av) = amber_parts(m.amber);
        let t = now_unix();
        sqlx::query(
            "INSERT INTO metric
                (id, project_id, role, stage_kind, name, def, target_raw, amber_kind, amber_value,
                 last_target, driver, pos, collect_kind, collect_query, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
             ON CONFLICT(id) DO UPDATE SET
                role=excluded.role, stage_kind=excluded.stage_kind, name=excluded.name,
                def=excluded.def, target_raw=excluded.target_raw, amber_kind=excluded.amber_kind,
                amber_value=excluded.amber_value, last_target=excluded.last_target,
                driver=excluded.driver, pos=excluded.pos, updated_at=excluded.updated_at,
                rev=metric.rev+1",
        )
        .bind(m.id.uuid().to_string())
        .bind(pid(m.project_id))
        .bind(role_metric_text(m.role))
        .bind(m.stage_kind.map(stage_kind_text))
        .bind(&m.name)
        .bind(&m.def)
        .bind(&m.target_raw)
        .bind(ak)
        .bind(av)
        .bind(&m.last_target)
        .bind(&m.driver)
        .bind(m.pos)
        .bind(&m.collect_kind)
        .bind(&m.collect_query)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn sync_metrics_file(&self, sync: MetricsFileSync) -> Result<MetricsFileSyncSummary> {
        let p = pid(sync.project_id);
        let t = now_unix();
        let mut tx = self.pool.begin().await?;

        // North star: the exact same UPDATE `set_north_star` performs, plus
        // its collect plan (the two columns that method doesn't touch).
        sqlx::query(
            "UPDATE project SET north_star=?, ns_def=?, north_star_collect_kind=?,
                north_star_collect_query=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(&sync.north_star_name)
        .bind(&sync.north_star_def)
        .bind(&sync.north_star_collect_kind)
        .bind(&sync.north_star_collect_query)
        .bind(t)
        .bind(&p)
        .execute(&mut *tx)
        .await?;

        // W3-1: upsert the north star as a real metric row (role=Leading,
        // stage_kind=NULL = project-level) so collect_project_metrics sees it
        // and recompute_signals derives its signal instead of a forever-grey
        // card. The two project columns above stay as a sync cache; this row
        // is the observation mount point. Same upsert shape (by project_id +
        // role + name) as lagging/leading below — a user-built same-name row
        // is overwritten by origin='file', identical to lagging/leading sync.
        let ns_name = sync.north_star_name.trim();
        if !ns_name.is_empty() {
            let ns_def = MetricDefSync {
                name: ns_name.to_string(),
                def: sync.north_star_def.clone(),
                target_raw: String::new(),
                collect_kind: sync.north_star_collect_kind.clone(),
                collect_query: sync.north_star_collect_query.clone(),
            };
            sync_one_metric_definition(&mut tx, &p, MetricRole::Leading, &ns_def, t).await?;
        }

        for m in &sync.lagging {
            sync_one_metric_definition(&mut tx, &p, MetricRole::Lagging, m, t).await?;
        }
        for m in &sync.leading {
            sync_one_metric_definition(&mut tx, &p, MetricRole::Leading, m, t).await?;
        }

        // 正本里已经消失的行 → 自动停用。作用域死死卡在 `origin='file'`:
        // 只有当初从正本同步进来的行,"正本里没有它了"才是一个成立的判断;
        // 界面手建的 `manual` 行正本里本来就没有,不能被这条规则沉默清场
        // (要停用得人在界面上显式点)。已经 archived 的不重复盖时戳。
        // 一个字节不碰 observation。
        let mut auto_archived = 0u32;
        for (role, defs) in [
            (MetricRole::Lagging, &sync.lagging),
            (MetricRole::Leading, &sync.leading),
        ] {
            let rows = sqlx::query(
                "SELECT id, name FROM metric
                 WHERE project_id=? AND role=? AND origin='file' AND archived=0",
            )
            .bind(&p)
            .bind(role_metric_text(role))
            .fetch_all(&mut *tx)
            .await?;
            // 名字比对在 Rust 侧做,避开 IN (?,?,…) 的动态占位符拼接;
            // 一个项目的指标是十几条量级,不值得为它上 SQL 生成。
            for r in &rows {
                let name: String = r.get("name");
                let is_current_north_star =
                    role == MetricRole::Leading && !ns_name.is_empty() && name == ns_name;
                if is_current_north_star || defs.iter().any(|m| m.name == name) {
                    continue;
                }
                let id: String = r.get("id");
                sqlx::query(
                    "UPDATE metric SET archived=1, archived_at=?, updated_at=?, rev=rev+1 WHERE id=?",
                )
                .bind(t)
                .bind(t)
                .bind(&id)
                .execute(&mut *tx)
                .await?;
                auto_archived += 1;
            }
        }

        tx.commit().await?;
        Ok(MetricsFileSyncSummary {
            lagging_synced: sync.lagging.len() as u32,
            leading_synced: sync.leading.len() as u32,
            auto_archived,
        })
    }

    /// V1 Issue2 Phase 3: upsert all `.bw/connectors.toml` connector
    /// definitions for a project in one atomic transaction. Each connector
    /// is upserted by `(project_id, name)` — existing rows keep their id
    /// (so connector history stays attached), new rows are inserted with
    /// `kind = 'script'` and default operational fields. Only `kind =
    /// 'script'` connectors are synced from the file; other kinds are not
    /// touched (they live in the DB from their creation paths).
    async fn sync_connectors_file(
        &self,
        sync: ConnectorsFileSync,
    ) -> Result<ConnectorsFileSyncSummary> {
        let p = pid(sync.project_id);
        let t = now_unix();
        let mut tx = self.pool.begin().await?;

        for c in &sync.connectors {
            sync_one_connector_definition(&mut tx, &p, c, t).await?;
        }

        tx.commit().await?;
        Ok(ConnectorsFileSyncSummary {
            connectors_synced: sync.connectors.len() as u32,
        })
    }

    async fn sync_project_file(&self, sync: ProjectFileSync) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "UPDATE project SET name=?, kind=?, descr=?, benchmark=?, opportunity=?,
                updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(&sync.name)
        .bind(&sync.kind)
        .bind(&sync.brief)
        .bind(&sync.benchmark)
        .bind(&sync.opportunity)
        .bind(t)
        .bind(pid(sync.project_id))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn set_metric_archived(&self, metric: MetricId, archived: bool) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "UPDATE metric SET archived=?, archived_at=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(archived as i64)
        // 恢复时清掉时戳:archived_at 只在"当下正处于停用中"时才有意义,
        // 留着一个陈旧的停用时刻会让人误以为它还停着。
        .bind(if archived { Some(t) } else { None })
        .bind(t)
        .bind(metric.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_week_plan(
        &self,
        metric: MetricId,
        new_target: &str,
        last_target: &str,
        driver: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE metric SET target_raw=?, last_target=?, driver=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(new_target)
        .bind(last_target)
        .bind(driver)
        .bind(now_unix())
        .bind(metric.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn append_observation(
        &self,
        metric_id: MetricId,
        source: SourceKind,
        raw: &str,
        ts: OffsetDateTime,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO observation (id, metric_id, ts, source_kind, raw, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(metric_id.uuid().to_string())
        .bind(ts.unix_timestamp())
        .bind(source_text(source))
        .bind(raw)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn latest_observation_ts(&self, metric_id: MetricId) -> Result<Option<i64>> {
        let row = sqlx::query("SELECT MAX(ts) AS ts FROM observation WHERE metric_id = ?")
            .bind(metric_id.uuid().to_string())
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get::<Option<i64>, _>("ts")?)
    }

    async fn materialize_stages(&self, stages: Vec<NewStage>) -> Result<()> {
        let t = now_unix();
        for s in stages {
            let dod = serde_json::to_string(&vec![false; s.kind.dod_items().len()])?;
            sqlx::query(
                "INSERT INTO op_stage
                    (id, project_id, kind, progress, dod, routine_schedule,
                     created_at, updated_at, rev)
                 VALUES (?, ?, ?, 0, ?, ?, ?, ?, 0)
                 ON CONFLICT(project_id, kind) DO NOTHING",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(pid(s.project_id))
            .bind(stage_kind_text(s.kind))
            .bind(dod)
            .bind(cadence_text(&s.schedule))
            .bind(t)
            .bind(t)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn set_stage_progress(
        &self,
        project_id: ProjectId,
        kind: StageKind,
        progress: u8,
    ) -> Result<()> {
        let progress = progress.min(100);
        let row = sqlx::query("SELECT id, trend FROM op_stage WHERE project_id=? AND kind=?")
            .bind(pid(project_id))
            .bind(stage_kind_text(kind))
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::Other("stage not materialized".into()))?;
        let sid: String = row.get("id");
        let mut trend: Vec<f32> =
            serde_json::from_str(&row.get::<String, _>("trend")).unwrap_or_default();
        trend.push(f32::from(progress));
        sqlx::query("UPDATE op_stage SET progress=?, trend=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(i64::from(progress))
            .bind(serde_json::to_string(&trend)?)
            .bind(now_unix())
            .bind(&sid)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn toggle_dod(&self, project_id: ProjectId, kind: StageKind, index: usize) -> Result<()> {
        let row = sqlx::query("SELECT id, dod FROM op_stage WHERE project_id=? AND kind=?")
            .bind(pid(project_id))
            .bind(stage_kind_text(kind))
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::Other("stage not materialized".into()))?;
        let sid: String = row.get("id");
        let mut dod: Vec<bool> =
            serde_json::from_str(&row.get::<String, _>("dod")).unwrap_or_default();
        if let Some(v) = dod.get_mut(index) {
            *v = !*v;
        } else {
            return Err(StoreError::Other(format!("dod index {index} out of range")));
        }
        sqlx::query("UPDATE op_stage SET dod=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(serde_json::to_string(&dod)?)
            .bind(now_unix())
            .bind(&sid)
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
        at: OffsetDateTime,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO handoff (id, project_id, from_stage, to_stage, risky, note, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(pid(project_id))
        .bind(stage_kind_text(from))
        .bind(stage_kind_text(to))
        .bind(risky as i64)
        .bind(note)
        .bind(at.unix_timestamp())
        .execute(&self.pool)
        .await?;
        sqlx::query("UPDATE project SET active_stage=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(stage_kind_text(to))
            .bind(now_unix())
            .bind(pid(project_id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn ensure_session(&self, s: NewSession) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO session (id, project_id, stage_kind, kind, title, snippet, status, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
             ON CONFLICT(id) DO NOTHING",
        )
        .bind(s.id.uuid().to_string())
        .bind(pid(s.project_id))
        .bind(s.stage_kind.map(stage_kind_text))
        .bind(session_kind_text(s.kind))
        .bind(&s.title)
        .bind(&s.snippet)
        .bind(session_status_text(bw_core::model::SessionStatus::Active))
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn append_message(&self, session_id: SessionId, role: Author, text: &str) -> Result<()> {
        let sid = session_id.uuid().to_string();
        let seq: i64 = sqlx::query(
            "SELECT COALESCE(MAX(seq), -1) + 1 AS next FROM message WHERE session_id=?",
        )
        .bind(&sid)
        .fetch_one(&self.pool)
        .await?
        .get("next");
        sqlx::query(
            "INSERT INTO message (id, session_id, seq, role, text, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&sid)
        .bind(seq)
        .bind(role_text(role))
        .bind(text)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn recompute_signals(&self, project_id: ProjectId, now: OffsetDateTime) -> Result<()> {
        let p = pid(project_id);
        let t = now.unix_timestamp();

        // Stage cadence + ids.
        let stage_rows =
            sqlx::query("SELECT id, kind, routine_schedule FROM op_stage WHERE project_id=?")
                .bind(&p)
                .fetch_all(&self.pool)
                .await?;
        let mut stage_cadence: HashMap<StageKind, _> = HashMap::new();
        let mut stages: Vec<(StageKind, String)> = Vec::new();
        for r in &stage_rows {
            let kind = parse_stage_kind(&r.get::<String, _>("kind"))
                .ok_or_else(|| StoreError::Other("bad stage kind".into()))?;
            stage_cadence.insert(kind, parse_cadence(&r.get::<String, _>("routine_schedule")));
            stages.push((kind, r.get::<String, _>("id")));
        }

        // L1→L3: each metric's signal from its latest observation vs its target.
        // `archived=0`:停用的指标整条退出派生 —— 既不再重算它自己那盏灯
        // (缓存冻结在停用那一刻),也不进下面的 by_stage/by_project 上卷
        // (一条被判定为坏指标的行不该再把项目健康灯往任何方向拽)。
        // 恢复后它自然回到这个 SELECT 里,由下一次 recompute 重新派生。
        // derive-only 不变:recompute_signals 仍是 signal 的唯一写入者。
        let metric_rows = sqlx::query(
            "SELECT id, stage_kind, target_raw, amber_kind, amber_value
             FROM metric WHERE project_id=? AND archived=0",
        )
        .bind(&p)
        .fetch_all(&self.pool)
        .await?;
        let mut by_stage: HashMap<StageKind, Vec<Signal>> = HashMap::new();
        // plan18-④ · 项目级指标(stage_kind=NULL,如北极星/项目级引领·滞后)
        // 的 signal 收集——L6 项目健康灯要把它们也卷入(否则业务北极星点亮
        // 了项目卡还是灰,违背"北极星驱动项目健康"的产品哲学)。补缝,非原
        // 设计:derive-only/recompute 唯一写入者不变,只是聚合多卷一组。
        let mut by_project: Vec<Signal> = Vec::new();
        for m in &metric_rows {
            let mid: String = m.get("id");
            let stage_kind = m
                .get::<Option<String>, _>("stage_kind")
                .and_then(|s| parse_stage_kind(&s));
            let target_raw: String = m.get("target_raw");
            let amber = amber_from(
                &m.get::<String, _>("amber_kind"),
                m.get::<f64, _>("amber_value"),
            );
            let cadence = stage_kind
                .and_then(|k| stage_cadence.get(&k).cloned())
                .unwrap_or(bw_core::model::Cadence::Daily);

            // rowid tie-break: ts is unix-seconds, so two appends in the same
            // second must still resolve to the later insertion.
            let obs = sqlx::query(
                "SELECT raw, ts, source_kind FROM observation WHERE metric_id=? ORDER BY ts DESC, rowid DESC LIMIT 1",
            )
            .bind(&mid)
            .fetch_optional(&self.pool)
            .await?;
            let measurement = match obs {
                Some(o) => measure(
                    &o.get::<String, _>("raw"),
                    OffsetDateTime::from_unix_timestamp(o.get::<i64, _>("ts"))
                        .unwrap_or(OffsetDateTime::UNIX_EPOCH),
                    parse_source(&o.get::<String, _>("source_kind")),
                    &cadence,
                    now,
                ),
                None => Measurement::Missing,
            };

            let (signal, hit) = match parse_target_with(&target_raw, amber) {
                Ok(target) => {
                    let e = evaluate_metric(&measurement, &target, &[]);
                    (e.signal(), e.hit)
                }
                // Unparseable target ⇒ Unknown (never green), surfaced as a lint upstream.
                Err(_) => (Signal::Unknown, false),
            };

            sqlx::query(
                "UPDATE metric SET signal=?, hit=?, signal_derived_rev=COALESCE(signal_derived_rev,0)+1,
                                   updated_at=?, rev=rev+1 WHERE id=?",
            )
            .bind(sig_text(signal))
            .bind(hit as i64)
            .bind(t)
            .bind(&mid)
            .execute(&self.pool)
            .await?;

            if let Some(k) = stage_kind {
                by_stage.entry(k).or_default().push(signal);
            } else {
                // plan18-④:项目级指标进 by_project,供 L6 上卷。
                by_project.push(signal);
            }
        }

        // L4: routine signal per stage = worst-of its metrics.
        let mut stage_signal: HashMap<StageKind, Signal> = HashMap::new();
        for (kind, sid) in &stages {
            let sigs = by_stage.get(kind).cloned().unwrap_or_default();
            let rolled = reduce_worst_of(sigs).into_inner();
            stage_signal.insert(*kind, rolled);
            sqlx::query(
                "UPDATE op_stage SET routine_signal=?, routine_signal_rev=COALESCE(routine_signal_rev,0)+1,
                                     updated_at=?, rev=rev+1 WHERE id=?",
            )
            .bind(sig_text(rolled))
            .bind(t)
            .bind(sid)
            .execute(&self.pool)
            .await?;
        }

        // L6: project signal = worst-of(各阶段聚合 + 项目级业务指标);
        // weekly_signal = snapshot。plan18-④:项目级指标(北极星/L1/L2/L3
        // 等业务指标)原本不上卷(只更新自己那行),导致业务北极星点亮了
        // 项目卡还是灰——补:把 by_project 也卷入 worst-of。reduce_worst_of
        // 有 Green 就不 Unknown,北极星 Green 能拉亮项目灯;北极星 Red→项目
        // Red,语义对(业务北极星该驱动项目健康)。
        let mut proj_inputs: Vec<Signal> = stages.iter().map(|(k, _)| stage_signal[k]).collect();
        proj_inputs.extend(by_project.iter().copied());
        let proj = reduce_worst_of(proj_inputs).into_inner();
        sqlx::query(
            "UPDATE project SET signal=?, weekly_signal=?, signal_derived_rev=COALESCE(signal_derived_rev,0)+1,
                                signal_derived_at=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(sig_text(proj))
        .bind(sig_text(proj))
        .bind(t)
        .bind(t)
        .bind(&p)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn annotate_weekly_review(
        &self,
        project_id: ProjectId,
        week_of: OffsetDateTime,
        derived: Signal,
        human_override: Option<Signal>,
        reason: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO weekly_review (id, project_id, week_of, derived_signal, human_override, override_reason, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(pid(project_id))
        .bind(week_of.unix_timestamp())
        .bind(sig_text(derived))
        .bind(human_override.map(sig_text))
        .bind(reason)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_project(&self, id: ProjectId) -> Result<Option<ProjectRow>> {
        let row = sqlx::query(
            "SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, remote_path, remote_host, north_star_collect_kind, north_star_collect_query, provider, signal, weekly_signal, created_at
             FROM project WHERE id=?",
        )
        .bind(pid(id))
        .fetch_optional(&self.pool)
        .await?;
        row.map(project_row).transpose()
    }

    async fn list_projects(&self) -> Result<Vec<ProjectRow>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, remote_path, remote_host, north_star_collect_kind, north_star_collect_query, provider, signal, weekly_signal, created_at
             FROM project ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(project_row).collect()
    }

    async fn persisted_signals(&self, id: ProjectId) -> Result<PersistedSignals> {
        let p = pid(id);
        let proj = sqlx::query("SELECT signal, weekly_signal FROM project WHERE id=?")
            .bind(&p)
            .fetch_one(&self.pool)
            .await?;

        let stage_rows =
            sqlx::query("SELECT kind, routine_signal FROM op_stage WHERE project_id=?")
                .bind(&p)
                .fetch_all(&self.pool)
                .await?;
        let mut stages = Vec::new();
        for r in stage_rows {
            if let Some(kind) = parse_stage_kind(&r.get::<String, _>("kind")) {
                stages.push(StageSignal {
                    kind,
                    routine: r
                        .get::<Option<String>, _>("routine_signal")
                        .and_then(|s| parse_sig(&s)),
                });
            }
        }

        let metric_rows = sqlx::query(
            "SELECT m.id, m.name, m.role, m.def, m.target_raw, m.last_target, m.driver, m.stage_kind, m.signal, m.hit,
                    m.collect_kind, m.collect_query, m.origin, m.archived,
                    (SELECT raw FROM observation o WHERE o.metric_id = m.id ORDER BY ts DESC, rowid DESC LIMIT 1) AS value_raw,
                    (SELECT source_kind FROM observation o WHERE o.metric_id = m.id ORDER BY ts DESC, rowid DESC LIMIT 1) AS src
             FROM metric m WHERE m.project_id=? ORDER BY m.pos",
        )
        .bind(&p)
        .fetch_all(&self.pool)
        .await?;
        let metrics = metric_rows
            .into_iter()
            .map(|r| {
                let id = parse_uuid(&r.get::<String, _>("id"), MetricId::from_uuid)?;
                Ok(MetricSignal {
                    id,
                    name: r.get("name"),
                    role: parse_metric_role(&r.get::<String, _>("role")),
                    def: r.get("def"),
                    value_raw: r.get::<Option<String>, _>("value_raw").unwrap_or_default(),
                    target_raw: r.get("target_raw"),
                    last_target: r.get("last_target"),
                    driver: r.get("driver"),
                    stage_kind: r
                        .get::<Option<String>, _>("stage_kind")
                        .and_then(|s| parse_stage_kind(&s)),
                    source: r.get::<Option<String>, _>("src").map(|s| parse_source(&s)),
                    signal: r
                        .get::<Option<String>, _>("signal")
                        .and_then(|s| parse_sig(&s)),
                    hit: r.get::<Option<i64>, _>("hit").map(|v| v != 0),
                    collect_kind: r.get("collect_kind"),
                    collect_query: r.get("collect_query"),
                    origin: parse_metric_origin(&r.get::<String, _>("origin")),
                    archived: r.get::<i64, _>("archived") != 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(PersistedSignals {
            project: proj
                .get::<Option<String>, _>("signal")
                .and_then(|s| parse_sig(&s)),
            weekly: proj
                .get::<Option<String>, _>("weekly_signal")
                .and_then(|s| parse_sig(&s)),
            stages,
            metrics,
        })
    }

    async fn list_stages(&self, project_id: ProjectId) -> Result<Vec<StageRow>> {
        let rows = sqlx::query(
            "SELECT kind, progress, trend, dod, routine_schedule, routine_signal
             FROM op_stage WHERE project_id=?",
        )
        .bind(pid(project_id))
        .fetch_all(&self.pool)
        .await?;
        let mut stages: Vec<StageRow> = rows
            .into_iter()
            .filter_map(|r| {
                let kind = parse_stage_kind(&r.get::<String, _>("kind"))?;
                Some(StageRow {
                    kind,
                    progress: r.get::<i64, _>("progress").clamp(0, 100) as u8,
                    trend: serde_json::from_str(&r.get::<String, _>("trend")).unwrap_or_default(),
                    dod: serde_json::from_str(&r.get::<String, _>("dod")).unwrap_or_default(),
                    schedule: parse_cadence(&r.get::<String, _>("routine_schedule")),
                    routine_signal: r
                        .get::<Option<String>, _>("routine_signal")
                        .and_then(|s| parse_sig(&s)),
                })
            })
            .collect();
        // Loop order, not insertion order.
        stages.sort_by_key(|s| s.kind.index());
        Ok(stages)
    }

    async fn list_observations(&self, project_id: ProjectId) -> Result<Vec<ObservationRow>> {
        let rows = sqlx::query(
            "SELECT o.metric_id, o.ts, o.source_kind, o.raw
             FROM observation o JOIN metric m ON m.id = o.metric_id
             WHERE m.project_id=? ORDER BY o.ts ASC, o.rowid ASC",
        )
        .bind(pid(project_id))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let metric_id = parse_uuid(&r.get::<String, _>("metric_id"), MetricId::from_uuid)?;
                Ok(ObservationRow {
                    metric_id,
                    ts: OffsetDateTime::from_unix_timestamp(r.get::<i64, _>("ts"))
                        .unwrap_or(OffsetDateTime::UNIX_EPOCH),
                    source: parse_source(&r.get::<String, _>("source_kind")),
                    raw: r.get("raw"),
                })
            })
            .collect()
    }

    async fn list_handoffs(&self, project_id: ProjectId) -> Result<Vec<HandoffRow>> {
        let rows = sqlx::query(
            "SELECT from_stage, to_stage, risky, note, created_at
             FROM handoff WHERE project_id=? ORDER BY created_at DESC, rowid DESC",
        )
        .bind(pid(project_id))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let from_stage = parse_stage_kind(&r.get::<String, _>("from_stage"))?;
                let to_stage = parse_stage_kind(&r.get::<String, _>("to_stage"))?;
                Some(HandoffRow {
                    from_stage,
                    to_stage,
                    risky: r.get::<i64, _>("risky") != 0,
                    note: r.get("note"),
                    at: OffsetDateTime::from_unix_timestamp(r.get::<i64, _>("created_at"))
                        .unwrap_or(OffsetDateTime::UNIX_EPOCH),
                })
            })
            .collect())
    }

    async fn list_recent_handoffs(&self, limit: u32) -> Result<Vec<GlobalHandoffRow>> {
        let rows = sqlx::query(
            "SELECT h.from_stage, h.to_stage, h.risky, h.note, h.created_at,
                    p.id AS project_id, p.name AS project_name
             FROM handoff h JOIN project p ON p.id = h.project_id
             ORDER BY h.created_at DESC, h.rowid DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| {
                let from_stage = parse_stage_kind(&r.get::<String, _>("from_stage"))?;
                let to_stage = parse_stage_kind(&r.get::<String, _>("to_stage"))?;
                let project_id =
                    parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid).ok()?;
                Some(GlobalHandoffRow {
                    project_id,
                    project_name: r.get("project_name"),
                    from_stage,
                    to_stage,
                    risky: r.get::<i64, _>("risky") != 0,
                    note: r.get("note"),
                    at: OffsetDateTime::from_unix_timestamp(r.get::<i64, _>("created_at"))
                        .unwrap_or(OffsetDateTime::UNIX_EPOCH),
                })
            })
            .collect())
    }

    async fn list_sessions(&self, project_id: ProjectId) -> Result<Vec<SessionRow>> {
        let rows = sqlx::query(
            "SELECT id, title, kind, stage_kind, status FROM session WHERE project_id=? ORDER BY created_at",
        )
        .bind(pid(project_id))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                let id = parse_uuid(&r.get::<String, _>("id"), SessionId::from_uuid)?;
                Ok(SessionRow {
                    id,
                    title: r.get("title"),
                    kind: parse_session_kind(&r.get::<String, _>("kind")),
                    stage_kind: r
                        .get::<Option<String>, _>("stage_kind")
                        .and_then(|s| parse_stage_kind(&s)),
                    status: parse_session_status(&r.get::<String, _>("status")),
                })
            })
            .collect()
    }

    async fn session_messages(&self, session_id: SessionId) -> Result<Vec<MessageRow>> {
        let rows = sqlx::query("SELECT role, text FROM message WHERE session_id=? ORDER BY seq")
            .bind(session_id.uuid().to_string())
            .fetch_all(&self.pool)
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| MessageRow {
                role: parse_role(&r.get::<String, _>("role")),
                text: r.get("text"),
            })
            .collect())
    }

    // ── hub library (global — no active-project gate) ──

    async fn create_workflow_spec(&self, w: NewWorkflowSpec) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO workflow_spec
                (id, name, kind_json, prompt, goal, stage_ref, phases, phase_prompts, agents_json,
                 skills_json, loop_retries, loop_max_iter, project_id, content, created_at,
                 updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(w.id.uuid().to_string())
        .bind(&w.name)
        .bind(serde_json::to_string(&w.kind)?)
        .bind(&w.prompt)
        .bind(&w.goal)
        .bind(w.stage_ref.map(i64::from))
        .bind(serde_json::to_string(&w.phases)?)
        .bind(serde_json::to_string(&w.phase_prompts)?)
        .bind(serde_json::to_string(&w.agents)?)
        .bind(serde_json::to_string(&w.skills)?)
        .bind(i64::from(w.loop_config.retries))
        .bind(i64::from(w.loop_config.max_iter))
        .bind(w.project_id.map(pid))
        .bind(&w.content)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_workflow_specs(&self) -> Result<Vec<WorkflowSpec>> {
        let rows = sqlx::query(
            "SELECT id, name, kind_json, prompt, goal, stage_ref, phases, phase_prompts,
                    agents_json, skills_json, loop_retries, loop_max_iter, project_id, content
             FROM workflow_spec ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(workflow_spec_row).collect()
    }

    async fn get_workflow_spec(&self, id: WorkflowId) -> Result<Option<WorkflowSpec>> {
        let row = sqlx::query(
            "SELECT id, name, kind_json, prompt, goal, stage_ref, phases, phase_prompts,
                    agents_json, skills_json, loop_retries, loop_max_iter, project_id, content
             FROM workflow_spec WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(workflow_spec_row).transpose()
    }

    async fn promote_workflow(
        &self,
        new_id: WorkflowId,
        from: &WorkflowSpec,
        source: HubSource,
    ) -> Result<()> {
        let kind = WorkflowKind::Static {
            maturity: Maturity::Fresh,
            version: 1,
            uses: 0,
            scope: String::new(),
            source,
            trigger: None,
        };
        let t = now_unix();
        sqlx::query(
            "INSERT INTO workflow_spec
                (id, name, kind_json, prompt, goal, stage_ref, phases, phase_prompts, agents_json,
                 skills_json, loop_retries, loop_max_iter, content, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(new_id.uuid().to_string())
        .bind(&from.name)
        .bind(serde_json::to_string(&kind)?)
        .bind(&from.prompt)
        .bind(&from.goal)
        .bind(from.stage_ref.map(i64::from))
        .bind(serde_json::to_string(&from.phases)?)
        .bind(serde_json::to_string(&from.phase_prompts)?)
        .bind(serde_json::to_string(&from.agents)?)
        .bind(serde_json::to_string(&from.skills)?)
        .bind(i64::from(from.loop_config.retries))
        .bind(i64::from(from.loop_config.max_iter))
        .bind(&from.content)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn record_workflow_use(&self, id: WorkflowId) -> Result<()> {
        let row = sqlx::query("SELECT kind_json FROM workflow_spec WHERE id=?")
            .bind(id.uuid().to_string())
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| StoreError::Other("workflow spec not found".into()))?;
        let mut kind: WorkflowKind = serde_json::from_str(&row.get::<String, _>("kind_json"))?;
        if let WorkflowKind::Static { uses, .. } = &mut kind {
            *uses += 1;
        }
        sqlx::query("UPDATE workflow_spec SET kind_json=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(serde_json::to_string(&kind)?)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn refresh_workflow_template_phases(
        &self,
        id: WorkflowId,
        phases: Vec<PhaseMeta>,
        phase_prompts: Vec<String>,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE workflow_spec SET phases=?, phase_prompts=?, updated_at=?, rev=rev+1
             WHERE id=?",
        )
        .bind(serde_json::to_string(&phases)?)
        .bind(serde_json::to_string(&phase_prompts)?)
        .bind(now_unix())
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_workflow_spec(&self, id: WorkflowId) -> Result<()> {
        sqlx::query("DELETE FROM workflow_spec WHERE id=?")
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn record_workflow_run_start(&self, run: NewWorkflowRun<'_>) -> Result<WorkflowRunId> {
        let id = WorkflowRunId::from_uuid(Uuid::new_v4());
        sqlx::query(
            "INSERT INTO workflow_run
             (id, workflow_id, workflow_name, project_id, session_id, trigger,
              status, started_at, finished_at, duration_ms, phases_completed,
              error, params_json, cron_task_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, 'running', ?, NULL, NULL, 0, '', ?, ?, ?)",
        )
        .bind(id.uuid().to_string())
        .bind(run.workflow_id.uuid().to_string())
        .bind(run.workflow_name)
        .bind(run.project_id.map(|p| p.uuid().to_string()))
        .bind(run.session_id.map(|s| s.uuid().to_string()))
        .bind(run.trigger.text())
        .bind(run.started_at)
        .bind(run.params_json)
        .bind(run.cron_task_id.map(|t| t.uuid().to_string()))
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(id)
    }

    async fn set_run_issue(&self, run_id: WorkflowRunId, issue_id: IssueId) -> Result<()> {
        sqlx::query("UPDATE workflow_run SET issue_id=? WHERE id=?")
            .bind(issue_id.uuid().to_string())
            .bind(run_id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_run_heads(
        &self,
        run_id: WorkflowRunId,
        head_before: Option<String>,
        head_after: Option<String>,
    ) -> Result<()> {
        sqlx::query("UPDATE workflow_run SET head_before=?, head_after=? WHERE id=?")
            .bind(head_before)
            .bind(head_after)
            .bind(run_id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn settle_workflow_run(
        &self,
        id: WorkflowRunId,
        status: RunStatus,
        finished_at: i64,
        duration_ms: i64,
        phases_completed: u32,
        error: &str,
    ) -> Result<()> {
        // Idempotent: a row already settled to a terminal state is left as-is
        // so a re-driven dogfood round never overwrites a real past outcome.
        let existing = sqlx::query("SELECT status FROM workflow_run WHERE id=?")
            .bind(id.uuid().to_string())
            .fetch_optional(&self.pool)
            .await?;
        match existing {
            None => Ok(()), // nothing to settle — honest no-op
            Some(row) => {
                let cur: String = row.get("status");
                if cur != "running" {
                    return Ok(()); // already terminal
                }
                sqlx::query(
                    "UPDATE workflow_run
                     SET status=?, finished_at=?, duration_ms=?, phases_completed=?, error=?
                     WHERE id=? AND status='running'",
                )
                .bind(status.text())
                .bind(finished_at)
                .bind(duration_ms)
                .bind(phases_completed as i64)
                .bind(error)
                .bind(id.uuid().to_string())
                .execute(&self.pool)
                .await?;
                Ok(())
            }
        }
    }

    async fn list_workflow_runs(&self, workflow_id: WorkflowId) -> Result<Vec<WorkflowRun>> {
        let rows = sqlx::query(
            "SELECT id, workflow_id, workflow_name, project_id, session_id, trigger, status,
                    started_at, finished_at, duration_ms, phases_completed, error, params_json, cron_task_id, issue_id, head_before, head_after
             FROM workflow_run WHERE workflow_id=? ORDER BY started_at DESC, rowid DESC",
        )
        .bind(workflow_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(parse_run_row).collect())
    }

    async fn list_all_workflow_runs(&self, limit: u32) -> Result<Vec<WorkflowRun>> {
        let rows = sqlx::query(
            "SELECT id, workflow_id, workflow_name, project_id, session_id, trigger, status,
                    started_at, finished_at, duration_ms, phases_completed, error, params_json, cron_task_id, issue_id, head_before, head_after
             FROM workflow_run ORDER BY started_at DESC, rowid DESC LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(parse_run_row).collect())
    }

    async fn workflow_analytics(&self, workflow_id: WorkflowId) -> Result<WorkflowRunAnalytics> {
        // One aggregation query: counts + mean over settled runs. Median is
        // computed in Rust over the fetched series (SQLite has no native
        // MEDIAN), which also gives us last_run_at/last_status in the same
        // pass. A workflow with no rows returns total_runs=0, success_rate=None.
        let agg = sqlx::query(
            "SELECT
                COUNT(*)                                               AS total,
                SUM(CASE WHEN status='ok'      THEN 1 ELSE 0 END)     AS ok_n,
                SUM(CASE WHEN status='failed'  THEN 1 ELSE 0 END)     AS fail_n,
                SUM(CASE WHEN status='running' THEN 1 ELSE 0 END)     AS run_n,
                AVG(CASE WHEN status IN ('ok','failed') THEN duration_ms END) AS avg_dur
             FROM workflow_run WHERE workflow_id=?",
        )
        .bind(workflow_id.uuid().to_string())
        .fetch_one(&self.pool)
        .await?;
        let total: i64 = agg.get("total");
        let ok_runs: i64 = agg.get("ok_n");
        let failed_runs: i64 = agg.get("fail_n");
        let running_runs: i64 = agg.get("run_n");
        let avg_dur: Option<f64> = agg.get("avg_dur");
        let settled = ok_runs + failed_runs;

        // Name + last run + the duration series (for median) in one fetch.
        let name_row = sqlx::query("SELECT workflow_name, started_at, status FROM workflow_run WHERE workflow_id=? ORDER BY started_at DESC, rowid DESC LIMIT 1")
            .bind(workflow_id.uuid().to_string())
            .fetch_optional(&self.pool)
            .await?;
        let (workflow_name, last_run_at, last_status) = match name_row {
            Some(r) => (
                r.get::<String, _>("workflow_name"),
                Some(r.get::<i64, _>("started_at")),
                Some(RunStatus::parse(&r.get::<String, _>("status"))),
            ),
            None => (String::new(), None, None),
        };

        // Median over settled durations — robust to a single slow outlier.
        let median = if settled > 0 {
            let dur_rows = sqlx::query(
                "SELECT duration_ms FROM workflow_run
                 WHERE workflow_id=? AND status IN ('ok','failed') AND duration_ms IS NOT NULL
                 ORDER BY duration_ms",
            )
            .bind(workflow_id.uuid().to_string())
            .fetch_all(&self.pool)
            .await?;
            let ds: Vec<i64> = dur_rows
                .iter()
                .map(|r| r.get::<i64, _>("duration_ms"))
                .collect();
            if ds.is_empty() {
                None
            } else {
                let mid = ds.len() / 2;
                Some(if ds.len() % 2 == 0 {
                    (ds[mid - 1] + ds[mid]) / 2
                } else {
                    ds[mid]
                })
            }
        } else {
            None
        };

        Ok(WorkflowRunAnalytics {
            workflow_id,
            workflow_name,
            total_runs: total as u32,
            ok_runs: ok_runs as u32,
            failed_runs: failed_runs as u32,
            running_runs: running_runs as u32,
            success_rate: if settled > 0 {
                Some(ok_runs as f32 / settled as f32)
            } else {
                None
            },
            avg_duration_ms: avg_dur.map(|v| v as i64),
            median_duration_ms: median,
            last_run_at,
            last_status,
        })
    }

    async fn cron_effectiveness(&self, cron_task_id: CronTaskId) -> Result<CronEffectiveness> {
        // Only runs this task auto-fired (trigger='scheduled' AND linked to
        // this task). Manual runs of the same workflow are excluded — a
        // schedule's track record is its own, not contaminated by ad-hoc fires.
        let row = sqlx::query(
            "SELECT
                COUNT(*)                                                         AS fires,
                SUM(CASE WHEN status='ok'     THEN 1 ELSE 0 END)                AS ok_n,
                SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END)                AS fail_n,
                AVG(CASE WHEN status IN ('ok','failed') THEN duration_ms END)   AS avg_dur,
                MAX(started_at)                                                  AS last_at
             FROM workflow_run WHERE cron_task_id=? AND trigger='scheduled'",
        )
        .bind(cron_task_id.uuid().to_string())
        .fetch_one(&self.pool)
        .await?;
        let fires: i64 = row.get("fires");
        let ok_fires: i64 = row.get("ok_n");
        let failed_fires: i64 = row.get("fail_n");
        let avg_dur: Option<f64> = row.get("avg_dur");
        let last_at: Option<i64> = row.get("last_at");
        let last_fire_ok = if fires > 0 {
            // Read the most recent fire's status in a second cheap query —
            // keeping it separate avoids a window-function dependency.
            let last = sqlx::query(
                "SELECT status FROM workflow_run WHERE cron_task_id=? AND trigger='scheduled'
                 ORDER BY started_at DESC, rowid DESC LIMIT 1",
            )
            .bind(cron_task_id.uuid().to_string())
            .fetch_one(&self.pool)
            .await?;
            Some(RunStatus::parse(&last.get::<String, _>("status")) == RunStatus::Ok)
        } else {
            None
        };
        Ok(CronEffectiveness {
            cron_task_id,
            fires: fires as u32,
            ok_fires: ok_fires as u32,
            failed_fires: failed_fires as u32,
            effectiveness: if fires > 0 {
                Some(ok_fires as f32 / fires as f32)
            } else {
                None
            },
            avg_duration_ms: avg_dur.map(|v| v as i64),
            last_fire_at: last_at,
            last_fire_ok,
        })
    }

    async fn update_workflow_spec(&self, id: WorkflowId, edit: WorkflowEdit) -> Result<()> {
        // iter 5: snapshot the CURRENT content into workflow_version BEFORE
        // the overwrite — so the evolution history survives. Read everything
        // the version row needs in one fetch.
        let cur = sqlx::query(
            "SELECT kind_json, name, prompt, goal, phases, phase_prompts, agents_json, skills_json
             FROM workflow_spec WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| StoreError::Other("workflow spec not found".into()))?;
        let kind: WorkflowKind = serde_json::from_str(&cur.get::<String, _>("kind_json"))?;
        let (old_version, is_static) = match &kind {
            WorkflowKind::Static { version, .. } => (*version, true),
            WorkflowKind::Dynamic { .. } => (0, false),
        };
        if !is_static {
            return Err(StoreError::Other("动态工作流没有持久内容可优化".into()));
        }
        // Bump the version on the existing kind, preserving every other Static
        // field (maturity/uses/scope/source/trigger) untouched.
        let new_kind = match kind {
            WorkflowKind::Static {
                maturity,
                version: _,
                uses,
                scope,
                source,
                trigger,
            } => WorkflowKind::Static {
                maturity,
                version: old_version + 1,
                uses,
                scope,
                source,
                trigger,
            },
            other => other,
        };
        // Freeze the about-to-be-replaced content as version `old_version`.
        sqlx::query(
            "INSERT INTO workflow_version
             (id, workflow_id, version, name, prompt, goal, phases, phase_prompts, agents_json,
              skills_json, loop_retries, loop_max_iter, note, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 3, ?, ?)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(id.uuid().to_string())
        .bind(old_version as i64)
        .bind(cur.get::<String, _>("name"))
        .bind(cur.get::<String, _>("prompt"))
        .bind(cur.get::<String, _>("goal"))
        .bind(cur.get::<String, _>("phases"))
        .bind(cur.get::<String, _>("phase_prompts"))
        .bind(cur.get::<String, _>("agents_json"))
        .bind(cur.get::<String, _>("skills_json"))
        .bind(&edit.note)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;

        // Now overwrite with the new version.
        sqlx::query(
            "UPDATE workflow_spec
             SET kind_json=?, prompt=?, goal=?, phases=?, phase_prompts=?, agents_json=?,
                 skills_json=?, updated_at=?, rev=rev+1
             WHERE id=?",
        )
        .bind(serde_json::to_string(&new_kind)?)
        .bind(&edit.prompt)
        .bind(&edit.goal)
        .bind(serde_json::to_string(&edit.phases)?)
        .bind(serde_json::to_string(&edit.phase_prompts)?)
        .bind(serde_json::to_string(&edit.agents)?)
        .bind(serde_json::to_string(&edit.skills)?)
        .bind(now_unix())
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_workflow_versions(
        &self,
        workflow_id: WorkflowId,
    ) -> Result<Vec<WorkflowVersion>> {
        let rows = sqlx::query(
            "SELECT id, workflow_id, version, name, prompt, goal, phases, phase_prompts,
                    agents_json, skills_json, loop_retries, loop_max_iter, note, created_at
             FROM workflow_version WHERE workflow_id=? ORDER BY version DESC",
        )
        .bind(workflow_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|r| {
                let wid = parse_uuid(&r.get::<String, _>("workflow_id"), WorkflowId::from_uuid)
                    .unwrap_or(WorkflowId::nil());
                let id = parse_uuid(&r.get::<String, _>("id"), WorkflowRunId::from_uuid)
                    .unwrap_or(WorkflowRunId::nil());
                Ok(WorkflowVersion {
                    id,
                    workflow_id: wid,
                    version: r.get::<i64, _>("version") as u32,
                    name: r.get("name"),
                    prompt: r.get("prompt"),
                    goal: r.get("goal"),
                    phases: serde_json::from_str(&r.get::<String, _>("phases"))?,
                    phase_prompts: serde_json::from_str(&r.get::<String, _>("phase_prompts"))
                        .unwrap_or_default(),
                    agents: serde_json::from_str(&r.get::<String, _>("agents_json"))?,
                    skills: serde_json::from_str(&r.get::<String, _>("skills_json"))?,
                    loop_retries: r.get::<i64, _>("loop_retries") as u8,
                    loop_max_iter: r.get::<i64, _>("loop_max_iter") as u8,
                    note: r.get("note"),
                    created_at: r.get("created_at"),
                })
            })
            .collect()
    }

    async fn hub_usage_ranking(&self) -> Result<Vec<UsageRank>> {
        // LEFT JOIN so a spec that's never run still appears (cold=true at the
        // bottom). Rank by real run count desc — the Static `uses` counter is
        // deliberately not used here so the ranking reflects the append-only
        // log, not a counter that could drift from it.
        let rows = sqlx::query(
            "SELECT ws.id AS wid, ws.name AS name, ws.stage_ref AS stage_ref,
                    COUNT(wr.id) AS total,
                    SUM(CASE WHEN wr.status='ok' THEN 1 ELSE 0 END) AS ok_n,
                    SUM(CASE WHEN wr.status='failed' THEN 1 ELSE 0 END) AS fail_n,
                    MAX(wr.started_at) AS last_at
             FROM workflow_spec ws
             LEFT JOIN workflow_run wr ON wr.workflow_id = ws.id
             GROUP BY ws.id
             ORDER BY total DESC, ws.name ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| {
                let total: i64 = r.get("total");
                let ok_runs: i64 = r.get("ok_n");
                let failed_runs: i64 = r.get("fail_n");
                let settled = ok_runs + failed_runs;
                UsageRank {
                    workflow_id: parse_uuid(&r.get::<String, _>("wid"), WorkflowId::from_uuid)
                        .unwrap_or(WorkflowId::nil()),
                    workflow_name: r.get("name"),
                    stage_ref: r.get::<Option<i64>, _>("stage_ref").map(|n| n as u8),
                    total_runs: total as u32,
                    ok_runs: ok_runs as u32,
                    failed_runs: failed_runs as u32,
                    success_rate: if settled > 0 {
                        Some(ok_runs as f32 / settled as f32)
                    } else {
                        None
                    },
                    last_run_at: r.get("last_at"),
                    cold: total == 0,
                }
            })
            .collect())
    }

    async fn create_skill(&self, s: NewSkill) -> Result<()> {
        let t = now_unix();
        let (source_tag, official_library) = hub_source_columns(&s.source);
        // 建行与阶段归属包在同一事务里 —— 让调用方没有「skill 行落地、
        // skill_stage 半途失败」的机会(seed/import 路径都靠这一点)。
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO skill (id, name, maturity, descr, category, stage_origin, source, official_library, uses, content, project_id, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 0)",
        )
        .bind(s.id.uuid().to_string())
        .bind(&s.name)
        .bind(maturity_text(s.maturity))
        .bind(&s.desc)
        .bind(&s.category)
        .bind(stage_origin_tag(s.stage_origin))
        .bind(source_tag)
        .bind(official_library)
        .bind(&s.content)
        .bind(s.project_id.map(pid))
        .bind(t)
        .bind(t)
        .execute(&mut *tx)
        .await?;
        for k in &s.stages {
            sqlx::query("INSERT OR IGNORE INTO skill_stage (skill_id, stage) VALUES (?, ?)")
                .bind(s.id.uuid().to_string())
                .bind(i64::from(k.index()))
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn update_skill(&self, id: SkillId, edit: SkillEdit) -> Result<()> {
        // T11 (plan/12 §7): `flip_to_self_built` flips `source` in the same
        // UPDATE — `official_library` is deliberately absent from either SET
        // list, so it survives untouched either way (留痕; see
        // `SkillEdit::flip_to_self_built`'s doc comment).
        if edit.flip_to_self_built {
            sqlx::query(
                "UPDATE skill SET name=?, descr=?, category=?, content=?, source='self_built', updated_at=?, rev=rev+1 WHERE id=?",
            )
            .bind(&edit.name)
            .bind(&edit.desc)
            .bind(&edit.category)
            .bind(&edit.content)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE skill SET name=?, descr=?, category=?, content=?, updated_at=?, rev=rev+1 WHERE id=?",
            )
            .bind(&edit.name)
            .bind(&edit.desc)
            .bind(&edit.category)
            .bind(&edit.content)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn set_skill_source(&self, id: SkillId, source: HubSource) -> Result<()> {
        let (source_tag, official_library) = hub_source_columns(&source);
        sqlx::query(
            "UPDATE skill SET source=?, official_library=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(source_tag)
        .bind(official_library)
        .bind(now_unix())
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_skill(&self, id: SkillId) -> Result<()> {
        let sid = id.uuid().to_string();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM skill_file WHERE skill_id=?")
            .bind(&sid)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM skill WHERE id=?")
            .bind(&sid)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn list_skills(&self) -> Result<Vec<SkillCard>> {
        let rows = sqlx::query(
            "SELECT id, name, maturity, descr, category, stage_origin, source, official_library, uses, content,
                    distilled_from_issue, origin_agent, project_id, rev
             FROM skill ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut cards: Vec<SkillCard> = rows
            .into_iter()
            .map(skill_row)
            .collect::<Result<Vec<_>>>()?;
        // 一次查关联表、按 id 分发 —— 不是每张卡一次查询。
        let by_id = self.list_skill_stages().await?;
        for c in cards.iter_mut() {
            if let Some(stages) = by_id.get(&c.id) {
                c.stages = stages.clone();
            }
        }
        Ok(cards)
    }

    async fn get_skill(&self, id: SkillId) -> Result<Option<SkillCard>> {
        let row = sqlx::query(
            "SELECT id, name, maturity, descr, category, stage_origin, source, official_library, uses, content,
                    distilled_from_issue, origin_agent, project_id, rev
             FROM skill WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        let Some(mut card) = row.map(skill_row).transpose()? else {
            return Ok(None);
        };
        let stages = sqlx::query("SELECT stage FROM skill_stage WHERE skill_id=? ORDER BY stage")
            .bind(id.uuid().to_string())
            .fetch_all(&self.pool)
            .await?;
        card.stages = stages
            .into_iter()
            .filter_map(|r| StageKind::from_index(r.get::<i64, _>("stage") as u8))
            .collect();
        Ok(Some(card))
    }

    async fn record_skill_use(&self, id: SkillId) -> Result<()> {
        sqlx::query("UPDATE skill SET uses=uses+1, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Distill a new skill from a completed, assigned Issue — the "every
    /// solution compounds into a reusable skill" link. Errors unless the issue
    /// exists, is `Done`, and has a real assignee (a distilled skill must
    /// attribute a real agent). The new skill is `SelfBuilt` / `Polishing` /
    /// `uses = 0`, carrying `distilled_from_issue` + `origin_agent`.
    async fn distill_skill_from_issue(&self, skill: NewSkill, from_issue: IssueId) -> Result<()> {
        let issue = self
            .get_issue(from_issue)
            .await?
            .ok_or_else(|| StoreError::Other("distill: issue not found".into()))?;
        if issue.status != IssueStatus::Done {
            return Err(StoreError::Other("distill: issue is not Done".into()));
        }
        let origin_agent = issue
            .assignee
            .ok_or_else(|| StoreError::Other("distill: issue has no assignee".into()))?;

        let t = now_unix();
        let (source_tag, official_library) = hub_source_columns(&HubSource::SelfBuilt);
        // 建行与阶段归属包在同一事务里(同 create_skill)—— 前置的只读校验
        // (issue 存在/Done/有 assignee)已经在事务外做完,这里只包写入。
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO skill
                (id, name, maturity, descr, category, stage_origin, source, official_library, uses, content,
                 distilled_from_issue, origin_agent, project_id,
                 created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(skill.id.uuid().to_string())
        .bind(&skill.name)
        .bind(maturity_text(Maturity::Polishing))
        .bind(&skill.desc)
        .bind(&skill.category)
        // T7 (plan/12 §0)/2026-08-05: same provenance-not-input rule the
        // line below already applies to `project_id` — a distilled skill
        // really did arise from work in the issue's real stage, not a
        // caller guess (`skill.stages`/`skill.stage_origin` are ignored the
        // same way `project_id` is, see the call site's own comment).
        .bind(stage_origin_tag(StageOrigin::Distilled))
        .bind(source_tag)
        .bind(official_library)
        .bind(&skill.content)
        .bind(from_issue.uuid().to_string())
        .bind(origin_agent.uuid().to_string())
        // 蒸馏出的技能归属本项目(plan/08 S1 完成标准):项目归属来自源
        // Issue 的真实 project_id,不是调用方随手传的值——provenance,不是输入。
        .bind(pid(issue.project_id))
        .bind(t)
        .bind(t)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT OR IGNORE INTO skill_stage (skill_id, stage) VALUES (?, ?)")
            .bind(skill.id.uuid().to_string())
            .bind(i64::from(issue.stage.index()))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn import_skill_package(&self, skill: NewSkill, files: Vec<NewSkillFile>) -> Result<()> {
        let t = now_unix();
        let (source_tag, official_library) = hub_source_columns(&skill.source);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO skill (id, name, maturity, descr, category, stage_origin, source, official_library, uses, content, project_id, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 0)",
        )
        .bind(skill.id.uuid().to_string())
        .bind(&skill.name)
        .bind(maturity_text(skill.maturity))
        .bind(&skill.desc)
        .bind(&skill.category)
        .bind(stage_origin_tag(skill.stage_origin))
        .bind(source_tag)
        .bind(official_library)
        .bind(&skill.content)
        .bind(skill.project_id.map(pid))
        .bind(t)
        .bind(t)
        .execute(&mut *tx)
        .await?;
        // 同 create_skill:建行与阶段归属在同一次调用里落地。
        for k in &skill.stages {
            sqlx::query("INSERT OR IGNORE INTO skill_stage (skill_id, stage) VALUES (?, ?)")
                .bind(skill.id.uuid().to_string())
                .bind(i64::from(k.index()))
                .execute(&mut *tx)
                .await?;
        }

        for f in files {
            sqlx::query(
                "INSERT INTO skill_file (id, skill_id, rel_path, content, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(SkillFileId::new().uuid().to_string())
            .bind(skill.id.uuid().to_string())
            .bind(&f.rel_path)
            .bind(&f.content)
            .bind(t)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    async fn list_skill_files(&self, skill_id: SkillId) -> Result<Vec<SkillFileRow>> {
        let rows = sqlx::query(
            "SELECT id, skill_id, rel_path, content, created_at
             FROM skill_file WHERE skill_id=? ORDER BY created_at",
        )
        .bind(skill_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(skill_file_row).collect()
    }

    async fn list_skill_stages(&self) -> Result<HashMap<SkillId, Vec<StageKind>>> {
        let rows = sqlx::query("SELECT skill_id, stage FROM skill_stage ORDER BY skill_id, stage")
            .fetch_all(&self.pool)
            .await?;
        let mut out: HashMap<SkillId, Vec<StageKind>> = HashMap::new();
        for r in rows {
            let id = parse_uuid(&r.get::<String, _>("skill_id"), SkillId::from_uuid)?;
            // 越界值(理论上进不来 —— 写侧只写 StageKind::index)如实丢弃,
            // 绝不映射成某个「差不多的」阶段。
            if let Some(k) = StageKind::from_index(r.get::<i64, _>("stage") as u8) {
                out.entry(id).or_default().push(k);
            }
        }
        Ok(out)
    }

    async fn set_skill_stages(
        &self,
        id: SkillId,
        stages: &[StageKind],
        origin: StageOrigin,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM skill_stage WHERE skill_id=?")
            .bind(id.uuid().to_string())
            .execute(&mut *tx)
            .await?;
        for k in stages {
            sqlx::query("INSERT INTO skill_stage (skill_id, stage) VALUES (?, ?)")
                .bind(id.uuid().to_string())
                .bind(i64::from(k.index()))
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("UPDATE skill SET stage_origin=?, updated_at=? WHERE id=?")
            .bind(stage_origin_tag(origin))
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn create_agent(&self, a: NewAgent) -> Result<()> {
        let t = now_unix();
        let (source_tag, official_library) = hub_source_columns(&a.source);
        sqlx::query(
            "INSERT INTO agent (id, name, role, stage_ref, maturity, skills, model, runs, win_rate, instructions, wins, tools, agent_cli, source, official_library, project_id, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, '', ?, 0, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(a.id.uuid().to_string())
        .bind(&a.name)
        .bind(&a.role)
        .bind(a.stage_ref.map(|k| i64::from(k.index())))
        .bind(maturity_text(a.maturity))
        .bind(serde_json::to_string(&a.skills)?)
        .bind(&a.model)
        .bind(&a.instructions)
        .bind(serde_json::to_string(&a.tools)?)
        .bind(&a.agent_cli)
        .bind(source_tag)
        .bind(official_library)
        .bind(a.project_id.map(pid))
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_agent(&self, id: AgentId, edit: AgentEdit) -> Result<()> {
        // T11 (plan/12 §7): same flip-in-place scheme as `update_skill` —
        // `official_library` stays untouched in both branches (留痕).
        if edit.flip_to_self_built {
            sqlx::query(
                "UPDATE agent SET name=?, role=?, skills=?, model=?, instructions=?, source='self_built', updated_at=?, rev=rev+1 WHERE id=?",
            )
            .bind(&edit.name)
            .bind(&edit.role)
            .bind(serde_json::to_string(&edit.skills)?)
            .bind(&edit.model)
            .bind(&edit.instructions)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                "UPDATE agent SET name=?, role=?, skills=?, model=?, instructions=?, updated_at=?, rev=rev+1 WHERE id=?",
            )
            .bind(&edit.name)
            .bind(&edit.role)
            .bind(serde_json::to_string(&edit.skills)?)
            .bind(&edit.model)
            .bind(&edit.instructions)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    async fn set_agent_stage_ref(&self, id: AgentId, stage_ref: Option<StageKind>) -> Result<()> {
        sqlx::query("UPDATE agent SET stage_ref=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(stage_ref.map(|k| i64::from(k.index())))
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<AgentCard>> {
        let rows = sqlx::query(
            "SELECT id, name, role, stage_ref, maturity, skills, model, runs, win_rate, instructions, tools, agent_cli, source, official_library, project_id FROM agent ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(agent_row).collect()
    }

    async fn get_agent(&self, id: AgentId) -> Result<Option<AgentCard>> {
        let row = sqlx::query(
            "SELECT id, name, role, stage_ref, maturity, skills, model, runs, win_rate, instructions, tools, agent_cli, source, official_library, project_id FROM agent WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(agent_row).transpose()
    }

    async fn record_agent_run(&self, id: AgentId, ok: bool) -> Result<()> {
        // runs/wins are the real counters; win_rate is a derived display
        // string recomputed from them in the same statement — never patched
        // independently, so it can't drift from the counters it summarizes.
        sqlx::query(
            "UPDATE agent SET runs=runs+1, wins=wins+?, \
             win_rate = printf('%d%%', (wins+?)*100/(runs+1)), \
             updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(if ok { 1 } else { 0 })
        .bind(if ok { 1 } else { 0 })
        .bind(now_unix())
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_agent(&self, id: AgentId) -> Result<()> {
        sqlx::query("DELETE FROM agent WHERE id=?")
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_cron_task(&self, c: NewCronTask) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO cron_task (id, name, target, schedule, project_id, status, last_run, next_run, mode, issue_stage, issue_assignee, created_at, updated_at, rev, last_run_at)
             VALUES (?, ?, ?, ?, ?, 'normal', '', '', ?, ?, ?, ?, ?, 0, ?)",
        )
        .bind(c.id.uuid().to_string())
        .bind(&c.name)
        .bind(&c.target)
        .bind(cadence_text(&c.schedule))
        .bind(c.project_id.map(pid))
        .bind(cron_mode_text(&c.mode))
        .bind(c.issue_stage.map(stage_kind_text))
        .bind(&c.issue_assignee)
        .bind(t)
        .bind(t)
        .bind(c.last_run_at.unwrap_or(0))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_cron_tasks(&self) -> Result<Vec<CronTask>> {
        let rows = sqlx::query(
            "SELECT id, name, target, schedule, project_id, status, last_run, next_run, last_run_at, mode, issue_stage, issue_assignee
             FROM cron_task ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(cron_task_row).collect()
    }

    async fn set_cron_status(&self, id: CronTaskId, status: CronStatus) -> Result<()> {
        sqlx::query("UPDATE cron_task SET status=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(cron_status_text(status))
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn record_cron_run(
        &self,
        id: CronTaskId,
        status: CronStatus,
        last_run: String,
    ) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "UPDATE cron_task SET status=?, last_run=?, last_run_at=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(cron_status_text(status))
        .bind(&last_run)
        .bind(t)
        .bind(t)
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn create_connector(&self, c: NewConnector) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO connector (id, name, kind, status, last_sync, scope, project_id, config, created_at, updated_at, rev)
             VALUES (?, ?, ?, 'disconnected', '', ?, ?, ?, ?, ?, 0)",
        )
        .bind(c.id.uuid().to_string())
        .bind(&c.name)
        .bind(&c.kind)
        .bind(&c.scope)
        .bind(c.project_id.map(pid))
        .bind(&c.config)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_connectors(&self) -> Result<Vec<Connector>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, status, last_sync, scope, project_id, config FROM connector ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(connector_row).collect()
    }

    async fn set_connector_sync(
        &self,
        id: ConnectorId,
        status: ConnectorStatus,
        last_sync: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE connector SET status=?, last_sync=?, updated_at=?, rev=rev+1 WHERE id=?",
        )
        .bind(connector_status_text(status))
        .bind(last_sync)
        .bind(now_unix())
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete_connector(&self, id: ConnectorId) -> Result<()> {
        sqlx::query("DELETE FROM connector WHERE id=?")
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn create_knowledge_source(&self, k: NewKnowledgeSource) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO knowledge_source (id, name, kind, chunks, updated_label, used_by, created_at, updated_at, rev)
             VALUES (?, ?, ?, 0, '', ?, ?, ?, 0)",
        )
        .bind(k.id.uuid().to_string())
        .bind(&k.name)
        .bind(&k.kind)
        .bind(&k.used_by)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_knowledge_sources(&self) -> Result<Vec<KnowledgeSource>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, chunks, updated_label, used_by FROM knowledge_source ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(knowledge_source_row).collect()
    }

    async fn register_artifacts(&self, items: Vec<NewArtifact>) -> Result<u32> {
        let mut fresh = 0u32;
        for a in items {
            // INSERT OR IGNORE against UNIQUE(project_id, path, git_commit):
            // a re-scan of an unchanged workspace inserts nothing; only a
            // genuinely new version counts.
            let res = sqlx::query(
                "INSERT OR IGNORE INTO artifact \
                 (id, project_id, workflow_run_id, issue_id, stage_kind, path, kind, bytes, git_commit, registered_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(a.id.uuid().to_string())
            .bind(pid(a.project_id))
            .bind(a.workflow_run_id.map(|r| r.uuid().to_string()))
            .bind(a.issue_id.map(|i| i.uuid().to_string()))
            .bind(a.stage_kind.map(stage_kind_text))
            .bind(&a.path)
            .bind(a.kind.text())
            .bind(a.bytes as i64)
            .bind(&a.git_commit)
            .bind(a.registered_at)
            .execute(&self.pool)
            .await?;
            fresh += res.rows_affected() as u32;
        }
        Ok(fresh)
    }

    async fn list_artifacts(&self, project_id: ProjectId) -> Result<Vec<Artifact>> {
        let rows = sqlx::query(
            "SELECT id, project_id, workflow_run_id, issue_id, stage_kind, path, kind, bytes, git_commit, registered_at \
             FROM artifact WHERE project_id=? ORDER BY registered_at DESC, path",
        )
        .bind(pid(project_id))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(artifact_row).collect()
    }

    async fn list_artifacts_for_issue(&self, issue_id: IssueId) -> Result<Vec<Artifact>> {
        let rows = sqlx::query(
            "SELECT id, project_id, workflow_run_id, issue_id, stage_kind, path, kind, bytes, git_commit, registered_at \
             FROM artifact WHERE issue_id=? ORDER BY registered_at DESC, path",
        )
        .bind(issue_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(artifact_row).collect()
    }

    async fn list_runs_for_issue(&self, issue_id: IssueId) -> Result<Vec<WorkflowRun>> {
        let rows = sqlx::query(
            "SELECT id, workflow_id, workflow_name, project_id, session_id, trigger, status,
                    started_at, finished_at, duration_ms, phases_completed, error, params_json, cron_task_id, issue_id, head_before, head_after
             FROM workflow_run WHERE issue_id=? ORDER BY started_at DESC, rowid DESC",
        )
        .bind(issue_id.uuid().to_string())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(parse_run_row).collect())
    }

    async fn create_issue(&self, i: NewIssue) -> Result<()> {
        let t = now_unix();
        // Per-project sequence: 1, 2, 3, … (COALESCE so the first issue gets 1).
        let number: i64 = sqlx::query(
            "SELECT COALESCE(MAX(number), 0) + 1 AS next FROM issue WHERE project_id=?",
        )
        .bind(pid(i.project_id))
        .fetch_one(&self.pool)
        .await?
        .get("next");
        sqlx::query(
            "INSERT INTO issue
                (id, project_id, stage, number, github_number, pr_number, title, descr, status,
                 priority, assignee, standard_skill, created_at, updated_at)
             VALUES (?, ?, ?, ?, 0, 0, ?, ?, 'backlog', ?, NULL, ?, ?, ?)",
        )
        .bind(i.id.uuid().to_string())
        .bind(pid(i.project_id))
        .bind(stage_kind_text(i.stage))
        .bind(number)
        .bind(&i.title)
        .bind(&i.desc)
        .bind(issue_priority_text(i.priority))
        .bind(&i.standard_skill)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_issues(
        &self,
        project_id: ProjectId,
        stage: Option<StageKind>,
        status: Option<IssueStatus>,
    ) -> Result<Vec<Issue>> {
        // Build the query dynamically: `None` filter = no constraint. Two
        // optional filters × the base WHERE keeps this readable without an
        // query-builder dependency.
        let mut sql = String::from(
            "SELECT id, project_id, stage, number, github_number, pr_number, title, descr, status,
                    priority, assignee, settled_at, blocked_reason, standard_skill,
                    created_at, updated_at
             FROM issue WHERE project_id=?",
        );
        if stage.is_some() {
            sql.push_str(" AND stage=?");
        }
        if status.is_some() {
            sql.push_str(" AND status=?");
        }
        sql.push_str(" ORDER BY number ASC");
        let mut q = sqlx::query(&sql).bind(pid(project_id));
        if let Some(k) = stage {
            q = q.bind(stage_kind_text(k));
        }
        if let Some(s) = status {
            q = q.bind(issue_status_text(s));
        }
        let rows = q.fetch_all(&self.pool).await?;
        rows.into_iter().map(issue_row).collect()
    }

    async fn get_issue(&self, id: IssueId) -> Result<Option<Issue>> {
        let row = sqlx::query(
            "SELECT id, project_id, stage, number, github_number, pr_number, title, descr, status,
                    priority, assignee, settled_at, blocked_reason, standard_skill,
                    created_at, updated_at
             FROM issue WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(issue_row).transpose()
    }

    async fn transition_issue(&self, id: IssueId, status: IssueStatus) -> Result<()> {
        // Nothing but `block_issue` can put an issue INTO Blocked (the App
        // layer rejects a bare TransitionIssue targeting Blocked), so every
        // move through this path unconditionally clears any stale reason —
        // a plain transition out of Blocked, or any other edge, leaves no
        // dangling `blocked_reason` behind.
        sqlx::query("UPDATE issue SET status=?, blocked_reason=NULL, updated_at=? WHERE id=?")
            .bind(issue_status_text(status))
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn block_issue(&self, id: IssueId, reason: &str) -> Result<()> {
        sqlx::query("UPDATE issue SET status=?, blocked_reason=?, updated_at=? WHERE id=?")
            .bind(issue_status_text(IssueStatus::Blocked))
            .bind(reason)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn count_open_issues(&self, project_id: ProjectId) -> Result<i64> {
        let row = sqlx::query(
            "SELECT COUNT(*) AS n FROM issue WHERE project_id=? AND status NOT IN (?, ?)",
        )
        .bind(pid(project_id))
        .bind(issue_status_text(IssueStatus::Done))
        .bind(issue_status_text(IssueStatus::Cancelled))
        .fetch_one(&self.pool)
        .await?;
        Ok(row.get::<i64, _>("n"))
    }

    async fn assign_issue(&self, id: IssueId, assignee: Option<AgentId>) -> Result<()> {
        sqlx::query("UPDATE issue SET assignee=?, updated_at=? WHERE id=?")
            .bind(assignee.map(|a| a.uuid().to_string()))
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn mark_issue_settled(&self, id: IssueId, at: i64) -> Result<()> {
        // COALESCE keeps the FIRST settle timestamp even if called twice —
        // the settle-once invariant is enforced in the DB, not just the app.
        sqlx::query("UPDATE issue SET settled_at=COALESCE(settled_at, ?) WHERE id=?")
            .bind(at)
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_issue_pr_number(&self, id: IssueId, pr_number: u32) -> Result<()> {
        sqlx::query("UPDATE issue SET pr_number=?, updated_at=? WHERE id=?")
            .bind(pr_number as i64)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// V1 终端会话重构(阶段1): 读一件活绑定的会话行。None = 从未点开过。
    async fn get_conversation_by_issue(
        &self,
        issue_id: IssueId,
    ) -> Result<Option<ClaudeConversation>> {
        let row = sqlx::query(
            "SELECT id, project_id, issue_id, claude_session_id, workspace_path, branch_name,
                    created_at, last_opened_at
             FROM claude_conversation WHERE issue_id=?",
        )
        .bind(issue_id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(conversation_row).transpose()
    }

    /// V1 终端会话重构: 首次 spawn 前建会话行(INSERT OR IGNORE,issue_id
    /// UNIQUE 兜底,幂等)。claude_session_id 留空,等 hook 填。返回行的
    /// `ConversationId`(新建或已存在都查回——底座 TerminalManager 要身份)。
    async fn ensure_conversation(
        &self,
        issue_id: IssueId,
        project_id: ProjectId,
        workspace_path: &str,
        branch_name: &str,
    ) -> Result<ConversationId> {
        let id = Uuid::new_v4().to_string();
        let now = now_unix();
        sqlx::query(
            "INSERT OR IGNORE INTO claude_conversation
             (id, project_id, issue_id, claude_session_id, workspace_path, branch_name,
              created_at, last_opened_at)
             VALUES (?, ?, ?, '', ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(project_id.uuid().to_string())
        .bind(issue_id.uuid().to_string())
        .bind(workspace_path)
        .bind(branch_name)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        // INSERT OR IGNORE 在行已存在时不改 id——一律按 issue_id 读回。
        let row = sqlx::query("SELECT id FROM claude_conversation WHERE issue_id=?")
            .bind(issue_id.uuid().to_string())
            .fetch_one(&self.pool)
            .await?;
        let id_str: String = row.get::<String, _>("id");
        Ok(parse_uuid(&id_str, ConversationId::from_uuid)?)
    }

    /// V1 终端会话重构(阶段1): hook SessionStart 回传 session_id 时填进会话行
    /// (UPDATE;行不存在则 0 行受影响,no-op 不报错)。
    async fn set_conversation_session_id(&self, issue_id: IssueId, session_id: &str) -> Result<()> {
        sqlx::query("UPDATE claude_conversation SET claude_session_id=? WHERE issue_id=?")
            .bind(session_id)
            .bind(issue_id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// V1 终端会话重构(重启恢复): 只回填空的 workspace_path/branch_name,
    /// 已有值不覆盖;刷新 last_opened_at。
    async fn update_conversation_workspace_if_empty(
        &self,
        issue_id: IssueId,
        workspace_path: &str,
        branch_name: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE claude_conversation SET
                workspace_path = CASE WHEN workspace_path = '' THEN ? ELSE workspace_path END,
                branch_name = CASE WHEN branch_name = '' THEN ? ELSE branch_name END,
                last_opened_at = ?
             WHERE issue_id = ?",
        )
        .bind(workspace_path)
        .bind(branch_name)
        .bind(now_unix())
        .bind(issue_id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// V1 终端会话重构(阶段1): 列出某项目下有会话行的 issue_id(poll 用)。
    async fn list_conversation_issue_ids(&self, project_id: ProjectId) -> Result<Vec<IssueId>> {
        let rows = sqlx::query("SELECT issue_id FROM claude_conversation WHERE project_id=?")
            .bind(project_id.uuid().to_string())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|r| parse_uuid(&r.get::<String, _>("issue_id"), IssueId::from_uuid))
            .collect()
    }

    async fn list_resumable_conversation_issue_ids(
        &self,
        project_id: ProjectId,
    ) -> Result<Vec<IssueId>> {
        let rows = sqlx::query(
            "SELECT issue_id FROM claude_conversation WHERE project_id=? AND claude_session_id != ''",
        )
            .bind(project_id.uuid().to_string())
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|r| parse_uuid(&r.get::<String, _>("issue_id"), IssueId::from_uuid))
            .collect()
    }

    async fn set_issue_github_number(&self, id: IssueId, github_number: u32) -> Result<()> {
        sqlx::query("UPDATE issue SET github_number=?, updated_at=? WHERE id=?")
            .bind(github_number as i64)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn update_issue_content(&self, id: IssueId, title: &str, desc: &str) -> Result<()> {
        sqlx::query("UPDATE issue SET title=?, descr=?, updated_at=? WHERE id=?")
            .bind(title)
            .bind(desc)
            .bind(now_unix())
            .bind(id.uuid().to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_issue_standard_skill_if_empty(&self, id: IssueId, skill: &str) -> Result<()> {
        let skill = skill.trim();
        if skill.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "UPDATE issue SET standard_skill=?, updated_at=? \
             WHERE id=? AND (standard_skill IS NULL OR standard_skill='')",
        )
        .bind(skill)
        .bind(now_unix())
        .bind(id.uuid().to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get_app_meta(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM app_meta WHERE key=?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    async fn set_app_meta(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_meta (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(now_unix())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn parse_run_row(r: &sqlx::sqlite::SqliteRow) -> WorkflowRun {
    let id = parse_uuid(&r.get::<String, _>("id"), WorkflowRunId::from_uuid)
        .unwrap_or(WorkflowRunId::nil());
    let workflow_id = parse_uuid(&r.get::<String, _>("workflow_id"), WorkflowId::from_uuid)
        .unwrap_or(WorkflowId::nil());
    let project_id = r
        .get::<Option<String>, _>("project_id")
        .filter(|s| !s.is_empty())
        .and_then(|s| parse_uuid(&s, ProjectId::from_uuid).ok());
    let session_id = r
        .get::<Option<String>, _>("session_id")
        .filter(|s| !s.is_empty())
        .and_then(|s| parse_uuid(&s, SessionId::from_uuid).ok());
    let finished_at: Option<i64> = r.get("finished_at");
    let duration_ms: Option<i64> = r.get("duration_ms");
    WorkflowRun {
        id,
        workflow_id,
        workflow_name: r.get("workflow_name"),
        project_id,
        session_id,
        trigger: RunTrigger::parse(&r.get::<String, _>("trigger")),
        status: RunStatus::parse(&r.get::<String, _>("status")),
        started_at: r.get("started_at"),
        finished_at,
        duration_ms,
        phases_completed: r.get::<i64, _>("phases_completed") as u32,
        error: r.get("error"),
        params_json: r.get("params_json"),
        cron_task_id: r
            .get::<Option<String>, _>("cron_task_id")
            .filter(|s| !s.is_empty())
            .and_then(|s| parse_uuid(&s, CronTaskId::from_uuid).ok()),
        issue_id: r
            .get::<Option<String>, _>("issue_id")
            .filter(|s| !s.is_empty())
            .and_then(|s| parse_uuid(&s, IssueId::from_uuid).ok()),
        head_before: r
            .get::<Option<String>, _>("head_before")
            .filter(|s| !s.is_empty()),
        head_after: r
            .get::<Option<String>, _>("head_after")
            .filter(|s| !s.is_empty()),
    }
}

fn project_row(r: sqlx::sqlite::SqliteRow) -> Result<ProjectRow> {
    let id = parse_uuid(&r.get::<String, _>("id"), ProjectId::from_uuid)?;
    let active_stage =
        parse_stage_kind(&r.get::<String, _>("active_stage")).unwrap_or(StageKind::Prototype);
    Ok(ProjectRow {
        id,
        name: r.get("name"),
        kind: r.get("kind"),
        desc: r.get("descr"),
        phase: parse_phase(&r.get::<String, _>("phase")),
        cycle: parse_cycle(&r.get::<String, _>("cycle")),
        active_stage,
        north_star: r.get("north_star"),
        ns_def: r.get("ns_def"),
        benchmark: r.get("benchmark"),
        opportunity: r.get("opportunity"),
        workspace_path: r.get("workspace_path"),
        allow_commands: r.get::<i64, _>("allow_commands") != 0,
        remote_path: r.get("remote_path"),
        remote_host: r.get("remote_host"),
        north_star_collect_kind: r.get("north_star_collect_kind"),
        north_star_collect_query: r.get("north_star_collect_query"),
        provider: r.get("provider"),
        signal: r
            .get::<Option<String>, _>("signal")
            .and_then(|s| parse_sig(&s)),
        weekly_signal: r
            .get::<Option<String>, _>("weekly_signal")
            .and_then(|s| parse_sig(&s)),
        created_at: r.get::<i64, _>("created_at"),
    })
}

/// Nullable `project_id TEXT` column → `Option<ProjectId>`. Same shape as
/// `cron_task_row`/`connector_row`'s existing parsing — `NULL`/empty = global.
fn opt_project_id(r: &sqlx::sqlite::SqliteRow) -> Result<Option<ProjectId>> {
    r.get::<Option<String>, _>("project_id")
        .filter(|s| !s.is_empty())
        .map(|s| parse_uuid(&s, ProjectId::from_uuid))
        .transpose()
}

fn workflow_spec_row(r: sqlx::sqlite::SqliteRow) -> Result<WorkflowSpec> {
    let id = parse_uuid(&r.get::<String, _>("id"), WorkflowId::from_uuid)?;
    let kind: WorkflowKind = serde_json::from_str(&r.get::<String, _>("kind_json"))?;
    // T8: `PhaseMeta`'s `Deserialize` impl accepts both the pre-T8 plain
    // string array and the new structured shape, per element — an old row
    // reads in as `role: Neutral`, never a hard crash.
    let phases: Vec<PhaseMeta> = serde_json::from_str(&r.get::<String, _>("phases"))?;
    let phase_prompts: Vec<String> =
        serde_json::from_str(&r.get::<String, _>("phase_prompts")).unwrap_or_default();
    let agents: Vec<AgentRef> = serde_json::from_str(&r.get::<String, _>("agents_json"))?;
    let skills: Vec<SkillRef> = serde_json::from_str(&r.get::<String, _>("skills_json"))?;
    let project_id = opt_project_id(&r)?;
    Ok(WorkflowSpec {
        id,
        name: r.get("name"),
        kind,
        prompt: r.get("prompt"),
        goal: r.get("goal"),
        stage_ref: r.get::<Option<i64>, _>("stage_ref").map(|v| v as u8),
        phases,
        phase_prompts,
        agents,
        skills,
        loop_config: LoopConfig {
            retries: r.get::<i64, _>("loop_retries") as u8,
            max_iter: r.get::<i64, _>("loop_max_iter") as u8,
        },
        project_id,
        content: r.get("content"),
    })
}

/// `skill.stage_origin` 列 ↔ 域枚举。未知/空值一律读成 `Unclassified` ——
/// 诚实降级,绝不猜一个归类出处出来。
fn parse_stage_origin(tag: &str) -> StageOrigin {
    match tag {
        "table" => StageOrigin::Table,
        "distilled" => StageOrigin::Distilled,
        "manual" => StageOrigin::Manual,
        "legacy" => StageOrigin::Legacy,
        _ => StageOrigin::Unclassified,
    }
}

fn stage_origin_tag(origin: StageOrigin) -> &'static str {
    match origin {
        StageOrigin::Unclassified => "",
        StageOrigin::Table => "table",
        StageOrigin::Distilled => "distilled",
        StageOrigin::Manual => "manual",
        StageOrigin::Legacy => "legacy",
    }
}

fn skill_row(r: sqlx::sqlite::SqliteRow) -> Result<SkillCard> {
    let id = parse_uuid(&r.get::<String, _>("id"), SkillId::from_uuid)?;
    let distilled_from_issue = r
        .get::<Option<String>, _>("distilled_from_issue")
        .filter(|s| !s.is_empty())
        .map(|s| parse_uuid(&s, IssueId::from_uuid))
        .transpose()?;
    let origin_agent = r
        .get::<Option<String>, _>("origin_agent")
        .filter(|s| !s.is_empty())
        .map(|s| parse_uuid(&s, AgentId::from_uuid))
        .transpose()?;
    let project_id = opt_project_id(&r)?;
    // 阶段归属不在 skill 行上——它在 skill_stage 关联表里(多值)。行读只带
    // 归类出处;`stages` 由调用方(list_skills / get_skill)按 skill_id 补齐,
    // 避免每行一次查询的 N+1。
    let stage_origin = parse_stage_origin(&r.get::<String, _>("stage_origin"));
    let source_tag: String = r.get("source");
    let official_library: String = r.get("official_library");
    Ok(SkillCard {
        id,
        name: r.get("name"),
        maturity: parse_maturity(&r.get::<String, _>("maturity")),
        desc: r.get("descr"),
        category: r.get("category"),
        stages: Vec::new(),
        stage_origin,
        source: parse_hub_source(&source_tag, &official_library),
        adapted_from: parse_adapted_from(&source_tag, &official_library),
        uses: r.get::<i64, _>("uses") as u32,
        content: r.get("content"),
        distilled_from_issue,
        origin_agent,
        project_id,
        rev: r.get::<i64, _>("rev") as u32,
    })
}

fn skill_file_row(r: sqlx::sqlite::SqliteRow) -> Result<SkillFileRow> {
    Ok(SkillFileRow {
        id: parse_uuid(&r.get::<String, _>("id"), SkillFileId::from_uuid)?,
        skill_id: parse_uuid(&r.get::<String, _>("skill_id"), SkillId::from_uuid)?,
        rel_path: r.get("rel_path"),
        content: r.get("content"),
        created_at: r.get::<i64, _>("created_at"),
    })
}

fn agent_row(r: sqlx::sqlite::SqliteRow) -> Result<AgentCard> {
    let id = parse_uuid(&r.get::<String, _>("id"), AgentId::from_uuid)?;
    let skills: Vec<String> = serde_json::from_str(&r.get::<String, _>("skills"))?;
    let tools: Vec<String> = serde_json::from_str(&r.get::<String, _>("tools"))?;
    let project_id = opt_project_id(&r)?;
    let stage_ref = r
        .get::<Option<i64>, _>("stage_ref")
        .and_then(|n| StageKind::from_index(n as u8));
    let source_tag: String = r.get("source");
    let official_library: String = r.get("official_library");
    Ok(AgentCard {
        id,
        name: r.get("name"),
        role: r.get("role"),
        stage_ref,
        maturity: parse_maturity(&r.get::<String, _>("maturity")),
        skills: skills
            .into_iter()
            .map(|name| AgentSkillTag { name })
            .collect(),
        model: r.get("model"),
        runs: r.get::<i64, _>("runs") as u32,
        win_rate: r.get("win_rate"),
        instructions: r.get("instructions"),
        tools,
        agent_cli: r.get("agent_cli"),
        source: parse_hub_source(&source_tag, &official_library),
        adapted_from: parse_adapted_from(&source_tag, &official_library),
        project_id,
    })
}

fn cron_task_row(r: sqlx::sqlite::SqliteRow) -> Result<CronTask> {
    let id = parse_uuid(&r.get::<String, _>("id"), CronTaskId::from_uuid)?;
    let project_id = r
        .get::<Option<String>, _>("project_id")
        .map(|s| parse_uuid(&s, ProjectId::from_uuid))
        .transpose()?;
    let last_run_at_raw: i64 = r.get("last_run_at");
    let target: String = r.get("target");
    let mode_text: String = r.get("mode");
    let mode = parse_cron_mode(&mode_text, &target);
    Ok(CronTask {
        id,
        name: r.get("name"),
        target,
        schedule: parse_cadence(&r.get::<String, _>("schedule")),
        project_id,
        status: parse_cron_status(&r.get::<String, _>("status")),
        last_run: r.get("last_run"),
        next_run: r.get("next_run"),
        last_run_at: (last_run_at_raw > 0)
            .then(|| OffsetDateTime::from_unix_timestamp(last_run_at_raw).ok())
            .flatten(),
        mode,
        issue_stage: r
            .get::<Option<String>, _>("issue_stage")
            .as_deref()
            .and_then(parse_stage_kind),
        issue_assignee: r.get::<Option<String>, _>("issue_assignee"),
    })
}

fn connector_row(r: sqlx::sqlite::SqliteRow) -> Result<Connector> {
    let id = parse_uuid(&r.get::<String, _>("id"), ConnectorId::from_uuid)?;
    let project_id = r
        .get::<Option<String>, _>("project_id")
        .filter(|s| !s.is_empty())
        .and_then(|s| parse_uuid(&s, ProjectId::from_uuid).ok());
    Ok(Connector {
        id,
        name: r.get("name"),
        kind: r.get("kind"),
        status: parse_connector_status(&r.get::<String, _>("status")),
        last_sync: r.get("last_sync"),
        scope: r.get("scope"),
        project_id,
        config: r.get("config"),
    })
}

fn artifact_row(r: sqlx::sqlite::SqliteRow) -> Result<Artifact> {
    let id = parse_uuid(&r.get::<String, _>("id"), ArtifactId::from_uuid)?;
    let project_id = parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid)?;
    let workflow_run_id = r
        .get::<Option<String>, _>("workflow_run_id")
        .filter(|s| !s.is_empty())
        .and_then(|s| parse_uuid(&s, WorkflowRunId::from_uuid).ok());
    let issue_id = r
        .get::<Option<String>, _>("issue_id")
        .filter(|s| !s.is_empty())
        .and_then(|s| parse_uuid(&s, IssueId::from_uuid).ok());
    let stage_kind = r
        .get::<Option<String>, _>("stage_kind")
        .as_deref()
        .and_then(parse_stage_kind);
    Ok(Artifact {
        id,
        project_id,
        workflow_run_id,
        issue_id,
        stage_kind,
        path: r.get("path"),
        kind: ArtifactKind::parse(&r.get::<String, _>("kind")),
        bytes: r.get::<i64, _>("bytes") as u64,
        git_commit: r.get("git_commit"),
        registered_at: r.get("registered_at"),
    })
}

fn knowledge_source_row(r: sqlx::sqlite::SqliteRow) -> Result<KnowledgeSource> {
    let id = parse_uuid(&r.get::<String, _>("id"), KnowledgeSourceId::from_uuid)?;
    Ok(KnowledgeSource {
        id,
        name: r.get("name"),
        kind: r.get("kind"),
        chunks: r.get::<i64, _>("chunks") as u32,
        updated_label: r.get("updated_label"),
        used_by: r.get("used_by"),
    })
}

fn issue_row(r: sqlx::sqlite::SqliteRow) -> Result<Issue> {
    let id = parse_uuid(&r.get::<String, _>("id"), IssueId::from_uuid)?;
    let project_id = parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid)?;
    let stage = parse_stage_kind(&r.get::<String, _>("stage"))
        .ok_or_else(|| StoreError::Other("bad issue stage".into()))?;
    let assignee = r
        .get::<Option<String>, _>("assignee")
        .filter(|s| !s.is_empty())
        .map(|s| parse_uuid(&s, AgentId::from_uuid))
        .transpose()?;
    Ok(Issue {
        id,
        project_id,
        stage,
        number: r.get::<i64, _>("number") as u32,
        github_number: r.get::<i64, _>("github_number") as u32,
        pr_number: r.get::<i64, _>("pr_number") as u32,
        title: r.get("title"),
        desc: r.get("descr"),
        status: parse_issue_status(&r.get::<String, _>("status")),
        priority: parse_issue_priority(&r.get::<String, _>("priority")),
        assignee,
        settled_at: r.get("settled_at"),
        blocked_reason: r
            .get::<Option<String>, _>("blocked_reason")
            .filter(|s| !s.is_empty()),
        standard_skill: r.get("standard_skill"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}

/// V1 终端会话重构(阶段1): row → ClaudeConversation。
fn conversation_row(r: sqlx::sqlite::SqliteRow) -> Result<ClaudeConversation> {
    let id = parse_uuid(&r.get::<String, _>("id"), ConversationId::from_uuid)?;
    let project_id = parse_uuid(&r.get::<String, _>("project_id"), ProjectId::from_uuid)?;
    let issue_id = parse_uuid(&r.get::<String, _>("issue_id"), IssueId::from_uuid)?;
    Ok(ClaudeConversation {
        id,
        project_id,
        issue_id,
        claude_session_id: r.get("claude_session_id"),
        workspace_path: r.get("workspace_path"),
        branch_name: r.get("branch_name"),
        created_at: r.get("created_at"),
        last_opened_at: r.get("last_opened_at"),
    })
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectorDefSync, ConnectorsFileSync, NewProject};

    /// V1 Issue2 Phase 3: `sync_connectors_file` upserts by `(project_id,
    /// name)` — first sync inserts, second sync updates config in place
    /// (same id), kind is always `script`, config is the JSON string.
    #[tokio::test]
    async fn sync_connectors_file_upserts_by_name() {
        let db_path = tempdb_path();
        let store = SqliteStore::open(&db_path).await.expect("open store");
        let project_id = ProjectId::new();
        store
            .create_project(NewProject {
                id: project_id,
                name: "测试项目".into(),
                kind: "content".into(),
                desc: "测试".into(),
                provider: "github".into(),
            })
            .await
            .expect("create project");

        // First sync: insert two connectors.
        let sync = ConnectorsFileSync {
            project_id,
            connectors: vec![
                ConnectorDefSync {
                    name: "leading".into(),
                    config: r#"{"script":"scripts/derive_leading.py","command":"python","output":"data.json"}"#.into(),
                },
                ConnectorDefSync {
                    name: "north-star".into(),
                    config: r#"{"script":"scripts/derive_ns.py","command":"","output":"ns.json"}"#.into(),
                },
            ],
        };
        let summary = store.sync_connectors_file(sync).await.expect("first sync");
        assert_eq!(summary.connectors_synced, 2);

        let connectors = store.list_connectors().await.expect("list");
        let project_connectors: Vec<_> = connectors
            .into_iter()
            .filter(|c| c.project_id == Some(project_id))
            .collect();
        assert_eq!(project_connectors.len(), 2);
        let leading = project_connectors
            .iter()
            .find(|c| c.name == "leading")
            .expect("leading found");
        assert_eq!(leading.kind, "script");
        assert!(leading.config.contains("derive_leading.py"));
        let leading_id = leading.id;

        // Second sync: update leading's config (same name → upsert, same id),
        // drop north-star from the file (file deletion does NOT delete DB
        // rows per design — docs/buddy/standards/connectors.md "正本里删掉的连接器").
        let sync2 = ConnectorsFileSync {
            project_id,
            connectors: vec![ConnectorDefSync {
                name: "leading".into(),
                config: r#"{"script":"scripts/derive_leading_v2.py","command":"node","output":"data2.json"}"#.into(),
            }],
        };
        let summary2 = store
            .sync_connectors_file(sync2)
            .await
            .expect("second sync");
        assert_eq!(summary2.connectors_synced, 1);

        let connectors2 = store.list_connectors().await.expect("list after upsert");
        let project_connectors2: Vec<_> = connectors2
            .into_iter()
            .filter(|c| c.project_id == Some(project_id))
            .collect();
        // north-star still in DB (file deletion doesn't delete DB rows).
        assert_eq!(project_connectors2.len(), 2);
        let leading2 = project_connectors2
            .iter()
            .find(|c| c.name == "leading")
            .expect("leading still found");
        // Same id (upsert, not insert).
        assert_eq!(leading2.id, leading_id);
        // Kind is still "script" (UPDATE path doesn't touch kind, but verify
        // — regression guard for the Med review finding: SELECT must filter
        // kind='script' so a non-script connector of the same name never
        // gets its kind clobbered or its config overwritten).
        assert_eq!(leading2.kind, "script");
        // Config was updated.
        assert!(leading2.config.contains("derive_leading_v2.py"));
        assert!(leading2.config.contains("node"));
        assert!(!leading2.config.contains("derive_leading.py") || leading2.config.contains("v2"));
    }

    /// V1 Issue2 Phase 3 review-fixup (Med): a file script connector with
    /// the same name as an existing non-script connector (e.g. `git-repo`)
    /// must NOT collide — the SELECT filters `kind='script'`, so the file
    /// connector gets a fresh INSERT (new row, kind=script) instead of
    /// updating the non-script row's config (which would leave the wrong
    /// kind on it and silently break `collect_project_metrics`'s
    /// `kind==script` filter).
    #[tokio::test]
    async fn sync_connectors_file_same_name_non_script_coexists() {
        let db_path = tempdb_path();
        let store = SqliteStore::open(&db_path).await.expect("open store");
        let project_id = ProjectId::new();
        store
            .create_project(NewProject {
                id: project_id,
                name: "共存项目".into(),
                kind: "content".into(),
                desc: "".into(),
                provider: "github".into(),
            })
            .await
            .expect("create project");

        // Pre-existing non-script connector with the same name as the file
        // connector we're about to sync.
        store
            .create_connector(NewConnector {
                id: bw_core::ConnectorId::new(),
                name: "shared-name".into(),
                kind: "git-repo".into(),
                scope: String::new(),
                project_id: Some(project_id),
                config: "/workspace/path".into(),
            })
            .await
            .expect("create git-repo connector");

        // Sync a file script connector with the same name.
        let sync = ConnectorsFileSync {
            project_id,
            connectors: vec![ConnectorDefSync {
                name: "shared-name".into(),
                config: r#"{"script":"scripts/derive.py","command":"python","output":"data.json"}"#
                    .into(),
            }],
        };
        let summary = store.sync_connectors_file(sync).await.expect("sync");
        assert_eq!(summary.connectors_synced, 1);

        let connectors = store.list_connectors().await.expect("list");
        let project_connectors: Vec<_> = connectors
            .into_iter()
            .filter(|c| c.project_id == Some(project_id))
            .collect();
        // Both rows coexist: the non-script row is untouched, the script
        // row is a fresh INSERT.
        assert_eq!(project_connectors.len(), 2);
        let git_repo = project_connectors
            .iter()
            .find(|c| c.kind == "git-repo")
            .expect("git-repo row untouched");
        assert_eq!(git_repo.name, "shared-name");
        assert_eq!(git_repo.config, "/workspace/path");
        let script = project_connectors
            .iter()
            .find(|c| c.kind == "script")
            .expect("script row created");
        assert_eq!(script.name, "shared-name");
        assert!(script.config.contains("derive.py"));
        assert_ne!(script.id, git_repo.id);
    }

    /// V1 Issue2 Phase 3: empty connectors list is a valid no-op (file with
    /// zero `[[connector]]` entries — honest no-op, not an error).
    #[tokio::test]
    async fn sync_connectors_file_empty_is_noop() {
        let db_path = tempdb_path();
        let store = SqliteStore::open(&db_path).await.expect("open store");
        let project_id = ProjectId::new();
        store
            .create_project(NewProject {
                id: project_id,
                name: "空项目".into(),
                kind: "content".into(),
                desc: "".into(),
                provider: "github".into(),
            })
            .await
            .expect("create project");

        let sync = ConnectorsFileSync {
            project_id,
            connectors: vec![],
        };
        let summary = store.sync_connectors_file(sync).await.expect("empty sync");
        assert_eq!(summary.connectors_synced, 0);

        let connectors = store.list_connectors().await.expect("list");
        assert!(connectors.is_empty());
    }

    /// Create a unique temp DB path for test isolation.
    fn tempdb_path() -> String {
        let dir = std::env::temp_dir().join(format!(
            "bw-store-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // Remove any stale file from a previous run.
        let _ = std::fs::remove_file(&dir);
        dir.to_string_lossy().into_owned()
    }
}
