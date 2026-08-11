//! `issue_kickoff` — 切片七C 收官验收指挥器(design-s7-real-project.md
//! §1.1/§1.2/§6.1)。
//!
//! 不开界面,直接调用 [`bw_app::cmd::issue::run_issue`]/[`bw_app::cmd::
//! issue::cancel_run`]——开工用例第一次被真实调用的地方:壳(切片七D)与
//! 这份指挥器调的是**同一段代码**,不是两份平行实现。**mock 档全程不依
//! 赖网关**(执行连接器是脚本化 mock,自我标注),但工作树是**真实** git
//! 检出——`bw_workspace::provision::provision_issue_worktree` 函数体不
//! 碰,这里真的会在临时目录里跑 `git worktree add`,验的是它真的被开工
//! 用例调用到,不是重新证明它自己已经证过的东西(那是
//! `provision_readback.rs` 的事)。
//!
//! 八段:K1(项目没配检出根,如实拒绝,一条运行行都没有)/ K2(配了检出
//! 根,工作树真的造出来 + 运行行落库 + 路径/分支吻合 + 活推进行中,结
//! 束后依旧进行中)/ K2b(`BW_WORKSPACES` 深链变量的第一个真实消费者——
//! 相对 `root_path` 接 `workspaces_root` 解析成绝对路径)/ K3(同一件活
//! 再点一次开工,如实拒绝并给出已有运行编号)/ K4(连接器零条/多条,如
//! 实报,不取第一条)/ K5(取消,结束事实=取消,结账一次;再取消,幂
//! 等不改字段)/ K7(事件广播真的收到,订阅端断言)/ K8(晚到消息不复
//! 活,复用 `run_races.rs` R3 的抖动扫窗形状,规模缩小、轮询节奏调快)。
//!
//! 跑法:
//! ```text
//! cargo run -p bw-app --example issue_kickoff
//! ```
//! 退出码 0 且末行 `ISSUE_KICKOFF_OK` = 全部断言通过;任何一条失败 →
//! stderr 打 `ASSERT FAILED: …`,末行 `ISSUE_KICKOFF_FAILED`,退出码 1。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bw_app::cmd::issue::{cancel_run, run_issue};
use bw_app::run::{RunManager, RunManagerConfig, RunStateChanged};
use bw_app::AppError;
use bw_connector::{
    guarded, CallCtx, ConfigRef, ConnError, ConnResult, Connector, ConnectorEntry, ConnectorKind,
    ConnectorRegistry, ExecSpec, ExecState, ExecTicket, Execute, OpClass, ProjectBinding,
    RequestId, SessionEnd,
};
use bw_core::{ConnectorId, IssueId, IssueStatus, ProjectId, RunId, RunState};
use bw_store::{IssueStore, NewIssue, NewProject, RunEndKind, RunStore, SqliteStore, StoreError};
use tokio::sync::broadcast;
use uuid::Uuid;

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

const DB_NAME_PREFIX: &str = "bw-issue-kickoff-";
const CONNECTOR_NAME: &str = "kickoff-mock";
const CONNECTOR_NAME_X: &str = "kickoff-mock-x";
const CONNECTOR_NAME_Y: &str = "kickoff-mock-y";

/// K8 用的抖动扫窗参数——同 `run_races.rs` R3 一条形状(`R3_FINISH_MS`/
/// `R3_JITTER_MS`),数值调小、轮数调少:这里的目的是证明「晚到消息不复
/// 活」在 `run_issue`/`cancel_run` 这一层没有被破坏,不是重新证明运行管
/// 理器自己的比较并置机制(那是 `run_races.rs` R3/R6 的既有覆盖)。
const K8_FINISH_MS: Duration = Duration::from_millis(60);
const K8_JITTER_MS: i64 = 25;
const K8_ROUNDS: usize = 10;

fn clear_stale_dbs(current: &Path) {
    if current.exists() {
        let _ = std::fs::remove_file(current);
    }
    let dir = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let current_name = current.file_name().map(|n| n.to_os_string());
    for entry in entries.flatten() {
        let name = entry.file_name();
        if current_name.as_deref() == Some(name.as_os_str()) {
            continue;
        }
        if name.to_string_lossy().starts_with(DB_NAME_PREFIX) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

fn fresh_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("{DB_NAME_PREFIX}{}.db", Uuid::new_v4()))
}

// ─────────────────────────── 断言账本(同 run_races.rs 既有形状)───────

struct Ledger {
    ok: bool,
    count: usize,
}

impl Ledger {
    fn new() -> Self {
        Self { ok: true, count: 0 }
    }
    fn check(&mut self, cond: bool, label: impl std::fmt::Display) {
        self.count += 1;
        if cond {
            println!("  ✓ {label}");
        } else {
            eprintln!("ASSERT FAILED: {label}");
            self.ok = false;
        }
    }
    fn fail(&mut self, label: impl std::fmt::Display) {
        self.check(false, label);
    }
    fn merge(&mut self, other: Ledger) {
        self.ok &= other.ok;
        self.count += other.count;
    }
}

// ─────────────────────── 脚本化 mock 执行连接器 ────────────────────────
// 同 `run_races.rs` `ScriptedExec` 一条形状(design §4.1「住在指挥器里,
// 不进任何 crate 的正式代码」);这里按**分支**登记脚本(键与
// `ScriptedExec` 相同),但用 `Mutex` 包起来允许在跑的过程中陆续登记——
// `run_issue` 自己算分支名(`bw_workspace::provision::issue_branch`,由活
// 编号推出),指挥器造活之前拿不到分支名,只能边造边registerScript,不
// 能像 `run_races.rs` 那样一次性 `build_scripts()`。

/// 只登记这一份指挥器真的用得到的结束方式——**不是** `SessionEnd` 的完
/// 整镜像(同 `run_races.rs` `EndKind` 一条既有先例:那边也只挑用得到的
/// 两档,这里连 `ContactLost` 都用不上,只留一档)。
#[derive(Clone, Copy)]
enum EndKind {
    ProcessExit,
}

#[derive(Clone)]
struct Script {
    start_fails: Option<String>,
    finish_after: Option<(Duration, EndKind)>,
}

impl Script {
    fn never_finishes() -> Self {
        Self {
            start_fails: None,
            finish_after: None,
        }
    }
    fn finishes_after(dur: Duration) -> Self {
        Self {
            start_fails: None,
            finish_after: Some((dur, EndKind::ProcessExit)),
        }
    }
}

struct Live {
    started_at: Instant,
    canceled_at: Option<Instant>,
    finish_after: Option<(Duration, EndKind)>,
}

/// 【mock】脚本化执行连接器。
struct KickoffExec {
    kind: ConnectorKind,
    binding: ProjectBinding,
    scripts: Mutex<HashMap<String, Script>>,
    live: Mutex<HashMap<RequestId, Live>>,
    starts_issued: AtomicUsize,
}

impl KickoffExec {
    fn new(binding: ProjectBinding) -> Self {
        Self {
            kind: ConnectorKind::AgentCli {
                cli: "kickoff-mock(issue_kickoff 指挥器自建,不进任何 crate 正式代码)".to_string(),
            },
            binding,
            scripts: Mutex::new(HashMap::new()),
            live: Mutex::new(HashMap::new()),
            starts_issued: AtomicUsize::new(0),
        }
    }

    /// 登记「这条分支该按什么脚本走」——调用方在造活之前用
    /// `bw_workspace::provision::issue_branch(number)` 算出同一个键。
    fn set_script(&self, branch: impl Into<String>, script: Script) {
        self.scripts
            .lock()
            .expect("mock scripts mutex poisoned")
            .insert(branch.into(), script);
    }
}

impl Connector for KickoffExec {
    fn kind(&self) -> &ConnectorKind {
        &self.kind
    }
    fn binding(&self) -> &ProjectBinding {
        &self.binding
    }
    fn as_execute(&self) -> Option<&dyn Execute> {
        Some(self)
    }
}

#[async_trait]
impl Execute for KickoffExec {
    async fn start(&self, cx: &CallCtx, spec: ExecSpec) -> ConnResult<ExecTicket> {
        let req = cx.req;
        let branch = spec.branch;
        guarded(cx, OpClass::Write, async move {
            let Some(script) = self
                .scripts
                .lock()
                .expect("mock scripts mutex poisoned")
                .get(&branch)
                .cloned()
            else {
                return Err(ConnError::Other(format!(
                    "issue_kickoff 脚本化连接器:分支 {branch:?} 没有登记脚本(指挥器装配缺口,不是用例本身)"
                )));
            };
            if let Some(msg) = &script.start_fails {
                return Err(ConnError::NotConnected(msg.clone()));
            }
            let finish_after = script.finish_after;
            self.starts_issued.fetch_add(1, Ordering::SeqCst);
            self.live.lock().expect("mock live mutex poisoned").insert(
                req,
                Live {
                    started_at: Instant::now(),
                    canceled_at: None,
                    finish_after,
                },
            );
            Ok(ExecTicket {
                req,
                upstream_session: Some(format!("mock-session-{req}")),
            })
        })
        .await
    }

    async fn poll(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<ExecState> {
        let req = t.req;
        guarded(cx, OpClass::Read, async move {
            let live = self.live.lock().expect("mock live mutex poisoned");
            let Some(l) = live.get(&req) else {
                return Ok(ExecState::Unknown);
            };
            if l.canceled_at.is_some() {
                return Ok(ExecState::Canceled);
            }
            match l.finish_after {
                None => Ok(ExecState::Running),
                Some((dur, EndKind::ProcessExit)) => {
                    if l.started_at.elapsed() >= dur {
                        let ended = SessionEnd::ProcessExit { code: Some(0) };
                        Ok(ExecState::Finished {
                            ended,
                            summary: "【mock】issue_kickoff 脚本化执行连接器:到点结束".to_string(),
                        })
                    } else {
                        Ok(ExecState::Running)
                    }
                }
            }
        })
        .await
    }

    async fn cancel(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<()> {
        let req = t.req;
        guarded(cx, OpClass::Write, async move {
            if let Some(l) = self
                .live
                .lock()
                .expect("mock live mutex poisoned")
                .get_mut(&req)
            {
                l.canceled_at = Some(Instant::now());
            }
            Ok(())
        })
        .await
    }
}

// ─────────────────────────── 真实 git 检出的造数 ────────────────────────
// 只在这份指挥器自己的临时目录里跑,不碰用户的真实工作区(核心纪律「真
// 实日常库只用副本读回」的同一条精神——这里连副本都不是,是从零造的临
// 时目录)。

fn run_git(dir: &Path, args: &[&str]) -> bool {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// 造一个真实的、带一次提交的 git 主检出——`provision_issue_worktree`
/// 要求「不是 git 仓库」时如实失败,所以这里必须是真的仓库,不能只造一
/// 个空目录。
fn init_main_checkout(dir: &Path) -> bool {
    std::fs::create_dir_all(dir).is_ok()
        && run_git(dir, &["init", "-q", "-b", "main"])
        && run_git(dir, &["config", "user.email", "issue-kickoff@bw.local"])
        && run_git(dir, &["config", "user.name", "issue_kickoff"])
        && std::fs::write(dir.join("README.md"), "issue_kickoff 指挥器临时主检出\n").is_ok()
        && run_git(dir, &["add", "."])
        && run_git(dir, &["commit", "-q", "-m", "init"])
}

// ─────────────────────────── 指挥器上下文 ───────────────────────────

struct Ctx {
    control: SqliteStore,
    manager: RunManager,
    workspaces_root: PathBuf,
    /// 按项目分配的下一个活编号——`(project_id, number)` 唯一,调用方分
    /// 配(design §2.2⑤ 同一条约束的 mock 造数版)。用 `std::sync::Mutex`
    /// 是因为指挥器全程串行跑各段,不需要 tokio 原生锁,同步锁足够。
    issue_no: Mutex<HashMap<ProjectId, i64>>,
}

impl Ctx {
    async fn new_issue(
        &self,
        project: ProjectId,
        title: &str,
    ) -> Result<(IssueId, i64), StoreError> {
        let id = IssueId::new();
        let number = {
            let mut map = self.issue_no.lock().expect("issue_no mutex poisoned");
            let n = map.entry(project).or_insert(1);
            let cur = *n;
            *n += 1;
            cur
        };
        self.control
            .create_issue(NewIssue {
                id,
                project_id: project,
                number,
                title: title.to_string(),
            })
            .await?;
        Ok((id, number))
    }
}

async fn new_project(control: &SqliteStore, name: &str, root_path: &str) -> ProjectId {
    let id = ProjectId::new();
    control
        .create_project(NewProject {
            id,
            name: name.to_string(),
            root_path: root_path.to_string(),
        })
        .await
        .unwrap_or_else(|e| panic!("create_project({name:?}) 失败:{e}"));
    id
}

/// 等一条运行关门(`ended_at` 非空),给够上限但不空等——同
/// `run_races.rs`/`bw-engine` `agent_session.rs` 既有的 `poll_until` 手法。
async fn wait_for_closed(
    control: &SqliteStore,
    id: RunId,
    deadline: Duration,
) -> Option<bw_store::RunRow> {
    let start = Instant::now();
    loop {
        if let Ok(Some(row)) = control.get_run(id).await {
            if row.ended_at.is_some() {
                return Some(row);
            }
        }
        if start.elapsed() >= deadline {
            return control.get_run(id).await.ok().flatten();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let db_path = fresh_db_path();
    clear_stale_dbs(&db_path);
    let scratch = std::env::temp_dir().join(format!("bw-issue-kickoff-ws-{}", Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&scratch);

    println!("== issue_kickoff · 切片七C 开工闭环验收指挥器(mock 档)==");
    println!("本次数据库:{}   ← 跑完不删,给人手工复核", db_path.display());
    println!(
        "本次工作区临时根:{}   ← 跑完不删,含真实 git 工作树",
        scratch.display()
    );
    println!("执行连接器:【mock】脚本化(自我标注);工作树是真实 git 检出");
    println!();

    let control = match SqliteStore::open(&db_path.to_string_lossy()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ASSERT FAILED: 开库失败:{e}");
            eprintln!("ISSUE_KICKOFF_FAILED");
            return ExitCode::FAILURE;
        }
    };

    // ── 项目们 ──────────────────────────────────────────────────────
    let project_no_root = new_project(&control, "K1 · 无检出根项目(合成)", "").await;

    let main_a = scratch.join("project-a");
    if !init_main_checkout(&main_a) {
        eprintln!("ASSERT FAILED: 造 project-a 的真实主检出失败(检查本机是否装了 git)");
        eprintln!("ISSUE_KICKOFF_FAILED");
        return ExitCode::FAILURE;
    }
    let project_a = new_project(
        &control,
        "K2/K3/K5/K7/K8 · 已配检出根项目(合成)",
        &main_a.display().to_string(),
    )
    .await;

    // K2b:相对 root_path,真实主检出直接造在 `workspaces_root`(=scratch)
    // 下面——`resolve_main_workspace` 应该把它接成
    // `workspaces_root.join("project-rel")`。
    let rel_component = "project-rel";
    let main_rel = scratch.join(rel_component);
    if !init_main_checkout(&main_rel) {
        eprintln!("ASSERT FAILED: 造 project-rel 的真实主检出失败");
        eprintln!("ISSUE_KICKOFF_FAILED");
        return ExitCode::FAILURE;
    }
    let project_rel = new_project(&control, "K2b · 相对检出根项目(合成)", rel_component).await;

    let main_zero = scratch.join("project-zero");
    if !init_main_checkout(&main_zero) {
        eprintln!("ASSERT FAILED: 造 project-zero 的真实主检出失败");
        eprintln!("ISSUE_KICKOFF_FAILED");
        return ExitCode::FAILURE;
    }
    let project_zero = new_project(
        &control,
        "K4a · 零连接器项目(合成)",
        &main_zero.display().to_string(),
    )
    .await;

    let main_multi = scratch.join("project-multi");
    if !init_main_checkout(&main_multi) {
        eprintln!("ASSERT FAILED: 造 project-multi 的真实主检出失败");
        eprintln!("ISSUE_KICKOFF_FAILED");
        return ExitCode::FAILURE;
    }
    let project_multi = new_project(
        &control,
        "K4b · 多连接器项目(合成)",
        &main_multi.display().to_string(),
    )
    .await;

    // ── 注册表:每个需要连接器的项目各自装配(design §1.3「壳启动时按项
    // 目装配」的 mock 版——这里由指挥器扮演壳的装配角色)──────────────
    let mock_a = Arc::new(KickoffExec::new(ProjectBinding {
        project: project_a,
        host: String::new(),
        path: main_a.display().to_string(),
    }));
    let mock_rel = Arc::new(KickoffExec::new(ProjectBinding {
        project: project_rel,
        host: String::new(),
        path: rel_component.to_string(),
    }));
    let mock_multi_x = Arc::new(KickoffExec::new(ProjectBinding {
        project: project_multi,
        host: String::new(),
        path: main_multi.display().to_string(),
    }));
    let mock_multi_y = Arc::new(KickoffExec::new(ProjectBinding {
        project: project_multi,
        host: String::new(),
        path: main_multi.display().to_string(),
    }));

    let mut registry = ConnectorRegistry::default();
    registry.register(
        ConnectorEntry {
            id: ConnectorId::new(),
            name: CONNECTOR_NAME.to_string(),
            kind: mock_a.kind.clone(),
            binding: mock_a.binding.clone(),
            config: ConfigRef::AgentRegistryRow {
                slug: "kickoff-mock".to_string(),
            },
        },
        mock_a.clone(),
    );
    registry.register(
        ConnectorEntry {
            id: ConnectorId::new(),
            name: CONNECTOR_NAME.to_string(),
            kind: mock_rel.kind.clone(),
            binding: mock_rel.binding.clone(),
            config: ConfigRef::AgentRegistryRow {
                slug: "kickoff-mock".to_string(),
            },
        },
        mock_rel.clone(),
    );
    // project_zero:刻意不注册任何连接器(K4a)。
    registry.register(
        ConnectorEntry {
            id: ConnectorId::new(),
            name: CONNECTOR_NAME_X.to_string(),
            kind: mock_multi_x.kind.clone(),
            binding: mock_multi_x.binding.clone(),
            config: ConfigRef::AgentRegistryRow {
                slug: "kickoff-mock-x".to_string(),
            },
        },
        mock_multi_x.clone(),
    );
    registry.register(
        ConnectorEntry {
            id: ConnectorId::new(),
            name: CONNECTOR_NAME_Y.to_string(),
            kind: mock_multi_y.kind.clone(),
            binding: mock_multi_y.binding.clone(),
            config: ConfigRef::AgentRegistryRow {
                slug: "kickoff-mock-y".to_string(),
            },
        },
        mock_multi_y.clone(),
    );
    let registry = Arc::new(registry);

    // 轮询节奏调快(30ms,默认是 250ms):这份指挥器是关于「开工用例」本
    // 身的行为验收,不是重新验证默认轮询节奏(那是 `run_races.rs` 的
    // 事)——调快只是让 K2/K7/K8 跑得快一点,不改变任何断言的判据。
    let manager = match RunManager::open(
        &db_path,
        registry,
        RunManagerConfig {
            poll_interval: Duration::from_millis(30),
        },
    )
    .await
    {
        Ok(m) => m,
        Err(e) => {
            eprintln!("ASSERT FAILED: RunManager::open 失败:{e}");
            eprintln!("ISSUE_KICKOFF_FAILED");
            return ExitCode::FAILURE;
        }
    };

    let ctx = Ctx {
        control,
        manager,
        workspaces_root: scratch.clone(),
        issue_no: Mutex::new(HashMap::new()),
    };

    let mut total = Ledger::new();

    println!("== K1 · 项目没配检出根 → 如实拒绝,一条运行行都没有 ==");
    total.merge(section_k1(&ctx, project_no_root).await);
    println!();

    println!("== K2 · 配了检出根 → 工作树真的造出来 + 运行行落库 + 路径/分支吻合 + 活推进行中(结束后依旧进行中)==");
    total.merge(section_k2(&ctx, project_a, &main_a, &mock_a).await);
    println!();

    println!("== K2b · BW_WORKSPACES 的第一个真实消费者:相对 root_path 接 workspaces_root 解析 ==");
    total.merge(section_k2b(&ctx, project_rel, &main_rel, &mock_rel).await);
    println!();

    println!("== K3 · 同一件活再点一次开工 → 如实拒绝,给出已有运行编号;运行行仍只有一条 ==");
    total.merge(section_k3(&ctx, project_a, &mock_a).await);
    println!();

    println!("== K4 · 连接器零条/多条 → 如实报,不取第一条 ==");
    total.merge(section_k4(&ctx, project_zero, project_multi).await);
    println!();

    println!("== K5 · 取消 → 结束事实=取消,结账一次;再取消 → 幂等不改字段 ==");
    total.merge(section_k5(&ctx, project_a, &mock_a).await);
    println!();

    println!("== K7 · 事件广播真的收到(订阅端断言)==");
    total.merge(section_k7(&ctx, project_a, &mock_a).await);
    println!();

    println!("== K8 · 晚到消息不复活(复用 run_races.rs R3 的抖动扫窗形状,规模缩小)==");
    total.merge(section_k8(&ctx, project_a, &mock_a).await);
    println!();

    println!(
        "断言 {} 条,{}",
        total.count,
        if total.ok { "全过" } else { "有失败" }
    );
    if total.ok {
        println!("ISSUE_KICKOFF_OK");
        ExitCode::SUCCESS
    } else {
        eprintln!("ISSUE_KICKOFF_FAILED");
        ExitCode::FAILURE
    }
}

// ─────────────────────────────── K1 ───────────────────────────────

async fn section_k1(ctx: &Ctx, project_no_root: ProjectId) -> Ledger {
    let mut l = Ledger::new();
    let (issue, _n) = match ctx.new_issue(project_no_root, "K1 示例活").await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K1 建活失败:{e}"));
            return l;
        }
    };
    let result = run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue,
        None,
    )
    .await;
    match result {
        Err(AppError::ProjectNoCheckoutRoot(p)) => {
            l.check(
                p == project_no_root,
                "K1:拒绝原因是 ProjectNoCheckoutRoot,项目编号对得上",
            );
        }
        other => {
            l.fail(format!(
                "K1:应该被 ProjectNoCheckoutRoot 拒绝,实得 {other:?}"
            ));
        }
    }
    let runs = ctx
        .control
        .list_runs(Some(project_no_root))
        .await
        .unwrap_or_default();
    l.check(
        runs.is_empty(),
        format!("K1:一条运行行都没有(实得 {} 条)", runs.len()),
    );
    l
}

// ─────────────────────────────── K2 ───────────────────────────────

async fn section_k2(
    ctx: &Ctx,
    project: ProjectId,
    main_workspace: &Path,
    mock: &Arc<KickoffExec>,
) -> Ledger {
    let mut l = Ledger::new();
    let (issue, number) = match ctx.new_issue(project, "K2 示例活").await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K2 建活失败:{e}"));
            return l;
        }
    };
    let branch = bw_workspace::provision::issue_branch(number as u32);
    mock.set_script(
        branch.clone(),
        Script::finishes_after(Duration::from_millis(50)),
    );

    let run_id = match run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue,
        None,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("K2:run_issue 应该成功,实得 {e}"));
            return l;
        }
    };

    // 工作树真的造出来:目录在、`.git` worktree 标记在、分支切对了。
    let stem = main_workspace
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("workspace");
    let expected_worktree = main_workspace
        .parent()
        .map(|p| p.join(format!("{stem}-issue-{number}")))
        .unwrap_or_default();
    l.check(
        expected_worktree.is_dir(),
        format!("K2:工作树目录存在:{}", expected_worktree.display()),
    );
    l.check(
        expected_worktree.join(".git").exists(),
        "K2:工作树带 .git worktree 标记",
    );
    let current_branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(&expected_worktree)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string());
    l.check(
        current_branch.as_deref() == Some(branch.as_str()),
        format!("K2:工作树分支 = {branch:?}(实得 {current_branch:?})"),
    );

    // 运行行落库,工作区路径与分支名与工作树一致。
    let row = ctx.control.get_run(run_id).await.ok().flatten();
    let Some(row) = row else {
        l.fail("K2:运行行应该落库");
        return l;
    };
    l.check(
        row.workspace == expected_worktree.to_string_lossy(),
        "K2:运行行的 workspace 与真实工作树路径一致",
    );
    l.check(row.branch == branch, "K2:运行行的 branch 与真实分支一致");
    l.check(row.issue_id == issue, "K2:运行行挂在这件活上");

    // 活推到「进行中」。
    let issue_row = ctx.control.get_issue(issue).await.ok().flatten();
    l.check(
        issue_row.as_ref().map(|r| r.status) == Some(IssueStatus::InProgress),
        format!(
            "K2:活状态推到进行中(实得 {:?})",
            issue_row.map(|r| r.status)
        ),
    );

    // 结束(脚本 50ms 后自然结束)后,依旧「进行中」——结束不等于干成
    // (design §1.5 第三行「活的状态一个字不改」)。
    let closed = wait_for_closed(&ctx.control, run_id, Duration::from_millis(800)).await;
    match closed {
        Some(row) if row.ended_at.is_some() => {
            l.check(
                row.state == RunState::Finished,
                format!("K2:运行正常结束(实得 {:?})", row.state),
            );
            l.check(
                row.settled_at.is_some(),
                "K2:结束后结过账一次(settled_at 非空)",
            );
        }
        _ => l.fail("K2:运行应该在 800ms 内关门"),
    }
    let issue_row_after = ctx.control.get_issue(issue).await.ok().flatten();
    l.check(
        issue_row_after.as_ref().map(|r| r.status) == Some(IssueStatus::InProgress),
        format!(
            "K2:结束之后活状态依旧进行中,不假装干成了(实得 {:?})",
            issue_row_after.map(|r| r.status)
        ),
    );
    l
}

// ─────────────────────────────── K2b ───────────────────────────────

async fn section_k2b(
    ctx: &Ctx,
    project: ProjectId,
    main_workspace: &Path,
    mock: &Arc<KickoffExec>,
) -> Ledger {
    let mut l = Ledger::new();
    let (issue, number) = match ctx.new_issue(project, "K2b 示例活(相对 root_path)").await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K2b 建活失败:{e}"));
            return l;
        }
    };
    let branch = bw_workspace::provision::issue_branch(number as u32);
    mock.set_script(branch, Script::finishes_after(Duration::from_millis(30)));

    let run_id = match run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue,
        None,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!(
                "K2b:相对 root_path 应该接 workspaces_root 解析成功,实得 {e}"
            ));
            return l;
        }
    };
    let row = ctx.control.get_run(run_id).await.ok().flatten();
    let expected_prefix = main_workspace.to_string_lossy().to_string();
    l.check(
        row.as_ref().map(|r| r.workspace.starts_with(&expected_prefix)).unwrap_or(false),
        format!(
            "K2b:工作区路径以 workspaces_root/相对 root_path 为前缀(期望前缀 {expected_prefix:?},实得 {:?})",
            row.map(|r| r.workspace)
        ),
    );
    l
}

// ─────────────────────────────── K3 ───────────────────────────────

async fn section_k3(ctx: &Ctx, project: ProjectId, mock: &Arc<KickoffExec>) -> Ledger {
    let mut l = Ledger::new();
    let (issue, number) = match ctx.new_issue(project, "K3 示例活(常驻不结束)").await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K3 建活失败:{e}"));
            return l;
        }
    };
    let branch = bw_workspace::provision::issue_branch(number as u32);
    mock.set_script(branch, Script::never_finishes());

    let first = run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue,
        None,
    )
    .await;
    let run_id = match first {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("K3:第一次开工应该成功,实得 {e}"));
            return l;
        }
    };

    let second = run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue,
        None,
    )
    .await;
    match second {
        Err(AppError::Run(bw_app::run::RunError::AlreadyRunning { existing })) => {
            l.check(
                existing == run_id,
                "K3:第二次开工拒绝,给出的已有运行编号与第一次一致",
            );
        }
        other => l.fail(format!(
            "K3:第二次开工应该被 AlreadyRunning 拒绝,实得 {other:?}"
        )),
    }

    let runs = ctx
        .control
        .list_runs(Some(project))
        .await
        .unwrap_or_default();
    let for_issue = runs.iter().filter(|r| r.issue_id == issue).count();
    l.check(
        for_issue == 1,
        format!("K3:这件活的运行行仍只有一条(实得 {for_issue} 条)"),
    );

    // 清场,不留一条常驻运行拖慢/干扰后续段落。
    let _ = cancel_run(&ctx.manager, run_id).await;
    l
}

// ─────────────────────────────── K4 ───────────────────────────────

async fn section_k4(ctx: &Ctx, project_zero: ProjectId, project_multi: ProjectId) -> Ledger {
    let mut l = Ledger::new();

    let (issue_zero, _n) = match ctx.new_issue(project_zero, "K4a 示例活(零连接器)").await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K4a 建活失败:{e}"));
            return l;
        }
    };
    let zero_result = run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue_zero,
        None,
    )
    .await;
    match zero_result {
        Err(AppError::Run(bw_app::run::RunError::NoExecutor)) => {
            l.check(
                true,
                "K4a:零条连接器 → 如实报 NoExecutor,不取第一条(反正也没有第一条)",
            );
        }
        other => l.fail(format!("K4a:应该被 NoExecutor 拒绝,实得 {other:?}")),
    }
    let runs_zero = ctx
        .control
        .list_runs(Some(project_zero))
        .await
        .unwrap_or_default();
    l.check(runs_zero.is_empty(), "K4a:一条运行行都没有");

    let (issue_multi, _n) = match ctx.new_issue(project_multi, "K4b 示例活(两条连接器)").await
    {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K4b 建活失败:{e}"));
            return l;
        }
    };
    let multi_result = run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue_multi,
        None,
    )
    .await;
    match multi_result {
        Err(AppError::Run(bw_app::run::RunError::AmbiguousExecutor { names, n })) => {
            l.check(
                n == 2,
                format!("K4b:两条连接器 → 如实报 Ambiguous,n=2(实得 {n})"),
            );
            let mut sorted = names.clone();
            sorted.sort();
            l.check(
                sorted == vec![CONNECTOR_NAME_X.to_string(), CONNECTOR_NAME_Y.to_string()],
                format!("K4b:名字清单如实列出两条(实得 {names:?}),不是随便挑一条报出来"),
            );
        }
        other => l.fail(format!("K4b:应该被 AmbiguousExecutor 拒绝,实得 {other:?}")),
    }
    let runs_multi = ctx
        .control
        .list_runs(Some(project_multi))
        .await
        .unwrap_or_default();
    l.check(
        runs_multi.is_empty(),
        "K4b:一条运行行都没有(不会因为『不知道选哪条』就随便开工)",
    );

    l
}

// ─────────────────────────────── K5 ───────────────────────────────

async fn section_k5(ctx: &Ctx, project: ProjectId, mock: &Arc<KickoffExec>) -> Ledger {
    let mut l = Ledger::new();
    let (issue, number) = match ctx.new_issue(project, "K5 示例活(取消)").await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K5 建活失败:{e}"));
            return l;
        }
    };
    let branch = bw_workspace::provision::issue_branch(number as u32);
    mock.set_script(branch, Script::never_finishes());

    let run_id = match run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue,
        None,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("K5:开工应该成功,实得 {e}"));
            return l;
        }
    };

    let cancel1 = cancel_run(&ctx.manager, run_id).await;
    l.check(
        cancel1.is_ok(),
        format!("K5:第一次取消成功(实得 {cancel1:?})"),
    );

    let row1 = ctx.control.get_run(run_id).await.ok().flatten();
    let Some(row1) = row1 else {
        l.fail("K5:取消后应该还能读到运行行");
        return l;
    };
    l.check(
        row1.state == RunState::Canceled,
        format!("K5:结束事实=取消(实得 {:?})", row1.state),
    );
    l.check(
        row1.end_kind == Some(RunEndKind::Canceled),
        "K5:end_kind=Canceled",
    );
    l.check(row1.settled_at.is_some(), "K5:结过账一次(settled_at 非空)");
    l.check(
        !row1.end_detail.is_empty(),
        "K5:结束事实原文非空(不是留白假装没发生)",
    );

    let cancel2 = cancel_run(&ctx.manager, run_id).await;
    l.check(
        cancel2.is_ok(),
        format!("K5:再取消一次仍然 Ok(幂等),实得 {cancel2:?}"),
    );
    let row2 = ctx.control.get_run(run_id).await.ok().flatten();
    l.check(
        row2 == Some(row1),
        "K5:再取消一次,字段逐字节不变(幂等,不是又结了一次账)",
    );

    l
}

// ─────────────────────────────── K7 ───────────────────────────────

async fn section_k7(ctx: &Ctx, project: ProjectId, mock: &Arc<KickoffExec>) -> Ledger {
    let mut l = Ledger::new();
    let (issue, number) = match ctx.new_issue(project, "K7 示例活(事件广播)").await {
        Ok(v) => v,
        Err(e) => {
            l.fail(format!("K7 建活失败:{e}"));
            return l;
        }
    };
    let branch = bw_workspace::provision::issue_branch(number as u32);
    mock.set_script(branch, Script::finishes_after(Duration::from_millis(60)));

    // 订阅必须在开工**之前**——`Starting` 事件在 `run_issue` 返回之前就已
    // 经发出去了(`Loop::handle_start` 里插行占名额之后立刻广播)。
    let mut rx = ctx.manager.subscribe();

    let run_id = match run_issue(
        &ctx.control,
        &ctx.manager,
        &ctx.workspaces_root,
        issue,
        None,
    )
    .await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("K7:开工应该成功,实得 {e}"));
            return l;
        }
    };

    let mut seen: Vec<RunState> = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(1000);
    loop {
        if Instant::now() >= deadline {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Ok(RunStateChanged { run, state })) if run == run_id => {
                seen.push(state);
                if state == RunState::Finished {
                    break;
                }
            }
            Ok(Ok(_)) => continue, // 别的运行(其它段落也在同一条全局频道上广播)的事件,过滤掉。
            Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(broadcast::error::RecvError::Closed)) => break,
            Err(_) => break, // 超时
        }
    }

    l.check(
        seen.first() == Some(&RunState::Starting),
        format!("K7:订阅端收到的第一条事件是 Starting(实得 {seen:?})"),
    );
    l.check(
        seen.contains(&RunState::Running),
        format!("K7:订阅端真的收到了 Running(实得 {seen:?})"),
    );
    l.check(
        seen.last() == Some(&RunState::Finished),
        format!("K7:订阅端最后收到 Finished(实得 {seen:?})"),
    );
    l.check(
        seen.windows(2).all(|w| w[0] != w[1]),
        format!("K7:没有相邻重复事件(每次都是状态真的变了才发,实得 {seen:?})"),
    );

    l
}

// ─────────────────────────────── K8 ───────────────────────────────
// 复用 `run_races.rs` R3 的抖动扫窗形状(见文件头「K8_*」常量注释),经
// `run_issue`/`cancel_run` 走一遍,证明「晚到消息不复活」在开工用例这一
// 层没有被破坏。

async fn section_k8(ctx: &Ctx, project: ProjectId, mock: &Arc<KickoffExec>) -> Ledger {
    let mut l = Ledger::new();
    let mut cancel_wins = 0usize;
    let mut finish_wins = 0usize;

    for round in 1..=K8_ROUNDS {
        let (issue, number) = match ctx
            .new_issue(project, &format!("K8 示例活 · 轮次 {round}"))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                l.fail(format!("K8 第 {round} 轮建活失败:{e}"));
                continue;
            }
        };
        let branch = bw_workspace::provision::issue_branch(number as u32);
        mock.set_script(branch, Script::finishes_after(K8_FINISH_MS));

        let run_id = match run_issue(
            &ctx.control,
            &ctx.manager,
            &ctx.workspaces_root,
            issue,
            None,
        )
        .await
        {
            Ok(id) => id,
            Err(e) => {
                l.fail(format!("K8 第 {round} 轮开工失败:{e}"));
                continue;
            }
        };

        let jitter_ms: i64 = if K8_ROUNDS <= 1 {
            0
        } else {
            -K8_JITTER_MS + (2 * K8_JITTER_MS * (round as i64 - 1)) / (K8_ROUNDS as i64 - 1)
        };
        let delay_ms = (K8_FINISH_MS.as_millis() as i64 + jitter_ms).max(0) as u64;
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let cancel_result = cancel_run(&ctx.manager, run_id).await;

        let closed = wait_for_closed(&ctx.control, run_id, Duration::from_millis(600)).await;
        let Some(row1) = closed.filter(|r| r.ended_at.is_some()) else {
            l.fail(format!("K8 轮次 {round}:运行应该在 600ms 内关门"));
            continue;
        };
        l.check(
            cancel_result.is_ok(),
            format!("K8 轮次 {round}:cancel_run() 本身不报错"),
        );
        l.check(
            row1.settled_at.is_some(),
            format!("K8 轮次 {round}:settled_at 非空(结过账一次)"),
        );
        l.check(
            matches!(row1.state, RunState::Canceled | RunState::Finished),
            format!(
                "K8 轮次 {round}:state ∈ {{canceled, finished}}(实得 {:?})",
                row1.state
            ),
        );
        match row1.state {
            RunState::Canceled => cancel_wins += 1,
            RunState::Finished => finish_wins += 1,
            _ => {}
        }

        // 再发一次结算(直接走存储层,同 run_races.rs R3 的既有理由:运行
        // 管理器没有暴露"手动再结一次"的公开方法)——晚到的一方即使真的
        // 追上来,settled_at 也不该再变。
        let second_settle = ctx.control.settle_run(run_id, now() + 999).await;
        let row2 = ctx.control.get_run(run_id).await.ok().flatten();
        match (second_settle, row2) {
            (Ok(false), Some(row2)) if row2.settled_at == row1.settled_at => {
                l.check(
                    true,
                    format!("K8 轮次 {round}:重结算命令后 settled_at 值不变(晚到不复活)"),
                );
            }
            (Ok(true), _) => {
                l.fail(format!(
                    "K8 轮次 {round}:settled_at 应该已经结过账,重结算命令不该再成功——晚到复活了"
                ));
            }
            (result, row2) => {
                l.fail(format!(
                    "K8 轮次 {round}:重结算读回异常(result={result:?}, row2.settled_at={:?})",
                    row2.map(|r| r.settled_at)
                ));
            }
        }

        // 关门后再显式取消一次 → 幂等 Ok,字段一个不改(经 `cancel_run`
        // 用例函数,不是直接调 `manager.cancel`)。
        let idem = cancel_run(&ctx.manager, run_id).await;
        let row3 = ctx.control.get_run(run_id).await.ok().flatten();
        match (idem, row3) {
            (Ok(()), Some(row3)) if row3 == row1 => {
                l.check(
                    true,
                    format!("K8 轮次 {round}:关门后再取消一次,幂等不改字段"),
                );
            }
            (result, row3) => {
                l.fail(format!(
                    "K8 轮次 {round}:关门后再取消应该幂等不改字段,实得 idem={result:?} row3={row3:?} row1={row1:?}"
                ));
            }
        }
    }

    l.check(
        cancel_wins > 0,
        format!("K8:抖动扫窗真的扫到了「取消先赢」的轮次(共 {cancel_wins} 轮)"),
    );
    l.check(
        finish_wins > 0,
        format!("K8:抖动扫窗真的扫到了「结束先赢」的轮次(共 {finish_wins} 轮)"),
    );

    l
}
