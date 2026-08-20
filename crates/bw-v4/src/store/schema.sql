-- V4 本机库 schema —— 一次写全,不是增量 diff。
--
-- 只有四张表。别的一律不建:观测、指标、战绩、产物、定时、连接器、技能、
-- workflow 正文、队友、阶段舱、会话登记、知识索引、发版记录、周计划索引、
-- 活↔指标关联、群通知去重账本、技能包登记——19 张 V3 表在这里从未存在过,
-- 不需要 DROP TABLE。理由与「数据现在去哪」逐条见
-- docs/v4-prototype/design/02-data-and-files.md §2.1。
--
-- 三条原则:①仓是正本(人、agent、第二台机器都要看的东西进仓走 MR);
-- ②库只放本机过程数据与显示缓存(仓里已有的不复制进库);③没人取的不存。
-- 四张表全部可以删掉从仓 + 远端重建。
--
-- V4 不兼容老库:新库文件(默认 workbench-v4.db),开发期换 schema 直接删库
-- 重建,不写迁移、不加 add_column_if_missing——试点起才恢复双守卫纪律。

-- 项目定位 + 项目墙显示缓存。项目墙要在「不打开任何项目」时列出 N 个项目的
-- 名字与灯,不能每次启动扫 N 个仓——这是这张表存在的唯一理由。
-- 名片(想做什么/对标/北极星)、项目群、规范版本、在研版本一律不在这里:
-- 正本是 PROJECT.md 与 .bw/project.toml,打开项目时现读现解析。
CREATE TABLE IF NOT EXISTS project (
    id                 TEXT PRIMARY KEY,
    slug               TEXT NOT NULL UNIQUE,     -- 工作区目录名,深链 BW_OPEN 认它
    name               TEXT NOT NULL,            -- 显示缓存,正本在 PROJECT.md 标题
    workspace_path     TEXT NOT NULL DEFAULT '', -- 本机仓路径;空 = 未配工作区
    provider           TEXT NOT NULL DEFAULT '', -- 'github' | 'codehub';空 = 未挂远端
    remote_host        TEXT NOT NULL DEFAULT '',
    remote_path        TEXT NOT NULL DEFAULT '', -- "owner/repo";空 = 未挂远端
    -- 健康灯显示缓存。只能由 derive::derive_project_health 现算后回写
    -- (store 没有 set_signal 方法);没数据写 NULL,界面显示 Unknown 灰,
    -- 绝不假装绿。
    signal             TEXT,
    weekly_signal      TEXT,
    signal_derived_at  INTEGER,
    sort_order         REAL NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL
);

-- 远端 issue 的本机缓存(离线可看、启动快);没配远端的项目它是唯一落脚点。
-- 九个扩展列是缓存不是正本——正本是 docs/plan/YYYY-Www.md 里活清单那一行。
-- 写入顺序是「缓存先动、文件随后追上」;两边分歧时以文件为准。
-- 不出现 assignee 列:「选类别→工具→workflow」完全取代「指派队友」。
CREATE TABLE IF NOT EXISTS issue (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id),
    number          INTEGER NOT NULL,             -- 本机连续号,项目内唯一,未挂远端时活也有号可引用
    remote_number   INTEGER NOT NULL DEFAULT 0,   -- 远端 issue 号;0 = 未映射,绝不编造
    title           TEXT NOT NULL,
    body            TEXT NOT NULL DEFAULT '',
    status          TEXT NOT NULL,                -- IssueStatus,snake_case
    branch          TEXT NOT NULL DEFAULT '',
    pr_number       INTEGER NOT NULL DEFAULT 0,   -- 0 = 没有 MR,绝不编造
    -- ↓ 九个扩展列(正本在周计划文件)
    week_of         TEXT NOT NULL DEFAULT '',     -- ISO 周 "2026-W34";'' = 待办池
    version         TEXT NOT NULL DEFAULT '',     -- 在研版本标签 "v0.3";'' = 未挂版本
    tool            TEXT NOT NULL DEFAULT '',     -- 'claude_cli' | 'cursor' | 'open_design'
    kind            TEXT NOT NULL DEFAULT 'business', -- business | ops | light
    origin          TEXT NOT NULL DEFAULT 'human',    -- human | auto | agent_split | backfill
    workflow        TEXT NOT NULL DEFAULT '',     -- 实际用的 workflow / 技能名(供现算用量)
    category        TEXT NOT NULL DEFAULT '',     -- 活的类别标签(五阶段降级而来),映射到工具/workflow
    sort_order      REAL NOT NULL DEFAULT 0,      -- 看板同列内排序,浮点数支持插入排序
    metric_key      TEXT NOT NULL DEFAULT '',     -- 预期推动的指标 id(.bw/metrics.toml 里的 id)
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    -- 人显式点「完成」的那一刻。非空 = 已结清;同一件活绝不结两次
    -- (store 的 settle 写入按「已非空则短路」处理)。
    settled_at      INTEGER,
    UNIQUE (project_id, number)
);
CREATE INDEX IF NOT EXISTS idx_issue_project_week ON issue(project_id, week_of);
CREATE INDEX IF NOT EXISTS idx_issue_project_status ON issue(project_id, status);

-- 活 ↔ claude 会话 ↔ worktree ↔ 分支。恢复会话必需,纯本机纯过程数据。
CREATE TABLE IF NOT EXISTS claude_conversation (
    id                TEXT PRIMARY KEY,
    project_id        TEXT NOT NULL REFERENCES project(id),
    issue_id          TEXT NOT NULL UNIQUE REFERENCES issue(id),
    claude_session_id TEXT NOT NULL DEFAULT '',
    workspace_path    TEXT NOT NULL DEFAULT '',
    branch_name       TEXT NOT NULL DEFAULT '',
    created_at        INTEGER NOT NULL,
    last_opened_at    INTEGER NOT NULL
);

-- key/value:schema 版本、通知屏「事件流看到哪个时间点」(notify_seen:<project_id>)。
-- 不为这些开第五张表。
CREATE TABLE IF NOT EXISTS app_meta (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);
