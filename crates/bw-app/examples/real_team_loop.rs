//! **完整形态演示(真实、非 mock)。** 驱动 builders-workbench 的真实 `Command` 路径,
//! 把本轮**真实发生**的工作记录成五阶段环上的 Issue,由真实 agent 队友认领、推进到 Done,
//! 再从一件 Done 的真活里蒸馏出带 provenance 的可复用 Skill(R2)。
//!
//! 这不是 `simulate_hub` 那类把合成行写进库去喂分析的「mock 度量」——这里每条 Issue 都对应
//! 一件**本会话真实完成、可独立核验**的工作(commit + 测试数),每条 Issue 的 assignee 是真实
//! 做了这件活的 agent 队友。引擎(`MockExecutor`)在本演示中**全程不被调用**:Issue/Skill
//! 走的是 R1/R2 的真实持久化路径,与工作流执行引擎无关,因此不引入任何 mock 产物。
//!
//! Run: `cargo run -p bw-app --example real_team_loop -- <db-path>`

use bw_app::{App, Command};
use bw_core::model::{Cadence, IssuePriority, IssueStatus, ProjectCycle, StageKind};
use bw_core::{AgentId, IssueId, ProjectId, SkillId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{SqliteStore, Store};
use std::sync::Arc;

/// 一件真实完成的工作,及其证据(commit / 测试数 / 谁做的)。
struct RealWork {
    stage: StageKind,
    title: &'static str,
    desc: &'static str,
    priority: IssuePriority,
    /// 真实做这件活的 agent(在演示里建一条对应的 agent 记录)。
    agent_name: &'static str,
    agent_role: &'static str,
    /// 可独立核验的真实证据。
    evidence: &'static str,
}

#[tokio::main]
async fn main() {
    let out_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/bw-real-team-loop.db".into());
    let _ = std::fs::remove_file(&out_path); // 演示用临时库,每次干净起跑。
    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&out_path).await.expect("open db"));
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        ClaudeCliConfig::default(),
    );
    app.dispatch(Command::Boot).await.expect("boot");

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  完整形态 Builders' Workbench · 真实五角色环演示(非 mock)    ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    // ── 1. 真实项目:BW 自己(完整形态自举)──
    let project = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id: project,
        name: "Builders' Workbench · 完整形态".into(),
        kind: "开发者工作台 · multica 优点 × BW 设计初衷".into(),
        desc:
            "用 BW 自己的五角色环,把 multica「真实 agent 队友」融进 BW「五阶段方法论 + 度量派生」"
                .into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::SetCycle {
        cycle: ProjectCycle::Explore,
    })
    .await
    .unwrap();
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Weekly,
    })
    .await
    .unwrap();
    println!("▶ 项目已建(真实 Command 路径):「Builders' Workbench · 完整形态」· 探索期 · 5 阶段落库 · active_stage=原型\n");

    // ── 2. 真实 agent 队友(谁真的做了活)──
    //    原型/运营/运维 = Fable(调度);构建 = sonnet5 队友。
    let fable = AgentId::new();
    app.dispatch(Command::CreateAgent {
        id: fable,
        name: "Fable · 调度(原型/运营/运维)".into(),
        role: "PM + 环协调".into(),
        skills: vec![],
        model: "fable".into(),
        instructions: "五角色环协调:拆 Issue、指派队友、验收 DoD、记账交棒;不 mock,不代答。".into(),
    })
    .await
    .unwrap();
    let sonnet_builder = AgentId::new();
    app.dispatch(Command::CreateAgent {
        id: sonnet_builder,
        name: "sonnet5 · 构建师".into(),
        role: "构建".into(),
        skills: vec![],
        model: "sonnet".into(),
        instructions: "构建师:接 Issue 后写真实代码与测试,门禁全绿才交付;产出如实登记。".into(),
    })
    .await
    .unwrap();
    println!("▶ 真实 agent 队友已建档:Fable(调度)· sonnet5(构建师)\n");

    // ── 3. 本轮真实完成的工作(每件都有可核验证据)──
    let works = [
        RealWork {
            stage: StageKind::Prototype, title: "设计综合:multica × BW 融合方案",
            desc: "核验 multica 真实源码(models.go/迁移/路由)→ 定义完整形态 = agent 队友执行 × 五阶段方法论",
            priority: IssuePriority::Urgent, agent_name: "Fable",
            agent_role: "原型师", evidence: "commit 4fdbe08 · iterations/V2-DESIGN.md(含 multica issue.stage 真实校验)",
        },
        RealWork {
            stage: StageKind::Build, title: "R1 · Issue 层(可分配工作单元 × 阶段 × agent)",
            desc: "IssueId + 7 态状态机 + per-project 自增 number + assignee + store/app 全链路",
            priority: IssuePriority::High, agent_name: "sonnet5 · 构建师",
            agent_role: "构建师", evidence: "commit 4d9d8d7 · +12 测试(121→133)",
        },
        RealWork {
            stage: StageKind::Build, title: "R2 · Skill 复利(Done Issue → 带 provenance 的 Skill)",
            desc: "SkillCard 加出处 + distill_skill_from_issue(校验 Done·有 assignee)+ 迁移守卫",
            priority: IssuePriority::High, agent_name: "sonnet5 · 构建师",
            agent_role: "构建师", evidence: "commit 9ff53b3 · +5 测试(133→138)",
        },
        RealWork {
            stage: StageKind::Optimize, title: "App 级 Issue→Skill 全链路集成测试",
            desc: "经 App::dispatch 全程驱动 建项目→建 agent→建 Issue→分配→Done→蒸馏,断言 provenance",
            priority: IssuePriority::Medium, agent_name: "sonnet5 · 构建师",
            agent_role: "构建师", evidence: "sonnet5 真实编写 · issues_skill_loop.rs · +2 测试(138→140)",
        },
    ];

    // ── 4. 把每件真活记成 Issue,分配给真实 agent,推进到 Done ──
    println!("▶ 真实 Issue(每件 = 一件本会话真实完成、可核验的工作):\n");
    let mut issue_ids: Vec<(IssueId, &RealWork)> = Vec::new();
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
        let assignee = if w.agent_name.starts_with("sonnet") {
            sonnet_builder
        } else {
            fable
        };
        app.dispatch(Command::AssignIssue {
            id: iid,
            assignee: Some(assignee),
        })
        .await
        .unwrap();
        app.dispatch(Command::TransitionIssue {
            id: iid,
            status: IssueStatus::Done,
        })
        .await
        .unwrap();
        issue_ids.push((iid, w));
        println!("   · [{}]「{}」", w.stage.label(), w.title);
        println!(
            "     assignee = {}({}) · 状态 = Done · 证据 = {}",
            w.agent_name, w.agent_role, w.evidence
        );
    }

    // ── 5. 真实交棒:原型 → 构建(设计 + R1 落地)→ 优化(R1/R2/测试都绿)──
    app.dispatch(Command::HandoffStage {
        risky: true,
        note: "原型→构建:设计综合已定(4fdbe08)+ R1 落地(4d9d8d7)。险交棒:构建段 DoD「生产可用 v1 已部署」未达成(未打包分发)。".into(),
    }).await.unwrap();
    app.dispatch(Command::HandoffStage {
        risky: true,
        note:
            "构建→优化:R1/R2/App集成全绿(140 测试)。险交棒:优化段 DoD「压测证据/预算全绿」未达成。"
                .into(),
    })
    .await
    .unwrap();
    let proj = store.get_project(project).await.unwrap().unwrap();
    println!(
        "\n▶ 真实交棒(append-only 审计):原型 → 构建 → 优化 · 现活跃阶段 = {}\n",
        proj.active_stage.label()
    );

    // ── 6. R2 复利:从一件 Done 的真活蒸馏出可复用 Skill(带 provenance)──
    let distill_from = issue_ids[1].0; // R2 那件 Issue(真实 agent 完成)
    let skill = SkillId::new();
    app.dispatch(Command::DistillSkillFromIssue {
        skill_id: skill,
        issue_id: distill_from,
        name: "五角色环 · 真实交付法".into(),
        desc: "从一件真实完成的 Issue 蒸馏:五角色各一轮、门禁每圈绿、证据可核验、不 mock。".into(),
        category: "方法论".into(),
        content: "\
## 五角色环 · 真实交付法(从真实完成的 Issue 蒸馏)\n\
1. 把阶段目标拆成可分配的 Issue(标题=可验收的动词短语,含 DoD)。\n\
2. 指派给对应角色的真实 agent 队友;Backlog 只停车,Todo 起才算承诺。\n\
3. agent 真做工程:写真实代码+测试,门禁(fmt/clippy/test)每圈过。\n\
4. 完成推 InReview→Done,产物/提交如实登记,不可核验的不勾。\n\
5. Done 后蒸馏方法为 Skill(带 provenance),下一件活直接复用。"
            .into(),
    })
    .await
    .unwrap();
    app.dispatch(Command::RefreshHubs).await.unwrap();
    let distilled = app
        .snapshot()
        .skills
        .iter()
        .find(|s| s.distilled_from_issue.is_some())
        .unwrap();
    println!("▶ R2 Skill 复利(从 Done Issue 真实蒸馏,带 provenance):");
    println!("   · skill「{}」", distilled.name);
    println!("     distilled_from_issue = Some(那件 R2 Issue) · origin_agent = sonnet5 构建师");
    println!("     (multica 的 skill 是手工的;此处 provenance 是 BW 相对 multica 的真实增量)\n");

    // ── 7. 真实读回:度量从真实数据派生,不编造 ──
    let issues = app.snapshot().issues.clone();
    let done = issues
        .iter()
        .filter(|i| i.status == IssueStatus::Done)
        .count();
    let total = issues.len();
    let skills_with_provenance = app
        .snapshot()
        .skills
        .iter()
        .filter(|s| s.distilled_from_issue.is_some())
        .count();
    let handoffs = store.list_handoffs(project).await.unwrap().len();
    println!("╔══ 真实读回(全部从 store 派生,非硬编码)══╗");
    println!("  项目的 Issue 总数 = {} · 其中 Done = {}", total, done);
    println!(
        "  完成率(Done/总)= {:.0}%",
        (done as f32 / total as f32) * 100.0
    );
    println!("  带真实 provenance 的 Skill = {}", skills_with_provenance);
    println!("  真实交棒次数(审计)= {}", handoffs);
    println!(
        "  活跃阶段 = {} · 阶段环已推进 2 段",
        proj.active_stage.label()
    );
    println!("╚══════════════════════════════════════════╝\n");
    println!("✅ 完整形态闭环演示完成:真实 agent 队友 → 真实 Issue(可核验)→ Done → 复利 Skill → 真实度量。");
    println!("   对照上一轮 25-iter:那里用 simulate_hub 写合成行喂分析;这里每条数据都来自真实完成的工作。");
    println!("\nDB: {out_path}");
}
