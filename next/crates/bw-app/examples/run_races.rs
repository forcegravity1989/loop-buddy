//! `run_races` — 切片四收官验收指挥器(design-s4-runmanager.md §5)。
//!
//! 不开界面,直接调用 [`bw_app::run::RunManager`] 的公开方法,把设计稿
//! §4 点名的五个竞态(同一件活开不出第二个交付运行 / 取消与完成同时到
//! 达只结算一次 / 单条失败不牵连 / 重启后遗留运行如实标注不假活 / 晚到
//! 消息不错账)与 §7 的验收条目「十件活并行互不覆盖、各自只结算一次」确
//! 定性地造出来、`sqlite3` 读回核对。**mock 档全程不依赖网关**,可安全
//! 常绿;`--real` 那一档依赖真实 `claude` 会话,不进常绿门禁(§5.3)。
//!
//! **本档的脚本化执行连接器([`ScriptedExec`])住在这个文件里,不进任何
//! crate 的正式代码**(design §4.1)——它的状态是「起跑时刻 + 脚本」的纯
//! 函数,不用定时器、不用后台任务,所以对调度抖动免疫,同样的脚本永远
//! 给同样的时间线。摘要一律带【mock】自我标注(核心纪律第 2 条)。
//!
//! 七段:R1(十件并行,互不覆盖、各自只结算一次)/ R2(同一件活开不出第
//! 二个交付运行)/ R3(取消与完成同时到达,只结算一次,20 轮抖动复现)/
//! R4(单条失败不牵连)/ R5(重启后的遗留运行如实标注,不假活)/ R6(晚
//! 到的消息不错账)/ 附(同工作区串线校验、存量库迁移双守卫)。
//!
//! 跑法:
//! ```text
//! cargo run -p bw-app --example run_races                    # mock 档,常绿
//! cargo run -p bw-app --example run_races -- --n 10 --rounds 20
//! cargo run -p bw-app --example run_races -- --real <git 检出根的父目录>
//! ```
//! 退出码 0 且末行 `RUN_RACES_OK` = 全部断言通过;任何一条失败 → stderr
//! 打 `ASSERT FAILED: …`,末行 `RUN_RACES_FAILED`,退出码 1。

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use bw_app::run::{RunManager, RunManagerConfig, StartRun};
use bw_app::App;
use bw_connector::{
    guarded, CallCtx, ConfigRef, ConnError, ConnResult, Connector, ConnectorEntry, ConnectorKind,
    ConnectorRegistry, ExecSpec, ExecState, ExecTicket, Execute, OpClass, ProjectBinding,
    RequestId, SessionEnd,
};
use bw_core::{ConnectorId, IssueId, IssueStatus, ProjectId, RunId};
use bw_store::{IssueStore, NewIssue, NewProject, RunStore, SqliteStore, StoreError};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};
use time::OffsetDateTime;
use uuid::Uuid;

fn now() -> i64 {
    OffsetDateTime::now_utc().unix_timestamp()
}

const DB_NAME_PREFIX: &str = "bw-run-races-";
const CONNECTOR_NAME: &str = "mock-scripted";

/// R3 的脚本结束时刻——与 [`RunManagerConfig::default`] 的 `poll_interval`
/// (design 默认 250ms)对齐,理由见 `build_scripts` 里 R3 那一段。
const R3_FINISH_MS: Duration = Duration::from_millis(250);
/// R3 抖动窗口的半宽——围绕 `R3_FINISH_MS` 扫 ±35ms,比设计稿举例的
/// ±5ms 宽,因为真实调度(tokio 任务唤醒、mpsc 队列排队)本身就有个位
/// 数到十几毫秒的抖动,窗口太窄扫不到两种结局都发生的证据(实测复
/// 现:±5ms 在本机 20 轮全部同一方赢)。
const R3_JITTER_MS: i64 = 35;

fn fresh_db_path() -> PathBuf {
    std::env::temp_dir().join(format!("{DB_NAME_PREFIX}{}.db", Uuid::new_v4()))
}

/// 开跑前清场——同 `bw-store` `examples/store_guards.rs` 的既有先例(评审
/// Important-2 补丁):PID 会被系统复用,uuid 不会;清掉临时目录里所有更
/// 早的同前缀库,任何一次跑读到的库要么是它自己刚建的,要么根本不存
/// 在,不会有第三种「读到别人的库」。
fn clear_stale_dbs(current: &PathBuf) {
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

// ─────────────────────────── 参数 ───────────────────────────

struct Args {
    n: usize,
    rounds: usize,
    real: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut n = 10usize;
    let mut rounds = 20usize;
    let mut real = None;
    let mut argv = std::env::args().skip(1);
    while let Some(a) = argv.next() {
        match a.as_str() {
            "--n" => {
                n = argv
                    .next()
                    .ok_or("--n 需要一个数字参数")?
                    .parse()
                    .map_err(|e| format!("--n 解析失败:{e}"))?;
            }
            "--rounds" => {
                rounds = argv
                    .next()
                    .ok_or("--rounds 需要一个数字参数")?
                    .parse()
                    .map_err(|e| format!("--rounds 解析失败:{e}"))?;
            }
            "--real" => {
                real = Some(PathBuf::from(argv.next().ok_or("--real 需要一个目录参数")?));
            }
            other => return Err(format!("未知参数:{other}")),
        }
    }
    Ok(Args { n, rounds, real })
}

// ─────────────────────────── 断言账本 ───────────────────────────

/// 每一节自己的断言账本,`main` 最后把所有节的账本合成一份总账——末行
/// 打印的「断言 N 条,全过」的 N 就是这份总账的 `count`。
struct Ledger {
    ok: bool,
    count: usize,
}

impl Ledger {
    fn new() -> Self {
        Self { ok: true, count: 0 }
    }
    /// **登记一条失败的唯一入口**(评审 Critical-3 的假绿根治)。此前有
    /// 25 处早退路径只打 `eprintln!("ASSERT FAILED: …")` 就 `return`,却
    /// 没有调用这个方法——`ok` 字段就不会翻,`main` 合账时看到的还是
    /// 「全过」,末行照样打 `RUN_RACES_OK`、退出码 0(实测复现:其中一处
    /// 甚至让断言总数从 169 静静掉到 165,还写着「全过」)。这 25 处现在
    /// 全部改成经这里(或简写 [`Ledger::fail`])登记,`ok` 才会真的跟着
    /// 翻——凡是打了 `ASSERT FAILED` 却没有一条 `l.count` 增量与它对应
    /// 的路径,就是这个方法要堵住的那种绕过。
    fn check(&mut self, cond: bool, label: impl std::fmt::Display) {
        self.count += 1;
        if cond {
            println!("  ✓ {label}");
        } else {
            eprintln!("ASSERT FAILED: {label}");
            self.ok = false;
        }
    }
    /// `check(false, label)` 的简写——早退路径「这条本身就是失败」时用,
    /// 语义完全等价,不是第二条打印通道(内部仍然只调用 [`Ledger::check`])。
    fn fail(&mut self, label: impl std::fmt::Display) {
        self.check(false, label);
    }
    fn merge(&mut self, other: Ledger) {
        self.ok &= other.ok;
        self.count += other.count;
    }
}

// ─────────────────────────── 脚本化 mock 执行连接器(design §4.1)───────

/// 脚本怎么结束——只覆盖 R1-R6 用得到的两档事实(`process_exit` /
/// `contact_lost`),不是 `bw_connector::SessionEnd` 的完整镜像(那边还有
/// `StoppedByBw`,本档的脚本不需要模拟挂钟/预算封顶)。
#[derive(Clone, Copy)]
enum EndKind {
    ProcessExit,
    ContactLost,
}

/// 一条分支对应的脚本——不用定时器、不用后台任务,状态永远是「起跑时刻
/// + 脚本」的纯函数(design §4.1)。
#[derive(Clone)]
struct Script {
    /// 起跑要不要直接失败(造「单条失败不牵连」的一半)。
    start_fails: Option<String>,
    /// 过了多久结束、怎么结束。`None` = 永不结束(造「重启遗留」)。
    finish_after: Option<(Duration, EndKind)>,
}

struct Live {
    started_at: Instant,
    canceled_at: Option<Instant>,
    finish_after: Option<(Duration, EndKind)>,
}

/// 【mock】脚本化执行连接器。住在指挥器里,不进任何 crate 的正式代码
/// (design §4.1)。
struct ScriptedExec {
    kind: ConnectorKind,
    binding: ProjectBinding,
    /// 按分支查脚本——一件活唯一(design §4.1)。
    scripts: HashMap<String, Script>,
    live: Mutex<HashMap<RequestId, Live>>,
    /// R5「不批量唤醒」断言用:reap 前后这个数不该变——reap 只标注 BW 侧
    /// 的数据库行,从不调用这里的 `start`。
    starts_issued: AtomicUsize,
}

impl ScriptedExec {
    fn new(binding: ProjectBinding, scripts: HashMap<String, Script>) -> Self {
        Self {
            kind: ConnectorKind::AgentCli {
                cli: "mock-scripted(run_races 指挥器自建,不进任何 crate 正式代码)".to_string(),
            },
            binding,
            scripts,
            live: Mutex::new(HashMap::new()),
            starts_issued: AtomicUsize::new(0),
        }
    }

    fn starts_issued(&self) -> usize {
        self.starts_issued.load(Ordering::SeqCst)
    }
}

impl Connector for ScriptedExec {
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
impl Execute for ScriptedExec {
    async fn start(&self, cx: &CallCtx, spec: ExecSpec) -> ConnResult<ExecTicket> {
        let req = cx.req;
        let branch = spec.branch;
        guarded(cx, OpClass::Write, async move {
            let Some(script) = self.scripts.get(&branch) else {
                return Err(ConnError::Other(format!(
                    "run_races 脚本化连接器:分支 {branch:?} 没有登记脚本(指挥器装配缺口,不是竞态本身)"
                )));
            };
            if let Some(msg) = &script.start_fails {
                return Err(ConnError::NotConnected(msg.clone()));
            }
            let finish_after = script.finish_after;
            self.starts_issued.fetch_add(1, Ordering::SeqCst);
            self.live.lock().expect("mock live 表 mutex poisoned").insert(
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
            let live = self.live.lock().expect("mock live 表 mutex poisoned");
            let Some(l) = live.get(&req) else {
                return Ok(ExecState::Unknown);
            };
            // 被取消过 → Canceled(design §4.1 四行逻辑的第一行——取消不
            // 改结束脚本,但取消这一刻起 poll 恒报 Canceled)。
            if l.canceled_at.is_some() {
                return Ok(ExecState::Canceled);
            }
            match l.finish_after {
                None => Ok(ExecState::Running), // 脚本说永不结束
                Some((dur, kind)) => {
                    if l.started_at.elapsed() >= dur {
                        let ended = match kind {
                            EndKind::ProcessExit => SessionEnd::ProcessExit { code: Some(0) },
                            EndKind::ContactLost => SessionEnd::ContactLost,
                        };
                        Ok(ExecState::Finished {
                            ended,
                            summary: "【mock】run_races 脚本化执行连接器:到点结束".to_string(),
                        })
                    } else {
                        Ok(ExecState::Running) // 还没到点
                    }
                }
            }
        })
        .await
    }

    async fn cancel(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<()> {
        let req = t.req;
        guarded(cx, OpClass::Write, async move {
            // 只打一个取消标记 + 记时刻,**不改结束脚本**(design §4.1)。
            if let Some(l) = self
                .live
                .lock()
                .expect("mock live 表 mutex poisoned")
                .get_mut(&req)
            {
                l.canceled_at = Some(Instant::now());
            }
            Ok(())
        })
        .await
    }
}

// ─────────────────────────── 指挥器上下文 ───────────────────────────

struct Ctx {
    control: SqliteStore,
    manager: RunManager,
    registry: std::sync::Arc<ConnectorRegistry>,
    /// 直接持有的 mock 连接器把手(不是 `Arc<dyn Connector>`)——「不批量
    /// 唤醒」那条断言(R5)要读它私有的 `starts_issued` 计数器,`Connector`
    /// trait 本身故意零 `dyn Any`(design-s2-connector.md §3),没有下转
    /// 路径,只能在装配时多留一份具体类型的 `Arc`。
    mock: std::sync::Arc<ScriptedExec>,
    project: ProjectId,
    db_path: PathBuf,
    issue_no: AtomicI64,
}

impl Ctx {
    async fn new_issue(&self, title: &str) -> Result<IssueId, StoreError> {
        let id = IssueId::new();
        let number = self.issue_no.fetch_add(1, Ordering::SeqCst);
        self.control
            .create_issue(NewIssue {
                id,
                project_id: self.project,
                number,
                title: title.to_string(),
            })
            .await?;
        Ok(id)
    }

    fn mock_start(
        &self,
        issue: IssueId,
        workspace: impl Into<PathBuf>,
        branch: impl Into<String>,
    ) -> StartRun {
        StartRun {
            issue,
            connector_name: Some(CONNECTOR_NAME.to_string()),
            workspace: workspace.into(),
            branch: branch.into(),
            inject: vec![],
            budget_usd: None,
        }
    }
}

/// 轮询等一批运行全部关门(`ended_at` 非空),给够上限但不空等——固定
/// `sleep` 在系统繁忙时会不够长,轮询更稳(同 `bw-engine`
/// `examples/agent_session.rs` `poll_until` 的既定先例)。
async fn wait_for_all_closed(control: &SqliteStore, ids: &[RunId], deadline: Duration) -> bool {
    let start = Instant::now();
    loop {
        let mut all_closed = true;
        for id in ids {
            match control.get_run(*id).await {
                Ok(Some(row)) if row.ended_at.is_some() => {}
                _ => {
                    all_closed = false;
                    break;
                }
            }
        }
        if all_closed {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("参数错误:{e}");
            eprintln!("用法:run_races [--n <N>] [--rounds <N>] [--real <git 检出根的父目录>]");
            return ExitCode::FAILURE;
        }
    };

    let db_path = fresh_db_path();
    clear_stale_dbs(&db_path);

    println!("== run_races · 切片四行为验收指挥器 ==");
    println!("本次数据库:{}   ← 跑完不删,给人手工复核", db_path.display());
    println!("执行连接器:【mock】脚本化(自我标注;真实并行档要 --real)");
    println!(
        "并行件数:{}   撞车轮数:{}   轮询节奏:250ms",
        args.n, args.rounds
    );
    println!();

    let control = match SqliteStore::open(&db_path.to_string_lossy()).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ASSERT FAILED: 开库失败:{e}");
            eprintln!("RUN_RACES_FAILED");
            return ExitCode::FAILURE;
        }
    };

    let project = ProjectId::new();
    if let Err(e) = control
        .create_project(NewProject {
            id: project,
            name: "run_races 示例项目".to_string(),
            root_path: String::new(),
        })
        .await
    {
        eprintln!("ASSERT FAILED: create_project 失败:{e}");
        eprintln!("RUN_RACES_FAILED");
        return ExitCode::FAILURE;
    }

    let scripts = build_scripts(args.n, args.rounds);
    let binding = ProjectBinding {
        project,
        host: String::new(),
        path: "run-races-mock".to_string(),
    };
    let mock = std::sync::Arc::new(ScriptedExec::new(binding.clone(), scripts));
    let mut registry = ConnectorRegistry::default();
    registry.register(
        ConnectorEntry {
            id: ConnectorId::new(),
            name: CONNECTOR_NAME.to_string(),
            kind: ConnectorKind::AgentCli {
                cli: "mock-scripted".to_string(),
            },
            binding,
            config: ConfigRef::AgentRegistryRow {
                slug: "mock-scripted".to_string(),
            },
        },
        mock.clone(),
    );
    let registry = std::sync::Arc::new(registry);

    let manager =
        match RunManager::open(&db_path, registry.clone(), RunManagerConfig::default()).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ASSERT FAILED: RunManager::open 失败:{e}");
                eprintln!("RUN_RACES_FAILED");
                return ExitCode::FAILURE;
            }
        };

    let ctx = Ctx {
        control,
        manager,
        registry,
        mock,
        project,
        db_path: db_path.clone(),
        issue_no: AtomicI64::new(1),
    };

    let mut total = Ledger::new();

    println!("== R1 · 十件并行,互不覆盖、各自只结算一次 ==");
    let (r1_ok, r1_ledger) = section_r1(&ctx, args.n).await;
    total.merge(r1_ledger);
    println!();
    if !r1_ok {
        // R1 是后续几节共用连接器/registry 的基础健全性证据——不硬性阻断
        // 后续几节(它们各自新建活,互不牵连),继续跑完整份指挥器,如实
        // 报告最终结果。
    }

    println!("== R2 · 同一件活开不出第二个交付运行 ==");
    total.merge(section_r2(&ctx).await);
    println!();

    println!(
        "== R3 · 取消与完成同时到达,只结算一次({} 轮抖动)==",
        args.rounds
    );
    total.merge(section_r3(&ctx, args.rounds).await);
    println!();

    println!("== R4 · 单条失败不牵连 ==");
    total.merge(section_r4(&ctx).await);
    println!();

    println!("== R5 · 重启后的遗留运行如实标注,不假活 ==");
    total.merge(section_r5(&ctx).await);
    println!();

    println!("== R6 · 晚到的消息不错账 ==");
    total.merge(section_r6(&ctx).await);
    println!();

    println!("== 附 · 降级为咨询(design §3.5,评审 Critical-1 补齐覆盖)==");
    total.merge(section_demote(&ctx).await);
    println!();

    println!("== 附 · 同工作区串线校验(主控裁决 #5)==");
    total.merge(section_workspace_guard(&ctx).await);
    println!();

    println!("== 附 · 完成永远人点(显式转移 + 合法转移表拒绝)==");
    total.merge(section_done_is_human(&ctx).await);
    println!();

    println!("== 附 · sqlite 读回清单(design §5.2 九条,原样打印可复制)==");
    total.merge(section_sql_readback(&db_path, r1_ok).await);
    println!();

    println!("== 附 · 存量库迁移双守卫 ==");
    total.merge(section_migration_guard().await);
    println!();

    // 评审 Critical-3 第二层问题:期望条数下限。上面的 `Ledger::check`/
    // `fail` 已经堵死了「打了 ASSERT FAILED 却不反映到 ok」这条路,但还
    // 有一类更隐蔽的写法级问题防不住:如果哪节的 `total.merge(...)` 调
    // 用被漏掉或注释掉,那一节的每条具体断言都不会跑(不是失败,是压根
    // 没执行),`ok` 不会翻,只有条数会悄悄变少——这正是评审复现的原始
    // 症状(169 静静掉到 165)。条数是 `--n`/`--rounds` 的确定函数(R1 是
    // 聚合断言,与 `--n` 无关;R3 每轮 6 条,与 `--rounds` 成正比;其余
    // 各节条数固定,常绿参数下逐节实测核对过):
    //   R1(6) + R2(3) + R3(6×rounds) + R4(15) + R5(19) + R6(7)
    //   + 降级为咨询(11) + 同工作区串线(1) + 完成永远人点(2)
    //   + sqlite 读回(10) + 存量库迁移双守卫(1) = 75 + 6×rounds
    //   （R5 从 15 变 19,切片四-2 修二新增:在 reap 之前对 ids[2]——库里
    //   开着、但管理器 B 内存里没有的运行——调一次 `cancel`,断言其效果
    //   +7;原本 3 条都被 reap 判成 orphaned 那半,现在 ids[2] 提前被
    //   cancel 关门、不再算 orphaned,少了它那份 3 条子断言 -3,orphaned
    //   计数本身也从 3 改判 2;净变化 +4,15→19。）
    let expected_assertions = 75 + 6 * args.rounds;
    let actual_before_meta_check = total.count;
    total.check(
        actual_before_meta_check == expected_assertions,
        format!(
            "断言条数与设计期望一致(公式 75+6×rounds,rounds={} ⇒ 期望 {expected_assertions},实得 {actual_before_meta_check};不等就说明有一整节的断言没有真的跑到)",
            args.rounds
        ),
    );

    // 评审 Important-5:`--real` 显式传了就要真的算数——此前这一档的返
    // 回值没有并进账本,失败只打印 `REAL_RUN_FAILED`,末行仍然
    // `RUN_RACES_OK`、退出码 0。design §5.3 结尾原文:「显式传了 --real
    // 但连接器不可用 → 打一行如实的原因并以退出码 1 结束」。`real_ok`
    // 默认 `true`(没传 `--real` 时这一档压根没跑,不影响判定)。
    let mut real_ok = true;
    if args.real.is_some() {
        println!("== 附 · 真实并行三件(--real,不进常绿门禁,经 RunManager)==");
        real_ok = section_real(&ctx, args.real.as_deref().unwrap(), args.n).await;
        println!();
    }

    println!(
        "断言 {} 条(mock 档),{}",
        total.count,
        if total.ok { "全过" } else { "有失败" }
    );
    println!("数据库留在:{}", db_path.display());
    let overall_ok = total.ok && real_ok;
    if overall_ok {
        println!("RUN_RACES_OK");
        ExitCode::SUCCESS
    } else {
        if !real_ok {
            eprintln!("ASSERT FAILED: --real 档未达到 design §5.3 的判定(见上方 REAL_RUN_FAILED/断言明细)");
        }
        eprintln!("RUN_RACES_FAILED");
        ExitCode::FAILURE
    }
}

/// 装配 R1-R6 全部用得到的脚本——集中一处构造,每节用不同前缀的分支名,
/// 互不冲突(design §4.2 六段脚本)。
fn build_scripts(n: usize, rounds: usize) -> HashMap<String, Script> {
    let mut scripts = HashMap::new();

    // R1:第 i 件在 200ms + i×30ms 结束,彼此错开,时间线唯一。
    for i in 1..=n {
        scripts.insert(
            format!("bw/issue-r1-{i}"),
            Script {
                start_fails: None,
                finish_after: Some((
                    Duration::from_millis(200 + (i as u64) * 30),
                    EndKind::ProcessExit,
                )),
            },
        );
    }

    // R2:一件活,长脚本(5 秒后结束,本节的等待窗口内跑不到)。
    scripts.insert(
        "bw/issue-r2".to_string(),
        Script {
            start_fails: None,
            finish_after: Some((Duration::from_secs(5), EndKind::ProcessExit)),
        },
    );

    // R3:每轮一件新活,固定在 `R3_FINISH_MS`(= 一个轮询节奏,design 默认
    // 250ms)结束——指挥器负责抖动「取消发出的时刻」,不是脚本本身抖
    // 动。**为什么定在正好一个轮询节奏,不是随便一个数**:轮询任务的第
    // 一次真实 poll 调用就落在起工之后约一个轮询节奏的时刻,把脚本的结
    // 束时刻和这个真实的 poll 时刻对齐,才能让「取消命令」与「轮询任务
    // 观测到 Finished 并回传的命令」两条消息真的在同一个窗口里竞争同一
    // 条命令队列——脚本结束时刻定得离第一次 poll 太远(比如原来的
    // 400ms,离 250ms 那次 poll 太早、离 500ms 那次 poll 又太晚),取消
    // 命令会稳赢,20 轮真实跑出来是 20:0(已实测复现,不是猜测),不是
    // 抖动窗口的问题,是没对准真正会竞争的那个时间点。
    for i in 1..=rounds {
        scripts.insert(
            format!("bw/issue-r3-{i}"),
            Script {
                start_fails: None,
                finish_after: Some((R3_FINISH_MS, EndKind::ProcessExit)),
            },
        );
    }

    // R4:十件活,第 3 件 contact_lost、第 7 件起跑就失败,其余正常结束。
    for i in 1..=10 {
        let script = if i == 3 {
            Script {
                start_fails: None,
                finish_after: Some((Duration::from_millis(150), EndKind::ContactLost)),
            }
        } else if i == 7 {
            Script {
                start_fails: Some(
                    "run_races R4 脚本:第 7 件故意起跑失败(未连接,造「起跑就失败」)".to_string(),
                ),
                finish_after: None,
            }
        } else {
            Script {
                start_fails: None,
                finish_after: Some((Duration::from_millis(150), EndKind::ProcessExit)),
            }
        };
        scripts.insert(format!("bw/issue-r4-{i}"), script);
    }

    // R5:3 个永不结束的脚本(造「重启遗留」)。
    for i in 1..=3 {
        scripts.insert(
            format!("bw/issue-r5-{i}"),
            Script {
                start_fails: None,
                finish_after: None,
            },
        );
    }

    // R6:A 300ms 结束(会被 100ms 时的取消抢先关门,300ms 的消息晚到);
    // B(取消后在同一件活上重开)600ms 结束——两条分支各自独立的脚本,
    // 时长不同,不会互相污染。
    scripts.insert(
        "bw/issue-r6-a".to_string(),
        Script {
            start_fails: None,
            finish_after: Some((Duration::from_millis(300), EndKind::ProcessExit)),
        },
    );
    scripts.insert(
        "bw/issue-r6-b".to_string(),
        Script {
            start_fails: None,
            finish_after: Some((Duration::from_millis(600), EndKind::ProcessExit)),
        },
    );

    // 附 · 同工作区串线校验:两件活抢同一个工作区,第二件用不到的脚本
    // (校验在名额层面就被挡下,压根不会真的调用 start)。
    scripts.insert(
        "bw/issue-wscheck-a".to_string(),
        Script {
            start_fails: None,
            finish_after: Some((Duration::from_secs(5), EndKind::ProcessExit)),
        },
    );
    scripts.insert(
        "bw/issue-wscheck-b".to_string(),
        Script {
            start_fails: None,
            finish_after: Some((Duration::from_secs(5), EndKind::ProcessExit)),
        },
    );

    // 附 · 降级为咨询(评审 Critical-1):A 是那条会被降级的交付运行,脚
    // 本足够长(900ms),留够时间做「降级 → 读回 → 同活立刻再开工 → 幂
    // 等降级」这一串步骤,又足够短,能在本节结束前等到它自然结束(验证
    // 「账不重结」)。B 是降级后在同一件活上重开的第二条交付运行,短脚
    // 本,不需要等它结束。
    scripts.insert(
        "bw/issue-demote-a".to_string(),
        Script {
            start_fails: None,
            finish_after: Some((Duration::from_millis(900), EndKind::ProcessExit)),
        },
    );
    scripts.insert(
        "bw/issue-demote-b".to_string(),
        Script {
            start_fails: None,
            finish_after: Some((Duration::from_millis(200), EndKind::ProcessExit)),
        },
    );

    scripts
}

// ─────────────────────────── R1 ───────────────────────────

async fn section_r1(ctx: &Ctx, n: usize) -> (bool, Ledger) {
    let mut l = Ledger::new();
    println!(
        "脚本:#1 230ms 退出(码 0)· … · #{n} {}ms 退出(码 0)",
        200 + n * 30
    );

    let mut issues = Vec::with_capacity(n);
    for i in 1..=n {
        match ctx.new_issue(&format!("R1 示例活 #{i}")).await {
            Ok(id) => issues.push(id),
            Err(e) => {
                l.fail(format!("R1 建活 #{i} 失败:{e}"));
                return (false, l);
            }
        }
    }

    let started = Instant::now();
    let mut handles = Vec::with_capacity(n);
    for (idx, issue) in issues.iter().enumerate() {
        let i = idx + 1;
        let manager = ctx.manager.clone();
        let req = ctx.mock_start(
            *issue,
            format!("/tmp/run-races-mock/r1-{i}"),
            format!("bw/issue-r1-{i}"),
        );
        handles.push(tokio::spawn(async move { manager.start(req).await }));
    }
    let mut ids = Vec::with_capacity(n);
    for h in handles {
        match h.await {
            Ok(Ok(id)) => ids.push(id),
            Ok(Err(e)) => {
                l.fail(format!("R1 并发开工失败:{e}"));
                return (false, l);
            }
            Err(e) => {
                l.fail(format!("R1 并发开工任务 panic:{e}"));
                return (false, l);
            }
        }
    }

    let closed = wait_for_all_closed(&ctx.control, &ids, Duration::from_secs(5)).await;
    let elapsed = started.elapsed();
    println!(
        "实跑:{n} 起,{} 关门,耗时 {:.2}s",
        ids.len(),
        elapsed.as_secs_f64()
    );
    l.check(closed, format!("{n} 条运行全部在 5s 内关门"));

    let mut branches = std::collections::HashSet::new();
    let mut workspaces_ok = true;
    let mut settled_count = 0usize;
    let mut ended_count = 0usize;
    for (idx, id) in ids.iter().enumerate() {
        let i = idx + 1;
        match ctx.control.get_run(*id).await {
            Ok(Some(row)) => {
                println!(
                    "  #{i}  {}  {:?}  {:?}  settled_at={:?}",
                    row.branch, row.state, row.end_kind, row.settled_at
                );
                branches.insert(row.branch.clone());
                if row.branch != format!("bw/issue-r1-{i}")
                    || row.workspace != format!("/tmp/run-races-mock/r1-{i}")
                {
                    workspaces_ok = false;
                }
                if row.ended_at.is_some() {
                    ended_count += 1;
                }
                if row.settled_at.is_some() {
                    settled_count += 1;
                }
            }
            _ => {
                eprintln!("ASSERT FAILED: 读不回运行 #{i}({id:?})");
                workspaces_ok = false;
            }
        }
    }

    l.check(ids.len() == n, format!("运行行数 = {n}"));
    l.check(
        branches.len() == ids.len(),
        "运行编号两两不同(通过 SET 去重分支名核验数量一致)",
    );
    l.check(
        workspaces_ok,
        "每行分支与自己的活对得上(无一条带别人的分支)",
    );
    l.check(
        ended_count == n,
        format!("全部 ended_at 非空(实得 {ended_count}/{n})"),
    );
    l.check(
        settled_count == n,
        format!("结过账的运行数 = {n}(实得 {settled_count})"),
    );

    (l.ok, l)
}

// ─────────────────────────── R2 ───────────────────────────

async fn section_r2(ctx: &Ctx) -> Ledger {
    let mut l = Ledger::new();

    let issue = match ctx.new_issue("R2 示例活").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("R2 建活失败:{e}"));
            return l;
        }
    };
    let req1 = ctx.mock_start(issue, "/tmp/run-races-mock/r2", "bw/issue-r2");
    let run1 = match ctx.manager.start(req1).await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("R2 第一次开工应该成功,实得:{e}"));
            return l;
        }
    };
    println!("第一件交付运行开工:{run1:?}");

    let req2 = ctx.mock_start(issue, "/tmp/run-races-mock/r2-second", "bw/issue-r2");
    match ctx.manager.start(req2).await {
        Err(bw_app::run::RunError::AlreadyRunning { existing }) => {
            l.check(
                existing == run1,
                format!(
                    "第二次开工被如实拒绝:「#{issue:?} 已有一个交付运行在跑(运行 {existing:?})」"
                ),
            );
        }
        Err(other) => {
            eprintln!("ASSERT FAILED: 第二次开工应该是 AlreadyRunning,实得:{other}");
            l.check(false, "第二次开工被正确分类为 AlreadyRunning");
        }
        Ok(id) => {
            eprintln!("ASSERT FAILED: 第二次开工应该被拒绝,实得成功:{id:?}");
            l.check(false, "第二次开工被拒绝");
        }
    }

    // 防伪断言(design §4.2):直接对数据库插一条同一件活的活交付运行
    // 行,绕过管理器还是插不进去,才叫铁律进了存储,不是 if 判断。
    let bypass = bw_store::NewRun {
        id: bw_core::RunId::new(),
        project_id: ctx.project,
        issue_id: issue,
        kind: bw_store::RunKind::Delivery,
        connector_name: CONNECTOR_NAME.to_string(),
        req_id: format!("run-races-r2-bypass/{}", Uuid::new_v4()),
        workspace: "/tmp/run-races-mock/r2-bypass".to_string(),
        branch: "bw/issue-r2-bypass".to_string(),
        state: bw_core::RunState::Starting,
        started_at: now(),
    };
    match ctx.control.create_run(bypass).await {
        Err(e) if e.is_unique_violation() => {
            println!("防伪:绕过管理器直接插同一件活的活交付行 → UNIQUE 约束冲突(错误原文:{e})");
            l.check(true, "防伪断言:守卫在存储,不在 if");
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 防伪插入应该是唯一约束冲突,实得别的错误:{e}");
            l.check(false, "防伪断言:错误分类正确");
        }
        Ok(()) => {
            eprintln!("ASSERT FAILED: 防伪插入应该被拒绝,实得成功——铁律没有真的钉进存储");
            l.check(false, "防伪断言:插入被数据库拒绝");
        }
    }

    // 评审 Important-3:此前这里借 `find_live_delivery_run` 的 0/1 存在性
    // 判断冒充「行数」——design §4.2 R2 要的是「该活运行行仍然只有 1
    // 条」,如果管理器不小心多插了一条 consultation/failed 行,存在性判
    // 断照样绿。改成真的 `SELECT COUNT(*)`(见 [`count_run_rows_for_issue`])。
    match count_run_rows_for_issue(&ctx.db_path, issue).await {
        Some(n) => l.check(n == 1, format!("该活运行行数(真实 COUNT)= 1(实得 {n})")),
        None => l.fail("该活运行行数读回失败"),
    }

    l
}

/// 真的按行数回答「这件活有几条运行行」——不是「有没有活着的那一条」
/// (评审 Important-3)。直接开一条独立只读连接跑 `SELECT COUNT(*)`,与
/// `section_sql_readback` 同款做法,不新增 `RunStore` trait 方法(裁决
/// #7:接口按聚合拆,不多堆方法)。
async fn count_run_rows_for_issue(db_path: &std::path::Path, issue: IssueId) -> Option<i64> {
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = SqlitePool::connect_with(opts).await.ok()?;
    let row = sqlx::query("SELECT COUNT(*) AS n FROM run WHERE issue_id = ?")
        .bind(issue.uuid().to_string())
        .fetch_one(&pool)
        .await
        .ok()?;
    Some(row.get::<i64, _>("n"))
}

// ─────────────────────────── R3 ───────────────────────────

async fn section_r3(ctx: &Ctx, rounds: usize) -> Ledger {
    let mut l = Ledger::new();
    let mut cancel_wins = 0usize;
    let mut finish_wins = 0usize;

    for round in 1..=rounds {
        let issue = match ctx.new_issue(&format!("R3 示例活 · 轮次 {round}")).await {
            Ok(id) => id,
            Err(e) => {
                eprintln!("ASSERT FAILED: R3 第 {round} 轮建活失败:{e}");
                l.check(false, format!("第 {round} 轮建活成功"));
                continue;
            }
        };
        let req = ctx.mock_start(
            issue,
            format!("/tmp/run-races-mock/r3-{round}"),
            format!("bw/issue-r3-{round}"),
        );
        let run_id = match ctx.manager.start(req).await {
            Ok(id) => id,
            Err(e) => {
                eprintln!("ASSERT FAILED: R3 第 {round} 轮开工失败:{e}");
                l.check(false, format!("第 {round} 轮开工成功"));
                continue;
            }
        };

        // 抖动:取消发出的时刻在 T±R3_JITTER_MS 之间(T=R3_FINISH_MS,脚本
        // 结束时刻,与轮询任务真实的第一次 poll 大致同时——见
        // `build_scripts` 里 R3 那一段的详细说明)——round 从 1 到 rounds
        // 均匀映射到这个窗口的两端,保证真的扫过整个窗口而不是只抖在中
        // 点附近。
        let jitter_ms: i64 = if rounds <= 1 {
            0
        } else {
            -R3_JITTER_MS + (2 * R3_JITTER_MS * (round as i64 - 1)) / (rounds as i64 - 1)
        };
        let delay_ms = (R3_FINISH_MS.as_millis() as i64 + jitter_ms).max(0) as u64;
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        let cancel_result = ctx.manager.cancel(run_id).await;

        // 给晚到/先到的消息一点余量落定(轮询节奏 250ms,给够一整轮 +
        // 富余)。
        let closed = wait_for_all_closed(&ctx.control, &[run_id], Duration::from_millis(600)).await;
        l.check(
            closed,
            format!("轮次 {round}:关门恰好发生一次(在 600ms 内关门)"),
        );
        l.check(
            cancel_result.is_ok(),
            format!("轮次 {round}:cancel() 调用本身不报错(幂等/正常路径都该是 Ok)"),
        );

        let row1 = ctx.control.get_run(run_id).await.ok().flatten();
        let Some(row1) = row1 else {
            eprintln!("ASSERT FAILED: 轮次 {round}:关门后应该还能读到运行行");
            l.check(false, format!("轮次 {round}:读回运行行"));
            continue;
        };

        l.check(
            row1.settled_at.is_some(),
            format!("轮次 {round}:settled_at 非空"),
        );
        l.check(
            matches!(
                row1.state,
                bw_core::RunState::Canceled | bw_core::RunState::Finished
            ),
            format!(
                "轮次 {round}:state ∈ {{canceled, finished}}(实得 {:?})",
                row1.state
            ),
        );
        match row1.state {
            bw_core::RunState::Canceled => cancel_wins += 1,
            bw_core::RunState::Finished => finish_wins += 1,
            _ => {}
        }

        // 再发一次结算命令(直接走存储层——运行管理器没有暴露"手动结
        // 算一次"的公开方法,这条断言要的是"数据库层面再结一次账,值
        // 不变",不是"运行管理器再暴露一个不该有的口")。
        let second_settle = ctx.control.settle_run(run_id, now() + 999).await;
        let row2 = ctx.control.get_run(run_id).await.ok().flatten();
        match (second_settle, row2) {
            (Ok(false), Some(row2)) if row2.settled_at == row1.settled_at => {
                l.check(
                    true,
                    format!("轮次 {round}:再发一次结算命令后 settled_at 值不变(逐字节比对)"),
                );
            }
            (Ok(true), _) => {
                eprintln!(
                    "ASSERT FAILED: 轮次 {round}:settled_at 应该已经结过账,重结算命令不该再成功"
                );
                l.check(false, format!("轮次 {round}:settled_at 只结算一次"));
            }
            (result, row2) => {
                eprintln!(
                    "ASSERT FAILED: 轮次 {round}:重结算读回异常(result={result:?}, row2.settled_at={:?}, row1.settled_at={:?})",
                    row2.map(|r| r.settled_at), row1.settled_at
                );
                l.check(
                    false,
                    format!("轮次 {round}:settled_at 只结算一次(读回异常)"),
                );
            }
        }

        // 关门后再显式 cancel 一次 → 幂等 Ok,不改字段。
        let idem = ctx.manager.cancel(run_id).await;
        let row3 = ctx.control.get_run(run_id).await.ok().flatten();
        match (idem, row3) {
            (Ok(()), Some(row3))
                if row3.ended_at == row1.ended_at
                    && row3.state == row1.state
                    && row3.end_kind == row1.end_kind =>
            {
                l.check(
                    true,
                    format!("轮次 {round}:关门后再 cancel 一次 → Ok,字段不变(幂等)"),
                );
            }
            (result, row3) => {
                eprintln!(
                    "ASSERT FAILED: 轮次 {round}:关门后再 cancel 应该幂等且不改字段,实得 result={result:?} row={row3:?}"
                );
                l.check(false, format!("轮次 {round}:关门后再 cancel 幂等"));
            }
        }
    }

    println!("胜负分布:{rounds} 轮中取消赢 {cancel_wins} 次、完成赢 {finish_wins} 次");
    if cancel_wins == 0 || finish_wins == 0 {
        eprintln!(
            "警告:{rounds} 轮里有一方全赢(取消赢 {cancel_wins} / 完成赢 {finish_wins})——抖动窗口可能没覆盖到真实的撞车区间,\
             这不代表撞车逻辑有问题,只说明本次没有真的造出双方都赢过的证据,不许把「没撞上」读成「撞了也没事」"
        );
    } else {
        println!("✓ 两种结局本次都真实发生过(不是单边巧合)");
    }

    l
}

// ─────────────────────────── R4 ───────────────────────────

async fn section_r4(ctx: &Ctx) -> Ledger {
    let mut l = Ledger::new();
    println!("脚本:十件活,#3 contact_lost(150ms)· #7 起跑就失败 · 其余 150ms process_exit");

    let mut issues = Vec::with_capacity(10);
    for i in 1..=10 {
        match ctx.new_issue(&format!("R4 示例活 #{i}")).await {
            Ok(id) => issues.push(id),
            Err(e) => {
                l.fail(format!("R4 建活 #{i} 失败:{e}"));
                return l;
            }
        }
    }

    let mut ids: Vec<Option<RunId>> = Vec::with_capacity(10);
    for (idx, issue) in issues.iter().enumerate() {
        let i = idx + 1;
        let req = ctx.mock_start(
            *issue,
            format!("/tmp/run-races-mock/r4-{i}"),
            format!("bw/issue-r4-{i}"),
        );
        match ctx.manager.start(req).await {
            Ok(id) => ids.push(Some(id)),
            Err(e) => {
                // 第 7 件不该在这里失败——`start()` 本身只做到「插行占名
                // 额」,连接器外呼失败是异步回写的(state=Failed),不是
                // `start()` 调用本身报错。到这里报错说明别的地方出了问题。
                eprintln!("ASSERT FAILED: R4 第 {i} 件开工调用本身不该失败:{e}");
                ids.push(None);
            }
        }
    }
    let known_ids: Vec<RunId> = ids.iter().filter_map(|o| *o).collect();
    let closed = wait_for_all_closed(&ctx.control, &known_ids, Duration::from_secs(3)).await;
    l.check(
        closed,
        "十件活的第一次运行全部在 3s 内关门(含起跑失败即刻关门的第 7 件)",
    );

    l.check(
        ids.iter().all(|o| o.is_some()),
        "运行行仍是 10 条(第 7 件也有行,开工调用本身不报错)",
    );

    let mut process_exit_count = 0usize;
    for (idx, id) in ids.iter().enumerate() {
        let i = idx + 1;
        let Some(id) = id else { continue };
        let Ok(Some(row)) = ctx.control.get_run(*id).await else {
            eprintln!("ASSERT FAILED: R4 第 {i} 件读不回运行行");
            l.check(false, format!("第 {i} 件运行行可读回"));
            continue;
        };
        match i {
            3 => {
                l.check(
                    row.end_kind == Some(bw_store::RunEndKind::ContactLost),
                    format!(
                        "第 3 件 end_kind = contact_lost(实得 {:?},不是 process_exit)",
                        row.end_kind
                    ),
                );
            }
            7 => {
                l.check(
                    row.state == bw_core::RunState::Failed
                        && row.end_kind == Some(bw_store::RunEndKind::StartFailed),
                    format!(
                        "第 7 件 state=failed end_kind=start_failed(实得 state={:?} end_kind={:?})",
                        row.state, row.end_kind
                    ),
                );
                l.check(
                    !row.end_detail.is_empty(),
                    "第 7 件 end_detail 非空(连接器的错误原文)",
                );
            }
            _ => {
                if row.end_kind == Some(bw_store::RunEndKind::ProcessExit) {
                    process_exit_count += 1;
                }
                l.check(row.settled_at.is_some(), format!("第 {i} 件已结账"));
            }
        }
    }
    l.check(
        process_exit_count == 8,
        format!("另外 8 件全部 process_exit 且各结账一次(实得 {process_exit_count}/8)"),
    );

    // 第 7 件立刻再开工——名额从没被占死。
    let issue7 = issues[6];
    let retry = ctx
        .manager
        .start(ctx.mock_start(issue7, "/tmp/run-races-mock/r4-7-retry", "bw/issue-r4-7"))
        .await;
    match retry {
        Ok(new_id) => {
            println!("第 7 件立刻再开工成功:{new_id:?}(脚本仍是「起跑就失败」,第二条运行行也会关门为 failed)");
            let _ = wait_for_all_closed(&ctx.control, &[new_id], Duration::from_secs(2)).await;
            let owns_two = match ids[6] {
                Some(first_run) => {
                    count_run_rows_at_least_two_distinct(&ctx.db_path, issue7, first_run, new_id)
                        .await
                }
                None => false,
            };
            l.check(
                owns_two,
                "第 7 件的活可以立刻再开工,新起一条运行行,与第一条编号不同(真实 COUNT ≥ 2 且编号不同)",
            );
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: 第 7 件应该能立刻再开工(名额没被占死),实得:{e}");
            l.check(false, "第 7 件立刻再开工成功");
        }
    }

    l
}

/// R4「第 7 件共 2 条运行、编号不同」的核验(评审 Important-3)。此前这
/// 里只回 `first_run.is_some()`——两个前置参数完全没用,而这件事在调用
/// 点已经被上一条断言(`ids[6]`)验过,是一条恒真断言。改成真的
/// `COUNT(*) >= 2` 加编号不同判定,design §4.2 R4 要的「该活共 2 条运
/// 行、编号不同」才真的被查了一次。
async fn count_run_rows_at_least_two_distinct(
    db_path: &std::path::Path,
    issue: IssueId,
    first_run: RunId,
    second_run: RunId,
) -> bool {
    match count_run_rows_for_issue(db_path, issue).await {
        Some(n) => n >= 2 && first_run != second_run,
        None => false,
    }
}

// ─────────────────────────── R5 ───────────────────────────

async fn section_r5(ctx: &Ctx) -> Ledger {
    let mut l = Ledger::new();

    let starts_before = ctx.mock.starts_issued();

    // **本节故意不用 `ctx.manager`**(那是 R1/R2/R3/R4/R6 等其它节共用的
    // 长命把手,R6 排在本节之后还要用它)——「丢弃管理器 A 的所有把
    // 手」这个动作必须真的让最后一份 `Arc` 归零才会触发收尾守卫,所以
    // 本节自己开一个独立的管理器 A(同一个数据库文件、同一份注册表),
    // 只在这个函数的作用域里存在。
    let manager_a = match RunManager::open(
        &ctx.db_path,
        ctx.registry.clone(),
        RunManagerConfig::default(),
    )
    .await
    {
        Ok(m) => m,
        Err(e) => {
            l.fail(format!("起管理器 A 失败:{e}"));
            return l;
        }
    };

    let mut issues = Vec::with_capacity(3);
    let mut ids = Vec::with_capacity(3);
    for i in 1..=3 {
        let issue = match ctx.new_issue(&format!("R5 示例活 #{i}")).await {
            Ok(id) => id,
            Err(e) => {
                l.fail(format!("R5 建活 #{i} 失败:{e}"));
                return l;
            }
        };
        let req = ctx.mock_start(
            issue,
            format!("/tmp/run-races-mock/r5-{i}"),
            format!("bw/issue-r5-{i}"),
        );
        match manager_a.start(req).await {
            Ok(id) => {
                issues.push(issue);
                ids.push(id);
            }
            Err(e) => {
                l.fail(format!("R5 第 {i} 件开工失败:{e}"));
                return l;
            }
        }
    }
    // 等三条运行行都真的离开 Starting——不是猜一个固定 sleep 够不够长
    // (同下面「重开工」那一步的理由:`start()` 只保证插行完成就返回,
    // 连接器外呼是后台任务)。
    let mut all_left_starting = true;
    for id in &ids {
        let settled = wait_for_state_left_starting(&ctx.control, *id, Duration::from_secs(2)).await;
        if !settled {
            eprintln!("ASSERT FAILED: R5 初始三件里有一条 2s 内没能离开 Starting({id:?})");
            all_left_starting = false;
        }
    }
    l.check(all_left_starting, "R5 初始三件全部在 2s 内离开 Starting");

    // 「重启」在进程内的模拟:丢弃管理器 A 的**全部**把手——本节只创建
    // 过这一份,`drop` 让它的 `Arc` 引用计数归零,触发收尾守卫(循环任
    // 务与轮询任务随之退出),再用同一个数据库文件新建管理器 B。**没有
    // 杀掉上游会话**——这是对「进程被杀」诚实模拟的边界(design §4.2 R5
    // 的第二条注记),真进程被杀时上游会话仍活着,mock 的 `starts_issued`
    // 计数器同样不会因为管理器 A 消失而减少,恰好用来证明 reap 之后它
    // 也不会增加。
    drop(manager_a);

    let manager_b = match RunManager::open(
        &ctx.db_path,
        ctx.registry.clone(),
        RunManagerConfig::default(),
    )
    .await
    {
        Ok(m) => m,
        Err(e) => {
            l.fail(format!("重启后新建管理器 B 失败:{e}"));
            return l;
        }
    };

    // 复审「修复复审」新发现 1 / 本轮简报第 1 条:评审 Important-2 的
    // 修复(`handle_cancel` 对「库里还开着、但不在内存」的运行真做一次
    // `close_run`,不再假报 `Ok`)此前在这份指挥器里零覆盖——panic 探针
    // 实测过(192 条断言一次都没走到那条分支)。这里正好是「库里开着但
    // 内存没有」这条边缘态天然成立的地方:管理器 B 刚开、内存三张表全
    // 空,而 `ids[2]` 这条运行(管理器 A 起的、永不结束的脚本)在数据库
    // 里仍然 `ended_at IS NULL`。在调 `reap_on_restart` **之前**对它调一
    // 次 `cancel`,让这条分支被真的走到,断言其效果(运行被真的关门、
    // 名额在存储层真的释放,不是打日志就当没发生过)。
    let late_cancel = manager_b.cancel(ids[2]).await;
    l.check(
        late_cancel.is_ok(),
        format!("cancel(库里开着、不在内存)返回 Ok(实得 {late_cancel:?})"),
    );
    match ctx.control.get_run(ids[2]).await {
        Ok(Some(row)) => {
            l.check(
                row.ended_at.is_some(),
                "cancel(不在内存)之后:ended_at 非空(真的被关过门,不是假报 Ok 却什么都没改)",
            );
            l.check(
                row.state == bw_core::RunState::Canceled,
                format!("cancel(不在内存)之后:state=canceled(实得 {:?})", row.state),
            );
            l.check(
                row.end_kind == Some(bw_store::RunEndKind::Canceled),
                format!(
                    "cancel(不在内存)之后:end_kind=canceled(实得 {:?})",
                    row.end_kind
                ),
            );
            l.check(
                row.settled_at.is_some(),
                "cancel(不在内存)之后:settled_at 非空(账已结)",
            );
            l.check(
                row.end_detail.contains("无法确认上游会话是否已真正停止"),
                format!(
                    "cancel(不在内存)之后:end_detail 如实写明未持有连接器句柄(实得 {:?})",
                    row.end_detail
                ),
            );
        }
        other => {
            eprintln!(
                "ASSERT FAILED: cancel(不在内存)之后读不回运行 {:?}:{other:?}",
                ids[2]
            );
            l.check(false, "cancel(不在内存)之后运行可读回");
        }
    }
    // 名额真的在存储层释放了:直接查「这件活还有没有活跃交付运行」——
    // 这条查询正是 `create_run` 撞唯一索引时用来回填「是哪一条」的同一
    // 条(`find_live_delivery_run`,manager.rs §3.4 第二行「存储才是真正
    // 的守卫」),不依赖任何内存缓存(管理器 B 的 `by_issue` 本来就是空
    // 的,这条查询才是唯一有意义的证据)。
    let slot_released = ctx.control.find_live_delivery_run(issues[2]).await;
    l.check(
        matches!(slot_released, Ok(None)),
        format!(
            "cancel(不在内存)之后:该活在存储层已无活跃交付运行,名额真释放(实得 {slot_released:?})"
        ),
    );

    let report = match manager_b.reap_on_restart().await {
        Ok(r) => r,
        Err(e) => {
            l.fail(format!("reap_on_restart 失败:{e}"));
            return l;
        }
    };
    println!(
        "reap_on_restart 收拾了 {} 条运行:{:?}(ids[2] 已经在上面被 cancel 提前关门,不在这份名单里)",
        report.reaped.len(),
        report.reaped
    );

    let mut orphaned_count = 0usize;
    let mut finished_process_exit_count = 0usize;
    for id in &ids {
        let Ok(Some(row)) = ctx.control.get_run(*id).await else {
            eprintln!("ASSERT FAILED: R5 读不回运行 {id:?}");
            l.check(false, format!("运行 {id:?} 可读回"));
            continue;
        };
        if row.state == bw_core::RunState::Orphaned {
            orphaned_count += 1;
        }
        if row.state == bw_core::RunState::Finished
            && row.end_kind == Some(bw_store::RunEndKind::ProcessExit)
        {
            finished_process_exit_count += 1;
        }
        if row.state == bw_core::RunState::Orphaned {
            l.check(
                row.ended_at.is_some(),
                format!("运行 {id:?}:ended_at 非空(名额已释放)"),
            );
            l.check(
                row.settled_at.is_none(),
                format!("运行 {id:?}:settled_at 为空(账没结,如实欠着)"),
            );
            l.check(
                row.end_kind.is_none(),
                format!("运行 {id:?}:end_kind 为 NULL(不知道怎么结束的就是不知道)"),
            );
        }
    }
    l.check(
        orphaned_count == 2,
        format!(
            "2 条 state=orphaned(实得 {orphaned_count})——第 3 条上面已经被 cancel 提前关门(canceled),不算在 reap 的收拾范围内"
        ),
    );
    l.check(
        finished_process_exit_count == 0,
        format!("反证:state=finished AND end_kind=process_exit 的行数为 0(实得 {finished_process_exit_count})——没有一条被写成「跑完了」"),
    );

    // 名额真的放开了:对其中一件活重新 start 成功。
    let reopen = manager_b
        .start(ctx.mock_start(
            issues[0],
            "/tmp/run-races-mock/r5-1-reopen",
            "bw/issue-r5-1",
        ))
        .await;
    let reopened_id = match reopen {
        Ok(new_id) => {
            l.check(
                new_id != ids[0],
                "重新 start 成功,新运行编号与遗留的那条不同",
            );
            Some(new_id)
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: reap 之后名额应该已经放开,重新 start 应该成功,实得:{e}");
            l.check(false, "reap 之后重新 start 成功");
            None
        }
    };

    // `start()` 只保证「插行占名额」这一步已经做完就返回(design §3.4
    // ⑤「只起不等」)——连接器那次真实的 `Execute::start` 外呼是后台
    // `tokio::spawn` 出去的,`starts_issued` 计数器要等那个后台任务真的
    // 跑到才会 +1。这里等这条重开工的运行行状态离开 `Starting`(变成
    // `Running`),确保外呼真的完成过,再去读计数器——否则会读到一个
    //「还没来得及 +1」的瞬时值,是本节此前偶发失败的根因,不是 reap 逻
    // 辑本身有问题。
    if let Some(id) = reopened_id {
        let settled = wait_for_state_left_starting(&ctx.control, id, Duration::from_secs(2)).await;
        l.check(
            settled,
            "重开工的运行行在 2s 内离开 Starting(连接器外呼真的跑完了)",
        );
    }

    // 不批量唤醒:reap 前后 mock 连接器的活票据数没有增加(reap 只标
    // 注,不重起任何会话)。
    let starts_after_reap = ctx.mock.starts_issued();
    // 注意:上面的「重新 start」调用会让计数 +1(它是一次真实的新开
    // 工,不是 reap 造成的)——这里比较的是 reap 调用本身前后的差值,不
    // 含这次重开工。
    l.check(
        starts_after_reap == starts_before + 3 + 1, // 3 = 本节最初起的三件;+1 = 上面的重开工;reap 本身贡献 0
        format!(
            "reap 本身贡献 0 次新起工(reap 前 {starts_before},最初三件 +3,重开工 +1,reap 后实得 {starts_after_reap})"
        ),
    );

    l
}

/// 轮询等一条运行行的 `state` 离开 `Starting`(变成 `Running` 或任何终
/// 态)——用来确认「插行占名额」之后紧跟着的连接器外呼(后台
/// `tokio::spawn`)真的执行过,不是靠一个固定 `sleep` 赌时间够不够长。
async fn wait_for_state_left_starting(
    control: &SqliteStore,
    id: RunId,
    deadline: Duration,
) -> bool {
    let start = Instant::now();
    loop {
        if let Ok(Some(row)) = control.get_run(id).await {
            if row.state != bw_core::RunState::Starting {
                return true;
            }
        }
        if start.elapsed() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ─────────────────────────── R6 ───────────────────────────

async fn section_r6(ctx: &Ctx) -> Ledger {
    let mut l = Ledger::new();

    let issue = match ctx.new_issue("R6 示例活").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("R6 建活失败:{e}"));
            return l;
        }
    };

    let req_a = ctx.mock_start(issue, "/tmp/run-races-mock/r6-a", "bw/issue-r6-a");
    let run_a = match ctx.manager.start(req_a).await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("R6 运行 A 开工失败:{e}"));
            return l;
        }
    };

    tokio::time::sleep(Duration::from_millis(100)).await;
    if let Err(e) = ctx.manager.cancel(run_a).await {
        l.fail(format!("R6 100ms 时取消 A 失败:{e}"));
    }
    let row_a_after_cancel = ctx.control.get_run(run_a).await.ok().flatten();
    let Some(row_a_after_cancel) = row_a_after_cancel else {
        l.fail("R6 取消后读不回 A");
        return l;
    };
    l.check(
        row_a_after_cancel.state == bw_core::RunState::Canceled,
        format!(
            "A 取消后立刻是 canceled(实得 {:?})",
            row_a_after_cancel.state
        ),
    );

    // **评审 Critical-2 的修复**:design §4.2 R6 的原意是「A 的轮询任务
    // 观测到一条晚到的 Finished,送回后被诚实空转」——但 RunManager 的
    // 真实取消语义(design §3.5②/主控裁决 #6)是取消令牌一响,A 的轮询
    // 任务在下一次 `select!` 立刻退出(`manager.rs` `spawn_poll_task`),
    // 不会再有第二次 `poll()`;mock 的 `poll()` 又是「取消过就恒报
    // Canceled」(design §4.1 四行逻辑第一行)。两条设计合在一起,「走轮
    // 询任务的真实晚到消息」在这份指挥器里**造不出来**——这是评审给的根
    // 因辨析,不是实现疏漏。指挥器因此直接对存储层重放一次「晚到的
    // close_run 调用」,绕过 RunManager,模拟 A 的轮询任务本该在 300ms
    // 观测到、但结构上永远发不出的那条晚到 Finished——如实标注这是指挥
    // 器造出来的,不是走真实轮询任务(评审「该补什么」选项二,与 R3 的
    // 「再结一次账值不变」同款形态,补的是「关门一次」这把此前零覆盖的
    // 守卫)。
    let late_at = row_a_after_cancel.ended_at.unwrap_or(0) + 999;
    match ctx
        .control
        .close_run(
            run_a,
            late_at,
            bw_core::RunState::Finished,
            Some(bw_store::RunEndKind::ProcessExit),
            "【模拟晚到】指挥器直接对 close_run 发起(绕过 RunManager),模拟 A 的轮询任务本该在 \
             300ms 观测到、但结构上不会真的发生的晚到 Finished 消息",
        )
        .await
    {
        Ok(false) => {
            l.check(
                true,
                "晚到的完成消息(直接对 close_run 发起,绕过 RunManager)没能改写已关门的 A——关门只发生一次",
            );
        }
        Ok(true) => {
            l.check(
                false,
                "晚到的完成消息不该改写已关门的 A,但 close_run 的比较并置居然赢了——关门发生了不止一次",
            );
        }
        Err(e) => {
            l.fail(format!("模拟晚到消息的 close_run 调用本身出错:{e}"));
        }
    }
    match ctx.control.get_run(run_a).await.ok().flatten() {
        Some(row) => {
            l.check(
                row.state == row_a_after_cancel.state
                    && row.end_kind == row_a_after_cancel.end_kind
                    && row.ended_at == row_a_after_cancel.ended_at
                    && row.end_detail == row_a_after_cancel.end_detail,
                "晚到消息之后 A 的 state/end_kind/ended_at/end_detail 与关门那一刻逐字节一致(晚到消息一个字段都没改动)",
            );
        }
        None => {
            l.fail("R6 晚到消息之后读不回 A");
        }
    }

    // 紧接着对同一件活起运行 B(名额已释放)。
    let req_b = ctx.mock_start(issue, "/tmp/run-races-mock/r6-b", "bw/issue-r6-b");
    let run_b = match ctx.manager.start(req_b).await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("R6 名额应该已经释放,B 开工应该成功,实得:{e}"));
            return l;
        }
    };

    // 等过 300ms(A 原脚本的结束时刻)——A 的轮询任务此时早已因为取消令
    // 牌退出(见上面的说明),这里只是让时间线走完,不再指望会有第二条
    // 真实晚到消息。
    tokio::time::sleep(Duration::from_millis(400)).await;

    let row_a_final = ctx.control.get_run(run_a).await.ok().flatten();
    match row_a_final {
        Some(row) => {
            l.check(
                row.state == bw_core::RunState::Canceled
                    && row.end_kind == Some(bw_store::RunEndKind::Canceled)
                    && row.ended_at == row_a_after_cancel.ended_at,
                format!(
                    "A 的 state/end_kind/ended_at 与关门那一刻逐字节一致(实得 state={:?} end_kind={:?} ended_at={:?}→{:?})",
                    row.state, row.end_kind, row_a_after_cancel.ended_at, row.ended_at
                ),
            );
        }
        None => {
            eprintln!("ASSERT FAILED: R6 读不回 A 的最终状态");
            l.check(false, "A 最终状态可读回");
        }
    }

    let closed_b = wait_for_all_closed(&ctx.control, &[run_b], Duration::from_secs(2)).await;
    l.check(closed_b, "B 在 2s 内关门(脚本 600ms,加上轮询节奏富余)");
    match ctx.control.get_run(run_b).await.ok().flatten() {
        Some(row) => {
            l.check(
                row.state == bw_core::RunState::Finished
                    && row.end_kind == Some(bw_store::RunEndKind::ProcessExit),
                format!(
                    "B 的 state=finished end_kind=process_exit(实得 state={:?} end_kind={:?})——A 的晚到消息没有落到 B 头上",
                    row.state, row.end_kind
                ),
            );
        }
        None => {
            eprintln!("ASSERT FAILED: R6 读不回 B");
            l.check(false, "B 可读回");
        }
    }

    // 全库结过账的运行数是 2(A + B),不是 3(没有第三条被凭空结算)。
    let a_settled = ctx
        .control
        .get_run(run_a)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.settled_at)
        .is_some();
    let b_settled = ctx
        .control
        .get_run(run_b)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.settled_at)
        .is_some();
    l.check(a_settled && b_settled, "A、B 各结账一次(两条都非空)");

    l
}

// ─────────────────────────── 附 · 降级为咨询 ───────────────────────────

/// design §3.5「降级为咨询」的显式覆盖(评审 Critical-1)。此前
/// `RunManager::demote_to_consultation`/`Loop::handle_demote`/
/// `RunStore::demote_run` 三处代码在整份指挥器里**没有任何调用点**——169
/// 条断言一条都没验过这条语义,突变检验把 kind 翻面和名额释放同时拆掉,
/// 断言照样全过。这里补上设计稿附带的验证脚本:起一条长脚本交付运行 →
/// 降级返回 `true` → 读回 kind 翻面/`demoted_at` 非空/`settled_at` 非空/
/// `ended_at` 仍空(会话没被杀)→ 同一件活立刻能开出新的交付运行(名额
/// 真释放)→ 再降级一次返回 `false`(幂等)→ 等原会话自然结束,读回
/// `ended_at` 落值但 `settled_at` 逐字节不变(账不重结,commit C 标题承
/// 诺的那半)。
async fn section_demote(ctx: &Ctx) -> Ledger {
    let mut l = Ledger::new();

    let issue = match ctx.new_issue("降级为咨询示例活").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("降级示例建活失败:{e}"));
            return l;
        }
    };

    let req_a = ctx.mock_start(issue, "/tmp/run-races-mock/demote-a", "bw/issue-demote-a");
    let run_a = match ctx.manager.start(req_a).await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("降级示例:交付运行 A 开工失败:{e}"));
            return l;
        }
    };
    if !wait_for_state_left_starting(&ctx.control, run_a, Duration::from_secs(2)).await {
        l.fail("降级示例:A 应该在 2s 内离开 Starting");
        return l;
    }

    // 降级:kind 翻面 + 结账,会话不杀、工作区不清(design §3.5)。
    match ctx.manager.demote_to_consultation(run_a).await {
        Ok(true) => l.check(true, "第一次降级返回 true(这次真降级了)"),
        Ok(false) => l.check(false, "第一次降级应该返回 true,实得 false"),
        Err(e) => {
            l.fail(format!("第一次降级调用出错:{e}"));
            return l;
        }
    }

    let row_after_demote = match ctx.control.get_run(run_a).await.ok().flatten() {
        Some(row) => row,
        None => {
            l.fail("降级后读不回 A");
            return l;
        }
    };
    l.check(
        row_after_demote.kind == bw_store::RunKind::Consultation,
        format!(
            "降级后 kind 翻面为 consultation(实得 {:?})",
            row_after_demote.kind
        ),
    );
    l.check(
        row_after_demote.demoted_at.is_some(),
        "降级后 demoted_at 非空",
    );
    l.check(
        row_after_demote.settled_at.is_some(),
        "降级后 settled_at 非空(账在降级这一刻结)",
    );
    l.check(
        row_after_demote.ended_at.is_none(),
        "降级后 ended_at 仍为空(会话没被杀,不冒充已结束)",
    );
    let settled_at_at_demote = row_after_demote.settled_at;

    // 同一件活立刻能开出新的交付运行——名额真的被释放了。
    let req_b = ctx.mock_start(issue, "/tmp/run-races-mock/demote-b", "bw/issue-demote-b");
    let run_b = match ctx.manager.start(req_b).await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("降级后同一件活应该能立刻再开工,实得:{e}"));
            return l;
        }
    };
    l.check(
        run_b != run_a,
        "降级后新起的交付运行是一条新行,编号与被降级的那条不同",
    );

    // 再降级一次——幂等,返回 false(已经不是「活着的交付运行」)。
    match ctx.manager.demote_to_consultation(run_a).await {
        Ok(false) => l.check(true, "第二次降级返回 false(幂等,没什么可降的了)"),
        Ok(true) => l.check(false, "第二次降级不该再返回 true,实得 true——降级不是幂等的"),
        Err(e) => l.fail(format!("第二次降级调用出错:{e}")),
    }

    // 等原会话(A)自然结束——总脚本 900ms,前面几步已经用掉一些时间,给
    // 够余量。
    let closed = wait_for_all_closed(&ctx.control, &[run_a], Duration::from_secs(3)).await;
    l.check(
        closed,
        "A 的原会话在 3s 内自然结束(降级不杀会话,只是不再占名额)",
    );
    match ctx.control.get_run(run_a).await.ok().flatten() {
        Some(row) => {
            l.check(
                row.ended_at.is_some(),
                "A 自然结束后 ended_at 落值(会话真的跑完了)",
            );
            l.check(
                row.state == bw_core::RunState::Finished
                    && row.end_kind == Some(bw_store::RunEndKind::ProcessExit),
                format!(
                    "A 自然结束后 state=finished end_kind=process_exit(实得 state={:?} end_kind={:?})",
                    row.state, row.end_kind
                ),
            );
            l.check(
                row.settled_at == settled_at_at_demote,
                format!(
                    "A 自然结束后 settled_at 与降级那一刻逐字节不变(账不重结,实得 {:?} → {:?})",
                    settled_at_at_demote, row.settled_at
                ),
            );
        }
        None => l.fail("A 自然结束后读不回"),
    }

    l
}

// ─────────────────────────── 附 · 同工作区串线校验 ───────────────────────────

async fn section_workspace_guard(ctx: &Ctx) -> Ledger {
    let mut l = Ledger::new();

    let issue_a = match ctx.new_issue("同工作区校验 · 活 A").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("建活 A 失败:{e}"));
            return l;
        }
    };
    let issue_b = match ctx.new_issue("同工作区校验 · 活 B").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("建活 B 失败:{e}"));
            return l;
        }
    };

    let shared_workspace = "/tmp/run-races-mock/wscheck-shared";
    let run_a = match ctx
        .manager
        .start(ctx.mock_start(issue_a, shared_workspace, "bw/issue-wscheck-a"))
        .await
    {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("活 A 用这个工作区开工应该成功,实得:{e}"));
            return l;
        }
    };

    match ctx
        .manager
        .start(ctx.mock_start(issue_b, shared_workspace, "bw/issue-wscheck-b"))
        .await
    {
        Err(bw_app::run::RunError::WorkspaceBusy { existing, .. }) => {
            l.check(
                existing == run_a,
                "活 B 用同一个工作区开工被如实拒绝(WorkspaceBusy,带出占用者的运行编号)",
            );
        }
        Err(other) => {
            eprintln!("ASSERT FAILED: 活 B 应该被 WorkspaceBusy 拒绝,实得别的错误:{other}");
            l.check(false, "活 B 被 WorkspaceBusy 正确分类拒绝");
        }
        Ok(id) => {
            eprintln!("ASSERT FAILED: 活 B 不该被允许用活跃的同一个工作区开工,实得成功:{id:?}");
            l.check(false, "活 B 被工作区占用挡下");
        }
    }

    // 收尾:取消 A,释放工作区,不留后续断言依赖的悬空状态。
    let _ = ctx.manager.cancel(run_a).await;

    l
}

// ─────────────────────────── 附 · 完成永远人点 ───────────────────────────

async fn section_done_is_human(ctx: &Ctx) -> Ledger {
    let mut l = Ledger::new();

    let app = match App::open(&ctx.db_path.to_string_lossy()).await {
        Ok(a) => a,
        Err(e) => {
            l.fail(format!("App::open 失败:{e}"));
            return l;
        }
    };

    let issue_review = match ctx.new_issue("完成永远人点 · 手工置评审中").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("建活失败:{e}"));
            return l;
        }
    };
    // 手工把一件活置成「评审中」——**本片手工置,真实来源是切片五的 MR
    // 巡检**(design §5.2 第 8 条后的显式验证段)。走两步合法转移(活默
    // 认 backlog:backlog→in_progress→in_review 都是合法边)。
    if let Err(e) = app
        .transition_issue(issue_review, IssueStatus::InProgress)
        .await
    {
        eprintln!("ASSERT FAILED: backlog → in_progress 应该合法,实得:{e}");
        l.check(false, "backlog → in_progress 合法转移成功");
        return l;
    }
    if let Err(e) = app
        .transition_issue(issue_review, IssueStatus::InReview)
        .await
    {
        eprintln!("ASSERT FAILED: in_progress → in_review 应该合法,实得:{e}");
        l.check(false, "in_progress → in_review 合法转移成功");
        return l;
    }
    println!("(本片手工置「评审中」,真实来源是切片五的 MR 巡检)");

    match app.transition_issue(issue_review, IssueStatus::Done).await {
        Ok(()) => l.check(true, "InReview → Done 显式转移成功(Done 的唯一入边)"),
        Err(e) => {
            eprintln!("ASSERT FAILED: InReview → Done 应该成功,实得:{e}");
            l.check(false, "InReview → Done 显式转移成功");
        }
    }

    // 反证:另一件「进行中」的活试转「已完成」,必须被合法转移表拒绝。
    let issue_in_progress = match ctx.new_issue("完成永远人点 · 反证活").await {
        Ok(id) => id,
        Err(e) => {
            l.fail(format!("建反证活失败:{e}"));
            return l;
        }
    };
    if let Err(e) = app
        .transition_issue(issue_in_progress, IssueStatus::InProgress)
        .await
    {
        eprintln!("ASSERT FAILED: 反证活 backlog → in_progress 应该合法,实得:{e}");
        l.check(false, "反证活推进到 in_progress");
        return l;
    }
    match app
        .transition_issue(issue_in_progress, IssueStatus::Done)
        .await
    {
        Err(bw_app::AppError::IllegalTransition { .. }) => {
            l.check(true, "进行中 → 已完成 被合法转移表拒绝(Done 只有一个入口)");
        }
        Err(other) => {
            eprintln!("ASSERT FAILED: 应该是 IllegalTransition,实得别的错误:{other}");
            l.check(false, "进行中 → 已完成 被正确分类拒绝");
        }
        Ok(()) => {
            eprintln!("ASSERT FAILED: 进行中 → 已完成 不该被允许,但是成功了——Done 有了第二个入口");
            l.check(false, "进行中 → 已完成 被拒绝");
        }
    }

    l
}

// ─────────────────────────── 附 · sqlite 读回清单 ───────────────────────────

/// 把一行 SQL 读回原样打成 `col=val col2=val2 …`(评审 Important-4)。
/// SQLite 是动态类型,这里按整数/浮点/文本三档级联尝试;`run`/`issue`
/// 两张表 + 本节用到的 PRAGMA 结果全是这三类之一,取不到才标
/// `<unreadable>`(设计上不会真的走到)。
fn format_row(row: &sqlx::sqlite::SqliteRow) -> String {
    use sqlx::{Column, Row, ValueRef};
    row.columns()
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let name = col.name();
            let is_null = row.try_get_raw(i).map(|v| v.is_null()).unwrap_or(false);
            let value = if is_null {
                "NULL".to_string()
            } else if let Ok(v) = row.try_get::<i64, _>(i) {
                v.to_string()
            } else if let Ok(v) = row.try_get::<f64, _>(i) {
                v.to_string()
            } else if let Ok(v) = row.try_get::<String, _>(i) {
                v
            } else {
                "<unreadable>".to_string()
            };
            format!("{name}={value}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

async fn section_sql_readback(db_path: &std::path::Path, r1_ok: bool) -> Ledger {
    let mut l = Ledger::new();
    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            l.fail(format!("无法用独立连接打开数据库:{e}"));
            return l;
        }
    };

    let queries: [(&str, &str); 9] = [
        ("1 结构核验", "PRAGMA table_info(run)"),
        ("2 部分唯一索引", "SELECT name, partial FROM pragma_index_list('run')"),
        ("3a 总量", "SELECT COUNT(*) AS n FROM run"),
        ("3b 互不覆盖", "SELECT issue_id, COUNT(*) AS n FROM run GROUP BY issue_id ORDER BY n DESC LIMIT 5"),
        ("4 结算一次(总量)", "SELECT COUNT(*) AS n FROM run WHERE settled_at IS NOT NULL"),
        ("6 各状态分布", "SELECT state, end_kind, COUNT(*) AS n FROM run GROUP BY state, end_kind"),
        ("7 重启遗留如实标注", "SELECT id, issue_id, state, end_kind, ended_at, settled_at FROM run WHERE state = 'orphaned'"),
        ("8 完成永远人点(已完成计数)", "SELECT COUNT(*) AS n FROM issue WHERE status = 'done'"),
        ("9 待人处理形状", "SELECT id, issue_id, state FROM run WHERE ended_at IS NOT NULL AND settled_at IS NULL"),
    ];
    for (label, sql) in queries {
        println!("[{label}] 执行 SQL: {sql}");
        match sqlx::query(sql).fetch_all(&pool).await {
            Ok(rows) => {
                // 评审 Important-4:原样打印每一行的值,不是只打行数——
                // 此前 `COUNT(*)` 那几条一律打「→ 1 行」,COUNT 天生只
                // 返回 1 行,真正要给人看的那个数字一个都没打出来,读的
                // 人还容易把「1 行」误读成「总量 1」。「读回为证」的意
                // 思是人能拿这条 SQL 自己 `sqlite3` 复制去核对,不是
                // 「SQL 没报错」——这里补上去,同时把「读回成功」变成一
                // 条真断言(此前成功路径不打任何 `l.check`,一条断言都
                // 没注册)。
                if rows.is_empty() {
                    println!("  → (0 行)");
                } else {
                    for row in &rows {
                        println!("  → {}", format_row(row));
                    }
                }
                l.check(
                    true,
                    format!("[{label}] SQL 读回成功({} 行,原文见上)", rows.len()),
                );
            }
            Err(e) => {
                l.fail(format!("[{label}] 读回失败:{e}"));
            }
        }
    }

    // 第 8 条的补充断言:全部竞态跑完之后,「已完成」的活数应该恰好是
    // `section_done_is_human` 那一条(=1)——除非 R1 之类的失败牵连了后
    // 续状态,这里如实读回,不预设具体数字为 0(确实应该有 1 条,由前面
    // 的显式转移产生)。
    match sqlx::query("SELECT COUNT(*) AS n FROM issue WHERE status = 'done'")
        .fetch_one(&pool)
        .await
    {
        Ok(row) => {
            let n: i64 = row.get("n");
            l.check(
                n == 1,
                format!("恰好 1 件活是「已完成」(唯一入口是显式转移,实得 {n})"),
            );
        }
        Err(e) => {
            l.fail(format!("读回「已完成」计数失败:{e}"));
        }
    }

    let _ = r1_ok; // 仅用于让调用方知道 R1 是否已经跑过(本节自身不依赖它的具体结果)。
    l
}

// ─────────────────────────── 附 · 存量库迁移双守卫 ───────────────────────────

/// 一份「少一列的老 schema」——`run` 表故意不含 `demoted_at`,模拟切片四B
/// 之前的库。**不经过 `SqliteStore::open`**(那样会用当前 schema.sql 把列
/// 建全,测不出「老库缺列」这件事),独立连接手工执行。
const OLD_SCHEMA_MISSING_DEMOTED_AT: &str = "
CREATE TABLE IF NOT EXISTS project (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    root_path   TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS issue (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id),
    number      INTEGER NOT NULL,
    title       TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'backlog',
    settled_at  INTEGER,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_issue_project_number ON issue(project_id, number);
CREATE TABLE IF NOT EXISTS run (
    id                TEXT PRIMARY KEY,
    project_id        TEXT NOT NULL REFERENCES project(id),
    issue_id          TEXT NOT NULL REFERENCES issue(id),
    kind              TEXT NOT NULL DEFAULT 'delivery',
    connector_name    TEXT NOT NULL,
    req_id            TEXT NOT NULL,
    upstream_session  TEXT NOT NULL DEFAULT '',
    workspace         TEXT NOT NULL,
    branch            TEXT NOT NULL,
    state             TEXT NOT NULL,
    end_kind          TEXT,
    end_detail        TEXT NOT NULL DEFAULT '',
    started_at        INTEGER NOT NULL,
    ended_at          INTEGER,
    settled_at        INTEGER
);
CREATE UNIQUE INDEX IF NOT EXISTS uq_run_live_delivery_per_issue
    ON run(issue_id) WHERE ended_at IS NULL AND kind = 'delivery';
CREATE INDEX IF NOT EXISTS idx_run_project_started ON run(project_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_run_open ON run(issue_id) WHERE ended_at IS NULL;
";

async fn section_migration_guard() -> Ledger {
    let mut l = Ledger::new();

    let old_db =
        std::env::temp_dir().join(format!("{DB_NAME_PREFIX}old-schema-{}.db", Uuid::new_v4()));
    let _ = std::fs::remove_file(&old_db);

    let opts = SqliteConnectOptions::new()
        .filename(&old_db)
        .create_if_missing(true);
    let pool = match SqlitePool::connect_with(opts).await {
        Ok(p) => p,
        Err(e) => {
            l.fail(format!("造老库失败:{e}"));
            return l;
        }
    };
    for stmt in OLD_SCHEMA_MISSING_DEMOTED_AT.split(';') {
        if stmt.trim().is_empty() {
            continue;
        }
        if let Err(e) = sqlx::query(stmt).execute(&pool).await {
            l.fail(format!("老库建表语句失败:{e}\n语句:{stmt}"));
            return l;
        }
    }
    pool.close().await;

    // 用正式开库流程打开这个老库——`SqliteStore::open` 内部的
    // `add_column_if_missing` 应该把 `demoted_at` 补上。
    match SqliteStore::open(&old_db.to_string_lossy()).await {
        Ok(store) => drop(store),
        Err(e) => {
            eprintln!("ASSERT FAILED: 用正式开库流程打开老库失败:{e}");
            l.check(false, "正式开库流程能打开少一列的老库");
            return l;
        }
    }

    let opts2 = SqliteConnectOptions::new()
        .filename(&old_db)
        .read_only(true);
    let pool2 = match SqlitePool::connect_with(opts2).await {
        Ok(p) => p,
        Err(e) => {
            l.fail(format!("老库读回连接失败:{e}"));
            return l;
        }
    };
    match sqlx::query("PRAGMA table_info(run)")
        .fetch_all(&pool2)
        .await
    {
        Ok(rows) => {
            let has_demoted_at = rows
                .iter()
                .any(|r| r.get::<String, _>("name") == "demoted_at");
            println!(
                "PRAGMA table_info(run) 读回:demoted_at 列{}",
                if has_demoted_at { "存在" } else { "缺失" }
            );
            l.check(
                has_demoted_at,
                "少一列的老库打开后,PRAGMA table_info(run) 读回该列(add_column_if_missing 生效)",
            );
        }
        Err(e) => {
            eprintln!("ASSERT FAILED: PRAGMA table_info(run) 读回失败:{e}");
            l.check(false, "PRAGMA table_info(run) 读回成功");
        }
    }

    l
}

// ─────────────────────────── 附 · --real ───────────────────────────

const REAL_CONNECTOR_NAME: &str = "real-claude";

/// 真实并行三件(design §5.3)。**评审 Important-6 的修复**:此前这一节
/// 直接拿 `AgentCliConnector` 调 `execute.start/poll/cancel`,完全绕开了
/// 运行管理器——没有 `RunManager::start`,没有一条运行行落库,design
/// §5.3 要的「3 条运行行 / 3 个互不相同的上游会话号 / 3 个互不相同的工
/// 作区路径 / 各结账一次」这组读回断言因此一条都没落地(它证的是连接器
/// 层能不能起三个会话,不是「真实并行三件经运行管理器跑通」)。这里改
/// 起一个独立的 `RunManager`(同一个数据库文件,注册表里换成真实
/// `AgentCliConnector`),开工/取消走 `StartRun`/`cancel`,断言全部走
/// `ctx.control.get_run` 读回。
///
/// **评审 Important-5 的修复**:返回 `bool`——`main` 把它并进最终退出
/// 码,不再是「打印一行 REAL_RUN_FAILED 但退出码恒 0」。design §5.3 结
/// 尾原文:「显式传了 `--real` 但连接器不可用 → 打一行如实的原因并**以
/// 退出码 1 结束**。悄悄回落到 mock 再报成功,是本仓库最恨的那种假
/// 绿」。这里把「连接器/环境不可用」(建工作区失败、git 初始化失败、
/// `RunManager::open`/`start` 本身报错)与「上游会话卡在 plan/23 §10 第
/// 6 条的双重确认对话框,90 秒内没能产出 jsonl 会话文件」区分开:前者
/// 是这份指挥器自己的装配/环境问题,判定这一档失败;后者是已登记的已
/// 知外部缺口(不是本次改动要修的东西),如实打印但不影响这一档的判
/// 定——`RunManager` 侧的行/会话号/工作区/结账读回断言该过的都要过。
///
/// **如实预期**:全新工作区首次交互式启动会先撞
/// plan/23-opc-stitching-rebuild.md §10 第 6 条(双重确认对话框,默认退
/// 出)——先试,撞上就如实打印,不为绕过对话框替用户自动应答安全确认。
async fn section_real(ctx: &Ctx, parent_dir: &std::path::Path, n: usize) -> bool {
    use bw_engine::agentcli::AgentCliConnector;
    use bw_engine::interactive_cli::{InteractiveCliExecutor, CLAUDE};

    let n = n.clamp(1, 3); // §5.3 的档位规模是三件;`--n` 给了更大的值也不在这里放大成本
    println!("前置:claude 在 PATH + 给定的父目录用于新建 {n} 个真实 git 检出,经 RunManager 起工(评审 Important-6)");

    let mut worktrees = Vec::new();
    for i in 1..=n {
        let dir = parent_dir.join(format!("run-races-real-{i}-{}", Uuid::new_v4()));
        if let Err(e) = std::fs::create_dir_all(&dir) {
            eprintln!("REAL_RUN_FAILED(建工作区失败):{e}");
            return false;
        }
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&dir)
                .status()
        };
        let ok = run(&["init", "-q", "-b", "main"])
            .map(|s| s.success())
            .unwrap_or(false)
            && run(&["config", "user.email", "run-races@bw.local"])
                .map(|s| s.success())
                .unwrap_or(false)
            && run(&["config", "user.name", "run_races --real"])
                .map(|s| s.success())
                .unwrap_or(false);
        if !ok {
            eprintln!("REAL_RUN_FAILED(git 初始化失败,第 {i} 件):检查本机是否装了 git");
            return false;
        }
        let _ = std::fs::write(dir.join("README.md"), "run_races --real 临时工作区\n");
        let _ = run(&["add", "."]);
        let _ = run(&["commit", "-q", "-m", "init"]);
        worktrees.push(dir);
    }

    // 真实注册表:同一个数据库文件,换成真实 AgentCliConnector,绑定到
    // 与 mock 档同一个项目(`ctx.project`)——`RunManager::start` 按
    // `issue_row.project_id` 反查连接器,新建的活也都挂在这个项目下。
    let real_entry = ConnectorEntry {
        id: ConnectorId::new(),
        name: REAL_CONNECTOR_NAME.to_string(),
        kind: ConnectorKind::AgentCli {
            cli: CLAUDE.slug.to_string(),
        },
        binding: ProjectBinding {
            project: ctx.project,
            host: String::new(),
            path: "run-races-real".to_string(),
        },
        config: ConfigRef::AgentRegistryRow {
            slug: CLAUDE.slug.to_string(),
        },
    };
    let real_conn = AgentCliConnector::from_entry(
        &real_entry,
        &CLAUDE,
        std::sync::Arc::new(InteractiveCliExecutor::new()),
    );
    let mut real_registry = ConnectorRegistry::default();
    real_registry.register(real_entry, real_conn);
    let real_registry = std::sync::Arc::new(real_registry);

    let manager_real =
        match RunManager::open(&ctx.db_path, real_registry, RunManagerConfig::default()).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("REAL_RUN_FAILED(RunManager::open 失败):{e}");
                for ws in &worktrees {
                    let _ = std::fs::remove_dir_all(ws);
                }
                return false;
            }
        };

    let mut l = Ledger::new();
    let mut ids: Vec<Option<RunId>> = Vec::with_capacity(worktrees.len());
    for (idx, dir) in worktrees.iter().enumerate() {
        let issue = match ctx
            .new_issue(&format!("--real 真实并行示例活 #{}", idx + 1))
            .await
        {
            Ok(id) => id,
            Err(e) => {
                eprintln!("REAL_RUN_FAILED(第 {} 件建活失败):{e}", idx + 1);
                ids.push(None);
                continue;
            }
        };
        let req = StartRun {
            issue,
            connector_name: Some(REAL_CONNECTOR_NAME.to_string()),
            workspace: dir.clone(),
            branch: "main".to_string(),
            inject: vec![bw_connector::InjectBlock {
                label: "demo".to_string(),
                body: "这是 run_races --real 档的演示注入块,不是真实技能库内容。".to_string(),
            }],
            budget_usd: None,
        };
        match manager_real.start(req).await {
            Ok(id) => {
                println!(
                    "第 {} 件 start 成功(经 RunManager),运行编号 {id:?}",
                    idx + 1
                );
                ids.push(Some(id));
            }
            Err(e) => {
                eprintln!(
                    "REAL_RUN_FAILED(第 {} 件 RunManager::start 失败,可能是 §10 第 6 条的双重确认对话框、网关抖动或环境问题):{e}",
                    idx + 1
                );
                ids.push(None);
            }
        }
    }

    let known_ids: Vec<RunId> = ids.iter().filter_map(|o| *o).collect();
    l.check(
        known_ids.len() == worktrees.len(),
        format!(
            "运行行数 = {}(design §5.3 第一项,实得 {})",
            worktrees.len(),
            known_ids.len()
        ),
    );

    // 等每一条离开 Starting——上游会话号在这一步之后才会落库(design
    // §3.4⑤「只起不等」)。
    for id in &known_ids {
        let _ = wait_for_state_left_starting(&ctx.control, *id, Duration::from_secs(15)).await;
    }

    // 90 秒窗口:轮巡数据库看是否已经关门。如实预期:大概率撞 plan/23
    // §10 第 6 条,停在 Running,90 秒后仍未关门——那不算这里的断言失
    // 败(见函数文档),后面统一走 `cancel` 关账。
    let _ = wait_for_all_closed(&ctx.control, &known_ids, Duration::from_secs(90)).await;

    // design §5.3 第二/三项:上游会话号、工作区路径两两不同。
    let mut sessions = std::collections::HashSet::new();
    let mut workspaces = std::collections::HashSet::new();
    let mut rows_read = 0usize;
    for id in &known_ids {
        match ctx.control.get_run(*id).await {
            Ok(Some(row)) => {
                rows_read += 1;
                println!(
                    "  运行 {id:?}:state={:?} upstream_session={:?} workspace={}",
                    row.state, row.upstream_session, row.workspace
                );
                if !row.upstream_session.is_empty() {
                    sessions.insert(row.upstream_session.clone());
                }
                workspaces.insert(row.workspace.clone());
            }
            // 复审「修复复审」新发现 3:此前这里只打印,不经 `Ledger`——
            // 单靠后面的 `rows_read == known_ids.len()` 断言兜底,形态与
            // 评审 Critical-3 治过的「打了 ASSERT FAILED 却没有登记」同
            // 源。`Ledger::fail` 是登记失败的唯一入口(见 `Ledger::check`
            // 文档),这里改走它,不开第二条路。
            _ => l.fail(format!("REAL_RUN_FAILED(读不回运行 {id:?})")),
        }
    }
    l.check(
        rows_read == known_ids.len(),
        format!("{} 条运行行全部可读回(实得 {rows_read})", known_ids.len()),
    );
    l.check(
        sessions.len() == known_ids.len(),
        format!(
            "{} 个互不相同的上游会话号(实得 {},claude 真的各自接受了 BW 指派的会话号)",
            known_ids.len(),
            sessions.len()
        ),
    );
    l.check(
        workspaces.len() == known_ids.len(),
        format!(
            "{} 个互不相同的工作区路径(实得 {})",
            known_ids.len(),
            workspaces.len()
        ),
    );

    // design §5.3 第四项「各结账一次」:逐条经 RunManager 取消(取消口径
    // 同 R6/主控裁决 #6),让结算真的发生——不管上游会话此刻是停在
    // Running(§10 第 6 条)还是已经自己跑完。
    for id in &known_ids {
        if let Err(e) = manager_real.cancel(*id).await {
            // 复审「修复复审」新发现 3:此前只打印,只被随后的
            // `settled_count == known_ids.len()` 间接兜住——若这条运行
            // 恰好已经自己关门结账,`settled_count` 照样对得上,会漏掉
            // 这次 `cancel` 调用本身的失败。改走 `l.fail`,失败只有一
            // 个登记入口。
            l.fail(format!("REAL_RUN_FAILED(取消运行 {id:?} 失败):{e}"));
        }
    }
    let _ = wait_for_all_closed(&ctx.control, &known_ids, Duration::from_secs(15)).await;

    let mut settled_count = 0usize;
    for id in &known_ids {
        if let Ok(Some(row)) = ctx.control.get_run(*id).await {
            if row.settled_at.is_some() {
                settled_count += 1;
            }
        }
    }
    l.check(
        settled_count == known_ids.len(),
        format!(
            "{} 件各结账一次(design §5.3 第四项,实得 {settled_count})",
            known_ids.len()
        ),
    );

    // jsonl 会话文件读回——纯信息性,不计入这一档的判定(如实预期大概
    // 率受阻于 plan/23 §10 第 6 条的双重确认对话框,这是已登记的已知缺
    // 口,不是本次改动要补的东西)。
    let mut jsonl_ok = 0usize;
    for id in &known_ids {
        let Ok(Some(row)) = ctx.control.get_run(*id).await else {
            continue;
        };
        if row.upstream_session.is_empty() {
            continue;
        }
        if let Some(home) = std::env::var_os("HOME") {
            let ws = PathBuf::from(&row.workspace);
            let encoded: String = ws
                .to_string_lossy()
                .chars()
                .map(|c| if c == '/' || c == '.' { '-' } else { c })
                .collect();
            let jsonl = PathBuf::from(home)
                .join(".claude")
                .join("projects")
                .join(encoded)
                .join(format!("{}.jsonl", row.upstream_session));
            let found = std::fs::metadata(&jsonl)
                .map(|m| m.len() > 0)
                .unwrap_or(false);
            println!("  上游会话文件读回:{} · 存在={found}", jsonl.display());
            if found {
                jsonl_ok += 1;
            }
        }
    }
    if jsonl_ok == known_ids.len() && !known_ids.is_empty() {
        println!(
            "REAL_RUN_OK:{jsonl_ok}/{} 件真实并行会话产出了非空上游会话文件",
            known_ids.len()
        );
    } else {
        println!(
            "REAL_RUN 未全部产出上游会话文件({jsonl_ok}/{}):如实登记为受阻(plan/23 §10 第 6 条),不影响本档 RunManager 读回断言的判定",
            known_ids.len()
        );
    }

    for ws in &worktrees {
        let _ = std::fs::remove_dir_all(ws);
    }

    println!(
        "--real 档:断言 {} 条,{}",
        l.count,
        if l.ok { "全过" } else { "有失败" }
    );
    l.ok
}
