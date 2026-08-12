//! `.bw/project.toml` — the project *intent* source of truth (V2-② Phase A,
//! §3.3/§6): a project's name / kind / brief / benchmark / opportunity live in
//! the git workspace and get PR-reviewed like code, parallel to
//! `.bw/metrics.toml`. This module only reads + parses the file into typed
//! Rust data — deciding what to do with it (upsert into SQLite, diff against
//! the cache, …) is the caller's job. Same "collector never interprets" idiom
//! as [`crate::metrics_file`]: read-only, real output, no fabrication.
//!
//! North star is **not** here — it lives in `.bw/metrics.toml` (its own
//! source of truth, synced by `SyncMetricsFile`). `project.toml` carries only
//! the five intent fields a creation flow collects.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Workspace-relative path to the project intent source-of-truth file.
pub const PROJECT_FILE_REL_PATH: &str = ".bw/project.toml";

#[derive(Debug, thiserror::Error)]
pub enum ProjectFileError {
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

/// The parsed `.bw/project.toml` file — the five intent fields a creation flow
/// collects, mirrored as the project row's `name`/`kind`/`descr`/`benchmark`/
/// `opportunity` columns. `deny_unknown_fields` (same idiom as
/// `connectors_file.rs`): a typo'd top-level key must fail loudly, not
/// silently parse into a partial file that then upserts fewer columns than
/// the author intended.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub name: String,
    pub kind: String,
    /// Maps to the project row's `descr` column (the "brief" / "你想做什么"
    /// free-text from the Intent card).
    #[serde(default)]
    pub brief: String,
    #[serde(default)]
    pub benchmark: String,
    #[serde(default)]
    pub opportunity: String,
}

/// Parse a `.bw/project.toml` body already in memory (local file or remote
/// raw fetch). Same shape / deny-unknown-fields rules as [`read`].
pub fn parse(raw: &str) -> Result<ProjectFile, ProjectFileError> {
    toml::from_str(raw).map_err(|e| ProjectFileError::Parse {
        path: PROJECT_FILE_REL_PATH.into(),
        source: e,
    })
}

/// Read + parse `<workspace>/.bw/project.toml`.
///
/// `Ok(None)` covers every "nothing to sync" case — an unconfigured
/// (empty) workspace, or a real workspace that simply has no file yet — an
/// honest no-op, not an error (mirrors [`crate::metrics_file::read`]'s
/// "not there yet" idiom). Any other IO failure, or a file that exists but
/// fails to parse/validate against the shape above, is `Err`: the caller must
/// not write a partial cache from a half-broken file — parse succeeds in full
/// or nothing is synced.
pub fn read(workspace: &str) -> Result<Option<ProjectFile>, ProjectFileError> {
    if workspace.trim().is_empty() {
        return Ok(None);
    }
    let path = Path::new(workspace).join(PROJECT_FILE_REL_PATH);
    let display = path.display().to_string();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ProjectFileError::Io {
                path: display,
                source: e,
            })
        }
    };
    match parse(&raw) {
        Ok(f) => Ok(Some(f)),
        Err(ProjectFileError::Parse { source, .. }) => Err(ProjectFileError::Parse {
            path: display,
            source,
        }),
        Err(e) => Err(e),
    }
}

/// Serialize a [`ProjectFile`] to a TOML string — the inverse of [`read`],
/// used by the creation flow to write `.bw/project.toml` into the workspace
/// (first-comer). The same struct owns both directions so the format stays
/// in one place.
pub fn render(file: &ProjectFile) -> Result<String, toml::ser::Error> {
    toml::to_string_pretty(file)
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
    fn read_valid_file_with_all_fields() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("project.toml"),
            r#"name = "增长实验看板"
kind = "看板 / 网页应用"
brief = "把 agent 会话里长出的工作流沉淀成可复用资产"
benchmark = "Linear"
opportunity = "被持续复用、效率可量化提升"
"#,
        )
        .unwrap();

        let file = read(&tmp.to_string_lossy())
            .expect("valid file parses")
            .expect("file exists");
        assert_eq!(file.name, "增长实验看板");
        assert_eq!(file.kind, "看板 / 网页应用");
        assert_eq!(file.brief, "把 agent 会话里长出的工作流沉淀成可复用资产");
        assert_eq!(file.benchmark, "Linear");
        assert_eq!(file.opportunity, "被持续复用、效率可量化提升");
    }

    #[test]
    fn read_file_with_default_optional_fields() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        // brief/benchmark/opportunity omitted — should default to empty.
        std::fs::write(
            bw_dir.join("project.toml"),
            r#"name = "minimal"
kind = "其他"
"#,
        )
        .unwrap();

        let file = read(&tmp.to_string_lossy())
            .expect("valid file parses")
            .expect("file exists");
        assert_eq!(file.name, "minimal");
        assert_eq!(file.kind, "其他");
        assert_eq!(file.brief, "");
        assert_eq!(file.benchmark, "");
        assert_eq!(file.opportunity, "");
    }

    #[test]
    fn read_unknown_field_fails() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("project.toml"),
            r#"name = "x"
kind = "其他"
north_star = "should not be here"
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
            bw_dir.join("project.toml"),
            r#"kind = "其他"
"#,
        )
        .unwrap();

        let result = read(&tmp.to_string_lossy());
        assert!(result.is_err());
    }

    #[test]
    fn read_missing_kind_fails() {
        let tmp = tempfile_dir();
        let bw_dir = tmp.join(".bw");
        std::fs::create_dir_all(&bw_dir).unwrap();
        std::fs::write(
            bw_dir.join("project.toml"),
            r#"name = "x"
"#,
        )
        .unwrap();

        let result = read(&tmp.to_string_lossy());
        assert!(result.is_err());
    }

    #[test]
    fn parse_accepts_in_memory_body() {
        let f = parse(
            r#"name = "cowelink"
kind = "其他"
brief = "hello"
opportunity = "win"
"#,
        )
        .expect("parse");
        assert_eq!(f.name, "cowelink");
        assert_eq!(f.opportunity, "win");
    }

    /// Create a temp directory for test isolation.
    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "bw-project-file-test-{}-{}",
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
