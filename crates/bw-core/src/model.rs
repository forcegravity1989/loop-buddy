//! The domain entity graph (plan `§2`), modelled so illegal states are
//! unrepresentable. Derived signals are never hand-written: only the derive
//! chain ([`crate::derive`]) produces a [`Derived<Signal>`], and persisted
//! caches are recomputed on load, never trusted as authority (plan `§2.5`:
//! "绝不把缓存当权威").

use crate::ids::{
    AgentId, ArtifactId, ConnectorId, ConversationId, CronTaskId, IssueId, KnowledgeSourceId,
    ProjectId, SessionId, SkillId, WorkflowId, WorkflowRunId,
};
use crate::stage_catalog::StageOrigin;
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
    /// C7 · 采集器: a value the standard GitHub collector pulled by running a
    /// real `gh` count query (issues/PRs) against the project's remote. A
    /// non-manual source, so it never wears the 手填 badge — the number is
    /// machine-collected and independently re-derivable from `gh`.
    Github,
    /// P5 · codehub 采集器: a value pulled by `codehub-cli issue|mr list
    /// --jq length` against the project's codehub remote. Same honesty as
    /// Github — machine-collected, no 手填 badge, independently re-derivable
    /// from codehub-cli.
    Codehub,
    /// plan18-③ · 项目侧自采脚本采集器: a value pulled by buddy shell-out
    /// 一个项目仓里既有的采集脚本(如 `derive_*.py` 机械解析真实数据源、产
    /// 出 `data.json`)、按指标的 `collect_query` 字段路径取回。非 manual——
    /// 是自动采集(脚本自身依赖由项目侧管,buddy 只调),不带手填徽,可从该脚
    /// 本独立重派生。
    Script,
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

/// plan/16: the `official_library` sub-tag of BW's own built-in standard
/// skill library (playbook 五 + 标配三) — the one `Official` library that is
/// BW 自产, not an external import. One const, not a string literal scattered
/// per call site.
pub const BW_STANDARD_LIBRARY: &str = "bw-standard";

/// plan/渠道6: the `official_library` sub-tag for skills/agents scanned in
/// from a project's own workspace (`skills/<slug>/SKILL.md`,
/// `agents/<name>.md`) — the project's own runtime/maintenance assets, not a
/// BW-authored library and not an external curated import. Scoped to the
/// project by `project_id`; registered-visible only (种A: 不进任何注入下拉
/// — issue standard_skill / issue assignee / workflow crew).
/// Like any `Official` library other than `bw-standard`, it counts as
/// `is_external_official()` → plan/16 spec findings degrade to Advisory
/// (project's own text, honestly shown, never rewritten in place).
pub const BW_PROJECT_ASSETS_LIBRARY: &str = "project-assets";

impl HubSource {
    pub fn label(&self) -> &'static str {
        match self {
            HubSource::Official { .. } => "官方选型",
            HubSource::Adopted => "选型引入",
            HubSource::SelfBuilt => "自建",
            HubSource::WithinSession => "会话内",
        }
    }

    /// plan/16 分域执行's discriminator: `true` = an *external* curated
    /// library import (mattpocock-skills/superpowers/ecc/…) whose text is
    /// another library's verbatim — spec findings degrade to Advisory.
    /// `false` for everything BW-authored, including the `Official`-labelled
    /// bw-standard library itself.
    pub fn is_external_official(&self) -> bool {
        matches!(self, HubSource::Official { official_library } if official_library != BW_STANDARD_LIBRARY)
    }

    /// plan/渠道6: `true` iff this row was scanned in from a project's own
    /// workspace (`BW_PROJECT_ASSETS_LIBRARY`). Such rows are registered-
    /// visible only (种A) — they must not appear in any injection picker
    /// (issue standard_skill / assignee / workflow crew). A
    /// VM projects this onto an `is_project_assets` bool because the UI tier
    /// doesn't see `HubSource`, only `source_label` (which is "官方选型" for
    /// every `Official` library and can't discriminate this one).
    pub fn is_project_assets(&self) -> bool {
        matches!(self, HubSource::Official { official_library } if official_library == BW_PROJECT_ASSETS_LIBRARY)
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
    /// T16 (plan/12 §10 v1.1#3): the real agent this phase actually runs
    /// under — a NAME, same namespace as `AgentRef.name` /
    /// `crate::playbook::RoleAgent.name` (a by-name reference, not a hard
    /// FK, matching how `WorkflowSpec.agents`/`skills` already resolve).
    /// `None` does **not** mean "no agent" — it means "falls back to the
    /// workflow-level default" (`WorkflowSpec.agents.first()`), the same
    /// fallback `phase_prompts`' empty-entry convention already uses for
    /// prompts. Populated for the five built-in stage playbooks
    /// (`crate::playbook::phase_metas`); `None` for every user-authored
    /// phase today (the create/edit form is still name-only text, no
    /// per-phase agent-assignment UI yet).
    #[serde(default)]
    pub agent: Option<String>,
    /// T16: real skill NAMEs injected into this phase specifically — same
    /// namespace as `SkillRef.name`/`crate::playbook::StageSkill.name`. `[]`
    /// does **not** mean "no skills" — it means "falls back to the
    /// workflow-level default" (`WorkflowSpec.skills`). Populated for the
    /// five built-in stage playbooks; `[]` for every user-authored phase
    /// today.
    #[serde(default)]
    pub skills: Vec<String>,
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
            agent: None,
            skills: Vec::new(),
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
///
/// T16 (plan/12 §10 v1.1#3) extends `Full` with `agent`/`skills`, both
/// `#[serde(default)]` — a pre-T16 row's `Full` objects (T8-T15 real data,
/// every one of them missing these two keys) read in as `None`/`[]`, never a
/// hard failure; a fresh row round-trips its real bindings unchanged.
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
                #[serde(default)]
                agent: Option<String>,
                #[serde(default)]
                skills: Vec<String>,
            },
        }
        Ok(match OnDisk::deserialize(deserializer)? {
            OnDisk::Legacy(name) => PhaseMeta::neutral(name),
            OnDisk::Full {
                name,
                role,
                reject_to_phase,
                agent,
                skills,
            } => PhaseMeta {
                name,
                role,
                reject_to_phase,
                agent,
                skills,
            },
        })
    }
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
    /// T16 (plan/12 §10 v1.1#3): the workflow's main MD document — same
    /// nature as `SkillCard.content` (real authored text a human wrote, not
    /// a display re-hash of `goal`/`prompt`). This is T17's parse input:
    /// the (not-yet-built) "🔍 解析为流程图" action reads this text and
    /// derives `phases`' real `agent`/`skills` bindings from it. Empty for
    /// the five built-in stage templates (`stage_template_workflow`) — their
    /// phases are bound directly from `crate::playbook`, not parsed from
    /// prose — and for every workflow created through today's still
    /// name-only-text create/edit forms, honestly: `''` means "structured
    /// definition, no original document", never a fabricated placeholder.
    #[serde(default)]
    pub content: String,
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
        // Dynamic session specs never carry a hand-authored document —
        // real per-phase instructions already ride in `phase_prompts`.
        content: String::new(),
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
            // plan/17: desc 正本在包文档 frontmatter 里,`bw_library` 把它
            // 与 raw 一起声明(Boot canon 构建器守卫两者逐字相等)——这里直接
            // 取,不现扫原文:扫描既是每次调用的重复功,又会在写法合法但扫
            // 不动时静默退化成空 def 写进 skills_json。
            def: s.desc.to_string(),
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
        // T16 (plan/12 §10 v1.1#3): built-in templates leave `content`
        // honestly empty — their phases bind straight off `crate::playbook`,
        // there is no authored MD document behind them yet. The detail UI
        // says so plainly ("结构化定义，无原始文档") instead of faking one.
        content: String::new(),
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
    /// 这件技能挂在哪几个阶段角色下(2026-08-05,用户拍板「通用的 skill 应该
    /// 被划分到对应的五角色中」)。多值:`code-review` 真的既属构建也属优化。
    /// 五个全挂 = 「全阶段通用」,对每个阶段的注入候选集都算命中。
    ///
    /// 空 `Vec` 有两种含义,靠 [`Self::stage_origin`] 分辨:origin 非
    /// `Unclassified` = **已判定**不属任何阶段(如 `obsidian-vault`);origin 为
    /// `Unclassified` = 还没人归过类。这两件事必须分开 —— 混成一格就是本仓
    /// 「无数据 = Unknown,绝不假装」纪律的反面。
    ///
    /// 存储在 `skill_stage` 关联表,不在 skill 行上(前身是 T7 的单值
    /// `stage_ref` 列,已随本次改动删除)。`WorkflowSpec.stage_ref` /
    /// `AgentCard.stage_ref` 本轮不动,仍是单值。
    #[serde(default)]
    pub stages: Vec<StageKind>,
    /// 上面那次归类**从哪来**——静态表 / 蒸馏派生 / 人工。见
    /// [`bw_core::stage_catalog::StageOrigin`]。
    #[serde(default)]
    pub stage_origin: StageOrigin,
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
    /// 单调递增的行版本号(`skill.rev` 列,每次内容编辑 `rev=rev+1`)。评审
    /// 找出的真坑(2026-08-06):`skill_materialize` 曾经因为这个字段不存在,
    /// 退而用「id + 正文长度 + 支撑文件数」拼一个「稳定指纹」——同长度的正文
    /// 编辑(改个错别字)不改变指纹,物化器就会判定「未变」而跳过,磁盘上
    /// 留着过期的 SKILL.md。`rev` 浮出来之后,指纹改用 `id + rev`,任何一次
    /// 真实编辑(`update_skill` 一律 `rev=rev+1`)都必然让指纹改变。
    #[serde(default)]
    pub rev: u32,
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
    /// `SkillCard.stages`(Skill 侧 2026-08-05 起改多值;Agent 侧本轮不动,
    /// 仍是单值)。`None` = cross-stage/general (every imported ECC agent,
    /// honestly unclassified); `Some` for the five built-in stage-role
    /// agents.
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

/// What a [`CronTask`] does when due. 2026-08-18 起只剩两种,而且都
/// **不执行任何工作**——这是产品铁律「定时任务只自动建活,绝不自动完成活」
/// 落到类型上的样子:
///
/// - [`CronMode::CreateIssue`](autopilot):到点造一张阶段内的 Issue,状态
///   Normal,等人(或人点 ▶跑)去干。
/// - [`CronMode::CollectMetrics`](采集器,plan/13 D7):到点把真实数据
///   (GitHub 查询等)拉进项目指标当追加观测。采集是观测不是活,不结算任何
///   东西。
///
/// 历史:曾有 `RunWorkflow`/`RunSkill`/`RunPrompt` 三种「到点跑」模式,它们的
/// 执行体是旧的聊天式工作流引擎(`claude -p` 批处理);2026-08-18 随引擎一起
/// 删除(真实日常库里三种模式零条,`docs/superpowers/specs/2026-08-17-…` §6)。
/// 老库里残留的 `cron_task.mode IN ('run_workflow','run_skill','run_prompt')`
/// 由 `bw_store` 打开时迁移成 `create_issue`——最接近的存活语义:「到点建一张
/// 同名的活」;`parse_cron_mode` 对认不出的文本也落到 `CreateIssue`。
///
/// Wire note: `CronMode` 不以 JSON 落盘,`cron_task.mode` TEXT 列由
/// `bw_store::cron_mode_text`/`parse_cron_mode` 手工往返。
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CronMode {
    /// autopilot:到点只建一件活,绝不自动跑。
    #[default]
    CreateIssue,
    /// 采集器 (plan/13 D7): pull real data (GitHub queries) into the
    /// project's metrics as append-only observations. Collecting is
    /// *observation*, never *work* — it never runs anything and never
    /// settles anything, so it can auto-fire without breaching 「Done 永不自动」.
    CollectMetrics,
}

impl CronMode {
    /// Cron 详情卡如实标出「到点做什么」。
    pub fn label(&self) -> &'static str {
        match self {
            CronMode::CreateIssue => "建活(autopilot · 不自动跑)",
            CronMode::CollectMetrics => "采集指标(脚本 → 观测)",
        }
    }

    /// CronHub 列表行首图标。`CreateIssue` 无图标(靠 `label()` 的 autopilot
    /// 字样区分);采集用与「立即同步」同族的图标。
    pub fn icon(&self) -> &'static str {
        match self {
            CronMode::CreateIssue => "",
            CronMode::CollectMetrics => "📈",
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
    /// 历史字段:曾是「到点跑什么」的自由文本(工作流名 / 技能 id / 裸
    /// prompt)。「到点跑」三种模式 2026-08-18 已删,现存两种模式都不读它,
    /// 新建任务一律写空串;列保留只为老库读回不崩。
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
    /// 到点做什么(建活 / 采集),见 [`CronMode`]。
    #[serde(default)]
    pub mode: CronMode,
    /// A1: the stage a `CreateIssue` task scopes its Issue to (`None` =
    /// 项目当前阶段;`CollectMetrics` 不用).
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
/// GitHub 为主体的创建流(2026-07-22)：记录一个项目挂的 GitHub 远端
/// ("owner/repo" 进 `config`)。plan/13 D12 起接真探针:`SyncConnector`
/// 走 `gh repo view` 真实探活;指标级统计采集由标配 Cron(collect_metrics)
/// 负责,两者各管一段。
pub const CONNECTOR_KIND_GITHUB_REPO: &str = "github-repo";
/// CodeHub(GitLab v4 兼容,绿/黄/内源三域名)为主体的创建流(2026-07-28):
/// 记录一个项目挂的 CodeHub 远端。`config` 存 `host\0path`(host=API 域名如
/// open.codehub.huawei.com,path=org/repo)。真探针走 `codehub-cli project view`
/// (Remote::Codehub.probe,P3 已通);issue/MR 计数采集由 collect arm(P5)负责。
pub const CONNECTOR_KIND_CODEHUB_REPO: &str = "codehub-repo";

/// Map a codehub host *alias* (`green`/`open`/`yellow` — what the engine
/// stores in `remote_host` and passes to `codehub-cli -H`) to the full web
/// domain for browser-URL construction. Legacy full-domain values (e.g. an
/// already-fully-qualified host stored by an older flow) pass through
/// unchanged via the `_ => alias` arm. PRACTICE-buddy §3 convention:
/// green→`codehub-g.huawei.com`, open→内源 `open.codehub.huawei.com`,
/// yellow→`codehub-y.huawei.com`. Single source of truth — app-desktop's web
/// links and any future engine-side URL builder share this one mapping
/// instead of each re-deriving it. Pure (zero IO), wasm32-clean.
pub fn codehub_alias_to_domain(alias: &str) -> &str {
    match alias.trim() {
        "green" => "codehub-g.huawei.com",
        "open" => "open.codehub.huawei.com",
        "yellow" => "codehub-y.huawei.com",
        _ => alias,
    }
}

/// plan18-③ · 项目侧脚本连接器:记录一个项目仓里既有的采集脚本(如
/// `derive_*.py` 机械解析真实数据源、产出 `data.json`)。`config` 存 JSON:
/// `{"script":"<相对工作区脚本路径>","output":"<相对工作区输出文件>","command":"<跑脚本的命令如 python>"}`。
/// `SyncConnector` 探活=检查脚本文件存在;指标采集(`CollectMetrics` 的
/// `script` arm)= 在项目工作区 shell-out 跑脚本、读 output、按各
/// `collect_kind=script` 指标的 `collect_query` 字段路径取值写 observation。
/// 脚本自身依赖(如 Playwright/SSO)由项目侧管,buddy 只 shell-out 调。
pub const CONNECTOR_KIND_SCRIPT: &str = "script";

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
    /// C4 · issue 身份映射: the GitHub issue number `gh issue create` minted
    /// for this Issue, when the owning project has a `remote_path`. `0` =
    /// unmapped — either the project has no GitHub repo (存量无仓项目保持
    /// 本地身份,如实留白), or the real `gh issue create` call failed
    /// (创建不破: the BW-side Issue still exists, only the mapping is
    /// missing). Never a fabricated number.
    #[serde(default)]
    pub github_number: u32,
    /// C5 · PR 验收环: the pull-request number an executor run opened for this
    /// Issue (`open_pr` pushed `bw/issue-<github_number>` and ran
    /// `gh pr create`). `0` = no PR — either the project isn't repo-attached
    /// / the Issue is unmapped, the run hasn't happened, or the PR submission
    /// failed (提 PR 失败不炸 run: the run's own accounting stands, the Issue
    /// stays retryable, only the mapping is missing). Never a fabricated
    /// number. When non-zero the Issue's `InReview` state is *derived from an
    /// open PR* (plan/13 D3) and human验收 is a `MergeIssuePr`, not a bare
    /// `TransitionIssue`.
    #[serde(default)]
    pub pr_number: u32,
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
    /// C8 · 标配 Issue 三件套(plan/13 D8): stable slug of the standard
    /// SkillCard this Issue is wired to (by C9's by-name convention, e.g.
    /// `"north-star-discovery"`, `"metrics-binding"`, `"competitive-analysis"`).
    /// `""` = no association — every hand-created / autopilot Issue. Set once
    /// at creation, never rewritten. `RunIssue` resolves it against the Skill
    /// Hub *by name* and injects the real content when found; a slug that
    /// doesn't resolve (there is none today — all three standard cards are
    /// seeded by C9+C10) is an honest skip, never an error.
    #[serde(default)]
    pub standard_skill: String,
    // V1 终端会话重构(阶段1): 旧的 `interactive_started` +
    // `claude_session_id` 两列已退场(物理 DROP,业务零读)。会话身份搬到
    // `claude_conversation` 表(见 [`ClaudeConversation`])—— is_resume /
    // is_interactive 改读新表,不留双读 fallback(守「不为向后兼容留旧
    // 路径」)。
    pub created_at: i64,
    pub updated_at: i64,
}

/// V1 终端会话重构(阶段1)· Claude 会话 / Conversation: 可持续的 claude
/// 对话记忆,有独立身份(自己的 `id`),可跨多次点开,不随活(Issue)Done
/// 而结束。一件交互式活 1:1 一个会话(表上 issue_id UNIQUE),但生命周期
/// 解耦——活 Done 只结束交付,不结束会话。
///
/// 这行存的是**持久身份和恢复所需事实**(`claude --resume` 要用):buddy
/// 自己的 `id`、claude CLI 的 `claude_session_id`(hook 回传,空=首次未捕
/// 获 → fallback `build_startup_plan`)、固定 worktree 路径、分支名。PTY
/// 进程句柄/channel/当前尺寸这些纯内存的东西**不进库**,进程死了如实消
/// 失,从这行恢复身份。详见
/// `docs/v1-prototype/issue2-terminal-conversation-refactor.md` §4。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClaudeConversation {
    pub id: ConversationId,
    pub project_id: ProjectId,
    /// 一件交互式活最多一个会话(UNIQUE)。
    pub issue_id: IssueId,
    /// claude CLI 的 `--resume` id(SessionStart hook 回传)。空 = 首次
    /// spawn 还没捕获到 session_id(F1: 下次走 startup_plan fallback,
    /// 不卡在无技能会话里)。
    pub claude_session_id: String,
    /// 首次建立会话的固定 worktree 路径(重启后 resume 重建 worktree 用
    /// 同一路径,保证 encoded-cwd 一致 → claude 能找到历史会话)。
    pub workspace_path: String,
    /// 该活分支 `bw/issue-<github_number>`。
    pub branch_name: String,
    pub created_at: i64,
    pub last_opened_at: i64,
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

    /// Percentage weight per [`StageKind::ALL`] stage, summing to 100.
    pub fn mix(self) -> [u8; 5] {
        match self {
            MaturityPeriod::Explore => [40, 30, 15, 10, 5],
            MaturityPeriod::Expand => [10, 25, 20, 30, 15],
            MaturityPeriod::Mature => [5, 10, 25, 25, 35],
        }
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
