//! Discover a running Open Design web sidecar so the prototype progress
//! panel can iframe its home page. UI-only: no kernel types, no store.
//!
//! Discovery order: `BW_OPEN_DESIGN_URL` → named-pipe / unix-socket STATUS
//! (see `docs/v2-prototype/open-design-embed-spike.md`). Port is assigned
//! per launch; never hard-code it.
//!
//! Callers must not run this on the UI thread: pipe `CreateFile` can block
//! with no open timeout (`#![forbid(unsafe_code)]` bars `WaitNamedPipe`).
//! The 400ms cap below only waits for a worker; a wedged open stays on that
//! worker and is dropped.

use std::io::{Read, Write};
use std::time::Duration;

const QUERY_TIMEOUT: Duration = Duration::from_millis(400);
const STATUS_PAYLOAD: &[u8] = b"{\"type\":\"status\"}\n";

/// Loopback HTTP origin of the Open Design web UI, if a sidecar is up.
pub fn discover_web_url() -> Option<String> {
    if let Ok(raw) = std::env::var("BW_OPEN_DESIGN_URL") {
        let trimmed = raw.trim();
        if looks_loopback_http(trimmed) {
            return Some(trimmed.trim_end_matches('/').to_string());
        }
    }
    query_with_timeout()
}

fn query_with_timeout() -> Option<String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(query_web_sidecar());
    });
    rx.recv_timeout(QUERY_TIMEOUT).ok().flatten()
}

fn query_web_sidecar() -> Option<String> {
    for path in sidecar_status_paths() {
        if let Some(url) = query_status_path(&path) {
            return Some(url);
        }
    }
    None
}

fn sidecar_status_paths() -> Vec<String> {
    #[cfg(windows)]
    {
        // `\\.\pipe\open-design-{namespace}-web` — packaged Windows uses
        // `release-stable-win`. Keep a couple of fallbacks.
        [
            r"\\.\pipe\open-design-release-stable-win-web",
            r"\\.\pipe\open-design-default-web",
            r"\\.\pipe\open-design-stable-web",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }
    #[cfg(not(windows))]
    {
        // Packaged Mac: `release-stable`. Packaged Linux: `release-stable-linux`.
        [
            "/tmp/open-design/ipc/release-stable/web.sock",
            "/tmp/open-design/ipc/release-stable-linux/web.sock",
            "/tmp/open-design/ipc/default/web.sock",
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    }
}

fn query_status_path(path: &str) -> Option<String> {
    let mut stream = open_duplex(path)?;
    stream.write_all(STATUS_PAYLOAD).ok()?;
    let _ = stream.flush();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    parse_status_url(&buf)
}

#[cfg(windows)]
fn open_duplex(path: &str) -> Option<std::fs::File> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x1;
    const FILE_SHARE_WRITE: u32 = 0x2;
    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(path)
        .ok()
}

#[cfg(not(windows))]
fn open_duplex(path: &str) -> Option<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(path).ok().map(|s| {
        let _ = s.set_read_timeout(Some(QUERY_TIMEOUT));
        let _ = s.set_write_timeout(Some(QUERY_TIMEOUT));
        s
    })
}

fn parse_status_url(bytes: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    if v.get("ok").and_then(|x| x.as_bool()) != Some(true) {
        return None;
    }
    let url = v
        .get("result")
        .and_then(|r| r.get("url"))
        .and_then(|u| u.as_str())?
        .trim();
    if looks_loopback_http(url) {
        Some(url.trim_end_matches('/').to_string())
    } else {
        None
    }
}

/// `http://127.0.0.1:<port>` or `http://localhost:<port>` only. Rejects
/// userinfo (`@`), missing port, and anything that is not a loopback host.
fn looks_loopback_http(url: &str) -> bool {
    let rest = match url.trim().strip_prefix("http://") {
        Some(r) => r,
        None => return false,
    };
    if rest.contains('@') {
        return false;
    }
    let hostport = rest.split('/').next().unwrap_or("");
    let Some((host, port)) = hostport.rsplit_once(':') else {
        return false;
    };
    if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    host == "127.0.0.1" || host.eq_ignore_ascii_case("localhost")
}
