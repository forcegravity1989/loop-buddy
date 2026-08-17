//! 内嵌终端组件(xterm.js 资产 + 初始化脚本 + PTY 字节桥接)。
//! 从 op.rs 机械拆出(2026-08-17),逻辑未改;`TerminalWidget` 由 op.rs 的 `WorkflowStage` 挂载。

use super::*;

/// Bundled xterm assets (no CDN — a failed fetch used to leave no terminal).
const XTERM_JS: &str = include_str!("../../../public/xterm.min.js");
const XTERM_CSS: &str = include_str!("../../../public/xterm.css");
const FIT_ADDON_JS: &str = include_str!("../../../public/xterm-addon-fit.min.js");

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

/// 按 conversation_id 挂的嵌入终端;focused=false 时隐藏但仍收字节。
#[component]
pub(super) fn TerminalWidget(conversation_id: ConversationId, focused: bool) -> Element {
    let k = use_context::<Kernel>();
    let cid_str = conversation_id.uuid().to_string();
    let div_id = format!("__bw_terminal_{cid_str}");

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

            let mut pty_rx = k.pty_bytes();
            let _ = pty_rx.borrow_and_update();
            let mut carry: Vec<u8> = Vec::new();
            loop {
                tokio::select! {
                    result = pty_rx.changed() => {
                        if result.is_err() {
                            break;
                        }
                        let batches = pty_rx.borrow().clone();
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
                                k.send(Command::TerminalInput {
                                    conversation_id: cid,
                                    bytes: input.as_bytes().to_vec(),
                                });
                            }
                        }
                        if let Some(r) = obj.get("resize").and_then(|r| r.as_object()) {
                            let cols = r.get("cols").and_then(|c| c.as_u64()).unwrap_or(80) as u16;
                            let rows = r.get("rows").and_then(|r| r.as_u64()).unwrap_or(24) as u16;
                            k.send(Command::TerminalResize {
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

    let border = theme::BORDER;
    // Never use display:none for unfocused xterms. Cross-stage remount used
    // to open FitAddon at 0×0; later refocus returned ok but the canvas stayed
    // blank (2026-08-11 log: fitted path Ok(true), user still saw no CLI).
    // Off-screen fixed box keeps real width/height so open/fit stay healthy;
    // focus flips CSS + remount (key f/h) onto a flex-growing host. Byte pumps
    // stay mounted for all live ids.
    let wrap = if focused {
        format!(
            "margin-top:14px;border:1px solid {border};border-radius:8px;overflow:hidden;\
             flex:1;min-height:0;display:flex;flex-direction:column;"
        )
    } else {
        "position:fixed;left:-10000px;top:0;width:800px;height:360px;overflow:hidden;opacity:0;pointer-events:none;".into()
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
            style: "{wrap}",
            div {
                style: "flex:none;background:#1e1e2e;color:#cdd6f4;font-family:JetBrains Mono,Consolas,monospace;font-size:11px;padding:4px 10px;display:flex;align-items:center;gap:6px;",
                span { style: "opacity:0.7;", "● in-app terminal" }
                span { style: "opacity:0.4;margin-left:auto;", "claude interactive session" }
            }
            div {
                id: "{div_id}",
                style: "flex:1;min-height:0;height:100%;background:#1e1e2e;",
            }
        }
    }
}
