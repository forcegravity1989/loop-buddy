-- next 切片七A / 七-1 修复轮 · 导入专用的两张表(design-s7-real-project.md
-- §2.4 第三处硬碰撞 / §2.6 / §6.1,task-s7a-review.md Critical-1)。这份
-- 文件只被 `bw-store-import` 这个 crate 自己的 `ImportSqliteStore::open`
-- 应用——`bw-store::SqliteStore::open`(壳唯一会调用的开库路径)完全不
-- 知道这份文件的存在,不是「特性关了就不知道」,是 `bw-store` 编译期就
-- 看不见它(结构约束,不是纪律;见 crate 文档「为什么这不算开后门」)。

-- 导入流水:只追加。每次真写(`--confirm`)记一行,不是防重复的机制本身
-- (防重复靠六类实体的身份编号原样沿用 + INSERT OR IGNORE),而是让第二
-- 次运行能说人话——"这份旧库在某月某日已经导过一次,本次新增 0 条",不
-- 是静悄悄什么都没发生(design §2.6)。
CREATE TABLE IF NOT EXISTS import_ledger (
    id                 TEXT PRIMARY KEY,
    legacy_db_path     TEXT NOT NULL,
    -- 旧库文件的指纹:大小+修改时刻的组合串,不是内容哈希——与 §2.7「打
    -- 印旧库文件的修改时刻与大小,验收时逐字节比对」用的是同一份证据,
    -- 这里只是把它也记进流水,方便回看"当时旧库是这个样子"。
    legacy_db_fingerprint TEXT NOT NULL,
    project_count      INTEGER NOT NULL,
    metric_count        INTEGER NOT NULL,
    observation_count   INTEGER NOT NULL,
    issue_count         INTEGER NOT NULL,
    handoff_count        INTEGER NOT NULL,
    artifact_count       INTEGER NOT NULL,
    created_at          INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_import_ledger_created ON import_ledger(created_at DESC);

-- 产物证据的历史存档(主控裁决 1,design §2.2 第⑥条 / §12 开放问题 1):
-- 明确标注"历史存档、不再增长"——与新工程读时现采的交付证据第三栏是两
-- 件事,不混用同一张表。228 条"哪件活登记的"归属关联(issue_id)在这里
-- 保全,仓库重采采不回来这条关联。这张表只有导入器一个写入方,导入之后
-- 再没有任何东西往里写——它的读者今天不存在(design §12 开放问题 1 如
-- 实记的代价),这是本片刻意接受的取舍,不是遗漏。
CREATE TABLE IF NOT EXISTS artifact_archive (
    id             TEXT PRIMARY KEY,          -- 旧库 artifact.id 原样沿用
    project_id     TEXT NOT NULL REFERENCES project(id),
    issue_id       TEXT,                      -- 旧库 artifact.issue_id;可空(不是每条产物都挂活)
    stage_index    INTEGER,                   -- 旧库 artifact.stage_kind 换算(1..5,可空)
    path           TEXT NOT NULL,
    kind           TEXT NOT NULL DEFAULT 'other',
    bytes          INTEGER NOT NULL DEFAULT 0,
    git_commit     TEXT NOT NULL DEFAULT '',
    registered_at  INTEGER NOT NULL,
    imported_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_artifact_archive_project ON artifact_archive(project_id, registered_at DESC);
