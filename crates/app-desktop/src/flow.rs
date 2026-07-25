//! `BW_FLOW=<command-file>` — an in-process click/fill/assert driver.
//!
//! WHY THIS EXISTS (verification discipline, not a product feature, same
//! rationale as `BW_HUB`/`BW_SEL` in `main.rs`): OS-level computer-use click
//! automation is blocked on the dev machine this app is verified on — the
//! window manager misattributes clicks meant for the app window to the Dock.
//! Moving the driver *into* the webview sidesteps that: it polls a plain-text
//! command file and turns each new line into a real DOM event dispatched
//! against the live document, exactly the same path a human mouse click
//! takes — DOM event → Dioxus's delegated listener → `Command` → kernel →
//! SQLite. That equivalence is the entire point: a click here proves the UI
//! itself works, not just that the kernel accepts a `Command`. Never shortcut
//! it by calling kernel `Command`s directly from this file.
//!
//! Three commands (YAGNI — exactly what E2E scripts in this repo need):
//! `click <visible text>`, `fill <placeholder>|<value>`, `assert_text
//! <text>`. Every processed line appends one result line to `<path>.log`
//! (and mirrors it to stderr) in the format `[BW_FLOW] <line#> <cmd> ok` /
//! `[BW_FLOW] <line#> <cmd> fail: <reason>` — report generators parse this
//! literally, so don't change the shape without updating them too.

use dioxus::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

const POLL_INTERVAL: Duration = Duration::from_millis(300);

/// Mounts the driver as a `use_future`, called unconditionally from `Root`
/// (hook-order stability) alongside the kernel-subscription futures. It is a
/// true no-op — the spawned future returns immediately — when `BW_FLOW`
/// isn't set, which is the common case (every normal launch).
pub fn spawn_driver() {
    use_future(move || async move {
        let Some(cmd_path) = std::env::var_os("BW_FLOW").map(PathBuf::from) else {
            return;
        };
        let log_path = log_path_for(&cmd_path);
        let mut processed = 0usize;
        loop {
            let content = std::fs::read_to_string(&cmd_path).unwrap_or_default();
            let lines: Vec<&str> = content.lines().collect();
            if lines.len() > processed {
                for (idx, raw) in lines.iter().enumerate().skip(processed) {
                    let lineno = idx + 1;
                    let cmd = raw.trim();
                    if cmd.is_empty() {
                        continue;
                    }
                    let (ok, reason) = run_command(lineno, cmd).await;
                    log_result(&log_path, lineno, cmd, ok, &reason);
                }
                processed = lines.len();
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    });
}

/// `<path>` → `<path>.log`, preserving the directory.
fn log_path_for(cmd_path: &Path) -> PathBuf {
    let mut name = cmd_path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".log");
    cmd_path.with_file_name(name)
}

/// Runs one already-trimmed, non-empty command line and returns `(ok,
/// reason)` — `reason` is empty on success. `seq` is the command's 1-based
/// line number, used as a monotonic tag on the JS-side stash (see
/// `stash_wrap`) so a read-back can never mistake a stale result for the
/// current one.
///
/// RACE THIS WORKS AROUND: a mutating command (click/fill) triggers a
/// Dioxus re-render before `document::eval(...).await` resolves. The
/// re-render can invalidate the `Eval`'s generational box mid-flight, which
/// makes the `await` resolve to `EvalError::Finished` *even though the JS
/// already ran to completion* — the DOM was clicked/filled, only the Rust
/// side never heard back. Reporting that as `fail` would be a false
/// negative, and an intermittent one at that (whether the race is lost
/// depends on render timing), which is worse than useless for a driver
/// whose entire job is to be trustworthy evidence.
///
/// So every action script stashes its own outcome on `window.__bw_flow`
/// before returning it — the "happy path" (eval survives) still resolves in
/// one round-trip. Only when the first eval's `await` errors do we issue a
/// second, non-mutating eval that just reads `window.__bw_flow` back; being
/// non-mutating, it doesn't itself trigger a re-render and so isn't subject
/// to the same race. If that read-back doesn't turn up a stash tagged with
/// this exact `seq`, we honestly report `unknown` rather than guess — never
/// fabricate an `ok`.
async fn run_command(seq: usize, cmd: &str) -> (bool, String) {
    let (verb, rest) = match cmd.split_once(' ') {
        Some((v, r)) => (v, r.trim()),
        None => (cmd, ""),
    };
    let script = match verb {
        "click" => click_script(seq, rest),
        "fill" => match rest.split_once('|') {
            Some((placeholder, value)) => fill_script(seq, placeholder.trim(), value.trim()),
            None => return (false, format!("fill missing '|': {rest:?}")),
        },
        "assert_text" => assert_text_script(seq, rest),
        other => return (false, format!("unknown command: {other}")),
    };

    match document::eval(&script).await {
        Ok(v) => stash_result(&v)
            .unwrap_or_else(|| (false, "eval returned no usable result".to_string())),
        Err(e) => {
            // The action's own eval didn't survive the round-trip — but it
            // may well have run. Ask the DOM what actually happened via a
            // fresh, non-mutating eval before conceding defeat.
            match document::eval(&readback_script(seq)).await {
                Ok(v) => match stash_result(&v) {
                    Some(result) => result,
                    None => (
                        false,
                        format!("unknown: eval error: {e}; no matching stash for seq {seq}"),
                    ),
                },
                Err(e2) => (
                    false,
                    format!("unknown: eval error: {e}; readback also failed: {e2}"),
                ),
            }
        }
    }
}

/// Extracts `(ok, reason)` from a stashed `{seq, ok, reason}` object.
/// `None` for anything that isn't one — notably the read-back script's
/// `null` for "no stash" / "stash is for a different seq", which must never
/// be coerced into a guessed result.
fn stash_result(v: &serde_json::Value) -> Option<(bool, String)> {
    let ok = v.get("ok")?.as_bool()?;
    let reason = v
        .get("reason")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    Some((ok, reason))
}

/// Wraps an action script's guts — which must assign `{ok, reason}` to the
/// JS variable `__bw_result` — so it also stashes that outcome on
/// `window.__bw_flow` (tagged with `seq`) before returning it. This is the
/// one mechanism shared by all three commands: the fast path (this eval's
/// `await` survives) uses the direct return value; the slow path
/// (`readback_script`) re-reads the same stash later.
fn stash_wrap(seq: usize, action_js: &str) -> String {
    format!(
        r#"
let __bw_result;
{action_js}
window.__bw_flow = Object.assign({{seq: {seq}}}, __bw_result);
return window.__bw_flow;
"#
    )
}

/// Non-mutating companion to `stash_wrap`: reads `window.__bw_flow` back
/// without touching the DOM, so issuing it can't itself race a re-render.
/// Returns `null` (not the stash) unless the stash's `seq` matches exactly
/// — a stale stash from an earlier, already-superseded command must never
/// be reported as the current command's result.
fn readback_script(seq: usize) -> String {
    format!(
        r#"
let stash = window.__bw_flow;
if (stash && stash.seq === {seq}) {{
    return stash;
}}
return null;
"#
    )
}

/// Finds the innermost element whose trimmed `textContent` exactly matches
/// `text` (fewest descendant elements wins — the most specific match, so a
/// wrapping card doesn't get picked over the label inside it) and clicks it.
/// `el.click()` dispatches a real, bubbling DOM `click` event, which is what
/// reaches Dioxus's delegated listener — same path a real mouse click takes.
fn click_script(seq: usize, text: &str) -> String {
    let target = js_string(text);
    let action_js = format!(
        r#"
let target = {target};
let best = null, bestCount = Infinity;
document.querySelectorAll('*').forEach((el) => {{
    let t = (el.textContent || '').trim();
    if (t === target) {{
        let c = el.querySelectorAll('*').length;
        if (c < bestCount) {{ bestCount = c; best = el; }}
    }}
}});
if (!best) {{
    __bw_result = {{ok: false, reason: 'no element with text: ' + target}};
}} else {{
    best.click();
    __bw_result = {{ok: true, reason: ''}};
}}
"#
    );
    stash_wrap(seq, &action_js)
}

/// Finds an `input`/`textarea` by exact `placeholder` match, sets `.value`,
/// then fires a bubbling `input` `Event` — plain assignment alone doesn't
/// reach Dioxus's `oninput`, which listens for the real DOM event.
fn fill_script(seq: usize, placeholder: &str, value: &str) -> String {
    let target = js_string(placeholder);
    let val = js_string(value);
    let action_js = format!(
        r#"
let target = {target};
let els = Array.from(document.querySelectorAll('input,textarea'));
let el = els.find((e) => e.placeholder === target);
if (!el) {{
    __bw_result = {{ok: false, reason: 'no input with placeholder: ' + target}};
}} else {{
    el.value = {val};
    el.dispatchEvent(new Event('input', {{bubbles: true}}));
    __bw_result = {{ok: true, reason: ''}};
}}
"#
    );
    stash_wrap(seq, &action_js)
}

/// `ok` iff `text` appears anywhere in `document.body.innerText`.
fn assert_text_script(seq: usize, text: &str) -> String {
    let target = js_string(text);
    let action_js = format!(
        r#"
let target = {target};
let bodyText = document.body.innerText || '';
if (bodyText.indexOf(target) !== -1) {{
    __bw_result = {{ok: true, reason: ''}};
}} else {{
    __bw_result = {{ok: false, reason: 'text not found: ' + target}};
}}
"#
    );
    stash_wrap(seq, &action_js)
}

/// Renders a Rust `&str` as a JS string literal (JSON string syntax is a
/// valid subset of JS string syntax, including Unicode) so command text can
/// carry quotes, backslashes, or Chinese text without breaking the script.
fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}

/// Appends one result line to `log_path` and mirrors it to stderr — same
/// dual-write discipline as `[BW_OPEN]`/`[BW_HUB]`.
fn log_result(log_path: &Path, lineno: usize, cmd: &str, ok: bool, reason: &str) {
    use std::io::Write;
    let line = if ok {
        format!("[BW_FLOW] {lineno} {cmd} ok")
    } else {
        format!("[BW_FLOW] {lineno} {cmd} fail: {reason}")
    };
    eprintln!("{line}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = writeln!(f, "{line}");
    }
}
