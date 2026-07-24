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
    AgentId, ArtifactId, ConnectorId, CronTaskId, IssueId, KnowledgeSourceId, ProjectId, SessionId,
    SkillId, WorkflowId, WorkflowRunId,
};
use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

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

/// The five stages of the project's lifecycle (体系重构 v2 · 阶段=角色=方法论):
/// each stage is hosted by exactly one role, running exactly one methodology.
/// The variant *is* the position — there is no way to construct a 6th stage or
/// an out-of-range index. The five stages close into a loop-back, not a
/// pipeline: [`StageKind::next`] wraps `Ops → Prototype`
/// (运维复盘回流原型 · 闭环回流). Not to be confused with a workflow's own
/// internal retry loop ([`LoopConfig`]) — that's a different "loop".
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

    /// Inverse of [`Self::index`] — `None` for `0` or `6..`. T7 (plan/12 §0):
    /// the shared conversion `Skill`/`Agent` need to interop with
    /// `WorkflowSpec.stage_ref`'s existing `Option<u8>` (1..=5) storage
    /// convention while their own domain field stays `Option<StageKind>` —
    /// same `StageKind::ALL.iter().find(|s| s.index() == n)` idiom
    /// `bw_core::analysis` and `bw-store`'s workflow-side code already used
    /// inline at several call sites, named once here instead of repeated.
    pub fn from_index(n: u8) -> Option<StageKind> {
        Self::ALL.iter().find(|k| k.index() == n).copied()
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
            StageKind::Build => "构建师 · Constructor",
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
pub enum Author {
    /// Builder (the human) — right, dark bubble.
    Builder,
    /// Agent — left, white bubble.
    Agent,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Author,
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
///
/// T1 (2026-07-23, plan/12 §6): collapsed from 5 variants down to 4. Curated
/// external libraries (OMC, ECC, mattpocock-skills, superpowers, …) are all
/// the same kind of thing — "官方选型预置", an open-ended and ever-growing
/// set — so they no longer get one enum variant each. `Omc`/`Ecc` merge into
/// one `Official` variant carrying an `official_library` sub-tag instead.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HubSource {
    /// 官方选型预置——BW 自己持续挑选、引入的高分精品库。`official_library`
    /// 标具体是哪个:写作日真实取值 "ecc" / "mattpocock-skills" /
    /// "superpowers";"omc" 是旧库迁移标签,暂无实例。
    Official { official_library: String },
    /// 预留:后期用户自选引入官方集之外的插件,今天无入口(plan/12 §6/§9)。
    Adopted,
    /// 自建
    SelfBuilt,
    /// 会话内
    WithinSession,
}

impl HubSource {
    pub fn label(&self) -> &'static str {
        match self {
            HubSource::Official { .. } => "官方选型",
            HubSource::Adopted => "选型引入",
            HubSource::SelfBuilt => "自建",
            HubSource::WithinSession => "会话内",
        }
    }

    /// Fixed chip-display order for the hub source filter row — every
    /// category counted even at 0 rows, so a chip never silently disappears
    /// just because nothing has that source yet. `Adopted` is deliberately
    /// left off (no UI entry produces it yet — plan/12 §9), unchanged from
    /// this list's pre-T1 shape (which also never surfaced a `选型引入` chip).
    pub const FILTER_CHIP_LABELS: [&'static str; 3] = ["官方选型", "自建", "会话内"];
}

/// Hand-written: a pre-T1 database's `workflow_spec.kind_json` blobs may
/// still hold the old bare-string `"omc"`/`"ecc"` unit-variant encoding.
/// `Official` now carries data, so the derived `Deserialize` these two
/// legacy strings used to satisfy no longer exists — without this impl,
/// opening an old row would hard-fail instead of "老库打开不崩" (T1
/// acceptance criterion). `self_built`/`within_session`/`adopted` keep
/// their original unit-variant wire shape untouched, so they round-trip
/// through ordinary derive-equivalent matching below; only `omc`/`ecc` need
/// an explicit legacy mapping.
impl<'de> Deserialize<'de> for HubSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "snake_case")]
        enum OnDisk {
            Official {
                official_library: String,
            },
            Adopted,
            SelfBuilt,
            WithinSession,
            /// Legacy pre-T1 rows (deleted directory-only OMC/ECC seeds).
            Omc,
            Ecc,
        }
        Ok(match OnDisk::deserialize(deserializer)? {
            OnDisk::Official { official_library } => HubSource::Official { official_library },
            OnDisk::Adopted => HubSource::Adopted,
            OnDisk::SelfBuilt => HubSource::SelfBuilt,
            OnDisk::WithinSession => HubSource::WithinSession,
            OnDisk::Omc => HubSource::Official {
                official_library: "omc".to_string(),
            },
            OnDisk::Ecc => HubSource::Official {
                official_library: "ecc".to_string(),
            },
        })
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoopConfig {
    pub retries: u8,
    pub max_iter: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentRef {
    pub name: String,
    pub def: String,
    pub from: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkillRef {
    pub name: String,
    pub def: String,
    pub from: String,
}

/// T8 (plan/12 §4): a phase's real role in the workflow's generator/evaluator
/// loop — what `workflow_flow.rs` used to *guess* from the phase's Chinese
/// name via a keyword heuristic. `Neutral` is the honest default for any
/// phase that isn't a generator/evaluator/optimizer (and for every
/// legacy/user-authored phase that never declared a role at all).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseRole {
    /// Produces the deliverable this phase is responsible for.
    Generator,
    /// A judging/review gate — the only role `reject_to_phase` is meaningful
    /// on.
    Evaluator,
    /// Refines/prunes an existing deliverable without adding new scope.
    Optimizer,
    #[default]
    Neutral,
}

/// One phase in a [`WorkflowSpec`]'s pipeline — structured (plan/12 §4)
/// replacement for the old bare phase name. `role` is real, declared data
/// (built-in stage playbooks in `crate::playbook`; `Neutral` for everything
/// user-authored today, since the create/edit UI doesn't yet expose role
/// editing — that's follow-up UI work, not this ticket).
///
/// `reject_to_phase` is only meaningful when `role == Evaluator`:
/// - `Some(i)` — a **Static** workflow's author fixed the reject target at
///   design time; `i` is a 0-based index into the same `WorkflowSpec.phases`
///   vector this `PhaseMeta` lives in (so a renderer can index straight into
///   it with no off-by-one translation).
/// - `None` — either this phase isn't a reject gate, or (for a **Dynamic**
///   workflow) the target is deliberately left to the evaluator agent's real
///   runtime verdict — see `PhaseOutcome` in plan/12 §4, built in T9, not
///   here.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PhaseMeta {
    pub name: String,
    #[serde(default)]
    pub role: PhaseRole,
    #[serde(default)]
    pub reject_to_phase: Option<u8>,
}

impl PhaseMeta {
    /// A plain, role-less phase — what every user-authored/edited phase
    /// (create/edit form, still name-only text) and every ad-hoc `Dynamic`
    /// spec produces today. Real role declarations exist only for the
    /// built-in stage playbooks (`crate::playbook::phase_metas`).
    pub fn neutral(name: impl Into<String>) -> Self {
        PhaseMeta {
            name: name.into(),
            role: PhaseRole::Neutral,
            reject_to_phase: None,
        }
    }
}

/// Hand-written (mirrors `HubSource`'s legacy-compat impl just above in this
/// file): a pre-T8 `workflow_spec.phases`/`workflow_version.phases` column
/// holds a plain JSON string array (`["阶段A","阶段B"]`) — every phase ever
/// created before this ticket. Each element deserializes as *either* a bare
/// string (legacy ⇒ `role: Neutral, reject_to_phase: None`) *or* a full
/// object (current shape) — per-element, not per-column, so a partially
/// migrated array (should one ever exist) still reads honestly. Old DBs must
/// not crash on open (repo-wide serde-compat rule).
impl<'de> Deserialize<'de> for PhaseMeta {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum OnDisk {
            Legacy(String),
            Full {
                name: String,
                #[serde(default)]
                role: PhaseRole,
                #[serde(default)]
                reject_to_phase: Option<u8>,
            },
        }
        Ok(match OnDisk::deserialize(deserializer)? {
            OnDisk::Legacy(name) => PhaseMeta::neutral(name),
            OnDisk::Full {
                name,
                role,
                reject_to_phase,
            } => PhaseMeta {
                name,
                role,
                reject_to_phase,
            },
        })
    }
}

/// T9 (plan/12 §4): the real runtime verdict an **Evaluator** phase renders on
/// the work it just reviewed — parsed from the phase's actual output text, never
/// machine-guessed. This is the runtime companion to the design-time
/// [`PhaseMeta`]/[`PhaseRole`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The reviewed work passes the gate — the workflow proceeds.
    Pass,
    /// The reviewed work is rejected; the `u8` is the evaluator's **proposed**
    /// 0-based target phase to restart from. A **Dynamic** workflow honours this
    /// proposal; a **Static** one ignores it and uses the phase's declared
    /// [`PhaseMeta::reject_to_phase`] instead (plan/12 §4).
    RejectToPhase(u8),
}

/// The structured decision block an Evaluator phase must emit (verdict + a
/// human-readable reason). Produced by [`parse_phase_outcome`] from real output.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PhaseOutcome {
    pub verdict: Verdict,
    pub reason: String,
}

/// The machine-parseable verdict contract appended to every Evaluator phase's
/// prompt (by `bw_engine`). It tells a real executor exactly how to render its
/// decision so [`parse_phase_outcome`] can read it back. Kept next to the parser
/// so the emit-side format and the parse-side format can never drift apart.
pub fn verdict_contract_suffix() -> &'static str {
    "\n\n────────────\n【评审裁决 · 机器解析(必须严格执行)】\n\
     完成本次评审后,在输出的最末尾单独成行给出结构化裁决,二选一:\n\
     • 通过 —— 输出一行:\n\
     VERDICT: PASS\n\
     • 打回 —— 输出两行(REJECT_TO_PHASE 后接要打回到的阶段的 0 基索引):\n\
     VERDICT: REJECT_TO_PHASE=<阶段索引>\n\
     REASON: <一句话说明打回原因>\n\
     解析只认最后一次出现的 VERDICT / REASON 行。缺少可解析的 VERDICT 行会被判为\
     评审失败(绝不会被当作通过)。\n"
}

/// Parse an Evaluator phase's real output for its structured [`PhaseOutcome`].
///
/// Robust to LLM chatter: scans every line and keeps the **last** line whose
/// trimmed, case-insensitive form starts with `VERDICT:` (and likewise the last
/// `REASON:` line) — so an evaluator that quotes the contract mid-output, then
/// renders its true verdict at the end, still reads correctly.
///
/// Accepted forms (case-insensitive on the marker and the `PASS`/`REJECT`
/// token): `VERDICT: PASS`; `VERDICT: REJECT_TO_PHASE=2` (also `= 2`, `:2`, or a
/// bare trailing number). Returns **`None`** when there is no well-formed
/// `VERDICT:` line, or a reject verdict carries no parseable target — an honest
/// parse failure the caller MUST treat as a failed review, never a default pass
/// (plan/12 §4, T9).
pub fn parse_phase_outcome(text: &str) -> Option<PhaseOutcome> {
    let mut verdict_value: Option<String> = None;
    let mut reason: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        let upper = line.to_ascii_uppercase();
        if upper.starts_with("VERDICT:") {
            // Slice the ORIGINAL line after its first ':' so casing/spacing of
            // the value is preserved for reason/target extraction.
            if let Some(colon) = line.find(':') {
                verdict_value = Some(line[colon + 1..].trim().to_string());
            }
        } else if upper.starts_with("REASON:") {
            if let Some(colon) = line.find(':') {
                reason = Some(line[colon + 1..].trim().to_string());
            }
        }
    }
    let value = verdict_value?;
    let value_upper = value.to_ascii_uppercase();
    if value_upper == "PASS" {
        return Some(PhaseOutcome {
            verdict: Verdict::Pass,
            reason: reason.unwrap_or_default(),
        });
    }
    if value_upper.starts_with("REJECT") {
        // Extract the target index: the first contiguous run of ASCII digits in
        // the value (`REJECT_TO_PHASE=2` → `2`). No digits ⇒ un-actionable
        // reject ⇒ honest parse failure (never guess a target).
        let digits: String = value.chars().filter(|c| c.is_ascii_digit()).collect();
        let target = digits.parse::<u8>().ok()?;
        return Some(PhaseOutcome {
            verdict: Verdict::RejectToPhase(target),
            reason: reason.unwrap_or_default(),
        });
    }
    // A VERDICT line with an unrecognised token is not a pass — fail honestly.
    None
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
    /// T8 (plan/12 §4): structured per-phase metadata (name + real role +
    /// static reject target) — `Vec<String>` before this ticket. serde-compat
    /// (see `PhaseMeta`'s `Deserialize` impl) reads old plain-string-array
    /// rows in as `role: Neutral`, so an already-seeded DB never crashes.
    pub phases: Vec<PhaseMeta>,
    /// Per-phase real instructions, index-aligned with `phases`. Empty (the
    /// pre-playbook default) or a missing/blank entry ⇒ that phase falls back
    /// to the shared `prompt` — byte-for-byte the old behavior. Rendered by
    /// `crate::playbook` for stage workflows; hand-authorable for custom ones.
    #[serde(default)]
    pub phase_prompts: Vec<String>,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub loop_config: LoopConfig,
    /// `None` = 全局/共享(built-in 阶段模板、Hub 目录条目);`Some` = 这个
    /// 项目自建的 workflow(plan/10 K1 项目侧边栏按这个字段过滤)。
    #[serde(default)]
    pub project_id: Option<ProjectId>,
}

/// Outcome of one workflow execution — the data a later "should this workflow
/// be optimized?" decision is built on. Persisted append-only (a run is never
/// mutated once it settles); the only transition is `Running → {Ok|Failed}`
/// when the engine returns.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    /// Engine is still executing (not yet persisted as a settled row in the
    /// common path — kept so an in-memory view can show a live run).
    Running,
    /// Engine returned `Ok` — every phase completed.
    Ok,
    /// Engine returned an error; `error` carries the message.
    Failed,
}

impl RunStatus {
    pub fn text(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::Ok => "ok",
            RunStatus::Failed => "failed",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "ok" => RunStatus::Ok,
            "failed" => RunStatus::Failed,
            _ => RunStatus::Running,
        }
    }
    /// `true` only for a settled-successful run — the basis of a "healthy
    /// workflow" signal later (iter 11).
    pub fn is_ok(self) -> bool {
        matches!(self, RunStatus::Ok)
    }
}

/// What triggered a run — distinguishes a user's manual fire from the
/// background scheduler's unattended auto-fire, so analytics (iter 2) can
/// attribute outcomes to the right source.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger {
    Manual,
    Scheduled,
}

impl RunTrigger {
    pub fn text(self) -> &'static str {
        match self {
            RunTrigger::Manual => "manual",
            RunTrigger::Scheduled => "scheduled",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "scheduled" => RunTrigger::Scheduled,
            _ => RunTrigger::Manual,
        }
    }
}

/// One execution record of a workflow. Append-only once settled (`status !=
/// Running`). `duration_ms` is the real wall-clock the engine took — the
/// primary cost/health input for optimization. `params_json` is left for
/// iter 3 (parameter capture) to fill; empty string until then.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: WorkflowRunId,
    pub workflow_id: WorkflowId,
    pub workflow_name: String,
    pub project_id: Option<ProjectId>,
    pub session_id: Option<SessionId>,
    pub trigger: RunTrigger,
    pub status: RunStatus,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    /// Real elapsed milliseconds (`finished_at - started_at`). `None` while
    /// running or if the clock was unavailable.
    pub duration_ms: Option<i64>,
    /// Phases that completed before the run settled (count) — a partial run
    /// that failed at phase 2 of 5 records `2` here, not a silent hole.
    pub phases_completed: u32,
    pub error: String,
    pub params_json: String,
    /// The cron task that fired this run (iter 4). `None` for manual runs.
    pub cron_task_id: Option<CronTaskId>,
    /// A2: the Issue this run executes — set only when the run is fired by
    /// `RunIssue` (`None` for ordinary workflow / scheduler runs). Lets an
    /// Issue's detail answer "which runs did this issue produce, and what?".
    pub issue_id: Option<IssueId>,
    /// P4: workspace HEAD when the run started / settled. `None` when the
    /// project has no real workspace (Mock runs touch no files). The pair is
    /// recorded fact — "这次运行改了什么" is answered by diffing between them,
    /// never by re-guessing after the tree has moved on.
    pub head_before: Option<String>,
    pub head_after: Option<String>,
}

/// P4: one run's resolved change list — `(run id, Ok(per-file (path, +added,
/// -deleted)) | Err(为何不可用的诚实原因))`. The shared shape between app
/// state (assembled at detail-open time) and the view layer.
pub type RunChanges = (WorkflowRunId, Result<Vec<(String, u32, u32)>, String>);

/// Per-workflow aggregate over its run history — the read-side shape optimization
/// intelligence consumes. Every field is derived from settled `workflow_run`
/// rows; a workflow with no runs returns `success_rate = None` (not 0 —
/// "unknown" must not masquerade as "always fails", mirroring `Signal::Unknown`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowRunAnalytics {
    pub workflow_id: WorkflowId,
    pub workflow_name: String,
    /// Total rows ever recorded (running + ok + failed).
    pub total_runs: u32,
    pub ok_runs: u32,
    pub failed_runs: u32,
    pub running_runs: u32,
    /// `ok_runs / settled_runs`. `None` when no run has settled yet — "no
    /// evidence", not "0%". The single most important optimization input.
    pub success_rate: Option<f32>,
    /// Mean `duration_ms` over settled runs. `None` if none settled.
    pub avg_duration_ms: Option<i64>,
    /// Median `duration_ms` over settled runs — robust to one slow outlier,
    /// a better "typical cost" than the mean for optimization decisions.
    pub median_duration_ms: Option<i64>,
    /// Unix seconds of the most recent run (any status), if any.
    pub last_run_at: Option<i64>,
    pub last_status: Option<RunStatus>,
}

/// Effectiveness of one cron schedule (iter 4): of the times this task's
/// target auto-fired, how many succeeded? The answer to "is this schedule
/// actually doing anything useful, or just burning runs?" — the gating input
/// for cadence auto-tune (iter 10) and the self-improving loop (iter 18).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CronEffectiveness {
    pub cron_task_id: CronTaskId,
    /// Scheduled fires attributed to this task (manual runs of the same
    /// workflow are excluded — this is purely the schedule's track record).
    pub fires: u32,
    pub ok_fires: u32,
    pub failed_fires: u32,
    /// `ok_fires / fires`. `None` when the task has never fired — "no
    /// evidence", mirroring `success_rate`.
    pub effectiveness: Option<f32>,
    /// Mean scheduled-run duration — the schedule's typical cost.
    pub avg_duration_ms: Option<i64>,
    pub last_fire_at: Option<i64>,
    pub last_fire_ok: Option<bool>,
}

/// One frozen version of a Static workflow's content (iter 5) — snapshotted
/// the instant before `UpdateWorkflowSpec` overwrites it. Together the series
/// is the spec's evolution: what changed, when, and (via `note`) why.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorkflowVersion {
    pub id: WorkflowRunId,
    pub workflow_id: WorkflowId,
    /// The `Static.version` this snapshot was taken at (pre-update).
    pub version: u32,
    pub name: String,
    pub prompt: String,
    pub goal: String,
    /// T8: structured (see `WorkflowSpec.phases`); same serde-compat with
    /// pre-T8 plain-string-array snapshots.
    pub phases: Vec<PhaseMeta>,
    /// Per-phase instructions frozen with the rest of the content — an
    /// evolution history that dropped them would misreport what old versions
    /// actually executed. Empty for pre-playbook snapshots.
    #[serde(default)]
    pub phase_prompts: Vec<String>,
    pub agents: Vec<AgentRef>,
    pub skills: Vec<SkillRef>,
    pub loop_retries: u8,
    pub loop_max_iter: u8,
    /// Caller's reason for the change that replaced this version (the "优化"
    /// note). `''` when none was given.
    pub note: String,
    pub created_at: i64,
}

/// One workflow's position in the global usage ranking (iter 6) — the
/// answer to "which workflows are actually earning their keep?" The hottest
/// (most-run) sit at the top; the coldest (never or rarely run) at the
/// bottom. A workflow that's in the hub but has **zero** runs is `cold =
/// true` — the prime "should this even exist / be optimized or retired?"
/// candidate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UsageRank {
    pub workflow_id: WorkflowId,
    pub workflow_name: String,
    pub stage_ref: Option<u8>,
    pub total_runs: u32,
    pub ok_runs: u32,
    pub failed_runs: u32,
    pub success_rate: Option<f32>,
    pub last_run_at: Option<i64>,
    /// `true` when `total_runs == 0` — never run since landing in the hub.
    pub cold: bool,
}

/// Shared by `stage_workflow` and `stage_template_workflow` — both are the
/// same methodology projected into a `WorkflowSpec.goal`, just with
/// different `kind` (Dynamic vs Static). `idgen`-gated like both callers:
/// with the feature off (wasm32 keepalive build) neither caller exists, so
/// this would otherwise be dead code.
#[cfg(feature = "idgen")]
fn stage_goal(kind: StageKind) -> String {
    format!(
        "{} → {}",
        kind.core_question(),
        kind.dod_items().first().copied().unwrap_or("交棒条件达成")
    )
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
    WorkflowSpec {
        id: WorkflowId::new(),
        name: format!("「{}」标准工作流", kind.label()),
        kind: WorkflowKind::Dynamic {
            origin: "阶段标准模板".into(),
            stage: kind.label().into(),
        },
        prompt: kind.method_loop().join(" → "),
        goal: stage_goal(kind),
        stage_ref: Some(kind.index()),
        // Dynamic ⇒ any Evaluator's reject target is honestly left `None`
        // (plan/12 §4: runtime evaluator decision, T9's job) — the same
        // roles as the Static template, just with the fixed target cleared.
        phases: crate::playbook::phase_metas_dynamic(kind),
        phase_prompts: vec![],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
        project_id: None,
    }
}

/// [`stage_workflow`] upgraded by the stage's executable playbook
/// (`crate::playbook`): same method-loop phases, but each phase carries a
/// real, project-contextualized instruction a real executor can act on. The
/// role that hosts the stage rides along as the spec's (real) `AgentRef` —
/// this is what actually executes, not a display-only crew suggestion.
#[cfg(feature = "idgen")]
pub fn stage_workflow_with_playbook(
    kind: StageKind,
    ctx: &crate::playbook::PlaybookCtx,
) -> WorkflowSpec {
    let mut spec = stage_workflow(kind);
    spec.name = format!("「{}」剧本工作流 · {}", kind.label(), kind.role_short());
    spec.prompt = crate::playbook::stage_prompt(kind, ctx);
    spec.phase_prompts = crate::playbook::rendered_phase_prompts(kind, ctx);
    spec.agents = vec![AgentRef {
        name: kind.role_short().to_string(),
        def: format!("{} · {}", kind.methodology(), kind.seek()),
        from: "阶段剧本(bw-core::playbook)".into(),
    }];
    // The stage's working-method skills ride along as real refs: their
    // *content* is already injected into every phase prompt by
    // `rendered_phase_prompts`, and the ref names let the run accounting
    // credit the Skill Hub rows that carry the same content.
    spec.skills = crate::playbook::stage_skills(kind)
        .iter()
        .map(|s| SkillRef {
            name: s.name.to_string(),
            def: s.def.to_string(),
            from: "阶段剧本(bw-core::playbook)".into(),
        })
        .collect();
    // A playbook phase is a full, self-contained work order: the executor
    // reports `done` on its first attempt, so the engine's *per-phase* inner
    // loop always runs exactly once — no blind re-run of an identical prompt
    // (real spend), regardless of `max_iter`. T9: `max_iter` now also caps the
    // *adversarial* review loop (Evaluator打回 → 重跑 → 重审); 1 would disable it
    // outright, so the playbook path allows up to 3 review rounds before the
    // Issue honestly parks in Blocked (Done 仍永不自动).
    spec.loop_config = LoopConfig {
        retries: 1,
        max_iter: 3,
    };
    spec
}

/// The persisted, browsable counterpart to [`stage_workflow`] — a **Static**
/// (自建 · Mature) Hub entry carrying the identical methodology, so each of
/// the five stages has one standing, importable template in WorkflowHub
/// instead of only the ephemeral spec a session constructs and discards.
/// Seeded once at boot (`bw_store::seed::seed_hub_if_empty`); `stage_workflow`
/// remains the throwaway variant the creation flow / direct "▶ 运行" path
/// builds fresh every time (running *this* template's hub row goes through
/// `RunHubWorkflow`, which looks the persisted spec back up by id).
#[cfg(feature = "idgen")]
pub fn stage_template_workflow(kind: StageKind) -> WorkflowSpec {
    let slug = match kind {
        StageKind::Prototype => "prototype",
        StageKind::Build => "build",
        StageKind::Optimize => "optimize",
        StageKind::Growth => "growth",
        StageKind::Ops => "ops",
    };
    WorkflowSpec {
        id: WorkflowId::new(),
        name: format!("「{}」标准工作流 · {}", kind.label(), kind.role_short()),
        kind: WorkflowKind::Static {
            maturity: Maturity::Mature,
            version: 1,
            uses: 0,
            scope: "全项目通用 · 阶段标准模板".into(),
            source: HubSource::SelfBuilt,
            trigger: Some(format!("/stage-{slug}")),
        },
        prompt: kind.method_loop().join(" → "),
        goal: stage_goal(kind),
        stage_ref: Some(kind.index()),
        // Static ⇒ real role + fixed reject target for the stage's
        // review-gate phase (plan/12 §4; declared per-stage in
        // `crate::playbook::phase_metas`, not machine-guessed).
        phases: crate::playbook::phase_metas(kind),
        phase_prompts: vec![],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 3,
        },
        project_id: None,
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
            PhaseMeta::neutral("周期判定"),
            PhaseMeta::neutral("北极星起草"),
            PhaseMeta::neutral("指标框架"),
            PhaseMeta::neutral("阶段激活"),
        ],
        phase_prompts: vec![],
        agents: vec![],
        skills: vec![],
        loop_config: LoopConfig {
            retries: 1,
            max_iter: 1,
        },
        project_id: None,
    }
}

// ─────────────────────────── skill / agent hub ───────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkillCard {
    pub id: SkillId,
    pub name: String,
    /// 2-tier in practice (成熟/打磨中) — a freshly created skill defaults to
    /// `Polishing`, never `Fresh` (see bw-app's `CreateSkill`).
    pub maturity: Maturity,
    pub desc: String,
    pub category: String,
    /// T7 (2026-07-23, plan/12 §0/§2): which of the five stage roles this
    /// skill belongs to — the same classification dimension `WorkflowSpec`
    /// already carries (its `stage_ref: Option<u8>`, 1..=5). Here the domain
    /// type is `Option<StageKind>` directly (the ticket's own alignment
    /// call) rather than the bare `u8` `WorkflowSpec` was left with — that
    /// field predates this ticket and stays untouched (T8/T9's workflow
    /// chain reads it), so storage-level interop goes through
    /// `StageKind::index`/`from_index`, not a shared Rust type. `None` =
    /// cross-stage/general — honest for every imported catalog skill (no
    /// one has manually classified them) and the default for a
    /// hand-authored one until edited.
    #[serde(default)]
    pub stage_ref: Option<StageKind>,
    /// T2 (2026-07-23, plan/12 §6): unified onto the same 4-tier
    /// [`HubSource`] Workflow already uses, replacing the former standalone
    /// `LibSource { Official, SelfBuilt }` — "which curated library this
    /// came from" is the same open-ended provenance question for every hub
    /// entity, not a Skill-specific vocabulary. `Official { official_library
    /// }` is populated by `ImportSkillPackage`/`ImportSkillLibrary`; bare
    /// pre-T2 `official` rows with no library sub-tag (the 5 built-in
    /// stage-methodology skills) read back as `SelfBuilt` — see
    /// `bw_store::parse_skill_source`'s doc comment for why.
    pub source: HubSource,
    /// T11 (2026-07-23, plan/12 §7): "改编自 <库名>" provenance — set iff this
    /// row was once `Official { official_library }` and a substantive edit
    /// (`content`/`desc`/`category`) flipped `source` to `SelfBuilt` (T11's
    /// "编辑即脱离源头"). The store deliberately leaves the raw
    /// `official_library` column untouched when it flips `source` away from
    /// `official` — this field is that surviving value read back, `None`
    /// whenever the column is empty (never edited away from an official
    /// origin) or the row is still `Official` itself (its library already
    /// shows up in `source`, no need to duplicate it here). Also doubles as
    /// the re-import dedup signal: `ImportSkillLibrary` skips a name match
    /// on this field the same as a live `Official { official_library }`
    /// match, so a flipped row is never silently re-created as a duplicate.
    #[serde(default)]
    pub adapted_from: Option<String>,
    pub uses: u32,
    /// The skill body — real instructions an executor can act on. Empty for
    /// catalog *references* (OMC/ECC entries whose full text lives in the
    /// source repo); non-empty means this row is executable content that
    /// really gets injected into prompts (stage skills, self-authored ones).
    #[serde(default)]
    pub content: String,
    /// The completed Issue this skill was distilled from, if any. `None` for
    /// catalog/seeded skills — only a `DistillSkillFromIssue` sets it. This is
    /// BW's "skills compound from real work" link (multica's skills are manual;
    /// we attribute them to the real issue + agent that produced them).
    #[serde(default)]
    pub distilled_from_issue: Option<IssueId>,
    /// The agent teammate that did the work behind `distilled_from_issue`.
    /// `None` iff `distilled_from_issue` is `None`.
    #[serde(default)]
    pub origin_agent: Option<AgentId>,
    /// `None` = 全局/共享;`Some` = 这个项目自建(或从其项目 Issue 蒸馏)的
    /// 技能(plan/10 K1 项目侧边栏按这个字段过滤)。
    #[serde(default)]
    pub project_id: Option<ProjectId>,
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
    /// T7 (2026-07-23, plan/12 §0/§3): same classification dimension as
    /// `SkillCard.stage_ref` — see that field's doc comment for the
    /// `Option<StageKind>`-vs-`WorkflowSpec`'s-`Option<u8>` alignment call.
    /// `None` = cross-stage/general (every imported ECC agent, honestly
    /// unclassified); `Some` for the five built-in stage-role agents.
    #[serde(default)]
    pub stage_ref: Option<StageKind>,
    pub maturity: Maturity,
    pub skills: Vec<AgentSkillTag>,
    pub model: String,
    /// Real settled runs credited to this agent (`record_agent_run_by_name`).
    pub runs: u32,
    /// Success rate over credited runs as a pre-formatted display string
    /// (e.g. `"94%"`), recomputed from real `runs`/`wins` on every credit —
    /// `""` while `runs == 0` ("no evidence", never "0%").
    pub win_rate: String,
    /// The agent's standing instructions (system-prompt tier). Empty for
    /// catalog references; the five stage-role agents carry their real
    /// `bw_core::playbook::role_preamble` template here — honestly what the
    /// role gets told, `{var}` slots filled per project at run time.
    #[serde(default)]
    pub instructions: String,
    /// T5 (2026-07-23, plan/12 §3): "Agent" == AGENT.md — this is that
    /// definition's `tools` frontmatter field, i.e. **AllowedTools**, the same
    /// vocabulary `claude` CLI's `--allowedTools` uses. Real at run time: the
    /// CLI adapter translates this list, not the field itself (decoupled —
    /// same reasoning as `agent_cli` below). Empty for the five built-in
    /// stage-role agents (no restriction declared, honest) and for a
    /// hand-authored `CreateAgent` row until edited.
    #[serde(default)]
    pub tools: Vec<String>,
    /// T5 (2026-07-23, plan/12 §3): which Agent CLI executes this agent
    /// ("claude-code" / "codex" / "cursor" / …). First version: only
    /// `"claude-code"` has a real executor behind it (`bw-engine`'s
    /// `ClaudeCliExecutor`); any other value is an honest label with no route
    /// yet — selecting one must error "本机未安装 X CLI", never silently fall
    /// back to Claude Code (real routing lands in T6).
    #[serde(default)]
    pub agent_cli: String,
    /// T5 (2026-07-23, plan/12 §6/§8): provenance — the same [`HubSource`]
    /// Skill/Workflow already carry. The five built-in stage-role agents (and
    /// any pre-T5 row opened from an old DB with no `source` column) read back
    /// as `SelfBuilt` (see the `agent` table's `add_column_if_missing`
    /// default); `ImportAgentDefinition`'s 67 ECC rows are
    /// `Official { official_library: "ecc" }`.
    pub source: HubSource,
    /// T11 (2026-07-23, plan/12 §7): same "改编自 <库名>" provenance-survives-
    /// the-flip field `SkillCard` carries — see its doc comment for the full
    /// reasoning (edit flips `source` away from `Official`, the raw
    /// `official_library` column stays, this is that surviving value; also
    /// the re-import dedup signal `ImportAgentDefinition` checks).
    #[serde(default)]
    pub adapted_from: Option<String>,
    /// `None` = 全局/共享(五角色内置 agent);`Some` = 这个项目自建的
    /// 专精 agent(plan/10 K1 项目侧边栏按这个字段过滤)。
    #[serde(default)]
    pub project_id: Option<ProjectId>,
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

/// What a [`CronTask`] does when due (A1; extended T10, plan/12 §5).
/// `RunWorkflow` (the default) resolves `target` as a hub workflow and runs
/// it — the original behavior. `RunSkill`/`RunPrompt` (T10) also really
/// execute (through the same engine/executor path `RunWorkflow` uses), just
/// against a single ad-hoc prompt instead of a full hub workflow's phases:
/// `RunSkill` takes its prompt from a real Skill's `content` (a genuine
/// `SkillId` reference — never free-text name matching, so a deleted/renamed
/// skill can never silently resolve to the wrong row); `RunPrompt` runs a bare
/// prompt with no entity involved at all. `CreateIssue` is autopilot: it
/// mints a stage-scoped Issue. No-hijack by construction: a `CreateIssue` task
/// never auto-runs anything, it only creates work — unaffected by T10.
///
/// Wire note: `CronMode` itself is never serialized as JSON on disk — the
/// `cron_task.mode`/`cron_task.target` TEXT columns already round-trip it by
/// hand (`bw_store::cron_mode_text`/`parse_cron_mode`), so the two new
/// variants need no schema migration: `target` (already free text, already
/// unused by `CreateIssue`) doubles as the payload column — a skill's real
/// id (text) for `RunSkill`, the raw prompt text for `RunPrompt`. The two
/// pre-T10 modes' storage is untouched.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CronMode {
    #[default]
    RunWorkflow,
    /// T10: run a real Skill's `content` as the prompt when due.
    RunSkill {
        skill_id: SkillId,
    },
    /// T10: run a bare prompt when due — no entity involved.
    RunPrompt {
        prompt: String,
    },
    CreateIssue,
}

impl CronMode {
    /// L1(plan/11); extended T10. Cron 详情卡要如实标出「到点做什么」。
    pub fn label(&self) -> &'static str {
        match self {
            CronMode::RunWorkflow => "运行工作流",
            CronMode::RunSkill { .. } => "运行技能",
            CronMode::RunPrompt { .. } => "运行 Prompt",
            CronMode::CreateIssue => "建活(autopilot · 不自动跑)",
        }
    }

    /// T10(plan/12 §5): the row-front icon that lets CronHub's list tell the
    /// four modes apart at a glance. `CreateIssue` deliberately keeps the
    /// pre-T10 "no icon" look (the issue asked to leave it 沿用现状) — its
    /// distinctiveness already comes from `label()`'s explicit "autopilot"
    /// text, not an icon.
    pub fn icon(&self) -> &'static str {
        match self {
            CronMode::RunWorkflow => "🔄",
            CronMode::RunSkill { .. } => "⚙",
            CronMode::RunPrompt { .. } => "💬",
            CronMode::CreateIssue => "",
        }
    }
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
    /// something outside this app entirely. T10 (plan/12 §5): also doubles as
    /// the payload column for the two new [`CronMode`] variants — a
    /// `RunSkill` task stores its referenced `SkillId` here (as text), a
    /// `RunPrompt` task stores its raw prompt text here. `mode` is always the
    /// typed, authoritative read of "what this really is"; `target` is the
    /// storage-level string both `mode` and this field derive from (see
    /// `bw_store::parse_cron_mode`) — unused (empty) for `CreateIssue`, as
    /// before.
    pub target: String,
    pub schedule: Cadence,
    /// `None` = 全部项目 (all projects), matching the prototype's own
    /// "全部项目" catch-all option.
    pub project_id: Option<ProjectId>,
    pub status: CronStatus,
    pub last_run: String,
    pub next_run: String,
    /// Real clock, `None` = never run. Separate from the pre-formatted
    /// `last_run` display string — this is what `cron_due` compares against,
    /// never a parsed-back label.
    pub last_run_at: Option<OffsetDateTime>,
    /// A1: what this task does when due. `RunWorkflow` (default) runs `target`;
    /// `CreateIssue` mints a stage-scoped Issue (autopilot, no-hijack);
    /// `RunSkill`/`RunPrompt` (T10) really execute too — see `CronMode`'s doc.
    #[serde(default)]
    pub mode: CronMode,
    /// A1: the stage a `CreateIssue` task scopes its Issue to (`None` for
    /// `RunWorkflow` tasks).
    #[serde(default)]
    pub issue_stage: Option<StageKind>,
    /// A1: agent NAME a `CreateIssue` task assigns its Issue to (`None` =
    /// unassigned). Name-led, matching the by-name accounting convention.
    #[serde(default)]
    pub issue_assignee: Option<String>,
}

/// Is `task` due to auto-fire right now? Pure and independently unit-tested —
/// the same function `App::tick_scheduler` calls and this module's tests
/// call directly, so "why did/didn't this fire" is always answerable without
/// a running app.
///
/// - Never run (`last_run_at: None`) is due immediately — an honest "overdue
///   since creation", not a fabricated wait.
/// - `RealTime` is always due (fires every scheduler tick while `Normal`).
/// - `Daily`/`Weekly` compare real elapsed wall-clock time — no shortcuts.
/// - `Cron(_)` (raw cron expressions) has no parser built yet; returns
///   `false` rather than guessing — an honest "not supported yet", not a
///   silent wrong answer.
pub fn cron_due(
    schedule: &Cadence,
    last_run_at: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> bool {
    // Cadence::Cron(_) is checked first, ahead of the never-run shortcut —
    // "unsupported" must win over "overdue", or a never-run raw-cron task
    // would wrongly fire on its very first tick.
    if matches!(schedule, Cadence::Cron(_)) {
        return false;
    }
    let Some(last) = last_run_at else {
        return true;
    };
    match schedule {
        Cadence::RealTime => true,
        Cadence::Daily => now - last >= Duration::hours(24),
        Cadence::Weekly => now - last >= Duration::days(7),
        Cadence::Cron(_) => unreachable!("handled above"),
    }
}

/// Real, honest "next run" display text for `CronRowVm` — replaces what was
/// an always-empty `next_run` column (nothing ever wrote it) now that
/// `tick_scheduler` gives this a real answer to compute. Never a guess: a
/// paused task says so, an unsupported raw-cron expression says so, and a
/// task already due says "等待下次检查" (the next scheduler tick, at most a
/// few seconds away) rather than a fabricated clock time.
pub fn cron_next_run_label(
    schedule: &Cadence,
    last_run_at: Option<OffsetDateTime>,
    status: CronStatus,
    now: OffsetDateTime,
) -> String {
    if status == CronStatus::Paused {
        return "已暂停".into();
    }
    if matches!(schedule, Cadence::Cron(_)) {
        return "不支持自动触发(cron 表达式)".into();
    }
    if cron_due(schedule, last_run_at, now) {
        return "等待下次检查".into();
    }
    // Only reachable with Some(last) — cron_due returns true above whenever
    // last_run_at is None, for every non-Cron schedule.
    let last = last_run_at.expect("due()=false implies a real last_run_at for this schedule");
    let period = match schedule {
        Cadence::Daily => Duration::hours(24),
        Cadence::Weekly => Duration::days(7),
        Cadence::RealTime | Cadence::Cron(_) => unreachable!("handled above"),
    };
    let remaining = (last + period) - now;
    if remaining >= Duration::hours(24) {
        format!("约 {} 天后", remaining.whole_days())
    } else if remaining >= Duration::hours(1) {
        format!("约 {} 小时后", remaining.whole_hours())
    } else {
        format!("约 {} 分钟后", remaining.whole_minutes().max(1))
    }
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

/// The two connector kinds the workbench can *really* sync today — everything
/// else stays a free-text reference entry (recorded, listed, honestly marked
/// unsynced). Matching is by the `Connector.kind` string.
pub const CONNECTOR_KIND_GIT_REPO: &str = "git-repo";
pub const CONNECTOR_KIND_CLAUDE_CLI: &str = "claude-cli";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Connector {
    pub id: ConnectorId,
    pub name: String,
    /// Connector type. [`CONNECTOR_KIND_GIT_REPO`] and
    /// [`CONNECTOR_KIND_CLAUDE_CLI`] are *live* kinds a `SyncConnector`
    /// really probes; any other value is a free-text reference entry.
    pub kind: String,
    pub status: ConnectorStatus,
    pub last_sync: String,
    pub scope: String,
    /// The project this connector feeds, if project-bound (a `git-repo`
    /// connector always is; a `claude-cli` probe is global).
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    /// Kind-specific real configuration — for `git-repo` the workspace path;
    /// for `claude-cli` the binary override (empty = `claude` on PATH).
    #[serde(default)]
    pub config: String,
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

// ─────────────────────────── artifact ───────────────────────────

/// Coarse classification of a workspace file — derived from its path alone
/// (see [`classify_artifact_path`]), never asserted by hand.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Markdown/docs — what playbook phases write under `docs/`.
    Doc,
    /// Source code.
    Code,
    /// Test code (`tests/`, `*_test.*`).
    Test,
    /// Shell/automation scripts.
    Script,
    /// Manifests & config (`Cargo.toml`, `*.yaml`, …).
    Config,
    /// Everything else.
    Other,
}

impl ArtifactKind {
    pub fn label(self) -> &'static str {
        match self {
            ArtifactKind::Doc => "文档",
            ArtifactKind::Code => "代码",
            ArtifactKind::Test => "测试",
            ArtifactKind::Script => "脚本",
            ArtifactKind::Config => "配置",
            ArtifactKind::Other => "其他",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            ArtifactKind::Doc => "doc",
            ArtifactKind::Code => "code",
            ArtifactKind::Test => "test",
            ArtifactKind::Script => "script",
            ArtifactKind::Config => "config",
            ArtifactKind::Other => "other",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "doc" => ArtifactKind::Doc,
            "code" => ArtifactKind::Code,
            "test" => ArtifactKind::Test,
            "script" => ArtifactKind::Script,
            "config" => ArtifactKind::Config,
            _ => ArtifactKind::Other,
        }
    }
}

/// Classify a workspace-relative path. Pure string rules, order matters:
/// tests before code (a `tests/*.rs` file is a test, not generic code), docs
/// by extension anywhere (playbooks write `docs/*.md`, but a root `README.md`
/// is a doc too).
pub fn classify_artifact_path(path: &str) -> ArtifactKind {
    let p = path.trim().trim_start_matches("./");
    let lower = p.to_ascii_lowercase();
    let file = lower.rsplit('/').next().unwrap_or(&lower).to_string();
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_string());

    let is_code_ext = matches!(
        ext.as_deref(),
        Some("rs" | "ts" | "tsx" | "js" | "jsx" | "py" | "go" | "c" | "h" | "cpp" | "java")
    );
    if lower.starts_with("tests/") || lower.contains("/tests/") {
        // Only actual code under tests/ is a test; a tests/fixture.md is a doc.
        if is_code_ext {
            return ArtifactKind::Test;
        }
    }
    if is_code_ext
        && (file.ends_with("_test.rs")
            || file.ends_with(".test.ts")
            || file.ends_with(".test.js")
            || file.ends_with("_test.py"))
    {
        return ArtifactKind::Test;
    }
    if matches!(ext.as_deref(), Some("md" | "mdx" | "txt")) {
        return ArtifactKind::Doc;
    }
    if matches!(ext.as_deref(), Some("sh" | "bash" | "zsh")) || lower.starts_with("scripts/") {
        return ArtifactKind::Script;
    }
    if matches!(
        ext.as_deref(),
        Some("toml" | "yaml" | "yml" | "json" | "ini")
    ) || file == "makefile"
        || file == "dockerfile"
        || file == ".gitignore"
    {
        return ArtifactKind::Config;
    }
    if is_code_ext {
        return ArtifactKind::Code;
    }
    ArtifactKind::Other
}

/// One registered file version in a project's workspace — the real 产物.
/// Identity is `project × path × git_commit`: registering the same path again
/// at the same commit is a no-op; at a *new* commit it appends a new row, so
/// the rows sharing one `path` are that artifact's real version history
/// (nothing is ever edited in place). Always harvested from a real workspace
/// scan (`bw-engine::evidence`), never typed in.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Artifact {
    pub id: ArtifactId,
    pub project_id: ProjectId,
    /// The run that most plausibly produced this version — the run whose
    /// settle-time scan first saw it. `None` when registered by a manual
    /// collect outside any run.
    pub workflow_run_id: Option<WorkflowRunId>,
    /// A2: the Issue whose Done-edge scan first registered this version
    /// (`None` for run-settle scans and manual collects).
    pub issue_id: Option<IssueId>,
    /// Stage the project was operating when this version appeared, if known.
    pub stage_kind: Option<StageKind>,
    /// Workspace-relative path (git's own path form).
    pub path: String,
    pub kind: ArtifactKind,
    /// Real size in bytes at registration time.
    pub bytes: u64,
    /// Short HEAD hash the workspace was at when this version was seen.
    /// Empty when the repo had no commits yet.
    pub git_commit: String,
    pub registered_at: i64,
}

// ─────────────────────────── issue ───────────────────────────

/// Kanban lifecycle of an [`Issue`] — an assignable unit of work scoped to a
/// project's stage. The seven states are ordered as a lifecycle: an issue
/// advances left-to-right (Backlog → Todo → InProgress → InReview → Done),
/// but `Blocked` is a recoverable side-state (not terminal — the work resumes
/// once the blocker clears), and `Cancelled` is the other terminal alongside
/// `Done`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueStatus {
    Backlog,
    Todo,
    InProgress,
    InReview,
    Done,
    Blocked,
    Cancelled,
}

impl IssueStatus {
    /// All seven, in lifecycle order.
    pub const ALL: [IssueStatus; 7] = [
        IssueStatus::Backlog,
        IssueStatus::Todo,
        IssueStatus::InProgress,
        IssueStatus::InReview,
        IssueStatus::Done,
        IssueStatus::Blocked,
        IssueStatus::Cancelled,
    ];

    pub fn label(self) -> &'static str {
        match self {
            IssueStatus::Backlog => "待办池",
            IssueStatus::Todo => "待办",
            IssueStatus::InProgress => "进行中",
            IssueStatus::InReview => "评审中",
            IssueStatus::Done => "已完成",
            IssueStatus::Blocked => "阻塞",
            IssueStatus::Cancelled => "已取消",
        }
    }

    /// `true` only for `Done` and `Cancelled` — the two states no further work
    /// is expected from. `Blocked` is deliberately NOT terminal (the work
    /// resumes when the blocker clears; treating it as done would hide stuck
    /// work).
    pub fn is_terminal(self) -> bool {
        matches!(self, IssueStatus::Done | IssueStatus::Cancelled)
    }

    /// `true` only for `Backlog` — the "not yet committed to" pile.
    pub fn is_backlog(self) -> bool {
        matches!(self, IssueStatus::Backlog)
    }

    /// `true` iff `to` is a legal next state from `self` in the Issue
    /// lifecycle graph — the single source of truth for every transition
    /// guard (App-layer `TransitionIssue`/`BlockIssue`/`RunIssue` all query
    /// this, never invent their own edges). `Blocked` is graph-legal from
    /// `Todo`/`InProgress`/`InReview`, but is reached in practice only
    /// through the `BlockIssue` command (which requires a reason) — bare
    /// `TransitionIssue` rejects a `Blocked` target regardless of this table.
    /// No state transitions to itself; `Cancelled` and `Done`-via-non-`InReview`
    /// have no legal predecessor edge here beyond what's listed.
    pub fn can_transition_to(self, to: IssueStatus) -> bool {
        use IssueStatus::*;
        matches!(
            (self, to),
            (Backlog, Todo)
                | (Backlog, InProgress)
                | (Backlog, Cancelled)
                | (Todo, InProgress)
                | (Todo, Backlog)
                | (Todo, Blocked)
                | (Todo, Cancelled)
                | (InProgress, InReview)
                | (InProgress, Todo)
                | (InProgress, Blocked)
                | (InProgress, Cancelled)
                | (InReview, Done)
                | (InReview, InProgress)
                | (InReview, Blocked)
                | (InReview, Cancelled)
                | (Blocked, Todo)
                | (Blocked, InProgress)
                | (Blocked, Cancelled)
                | (Done, Todo)
                | (Done, InProgress)
        )
    }
}

/// How urgent an [`Issue`] is — drives ordering and visual emphasis. `None`
/// (the default for a freshly created issue) means "no priority assigned",
/// distinct from `Low` which is an explicit, deliberate low-urgency tag.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuePriority {
    None,
    Low,
    Medium,
    High,
    Urgent,
}

impl IssuePriority {
    pub fn label(self) -> &'static str {
        match self {
            IssuePriority::None => "无",
            IssuePriority::Low => "低",
            IssuePriority::Medium => "中",
            IssuePriority::High => "高",
            IssuePriority::Urgent => "紧急",
        }
    }
}

/// An assignable unit of work scoped to a project's stage — the multica
/// "assign a task to a teammate" model fused into BW's stage ring. `number`
/// is per-project (1, 2, 3, …), auto-assigned at creation. `assignee` is the
/// agent teammate the issue is currently delegated to (`None` = unassigned).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Issue {
    pub id: IssueId,
    pub project_id: ProjectId,
    pub stage: StageKind,
    pub number: u32,
    pub title: String,
    pub desc: String,
    pub status: IssueStatus,
    pub priority: IssuePriority,
    pub assignee: Option<AgentId>,
    /// Unix ts of the FIRST …→Done edge (when issue-side accounting fired).
    /// `None` = never settled. Reopen-and-redo does not settle again.
    #[serde(default)]
    pub settled_at: Option<i64>,
    /// Non-empty only while `status == Blocked`; set exclusively via the
    /// `BlockIssue` command and cleared on every other transition (nothing
    /// but `BlockIssue` can reach `Blocked`, so a plain `transition_issue`
    /// unconditionally clearing it on every other move is safe and correct).
    #[serde(default)]
    pub blocked_reason: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

// ─────────────────────────── project ───────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
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
pub enum MaturityPeriod {
    /// 探索期 · 0→1 · 未达 PMF
    Explore,
    /// 扩张期 · 1→N · 增长
    Expand,
    /// 成熟期 · Sustain
    Mature,
}

impl MaturityPeriod {
    pub fn label(self) -> &'static str {
        match self {
            MaturityPeriod::Explore => "探索期",
            MaturityPeriod::Expand => "扩张期",
            MaturityPeriod::Mature => "成熟期",
        }
    }

    pub fn sub_label(self) -> &'static str {
        match self {
            MaturityPeriod::Explore => "0→1 · 未达 PMF",
            MaturityPeriod::Expand => "1→N · 增长",
            MaturityPeriod::Mature => "Sustain · 原「运维」期",
        }
    }

    /// Percentage weight per [`StageKind::ALL`] stage, summing to 100.
    pub fn mix(self) -> [u8; 5] {
        match self {
            MaturityPeriod::Explore => [40, 30, 15, 10, 5],
            MaturityPeriod::Expand => [10, 25, 20, 30, 15],
            MaturityPeriod::Mature => [5, 10, 25, 25, 35],
        }
    }

    pub fn main_loop_label(self) -> &'static str {
        match self {
            MaturityPeriod::Explore => "主环 · 原型 ↔ 构建 来回",
            MaturityPeriod::Expand => "主环 · 构建 → 优化 → 推广",
            MaturityPeriod::Mature => "主环 · 优化 ↔ 运维 · 推广保温",
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
    pub phase: Readiness,
    pub cycle: MaturityPeriod,
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
