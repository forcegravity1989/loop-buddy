//! SQLite implementation of [`Store`] (sqlx, runtime-checked queries).
//!
//! Single-connection pool: a desktop app has one writer, so serializing access
//! sidesteps `SQLITE_BUSY` without ceremony.

use crate::{
    cadence_text, connector_status_text, cron_mode_text, cron_status_text, cycle_text,
    hub_source_columns, issue_priority_text, issue_status_text, maturity_text, parse_cadence,
    parse_connector_status, parse_cron_mode, parse_cron_status, parse_cycle, parse_hub_source,
    parse_issue_priority, parse_issue_status, parse_maturity, parse_session_status, parse_sig,
    parse_stage_kind, session_status_text, sig_text, stage_kind_text, AgentEdit, GlobalHandoffRow,
    HandoffRow, MessageRow, MetricRole, MetricSignal, NewAgent, NewArtifact, NewConnector,
    NewCronTask, NewIssue, NewKnowledgeSource, NewMetric, NewProject, NewSession, NewSkill,
    NewSkillFile, NewStage, NewWorkflowRun, NewWorkflowSpec, ObservationRow, PersistedSignals,
    ProjectRow, Result, SessionKind, SessionRow, SkillEdit, SkillFileRow, StageRow, StageSignal,
    Store, StoreError, WorkflowEdit,
};
use async_trait::async_trait;
use bw_core::derive::{
    evaluate_metric, measure, parse_target_with, reduce_worst_of, AmberBand, Measurement,
};
use bw_core::model::{
    AgentCard, AgentRef, AgentSkillTag, Artifact, ArtifactKind, Connector, ConnectorStatus,
    CronEffectiveness, CronStatus, CronTask, HubSource, Issue, IssueStatus, KnowledgeSource,
    LoopConfig, Maturity, PhaseMeta, ProjectCycle, ProjectPhase, Role, RunStatus, RunTrigger,
    Signal, SkillCard, SkillRef, SourceKind, StageKind, UsageRank, WorkflowKind, WorkflowRun,
    WorkflowRunAnalytics, WorkflowSpec, WorkflowVersion,
};
use bw_core::{
    AgentId, ArtifactId, ConnectorId, CronTaskId, IssueId, KnowledgeSourceId, MetricId, ProjectId,
    SessionId, SkillFileId, SkillId, WorkflowId, WorkflowRunId,
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

        Ok(Self { pool })
    }
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

fn phase_text(p: ProjectPhase) -> &'static str {
    match p {
        ProjectPhase::Running => "running",
        ProjectPhase::ColdStart => "cold_start",
    }
}
fn parse_phase(s: &str) -> ProjectPhase {
    match s {
        "running" => ProjectPhase::Running,
        _ => ProjectPhase::ColdStart,
    }
}
fn role_text(r: Role) -> &'static str {
    match r {
        Role::Builder => "builder",
        Role::Agent => "agent",
    }
}
fn parse_role(s: &str) -> Role {
    match s {
        "agent" => Role::Agent,
        _ => Role::Builder,
    }
}
fn source_text(s: SourceKind) -> &'static str {
    match s {
        SourceKind::GatewayLog => "gateway_log",
        SourceKind::Ci => "ci",
        SourceKind::GitPr => "git_pr",
        SourceKind::Telemetry => "telemetry",
        SourceKind::Connector => "connector",
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

#[async_trait]
impl Store for SqliteStore {
    async fn create_project(&self, p: NewProject) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO project (id, name, kind, descr, phase, cycle, active_stage, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, 'cold_start', 'explore', 'prototype', ?, ?, 0)",
        )
        .bind(pid(p.id))
        .bind(&p.name)
        .bind(&p.kind)
        .bind(&p.desc)
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

    async fn set_project_phase(&self, id: ProjectId, phase: ProjectPhase) -> Result<()> {
        sqlx::query("UPDATE project SET phase=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(phase_text(phase))
            .bind(now_unix())
            .bind(pid(id))
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn set_project_cycle(&self, id: ProjectId, cycle: ProjectCycle) -> Result<()> {
        sqlx::query("UPDATE project SET cycle=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(cycle_text(cycle))
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

    async fn upsert_metric(&self, m: NewMetric) -> Result<()> {
        let (ak, av) = amber_parts(m.amber);
        let t = now_unix();
        sqlx::query(
            "INSERT INTO metric
                (id, project_id, role, stage_kind, name, def, target_raw, amber_kind, amber_value,
                 last_target, driver, pos, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
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
        .bind(t)
        .bind(t)
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

    async fn append_message(&self, session_id: SessionId, role: Role, text: &str) -> Result<()> {
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
        let metric_rows = sqlx::query(
            "SELECT id, stage_kind, target_raw, amber_kind, amber_value FROM metric WHERE project_id=?",
        )
        .bind(&p)
        .fetch_all(&self.pool)
        .await?;
        let mut by_stage: HashMap<StageKind, Vec<Signal>> = HashMap::new();
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

        // L6: project signal = worst-of its stages; weekly_signal = snapshot.
        let proj = reduce_worst_of(stages.iter().map(|(k, _)| stage_signal[k])).into_inner();
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
            "SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, signal, weekly_signal, created_at
             FROM project WHERE id=?",
        )
        .bind(pid(id))
        .fetch_optional(&self.pool)
        .await?;
        row.map(project_row).transpose()
    }

    async fn list_projects(&self) -> Result<Vec<ProjectRow>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, signal, weekly_signal, created_at
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
                 skills_json, loop_retries, loop_max_iter, project_id, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
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
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_workflow_specs(&self) -> Result<Vec<WorkflowSpec>> {
        let rows = sqlx::query(
            "SELECT id, name, kind_json, prompt, goal, stage_ref, phases, phase_prompts,
                    agents_json, skills_json, loop_retries, loop_max_iter, project_id
             FROM workflow_spec ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(workflow_spec_row).collect()
    }

    async fn get_workflow_spec(&self, id: WorkflowId) -> Result<Option<WorkflowSpec>> {
        let row = sqlx::query(
            "SELECT id, name, kind_json, prompt, goal, stage_ref, phases, phase_prompts,
                    agents_json, skills_json, loop_retries, loop_max_iter, project_id
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
                 skills_json, loop_retries, loop_max_iter, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
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
        sqlx::query(
            "INSERT INTO skill (id, name, maturity, descr, category, source, official_library, uses, content, project_id, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 0)",
        )
        .bind(s.id.uuid().to_string())
        .bind(&s.name)
        .bind(maturity_text(s.maturity))
        .bind(&s.desc)
        .bind(&s.category)
        .bind(source_tag)
        .bind(official_library)
        .bind(&s.content)
        .bind(s.project_id.map(pid))
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_skill(&self, id: SkillId, edit: SkillEdit) -> Result<()> {
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
        Ok(())
    }

    async fn list_skills(&self) -> Result<Vec<SkillCard>> {
        let rows = sqlx::query(
            "SELECT id, name, maturity, descr, category, source, official_library, uses, content,
                    distilled_from_issue, origin_agent, project_id
             FROM skill ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(skill_row).collect()
    }

    async fn get_skill(&self, id: SkillId) -> Result<Option<SkillCard>> {
        let row = sqlx::query(
            "SELECT id, name, maturity, descr, category, source, official_library, uses, content,
                    distilled_from_issue, origin_agent, project_id
             FROM skill WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_row).transpose()
    }

    async fn record_skill_use_by_name(&self, name: &str) -> Result<u32> {
        let res = sqlx::query("UPDATE skill SET uses=uses+1, updated_at=?, rev=rev+1 WHERE name=?")
            .bind(now_unix())
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() as u32)
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
        sqlx::query(
            "INSERT INTO skill
                (id, name, maturity, descr, category, source, official_library, uses, content,
                 distilled_from_issue, origin_agent, project_id,
                 created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(skill.id.uuid().to_string())
        .bind(&skill.name)
        .bind(maturity_text(Maturity::Polishing))
        .bind(&skill.desc)
        .bind(&skill.category)
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
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn import_skill_package(&self, skill: NewSkill, files: Vec<NewSkillFile>) -> Result<()> {
        let t = now_unix();
        let (source_tag, official_library) = hub_source_columns(&skill.source);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO skill (id, name, maturity, descr, category, source, official_library, uses, content, project_id, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 0)",
        )
        .bind(skill.id.uuid().to_string())
        .bind(&skill.name)
        .bind(maturity_text(skill.maturity))
        .bind(&skill.desc)
        .bind(&skill.category)
        .bind(source_tag)
        .bind(official_library)
        .bind(&skill.content)
        .bind(skill.project_id.map(pid))
        .bind(t)
        .bind(t)
        .execute(&mut *tx)
        .await?;

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

    async fn create_agent(&self, a: NewAgent) -> Result<()> {
        let t = now_unix();
        let (source_tag, official_library) = hub_source_columns(&a.source);
        sqlx::query(
            "INSERT INTO agent (id, name, role, maturity, skills, model, runs, win_rate, instructions, wins, tools, agent_cli, source, official_library, project_id, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, 0, '', ?, 0, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(a.id.uuid().to_string())
        .bind(&a.name)
        .bind(&a.role)
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
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<AgentCard>> {
        let rows = sqlx::query(
            "SELECT id, name, role, maturity, skills, model, runs, win_rate, instructions, tools, agent_cli, source, official_library, project_id FROM agent ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(agent_row).collect()
    }

    async fn get_agent(&self, id: AgentId) -> Result<Option<AgentCard>> {
        let row = sqlx::query(
            "SELECT id, name, role, maturity, skills, model, runs, win_rate, instructions, tools, agent_cli, source, official_library, project_id FROM agent WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(agent_row).transpose()
    }

    async fn record_agent_run_by_name(&self, name: &str, ok: bool) -> Result<u32> {
        // runs/wins are the real counters; win_rate is a derived display
        // string recomputed from them in the same statement — never patched
        // independently, so it can't drift from the counters it summarizes.
        let res = sqlx::query(
            "UPDATE agent SET runs=runs+1, wins=wins+?, \
             win_rate = printf('%d%%', (wins+?)*100/(runs+1)), \
             updated_at=?, rev=rev+1 WHERE name=?",
        )
        .bind(if ok { 1 } else { 0 })
        .bind(if ok { 1 } else { 0 })
        .bind(now_unix())
        .bind(name)
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() as u32)
    }

    async fn create_cron_task(&self, c: NewCronTask) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO cron_task (id, name, target, schedule, project_id, status, last_run, next_run, mode, issue_stage, issue_assignee, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, 'normal', '', '', ?, ?, ?, ?, ?, 0)",
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
                (id, project_id, stage, number, title, descr, status, priority, assignee,
                 created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, 'backlog', ?, NULL, ?, ?)",
        )
        .bind(i.id.uuid().to_string())
        .bind(pid(i.project_id))
        .bind(stage_kind_text(i.stage))
        .bind(number)
        .bind(&i.title)
        .bind(&i.desc)
        .bind(issue_priority_text(i.priority))
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
            "SELECT id, project_id, stage, number, title, descr, status, priority, assignee,
                    settled_at, blocked_reason, created_at, updated_at
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
            "SELECT id, project_id, stage, number, title, descr, status, priority, assignee,
                    settled_at, blocked_reason,
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
    })
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
    Ok(SkillCard {
        id,
        name: r.get("name"),
        maturity: parse_maturity(&r.get::<String, _>("maturity")),
        desc: r.get("descr"),
        category: r.get("category"),
        source: parse_hub_source(
            &r.get::<String, _>("source"),
            &r.get::<String, _>("official_library"),
        ),
        uses: r.get::<i64, _>("uses") as u32,
        content: r.get("content"),
        distilled_from_issue,
        origin_agent,
        project_id,
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
    Ok(AgentCard {
        id,
        name: r.get("name"),
        role: r.get("role"),
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
        source: parse_hub_source(
            &r.get::<String, _>("source"),
            &r.get::<String, _>("official_library"),
        ),
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
        title: r.get("title"),
        desc: r.get("descr"),
        status: parse_issue_status(&r.get::<String, _>("status")),
        priority: parse_issue_priority(&r.get::<String, _>("priority")),
        assignee,
        settled_at: r.get("settled_at"),
        blocked_reason: r
            .get::<Option<String>, _>("blocked_reason")
            .filter(|s| !s.is_empty()),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    })
}
