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
//! Four commands (YAGNI — exactly what E2E scripts in this repo need):
//! `click <visible text>`, `fill <placeholder>|<value>`, `assert_text
//! <text>`, `snap <name>`. Every processed line appends one result line to
//! `<path>.log` (and mirrors it to stderr) in the format `[BW_FLOW] <line#>
//! <cmd> ok` / `[BW_FLOW] <line#> <cmd> fail: <reason>` — report generators
//! parse this literally, so don't change the shape without updating them
//! too.
//!
//! `snap` is the odd one out: OS-level screenshotting is dead on the dev
//! machine this app is verified on (CLI `screencapture` produces
//! wallpaper-only images — the host process lacks Screen Recording
//! permission — and computer-use tooling can see the window but can't save
//! files). Rather than chase OS permissions further, `snap` captures the
//! app's own rendered UI from *inside* the webview: JS serializes the live
//! DOM into an SVG `<foreignObject>`, rasterizes it to a canvas, and hands
//! back a PNG data URL; Rust decodes and writes it to disk. Zero OS
//! permission needed, because nothing leaves the webview's own process.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use dioxus::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::theme;

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
                    let (ok, reason) = run_command(lineno, cmd, &cmd_path).await;
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
async fn run_command(seq: usize, cmd: &str, cmd_path: &Path) -> (bool, String) {
    let (verb, rest) = match cmd.split_once(' ') {
        Some((v, r)) => (v, r.trim()),
        None => (cmd, ""),
    };
    // `snap` doesn't fit the generic "eval a script, get {ok, reason} back"
    // shape below — it also carries a multi-MB PNG payload that must never
    // touch the `window.__bw_flow` stash (see `run_snap`) — so it's handled
    // by its own function entirely, before the generic dispatch.
    if verb == "snap" {
        return run_snap(seq, rest, cmd_path).await;
    }
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

/// Runs `snap <name>` end to end: validates `name`, evals the capture
/// script, and (on success) decodes+writes the PNG. Kept separate from
/// `run_command`'s generic dispatch because it needs `cmd_path` (to find
/// `snaps/`) and because its success/failure data flow doesn't fit the
/// plain `{ok, reason}` shape the other three commands share — see
/// `snap_stash_wrap` for why the stash and the return value carry different
/// payloads.
async fn run_snap(seq: usize, name: &str, cmd_path: &Path) -> (bool, String) {
    let name = name.trim();
    if !valid_snap_name(name) {
        return (
            false,
            format!("invalid snap name (only [A-Za-z0-9._-] allowed): {name:?}"),
        );
    }
    let script = snap_script(seq);
    let data_url = match document::eval(&script).await {
        Ok(v) => match snap_result(&v) {
            Some(Ok(data)) => data,
            Some(Err(reason)) => return (false, reason),
            None => return (false, "eval returned no usable result".to_string()),
        },
        Err(e) => {
            // Same re-render race `run_command` documents for click/fill —
            // but the stash for `snap` deliberately never carries `data`
            // (see `snap_stash_wrap`), so even a successful read-back can't
            // recover the PNG bytes. Reporting a truncated/blank file here
            // would be worse than reporting the honest gap.
            match document::eval(&readback_script(seq)).await {
                Ok(v) => match stash_result(&v) {
                    Some((true, _)) => return (false, "unknown: 截图数据丢失(重渲染)".to_string()),
                    Some((false, reason)) => return (false, reason),
                    None => {
                        return (
                            false,
                            format!("unknown: eval error: {e}; no matching stash for seq {seq}"),
                        )
                    }
                },
                Err(e2) => {
                    return (
                        false,
                        format!("unknown: eval error: {e}; readback also failed: {e2}"),
                    )
                }
            }
        }
    };
    write_snap(cmd_path, name, &data_url)
}

/// Command-line snap names become file names directly (`snaps/<name>.png`)
/// — restrict to a safe charset so a command line can never write outside
/// `snaps/` (no `/`, `..`, or anything shell/path-meaningful).
fn valid_snap_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// Decodes a `snap`-shaped eval result: `Some(Ok(data_url))` on success,
/// `Some(Err(reason))` for a JS-reported failure, `None` if the value isn't
/// even `{ok, ...}`-shaped (unlike `stash_result`, a successful `snap` MUST
/// carry `data` — a `{ok: true}` with no `data` is treated as "no usable
/// result", never as a silent empty capture).
fn snap_result(v: &serde_json::Value) -> Option<Result<String, String>> {
    let ok = v.get("ok")?.as_bool()?;
    if ok {
        let data = v.get("data")?.as_str()?.to_string();
        Some(Ok(data))
    } else {
        let reason = v
            .get("reason")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        Some(Err(reason))
    }
}

/// Decodes the `data:image/png;base64,...` payload and writes it to
/// `<dir-of-cmd_path>/snaps/<name>.png`, creating `snaps/` if needed.
/// `name` has already passed `valid_snap_name`.
fn write_snap(cmd_path: &Path, name: &str, data_url: &str) -> (bool, String) {
    const PREFIX: &str = "data:image/png;base64,";
    let Some(b64) = data_url.strip_prefix(PREFIX) else {
        let preview: String = data_url.chars().take(40).collect();
        return (
            false,
            format!("unexpected data URL shape (want prefix {PREFIX:?}): {preview:?}..."),
        );
    };
    let bytes = match BASE64.decode(b64) {
        Ok(b) => b,
        Err(e) => return (false, format!("base64 decode failed: {e}")),
    };
    let snaps_dir = cmd_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("snaps");
    if let Err(e) = std::fs::create_dir_all(&snaps_dir) {
        return (false, format!("mkdir {snaps_dir:?} failed: {e}"));
    }
    let out_path = snaps_dir.join(format!("{name}.png"));
    if let Err(e) = std::fs::write(&out_path, &bytes) {
        return (false, format!("write {out_path:?} failed: {e}"));
    }
    (true, String::new())
}

/// Captures the live rendered UI as a PNG data URL, entirely inside the
/// webview (no OS screenshot permission involved — see the module doc for
/// why that matters on this machine):
///
/// 1. Walk `document.styleSheets`, concatenating `cssRules.cssText` (wrapped
///    in try/catch — a cross-origin or otherwise inaccessible sheet is
///    skipped, not fatal; BW styles most things with inline `style=`
///    already, so a partial stylesheet still yields a faithful image).
/// 2. Clone `document.documentElement`, inline that CSS as a `<style>` in
///    the clone's `<head>`, and serialize the clone to a string (it needs
///    `xmlns="http://www.w3.org/1999/xhtml"` to be valid inside an SVG
///    `foreignObject` — `cloneNode` on `<html>` already carries that
///    attribute in a webview document, so nothing extra is added here).
/// 3. Wrap the markup in `<svg><foreignObject>...</foreignObject></svg>`
///    sized from `window.innerWidth`/`innerHeight` (deliberately NOT scaled
///    by `devicePixelRatio` — this keeps the base64 payload small, since it
///    travels through the eval IPC channel).
/// 4. Load that SVG into an `Image`, `await` its `onload`, draw it onto a
///    canvas pre-filled with the app's paper background (so transparent
///    regions don't render black), and return `canvas.toDataURL('image/png')`.
///
/// Everything is wrapped in one try/catch: any failure (tainted canvas,
/// image load error, serialization error) becomes `{ok: false, reason}`,
/// never a hang and never a silently blank image reported as success.
fn snap_script(seq: usize) -> String {
    let paper = js_string(theme::PAPER);
    let action_js = format!(
        r#"
try {{
    let cssText = '';
    for (const sheet of Array.from(document.styleSheets)) {{
        try {{
            for (const rule of Array.from(sheet.cssRules || [])) {{
                cssText += rule.cssText + '\n';
            }}
        }} catch (e) {{
            // cross-origin or otherwise inaccessible sheet — skip, not fatal.
        }}
    }}
    let w = window.innerWidth;
    let h = window.innerHeight;
    let clone = document.documentElement.cloneNode(true);
    clone.setAttribute('xmlns', 'http://www.w3.org/1999/xhtml');
    let styleEl = document.createElement('style');
    styleEl.textContent = cssText;
    let head = clone.querySelector('head');
    if (head) {{
        head.insertBefore(styleEl, head.firstChild);
    }} else {{
        clone.insertBefore(styleEl, clone.firstChild);
    }}
    let markup = new XMLSerializer().serializeToString(clone);
    let svg = '<svg xmlns="http://www.w3.org/2000/svg" width="' + w + '" height="' + h + '">'
        + '<foreignObject width="100%" height="100%">' + markup + '</foreignObject></svg>';
    let svgUrl = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
    let img = new Image();
    let loaded = new Promise((resolve, reject) => {{
        img.onload = () => resolve();
        img.onerror = () => reject(new Error('svg image failed to load'));
    }});
    img.src = svgUrl;
    await loaded;
    let canvas = document.createElement('canvas');
    canvas.width = w;
    canvas.height = h;
    let ctx = canvas.getContext('2d');
    ctx.fillStyle = {paper};
    ctx.fillRect(0, 0, w, h);
    ctx.drawImage(img, 0, 0, w, h);
    let dataUrl = canvas.toDataURL('image/png');
    __bw_result = {{ok: true, reason: '', data: dataUrl}};
}} catch (e) {{
    __bw_result = {{ok: false, reason: 'snap failed: ' + (e && e.message ? e.message : String(e))}};
}}
"#
    );
    snap_stash_wrap(seq, &action_js)
}

/// Like `stash_wrap`, but for `snap`: the returned value carries the full
/// `{seq, ok, reason, data}` (the PNG data URL travels once, over the same
/// eval round-trip already carrying it), while `window.__bw_flow` stashes
/// only `{seq, ok, reason}` — deliberately WITHOUT `data`. A multi-MB data
/// URL sitting on `window.__bw_flow` would itself bloat every future
/// `readback_script` round-trip, and the read-back path only ever needs to
/// answer "did it succeed", not "here are the bytes again" — if the
/// happy-path return is lost to the re-render race, `run_snap` reports the
/// honest `截图数据丢失(重渲染)` rather than trying to recover bytes that
/// were never stashed.
fn snap_stash_wrap(seq: usize, action_js: &str) -> String {
    format!(
        r#"
let __bw_result;
{action_js}
window.__bw_flow = {{seq: {seq}, ok: __bw_result.ok, reason: __bw_result.reason}};
return Object.assign({{seq: {seq}}}, __bw_result);
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
