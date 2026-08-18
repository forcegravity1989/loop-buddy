//! `Command` / `Event` 与界面导航枚举(View/Panel/Scope 等)——UI 与内核之间的全部词汇。
//! 从 lib.rs 机械拆出(2026-08-17),经 `pub use command::*` 原路径可达。

use super::*;

/// 「接入已有仓」一次拉多少条。搜索只过滤已加载的，所以要多拉、下拉少画
/// （PRACTICE §4.16：30 截掉第 74 名；200 仍盖不住很多人的仓数）。
pub const ONBOARD_REPO_LIST_LIMIT: u32 = 999;

/// Top-level workspace view (only meaningful for `hub == workspace`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum View {
    #[default]
    Projects,
    /// The creation card-flow (意图 → 快答 → 起草 → 审阅确认).
    Create,
    App,
}

/// Operating-view toolbar tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Panel {
    Progress,
    Workflow,
    Routine,
    Artifact,
    Version,
    /// Issue 看板 (R1) — assignable work units scoped to a stage, the
    /// multica-style board the operating view now surfaces.
    Issues,
}

/// Stage-axis selection: all stages or one of the five.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    All,
    Stage(StageKind),
}

/// Where a newly-created project's git remote comes from — the Repo 卡片的
/// 选择,carried into `Command::CreateProject`. `New` mints a fresh GitHub
/// repo (`gh repo create --clone`); `Existing` clones one the user already
/// owns. `None` on the command (every pre-2026-07-22 caller) keeps every
/// existing behavior — pure local mint or bound-local-path — untouched.
#[derive(Clone, Debug)]
pub enum GithubOrigin {
    New { slug: String, private: bool },
    Existing { owner: String, repo: String },
}

/// CodeHub 为主体的创建流(2026-07-28):Repo 卡片选 codehub 平台时的远端
/// 身份。V1 Issue 1(2026-08-04)改为 enum,对仗 [`GithubOrigin`] 的
/// `New`/`Existing` 两臂:
/// - [`CodehubOrigin::Existing`] = 接入已有仓(`host` + `path` = org/repo,clone)
/// - [`CodehubOrigin::New`] = 新建仓(`host` + 个人 `namespace` 路径 + `name` +
///   `visibility`,`codehub-cli project create` + clone + BW root commit)
///
/// `host` = API 域名 alias(green/open/yellow);`namespace` = 个人 namespace
/// 路径(如 `z30026659`,空串 = 引擎自动解析个人 namespace)。group namespace
/// 选择 V1 不做(§6 偏差,如实标)。
#[derive(Clone, Debug)]
pub enum CodehubOrigin {
    New {
        host: String,
        namespace: String,
        name: String,
        visibility: String,
    },
    Existing {
        host: String,
        path: String,
    },
}

/// plan/20 R5: what [`Command::AdoptIntoProject`] copies — plan/08 S1 的
/// `{ kind, id }`,按本仓命令风格落成带类型 id 的枚举,拼错 kind 编译不过。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptTarget {
    Skill(SkillId),
    Agent(AgentId),
    Workflow(WorkflowId),
}

/// UI → kernel intents.
pub enum Command {
    /// App start: load the project wall and re-derive every running project's
    /// signals against the *current* clock (staleness must show on the wall).
    Boot,
    /// Creation flow step 1 (意图): mints the project row immediately so the
    /// rest of the flow (drafting run, resume-if-interrupted) has somewhere
    /// real to attach to. `desc` carries the free-text brief.
    CreateProject {
        /// C16(plan/14 规范条 4): 仓平台选择器的选中值 —— 今天恒 `"github"`
        /// (唯一可选项,GitLab/Gitcode 灰置「未接」),从 `RepoCard` 顶部的平台
        /// chip 传入,不在这里硬编码。留好字段语义:第二个平台真接时,这里
        /// 已经是"选出来的"而不需要改 Command 形状。落进 `project.provider`。
        provider: String,
        id: ProjectId,
        name: String,
        kind: String,
        desc: String,
        /// P1: optional pre-existing *local* repo to bind (must contain
        /// `.git`). Mutually exclusive with `github` — the Repo 卡片 is the
        /// sole UI entry point and only ever sets one of the two.
        workspace: Option<String>,
        /// GitHub 为主体(2026-07-22): Repo 卡片的选择. `None` = neither
        /// bound (`workspace` also `None`) → today's local-mint-if-configured
        /// default, unchanged.
        github: Option<GithubOrigin>,
        /// CodeHub 为主体(2026-07-28):Repo 卡片选 codehub 平台时的远端身份。
        /// 与 `github`/`workspace` 三选一(UI 只设其一)。`None` = 没选 codehub
        /// (走 github 或本地)。maas 这类已有 codehub 仓走这条(clone,不建新)。
        codehub: Option<CodehubOrigin>,
    },
    /// GitHub 为主体的创建流(2026-07-22): 读一次当前用户可接入的仓列表,
    /// 填充 `AppState.github_repos`(Repo 卡片"接入已有仓"下拉的数据源)。
    /// 显式加载,同 `LoadVersionLog`/`LoadArtifacts` 惯例——不在每次
    /// rebuild 里打 GitHub API。
    ListGithubRepos,
    /// CodeHub 为主体的创建流(V1 Issue 1): 读一次当前用户在指定 host 上的
    /// codehub 仓列表,填充 `AppState.codehub_repos`(Repo 卡片"接入已有仓"
    /// 下拉的数据源)。对仗 `ListGithubRepos`,但 codehub 有 green/open/
    /// yellow 三域名,需显式带 `host`。显式加载,同 `ListGithubRepos` 惯例。
    ListCodehubRepos {
        host: String,
    },
    /// Creation flow step 2 (快速问题 · 周期).
    SetCycle {
        cycle: MaturityPeriod,
    },
    /// 对标竞品 + 三个月成功标准 (creation flow's free-text questions).
    UpdateBrief {
        benchmark: String,
        opportunity: String,
    },
    UpdateNorthStar {
        value: String,
        def: String,
    },
    /// P9(项目编辑缺口): `name`/`kind`/`descr` — the three fields
    /// `CreateProject` writes once and, before this command existed, had no
    /// setter anywhere in `bw-store`, so a typo'd name was permanently
    /// stuck (only `DeleteProject` could touch it, and that takes every
    /// Issue/run/artifact with it). Same duplicate-name policy as
    /// `CreateProject`: names aren't unique in this product (no `UNIQUE`
    /// constraint, no dedup check at either the command or UI layer), so
    /// renaming to a name that collides with another project is allowed —
    /// rejecting it here would be a stricter bar than creation itself.
    /// `name` is still required non-empty (creation's UI-only `can_send`
    /// gate, enforced here for real since a UI button being disabled is not
    /// a guard). Writes through `self.active()`, same as `UpdateBrief`/
    /// `SetCycle`/`UpdateNorthStar`.
    UpdateProjectIdentity {
        name: String,
        kind: String,
        descr: String,
    },
    /// Record a metric + its current value as an append-only Manual observation
    /// (creation-flow review, or later while operating a stage). Signal is
    /// derived, never set here.
    UpsertManualMetric {
        id: MetricId,
        name: String,
        def: String,
        role: MetricRole,
        stage_kind: Option<StageKind>,
        target: String,
        amber: AmberBand,
        value: String,
    },
    /// 停用(归档)或恢复一条指标 —— 指标退役的唯一产品路径,**没有物理
    /// 删除**。`observation` 是 append-only 的:硬删 metric 行要么级联抹掉
    /// 真实测量历史,要么留下孤儿观测,两个都不可接受。停用把"不想再看见
    /// 它"和"它当初真测过什么"拆开——行留着、观测一条不删,只是退出界面
    /// 默认视图、退出健康灯上卷、退出自动采集。
    ///
    /// 停用后紧跟一次 `recompute_signals`:项目/阶段的上卷要把这条的进出
    /// 反映出来。被停用行自己那盏灯冻结在停用那一刻(recompute 跳过归档
    /// 行),恢复后由下一次 recompute 重新派生 —— derive-only 不破。
    SetMetricArchived {
        metric: MetricId,
        archived: bool,
    },
    /// The monitoring loop's heartbeat: a new Manual value is born as an
    /// observation, then every signal is re-derived. Never sets a signal.
    RecordObservation {
        metric: MetricId,
        value: String,
    },
    /// A **machine-collected** observation — same append-only path as
    /// `RecordObservation`, but the source is the collector that really
    /// measured it (`Ci` / `GitPr` / …), never `Manual`. This is the evidence
    /// collector's write path (`bw_engine::evidence` → metric), the first
    /// non-Manual L0 producer (Tier D's minimal down payment).
    RecordCollectedObservation {
        metric: MetricId,
        value: String,
        source: SourceKind,
    },
    /// Hand-set plan progress for one stage (plan data, not a signal — the
    /// derive chain is untouched).
    SetStageProgress {
        stage_kind: StageKind,
        progress: u8,
    },
    /// Flip one handoff/DoD checklist box.
    ToggleDod {
        stage_kind: StageKind,
        index: usize,
    },
    /// Advance the project's active stage (or reflux `Ops → Prototype`).
    /// `risky` and `note` are the caller's honest account of the checklist
    /// state — a handoff is never silently blocked on an unchecked box.
    HandoffStage {
        risky: bool,
        note: String,
    },
    /// Confirms the creation-flow draft: materializes the five stages (each
    /// on the chosen review cadence) and switches the project into `Running`.
    /// This is the creation flow's real *landing* point (读源码定,写明选择
    /// — 不是 Repo/Intent 卡提交,是末卡「确认 · 建立项目」): a
    /// remote_path-backed project gets its standard Issue trio
    /// (竞品分析→找指标→绑数据, plan/13 D8) minted here, right alongside
    /// `set_project_phase(Running)`.
    CompleteCreation {
        cadence: Cadence,
        /// C8 · 末卡「立即让队友开工第一件?」勾选(plan/13 D8). Default
        /// `false` — every pre-C8 caller. `true` dispatches a real
        /// `RunIssue` against the standard trio's 竞品分析 Issue right after
        /// landing, same explicit-authorization shape as any other
        /// human-triggered run (never autopilot, never on a project with no
        /// standard trio to run).
        run_first: bool,
    },
    /// Configure (or, with an empty `path`, clear) the real-executor target
    /// directory + whether it may also run shell commands. `path` must be a
    /// real, existing directory unless empty — a bad path fails fast here
    /// rather than surfacing only when a workflow is next run.
    SetWorkspace {
        path: String,
        allow_commands: bool,
    },
    /// P1(loop-buddy↔aihot 接线 spec):给一个**存量**项目补上 GitHub 远端
    /// —— `CreateProject` 的「绑定本地目录」分支([lib.rs:3121] 附近)只
    /// `set_workspace`,从不写 `remote_path`,产品里此前没有补救入口。
    /// 对活跃项目生效(`self.active()`)。`gh repo view` 先探活,探不到就
    /// 如实报错、一个字节不写库;探到之后依次写 `remote_path`、补建
    /// (幂等)`github-repo` connector、再接线本地工作区的 `origin`(工作区
    /// 已有 remote 且不符目标 → 拒绝覆盖,不静默改写用户的 git 配置)。
    /// `push_local=true` 时额外推当前分支。
    AttachRepo {
        owner: String,
        repo: String,
        push_local: bool,
    },
    /// Replace the process-wide `ClaudeCliConfig` outright (Settings hub).
    /// In-memory only — same persistence tier it already had (env-var-seeded
    /// once at boot); this just makes it editable for the rest of the
    /// process's lifetime instead of frozen.
    SetClaudeConfig {
        binary: Option<String>,
    },
    /// Real `git log` on the active project's `workspace_path` (Version
    /// panel). Explicit, user-triggered — never fetched eagerly on `Boot`,
    /// since it's per-project, potentially slow, and most projects have no
    /// `workspace_path` configured at all.
    LoadVersionLog,
    /// Load the active project's registered artifacts into state (Artifact
    /// panel). Same explicit-load pattern as `LoadVersionLog`.
    LoadArtifacts,
    /// P4: assemble one Issue's detail (its runs + each run's real file
    /// changes + its artifacts) into state for the board overlay. Read-only.
    OpenIssueDetail(IssueId),
    /// P4: close the overlay (clears the assembled detail).
    CloseIssueDetail,
    /// Re-scan the active project's workspace right now and register any new
    /// artifact versions (the manual counterpart to the automatic post-run
    /// scan). Requires a configured workspace.
    CollectArtifacts,
    /// Run a connector's *real* probe: `git-repo` collects live workspace
    /// evidence (and feeds it to the bound project's matching metrics as
    /// `SourceKind::Connector` observations — Tier D for real); `claude-cli`
    /// checks the executor binary. Any other kind errors honestly — there is
    /// no fake "synced" state.
    SyncConnector {
        id: ConnectorId,
    },
    /// C6 (plan/13 D5+D6): read the active project's `.bw/metrics.toml`
    /// (metrics source of truth) and sync it into the SQLite cache — north
    /// star name/def/collect plan updated in place, every lagging/leading
    /// metric upserted by name (idempotent: re-syncing an unchanged file
    /// inserts zero new rows). No configured workspace, or a workspace with
    /// no file yet, is a deliberate silent no-op — same "nothing to report"
    /// stance as a project that was never wired to GitHub. A file that fails
    /// to parse emits `Event::ConnectorSynced { ok: false, .. }` and writes
    /// nothing (parse succeeds in full or the cache stays untouched). Never
    /// appends an observation or calls `recompute_signals` — this syncs
    /// *definitions*, not values (collection execution is a later ticket,
    /// C7).
    SyncMetricsFile,
    /// V2-②-I: list open issues on the project's remote and rebuild/refresh
    /// local issue rows (Backlog). Never creates remote issues; never writes
    /// local Done. Local rows whose remote is no longer open and that this
    /// Buddy never settled → Cancelled (off-board); local Done stays for
    /// resume. Same path for creation-flow auto sync and the manual
    ///「从仓同步 Issue」button.
    SyncRemoteIssues,
    /// V2-② Intent UX (§6.2): probe remote `.bw/project.toml` *before* clone,
    /// so the Intent card can readonly-prefill later-comers. `provider` =
    /// `"codehub"` | `"github"`; `host` is the codehub alias (ignored for
    /// github); `path` = `namespace/repo` (codehub) or `owner/repo` (github);
    /// `default_branch` from the repo list (empty → engine falls back to
    /// `main`). Absent file → first-comer; fetch/parse error → Failed (UI
    /// stays editable — never pretend later-comer).
    ProbeRemoteProjectToml {
        provider: String,
        host: String,
        path: String,
        default_branch: String,
    },
    /// Clear [`AppState::remote_project_probe`] when the user switches back to
    ///「新建仓」or leaves the existing-repo picker.
    ClearRemoteProjectProbe,
    /// C7 · 采集器 (plan/13 D7): pull real data into the active project's
    /// metrics *right now* — the manual「立即采集」counterpart to the standard
    /// daily collect cron. For every `collect.kind = "github"` metric it runs
    /// a real `gh` count query against the project's remote and appends an
    /// append-only observation *only when the value changed* (change-guard),
    /// then re-derives signals. `bw`/`connector` kinds are v1 未接 — left
    /// blank, their signal stays Unknown (无数据 ≠ 绿). A `gh` failure writes
    /// nothing and surfaces an honest `ConnectorSynced { ok: false, .. }`
    /// toast; the signal degrades on staleness instead of flashing a fake
    /// zero. Never settles or runs anything — collection is observation, not
    /// work.
    CollectMetrics,
    StartSession {
        id: SessionId,
        stage_kind: Option<StageKind>,
        kind: SessionKind,
        title: String,
    },
    /// Run an Issue — the one real "干活" entry: the issue's title/desc + its
    /// stage's role playbook + any distilled (compounded) skills from the same
    /// project are assembled into one prompt and handed to the interactive
    /// executor (embedded-terminal PTY, or the mock when the project has no
    /// workspace). Every run writes a `workflow_run` row bound to the issue,
    /// so the issue's detail answers "which runs/产物 did this produce?". The
    /// issue is pushed `InProgress` at start, `InReview` on success, and left
    /// `InProgress` on failure — **never auto-Done** (Done is a human
    /// `TransitionIssue` only; one work item, one human-confirmed credit).
    RunIssue {
        session: SessionId,
        id: IssueId,
    },
    /// plan/17 S3 (① 中止): abort the in-flight backgrounded run on an Issue
    /// (the desktop kernel's settle channel path). No-op when there's no
    /// in-flight run on that issue (a normal completion already settled, or
    /// the run was inline-only — examples / headless never background). The
    /// issue STAYS `InProgress` (retryable); `settled_at` stays empty —
    /// **never auto-Done** (铁律). Inlined issue runs (no `settle_tx`) have
    /// no `JoinHandle` to abort, so `CancelRun` is a no-op there too (the
    /// inline `await` is already blocking the kernel loop — nothing to
    /// cancel, the run either completes or fails on its own).
    CancelRun {
        id: IssueId,
    },
    CreateWorkflowSpec {
        id: WorkflowId,
        name: String,
        prompt: String,
        goal: String,
        stage_ref: Option<u8>,
        phases: Vec<String>,
        /// Per-phase real instructions (playbook), index-aligned with
        /// `phases`; empty = every phase shares `prompt` (legacy behavior).
        phase_prompts: Vec<String>,
        agents: Vec<AgentRef>,
        skills: Vec<SkillRef>,
        loop_config: LoopConfig,
        maturity: Maturity,
        scope: String,
        source: HubSource,
        trigger: Option<String>,
    },
    /// "优化" an existing **Static** hub workflow — revise its authored
    /// content in place (bumps `version`; `uses`/`maturity`/`source` are
    /// untouched). Distinct from `CreateWorkflowSpec` (a fresh spec).
    UpdateWorkflowSpec {
        id: WorkflowId,
        prompt: String,
        goal: String,
        phases: Vec<String>,
        /// Per-phase instructions (may be empty — dropping back to a single
        /// shared `prompt` is a legal edit).
        phase_prompts: Vec<String>,
        agents: Vec<AgentRef>,
        skills: Vec<SkillRef>,
        /// Why this "优化" happened — frozen onto the version snapshot (iter 5).
        note: String,
    },
    CreateSkill {
        id: SkillId,
        name: String,
        desc: String,
        category: String,
        source: HubSource,
        /// Executable body (may be empty — a catalog reference entry).
        content: String,
    },
    /// Distill a new skill from a completed, assigned Issue — the "every
    /// solution compounds into a reusable skill" link. Provenance + Done/
    /// assignee validation lives in the store; this is a thin wrapper that
    /// delegates and refreshes, like `CreateSkill`. `content` is the distilled
    /// method body itself — a skill minted from real work must be executable
    /// content, not another empty catalog card.
    DistillSkillFromIssue {
        skill_id: SkillId,
        issue_id: IssueId,
        name: String,
        desc: String,
        category: String,
        content: String,
    },
    /// Copy-on-import a real, on-disk skill folder (T2, plan/12 §2):
    /// `source_path` must contain a `SKILL.md` whose frontmatter has
    /// `name`/`description`; every other file underneath lands in
    /// `skill_file` verbatim (real relative paths, no predetermined
    /// category). Once imported, the new skill has zero dependency on
    /// `source_path` — it can move, change, or vanish afterward.
    ///
    /// `official_library` is not part of plan/12 §2's headline
    /// `{ source_path, project_id }` shorthand, but this command still needs
    /// it: without an explicit sub-tag, a generic "import any SKILL.md
    /// folder" command has no honest way to know whether the folder came
    /// from a BW-curated library — inventing "mattpocock-skills" from a path
    /// convention would be the exact kind of guessing this ticket's own
    /// frontmatter-parsing rule forbids. `None` = ad-hoc personal import →
    /// `HubSource::SelfBuilt`; `Some(lib)` → `HubSource::Official {
    /// official_library: lib }`. T3's `ImportSkillLibrary` (batch) threads a
    /// real `Some(..)` through this same field for every package it finds
    /// under a library root.
    ImportSkillPackage {
        source_path: String,
        project_id: Option<ProjectId>,
        official_library: Option<String>,
    },
    /// Batch-import every real skill folder under a library root (T3,
    /// plan/12 §1/§2): finds every directory that directly contains a
    /// `SKILL.md` (`node_modules`/`.git`/`target` pruned without
    /// descending — real libraries don't nest skills inside these, it's
    /// pure efficiency/safety insurance), and each hit goes through the
    /// exact same disk-parsing path `ImportSkillPackage` uses — a batch
    /// import and a hand-run single-package import of the same folder
    /// produce byte-identical rows.
    ///
    /// Idempotent by `(name, official_library)`: a name already imported
    /// from the same `official_library` is skipped, never overwritten —
    /// re-running this (e.g. a library version bump) can't silently clobber
    /// a row a user has since hand-edited (T11 territory: editing flips a
    /// row to `SelfBuilt`, which this check's `official_library` filter
    /// naturally no longer matches, so an edited row is never skipped-away
    /// from re-import consideration by name collision with itself) or
    /// double-insert a duplicate. `official_library` is required (not
    /// `Option`, unlike `ImportSkillPackage`) — a library import is by
    /// definition an official-selection provenance, never an ad-hoc
    /// personal one. Emits `Event::SkillLibraryImported` with the real
    /// imported/skipped tally.
    ImportSkillLibrary {
        root_path: String,
        official_library: String,
        project_id: Option<ProjectId>,
    },
    /// SkillHub's detail-panel edit — content only (`maturity`/`uses` are
    /// lifecycle data, untouched).
    UpdateSkill {
        id: SkillId,
        name: String,
        desc: String,
        category: String,
        content: String,
        /// 五角色归类(2026-08-05)。`None` = 本次编辑不碰归类(保持既有行为,
        /// 让任何不带归类 UI 的调用方原样工作);`Some(v)` = 人工归类,落
        /// `StageOrigin::Manual`,此后 Boot 的静态表回填不再覆盖这件技能。
        /// `Some(vec![])` 是合法且有意义的输入:人工判定「不属任何阶段」。
        stages: Option<Vec<StageKind>>,
    },
    /// plan/20 R5(plan/08 S1 原拍板命名):「引入本项目」= 从共享目录
    /// **复制一份归我**——新 id、`project_id = 目标项目`、描述尾注「引入自
    /// <归属> · <日期>」、uses/战绩清零(新账,本地挣);skill 的支撑文件
    /// (`skill_file`)一并复制;`source` 原样保留(出处保真——Official 库
    /// 文本仍是那个库的原文,T11「编辑即脱离源头」照常生效,收录副本靠
    /// 归属徽记 + 描述尾注辨认)。只认全局行(`project_id IS NULL`):他
    /// 项目的行不属于共享池——想共享先升格全局(promote,留口未建)。
    /// 同池同名(含重复收录同一件)按 R4 拒绝,诚实报错不静默去重。
    AdoptIntoProject {
        target: AdoptTarget,
        project_id: ProjectId,
    },
    CreateAgent {
        id: AgentId,
        name: String,
        role: String,
        skills: Vec<String>,
        model: String,
        /// Standing instructions (may be empty — a catalog reference entry).
        instructions: String,
    },
    /// AgentHub's detail-panel edit — content only (`maturity`/`runs`/
    /// `win_rate` are lifecycle data, untouched).
    UpdateAgent {
        id: AgentId,
        name: String,
        role: String,
        skills: Vec<String>,
        model: String,
        instructions: String,
    },
    /// Import a real, on-disk AGENT.md (T5, plan/12 §3): `source_path` must
    /// be a file whose frontmatter has `name`/`description`, and may have
    /// `tools` (→ AllowedTools)/`model`; the body becomes `instructions`.
    /// Every other frontmatter key is read and silently ignored (same rule
    /// `ImportSkillPackage` follows for SKILL.md). No file-tree concept here
    /// unlike Skill — one AGENT.md is the entire definition, so this maps
    /// straight onto `Store::create_agent`, no new store method needed.
    ///
    /// `official_library`: same shape as `ImportSkillPackage`'s field —
    /// `None` = ad-hoc personal import → `HubSource::SelfBuilt`;
    /// `Some(lib)` → `HubSource::Official { official_library: lib }`. The
    /// 67-file ECC batch import threads `Some("ecc")` through this same
    /// field for every file.
    ///
    /// T11 (2026-07-23, plan/12 §7): unlike `ImportSkillPackage` (which stays
    /// purely additive — dedup is `ImportSkillLibrary`'s job, a separate
    /// batch command), Skill has no standalone "import one AGENT.md" caller
    /// driving a real 67-file batch the way this command does (there is no
    /// `ImportAgentLibrary`; the vendored-ECC example dispatches this command
    /// once per file in a loop). So *this* singular command is where an
    /// `official_library: Some(lib)` re-import's own idempotency has to
    /// live: a name that already exists under `lib` (still `Official`, or
    /// hand-edited and flipped to `SelfBuilt` — see `AgentCard::adapted_from`)
    /// is silently skipped, never overwritten or duplicated. An ad-hoc
    /// `None` import stays purely additive (no batch-reimport concept for a
    /// personal one-off), matching `ImportSkillPackage`'s own rule.
    ImportAgentDefinition {
        source_path: String,
        official_library: Option<String>,
    },
    /// A1: an autopilot cron task — when due, it mints a stage-scoped Issue
    /// (Todo, optionally assigned) instead of running anything. No-hijack: it
    /// never auto-runs anything. `assignee` is an agent NAME matched at fire
    /// time (no match ⇒ honest unassigned Issue, not a failure). `stage: None`
    /// = mint into whatever stage the project is in at fire time.
    ///
    /// 2026-08-18:这是 Cron Hub 表单唯一能建的类型。「采集指标」定时器由
    /// `CreateProject`(挂了远端仓的项目)自动配一条,不走表单;旧的
    /// 「运行工作流 / 运行技能 / 运行 Prompt」三种到点跑旧聊天式引擎的模式
    /// 随引擎一起拔掉。
    CreateAutopilotTask {
        id: CronTaskId,
        name: String,
        schedule: Cadence,
        project_id: Option<ProjectId>,
        stage: Option<StageKind>,
        assignee: Option<String>,
    },
    /// Pause/resume a cron task — the "人工介入" lever. Pure status flip;
    /// never touches `last_run` since nothing actually ran.
    SetCronStatus {
        id: CronTaskId,
        status: CronStatus,
    },
    CreateConnector {
        id: ConnectorId,
        name: String,
        kind: String,
        scope: String,
        /// Project this connector feeds (`git-repo` is always bound).
        project_id: Option<ProjectId>,
        /// Kind-specific real config (`git-repo`: workspace path;
        /// `claude-cli`: binary override, empty = PATH).
        config: String,
    },
    CreateKnowledgeSource {
        id: KnowledgeSourceId,
        name: String,
        kind: String,
        used_by: String,
    },
    /// Create a new issue in the active project (defaults to `Backlog`,
    /// auto-assigned per-project number). Scoped to the given stage.
    ///
    /// P3 (loop-buddy↔aihot spec): `standard_skill` names a skill-library
    /// slug to carry on the issue from the moment it's created — the same
    /// field `seed_standard_issue_trio` sets for the three creation-flow
    /// standard cards, now reachable from this manual entry too (the
    /// op-panel create strip; `autopilot_fire`'s cron-minted issues still
    /// call `store.create_issue` directly and are untouched by this field).
    /// Empty (every pre-existing call site) means "no method chosen",
    /// byte-identical to today's behavior. Resolution is honest-by-name at
    /// run time via `standard_skill_block` — an unknown or content-less
    /// slug never fails issue creation, it just injects nothing.
    CreateIssue {
        id: IssueId,
        stage: StageKind,
        title: String,
        desc: String,
        priority: IssuePriority,
        standard_skill: String,
    },
    /// Move an issue to a new kanban status (the kanban lifecycle transition).
    TransitionIssue {
        id: IssueId,
        status: IssueStatus,
    },
    /// C5 · PR 验收环 (plan/13 D3): the **human验收** entry point for an Issue
    /// whose run opened a PR — merge the PR, which (via `Closes #<n>`) closes
    /// the GitHub issue, then settle the Issue Done through the *existing*
    /// `TransitionIssue` InReview→Done accounting path (settle-once reused, no
    /// second accounting path). The executor never merges — this command is the
    /// only place `gh pr merge` is ever called. Idempotent: a re-dispatch after
    /// the Issue is already Done is a no-op that never re-merges or re-accounts.
    /// Issues with no PR (no-repo/存量) keep using bare `TransitionIssue` to
    /// Done — 全活 PR 化是纪律不是硬闸 (只留痕不拦人).
    MergeIssuePr {
        id: IssueId,
    },
    /// Assign (or, with `None`, unassign) an issue to an agent teammate.
    AssignIssue {
        id: IssueId,
        assignee: Option<AgentId>,
    },
    /// A5-F: the only path into `Blocked` — `reason` must be non-empty.
    /// `TransitionIssue { status: Blocked }` is rejected; this is how a stuck
    /// issue leaves a record of *why*, not just *that*.
    BlockIssue {
        id: IssueId,
        reason: String,
    },
    OpenProject(ProjectId),
    /// Delete a project and everything scoped to it. The CRUD-completeness
    /// counterpart to `CreateProject` — irreversible; the UI is responsible
    /// for confirming with the user before dispatching this.
    DeleteProject(ProjectId),
    /// 硬删一条阶段记录（session 行；message 表已随旧引擎删除）。issue 行留下；
    /// claude_conversation 不删，看板仍可按现设计唤醒。
    DeleteSession(SessionId),
    /// 项目墙「测一下」：真跑 claude --version 与 codehub-cli 探活。
    /// 未测过是 Unknown，不装绿。
    ProbeLocalEnv,
    BackToProjects,
    SetPanel(Panel),
    SetScope(Scope),
    /// Select (or clear) the chat-focused session in the operating view.
    SelectSession(Option<SessionId>),
    /// V1 Issue2 Phase2b: user-typed bytes from the in-app xterm.js terminal
    /// (`onData` callback). Forwarded to TerminalManager by conversation_id.
    TerminalInput {
        conversation_id: ConversationId,
        bytes: Vec<u8>,
    },
    /// V1 Issue2 Phase2b: terminal resize from the in-app xterm.js
    /// (`onResize` callback). Forwarded to TerminalManager by conversation_id.
    TerminalResize {
        conversation_id: ConversationId,
        cols: u16,
        rows: u16,
    },
}

/// 项目墙本机环境探测。未点「测一下」= Unknown，不装绿。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum EnvCheck {
    #[default]
    Unknown,
    Probing,
    Ok(String),
    Fail(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalEnvProbe {
    pub claude: EnvCheck,
    pub codehub: EnvCheck,
}

/// Result of [`Command::ProbeRemoteProjectToml`] — creation-flow UI state
/// only (not persisted). Final later-comer gate after confirm is still the
/// on-disk `.bw/project.toml` post-clone.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum RemoteProjectProbe {
    #[default]
    Idle,
    Probing,
    /// Remote has no `.bw/project.toml` → first-comer Intent (editable).
    Absent,
    /// Parsed正本 → later-comer Intent (readonly prefill).
    Present(ProjectFile),
    /// Network/auth/parse failed — stay editable; confirm still uses clone gate.
    Failed(String),
}

/// Kernel → UI facts (already happened).
#[derive(Clone, Debug)]
pub enum Event {
    ProjectsChanged,
    ProjectUpdated(ProjectId),
    ViewChanged(View),
    /// An Issue run failed or was aborted — the human-readable reason. The
    /// desktop shows it as a toast; the Issue itself stays where it was
    /// (InProgress, retryable — never faked forward).
    RunFailed(String),
    StageHandoff {
        from: StageKind,
        to: StageKind,
        risky: bool,
    },
    WorkflowSpecsChanged,
    SkillsChanged,
    /// A batch `Command::ImportSkillLibrary` just finished — the real tally,
    /// not an assumption: `imported` = new rows this run actually inserted,
    /// `skipped` = `(name, official_library)` matches that already existed
    /// and were left untouched (idempotent re-run safety).
    SkillLibraryImported {
        official_library: String,
        imported: u32,
        skipped: u32,
    },
    AgentsChanged,
    CronTasksChanged,
    /// A real, unattended auto-fire from `App::tick_scheduler` just finished
    /// (not a manual "▶ 立即执行") — the live "monitoring" signal for the
    /// scheduler: a subscriber can toast/notify without the run having
    /// stolen `active_project`/the user's current screen to get there.
    CronAutoFired {
        id: CronTaskId,
        name: String,
        ok: bool,
    },
    ConnectorsChanged,
    /// A connector's real probe just finished — `detail` is the probe's
    /// honest summary (e.g. "3 提交 · 12 文件" or the error text).
    ConnectorSynced {
        name: String,
        ok: bool,
        detail: String,
    },
    /// plan/14 C14 · a creation-flow background action (repo create/clone,
    /// repo-list fetch, standard-Issue mint, landing push) really started or
    /// finished — the "is this real or stuck" visibility the creation flow's
    /// slow `gh`-backed calls lacked (体验规范条 2:后台动作永远有状态回显).
    /// Emitted as a Started → Ok/Fail pair around each real network call,
    /// `name` stable across the pair so a subscriber can correlate them.
    /// Purely additive visibility, never a gate — 乐观推进哲学不变
    /// (CLAUDE.md): the card the user sees has already advanced before this
    /// fires, same as the pre-existing `ConnectorSynced` toasts these sit
    /// alongside (kept unchanged — this doesn't replace them).
    ActionProgress {
        name: String,
        state: ActionState,
    },
    KnowledgeSourcesChanged,
    IssuesChanged,
    ActivityChanged,
    ClaudeConfigChanged,
    VersionLogChanged,
    /// New artifact versions were registered (post-run auto-scan or a manual
    /// `CollectArtifacts`). Carries the honest count of *genuinely new* rows.
    ArtifactsRegistered {
        fresh: u32,
    },
    /// The `AppState.artifacts` snapshot was (re)loaded.
    ArtifactsChanged,
}

/// Three-state visibility for one `Event::ActionProgress` — see that
/// variant's doc comment. `bw-app`-local (not `bw-core`): this is UI-facing
/// transient signal, not domain state, so it never touches the wasm-clean
/// kernel crates.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionState {
    /// The real call just started.
    Started,
    /// The real call really succeeded — a short honest summary, never
    /// invented (e.g. the repo slug `gh` actually minted).
    Ok(String),
    /// The real call really failed — the real error text.
    Fail(String),
}
