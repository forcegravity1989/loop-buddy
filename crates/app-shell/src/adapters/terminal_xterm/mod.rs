//! 适配模块 · 内嵌终端(xterm.js + PTY 字节桥)。
//!
//! 从 `crates/app-desktop/src/screens/op/terminal_widget.rs` 整块搬过来,只换
//! 了两处接线:字节从 [`crate::bridge::Bridge`] 的 pty 通道来,键盘与尺寸经
//! [`crate::bridge::Req::Cmd`] 发回内核。渲染那部分是 V3 试用期一个坑一个坑
//! 填出来的,除了下面这一件之外一行没改,见本目录 README。
//!
//! 搬过来之后自己加的一件:**复制/粘贴在终端里自己接管**(设计 md §7.1 要求
//! 「内容和真终端一样能选中复制」)。原因见 [`TERM_PRE_HANDLER_JS`] 里那段
//! 注释 —— 这个壳没有 macOS 原生菜单,Cmd+C 到不了网页。
//!
//! 借了什么、没借什么见 `README.md`。

use crate::bridge::Bridge;
use bw_v4::command::Command;
use bw_v4::model::ConversationId;
use dioxus::prelude::*;
use std::time::Duration;

/// xterm 资产随二进制走,不联网。CDN 拉不下来的那一次,用户看到的是一整块
/// 空白 —— 那之后就一直是内嵌的。
const XTERM_JS: &str = include_str!("../../../../app-desktop/public/xterm.min.js");
const XTERM_CSS: &str = include_str!("../../../../app-desktop/public/xterm.css");
const FIT_ADDON_JS: &str = include_str!("../../../../app-desktop/public/xterm-addon-fit.min.js");

/// Pre-handler write/drain keyed by conversation id. Must run BEFORE any
/// bytes arrive so the pre-handler buffer (orca §2.4) catches early output.
/// Map shape: `window.__bw_term_sessions[id] = { term, fit, ready, buffer,
/// input[], resize }` — 修掉旧全局 `window.__bw_term` 单例(设计 md §7.3)。
const TERM_PRE_HANDLER_JS: &str = r#"
window.__bw_term_sessions = window.__bw_term_sessions || {};
window.__bw_term_ensure = function(id) {
    var s = window.__bw_term_sessions;
    if (!s[id]) s[id] = { term: null, fit: null, ready: false, buffer: '', input: [], resize: null };
    return s[id];
};
window.__bw_term_write = function(id, text) {
    var sess = window.__bw_term_ensure(id);
    if (!sess.ready || !sess.term) {
        sess.buffer += text;
        return;
    }
    try { sess.term.write(text); } catch(e) {}
};
// One call drains both queues for one conversation: Rust polls on a timer,
// and every `document::eval` is a full IPC round trip.
window.__bw_term_drain = function(id) {
    var sess = window.__bw_term_sessions && window.__bw_term_sessions[id];
    if (!sess) return null;
    var input = null;
    if (sess.input && sess.input.length > 0) {
        input = sess.input.join('');
        sess.input = [];
    }
    var resize = sess.resize || null;
    sess.resize = null;
    if (input === null && resize === null) return null;
    return { input: input, resize: resize, ready: !!sess.ready };
};
// Bug2: display:none → 0×0 open 假成功;非焦点离屏保尺寸;焦点回来 re-home+fit+refresh。
window.__bw_term_refocus = function(id) {
    var sess = window.__bw_term_sessions && window.__bw_term_sessions[id];
    var div = document.getElementById('__bw_terminal_' + id);
    if (!sess || !sess.term || !div) return false;
    if (sess.term.element && !div.contains(sess.term.element)) {
        try { div.appendChild(sess.term.element); } catch (e) {}
    }
    var tries = 0;
    var go = function() {
        tries++;
        var el = sess.term.element || div;
        var w = el.clientWidth || 0;
        var h = el.clientHeight || 0;
        if (w > 0 && h > 0) {
            try { if (sess.fit) sess.fit.fit(); } catch (e) {}
            try { sess.term.refresh(0, Math.max(0, sess.term.rows - 1)); } catch (e) {}
            try { sess.term.focus(); } catch (e) {}
            sess.resize = { cols: sess.term.cols, rows: sess.term.rows };
            return;
        }
        if (tries < 20) requestAnimationFrame(go);
    };
    requestAnimationFrame(function() { requestAnimationFrame(go); });
    return true;
};

// ── 剪贴板:终端自己接管,不靠宿主的原生菜单 ──────────────────────────
// macOS 上 Cmd+C 走的是这条链:应用的「编辑」菜单里那一项带 ⌘C 快捷键 →
// 系统把 copy: 动作沿响应链发给 WebView → WebView 给网页发一个 copy 事件 →
// xterm 自己挂在 term.element 上的 copy 监听器把选区文字塞进剪贴板。
// 这个壳没有装原生菜单,链在第一步就断了 —— 选中是好的,复制永远出不来。
// 装菜单是全应用范围的改动,而且 Windows 上根本用不着,所以改成在终端自己
// 身上认按键、自己写剪贴板。粘贴同理(Cmd+V 也到不了网页)。
window.__bw_term_status = function(id, text) {
    try { console.log('[BW] 终端剪贴板 ' + id + ':' + text); } catch (e) {}
    var el = document.getElementById('__bw_term_status_' + id);
    if (!el) return;
    el.textContent = text;
    if (el.__bw_clear) clearTimeout(el.__bw_clear);
    el.__bw_clear = setTimeout(function() {
        if (el.textContent === text) el.textContent = '';
    }, 4000);
};
// 先走 navigator.clipboard.writeText。这个壳的页面地址是 dioxus://index.html,
// WebKit 不把自定义协议当安全上下文,navigator.clipboard 很可能压根不存在;
// 那时退到临时 textarea + execCommand('copy') —— 它只要求一次用户手势,不要
// 求安全上下文。两条都不成就如实报失败,绝不假装复制成功了。
window.__bw_clip_write = function(text) {
    var viaTextarea = function() {
        var active = document.activeElement;
        var ta = document.createElement('textarea');
        ta.value = text;
        ta.setAttribute('readonly', '');
        ta.style.cssText = 'position:fixed;top:0;left:-9999px;opacity:0;';
        document.body.appendChild(ta);
        var ok = false;
        try {
            ta.select();
            ta.setSelectionRange(0, text.length);
            ok = document.execCommand('copy');
        } catch (e) { ok = false; }
        try { document.body.removeChild(ta); } catch (e) {}
        try { if (active && active.focus) active.focus(); } catch (e) {}
        return ok;
    };
    return new Promise(function(resolve, reject) {
        var fallback = function() {
            if (viaTextarea()) resolve();
            else reject(new Error('这个窗口不让网页写剪贴板'));
        };
        var nav = window.navigator;
        if (nav && nav.clipboard && nav.clipboard.writeText) {
            nav.clipboard.writeText(text).then(function() { resolve(); }, fallback);
            return;
        }
        fallback();
    });
};
// allow_whole=true 是标题栏「复制」按钮走的:没选中就把整段(含回滚)拿走。
// 键盘那条永远只复制选中 —— 跟真终端一样。
window.__bw_term_copy = function(id, allow_whole) {
    var sess = window.__bw_term_sessions && window.__bw_term_sessions[id];
    if (!sess || !sess.term) return;
    var term = sess.term;
    var refocus = function() { try { term.focus(); } catch (e) {} };
    var text = '';
    try { text = term.getSelection() || ''; } catch (e) { text = ''; }
    var what = '选中';
    if (!text && allow_whole) {
        what = '整段';
        try {
            var buf = term.buffer.active;
            var lines = [];
            for (var i = 0; i < buf.length; i++) {
                var line = buf.getLine(i);
                lines.push(line ? line.translateToString(true) : '');
            }
            text = lines.join('\n').replace(/\s+$/, '');
        } catch (e) { text = ''; }
    }
    if (!text) {
        window.__bw_term_status(id, '没有可复制的内容');
        refocus();
        return;
    }
    var n = text.length;
    window.__bw_clip_write(text).then(function() {
        window.__bw_term_status(id, '已复制' + what + ' ' + n + ' 字');
        refocus();
    }, function(e) {
        window.__bw_term_status(id, '复制失败:' + ((e && e.message) || '未知原因'));
        refocus();
    });
};
// 粘贴只有 navigator.clipboard.readText 一条路:execCommand('paste') 在
// WebKit 里对网页是禁用的,不存在第二条。读不到就如实说读不到。
// 文字交给 term.paste() 而不是直接推给 PTY —— 它负责括号粘贴模式的包裹和
// 换行归一,自己拼会把多行粘贴变成一串回车。
window.__bw_term_paste = function(id) {
    var sess = window.__bw_term_sessions && window.__bw_term_sessions[id];
    if (!sess || !sess.term) return;
    var nav = window.navigator;
    if (!(nav && nav.clipboard && nav.clipboard.readText)) {
        window.__bw_term_status(id, '粘贴用不了:这个窗口不让网页读剪贴板');
        return;
    }
    nav.clipboard.readText().then(function(text) {
        if (!text) {
            window.__bw_term_status(id, '剪贴板是空的');
            return;
        }
        try {
            sess.term.paste(text);
        } catch (e) {
            window.__bw_term_status(id, '粘贴失败:' + ((e && e.message) || '未知原因'));
            return;
        }
        window.__bw_term_status(id, '已粘贴 ' + text.length + ' 字');
    }, function(e) {
        window.__bw_term_status(id, '粘贴失败:' + ((e && e.message) || '拿不到剪贴板'));
    });
};
// 标题栏「复制」按钮的点击**必须在 JS 里当场处理完**:走 Rust 的 onclick 再
// document::eval 回来是一整趟异步 IPC,等 JS 真跑起来时浏览器认的那一下「用户
// 手势」早过期了,execCommand('copy') 会被直接拒掉。
// 用事件委托挂在 document 上挂一次,按钮被重新渲染也不会变成哑巴。
if (!window.__bw_term_click_bound) {
    window.__bw_term_click_bound = true;
    document.addEventListener('click', function(e) {
        var prefix = '__bw_term_copybtn_';
        var node = e.target;
        while (node && node !== document) {
            if (node.id && node.id.indexOf(prefix) === 0) {
                window.__bw_term_copy(node.id.slice(prefix.length), true);
                return;
            }
            node = node.parentNode;
        }
    });
}
"#;

/// Build the per-conversation init IIFE. `id` is the conversation uuid
/// string. Re-attach path: panel switch drops the Dioxus div but JS Map +
/// PTY stay; remount re-homes `term.element` and re-fits (尺寸同步链).
fn term_init_js(conversation_id: &str) -> String {
    let id_json = serde_json::to_string(conversation_id).unwrap_or_else(|_| "\"\"".into());
    format!(
        r#"
return (async function(id) {{
    var div = document.getElementById('__bw_terminal_' + id);
    if (!div) return {{ ok: false, reason: 'div not found' }};
    var sess = window.__bw_term_ensure(id);

    var push = function(data) {{
        sess.input = sess.input || [];
        sess.input.push(data);
    }};
    var CTRL_A = 'a'.charCodeAt(0);
    var KEYS = {{
        Enter: '\r', Backspace: '\x7f', Escape: '\x1b', Tab: '\t',
        Delete: '\x1b[3~', Insert: '\x1b[2~',
        ArrowUp: '\x1b[A', ArrowDown: '\x1b[B',
        ArrowRight: '\x1b[C', ArrowLeft: '\x1b[D',
        Home: '\x1b[H', End: '\x1b[F',
        PageUp: '\x1b[5~', PageDown: '\x1b[6~',
    }};
    // 复制/粘贴的组合键。两套都认,不去嗅平台:macOS 是 Cmd+C / Cmd+V,
    // Windows·Linux 终端的惯例是 Ctrl+Shift+C / Ctrl+Shift+V。
    //
    // **不带 Shift 的 Ctrl+C 一定不在这里面** —— 那是「中断正在跑的命令」,
    // 是终端最基本的能力,任何时候都原样送到 PTY 去,不管有没有选中。
    var isCopyChord = function(e) {{
        if (e.key !== 'c' && e.key !== 'C') return false;
        if (e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) return true;
        return e.ctrlKey && e.shiftKey && !e.metaKey && !e.altKey;
    }};
    var isPasteChord = function(e) {{
        if (e.key !== 'v' && e.key !== 'V') return false;
        if (e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey) return true;
        return e.ctrlKey && e.shiftKey && !e.metaKey && !e.altKey;
    }};
    var keyBytes = function(e) {{
        // Cmd(macOS)/ Super 组合从来不是终端输入。以前这里会漏到最后一行,
        // 把 Cmd+C 当成普通的 'c' 推给 PTY,还顺手 preventDefault 掉。
        if (e.metaKey) return null;
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
        // 捕获阶段挂在外层 div 上,先于 xterm 挂在内层 textarea 上的那个
        // keydown 跑。只有真吃下这个键时才 stopPropagation —— 捕获阶段停掉
        // 之后,目标阶段和冒泡阶段(包括本 div 上那个把按键推给 PTY 的
        // 监听器)都不会再跑。重挂时 Dioxus 给的是新 div,标记防重复注册。
        if (!div.__bw_clip_wired) {{
            div.__bw_clip_wired = true;
            div.addEventListener('keydown', function(e) {{
                if (isCopyChord(e)) {{
                    var has = false;
                    try {{ has = !!(sess.term && sess.term.hasSelection()); }} catch (x) {{}}
                    // 没选中就完全不碰这个键,让它按原路走完。
                    if (!has) return;
                    e.preventDefault();
                    e.stopPropagation();
                    if (window.__bw_term_copy) window.__bw_term_copy(id, false);
                    return;
                }}
                if (isPasteChord(e)) {{
                    e.preventDefault();
                    e.stopPropagation();
                    if (window.__bw_term_paste) window.__bw_term_paste(id);
                }}
            }}, true);
        }}
        div.addEventListener('keydown', function(e) {{
            if (textarea && e.target === textarea) return;
            var data = keyBytes(e);
            if (data === null) return;
            push(data);
            e.preventDefault();
        }});
    }};

    // Re-attach: existing term for this conversation survives Dioxus remount.
    if (sess.term) {{
        if (sess.term.element && !div.contains(sess.term.element)) {{
            div.appendChild(sess.term.element);
        }}
        try {{
            if (sess.fit && div.clientWidth > 0 && div.clientHeight > 0) {{
                sess.fit.fit();
                sess.term.refresh(0, Math.max(0, sess.term.rows - 1));
                sess.resize = {{ cols: sess.term.cols, rows: sess.term.rows }};
            }}
        }} catch (e) {{}}
        wireDiv(div, sess.term);
        return {{ ok: true, reason: 'already-initialized', w: div.clientWidth, h: div.clientHeight }};
    }}

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
        sess.resize = {{ cols: size.cols, rows: size.rows }};
    }});

    // V1-TermRefactor review · 设计 md §7.6:卡片重新显示 / 窗口缩放 /
    // 侧栏变化 / 字体就绪都 re-fit。fit() 触发 onResize → stash
    // sess.resize → Rust 30ms drain 发 TerminalResize(带 id)。观察
    // term.element(跨 remount 稳定);display:none 下尺寸为 0 跳过,避免
    // FitAddon 在零宽框上抛错。仅新建分支挂一次,re-attach 不重复。
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
    sess.ready = true;
    if (sess.buffer) {{
        term.write(sess.buffer);
        sess.buffer = '';
    }}
    // Push fit size immediately so Rust can resize PTY off 80×24.
    sess.resize = {{ cols: term.cols, rows: term.rows }};

    return {{ ok: true }};
}})({id_json})
"#
    )
}

/// Take the longest valid UTF-8 prefix out of `buf`, leaving whatever
/// trailing bytes form an incomplete character behind for the next batch.
///
/// PTY output is a byte stream cut into ~100ms batches at arbitrary offsets;
/// a 3-byte CJK character routinely straddles two of them. Decoding a batch
/// in isolation (`from_utf8_lossy`) replaces both halves with U+FFFD.
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

/// 按会话 id 挂的内嵌终端。`focused=false` 时移到屏外但**仍然收字节** ——
/// 切走再切回来,中间 agent 说的话不能丢。
#[component]
pub fn TerminalWidget(conversation_id: ConversationId, focused: bool, bridge: Bridge) -> Element {
    let k = bridge;
    let cid_str = conversation_id.uuid().to_string();
    let div_id = format!("__bw_terminal_{cid_str}");
    // 剪贴板那条回执写在这个 span 里(JS 直接改 textContent):复制成功说复制
    // 了多少字,失败说为什么失败 —— 不静默,也不假装成功。
    let status_id = format!("__bw_term_status_{cid_str}");
    // 「复制」按钮**故意不挂 Rust 的 onclick**,点击由 JS 侧按 id 认领。理由见
    // `TERM_PRE_HANDLER_JS` 末尾那段:绕一趟 IPC 回来,用户手势就过期了。
    let copy_btn_id = format!("__bw_term_copybtn_{cid_str}");

    use_future(move || {
        let k = k.clone();
        let cid = conversation_id;
        let cid_str = cid.uuid().to_string();
        async move {
            let debug = std::env::var("BW_PTY_DEBUG").is_ok_and(|v| v != "0");
            let cid_json = serde_json::to_string(&cid_str).unwrap_or_else(|_| "\"\"".into());

            let _ = document::eval(TERM_PRE_HANDLER_JS).await;
            let _ = document::eval(XTERM_JS).await;
            let _ = document::eval(FIT_ADDON_JS).await;
            let _ = document::eval(&format!(
                "if(!document.getElementById('__bw_xterm_css')){{var __s=document.createElement('style');__s.id='__bw_xterm_css';__s.textContent={};document.head.appendChild(__s)}}",
                serde_json::to_string(XTERM_CSS).unwrap_or_else(|_| String::new())
            ))
            .await;
            let init = document::eval(&term_init_js(&cid_str)).await;
            if debug {
                eprintln!("[pty] terminal init {cid_str}: {init:?}");
            }

            // 订阅在这里开始 —— 这一刻之前的字节这个终端拿不到,但 agent 的输出
            // 是开工那一下才开始产生的,而挂件比开工先在。
            let mut pty_rx = k.pty.subscribe();
            let mut carry: Vec<u8> = Vec::new();
            loop {
                tokio::select! {
                    result = pty_rx.recv() => {
                        let batches = match result {
                            Ok(b) => b,
                            // 队列被写满冲掉了 n 批 —— 真丢了就说丢了,不装作
                            // 没发生。终端里也留一行,免得人以为 agent 沉默了。
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                eprintln!("[BW_WARN] 终端 {cid_str} 落后,丢了 {n} 批输出");
                                let note = format!("\r\n[BW] 界面跟不上,丢了 {n} 批输出\r\n");
                                let escaped = serde_json::to_string(&note)
                                    .unwrap_or_else(|_| "\"\"".into());
                                let _ = document::eval(&format!(
                                    "window.__bw_term_write({cid_json}, {escaped})"
                                )).await;
                                carry.clear();
                                continue;
                            }
                            // 发送端没了 = 内核线程结束,这个挂件也没事可做了。
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        };
                        for (batch_cid, bytes) in batches {
                            if batch_cid != cid || bytes.is_empty() {
                                continue;
                            }
                            carry.extend_from_slice(&bytes);
                            let text = take_utf8_prefix(&mut carry);
                            if text.is_empty() {
                                continue;
                            }
                            let escaped = serde_json::to_string(&text)
                                .unwrap_or_else(|_| "\"\"".into());
                            let script = format!(
                                "window.__bw_term_write({cid_json}, {escaped})"
                            );
                            let _ = document::eval(&script).await;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_millis(30)) => {
                        let drain_script = format!(
                            "return window.__bw_term_drain ? window.__bw_term_drain({cid_json}) : null"
                        );
                        let Ok(v) = document::eval(&drain_script).await else { continue };
                        let Some(obj) = v.as_object() else { continue };
                        if let Some(input) = obj.get("input").and_then(|i| i.as_str()) {
                            if !input.is_empty() {
                                if debug {
                                    eprintln!("[pty] stdin {} bytes: {input:?}", input.len());
                                }
                                k.cmd(Command::TerminalInput {
                                    conversation_id: cid,
                                    bytes: input.as_bytes().to_vec(),
                                });
                            }
                        }
                        if let Some(r) = obj.get("resize").and_then(|r| r.as_object()) {
                            let cols = r.get("cols").and_then(|c| c.as_u64()).unwrap_or(80) as u16;
                            let rows = r.get("rows").and_then(|r| r.as_u64()).unwrap_or(24) as u16;
                            k.cmd(Command::TerminalResize {
                                conversation_id: cid,
                                cols,
                                rows,
                            });
                        }
                    }
                }
            }
        }
    });

    // 不在焦点上的终端**绝不能 display:none**:跨屏重挂时 FitAddon 会以 0×0
    // 打开,之后再 refocus 返回 ok 但画布一片空白(2026-08-11 的日志里
    // fitted 路径 Ok(true),人还是看不见 CLI)。挪到屏外的固定盒子保住真实
    // 宽高,open/fit 才是健康的;拿到焦点时换类名 + 重挂(key 里带 f/h)到
    // 一个会长大的宿主上。收字节那条循环对所有活着的会话一直挂着。
    //
    // 焦点态用的两个类名(`sess-col sess-midbody`)是**会话屏中栏下半格**在
    // `.content.session-mode` 那套网格里的位置。终端挂在 `.content` 上而不是
    // 挂在会话屏里 —— 挂在屏里会被切面板连屏卸载,字节就真丢了。
    let wrap = if focused {
        "sess-col sess-midbody"
    } else {
        "term-offscreen"
    };

    // Dioxus 0.7: subscribe focused with use_reactive (bare bool prop is not
    // reactive — focus-only updates used to skip this effect entirely).
    use_effect(use_reactive((&focused, &cid_str), |(focused, cid_str)| {
        if !focused {
            return;
        }
        let cid_json = serde_json::to_string(&cid_str).unwrap_or_else(|_| "\"\"".into());
        spawn(async move {
            // Two frames for CSS/layout after remount, then refit.
            tokio::time::sleep(Duration::from_millis(48)).await;
            let script = format!(
                "return window.__bw_term_refocus ? window.__bw_term_refocus({cid_json}) : false"
            );
            let _ = document::eval(&script).await;
        });
    }));

    rsx! {
        div {
            class: "{wrap} terminal",
            style: "padding:0;",
            div { class: "term-titlebar",
                span { "● 内嵌终端" }
                span { style: "opacity:.5;", "claude 交互式会话" }
                span { class: "spacer" }
                span { id: "{status_id}", style: "opacity:.75;" }
                button {
                    id: "{copy_btn_id}",
                    class: "copybtn",
                    title: "复制选中的内容;没选中就复制整段。键盘 Cmd+C(或 Ctrl+Shift+C)只复制选中。拖不出选区多半是 agent 的界面开了鼠标上报,按住 Option 再拖",
                    "复制"
                }
            }
            div {
                id: "{div_id}",
                class: "xterm-host",
                style: "background:#1c1b19;",
            }
        }
    }
}
