//! `bw-core` — Builders' Workbench domain kernel.
//!
//! Two non-negotiables (plan `00 §3`) live here as *types*, not conventions:
//!
//! 1. **UI-agnostic.** Only `serde` / `time` / `uuid` / `thiserror`. No async,
//!    no IO, never `use dioxus` / `use tauri`. The kernel also stays
//!    `wasm32`-compilable (CI keepalive) so a future Web shell reuses it
//!    unchanged.
//! 2. **Health is always derived, never hand-set.** A [`Signal`] can only land
//!    in a cache field wrapped in [`derive::Derived`], whose sole constructors
//!    live inside [`mod@derive`]. See [`derive`] for the 6-layer chain
//!    L0 Observation → L6 Project signal.
//!
//! The split:
//! - [`ids`] — opaque newtype identities (`ProjectId`, `IssueId`, …).
//! - [`model`] — the entity graph (`Signal` / `StageKind` / `IssueStatus` /
//!   `RunState` / …), modelled so illegal states are unrepresentable.
//! - [`derive`] — the metric→signal→health chain + the sealed [`derive::Derived`].
//! - [`playbook`] — role playbook context + template rendering
//!   (`PlaybookCtx`/`render`), consumed by `bw-engine`'s interactive session
//!   setup.
//!
//! next 减法专项(2026-08,死代码审计):v1 整包移植带进来的退役概念族
//! (旧工作流引擎构建管线/Hub 目录/Skill·Agent Card/旧 Cron/旧 Connector/旧
//! Artifact/旧 Issue 等零消费者类型,连同专属承载它们的
//! `analysis`/`bw_library`/`scope`/`skill_body`/`skill_spec`/`stage_catalog`/
//! `standards` 七个整文件)已删除——逐符号 grep 复核零消费者,详见该批
//! commit 正文。仓规撑腰:「发现过时的实现路径,直接移除它」。

pub mod derive;
pub mod ids;
pub mod model;
pub mod playbook;

pub use ids::{
    ArtifactId, ConnectorId, ConversationId, HandoffId, IssueId, MetricId, ObservationId,
    ProjectId, RunId,
};
pub use model::*;
