//! `import_legacy` —— next 切片七B 一次性导入器(design-s7-real-project.md
//! §2)。
//!
//! **它是一个命令行程序,不进壳**。界面上没有「导入」按钮,永远没有。
//! **默认只看不写**:不给 `--confirm`,只读旧库、打印计数表 + 差异清单 +
//! 拒绝清单,一个字节不写新库。**不是同步器**:跑完之后旧库与新库各过
//! 各的日子,没有任何机制会再让它们对账。
//!
//! 同址同形(主控裁决 3):与仓里既有指挥器(`run_races`/`hex_readback`/
//! `store_guards`)住在同一个 examples 目录,不新开 crate、不进壳。**两种
//! 跑法**:
//!
//! ```text
//! cargo run -p bw-app --example import_legacy
//!     # 不给 --from/--to:合成老库自测档(照旧 schema 原文建六张表、放已
//!     # 知数据、导入、读回逐实体计数断言、二次导入幂等断言、旧库哈希不
//!     # 变断言)——这是 CI 常绿门禁真正跑的那一档。末行 IMPORT_LEGACY_OK。
//!
//! cargo run -p bw-app --example import_legacy -- --from <旧库> --to <新库>
//!     # 默认:只读旧库,打印计数表 + 差异清单,一个字节不写。末行
//!     # IMPORT_DRYRUN_OK。
//!
//! cargo run -p bw-app --example import_legacy -- --from <旧库> --to <新库> --confirm
//!     # 真写:先把同一份计数表再打一遍,末行 IMPORT_WROTE <各实体条数>
//!     # (拒绝清单非空时改打 IMPORT_REJECTED_NO_WRITE,一个字节不写)。
//! ```
//!
//! **真实旧库那次运行不在本任务**(design §2.7:先拷一份旧库副本全流程
//! 验过,再对真库原件只跑只看档核对计数——那是切片七E/`plan/23` §10
//! 第 22 条待办登记的事,配人工确认门)。
//!
//! **七类实体的新旧形状对照、三处硬碰撞的处置、幂等语义**全部照 design
//! §2.2/§2.4/§2.6 实现,逐条注记见下方各函数文档。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bw_core::{
    ArtifactId, HandoffId, IssueId, IssueStatus, MetricId, ObservationId, ProjectId, StageKind,
};
use bw_store::{MetricTier, SqliteStore};
use bw_store_import::{
    ImportArtifactRow, ImportHandoffRow, ImportIssueRow, ImportLedgerEntry, ImportMetricRow,
    ImportObservationRow, ImportProjectRow, ImportSqliteStore, ImportStore,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;
use uuid::Uuid;

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

const DB_NAME_PREFIX: &str = "bw-import-legacy-";

fn fresh_db_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{DB_NAME_PREFIX}{tag}-{}.db", Uuid::new_v4()))
}

/// 开跑前清场——同 `run_races.rs`/`store_guards.rs` 的既有先例:PID 会被
/// 系统复用,uuid 不会;清掉临时目录里所有更早的同前缀库,任何一次跑读
/// 到的库要么是它自己刚建的,要么根本不存在。
fn clear_stale_dbs() {
    let dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(DB_NAME_PREFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

// ─────────────────────────── Ledger(同 run_races.rs 既有先例)───────────

struct Ledger {
    ok: bool,
    count: usize,
}

impl Ledger {
    fn new() -> Self {
        Self { ok: true, count: 0 }
    }
    fn check(&mut self, cond: bool, label: impl std::fmt::Display) {
        self.count += 1;
        if cond {
            println!("  ✓ {label}");
        } else {
            eprintln!("ASSERT FAILED: {label}");
            self.ok = false;
        }
    }
    fn fail(&mut self, label: impl std::fmt::Display) {
        self.check(false, label);
    }
}

// ─────────────────────────── 参数 ───────────────────────────

enum Mode {
    SelfTest,
    Real {
        from: PathBuf,
        to: PathBuf,
        confirm: bool,
    },
}

fn parse_args() -> Result<Mode, String> {
    let mut from: Option<PathBuf> = None;
    let mut to: Option<PathBuf> = None;
    let mut confirm = false;
    let mut argv = std::env::args().skip(1);
    while let Some(a) = argv.next() {
        match a.as_str() {
            "--from" => from = Some(PathBuf::from(argv.next().ok_or("--from 需要一个路径参数")?)),
            "--to" => to = Some(PathBuf::from(argv.next().ok_or("--to 需要一个路径参数")?)),
            "--confirm" => confirm = true,
            other => return Err(format!("未知参数:{other:?}")),
        }
    }
    match (from, to) {
        (None, None) => Ok(Mode::SelfTest),
        (Some(from), Some(to)) => Ok(Mode::Real { from, to, confirm }),
        _ => Err("--from 和 --to 必须同时给出(或者都不给,走合成老库自测档)".to_string()),
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let mode = match parse_args() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("参数错误:{e}");
            return ExitCode::FAILURE;
        }
    };
    match mode {
        Mode::SelfTest => run_self_test().await,
        Mode::Real { from, to, confirm } => run_real(&from, &to, confirm).await,
    }
}

// ─────────────────────────── 旧库读法(照旧 schema 只读)───────────────

/// 旧库只读打开,不带自动建库(design §2.7)——路径打错时应当直接报「打
/// 不开」,绝不悄悄建一个空库然后报告「导入 0 条」。
async fn open_legacy_readonly(path: &Path) -> Result<SqlitePool, String> {
    if !path.exists() {
        return Err(format!("旧库文件不存在:{}", path.display()));
    }
    let opts = SqliteConnectOptions::new().filename(path).read_only(true);
    SqlitePool::connect_with(opts)
        .await
        .map_err(|e| format!("旧库打不开(只读连接失败):{e}"))
}

/// 旧库文件的「指纹」——大小 + 修改时刻的组合串,不是内容哈希(design
/// §2.7:「打印旧库文件的修改时刻与大小,验收时逐字节比对」用的就是这两
/// 个值本身,不是重新算一遍加密哈希)。跑前跑后这个串相同就是「只读」的
/// 硬证明——比「代码里写了 read_only(true)」更硬,本仓库真的踩过一个本
/// 意是校验的工具因为参数位置写错先把库删了再报成功的教训。
fn fingerprint(path: &Path) -> Result<String, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("读旧库文件元信息失败:{e}"))?;
    let mtime = meta
        .modified()
        .map_err(|e| format!("读旧库文件修改时刻失败:{e}"))?;
    // 纳秒精度,不是整秒——2026-08-11 突变自证(临时去掉 read_only(true)、
    // 故意对旧库写一次)实测抓到:同一次运行里 dry-run 前后往往落在同一
    // 个墙钟秒内,整秒精度的 mtime 会让这次真实发生的篡改被误判成"没变"
    // (assert 仍然是绿的,但那是假绿——它没有真的检测到篡改,只是精度不
    // 够)。纳秒精度下,文件系统(本机 APFS)真实记录了这次写,断言能如
    // 实翻红。
    let mtime_nanos = mtime
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Ok(format!("size={};mtime_ns={}", meta.len(), mtime_nanos))
}

// ─────────────────────────── 阶段文字 ↔ 数字 ───────────────────────────

/// 旧库 `active_stage`/`stage_kind`/`from_stage`/`to_stage` 的文字编码
/// (design 工程对照表:旧库 `stage_kind_text` ↔ `bw_core::StageKind`)。
fn parse_stage_text(s: &str) -> Option<StageKind> {
    match s {
        "prototype" => Some(StageKind::Prototype),
        "build" => Some(StageKind::Build),
        "optimize" => Some(StageKind::Optimize),
        "growth" => Some(StageKind::Growth),
        "ops" => Some(StageKind::Ops),
        _ => None,
    }
}

fn parse_role_to_tier(role: &str) -> Option<MetricTier> {
    match role {
        "leading" => Some(MetricTier::Leading),
        "lagging" => Some(MetricTier::Lagging),
        _ => None,
    }
}

// ─────────────────────────── 旧库原始行(照 v1 schema.sql 逐列读)───────

struct OldProject {
    id: String,
    name: String,
    workspace_path: String,
    active_stage: String,
    north_star: String,
    ns_def: String,
    north_star_collect_kind: String,
    north_star_collect_query: String,
    created_at: i64,
}

struct OldMetric {
    id: String,
    project_id: String,
    role: String,
    stage_kind: Option<String>,
    name: String,
    def: String,
    target_raw: String,
    collect_kind: String,
    collect_query: String,
    origin: String,
    /// 旧库 v1 schema.sql:`archived INTEGER NOT NULL DEFAULT 0` ——独立于
    /// `archived_at` 的一个布尔标记(Important-2:两列各自维护,旧库可能
    /// 出现 `archived=1` 但 `archived_at IS NULL` 这种"标了停用没记时刻"的
    /// 数据形态)。
    archived: i64,
    archived_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

struct OldObservation {
    id: String,
    metric_id: String,
    ts: i64,
    source_kind: String,
    raw: String,
    source_run_id: Option<String>,
    created_at: i64,
}

struct OldIssue {
    id: String,
    project_id: String,
    stage: String,
    number: i64,
    title: String,
    descr: String,
    status: String,
    settled_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

struct OldHandoff {
    id: String,
    project_id: String,
    from_stage: String,
    to_stage: String,
    risky: i64,
    note: String,
    created_at: i64,
}

struct OldArtifact {
    id: String,
    project_id: String,
    issue_id: Option<String>,
    stage_kind: Option<String>,
    path: String,
    kind: String,
    bytes: i64,
    git_commit: String,
    registered_at: i64,
}

async fn read_old_projects(pool: &SqlitePool) -> Result<Vec<OldProject>, String> {
    let rows = sqlx::query(
        "SELECT id, name, workspace_path, active_stage, north_star, ns_def, \
         north_star_collect_kind, north_star_collect_query, created_at FROM project",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读旧库 project 失败:{e}"))?;
    Ok(rows
        .iter()
        .map(|r| OldProject {
            id: r.get("id"),
            name: r.get("name"),
            workspace_path: r.get("workspace_path"),
            active_stage: r.get("active_stage"),
            north_star: r.get("north_star"),
            ns_def: r.get("ns_def"),
            north_star_collect_kind: r.get("north_star_collect_kind"),
            north_star_collect_query: r.get("north_star_collect_query"),
            created_at: r.get("created_at"),
        })
        .collect())
}

async fn read_old_metrics(pool: &SqlitePool) -> Result<Vec<OldMetric>, String> {
    let rows = sqlx::query(
        "SELECT id, project_id, role, stage_kind, name, def, target_raw, collect_kind, \
         collect_query, origin, archived, archived_at, created_at, updated_at FROM metric",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读旧库 metric 失败:{e}"))?;
    Ok(rows
        .iter()
        .map(|r| OldMetric {
            id: r.get("id"),
            project_id: r.get("project_id"),
            role: r.get("role"),
            stage_kind: r.get("stage_kind"),
            name: r.get("name"),
            def: r.get("def"),
            target_raw: r.get("target_raw"),
            collect_kind: r.get("collect_kind"),
            collect_query: r.get("collect_query"),
            origin: r.get("origin"),
            archived: r.get("archived"),
            archived_at: r.get("archived_at"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

async fn read_old_observations(pool: &SqlitePool) -> Result<Vec<OldObservation>, String> {
    let rows = sqlx::query(
        "SELECT id, metric_id, ts, source_kind, raw, source_run_id, created_at FROM observation",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读旧库 observation 失败:{e}"))?;
    Ok(rows
        .iter()
        .map(|r| OldObservation {
            id: r.get("id"),
            metric_id: r.get("metric_id"),
            ts: r.get("ts"),
            source_kind: r.get("source_kind"),
            raw: r.get("raw"),
            source_run_id: r.get("source_run_id"),
            created_at: r.get("created_at"),
        })
        .collect())
}

async fn read_old_issues(pool: &SqlitePool) -> Result<Vec<OldIssue>, String> {
    let rows = sqlx::query(
        "SELECT id, project_id, stage, number, title, descr, status, settled_at, \
         created_at, updated_at FROM issue",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读旧库 issue 失败:{e}"))?;
    Ok(rows
        .iter()
        .map(|r| OldIssue {
            id: r.get("id"),
            project_id: r.get("project_id"),
            stage: r.get("stage"),
            number: r.get("number"),
            title: r.get("title"),
            descr: r.get("descr"),
            status: r.get("status"),
            settled_at: r.get("settled_at"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

async fn read_old_handoffs(pool: &SqlitePool) -> Result<Vec<OldHandoff>, String> {
    let rows = sqlx::query(
        "SELECT id, project_id, from_stage, to_stage, risky, note, created_at FROM handoff",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读旧库 handoff 失败:{e}"))?;
    Ok(rows
        .iter()
        .map(|r| OldHandoff {
            id: r.get("id"),
            project_id: r.get("project_id"),
            from_stage: r.get("from_stage"),
            to_stage: r.get("to_stage"),
            risky: r.get("risky"),
            note: r.get("note"),
            created_at: r.get("created_at"),
        })
        .collect())
}

async fn read_old_artifacts(pool: &SqlitePool) -> Result<Vec<OldArtifact>, String> {
    let rows = sqlx::query(
        "SELECT id, project_id, issue_id, stage_kind, path, kind, bytes, git_commit, \
         registered_at FROM artifact",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读旧库 artifact 失败:{e}"))?;
    Ok(rows
        .iter()
        .map(|r| OldArtifact {
            id: r.get("id"),
            project_id: r.get("project_id"),
            issue_id: r.get("issue_id"),
            stage_kind: r.get("stage_kind"),
            path: r.get("path"),
            kind: r.get("kind"),
            bytes: r.get("bytes"),
            git_commit: r.get("git_commit"),
            registered_at: r.get("registered_at"),
        })
        .collect())
}

// ─────────────────────────── 计划(analyze)───────────────────────────

#[derive(Default)]
struct RawCounts {
    project: i64,
    north_star: i64,
    metric: i64,
    observation: i64,
    issue: i64,
    artifact: i64,
    handoff: i64,
}

impl RawCounts {
    fn print(&self) {
        println!("== 计数表(七类实体各读到多少条,design §2.5 第 1 段)==");
        println!("  项目          {}", self.project);
        println!("  目标(北极星) {}", self.north_star);
        println!("  指标          {}", self.metric);
        println!("  观测          {}", self.observation);
        println!("  活            {}", self.issue);
        println!("  产物证据      {}", self.artifact);
        println!("  交棒          {}", self.handoff);
    }
}

/// 北极星来源判定的依据(Important-3:裁决 6「读正本比名字」的判定依据打
/// 进导入报告)——`basis` 是同一段人话原样打印进差异清单的那句话,不是
/// 事后另算一遍摘要;指挥器对报告输出的断言查的就是这个字段。
struct NorthStarOriginNote {
    project_name: String,
    origin: String,
    basis: String,
}

struct ImportPlan {
    projects: Vec<ImportProjectRow>,
    metrics: Vec<ImportMetricRow>,
    observations: Vec<ImportObservationRow>,
    issues: Vec<ImportIssueRow>,
    handoffs: Vec<ImportHandoffRow>,
    artifacts: Vec<ImportArtifactRow>,
    /// (项目名, 旧名字, 新名字)——§2.4 第一处硬碰撞的改名日志,每一行都
    /// 要逐行打印进差异清单(design §2.4:「这是导入器造了一个旧库里不存
    /// 在的名字,人必须看见」)。
    renamed: Vec<(String, String, String)>,
    /// 北极星来源判定依据,一个定义了北极星的项目一条(Important-3)。
    north_star_notes: Vec<NorthStarOriginNote>,
    /// 人话原因,§2.5 第 3 段「拒绝清单」。非空 = 整次导入一条都不写。
    rejected: Vec<String>,
    raw_counts: RawCounts,
}

/// 项目那一列固定的「未导入列清单」(design §2.2①)——静态的 schema 事
/// 实,不依赖旧库这一次具体读到了什么数据。
const PROJECT_UNTRANSFERRED: &str = "kind、descr、phase、cycle、benchmark、opportunity、\
allow_commands、remote_path/remote_host/provider(远端身份三件套——新工程里\"这个项目对应\
远端哪个仓\"住在连接器绑定,连接器今天不落库,这三列确实无处可去)、三个信号缓存列、\
updated_at、rev";

const METRIC_UNTRANSFERRED: &str = "stage_kind(阶段归属,见第一处硬碰撞的处置)、\
amber_kind/amber_value(黄灯带宽——真库里全是默认值,丢的是能力不是数据)、\
last_target、driver、pos(排序)、两个信号缓存列、rev";

const ISSUE_UNTRANSFERRED: &str = "github_number(远端编号)、pr_number(合并请求号)、\
priority、assignee(指派给哪个队友)、blocked_reason、standard_skill(标配技能)、\
interactive_started、claude_session_id(上游会话号——新工程记在运行行上,运行不在导入\
范围内,这些旧会话在新库里接不回来)";

/// 核心分析函数:读旧库、算出「新旧形状对照 + 三处硬碰撞的处置 + 幂等」
/// 的完整计划,不碰新库一个字节。dry-run 与 confirm 两条路径共用同一份
/// 结果——「先把同一份计数表再打一遍」(design §2.5)靠的就是同一个函数
/// 被调用两次,不是两份平行发明。
async fn analyze(pool: &SqlitePool) -> Result<ImportPlan, String> {
    let old_projects = read_old_projects(pool).await?;
    let old_metrics = read_old_metrics(pool).await?;
    let old_observations = read_old_observations(pool).await?;
    let old_issues = read_old_issues(pool).await?;
    let old_handoffs = read_old_handoffs(pool).await?;
    let old_artifacts = read_old_artifacts(pool).await?;

    let mut raw_counts = RawCounts {
        project: old_projects.len() as i64,
        north_star: old_projects
            .iter()
            .filter(|p| !p.north_star.is_empty())
            .count() as i64,
        metric: old_metrics.len() as i64,
        observation: old_observations.len() as i64,
        issue: old_issues.len() as i64,
        artifact: old_artifacts.len() as i64,
        handoff: old_handoffs.len() as i64,
    };
    // 上面这份 struct-literal 初始化已经把全部字段填满,这里只是给
    // `raw_counts` 一个可变绑定的名字(clippy 顺手识别不出来上面是唯一赋
    // 值,留一手不报"从未 mut"的死告警)。
    let _ = &mut raw_counts;

    let mut rejected: Vec<String> = Vec::new();

    // ── ① 项目(design §2.2①)──────────────────────────────────────
    let mut project_name_of: HashMap<String, String> = HashMap::new();
    let mut project_id_of: HashMap<String, ProjectId> = HashMap::new();
    let mut projects: Vec<ImportProjectRow> = Vec::new();
    for p in &old_projects {
        let uuid = match Uuid::parse_str(&p.id) {
            Ok(u) => u,
            Err(e) => {
                rejected.push(format!("项目 {}({}):id 不是合法 uuid:{e}", p.name, p.id));
                continue;
            }
        };
        let pid = ProjectId::from_uuid(uuid);
        let active_stage = parse_stage_text(&p.active_stage);
        if active_stage.is_none() {
            // design §2.2①:旧库这一列不可空,理论上总能解析;解析不出来
            // 是旧库数据本身坏了,如实拒绝而不是悄悄丢弃这个项目。
            rejected.push(format!(
                "项目 {}({}):active_stage 无法解析:{:?}",
                p.name, p.id, p.active_stage
            ));
            continue;
        }
        project_name_of.insert(p.id.clone(), p.name.clone());
        project_id_of.insert(p.id.clone(), pid);
        projects.push(ImportProjectRow {
            id: pid,
            name: p.name.clone(),
            root_path: p.workspace_path.clone(),
            active_stage,
            created_at: p.created_at,
        });
    }

    // ── ② 目标/北极星(design §2.2②)────────────────────────────────
    // 旧库把北极星做成项目表上的四个文本列;新库把它做成指标表里
    // tier=north_star 的一行。判定规则:读项目仓正本文件(`.bw/
    // metrics.toml`)比北极星一节的名字,逐字相同才标「来自正本」,否则
    // 「手建」——这条规则的理由见 design §2.2②(同步器只碰来自正本的
    // 行,标错会在下一次同步正本时撞唯一索引或被误停用)。
    let mut north_star_metrics: Vec<ImportMetricRow> = Vec::new();
    let mut north_star_notes: Vec<NorthStarOriginNote> = Vec::new();
    for p in &old_projects {
        if p.north_star.trim().is_empty() {
            continue; // 这个项目从未定过北极星,新库里也不该凭空长出一行
        }
        let Some(&pid) = project_id_of.get(&p.id) else {
            continue; // 项目本身已经因为 id/active_stage 问题被拒绝
        };
        let judged = determine_north_star_origin(&p.workspace_path, &p.north_star);
        north_star_notes.push(NorthStarOriginNote {
            project_name: p.name.clone(),
            origin: judged.origin.clone(),
            basis: judged.basis.clone(),
        });
        north_star_metrics.push(ImportMetricRow {
            id: MetricId::new(),
            project_id: pid,
            tier: MetricTier::NorthStar,
            name: p.north_star.clone(),
            def: p.ns_def.clone(),
            target_raw: String::new(), // design §2.2②:旧库没有北极星目标值,留空=未知,不点灯
            collect_kind: p.north_star_collect_kind.clone(),
            collect_query: p.north_star_collect_query.clone(),
            origin: judged.origin,
            archived_at: None,
            created_at: p.created_at,
            updated_at: p.created_at,
        });
    }

    // ── ③ 指标(design §2.2③ / §2.4 第一处硬碰撞)──────────────────
    // 先按 (project_id, tier, name) 分组;同组 > 1 行时,手建
    // (origin='manual')的行一律改名(接阶段名后缀),来自正本
    // (origin='file')的行一律不许改名——真撞上正本行同名,那才是真正
    // 拒不了的碰撞。
    let mut effective_name: Vec<String> = old_metrics.iter().map(|m| m.name.clone()).collect();
    {
        let mut groups: HashMap<(String, String, String), Vec<usize>> = HashMap::new();
        for (i, m) in old_metrics.iter().enumerate() {
            let tier_key = m.role.clone();
            groups
                .entry((m.project_id.clone(), tier_key, m.name.clone()))
                .or_default()
                .push(i);
        }
        for idxs in groups.values() {
            if idxs.len() <= 1 {
                continue;
            }
            for &i in idxs {
                let m = &old_metrics[i];
                if m.origin == "manual" {
                    let stage_label = m
                        .stage_kind
                        .as_deref()
                        .and_then(parse_stage_text)
                        .map(StageKind::label)
                        .unwrap_or("未归类阶段");
                    let new_name = format!("{} · {stage_label}", m.name);
                    let project_name = project_name_of
                        .get(&m.project_id)
                        .cloned()
                        .unwrap_or_else(|| m.project_id.clone());
                    println!(
                        "  【改名】{project_name}:「{}」→「{new_name}」(阶段维度的重名指标,§2.4 第一处硬碰撞)",
                        m.name
                    );
                    effective_name[i] = new_name;
                }
            }
        }
        // 改名之后再分组一次——理论上只有"来自正本的行互相撞名"这一种情
        // 形还会剩下碰撞(真库/合成库都没有这一种,防御式保留)。
        let mut groups2: HashMap<(String, String, String), Vec<usize>> = HashMap::new();
        for (i, m) in old_metrics.iter().enumerate() {
            groups2
                .entry((
                    m.project_id.clone(),
                    m.role.clone(),
                    effective_name[i].clone(),
                ))
                .or_default()
                .push(i);
        }
        for idxs in groups2.values() {
            if idxs.len() > 1 {
                for &i in idxs {
                    let m = &old_metrics[i];
                    rejected.push(format!(
                        "指标「{}」({}) 改名后仍与同项目同层级的另一行撞名——来自正本的行不许改名,\
                         需要去改正本文件,而不是靠导入器悄悄处理",
                        m.name, m.id
                    ));
                }
            }
        }
    }

    let mut renamed: Vec<(String, String, String)> = Vec::new();
    let mut accepted_metric_ids: HashSet<String> = HashSet::new();
    let mut metrics: Vec<ImportMetricRow> = Vec::new();
    let mut metric_project_of: HashMap<String, ProjectId> = HashMap::new();
    for (i, m) in old_metrics.iter().enumerate() {
        let Some(&pid) = project_id_of.get(&m.project_id) else {
            continue; // 挂在一个已经被拒绝的项目下面
        };
        let Ok(uuid) = Uuid::parse_str(&m.id) else {
            rejected.push(format!("指标 {}:id 不是合法 uuid", m.id));
            continue;
        };
        let Some(tier) = parse_role_to_tier(&m.role) else {
            rejected.push(format!(
                "指标 {}({}):role={:?} 无法识别",
                m.name, m.id, m.role
            ));
            continue;
        };
        // 这一行是不是最终被判定"改名后仍撞名"而拒绝的行——上一步已经把
        // 它记进 rejected,这里跳过写入。
        let still_conflicts = {
            let mut groups2: HashMap<(String, String, String), usize> = HashMap::new();
            for (j, mm) in old_metrics.iter().enumerate() {
                *groups2
                    .entry((
                        mm.project_id.clone(),
                        mm.role.clone(),
                        effective_name[j].clone(),
                    ))
                    .or_insert(0) += 1;
                let _ = j;
            }
            groups2
                .get(&(
                    m.project_id.clone(),
                    m.role.clone(),
                    effective_name[i].clone(),
                ))
                .copied()
                .unwrap_or(0)
                > 1
        };
        if still_conflicts {
            continue;
        }
        metric_project_of.insert(m.id.clone(), pid);
        accepted_metric_ids.insert(m.id.clone());
        if effective_name[i] != m.name {
            renamed.push((
                project_name_of
                    .get(&m.project_id)
                    .cloned()
                    .unwrap_or_default(),
                m.name.clone(),
                effective_name[i].clone(),
            ));
        }
        // Important-2 兜底分支(design §2.2③):旧库 `archived`(布尔)与
        // `archived_at`(时刻)是两个独立列,可能出现"标了停用、没记时
        // 刻"这种数据形态——新库 schema 只有 `archived_at`(有值=停用,
        // `NULL`=在用),原样透传 `m.archived_at` 会把这种行悄悄读成"在
        // 用",派生链重新把它捡回来。`archived != 0` 时若 `archived_at`
        // 为空,取 `updated_at` 顶上,并逐行打印"这一行的停用时刻是估
        // 的",不静默复活。
        let resolved_archived_at = if m.archived != 0 {
            match m.archived_at {
                Some(ts) => Some(ts),
                None => {
                    let project_name = project_name_of
                        .get(&m.project_id)
                        .cloned()
                        .unwrap_or_else(|| m.project_id.clone());
                    println!(
                        "  【停用时刻是估的】{project_name}:指标「{}」({}) archived=1 但 \
                         archived_at 为空,取 updated_at({}) 顶上(design §2.2③兜底分支,\
                         Important-2)",
                        effective_name[i], m.id, m.updated_at
                    );
                    Some(m.updated_at)
                }
            }
        } else {
            // archived=0(在用):不管 archived_at 是否意外留有旧值,新库
            // 只认 archived_at 是否为空这一条,原样搬会把"在用"的行错读成
            // "停用"——同一个兜底分支,反方向同样不该发生。
            None
        };
        metrics.push(ImportMetricRow {
            id: MetricId::from_uuid(uuid),
            project_id: pid,
            tier,
            name: effective_name[i].clone(),
            def: m.def.clone(),
            target_raw: m.target_raw.clone(),
            collect_kind: m.collect_kind.clone(),
            collect_query: m.collect_query.clone(),
            origin: m.origin.clone(),
            archived_at: resolved_archived_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        });
    }
    metrics.extend(north_star_metrics);

    // ── ④ 观测(design §2.2④)──────────────────────────────────────
    let mut observations: Vec<ImportObservationRow> = Vec::new();
    for o in &old_observations {
        let Some(&pid) = metric_project_of.get(&o.metric_id) else {
            // 挂在一个不存在/被拒绝的指标下面——不单独拒绝(它不是三处硬
            // 碰撞之一,是上游指标问题的自然连带),只如实跳过并留痕。
            println!(
                "  【跳过】观测 {}:挂靠的指标 {} 不在这次导入计划里(指标本身已被拒绝或缺失)",
                o.id, o.metric_id
            );
            continue;
        };
        let Ok(oid) = Uuid::parse_str(&o.id) else {
            rejected.push(format!("观测 {}:id 不是合法 uuid", o.id));
            continue;
        };
        let Ok(mid) = Uuid::parse_str(&o.metric_id) else {
            rejected.push(format!("观测 {}:metric_id 不是合法 uuid", o.metric_id));
            continue;
        };
        let source_hint = match &o.source_run_id {
            Some(run_id) if !run_id.is_empty() => format!("旧运行编号:{run_id}"),
            _ => String::new(),
        };
        observations.push(ImportObservationRow {
            id: ObservationId::from_uuid(oid),
            metric_id: MetricId::from_uuid(mid),
            project_id: pid,
            ts: o.ts,
            raw_value: o.raw.clone(),
            source: o.source_kind.clone(),
            source_hint,
            created_at: o.created_at,
        });
    }

    // ── ⑤ 活(design §2.2⑤)────────────────────────────────────────
    // 拒绝判据(第二处硬碰撞的姊妹判据,design §2.4 上方 / §2.5 拒绝
    // 段):同一项目内活编号重复——新库的唯一索引钉死这条,撞了整条如实
    // 拒绝并打印冲突清单,不自动改号。
    let mut number_groups: HashMap<(String, i64), Vec<usize>> = HashMap::new();
    for (i, iss) in old_issues.iter().enumerate() {
        number_groups
            .entry((iss.project_id.clone(), iss.number))
            .or_default()
            .push(i);
    }
    let mut duplicate_issue_idx: HashSet<usize> = HashSet::new();
    for ((project_id, number), idxs) in &number_groups {
        if idxs.len() > 1 {
            let ids: Vec<&str> = idxs.iter().map(|&i| old_issues[i].id.as_str()).collect();
            let project_name = project_name_of.get(project_id).cloned().unwrap_or_default();
            rejected.push(format!(
                "项目「{project_name}」内活编号 #{number} 重复:{} 条旧记录({}) —— 新库的\
                 (project_id, number) 唯一索引钉死这条,整条如实拒绝,不自动改号",
                idxs.len(),
                ids.join(", ")
            ));
            for &i in idxs {
                duplicate_issue_idx.insert(i);
            }
        }
    }

    let mut issues: Vec<ImportIssueRow> = Vec::new();
    for (i, iss) in old_issues.iter().enumerate() {
        if duplicate_issue_idx.contains(&i) {
            continue;
        }
        let Some(&pid) = project_id_of.get(&iss.project_id) else {
            continue;
        };
        let Ok(uuid) = Uuid::parse_str(&iss.id) else {
            rejected.push(format!("活 {}:id 不是合法 uuid", iss.id));
            continue;
        };
        let status = match parse_issue_status(&iss.status) {
            Some(s) => s,
            None => {
                rejected.push(format!(
                    "活 {}(#{}):status={:?} 无法识别",
                    iss.id, iss.number, iss.status
                ));
                continue;
            }
        };
        let stage = parse_stage_text(&iss.stage);
        if stage.is_none() {
            rejected.push(format!(
                "活 {}(#{}):stage 无法解析:{:?}",
                iss.id, iss.number, iss.stage
            ));
            continue;
        }
        issues.push(ImportIssueRow {
            id: IssueId::from_uuid(uuid),
            project_id: pid,
            number: iss.number,
            title: iss.title.clone(),
            status,
            settled_at: iss.settled_at,
            stage,
            body: iss.descr.clone(),
            created_at: iss.created_at,
            updated_at: iss.updated_at,
        });
    }

    // ── ⑥ 产物证据(design §2.2⑥,主控裁决 1:导成历史存档表)──────
    let mut artifacts: Vec<ImportArtifactRow> = Vec::new();
    for a in &old_artifacts {
        let Some(&pid) = project_id_of.get(&a.project_id) else {
            continue;
        };
        let Ok(uuid) = Uuid::parse_str(&a.id) else {
            rejected.push(format!("产物登记 {}:id 不是合法 uuid", a.id));
            continue;
        };
        let issue_id = match &a.issue_id {
            Some(s) if !s.is_empty() => match Uuid::parse_str(s) {
                Ok(u) => Some(IssueId::from_uuid(u)),
                Err(_) => {
                    println!(
                        "  【留痕】产物登记 {} 的 issue_id={s:?} 不是合法 uuid,按未挂活处理",
                        a.id
                    );
                    None
                }
            },
            _ => None,
        };
        let stage = a.stage_kind.as_deref().and_then(parse_stage_text);
        artifacts.push(ImportArtifactRow {
            id: ArtifactId::from_uuid(uuid),
            project_id: pid,
            issue_id,
            stage,
            path: a.path.clone(),
            kind: a.kind.clone(),
            bytes: a.bytes,
            git_commit: a.git_commit.clone(),
            registered_at: a.registered_at,
        });
    }

    // ── ⑦ 交棒(design §2.2⑦)──────────────────────────────────────
    let mut handoffs: Vec<ImportHandoffRow> = Vec::new();
    for h in &old_handoffs {
        let Some(&pid) = project_id_of.get(&h.project_id) else {
            continue;
        };
        let Ok(uuid) = Uuid::parse_str(&h.id) else {
            rejected.push(format!("交棒 {}:id 不是合法 uuid", h.id));
            continue;
        };
        let (Some(from_stage), Some(to_stage)) = (
            parse_stage_text(&h.from_stage),
            parse_stage_text(&h.to_stage),
        ) else {
            rejected.push(format!(
                "交棒 {}:from_stage/to_stage 无法解析:{:?}/{:?}",
                h.id, h.from_stage, h.to_stage
            ));
            continue;
        };
        handoffs.push(ImportHandoffRow {
            id: HandoffId::from_uuid(uuid),
            project_id: pid,
            from_stage,
            to_stage,
            risky: h.risky != 0,
            note: h.note.clone(),
            created_at: h.created_at,
        });
    }

    let _ = accepted_metric_ids; // 保留供未来扩展观测跳过逻辑复用,当前已经靠 metric_project_of 间接生效

    Ok(ImportPlan {
        projects,
        metrics,
        observations,
        issues,
        handoffs,
        artifacts,
        renamed,
        north_star_notes,
        rejected,
        raw_counts,
    })
}

fn parse_issue_status(s: &str) -> Option<IssueStatus> {
    match s {
        "backlog" => Some(IssueStatus::Backlog),
        "todo" => Some(IssueStatus::Todo),
        "in_progress" => Some(IssueStatus::InProgress),
        "in_review" => Some(IssueStatus::InReview),
        "done" => Some(IssueStatus::Done),
        "blocked" => Some(IssueStatus::Blocked),
        "cancelled" => Some(IssueStatus::Cancelled),
        _ => None,
    }
}

/// 北极星来源判定的结果——`origin` 是落库的值("file"|"manual"),`basis`
/// 是 Important-3 要求的判定依据人话说明,原样打进导入报告(见
/// `NorthStarOriginNote`/`print_diff`),不是另算的摘要。
struct NorthStarOrigin {
    origin: String,
    basis: String,
}

/// 北极星来源判定(design §2.2②「判定规则」,2026-08-11 主控裁决 6:按
/// 稿——读项目仓正本文件比名字)。`root_path` 空 = 项目从未配置本地检
/// 出,直接判「手建」,不会去试着读一个不存在的目录。**Important-3**:判
/// 定依据(比中了哪个名字/没比中走了什么)不再只留在返回值里——调用方
/// (`analyze`)把 `basis` 存进 `ImportPlan::north_star_notes`,`print_diff`
/// 逐行打进差异清单,确认门前人能看见这次判定的依据。
fn determine_north_star_origin(root_path: &str, ns_name: &str) -> NorthStarOrigin {
    if root_path.trim().is_empty() {
        return NorthStarOrigin {
            origin: "manual".to_string(),
            basis: "项目没有配置本地检出根(root_path 为空),没有正本文件可比,判「手建」".to_string(),
        };
    }
    match bw_workspace::metrics_lenient::read_lenient(root_path) {
        Ok(Some(file)) => match file.north_star {
            Some(ns) if ns.name == ns_name => NorthStarOrigin {
                origin: "file".to_string(),
                basis: format!(
                    "正本 {root_path}/.bw/metrics.toml 读到北极星名字「{}」,与旧库项目表北极星\
                     名字「{ns_name}」逐字相同,判「来自正本」",
                    ns.name
                ),
            },
            Some(ns) => NorthStarOrigin {
                origin: "manual".to_string(),
                basis: format!(
                    "正本 {root_path}/.bw/metrics.toml 读到北极星名字「{}」,与旧库项目表北极星\
                     名字「{ns_name}」不同,判「手建」",
                    ns.name
                ),
            },
            None => NorthStarOrigin {
                origin: "manual".to_string(),
                basis: "正本 .bw/metrics.toml 读到了,但没有 [north_star] 一节,判「手建」"
                    .to_string(),
            },
        },
        Ok(None) => NorthStarOrigin {
            origin: "manual".to_string(),
            basis: format!("{root_path}/.bw/metrics.toml 不存在,判「手建」"),
        },
        Err(e) => NorthStarOrigin {
            origin: "manual".to_string(),
            basis: format!("读 {root_path}/.bw/metrics.toml 失败({e}),判「手建」"),
        },
    }
}

fn print_diff(plan: &ImportPlan) {
    println!("== 差异清单(design §2.5 第 2 段:没有原样导过去的东西)==");
    if plan.renamed.is_empty() {
        println!("  (本次没有指标被改名)");
    } else {
        for (project, old_name, new_name) in &plan.renamed {
            println!("  【改名】{project}:「{old_name}」→「{new_name}」");
        }
    }
    println!("  项目未导入列:{PROJECT_UNTRANSFERRED}");
    println!("  指标未导入列:{METRIC_UNTRANSFERRED}");
    println!("  活未导入列:{ISSUE_UNTRANSFERRED}");
    println!(
        "  旧库没有『尚未开棒』这个状态——旧库 active_stage 不可空(默认原型),\
         导入后每个项目都会有一棒"
    );
    println!(
        "  产物证据:{} 行,主控裁决 1 导成历史存档表 artifact_archive(明确标注\"历史存档、\
         不再增长\",与新工程读时现采的交付证据栏分开)",
        plan.artifacts.len()
    );
    print_north_star_notes(plan);
}

/// Important-3(主控裁决 6 后半句):北极星来源的判定依据打进导入报告,
/// 不止落在 `plan.metrics` 那一行的 `origin` 字段里——人在敲 `--confirm`
/// 之前要能看见"这次判成什么、依据是什么",不用去翻数据库。
fn print_north_star_notes(plan: &ImportPlan) {
    println!("  == 北极星来源判定依据(design §2.2②/主控裁决 6)==");
    if plan.north_star_notes.is_empty() {
        println!("    (本次没有项目定义北极星)");
        return;
    }
    for note in &plan.north_star_notes {
        let origin_label = match note.origin.as_str() {
            "file" => "来自正本",
            "manual" => "手建",
            other => other,
        };
        println!(
            "    {}:判「{origin_label}」——{}",
            note.project_name, note.basis
        );
    }
}

fn print_rejected(plan: &ImportPlan) {
    println!("== 拒绝清单(design §2.5 第 3 段)==");
    if plan.rejected.is_empty() {
        println!("  (空——没有撞唯一约束又不能自动处置的行)");
    } else {
        for (i, r) in plan.rejected.iter().enumerate() {
            println!("  [{}] {r}", i + 1);
        }
        println!(
            "  拒绝清单非空——只要走 --confirm,这次导入一条都不会写(design §2.5:\
             半截导入比不导更难收拾)"
        );
    }
}

// ─────────────────────────── 真写(write_plan)───────────────────────────

/// Important-4:每类实体除了「新增」(`import_*` 返回 `true`),再独立计一
/// 份「撞主键被 `INSERT OR IGNORE` 静默挡下」(`import_*` 返回 `false`)。
/// 两个计数器在同一次循环里各自累加、互不依赖对方算出来——`written +
/// skipped` 因此是一条真的勾稽关系,不是「跳过 = 计划 - 新增」这种从一个
/// 数字反推出另一个数字的同义反复。
#[derive(Default, Clone, Copy)]
struct WriteReport {
    project: i64,
    project_skipped: i64,
    metric: i64,
    metric_skipped: i64,
    observation: i64,
    observation_skipped: i64,
    issue: i64,
    issue_skipped: i64,
    handoff: i64,
    handoff_skipped: i64,
    artifact: i64,
    artifact_skipped: i64,
}

impl WriteReport {
    fn print(&self, marker: &str) {
        println!(
            "{marker} project={} metric={} observation={} issue={} handoff={} artifact={}",
            self.project, self.metric, self.observation, self.issue, self.handoff, self.artifact
        );
    }
}

/// Important-4 处置:生产写入路径按实体打「计划 N / 实写 M / 跳过 K」,
/// `N == M + K` 三数勾稽——`import_*` 全是 `INSERT OR IGNORE`,任何约束冲
/// 突(不只是主键重复,比如 `uq_metric_identity`)都会被静默挡下,这条对
/// 账让「少了一条」从「肉眼看不出来」变成「一行打印」。`plan.X.len()` 是
/// N(独立于 `write_plan` 内部计数,来自 `analyze` 阶段就定下的计划规模);
/// M/K 是 `write_plan` 循环里各自累加的独立计数——三者不自洽本身就是一次
/// 真实的账目错误(比如某条 `import_*` 调用 panic 被外层吞掉、或者一次意
/// 外的提前 `continue`),不是逻辑上永远成立的等式。返回 `false` 时调用方
/// 应当判定整次运行失败,不能假装对上了。
fn print_write_reconciliation(plan: &ImportPlan, report: &WriteReport) -> bool {
    println!("== 写入对账(计划/实写/跳过,design 七-1 修复轮 Important-4)==");
    let rows: [(&str, i64, i64, i64); 6] = [
        (
            "project",
            plan.projects.len() as i64,
            report.project,
            report.project_skipped,
        ),
        (
            "metric",
            plan.metrics.len() as i64,
            report.metric,
            report.metric_skipped,
        ),
        (
            "observation",
            plan.observations.len() as i64,
            report.observation,
            report.observation_skipped,
        ),
        (
            "issue",
            plan.issues.len() as i64,
            report.issue,
            report.issue_skipped,
        ),
        (
            "handoff",
            plan.handoffs.len() as i64,
            report.handoff,
            report.handoff_skipped,
        ),
        (
            "artifact",
            plan.artifacts.len() as i64,
            report.artifact,
            report.artifact_skipped,
        ),
    ];
    let mut ok = true;
    for (label, planned, written, skipped) in rows {
        let consistent = planned == written + skipped;
        if !consistent {
            ok = false;
        }
        let mark = if consistent {
            ""
        } else {
            "  ← ASSERT FAILED:N ≠ M+K"
        };
        println!("  {label}:计划 {planned} / 实写 {written} / 跳过 {skipped}{mark}");
    }
    ok
}

/// 真写——只在 `plan.rejected` 为空时才会被调用(调用方负责这条门禁,
/// design §2.5:「只要拒绝清单非空,就一条都不写」)。每个 `import_*` 返
/// 回 `true`/`false` 分别计进「新增」/「撞主键跳过」两个独立计数器
/// (Important-4:跳过也要被看见,不只是新增)。
async fn write_plan(store: &ImportSqliteStore, plan: &ImportPlan) -> Result<WriteReport, String> {
    let mut report = WriteReport::default();
    for row in &plan.projects {
        if store
            .import_project(row.clone())
            .await
            .map_err(|e| format!("import_project 失败:{e}"))?
        {
            report.project += 1;
        } else {
            report.project_skipped += 1;
        }
    }
    for row in &plan.metrics {
        if store
            .import_metric(row.clone())
            .await
            .map_err(|e| format!("import_metric 失败:{e}"))?
        {
            report.metric += 1;
        } else {
            report.metric_skipped += 1;
        }
    }
    for row in &plan.observations {
        if store
            .import_observation(row.clone())
            .await
            .map_err(|e| format!("import_observation 失败:{e}"))?
        {
            report.observation += 1;
        } else {
            report.observation_skipped += 1;
        }
    }
    for row in &plan.issues {
        if store
            .import_issue(row.clone())
            .await
            .map_err(|e| format!("import_issue 失败:{e}"))?
        {
            report.issue += 1;
        } else {
            report.issue_skipped += 1;
        }
    }
    for row in &plan.handoffs {
        if store
            .import_handoff(row.clone())
            .await
            .map_err(|e| format!("import_handoff 失败:{e}"))?
        {
            report.handoff += 1;
        } else {
            report.handoff_skipped += 1;
        }
    }
    for row in &plan.artifacts {
        if store
            .import_artifact(row.clone())
            .await
            .map_err(|e| format!("import_artifact 失败:{e}"))?
        {
            report.artifact += 1;
        } else {
            report.artifact_skipped += 1;
        }
    }
    Ok(report)
}

// ─────────────────────────── --from/--to 真实生产路径 ───────────────────

/// `run_real_core` 的结局——Important-1 处置:七A 报告自称「43 条断言全
/// 过」时,`run_real`(将来真的对着用户日常库跑的那个函数)一次都没被自
/// 测档调用过,自测档自己手工拼 `analyze`+`write_plan`,绕开了默认只看
/// 门(:1099 原行号)与拒绝门(:1109 原行号)——2026-08-11 复审 M4 突变把
/// 两条门同时废掉,43 条断言照旧全绿。这个枚举 + `run_real_core` 把
/// `run_real` 唯一的编排逻辑抽成一个自测档也能直接调用的函数,`run_self_
/// test` 从此调的是**同一份产品代码**,两条门被删,自测档会真的翻红,不
/// 是被指挥器绕过去。
enum RunOutcome {
    /// 默认档(`!confirm`),或产品代码判定「不该往下走」时的门——一个字
    /// 节没写入 `to`。
    DryRun { plan: ImportPlan },
    /// `--confirm` 但拒绝清单非空——一个字节没写入 `to`(`to` 这个文件本
    /// 身也不会被创建,见 §2.7 手工核验:门在开新库之前就拦下了)。
    RejectedNoWrite { plan: ImportPlan },
    /// 真写完成。
    Wrote {
        plan: ImportPlan,
        report: WriteReport,
    },
    /// 任何一步失败(旧库打不开、analyze 失败、开新库失败、写入失败、写
    /// 入对账不自洽、旧库指纹跑飞……)——人话原因原样带出来,调用方决定
    /// 怎么处理(CLI 路径打到 stderr + 非零退出;自测档记进 Ledger)。
    Failed(String),
}

/// `run_real`(CLI 生产路径)与 `run_self_test`(自测档)共用的唯一编排
/// 核心——Important-1 的落地:默认只看门与拒绝门都在**这个函数里**,不
/// 是外层各自实现一遍。`confirm=false` 时旧库指纹跑前跑后必须相同——这
/// 条检查在门 1 里,不是调用方的事。
async fn run_real_core(from: &Path, to: &Path, confirm: bool) -> RunOutcome {
    let fp_before = match fingerprint(from) {
        Ok(f) => f,
        Err(e) => return RunOutcome::Failed(e),
    };

    let old_pool = match open_legacy_readonly(from).await {
        Ok(p) => p,
        Err(e) => return RunOutcome::Failed(e),
    };
    let plan = match analyze(&old_pool).await {
        Ok(p) => p,
        Err(e) => return RunOutcome::Failed(e),
    };
    old_pool.close().await;

    plan.raw_counts.print();
    print_diff(&plan);
    print_rejected(&plan);

    // 门 1(design §2.5「默认只看不写」):不给 --confirm,分析完就打住,
    // 连新库文件都不碰。
    if !confirm {
        let fp_after = fingerprint(from).unwrap_or_default();
        if fp_after != fp_before {
            return RunOutcome::Failed(format!(
                "ASSERT FAILED: 只看档跑完,旧库文件指纹变了({fp_before} → {fp_after})——绝不应该发生"
            ));
        }
        return RunOutcome::DryRun { plan };
    }

    // 门 2(design §2.5「拒绝清单非空就一条都不写」):这条门在开新库**之
    // 前**——拒绝清单非空时,`to` 这个文件本身都不会被创建(§2.7 手工核
    // 验实测:比设计稿要求的「一个字节不写」更硬)。
    if !plan.rejected.is_empty() {
        println!("拒绝清单非空,本次不写入任何实体(design §2.5)");
        return RunOutcome::RejectedNoWrite { plan };
    }

    // 基础五表(project/metric/observation/issue/handoff)——只有
    // `bw_store::SqliteStore::open` 知道 schema.sql,这是壳唯一会走的开
    // 库路径,也是它「结构上不知道导入表存在」这条承诺的来源(Critical-1
    // 修复轮)。开完立刻丢弃这个连接,真正的写入面是下面的
    // `ImportSqliteStore`。
    if let Err(e) = SqliteStore::open(&to.to_string_lossy()).await {
        return RunOutcome::Failed(format!("开新库失败:{e}"));
    }
    let store = match ImportSqliteStore::open(&to.to_string_lossy()).await {
        Ok(s) => s,
        Err(e) => return RunOutcome::Failed(format!("开新库(导入写入面)失败:{e}")),
    };
    println!("即将写入……");
    plan.raw_counts.print();
    let report = match write_plan(&store, &plan).await {
        Ok(r) => r,
        Err(e) => return RunOutcome::Failed(e),
    };
    if !print_write_reconciliation(&plan, &report) {
        return RunOutcome::Failed(
            "写入对账不自洽(计划 N ≠ 实写 M + 跳过 K)——记账本身出了错,不能假装写完了".to_string(),
        );
    }
    let fp_now = fingerprint(from).unwrap_or_default();
    if let Err(e) = store
        .record_import_ledger(ImportLedgerEntry {
            legacy_db_path: from.to_string_lossy().to_string(),
            legacy_db_fingerprint: fp_now.clone(),
            project_count: report.project,
            metric_count: report.metric,
            observation_count: report.observation,
            issue_count: report.issue,
            handoff_count: report.handoff,
            artifact_count: report.artifact,
        })
        .await
    {
        return RunOutcome::Failed(format!("记导入流水失败:{e}"));
    }
    if fp_now != fp_before {
        return RunOutcome::Failed(format!(
            "ASSERT FAILED: 真写完,旧库文件指纹变了({fp_before} → {fp_now})——旧库不该被碰"
        ));
    }
    RunOutcome::Wrote { plan, report }
}

async fn run_real(from: &Path, to: &Path, confirm: bool) -> ExitCode {
    println!("旧库:{}", from.display());
    println!("新库:{}", to.display());
    match run_real_core(from, to, confirm).await {
        RunOutcome::DryRun { .. } => {
            println!("IMPORT_DRYRUN_OK");
            ExitCode::SUCCESS
        }
        RunOutcome::RejectedNoWrite { .. } => {
            println!("IMPORT_REJECTED_NO_WRITE");
            ExitCode::SUCCESS
        }
        RunOutcome::Wrote { report, .. } => {
            report.print("IMPORT_WROTE");
            ExitCode::SUCCESS
        }
        RunOutcome::Failed(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

// ─────────────────────────── 合成老库指挥器(自测档)───────────────────

/// 造合成旧库需要的六张表——照 v1 `crates/bw-store/src/schema.sql` 原文
/// 抄需要的六张(design §9「旧库的表结构」:照抄需要的六张,不抄整份)。
const LEGACY_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS project (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    kind               TEXT NOT NULL,
    descr              TEXT NOT NULL DEFAULT '',
    phase              TEXT NOT NULL,
    cycle              TEXT NOT NULL DEFAULT 'explore',
    active_stage       TEXT NOT NULL DEFAULT 'prototype',
    north_star         TEXT NOT NULL DEFAULT '',
    ns_def             TEXT NOT NULL DEFAULT '',
    benchmark          TEXT NOT NULL DEFAULT '',
    opportunity        TEXT NOT NULL DEFAULT '',
    workspace_path     TEXT NOT NULL DEFAULT '',
    allow_commands     INTEGER NOT NULL DEFAULT 0,
    remote_path        TEXT NOT NULL DEFAULT '',
    remote_host        TEXT NOT NULL DEFAULT 'github.com',
    provider           TEXT NOT NULL DEFAULT 'github',
    north_star_collect_kind  TEXT NOT NULL DEFAULT '',
    north_star_collect_query TEXT NOT NULL DEFAULT '',
    signal             TEXT,
    weekly_signal      TEXT,
    signal_derived_rev INTEGER,
    signal_derived_at  INTEGER,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    rev                INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS metric (
    id                 TEXT PRIMARY KEY,
    project_id         TEXT NOT NULL REFERENCES project(id),
    role               TEXT NOT NULL,
    stage_kind         TEXT,
    name               TEXT NOT NULL,
    def                TEXT NOT NULL DEFAULT '',
    target_raw         TEXT NOT NULL DEFAULT '',
    amber_kind         TEXT NOT NULL DEFAULT 'rel',
    amber_value        REAL NOT NULL DEFAULT 0.10,
    last_target        TEXT NOT NULL DEFAULT '',
    driver             TEXT NOT NULL DEFAULT '',
    collect_kind       TEXT NOT NULL DEFAULT '',
    collect_query      TEXT NOT NULL DEFAULT '',
    origin             TEXT NOT NULL DEFAULT 'manual',
    archived           INTEGER NOT NULL DEFAULT 0,
    archived_at        INTEGER,
    signal             TEXT,
    hit                INTEGER,
    signal_derived_rev INTEGER,
    pos                INTEGER NOT NULL DEFAULT 0,
    created_at         INTEGER NOT NULL,
    updated_at         INTEGER NOT NULL,
    rev                INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS observation (
    id            TEXT PRIMARY KEY,
    metric_id     TEXT NOT NULL REFERENCES metric(id),
    ts            INTEGER NOT NULL,
    source_kind   TEXT NOT NULL,
    raw           TEXT NOT NULL,
    source_run_id TEXT,
    created_at    INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_observation_metric_ts ON observation(metric_id, ts DESC);
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
CREATE TABLE IF NOT EXISTS artifact (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES project(id),
    workflow_run_id TEXT,
    issue_id        TEXT,
    stage_kind      TEXT,
    path            TEXT NOT NULL,
    kind            TEXT NOT NULL DEFAULT 'other',
    bytes           INTEGER NOT NULL DEFAULT 0,
    git_commit      TEXT NOT NULL DEFAULT '',
    registered_at   INTEGER NOT NULL,
    UNIQUE(project_id, path, git_commit)
);
CREATE INDEX IF NOT EXISTS idx_artifact_project ON artifact(project_id, registered_at DESC);
CREATE TABLE IF NOT EXISTS issue (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    stage       TEXT NOT NULL,
    number      INTEGER NOT NULL,
    github_number INTEGER NOT NULL DEFAULT 0,
    pr_number   INTEGER NOT NULL DEFAULT 0,
    title       TEXT NOT NULL,
    descr       TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'backlog',
    priority    TEXT NOT NULL DEFAULT 'none',
    assignee    TEXT,
    settled_at  INTEGER,
    blocked_reason TEXT,
    standard_skill TEXT NOT NULL DEFAULT '',
    interactive_started INTEGER NOT NULL DEFAULT 0,
    claude_session_id TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_issue_project_number ON issue(project_id, number);
CREATE INDEX IF NOT EXISTS idx_issue_project_stage ON issue(project_id, stage);
";

async fn build_legacy_schema(pool: &SqlitePool) -> Result<(), String> {
    for stmt in LEGACY_SCHEMA.split(';') {
        if stmt.trim().is_empty() {
            continue;
        }
        sqlx::query(stmt)
            .execute(pool)
            .await
            .map_err(|e| format!("老库建表语句失败:{e}\n语句:{stmt}"))?;
    }
    Ok(())
}

async fn create_legacy_db(path: &Path) -> Result<SqlitePool, String> {
    let _ = std::fs::remove_file(path);
    let opts = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(opts)
        .await
        .map_err(|e| format!("造老库失败:{e}"))?;
    build_legacy_schema(&pool).await?;
    Ok(pool)
}

struct LegacyIds {
    project_aihot: String,
    project_workflowhub: String,
    issue_done: String,
    /// Important-2 兜底分支的样本:`archived=1`、`archived_at=NULL`。
    archived_no_ts_metric_id: String,
    /// 上面这条指标的 `updated_at`——兜底逻辑应当把它当停用时刻顶上,读
    /// 回断言直接比这个值,不是重新算一遍。
    archived_no_ts_expected_at: i64,
}

/// 造「好」的合成旧库——覆盖 §2.2/§2.4 三处硬碰撞里的两处(阶段维度重名
/// 指标族 + 一条来自正本的行),不含拒绝路径(那是 `seed_dirty` 的事)。
async fn seed_good(pool: &SqlitePool, workspace_root: &Path) -> Result<LegacyIds, String> {
    let now = now();

    let project_aihot = Uuid::new_v4().to_string();
    let project_workflowhub = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO project (id, name, kind, phase, workspace_path, active_stage, north_star, \
         ns_def, north_star_collect_kind, north_star_collect_query, created_at, updated_at) \
         VALUES (?, 'aihot 日报(合成)', 'content', 'running', ?, 'build', '周活跃创作者数', \
         '每周至少互动一次的创作者数', 'connector', 'content-analytics', ?, ?)",
    )
    .bind(&project_aihot)
    .bind(workspace_root.to_string_lossy().to_string())
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入 project(aihot)失败:{e}"))?;

    sqlx::query(
        "INSERT INTO project (id, name, kind, phase, workspace_path, active_stage, north_star, \
         ns_def, created_at, updated_at) \
         VALUES (?, 'WorkflowHub(合成)', 'tool', 'running', '', 'ops', '', '', ?, ?)",
    )
    .bind(&project_workflowhub)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入 project(workflowhub)失败:{e}"))?;

    // .bw/metrics.toml —— aihot 项目工作区里放一份正本文件,北极星名字与
    // 上面 project.north_star 逐字相同,用来验证「来自正本」判定分支
    // (design §2.2②)。
    let bw_dir = workspace_root.join(".bw");
    std::fs::create_dir_all(&bw_dir).map_err(|e| format!("造 .bw 目录失败:{e}"))?;
    std::fs::write(
        bw_dir.join("metrics.toml"),
        r#"schema_version = 1

[north_star]
name = "周活跃创作者数"
def = "每周至少互动一次的创作者数"
collect = { kind = "connector", query = "content-analytics" }

[[lagging]]
name = "月流失率"
def = "上月活跃、本月零互动的创作者占比"
target = "≤5%"
collect = { kind = "connector", query = "content-analytics" }
"#,
    )
    .map_err(|e| format!("写 .bw/metrics.toml 失败:{e}"))?;

    // 阶段维度的重名指标族(§2.4 第一处硬碰撞):5 条同名"阶段完成 Issue
    // 数",挂在 aihot 项目下,tier=leading,origin=manual,各自一条观测。
    let family_ids: Vec<String> = (0..5).map(|_| Uuid::new_v4().to_string()).collect();
    let stages = ["prototype", "build", "optimize", "growth", "ops"];
    for (i, stage) in stages.iter().enumerate() {
        sqlx::query(
            "INSERT INTO metric (id, project_id, role, stage_kind, name, def, target_raw, \
             collect_kind, collect_query, origin, created_at, updated_at) \
             VALUES (?, ?, 'leading', ?, '阶段完成 Issue 数', '这一棒里已完成的 Issue 数', '≥3', \
             'bw', 'issue.settled_at within stage', 'manual', ?, ?)",
        )
        .bind(&family_ids[i])
        .bind(&project_aihot)
        .bind(stage)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| format!("插入重名指标族第 {i} 条失败:{e}"))?;

        let obs_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO observation (id, metric_id, ts, source_kind, raw, created_at) \
             VALUES (?, ?, ?, 'connector', ?, ?)",
        )
        .bind(&obs_id)
        .bind(&family_ids[i])
        .bind(now - (i as i64) * 3600)
        .bind(format!("{}", i + 1))
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| format!("插入重名指标族观测第 {i} 条失败:{e}"))?;
    }

    // 一条来自正本的行(§2.2③ origin 原样透传测试):与 .bw/metrics.toml
    // 里的"月流失率"同名同层,origin='file'。
    let file_metric_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO metric (id, project_id, role, name, def, target_raw, collect_kind, \
         collect_query, origin, created_at, updated_at) \
         VALUES (?, ?, 'lagging', '月流失率', '上月活跃、本月零互动的创作者占比', '≤5%', \
         'connector', 'content-analytics', 'file', ?, ?)",
    )
    .bind(&file_metric_id)
    .bind(&project_aihot)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入 file-origin 指标失败:{e}"))?;
    let obs_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO observation (id, metric_id, ts, source_kind, raw, source_run_id, created_at) \
         VALUES (?, ?, ?, 'telemetry', '4.2%', 'old-run-42', ?)",
    )
    .bind(&obs_id)
    .bind(&file_metric_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入 file-origin 指标观测失败:{e}"))?;

    // WorkflowHub 项目下一条手建指标(填充,不参与碰撞)。
    let filler_metric_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO metric (id, project_id, role, name, origin, created_at, updated_at) \
         VALUES (?, ?, 'leading', '测试指标(合成)', 'manual', ?, ?)",
    )
    .bind(&filler_metric_id)
    .bind(&project_workflowhub)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入填充指标失败:{e}"))?;

    // Important-2 兜底分支的样本:archived=1、archived_at 留空——旧库真实
    // 可能出现的一种数据形态(两列各自维护,不保证同时落值)。updated_at
    // 与 created_at 故意不同(created_at 更早),这样"取 updated_at 顶上"
    // 这条兜底逻辑能被读回精确核对,不会跟 created_at 混同。
    let archived_no_ts_metric_id = Uuid::new_v4().to_string();
    let archived_no_ts_expected_at = now - 400_000;
    sqlx::query(
        "INSERT INTO metric (id, project_id, role, name, origin, archived, archived_at, \
         created_at, updated_at) \
         VALUES (?, ?, 'lagging', '停用无时刻指标(合成)', 'manual', 1, NULL, ?, ?)",
    )
    .bind(&archived_no_ts_metric_id)
    .bind(&project_workflowhub)
    .bind(now - 500_000)
    .bind(archived_no_ts_expected_at)
    .execute(pool)
    .await
    .map_err(|e| format!("插入停用无时刻指标失败:{e}"))?;

    // 活:aihot 3 件(待办/进行中/已完成),WorkflowHub 2 件(待办/评审中)。
    let issue_backlog = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO issue (id, project_id, stage, number, title, descr, status, created_at, updated_at) \
         VALUES (?, ?, 'build', 1, '写日报模板', '', 'backlog', ?, ?)",
    )
    .bind(&issue_backlog)
    .bind(&project_aihot)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入活(backlog)失败:{e}"))?;

    let issue_in_progress = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO issue (id, project_id, stage, number, title, descr, status, created_at, updated_at) \
         VALUES (?, ?, 'build', 2, '接入新数据源', '', 'in_progress', ?, ?)",
    )
    .bind(&issue_in_progress)
    .bind(&project_aihot)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入活(in_progress)失败:{e}"))?;

    let issue_done = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO issue (id, project_id, stage, number, title, descr, status, settled_at, created_at, updated_at) \
         VALUES (?, ?, 'optimize', 3, '修复日报延迟', '', 'done', ?, ?, ?)",
    )
    .bind(&issue_done)
    .bind(&project_aihot)
    .bind(now - 86400)
    .bind(now - 2 * 86400)
    .bind(now - 86400)
    .execute(pool)
    .await
    .map_err(|e| format!("插入活(done)失败:{e}"))?;

    let wf_issue_1 = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO issue (id, project_id, stage, number, title, descr, status, created_at, updated_at) \
         VALUES (?, ?, 'ops', 1, '排查网关抖动', '', 'todo', ?, ?)",
    )
    .bind(&wf_issue_1)
    .bind(&project_workflowhub)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入活(workflowhub #1)失败:{e}"))?;

    let wf_issue_2 = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO issue (id, project_id, stage, number, title, descr, status, created_at, updated_at) \
         VALUES (?, ?, 'ops', 2, '补充监理脚本文档', '', 'in_review', ?, ?)",
    )
    .bind(&wf_issue_2)
    .bind(&project_workflowhub)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入活(workflowhub #2)失败:{e}"))?;

    // 交棒:WorkflowHub 2 条(1 条带险),这正是 design §2.2⑦「当前一棒的
    // 带险欠账会不会亮」的天然样本——WorkflowHub 当前一棒是 ops,第二条
    // 带险交棒交到的正是 ops,导入后 §5⑤ 那盏灯应该亮。
    sqlx::query(
        "INSERT INTO handoff (id, project_id, from_stage, to_stage, risky, note, created_at) \
         VALUES (?, ?, 'prototype', 'build', 0, '', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&project_workflowhub)
    .bind(now - 200000)
    .execute(pool)
    .await
    .map_err(|e| format!("插入交棒(不带险)失败:{e}"))?;

    sqlx::query(
        "INSERT INTO handoff (id, project_id, from_stage, to_stage, risky, note, created_at) \
         VALUES (?, ?, 'growth', 'ops', 1, '清单没勾完:监理脚本文档没补齐', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&project_workflowhub)
    .bind(now - 100000)
    .execute(pool)
    .await
    .map_err(|e| format!("插入交棒(带险)失败:{e}"))?;

    // 产物证据:aihot 2 条(1 条挂 issue_done,1 条不挂),WorkflowHub 1
    // 条不挂——用来验「历史存档表关联保全」(issue_id 这条关联不丢)。
    sqlx::query(
        "INSERT INTO artifact (id, project_id, issue_id, stage_kind, path, kind, bytes, \
         git_commit, registered_at) \
         VALUES (?, ?, ?, 'optimize', 'src/report/daily.py', 'code', 4096, 'abc1234', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&project_aihot)
    .bind(&issue_done)
    .bind(now - 86400)
    .execute(pool)
    .await
    .map_err(|e| format!("插入产物登记(挂活)失败:{e}"))?;

    sqlx::query(
        "INSERT INTO artifact (id, project_id, issue_id, stage_kind, path, kind, bytes, \
         git_commit, registered_at) \
         VALUES (?, ?, NULL, 'build', 'README.md', 'docs', 512, 'def5678', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&project_aihot)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入产物登记(不挂活)失败:{e}"))?;

    sqlx::query(
        "INSERT INTO artifact (id, project_id, issue_id, stage_kind, path, kind, bytes, \
         git_commit, registered_at) \
         VALUES (?, ?, NULL, 'ops', 'scripts/supervise.sh', 'code', 1024, 'ghi9012', ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&project_workflowhub)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入产物登记(workflowhub)失败:{e}"))?;

    Ok(LegacyIds {
        project_aihot,
        project_workflowhub,
        issue_done,
        archived_no_ts_metric_id,
        archived_no_ts_expected_at,
    })
}

/// 造「脏」的合成旧库——只造一处硬碰撞(重复活编号),用来单独验证
/// design §2.5「拒绝清单非空,就一条都不写」的全局门(§6.1 读回清单第
/// 8 条)。
async fn seed_dirty(pool: &SqlitePool) -> Result<(), String> {
    let now = now();
    let project_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO project (id, name, kind, phase, active_stage, created_at, updated_at) \
         VALUES (?, '脏合成项目', 'tool', 'running', 'prototype', ?, ?)",
    )
    .bind(&project_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .map_err(|e| format!("插入 project(dirty)失败:{e}"))?;

    // 两条 number=1 的活——新库的 (project_id, number) 唯一索引钉死这
    // 条,应当整体拒绝。
    for title in ["撞号活 A", "撞号活 B"] {
        sqlx::query(
            "INSERT INTO issue (id, project_id, stage, number, title, status, created_at, updated_at) \
             VALUES (?, ?, 'prototype', 1, ?, 'backlog', ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&project_id)
        .bind(title)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await
        .map_err(|e| format!("插入撞号活 {title} 失败:{e}"))?;
    }
    Ok(())
}

async fn count_table(pool: &SqlitePool, table: &str) -> Result<i64, String> {
    let row = sqlx::query(&format!("SELECT COUNT(*) AS n FROM {table}"))
        .fetch_one(pool)
        .await
        .map_err(|e| format!("COUNT({table}) 失败:{e}"))?;
    Ok(row.get("n"))
}

async fn count_observations_for_metric(pool: &SqlitePool, metric_id: &str) -> Result<i64, String> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM observation WHERE metric_id = ?")
        .bind(metric_id)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("COUNT(observation) 失败:{e}"))?;
    Ok(row.get("n"))
}

/// Critical-1 修复轮的行为承诺兜底(task-s7a-review.md 验收标准 1):**只用
/// 壳会走的那条开库路径**(`bw_store::SqliteStore::open`,不碰
/// `bw_store_import` 一个字)开一个全新的库,直接读 `sqlite_master` 断言
/// `import_ledger`/`artifact_archive` 这两张导入专用表不存在。这条断言不
/// 是「查 cargo 依赖图」那类间接检法,是照着壳自己会做的事原样做一遍、
/// 再读回——即便这次编译把 `bw-store-import` 统一进了某个中间产物(结构
/// 解法之后这已经不可能发生,见 crate 文档),只要 `SqliteStore::open` 内
/// 部真的不调用那份 schema,这两张表就永远不会出现在这里。
async fn assert_shell_open_path_has_no_import_tables(l: &mut Ledger) {
    let db_path = fresh_db_path("shell-open-path");
    let store = match SqliteStore::open(&db_path.to_string_lossy()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("Critical-1:壳路径(SqliteStore::open)开一个全新库成功");
            return;
        }
    };
    drop(store);
    let verify = match SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&db_path)
            .read_only(true),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("Critical-1:壳路径新库的只读校验连接成功");
            return;
        }
    };
    for table in ["import_ledger", "artifact_archive"] {
        match sqlx::query(
            "SELECT COUNT(*) AS n FROM sqlite_master WHERE type = 'table' AND name = ?",
        )
        .bind(table)
        .fetch_one(&verify)
        .await
        {
            Ok(row) => {
                let n: i64 = row.get("n");
                l.check(
                    n == 0,
                    format!(
                        "Critical-1:壳的开库路径(SqliteStore::open)不建 {table}(sqlite_master \
                         读回,实得 {n} 行)"
                    ),
                );
            }
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail(format!(
                    "Critical-1:查询 sqlite_master 里有没有 {table} 成功"
                ));
            }
        }
    }
    verify.close().await;
    let _ = std::fs::remove_file(&db_path);
}

fn outcome_label(o: &RunOutcome) -> &'static str {
    match o {
        RunOutcome::DryRun { .. } => "DryRun",
        RunOutcome::RejectedNoWrite { .. } => "RejectedNoWrite",
        RunOutcome::Wrote { .. } => "Wrote",
        RunOutcome::Failed(_) => "Failed",
    }
}

async fn run_self_test() -> ExitCode {
    clear_stale_dbs();
    let mut l = Ledger::new();

    // ═══════════════ Critical-1:壳的开库路径绝不建导入表(行为承诺兜底)═══
    assert_shell_open_path_has_no_import_tables(&mut l).await;

    // ═══════════════ good 库:主流程 + 幂等 + 只读证明 + 触发器 ═══════════════
    let legacy_good = fresh_db_path("legacy-good");
    let workspace_root =
        std::env::temp_dir().join(format!("bw-import-legacy-ws-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&workspace_root);

    let ids = {
        let pool = match create_legacy_db(&legacy_good).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail("造合成旧库(good)成功");
                return finish(l);
            }
        };
        let ids = match seed_good(&pool, &workspace_root).await {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail("造合成旧库(good)种子数据成功");
                return finish(l);
            }
        };
        pool.close().await;
        ids
    };

    let fp_before = fingerprint(&legacy_good).unwrap_or_default();
    println!("旧库(good)指纹(跑前):{fp_before}");

    // ── dry-run(经 run_real_core——Important-1:自测档从此走产品代码里
    //    那条真门,不是自己手工拼 analyze,两条门被删会真的翻红)──
    let new_good = fresh_db_path("new-good");
    println!("\n[good 库 · dry-run(经 run_real_core)]");
    let plan = match run_real_core(&legacy_good, &new_good, false).await {
        RunOutcome::DryRun { plan } => plan,
        RunOutcome::Failed(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("run_real_core(good, dry-run) 返回 DryRun(而不是 Failed)");
            return finish(l);
        }
        other => {
            l.fail(format!(
                "run_real_core(good, dry-run) 应该返回 DryRun,实际:{}",
                outcome_label(&other)
            ));
            return finish(l);
        }
    };
    l.check(
        plan.rejected.is_empty(),
        "good 库:拒绝清单为空(三处硬碰撞里的重复编号只在 dirty 库造)",
    );
    l.check(
        !plan.renamed.is_empty(),
        "good 库:阶段维度重名指标族触发了改名(§2.4 第一处硬碰撞)",
    );
    println!("IMPORT_DRYRUN_OK(good 库,未写入)");

    let fp_after_dryrun = fingerprint(&legacy_good).unwrap_or_default();
    l.check(fp_after_dryrun == fp_before, "dry-run 之后旧库指纹不变");

    // ── confirm 第一次(同样经 run_real_core)──
    println!("\n[good 库 · confirm 第 1 次(经 run_real_core)]");
    let (plan, report1) = match run_real_core(&legacy_good, &new_good, true).await {
        RunOutcome::Wrote { plan, report } => (plan, report),
        RunOutcome::Failed(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("run_real_core(good, confirm 第 1 次) 返回 Wrote(而不是 Failed)");
            return finish(l);
        }
        other => {
            l.fail(format!(
                "run_real_core(good, confirm 第 1 次) 应该返回 Wrote,实际:{}",
                outcome_label(&other)
            ));
            return finish(l);
        }
    };
    report1.print("IMPORT_WROTE");

    // 读回清单 1:七类实体各写入几条 = 计数表打印的数字。
    l.check(
        report1.project as usize == plan.projects.len(),
        "读回 1:project 新增条数 == 计划条数",
    );
    l.check(
        report1.metric as usize == plan.metrics.len(),
        "读回 1:metric 新增条数 == 计划条数",
    );
    l.check(
        report1.observation as usize == plan.observations.len(),
        "读回 1:observation 新增条数 == 计划条数",
    );
    l.check(
        report1.issue as usize == plan.issues.len(),
        "读回 1:issue 新增条数 == 计划条数",
    );
    l.check(
        report1.handoff as usize == plan.handoffs.len(),
        "读回 1:handoff 新增条数 == 计划条数",
    );
    l.check(
        report1.artifact as usize == plan.artifacts.len(),
        "读回 1:artifact(→artifact_archive) 新增条数 == 计划条数",
    );

    // 用一条独立只读连接核对新库(同 store 的写入连接分开,验证写入真的
    // 落盘可见,不是只在内存里对上)。
    let verify = match SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&new_good)
            .read_only(true),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: 新库(good)只读校验连接失败:{e}");
            l.fail("新库(good)只读校验连接成功");
            return finish(l);
        }
    };

    for (table, expect) in [
        ("project", plan.projects.len() as i64),
        ("metric", plan.metrics.len() as i64),
        ("observation", plan.observations.len() as i64),
        ("issue", plan.issues.len() as i64),
        ("handoff", plan.handoffs.len() as i64),
        ("artifact_archive", plan.artifacts.len() as i64),
    ] {
        match count_table(&verify, table).await {
            Ok(n) => l.check(
                n == expect,
                format!("读回 1:SELECT COUNT(*) FROM {table} = {expect}(实得 {n})"),
            ),
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail(format!("COUNT({table}) 查询成功"));
            }
        }
    }

    // 读回清单 2:观测的编号原样保留。
    if let Some(sample) = plan.observations.first() {
        let sample_id = sample.id.uuid().to_string();
        match sqlx::query("SELECT id FROM observation WHERE id = ?")
            .bind(&sample_id)
            .fetch_optional(&verify)
            .await
        {
            Ok(Some(_)) => l.check(true, format!("读回 2:观测编号原样保留(样本 {sample_id})")),
            Ok(None) => l.fail(format!("读回 2:观测编号原样保留(样本 {sample_id} 查不到)")),
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail("读回 2:观测按 id 查询本身成功");
            }
        }
    } else {
        l.fail("读回 2:计划里至少要有一条观测(合成数据缺了这一段)");
    }

    // 读回清单 3:被改名的指标——新名字带阶段后缀,底下的观测条数与旧
    // 库一致(这是「改名没有丢观测」的直接证据)。
    for (project, old_name, new_name) in &plan.renamed {
        let has_stage_suffix = new_name.starts_with(old_name) && new_name.contains(" · ");
        l.check(
            has_stage_suffix,
            format!("读回 3:{project}「{old_name}」→「{new_name}」带阶段后缀"),
        );
    }
    // 抽一条改了名的指标,核对它的观测条数与旧库一致(旧库里这一族每条
    // 各有 1 条观测)。
    if let Some(renamed_metric) = plan.metrics.iter().find(|m| {
        plan.renamed
            .iter()
            .any(|(_, _, new_name)| &m.name == new_name)
    }) {
        match count_observations_for_metric(&verify, &renamed_metric.id.uuid().to_string()).await {
            Ok(n) => l.check(
                n == 1,
                format!(
                    "读回 3:改名指标「{}」底下观测条数与旧库一致(期望 1,实得 {n})",
                    renamed_metric.name
                ),
            ),
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail("读回 3:改名指标观测计数查询成功");
            }
        }
    } else {
        l.fail("读回 3:能找到至少一条改名后的指标行");
    }

    // 读回清单 4:来自正本的行,来源标记原样是「来自正本」。
    if let Some(file_metric) = plan.metrics.iter().find(|m| m.name == "月流失率") {
        l.check(
            file_metric.origin == "file",
            "读回 4:「月流失率」origin 原样是 file(来自正本)",
        );
        match sqlx::query("SELECT origin FROM metric WHERE id = ?")
            .bind(file_metric.id.uuid().to_string())
            .fetch_one(&verify)
            .await
        {
            Ok(row) => {
                let origin: String = row.get("origin");
                l.check(
                    origin == "file",
                    format!("读回 4:新库里这一行 origin 列原样是 'file'(实得 {origin:?})"),
                );
            }
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail("读回 4:查询「月流失率」origin 列成功");
            }
        }
    } else {
        l.fail("读回 4:计划里能找到「月流失率」这一条来自正本的指标");
    }

    // 北极星那一行也应该判定为「来自正本」(工作区里放了同名的 .bw/metrics.toml)。
    if let Some(ns) = plan
        .metrics
        .iter()
        .find(|m| m.tier == MetricTier::NorthStar)
    {
        l.check(
            ns.origin == "file",
            format!(
                "读回 4 附:北极星「{}」判定为来自正本(工作区里有同名正本文件)",
                ns.name
            ),
        );
    } else {
        l.fail("读回 4 附:计划里能找到北极星那一行");
    }

    // Important-3(主控裁决 6 后半句):北极星判定依据打进了报告输出——
    // `plan.north_star_notes` 存的正是 `print_diff`/`print_north_star_notes`
    // 打印的那份数据(不是重新算一遍摘要),这里断言它存在且与数据一致:
    // 恰好一条(aihot,唯一定义了北极星的项目),判「来自正本」,依据文本
    // 里能看到具体比中了哪个名字。
    l.check(
        plan.north_star_notes.len() == 1,
        format!(
            "Important-3:北极星判定依据条数与「定义了北极星的项目数」一致(期望 1,实得 {})",
            plan.north_star_notes.len()
        ),
    );
    if let Some(note) = plan.north_star_notes.first() {
        l.check(
            note.origin == "file"
                && note.basis.contains("逐字相同")
                && note.basis.contains("周活跃创作者数"),
            format!(
                "Important-3:北极星判定依据打进了报告(project={}, origin={}, basis={:?})",
                note.project_name, note.origin, note.basis
            ),
        );
    } else {
        l.fail("Important-3:north_star_notes 里能找到 aihot 那一条");
    }

    // Important-2(design §2.2③兜底分支):archived=1 但 archived_at 为空的
    // 那一条,导入后 archived_at 应当等于旧库的 updated_at(取的是兜底值,
    // 不是被静默复活成"在用")。
    if let Some(m) = plan
        .metrics
        .iter()
        .find(|m| m.id.uuid().to_string() == ids.archived_no_ts_metric_id)
    {
        l.check(
            m.archived_at == Some(ids.archived_no_ts_expected_at),
            format!(
                "Important-2:archived=1/archived_at=NULL 的行,计划里 archived_at 取了 updated_at \
                 兜底(期望 {:?},实得 {:?})",
                Some(ids.archived_no_ts_expected_at),
                m.archived_at
            ),
        );
    } else {
        l.fail("Important-2:计划里能找到「停用无时刻指标(合成)」那一条");
    }
    // 同一件事在新库里再读回一遍(SQL 读回,不只是查内存里的 plan 对
    // 象)——「报告不代答,读回为证」。
    match sqlx::query("SELECT archived_at FROM metric WHERE id = ?")
        .bind(&ids.archived_no_ts_metric_id)
        .fetch_optional(&verify)
        .await
    {
        Ok(Some(row)) => {
            let archived_at: Option<i64> = row.get("archived_at");
            l.check(
                archived_at == Some(ids.archived_no_ts_expected_at),
                format!(
                    "Important-2:新库 SQL 读回 archived_at(期望 {:?},实得 {archived_at:?})——\
                     不是静默复活成在用",
                    Some(ids.archived_no_ts_expected_at)
                ),
            );
        }
        Ok(None) => l.fail("Important-2:「停用无时刻指标(合成)」应该能在新库里按旧编号查到"),
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("Important-2:查询 metric.archived_at 成功");
        }
    }

    // 历史存档表关联保全:artifact_archive.issue_id 原样保留(这是第三组
    // 突变自证的目标断言)。
    if let Some(a) = plan.artifacts.iter().find(|a| a.issue_id.is_some()) {
        match sqlx::query("SELECT issue_id FROM artifact_archive WHERE id = ?")
            .bind(a.id.uuid().to_string())
            .fetch_one(&verify)
            .await
        {
            Ok(row) => {
                let issue_id: Option<String> = row.get("issue_id");
                let expect = a.issue_id.map(|i| i.uuid().to_string());
                l.check(
                    issue_id == expect,
                    format!("历史存档表关联保全:artifact_archive.issue_id 原样保留(期望 {expect:?},实得 {issue_id:?})"),
                );
            }
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail("查询 artifact_archive.issue_id 成功");
            }
        }
    } else {
        l.fail("计划里能找到至少一条挂了 issue_id 的产物登记");
    }

    // 「导入的活按旧库状态原样搬,不做任何状态推进」(硬约束):旧库里
    // issue_done 已经是 done + 有 settled_at,导入不经过
    // `can_transition_to`,新库里这一行应当原样是 done,而不是被推导成别
    // 的状态或者被状态机拦下来。
    match sqlx::query("SELECT status, settled_at FROM issue WHERE id = ?")
        .bind(&ids.issue_done)
        .fetch_optional(&verify)
        .await
    {
        Ok(Some(row)) => {
            let status: String = row.get("status");
            let settled_at: Option<i64> = row.get("settled_at");
            l.check(
                status == "done" && settled_at.is_some(),
                format!(
                    "活按旧库状态原样搬:issue_done 落库 status={status:?} settled_at={settled_at:?}(应为 done + 有值)"
                ),
            );
        }
        Ok(None) => l.fail("issue_done 应该能在新库里按旧编号查到"),
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("查询 issue_done 的 status/settled_at 成功");
        }
    }

    // 两个合成项目都应该落库,且编号原样沿用旧库(幂等语义的另一半证据)。
    for (label, old_id) in [
        ("aihot", &ids.project_aihot),
        ("workflowhub", &ids.project_workflowhub),
    ] {
        match sqlx::query("SELECT name FROM project WHERE id = ?")
            .bind(old_id)
            .fetch_optional(&verify)
            .await
        {
            Ok(Some(_)) => l.check(
                true,
                format!("项目({label})编号原样沿用旧库,新库能按旧 id 查到"),
            ),
            Ok(None) => l.fail(format!("项目({label})应该能按旧编号在新库里查到")),
            Err(e) => {
                eprintln!("ASSERT FAILED: {e}");
                l.fail(format!("查询项目({label})成功"));
            }
        }
    }

    verify.close().await;

    // 读回清单 6:观测表的两条数据库触发器仍然生效(试一次更新、一次删除)。
    check_observation_triggers(&new_good, &plan, &mut l).await;

    // ── confirm 第二次(幂等,同样经 run_real_core)──
    println!("\n[good 库 · confirm 第 2 次(同一份旧库,幂等,经 run_real_core)]");
    let report2 = match run_real_core(&legacy_good, &new_good, true).await {
        RunOutcome::Wrote { report, .. } => report,
        RunOutcome::Failed(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("run_real_core(good, confirm 第 2 次) 返回 Wrote(而不是 Failed)");
            return finish(l);
        }
        other => {
            l.fail(format!(
                "run_real_core(good, confirm 第 2 次) 应该返回 Wrote,实际:{}",
                outcome_label(&other)
            ));
            return finish(l);
        }
    };
    report2.print("IMPORT_WROTE");
    l.check(
        report2.project == 0
            && report2.metric == 0
            && report2.observation == 0
            && report2.issue == 0
            && report2.handoff == 0
            && report2.artifact == 0,
        "读回 5:第二次导入(同一旧库)七类实体新增全部是 0",
    );

    let verify2 = match SqlitePool::connect_with(
        SqliteConnectOptions::new()
            .filename(&new_good)
            .read_only(true),
    )
    .await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("新库(good)第二次只读校验连接成功");
            return finish(l);
        }
    };
    match count_table(&verify2, "project").await {
        Ok(n) => l.check(
            n == plan.projects.len() as i64,
            "读回 5:总条数不变(project)",
        ),
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("COUNT(project) 第二次查询成功");
        }
    }
    match count_table(&verify2, "import_ledger").await {
        Ok(n) => l.check(n == 2, format!("读回 5:导入流水多一行(期望 2,实得 {n})")),
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("COUNT(import_ledger) 查询成功");
        }
    }
    verify2.close().await;

    // 读回清单 7:旧库文件的修改时刻与大小,跑前跑后逐字节相同(只读证明)。
    let fp_final = fingerprint(&legacy_good).unwrap_or_default();
    l.check(
        fp_final == fp_before,
        format!("读回 7:旧库(good)文件指纹跑前跑后相同(跑前 {fp_before},跑后 {fp_final})"),
    );

    // ═══════════════ dirty 库:拒绝清单非空 → 一条都不写 ═══════════════
    println!("\n[dirty 库 · 重复活编号 → 拒绝清单非空]");
    let legacy_dirty = fresh_db_path("legacy-dirty");
    let dirty_pool = match create_legacy_db(&legacy_dirty).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("造合成旧库(dirty)成功");
            return finish(l);
        }
    };
    if let Err(e) = seed_dirty(&dirty_pool).await {
        eprintln!("ASSERT FAILED: {e}");
        l.fail("造合成旧库(dirty)种子数据成功");
        return finish(l);
    }
    dirty_pool.close().await;

    let dirty_fp_before = fingerprint(&legacy_dirty).unwrap_or_default();

    // 拒绝清单非空 → 一条都不写(design §2.5)。Important-1:经
    // `run_real_core` 同一份产品代码——这条门本身在 `run_real_core` 里
    // (`if !plan.rejected.is_empty()`),不是这里另写一份判断去绕过它。
    let new_dirty = fresh_db_path("new-dirty");
    let plan_dirty = match run_real_core(&legacy_dirty, &new_dirty, true).await {
        RunOutcome::RejectedNoWrite { plan } => plan,
        RunOutcome::Failed(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("run_real_core(dirty, confirm) 返回 RejectedNoWrite(而不是 Failed)");
            return finish(l);
        }
        other => {
            l.fail(format!(
                "run_real_core(dirty, confirm) 应该返回 RejectedNoWrite,实际:{}",
                outcome_label(&other)
            ));
            return finish(l);
        }
    };
    l.check(
        !plan_dirty.rejected.is_empty(),
        "dirty 库:重复的活编号被正确识别进拒绝清单",
    );
    println!("IMPORT_REJECTED_NO_WRITE");

    // 读回 8(比设计稿原文更硬的形态,§2.7 手工核验实测的同款结论):门 2
    // 在 `run_real_core` 里挡在「开新库」之前,拒绝清单非空时 `new_dirty`
    // 这个文件本身根本不会被创建——不是「建了表但零行」,是文件都不存
    // 在,全局零写入。
    l.check(
        !new_dirty.exists(),
        format!(
            "读回 8:拒绝清单非空 → 新库文件根本没被创建({})",
            new_dirty.display()
        ),
    );

    let dirty_fp_after = fingerprint(&legacy_dirty).unwrap_or_default();
    l.check(
        dirty_fp_after == dirty_fp_before,
        "dirty 库:旧库文件指纹跑前跑后相同",
    );

    finish(l)
}

async fn check_observation_triggers(new_db: &Path, plan: &ImportPlan, l: &mut Ledger) {
    let Some(sample) = plan.observations.first() else {
        l.fail("触发器检查:计划里至少要有一条观测");
        return;
    };
    let pool = match SqlitePool::connect_with(SqliteConnectOptions::new().filename(new_db)).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: {e}");
            l.fail("触发器检查:打开新库连接成功");
            return;
        }
    };
    let update_result = sqlx::query("UPDATE observation SET raw_value = 'tampered' WHERE id = ?")
        .bind(sample.id.uuid().to_string())
        .execute(&pool)
        .await;
    l.check(
        update_result.is_err(),
        "读回 6:直接 UPDATE observation 被数据库触发器拒绝(trg_observation_no_update)",
    );
    let delete_result = sqlx::query("DELETE FROM observation WHERE id = ?")
        .bind(sample.id.uuid().to_string())
        .execute(&pool)
        .await;
    l.check(
        delete_result.is_err(),
        "读回 6:直接 DELETE observation 被数据库触发器拒绝(trg_observation_no_delete)",
    );
    pool.close().await;
}

fn finish(l: Ledger) -> ExitCode {
    println!(
        "\n断言 {} 条,{}",
        l.count,
        if l.ok { "全过" } else { "有失败" }
    );
    if l.ok {
        println!("IMPORT_LEGACY_OK");
        ExitCode::SUCCESS
    } else {
        println!("IMPORT_LEGACY_FAILED");
        ExitCode::FAILURE
    }
}
