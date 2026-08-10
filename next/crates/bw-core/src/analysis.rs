//! Pure analysis layer over the telemetry foundation (Arc 2, iters 7–12).
//!
//! Every function here is a **pure transformation** of already-fetched run
//! data — no IO, no Store, no async. That keeps it in the wasm-clean kernel
//! (testable with synthetic data, no DB setup) and lets both `bw-app` (to
//! drive proposals) and `ui` (to render) call the identical logic.
//!
//! The Store fetches the grain (runs / analytics / versions); this module is
//! where that grain becomes *judgment* — failure modes, health signals,
//! optimization proposals. The split mirrors the existing derive
//! chain: raw values in, derived signal out.

use crate::model::{RunStatus, Signal, WorkflowRun};
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
                title: format!(
                    "「{}」成功率 {:.0}% · 先修「{}」",
                    name,
                    rate * 100.0,
                    cause
                ),
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
            title: format!("「{}」从未运行 · 复核是否保留", name),
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
                    title: format!(
                        "「{}」定时成功率 {:.0}% · 调节奏或修目标",
                        name,
                        rate * 100.0
                    ),
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
                title: format!("「{}」典型耗时 {}ms · 考虑精简", name, med),
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
                title: format!("「{}」高频且可靠({:.0}%) · 可作模板", name, rate * 100.0),
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

// ───────────────────────── iter 13: proposal apply gate ─────────────────────────

/// What to do with a proposal in the self-driving loop (iter 13). The gate
/// that turns *analysis* into *action* — but conservatively, because acting
/// on a workflow is outward-facing (it changes what runs next).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplyDecision {
    /// Safe to apply without a human — positive or strongly-evidenced.
    AutoApply,
    /// Needs a human's judgement before acting — surfaced, not executed.
    DeferToHuman(String),
    /// Should not be applied (insufficient evidence / below policy floor).
    Reject(String),
}

/// Policy for the self-driving apply gate (iter 13). Conservative by default:
/// the loop only auto-applies the *positive* (promote) and the *strongly-
/// evidenced* cadence step-up; everything that changes content or removes a
/// workflow defers to a human. Tunable — the loop's autonomy dial.
#[derive(Clone, Debug, PartialEq)]
pub struct ApplyPolicy {
    /// Minimum settled runs before ANY auto-apply trusts the evidence.
    pub min_sample: u32,
    /// How strong the cadence demand signal must be to auto-step-up (manual
    /// re-runs since last fire). Higher = more conservative.
    pub cadence_demand_floor: u32,
}

impl Default for ApplyPolicy {
    fn default() -> Self {
        // min_sample 5: five settled runs before the loop trusts a rate. Below
        // this, even a "100%" is one lucky run away from noise.
        ApplyPolicy {
            min_sample: 5,
            cadence_demand_floor: 3,
        }
    }
}

/// Decide whether a proposal is safe to auto-apply under `policy`, given the
/// workflow's settled-run count (iter 13). Pure. The whole point of the gate:
/// the self-driving loop (iter 18) never silently changes a workflow on thin
/// evidence or destructive intent.
pub fn review_proposal(
    proposal: &OptimizationProposal,
    settled_runs: u32,
    policy: &ApplyPolicy,
) -> ApplyDecision {
    // Retire is *about* absence-of-runs — a cold workflow has 0 settled runs
    // by definition, so the sample floor must not block it. It always defers
    // to a human (retiring is a judgement call, never auto-applied).
    if proposal.kind == ProposalKind::Retire {
        return ApplyDecision::DeferToHuman(format!(
            "「{}」需人工判断后执行(退役是判断题)",
            proposal.title
        ));
    }
    // Floor: no auto-apply below the sample minimum for rate-based proposals.
    // Even a "promote" defers until there's enough track record to trust.
    if settled_runs < policy.min_sample {
        return ApplyDecision::Reject(format!(
            "样本不足({}<{} 条 settled),不自动应用",
            settled_runs, policy.min_sample
        ));
    }
    match proposal.kind {
        // Positive mirror — safe to surface as a default. Still AutoApply, not
        // a forced change: promoting a template adds an option, removes none.
        ProposalKind::PromoteTemplate => ApplyDecision::AutoApply,
        // Cadence step-up is reversible + low-risk, but only when the demand
        // signal clears the floor. Below it, defer (a human should confirm).
        ProposalKind::TuneCadence => {
            ApplyDecision::DeferToHuman("节奏调整建议人工确认(可逆,但影响下一次触发时机)".into())
        }
        // Content-changing → always human. The loop never silently rewrites a
        // prompt or drops phases.
        ProposalKind::FixFailure | ProposalKind::Simplify => {
            ApplyDecision::DeferToHuman(format!("「{}」需人工判断后执行", proposal.title))
        }
        // Retire handled above (before the sample floor).
        ProposalKind::Retire => unreachable!(),
    }
}

// ───────────────────────── iter 14: A/B version comparison ─────────────────────────

/// Did an optimization actually help? (iter 14) The verdict on a version
/// change, comparing the settled runs *before* vs *after*.
#[derive(Clone, Debug, PartialEq)]
pub enum AbVerdict {
    /// After is meaningfully better on the metric that mattered.
    Improved,
    /// After is meaningfully worse — roll back / reconsider.
    Regressed,
    /// Not enough settled runs on one or both sides to tell.
    Inconclusive(String),
}

/// The before/after delta from one version change (iter 14). A positive
/// `rate_delta` and negative `duration_delta` = the optimization worked.
#[derive(Clone, Debug, PartialEq)]
pub struct VersionDelta {
    pub before_settled: u32,
    pub after_settled: u32,
    pub before_rate: Option<f32>,
    pub after_rate: Option<f32>,
    /// `after - before` success rate. `None` when either side has no data.
    pub rate_delta: Option<f32>,
    pub before_median_ms: Option<i64>,
    pub after_median_ms: Option<i64>,
    pub duration_delta_ms: Option<i64>,
    pub verdict: AbVerdict,
}

/// Compare two run slices (before vs after a version change) into a delta +
/// verdict (iter 14). Pure. A side needs ≥3 settled runs to count; below that
/// the verdict is `Inconclusive` — never a confident "improved" on thin data.
pub fn ab_compare(before: &[WorkflowRun], after: &[WorkflowRun]) -> VersionDelta {
    let (b_settled, b_rate, b_med) = slice_stats(before);
    let (a_settled, a_rate, a_med) = slice_stats(after);
    let rate_delta = match (b_rate, a_rate) {
        (Some(b), Some(a)) => Some(a - b),
        _ => None,
    };
    let duration_delta_ms = match (b_med, a_med) {
        (Some(b), Some(a)) => Some(a - b),
        _ => None,
    };
    // Verdict needs ≥3 settled on BOTH sides, else we can't trust a delta.
    let verdict = if b_settled < 3 || a_settled < 3 {
        AbVerdict::Inconclusive(format!(
            "样本不足(前 {}/后 {} 条 settled,各需 ≥3)",
            b_settled, a_settled
        ))
    } else if let Some(d) = rate_delta {
        // Rate is the primary signal; duration is secondary tiebreak.
        if d >= 0.1 {
            AbVerdict::Improved
        } else if d <= -0.1 {
            AbVerdict::Regressed
        } else {
            // Rate flat within ±10% — let duration break the tie (faster = improved).
            match duration_delta_ms {
                Some(dd) if dd <= -500 => AbVerdict::Improved,
                Some(dd) if dd >= 500 => AbVerdict::Regressed,
                _ => AbVerdict::Inconclusive("成功率与耗时均无显著变化".into()),
            }
        }
    } else {
        AbVerdict::Inconclusive("缺少成功率数据".into())
    };
    VersionDelta {
        before_settled: b_settled,
        after_settled: a_settled,
        before_rate: b_rate,
        after_rate: a_rate,
        rate_delta,
        before_median_ms: b_med,
        after_median_ms: a_med,
        duration_delta_ms,
        verdict,
    }
}

/// (settled_count, success_rate, median_duration) for a run slice. Factored
/// out so both sides of the comparison use the identical computation.
fn slice_stats(runs: &[WorkflowRun]) -> (u32, Option<f32>, Option<i64>) {
    let settled: Vec<&WorkflowRun> = runs
        .iter()
        .filter(|r| matches!(r.status, RunStatus::Ok | RunStatus::Failed))
        .collect();
    if settled.is_empty() {
        return (0, None, None);
    }
    let ok = settled.iter().filter(|r| r.status == RunStatus::Ok).count() as u32;
    let n = settled.len() as u32;
    let rate = Some(ok as f32 / n as f32);
    let mut durs: Vec<i64> = settled.iter().filter_map(|r| r.duration_ms).collect();
    durs.sort_unstable();
    let med = if durs.is_empty() {
        None
    } else {
        let mid = durs.len() / 2;
        Some(if durs.len() % 2 == 0 {
            (durs[mid - 1] + durs[mid]) / 2
        } else {
            durs[mid]
        })
    };
    (n, rate, med)
}

// ───────────────────────── iter 20: effectiveness summary ─────────────────────────

/// Hub-wide roll-up of how much optimization *actually* helped (iter 20) —
/// the answer to "are we getting better, across all workflows?" Built from a
/// list of per-workflow `VersionDelta`s (iter 14). This is the loop's
/// scoreboard: if `improved > regressed` over time, the self-driving
/// optimization is earning its keep.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectivenessSummary {
    pub compared: u32,
    pub improved: u32,
    pub regressed: u32,
    pub inconclusive: u32,
    /// Mean of all non-None `rate_delta` — the average success-rate lift (pp)
    /// optimization delivered across comparable workflows. Positive = better.
    pub avg_rate_delta: Option<f32>,
    /// Mean duration delta across comparable workflows (ms). Negative = faster.
    pub avg_duration_delta_ms: Option<i64>,
    /// One-line verdict, e.g. "改善 3 / 回归 1 · 平均成功率 +12pp".
    pub verdict: String,
}

/// Aggregate per-workflow A/B deltas into a hub-wide effectiveness summary
/// (iter 20). Pure. Skips `Inconclusive` deltas in the averages (they carry
/// no usable delta) but still count them, so the summary never hides "we
/// couldn't tell" behind a rosy average.
pub fn summarize_effectiveness(deltas: &[VersionDelta]) -> EffectivenessSummary {
    let compared = deltas.len() as u32;
    let improved = deltas
        .iter()
        .filter(|d| d.verdict == AbVerdict::Improved)
        .count() as u32;
    let regressed = deltas
        .iter()
        .filter(|d| d.verdict == AbVerdict::Regressed)
        .count() as u32;
    let inconclusive = deltas
        .iter()
        .filter(|d| matches!(d.verdict, AbVerdict::Inconclusive(_)))
        .count() as u32;
    let rate_deltas: Vec<f32> = deltas.iter().filter_map(|d| d.rate_delta).collect();
    let dur_deltas: Vec<i64> = deltas.iter().filter_map(|d| d.duration_delta_ms).collect();
    let avg_rate_delta = if rate_deltas.is_empty() {
        None
    } else {
        Some(rate_deltas.iter().sum::<f32>() / rate_deltas.len() as f32)
    };
    let avg_duration_delta_ms = if dur_deltas.is_empty() {
        None
    } else {
        Some(dur_deltas.iter().sum::<i64>() / dur_deltas.len() as i64)
    };
    let verdict = format!(
        "改善 {} / 回归 {} / 未定 {}{}",
        improved,
        regressed,
        inconclusive,
        avg_rate_delta
            .map(|d| format!(" · 平均成功率 {:+.0}pp", d * 100.0))
            .unwrap_or_default()
    );
    EffectivenessSummary {
        compared,
        improved,
        regressed,
        inconclusive,
        avg_rate_delta,
        avg_duration_delta_ms,
        verdict,
    }
}
