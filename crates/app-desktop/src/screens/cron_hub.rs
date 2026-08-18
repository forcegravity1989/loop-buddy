//! `Hub::Cron` — scheduled tasks. Real store-backed records, and a real
//! in-process scheduler (`App::tick_scheduler`, ticked every few seconds by
//! `app-desktop/src/kernel.rs`): while this app is running, a `Normal`-
//! status task bound to a project really auto-fires on its own once
//! `bw_core::model::cron_due` says so — no click required. What's still
//! honestly *not* here: a background daemon that fires while the app is
//! fully closed (that belongs to a `Connector`/server-side piece, not a
//! desktop process) — see `tick_scheduler`'s own doc comment.
//!
//! 2026-08-18 拔旧执行引擎后,定时任务只剩两种,而且都**不跑活**:
//! - 建活(autopilot):到点在项目里新建一张 Issue(可指定阶段/指派),
//!   活本身要等人或队友去跑——定时任务绝不自动完成活(CLAUDE.md 铁律)。
//!   这是表单里唯一能新建的类型。
//! - 采集指标:到点跑该项目的 script 连接器,把输出写成观测。挂了远端仓的
//!   项目出生即自带一条(`CreateProject` 配),不走表单。
//!
//! 「▶ 立即执行」随旧引擎一起拔掉(它接的是「运行工作流」模式)。
//! 「⏸ 暂停/▶ 恢复」是真实的人工介入(`Command::SetCronStatus`)——暂停的
//! 任务是 `tick_scheduler` 永远不会自动触发的那一种,每一 tick 先查它。

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::model::{Cadence, CronStatus, StageKind};
use bw_core::CronTaskId;
use dioxus::prelude::*;
use ui::vm::{CronRowVm, ProjectCardVm};

#[component]
pub fn CronHub(hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.cron_tasks.len();

    let mut creating = use_signal(|| false);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "CRONHUB" }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin:4px 0 8px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "定时任务" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 任务" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 新建定时建活" }
                }
            }
            p { style: "color:{ink3};font-size:11.5px;line-height:1.6;margin:0 0 14px;",
                "真实调度:应用运行期间,「正常」状态且已绑定项目的任务,到期后无需点击就会在后台自动触发(每几秒检查一次)——不是应用完全关闭时也在跑的常驻守护进程。两种任务到点都真实执行、真实记账,但都不跑活:「建活」只新建 Issue,活要等人或队友去跑;「📈 采集指标」只把脚本输出写成观测(挂了远端仓的项目出生即自带一条)。「⏸ 暂停/▶ 恢复」是真实的人工介入,暂停的任务永远不会被自动触发。"
            }
            if creating() {
                CreateCronForm { projects: projects.clone(), on_done: move |_| creating.set(false) }
            }
            if hub.cron_tasks.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有定时任务——点「+ 新建定时建活」录入第一个。" }
            } else {
                div {
                    style: "{theme::card()} overflow:hidden;",
                    div {
                        style: "display:grid;grid-template-columns:1.3fr .9fr .9fr .8fr .8fr 1fr;gap:10px;padding:10px 16px;font-size:11px;color:{ink3};border-bottom:1px solid {theme::BORDER};",
                        span { "任务/到点做什么" }
                        span { "频率" }
                        span { "项目" }
                        span { "上次/下次" }
                        span { "状态" }
                        span { "操作" }
                    }
                    for c in hub.cron_tasks.clone() {
                        CronTaskRowView { key: "{c.id.uuid()}", c: c.clone() }
                    }
                }
            }
        }
    }
}

/// One `CronHub` row, keyed by `CronTaskId` in the outer `for` loop.
#[component]
fn CronTaskRowView(c: CronRowVm) -> Element {
    let k = use_context::<Kernel>();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let cron_id = c.id;
    let paused = c.status == CronStatus::Paused;
    let status_color = match c.status {
        CronStatus::Failed => theme::ALERT_DEEP,
        CronStatus::Running => theme::CLAY,
        CronStatus::Paused => ink3,
        CronStatus::Normal => ink2,
    };
    // 建活任务的副标题:作用阶段(空 = 项目当前阶段)+ 指派对象。
    let create_issue_subtitle = if !c.is_collect_metrics {
        let stage = c.issue_stage_label.unwrap_or("项目当前阶段");
        match &c.issue_assignee {
            Some(a) if !a.trim().is_empty() => format!("到点建活 · {stage} · 指派 {a}"),
            _ => format!("到点建活 · {stage} · 不指派"),
        }
    } else {
        String::new()
    };

    rsx! {
        div {
            style: "display:grid;grid-template-columns:1.3fr .9fr .9fr .8fr .8fr 1fr;gap:10px;padding:10px 16px;font-size:12.5px;align-items:center;border-bottom:1px dashed {theme::BORDER};",
            div {
                div {
                    style: "font-weight:500;display:flex;align-items:center;gap:6px;",
                    if !c.mode_icon.is_empty() {
                        span { title: "{c.mode_label}", "{c.mode_icon}" }
                    }
                    span { "{c.name}" }
                }
                if c.is_collect_metrics {
                    // PF1-3a: CollectMetrics 卡具体化,targets 非空时拼上指标名
                    // (从该项目 collect_kind='script' 的全部 metric 派生,kernel
                    // 填——不止代码仓统计,业务脚本指标也在里面)。P13(2026-08-06
                    // cowelink 验证):这一条定时器到点会跑该项目**全部** script
                    // 连接器,不是只跑代码仓统计;前缀用中性的「本项目全部
                    // script 指标」,不预设内容类别。
                    {
                        let targets = c.collect_targets.join(" / ");
                        let subtitle = if targets.is_empty() {
                            "本项目全部 script 指标 · 每日".to_string()
                        } else {
                            format!("本项目全部 script 指标({targets})· 每日")
                        };
                        rsx! {
                            div {
                                style: "font-size:11px;color:{ink3};",
                                "{subtitle}"
                            }
                        }
                    }
                } else {
                    div { style: "font-size:11px;color:{ink3};", "{create_issue_subtitle}" }
                }
            }
            span { style: "color:{ink2};", "{c.schedule_label}" }
            span { style: "color:{ink2};", "{c.project_label}" }
            div {
                div { style: "font-size:11px;color:{ink3};", "{c.last_run}" }
                div { style: "font-size:11px;color:{ink3};", "{c.next_run}" }
            }
            span { style: "{theme::chip(\"#EFE9DA\", status_color)}", "{c.status_label}" }
            div {
                style: "display:flex;gap:6px;flex-wrap:wrap;",
                if paused {
                    button {
                        style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:5px 10px;font-size:11.5px;",
                        onclick: move |_| {
                            k.send(Command::SetCronStatus {
                                id: cron_id,
                                status: CronStatus::Normal,
                            });
                        },
                        "▶ 恢复"
                    }
                } else {
                    button {
                        style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {theme::BORDER};border-radius:7px;padding:5px 10px;font-size:11.5px;",
                        onclick: move |_| {
                            k.send(Command::SetCronStatus {
                                id: cron_id,
                                status: CronStatus::Paused,
                            });
                        },
                        "⏸ 暂停"
                    }
                }
            }
        }
    }
}

/// 新建「定时建活」任务的表单——到点在绑定项目里建一张 Issue,可选作用
/// 阶段(默认跟项目当前阶段走)与指派对象(自由文本,到点按名匹配队友,
/// 匹配不上就如实建成未指派的活,不算失败)。
#[component]
fn CreateCronForm(projects: Vec<ProjectCardVm>, on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;

    let mut name = use_signal(String::new);
    // 0 = 全部项目 (None); 1..=projects.len() maps to projects[i-1].
    let mut project_choice = use_signal(|| 0usize);
    let mut schedule = use_signal(|| Cadence::Weekly);
    // None = 项目当前阶段;Some(stage) = 固定阶段。
    let mut stage_choice = use_signal(|| None::<StageKind>);
    let mut assignee = use_signal(String::new);

    let projects_for_save = projects.clone();
    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        let project_id = (project_choice() > 0)
            .then(|| projects_for_save.get(project_choice() - 1))
            .flatten()
            .map(|p| p.id);
        let a = assignee().trim().to_string();
        k.send(Command::CreateAutopilotTask {
            id: CronTaskId::new(),
            name: n,
            schedule: schedule(),
            project_id,
            stage: stage_choice(),
            assignee: (!a.is_empty()).then_some(a),
        });
        name.set(String::new());
        project_choice.set(0);
        schedule.set(Cadence::Weekly);
        stage_choice.set(None);
        assignee.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div {
                style: "display:grid;grid-template-columns:1.3fr 1fr;gap:12px;margin-bottom:10px;",
                div {
                    div { style: "{label}", "名称" }
                    input {
                        style: "{input}",
                        placeholder: "如 每周竞品扫描",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div {
                    div { style: "{label}", "绑定项目(需要绑定才能自动触发)" }
                    select {
                        style: "{input}",
                        onchange: move |e| {
                            if let Ok(i) = e.value().parse::<usize>() {
                                project_choice.set(i);
                            }
                        },
                        option { value: "0", "全部项目(不可自动触发)" }
                        for (i , p) in projects.iter().enumerate() {
                            option { key: "{i}", value: "{i + 1}", "{p.name}" }
                        }
                    }
                }
            }
            div {
                style: "display:grid;grid-template-columns:1fr 1.3fr;gap:12px;margin-bottom:10px;",
                div {
                    div { style: "{label}", "建到哪个阶段" }
                    select {
                        style: "{input}",
                        onchange: move |e| {
                            stage_choice.set(
                                e.value().parse::<u8>().ok().and_then(StageKind::from_index),
                            );
                        },
                        option { value: "", selected: true, "项目当前阶段(跟着交棒走)" }
                        for s in StageKind::ALL {
                            option { key: "{s.index()}", value: "{s.index()}", "{s.label()}" }
                        }
                    }
                }
                div {
                    div { style: "{label}", "指派给(队友名,可空;到点按名匹配,匹配不上就建成未指派)" }
                    input {
                        style: "{input}",
                        placeholder: "如 构建师",
                        value: "{assignee}",
                        oninput: move |e| assignee.set(e.value()),
                    }
                }
            }
            div { style: "{label}", "频率(真实调度——满足条件后无需点击,后台自动触发)" }
            select {
                style: "{input} width:auto;margin-bottom:6px;",
                onchange: move |e| {
                    schedule.set(match e.value().as_str() {
                        "realtime" => Cadence::RealTime,
                        "daily" => Cadence::Daily,
                        _ => Cadence::Weekly,
                    });
                },
                option { value: "daily", "每日(24 小时)" }
                option { value: "weekly", selected: true, "每周(7 天)" }
                option { value: "realtime", "实时(每次调度检查都触发)" }
            }
            p { style: "font-size:11px;color:{ink3};margin:0 0 12px;line-height:1.6;",
                "从未运行过的任务视为已到期,保存后的下一次后台检查(≤5 秒)就会真实建一张活。定时任务只建活,绝不自动跑活、绝不自动完成活。"
            }
            button {
                style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                onclick: save,
                "保存"
            }
        }
    }
}
