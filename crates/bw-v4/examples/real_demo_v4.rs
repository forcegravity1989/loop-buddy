//! `real_demo_v4` —— V4 主环的 headless 指挥器。
//!
//! 不开界面,直接驱动内核走完一圈:接入项目 → 规范铺底 → 开始本周 → 代人确认
//! 建活 → 一张活开工 → 代人推评审中 → 代人点完成 → 发版本 → 导出证据。
//!
//! ```bash
//! cargo run -p bw-v4 --example real_demo_v4 -- <db-path> <workspaces-root> [--project <slug>]
//! ```
//!
//! 四条取舍,如实写在这里:
//!
//! 1. **工作区是 buddy 自己这个仓的本地浅拷贝**(`git clone --local`),不是空
//!    仓——健康判据里「本周有没有真实提交」「上周合入了几次」要读真实 git 历史
//!    才有意义,数字能对着 `git log` 复算。
//! 2. **▶跑走的是自我标注的替身执行器**,产出带【mock】字样。不碰真 `claude`、
//!    不碰网关。
//! 3. **不真开 MR**。没挂远端,所以「开 MR 才能进评审中」那条路退化成既有的
//!    「没有 MR → 人点确认」;推评审中与点完成都是**脚本代人**,证据 JSON 里
//!    逐条写明。
//! 4. **重复跑不产生重复数据**:项目按 slug、活按标题、周计划按文件在不在、
//!    发版按版本号判断跳不跳过。

use bw_v4::app::App;
use bw_v4::command::{Command, Event, ProjectIntent, RemoteRef};
use bw_v4::model::{IssueKind, IssueOrigin, IssueStatus, ProjectId, StageKind};
use bw_v4::{isoweek, V4Store};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 起任何线程之前先把本机时区定住 —— 周是按本机时区算的。
    bw_v4::isoweek::init_local_offset();
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "用法:cargo run -p bw-v4 --example real_demo_v4 -- <db-path> <workspaces-root> [--project <slug>]"
        );
        std::process::exit(2);
    }
    let db_path = args[1].clone();
    let ws_root = PathBuf::from(&args[2]);
    let slug = args
        .iter()
        .position(|a| a == "--project")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "buddy-v4-demo".to_string());

    std::fs::create_dir_all(&ws_root)?;
    let store = V4Store::open(&db_path).await?;
    let mut app = App::new(
        store.clone(),
        &ws_root,
        Arc::new(bw_engine::MockInteractiveExecutor::new()),
    );

    let week = isoweek::current_week();
    let mut log = Vec::new();
    say(&mut log, &format!("本周 = {week}"));

    // ── 步骤 1:接入项目 ──────────────────────────────────
    let workspace = clone_buddy_repo(&ws_root, &slug)?;
    say(
        &mut log,
        &format!("步骤 1 · 接入项目:工作区 = {}", workspace.display()),
    );
    let events = app
        .dispatch(Command::CreateProject {
            slug: slug.clone(),
            intent: ProjectIntent {
                name: "buddy V4 演示项目".into(),
                brief: "验证 V4 主环:接入 → 铺底 → 开始本周 → 干一张活 → 发版本".into(),
                benchmark: "Linear".into(),
                north_star: "每周被真实合入的活数".into(),
            },
            remote: RemoteRef::default(),
            workspace_path: workspace.display().to_string(),
        })
        .await?;
    let pid = events
        .iter()
        .find_map(|e| match e {
            Event::ProjectCreated { id, .. } => Some(*id),
            _ => None,
        })
        .expect("项目没建出来");

    // ── 步骤 2:规范铺底第 1 步 ────────────────────────────
    let events = app
        .dispatch(Command::RunStandardBootstrap { project_id: pid })
        .await?;
    let (bootstrap_files, committed) = match events.first() {
        Some(Event::StandardBootstrapped {
            files, committed, ..
        }) => (files.clone(), *committed),
        _ => (Vec::new(), false),
    };
    say(
        &mut log,
        &format!(
            "步骤 2 · 规范铺底:落盘 {} 个文件,{}",
            bootstrap_files.len(),
            if committed {
                "已提交"
            } else {
                "无改动可提交"
            }
        ),
    );

    // ── 步骤 3-4:开始本周 + 脚本代人确认建活 ──────────────
    let events = app
        .dispatch(Command::StartWeekPlanning {
            project_id: pid,
            week: week.clone(),
        })
        .await?;
    let drafts = match events.first() {
        Some(Event::WeekPlanStarted { draft_titles, .. }) => {
            say(&mut log, "步骤 3 · 开始本周:周计划文件已写出(mock 草稿)");
            draft_titles.clone()
        }
        Some(Event::WeekPlanAlreadyExists { .. }) => {
            say(&mut log, "步骤 3 · 本周文件已存在,跳过(重跑不产生重复数据)");
            Vec::new()
        }
        _ => Vec::new(),
    };
    if !drafts.is_empty() {
        app.dispatch(Command::ConfirmWeekDraft {
            project_id: pid,
            week: week.clone(),
            titles: drafts.clone(),
        })
        .await?;
        say(
            &mut log,
            &format!("步骤 4 · 脚本代人确认,建了 {} 张业务活", drafts.len()),
        );
    }

    // 另建一张挂了类别的业务活 —— 用来验证「类别→工具→workflow」映射真的生效。
    let events = app
        .dispatch(Command::CreateIssue {
            project_id: pid,
            title: "把 V4 主环跑通一次".into(),
            body: "指挥器建的业务活,用来验证类别映射与干活闭环。".into(),
            category: Some(StageKind::Build),
            kind: IssueKind::Business,
            origin: IssueOrigin::Human,
            week_of: week.clone(),
        })
        .await?;
    let main_issue = events
        .iter()
        .find_map(|e| match e {
            Event::IssueCreated { id, .. } => Some(*id),
            _ => None,
        })
        .expect("主线活没建出来");

    // ── 步骤 5-6:▶跑 → 代人推评审中 → 代人点完成 ─────────
    // 重跑判据是这张活现在什么状态 —— 已经完成的不再开工一次(既是幂等,也是
    // 「干完的活不会被自动重开」这条本来就该成立的行为)。
    let status_now = store.issue(main_issue).await?.map(|i| i.status);
    if status_now == Some(IssueStatus::Done) {
        say(
            &mut log,
            "步骤 5-6 · 这张活上次已经走完并结清,跳过(重跑不重复记账)",
        );
    } else {
        let events = app.dispatch(Command::RunIssue { id: main_issue }).await?;
        if let Some(Event::IssueRan { ok, summary, .. }) = events.first() {
            say(
                &mut log,
                &format!(
                    "步骤 5 · ▶跑:{}(执行器原话:{summary})",
                    if *ok { "成功" } else { "未完成" }
                ),
            );
        }
        if store.issue(main_issue).await?.map(|i| i.status) != Some(IssueStatus::InReview) {
            app.dispatch(Command::TransitionIssue {
                id: main_issue,
                to: IssueStatus::InReview,
            })
            .await?;
            say(
                &mut log,
                "步骤 6a · 脚本代人推「评审中」(没挂远端,没有 MR 可合)",
            );
        }
        let settled = match app
            .dispatch(Command::TransitionIssue {
                id: main_issue,
                to: IssueStatus::Done,
            })
            .await?
            .first()
        {
            Some(Event::IssueTransitioned { settled, .. }) => *settled,
            _ => false,
        };
        say(
            &mut log,
            &format!(
                "步骤 6b · 脚本代人点「完成」:{}",
                if settled {
                    "这次真结清了"
                } else {
                    "之前已经结过,没有第二次记账"
                }
            ),
        );
    }

    // ── 步骤 7:发版本 ───────────────────────────────────
    let events = app
        .dispatch(Command::CutRelease {
            project_id: pid,
            version: "v0.1".into(),
            note: "V4 主环首次跑通(指挥器)".into(),
            included: vec![main_issue],
        })
        .await?;
    if let Some(Event::ReleaseCut { rows_written, .. }) = events.first() {
        say(
            &mut log,
            &format!(
                "步骤 7 · 发版本 v0.1:{}",
                if *rows_written {
                    "发版记录新增一行"
                } else {
                    "这个版本号已经在记录里,不写第二行"
                }
            ),
        );
    }

    // 收尾提交:周计划与发版记录是铺底之后才写的,这里一并进仓,免得下一次
    // 跑的时候被上一轮的残留改动混进提交里。
    let tail = bw_v4::git::commit_all(&workspace, "docs(bw): 本周计划与发版记录(指挥器)")
        .await
        .unwrap_or(false);
    say(
        &mut log,
        &format!(
            "收尾提交:{}",
            if tail {
                "周计划与发版记录已进仓"
            } else {
                "没有待提交的改动"
            }
        ),
    );

    // 健康现算一次(算完顺手写回项目墙的显示缓存)。
    let health = app.recompute_health(pid).await?;
    say(&mut log, &format!("健康现算:{:?}", health.signal()));

    // ── 步骤 10:证据 JSON,数字全部真实读回 ──────────────
    let evidence = build_evidence(&store, pid, &slug, &workspace, &week, &health, &log).await?;
    let out = format!("evidence-v4-{slug}.json");
    std::fs::write(&out, &evidence)?;
    println!("{evidence}");
    eprintln!("[REAL_DEMO_V4] 证据已写入 {out}");
    Ok(())
}

fn say(log: &mut Vec<String>, line: &str) {
    eprintln!("[REAL_DEMO_V4] {line}");
    log.push(line.to_string());
}

/// 把 buddy 自己这个仓浅拷贝一份当工作区。已经拷过就原样用(幂等)。
fn clone_buddy_repo(root: &Path, slug: &str) -> std::io::Result<PathBuf> {
    let target = root.join(slug);
    if target.join(".git").is_dir() {
        return Ok(target);
    }
    let here = std::env::current_dir()?;
    let status = std::process::Command::new("git")
        .args(["clone", "--local", "--no-hardlinks", "--quiet"])
        .arg(&here)
        .arg(&target)
        .status()?;
    if !status.success() {
        // 拷不动就退回一个空目录,并如实说明——不假装有 git 历史。
        eprintln!("[REAL_DEMO_V4] git clone --local 失败,退回空目录(历史相关判据会是「没数据」)");
        std::fs::create_dir_all(&target)?;
    }
    Ok(target)
}

async fn build_evidence(
    store: &V4Store,
    pid: ProjectId,
    slug: &str,
    workspace: &Path,
    week: &str,
    health: &bw_v4::derive::DerivedHealth,
    log: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    let project = store.project(pid).await?.expect("项目行");
    let issues = store.issues(pid).await?;
    let tables = store.table_names().await?;
    let usage = store.workflow_usage(pid).await?;

    let issue_json: Vec<serde_json::Value> = issues
        .iter()
        .map(|i| {
            serde_json::json!({
                "number": i.number,
                "title": i.title,
                "status": format!("{:?}", i.status),
                "kind": format!("{:?}", i.kind),
                "origin": format!("{:?}", i.origin),
                "week_of": i.week_of,
                "tool": i.tool,
                "workflow": i.workflow,
                "version": i.version,
                "category": i.category.map(|c| c.label()),
                "settled": i.settled_at.is_some(),
            })
        })
        .collect();

    let plan_path = workspace.join(format!("docs/plan/{week}.md"));
    let releases_path = workspace.join("docs/releases.md");

    let v = serde_json::json!({
        "说明": "每个数字都是从真实库与真实仓文件读回来的,没有一个是硬编码。\
                 ▶跑走的是自我标注的替身执行器(产出带【mock】字样);推评审中与\
                 点完成是脚本代人(没挂远端,没有 MR 可合)。",
        "slug": slug,
        "week": week,
        "workspace": workspace.display().to_string(),
        "库里的表": tables,
        "项目": {
            "name": project.name,
            "灯": format!("{:?}", health.signal()),
            "灯的理由": health.reasons().iter()
                .map(|r| format!("{} {}", if r.ok { "✓" } else { "✗" }, r.text))
                .collect::<Vec<_>>(),
        },
        "活": issue_json,
        "workflow 用过几次(现算,没有战绩表)": usage,
        "仓文件": {
            "周计划": plan_path.is_file(),
            "发版记录": releases_path.is_file(),
            "规范元信息": workspace.join(".bw/standard.toml").is_file(),
            "指纹清单": workspace.join(".bw/managed.toml").is_file(),
            "活的约定": workspace.join(".bw/issue-policy.toml").is_file(),
            "预置技能包数": count_skill_packages(workspace),
        },
        "步骤日志": log,
    });
    Ok(serde_json::to_string_pretty(&v)?)
}

fn count_skill_packages(workspace: &Path) -> usize {
    std::fs::read_dir(workspace.join(".claude/skills"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().join("SKILL.md").is_file())
                .count()
        })
        .unwrap_or(0)
}
