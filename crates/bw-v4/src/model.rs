//! V4 的领域类型。
//!
//! 复用 `bw-core` 里仍然作数的东西:身份(`ProjectId`/`IssueId`/…)、
//! `Signal`、活的状态机 `IssueStatus`(「完成」的唯一入边是「评审中」这条
//! 铁律就锁在 `can_transition_to` 里)、`StageKind`(V4 里降级成活的类别
//! 标签)。**不复用** `bw-core::model` 的 `Project`/`Issue` 实体——V4 的字段
//! 集与 V3 不是同一件东西(名片、群、版本全部搬去了仓文件)。

use serde::{Deserialize, Serialize};

pub use bw_core::{ConversationId, IssueId, IssueStatus, ProjectId, Signal, StageKind};

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
