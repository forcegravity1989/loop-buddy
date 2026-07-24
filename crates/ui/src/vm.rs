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
    AgentCard, Artifact, Cadence, Connector, ConnectorStatus, CronMode, CronStatus, CronTask,
    FeedLevel, HubCard, HubKind, HubSource, Issue, IssueStatus, KnowledgeSource, Maturity,
    MaturityPeriod, PhaseMeta, Readiness, RunChanges, RunStatus, RunTrigger, SessionStatus, Signal,
    SkillCard, SourceKind, StageKind, UsageRank, WorkflowKind, WorkflowRun, WorkflowSpec,
};
use bw_core::{
    AgentId, ConnectorId, CronTaskId, IssueId, KnowledgeSourceId, MetricId, ProjectId, SessionId,
    SkillId, WorkflowId,
};
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
    /// A5-H: count of non-terminal issues in this project (same predicate as
    /// the A4 handoff risky-guard) — the wall's "open work" badge. `0` means
    /// the badge doesn't render; this field just carries the honest number.
    pub open_issues: usize,
}

/// Build one wall card. `stage_progresses` = the project's real stage progress
/// values (empty while cold-starting, before any stage is materialized).
#[allow(clippy::too_many_arguments)]
pub fn project_card(
    id: ProjectId,
    name: &str,
    kind: &str,
    desc: &str,
    phase: Readiness,
    cycle: MaturityPeriod,
    active_stage: StageKind,
    signal: Option<Signal>,
    stage_progresses: &[u8],
    open_issues: usize,
) -> ProjectCardVm {
    let running = phase == Readiness::Running;
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
        open_issues,
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

// ───────────────────────── hub library ─────────────────────────

pub fn maturity_label(m: Maturity) -> &'static str {
    match m {
        Maturity::Mature => "成熟",
        Maturity::Polishing => "打磨中",
        Maturity::Fresh => "新沉淀",
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct WorkflowHubRowVm {
    pub id: WorkflowId,
    pub name: String,
    pub source_label: &'static str,
    pub maturity_label: &'static str,
    pub trigger: Option<String>,
    /// First `agents[0].name`, `"—"` if the spec has none.
    pub primary_agent: String,
    /// Pre-formatted, e.g. `"v3"`.
    pub version_label: String,
    pub uses: u32,
    pub goal: String,
    pub phases_count: usize,
    /// Pre-formatted, e.g. `"重试1·迭代3"`.
    pub loop_label: String,
    /// L3(plan/11): the real numbers behind `loop_label` — `WorkflowFlow`
    /// needs them as data, not a string to re-parse.
    pub loop_retries: u8,
    pub loop_max_iter: u8,
    /// Bare phase names — the text-editing surface (`OptimizeWorkflowForm`'s
    /// "阶段流程" input) still works with, and `phases_count` above.
    pub phases: Vec<String>,
    /// T8 (plan/12 §4): the real per-phase role + static reject target —
    /// same "give the component data, not a pre-formatted string" rule as
    /// `loop_retries`/`loop_max_iter` above. `WorkflowFlow` reads this
    /// directly instead of guessing a role from `phases[i]`'s name.
    pub phase_metas: Vec<PhaseMeta>,
    pub skills: Vec<String>,
    pub stage_ref: Option<u8>,
    /// W1: the row's real run record, e.g. `"跑 3 次 · 成功 67%"` — or
    /// `"暂无运行"` when nothing ever ran (never a fabricated `0%`).
    pub record_label: String,
    /// W1: `"最近 07-16"` from the newest run's real timestamp; empty when
    /// there is none.
    pub last_run_label: String,
    /// `None` = 全局/共享;`Some` = 项目自建(plan/10 K1 侧边栏过滤用)。
    pub project_id: Option<ProjectId>,
}

/// W1: fold a workflow's real run aggregate (`UsageRank`, derived from
/// `workflow_run` rows) into its hub row. Separate from [`workflow_hub_row`]
/// so spec-only callers/tests stay untouched; a cold workflow keeps the
/// honest `"暂无运行"` default.
pub fn attach_run_record(row: &mut WorkflowHubRowVm, rank: &UsageRank) {
    if rank.total_runs == 0 {
        return;
    }
    let rate = match rank.success_rate {
        Some(r) => format!("{:.0}%", r * 100.0),
        // Runs exist but none settled yet — unknown, not 0%.
        None => "—".to_string(),
    };
    row.record_label = format!("跑 {} 次 · 成功 {}", rank.total_runs, rate);
    if let Some(ts) = rank.last_run_at {
        if let Ok(t) = time::OffsetDateTime::from_unix_timestamp(ts) {
            row.last_run_label = format!("最近 {:02}-{:02}", u8::from(t.month()), t.day());
        }
    }
}

/// One hub row from a stored [`WorkflowSpec`] — `None` for a `Dynamic` spec
/// (nothing to browse yet; only `Static` entries are hub-catalog items).
pub fn workflow_hub_row(spec: &WorkflowSpec) -> Option<WorkflowHubRowVm> {
    let WorkflowKind::Static {
        maturity,
        version,
        uses,
        source,
        trigger,
        ..
    } = &spec.kind
    else {
        return None;
    };
    Some(WorkflowHubRowVm {
        id: spec.id,
        name: spec.name.clone(),
        source_label: source.label(),
        maturity_label: maturity_label(*maturity),
        trigger: trigger.clone(),
        primary_agent: spec
            .agents
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "—".into()),
        version_label: format!("v{version}"),
        uses: *uses,
        goal: spec.goal.clone(),
        phases_count: spec.phases.len(),
        loop_label: format!(
            "重试{}·迭代{}",
            spec.loop_config.retries, spec.loop_config.max_iter
        ),
        loop_retries: spec.loop_config.retries,
        loop_max_iter: spec.loop_config.max_iter,
        phases: spec.phases.iter().map(|p| p.name.clone()).collect(),
        phase_metas: spec.phases.clone(),
        skills: spec.skills.iter().map(|s| s.name.clone()).collect(),
        stage_ref: spec.stage_ref,
        record_label: "暂无运行".into(),
        last_run_label: String::new(),
        project_id: spec.project_id,
    })
}

/// Group hub rows by `stage_ref` (1..=5) + a 6th "metrics layer" bucket
/// (`stage_ref == None` or unmapped), matching the 5-stage-plus-cross-cutting
/// layout every stage-scoped screen in this app already uses.
pub fn group_by_stage(
    rows: &[WorkflowHubRowVm],
) -> Vec<(Option<StageKind>, Vec<WorkflowHubRowVm>)> {
    let mut groups: Vec<(Option<StageKind>, Vec<WorkflowHubRowVm>)> = StageKind::ALL
        .iter()
        .map(|k| (Some(*k), Vec::new()))
        .collect();
    groups.push((None, Vec::new()));
    for r in rows {
        let idx = r
            .stage_ref
            .and_then(|n| StageKind::ALL.iter().position(|k| k.index() == n))
            .unwrap_or(groups.len() - 1);
        groups[idx].1.push(r.clone());
    }
    groups
}

/// Counts per source label, in a fixed display order — a filter-chip row.
/// The order/labels come from `HubSource::FILTER_CHIP_LABELS` (bw-core), not
/// a locally hardcoded list — T1 (plan/12 §6) retired the old per-library
/// `Omc`/`Ecc` enum variants, so this can no longer name them directly.
pub fn source_chip_counts(rows: &[WorkflowHubRowVm]) -> Vec<(&'static str, usize)> {
    let mut counts: Vec<(&'static str, usize)> = HubSource::FILTER_CHIP_LABELS
        .iter()
        .map(|&label| (label, 0))
        .collect();
    for r in rows {
        if let Some(slot) = counts
            .iter_mut()
            .find(|(label, _)| *label == r.source_label)
        {
            slot.1 += 1;
        }
    }
    counts
}

/// T7 (2026-07-23, plan/12 §0/§2/§3): the shared five-role classification
/// filter every Hub screen (Skill/Agent/Workflow) now applies to its rows —
/// one pure predicate instead of three copy-pasted per-screen closures.
/// `General` is a real, selectable state (not merely "no filter"), matching
/// the ticket's chip row 全部/原型师/构建师/优化师/运营推广师/运维师/通用 —
/// a user can ask "show me only the honestly-unclassified rows" the same
/// way they can ask for a specific stage.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RoleFilter {
    All,
    Stage(StageKind),
    General,
}

impl RoleFilter {
    /// Does `stage_ref` pass this filter?
    pub fn matches(self, stage_ref: Option<StageKind>) -> bool {
        match self {
            RoleFilter::All => true,
            RoleFilter::Stage(k) => stage_ref == Some(k),
            RoleFilter::General => stage_ref.is_none(),
        }
    }
}

/// Real per-role counts for a stage-role filter chip row — shared by all
/// three Hub screens now that Skill/Agent/Workflow carry the identical
/// `Option<StageKind>` classification dimension (T7). Returns the five real
/// stages in loop order plus a trailing 通用 (unclassified) count — never
/// invented: a row with no `stage_ref` is honestly tallied there, never
/// folded into a stage it was never assigned to.
pub fn role_chip_counts(stage_refs: &[Option<StageKind>]) -> (Vec<(StageKind, usize)>, usize) {
    let mut per_stage: Vec<(StageKind, usize)> = StageKind::ALL.iter().map(|&k| (k, 0)).collect();
    let mut general = 0usize;
    for sr in stage_refs {
        match sr {
            Some(k) => {
                if let Some(slot) = per_stage.iter_mut().find(|(s, _)| s == k) {
                    slot.1 += 1;
                }
            }
            None => general += 1,
        }
    }
    (per_stage, general)
}

#[derive(Clone, PartialEq, Debug)]
pub struct WorkflowDetailVm {
    pub row: WorkflowHubRowVm,
    pub prompt: String,
    /// (name, def, from) per agent — the per-workflow-instance description +
    /// provenance tag, not just a bare name.
    pub agents: Vec<(String, String, String)>,
    pub skills: Vec<(String, String, String)>,
    pub phases_numbered: Vec<(usize, String)>,
}

/// The single-workflow "anatomy" view — `None` for a `Dynamic` spec, same
/// rule as [`workflow_hub_row`].
pub fn workflow_detail(spec: &WorkflowSpec) -> Option<WorkflowDetailVm> {
    let row = workflow_hub_row(spec)?;
    Some(WorkflowDetailVm {
        row,
        prompt: spec.prompt.clone(),
        agents: spec
            .agents
            .iter()
            .map(|a| (a.name.clone(), a.def.clone(), a.from.clone()))
            .collect(),
        skills: spec
            .skills
            .iter()
            .map(|s| (s.name.clone(), s.def.clone(), s.from.clone()))
            .collect(),
        phases_numbered: spec
            .phases
            .iter()
            .map(|p| p.name.clone())
            .enumerate()
            .map(|(i, p)| (i + 1, p))
            .collect(),
    })
}

/// P4: one run row inside the Issue-detail overlay — every field is a real
/// recorded value off `workflow_run` (+ the diff between its recorded HEAD
/// pair). Nothing here is recomputed or guessed at render time.
#[derive(Clone, PartialEq, Debug)]
pub struct IssueRunRowVm {
    pub workflow_name: String,
    pub status_label: &'static str,
    pub ok: bool,
    pub trigger_label: &'static str,
    /// `"1.2s"` / `"340ms"`; `"—"` while running.
    pub duration_label: String,
    pub phases_label: String,
    pub error: String,
    /// (path, +added, -deleted) per really-changed file.
    pub changes: Vec<(String, u32, u32)>,
    /// The honest reason a diff is unavailable (mock run / pre-tracking run /
    /// git error); `None` when `changes` is the truth (possibly empty).
    pub changes_unavailable: Option<String>,
}

/// P4: the Issue-detail overlay — header + run history + artifacts. Actions
/// (确认完成/打回/蒸馏) are gated on `status` by the view; the VM only reports.
#[derive(Clone, PartialEq, Debug)]
pub struct IssueDetailVm {
    pub id: IssueId,
    pub number: u32,
    pub title: String,
    pub desc: String,
    pub status: IssueStatus,
    pub status_label: &'static str,
    pub stage: StageKind,
    pub stage_label: &'static str,
    pub assignee_name: Option<String>,
    pub priority_label: &'static str,
    pub blocked_reason: Option<String>,
    pub settled: bool,
    pub runs: Vec<IssueRunRowVm>,
    /// (path, short commit, bytes) per registered artifact version.
    pub artifacts: Vec<(String, String, u64)>,
}

fn duration_label(ms: Option<i64>) -> String {
    match ms {
        None => "—".into(),
        Some(v) if v >= 1000 => format!("{:.1}s", v as f64 / 1000.0),
        Some(v) => format!("{v}ms"),
    }
}

/// P4: assemble the Issue-detail overlay from store-read rows. `changes` is
/// keyed by run id (the app resolves diffs at open time); a run with no entry
/// falls back to "无变更记录".
pub fn issue_detail_vm(
    issue: &Issue,
    runs: &[WorkflowRun],
    changes: &[RunChanges],
    artifacts: &[Artifact],
    agents: &[AgentCard],
) -> IssueDetailVm {
    let run_rows = runs
        .iter()
        .map(|r| {
            let (chg, unavailable) = match changes.iter().find(|(id, _)| *id == r.id) {
                Some((_, Ok(list))) => (list.clone(), None),
                Some((_, Err(why))) => (Vec::new(), Some(why.clone())),
                None => (Vec::new(), Some("无变更记录".to_string())),
            };
            IssueRunRowVm {
                workflow_name: r.workflow_name.clone(),
                status_label: match r.status {
                    RunStatus::Ok => "成功",
                    RunStatus::Failed => "失败",
                    RunStatus::Running => "进行中",
                },
                ok: r.status == RunStatus::Ok,
                trigger_label: match r.trigger {
                    RunTrigger::Manual => "手动",
                    RunTrigger::Scheduled => "定时",
                },
                duration_label: duration_label(r.duration_ms),
                phases_label: format!("{} 个阶段完成", r.phases_completed),
                error: r.error.clone(),
                changes: chg,
                changes_unavailable: unavailable,
            }
        })
        .collect();
    IssueDetailVm {
        id: issue.id,
        number: issue.number,
        title: issue.title.clone(),
        desc: issue.desc.clone(),
        status: issue.status,
        status_label: issue.status.label(),
        stage: issue.stage,
        stage_label: issue.stage.label(),
        assignee_name: issue
            .assignee
            .and_then(|aid| agents.iter().find(|a| a.id == aid))
            .map(|a| a.name.clone()),
        priority_label: issue.priority.label(),
        blocked_reason: issue.blocked_reason.clone(),
        settled: issue.settled_at.is_some(),
        runs: run_rows,
        artifacts: artifacts
            .iter()
            .map(|a| {
                let short = a.git_commit.chars().take(7).collect::<String>();
                (a.path.clone(), short, a.bytes)
            })
            .collect(),
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct SkillCardVm {
    pub id: SkillId,
    pub name: String,
    pub maturity_label: &'static str,
    pub desc: String,
    pub category: String,
    /// T7 (plan/12 §0/§2): the stage-role classification dimension shared
    /// with `AgentCardVm`/`WorkflowHubRowVm` — `None` = 通用/跨阶段. Kept as
    /// the real `StageKind` (not `WorkflowHubRowVm`'s `Option<u8>`), since
    /// `SkillCard.stage_ref` already is one — no round-trip needed here.
    pub stage_ref: Option<StageKind>,
    pub source_label: &'static str,
    /// T11 (plan/12 §7): "改编自 <库名>" — non-`None` iff an edit flipped
    /// this row away from `Official`; the card face renders this as a small
    /// provenance note alongside `source_label` rather than pretending the
    /// row never had a curated origin. See `SkillCard::adapted_from`.
    pub adapted_from: Option<String>,
    pub uses: u32,
    /// Executable body. Empty = catalog reference (the detail panel says so
    /// honestly instead of showing a blank that reads as broken).
    pub content: String,
    /// `None` = 全局/共享;`Some` = 项目自建(plan/10 K1 侧边栏过滤用)。
    pub project_id: Option<ProjectId>,
    /// L4(plan/11): the domain `SkillCard` has carried this since the
    /// distillation feature landed, but no VM ever surfaced it — a real
    /// provenance signal ("出处可信度") was sitting unused. `None` = catalog/
    /// seeded skill, not distilled from a real Issue.
    pub distilled_from_issue: Option<IssueId>,
    /// The agent teammate credited for the issue behind `distilled_from_issue`
    /// — `None` iff that field is `None` (same domain invariant).
    pub origin_agent: Option<AgentId>,
    /// T4(plan/12 §2): the skill folder's real support files (`skill_file`
    /// rows, T2), verbatim — everything except `SKILL.md` itself, which
    /// stays `content` above. Empty = flat skill (self-built/distilled/the
    /// five built-in stage-role skills), the honest signal the detail panel
    /// uses to skip the file tree instead of showing an empty one.
    pub files: Vec<SkillFileVm>,
}

/// One real support file alongside a skill's `SKILL.md` — T2's `skill_file`
/// table read back verbatim, no re-interpretation.
#[derive(Clone, PartialEq, Debug)]
pub struct SkillFileVm {
    pub rel_path: String,
    pub content: String,
}

pub fn skill_card(s: &SkillCard, files: Vec<SkillFileVm>) -> SkillCardVm {
    SkillCardVm {
        id: s.id,
        name: s.name.clone(),
        maturity_label: maturity_label(s.maturity),
        desc: s.desc.clone(),
        category: s.category.clone(),
        stage_ref: s.stage_ref,
        source_label: s.source.label(),
        adapted_from: s.adapted_from.clone(),
        uses: s.uses,
        content: s.content.clone(),
        project_id: s.project_id,
        distilled_from_issue: s.distilled_from_issue,
        origin_agent: s.origin_agent,
        files,
    }
}

/// A skill folder's real directory structure, built purely off `rel_path`
/// strings (T4, plan/12 §2) — no IO, no invented structure: a file at
/// `"references/mocking.md"` nests under a `references` dir node exactly
/// because that's the literal path, nothing more.
#[derive(Clone, PartialEq, Debug)]
pub enum SkillTreeNode {
    Dir {
        name: String,
        /// Full path from the skill root (e.g. `"references"`, or
        /// `"a/b"` when nested) — doubles as the collapse-state key so a
        /// UI's "which dirs are collapsed" set can be plain `HashSet<String>`.
        path: String,
        children: Vec<SkillTreeNode>,
    },
    File {
        name: String,
        /// The real `skill_file.rel_path` — the lookup key back into
        /// `SkillCardVm.files` when a click needs the file's content.
        rel_path: String,
    },
}

/// Build the nested tree `SkillFileBrowser` renders — dirs before files at
/// each level, both alphabetical, for a stable IDE-like listing. Pure and
/// wasm32-clean like every other `ui` selector, so it's the one place this
/// logic exists (E2E can exercise it indirectly through the rendered tree
/// without a second copy of the split-on-`/` logic living in `app-desktop`).
pub fn skill_file_tree(files: &[SkillFileVm]) -> Vec<SkillTreeNode> {
    #[derive(Default)]
    struct Builder {
        files: Vec<(String, String)>,
        dirs: std::collections::BTreeMap<String, Builder>,
    }

    fn into_nodes(prefix: &str, b: Builder) -> Vec<SkillTreeNode> {
        let mut out: Vec<SkillTreeNode> = b
            .dirs
            .into_iter()
            .map(|(name, sub)| {
                let path = if prefix.is_empty() {
                    name.clone()
                } else {
                    format!("{prefix}/{name}")
                };
                let children = into_nodes(&path, sub);
                SkillTreeNode::Dir {
                    name,
                    path,
                    children,
                }
            })
            .collect();
        let mut files = b.files;
        files.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(
            files
                .into_iter()
                .map(|(name, rel_path)| SkillTreeNode::File { name, rel_path }),
        );
        out
    }

    let mut root = Builder::default();
    for f in files {
        let parts: Vec<&str> = f.rel_path.split('/').filter(|p| !p.is_empty()).collect();
        let Some(file_name) = parts.last() else {
            continue;
        };
        let mut node = &mut root;
        for dir in &parts[..parts.len() - 1] {
            node = node.dirs.entry((*dir).to_string()).or_default();
        }
        node.files.push((file_name.to_string(), f.rel_path.clone()));
    }
    into_nodes("", root)
}

#[derive(Clone, PartialEq, Debug)]
pub struct AgentCardVm {
    pub id: AgentId,
    pub name: String,
    pub initial: String,
    pub role: String,
    /// T7 (plan/12 §0/§3): same dimension as `SkillCardVm::stage_ref`.
    pub stage_ref: Option<StageKind>,
    pub maturity_label: &'static str,
    pub skills: Vec<String>,
    pub model: String,
    pub runs: u32,
    /// `""` while `runs == 0` — render as "—" (no evidence), never "0%".
    pub win_rate: String,
    /// Standing instructions. Empty = catalog reference.
    pub instructions: String,
    /// T5 (plan/12 §3): AllowedTools — the card-face Tools chip row, at the
    /// same tier as `skills` chips. Empty = no restriction declared (the
    /// five built-in stage-role agents, or an unedited hand-authored row).
    pub tools: Vec<String>,
    /// T5: human-friendly label for `AgentCard.agent_cli`, e.g. `"Claude
    /// Code"` for the real `"claude-code"` value — the detail panel's
    /// "执行引擎" line. Any other raw value (future codex/cursor) passes
    /// through unmapped rather than being guessed into a nicer label.
    pub agent_cli_label: String,
    /// T5 (plan/12 §6): provenance chip label, same vocabulary
    /// `SkillCardVm::source_label` already surfaces.
    pub source_label: &'static str,
    /// T11 (plan/12 §7): "改编自 <库名>" — same field/reasoning as
    /// `SkillCardVm::adapted_from`.
    pub adapted_from: Option<String>,
    /// `None` = 全局/共享;`Some` = 项目自建(plan/10 K1 侧边栏过滤用)。
    pub project_id: Option<ProjectId>,
}

/// Human-friendly label for a real `AgentCard.agent_cli` value — T5's
/// "执行引擎" detail line. First version: only `"claude-code"` has a real
/// executor (`bw-engine::ClaudeCliExecutor`) behind it, so it's the only one
/// worth a friendly name; anything else passes through as-is rather than
/// inventing a translation for a CLI this app can't actually run yet (real
/// routing lands in T6).
pub fn agent_cli_label(agent_cli: &str) -> String {
    match agent_cli {
        "claude-code" => "Claude Code".to_string(),
        other => other.to_string(),
    }
}

pub fn agent_card(a: &AgentCard) -> AgentCardVm {
    AgentCardVm {
        id: a.id,
        name: a.name.clone(),
        initial: a
            .name
            .chars()
            .next()
            .map(|c| c.to_string())
            .unwrap_or_default(),
        role: a.role.clone(),
        stage_ref: a.stage_ref,
        maturity_label: maturity_label(a.maturity),
        skills: a.skills.iter().map(|t| t.name.clone()).collect(),
        model: a.model.clone(),
        runs: a.runs,
        win_rate: a.win_rate.clone(),
        instructions: a.instructions.clone(),
        tools: a.tools.clone(),
        agent_cli_label: agent_cli_label(&a.agent_cli),
        source_label: a.source.label(),
        adapted_from: a.adapted_from.clone(),
        project_id: a.project_id,
    }
}

// ───────────────────────── issue board (R1) ─────────────────────────

/// One assignable work unit on the Issue board (R1). Scoped to a stage,
/// owned by an agent teammate, carrying a kanban status. Every field traces
/// back to a real `issue` row — nothing invented. `status_color` is the
/// board's per-status accent (precomputed so the view stays simple).
#[derive(Clone, PartialEq)]
pub struct IssueVm {
    pub id: IssueId,
    pub number: u32,
    pub stage: StageKind,
    pub title: String,
    pub desc: String,
    pub status: IssueStatus,
    pub status_label: &'static str,
    pub status_color: &'static str,
    pub priority_label: &'static str,
    pub assignee_name: Option<String>,
    /// A5-H: non-empty only while `status == Blocked` — the board's Blocked
    /// column renders this; every other column ignores it.
    pub blocked_reason: Option<String>,
}

/// Board accent for a status — multica's warning/success/info/destructive
/// theming, in this app's own signal-adjacent palette.
pub fn issue_status_color(s: IssueStatus) -> &'static str {
    match s {
        IssueStatus::Backlog | IssueStatus::Todo => "#9A9384",
        IssueStatus::InProgress => "#B5862F",
        IssueStatus::InReview => "#6E8C5A",
        IssueStatus::Done => "#5F7355",
        IssueStatus::Blocked => "#B0503A",
        IssueStatus::Cancelled => "#9A9384",
    }
}

/// `Issue` → `IssueVm`, resolving the assignee agent's name against the hub
/// roster. An unassigned issue is honestly `None`, not a fabricated name.
pub fn issue_card(i: &Issue, agents: &[AgentCard]) -> IssueVm {
    IssueVm {
        id: i.id,
        number: i.number,
        stage: i.stage,
        title: i.title.clone(),
        desc: i.desc.clone(),
        status: i.status,
        status_label: i.status.label(),
        status_color: issue_status_color(i.status),
        priority_label: i.priority.label(),
        assignee_name: i
            .assignee
            .and_then(|aid| agents.iter().find(|a| a.id == aid).map(|a| a.name.clone())),
        blocked_reason: i.blocked_reason.clone(),
    }
}

/// The 3-card "从 Hub 导入" overview strip — count + a few sample names per
/// library. Takes primitives (not the row `Vec`s themselves) so it stays a
/// plain, easily-tested pure function.
pub fn hub_overview(
    workflow_count: usize,
    workflow_sample: &[String],
    skill_count: usize,
    skill_sample: &[String],
    agent_count: usize,
    agent_sample: &[String],
) -> Vec<HubCard> {
    vec![
        HubCard {
            id: HubKind::Workflow,
            name: "WorkflowHub".into(),
            kind_label: "完整工作流".into(),
            count: workflow_count as u32,
            color: "#B0503A".into(),
            desc: "整套 workflow 模板：含 phases、goal(验收)、loop 配置，导入即可跑".into(),
            items: workflow_sample.iter().take(4).cloned().collect(),
        },
        HubCard {
            id: HubKind::Skill,
            name: "SkillHub".into(),
            kind_label: "可插拔技能".into(),
            count: skill_count as u32,
            color: "#5F7355".into(),
            desc: "单一能力的 skill，可被任意 agent / 工作流复用".into(),
            items: skill_sample.iter().take(4).cloned().collect(),
        },
        HubCard {
            id: HubKind::Agent,
            name: "AgentHub".into(),
            kind_label: "优化好的智能体".into(),
            count: agent_count as u32,
            color: "#5A4E7A".into(),
            desc: "带系统提示与技能组合的 agent，定义各不相同".into(),
            items: agent_sample.iter().take(4).cloned().collect(),
        },
    ]
}

// ───────────────────────── cron / connector / knowledge hub ─────────────────────────

#[derive(Clone, PartialEq, Debug)]
pub struct CronRowVm {
    pub id: CronTaskId,
    pub name: String,
    pub target: String,
    pub schedule_label: String,
    /// Raw scoping fact — `None` = 全部项目. The UI needs the id itself (not
    /// just `project_label`) to actually dispatch a manual "立即执行" run.
    pub project_id: Option<ProjectId>,
    /// "全部项目" when `project_id` is `None`, else the resolved project name
    /// (falls back to a short id-derived label if the project can't be found —
    /// never silently drops the scoping fact).
    pub project_label: String,
    pub status: CronStatus,
    pub status_label: &'static str,
    pub last_run: String,
    pub next_run: String,
    /// L1(plan/11): 到点做什么——`bw_core::model::CronMode` 一直在 domain
    /// struct 上,此前从没有一个 VM 字段读出来过。
    pub mode_label: &'static str,
    /// T10(plan/12 §5): row-front icon distinguishing all four modes at a
    /// glance (🔄/⚙/💬; `CreateIssue` deliberately keeps no icon — see
    /// `CronMode::icon`'s doc).
    pub mode_icon: &'static str,
    /// `CreateIssue` 任务的 Issue 作用阶段;`RunWorkflow` 任务恒 `None`。
    pub issue_stage_label: Option<&'static str>,
    /// `CreateIssue` 任务的 Issue 指派对象名(自由文本,同全仓 by-name 约定)。
    pub issue_assignee: Option<String>,
    /// T10: `RunSkill`'s target line — the real skill's current name, or the
    /// honest `"(技能已删除)"` when the referenced `SkillId` no longer
    /// resolves against the live Skill Hub. `None` for every other mode.
    pub skill_target_label: Option<String>,
    /// T10: `true` only for a `RunSkill` task whose referenced skill no
    /// longer exists — CronHub's honest "失联" marker. The row still
    /// renders, still lets the task be paused; it just can't fire for real
    /// until re-pointed at a live skill (or deleted).
    pub skill_missing: bool,
    /// T10: `RunPrompt`'s first-40-character preview (a real truncation of
    /// `prompt_full`, never a placeholder). `None` for every other mode.
    pub prompt_preview: Option<String>,
    /// T10: `RunPrompt`'s full text, for the "点击展开全文" affordance.
    /// `None` for every other mode.
    pub prompt_full: Option<String>,
}

/// A real, honest first-40-character preview — counts chars (not bytes), so
/// CJK text truncates at a sane visual length instead of mid-codepoint.
/// Appends `…` only when something was actually cut.
fn preview_chars(s: &str, max: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max {
        return trimmed.to_string();
    }
    let mut out: String = trimmed.chars().take(max).collect();
    out.push('…');
    out
}

/// `project_names` resolves `CronTask.project_id` to a display name — pass
/// the real project rows' `(id, name)` pairs, not a hand-maintained lookup.
/// `skills` resolves a `RunSkill` task's real `SkillId` to its current name
/// (or an honest "deleted" reading if it no longer exists) — pass the live
/// Skill Hub rows, not a cached/stale list. `now` feeds `cron_next_run_label`
/// — the real scheduler's own due-check, not the always-empty
/// `CronTask.next_run` column (nothing ever wrote it).
pub fn cron_row(
    c: &CronTask,
    project_names: &[(ProjectId, String)],
    skills: &[SkillCard],
    now: OffsetDateTime,
) -> CronRowVm {
    let project_label = match c.project_id {
        None => "全部项目".to_string(),
        Some(pid) => project_names
            .iter()
            .find(|(id, _)| *id == pid)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "(项目已删除)".to_string()),
    };
    let (skill_target_label, skill_missing) = match &c.mode {
        CronMode::RunSkill { skill_id } => match skills.iter().find(|s| s.id == *skill_id) {
            Some(s) => (Some(s.name.clone()), false),
            None => (Some("(技能已删除)".to_string()), true),
        },
        _ => (None, false),
    };
    let (prompt_preview, prompt_full) = match &c.mode {
        CronMode::RunPrompt { prompt } => (Some(preview_chars(prompt, 40)), Some(prompt.clone())),
        _ => (None, None),
    };
    CronRowVm {
        id: c.id,
        name: c.name.clone(),
        target: c.target.clone(),
        schedule_label: cadence_label(&c.schedule),
        project_id: c.project_id,
        project_label,
        status: c.status,
        status_label: c.status.label(),
        last_run: c.last_run.clone(),
        next_run: bw_core::model::cron_next_run_label(&c.schedule, c.last_run_at, c.status, now),
        mode_label: c.mode.label(),
        mode_icon: c.mode.icon(),
        issue_stage_label: c.issue_stage.map(|s| s.label()),
        issue_assignee: c.issue_assignee.clone(),
        skill_target_label,
        skill_missing,
        prompt_preview,
        prompt_full,
    }
}

/// L1(plan/11): a cron task's real fire history — `bw_core::model::
/// CronEffectiveness` computed by the store (`Store::cron_effectiveness`) but
/// never surfaced past it. Pre-formatted the same "no evidence, never a fake
/// 0%" way every other rate in this app already reads.
#[derive(Clone, PartialEq, Debug)]
pub struct CronEffectivenessVm {
    pub fires: u32,
    pub ok_fires: u32,
    pub failed_fires: u32,
    /// `"67%"`, or `"—(尚无触发)"` when `fires == 0`.
    pub effectiveness_label: String,
    pub avg_duration_label: String,
    /// `"最近 07-21"`, empty when never fired.
    pub last_fire_label: String,
}

pub fn cron_effectiveness_vm(e: &bw_core::model::CronEffectiveness) -> CronEffectivenessVm {
    let effectiveness_label = match e.effectiveness {
        Some(r) => format!("{:.0}%", r * 100.0),
        None => "—(尚无触发)".to_string(),
    };
    let last_fire_label = e
        .last_fire_at
        .and_then(|ts| time::OffsetDateTime::from_unix_timestamp(ts).ok())
        .map(|t| format!("最近 {:02}-{:02}", u8::from(t.month()), t.day()))
        .unwrap_or_default();
    CronEffectivenessVm {
        fires: e.fires,
        ok_fires: e.ok_fires,
        failed_fires: e.failed_fires,
        effectiveness_label,
        avg_duration_label: duration_label(e.avg_duration_ms),
        last_fire_label,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ConnectorCardVm {
    pub id: ConnectorId,
    pub name: String,
    pub initial: String,
    pub kind: String,
    pub status: ConnectorStatus,
    pub status_label: &'static str,
    pub last_sync: String,
    pub scope: String,
    /// `true` only for kinds with a *real* probe (`git-repo`/`claude-cli`) —
    /// the sync button renders only where syncing really does something;
    /// reference entries honestly show none.
    pub syncable: bool,
    /// `None` = 全局(如 claude-cli 探针);`Some` = 项目自有(plan/10 K1
    /// 侧边栏过滤用)。
    pub project_id: Option<ProjectId>,
}

pub fn connector_card(c: &Connector) -> ConnectorCardVm {
    ConnectorCardVm {
        id: c.id,
        name: c.name.clone(),
        initial: c
            .name
            .chars()
            .next()
            .map(|ch| ch.to_string())
            .unwrap_or_default(),
        kind: c.kind.clone(),
        status: c.status,
        status_label: c.status.label(),
        last_sync: c.last_sync.clone(),
        scope: c.scope.clone(),
        syncable: matches!(
            c.kind.as_str(),
            bw_core::model::CONNECTOR_KIND_GIT_REPO | bw_core::model::CONNECTOR_KIND_CLAUDE_CLI
        ),
        project_id: c.project_id,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct KnowledgeRowVm {
    pub id: KnowledgeSourceId,
    pub name: String,
    pub kind: String,
    /// Pre-formatted, e.g. `"1.2k 片段"`.
    pub chunks_label: String,
    pub updated_label: String,
    pub used_by: String,
}

pub fn knowledge_row(k: &KnowledgeSource) -> KnowledgeRowVm {
    let chunks_label = if k.chunks >= 1000 {
        format!("{:.1}k 片段", k.chunks as f32 / 1000.0)
    } else {
        format!("{} 片段", k.chunks)
    };
    KnowledgeRowVm {
        id: k.id,
        name: k.name.clone(),
        kind: k.kind.clone(),
        chunks_label,
        updated_label: k.updated_label.clone(),
        used_by: k.used_by.clone(),
    }
}

// ───────────────────────── activity hub ─────────────────────────

/// Input to [`activity_row`] — one `handoff` row already joined with its
/// project's name. `bw-store`'s `GlobalHandoffRow` is the real source (a
/// `handoff` + `project` join); `ui` can't depend on `bw-store` (must stay
/// wasm32-clean), so `app-desktop` re-packs the fields here, mirroring the
/// `FeedSource` pattern above.
#[derive(Clone, Debug)]
pub struct ActivitySource {
    pub project_id: ProjectId,
    pub project_name: String,
    pub from_stage: StageKind,
    pub to_stage: StageKind,
    pub risky: bool,
    pub note: String,
    pub at: OffsetDateTime,
}

#[derive(Clone, PartialEq, Debug)]
pub struct ActivityRowVm {
    pub project_id: ProjectId,
    pub project_name: String,
    pub from_label: &'static str,
    pub to_label: &'static str,
    pub risky: bool,
    pub note: String,
    pub time_label: String,
}

/// One real stage handoff → one activity line. No invented events: every row
/// traces back to an actual `handoff_stage` call (see `Command::HandoffStage`).
pub fn activity_row(a: &ActivitySource, now: OffsetDateTime) -> ActivityRowVm {
    ActivityRowVm {
        project_id: a.project_id,
        project_name: a.project_name.clone(),
        from_label: a.from_stage.label(),
        to_label: a.to_stage.label(),
        risky: a.risky,
        note: a.note.clone(),
        time_label: time_label(a.at, now),
    }
}

// ───────────────────────── notify hub ─────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum NotifyLevel {
    Alert,
    Done,
}

impl NotifyLevel {
    pub fn label(self) -> &'static str {
        match self {
            NotifyLevel::Alert => "告警",
            NotifyLevel::Done => "已完成",
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct NotifyItemVm {
    pub level: NotifyLevel,
    pub title: String,
    pub detail: String,
    pub time_label: String,
}

/// The notify feed has no table of its own — every row is a real status
/// that already flipped somewhere else in the hub library (a failed cron
/// task, an errored connector, a risky or clean stage handoff). Nothing
/// here is hand-authored, so there's no "mark as read": the item disappears
/// once the underlying status changes, same as the badge counts elsewhere.
pub fn notify_feed(
    cron_tasks: &[CronTask],
    connectors: &[Connector],
    activity: &[ActivityRowVm],
) -> Vec<NotifyItemVm> {
    let mut items = Vec::new();
    for c in cron_tasks {
        if c.status == CronStatus::Failed {
            // T10: `target` is a real `SkillId`/full prompt text for the two
            // new modes — never dump that raw payload into a notify line;
            // `mode.label()` says honestly what kind of task this is instead.
            let target_display = match &c.mode {
                CronMode::RunSkill { .. } | CronMode::RunPrompt { .. } => {
                    c.mode.label().to_string()
                }
                _ => c.target.clone(),
            };
            items.push(NotifyItemVm {
                level: NotifyLevel::Alert,
                title: format!("定时任务「{}」失败", c.name),
                detail: format!("目标：{} · 上次运行 {}", target_display, c.last_run),
                time_label: c.last_run.clone(),
            });
        }
    }
    for c in connectors {
        if c.status == ConnectorStatus::Error {
            items.push(NotifyItemVm {
                level: NotifyLevel::Alert,
                title: format!("连接器「{}」异常", c.name),
                detail: format!("{} · 上次同步 {}", c.kind, c.last_sync),
                time_label: c.last_sync.clone(),
            });
        }
    }
    for a in activity {
        if a.risky {
            let detail = if a.note.is_empty() {
                format!("{} → {}", a.from_label, a.to_label)
            } else {
                format!("{} → {} · {}", a.from_label, a.to_label, a.note)
            };
            items.push(NotifyItemVm {
                level: NotifyLevel::Alert,
                title: format!("{} 风险交接", a.project_name),
                detail,
                time_label: a.time_label.clone(),
            });
        } else {
            items.push(NotifyItemVm {
                level: NotifyLevel::Done,
                title: format!("{} 交接完成", a.project_name),
                detail: format!("{} → {}", a.from_label, a.to_label),
                time_label: a.time_label.clone(),
            });
        }
    }
    items
}

// ───────────────────────── settings hub ─────────────────────────

/// The real, process-wide `ClaudeCliExecutor` config — `ui` can't depend on
/// `bw-engine` (must stay wasm32-clean), so `app-desktop` unpacks
/// `ClaudeCliConfig`/`PermissionMode` into primitives before calling
/// [`settings_vm`]. No new table: this mirrors how the value already lived
/// only in memory (env-var-seeded at boot), just now editable at runtime via
/// `Command::SetClaudeConfig` instead of frozen for the process's lifetime.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct SettingsVm {
    /// Raw text for the edit field — empty means "resolve from PATH".
    pub binary_raw: String,
    /// Display copy for the read-only summary row.
    pub binary_label: String,
    pub max_budget_usd: f64,
    pub max_budget_label: String,
    /// `true` iff the mode used when a project has NOT opted into command
    /// execution is `BypassPermissions` — off by default and flagged in the
    /// UI, never silently defaulted on.
    pub bypass_default: bool,
    /// Same, for the mode used when a project HAS opted into command
    /// execution (`allow_commands = true`).
    pub bypass_commands: bool,
}

pub fn settings_vm(
    binary: Option<&str>,
    max_budget_usd: f64,
    bypass_default: bool,
    bypass_commands: bool,
) -> SettingsVm {
    let binary_raw = binary.unwrap_or_default().to_string();
    let binary_label = if binary_raw.trim().is_empty() {
        "自动从 PATH 解析".to_string()
    } else {
        binary_raw.clone()
    };
    SettingsVm {
        binary_raw,
        binary_label,
        max_budget_usd,
        max_budget_label: format!("${max_budget_usd:.2}"),
        bypass_default,
        bypass_commands,
    }
}

// ───────────────────────── version panel ─────────────────────────

/// One real commit — `ui` can't depend on `bw-engine`, so `app-desktop`
/// unpacks its `GitCommit` into this before calling [`commit_row`].
#[derive(Clone, Debug)]
pub struct CommitSource {
    pub short_hash: String,
    pub author: String,
    /// Raw `--date=iso-strict` string, e.g. `2026-07-09T03:15:42+00:00`.
    pub date: String,
    pub subject: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct CommitRowVm {
    pub short_hash: String,
    pub author: String,
    pub date_label: String,
    pub subject: String,
}

/// git's own `iso-strict` date, lightly reformatted (`T` → space, offset
/// dropped) — no date-parsing dependency for a single cosmetic change, same
/// "keep it a plain label" choice already made for `CronTask.last_run`.
pub fn commit_row(c: &CommitSource) -> CommitRowVm {
    let date_label = c
        .date
        .get(0..16)
        .map(|s| s.replace('T', " "))
        .unwrap_or_else(|| c.date.clone());
    CommitRowVm {
        short_hash: c.short_hash.clone(),
        author: c.author.clone(),
        date_label,
        subject: c.subject.clone(),
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub enum VersionLogVm {
    /// Never fetched for this project yet — the screen shows a real call to
    /// action, not an empty list pretending nothing has ever happened.
    #[default]
    NotLoaded,
    /// A real `git log` attempt failed, or `workspace_path` isn't
    /// configured — carries git's (or the config check's) own message.
    Unavailable(String),
    Commits(Vec<CommitRowVm>),
}

pub fn version_log_vm(fetched: Option<Result<Vec<CommitSource>, String>>) -> VersionLogVm {
    match fetched {
        None => VersionLogVm::NotLoaded,
        Some(Err(msg)) => VersionLogVm::Unavailable(msg),
        Some(Ok(commits)) => VersionLogVm::Commits(commits.iter().map(commit_row).collect()),
    }
}

// ───────────────────────── artifact panel ─────────────────────────

/// One file in the Artifact panel — the *latest* registered version of a
/// path, plus how many versions (distinct commits) the registry holds for it.
#[derive(Clone, PartialEq, Debug)]
pub struct ArtifactRowVm {
    pub path: String,
    pub kind_label: &'static str,
    pub bytes_label: String,
    /// Short commit of the latest version; "(未提交)" for a commitless repo.
    pub commit_label: String,
    pub time_label: String,
    /// Registered versions of this path (rows sharing the path).
    pub versions: u32,
    /// Whether the latest version is attributed to a workflow run.
    pub from_run: bool,
    pub stage_label: Option<&'static str>,
}

/// Human byte size — real value, coarse unit (the panel is a registry view,
/// not a disk auditor).
pub fn bytes_label(bytes: u64) -> String {
    if bytes >= 1_048_576 {
        format!("{:.1} MB", bytes as f64 / 1_048_576.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// Fold the raw registry rows (newest first, as `list_artifacts` returns
/// them) into one row per path: latest version wins the display, the rest
/// count as history. Pure fold — no invention, no reordering surprises.
pub fn artifact_rows(rows: &[bw_core::model::Artifact], now: OffsetDateTime) -> Vec<ArtifactRowVm> {
    let mut out: Vec<ArtifactRowVm> = Vec::new();
    for a in rows {
        if let Some(existing) = out.iter_mut().find(|r| r.path == a.path) {
            existing.versions += 1;
            continue;
        }
        out.push(ArtifactRowVm {
            path: a.path.clone(),
            kind_label: a.kind.label(),
            bytes_label: bytes_label(a.bytes),
            commit_label: if a.git_commit.is_empty() {
                "(未提交)".to_string()
            } else {
                a.git_commit.clone()
            },
            time_label: OffsetDateTime::from_unix_timestamp(a.registered_at)
                .map(|ts| time_label(ts, now))
                .unwrap_or_default(),
            versions: 1,
            from_run: a.workflow_run_id.is_some(),
            stage_label: a.stage_kind.map(|k| k.label()),
        });
    }
    out
}

/// P5: the weekly-review card — a pure read of already-recorded facts (issues
/// settled this ISO week, still-open issues, metrics with no observation this
/// week, and the countdown to the 90-day success line). `now` is injected so
/// the date math is deterministic; nothing here is invented.
#[derive(Clone, PartialEq, Debug)]
pub struct WeekReviewVm {
    /// `"本周 07-14 ~ 07-20"` — the ISO (Monday-anchored) week the card covers.
    pub week_label: String,
    /// Issues settled (`settled_at`) within this ISO week.
    pub done_this_week: u32,
    /// Non-terminal issues still open.
    pub open_count: u32,
    /// Metrics whose latest observation predates this week's Monday (or none).
    pub metrics_stale: u32,
    /// `"距 90 天目标剩 23 天"` or `"已过 90 天目标线 5 天"`.
    pub goal_label: String,
    /// `true` once the 90-day line is crossed.
    pub goal_negative: bool,
}

/// The unix timestamp of the current ISO week's Monday, 00:00 UTC. Pure
/// integer math on the epoch (1970-01-01 was a Thursday) — no calendar crate,
/// no DST, no local-time drift. Shared by the label and the "this week"
/// counts so they always agree on the same boundary.
pub fn iso_week_start_unix(now_unix: i64) -> i64 {
    const DAY: i64 = 86_400;
    let days_since_epoch = now_unix.div_euclid(DAY);
    // 1970-01-01 = Thursday → (days + 3) mod 7 gives 0=Monday..6=Sunday.
    let dow = (days_since_epoch + 3).rem_euclid(7);
    (days_since_epoch - dow) * DAY
}

/// P5: assemble the weekly-review card. `done_this_week` / `open_count` /
/// `metrics_stale` are real counts the caller computed off the store; this fn
/// only does the honest date math (week label + 90-day countdown).
pub fn week_review_vm(
    now_unix: i64,
    created_at_unix: i64,
    done_this_week: u32,
    open_count: u32,
    metrics_stale: u32,
) -> WeekReviewVm {
    const DAY: i64 = 86_400;
    let week_start = iso_week_start_unix(now_unix);
    let mon = OffsetDateTime::from_unix_timestamp(week_start).unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let sun = OffsetDateTime::from_unix_timestamp(week_start + 6 * DAY)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    let week_label = format!(
        "本周 {:02}-{:02} ~ {:02}-{:02}",
        u8::from(mon.month()),
        mon.day(),
        u8::from(sun.month()),
        sun.day()
    );
    let days_since = (now_unix - created_at_unix).div_euclid(DAY);
    let remaining = 90 - days_since;
    let (goal_label, goal_negative) = if remaining >= 0 {
        (format!("距 90 天目标剩 {remaining} 天"), false)
    } else {
        (format!("已过 90 天目标线 {} 天", -remaining), true)
    };
    WeekReviewVm {
        week_label,
        done_this_week,
        open_count,
        metrics_stale,
        goal_label,
        goal_negative,
    }
}
