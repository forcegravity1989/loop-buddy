//! `.bw/metrics.toml` — the metrics *source of truth* (plan/13 D5+D6): a
//! project's north-star + lagging + leading metric definitions, each with a
//! collection plan, live in the git workspace and get PR-reviewed like code.
//! This module only reads + parses the file into typed Rust data — deciding
//! what to do with it (upsert into SQLite, diff against the cache, …) is the
//! caller's job. Same "collector never interprets" idiom as [`crate::evidence`]:
//! read-only, real output, no fabrication.
//!
//! Format is documented (with a fully-commented sample) in
//! `docs/metrics-toml-format.md` — that doc is the Skill contract the 找指标
//! Skill (a later ticket) writes against; this module is its parser.

use serde::Deserialize;
use std::path::Path;

/// Workspace-relative path to the metrics source-of-truth file.
pub const METRICS_FILE_REL_PATH: &str = ".bw/metrics.toml";

#[derive(Debug, thiserror::Error)]
pub enum MetricsFileError {
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

/// How a metric's value is meant to be collected — a fixed vocabulary (D6).
/// A `.bw/metrics.toml` with any other `kind` string fails to parse rather
/// than silently accepting an uncollectible plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectKind {
    /// A GitHub query (issues/PRs/releases/…) — consumed by a later
    /// GitHub-collector ticket (C7), not executed here.
    Github,
    /// A configured BW Connector probe.
    Connector,
    /// BW's own bookkeeping (issue settle-count, run telemetry, …) — no
    /// external system involved.
    Bw,
    /// Hand-typed; no collector will ever fill this automatically.
    Manual,
}

impl CollectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CollectKind::Github => "github",
            CollectKind::Connector => "connector",
            CollectKind::Bw => "bw",
            CollectKind::Manual => "manual",
        }
    }
}

/// One metric's collection plan — every metric in the file (north star
/// included) must carry one: "每条指标必附采集方案" (D6). `query` is
/// kind-specific free text (a GitHub search query, a connector name, a BW
/// bookkeeping key, …); empty is only meaningful for `Manual`, but the
/// parser doesn't enforce that split — an under-specified non-manual plan is
/// a *content* problem for the 找指标/绑数据 skills to fix later, not a
/// structural parse failure here.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CollectPlan {
    pub kind: CollectKind,
    #[serde(default)]
    pub query: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NorthStarDef {
    pub name: String,
    #[serde(default)]
    pub def: String,
    pub collect: CollectPlan,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricDef {
    pub name: String,
    #[serde(default)]
    pub def: String,
    /// Mini-DSL target, same vocabulary as the store's `metric.target_raw`
    /// (`"≥5"` `"≤24h"` `"清零"` …). Optional — 找指标 v1 may draft a metric
    /// before a target is agreed.
    #[serde(default)]
    pub target: String,
    pub collect: CollectPlan,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsFile {
    #[serde(default)]
    pub schema_version: u32,
    pub north_star: NorthStarDef,
    #[serde(default)]
    pub lagging: Vec<MetricDef>,
    #[serde(default)]
    pub leading: Vec<MetricDef>,
}

/// Read + parse `<workspace>/.bw/metrics.toml`.
///
/// `Ok(None)` covers every "nothing to sync" case — an unconfigured
/// (empty) workspace, or a real workspace that simply has no file yet — an
/// honest no-op, not an error (mirrors [`crate::evidence`]'s "not there yet"
/// idiom). Any other IO failure, or a file that exists but fails to
/// parse/validate against the shape above, is `Err`: the caller must not
/// write a partial cache from a half-broken file — parse succeeds in full or
/// nothing is synced.
pub fn read(workspace: &str) -> Result<Option<MetricsFile>, MetricsFileError> {
    if workspace.trim().is_empty() {
        return Ok(None);
    }
    let path = Path::new(workspace).join(METRICS_FILE_REL_PATH);
    let display = path.display().to_string();
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(MetricsFileError::Io {
                path: display,
                source: e,
            })
        }
    };
    toml::from_str(&raw)
        .map(Some)
        .map_err(|e| MetricsFileError::Parse {
            path: display,
            source: e,
        })
}
