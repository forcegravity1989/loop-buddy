//! `.bw/connectors.toml` — the script-connector *source of truth* (V1 Issue2
//! Phase 3, §3.2): a project's script-connector definitions live in the git
//! workspace and get PR-reviewed like code, parallel to `.bw/metrics.toml`.
//! This module only reads + parses the file into typed Rust data — deciding
//! what to do with it (upsert into SQLite, diff against the cache, …) is the
//! caller's job. Same "collector never interprets" idiom as
//! [`crate::metrics_file`]: read-only, real output, no fabrication.
//!
//! Format is documented in `docs/connectors-toml-format.md` — that doc is the
//! 绑数据 Skill's contract; this module is its parser.
//!
//! ## collect_kind context (§0 心智模型)
//!
//! `collect_kind` has two real kinds: `script` (automated — a script
//! mechanically parses a data source and produces JSON; `collect_query` is
//! the dotted field path into that JSON) and `manual` (hand-typed, wears a
//! 「手填」badge). `github`/`codehub`/`bw`/`connector` are legacy inline
//! arms that are retiring into `script` — this file only defines `script`
//! connectors; the legacy kinds live in the DB's inline collect arms and are
//! not part of this file's vocabulary.

use serde::Deserialize;
use std::path::Path;

/// Workspace-relative path to the connectors source-of-truth file.
pub const CONNECTORS_FILE_REL_PATH: &str = ".bw/connectors.toml";

#[derive(Debug, thiserror::Error)]
pub enum ConnectorsFileError {
    #[error("读取 {path} 失败:{source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("解析 {path} 失败:{source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
}

/// One connector definition from `.bw/connectors.toml`. V1 only supports
/// `kind = "script"` (the `script` collect_kind's backing connector — a
/// script that mechanically parses a real data source into JSON, which
/// buddy shell-outs at collect time). Other kinds (`github`/`codehub`/`bw`/
/// `connector`) are legacy inline arms retiring into `script` (§0 §3.1) —
/// they are not part of this file's vocabulary; a `kind` other than
/// `"script"` is a parse error (not silently ignored).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorDef {
    pub name: String,
    /// Only `"script"` is accepted. The fixed vocabulary is enforced at
    /// parse time — an unknown kind fails the whole file (same "no silent
    /// acceptance" idiom as `CollectKind`).
    pub kind: ConnectorKind,
    /// Path to the collection script, relative to the workspace root (or
    /// `.bw/` — both are resolved by the caller). E.g.
    /// `scripts/derive_leading.py` or `governance/derive_leading.py`.
    pub script: String,
    /// Command to run the script (`python` / `ts-node` / `node` …). Empty
    /// defaults to `python` at collect time (matches `ScriptConnectorConfig`'s
    /// default in bw-app).
    #[serde(default)]
    pub command: String,
    /// Output file path (relative to workspace root) or field structure
    /// description. buddy reads this after running the script to get the
    /// JSON from which `collect_query` field paths are extracted.
    #[serde(default)]
    pub output: String,
}

/// The fixed kind vocabulary for `.bw/connectors.toml`. V1 only `Script`;
/// `Github`/`Codehub`/`Bw`/`Connector` are legacy inline arms (not in this
/// file's vocabulary). Unknown strings fail to parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    /// A project-side collection script (e.g. `derive_*.py` that
    /// mechanically parses a real data source into a `data.json`). buddy
    /// shell-outs the script and reads the named field — the script's own
    /// deps (Playwright/SSO/…) are the project's responsibility.
    Script,
}

impl ConnectorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ConnectorKind::Script => "script",
        }
    }
}

/// The parsed `.bw/connectors.toml` file.
///
/// `deny_unknown_fields` (P5, 2026-08-06 real-world incident): without it, an
/// agent writing `[[connectors]]` (plural — an easy typo against the
/// singular `[[connector]]` array-table header below) parses "successfully"
/// into an empty `connectors: Vec` — `sync_connectors_file_for` then upserts
/// zero rows with no error, silently as if the file had genuinely defined no
/// connectors. Denying unknown top-level keys turns that into a loud parse
/// error naming the bad key instead.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectorsFile {
    /// `[[connector]]` array entries. Empty is valid (a file with zero
    /// connectors — honest no-op, not an error). The TOML key is `connector`
    /// (singular, matching the `[[connector]]` array-table header); the
    /// Rust field is `connectors` for readability.
    #[serde(rename = "connector", default)]
    pub connectors: Vec<ConnectorDef>,
}

/// Read + parse `<workspace>/.bw/connectors.toml`.
///
/// `Ok(None)` covers every "nothing to sync" case — an unconfigured
/// (empty) workspace, or a real workspace that simply has no file yet — an
/// honest no-op, not an error (mirrors [`crate::metrics_file::read`]'s
/// "not there yet" idiom). Any other IO failure, or a file that exists but
/// fails to parse/validate against the shape above, is `Err`: the caller
/// must not write a partial cache from a half-broken file — parse succeeds
/// in full or nothing is synced.
pub fn read(workspace: &str) -> Result<Option<ConnectorsFile>, ConnectorsFileError> {
    if workspace.trim().is_empty() {
        return Ok(None);
    }
    let path = Path::new(workspace).join(CONNECTORS_FILE_REL_PATH);
    let display = path.display().to_string();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ConnectorsFileError::Io {
                path: display,
                source: e,
            })
        }
    };
    toml::from_str(&raw)
        .map(Some)
        .map_err(|e| ConnectorsFileError::Parse {
            path: display,
            source: e,
        })
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_missing_file_returns_none() {
        let tmp = tempfile_dir();
        let result = read(&tmp.to_string_lossy()).expect("missing file is Ok(None)");
        assert!(result.is_none());
    }

    #[test]
    fn read_empty_workspace_returns_none() {
        let result = read("").expect("empty workspace is Ok(None)");
        assert!(result.is_none());
    }

    #[test]
    fn read_valid_file_with_one_script_connector() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("connectors.toml"),
            r#"[[connector]]
name = "leading-indicators"
kind = "script"
script = "scripts/derive_leading.py"
command = "python"
output = "data.json"
"#,
        )
        .unwrap();

        let file = read(&tmp.to_string_lossy())
            .expect("valid file parses")
            .expect("file exists");
        assert_eq!(file.connectors.len(), 1);
        let c = &file.connectors[0];
        assert_eq!(c.name, "leading-indicators");
        assert_eq!(c.kind, ConnectorKind::Script);
        assert_eq!(c.script, "scripts/derive_leading.py");
        assert_eq!(c.command, "python");
        assert_eq!(c.output, "data.json");
    }

    #[test]
    fn read_file_with_default_command_and_output() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        // command and output omitted — should default to empty strings.
        std::fs::write(
            bw_dir.join("connectors.toml"),
            r#"[[connector]]
name = "minimal"
kind = "script"
script = "governance/derive_leading.py"
"#,
        )
        .unwrap();

        let file = read(&tmp.to_string_lossy())
            .expect("valid file parses")
            .expect("file exists");
        assert_eq!(file.connectors.len(), 1);
        let c = &file.connectors[0];
        assert_eq!(c.name, "minimal");
        assert_eq!(c.kind, ConnectorKind::Script);
        assert_eq!(c.script, "governance/derive_leading.py");
        assert_eq!(c.command, "");
        assert_eq!(c.output, "");
    }

    #[test]
    fn read_file_with_multiple_connectors() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("connectors.toml"),
            r#"[[connector]]
name = "north-star"
kind = "script"
script = "scripts/derive_ns.py"
output = "ns.json"

[[connector]]
name = "lagging"
kind = "script"
script = "scripts/derive_lagging.py"
command = "node"
output = "lagging.json"
"#,
        )
        .unwrap();

        let file = read(&tmp.to_string_lossy())
            .expect("valid file parses")
            .expect("file exists");
        assert_eq!(file.connectors.len(), 2);
        assert_eq!(file.connectors[0].name, "north-star");
        assert_eq!(file.connectors[1].name, "lagging");
        assert_eq!(file.connectors[1].command, "node");
    }

    #[test]
    fn read_empty_file_is_zero_connectors() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(bw_dir.join("connectors.toml"), "").unwrap();

        let file = read(&tmp.to_string_lossy())
            .expect("valid file parses")
            .expect("file exists");
        assert!(file.connectors.is_empty());
    }

    #[test]
    fn read_unknown_kind_fails() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("connectors.toml"),
            r#"[[connector]]
name = "bad"
kind = "github"
script = "scripts/x.py"
"#,
        )
        .unwrap();

        let result = read(&tmp.to_string_lossy());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("解析"));
    }

    #[test]
    fn read_plural_key_typo_fails_loud_not_silent_empty() {
        // P5 real-world incident (2026-08-06): an agent wrote `[[connectors]]`
        // (plural) instead of `[[connector]]`. Before `deny_unknown_fields`,
        // this parsed to `Ok(Some(ConnectorsFile { connectors: vec![] }))` —
        // a silent zero-connector "success". It must now be a loud `Err`.
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("connectors.toml"),
            r#"[[connectors]]
name = "leading-indicators"
kind = "script"
script = "scripts/derive_leading.py"
output = "data.json"
"#,
        )
        .unwrap();

        let result = read(&tmp.to_string_lossy());
        assert!(
            result.is_err(),
            "plural [[connectors]] typo must fail parsing, not silently yield zero connectors"
        );
    }

    #[test]
    fn read_unknown_field_in_connector_def_fails() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("connectors.toml"),
            r#"[[connector]]
name = "typo-field"
kind = "script"
script = "scripts/x.py"
outptu = "data.json"
"#,
        )
        .unwrap();

        let result = read(&tmp.to_string_lossy());
        assert!(
            result.is_err(),
            "unknown field must fail, not be silently dropped"
        );
    }

    #[test]
    fn read_missing_name_fails() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("connectors.toml"),
            r#"[[connector]]
kind = "script"
script = "scripts/x.py"
"#,
        )
        .unwrap();

        let result = read(&tmp.to_string_lossy());
        assert!(result.is_err());
    }

    /// Create a temp directory for test isolation.
    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bw-connectors-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
