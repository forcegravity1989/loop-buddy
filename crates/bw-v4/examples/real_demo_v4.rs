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
    // 「开始本周」现在回一串事件:建运作活① → ▶跑 → 周计划已写出。**按类型
    // 找,不按位置取** —— 上一版取 events.first(),运作活①插进来之后这一步
    // 就默默什么都不做了,日志里连行都没有。
    let drafts = if let Some(Event::WeekPlanStarted { draft_titles, .. }) = events
        .iter()
        .find(|e| matches!(e, Event::WeekPlanStarted { .. }))
    {
        say(
            &mut log,
            "步骤 3 · 开始本周:建了运作活①、开了工,周计划文件已写出(mock 草稿)",
        );
        draft_titles.clone()
    } else if events
        .iter()
        .any(|e| matches!(e, Event::WeekPlanAlreadyExists { .. }))
    {
        say(&mut log, "步骤 3 · 本周文件已存在,跳过(重跑不产生重复数据)");
        Vec::new()
    } else {
        Vec::new()
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

    // ── 步骤 8:定时的一跳 ────────────────────────────────
    // 判据是「到点了没有」+「本周有没有那张活」。**不改系统时间、不假装到
    // 点**:周五 20:00 之前跑这个指挥器,这一跳就该什么都不做,如实说没到点。
    // 演示项目的节律先改成「周一 00:00」——**不是伪造时间**,是把这个项目自
    // 己的配置改成一个已经过去的时刻,让这一跳真的到点。正式项目的默认值是
    // 周五 20:00,`.bw/issue-policy.toml` 里写着,人随时能改。
    let mut restore_schedule: Option<String> = None;
    if let Ok(Some(mut policy)) = bw_v4::repo::issue_policy_file::read(&workspace) {
        if let Some(c) = policy.cadence.as_mut() {
            restore_schedule = Some(std::mem::replace(&mut c.ops2_schedule, "mon 00:00".into()));
        }
        let _ = bw_v4::repo::issue_policy_file::write(&workspace, &policy);
        say(
            &mut log,
            "步骤 8 前置 · 把演示项目的 ops2_schedule 改成「mon 00:00」(正式默认是 fri 20:00)",
        );
    }
    let before: usize = store.issues(pid).await?.len();
    app.dispatch(Command::TickScheduler { project_id: pid })
        .await?;
    let after = store.issues(pid).await?;
    let audit = after
        .iter()
        .find(|i| i.workflow == bw_v4::app::OPS2_WORKFLOW && i.week_of == week);
    say(
        &mut log,
        &match audit {
            Some(i) => format!(
                "步骤 8 · 定时:本周的「资产盘点」在了 —— #{} 来源 {} 状态「{}」\
                 (自动建的活绝不被自动推进到完成)",
                i.number,
                i.origin.label(),
                i.status.label()
            ),
            None if after.len() == before => {
                "步骤 8 · 定时:还没到 .bw/issue-policy.toml 里那个时刻,这一跳什么都没做".into()
            }
            None => "步骤 8 · 定时:建出了活但不是资产盘点 —— 判据对不上,如实记下".into(),
        },
    );

    // 这一跳完了就把节律改回去 —— 这是演示为了让「到点」真的发生临时动的,
    // 留在仓里的话人打开演示项目会看见一个不是默认值的时刻,以为产品就长这样。
    if let Some(orig) = restore_schedule {
        if let Ok(Some(mut policy)) = bw_v4::repo::issue_policy_file::read(&workspace) {
            if let Some(c) = policy.cadence.as_mut() {
                c.ops2_schedule = orig.clone();
            }
            let _ = bw_v4::repo::issue_policy_file::write(&workspace, &policy);
            say(
                &mut log,
                &format!("步骤 8 收尾 · ops2_schedule 改回「{orig}」"),
            );
        }
    }

    // ── 步骤 9:老项目历史回填 ────────────────────────────
    // 这个演示项目是 buddy 自己的仓 `git clone --local` 出来的,所以它有真实
    // 历史 —— 回填出来的每一份周文件都能拿 git 复算。
    let events = app
        .dispatch(Command::BackfillHistory { project_id: pid })
        .await?;
    if let Some(Event::HistoryBackfilled {
        weeks,
        releases,
        note,
        ..
    }) = events.first()
    {
        say(&mut log, &format!("步骤 9 · 历史回填:{note}"));
        // 抽一份出来标明怎么复算 —— 报告里给命令,不给结论。
        if let Some(w) = weeks.first() {
            say(
                &mut log,
                &format!(
                    "  复算样例:git -C <ws> log --since=<{w} 周一> --until=<下周一> --pretty=format:%H | wc -l",
                ),
            );
        }
        if let Some(v) = releases.first() {
            say(&mut log, &format!("  回填的第一个版本:{v}(来自 git 标签)"));
        }
    }

    // ── 步骤 10:项目群(mock 提供方)────────────────────
    // 真实提供方(WeLink)还没接,所以这里用自我标注的假群:每发一条往 stderr
    // 打一行 `[BW_CHAT_SENT]`,不落库、不进仓。**同一件事连发两次就打两行** ——
    // 要确认的正是「它确实会重复」,不是反过来验证「不会重复」。
    let chat_before = bw_v4::repo::project_file::read(&workspace).ok().flatten();
    if let Some(mut file) = chat_before.clone() {
        file.chat = Some(bw_v4::repo::project_file::ChatConfig {
            provider: "mock".into(),
            group_id: "demo-group".into(),
            notify: Some(vec!["review".into(), "merged".into(), "release".into()]),
        });
        let _ = bw_v4::repo::project_file::write(&workspace, &file);
    }
    if let Some(one) = store
        .issues(pid)
        .await?
        .into_iter()
        .find(|i| i.kind == bw_v4::IssueKind::Business)
    {
        for _ in 0..2 {
            app.dispatch(Command::SyncNotifyToChat {
                issue_id: one.id,
                event_type: "review".into(),
            })
            .await?;
        }
        say(
            &mut log,
            &format!(
                "步骤 10 · 项目群(mock):对 #{} 连发两次「评审中」—— stderr 上应该有两行 \
                 [BW_CHAT_SENT],这是设计上认了的重复,不是 bug",
                one.number
            ),
        );
    }
    // 演示完把 [chat] 段改回去 —— 假群留在仓里,人打开演示项目会以为真配了群。
    if let Some(file) = chat_before {
        let _ = bw_v4::repo::project_file::write(&workspace, &file);
        say(&mut log, "步骤 10 收尾 · [chat] 段改回演示项目原来的样子");
    }

    // 收尾提交:周计划与发版记录是铺底之后才写的,这里一并进仓,免得下一次
    // 跑的时候被上一轮的残留改动混进提交里。
    let tail = bw_v4::git::commit_paths(
        &workspace,
        &[
            bw_v4::repo::week_plan_file::DIR.to_string(),
            bw_v4::repo::release_file::REL_PATH.to_string(),
            ".bw".to_string(),
        ],
        "docs(bw): 本周计划与发版记录(指挥器)",
    )
    .await
    .unwrap_or_default()
    .committed;
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
    // 证据写在**工作区根旁边**,不写进当前目录 —— 从仓里跑一次指挥器就把仓
    // 弄脏(git status 多一行),那不像话。
    let out = ws_root.join(format!("evidence-v4-{slug}.json"));
    std::fs::write(&out, &evidence)?;
    let out = out.display();
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

    let plan_path = workspace.join(bw_v4::repo::week_plan_file::rel_path(week));
    let releases_path = workspace.join(bw_v4::repo::release_file::REL_PATH);

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
