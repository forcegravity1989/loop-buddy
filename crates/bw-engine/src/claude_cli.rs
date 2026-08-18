//! Process-wide configuration for the `claude` CLI the interactive executor
//! spawns. Editable from the Settings hub via `Command::SetClaudeConfig`;
//! seeded from `BW_CLAUDE_BIN` at boot. The one-shot `claude -p` executor
//! (with its per-call budget cap and permission modes) was deleted with the
//! phase-loop engine on 2026-08-18 — interactive sessions are user-paced and
//! always run `--dangerously-skip-permissions` (see `build_startup_plan`), so
//! those knobs had nothing left to configure and were removed rather than
//! shown as settings that do nothing.

/// Process-wide configuration (per-project data such as the workspace path is
/// runtime data read from a `ProjectRow`, not something fixed at startup).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClaudeCliConfig {
    /// Override the `claude` binary path/name. `None` → resolved from `PATH`.
    pub binary: Option<String>,
}
