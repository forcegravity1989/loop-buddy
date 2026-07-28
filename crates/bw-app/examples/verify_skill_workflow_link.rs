//! plan/16 §5 headless E2E · 「创建新工作流可从 SkillHub 选技能」+「反查含
//! phase 绑定」的命令层证明。与 `migrate_legacy_db` 同一纪律:**跑在真实
//! 日常库的一次性副本上**,本例只开给定路径,绝不指原件。
//!
//! 三段验证,全部真实读回,不 mock 断言:
//! 1. 从 Hub 里按名取真实技能(SkillAgentPicker 的 `resolve_refs` 同款
//!    `SkillRef { from: "SkillHub" }`),走真实 `Command::CreateWorkflowSpec`
//!    建一条工作流,再 `list_workflow_specs` 读回 `skills` 非空、名字逐一
//!    对上——这就是 UI 三个表单(创建/优化/临时任务)背后的同一条命令链路。
//! 2. `ui::vm::workflow_detail(「原型」标准工作流)` 的 `phase_skills` 必须
//!    含 evidence-first——SkillHub「被这些工作流使用」反查修复的数据面证明。
//! 3. `ui::vm::skill_card` 的规范机检口径:bw-standard 行 0 findings(徽记
//!    不出声);官方外库行只有 Advisory(`spec_must_fix == 0`,黄灯不亮)。
//!
//!   cp <真实库> /tmp/e2e.db
//!   cargo run -p bw-app --example verify_skill_workflow_link -- /tmp/e2e.db
//!   sqlite3 /tmp/e2e.db "SELECT name, skills_json FROM workflow_spec WHERE name LIKE 'spec-e2e%'"

use bw_app::{App, Command};
use bw_core::model::{HubSource, LoopConfig, Maturity, SkillRef};
use bw_core::WorkflowId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let db_path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: verify_skill_workflow_link <db-path>(真实库的一次性副本)");
        std::process::exit(2);
    });
    println!("================ plan/16 §5 · 技能关联 E2E ================");
    println!("db: {db_path}");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    // ── 1 · 从真实 Hub 目录选技能建工作流(resolve_refs 同款投影) ──
    let skills = store.list_skills().await.unwrap();
    let picked: Vec<SkillRef> = ["evidence-first", "keyword-focus-scoring"]
        .iter()
        .filter_map(|want| skills.iter().find(|s| s.name == *want))
        .map(|s| SkillRef {
            name: s.name.clone(),
            def: s.desc.clone(),
            from: "SkillHub".into(),
        })
        .collect();
    assert_eq!(
        picked.len(),
        2,
        "前提不成立:两个目标技能须真实在库(先跑 audit_skills --fix)"
    );

    let wf_id = WorkflowId::new();
    let wf_name = "spec-e2e-技能关联验证".to_string();
    // 幂等:重跑前先清同名旧行,避免在副本上越积越多。
    for old in store
        .list_workflow_specs()
        .await
        .unwrap()
        .iter()
        .filter(|w| w.name == wf_name)
    {
        store.delete_workflow_spec(old.id).await.unwrap();
    }
    app.dispatch(Command::CreateWorkflowSpec {
        id: wf_id,
        name: wf_name.clone(),
        prompt: "E2E:验证 SkillHub 选取的技能真实落进 skills_json".into(),
        goal: "skills_json 非空且逐名对上".into(),
        stage_ref: None,
        phases: vec!["验证".into()],
        phase_prompts: vec![],
        agents: vec![],
        skills: picked.clone(),
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 1,
        },
        maturity: Maturity::Polishing,
        scope: "E2E".into(),
        source: HubSource::SelfBuilt,
        trigger: None,
    })
    .await
    .unwrap();

    let created = store
        .list_workflow_specs()
        .await
        .unwrap()
        .into_iter()
        .find(|w| w.id == wf_id)
        .expect("刚建的工作流必须能读回");
    let got: Vec<&str> = created.skills.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(got, vec!["evidence-first", "keyword-focus-scoring"]);
    assert!(created.skills.iter().all(|s| s.from == "SkillHub"));
    println!(
        "1) CreateWorkflowSpec 读回 OK: skills={got:?} (from=SkillHub) — sqlite 复核: \
         SELECT skills_json FROM workflow_spec WHERE id='{}'",
        wf_id.uuid()
    );

    // ── 2 · 反查数据面:标准工作流的 phase 级技能绑定进 VM ──
    let prototype_wf = store
        .list_workflow_specs()
        .await
        .unwrap()
        .into_iter()
        .find(|w| w.name.contains("「原型」标准工作流"))
        .expect("五条标准工作流须在库");
    let detail = ui::vm::workflow_detail(&prototype_wf).expect("标准工作流是 Static,必有 detail");
    assert!(
        detail.phase_skills.iter().any(|n| n == "evidence-first"),
        "phase_skills 必须含 evidence-first,实际: {:?}",
        detail.phase_skills
    );
    println!(
        "2) workflow_detail(「原型」).phase_skills={:?} — 反查不再漏 phase 绑定",
        detail.phase_skills
    );

    // ── 3 · 徽记口径:bw-standard 全合规;官方外库只提示不点黄灯 ──
    let ef = skills.iter().find(|s| s.name == "evidence-first").unwrap();
    let ef_vm = ui::vm::skill_card(ef, vec![]);
    assert!(
        ef_vm.spec_findings.is_empty() && ef_vm.spec_must_fix == 0,
        "evidence-first 应 0 findings,实际 {:?}",
        ef_vm.spec_findings
    );
    let official_advisories = skills
        .iter()
        .filter(|s| matches!(&s.source, HubSource::Official { official_library } if official_library != "bw-standard"))
        .map(|s| ui::vm::skill_card(s, vec![]))
        .filter(|vm| !vm.spec_findings.is_empty())
        .collect::<Vec<_>>();
    assert!(
        official_advisories.iter().all(|vm| vm.spec_must_fix == 0),
        "官方外库导入不得出现 MustFix(分域规则)"
    );
    println!(
        "3) 徽记口径 OK: evidence-first 0 findings;官方外库 {} 条仅提示、0 黄灯",
        official_advisories.len()
    );

    println!("ALL OK — 三段验证全部真实读回通过");
}
