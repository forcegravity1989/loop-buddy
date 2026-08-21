//! 命令与事件 —— 界面和内核之间唯一的通路。
//!
//! 界面只发 [`Command`]、只收 [`Event`],别的一概不做:不直接开库、不直接写
//! 仓文件、不自己判断状态能不能转。所有用例与守卫都在 [`crate::app`]。
//!
//! 这里只列 A 刀真接了实现的命令。**没接的不放进枚举** —— 一条能发出去但内核
//! 悄悄什么都不做的命令,比没有这条命令更糟;会话屏、项目群、回填那几条随
//! B / C 刀落地时再加。

use crate::model::{Category, ConversationId, IssueId, IssueStatus, ProjectId};

/// 接入一个项目时填的四个字段(意图卡)。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ProjectIntent {
    pub name: String,
    /// 想做什么。
    pub brief: String,
    /// 最像的对标。
    pub benchmark: String,
    /// 三个月长成什么样(北极星一句话)。
    pub north_star: String,
}

/// 远端仓的地址。两个字段都空 = 没挂远端(本地仓),如实留白。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RemoteRef {
    /// `"github"` | `"codehub"`;空 = 未挂远端。
    pub provider: String,
    pub host: String,
    /// `"owner/repo"`。
    pub path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    // ── 接入 ──────────────────────────────────────────────
    /// 接入项目:写项目行 + 备好工作区。名片四字段落 `PROJECT.md`,不落库。
    CreateProject {
        slug: String,
        intent: ProjectIntent,
        remote: RemoteRef,
        /// 空 = 用工作区根目录下的 `<slug>`。
        workspace_path: String,
    },
    /// 名片「编辑→保存」:改 `PROJECT.md` 与 `.bw/project.toml`,走一张轻量活。
    EditProjectCard {
        project_id: ProjectId,
        intent: ProjectIntent,
    },
    /// 配项目群:写 `.bw/project.toml` 的 `[chat]` 段。`provider` 传 `"none"`
    /// 就是明确不配群。
    SetProjectChat {
        project_id: ProjectId,
        provider: String,
        group_id: String,
        notify: Vec<String>,
    },

    // ── 规范铺底(运作活③)─────────────────────────────────
    /// 一次性运作活③:按探测结果决定要跑几步,A 刀只做第 1 步(写核心件)。
    RunStandardBootstrap { project_id: ProjectId },
    /// 把一个项目从工作台上移走。**只动库,不动仓** —— 仓是正本,里面是真实
    /// 的劳动成果。界面上要人点两下才发得出这条(第一下问「真移走?」)。
    RemoveProject { project_id: ProjectId },
    /// 把这张活上次那场会话接回来看看(`claude --resume <id>`)。
    ///
    /// **不改活的状态、不发任何 prompt** —— 纯粹是「让我看看上次聊到哪了」。
    /// 和 ▶开工 的区别就在这:▶开工 会把活推到「进行中」,这条不会。
    ReopenSession { issue_id: IssueId },
    /// 纯读:按「缺 / 过期 / 人改过 / 一致」四类对账,不建活不写仓。
    ReconcileStandard { project_id: ProjectId },

    // ── 计划 ──────────────────────────────────────────────
    /// 总览「开始本周」:写这一周的周计划文件并产出草稿活标,等人确认。
    /// 判据是「本周文件不存在」——查文件,不查任何索引表。
    StartWeekPlanning { project_id: ProjectId, week: String },
    /// 人确认草稿后真的建活(`origin = agent_split`)。
    ConfirmWeekDraft {
        project_id: ProjectId,
        week: String,
        titles: Vec<String>,
    },
    /// 建一张活。`category` 决定默认工具与 workflow(查 `.bw/issue-policy.toml`)。
    CreateIssue {
        project_id: ProjectId,
        title: String,
        body: String,
        category: Option<Category>,
        kind: crate::model::IssueKind,
        origin: crate::model::IssueOrigin,
        week_of: String,
    },
    /// 排期:排进某一周,或传空串移回待办池。
    ScheduleIssue {
        id: IssueId,
        week_of: Option<String>,
    },
    /// 同列内排先后。纯展示,不动状态机。
    ReorderIssue { id: IssueId, after: Option<IssueId> },
    /// 换这张活用的 workflow / 单技能。
    SetIssueWorkflow { id: IssueId, workflow: String },
    /// 切在研版本,纯本机动作,不建活。
    SetCurrentVersion {
        project_id: ProjectId,
        version: String,
    },
    /// 发版本:给选中的活写版本号 + 往 `.bw/releases.md` 追加一行。
    CutRelease {
        project_id: ProjectId,
        version: String,
        note: String,
        included: Vec<IssueId>,
    },
    /// 用周计划文件覆盖 `issue` 的缓存列 —— 文件说了算。幂等。
    RefreshIssueCacheFromPlan { project_id: ProjectId, week: String },
    /// 把项目工作区的主检出快进到远端最新。
    ///
    /// **为什么需要人手点这一下**:buddy 只在「合入并完成」某张活的时候自动拉
    /// 一次。人在 GitHub / codehub 网页上直接合了 MR,buddy 完全不知道 —— 工作
    /// 区就一直停在旧提交,合进去的 `.bw/` 那些件在本机根本不存在,而界面还照
    /// 常显示旧内容。以前没有任何入口能补这一下。
    PullWorkspace { project_id: ProjectId },

    // ── 干活 ──────────────────────────────────────────────
    /// 唯一的干活入口。跑完最远只到「评审中」。
    RunIssue { id: IssueId },
    /// 「提交并开 MR」:把这张活 worktree 里 agent 干出来的改动提交、推分支、
    /// 开 MR,然后把活推到「评审中」。
    ///
    /// **由人点**。agent 什么时候算干完只有人知道;而且这一下之后活就进了别人
    /// 的视线,不该由程序替人决定。它**最远只到「评审中」** —— 「完成」还是得
    /// 人在评审完之后再点一次。
    SubmitIssueWork { id: IssueId },
    /// 状态转移。合法性由 `can_transition_to` 守;「完成」只能从「评审中」来,
    /// 而且只能是人显式发这条命令。
    TransitionIssue { id: IssueId, to: IssueStatus },
    /// 卡住了。如实停在原地,可以重试,不假装前进。
    BlockIssue { id: IssueId, reason: String },

    // ── 配置 ──────────────────────────────────────────────
    /// 保存一行「类别→工具→workflow」映射,写 `.bw/issue-policy.toml`。
    SaveToolMapping {
        project_id: ProjectId,
        category: Category,
        tool: String,
        workflow: String,
    },
    /// 手动探活一次。项目墙「测一下」与配置屏共用。
    ProbeTool { name: String },
    /// 改「工作区根目录」——新接入的项目默认落在哪。**只是本机设置**,存 `app_meta`,
    /// 不进任何仓文件:换台机器仓拉下来还在原处,这个值本来就该各人各配。
    ///
    /// 只影响**之后**接入的项目:已接入的项目行里各自记着自己的仓路径,不会被这
    /// 一改牵着走(不然改一下根目录,已有项目就集体找不到仓了)。
    SetWorkspacesRoot { path: String },

    // ── 通知 ──────────────────────────────────────────────
    /// 记「这个项目的事件流看到哪个时间点」。只影响视觉状态,不参与待处理计数。
    MarkNotifySeen { project_id: ProjectId, at: i64 },

    /// 定时的一跳。**只会自动建活,绝不自动完成活**——到点了就查本周有没有
    /// 那张资产盘点活,没有就建一张并自动开工。没到点、已经有了,都是原地
    /// 返回空事件,不是错误。
    TickScheduler { project_id: ProjectId },

    /// 老项目历史回填 —— 把「本该有但老项目没攒出来」的周计划文件与发版行补
    /// 出来。语义是「给资产盘点这个 workflow 传一次 `mode=first`」,不是另开
    /// 一条平行流水线。重跑安全:已有的周文件不碰,发版行按版本号去重。
    BackfillHistory { project_id: ProjectId },
    /// 通知屏的「合入并完成」。先真的合 MR,再把活推「完成」——顺序反了,
    /// 合入失败就会留下一张已完成、改动却还挂在分支上的活。
    MergeAndSettle { id: IssueId },

    /// 把一件事同步进项目群。**调用即完成**:不写库、不去重、不排重试队列。
    SyncNotifyToChat {
        issue_id: IssueId,
        /// `review` / `merged` / `release`。
        event_type: String,
    },
    /// ■停止:关掉这件活的内嵌终端。**状态原地不动**——停下来既不是失败也
    /// 不是完成,人随时能再点▶跑接回去。
    CancelRun { id: IssueId },
    /// 人在内嵌终端里敲的字节。
    TerminalInput {
        conversation_id: ConversationId,
        bytes: Vec<u8>,
    },
    /// 终端框大小变了,PTY 那头也要跟着变,否则 agent 输出会按 80×24 折行。
    TerminalResize {
        conversation_id: ConversationId,
        cols: u16,
        rows: u16,
    },
}

/// 一个工具探活的结果。探不到就是探不到,不猜。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProbeResult {
    /// 探到了,带上探到的路径或版本。
    Found(String),
    /// 本机没装 / 没跑起来。
    Missing(String),
    /// 还没接这个工具的探活实现 —— 如实说「不知道」,不报绿也不报红。
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    ProjectCreated {
        id: ProjectId,
        slug: String,
        /// 仓里本来就有 `.bw/project.toml` —— 这次是「接手已有项目」,
        /// 人手填过的字段一个都没覆盖。
        adopted: bool,
    },
    ProjectCardEditPending {
        issue_id: IssueId,
    },
    ProjectChatChanged {
        project_id: ProjectId,
    },
    /// 规范铺底第 1 步真的把文件写进仓并提交了。`files` 是实际落盘的路径。
    /// 项目从工作台上移走了。`issues` = 跟着一起删掉的活数,好让人知道
    /// 自己刚才丢掉了多少账;仓和活自己的 worktree 都还在硬盘上。
    ProjectRemoved {
        slug: String,
        issues: u64,
        workspace: String,
    },
    /// 上次那场会话接回来了。`live` = 之前就开着、这次什么都没做。
    SessionReopened {
        issue_id: IssueId,
        live: bool,
    },
    StandardBootstrapped {
        project_id: ProjectId,
        issue_id: IssueId,
        files: Vec<String>,
        committed: bool,
    },
    StandardReconciled {
        project_id: ProjectId,
        missing: Vec<String>,
        stale: Vec<String>,
        human_edited: Vec<String>,
    },
    /// 周计划文件写出来了,附带等人确认的草稿活标。
    WeekPlanStarted {
        project_id: ProjectId,
        week: String,
        /// 运作活①本身那张活。界面据此跳会话屏。
        issue_id: IssueId,
        draft_titles: Vec<String>,
    },
    /// 本周文件已经存在,这次什么都没做 —— 重跑不产生重复数据。
    WeekPlanAlreadyExists {
        project_id: ProjectId,
        week: String,
    },
    /// 本周那张运作活①还在路上(周计划文件要等它的 MR 合入才落地),这次什么
    /// 都没做。**不是错误** —— 人再点一次「开始本周」该被平静地挡住,而不是
    /// 收到一句「这张活现在是评审中,不是能开工的状态」。
    WeekPlanInProgress {
        project_id: ProjectId,
        week: String,
        issue_id: IssueId,
        status: String,
    },
    IssueCreated {
        id: IssueId,
        number: u32,
    },
    IssueScheduled {
        id: IssueId,
        week_of: String,
    },
    IssueReordered {
        id: IssueId,
    },
    /// 一次 ▶跑 结束。`summary` 原样来自执行器(mock 的自带【mock】字样)。
    IssueRan {
        id: IssueId,
        ok: bool,
        summary: String,
    },
    /// 「提交并开 MR」走完了。`commits` 是这条分支上比基线多出来的提交数,
    /// `pr_number = 0` 时 `note` 说明为什么没有 MR —— 绝不摆一个空号。
    IssueSubmitted {
        id: IssueId,
        branch: String,
        commits: u32,
        pr_number: u32,
        note: String,
    },
    /// 工作区根目录改好了。`pinned` = 有几个原本没填仓路径的老项目,被就地钉死在
    /// 了老位置(它们一个都没搬,只是从此不再跟着根目录走)。
    WorkspacesRootChanged {
        path: String,
        pinned: u32,
    },
    IssueTransitioned {
        id: IssueId,
        to: IssueStatus,
        /// 这次是不是真的结清了(人点「完成」那一下)。已经结过就是 `false`。
        settled: bool,
    },
    IssueBlocked {
        id: IssueId,
    },
    ReleaseCut {
        version: String,
        rows_written: bool,
    },
    CurrentVersionChanged {
        version: String,
    },
    /// 拉工作区的结果。`moved` = 主检出真的往前走了;`note` 是给人看的一句话,
    /// 拉不动时装的是 git 的原话(压成一行),**不吞**。
    WorkspacePulled {
        moved: bool,
        note: String,
    },
    ToolMappingSaved {
        category: Category,
    },
    ToolProbed {
        name: String,
        result: ProbeResult,
    },
    IssueCacheRefreshed {
        week: String,
        updated: u32,
        /// 文件里有、库里还没有,这次照着建出来的活数。
        created: u32,
    },
    /// 回填跑完了。`weeks`/`releases` 是**真的写出去**的那些,不是"扫到的"。
    HistoryBackfilled {
        project_id: ProjectId,
        weeks: Vec<String>,
        releases: Vec<String>,
        note: String,
    },
    /// 往项目群发了(或没发成)一条。**不落库**——通知行上那句「已发到群 ✓」
    /// 和事件流里那一条是同一个事件在两处的展示,只在当前这次运行里存在。
    ChatNotifySent {
        number: u32,
        event_type: String,
        ok: bool,
        /// 失败原文。成功时为空。
        note: String,
    },

    /// 合入并完成走完了。`merged=false` = 这张活本来就没有 MR 可合(本地项目
    /// 或者还没开 PR),只走了「完成」那一步。
    IssueMerged {
        id: IssueId,
        pr_number: u32,
        merged: bool,
        /// 合入之后在本机做的收尾:主检出有没有拉到最新、那条活分支有没有
        /// 收掉。**拉不动就写拉不动、删不掉就写删不掉**,绝不假装做过了。
        /// `merged=false` 时为空(没合过就没有收尾这回事)。
        local_note: String,
    },
    /// ■停止按下去之后。`was_live=false` = 本来就没有活着的终端可停。
    RunCancelled {
        id: IssueId,
        was_live: bool,
    },
    /// 定时真的建出了一张活。界面上「本周运作」栏据此显示「已自动开工」。
    OpsAutoFired {
        id: IssueId,
        workflow: String,
        week: String,
    },
    NotifySeenMarked {
        project_id: ProjectId,
    },
    /// 项目健康算完了一次。灯与理由都是现算出来的,库里那两列只是显示缓存。
    HealthDerived {
        project_id: ProjectId,
        signal: crate::model::Signal,
        reasons: Vec<String>,
    },
}
