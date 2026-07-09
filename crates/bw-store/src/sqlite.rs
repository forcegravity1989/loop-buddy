//! SQLite implementation of [`Store`] (sqlx, runtime-checked queries).
//!
//! Single-connection pool: a desktop app has one writer, so serializing access
//! sidesteps `SQLITE_BUSY` without ceremony.

use crate::{
    cadence_text, cycle_text, lib_source_text, maturity_text, parse_cadence,
    parse_connector_status, parse_cron_status, parse_cycle, parse_lib_source, parse_maturity,
    parse_session_status, parse_sig, parse_stage_kind, session_status_text, sig_text,
    stage_kind_text, GlobalHandoffRow, HandoffRow, MessageRow, MetricRole, MetricSignal, NewAgent,
    NewConnector, NewCronTask, NewKnowledgeSource, NewMetric, NewProject, NewSession, NewSkill,
    NewStage, NewWorkflowSpec, ObservationRow, PersistedSignals, ProjectRow, Result, SessionKind,
    SessionRow, StageRow, StageSignal, Store, StoreError,
};
use async_trait::async_trait;
use bw_core::derive::{
    evaluate_metric, measure, parse_target_with, reduce_worst_of, AmberBand, Measurement,
};
use bw_core::model::{
    AgentCard, AgentRef, AgentSkillTag, Connector, CronTask, HubSource, KnowledgeSource,
    LoopConfig, Maturity, ProjectCycle, ProjectPhase, Role, Signal, SkillCard, SkillRef,
    SourceKind, StageKind, WorkflowKind, WorkflowSpec,
};
use bw_core::{
    AgentId, ConnectorId, CronTaskId, KnowledgeSourceId, MetricId, ProjectId, SessionId, SkillId,
    WorkflowId,
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
        Ok(Self { pool })
    }
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
            "SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, signal, weekly_signal
             FROM project WHERE id=?",
        )
        .bind(pid(id))
        .fetch_optional(&self.pool)
        .await?;
        row.map(project_row).transpose()
    }

    async fn list_projects(&self) -> Result<Vec<ProjectRow>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, descr, phase, cycle, active_stage, north_star, ns_def, benchmark, opportunity, workspace_path, allow_commands, signal, weekly_signal
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
                (id, name, kind_json, prompt, goal, stage_ref, phases, agents_json, skills_json,
                 loop_retries, loop_max_iter, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(w.id.uuid().to_string())
        .bind(&w.name)
        .bind(serde_json::to_string(&w.kind)?)
        .bind(&w.prompt)
        .bind(&w.goal)
        .bind(w.stage_ref.map(i64::from))
        .bind(serde_json::to_string(&w.phases)?)
        .bind(serde_json::to_string(&w.agents)?)
        .bind(serde_json::to_string(&w.skills)?)
        .bind(i64::from(w.loop_config.retries))
        .bind(i64::from(w.loop_config.max_iter))
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_workflow_specs(&self) -> Result<Vec<WorkflowSpec>> {
        let rows = sqlx::query(
            "SELECT id, name, kind_json, prompt, goal, stage_ref, phases, agents_json, skills_json,
                    loop_retries, loop_max_iter
             FROM workflow_spec ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(workflow_spec_row).collect()
    }

    async fn get_workflow_spec(&self, id: WorkflowId) -> Result<Option<WorkflowSpec>> {
        let row = sqlx::query(
            "SELECT id, name, kind_json, prompt, goal, stage_ref, phases, agents_json, skills_json,
                    loop_retries, loop_max_iter
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
                (id, name, kind_json, prompt, goal, stage_ref, phases, agents_json, skills_json,
                 loop_retries, loop_max_iter, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(new_id.uuid().to_string())
        .bind(&from.name)
        .bind(serde_json::to_string(&kind)?)
        .bind(&from.prompt)
        .bind(&from.goal)
        .bind(from.stage_ref.map(i64::from))
        .bind(serde_json::to_string(&from.phases)?)
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

    async fn create_skill(&self, s: NewSkill) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO skill (id, name, maturity, descr, category, source, uses, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, ?, 0)",
        )
        .bind(s.id.uuid().to_string())
        .bind(&s.name)
        .bind(maturity_text(s.maturity))
        .bind(&s.desc)
        .bind(&s.category)
        .bind(lib_source_text(s.source))
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_skills(&self) -> Result<Vec<SkillCard>> {
        let rows = sqlx::query(
            "SELECT id, name, maturity, descr, category, source, uses FROM skill ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(skill_row).collect()
    }

    async fn get_skill(&self, id: SkillId) -> Result<Option<SkillCard>> {
        let row = sqlx::query(
            "SELECT id, name, maturity, descr, category, source, uses FROM skill WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(skill_row).transpose()
    }

    async fn create_agent(&self, a: NewAgent) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO agent (id, name, role, maturity, skills, model, runs, win_rate, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, ?, 0, '', ?, ?, 0)",
        )
        .bind(a.id.uuid().to_string())
        .bind(&a.name)
        .bind(&a.role)
        .bind(maturity_text(a.maturity))
        .bind(serde_json::to_string(&a.skills)?)
        .bind(&a.model)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<AgentCard>> {
        let rows = sqlx::query(
            "SELECT id, name, role, maturity, skills, model, runs, win_rate FROM agent ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(agent_row).collect()
    }

    async fn get_agent(&self, id: AgentId) -> Result<Option<AgentCard>> {
        let row = sqlx::query(
            "SELECT id, name, role, maturity, skills, model, runs, win_rate FROM agent WHERE id=?",
        )
        .bind(id.uuid().to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(agent_row).transpose()
    }

    async fn create_cron_task(&self, c: NewCronTask) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO cron_task (id, name, target, schedule, project_id, status, last_run, next_run, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, ?, 'normal', '', '', ?, ?, 0)",
        )
        .bind(c.id.uuid().to_string())
        .bind(&c.name)
        .bind(&c.target)
        .bind(cadence_text(&c.schedule))
        .bind(c.project_id.map(pid))
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_cron_tasks(&self) -> Result<Vec<CronTask>> {
        let rows = sqlx::query(
            "SELECT id, name, target, schedule, project_id, status, last_run, next_run
             FROM cron_task ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(cron_task_row).collect()
    }

    async fn create_connector(&self, c: NewConnector) -> Result<()> {
        let t = now_unix();
        sqlx::query(
            "INSERT INTO connector (id, name, kind, status, last_sync, scope, created_at, updated_at, rev)
             VALUES (?, ?, ?, 'disconnected', '', ?, ?, ?, 0)",
        )
        .bind(c.id.uuid().to_string())
        .bind(&c.name)
        .bind(&c.kind)
        .bind(&c.scope)
        .bind(t)
        .bind(t)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn list_connectors(&self) -> Result<Vec<Connector>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, status, last_sync, scope FROM connector ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(connector_row).collect()
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
    })
}

fn workflow_spec_row(r: sqlx::sqlite::SqliteRow) -> Result<WorkflowSpec> {
    let id = parse_uuid(&r.get::<String, _>("id"), WorkflowId::from_uuid)?;
    let kind: WorkflowKind = serde_json::from_str(&r.get::<String, _>("kind_json"))?;
    let phases: Vec<String> = serde_json::from_str(&r.get::<String, _>("phases"))?;
    let agents: Vec<AgentRef> = serde_json::from_str(&r.get::<String, _>("agents_json"))?;
    let skills: Vec<SkillRef> = serde_json::from_str(&r.get::<String, _>("skills_json"))?;
    Ok(WorkflowSpec {
        id,
        name: r.get("name"),
        kind,
        prompt: r.get("prompt"),
        goal: r.get("goal"),
        stage_ref: r.get::<Option<i64>, _>("stage_ref").map(|v| v as u8),
        phases,
        agents,
        skills,
        loop_config: LoopConfig {
            retries: r.get::<i64, _>("loop_retries") as u8,
            max_iter: r.get::<i64, _>("loop_max_iter") as u8,
        },
    })
}

fn skill_row(r: sqlx::sqlite::SqliteRow) -> Result<SkillCard> {
    let id = parse_uuid(&r.get::<String, _>("id"), SkillId::from_uuid)?;
    Ok(SkillCard {
        id,
        name: r.get("name"),
        maturity: parse_maturity(&r.get::<String, _>("maturity")),
        desc: r.get("descr"),
        category: r.get("category"),
        source: parse_lib_source(&r.get::<String, _>("source")),
        uses: r.get::<i64, _>("uses") as u32,
    })
}

fn agent_row(r: sqlx::sqlite::SqliteRow) -> Result<AgentCard> {
    let id = parse_uuid(&r.get::<String, _>("id"), AgentId::from_uuid)?;
    let skills: Vec<String> = serde_json::from_str(&r.get::<String, _>("skills"))?;
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
    })
}

fn cron_task_row(r: sqlx::sqlite::SqliteRow) -> Result<CronTask> {
    let id = parse_uuid(&r.get::<String, _>("id"), CronTaskId::from_uuid)?;
    let project_id = r
        .get::<Option<String>, _>("project_id")
        .map(|s| parse_uuid(&s, ProjectId::from_uuid))
        .transpose()?;
    Ok(CronTask {
        id,
        name: r.get("name"),
        target: r.get("target"),
        schedule: parse_cadence(&r.get::<String, _>("schedule")),
        project_id,
        status: parse_cron_status(&r.get::<String, _>("status")),
        last_run: r.get("last_run"),
        next_run: r.get("next_run"),
    })
}

fn connector_row(r: sqlx::sqlite::SqliteRow) -> Result<Connector> {
    let id = parse_uuid(&r.get::<String, _>("id"), ConnectorId::from_uuid)?;
    Ok(Connector {
        id,
        name: r.get("name"),
        kind: r.get("kind"),
        status: parse_connector_status(&r.get::<String, _>("status")),
        last_sync: r.get("last_sync"),
        scope: r.get("scope"),
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
