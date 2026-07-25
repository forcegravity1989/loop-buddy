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
                    let (ok, reason) = run_command(cmd).await;
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
/// reason)` — `reason` is empty on success.
async fn run_command(cmd: &str) -> (bool, String) {
    let (verb, rest) = match cmd.split_once(' ') {
        Some((v, r)) => (v, r.trim()),
        None => (cmd, ""),
    };
    let script = match verb {
        "click" => click_script(rest),
        "fill" => match rest.split_once('|') {
            Some((placeholder, value)) => fill_script(placeholder.trim(), value.trim()),
            None => return (false, format!("fill missing '|': {rest:?}")),
        },
        "assert_text" => assert_text_script(rest),
        other => return (false, format!("unknown command: {other}")),
    };

    match document::eval(&script).await {
        Ok(v) => {
            let ok = v.get("ok").and_then(|x| x.as_bool()).unwrap_or(false);
            let reason = v
                .get("reason")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            (ok, reason)
        }
        Err(e) => (false, format!("eval error: {e}")),
    }
}

/// Finds the innermost element whose trimmed `textContent` exactly matches
/// `text` (fewest descendant elements wins — the most specific match, so a
/// wrapping card doesn't get picked over the label inside it) and clicks it.
/// `el.click()` dispatches a real, bubbling DOM `click` event, which is what
/// reaches Dioxus's delegated listener — same path a real mouse click takes.
fn click_script(text: &str) -> String {
    let target = js_string(text);
    format!(
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
    return {{ok: false, reason: 'no element with text: ' + target}};
}}
best.click();
return {{ok: true, reason: ''}};
"#
    )
}

/// Finds an `input`/`textarea` by exact `placeholder` match, sets `.value`,
/// then fires a bubbling `input` `Event` — plain assignment alone doesn't
/// reach Dioxus's `oninput`, which listens for the real DOM event.
fn fill_script(placeholder: &str, value: &str) -> String {
    let target = js_string(placeholder);
    let val = js_string(value);
    format!(
        r#"
let target = {target};
let els = Array.from(document.querySelectorAll('input,textarea'));
let el = els.find((e) => e.placeholder === target);
if (!el) {{
    return {{ok: false, reason: 'no input with placeholder: ' + target}};
}}
el.value = {val};
el.dispatchEvent(new Event('input', {{bubbles: true}}));
return {{ok: true, reason: ''}};
"#
    )
}

/// `ok` iff `text` appears anywhere in `document.body.innerText`.
fn assert_text_script(text: &str) -> String {
    let target = js_string(text);
    format!(
        r#"
let target = {target};
let body = document.body.innerText || '';
if (body.indexOf(target) !== -1) {{
    return {{ok: true, reason: ''}};
}}
return {{ok: false, reason: 'text not found: ' + target}};
"#
    )
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
