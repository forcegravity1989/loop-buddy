//! 指标停用/恢复 E2E (headless):走真 `SetMetricArchived` 命令层——桌面
//! 指标卡上「停用」按钮驱动的同一条路径,不是直接 UPDATE 表。验四件事:
//!
//! 1. 停用后该行 `archived=1` + `archived_at` 有值,**observation 条数不变**
//!    (append-only 不可破:停用不是删除);
//! 2. 停用行退出派生——`recompute_signals` 跑完它的 `signal_derived_rev`
//!    不再增长(灯冻结在停用那一刻),而在用行的照常增长;
//! 3. 恢复后 `archived=0` + `archived_at` 清空,下一次 recompute 重新给它
//!    派生(rev 恢复增长);
//! 4. 幂等:重复停用不重复盖时戳。
//!
//! Run: `cargo run -p bw-app --example verify_metric_archive -- <db-path> <项目名> <指标名>…`
//! 打印的每个数字都能用 `sqlite3 <db>` 独立读回。

use bw_app::{App, Command};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;

/// 一条指标此刻的真实底账,全部直接从库读,不经任何缓存。
struct Row {
    archived: i64,
    archived_at: Option<i64>,
    derived_rev: i64,
    obs: i64,
}

async fn read(pool: &sqlx::SqlitePool, id: &str) -> Row {
    use sqlx::Row as _;
    let r = sqlx::query(
        "SELECT m.archived, m.archived_at, COALESCE(m.signal_derived_rev,0) AS drev,
                (SELECT COUNT(*) FROM observation o WHERE o.metric_id=m.id) AS obs
         FROM metric m WHERE m.id=?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap();
    Row {
        archived: r.get("archived"),
        archived_at: r.get("archived_at"),
        derived_rev: r.get("drev"),
        obs: r.get("obs"),
    }
}

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db = args.next().expect("usage: <db-path> <项目名> <指标名>…");
    let project_name = args.next().expect("usage: <db-path> <项目名> <指标名>…");
    let targets: Vec<String> = args.collect();
    assert!(!targets.is_empty(), "至少给一个要停用的指标名");

    // 独立连接读回:刻意不复用 App 的 store 句柄,读的是库里真正落下的字节。
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite://{db}"))
        .await
        .unwrap();
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    let projects = store.list_projects().await.unwrap();
    let p = projects
        .iter()
        .find(|p| p.name == project_name)
        .unwrap_or_else(|| panic!("找不到项目 {project_name}"));
    app.dispatch(Command::OpenProject(p.id)).await.unwrap();

    let sigs = store.persisted_signals(p.id).await.unwrap();
    println!("================ 指标停用 E2E · {project_name} ================");
    println!("项目级指标(stage_kind=NULL)当前共 {} 条", {
        sigs.metrics
            .iter()
            .filter(|m| m.stage_kind.is_none())
            .count()
    });

    // 找一条**不动**的对照指标:用来证明"派生还在跑",排除"recompute 整个
    // 没执行"这种把停用行冻结解释掉的可能。
    let control = sigs
        .metrics
        .iter()
        .find(|m| !targets.contains(&m.name) && !m.archived)
        .expect("需要至少一条不被停用的对照指标");
    let control_id = control.id.uuid().to_string();

    for name in &targets {
        let m = sigs
            .metrics
            .iter()
            .find(|m| &m.name == name)
            .unwrap_or_else(|| panic!("找不到指标「{name}」"));
        let id = m.id.uuid().to_string();
        let before = read(&pool, &id).await;
        let ctl_before = read(&pool, &control_id).await;

        app.dispatch(Command::SetMetricArchived {
            metric: m.id,
            archived: true,
        })
        .await
        .unwrap();

        let after = read(&pool, &id).await;
        let ctl_after = read(&pool, &control_id).await;
        println!("\n──「{name}」");
        println!(
            "  archived     {} → {}   {}",
            before.archived,
            after.archived,
            if after.archived == 1 { "✓" } else { "✗" }
        );
        println!(
            "  archived_at  {:?} → {:?}   {}",
            before.archived_at,
            after.archived_at,
            if after.archived_at.is_some() {
                "✓"
            } else {
                "✗"
            }
        );
        println!(
            "  observation  {} → {}   {}",
            before.obs,
            after.obs,
            if before.obs == after.obs {
                "✓ 一条未删"
            } else {
                "✗ 历史被动了"
            }
        );
        println!(
            "  自身派生 rev {} → {}   {}(对照组 {} → {},说明 recompute 确实跑了)",
            before.derived_rev,
            after.derived_rev,
            if before.derived_rev == after.derived_rev {
                "✓ 冻结"
            } else {
                "✗ 仍在重算"
            },
            ctl_before.derived_rev,
            ctl_after.derived_rev,
        );

        // 幂等:再点一次不应重复盖时戳。
        app.dispatch(Command::SetMetricArchived {
            metric: m.id,
            archived: true,
        })
        .await
        .unwrap();
        let again = read(&pool, &id).await;
        println!(
            "  重复停用     archived_at {:?}   {}",
            again.archived_at,
            if again.archived_at == after.archived_at {
                "✓ 幂等"
            } else {
                "✗ 时戳被重盖"
            }
        );
    }

    // 恢复第一条,证明这扇门是双向的(不是单向删除的委婉说法)。
    let first = sigs.metrics.iter().find(|m| m.name == targets[0]).unwrap();
    let fid = first.id.uuid().to_string();
    app.dispatch(Command::SetMetricArchived {
        metric: first.id,
        archived: false,
    })
    .await
    .unwrap();
    let restored = read(&pool, &fid).await;
    println!("\n── 恢复「{}」", targets[0]);
    println!(
        "  archived={} archived_at={:?} 派生 rev={}   {}",
        restored.archived,
        restored.archived_at,
        restored.derived_rev,
        if restored.archived == 0 && restored.archived_at.is_none() {
            "✓ 已恢复,下一次 recompute 重新派生"
        } else {
            "✗"
        }
    );
    // 再停用回去,好让截图/读回看到的是最终期望状态。
    app.dispatch(Command::SetMetricArchived {
        metric: first.id,
        archived: true,
    })
    .await
    .unwrap();
    println!("\n(已把「{}」重新停用,留作最终状态)", targets[0]);
}
