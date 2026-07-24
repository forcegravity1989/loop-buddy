//! **T10 headless E2E** (plan/12 §5): `CronMode::RunSkill` / `RunPrompt`
//! really execute on `App::tick_scheduler`'s auto-fire — same H18-style
//! verification `verify_goal.rs` already applies to `RunWorkflow`/
//! `CreateIssue`, extended to the two new modes: real store round-trip, real
//! engine execution (MockExecutor, self-labelled 【mock】), real
//! `CronEffectiveness` accounting, and the honest-failure path when a
//! `RunSkill`'s referenced skill disappears mid-flight.
//!
//! This repo has no delete-skill product feature (out of scope, plan/12
//! never asks for one) — H3 below removes the row with a direct SQL DELETE
//! against the store's own db file, purely to simulate "the referenced skill
//! is gone" for this test. That is not a stand-in for a product action; it
//! exists solely to exercise `tick_scheduler`'s honest-failure branch.
//!
//! Run: `cargo run -p bw-app --example cron_modes -- <output-db-path>`

use bw_app::{App, Command};
use bw_core::model::{Cadence, CronMode, CronStatus};
use bw_core::{CronTaskId, ProjectId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

struct Hyp {
    id: &'static str,
    title: &'static str,
    passed: bool,
    evidence: String,
}

/// Run `sqlite3 <path> <sql>` against the store's own db file — direct SQL,
/// bypassing the `Store` trait on purpose (see module doc: simulating "gone"
/// data, not exercising a product command).
fn sqlite3_exec(path: &str, sql: &str) {
    let status = std::process::Command::new("sqlite3")
        .arg(path)
        .arg(sql)
        .status()
        .expect("sqlite3 CLI must be on PATH to run this E2E");
    assert!(status.success(), "sqlite3 exec failed: {sql}");
}

#[tokio::main]
async fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        std::env::temp_dir()
            .join("bw_cron_modes.db")
            .to_string_lossy()
            .into_owned()
    });
    let _ = std::fs::remove_file(&path);

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&path).await.unwrap());
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    let mut h: Vec<Hyp> = Vec::new();

    app.dispatch(Command::Boot).await.unwrap();

    // ── 一个真实项目(走完整创建向导命令序列,不是直接插库) ──
    let pid = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: pid,
        name: "T10 验证项目".into(),
        kind: "内部工具".into(),
        desc: String::new(),
        workspace: None,
        github: None,
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
        run_first: false,
    })
    .await
    .unwrap();

    // 一个真实、带正文的 skill —— Boot 已经 seed 了五阶段方法论 skill(真实
    // content,来自 bw_core::playbook::stage_skills),挑一个真实 id 引用,
    // 绝不按名字匹配。
    let skills = store.list_skills().await.unwrap();
    let real_skill = skills
        .iter()
        .find(|s| !s.content.trim().is_empty())
        .expect("Boot 必须 seed 出至少一个带正文的真实 skill")
        .clone();

    let skill_cron = CronTaskId::new();
    app.dispatch(Command::CreateRunSkillCronTask {
        id: skill_cron,
        name: "T10 验证 · RunSkill".into(),
        schedule: Cadence::Daily,
        project_id: Some(pid),
        skill_id: real_skill.id,
    })
    .await
    .unwrap();

    let prompt_cron = CronTaskId::new();
    let prompt_text =
        "【mock 验证】T10 RunPrompt cron 的裸 prompt —— 不依赖任何 skill/workflow 实体".to_string();
    app.dispatch(Command::CreateRunPromptCronTask {
        id: prompt_cron,
        name: "T10 验证 · RunPrompt".into(),
        schedule: Cadence::Daily,
        project_id: Some(pid),
        prompt: prompt_text.clone(),
    })
    .await
    .unwrap();

    // ── H1: 两个新模式真实存库,mode 往返正确(target 列复用为 payload,零迁移) ──
    let tasks_before = store.list_cron_tasks().await.unwrap();
    let skill_row_before = tasks_before.iter().find(|c| c.id == skill_cron).unwrap();
    let prompt_row_before = tasks_before.iter().find(|c| c.id == prompt_cron).unwrap();
    let skill_mode_ok = matches!(
        &skill_row_before.mode,
        CronMode::RunSkill { skill_id } if *skill_id == real_skill.id
    );
    let prompt_mode_ok = matches!(
        &prompt_row_before.mode,
        CronMode::RunPrompt { prompt } if *prompt == prompt_text
    );
    h.push(Hyp {
        id: "H1",
        title: "CronMode::RunSkill/RunPrompt 真实存库,mode 往返正确(target 列复用为 payload,零 schema 迁移)",
        passed: skill_mode_ok && prompt_mode_ok,
        evidence: format!(
            "RunSkill: mode={:?} · target(raw skill_id)={} | RunPrompt: mode 还原={} · target(raw)前 24 字={}",
            skill_row_before.mode,
            skill_row_before.target,
            prompt_mode_ok,
            prompt_row_before.target.chars().take(24).collect::<String>(),
        ),
    });

    // ── H2: tick 到点 —— 两个任务真实执行(走 run_workflow_inner,MockExecutor
    //    自我标注【mock】),CronStatus 回 Normal,CronEffectiveness 真实入账,
    //    与 RunWorkflow 同等待遇 ──
    let fired = app.tick_scheduler().await.unwrap();

    let tasks_after = store.list_cron_tasks().await.unwrap();
    let skill_row_after = tasks_after
        .iter()
        .find(|c| c.id == skill_cron)
        .unwrap()
        .clone();
    let prompt_row_after = tasks_after
        .iter()
        .find(|c| c.id == prompt_cron)
        .unwrap()
        .clone();
    let skill_eff = store.cron_effectiveness(skill_cron).await.unwrap();
    let prompt_eff = store.cron_effectiveness(prompt_cron).await.unwrap();

    h.push(Hyp {
        id: "H2",
        title: "tick_scheduler 到点真实执行 RunSkill/RunPrompt,CronStatus→Normal,CronEffectiveness 真实入账",
        passed: fired.contains(&skill_cron)
            && fired.contains(&prompt_cron)
            && skill_row_after.status == CronStatus::Normal
            && prompt_row_after.status == CronStatus::Normal
            && skill_row_after.last_run_at.is_some()
            && prompt_row_after.last_run_at.is_some()
            && skill_eff.fires == 1
            && skill_eff.ok_fires == 1
            && prompt_eff.fires == 1
            && prompt_eff.ok_fires == 1,
        evidence: format!(
            "fired={fired:?} · RunSkill status={:?} last_run_at={:?} fires={}/ok{} · RunPrompt status={:?} last_run_at={:?} fires={}/ok{}",
            skill_row_after.status,
            skill_row_after.last_run_at.is_some(),
            skill_eff.fires,
            skill_eff.ok_fires,
            prompt_row_after.status,
            prompt_row_after.last_run_at.is_some(),
            prompt_eff.fires,
            prompt_eff.ok_fires,
        ),
    });

    // ── H3: 引用的 skill 被删后再 tick —— 诚实失败记账,不崩、不假装成功 ──
    // repo 无 delete-skill 产品功能;直接对 store 的 db 文件做一次 SQL DELETE,
    // 模拟"存量技能没了",专为验证这条容错路径而非产品动作。
    sqlite3_exec(
        &path,
        &format!("DELETE FROM skill WHERE id='{}';", real_skill.id.uuid()),
    );
    let skill_gone = store.get_skill(real_skill.id).await.unwrap().is_none();
    // Daily 调度这条 cron 刚跑过,不会立刻再到期——同样用直接 SQL 把
    // last_run_at 清零(而非等一整天),重现"到期"条件;这是测试手法,不是
    // 产品路径(参考 verify_goal.rs H18:那边靠"从未运行过=立即到期"天然造
    // 到期,这里第二次 tick 需要显式清空)。
    sqlite3_exec(
        &path,
        &format!(
            "UPDATE cron_task SET last_run_at=0, status='normal' WHERE id='{}';",
            skill_cron.uuid()
        ),
    );

    let fired2 = app.tick_scheduler().await.unwrap();
    let skill_row_final = store
        .list_cron_tasks()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.id == skill_cron)
        .unwrap();
    let skill_eff_final = store.cron_effectiveness(skill_cron).await.unwrap();

    h.push(Hyp {
        id: "H3",
        title: "RunSkill 引用的技能被删后再 tick —— 诚实失败记账,不崩、不假装成功",
        passed: skill_gone
            && fired2.contains(&skill_cron)
            && skill_row_final.status == CronStatus::Failed
            && skill_eff_final.fires == 2
            && skill_eff_final.failed_fires == 1,
        evidence: format!(
            "skill_gone(store.get_skill→None)={skill_gone} · fired2={fired2:?} · status={:?} · fires={} failed={}",
            skill_row_final.status, skill_eff_final.fires, skill_eff_final.failed_fires,
        ),
    });

    // ── 汇总 ──
    let total = h.len();
    let passed = h.iter().filter(|x| x.passed).count();
    println!("\n================ T10 CronMode::RunSkill/RunPrompt 验证结果 ================");
    for x in &h {
        println!(
            "[{}] {} — {}\n    {}",
            if x.passed { "PASS" } else { "FAIL" },
            x.id,
            x.title,
            x.evidence
        );
    }
    println!("================ {passed}/{total} 通过 · db={path} ================\n");
    if passed != total {
        std::process::exit(1);
    }
}
