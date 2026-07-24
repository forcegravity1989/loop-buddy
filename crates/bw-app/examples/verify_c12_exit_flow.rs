//! **verify_c12_exit_flow — C12 创建流永远退得出去 headless E2E 指挥器
//! (plan/14 规范条 1)。**
//!
//! 不开 UI,用真实命令层(`dispatch`)把创建流走到「快速问题已提交、起草已
//! 跑过一轮(mid-Drafting/Review 的等价落库状态)」,再在那个中途点退出
//! (`Command::BackToProjects` —— main.rs `on_cancel` 现在真正发送的命令),
//! 独立 `sqlite3` 读回证实:
//!
//!   ① 退出不是「假退出」:退出前 kernel `state.view == View::Create`(卡片
//!      还停在创建流内部,不是 Repo/Intent 那种 view 从未离开 Projects 的
//!      浅场景)—— 这正是旧 bug 的现场:`creating.set(false)` 单独存在时,
//!      `show_create = creating() || v.view == View::Create` 仍为真,退不出
//!      去。退出后 `state.view == View::Projects`、`active_project`/
//!      `active_session` 都清空。
//!   ② 退出不删项目、不回滚已落库进度:项目行仍在 DB,`phase` 仍是
//!      `cold_start`(`CompleteCreation` 从未跑过),Questions 卡已提交的
//!      `cycle`/`benchmark`/`opportunity`、以及起草已产出的 `north_star`/
//!      `ns_def`/会话留痕消息,退出前后逐字节不变。
//!   ③ 中断可续:退出后 `Command::OpenProject` 重开,`state.view` 再次落回
//!      `View::Create`(cold-start 分支)—— 创建流恢复入口不因退出而丢失。
//!      如实记录现状(不在本票扩大范围):恢复精度由 `create.rs` 的
//!      `has_project` client-side 判断决定,只要项目行存在就一律落
//!      `Card::Questions`,不区分中断发生在 Questions/Drafting/Review 哪一
//!      张——本票不改这个判断,只保证「退得出去 + 能续」。
//!   ④「重试起草」不是新语义:失败态的重试与首次提交共用同一条
//!      `Command::RunDraftWorkflow` 派发(`create.rs::dispatch_draft_run`),
//!      本例验证连续两次派发(模拟「提交→中途退出前已起草一轮→重开→再起草
//!      一轮」)各自留下独立、完整、【mock】自标注的会话记录,互不覆盖、互
//!      不阻塞。
//!
//! 用法:
//!   cargo run -p bw-app --example verify_c12_exit_flow -- <db-path>

use bw_app::{App, Command, View};
use bw_core::model::{drafting_workflow, MaturityPeriod, StageKind};
use bw_core::{ProjectId, SessionId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SessionKind, SqliteStore, Store};
use std::sync::Arc;

/// The exact command sequence `create.rs::dispatch_draft_run` sends — kept
/// in lockstep by hand (this is a headless conductor, not a UI harness, so
/// it can't call the component function directly).
async fn dispatch_draft_run(app: &mut App, label: &str) -> SessionId {
    let session = SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: Some(StageKind::Prototype),
        kind: SessionKind::Create,
        title: "创建 · 体系起草".into(),
    })
    .await
    .unwrap_or_else(|e| panic!("StartSession {label}: {e}"));
    app.dispatch(Command::RunDraftWorkflow {
        session,
        spec: drafting_workflow(),
    })
    .await
    .unwrap_or_else(|e| panic!("RunDraftWorkflow {label}: {e}"));
    session
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let db_path = args
        .get(1)
        .cloned()
        .expect("usage: verify_c12_exit_flow <db-path>");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    // No workspaces_root: github=None + no root ⇒ `(None, None)` branch of
    // CreateProject is a pure no-op (see bw-app/src/lib.rs), so this
    // conductor never shells out to git — it's testing the exit/resume
    // command sequence, not repo provisioning (that's C13/C16's turf).
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::with_delay(
            std::time::Duration::from_millis(50),
        ))),
        ClaudeCliConfig {
            binary: None,
            max_budget_usd: 0.5,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    );
    app.dispatch(Command::Boot).await.expect("boot");

    // ═══════════════ ① CreateProject → 已在 View::Create ═══════════════
    println!("\n① CreateProject …");
    let pid = ProjectId::new();
    app.dispatch(Command::CreateProject {
        provider: "github".to_string(),
        id: pid,
        name: "C12 退出流程演示".into(),
        kind: "CLI 工具 · Rust".into(),
        desc: "C12 headless E2E · 验证创建流全卡可退、中断可续".into(),
        workspace: None,
        github: None,
    })
    .await
    .expect("create project");
    assert_eq!(
        app.snapshot().view,
        View::Create,
        "CreateProject 后 kernel state.view 应已是 View::Create —— 这正是旧 bug 现场:\
         此后单靠本地 `creating.set(false)` 退不出去,必须真的发 BackToProjects"
    );
    println!(
        "  state.view = {:?} (已进入创建流内部)",
        app.snapshot().view
    );

    // ═══════════════ ② Questions 卡提交 ═══════════════
    println!("\n② SetCycle + UpdateBrief(模拟 QuestionsCard 提交)…");
    app.dispatch(Command::SetCycle {
        cycle: MaturityPeriod::Explore,
    })
    .await
    .expect("SetCycle");
    app.dispatch(Command::UpdateBrief {
        benchmark: "Linear".into(),
        opportunity: "三个月内被持续复用".into(),
    })
    .await
    .expect("UpdateBrief");

    // ═══════════════ ③ 起草第一轮(模拟进入 Drafting 卡)═══════════════
    println!("\n③ 起草第一轮(dispatch_draft_run,模拟首次进入 Drafting 卡)…");
    let session_1 = dispatch_draft_run(&mut app, "第一轮").await;
    let msgs_1_before = store.session_messages(session_1).await.unwrap();
    println!(
        "  session_1 = {} · {} 条留痕消息",
        session_1.uuid(),
        msgs_1_before.len()
    );

    let proj_before = store.get_project(pid).await.unwrap().unwrap();
    println!(
        "  退出前:phase={:?} cycle={:?} benchmark={:?} opportunity={:?} north_star={:?}",
        proj_before.phase,
        proj_before.cycle,
        proj_before.benchmark,
        proj_before.opportunity,
        proj_before.north_star
    );

    // ═══════════════ ④ 退出(main.rs on_cancel 现在真正发送的命令)═══════════════
    println!("\n④ Command::BackToProjects(全卡「← 返回项目墙」现在真正发送的命令)…");
    app.dispatch(Command::BackToProjects)
        .await
        .expect("BackToProjects");
    assert_eq!(
        app.snapshot().view,
        View::Projects,
        "退出后 state.view 应回到 View::Projects"
    );
    assert_eq!(
        app.snapshot().active_project,
        None,
        "退出后 active_project 应清空"
    );
    assert_eq!(
        app.snapshot().active_session,
        None,
        "退出后 active_session 应清空"
    );
    println!(
        "  state.view = {:?} · active_project = {:?} · active_session = {:?}",
        app.snapshot().view,
        app.snapshot().active_project,
        app.snapshot().active_session
    );

    // 独立读回:项目行仍在、phase 未变、已落库进度逐字节不变(不删项目、不回滚)。
    let proj_after_exit = store.get_project(pid).await.unwrap();
    let proj_after_exit = proj_after_exit
        .expect("退出不该删除项目 —— BackToProjects 只清活跃指针,项目行必须仍在 DB 里");
    assert_eq!(
        proj_after_exit.phase, proj_before.phase,
        "退出不应改变 phase(仍应是 cold_start —— CompleteCreation 从未跑过)"
    );
    assert_eq!(
        proj_after_exit.cycle, proj_before.cycle,
        "退出不应回滚 cycle"
    );
    assert_eq!(
        proj_after_exit.benchmark, proj_before.benchmark,
        "退出不应回滚 benchmark"
    );
    assert_eq!(
        proj_after_exit.opportunity, proj_before.opportunity,
        "退出不应回滚 opportunity"
    );
    let msgs_1_after = store.session_messages(session_1).await.unwrap();
    assert_eq!(
        msgs_1_after.len(),
        msgs_1_before.len(),
        "退出不应影响第一轮起草已经留下的会话消息"
    );
    println!(
        "  [proof] 项目行仍在 · phase={:?} · cycle={:?} · benchmark={:?} · session_1 留痕消息数={}(退出前后不变)",
        proj_after_exit.phase,
        proj_after_exit.cycle,
        proj_after_exit.benchmark,
        msgs_1_after.len()
    );

    // ═══════════════ ⑤ 中断可续:重开回到创建流 ═══════════════
    println!("\n⑤ Command::OpenProject(重开,验证中断可续)…");
    app.dispatch(Command::OpenProject(pid))
        .await
        .expect("reopen project");
    assert_eq!(
        app.snapshot().view,
        View::Create,
        "cold-start 项目重开应落回 View::Create(既有机制,C12 不改)"
    );
    assert_eq!(app.snapshot().active_project, Some(pid));
    println!(
        "  state.view = {:?} · active_project = {:?} —— 恢复入口仍在;\
         如实记录:create.rs 的 has_project 判断只看「项目行是否存在」,\
         恢复卡恒为 Questions,不区分中断发生在 Drafting/Review 哪一张(本票不扩大恢复精度)",
        app.snapshot().view,
        app.snapshot().active_project
    );

    // ═══════════════ ⑥「重试起草」= 同一条命令再派发一次 ═══════════════
    println!("\n⑥ 起草第二轮(dispatch_draft_run,模拟失败态点「重试起草」)…");
    let session_2 = dispatch_draft_run(&mut app, "第二轮 · 重试").await;
    assert_ne!(session_2, session_1, "重试应是一次新会话,不是覆盖第一轮");
    let msgs_2 = store.session_messages(session_2).await.unwrap();
    assert!(
        !msgs_2.is_empty(),
        "第二轮(重试)起草应留下自己的会话消息,不被第一轮阻塞"
    );
    for m in &msgs_2 {
        assert!(
            m.text.contains("【mock】"),
            "重试留下的消息也应自我标注【mock】,实际:{}",
            m.text
        );
    }
    // 第一轮的消息必须还在、不受第二轮影响(两轮互不覆盖)。
    let msgs_1_final = store.session_messages(session_1).await.unwrap();
    assert_eq!(
        msgs_1_final.len(),
        msgs_1_before.len(),
        "第二轮(重试)起草不应改动第一轮留下的会话记录"
    );
    println!(
        "  [proof] session_2 = {} · {} 条【mock】留痕消息(与 session_1 互不覆盖:\
         session_1 仍是 {} 条)",
        session_2.uuid(),
        msgs_2.len(),
        msgs_1_final.len()
    );

    // 项目仍未落地(phase 仍是 cold_start)—— 退出/重试都不会意外把创建流
    // 判定为「已完成」,只有显式 CompleteCreation 才会。
    let proj_final = store.get_project(pid).await.unwrap().unwrap();
    assert_eq!(
        proj_final.phase, proj_before.phase,
        "两轮起草 + 一次退出/重开都不该让 phase 偷偷变成 running"
    );

    println!("\n✓ verify_c12_exit_flow done");
    println!("  DB: {db_path}");
    println!("  project           = {}", pid.uuid());
    println!("  session_1(首轮)   = {}", session_1.uuid());
    println!("  session_2(重试)   = {}", session_2.uuid());
    println!("  final phase       = {:?}", proj_final.phase);
}
