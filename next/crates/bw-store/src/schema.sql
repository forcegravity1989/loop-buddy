-- next 切片四A · 最小 schema(design-s4-runmanager.md §2.2,原文照搬)。
-- 只有本片真有写入方的三张表——观测/交棒/风险/决策等事实表本片不建
-- (没有写入方的表就是假表,建了只会在切片五被重做一遍)。

-- 项目:本片只需要「有这么个项目、它的本地检出在哪」。
CREATE TABLE IF NOT EXISTS project (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    root_path   TEXT NOT NULL DEFAULT '',      -- 本地 git 检出根;空 = 未配置
    created_at  INTEGER NOT NULL
);

-- 活:砍到只剩运行管理器真读真写的字段。
-- 砍掉的(阶段、远端编号、MR 号、描述、优先级、指派、阻塞原因、标配技能)
-- 全部留给切片五,那时用 add_column_if_missing 加回来 —— 那正好是双守卫的
-- 第一次真实使用,而不是为了演示而演示。
CREATE TABLE IF NOT EXISTS issue (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    number      INTEGER NOT NULL,              -- 项目内序号 1,2,3…
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'backlog',
    settled_at  INTEGER,                       -- 首次 …→已完成 的时刻;NULL = 从未结算
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_issue_project_number ON issue(project_id, number);

-- 运行:本片的主角。
CREATE TABLE IF NOT EXISTS run (
    id                TEXT PRIMARY KEY,
    project_id        TEXT NOT NULL REFERENCES project(id),
    issue_id          TEXT NOT NULL REFERENCES issue(id),
    -- 'delivery' | 'consultation'。降级为咨询 = 这一列翻面(见设计稿 §3.5)。
    kind              TEXT NOT NULL DEFAULT 'delivery',
    connector_name    TEXT NOT NULL,           -- 哪条执行连接器跑的
    req_id            TEXT NOT NULL,           -- 连接器那次调用的请求编号(证据链要能对上)
    upstream_session  TEXT NOT NULL DEFAULT '',-- 上游会话号;空 = 这家不能指派/还没拿到
    workspace         TEXT NOT NULL,
    branch            TEXT NOT NULL,
    -- starting | running | finished | canceled | orphaned | failed
    state             TEXT NOT NULL,
    -- process_exit | stopped_by_bw | contact_lost | canceled | start_failed
    -- NULL = 如实不知道(重启遗留的运行就是 NULL,绝不填一个猜的)
    end_kind          TEXT,
    end_detail        TEXT NOT NULL DEFAULT '',-- 上游原话或诊断原文,不放 BW 编的结论
    started_at        INTEGER NOT NULL,
    ended_at          INTEGER,                 -- BW 不再把它当活的那一刻
    settled_at        INTEGER,                 -- 这次运行的账结过了(至多一次)
    demoted_at        INTEGER                  -- 降级为咨询的时刻
);

-- 铁律进存储:一件活至多一个「还活着的交付运行」。第二个插不进去,
-- 不是被 if 拦住,是数据库根本不收。降级为咨询(kind 翻面)与结束
-- (ended_at 落值)都会让这一行退出索引谓词,名额自然释放。
CREATE UNIQUE INDEX IF NOT EXISTS uq_run_live_delivery_per_issue
    ON run(issue_id) WHERE ended_at IS NULL AND kind = 'delivery';

CREATE INDEX IF NOT EXISTS idx_run_project_started ON run(project_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_run_open ON run(issue_id) WHERE ended_at IS NULL;
