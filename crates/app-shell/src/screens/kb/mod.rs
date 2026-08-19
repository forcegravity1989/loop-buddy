//! 项目内 · 知识库。A 刀做仓内 `docs/` 的文档树 + Markdown 渲染。
//!
//! 没有索引表:知识库就是仓里的文档,扫目录即得。代码图与资产页签是后面刀的事,
//! 这里如实说还没建,不放占位的假数据。

use crate::bridge::{Bridge, Req};
use crate::theme;
use crate::vm::ProjectVm;
use dioxus::prelude::*;

#[component]
pub fn View(p: ProjectVm, bridge: Bridge) -> Element {
    let (p, bridge) = (&p, &bridge);
    rsx! {
        div {
            style: "display:flex;gap:16px;align-items:flex-start;",
            div {
                style: "width:280px;flex:none;{theme::card()}padding:14px;\
                        max-height:calc(100vh - 160px);overflow:auto;",
                div { style: "font-size:12px;color:{theme::INK_3};margin-bottom:10px;", "仓内文档" }
                if p.kb.docs.is_empty() {
                    div {
                        style: "font-size:12px;color:{theme::INK_4};line-height:1.8;",
                        "这个仓的 docs/ 下还没有 Markdown 文档。"
                    }
                }
                for d in p.kb.docs.iter() {
                    {doc_row(d, p.kb.open_doc.as_ref().map(|(k, _)| k.as_str()), bridge)}
                }
            }
            div {
                style: "flex:1;min-width:0;{theme::card()}padding:24px 28px;\
                        max-height:calc(100vh - 160px);overflow:auto;",
                match &p.kb.open_doc {
                    None => rsx! {
                        div {
                            style: "color:{theme::INK_3};font-size:13px;line-height:2;",
                            "点左边一份文档看内容。"
                            br {}
                            "知识库没有索引表 —— 这里列的就是仓里真实存在的 Markdown 文件。"
                            br { }
                            br { }
                            span {
                                style: "color:{theme::INK_4};",
                                "代码图与资产页签还没建。"
                            }
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

fn doc_row(path: &str, open: Option<&str>, bridge: &Bridge) -> Element {
    let b = bridge.clone();
    let p = path.to_string();
    let active = open == Some(path);
    let bg = if active {
        theme::CARD_ALT
    } else {
        "transparent"
    };
    rsx! {
        div {
            key: "{path}",
            style: "padding:6px 8px;border-radius:6px;cursor:pointer;background:{bg};\
                    font-size:12px;color:{theme::INK_2};word-break:break-all;line-height:1.6;",
            onclick: move |_| b.send(Req::OpenDoc(Some(p.clone()))),
            "{path}"
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
