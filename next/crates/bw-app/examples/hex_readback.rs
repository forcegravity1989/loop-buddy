//! `hex_readback` — 切片五行为验收指挥器(design-s5-hexpanel.md §7.1)。
//!
//! 不开界面,直接调用 [`bw_app::App::hex_view`]/[`bw_app::App::attention_view`]
//! ——界面(切片五E,本片不建)将来真正会用的同一段代码——造一批真数据
//! (真 git 工作区 + 真 `.bw/metrics.toml` + 真项目/活/运行/观测/交棒),
//! 逐段读回断言,五项待人处理投影正反各验一次(造出来 → 消掉 → 行自然
//! 消失),**真进程重启**前后同一份查询逐字节比对,store 层真实执行的查
//! 询结果原样打印供人复核。
//!
//! 八段:
//!
//! 1. 指标正本同步(北极星也是一行,读一份真实 `.bw/metrics.toml`)
//! 2. 观测只追加 + 手填戴徽
//! 3. 信号只能推导(读时现算,零缓存;含「零观测不是 Green」的反证)
//! 4. 杀进程重开,数字一致(裁决 4 的新验收形态:**真子进程**重启前后,
//!    同一份组装用例产出逐字段比对——见评审 task-s5b-review.md Important-1
//!    修复轮:本节用 `std::env::current_exe()` re-exec 出一个全新操作系统进
//!    程跑「后半场」,不是同进程内换一个变量名假装重启)
//! 5. 六段逐段读回(含 `RunSnapshot` 三个数与独立 SQL 现查的等式核验,评审
//!    Important-2 修复轮)
//! 6. 完成永远人点(显式转移 + 合法转移表拒绝 + 源码扫描断言编排层没有第
//!    二条写「已完成」的路径,评审 Important-3 修复轮)
//! 7. 待人处理:五项正反各验一次(打印区改成直接打真实调用的返回行,不
//!    再抄一份可能分叉的 SQL 文本,评审 Minor-1 修复轮)
//! 8. 能点『开工』的活(next 切片七-2 修,评审 `task-s7b-review.md`
//!    Important-3:待办池/待办全部正例,进行中+无活运行正例,进行中+有
//!    活运行反例,评审中反例——独立小夹具,不搅进 §5/§6/§7 共用的那份)
//!
//! **不重复的一段**:「附 · 存量库迁移双守卫」本档不再重复一遍——`bw-store`
//! `examples/store_guards.rs` 第 14 节与 `bw-app` `examples/run_races.rs`
//! 「附」都已经真实覆盖过这条守卫,本片(五C/五D)没有新增任何列迁移,
//! 再抄一份只是同一件事的第三份复制,不产生新的验证价值。
//!
//! 跑法:`cd next && cargo run -p bw-app --example hex_readback`
//! 退出码 0 且末行 `HEX_READBACK_OK` = 全部断言通过。
//! 数据库/工作区跑完不删,路径打印出来,人可以自己 `sqlite3`/`git` 复核。
//!
//! **隐藏子命令 `--restart-probe`**:第 4 节「真进程重启」的后半场——父进
//! 程 `std::env::current_exe()` re-exec 出的子进程会带这个参数启动,走
//! [`run_restart_probe`] 而不是正常的七段流程。不是给人手跑的入口,人手
//! 跑就用上面那条不带参数的命令。

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use bw_app::run::{RunManager, RunManagerConfig, RunSnapshot};
use bw_app::view::hex::{LoopInputs, WorkspaceEvidenceState};
use bw_app::{App, Command, Event};
use bw_connector::ConnectorRegistry;
use bw_core::{IssueId, IssueStatus, MetricId, ProjectId, RunId, RunState, StageKind};
use bw_store::{
    HandoffStore, IssueStore, MetricStore, NewIssue, NewObservation, NewProject, NewRun,
    ObservationStore, RunEndKind, RunKind, RunStore, SqliteStore,
};
use bw_workspace::git_support::{commit_initial, git_in};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;
use uuid::Uuid;

const DB_NAME_PREFIX: &str = "bw-hex-";
const WS_DIR_PREFIX: &str = "bw-hex-ws-";
/// 隐藏子命令——[`main`] 顶部检测到 `argv[1]` 等于这个字符串就转去
/// [`run_restart_probe`],不走正常的七段流程。见文件头文档「隐藏子命令」
/// 一节。
const RESTART_PROBE_ARG: &str = "--restart-probe";

fn now_i64() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

fn fresh_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("{DB_NAME_PREFIX}{}.db", Uuid::new_v4()))
}

fn fresh_workspace_dir() -> PathBuf {
    std::env::temp_dir().join(format!("{WS_DIR_PREFIX}{}", Uuid::new_v4()))
}

/// 清场——同 `store_guards.rs`/`run_races.rs`/`provision_readback.rs` 的既
/// 有先例:uuid v4 命名,PID 会被系统复用,uuid 不会;清掉临时目录里所有
/// **更早**的同前缀残留(库文件与工作区目录各自一份前缀),下一次跑不会
/// 意外读到任何一次旧跑的残留。
fn clear_stale(current_db: &Path, current_ws: &Path) {
    if current_db.exists() {
        let _ = std::fs::remove_file(current_db);
    }
    let dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let current_db_name = current_db.file_name().map(|n| n.to_os_string());
    let current_ws_name = current_ws.file_name().map(|n| n.to_os_string());
    for entry in entries.flatten() {
        let name = entry.file_name();
        if current_db_name.as_deref() == Some(name.as_os_str())
            || current_ws_name.as_deref() == Some(name.as_os_str())
        {
            continue;
        }
        let name_str = name.to_string_lossy();
        if name_str.starts_with(DB_NAME_PREFIX) {
            let _ = std::fs::remove_file(entry.path());
        } else if name_str.starts_with(WS_DIR_PREFIX) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

// ─────────────────────────── 断言账本(同 run_races.rs 既有先例)───────────────────────────

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
    fn merge(&mut self, other: Ledger) {
        self.ok &= other.ok;
        self.count += other.count;
    }
}

fn format_row(row: &sqlx::sqlite::SqliteRow) -> String {
    use sqlx::{Column, ValueRef};
    row.columns()
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let name = col.name();
            let is_null = row.try_get_raw(i).map(|v| v.is_null()).unwrap_or(false);
            let value = if is_null {
                "NULL".to_string()
            } else if let Ok(v) = row.try_get::<i64, _>(i) {
                v.to_string()
            } else if let Ok(v) = row.try_get::<f64, _>(i) {
                v.to_string()
            } else if let Ok(v) = row.try_get::<String, _>(i) {
                v
            } else {
                "<unreadable>".to_string()
            };
            format!("{name}={value}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 独立只读连接原样跑一条 SQL、打印结果、把「读回成功」记一条断言——同
/// `run_races.rs` `section_sql_readback` 的既有先例:每条查询都真的执行、
/// 每一行都原样打出来,人可以自己复制这条 SQL 去 `sqlite3` 复核。
async fn print_and_run(
    pool: &SqlitePool,
    label: &str,
    sql: &str,
    l: &mut Ledger,
) -> Vec<sqlx::sqlite::SqliteRow> {
    println!("  [{label}] 执行 SQL: {sql}");
    match sqlx::query(sql).fetch_all(pool).await {
        Ok(rows) => {
            if rows.is_empty() {
                println!("    → (0 行)");
            } else {
                for row in &rows {
                    println!("    → {}", format_row(row));
                }
            }
            l.check(
                true,
                format!("[{label}] SQL 读回成功({} 行,原文见上)", rows.len()),
            );
            rows
        }
        Err(e) => {
            l.fail(format!("[{label}] 读回失败:{e}"));
            Vec::new()
        }
    }
}

// ─────────────────────────── 造真实工作区 + 正本 ───────────────────────────

/// 完整版 `.bw/metrics.toml`(1 北极星 + 1 滞后 + 1 引领)。
fn metrics_toml_full() -> &'static str {
    "schema_version = 1\n\n\
     [north_star]\n\
     name = \"每周真实交付的活数\"\n\
     def = \"hex_readback 指挥器自造的北极星,验证三层同构\"\n\
     collect = { kind = \"manual\", query = \"\" }\n\n\
     [[lagging]]\n\
     name = \"滞后指标·周吞吐\"\n\
     def = \"过去一周结算的活数\"\n\
     target = \"≥3\"\n\
     collect = { kind = \"manual\", query = \"\" }\n\n\
     [[leading]]\n\
     name = \"引领指标·评审周转\"\n\
     def = \"评审中平均停留天数(越小越好)\"\n\
     target = \"≤2\"\n\
     collect = { kind = \"script\", query = \"\" }\n"
}

/// 删掉 `[north_star]` 一节的版本(§5「北极星尚未定稿」灰卡那一步复用)。
fn metrics_toml_no_north_star() -> &'static str {
    "schema_version = 1\n\n\
     [[lagging]]\n\
     name = \"滞后指标·周吞吐\"\n\
     def = \"过去一周结算的活数\"\n\
     target = \"≥3\"\n\
     collect = { kind = \"manual\", query = \"\" }\n\n\
     [[leading]]\n\
     name = \"引领指标·评审周转\"\n\
     def = \"评审中平均停留天数(越小越好)\"\n\
     target = \"≤2\"\n\
     collect = { kind = \"script\", query = \"\" }\n"
}

fn write_metrics_toml(ws: &Path, content: &str) -> Result<(), String> {
    let bw_dir = ws.join(".bw");
    std::fs::create_dir_all(&bw_dir).map_err(|e| format!("建 .bw 目录失败:{e}"))?;
    std::fs::write(bw_dir.join("metrics.toml"), content)
        .map_err(|e| format!("写 metrics.toml 失败:{e}"))
}

/// 真造一个 git 工作区:`git init` + 真 `.bw/metrics.toml` + 一份已提交的
/// `docs/` 文档(供交付证据段③ `docs_files`/`recent_subjects` 有真数据)+
/// 一个未提交的文件(供 `dirty_paths` > 0)。
async fn setup_workspace(ws: &Path) -> Result<(), String> {
    std::fs::create_dir_all(ws).map_err(|e| format!("建工作区目录失败:{e}"))?;
    git_in(ws, &["init", "-q"])
        .await
        .map_err(|e| format!("git init 失败:{e}"))?;
    write_metrics_toml(ws, metrics_toml_full())?;
    let docs_dir = ws.join("docs");
    std::fs::create_dir_all(&docs_dir).map_err(|e| format!("建 docs 目录失败:{e}"))?;
    std::fs::write(
        docs_dir.join("hex-readback-fixture.md"),
        "# hex_readback 夹具\n\n本文档由 hex_readback 指挥器真实写入,供交付证据段③读回。\n",
    )
    .map_err(|e| format!("写 docs 文档失败:{e}"))?;
    commit_initial(
        ws,
        "hex_readback 示例工作区",
        "本目录由 next 切片五D 的 hex_readback 指挥器真实创建,带真 .bw/metrics.toml 与一份真 docs/ 文档。",
    )
    .await
    .map_err(|e| format!("首提交失败:{e}"))?;
    // 未提交的一个改动——`dirty_paths` 因此 > 0,交付证据段③的「工作区此
    // 刻真状态」不是一个永远干净的假摆设。
    std::fs::write(
        ws.join("SCRATCH.md"),
        "指挥器留下的未提交改动,供 dirty_paths 读回非零。\n",
    )
    .map_err(|e| format!("写未提交文件失败:{e}"))?;
    Ok(())
}

// ─────────────────────────── 造数:活 + 阶段归类 ───────────────────────────

struct Issues {
    a_build: IssueId,     // 归类到「构建」阶段,示范五角色卡的活数
    b_review: IssueId,    // 评审中——待人处理②的正反主角
    c_stalled: IssueId,   // 停在原地的失败运行——待人处理③的正反主角
    d_unsettled: IssueId, // 遗留未结账的运行——待人处理①的正反主角
}

async fn create_issue(
    store: &SqliteStore,
    project_id: ProjectId,
    number: i64,
    title: &str,
) -> Result<IssueId, String> {
    let id = IssueId::new();
    store
        .create_issue(NewIssue {
            id,
            project_id,
            number,
            title: title.to_string(),
        })
        .await
        .map_err(|e| format!("create_issue({title:?}) 失败:{e}"))?;
    Ok(id)
}

/// `issue.stage` 的生产写入方是 `Command::SetIssueStage`(切片五E 主控补
/// 充要求,plan/23 §10 第 16 条清偿,落地见 `bw-app/src/cmd/issue.rs::
/// set_stage`)。**评审 task-s5c-review.md Important-2 修复**:此前这里
/// 绕过 store 层另开一条独立连接直接 `UPDATE issue SET stage = ?`(同
/// design §7.4 降级口径的既有标注惯例),这条新写入路径因此在常绿门禁
/// 里没有任何行使者,唯一真跑它的是不进 CI 的 `seed_shell_demo` 造数脚
/// 本——而这份指挥器自己顶部文档与 `bw-app/src/lib.rs` 顶部文档都写着
/// 「`hex_readback` 指挥器同样经这条总线造数据,不绕开它直接调
/// `cmd::*` 私自造一条平行路径」,实现却在这一处食言。现在改经
/// `App::dispatch(Command::SetIssueStage{..})`——让这条命令在
/// `hex_readback` 这个进 CI 的常绿指挥器里有真实行使者;写完立刻独立读
/// 回一次,断言库里的 `stage` 与命令参数一致,不是「命令返回 Ok 就当写
/// 成功」。
///
/// 突变自证:把 `bw-app/src/cmd/issue.rs::set_stage` 里的
/// `store.set_issue_stage(..)` 调用删掉或改错,这里的读回断言立刻报错、
/// `bootstrap_issues` 立刻 `bail!`。
async fn set_issue_stage_via_command(
    app: &App,
    issue: IssueId,
    stage: StageKind,
) -> Result<(), String> {
    let event = app
        .dispatch(Command::SetIssueStage {
            issue,
            stage: Some(stage),
        })
        .await
        .map_err(|e| format!("Command::SetIssueStage({issue:?}, {stage:?}) 失败:{e}"))?;
    if !matches!(event, Event::IssueStageSet) {
        return Err(format!(
            "Command::SetIssueStage 应该回 Event::IssueStageSet,实得 {event:?}"
        ));
    }
    let row = app
        .store
        .get_issue(issue)
        .await
        .map_err(|e| format!("命令写入后读回 get_issue({issue:?}) 失败:{e}"))?
        .ok_or_else(|| format!("命令写入后读回 get_issue({issue:?}) 应该存在,实得 None"))?;
    if row.stage != Some(stage) {
        return Err(format!(
            "命令写入后读回 issue.stage 应该等于命令参数 {stage:?},实得 {:?}",
            row.stage
        ));
    }
    println!(
        "  ✓ Command::SetIssueStage 写入 issue={issue:?} stage={stage:?}——命令写入后读回 stage 与命令参数一致"
    );
    Ok(())
}

async fn bootstrap_issues(app: &App, project_id: ProjectId) -> Result<Issues, String> {
    let a = create_issue(&app.store, project_id, 1, "归类到构建阶段的活").await?;
    let b = create_issue(&app.store, project_id, 2, "评审中等你点的活").await?;
    let c = create_issue(&app.store, project_id, 3, "停在原地失败的活").await?;
    let d = create_issue(&app.store, project_id, 4, "遗留未结账运行的活").await?;

    set_issue_stage_via_command(app, a, StageKind::Build).await?;
    set_issue_stage_via_command(app, c, StageKind::Optimize).await?;

    // b: backlog → in_progress → in_review(停在评审中,section 7 再点完成)。
    app.transition_issue(b, IssueStatus::InProgress)
        .await
        .map_err(|e| format!("issue b backlog→in_progress 失败:{e}"))?;
    app.transition_issue(b, IssueStatus::InReview)
        .await
        .map_err(|e| format!("issue b in_progress→in_review 失败:{e}"))?;

    Ok(Issues {
        a_build: a,
        b_review: b,
        c_stalled: c,
        d_unsettled: d,
    })
}

/// 本指挥器不用 `RunManager` 开工(所有运行都是直接插库造的既成事实,不
/// 是「人点开工」产生的)——`running` 因此恒为 0,如实标注,不冒充有真在
/// 跑的运行。
///
/// **评审 Important-2 修复**:`running`/`in_review`/`recent_failed`/
/// `unsettled` 四个数此前是这里另起一份查询平行算出来的——`RunSnapshot`
/// 那三个新字段与 `handle_snapshot` 的新查库代码因此零消费者、零验证(两
/// 份实现各自各的)。现在这四个数**直接从一次真实
/// `RunManager::snapshot` 调用取**,`LoopInputs` 是 `RunSnapshot` 的真实
/// 消费者;`exceptions`/`has_any_run` 不在 `RunSnapshot` 里(那两个字段本
/// 来就不是它的职责,design §1.2④「聚合数走快照接口,例外明细单独
/// 列」),仍然直接查库。返回值把 `RunSnapshot` 本身也带出来——
/// `section_hex_segments` 用它做「与独立 SQL 逐一相等」的等式核验
/// (`assert_snapshot_matches_independent_sql`),把两份实现钉在一起:
/// `handle_snapshot` 里任何一条查询改错,那条等式立刻红。
async fn current_loop_inputs(
    store: &SqliteStore,
    manager: &RunManager,
    project_id: ProjectId,
) -> Result<(LoopInputs, RunSnapshot), String> {
    let project = Some(project_id);
    let snap = manager
        .snapshot(project)
        .await
        .map_err(|e| format!("RunManager::snapshot 失败:{e}"))?;
    let stalled_rows = store
        .list_stalled_runs(project)
        .await
        .map_err(|e| e.to_string())?;
    let has_any_run = !store
        .list_runs(project)
        .await
        .map_err(|e| e.to_string())?
        .is_empty();
    let loop_inputs = LoopInputs {
        running: snap.running,
        in_review: snap.in_review,
        recent_failed: snap.recent_failed,
        unsettled: snap.unsettled,
        exceptions: stalled_rows,
        has_any_run,
    };
    Ok((loop_inputs, snap))
}

/// 评审 Important-2 的核验半场:独立开一条只读连接,拿三条原始 SQL
/// `COUNT(*)` 现查一遍 `in_review`/`recent_failed`/`unsettled`,与
/// [`current_loop_inputs`] 传回来的那份 `RunSnapshot`(经一次真实
/// `RunManager::snapshot` 调用得到,不是这里现造的)逐一比对。这条独立
/// SQL **不复用** `bw-store` 里 `handle_snapshot` 调用的那几个 store 方
/// 法——用的是指挥器自己另写的 `COUNT(*)`,两边各自独立到库文件,一边改
/// 错另一边不会跟着错。突变自证:把 `bw-app/src/run/manager.rs`
/// `handle_snapshot` 里任意一条查询改错(比如状态判断打错、`project_id`
/// 过滤漏绑),这三条等式里至少一条立刻红。
async fn assert_snapshot_matches_independent_sql(
    db_path: &str,
    project_id: ProjectId,
    snap: &RunSnapshot,
    l: &mut Ledger,
) {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            l.fail(format!("独立只读连接打开数据库失败:{e}"));
            return;
        }
    };
    let pid = project_id.uuid().to_string();
    let checks: [(&str, &str, usize); 3] = [
        (
            "in_review",
            "SELECT COUNT(*) AS n FROM issue WHERE project_id = ? AND status = 'in_review'",
            snap.in_review,
        ),
        (
            "recent_failed",
            "SELECT COUNT(*) AS n FROM run WHERE project_id = ? AND state = 'failed'",
            snap.recent_failed,
        ),
        (
            "unsettled",
            "SELECT COUNT(*) AS n FROM run WHERE project_id = ? AND ended_at IS NOT NULL \
             AND settled_at IS NULL",
            snap.unsettled,
        ),
    ];
    for (label, sql, from_snapshot) in checks {
        match sqlx::query(sql).bind(&pid).fetch_one(&pool).await {
            Ok(row) => {
                let n: i64 = row.get("n");
                l.check(
                    n == from_snapshot as i64,
                    format!(
                        "RunSnapshot.{label} 与独立 SQL COUNT(*) 逐一相等(RunSnapshot={from_snapshot} \
                         独立 SQL={n})——`handle_snapshot` 与这条独立查询各自到库,不共用同一条实现"
                    ),
                );
            }
            Err(e) => l.fail(format!("独立 SQL 读回 {label} 失败:{e}")),
        }
    }
}

// ─────────────────────────── 1 · 指标正本同步 ───────────────────────────

struct MetricIds {
    north_star: MetricId,
    lagging: MetricId,
    leading: MetricId,
    /// 界面手建(`origin='manual'`)的第四条指标,专门用来演示待人处理④a
    /// 「从没采过 → 出现;填一条 → 消失」的正反两步——**特意不用北极星
    /// 来演示这一步**:北极星在 §5 末尾会被拿去演示「仓里删掉 `[north_star]`
    /// 一节 → 自动停用 → 灰卡」,停用之后它会退出④a 的候选范围(停用指
    /// 标不算「没数据的活指标」),两件事用同一条指标做会互相绊住,拆成
    /// 两条各自干净。
    manual: MetricId,
}

async fn section_metrics_sync(
    app: &App,
    project_id: ProjectId,
    workspace: &str,
    db_path: &str,
) -> (Ledger, Option<MetricIds>) {
    let mut l = Ledger::new();

    let event = match app
        .dispatch(Command::SyncMetricsFile {
            project: project_id,
            workspace: workspace.to_string(),
        })
        .await
    {
        Ok(e) => e,
        Err(e) => {
            l.fail(format!("Command::SyncMetricsFile 应该成功,实得错误:{e}"));
            return (l, None);
        }
    };
    let Event::MetricsSynced(report) = event else {
        l.fail("Command::SyncMetricsFile 应该回 Event::MetricsSynced");
        return (l, None);
    };
    l.check(
        report.inserted.len() == 3
            && report.updated.is_empty()
            && report.archived.is_empty()
            && report.restored.is_empty(),
        format!(
            "首次同步:inserted=3(1 北极星+1 滞后+1 引领)、其余三类为空(实得 inserted={} updated={} \
             archived={} restored={})",
            report.inserted.len(),
            report.updated.len(),
            report.archived.len(),
            report.restored.len()
        ),
    );

    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            l.fail(format!("独立只读连接打开数据库失败:{e}"));
            return (l, None);
        }
    };
    let rows = print_and_run(
        &pool,
        "① metric 表读回",
        "SELECT tier, name, target_raw, collect_kind, origin FROM metric ORDER BY tier, name",
        &mut l,
    )
    .await;
    l.check(
        rows.len() == 3,
        format!("metric 表恰好 3 行,实得 {}", rows.len()),
    );

    let project_columns = print_and_run(
        &pool,
        "② project 表结构(核验北极星不在这张表上)",
        "PRAGMA table_info(project)",
        &mut l,
    )
    .await;
    let has_north_star_column_on_project = project_columns.iter().any(|r| {
        let name: String = r.get("name");
        name.to_lowercase().contains("north")
    });
    l.check(
        !has_north_star_column_on_project,
        "project 表结构里没有任何名字含 north 的列——北极星只在 metric 表当一行,不是项目表上的文本字段",
    );

    let metrics = match app.store.list_metrics(project_id).await {
        Ok(m) => m,
        Err(e) => {
            l.fail(format!("list_metrics 应该成功,实得错误:{e}"));
            return (l, None);
        }
    };
    let north_star = metrics
        .iter()
        .find(|m| m.tier == bw_store::MetricTier::NorthStar);
    let lagging = metrics
        .iter()
        .find(|m| m.tier == bw_store::MetricTier::Lagging);
    let leading = metrics
        .iter()
        .find(|m| m.tier == bw_store::MetricTier::Leading);
    let (Some(north_star), Some(lagging), Some(leading)) = (north_star, lagging, leading) else {
        l.fail("同步之后应该能分别读到北极星/滞后/引领各一条,实得缺失至少一条");
        return (l, None);
    };
    l.check(
        north_star.target_raw.is_empty(),
        "北极星那一行 target_raw 为空串(它是项目唯一目标,不是「朝着某个目标努力的一条指标」)",
    );

    // 突变自证:绕过用例直接插第二条北极星 → 必须撞 uq_metric_north_star。
    let dup_id = bw_core::MetricId::new();
    let dup_result = app
        .store
        .create_manual_metric(
            dup_id,
            project_id,
            bw_store::MetricTier::NorthStar,
            "绕过用例插的第二条北极星",
            "",
            "",
            "manual",
            "",
            now_i64(),
        )
        .await;
    match dup_result {
        Err(e) if e.is_unique_violation() => {
            l.check(true, "绕过同步用例直接插第二条北极星 → 撞 uq_metric_north_star,如实拒绝(is_unique_violation=true)");
        }
        Err(e) => l.fail(format!("插第二条北极星应该撞唯一索引,实得别的错误:{e}")),
        Ok(()) => l.fail(
            "插第二条北极星竟然成功了——uq_metric_north_star 没有真的钉住「一个项目至多一个北极星」",
        ),
    }

    // 界面手建一条指标(origin='manual')——待人处理④a 正反两步的专用道
    // 具(理由见 `MetricIds::manual` 文档)。
    let manual_id = bw_core::MetricId::new();
    if let Err(e) = app
        .store
        .create_manual_metric(
            manual_id,
            project_id,
            bw_store::MetricTier::Leading,
            "界面手建·待人处理④a 演示专用",
            "专门演示「从没采过 → 出现;填一条 → 消失」,不掺进正本同步的三条",
            "≥1",
            "manual",
            "",
            now_i64(),
        )
        .await
    {
        l.fail(format!("界面手建指标应该成功,实得错误:{e}"));
        return (l, None);
    }
    l.check(
        true,
        format!("界面手建一条指标(origin='manual'):{manual_id:?}"),
    );

    (
        l,
        Some(MetricIds {
            north_star: north_star.id,
            lagging: lagging.id,
            leading: leading.id,
            manual: manual_id,
        }),
    )
}

// ─────────────────────────── 2 · 观测只追加 + 手填戴徽 ───────────────────────────

const STALE_DAYS: i64 = 10; // > Cadence::Weekly(7 天)的窗口,制造「过期」

async fn section_observations(app: &App, project_id: ProjectId, metrics: &MetricIds) -> Ledger {
    let mut l = Ledger::new();

    // 手填一条:滞后指标「5」(target「≥3」→ 之后会算出 Green)。
    let event = match app
        .dispatch(Command::RecordManualObservation {
            metric: metrics.lagging,
            raw: "5".to_string(),
        })
        .await
    {
        Ok(e) => e,
        Err(e) => {
            l.fail(format!(
                "Command::RecordManualObservation(滞后)应该成功,实得错误:{e}"
            ));
            return l;
        }
    };
    l.check(
        matches!(event, Event::ObservationRecorded(_)),
        "手填一条滞后指标观测,回执 Event::ObservationRecorded",
    );

    // 脚本采一条(source='script'):引领指标「1」(target「≤2」单看数值该是
    // Green),但 ts 故意回拨到 10 天前——制造「有观测但过期」的场景
    // (§3.2④b 的正例,同时也是「零观测不是唯一能不显绿的理由」的反证素
    // 材:过期同样能压住信号)。直接调 store,不经 Command——`RecordManualObservation`
    // 命令按名字就只产 `source='manual'`,这条不是那条命令的职责。
    let stale_ts = now_i64() - STALE_DAYS * 24 * 3600;
    if let Err(e) = app
        .store
        .insert_observation(NewObservation {
            id: bw_core::ObservationId::new(),
            metric_id: metrics.leading,
            project_id,
            ts: stale_ts,
            raw_value: "1".to_string(),
            source: "script".to_string(),
            source_hint: "hex_readback 指挥器模拟脚本采集,故意回拨时刻".to_string(),
        })
        .await
    {
        l.fail(format!("脚本采集插入引领指标观测应该成功,实得错误:{e}"));
        return l;
    }
    l.check(
        true,
        "脚本采一条引领指标观测(source='script',ts 回拨 10 天)",
    );

    let obs = match app.store.list_observations(metrics.lagging).await {
        Ok(o) => o,
        Err(e) => {
            l.fail(format!("list_observations(滞后)应该成功,实得错误:{e}"));
            return l;
        }
    };
    l.check(
        obs.len() == 1 && obs[0].source == "manual",
        format!("滞后指标恰好 1 条观测,source='manual'(实得 {obs:?})"),
    );

    let obs_leading = match app.store.list_observations(metrics.leading).await {
        Ok(o) => o,
        Err(e) => {
            l.fail(format!("list_observations(引领)应该成功,实得错误:{e}"));
            return l;
        }
    };
    l.check(
        obs_leading.len() == 1 && obs_leading[0].source == "script",
        format!("引领指标恰好 1 条观测,source='script'(实得 {obs_leading:?})"),
    );
    // 「手填」徽记本身(`SourceKind::is_manual()` → `MetricCard.latest_is_manual`)
    // 走的是六段视图组装用例的产出——放在 §5(六段逐段读回)一起断言,验的
    // 是界面真正会用的那个字段,不在这里另调一份内部私有函数搭一条平行
    // 验证路径。

    l
}

// ─────────────────────────── 3 · 信号只能推导 ───────────────────────────

async fn section_signal_derive_only(
    app: &App,
    manager: &RunManager,
    db_path: &str,
    project_id: ProjectId,
    metrics: &MetricIds,
    evidence: &bw_workspace::evidence::WorkspaceEvidence,
) -> Ledger {
    let mut l = Ledger::new();

    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            l.fail(format!("独立只读连接打开数据库失败:{e}"));
            return l;
        }
    };
    for table in [
        "project",
        "issue",
        "run",
        "metric",
        "observation",
        "handoff",
    ] {
        let sql = format!("PRAGMA table_info({table})");
        let label = format!("结构核验 {table}");
        let cols = print_and_run(&pool, &label, &sql, &mut l).await;
        let has_signal_col = cols.iter().any(|r| {
            let name: String = r.get("name");
            name.eq_ignore_ascii_case("signal")
        });
        l.check(
            !has_signal_col,
            format!("{table} 表结构里没有任何一列叫 signal"),
        );
    }

    let (loop_inputs, _snap) = match current_loop_inputs(&app.store, manager, project_id).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("组装 LoopInputs 失败:{e}"));
            return l;
        }
    };
    let now = OffsetDateTime::now_utc();
    let view = match app
        .hex_view(
            project_id,
            loop_inputs,
            WorkspaceEvidenceState::Present(evidence.clone()),
            now,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("App::hex_view 应该成功,实得错误:{e}"));
            return l;
        }
    };

    // 按 `id` 找,不按 `tier`——`metrics.leading`(正本同步来的)与
    // `metrics.manual`(界面手建,§1 末尾新增)同属 `Leading` 层,`tier` 已
    // 经不是这一层唯一的一条卡,必须用身份精确定位。
    let lagging_card = view.metrics.cards.iter().find(|c| c.id == metrics.lagging);
    let leading_card = view.metrics.cards.iter().find(|c| c.id == metrics.leading);

    match lagging_card {
        Some(c) => l.check(
            c.signal == bw_core::Signal::Green,
            format!("滞后指标「5 ≥ 3」→ 现算信号 Green(实得 {:?})", c.signal),
        ),
        None => l.fail("应该能在 hex_view 里找到滞后指标卡"),
    }
    match leading_card {
        Some(c) => l.check(
            c.signal == bw_core::Signal::Amber,
            format!(
                "引领指标「1 ≤ 2」本该 Green,但观测已过期(10 天 > Cadence::Weekly 窗口)→ \
                 stale 把 Green 压到 Amber,「新鲜的绿不能掩盖一个失联的数据源」(实得 {:?})",
                c.signal
            ),
        ),
        None => l.fail("应该能在 hex_view 里找到引领指标卡"),
    }

    // 反证:北极星此刻还没有任何观测,不能是 Green——必须是 Unknown。
    match &view.north_star {
        bw_app::view::hex::NorthStarView::Defined(card) => {
            l.check(
                card.signal == bw_core::Signal::Unknown && !card.has_observation,
                format!(
                    "反证:北极星零观测,信号不是 Green,如实 Unknown(实得 signal={:?} has_observation={})",
                    card.signal, card.has_observation
                ),
            );
        }
        bw_app::view::hex::NorthStarView::Undefined => {
            l.fail("此刻北极星已经定义(§1 同步过),不该显示 Undefined 灰卡");
        }
    }

    l
}

// ─────────────────────────── 造数:运行(供①③以及 Loop/交付证据段用)───────────────────────────

/// 【hex_readback 指挥器直接写入运行表,非运行管理器产出——本档不起
/// `RunManager`,所有运行都是直接插库造的既成事实】起一条运行、立刻关
/// 门,返回运行编号。`settle` 控制要不要顺带结账——`false` 就是待人处
/// 理①「遗留运行」的造法。
async fn create_closed_run(
    store: &SqliteStore,
    project_id: ProjectId,
    issue_id: IssueId,
    state: RunState,
    end_kind: RunEndKind,
    started_at: i64,
    settle: bool,
) -> Result<RunId, String> {
    let run_id = RunId::new();
    store
        .create_run(NewRun {
            id: run_id,
            project_id,
            issue_id,
            kind: RunKind::Delivery,
            connector_name: "hex-readback-fixture".to_string(),
            req_id: Uuid::new_v4().to_string(),
            workspace: format!("/tmp/hex-readback-fixture-{issue_id:?}"),
            branch: "bw/hex-readback-fixture".to_string(),
            state: RunState::Starting,
            started_at,
        })
        .await
        .map_err(|e| format!("create_run 失败:{e}"))?;
    let at = started_at + 60;
    let closed = store
        .close_run(
            run_id,
            at,
            state,
            Some(end_kind),
            "【hex_readback 指挥器直接写入,非真实执行连接器上报】",
        )
        .await
        .map_err(|e| format!("close_run 失败:{e}"))?;
    if !closed {
        return Err("close_run 应该是第一次抵达(受影响 1 行),实得比较并置落空".to_string());
    }
    if settle {
        let settled = store
            .settle_run(run_id, at + 1)
            .await
            .map_err(|e| format!("settle_run 失败:{e}"))?;
        if !settled {
            return Err("settle_run 应该是第一次抵达,实得比较并置落空".to_string());
        }
    }
    Ok(run_id)
}

// ─────────────────────────── 4 · 杀进程重开,数字一致 ───────────────────────────

/// 裁决 4 的新验收形态:信号不落库,「重启后数字一致」因此不是「读回缓
/// 存列」,而是**同一份组装用例(`App::hex_view`/`App::attention_view`)
/// 在真进程重启前后各跑一次,结果逐字段比对**。
///
/// **评审 Important-1 修复**:此前这里是同进程内 `App::open` → `drop` →
/// 再 `App::open`——突变测试当场证实了这不够格:把「重启」那一步整个跳
/// 过(`let app2 = app1;`,连接都不重开),102 条断言仍然全绿,说明这条
/// 断言证明的只是「第二次读回真的落在库文件上」,证明不了「重启过」。
/// 现在「后半场」真的用 `std::env::current_exe()` re-exec 出一个**全新操
/// 作系统进程**(带隐藏子命令 [`RESTART_PROBE_ARG`],落地在
/// [`run_restart_probe`]):那个进程自己独立打开 `App`/`RunManager`、独立
/// 重新采工作区证据、独立重新组装六段总控视图与待人处理投影,把结果打
/// 印成可比对的文本行传回父进程——这才抓得住「进程级缓存」这一类东西
/// (库里已经没有信号列可写,唯一还需要防的正是这一类,同进程内换一个
/// 变量名抓不到)。
///
/// `now` 由父进程生成、原样传给子进程(不是子进程自己现取)——六段视图
/// ④b「过期」判断依赖时钟,父子两次组装若各自取不同的 `now` 会引入一个
/// 真实存在但与「重启是否保持一致」无关的变量,固定住它才是干净的对
/// 照。`loop_inputs`/`workspace_evidence` 不这样处理——两者本来就是「当
/// 下现查/现采」的东西,父子两侧各自独立重新查一遍(数据库与工作区在
/// 这两次调用之间都没有别的写入,理应查出同一个结果);本节要单独证的
/// 是「同一批已经持久化的数据,换一个全新进程重新接上,组装出来的东西
/// 必须一模一样」,不是「把这两样东西原样搬过去」。
///
/// **guard-no-direct-process.sh 覆盖面核实**:该守卫的 `TARGETS` 只列了
/// `bw-core/src`/`bw-app/src`(脚本注释原文),不含任何 `examples/` 目
/// 录——这份指挥器本身早就在用 `std::process::Command::new("git")`(见
/// §6「独立复核」一节)且门禁一直绿,这里 re-exec 自己走的是同一条已核
/// 实过的豁免,不是新开一个口子。
async fn section_restart_consistency(db_path: &str, ws_str: &str, project_id: ProjectId) -> Ledger {
    let mut l = Ledger::new();

    let app1 = match App::open(db_path).await {
        Ok(a) => a,
        Err(e) => {
            l.fail(format!("第一次 App::open 应该成功,实得错误:{e}"));
            return l;
        }
    };
    let registry1 = Arc::new(ConnectorRegistry::default());
    let manager1 =
        match RunManager::open(Path::new(db_path), registry1, RunManagerConfig::default()).await {
            Ok(m) => m,
            Err(e) => {
                l.fail(format!("第一次 RunManager::open 应该成功,实得错误:{e}"));
                return l;
            }
        };
    let (loop_inputs, _snap) = match current_loop_inputs(&app1.store, &manager1, project_id).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("组装 LoopInputs 失败:{e}"));
            return l;
        }
    };
    let evidence1 = match bw_workspace::evidence::collect(ws_str).await {
        Ok(e) => e,
        Err(e) => {
            l.fail(format!("重启前采集工作区证据失败:{e}"));
            return l;
        }
    };
    let now = OffsetDateTime::now_utc();

    let hex_before = match app1
        .hex_view(
            project_id,
            loop_inputs,
            WorkspaceEvidenceState::Present(evidence1),
            now,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("重启前 App::hex_view 应该成功,实得错误:{e}"));
            return l;
        }
    };
    let attention_before = match app1.attention_view(Some(project_id), now).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("重启前 App::attention_view 应该成功,实得错误:{e}"));
            return l;
        }
    };
    drop(app1);
    drop(manager1); // 真丢:这一侧的 App/RunManager 两条存储连接都被丢弃。

    // 真进程重启:re-exec 出一个全新操作系统进程跑「后半场」——不是同进
    // 程内的变量重绑定。
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            l.fail(format!("std::env::current_exe() 失败:{e}"));
            return l;
        }
    };
    let output = match std::process::Command::new(&exe)
        .arg(RESTART_PROBE_ARG)
        .arg(db_path)
        .arg(project_id.uuid().to_string())
        .arg(ws_str)
        .arg(now.unix_timestamp().to_string())
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            l.fail(format!("re-exec 子进程启动失败:{e}"));
            return l;
        }
    };
    if !output.status.success() {
        l.fail(format!(
            "--restart-probe 子进程应该成功退出,实得 status={:?} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
        return l;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let hex_after_text = stdout
        .lines()
        .find_map(|line| line.strip_prefix("HEX_VIEW_DEBUG:"));
    let attention_after_text = stdout
        .lines()
        .find_map(|line| line.strip_prefix("ATTENTION_VIEW_DEBUG:"));
    let child_pid_text = stdout
        .lines()
        .find_map(|line| line.strip_prefix("CHILD_PID:"));
    let (Some(hex_after_text), Some(attention_after_text), Some(child_pid_text)) =
        (hex_after_text, attention_after_text, child_pid_text)
    else {
        l.fail(format!(
            "--restart-probe 子进程输出里没找到 HEX_VIEW_DEBUG/ATTENTION_VIEW_DEBUG/CHILD_PID 行,原文子进程 stdout:\n{stdout}"
        ));
        return l;
    };

    // 进程身份本身的证据(不是逐字段比对能替代的):子进程的 PID 必须与
    // 本进程的 PID 不同——这是「真的换了一个操作系统进程」唯一硬事实层
    // 面的证明。突变自证:把上面 `std::process::Command::new(&exe)` 那一
    // 段临时换成直接在本进程内调 `run_restart_probe` 的逻辑(不真的
    // re-exec),`child_pid` 会变成本进程自己的 PID,这条断言必红——用这
    // 条堵住「评审 Important-1 揪出的『跳过重启这一步仍然全绿』」同一类
    // 漏洞在这份新实现里复发。
    match child_pid_text.parse::<u32>() {
        Ok(child_pid) => {
            let my_pid = std::process::id();
            l.check(
                child_pid != my_pid,
                format!(
                    "子进程 PID({child_pid})与本进程 PID({my_pid})不同——真的是一个独立操作系统进程,\
                     不是同进程内换一个变量名假装重启"
                ),
            );
        }
        Err(e) => l.fail(format!("CHILD_PID 解析失败:{e}(原文 {child_pid_text:?})")),
    }

    let hex_before_text = format!("{hex_before:?}");
    let attention_before_text = format!("{attention_before:?}");

    l.check(
        hex_before_text == hex_after_text,
        "六段总控视图:真进程重启(子进程独立打开 App/RunManager 重新组装)前后逐字段一致",
    );
    l.check(
        attention_before_text == attention_after_text,
        "待人处理投影:真进程重启前后逐字段一致",
    );

    // 突变自证(运行期负对照,不靠临时改源码):故意改一份文本,证明字
    // 符串相等比较不是恒真——上面两条「相等」真的会在不等时报不等,不是
    // 比较本身失效了才恒真。
    let mutated = format!("{hex_before_text}·被指挥器故意改动,不该与原值相等");
    l.check(
        mutated != hex_after_text,
        "反证:故意改一份文本之后,字符串比较判它们不相等——上面两条「相等」的断言不是矮化成永远为真",
    );

    l
}

// ─────────────────────────── 5 · 六段逐段读回 ───────────────────────────

#[allow(clippy::too_many_arguments)]
async fn section_hex_segments(
    app: &App,
    manager: &RunManager,
    db_path: &str,
    project_id: ProjectId,
    metrics: &MetricIds,
    issues: &Issues,
    workspace: &str,
    evidence: &bw_workspace::evidence::WorkspaceEvidence,
    c_run: RunId,
    d_run: RunId,
) -> Ledger {
    let mut l = Ledger::new();

    // `issues.a_build` 的行本身(不只是聚合计数)再读回一次:`bootstrap_issues`
    // 里 `set_issue_stage_via_command` 那次经 `Command::SetIssueStage` 的写
    // 入真的落进了 `issue.stage`(那边已经做过一次即时读回,这里是六段视
    // 图组装用例读到的同一份数据,独立确认查询侧也没有碰巧算对)。
    match app.store.get_issue(issues.a_build).await {
        Ok(Some(row)) => l.check(
            row.stage == Some(StageKind::Build),
            format!("issue a 的行读回 stage=Some(Build)(实得 {:?})", row.stage),
        ),
        Ok(None) => l.fail("issue a 应该存在"),
        Err(e) => l.fail(format!("get_issue(issue a) 失败:{e}")),
    }

    let (loop_inputs, snap) = match current_loop_inputs(&app.store, manager, project_id).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("组装 LoopInputs 失败:{e}"));
            return l;
        }
    };
    assert_snapshot_matches_independent_sql(db_path, project_id, &snap, &mut l).await;
    let now = OffsetDateTime::now_utc();
    let view = match app
        .hex_view(
            project_id,
            loop_inputs,
            WorkspaceEvidenceState::Present(evidence.clone()),
            now,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("App::hex_view 应该成功,实得错误:{e}"));
            return l;
        }
    };

    println!("  -- ① 项目目标(北极星)--");
    match &view.north_star {
        bw_app::view::hex::NorthStarView::Defined(card) => {
            l.check(
                card.id == metrics.north_star && card.name == "每周真实交付的活数",
                format!("北极星文案与库里一致(实得 name={:?})", card.name),
            );
            l.check(
                !card.has_observation,
                "北极星此刻仍无观测(还没到§7④a 的反转步)",
            );
        }
        bw_app::view::hex::NorthStarView::Undefined => {
            l.fail("此刻北极星已经定义,不该显示「尚未定稿」灰卡");
        }
    }

    println!("  -- ② 五角色责任卡(零 SQL,静态元数据)--");
    l.check(
        view.five_roles.active_stage == Some(StageKind::Prototype),
        format!(
            "当前一棒 = Prototype(实得 {:?})",
            view.five_roles.active_stage
        ),
    );
    for card in view.five_roles.cards {
        let expect_current = card.stage == StageKind::Prototype;
        l.check(
            card.is_current == expect_current,
            format!(
                "{:?} 卡片 is_current={}(期望 {})",
                card.stage, card.is_current, expect_current
            ),
        );
        // 零 SQL:静态元数据直接来自 `StageKind` 自身方法,与卡片渲染时用
        // 的是同一份,这里独立再调一次做交叉核验。
        l.check(
            !card.stage.role_short().is_empty()
                && !card.stage.methodology().is_empty()
                && !card.stage.core_question().is_empty()
                && !card.stage.dod_items().is_empty()
                && !card.stage.anti_patterns().is_empty()
                && !card.stage.cycle_rhythm().is_empty(),
            format!(
                "{:?} 的角色/方法论/核心问题/完成清单/常见的坑/节奏均非空(零数据库)",
                card.stage
            ),
        );
    }
    let build_count = view
        .five_roles
        .cards
        .iter()
        .find(|c| c.stage == StageKind::Build)
        .map(|c| c.issue_count)
        .unwrap_or(-1);
    let optimize_count = view
        .five_roles
        .cards
        .iter()
        .find(|c| c.stage == StageKind::Optimize)
        .map(|c| c.issue_count)
        .unwrap_or(-1);
    l.check(
        build_count == 1,
        format!("构建阶段活数 = 1(issue a,实得 {build_count})"),
    );
    l.check(
        optimize_count == 1,
        format!("优化阶段活数 = 1(issue c,实得 {optimize_count})"),
    );
    l.check(
        view.five_roles.unclassified_issue_count == 2,
        format!(
            "未归类活数 = 2(issue b/d,实得 {})",
            view.five_roles.unclassified_issue_count
        ),
    );

    println!("  -- ③ 引领指标(三层顺序固定,无观测那条虚线灰)--");
    l.check(
        view.metrics.cards.len() == 4,
        format!(
            "此刻共 4 张指标卡(北极星+滞后+引领+手建,实得 {})",
            view.metrics.cards.len()
        ),
    );
    let tiers: Vec<_> = view.metrics.cards.iter().map(|c| c.tier).collect();
    l.check(
        tiers.first() == Some(&bw_store::MetricTier::NorthStar),
        format!("顺序固定:第一张是北极星(实得 {tiers:?})"),
    );
    let lagging_card = view.metrics.cards.iter().find(|c| c.id == metrics.lagging);
    let leading_card = view.metrics.cards.iter().find(|c| c.id == metrics.leading);
    let manual_card = view.metrics.cards.iter().find(|c| c.id == metrics.manual);
    l.check(
        lagging_card.map(|c| c.signal) == Some(bw_core::Signal::Green),
        "滞后指标卡:信号 Green",
    );
    l.check(
        leading_card.map(|c| c.latest_is_manual) == Some(false),
        "引领指标卡:latest_is_manual = false(source='script',不戴「手填」徽记)",
    );
    l.check(
        lagging_card.map(|c| c.latest_is_manual) == Some(true),
        "滞后指标卡:latest_is_manual = true(source='manual',戴「手填」徽记)",
    );
    l.check(
        manual_card.map(|c| (c.has_observation, c.signal))
            == Some((false, bw_core::Signal::Unknown)),
        "界面手建的指标此刻无观测,虚线灰(Unknown)——§7④a 的初始状态",
    );

    println!("  -- ④ 当前 Loop(聚合数 + 只列例外)--");
    l.check(
        view.loop_segment.running == 0,
        "在跑数 = 0(本档不起 RunManager)",
    );
    l.check(
        view.loop_segment.in_review == 1,
        format!("评审中数 = 1(issue b,实得 {})", view.loop_segment.in_review),
    );
    l.check(
        view.loop_segment.recent_failed == 1,
        format!(
            "最近失败数 = 1(issue c 的失败运行,实得 {})",
            view.loop_segment.recent_failed
        ),
    );
    l.check(
        view.loop_segment.unsettled == 1,
        format!(
            "遗留未结账数 = 1(issue d 的运行,实得 {})",
            view.loop_segment.unsettled
        ),
    );
    l.check(
        view.loop_segment.has_any_run,
        "has_any_run = true(已经有真实运行)",
    );
    l.check(
        view.loop_segment.exceptions.iter().any(|r| r.id == c_run),
        "例外明细包含 issue c 的失败运行",
    );
    l.check(
        !view.loop_segment.exceptions.iter().any(|r| r.id == d_run),
        "d 的运行已结束但不是失败/遗留状态(Finished),不出现在「例外」里(它出现在遗留未结账那个数,不是这份明细)",
    );

    println!("  -- ⑤ 风险与决策(此刻还没有交棒,如实空)--");
    l.check(
        view.risk_decision.handoffs.is_empty(),
        "此刻交棒流水为空(§7⑤ 还没开始)",
    );
    l.check(
        !view.risk_decision.has_any_handoff,
        "has_any_handoff = false",
    );
    l.check(
        !bw_app::view::hex::RiskDecisionSegment::DECISION_NOTE.is_empty(),
        "决策栏留白说明非空,不拿交棒记录冒充决策记录",
    );

    println!("  -- ⑥ 交付证据(运行账 + 观测出处 + 工作区现采)--");
    l.check(
        view.evidence.runs.len() == 2,
        format!(
            "运行账恰好 2 条(issue c + issue d,实得 {})",
            view.evidence.runs.len()
        ),
    );
    l.check(
        view.evidence.observations.len() == 2,
        format!(
            "观测出处恰好 2 条(滞后手填 + 引领脚本,实得 {})",
            view.evidence.observations.len()
        ),
    );
    match &view.evidence.workspace_evidence {
        WorkspaceEvidenceState::Present(we) => {
            l.check(
                we.commit_count >= 1,
                format!("工作区证据:commit_count >= 1(实得 {})", we.commit_count),
            );
            l.check(
                we.dirty_paths >= 1,
                format!(
                    "工作区证据:dirty_paths >= 1(SCRATCH.md 未提交,实得 {})",
                    we.dirty_paths
                ),
            );
            l.check(
                we.docs_files >= 1,
                format!("工作区证据:docs_files >= 1(实得 {})", we.docs_files),
            );
            // 独立复核:不信任 `bw_workspace::evidence::collect` 自己,另起
            // 一个同步子进程直接跑 `git rev-list --count HEAD`,两边必须一致
            // ——「在工作区跑同一条 git 命令」就是这一栏的复核方式(裁决 6)。
            let independent = std::process::Command::new("git")
                .current_dir(workspace)
                .args(["rev-list", "--count", "HEAD"])
                .output();
            match independent {
                Ok(out) if out.status.success() => {
                    let n: u32 = String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .parse()
                        .unwrap_or(u32::MAX);
                    l.check(
                        n == we.commit_count,
                        format!(
                            "独立复核:`git rev-list --count HEAD` 在工作区里跑出来的数({n})与工作区证据里的 commit_count 一致({})",
                            we.commit_count
                        ),
                    );
                }
                _ => l.fail("独立跑 git rev-list --count HEAD 应该成功"),
            }
        }
        WorkspaceEvidenceState::NotConfigured => l.fail("此刻工作区已配置,不该是 NotConfigured"),
        WorkspaceEvidenceState::CollectFailed(err) => l.fail(format!(
            "此刻工作区已配置且应该采证成功,不该 CollectFailed:{err}"
        )),
    }

    println!("  -- 北极星尚未定稿:仓里删掉 [north_star] 一节再同步 → 灰卡 --");
    if let Err(e) = write_metrics_toml(Path::new(workspace), metrics_toml_no_north_star()) {
        l.fail(format!("重写 metrics.toml(去掉 north_star)失败:{e}"));
        return l;
    }
    let resync = app
        .dispatch(Command::SyncMetricsFile {
            project: project_id,
            workspace: workspace.to_string(),
        })
        .await;
    match resync {
        Ok(Event::MetricsSynced(report)) => {
            l.check(
                report.archived.contains(&metrics.north_star),
                format!(
                    "重同步(缺 [north_star])→ 北极星那一行自动停用(实得 archived={:?})",
                    report.archived
                ),
            );
        }
        Ok(_) => l.fail("重同步应该回 Event::MetricsSynced"),
        Err(e) => l.fail(format!("重同步应该成功,实得错误:{e}")),
    }
    let (loop_inputs2, _snap2) = match current_loop_inputs(&app.store, manager, project_id).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("组装 LoopInputs 失败:{e}"));
            return l;
        }
    };
    match app
        .hex_view(
            project_id,
            loop_inputs2,
            WorkspaceEvidenceState::Present(evidence.clone()),
            OffsetDateTime::now_utc(),
        )
        .await
    {
        Ok(v) => l.check(
            matches!(v.north_star, bw_app::view::hex::NorthStarView::Undefined),
            "仓里没有 [north_star] 一节 → 北极星显示「尚未定稿」灰卡,绝不显绿",
        ),
        Err(e) => l.fail(format!("重同步之后 App::hex_view 应该成功,实得错误:{e}")),
    }
    // 复原,后续几节(§6/§7)仍然要用一个已定稿的北极星场景。
    if let Err(e) = write_metrics_toml(Path::new(workspace), metrics_toml_full()) {
        l.fail(format!("复原 metrics.toml 失败:{e}"));
        return l;
    }
    match app
        .dispatch(Command::SyncMetricsFile {
            project: project_id,
            workspace: workspace.to_string(),
        })
        .await
    {
        Ok(Event::MetricsSynced(report)) => l.check(
            report.restored.contains(&metrics.north_star),
            format!(
                "复原 [north_star] 再同步 → 自动恢复(实得 restored={:?})",
                report.restored
            ),
        ),
        Ok(_) => l.fail("复原同步应该回 Event::MetricsSynced"),
        Err(e) => l.fail(format!("复原同步应该成功,实得错误:{e}")),
    }

    l
}

// ─────────────────────────── 6 · 完成永远人点 ───────────────────────────

/// 评审 Important-3:design-s5-hexpanel.md §7.1 点名的「全库搜索:编排层
/// 里没有第二条写『已完成』的路径」这条断言此前没做——文件头对「附 ·
/// 存量库迁移双守卫」那一档明写了为什么不重复,同属 §7.1 的这一条却既
/// 没做也没交代。这里把「全库搜索」变成机器能查的东西,同
/// `bw-workspace` `examples/provision_readback.rs` `scan_no_delete_surface`
/// 的既有纹理(源码扫描,不是运行期调用——「这条 SQL 存不存在」本身没
/// 有运行期行为可触发)。
///
/// 扫 `bw-app/src` + `bw-store/src` 里所有 `SET status`(改 `issue.status`
/// 的 SQL)出现的地方,只认两条钦定路径:
/// - `bw_store::sqlite::mark_issue_in_progress` —— `SET status =
///   'in_progress'`,字面量硬编码,结构上写不出 `'done'`;
/// - `bw_store::sqlite::transition_issue_status` —— 参数化 `SET status =
///   ?`(`cmd/issue.rs::transition` 唯一调用点,写之前必过
///   `bw_core::IssueStatus::can_transition_to`)。
///
/// 除此之外任何一行 `SET status` 都判定为「第二条路径」。**突变自
/// 证**:临时在 `bw-store/src/sqlite.rs` 或 `bw-app/src` 任意位置加一行
/// `sqlx::query("UPDATE issue SET status = 'done' WHERE 1=1")` 之类的越权
/// 写,这一节必须变红。
fn scan_no_second_done_write_path(bw_app_src: &Path, bw_store_src: &Path) -> Vec<String> {
    const ALLOWED_SNIPPETS: &[&str] = &[
        // mark_issue_in_progress——字面量,结构上写不出 'done'。
        "SET status = 'in_progress'",
        // transition_issue_status——钦定的比较并置写路径。
        "SET status = ?, updated_at = ? WHERE id = ? AND status = ?",
    ];
    let mut findings = Vec::new();
    for src_dir in [bw_app_src, bw_store_src] {
        let mut stack = vec![src_dir.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let Ok(content) = std::fs::read_to_string(&path) else {
                    findings.push(format!("{}: 读取失败,无法扫描", path.display()));
                    continue;
                };
                for (lineno, line) in content.lines().enumerate() {
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("//") {
                        continue; // 注释/文档行不是代码——提一句「SET status」不算一条写。
                    }
                    if !line.contains("SET status") {
                        continue;
                    }
                    if ALLOWED_SNIPPETS.iter().any(|snip| line.contains(snip)) {
                        continue;
                    }
                    findings.push(format!(
                        "{}:{}: 出现一条不在钦定名单里的 `SET status` 写(钦定名单只有 \
                         mark_issue_in_progress 与 transition_issue_status 两条)——{}",
                        path.display(),
                        lineno + 1,
                        line.trim()
                    ));
                }
            }
        }
    }
    findings
}

/// 用两件**专用**的活(不碰 issue b——那是 §7②的正反主角,提前把它转到
/// Done 会让 §7 无法再演示「造→消」两步):一件走完整合法链路到
/// Done,一件在「进行中」直接尝试转「已完成」被合法转移表拒绝。
async fn section_done_is_human(app: &App, project_id: ProjectId) -> Ledger {
    let mut l = Ledger::new();

    let done_issue = match create_issue(&app.store, project_id, 5, "完成永远人点·合法链路").await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(e);
            return l;
        }
    };
    let blocked_issue = match create_issue(&app.store, project_id, 6, "完成永远人点·非法转移").await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(e);
            return l;
        }
    };

    for (label, to) in [
        ("backlog → in_progress", IssueStatus::InProgress),
        ("in_progress → in_review", IssueStatus::InReview),
    ] {
        if let Err(e) = app.transition_issue(done_issue, to).await {
            l.fail(format!("{label} 应该合法,实得:{e}"));
            return l;
        }
    }
    match app
        .dispatch(Command::TransitionIssue {
            issue: done_issue,
            to: IssueStatus::Done,
        })
        .await
    {
        Ok(Event::IssueTransitioned) => l.check(
            true,
            "InReview → Done 经 Command::TransitionIssue 显式转移成功(Done 的唯一入边)",
        ),
        Ok(_) => l.fail("应该回 Event::IssueTransitioned"),
        Err(e) => l.fail(format!("InReview → Done 应该合法,实得:{e}")),
    }

    if let Err(e) = app
        .transition_issue(blocked_issue, IssueStatus::InProgress)
        .await
    {
        l.fail(format!("backlog → in_progress 应该合法,实得:{e}"));
        return l;
    }
    match app.transition_issue(blocked_issue, IssueStatus::Done).await {
        Err(bw_app::AppError::IllegalTransition { .. }) => {
            l.check(
                true,
                "「进行中」直接转「已完成」被合法转移表拒绝(IllegalTransition)",
            );
        }
        Err(e) => l.fail(format!("应该报 IllegalTransition,实得别的错误:{e}")),
        Ok(()) => l.fail("「进行中」直接转「已完成」竟然成功了——合法转移表没有真的挡住这条边"),
    }

    println!("  -- 全库搜索:编排层里没有第二条写「已完成」的路径(design §7.1)--");
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let bw_app_src = manifest_dir.join("src");
    let bw_store_src = manifest_dir.join("../bw-store/src");
    let findings = scan_no_second_done_write_path(&bw_app_src, &bw_store_src);
    if findings.is_empty() {
        l.check(
            true,
            format!(
                "源码扫描 {} + {}:`SET status` 只出现在两条钦定路径(mark_issue_in_progress / \
                 transition_issue_status),没有第二条写「已完成」的路径",
                bw_app_src.display(),
                bw_store_src.display()
            ),
        );
    } else {
        for finding in &findings {
            l.fail(finding.clone());
        }
    }

    l
}

// ─────────────────────────── 7 · 待人处理:五项正反各验一次 ───────────────────────────

/// 评审 Minor-1:此前这里打给人复核的 Q1-Q5 是设计稿原文 SQL 的手抄版
/// (没有 project 过滤、没有次级排序键),与 `bw-store/src/sqlite.rs`
/// 真正执行的查询(`format!` 拼出来,按 `project: Option<ProjectId>` 参
/// 数化)可以分叉——突变测试实测到:store 层 SQL 改成 `WHERE 1=0` 之
/// 后,断言正确变红,但打印区照样显示旧行,照着打印的 SQL 去 `sqlite3`
/// 复核的人会被安抚。修法:不再另抄一份 SQL 字符串去单独执行,直接打印
/// `before`(下方 `App::attention_view` 的真实返回值)本身的六个字
/// 段——这就是下面全部断言用的同一份数据,打印用的和断言用的不可能再
/// 分叉。真实 SQL 见 `bw-store/src/sqlite.rs` 对应的 store 方法(下面每
/// 条打印都点了名)。
fn print_rows<T: std::fmt::Debug>(label: &str, store_method: &str, rows: &[T]) {
    println!("  [{label}] 源:App::attention_view → bw_store::{store_method}(与下方断言同一次调用的返回值)");
    if rows.is_empty() {
        println!("    → (0 行)");
    } else {
        for row in rows {
            println!("    → {row:?}");
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn section_attention_five_items(
    app: &App,
    project_id: ProjectId,
    metrics: &MetricIds,
    issues: &Issues,
    c_run: RunId,
    d_run: RunId,
) -> Ledger {
    let mut l = Ledger::new();

    let now = OffsetDateTime::now_utc();
    let before = match app.attention_view(Some(project_id), now).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("App::attention_view(造)应该成功,实得错误:{e}"));
            return l;
        }
    };

    println!("  -- 五条查询的真实返回行原样打印(供人复核)--");
    print_rows(
        "① 遗留运行",
        "RunStore::list_unsettled_runs",
        &before.unsettled_runs,
    );
    print_rows(
        "② 评审中等你点",
        "IssueStore::list_issues_by_status(_, InReview)",
        &before.in_review_issues,
    );
    print_rows(
        "③ 停在原地的运行",
        "RunStore::list_stalled_runs",
        &before.stalled_runs,
    );
    print_rows(
        "④a 从没采过",
        "MetricStore::list_metrics_without_observation",
        &before.metrics_without_observation,
    );
    print_rows(
        "④b 数据过期(节奏窗口过滤后)",
        "MetricStore::list_metric_latest_observation + cadence_window 过滤",
        &before.metrics_stale,
    );
    print_rows(
        "⑤ 当前一棒的带险交棒欠账",
        "HandoffStore::list_current_stage_risky_handoffs",
        &before.current_stage_risky_handoffs,
    );

    println!("  -- ① 遗留运行:造(已在造数阶段成立)→ 消(结算一次)--");
    l.check(
        before.unsettled_runs.iter().any(|r| r.id == d_run),
        "造:issue d 的运行关门未结账 → 出现在①",
    );
    match app.store.settle_run(d_run, now_i64()).await {
        Ok(true) => l.check(true, "结算一次(第一次抵达)"),
        Ok(false) => l.fail("结算应该是第一次抵达,实得比较并置落空"),
        Err(e) => l.fail(format!("settle_run 失败:{e}")),
    }

    println!("  -- ② 评审中等你点:造(已在造数阶段成立)→ 消(点完成)--");
    l.check(
        before
            .in_review_issues
            .iter()
            .any(|i| i.id == issues.b_review),
        "造:issue b 处在评审中 → 出现在②",
    );
    match app
        .dispatch(Command::TransitionIssue {
            issue: issues.b_review,
            to: IssueStatus::Done,
        })
        .await
    {
        Ok(Event::IssueTransitioned) => l.check(true, "点完成(InReview → Done)"),
        Ok(_) => l.fail("应该回 Event::IssueTransitioned"),
        Err(e) => l.fail(format!("InReview → Done 应该合法,实得:{e}")),
    }

    println!("  -- ③ 停在原地的运行:造(已在造数阶段成立)→ 消(重开一次,哪怕又失败)--");
    l.check(
        before.stalled_runs.iter().any(|r| r.id == c_run),
        "造:issue c 的失败运行且没有更晚的运行 → 出现在③",
    );
    let c_run2 = match create_closed_run(
        &app.store,
        project_id,
        issues.c_stalled,
        RunState::Failed,
        RunEndKind::StartFailed,
        now_i64() + 3600,
        false,
    )
    .await
    {
        Ok(id) => {
            l.check(true, format!("重开一次(哪怕又失败):新运行 {id:?}"));
            id
        }
        Err(e) => {
            l.fail(e);
            RunId::new()
        }
    };

    println!("  -- ④a 从没采过:造(已在造数阶段成立)→ 消(填一条)--");
    l.check(
        before
            .metrics_without_observation
            .iter()
            .any(|m| m.id == metrics.manual),
        "造:界面手建的指标从没采过 → 出现在④a",
    );
    match app
        .dispatch(Command::RecordManualObservation {
            metric: metrics.manual,
            raw: "1".to_string(),
        })
        .await
    {
        Ok(Event::ObservationRecorded(_)) => l.check(true, "填一条观测"),
        Ok(_) => l.fail("应该回 Event::ObservationRecorded"),
        Err(e) => l.fail(format!("RecordManualObservation 应该成功,实得错误:{e}")),
    }

    println!("  -- ④b 数据过期:造(已在造数阶段成立,§2 的引领指标)→ 消(填一条新的)--");
    l.check(
        before.metrics_stale.iter().any(|m| m.id == metrics.leading),
        "造:引领指标最新一条观测已经过期(10 天 > Weekly 窗口)→ 出现在④b",
    );
    if let Err(e) = app
        .store
        .insert_observation(NewObservation {
            id: bw_core::ObservationId::new(),
            metric_id: metrics.leading,
            project_id,
            ts: now_i64(),
            raw_value: "1".to_string(),
            source: "script".to_string(),
            source_hint: "hex_readback 指挥器:填一条新的,证明「过期」能自然消失".to_string(),
        })
        .await
    {
        l.fail(format!("填一条新观测应该成功,实得错误:{e}"));
    } else {
        l.check(true, "填一条新的观测(ts=now)");
    }

    println!("  -- ⑤ 当前一棒的带险交棒欠账:造(带险交一棒)→ 消(再交一棒,handoff 表里那条还在)--");
    let handoff1 = match app
        .dispatch(Command::HandoffStage {
            project: project_id,
            risky: true,
            note: "评审清单没勾完就交了,hex_readback 示例带险交棒".to_string(),
        })
        .await
    {
        Ok(Event::HandoffRecorded(id)) => id,
        Ok(_) => {
            l.fail("应该回 Event::HandoffRecorded");
            bw_core::HandoffId::new()
        }
        Err(e) => {
            l.fail(format!("HandoffStage(risky) 应该成功,实得错误:{e}"));
            bw_core::HandoffId::new()
        }
    };
    let after_risky_handoff = match app
        .attention_view(Some(project_id), OffsetDateTime::now_utc())
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!(
                "App::attention_view(带险交棒后)应该成功,实得错误:{e}"
            ));
            return l;
        }
    };
    l.check(
        after_risky_handoff
            .current_stage_risky_handoffs
            .iter()
            .any(|h| h.id == handoff1),
        "造:带险交一棒(交到当前一棒)→ 出现在⑤",
    );
    match app
        .dispatch(Command::HandoffStage {
            project: project_id,
            risky: false,
            note: "评审清单已勾完,正常交棒".to_string(),
        })
        .await
    {
        Ok(Event::HandoffRecorded(_)) => l.check(true, "再交一棒(阶段再往前推一棒,当前一棒变了)"),
        Ok(_) => l.fail("应该回 Event::HandoffRecorded"),
        Err(e) => l.fail(format!("HandoffStage(不带险)应该成功,实得错误:{e}")),
    }

    // ── 五项一起回读,验「消」那一半全部生效 ──
    let after = match app
        .attention_view(Some(project_id), OffsetDateTime::now_utc())
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("App::attention_view(消)应该成功,实得错误:{e}"));
            return l;
        }
    };
    l.check(
        !after.unsettled_runs.iter().any(|r| r.id == d_run),
        "消:①行自然消失(结算一次)",
    );
    l.check(
        !after
            .in_review_issues
            .iter()
            .any(|i| i.id == issues.b_review),
        "消:②行自然消失(点完成)",
    );
    l.check(
        !after.stalled_runs.iter().any(|r| r.id == c_run),
        "消:③旧行自然消失(重开一次,有更晚的运行接手)",
    );
    l.check(
        after.stalled_runs.iter().any(|r| r.id == c_run2),
        "重开的那条新运行(还是失败)如实出现在③——它现在是最新一条",
    );
    l.check(
        !after
            .metrics_without_observation
            .iter()
            .any(|m| m.id == metrics.manual),
        "消:④a 行自然消失(填一条)",
    );
    l.check(
        !after.metrics_stale.iter().any(|m| m.id == metrics.leading),
        "消:④b 行自然消失(填一条新的)",
    );
    l.check(
        !after
            .current_stage_risky_handoffs
            .iter()
            .any(|h| h.id == handoff1),
        "消:⑤行自然消失(阶段再往前推一棒,当前一棒变了)",
    );

    // 「更早的欠账在风险与决策段永久可查」——handoff 表里那条带险记录本身
    // 一个字节都没有被动过,只是不再出现在待人处理这份**投影**里。
    match app.store.list_handoffs(project_id).await {
        Ok(rows) => {
            let still_there = rows.iter().find(|h| h.id == handoff1);
            l.check(
                still_there.is_some_and(|h| h.risky),
                "反证:带险交棒记录本身在 handoff 表(风险与决策段)里永久还在,risky=true 没有被改动",
            );
        }
        Err(e) => l.fail(format!("list_handoffs 应该成功,实得错误:{e}")),
    }
    l.check(
        !bw_app::view::attention::CURRENT_STAGE_RISKY_HANDOFF_CAVEAT.is_empty(),
        "裁决 8 的防误读文案非空(界面渲染⑤时附加在旁边)",
    );

    // ── 全部安静时,清单长度真的是 0(不是带框的空壳)──
    let empty_project = ProjectId::new();
    if let Err(e) = app
        .store
        .create_project(NewProject {
            id: empty_project,
            name: "hex_readback 全空项目(演示 is_quiet)".to_string(),
            root_path: String::new(),
        })
        .await
    {
        l.fail(format!("造一个全空项目失败:{e}"));
        return l;
    }
    match app
        .attention_view(Some(empty_project), OffsetDateTime::now_utc())
        .await
    {
        Ok(quiet) => {
            l.check(
                quiet.is_quiet() && quiet.total_count() == 0,
                format!("全空项目:待人处理清单长度 = 0(真的空,不是「0 条告警」的空壳),实得 total_count={}", quiet.total_count()),
            );
        }
        Err(e) => l.fail(format!(
            "空项目的 App::attention_view 应该成功,实得错误:{e}"
        )),
    }

    l
}

// ─────────────────────────── --restart-probe 子命令 ───────────────────────────

/// [`section_restart_consistency`] 「后半场」的真子进程落点——父进程用
/// `std::env::current_exe()` re-exec 出的**全新操作系统进程**带着
/// [`RESTART_PROBE_ARG`] 跑到这里,不是正常的七段流程。
///
/// 独立打开 `App`/`RunManager`、独立重新采工作区证据、独立重新组装六段
/// 总控视图与待人处理投影,把结果各打印成一行 Debug 文本(`HEX_VIEW_DEBUG:`/
/// `ATTENTION_VIEW_DEBUG:` 前缀,父进程按这两个前缀切出来做字符串比
/// 对)。`now` 由父进程传入而不是这里现取(见调用方文档「为什么固定
/// `now`」)。
///
/// 位置参数(父进程负责按这个顺序拼,顺序错了会解析失败,`ASSERT
/// FAILED` 到 stderr、非零退出,父进程会如实报「子进程输出里没找到…
/// 行」):`<db_path> <project_id-uuid> <workspace> <now-unix-ts>`。
async fn run_restart_probe(args: &[String]) -> ExitCode {
    if args.len() != 4 {
        eprintln!(
            "ASSERT FAILED: --restart-probe 需要 4 个位置参数(db_path project_id workspace now),实得 {}",
            args.len()
        );
        return ExitCode::FAILURE;
    }
    let db_path = args[0].as_str();
    let project_id_str = args[1].as_str();
    let workspace = args[2].as_str();
    let now_str = args[3].as_str();

    let project_uuid = match Uuid::parse_str(project_id_str) {
        Ok(u) => u,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe project_id 解析失败:{e}");
            return ExitCode::FAILURE;
        }
    };
    let project_id = ProjectId::from_uuid(project_uuid);
    let now_ts: i64 = match now_str.parse() {
        Ok(n) => n,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe now 解析失败:{e}");
            return ExitCode::FAILURE;
        }
    };
    let now = match OffsetDateTime::from_unix_timestamp(now_ts) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe now 转换失败:{e}");
            return ExitCode::FAILURE;
        }
    };

    let app = match App::open(db_path).await {
        Ok(a) => a,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe 子进程 App::open 失败:{e}");
            return ExitCode::FAILURE;
        }
    };
    let registry = Arc::new(ConnectorRegistry::default());
    let manager =
        match RunManager::open(Path::new(db_path), registry, RunManagerConfig::default()).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ASSERT FAILED: --restart-probe 子进程 RunManager::open 失败:{e}");
                return ExitCode::FAILURE;
            }
        };
    let (loop_inputs, _snap) = match current_loop_inputs(&app.store, &manager, project_id).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe 子进程组装 LoopInputs 失败:{e}");
            return ExitCode::FAILURE;
        }
    };
    let evidence = match bw_workspace::evidence::collect(workspace).await {
        Ok(e) => e,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe 子进程采集工作区证据失败:{e}");
            return ExitCode::FAILURE;
        }
    };
    let hex = match app
        .hex_view(
            project_id,
            loop_inputs,
            WorkspaceEvidenceState::Present(evidence),
            now,
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe 子进程 App::hex_view 失败:{e}");
            return ExitCode::FAILURE;
        }
    };
    let attention = match app.attention_view(Some(project_id), now).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ASSERT FAILED: --restart-probe 子进程 App::attention_view 失败:{e}");
            return ExitCode::FAILURE;
        }
    };
    println!("HEX_VIEW_DEBUG:{hex:?}");
    println!("ATTENTION_VIEW_DEBUG:{attention:?}");
    // 进程身份本身的证据——父进程拿这一行与自己的 `std::process::id()` 比
    // 对,逐字段相等再多也证明不了「真的换了一个进程」,PID 不同这一条
    // 硬事实才是(见 `section_restart_consistency` 的 `l.check(child_pid
    // != my_pid, …)`)。
    println!("CHILD_PID:{}", std::process::id());
    println!("RESTART_PROBE_OK");
    ExitCode::SUCCESS
}

// ─────────────────────────── 8 · 能点「开工」的活(六段④)───────────────────────────

/// next 切片七-2 修(design-s7-real-project.md §1「壳的活卡加『开工』」,
/// 评审 `task-s7b-review.md` Important-3):`view::hex::LoopSegment::
/// startable_issues` 此前没有任何断言覆盖——判据(待办池/待办全部,加上
/// 「进行中」里再减掉「当前有一条活着的运行」的那部分)只靠这里的正反
/// 各验一次才真的被钉住。**独立开一个项目**,不搅进 §5/§6/§7 共用的那
/// 份大夹具(issue a/b/c/d 的状态/阶段已经被很多条既有断言绑住,插进去
/// 风险大于收益,同 `create_closed_run` 文档「issue c 的失败运行立刻结
/// 账,不然会同时出现在①和③」那条隔离既有先例)。
async fn section_startable_issues(app: &App) -> Ledger {
    let mut l = Ledger::new();
    let project_id = ProjectId::new();
    if let Err(e) = app
        .store
        .create_project(NewProject {
            id: project_id,
            name: "startable_issues 独立夹具(评审 Important-3)".to_string(),
            root_path: String::new(),
        })
        .await
    {
        l.fail(format!("create_project 失败:{e}"));
        return l;
    }

    let backlog = match create_issue(&app.store, project_id, 1, "待办池活(默认状态,原样起点)").await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(e);
            return l;
        }
    };

    let todo = match create_issue(&app.store, project_id, 2, "待办活").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(e);
            return l;
        }
    };
    if let Err(e) = app.transition_issue(todo, IssueStatus::Todo).await {
        l.fail(format!("todo 活 backlog→todo 失败:{e}"));
        return l;
    }

    // 死角修复要保的正例:「进行中」+ 一条已经关门的运行(上一次跑完了
    // 不等于干成,§1.5 第三行)——这件活应该出现在能开工清单里。
    let stalled = match create_issue(&app.store, project_id, 3, "进行中、没有活着的运行的活").await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(e);
            return l;
        }
    };
    if let Err(e) = app.transition_issue(stalled, IssueStatus::InProgress).await {
        l.fail(format!("stalled 活推进行中失败:{e}"));
        return l;
    }
    if let Err(e) = create_closed_run(
        &app.store,
        project_id,
        stalled,
        RunState::Finished,
        RunEndKind::ProcessExit,
        now_i64(),
        true,
    )
    .await
    {
        l.fail(e);
        return l;
    }

    // 反例:「进行中」+ 一条**还没关门**的运行——当前有一条活着的运行,
    // 即使状态也是「进行中」,也不该出现在能开工清单里(已经在跑)。
    let live = match create_issue(&app.store, project_id, 4, "进行中、有一条活着的运行的活").await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(e);
            return l;
        }
    };
    if let Err(e) = app.transition_issue(live, IssueStatus::InProgress).await {
        l.fail(format!("live 活推进行中失败:{e}"));
        return l;
    }
    if let Err(e) = app
        .store
        .create_run(NewRun {
            id: RunId::new(),
            project_id,
            issue_id: live,
            kind: RunKind::Delivery,
            connector_name: "hex-readback-fixture".to_string(),
            req_id: Uuid::new_v4().to_string(),
            workspace: format!("/tmp/hex-readback-fixture-{live:?}"),
            branch: "bw/hex-readback-fixture".to_string(),
            state: RunState::Running,
            started_at: now_i64(),
        })
        .await
    {
        l.fail(format!("造活着的运行失败:{e}"));
        return l;
    }

    // 反例:「评审中」——不在 `RunManager::start` 允许开工的三档状态里,
    // 判据本身不该重新发明,评审中的活不该出现在能开工清单里。
    let in_review = match create_issue(&app.store, project_id, 5, "评审中的活").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(e);
            return l;
        }
    };
    if let Err(e) = app
        .transition_issue(in_review, IssueStatus::InProgress)
        .await
    {
        l.fail(format!("in_review 活推进行中失败:{e}"));
        return l;
    }
    if let Err(e) = app.transition_issue(in_review, IssueStatus::InReview).await {
        l.fail(format!("in_review 活推评审中失败:{e}"));
        return l;
    }

    let loop_inputs = LoopInputs {
        running: 0,
        in_review: 0,
        recent_failed: 0,
        unsettled: 0,
        exceptions: Vec::new(),
        has_any_run: true,
    };
    let view = match app
        .hex_view(
            project_id,
            loop_inputs,
            WorkspaceEvidenceState::NotConfigured,
            OffsetDateTime::now_utc(),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("App::hex_view 失败:{e}"));
            return l;
        }
    };

    let startable_ids: std::collections::HashSet<IssueId> = view
        .loop_segment
        .startable_issues
        .iter()
        .map(|i| i.id)
        .collect();
    l.check(startable_ids.contains(&backlog), "待办池的活在能开工清单里");
    l.check(startable_ids.contains(&todo), "待办的活在能开工清单里");
    l.check(
        startable_ids.contains(&stalled),
        "进行中且没有活着的运行的活在能开工清单里(死角修复:上一次跑完了不等于干成,可以重新开工)",
    );
    l.check(
        !startable_ids.contains(&live),
        "进行中且当前有一条活着的运行的活不在能开工清单里(已经在跑,不该再给一个开工按钮)",
    );
    l.check(
        !startable_ids.contains(&in_review),
        "评审中的活不在能开工清单里(不在 RunManager::start 允许开工的三档状态里)",
    );
    l.check(
        view.loop_segment.startable_issues.len() == 3,
        format!(
            "能开工清单恰好 3 件(待办池 + 待办 + 进行中且无活运行,实得 {})",
            view.loop_segment.startable_issues.len()
        ),
    );

    l
}

// ─────────────── 9 · ArchiveMetric 命令进断言面(主控裁决,接线组) ───────────────

/// 主控对死代码审计报告「需人判断」项的裁决(2026-08-11,`.superpowers/
/// sdd/progress.md` 「接线」组第一条):「停用不物删是产品支柱,孤儿功能
/// 接上不删」——`Command::ArchiveMetric`/`Event::MetricArchived` 此前只
/// 在 `bw-app` 实现代码里存在(`cmd::metric::archive_metric` 正文 +
/// `App::dispatch` 的 match 分支),`hex_readback` 从没真的调过这条命
/// 令,铁律有实现、没断言。这一节补上完整链路:构造(一条活跃指标)→
/// 经命令停用(不是直接 `UPDATE`)→ 读回 `archived_at` 落值(停用不是删
/// 除——行还在)→ 派生链跳过(`view::hex::build` 的 `metrics.iter()
/// .filter(|m| m.archived_at.is_none())`,停用之后的指标不再出现在六段
/// 视图的指标卡列表里)→ 再停用一次证明幂等(`changed=false`,不是又停
/// 用了一次)。
async fn section_archive_metric_wiring(
    app: &App,
    manager: &RunManager,
    project_id: ProjectId,
    metrics: &MetricIds,
) -> Ledger {
    let mut l = Ledger::new();

    // 停用前:这条指标真的出现在指标卡列表里——否则下面「消失」这一半
    // 是恒真(没出现过的东西当然不会出现)。
    let (loop_inputs, _snap) = match current_loop_inputs(&app.store, manager, project_id).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("组装 LoopInputs 失败:{e}"));
            return l;
        }
    };
    let before = match app
        .hex_view(
            project_id,
            loop_inputs,
            WorkspaceEvidenceState::NotConfigured,
            OffsetDateTime::now_utc(),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("停用前 App::hex_view 应该成功,实得错误:{e}"));
            return l;
        }
    };
    l.check(
        before.metrics.cards.iter().any(|c| c.id == metrics.lagging),
        "停用前:这条滞后指标出现在指标卡列表里",
    );

    // 停用:经 `Command::ArchiveMetric`——证明的是这条命令本身真的接了
    // 线,不是「反正 store 层测过就默认命令层也对」。
    match app
        .dispatch(Command::ArchiveMetric {
            metric: metrics.lagging,
        })
        .await
    {
        Ok(Event::MetricArchived { changed }) => l.check(changed, "首次停用:changed=true"),
        Ok(_) => l.fail("Command::ArchiveMetric 应该回 Event::MetricArchived"),
        Err(e) => l.fail(format!("Command::ArchiveMetric 应该成功,实得错误:{e}")),
    }

    // 读回:`archived_at` 真的落值,不是命令回执自己说了算。
    match app.store.get_metric(metrics.lagging).await {
        Ok(Some(row)) => l.check(
            row.archived_at.is_some(),
            format!("读回:archived_at 落值(实得 {:?})", row.archived_at),
        ),
        Ok(None) => l.fail("停用之后这条指标本身应该还在(停用不物删)"),
        Err(e) => l.fail(format!("读回 get_metric 失败:{e}")),
    }

    // 派生链跳过:六段视图的指标卡不再列出它。
    let (loop_inputs2, _snap2) = match current_loop_inputs(&app.store, manager, project_id).await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("组装 LoopInputs 失败:{e}"));
            return l;
        }
    };
    let after = match app
        .hex_view(
            project_id,
            loop_inputs2,
            WorkspaceEvidenceState::NotConfigured,
            OffsetDateTime::now_utc(),
        )
        .await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("停用后 App::hex_view 应该成功,实得错误:{e}"));
            return l;
        }
    };
    l.check(
        !after.metrics.cards.iter().any(|c| c.id == metrics.lagging),
        "停用后:这条指标不再出现在指标卡列表里(派生链跳过;上面读回已经证过行还在,这不是删除)",
    );

    // 幂等:再停用一次,`changed=false`——不是「又真的停用了一次」。
    match app
        .dispatch(Command::ArchiveMetric {
            metric: metrics.lagging,
        })
        .await
    {
        Ok(Event::MetricArchived { changed }) => {
            l.check(!changed, "再停用一次:changed=false(幂等空转)")
        }
        Ok(_) => l.fail("Command::ArchiveMetric 应该回 Event::MetricArchived"),
        Err(e) => l.fail(format!("Command::ArchiveMetric 应该成功,实得错误:{e}")),
    }

    l
}

// ─────────────────────────── main ───────────────────────────

macro_rules! bail {
    ($msg:expr) => {{
        eprintln!("ASSERT FAILED: {}", $msg);
        eprintln!("HEX_READBACK_FAILED");
        return ExitCode::FAILURE;
    }};
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some(RESTART_PROBE_ARG) {
        return run_restart_probe(&args[2..]).await;
    }

    let db_path = fresh_db_path();
    let ws_dir = fresh_workspace_dir();
    clear_stale(&db_path, &ws_dir);
    let db_path_str = db_path.to_string_lossy().to_string();
    let ws_str = ws_dir.to_string_lossy().to_string();

    println!("== hex_readback · 切片五行为验收指挥器 ==");
    println!("本次数据库:{}   ← 跑完不删,给人手工复核", db_path.display());
    println!(
        "本次工作区:{}   ← 真 git 检出,带真 .bw/metrics.toml",
        ws_dir.display()
    );
    println!();

    let app = match App::open(&db_path_str).await {
        Ok(a) => a,
        Err(e) => bail!(format!("App::open 失败:{e}")),
    };
    let registry = Arc::new(ConnectorRegistry::default());
    let manager = match RunManager::open(&db_path, registry, RunManagerConfig::default()).await {
        Ok(m) => m,
        Err(e) => bail!(format!("RunManager::open 失败:{e}")),
    };

    let project_id = ProjectId::new();
    if let Err(e) = app
        .store
        .create_project(NewProject {
            id: project_id,
            name: "hex_readback 示例项目".to_string(),
            root_path: ws_str.clone(),
        })
        .await
    {
        bail!(format!("create_project 失败:{e}"));
    }
    if let Err(e) = setup_workspace(&ws_dir).await {
        bail!(format!("造真实工作区失败:{e}"));
    }

    match app
        .dispatch(Command::SetActiveStage {
            project: project_id,
            stage: StageKind::Prototype,
        })
        .await
    {
        Ok(Event::StageSet) => {}
        Ok(_) => bail!("Command::SetActiveStage 应该回 Event::StageSet"),
        Err(e) => bail!(format!("首次开棒应该成功,实得错误:{e}")),
    }

    let issues = match bootstrap_issues(&app, project_id).await {
        Ok(i) => i,
        Err(e) => bail!(e),
    };

    let evidence = match bw_workspace::evidence::collect(&ws_str).await {
        Ok(e) => e,
        Err(e) => bail!(format!("采集工作区证据失败:{e}")),
    };

    let mut total = Ledger::new();

    println!("== 1 · 指标正本同步(北极星也是一行)==");
    let (l1, metric_ids) = section_metrics_sync(&app, project_id, &ws_str, &db_path_str).await;
    total.merge(l1);
    let Some(metrics) = metric_ids else {
        bail!("第一次指标同步本身没成功,后续步骤无法继续");
    };
    println!();

    println!("== 2 · 观测只追加 + 手填戴徽 ==");
    total.merge(section_observations(&app, project_id, &metrics).await);
    println!();

    println!("== 3 · 信号只能推导(读时现算,零缓存)==");
    total.merge(
        section_signal_derive_only(
            &app,
            &manager,
            &db_path_str,
            project_id,
            &metrics,
            &evidence,
        )
        .await,
    );
    println!();

    // 造运行(供 §4 重启比对、§5 六段读回、§7①③ 用)——issue c 的失败运行
    // 立刻结账(不然会同时出现在①和③,把两条各自的正反验证绊在一
    // 起);issue d 的运行关门不结账,是①的正例。
    let c_run = match create_closed_run(
        &app.store,
        project_id,
        issues.c_stalled,
        RunState::Failed,
        RunEndKind::StartFailed,
        now_i64(),
        true,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => bail!(e),
    };
    let d_run = match create_closed_run(
        &app.store,
        project_id,
        issues.d_unsettled,
        RunState::Finished,
        RunEndKind::ProcessExit,
        now_i64(),
        false,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => bail!(e),
    };

    println!("== 4 · 杀进程重开,数字一致(真子进程 re-exec)==");
    total.merge(section_restart_consistency(&db_path_str, &ws_str, project_id).await);
    println!();

    println!("== 5 · 六段逐段读回 ==");
    total.merge(
        section_hex_segments(
            &app,
            &manager,
            &db_path_str,
            project_id,
            &metrics,
            &issues,
            &ws_str,
            &evidence,
            c_run,
            d_run,
        )
        .await,
    );
    println!();

    println!("== 6 · 完成永远人点(显式转移 + 合法转移表拒绝 + 没有第二条写路径)==");
    total.merge(section_done_is_human(&app, project_id).await);
    println!();

    println!("== 7 · 待人处理:五项正反各验一次 ==");
    total.merge(
        section_attention_five_items(&app, project_id, &metrics, &issues, c_run, d_run).await,
    );
    println!();

    println!("== 8 · 能点『开工』的活(六段④,next 切片七-2 修,评审 Important-3)==");
    total.merge(section_startable_issues(&app).await);
    println!();

    println!("== 9 · ArchiveMetric 命令进断言面(主控裁决,接线组,2026-08-11)==");
    total.merge(section_archive_metric_wiring(&app, &manager, project_id, &metrics).await);
    println!();

    // 终审 Important-3(2026-08-11):断言条数守卫,推广自 `run_races.rs`
    // 「== 5 ==」那一节的既有先例——同一类假绿在这份指挥器上没有理由不
    // 会发生:哪一节的 `total.merge(...)` 被漏调,那一节所有断言压根不
    // 执行(不是失败),`ok` 不会翻,只有条数悄悄变少。这份指挥器没有
    // `--rounds` 这类会改变断言数的参数,九节各自条数在固定输入下是常
    // 量,因此不是公式、是这次终审实跑坐实的基线;哪天某一节的断言数真
    // 的变了(加断言/改场景),照实改这个数,不要删掉这条守卫。
    let expected_assertions = 112;
    let actual_before_meta_check = total.count;
    total.check(
        actual_before_meta_check == expected_assertions,
        format!(
            "断言条数与基线一致(九节固定输入下的常量,期望 {expected_assertions},实得 {actual_before_meta_check};不等就说明有一整节的断言没有真的跑到)"
        ),
    );

    println!(
        "断言 {} 条,{}",
        total.count,
        if total.ok { "全过" } else { "有失败" }
    );
    println!("数据库留在:{}", db_path.display());
    println!("工作区留在:{}", ws_dir.display());
    if total.ok {
        println!("HEX_READBACK_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("HEX_READBACK_FAILED");
        ExitCode::FAILURE
    }
}
