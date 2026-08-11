//! `terminal_feed_smoke` — next 切片 5.5 行为验收指挥器(design-s5-
//! hexpanel.md §5.3/主控裁决 7)。
//!
//! 任务简报要求的「mock 会话档」:用 `MockInteractiveExecutor`(或等价)
//! 起一个假会话,把已知字节序列打进 PTY → 断言 `bw-app` 侧订阅收到同样
//! 字节。headless,不驱动真 UI(壳的点击这条路在这套桌面框架上打不通,
//! 是本仓库踩过两轮的结论;深链 + 读回才是常绿验收手段)。
//!
//! **这里用的是「等价」那一档,不是裸 `MockInteractiveExecutor`**——本片
//! 装配时踩出的真发现(见 [`SmokeMock`] 文档):裸 mock 发完字节立刻返
//! 回、`input_rx` 随之被丢弃,测不出「会话还活着时 `terminal_input` 能把
//! 输入送到」这一半。[`SmokeMock`] 同样自我标注【mock】(核心纪律第 2
//! 条),只是把「发完字节」与「返回」这两个时间点分开,给写方向断言留
//! 一个真实存在的窗口。
//!
//! 全链路:`RunManager::start` → `bw_engine::agentcli::AgentCliConnector`
//! (真实实现,换成 [`SmokeMock`])→ 后台任务调 `SmokeMock::run_skill_pty`
//! (把已知字节序列 [`KNOWN_BYTES`] 送进它自己内部的 PTY 字节通道)→
//! `bw_engine::agentcli::pump::spawn_pump`(引擎侧字节泵,100ms 节奏,壳
//! 不参与)→ `RunManager::terminal_feed`(bw-app 侧订阅接口,本片新增)→
//! 这份指挥器收到的字节。**一路都是真代码,唯一的替身在 [`SmokeMock`]
//! 这一层**——连接器/泵/订阅接口全部是生产代码。
//!
//! 跑法:`cd next && cargo run -p bw-app --example terminal_feed_smoke`
//! 退出码 0、末行 `TERMINAL_FEED_SMOKE_OK`、每节都印 `OK` = 通过。

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bw_app::run::{RunManager, RunManagerConfig, StartRun, TermInput};
use bw_connector::{ConfigRef, ConnectorEntry, ConnectorKind, ConnectorRegistry, ProjectBinding};
use bw_core::{ConnectorId, IssueId, ProjectId, RunId};
use bw_engine::agentcli::AgentCliConnector;
use bw_engine::interactive_cli::{InteractiveExecutor, LaunchPlan, PtyInput, SkillOutput, CLAUDE};
use bw_engine::{ExecError, RunCtx};
use bw_store::{IssueStore, NewIssue, NewProject, RunStore, SqliteStore};
use tokio::sync::mpsc;
use uuid::Uuid;

const CONNECTOR_NAME: &str = "mock-agentcli";
/// 已知字节序列——[`SmokeMock::run_skill_pty`] 打进 PTY 的那句自我标注
/// 文本,读方向断言逐字节比对的就是这一份。
const KNOWN_BYTES: &[u8] = "【mock】terminal_feed_smoke 假会话的已知输出\r\n".as_bytes();
/// 会话结束前的观察窗口——给订阅(§2)、读方向(§4)、写方向(§3)三节留
/// 够时间在会话真正关闭前完成。同
/// `bw-engine/examples/agent_session.rs` `DelayedMock` 的既有先例
/// (300ms)同一数量级,这里给到 1 秒余量。
const HOLD_OPEN: Duration = Duration::from_secs(1);
/// 泵节奏是 100ms(`bw_engine::agentcli::pump::PUMP_INTERVAL`),订阅+接
/// 收整条链路给 3 秒上限——比 `HOLD_OPEN` 富余,不是掐着点算。
const RECV_TIMEOUT: Duration = Duration::from_secs(3);
const SUBSCRIBE_POLL_TIMEOUT: Duration = Duration::from_secs(2);

#[tokio::main]
async fn main() -> ExitCode {
    println!("== terminal_feed_smoke · 切片 5.5 行为验收指挥器 ==");

    let db_path =
        std::env::temp_dir().join(format!("bw-terminal-feed-smoke-{}.db", Uuid::new_v4()));
    println!("本次数据库:{}   ← 跑完不删,给人手工复核", db_path.display());

    let workspace = make_workspace();
    println!("本次工作区(真 git 检出):{}", workspace.display());

    let control = match SqliteStore::open(&db_path.to_string_lossy()).await {
        Ok(s) => s,
        Err(e) => return fail(&format!("开库失败:{e}")),
    };

    let project = ProjectId::new();
    if let Err(e) = control
        .create_project(NewProject {
            id: project,
            name: "terminal_feed_smoke 示例项目".to_string(),
            root_path: String::new(),
        })
        .await
    {
        return fail(&format!("create_project 失败:{e}"));
    }

    let issue = IssueId::new();
    if let Err(e) = control
        .create_issue(NewIssue {
            id: issue,
            project_id: project,
            number: 1,
            title: "terminal_feed_smoke 示例活".to_string(),
        })
        .await
    {
        return fail(&format!("create_issue 失败:{e}"));
    }

    // 装配:真实 AgentCliConnector,执行器换成这份指挥器自己的
    // `SmokeMock`——connector/pump/订阅接口全部是生产代码,唯一的替身在
    // 最底下这一层(核心纪律第 2 条:mock 自我标注)。
    let binding = ProjectBinding {
        project,
        host: String::new(),
        path: "terminal-feed-smoke".to_string(),
    };
    let entry = ConnectorEntry {
        id: ConnectorId::new(),
        name: CONNECTOR_NAME.to_string(),
        kind: ConnectorKind::AgentCli {
            cli: CLAUDE.slug.to_string(),
        },
        binding: binding.clone(),
        config: ConfigRef::AgentRegistryRow {
            slug: CLAUDE.slug.to_string(),
        },
    };
    let exec: Arc<dyn InteractiveExecutor> = Arc::new(SmokeMock { hold: HOLD_OPEN });
    // 具体类型 `Arc<AgentCliConnector>` 单留一份(不经 `from_entry` 抹成
    // `Arc<dyn Connector>`)——同 `run_races.rs` `Ctx::mock` 的既有先例
    // (`Connector` trait 故意零 `dyn Any`,没有下转路径,只能在装配时多
    // 留一份具体类型的把手)。§4 新增的尺寸读回断言要调
    // `terminal_size_by_workspace`,那是 `AgentCliConnector` 自己的方法,
    // 不在契约里。
    let concrete = Arc::new(AgentCliConnector::new(binding.clone(), &CLAUDE, exec));
    let mut registry = ConnectorRegistry::default();
    registry.register(entry, concrete.clone());
    let registry = Arc::new(registry);

    let manager = match RunManager::open(&db_path, registry, RunManagerConfig::default()).await {
        Ok(m) => m,
        Err(e) => return fail(&format!("RunManager::open 失败:{e}")),
    };

    println!();
    println!("== 1 · 开工,起一个假会话 ==");
    let run_id = match manager
        .start(StartRun {
            issue,
            connector_name: Some(CONNECTOR_NAME.to_string()),
            workspace: workspace.clone(),
            branch: "main".to_string(),
            inject: vec![],
            budget_usd: None,
        })
        .await
    {
        Ok(id) => {
            println!("  ✓ RunManager::start 立刻返回运行编号 {id:?}");
            id
        }
        Err(e) => return fail(&format!("RunManager::start 失败:{e}")),
    };

    println!();
    println!("== 2 · bw-app 侧订阅接口:RunManager::terminal_feed ==");
    let mut pty_rx = match poll_until_subscribed(&manager, run_id, SUBSCRIBE_POLL_TIMEOUT).await {
        Some(rx) => {
            println!("  ✓ terminal_feed 拿到票据、connector 支持 Interactive、订阅成功");
            rx
        }
        None => return fail("terminal_feed 在超时窗口内始终没有返回 Some(receiver)"),
    };

    println!();
    println!("== 3 · 写入方向:RunManager::terminal_input(会话仍在 HOLD_OPEN 窗口内)==");
    match manager
        .terminal_input(run_id, TermInput::Bytes(b"echo probe\n".to_vec()))
        .await
    {
        Ok(true) => println!("  ✓ terminal_input(Bytes) 返回 true(会话还活着,写入被接受)"),
        Ok(false) => return fail("terminal_input(Bytes) 在会话存活期间返回了 false"),
        Err(e) => return fail(&format!("terminal_input(Bytes) 报错:{e}")),
    }
    match manager
        .terminal_input(
            run_id,
            TermInput::Resize {
                cols: 100,
                rows: 40,
            },
        )
        .await
    {
        Ok(true) => println!("  ✓ terminal_input(Resize) 返回 true"),
        Ok(false) => return fail("terminal_input(Resize) 在会话存活期间返回了 false"),
        Err(e) => return fail(&format!("terminal_input(Resize) 报错:{e}")),
    }

    // next 切片 5.5 修(评审 task-s55-review.md Important-1):`terminal_
    // input(Resize)` 返回 `true` 只证明字节送达了 PTY,证不了尺寸账目真
    // 的落进了 `TerminalManager`——这条断言直接从管理器读回
    // `current_size`,核对它等于刚才设的 100×40,而不是 `attach` 时的
    // 80×24 默认值(`send_input` 若退回到走 `input()` 而不是
    // `resize()`,`current_size` 会原地不动,这里就会读到 80×24 或者压根
    // 对不上)。
    match concrete.terminal_size_by_workspace(&workspace) {
        Some((100, 40)) => println!("  ✓ 从 TerminalManager 读回的尺寸是 100×40,与所设一致"),
        Some((80, 24)) => return fail(
            "从 TerminalManager 读回的尺寸仍是 80×24 默认值——resize 没有真的走 TerminalManager::resize",
        ),
        Some((cols, rows)) => {
            return fail(&format!("从 TerminalManager 读回的尺寸是 {cols}×{rows},与所设的 100×40 不一致"))
        }
        None => return fail("terminal_size_by_workspace 读回 None——会话应该还在管理器里活着"),
    }

    println!();
    println!("== 4 · 读方向:已知字节序列打进 PTY → 订阅方收到同样字节 ==");
    println!(
        "  已知字节序列(SmokeMock::run_skill_pty 打进 PTY 的那一句):{:?}",
        String::from_utf8_lossy(KNOWN_BYTES)
    );
    let received = match tokio::time::timeout(RECV_TIMEOUT, pty_rx.recv()).await {
        Ok(Ok(bytes)) => bytes,
        Ok(Err(e)) => return fail(&format!("broadcast::Receiver::recv 报错:{e}")),
        Err(_) => return fail("在 3 秒超时窗口内没有收到任何字节——泵/订阅链路没有真的通"),
    };
    if received == KNOWN_BYTES {
        println!(
            "  ✓ 收到的字节与已知序列逐字节一致({} 字节)",
            received.len()
        );
    } else {
        return fail(&format!(
            "字节不一致:收到 {:?},期望 {:?}",
            String::from_utf8_lossy(&received),
            String::from_utf8_lossy(KNOWN_BYTES)
        ));
    }

    println!();
    println!("== 5 · 取消 → terminal_feed/terminal_input 如实回落到「没有」==");
    if let Err(e) = manager.cancel(run_id).await {
        return fail(&format!("cancel 失败:{e}"));
    }
    match manager.terminal_feed(run_id).await {
        Ok(None) => println!("  ✓ 取消之后 terminal_feed 如实返回 None(不在活跃表了)"),
        Ok(Some(_)) => return fail("取消之后 terminal_feed 仍然返回了 Some——活跃表没有真的摘掉"),
        Err(e) => return fail(&format!("terminal_feed(取消后)报错:{e}")),
    }
    match manager
        .terminal_input(run_id, TermInput::Bytes(b"should not land".to_vec()))
        .await
    {
        Ok(false) => println!("  ✓ 取消之后 terminal_input 如实返回 false"),
        Ok(true) => return fail("取消之后 terminal_input 仍然返回了 true"),
        Err(e) => return fail(&format!("terminal_input(取消后)报错:{e}")),
    }

    // 读回:运行行确实落了账(核心纪律第 1 条「数字一律 SQL 读回」的同一
    // 条老规矩——这里读的是布尔状态,不是数字,但手法同一条:走真实存
    // 储查询,不是自己在内存里断言一遍就当数过了)。
    println!();
    println!("== 附 · 读回 run 表这一行的真实状态 ==");
    match control.get_run(run_id).await {
        Ok(Some(row)) => {
            println!(
                "  → state={:?} end_kind={:?} ended_at={:?} settled_at={:?}",
                row.state, row.end_kind, row.ended_at, row.settled_at
            );
            if row.ended_at.is_some() {
                println!("  ✓ 关门事实与 cancel() 的行为一致");
            } else {
                return fail("run 行 ended_at 仍为空,cancel 没有真的关门");
            }
        }
        Ok(None) => return fail("get_run 读回空——运行行不该消失"),
        Err(e) => return fail(&format!("get_run 报错:{e}")),
    }

    println!();
    println!("TERMINAL_FEED_SMOKE_OK");
    ExitCode::SUCCESS
}

fn fail(msg: &str) -> ExitCode {
    eprintln!("ASSERT FAILED: {msg}");
    eprintln!("TERMINAL_FEED_SMOKE_FAILED");
    ExitCode::FAILURE
}

/// 轮询直到 `terminal_feed` 返回 `Some`(票据/连接器能力/会话都齐了)或
/// 超时。开工外呼是异步的(「只起不等」),`start()` 返回运行编号之后,
/// 真正的连接器 `Execute::start` 调用还要走一趟 `tokio::spawn` 才落
/// 定——这条轮询等的就是那一小段异步装配窗口,不是等 [`HOLD_OPEN`] 那
/// 条业务延迟([`SmokeMock`] 的等待在 `run_skill_pty` 内部,
/// `AgentCliConnector::start` 本身早就已经同步返回票据了)。
async fn poll_until_subscribed(
    manager: &RunManager,
    run: RunId,
    timeout: Duration,
) -> Option<tokio::sync::broadcast::Receiver<Vec<u8>>> {
    let start = std::time::Instant::now();
    loop {
        if let Ok(Some(rx)) = manager.terminal_feed(run).await {
            return Some(rx);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// 这份指挥器自己的假会话执行器——发一次已知字节序列,然后在 `hold` 这
/// 段时间里**持续等着接输入**再返回。**不能直接套用裸
/// `MockInteractiveExecutor`**(踩出来的真发现,写进 commit 正文/报告
/// concerns):它的 `run_skill_pty` send 完字节就用一个同步 `while
/// input_rx.try_recv().is_ok() {{}}` 清一遍队列、立刻返回——`input_rx`
/// 随之被丢弃,`TerminalManager::input`/`pty_input_tx.send` 从那一刻起必
/// 然失败(接收端已经不在了),读方向的字节序列断言不受影响,但写方向
/// (`terminal_input`)测不出「会话还活着时输入能送到」这件事,会被误判
/// 成假的接口缺陷。这里新写一个同样自我标注【mock】的执行器,用
/// `tokio::select!` 让 `input_rx` 在 `hold` 窗口内持续存活。
struct SmokeMock {
    hold: Duration,
}

#[async_trait]
impl InteractiveExecutor for SmokeMock {
    async fn run_skill(&self, _plan: &LaunchPlan, _ctx: &RunCtx) -> Result<SkillOutput, ExecError> {
        Ok(SkillOutput {
            completed: true,
            summary: "【mock】terminal_feed_smoke 专用假会话完成(流程演示)".to_string(),
            exit_code: None,
        })
    }

    async fn run_skill_resume(
        &self,
        _plan: &LaunchPlan,
        _ctx: &RunCtx,
    ) -> Result<SkillOutput, ExecError> {
        self.run_skill(_plan, _ctx).await
    }

    async fn run_skill_pty(
        &self,
        _plan: &LaunchPlan,
        _ctx: &RunCtx,
        bytes_tx: mpsc::UnboundedSender<Vec<u8>>,
        mut input_rx: mpsc::UnboundedReceiver<PtyInput>,
    ) -> Result<SkillOutput, ExecError> {
        let _ = bytes_tx.send(KNOWN_BYTES.to_vec());
        let deadline = tokio::time::Instant::now() + self.hold;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            tokio::select! {
                _ = tokio::time::sleep(remaining) => break,
                // 收到就继续等——`input_rx` 保持存活直到 `hold` 用完,不
                // 是收到第一条就提前返回。
                got = input_rx.recv() => {
                    if got.is_none() {
                        break; // 发送端全部掉了(不该发生,防御式退出)。
                    }
                }
            }
        }
        Ok(SkillOutput {
            completed: true,
            summary: "【mock】terminal_feed_smoke 专用假会话完成(流程演示)".to_string(),
            exit_code: None,
        })
    }
}

/// 起一个真实(临时)git 工作区——`AgentCliConnector::start` ①校验的就是
/// 这个(`.git` 目录必须在、当前分支必须等于 `spec.branch`)。同
/// `bw-engine/examples/agent_session.rs`/`bw-app/examples/run_races.rs`
/// `--real` 档的既有先例。
fn make_workspace() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bw-terminal-feed-smoke-ws-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("临时工作区目录应可创建");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(&dir)
            .status()
            .expect("git 应在 PATH 里")
    };
    run(&["init", "-q", "-b", "main"]);
    run(&["config", "user.email", "terminal-feed-smoke@bw.local"]);
    run(&["config", "user.name", "terminal_feed_smoke 指挥器"]);
    std::fs::write(dir.join("README.md"), "terminal_feed_smoke 临时工作区\n")
        .expect("README 应可写入");
    run(&["add", "."]);
    run(&["commit", "-q", "-m", "init"]);
    dir
}
