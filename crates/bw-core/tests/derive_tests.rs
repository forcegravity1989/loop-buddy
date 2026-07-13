//! P0 exit criteria (plan `04 §P0`): `evaluate_metric` is unit-tested against
//! *every* target syntax in the source; `reduce_worst_of` covers the worst-of +
//! Unknown lattice; Manual values still derive (never hand-set); `Missing →
//! Unknown` and `stale → Amber` degrade honestly. The compile-time "can't build a
//! Signal outside derive" guarantee is proved by the `compile_fail` doctests on
//! `Derived`.

use bw_core::derive::{
    evaluate_metric, measure, parse_target, parse_target_with, reduce_worst_of, AmberBand,
    Comparator, Measurement, Target,
};
use bw_core::model::{Cadence, SourceKind};
use bw_core::Signal;
use time::{Duration, OffsetDateTime};

// A fixed, fresh measurement helper (as_of == now, Daily cadence ⇒ never stale).
fn fresh(raw: &str) -> Measurement {
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    measure(raw, now, SourceKind::Manual, &Cadence::Daily, now)
}

fn sig(raw_value: &str, raw_target: &str) -> Signal {
    let t = parse_target(raw_target).unwrap();
    evaluate_metric(&fresh(raw_value), &t, &[]).signal()
}

// ───────────────── L2 parse_target: every source syntax ─────────────────

#[test]
fn parse_every_target_syntax() {
    // numeric comparators
    assert_eq!(
        parse_target("≥5").unwrap(),
        Target::Threshold {
            cmp: Comparator::Ge,
            value: 5.0,
            unit: String::new(),
            amber: AmberBand::default()
        }
    );
    assert!(matches!(
        parse_target("<800").unwrap(),
        Target::Threshold {
            cmp: Comparator::Lt,
            ..
        }
    ));
    assert!(matches!(
        parse_target(">3").unwrap(),
        Target::Threshold {
            cmp: Comparator::Gt,
            ..
        }
    ));
    assert!(matches!(
        parse_target("=10").unwrap(),
        Target::Threshold {
            cmp: Comparator::Eq,
            ..
        }
    ));
    assert!(matches!(
        parse_target(">=5").unwrap(),
        Target::Threshold {
            cmp: Comparator::Ge,
            ..
        }
    ));
    assert!(matches!(
        parse_target("<=5").unwrap(),
        Target::Threshold {
            cmp: Comparator::Le,
            ..
        }
    ));

    // duration normalizes to ms
    match parse_target("≤24h").unwrap() {
        Target::Threshold {
            cmp, value, unit, ..
        } => {
            assert_eq!(cmp, Comparator::Le);
            assert_eq!(value, 24.0 * 3_600_000.0);
            assert_eq!(unit, "ms");
        }
        other => panic!("expected threshold, got {other:?}"),
    }

    // bare forms ⇒ implicit ≥
    assert!(
        matches!(parse_target("100%").unwrap(), Target::Threshold { cmp: Comparator::Ge, unit, .. } if unit == "%")
    );
    match parse_target("7/7").unwrap() {
        Target::Threshold { cmp, value, .. } => {
            assert_eq!(cmp, Comparator::Ge);
            assert!((value - 1.0).abs() < 1e-9);
        }
        other => panic!("expected threshold, got {other:?}"),
    }

    // qualitative tokens
    assert_eq!(parse_target("清零").unwrap(), Target::DriveToZero);
    assert_eq!(parse_target("全覆盖").unwrap(), Target::FullCoverage);
    assert_eq!(parse_target("↑").unwrap(), Target::DirectionUp);
    assert_eq!(parse_target("跟踪").unwrap(), Target::TrackOnly);

    // errors are surfaced, not swallowed
    assert!(parse_target("").is_err());
    assert!(parse_target("not a target").is_err());
}

// ───────────────── L2 evaluate: threshold green/amber/red ─────────────────

#[test]
fn higher_is_better_bands() {
    // ≥5, default RelPct(0.10) ⇒ amber floor 4.5
    assert_eq!(sig("5", "≥5"), Signal::Green);
    assert_eq!(sig("8", "≥5"), Signal::Green);
    assert_eq!(sig("4.7", "≥5"), Signal::Amber);
    assert_eq!(sig("4.0", "≥5"), Signal::Red);
}

#[test]
fn lower_is_better_duration_bands() {
    // ≤24h, amber ceiling 26.4h
    assert_eq!(sig("18h", "≤24h"), Signal::Green);
    assert_eq!(sig("25h", "≤24h"), Signal::Amber);
    assert_eq!(sig("40h", "≤24h"), Signal::Red);
    // milliseconds vs hours compare in the same normalized unit
    assert_eq!(sig("842ms", "≤24h"), Signal::Green);
}

#[test]
fn raw_number_lower_bound() {
    assert_eq!(sig("700", "<800"), Signal::Green);
    assert_eq!(sig("850", "<800"), Signal::Amber); // within 800 + 80
    assert_eq!(sig("1000", "<800"), Signal::Red);
}

#[test]
fn ratios() {
    // target 5/7 (≈0.714); amber floor 0.643
    assert_eq!(sig("6/7", "5/7"), Signal::Green);
    assert_eq!(sig("5/7", "5/7"), Signal::Green);
    assert_eq!(sig("3/7", "5/7"), Signal::Red); // 0.428 < 0.643
                                                // target 7/7 (1.0); 6/7 = 0.857 < amber floor 0.9 ⇒ red
    assert_eq!(sig("7/7", "7/7"), Signal::Green);
    assert_eq!(sig("6/7", "7/7"), Signal::Red);
}

#[test]
fn exact_target() {
    assert_eq!(sig("10", "=10"), Signal::Green);
    assert_eq!(sig("10.5", "=10"), Signal::Amber); // within band 1.0
    assert_eq!(sig("13", "=10"), Signal::Red);
}

// ───────────────── the amber-band footgun (plan §2.5 开放设计问题) ─────────────────

#[test]
fn availability_band_needs_abs_points() {
    // A flat relative 10% on a ≥99.9% target greenlights ~90% — wrong.
    let rel = parse_target_with("≥99.9%", AmberBand::RelPct(0.10)).unwrap();
    assert_ne!(
        evaluate_metric(&fresh("95%"), &rel, &[]).signal(),
        Signal::Red,
        "RelPct(10%) wrongly tolerates 95% availability (this is the footgun)"
    );

    // AbsPoints(0.1) models it correctly: green ≥99.9, amber ≥99.8, red below.
    let abs = parse_target_with("≥99.9%", AmberBand::AbsPoints(0.1)).unwrap();
    assert_eq!(
        evaluate_metric(&fresh("99.95%"), &abs, &[]).signal(),
        Signal::Green
    );
    assert_eq!(
        evaluate_metric(&fresh("99.85%"), &abs, &[]).signal(),
        Signal::Amber
    );
    assert_eq!(
        evaluate_metric(&fresh("95%"), &abs, &[]).signal(),
        Signal::Red
    );
}

// ───────────────── honest degradation: Missing / stale ─────────────────

#[test]
fn missing_is_unknown_never_green() {
    let t = parse_target("≥5").unwrap();
    let e = evaluate_metric(&Measurement::Missing, &t, &[]);
    assert_eq!(e.signal(), Signal::Unknown);
    assert!(!e.hit);
    // empty string ⇒ Missing
    assert!(matches!(fresh(""), Measurement::Missing));
}

#[test]
fn stale_source_caps_green_at_amber() {
    let t0 = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    let now = t0 + Duration::hours(48); // 48h > Daily window (24h) ⇒ stale
    let stale = measure("8", t0, SourceKind::Manual, &Cadence::Daily, now);
    let target = parse_target("≥5").unwrap();
    let e = evaluate_metric(&stale, &target, &[]);
    assert_eq!(
        e.signal(),
        Signal::Amber,
        "a green over a dead source caps at amber"
    );
    assert!(!e.hit);

    // same value, fresh ⇒ green
    let fresh_m = measure("8", now, SourceKind::Manual, &Cadence::Daily, now);
    assert_eq!(
        evaluate_metric(&fresh_m, &target, &[]).signal(),
        Signal::Green
    );
}

// ───────────────── qualitative targets ─────────────────

#[test]
fn direction_up_never_red_and_needs_two_points() {
    let t = parse_target("↑").unwrap();
    assert_eq!(
        evaluate_metric(&fresh("3"), &t, &[1.0, 2.0, 3.0]).signal(),
        Signal::Green
    );
    assert_eq!(
        evaluate_metric(&fresh("3"), &t, &[3.0, 3.0]).signal(),
        Signal::Amber
    );
    // fewer than two points ⇒ Unknown, never a guess
    assert_eq!(
        evaluate_metric(&fresh("3"), &t, &[5.0]).signal(),
        Signal::Unknown
    );
    assert_eq!(
        evaluate_metric(&fresh("3"), &t, &[]).signal(),
        Signal::Unknown
    );
}

#[test]
fn track_only_is_always_unknown() {
    let t = parse_target("跟踪").unwrap();
    assert_eq!(
        evaluate_metric(&fresh("999"), &t, &[1.0, 2.0]).signal(),
        Signal::Unknown
    );
}

#[test]
fn drive_to_zero_and_full_coverage() {
    assert_eq!(sig("0", "清零"), Signal::Green);
    assert_eq!(sig("3", "清零"), Signal::Red);
    assert_eq!(sig("100%", "全覆盖"), Signal::Green);
    assert_eq!(sig("95%", "全覆盖"), Signal::Red);
}

// ───────────────── L4/L6 reduce_worst_of ─────────────────

#[test]
fn worst_of_lattice() {
    use Signal::*;
    let r = |v: &[Signal]| reduce_worst_of(v.iter().copied()).into_inner();

    assert_eq!(r(&[Green, Green, Green]), Green);
    assert_eq!(r(&[Green, Amber]), Amber);
    assert_eq!(r(&[Green, Red, Amber]), Red);
    // unknown is tolerated only when something is actually green
    assert_eq!(r(&[Green, Unknown]), Green);
    assert_eq!(r(&[Unknown, Amber]), Amber);
    // no green + an unknown ⇒ unknown (not green)
    assert_eq!(r(&[Unknown, Unknown]), Unknown);
    // empty ⇒ unknown (no data ≠ healthy)
    assert_eq!(r(&[]), Unknown);
}

// ───────────────── the seal in practice ─────────────────

#[test]
fn cache_fields_only_fillable_via_derive() {
    use bw_core::model::{
        Cadence, FeedItem, OpStage, Project, ProjectCycle, ProjectPhase, Routine, StageKind,
    };

    // A routine whose L4 signal can ONLY be set from a derived value.
    let routine = Routine {
        schedule: Cadence::Weekly,
        signal: Some(reduce_worst_of([Signal::Red])), // ← sealed; no literal possible
        watches: vec!["错误率".into()],
        feed: Vec::<FeedItem>::new(),
    };
    assert_eq!(routine.signal(), Signal::Red);

    let stage = OpStage {
        kind: StageKind::Ops,
        progress: 80,
        trend: vec![],
        metrics: vec![],
        routine,
        dod: vec![false, false, false],
        create: vec![],
        optimize: vec![],
    };
    assert_eq!(stage.health(), Signal::Red); // L5 projection

    let project = Project {
        id: bw_core::ProjectId::nil(),
        name: "Demo".into(),
        kind: "看板 / 网页应用".into(),
        desc: String::new(),
        phase: ProjectPhase::Running,
        cycle: ProjectCycle::Explore,
        active_stage: StageKind::Ops,
        signal: None, // cache miss
        progress: 80,
        stages: vec![stage],
        north_star: String::new(),
        ns_def: String::new(),
        weekly_signal: None,
    };
    // L6: project rolls up to its worst stage; an empty cache reads Unknown.
    assert_eq!(project.signal(), Signal::Unknown);
    assert_eq!(project.derive_signal().into_inner(), Signal::Red);
}

#[test]
fn signal_serializes_lowercase() {
    assert_eq!(serde_json::to_string(&Signal::Green).unwrap(), "\"green\"");
    assert_eq!(
        serde_json::to_string(&Signal::Unknown).unwrap(),
        "\"unknown\""
    );
    // Derived<Signal> is transparent over its inner value.
    let d = reduce_worst_of([Signal::Amber]);
    assert_eq!(serde_json::to_string(&d).unwrap(), "\"amber\"");
}

#[test]
fn stage_kind_indices_and_labels() {
    assert_eq!(bw_core::StageKind::Prototype.index(), 1);
    assert_eq!(bw_core::StageKind::Ops.index(), 5);
    assert_eq!(bw_core::StageKind::Prototype.label(), "原型");
    assert_eq!(bw_core::StageKind::ALL.len(), 5);
    // The loop closes: Ops hands back to Prototype (reflux), not off a cliff.
    assert_eq!(
        bw_core::StageKind::Ops.next(),
        bw_core::StageKind::Prototype
    );
    for k in bw_core::StageKind::ALL {
        assert_eq!(k.dod_items().len(), 3, "{k:?} should carry 3 DoD items");
        assert_eq!(k.ai_crew().len(), 3, "{k:?} should carry 3 AI-crew entries");
        assert!(!k.method_loop().is_empty());
    }
}

#[test]
fn project_cycle_mix_sums_to_100() {
    use bw_core::model::ProjectCycle;
    for c in [
        ProjectCycle::Explore,
        ProjectCycle::Expand,
        ProjectCycle::Mature,
    ] {
        let sum: u16 = c.mix().iter().map(|&v| v as u16).sum();
        assert_eq!(sum, 100, "{c:?} mix must sum to 100");
    }
}

// ───────────────── real scheduler: cron_due ─────────────────

#[test]
fn cron_due_never_run_is_immediately_due() {
    use bw_core::model::cron_due;
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    for c in [Cadence::RealTime, Cadence::Daily, Cadence::Weekly] {
        assert!(
            cron_due(&c, None, now),
            "{c:?} with no last_run must be due"
        );
    }
    // The one honest exception: unparsed raw cron expressions never claim to
    // know, even for a task that's never run.
    assert!(!cron_due(&Cadence::Cron("0 9 * * 1".into()), None, now));
}

#[test]
fn cron_due_real_time_always_due() {
    use bw_core::model::cron_due;
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    assert!(cron_due(&Cadence::RealTime, Some(now), now));
    assert!(cron_due(
        &Cadence::RealTime,
        Some(now - Duration::seconds(1)),
        now
    ));
}

#[test]
fn cron_due_daily_respects_real_24h() {
    use bw_core::model::cron_due;
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    assert!(!cron_due(
        &Cadence::Daily,
        Some(now - Duration::hours(23)),
        now
    ));
    assert!(cron_due(
        &Cadence::Daily,
        Some(now - Duration::hours(24)),
        now
    ));
    assert!(cron_due(
        &Cadence::Daily,
        Some(now - Duration::hours(25)),
        now
    ));
}

#[test]
fn cron_due_weekly_respects_real_7d() {
    use bw_core::model::cron_due;
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    assert!(!cron_due(
        &Cadence::Weekly,
        Some(now - Duration::days(6)),
        now
    ));
    assert!(cron_due(
        &Cadence::Weekly,
        Some(now - Duration::days(7)),
        now
    ));
}

#[test]
fn cron_next_run_label_never_guesses() {
    use bw_core::model::{cron_next_run_label, CronStatus};
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();

    assert_eq!(
        cron_next_run_label(&Cadence::Daily, Some(now), CronStatus::Paused, now),
        "已暂停"
    );
    assert_eq!(
        cron_next_run_label(
            &Cadence::Cron("0 9 * * 1".into()),
            None,
            CronStatus::Normal,
            now
        ),
        "不支持自动触发(cron 表达式)"
    );
    assert_eq!(
        cron_next_run_label(&Cadence::Daily, None, CronStatus::Normal, now),
        "等待下次检查",
        "never-run is due now, not a fabricated future time"
    );
    assert_eq!(
        cron_next_run_label(
            &Cadence::Daily,
            Some(now - Duration::hours(23)),
            CronStatus::Normal,
            now
        ),
        "约 1 小时后"
    );
    assert_eq!(
        cron_next_run_label(
            &Cadence::Weekly,
            Some(now - Duration::days(2)),
            CronStatus::Normal,
            now
        ),
        "约 5 天后"
    );
}

#[test]
fn cron_due_raw_cron_expr_unsupported_not_fabricated() {
    use bw_core::model::cron_due;
    let now = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
    // Even a task that's been "overdue" for a year doesn't get guessed at.
    assert!(!cron_due(
        &Cadence::Cron("*/5 * * * *".into()),
        Some(now - Duration::days(365)),
        now
    ));
}
