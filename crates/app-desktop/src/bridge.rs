//! The Event→Signal bridge + async `App` host — the headline integration risk
//! of P2 (plan `04 §P2`).
//!
//! ## Why this shape
//! [`bw_app::App`] is `!Sync` and its [`dispatch`](bw_app::App::dispatch) takes
//! `&mut self`, so it must **not** be shared across components behind a lock.
//! Instead a single Dioxus [`use_coroutine`] task *owns* the `App` for the life
//! of the window. That coroutine's input channel **is** the command bus: any
//! component reads the [`Coroutine<Command>`] from context and `send`s a
//! [`Command`] into it. The coroutine `dispatch`es it (the only place `&mut App`
//! exists), then recomputes a render-ready [`ViewModel`] from `app.snapshot()`
//! and pushes it into a [`Signal<ViewModel>`] that every component reads.
//!
//! ## No over-render
//! [`ViewModel`] (and its parts) are `Clone + PartialEq`. Dioxus compares the new
//! VM against the old one and skips re-rendering subtrees whose inputs are
//! unchanged — so dispatching e.g. `SetPanel` doesn't redraw the project list.
//! Keep every field of the VM `PartialEq` to preserve that.
//!
//! ## Streaming hook (later phases)
//! For P2-A, recompute-after-dispatch is sufficient and correct: every command
//! goes through the store, so re-reading the snapshot reflects the new truth.
//! `App::subscribe()` exposes a broadcast [`bw_app::Event`] stream — the seam for
//! P2 *streaming* screens (chat token-by-token, live workflow progress). When a
//! later agent wants incremental UI, spawn a second task that
//! `app.subscribe()`s and patches the VM per event, instead of (or alongside)
//! the full recompute here. Do not build that now.

use std::path::PathBuf;
use std::sync::Arc;

use bw_app::{App, Command, Panel, Scope, View};
// NOTE: deliberately do NOT import `bw_core::Signal` by its bare name — this
// module's `use dioxus::prelude::*` brings Dioxus's own `Signal<T>` into scope
// (the VM signal), and a bare `Signal` import would shadow it. The bw-core
// enum is referenced fully-qualified as `bw_core::Signal` below.
use bw_core::model::{ProjectPhase, StageKind, StagePhase};
use bw_core::ProjectId;
use bw_engine::{Engine, MockExecutor};
use bw_store::{ProjectRow, SqliteStore};
use dioxus::prelude::*;
use futures_util::StreamExt;

/// One project card, fully derived from a [`ProjectRow`] so screens render
/// strings and never re-run business logic. `PartialEq` lets the wall skip
/// re-render when nothing about a card changed.
#[derive(Clone, PartialEq, Debug)]
pub struct ProjectCardVM {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
    pub phase: ProjectPhase,
    /// Derived L6 health (already a [`bw_core::Signal`]; render with
    /// [`ui::signal_color`] — `Unknown` is grey, never green).
    pub signal: bw_core::Signal,
    /// Cold-start wizard step (1..=7) when `phase == ColdStart`.
    pub cold_step: u8,
    /// 0..=100, derived. Cold projects show wizard progress (`step/7`); running
    /// projects show mean stage progress (0 until ops work lands — honest).
    pub progress: u8,
}

impl ProjectCardVM {
    /// Pure projection `ProjectRow` → card. Centralised here so the screen stays
    /// declarative and this stays unit-testable.
    fn from_row(r: &ProjectRow) -> Self {
        let cold_step = r.cold_step.unwrap_or(0);
        let progress = match r.phase {
            // Wizard progression as a 0..100 bar (step 0..=7 of 7).
            ProjectPhase::ColdStart => ((u16::from(cold_step).min(7) * 100) / 7) as u8,
            // Running: mean of stage progresses. `ProjectRow` doesn't carry the
            // per-stage rows (no N+1 query on the wall), and stages are
            // materialized at 0, so a freshly-completed project honestly reads 0%
            // here until ops progress is recorded in the operating view (P2-C).
            ProjectPhase::Running => ui::overall_progress(&[]),
        };
        Self {
            id: r.id,
            name: r.name.clone(),
            kind: r.kind.clone(),
            desc: r.desc.clone(),
            phase: r.phase,
            // Read-only: the cache `recompute_signals` wrote. Never fabricated.
            signal: r.signal.unwrap_or(bw_core::Signal::Unknown),
            cold_step,
            progress,
        }
    }
}

// ───────────────────────────── ops view models ─────────────────────────────
//
// The operating view (`View::App`) renders entirely off these. Every `signal`
// here is READ from the persisted derive cache (`persisted_signals`) — joined to
// the stage/metric definition by `StageKind` / name. Trends are the real
// observation series from `metric_trends`; nothing is fabricated. All
// `Clone + PartialEq` so the VM diff still skips unchanged ops subtrees.

/// One KPI under a stage: its definition (name/value/target), the derived signal
/// + hit (read, never set), and its real observation `trend` for a sparkline.
#[derive(Clone, PartialEq, Debug)]
pub struct OpsMetricVM {
    pub name: String,
    /// Latest display value (`"8"`, `"60%"`, `"842ms"`), or empty if unobserved.
    pub value_raw: String,
    pub target_raw: String,
    /// Derived L3 cache; `Unknown` when no data (never green).
    pub signal: bw_core::Signal,
    /// `signal == Green` per the derive (cache), `None` if not yet computed.
    pub hit: Option<bool>,
    /// Real recent observations, oldest→newest. `< 2` points ⇒ a flat sparkline.
    pub trend: Vec<f32>,
    /// All wizard metrics are Manual in P2 → the prototype's "手填 · 未接入度量源"
    /// honesty badge. Carried so the UI doesn't have to assume.
    pub manual: bool,
}

/// One control point in the operating view: its definition + derived health +
/// its metrics + a representative real trend for the big "进度趋势" sparkline.
#[derive(Clone, PartialEq, Debug)]
pub struct OpsStageVM {
    pub kind: StageKind,
    /// 1-based control-point number (1..=7).
    pub index: u8,
    pub label: String,
    pub phase: StagePhase,
    /// L5 stage health = the persisted routine signal (read-only).
    pub signal: bw_core::Signal,
    pub progress: u8,
    pub owns: String,
    pub accept: String,
    pub control: String,
    pub metrics: Vec<OpsMetricVM>,
    /// The stage's representative real series (its first metric's observation
    /// trend) for the big trend sparkline + WoW. Empty when the stage has no
    /// observed metric — the UI then shows only current progress (honest).
    pub trend: Vec<f32>,
}

/// The render-ready operating view: all seven stages + the one in scope.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct OpsVM {
    pub stages: Vec<OpsStageVM>,
    /// The stage the current `Scope::Stage(n)` points at, if any.
    pub active: Option<OpsStageVM>,
}

/// The single render-ready state every screen reads from context. Extensible:
/// P2-B/P2-C add wizard / ops fields here without touching the bridge plumbing.
#[derive(Clone, PartialEq, Debug)]
pub struct ViewModel {
    pub view: View,
    pub panel: Panel,
    pub scope: Scope,
    pub wizard_step: u8,
    pub active_project: Option<ProjectId>,
    /// Render-ready project wall.
    pub projects: Vec<ProjectCardVM>,
    /// Render-ready operating view (empty unless `view == App`).
    pub ops: OpsVM,
}

impl Default for ViewModel {
    fn default() -> Self {
        Self {
            view: View::Projects,
            panel: Panel::Progress,
            scope: Scope::All,
            wizard_step: 0,
            active_project: None,
            projects: Vec::new(),
            ops: OpsVM::default(),
        }
    }
}

/// Context handle: send a [`Command`] into the App-host coroutine. Stored via
/// `use_context_provider` in [`crate::Root`]; read with
/// `use_context::<CommandBus>()`. Other agents import this exact type.
pub type CommandBus = Coroutine<Command>;

/// Build a fresh [`ViewModel`] from the App's current snapshot and publish it.
/// Called once on startup and after every dispatched command.
///
/// The nav fields (`view`/`panel`/`scope`/`wizard_step`/`active_project`) come
/// from [`App::snapshot`] — the authoritative in-memory state the kernel just
/// mutated. The project list is read straight from the store so the wall always
/// mirrors the DB, independent of which commands happen to call the kernel's
/// internal `refresh_projects` (e.g. `OpenProject`/`BackToProjects` don't).
async fn rebuild_vm(app: &App<MockExecutor>, vm: &mut Signal<ViewModel>) {
    let s = app.snapshot();
    let mut next = ViewModel {
        view: s.view,
        panel: s.panel,
        scope: s.scope,
        wizard_step: s.wizard_step,
        active_project: s.active_project,
        projects: Vec::new(),
        ops: OpsVM::default(),
    };
    match app.store().list_projects().await {
        Ok(rows) => next.projects = rows.iter().map(ProjectCardVM::from_row).collect(),
        Err(e) => eprintln!("list_projects: {e}"),
    }
    // Operating view: only build the (potentially N+1-query) ops VM when we're
    // actually on the App screen with a project open. Otherwise it stays empty,
    // so the wall/wizard pay nothing for it.
    if next.view == View::App {
        if let Some(pid) = next.active_project {
            match build_ops(app.store().as_ref(), pid, next.scope).await {
                Ok(ops) => next.ops = ops,
                Err(e) => eprintln!("build_ops: {e}"),
            }
        }
    }
    // VM is PartialEq, so an unchanged dispatch won't churn the component tree.
    vm.set(next);
}

/// Assemble the operating [`OpsVM`] from the store: stage definitions
/// (`stage_details`) joined with the derived caches (`persisted_signals`) and the
/// real observation series (`metric_trends`).
///
/// Honesty invariants preserved here:
/// * every `signal` is **read** from `persisted_signals` (the derive cache) —
///   never recomputed or set in the UI; a stage/metric with no cache reads
///   `Unknown` (grey, never green);
/// * every `trend` is the real observation series — a metric with one
///   observation yields a one-point (flat) sparkline.
async fn build_ops(
    store: &dyn bw_store::Store,
    project: ProjectId,
    scope: Scope,
) -> bw_store::Result<OpsVM> {
    let details = store.stage_details(project).await?;
    let sigs = store.persisted_signals(project).await?;
    let trends = store.metric_trends(project).await?;

    // Index the derive caches + trends for an O(1) join by stage / name.
    let stage_sig: std::collections::HashMap<StageKind, bw_core::Signal> = sigs
        .stages
        .iter()
        .map(|s| (s.kind, s.routine.unwrap_or(bw_core::Signal::Unknown)))
        .collect();
    let trend_by_name: std::collections::HashMap<&str, &Vec<f32>> =
        trends.iter().map(|t| (t.name.as_str(), &t.trend)).collect();

    let stages: Vec<OpsStageVM> = details
        .iter()
        .map(|d| {
            // Metrics belonging to this control point, in persisted order.
            let metrics: Vec<OpsMetricVM> = sigs
                .metrics
                .iter()
                .filter(|m| m.stage_kind == Some(d.kind))
                .map(|m| OpsMetricVM {
                    name: m.name.clone(),
                    value_raw: m.value_raw.clone(),
                    target_raw: m.target_raw.clone(),
                    // Read-only: the cache recompute wrote. Unknown ≠ green.
                    signal: m.signal.unwrap_or(bw_core::Signal::Unknown),
                    hit: m.hit,
                    trend: trend_by_name
                        .get(m.name.as_str())
                        .map(|t| (*t).clone())
                        .unwrap_or_default(),
                    // All wizard metrics are Manual in P2 (the store read doesn't
                    // carry source yet) → badge accordingly. See report note.
                    manual: true,
                })
                .collect();
            // Representative series for the big trend = the first observed metric's
            // real trend (≥1 point). None ⇒ empty (UI shows current progress only).
            let trend = metrics
                .iter()
                .map(|m| &m.trend)
                .find(|t| !t.is_empty())
                .cloned()
                .unwrap_or_default();
            OpsStageVM {
                kind: d.kind,
                index: d.kind.index(),
                label: d.kind.label().to_string(),
                phase: d.phase,
                signal: stage_sig
                    .get(&d.kind)
                    .copied()
                    .unwrap_or(bw_core::Signal::Unknown),
                progress: d.progress,
                owns: d.owns.clone(),
                accept: d.accept.clone(),
                control: d.control.clone(),
                metrics,
                trend,
            }
        })
        .collect();

    // The stage in scope (Scope::Stage(n) is 1-based; match on the control-point
    // index so the axis selection and the body agree).
    let active = match scope {
        Scope::Stage(n) => stages.iter().find(|s| s.index == n).cloned(),
        Scope::All => None,
    };

    Ok(OpsVM { stages, active })
}

/// Resolve the persistent DB path under the OS app-data dir and ensure its
/// parent exists, so quit→reopen restores state (P2 durability requirement).
fn db_path() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("builders-workbench");
    // Best-effort: if this fails, SqliteStore::open surfaces the real error.
    let _ = std::fs::create_dir_all(&base);
    base.join("bw.sqlite")
}

/// Install the App host: a [`Signal<ViewModel>`] + a [`CommandBus`], both shared
/// via context. Call once from the root component.
///
/// Returns the VM signal (also provided as context) so the root can render off
/// it directly.
pub fn use_app_host() -> Signal<ViewModel> {
    // Provided into context up-front so children mount with a valid (empty) VM
    // before the async open completes.
    let vm = use_context_provider(|| Signal::new(ViewModel::default()));

    // The coroutine OWNS the App. `tx` (the returned `Coroutine<Command>`) is the
    // command bus; we hand it to children via context.
    let bus = use_coroutine(move |mut rx: UnboundedReceiver<Command>| {
        // `vm` is Copy (Dioxus Signal); move a copy into the task.
        let mut vm = vm;
        async move {
            let path = db_path();
            let store = match SqliteStore::open(&path.to_string_lossy()).await {
                Ok(s) => Arc::new(s),
                Err(e) => {
                    // Without a store there is no app; surface and stop. (A real
                    // error screen is a later polish item.)
                    eprintln!("FATAL: open store at {}: {e}", path.display());
                    return;
                }
            };
            let mut app = App::new(store, Engine::new(MockExecutor::new()));

            // Initial snapshot → VM. `rebuild_vm` reads the store directly, so this
            // surfaces any projects persisted from a previous run.
            rebuild_vm(&app, &mut vm).await;

            // The command loop: dispatch in (the only `&mut app` site), recompute
            // the VM out. See module docs for the streaming alternative.
            while let Some(cmd) = rx.next().await {
                if let Err(e) = app.dispatch(cmd).await {
                    eprintln!("dispatch error: {e}");
                }
                rebuild_vm(&app, &mut vm).await;
            }
        }
    });

    use_context_provider(|| bus);
    vm
}
