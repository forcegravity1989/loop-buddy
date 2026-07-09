-- Builders' Workbench — persistence slice (plan 03 §5 / §2.5 + 体系重构 v2).
--
-- Invariants encoded here:
--   * `observation` is append-only — the SOLE birthplace of a metric value.
--   * every `signal` / `hit` column is a NULLABLE write-through cache, written
--     ONLY by recompute_signals(); there is no setter.
--   * `handoff` is append-only — the SOLE birthplace of a stage transition;
--     `project.active_stage` is the derived-from-log current position.
--   * every table carries `updated_at + rev` so a future SyncCursor needs no
--     migration.
--
-- Pre-1.0: `stage_kind` values are a breaking rename (7 control points → 5
-- stage=role=methodology). No migration path from older dev databases —
-- delete and let the app recreate.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS project (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL,
    descr              TEXT NOT NULL DEFAULT '',
    phase              TEXT NOT NULL,            -- 'running' | 'cold_start'
    cycle              TEXT NOT NULL DEFAULT 'explore', -- ProjectCycle
    active_stage       TEXT NOT NULL DEFAULT 'prototype', -- StageKind
    north_star         TEXT NOT NULL DEFAULT '',
    ns_def             TEXT NOT NULL DEFAULT '',
    benchmark          TEXT NOT NULL DEFAULT '', -- 对标竞品(创建流程真输入)
    opportunity        TEXT NOT NULL DEFAULT '', -- 机会缺口/三月成功标准(创建流程真输入)
    workspace_path     TEXT NOT NULL DEFAULT '', -- 真执行器目标目录;空=未配置,只跑 Mock
    allow_commands     INTEGER NOT NULL DEFAULT 0, -- 真执行器是否额外放行 Bash(不只编辑文件)
    signal             TEXT,                     -- derived cache (L6)
    weekly_signal      TEXT,                     -- derived snapshot
    signal_derived_rev INTEGER,
    signal_derived_at  INTEGER,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    rev                INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS metric (
    id                 TEXT PRIMARY KEY,
    project_id         TEXT NOT NULL REFERENCES project(id),
    role               TEXT NOT NULL,            -- 'leading' | 'lagging'
    stage_kind         TEXT,                     -- stage this rolls up to
    name               TEXT NOT NULL,
    def                TEXT NOT NULL DEFAULT '',
    target_raw         TEXT NOT NULL DEFAULT '', -- mini-DSL: '≥5' '≤24h' '清零' …
    amber_kind         TEXT NOT NULL DEFAULT 'rel',  -- 'rel' | 'abs'
    amber_value        REAL NOT NULL DEFAULT 0.10,
    last_target        TEXT NOT NULL DEFAULT '',
    driver             TEXT NOT NULL DEFAULT '',
    signal             TEXT,                     -- derived cache (L2/L3)
    hit                INTEGER,                  -- derived cache (= signal==green)
    signal_derived_rev INTEGER,
    pos                INTEGER NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    rev                INTEGER NOT NULL DEFAULT 0
);

-- The ONLY place a value is born. Never updated, never deleted.
CREATE TABLE IF NOT EXISTS observation (
    id            TEXT PRIMARY KEY,
    metric_id     TEXT NOT NULL REFERENCES metric(id),
    ts            INTEGER NOT NULL,              -- unix seconds, as_of
    source_kind   TEXT NOT NULL,                -- 'manual' | 'connector' | …
    raw           TEXT NOT NULL,                -- display value: '60%' '5/7' '842ms'
    source_run_id TEXT,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_observation_metric_ts ON observation(metric_id, ts DESC);

CREATE TABLE IF NOT EXISTS op_stage (
    id                  TEXT PRIMARY KEY,
    project_id          TEXT NOT NULL REFERENCES project(id),
    kind                TEXT NOT NULL,           -- StageKind (5 values)
    progress            INTEGER NOT NULL DEFAULT 0,
    trend               TEXT NOT NULL DEFAULT '[]',  -- JSON [f32]
    dod                 TEXT NOT NULL DEFAULT '[]',  -- JSON [bool], indexed like StageKind::dod_items()
    routine_schedule    TEXT NOT NULL DEFAULT 'weekly',
    routine_signal      TEXT,                    -- derived cache (L4)
    routine_signal_rev  INTEGER,
    created_at          INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL,
    rev                 INTEGER NOT NULL DEFAULT 0,
    UNIQUE(project_id, kind)
);

CREATE TABLE IF NOT EXISTS session (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    stage_kind  TEXT,
    kind        TEXT NOT NULL,                   -- 'create' | 'optimize'
    title       TEXT NOT NULL,
    snippet     TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'active',  -- 'active' | 'archived' | 'done'
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    rev         INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS message (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES session(id),
    seq         INTEGER NOT NULL,
    role        TEXT NOT NULL,                   -- 'builder' | 'agent'
    text        TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_message_session_seq ON message(session_id, seq);

-- Weekly snapshot + auditable human override (never silently overwrites derived).
CREATE TABLE IF NOT EXISTS weekly_review (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id),
    week_of         INTEGER NOT NULL,
    derived_signal  TEXT NOT NULL,
    human_override  TEXT,
    override_reason TEXT,
    created_at      INTEGER NOT NULL
);

-- Append-only stage-transition audit log (体系重构 v2 §07③: 交棒清单 checked or
-- not, the handoff still happens — just marked risky). `Ops → Prototype` is
-- the reflux that closes the loop, same table, no special-casing.
CREATE TABLE IF NOT EXISTS handoff (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id),
    from_stage      TEXT NOT NULL,
    to_stage        TEXT NOT NULL,
    risky           INTEGER NOT NULL DEFAULT 0,
    note            TEXT NOT NULL DEFAULT '',
    created_at      INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_handoff_project_ts ON handoff(project_id, created_at DESC);

-- ═══════════════════════ hub library (global, no project_id) ═══════════════════════
-- Workflow/Skill/Agent hub tables are the FIRST global (non-project-scoped)
-- tables in this schema — a deliberate architectural first, not a retrofit:
-- a hub entry is a catalog/library item that exists independent of any
-- project and gets *imported into* one, not owned by one.

CREATE TABLE IF NOT EXISTS workflow_spec (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    kind_json      TEXT NOT NULL,             -- JSON-serialized WorkflowKind (Static|Dynamic)
    prompt         TEXT NOT NULL DEFAULT '',
    goal           TEXT NOT NULL DEFAULT '',
    stage_ref      INTEGER,                   -- 1..=5, nullable (metrics-layer / cross-cutting)
    phases         TEXT NOT NULL DEFAULT '[]', -- JSON [String]
    agents_json    TEXT NOT NULL DEFAULT '[]', -- JSON [AgentRef]
    skills_json    TEXT NOT NULL DEFAULT '[]', -- JSON [SkillRef]
    loop_retries   INTEGER NOT NULL DEFAULT 1,
    loop_max_iter  INTEGER NOT NULL DEFAULT 3,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    rev            INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_workflow_spec_stage ON workflow_spec(stage_ref);

CREATE TABLE IF NOT EXISTS skill (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    maturity    TEXT NOT NULL DEFAULT 'fresh',
    descr       TEXT NOT NULL DEFAULT '',
    category    TEXT NOT NULL DEFAULT '',
    source      TEXT NOT NULL DEFAULT 'self_built',
    uses        INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    rev         INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS agent (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL DEFAULT '',
    maturity    TEXT NOT NULL DEFAULT 'fresh',
    skills      TEXT NOT NULL DEFAULT '[]',   -- JSON [String] tag names
    model       TEXT NOT NULL DEFAULT '',
    runs        INTEGER NOT NULL DEFAULT 0,
    win_rate    TEXT NOT NULL DEFAULT '',     -- pre-formatted, e.g. '94%'
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    rev         INTEGER NOT NULL DEFAULT 0
);

-- Global, except project_id which is nullable (NULL = 全部项目/all projects) —
-- the one hub entity that legitimately optionally scopes to a project.
CREATE TABLE IF NOT EXISTS cron_task (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    target      TEXT NOT NULL DEFAULT '',
    schedule    TEXT NOT NULL DEFAULT 'weekly', -- Cadence text (reuses cadence_text/parse_cadence)
    project_id  TEXT REFERENCES project(id),
    status      TEXT NOT NULL DEFAULT 'normal',
    last_run    TEXT NOT NULL DEFAULT '',
    next_run    TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    rev         INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS connector (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'disconnected',
    last_sync   TEXT NOT NULL DEFAULT '',
    scope       TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    rev         INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS knowledge_source (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    kind          TEXT NOT NULL DEFAULT '',
    chunks        INTEGER NOT NULL DEFAULT 0,
    updated_label TEXT NOT NULL DEFAULT '',
    used_by       TEXT NOT NULL DEFAULT '',
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    rev           INTEGER NOT NULL DEFAULT 0
);
