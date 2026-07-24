//! 共用 MD 富文本渲染组件(T15,plan/12 §10 v1.1 增补第 2 条)。
//!
//! 三处正文共用一个组件——Skill 文件树右栏(.md 文件)、Agent 常驻指令、
//! Workflow 详情的 goal 正文——都改走这里的 `MarkdownView`,而不是各自维护
//! 一份 `<pre>`。用 `pulldown-cmark`(纯 Rust,只加进这一个 crate,内核五个
//! crate 零新增依赖)把 CommonMark + 表格/删除线/任务列表解析成事件流,手写
//! 一个小型递归下降渲染器直接产出 Dioxus `Element` 树——没有再引入第三方
//! markdown→html→dioxus 的桥接库,产物就是原生元素,配色/字体直接吃
//! `theme` 模块的设计系统 token(正文 `theme::SERIF`、代码 `theme::MONO`、
//! 暖纸/clay 配色)。
//!
//! **覆盖的 MD 元素**:标题(h1-h6)/段落/列表(有序+无序,含嵌套)/代码块
//! (带语言标签)/行内代码/表格(含对齐)/引用块/链接/粗体/斜体/删除线/任务
//! 列表复选框/分隔线。**不支持**(按此仓库"不假装"的原则,遇到就静默跳过
//! 而不是崩溃或显示乱码/半成品标签):原始 HTML 块/行内 HTML、脚注、定义
//! 列表、数学公式——这几类之外的语法本仓库的 SKILL.md/AGENT.md 源文件基本
//! 用不到,解析选项(`Options`)也没打开对应开关,事件流里根本不会出现。
//!
//! **链接的取舍(如实记录)**:wry 的 WebView 就是整个应用窗口本身——真给
//! 链接走浏览器式的原生导航,会把用户带离 BW 整个界面(没有地址栏、没有
//! 后退按钮),等于"点一下链接=app 卡在外部页面出不来"。选择:链接渲染成
//! clay 色、点状下划线的可点文本;只有 `http(s)://` 开头的真实 URL 才可点
//! ——点击时不改变 WebView 自身的导航,而是 `std::process::Command` 拉起
//! 系统默认浏览器打开(macOS `open` / Windows `cmd /C start` / 其他类 Unix
//! `xdg-open`),跟现有 `ClaudeCliExecutor` 一样走"shell 出真实进程"这条已
//! 验证的路子。非 http(s) 协议(本地路径、`mailto:` 等)一律降级成不可点的
//! 纯文字,避免拉起未知协议处理器。
//!
//! **frontmatter 的取舍**:`SkillCard.content` 本身在导入时已把 frontmatter
//! 剥离(见 `bw_app::skill_import`),但 `skill_file` 里独立的 `.md` 子文件
//! 可能原样保留了自己的 `---` 头。这个组件渲染前统一探测一次开头的
//! `---`…`---` 块,命中就用 `serde_yaml`(复用 T2 已经选定的真实 YAML
//! 解析器,workspace 里本就有,不是新引入的第三方依赖——不是手搓的行扫描)
//! 解析成一张紧凑键值表,单独放在正文上方,不混进 MD body 渲染;YAML 解析
//! 失败时不吞掉内容,原样退化成一段等宽块并标注"解析失败"。

use crate::theme;
use dioxus::prelude::*;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// 三处接入点共用的顶层组件:默认渲染态,右上角「原文」开关切回等宽原文
/// (纯前端 `use_signal`,不需要新命令,同 `SkillFileBrowser` 已有的
/// selected/collapsed 信号一个性质)。空内容诚实显示 `empty_label`,不渲染
/// 一个空壳,也不显示开关(切换一个空态没有意义)。
#[component]
pub fn MarkdownView(content: String, empty_label: Option<String>) -> Element {
    let mut raw_mode = use_signal(|| false);

    if content.trim().is_empty() {
        let label = empty_label.unwrap_or_else(|| "(空)".to_string());
        return rsx! {
            div { style: "font-size:12px;color:{theme::INK_3};", "{label}" }
        };
    }

    rsx! {
        div {
            div {
                style: "display:flex;justify-content:flex-end;margin-bottom:6px;",
                button {
                    style: "cursor:pointer;background:transparent;border:1px solid {theme::BORDER};color:{theme::INK_3};border-radius:6px;padding:2px 10px;font-size:10.5px;font-family:{theme::MONO};",
                    onclick: move |_| raw_mode.set(!raw_mode()),
                    if raw_mode() { "渲染 →" } else { "原文 →" }
                }
            }
            if raw_mode() {
                pre {
                    style: "font-family:{theme::MONO};font-size:11.5px;line-height:1.6;color:{theme::INK_2};background:{theme::CARD_ALT};border:1px solid {theme::BORDER};border-radius:8px;padding:10px 12px;white-space:pre-wrap;margin:0;",
                    "{content}"
                }
            } else {
                {render_rich(&content)}
            }
        }
    }
}

/// frontmatter 探测 + 折叠属性卡 + MD body 渲染——`MarkdownView` 渲染态的
/// 实体部分,独立出来只是让"开关/空态"和"怎么渲染"分开,不是复用边界。
fn render_rich(content: &str) -> Element {
    let wrap_style = format!(
        "font-family:{};font-size:13px;line-height:1.75;color:{};",
        theme::SERIF,
        theme::INK_2
    );
    match split_frontmatter(content) {
        Some((fm, body)) => rsx! {
            div {
                style: "{wrap_style}",
                {render_frontmatter_card(&fm)}
                {render_markdown_body(&body)}
            }
        },
        None => rsx! {
            div { style: "{wrap_style}", {render_markdown_body(content)} }
        },
    }
}

/// 探测并剥离开头的 `---\n...\n---\n` frontmatter 块——同 `bw-app` 导入侧
/// (`skill_import::split_frontmatter`)的约定一致(必须是文本第一行
/// `---`),这里是展示态的"尽力而为"探测:第二个 `---` 没有闭合就诚实当作
/// 没有 frontmatter,整段原样进入 MD 渲染,不猜测半截结构。
fn split_frontmatter(raw: &str) -> Option<(String, String)> {
    let mut lines = raw.lines();
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return None,
    }
    let mut fm_lines: Vec<&str> = Vec::new();
    let mut closed = false;
    for line in lines.by_ref() {
        if line.trim() == "---" {
            closed = true;
            break;
        }
        fm_lines.push(line);
    }
    if !closed {
        return None;
    }
    let body = lines.collect::<Vec<_>>().join("\n");
    Some((fm_lines.join("\n"), body))
}

/// frontmatter YAML → 紧凑属性卡(键值小表)。解析失败/不是映射类型都不
/// 吞内容,原样展示并标注状态,不假装解析成功了。
fn render_frontmatter_card(fm: &str) -> Element {
    match serde_yaml::from_str::<serde_yaml::Value>(fm) {
        Ok(serde_yaml::Value::Mapping(map)) if !map.is_empty() => {
            let rows: Vec<(String, String)> = map
                .iter()
                .map(|(k, v)| (yaml_scalar(k), format_yaml_value(v)))
                .collect();
            rsx! {
                div {
                    style: "margin-bottom:12px;border:1px solid {theme::BORDER};border-radius:8px;overflow:hidden;background:{theme::CARD_ALT};",
                    div {
                        style: "font-family:{theme::MONO};font-size:10px;letter-spacing:.05em;color:{theme::INK_3};padding:6px 12px;border-bottom:1px dashed {theme::BORDER};",
                        "FRONTMATTER"
                    }
                    div {
                        style: "padding:2px 0;",
                        for (k , v) in rows {
                            div {
                                key: "{k}",
                                style: "display:flex;gap:10px;padding:4px 12px;font-size:11.5px;",
                                span { style: "font-family:{theme::MONO};color:{theme::CLAY};flex:none;min-width:120px;", "{k}" }
                                span { style: "color:{theme::INK_2};word-break:break-word;", "{v}" }
                            }
                        }
                    }
                }
            }
        }
        Ok(_) => rsx! {
            div {
                style: "margin-bottom:12px;font-size:11px;color:{theme::INK_3};",
                "frontmatter 不是键值映射,已跳过属性卡展示"
            }
        },
        Err(_) => rsx! {
            div {
                style: "margin-bottom:12px;",
                div { style: "font-size:10.5px;color:{theme::INK_3};margin-bottom:4px;", "frontmatter(YAML 解析失败,原样展示)" }
                pre {
                    style: "font-family:{theme::MONO};font-size:11px;color:{theme::INK_3};background:{theme::CARD_ALT};border:1px solid {theme::BORDER};border-radius:8px;padding:8px 10px;white-space:pre-wrap;margin:0;",
                    "{fm}"
                }
            }
        },
    }
}

fn yaml_scalar(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "~".to_string(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn format_yaml_value(v: &serde_yaml::Value) -> String {
    match v {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Null => "—".to_string(),
        serde_yaml::Value::Sequence(seq) => seq
            .iter()
            .map(format_yaml_value)
            .collect::<Vec<_>>()
            .join(", "),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

// ── MD body:事件流 → Element 树 ──────────────────────────────────────────

fn render_markdown_body(text: &str) -> Element {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut iter = Parser::new_ext(text, options);
    let blocks = parse_flow(&mut iter, None);
    if blocks.is_empty() {
        return rsx! {
            div { style: "font-size:12px;color:{theme::INK_3};", "(正文为空)" }
        };
    }
    rsx! {
        div { {blocks.into_iter()} }
    }
}

/// 块级 tag——命中就先把此前攒的裸行内事件(松散段落/紧凑列表项里的无
/// `Paragraph` 包裹内容)冲刷成一个隐式段落再继续,不会跟块级内容混在一起。
fn is_block_start(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::DefinitionListTitle
            | Tag::DefinitionListDefinition
            | Tag::Table(_)
            | Tag::TableHead
            | Tag::TableRow
            | Tag::TableCell
            | Tag::MetadataBlock(_)
    )
}

/// 块级流解析:一个容器(文档根 / 引用块 / 列表项)的直接子内容——遇到块级
/// `Start` 就递归 `render_block`,遇到自己的收尾 `End(stop)` 就把攒的行内
/// 事件冲刷完毕后返回。既处理松散内容(显式 `Paragraph`),也处理紧凑列表项
/// /表格单元格那种没有 `Paragraph` 包裹、直接落在容器里的裸行内事件。
fn parse_flow<'a, I: Iterator<Item = Event<'a>>>(
    iter: &mut I,
    stop: Option<TagEnd>,
) -> Vec<Element> {
    let mut blocks: Vec<Element> = Vec::new();
    let mut inline_buf: Vec<Event<'a>> = Vec::new();

    while let Some(event) = iter.next() {
        if let Event::End(end) = &event {
            if Some(*end) == stop {
                flush_inline(&mut inline_buf, &mut blocks);
                return blocks;
            }
        }
        match event {
            Event::Start(tag) if is_block_start(&tag) => {
                flush_inline(&mut inline_buf, &mut blocks);
                blocks.push(render_block(tag, iter));
            }
            Event::Rule => {
                flush_inline(&mut inline_buf, &mut blocks);
                blocks.push(rsx! {
                    hr { style: "border:none;border-top:1px solid {theme::BORDER_DEEP};margin:14px 0;" }
                });
            }
            Event::End(_) => {
                // 不认识的收尾(理论上不会命中,防御性收尾而不是 panic 或死循环)。
                flush_inline(&mut inline_buf, &mut blocks);
            }
            other => inline_buf.push(other),
        }
    }
    flush_inline(&mut inline_buf, &mut blocks);
    blocks
}

fn flush_inline<'a>(buf: &mut Vec<Event<'a>>, blocks: &mut Vec<Element>) {
    if buf.is_empty() {
        return;
    }
    let events = std::mem::take(buf);
    let mut idx = 0;
    let nodes = render_inline_seq(&events, &mut idx, None);
    blocks.push(rsx! {
        p { style: "margin:6px 0;", {nodes.into_iter()} }
    });
}

fn render_block<'a, I: Iterator<Item = Event<'a>>>(tag: Tag<'a>, iter: &mut I) -> Element {
    let stop = tag.to_end();
    match tag {
        Tag::Paragraph => {
            let events = collect_inline_run(iter, stop);
            let mut idx = 0;
            let nodes = render_inline_seq(&events, &mut idx, None);
            rsx! {
                p { style: "margin:6px 0;", {nodes.into_iter()} }
            }
        }
        Tag::Heading { level, .. } => {
            let events = collect_inline_run(iter, stop);
            let mut idx = 0;
            let nodes = render_inline_seq(&events, &mut idx, None);
            render_heading(level, nodes)
        }
        Tag::BlockQuote(_) => {
            let inner = parse_flow(iter, Some(stop));
            rsx! {
                blockquote {
                    style: "margin:8px 0;padding:6px 14px;border-left:3px solid {theme::CLAY};background:{theme::CARD_ALT};color:{theme::INK_3};font-style:italic;border-radius:0 6px 6px 0;",
                    {inner.into_iter()}
                }
            }
        }
        Tag::CodeBlock(kind) => render_code_block(kind, iter, stop),
        Tag::List(start) => render_list(start, iter, stop),
        Tag::Table(aligns) => render_table(aligns, iter, stop),
        _ => {
            // 不在支持范围内的容器(原始 HTML 块/脚注/定义列表/元数据块,
            // 或误入此处的表格子 tag)——安全吞掉它自己的事件流,保持游标
            // 同步,不渲染任何东西,而不是崩溃或显示半成品。
            drain_until(iter, stop);
            rsx! {}
        }
    }
}

fn render_heading(level: HeadingLevel, nodes: Vec<Element>) -> Element {
    let style = heading_style(level);
    match level {
        HeadingLevel::H1 => rsx! { h1 { style: "{style}", {nodes.into_iter()} } },
        HeadingLevel::H2 => rsx! { h2 { style: "{style}", {nodes.into_iter()} } },
        HeadingLevel::H3 => rsx! { h3 { style: "{style}", {nodes.into_iter()} } },
        HeadingLevel::H4 => rsx! { h4 { style: "{style}", {nodes.into_iter()} } },
        HeadingLevel::H5 => rsx! { h5 { style: "{style}", {nodes.into_iter()} } },
        HeadingLevel::H6 => rsx! { h6 { style: "{style}", {nodes.into_iter()} } },
    }
}

fn heading_style(level: HeadingLevel) -> String {
    let (size, mt, mb, underline) = match level {
        HeadingLevel::H1 => ("19px", "4px", "10px", true),
        HeadingLevel::H2 => ("16.5px", "16px", "8px", true),
        HeadingLevel::H3 => ("14.5px", "14px", "6px", false),
        HeadingLevel::H4 => ("13.5px", "12px", "5px", false),
        HeadingLevel::H5 => ("13px", "10px", "4px", false),
        HeadingLevel::H6 => ("12.5px", "10px", "4px", false),
    };
    let border = if underline {
        format!(
            "border-bottom:1px dashed {};padding-bottom:6px;",
            theme::BORDER
        )
    } else {
        String::new()
    };
    format!(
        "font-family:{};font-weight:700;color:{};font-size:{size};margin:{mt} 0 {mb};line-height:1.35;{border}",
        theme::SERIF,
        theme::INK
    )
}

fn render_code_block<'a, I: Iterator<Item = Event<'a>>>(
    kind: CodeBlockKind<'a>,
    iter: &mut I,
    stop: TagEnd,
) -> Element {
    let mut code = String::new();
    for event in iter.by_ref() {
        match event {
            Event::Text(t) => code.push_str(&t),
            Event::End(e) if e == stop => break,
            _ => {}
        }
    }
    while code.ends_with('\n') {
        code.pop();
    }
    let lang = match &kind {
        CodeBlockKind::Fenced(l) => {
            let s = l.to_string();
            if s.trim().is_empty() {
                None
            } else {
                Some(s)
            }
        }
        CodeBlockKind::Indented => None,
    };
    rsx! {
        div {
            style: "margin:8px 0;border:1px solid {theme::BORDER};border-radius:8px;overflow:hidden;",
            if let Some(l) = &lang {
                div {
                    style: "font-family:{theme::MONO};font-size:10px;color:{theme::INK_3};background:{theme::CARD_ALT};padding:4px 10px;border-bottom:1px dashed {theme::BORDER};",
                    "{l}"
                }
            }
            pre {
                style: "margin:0;padding:10px 12px;background:{theme::CARD_ALT};overflow-x:auto;",
                code {
                    style: "font-family:{theme::MONO};font-size:12px;line-height:1.6;color:{theme::INK};white-space:pre;",
                    "{code}"
                }
            }
        }
    }
}

fn render_list<'a, I: Iterator<Item = Event<'a>>>(
    start: Option<u64>,
    iter: &mut I,
    stop: TagEnd,
) -> Element {
    let ordered = start.is_some();
    let mut items: Vec<Element> = Vec::new();
    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::Item) => {
                // 嵌套列表:子 `Tag::List` 本身也是块级 tag,`parse_flow`
                // 递归处理这个 item 内容时会自然把它当成一个子块渲染成
                // 嵌套的 <ul>/<ol>,不需要这里单独处理。
                let content = parse_flow(iter, Some(TagEnd::Item));
                items.push(rsx! {
                    li { style: "margin:2px 0;", {content.into_iter()} }
                });
            }
            Event::End(e) if e == stop => break,
            _ => {}
        }
    }
    if ordered {
        let first = start.unwrap_or(1) as isize;
        rsx! {
            ol { start: first, style: "margin:6px 0;padding-left:22px;", {items.into_iter()} }
        }
    } else {
        rsx! {
            ul { style: "margin:6px 0;padding-left:20px;", {items.into_iter()} }
        }
    }
}

fn render_table<'a, I: Iterator<Item = Event<'a>>>(
    aligns: Vec<Alignment>,
    iter: &mut I,
    stop: TagEnd,
) -> Element {
    let mut header: Vec<Vec<Event<'a>>> = Vec::new();
    let mut rows: Vec<Vec<Vec<Event<'a>>>> = Vec::new();
    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::TableHead) => header = collect_table_cells(iter, TagEnd::TableHead),
            Event::Start(Tag::TableRow) => rows.push(collect_table_cells(iter, TagEnd::TableRow)),
            Event::End(e) if e == stop => break,
            _ => {}
        }
    }
    let align_style = |i: usize| -> &'static str {
        match aligns.get(i) {
            Some(Alignment::Left) => "text-align:left;",
            Some(Alignment::Center) => "text-align:center;",
            Some(Alignment::Right) => "text-align:right;",
            _ => "text-align:left;",
        }
    };
    let header_cells: Vec<Element> = header
        .iter()
        .enumerate()
        .map(|(i, cell)| {
            let mut idx = 0;
            let nodes = render_inline_seq(cell, &mut idx, None);
            let al = align_style(i);
            rsx! {
                th {
                    style: "border:1px solid {theme::BORDER};padding:6px 10px;background:{theme::CARD_ALT};font-family:{theme::SERIF};font-weight:700;font-size:12px;{al}",
                    {nodes.into_iter()}
                }
            }
        })
        .collect();
    let body_rows: Vec<Element> = rows
        .iter()
        .map(|row| {
            let cells: Vec<Element> = row
                .iter()
                .enumerate()
                .map(|(i, cell)| {
                    let mut idx = 0;
                    let nodes = render_inline_seq(cell, &mut idx, None);
                    let al = align_style(i);
                    rsx! {
                        td { style: "border:1px solid {theme::BORDER};padding:6px 10px;{al}", {nodes.into_iter()} }
                    }
                })
                .collect();
            rsx! { tr { {cells.into_iter()} } }
        })
        .collect();
    rsx! {
        div {
            style: "overflow-x:auto;margin:8px 0;",
            table {
                style: "border-collapse:collapse;width:100%;font-size:12.5px;",
                thead { tr { {header_cells.into_iter()} } }
                tbody { {body_rows.into_iter()} }
            }
        }
    }
}

fn collect_table_cells<'a, I: Iterator<Item = Event<'a>>>(
    iter: &mut I,
    row_stop: TagEnd,
) -> Vec<Vec<Event<'a>>> {
    let mut cells = Vec::new();
    while let Some(event) = iter.next() {
        match event {
            Event::Start(Tag::TableCell) => cells.push(collect_inline_run(iter, TagEnd::TableCell)),
            Event::End(e) if e == row_stop => break,
            _ => {}
        }
    }
    cells
}

/// 收集一段行内事件直到其收尾 tag——供 `Paragraph`/`Heading`/`TableCell`
/// 这些"只包含行内内容,不会嵌套块级内容"的容器复用。
fn collect_inline_run<'a, I: Iterator<Item = Event<'a>>>(
    iter: &mut I,
    stop: TagEnd,
) -> Vec<Event<'a>> {
    let mut out = Vec::new();
    for event in iter.by_ref() {
        if let Event::End(e) = &event {
            if *e == stop {
                break;
            }
        }
        out.push(event);
    }
    out
}

fn drain_until<'a, I: Iterator<Item = Event<'a>>>(iter: &mut I, stop: TagEnd) {
    for event in iter.by_ref() {
        if let Event::End(e) = event {
            if e == stop {
                break;
            }
        }
    }
}

// ── 行内:递归下降(粗体/斜体/删除线/链接可以互相嵌套)────────────────────

fn render_inline_seq<'a>(
    events: &[Event<'a>],
    idx: &mut usize,
    stop: Option<TagEnd>,
) -> Vec<Element> {
    let mut out = Vec::new();
    while *idx < events.len() {
        if let Event::End(e) = &events[*idx] {
            if Some(*e) == stop {
                *idx += 1;
                return out;
            }
            // 不匹配当前 stop 的收尾(理论上不会命中)——跳过,不要死循环。
            *idx += 1;
            continue;
        }
        let event = events[*idx].clone();
        *idx += 1;
        match event {
            Event::Text(t) => out.push(rsx! { "{t}" }),
            Event::Code(t) => out.push(rsx! {
                code {
                    style: "font-family:{theme::MONO};font-size:.92em;color:{theme::CLAY};background:#F2E4DD;padding:1px 5px;border-radius:4px;",
                    "{t}"
                }
            }),
            Event::SoftBreak => out.push(rsx! { " " }),
            Event::HardBreak => out.push(rsx! { br {} }),
            Event::TaskListMarker(checked) => {
                let mark = if checked { "☑" } else { "☐" };
                out.push(rsx! { span { style: "margin-right:6px;", "{mark}" } });
            }
            Event::Start(Tag::Emphasis) => {
                let inner = render_inline_seq(events, idx, Some(TagEnd::Emphasis));
                out.push(rsx! { em { style: "font-style:italic;", {inner.into_iter()} } });
            }
            Event::Start(Tag::Strong) => {
                let inner = render_inline_seq(events, idx, Some(TagEnd::Strong));
                out.push(rsx! { strong { style: "font-weight:700;color:{theme::INK};", {inner.into_iter()} } });
            }
            Event::Start(Tag::Strikethrough) => {
                let inner = render_inline_seq(events, idx, Some(TagEnd::Strikethrough));
                out.push(rsx! { s { style: "color:{theme::INK_3};", {inner.into_iter()} } });
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let inner = render_inline_seq(events, idx, Some(TagEnd::Link));
                out.push(render_link_node(dest_url.to_string(), inner));
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let inner = render_inline_seq(events, idx, Some(TagEnd::Image));
                let _ = dest_url; // 不真实拉取外部图片(wry 无地址栏,加载失败也没法排障)——只留 alt 文案。
                out.push(rsx! {
                    span { style: "color:{theme::INK_3};font-size:.9em;", "🖼 " {inner.into_iter()} }
                });
            }
            _ => {
                // 未开启对应 Options 的语法(原始 HTML/脚注/数学等实际不会
                // 出现在事件流里)——静默跳过,防御性兜底,不渲染乱码。
            }
        }
    }
    out
}

/// 链接的取舍见模块顶部文档:只有 `http(s)://` 开头的真实 URL 可点,点击
/// 拉起系统默认浏览器,不改变 wry WebView 自身的导航。
fn render_link_node(url: String, children: Vec<Element>) -> Element {
    let openable = url.starts_with("http://") || url.starts_with("https://");
    if openable {
        let href = url.clone();
        rsx! {
            span {
                title: "{url}",
                style: "color:{theme::CLAY};text-decoration:underline;text-decoration-style:dotted;cursor:pointer;",
                onclick: move |_| open_external(&href),
                {children.into_iter()}
            }
        }
    } else {
        rsx! {
            span {
                title: "{url}",
                style: "color:{theme::CLAY};text-decoration:underline;text-decoration-style:dotted;",
                {children.into_iter()}
            }
        }
    }
}

/// 拉起系统默认浏览器打开外部链接——不让 wry 的 WebView 自己导航(会把整个
/// app 窗口带离,没有地址栏/后退键回不来)。`spawn()` 不等待、不阻塞 UI
/// 线程;打开失败(没有默认浏览器等)静默丢弃——这是一个锦上添花的便捷
/// 操作,不是需要往 `Command` 总线报错的业务动作。
fn open_external(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}
