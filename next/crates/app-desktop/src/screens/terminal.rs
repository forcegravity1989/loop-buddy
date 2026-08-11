//! 嵌入终端(next 切片 5.5,design-s5-hexpanel.md §5.3 搬运清单)——移植自
//! v1 `app-desktop/src/screens/op.rs` 文件末尾的 `TerminalWidget`(xterm.js
//! 渲染 + 键盘/尺寸桥接)。前端资源(`XTERM_JS`/`XTERM_CSS`/`FIT_ADDON_JS`)
//! 随包内联、零改写(见 `public/` 目录);key 编码表(`KEYS`/`keyBytes`)、
//! xterm 生命周期语句(创建/`onData`/`onResize`/`ResizeObserver`/re-attach
//! 判断)、`take_utf8_prefix`(PTY 字节跨批 UTF-8 边界处理)全部照抄 v1 原
//! 文,不做「顺手」改动。
//!
//! **一处必要的接口适配,不是重写**(v1 §5.3 表「隐藏不等于停收」旁边那
//! 条主控裁决的直接后果):v1 用一条 30ms 睡眠轮询(tokio 定时器)去抽
//! JS 端缓冲的键盘/resize 事件(`op.rs` 原文 `__bw_term_drain`/
//! `pty_ticker`),那正是 `scripts/guard-shell-no-clock.sh` 禁的定时器构
//! 造——「壳零时钟不动摇」不开这个口子。改法:JS 端从"攒进缓冲、等 Rust
//! 来 poll"换成"事件发生就直接 `dioxus.send(...)` 推给 Rust"(Dioxus 0.7
//! `document::eval` 原生支持的双向 channel——`Eval::recv()` 是真异步
//! `poll_fn`,底层是消息
//! 队列,不是定时器,见其源码 `dioxus-document-0.7.9/src/eval.rs`);Rust
//! 侧一条 `tokio::select!` 同时等「JS 推来的键盘/resize 事件」与「PTY 输
//! 出字节」两路,两路都是**事件到达才唤醒**,全程没有一次 `sleep`/
//! `interval`。语义不变(键位表、resize 触发时机、UTF-8 跨批处理全部照
//! 抄),只是传输机制从"拉"换成"推"——这同时也让 v1 原有的
//! `sess.buffer`/`sess.ready`(等 Rust 来 poll 之前的字符串缓冲)不再需
//! 要:输出改走这条 eval 自己的 `recv` 循环,写之前已经确认 xterm 就绪
//! (`ready` 握手),没有"写得比初始化早"的竞态需要拿缓冲区兜底。
//!
//! **字节从哪来**:壳不拉取,只订阅——`bw_app::run::RunManager::
//! terminal_feed`(design-s5-hexpanel.md §5.3/主控裁决 7 落地,见
//! `bw-app/src/run/manager.rs` 与 `bw-engine/src/agentcli/pump.rs` 两处文
//! 档)。这里没有任何轮询,`use_future` 起的 `tokio::select!` 两条臂都是
//! 事件驱动。
//!
//! **每卡一个终端,不是全局单例**——v1 主线旧壳那份 `window.__bw_term`
//! 单例是错的源(§5.3 表「别搬错主线单例那份」),这里没有搬它;每个
//! [`TerminalWidget`] 实例按 `run.id` 维护自己的 xterm 会话/DOM 节点
//! (`window.__bw_term_sessions[run_id]`)。

use bw_app::run::TermInput;
use bw_store::RunRow;
use dioxus::prelude::*;
use tokio::sync::broadcast;

use crate::kernel::Kernel;
use crate::theme;

/// 随包内联的三个前端资源(约 291KB,design §5.3「随包内联,不外链——外
/// 链失败会让会话彻底没有终端,这是 v1 踩过并明确修掉的坑」)。文件内容
/// 零改写,`next/crates/app-desktop/public/` 三个文件逐字节复制自
/// `origin/v1:crates/app-desktop/public/`。
const XTERM_JS: &str = include_str!("../../public/xterm.min.js");
const XTERM_CSS: &str = include_str!("../../public/xterm.css");
const FIT_ADDON_JS: &str = include_str!("../../public/xterm-addon-fit.min.js");

/// 会话表 + 重挂载(remount)判断——v1 同名逻辑原样保留(卡片可能因为
/// Dioxus 树 diff 重新创建这个 div,已经开的 xterm/PTY 不该跟着重开)。
/// 键改成 `run.id` 的 uuid 字符串(next 没有客户端可见的
/// `ConversationId`——那是引擎侧的内部身份,壳全程只认 `RunId`)。**不再
/// 需要** v1 那份 `write`/`drain`/`buffer`/`ready` 字段——见本文件顶部模
/// 块文档「一处必要的接口适配」。
const TERM_PRE_HANDLER_JS: &str = r#"
window.__bw_term_sessions = window.__bw_term_sessions || {};
window.__bw_term_ensure = function(id) {
    var s = window.__bw_term_sessions;
    if (!s[id]) s[id] = { term: null, fit: null };
    return s[id];
};
"#;

/// 装配单个会话的初始化 + 事件循环脚本。`id` 是 `run.id` 的 uuid 字符
/// 串。key 编码表/xterm 生命周期语句照抄 v1 `term_init_js`;末尾的
/// `while(true) { await dioxus.recv() }` 是本片新写的输出循环(替代 v1
/// 的 `window.__bw_term_write`/`__bw_term_drain` 一对轮询函数)。
fn term_init_js(run_id: &str) -> String {
    let id_json = serde_json::to_string(run_id).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"
return (async function(id) {{
    var div = document.getElementById('__bw_terminal_' + id);
    if (!div) return {{ ok: false, reason: 'div not found' }};
    var sess = window.__bw_term_ensure(id);

    // 键盘输入/resize 直接推给 Rust(`dioxus.send`),不再攒进缓冲等 Rust
    // 来 poll——v1 原文 `push`/`KEYS`/`keyBytes`/`wireDiv` 四段逻辑本身零
    // 改写,只有 `push` 的落点从"塞进 sess.input 数组"变成"直接 send"。
    var push = function(data) {{ dioxus.send({{ type: 'input', data: data }}); }};
    var CTRL_A = 'a'.charCodeAt(0);
    var KEYS = {{
        Enter: '\r', Backspace: '\x7f', Escape: '\x1b', Tab: '\t',
        Delete: '\x1b[3~', Insert: '\x1b[2~',
        ArrowUp: '\x1b[A', ArrowDown: '\x1b[B',
        ArrowRight: '\x1b[C', ArrowLeft: '\x1b[D',
        Home: '\x1b[H', End: '\x1b[F',
        PageUp: '\x1b[5~', PageDown: '\x1b[6~',
    }};
    var keyBytes = function(e) {{
        if (e.ctrlKey && e.key.length === 1) {{
            var c = e.key.toLowerCase().charCodeAt(0);
            if (c >= CTRL_A && c < CTRL_A + 26) return String.fromCharCode(c - CTRL_A + 1);
            return null;
        }}
        if (KEYS[e.key]) return KEYS[e.key];
        if (e.key.length !== 1) return null;
        return e.altKey ? '\x1b' + e.key : e.key;
    }};
    var wireDiv = function(div, term) {{
        var textarea = div.querySelector('.xterm-helper-textarea');
        div.tabIndex = 0;
        var focusTerm = function() {{
            term.focus();
            if (!textarea || document.activeElement !== textarea) div.focus();
        }};
        div.addEventListener('click', focusTerm);
        focusTerm();
        div.addEventListener('keydown', function(e) {{
            if (textarea && e.target === textarea) return;
            var data = keyBytes(e);
            if (data === null) return;
            push(data);
            e.preventDefault();
        }});
    }};

    // 重挂载:这个会话的 xterm 已经在(`window.__bw_term_sessions` 跨 div
    // 重建存活),把它的 DOM 节点搬回来就好,不重新 new 一个 Terminal。
    if (sess.term) {{
        if (sess.term.element && !div.contains(sess.term.element)) {{
            div.appendChild(sess.term.element);
            if (sess.fit) sess.fit.fit();
            wireDiv(div, sess.term);
        }}
    }} else {{
        if (!window.Terminal || !window.FitAddon) {{
            return {{ ok: false, reason: 'xterm bundles not loaded' }};
        }}
        var term = new Terminal({{
            fontFamily: 'JetBrains Mono, Consolas, monospace',
            fontSize: 13,
            cols: 80,
            rows: 24,
            cursorBlink: true,
            theme: {{ background: '#1e1e2e', foreground: '#cdd6f4' }},
        }});
        var fitAddon = new FitAddon.FitAddon();
        term.loadAddon(fitAddon);
        term.open(div);
        fitAddon.fit();

        term.onData(push);
        wireDiv(div, term);
        term.onResize(function(size) {{
            dioxus.send({{ type: 'resize', cols: size.cols, rows: size.rows }});
        }});

        var refit = function() {{
            try {{
                if (term.element && term.element.clientWidth > 0 && term.element.clientHeight > 0) {{
                    fitAddon.fit();
                }}
            }} catch(e) {{}}
        }};
        if (window.ResizeObserver) {{
            new ResizeObserver(refit).observe(term.element || div);
        }}
        window.addEventListener('resize', refit);

        sess.term = term;
        sess.fit = fitAddon;
        // 立刻把首次 fit 到的尺寸推给 Rust,PTY 才能脱离 80×24 默认值。
        dioxus.send({{ type: 'resize', cols: term.cols, rows: term.rows }});
    }}

    // 就绪握手——Rust 侧收到这条之后才开始转发 PTY 输出批次,不依赖"消
    // 息队列先进先出"这条实现细节兜底早到的写。
    dioxus.send({{ type: 'ready' }});

    // 输出循环:Rust 每收到一批 PTY 字节就 `eval.send({{type:'write',...}})`
    // 一次,这里 `await dioxus.recv()` 醒来就写进 xterm——事件驱动,不是
    // 轮询。永远不 `return`(没有需要等待的最终结果),组件卸载时这个
    // eval 句柄被 Rust 侧丢弃,底层连接关闭。
    while (true) {{
        var msg = await dioxus.recv();
        if (msg && msg.type === 'write' && sess.term) {{
            try {{ sess.term.write(msg.text); }} catch(e) {{}}
        }}
    }}
}})({id_json})
"#
    )
}

/// 从 `buf` 里取出最长的合法 UTF-8 前缀,把凑不成一个完整字符的尾巴留给
/// 下一批——**v1 原文 `take_utf8_prefix` 零改写**(PTY 输出按~字节数切
/// 成的批次,一个三字节中文字符常常正好跨在两批中间;分别对每批做
/// `from_utf8_lossy` 会把两半都替换成 U+FFFD)。
fn take_utf8_prefix(buf: &mut Vec<u8>) -> String {
    match std::str::from_utf8(buf) {
        Ok(s) => {
            let out = s.to_string();
            buf.clear();
            out
        }
        Err(e) => {
            let valid = e.valid_up_to();
            match e.error_len() {
                None => {
                    let out = String::from_utf8_lossy(&buf[..valid]).into_owned();
                    buf.drain(..valid);
                    out
                }
                Some(_) => {
                    let out = String::from_utf8_lossy(buf).into_owned();
                    buf.clear();
                    out
                }
            }
        }
    }
}

/// JS → Rust 的三种推送(见 `term_init_js` 里 `dioxus.send` 的三处调
/// 用)。内部标签式枚举,字段名与 JS 侧对象键一一对应。
#[derive(serde::Deserialize, Debug)]
#[serde(tag = "type", rename_all = "lowercase")]
enum JsEvent {
    Ready,
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

/// 一张运行卡的嵌入终端。**内部持有自己的展开/收起与「找不找得到活会
/// 话」两个信号**——父组件(`EscapeHatchCard`)只管 `run: RunRow { }` 传
/// 进来,不需要先问一遍「这条运行有没有终端」再决定要不要挂这个组件:
/// 没有活会话时,这个组件把自己渲染成空(逃生舱那一行原样照旧,界面上
/// 没有任何新东西冒出来),行为对今天的壳(`ConnectorRegistry::default()`
/// 是空的,没有任何一条运行真的在本进程里跑着交互式会话)是零回归的。
///
/// **隐藏不等于停收**(§5.3 表原文语义):`expanded` 只控制 CSS
/// `display:none`,`use_future` 起的接收循环与 `expanded` 无关,收起卡片
/// 不会让它停止收字节——同 v1 `focused: bool` 的既有判断,只是这里的
/// 「收起」是本片新加的展开/收起交互(v1 是"当前 tab 是不是这一个会
/// 话"),语义对应但触发方式不同。
#[component]
pub fn TerminalWidget(run: RunRow) -> Element {
    let kernel = use_context::<Kernel>();
    let run_id = run.id;
    let run_id_str = run_id.uuid().to_string();
    let div_id = format!("__bw_terminal_{run_id_str}");

    let mut available = use_signal(|| false);
    let mut expanded = use_signal(|| false);
    let mut restoring = use_signal(|| true);

    use_future(move || {
        let kernel = kernel.clone();
        let run_id_str = run_id_str.clone();
        async move {
            let Some(manager) = kernel.manager() else {
                return; // 运行管理器还没就绪——如实空转,不是这个组件的错。
            };
            let mut pty_rx = match manager.terminal_feed(run_id).await {
                Ok(Some(rx)) => rx,
                // 三档「如实没有」(不在活跃表/还没拿到票据/连接器不支持
                // 交互式字节流)统一处理:这条运行没有嵌入终端可看,组件
                // 保持 `available == false`,渲染空,逃生舱行原样照旧。
                Ok(None) => return,
                Err(e) => {
                    eprintln!(
                        "[terminal] 诊断:运行 {run_id:?} 订阅字节流失败(如实降级为无终端):{e}"
                    );
                    return;
                }
            };
            available.set(true);

            let _ = document::eval(TERM_PRE_HANDLER_JS).await;
            let _ = document::eval(XTERM_JS).await;
            let _ = document::eval(FIT_ADDON_JS).await;
            let _ = document::eval(&format!(
                "if(!document.getElementById('__bw_xterm_css')){{var __s=document.createElement('style');__s.id='__bw_xterm_css';__s.textContent={};document.head.appendChild(__s)}}",
                serde_json::to_string(XTERM_CSS).unwrap_or_else(|_| String::new())
            ))
            .await;

            let mut eval = document::eval(&term_init_js(&run_id_str));
            let mut carry: Vec<u8> = Vec::new();
            loop {
                tokio::select! {
                    js_msg = eval.recv::<JsEvent>() => {
                        match js_msg {
                            Ok(JsEvent::Ready) => {}
                            Ok(JsEvent::Input { data }) => {
                                if let Some(m) = kernel.manager() {
                                    let _ = m
                                        .terminal_input(run_id, TermInput::Bytes(data.into_bytes()))
                                        .await;
                                }
                            }
                            Ok(JsEvent::Resize { cols, rows }) => {
                                if let Some(m) = kernel.manager() {
                                    let _ = m
                                        .terminal_input(run_id, TermInput::Resize { cols, rows })
                                        .await;
                                }
                            }
                            // eval 通道断了(组件被卸载 / WebView 侧异
                            // 常)——退出这个循环,`use_future` 自然收尾。
                            Err(_) => break,
                        }
                    }
                    batch = pty_rx.recv() => {
                        match batch {
                            Ok(bytes) => {
                                carry.extend_from_slice(&bytes);
                                let text = take_utf8_prefix(&mut carry);
                                if text.is_empty() {
                                    continue;
                                }
                                if restoring() {
                                    restoring.set(false); // 首包到达即清「恢复中」。
                                }
                                let _ = eval.send(serde_json::json!({"type": "write", "text": text}));
                            }
                            // 输出环把这批之前的字节挤掉了(§12 第 4 条同
                            // 一条老实账:无人拉取时输出环按上限丢最老;
                            // 这里是"迟到的订阅方",继续收后面的就好,不
                            // 是错误)。
                            Err(broadcast::error::RecvError::Lagged(_)) => continue,
                            // 会话关了(`AgentCliConnector::close_conversation`
                            // 摘掉了 `byte_senders` 里这一条)——如实退
                            // 出,不重连、不假装还活着。
                            Err(broadcast::error::RecvError::Closed) => break,
                        }
                    }
                }
            }
        }
    });

    if !available() {
        return rsx! { Fragment {} };
    }

    let visible = expanded();
    let wrap = if visible {
        format!(
            "margin-top:10px;border:1px solid {};border-radius:8px;overflow:hidden;",
            theme::BORDER
        )
    } else {
        "display:none;".to_string()
    };

    rsx! {
        div {
            style: "margin-top:8px;",
            button {
                style: format!(
                    "border:1px solid {};border-radius:6px;padding:3px 10px;font-size:11px;background:{};color:{};",
                    theme::BORDER_DEEP, theme::CARD, theme::INK_2,
                ),
                onclick: move |_| expanded.set(!expanded()),
                if visible { "收起嵌入终端" } else { "展开嵌入终端" }
            }
            div {
                style: "{wrap}",
                div {
                    style: "background:#1e1e2e;color:#cdd6f4;font-family:JetBrains Mono,Consolas,monospace;font-size:11px;padding:4px 10px;display:flex;align-items:center;gap:6px;",
                    span { style: "opacity:0.7;", "● in-app terminal" }
                    if restoring() {
                        span { style: "opacity:0.7;", "恢复中…" }
                    }
                    span { style: "opacity:0.4;margin-left:auto;", "claude interactive session" }
                }
                div {
                    id: "{div_id}",
                    style: "min-height:320px;background:#1e1e2e;",
                }
            }
        }
    }
}
