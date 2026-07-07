//! `view=wizard` — the 7-step creation flow (step 0 引子 + 01..07).
//!
//! Unlike the prototype (whose "created" project was simulated — even the
//! project name was a hard-coded constant), every step here collects REAL
//! input and lands it through Commands:
//!
//!   step 0  name/kind/desc            → CreateProject
//!   step 1  对标竞品                   → UpdateBrief
//!   step 2  机会缺口                   → UpdateBrief
//!   step 3  北极星 + 口径              → UpdateNorthStar
//!   step 4  引领指标(+当前值→观测)      → UpsertManualMetric×n
//!   step 5  滞后指标(+当前值→观测)      → UpsertManualMetric×n
//!   step 6  原型即规格(方法论)          → —
//!   step 7  周计划 + 本周自评           → UpdateWeekPlan×n → CompleteWizard
//!                                        → AnnotateWeeklyReview
//!
//! 自评映射保持诚实:绿=不覆写(派生说了算);黄/红=更悲观的 override,连同理由
//! 入 weekly_review 审计表 —— 信号本身永远 derive。

use crate::kernel::{Kernel, WizardVm};
use crate::theme;
use bw_app::Command;
use bw_core::model::{Signal, StageKind};
use bw_core::MetricId;
use bw_store::MetricRole;
use dioxus::prelude::*;
use ui::vm::{step_state, MetricVm, StepState, WeekPlanRowVm, WIZARD_STEPS};

/// One editable metric line (steps 4/5).
#[derive(Clone, PartialEq)]
pub struct Draft {
    pub id: Option<MetricId>,
    pub name: String,
    pub def: String,
    pub cur: String,
    pub target: String,
}

impl Draft {
    fn empty() -> Self {
        Draft {
            id: None,
            name: String::new(),
            def: String::new(),
            cur: String::new(),
            target: String::new(),
        }
    }
    fn from_vm(m: &MetricVm) -> Self {
        Draft {
            id: Some(m.id),
            name: m.name.clone(),
            def: m.def.clone(),
            cur: m.value_raw.clone(),
            target: m.target_raw.clone(),
        }
    }
}

#[component]
pub fn Wizard(
    vm: Option<WizardVm>,
    on_start: EventHandler<(String, String, String)>,
    on_cancel: EventHandler<()>,
) -> Element {
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let pre_create = vm.is_none();
    let created = vm.is_some();
    let step = vm.as_ref().map(|w| w.step).unwrap_or(0);
    let name = vm
        .as_ref()
        .map(|w| format!("{} · {}", w.name, w.kind))
        .unwrap_or_default();
    let body = match (step, vm) {
        (0, w) => rsx! { Step0 { created: w.is_some(), on_start } },
        (1, Some(w)) => rsx! { Step1 { w } },
        (2, Some(w)) => rsx! { Step2 { w } },
        (3, Some(w)) => rsx! { Step3 { w } },
        (4, Some(w)) => rsx! { Step4 { w } },
        (5, Some(w)) => rsx! { Step5 { w } },
        (6, Some(_)) => rsx! { Step6 {} },
        (7, Some(w)) => rsx! { Step7 { w } },
        _ => rsx! { div { "…" } },
    };
    rsx! {
        div {
            style: "max-width:1020px;margin:0 auto;padding:34px 40px 80px;",
            div {
                style: "display:flex;align-items:baseline;justify-content:space-between;margin-bottom:20px;",
                div {
                    span { style: "font-family:{serif};font-size:17px;font-weight:600;", "新建产品 · 创建引导" }
                    if !name.is_empty() {
                        span { style: "color:{ink2};font-size:13px;margin-left:12px;", "{name}" }
                    }
                }
                if pre_create {
                    button {
                        style: "background:transparent;border:none;color:{ink2};cursor:pointer;font-size:13px;",
                        onclick: move |_| on_cancel.call(()),
                        "← 返回项目墙"
                    }
                }
            }
            StepBar { current: step, created }
            {body}
        }
    }
}

#[component]
fn StepBar(current: u8, created: bool) -> Element {
    let k = use_context::<Kernel>();
    let clay = theme::CLAY;
    let ink3 = theme::INK_3;
    let green = ui::signal_color(Signal::Green);
    rsx! {
        div {
            style: "display:flex;gap:6px;flex-wrap:wrap;margin-bottom:30px;",
            for (i, title) in WIZARD_STEPS.iter().enumerate() {
                {
                    let i = i as u8;
                    let st = step_state(i, current);
                    let (bg, fg, bd) = match st {
                        StepState::Done => (green, "#FFF", green),
                        StepState::Current => (clay, "#FFF", clay),
                        StepState::Todo => ("transparent", ink3, "#D8D1C2"),
                    };
                    let kk = k.clone();
                    rsx! {
                        button {
                            key: "{i}",
                            onclick: move |_| if created { kk.send(Command::SetWizardStep { step: i }) },
                            style: "display:flex;align-items:center;gap:6px;background:transparent;border:none;cursor:pointer;padding:4px 6px;",
                            span {
                                style: "width:22px;height:22px;border-radius:50%;background:{bg};color:{fg};border:1.5px solid {bd};display:inline-flex;align-items:center;justify-content:center;font-size:11px;",
                                "{i}"
                            }
                            span { style: "font-size:12px;color:{ink3};", "{title}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Footer(step: u8, next_label: &'static str, on_next: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let primary = theme::btn_primary();
    let ghost = theme::btn_ghost();
    rsx! {
        div {
            style: "display:flex;justify-content:space-between;margin-top:28px;",
            if step > 1 {
                button {
                    style: "{ghost}",
                    onclick: move |_| k.send(Command::SetWizardStep { step: step - 1 }),
                    "← 上一步"
                }
            } else {
                span {}
            }
            button { style: "{primary}", onclick: move |_| on_next.call(()), "{next_label}" }
        }
    }
}

fn section(title: &str, body: Element) -> Element {
    let card = theme::card();
    let serif = theme::SERIF;
    rsx! {
        div {
            style: "{card} padding:22px 24px;margin-bottom:16px;",
            div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:12px;", "{title}" }
            {body}
        }
    }
}

// ───────────────────────── step 0 · 引子 ─────────────────────────

const KINDS: [&str; 5] = [
    "看板 / 网页应用",
    "对话应用",
    "Design / 无限画布",
    "数据 / API 服务",
    "其他",
];

const CONTROL_POINTS: [(&str, &str); 4] = [
    ("01 知道在对标谁", "竞品洞察持续在线,差距和机会缺口一目了然"),
    ("02 每周在正常演进", "健康信号由指标派生,绿点从不手设"),
    ("03 让 agent loop 干活", "工作流交给执行引擎,人守住 GATE"),
    ("04 目标清晰且难造假", "北极星 + 引领/滞后指标,值只来自观测"),
];

#[component]
fn Step0(created: bool, on_start: EventHandler<(String, String, String)>) -> Element {
    let k = use_context::<Kernel>();
    let mut name = use_signal(String::new);
    let mut kind = use_signal(|| KINDS[0].to_string());
    let mut desc = use_signal(String::new);

    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let card = theme::card();
    let card_alt = theme::CARD_ALT;
    let input = theme::input();
    let label = theme::label();
    let primary = theme::btn_primary();
    let clay = theme::CLAY;
    let can_start = !name().trim().is_empty();

    rsx! {
        h2 { style: "font-family:{serif};font-size:26px;font-weight:600;margin:0 0 8px;", "从 0 到一个会自己运转的项目" }
        p { style: "color:{ink2};font-size:14px;line-height:1.8;margin:0 0 22px;max-width:720px;",
            "传统项目管理约 10 套流程、5 类角色;AI 时代的 Builders 模式把角色收敛为一个 Builder,流程收敛为七个控制点。接下来的七步,把它们逐一立起来。"
        }
        div {
            style: "display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px;margin-bottom:22px;",
            for (t, d) in CONTROL_POINTS {
                div {
                    style: "{card} padding:16px 18px;",
                    div { style: "font-weight:600;font-size:14px;margin-bottom:6px;color:{clay};", "{t}" }
                    div { style: "font-size:13px;color:{ink2};line-height:1.6;", "{d}" }
                }
            }
        }
        if created {
            div {
                style: "display:flex;justify-content:flex-end;",
                button {
                    style: "{primary}",
                    onclick: move |_| k.send(Command::SetWizardStep { step: 1 }),
                    "继续 →"
                }
            }
        } else {
            div {
                style: "background:{card_alt};border:1px solid #DBD4C5;border-radius:10px;padding:20px 22px;",
                div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:14px;", "项目基本盘" }
                div {
                    style: "display:grid;grid-template-columns:2fr 1fr;gap:14px;margin-bottom:12px;",
                    div {
                        label { style: "{label}", "项目名称 *" }
                        input {
                            style: "{input}",
                            placeholder: "例:增长实验看板",
                            value: "{name}",
                            oninput: move |e| name.set(e.value()),
                        }
                    }
                    div {
                        label { style: "{label}", "项目类型" }
                        select {
                            style: "{input}",
                            value: "{kind}",
                            onchange: move |e| kind.set(e.value()),
                            for kd in KINDS {
                                option { value: "{kd}", "{kd}" }
                            }
                        }
                    }
                }
                div {
                    style: "margin-bottom:16px;",
                    label { style: "{label}", "一句话描述" }
                    input {
                        style: "{input}",
                        placeholder: "这个产品为谁解决什么问题",
                        value: "{desc}",
                        oninput: move |e| desc.set(e.value()),
                    }
                }
                div {
                    style: "display:flex;justify-content:flex-end;",
                    {
                        let opacity = if can_start { "1" } else { ".45" };
                        rsx! {
                            button {
                                style: "{primary} opacity:{opacity};",
                                disabled: !can_start,
                                onclick: move |_| {
                                    if can_start {
                                        on_start.call((
                                            name().trim().to_string(),
                                            kind(),
                                            desc().trim().to_string(),
                                        ));
                                    }
                                },
                                "开始创建项目体系 →"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ───────────────────────── step 1/2 · 竞品 ─────────────────────────

#[component]
fn Step1(w: WizardVm) -> Element {
    let k = use_context::<Kernel>();
    let mut benchmark = use_signal(|| w.benchmark.clone());
    let opportunity = w.opportunity.clone();
    let ink2 = theme::INK_2;
    let input = theme::input();
    let label = theme::label();
    let red = ui::signal_color(Signal::Red);
    let flow = section(
        "方法:界定 → 采集 → 结构化 → 分析 → GATE → 洞察",
        rsx! {
            p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0;",
                "竞品洞察是第一个控制点。机器负责采集与结构化;「发现 → 洞察」这一步由人把关 —— GATE 之后的判断才允许进入决策。"
            }
        },
    );
    rsx! {
        {flow}
        div {
            style: "border:1.5px solid {red};border-radius:10px;padding:14px 18px;margin-bottom:16px;background:#FBF3F0;",
            span { style: "color:{red};font-weight:600;font-size:13px;", "GATE · 发现→洞察 由人把关" }
            span { style: "color:{ink2};font-size:13px;margin-left:10px;", "工作流产出发现;洞察必须由你确认后沉淀。" }
        }
        {section("你的对标竞品(每行一个)", rsx! {
            textarea {
                style: "{input} min-height:120px;",
                placeholder: "例:\nLinear\nNotion Projects\nHeight",
                value: "{benchmark}",
                oninput: move |e| benchmark.set(e.value()),
            }
            span { style: "{label} margin-top:6px;", "运营视图的竞品环节将以此为观察名单;后续每轮竞品洞察工作流都会围绕它展开。" }
        })}
        Footer {
            step: 1,
            next_label: "下一步 →",
            on_next: move |_| {
                k.send(Command::UpdateBrief {
                    benchmark: benchmark().trim().to_string(),
                    opportunity: opportunity.clone(),
                });
                k.send(Command::SetWizardStep { step: 2 });
            },
        }
    }
}

#[component]
fn Step2(w: WizardVm) -> Element {
    let k = use_context::<Kernel>();
    let mut opportunity = use_signal(|| w.opportunity.clone());
    let benchmark = w.benchmark.clone();
    let ink2 = theme::INK_2;
    let input = theme::input();
    rsx! {
        {section("差距分析 → 机会缺口", rsx! {
            p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0 0 12px;",
                "对照上一步的对标名单,写下你判断的机会缺口:竞品覆盖不了、而你能守住的那条缝。这是后面所有指标的\"为什么\"。"
            }
            textarea {
                style: "{input} min-height:140px;",
                placeholder: "例:现有工具都在服务多角色协作,单人 Builder 的运营闭环(指标→信号→动作)没有人做透…",
                value: "{opportunity}",
                oninput: move |e| opportunity.set(e.value()),
            }
        })}
        Footer {
            step: 2,
            next_label: "下一步 →",
            on_next: move |_| {
                k.send(Command::UpdateBrief {
                    benchmark: benchmark.clone(),
                    opportunity: opportunity().trim().to_string(),
                });
                k.send(Command::SetWizardStep { step: 3 });
            },
        }
    }
}

// ───────────────────────── step 3 · 北极星 ─────────────────────────

#[component]
fn Step3(w: WizardVm) -> Element {
    let k = use_context::<Kernel>();
    let mut ns = use_signal(|| w.north_star.clone());
    let mut ns_def = use_signal(|| w.ns_def.clone());
    let ink2 = theme::INK_2;
    let input = theme::input();
    let label = theme::label();
    rsx! {
        {section("北极星指标", rsx! {
            p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0 0 12px;",
                "一个指标,回答\"这个产品是否真的在被需要\"。要求:清晰、可计算、难造假。"
            }
            label { style: "{label}", "指标" }
            textarea {
                style: "{input} min-height:64px;margin-bottom:12px;",
                placeholder: "例:每周留存对话用户数",
                value: "{ns}",
                oninput: move |e| ns.set(e.value()),
            }
            label { style: "{label}", "计算口径(怎么算,数据从哪来)" }
            textarea {
                style: "{input} min-height:88px;",
                placeholder: "例:7 日窗口内发生 ≥2 次有效对话的独立用户数;数据源:网关日志。",
                value: "{ns_def}",
                oninput: move |e| ns_def.set(e.value()),
            }
        })}
        Footer {
            step: 3,
            next_label: "下一步 →",
            on_next: move |_| {
                k.send(Command::UpdateNorthStar {
                    value: ns().trim().to_string(),
                    def: ns_def().trim().to_string(),
                });
                k.send(Command::SetWizardStep { step: 4 });
            },
        }
    }
}

// ───────────────────── step 4/5 · 指标编辑器 ─────────────────────

#[component]
fn MetricEditor(
    initial: Vec<Draft>,
    name_ph: &'static str,
    def_ph: &'static str,
    cur_ph: &'static str,
    target_ph: &'static str,
    on_confirm: EventHandler<Vec<Draft>>,
    step: u8,
) -> Element {
    let mut rows = use_signal(|| {
        if initial.is_empty() {
            vec![Draft::empty()]
        } else {
            initial.clone()
        }
    });
    let input = theme::input();
    let label = theme::label();
    let ghost = theme::btn_ghost();
    let ink3 = theme::INK_3;
    let card_alt = theme::CARD_ALT;
    let snapshot = rows();
    rsx! {
        for (i, row) in snapshot.into_iter().enumerate() {
            div {
                key: "{i}",
                style: "background:{card_alt};border:1px solid #DBD4C5;border-radius:10px;padding:16px 18px;margin-bottom:12px;",
                div {
                    style: "display:grid;grid-template-columns:1.4fr 2fr;gap:12px;margin-bottom:10px;",
                    div {
                        label { style: "{label}", "指标名 *" }
                        input {
                            style: "{input}",
                            placeholder: "{name_ph}",
                            value: "{row.name}",
                            oninput: move |e| rows.write()[i].name = e.value(),
                        }
                    }
                    div {
                        label { style: "{label}", "口径" }
                        input {
                            style: "{input}",
                            placeholder: "{def_ph}",
                            value: "{row.def}",
                            oninput: move |e| rows.write()[i].def = e.value(),
                        }
                    }
                }
                div {
                    style: "display:grid;grid-template-columns:1fr 1fr;gap:12px;",
                    div {
                        label { style: "{label}", "当前值(手填 · 未接入度量源)" }
                        input {
                            style: "{input}",
                            placeholder: "{cur_ph}",
                            value: "{row.cur}",
                            oninput: move |e| rows.write()[i].cur = e.value(),
                        }
                    }
                    div {
                        label { style: "{label}", "目标(≥5 · ≤24h · 60% · 7/7 · 清零 · ↑)" }
                        input {
                            style: "{input}",
                            placeholder: "{target_ph}",
                            value: "{row.target}",
                            oninput: move |e| rows.write()[i].target = e.value(),
                        }
                    }
                }
            }
        }
        div {
            style: "display:flex;align-items:center;gap:12px;margin-bottom:4px;",
            button {
                style: "{ghost}",
                onclick: move |_| rows.write().push(Draft::empty()),
                "+ 添加一条"
            }
            span { style: "color:{ink3};font-size:12px;", "值会作为一条 Manual 观测入库;信号由 目标×值 派生,不能手设。" }
        }
        Footer {
            step,
            next_label: "下一步 →",
            on_next: move |_| on_confirm.call(rows()),
        }
    }
}

fn dispatch_drafts(k: &Kernel, drafts: &[Draft], role: MetricRole, stage: StageKind, next: u8) {
    for d in drafts {
        if d.name.trim().is_empty() {
            continue;
        }
        k.send(Command::UpsertManualMetric {
            id: d.id.unwrap_or_default(),
            name: d.name.trim().to_string(),
            def: d.def.trim().to_string(),
            role,
            stage_kind: Some(stage),
            target: d.target.trim().to_string(),
            amber: Default::default(),
            value: d.cur.trim().to_string(),
        });
    }
    k.send(Command::SetWizardStep { step: next });
}

#[component]
fn Step4(w: WizardVm) -> Element {
    let k = use_context::<Kernel>();
    let ink2 = theme::INK_2;
    let initial: Vec<Draft> = w.leading.iter().map(Draft::from_vm).collect();
    let intro = section(
        "引领指标(可控 · 难造假)",
        rsx! {
            p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0;",
                "引领指标是你本周就能推动的量,它们向前预示北极星。每条给出口径、当前值与目标;目标写成可判定的表达式,健康信号将由它派生。"
            }
        },
    );
    rsx! {
        {intro}
        MetricEditor {
            initial,
            name_ph: "例:每周有效对话数",
            def_ph: "例:7日窗口内 ≥2 轮的对话数",
            cur_ph: "例:8",
            target_ph: "例:≥5",
            step: 4,
            on_confirm: move |drafts: Vec<Draft>| {
                dispatch_drafts(&k, &drafts, MetricRole::Leading, StageKind::Leading, 5);
            },
        }
    }
}

#[component]
fn Step5(w: WizardVm) -> Element {
    let k = use_context::<Kernel>();
    let ink2 = theme::INK_2;
    let initial: Vec<Draft> = w.lagging.iter().map(Draft::from_vm).collect();
    let intro = section(
        "滞后指标(最终在意的结果)",
        rsx! {
            p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0;",
                "滞后指标衡量结果本身 —— 它们动得慢,但作数。与引领指标一起构成\"本周动作 → 长期结果\"的证据链。"
            }
        },
    );
    rsx! {
        {intro}
        MetricEditor {
            initial,
            name_ph: "例:周留存率",
            def_ph: "例:上周活跃且本周仍活跃的用户占比",
            cur_ph: "例:41%",
            target_ph: "例:≥45%",
            step: 5,
            on_confirm: move |drafts: Vec<Draft>| {
                dispatch_drafts(&k, &drafts, MetricRole::Lagging, StageKind::Lagging, 6);
            },
        }
    }
}

// ───────────────────────── step 6 · 原型即规格 ─────────────────────────

#[component]
fn Step6() -> Element {
    let k = use_context::<Kernel>();
    let ink2 = theme::INK_2;
    rsx! {
        {section("原型创建 · 原型即规格", rsx! {
            p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0 0 8px;",
                "在 Builders 模式里,原型不是示意图,而是规格本身:可点击、可反驳、可直接交给工作流迭代。文字描述会漂移,原型不会。"
            }
            p { style: "color:{ink2};font-size:13px;line-height:1.8;margin:0;",
                "项目进入运营后,「原型创建」环节将承载原型产物与迭代会话;它的健康同样由该环节指标派生。"
            }
        })}
        Footer {
            step: 6,
            next_label: "下一步 →",
            on_next: move |_| k.send(Command::SetWizardStep { step: 7 }),
        }
    }
}

// ───────────────────────── step 7 · 周计划 + 自评 ─────────────────────────

#[derive(Clone, PartialEq)]
struct PlanDraft {
    metric: MetricId,
    name: String,
    last_target: String,
    current: String,
    orig_target: String,
    target: String,
    driver: String,
    orig_driver: String,
}

impl PlanDraft {
    fn from_row(r: &WeekPlanRowVm, orig: (&str, &str)) -> Self {
        PlanDraft {
            metric: r.metric,
            name: r.name.clone(),
            last_target: r.last_target.clone(),
            current: r.current.clone(),
            orig_target: orig.0.to_string(),
            target: r.target.clone(),
            driver: r.driver.clone(),
            orig_driver: orig.1.to_string(),
        }
    }
}

#[component]
fn Step7(w: WizardVm) -> Element {
    let k = use_context::<Kernel>();
    let mut rows = use_signal(|| {
        w.week_plan
            .iter()
            .map(|r| PlanDraft::from_row(r, (&r.target, &r.driver)))
            .collect::<Vec<_>>()
    });
    let mut pick = use_signal(|| Signal::Green);

    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let input = theme::input();
    let card = theme::card();
    let serif = theme::SERIF;
    let primary = theme::btn_primary();
    let mono = theme::MONO;
    let ghost = theme::btn_ghost();
    let n = rows().len();

    let finish = {
        let k = k.clone();
        move |_| {
            for r in rows() {
                let changed = r.target != r.orig_target || r.driver != r.orig_driver;
                if changed {
                    let last = if r.target != r.orig_target {
                        r.orig_target.clone()
                    } else {
                        r.last_target.clone()
                    };
                    k.send(Command::UpdateWeekPlan {
                        metric: r.metric,
                        new_target: r.target.trim().to_string(),
                        last_target: if last == "—" { String::new() } else { last },
                        driver: r.driver.trim().to_string(),
                    });
                }
            }
            // Materialize the seven stages + derive every signal…
            k.send(Command::CompleteWizard);
            // …then record the self-assessment. Green = no override (derive
            // rules); amber/red = a more-pessimistic override, audited.
            let (ov, label) = match pick() {
                Signal::Amber => (Some(Signal::Amber), "需要关注"),
                Signal::Red => (Some(Signal::Red), "阻塞"),
                _ => (None, "正常演进"),
            };
            k.send(Command::AnnotateWeeklyReview {
                human_override: ov,
                reason: format!("向导 · 本周自评:{label}"),
            });
        }
    };

    rsx! {
        {section("本周计划(引领指标 × 目标 × 抓手)", rsx! {
            if n == 0 {
                p { style: "color:{ink2};font-size:13px;margin:0;", "第 4 步还没有录入引领指标 —— 返回补上,周计划才有对象。" }
            } else {
                div {
                    style: "display:grid;grid-template-columns:1.3fr .7fr .7fr .9fr 1.6fr;gap:8px;font-size:12px;color:{ink3};margin-bottom:6px;",
                    span { "指标" } span { "上周目标" } span { "当前值" } span { "本周目标" } span { "依据 / 本周抓手" }
                }
                for (i, row) in rows().into_iter().enumerate() {
                    div {
                        key: "{i}",
                        style: "display:grid;grid-template-columns:1.3fr .7fr .7fr .9fr 1.6fr;gap:8px;align-items:center;margin-bottom:8px;",
                        span { style: "font-size:13px;", "{row.name}" }
                        span { style: "font-family:{mono};font-size:12px;color:{ink2};", "{row.last_target}" }
                        span { style: "font-family:{mono};font-size:12px;", "{row.current}" }
                        input {
                            style: "{input} padding:6px 8px;font-family:{mono};font-size:12px;",
                            value: "{row.target}",
                            oninput: move |e| rows.write()[i].target = e.value(),
                        }
                        input {
                            style: "{input} padding:6px 8px;font-size:12px;",
                            placeholder: "本周靠什么推动它",
                            value: "{row.driver}",
                            oninput: move |e| rows.write()[i].driver = e.value(),
                        }
                    }
                }
            }
        })}
        div {
            style: "{card} padding:22px 24px;margin-bottom:16px;",
            div { style: "font-family:{serif};font-size:16px;font-weight:600;margin-bottom:6px;", "本周健康自评" }
            p { style: "color:{ink3};font-size:12px;margin:0 0 14px;",
                "信号本身永远由指标派生。这里是你的自评:选\"正常演进\"不会覆写派生结果;选\"需要关注/阻塞\"会作为更悲观的 override 连同理由记入周复盘,可审计。"
            }
            div {
                style: "display:flex;gap:12px;",
                for s in [Signal::Green, Signal::Amber, Signal::Red] {
                    {
                        let active = pick() == s;
                        let color = ui::signal_color(s);
                        let label = ui::vm::signal_label(s);
                        let bd = if active { color } else { "#DBD4C5" };
                        let dot = theme::dot(color, 10);
                        rsx! {
                            button {
                                key: "{label}",
                                onclick: move |_| pick.set(s),
                                style: "flex:1;cursor:pointer;background:#FFFDF8;border:1.6px solid {bd};border-radius:10px;padding:12px;display:flex;align-items:center;gap:9px;justify-content:center;",
                                span { style: "{dot}" }
                                span { style: "font-size:13px;", "{label}" }
                            }
                        }
                    }
                }
            }
        }
        div {
            style: "display:flex;justify-content:space-between;",
            button {
                style: "{ghost}",
                onclick: {
                    let k = k.clone();
                    move |_| k.send(Command::SetWizardStep { step: 6 })
                },
                "← 上一步"
            }
            button { style: "{primary}", onclick: finish, "完成,生成项目看板 →" }
        }
    }
}
