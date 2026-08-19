//! 仓文件解析器的读回。用的样例就是详细设计 02 篇 §2.5 里那几份文件原文
//! ——解析器认不认得设计稿给的格式,是这一层唯一要证明的事。

use bw_v4::isoweek;
use bw_v4::model::StageKind;
use bw_v4::repo::{issue_policy_file, managed_file, project_file, release_file, week_plan_file};

const WEEK_SAMPLE: &str = r#"---
week: 2026-W34
origin: human
---

# 2026-W34 周计划

> 正本文件。

## 周目标

把 V4 详细设计稿写完并过一轮内部评审。

## 业务活

| 顺序 | 标题 | 类别 | 工具 | workflow | 预期推动的指标 | 远端 issue |
|---|---|---|---|---|---|---|
| 1 | V4 高保真可点击原型 | 原型 | Open Design | 原型设计 workflow | — | #104 |
| 2 | 减负线两轮收尾合入 | 构建 | Claude CLI | mattpocock-skills | 本周合入活数 | #102 |

## 本周指标读数

| 指标 | 数值 | 来源 | 采集时间 |
|---|---|---|---|
| 本周合入活数(引领) | 4 | `git log --merges` 现算 | 2026-08-17 09:00 |

## 本周运作

| 活 | 状态 | 说明 |
|---|---|---|
| 运作活①更新指标 + 制定本周计划 | 已完成 08-17 | 复盘上周 |
"#;

#[test]
fn week_plan_sample_parses() {
    let plan = week_plan_file::parse(WEEK_SAMPLE);
    let fm = plan.front_matter.as_ref().unwrap();
    assert_eq!(fm.week, "2026-W34");
    assert!(!fm.is_backfill());
    assert!(plan.has_goal());
    assert_eq!(plan.activities.len(), 2);
    assert_eq!(plan.activities[0].title, "V4 高保真可点击原型");
    assert_eq!(plan.activities[0].category, Some(StageKind::Prototype));
    assert_eq!(plan.activities[0].tool, "open_design");
    assert_eq!(plan.activities[0].metric_key, "", "「—」是留白,不是一个值");
    assert_eq!(plan.activities[0].remote_number, 104);
    assert_eq!(plan.activities[1].category, Some(StageKind::Build));
    assert_eq!(plan.activities[1].tool, "claude_cli");
    assert!(plan.has_reading());
    assert_eq!(plan.ops.len(), 1);
}

#[test]
fn backfilled_week_placeholders_are_not_real_data() {
    let raw = "---\nweek: 2026-W32\norigin: backfill\n---\n\n## 周目标\n\n(未发现——历史周没有周计划记录,不倒推)\n";
    let plan = week_plan_file::parse(raw);
    assert!(plan.front_matter.as_ref().unwrap().is_backfill());
    assert!(
        !plan.has_goal(),
        "括号包起来的「未发现」是留白,不能当成定过周目标"
    );
}

#[test]
fn iso_week_math_round_trips() {
    assert!(isoweek::starts_on_monday("2026-W34"));
    assert_eq!(
        isoweek::iso_week_of(isoweek::week_start("2026-W34").unwrap()),
        "2026-W34"
    );
    assert_eq!(
        isoweek::previous_week("2026-W34").as_deref(),
        Some("2026-W33")
    );
    assert_eq!(
        isoweek::previous_week("2026-W01").as_deref(),
        Some("2025-W52")
    );
    assert!(isoweek::week_start("2026-W99").is_none(), "假周号不该被认");
    assert!(isoweek::week_start("不是周").is_none());
}

#[test]
fn release_rows_parse_and_append_is_idempotent() {
    let dir = tempdir("release");
    release_file::write_default_if_missing(&dir).unwrap();
    let row = release_file::ReleaseRow {
        version: "v0.1".into(),
        released_at: "2026-08-20".into(),
        note: "首个可跑版本".into(),
        included_numbers: vec![1, 2],
        origin: "人发".into(),
    };
    assert!(
        release_file::append_row(&dir, &row).unwrap(),
        "第一次要写进去"
    );
    assert!(
        !release_file::append_row(&dir, &row).unwrap(),
        "同一个版本号绝不写第二行"
    );
    let rows = release_file::read(&dir).unwrap().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].version, "v0.1");
    assert_eq!(rows[0].included_numbers, vec![1, 2]);
    assert_eq!(rows[0].origin, "人发");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn issue_policy_round_trips_and_maps_categories() {
    let raw = r#"
schema_version = 1

[[tool]]
name = "claude_cli"
kind = "terminal"
probe = "path_candidates"
capabilities = ["inject_skills", "resume", "hooks"]

[[mapping]]
category = "build"
tool     = "claude_cli"
workflow = "mattpocock-skills"

[review]
who_can_merge  = "repo_write"
require_pr_for = ["code", "docs"]

[cadence]
ops2_trigger  = "scheduled"
ops2_schedule = "fri 20:00"
"#;
    let f = issue_policy_file::parse(raw).unwrap();
    assert_eq!(f.tools.len(), 1);
    assert_eq!(f.tool("claude_cli").unwrap().kind, "terminal");
    let m = f.mapping_for(StageKind::Build).unwrap();
    assert_eq!(m.tool, "claude_cli");
    assert_eq!(m.workflow, "mattpocock-skills");
    assert!(
        f.mapping_for(StageKind::Growth).is_none(),
        "没配的类别就是没配,不替用户挑一个"
    );
    // 渲染出来的还认得回去。
    let again = issue_policy_file::parse(&issue_policy_file::render(&f)).unwrap();
    assert_eq!(again.mappings, f.mappings);
    assert_eq!(again.tools, f.tools);
}

#[test]
fn unknown_key_fails_loudly() {
    let err = project_file::parse("name = \"x\"\nnot_a_real_key = 1\n");
    assert!(err.is_err(), "键名写错要当场报错,不能静默丢掉");
}

#[test]
fn managed_reconcile_tells_the_four_cases_apart() {
    use managed_file::Reconcile;
    let body = b"hello";
    let entry = managed_file::ManagedEntry {
        path: "AGENTS.md".into(),
        version: "4.0".into(),
        fingerprint: managed_file::fingerprint(body),
    };
    assert_eq!(
        managed_file::reconcile(Some(&entry), Some(body), "4.0"),
        Reconcile::UpToDate
    );
    assert_eq!(
        managed_file::reconcile(Some(&entry), Some(body), "4.1"),
        Reconcile::Stale
    );
    assert_eq!(
        managed_file::reconcile(Some(&entry), Some("人改过了".as_bytes()), "4.0"),
        Reconcile::HumanEdited
    );
    assert_eq!(
        managed_file::reconcile(None, Some(body), "4.0"),
        Reconcile::Missing
    );
    assert_eq!(
        managed_file::reconcile(Some(&entry), None, "4.0"),
        Reconcile::Missing
    );
}

#[test]
fn project_file_chat_section_is_optional() {
    let without = project_file::parse("name = \"WorkflowHub\"\n").unwrap();
    assert!(
        without.chat.is_none(),
        "不配群 = 整段不写,不是写一个空 provider"
    );
    let with = project_file::parse(
        "name = \"WorkflowHub\"\ncurrent_version = \"v0.3\"\n\n[chat]\nprovider = \"welink\"\ngroup_id = \"638201\"\nnotify = [\"review\"]\n",
    )
    .unwrap();
    assert_eq!(with.chat.as_ref().unwrap().group_id, "638201");
    assert_eq!(with.current_version, "v0.3");
    let again = project_file::parse(&project_file::render(&with)).unwrap();
    assert_eq!(again, with, "渲染出来的还认得回去");
}

fn tempdir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bw-v4-repo-test-{tag}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
