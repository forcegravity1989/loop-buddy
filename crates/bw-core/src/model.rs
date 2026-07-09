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
use crate::ids::{
    AgentId, ConnectorId, CronTaskId, KnowledgeSourceId, ProjectId, SessionId, SkillId, WorkflowId,
};
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

// ─────────────────────────── op stages ───────────────────────────

/// The five stages of the operating loop (体系重构 v2 · 阶段=角色=方法论):
/// each stage is hosted by exactly one role, running exactly one methodology.
/// The variant *is* the position — there is no way to construct a 6th stage or
/// an out-of-range index. The loop closes: [`StageKind::next`] wraps
/// `Ops → Prototype` (运维复盘回流原型 · 线闭成环).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    /// 原型 · 原型师 · 假设驱动探索 · 求真
    Prototype,
    /// 构建 · 构建师 · 规格驱动交付 · 求成
    Build,
    /// 优化 · 优化师 · 度量驱动打磨 · 求简
    Optimize,
    /// 运营推广 · 运营推广师 · 增长实验 · 求增
    Growth,
    /// 运维 · 运维师 · 可靠性工程 SRE · 求稳
    Ops,
}

impl StageKind {
    /// All five, in loop order.
    pub const ALL: [StageKind; 5] = [
        StageKind::Prototype,
        StageKind::Build,
        StageKind::Optimize,
        StageKind::Growth,
        StageKind::Ops,
    ];

    /// 1-based stage number (1..=5).
    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|&k| k == self).unwrap() as u8 + 1
    }

    /// The next stage in the loop. Wraps `Ops → Prototype` — the reflux that
    /// closes the line into a ring (a [`Command::HandoffStage`] dispatched from
    /// `Ops` is a *reflux*, not a dead end).
    pub fn next(self) -> StageKind {
        match self {
            StageKind::Prototype => StageKind::Build,
            StageKind::Build => StageKind::Optimize,
            StageKind::Optimize => StageKind::Growth,
            StageKind::Growth => StageKind::Ops,
            StageKind::Ops => StageKind::Prototype,
        }
    }

    /// Stage name.
    pub fn label(self) -> &'static str {
        match self {
            StageKind::Prototype => "原型",
            StageKind::Build => "构建",
            StageKind::Optimize => "优化",
            StageKind::Growth => "运营推广",
            StageKind::Ops => "运维",
        }
    }

    /// `"原型师 · Prototyper"` style full role label.
    pub fn role(self) -> &'static str {
        match self {
            StageKind::Prototype => "原型师 · Prototyper",
            StageKind::Build => "构建师 · Builder",
            StageKind::Optimize => "优化师 · Optimizer",
            StageKind::Growth => "运营推广师 · Grower",
            StageKind::Ops => "运维师 · Maintainer",
        }
    }

    /// Bare role name (`"原型师"` etc.) — for chips.
    pub fn role_short(self) -> &'static str {
        match self {
            StageKind::Prototype => "原型师",
            StageKind::Build => "构建师",
            StageKind::Optimize => "优化师",
            StageKind::Growth => "运营推广师",
            StageKind::Ops => "运维师",
        }
    }

    /// The stage's methodology name.
    pub fn methodology(self) -> &'static str {
        match self {
            StageKind::Prototype => "假设驱动探索",
            StageKind::Build => "规格驱动交付",
            StageKind::Optimize => "度量驱动打磨",
            StageKind::Growth => "增长实验",
            StageKind::Ops => "可靠性工程 SRE",
        }
    }

    /// One-word motto (`"求真"` etc.) — what this stage optimizes for.
    pub fn seek(self) -> &'static str {
        match self {
            StageKind::Prototype => "求真",
            StageKind::Build => "求成",
            StageKind::Optimize => "求简",
            StageKind::Growth => "求增",
            StageKind::Ops => "求稳",
        }
    }

    /// Brand color (hex).
    pub fn color(self) -> &'static str {
        match self {
            StageKind::Prototype => "#C5654A",
            StageKind::Build => "#CC8B3C",
            StageKind::Optimize => "#6E8C5A",
            StageKind::Growth => "#4F7E86",
            StageKind::Ops => "#8A8275",
        }
    }

    /// Typical loop cadence, e.g. `"小时级 · 48h 一圈"`.
    pub fn cycle_rhythm(self) -> &'static str {
        match self {
            StageKind::Prototype => "小时级 · 48h 一圈",
            StageKind::Build => "天级 · Spec → 合入",
            StageKind::Optimize => "天—周级 · 基线 → 回归",
            StageKind::Growth => "周级 · 实验批次",
            StageKind::Ops => "持续 · 无终点",
        }
    }

    /// The question this stage exists to answer.
    pub fn core_question(self) -> &'static str {
        match self {
            StageKind::Prototype => "这个问题真的存在、值得解吗？",
            StageKind::Build => "怎么把验证过的原型，变成生产可用的系统？",
            StageKind::Optimize => "系统扛得住被更多人用吗？哪些东西该删？",
            StageKind::Growth => "增长卡在哪个环节？哪个实验能放大它？",
            StageKind::Ops => "系统此刻健康吗？出了事多快能恢复？",
        }
    }

    /// The repeating method loop, in order (the last step feeds back to the
    /// first — rendered with a trailing `↺`).
    pub fn method_loop(self) -> &'static [&'static str] {
        match self {
            StageKind::Prototype => &["证据", "洞察", "假设", "原型", "验证"],
            StageKind::Build => &[
                "规格 Spec",
                "任务分解",
                "Agent 并行实现",
                "评审合入 · CI 门禁",
            ],
            StageKind::Optimize => &["基线测量", "瓶颈定位", "优化 / 删减", "回归验证"],
            StageKind::Growth => &["漏斗诊断", "实验设计", "A/B 上线", "放大或废弃"],
            StageKind::Ops => &["SLO / 错误预算", "监控告警", "事故响应", "复盘回灌"],
        }
    }

    /// Handoff/DoD checklist items — checked state lives in [`OpStage::dod`],
    /// same index. Not all boxes need to be checked to hand off (an
    /// incomplete handoff is recorded as *risky*, never silently blocked).
    pub fn dod_items(self) -> &'static [&'static str] {
        match self {
            StageKind::Prototype => &[
                "原型经真实使用 · dogfood 验证",
                "北极星草案已定",
                "Spec 骨架已从原型固化",
            ],
            StageKind::Build => &[
                "生产可用 v1 已部署",
                "埋点齐全 · 北极星可采集",
                "性能基线已测",
            ],
            StageKind::Optimize => &[
                "性能 / 成本 / 体验预算全绿",
                "债务台账已建 · 下线清单已执行",
                "可扛 10× 流量的压测证据",
            ],
            StageKind::Growth => &[
                "≥ 1 个可复制的增长循环",
                "获客 / 渗透成本可归因",
                "稳定流量下的 SLO 需求清单",
            ],
            StageKind::Ops => &[
                "SLO / 错误预算持续达标",
                "本轮事故已复盘",
                "复盘洞察已回流原型段",
            ],
        }
    }

    /// `"→ 交棒 构建师"` style label for the handoff button. `Ops`'s handoff is
    /// the reflux, phrased as a loop-back rather than a forward pass.
    pub fn handoff_label(self) -> &'static str {
        match self {
            StageKind::Prototype => "交棒给构建师 · 进入构建段 →",
            StageKind::Build => "交棒给优化师 · 进入优化段 →",
            StageKind::Optimize => "交棒给运营推广师 · 进入推广段 →",
            StageKind::Growth => "交棒给运维师 · 进入运维段 →",
            StageKind::Ops => "↩ 复盘回流 · 交棒原型师(新一环)",
        }
    }

    /// Default workspace view when entering this stage.
    pub fn default_view(self) -> &'static str {
        match self {
            StageKind::Prototype => "洞察板（证据 → 发现 → 洞察）",
            StageKind::Build => "任务树 + CI 状态",
            StageKind::Optimize => "性能预算红绿灯",
            StageKind::Growth => "漏斗 + 实验队列",
            StageKind::Ops => "SLO 面板 + 值班台",
        }
    }

    /// Leading-metric focus called out when entering this stage.
    pub fn lead_focus(self) -> &'static str {
        match self {
            StageKind::Prototype => "洞察密度 · 周验证假设数",
            StageKind::Build => "CI 通过率 · 评审周转",
            StageKind::Optimize => "预算达标率 · 债务燃尽",
            StageKind::Growth => "周实验数 · 激活率",
            StageKind::Ops => "错误预算余量 · MTTR",
        }
    }

    /// Recommended AI crew: `(name, description)`, display-only (real
    /// execution is the colleague team's `Executor`, Tier C).
    pub fn ai_crew(self) -> &'static [(&'static str, &'static str)] {
        match self {
            StageKind::Prototype => &[
                ("竞品分析 Agent", "强检索低臆测，结论必附来源"),
                ("前端原型 Agent", "小时级产出可点原型"),
                ("访谈纪要 skill", "录音 → 结构化发现"),
            ],
            StageKind::Build => &[
                ("编码 Agent 车队", "按任务树并行实现"),
                ("Code Review Agent", "合入前双审之一"),
                ("测试生成 skill", "从验收标准长出用例"),
            ],
            StageKind::Optimize => &[
                ("重构 Agent", "小步等价变换 + 回归护栏"),
                ("性能剖析 skill", "火焰图 → 瓶颈榜"),
                ("死代码扫描 skill", "生成下线候选"),
            ],
            StageKind::Growth => &[
                ("增长分析 Agent", "漏斗分层归因，反对只看均值"),
                ("文案多版本 skill", "一稿出 N 版投放素材"),
                ("A/B 编排工作流", "上线 → 显著性判定全托管"),
            ],
            StageKind::Ops => &[
                ("SRE Agent", "保守可控，改动必留回滚"),
                ("告警模板 skill", "按指标类型生成规则"),
                ("根因分析工作流", "事故 → 时间线 → 假因排序"),
            ],
        }
    }

    /// Common failure modes for this stage (display-only, warns against them).
    pub fn anti_patterns(self) -> &'static str {
        match self {
            StageKind::Prototype => {
                "先写 10 页 PRD 才动手 · 在原型上追求代码质量 · 没验证的想法直接进构建"
            }
            StageKind::Build => {
                "边建边改方向（方向问题退回原型段）· 无验收标准的任务 · 人肉串行做 Agent 能并行的事"
            }
            StageKind::Optimize => {
                "顺手加新功能 · 没有基线就动手 · 只优化不删减（代码量只增不减是警报）"
            }
            StageKind::Growth => {
                "拍脑袋铺渠道不做实验 · 只看均值不看分层 · 实验冲击可靠性却不通知运维师"
            }
            StageKind::Ops => "只灭火不复盘 · 用增长节奏对待稳定性 · 告警噪声不治理（狼来了效应）",
        }
    }
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

/// One of the five stages in a running project. `kind`'s methodology metadata
/// (core question, method loop, DoD item labels, AI crew, anti-patterns) is
/// **static** (see `StageKind` methods) — only the dynamic operating facts
/// live here.
#[derive(Clone, Debug, Serialize)]
pub struct OpStage {
    pub kind: StageKind,
    pub progress: u8,
    pub trend: Vec<f32>,
    pub metrics: Vec<StageMetric>,
    pub routine: Routine,
    /// Handoff/DoD checklist state, same length + index as
    /// [`StageKind::dod_items`]. A human check — never derived, never faked.
    pub dod: Vec<bool>,
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

/// Where a hub-catalog workflow's own definition originated. Only meaningful
/// on `WorkflowKind::Static` — a `Dynamic` (session-scoped, ad-hoc) workflow
/// has no stable provenance to tag, so this stays off that variant entirely
/// rather than becoming an always-present-but-sometimes-meaningless field.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubSource {
    /// oh-my-claudecode
    Omc,
    /// Everything Claude Code
    Ecc,
    /// 自建
    SelfBuilt,
    /// 会话内
    WithinSession,
}

impl HubSource {
    pub fn label(self) -> &'static str {
        match self {
            HubSource::Omc => "OMC",
            HubSource::Ecc => "ECC",
            HubSource::SelfBuilt => "自建",
            HubSource::WithinSession => "会话内",
        }
    }
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
        source: HubSource,
        /// Optional slash-command trigger, e.g. `/security-review`. Not every
        /// hub workflow has one — most are browse-and-import only.
        trigger: Option<String>,
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
    /// Associated stage (1..=5), if any.
    pub stage_ref: Option<u8>,
    pub phases: Vec<String>,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub loop_config: LoopConfig,
}

/// The standard (dynamic, use-and-discard) workflow for one stage, driven
/// straight through its method loop. Pure function of `StageKind`'s own
/// methodology metadata — no UI/store dependency, so both `bw-app` (to
/// reconstruct a promoted workflow's source spec) and `app-desktop` (to run
/// it) can call the identical logic.
///
/// `idgen`-gated (mints a fresh `WorkflowId`) — native-only, matches every
/// other id-minting call in this crate; the wasm32 keepalive build never
/// needs to construct a runnable spec, only the types that describe one.
#[cfg(feature = "idgen")]
pub fn stage_workflow(kind: StageKind) -> WorkflowSpec {
    let goal = format!(
        "{} → {}",
        kind.core_question(),
        kind.dod_items().first().copied().unwrap_or("交棒条件达成")
    );
    WorkflowSpec {
        id: WorkflowId::new(),
        name: format!("「{}」标准工作流", kind.label()),
        kind: WorkflowKind::Dynamic {
            origin: "阶段标准模板".into(),
            stage: kind.label().into(),
        },
        prompt: kind.method_loop().join(" → "),
        goal,
        stage_ref: Some(kind.index()),
        phases: kind.method_loop().iter().map(|s| s.to_string()).collect(),
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
    }
}

/// The drafting run for the creation flow: one workflow, phases matching the
/// "正在按方法论起草体系" loading copy. Runs through the same `Engine` as any
/// other workflow — `MockExecutor` produces a clearly-labeled mock transcript;
/// nothing here is injected into the editable draft fields as fact.
#[cfg(feature = "idgen")]
pub fn drafting_workflow() -> WorkflowSpec {
    WorkflowSpec {
        id: WorkflowId::new(),
        name: "创建 · 体系起草".into(),
        kind: WorkflowKind::Dynamic {
            origin: "创建流程".into(),
            stage: StageKind::Prototype.label().into(),
        },
        prompt: "周期判定 → 北极星起草 → 指标框架 → 阶段激活".into(),
        goal: "产出可编辑的北极星候选 + 指标框架草案".into(),
        stage_ref: Some(StageKind::Prototype.index()),
        phases: vec![
            "周期判定".into(),
            "北极星起草".into(),
            "指标框架".into(),
            "阶段激活".into(),
        ],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 1,
        },
    }
}

// ─────────────────────────── skill / agent hub ───────────────────────────

/// Binary provenance for Skill/Agent hub items — a library entry the
/// platform ships (官方) or one a builder authored locally (自建). Distinct
/// from [`HubSource`] (Workflow's 4-tier provenance): these are two
/// independent, purpose-built vocabularies, not one shared enum.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibSource {
    /// 官方
    Official,
    /// 自建
    SelfBuilt,
}

impl LibSource {
    pub fn label(self) -> &'static str {
        match self {
            LibSource::Official => "官方",
            LibSource::SelfBuilt => "自建",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillCard {
    pub id: SkillId,
    pub name: String,
    /// 2-tier in practice (成熟/打磨中) — a freshly created skill defaults to
    /// `Polishing`, never `Fresh` (see bw-app's `CreateSkill`).
    pub maturity: Maturity,
    pub desc: String,
    pub category: String,
    pub source: LibSource,
    pub uses: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentSkillTag {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentCard {
    pub id: AgentId,
    pub name: String,
    pub role: String,
    pub maturity: Maturity,
    pub skills: Vec<AgentSkillTag>,
    pub model: String,
    pub runs: u32,
    /// Adoption rate as a pre-formatted display string (e.g. `"94%"`) —
    /// matches how metric values are stored as display strings elsewhere.
    pub win_rate: String,
}

// ─────────────────────────── cron / connector / knowledge hub ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CronStatus {
    Running,
    Normal,
    Failed,
    Paused,
}

impl CronStatus {
    pub fn label(self) -> &'static str {
        match self {
            CronStatus::Running => "运行中",
            CronStatus::Normal => "正常",
            CronStatus::Failed => "失败",
            CronStatus::Paused => "暂停",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CronTask {
    pub id: CronTaskId,
    pub name: String,
    /// What it runs — free text (e.g. a workflow/routine name); not a hard FK
    /// since a cron target may be a hub workflow, a connector sync, or
    /// something outside this app entirely.
    pub target: String,
    pub schedule: Cadence,
    /// `None` = 全部项目 (all projects), matching the prototype's own
    /// "全部项目" catch-all option.
    pub project_id: Option<ProjectId>,
    pub status: CronStatus,
    pub last_run: String,
    pub next_run: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorStatus {
    Connected,
    Syncing,
    Error,
    Disconnected,
}

impl ConnectorStatus {
    pub fn label(self) -> &'static str {
        match self {
            ConnectorStatus::Connected => "已连接",
            ConnectorStatus::Syncing => "同步中",
            ConnectorStatus::Error => "异常",
            ConnectorStatus::Disconnected => "未连接",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Connector {
    pub id: ConnectorId,
    pub name: String,
    /// e.g. 可观测性/数据库/代码仓库 — free text, this app has no fixed
    /// connector-type taxonomy yet (Tier D territory).
    pub kind: String,
    pub status: ConnectorStatus,
    pub last_sync: String,
    pub scope: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnowledgeSource {
    pub id: KnowledgeSourceId,
    pub name: String,
    /// e.g. Notion/Markdown/OpenAPI — free text source format.
    pub kind: String,
    pub chunks: u32,
    pub updated_label: String,
    /// Which agent (by name) consumes this source — free text, matching the
    /// prototype's own by-name (not by-id) reference.
    pub used_by: String,
}

// ─────────────────────────── project ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectPhase {
    /// 运营中
    Running,
    /// 冷启动中(创建流程未完成确认)
    ColdStart,
}

/// A project's declared lifecycle position — how it's expected to distribute
/// effort across the five stages (体系重构 v2 `§06`). User-declared at
/// creation (from the "项目处在什么周期" question), purely informational: it
/// biases nothing in the derive chain, only the wall's mix-bar display.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectCycle {
    /// 探索期 · 0→1 · 未达 PMF
    Explore,
    /// 扩张期 · 1→N · 增长
    Expand,
    /// 成熟期 · Sustain
    Mature,
}

impl ProjectCycle {
    pub fn label(self) -> &'static str {
        match self {
            ProjectCycle::Explore => "探索期",
            ProjectCycle::Expand => "扩张期",
            ProjectCycle::Mature => "成熟期",
        }
    }

    pub fn sub_label(self) -> &'static str {
        match self {
            ProjectCycle::Explore => "0→1 · 未达 PMF",
            ProjectCycle::Expand => "1→N · 增长",
            ProjectCycle::Mature => "Sustain · 原「运维」期",
        }
    }

    /// Percentage weight per [`StageKind::ALL`] stage, summing to 100.
    pub fn mix(self) -> [u8; 5] {
        match self {
            ProjectCycle::Explore => [40, 30, 15, 10, 5],
            ProjectCycle::Expand => [10, 25, 20, 30, 15],
            ProjectCycle::Mature => [5, 10, 25, 25, 35],
        }
    }

    pub fn main_loop_label(self) -> &'static str {
        match self {
            ProjectCycle::Explore => "主环 · 原型 ↔ 构建 来回",
            ProjectCycle::Expand => "主环 · 构建 → 优化 → 推广",
            ProjectCycle::Mature => "主环 · 优化 ↔ 运维 · 推广保温",
        }
    }
}

/// A product project. `signal` (L6) and `weekly_signal` are derived caches.
#[derive(Clone, Debug, Serialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub kind: String,
    pub desc: String,
    pub phase: ProjectPhase,
    pub cycle: ProjectCycle,
    /// Which of the five stages is currently hosting the work.
    pub active_stage: StageKind,
    /// L6 cache — only [`crate::derive::reduce_worst_of`] can fill it.
    pub signal: SignalCache,
    pub progress: u8,
    pub stages: Vec<OpStage>,
    pub north_star: String,
    pub ns_def: String,
    /// Friday-boundary snapshot of the derived signal (audited override lives in
    /// `weekly_review`, not here).
    pub weekly_signal: SignalCache,
}

impl Project {
    /// **L6.** Project signal = worst-of its five stages' routine signals.
    /// Always derived (returns a sealed value); never hand-set.
    pub fn derive_signal(&self) -> Derived<Signal> {
        reduce_worst_of(self.stages.iter().map(|s| s.routine.signal()))
    }

    /// The cached project signal, or `Unknown` if not yet computed.
    pub fn signal(&self) -> Signal {
        cached(&self.signal)
    }
}

// ─────────────────────────── handoff ───────────────────────────

/// One audited stage transition (体系重构 v2 `§07`①③): the DoD checklist for
/// `from_stage` need not be fully checked to hand off — an incomplete one is
/// simply recorded as `risky`, never silently blocked. `Ops → Prototype` is
/// the reflux that closes the loop.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HandoffRecord {
    pub from_stage: StageKind,
    pub to_stage: StageKind,
    pub risky: bool,
    pub note: String,
}

// ───────────────────────────── hub ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubKind {
    Workflow,
    Skill,
    Agent,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct HubCard {
    pub id: HubKind,
    pub name: String,
    /// One-line subtitle (e.g. "完整工作流") — distinct from `HubKind`'s own
    /// variant identity.
    pub kind_label: String,
    pub count: u32,
    pub color: String,
    pub desc: String,
    pub items: Vec<String>,
}
