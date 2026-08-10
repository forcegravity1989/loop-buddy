//! `RunManager` 本体(design-s4-runmanager.md §3):单条命令队列(有界
//! 1024)+ 一个循环任务独占内存活跃表——竞态串行化天然成立,循环里唯一
//! 允许 `.await` 的是存储调用,连接器调用(开工/轮询/取消)一律
//! `tokio::spawn` 出去,结果经命令队列送回(§3.2「写进代码注释的并发纪
//! 律」三条)。
//!
//! **「同一件活一个交付运行」守在三层,底下那层是真的**(§3.4):内存
//! `by_issue` 只是快路径;真正的守卫是 `bw-store` 的部分唯一索引
//! (`create_run` 撞违反时返回 `is_unique_violation`);顺序是**先插行占
//! 名额,再外呼开工**(`handle_start` 的 ④ 早于 ⑥)。
//!
//! **结束回写只能被观测,不能被宣布**:[`Cmd::Observed`] 是私有的内部命
//! 令,唯一构造点是 [`spawn_poll_task`] 派出去的轮询任务——外部调用方拿
//! 不到能构造它的路径,没有一个公开方法能「宣布」一次运行结束了。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use bw_connector::{
    CallCtx, ConnError, Connector, ConnectorRegistry, ExecSpec, ExecState, ExecTicket, RequestId,
    SessionEnd,
};
use bw_core::{IssueId, IssueStatus, ProjectId, RunId, RunState};
use bw_store::{IssueStore, NewRun, RunEndKind, RunKind, RunStore, SqliteStore};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use super::types::{ReapReport, RunError, RunManagerConfig, RunSnapshot, StartRun};

fn now_unix() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

/// 取消口径(主控裁决 #6):BW 侧取消 = 让连接器杀掉进程组/收尾会话 + 关
/// 账;上游会话档案(claude 一家是 `~/.claude/projects/…/*.jsonl`)仍在,
/// 人可以自己续接。**不叫「暂停」**——BW 不承诺、也不实现能续接回同一次
/// 运行,`end_detail` 的原文如实说清这一点,不让读的人误以为这是一次可
/// 逆的暂停。
const CANCEL_END_DETAIL: &str = "已取消(上游会话档案保留,人可自行续接)";

/// 运行管理器的把手。可克隆(克隆的是发信端,不是状态)——克隆的每一份都
/// 指向同一条命令队列、同一个循环任务。
#[derive(Clone)]
pub struct RunManager {
    tx: mpsc::Sender<Cmd>,
    /// 共享的收尾守卫:最后一份 `RunManager` 把手被丢弃时(`Arc` 引用计
    /// 数归零),`ManagerGuard::drop` 触发管理器级取消令牌,循环任务与所
    /// 有还在跑的轮询任务随之退出——不需要调用方显式调一个 `close()`。
    _guard: Arc<ManagerGuard>,
}

struct ManagerGuard {
    cancel: CancellationToken,
}

impl Drop for ManagerGuard {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl RunManager {
    /// 开库 + 起循环任务。**不自动清理遗留**——清不清、什么时候清,由调
    /// 用方显式决定([`RunManager::reap_on_restart`] 的存在本身就是这个
    /// 决定权的落点;开机批量唤醒是 v1 明令禁止的,design §3.1)。
    pub async fn open(
        db: &Path,
        registry: Arc<ConnectorRegistry>,
        cfg: RunManagerConfig,
    ) -> Result<Self, RunError> {
        let store = Arc::new(SqliteStore::open(&db.to_string_lossy()).await?);
        let (tx, rx) = mpsc::channel(1024);
        let cancel = CancellationToken::new();
        let state = Loop {
            store,
            registry,
            cfg,
            tx: tx.clone(),
            active: HashMap::new(),
            by_issue: HashMap::new(),
            by_workspace: HashMap::new(),
        };
        let loop_cancel = cancel.clone();
        tokio::spawn(run_loop(rx, loop_cancel, state));
        Ok(Self {
            tx,
            _guard: Arc::new(ManagerGuard { cancel }),
        })
    }

    /// 开工。返回运行编号;活会被推到「进行中」。返回时运行行已经插入
    /// (`state = Starting`)、名额已经占住——**不等**连接器的 `start` 外呼
    /// 真正返回(那一步在后台跑,结果经 [`Cmd::Started`] 回到循环)。
    pub async fn start(&self, req: StartRun) -> Result<RunId, RunError> {
        self.call(|reply| Cmd::Start { req, reply }).await
    }

    /// 取消。幂等:已经关门的运行再取消 = `Ok`,不改任何字段。
    pub async fn cancel(&self, run: RunId) -> Result<(), RunError> {
        self.call(|reply| Cmd::Cancel { run, reply }).await
    }

    /// 降级为咨询:放开这件活的交付名额,会话不杀、工作区不清(design
    /// §3.5)。返回 `true` = 这次真降级了;`false` = 没什么可降的(幂
    /// 等)。
    pub async fn demote_to_consultation(&self, run: RunId) -> Result<bool, RunError> {
        self.call(|reply| Cmd::Demote { run, reply }).await
    }

    /// 重启后收拾遗留:库里还开着的运行,BW 这边已经没有句柄了,如实标
    /// 成「不知道怎么结束的」并放开名额。返回收拾了几条。
    pub async fn reap_on_restart(&self) -> Result<ReapReport, RunError> {
        self.call(|reply| Cmd::Reap { reply }).await
    }

    /// 读:聚合快照(界面与指挥器用)。返回的是计数,不是一百行明细。
    pub async fn snapshot(&self, project: Option<ProjectId>) -> Result<RunSnapshot, RunError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(Cmd::Snapshot { project, reply })
            .await
            .map_err(|_| RunError::Closed)?;
        rx.await.map_err(|_| RunError::Closed)
    }

    /// 四个「发命令、等回信」公开方法共用的样板:发送失败(队列已关)与
    /// 回信丢失(循环任务提前退出)统一映射成 [`RunError::Closed`]。
    async fn call<T>(
        &self,
        make: impl FnOnce(oneshot::Sender<Result<T, RunError>>) -> Cmd,
    ) -> Result<T, RunError> {
        let (reply, rx) = oneshot::channel();
        self.tx
            .send(make(reply))
            .await
            .map_err(|_| RunError::Closed)?;
        rx.await.map_err(|_| RunError::Closed)?
    }
}

/// 循环处理的命令。`Start`/`Cancel`/`Demote`/`Reap`/`Snapshot` 五个是外部
/// 公开方法的落地;`Started`/`Observed` 两个是内部命令,构造点分别是
/// [`Loop::handle_start`] 派出去的开工外呼任务与 [`spawn_poll_task`] 派出
/// 去的轮询任务——外部代码没有路径构造它们。
enum Cmd {
    Start {
        req: StartRun,
        reply: oneshot::Sender<Result<RunId, RunError>>,
    },
    Cancel {
        run: RunId,
        reply: oneshot::Sender<Result<(), RunError>>,
    },
    Demote {
        run: RunId,
        reply: oneshot::Sender<Result<bool, RunError>>,
    },
    Reap {
        reply: oneshot::Sender<Result<ReapReport, RunError>>,
    },
    Snapshot {
        project: Option<ProjectId>,
        reply: oneshot::Sender<RunSnapshot>,
    },
    /// 内部命令:某运行的开工外呼(`Execute::start`)完成了。
    Started {
        run: RunId,
        outcome: Result<ExecTicket, ConnError>,
    },
    /// 内部命令:某运行的轮询任务观测到一个终态。
    Observed { run: RunId, state: ExecState },
}

/// 循环内存持有的一条「还活着」的运行——连接器句柄 + 取消令牌。**运行表
/// 由循环按值持有**(design §3.2 第二条并发纪律),不放进共享互斥体。
struct ActiveRun {
    issue: IssueId,
    project: ProjectId,
    workspace: PathBuf,
    /// 内存里的 kind 副本,随 [`Loop::handle_demote`] 翻面——只用于「降
    /// 级后是否还占 `by_issue` 名额」这类内存记账判断,不是权威(权威在
    /// 数据库,`bw_store::RunRow.kind`)。
    kind: RunKind,
    connector: Arc<dyn Connector>,
    /// `None` = 还在 `Starting`(连接器的 `start` 调用还没返回,没有票
    /// 据可取消)。
    ticket: Option<ExecTicket>,
    /// 这条运行专属的取消令牌:轮询任务拿它 `select!`;也复用作开工外呼
    /// 的 `CallCtx.cancel`——取消能打断还没返回的 `start` 调用。
    poll_cancel: CancellationToken,
}

/// 循环任务独占的全部可变状态。**不用锁**——这是「循环独占」这条设计的
/// 全部含义:只有 `run_loop` 这一个 `.await` 点会碰它。
struct Loop {
    store: Arc<SqliteStore>,
    registry: Arc<ConnectorRegistry>,
    cfg: RunManagerConfig,
    tx: mpsc::Sender<Cmd>,
    active: HashMap<RunId, ActiveRun>,
    /// 「同一件活一个交付运行」的内存快路径(design §3.4 第一行)——真正
    /// 的守卫在存储层的部分唯一索引,这张表只是省一次数据库往返。
    by_issue: HashMap<IssueId, RunId>,
    /// 「同工作区串线校验」的内存实现(主控裁决 #5:单点实现,不跨层,
    /// 只在运行管理器这一处查)。**只在本进程存活期内成立**——重启后从
    /// 空表开始,`reap_on_restart()` 之前(它不是自动触发的)对一个「库
    /// 里还开着旧运行、但本进程从未见过」的工作区开工,这里查不到冲
    /// 突,不会被挡下(plan/23-opc-stitching-rebuild.md §10 第 11 条,如
    /// 实登记,本片不修)。
    by_workspace: HashMap<PathBuf, RunId>,
}

async fn run_loop(mut rx: mpsc::Receiver<Cmd>, cancel: CancellationToken, mut state: Loop) {
    loop {
        let cmd = tokio::select! {
            _ = cancel.cancelled() => break,
            cmd = rx.recv() => match cmd {
                Some(c) => c,
                None => break, // 命令队列已关(所有发信端都掉了)。
            },
        };
        state.handle(cmd).await;
    }
}

impl Loop {
    async fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Start { req, reply } => {
                let result = self.handle_start(req).await;
                let _ = reply.send(result);
            }
            Cmd::Cancel { run, reply } => {
                let result = self.handle_cancel(run).await;
                let _ = reply.send(result);
            }
            Cmd::Demote { run, reply } => {
                let result = self.handle_demote(run).await;
                let _ = reply.send(result);
            }
            Cmd::Reap { reply } => {
                let result = self.handle_reap().await;
                let _ = reply.send(result);
            }
            Cmd::Snapshot { project, reply } => {
                let snap = self.handle_snapshot(project);
                let _ = reply.send(snap);
            }
            Cmd::Started { run, outcome } => self.handle_started(run, outcome).await,
            Cmd::Observed { run, state } => self.handle_observed(run, state).await,
        }
    }

    /// design §3.4 的五步:①②③④⑤(⑤本身又分「成功/失败回信」两支,那
    /// 两支落在 [`Loop::handle_started`],不在这里——这个函数只做到「插
    /// 行占名额 + 推进行中 + 派出外呼」,随即把运行编号回给调用方(「只
    /// 起不等」这条纪律在运行管理器自己身上的体现,不只是连接器层的)。
    async fn handle_start(&mut self, req: StartRun) -> Result<RunId, RunError> {
        // 内存快路径:重复请求立刻如实拒绝,不碰数据库。
        if let Some(existing) = self.by_issue.get(&req.issue) {
            return Err(RunError::AlreadyRunning {
                existing: *existing,
            });
        }
        // 主控裁决 #5:同工作区串线校验。
        if let Some(existing) = self.by_workspace.get(&req.workspace) {
            return Err(RunError::WorkspaceBusy {
                workspace: req.workspace.clone(),
                existing: *existing,
            });
        }

        // ① 查活、判状态——只有「待办池/待办/进行中」能开工(design
        // §3.4①,「进行中」是诚实失败后的重试路径)。
        let issue_row = self
            .store
            .get_issue(req.issue)
            .await?
            .ok_or(RunError::IssueNotFound(req.issue))?;
        if !matches!(
            issue_row.status,
            IssueStatus::Backlog | IssueStatus::Todo | IssueStatus::InProgress
        ) {
            return Err(RunError::NotStartable {
                status: issue_row.status,
            });
        }
        let project = issue_row.project_id;

        // ② 解析执行连接器。
        let (connector_name, connector) =
            resolve_executor(&self.registry, project, req.connector_name.as_deref())?;

        // ③ 先插行占名额(design §3.4 第三行——最容易写错的一处:必须先
        // 插行再外呼,消灭「查完到插入之间」的空窗)。
        let run_id = RunId::new();
        let call_req = RequestId::new();
        let started_at = now_unix();
        let insert = self
            .store
            .create_run(NewRun {
                id: run_id,
                project_id: project,
                issue_id: req.issue,
                kind: RunKind::Delivery,
                connector_name,
                req_id: call_req.to_string(),
                workspace: req.workspace.to_string_lossy().to_string(),
                branch: req.branch.clone(),
                state: RunState::Starting,
                started_at,
            })
            .await;
        if let Err(e) = insert {
            if e.is_unique_violation() {
                // 撞了唯一索引——查回真正是哪一条(跨重启也成立,不靠内
                // 存缓存;§3.4 第二行「存储才是真正的守卫」)。
                let existing = self.store.find_live_delivery_run(req.issue).await?;
                return match existing {
                    Some(existing) => {
                        self.by_issue.insert(req.issue, existing); // 补上快路径缓存
                        Err(RunError::AlreadyRunning { existing })
                    }
                    // 理论不该发生(撞了索引又查不到同一件活的活跃交付
                    // 运行):如实透传原始存储错误,不猜一个说法。
                    None => Err(RunError::Store(e)),
                };
            }
            return Err(RunError::Store(e));
        }

        // 名额真正占住——内存记账。
        let poll_cancel = CancellationToken::new();
        self.by_issue.insert(req.issue, run_id);
        self.by_workspace.insert(req.workspace.clone(), run_id);
        self.active.insert(
            run_id,
            ActiveRun {
                issue: req.issue,
                project,
                workspace: req.workspace.clone(),
                kind: RunKind::Delivery,
                connector: connector.clone(),
                ticket: None,
                poll_cancel: poll_cancel.clone(),
            },
        );

        // ④ 把活推到「进行中」——运行管理器唯一改活状态的写(design
        // §3.6)。没有 `IssueStatus` 形参可传:`mark_issue_in_progress`
        // 天生写不出别的值。**不能用 `?` 直接甩手**(评审 Important-1):
        // 这一步失败时,③ 已经插了运行行、也已经在内存三张表里占了名
        // 额——直接传播错误,调用方拿到的是 `Err` 却没有 `RunId`,连
        // `cancel` 都调不了,这件活的名额会被一条外部看不见的
        // `starting` 行占死到下次重启 + `reap_on_restart`。这里尽力把已
        // 插的行诚实关成「失败」并回滚三张内存表,让这件活在这个进程里
        // 立刻能重新开工——补写要是也失败,如实记一行诊断,那一行只能
        // 等 `reap_on_restart` 收拾,但至少内存槽位不再被这个进程自己
        // 锁死。
        if let Err(e) = self
            .store
            .mark_issue_in_progress(req.issue, started_at)
            .await
        {
            let at = now_unix();
            let detail = format!("[run-manager] 把活推到「进行中」时存储出错,已尽力回滚:{e}");
            match self
                .store
                .close_run(
                    run_id,
                    at,
                    RunState::Failed,
                    Some(RunEndKind::StartFailed),
                    &detail,
                )
                .await
            {
                Ok(true) => {
                    let _ = self.store.settle_run(run_id, at).await;
                }
                Ok(false) => eprintln!(
                    "[run-manager] 诊断:运行 {run_id:?} 回滚关门时发现已经被别的动作关过门(诚实空转)"
                ),
                Err(close_err) => eprintln!(
                    "[run-manager] 诊断:运行 {run_id:?} 回滚关门也失败,库里这一行会一直挂着 \
                     state=starting,直到下次重启 + reap_on_restart 收拾(如实记录,不假装已收\
                     尾):{close_err}"
                ),
            }
            self.active.remove(&run_id);
            self.release_issue_slot(req.issue, run_id);
            self.release_workspace_slot(&req.workspace, run_id);
            return Err(RunError::Store(e));
        }

        // ⑤ 派出去调连接器的 start——不等。
        let tx = self.tx.clone();
        let spec = ExecSpec {
            workspace: req.workspace,
            branch: req.branch,
            inject: req.inject,
            budget_usd: req.budget_usd,
        };
        tokio::spawn(async move {
            let cx = CallCtx {
                req: call_req,
                timeout: None,
                cancel: poll_cancel,
            };
            let outcome = match connector.as_execute() {
                Some(execute) => execute
                    .start(&cx, spec)
                    .await
                    .map(|ok| ok.value)
                    .map_err(|fail| fail.err),
                None => Err(ConnError::Other(
                    "registry 返回的连接器不支持 Execute(装配期编码错误)".into(),
                )),
            };
            let _ = tx
                .send(Cmd::Started {
                    run: run_id,
                    outcome,
                })
                .await;
        });

        Ok(run_id)
    }

    /// design §3.4⑤ 的两支:成功 → 推 `running`、记会话号、起轮询任务;
    /// 失败 → 关门(`Failed`/`StartFailed`)+ 结账一次,名额释放。
    async fn handle_started(&mut self, run: RunId, outcome: Result<ExecTicket, ConnError>) {
        let Some(active_run) = self.active.get_mut(&run) else {
            // 已经不在活跃表——多半是这条运行在外呼返回之前就被取消/收
            // 拾掉了。如实空转,不碰已经写定的行(见 `handle_cancel` 里
            // `poll_cancel` 复用作 `CallCtx.cancel` 的说明:多数情况下这
            // 个外呼本身也会因为同一个令牌被取消而报 `Err`,这里是防御
            // 式兜底,不是常态路径)。
            eprintln!(
                "[run-manager] 诊断:运行 {run:?} 的开工外呼结果晚到,该运行已不在活跃表,诚实空转"
            );
            return;
        };

        match outcome {
            Ok(ticket) => {
                active_run.ticket = Some(ticket.clone());
                let connector = active_run.connector.clone();
                let poll_cancel = active_run.poll_cancel.clone();
                let upstream = ticket.upstream_session.clone().unwrap_or_default();
                match self
                    .store
                    .mark_run_started(run, &upstream, now_unix())
                    .await
                {
                    Ok(true) => {
                        spawn_poll_task(
                            self.tx.clone(),
                            connector,
                            run,
                            ticket,
                            poll_cancel,
                            self.cfg.poll_interval,
                        );
                    }
                    // 评审 Important-1:此前这两支只打诊断,不起轮询任务
                    // 也不关门——运行留在 `self.active`(占着 issue/
                    // workspace 名额),但没有任何后续机制会再碰它,是一
                    // 次永久搁浅(直到下次重启 + `reap_on_restart`)。这里
                    // 改成诚实收尾:关门 + 结账 + 释放内存槽位,不再是
                    // 「打完日志就当没发生过」。
                    Ok(false) => {
                        self.honest_close_on_storage_error(
                            run,
                            "起工回写落空(比较并置未命中——理论不该发生)",
                        )
                        .await;
                    }
                    Err(e) => {
                        self.honest_close_on_storage_error(run, format!("起工回写失败:{e}"))
                            .await;
                    }
                }
            }
            Err(conn_err) => {
                let at = now_unix();
                let detail = conn_err.to_string();
                let issue = active_run.issue;
                let workspace = active_run.workspace.clone();
                match self
                    .store
                    .close_run(
                        run,
                        at,
                        RunState::Failed,
                        Some(RunEndKind::StartFailed),
                        &detail,
                    )
                    .await
                {
                    Ok(true) => {
                        let _ = self.store.settle_run(run, at).await;
                    }
                    Ok(false) => eprintln!(
                        "[run-manager] 诊断:运行 {run:?} 起工失败的关门回写晚了(比较并置未命中)"
                    ),
                    Err(e) => {
                        eprintln!("[run-manager] 诊断:运行 {run:?} 起工失败的关门回写出错:{e}")
                    }
                }
                self.active.remove(&run);
                self.release_issue_slot(issue, run);
                self.release_workspace_slot(&workspace, run);
            }
        }
    }

    /// 轮询任务观测到一个终态——**唯一合法调用路径**(design §3.1)。关
    /// 门用比较并置:赢了才结算 + 释放名额;输了(晚到消息)诚实空转,
    /// 一个字段都不碰(这是 R3/R6 两个竞态共用的同一个机制)。
    async fn handle_observed(&mut self, run: RunId, exec_state: ExecState) {
        let Some(observed) = map_exec_state(exec_state) else {
            return; // 仍在跑/未知——轮询任务只在终态时才会发这个命令,防御式忽略。
        };
        let at = now_unix();
        match self
            .store
            .close_run(run, at, observed.state, observed.end_kind, &observed.detail)
            .await
        {
            Ok(true) => {
                // 结算:false 也无害——多半是这条运行之前已经被降级为咨
                // 询、账已经结过了(design §3.5②「账不重结」)。
                let _ = self.store.settle_run(run, at).await;
                if let Some(active_run) = self.active.remove(&run) {
                    self.release_issue_slot(active_run.issue, run);
                    self.release_workspace_slot(&active_run.workspace, run);
                }
            }
            Ok(false) => eprintln!(
                "[run-manager] 诊断:运行 {run:?} 的结束消息晚到,已经被别的动作关过门,诚实空转"
            ),
            // 评审 Important-1:此前这一支只打诊断——`close_run` 本身出
            // 错意味着这次「关门」没有写进去任何字段,`self.active` 里
            // 这条运行仍然「活着」、仍然占着 issue/workspace 名额,却再
            // 也没有别的机制会去碰它(轮询任务已经在派出这条命令前退出
            // 了),是一次永久搁浅。改成诚实收尾:尽力补一次关门写(标
            // 成失败 + 存储错误原文),不管补写成不成功,内存槽位都要
            // 释放。
            Err(e) => {
                self.honest_close_on_storage_error(run, format!("结束回写失败:{e}"))
                    .await;
            }
        }
    }

    /// 存储调用出错时的诚实收尾(评审 Important-1):三处调用点(见各自
    /// 文档)在写库失败之后,不能只打一行诊断就当没发生过——那会让这条
    /// 运行永久留在 `self.active`(占着 issue/workspace 名额),却再没有
    /// 任何机制会去推进它,直到下次重启 + `reap_on_restart` 才会被发
    /// 现。这里尽力再补一次关门写(`state=Failed`,`end_detail` 写存储错
    /// 误原文/诊断上下文);不管补写成不成功,内存槽位都要释放——补写
    /// 成功 → 数据库如实记下「失败」;补写也失败 → 数据库那一行还是
    /// `ended_at IS NULL`(要等 `reap_on_restart` 收拾),但至少这个进程
    /// 不会再把这件活/这个工作区锁死。
    ///
    /// **E2E 未覆盖,见 plan/23-opc-stitching-rebuild.md §10 第 12
    /// 条**:这三条分支要真的被走到,前提是存储调用本身出错——
    /// `run_races` 指挥器背后是真实 `SqliteStore`,正常路径下不会自己报
    /// 错,没有故障注入机制就造不出这个前提。`run_races` 里目前没有一
    /// 条断言会经过这个函数体(§10 第 12 条已记账,不是本函数自己的假
    /// 装)。
    async fn honest_close_on_storage_error(&mut self, run: RunId, context: impl std::fmt::Display) {
        let at = now_unix();
        let detail = format!("[run-manager] 存储调用出错,已尽力标成失败:{context}");
        match self
            .store
            .close_run(run, at, RunState::Failed, None, &detail)
            .await
        {
            Ok(true) => {
                let _ = self.store.settle_run(run, at).await;
                eprintln!(
                    "[run-manager] 诊断:运行 {run:?} 存储出错({context}),已补写关门(state=Failed)"
                );
            }
            Ok(false) => eprintln!(
                "[run-manager] 诊断:运行 {run:?} 存储出错({context}),补写时发现已经被别的动作关过门,诚实空转"
            ),
            Err(close_err) => eprintln!(
                "[run-manager] 诊断:运行 {run:?} 存储出错({context}),补写关门也失败,库里这一行会\
                 一直挂着 ended_at=NULL,直到下次重启 + reap_on_restart 收拾(如实记录,不假装已\
                 收尾):{close_err}"
            ),
        }
        if let Some(active_run) = self.active.remove(&run) {
            self.release_issue_slot(active_run.issue, run);
            self.release_workspace_slot(&active_run.workspace, run);
        }
    }

    /// 取消(design §3.5②/主控裁决 #6):比较并置关门,赢了才结算 + 通
    /// 知连接器。幂等——不在活跃表时,查库判定是「已经关过门」(`Ok`)还
    /// 是「库里还开着、但本进程从没见过」(评审 Important-2)。
    async fn handle_cancel(&mut self, run: RunId) -> Result<(), RunError> {
        let Some(active_run) = self.active.remove(&run) else {
            return match self.store.get_run(run).await? {
                Some(row) if row.ended_at.is_some() => Ok(()), // 已关门,真幂等
                // 评审 Important-2:此前这一支直接报 `Ok(())` 却什么都不
                // 做——被当成「理论边缘态」,实际上是重启之后、
                // `reap_on_restart()` 之前的常规状态(它不是自动触发
                // 的)。数据库那一行仍然 `ended_at IS NULL`,仍然占着交
                // 付名额,回一个「取消成功」是一次没做事的假 `Ok`。这里
                // 改成真去做一次 `close_run` 比较并置:本进程重启后没有
                // 这条运行的连接器句柄/票据(那是运行时对象,不落库),
                // 没法真的去杀上游进程,但至少把账如实关上——`end_detail`
                // 写清楚这一点,不冒充「已经杀掉上游」。
                Some(_) => {
                    let at = now_unix();
                    let detail = "已取消(本进程重启后未持有这条运行的连接器句柄,只能关账,\
                                   无法确认上游会话是否已真正停止——上游会话档案保留,人可自行续接核实)";
                    match self
                        .store
                        .close_run(
                            run,
                            at,
                            RunState::Canceled,
                            Some(RunEndKind::Canceled),
                            detail,
                        )
                        .await
                    {
                        Ok(true) => {
                            let _ = self.store.settle_run(run, at).await;
                            Ok(())
                        }
                        Ok(false) => Ok(()), // 别人抢先关门了,真幂等
                        Err(e) => Err(RunError::Store(e)),
                    }
                }
                None => Err(RunError::RunNotFound(run)),
            };
        };
        self.release_issue_slot(active_run.issue, run);
        self.release_workspace_slot(&active_run.workspace, run);
        // 让轮询任务尽快退出;同一个令牌也是 `handle_start` 派给开工外呼
        // 的 `CallCtx.cancel`——还没返回的 `start` 调用会被这一下打断。
        active_run.poll_cancel.cancel();

        let at = now_unix();
        let closed = self
            .store
            .close_run(
                run,
                at,
                RunState::Canceled,
                Some(RunEndKind::Canceled),
                CANCEL_END_DETAIL,
            )
            .await?;
        if closed {
            let _ = self.store.settle_run(run, at).await;
        }
        // 只有真的拿到过票据才有可取消的对象——还在 Starting 时没有
        // ticket,`handle_started` 那边收到晚到的结果会如实空转。
        if let Some(ticket) = active_run.ticket {
            let connector = active_run.connector;
            tokio::spawn(async move {
                let cx = CallCtx {
                    req: RequestId::new(),
                    timeout: None,
                    cancel: CancellationToken::new(),
                };
                if let Some(execute) = connector.as_execute() {
                    let _ = execute.cancel(&cx, &ticket).await;
                }
            });
        }
        Ok(())
    }

    /// 降级为咨询(design §3.5):kind 翻面 + 结账,内存里同步释放交付名
    /// 额;会话/工作区/活状态一个字都不碰。
    async fn handle_demote(&mut self, run: RunId) -> Result<bool, RunError> {
        let at = now_unix();
        let demoted = self.store.demote_run(run, at).await?;
        if demoted {
            let _ = self.store.settle_run(run, at).await;
            let issue = self.active.get_mut(&run).map(|a| {
                a.kind = RunKind::Consultation;
                a.issue
            });
            if let Some(issue) = issue {
                self.release_issue_slot(issue, run);
            }
        }
        Ok(demoted)
    }

    /// 重启后收拾遗留:一次 UPDATE 覆盖全部还开着的运行(§9「集合式
    /// UPDATE」),内存跟着清——**不重起任何会话**,不调用任何连接器方
    /// 法(「不批量唤醒」)。
    async fn handle_reap(&mut self) -> Result<ReapReport, RunError> {
        let at = now_unix();
        let reaped = self.store.reap_open_runs(at).await?;
        for id in &reaped {
            if let Some(active_run) = self.active.remove(id) {
                self.release_issue_slot(active_run.issue, *id);
                self.release_workspace_slot(&active_run.workspace, *id);
                active_run.poll_cancel.cancel();
            }
        }
        Ok(ReapReport { reaped })
    }

    fn handle_snapshot(&self, project: Option<ProjectId>) -> RunSnapshot {
        let active = self
            .active
            .values()
            .filter(|a| match project {
                Some(p) => a.project == p,
                None => true,
            })
            .count();
        RunSnapshot { project, active }
    }

    fn release_issue_slot(&mut self, issue: IssueId, run: RunId) {
        if self.by_issue.get(&issue) == Some(&run) {
            self.by_issue.remove(&issue);
        }
    }

    fn release_workspace_slot(&mut self, workspace: &PathBuf, run: RunId) {
        if self.by_workspace.get(workspace) == Some(&run) {
            self.by_workspace.remove(workspace);
        }
    }
}

/// 一次「观测到的终态」——[`map_exec_state`] 的输出,喂给
/// [`bw_store::RunStore::close_run`] 的三个参数打了个包。
struct Observed {
    state: RunState,
    end_kind: Option<RunEndKind>,
    detail: String,
}

/// 契约执行状态 → BW 运行状态的穷举映射(design §7)。**没有 `_` 通配分
/// 支**——`bw_connector::ExecState` 改形状时这里编译不过,逼着这份映射同
/// 步更新,分叉不可能。`None` = 还没到终态,轮询任务应该继续轮询。
fn map_exec_state(state: ExecState) -> Option<Observed> {
    match state {
        ExecState::Running => None,
        // 未知票据也当「继续轮询」处理——如果票据真的丢了,连接器会一直
        // 报 Unknown,不属于运行管理器这一层要诊断的事。
        ExecState::Unknown => None,
        ExecState::Finished { ended, summary } => {
            let end_kind = match ended {
                SessionEnd::ProcessExit { .. } => RunEndKind::ProcessExit,
                SessionEnd::StoppedByBw { .. } => RunEndKind::StoppedByBw,
                SessionEnd::ContactLost => RunEndKind::ContactLost,
            };
            Some(Observed {
                state: RunState::Finished,
                end_kind: Some(end_kind),
                detail: summary,
            })
        }
        // 连接器自己主动报告 Canceled(不是经运行管理器的 `cancel()` 触
        // 发)的边缘情形——穷举匹配要求这一支存在,按同样的「关门」机制
        // 处理,不当特例。
        ExecState::Canceled => Some(Observed {
            state: RunState::Canceled,
            end_kind: Some(RunEndKind::Canceled),
            detail: String::new(),
        }),
        ExecState::Orphaned => Some(Observed {
            state: RunState::Orphaned,
            end_kind: None,
            detail: String::new(),
        }),
    }
}

/// 解析执行连接器:给了名字就按名字找;没给按「恰好一条」规则,零条/多
/// 条都如实报,**绝不「取第一条」蒙混**(design §3.1,同 `issue_ops` 的
/// 先例)。
fn resolve_executor(
    registry: &ConnectorRegistry,
    project: ProjectId,
    name: Option<&str>,
) -> Result<(String, Arc<dyn Connector>), RunError> {
    let candidates = registry.executors(project);
    match name {
        Some(n) => candidates
            .into_iter()
            .find(|(e, _)| e.name == n)
            .map(|(e, c)| (e.name, c))
            .ok_or_else(|| RunError::UnknownExecutor {
                name: n.to_string(),
            }),
        None => {
            let n = candidates.len();
            match n {
                0 => Err(RunError::NoExecutor),
                1 => {
                    let (e, c) = candidates.into_iter().next().expect("len == 1");
                    Ok((e.name, c))
                }
                _ => Err(RunError::AmbiguousExecutor {
                    names: candidates.iter().map(|(e, _)| e.name.clone()).collect(),
                    n,
                }),
            }
        }
    }
}

/// 每个运行一个轮询任务(design §3.3——不是一个全局循环遍历所有运行,那
/// 会把 N 次连接器调用串起来,本身就是一个小 N 假设)。退出条件:观测到
/// 终态(发完 [`Cmd::Observed`] 就退出,不再继续轮询)/ 取消令牌被触发 /
/// 命令队列已关(`tx.send` 失败)。
fn spawn_poll_task(
    tx: mpsc::Sender<Cmd>,
    connector: Arc<dyn Connector>,
    run: RunId,
    ticket: ExecTicket,
    cancel: CancellationToken,
    interval: Duration,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return,
                _ = tokio::time::sleep(interval) => {}
            }
            let Some(execute) = connector.as_execute() else {
                return; // 装配期编码错误的防御式兜底,理论不该发生。
            };
            let cx = CallCtx {
                req: RequestId::new(),
                timeout: None,
                cancel: CancellationToken::new(),
            };
            match execute.poll(&cx, &ticket).await {
                Ok(ok) => {
                    let terminal = matches!(
                        ok.value,
                        ExecState::Finished { .. } | ExecState::Canceled | ExecState::Orphaned
                    );
                    if terminal {
                        let _ = tx
                            .send(Cmd::Observed {
                                run,
                                state: ok.value,
                            })
                            .await;
                        return; // 终态已发出,任务退出——不再继续轮询。
                    }
                    // Running/Unknown:继续下一轮。
                }
                Err(fail) => {
                    eprintln!(
                        "[run-manager] 诊断:运行 {run:?} 轮询失败(继续重试):{}",
                        fail.err
                    );
                }
            }
        }
    });
}
