//! **practice_aihot — aihot 日报践行指挥器(real conductor)。**
//!
//! 用户 2026-07-20 拍板的真实践行:以「aihot 日报」(解决 AI 热点信息过载,按
//! 用户关注面出每日日报的小 web 应用)从零到一践行 Builders' Workbench 的项目
//! 管理体系——五角色、五阶段、Issue 驱动的活、workflow、cron、技能蒸馏——
//! 全部走真实 `Command` 路径,同 `real_demo.rs` / `record_fusion_round.rs` 一脉。
//! 详见 `plan/09-aihot-practice-run.md`、`iterations/PRACTICE-AIHOT.md`。
//!
//! 诚实约束:
//! 1. **零 mock**:走 `Command` 真实路径,真实 SQLite,真实本地 git 工作区
//!    (`git init` + 真提交,不接 GitHub 远端——见 iterations/PRACTICE-AIHOT.md
//!    §0 的授权与理由)。执行器构造用 `MockExecutor` 只是因为 `App::new` 需要
//!    一个,**真正跑活走 `probe-run` 子命令显式指定 `ClaudeCliExecutor`**,
//!    其余子命令根本不触发任何 workflow 执行,不受这个默认值影响。
//! 2. **幂等**:`setup`/`cron`/`component` 全部按真实名字查重,重跑不重复造。
//!
//! 2026-07-20 探测已确认真执行器网关配额耗尽(见 practice log)——本指挥器把
//! "真跑一次留证据"和"其余活由值班 agent 直接实现、只登记证据"拆成不同子命令,
//! 不在每条活上重复撞同一堵已知墙。
//!
//! 子命令(每次调用做一件真事,方便 30 轮循环里反复调而不必重新设计):
//!   setup                                         幂等创建项目+指标+项目自有组件
//!   open-issue <stage> <priority> <assignee|-> <title> <desc>   建活+指派+转 InProgress,打印 number
//!   probe-run <number>                            对该活做一次真实 RunIssue 探测(仅用一次)
//!   settle-issue <number> <note>                   真实证据回收 + InReview → Done
//!   block-issue <number> <reason>                  转 Blocked(唯一合法路径,reason 必填)
//!   cron                                           L5:暂停旧的每日建活 cron,注册 Weekly@Optimize 治理 cron(到点建活,不自动跑)
//!   distill <number> <name> <desc> <category> <content-file>   从真实 Done 活蒸馏技能
//!   sync                                           SyncConnector(git-repo)真喂工作区指标
//!   record-metric <name> <value>                  人工记一条 append-only 观测(手填指标用)
//!   feed-telemetry                                 读真实 digests/telemetry.json,真喂命中率/连续产出天数(K4)
//!   relabel-workload-metrics                       把「本周结算活数」的定义改成诚实的工作量旁注框架(K4)
//!   weekly-review <reason>                         用当前真实派生信号记一次周复盘
//!   summary                                        真实读回汇总(agent/skill/workflow/cron/issue/observation 计数)
//!
//! 用法: cargo run -p bw-app --example practice_aihot -- <subcommand> [args...]
//! 环境变量: BW_DB(默认 practice-aihot/bw-aihot.db)· BW_WORKSPACES(默认 practice-aihot/workspaces)

use bw_core::model::{
    Cadence, CronMode, HubSource, IssuePriority, IssueStatus, LoopConfig, Maturity, ProjectCycle,
    SourceKind, StageKind, WorkflowKind, CONNECTOR_KIND_GIT_REPO,
};
use bw_core::{AgentId, CronTaskId, IssueId, MetricId, ProjectId, SessionId, SkillId, WorkflowId};
use bw_engine::{ClaudeCliConfig, Engine, MockExecutor};
use bw_store::{MetricRole, NewAgent, NewSkill, NewWorkflowSpec, SessionKind, SqliteStore, Store};
use std::sync::Arc;

use bw_app::{App, Command};

const PROJECT_NAME: &str = "aihot 日报";
// 与 bw-app::METRIC_WS_COMMITS 同名——git-repo connector 的 SyncConnector 探针
// 按名字匹配自动真喂,这里复用同一个名字，让「本周提交活跃度」这条引领指标
// 不需要任何自定义采集代码，直接吃现成的 Tier D 本地 git 真实证据。
const METRIC_COMMITS: &str = "工作区真实提交数";
const METRIC_DIGESTS: &str = "累计生成日报天数";
const METRIC_ISSUES_SETTLED: &str = "本周结算活数";
// plan/10 K4:产出连续性为纲——两条真正的产品信号,零 mock,从
// aihot/main.py 每次真实运行落盘的 digests/telemetry.json 真喂(见
// `cmd_feed_telemetry`)。取代 issue 解决数量这种工作量代理。
const METRIC_HIT_RATE: &str = "每日命中率";
const METRIC_STREAK: &str = "连续产出日报天数";

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).cloned().unwrap_or_default();

    let db = std::env::var("BW_DB").unwrap_or_else(|_| "practice-aihot/bw-aihot.db".into());
    let ws_root = std::env::var("BW_WORKSPACES")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("practice-aihot/workspaces"));

    let store: Arc<dyn Store> = Arc::new(SqliteStore::open(&db).await.expect("open db"));
    // 2026-07-21 真实探测:`ClaudeCliConfig::default()` 的 $0.50 预算对真实编码活
    // 太紧——同一网关下一次「回一个词」的空转调用就吃掉 ~$0.10(缓存/工具上下文
    // 固定开销),真实活(读文件+改代码+跑测试)几个 tool-use 回合就会撞
    // `error_max_budget_usd` 提前腰斩。这里只调宽 aihot 践行用的这份配置,
    // 不动 crate 全局默认——预算上限是产品级的花费封顶决策,不该被一次践行
    // 静默改掉。
    let claude_config = ClaudeCliConfig {
        max_budget_usd: 3.0,
        ..ClaudeCliConfig::default()
    };
    let mut app = App::new(
        store.clone(),
        Engine::new(Arc::new(MockExecutor::new())),
        claude_config,
    )
    .with_workspaces_root(ws_root);
    app.dispatch(Command::Boot).await.expect("boot");

    let project = find_or_create_project(&mut app).await;
    // 每次 `cargo run` 都是全新进程——`active_project` 是内存态,不跨进程持久,
    // `Boot` 只重载项目列表不恢复"上次开着哪个"。凡用到 `self.active()?` 的
    // 命令(CreateIssue 等)都得显式先 OpenProject。
    app.dispatch(Command::OpenProject(project))
        .await
        .expect("open project");

    match sub.as_str() {
        "setup" => cmd_setup(&mut app, &store, project).await,
        "open-issue" => cmd_open_issue(&mut app, project, &args[2..]).await,
        "probe-run" => cmd_probe_run(&mut app, project, &args[2..]).await,
        "settle-issue" => cmd_settle_issue(&mut app, &store, project, &args[2..]).await,
        "block-issue" => cmd_block_issue(&mut app, project, &args[2..]).await,
        "cron" => cmd_cron(&mut app, project).await,
        "distill" => cmd_distill(&mut app, project, &args[2..]).await,
        "sync" => cmd_sync(&mut app, &store, project).await,
        "record-metric" => cmd_record_metric(&mut app, &store, project, &args[2..]).await,
        "feed-telemetry" => cmd_feed_telemetry(&mut app, &store, project).await,
        "relabel-workload-metrics" => cmd_relabel_workload_metrics(&mut app, &store, project).await,
        "weekly-review" => cmd_weekly_review(&mut app, &args[2..]).await,
        "summary" => cmd_summary(&app, &store, project).await,
        other => {
            eprintln!("未知子命令:「{other}」");
            eprintln!(
                "用法: setup|open-issue|probe-run|settle-issue|block-issue|cron|distill|sync|\
                 record-metric|feed-telemetry|relabel-workload-metrics|weekly-review|summary"
            );
            std::process::exit(1);
        }
    }
}

/// Phase 1(幂等):项目不存在则真实走创建流——本地开仓(不接 GitHub 远端)、
/// 章程与四份标准文件由 `CreateProject`/`CompleteCreation` 内部自动写入
/// (write_charter / write_component_standards,零额外代码)、三条真实指标。
async fn find_or_create_project(app: &mut App) -> ProjectId {
    if let Some(p) = app
        .snapshot()
        .projects
        .iter()
        .find(|p| p.name == PROJECT_NAME)
    {
        return p.id;
    }
    let id = ProjectId::new();
    app.dispatch(Command::CreateProject {
        id,
        name: PROJECT_NAME.into(),
        kind: "Web 应用 · AI 热点日报".into(),
        desc: "AI 热点圈信息过多——按用户自己的关注面(关键词配置),每天从多个真实来源\
               (Hacker News、arXiv)聚合、去重、过滤,生成一份可读的每日 AI 热点摘要。"
            .into(),
        workspace: None, // 本地 mint,不绑 GitHub 远端(见文件头注释)
    })
    .await
    .expect("create aihot project");
    app.dispatch(Command::SetCycle {
        cycle: ProjectCycle::Explore,
    })
    .await
    .expect("set cycle");
    app.dispatch(Command::UpdateBrief {
        benchmark: "Feedly/RSS 阅读器(需要自己订阅源,无 AI 相关性过滤);\
                    各类「AI 日报」公众号/newsletter(人工编辑,非自动化,更新看编辑心情)"
            .into(),
        opportunity: "三个月内:每天真实生成一份不需要手动修正的 AI 热点摘要,\
                      覆盖至少两类真实来源(社区热度 + 学术前沿),命中用户自定义的关注面。"
            .into(),
    })
    .await
    .expect("update brief");
    app.dispatch(Command::UpdateNorthStar {
        value: "连续 7 天,每天都真实生成一份包含 ≥5 条命中关注面的摘要,无需手动修正即可读".into(),
        def: "按「digests/YYYY-MM-DD.md 真实存在 且 该文件命中条目数 ≥5」逐日核验,\
              统计连续满足的天数"
            .into(),
    })
    .await
    .expect("update north star");
    app.dispatch(Command::CompleteCreation {
        cadence: Cadence::Daily,
    })
    .await
    .expect("complete creation");
    id
}

/// Phase 2(幂等):三条真实指标(引领×2 + 结果×1) + 项目自有组件(一个专精
/// agent、一条方法技能、一条主 workflow——引用真实装好的 superpowers 技能,
/// `HubSource::Adopted` 如实标选型来源)。
async fn cmd_setup(app: &mut App, store: &Arc<dyn Store>, project: ProjectId) {
    let sigs = store.persisted_signals(project).await.expect("signals");
    let have = |name: &str| sigs.metrics.iter().any(|m| m.name == name);

    if !have(METRIC_COMMITS) {
        app.dispatch(Command::UpsertManualMetric {
            id: MetricId::new(),
            name: METRIC_COMMITS.into(),
            def: "工作区 git 提交总数(git-repo connector 每次 SyncConnector 真实探测自动喂,\
                  非手填)"
                .into(),
            role: MetricRole::Leading,
            stage_kind: None,
            target: "≥1/天".into(),
            amber: Default::default(),
            value: "0".into(),
        })
        .await
        .expect("metric commits");
    }
    if !have(METRIC_ISSUES_SETTLED) {
        app.dispatch(Command::UpsertManualMetric {
            id: MetricId::new(),
            name: METRIC_ISSUES_SETTLED.into(),
            def: "本周真实结算(转 Done)的活数——工作台记账自生,人工按周核对更新".into(),
            role: MetricRole::Leading,
            stage_kind: None,
            target: "≥5/周".into(),
            amber: Default::default(),
            value: "0".into(),
        })
        .await
        .expect("metric issues settled");
    }
    if !have(METRIC_DIGESTS) {
        app.dispatch(Command::UpsertManualMetric {
            id: MetricId::new(),
            name: METRIC_DIGESTS.into(),
            def: "digests/ 目录下真实生成的日报文件数(ls 可数)".into(),
            role: MetricRole::Lagging,
            stage_kind: None,
            target: "清零起步,逐日涨".into(),
            amber: Default::default(),
            value: "0".into(),
        })
        .await
        .expect("metric digests");
    }

    // 项目自有组件(践行最小切片:直接 store 写,project_id=Some——Command 层
    // 暂不带这个参数,见 crates/bw-store/src/lib.rs 的 project_id 字段注释)。
    let agents = store.list_agents().await.expect("list agents");
    if !agents.iter().any(|a| a.name == "日报编辑") {
        store
            .create_agent(NewAgent {
                id: AgentId::new(),
                name: "日报编辑".into(),
                role: "aihot 专精 · 关注面判断与摘要质量把关".into(),
                maturity: Maturity::Fresh,
                skills: vec!["关键词关注面打分法".into()],
                model: "claude CLI · 跟随执行器配置".into(),
                instructions: "你负责判断一条真实抓取到的条目是否命中用户配置的关注面\
                    (config.json 的 keywords),以及摘要文案是否清楚——不做抓取本身\
                    (那是构建师的活),只做「这条该不该上日报、摘要写得好不好」的判断。\
                    绝不为了凑数把不相关条目硬塞进日报。"
                    .into(),
                project_id: Some(project),
            })
            .await
            .expect("create agent 日报编辑");
    }
    let skills = store.list_skills().await.expect("list skills");
    if !skills.iter().any(|s| s.name == "关键词关注面打分法") {
        store
            .create_skill(NewSkill {
                id: SkillId::new(),
                name: "关键词关注面打分法".into(),
                maturity: Maturity::Fresh,
                desc: "按用户配置的关注面关键词给抓取条目打分,分数不够不上日报".into(),
                category: "aihot 方法论".into(),
                source: HubSource::SelfBuilt,
                content: "### 关键词关注面打分法\n\
                    1. 读 config.json 的 keywords 列表(用户真实配置的关注面,不是猜的)。\n\
                    2. 对每条真实抓取到的标题/摘要,逐关键词做子串匹配(忽略大小写),\
                    命中数 = 分数。\n\
                    3. 分数为 0 的条目不上日报——没有例外,不为了「凑够数量」降低门槛。\n\
                    4. 命中多个关键词的条目排在日报前面(分数降序)。\n\
                    5. 同一天多条命中同一实际事件的,去重只留一条(见去重技能),\
                    不是「都留着凑数」。"
                    .into(),
                project_id: Some(project),
            })
            .await
            .expect("create skill 关键词关注面打分法");
    }
    let specs = store.list_workflow_specs().await.expect("list specs");
    if !specs.iter().any(|w| w.name == "aihot 主 workflow") {
        store
            .create_workflow_spec(NewWorkflowSpec {
                id: WorkflowId::new(),
                name: "aihot 主 workflow".into(),
                kind: WorkflowKind::Static {
                    maturity: Maturity::Fresh,
                    version: 1,
                    uses: 0,
                    scope: "aihot 日报项目专用".into(),
                    source: HubSource::Adopted,
                    trigger: None,
                },
                prompt: "贯穿 aihot 全生命周期的主线:选型引入 superpowers\
                    (marketplace: superpowers-dev,version 6.1.1,2026-07-20 真实\
                    `claude plugin install superpowers@superpowers-dev` 装入本机)\
                    的头脑风暴→写计划→按计划实现→评审方法论,不重新发明。"
                    .into(),
                goal: "每一件 aihot 的活都经这条主线的方法论落地,而不是随手写代码".into(),
                stage_ref: None,
                phases: vec![
                    "头脑风暴".into(),
                    "写计划".into(),
                    "按计划实现(TDD)".into(),
                    "请求评审".into(),
                ],
                phase_prompts: vec![
                    "调用 superpowers 的 brainstorming 技能:针对当前这件活,先发散\
                     再收敛出一个可执行方向,写清楚「为什么选这个方向、放弃了哪些」。"
                        .into(),
                    "调用 superpowers 的 writing-plans 技能:把方向落成有序步骤清单,\
                     每步都可独立验证。"
                        .into(),
                    "调用 superpowers 的 executing-plans + test-driven-development 技能:\
                     按计划逐步实现,每步先写能失败的真实检验(测试或可观察输出),\
                     再实现到通过。"
                        .into(),
                    "调用 superpowers 的 requesting-code-review / verification-before-completion \
                     技能:自查产出是否真的做到了计划里说的,如实记录还差什么。"
                        .into(),
                ],
                agents: vec![],
                skills: vec![],
                loop_config: LoopConfig {
                    retries: 1,
                    max_iter: 3,
                },
                project_id: Some(project),
            })
            .await
            .expect("create workflow aihot 主 workflow");
    }

    println!("setup 完成:project_id={}", project.uuid());
}

fn parse_stage(s: &str) -> StageKind {
    match s {
        "prototype" => StageKind::Prototype,
        "build" => StageKind::Build,
        "optimize" => StageKind::Optimize,
        "growth" => StageKind::Growth,
        "ops" => StageKind::Ops,
        other => panic!("未知 stage:{other}(合法值:prototype/build/optimize/growth/ops)"),
    }
}

fn parse_priority(s: &str) -> IssuePriority {
    match s {
        "none" => IssuePriority::None,
        "low" => IssuePriority::Low,
        "medium" => IssuePriority::Medium,
        "high" => IssuePriority::High,
        "urgent" => IssuePriority::Urgent,
        other => panic!("未知 priority:{other}"),
    }
}

/// `open-issue <stage> <priority> <assignee|-> <title> <desc>`
async fn cmd_open_issue(app: &mut App, project: ProjectId, args: &[String]) {
    let [stage, priority, assignee, title, desc] = args else {
        panic!("用法: open-issue <stage> <priority> <assignee|-> <title> <desc>");
    };
    let id = IssueId::new();
    app.dispatch(Command::CreateIssue {
        id,
        stage: parse_stage(stage),
        title: title.clone(),
        desc: desc.clone(),
        priority: parse_priority(priority),
    })
    .await
    .expect("create issue");

    if assignee != "-" {
        let agents = app.snapshot().agents.clone();
        let a = agents
            .iter()
            .find(|a| &a.name == assignee)
            .unwrap_or_else(|| panic!("assignee 不存在:{assignee}"));
        app.dispatch(Command::AssignIssue {
            id,
            assignee: Some(a.id),
        })
        .await
        .expect("assign issue");
    }
    app.dispatch(Command::TransitionIssue {
        id,
        status: IssueStatus::Todo,
    })
    .await
    .expect("todo");
    app.dispatch(Command::TransitionIssue {
        id,
        status: IssueStatus::InProgress,
    })
    .await
    .expect("in progress");

    let issue = app
        .store()
        .get_issue(id)
        .await
        .expect("get issue")
        .expect("issue exists");
    println!(
        "opened #{} 「{}」(id={})",
        issue.number,
        issue.title,
        id.uuid()
    );
    let _ = project;
}

fn find_issue_by_number(issues: &[bw_core::model::Issue], number: i64) -> &bw_core::model::Issue {
    issues
        .iter()
        .find(|i| i64::from(i.number) == number)
        .unwrap_or_else(|| panic!("找不到 issue #{number}"))
}

/// `probe-run <number>` —— 真执行器诚实探测(只用这一次,见文件头注释)。
/// 真跑一个真实小活,让 workflow_run 留一条 100% 真实的记录(成或败都如实)。
async fn cmd_probe_run(app: &mut App, project: ProjectId, args: &[String]) {
    let [number] = args else {
        panic!("用法: probe-run <number>");
    };
    let number: i64 = number.parse().expect("number 必须是整数");
    let issues = app.store().list_issues(project, None, None).await.unwrap();
    let issue = find_issue_by_number(&issues, number);
    let issue_id = issue.id;

    let proj = app.store().get_project(project).await.unwrap().unwrap();
    if proj.workspace_path.trim().is_empty() {
        eprintln!("项目没有配置真实工作区,probe-run 无意义(会走 MockExecutor)");
        std::process::exit(1);
    }

    let session = SessionId::new();
    app.dispatch(Command::StartSession {
        id: session,
        stage_kind: Some(issue.stage),
        kind: SessionKind::Optimize,
        title: format!("真执行器探测 · #{number}"),
    })
    .await
    .expect("start session");

    println!("→ 真实 RunIssue 探测开始(可能耗时数分钟,配额耗尽会较快失败)…");
    let t0 = std::time::Instant::now();
    match app
        .dispatch(Command::RunIssue {
            session,
            id: issue_id,
        })
        .await
    {
        Ok(()) => println!(
            "✓ 真实执行成功,耗时 {:.1}s(配额看来已恢复)",
            t0.elapsed().as_secs_f32()
        ),
        Err(e) => println!(
            "✗ 真实执行失败(如实记录,这正是本次探测要验证的):{e}\n  耗时 {:.1}s",
            t0.elapsed().as_secs_f32()
        ),
    }
}

/// `settle-issue <number> <note>` —— 值班 agent(本会话)已在真实工作区完成
/// 实现,回收真实证据(CollectArtifacts + SyncConnector)后转 InReview→Done。
/// 这不是"假装 agent 跑过"——`TransitionIssue` 到 Done 的唯一前提是命令层的
/// 合法转移表(InReview→Done),这条链路本来就是"人确认完成"的真实路径,
/// 本次由值班 agent 代行人工复核(用户已授权整夜自主决策,见 practice log §0)。
async fn cmd_settle_issue(
    app: &mut App,
    store: &Arc<dyn Store>,
    project: ProjectId,
    args: &[String],
) {
    let [number, note] = args else {
        panic!("用法: settle-issue <number> <note>");
    };
    let number: i64 = number.parse().expect("number 必须是整数");
    let issues = app.store().list_issues(project, None, None).await.unwrap();
    let issue = find_issue_by_number(&issues, number).clone();

    // 真实证据回收(工作区有配置才有意义)。
    let proj = app.store().get_project(project).await.unwrap().unwrap();
    if !proj.workspace_path.trim().is_empty() {
        app.dispatch(Command::CollectArtifacts)
            .await
            .expect("collect artifacts");
        if let Some(c) = store
            .list_connectors()
            .await
            .unwrap()
            .into_iter()
            .find(|c| c.kind == CONNECTOR_KIND_GIT_REPO && c.project_id == Some(project))
        {
            app.dispatch(Command::SyncConnector { id: c.id })
                .await
                .expect("sync connector");
        }
    }

    let cur = issue.status;
    if cur == IssueStatus::InProgress {
        app.dispatch(Command::TransitionIssue {
            id: issue.id,
            status: IssueStatus::InReview,
        })
        .await
        .expect("in review");
    }
    app.dispatch(Command::TransitionIssue {
        id: issue.id,
        status: IssueStatus::Done,
    })
    .await
    .expect("done");

    println!("settled #{number}「{}」→ Done。{note}", issue.title);
}

/// `block-issue <number> <reason>`
async fn cmd_block_issue(app: &mut App, project: ProjectId, args: &[String]) {
    let [number, reason] = args else {
        panic!("用法: block-issue <number> <reason>");
    };
    let number: i64 = number.parse().expect("number 必须是整数");
    let issues = app.store().list_issues(project, None, None).await.unwrap();
    let issue = find_issue_by_number(&issues, number);
    app.dispatch(Command::BlockIssue {
        id: issue.id,
        reason: reason.clone(),
    })
    .await
    .expect("block issue");
    println!("blocked #{number}:{reason}");
}

/// `cron` —— L5(plan/11)修正 · aihot 真实运行态/开发态双环拆分。
///
/// **原设的真实错误**:上一夜把「aihot 每日日报生成」注册成 Daily +
/// `CreateIssue`@Build——对一个已建成的产品,这等于"每天自动建一件开发
/// 任务"。真实产品不该每天都在建新活;已建成产品的开发活该是**事件/阈值
/// 触发**(命中率跌破阈值、或断更了),不是无脑每日。这条也把作用阶段错标
/// 成 Build(从零构建),而 aihot 早过了构建段,持续改进应该落在 Optimize。
///
/// **真实修法(留痕,不是静默改库)**:老任务如果还在(名字仍是旧名),先
/// `SetCronStatus(Paused)` 真实暂停它(留痕:Cron Hub 里能看到它被标记
/// 暂停,不是凭空消失);再注册新任务(Weekly、Optimize)。**诚实局限**:
/// BW 的 `cron_due` 只按经过时间判断,没有"指标跌破阈值才触发"的能力——
/// 真正的阈值触发需要给调度器加度量条件读取,这是比本轮范围更大的引擎
/// 改动,不在这里假装做到。Weekly 是诚实的退让:比每日合理,但不是精确
/// 的阈值门。
///
/// **运行态(每天真实产出日报 + 写 telemetry.json)不经过这条 cron**——那是
/// `aihot/main.py` 自己的确定性脚本,该由真实的 OS 级调度(launchd/cron)
/// 每天调用它,再调 `feed-telemetry` 子命令把产出真实喂进 BW(命中率/连续
/// 产出天数)。BW 自己的 `CronTask` 今天只有 `RunWorkflow`(过 agent 执行
/// 器,会真花钱)和 `CreateIssue` 两种 mode,都不是"跑一个确定性脚本"——
/// 如实留白,不为了看起来"全自动"而建一个真点了会让 agent 花钱重做脚本
/// 已经做完的事的假 cron。示例调度(用户在自己机器上配置,不是 BW 内建):
/// ```text
/// # crontab -e:每天 07:00 真实生成日报 + 真实喂 telemetry
/// 0 7 * * * cd <repo> && python -m aihot.main --out <ws>/aihot/digests \
///   && BW_DB=<db> BW_WORKSPACES=<ws-root> \
///      cargo run -p bw-app --example practice_aihot -- feed-telemetry
/// ```
async fn cmd_cron(app: &mut App, project: ProjectId) {
    const OLD_NAME: &str = "aihot 每日日报生成";
    const NEW_NAME: &str = "aihot 治理复盘 · 按需开发";

    let tasks = app.snapshot().cron_tasks.clone();
    if let Some(old) = tasks
        .iter()
        .find(|t| t.name == OLD_NAME && t.status != bw_core::model::CronStatus::Paused)
    {
        app.dispatch(Command::SetCronStatus {
            id: old.id,
            status: bw_core::model::CronStatus::Paused,
        })
        .await
        .expect("pause old daily-build autopilot cron");
        println!("已暂停旧的每日建活 cron「{OLD_NAME}」(Build 段、Daily——对已建成产品是错的配置,留痕而非静默删除)");
    }

    if tasks.iter().any(|t| t.name == NEW_NAME) {
        println!("cron 任务「{NEW_NAME}」已存在,跳过");
        return;
    }
    app.dispatch(Command::CreateAutopilotTask {
        id: CronTaskId::new(),
        name: NEW_NAME.into(),
        schedule: Cadence::Weekly,
        project_id: Some(project),
        stage: StageKind::Optimize,
        assignee: Some("日报编辑".into()),
    })
    .await
    .expect("create autopilot task");
    println!(
        "cron 任务已注册(mode=create_issue,Weekly@Optimize,到点只建活,不自动跑——no-hijack)。\
         真实运行态(每日产出+telemetry)不在 BW cron 里,由 OS 级调度跑 aihot/main.py \
         + feed-telemetry,见本函数文档注释里的 crontab 示例。"
    );
    let _ = CronMode::CreateIssue; // 文档锚点:此路径正是这个变体
}

/// `distill <number> <name> <desc> <category> <content-file>`
async fn cmd_distill(app: &mut App, project: ProjectId, args: &[String]) {
    let [number, name, desc, category, content_file] = args else {
        panic!("用法: distill <number> <name> <desc> <category> <content-file>");
    };
    let number: i64 = number.parse().expect("number 必须是整数");
    let issues = app.store().list_issues(project, None, None).await.unwrap();
    let issue = find_issue_by_number(&issues, number);
    let content = std::fs::read_to_string(content_file)
        .unwrap_or_else(|e| panic!("读 {content_file} 失败:{e}"));

    app.dispatch(Command::DistillSkillFromIssue {
        skill_id: SkillId::new(),
        issue_id: issue.id,
        name: name.clone(),
        desc: desc.clone(),
        category: category.clone(),
        content,
    })
    .await
    .expect("distill skill");
    println!("已从 #{number} 蒸馏技能「{name}」");
}

/// `sync` —— 单独触发一次 git-repo connector 真实探测(喂 METRIC_COMMITS)。
async fn cmd_sync(app: &mut App, store: &Arc<dyn Store>, project: ProjectId) {
    if let Some(c) = store
        .list_connectors()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.kind == CONNECTOR_KIND_GIT_REPO && c.project_id == Some(project))
    {
        app.dispatch(Command::SyncConnector { id: c.id })
            .await
            .expect("sync");
        println!("已同步 git-repo connector");
    } else {
        println!("没有找到 aihot 的 git-repo connector");
    }
}

/// `record-metric <name> <value>` —— 人工按真实观察记一条 append-only 观测
/// (METRIC_COMMITS 走 `sync` 自动喂,这个子命令给另外两条手填指标用)。
async fn cmd_record_metric(
    app: &mut App,
    store: &Arc<dyn Store>,
    project: ProjectId,
    args: &[String],
) {
    let [name, value] = args else {
        panic!("用法: record-metric <name> <value>");
    };
    let sigs = store.persisted_signals(project).await.unwrap();
    let m = sigs
        .metrics
        .iter()
        .find(|m| &m.name == name)
        .unwrap_or_else(|| panic!("指标不存在:{name}"));
    app.dispatch(Command::RecordObservation {
        metric: m.id,
        value: value.clone(),
    })
    .await
    .expect("record observation");
    println!("已记录「{name}」= {value}");
}

/// `feed-telemetry` —— plan/10 K4:读真实 `digests/telemetry.json`(main.py 每次
/// 真实运行落盘),幂等建两条真产品指标(命中率/连续产出天数)并记一条真实
/// `SourceKind::Telemetry` 观测。零 mock——数字来自 aihot 自己的真实运行,
/// 不是这个指挥器编的。首次调用建指标(target 用 derive 引擎认得的语法:
/// `≥8%` 阈值比较、`↑` 方向性——不是自由文本),之后每次调用只追加观测。
async fn cmd_feed_telemetry(app: &mut App, store: &Arc<dyn Store>, project: ProjectId) {
    let proj = app.store().get_project(project).await.unwrap().unwrap();
    let telemetry_path =
        std::path::Path::new(proj.workspace_path.trim()).join("digests/telemetry.json");
    let raw_text = std::fs::read_to_string(&telemetry_path)
        .unwrap_or_else(|e| panic!("读 {telemetry_path:?} 失败(先跑一次真实 main.py):{e}"));
    let v: serde_json::Value = serde_json::from_str(&raw_text)
        .unwrap_or_else(|e| panic!("{telemetry_path:?} 不是合法 JSON:{e}"));
    let raw_count = v["raw"].as_f64().expect("telemetry.json 缺 raw 字段");
    let hit_count = v["hit"].as_f64().expect("telemetry.json 缺 hit 字段");
    let days = v["days"].as_u64().expect("telemetry.json 缺 days 字段");
    let hit_rate_pct = if raw_count > 0.0 {
        hit_count / raw_count * 100.0
    } else {
        0.0
    };
    let hit_rate_str = format!("{hit_rate_pct:.1}%");

    let sigs = store.persisted_signals(project).await.unwrap();
    let existing = |name: &str| sigs.metrics.iter().find(|m| m.name == name).map(|m| m.id);

    let hit_rate_id = match existing(METRIC_HIT_RATE) {
        Some(id) => id,
        None => {
            let id = MetricId::new();
            app.dispatch(Command::UpsertManualMetric {
                id,
                name: METRIC_HIT_RATE.into(),
                def: "真实产品信噪比信号:main.py 每次真实运行的 命中数/原始条目\
                      (digests/telemetry.json 真喂,零 mock)。命中率下滑预示\
                      关注面该收紧,或来源开始水化——不是造出来的健康灯。"
                    .into(),
                role: MetricRole::Leading,
                stage_kind: None,
                target: "≥8%".into(),
                amber: Default::default(),
                value: String::new(),
            })
            .await
            .expect("create metric 每日命中率");
            id
        }
    };
    let streak_id = match existing(METRIC_STREAK) {
        Some(id) => id,
        None => {
            let id = MetricId::new();
            app.dispatch(Command::UpsertManualMetric {
                id,
                name: METRIC_STREAK.into(),
                def: "真实产品结果信号:main.py 每次真实运行时从今天真实往前数的\
                      连续产出天数,有缺口就断(digests/telemetry.json 真喂)。\
                      直接回答「我到底有没有每天真的在出这个产品」。"
                    .into(),
                role: MetricRole::Lagging,
                stage_kind: None,
                target: "↑".into(),
                amber: Default::default(),
                value: String::new(),
            })
            .await
            .expect("create metric 连续产出日报天数");
            id
        }
    };

    app.dispatch(Command::RecordCollectedObservation {
        metric: hit_rate_id,
        value: hit_rate_str.clone(),
        source: SourceKind::Telemetry,
    })
    .await
    .expect("record hit rate");
    app.dispatch(Command::RecordCollectedObservation {
        metric: streak_id,
        value: days.to_string(),
        source: SourceKind::Telemetry,
    })
    .await
    .expect("record streak");

    println!(
        "已真喂「{METRIC_HIT_RATE}」={hit_rate_str}(命中{hit_count:.0}/原始{raw_count:.0})、\
         「{METRIC_STREAK}」={days} 天(来源:{telemetry_path:?})"
    );
}

/// `relabel-workload-metrics` —— plan/10 K4:把 issue 计数类指标的 def 改成
/// 诚实的"工作量参考,非产品信号"框架。这两条本来就不带 stage_kind,不参与
/// `recompute_signals` 的项目/阶段健康聚合(只有带 stage_kind 的指标才会被
/// 卷进 `by_stage`)——所以这里不是修聚合逻辑(那是全局机制,超出 aihot
/// 这一次践行的范围),只是让指标自己的定义文本如实说清楚它是什么、不是什么,
/// 不让人误读成引领性产品指标。用同一个 id 重新 Upsert(不传 value,不碰
/// 观测历史)。
async fn cmd_relabel_workload_metrics(app: &mut App, store: &Arc<dyn Store>, project: ProjectId) {
    let sigs = store.persisted_signals(project).await.unwrap();
    let Some(m) = sigs
        .metrics
        .iter()
        .find(|m| m.name == METRIC_ISSUES_SETTLED)
    else {
        println!("「{METRIC_ISSUES_SETTLED}」还不存在,先跑 setup");
        return;
    };
    app.dispatch(Command::UpsertManualMetric {
        id: m.id,
        name: METRIC_ISSUES_SETTLED.into(),
        def: "工作量参考(非产品引领指标——issue 解决数量不等于产品在变好,\
              真正的产品信号见「每日命中率」「连续产出日报天数」):\
              本周真实结算(转 Done)的活数,工作台记账自生。"
            .into(),
        role: MetricRole::Leading,
        stage_kind: None,
        target: "≥5/周".into(),
        amber: Default::default(),
        value: String::new(),
    })
    .await
    .expect("relabel 本周结算活数");
    println!("已把「{METRIC_ISSUES_SETTLED}」的定义改为诚实的工作量旁注框架");
}

/// `weekly-review <reason>` —— 用当前真实派生信号周复盘(不手设,human_override
/// 留 None;只并置事实,不编因果——是否正向,人看 reason 里的真实数据判断)。
async fn cmd_weekly_review(app: &mut App, args: &[String]) {
    let [reason] = args else {
        panic!("用法: weekly-review <reason>");
    };
    app.dispatch(Command::AnnotateWeeklyReview {
        human_override: None,
        reason: reason.clone(),
    })
    .await
    .expect("annotate weekly review");
    println!("周复盘已记录(派生信号,未手设)");
}

/// `summary` —— 真实读回汇总。每个数字都能独立用 sqlite3 核对。
async fn cmd_summary(app: &App, store: &Arc<dyn Store>, project: ProjectId) {
    let snap = app.snapshot();
    let issues = store.list_issues(project, None, None).await.unwrap();
    let done = issues
        .iter()
        .filter(|i| i.status == IssueStatus::Done)
        .count();
    let blocked = issues
        .iter()
        .filter(|i| i.status == IssueStatus::Blocked)
        .count();
    let agents = store.list_agents().await.unwrap();
    let skills = store.list_skills().await.unwrap();
    let specs = store.list_workflow_specs().await.unwrap();
    let crons = snap.cron_tasks.clone();
    let runs = store.list_all_workflow_runs(1000).await.unwrap();
    let project_runs: Vec<_> = runs
        .iter()
        .filter(|r| r.project_id == Some(project))
        .collect();
    let sigs = store.persisted_signals(project).await.unwrap();

    println!("╔══ aihot 日报 · 真实读回汇总 ══╗");
    println!(
        "  Issue 总数:{}(Done={done}, Blocked={blocked})",
        issues.len()
    );
    println!("  workflow_run(本项目):{}", project_runs.len());
    println!(
        "  agent 总数(全局+本项目):{}(本项目自有:{})",
        agents.len(),
        agents.iter().filter(|a| a.name == "日报编辑").count()
    );
    println!(
        "  skill 总数(全局+本项目):{}(蒸馏自本项目真活:{})",
        skills.len(),
        skills
            .iter()
            .filter(|s| s.distilled_from_issue.is_some())
            .count()
    );
    println!(
        "  workflow_spec 总数(全局+本项目自有:{})",
        specs.iter().filter(|w| w.name.starts_with("aihot")).count()
    );
    println!(
        "  cron_task(本项目):{}",
        crons
            .iter()
            .filter(|c| c.project_id == Some(project))
            .count()
    );
    for m in &sigs.metrics {
        println!(
            "  指标「{}」当前值={} signal={:?}",
            m.name, m.value_raw, m.signal
        );
    }
    println!("╚═══════════════════════════════╝");
}
