//! 项目内 · 配置。**结构照 `hifi/index.html` 的 `renderConfig` 排**:开工工具
//! 映射 / 技能与 workflow / 连接器 + 项目群 / 定时,四大块,都是真表格。
//!
//! 三条如实:
//!
//! 1. **「用过几次」是现算的** —— 扫活的 workflow 列聚合,没有战绩表可查。
//! 2. **技能清单没有登记表** —— buddy 自带的那十三份编在二进制里(摊在 buddy
//!    自己的资产目录,不复制进用户的仓),项目自有的扫仓里 `.claude/skills/`
//!    得到,同名以仓里那份为准。高保真把「workflow」和「skill」分成两张表,
//!    V4 里它们是同一样东西(workflow = SOP 类技能包),所以合成一张,不假装
//!    有两套。
//! 3. **定时那张表没有「下次触发」列** —— 判据是「本周有没有这张活」,不查
//!    任何定时表,所以那一列写的是判据本身。

use crate::bridge::Bridge;
use crate::vm::{MappingVm, ProjectVm};
use bw_v4::command::Command;
use bw_v4::model::category_from_key;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    rsx! {
        section { style: "max-width:1120px;",
            {mapping_block(p, bridge)}
            {skill_block(p)}
            div { class: "cfg-pair",
                {connector_block(p, bridge)}
                {cron_block(p)}
            }
            StandardBlock { p: p.clone(), bridge: bridge.clone() }
        }
    }
}

// ── ① 开工工具映射 ─────────────────────────────────

fn mapping_block(p: &ProjectVm, bridge: &Bridge) -> Element {
    rsx! {
        div { class: "cfg-section",
            div { class: "cfg-section-head", h3 { "开工工具映射" } }
            if p.config.mappings.is_empty() {
                div { class: "detail-empty",
                    "这个项目还没有 .bw/issue-policy.toml。到下面「规范件」那块点「规范铺底」把它铺出来。"
                }
            } else {
                div { class: "tbl-wrap",
                    table { class: "tbl",
                        thead {
                            tr {
                                th { "类别" }
                                th { "开工工具" }
                                th { "workflow" }
                                th { "" }
                            }
                        }
                        tbody {
                            for m in p.config.mappings.iter() {
                                MappingRow {
                                    key: "{m.category_key}",
                                    m: m.clone(),
                                    pid: p.id,
                                    bridge: bridge.clone(),
                                }
                            }
                        }
                    }
                }
            }
            div { class: "cfg-readonly-note",
                "建一张活的时候,类别决定它默认用哪个工具、挂哪个 workflow。正本是仓里的 \
                 .bw/issue-policy.toml —— 点「保存」就是改那份文件。"
            }
        }
    }
}

/// 一行映射。**必须是组件**:它有自己的 `use_signal`,而它是在 `for` 循环里
/// 渲染的 —— 做成普通函数的话,hook 数量会随映射条数变,行与行的输入框内容
/// 会串位(改 A 类别却存到 B 类别上)。
#[component]
fn MappingRow(m: MappingVm, pid: bw_v4::model::ProjectId, bridge: Bridge) -> Element {
    let mut tool = use_signal(|| m.tool.clone());
    let mut workflow = use_signal(|| m.workflow.clone());
    let b = bridge.clone();
    let key = m.category_key.clone();
    let save = move |_| {
        let Some(cat) = category_from_key(&key) else {
            return;
        };
        b.cmd(Command::SaveToolMapping {
            project_id: pid,
            category: cat,
            tool: normalize_tool(&tool.read()),
            workflow: workflow.read().trim().to_string(),
        });
    };
    rsx! {
        tr {
            td { "{m.category_label}" }
            td {
                select {
                    class: "select-sm",
                    value: "{tool}",
                    onchange: move |e| tool.set(e.value()),
                    option { value: "Claude CLI", "Claude CLI" }
                    option { value: "Cursor", "Cursor" }
                    option { value: "Open Design", "Open Design" }
                    option { value: "—", "—(未定)" }
                }
            }
            td {
                input {
                    class: "input",
                    style: "min-width:200px;",
                    value: "{workflow}",
                    placeholder: "workflow / 技能包名,留空 = 无默认",
                    oninput: move |e| workflow.set(e.value()),
                }
            }
            td { button { class: "btn btn-sm btn-primary", onclick: save, "保存" } }
        }
    }
}

fn normalize_tool(label: &str) -> String {
    match label.trim() {
        "Claude CLI" => "claude_cli".into(),
        "Cursor" => "cursor".into(),
        "Open Design" => "open_design".into(),
        _ => String::new(),
    }
}

// ── ② 技能与 workflow ───────────────────────────────

fn skill_block(p: &ProjectVm) -> Element {
    rsx! {
        div { class: "cfg-section",
            div { class: "cfg-section-head",
                h3 { "技能与 workflow" }
                span { style: "font-size:11.5px;color:var(--ink-3);",
                    "buddy 自带 + 本仓 .claude/skills/,共 {p.config.skills.len()} 个"
                }
            }
            if p.config.skills.is_empty() {
                div { class: "detail-empty", "一个都没有 —— buddy 自带的那份没读出来,这不正常。" }
            } else {
                div { class: "tbl-wrap",
                    table { class: "tbl",
                        thead {
                            tr {
                                th { "名称" }
                                th { "来源" }
                                th { "一句话" }
                                th { "用过几次" }
                            }
                        }
                        tbody {
                            for s in p.config.skills.iter() {
                                tr { key: "{s.slug}",
                                    td { "{s.title}" }
                                    td {
                                        span {
                                            class: if s.origin == "蒸馏" { "chip chip-green" } else { "chip chip-gray" },
                                            "{s.origin}"
                                        }
                                    }
                                    td { style: "color:var(--ink-2);", {desc_or_dash(&s.desc)} }
                                    td { class: "mono",
                                        if s.uses == 0 { "—(还没被任何活用过)" } else { "{s.uses}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "cfg-readonly-note",
                "workflow 的入口是一份 SKILL.md,写的是一整套做事流程,自己调度该用哪个 agent \
                 —— 所以 V4 里 workflow 和技能是同一样东西,不分两张表。「用过几次」现算:\
                 扫活的 workflow 列聚合,没有战绩表。导入与启用/停用还没接,这里只列不改。"
            }
        }
    }
}

fn desc_or_dash(d: &str) -> String {
    if d.is_empty() {
        "—(SKILL.md 里没写 description)".into()
    } else {
        d.to_string()
    }
}

// ── ③ 连接器 + 项目群 ───────────────────────────────

fn connector_block(p: &ProjectVm, bridge: &Bridge) -> Element {
    let c = &p.config;
    rsx! {
        div { class: "cfg-section",
            div { class: "cfg-section-head", h3 { "连接器" } }
            if c.connectors.is_empty() {
                div { class: "detail-empty", "这个仓没有 .bw/connectors.toml。" }
            } else {
                div { class: "tbl-wrap",
                    table { class: "tbl",
                        thead { tr { th { "连接器" } th { "种类" } th { "跑什么" } } }
                        tbody {
                            for x in c.connectors.iter() {
                                tr { key: "{x.name}",
                                    td { "{x.name}" }
                                    td { span { class: "chip chip-gray", "{x.kind}" } }
                                    td { class: "mono", style: "font-size:10.6px;", "{x.target}" }
                                }
                            }
                        }
                    }
                }
            }

            div { style: "border-top:1px dashed var(--border);margin-top:12px;padding-top:12px;font-size:12.2px;",
                div { style: "font-weight:600;margin-bottom:7px;", "项目群" }
                div { style: "display:flex;flex-direction:column;gap:7px;",
                    div {
                        "提供方 "
                        span {
                            class: if c.chat_provider == "未配" { "chip chip-gray" } else { "chip chip-clay" },
                            "{c.chat_provider}"
                        }
                    }
                    div { "群号 " span { class: "mono", "{c.chat_group}" } }
                    div {
                        "同步哪些通知 "
                        if c.chat_events.is_empty() {
                            span { style: "color:var(--ink-3);", "—(没配群)" }
                        }
                        for (label, on) in c.chat_events.iter() {
                            span { key: "{label}", style: "display:inline-flex;align-items:center;gap:5px;margin-right:12px;font-size:11.3px;",
                                span { class: if *on { "tglswitch on" } else { "tglswitch" }, span { class: "knob" } }
                                "{label}"
                            }
                        }
                    }
                }
                div { class: "cfg-readonly-note",
                    "发出去就算完:不记账、不去重、失败不自动重发 —— 极小概率下同一件事会\
                     重推一条,这是已经认了的代价。"
                    strong { "仓是正本" }
                    ",改群号或改勾选走「编辑项目信息」那条活 + MR,不在这里直接写仓。"
                }
            }

            div { style: "border-top:1px dashed var(--border);margin-top:12px;padding-top:12px;",
                div { style: "font-size:11.5px;color:var(--ink-3);margin-bottom:8px;",
                    "开工工具探活 · 远端仓 {c.remote}"
                }
                div { style: "display:flex;gap:8px;flex-wrap:wrap;",
                    for t in c.tools.iter() {
                        {probe_chip(&t.name, &t.label, t.ok, &t.detail, bridge)}
                    }
                }
            }
        }
    }
}

fn probe_chip(name: &str, label: &str, ok: Option<bool>, detail: &str, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let n = name.to_string();
    let color = match ok {
        Some(true) => "var(--green)",
        Some(false) => "var(--red)",
        None => "var(--gray)",
    };
    rsx! {
        button {
            key: "{name}",
            class: "btn btn-sm",
            title: "{detail}",
            onclick: move |_| b.cmd(Command::ProbeTool { name: n.clone() }),
            span { class: "dot", style: "background:{color};margin-right:6px;" }
            "{label} · 测一下"
        }
    }
}

// ── ④ 定时 ──────────────────────────────────────────

fn cron_block(p: &ProjectVm) -> Element {
    rsx! {
        div { class: "cfg-section",
            div { class: "cfg-section-head", h3 { "定时" } }
            if p.config.crons.is_empty() {
                div { class: "detail-empty", "{p.config.cadence}" }
            } else {
                div { class: "tbl-wrap",
                    table { class: "tbl",
                        thead { tr { th { "运作活" } th { "怎么触发" } th { "节律" } th { "判据" } } }
                        tbody {
                            for c in p.config.crons.iter() {
                                tr { key: "{c.name}",
                                    td { "{c.name}" }
                                    td { span { class: "chip", "{c.trigger}" } }
                                    td { class: "mono", "{c.schedule}" }
                                    td { style: "color:var(--ink-2);", "{c.rule}" }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "cfg-readonly-note",
                strong { "没有定时表" }
                ",所以没有「下次触发时间」这一列。到点只会自动"
                strong { "建" }
                "活,绝不自动推进 —— 自动建出来的活一样停在待办,「完成」永远由人点。"
            }
        }
    }
}

// ── ⑤ 规范件与在研版本 ──────────────────────────────

/// 对账是**纯读**的:只回报缺 / 过期 / 人改过,不动仓、不建活 —— 要不要补由
/// 人决定。「规范铺底」才是真写仓的那一颗,它会建活提 MR。
#[component]
fn StandardBlock(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    let b_rec = bridge.clone();
    let b_boot = bridge.clone();
    let b_ver = bridge.clone();
    let pid = p.id;
    let mut version = use_signal(|| p.card.current_version_raw.clone());
    // 「规范铺底」要写仓、提交、推分支、开 MR —— 十几秒起步。不报进度的话
    // 按下去界面一动不动,人会以为没点上。和接入屏同一条通道、同一种画法。
    let mut log = use_signal(Vec::<bw_v4::app::ProgressLine>::new);
    let prog = bridge.progress.clone();
    use_future(move || {
        let mut rx = prog.subscribe();
        async move {
            while let Ok(line) = rx.recv().await {
                let mut rows = log.write();
                match rows
                    .iter()
                    .position(|r: &bw_v4::app::ProgressLine| r.step == line.step)
                {
                    Some(i) => rows[i] = line,
                    None => rows.push(line),
                }
            }
        }
    });
    let std_ver = if p.card.standard_version.is_empty() {
        "—(这个仓还没铺过规范件)".to_string()
    } else {
        p.card.standard_version.clone()
    };
    rsx! {
        div { class: "cfg-section",
            div { class: "cfg-section-head",
                h3 { "规范件与在研版本" }
                div { style: "display:flex;gap:8px;",
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| b_rec.cmd(Command::ReconcileStandard { project_id: pid }),
                        "对一遍账"
                    }
                    button {
                        class: "btn btn-sm btn-primary",
                        onclick: move |_| {
                            log.write().clear();
                            b_boot.cmd(Command::RunStandardBootstrap { project_id: pid });
                        },
                        "规范铺底"
                    }
                }
            }
            div { class: "settings-list",
                div { class: "settings-row",
                    div { div { class: "k", "规范版本" } div { class: "v", "{std_ver}" } }
                }
                div { class: "settings-row",
                    div { style: "flex:1;",
                        div { class: "k", "在研版本" }
                        input {
                            class: "input mono",
                            style: "margin-top:5px;max-width:180px;",
                            value: "{version}",
                            oninput: move |e| version.set(e.value()),
                        }
                    }
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| b_ver.cmd(Command::SetCurrentVersion {
                            project_id: pid,
                            version: version.read().trim().to_string(),
                        }),
                        "保存"
                    }
                }
            }
            {crate::chrome::progress_log(&log.read())}
            div { class: "cfg-readonly-note",
                "对账只看不改:缺哪几份、哪几份过期了、哪几份被人手改过,结果显示在页脚回执里。\
                 「规范铺底」会真的写仓 —— 建分支、写核心件、建一张活提 MR,停在评审中等人合。"
            }
        }
    }
}
