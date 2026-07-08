//! ViewModel DTOs + pure builders — the `buildApp()` port, phase 2 batch.
//!
//! Everything here is a pure function over `bw-core` types and primitives;
//! `app-desktop` maps store rows into these inputs. Two honesty rules carry
//! through from the plan:
//!
//! * a missing cached signal renders as [`Signal::Unknown`], never green;
//! * a trend is the **real observation history** (via
//!   [`bw_core::derive::parse_magnitude`]) — one recorded value = one point.
//!   Nothing is interpolated or invented.

use crate::{overview_attention, sparkline_path, Attention, SparkPath, StageAttention};
use bw_core::derive::parse_magnitude;
use bw_core::model::{
    Cadence, FeedLevel, ProjectCycle, ProjectPhase, SessionStatus, Signal, SourceKind, StageKind,
};
use bw_core::{MetricId, ProjectId, SessionId};
use time::OffsetDateTime;

/// A cached signal read: cache miss = `Unknown`, never green.
pub fn resolved(cache: Option<Signal>) -> Signal {
    cache.unwrap_or(Signal::Unknown)
}

// ───────────────────────── project wall ─────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct ProjectCardVm {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
    pub running: bool,
    /// 运营中 / 创建中
    pub phase_label: &'static str,
    pub signal: Signal,
    pub progress: u8,
    /// 创建中:desc 预览;运营中:"5 段 · kind · 当前 {active_stage}"
    pub meta: String,
    pub cycle_label: &'static str,
}

/// Build one wall card. `stage_progresses` = the project's real stage progress
/// values (empty while cold-starting, before any stage is materialized).
#[allow(clippy::too_many_arguments)]
pub fn project_card(
    id: ProjectId,
    name: &str,
    kind: &str,
    desc: &str,
    phase: ProjectPhase,
    cycle: ProjectCycle,
    active_stage: StageKind,
    signal: Option<Signal>,
    stage_progresses: &[u8],
) -> ProjectCardVm {
    let running = phase == ProjectPhase::Running;
    let progress = if running {
        crate::overall_progress(stage_progresses)
    } else {
        0 // nothing materializes until creation is confirmed — no invented interim %
    };
    let meta = if running {
        format!(
            "{} 段 · {} · 当前 {}",
            stage_progresses.len().max(StageKind::ALL.len()),
            kind,
            active_stage.label()
        )
    } else if desc.is_empty() {
        format!("创建中 · {kind}")
    } else {
        desc.chars().take(40).collect::<String>()
    };
    ProjectCardVm {
        id,
        name: name.into(),
        kind: kind.into(),
        desc: desc.into(),
        running,
        phase_label: if running { "运营中" } else { "创建中" },
        signal: resolved(signal),
        progress,
        meta,
        cycle_label: cycle.label(),
    }
}

// ───────────────────────── operating view ─────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct StageNavItemVm {
    pub kind: StageKind,
    /// 1..=5, zero-padded label ("01".."05") is formatting-side.
    pub n: u8,
    pub label: &'static str,
    pub role_short: &'static str,
    pub color: &'static str,
    pub signal: Signal,
    /// In-progress optimize/create sessions bound to this stage.
    pub active: u32,
}

/// The five stage-axis buttons. `sessions` = (stage, is_active) pairs.
pub fn stage_nav(
    stages: &[(StageKind, Option<Signal>)],
    sessions: &[(Option<StageKind>, bool)],
) -> Vec<StageNavItemVm> {
    StageKind::ALL
        .into_iter()
        .map(|kind| {
            let signal = stages
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, s)| resolved(*s))
                .unwrap_or(Signal::Unknown);
            let active = sessions
                .iter()
                .filter(|(k, live)| *live && *k == Some(kind))
                .count() as u32;
            StageNavItemVm {
                kind,
                n: kind.index(),
                label: kind.label(),
                role_short: kind.role_short(),
                color: kind.color(),
                signal,
                active,
            }
        })
        .collect()
}

/// The health-overview filter, from raw rows (delegates to
/// [`overview_attention`], the tested "green hides" rule).
pub fn attention_from_rows(
    stages: &[(StageKind, Option<Signal>)],
    sessions: &[(Option<StageKind>, bool)],
) -> Attention {
    let inputs: Vec<StageAttention> = stages
        .iter()
        .map(|(kind, sig)| StageAttention {
            kind: *kind,
            signal: resolved(*sig),
            active_sessions: sessions
                .iter()
                .filter(|(k, live)| *live && *k == Some(*kind))
                .count() as u32,
        })
        .collect();
    overview_attention(&inputs)
}

#[derive(Clone, PartialEq, Debug)]
pub struct MetricVm {
    pub id: MetricId,
    pub name: String,
    pub def: String,
    pub leading: bool,
    pub stage_kind: Option<StageKind>,
    pub value_raw: String,
    pub target_raw: String,
    pub last_target: String,
    pub driver: String,
    pub signal: Signal,
    pub hit: Option<bool>,
    /// Latest source is Manual ⇒ carries the「手填 · 未接入度量源」badge.
    pub manual: bool,
    /// Real observation magnitudes, oldest→newest. One point per recorded value.
    pub trend: Vec<f32>,
    /// Sparkline geometry over the trend (empty polyline when <1 point).
    pub spark: SparkPath,
}

/// Sparkline box used by the stage KPI cards (matches prototype wsMetrics).
pub const SPARK_W: f32 = 120.0;
pub const SPARK_H: f32 = 34.0;

#[allow(clippy::too_many_arguments)]
pub fn metric_vm(
    id: MetricId,
    name: &str,
    def: &str,
    leading: bool,
    stage_kind: Option<StageKind>,
    value_raw: &str,
    target_raw: &str,
    last_target: &str,
    driver: &str,
    signal: Option<Signal>,
    hit: Option<bool>,
    source: Option<SourceKind>,
    observation_raws: &[String],
) -> MetricVm {
    let trend: Vec<f32> = observation_raws
        .iter()
        .filter_map(|raw| parse_magnitude(raw).map(|m| m as f32))
        .collect();
    MetricVm {
        id,
        name: name.into(),
        def: def.into(),
        leading,
        stage_kind,
        value_raw: value_raw.into(),
        target_raw: target_raw.into(),
        last_target: last_target.into(),
        driver: driver.into(),
        signal: resolved(signal),
        hit,
        manual: source.map(|s| s.is_manual()).unwrap_or(false),
        spark: sparkline_path(&trend, SPARK_W, SPARK_H),
        trend,
    }
}

/// 本周计划 row (step 7 + progress panel): one leading metric's plan line.
#[derive(Clone, PartialEq, Debug)]
pub struct WeekPlanRowVm {
    pub metric: MetricId,
    pub name: String,
    pub last_target: String,
    /// 上周实际 = the latest real value (we never fabricate a "was").
    pub current: String,
    pub target: String,
    pub driver: String,
    pub hit: Option<bool>,
}

pub fn week_plan_rows(metrics: &[MetricVm]) -> Vec<WeekPlanRowVm> {
    metrics
        .iter()
        .filter(|m| m.leading)
        .map(|m| WeekPlanRowVm {
            metric: m.id,
            name: m.name.clone(),
            last_target: if m.last_target.is_empty() {
                "—".into()
            } else {
                m.last_target.clone()
            },
            current: if m.value_raw.is_empty() {
                "—".into()
            } else {
                m.value_raw.clone()
            },
            target: m.target_raw.clone(),
            driver: m.driver.clone(),
            hit: m.hit,
        })
        .collect()
}

// ───────────────────────── routine feed ─────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct FeedItemVm {
    pub time_label: String,
    pub level: FeedLevel,
    pub text: String,
}

/// One real observation, as input to the feed projection.
#[derive(Clone, PartialEq, Debug)]
pub struct FeedSource {
    pub metric_name: String,
    pub raw: String,
    pub source: SourceKind,
    pub ts: OffsetDateTime,
    /// The metric's *current* derived signal.
    pub current_signal: Signal,
    /// This is the metric's newest observation.
    pub is_latest: bool,
}

/// One observation → one feed line. Newest first. The level echoes the metric's
/// *current* signal for its newest entry (that's what needs an eye); older
/// entries are plain history.
pub fn observation_feed(observations: &[FeedSource], now: OffsetDateTime) -> Vec<FeedItemVm> {
    let mut items: Vec<(OffsetDateTime, FeedItemVm)> = observations
        .iter()
        .map(|o| {
            let level = if o.is_latest {
                match o.current_signal {
                    Signal::Red => FeedLevel::Err,
                    Signal::Amber | Signal::Unknown => FeedLevel::Warn,
                    Signal::Green => FeedLevel::Info,
                }
            } else {
                FeedLevel::Info
            };
            let src = if o.source.is_manual() {
                "手填"
            } else {
                "连接器"
            };
            (
                o.ts,
                FeedItemVm {
                    time_label: time_label(o.ts, now),
                    level,
                    text: format!("{} = {} · {src}", o.metric_name, o.raw),
                },
            )
        })
        .collect();
    items.sort_by_key(|item| std::cmp::Reverse(item.0));
    items.into_iter().map(|(_, i)| i).collect()
}

/// Human time label, prototype-style (`刚刚`/`N分钟前`/`今日`/`昨日`/`N天前`/date).
pub fn time_label(ts: OffsetDateTime, now: OffsetDateTime) -> String {
    let d = now - ts;
    let mins = d.whole_minutes();
    if mins < 1 {
        return "刚刚".into();
    }
    if mins < 60 {
        return format!("{mins}分钟前");
    }
    if d.whole_hours() < 24 && ts.date() == now.date() {
        return "今日".into();
    }
    let days = d.whole_days();
    if days < 2 {
        return "昨日".into();
    }
    if days < 7 {
        return format!("{days}天前");
    }
    format!("{:02}-{:02}", ts.month() as u8, ts.day())
}

// ───────────────────────── labels ─────────────────────────

pub fn cadence_label(c: &Cadence) -> String {
    match c {
        Cadence::RealTime => "实时".into(),
        Cadence::Daily => "每日".into(),
        Cadence::Weekly => "每周".into(),
        Cadence::Cron(e) => format!("cron {e}"),
    }
}

pub fn session_status_label(s: SessionStatus) -> &'static str {
    match s {
        SessionStatus::Active => "进行中",
        SessionStatus::Archived => "已归档",
        SessionStatus::Done => "已完成",
    }
}

pub fn signal_label(s: Signal) -> &'static str {
    match s {
        Signal::Green => "正常演进",
        Signal::Amber => "需要关注",
        Signal::Red => "阻塞",
        Signal::Unknown => "无数据",
    }
}

// ───────────────────────── stat cards ─────────────────────────

/// The three showProgAll stat cards, from real rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct StatCardsVm {
    /// 工作流累计 = create sessions ever run.
    pub workflows_total: u32,
    /// 定时任务运行中 = materialized stages (each carries a standing routine
    /// once the project is running).
    pub routines_active: u32,
    /// 优化中待验收 = active optimize sessions.
    pub optimizing: u32,
}

pub fn stat_cards(
    materialized_stage_count: usize,
    // (kind is create?, is_active)
    sessions: &[(bool, bool)],
) -> StatCardsVm {
    StatCardsVm {
        workflows_total: sessions.iter().filter(|(create, _)| *create).count() as u32,
        routines_active: materialized_stage_count as u32,
        optimizing: sessions
            .iter()
            .filter(|(create, live)| !*create && *live)
            .count() as u32,
    }
}

// ───────────────────────── chat ─────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct SessionCardVm {
    pub id: SessionId,
    pub title: String,
    pub create: bool,
    pub stage_kind: Option<StageKind>,
    pub status_label: &'static str,
    pub active: bool,
}

// ───────────────────────── stage detail (阶段舱) ─────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct DodItemVm {
    pub label: &'static str,
    pub checked: bool,
}

/// One stage's full detail card: static methodology metadata
/// ([`StageKind`]'s own methods) assembled with the project's real DoD
/// checklist state and handoff count. Everything textual here (core
/// question, method loop, AI crew, anti-patterns) is universal methodology
/// content, not project-specific fabrication.
#[derive(Clone, PartialEq, Debug)]
pub struct StageDetailVm {
    pub kind: StageKind,
    pub label: &'static str,
    pub role: &'static str,
    pub methodology: &'static str,
    pub seek: &'static str,
    pub color: &'static str,
    pub cycle_rhythm: &'static str,
    pub core_question: &'static str,
    pub method_loop: Vec<&'static str>,
    pub default_view: &'static str,
    pub lead_focus: &'static str,
    pub ai_crew: Vec<(&'static str, &'static str)>,
    pub anti_patterns: &'static str,
    pub dod: Vec<DodItemVm>,
    pub dod_all_checked: bool,
    pub handoff_label: &'static str,
    /// How many times this project has passed through this stage (0 = never
    /// yet handed off from here).
    pub handoff_count: u32,
}

/// Assemble one stage's detail view. `dod_checked` is the project's real
/// checklist state (same length/index as `kind.dod_items()`); `handoff_count`
/// is how many entries this stage has as `from_stage` in the audit log.
pub fn stage_detail(kind: StageKind, dod_checked: &[bool], handoff_count: u32) -> StageDetailVm {
    let dod: Vec<DodItemVm> = kind
        .dod_items()
        .iter()
        .enumerate()
        .map(|(i, &label)| DodItemVm {
            label,
            checked: dod_checked.get(i).copied().unwrap_or(false),
        })
        .collect();
    let dod_all_checked = !dod.is_empty() && dod.iter().all(|d| d.checked);
    StageDetailVm {
        kind,
        label: kind.label(),
        role: kind.role(),
        methodology: kind.methodology(),
        seek: kind.seek(),
        color: kind.color(),
        cycle_rhythm: kind.cycle_rhythm(),
        core_question: kind.core_question(),
        method_loop: kind.method_loop().to_vec(),
        default_view: kind.default_view(),
        lead_focus: kind.lead_focus(),
        ai_crew: kind.ai_crew().to_vec(),
        anti_patterns: kind.anti_patterns(),
        dod,
        dod_all_checked,
        handoff_label: kind.handoff_label(),
        handoff_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    fn t0() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    #[test]
    fn card_progress_is_real_not_invented() {
        // Cold start: nothing materializes yet, no invented interim %.
        let c = project_card(
            ProjectId::nil(),
            "P",
            "看板",
            "",
            ProjectPhase::ColdStart,
            ProjectCycle::Explore,
            StageKind::Prototype,
            None,
            &[],
        );
        assert_eq!(c.progress, 0);
        assert_eq!(c.phase_label, "创建中");
        assert_eq!(c.signal, Signal::Unknown); // no cache ⇒ Unknown, not green
        assert!(c.meta.contains("创建中"));

        // Running: mean of REAL stage progresses (all zero for a fresh project).
        let r = project_card(
            ProjectId::nil(),
            "P",
            "看板",
            "",
            ProjectPhase::Running,
            ProjectCycle::Explore,
            StageKind::Build,
            Some(Signal::Green),
            &[0, 0, 0, 0, 0],
        );
        assert_eq!(r.progress, 0);
        assert!(r.meta.contains("5 段"));
        assert!(r.meta.contains("构建")); // active_stage surfaces on the wall
    }

    #[test]
    fn trend_is_observation_history() {
        let m = metric_vm(
            MetricId::nil(),
            "对话数",
            "",
            true,
            Some(StageKind::Prototype),
            "3",
            "≥5",
            "",
            "",
            Some(Signal::Red),
            Some(false),
            Some(SourceKind::Manual),
            &["8".into(), "60%".into(), "3".into(), "口径变更".into()],
        );
        // Unparseable entries drop out; nothing is interpolated.
        assert_eq!(m.trend, vec![8.0, 60.0, 3.0]);
        assert!(m.manual);
        assert!(!m.spark.polyline.is_empty());

        // One observation = one honest point, no fake series.
        let single = metric_vm(
            MetricId::nil(),
            "留存",
            "",
            true,
            None,
            "8",
            "≥5",
            "",
            "",
            None,
            None,
            None,
            &["8".into()],
        );
        assert_eq!(single.trend, vec![8.0]);
        assert_eq!(single.signal, Signal::Unknown); // cache miss ⇒ Unknown
    }

    #[test]
    fn stage_nav_covers_all_five_in_order() {
        let nav = stage_nav(
            &[(StageKind::Build, Some(Signal::Amber))],
            &[(Some(StageKind::Build), true), (None, true)],
        );
        assert_eq!(nav.len(), 5);
        assert_eq!(nav[0].n, 1);
        assert_eq!(nav[0].kind, StageKind::Prototype);
        assert_eq!(nav[1].kind, StageKind::Build);
        assert_eq!(nav[1].signal, Signal::Amber);
        assert_eq!(nav[1].active, 1);
        assert_eq!(nav[1].role_short, "构建师");
        // Unmaterialized stages read Unknown, not green.
        assert_eq!(nav[0].signal, Signal::Unknown);
    }

    #[test]
    fn feed_newest_first_with_signal_level() {
        let now = t0() + Duration::hours(2);
        let feed = observation_feed(
            &[
                FeedSource {
                    metric_name: "对话数".into(),
                    raw: "8".into(),
                    source: SourceKind::Manual,
                    ts: t0(),
                    current_signal: Signal::Red,
                    is_latest: false,
                },
                FeedSource {
                    metric_name: "对话数".into(),
                    raw: "3".into(),
                    source: SourceKind::Manual,
                    ts: t0() + Duration::hours(1),
                    current_signal: Signal::Red,
                    is_latest: true,
                },
            ],
            now,
        );
        assert_eq!(feed.len(), 2);
        assert!(feed[0].text.contains("3"), "newest first");
        assert_eq!(feed[0].level, FeedLevel::Err); // latest echoes current red
        assert_eq!(feed[1].level, FeedLevel::Info); // history stays plain
        assert!(feed[0].text.contains("手填"));
    }

    #[test]
    fn time_labels() {
        let now = t0();
        assert_eq!(time_label(now, now), "刚刚");
        assert_eq!(time_label(now - Duration::minutes(5), now), "5分钟前");
        assert_eq!(time_label(now - Duration::days(1), now), "昨日");
        assert_eq!(time_label(now - Duration::days(3), now), "3天前");
        assert_eq!(time_label(now - Duration::days(30), now), "10-15");
    }

    #[test]
    fn week_plan_from_leading_only() {
        let lead = metric_vm(
            MetricId::nil(),
            "对话数",
            "",
            true,
            None,
            "8",
            "≥5",
            "≥4",
            "抓手A",
            Some(Signal::Green),
            Some(true),
            Some(SourceKind::Manual),
            &[],
        );
        let lag = metric_vm(
            MetricId::nil(),
            "周留存",
            "",
            false,
            None,
            "41%",
            "≥45%",
            "",
            "",
            Some(Signal::Amber),
            Some(false),
            Some(SourceKind::Manual),
            &[],
        );
        let rows = week_plan_rows(&[lead, lag]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].last_target, "≥4");
        assert_eq!(rows[0].current, "8");
        assert_eq!(rows[0].hit, Some(true));
    }

    #[test]
    fn stat_cards_from_real_rows() {
        let s = stat_cards(
            5,
            &[(true, false), (true, true), (false, true), (false, false)],
        );
        assert_eq!(s.workflows_total, 2);
        assert_eq!(s.routines_active, 5);
        assert_eq!(s.optimizing, 1);
    }

    #[test]
    fn stage_detail_carries_real_dod_state() {
        let d = stage_detail(StageKind::Prototype, &[true, false, false], 0);
        assert_eq!(d.dod.len(), 3);
        assert!(d.dod[0].checked);
        assert!(!d.dod[1].checked);
        assert!(!d.dod_all_checked);
        assert_eq!(d.handoff_count, 0);
        assert!(!d.method_loop.is_empty());
        assert_eq!(d.ai_crew.len(), 3);

        let clean = stage_detail(StageKind::Build, &[true, true, true], 2);
        assert!(clean.dod_all_checked);
        assert_eq!(clean.handoff_count, 2);
    }
}
