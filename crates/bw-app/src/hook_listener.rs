//! V1 Issue2 Phase2b: claude hook listener — a localhost HTTP server that
//! receives claude Code hook events (SessionStart / Stop) POSTed by a curl
//! script installed in `~/.claude/settings.json`.
//!
//! ## Why
//! Phase2a used `claude --continue` (resume most-recent session in cwd) + a
//! 5-minute poller for InReview detection. Phase2b upgrades both:
//!  - **SessionStart** → captures the real `session_id` (stored on the issue,
//!    used by `claude --resume <id>` — precise, not "most recent"). F1 fix:
//!    no session_id captured → next run falls back to `build_startup_plan`
//!    (re-inject skill), never gets stuck in a skill-less session.
//!  - **Stop** → triggers InReview detection (debounced) — replaces the
//!    5-minute poller's cadence with a real-time trigger.
//!
//! ## How
//! `HookListener::start` binds a `TcpListener` on `127.0.0.1:51790` (fixed
//! port; falls back to OS-assigned `:0` if the port is in use). Each
//! connection reads the HTTP POST body (the claude hook JSON payload), parses
//! `hook_event_name` / `session_id` / `cwd`, and sends a [`HookEvent`] via
//! the mpsc channel. The App polls this channel in `tick_scheduler` and
//! processes events (store session_id, trigger InReview check).
//!
//! `install_hooks_config(port)` writes the hooks section of
//! `~/.claude/settings.json` — idempotent (strips old buddy-managed entries
//! by URL-path marker, preserves user hooks, appends fresh entries). Atomic
//! (temp + rename). Cross-platform (`curl` is available on Windows 10+,
//! macOS, and Linux).
//!
//! ## Deviation from orca
//! orca uses form-encoded body + auth token + endpoint file for port
//! resilience. buddy is single-pane single-agent — JSON body (simpler),
//! no auth token (localhost only, no remote), fixed port (no endpoint file).
//! Fail-open: a parse error still responds 200 (never blocks the agent).

use std::path::PathBuf;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// The fixed port the hook listener tries to bind first. Falls back to
/// OS-assigned (`:0`) if this port is in use. A fixed port means
/// `~/.claude/settings.json` only needs writing once (idempotent); an
/// OS-assigned port would require re-writing settings.json on every start.
const BW_HOOK_PORT: u16 = 51790;

/// The URL path the hook script POSTs to. Used as a marker to identify
/// buddy-managed hooks in `~/.claude/settings.json` (idempotent merge: strip
/// old entries whose command contains this path, preserve user hooks).
const BW_HOOK_PATH: &str = "/bw-hook";

/// Events the hook listener sends to the App via mpsc. The App polls these
/// in `tick_scheduler` (or the desktop kernel's `select!` loop) and processes
/// them under `&mut self`.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Stop's session_id/cwd reserved for future targeted checks
pub enum HookEvent {
    /// claude `SessionStart` — the payload carries `session_id` + `cwd`.
    /// The App maps `cwd` → issue (via the `interactive_sessions` map) and
    /// stores `session_id` on the issue (for `--resume <id>`).
    SessionStart { session_id: String, cwd: String },
    /// claude `Stop` — the agent finished a turn (waiting for user input).
    /// Fires frequently (every agent response). The App uses this as a
    /// **debounced** trigger for InReview detection (not "immediately check
    /// on every Stop" — debounce prevents flooding the remote).
    Stop { session_id: String, cwd: String },
}

/// The hook listener: a localhost HTTP server that receives claude hook
/// events (POSTed by a curl script installed in `~/.claude/settings.json`).
/// All methods are associated functions — no instance state (the port is
/// returned by `bind`, the accept loop runs in a background task).
#[derive(Debug, Default)]
pub struct HookListener;

impl HookListener {
    /// Start the listener on `127.0.0.1`. Returns the bound port.
    ///
    /// Spawns a background task that accepts connections, reads the HTTP
    /// POST body, parses the claude hook event JSON, and sends a
    /// [`HookEvent`] via `tx`. Each connection is handled in its own
    /// task (a slow client never blocks others).
    ///
    /// Fail-open: any parse error responds `200 OK` and moves on — a broken
    /// hook never blocks the agent (claude waits for the hook command to
    /// exit; curl's `--max-time 2` is the backstop if the server is slow).
    /// Sync bind — returns `(port, TcpListener)`. Uses `std::net` for the
    /// initial bind (no async runtime needed) so the port is known
    /// immediately. `App::new` calls this, then spawns the accept loop via
    /// [`HookListener::spawn`] if a tokio runtime is available.
    pub fn bind() -> std::io::Result<(u16, TcpListener)> {
        let std_listener = std::net::TcpListener::bind(("127.0.0.1", BW_HOOK_PORT))
            .or_else(|_| std::net::TcpListener::bind(("127.0.0.1", 0)))?;
        let port = std_listener.local_addr()?.port();
        std_listener.set_nonblocking(true)?;
        let listener = TcpListener::from_std(std_listener)?;
        Ok((port, listener))
    }

    /// Spawn the accept loop (async — must be called from a tokio runtime).
    /// The listener was pre-bound by [`bind`]; this just runs the accept
    /// loop forever (each connection handled in its own task).
    pub fn spawn(listener: TcpListener, tx: mpsc::UnboundedSender<HookEvent>) {
        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let _ = handle_connection(stream, tx).await;
                        });
                    }
                    Err(_) => continue,
                }
            }
        });
    }
}

/// Handle one HTTP connection: read the POST body, parse the claude hook
/// event JSON, send a [`HookEvent`] via `tx`, respond `200 OK`.
///
/// Minimal HTTP parsing (sufficient for localhost curl POSTs):
/// 1. Read until `\r\n\r\n` (end of headers).
/// 2. Find `Content-Length`, read that many bytes (body).
/// 3. Parse body as JSON, extract `hook_event_name` / `session_id` / `cwd`.
/// 4. Respond `HTTP/1.1 200 OK` (quick, so curl exits and claude unblocks).
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    tx: mpsc::UnboundedSender<HookEvent>,
) -> std::io::Result<()> {
    // Read headers (until \r\n\r\n). Cap at 64KB so a malicious/malformed
    // client can't exhaust memory.
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if find_subsequence(&buf, b"\r\n\r\n").is_some() {
            break;
        }
        if buf.len() > 65536 {
            break; // header too large — bail
        }
    }

    // Find the header/body boundary.
    let header_end = match find_subsequence(&buf, b"\r\n\r\n") {
        Some(pos) => pos,
        None => {
            return respond_200(&mut stream).await; // no header end — bail fast
        }
    };

    // Parse Content-Length from the header section.
    let headers = String::from_utf8_lossy(&buf[..header_end]);
    // Cap the body the same way the header read is capped: a claude hook
    // payload is a few KB of JSON, so anything past MAX_BODY is either
    // malformed or a local process trying to make buddy allocate. Reading
    // `Content-Length` unbounded would let it.
    const MAX_BODY: usize = 256 * 1024;
    let content_length = parse_content_length(&headers).unwrap_or(0).min(MAX_BODY);

    // The body may already be partially (or fully) in `buf` — collect the
    // remaining bytes if needed.
    let body_start = header_end + 4; // skip \r\n\r\n
    let mut body: Vec<u8> = if body_start < buf.len() {
        buf[body_start..].to_vec()
    } else {
        Vec::new()
    };
    while body.len() < content_length {
        let n = stream.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(content_length);

    // Parse the body as JSON (claude hook event payload).
    // Fail-open: a parse error still responds 200 (never blocks the agent).
    if !body.is_empty() {
        if let Ok(payload) = serde_json::from_slice::<serde_json::Value>(&body) {
            let event_name = payload
                .get("hook_event_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let session_id = payload
                .get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let cwd = payload.get("cwd").and_then(|v| v.as_str()).unwrap_or("");

            let event = match event_name {
                "SessionStart" => Some(HookEvent::SessionStart {
                    session_id: session_id.to_string(),
                    cwd: cwd.to_string(),
                }),
                "Stop" => Some(HookEvent::Stop {
                    session_id: session_id.to_string(),
                    cwd: cwd.to_string(),
                }),
                _ => None, // ignore other events (PreToolUse, PostToolUse, …)
            };
            if let Some(ev) = event {
                let _ = tx.send(ev); // best-effort: if the App dropped the rx, drop the event
            }
        }
    }

    respond_200(&mut stream).await
}

/// Respond `HTTP/1.1 200 OK` with an empty body. Quick so curl exits and
/// claude's hook unblocks.
async fn respond_200(stream: &mut tokio::net::TcpStream) -> std::io::Result<()> {
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await?;
    stream.flush().await
}

/// Find the first occurrence of `needle` in `haystack`. Returns the byte
/// index of the match, or `None`.
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Parse `Content-Length: <n>` from an HTTP header block (case-insensitive
/// header name). Returns the value, or `None` if not found / unparseable.
fn parse_content_length(headers: &str) -> Option<usize> {
    for line in headers.split("\r\n") {
        if let Some((name, value)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                return value.trim().parse().ok();
            }
        }
    }
    None
}

// ─── ~/.claude/settings.json hooks config ──────────────────────────────

/// Build the curl command string for the hook script. Cross-platform.
///
/// On Windows, uses `curl.exe` (explicit `.exe` bypasses PowerShell's
/// `curl` → `Invoke-WebRequest` alias, which doesn't recognize
/// `--data-binary`/`--connect-timeout`/`--max-time` and silently fails).
/// On macOS/Linux, `curl` resolves to the real curl binary.
///
/// `--connect-timeout 1 --max-time 2`: hook must never block the agent. If
/// the server is down/slow, curl exits within 2s and claude continues.
/// `--data-binary @-`: reads stdin (the JSON payload) and sends it as the
/// POST body, with no line-ending conversion (binary-safe).
fn hook_command(port: u16) -> String {
    let curl = if cfg!(target_os = "windows") {
        "curl.exe"
    } else {
        "curl"
    };
    format!(
        "{curl} -s --connect-timeout 1 --max-time 2 -X POST http://127.0.0.1:{port}{BW_HOOK_PATH} --data-binary @-"
    )
}

/// The claude hook events buddy cares about. SessionStart captures the
/// session_id; Stop triggers InReview detection (debounced). The hooks
/// config in settings.json uses these as the top-level keys.
const BW_HOOK_EVENTS: &[&str] = &["SessionStart", "Stop"];

/// Install buddy's hooks into `~/.claude/settings.json` — idempotent merge
/// (strips old buddy-managed entries by URL-path marker, preserves user
/// hooks, appends fresh entries). Atomic write (temp + rename).
///
/// Returns `Ok(())` on success or if settings.json is skipped (no home dir,
/// file unwritable). The app works without hooks — just no real-time
/// session_id capture / Stop trigger (falls back to 2a's polling).
///
/// # Safety / non-destructive merge
/// 1. Read existing settings.json (if it exists).
/// 2. Parse as a `serde_json::Value` (object). If parse fails, **don't**
///    write — the file may have user edits in a format we don't understand.
///    Return `Err` so the caller can log.
/// 3. Get/create the `hooks` object.
/// 4. For each event in [`BW_HOOK_EVENTS`]:
///    - Get/create the event's array.
///    - Strip any existing entries whose command contains `BW_HOOK_PATH`
///      (buddy-managed — update port if changed).
///    - Append one fresh managed entry `{ matcher: "", hooks: [{ type:
///      "command", command: <curl> }] }`.
/// 5. Write back atomically (temp file + rename). Skip the write if the
///    serialized content is unchanged (prevents needless `.bak` rotation).
pub fn install_hooks_config(port: u16) -> std::io::Result<()> {
    let settings_path = claude_settings_path().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::NotFound, "no home directory found")
    })?;

    // Ensure ~/.claude/ exists.
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Read existing settings.json (or start fresh).
    let existing = std::fs::read_to_string(&settings_path).unwrap_or_default();
    let mut root: serde_json::Value = if existing.trim().is_empty() {
        serde_json::json!({})
    } else {
        match serde_json::from_str(&existing) {
            Ok(v) => v,
            // Parse error → don't write (preserve user's file). The app
            // works without hooks — just log the error.
            Err(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "{} exists but is not valid JSON — not overwriting (preserve user file)",
                        settings_path.display()
                    ),
                ));
            }
        }
    };

    // Ensure root is an object.
    if !root.is_object() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "settings.json root is not a JSON object — not overwriting",
        ));
    }

    let cmd = hook_command(port);
    let root_obj = root.as_object_mut().unwrap();

    // Get or create the "hooks" object.
    let hooks = root_obj
        .entry("hooks".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !hooks.is_object() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "settings.json \"hooks\" is not a JSON object — not overwriting",
        ));
    }
    let hooks_obj = hooks.as_object_mut().unwrap();

    // For each event: strip old buddy-managed entries + append fresh.
    for event_name in BW_HOOK_EVENTS {
        let arr = hooks_obj
            .entry(event_name.to_string())
            .or_insert_with(|| serde_json::json!([]));
        if !arr.is_array() {
            // User had a non-array value for this event — don't touch it.
            continue;
        }
        let arr_obj = arr.as_array_mut().unwrap();

        // Strip old buddy-managed entries (command contains BW_HOOK_PATH).
        // This is idempotent: re-running install doesn't duplicate hooks.
        arr_obj.retain(|entry| {
            !entry
                .get("hooks")
                .and_then(|h| h.as_array())
                .map(|hooks_arr| {
                    hooks_arr.iter().any(|h| {
                        h.get("command")
                            .and_then(|c| c.as_str())
                            .is_some_and(|c| c.contains(BW_HOOK_PATH))
                    })
                })
                .unwrap_or(false)
        });

        // Append the fresh managed entry.
        arr_obj.push(serde_json::json!({
            "matcher": "",
            "hooks": [{
                "type": "command",
                "command": cmd
            }]
        }));
    }

    // Serialize + write atomically (temp + rename).
    let serialized = serde_json::to_string_pretty(&root)?;
    // Skip the write if content is unchanged (prevents needless disk churn).
    if serialized == existing {
        return Ok(());
    }
    let tmp = settings_path.with_extension("json.tmp");
    std::fs::write(&tmp, &serialized)?;
    std::fs::rename(&tmp, &settings_path)?;
    Ok(())
}

/// Remove buddy's hooks from `~/.claude/settings.json` — the inverse of
/// [`install_hooks_config`]. Strips entries whose command contains
/// `BW_HOOK_PATH`. Called on app shutdown (best-effort cleanup).
#[allow(dead_code)] // reserved for shutdown path (not yet wired)
pub fn uninstall_hooks_config() -> std::io::Result<()> {
    let Some(settings_path) = claude_settings_path() else {
        return Ok(()); // no home dir → nothing to clean
    };
    let existing = match std::fs::read_to_string(&settings_path) {
        Ok(s) => s,
        Err(_) => return Ok(()), // file doesn't exist → nothing to clean
    };
    let mut root: serde_json::Value = match serde_json::from_str(&existing) {
        Ok(v) => v,
        Err(_) => return Ok(()), // parse error → don't touch
    };
    let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return Ok(());
    };
    for event_name in BW_HOOK_EVENTS {
        if let Some(arr) = hooks.get_mut(*event_name).and_then(|a| a.as_array_mut()) {
            arr.retain(|entry| {
                !entry
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|hooks_arr| {
                        hooks_arr.iter().any(|h| {
                            h.get("command")
                                .and_then(|c| c.as_str())
                                .is_some_and(|c| c.contains(BW_HOOK_PATH))
                        })
                    })
                    .unwrap_or(false)
            });
            // If the event array is now empty, remove it.
            if arr.is_empty() {
                hooks.remove(*event_name);
            }
        }
    }
    // If hooks is now empty, remove it.
    if hooks.is_empty() {
        if let Some(obj) = root.as_object_mut() {
            obj.remove("hooks");
        }
    }
    let serialized = serde_json::to_string_pretty(&root)?;
    if serialized != existing {
        let tmp = settings_path.with_extension("json.tmp");
        std::fs::write(&tmp, &serialized)?;
        std::fs::rename(&tmp, &settings_path)?;
    }
    Ok(())
}

/// Get the path to `~/.claude/settings.json`. Cross-platform:
/// - Unix: `$HOME/.claude/settings.json`
/// - Windows: `%USERPROFILE%\.claude\settings.json`
fn claude_settings_path() -> Option<PathBuf> {
    let home = if cfg!(target_os = "windows") {
        std::env::var("USERPROFILE").ok()
    } else {
        std::env::var("HOME").ok()
    }?;
    Some(PathBuf::from(home).join(".claude").join("settings.json"))
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_command_contains_port_and_path() {
        let cmd = hook_command(51790);
        assert!(cmd.contains("127.0.0.1:51790"));
        assert!(cmd.contains(BW_HOOK_PATH));
        assert!(cmd.contains("--data-binary @-"));
        assert!(cmd.contains("--max-time 2"));
        // Windows: must use curl.exe (bypasses PowerShell alias).
        // Unix: plain curl.
        if cfg!(target_os = "windows") {
            assert!(cmd.starts_with("curl.exe"));
        } else {
            assert!(cmd.starts_with("curl "));
        }
    }

    #[test]
    fn parse_content_length_finds_value() {
        let headers = "POST /bw-hook HTTP/1.1\r\nHost: 127.0.0.1:51790\r\nContent-Length: 42\r\n";
        assert_eq!(parse_content_length(headers), Some(42));
    }

    #[test]
    fn parse_content_length_case_insensitive() {
        let headers = "POST /bw-hook HTTP/1.1\r\ncontent-length: 99\r\n";
        assert_eq!(parse_content_length(headers), Some(99));
    }

    #[test]
    fn parse_content_length_missing() {
        let headers = "POST /bw-hook HTTP/1.1\r\nHost: 127.0.0.1:51790\r\n";
        assert_eq!(parse_content_length(headers), None);
    }

    #[test]
    fn find_subsequence_finds_double_crlf() {
        let buf = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\nbody";
        // \r\n\r\n starts at byte 34 (after "Content-Length: 0").
        assert_eq!(find_subsequence(buf, b"\r\n\r\n"), Some(34));
    }

    #[test]
    fn find_subsequence_not_found() {
        let buf = b"no double crlf here";
        assert_eq!(find_subsequence(buf, b"\r\n\r\n"), None);
    }

    #[test]
    fn install_hooks_config_idempotent_merge_preserves_user_hooks() {
        // Create a temp dir as a fake home.
        let tmp = std::env::temp_dir().join(format!(
            "bw-hook-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&tmp).unwrap();
        let claude_dir = tmp.join(".claude");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let settings_path = claude_dir.join("settings.json");

        // Pre-existing user hooks + other config.
        let user_config = r#"{
            "theme": "dark",
            "hooks": {
                "SessionStart": [
                    { "matcher": "", "hooks": [{ "type": "command", "command": "echo user-hook" }] }
                ],
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "echo pre-tool" }] }
                ]
            }
        }"#;
        std::fs::write(&settings_path, user_config).unwrap();

        // Override the home dir for this test.
        let orig_home = std::env::var("HOME").ok();
        if cfg!(target_os = "windows") {
            std::env::set_var("USERPROFILE", &tmp);
        } else {
            std::env::set_var("HOME", &tmp);
        }

        // Install buddy hooks.
        install_hooks_config(51790).unwrap();

        // Read back + verify.
        let content = std::fs::read_to_string(&settings_path).unwrap();
        let root: serde_json::Value = serde_json::from_str(&content).unwrap();

        // User config preserved.
        assert_eq!(root["theme"], "dark");
        // User's PreToolUse hook preserved.
        let pre_tool = root["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre_tool.len(), 1);
        assert_eq!(pre_tool[0]["hooks"][0]["command"], "echo pre-tool");

        // SessionStart: user hook preserved + buddy hook appended.
        let session_start = root["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 2); // user + buddy
        assert_eq!(session_start[0]["hooks"][0]["command"], "echo user-hook");
        assert!(session_start[1]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(BW_HOOK_PATH));

        // Stop: buddy hook added (didn't exist before).
        let stop = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert!(stop[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains(BW_HOOK_PATH));

        // Install again — should be idempotent (no duplicates).
        install_hooks_config(51790).unwrap();
        let content2 = std::fs::read_to_string(&settings_path).unwrap();
        let root2: serde_json::Value = serde_json::from_str(&content2).unwrap();
        let session_start2 = root2["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start2.len(), 2); // still 2, not 3

        // Install with a different port — should update the port, not duplicate.
        install_hooks_config(9999).unwrap();
        let content3 = std::fs::read_to_string(&settings_path).unwrap();
        let root3: serde_json::Value = serde_json::from_str(&content3).unwrap();
        let session_start3 = root3["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start3.len(), 2); // still 2
        assert!(session_start3[1]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("9999"));

        // Uninstall — should remove buddy hooks, preserve user hooks.
        uninstall_hooks_config().unwrap();
        let content4 = std::fs::read_to_string(&settings_path).unwrap();
        let root4: serde_json::Value = serde_json::from_str(&content4).unwrap();
        let session_start4 = root4["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start4.len(), 1); // only user hook remains
        assert_eq!(session_start4[0]["hooks"][0]["command"], "echo user-hook");
        // Stop removed entirely (was only buddy).
        assert!(root4["hooks"].get("Stop").is_none());

        // Restore HOME.
        if let Some(h) = orig_home {
            if cfg!(target_os = "windows") {
                std::env::set_var("USERPROFILE", h);
            } else {
                std::env::set_var("HOME", h);
            }
        }
    }

    #[tokio::test]
    async fn hook_listener_receives_session_start_and_stop() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;
        use tokio::time::{timeout, Duration};

        let (tx, mut rx) = mpsc::unbounded_channel::<HookEvent>();
        let (port, listener) = HookListener::bind().unwrap();
        HookListener::spawn(listener, tx);

        // Helper: send a raw HTTP POST to the hook listener (simulates what
        // curl does — no external dependency on curl being installed).
        async fn post_hook(port: u16, body: &str) {
            let mut stream = TcpStream::connect(("127.0.0.1", port))
                .await
                .expect("connect to hook listener");
            let request = format!(
                "POST {BW_HOOK_PATH} HTTP/1.1\r\n\
                 Host: 127.0.0.1:{port}\r\n\
                 Content-Type: application/json\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\
                 \r\n\
                 {body}",
                body.len()
            );
            stream.write_all(request.as_bytes()).await.unwrap();
            stream.flush().await.unwrap();
            // Read the response (don't care about content, just drain).
            let mut buf = [0u8; 256];
            let _ = timeout(Duration::from_secs(2), stream.read(&mut buf)).await;
        }

        // SessionStart event.
        let payload =
            r#"{"hook_event_name":"SessionStart","session_id":"abc-123","cwd":"/tmp/test"}"#;
        post_hook(port, payload).await;

        // Stop event.
        let payload2 = r#"{"hook_event_name":"Stop","session_id":"abc-123","cwd":"/tmp/test"}"#;
        post_hook(port, payload2).await;

        // Verify the events were received.
        let ev1 = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for SessionStart")
            .expect("SessionStart event");
        match ev1 {
            HookEvent::SessionStart { session_id, cwd } => {
                assert_eq!(session_id, "abc-123");
                assert_eq!(cwd, "/tmp/test");
            }
            _ => panic!("expected SessionStart, got {ev1:?}"),
        }

        let ev2 = timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for Stop")
            .expect("Stop event");
        match ev2 {
            HookEvent::Stop { session_id, cwd } => {
                assert_eq!(session_id, "abc-123");
                assert_eq!(cwd, "/tmp/test");
            }
            _ => panic!("expected Stop, got {ev2:?}"),
        }
    }
}
