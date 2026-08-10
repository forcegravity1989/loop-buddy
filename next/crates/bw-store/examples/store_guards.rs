//! `store_guards` — bw-store 最小行为读回指挥器(next 切片四A)。
//!
//! 不开界面,直接调用 [`IssueStore`]/[`RunStore`] 的公开方法,把
//! design-s4-runmanager.md §2 的两把结算/关门守卫各跑一遍并读回,证明「铁
//! 律钉进了存储,不是 if 判断」:
//!
//! 1. 插一个项目、一件活。
//! 2. 给这件活开两个交付运行——第二个必须被 `uq_run_live_delivery_per_issue`
//!    部分唯一索引挡下,打印 SQLite 报错的分类(`is_unique_violation`)。
//! 3. 对第一个运行「关门」两次——第二次的比较并置必须诚实空转(受影响
//!    0 行),字段值前后逐字节一致。
//! 4. 对同一个运行「结算」两次——同上。
//! 5. 关门之后,名额真的放开了:同一件活能再开一个交付运行;它还活着时,
//!    第五次尝试又会被挡下——正反两个方向都读回。
//! 6. 活的结算(`issue.settled_at` COALESCE)也调两次,确认第一次的时刻
//!    保住不变。
//! 7. 用一条独立连接直接查 `PRAGMA index_list('run')`,读回索引真的建了;
//!    再读 `sqlite_master.sql` 的谓词原文,断言同时含 `ended_at IS NULL`
//!    与 `kind = 'delivery'` 两半(评审 Important-1:只读 `unique`/`partial`
//!    两个布尔不够,`kind` 那一半悄悄丢了指挥器也照样全绿)。
//! 8. 一件活开着一个活着的**咨询**运行时,再开一个**交付**运行必须被允许
//!    ——这是 `kind = 'delivery'` 那一半谓词唯一的存在理由(设计稿
//!    §3.5「降级为咨询当场释放交付名额」),第 7 节的谓词原文读回只证明
//!    这半句 SQL 被建了,不证明它的**行为**是对的;这一节才是确定性行
//!    为断言,去掉这半句谓词会让它当场变红。
//!
//! 运行管理器本体(开工/取消/重启清理)与「同一件活开不出第二个交付运行」
//! 之外的其余四个竞态(取消完成撞车/单条失败不牵连/重启遗留/晚到消息)
//! 是下一任务的 `run_races` 指挥器要证的,本片只证存储层这两把守卫。
//!
//! **next 切片五B 新增(design-s5-hexpanel.md §2)**——第 9-14 节:
//!
//! 9.  指标正本同步:真实读一份 `.bw/metrics.toml`(经 `bw-workspace` 的
//!     `metrics_file::read` + `metric_shape::flatten` 归一),喂给
//!     `MetricStore::sync_metrics_from_file`,SQL 读回北极星落进 `metric`
//!     表当一行(不在 `project` 表上)、`origin='file'` 行数与文件条数一致。
//! 10. 二次同步:改一条定义(保住行 id)/ 删一条(自动停用,不物删)/ 手建
//!     行(`origin='manual'`)不被同步器碰 / 再同步回来(自动恢复)——
//!     `docs/metrics-toml-format.md` 那套同步语义逐条读回。
//! 11. 北极星至多一行的突变自证:绕过同步用例直接插第二条 `north_star` →
//!     必须撞 `uq_metric_north_star` 唯一索引冲突。
//! 12. 观测只追加:接口上没有 update/delete 方法(结构事实,不是纪律);
//!     手填一条(`source='manual'`)、脚本采一条(`source='script'`),读回
//!     顺序按 `ts DESC`。
//! 13. 交棒 + 阶段推进:首次开棒(`set_active_stage`)→ 交棒到下一棒
//!     (`handoff_stage`,原子写 handoff 流水 + `project.active_stage`)→
//!     两处读回一致。
//! 14. 附 · 存量库迁移双守卫第二次真实使用:用缺
//!     `project.active_stage`/`issue.stage`/`issue.body` 三列的老 schema
//!     造一个库 → 走正式开库流程 → `PRAGMA table_info` 读回三列都补上了。
//!
//! **next 切片五-1 修复轮新增(评审 task-s5a-review.md)**——第 15-16 节:
//!
//! 15. 观测只追加升级成数据库结构约束(Important-2):`schema.sql` 给
//!     `observation` 表加了 `BEFORE UPDATE`/`BEFORE DELETE` 触发器,
//!     `RAISE(ABORT, …)`。绕过 `ObservationStore` 的公开方法,用一条独立
//!     连接直接对同一个数据库文件跑裸 `UPDATE`/`DELETE`,断言两条都被
//!     数据库拒绝,原始行一个字节没变——比第 12 节「trait 上没有这两个
//!     方法」更硬,评审的突变 M3(把这两个方法加回 trait)今天会被这一
//!     节挡下来,不会再像评审报告里那样全绿溜走。
//! 16. `.bw/metrics.toml` 缺 `[north_star]` 一节:如实降级为 `None`,不
//!     让整份正本一条都同步不进去(Important-4)——真读一份没有
//!     `[north_star]` 的 `.bw/metrics.toml`,经 `metrics_lenient::
//!     read_lenient` + `metric_shape::flatten_lenient` 归一,喂给
//!     `sync_metrics_from_file`,SQL 读回滞后/引领正常入库、没有
//!     `tier='north_star'` 的行、同步过程不报错。
//!
//! 跑法:`cd next && cargo run -p bw-store --example store_guards`
//! 退出码 0 且末行 `STORE_GUARDS_OK` = 全部断言通过。
//! 数据库跑完不删,路径打印出来,人可以自己 `sqlite3 <path>` 复核。
//!
//! **库名(评审 Important-2 补丁)**:文件名带 uuid v4,不用 PID——PID 会
//! 被操作系统复用,某次跑挂在临时目录里的残留库会被下一次「同名」跑
//! 直接复用,而 `SqliteStore::open` 对已存在的库只会补列/补索引不会重建,
//! 改坏 `schema.sql` 之后如果连的是这么一个残留库,读到的还是旧定义、
//! 会假绿(评审者本人复现时踩过这个坑,必须先手工 `rm -f` 残留库才能让
//! 突变真正落地)。开跑前会做两件事:①(几乎不会触发,纯防御)如果本次
//! 生成的路径碰巧已经存在就先删;②清掉临时目录里**所有**更早的
//! `bw-store-guards-*` 库(旧 PID 命名的历史遗留、或上一次跑忘记清的失
//! 败现场)——这样任何一次跑读到的库,要么是它自己刚建的,要么根本不
//! 存在,不会有第三种「读到别人的库」。

use bw_core::{IssueId, MetricId, ObservationId, ProjectId, RunId, RunState, StageKind};
use bw_store::{
    HandoffStore, IncomingMetricDef, IssueStore, MetricStore, MetricTier, NewIssue, NewObservation,
    NewProject, NewRun, ObservationStore, RunEndKind, RunKind, RunStore, SqliteStore,
};
use bw_workspace::metric_shape::flatten;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use time::OffsetDateTime;
use uuid::Uuid;

/// 【store_guards 指挥器直接写入,非真实执行连接器上报】—— 本档不装连接
/// 器,close/settle 的调用是指挥器自己模拟「运行走完了」这一步,不冒充
/// 真实执行结果。
const SIM_END_DETAIL: &str = "【store_guards 指挥器直接写入,非真实执行连接器上报】进程退出,码 0";

/// 本指挥器所有库文件共享的前缀,`clear_stale_dbs` 靠它识别「这是我家的
/// 文件,可以扫」,不会误删临时目录里别的东西。
const DB_NAME_PREFIX: &str = "bw-store-guards-";

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// 本次要用的库路径——uuid v4,每次跑都唯一,PID 复用不再是问题。
fn fresh_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("{DB_NAME_PREFIX}{}.db", Uuid::new_v4()))
}

/// 开跑前清场(评审 Important-2):删掉任何与本次路径同名的残留文件
/// (纯防御,uuid 下概率可忽略),并清掉临时目录里所有**更早**的
/// `bw-store-guards-*` 库——不管是旧 PID 命名方案留下的,还是某次失败
/// 跑忘了清的现场。跑完之后临时目录里只剩本次这一个库,下一次跑不会
/// 意外读到任何人的旧库。
fn clear_stale_dbs(current: &Path) {
    if current.exists() {
        let _ = std::fs::remove_file(current);
    }
    let dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let current_name = current.file_name().map(|n| n.to_os_string());
    for entry in entries.flatten() {
        let name = entry.file_name();
        if current_name.as_deref() == Some(name.as_os_str()) {
            continue;
        }
        if name.to_string_lossy().starts_with(DB_NAME_PREFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let db_path = fresh_db_path();
    clear_stale_dbs(&db_path);
    let db_path = db_path.to_string_lossy().to_string();

    println!("== store_guards · bw-store 最小行为读回指挥器(next 切片四A) ==");
    println!(
        "本次数据库:{db_path}   ← 跑完不删,给人手工复核(uuid 命名,清过临时目录里更早的残留库)"
    );
    println!();

    let store = match SqliteStore::open(&db_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ASSERT FAILED: 开库失败: {e}");
            eprintln!("STORE_GUARDS_FAILED");
            return ExitCode::FAILURE;
        }
    };

    let mut all_ok = true;

    println!("== 0 · 插活 ==");
    let Some((project_id, issue_id)) = section_bootstrap(&store).await else {
        eprintln!("STORE_GUARDS_FAILED");
        return ExitCode::FAILURE;
    };
    println!();

    println!("== 1 · 同一件活开不出第二个交付运行 ==");
    let (ok, run1) = section_two_delivery_runs(&store, project_id, issue_id).await;
    all_ok &= ok;
    let Some(run1) = run1 else {
        eprintln!("STORE_GUARDS_FAILED(第一个交付运行本身没开成,后续步骤无法继续)");
        return ExitCode::FAILURE;
    };
    println!();

    println!("== 2 · 关门一次(比较并置) ==");
    all_ok &= section_close_twice(&store, run1).await;
    println!();

    println!("== 3 · 结算一次(比较并置) ==");
    all_ok &= section_settle_twice(&store, run1).await;
    println!();

    println!("== 4 · 关门后名额真的放开,活着时又真的挡住 ==");
    all_ok &= section_slot_released_after_close(&store, project_id, issue_id).await;
    println!();

    println!("== 5 · 活的结算(COALESCE,原样移植) ==");
    all_ok &= section_issue_settle_coalesce(&store, issue_id).await;
    println!();

    println!("== 6 · 独立连接读回:部分唯一索引真的建了(含谓词原文)==");
    all_ok &= section_index_readback(&db_path).await;
    println!();

    println!("== 7 · 咨询运行不占交付名额(kind='delivery' 半边谓词的行为断言)==");
    all_ok &= section_consultation_does_not_block_delivery(&store, project_id).await;
    println!();

    println!("== 9 · 指标正本同步(北极星也是一行,读一份真实 .bw/metrics.toml)==");
    let (ok, fixture) = section_metrics_sync_first(&store, project_id).await;
    all_ok &= ok;
    let Some(fixture) = fixture else {
        eprintln!("STORE_GUARDS_FAILED(第一次指标同步本身没成功,后续步骤无法继续)");
        return ExitCode::FAILURE;
    };
    println!();

    println!("== 10 · 二次同步:改一条 / 删一条(停用不物删)/ 手建行不被碰 / 再同步回来(自动恢复)==");
    all_ok &= section_metrics_sync_second_and_third(&store, &fixture).await;
    println!();

    println!("== 11 · 北极星至多一行的突变自证(绕过同步用例直接插第二条)==");
    all_ok &= section_north_star_at_most_one(&store, project_id).await;
    println!();

    println!("== 12 · 观测只追加(接口上没有 update/delete)+ 手填/脚本两种来源 ==");
    all_ok &= section_observation_append_only(&store, project_id, fixture.north_star_id).await;
    println!();

    println!("== 13 · 交棒 + 阶段推进(首次开棒 → 交棒到下一棒,原子写)==");
    all_ok &= section_handoff_and_active_stage(&store, project_id).await;
    println!();

    println!(
        "== 14 · 附:存量库迁移双守卫第二次真实使用(project.active_stage/issue.stage/issue.body)=="
    );
    all_ok &= section_migration_guard_s5_columns().await;
    println!();

    println!(
        "== 15 · 观测只追加升级成数据库结构约束(触发器棘轮,绕过 store 直接 SQL UPDATE/DELETE)=="
    );
    all_ok &=
        section_observation_ratchet(&store, project_id, fixture.north_star_id, &db_path).await;
    println!();

    println!("== 16 · `.bw/metrics.toml` 缺 [north_star] 一节:如实降级为 None,不整份失败 ==");
    all_ok &= section_north_star_missing_section(&store).await;
    println!();

    if all_ok {
        println!("数据库留在:{db_path}");
        println!("STORE_GUARDS_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("数据库留在:{db_path}");
        eprintln!("STORE_GUARDS_FAILED");
        ExitCode::FAILURE
    }
}

/// 第 0 节:插一个项目、一件活,后续所有节都在它们上面操作。
async fn section_bootstrap(store: &SqliteStore) -> Option<(ProjectId, IssueId)> {
    let project_id = ProjectId::new();
    let issue_id = IssueId::new();

    if let Err(e) = store
        .create_project(NewProject {
            id: project_id,
            name: "store_guards 示例项目".to_string(),
            root_path: String::new(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: create_project 应成功,实得错误: {e}");
        return None;
    }
    println!("项目: {project_id:?}");

    if let Err(e) = store
        .create_issue(NewIssue {
            id: issue_id,
            project_id,
            number: 1,
            title: "示例活 #1".to_string(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: create_issue 应成功,实得错误: {e}");
        return None;
    }
    println!("活: {issue_id:?} (#1 示例活 #1)");

    Some((project_id, issue_id))
}

/// 第 1 节:第一个交付运行应该成功;第二个必须被部分唯一索引挡下。返回
/// 第一个运行的编号,供后续节使用。
async fn section_two_delivery_runs(
    store: &SqliteStore,
    project_id: ProjectId,
    issue_id: IssueId,
) -> (bool, Option<RunId>) {
    let mut ok = true;

    let run1 = RunId::new();
    let create1 = store
        .create_run(NewRun {
            id: run1,
            project_id,
            issue_id,
            kind: RunKind::Delivery,
            connector_name: "store_guards".to_string(),
            req_id: format!("store-guards/{}", run1.uuid()),
            workspace: "/tmp/store-guards-workspace".to_string(),
            branch: "bw/issue-1".to_string(),
            state: RunState::Starting,
            started_at: now(),
        })
        .await;
    match create1 {
        Ok(()) => println!("第一个交付运行开工: {run1:?} → 成功(占住名额)"),
        Err(e) => {
            eprintln!("ASSERT FAILED: 第一个交付运行应该成功,实得错误: {e}");
            return (false, None);
        }
    }

    let run2 = RunId::new();
    let create2 = store
        .create_run(NewRun {
            id: run2,
            project_id,
            issue_id,
            kind: RunKind::Delivery,
            connector_name: "store_guards".to_string(),
            req_id: format!("store-guards/{}", run2.uuid()),
            workspace: "/tmp/store-guards-workspace-2".to_string(),
            branch: "bw/issue-1".to_string(),
            state: RunState::Starting,
            started_at: now(),
        })
        .await;
    match create2 {
        Ok(()) => {
            eprintln!(
                "ASSERT FAILED: 第二个交付运行应该被 uq_run_live_delivery_per_issue 挡下,实得成功: {run2:?}"
            );
            ok = false;
        }
        Err(e) => {
            let classified = e.is_unique_violation();
            println!("第二个交付运行开工: {run2:?} → 失败(如实预期)");
            println!("  错误原文: {e}");
            println!("  分类: is_unique_violation = {classified}");
            if !classified {
                eprintln!(
                    "ASSERT FAILED: 第二个交付运行的失败应分类为 is_unique_violation,实得 {classified}(可能是别的错误撞上了,不是铁律在拦)"
                );
                ok = false;
            }
        }
    }

    (ok, Some(run1))
}

/// 第 2 节:关门(结束回写)的比较并置——第二次调用必须诚实空转,字段值
/// 前后逐字节一致。
async fn section_close_twice(store: &SqliteStore, run_id: RunId) -> bool {
    let mut ok = true;

    let ended_at = now();
    let first = store
        .close_run(
            run_id,
            ended_at,
            RunState::Finished,
            Some(RunEndKind::ProcessExit),
            SIM_END_DETAIL,
        )
        .await;
    match first {
        Ok(true) => println!("第一次关门: 受影响 1 行 → 是第一个抵达的(如实预期)"),
        Ok(false) => {
            eprintln!("ASSERT FAILED: 第一次关门应该是第一个抵达的(true),实得 false");
            ok = false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 第一次关门应该成功,实得错误: {e}");
            return false;
        }
    }

    let row_after_first = match store.get_run(run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("ASSERT FAILED: 关门后应该还能读到这条运行,实得 None");
            return false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 读回运行行失败: {e}");
            return false;
        }
    };

    // 第二次关门:用不同的 ended_at/state/end_kind 调用,如果比较并置失
    // 效,这些新值会覆盖第一次的——那正是要挡住的错账。
    let second = store
        .close_run(
            run_id,
            ended_at + 999,
            RunState::Canceled,
            Some(RunEndKind::Canceled),
            "【store_guards 指挥器直接写入】这次晚到的调用不该生效",
        )
        .await;
    match second {
        Ok(false) => println!("第二次关门: 受影响 0 行 → 诚实空转(如实预期)"),
        Ok(true) => {
            eprintln!("ASSERT FAILED: 第二次关门应该诚实空转(false),实得 true(关门发生了两次)");
            ok = false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 第二次关门调用本身不应该报错,实得: {e}");
            ok = false;
        }
    }

    let row_after_second = match store.get_run(run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("ASSERT FAILED: 第二次关门后应该还能读到这条运行,实得 None");
            return false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 读回运行行失败: {e}");
            return false;
        }
    };

    println!(
        "  关门前后逐字段比对: ended_at={:?}→{:?} state={:?}→{:?} end_kind={:?}→{:?}",
        row_after_first.ended_at,
        row_after_second.ended_at,
        row_after_first.state,
        row_after_second.state,
        row_after_first.end_kind,
        row_after_second.end_kind,
    );
    if row_after_first.ended_at != row_after_second.ended_at
        || row_after_first.state != row_after_second.state
        || row_after_first.end_kind != row_after_second.end_kind
        || row_after_first.end_detail != row_after_second.end_detail
    {
        eprintln!("ASSERT FAILED: 第二次(晚到的)关门调用不应该改动任何字段,实得字段值发生了变化");
        ok = false;
    } else {
        println!("  ✓ 第二次调用一个字段都没改动");
    }

    ok
}

/// 第 3 节:结算的比较并置——同款语义,守的是 `settled_at`。
async fn section_settle_twice(store: &SqliteStore, run_id: RunId) -> bool {
    let mut ok = true;

    let settled_at = now();
    let first = store.settle_run(run_id, settled_at).await;
    match first {
        Ok(true) => println!("第一次结算: 受影响 1 行 → 是第一个抵达的(如实预期)"),
        Ok(false) => {
            eprintln!("ASSERT FAILED: 第一次结算应该是第一个抵达的(true),实得 false");
            ok = false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 第一次结算应该成功,实得错误: {e}");
            return false;
        }
    }

    let value_after_first = match store.get_run(run_id).await {
        Ok(Some(r)) => r.settled_at,
        Ok(None) => {
            eprintln!("ASSERT FAILED: 结算后应该还能读到这条运行,实得 None");
            return false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 读回运行行失败: {e}");
            return false;
        }
    };

    let second = store.settle_run(run_id, settled_at + 999).await;
    match second {
        Ok(false) => println!("第二次结算: 受影响 0 行 → 诚实空转(如实预期)"),
        Ok(true) => {
            eprintln!("ASSERT FAILED: 第二次结算应该诚实空转(false),实得 true(结算发生了两次)");
            ok = false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 第二次结算调用本身不应该报错,实得: {e}");
            ok = false;
        }
    }

    let value_after_second = match store.get_run(run_id).await {
        Ok(Some(r)) => r.settled_at,
        Ok(None) => {
            eprintln!("ASSERT FAILED: 第二次结算后应该还能读到这条运行,实得 None");
            return false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 读回运行行失败: {e}");
            return false;
        }
    };

    println!("  结算前后逐字节比对: settled_at={value_after_first:?}→{value_after_second:?}");
    if value_after_first != value_after_second {
        eprintln!("ASSERT FAILED: 第二次(晚到的)结算调用不应该改动 settled_at,实得值发生了变化");
        ok = false;
    } else {
        println!("  ✓ settled_at 值逐字节一致");
    }

    ok
}

/// 第 4 节:名额释放是双向的——运行关门后,同一件活能再开一个交付运行;
/// 新运行还活着时,再开第三个又会被挡下。正反两个方向都读回,证明部分
/// 唯一索引的谓词(`WHERE ended_at IS NULL`)真的只挡「活着」的那一条。
async fn section_slot_released_after_close(
    store: &SqliteStore,
    project_id: ProjectId,
    issue_id: IssueId,
) -> bool {
    let mut ok = true;

    let run2 = RunId::new();
    let reopen = store
        .create_run(NewRun {
            id: run2,
            project_id,
            issue_id,
            kind: RunKind::Delivery,
            connector_name: "store_guards".to_string(),
            req_id: format!("store-guards/{}", run2.uuid()),
            workspace: "/tmp/store-guards-workspace-3".to_string(),
            branch: "bw/issue-1".to_string(),
            state: RunState::Starting,
            started_at: now(),
        })
        .await;
    match reopen {
        Ok(()) => println!("名额已放开: 同一件活的第二个交付运行 {run2:?} → 成功"),
        Err(e) => {
            eprintln!(
                "ASSERT FAILED: 前一个运行已关门,同一件活应该能再开一个交付运行,实得错误: {e}"
            );
            return false;
        }
    }

    let run3 = RunId::new();
    let blocked_again = store
        .create_run(NewRun {
            id: run3,
            project_id,
            issue_id,
            kind: RunKind::Delivery,
            connector_name: "store_guards".to_string(),
            req_id: format!("store-guards/{}", run3.uuid()),
            workspace: "/tmp/store-guards-workspace-4".to_string(),
            branch: "bw/issue-1".to_string(),
            state: RunState::Starting,
            started_at: now(),
        })
        .await;
    match blocked_again {
        Ok(()) => {
            eprintln!("ASSERT FAILED: run2 还活着,第三个交付运行应该被挡下,实得成功: {run3:?}");
            ok = false;
        }
        Err(e) if e.is_unique_violation() => {
            println!("run2 还活着时再开一个: {run3:?} → 失败(如实预期,is_unique_violation=true)");
        }
        Err(e) => {
            eprintln!(
                "ASSERT FAILED: 第三个交付运行的失败应分类为 is_unique_violation,实得别的错误: {e}"
            );
            ok = false;
        }
    }

    ok
}

/// 第 5 节:活的结算——COALESCE,原样移植 v1 语义。调两次,确认第一次的
/// 时刻保住不变。
async fn section_issue_settle_coalesce(store: &SqliteStore, issue_id: IssueId) -> bool {
    let mut ok = true;

    let at1 = now();
    if let Err(e) = store.settle_issue(issue_id, at1).await {
        eprintln!("ASSERT FAILED: settle_issue 第一次调用应该成功,实得错误: {e}");
        return false;
    }
    let value_after_first = match store.get_issue(issue_id).await {
        Ok(Some(row)) => row.settled_at,
        Ok(None) => {
            eprintln!("ASSERT FAILED: 结算后应该还能读到这件活,实得 None");
            return false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 读回活失败: {e}");
            return false;
        }
    };
    println!("活结算第一次: settled_at = {value_after_first:?}");

    if let Err(e) = store.settle_issue(issue_id, at1 + 999).await {
        eprintln!("ASSERT FAILED: settle_issue 第二次调用本身不应该报错,实得: {e}");
        ok = false;
    }
    let value_after_second = match store.get_issue(issue_id).await {
        Ok(Some(row)) => row.settled_at,
        Ok(None) => {
            eprintln!("ASSERT FAILED: 第二次结算后应该还能读到这件活,实得 None");
            return false;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 读回活失败: {e}");
            return false;
        }
    };
    println!("活结算第二次(晚到,不同的时刻): settled_at = {value_after_second:?}");

    if value_after_first != value_after_second {
        eprintln!("ASSERT FAILED: COALESCE 应保住第一次的时刻,实得值被第二次调用覆盖");
        ok = false;
    } else {
        println!("  ✓ COALESCE 生效:两次调用后 settled_at 值不变");
    }

    ok
}

/// 第 6 节:不经过 [`SqliteStore`] 的公开方法,用一条独立连接直接对同一个
/// 数据库文件跑 `PRAGMA index_list`,读回部分唯一索引真的建了——这是
/// 「铁律钉进存储」最硬的证明:绕过应用代码也拦不住,因为拦的人是 SQLite
/// 自己。
///
/// **评审 Important-1 补丁**:只读 `unique`/`partial` 两个布尔不够——
/// `WHERE ended_at IS NULL` 这半条谓词去掉 `kind = 'delivery'` 之后,索引
/// 依然是 unique + partial,两个布尔照样读回 `true`。真正能分辨「谓词是
/// 不是完整」的只有谓词原文本身,所以这里再加一条 `sqlite_master.sql` 读
/// 回,断言原文同时含 `ended_at IS NULL` 与 `kind = 'delivery'` 两半。
async fn section_index_readback(db_path: &str) -> bool {
    let mut ok = true;

    let sql = "SELECT name, \"unique\", partial FROM pragma_index_list('run')";
    println!("执行 SQL: {sql}");
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: 无法用独立连接打开同一个数据库文件: {e}");
            return false;
        }
    };

    let rows = match sqlx::query(sql).fetch_all(&pool).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: PRAGMA index_list('run') 查询失败: {e}");
            return false;
        }
    };

    let mut found_partial_unique = false;
    for r in &rows {
        let name: String = r.get("name");
        let is_unique: i64 = r.get("unique");
        let is_partial: i64 = r.get("partial");
        println!("  index: name={name} unique={is_unique} partial={is_partial}");
        if name == "uq_run_live_delivery_per_issue" {
            found_partial_unique = is_unique == 1 && is_partial == 1;
        }
    }

    if found_partial_unique {
        println!("  ✓ uq_run_live_delivery_per_issue 存在,且 unique=1 partial=1");
    } else {
        eprintln!(
            "ASSERT FAILED: 期望在 run 表上找到 unique=1 partial=1 的 uq_run_live_delivery_per_issue 索引,没找到"
        );
        ok = false;
    }

    // 谓词原文读回(评审 Important-1 建议的三行读回,升级成断言)。
    let predicate_sql =
        "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = 'uq_run_live_delivery_per_issue'";
    println!("执行 SQL: {predicate_sql}");
    let predicate_row = match sqlx::query(predicate_sql).fetch_optional(&pool).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: sqlite_master 谓词原文查询失败: {e}");
            return false;
        }
    };
    let Some(predicate_row) = predicate_row else {
        eprintln!(
            "ASSERT FAILED: sqlite_master 里没找到 uq_run_live_delivery_per_issue 这条索引的定义"
        );
        return false;
    };
    let predicate_sql_text: String = predicate_row.get("sql");
    println!("  索引定义原文: {predicate_sql_text}");
    // 大小写、空白都不敏感地比对——只关心两半条件的文字都还在,不关心
    // schema.sql 里的换行/缩进怎么写。
    let normalized: String = predicate_sql_text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let has_ended_at_half = normalized.contains("ended_at is null");
    let has_kind_half = normalized.contains("kind = 'delivery'");
    if has_ended_at_half && has_kind_half {
        println!("  ✓ 谓词原文同时含 `ended_at IS NULL` 与 `kind = 'delivery'` 两半");
    } else {
        eprintln!(
            "ASSERT FAILED: 谓词原文应同时含 `ended_at IS NULL`(实得 {has_ended_at_half})与 \
             `kind = 'delivery'`(实得 {has_kind_half})两半,原文: {predicate_sql_text:?}"
        );
        ok = false;
    }

    ok
}

/// 第 7 节:`kind = 'delivery'` 那半条谓词唯一的存在理由(设计稿 §3.5)是
/// 「降级为咨询当场释放交付名额」——一件活开着一个活着的**咨询**运行时,
/// 交付名额不该被它占住,再开一个**交付**运行必须成功。
///
/// 这是评审 Important-1 点名的确定性行为断言:如果索引谓词丢了
/// `kind = 'delivery'` 那一半(退化成只剩 `WHERE ended_at IS NULL`),这
/// 条断言会立刻变红——统一谓词会把「活着的咨询运行」也算进「名额已占
/// 用」,把交付运行错误地挡下。不像第 6 节的谓词原文读回(证明 SQL 文
/// 本写对了),这一节直接证明**行为**对了。
async fn section_consultation_does_not_block_delivery(
    store: &SqliteStore,
    project_id: ProjectId,
) -> bool {
    // 用一件全新的活,不沾前面几节在同一个 project 下留的运行,断言面干
    // 净——这件活此前从未开过任何运行。
    let issue_id = IssueId::new();
    if let Err(e) = store
        .create_issue(NewIssue {
            id: issue_id,
            project_id,
            number: 2,
            title: "示例活 #2(咨询/交付名额并存)".to_string(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: create_issue(#2) 应该成功,实得错误: {e}");
        return false;
    }
    println!("活: {issue_id:?} (#2 示例活 #2(咨询/交付名额并存))");

    let consultation = RunId::new();
    if let Err(e) = store
        .create_run(NewRun {
            id: consultation,
            project_id,
            issue_id,
            kind: RunKind::Consultation,
            connector_name: "store_guards".to_string(),
            req_id: format!("store-guards/{}", consultation.uuid()),
            workspace: "/tmp/store-guards-workspace-consultation".to_string(),
            branch: "bw/issue-2".to_string(),
            state: RunState::Starting,
            started_at: now(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: 咨询运行开工应该成功,实得错误: {e}");
        return false;
    }
    println!("咨询运行开工: {consultation:?} → 成功,故意不关门,让它保持活着");

    let delivery = RunId::new();
    let delivery_result = store
        .create_run(NewRun {
            id: delivery,
            project_id,
            issue_id,
            kind: RunKind::Delivery,
            connector_name: "store_guards".to_string(),
            req_id: format!("store-guards/{}", delivery.uuid()),
            workspace: "/tmp/store-guards-workspace-consultation-delivery".to_string(),
            branch: "bw/issue-2".to_string(),
            state: RunState::Starting,
            started_at: now(),
        })
        .await;

    match delivery_result {
        Ok(()) => {
            println!(
                "同一件活、咨询运行还活着时开交付运行: {delivery:?} → 成功(如实预期:\
                 部分唯一索引只挡活着的**交付**运行,咨询运行不占交付名额)"
            );
            true
        }
        Err(e) => {
            eprintln!(
                "ASSERT FAILED: 一件活开着一个活着的咨询运行时,交付运行应该被允许开工,\
                 实得被挡下: {e}(is_unique_violation={})——这正是索引谓词丢了 \
                 `kind = 'delivery'` 那一半的信号:降级为咨询之后仍会永久占死这件活的\
                 交付名额(设计稿 §3.5)",
                e.is_unique_violation()
            );
            false
        }
    }
}

// ═══════════════════ next 切片五B:指标 / 观测 / 交棒 ═══════════════════

fn now_i64() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

/// 第 9 节留给第 10 节复用的状态——三次同步都要认得同一批指标身份,靠这
/// 几个 id 串联,而不是每节各自重新查一遍。
struct MetricsSyncFixture {
    project_id: ProjectId,
    north_star_id: MetricId,
    /// 会在第 10 节被改定义(验证「原地更新保住行 id」)。
    lagging_a_id: MetricId,
    /// 会在第 10 节被从正本删除、第三次同步又恢复(验证「停用不物删 +
    /// 自动恢复」)。
    leading_a_id: MetricId,
    /// 界面手建(`origin='manual'`),同步器全程不该碰它。
    manual_metric_id: MetricId,
}

/// 一份真实的 `.bw/metrics.toml` 内容:1 北极星 + 2 滞后 + 2 引领,四种
/// `collect.kind` 都用 `manual`(指挥器不需要真去采集,只验证同步语义)。
fn sample_metrics_toml(lagging_a_def: &str, include_leading_a: bool) -> String {
    let leading_a_block = if include_leading_a {
        "\n[[leading]]\nname = \"引领指标A\"\ndef = \"会在二次同步时被从正本删除\"\ntarget = \"≥3\"\ncollect = { kind = \"manual\", query = \"\" }\n"
    } else {
        ""
    };
    format!(
        "schema_version = 1\n\n\
         [north_star]\n\
         name = \"store_guards 北极星\"\n\
         def  = \"指挥器自造的北极星,用于验证三层同构\"\n\
         collect = {{ kind = \"manual\", query = \"\" }}\n\n\
         [[lagging]]\n\
         name = \"滞后指标A\"\n\
         def = \"{lagging_a_def}\"\n\
         target = \"≥1\"\n\
         collect = {{ kind = \"manual\", query = \"\" }}\n\n\
         [[lagging]]\n\
         name = \"滞后指标B\"\n\
         def = \"保持不变的对照组\"\n\
         target = \"≥2\"\n\
         collect = {{ kind = \"manual\", query = \"\" }}\n\
         {leading_a_block}\n\
         [[leading]]\n\
         name = \"引领指标B\"\n\
         def = \"保持不变的对照组\"\n\
         target = \"≥4\"\n\
         collect = {{ kind = \"manual\", query = \"\" }}\n"
    )
}

/// 把 `bw_workspace::metric_shape` 归一出来的扁平定义,逐字段搬成
/// `bw-store` 自己的 `IncomingMetricDef`(两个类型分住两个 crate,`bw-store`
/// 不依赖 `bw-workspace`——见 lib.rs `IncomingMetricDef` 文档)。
fn to_incoming(defs: Vec<bw_workspace::metric_shape::FlatMetricDef>) -> Vec<IncomingMetricDef> {
    defs.into_iter()
        .map(|d| IncomingMetricDef {
            tier: match d.tier {
                bw_workspace::metric_shape::MetricTier::NorthStar => MetricTier::NorthStar,
                bw_workspace::metric_shape::MetricTier::Lagging => MetricTier::Lagging,
                bw_workspace::metric_shape::MetricTier::Leading => MetricTier::Leading,
            },
            name: d.name,
            def: d.def,
            target_raw: d.target_raw,
            collect_kind: d.collect_kind.to_string(),
            collect_query: d.collect_query,
        })
        .collect()
}

/// 造一个真实工作区目录,写一份真实 `.bw/metrics.toml`,过
/// `bw_workspace::metrics_file::read` 真实解析 + `metric_shape::flatten`
/// 归一,返回扁平定义。任何一步失败都直接 panic 式返回 None——这是指挥器
/// 自己的测试夹具,不是被测代码,失败了没有「诚实退化」这一说。
fn read_and_flatten_fixture(
    toml_content: &str,
) -> Option<Vec<bw_workspace::metric_shape::FlatMetricDef>> {
    let dir = std::env::temp_dir().join(format!("bw-store-guards-metrics-{}", Uuid::new_v4()));
    let bw_dir = dir.join(".bw");
    if std::fs::create_dir_all(&bw_dir).is_err() {
        return None;
    }
    if std::fs::write(bw_dir.join("metrics.toml"), toml_content).is_err() {
        return None;
    }
    let workspace_str = dir.to_string_lossy().to_string();
    let file = match bw_workspace::metrics_file::read(&workspace_str) {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!(
                "ASSERT FAILED: 真实工作区 {workspace_str} 应该能读到 .bw/metrics.toml,实得 None"
            );
            return None;
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: .bw/metrics.toml 解析失败: {e}");
            return None;
        }
    };
    Some(flatten(&file))
}

/// 第 9 节:第一次同步——真读一份真实 `.bw/metrics.toml`(1 北极星 + 2
/// 滞后 + 2 引领),归一后喂给 `sync_metrics_from_file`,SQL 读回北极星落
/// 进 `metric` 表当一行、`origin='file'` 行数与文件条数一致。
async fn section_metrics_sync_first(
    store: &SqliteStore,
    project_id: ProjectId,
) -> (bool, Option<MetricsSyncFixture>) {
    let mut ok = true;

    let toml_content = sample_metrics_toml("会在二次同步时被改定义", true);
    println!("读:临时工作区 .bw/metrics.toml(真文件,内容原样打印)");
    println!("{toml_content}");
    let Some(flat) = read_and_flatten_fixture(&toml_content) else {
        return (false, None);
    };
    if flat.len() != 5 {
        eprintln!(
            "ASSERT FAILED: 归一后应有 5 条定义(1 北极星 + 2 滞后 + 2 引领),实得 {}",
            flat.len()
        );
        return (false, None);
    }
    println!("  ✓ 归一(flatten)后 5 条定义(1 北极星 + 2 滞后 + 2 引领)");

    let incoming = to_incoming(flat);
    let at = now_i64();
    let report = match store.sync_metrics_from_file(project_id, incoming, at).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: 第一次 sync_metrics_from_file 应该成功,实得错误: {e}");
            return (false, None);
        }
    };
    println!(
        "第一次同步: inserted={} updated={} archived={} restored={}",
        report.inserted.len(),
        report.updated.len(),
        report.archived.len(),
        report.restored.len()
    );
    if report.inserted.len() == 5
        && report.updated.is_empty()
        && report.archived.is_empty()
        && report.restored.is_empty()
    {
        println!("  ✓ 全部 5 条都是新插入,没有更新/停用/恢复");
    } else {
        eprintln!("ASSERT FAILED: 第一次同步应该 inserted=5 且其余三类为空,实得如上");
        ok = false;
    }

    let rows = match store.list_metrics(project_id).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: list_metrics 应该成功,实得错误: {e}");
            return (false, None);
        }
    };
    println!("读回:MetricStore::list_metrics(project_id)");
    for r in &rows {
        println!(
            "  tier={:?} name={:?} target_raw={:?} collect_kind={:?} origin={:?}",
            r.tier, r.name, r.target_raw, r.collect_kind, r.origin
        );
    }
    let file_rows: Vec<_> = rows.iter().filter(|r| r.origin == "file").collect();
    if file_rows.len() == 5 {
        println!("  ✓ origin='file' 的行数 = 5,与文件里的条数一致");
    } else {
        eprintln!(
            "ASSERT FAILED: origin='file' 行数应为 5,实得 {}",
            file_rows.len()
        );
        ok = false;
    }

    let Some(north_star) = rows.iter().find(|r| r.tier == MetricTier::NorthStar) else {
        eprintln!("ASSERT FAILED: 应该有一行 tier=NorthStar,实得没有");
        return (false, None);
    };
    if north_star.name == "store_guards 北极星" && north_star.target_raw.is_empty() {
        println!(
            "  ✓ 北极星在 metric 表里当一行(不在 project 表上——ProjectRow 类型定义里\
             压根没有北极星文本字段),target_raw 为空串(北极星本身没有 target)"
        );
    } else {
        eprintln!("ASSERT FAILED: 北极星那一行字段不对: {north_star:?}");
        ok = false;
    }

    let Some(lagging_a) = rows
        .iter()
        .find(|r| r.tier == MetricTier::Lagging && r.name == "滞后指标A")
    else {
        eprintln!("ASSERT FAILED: 应该有一行「滞后指标A」");
        return (false, None);
    };
    let Some(leading_a) = rows
        .iter()
        .find(|r| r.tier == MetricTier::Leading && r.name == "引领指标A")
    else {
        eprintln!("ASSERT FAILED: 应该有一行「引领指标A」");
        return (false, None);
    };

    // 手建一条指标(origin='manual')——同步器全程不该碰它,第 10 节读回验证。
    let manual_metric_id = MetricId::new();
    if let Err(e) = store
        .create_manual_metric(
            manual_metric_id,
            project_id,
            MetricTier::Leading,
            "手建指标(同步器不该碰)",
            "界面手建,不来自正本",
            "",
            "manual",
            "",
            at,
        )
        .await
    {
        eprintln!("ASSERT FAILED: create_manual_metric 应该成功,实得错误: {e}");
        ok = false;
    } else {
        println!("  ✓ 手建一条指标: {manual_metric_id:?}(origin='manual')");
    }

    (
        ok,
        Some(MetricsSyncFixture {
            project_id,
            north_star_id: north_star.id,
            lagging_a_id: lagging_a.id,
            leading_a_id: leading_a.id,
            manual_metric_id,
        }),
    )
}

/// 第 10 节:第二次同步(改一条定义 + 从正本删一条)与第三次同步(把删掉
/// 的那条加回来,验证自动恢复),外加手建行全程不受影响的读回。
async fn section_metrics_sync_second_and_third(
    store: &SqliteStore,
    fixture: &MetricsSyncFixture,
) -> bool {
    let mut ok = true;

    // —— 第二次同步:滞后指标A 改定义,引领指标A 从正本里删掉 ——
    let toml_2 = sample_metrics_toml("已改成新定义(二次同步)", false);
    println!("读:临时工作区 .bw/metrics.toml(第二版,引领指标A 已从正本删除)");
    let Some(flat_2) = read_and_flatten_fixture(&toml_2) else {
        return false;
    };
    if flat_2.len() != 4 {
        eprintln!(
            "ASSERT FAILED: 第二版应有 4 条定义(1 北极星 + 2 滞后 + 1 引领),实得 {}",
            flat_2.len()
        );
        return false;
    }
    let incoming_2 = to_incoming(flat_2);
    let at2 = now_i64();
    let report_2 = match store
        .sync_metrics_from_file(fixture.project_id, incoming_2, at2)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: 第二次 sync_metrics_from_file 应该成功,实得错误: {e}");
            return false;
        }
    };
    println!(
        "第二次同步: inserted={} updated={:?} archived={:?} restored={}",
        report_2.inserted.len(),
        report_2.updated,
        report_2.archived,
        report_2.restored.len()
    );
    if report_2.inserted.is_empty()
        && report_2.updated.len() == 4
        && report_2.archived == vec![fixture.leading_a_id]
        && report_2.restored.is_empty()
    {
        println!("  ✓ 4 条原地更新(含北极星),引领指标A 被自动停用,没有新插入/恢复");
    } else {
        eprintln!("ASSERT FAILED: 第二次同步的报告形状不对,实得如上");
        ok = false;
    }
    if report_2.updated.contains(&fixture.lagging_a_id) {
        println!(
            "  ✓ 滞后指标A 的行 id 在改定义前后保持不变: {:?}",
            fixture.lagging_a_id
        );
    } else {
        eprintln!("ASSERT FAILED: 滞后指标A 应该出现在 updated 列表里(id 保持不变)");
        ok = false;
    }

    let lagging_a_after = match store.get_metric(fixture.lagging_a_id).await {
        Ok(Some(r)) => r,
        other => {
            eprintln!("ASSERT FAILED: get_metric(滞后指标A) 应该还能读到,实得 {other:?}");
            return false;
        }
    };
    if lagging_a_after.def == "已改成新定义(二次同步)" {
        println!("  ✓ 滞后指标A 的 def 已更新为: {:?}", lagging_a_after.def);
    } else {
        eprintln!(
            "ASSERT FAILED: 滞后指标A 的 def 应该已更新,实得 {:?}",
            lagging_a_after.def
        );
        ok = false;
    }

    let leading_a_after = match store.get_metric(fixture.leading_a_id).await {
        Ok(Some(r)) => r,
        other => {
            eprintln!(
                "ASSERT FAILED: get_metric(引领指标A) 应该还能读到(停用不物删),实得 {other:?}"
            );
            return false;
        }
    };
    if leading_a_after.archived_at.is_some() {
        println!(
            "  ✓ 引领指标A 这一行还在(未物删),archived_at = {:?}(自动停用)",
            leading_a_after.archived_at
        );
    } else {
        eprintln!("ASSERT FAILED: 引领指标A 应该已被自动停用(archived_at 落值),实得 None");
        ok = false;
    }

    let manual_after_sync2 = match store.get_metric(fixture.manual_metric_id).await {
        Ok(Some(r)) => r,
        other => {
            eprintln!("ASSERT FAILED: 手建行应该还能读到,实得 {other:?}");
            return false;
        }
    };
    if manual_after_sync2.origin == "manual"
        && manual_after_sync2.archived_at.is_none()
        && manual_after_sync2.def == "界面手建,不来自正本"
    {
        println!("  ✓ 手建行全程未被同步器碰(origin/def/archived_at 都没变)");
    } else {
        eprintln!("ASSERT FAILED: 手建行被同步器动过了,实得 {manual_after_sync2:?}");
        ok = false;
    }

    // —— 第三次同步:引领指标A 重新出现在正本里,验证自动恢复 ——
    let toml_3 = sample_metrics_toml("已改成新定义(二次同步)", true);
    println!("读:临时工作区 .bw/metrics.toml(第三版,引领指标A 重新出现)");
    let Some(flat_3) = read_and_flatten_fixture(&toml_3) else {
        return false;
    };
    let incoming_3 = to_incoming(flat_3);
    let at3 = now_i64();
    let report_3 = match store
        .sync_metrics_from_file(fixture.project_id, incoming_3, at3)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: 第三次 sync_metrics_from_file 应该成功,实得错误: {e}");
            return false;
        }
    };
    println!(
        "第三次同步: inserted={} updated={:?} archived={} restored={:?}",
        report_3.inserted.len(),
        report_3.updated,
        report_3.archived.len(),
        report_3.restored
    );
    if report_3.restored == vec![fixture.leading_a_id] {
        println!(
            "  ✓ 引领指标A 自动恢复,行 id 与最初插入时一致: {:?}",
            fixture.leading_a_id
        );
    } else {
        eprintln!(
            "ASSERT FAILED: 第三次同步应该把引领指标A 恢复,实得 restored={:?}",
            report_3.restored
        );
        ok = false;
    }

    let leading_a_restored = match store.get_metric(fixture.leading_a_id).await {
        Ok(Some(r)) => r,
        other => {
            eprintln!("ASSERT FAILED: 恢复后应该还能读到引领指标A,实得 {other:?}");
            return false;
        }
    };
    if leading_a_restored.archived_at.is_none() {
        println!("  ✓ 恢复后 archived_at 已清空,三轮同步下来行 id 全程未变");
    } else {
        eprintln!(
            "ASSERT FAILED: 恢复后 archived_at 应该是 None,实得 {:?}",
            leading_a_restored.archived_at
        );
        ok = false;
    }

    ok
}

/// 第 11 节:北极星至多一行的突变自证——绕过同步用例,直接调
/// `create_manual_metric` 插第二条 `tier=NorthStar`,必须撞
/// `uq_metric_north_star` 部分唯一索引。
async fn section_north_star_at_most_one(store: &SqliteStore, project_id: ProjectId) -> bool {
    let second_id = MetricId::new();
    let result = store
        .create_manual_metric(
            second_id,
            project_id,
            MetricTier::NorthStar,
            "第二个北极星(应被拒)",
            "",
            "",
            "manual",
            "",
            now_i64(),
        )
        .await;
    match result {
        Ok(()) => {
            eprintln!(
                "ASSERT FAILED: 同一个项目插第二条 north_star 应该被 uq_metric_north_star 挡下,\
                 实得成功: {second_id:?}"
            );
            false
        }
        Err(e) => {
            let classified = e.is_unique_violation();
            println!("插第二个北极星: {second_id:?} → 失败(如实预期)");
            println!("  错误原文: {e}");
            println!("  分类: is_unique_violation = {classified}");
            if classified {
                println!("  ✓ 一个项目至多一条北极星,铁律钉进数据库,不是 if 判断");
                true
            } else {
                eprintln!(
                    "ASSERT FAILED: 失败原因应分类为 is_unique_violation,实得 {classified}\
                     (可能是别的错误撞上了,不是铁律在拦)"
                );
                false
            }
        }
    }
}

/// 第 12 节:观测只追加。`ObservationStore` trait 定义上只有 `insert_observation`
/// / `list_observations` 两个方法——没有 update/delete 方法可调,这份指挥
/// 器能编译通过本身就是这条结构约束成立的证明(不是运行期反证,是编译期
/// 反证:trait 上不存在的方法,任何调用点都写不出来)。这里另外验证功能性
/// 行为:手填一条 + 脚本采一条,读回顺序按 `ts DESC`。
async fn section_observation_append_only(
    store: &SqliteStore,
    project_id: ProjectId,
    metric_id: MetricId,
) -> bool {
    let mut ok = true;
    println!(
        "  ✓(结构事实,非运行期断言)ObservationStore trait 定义里只有 insert_observation/\
         list_observations 两个方法——没有 update/delete,这份指挥器能编译通过就是证明"
    );

    let older_ts = now_i64() - 3600;
    let newer_ts = now_i64();

    let script_obs_id = ObservationId::new();
    if let Err(e) = store
        .insert_observation(NewObservation {
            id: script_obs_id,
            metric_id,
            project_id,
            ts: older_ts,
            raw_value: "37%".to_string(),
            source: "script".to_string(),
            source_hint: "本地脚本 derive_adoption.py 输出".to_string(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: 脚本采集的观测插入应该成功,实得错误: {e}");
        return false;
    }
    println!("插入观测(脚本采集): {script_obs_id:?} ts={older_ts} raw_value=37% source=script");

    let manual_obs_id = ObservationId::new();
    if let Err(e) = store
        .insert_observation(NewObservation {
            id: manual_obs_id,
            metric_id,
            project_id,
            ts: newer_ts,
            raw_value: "42%".to_string(),
            source: "manual".to_string(),
            source_hint: "人手填".to_string(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: 手填的观测插入应该成功,实得错误: {e}");
        return false;
    }
    println!("插入观测(手填): {manual_obs_id:?} ts={newer_ts} raw_value=42% source=manual");

    let rows = match store.list_observations(metric_id).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: list_observations 应该成功,实得错误: {e}");
            return false;
        }
    };
    println!("读回:ObservationStore::list_observations(metric_id)");
    for r in &rows {
        println!(
            "  id={:?} ts={} raw_value={:?} source={:?} source_hint={:?}",
            r.id, r.ts, r.raw_value, r.source, r.source_hint
        );
    }
    if rows.len() == 2 && rows[0].id == manual_obs_id && rows[1].id == script_obs_id {
        println!("  ✓ 顺序按 ts DESC:手填(更新)的那条排在脚本采集(更早)的前面");
    } else {
        eprintln!("ASSERT FAILED: 观测顺序应为 [manual, script](ts DESC),实得如上");
        ok = false;
    }
    if rows
        .iter()
        .any(|r| r.source == "manual" && r.source_hint == "人手填")
    {
        println!("  ✓ 手填的观测带真实 source_hint,界面据此挂「手填」徽记");
    } else {
        eprintln!("ASSERT FAILED: 应该有一条 source='manual' 且 source_hint='人手填' 的观测");
        ok = false;
    }

    ok
}

/// 第 13 节:首次开棒(`set_active_stage`)→ 交棒到下一棒(`handoff_stage`,
/// 原子写 handoff 流水 + `project.active_stage`)→ 两处读回一致。
async fn section_handoff_and_active_stage(store: &SqliteStore, project_id: ProjectId) -> bool {
    let mut ok = true;

    let project_before = match store.get_project(project_id).await {
        Ok(Some(p)) => p,
        other => {
            eprintln!("ASSERT FAILED: get_project 应该成功,实得 {other:?}");
            return false;
        }
    };
    if project_before.active_stage.is_none() {
        println!("交棒前:project.active_stage = None(尚未开棒,如实预期)");
    } else {
        eprintln!(
            "ASSERT FAILED: 交棒前 active_stage 应为 None,实得 {:?}",
            project_before.active_stage
        );
        ok = false;
    }

    if let Err(e) = store
        .set_active_stage(project_id, StageKind::Prototype, now_i64())
        .await
    {
        eprintln!("ASSERT FAILED: set_active_stage 应该成功,实得错误: {e}");
        return false;
    }
    let project_after_open = match store.get_project(project_id).await {
        Ok(Some(p)) => p,
        other => {
            eprintln!("ASSERT FAILED: get_project 应该成功,实得 {other:?}");
            return false;
        }
    };
    if project_after_open.active_stage == Some(StageKind::Prototype) {
        println!("首次开棒后:project.active_stage = Some(Prototype)");
    } else {
        eprintln!(
            "ASSERT FAILED: 首次开棒后 active_stage 应为 Some(Prototype),实得 {:?}",
            project_after_open.active_stage
        );
        ok = false;
    }

    let note = "评审清单没勾完就交了,示例带险交棒注记";
    let handoff_id = match store
        .handoff_stage(
            project_id,
            StageKind::Prototype,
            StageKind::Build,
            true,
            note,
            now_i64(),
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            eprintln!("ASSERT FAILED: handoff_stage 应该成功,实得错误: {e}");
            return false;
        }
    };
    println!("交棒: Prototype → Build,risky=true,handoff_id={handoff_id:?}");

    let project_after_handoff = match store.get_project(project_id).await {
        Ok(Some(p)) => p,
        other => {
            eprintln!("ASSERT FAILED: get_project 应该成功,实得 {other:?}");
            return false;
        }
    };
    if project_after_handoff.active_stage == Some(StageKind::Build) {
        println!("  ✓ 交棒后 project.active_stage = Some(Build)(原子写的另一半生效)");
    } else {
        eprintln!(
            "ASSERT FAILED: 交棒后 active_stage 应为 Some(Build),实得 {:?}",
            project_after_handoff.active_stage
        );
        ok = false;
    }

    let handoffs = match store.list_handoffs(project_id).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("ASSERT FAILED: list_handoffs 应该成功,实得错误: {e}");
            return false;
        }
    };
    println!("读回:HandoffStore::list_handoffs(project_id)");
    for h in &handoffs {
        println!(
            "  id={:?} from={:?} to={:?} risky={} note={:?}",
            h.id, h.from_stage, h.to_stage, h.risky, h.note
        );
    }
    let matched = handoffs.iter().find(|h| h.id == handoff_id);
    match matched {
        Some(h)
            if h.from_stage == StageKind::Prototype
                && h.to_stage == StageKind::Build
                && h.risky
                && h.note == note =>
        {
            println!("  ✓ 交棒流水与调用参数逐字段一致");
        }
        other => {
            eprintln!("ASSERT FAILED: 交棒流水字段与预期不符,实得 {other:?}");
            ok = false;
        }
    }

    ok
}

/// 一份「少三列且没有五B 三张新表」的老 schema——`project` 没有
/// `active_stage`,`issue` 没有 `stage`/`body`,更没有 `metric`/`observation`/
/// `handoff` 三张表,`run` 也还没有 `demoted_at`(模拟切片四A 刚落地、四D/
/// 五B 都还没来的库)。**不经过 `SqliteStore::open`**(那样会用当前
/// schema.sql 把列建全,测不出「老库缺列」这件事),独立连接手工执行。
const OLD_SCHEMA_PRE_S5: &str = "
CREATE TABLE IF NOT EXISTS project (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    root_path   TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS issue (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    number      INTEGER NOT NULL,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'backlog',
    settled_at  INTEGER,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_issue_project_number ON issue(project_id, number);
CREATE TABLE IF NOT EXISTS run (
    id                TEXT PRIMARY KEY,
    project_id        TEXT NOT NULL REFERENCES project(id),
    issue_id          TEXT NOT NULL REFERENCES issue(id),
    kind              TEXT NOT NULL DEFAULT 'delivery',
    connector_name    TEXT NOT NULL,
    req_id            TEXT NOT NULL,
    upstream_session  TEXT NOT NULL DEFAULT '',
    workspace         TEXT NOT NULL,
    branch            TEXT NOT NULL,
    state             TEXT NOT NULL,
    end_kind          TEXT,
    end_detail        TEXT NOT NULL DEFAULT '',
    started_at        INTEGER NOT NULL,
    ended_at          INTEGER,
    settled_at        INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_run_live_delivery_per_issue
    ON run(issue_id) WHERE ended_at IS NULL AND kind = 'delivery';
CREATE INDEX IF NOT EXISTS idx_run_project_started ON run(project_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_run_open ON run(issue_id) WHERE ended_at IS NULL;
";

async fn section_migration_guard_s5_columns() -> bool {
    let old_db = std::env::temp_dir().join(format!(
        "{DB_NAME_PREFIX}pre-s5-schema-{}.db",
        Uuid::new_v4()
    ));
    let _ = std::fs::remove_file(&old_db);

    let opts = SqliteConnectOptions::new()
        .filename(&old_db)
        .create_if_missing(true);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: 造老库失败: {e}");
            return false;
        }
    };
    for stmt in OLD_SCHEMA_PRE_S5.split(';') {
        if stmt.trim().is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(stmt).execute(&pool).await {
            eprintln!("ASSERT FAILED: 老库建表语句失败: {e}\n语句: {stmt}");
            return false;
        }
    }
    pool.close().await;
    println!("造了一个缺 active_stage/stage/body 三列、也没有 metric/observation/handoff 三张表的老库: {}", old_db.display());

    // 用正式开库流程打开这个老库——`SqliteStore::open` 内部的三条
    // `add_column_if_missing` 应该把三列都补上,`CREATE TABLE IF NOT
    // EXISTS` 应该把三张新表都建起来。
    match SqliteStore::open(&old_db.to_string_lossy()).await {
        Ok(store) => drop(store),
        Err(e) => {
            eprintln!("ASSERT FAILED: 用正式开库流程打开老库失败: {e}");
            return false;
        }
    }

    let opts2 = SqliteConnectOptions::new()
        .filename(&old_db)
        .read_only(true);
    let pool2 = match SqlitePool::connect_with(opts2).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: 老库读回连接失败: {e}");
            return false;
        }
    };

    let mut ok = true;
    for (table, column) in [
        ("project", "active_stage"),
        ("issue", "stage"),
        ("issue", "body"),
    ] {
        let sql = format!("PRAGMA table_info({table})");
        let rows = match sqlx::query(&sql).fetch_all(&pool2).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("ASSERT FAILED: {sql} 失败: {e}");
                ok = false;
                continue;
            }
        };
        let has_column = rows.iter().any(|r| r.get::<String, _>("name") == column);
        println!(
            "PRAGMA table_info({table}) 读回:{column} 列{}",
            if has_column { "存在" } else { "缺失" }
        );
        if !has_column {
            eprintln!(
                "ASSERT FAILED: 老库打开后 {table}.{column} 应该存在(add_column_if_missing 生效),实得缺失"
            );
            ok = false;
        }
    }

    let tables_sql =
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name IN ('metric', 'observation', 'handoff') ORDER BY name";
    match sqlx::query(tables_sql).fetch_all(&pool2).await {
        Ok(rows) => {
            let names: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
            println!("sqlite_master 读回三张新表: {names:?}");
            if names == vec!["handoff", "metric", "observation"] {
                println!("  ✓ metric/observation/handoff 三张表在老库上也真的建起来了");
            } else {
                eprintln!("ASSERT FAILED: 三张新表应该都存在,实得 {names:?}");
                ok = false;
            }
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: {tables_sql} 失败: {e}");
            ok = false;
        }
    }

    if ok {
        println!("老库(打开后)留在:{}", old_db.display());
    }
    ok
}

// ═══════════════ next 切片五-1 修复轮:第 15-16 节 ═══════════════

/// 第 15 节:观测只追加,从「trait 上没有 update/delete 方法」升级成「数
/// 据库结构约束」(评审 task-s5a-review.md Important-2)。第 12 节证的是
/// 接口面——`ObservationStore` 编译期就没有那两个方法可调;但评审的突变
/// M3 实测过:把这两个方法加回 trait、在 `SqliteStore` 上接一段真
/// `UPDATE`/`DELETE` SQL,门禁、`store_guards` 全部照样绿到底,没有任何
/// 东西拦得住。`schema.sql` 新增的两条 `BEFORE UPDATE`/`BEFORE DELETE`
/// 触发器把这条铁律钉进数据库本身——这里绕过 `SqliteStore` 的公开方法,
/// 用一条独立连接直接对同一个数据库文件跑裸 SQL,断言两条都被拒绝,而
/// 且原始行一个字节没变(不只是"报错了",要确认真的没生效)。
///
/// **突变自证**(本节新增的护栏,复现见 task-s5a-report.md「修复轮」):
/// 临时从 `schema.sql` 删掉这两条触发器 → 这一节必须变红;加回来 → 恢复
/// 绿。
async fn section_observation_ratchet(
    store: &SqliteStore,
    project_id: ProjectId,
    metric_id: MetricId,
    db_path: &str,
) -> bool {
    let mut ok = true;

    let obs_id = ObservationId::new();
    if let Err(e) = store
        .insert_observation(NewObservation {
            id: obs_id,
            metric_id,
            project_id,
            ts: now_i64(),
            raw_value: "原始值,不该被改动".to_string(),
            source: "store_guards".to_string(),
            source_hint: "第 15 节触发器棘轮的靶子行".to_string(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: 插入靶子观测应该成功,实得错误: {e}");
        return false;
    }
    println!("插入靶子观测: {obs_id:?}(raw_value=\"原始值,不该被改动\")");

    let opts = SqliteConnectOptions::new().filename(db_path);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("ASSERT FAILED: 无法用独立连接打开同一个数据库文件: {e}");
            return false;
        }
    };

    let update_sql = "UPDATE observation SET raw_value = 'HACKED · 不该成功' WHERE id = ?";
    println!("执行 SQL(绕过 store 层,直接对同一个数据库文件): {update_sql}");
    match sqlx::query(update_sql)
        .bind(obs_id.uuid().to_string())
        .execute(&pool)
        .await
    {
        Ok(_) => {
            eprintln!(
                "ASSERT FAILED: 直接 UPDATE observation 应该被 BEFORE UPDATE 触发器拒绝,实得成功"
            );
            ok = false;
        }
        Err(e) => {
            let msg = e.to_string();
            println!("  UPDATE 被拒绝(如实预期),错误原文: {msg}");
            if msg.contains("观测只追加") {
                println!(
                    "  ✓ 拒绝原因是 append-only 触发器(错误原文带着那句中文),不是别的错误撞上"
                );
            } else {
                eprintln!(
                    "ASSERT FAILED: UPDATE 失败了,但错误原文没带触发器的中文提示,可能是别的原因挡的: {msg}"
                );
                ok = false;
            }
        }
    }

    let delete_sql = "DELETE FROM observation WHERE id = ?";
    println!("执行 SQL(绕过 store 层,直接对同一个数据库文件): {delete_sql}");
    match sqlx::query(delete_sql)
        .bind(obs_id.uuid().to_string())
        .execute(&pool)
        .await
    {
        Ok(_) => {
            eprintln!(
                "ASSERT FAILED: 直接 DELETE observation 应该被 BEFORE DELETE 触发器拒绝,实得成功"
            );
            ok = false;
        }
        Err(e) => {
            let msg = e.to_string();
            println!("  DELETE 被拒绝(如实预期),错误原文: {msg}");
            if msg.contains("观测只追加") {
                println!("  ✓ 拒绝原因是 append-only 触发器,不是别的错误撞上");
            } else {
                eprintln!(
                    "ASSERT FAILED: DELETE 失败了,但错误原文没带触发器的中文提示,可能是别的原因挡的: {msg}"
                );
                ok = false;
            }
        }
    }

    let rows = match store.list_observations(metric_id).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: list_observations 复核应该成功,实得错误: {e}");
            return false;
        }
    };
    match rows.iter().find(|r| r.id == obs_id) {
        Some(r) if r.raw_value == "原始值,不该被改动" => {
            println!(
                "  ✓ 复核:靶子行还在,raw_value 一个字节没变(UPDATE/DELETE 真的没有生效,不只是报错)"
            );
        }
        Some(r) => {
            eprintln!(
                "ASSERT FAILED: 靶子行的 raw_value 被改动了,实得 {:?}",
                r.raw_value
            );
            ok = false;
        }
        None => {
            eprintln!("ASSERT FAILED: 靶子行不见了,DELETE 其实生效了");
            ok = false;
        }
    }

    ok
}

/// 造一个临时工作区,写一份**没有 `[north_star]` 一节**的 `.bw/metrics.toml`
/// (只有 `[[lagging]]`/`[[leading]]` 两张数组表),过
/// `bw_workspace::metrics_lenient::read_lenient` +
/// `bw_workspace::metric_shape::flatten_lenient` 解析 + 归一,返回「北极星
/// 是否存在」与扁平定义——供第 16 节使用。
fn read_and_flatten_no_north_star_fixture(
    toml_content: &str,
) -> Option<(bool, Vec<bw_workspace::metric_shape::FlatMetricDef>)> {
    let dir = std::env::temp_dir().join(format!(
        "bw-store-guards-metrics-no-north-star-{}",
        Uuid::new_v4()
    ));
    let bw_dir = dir.join(".bw");
    if std::fs::create_dir_all(&bw_dir).is_err() {
        return None;
    }
    if std::fs::write(bw_dir.join("metrics.toml"), toml_content).is_err() {
        return None;
    }
    let workspace_str = dir.to_string_lossy().to_string();
    let file = match bw_workspace::metrics_lenient::read_lenient(&workspace_str) {
        Ok(Some(f)) => f,
        Ok(None) => {
            eprintln!(
                "ASSERT FAILED: 真实工作区 {workspace_str} 应该能读到 .bw/metrics.toml,实得 None"
            );
            return None;
        }
        Err(e) => {
            eprintln!(
                "ASSERT FAILED: 缺 [north_star] 一节的 .bw/metrics.toml 不该整份解析失败,实得错误: {e}"
            );
            return None;
        }
    };
    let has_north_star = file.north_star.is_some();
    Some((
        has_north_star,
        bw_workspace::metric_shape::flatten_lenient(&file),
    ))
}

/// 第 16 节:`.bw/metrics.toml` 缺 `[north_star]` 一节时,`read_lenient` 如
/// 实降级为 `north_star = None`,不让整份正本解析失败(评审
/// task-s5a-review.md Important-4)。滞后/引领正常入库,`metric` 表里没有
/// `tier='north_star'` 那一行,同步过程本身不报错——这正是设计稿 §1.2 第
/// ①行钉死的产品语义("没有这一节 → 灰卡「北极星尚未定稿」",不是"整份
/// 正本一条都不同步")的数据基础。
async fn section_north_star_missing_section(store: &SqliteStore) -> bool {
    let mut ok = true;

    let project_id = ProjectId::new();
    if let Err(e) = store
        .create_project(NewProject {
            id: project_id,
            name: "store_guards 缺北极星示例项目".to_string(),
            root_path: String::new(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: create_project 应成功,实得错误: {e}");
        return false;
    }
    println!("项目(缺北极星场景): {project_id:?}");

    let toml_content = "schema_version = 1\n\n\
         [[lagging]]\n\
         name = \"无北极星-滞后指标\"\n\
         def = \"这份正本没有 [north_star] 一节\"\n\
         target = \"≥1\"\n\
         collect = { kind = \"manual\", query = \"\" }\n\n\
         [[leading]]\n\
         name = \"无北极星-引领指标\"\n\
         def = \"这份正本没有 [north_star] 一节\"\n\
         target = \"≥2\"\n\
         collect = { kind = \"manual\", query = \"\" }\n";
    println!("读:临时工作区 .bw/metrics.toml(真文件,故意不写 [north_star],内容原样打印)");
    println!("{toml_content}");

    let Some((has_north_star, flat)) = read_and_flatten_no_north_star_fixture(toml_content) else {
        return false;
    };
    if has_north_star {
        eprintln!("ASSERT FAILED: 这份正本没写 [north_star],read_lenient 却读出了 Some");
        ok = false;
    } else {
        println!("  ✓ read_lenient: north_star = None(如实降级,不是解析失败)");
    }
    if flat.len() == 2 {
        println!("  ✓ 归一(flatten_lenient)后 2 条定义(1 滞后 + 1 引领,没有北极星那一条)");
    } else {
        eprintln!("ASSERT FAILED: 归一后应有 2 条定义,实得 {}", flat.len());
        ok = false;
    }

    let incoming = to_incoming(flat);
    let at = now_i64();
    let report = match store.sync_metrics_from_file(project_id, incoming, at).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: 缺北极星的正本同步应该成功,实得错误: {e}");
            return false;
        }
    };
    println!(
        "同步(不报错): inserted={} updated={} archived={} restored={}",
        report.inserted.len(),
        report.updated.len(),
        report.archived.len(),
        report.restored.len()
    );
    if report.inserted.len() == 2 {
        println!("  ✓ 2 条(滞后+引领)全部插入成功,同步没有因为缺北极星整批失败");
    } else {
        eprintln!(
            "ASSERT FAILED: 应该插入 2 条,实得 {}",
            report.inserted.len()
        );
        ok = false;
    }

    let rows = match store.list_metrics(project_id).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("ASSERT FAILED: list_metrics 应该成功,实得错误: {e}");
            return false;
        }
    };
    println!("读回:MetricStore::list_metrics(project_id)");
    for r in &rows {
        println!("  tier={:?} name={:?}", r.tier, r.name);
    }
    if rows.len() == 2 && !rows.iter().any(|r| r.tier == MetricTier::NorthStar) {
        println!("  ✓ metric 表里恰好 2 行,没有 tier=NorthStar 的行(下游据此显示「尚未定稿」灰卡)");
    } else {
        eprintln!("ASSERT FAILED: metric 表应恰好 2 行且没有 NorthStar,实得 {rows:?}");
        ok = false;
    }

    ok
}
