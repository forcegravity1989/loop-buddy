//! ViewModel:内核算好的、可直接渲染的数据。
//!
//! 屏与屏之间**只经这里**共享数据结构——`screens/plan/` 永远不许
//! `use crate::screens::overview::…`(`guard-no-cross-screen-import.sh` 守着)。
//!
//! 这一层不算任何东西:每个字段都是内核那边现算好塞进来的。壳只渲染和转发意图。

use bw_v4::model::{ConversationId, IssueId, IssueStatus, ProjectId, Signal};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vm {
    pub ready: bool,
    /// 起不来就如实说起不来,不给一个空界面假装正常。
    pub fatal: Option<String>,
    pub projects: Vec<ProjectCardVm>,
    /// 本机环境条:各个开工工具探到没探到。
    pub env: Vec<ToolProbeVm>,
    /// 打开了哪个项目。`None` = 还在项目墙。
    pub open: Option<ProjectVm>,
    pub settings: SettingsVm,
    /// 最近一条后台动作的回执(建活、铺底、发版这类)。
    pub note: Option<String>,
    /// 仓文件读不动或者解析炸了的实话。**不退回默认值假装文件不存在** ——
    /// `.bw/*.toml` 是 deny-unknown-fields 的,一个手误的键就让整份文件读不出
    /// 来,退回默认值的表现是「名片全是(待填)、配置屏说你还没铺过规范件」,
    /// 而真相是文件在、只是有一行写错了。
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SettingsVm {
    pub workspaces_root: String,
    pub db_path: String,
    pub claude_binary: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProjectCardVm {
    pub id: ProjectId,
    pub slug: String,
    pub name: String,
    /// 「想做什么」一句话,来自 `PROJECT.md` / `.bw/project.toml`。
    pub brief: String,
    pub signal: Option<Signal>,
    pub workspace_path: String,
    pub remote: String,
    /// 本周排了几张活、完成了几张。
    pub week_total: u32,
    pub week_done: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolProbeVm {
    pub name: String,
    pub label: String,
    /// `Some(true)` 探到、`Some(false)` 没装、`None` 还没接实现(灰,不是红)。
    pub ok: Option<bool>,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProjectVm {
    pub id: ProjectId,
    pub slug: String,
    pub name: String,
    pub card: CardVm,
    pub health: HealthVm,
    pub metrics: MetricsVm,
    pub current_week: String,
    /// 计划屏左栏:扫 `docs/plan/` 得到的周列表(新的在前)。
    pub weeks: Vec<WeekVm>,
    /// 正在看哪一周。
    pub viewing_week: String,
    pub board: BoardVm,
    /// 「开始本周」刚产出、**还没经人确认**的草稿活标。确认之前一张活都不建
    /// —— 这是「活由人确认才存在」那条,不是界面装饰。
    pub pending_drafts: Vec<String>,
    pub releases: Vec<ReleaseVm>,
    pub sessions: Vec<SessionVm>,
    /// 会话屏选中的那一个(按活)。
    pub session_open: Option<IssueId>,
    pub workbench: WorkbenchVm,
    pub notify: NotifyVm,
    pub config: ConfigVm,
    pub kb: KbVm,
}

/// 名片。四个字段的正本是 `PROJECT.md` 与 `.bw/project.toml`,不在库里。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardVm {
    pub brief: String,
    pub benchmark: String,
    pub north_star: String,
    pub remote: String,
    /// 在研版本,**显示用**:空的时候是「(待填)」。
    pub current_version: String,
    /// 在研版本,**机读用**:空就是空。发版这类要把它当负载送进命令的地方
    /// 只能用这一个 —— 把「(待填)」当版本号发出去,发版记录里就多一行叫
    /// 「(待填)」的版本,而且按版本号幂等,这个项目以后再也发不出版。
    pub current_version_raw: String,
    pub standard_version: String,
    /// 项目群。没配就是「未配」。
    pub chat: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HealthVm {
    pub signal: Option<Signal>,
    /// 三条判据,每条一句人话 +「成不成立」。
    pub reasons: Vec<(bool, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MetricsVm {
    pub north_star: Option<MetricCardVm>,
    pub lagging: Vec<MetricCardVm>,
    pub leading: Vec<MetricCardVm>,
    /// 读不到 `.bw/metrics.toml` 时的实话。
    pub note: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MetricCardVm {
    pub id: String,
    pub name: String,
    /// 本周读数。`None` = 没读数,显示「无数据」而不是 0。
    pub reading: Option<String>,
    pub source: String,
    pub collected_at: String,
    /// 本周哪些活在推它。
    pub driving: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeekVm {
    pub week: String,
    /// 回填出来的历史周带徽记。
    pub backfill: bool,
    pub goal: Option<String>,
    pub activity_count: u32,
}

/// 六列看板。列的顺序就是活的生命周期顺序。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BoardVm {
    pub columns: Vec<ColumnVm>,
    pub pool_label: String,
    pub todo_label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnVm {
    pub status: IssueStatus,
    pub title: String,
    pub cards: Vec<CardItemVm>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CardItemVm {
    pub id: IssueId,
    pub number: u32,
    pub title: String,
    pub category: String,
    pub tool: String,
    pub workflow: String,
    pub kind: String,
    pub origin: String,
    pub week_of: String,
    pub version: String,
    pub metric_key: String,
    pub settled: bool,
    pub status: IssueStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReleaseVm {
    pub version: String,
    pub released_at: String,
    pub note: String,
    pub included: String,
    pub origin: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionVm {
    pub issue_id: IssueId,
    pub conversation_id: ConversationId,
    pub issue_number: u32,
    pub issue_title: String,
    pub issue_status: String,
    pub branch: String,
    pub workspace_path: String,
    /// claude 的 resume id。空 = 还没捕获到,如实留空。
    pub session_id: String,
    /// PTY 进程还活着没有。**只有这一个信号是真的**——「等你输入」那种细粒度
    /// 状态要靠 claude 的 hook 回传,还没接,所以不显示,不猜。
    pub live: bool,
}

/// 会话屏中栏三个页签。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionTab {
    #[default]
    Terminal,
    /// 从文件树点开的只读视图。
    File,
    /// 从改动文件列表点开的单文件 diff。
    Diff,
}

/// 会话屏右栏:文件树 / 改动文件 / git 状态 / MR 卡。全部现算,没有一张表
/// 缓存它们。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorkbenchVm {
    /// 当前选中的那个会话在哪个 worktree 里干活。空 = 没选中任何会话。
    pub workspace: String,
    pub branch: String,
    /// 相对主干领先 / 落后。`None` = 问不出来(没有主干、没有上游),
    /// 界面显示「—」,不显示 0。
    pub ahead_behind: Option<(u32, u32)>,
    pub dirty: bool,
    /// 展开着的目录(相对仓根)。根目录恒在里面。
    pub expanded: Vec<String>,
    /// 已经读出来的目录内容:`(目录, 这一层的条目)`。
    pub tree: Vec<(String, Vec<TreeEntryVm>)>,
    pub changed: Vec<ChangedFileVm>,
    /// MR 号 + 状态。0 = 没有 MR,如实说没有,不编一个号。
    pub pr_number: u32,
    pub tab: SessionTab,
    /// 中栏打开的文件:路径 + 正文(只读)或 diff 正文。
    pub open_path: String,
    pub open_body: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TreeEntryVm {
    pub rel: String,
    pub name: String,
    pub is_dir: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChangedFileVm {
    pub path: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NotifyVm {
    /// 评审中、等人合入的活。
    pub in_review: Vec<CardItemVm>,
    /// 阻塞的活。
    pub blocked: Vec<CardItemVm>,
    pub seen_at: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigVm {
    /// 「类别→工具→workflow」三列映射。
    pub mappings: Vec<MappingVm>,
    /// 项目仓 `.claude/skills/` 里扫出来的技能包 + 现算的「用过几次」。
    pub skills: Vec<SkillVm>,
    pub tools: Vec<ToolProbeVm>,
    pub remote: String,
    /// `.bw/issue-policy.toml` 的 `[cadence]` 段:定时节律。
    pub cadence: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MappingVm {
    pub category_key: String,
    pub category_label: String,
    pub tool: String,
    pub workflow: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkillVm {
    pub slug: String,
    pub title: String,
    pub uses: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KbVm {
    /// 仓内 `docs/` 的文档树(相对路径,已排序)。
    pub docs: Vec<String>,
    /// 打开的那份文档:路径 + 原文。
    pub open_doc: Option<(String, String)>,
}
