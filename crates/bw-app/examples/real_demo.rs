//! **real_demo — 主环指挥器(headless conductor)。**
//!
//! 不开界面,直接驱动内核把 1–2 个小需求从 0→1 走完 Builders' Workbench 的
//! 真正生命周期——**产品的主环**,而不是任何"剧本":
//!
//! ```text
//! 创建流程 → 每个阶段:建一张 Issue → 指派给该阶段的 AI 队友 → ▶跑
//!           (`RunIssue`,与桌面 Issue 看板同一条命令)→ 队友干完停在
//!           「进行中」→ 脚本**代人**推到「评审中」再点「完成」(输出里明写)
//!           → 一键蒸馏成技能(记着来自哪件活)→ 证据采集回流观测 → DoD
//!           按证据勾 → 交棒(缺什么如实带险)→ 运维回流原型
//! ```
//!
//! **它永远是 mock**:项目不配真实工作区(`workspace_path` 为空),所以每次
//! `RunIssue` 都落在 `MockInteractiveExecutor` 上——完成即返回、产出自我标注
//! 【mock】。真跑 `claude` 只发生在桌面内嵌终端里(受信任对话框/网关影响,
//! 不是常绿验证手段——CLAUDE.md 核心纪律 3)。这个例子存在的意义是廉价、
//! 无网关依赖地证明**管线本身**是真的:Issue 状态机、指派/战绩、技能 uses、
//! 蒸馏来源、交棒记录、观测/信号,全部经真实 `Command` 落库,`sqlite3` 可读回。
//!
//! 诚实约束:
//! 1. **幂等**:每个阶段以 Issue 标题为幂等键;重跑只补没发生过的步骤(未建的
//!    建、未跑的跑、未 Done 的推、未蒸馏的蒸馏、未交棒的交),不重复、不覆盖。
//! 2. **「完成」由脚本代人点,输出里明写**——产品铁律是「完成永远由人显式
//!    点」,这里脚本扮演的就是那个人,不是自动完成。
//! 3. **DoD 只按证据勾**:谓词能核实的才勾;核实不了的如实不勾,险交棒并在
//!    note 里写明缺什么。mock 跑不产出文件,所以大多数项会如实留空。
//! 4. **结论真实读回**:结尾的汇总与导出的 evidence JSON 全部从 DB 与工作区
//!    读回,绝不硬编码。
//!
//! 用法:
//!   cargo run -p bw-app --example real_demo -- <db-path> <workspaces-root> [--only <slug>]

use bw_app::{App, Command};
use bw_core::model::{
    Cadence, IssuePriority, IssueStatus, MaturityPeriod, SourceKind, StageKind,
    CONNECTOR_KIND_GIT_REPO,
};
use bw_core::{ConnectorId, CronTaskId, IssueId, MetricId, ProjectId, SessionId, SkillId};
use bw_engine::{evidence, ClaudeCliConfig};
use bw_store::{MetricRole, SessionKind, SqliteStore, Store};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

/// One real, small requirement to take 0→1.
struct Requirement {
    slug: &'static str,
    name: &'static str,
    kind: &'static str,
    desc: &'static str,
    benchmark: &'static str,
    opportunity: &'static str,
    north_star: &'static str,
    ns_def: &'static str,
}

const REQUIREMENTS: &[Requirement] = &[
    Requirement {
        slug: "linkcheck-md",
        name: "linkcheck-md",
        kind: "CLI 工具 · Rust",
        desc: "扫描 Markdown 文件里的链接并报告死链（相对路径的本地文件链接为主）。\
               输入一个文件或目录，输出死链清单；有死链时以非零码退出，可进 CI 门禁。",
        benchmark: "lychee（Rust 死链检查器，功能全但依赖重）\nmarkdown-link-check（Node 生态，需 npm）",
        opportunity: "三个月内：builders-workbench 仓库 plan/ 与 iterations/ 的文档死链检查\
                      进入一键门禁（healthcheck 可直接调用），误报可控。",
        north_star: "对真实文档目录跑通死链检查：报告准确、退出码正确、单次 < 5s",
        ns_def: "对一个含真实死链的 Markdown 目录运行 linkcheck-md，报出的死链与人工核对一致，\
                 无死链时退出码 0，有死链时非 0。",
    },
    Requirement {
        slug: "standup-digest",
        name: "standup-digest",
        kind: "CLI 工具 · Rust",
        desc: "从 git log 生成站会摘要：按作者与日期分组最近 N 天的真实提交，\
               输出可直接贴进周报的 Markdown 摘要。",
        benchmark: "git-standup（shell 脚本，star 数高但输出不结构化）\ngit shortlog（内建，无日期分组叙事）",
        opportunity: "三个月内：对任意真实仓库（含 builders-workbench 自己）一键生成本周站会\
                      摘要，输出无需手工修正即可用。",
        north_star: "对一个真实 git 仓库生成的周摘要无需手工修正即可用",
        ns_def: "standup-digest 对真实仓库的 git log 输出按作者/日期分组的 Markdown 摘要，\
                 分组正确、日期正确、空仓库不崩溃。",
    },
];

fn now_label() -> String {
    // The workspace `time` build has no `formatting` feature — a raw unix
    // timestamp is still a real, verifiable clock reading.
    format!("unix {}", time::OffsetDateTime::now_utc().unix_timestamp())
}

/// Run a real subprocess in `dir`, capture stdout+stderr. Returns
/// `(success, combined_output)` — never panics on spawn failure.
async fn run_in(dir: &Path, cmd: &str, args: &[&str]) -> (bool, String) {
    let out = tokio::process::Command::new(cmd)
        .current_dir(dir)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;
    match out {
        Ok(o) => {
            let mut text = String::from_utf8_lossy(&o.stdout).into_owned();
            text.push_str(&String::from_utf8_lossy(&o.stderr));
            (o.status.success(), text)
        }
        Err(e) => (false, format!("spawn {cmd} failed: {e}")),
    }
}

/// Create (idempotently) the requirement's real workspace — the same
/// provisioner `CompleteCreation` uses (README from the requirement's own
/// brief + .gitignore + one real first commit), so conductor-made and
/// creation-flow-made repos are indistinguishable. The project itself is
/// NOT pointed at it (mock stays mock); it exists so the git-repo connector
/// and the artifact scan have a real directory to read.
async fn ensure_workspace(root: &Path, req: &Requirement) -> PathBuf {
    let dir = root.join(req.slug);
    let existed = dir.join(".git").exists();
    bw_engine::provision_git_workspace(&dir, req.name, req.desc)
        .await
        .expect("provision workspace");
    println!(
        "  [{}] {}",
        if existed {
            "工作区已存在"
        } else {
            "工作区已开仓"
        },
        dir.display()
    );
    dir
}

/// Really run `cargo test` in the workspace (when it is a Rust project) and
/// parse the real pass/total counts. `None` when there's no Cargo.toml or the
/// output carried no test-result lines (e.g. build error) — unknown, not 0.
async fn measure_tests(workspace: &Path) -> Option<(u32, u32)> {
    if !workspace.join("Cargo.toml").exists() {
        return None;
    }
    let (_ok, out) = run_in(workspace, "cargo", &["test", "-q"]).await;
    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut saw_any = false;
    for line in out.lines() {
        // e.g. "test result: ok. 5 passed; 0 failed; ..." (also FAILED lines)
        if let Some(rest) = line.split("test result:").nth(1) {
            let mut p = 0u32;
            let mut f = 0u32;
            let words: Vec<&str> = rest.split_whitespace().collect();
            for w in words.windows(2) {
                if w[1].starts_with("passed") {
                    p = w[0].parse().unwrap_or(0);
                }
                if w[1].starts_with("failed") {
                    f = w[0].trim_end_matches(';').parse().unwrap_or(0);
                }
            }
            passed += p;
            failed += f;
            saw_any = true;
        }
    }
    if saw_any {
        Some((passed, passed + failed))
    } else {
        None
    }
}

/// Find a metric by real, stable name — or define it once (initial value is
/// the honest current state, typically zero).
#[allow(clippy::too_many_arguments)]
async fn find_or_create_metric(
    app: &mut App,
    store: &Arc<dyn Store>,
    project: ProjectId,
    name: &str,
    def: &str,
    role: MetricRole,
    target: &str,
    initial_value: &str,
) -> MetricId {
    let sigs = store.persisted_signals(project).await.unwrap();
    if let Some(m) = sigs.metrics.iter().find(|m| m.name == name) {
        return m.id;
    }
    let id = MetricId::new();
    app.dispatch(Command::UpsertManualMetric {
        id,
        name: name.to_string(),
        def: def.to_string(),
        role,
        stage_kind: None,
        target: target.to_string(),
        amber: Default::default(),
        value: initial_value.to_string(),
    })
    .await
    .expect("define metric");
    id
}

/// The evidence-gated DoD plan for one stage: which checklist boxes the
/// conductor may honestly check, given what it can really verify in the
/// workspace. Everything else stays unchecked and is named in the handoff
/// note — an unverifiable claim is never a checked box. (A mock run writes
/// no files, so most boxes stay honestly empty; the predicates are kept so
/// the same conductor reads a real workspace correctly.)
async fn dod_evidence(kind: StageKind, ws: &Path) -> (Vec<bool>, Vec<String>) {
    let exists = |p: &str| ws.join(p).exists();
    let mut checks = vec![false; kind.dod_items().len()];
    let mut why: Vec<String> = Vec::new();
    match kind {
        StageKind::Prototype => {
            // [0] 原型经真实使用·dogfood 验证 ← 验证记录文件真实存在
            checks[0] = exists("docs/validation.md");
            // [1] 北极星草案已定 ← 创建流真实录入(恒真于本 conductor 流程)
            checks[1] = true;
            // [2] Spec 骨架已从原型固化 ← SPEC 在构建段才写,如实不勾
            why.push("Spec 骨架在构建段才固化,如实不勾".into());
        }
        StageKind::Build => {
            why.push("本地 CLI 无「部署」动作,如实不勾(growth 段验证可安装性)".into());
            why.push("无遥测埋点,如实不勾".into());
            why.push("性能基线在优化段才测,如实不勾".into());
        }
        StageKind::Optimize => {
            why.push("预算体系未正式定义,如实不勾(test/clippy 结果见观测值)".into());
            checks[1] = exists("docs/bottlenecks.md") && exists("docs/regression.md");
            why.push("未做压测,如实不勾".into());
        }
        StageKind::Growth => {
            checks[0] = exists("docs/funnel.md") && exists("docs/growth-verdict.md");
            why.push("无真实投放渠道,「获客成本归因」不适用,如实不勾".into());
            why.push("SLO 清单在运维段产出,如实不勾".into());
        }
        StageKind::Ops => {
            why.push("healthcheck 只有单次运行记录,「持续达标」谈不上,如实不勾".into());
            checks[1] = exists("docs/incident-drill.md");
            checks[2] = exists("docs/retro.md");
        }
    }
    (checks, why)
}

/// The stage's role agent (「原型师」/「构建师」…) as seen from this project —
/// same by-name, scope-aware pick the desktop's assign menu and the
/// scheduler's autopilot use. `None` = no such teammate (honest unassigned).
fn stage_agent(app: &App, project: ProjectId, kind: StageKind) -> Option<bw_core::AgentId> {
    let snap = app.snapshot();
    bw_core::scope::scoped_pick(
        snap.agents.iter(),
        Some(project),
        |a| a.project_id,
        |a| a.name == kind.role_short(),
    )
    .map(|a| a.id)
}

/// The distilled-skill name for one stage of one requirement — kebab-case
/// per the skill spec (plan/16 S1), unique per project pool.
fn distilled_name(req: &Requirement, kind: StageKind) -> String {
    let stage_key = match kind {
        StageKind::Prototype => "prototype",
        StageKind::Build => "build",
        StageKind::Optimize => "optimize",
        StageKind::Growth => "growth",
        StageKind::Ops => "ops",
    };
    format!("demo-{}-{stage_key}", req.slug)
}

/// Drive one stage of the ring through the real main loop. Idempotent by
/// Issue title. Returns `false` if the stage could not be advanced (the
/// caller stops the ring for this project rather than pretending onward).
async fn run_stage(
    app: &mut App,
    store: &Arc<dyn Store>,
    req: &Requirement,
    project: ProjectId,
    kind: StageKind,
    ws: &Path,
    metrics: &DemoMetrics,
) -> bool {
    let title = format!("主环实跑 · {} · {}", kind.label(), kind.role_short());

    // ── ① 建活(幂等:同名 Issue 已在就续)──
    let existing = store
        .list_issues(project, None, None)
        .await
        .expect("list issues")
        .into_iter()
        .find(|i| i.title == title);
    let issue_id = match existing {
        Some(i) => {
            println!(
                "  [{}] Issue #{} 已在({}),续跑",
                kind.label(),
                i.number,
                i.status.label()
            );
            i.id
        }
        None => {
            let id = IssueId::new();
            app.dispatch(Command::CreateIssue {
                id,
                stage: kind,
                title: title.clone(),
                desc: format!(
                    "{}阶段的一件演示活:按「{}」的方法循环把这一段的产出做出来。\
                     (real_demo 指挥器建的活;项目未配真实工作区,▶跑 落在 mock 交互执行器上)",
                    kind.label(),
                    kind.role_short()
                ),
                priority: IssuePriority::Medium,
                standard_skill: String::new(),
            })
            .await
            .expect("create issue");
            let n = store
                .get_issue(id)
                .await
                .expect("get issue")
                .map(|i| i.number)
                .unwrap_or(0);
            println!("  [{}] 建活 Issue #{n}「{title}」", kind.label());
            id
        }
    };
    let issue = store
        .get_issue(issue_id)
        .await
        .expect("get issue")
        .expect("issue exists");

    // ── ② 指派给该阶段的 AI 队友(幂等:已指派就不动)──
    if issue.assignee.is_none() {
        match stage_agent(app, project, kind) {
            Some(aid) => {
                app.dispatch(Command::AssignIssue {
                    id: issue_id,
                    assignee: Some(aid),
                })
                .await
                .expect("assign issue");
                println!("  [{}] 指派给队友「{}」", kind.label(), kind.role_short());
            }
            None => println!(
                "  [{}] 没找到队友「{}」——如实不指派(蒸馏那一步会因此跳过)",
                kind.label(),
                kind.role_short()
            ),
        }
    }

    // ── ③ ▶跑(与桌面 Issue 看板同一条命令;mock 交互执行器,完成即返回)──
    let issue = store.get_issue(issue_id).await.unwrap().unwrap();
    if matches!(
        issue.status,
        IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
    ) {
        // The desktop's ▶跑 opens a session titled `#N title` first (the
        // left rail's index) — same here.
        let sess_title = format!("#{} {}", issue.number, issue.title);
        let session = store
            .list_sessions(project)
            .await
            .expect("list sessions")
            .into_iter()
            .find(|s| s.stage_kind == Some(kind) && s.title == sess_title)
            .map(|s| s.id)
            .unwrap_or_else(SessionId::new);
        app.dispatch(Command::StartSession {
            id: session,
            stage_kind: Some(kind),
            kind: SessionKind::Create,
            title: sess_title,
        })
        .await
        .expect("start session");
        let t0 = std::time::Instant::now();
        match app
            .dispatch(Command::RunIssue {
                session,
                id: issue_id,
            })
            .await
        {
            Ok(()) => println!(
                "  [{}] ▶跑 完成(mock 交互执行器,{:.2}s)——活停在「{}」,不自动前进",
                kind.label(),
                t0.elapsed().as_secs_f32(),
                store
                    .get_issue(issue_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status
                    .label()
            ),
            Err(e) => {
                println!("  [{}] ✗ ▶跑 失败:{e}(活如实留在原地,可重试)", kind.label());
                return false;
            }
        }
    }

    // ── ④ 脚本代人:「评审中」→「完成」(产品铁律:完成永远由人显式点;
    //      这里脚本就是那个人,输出里明写。真实运行里「评审中」由 MR 检测
    //      推,这里 mock 没有 MR,同样由脚本代推。)──
    let issue = store.get_issue(issue_id).await.unwrap().unwrap();
    if issue.status == IssueStatus::InProgress {
        app.dispatch(Command::TransitionIssue {
            id: issue_id,
            status: IssueStatus::InReview,
        })
        .await
        .expect("to in_review");
        println!(
            "  [{}] 脚本代队友推到「评审中」(mock 无 MR;真实运行由 MR 检测推)",
            kind.label()
        );
    }
    let issue = store.get_issue(issue_id).await.unwrap().unwrap();
    if issue.status == IssueStatus::InReview {
        app.dispatch(Command::TransitionIssue {
            id: issue_id,
            status: IssueStatus::Done,
        })
        .await
        .expect("to done");
        println!(
            "  [{}] **脚本代人点「完成」**(settle-once:队友记一次胜场、settled_at 打点)",
            kind.label()
        );
    }
    let issue = store.get_issue(issue_id).await.unwrap().unwrap();
    if issue.status != IssueStatus::Done {
        println!(
            "  [{}] Issue #{} 处于「{}」,不是 Done——如实停在这里,不假装往前",
            kind.label(),
            issue.number,
            issue.status.label()
        );
        return false;
    }

    // ── ⑤ 蒸馏成技能(幂等:同名技能已在本项目池就跳过;需 Done + 有指派)──
    let skill_name = distilled_name(req, kind);
    let already = store
        .list_skills()
        .await
        .expect("list skills")
        .iter()
        .any(|s| s.name == skill_name && s.project_id == Some(project));
    if already {
        println!("  [{}] 技能 `{skill_name}` 已蒸馏过,幂等跳过", kind.label());
    } else if issue.assignee.is_none() {
        println!(
            "  [{}] 未指派,蒸馏如实跳过(蒸馏要求活已完成且有指派)",
            kind.label()
        );
    } else {
        app.dispatch(Command::DistillSkillFromIssue {
            skill_id: SkillId::new(),
            issue_id,
            name: skill_name.clone(),
            desc: format!(
                "从 Issue #{}「{}」蒸馏(real_demo,mock 运行)——{}阶段「{}」方法的一次沉淀",
                issue.number,
                issue.title,
                kind.label(),
                kind.role_short()
            ),
            category: kind.role_short().to_string(),
            content: format!(
                "# {skill_name}\n\n\
                 来源:Issue #{}「{}」({}阶段,指派「{}」)。\n\n\
                 本技能由 real_demo 指挥器在 **mock** 运行后蒸馏,正文是流程说明,\
                 不是真实方法沉淀:\n\n\
                 1. 按「{}」的方法循环拆解这一段的产出;\n\
                 2. 队友在项目工作区里干活,干完停在「评审中」;\n\
                 3. 人验收后点「完成」,再一键蒸馏——下次同类活自动注入本技能。\n",
                issue.number,
                issue.title,
                kind.label(),
                kind.role_short(),
                kind.role_short()
            ),
        })
        .await
        .expect("distill skill");
        println!(
            "  [{}] 蒸馏成技能 `{skill_name}`(记着来自 Issue #{})",
            kind.label(),
            issue.number
        );
    }

    // ── ⑥ 真实证据采集(对当前工作区重新测量;git-repo 连接器是常驻采集者)──
    let ev = evidence::collect(&ws.to_string_lossy())
        .await
        .expect("collect evidence");
    println!(
        "  [证据] commits={} tracked={} docs={} dirty={}",
        ev.commit_count, ev.tracked_files, ev.docs_files, ev.dirty_paths
    );
    let repo_conn = store
        .list_connectors()
        .await
        .expect("list connectors")
        .into_iter()
        .find(|c| c.kind == CONNECTOR_KIND_GIT_REPO && c.project_id == Some(project));
    match repo_conn {
        Some(c) => {
            app.dispatch(Command::SyncConnector { id: c.id })
                .await
                .expect("sync git-repo connector");
        }
        None => println!("  [警告] 该项目没有 git-repo 连接器——工作区指标本轮无人喂"),
    }
    if let Some((passed, total)) = measure_tests(ws).await {
        println!("  [证据] cargo test 真实结果:{passed}/{total}");
        if total > 0 {
            app.dispatch(Command::RecordCollectedObservation {
                metric: metrics.tests,
                value: format!("{passed}/{total}"),
                source: SourceKind::Ci,
            })
            .await
            .expect("record tests observation");
        }
    }

    // ── ⑦ DoD:证据谓词过了才勾;ToggleDod 是翻转,先读当前值防重复翻转 ──
    let (want, why) = dod_evidence(kind, ws).await;
    let stages = store.list_stages(project).await.unwrap();
    let current = stages
        .iter()
        .find(|s| s.kind == kind)
        .map(|s| s.dod.clone())
        .unwrap_or_default();
    for (i, should) in want.iter().enumerate() {
        if *should && !current.get(i).copied().unwrap_or(false) {
            app.dispatch(Command::ToggleDod {
                stage_kind: kind,
                index: i,
            })
            .await
            .expect("toggle dod");
        }
    }
    // 阶段进度是计划数据:这一段的活 Done 了 = 100。
    app.dispatch(Command::SetStageProgress {
        stage_kind: kind,
        progress: 100,
    })
    .await
    .expect("set progress");

    // ── ⑧ 交棒(幂等:这一段已交过就不再交)──
    let handed = store
        .list_handoffs(project)
        .await
        .unwrap()
        .iter()
        .any(|h| h.from_stage == kind);
    if !handed {
        let checked = want.iter().filter(|b| **b).count();
        let total_dod = want.len();
        let risky = checked < total_dod;
        let unchecked_why = if why.is_empty() {
            String::new()
        } else {
            format!(";未勾项:{}", why.join(";"))
        };
        app.dispatch(Command::HandoffStage {
            risky,
            note: format!(
                "Issue #{} 已完成(mock 运行,脚本代人点完成)。真实证据:工作区 {} 个提交、{} 个 docs 产物。DoD {checked}/{total_dod} 按证据勾选{unchecked_why}",
                issue.number, ev.commit_count, ev.docs_files
            ),
        })
        .await
        .expect("handoff");
        println!(
            "  [{}] 交棒({},DoD {}/{})\n",
            kind.label(),
            if risky { "险交棒·如实" } else { "非险" },
            checked,
            total_dod
        );
    }
    true
}

struct DemoMetrics {
    /// The one metric the conductor still feeds directly (`Ci` source) —
    /// docs/commits moved to the project's git-repo connector.
    tests: MetricId,
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let usage = "usage: real_demo <db-path> <workspaces-root> [--only <slug>]";
    let db_path = args.get(1).cloned().expect(usage);
    let ws_root = PathBuf::from(args.get(2).cloned().expect(usage));
    let only = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1))
        .cloned();

    std::fs::create_dir_all(&ws_root).expect("create workspaces root");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(store.clone(), ClaudeCliConfig::default());
    app.dispatch(Command::Boot).await.expect("boot");

    println!(
        "=== real_demo(主环 · mock 交互执行器)· db={} · workspaces={} · {} ===\n",
        db_path,
        ws_root.display(),
        now_label()
    );

    for req in REQUIREMENTS {
        if let Some(o) = &only {
            if o != req.slug {
                continue;
            }
        }
        println!("━━ 需求「{}」({})━━", req.name, req.kind);
        let ws = ensure_workspace(&ws_root, req).await;

        // ── 创建流程(幂等:项目已存在则直接续跑)──
        let project = match store
            .list_projects()
            .await
            .unwrap()
            .into_iter()
            .find(|p| p.name == req.name)
        {
            Some(p) => {
                println!("  [项目已存在] 续跑其生命周期");
                p.id
            }
            None => {
                let id = ProjectId::new();
                app.dispatch(Command::CreateProject {
                    provider: "github".to_string(),
                    id,
                    name: req.name.into(),
                    kind: req.kind.into(),
                    desc: req.desc.into(),
                    workspace: None,
                    github: None,
                    codehub: None,
                })
                .await
                .expect("create project");
                app.dispatch(Command::SetCycle {
                    cycle: MaturityPeriod::Explore,
                })
                .await
                .expect("set cycle");
                app.dispatch(Command::UpdateBrief {
                    benchmark: req.benchmark.into(),
                    opportunity: req.opportunity.into(),
                })
                .await
                .expect("update brief");
                app.dispatch(Command::UpdateNorthStar {
                    value: req.north_star.into(),
                    def: req.ns_def.into(),
                })
                .await
                .expect("north star");
                app.dispatch(Command::CompleteCreation {
                    cadence: Cadence::Daily,
                    run_first: false,
                })
                .await
                .expect("complete creation");
                println!("  [创建流程完成] 5 阶段落库,active_stage=原型");
                id
            }
        };
        app.dispatch(Command::OpenProject(project))
            .await
            .expect("open project");
        // 项目**不**指向真实工作区:workspace_path 留空 = 每次 ▶跑 都在 mock
        // 交互执行器上,产出自我标注【mock】(见文件顶部)。

        // ── 项目的 git-repo 连接器(工作区指标的常驻采集者)──
        let existing_conns = store.list_connectors().await.unwrap();
        if !existing_conns
            .iter()
            .any(|c| c.kind == CONNECTOR_KIND_GIT_REPO && c.project_id == Some(project))
        {
            app.dispatch(Command::CreateConnector {
                id: ConnectorId::new(),
                name: format!("{} · 代码仓", req.name),
                kind: CONNECTOR_KIND_GIT_REPO.into(),
                scope: req.name.into(),
                project_id: Some(project),
                // config = the real repo path — the probe's fallback when the
                // project has no workspace_path (left empty on purpose).
                config: ws.to_string_lossy().into_owned(),
            })
            .await
            .expect("create git-repo connector");
            println!("  [连接器] git-repo 已绑定(工作区指标的常驻采集者)");
        }

        // ── 三个机器采集指标(初值 = 真实当前态)——docs/commits 定义在此,
        // 喂入由 git-repo 连接器负责(名字即契约:bw_app::METRIC_WS_DOCS /
        // METRIC_WS_COMMITS)。
        let _ = find_or_create_metric(
            &mut app,
            &store,
            project,
            bw_app::METRIC_WS_DOCS,
            "工作区 docs/ 下被 git 追踪的 .md 数 —— 真实产出物(Connector 采集)",
            MetricRole::Leading,
            "≥10",
            "0",
        )
        .await;
        let _ = find_or_create_metric(
            &mut app,
            &store,
            project,
            bw_app::METRIC_WS_COMMITS,
            "git rev-list --count HEAD —— 阶段产出被真实合入的次数(Connector 采集)",
            MetricRole::Leading,
            "≥5",
            "1",
        )
        .await;
        let metrics = DemoMetrics {
            tests: find_or_create_metric(
                &mut app,
                &store,
                project,
                "测试通过率",
                "cargo test 真实运行的 passed/total(Ci 采集);构建段起才有测试",
                MetricRole::Lagging,
                "100%",
                "0%",
            )
            .await,
        };

        // ── 五阶段环:原型 → 构建 → 优化 → 运营推广 → 运维(→ 回流)──
        let mut ring_ok = true;
        for kind in StageKind::ALL {
            if !run_stage(&mut app, &store, req, project, kind, &ws, &metrics).await {
                ring_ok = false;
                break;
            }
        }

        // ── 运维段绑定一条「定时建活」(真实调度器有真实对象可管;到点只
        // 建活,绝不自动跑活)──
        if ring_ok {
            let cron_name = format!("{} · 每日健康巡检", req.name);
            let existing = store.list_cron_tasks().await.unwrap();
            if !existing.iter().any(|c| c.name == cron_name) {
                app.dispatch(Command::CreateAutopilotTask {
                    id: CronTaskId::new(),
                    name: cron_name,
                    schedule: Cadence::Daily,
                    project_id: Some(project),
                    stage: Some(StageKind::Ops),
                    assignee: Some(StageKind::Ops.role_short().to_string()),
                })
                .await
                .expect("create cron");
                println!("  [定时任务] 每日健康巡检建活已绑定(真实调度器接管,只建活不跑活)");
            }
        }

        // ── 产物登记:项目没有 workspace_path(mock 是刻意的),仅为扫描
        // 临时绑定再清空——期间不发生任何执行。
        app.dispatch(Command::SetWorkspace {
            path: ws.to_string_lossy().into_owned(),
            allow_commands: false,
        })
        .await
        .expect("bind ws for scan");
        app.dispatch(Command::CollectArtifacts)
            .await
            .expect("collect artifacts");
        app.dispatch(Command::SetWorkspace {
            path: String::new(),
            allow_commands: false,
        })
        .await
        .expect("unbind ws after scan");

        // ── 结论:全部真实读回 ──
        let proj = store.get_project(project).await.unwrap().unwrap();
        let handoffs = store.list_handoffs(project).await.unwrap();
        let sessions = store.list_sessions(project).await.unwrap();
        let issues = store.list_issues(project, None, None).await.unwrap();
        let obs = store.list_observations(project).await.unwrap();
        let sigs = store.persisted_signals(project).await.unwrap();
        let artifacts = store.list_artifacts(project).await.unwrap();
        let connectors = store.list_connectors().await.unwrap();
        let agents = store.list_agents().await.unwrap();
        let role_agents: Vec<_> = agents
            .iter()
            .filter(|a| {
                bw_core::scope::in_scope(a.project_id, Some(project))
                    && StageKind::ALL.iter().any(|k| a.name == k.role_short())
            })
            .collect();
        let distilled: Vec<_> = store
            .list_skills()
            .await
            .unwrap()
            .into_iter()
            .filter(|s| s.project_id == Some(project) && s.name.starts_with("demo-"))
            .collect();
        println!("\n  ── 「{}」真实读回 ──", req.name);
        println!("  active_stage = {:?}(环闭合后回到原型)", proj.active_stage);
        println!(
            "  Issue {} 张(Done {} · 有 settled_at {})· 会话 {} 个 · 交接 {} 次",
            issues.len(),
            issues
                .iter()
                .filter(|i| i.status == IssueStatus::Done)
                .count(),
            issues.iter().filter(|i| i.settled_at.is_some()).count(),
            sessions.len(),
            handoffs.len(),
        );
        for i in &issues {
            let who = i
                .assignee
                .and_then(|aid| agents.iter().find(|a| a.id == aid))
                .map(|a| a.name.as_str())
                .unwrap_or("未指派");
            println!(
                "    #{} {} · {} · {} · settled_at={}",
                i.number,
                i.title,
                i.status.label(),
                who,
                i.settled_at
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "-".into())
            );
        }
        println!("  蒸馏技能 {} 条:", distilled.len());
        for s in &distilled {
            println!(
                "    {} · 来源 Issue={} · uses={} · 正文 {} 字",
                s.name,
                s.distilled_from_issue.map(|_| "有").unwrap_or("无"),
                s.uses,
                s.content.chars().count()
            );
        }
        println!(
            "  观测 {} 条(其中机器采集 {} 条)· 项目信号 = {:?}",
            obs.len(),
            obs.iter()
                .filter(|o| !matches!(o.source, SourceKind::Manual))
                .count(),
            sigs.project,
        );
        println!(
            "  产物登记 {} 个版本 · 连接器 {} 个",
            artifacts.len(),
            connectors
                .iter()
                .filter(|c| c.project_id == Some(project))
                .count()
        );
        for a in &role_agents {
            if a.runs > 0 {
                println!(
                    "  队友战绩:{} 运行 {} 次 · 胜率 {}",
                    a.name,
                    a.runs,
                    if a.win_rate.is_empty() {
                        "—"
                    } else {
                        &a.win_rate
                    }
                );
            }
        }
        println!();

        // ── 证据导出(报告的数据源;全部读回值,无一手写)──
        let export = serde_json::json!({
            "project": {
                "name": proj.name,
                "kind": proj.kind,
                "cycle": format!("{:?}", proj.cycle),
                "active_stage": format!("{:?}", proj.active_stage),
                "north_star": proj.north_star,
                "workspace_path": proj.workspace_path,
                "signal": proj.signal.map(|s| format!("{s:?}")),
            },
            "issues": issues.iter().map(|i| serde_json::json!({
                "number": i.number,
                "title": i.title,
                "stage": i.stage.label(),
                "status": i.status.label(),
                "assignee": i.assignee.and_then(|aid| agents.iter().find(|a| a.id == aid)).map(|a| a.name.clone()),
                "settled_at": i.settled_at,
            })).collect::<Vec<_>>(),
            "distilled_skills": distilled.iter().map(|s| serde_json::json!({
                "name": s.name,
                "uses": s.uses,
                "has_source_issue": s.distilled_from_issue.is_some(),
                "content_chars": s.content.chars().count(),
            })).collect::<Vec<_>>(),
            "handoffs": handoffs.iter().map(|h| serde_json::json!({
                "from": h.from_stage.label(),
                "to": h.to_stage.label(),
                "risky": h.risky,
                "note": h.note,
                "at_unix": h.at.unix_timestamp(),
            })).collect::<Vec<_>>(),
            "sessions": sessions.iter().map(|s| serde_json::json!({
                "title": s.title,
                "stage": s.stage_kind.map(|k| k.label()),
            })).collect::<Vec<_>>(),
            "observations": obs.iter().map(|o| serde_json::json!({
                "metric": sigs.metrics.iter().find(|m| m.id == o.metric_id).map(|m| m.name.clone()),
                "source": format!("{:?}", o.source),
                "value": o.raw,
                "ts_unix": o.ts.unix_timestamp(),
            })).collect::<Vec<_>>(),
            "metric_signals": sigs.metrics.iter().map(|m| serde_json::json!({
                "name": m.name,
                "signal": m.signal.map(|s| format!("{s:?}")),
                "source": m.source.map(|s| format!("{s:?}")),
            })).collect::<Vec<_>>(),
            "artifacts": artifacts.iter().map(|a| serde_json::json!({
                "path": a.path,
                "kind": a.kind.text(),
                "bytes": a.bytes,
                "git_commit": a.git_commit,
                "stage": a.stage_kind.map(|k| k.label()),
            })).collect::<Vec<_>>(),
            "connectors": connectors.iter().map(|c| serde_json::json!({
                "name": c.name,
                "kind": c.kind,
                "status": c.status.label(),
                "last_sync": c.last_sync,
                "bound_project": c.project_id.map(|p| p.uuid().to_string()),
            })).collect::<Vec<_>>(),
            "role_agents": role_agents.iter().map(|a| serde_json::json!({
                "name": a.name,
                "runs": a.runs,
                "win_rate": a.win_rate,
            })).collect::<Vec<_>>(),
        });
        let export_path = ws_root.join(format!("evidence-{}.json", req.slug));
        std::fs::write(
            &export_path,
            serde_json::to_string_pretty(&export).expect("serialize evidence"),
        )
        .expect("write evidence json");
        println!("  [证据导出] {}", export_path.display());
        println!();
    }

    println!("=== real_demo 结束({})===", now_label());
    println!("打开桌面应用查看:BW_DB=\"{db_path}\" BW_OPEN=linkcheck-md BW_PANEL=issues cargo run -p app-desktop");
}
