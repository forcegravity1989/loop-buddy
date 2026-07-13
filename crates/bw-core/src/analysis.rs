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

use crate::model::{RunStatus, RunTrigger, Signal, WorkflowRun};
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

// ───────────────────────── iter 9: optimization proposals ─────────────────────────

use crate::model::{CronEffectiveness, UsageRank, WorkflowRunAnalytics};

/// What kind of optimization a proposal recommends (iter 9). The variant is
/// the *action class*; the `rationale` carries the why.
#[derive(Clone, Debug, PartialEq)]
pub enum ProposalKind {
    /// A cold workflow (0 runs) — review whether it should stay in the hub.
    Retire,
    /// Success rate is below healthy — fix the dominant failure mode first.
    FixFailure,
    /// Runs are slow / heavy — simplify the phase structure.
    Simplify,
    /// A schedule fires but rarely succeeds — tune cadence or fix the target.
    TuneCadence,
    /// A hot, reliable workflow — promote its shape as a default/template.
    PromoteTemplate,
}

/// One actionable optimization suggestion (iter 9). Every proposal is
/// *grounded* — it cites the concrete evidence (numbers) that triggered it,
/// never a bare "you should optimize this". Priority is 0 (highest) → larger.
#[derive(Clone, Debug, PartialEq)]
pub struct OptimizationProposal {
    pub kind: ProposalKind,
    pub workflow_id: crate::WorkflowId,
    pub workflow_name: String,
    /// Human-readable one-liner ("成功率 60%,主要失败:网络超时(7次)").
    pub title: String,
    /// The why — the evidence chain a human reads before acting.
    pub rationale: String,
    /// 0 = most urgent. Derived from severity (failure > cold > slow > promote).
    pub priority: u8,
}

/// Compose analytics + usage + failures into ranked, evidence-grounded
/// optimization proposals (iter 9). Pure: pass the already-fetched data,
/// get suggestions back, most-urgent first. No thresholds are magic — each
/// is documented at the check that uses it.
pub fn propose_optimizations(
    analytics: &WorkflowRunAnalytics,
    usage: &UsageRank,
    failures: &[FailureMode],
    cron_eff: Option<&CronEffectiveness>,
) -> Vec<OptimizationProposal> {
    let mut out = Vec::new();
    let id = analytics.workflow_id;
    let name = analytics.workflow_name.clone();

    // 1. Failure-driven (most urgent): <80% success over ≥3 settled runs.
    // The fix-first principle — one root cause often explains most failures.
    if let Some(rate) = analytics.success_rate {
        if analytics.total_runs >= 3 && rate < 0.8 {
            let (cause, count) = failures
                .first()
                .map(|f| (f.cause.clone(), f.count))
                .unwrap_or(("未知".into(), analytics.failed_runs));
            out.push(OptimizationProposal {
                kind: ProposalKind::FixFailure,
                workflow_id: id,
                workflow_name: name.clone(),
                title: format!("成功率 {:.0}% · 先修「{}」", rate * 100.0, cause),
                rationale: format!(
                    "近 {} 次运行成功 {}/{}({:.0}%),头号失败「{}」占 {} 次 —— 修它收益最大。",
                    analytics.total_runs,
                    analytics.ok_runs,
                    analytics.total_runs,
                    rate * 100.0,
                    cause,
                    count
                ),
                priority: 0,
            });
        }
    }

    // 2. Cold workflow (review/retire). Never run = pure maintenance tax.
    if usage.cold {
        out.push(OptimizationProposal {
            kind: ProposalKind::Retire,
            workflow_id: id,
            workflow_name: name.clone(),
            title: "从未运行 · 复核是否保留".into(),
            rationale: format!(
                "「{}」进 hub 后一次未跑 —— 要么退役减负,要么明确它的触发场景。",
                name
            ),
            priority: 1,
        });
    }

    // 3. Schedule misfire: a cron task fires but <50% succeed.
    if let Some(eff) = cron_eff {
        if let Some(rate) = eff.effectiveness {
            if eff.fires >= 2 && rate < 0.5 {
                out.push(OptimizationProposal {
                    kind: ProposalKind::TuneCadence,
                    workflow_id: id,
                    workflow_name: name.clone(),
                    title: format!("定时成功率 {:.0}% · 调节奏或修目标", rate * 100.0),
                    rationale: format!(
                        "定时任务自动触发 {} 次,成功 {}({:.0}%) —— 继续烧 run 不如先修。",
                        eff.fires,
                        eff.ok_fires,
                        rate * 100.0
                    ),
                    priority: 1,
                });
            }
        }
    }

    // 4. Slow: median duration over 5s — simplify the phase structure.
    // (5s is a placeholder product threshold; the point is the check exists
    // and is tunable, not the specific number.)
    if let Some(med) = analytics.median_duration_ms {
        if med > 5_000 {
            out.push(OptimizationProposal {
                kind: ProposalKind::Simplify,
                workflow_id: id,
                workflow_name: name.clone(),
                title: format!("典型耗时 {}ms · 考虑精简", med),
                rationale: format!(
                    "中位耗时 {}ms(>5s) —— 看哪个阶段最重,能否拆/并行/缓存。",
                    med
                ),
                priority: 2,
            });
        }
    }

    // 5. Promote: hot (≥5 runs) AND reliable (≥95%) — its shape is worth
    // copying. The positive mirror of the failure check.
    if let Some(rate) = analytics.success_rate {
        if analytics.total_runs >= 5 && rate >= 0.95 {
            out.push(OptimizationProposal {
                kind: ProposalKind::PromoteTemplate,
                workflow_id: id,
                workflow_name: name.clone(),
                title: format!("高频且可靠({:.0}%) · 可作模板", rate * 100.0),
                rationale: format!(
                    "{} 次运行成功 {}/{},中位 {}ms —— 形状稳定,适合做同类任务的默认模板。",
                    analytics.total_runs,
                    analytics.ok_runs,
                    analytics.total_runs,
                    analytics
                        .median_duration_ms
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "—".into())
                ),
                priority: 3,
            });
        }
    }

    out.sort_by_key(|p| p.priority);
    out
}

// ───────────────────────── iter 10: cadence auto-tune ─────────────────────────

use crate::model::Cadence;

/// A suggestion to change (or keep) a cron task's cadence (iter 10). Grounded
/// in the schedule's real track record + how often the user manually re-ran
/// the same workflow between fires (the demand signal).
#[derive(Clone, Debug, PartialEq)]
pub struct CadenceSuggestion {
    pub current: Cadence,
    /// `None` = keep the current cadence (with a reason). `Some` = the
    /// suggested replacement, always *one step* from current (never a jump
    /// from Weekly straight to RealTime — tuning is incremental).
    pub suggested: Option<Cadence>,
    pub reason: String,
}

/// The most-frequent cadence at or below `c` (one step). `RealTime` and
/// `Cron(_)` have no "more frequent" form, so they return unchanged.
fn more_frequent(c: Cadence) -> Cadence {
    match c {
        Cadence::Weekly => Cadence::Daily,
        Cadence::Daily => Cadence::RealTime,
        other => other,
    }
}

/// Suggest a cadence change from the schedule's effectiveness + the count of
/// **manual re-runs** of the same workflow since the last scheduled fire
/// (iter 10). The demand signal: if the user keeps manually firing between
/// scheduled fires, the schedule is too sparse. Pure + conservative — it
/// never suggests tuning a failing task (fix first), and only moves one step.
pub fn suggest_cadence(
    current: Cadence,
    eff: &CronEffectiveness,
    manual_re_runs: u32,
) -> CadenceSuggestion {
    // Rule 1: a failing schedule gets no cadence change — fix the failure
    // first. Tuning the rhythm of something that breaks is noise.
    if let Some(rate) = eff.effectiveness {
        if eff.fires >= 2 && rate < 0.5 {
            return CadenceSuggestion {
                current,
                suggested: None,
                reason: format!(
                    "定时成功率仅 {:.0}%({}/{}) —— 先修失败,再调节奏。",
                    rate * 100.0,
                    eff.ok_fires,
                    eff.fires
                ),
            };
        }
    }

    // Rule 2: demand signal — user manually re-ran ≥2 times since the last
    // scheduled fire. The schedule is too sparse → step up one notch.
    if manual_re_runs >= 2 {
        let next = more_frequent(current.clone());
        if next != current {
            return CadenceSuggestion {
                current,
                suggested: Some(next),
                reason: format!(
                    "用户在两次定时之间手动重跑了 {} 次 —— 需求高于当前节奏,建议加密。",
                    manual_re_runs
                ),
            };
        }
        // Already at the most frequent fixed cadence.
        return CadenceSuggestion {
            current,
            suggested: None,
            reason: format!(
                "用户重跑 {} 次但已是最高频(RealTime/Cron) —— 看是否该拆任务。",
                manual_re_runs
            ),
        };
    }

    // Rule 3: healthy + no manual demand → keep.
    CadenceSuggestion {
        current,
        suggested: None,
        reason: "节奏合适:定时健康且无手动补跑信号。".into(),
    }
}

// ───────────────────────── iter 11: workflow health signal ─────────────────────────

/// Derive a workflow's health `Signal` from its run analytics (iter 11) —
/// reusing the *same* `Signal{Green,Amber,Red,Unknown}` the metric derive
/// chain already defines, so a workflow is "green" by exactly the same
/// semantics a metric is. Pure + threshold-documented.
///
/// * `Unknown` — no settled runs yet (mirrors "no data ≠ green"; never a
///   fabricated green for a workflow that's never really run).
/// * `Red`     — success rate < 50% over ≥2 settled runs (mostly broken).
/// * `Amber`   — success rate 50–80%, OR the most recent run failed (a
///   fresh regression deserves attention even if the long-run rate is ok).
/// * `Green`   — success rate ≥ 80% over ≥2 settled runs, last run ok.
pub fn workflow_health(a: &WorkflowRunAnalytics) -> Signal {
    // No evidence → Unknown, never a guessed green. This is the load-bearing
    // rule: it's what stops a brand-new workflow from masquerading as healthy.
    let Some(rate) = a.success_rate else {
        return Signal::Unknown;
    };
    let settled = a.ok_runs + a.failed_runs;
    if settled < 2 {
        // One run isn't a track record — call it Unknown, not Green/Red on a
        // sample of one. Same caution as a metric with a single observation.
        return Signal::Unknown;
    }
    // A fresh failure is an amber regression even when the rate looks fine —
    // "it broke just now" is actionable before the long-run average catches up.
    let last_failed = matches!(a.last_status, Some(RunStatus::Failed));
    if rate < 0.5 {
        Signal::Red
    } else if rate < 0.8 || last_failed {
        Signal::Amber
    } else {
        Signal::Green
    }
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

    fn analytics(
        total: u32,
        ok: u32,
        failed: u32,
        rate: Option<f32>,
        med: Option<i64>,
    ) -> WorkflowRunAnalytics {
        WorkflowRunAnalytics {
            workflow_id: WorkflowId::nil(),
            workflow_name: "wf".into(),
            total_runs: total,
            ok_runs: ok,
            failed_runs: failed,
            running_runs: 0,
            success_rate: rate,
            avg_duration_ms: med,
            median_duration_ms: med,
            last_run_at: None,
            last_status: None,
        }
    }

    fn usage(total: u32, cold: bool) -> UsageRank {
        UsageRank {
            workflow_id: WorkflowId::nil(),
            workflow_name: "wf".into(),
            stage_ref: None,
            total_runs: total,
            ok_runs: 0,
            failed_runs: 0,
            success_rate: None,
            last_run_at: None,
            cold,
        }
    }

    #[test]
    fn proposal_fix_failure_cites_the_top_failure_mode() {
        let a = analytics(10, 6, 4, Some(0.6), None);
        let u = usage(10, false);
        let f = vec![FailureMode {
            cause: "网络超时".into(),
            count: 3,
            affected_workflows: 1,
            last_seen: Some(100),
        }];
        let p = propose_optimizations(&a, &u, &f, None);
        assert_eq!(p[0].kind, ProposalKind::FixFailure);
        assert_eq!(p[0].priority, 0);
        assert!(p[0].title.contains("60%"), "rate in title: {}", p[0].title);
        assert!(p[0].rationale.contains("网络超时"), "evidence cited");
        assert!(p[0].rationale.contains("3 次"), "count cited");
    }

    #[test]
    fn proposal_retire_for_cold_and_promote_for_hot_reliable() {
        // Cold → Retire.
        let a = analytics(0, 0, 0, None, None);
        let u = usage(0, true);
        let p = propose_optimizations(&a, &u, &[], None);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].kind, ProposalKind::Retire);

        // Hot + reliable → Promote.
        let a2 = analytics(8, 8, 0, Some(1.0), Some(200));
        let u2 = usage(8, false);
        let p2 = propose_optimizations(&a2, &u2, &[], None);
        assert!(p2.iter().any(|x| x.kind == ProposalKind::PromoteTemplate));
    }

    #[test]
    fn no_proposal_when_healthy_and_warm() {
        // 3 runs, 100% success, not cold, fast, no failures → nothing to say.
        let a = analytics(3, 3, 0, Some(1.0), Some(50));
        let u = usage(3, false);
        assert!(propose_optimizations(&a, &u, &[], None).is_empty());
    }

    fn cron_eff(fires: u32, ok: u32) -> CronEffectiveness {
        CronEffectiveness {
            cron_task_id: crate::CronTaskId::nil(),
            fires,
            ok_fires: ok,
            failed_fires: fires - ok,
            effectiveness: if fires > 0 {
                Some(ok as f32 / fires as f32)
            } else {
                None
            },
            avg_duration_ms: None,
            last_fire_at: None,
            last_fire_ok: None,
        }
    }

    #[test]
    fn cadence_step_up_on_manual_demand_healthy() {
        // Healthy (4/4) but user manually re-ran 3× since last fire → too sparse.
        let eff = cron_eff(4, 4);
        let s = suggest_cadence(Cadence::Weekly, &eff, 3);
        assert_eq!(
            s.suggested,
            Some(Cadence::Daily),
            "Weekly → Daily, one step"
        );
        assert!(s.reason.contains("3 次"));
    }

    #[test]
    fn cadence_no_tune_when_failing() {
        // Failing (1/4) → no cadence change, fix first.
        let eff = cron_eff(4, 1);
        let s = suggest_cadence(Cadence::Weekly, &eff, 5);
        assert_eq!(s.suggested, None, "don't tune a failing schedule");
        assert!(s.reason.contains("先修失败"));
    }

    #[test]
    fn cadence_keep_when_healthy_no_demand() {
        let eff = cron_eff(3, 3);
        let s = suggest_cadence(Cadence::Daily, &eff, 0);
        assert_eq!(s.suggested, None);
        assert!(s.reason.contains("合适"));
    }

    #[test]
    fn workflow_health_unknown_until_two_settled_runs() {
        // No runs → Unknown (never a guessed green).
        assert_eq!(
            workflow_health(&analytics(0, 0, 0, None, None)),
            Signal::Unknown
        );
        // One run, even successful → still Unknown (sample of one isn't a track record).
        assert_eq!(
            workflow_health(&analytics(1, 1, 0, Some(1.0), None)),
            Signal::Unknown
        );
    }

    #[test]
    fn workflow_health_red_amber_green_by_success_rate() {
        // 40% over 5 settled → Red.
        assert_eq!(
            workflow_health(&analytics(5, 2, 3, Some(0.4), None)),
            Signal::Red
        );
        // 60% over 5 → Amber.
        assert_eq!(
            workflow_health(&analytics(5, 3, 2, Some(0.6), None)),
            Signal::Amber
        );
        // 90% over 10, last ok → Green.
        let mut green = analytics(10, 9, 1, Some(0.9), None);
        green.last_status = Some(RunStatus::Ok);
        assert_eq!(workflow_health(&green), Signal::Green);
    }

    #[test]
    fn workflow_health_amber_when_rate_ok_but_last_failed() {
        // 90% but the most recent run failed → Amber (fresh regression).
        let mut a = analytics(10, 9, 1, Some(0.9), None);
        a.last_status = Some(RunStatus::Failed);
        assert_eq!(
            workflow_health(&a),
            Signal::Amber,
            "recent failure is amber"
        );
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
