//! `AgentCliConnector`(next 切片三D,design-s3-agentcli.md §2/§6/§7.4)——
//! `bw_connector::caps::{Connector, Probe, Execute}` 的真实实现,claude 一
//! 家。把 [`crate::interactive_cli`] 的计划构造器、[`crate::pty_backend`]
//! 的平台后端(经 [`crate::interactive_cli::InteractiveExecutor`])、
//! [`crate::terminal_manager::TerminalManager`] 的字节路由、`session` 模块
//! 的生命周期表接起来。
//!
//! **只起不等**(契约 `Execute::start` 的硬约束):`start` 校验完工作区/
//! 分支、决定好首启还是续接、起好 PTY 子进程之后立刻返回票据——真正跑会话
//! 的 `InteractiveExecutor::run_skill_pty` 调用挪进 `tokio::spawn` 出去的
//! 后台任务,不在 `start` 里 `.await` 它。`poll` 读会话表,`cancel` 靠丢
//! `TerminalManager` 持有的输入 sender 触发 `pty_backend` 已经验证过的
//! killpg 收尾语义(见下方 `cancel` 文档)——不重新发明一套杀进程逻辑。

use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bw_connector::caps::{Connector, Execute, Probe, ProbeReport};
use bw_connector::contract::{
    guarded, CallCtx, ConnError, ConnResult, ConnectorEntry, ConnectorKind, ExecSpec, ExecState,
    ExecTicket, InjectBlock, OpClass, ProjectBinding, RequestId, SessionEnd,
};
use bw_core::{ConversationId, IssueId, WorkflowId};
use sha2::{Digest, Sha256};

use super::session::{discover_sessions, InjectRecord, SessionRow, SessionTable};
use crate::interactive_cli::{
    build_resume_plan, build_startup_plan, InteractiveExecutor, TuiAgentConfig,
};
use crate::terminal_manager::{ConversationMeta, TerminalManager};
use crate::{ExecError, RunCtx};

/// `ExecSpec` 目前没有独立的「这件活要干什么」字段(v1 `position_prompt`
/// 承担的角色,任务正文/需求描述)——契约冻结在切片二,`inject` 按本片口径
/// (design §6.1/brief)整体进系统提示词(`--append-system-prompt`),不拆一
/// 块出来当位置 prompt。首启仍然需要一个位置 prompt 才能让 claude 的 TUI
/// 收到第一条用户消息、真正开始动手——完全空字符串会让 auto-submit 提交
/// 一个空消息,agent 大概率只是空等在那里。这里用一条固定的通用开局句,不
/// 编造任务内容;如实标注这是本片继承的契约缺口(该不该给 `ExecSpec` 加任
/// 务字段是切片四编排层要接的事,agentcli 层不能单方面改契约形状)——见任
/// 务报告 concerns。
const GENERIC_KICKOFF_PROMPT: &str = "请阅读上面的系统提示词并开始执行。";

/// claude 交互式接不上按次预算封顶(`--max-budget-usd` 只在 `--print` 模式
/// 有效,design §8/主控裁决 #3)。收到预算值时如实无视 + 诊断行,绝不假装
/// 设了上限。
fn note_budget_ignored(budget_usd: Option<f64>) {
    if let Some(usd) = budget_usd {
        eprintln!(
            "[agentcli] 诊断: budget_usd={usd} 在交互式 claude 上不生效(--max-budget-usd \
             仅 --print 模式有效),已忽略——会话行 budget_enforced=false"
        );
    }
}

/// 把 `ExecSpec.inject` 按序拼进一段系统提示词,同时产出记账用的
/// [`InjectRecord`] 清单(design §6.2:标签/字节数/sha256 前 8 位)。**顺序
/// 等于调用方给的顺序,一个字不改、不截断、不重排**。空输入 → 空字符串 +
/// 空清单——如实,不假装注入了(§6.2「空就是空」)。
///
/// `pub`:切片三E 的 `agent_session` 指挥器要独立调这个函数,把落盘的系统
/// 提示词与 `SessionRow.injected` 交叉核对(§7.2 第 6 条断言)。
pub fn assemble_system_prompt(blocks: &[InjectBlock]) -> (String, Vec<InjectRecord>) {
    let mut prompt = String::new();
    let mut records = Vec::with_capacity(blocks.len());
    for block in blocks {
        if !prompt.is_empty() {
            prompt.push_str("\n\n");
        }
        prompt.push_str("## ");
        prompt.push_str(&block.label);
        prompt.push_str("\n\n");
        prompt.push_str(&block.body);
        records.push(InjectRecord {
            label: block.label.clone(),
            bytes: block.body.len(),
            digest8: digest8(&block.body),
        });
    }
    (prompt, records)
}

/// sha256 前 8 位十六进制字符(摘要头 4 字节各自格式化两位十六进制)——够
/// 对上「是不是同一块」,不引入额外的 hex crate。
fn digest8(s: &str) -> String {
    let hash = Sha256::digest(s.as_bytes());
    hash.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

/// 读工作区当前分支名。**新写代码,兑现取消义务**(`.kill_on_drop(true)`,
/// design §4/`guarded` 文档对新写适配器的要求)。
async fn current_branch(workspace: &Path) -> Result<String, ConnError> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("-C")
        .arg(workspace)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    let output = cmd
        .output()
        .await
        .map_err(|e| ConnError::NotConnected(format!("读取当前分支失败(git 未安装?):{e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ConnError::NotConnected(format!(
            "工作区 {} 不是有效 git 检出:{}",
            workspace.display(),
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// claude 一家的执行连接器。`row` 是注册表里的那一行(§5),`exec` 是真实
/// PTY 执行器或自我标注的 mock——可插拔,指挥器用它换入
/// `MockInteractiveExecutor` 做确定性断言,真跑用
/// `InteractiveCliExecutor::new()`。
///
/// **两张表,键不同,不绑一起**(design §1.2):`terminals` 按
/// `ConversationId` 路由字节(界面面向,移植件原样用);`sessions` 按
/// `RequestId` 管生命周期(编排层面向,本片新写)。**并发纪律**:两把锁都
/// 是同步 `Mutex`,任何一处都不许跨 `.await` 持有——拿到要的东西立刻放,
/// 真会话跑在 `tokio::spawn` 出去的任务里,不在锁里等。
pub struct AgentCliConnector {
    kind: ConnectorKind,
    binding: ProjectBinding,
    row: &'static TuiAgentConfig,
    exec: Arc<dyn InteractiveExecutor>,
    terminals: Arc<Mutex<TerminalManager>>,
    sessions: Arc<Mutex<SessionTable>>,
}

impl AgentCliConnector {
    pub fn new(
        binding: ProjectBinding,
        row: &'static TuiAgentConfig,
        exec: Arc<dyn InteractiveExecutor>,
    ) -> Self {
        Self {
            kind: ConnectorKind::AgentCli {
                cli: row.slug.to_string(),
            },
            binding,
            row,
            exec,
            terminals: Arc::new(Mutex::new(TerminalManager::new())),
            sessions: Arc::new(Mutex::new(SessionTable::new())),
        }
    }

    /// 登记工厂用的构造入口(同 gh/script 两家的 `from_entry` 口径)。
    /// **不经共享的 `bw_connector::adapters::from_entry`**——那个函数只分派
    /// 仓连接器,`AgentCli` 各自有自己的构造路径(需要 `row`/`exec` 两个额
    /// 外参数,不是单靠 `ConnectorEntry` 能决定的),composition root 直接
    /// 调这里。
    pub fn from_entry(
        entry: &ConnectorEntry,
        row: &'static TuiAgentConfig,
        exec: Arc<dyn InteractiveExecutor>,
    ) -> Arc<dyn Connector> {
        let expected_kind = ConnectorKind::AgentCli {
            cli: row.slug.to_string(),
        };
        assert_eq!(
            entry.kind, expected_kind,
            "AgentCliConnector::from_entry 收到的登记 kind={:?} 与传入的注册表行 \
             slug={:?} 不符——composition root 的装配期编码错误",
            entry.kind, row.slug
        );
        Arc::new(Self::new(entry.binding.clone(), row, exec))
    }

    /// 供指挥器/未来 debug 读回用的会话行快照(克隆)——不是 `Execute` 契约
    /// 的一部分。指挥器用它核对 `injected`/`upstream_session`/
    /// `budget_enforced` 这些不会经 `ExecTicket`/`ExecState` 回传的内部
    /// 事实(§7.2 第 6 条断言、装配读回)。
    pub fn session_row(&self, req: RequestId) -> Option<SessionRow> {
        self.sessions
            .lock()
            .expect("session table mutex poisoned")
            .get(req)
            .cloned()
    }
}

impl Connector for AgentCliConnector {
    fn kind(&self) -> &ConnectorKind {
        &self.kind
    }

    fn binding(&self) -> &ProjectBinding {
        &self.binding
    }

    fn as_probe(&self) -> Option<&dyn Probe> {
        Some(self)
    }

    fn as_execute(&self) -> Option<&dyn Execute> {
        Some(self)
    }
}

#[async_trait]
impl Probe for AgentCliConnector {
    /// 「已安装」档(主控裁决 #10 口径):真跑一次 `detect_cmd`(claude:
    /// `claude --version`),成功只证明「装了」,不证明「能真跑一次会话」
    /// ——一行人话形如 `claude 2.1.217 已安装 · 尚未验证能否真跑`。
    async fn probe(&self, cx: &CallCtx) -> ConnResult<ProbeReport> {
        let row = self.row;
        guarded(cx, OpClass::Probe, async move {
            let mut parts = row.detect_cmd.split_whitespace();
            let bin = parts
                .next()
                .ok_or_else(|| ConnError::Other(format!("{} 的 detect_cmd 是空串", row.slug)))?;
            let args: Vec<&str> = parts.collect();
            let mut cmd = tokio::process::Command::new(bin);
            cmd.args(&args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .kill_on_drop(true);
            let output = cmd
                .output()
                .await
                .map_err(|e| ConnError::NotConnected(format!("{bin} 未安装或不在 PATH:{e}")))?;
            if !output.status.success() {
                return Err(ConnError::NotConnected(format!(
                    "{bin} 探活失败(退出码 {:?})",
                    output.status.code()
                )));
            }
            let raw = String::from_utf8_lossy(&output.stdout);
            let version = raw.trim();
            Ok(ProbeReport {
                identity: row.slug.to_string(),
                detail: format!("{} {version} 已安装 · 尚未验证能否真跑", row.slug),
            })
        })
        .await
    }
}

#[async_trait]
impl Execute for AgentCliConnector {
    /// design §2.2 的四步:①校验工作区是 git 检出 ②校验当前分支等于
    /// `spec.branch` ③查会话表(内存)→ 查不到再查上游目录反查,决定首启
    /// 还是续接 ④起 PTY,立刻返回票据,不等会话结束。
    async fn start(&self, cx: &CallCtx, spec: ExecSpec) -> ConnResult<ExecTicket> {
        let req = cx.req;
        let row = self.row;
        let exec = self.exec.clone();
        let terminals = self.terminals.clone();
        let sessions = self.sessions.clone();
        let project = self.binding.project;

        guarded(cx, OpClass::Write, async move {
            let ExecSpec {
                workspace,
                branch: required_branch,
                inject,
                budget_usd,
            } = spec;

            // ① 工作区必须已存在、且是 git 检出——适配器不给用户造工作区,
            // 谁给的工作区谁负责(design §2.2)。
            if !workspace.join(".git").exists() {
                return Err(ConnError::NotConnected(format!(
                    "工作区 {} 不存在或不是 git 检出(无 .git)",
                    workspace.display()
                )));
            }

            // ② 当前分支必须等于 spec.branch——不等就如实拒绝,绝不在错分
            // 支上放 agent 干活。
            let current = current_branch(&workspace).await?;
            if current != required_branch {
                return Err(ConnError::UpstreamRejected {
                    message: format!(
                        "当前分支 {current} 与要求的 {required_branch} 不符,拒绝在错分支上执行"
                    ),
                });
            }

            // ③ 查会话表(内存)→ 查不到再查上游目录反查。
            let found_in_table = {
                let table = sessions.lock().expect("session table mutex poisoned");
                table
                    .find_by_workspace(&workspace)
                    .map(|row| row.upstream_session.clone())
            };
            let (upstream_session, is_resume) =
                if let Some(id) = found_in_table.filter(|s| !s.is_empty()) {
                    (id, true)
                } else if let Some((id, _)) = discover_sessions(&workspace)
                    .into_iter()
                    .max_by_key(|(_, mtime)| *mtime)
                {
                    (id, true)
                } else {
                    (uuid::Uuid::new_v4().to_string(), false)
                };

            let (plan, injected) = if is_resume {
                // §6.3:续接不重注入——复利块首启时已经在会话里,重注是污
                // 染,不是漏注。
                let plan = build_resume_plan(row, Some(&upstream_session), &workspace)
                    .map_err(exec_error_to_conn_error)?;
                (plan, Vec::new())
            } else {
                let (system_prompt, injected) = assemble_system_prompt(&inject);
                let plan =
                    build_startup_plan(row, GENERIC_KICKOFF_PROMPT, &system_prompt, &workspace)
                        .map_err(exec_error_to_conn_error)?;
                (plan, injected)
            };

            // design §8/主控裁决 #3:如实无视 + 诊断行,绝不假装设了上限。
            note_budget_ignored(budget_usd);
            let budget_enforced = false;

            // ④ 起 PTY,立刻返回票据,不等会话结束。
            let conversation_id = ConversationId::new();
            let meta = ConversationMeta {
                conversation_id,
                // agentcli 层没有「活」的概念(§8:不做存储、不做编排层接
                // 线)——用 nil 占位,如实标注不是真实 Issue 绑定。
                issue_id: IssueId::nil(),
                claude_session_id: upstream_session.clone(),
                workspace_path: workspace.clone(),
                branch_name: required_branch.clone(),
            };
            let initial_size = {
                let t = terminals.lock().expect("terminal manager mutex poisoned");
                t.last_fit_size()
            };
            let (bytes_tx, input_rx) = {
                let mut t = terminals.lock().expect("terminal manager mutex poisoned");
                t.attach(conversation_id, meta, initial_size)
            };

            {
                let mut t = sessions.lock().expect("session table mutex poisoned");
                t.insert(SessionRow {
                    req,
                    conversation: conversation_id,
                    workspace: workspace.clone(),
                    branch: required_branch.clone(),
                    upstream_session: upstream_session.clone(),
                    injected,
                    budget_enforced,
                    state: ExecState::Running,
                });
            }

            // 后台任务:真跑会话,不等(「只起不等」的义务)。会话结束后把
            // 结果写回会话表——`poll` 读到的就是这里写的事实。
            let sessions_bg = sessions.clone();
            let run_ctx = RunCtx {
                project,
                // agentcli 层没有 workflow 身份(§8 范围裁剪)——两个
                // `InteractiveExecutor` 实现(真实/mock)的 `run_skill_pty`
                // 均不读这个字段,nil 占位无副作用。
                workflow: WorkflowId::nil(),
            };
            tokio::spawn(async move {
                let outcome = exec
                    .run_skill_pty(&plan, &run_ctx, bytes_tx, input_rx)
                    .await;
                let mut t = sessions_bg.lock().expect("session table mutex poisoned");
                if let Some(row) = t.get_mut(req) {
                    // 竞态守卫:`cancel` 可能已经把这一行标成 Canceled——
                    // 那一档一旦落定就不许被这里的「正常结束」覆盖回去
                    // (人点的取消优先于事后才跑完的收尾)。
                    if row.state != ExecState::Canceled {
                        row.state = match outcome {
                            Ok(output) => ExecState::Finished {
                                ended: SessionEnd::ProcessExit { code: None },
                                summary: output.summary,
                            },
                            Err(e) => ExecState::Finished {
                                ended: SessionEnd::ContactLost,
                                summary: e.to_string(),
                            },
                        };
                    }
                }
            });

            Ok(ExecTicket {
                req,
                upstream_session: Some(upstream_session),
            })
        })
        .await
    }

    /// 轮询当前状态,不建订阅/游标/重放——直接读会话表这一行的事实。未知
    /// 票据(不存在/进程重启后表已清空)如实 `Unknown`,不是错误。
    async fn poll(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<ExecState> {
        let req = t.req;
        let sessions = self.sessions.clone();
        guarded(cx, OpClass::Read, async move {
            let table = sessions.lock().expect("session table mutex poisoned");
            Ok(table
                .get(req)
                .map(|row| row.state.clone())
                .unwrap_or(ExecState::Unknown))
        })
        .await
    }

    /// 取消。幂等:未知/已结束/已取消的票据再取消 = `Ok`。真正在跑的会
    /// 话——**不重新发明杀进程逻辑**,靠丢 `TerminalManager` 里为这条会话
    /// 持有的输入 sender(`TerminalManager::close`):
    /// `pty_backend`(unix/windows 两份后端)的收尾循环里 `input_rx.recv()`
    /// 返回 `None` 就会 `break` 出 `select!`,走进已经在切片三-1 验证过的
    /// killpg 真杀收尾(见 `pty_backend.rs` unix 模块的孙进程杀灭验证记
    /// 录)——cancel 这里只负责「让那条路径被触发」,真杀的机制在更下层,
    /// 这是新代码必须兑现的取消义务(`guarded` 文档)在这一层的落地形态。
    async fn cancel(&self, cx: &CallCtx, t: &ExecTicket) -> ConnResult<()> {
        let req = t.req;
        let sessions = self.sessions.clone();
        let terminals = self.terminals.clone();
        guarded(cx, OpClass::Write, async move {
            let snapshot = {
                let table = sessions.lock().expect("session table mutex poisoned");
                table
                    .get(req)
                    .map(|row| (row.conversation, row.state.clone()))
            };
            let Some((conversation, state)) = snapshot else {
                return Ok(()); // 未知票据——幂等,当「已经不在了」处理。
            };
            if matches!(state, ExecState::Finished { .. } | ExecState::Canceled) {
                return Ok(()); // 已结束/已取消——幂等。
            }

            {
                let mut term = terminals.lock().expect("terminal manager mutex poisoned");
                term.close(conversation);
            }
            {
                let mut table = sessions.lock().expect("session table mutex poisoned");
                if let Some(row) = table.get_mut(req) {
                    row.state = ExecState::Canceled;
                }
            }
            Ok(())
        })
        .await
    }
}

fn exec_error_to_conn_error(e: ExecError) -> ConnError {
    ConnError::Other(e.to_string())
}
