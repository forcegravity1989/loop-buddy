//! The domain entity graph (plan `§2`), modelled so illegal states are
//! unrepresentable. Mirrors the prototype's `state.*` but replaces every
//! hand-written signal with a [`SignalCache`] that only the derive chain can fill.
//!
//! ## A note on `Serialize` without `Deserialize`
//!
//! Structs that embed a [`SignalCache`] (`StageMetric`, `Routine`, `OpStage`,
//! `Project`) derive `Serialize` (export to a UI DTO) but **not** `Deserialize`:
//! a cached signal must never be reconstructed from bytes — it is recomputed on
//! load (plan `§2.5`: "绝不把缓存当权威"). Leaf, signal-free structs are fully
//! `serde`-round-trippable.

use crate::derive::{reduce_worst_of, AmberBand, Derived};
use crate::ids::{ProjectId, SessionId, WorkflowId};
use serde::{Deserialize, Serialize};

/// Health signal. The prototype had three states; `Unknown` is the honesty
/// fourth — "no data" must never default to green (plan `§2.5`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Signal {
    Green,
    Amber,
    Red,
    Unknown,
}

/// Write-through cache for a derived signal. `None` = cache miss / not yet
/// computed ⇒ recompute, never assume green. Only the derive chain can produce
/// the inner `Derived<Signal>` (see [`crate::derive`]).
pub type SignalCache = Option<Derived<Signal>>;

/// Read a signal cache, treating an empty cache as `Unknown` (not green).
fn cached(c: &SignalCache) -> Signal {
    c.as_ref().map(|d| *d.get()).unwrap_or(Signal::Unknown)
}

// ───────────────────────────── metrics ─────────────────────────────

/// Where a value came from. `Manual` is an *explicit* source (a human typed it),
/// not the absence of one — there is no "no source" path that yields a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    GatewayLog,
    Ci,
    GitPr,
    Telemetry,
    Connector,
    /// Hand-entered. Carries a `手填 · 未接入度量源` badge in the UI until a real
    /// connector is bound (Tier D), at which point the badge auto-drops.
    Manual,
}

impl SourceKind {
    /// Manual sources get a standing "not yet wired to a real meter" badge.
    pub fn is_manual(self) -> bool {
        matches!(self, SourceKind::Manual)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricSource {
    pub kind: SourceKind,
    pub note: String,
}

/// Leading metric (controllable, hard-to-fake). Stores *inputs only*; `hit` and
/// signal are derived on demand via [`crate::derive::evaluate_metric`] — never
/// stored as a hand-set truth.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeadingMetric {
    pub name: String,
    pub def: String,
    pub current: String,
    pub target: String,
    pub source: MetricSource,
    pub last_target: String,
    /// This week's lever (prototype `weekPlan.driver`, editable).
    pub driver: String,
}

/// Lagging metric (outcome we ultimately care about).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaggingMetric {
    pub name: String,
    pub def: String,
    pub current: String,
    pub target: String,
}

// ─────────────────────────── op stages ───────────────────────────

/// The seven control points, in order. The variant *is* the position — there is
/// no way to construct an 8th stage or an out-of-range index.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    CompetitorInsight,
    RequirementIntake,
    NorthStar,
    Leading,
    Lagging,
    PrototypeCreate,
    ProgressMgmt,
}

impl StageKind {
    /// All seven, in control-point order.
    pub const ALL: [StageKind; 7] = [
        StageKind::CompetitorInsight,
        StageKind::RequirementIntake,
        StageKind::NorthStar,
        StageKind::Leading,
        StageKind::Lagging,
        StageKind::PrototypeCreate,
        StageKind::ProgressMgmt,
    ];

    /// 1-based control-point number (1..=7).
    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|&k| k == self).unwrap() as u8 + 1
    }

    /// Chinese label used throughout the prototype.
    pub fn label(self) -> &'static str {
        match self {
            StageKind::CompetitorInsight => "竞品洞察",
            StageKind::RequirementIntake => "需求导入",
            StageKind::NorthStar => "北极星指标",
            StageKind::Leading => "引领指标",
            StageKind::Lagging => "滞后指标",
            StageKind::PrototypeCreate => "原型创建",
            StageKind::ProgressMgmt => "进度管理",
        }
    }
}

/// Maturity phase of a stage (drives a badge color, **not** health — L5).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagePhase {
    /// 已定稿
    Finalized,
    /// 迭代中
    Iterating,
    /// 监测中
    Monitoring,
    /// 持续运行
    Running,
}

/// One KPI under a stage. `signal` is the L3 write-through cache.
#[derive(Clone, Debug, Serialize)]
pub struct StageMetric {
    pub name: String,
    /// Latest display value, e.g. `"60%"` / `"5/7"` / `"842ms"`.
    pub value_raw: String,
    /// Target in the mini-DSL, e.g. `"≥5"` / `"≤24h"` / `"清零"`.
    pub target_raw: String,
    /// Per-metric Amber band (default `RelPct(0.10)`).
    pub amber: AmberBand,
    /// Recent series for sparkline + `↑` direction targets.
    pub trend: Vec<f32>,
    /// L3 cache — only [`crate::derive::evaluate_metric`] can fill it.
    pub signal: SignalCache,
}

impl StageMetric {
    /// The cached signal, or `Unknown` if not yet computed.
    pub fn signal(&self) -> Signal {
        cached(&self.signal)
    }
}

/// Evidence→finding→insight chain node (prototype `method.logic[]`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MethodLogicNode {
    pub k: String,
    pub d: String,
    pub c: String,
}

/// A metric row inside a stage's method panel.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MethodMetric {
    pub name: String,
    pub val: String,
    pub unit: String,
    pub target: String,
    pub note: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FunnelStep {
    pub label: String,
    pub n: u32,
}

/// The method panel some stages carry (competitor insight has the full set).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageMethod {
    pub principle: String,
    pub logic: Vec<MethodLogicNode>,
    pub lead: Vec<MethodMetric>,
    pub lag: Vec<MethodMetric>,
    pub funnel: Vec<FunnelStep>,
}

/// One of the seven control points in a running project.
#[derive(Clone, Debug, Serialize)]
pub struct OpStage {
    pub kind: StageKind,
    pub phase: StagePhase,
    pub progress: u8,
    pub trend: Vec<f32>,
    pub metrics: Vec<StageMetric>,
    pub routine: Routine,
    pub method: Option<StageMethod>,
    pub owns: String,
    pub accept: String,
    pub control: String,
    pub create: Vec<Session>,
    pub optimize: Vec<Session>,
}

impl OpStage {
    /// **L5.** Stage health is exactly the routine signal — a pure projection,
    /// not an independent field (plan `§2.5`).
    pub fn health(&self) -> Signal {
        self.routine.signal()
    }
}

// ─────────────────────────── routine ───────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cadence {
    RealTime,
    Daily,
    Weekly,
    Cron(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedLevel {
    Info,
    Warn,
    Err,
}

/// One append-only observation record in a routine feed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeedItem {
    /// Human time label (`今日` / `本周` / `2min前`).
    pub time_label: String,
    pub level: FeedLevel,
    pub text: String,
}

/// Scheduled observation for a stage. `signal` is the L4 worst-of cache.
#[derive(Clone, Debug, Serialize)]
pub struct Routine {
    pub schedule: Cadence,
    /// L4 cache — only [`crate::derive::reduce_worst_of`] can fill it.
    pub signal: SignalCache,
    pub watches: Vec<String>,
    pub feed: Vec<FeedItem>,
}

impl Routine {
    /// The cached routine signal, or `Unknown` if not yet computed.
    pub fn signal(&self) -> Signal {
        cached(&self.signal)
    }
}

// ─────────────────────────── sessions ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// 进行中
    Active,
    /// 已归档
    Archived,
    /// 已完成
    Done,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Builder (the human) — right, dark bubble.
    Builder,
    /// Agent — left, white bubble.
    Agent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: SessionId,
    pub title: String,
    pub snippet: String,
    pub status: SessionStatus,
    pub msgs: Vec<Message>,
}

// ─────────────────────────── workflow ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Maturity {
    /// 成熟
    Mature,
    /// 打磨中
    Polishing,
    /// 新沉淀
    Fresh,
}

/// Static (distilled, reusable) vs dynamic (use-and-discard) workflow.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum WorkflowKind {
    Static {
        maturity: Maturity,
        version: u32,
        uses: u32,
        scope: String,
    },
    Dynamic {
        origin: String,
        stage: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoopConfig {
    pub retries: u8,
    pub max_iter: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRef {
    pub name: String,
    pub def: String,
    pub from: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillRef {
    pub name: String,
    pub def: String,
    pub from: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub id: WorkflowId,
    pub name: String,
    pub kind: WorkflowKind,
    pub prompt: String,
    pub goal: String,
    /// Associated control point (1..=7), if any.
    pub stage_ref: Option<u8>,
    pub phases: Vec<String>,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub loop_config: LoopConfig,
}

// ─────────────────────────── project ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPhase {
    /// 运营中
    Running,
    /// 冷启动中
    ColdStart,
}

/// A product project. `signal` (L6) and `weekly_signal` are derived caches.
#[derive(Clone, Debug, Serialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
    pub phase: ProjectPhase,
    /// L6 cache — only [`crate::derive::reduce_worst_of`] can fill it.
    pub signal: SignalCache,
    pub progress: u8,
    pub stages: Vec<OpStage>,
    pub leading: Vec<LeadingMetric>,
    pub lagging: Vec<LaggingMetric>,
    pub north_star: String,
    pub ns_def: String,
    /// Friday-boundary snapshot of the derived signal (audited override lives in
    /// `weekly_review`, not here).
    pub weekly_signal: SignalCache,
    /// When cold-starting: the current wizard step (0..=7).
    pub cold_step: Option<u8>,
}

impl Project {
    /// **L6.** Project signal = worst-of its seven stages' routine signals.
    /// Always derived (returns a sealed value); never hand-set.
    pub fn derive_signal(&self) -> Derived<Signal> {
        reduce_worst_of(self.stages.iter().map(|s| s.routine.signal()))
    }

    /// The cached project signal, or `Unknown` if not yet computed.
    pub fn signal(&self) -> Signal {
        cached(&self.signal)
    }
}

// ───────────────────────────── hub ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubKind {
    Workflow,
    Skill,
    Agent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HubCard {
    pub id: HubKind,
    pub name: String,
    pub count: u32,
    pub color: String,
    pub desc: String,
    pub items: Vec<String>,
}
