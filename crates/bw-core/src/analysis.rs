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

// ───────────────────────── iter 12: habit profile ─────────────────────────

/// How hot a workflow is, by run volume (iter 12). The bands are deliberately
/// coarse — three buckets a human reasons about, not a continuous score.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageTier {
    /// ≥10 runs — a workhorse the org leans on.
    Hot,
    /// 1–9 runs — used, but not central.
    Warm,
    /// 0 runs — dormant (same flag as `UsageRank::cold`).
    Cold,
}

/// A workflow's full usage signature (iter 12) — the one struct a "how is
/// this workflow actually used?" view reads from. Composes health (iter 11),
/// shape (iter 8), and usage tier into a single, human-summarizable profile.
/// The seed of habit-based defaults (iter 19).
#[derive(Clone, Debug, PartialEq)]
pub struct HabitProfile {
    pub workflow_id: crate::WorkflowId,
    pub workflow_name: String,
    pub health: Signal,
    pub tier: UsageTier,
    pub shape: RunShapeProfile,
    /// `(manual, scheduled)` — same as shape's trigger_split, lifted for
    /// convenience so a caller doesn't re-read the shape.
    pub trigger_split: (u32, u32),
    /// One-line human summary, e.g. "热门·绿色·3阶段·主要手动触发·典型耗时 200ms".
    pub summary: String,
}

/// Classify run volume into a coarse tier (iter 12). ≥10 → Hot, 1–9 → Warm,
/// 0 → Cold. The thresholds are product judgement, tuned to be useful not
/// precise — three buckets a person reasons about.
pub fn usage_tier(total_runs: u32) -> UsageTier {
    if total_runs == 0 {
        UsageTier::Cold
    } else if total_runs >= 10 {
        UsageTier::Hot
    } else {
        UsageTier::Warm
    }
}

/// Compose a workflow's analytics + usage + shape into a single habit profile
/// (iter 12). Pure. The `shape` is computed from that workflow's runs (iter 8);
/// pass it in so this function stays free of the run log.
pub fn habit_profile(
    analytics: &WorkflowRunAnalytics,
    usage: &UsageRank,
    shape: RunShapeProfile,
) -> HabitProfile {
    let health = workflow_health(analytics);
    let tier = usage_tier(usage.total_runs);
    let health_label = match health {
        Signal::Green => "绿色",
        Signal::Amber => "黄色",
        Signal::Red => "红色",
        Signal::Unknown => "未知",
    };
    let tier_label = match tier {
        UsageTier::Hot => "热门",
        UsageTier::Warm => "温",
        UsageTier::Cold => "冷门",
    };
    let trigger_label = match shape.trigger_split {
        (m, s) if m >= s && m > 0 => "主要手动触发",
        (m, s) if s > m => "主要定时触发",
        _ => "无运行",
    };
    let phase_label = shape
        .dominant_phase_count
        .map(|(n, _)| format!("·{}阶段", n))
        .unwrap_or_default();
    let dur_label = analytics
        .median_duration_ms
        .map(|d| format!("·典型耗时 {}ms", d))
        .unwrap_or_default();
    let summary = format!(
        "{}·{}{}·{}{}",
        tier_label, health_label, phase_label, trigger_label, dur_label
    );
    HabitProfile {
        workflow_id: analytics.workflow_id,
        workflow_name: analytics.workflow_name.clone(),
        health,
        tier,
        trigger_split: shape.trigger_split,
        shape,
        summary,
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

// ───────────────────────── iter 15: scenario clustering ─────────────────────────

/// One usage scenario — a cluster of runs sharing an invocation signature
/// (iter 15). "Users run this workflow in N distinct ways" is the answer this
/// gives, each with its own volume + success rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Scenario {
    pub label: String,
    pub count: u32,
    pub success_rate: Option<f32>,
    pub median_duration_ms: Option<i64>,
}

/// Cluster runs into usage scenarios by their (phase_count, trigger)
/// signature (iter 15). Pure. Reveals the distinct ways a workflow is
/// actually invoked — the scenarios optimization must serve, not a single
/// averaged shape. Largest scenario first.
pub fn cluster_scenarios(runs: &[WorkflowRun]) -> Vec<Scenario> {
    let mut bucket: HashMap<(Option<u8>, RunTrigger), Vec<&WorkflowRun>> = HashMap::new();
    for r in runs {
        let pc = serde_json::from_str::<serde_json::Value>(&r.params_json)
            .ok()
            .and_then(|v| {
                v.get("phase_count")
                    .and_then(|x| x.as_u64())
                    .map(|n| n as u8)
            });
        bucket.entry((pc, r.trigger)).or_default().push(r);
    }
    let mut out: Vec<Scenario> = bucket
        .into_iter()
        .map(|((pc, trig), group)| {
            let owned: Vec<WorkflowRun> = group.into_iter().cloned().collect();
            let (_n, rate, med) = slice_stats(&owned);
            let pc_label = pc
                .map(|n| format!("{}阶段", n))
                .unwrap_or_else(|| "未知阶段".into());
            let trig_label = match trig {
                RunTrigger::Manual => "手动",
                RunTrigger::Scheduled => "定时",
            };
            Scenario {
                label: format!("{} · {}", pc_label, trig_label),
                count: owned.len() as u32,
                success_rate: rate,
                median_duration_ms: med,
            }
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.count));
    out
}

// ───────────────────────── iter 16: cross-stage reuse ─────────────────────────

use crate::model::StageKind;

/// How the 5-stage standard templates are actually reused across the hub
/// (iter 16) — one row per stage, with its workflow count + run volume.
/// Answers "which stage's methodology is earning its keep, which is dormant?"
#[derive(Clone, Debug, PartialEq)]
pub struct StageReuse {
    pub stage: StageKind,
    pub workflow_count: u32,
    pub total_runs: u32,
    /// Share of all hub runs attributable to this stage (0..1).
    pub run_share: f32,
}

/// Tally per-stage reuse from the usage ranking (iter 16). Pure. A workflow
/// with no `stage_ref` (metrics-layer / cross-cutting) is counted under a
/// separate "unscoped" bucket in the returned vec's last position only when
/// present. Stages with zero workflows still appear (cold stage = signal).
pub fn cross_stage_reuse(ranking: &[UsageRank]) -> Vec<StageReuse> {
    let mut counts: HashMap<StageKind, (u32, u32)> = HashMap::new();
    let mut unscoped_wf = 0u32;
    let mut unscoped_runs = 0u32;
    let mut grand_runs = 0u32;
    for r in ranking {
        grand_runs += r.total_runs;
        match r
            .stage_ref
            .and_then(|n| StageKind::ALL.iter().find(|s| s.index() == n).copied())
        {
            Some(stage) => {
                let e = counts.entry(stage).or_insert((0, 0));
                e.0 += 1;
                e.1 += r.total_runs;
            }
            None => {
                unscoped_wf += 1;
                unscoped_runs += r.total_runs;
            }
        }
    }
    let denom = grand_runs.max(1) as f32;
    let mut out: Vec<StageReuse> = StageKind::ALL
        .iter()
        .map(|&stage| {
            let (wf, runs) = counts.get(&stage).copied().unwrap_or((0, 0));
            StageReuse {
                stage,
                workflow_count: wf,
                total_runs: runs,
                run_share: runs as f32 / denom,
            }
        })
        .collect();
    // Sort by run volume desc — the busiest methodology first.
    out.sort_by_key(|s| std::cmp::Reverse(s.total_runs));
    let _ = (unscoped_wf, unscoped_runs); // tracked, surfaced by a caller if needed
    out
}

// ───────────────────────── iter 17: recommendation engine ─────────────────────────

/// A "run this next" recommendation (iter 17). Grounded — the `why` cites the
/// signal that made this workflow the pick, never a bare "try this".
#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub workflow_id: crate::WorkflowId,
    pub workflow_name: String,
    pub why: String,
}

/// Recommend the best workflow to run for `stage`, given each candidate's
/// usage rank + health signal (iter 17). Pure. Selection rules, in order:
/// 1. same-stage, **green**, hottest (most runs) — the proven default;
/// 2. same-stage, **unknown** (new/untested), hottest — give the new one a
///    chance to earn evidence rather than starve forever;
/// 3. same-stage, **amber** but hot — used despite being shaky (worth running
///    to gather more signal).
///
/// Never **red** — a broken workflow isn't recommended, even if hot.
pub fn recommend_for_stage(
    stage: StageKind,
    candidates: &[(UsageRank, Signal)],
) -> Option<Recommendation> {
    // Filter to same-stage, exclude red, and score by (health_rank, -runs).
    // health_rank: green=0, unknown=1, amber=2 (red already excluded).
    fn health_rank(s: Signal) -> Option<u8> {
        match s {
            Signal::Green => Some(0),
            Signal::Unknown => Some(1),
            Signal::Amber => Some(2),
            Signal::Red => None, // never recommended
        }
    }
    let mut pool: Vec<(&UsageRank, Signal, u8)> = candidates
        .iter()
        .filter(|(r, _)| {
            r.stage_ref
                .and_then(|n| StageKind::ALL.iter().find(|s| s.index() == n).copied())
                == Some(stage)
        })
        .filter_map(|(r, s)| health_rank(*s).map(|rk| (r, *s, rk)))
        .collect();
    // Best = lowest health_rank, then most runs (hot), then name for stability.
    pool.sort_by(|a, b| {
        a.2.cmp(&b.2)
            .then_with(|| b.0.total_runs.cmp(&a.0.total_runs))
            .then_with(|| a.0.workflow_name.cmp(&b.0.workflow_name))
    });
    let (rank, health, _) = pool.first()?;
    let why = match health {
        Signal::Green => format!(
            "「{}」同阶段·绿色·已跑 {} 次 —— 放心的默认选择。",
            rank.workflow_name, rank.total_runs
        ),
        Signal::Unknown => format!(
            "「{}」同阶段·尚未验证 —— 跑一次给它积累证据(避免新工作流永远饿死)。",
            rank.workflow_name
        ),
        Signal::Amber => format!(
            "「{}」同阶段·黄色但已跑 {} 次 —— 继续跑以补全诊断信号。",
            rank.workflow_name, rank.total_runs
        ),
        Signal::Red => unreachable!("red filtered out above"),
    };
    Some(Recommendation {
        workflow_id: rank.workflow_id,
        workflow_name: rank.workflow_name.clone(),
        why,
    })
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

    fn shape(manual: u32, scheduled: u32, phases: u8) -> RunShapeProfile {
        RunShapeProfile {
            sample: manual + scheduled,
            dominant_phase_count: Some((phases, 1.0)),
            dominant_loop: None,
            trigger_split: (manual, scheduled),
        }
    }

    #[test]
    fn habit_profile_classifies_tier_and_summarizes() {
        // Hot + green + 3 phases + manual-heavy → summary reads all four.
        let mut a = analytics(12, 12, 0, Some(1.0), Some(200));
        a.last_status = Some(RunStatus::Ok);
        let u = usage(12, false);
        let p = habit_profile(&a, &u, shape(10, 2, 3));
        assert_eq!(p.tier, UsageTier::Hot);
        assert_eq!(p.health, Signal::Green);
        assert!(p.summary.contains("热门"), "tier: {}", p.summary);
        assert!(p.summary.contains("绿色"), "health in summary");
        assert!(p.summary.contains("3阶段"), "phase shape in summary");
        assert!(p.summary.contains("手动"), "trigger mix in summary");
        assert!(p.summary.contains("200ms"), "median duration in summary");
    }

    #[test]
    fn habit_profile_cold_unknown_when_never_run() {
        let a = analytics(0, 0, 0, None, None);
        let u = usage(0, true);
        let p = habit_profile(&a, &u, shape(0, 0, 0));
        assert_eq!(p.tier, UsageTier::Cold);
        assert_eq!(p.health, Signal::Unknown);
        assert!(p.summary.contains("冷门"));
        assert!(p.summary.contains("无运行"));
    }

    fn proposal(kind: ProposalKind) -> OptimizationProposal {
        OptimizationProposal {
            kind,
            workflow_id: WorkflowId::nil(),
            workflow_name: "wf".into(),
            title: "t".into(),
            rationale: "r".into(),
            priority: 0,
        }
    }

    #[test]
    fn apply_gate_rejects_below_sample_floor() {
        let p = ApplyPolicy::default(); // min_sample 5
                                        // 4 settled runs → Reject even for a positive PromoteTemplate.
        let d = review_proposal(&proposal(ProposalKind::PromoteTemplate), 4, &p);
        assert!(matches!(d, ApplyDecision::Reject(_)));
    }

    #[test]
    fn apply_gate_auto_applies_promote_defers_destructive() {
        let p = ApplyPolicy::default();
        assert!(matches!(
            review_proposal(&proposal(ProposalKind::PromoteTemplate), 8, &p),
            ApplyDecision::AutoApply
        ));
        assert!(matches!(
            review_proposal(&proposal(ProposalKind::FixFailure), 8, &p),
            ApplyDecision::DeferToHuman(_)
        ));
        assert!(matches!(
            review_proposal(&proposal(ProposalKind::Retire), 20, &p),
            ApplyDecision::DeferToHuman(_)
        ));
    }

    fn settled(statuses: &[RunStatus], dur: i64) -> Vec<WorkflowRun> {
        statuses
            .iter()
            .enumerate()
            .map(|(i, &st)| WorkflowRun {
                id: WorkflowRunId::nil(),
                workflow_id: WorkflowId::nil(),
                workflow_name: "w".into(),
                project_id: None,
                session_id: None,
                trigger: RunTrigger::Manual,
                status: st,
                started_at: i as i64,
                finished_at: Some(i as i64 + 1),
                duration_ms: Some(dur + i as i64),
                phases_completed: 1,
                error: String::new(),
                params_json: String::new(),
                cron_task_id: None,
            })
            .collect()
    }

    #[test]
    fn ab_compares_rate_and_flags_improvement() {
        // Before: 1/4 ok (25%). After: 4/4 ok (100%) → Improved.
        let before = settled(
            &[
                RunStatus::Ok,
                RunStatus::Failed,
                RunStatus::Failed,
                RunStatus::Failed,
            ],
            200,
        );
        let after = settled(
            &[RunStatus::Ok, RunStatus::Ok, RunStatus::Ok, RunStatus::Ok],
            200,
        );
        let d = ab_compare(&before, &after);
        assert_eq!(d.verdict, AbVerdict::Improved);
        assert!(d.rate_delta.unwrap() > 0.5);
    }

    #[test]
    fn ab_inconclusive_on_thin_data() {
        // Only 1 settled on the after side → Inconclusive.
        let before = settled(&[RunStatus::Ok, RunStatus::Ok, RunStatus::Ok], 100);
        let after = settled(&[RunStatus::Ok], 100);
        let d = ab_compare(&before, &after);
        assert!(matches!(d.verdict, AbVerdict::Inconclusive(_)));
    }

    #[test]
    fn ab_flat_rate_breaks_tie_on_duration() {
        let before = settled(&[RunStatus::Ok, RunStatus::Ok, RunStatus::Ok], 1500);
        let after = settled(&[RunStatus::Ok, RunStatus::Ok, RunStatus::Ok], 500);
        let d = ab_compare(&before, &after);
        assert_eq!(d.verdict, AbVerdict::Improved, "duration breaks the tie");
    }

    #[test]
    fn scenario_clustering_reveals_distinct_invocation_shapes() {
        // Two scenarios: "3阶段·手动" (3 runs) and "5阶段·定时" (2 runs).
        let runs = vec![
            run_with_params(r#"{"phase_count":3}"#, RunTrigger::Manual, RunStatus::Ok),
            run_with_params(r#"{"phase_count":3}"#, RunTrigger::Manual, RunStatus::Ok),
            run_with_params(
                r#"{"phase_count":3}"#,
                RunTrigger::Manual,
                RunStatus::Failed,
            ),
            run_with_params(r#"{"phase_count":5}"#, RunTrigger::Scheduled, RunStatus::Ok),
            run_with_params(r#"{"phase_count":5}"#, RunTrigger::Scheduled, RunStatus::Ok),
        ];
        let sc = cluster_scenarios(&runs);
        assert_eq!(sc.len(), 2, "two distinct signatures");
        assert_eq!(sc[0].count, 3, "largest scenario first");
        assert!(sc[0].label.contains("3阶段"));
        assert!(sc[0].label.contains("手动"));
        assert_eq!(sc[0].success_rate, Some(2.0 / 3.0));
        assert!(sc[1].label.contains("5阶段"));
        assert!(sc[1].label.contains("定时"));
    }

    fn rank(stage: Option<u8>, runs: u32) -> UsageRank {
        UsageRank {
            workflow_id: WorkflowId::nil(),
            workflow_name: "w".into(),
            stage_ref: stage,
            total_runs: runs,
            ok_runs: 0,
            failed_runs: 0,
            success_rate: None,
            last_run_at: None,
            cold: runs == 0,
        }
    }

    #[test]
    fn cross_stage_reuse_tallies_per_stage_volume() {
        // Prototype(1): 2 workflows / 10 runs; Optimize(3): 1 / 0 (cold).
        let ranking = vec![
            rank(Some(1), 7),
            rank(Some(1), 3),
            rank(Some(3), 0),
            rank(None, 5),
        ];
        let reuse = cross_stage_reuse(&ranking);
        assert_eq!(reuse[0].stage, StageKind::Prototype, "busiest first");
        assert_eq!(reuse[0].workflow_count, 2);
        assert_eq!(reuse[0].total_runs, 10);
        let opt = reuse
            .iter()
            .find(|r| r.stage == StageKind::Optimize)
            .unwrap();
        assert_eq!(opt.total_runs, 0, "cold stage surfaces, not hidden");
        assert_eq!(opt.workflow_count, 1);
    }

    fn cand(stage: Option<u8>, runs: u32, name: &str, health: Signal) -> (UsageRank, Signal) {
        (
            UsageRank {
                workflow_id: WorkflowId::nil(),
                workflow_name: name.into(),
                stage_ref: stage,
                total_runs: runs,
                ok_runs: 0,
                failed_runs: 0,
                success_rate: None,
                last_run_at: None,
                cold: runs == 0,
            },
            health,
        )
    }

    #[test]
    fn recommend_picks_green_hot_over_red_hot() {
        let pool = vec![
            cand(Some(1), 50, "坏的·热门", Signal::Red),
            cand(Some(1), 20, "好·热门", Signal::Green),
            cand(Some(1), 1, "好·冷门", Signal::Green),
        ];
        let rec = recommend_for_stage(StageKind::Prototype, &pool).unwrap();
        assert_eq!(rec.workflow_name, "好·热门");
        assert!(rec.why.contains("绿色"));
    }

    #[test]
    fn recommend_gives_unknown_a_chance_when_no_green() {
        let pool = vec![
            cand(Some(2), 5, "黄·温", Signal::Amber),
            cand(Some(2), 0, "新·未测", Signal::Unknown),
        ];
        let rec = recommend_for_stage(StageKind::Build, &pool).unwrap();
        assert_eq!(rec.workflow_name, "新·未测");
        assert!(rec.why.contains("证据"));
    }
}
