//! `Hub::Skill` — the skill library: a flat card grid, matching the
//! prototype's own SkillHub. Real store-backed CRUD, and now (like
//! WorkflowHub) a real detail/edit panel: click a card to expand it in
//! place — full description, a real "被这些工作流使用" reverse lookup
//! computed from `hub.workflow_details` (a `SkillRef.name` match, same
//! by-name convention as everywhere else this hub cross-references
//! skills/agents), and an edit form dispatching `Command::UpdateSkill` —
//! content only, `maturity`/`uses` stay untouched.

use crate::kernel::{HubVm, Kernel};
use crate::theme;
use bw_app::Command;
use bw_core::model::LibSource;
use bw_core::SkillId;
use dioxus::prelude::*;
use ui::vm::{ProjectCardVm, SkillCardVm};

#[component]
pub fn SkillHub(hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.skills.len();

    let mut creating = use_signal(|| false);
    let mut expanded = use_signal(|| None::<SkillId>);
    let mut editing = use_signal(|| None::<SkillId>);

    rsx! {
        div {
            style: "display:flex;flex-direction:column;height:100%;background:{paper};padding:22px 26px;overflow-y:auto;",
            div {
                style: "display:flex;align-items:baseline;gap:12px;margin-bottom:4px;",
                span { style: "font-family:{mono};font-size:11px;letter-spacing:.06em;color:{ink3};", "SKILLHUB" }
            }
            div {
                style: "display:flex;align-items:center;justify-content:space-between;margin-bottom:18px;",
                div { style: "display:flex;align-items:baseline;gap:10px;",
                    span { style: "font-family:{serif};font-size:22px;font-weight:600;", "技能库" }
                    span { style: "font-size:12.5px;color:{ink3};", "{n} 技能" }
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12.5px;",
                    onclick: move |_| creating.set(!creating()),
                    if creating() { "取消" } else { "+ 新建技能" }
                }
            }
            if creating() {
                CreateSkillForm { on_done: move |_| creating.set(false) }
            }
            if hub.skills.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有技能——点「+ 新建技能」录入第一个。" }
            } else {
                div {
                    style: "display:grid;grid-template-columns:repeat(3,1fr);gap:14px;",
                    for s in hub.skills.clone() {
                        {
                            let sid = s.id;
                            let is_open = expanded() == Some(sid);
                            let is_editing = editing() == Some(sid);
                            let used_by = workflows_using_skill(&hub, &s.name);
                            let owner_project = s
                                .project_id
                                .and_then(|pid| projects.iter().find(|p| p.id == pid))
                                .map(|p| p.name.clone());
                            // L4: 出处可信度——蒸馏来源(真实来自哪件活 · 哪个
                            // agent 产出),不是编的社会证明。
                            let origin_agent_name = s
                                .origin_agent
                                .and_then(|aid| hub.agents.iter().find(|a| a.id == aid))
                                .map(|a| a.name.clone());
                            rsx! {
                                SkillCard {
                                    key: "{sid.uuid()}",
                                    s,
                                    is_open,
                                    is_editing,
                                    used_by,
                                    owner_project,
                                    origin_agent_name,
                                    on_toggle: move |_| {
                                        expanded.set(if is_open { None } else { Some(sid) });
                                        editing.set(None);
                                    },
                                    on_edit: move |_| editing.set(Some(sid)),
                                    on_done_edit: move |_| editing.set(None),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Real reverse lookup: which Hub workflows list this skill (by name — same
/// free-text `SkillRef` convention as `SkillAgentPicker`, not a hard FK).
/// `pub(crate)`: L1's `component_detail.rs` reuses this for the project-rail
/// skill detail card — one lookup, not two copies.
pub(crate) fn workflows_using_skill(hub: &HubVm, skill_name: &str) -> Vec<String> {
    hub.workflow_details
        .iter()
        .filter(|d| d.skills.iter().any(|(name, _, _)| name == skill_name))
        .map(|d| d.row.name.clone())
        .collect()
}

/// Truncate a skill's real body to a one-line preview — never a synthesized
/// summary, just the literal opening text so far as it fits.
fn content_preview(content: &str) -> String {
    let flat = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 72 {
        format!("{}…", flat.chars().take(72).collect::<String>())
    } else {
        flat
    }
}

#[component]
fn SkillCard(
    s: SkillCardVm,
    is_open: bool,
    is_editing: bool,
    used_by: Vec<String>,
    /// 真实项目名(从 project_id 反查)——`None` = 共享/全局目录条目,不编
    /// 一个假归属出来。
    owner_project: Option<String>,
    /// L4: 真实反查出的蒸馏产出 agent 名——`None` = 非蒸馏(目录条目)或
    /// 蒸馏来源 agent 已不在库(诚实缺席,不补一个假名)。
    origin_agent_name: Option<String>,
    on_toggle: EventHandler<()>,
    on_edit: EventHandler<()>,
    on_done_edit: EventHandler<()>,
) -> Element {
    let card = theme::card();
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let (chip_bg, chip_fg) = ("#EFE9DA", theme::INK_2);
    let chip = theme::chip(chip_bg, chip_fg);
    let span_style = if is_open { "grid-column:1/-1;" } else { "" };
    let distilled = s.distilled_from_issue.is_some();
    rsx! {
        div {
            style: "{card} padding:16px 18px;{span_style}",
            // ── 1. 身份行:名称 + 归属 + 成熟度 ──
            div {
                style: "display:flex;align-items:center;gap:8px;margin-bottom:6px;cursor:pointer;",
                onclick: move |_| on_toggle.call(()),
                span { style: "font-family:{theme::MONO};font-size:13px;font-weight:500;", "{s.name}" }
                if let Some(p) = &owner_project {
                    span { style: "{theme::chip(\"#F2E4DD\", theme::CLAY)}", "◇ {p}" }
                }
                span { style: "{chip} margin-left:auto;", "{s.maturity_label}" }
            }
            // ── 2. 一句话价值主张:这个技能解决什么 ──
            if !s.desc.is_empty() {
                div { style: "font-size:12px;color:{ink2};line-height:1.6;margin-bottom:8px;", "{s.desc}" }
            }
            // ── 3. 社会证明:真实引用数 + 被多少工作流用 ──
            div {
                style: "font-size:11px;color:{ink3};font-family:{theme::MONO};margin-bottom:6px;",
                "{s.uses} 次引用 · 被 {used_by.len()} 个工作流使用"
            }
            // ── 4. 出处可信度:来源 + 真实蒸馏出处(非编造)──
            div {
                style: "display:flex;align-items:center;gap:8px;font-size:11px;color:{ink3};margin-bottom:8px;flex-wrap:wrap;",
                span { "{s.category}" }
                span { "·" }
                span { "{s.source_label}" }
                if distilled {
                    span {
                        style: "{theme::chip(\"#EAF0E2\", \"#4A5E42\")}",
                        if let Some(a) = &origin_agent_name {
                            "⚗ 蒸馏自实战 · {a}"
                        } else {
                            "⚗ 蒸馏自实战"
                        }
                    }
                }
            }
            // ── 5. 结构预览:正文首句(有则给一眼,没有诚实说无)──
            if s.content.trim().is_empty() {
                div { style: "font-size:11px;color:{ink3};", "目录引用 · 无正文" }
            } else {
                div {
                    style: "font-size:11px;color:{ink3};font-family:{theme::MONO};overflow:hidden;text-overflow:ellipsis;white-space:nowrap;",
                    "{content_preview(&s.content)}"
                }
            }
            if is_open {
                div {
                    style: "margin-top:12px;padding-top:12px;border-top:1px dashed {theme::BORDER};",
                    if is_editing {
                        EditSkillForm { s: s.clone(), on_done: move |_| on_done_edit.call(()) }
                    } else {
                        if s.content.trim().is_empty() {
                            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "目录引用 · 无正文(全文在来源仓库;可「编辑」补充本地正文)" }
                        } else {
                            div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "技能正文(运行时注入 prompt)" }
                            pre {
                                style: "font-family:{theme::MONO};font-size:11.5px;line-height:1.6;color:{ink2};background:{theme::CARD_ALT};border:1px solid {theme::BORDER};border-radius:8px;padding:10px 12px;white-space:pre-wrap;margin:0 0 10px;",
                                "{s.content}"
                            }
                        }
                        div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "被这些工作流使用" }
                        if used_by.is_empty() {
                            div { style: "font-size:12px;color:{ink3};margin-bottom:10px;", "还没有工作流引用这个技能。" }
                        } else {
                            div {
                                style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:10px;",
                                for (i , wname) in used_by.iter().enumerate() {
                                    span { key: "{i}", style: "{theme::chip(\"#F4F0E7\", theme::CLAY)}", "{wname}" }
                                }
                            }
                        }
                        button {
                            style: "cursor:pointer;background:transparent;color:{theme::CLAY};border:1px solid {theme::CLAY};border-radius:7px;padding:6px 14px;font-size:12px;",
                            onclick: move |_| on_edit.call(()),
                            "编辑 →"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn EditSkillForm(s: SkillCardVm, on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;
    let skill_id = s.id;

    let mut name = use_signal(|| s.name.clone());
    let mut desc = use_signal(|| s.desc.clone());
    let mut category = use_signal(|| s.category.clone());
    let mut content = use_signal(|| s.content.clone());

    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        k.send(Command::UpdateSkill {
            id: skill_id,
            name: n,
            desc: desc().trim().to_string(),
            category: category().trim().to_string(),
            content: content().trim().to_string(),
        });
        on_done.call(());
    };

    rsx! {
        div {
            style: "background:{theme::CARD_ALT};border:1px solid {theme::BORDER_DEEP};border-radius:9px;padding:14px 16px;",
            div { style: "font-size:12px;color:{theme::CLAY};margin-bottom:10px;font-weight:600;", "编辑「{s.name}」" }
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "描述" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{desc}",
                oninput: move |e| desc.set(e.value()),
            }
            div { style: "{label}", "分类" }
            input {
                style: "{input} margin-bottom:10px;",
                value: "{category}",
                oninput: move |e| category.set(e.value()),
            }
            div { style: "{label}", "正文(可执行指令,运行时注入 prompt;留空=仅目录引用)" }
            textarea {
                style: "{input} margin-bottom:12px;min-height:120px;font-family:{theme::MONO};font-size:11.5px;line-height:1.6;resize:vertical;",
                value: "{content}",
                oninput: move |e| content.set(e.value()),
            }
            div {
                style: "display:flex;align-items:center;gap:10px;",
                button {
                    style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                    onclick: save,
                    "保存"
                }
                button {
                    style: "cursor:pointer;background:transparent;color:{ink3};border:1px solid {theme::BORDER};border-radius:7px;padding:7px 14px;font-size:12.5px;",
                    onclick: move |_| on_done.call(()),
                    "取消"
                }
            }
        }
    }
}

#[component]
fn CreateSkillForm(on_done: EventHandler<()>) -> Element {
    let k = use_context::<Kernel>();
    let card = theme::card();
    let input = theme::input();
    let label = theme::label();
    let ink3 = theme::INK_3;

    let mut name = use_signal(String::new);
    let mut desc = use_signal(String::new);
    let mut category = use_signal(String::new);
    let mut content = use_signal(String::new);

    let save = move |_| {
        let n = name().trim().to_string();
        if n.is_empty() {
            return;
        }
        k.send(Command::CreateSkill {
            id: SkillId::new(),
            name: n,
            desc: desc().trim().to_string(),
            category: category().trim().to_string(),
            source: LibSource::SelfBuilt,
            content: content().trim().to_string(),
        });
        name.set(String::new());
        desc.set(String::new());
        category.set(String::new());
        content.set(String::new());
        on_done.call(());
    };

    rsx! {
        div {
            style: "{card} padding:16px 18px;margin-bottom:16px;",
            div { style: "{label}", "名称" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 web-scan",
                value: "{name}",
                oninput: move |e| name.set(e.value()),
            }
            div { style: "{label}", "描述" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "这个技能做什么",
                value: "{desc}",
                oninput: move |e| desc.set(e.value()),
            }
            div { style: "{label}", "分类" }
            input {
                style: "{input} margin-bottom:10px;",
                placeholder: "如 检索 / 数据 / 前端",
                value: "{category}",
                oninput: move |e| category.set(e.value()),
            }
            div { style: "{label}", "正文(可执行指令,运行时注入 prompt;留空=仅目录引用)" }
            textarea {
                style: "{input} margin-bottom:12px;min-height:100px;font-family:{theme::MONO};font-size:11.5px;line-height:1.6;resize:vertical;",
                placeholder: "### 方法\n1. …",
                value: "{content}",
                oninput: move |e| content.set(e.value()),
            }
            div {
                style: "display:flex;align-items:center;gap:10px;",
                button {
                    style: "cursor:pointer;background:{theme::CLAY};color:#FFF;border:none;border-radius:7px;padding:7px 16px;font-size:12.5px;",
                    onclick: save,
                    "保存"
                }
                span { style: "font-size:11.5px;color:{ink3};", "新建的技能默认「打磨中」。" }
            }
        }
    }
}
