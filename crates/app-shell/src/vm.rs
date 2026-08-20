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
    /// 这是第几条回执。**同一句话第二次发生时序号也会变**,toast 才不会把它
    /// 当成「已经关过的那条」而静默吞掉。
    pub note_seq: u64,
    /// 接入屏那份「我账号下的仓」列表 —— 现去平台问的,不是库里的。
    pub repos: RepoPickerVm,
    /// 仓文件读不动或者解析炸了的实话。**不退回默认值假装文件不存在** ——
    /// `.bw/*.toml` 是 deny-unknown-fields 的,一个手误的键就让整份文件读不出
    /// 来,退回默认值的表现是「名片全是(待填)、配置屏说你还没铺过规范件」,
    /// 而真相是文件在、只是有一行写错了。
    pub warnings: Vec<String>,
}

/// 接入屏的仓选择器。**列不出来就说为什么**,不摆一张假列表 —— 高保真上那份
/// `REPO_LIST` 是工厂造的,真壳里这些行必须来自 `gh repo list` / `codehub-cli`。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepoPickerVm {
    /// 正在问平台。界面上是「列着…」,不是空列表 —— 空列表会被读成「你没有仓」。
    pub loading: bool,
    /// 问过一次了没有。没问过 = 界面上是一个「列出我的仓」按钮,不是「一个都没有」。
    pub asked: bool,
    /// 问失败的原话(没装 gh、没登录、域名填错)。
    pub error: Option<String>,
    pub rows: Vec<RepoRowVm>,
    /// 现在盯着哪个仓(点中的那一行,或者人手打进去的地址)。
    pub picked: Option<String>,
    /// 从那个仓的远端 `.bw/project.toml` 读回来的名片。
    pub prefill: Option<RepoPrefillVm>,
    /// 那一读的结局。**「没读到」和「没读成」必须分开** —— 网络断了、没登录、
    /// 默认分支不叫 main,这些都会读不到,但它们**不等于**「这个仓没被接管过」。
    /// 混成一句「还没被接管过」就是在瞎说,人照着填一遍反而会覆盖仓里的真名片。
    pub probe: RepoProbe,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum RepoProbe {
    /// 还没查过(人还没选仓,或者选完还没轮到)。
    #[default]
    NotAsked,
    Loading,
    /// 读到了 `.bw/project.toml` —— 这个仓被 buddy 接管过。
    Adopted,
    /// 平台明确回「没有这个文件」,而且**这个仓确实在**(另问了一次)——
    /// 这个仓没被接管过。
    Absent,
    /// 这个地址根本找不到仓(敲错了、私有仓没权限、没登录)。仓都不在,
    /// 「接管过没有」就无从谈起,更不能放人往下走去接一个不存在的仓。
    NoRepo(String),
    /// 压根没查成,原话在里面。**不是**「没被接管过」。
    Failed(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepoRowVm {
    /// `owner/repo`,直接就是要填进「远端地址」的那个值。
    pub path: String,
    pub private: bool,
    pub description: String,
    pub default_branch: String,
    /// 平台给的最近推送时间原文(可能是空的)。
    pub pushed_at: String,
}

/// 已经被 buddy 接管过的仓,名片直接回显,不用人再填一遍。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepoPrefillVm {
    pub name: String,
    pub brief: String,
    pub benchmark: String,
    pub north_star: String,
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
    /// 在研版本(仓里 `.bw/project.toml` 写的)。没写就是空,不猜一个。
    pub version: String,
    /// 当前 ISO 周,卡片上那个周 chip 显示它。
    pub week: String,
    /// 还没看过的动静:评审中 + 阻塞里,更新时间晚于「读到这里」那一下的。
    /// 通知屏点过之后这个数会掉下去。
    pub unread: u32,
    /// 本周目标,来自仓里的周计划文件。文件没有 / 目标还是占位符 = 空。
    pub week_goal: String,
    /// 上次交付:发版记录最后一行的日期与版本。仓里没有发版记录 = 空。
    pub last_delivery: String,
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
    /// 左栏点的是「全部」——看板不按周过滤。
    pub view_all: bool,
    pub board: BoardVm,
    /// 本周四段计数:待办 / 进行中 / 评审中 / 完成。总览与计划屏的进度条都用它。
    /// **当前周**的五段计数。总览那块「本周计划进度」用它。
    pub week_counts: WeekCountsVm,
    /// 计划屏正在看的那个看板的五段计数(看某一周就是那一周,点了「全部」就是
    /// 全部)。它跟着左栏走,所以**不能拿去当「本周」**。
    pub board_counts: WeekCountsVm,
    /// 本周计划文件「本周运作」那张表的行 —— 三张运作活各自走到哪了。
    pub ops: Vec<OpsChipVm>,
    /// 代码仓级指标。**现算很贵(要起好几个 git 子进程),所以按需**:
    /// `None` = 还没采过,界面显示一颗「立即采集」。
    pub repo_stats: Option<RepoStatsVm>,
    /// 名片改动那张轻量活走到哪了。`None` = 没有在途的名片改动。
    pub card_mr: Option<CardMrVm>,
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
    /// `.bw/metrics.toml` 里写的目标。空 = 还没定目标,显示「目标未设」。
    pub target: String,
    /// 这条指标的定义(`def`),给人看「这个数到底数的是什么」。
    pub def: String,
    /// 采集方式是「手填」。手填的数带徽记,一眼看得出它不是自动采来的。
    pub manual: bool,
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WeekCountsVm {
    pub todo: usize,
    pub doing: usize,
    pub review: usize,
    pub done: usize,
    /// 卡住的那几张。**单独一段** —— 混进别的段或者干脆不数,等于把红灯藏了。
    pub blocked: usize,
}

impl WeekCountsVm {
    pub fn total(&self) -> usize {
        self.todo + self.doing + self.review + self.done + self.blocked
    }

    /// 进度条一段占多宽。总数是 0 就全都是 0 —— 不给空周画一条满的。
    pub fn pct(&self, n: usize) -> u32 {
        if self.total() == 0 {
            0
        } else {
            (n * 100 / self.total()) as u32
        }
    }
}

/// 「本周运作」表里的一行。正本是周计划文件,不是库。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OpsChipVm {
    pub title: String,
    pub status: String,
    pub note: String,
}

/// 代码仓级指标。每一项都带「这个数从哪来」——采不到就整块给出原话,不填 0。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RepoStatsVm {
    /// `(数值, 这个数是什么, 从哪采的)`。
    pub items: Vec<(String, String, String)>,
    pub error: String,
}

/// 名片改动那张轻量活。名片是仓文件,改它一律走分支 + MR。
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardMrVm {
    pub issue_id: Option<IssueId>,
    pub number: u32,
    /// 活现在的状态标签(「评审中」/「已完成」这种)。
    pub status: String,
    pub pr_number: u32,
    /// 能不能点「合入并完成」——只有停在评审中的才能。
    pub mergeable: bool,
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
    /// 还没看过的动静有几件。**和项目墙卡片上那个 ⚑ 是同一个定义**(评审中/
    /// 阻塞里、更新时间晚于「读到这里」的);项目轨的红点用它,点过「读到
    /// 这里」之后它会掉到 0,而不是永远亮着。
    pub unread: u32,
    /// 事件流。**没有事件表** —— 这条流是从四张表里现算出来的:活什么时候建
    /// 的、什么时候结清的、会话什么时候开的。存不下来的事(比如某次运行失败)
    /// 就不在流里,不补一条假的。
    pub events: Vec<NotifyEventVm>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NotifyEventVm {
    /// 本机时区的「MM-DD HH:MM」。
    pub time: String,
    pub text: String,
    /// 点它跳到哪张活。`None` = 不可点。
    pub issue: Option<IssueId>,
    /// 这件事后来被处理掉了(活已完成)。
    pub done: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigVm {
    /// 「类别→工具→workflow」三列映射。
    pub mappings: Vec<MappingVm>,
    /// 项目仓 `.claude/skills/` 里扫出来的技能包 + 现算的「用过几次」。
    pub skills: Vec<SkillVm>,
    pub tools: Vec<ToolProbeVm>,
    pub remote: String,
    /// `.bw/issue-policy.toml` 的 `[cadence]` 段:定时节律。原样一句话。
    pub cadence: String,
    /// 节律拆成表格行:哪张运作活、怎么触发、判据是什么。
    pub crons: Vec<CronVm>,
    /// `.bw/connectors.toml` 里声明的连接器。文件没有就是空的。
    pub connectors: Vec<ConnectorVm>,
    /// 项目群三件:提供方 / 群号 / 同步哪些事件。仓是正本,这里只显示。
    pub chat_provider: String,
    pub chat_group: String,
    pub chat_events: Vec<(String, bool)>,
    /// `.bw/project.toml` 的 `[chat]` 段:项目群。**仓是正本**,这里只显示;
    /// 改它走「编辑项目信息」那条轻量活 + MR,不在配置屏直接写仓。
    pub chat: String,
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
    /// 「内置」(随 buddy 出厂、铺底时复制进来)/「项目自有」/「蒸馏」。
    pub origin: String,
    /// SKILL.md 头里的 `description:`。读不到就空着,不替它写一句。
    pub desc: String,
}

/// 定时那张表的一行。**没有定时表** —— 判据是「本周有没有这张活」,
/// 所以这里没有「下次触发时间」那一列,有的是「判据」。
#[derive(Clone, Debug, PartialEq)]
pub struct CronVm {
    pub name: String,
    /// `manual` / `scheduled`,翻成人话。
    pub trigger: String,
    /// 形如 `fri 20:00`;手动触发的就是「—」。
    pub schedule: String,
    /// 到底看什么才算「该跑了」。
    pub rule: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectorVm {
    pub name: String,
    /// `script` / `command` 这类。
    pub kind: String,
    /// 具体跑什么。
    pub target: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum KbTab {
    #[default]
    Docs,
    CodeGraph,
    Assets,
}

impl KbTab {
    pub fn label(self) -> &'static str {
        match self {
            KbTab::Docs => "知识",
            KbTab::CodeGraph => "代码图",
            KbTab::Assets => "资产",
        }
    }

    pub const ALL: [KbTab; 3] = [KbTab::Docs, KbTab::CodeGraph, KbTab::Assets];
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KbVm {
    pub tab: KbTab,
    /// 规范管账(`.bw/managed.toml`)里登记了几份核心件。**不做对账** ——
    /// 对账是配置屏那颗按钮的事,这里只报个数。
    pub managed_count: usize,
    /// 知识页签:按规范八大类分组的文件清单。**只列存在的文件**,不列位置。
    pub groups: Vec<KbGroupVm>,
    /// 打开的那份文档:路径 + 原文。懒加载 —— 点了才现读。
    pub open_doc: Option<(String, String)>,
    /// 代码图页签。`None` = 还没跑过(没点过这个页签)。
    pub codegraph: Option<CodeGraphVm>,
    /// 资产页签。`None` = 还没扫过。
    pub assets: Option<AssetsVm>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KbGroupVm {
    pub title: String,
    pub files: Vec<KbFileVm>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KbFileVm {
    /// 仓内相对路径,也是打开它的 key。
    pub rel: String,
    pub label: String,
    /// 「回填」这类小徽记。回填的周文件与人写的同目录同格式,只靠这个区分。
    pub badge: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeGraphVm {
    /// `ready` / `not_installed` / `not_indexed`。
    pub state: String,
    /// 灰态时的下一步该干什么。
    pub hint: String,
    pub rows: Vec<CodeFileVm>,
    /// 子进程失败的原文。**原样显示**,不留空白也不静默重试。
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeFileVm {
    pub path: String,
    pub language: String,
    pub nodes: u64,
    pub size: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AssetsVm {
    /// 项目自己的技能包(扫 `.claude/skills/**/SKILL.md` 得到)。
    pub skills: Vec<SkillVm>,
    /// 蒸馏产出的技能。V4 还没有蒸馏这颗按钮,所以今天恒为空 —— 不放假数据。
    pub distilled: Vec<SkillVm>,
    /// 产物登记 = `git log --name-only`,没有登记表。
    pub artifacts: Vec<ArtifactVm>,
    pub releases: Vec<ReleaseVm>,
    /// 仓统计:与总览第⑤块同一次采集逻辑。
    pub repo_stats: Vec<(String, String)>,
    /// 采不到时的原话。
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArtifactVm {
    pub path: String,
    pub commit: String,
    pub subject: String,
    /// 提交消息里解析到的活号。解析不到就空着,不强凑。
    pub issue: String,
}
