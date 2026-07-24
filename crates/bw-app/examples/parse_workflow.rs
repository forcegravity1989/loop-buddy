//! **parse_workflow — T17「解析为流程图」headless E2E 指挥器.**
//!
//! plan/12 §10 v1.1#4:Workflow 详情页「🔍 解析为流程图」按钮的真实执行路径——
//! 读 `WorkflowSpec.content`,经 Engine/Executor(本例用可脚本化 MockExecutor,
//! 产出自我标注【mock】)真实调用一次,严格解析末次
//! `WORKFLOW_PHASES_BEGIN…END` 契约块为 `Vec<PhaseMeta>`;成功先留
//! `WorkflowVersion` 快照再覆盖 `phases`,失败(缺契约块/字段越界)诚实报错、
//! `phases` 原封不动——绝不猜、绝不部分采纳。三条路径全部从 store 读回验证
//! (报告不代答)。`Command::ParseWorkflowContent` 不要求 active project(Hub
//! 级动作,`BW_SEL` 可以在没有打开项目的情况下直接深链进详情页),本例也全程
//! 不建项目、不 OpenProject,直接验证这条设计判断。
//!
//! 三条路径:
//!   a 带评审打回语义的真实多阶段文档 → mock 给出合法契约块 → phases=解析结果、
//!     workflow_version 快照 +1
//!   b mock 输出没有契约块(忘了按格式收尾)→ phases 未动、诚实错误原因
//!   c 契约块合法但 reject_to_phase 越界 → 诚实拒绝、phases 未动
//!
//! 用法:parse_workflow <db-path>
//!   跑完后用 sqlite3 <db> 独立复核 workflow_spec / workflow_version 表。

use bw_app::{App, Command};
use bw_core::model::{HubSource, LoopConfig, Maturity, WorkflowKind};
use bw_core::WorkflowId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{NewWorkflowSpec, SqliteStore, Store};
use std::sync::Arc;

/// The literal phase name `bw-app`'s `App::parse_workflow_content` builds its
/// ad-hoc `PhaseNode` with — the key `MockExecutor::scripted` matches on.
const PARSE_PHASE: &str = "解析工作流文档";

/// A real multi-phase workflow document, including reject/review semantics
/// (起草 → 评审 打回起草 → 定稿) — this is what the parse call actually reads,
/// not a placeholder string.
const SAMPLE_CONTENT: &str = "\
# 内容审校工作流

一份真实的多阶段工作流文档,包含评审打回语义(用于 T17 解析验证):

## 1. 起草
撰写初稿,产出交给评审阶段核验。

## 2. 评审
检查初稿质量;不达标则打回「起草」阶段重写,达标则放行进入「定稿」。

## 3. 定稿
评审通过后整理、发布最终版本。
";

async fn app_with_script(store: Arc<dyn Store>, script: Vec<(String, Vec<String>)>) -> App {
    let mut app = App::new(
        store,
        Engine::new(Arc::new(MockExecutor::scripted(script))),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.expect("boot");
    app
}

/// Seed one Static hub workflow with real `content` and empty `phases` —
/// store-level (bypasses `Command::CreateWorkflowSpec`, which has no
/// content-authoring param yet — T16's "还没有正文录入 UI" note, plan/12
/// §10 v1.1#3). This is exactly the shape T17's parse action targets: a
/// persisted workflow whose real MD document hasn't been structured yet.
async fn seed_content_workflow(store: &Arc<dyn Store>, name: &str, content: &str) -> WorkflowId {
    let id = WorkflowId::new();
    store
        .create_workflow_spec(NewWorkflowSpec {
            id,
            name: name.to_string(),
            kind: WorkflowKind::Static {
                maturity: Maturity::Fresh,
                version: 1,
                uses: 0,
                scope: String::new(),
                source: HubSource::SelfBuilt,
                trigger: None,
            },
            prompt: "(未使用 · 解析动作只读 content)".into(),
            goal: "T17 解析 E2E 演示".into(),
            stage_ref: None,
            phases: vec![],
            phase_prompts: vec![],
            agents: vec![],
            skills: vec![],
            loop_config: LoopConfig {
                retries: 1,
                max_iter: 3,
            },
            // Hub 级、无所属项目——验证 ParseWorkflowContent 不要求
            // active project 的设计判断（本例全程不 CreateProject）。
            project_id: None,
            content: content.to_string(),
        })
        .await
        .expect("seed workflow");
    id
}

async fn dump_phases(store: &Arc<dyn Store>, id: WorkflowId, label: &str) {
    let spec = store
        .get_workflow_spec(id)
        .await
        .expect("get")
        .expect("row");
    println!("  [{label}] phases({}):", spec.phases.len());
    for (i, p) in spec.phases.iter().enumerate() {
        println!(
            "    {i}: name={:?} role={:?} reject_to_phase={:?} agent={:?} skills={:?}",
            p.name, p.role, p.reject_to_phase, p.agent, p.skills
        );
    }
    let versions = store.list_workflow_versions(id).await.expect("versions");
    println!("  [{label}] workflow_version 行数 = {}", versions.len());
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let db = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: parse_workflow <db-path>");
        std::process::exit(2);
    });
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.expect("open db"));

    println!("== T17 解析为流程图 · headless E2E ==\n");

    // ── Path a: scripted 成功路径 ────────────────────────────────────────
    {
        let wid = seed_content_workflow(&store, "a · 成功解析", SAMPLE_CONTENT).await;
        let before_versions = store
            .list_workflow_versions(wid)
            .await
            .expect("versions")
            .len();
        let contract_output = "【mock】已读文档,识别出三个阶段(含评审打回语义)。\n\
WORKFLOW_PHASES_BEGIN\n\
{\"phases\":[\
{\"name\":\"起草\",\"role\":\"generator\",\"reject_to_phase\":null,\"agent\":\"writer\",\"skills\":[\"drafting\"]},\
{\"name\":\"评审\",\"role\":\"evaluator\",\"reject_to_phase\":0,\"agent\":null,\"skills\":[]},\
{\"name\":\"定稿\",\"role\":\"optimizer\",\"reject_to_phase\":null,\"agent\":null,\"skills\":[]}\
]}\n\
WORKFLOW_PHASES_END\n"
            .to_string();
        let mut app = app_with_script(
            store.clone(),
            vec![(PARSE_PHASE.to_string(), vec![contract_output])],
        )
        .await;
        let res = app
            .dispatch(Command::ParseWorkflowContent { workflow_id: wid })
            .await;
        println!(
            "Path a · scripted 成功路径(dispatch = {}):",
            if res.is_ok() {
                "Ok"
            } else {
                "Err(不应发生)"
            }
        );
        res.expect("path a should succeed");
        dump_phases(&store, wid, "a").await;
        let after_versions = store
            .list_workflow_versions(wid)
            .await
            .expect("versions")
            .len();
        println!("  [a] workflow_version 计数:{before_versions} → {after_versions}(应 +1)\n");
        assert_eq!(after_versions, before_versions + 1, "版本快照应 +1");
        let spec = store
            .get_workflow_spec(wid)
            .await
            .expect("get")
            .expect("row");
        assert_eq!(spec.phases.len(), 3, "应解析出 3 个阶段");
        assert!(
            spec.phase_prompts.is_empty(),
            "phase_prompts 应置空(共享 prompt)"
        );
    }

    // ── Path b: 输出无契约块 → 诚实失败,phases 未动 ──────────────────────
    {
        let wid = seed_content_workflow(&store, "b · 缺契约块", SAMPLE_CONTENT).await;
        let mut app = app_with_script(
            store.clone(),
            vec![(
                PARSE_PHASE.to_string(),
                vec!["【mock】看过文档了,但这次忘了按格式给出 phases 块。".to_string()],
            )],
        )
        .await;
        let res = app
            .dispatch(Command::ParseWorkflowContent { workflow_id: wid })
            .await;
        println!(
            "Path b · 输出缺契约块(dispatch = {}):",
            if res.is_err() {
                "Err(诚实失败,预期)"
            } else {
                "Ok(不应发生)"
            }
        );
        match &res {
            Err(e) => println!("  [b] 真实错误原因:{e}"),
            Ok(_) => panic!("path b should fail honestly"),
        }
        dump_phases(&store, wid, "b").await;
        let spec = store
            .get_workflow_spec(wid)
            .await
            .expect("get")
            .expect("row");
        assert!(spec.phases.is_empty(), "解析失败 phases 必须原封不动");
        println!();
    }

    // ── Path c: reject_to_phase 越界 → 诚实拒绝,phases 未动 ──────────────
    {
        let wid = seed_content_workflow(&store, "c · 越界打回目标", SAMPLE_CONTENT).await;
        let contract_output = "WORKFLOW_PHASES_BEGIN\n\
{\"phases\":[\
{\"name\":\"起草\",\"role\":\"generator\",\"reject_to_phase\":null,\"agent\":null,\"skills\":[]},\
{\"name\":\"评审\",\"role\":\"evaluator\",\"reject_to_phase\":9,\"agent\":null,\"skills\":[]}\
]}\n\
WORKFLOW_PHASES_END\n"
            .to_string();
        let mut app = app_with_script(
            store.clone(),
            vec![(PARSE_PHASE.to_string(), vec![contract_output])],
        )
        .await;
        let res = app
            .dispatch(Command::ParseWorkflowContent { workflow_id: wid })
            .await;
        println!(
            "Path c · reject_to_phase 越界(数组只 2 项却给 9)(dispatch = {}):",
            if res.is_err() {
                "Err(诚实拒绝,预期)"
            } else {
                "Ok(不应发生)"
            }
        );
        match &res {
            Err(e) => println!("  [c] 真实错误原因:{e}"),
            Ok(_) => panic!("path c should fail honestly"),
        }
        dump_phases(&store, wid, "c").await;
        let spec = store
            .get_workflow_spec(wid)
            .await
            .expect("get")
            .expect("row");
        assert!(spec.phases.is_empty(), "越界拒绝后 phases 必须原封不动");
        println!();
    }

    println!("== 读回完毕。用 sqlite3 {db} 独立复核 workflow_spec / workflow_version 表 ==");
}
