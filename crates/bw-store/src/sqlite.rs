//! SQLite implementation of [`Store`] (sqlx, runtime-checked queries).
//!
//! Single-connection pool: a desktop app has one writer, so serializing access
//! sidesteps `SQLITE_BUSY` without ceremony.

use crate::{
    cadence_text, parse_cadence, parse_session_status, parse_sig, parse_stage_kind,
    session_status_text, sig_text, stage_kind_text, MessageRow, MetricRole, MetricSignal,
    NewMetric, NewProject, NewSession, NewStage, ObservationRow, PersistedSignals, ProjectRow,
    Result, SessionKind, SessionRow, StageRow, StageSignal, Store, StoreError,
};
use async_trait::async_trait;
use bw_core::derive::{
    evaluate_metric, measure, parse_target_with, reduce_worst_of, AmberBand, Measurement,
};
use bw_core::model::{ProjectPhase, Role, Signal, SourceKind, StageKind, StagePhase};
use bw_core::{MetricId, ProjectId, SessionId};
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

        // Additive migrations for databases created before a column existed.
        // `CREATE TABLE IF NOT EXISTS` skips existing tables, so new columns
        // must be back-filled; "duplicate column name" just means it's there.
        for alter in [
            "ALTER TABLE project ADD COLUMN benchmark TEXT NOT NULL DEFAULT ''",
            "ALTER TABLE project ADD COLUMN opportunity TEXT NOT NULL DEFAULT ''",
        ] {
            if let Err(e) = sqlx::query(alter).execute(&pool).await {
                if !e.to_string().contains("duplicate column name") {
                    return Err(e.into());
                }
            }
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
fn stage_phase_text(p: StagePhase) -> &'static str {
    match p {
        StagePhase::Finalized => "finalized",
        StagePhase::Iterating => "iterating",
        StagePhase::Monitoring => "monitoring",
        StagePhase::Running => "running",
    }
}
fn parse_stage_phase(s: &str) -> StagePhase {
    match s {
        "finalized" => StagePhase::Finalized,
        "iterating" => StagePhase::Iterating,
        "monitoring" => StagePhase::Monitoring,
        _ => StagePhase::Running,
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
            "INSERT INTO project (id, name, kind, descr, phase, cold_step, created_at, updated_at, rev)
             VALUES (?, ?, ?, ?, 'cold_start', 0, ?, ?, 0)",
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

    async fn set_project_phase(
        &self,
        id: ProjectId,
        phase: ProjectPhase,
        cold_step: Option<u8>,
    ) -> Result<()> {
        sqlx::query("UPDATE project SET phase=?, cold_step=?, updated_at=?, rev=rev+1 WHERE id=?")
            .bind(phase_text(phase))
            .bind(cold_step.map(|v| v as i64))
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
            sqlx::query(
                "INSERT INTO op_stage
                    (id, project_id, kind, phase, progress, routine_schedule, owns, accept, control,
                     created_at, updated_at, rev)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
                 ON CONFLICT(project_id, kind) DO NOTHING",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(pid(s.project_id))
            .bind(stage_kind_text(s.kind))
            .bind(stage_phase_text(s.phase))
            .bind(s.progress as i64)
            .bind(cadence_text(&s.schedule))
            .bind(&s.owns)
            .bind(&s.accept)
            .bind(&s.control)
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
            "SELECT id, name, kind, descr, phase, cold_step, north_star, ns_def, benchmark, opportunity, signal, weekly_signal
             FROM project WHERE id=?",
        )
        .bind(pid(id))
        .fetch_optional(&self.pool)
        .await?;
        row.map(project_row).transpose()
    }

    async fn list_projects(&self) -> Result<Vec<ProjectRow>> {
        let rows = sqlx::query(
            "SELECT id, name, kind, descr, phase, cold_step, north_star, ns_def, benchmark, opportunity, signal, weekly_signal
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
            "SELECT kind, phase, progress, trend, routine_schedule, owns, accept, control, routine_signal
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
                    phase: parse_stage_phase(&r.get::<String, _>("phase")),
                    progress: r.get::<i64, _>("progress").clamp(0, 100) as u8,
                    trend: serde_json::from_str(&r.get::<String, _>("trend")).unwrap_or_default(),
                    schedule: parse_cadence(&r.get::<String, _>("routine_schedule")),
                    owns: r.get("owns"),
                    accept: r.get("accept"),
                    control: r.get("control"),
                    routine_signal: r
                        .get::<Option<String>, _>("routine_signal")
                        .and_then(|s| parse_sig(&s)),
                })
            })
            .collect();
        // Control-point order, not insertion order.
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
}

fn project_row(r: sqlx::sqlite::SqliteRow) -> Result<ProjectRow> {
    let id = parse_uuid(&r.get::<String, _>("id"), ProjectId::from_uuid)?;
    Ok(ProjectRow {
        id,
        name: r.get("name"),
        kind: r.get("kind"),
        desc: r.get("descr"),
        phase: parse_phase(&r.get::<String, _>("phase")),
        cold_step: r.get::<Option<i64>, _>("cold_step").map(|v| v as u8),
        north_star: r.get("north_star"),
        ns_def: r.get("ns_def"),
        benchmark: r.get("benchmark"),
        opportunity: r.get("opportunity"),
        signal: r
            .get::<Option<String>, _>("signal")
            .and_then(|s| parse_sig(&s)),
        weekly_signal: r
            .get::<Option<String>, _>("weekly_signal")
            .and_then(|s| parse_sig(&s)),
    })
}
