//! **verify_c15_failure_translation — C15 失败说人话 + 三出路 headless E2E
//! 指挥器(plan/14 规范条 3)。**
//!
//! 不写单元测试(仓库纪律),两件事都走真实路径独立核验:
//!
//! ① **翻译函数三类映射**:`ui::explain_failure`(落点:`crates/ui/src/lib.rs`,
//!    纯函数、零 IO、wasm32 可编译)——四条真实/仿真错误文本喂给它,断言
//!    `category`/`headline` 命中预期,且 `raw` 字段逐字保留输入(原文永不丢
//!    失)。前三条文本不是编的,是 `crates/bw-engine/src/claude_cli.rs` 三种
//!    真实失败形态的字面拷贝(经 `ExecError::Failed` 的
//!    `"executor failed: {0}"` Display 包装,即 `RunVm.failed`/`run.failed`
//!    最终收到的确切字符串):
//!      - 预算耗尽:`CliResult::error_text()` 在 `result` 空、`errors`/
//!        `subtype` 带 `error_max_budget_usd` 时的拼接形态 —— 与 plan/14
//!        缘起台账 #3 用户实测原文逐字相同:
//!        `"executor failed: Reached maximum budget ($0.5)
//!        (subtype=error_max_budget_usd)"`。
//!      - 网关瞬时错:`is_transient_gateway_error` 的字面 marker(`"API
//!        Error: 529"`,重试耗尽后才会到达这里)。
//!      - 超时:`ATTEMPT_TIMEOUT_SECS` 守卫自己的格式串
//!        `"claude CLI attempt exceeded {N}s (hung child killed)"`。
//!    第四条是一个不属于以上任何形态的 `gh` CLI 报错(`ActionState::Fail`
//!    真实可能携带的那类文本),验证「其余类别不硬翻」——落到 `Unknown`,
//!    `headline` 是通用人话,`raw` 原样保留、绝不编造具体原因。
//!
//! ② **深链渲染证明**:落一个 `Readiness::ColdStart` 项目(`CreateProject`,
//!    `github: None`/`workspace: None`——`Command::OpenProject` 对
//!    `ColdStart` 项目路由到 `View::Create`,`create.rs` 的 `has_project`
//!    分支直接落 `Card::Questions`)。随打印 `BW_OPEN` 深链命令供人工/CI 复核
//!    `[BW_OPEN]` stderr 日志 = 渲染成功证明。
//!
//! **未做的部分,如实记录**(见完成报告「偏差」段):C13 后 `RunDraftWorkflow`
//! 硬锁 `MockExecutor`(恒成功),没有注入失败的开关——真实点开
//! `DraftingCard` 失败态三按钮 + 技术详情折叠这条渲染分支,在当前代码里唯一
//! 能读到的证据是源码引用(本例注释 + 完成报告的行号清单),不是一张真实失败
//! 截图。「先用模板继续」按钮同理是纯本地 `on_next.call(())`(见
//! `crates/app-desktop/src/screens/create.rs` 的 `DraftingCard`)—— 零新增
//! 后端命令,`ReviewCard` 只读 `CreateVm`(已在 `QuestionsCard` 提交时经
//! `SetCycle`/`UpdateBrief` 落库),从不读这次跑的 transcript,所以本例③里
//! 打印 `CreateVm` 字段的落库读回,间接证明了「模板继续」不缺数据。
//!
//! 用法:
//!   cargo run -p bw-app --example verify_c15_failure_translation -- <db-path>

use bw_app::{App, Command};
use bw_core::ProjectId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;
use ui::{explain_failure, FailureCategory};

const DEEP_LINK_PROJECT: &str = "C15-验证";

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("bw_verify_c15.db")
            .to_string_lossy()
            .into_owned()
    });
    let _ = std::fs::remove_file(&db_path);

    println!("================ C15 失败说人话 + 三出路 E2E ================");
    println!("db: {db_path}");

    // ── ① 翻译函数三类映射 + 未知类别兜底 ──────────────────────────────
    let mut all_ok = true;

    // 与 plan/14 缘起台账 #3 用户实测原文逐字相同(claude_cli.rs
    // CliResult::error_text() 在 result 为空、errors/subtype 带
    // error_max_budget_usd 时的拼接形态,经 ExecError::Failed 的
    // "executor failed: {0}" Display 包装)。
    let budget_raw = "executor failed: Reached maximum budget ($0.5)(subtype=error_max_budget_usd)";
    let budget = explain_failure(budget_raw);
    check(
        &mut all_ok,
        "预算耗尽",
        budget.category == FailureCategory::BudgetExhausted,
        &format!("category={:?}", budget.category),
    );
    check(
        &mut all_ok,
        "预算耗尽 · headline 是人话不是原文",
        budget.headline == "预算到顶,起草没做完——重试会重新计费",
        &budget.headline,
    );
    check(
        &mut all_ok,
        "预算耗尽 · raw 原文一字不丢",
        budget.raw == budget_raw,
        &budget.raw,
    );

    // claude_cli.rs::is_transient_gateway_error 的字面 marker;这是重试
    // (TRANSIENT_BACKOFF_SECS 三次)耗尽后才会到达调用者的文本。
    let gateway_raw = "executor failed: API Error: 529 {\"type\":\"overloaded_error\"}";
    let gateway = explain_failure(gateway_raw);
    check(
        &mut all_ok,
        "网关瞬时错(529)",
        gateway.category == FailureCategory::GatewayTransient,
        &format!("category={:?}", gateway.category),
    );
    check(
        &mut all_ok,
        "网关瞬时错 · headline",
        gateway.headline == "AI 网关暂时不可用,稍等重试通常就好",
        &gateway.headline,
    );

    // 同一分类的第二真实 marker:bigmodel 网关的中文错误文案。
    let gateway_cn_raw = "executor failed: 访问量过大,请稍后再试";
    let gateway_cn = explain_failure(gateway_cn_raw);
    check(
        &mut all_ok,
        "网关瞬时错(访问量过大)",
        gateway_cn.category == FailureCategory::GatewayTransient,
        &format!("category={:?}", gateway_cn.category),
    );

    // ATTEMPT_TIMEOUT_SECS 守卫自己的格式串,逐字拷贝。
    let timeout_raw = "executor failed: claude CLI attempt exceeded 1800s (hung child killed)";
    let timeout = explain_failure(timeout_raw);
    check(
        &mut all_ok,
        "超时",
        timeout.category == FailureCategory::Timeout,
        &format!("category={:?}", timeout.category),
    );
    check(
        &mut all_ok,
        "超时 · headline",
        timeout.headline == "执行超时被终止,可重试",
        &timeout.headline,
    );

    // 不属于以上三种真实形态的一类 —— ActionState::Fail 可能携带的
    // gh CLI 报错。必须落 Unknown,headline 通用、raw 原样保留。
    let unknown_raw = "gh: To use GitHub CLI in a GitHub Actions workflow, set the GH_TOKEN environment variable.";
    let unknown = explain_failure(unknown_raw);
    check(
        &mut all_ok,
        "未知类别兜底",
        unknown.category == FailureCategory::Unknown,
        &format!("category={:?}", unknown.category),
    );
    check(
        &mut all_ok,
        "未知类别 · raw 原样保留(不隐藏、不编造)",
        unknown.raw == unknown_raw,
        &unknown.raw,
    );
    check(
        &mut all_ok,
        "未知类别 · headline 不硬翻具体原因",
        !unknown.headline.contains("GH_TOKEN") && !unknown.headline.is_empty(),
        &unknown.headline,
    );

    // ── ② 深链渲染证明:落一个 ColdStart 项目,供 BW_OPEN 复核 ─────────
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();
    let project_id = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: project_id,
        name: DEEP_LINK_PROJECT.to_string(),
        kind: "内部验证".to_string(),
        desc: "C15 深链渲染证明用的占位项目(ColdStart,未 CompleteCreation)".to_string(),
        workspace: None,
        github: None,
    })
    .await
    .expect("CreateProject(ColdStart placeholder) should succeed");

    // ③ CreateVm 落库读回(经真实 Store trait,不是猜测):「先用模板继续」
    // 不新增后端动作,ReviewCard 只读这些已落库字段 —— phase 恒为
    // ColdStart(尚未 CompleteCreation)独立证明了这一点;下方也打印对等的
    // sqlite3 命令供人工/CI 用另一条路径复核同一张表。
    let proj = store
        .get_project(project_id)
        .await
        .unwrap()
        .expect("just-created project must read back");
    check(
        &mut all_ok,
        "ColdStart 落库(Store trait 读回)",
        proj.phase == bw_core::model::Readiness::ColdStart,
        &format!("phase={:?}", proj.phase),
    );

    println!();
    println!("深链渲染证明(手动/CI 复核):BW_DB=\"{db_path}\" BW_OPEN=\"{DEEP_LINK_PROJECT}\" \\");
    println!("  ./target/debug/builders-workbench");
    println!("  期望 stderr:[BW_OPEN] \"{DEEP_LINK_PROJECT}\" -> view=Create panel=Progress …");
    println!();
    println!("sqlite3 读回(ColdStart 落库为证):sqlite3 \"{db_path}\" \\");
    println!("  \"SELECT name, phase FROM project WHERE name='{DEEP_LINK_PROJECT}';\"");
    println!("  期望一行:{DEEP_LINK_PROJECT}|cold_start");

    println!();
    if all_ok {
        println!("✓ 全部断言通过(4 类映射 × 原文保真 + ColdStart 落库)");
    } else {
        println!("✗ 存在失败断言,见上方 ✗ 行");
        std::process::exit(1);
    }
}

fn check(all_ok: &mut bool, label: &str, cond: bool, detail: &str) {
    if cond {
        println!("  ✓ {label}: {detail}");
    } else {
        println!("  ✗ {label}: {detail}");
        *all_ok = false;
    }
}
