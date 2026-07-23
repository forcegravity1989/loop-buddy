//! T11 (plan/12 §7) headless E2E: "编辑即脱离源头" — `Official` content is
//! editable, and a *substantive* edit (Skill: content/desc/category; Agent:
//! instructions/role/model) flips `source` to `SelfBuilt`, decoupled from
//! its curated library; a non-substantive edit (stage_ref reclassification,
//! or resubmitting byte-identical content) never flips; and re-running the
//! same library import afterward neither overwrites the flipped row's real
//! edit nor mints a same-name duplicate.
//!
//! Drives the real `App`/`Command` layer (`UpdateSkill`/`UpdateAgent`/
//! `ImportSkillLibrary`/`ImportAgentDefinition`), reads results back from the
//! real store — no mocked assertions. Every number this prints should also
//! be independently readable via `sqlite3` (this repo's own core discipline,
//! 报告不代答,读回为证) — see the sqlite3 commands the report alongside this
//! example quotes.
//!
//! Run: `cargo run -p bw-app --example edit_detach -- <output-db-path>`

use bw_app::{App, Command};
use bw_core::model::{HubSource, StageKind};
use bw_core::ProjectId;
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

/// Deep-link target for this example's own render proof (`BW_OPEN=T11-验证
/// BW_PANEL=issues`) — a real project so `[BW_OPEN]` logs a match instead of
/// "NOT FOUND"; the Skill/Agent Hub VM this ticket touches (`skill_card`/
/// `agent_card`) is built unconditionally on every boot regardless of which
/// project is open, so this project's only job is giving the deep-link
/// something real to find.
const DEEP_LINK_PROJECT: &str = "T11-验证";

const MATTPOCOCK_ROOT: &str =
    "/Users/gravity/.claude/plugins/cache/mattpocock/mattpocock-skills/1.2.0/skills";
const ECC_VENDOR_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bw-store/vendor/ecc-agents");

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let db_path = args.next().unwrap_or_else(|| {
        std::env::temp_dir()
            .join("bw_edit_detach.db")
            .to_string_lossy()
            .into_owned()
    });
    let _ = std::fs::remove_file(&db_path);

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.unwrap();

    println!("================ T11 编辑脱离源头 E2E ================");
    println!("db: {db_path}");

    // A minimal real project — purely so this DB has something for
    // `BW_OPEN={DEEP_LINK_PROJECT}` to find afterward (deep-link render
    // proof, e) acceptance criterion); no workspace bound (`None`, and this
    // example configures no `workspaces_root`), so it degrades gracefully to
    // Mock-executor mode, same as every other headless example's projects.
    app.dispatch(Command::CreateProject {
        id: ProjectId::new(),
        name: DEEP_LINK_PROJECT.to_string(),
        kind: "内部验证".to_string(),
        desc: "T11 深链渲染证明用的占位项目".to_string(),
        workspace: None,
    })
    .await
    .expect("CreateProject(deep-link placeholder) should succeed");

    let mut all_ok = true;

    // ============================== Skill 侧 ==============================
    app.dispatch(Command::ImportSkillLibrary {
        root_path: MATTPOCOCK_ROOT.to_string(),
        official_library: "mattpocock-skills".to_string(),
        project_id: None,
    })
    .await
    .expect("ImportSkillLibrary(mattpocock-skills) pass 1 should succeed");

    let skills_pass1 = store.list_skills().await.unwrap();
    let mp_rows_pass1: Vec<_> = skills_pass1
        .iter()
        .filter(|s| {
            matches!(&s.source, HubSource::Official { official_library } if official_library == "mattpocock-skills")
        })
        .collect();
    println!("----------------------------------------------------------");
    println!(
        "[skill] mattpocock-skills rows after pass 1: {}",
        mp_rows_pass1.len()
    );

    // a) 编辑 official skill 的 content(实质字段)→ 应翻转 SelfBuilt。
    let tdd = skills_pass1
        .iter()
        .find(|s| s.name == "tdd")
        .expect("skill named \"tdd\" must exist after import")
        .clone();
    assert!(
        matches!(&tdd.source, HubSource::Official { official_library } if official_library == "mattpocock-skills")
    );
    let edited_content = format!(
        "{}\n\n<!-- T11 E2E: 本地实质编辑,应脱离源头 -->",
        tdd.content
    );
    app.dispatch(Command::UpdateSkill {
        id: tdd.id,
        name: tdd.name.clone(),
        desc: tdd.desc.clone(),
        category: tdd.category.clone(),
        content: edited_content.clone(),
    })
    .await
    .expect("UpdateSkill(tdd, edited content) should succeed");

    let tdd_after_edit = store.get_skill(tdd.id).await.unwrap().unwrap();
    let a_flip_ok = matches!(tdd_after_edit.source, HubSource::SelfBuilt)
        && tdd_after_edit.content == edited_content
        && tdd_after_edit.adapted_from.as_deref() == Some("mattpocock-skills");
    println!(
        "[skill a] tdd 编辑 content 后:source={:?} adapted_from={:?} content_match={}  -> {a_flip_ok}",
        tdd_after_edit.source,
        tdd_after_edit.adapted_from,
        tdd_after_edit.content == edited_content
    );
    all_ok &= a_flip_ok;

    // b1) 仅提交与现值完全相同的内容(no-op 编辑)→ 不应翻转。候选池必须限定
    // 在这次真正导入的 mattpocock-skills 行内——`skills_pass1` 是全库列表,
    // 混着 Boot 时播种的 5 个内置阶段技能(`SelfBuilt`,created_at 更早,
    // 排序上排在前面),从那份全量表里随手挑第二条会挑到内置行而非导入行。
    let tracer = mp_rows_pass1
        .iter()
        .find(|s| s.name == "tracer-bullets")
        .copied()
        .or_else(|| mp_rows_pass1.iter().find(|s| s.id != tdd.id).copied())
        .expect("a second official mattpocock-skills row must exist to test the no-op path")
        .clone();
    assert!(matches!(&tracer.source, HubSource::Official { .. }));
    app.dispatch(Command::UpdateSkill {
        id: tracer.id,
        name: tracer.name.clone(),
        desc: tracer.desc.clone(),
        category: tracer.category.clone(),
        content: tracer.content.clone(),
    })
    .await
    .expect("UpdateSkill(no-op resubmit) should succeed");
    let tracer_after_noop = store.get_skill(tracer.id).await.unwrap().unwrap();
    let b1_ok = matches!(tracer_after_noop.source, HubSource::Official { .. });
    println!(
        "[skill b1] {} no-op 重复提交后:source={:?}  -> {b1_ok}",
        tracer.name, tracer_after_noop.source
    );
    all_ok &= b1_ok;

    // b2) 仅改 stage_ref 归类(非实质字段;无 UpdateSkill 命令面覆盖它,直接
    // 走 store 的 set_skill_stage_ref 窄口——它的 SQL 从不碰 source 列,结构上
    // 就不可能触发翻转)→ 不应翻转。
    let third = mp_rows_pass1
        .iter()
        .find(|s| s.id != tdd.id && s.id != tracer.id)
        .copied()
        .expect("a third official mattpocock-skills row must exist to test the stage_ref-only path")
        .clone();
    assert!(matches!(&third.source, HubSource::Official { .. }));
    store
        .set_skill_stage_ref(third.id, Some(StageKind::Build))
        .await
        .expect("set_skill_stage_ref should succeed");
    let third_after_stage = store.get_skill(third.id).await.unwrap().unwrap();
    let b2_ok = matches!(third_after_stage.source, HubSource::Official { .. })
        && third_after_stage.stage_ref == Some(StageKind::Build);
    println!(
        "[skill b2] {} 仅改 stage_ref 后:source={:?} stage_ref={:?}  -> {b2_ok}",
        third.name, third_after_stage.source, third_after_stage.stage_ref
    );
    all_ok &= b2_ok;

    // c) 重新 ImportSkillLibrary 同库 → 已脱离(tdd)不覆盖、不重复;其余
    // 未编辑的行照旧 skip;总行数不变。
    app.dispatch(Command::ImportSkillLibrary {
        root_path: MATTPOCOCK_ROOT.to_string(),
        official_library: "mattpocock-skills".to_string(),
        project_id: None,
    })
    .await
    .expect("ImportSkillLibrary(mattpocock-skills) pass 2 should succeed");

    let skills_pass2 = store.list_skills().await.unwrap();
    let tdd_rows: Vec<_> = skills_pass2.iter().filter(|s| s.name == "tdd").collect();
    let tdd_after_reimport = store.get_skill(tdd.id).await.unwrap().unwrap();
    let c_no_dup = tdd_rows.len() == 1;
    let c_not_overwritten = tdd_after_reimport.content == edited_content
        && tdd_after_reimport.source == HubSource::SelfBuilt;
    let c_total_stable = skills_pass2.len() == skills_pass1.len();
    let c_ok = c_no_dup && c_not_overwritten && c_total_stable;
    println!(
        "[skill c] 重导后:tdd 同名行数={} 内容未被覆盖={} 总行数 {}->{} (稳定={})  -> {c_ok}",
        tdd_rows.len(),
        c_not_overwritten,
        skills_pass1.len(),
        skills_pass2.len(),
        c_total_stable
    );
    all_ok &= c_ok;

    // ============================== Agent 侧 ==============================
    let mut ecc_files: Vec<std::path::PathBuf> = std::fs::read_dir(ECC_VENDOR_DIR)
        .unwrap_or_else(|e| panic!("cannot read ECC_VENDOR_DIR {ECC_VENDOR_DIR}: {e}"))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    ecc_files.sort();
    let ecc_total = ecc_files.len();

    for f in &ecc_files {
        app.dispatch(Command::ImportAgentDefinition {
            source_path: f.to_string_lossy().into_owned(),
            official_library: Some("ecc".to_string()),
        })
        .await
        .unwrap_or_else(|e| panic!("ImportAgentDefinition({f:?}) pass 1 should succeed: {e}"));
    }

    let agents_pass1 = store.list_agents().await.unwrap();
    let ecc_rows_pass1: Vec<_> = agents_pass1
        .iter()
        .filter(|a| matches!(&a.source, HubSource::Official { official_library } if official_library == "ecc"))
        .collect();
    println!("----------------------------------------------------------");
    println!(
        "[agent] ECC files on disk={ecc_total} rows after pass 1={} total agents={}",
        ecc_rows_pass1.len(),
        agents_pass1.len()
    );

    // d-a) 编辑一个 ECC agent 的 instructions(实质字段)→ 应翻转 SelfBuilt。
    let architect = agents_pass1
        .iter()
        .find(|a| a.name == "architect")
        .expect("agent named \"architect\" must exist after ECC import")
        .clone();
    assert!(
        matches!(&architect.source, HubSource::Official { official_library } if official_library == "ecc")
    );
    let edited_instructions = format!(
        "{}\n\n<!-- T11 E2E: 本地实质编辑,应脱离源头 -->",
        architect.instructions
    );
    app.dispatch(Command::UpdateAgent {
        id: architect.id,
        name: architect.name.clone(),
        role: architect.role.clone(),
        skills: architect.skills.iter().map(|t| t.name.clone()).collect(),
        model: architect.model.clone(),
        instructions: edited_instructions.clone(),
    })
    .await
    .expect("UpdateAgent(architect, edited instructions) should succeed");
    let architect_after_edit = store.get_agent(architect.id).await.unwrap().unwrap();
    let d_flip_ok = matches!(architect_after_edit.source, HubSource::SelfBuilt)
        && architect_after_edit.instructions == edited_instructions
        && architect_after_edit.adapted_from.as_deref() == Some("ecc");
    println!(
        "[agent d-a] architect 编辑 instructions 后:source={:?} adapted_from={:?} instructions_match={}  -> {d_flip_ok}",
        architect_after_edit.source,
        architect_after_edit.adapted_from,
        architect_after_edit.instructions == edited_instructions
    );
    all_ok &= d_flip_ok;

    // d-b1) no-op 重复提交(与现值完全相同)→ 不应翻转。同 Skill b1 的候选池
    // 陷阱——必须从 `ecc_rows_pass1`(已过滤为 Official/ecc)里挑,不能从
    // `agents_pass1` 全量表里挑(混着 5 个内置阶段 agent)。
    let untouched_agent = ecc_rows_pass1
        .iter()
        .find(|a| a.id != architect.id)
        .copied()
        .expect("a second ECC agent must exist to test the no-op path")
        .clone();
    assert!(matches!(
        &untouched_agent.source,
        HubSource::Official { .. }
    ));
    app.dispatch(Command::UpdateAgent {
        id: untouched_agent.id,
        name: untouched_agent.name.clone(),
        role: untouched_agent.role.clone(),
        skills: untouched_agent
            .skills
            .iter()
            .map(|t| t.name.clone())
            .collect(),
        model: untouched_agent.model.clone(),
        instructions: untouched_agent.instructions.clone(),
    })
    .await
    .expect("UpdateAgent(no-op resubmit) should succeed");
    let untouched_after_noop = store.get_agent(untouched_agent.id).await.unwrap().unwrap();
    let d_b1_ok = matches!(untouched_after_noop.source, HubSource::Official { .. });
    println!(
        "[agent d-b1] {} no-op 重复提交后:source={:?}  -> {d_b1_ok}",
        untouched_agent.name, untouched_after_noop.source
    );
    all_ok &= d_b1_ok;

    // d-b2) 仅改 stage_ref(非实质;走 store 的 set_agent_stage_ref 窄口,同
    // Skill b2 的结构性理由)→ 不应翻转。
    let third_agent = ecc_rows_pass1
        .iter()
        .find(|a| a.id != architect.id && a.id != untouched_agent.id)
        .copied()
        .expect("a third ECC agent must exist to test the stage_ref-only path")
        .clone();
    assert!(matches!(&third_agent.source, HubSource::Official { .. }));
    store
        .set_agent_stage_ref(third_agent.id, Some(StageKind::Optimize))
        .await
        .expect("set_agent_stage_ref should succeed");
    let third_agent_after_stage = store.get_agent(third_agent.id).await.unwrap().unwrap();
    let d_b2_ok = matches!(third_agent_after_stage.source, HubSource::Official { .. })
        && third_agent_after_stage.stage_ref == Some(StageKind::Optimize);
    println!(
        "[agent d-b2] {} 仅改 stage_ref 后:source={:?} stage_ref={:?}  -> {d_b2_ok}",
        third_agent.name, third_agent_after_stage.source, third_agent_after_stage.stage_ref
    );
    all_ok &= d_b2_ok;

    // d-c) 重跑 67 个 ImportAgentDefinition(同 official_library="ecc")→
    // 已脱离(architect)不覆盖、不重复;其余未编辑的照旧 skip;总数不变。
    for f in &ecc_files {
        app.dispatch(Command::ImportAgentDefinition {
            source_path: f.to_string_lossy().into_owned(),
            official_library: Some("ecc".to_string()),
        })
        .await
        .unwrap_or_else(|e| panic!("ImportAgentDefinition({f:?}) pass 2 should succeed: {e}"));
    }
    let agents_pass2 = store.list_agents().await.unwrap();
    let architect_rows: Vec<_> = agents_pass2
        .iter()
        .filter(|a| a.name == "architect")
        .collect();
    let architect_after_reimport = store.get_agent(architect.id).await.unwrap().unwrap();
    let d_c_no_dup = architect_rows.len() == 1;
    let d_c_not_overwritten = architect_after_reimport.instructions == edited_instructions
        && architect_after_reimport.source == HubSource::SelfBuilt;
    let d_c_total_stable = agents_pass2.len() == agents_pass1.len();
    let d_c_ok = d_c_no_dup && d_c_not_overwritten && d_c_total_stable;
    println!(
        "[agent d-c] 重导后:architect 同名行数={} 内容未被覆盖={} 总行数 {}->{} (稳定={})  -> {d_c_ok}",
        architect_rows.len(),
        d_c_not_overwritten,
        agents_pass1.len(),
        agents_pass2.len(),
        d_c_total_stable
    );
    all_ok &= d_c_ok;

    println!("==========================================================");
    println!("全部断言通过: {all_ok}");
    println!("db 文件: {db_path}(供独立 sqlite3 读回复核)");

    if !all_ok {
        std::process::exit(1);
    }
}
