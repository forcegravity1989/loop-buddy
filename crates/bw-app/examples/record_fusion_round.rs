//! **本轮融合收口的真实落账。** 把 2026-07-15 这轮**真实发生**的工作(multica×BW
//! 合并、Done 边沿记账焊接、整体方案对齐)记进桌面演示库 `bw-demo.db`——每条 Issue
//! 对应一个可独立核验的真实 commit,assignee 是对应阶段的五角色 agent 实体(实际
//! 执行者在 desc 里如实署名)。走的全是公开 `Command` 路径:CreateProject →
//! CompleteCreation(真实开仓)→ CreateIssue → AssignIssue → TransitionIssue(Done,
//! 触发 R3 记账)→ DistillSkillFromIssue(R2,带正文)。
//!
//! 幂等:同名项目已存在则整体跳过(演示库不重复落账)。
//!
//! Run: `cargo run -p bw-app --example record_fusion_round -- demo-workspaces/bw-demo.db`

use bw_app::{App, Command};
use bw_core::model::{Cadence, IssuePriority, IssueStatus, MaturityPeriod, StageKind};
use bw_core::{IssueId, ProjectId, SkillId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

const PROJECT_NAME: &str = "Builders' Workbench · 完整形态(multica 融合)";

struct RoundWork {
    stage: StageKind,
    role_agent: &'static str,
    title: &'static str,
    desc: &'static str,
    priority: IssuePriority,
}

#[tokio::main]
async fn main() {
    let db = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "demo-workspaces/bw-demo.db".into());
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.expect("open db"));
    // 与桌面同一约定:BW_WORKSPACES 或 DB 旁 workspaces/ —— CompleteCreation
    // 才会真实开仓(git init + README 首提交 + 绑 git-repo 连接器)。
    let ws_root = std::env::var("BW_WORKSPACES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::Path::new(&db)
                .parent()
                .map(|d| d.join("workspaces"))
                .unwrap_or_else(|| std::path::PathBuf::from("workspaces"))
        });
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    )
    .with_workspaces_root(ws_root);
    app.dispatch(Command::Boot).await.expect("boot");

    if app
        .snapshot()
        .projects
        .iter()
        .any(|p| p.name == PROJECT_NAME)
    {
        println!("项目「{PROJECT_NAME}」已存在——幂等跳过(不重复落账)。");
        return;
    }

    // ── 项目:工作台自举(它管理的是它自己的融合收口)──────────────────
    let project = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: project,
        name: PROJECT_NAME.into(),
        kind: "开发者工作台 · 自举".into(),
        desc: "把 multica 的 Issue/队友/复利并入九实体主线,焊上 Done 边沿记账,对齐为单一方案。\
               证据即本仓库 git 历史:c723480(合并)/ fc5bded(记账)/ plan/06(对齐)。"
            .into(),

        workspace: None,
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCycle {
        cycle: MaturityPeriod::Explore,
    })
    .await
    .unwrap();
    // CompleteCreation 真实开仓(git init + README 首提交)并绑 git-repo 连接器。
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();

    // ── 本轮真实完成的工作(每条的 evidence 都是本仓库可查的 commit)──────
    let works = [
        RoundWork {
            stage: StageKind::Prototype,
            role_agent: "原型师",
            title: "整体方案对齐:plan/06(multica × BW 单一事实)",
            desc: "十实体一张图 + 两条 settle 路径汇一组记账函数 + 缺口台账合一。\
                   实际执行者:fable5(本会话)。证据:plan/06-overall-alignment.md 所在 commit。",
            priority: IssuePriority::Urgent,
        },
        RoundWork {
            stage: StageKind::Build,
            role_agent: "构建师",
            title: "merge:bw-complete-form(R1 Issue 层+R2 复利)并入九实体主线",
            desc: "8 文件冲突消解(导入并集/SELECT 三列并集/两侧共存);蒸馏技能补 content 正文;\
                   BW_PANEL 深链纳入 issues。实际执行者:fable5。证据:commit c723480,166 测试 0 失败。",
            priority: IssuePriority::High,
        },
        RoundWork {
            stage: StageKind::Build,
            role_agent: "构建师",
            title: "R3:Issue Done 边沿 = issue 侧 settle(真实记账联动)",
            desc: "读前态守卫的 …→Done 边沿:assignee 记 runs/wins,工作区产物按 issue 阶段登记;\
                   重复 Done 不重计,Cancelled 拒造损失。实际执行者:fable5。证据:commit fc5bded,+1 测试。",
            priority: IssuePriority::High,
        },
        RoundWork {
            stage: StageKind::Optimize,
            role_agent: "优化师",
            title: "全门禁核验:167 测试 + fmt/clippy/wasm32×2/kernel-ui-free",
            desc: "合并体全绿:cargo test --workspace 167 通过 0 失败;clippy -D warnings 零告警;\
                   wasm32 keepalive×2;内核 UI-free 守卫。实际执行者:fable5。",
            priority: IssuePriority::Medium,
        },
    ];

    // 五角色 agent 实体按名解析(seed_stage_entities_if_missing 播种的真实实体)。
    let agents = app.snapshot().agents.clone();
    let agent_by_name = |name: &str| {
        agents
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("五角色实体缺失:{name}"))
            .id
    };

    println!("▶ 本轮真实工作 → Issue(项目「{PROJECT_NAME}」):\n");
    let mut first_build_issue: Option<IssueId> = None;
    for w in &works {
        let iid = IssueId::new();
        app.dispatch(Command::CreateIssue {
            id: iid,
            stage: w.stage,
            title: w.title.into(),
            desc: w.desc.into(),
            priority: w.priority,
        })
        .await
        .unwrap();
        app.dispatch(Command::AssignIssue {
            id: iid,
            assignee: Some(agent_by_name(w.role_agent)),
        })
        .await
        .unwrap();
        // 真实生命周期推进(不跳态,A5-F 合法链):Todo → InProgress → InReview → Done。
        for st in [
            IssueStatus::Todo,
            IssueStatus::InProgress,
            IssueStatus::InReview,
            IssueStatus::Done,
        ] {
            app.dispatch(Command::TransitionIssue {
                id: iid,
                status: st,
            })
            .await
            .unwrap();
        }
        if w.title.starts_with("merge") && first_build_issue.is_none() {
            first_build_issue = Some(iid);
        }
        println!(
            "   · [{}→{}]「{}」→ Done",
            w.stage.label(),
            w.role_agent,
            w.title
        );
    }

    // ── R2 复利:从「合并」这件 Done 真活蒸馏方法技能(带正文+溯源)────────
    app.dispatch(Command::DistillSkillFromIssue {
        skill_id: SkillId::new(),
        issue_id: first_build_issue.expect("merge issue exists"),
        name: "同源双分支合并消解法".into(),
        desc: "从本轮 multica×BW 真实合并蒸馏:加法冲突的系统化消解手法。".into(),
        category: "方法论".into(),
        content: "\
## 同源双分支合并消解法(从真实合并蒸馏)\n\
1. 先取两侧对共同基点的净 diff,判定冲突性质(加法/改写/移动)。\n\
2. 导入/枚举类冲突取并集去重;SELECT 列清单取全列并集,读回函数同步。\n\
3. 区块交错冲突以一侧全文为基底,把另一侧净增量按锚点整体植入。\n\
4. keep-both 拼接后必查函数闭合(编译器 unclosed delimiter 即断点)。\n\
5. 语义交汇点(如 蒸馏×正文)当场焊接并加测试,不留「并排共存」。\n\
6. 门禁全绿才算合并完成:fmt/clippy/test/目标平台 keepalive/架构守卫。"
            .into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RefreshHubs).await.unwrap();

    // ── 真实读回(全部从 store 派生)─────────────────────────────────────
    let snap = app.snapshot();
    let issues: Vec<_> = snap.issues.iter().collect();
    let done = issues
        .iter()
        .filter(|i| i.status == IssueStatus::Done)
        .count();
    let provenance = snap
        .skills
        .iter()
        .filter(|s| s.distilled_from_issue.is_some())
        .count();
    println!("\n╔══ 真实读回(bw-demo.db)══╗");
    for name in ["原型师", "构建师", "优化师"] {
        let a = snap.agents.iter().find(|a| a.name == name).unwrap();
        println!("  {name}:runs={} wins 派生 win_rate={}", a.runs, a.win_rate);
    }
    println!("  本项目 Issue:{}(Done={done})", issues.len());
    println!("  带 provenance 的技能:{provenance}");
    let arts = store.list_artifacts(project).await.unwrap();
    println!("  本项目产物登记:{} 行(开仓 README 等真实文件)", arts.len());
    println!("╚═══════════════════════════╝");
    println!("\nDB: {db}(桌面打开:BW_DB=... BW_OPEN={PROJECT_NAME} BW_PANEL=issues)");
}
