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
//! - [`ids`] — opaque newtype identities (`ProjectId`, `WorkflowId`, …).
//! - [`model`] — the entity graph (`Project` / `OpStage` / `Routine` / …),
//!   modelled so illegal states are unrepresentable.
//! - [`derive`] — the metric→signal→health chain + the sealed [`derive::Derived`].

pub mod analysis;
pub mod derive;
pub mod ids;
pub mod model;
pub mod playbook;
pub mod standards;

pub use ids::{
    AgentId, ArtifactId, ConnectorId, CronTaskId, IssueId, KnowledgeSourceId, MetricId, ProjectId,
    RoutineId, SessionId, SkillId, WorkflowId, WorkflowRunId,
};
pub use model::*;
