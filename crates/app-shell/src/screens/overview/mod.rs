//! 项目内 · 总览。**结构照 `hifi/index.html` 的 `renderOverview` 排**:七块
//! 竖排,名片与健康并排在第一块的左右两栏。
//!
//! 每一块的数字都是现算的:名片来自 `PROJECT.md` / `.bw/project.toml`,健康来
//! 自仓文件与 git 的三条判据,指标定义来自 `.bw/metrics.toml`、读数来自周计划
//! 文件的「本周指标读数」段,发版记录来自 `.bw/releases.md`。**没有读数就说
//! 无数据,不显示 0**。

use crate::bridge::{Bridge, Panel, PanelNav, Req};
use crate::chrome::light_dot;
use crate::vm::{MetricCardVm, ProjectVm};
use bw_v4::command::{Command, ProjectIntent};
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    // 名片是不是正在编辑,以及编辑中的三个字段。纯本机状态,没确认前不发命令。
    let nav = use_context::<PanelNav>();
    let editing = use_signal(|| false);
    let draft = use_signal(|| (String::new(), String::new(), String::new()));

    rsx! {
        section { class: "ov-stack", style: "max-width:1120px;",
            // 判据是**文件在不在**,不是周列表里有没有本周(本周永远在列表里)。
            // 文件还没有、但运作活①已经在路上时,不再给「开始本周」按钮 ——
            // 再点一次只会收到「终端还开着」的拒绝,给一条去会话屏的路才对。
            if !p.week_file_exists {
                if let Some(st) = p.ops1_status.clone() {
                    {running_banner(st, nav)}
                } else {
                    {start_banner(&p, &bridge, nav)}
                }
            }
            {card_and_health(&p, &bridge, editing, draft)}
            {north_star_block(&p)}
            // 本周计划进度紧跟北极星:第一眼要能看全「这项目是什么 · 顶层目标
            // 是什么 · 这周在往那个目标上推什么」。滞后/引领两层是展开看的细节,
            // 排在后面。
            {week_block(&p, nav)}
            {metric_row_block("滞后性指标", &p.metrics.lagging, p.metrics.note.as_deref())}
            {metric_row_block("引领性指标", &p.metrics.leading, None)}
            {repo_block(&p, &bridge)}
            {version_block(&p)}
        }
    }
}

fn start_banner(p: &ProjectVm, bridge: &Bridge, nav: PanelNav) -> Element {
    let b = bridge.clone();
    let (pid, week) = (p.id, p.current_week.clone());
    rsx! {
        div { class: "ov-banner",
            span { "本周({p.current_week})还没有周计划文件" }
            button {
                class: "btn btn-primary btn-sm",
                onclick: move |_| {
                    b.cmd(Command::StartWeekPlanning {
                        project_id: pid,
                        week: week.clone(),
                    });
                    // 剩下的在会话屏里发生(复盘上周 → 更新指标 → 聊出本周),
                    // 人点完这一下就该看到那场会话,不是留在总览上猜。
                    nav.go(Panel::Session);
                },
                "开始本周"
            }
        }
    }
}

/// 运作活①已经在路上:文件要等会话里的 MR 合入才落地,这期间横幅只指路,
/// 不再给「开始本周」按钮。
fn running_banner(status: String, nav: PanelNav) -> Element {
    rsx! {
        div { class: "ov-banner",
            span {
                "运作活①(更新指标 + 制定本周计划)已开工 · {status}。本周文件由那场会话产出,合入 MR 后这里才亮。"
            }
            button {
                class: "btn btn-sm",
                onclick: move |_| nav.go(Panel::Session),
                "去会话屏"
            }
        }
    }
}

// ── ① 名片 + 健康(并排两栏)────────────────────────────

fn card_and_health(
    p: &ProjectVm,
    bridge: &Bridge,
    mut editing: Signal<bool>,
    mut draft: Signal<(String, String, String)>,
) -> Element {
    let c = &p.card;
    let is_editing = *editing.read();
    let b_edit = bridge.clone();
    let b_pull = bridge.clone();
    let pid = p.id;
    let name = p.name.clone();

    let start_edit = {
        let (brief, benchmark, north) =
            (c.brief.clone(), c.benchmark.clone(), c.north_star.clone());
        move |_| {
            draft.set((brief.clone(), benchmark.clone(), north.clone()));
            editing.set(true);
        }
    };

    rsx! {
        div { class: "card ov-block ov-block1",
            div { class: "info",
                div { class: "charter-name", "{name}" }
                if is_editing {
                    div { class: "formrow",
                        label { class: "label", "想做什么" }
                        textarea {
                            class: "textarea",
                            value: "{draft.read().0}",
                            oninput: move |e| draft.write().0 = e.value(),
                        }
                    }
                    div { class: "formrow",
                        label { class: "label", "最像的对标" }
                        input {
                            class: "input",
                            value: "{draft.read().1}",
                            oninput: move |e| draft.write().1 = e.value(),
                        }
                    }
                    div { class: "formrow",
                        label { class: "label", "三个月长成什么样(北极星)" }
                        textarea {
                            class: "textarea",
                            value: "{draft.read().2}",
                            oninput: move |e| draft.write().2 = e.value(),
                        }
                    }
                    div { style: "display:flex;gap:8px;justify-content:flex-end;",
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| editing.set(false),
                            "取消"
                        }
                        button {
                            class: "btn btn-sm btn-primary",
                            onclick: move |_| {
                                let d = draft.read().clone();
                                b_edit.cmd(Command::EditProjectCard {
                                    project_id: pid,
                                    intent: ProjectIntent {
                                        name: name.clone(),
                                        brief: d.0,
                                        benchmark: d.1,
                                        north_star: d.2,
                                    },
                                });
                                editing.set(false);
                            },
                            "保存 · 建一张活提 MR"
                        }
                    }
                } else {
                    {charter_line("想做什么", &c.brief)}
                    {charter_line("对标", &c.benchmark)}
                    {charter_line("北极星", &c.north_star)}
                    {charter_line("项目群", &c.chat)}
                    div { class: "charter-meta",
                        span { class: "mono", "{c.remote}" }
                        span { "规范 v{c.standard_version}" }
                        span { style: "margin-left:auto;display:flex;gap:6px;",
                            // 人在网页上直接合了 MR 时,buddy 是不知道的 ——
                            // 工作区会一直停在旧提交,而界面照常显示旧内容。
                            // 这颗按钮是补那一下的唯一入口。
                            button {
                                class: "btn btn-sm",
                                title: "把工作区的主检出快进到远端最新(git pull --ff-only)",
                                onclick: move |_| b_pull.cmd(Command::PullWorkspace { project_id: pid }),
                                "↻ 拉到最新"
                            }
                            button { class: "btn btn-sm", onclick: start_edit, "编辑" }
                        }
                    }
                }
                {card_mr_banner(p, bridge)}
            }
            div { class: "health",
                div { class: "health-big",
                    {light_dot(p.health.signal, true)}
                    span { class: "health-word", "{crate::theme::signal_label(p.health.signal)}" }
                }
                if p.health.reasons.is_empty() {
                    div { class: "detail-empty", "还没有任何真实数据 —— 灰不是绿。" }
                } else {
                    div { class: "health-reasons",
                        for (i, (ok, text)) in p.health.reasons.iter().enumerate() {
                            div { key: "{i}", class: "health-reason",
                                span { class: "mark", {if *ok { "✓" } else { "○" }} }
                                span { "{text}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn charter_line(k: &str, v: &str) -> Element {
    rsx! {
        div { class: "charter-line",
            span { class: "k", "{k}" }
            "{v}"
        }
    }
}

/// 名片是仓文件,改它一律走分支 + MR。改完到合入之间这条横幅一直在。
fn card_mr_banner(p: &ProjectVm, bridge: &Bridge) -> Element {
    let Some(mr) = p.card_mr.as_ref() else {
        return rsx! {};
    };
    let b = bridge.clone();
    let id = mr.issue_id;
    let pr = if mr.pr_number > 0 {
        format!("MR !{}", mr.pr_number)
    } else {
        "还没提 MR".to_string()
    };
    rsx! {
        div { class: "mr-banner",
            span { "名片改动在 #{mr.number}「{mr.status}」· {pr}" }
            if mr.mergeable {
                if let Some(id) = id {
                    button {
                        class: "btn btn-sm btn-primary",
                        onclick: move |_| b.cmd(Command::MergeAndSettle { id }),
                        "合入并完成"
                    }
                }
            }
        }
    }
}

// ── ②③④ 指标 ────────────────────────────────────────

fn north_star_block(p: &ProjectVm) -> Element {
    rsx! {
        div { class: "card ov-block",
            div { class: "ov-block-title", "北极星" }
            match p.metrics.north_star.as_ref() {
                Some(m) => metric_card(m, true),
                None => rsx! { div { class: "detail-empty", "还没定北极星。" } },
            }
        }
    }
}

fn metric_row_block(title: &str, cards: &[MetricCardVm], note: Option<&str>) -> Element {
    rsx! {
        div { class: "card ov-block",
            div { class: "ov-block-title", "{title}" }
            if let Some(n) = note {
                div { class: "detail-empty", "{n}" }
            }
            if cards.is_empty() && note.is_none() {
                div { class: "detail-empty", "这一档还没有指标。" }
            }
            div { class: "ov-row",
                for m in cards.iter() {
                    {metric_card(m, false)}
                }
            }
        }
    }
}

/// 指标卡。**没读数走灰卡** —— 灰卡上写的是「无观测 · Unknown ≠ 绿」,
/// 不是一个 0。
fn metric_card(m: &MetricCardVm, big: bool) -> Element {
    let name_cls = if big { "mcard-name big" } else { "mcard-name" };
    let value_cls = if big {
        "mcard-value big"
    } else {
        "mcard-value"
    };
    let target = if m.target.is_empty() {
        "目标未设".to_string()
    } else {
        format!("目标 {}", m.target)
    };
    let Some(reading) = m.reading.as_ref() else {
        let cls = if big {
            "mcard gray north"
        } else {
            "mcard gray"
        };
        return rsx! {
            div { key: "{m.id}", class: "{cls}", title: "{m.def}",
                div { class: "mcard-head",
                    span { class: "{name_cls}", "{m.name}" }
                    if m.manual {
                        span { class: "chip badge-manual", "手填" }
                    }
                }
                div { class: "mcard-value-row",
                    span { class: "{value_cls}", style: "color:var(--ink-4);", "—" }
                    span { class: "mcard-target", "{target}" }
                }
                div { class: "mcard-sub", "无观测 · Unknown ≠ 绿(本周与上周的周计划文件里都没有这条读数)" }
            }
        };
    };
    let dot_color = if m.manual {
        "var(--amber)"
    } else {
        "var(--green)"
    };
    let cls = if big { "mcard north" } else { "mcard" };
    rsx! {
        div { key: "{m.id}", class: "{cls}", title: "{m.def}",
            div { class: "mcard-head",
                span { class: "dot", style: "background:{dot_color};" }
                span { class: "{name_cls}", "{m.name}" }
                if m.manual {
                    span { class: "chip badge-manual", "手填" }
                }
            }
            div { class: "mcard-value-row",
                span { class: "{value_cls}", "{reading}" }
                span { class: "mcard-target", "{target}" }
            }
            if !m.source.is_empty() || !m.collected_at.is_empty() {
                div { class: "mcard-sub", "来源 {m.source} · 采于 {m.collected_at}" }
            }
            if !m.driving.is_empty() {
                div { class: "mcard-driven",
                    for t in m.driving.iter() {
                        span { key: "{t}", class: "chip", "{t}" }
                    }
                }
            }
        }
    }
}

/// 近四周三条走势。**每个点都是现算的** —— 能采到今天的数就能采到过去任意
/// 一周的,所以第一次采集就有完整四周,不用先攒。
fn trend_row(s: &crate::vm::RepoStatsVm) -> Element {
    use crate::chrome::sparkline::{trend_chart, Series};
    if s.trend.is_empty() {
        return rsx! {};
    }
    // `2026-W34` → `W34`,四个点的小图放不下全称。
    let x = |p: &crate::vm::TrendPointVm| {
        p.week
            .split_once("-W")
            .map_or(p.week.clone(), |(_, w)| format!("W{w}"))
    };
    let commits = Series {
        label: "每周提交".into(),
        points: s
            .trend
            .iter()
            .map(|p| (x(p), Some(p.commits as f64)))
            .collect(),
        color: "var(--clay)",
    };
    let merges = Series {
        label: "每周合入".into(),
        points: s
            .trend
            .iter()
            .map(|p| (x(p), Some(p.merges as f64)))
            .collect(),
        color: "var(--green)",
    };
    let prs = Series {
        label: "每周合入的 PR(远端)".into(),
        points: s
            .trend
            .iter()
            .map(|p| (x(p), p.merged_prs.map(|n| n as f64)))
            .collect(),
        color: "var(--amber)",
    };
    rsx! {
        div { class: "trend-row",
            {trend_chart(&commits)}
            {trend_chart(&merges)}
            {trend_chart(&prs)}
        }
        if !s.trend_note.is_empty() {
            div { class: "cfg-readonly-note", "{s.trend_note}" }
        }
    }
}

// ── ⑤ 项目指标 · 代码仓级 ────────────────────────────

fn repo_block(p: &ProjectVm, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    rsx! {
        div { class: "card ov-block",
            div { class: "repo-metric-head",
                h3 { "项目指标 · 代码仓级" }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| b.send(Req::CollectRepoStats),
                    {if p.repo_stats.is_some() { "↻ 重新采集" } else { "↻ 立即采集" }}
                }
            }
            match p.repo_stats.as_ref() {
                None => rsx! {
                    div { class: "detail-empty",
                        "还没采过。采一次要在项目仓里跑好几条 git —— 所以不在每次\
                         打开时自动跑,点上面那颗按钮才采。"
                    }
                },
                Some(s) if !s.error.is_empty() => rsx! {
                    div { class: "detail-empty", style: "color:var(--alert-deep);", "{s.error}" }
                },
                Some(s) => rsx! {
                    // 静态那几个数压成一行小字 —— 它们是「此刻的存量」,没有走势
                    // 可言,不值得一人占一张卡片。
                    div { class: "repo-metric-line",
                        // 分隔点包在同一个带 key 的节点里 —— Dioxus 只认块里
                        // 第一个节点上的 key,拆成两个平级节点会被判 deprecated。
                        for (i, (v, k, _)) in s.items.iter().enumerate() {
                            span { key: "{k}",
                                if i > 0 {
                                    span { class: "sep", "·" }
                                }
                                "{k} "
                                b { class: "mono", "{v}" }
                            }
                        }
                        span { class: "sep", "·" }
                        span { class: "chip chip-gray", "全部来自 git" }
                    }
                    {trend_row(s)}
                },
            }
        }
    }
}

// ── ⑥ 本周计划进度 ──────────────────────────────────

fn week_block(p: &ProjectVm, nav: PanelNav) -> Element {
    let w = p.weeks.iter().find(|w| w.week == p.current_week);
    let goal = w
        .and_then(|w| w.goal.clone())
        .unwrap_or_else(|| "(本周尚无计划)".into());
    let c = &p.week_counts;
    rsx! {
        div { class: "card ov-block",
            div { class: "ov-block-title", "本周计划进度" }
            div { class: "week-goal",
                b { "{p.current_week}" }
                " · {goal}"
            }
            div { class: "week-progress",
                div { style: "width:{c.pct(c.done)}%;background:var(--green);" }
                div { style: "width:{c.pct(c.review)}%;background:var(--amber);" }
                div { style: "width:{c.pct(c.doing)}%;background:var(--clay);" }
                div { style: "width:{c.pct(c.blocked)}%;background:var(--red);" }
                div { style: "width:{c.pct(c.todo)}%;background:#E4DDC8;" }
            }
            div { class: "week-counts",
                span { "待办 " b { "{c.todo}" } }
                span { "进行中 " b { "{c.doing}" } }
                span { "评审中 " b { "{c.review}" } }
                span { "完成 " b { "{c.done}" } }
                if c.blocked > 0 {
                    span { style: "color:var(--alert-deep);", "阻塞 " b { "{c.blocked}" } }
                }
            }
            if !p.ops.is_empty() {
                div { class: "ops-chip-row",
                    for o in p.ops.iter() {
                        span { key: "{o.title}", class: "chip chip-outline", title: "{o.note}",
                            "{o.title} · {o.status}"
                        }
                    }
                }
            }
            button {
                class: "btn btn-sm",
                onclick: move |_| nav.go(Panel::Plan),
                "去计划 →"
            }
        }
    }
}

// ── ⑦ 在研版本与发版记录 ────────────────────────────

fn version_block(p: &ProjectVm) -> Element {
    rsx! {
        div { class: "card ov-block",
            div { class: "ov-block-title", "在研版本与发版记录" }
            div { class: "version-cols",
                div { class: "version-col",
                    div { class: "k", "在研版本" }
                    div { class: "v mono", "{p.card.current_version}" }
                }
                div { class: "version-col",
                    div { class: "k", "已发版次数" }
                    div { class: "v mono", "{p.releases.len()}" }
                }
            }
            if p.releases.is_empty() {
                div { class: "detail-empty",
                    "还没有发过版。.bw/releases.md 是这份记录的唯一正本,库里不存副本。"
                }
            }
            for r in p.releases.iter() {
                div { key: "{r.version}", class: "release-line",
                    "{r.version} · {r.released_at}"
                    if !r.note.is_empty() {
                        " · {r.note}"
                    }
                    if !r.included.is_empty() {
                        " · 含 {r.included}"
                    }
                    if r.origin != "human" {
                        span { class: "chip chip-gray", style: "margin-left:5px;", "{r.origin}" }
                    }
                }
            }
        }
    }
}
