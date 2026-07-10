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
    AgentCard, Cadence, Connector, ConnectorStatus, CronStatus, CronTask, FeedLevel, HubCard,
    HubKind, KnowledgeSource, Maturity, ProjectCycle, ProjectPhase, SessionStatus, Signal,
    SkillCard, SourceKind, StageKind, WorkflowKind, WorkflowSpec,
};
use bw_core::{
    AgentId, ConnectorId, CronTaskId, KnowledgeSourceId, MetricId, ProjectId, SessionId, SkillId,
    WorkflowId,
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
    pub phases: Vec<String>,
    pub skills: Vec<String>,
    pub stage_ref: Option<u8>,
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
        phases: spec.phases.clone(),
        skills: spec.skills.iter().map(|s| s.name.clone()).collect(),
        stage_ref: spec.stage_ref,
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
pub fn source_chip_counts(rows: &[WorkflowHubRowVm]) -> Vec<(&'static str, usize)> {
    let mut counts: Vec<(&'static str, usize)> =
        vec![("OMC", 0), ("ECC", 0), ("自建", 0), ("会话内", 0)];
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
            .cloned()
            .enumerate()
            .map(|(i, p)| (i + 1, p))
            .collect(),
    })
}

#[derive(Clone, PartialEq, Debug)]
pub struct SkillCardVm {
    pub id: SkillId,
    pub name: String,
    pub maturity_label: &'static str,
    pub desc: String,
    pub category: String,
    pub source_label: &'static str,
    pub uses: u32,
}

pub fn skill_card(s: &SkillCard) -> SkillCardVm {
    SkillCardVm {
        id: s.id,
        name: s.name.clone(),
        maturity_label: maturity_label(s.maturity),
        desc: s.desc.clone(),
        category: s.category.clone(),
        source_label: s.source.label(),
        uses: s.uses,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct AgentCardVm {
    pub id: AgentId,
    pub name: String,
    pub initial: String,
    pub role: String,
    pub maturity_label: &'static str,
    pub skills: Vec<String>,
    pub model: String,
    pub runs: u32,
    pub win_rate: String,
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
        maturity_label: maturity_label(a.maturity),
        skills: a.skills.iter().map(|t| t.name.clone()).collect(),
        model: a.model.clone(),
        runs: a.runs,
        win_rate: a.win_rate.clone(),
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
}

/// `project_names` resolves `CronTask.project_id` to a display name — pass
/// the real project rows' `(id, name)` pairs, not a hand-maintained lookup.
pub fn cron_row(c: &CronTask, project_names: &[(ProjectId, String)]) -> CronRowVm {
    let project_label = match c.project_id {
        None => "全部项目".to_string(),
        Some(pid) => project_names
            .iter()
            .find(|(id, _)| *id == pid)
            .map(|(_, name)| name.clone())
            .unwrap_or_else(|| "(项目已删除)".to_string()),
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
        next_run: c.next_run.clone(),
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct ConnectorCardVm {
    pub id: ConnectorId,
    pub name: String,
    pub initial: String,
    pub kind: String,
    pub status_label: &'static str,
    pub last_sync: String,
    pub scope: String,
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
        status_label: c.status.label(),
        last_sync: c.last_sync.clone(),
        scope: c.scope.clone(),
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
            items.push(NotifyItemVm {
                level: NotifyLevel::Alert,
                title: format!("定时任务「{}」失败", c.name),
                detail: format!("目标：{} · 上次运行 {}", c.target, c.last_run),
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

    fn static_spec(stage_ref: Option<u8>) -> WorkflowSpec {
        WorkflowSpec {
            id: WorkflowId::nil(),
            name: "深度访谈 → 问题定义".into(),
            kind: WorkflowKind::Static {
                maturity: Maturity::Mature,
                version: 3,
                uses: 12,
                scope: "跨项目复用".into(),
                source: bw_core::model::HubSource::SelfBuilt,
                trigger: Some("deep interview".into()),
            },
            prompt: "界定→采集→结构化→分析".into(),
            goal: "产出验证过的问题陈述".into(),
            stage_ref,
            phases: vec!["访谈提纲".into(), "深挖场景".into()],
            agents: vec![],
            skills: vec![],
            loop_config: bw_core::model::LoopConfig {
                retries: 2,
                max_iter: 3,
            },
        }
    }

    fn dynamic_spec() -> WorkflowSpec {
        WorkflowSpec {
            id: WorkflowId::nil(),
            name: "「原型」标准工作流".into(),
            kind: WorkflowKind::Dynamic {
                origin: "阶段标准模板".into(),
                stage: "原型".into(),
            },
            prompt: "p".into(),
            goal: "g".into(),
            stage_ref: Some(1),
            phases: vec![],
            agents: vec![],
            skills: vec![],
            loop_config: bw_core::model::LoopConfig {
                retries: 1,
                max_iter: 1,
            },
        }
    }

    #[test]
    fn workflow_hub_row_returns_none_for_dynamic() {
        assert!(workflow_hub_row(&dynamic_spec()).is_none());
    }

    #[test]
    fn workflow_hub_row_reads_static_fields() {
        let row = workflow_hub_row(&static_spec(Some(1))).unwrap();
        assert_eq!(row.source_label, "自建");
        assert_eq!(row.maturity_label, "成熟");
        assert_eq!(row.trigger.as_deref(), Some("deep interview"));
        assert_eq!(row.version_label, "v3");
        assert_eq!(row.uses, 12);
        assert_eq!(row.loop_label, "重试2·迭代3");
        assert_eq!(row.primary_agent, "—");
    }

    #[test]
    fn group_by_stage_covers_six_groups_in_order() {
        let rows = vec![
            workflow_hub_row(&static_spec(Some(1))).unwrap(),
            workflow_hub_row(&static_spec(None)).unwrap(),
        ];
        let groups = group_by_stage(&rows);
        assert_eq!(groups.len(), 6);
        assert_eq!(groups[0].0, Some(StageKind::Prototype));
        assert_eq!(groups[0].1.len(), 1);
        assert_eq!(groups[5].0, None, "6th group is the metrics-layer bucket");
        assert_eq!(groups[5].1.len(), 1);
    }

    #[test]
    fn source_chip_counts_tallies_by_label() {
        let rows = vec![
            workflow_hub_row(&static_spec(Some(1))).unwrap(),
            workflow_hub_row(&static_spec(Some(2))).unwrap(),
        ];
        let counts = source_chip_counts(&rows);
        let self_built = counts.iter().find(|(l, _)| *l == "自建").unwrap();
        assert_eq!(self_built.1, 2);
    }

    #[test]
    fn workflow_detail_carries_agent_and_skill_provenance() {
        let mut spec = static_spec(Some(1));
        spec.agents.push(bw_core::model::AgentRef {
            name: "竞品分析 Agent".into(),
            def: "强检索、低臆测".into(),
            from: "AgentHub".into(),
        });
        let detail = workflow_detail(&spec).unwrap();
        assert_eq!(detail.agents.len(), 1);
        assert_eq!(detail.agents[0].0, "竞品分析 Agent");
        assert_eq!(detail.agents[0].2, "AgentHub");
        assert_eq!(
            detail.phases_numbered,
            vec![(1, "访谈提纲".into()), (2, "深挖场景".into())]
        );
    }

    #[test]
    fn skill_card_maps_2tier_maturity() {
        let card = skill_card(&SkillCard {
            id: SkillId::nil(),
            name: "web-scan".into(),
            maturity: Maturity::Polishing,
            desc: "d".into(),
            category: "检索".into(),
            source: bw_core::model::LibSource::SelfBuilt,
            uses: 128,
        });
        assert_eq!(card.maturity_label, "打磨中");
        assert_eq!(card.source_label, "自建");
        assert_eq!(card.uses, 128);
    }

    #[test]
    fn agent_card_derives_initial_and_skill_names() {
        let card = agent_card(&AgentCard {
            id: AgentId::nil(),
            name: "竞品分析 Agent".into(),
            role: "r".into(),
            maturity: Maturity::Mature,
            skills: vec![bw_core::model::AgentSkillTag {
                name: "web-scan".into(),
            }],
            model: "claude-opus".into(),
            runs: 213,
            win_rate: "94%".into(),
        });
        assert_eq!(card.initial, "竞");
        assert_eq!(card.skills, vec!["web-scan".to_string()]);
        assert_eq!(card.runs, 213);
    }

    #[test]
    fn hub_overview_builds_three_cards_with_capped_samples() {
        let names = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
            "e".to_string(),
        ];
        let cards = hub_overview(53, &names, 340, &names, 96, &names);
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0].id, HubKind::Workflow);
        assert_eq!(cards[0].count, 53);
        assert_eq!(cards[0].items.len(), 4, "sample list caps at 4 items");
        assert_eq!(cards[1].id, HubKind::Skill);
        assert_eq!(cards[2].id, HubKind::Agent);
    }

    #[test]
    fn activity_row_labels_stages_and_carries_risky_flag() {
        let row = activity_row(
            &ActivitySource {
                project_id: ProjectId::nil(),
                project_name: "智能客服知识库".into(),
                from_stage: StageKind::Prototype,
                to_stage: StageKind::Build,
                risky: true,
                note: "赶工期，测试覆盖不足".into(),
                at: t0(),
            },
            t0() + Duration::minutes(5),
        );
        assert_eq!(row.project_name, "智能客服知识库");
        assert_eq!(row.from_label, StageKind::Prototype.label());
        assert_eq!(row.to_label, StageKind::Build.label());
        assert!(row.risky);
        assert_eq!(row.time_label, "5分钟前");
    }

    #[test]
    fn notify_feed_surfaces_only_flipped_signals() {
        let cron_tasks = vec![
            CronTask {
                id: CronTaskId::nil(),
                name: "夜间索引".into(),
                target: "knowledge-sync".into(),
                schedule: Cadence::Daily,
                project_id: None,
                status: CronStatus::Failed,
                last_run: "1h 前".into(),
                next_run: "-".into(),
            },
            CronTask {
                id: CronTaskId::nil(),
                name: "健康扫描".into(),
                target: "health-check".into(),
                schedule: Cadence::Daily,
                project_id: None,
                status: CronStatus::Normal,
                last_run: "10min 前".into(),
                next_run: "今晚".into(),
            },
        ];
        let connectors = vec![Connector {
            id: ConnectorId::nil(),
            name: "飞书云文档".into(),
            kind: "知识库".into(),
            status: ConnectorStatus::Error,
            last_sync: "2h 前".into(),
            scope: "全部项目".into(),
        }];
        let activity = vec![
            activity_row(
                &ActivitySource {
                    project_id: ProjectId::nil(),
                    project_name: "P1".into(),
                    from_stage: StageKind::Prototype,
                    to_stage: StageKind::Build,
                    risky: true,
                    note: "".into(),
                    at: t0(),
                },
                t0(),
            ),
            activity_row(
                &ActivitySource {
                    project_id: ProjectId::nil(),
                    project_name: "P2".into(),
                    from_stage: StageKind::Build,
                    to_stage: StageKind::Optimize,
                    risky: false,
                    note: "".into(),
                    at: t0(),
                },
                t0(),
            ),
        ];
        let items = notify_feed(&cron_tasks, &connectors, &activity);
        // 1 failed cron + 1 errored connector + 1 risky handoff = 3 alerts;
        // the normal cron contributes nothing, the clean handoff contributes
        // exactly 1 "done" entry.
        let alerts = items
            .iter()
            .filter(|i| i.level == NotifyLevel::Alert)
            .count();
        let done = items
            .iter()
            .filter(|i| i.level == NotifyLevel::Done)
            .count();
        assert_eq!(alerts, 3);
        assert_eq!(done, 1);
        assert!(items.iter().any(|i| i.title.contains("夜间索引")));
        assert!(items.iter().any(|i| i.title.contains("飞书云文档")));
    }

    #[test]
    fn settings_vm_labels_unconfigured_binary_and_formats_budget() {
        let auto = settings_vm(None, 0.5, false, false);
        assert_eq!(auto.binary_label, "自动从 PATH 解析");
        assert_eq!(auto.binary_raw, "");
        assert_eq!(auto.max_budget_label, "$0.50");
        assert!(!auto.bypass_default);
        assert!(!auto.bypass_commands);

        let custom = settings_vm(Some("/usr/local/bin/claude"), 2.0, true, false);
        assert_eq!(custom.binary_label, "/usr/local/bin/claude");
        assert_eq!(custom.max_budget_label, "$2.00");
        assert!(custom.bypass_default);
    }

    #[test]
    fn commit_row_reformats_iso_date_without_parsing_it() {
        let row = commit_row(&CommitSource {
            short_hash: "abc12".into(),
            author: "Builder".into(),
            date: "2026-07-09T03:15:42+00:00".into(),
            subject: "feat: real git log".into(),
        });
        assert_eq!(row.short_hash, "abc12");
        assert_eq!(row.date_label, "2026-07-09 03:15");
        assert_eq!(row.subject, "feat: real git log");
    }

    #[test]
    fn version_log_vm_distinguishes_not_loaded_unavailable_and_commits() {
        assert_eq!(version_log_vm(None), VersionLogVm::NotLoaded);
        assert_eq!(
            version_log_vm(Some(Err("工作目录未配置".to_string()))),
            VersionLogVm::Unavailable("工作目录未配置".to_string())
        );
        let source = CommitSource {
            short_hash: "abc12".into(),
            author: "Builder".into(),
            date: "2026-07-09T03:15:42+00:00".into(),
            subject: "s".into(),
        };
        match version_log_vm(Some(Ok(vec![source]))) {
            VersionLogVm::Commits(rows) => assert_eq!(rows.len(), 1),
            other => panic!("expected Commits, got {other:?}"),
        }
    }
}
