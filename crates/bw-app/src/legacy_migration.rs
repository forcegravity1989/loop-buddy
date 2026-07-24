//! T14 (2026-07-24, plan/12 §10 v1.1): the real-daily-DB, one-shot "存量库
//! 迁移" — clears empty-body OMC/ECC catalog shells the pre-T1 `seed.rs`
//! planted (see that file's own doc comment: T1 stopped *seeding* new shells
//! but never touched the ones already sitting in a real user's DB) and
//! auto-imports the real content that replaced them (T2/T3's two on-disk
//! skill libraries + T5's vendored ECC agents). Isolated in its own module
//! for the same reason `skill_import.rs`/`agent_import.rs` are: this is
//! `bw-app`'s one real filesystem/OS-clock-touching corner, kept out of the
//! big `Command` match arm in `lib.rs`.
//!
//! Business judgement (which rows are safe to delete) also lives here/in
//! `lib.rs`'s dispatch handler, never in `bw-store` — this repo's "store 无
//! 业务判断" rule (CLAUDE.md).
//!
//! T16.5 (2026-07-24, GH#54): a second, independent piece of business
//! judgement lives in this module too — which `workflow_spec` rows are safe
//! to have their `phases`/`phase_prompts` overwritten with the current
//! playbook's real values. See [`matching_template_kind`] and
//! [`is_pure_legacy_phases`].
//!
//! T14.5 (2026-07-24, GH#59): a third, independent piece — which
//! `workflow_spec` rows are directory-import (ECC/Adopted) catalog shells
//! safe to delete outright (not just have a column overwritten). Parent
//! ticket to T14: T14 only ever looked at `skill`/`agent` shells, but a real
//! daily DB's `workflow_spec` table carries the exact same kind of stale
//! zero-trace catalog-import row (92 ECC directory shells + 1 adopted, real
//! count on the DB this ticket targets) — 违反"mock 必须自我标注"纪律 if left
//! sitting there looking like real content. See
//! [`is_directory_import_source`] and [`workflow_uses`].

use std::path::{Path, PathBuf};
use time::OffsetDateTime;

/// Copy `db_path` to `<db_path>.bak-<yyyymmdd-hhmmss>` and return the backup
/// path. Plain `std::fs::copy` — see this ticket's own doc/commit message for
/// the WAL/consistency tradeoff this implies (backup happens *after*
/// `SqliteStore::open`'s additive-only schema migration has already run and
/// committed, not before any connection is ever opened, because the trigger
/// decision itself needs a live query against the store). Errors honestly
/// (never silently skips) — a caller that can't back up the real DB must not
/// proceed to delete anything from it.
pub(crate) fn backup_db_file(db_path: &str) -> Result<String, String> {
    let now = OffsetDateTime::now_utc();
    let stamp = format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    let backup_path = format!("{db_path}.bak-{stamp}");
    std::fs::copy(db_path, &backup_path)
        .map_err(|e| format!("备份数据库文件失败(source={db_path}, dest={backup_path}):{e}"))?;
    Ok(backup_path)
}

/// Find the highest-numbered version directory directly under `library_root`
/// that itself contains a `skills` subdirectory, and return that `skills`
/// path — i.e. resolve `~/.claude/plugins/cache/<publisher>/<plugin>/*/skills`
/// to the one real, currently-installed version's skills root. Plugin cache
/// version directories change over time (a version bump replaces the old
/// one), so a hardcoded version string would silently go stale; this scans
/// instead. `None` (not an error) when `library_root` doesn't exist at all or
/// no version subdirectory has a `skills` folder — the caller's job is to
/// skip honestly, never fail the whole migration over one missing local
/// plugin install.
pub(crate) fn resolve_versioned_skills_root(library_root: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(library_root).ok()?;
    let mut candidates: Vec<(Vec<u64>, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| p.join("skills").is_dir())
        .map(|p| {
            let key = version_sort_key(p.file_name().and_then(|n| n.to_str()).unwrap_or(""));
            (key, p)
        })
        .collect();
    // Highest version wins — component-wise numeric compare ("1.10.0" >
    // "1.2.0", unlike a bare string sort), falling back to directory-name
    // string order for any non-numeric component (defensive; every real
    // version directory observed on this machine is plain semver).
    candidates.sort_by(|a, b| a.0.cmp(&b.0));
    candidates.pop().map(|(_, p)| p.join("skills"))
}

fn version_sort_key(name: &str) -> Vec<u64> {
    name.split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

/// `~/.claude/plugins/cache/mattpocock/mattpocock-skills/*/skills` — `None`
/// if the plugin isn't installed on this machine (honest skip, not an
/// error).
pub(crate) fn mattpocock_skills_root() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    resolve_versioned_skills_root(&PathBuf::from(format!(
        "{home}/.claude/plugins/cache/mattpocock/mattpocock-skills"
    )))
}

/// `~/.claude/plugins/cache/superpowers-dev/superpowers/*/skills` — same
/// honest-skip contract as [`mattpocock_skills_root`].
pub(crate) fn superpowers_skills_root() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    resolve_versioned_skills_root(&PathBuf::from(format!(
        "{home}/.claude/plugins/cache/superpowers-dev/superpowers"
    )))
}

/// The 67 vendored real ECC (everything-claude-code) AGENT.md files this repo
/// commits at `crates/bw-store/vendor/ecc-agents/` (T5, plan/12 §3;
/// `f88bf62`). Resolved via `CARGO_MANIFEST_DIR` — a *build-time* constant
/// baked into the binary, the same tradeoff `examples/import_ecc_agents.rs`
/// already made and this ticket deliberately keeps consistent with rather
/// than switching to a compile-time `include_dir!` embed: this repo builds
/// and runs on one machine for one solo builder (CLAUDE.md: "不是云服务"),
/// so "the path that was true at `cargo build` time is still true at `cargo
/// run` time" holds in practice. Documented honestly as a real limitation:
/// this would break for a relocated/distributed binary shipped without the
/// source checkout — not this ticket's problem to solve (no such
/// distribution path exists yet), but not silently assumed away either.
pub(crate) fn ecc_agents_vendor_dir() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../bw-store/vendor/ecc-agents"
    ))
}

/// Every `*.md` file directly under `dir`, sorted — honest empty `Vec` (not
/// an error) if `dir` doesn't exist, so a missing vendor dir degrades the
/// same way a missing plugin-cache library does (skip + log, never abort the
/// whole migration).
pub(crate) fn list_md_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
        .collect();
    files.sort();
    files
}

/// The app_meta key marking "the T14 legacy-shell migration has run". Once
/// set, every future boot's check is this one cheap `get_app_meta` read —
/// no re-scan of skill/agent rows.
pub const LEGACY_MIGRATION_DONE_KEY: &str = "legacy_shells_migration_v1";

/// The app_meta key marking "T16.5's built-in-template phase refresh has
/// run" — deliberately its OWN key, not folded into
/// [`LEGACY_MIGRATION_DONE_KEY`], for two reasons documented in this
/// ticket's commit message:
///
/// 1. **The real user DB this ticket targets has never run `v1` at all**
///    (T14 hasn't shipped to it yet) — folding this step into the SAME flow
///    (both checked/stamped together inside one dispatch, see `lib.rs`'s
///    handler) means one real restart does both, honestly, in one pass.
/// 2. **A DB that already ran `v1`** (e.g. this ticket's own verification
///    copy, or any dev DB that migrated before this ticket landed) must
///    still be able to pick up the phase refresh on its own — a single
///    combined flag would make that DB's `legacy_shells_migration_v1=done`
///    permanently mask the new step. An independent flag lets the dispatch
///    handler check/run each piece on its own idempotent terms.
pub const TEMPLATE_PHASE_REFRESH_DONE_KEY: &str = "template_phase_refresh_v1";

/// T14.5 (2026-07-24, GH#59): the app_meta key marking "the workflow_spec
/// directory-import shell purge (Pass C) has run" — deliberately its OWN
/// key, same rationale [`TEMPLATE_PHASE_REFRESH_DONE_KEY`]'s own doc comment
/// already gives for being independent of [`LEGACY_MIGRATION_DONE_KEY`]: the
/// real DB this ticket targets has run neither pass yet, so one real restart
/// must do all three; a DB that already ran one or two of the three passes
/// (a dev DB migrated before this ticket landed, or this ticket's own
/// verification copy re-run after a partial state) must still be able to
/// pick up whichever pass it hasn't run, on that pass's own idempotent
/// terms, independent of the other two flags.
pub const WORKFLOW_SHELL_PURGE_DONE_KEY: &str = "workflow_shell_purge_v1";

/// T16.5: is `name` one of the five built-in stage templates
/// (`bw_core::model::stage_template_workflow`'s real produced name, e.g.
/// `「原型」标准工作流 · 原型师`)? Exact string match only — never a
/// `stage_ref`-alone match, so a user's own custom workflow that merely
/// happens to carry the same `stage_ref` is never a candidate. Returns the
/// matching [`bw_core::model::StageKind`] so the caller can pull that stage's
/// current [`bw_core::playbook::phase_metas`]/`rendered_phase_prompts`.
pub(crate) fn matching_template_kind(name: &str) -> Option<bw_core::model::StageKind> {
    bw_core::model::StageKind::ALL
        .into_iter()
        .find(|&kind| bw_core::model::stage_template_workflow(kind).name == name)
}

/// T16.5: is `phases` a *pure* legacy shape — every phase still exactly the
/// honest default ([`bw_core::model::PhaseMeta::neutral`]'s `role: Neutral,
/// agent: None, skills: []`) that either a pre-T8 plain-string-array row
/// deserializes into, or a row nobody has ever hand-edited/re-seeded since?
/// This is the sole "safe to overwrite" gate: a row carrying even one real
/// trace — a non-`Neutral` role, a bound agent, or a bound skill, any of
/// which only a real playbook refresh or a real human edit could have set —
/// is left alone, always. `false` on an empty slice too (nothing to refresh,
/// and a row that legitimately matches a template name by coincidence but
/// carries zero phases is not this migration's business to touch).
pub(crate) fn is_pure_legacy_phases(phases: &[bw_core::model::PhaseMeta]) -> bool {
    !phases.is_empty()
        && phases.iter().all(|p| {
            p.role == bw_core::model::PhaseRole::Neutral && p.agent.is_none() && p.skills.is_empty()
        })
}

/// T14.5 (GH#59): is `kind` a directory-imported catalog entry —
/// `HubSource::Official { .. }` (every real value observed on this machine
/// is `official_library: "ecc"`, but any curated-library tag counts the same
/// way) or `HubSource::Adopted` — as opposed to `HubSource::SelfBuilt` (the
/// five built-in stage templates, and any hand-authored workflow) or
/// `HubSource::WithinSession` (session-scoped, never a directory import)?
/// `WorkflowKind::Dynamic` carries no `source` at all and always returns
/// `false` — a session-scoped workflow was never a directory import to begin
/// with.
///
/// This is Pass C's *first* gate only — narrows the candidate set down to
/// "came from a directory import at all". A `true` result alone is never
/// sufficient to purge a row; see the dispatch handler in `lib.rs` for the
/// remaining independent gates (zero `workflow_run`, zero `uses`, not
/// referenced by a `run_workflow`-mode cron target, not a built-in template
/// name — belt-and-suspenders alongside this `source` check).
pub(crate) fn is_directory_import_source(kind: &bw_core::model::WorkflowKind) -> bool {
    match kind {
        bw_core::model::WorkflowKind::Static { source, .. } => matches!(
            source,
            bw_core::model::HubSource::Official { .. } | bw_core::model::HubSource::Adopted
        ),
        bw_core::model::WorkflowKind::Dynamic { .. } => false,
    }
}

/// T14.5 (GH#59): the real `uses` counter carried on a `Static` spec's
/// `WorkflowKind` — `0` for `Dynamic` (no such field; a session-scoped
/// workflow was never "used" as a catalog entry).
pub(crate) fn workflow_uses(kind: &bw_core::model::WorkflowKind) -> u32 {
    match kind {
        bw_core::model::WorkflowKind::Static { uses, .. } => *uses,
        bw_core::model::WorkflowKind::Dynamic { .. } => 0,
    }
}

/// The real tally a migration run leaves behind — every field independently
/// readable back from the DB (skill/agent row counts, `app_meta`), never just
/// asserted. Returned by the `bw-app` dispatch handler and carried on
/// `Event::LegacyShellsMigrated` for a UI/log subscriber.
#[derive(Debug, Default, Clone)]
pub struct LegacyMigrationReport {
    pub backup_path: Option<String>,
    pub deleted_skills: u32,
    pub kept_skills_with_trace: u32,
    pub deleted_agents: u32,
    pub kept_agents_with_trace: u32,
    pub imported_skills: u32,
    pub imported_agents: u32,
    /// Library roots that were expected but not found on this machine
    /// (honest skip, not a failure) — e.g. `"mattpocock-skills 未安装:
    /// ~/.claude/plugins/... 不存在"`.
    pub skipped_sources: Vec<String>,
    /// T16.5 (GH#54): built-in stage-template `workflow_spec` rows whose
    /// `phases`/`phase_prompts` were just overwritten with the current
    /// playbook's real values — real names, not just a count, so a log
    /// subscriber can say exactly which of the five templates changed.
    pub refreshed_templates: Vec<String>,
    /// T14.5 (GH#59): directory-import (ECC/Adopted) `workflow_spec` shells
    /// just deleted outright — real names, not just a count, same
    /// "log subscriber can say exactly which ones" contract
    /// `refreshed_templates` already gives.
    pub purged_workflows: Vec<String>,
}
