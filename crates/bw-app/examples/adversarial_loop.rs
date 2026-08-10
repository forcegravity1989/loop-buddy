//! **adversarial_loop — T9 对抗式评审运行时的 headless E2E 指挥器。**
//!
//! plan/12 §4 的对抗小 loop 全真跑通:Evaluator 阶段跑完 → 解析其真实输出的
//! 结构化裁决(PhaseOutcome)→ 通过继续 / 打回重跑 / 打满上限转 Blocked。
//! 全程用**可脚本化的 MockExecutor**(产出自我标注【mock】),不碰真 claude(网关
//! 抖动),每条路径结束都从 store 读回并打印证据(报告不代答)。
//!
//! V1-TermClose3:issue ▶跑 全走嵌入终端后,RunIssue 不再走阶段循环机器。A/B/C
//! 三路从 RunIssue retarget 到 RunStagePlaybook(阶段循环机器仍在,对抗循环还在
//! 那里);D 路用 RunWorkflow(自定义 spec,静态轨)。断言改读 workflow_run 行
//! (按 session 过滤),不再断言 issue 状态(RunStagePlaybook 无 issue)。
//!
//! 四条路径(全走阶段循环,非 issue 终端):
//!   A 两轮打回后通过(Dynamic 轨,评审真实提议目标)→ 多轮 run 行
//!   B 打满上限(Dynamic 轨,每轮打回)→ 3 轮 run 行
//!   C 解析失败=诚实失败(评审无裁决块)→ run 行 failed 且原因如实
//!   D 静态轨打回目标覆盖(评审提议越界目标,按声明目标打回)+ 评审后尾阶段
//!
//! 用法:adversarial_loop <db-path>
//!   跑完后用 sqlite3 <db> 独立复核(读回为证)。

use bw_app::{App, Command};
use bw_core::model::{
    HubSource, LoopConfig, Maturity, PhaseMeta, PhaseRole, StageKind, WorkflowKind, WorkflowSpec,
};
use bw_core::{ProjectId, SessionId, WorkflowId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{NewSession, SessionKind, SqliteStore, Store};
use std::sync::Arc;

/// The Build stage's Evaluator phase name (`kind.method_loop()` 最后一项) — the
/// gate the stage playbook drives, so this is the phase we script.
const BUILD_GATE: &str = "评审合入 · CI 门禁";

/// Build a fresh App wrapping a scripted mock over the shared store, then Boot.
async fn app_with_script(store: Arc<dyn Store>, script: Vec<(String, Vec<String>)>) -> App {
    let mut app = App::new(
        store,
        Engine::new(Arc::new(MockExecutor::scripted(script))),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.expect("boot");
    app
}

/// Print every workflow_run row for a session, oldest→newest (read-back).
/// V1-TermClose3:RunStagePlaybook 无 issue,改按 session_id 过滤 workflow_run 行。
async fn dump_stage_runs(store: &Arc<dyn Store>, session: SessionId, label: &str) {
    let mut runs: Vec<_> = store
        .list_all_workflow_runs(1000)
        .await
        .expect("runs")
        .into_iter()
        .filter(|r| r.session_id == Some(session))
        .collect();
    runs.reverse(); // list is newest-first; show in round order
    println!("  [{label}] {} 轮 run 行:", runs.len());
    for (i, r) in runs.iter().enumerate() {
        println!(
            "      round {} · status={} · phases={} · error={:?}",
            i + 1,
            r.status.text(),
            r.phases_completed,
            if r.error.is_empty() {
                None
            } else {
                Some(&r.error)
            }
        );
    }
}

/// Mint a real session row (RunStagePlaybook/RunWorkflow append messages, so
/// the FK target must exist first).
async fn new_session(store: &Arc<dyn Store>, pid: ProjectId, title: &str) -> SessionId {
    let session = SessionId::new();
    store
        .ensure_session(NewSession {
            id: session,
            project_id: pid,
            stage_kind: Some(StageKind::Build),
            kind: SessionKind::Optimize,
            title: title.into(),
            snippet: String::new(),
        })
        .await
        .expect("ensure session");
    session
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let db = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: adversarial_loop <db-path>");
        std::process::exit(2);
    });

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.expect("open db"));

    // One shared project (mock mode — no workspace), created once.
    let pid = ProjectId::new();
    {
        let mut app = app_with_script(store.clone(), vec![]).await;
        app.dispatch(Command::CreateProject {
            provider: "github".to_string(),
            id: pid,
            name: "T9 对抗演示项目".into(),
            kind: "demo".into(),
            desc: "adversarial loop E2E".into(),
            workspace: None,
            github: None,
            codehub: None,
        })
        .await
        .expect("create project");
    }

    println!("== T9 对抗式评审运行时 · headless E2E ==\n");

    // ── Path A: 两轮打回后通过(Dynamic 轨)────────────────────────────────
    {
        let mut app = app_with_script(
            store.clone(),
            vec![(
                BUILD_GATE.to_string(),
                vec![
                    "【mock】评审:实现有缺陷,打回重做\nVERDICT: REJECT_TO_PHASE=2\nREASON: 单测未覆盖边界条件".to_string(),
                    "【mock】评审:本轮达标,放行\nVERDICT: PASS\nREASON: 【mock】覆盖率与门禁均通过".to_string(),
                ],
            )],
        )
        .await;
        app.dispatch(Command::OpenProject(pid)).await.expect("open");
        let session = new_session(&store, pid, "A · 两轮打回后通过").await;
        app.dispatch(Command::RunStagePlaybook {
            session,
            stage_kind: StageKind::Build,
        })
        .await
        .expect("run stage playbook A");
        println!("Path A · 两轮打回后通过(Dynamic 轨,评审提议目标=2):");
        dump_stage_runs(&store, session, "A").await;
        println!();
    }

    // ── Path B: 打满上限(Dynamic 轨)──────────────────────────────────────
    {
        let mut app = app_with_script(
            store.clone(),
            vec![(
                BUILD_GATE.to_string(),
                // max_iter=3 ⇒ 评审最多跑 3 次;第 3 次打回即触上限。
                (1..=3)
                    .map(|n| {
                        format!(
                            "【mock】评审第 {n} 轮:仍不达标\nVERDICT: REJECT_TO_PHASE=2\nREASON: 覆盖率仍不足,需继续补测"
                        )
                    })
                    .collect(),
            )],
        )
        .await;
        app.dispatch(Command::OpenProject(pid)).await.expect("open");
        let session = new_session(&store, pid, "B · 打满上限").await;
        app.dispatch(Command::RunStagePlaybook {
            session,
            stage_kind: StageKind::Build,
        })
        .await
        .expect("run stage playbook B（触上限是正常终态,非错误）");
        println!("Path B · 打满上限(Dynamic 轨,每轮打回):");
        dump_stage_runs(&store, session, "B").await;
        println!();
    }

    // ── Path C: 解析失败=诚实失败 ────────────────────────────────────────
    {
        let mut app = app_with_script(
            store.clone(),
            vec![(
                BUILD_GATE.to_string(),
                vec!["【mock】评审完成,但本轮忘了给出结构化裁决块(演示解析失败)".to_string()],
            )],
        )
        .await;
        app.dispatch(Command::OpenProject(pid)).await.expect("open");
        let session = new_session(&store, pid, "C · 评审输出缺裁决").await;
        let res = app
            .dispatch(Command::RunStagePlaybook {
                session,
                stage_kind: StageKind::Build,
            })
            .await;
        println!(
            "Path C · 解析失败=诚实失败(RunStagePlaybook 返回 = {}):",
            if res.is_err() {
                "Err(诚实失败)"
            } else {
                "Ok"
            }
        );
        dump_stage_runs(&store, session, "C").await;
        println!();
    }

    // ── Path D: 静态轨打回目标覆盖 + 尾阶段 ──────────────────────────────
    {
        let spec = WorkflowSpec {
            id: WorkflowId::new(),
            name: "静态轨对抗演示 workflow".into(),
            kind: WorkflowKind::Static {
                maturity: Maturity::Mature,
                version: 1,
                uses: 0,
                scope: "demo".into(),
                source: HubSource::SelfBuilt,
                trigger: None,
            },
            prompt: "静态轨:评审提议被忽略,按声明目标打回".into(),
            goal: "演示 Static reject_to_phase 覆盖 agent 提议".into(),
            stage_ref: None,
            phases: vec![
                PhaseMeta {
                    name: "实现".into(),
                    role: PhaseRole::Generator,
                    reject_to_phase: None,
                    agent: None,
                    skills: vec![],
                },
                PhaseMeta {
                    name: "评审".into(),
                    role: PhaseRole::Evaluator,
                    // 声明打回到「实现」(0);运行时评审会提议一个越界目标,
                    // 静态轨必须忽略提议、按此声明打回。
                    reject_to_phase: Some(0),
                    agent: None,
                    skills: vec![],
                },
                PhaseMeta {
                    name: "合入".into(),
                    role: PhaseRole::Neutral,
                    reject_to_phase: None,
                    agent: None,
                    skills: vec![],
                },
            ],
            phase_prompts: vec![],
            agents: vec![],
            skills: vec![],
            loop_config: LoopConfig {
                retries: 1,
                max_iter: 3,
            },
            project_id: Some(pid),
            content: String::new(),
        };
        let wid = spec.id;
        let session = SessionId::new();
        // RunWorkflow 走 append_message,需要真实 session 行。
        store
            .ensure_session(NewSession {
                id: session,
                project_id: pid,
                stage_kind: None,
                kind: SessionKind::Optimize,
                title: "D · 静态轨演示".into(),
                snippet: String::new(),
            })
            .await
            .expect("ensure session");

        let mut app = app_with_script(
            store.clone(),
            vec![(
                "评审".to_string(),
                vec![
                    // 评审提议越界目标 99;静态轨须忽略、按声明 0 打回。
                    "【mock】评审:打回(提议一个越界目标以验证覆盖)\nVERDICT: REJECT_TO_PHASE=99\nREASON: 静态轨演示:提议目标应被忽略".to_string(),
                    "【mock】评审:第二轮通过\nVERDICT: PASS\nREASON: 【mock】静态轨演示通过".to_string(),
                ],
            )],
        )
        .await;
        app.dispatch(Command::OpenProject(pid)).await.expect("open");
        app.dispatch(Command::RunWorkflow { session, spec })
            .await
            .expect("run workflow D");

        let mut runs = store.list_workflow_runs(wid).await.expect("runs");
        runs.reverse();
        println!("Path D · 静态轨打回目标覆盖(评审提议 99,声明打回=0)+ 尾阶段「合入」:");
        println!("    ↳ {} 轮 run 行:", runs.len());
        for (i, r) in runs.iter().enumerate() {
            println!(
                "      round {} · status={} · phases={} · error={:?}",
                i + 1,
                r.status.text(),
                r.phases_completed,
                if r.error.is_empty() {
                    None
                } else {
                    Some(&r.error)
                }
            );
        }
        // 会话消息里「实现」出现两次(两轮都从声明目标 0 重跑)、「合入」出现一次
        // (仅通过后的尾阶段跑)= 静态覆盖 + 尾阶段的读回证据。
        let msgs = store.session_messages(session).await.expect("messages");
        let count = |needle: &str| msgs.iter().filter(|m| m.text.contains(needle)).count();
        println!(
            "    ↳ 会话消息读回:「实现」×{} · 「评审」×{} · 「合入」×{}",
            count("阶段「实现」"),
            count("阶段「评审」"),
            count("阶段「合入」")
        );
        println!();
    }

    println!("== 读回完毕。用 sqlite3 {db} 独立复核 workflow_run 表 ==");
}
