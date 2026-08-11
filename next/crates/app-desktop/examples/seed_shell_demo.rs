//! `seed_shell_demo` —— 桌面壳深链验证用的造数脚本(task-s5c 深链验证的
//! 前置步骤,design-s5-hexpanel.md §7.2「深链档:渲染证明」)。
//!
//! **不是** `hex_readback` 那样的行为验收指挥器——它不断言、不判定
//! 通过/失败,唯一职责是往一个真实数据库里造一批**看得出六段每一格**
//! 的真实数据(项目/五阶段当前一棒/活及其归属阶段/三层指标及观测/一条
//! 带险交棒/一个「进行中」的运行,upstream_session 非空以验证逃生舱),
//! 然后打印出数据库路径与项目名,供人接一条真实的深链启动命令
//! (`BW_DB=<这里打印的路径> BW_OPEN=<这里打印的项目名> BW_PANEL=hex|
//! attention ./target/debug/bw-next`)。
//!
//! **造数只走两条正经路径**(任务补充要求「库里造数走正经命令路径…或
//! store 层方法,别再直接 SQL」):①`bw_app::App::dispatch`(切片五-2 建
//! 好的命令总线——本脚本借此机会真实验证 `Command::SetIssueStage` 这条
//! 本片新加的写入路径,给 issue b 一个真实的阶段归属,不再是
//! `hex_readback` 那种绕过 store 直接 `UPDATE` 的非生产写法);②`bw_store`
//! 的公开 trait 方法(`create_project`/`create_issue`/
//! `create_manual_metric`/`insert_observation`/`create_run`/
//! `mark_run_started`——这些方法本身就是各自聚合的正经写入口,不是绕
//! 过谁,只是这些聚合的"从命令总线发起"路径本片没有建,同 `hex_readback`
//! 造初始既成事实的既有先例)。
//!
//! 跑法:`cd next && cargo run -p app-desktop --example seed_shell_demo`

use bw_app::{App, Command};
use bw_core::{
    IssueId, IssueStatus, MetricId, ObservationId, ProjectId, RunId, RunState, StageKind,
};
use bw_store::{
    HandoffStore, IssueStore, MetricStore, MetricTier, NewIssue, NewProject, NewRun, RunEndKind,
    RunKind, RunStore, SqliteStore,
};
use time::OffsetDateTime;
use uuid::Uuid;

fn now_i64() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

#[tokio::main]
async fn main() {
    let db_path = std::env::temp_dir().join(format!("bw-next-shell-demo-{}.db", Uuid::new_v4()));
    let db_path_str = db_path.to_string_lossy().to_string();
    // 工作区留空(root_path=""):§2.6「现采不落库」——本脚本不造真 git 检
    // 出,交付证据第③栏因此如实显示「工作区未配置」,不是漏了,是这个
    // 演示脚本的取舍(真实项目会有真工作区,`hex_readback` 已经验过那条
    // 现采路径)。
    let store = SqliteStore::open(&db_path_str).await.expect("open demo db");
    let app = App::open(&db_path_str).await.expect("open App");

    let project_id = ProjectId::new();
    let project_name = "bw-next 深链示范项目";
    store
        .create_project(NewProject {
            id: project_id,
            name: project_name.to_string(),
            root_path: String::new(),
        })
        .await
        .expect("create_project");
    store
        .set_active_stage(project_id, StageKind::Build, now_i64())
        .await
        .expect("set_active_stage");

    // 三件活:a=Prototype 阶段(未动)、b=Build 阶段 + 评审中(待人处理②
    // 的数据源)、c=已完成。issue b 的阶段归属**经真实
    // `Command::SetIssueStage`** 写入——本片新加的写入路径,不再是
    // `hex_readback` 那种绕过 store 直接 UPDATE 的非生产写法。
    let issue_a = IssueId::new();
    let issue_b = IssueId::new();
    let issue_c = IssueId::new();
    for (id, number, title) in [
        (issue_a, 1, "原型阶段的活"),
        (issue_b, 2, "构建阶段 · 评审中的活"),
        (issue_c, 3, "已完成的活"),
    ] {
        store
            .create_issue(NewIssue {
                id,
                project_id,
                number,
                title: title.to_string(),
            })
            .await
            .expect("create_issue");
    }
    app.dispatch(Command::SetIssueStage {
        issue: issue_a,
        stage: Some(StageKind::Prototype),
    })
    .await
    .expect("SetIssueStage a");
    app.dispatch(Command::SetIssueStage {
        issue: issue_b,
        stage: Some(StageKind::Build),
    })
    .await
    .expect("SetIssueStage b (第一次真实经命令总线写 issue.stage)");
    // b: backlog -> in_progress -> in_review(经真实转移命令,不是直接写库)。
    store
        .mark_issue_in_progress(issue_b, now_i64())
        .await
        .expect("mark_issue_in_progress b");
    store
        .transition_issue_status(
            issue_b,
            IssueStatus::InProgress,
            IssueStatus::InReview,
            now_i64(),
        )
        .await
        .expect("transition b -> in_review");
    // c: backlog -> in_progress -> in_review -> done(全程走合法转移表)。
    store
        .mark_issue_in_progress(issue_c, now_i64())
        .await
        .expect("mark_issue_in_progress c");
    store
        .transition_issue_status(
            issue_c,
            IssueStatus::InProgress,
            IssueStatus::InReview,
            now_i64(),
        )
        .await
        .expect("transition c -> in_review");
    app.dispatch(Command::TransitionIssue {
        issue: issue_c,
        to: IssueStatus::Done,
    })
    .await
    .expect("TransitionIssue c -> done(完成永远人点的唯一入口)");
    store
        .settle_issue(issue_c, now_i64())
        .await
        .expect("settle_issue c");

    // 三层指标:北极星(有观测)、滞后(有观测)、引领(无观测,验证段③
    // 「无观测 · Unknown ≠ 绿」的诚实灰)。
    let north_star = MetricId::new();
    let lagging = MetricId::new();
    let leading = MetricId::new();
    store
        .create_manual_metric(
            north_star,
            project_id,
            MetricTier::NorthStar,
            "每周真实交付的活数",
            "每周结算的活数(settled_at 落在过去 7 天内)",
            "≥3",
            "manual",
            "",
            now_i64(),
        )
        .await
        .expect("create north star");
    store
        .create_manual_metric(
            lagging,
            project_id,
            MetricTier::Lagging,
            "滞后指标·周吞吐",
            "",
            "≥3",
            "manual",
            "",
            now_i64(),
        )
        .await
        .expect("create lagging");
    store
        .create_manual_metric(
            leading,
            project_id,
            MetricTier::Leading,
            "引领指标·评审周转",
            "",
            "≤2",
            "manual",
            "",
            now_i64(),
        )
        .await
        .expect("create leading");
    app.dispatch(Command::RecordManualObservation {
        metric: north_star,
        raw: "3".to_string(),
    })
    .await
    .expect("observe north star");
    app.dispatch(Command::RecordManualObservation {
        metric: lagging,
        raw: "5".to_string(),
    })
    .await
    .expect("observe lagging");
    let _: ObservationId = match app
        .dispatch(Command::RecordManualObservation {
            metric: leading,
            raw: "1".to_string(),
        })
        .await
        .expect("observe leading (only to prove the path works; superseded below)")
    {
        bw_app::Event::ObservationRecorded(id) => id,
        other => panic!("unexpected event: {other:?}"),
    };
    // leading 指标特意不留这条观测——立即停用指标重开?不,更简单:直接
    // 不再处理,让它「有过一条观测」不是本脚本要演示的那一种诚实灰;为
    // 了六段③段里出现一张真正的「无观测」灰卡,另建一条**从未观测过**
    // 的手建指标。
    let never_observed = MetricId::new();
    store
        .create_manual_metric(
            never_observed,
            project_id,
            MetricTier::Leading,
            "引领指标·从未观测过",
            "专门演示「无观测 · Unknown ≠ 绿」的诚实灰卡",
            "",
            "manual",
            "",
            now_i64(),
        )
        .await
        .expect("create never-observed leading metric");

    // 一条带险交棒,交到当前一棒(Build)——风险与决策段 + 待人处理⑤的
    // 数据源。
    store
        .handoff_stage(
            project_id,
            StageKind::Prototype,
            StageKind::Build,
            true,
            "原型阶段的探索性验证清单没跑完,带险交出",
            now_i64(),
        )
        .await
        .expect("handoff_stage");

    // 一个「进行中」的运行(issue a,state=Running,真实
    // `mark_run_started` 写一个非空 upstream_session)——逃生舱卡片
    // (design §5.2)的数据源,证明续接命令的语法(工作区路径是真实创建
    // 的临时目录,会话号是本脚本编的一个看起来真实的字符串——**不是**
    // 真的 claude 会话,这一点在下面的说明文字里如实标注,不冒充)。
    let demo_workspace =
        std::env::temp_dir().join(format!("bw-next-shell-demo-ws-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&demo_workspace).expect("mkdir demo workspace");
    let run_id = RunId::new();
    store
        .create_run(NewRun {
            id: run_id,
            project_id,
            issue_id: issue_a,
            kind: RunKind::Delivery,
            connector_name: "seed-shell-demo-fixture".to_string(),
            req_id: Uuid::new_v4().to_string(),
            workspace: demo_workspace.to_string_lossy().to_string(),
            branch: "bw/seed-shell-demo".to_string(),
            state: RunState::Starting,
            started_at: now_i64(),
        })
        .await
        .expect("create_run");
    let session = format!("demo-sess-{}", Uuid::new_v4());
    store
        .mark_run_started(run_id, &session, now_i64())
        .await
        .expect("mark_run_started");

    // 逃生舱证据(裁决 7「本片必须真能用,评审要实测」,design §5.2 允
    // 许「真实(或 mock 会话表)数据」):**调用与壳完全同一份代码**
    // (`app_desktop::escape_hatch::build`,经这个 crate 新加的库面暴
    // 露,不是本脚本另抄一遍格式化逻辑)喂给刚刚真实写进库里的这一行
    // `run`,打印出真实算出来的续接命令,供人复核语法。
    let run_row = store
        .get_run(run_id)
        .await
        .expect("get_run")
        .expect("run row must exist right after create_run + mark_run_started");
    let eh = app_desktop::escape_hatch::build(&run_row);
    println!("== 逃生舱证据(app_desktop::escape_hatch::build,真实 run 行)==");
    println!("  运行标签: {}", eh.run_label);
    println!("  上游会话: {:?}", eh.upstream_session);
    match &eh.resume_command {
        Some(cmd) => println!("  续接命令: {cmd}"),
        None => println!("  续接命令: (无——这家不支持指派会话号)"),
    }
    println!(
        "  如实标注:这条命令语法是真的(cd <工作区> && claude --resume <会话号>,\n            \
         两者都来自刚写进 run 表的真实字段);但 upstream_session 本身是本脚本编的\n            \
         演示字符串,不是一次真的 claude 会话,cd 过去 --resume 不会真的接上历史——\n            \
         这条边界在下面还会再说一遍。"
    );
    println!();
    println!("SEED_SHELL_DEMO_OK");
    println!("db={db_path_str}");
    println!("project={project_name:?}");
    println!(
        "workspace(演示用,非真实 claude 会话)={}",
        demo_workspace.display()
    );
    println!("upstream_session(演示用,非真实 claude 会话)={session}");
    println!();
    println!("深链验证命令(hex 屏):");
    println!("  BW_DB={db_path_str} BW_OPEN={project_name:?} BW_PANEL=hex ./target/debug/bw-next");
    println!("深链验证命令(attention 屏):");
    println!(
        "  BW_DB={db_path_str} BW_OPEN={project_name:?} BW_PANEL=attention ./target/debug/bw-next"
    );
    println!();
    println!(
        "如实提醒:`bw-next` 启动时会真的调用 RunManager::reap_on_restart()\n\
         (design-s4-runmanager.md §3.1「重启后收拾遗留」)——它会发现上面这个\n\
         `run` 行「关着门却没有活的进程句柄」,如实标成 orphaned 并结账(这是\n\
         正确的产品行为,不是缺陷:一件运行从来不该在没人盯着的情况下继续\n\
         『看起来在跑』)。因此这一行在 `bw-next` 第一次深链启动之后就不会再\n\
         出现在『当前 Loop』段的进行中卡片里——上面那段逃生舱证据证的是\n\
         『escape_hatch::build 这个函数对真实 run 行算出的命令语法是对的』,\n\
         不是『重启后这一行还会显示为进行中』(那要等 RunIssue 接入壳的下一\n\
         片,让一次运行的整个生命周期都发生在同一个活着的 bw-next 进程里才\n\
         成立)。"
    );

    // 附:一次「停在原地的运行」+「遗留未结账」,让 attention 屏 ①③ 两
    // 项也有真数据(issue b 追加一条失败且未结账的运行)。
    let stalled_id = RunId::new();
    store
        .create_run(NewRun {
            id: stalled_id,
            project_id,
            issue_id: issue_b,
            kind: RunKind::Delivery,
            connector_name: "seed-shell-demo-fixture".to_string(),
            req_id: Uuid::new_v4().to_string(),
            workspace: demo_workspace.to_string_lossy().to_string(),
            branch: "bw/seed-shell-demo-b".to_string(),
            state: RunState::Starting,
            started_at: now_i64(),
        })
        .await
        .expect("create_run stalled");
    store
        .close_run(
            stalled_id,
            now_i64(),
            RunState::Failed,
            Some(RunEndKind::StartFailed),
            "【seed_shell_demo 演示数据,非真实执行连接器上报】",
        )
        .await
        .expect("close_run stalled");
}
