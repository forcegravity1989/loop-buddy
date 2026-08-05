//! `Hub::Skill` — the skill library: a flat card grid, matching the
//! prototype's own SkillHub. Real store-backed CRUD, and now (like
//! WorkflowHub) a real detail/edit panel: click a card to expand it in
//! place — full description, a real "被这些工作流使用" reverse lookup
//! computed from `hub.workflow_details` (a `SkillRef.name` match, same
//! by-name convention as everywhere else this hub cross-references
//! skills/agents), and an edit form dispatching `Command::UpdateSkill` —
//! content only, `maturity`/`uses` stay untouched.
//!
//! T7 (2026-07-23, plan/12 §0/§2): a stage-role filter chip row — the same
//! dimension Workflow Hub already had (its stage chips), sharing
//! `ui::vm::RoleFilter`/`role_chip_counts` across all three Hub screens
//! instead of three ad hoc ones. 2026-08-05:Skill 侧改多值(`RoleTag`),chip
//! 行五档:全部/{五角色}/全阶段通用/不属任何阶段/未归类——后两枚只在这屏,见
//! Agent/Workflow Hub 各自的模块文档为何不加。

use crate::kernel::{HubVm, Kernel};
use crate::screens::markdown::MarkdownView;
use crate::theme;
use bw_app::Command;
use bw_core::model::HubSource;
use bw_core::SkillId;
use dioxus::prelude::*;
use std::collections::HashSet;
use ui::vm::{
    role_chip_counts, skill_file_tree, ProjectCardVm, RoleFilter, SkillCardVm, SkillTreeNode,
};

#[component]
pub fn SkillHub(hub: HubVm, projects: Vec<ProjectCardVm>) -> Element {
    let paper = theme::PAPER;
    let serif = theme::SERIF;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let mono = theme::MONO;
    let n = hub.skills.len();

    let mut creating = use_signal(|| false);
    let mut expanded = use_signal(|| None::<SkillId>);
    let mut editing = use_signal(|| None::<SkillId>);
    let mut role_filter = use_signal(|| RoleFilter::All);

    let counts = role_chip_counts(
        &hub.skills
            .iter()
            .map(|s| s.role_tag.clone())
            .collect::<Vec<_>>(),
    );
    let filtered: Vec<SkillCardVm> = hub
        .skills
        .iter()
        .filter(|s| role_filter().matches(&s.role_tag))
        .cloned()
        .collect();

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
            div {
                style: "display:flex;flex-wrap:wrap;gap:6px;margin-bottom:16px;",
                {
                    let active = role_filter() == RoleFilter::All;
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| role_filter.set(RoleFilter::All),
                            "全部"
                        }
                    }
                }
                for (sk , count) in counts.per_stage.clone() {
                    {
                        let active = role_filter() == RoleFilter::Stage(sk);
                        let (bg, fg): (&str, &str) = if active { (sk.color(), "#FFF") } else { ("#EFE9DA", ink2) };
                        rsx! {
                            button {
                                key: "{sk.index()}",
                                style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                                onclick: move |_| role_filter.set(RoleFilter::Stage(sk)),
                                "{sk.role_short()} · {count}"
                            }
                        }
                    }
                }
                {
                    let active = role_filter() == RoleFilter::Universal;
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| role_filter.set(RoleFilter::Universal),
                            "全阶段通用 · {counts.universal}"
                        }
                    }
                }
                {
                    let active = role_filter() == RoleFilter::NoStage;
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| role_filter.set(RoleFilter::NoStage),
                            "不属任何阶段 · {counts.no_stage}"
                        }
                    }
                }
                {
                    let active = role_filter() == RoleFilter::Unclassified;
                    let (bg, fg): (&str, &str) = if active { (theme::CLAY, "#FFF") } else { ("#EFE9DA", ink2) };
                    rsx! {
                        button {
                            style: "{theme::chip(bg, fg)} cursor:pointer;border:none;padding:4px 10px;",
                            onclick: move |_| role_filter.set(RoleFilter::Unclassified),
                            "未归类 · {counts.unclassified}"
                        }
                    }
                }
            }
            if hub.skills.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "还没有技能——点「+ 新建技能」录入第一个。" }
            } else if filtered.is_empty() {
                div { style: "color:{ink3};font-size:13px;padding:30px 0;", "没有符合筛选的技能。" }
            } else {
                div {
                    // 宽度自适应,不再固定 3 列——固定列数在宽屏(如 2560px 窗口)
                    // 下每列被无限拉宽,长文本一行变成几百字符,没法读。改用
                    // auto-fill + minmax:窄窗口自动减列、宽窗口自动加列,单张
                    // 卡永远落在 [300px, 一列可用宽度] 区间。300px 下限按卡面
                    // 身份行实测:等宽字体技能名(~12-16 字符 ≈ 100-130px)+
                    // 「◇ 项目名」归属 chip(~70-90px)+ 成熟度 chip(~55-70px)
                    // 在 18px×2 卡内边距之后仍需单行放下,300px(内容区
                    // ≈264px)是这一行常见组合不换行挤爆的最小整数下限。
                    style: "display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:14px;",
                    for s in filtered {
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
/// plan/16 §5: counts both the top-level `SkillRef` list *and* the T16
/// phase-level binding (`phase_skills`) — the five standard workflows carry
/// their stage skill on every phase and nothing at the top level, so reading
/// only `skills` reported "被 0 个工作流使用" for exactly the skills the app
/// itself injects on every run. `pub(crate)`: L1's `component_detail.rs`
/// reuses this for the project-rail skill detail card — one lookup, not two
/// copies.
pub(crate) fn workflows_using_skill(hub: &HubVm, skill_name: &str) -> Vec<String> {
    hub.workflow_details
        .iter()
        .filter(|d| {
            d.skills.iter().any(|(name, _, _)| name == skill_name)
                || d.phase_skills.iter().any(|name| name == skill_name)
        })
        .map(|d| d.row.name.clone())
        .collect()
}

/// plan/16 §2 防线 3 · the one spec-compliance visual, shared by the SkillHub
/// card and the project-rail component detail (可视化一致 = one component,
/// not two styles). Renders **nothing** when compliant — 绿色隐身,只有
/// 待校正出声(黄) — and lists every finding verbatim in the detail context.
#[component]
pub(crate) fn SkillSpecNotes(s: SkillCardVm, compact: bool) -> Element {
    let amber = ui::signal_color(bw_core::model::Signal::Amber);
    let ink3 = theme::INK_3;
    if s.spec_findings.is_empty() {
        return rsx! {};
    }
    if compact {
        // 卡面:只在有硬规违规时出声;纯 Advisory 不点黄灯。
        if s.spec_must_fix == 0 {
            return rsx! {};
        }
        return rsx! {
            span {
                style: "{theme::chip(\"#F2E8D2\", amber)}",
                "规范 · 待校正 {s.spec_must_fix}"
            }
        };
    }
    rsx! {
        div { style: "font-size:11px;color:{ink3};margin-bottom:6px;", "规范核对(plan/16)" }
        div {
            style: "display:flex;flex-direction:column;gap:4px;margin-bottom:10px;",
            for (i , f) in s.spec_findings.iter().enumerate() {
                {
                    let must_fix = f.severity == bw_core::skill_spec::SpecSeverity::MustFix;
                    let (mark, color) = if must_fix { ("待校正", amber) } else { ("提示", ink3) };
                    rsx! {
                        div {
                            key: "{i}",
                            style: "font-size:11.5px;line-height:1.6;color:{ink3};",
                            span { style: "color:{color};font-weight:600;", "{f.rule} · {mark} " }
                            span { "{f.message}" }
                        }
                    }
                }
            }
        }
    }
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

/// T4(plan/12 §2)/T18: the skill detail's real body — always the same
/// double-column「文件树 + 预览」包浏览器, whether the skill folder has
/// support files (`skill_file` rows, T2, mattpocock/superpowers imports) or
/// not (自建/蒸馏/五阶段内置). Every skill's real unit is a package (at
/// least one `SKILL.md`), so a one-node tree for a support-file-less skill
/// is honest, not a "fake tree" — the previous single-column early-return
/// (removed here) was the last spot rendering skills through two different
/// visual systems depending on import source. `SKILL.md` (`s.content`) is
/// always the fixed, default-selected root entry; the rest of the tree is
/// built purely off real `rel_path`s (`ui::vm::skill_file_tree`, empty when
/// `s.files` is empty — never a guessed structure). `pub(crate)`:
/// `component_detail.rs`'s project-rail skill detail reuses this, same
/// one-copy convention `workflows_using_skill` already established for this
/// screen.
///
/// Product decision (locked, don't re-derive): a code-review pass once
/// collapsed the 200px sidebar + 360px height cap for support-file-less
/// skills on readability grounds (more of them get to use full width). The
/// Builder overruled that — every skill card, including the vast majority
/// with zero support files, must render through the *exact same* box shape
/// as a real multi-file package (reference: `writing-skills`, SKILL.md + 6
/// files). Template alignment outranks the readability win here; don't
/// reintroduce the conditional.
#[component]
pub(crate) fn SkillFileBrowser(s: SkillCardVm) -> Element {
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;

    let tree = skill_file_tree(&s.files);
    // 纯前端状态:选中文件路径(`None` = 固定置顶默认选中的 SKILL.md)+ 折叠的
    // 目录集合——都是显示态,不需要新命令。
    let mut selected = use_signal(|| None::<String>);
    let collapsed = use_signal(HashSet::<String>::new);

    let selected_path = selected();
    let (selected_label, selected_content) = match &selected_path {
        None => ("SKILL.md".to_string(), s.content.clone()),
        Some(path) => {
            let content = s
                .files
                .iter()
                .find(|f| &f.rel_path == path)
                .map(|f| f.content.clone())
                .unwrap_or_default();
            (path.clone(), content)
        }
    };
    let root_selected = selected_path.is_none();
    let (root_bg, root_fg): (&str, &str) = if root_selected {
        (theme::CARD, theme::CLAY)
    } else {
        ("transparent", ink2)
    };
    // T15:.md 文件(含固定置顶的 SKILL.md 本身)走共用 MD 渲染组件;非 .md
    // 支撑文件(yaml/sh/cjs/json/py …)保持现有等宽原文——不猜测它是不是
    // "看起来像 markdown"。
    let is_md = selected_path
        .as_deref()
        .map(|p| matches!(p.rsplit('.').next().unwrap_or(""), "md" | "mdx"))
        .unwrap_or(true);

    // T18(用户复核收紧):模板对齐——不是「有支撑文件才配得上文件夹外观」,
    // 而是每个技能包(哪怕只有 SKILL.md 一个文件)都按同一套「文件树 + 预览」
    // 外壳渲染,与 writing-skills(SKILL.md + 6 支撑文件)那种真有多文件的
    // 包**完全同一个模板**——侧栏、句式、高度上限一律不因文件数分叉。此前
    // 一度按文件数收起过侧栏(读起来更宽),但那等于又长出第二套展示,正是
    // 「所有 Skill 可视化一致」要消灭的东西;宽度代价接受,一致性优先。
    let has_files = !s.files.is_empty();
    let package_label = if has_files {
        format!("技能包(SKILL.md + {} 个支撑文件)", s.files.len())
    } else {
        "技能包(SKILL.md)".to_string()
    };
    // 空正文占位说明:root(SKILL.md 本身)沿用旧分支更具体的措辞——它多半
    // 是纯目录引用(全文在来源仓库),值得提示「可编辑补充」;子文件的空则是
    // 普通空文件,不套同一句诚不实的话。
    let content_empty_label = if selected_path.is_none() {
        "目录引用 · 无正文(全文在来源仓库;可「编辑」补充本地正文)".to_string()
    } else {
        "(空文件)".to_string()
    };

    rsx! {
        div {
            style: "font-size:11px;color:{ink3};margin-bottom:6px;",
            "{package_label}"
        }
        div {
            style: "display:flex;border:1px solid {theme::BORDER};border-radius:8px;overflow:hidden;margin-bottom:10px;",
            div {
                style: "width:200px;flex:none;background:{theme::CARD_ALT};border-right:1px solid {theme::BORDER};max-height:360px;overflow-y:auto;padding:6px 0;",
                div {
                    style: "display:flex;align-items:center;gap:6px;padding:5px 12px;cursor:pointer;font-family:{theme::MONO};font-size:12px;background:{root_bg};color:{root_fg};",
                    onclick: move |_| selected.set(None),
                    span { "▤" }
                    span { "SKILL.md" }
                }
                // has_files == false ⇒ tree 为空,侧栏就只剩上面这一行固定
                // 的 SKILL.md——一个只有一个文件的文件夹,如实,不是假树。
                for node in tree.iter() {
                    SkillTreeItem { node: node.clone(), depth: 1, selected, collapsed }
                }
            }
            div {
                style: "flex:1;min-width:0;display:flex;flex-direction:column;background:{theme::CARD};",
                div {
                    style: "font-family:{theme::MONO};font-size:11px;color:{ink3};padding:7px 14px;border-bottom:1px dashed {theme::BORDER};flex:none;",
                    "{selected_label}"
                }
                if is_md {
                    div {
                        style: "padding:12px 14px;overflow-y:auto;max-height:360px;",
                        MarkdownView {
                            content: selected_content.clone(),
                            empty_label: content_empty_label.clone(),
                        }
                    }
                } else if selected_content.trim().is_empty() {
                    div { style: "font-size:12px;color:{ink3};padding:14px;", "(空文件)" }
                } else {
                    pre {
                        style: "font-family:{theme::MONO};font-size:11.5px;line-height:1.6;color:{ink2};margin:0;padding:12px 14px;white-space:pre-wrap;overflow-y:auto;max-height:360px;",
                        "{selected_content}"
                    }
                }
            }
        }
    }
}

/// One row of `SkillFileBrowser`'s left-hand tree, recursing into its own
/// children — a directory toggles its own collapse state (keyed by its full
/// path, so two same-named dirs at different nesting never collide); a file
/// selects itself into the right-hand preview.
#[component]
fn SkillTreeItem(
    node: SkillTreeNode,
    depth: usize,
    selected: Signal<Option<String>>,
    collapsed: Signal<HashSet<String>>,
) -> Element {
    let mut selected = selected;
    let mut collapsed = collapsed;
    let ink2 = theme::INK_2;
    let ink3 = theme::INK_3;
    let indent = 12 + depth * 14;

    match node {
        SkillTreeNode::Dir {
            name,
            path,
            children,
        } => {
            let is_collapsed = collapsed().contains(&path);
            let arrow = if is_collapsed { "▸" } else { "▾" };
            let toggle_path = path.clone();
            rsx! {
                div {
                    style: "display:flex;align-items:center;gap:6px;padding:5px 12px 5px {indent}px;cursor:pointer;font-family:{theme::MONO};font-size:12px;color:{ink3};",
                    onclick: move |_| {
                        collapsed.with_mut(|set| {
                            if !set.remove(&toggle_path) {
                                set.insert(toggle_path.clone());
                            }
                        });
                    },
                    span { "{arrow}" }
                    span { "{name}" }
                }
                if !is_collapsed {
                    for child in children {
                        SkillTreeItem { node: child, depth: depth + 1, selected, collapsed }
                    }
                }
            }
        }
        SkillTreeNode::File { name, rel_path } => {
            let is_sel = selected().as_deref() == Some(rel_path.as_str());
            let icon = file_icon(&rel_path);
            let (bg, fg): (&str, &str) = if is_sel {
                (theme::CARD, theme::CLAY)
            } else {
                ("transparent", ink2)
            };
            let click_path = rel_path.clone();
            rsx! {
                div {
                    style: "display:flex;align-items:center;gap:6px;padding:5px 12px 5px {indent}px;cursor:pointer;font-family:{theme::MONO};font-size:12px;background:{bg};color:{fg};",
                    onclick: move |_| selected.set(Some(click_path.clone())),
                    span { "{icon}" }
                    span { "{name}" }
                }
            }
        }
    }
}

/// Extension → generic icon glyph — a "simple mapping" (plan/12 §2's own
/// wording), not a MIME registry: the extensions the two real imported
/// libraries (mattpocock/superpowers) actually carry get a distinguishing
/// glyph, anything else honestly falls back to a plain generic-file mark
/// instead of a guessed one.
fn file_icon(rel_path: &str) -> &'static str {
    match rel_path.rsplit('.').next().unwrap_or("") {
        "md" | "mdx" => "▤",
        "yaml" | "yml" => "⚙",
        "sh" | "bash" => "$",
        "cjs" | "js" | "mjs" | "ts" => "{}",
        "json" | "toml" => "≡",
        "py" => "λ",
        _ => "▫",
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
    // 展开态横跨整行取版面,但正文可读宽度必须封顶——否则 span 到整个宽屏窗
    // 口后,SkillFileBrowser 的正文/Markdown 一行变成几百字符。760px 与
    // `component_detail.rs` 的 `SkillDetailCard`(同一份 SkillFileBrowser 内容)
    // 对齐,而不是另定一套数值;那边已验证 200px 文件树 + 双栏预览在这个宽
    // 度下不挤不溢出。网格默认 `justify-items:stretch`,给了 max-width 之后
    // 会退化成从列起始边对齐、不撑满——效果等价于左对齐,不需要额外居中。
    let span_style = if is_open {
        "grid-column:1/-1;max-width:760px;"
    } else {
        ""
    };
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
                SkillSpecNotes { s: s.clone(), compact: true }
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
                // T11(plan/12 §7):编辑脱离源头后的留痕——不再顶官方库徽记
                // (source_label 已随 HubSource 翻转自动变「自建」),但如实
                // 留一句「改编自 <库名>」,不假装这条从没来自过官方选型。
                if let Some(lib) = &s.adapted_from {
                    span { style: "color:{ink3};font-style:italic;", "改编自 {lib}" }
                }
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
                        SkillFileBrowser { s: s.clone() }
                        SkillSpecNotes { s: s.clone(), compact: false }
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
            source: HubSource::SelfBuilt,
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
