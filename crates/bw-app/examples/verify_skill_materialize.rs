//! **verify_skill_materialize — Task 7 headless E2E:目录进 prompt、正文物化
//! 到工作区 `.claude/skills/<name>/`(`prepare_issue_run` 新挂的逻辑)。**
//!
//! `real_demo` 只驱动 `Command::RunStagePlaybook`,从不 `Command::RunIssue`——
//! 测不到 Task 7 真挂的位置(`prepare_issue_run`,`RunIssue` 的起手)。本例沿
//! 最短真实路径走一遍:`CreateProject`(不挂仓)→ `SetWorkspace` 指向一个
//! 普通目录(不需要真 git 仓 —— 没有 `remote_path`,`prepare_issue_run` 里的
//! `pr_eligible` 天然为 false,不走 issue worktree,省掉整套 gh stub)→ 造
//! 一件真实内容的技能并用 `UpdateSkill{stages:Some(..)}` 归类(SR6 落地的同
//! 一条命令)→ `CreateIssue` 落在那个阶段 → `RunIssue` 显式开工。`claude`
//! CLI 由自带【mock】标注的 stub 顶替(同 `verify_c8_standard_trio.rs` 手法,
//! 已验证在这条命令层路径上可用),不依赖网关;证据全部 `sqlite3` / 文件
//! 系统独立读回,不在本例内部断言。
//!
//! 子命令(串起来演 Step 6/7/8 三条验证):
//!   verify_skill_materialize <db> <ws-root> init          — 建项目+技能+issue #1,打印编号
//!   verify_skill_materialize <db> <ws-root> run <编号>     — 对该编号 RunIssue,打印读回
//!   verify_skill_materialize <db> <ws-root> new-issue      — 再建一张同阶段 issue,打印编号
//!
//! Step 7(手写同名目录不被覆盖)要在两次 `run` 之间由外层 shell 手工在工作区
//! 摆放一个没有 `.bw-managed` 标记的同名目录,再对第二张 issue `run` 一遍 ——
//! 那一步不需要改这个例子,直接摸文件系统即可(见 `task-7-report.md`)。

use bw_app::{App, Command};
use bw_core::model::{HubSource, IssuePriority, StageKind};
use bw_core::{IssueId, ProjectId, SessionId, SkillId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{SessionKind, SqliteStore, Store};
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(unix)]
fn make_executable(p: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755));
}
#[cfg(not(unix))]
fn make_executable(_p: &std::path::Path) {}

/// 自带【mock】标注的 stub `claude` —— 不是真实 Anthropic API。手法照抄
/// `verify_c8_standard_trio.rs` 的 `STUB_CLAUDE`(已验证在这条命令层路径上
/// 可用):对每次调用直接返回一个能通过评审门的假产出,让 run 能真实推进到
/// InReview,而不是卡在网关抖动上——本例要验的是物化,不是执行器成败。额外
/// 把收到的 prompt(`-p` 后那个参数)原样追加进 `$STUB_CLAUDE_LOG`(设了才
/// 写)——用来独立核验「目录超限如实写明未列出条数,不静默截断」这条附加
/// 纪律:命令层不暴露 `stage_catalog_block` 的私有返回值,只能从执行器真实
/// 收到的 prompt 里抓文本。
const STUB_CLAUDE: &str = r#"#!/bin/sh
# 【mock】stub claude CLI for Task 7 verify — NOT the real Anthropic API.
if [ -n "$STUB_CLAUDE_LOG" ]; then
  {
    echo "===PHASE $(date +%s%N)==="
    printf '%s\n' "$2"
  } >> "$STUB_CLAUDE_LOG"
fi
printf '{"result":"【mock】stub claude phase output (task7 verify).\\nVERDICT: PASS\\nREASON: 【mock】E2E stub 放行","is_error":false}\n'
exit 0
"#;

const PROJECT_NAME: &str = "task7-verify-project";
const SKILL_NAME: &str = "task7-demo-skill";
const SKILL_CONTENT: &str = "# Task 7 Demo Skill\n\n适用:验证目录进 prompt + 正文物化到工作区。\n\n步骤:\n1. 什么都不用做,这是验证用的假技能正文。\n2. 用来确认 SKILL.md 首行原样是 `#`,没被 demote_headings 动过。\n";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (db, ws_root_s, action) = match (args.first(), args.get(1), args.get(2)) {
        (Some(a), Some(b), Some(c)) => (a.clone(), b.clone(), c.clone()),
        _ => {
            eprintln!("usage: verify_skill_materialize <db> <ws-root> <init|run|new-issue|bulk> …");
            std::process::exit(2);
        }
    };
    let ws_root = PathBuf::from(&ws_root_s);
    std::fs::create_dir_all(&ws_root).expect("create ws root");

    // stub claude 前置进 PATH —— 【mock】,不是真实执行,只为让 run 能推进到
    // InReview,不卡在网关抖动;物化本身发生在执行器被调用之前,跟这个 stub
    // 是否"演得像"无关。
    let stub_bin = ws_root.join(".stub-bin");
    std::fs::create_dir_all(&stub_bin).unwrap();
    let claude = stub_bin.join("claude");
    std::fs::write(&claude, STUB_CLAUDE).unwrap();
    make_executable(&claude);
    let old_path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{}", stub_bin.display(), old_path));
    let claude_log = ws_root.join(".stub-claude.log");
    std::env::set_var("STUB_CLAUDE_LOG", claude_log.display().to_string());

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig {
            binary: None, // 从 PATH 解析 —— 命中上面前置的 stub
            max_budget_usd: 1.0,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    );
    app.dispatch(Command::Boot).await.expect("boot");

    match action.as_str() {
        "init" => {
            let project = match store
                .list_projects()
                .await
                .unwrap()
                .into_iter()
                .find(|p| p.name == PROJECT_NAME)
            {
                Some(p) => {
                    println!("[项目已存在] 续用");
                    p.id
                }
                None => {
                    let id = ProjectId::new();
                    app.dispatch(Command::CreateProject {
                        provider: "github".to_string(),
                        id,
                        name: PROJECT_NAME.into(),
                        kind: "验证 fixture".into(),
                        desc: "Task 7 headless 物化验证,不挂仓".into(),
                        workspace: None,
                        github: None,
                        codehub: None,
                    })
                    .await
                    .expect("create project");
                    id
                }
            };
            app.dispatch(Command::OpenProject(project))
                .await
                .expect("open project");

            let ws_dir = ws_root.join("proj");
            std::fs::create_dir_all(&ws_dir).expect("create project workspace dir");
            app.dispatch(Command::SetWorkspace {
                path: ws_dir.to_string_lossy().into_owned(),
                allow_commands: true,
            })
            .await
            .expect("set workspace");
            println!("[工作区] {}", ws_dir.display());

            // 真实技能 + 真实归类(UpdateSkill 走的是 SR6 落地的同一条命令)。
            let existing_skill = store
                .list_skills()
                .await
                .unwrap()
                .into_iter()
                .find(|s| s.name == SKILL_NAME);
            let skill_id = match existing_skill {
                Some(s) => s.id,
                None => {
                    let id = SkillId::new();
                    app.dispatch(Command::CreateSkill {
                        id,
                        name: SKILL_NAME.into(),
                        desc: "适用:Task 7 物化验证专用假技能。".into(),
                        category: "验证 fixture".into(),
                        source: HubSource::SelfBuilt,
                        content: SKILL_CONTENT.into(),
                    })
                    .await
                    .expect("create skill");
                    id
                }
            };
            app.dispatch(Command::UpdateSkill {
                id: skill_id,
                name: SKILL_NAME.into(),
                desc: "适用:Task 7 物化验证专用假技能。".into(),
                category: "验证 fixture".into(),
                content: SKILL_CONTENT.into(),
                stages: Some(vec![StageKind::Prototype]),
            })
            .await
            .expect("classify skill");
            let after = store
                .list_skills()
                .await
                .unwrap()
                .into_iter()
                .find(|s| s.id == skill_id)
                .unwrap();
            println!(
                "[技能] {} stages={:?} origin={:?} content={}字",
                after.name,
                after.stages,
                after.stage_origin,
                after.content.chars().count()
            );

            let issue_id = IssueId::new();
            app.dispatch(Command::CreateIssue {
                id: issue_id,
                stage: StageKind::Prototype,
                title: "Task 7 物化验证活".into(),
                desc: "只为触发 prepare_issue_run 的目录/物化逻辑,无真实产出诉求。".into(),
                priority: IssuePriority::Medium,
                standard_skill: String::new(),
            })
            .await
            .expect("create issue");
            let issue = store.get_issue(issue_id).await.unwrap().unwrap();
            println!("[建卡] #{} {}", issue.number, issue.title);
        }

        "new-issue" => {
            let project = store
                .list_projects()
                .await
                .unwrap()
                .into_iter()
                .find(|p| p.name == PROJECT_NAME)
                .expect("先跑 init");
            app.dispatch(Command::OpenProject(project.id))
                .await
                .expect("open project");
            let issue_id = IssueId::new();
            app.dispatch(Command::CreateIssue {
                id: issue_id,
                stage: StageKind::Prototype,
                title: "Task 7 物化验证活(第二张,foreign-skip 用)".into(),
                desc: "复跑物化逻辑,验证同名手写目录不被覆盖。".into(),
                priority: IssuePriority::Medium,
                standard_skill: String::new(),
            })
            .await
            .expect("create issue");
            let issue = store.get_issue(issue_id).await.unwrap().unwrap();
            println!("[建卡] #{} {}", issue.number, issue.title);
        }

        // 附加纪律核验专用:批量灌 N 件同样归类到 Prototype 的候选技能,把
        // `stage_catalog_block` 的候选集撑过 `MAX_BLOCK_CHARS`(4000)硬上限,
        // 好独立证明「超限如实写明未列出条数,不静默截断」——真实 68 件库里最
        // 大的原型段只有 30 候选,天然不触顶,这条纪律靠合成数据才验得到。
        "bulk" => {
            let n: usize = args.get(3).expect("bulk <count>").parse().expect("count");
            let project = store
                .list_projects()
                .await
                .unwrap()
                .into_iter()
                .find(|p| p.name == PROJECT_NAME)
                .expect("先跑 init");
            app.dispatch(Command::OpenProject(project.id))
                .await
                .expect("open project");
            for i in 0..n {
                let name = format!("task7-bulk-{i:04}");
                if store
                    .list_skills()
                    .await
                    .unwrap()
                    .iter()
                    .any(|s| s.name == name)
                {
                    continue;
                }
                let id = SkillId::new();
                // desc 刻意写够长(远超 DESC_CAP=80 字符、且不含句号/换行,不会被
                // `first_sentence_capped` 提前截短)——目的是让每条目录行都顶到
                // 单行上限,用最少的候选数就能撑爆 `MAX_BLOCK_CHARS`。
                let desc = format!(
                    "适用于批量合成候选 #{i},专门用来把目录块字符预算撑爆,本身没有任何真实可执行做法,\
                     只是为了独立核验超限时是否如实写明未列出条数、不会静默截断这条附加纪律而存在的假条目占位符文本"
                );
                app.dispatch(Command::CreateSkill {
                    id,
                    name: name.clone(),
                    desc: desc.clone(),
                    category: "验证 fixture".into(),
                    source: HubSource::SelfBuilt,
                    content: format!("# {name}\n\n批量合成候选,内容无意义。\n"),
                })
                .await
                .expect("create bulk skill");
                app.dispatch(Command::UpdateSkill {
                    id,
                    name: name.clone(),
                    desc,
                    category: "验证 fixture".into(),
                    content: format!("# {name}\n\n批量合成候选,内容无意义。\n"),
                    stages: Some(vec![StageKind::Prototype]),
                })
                .await
                .expect("classify bulk skill");
            }
            println!("[批量] 已确保 {n} 件 task7-bulk-* 候选技能挂 Prototype");
        }

        "run" => {
            let n: u32 = args.get(3).expect("run <编号>").parse().expect("编号");
            let project = store
                .list_projects()
                .await
                .unwrap()
                .into_iter()
                .find(|p| p.name == PROJECT_NAME)
                .expect("先跑 init");
            app.dispatch(Command::OpenProject(project.id))
                .await
                .expect("open project");
            let issues = store.list_issues(project.id, None, None).await.unwrap();
            let issue = issues
                .into_iter()
                .find(|i| i.number == n)
                .unwrap_or_else(|| panic!("issue #{n} 不存在,先 init/new-issue"));
            let session = SessionId::new();
            app.dispatch(Command::StartSession {
                id: session,
                stage_kind: Some(issue.stage),
                kind: SessionKind::Create,
                title: format!("verify · #{n} 显式开工"),
            })
            .await
            .expect("start session");
            match app
                .dispatch(Command::RunIssue {
                    session,
                    id: issue.id,
                })
                .await
            {
                Ok(()) => println!("[RunIssue #{n}] dispatch 成功返回"),
                Err(e) => println!(
                    "[RunIssue #{n}] dispatch 报错(物化发生在这之前的 prepare_issue_run 里,不受影响):{e}"
                ),
            }
            let after = store.get_issue(issue.id).await.unwrap().unwrap();
            println!(
                "[读回] #{} status={:?} settled={}",
                after.number,
                after.status,
                after.settled_at.is_some()
            );
        }

        other => {
            eprintln!("未知动作 {other:?}");
            std::process::exit(2);
        }
    }
}
