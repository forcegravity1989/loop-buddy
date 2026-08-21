//! V4 的领域类型。**2026-08-21 起不再从 `bw-core` 借任何东西** —— 身份、
//! 信号、活的状态机、五类别,全部住在这里。逐字拷自 V3 内核里仍然作数的
//! 定义(和 `v4-engine` 同一个「拷贝接管」逻辑:V3 那一整个目录最终要删,
//! V4 不能有依赖指向那边),serde 形状一字未动,所以库里已有的行照读。
//!
//! 「完成」的唯一入边是「评审中」这条铁律锁在 [`IssueStatus::can_transition_to`]
//! 里 —— 那张转移表是**逐字拷贝**的,改它任何一条边都等于改产品铁律,先去
//! 读 CLAUDE.md 的铁律表再动。

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_newtype {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// 包住一个已有的 UUID(比如从库里读回来的)。
            pub const fn from_uuid(u: Uuid) -> Self {
                Self(u)
            }

            /// 全零占位 id,测试与「不属于任何一个」的场合用。
            pub const fn nil() -> Self {
                Self(Uuid::nil())
            }

            pub const fn uuid(self) -> Uuid {
                self.0
            }

            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }
    };
}

id_newtype!(
    /// 项目的稳定身份。
    ProjectId
);
id_newtype!(
    /// 活的稳定身份。
    IssueId
);
id_newtype!(
    /// 一场 agent 会话的稳定身份(`claude_conversation` 表的主键)。
    ConversationId
);

/// 健康信号。`Unknown` 是诚实的第四态 —— **没数据绝不默认成绿**。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Signal {
    Green,
    Amber,
    Red,
    Unknown,
}

/// 活的类别 —— V3 的五阶段在 V4 降级成的标签。类别决定默认开工工具与默认
/// workflow(映射正本在 `.bw/issue-policy.toml` 的 `[[mapping]]` 段)。
/// V4 只用到标签和全集;V3 那套阶段方法论元数据(角色、下一棒、DoD)不拷。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
// 拷贝时漏过一次(评审抓的)。少了它,serde 名字从 `build` 变成 `Build`,
// 而本文件下方 `category_key` 的注释正说「文件里写 build 就是 Build」——
// 今天手写的那对转换兜住了,但只要有人照注释改走 serde 就会写出大写值,
// 和库里已有的小写行混在同一列。
#[serde(rename_all = "snake_case")]
pub enum StageKind {
    /// 原型
    Prototype,
    /// 构建
    Build,
    /// 优化
    Optimize,
    /// 运营推广
    Growth,
    /// 运维
    Ops,
}

impl StageKind {
    pub const ALL: [StageKind; 5] = [
        StageKind::Prototype,
        StageKind::Build,
        StageKind::Optimize,
        StageKind::Growth,
        StageKind::Ops,
    ];

    pub fn label(self) -> &'static str {
        match self {
            StageKind::Prototype => "原型",
            StageKind::Build => "构建",
            StageKind::Optimize => "优化",
            StageKind::Growth => "运营推广",
            StageKind::Ops => "运维",
        }
    }
}

/// 活的状态。生命周期顺序;`Blocked` 是可恢复的旁路态,不是终点。
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

    /// 状态机的**唯一**事实源 —— 每一处转移守卫都问这张表,谁都不许自己发明边。
    /// 「完成」的唯一入边是「评审中」(`(InReview, Done)`),这条是产品铁律。
    /// 逐字拷自 V3 内核,一条边没加没减。
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

/// 活的三个种类。`Light`(轻量活)= 没有 agent 会话、只有 buddy 自己写仓 +
/// 开 MR 的活,名片编辑与发版本用它。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    Business,
    Ops,
    Light,
}

impl IssueKind {
    pub fn label(self) -> &'static str {
        match self {
            IssueKind::Business => "业务活",
            IssueKind::Ops => "运作活",
            IssueKind::Light => "轻量活",
        }
    }
}

/// 这张活是谁建的。`Backfill` = 历史回填出来的,照远端状态,**不影响任何
/// 计数或排序特权**,也不参与健康灯的三条判据(回填不点灯)。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueOrigin {
    Human,
    Auto,
    AgentSplit,
    Backfill,
}

impl IssueOrigin {
    pub fn label(self) -> &'static str {
        match self {
            IssueOrigin::Human => "人建",
            IssueOrigin::Auto => "定时自动建",
            IssueOrigin::AgentSplit => "agent 拆出",
            IssueOrigin::Backfill => "回填",
        }
    }
}

/// 活的类别标签 —— 五阶段在 V4 降级成的东西。类别决定默认开工工具与默认
/// workflow(映射正本在 `.bw/issue-policy.toml` 的 `[[mapping]]` 段)。
/// 用 `bw_core::StageKind` 的五个变体,不另造一套枚举。
pub type Category = StageKind;

/// `.bw/issue-policy.toml` 里 `[[mapping]]` 段用的类别键。与
/// `StageKind` 的 serde 名字(snake_case)一致,所以文件里写 `build` 就是
/// `StageKind::Build`。
pub fn category_from_key(key: &str) -> Option<Category> {
    match key {
        "prototype" => Some(StageKind::Prototype),
        "build" => Some(StageKind::Build),
        "optimize" => Some(StageKind::Optimize),
        "growth" => Some(StageKind::Growth),
        "ops" => Some(StageKind::Ops),
        _ => None,
    }
}

pub fn category_key(c: Category) -> &'static str {
    match c {
        StageKind::Prototype => "prototype",
        StageKind::Build => "build",
        StageKind::Optimize => "optimize",
        StageKind::Growth => "growth",
        StageKind::Ops => "ops",
    }
}

/// 项目行:定位 + 项目墙显示缓存。名片/群/版本不在这里(正本 `PROJECT.md`
/// 与 `.bw/project.toml`,打开项目时现读)。
#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub slug: String,
    pub name: String,
    pub workspace_path: String,
    pub provider: String,
    pub remote_host: String,
    pub remote_path: String,
    /// 健康灯显示缓存。`None` = 没数据 = Unknown 灰,不是绿。
    pub signal: Option<Signal>,
    pub weekly_signal: Option<Signal>,
    pub signal_derived_at: Option<i64>,
    pub sort_order: f64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Project {
    /// 有没有挂远端。空 `remote_path` = 没挂,如实留白,绝不编造地址。
    pub fn has_remote(&self) -> bool {
        !self.remote_path.is_empty()
    }
}

/// 活:远端 issue 的本机缓存 + 九个扩展列(正本在周计划文件)。
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub id: IssueId,
    pub project_id: ProjectId,
    /// 本机连续号,项目内唯一。周计划文件与发版记录引用的就是它(挂了远端
    /// 的活另有 `remote_number`)。
    pub number: u32,
    /// 远端 issue 号;`0` = 未映射,绝不编造。
    pub remote_number: u32,
    pub title: String,
    pub body: String,
    pub status: IssueStatus,
    pub branch: String,
    /// MR 号;`0` = 没有 MR。「干没干成看远端 MR 合没合入」这条判据的落点。
    pub pr_number: u32,
    pub week_of: String,
    pub version: String,
    pub tool: String,
    pub kind: IssueKind,
    pub origin: IssueOrigin,
    pub workflow: String,
    pub category: Option<Category>,
    pub sort_order: f64,
    pub metric_key: String,
    pub created_at: i64,
    pub updated_at: i64,
    /// 人显式点「完成」的那一刻;`None` = 还没结清。
    pub settled_at: Option<i64>,
}

impl Issue {
    /// 排进某一周了没有。`week_of` 为空 = 还在待办池。
    pub fn is_scheduled(&self) -> bool {
        !self.week_of.is_empty()
    }
}

/// 活 ↔ claude 会话 ↔ worktree ↔ 分支。
#[derive(Clone, Debug, PartialEq)]
pub struct Conversation {
    pub id: ConversationId,
    pub project_id: ProjectId,
    pub issue_id: IssueId,
    pub claude_session_id: String,
    pub workspace_path: String,
    pub branch_name: String,
    pub created_at: i64,
    pub last_opened_at: i64,
}
