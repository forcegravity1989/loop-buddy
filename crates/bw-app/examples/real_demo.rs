//! **real_demo — 完整形态实跑指挥器（headless conductor）。**
//!
//! 把 1–2 个真实小需求，从 0→1 走完 Builders' Workbench 的完整生命周期：
//! 创建流程 → 五阶段环（每阶段 = 该角色的剧本工作流，经 `ClaudeCliExecutor`
//! 真实执行，产出真实文件/提交/测试）→ 真实证据采集回流度量派生链 →
//! DoD 按证据勾选 → 交棒（含 Ops→Prototype 回流）。
//!
//! 诚实约束（与 dogfood_workflowhub 同一血统，全部沿用）：
//! 1. **绝不 mock 数据**：默认模式下每个阶段都是真实 `claude -p` 子进程在
//!    真实工作区里干活；观测值全部来自真实命令输出（git/cargo）。
//!    （`--mock` 旗标存在的唯一目的是让管线自身可以先被廉价地验证——
//!    它跑出来的东西会如实标注为 mock，绝不冒充真实证据。）
//! 2. **幂等**：每个阶段以会话标题为幂等键；重跑只补没发生过的阶段，
//!    不重复、不覆盖已真实发生的历史。
//! 3. **DoD 只按证据勾**：谓词能核实的才勾；核实不了的如实不勾，
//!    险交棒并在 note 里写明缺什么。
//! 4. **结论真实读回**：结尾的汇总与导出的 evidence JSON 全部从 DB 与
//!    工作区读回，绝不硬编码。
//!
//! 用法：
//!   cargo run -p bw-app --example real_demo -- <db-path> <workspaces-root> [--mock] [--only <slug>]

use bw_app::{App, Command, Event};
use bw_core::model::{
    Cadence, ProjectCycle, SourceKind, StageKind, CONNECTOR_KIND_CLAUDE_CLI,
    CONNECTOR_KIND_GIT_REPO,
};
use bw_core::{ConnectorId, CronTaskId, MetricId, ProjectId, SessionId};
use bw_engine::{evidence, ClaudeCliConfig, Engine, MockExecutor, PermissionMode};
use bw_store::{MetricRole, SessionKind, SqliteStore, Store};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

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
/// creation-flow-made repos are indistinguishable.
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
/// note — an unverifiable claim is never a checked box.
async fn dod_evidence(kind: StageKind, ws: &Path) -> (Vec<bool>, Vec<String>) {
    let exists = |p: &str| ws.join(p).exists();
    let mut checks = vec![false; kind.dod_items().len()];
    let mut why: Vec<String> = Vec::new();
    match kind {
        StageKind::Prototype => {
            // [0] 原型经真实使用·dogfood 验证 ← 验证记录文件真实存在
            checks[0] = exists("docs/validation.md");
            // [1] 北极星草案已定 ← 创建流真实录入（恒真于本 conductor 流程）
            checks[1] = true;
            // [2] Spec 骨架已从原型固化 ← SPEC 在构建段才写，如实不勾
            why.push("Spec 骨架在构建段才固化，如实不勾".into());
        }
        StageKind::Build => {
            // [0] 生产可用 v1 已部署 ← 本地 CLI 无部署动作，如实不勾
            why.push("本地 CLI 无「部署」动作，如实不勾（growth 段验证可安装性）".into());
            // [1] 埋点齐全·北极星可采集 ← 无遥测埋点，如实不勾
            why.push("无遥测埋点，如实不勾".into());
            // [2] 性能基线已测 ← 基线在优化段测，如实不勾
            why.push("性能基线在优化段才测，如实不勾".into());
        }
        StageKind::Optimize => {
            // [0] 性能/成本/体验预算全绿 ← conductor 独立复测 test 全绿代表其中
            //     可测的部分；预算体系并未真实定义，如实不勾
            why.push("预算体系未正式定义，如实不勾（test/clippy 结果见观测值）".into());
            // [1] 债务台账已建·下线清单已执行 ← 两份真实文档都在才算
            checks[1] = exists("docs/bottlenecks.md") && exists("docs/regression.md");
            // [2] 可扛 10× 流量的压测证据 ← 没做压测，如实不勾
            why.push("未做压测，如实不勾".into());
        }
        StageKind::Growth => {
            // [0] ≥1 个可复制的增长循环 ← 漏斗与实验结论文档真实存在
            checks[0] = exists("docs/funnel.md") && exists("docs/growth-verdict.md");
            // [1] 获客/渗透成本可归因 ← 无真实投放，对本地工具不适用，如实不勾
            why.push("无真实投放渠道，「获客成本归因」不适用，如实不勾".into());
            // [2] 稳定流量下的 SLO 需求清单 ← SLO 在运维段定义，如实不勾
            why.push("SLO 清单在运维段产出，如实不勾".into());
        }
        StageKind::Ops => {
            // [0] SLO/错误预算持续达标 ← 单次绿 ≠「持续」，如实不勾
            why.push("healthcheck 只有单次运行记录，「持续达标」谈不上，如实不勾".into());
            // [1] 本轮事故已复盘 ← 演练记录真实存在
            checks[1] = exists("docs/incident-drill.md");
            // [2] 复盘洞察已回流原型段 ← retro 真实存在（其内容就是回流交接词）
            checks[2] = exists("docs/retro.md");
        }
    }
    (checks, why)
}

/// Drive one stage of the ring: run its playbook workflow for real, collect
/// real evidence, feed metrics, check what's honestly checkable, hand off.
/// Idempotent by session title. Returns `false` if the stage failed (the
/// caller stops the ring for this project rather than pretending onward).
#[allow(clippy::too_many_arguments)]
async fn run_stage(
    app: &mut App,
    store: &Arc<dyn Store>,
    project: ProjectId,
    kind: StageKind,
    ws: &Path,
    metrics: &DemoMetrics,
    real_executor: bool,
) -> bool {
    let title_base = format!("剧本实跑 · {} · {}", kind.label(), kind.role_short());
    // Idempotency = "a session with this stage's title has a settled-OK run".
    // A session whose run failed (or never settled — crash) does NOT count:
    // a re-invocation honestly re-attempts the stage under a numbered title
    // instead of skipping work that never actually succeeded.
    let sessions = store.list_sessions(project).await.expect("list sessions");
    let runs = store.list_all_workflow_runs(1000).await.expect("list runs");
    let attempts: Vec<_> = sessions
        .iter()
        .filter(|s| s.title.starts_with(&title_base))
        .collect();
    let succeeded = attempts.iter().any(|s| {
        runs.iter()
            .any(|r| r.session_id == Some(s.id) && r.status.is_ok())
    });

    if succeeded {
        println!("  [{}] 已真实成功过，幂等跳过", kind.label());
    } else {
        let title = if attempts.is_empty() {
            title_base.clone()
        } else {
            format!("{title_base} · 第{}次尝试", attempts.len() + 1)
        };
        let session = SessionId::new();
        app.dispatch(Command::StartSession {
            id: session,
            stage_kind: Some(kind),
            kind: SessionKind::Create,
            title: title.clone(),
        })
        .await
        .expect("start session");

        println!(
            "  [{}] {} 开始真实执行（{} 个 phase，{}）…",
            kind.label(),
            kind.role_short(),
            kind.method_loop().len(),
            now_label()
        );
        let t0 = std::time::Instant::now();
        // The kernel assembles the playbook (role + real project context +
        // last handoff note + workspace state) — same command the desktop
        // UI's ▶运行 dispatches. A hung claude subprocess must not hang the
        // whole demo: 75 min cap per stage (sanity probe measured ~4.5 min
        // TTFT on a trivial call, so 4-5 real phases need generous headroom);
        // on timeout the run row honestly stays "started, never settled" —
        // the crash path telemetry was built for.
        let run = tokio::time::timeout(
            Duration::from_secs(75 * 60),
            app.dispatch(Command::RunStagePlaybook {
                session,
                stage_kind: kind,
            }),
        )
        .await;
        match run {
            Err(_) => {
                println!("  [{}] ✗ 超时（75min），如实中止该项目的环", kind.label());
                return false;
            }
            Ok(Err(e)) => {
                println!(
                    "  [{}] ✗ 执行失败：{e}（run 已如实落 Failed）",
                    kind.label()
                );
                return false;
            }
            Ok(Ok(())) => {
                println!(
                    "  [{}] ✓ 完成，真实耗时 {:.1}s",
                    kind.label(),
                    t0.elapsed().as_secs_f32()
                );
            }
        }

        // Permission-denial escalation: if the CLI reported denied actions,
        // flip the process-wide commands mode to BypassPermissions — the
        // documented fallback when acceptEdits+allowedTools can't unlock
        // command execution. Logged loudly; never silent.
        if real_executor {
            let msgs = store.session_messages(session).await.unwrap_or_default();
            let denied = msgs.iter().any(|m| m.text.contains("[权限提示]"));
            if denied
                && app.snapshot().claude_config.commands_mode != PermissionMode::BypassPermissions
            {
                println!(
                    "  [{}] ⚠ 检测到权限拒绝 —— 升级 commands_mode 为 BypassPermissions（claude_cli.rs 文档中的既定退路），后续阶段生效",
                    kind.label()
                );
                let cfg = app.snapshot().claude_config.clone();
                app.dispatch(Command::SetClaudeConfig {
                    binary: cfg.binary.clone(),
                    max_budget_usd: cfg.max_budget_usd,
                    default_mode: cfg.default_mode,
                    commands_mode: PermissionMode::BypassPermissions,
                })
                .await
                .expect("escalate commands mode");
            }
        }
    }

    // ── 真实证据采集（无论本次是真跑还是幂等跳过，都对当前工作区重新测量）──
    let ev = evidence::collect(&ws.to_string_lossy())
        .await
        .expect("collect evidence");
    println!(
        "  [证据] commits={} tracked={} docs={} dirty={}",
        ev.commit_count, ev.tracked_files, ev.docs_files, ev.dirty_paths
    );
    // Connector 真喂指标 (Tier D): the project's own git-repo connector probes
    // the workspace and feeds 工作区真实提交数/剧本产物文档数 as
    // `SourceKind::Connector` observations (change-guarded — a stage that
    // moved nothing appends nothing). Replaces the conductor's former direct
    // `RecordCollectedObservation(GitPr)` writes for these two metrics: the
    // standing connector, not the demo script, is now the collector.
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
        println!("  [证据] cargo test 真实结果：{passed}/{total}");
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

    // ── DoD：证据谓词过了才勾；ToggleDod 是翻转，先读当前值防重复翻转 ──
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

    // 阶段进度是计划数据：真实跑完 = 100。
    app.dispatch(Command::SetStageProgress {
        stage_kind: kind,
        progress: 100,
    })
    .await
    .expect("set progress");

    // ── 交棒（幂等：这一段已交过就不再交）──
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
            format!("；未勾项：{}", why.join("；"))
        };
        let recent = ev
            .recent_subjects
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("｜");
        app.dispatch(Command::HandoffStage {
            risky,
            note: format!(
                "真实证据：工作区 {} 个提交、{} 个 docs 产物；最近提交：{}。DoD {checked}/{total_dod} 按证据勾选{unchecked_why}",
                ev.commit_count, ev.docs_files, recent
            ),
        })
        .await
        .expect("handoff");
        println!(
            "  [{}] 交棒（{}，DoD {}/{}）\n",
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
    let db_path = args
        .get(1)
        .cloned()
        .expect("usage: real_demo <db-path> <workspaces-root> [--mock] [--only slug]");
    let ws_root = PathBuf::from(
        args.get(2)
            .cloned()
            .expect("usage: real_demo <db-path> <workspaces-root> [--mock] [--only slug]"),
    );
    let mock = args.iter().any(|a| a == "--mock");
    let only = args
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| args.get(i + 1))
        .cloned();

    std::fs::create_dir_all(&ws_root).expect("create workspaces root");

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig {
            binary: None,
            max_budget_usd: 0.75,
            default_mode: PermissionMode::AcceptEdits,
            commands_mode: PermissionMode::AcceptEdits,
        },
    );
    app.dispatch(Command::Boot).await.expect("boot");

    // Live progress stream — the same Event bus the desktop UI consumes.
    let mut rx = app.subscribe();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            match ev {
                Event::RunStarted {
                    workflow_name,
                    agents,
                    ..
                } => {
                    let who = agents
                        .first()
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "-".into());
                    println!("      ▶ {workflow_name}（执行角色：{who}）");
                }
                Event::WorkflowProgress { phase_idx, status } => {
                    println!("      · phase {} {}", phase_idx + 1, status);
                }
                Event::WorkflowFailed(e) => println!("      ✗ {e}"),
                _ => {}
            }
        }
    });

    println!(
        "=== real_demo（{}模式）· db={} · workspaces={} · {} ===\n",
        if mock {
            "MOCK 管线自检"
        } else {
            "真实执行"
        },
        db_path,
        ws_root.display(),
        now_label()
    );

    // ── 执行器连接器(全局一次):claude CLI 的真实版本探针 ──
    {
        let conns = store.list_connectors().await.unwrap();
        let cli_conn = match conns.iter().find(|c| c.kind == CONNECTOR_KIND_CLAUDE_CLI) {
            Some(c) => c.id,
            None => {
                let id = ConnectorId::new();
                app.dispatch(Command::CreateConnector {
                    id,
                    name: "claude CLI · 执行器".into(),
                    kind: CONNECTOR_KIND_CLAUDE_CLI.into(),
                    scope: "全部项目".into(),
                    project_id: None,
                    config: String::new(),
                })
                .await
                .expect("create claude-cli connector");
                id
            }
        };
        app.dispatch(Command::SyncConnector { id: cli_conn })
            .await
            .expect("sync claude-cli connector");
        let after = store.list_connectors().await.unwrap();
        if let Some(c) = after.iter().find(|c| c.id == cli_conn) {
            println!("[执行器探针] {} → {}\n", c.name, c.status.label());
        }
    }

    for req in REQUIREMENTS {
        if let Some(o) = &only {
            if o != req.slug {
                continue;
            }
        }
        println!("━━ 需求「{}」（{}）━━", req.name, req.kind);
        let ws = ensure_workspace(&ws_root, req).await;

        // ── 创建流程（幂等：项目已存在则直接续跑）──
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
                    id,
                    name: req.name.into(),
                    kind: req.kind.into(),
                    desc: req.desc.into(),

                    workspace: None,
                    github: None,
                })
                .await
                .expect("create project");
                app.dispatch(Command::SetCycle {
                    cycle: ProjectCycle::Explore,
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
                println!("  [创建流程完成] 5 阶段落库，active_stage=原型");
                id
            }
        };
        app.dispatch(Command::OpenProject(project))
            .await
            .expect("open project");

        // Real executor: point the project at its real workspace (unless
        // --mock, in which case the empty workspace_path keeps every run
        // honestly on MockExecutor).
        if !mock {
            app.dispatch(Command::SetWorkspace {
                path: ws.to_string_lossy().into_owned(),
                allow_commands: true,
            })
            .await
            .expect("set workspace");
        }

        // ── 项目的 git-repo 连接器（工作区指标的常驻采集者）──
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
                // project runs --mock (workspace_path left empty on purpose).
                config: ws.to_string_lossy().into_owned(),
            })
            .await
            .expect("create git-repo connector");
            println!("  [连接器] git-repo 已绑定(工作区指标的常驻采集者)");
        }

        // ── 三个机器采集指标（初值 = 真实当前态）——
        // docs/commits 定义在此,喂入由 git-repo 连接器负责(名字即契约:
        // bw_app::METRIC_WS_DOCS / METRIC_WS_COMMITS)。
        let _ = find_or_create_metric(
            &mut app,
            &store,
            project,
            bw_app::METRIC_WS_DOCS,
            "工作区 docs/ 下被 git 追踪的 .md 数 —— 五角色剧本的真实产出物（Connector 采集）",
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
            "git rev-list --count HEAD —— 阶段产出被真实合入的次数（Connector 采集）",
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
                "cargo test 真实运行的 passed/total（Ci 采集）；构建段起才有测试",
                MetricRole::Lagging,
                "100%",
                "0%",
            )
            .await,
        };

        // ── 五阶段环：原型 → 构建 → 优化 → 运营推广 → 运维（→ 回流）──
        let mut ring_ok = true;
        for kind in StageKind::ALL {
            if !run_stage(&mut app, &store, project, kind, &ws, &metrics, !mock).await {
                ring_ok = false;
                break;
            }
        }

        // ── 运维段绑定真实周期巡检（真实调度器有真实对象可管）──
        if ring_ok {
            let cron_name = format!("{} · 每日健康巡检", req.name);
            let existing = store.list_cron_tasks().await.unwrap();
            if !existing.iter().any(|c| c.name == cron_name) {
                let ops_template = app
                    .snapshot()
                    .workflow_specs
                    .iter()
                    .find(|w| {
                        w.stage_ref == Some(StageKind::Ops.index())
                            && matches!(
                                &w.kind,
                                bw_core::model::WorkflowKind::Static { source, .. }
                                    if *source == bw_core::model::HubSource::SelfBuilt
                            )
                    })
                    .map(|w| w.name.clone());
                if let Some(target) = ops_template {
                    app.dispatch(Command::CreateCronTask {
                        id: CronTaskId::new(),
                        name: cron_name,
                        target,
                        schedule: Cadence::Daily,
                        project_id: Some(project),
                    })
                    .await
                    .expect("create cron");
                    println!("  [定时任务] 每日健康巡检已绑定（真实调度器接管）");
                }
            }
        }

        // ── 产物登记:真实运行已自动登记;这里再做一次显式采集兜底。
        // --mock 时项目没有 workspace_path(执行留在 Mock 是刻意的),仅为
        // 扫描临时绑定再清空——期间不发生任何执行。
        if mock {
            app.dispatch(Command::SetWorkspace {
                path: ws.to_string_lossy().into_owned(),
                allow_commands: false,
            })
            .await
            .expect("bind ws for scan");
        }
        app.dispatch(Command::CollectArtifacts)
            .await
            .expect("collect artifacts");
        if mock {
            app.dispatch(Command::SetWorkspace {
                path: String::new(),
                allow_commands: false,
            })
            .await
            .expect("unbind ws after scan");
        }

        // ── 结论：全部真实读回 ──
        let proj = store.get_project(project).await.unwrap().unwrap();
        let handoffs = store.list_handoffs(project).await.unwrap();
        let sessions = store.list_sessions(project).await.unwrap();
        let runs = store.list_all_workflow_runs(500).await.unwrap();
        let my_runs: Vec<_> = runs
            .iter()
            .filter(|r| r.project_id == Some(project))
            .collect();
        let obs = store.list_observations(project).await.unwrap();
        let sigs = store.persisted_signals(project).await.unwrap();
        let artifacts = store.list_artifacts(project).await.unwrap();
        let connectors = store.list_connectors().await.unwrap();
        let role_agents: Vec<_> = store
            .list_agents()
            .await
            .unwrap()
            .into_iter()
            .filter(|a| StageKind::ALL.iter().any(|k| a.name == k.role_short()))
            .collect();
        let stage_skills: Vec<_> = store
            .list_skills()
            .await
            .unwrap()
            .into_iter()
            .filter(|s| {
                StageKind::ALL
                    .iter()
                    .flat_map(|k| bw_core::playbook::stage_skills(*k))
                    .any(|sk| sk.name == s.name)
            })
            .collect();
        println!("\n  ── 「{}」真实读回 ──", req.name);
        println!(
            "  active_stage = {:?}（环闭合后回到原型）",
            proj.active_stage
        );
        println!(
            "  交接 {} 次 · 会话 {} 个 · 真实 run {} 条（ok {} / failed {}）",
            handoffs.len(),
            sessions.len(),
            my_runs.len(),
            my_runs.iter().filter(|r| r.status.is_ok()).count(),
            my_runs
                .iter()
                .filter(|r| matches!(r.status, bw_core::model::RunStatus::Failed))
                .count(),
        );
        println!(
            "  观测 {} 条（其中机器采集 {} 条）· 项目信号 = {:?}",
            obs.len(),
            obs.iter()
                .filter(|o| !matches!(o.source, SourceKind::Manual))
                .count(),
            sigs.project,
        );
        println!(
            "  产物登记 {} 个版本（{} 个文件）· 其中 run 产出归属 {} 个",
            artifacts.len(),
            {
                let mut paths: Vec<_> = artifacts.iter().map(|a| a.path.as_str()).collect();
                paths.sort_unstable();
                paths.dedup();
                paths.len()
            },
            artifacts
                .iter()
                .filter(|a| a.workflow_run_id.is_some())
                .count(),
        );
        for a in &role_agents {
            if a.runs > 0 {
                println!(
                    "  角色记账:{} 运行 {} 次 · 成功率 {}",
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

        // ── 证据导出（报告的数据源；全部读回值，无一手写）──
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
            "runs": my_runs.iter().map(|r| serde_json::json!({
                "workflow": r.workflow_name,
                "status": r.status.text(),
                "duration_ms": r.duration_ms,
                "phases_completed": r.phases_completed,
                "trigger": r.trigger.text(),
                "started_at": r.started_at,
                "params": serde_json::from_str::<serde_json::Value>(&r.params_json).unwrap_or(serde_json::Value::Null),
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
                "from_run": a.workflow_run_id.is_some(),
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
                "has_instructions": !a.instructions.trim().is_empty(),
            })).collect::<Vec<_>>(),
            "stage_skills": stage_skills.iter().map(|s| serde_json::json!({
                "name": s.name,
                "uses": s.uses,
                "has_content": !s.content.trim().is_empty(),
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

    println!("=== real_demo 结束（{}）===", now_label());
    println!("打开桌面应用查看:BW_DB=\"{db_path}\" cargo run -p app-desktop");
}
