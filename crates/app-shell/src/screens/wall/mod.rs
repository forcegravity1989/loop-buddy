//! 顶层 · 项目墙。不打开任何项目时看到的那一屏。
//!
//! 版式照 `docs/v4-prototype/hifi/index.html` 的 `renderWall`:本机环境条 →
//! 健康概览条 → 两列项目卡。
//!
//! 卡片上的灯来自库里的显示缓存(项目墙要在不打开项目时列出 N 个项目,不能每
//! 次启动扫 N 个仓)。**没数据的灯是灰的,不是绿的**;健康概览条里那个「无数据」
//! 计数就是专门为了让人看见有多少项目的灯还没被真实数据点亮。

use crate::bridge::{Bridge, GuideNav, Req, TopView};
use crate::chrome::light_dot;
use crate::vm::{ProjectCardVm, ToolProbeVm, Vm};
use bw_v4::Signal as HealthSignal;
use dioxus::prelude::*;

#[component]
pub fn View(vm: Vm, bridge: Bridge, go_top: EventHandler<TopView>) -> Element {
    let (vm, bridge) = (&vm, &bridge);
    let counts = Counts::of(&vm.projects);
    // 抽屉导航在这里取一次往下传。**不能在 env_item 里取** —— 那个函数在
    // `for` 循环里被调用,hook 进了循环就不再是每轮渲染同样的调用序列。
    let guide = use_context::<GuideNav>();
    // 哪张卡正在问「真移走?」。**放在这里而不是每张卡里** —— 卡片是在 `for`
    // 循环里画的,hook 进了循环就不再是每轮渲染同样的调用序列。
    let asking = use_signal(|| None::<bw_v4::model::ProjectId>);
    rsx! {
        section {
            div { class: "wall-topline",
                h1 { "项目墙" }
                button {
                    class: "btn btn-primary",
                    onclick: move |_| go_top(TopView::Onboard),
                    "+ 接入项目"
                }
            }

            {env_bar(&vm.env, bridge, guide)}

            div { class: "card healthbar",
                span { class: "bar-label", "健康概览" }
                {health_group(Some(HealthSignal::Green), "平稳", counts.green)}
                {health_group(Some(HealthSignal::Amber), "需要关注", counts.amber)}
                {health_group(Some(HealthSignal::Red), "阻塞", counts.red)}
                {health_group(None, "无数据", counts.unknown)}
                span { style: "margin-left:auto;font-size:12px;color:var(--ink-3);",
                    "共 {vm.projects.len()} 个项目"
                }
            }

            if vm.projects.is_empty() {
                div { class: "card",
                    style: "padding:40px;text-align:center;color:var(--ink-3);\
                            font-size:14px;line-height:2;",
                    "还没有接入任何项目。"
                    br {}
                    "点右上角「接入项目」,填四个字段,buddy 会把管理体系铺进这个仓。"
                }
            } else {
                div { class: "wall-grid",
                    for p in vm.projects.iter() {
                        {project_card(p, bridge, asking)}
                    }
                }
            }
        }
    }
}

/// 四个灯各有多少个项目。**灰的单独数一格** —— 「无数据」不是「一切正常」,
/// 混进绿里就把没接上数据的项目藏起来了。
struct Counts {
    green: usize,
    amber: usize,
    red: usize,
    unknown: usize,
}

impl Counts {
    fn of(projects: &[ProjectCardVm]) -> Counts {
        let n = |s: Option<HealthSignal>| projects.iter().filter(|p| p.signal == s).count();
        Counts {
            green: n(Some(HealthSignal::Green)),
            amber: n(Some(HealthSignal::Amber)),
            red: n(Some(HealthSignal::Red)),
            // 库里存的是 `Unknown`,还没算过的是 `None` —— 两种都是「没数据」。
            unknown: n(Some(HealthSignal::Unknown)) + n(None),
        }
    }
}

fn health_group(s: Option<HealthSignal>, label: &str, n: usize) -> Element {
    rsx! {
        span { class: "health-group", {light_dot(s, false)} "{label} {n}" }
    }
}

fn env_bar(env: &[ToolProbeVm], bridge: &Bridge, guide: GuideNav) -> Element {
    let b = bridge.clone();
    rsx! {
        div { class: "card envbar",
            span { class: "bar-label", "本机环境" }
            for t in env.iter() {
                {env_item(t, guide)}
            }
            span { style: "margin-left:auto;" }
            button {
                class: "btn btn-sm",
                // 探活跟着整份 ViewModel 一起重算,所以「测一下」就是让内核再拼
                // 一份 —— 不另开一条只探环境的路。
                onclick: move |_| b.send(Req::Refresh),
                "测一下"
            }
        }
    }
}

fn env_item(t: &ToolProbeVm, guide: GuideNav) -> Element {
    let (text, fail) = match t.ok {
        Some(true) => ("可用", false),
        Some(false) => ("没装", true),
        None => ("还没接", false),
    };
    rsx! {
        span {
            key: "{t.name}",
            class: if fail { "env-item fail" } else { "env-item" },
            title: "{t.detail}",
            {light_dot(dot_of(t.ok), false)}
            "{t.label} · {text}"
            if fail {
                span {
                    class: "env-help",
                    onclick: move |_| guide.open("env"),
                    "怎么处理 →"
                }
            }
        }
    }
}

/// 探活三态借用健康三色:探到=绿、没装=红、**还没接实现=灰**。灰这一档不能
/// 画成红 —— 「我们还没写这个探活」和「你本机没装」是两件事。
fn dot_of(ok: Option<bool>) -> Option<HealthSignal> {
    match ok {
        Some(true) => Some(HealthSignal::Green),
        Some(false) => Some(HealthSignal::Red),
        None => None,
    }
}

/// 一张项目卡。右上角那个 × 是**从工作台移走**,不是删仓 —— 点一下先问一句,
/// 再点「移走」才真发命令。高保真上那个 × 当初没做,理由是「内核没有删除项目
/// 这条命令」;试点第一天用户当场问它去哪了,而且中途失败留下的半成品项目正是
/// 靠它才收得掉。命令补上了,× 也就回来了。
fn project_card(
    p: &ProjectCardVm,
    bridge: &Bridge,
    mut asking: Signal<Option<bw_v4::model::ProjectId>>,
) -> Element {
    let b = bridge.clone();
    let b_del = bridge.clone();
    let id = p.id;
    let confirming = *asking.read() == Some(id);
    let pct = (p.week_done * 100).checked_div(p.week_total).unwrap_or(0);
    let goal = if p.week_goal.trim().is_empty() {
        "本周目标:还没定".to_string()
    } else {
        let g = p.week_goal.trim();
        let short: String = g.chars().take(30).collect();
        format!(
            "本周目标:{short}{}",
            if g.chars().count() > 30 { "…" } else { "" }
        )
    };
    let delivery = if p.last_delivery.trim().is_empty() {
        "上次交付:仓里还没有发版记录".to_string()
    } else {
        format!("上次交付 {}", p.last_delivery)
    };
    rsx! {
        div {
            key: "{p.slug}",
            class: "pcard",
            onclick: move |_| b.send(Req::Open(Some(id))),
            div { class: "pcard-top",
                if p.version.trim().is_empty() {
                    span { class: "chip chip-outline mono", title: "仓里的 .bw/project.toml 没写在研版本", "版本未定" }
                } else {
                    span { class: "chip chip-outline mono", "{p.version}" }
                }
                span { class: "chip", "{p.week}" }
                if p.unread > 0 {
                    span { class: "chip chip-clay", title: "评审中或阻塞、你还没在通知屏看过的活", "⚑ {p.unread}" }
                }
                span { style: "margin-left:auto;" }
                {light_dot(p.signal, true)}
                span {
                    class: "pcard-x",
                    title: "从工作台移走(不删仓)",
                    onclick: move |e: MouseEvent| {
                        // 卡片整张是「打开这个项目」,× 不能顺带把项目开了。
                        e.stop_propagation();
                        asking.set(if confirming { None } else { Some(id) });
                    },
                    "✕"
                }
            }
            div { class: "pcard-name", "{p.name}" }
            div { class: "pcard-oneliner",
                if p.brief.trim().is_empty() { "(还没填「想做什么」)" } else { "{p.brief}" }
            }
            div { class: "pcard-meta",
                span { "{goal}" }
                span { "{delivery}" }
            }
            if confirming {
                div {
                    class: "pcard-confirm",
                    onclick: move |e: MouseEvent| e.stop_propagation(),
                    div { style: "margin-bottom:6px;",
                        "把「" strong { "{p.name}" } "」从工作台移走?"
                        br {}
                        "库里这个项目和它所有活的账会一起没,"
                        strong { "仓不动" }
                        " —— 硬盘上的代码一个字节都不碰。"
                    }
                    div { style: "display:flex;gap:8px;",
                        button {
                            class: "btn btn-sm",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                asking.set(None);
                            },
                            "取消"
                        }
                        button {
                            class: "btn btn-sm btn-danger",
                            onclick: move |e: MouseEvent| {
                                e.stop_propagation();
                                asking.set(None);
                                b_del.cmd(bw_v4::command::Command::RemoveProject { project_id: id });
                            },
                            "移走"
                        }
                    }
                }
            }
            div { class: "pcard-bar",
                div { class: "pcard-bar-fill", style: "width:{pct}%" }
            }
            div { style: "margin-top:7px;font-size:11px;color:var(--ink-3);",
                if p.week_total == 0 {
                    "本周还没排活"
                } else {
                    "本周 {p.week_done}/{p.week_total} 完成 · {p.remote}"
                }
            }
        }
    }
}
