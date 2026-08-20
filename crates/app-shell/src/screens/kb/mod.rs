//! 项目内 · 知识库。**结构照 `hifi/index.html` 的 `renderSpace` 排**:顶上一条
//! 规范条,下面三个页签(知识 / 代码图 / 资产)。
//!
//! 四条如实:
//!
//! 1. **没有索引表**。库里只有四张表,这一屏的每个数字都是打开时现扫仓目录、
//!    现走 git、现解析仓文件得来的。
//! 2. **代码图装了才亮**。没装 codegraph 就整块灰,并且说清下一步该跑什么;
//!    不猜、不给一个空榜冒充「这个仓没有大文件」。
//! 3. **顶上那条规范条只报数,不对账**。对账与铺底是配置屏那两颗按钮的事 ——
//!    对账要读一遍全部核心件并逐份比对指纹,不能每次打开知识库都跑一次。
//! 4. **只读**。改文档一律走活 + MR,这一屏不提供编辑框。

use crate::bridge::{Bridge, Panel, PanelNav, Req};
use crate::vm::{AssetsVm, CodeGraphVm, KbFileVm, KbTab, ProjectVm, SkillVm};
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let nav = use_context::<PanelNav>();
    let (p, bridge) = (&p, &bridge);
    let std_ver = if p.card.standard_version.is_empty() {
        "—(还没铺过规范件)".to_string()
    } else {
        format!("规范 v{}", p.card.standard_version)
    };
    rsx! {
        section {
            div { class: "card sp-topinfo",
                span { "{std_ver}" }
                span { "管账里登记着 {p.kb.managed_count} 份核心件" }
                span { class: "spacer" }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| nav.go(Panel::Config),
                    "去配置屏对账 / 铺底 →"
                }
            }
            div { class: "tabbtn-row",
                for t in KbTab::ALL {
                    {tab_button(t, p.kb.tab == t, bridge)}
                }
            }
            match p.kb.tab {
                KbTab::Docs => docs_tab(p, bridge),
                KbTab::CodeGraph => codegraph_tab(p.kb.codegraph.as_ref(), bridge),
                KbTab::Assets => assets_tab(p.kb.assets.as_ref(), bridge),
            }
        }
    }
}

fn tab_button(t: KbTab, active: bool, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    rsx! {
        div {
            key: "{t:?}",
            class: if active { "tabbtn active" } else { "tabbtn" },
            onclick: move |_| b.send(Req::KbTab(t)),
            "{t.label()}"
        }
    }
}

// ── 知识页签 ─────────────────────────────────────────────

fn docs_tab(p: &ProjectVm, bridge: &Bridge) -> Element {
    rsx! {
        div { class: "sp-layout",
            div { class: "card sp-tree",
                if p.kb.groups.is_empty() {
                    div { class: "detail-empty",
                        "这个仓还没有铺底,也没有周计划 —— 去配置屏点「规范铺底」。"
                    }
                }
                for g in p.kb.groups.iter() {
                    div { key: "{g.title}",
                        div { class: "sp-tree-group", "{g.title} · {g.files.len()}" }
                        for f in g.files.iter() {
                            {doc_row(f, p.kb.open_doc.as_ref().map(|(k, _)| k.as_str()), bridge)}
                        }
                    }
                }
            }
            div { class: "card sp-preview",
                match &p.kb.open_doc {
                    None => rsx! {
                        div { class: "detail-empty", style: "line-height:2;",
                            "点左边一份文档看内容。"
                            br {}
                            "树不是扫全仓扫出来的 —— 是按规范的几大类去固定路径找,找到才列。"
                            br {}
                            "标了「回填」的周文件是从 git 历史补出来的,和人写的同目录同格式。"
                        }
                    },
                    Some((path, body)) => rsx! {
                        div { class: "path", "{path}" }
                        {render_markdown(body)}
                    },
                }
            }
        }
    }
}

fn doc_row(f: &KbFileVm, open: Option<&str>, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let rel = f.rel.clone();
    let active = open == Some(f.rel.as_str());
    rsx! {
        div {
            key: "{f.rel}",
            class: if active { "sp-tree-item active" } else { "sp-tree-item" },
            title: "{f.rel}",
            onclick: move |_| b.send(Req::OpenDoc(Some(rel.clone()))),
            "{f.label}"
            if !f.badge.is_empty() {
                span { class: "chip-muted", style: "margin-left:5px;", "{f.badge}" }
            }
        }
    }
}

// ── 代码图页签 ───────────────────────────────────────────

fn codegraph_tab(cg: Option<&CodeGraphVm>, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let Some(cg) = cg else {
        return rsx! {
            div { class: "card", style: "padding:20px 22px;",
                div { class: "detail-empty",
                    "点一下上面的「代码图」页签就现跑一次。"
                    br {}
                    "每次都是新的子进程调用,不缓存 —— 数字永远是此刻的仓,不是上次的。"
                }
            }
        };
    };
    rsx! {
        div { class: "graph-lower",
            div { class: "card", style: "padding:12px 14px;",
                div { style: "display:flex;align-items:baseline;gap:8px;",
                    div { class: "sr-h", style: "margin-top:0;flex:1;", "大文件榜" }
                    button {
                        class: "btn btn-sm",
                        onclick: move |_| b.send(Req::KbTab(KbTab::CodeGraph)),
                        "重新跑一次"
                    }
                }
                if cg.state != "ready" {
                    div { class: "detail-empty", style: "white-space:pre-wrap;", "{cg.hint}" }
                }
                if !cg.error.is_empty() {
                    div {
                        class: "mono",
                        style: "font-size:11px;color:var(--alert-deep);white-space:pre-wrap;line-height:1.8;",
                        "{cg.error}"
                    }
                }
                if cg.state == "ready" && cg.error.is_empty() && cg.rows.is_empty() {
                    div { class: "detail-empty", "—" }
                }
                for r in cg.rows.iter() {
                    div { key: "{r.path}", class: "leaderboard-row",
                        span { style: "flex:1;word-break:break-all;", "{r.path}" }
                        span { style: "color:var(--ink-3);margin-left:10px;flex:none;",
                            "{r.language} · {r.nodes} 个符号 · {r.size} B"
                        }
                    }
                }
                div { class: "graph-foot",
                    "codegraph files -j,按体积排序取前 20 · 状态 {cg.state}"
                }
            }
            div { class: "card", style: "padding:12px 14px;",
                div { class: "sr-h", style: "margin-top:0;", "符号 → 调用者 / 影响面" }
                input {
                    class: "input",
                    disabled: true,
                    placeholder: "还没接:要走 codegraph 的符号查询",
                }
                div { class: "cfg-readonly-note", style: "margin-top:10px;",
                    "高保真这一格能按符号名查它被谁调用。真接起来要走 codegraph 的符号索引,\
                     还没做 —— 灰在这里,不放一个查了没结果的输入框。"
                }
                div { class: "sr-h", "怎么读这一屏" }
                div { style: "font-size:11.5px;color:var(--ink-3);line-height:1.85;",
                    "只摆原始数字,不下结论。特别是:零调用者不等于死代码 —— 这个仓大量用 \
                     dyn Trait 动态派发,调用关系本来就查不全。"
                    br {}
                    "高保真顶上那张 crate 依赖图是照 buddy 自己的仓画死的,换个项目就不对了;\
                     真画它要有一份真实的依赖来源,还没接,所以这里没有那张图。"
                }
            }
        }
    }
}

// ── 资产页签 ─────────────────────────────────────────────

fn assets_tab(a: Option<&AssetsVm>, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let Some(a) = a else {
        return rsx! {
            div { class: "card", style: "padding:20px 22px;",
                div { class: "detail-empty",
                    "点一下上面的「资产」页签就现扫一次。"
                    br {}
                    "五个区块全部现算:扫 .claude/skills/、走 git log、解析 docs/releases.md。\
                     没有登记表可查。"
                }
            }
        };
    };
    let more = if a.artifacts.len() > ARTIFACT_ROWS {
        format!(",下面只列前 {ARTIFACT_ROWS} 个")
    } else {
        String::new()
    };
    rsx! {
        div { style: "display:flex;justify-content:flex-end;margin-bottom:8px;",
            button {
                class: "btn btn-sm",
                onclick: move |_| b.send(Req::KbTab(KbTab::Assets)),
                "重新扫一次"
            }
        }

        div { class: "asset-sect-title", "技能与 workflow" }
        div { class: "cfg-readonly-note",
            "扫仓里的 .claude/skills/**/SKILL.md。「用过几次」按活挂的 workflow 现算,\
             没有胜率 —— V4 不留战绩账本。"
        }
        if a.skills.is_empty() {
            div { class: "detail-empty", "暂无" }
        }
        div { class: "asset-grid",
            for s in a.skills.iter() {
                {skill_card(s)}
            }
        }

        div { class: "asset-sect-title", "蒸馏出来的技能" }
        div { class: "cfg-readonly-note",
            "把做完的活蒸馏成技能这颗按钮 V4 还没建(docs/LEFTOVERS.md V4B-6),\
             所以这里现在恒为空 —— 不放占位数据。"
        }
        if a.distilled.is_empty() {
            div { class: "detail-empty", "暂无" }
        }
        div { class: "asset-grid",
            for s in a.distilled.iter() {
                {skill_card(s)}
            }
        }

        div { class: "asset-sect-title", "产物登记" }
        div { class: "cfg-readonly-note",
            "没有登记表 —— git log --name-only 就是登记表。扫最近 200 个提交,\
             每个文件记最近碰它的那一次,一共 {a.artifacts.len()} 个文件{more}。"
        }
        if a.artifacts.is_empty() {
            div { class: "detail-empty", "暂无登记产物" }
        }
        div { class: "card", style: "padding:10px 14px;",
            for f in a.artifacts.iter().take(ARTIFACT_ROWS) {
                div { key: "{f.path}", class: "leaderboard-row",
                    span { style: "color:var(--ink-4);width:66px;flex:none;", "{f.commit}" }
                    span { style: "flex:1;word-break:break-all;", "{f.path}" }
                    if !f.issue.is_empty() {
                        span { class: "chip chip-gray", style: "margin-left:8px;", "{f.issue}" }
                    }
                }
            }
        }

        div { class: "asset-sect-title", "发版记录" }
        div { class: "cfg-readonly-note",
            "解析 docs/releases.md —— 那份文件是唯一正本,库里没有版本表。"
        }
        if a.releases.is_empty() {
            div { class: "detail-empty", "暂无发版记录" }
        }
        div { class: "card", style: "padding:10px 14px;",
            for r in a.releases.iter() {
                div { key: "{r.version}", class: "leaderboard-row",
                    span { style: "width:80px;flex:none;", "{r.version}" }
                    span { style: "width:96px;flex:none;color:var(--ink-4);", "{r.released_at}" }
                    span { style: "flex:1;color:var(--ink-3);", "{r.note}" }
                    if !r.included.is_empty() {
                        span { style: "color:var(--ink-4);margin-left:8px;", "{r.included}" }
                    }
                    span { class: "chip chip-gray", style: "margin-left:8px;", "{r.origin}" }
                }
            }
        }

        div { class: "asset-sect-title", "仓统计" }
        if !a.error.is_empty() {
            div { class: "mono", style: "font-size:11px;color:var(--alert-deep);", "{a.error}" }
        }
        div { class: "repo-metric-grid",
            for (k, v) in a.repo_stats.iter() {
                div { key: "{k}", class: "repo-metric-item",
                    div { class: "v mono", "{v}" }
                    div { class: "k", "{k}" }
                }
            }
        }
    }
}

/// 产物区块最多列多少行。再多人也读不完,而且每行都是一个 DOM 节点。
/// **列不下的要说出来** —— 说明里带总数和这个上限。
const ARTIFACT_ROWS: usize = 60;

fn skill_card(s: &SkillVm) -> Element {
    rsx! {
        div { key: "{s.slug}", class: "card asset-card",
            div { class: "name", "{s.title}" }
            div { class: "meta",
                span { class: "chip chip-gray", "{s.origin}" }
                span { style: "margin-left:6px;",
                    if s.uses == 0 { "还没被任何活用过" } else { "用过 {s.uses} 次" }
                }
            }
            if !s.desc.is_empty() {
                div { class: "meta", "{s.desc}" }
            }
        }
    }
}

/// 纯 Rust CommonMark 渲染。不联网、不加载外部样式。
///
/// 这些 Markdown 来自项目仓的 `docs/` —— agent 天天在往里写东西的地方,
/// 所以三样东西一律不放过去:
///
/// 1. **内嵌的原始 HTML**。一份带 `<img src=x onerror=…>` 的文档打开就在桌面壳
///    的 WebView 里执行 JS。HTML 块整块丢掉,行内 HTML 降级成纯文本 ——
///    让人看得见原文写了什么,但它不再是标签。
/// 2. **图片**。markdown 原生的 `![](https://…)` 一样会让 WebView 去外部主机取
///    图,「不联网」那句话照样变成假话。整块丢掉,`alt` 文字留着。
/// 3. **链接**。降级成纯文本 —— 这是一个只读文档预览,不是浏览器;而且
///    `[点我](javascript:…)` 渲染出来就是一颗能点的雷。
fn render_markdown(body: &str) -> Element {
    use pulldown_cmark::{Event, Tag, TagEnd};
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);
    opts.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(body, opts).filter_map(|e| match e {
        Event::Html(_) => None,
        Event::InlineHtml(raw) => Some(Event::Text(raw)),
        // 丢掉 Start/End 这一对,中间的 alt / 链接文字照常留下来当正文。
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        Event::Start(Tag::Link { .. }) | Event::End(TagEnd::Link) => None,
        other => Some(other),
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    rsx! {
        div {
            style: "font-size:13.5px;line-height:1.85;",
            dangerous_inner_html: "{html}",
        }
    }
}
