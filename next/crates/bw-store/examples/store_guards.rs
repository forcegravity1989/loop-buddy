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
//! 7. 用一条独立连接直接查 `PRAGMA index_list('run')`,读回索引真的建了。
//!
//! 运行管理器本体(开工/取消/重启清理)与「同一件活开不出第二个交付运行」
//! 之外的其余四个竞态(取消完成撞车/单条失败不牵连/重启遗留/晚到消息)
//! 是下一任务的 `run_races` 指挥器要证的,本片只证存储层这两把守卫。
//!
//! 跑法:`cd next && cargo run -p bw-store --example store_guards`
//! 退出码 0 且末行 `STORE_GUARDS_OK` = 全部断言通过。
//! 数据库跑完不删,路径打印出来,人可以自己 `sqlite3 <path>` 复核。

use bw_core::{IssueId, ProjectId, RunId, RunState};
use bw_store::{
    IssueStore, NewIssue, NewProject, NewRun, RunEndKind, RunKind, RunStore, SqliteStore,
};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use std::process::ExitCode;
use time::OffsetDateTime;

/// 【store_guards 指挥器直接写入,非真实执行连接器上报】—— 本档不装连接
/// 器,close/settle 的调用是指挥器自己模拟「运行走完了」这一步,不冒充
/// 真实执行结果。
const SIM_END_DETAIL: &str = "【store_guards 指挥器直接写入,非真实执行连接器上报】进程退出,码 0";

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

#[tokio::main]
async fn main() -> ExitCode {
    let db_path = std::env::temp_dir().join(format!("bw-store-guards-{}.db", std::process::id()));
    let db_path = db_path.to_string_lossy().to_string();

    println!("== store_guards · bw-store 最小行为读回指挥器(next 切片四A) ==");
    println!("本次数据库:{db_path}   ← 跑完不删,给人手工复核");
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

    println!("== 6 · 独立连接读回:部分唯一索引真的建了 ==");
    all_ok &= section_index_readback(&db_path).await;
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

    ok
}
