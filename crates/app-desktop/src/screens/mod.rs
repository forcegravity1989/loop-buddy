//! Workspace screens, one per [`bw_app::View`]. Each reads the [`crate::bridge::ViewModel`]
//! from context and sends [`bw_app::Command`]s via the [`crate::bridge::CommandBus`]
//! — no screen touches the `Store` or `App` directly. That seam is the contract
//! P2-B (wizard) and P2-C (ops) build against.

pub mod ops;
pub mod projects;
pub mod wizard;
