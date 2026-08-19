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
fn foreign_release_table_is_never_written_into() {
    // 老项目仓里已经有一份格式不同的发版记录 —— 列数与列名都不一样。
    let dir = tempdir("foreign-release");
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    let foreign = "# 版本登记\n\n| 版本号 | 出包日 | 阶段 | 这一版是什么 | 修了什么 |\n|---|---|---|---|---|\n| 0.3.0 | 2026-08-14 | V3 | 首个安装包 | — |\n";
    std::fs::write(dir.join("docs/releases.md"), foreign).unwrap();

    let row = release_file::ReleaseRow {
        version: "v0.1".into(),
        released_at: "2026-08-20".into(),
        note: "主环跑通".into(),
        included_numbers: vec![4],
        origin: "人发".into(),
    };
    assert!(release_file::append_row(&dir, &row).unwrap());

    let body = std::fs::read_to_string(dir.join("docs/releases.md")).unwrap();
    assert!(
        body.contains("| 0.3.0 | 2026-08-14 | V3 | 首个安装包 | — |"),
        "人家原来那张表一个字都不该动"
    );
    assert!(
        body.contains("## buddy 管理的发版记录"),
        "认不出 buddy 那张表时该另起一段,不是往陌生的表里塞行"
    );
    // 解析只认 buddy 那张表 —— 老项目那行不该被当成 buddy 的发版记录。
    let rows = release_file::read(&dir).unwrap().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].version, "v0.1");
    assert_eq!(rows[0].note, "主环跑通", "列不能错位");
    // 再来一次仍然只有一行。
    assert!(!release_file::append_row(&dir, &row).unwrap());
    assert_eq!(release_file::read(&dir).unwrap().unwrap().len(), 1);
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

/// 拖一张卡会触发一次周计划文件回写。**人手写在这份文件里的东西一个字都不能
/// 少** —— 它是正本,而且回写完还会被顺手 commit。
#[test]
fn replacing_the_activity_table_keeps_everything_a_human_wrote() {
    let raw = "---\nweek: 2026-W34\norigin: human\n---\n\n# 2026-W34 周计划\n\n\
        ## 周目标\n\n第一段目标。\n\n第二段:我还想顺手把门禁修绿。\n\n\
        ## 业务活\n\n这一段说明是我手写的。\n\n\
        | 顺序 | 标题 | 类别 | 工具 | workflow | 预期推动的指标 | 远端 issue |\n\
        |---|---|---|---|---|---|---|\n\
        | 1 | 老的一张活 | 构建 | Claude CLI | — | — | — |\n\n\
        ## 我自己加的一节:风险\n\n- 风险一:网关抖\n- 风险二:时间不够\n\n\
        ## 本周指标读数\n\n| 指标 | 数值 | 来源 | 采集时间 |\n|---|---|---|---|\n\n";

    let rows = vec![week_plan_file::ActivityRow {
        order: 2.0,
        title: "新排进来的活".into(),
        category: None,
        tool: "claude_cli".into(),
        workflow: "mattpocock-skills".into(),
        metric_key: String::new(),
        remote_number: 0,
    }];
    let out =
        week_plan_file::replace_table(raw, "业务活", &week_plan_file::render_activity_table(&rows))
            .expect("这一节找得到");

    assert!(
        out.contains("第二段:我还想顺手把门禁修绿。"),
        "周目标第二段没了"
    );
    assert!(
        out.contains("这一段说明是我手写的。"),
        "业务活上面那段说明没了"
    );
    assert!(out.contains("## 我自己加的一节:风险"), "人自己加的一节没了");
    assert!(out.contains("- 风险一:网关抖"), "风险条目没了");
    assert!(out.contains("## 本周指标读数"), "后面的小节没了");
    assert!(out.contains("新排进来的活"), "新活没写进去");
    assert!(!out.contains("老的一张活"), "旧表没被换掉");
}

/// 标题里带一个竖线,整行不能串列 —— 标题是自由文本,文件是正本。
#[test]
fn a_pipe_in_the_title_does_not_shift_every_column() {
    let rows = vec![week_plan_file::ActivityRow {
        order: 1.0,
        title: "修 a|b 解析".into(),
        category: None,
        tool: "claude_cli".into(),
        workflow: "mattpocock-skills".into(),
        metric_key: "本周合入活数".into(),
        remote_number: 7,
    }];
    let body = format!(
        "---\nweek: 2026-W34\norigin: human\n---\n\n# x\n\n## 业务活\n\n{}\n",
        week_plan_file::render_activity_table(&rows)
    );
    let plan = week_plan_file::parse(&body);
    assert_eq!(plan.activities.len(), 1);
    let a = &plan.activities[0];
    assert_eq!(a.title, "修 a|b 解析");
    assert_eq!(a.tool, "claude_cli");
    assert_eq!(a.workflow, "mattpocock-skills");
    assert_eq!(a.metric_key, "本周合入活数");
    assert_eq!(a.remote_number, 7);
}

/// 接入页的「想做什么 / 北极星」是多行输入框。粘一段带换行、带引号的文字
/// 进去,写出来的 TOML 必须还读得回来 —— 读不回来的表现是「这个项目什么都
/// 没有」,而不是一句报错。
#[test]
fn a_multiline_brief_survives_a_write_read_round_trip() {
    let dir = tempdir("toml-escape");
    let f = project_file::ProjectFile {
        name: "带\"引号\"的项目".into(),
        kind: String::new(),
        brief: "第一行\n第二行\t带制表符\n还有一个反斜杠 \\".into(),
        benchmark: "Linear".into(),
        opportunity: "北极星\n也是多行的".into(),
        standard_version: "4.0".into(),
        current_version: "v0.1".into(),
        chat: None,
    };
    project_file::write(&dir, &f).expect("写得出去");
    let back = project_file::read(&dir).expect("读得回来").expect("文件在");
    assert_eq!(back.name, f.name);
    assert_eq!(back.brief, f.brief);
    assert_eq!(back.opportunity, f.opportunity);
}
