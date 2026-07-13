//! Pure analysis layer over the telemetry foundation (Arc 2, iters 7–12).
//!
//! Every function here is a **pure transformation** of already-fetched run
//! data — no IO, no Store, no async. That keeps it in the wasm-clean kernel
//! (testable with synthetic data, no DB setup) and lets both `bw-app` (to
//! drive proposals) and `ui` (to render) call the identical logic.
//!
//! The Store fetches the grain (runs / analytics / versions); this module is
//! where that grain becomes *judgment* — failure modes, param habits, health
//! signals, optimization proposals. The split mirrors the existing derive
//! chain: raw values in, derived signal out.

use crate::model::{RunStatus, RunTrigger, WorkflowRun};
use std::collections::{HashMap, HashSet};

/// One cluster of failed runs sharing a common cause (iter 7). The "cause" is
/// a normalized prefix of the raw error string — failed runs whose errors
/// share a root (e.g. `模拟 · 第二步失败`) collapse into one mode, not a flat
/// list. The count + recency tell you which failure to fix first.
#[derive(Clone, Debug, PartialEq)]
pub struct FailureMode {
    /// Normalized cause: the error string trimmed of volatile suffixes, lower
    ///cased for grouping. Two runs with the same root cause share one `cause`.
    pub cause: String,
    pub count: u32,
    /// How many distinct workflows hit this mode — `1` means one workflow's
    /// problem; `>1` means a systemic issue across the hub.
    pub affected_workflows: u32,
    /// Unix seconds of the most recent occurrence, if any.
    pub last_seen: Option<i64>,
}

/// Cluster failed runs by normalized error cause (iter 7). Pure: pass the
/// run log, get the failure taxonomy back, most-frequent first. Non-failed
/// runs are ignored. An empty input (or one with no failures) returns `[]`.
pub fn failure_modes(runs: &[WorkflowRun]) -> Vec<FailureMode> {
    // Map cause → (count, set of workflow_ids, last_seen).
    let mut bucket: HashMap<String, (u32, HashSet<crate::WorkflowId>, Option<i64>)> =
        HashMap::new();
    for r in runs.iter().filter(|r| r.status == RunStatus::Failed) {
        let cause = normalize_cause(&r.error);
        let entry = bucket.entry(cause).or_insert((0, HashSet::new(), None));
        entry.0 += 1;
        entry.1.insert(r.workflow_id);
        entry.2 = Some(entry.2.map_or(r.started_at, |prev| prev.max(r.started_at)));
    }
    let mut out: Vec<FailureMode> = bucket
        .into_iter()
        .map(|(cause, (count, wfids, last_seen))| FailureMode {
            cause,
            count,
            affected_workflows: wfids.len() as u32,
            last_seen,
        })
        .collect();
    // Most frequent first; ties broken by most-recent.
    out.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.last_seen.cmp(&a.last_seen))
    });
    out
}

/// Reduce a raw error string to its stable root cause. Trims, takes the part
/// before any `:` / `—` / `(` / stack-trace newline, and lowercases — so
/// `模拟 · 第二步失败` and `模拟 · 第二步失败 (retry 3)` group together.
fn normalize_cause(raw: &str) -> String {
    let trimmed = raw.trim();
    let head = trimmed
        .split([':', '\n', '—', '('])
        .next()
        .unwrap_or(trimmed)
        .trim();
    head.to_lowercase()
}

/// The distribution of run "shapes" across a set of runs (iter 8) — what
/// phase-count, loop-config, and trigger mix users actually invoke. Reveals
/// the *habitual* shape, which is the seed of habit-based defaults (iter 19).
#[derive(Clone, Debug, PartialEq)]
pub struct RunShapeProfile {
    pub sample: u32,
    /// Most common phase_count, with its share of runs. `None` when no run
    /// carried a parseable snapshot.
    pub dominant_phase_count: Option<(u8, f32)>,
    /// Most common (retries, max_iter) loop config + its share.
    pub dominant_loop: Option<((u8, u8), f32)>,
    /// `(manual_count, scheduled_count)` — the trigger mix.
    pub trigger_split: (u32, u32),
}

/// Aggregate the run-shape distribution from each run's `params_json`
/// snapshot (iter 3). Pure + tolerant: a malformed/empty snapshot is skipped,
/// not a panic. Returns an empty profile (`sample == 0`) if nothing parsed.
pub fn run_shape_profile(runs: &[WorkflowRun]) -> RunShapeProfile {
    let mut phase_counts: HashMap<u8, u32> = HashMap::new();
    let mut loops: HashMap<(u8, u8), u32> = HashMap::new();
    let mut manual = 0u32;
    let mut scheduled = 0u32;
    let mut sample = 0u32;

    for r in runs {
        match r.trigger {
            RunTrigger::Manual => manual += 1,
            RunTrigger::Scheduled => scheduled += 1,
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&r.params_json) else {
            continue;
        };
        sample += 1;
        if let Some(n) = v.get("phase_count").and_then(|x| x.as_u64()) {
            *phase_counts.entry(n as u8).or_insert(0) += 1;
        }
        if let (Some(rt), Some(mi)) = (
            v.get("loop")
                .and_then(|l| l.get("retries"))
                .and_then(|x| x.as_u64()),
            v.get("loop")
                .and_then(|l| l.get("max_iter"))
                .and_then(|x| x.as_u64()),
        ) {
            *loops.entry((rt as u8, mi as u8)).or_insert(0) += 1;
        }
    }
    let total = sample.max(1) as f32;
    RunShapeProfile {
        sample,
        dominant_phase_count: mode(&phase_counts).map(|(k, c)| (*k, c as f32 / total)),
        dominant_loop: mode(&loops).map(|(k, c)| (*k, c as f32 / total)),
        trigger_split: (manual, scheduled),
    }
}

/// Pick the most-frequent key from a histogram. Ties broken by smallest key
/// for determinism (so the output is stable across equal-data reruns).
fn mode<K: Ord + Copy>(hist: &HashMap<K, u32>) -> Option<(&K, u32)> {
    hist.iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(k, c)| (k, *c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RunTrigger;
    use crate::{WorkflowId, WorkflowRunId};

    fn run(err: &str, status: RunStatus, wf: WorkflowId, started: i64) -> WorkflowRun {
        WorkflowRun {
            id: WorkflowRunId::nil(),
            workflow_id: wf,
            workflow_name: "w".into(),
            project_id: None,
            session_id: None,
            trigger: RunTrigger::Manual,
            status,
            started_at: started,
            finished_at: Some(started + 1),
            duration_ms: Some(10),
            phases_completed: 1,
            error: err.into(),
            params_json: String::new(),
            cron_task_id: None,
        }
    }

    fn run_with_params(params: &str, trigger: RunTrigger, status: RunStatus) -> WorkflowRun {
        WorkflowRun {
            id: WorkflowRunId::nil(),
            workflow_id: WorkflowId::nil(),
            workflow_name: "w".into(),
            project_id: None,
            session_id: None,
            trigger,
            status,
            started_at: 0,
            finished_at: Some(1),
            duration_ms: Some(10),
            phases_completed: 1,
            error: String::new(),
            params_json: params.into(),
            cron_task_id: None,
        }
    }

    #[test]
    fn run_shape_finds_dominant_phase_count_and_loop() {
        let runs = vec![
            run_with_params(
                r#"{"phase_count":3,"loop":{"retries":1,"max_iter":3}}"#,
                RunTrigger::Manual,
                RunStatus::Ok,
            ),
            run_with_params(
                r#"{"phase_count":3,"loop":{"retries":1,"max_iter":3}}"#,
                RunTrigger::Manual,
                RunStatus::Ok,
            ),
            run_with_params(
                r#"{"phase_count":5,"loop":{"retries":2,"max_iter":5}}"#,
                RunTrigger::Scheduled,
                RunStatus::Ok,
            ),
            run_with_params("not json", RunTrigger::Manual, RunStatus::Ok), // skipped gracefully
        ];
        let p = run_shape_profile(&runs);
        assert_eq!(p.sample, 3, "malformed snapshot skipped, not counted");
        let (pc, share) = p.dominant_phase_count.unwrap();
        assert_eq!(pc, 3);
        assert!(
            (share - (2.0 / 3.0)).abs() < 1e-5,
            "2 of 3 snapshots had 3 phases"
        );
        let ((rt, mi), _) = p.dominant_loop.unwrap();
        assert_eq!((rt, mi), (1, 3));
        assert_eq!(p.trigger_split, (3, 1), "3 manual + 1 scheduled");
    }

    #[test]
    fn run_shape_empty_when_no_snapshots() {
        let p = run_shape_profile(&[run_with_params("", RunTrigger::Manual, RunStatus::Ok)]);
        assert_eq!(p.sample, 0);
        assert!(p.dominant_phase_count.is_none());
    }

    #[test]
    fn clusters_failures_by_normalized_cause_desc() {
        let a = WorkflowId::nil();
        let b = WorkflowId::from_uuid(
            uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap(),
        );
        let runs = vec![
            run("模拟 · 第二步失败", RunStatus::Failed, a, 100),
            run("模拟 · 第二步失败 (retry 2)", RunStatus::Failed, a, 200),
            run("网络超时", RunStatus::Failed, b, 150),
            run("ok", RunStatus::Ok, a, 300), // ignored
        ];
        let modes = failure_modes(&runs);
        assert_eq!(modes.len(), 2, "two distinct causes");
        assert_eq!(modes[0].cause, "模拟 · 第二步失败");
        assert_eq!(modes[0].count, 2, "two runs collapsed");
        assert_eq!(modes[0].affected_workflows, 1, "both from workflow a");
        assert_eq!(modes[0].last_seen, Some(200));
        assert_eq!(modes[1].cause, "网络超时");
        assert_eq!(modes[1].count, 1);
        assert_eq!(modes[1].affected_workflows, 1);
    }

    #[test]
    fn no_failures_yields_empty_not_error() {
        let runs = vec![run("", RunStatus::Ok, WorkflowId::nil(), 1)];
        assert!(failure_modes(&runs).is_empty());
    }
}
