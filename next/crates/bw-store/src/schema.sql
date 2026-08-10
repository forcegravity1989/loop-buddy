-- next 切片四A · 最小 schema(design-s4-runmanager.md §2.2,原文照搬)。
-- 只有本片真有写入方的三张表——观测/交棒/风险/决策等事实表本片不建
-- (没有写入方的表就是假表,建了只会在切片五被重做一遍)。

-- 项目:本片只需要「有这么个项目、它的本地检出在哪」。
CREATE TABLE IF NOT EXISTS project (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    root_path   TEXT NOT NULL DEFAULT '',      -- 本地 git 检出根;空 = 未配置
    -- next 切片五B(design-s5-hexpanel.md §2.1):五阶段方法论的当前一棒。
    -- 1..5(对应 bw_core::StageKind::index()),NULL = 尚未开棒 —— 不默认
    -- 成 1,「还没开始」和「在原型阶段」是两件事,默认成 1 就是替用户拍板。
    -- 存量库加列走 sqlite.rs 的双守卫(add_column_if_missing)。
    active_stage INTEGER,
    created_at  INTEGER NOT NULL
);

-- 活:砍到只剩运行管理器真读真写的字段,再由切片五B 按真实需要加回两列。
-- 仍然砍掉的(远端编号、MR 号、优先级、指派、阻塞原因、标配技能)留给再
-- 往后的任务,同样按需用 add_column_if_missing 加回来。
CREATE TABLE IF NOT EXISTS issue (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    number      INTEGER NOT NULL,              -- 项目内序号 1,2,3…
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'backlog',
    settled_at  INTEGER,                       -- 首次 …→已完成 的时刻;NULL = 从未结算
    -- next 切片五B:这件活归哪个阶段。1..5,NULL = 未归类(同 active_stage
    -- 的「不默认」原则)。存量库加列走双守卫。
    stage       INTEGER,
    -- next 切片五B:活的描述正文——六段的「当前 Loop」要显示活在干什么;
    -- 也是切片四留下的那条缺口(执行规格里没有任务正文字段)在编排层这
    -- 一侧的真实来源。NOT NULL DEFAULT '':不是可空字段,没填就是空串,
    -- 不是「不知道」。存量库加列走双守卫。
    body        TEXT NOT NULL DEFAULT '',
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
--
-- ⚠️ 如实登记(评审 Important-2,plan/23 §10 第 10 条):`CREATE INDEX
-- IF NOT EXISTS` 对存量库的语义是「同名索引已存在就整条跳过,连谓词都
-- 不比对」——将来如果要改这条谓词(比如 kind 值集扩展),光改这一行
-- schema.sql 对已经开过库的老库**不会生效**,`sqlite_master.sql` 里留
-- 的还是旧定义,新库老库行为会分叉。这与本仓库「`CREATE TABLE IF NOT
-- EXISTS` 对存量表不加新列」是同一类坑,只是从列扩到了索引;现有的双
-- 守卫(`add_column_if_missing`,sqlite.rs)只覆盖列,不覆盖索引定义。
-- 本片(切片四A)只登记这个事实,不修机制——真要改这条谓词时,必须
-- 配一条索引迁移(比对 sqlite_master.sql 与期望定义,不一致就 DROP
-- INDEX 后重建),不能只改这一行。
CREATE UNIQUE INDEX IF NOT EXISTS uq_run_live_delivery_per_issue
    ON run(issue_id) WHERE ended_at IS NULL AND kind = 'delivery';

CREATE INDEX IF NOT EXISTS idx_run_project_started ON run(project_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_run_open ON run(issue_id) WHERE ended_at IS NULL;

-- ═══════════════ next 切片五B:指标 / 观测 / 交棒(design-s5-hexpanel.md §2) ═══════════════
-- 三张全新的表——CREATE TABLE 一步到位,不需要迁移双守卫(那是给"老表加
-- 列"的问题;新表本身没有"存量库缺列"这回事)。三张表都带唯一索引,都是
-- 新表新索引,不改任何既有索引的谓词,不触发 plan/23 §10 第 10 条登记的
-- 那条缺口(索引迁移机制缺失)。

-- 指标定义的本地缓存。正本永远是项目仓的 .bw/metrics.toml,BW 只读它、
-- 同步进来,绝不反向改写那份文件(改指标和改代码一样走评审)。
CREATE TABLE IF NOT EXISTS metric (
    id             TEXT PRIMARY KEY,
    project_id     TEXT NOT NULL REFERENCES project(id),
    -- 'north_star' | 'lagging' | 'leading'。三层同构,北极星不是特例——
    -- 这是 v1 后来自己修过的形态(f0a187a「北极星建 metric 行」),项目表
    -- 上不再有北极星文本列,旧路径不带进来(design §1.3)。
    tier           TEXT NOT NULL,
    name           TEXT NOT NULL,          -- 文件里没有 id,名字就是身份
    def            TEXT NOT NULL DEFAULT '',
    target_raw     TEXT NOT NULL DEFAULT '',  -- "≥5" "≤24h" "清零" …(内核解析)
    collect_kind   TEXT NOT NULL DEFAULT '',  -- 采集方案:github/connector/bw/script/manual
    collect_query  TEXT NOT NULL DEFAULT '',
    -- 'file'(从正本同步来)| 'manual'(界面手建)。同步器只碰 origin='file' 的行。
    origin         TEXT NOT NULL DEFAULT 'file',
    -- 停用:整条退出派生链,历史观测原样留着,可恢复。永不物理删除。
    archived_at    INTEGER,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);
-- 名字就是身份:同一个项目、同一层、同一个名字只能有一行。
CREATE UNIQUE INDEX IF NOT EXISTS uq_metric_identity ON metric(project_id, tier, name);
-- 一个项目至多一个北极星(部分唯一索引;和「一件活一个交付运行」同款手法:
-- 铁律钉进数据库,不是靠 if 判断)。
CREATE UNIQUE INDEX IF NOT EXISTS uq_metric_north_star
    ON metric(project_id) WHERE tier = 'north_star';

-- 观测:一个真实数据点。只追加,绝不修改、绝不插值,绝不为了点亮而伪造。
-- 存储层不给这张表任何更新或删除的方法——见 lib.rs 的 ObservationStore
-- trait,接口上就没有那两个方法,这不是纪律,是结构约束。
CREATE TABLE IF NOT EXISTS observation (
    id           TEXT PRIMARY KEY,
    metric_id    TEXT NOT NULL REFERENCES metric(id),
    project_id   TEXT NOT NULL REFERENCES project(id),  -- 冗余一列,按项目查不用连表
    ts           INTEGER NOT NULL,            -- 这个值是「什么时候测到的」
    raw_value    TEXT NOT NULL,               -- 原始显示串("60%" "5/7" "842ms")
    source       TEXT NOT NULL,               -- manual/script/github/codehub/connector/…
    source_hint  TEXT NOT NULL DEFAULT '',    -- 「这个数从哪来的」——证据链要能点开
    created_at   INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_observation_metric_ts ON observation(metric_id, ts DESC);

-- 阶段交棒的审计流水。清单没勾完也允许交,但会被标成带险并永久记下——
-- 只追加、不修改,事后抹不掉。字段与索引逐字移植 v1 同名表。
CREATE TABLE IF NOT EXISTS handoff (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    from_stage  TEXT NOT NULL,
    to_stage    TEXT NOT NULL,
    risky       INTEGER NOT NULL DEFAULT 0,
    note        TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_handoff_project_ts ON handoff(project_id, created_at DESC);
