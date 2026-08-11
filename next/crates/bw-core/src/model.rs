//! The domain entity graph (plan `§2`), modelled so illegal states are
//! unrepresentable. Derived signals are never hand-written: only the derive
//! chain ([`crate::derive`]) produces a [`Derived<Signal>`], and persisted
//! caches are recomputed on load, never trusted as authority (plan `§2.5`:
//! "绝不把缓存当权威").

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

// ─────────────────────────── issue ───────────────────────────

/// Kanban lifecycle of an issue — an assignable unit of work scoped to a
/// project's stage (the live row shape is `bw_store::IssueRow`). The seven
/// states are ordered as a lifecycle: an issue advances left-to-right
/// (Backlog → Todo → InProgress → InReview → Done), but `Blocked` is a
/// recoverable side-state (not terminal — the work resumes once the blocker
/// clears), and `Cancelled` is the other terminal alongside `Done`.
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

// ───────────────────────────── run (next 切片四) ─────────────────────────────

/// 一次运行(交付/咨询)走到哪一步了——BW 自己记的账,design-s4-runmanager.md
/// §7。**这不是** `bw-connector` 契约里那个执行状态(`ExecState`)的镜像:
/// 一个是连接器报的"上游怎么样了",另一个是 BW 记的"这次运行走到哪了"。
/// 两者的映射是编排层(`bw-app`)里一个穷举匹配的函数——契约改形状时编译器
/// 会当场揪出来,分叉不可能;`bw-core` 本身对 `ExecState` 零依赖。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    /// 已插行占名额,连接器的 `start` 调用还没返回。
    Starting,
    /// 连接器确认已经在跑,拿到了上游会话号。
    Running,
    /// 正常收尾:上游报告了一个终态(成功或失败,由 `end_kind` 细分)。
    Finished,
    /// BW 侧主动取消,且这次关门先于任何"结束了"的消息抵达。
    Canceled,
    /// 重启后发现库里还开着、但 BW 已经没有句柄的运行——如实标成
    /// "不知道怎么结束的",不是猜出来的终态。
    Orphaned,
    /// 开工本身就没起来(连接器的 `start` 直接失败)。
    Failed,
}

impl RunState {
    pub const ALL: [RunState; 6] = [
        RunState::Starting,
        RunState::Running,
        RunState::Finished,
        RunState::Canceled,
        RunState::Orphaned,
        RunState::Failed,
    ];

    /// 存进 `run.state` 列的文本键——与 `Self::parse` 互为逆运算。
    pub const fn as_str(self) -> &'static str {
        match self {
            RunState::Starting => "starting",
            RunState::Running => "running",
            RunState::Finished => "finished",
            RunState::Canceled => "canceled",
            RunState::Orphaned => "orphaned",
            RunState::Failed => "failed",
        }
    }

    /// 从 `run.state` 列的文本值解析回类型;未知文本 `None`——不是所有
    /// 存储层的字符串都值得信任是这六档之一,调用方自己决定怎么如实报错。
    pub fn parse(s: &str) -> Option<RunState> {
        match s {
            "starting" => Some(RunState::Starting),
            "running" => Some(RunState::Running),
            "finished" => Some(RunState::Finished),
            "canceled" => Some(RunState::Canceled),
            "orphaned" => Some(RunState::Orphaned),
            "failed" => Some(RunState::Failed),
            _ => None,
        }
    }
}
