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

-- ═══════════════════════ hub library (global by default) ═══════════════════════
-- Workflow/Skill/Agent hub tables were originally global-only (no project_id) —
-- a deliberate architectural first: a hub entry is a catalog/library item that
-- exists independent of any project. 2026-07-20 践行最小切片(plan/09 墙 B)
-- added a nullable `project_id`: NULL keeps the original global/shared
-- semantics byte-for-byte; non-NULL marks a project-owned row. Query-side
-- scoping (指派下拉/技能注入/剧本选择只看本项目) is deliberately NOT part of
-- this slice — that's plan/08 的 P2,一次性做够,不留半破的收窄。

CREATE TABLE IF NOT EXISTS workflow_spec (
    id             TEXT PRIMARY KEY,
    name           TEXT NOT NULL,
    -- 践行最小切片(2026-07-20,见 plan/09 墙 B):NULL = hub library(全局,现有
    -- 行为不变);非 NULL = 项目自有。查询收窄到 project_id 是 P2 全量的事,
    -- 这里只落列+落值,不改任何现有查询口径。
    project_id     TEXT REFERENCES project(id),
    kind_json      TEXT NOT NULL,             -- JSON-serialized WorkflowKind (Static|Dynamic)
    prompt         TEXT NOT NULL DEFAULT '',
    goal           TEXT NOT NULL DEFAULT '',
    stage_ref      INTEGER,                   -- 1..=5, nullable (metrics-layer / cross-cutting)
    phases         TEXT NOT NULL DEFAULT '[]', -- JSON [String]
    phase_prompts  TEXT NOT NULL DEFAULT '[]', -- JSON [String], index-aligned with phases;
                                               -- '[]' = pre-playbook (all phases share prompt)
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
    -- 践行最小切片(2026-07-20):NULL = hub library(全局);非 NULL = 项目自有。
    project_id  TEXT REFERENCES project(id),
    maturity    TEXT NOT NULL DEFAULT 'fresh',
    descr       TEXT NOT NULL DEFAULT '',
    category    TEXT NOT NULL DEFAULT '',
    source      TEXT NOT NULL DEFAULT 'self_built',
    -- T2 (plan/12 §6): sub-tag for source='official' only — which curated
    -- external library ("mattpocock-skills"/"superpowers"/"ecc"/…). '' for
    -- every other source value (see bw_store::parse_skill_source).
    official_library TEXT NOT NULL DEFAULT '',
    uses        INTEGER NOT NULL DEFAULT 0,
    -- R2: provenance link from a real completed Issue. NULL = catalog/seeded
    -- skill (no real-work origin); populated only by distill_skill_from_issue.
    distilled_from_issue TEXT,                   -- IssueId uuid string; NULL = catalog
    origin_agent         TEXT,                   -- AgentId uuid string; NULL = catalog
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    rev         INTEGER NOT NULL DEFAULT 0
);

-- T2 (plan/12 §2): a skill's real support files, copy-on-import — the
-- non-SKILL.md contents of an imported skill folder (SkillCard.content stays
-- SKILL.md's own body only). No predetermined category/subfolder scheme:
-- rel_path is the real path as found on disk ("references/mocking.md",
-- "agents/openai.yaml", a bare "GLOSSARY.md", …).
CREATE TABLE IF NOT EXISTS skill_file (
    id         TEXT PRIMARY KEY,
    skill_id   TEXT NOT NULL REFERENCES skill(id),
    rel_path   TEXT NOT NULL,
    content    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_skill_file_skill ON skill_file(skill_id);

CREATE TABLE IF NOT EXISTS agent (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    -- 践行最小切片(2026-07-20):NULL = hub library(全局);非 NULL = 项目自有。
    project_id  TEXT REFERENCES project(id),
    role        TEXT NOT NULL DEFAULT '',
    maturity    TEXT NOT NULL DEFAULT 'fresh',
    skills      TEXT NOT NULL DEFAULT '[]',   -- JSON [String] tag names
    model       TEXT NOT NULL DEFAULT '',
    runs        INTEGER NOT NULL DEFAULT 0,
    win_rate    TEXT NOT NULL DEFAULT '',     -- pre-formatted, e.g. '94%'
    -- T5 (2026-07-23, plan/12 §3): "Agent" == AGENT.md real modeling.
    -- AllowedTools JSON [String] — '[]' = no restriction declared (honest for
    -- the five built-in stage-role agents and any hand-authored row).
    tools       TEXT NOT NULL DEFAULT '[]',
    -- Which Agent CLI executes this agent; first version only "claude-code"
    -- has a real executor (bw-engine::ClaudeCliExecutor).
    agent_cli   TEXT NOT NULL DEFAULT 'claude-code',
    -- Provenance — same HubSource tag/sub-tag two-column scheme T2 gave
    -- `skill` (plan/12 §6). 'self_built' = honest default for the five
    -- built-in stage-role agents; ImportAgentDefinition's ECC rows write
    -- 'official' + official_library='ecc'.
    source      TEXT NOT NULL DEFAULT 'self_built',
    official_library TEXT NOT NULL DEFAULT '',
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
    -- Unix seconds, 0 = never run. Kept separate from the pre-formatted
    -- `last_run` display string so the real scheduler (App::tick_scheduler)
    -- has a machine-comparable clock to test "due" against — parsing the
    -- display string back would be fragile. See bw_core::model::cron_due.
    last_run_at INTEGER NOT NULL DEFAULT 0,
    -- A1: what this task does when due. Defaults to run_workflow so pre-A1
    -- rows keep their semantics byte-for-byte; create_issue mints an Issue.
    mode            TEXT NOT NULL DEFAULT 'run_workflow',
    issue_stage     TEXT,                        -- A1: stage for a create_issue task
    issue_assignee  TEXT,                        -- A1: agent name to assign (NULL = unassigned)
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

-- ═══════════════════════ workflow_run (iter 1 · telemetry foundation) ═══════════════════════
-- Append-only execution log: the SOLE birthplace of "did this run succeed,
-- how long did it take, who fired it". Every workflow execution — manual
-- (RunWorkflow/RunHubWorkflow) and the background scheduler's auto-fire
-- (tick_scheduler) — records a row here. This is the grain optimization
-- intelligence (iters 6-12) is built on; without it, no "this workflow fails
-- 30%" claim is knowable.
--
-- A row is inserted at status='running' when the run starts, then its
-- status/finished_at/duration_ms/phases_completed/error are settled exactly
-- once when the engine returns. `params_json` is reserved for iter 3
-- (parameter capture) and is '' until then.
CREATE TABLE IF NOT EXISTS workflow_run (
    id               TEXT PRIMARY KEY,
    workflow_id      TEXT NOT NULL,                 -- FK omitted: a run may outlive a deleted spec
    workflow_name    TEXT NOT NULL DEFAULT '',      -- snapshot of name at run time
    project_id       TEXT,                          -- nullable: a hub run need not bind a project
    session_id       TEXT,
    trigger          TEXT NOT NULL DEFAULT 'manual',-- 'manual' | 'scheduled'
    status           TEXT NOT NULL DEFAULT 'running',-- 'running' | 'ok' | 'failed'
    started_at       INTEGER NOT NULL,
    finished_at      INTEGER,
    duration_ms      INTEGER,
    phases_completed INTEGER NOT NULL DEFAULT 0,
    error            TEXT NOT NULL DEFAULT '',
    params_json      TEXT NOT NULL DEFAULT '',
    -- A2: the Issue this run executes (NULL unless fired by RunIssue). Kept
    -- denormalized (no FK) so a run survives its issue being deleted — the
    -- linkage is the point, an orphaned run is still honest evidence.
    issue_id         TEXT,
    -- P4: workspace HEAD at run start / settle (NULL = no real workspace).
    -- The pair answers "这次运行改了什么" via git diff — recorded fact, not
    -- recomputed later. Applied to old DBs by add_column_if_missing.
    head_before      TEXT,
    head_after       TEXT,
    created_at       INTEGER NOT NULL
);
-- iter 4: link a scheduled run back to the cron task that fired it (NULL for
-- manual runs). Added via guarded migration so pre-iter-4 DBs keep opening.
-- Kept denormalized (not a FK) so a run survives its task being deleted — the
-- history is the point, and an orphaned run is still honest evidence.
--   NOTE: applied by add_column_if_missing in SqliteStore::open, not inline.
CREATE INDEX IF NOT EXISTS idx_workflow_run_spec ON workflow_run(workflow_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_workflow_run_proj ON workflow_run(project_id, started_at DESC);

-- ═══════════════════════ artifact (完整形态 · 真实产物登记) ═══════════════════════
-- Append-only registry of real workspace files. One row = one *version* of a
-- file: identity is (project, path, git_commit) — re-registering the same
-- path at the same commit is a no-op (INSERT OR IGNORE); at a new commit it
-- appends a new row, so the rows sharing a path ARE that artifact's version
-- history. Rows are only ever harvested from a real `git ls-files` scan
-- (bw-engine::evidence), never typed in; `workflow_run_id` links a version to
-- the run whose settle-time scan first saw it (NULL for manual collects).
CREATE TABLE IF NOT EXISTS artifact (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id),
    workflow_run_id TEXT,                        -- no FK: a registration outlives a purged run
    issue_id        TEXT,                        -- A2: the Issue whose Done-edge registered this version
    stage_kind      TEXT,                        -- StageKind at registration time, if known
    path            TEXT NOT NULL,               -- workspace-relative, git's own form
    kind            TEXT NOT NULL DEFAULT 'other', -- bw_core::model::ArtifactKind::text()
    bytes           INTEGER NOT NULL DEFAULT 0,  -- real stat at scan time
    git_commit      TEXT NOT NULL DEFAULT '',    -- short HEAD at scan; '' = commitless repo
    registered_at   INTEGER NOT NULL,
    UNIQUE(project_id, path, git_commit)
);
CREATE INDEX IF NOT EXISTS idx_artifact_project ON artifact(project_id, registered_at DESC);

-- ═══════════════════════ workflow_version (iter 5 · evolution history) ═══════════════════════
-- Append-only snapshot of a Static spec's content, taken the instant BEFORE
-- each `UpdateWorkflowSpec` overwrites it. So the live `workflow_spec` row is
-- always the latest, but every prior version's prompt/goal/phases/agents/
-- skills survives here — the diff/rollback/A-B material optimization
-- intelligence (iter 14) and the "what did we change and why" audit need.
--
-- A row records version N's content, written when version N+1 is being
-- authored (i.e. the snapshot is of what's about to be replaced). No FK on
-- workflow_id: a version outlives its spec being deleted (the history is the
-- point). `version` matches the `Static.version` that was current pre-update.
CREATE TABLE IF NOT EXISTS workflow_version (
    id             TEXT PRIMARY KEY,
    workflow_id    TEXT NOT NULL,
    version        INTEGER NOT NULL,
    name           TEXT NOT NULL DEFAULT '',
    prompt         TEXT NOT NULL DEFAULT '',
    goal           TEXT NOT NULL DEFAULT '',
    phases         TEXT NOT NULL DEFAULT '[]',
    phase_prompts  TEXT NOT NULL DEFAULT '[]', -- frozen with the rest of the content
    agents_json    TEXT NOT NULL DEFAULT '[]',
    skills_json    TEXT NOT NULL DEFAULT '[]',
    loop_retries   INTEGER NOT NULL DEFAULT 1,
    loop_max_iter  INTEGER NOT NULL DEFAULT 3,
    note           TEXT NOT NULL DEFAULT '',
    created_at     INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_workflow_version_spec ON workflow_version(workflow_id, version DESC);

-- ═══════════════════════ issue (R1 · assignable stage-scoped work) ═══════════════════════
-- An assignable unit of work scoped to a project's stage — the multica "assign
-- a task to a teammate" model fused into BW's stage ring. `number` is
-- per-project (1, 2, 3, …), auto-assigned at creation. `assignee` is nullable
-- (NULL = unassigned). `status` is a kanban lifecycle (Backlog → Done /
-- Cancelled; Blocked is recoverable, not terminal).
CREATE TABLE IF NOT EXISTS issue (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    stage       TEXT NOT NULL,                   -- StageKind (5 values)
    number      INTEGER NOT NULL,                -- per-project sequence: 1, 2, 3, …
    title       TEXT NOT NULL,
    descr       TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'backlog', -- IssueStatus
    priority    TEXT NOT NULL DEFAULT 'none',    -- IssuePriority
    assignee    TEXT,                            -- AgentId uuid string; NULL = unassigned
    -- Settle-once marker: unix ts of the FIRST …→Done edge (when accounting
    -- fired). NULL = never settled. A reopened-and-redone issue does NOT
    -- settle twice — one work item, one credit.
    settled_at  INTEGER,
    -- A5-F: non-empty only while status='blocked'; set exclusively via the
    -- BlockIssue command, cleared on every other transition.
    blocked_reason TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_issue_project_number ON issue(project_id, number);
CREATE INDEX IF NOT EXISTS idx_issue_project_stage ON issue(project_id, stage);
