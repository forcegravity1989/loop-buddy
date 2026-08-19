//! 项目内 · 知识库。三个页签:知识(仓内文档)、代码图、资产。
//!
//! 三条如实:
//!
//! 1. **没有索引表**。库里只有四张表,这一屏的每个数字都是打开时现扫仓目录、
//!    现走 git、现解析仓文件得来的。
//! 2. **代码图装了才亮**。没装 codegraph 就整块灰,并且说清下一步该跑什么;
//!    不猜、不给一个空榜冒充「这个仓没有大文件」。
//! 3. **只读**。改文档一律走活 + MR,这一屏不提供编辑框。

use crate::bridge::{Bridge, Req};
use crate::theme;
use crate::vm::{AssetsVm, CodeGraphVm, KbFileVm, KbTab, ProjectVm, SkillVm};
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    rsx! {
        div {
            style: "display:flex;flex-direction:column;gap:14px;",
            {tabs(p, bridge)}
            match p.kb.tab {
                KbTab::Docs => docs_tab(p, bridge),
                KbTab::CodeGraph => codegraph_tab(p.kb.codegraph.as_ref(), bridge),
                KbTab::Assets => assets_tab(p.kb.assets.as_ref(), bridge),
            }
        }
    }
}

fn tabs(p: &ProjectVm, bridge: &Bridge) -> Element {
    rsx! {
        div {
            style: "display:flex;gap:6px;",
            for t in KbTab::ALL {
                {tab_button(t, p.kb.tab == t, bridge)}
            }
        }
    }
}

fn tab_button(t: KbTab, active: bool, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let style = if active {
        format!(
            "cursor:pointer;border:1px solid {};border-radius:8px;padding:7px 16px;\
             font-size:13px;background:{};color:#FFF;",
            theme::CLAY,
            theme::CLAY
        )
    } else {
        format!(
            "cursor:pointer;border:1px solid {};border-radius:8px;padding:7px 16px;\
             font-size:13px;background:transparent;color:{};",
            theme::BORDER,
            theme::INK_2
        )
    };
    rsx! {
        button {
            key: "{t:?}",
            style: "{style}",
            onclick: move |_| b.send(Req::KbTab(t)),
            "{t.label()}"
        }
    }
}

// ── 知识页签 ─────────────────────────────────────────────

fn docs_tab(p: &ProjectVm, bridge: &Bridge) -> Element {
    rsx! {
        div {
            style: "display:flex;gap:16px;align-items:flex-start;",
            div {
                style: "width:300px;flex:none;{theme::card()}padding:14px;\
                        max-height:calc(100vh - 210px);overflow:auto;",
                if p.kb.groups.is_empty() {
                    div {
                        style: "font-size:12px;color:{theme::INK_4};line-height:1.9;",
                        "这个仓还没有铺底,也没有周计划 —— 去总览点「规范铺底」。"
                    }
                }
                for g in p.kb.groups.iter() {
                    div {
                        key: "{g.title}",
                        style: "margin-bottom:14px;",
                        div {
                            style: "display:flex;gap:6px;align-items:baseline;margin-bottom:6px;",
                            div { style: "font-size:12px;color:{theme::INK_3};", "{g.title}" }
                            div { style: "font-size:11px;color:{theme::INK_4};", "{g.files.len()}" }
                        }
                        for f in g.files.iter() {
                            {doc_row(f, p.kb.open_doc.as_ref().map(|(k, _)| k.as_str()), bridge)}
                        }
                    }
                }
            }
            div {
                style: "flex:1;min-width:0;{theme::card()}padding:24px 28px;\
                        max-height:calc(100vh - 210px);overflow:auto;",
                match &p.kb.open_doc {
                    None => rsx! {
                        div {
                            style: "color:{theme::INK_3};font-size:13px;line-height:2;",
                            "点左边一份文档看内容。"
                            br {}
                            "树不是扫全仓扫出来的 —— 是按规范的几大类去固定路径找,找到才列。"
                            br {}
                            "标了「回填」的周文件是从 git 历史补出来的,和人写的同目录同格式。"
                        }
                    },
                    Some((path, body)) => rsx! {
                        div {
                            style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_4};\
                                    margin-bottom:14px;",
                            "{path}"
                        }
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
    let bg = if active {
        theme::CARD_ALT
    } else {
        "transparent"
    };
    rsx! {
        div {
            key: "{f.rel}",
            style: "padding:6px 8px;border-radius:6px;cursor:pointer;background:{bg};\
                    font-size:12px;color:{theme::INK_2};word-break:break-all;line-height:1.6;\
                    display:flex;gap:6px;align-items:baseline;",
            onclick: move |_| b.send(Req::OpenDoc(Some(rel.clone()))),
            span { style: "flex:1;", "{f.label}" }
            if !f.badge.is_empty() {
                span { style: "{theme::chip(theme::CARD_ALT, theme::INK_4)}", "{f.badge}" }
            }
        }
    }
}

// ── 代码图页签 ───────────────────────────────────────────

fn codegraph_tab(cg: Option<&CodeGraphVm>, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let Some(cg) = cg else {
        return rsx! {
            div {
                style: "{theme::card()}padding:20px 22px;font-size:13px;color:{theme::INK_3};\
                        line-height:1.9;",
                "点一下上面的「代码图」页签就现跑一次。"
                br {}
                span {
                    style: "font-size:12px;color:{theme::INK_4};",
                    "每次都是新的子进程调用,不缓存 —— 数字永远是此刻的仓,不是上次的。"
                }
            }
        };
    };
    rsx! {
        div {
            style: "{theme::card()}padding:20px 22px;",
            div {
                style: "display:flex;align-items:baseline;gap:10px;margin-bottom:12px;",
                div { style: "font-family:{theme::SERIF};font-size:18px;", "大文件榜" }
                div {
                    style: "font-size:12px;color:{theme::INK_3};",
                    "codegraph files -j,按体积排序取前 20"
                }
                div { style: "flex:1;" }
                button {
                    style: "{theme::btn_ghost()}padding:6px 12px;font-size:12px;",
                    onclick: move |_| b.send(Req::KbTab(KbTab::CodeGraph)),
                    "重新跑一次"
                }
            }
            if cg.state != "ready" {
                div {
                    style: "font-size:13px;color:{theme::INK_3};line-height:1.9;\
                            white-space:pre-wrap;background:{theme::CARD_ALT};\
                            border-radius:8px;padding:14px 16px;",
                    "{cg.hint}"
                }
            }
            if !cg.error.is_empty() {
                div {
                    style: "font-size:12px;color:{theme::ALERT_DEEP};line-height:1.8;\
                            white-space:pre-wrap;font-family:{theme::MONO};",
                    "{cg.error}"
                }
            }
            if cg.state == "ready" && cg.error.is_empty() && cg.rows.is_empty() {
                div { style: "font-size:13px;color:{theme::INK_4};", "—" }
            }
            for r in cg.rows.iter() {
                div {
                    key: "{r.path}",
                    style: "display:flex;gap:10px;align-items:baseline;padding:7px 0;\
                            border-top:1px solid {theme::BORDER};font-size:12px;",
                    span {
                        style: "flex:1;font-family:{theme::MONO};color:{theme::INK_2};\
                                word-break:break-all;",
                        "{r.path}"
                    }
                    span { style: "color:{theme::INK_4};width:70px;flex:none;", "{r.language}" }
                    span {
                        style: "color:{theme::INK_3};width:80px;flex:none;text-align:right;",
                        "{r.nodes} 个符号"
                    }
                    span {
                        style: "color:{theme::INK_3};width:80px;flex:none;text-align:right;\
                                font-family:{theme::MONO};",
                        "{r.size} B"
                    }
                }
            }
            div {
                style: "margin-top:14px;font-size:12px;color:{theme::INK_4};line-height:1.8;",
                "只摆原始数字,不下结论。特别是:零调用者不等于死代码 —— 这个仓大量用 \
                 dyn Trait 动态派发,调用关系本来就查不全。"
            }
        }
    }
}

// ── 资产页签 ─────────────────────────────────────────────

fn assets_tab(a: Option<&AssetsVm>, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let Some(a) = a else {
        return rsx! {
            div {
                style: "{theme::card()}padding:20px 22px;font-size:13px;color:{theme::INK_3};\
                        line-height:1.9;",
                "点一下上面的「资产」页签就现扫一次。"
                br {}
                span {
                    style: "font-size:12px;color:{theme::INK_4};",
                    "五个区块全部现算:扫 .claude/skills/、走 git log、解析 docs/releases.md。\
                     没有登记表可查。"
                }
            }
        };
    };
    rsx! {
        div {
            style: "display:flex;flex-direction:column;gap:14px;",
            div {
                style: "display:flex;justify-content:flex-end;",
                button {
                    style: "{theme::btn_ghost()}padding:6px 12px;font-size:12px;",
                    onclick: move |_| b.send(Req::KbTab(KbTab::Assets)),
                    "重新扫一次"
                }
            }
            {block("技能与 workflow", "扫仓里的 .claude/skills/**/SKILL.md。「用过几次」按活挂的 \
                    workflow 现算,没有胜率 —— V4 不留战绩账本。", rsx! {
                if a.skills.is_empty() {
                    div { style: "font-size:13px;color:{theme::INK_4};", "暂无" }
                }
                for s in a.skills.iter() {
                    {skill_row(s)}
                }
            })}
            {block("蒸馏出来的技能", "把做完的活蒸馏成技能这颗按钮 V4 还没建(docs/LEFTOVERS.md \
                    V4B-6),所以这里现在恒为空 —— 不放占位数据。", rsx! {
                if a.distilled.is_empty() {
                    div { style: "font-size:13px;color:{theme::INK_4};", "暂无" }
                }
                for s in a.distilled.iter() {
                    {skill_row(s)}
                }
            })}
            {block("产物登记", "没有登记表 —— git log --name-only 就是登记表。列的是最近 200 个 \
                    提交碰过的文件,每个文件记最近碰它的那一次。", rsx! {
                if a.artifacts.is_empty() {
                    div { style: "font-size:13px;color:{theme::INK_4};", "暂无登记产物" }
                }
                for f in a.artifacts.iter().take(60) {
                    div {
                        key: "{f.path}",
                        style: "display:flex;gap:10px;align-items:baseline;padding:6px 0;\
                                border-top:1px solid {theme::BORDER};font-size:12px;",
                        span {
                            style: "font-family:{theme::MONO};color:{theme::INK_4};width:70px;flex:none;",
                            "{f.commit}"
                        }
                        span {
                            style: "flex:1;font-family:{theme::MONO};color:{theme::INK_2};\
                                    word-break:break-all;",
                            "{f.path}"
                        }
                        if !f.issue.is_empty() {
                            span { style: "{theme::chip(theme::CARD_ALT, theme::INK_3)}", "{f.issue}" }
                        }
                    }
                }
            })}
            {block("发版记录", "解析 docs/releases.md —— 那份文件是唯一正本,库里没有版本表。", rsx! {
                if a.releases.is_empty() {
                    div { style: "font-size:13px;color:{theme::INK_4};", "暂无发版记录" }
                }
                for r in a.releases.iter() {
                    div {
                        key: "{r.version}",
                        style: "display:flex;gap:10px;align-items:baseline;padding:6px 0;\
                                border-top:1px solid {theme::BORDER};font-size:12px;",
                        span { style: "font-family:{theme::MONO};color:{theme::INK_2};width:80px;flex:none;", "{r.version}" }
                        span { style: "color:{theme::INK_4};width:96px;flex:none;", "{r.released_at}" }
                        span { style: "flex:1;color:{theme::INK_3};", "{r.note}" }
                        if !r.included.is_empty() {
                            span { style: "color:{theme::INK_4};", "{r.included}" }
                        }
                        span { style: "{theme::chip(theme::CARD_ALT, theme::INK_4)}", "{r.origin}" }
                    }
                }
            })}
            {block("仓统计", "和总览那一块同一次采集逻辑,打开时现算,没有后台定时刷新。", rsx! {
                if !a.error.is_empty() {
                    div {
                        style: "font-size:12px;color:{theme::ALERT_DEEP};font-family:{theme::MONO};",
                        "{a.error}"
                    }
                }
                div {
                    style: "display:flex;gap:26px;flex-wrap:wrap;",
                    for (k, v) in a.repo_stats.iter() {
                        div {
                            key: "{k}",
                            div { style: "font-size:11px;color:{theme::INK_4};", "{k}" }
                            div { style: "font-family:{theme::SERIF};font-size:22px;color:{theme::INK};", "{v}" }
                        }
                    }
                }
            })}
        }
    }
}

fn skill_row(s: &SkillVm) -> Element {
    rsx! {
        div {
            key: "{s.slug}",
            style: "display:flex;gap:10px;align-items:baseline;padding:7px 0;\
                    border-top:1px solid {theme::BORDER};font-size:12px;",
            span { style: "flex:1;color:{theme::INK_2};word-break:break-all;", "{s.title}" }
            span { style: "{theme::chip(theme::CARD_ALT, theme::INK_4)}", "{s.origin}" }
            span { style: "color:{theme::INK_3};width:70px;flex:none;text-align:right;", "用过 {s.uses} 次" }
        }
    }
}

fn block(title: &str, hint: &str, body: Element) -> Element {
    rsx! {
        div {
            style: "{theme::card()}padding:18px 20px;",
            div { style: "font-family:{theme::SERIF};font-size:16px;margin-bottom:4px;", "{title}" }
            div { style: "font-size:12px;color:{theme::INK_3};margin-bottom:12px;line-height:1.8;", "{hint}" }
            {body}
        }
    }
}

/// 纯 Rust CommonMark 渲染。不联网、不加载外部样式。
///
/// **文档里内嵌的原始 HTML 一律丢掉。** 这些 Markdown 来自项目仓的 `docs/`
/// —— agent 天天在往里写东西的地方。原样透传的话,一份带
/// `<img src=x onerror=…>` 的文档打开就在桌面壳的 WebView 里执行 JS;一个
/// 远程 `<img>` 就把「不联网」这句话变成假话。
fn render_markdown(body: &str) -> Element {
    use pulldown_cmark::Event;
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);
    opts.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(body, opts).filter_map(|e| match e {
        // HTML 块整块丢掉;行内 HTML 降级成纯文本,让人看得见原文写了什么。
        Event::Html(_) => None,
        Event::InlineHtml(raw) => Some(Event::Text(raw)),
        other => Some(other),
    });
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    rsx! {
        div {
            style: "font-size:14px;line-height:1.85;color:{theme::INK};",
            dangerous_inner_html: "{html}",
        }
    }
}
